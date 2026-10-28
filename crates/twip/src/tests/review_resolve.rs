//! Accepting and rejecting revisions. Accepting or rejecting everything in each review fixture gives the document of
//! its `-ACCEPTED` or `-REJECTED` twin (text, run formatting, paragraph properties, table structure); and each
//! recorded resolve in `tests/scenarios/review/expected/resolve-*.json` gives the recorded result: the paragraphs,
//! their runs' and marks' revisions, the table structure, and the revisions left.

use std::path::Path;

use serde_json::{Value, json};

use crate::docx::review::MarkKind;
use crate::docx::{Block, BreakKind, Document, Inline, ParaStyle, Paragraph, RunStyle, Table, VMerge};
use crate::editing::Editor;
use crate::editing::commands::{Command, Outcome};
use crate::editing::integrity;
use crate::editing::position::{Position, StoryId};
use crate::editing::revisions::index::revisions;
use crate::editing::revisions::resolve::Scope;
use crate::editing::tests::fonts;
use crate::layout::{ParaId, Unit};

fn open(name: &str) -> Editor {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/review").join(name);
    let bytes = std::fs::read(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
    Editor::open(&bytes, fonts()).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn resolve(e: &mut Editor, accept: bool, scope: Scope) -> Outcome {
    let out = e.execute(Command::ResolveRevisions { accept, scope }, fonts());
    let v = integrity::check(e.document(), e.index(), &e.selection());
    assert!(v.is_empty(), "integrity after resolving: {v:?}");
    out
}

// ---------------- the twins ----------------

/// A paragraph's text and runs (adjacent runs of equal formatting merged, revisions left out), and its properties.
#[derive(Debug, PartialEq)]
pub(super) enum Shape {
    Para { text: String, runs: Vec<(String, RunStyle)>, style: Box<ParaStyle> },
    Table(Vec<Vec<(usize, VMerge, Vec<Shape>)>>),
}

fn text_of(i: &Inline) -> String {
    match i {
        Inline::Text(t, _) => t.clone(),
        Inline::Tab(_) | Inline::PTab(..) => "\t".into(),
        Inline::Break(BreakKind::Line, _) => "\n".into(),
        Inline::Break(..) => "\u{c}".into(),
        Inline::Sym { ch, .. } => ch.to_string(),
        Inline::Picture(..) | Inline::Float(..) | Inline::NoteRef { .. } => "\u{FFFC}".into(),
        _ => String::new(),
    }
}

fn shapes(blocks: &[Block]) -> Vec<Shape> {
    blocks
        .iter()
        .map(|b| match b {
            Block::Para(p) => {
                let mut runs: Vec<(String, RunStyle)> = Vec::new();
                for i in &p.inlines {
                    let (t, Some(r)) = (text_of(i), i.run()) else { continue };
                    if t.is_empty() {
                        continue;
                    }
                    let mut s = (**r).clone();
                    s.rev = Default::default();
                    match runs.last_mut() {
                        Some((prev, ps)) if *ps == s => prev.push_str(&t),
                        _ => runs.push((t, s)),
                    }
                }
                let mut style = Box::new((*p.style).clone());
                style.change = None;
                Shape::Para { text: runs.iter().map(|r| r.0.as_str()).collect(), runs, style }
            }
            Block::Table(t) => {
                Shape::Table(t.rows.iter().map(|r| r.cells.iter().map(|c| (c.span.max(1), c.v_merge, shapes(&c.blocks))).collect()).collect())
            }
        })
        .collect()
}

pub(super) fn body(doc: &Document) -> Vec<Shape> {
    shapes(&doc.sections.iter().flat_map(|s| s.blocks.iter().cloned()).collect::<Vec<_>>())
}

#[test]
fn accepting_or_rejecting_all_gives_the_twins() {
    for name in ["RUNS", "MARKS", "TABLES", "SECTIONS"] {
        for (accept, twin) in [(true, "ACCEPTED"), (false, "REJECTED")] {
            let mut e = open(&format!("FX-REVIEW-{name}.docx"));
            let before = body(e.document());
            let blocks = e.document().sections[0].blocks.clone();
            let pages: Vec<_> = e.document().sections.iter().map(|s| (s.page(), s.change.clone())).collect();
            let out = resolve(&mut e, accept, Scope::All);
            assert!(matches!(out, Outcome::Changed(_)), "{name} {twin}: {out:?}");
            let twin_doc = open(&format!("FX-REVIEW-{name}-{twin}.docx"));
            let want = body(twin_doc.document());
            let got = body(e.document());
            assert_eq!(got.len(), want.len(), "{name} {twin}: blocks");
            for (k, (g, w)) in got.iter().zip(&want).enumerate() {
                assert_eq!(g, w, "{name} {twin}: block {k}");
            }
            let page = |d: &Document| d.sections.iter().map(|s| s.page()).collect::<Vec<_>>();
            assert_eq!(page(e.document()), page(twin_doc.document()), "{name} {twin}: section pages");
            let left: Vec<_> = revisions(e.document(), e.index()).into_iter().filter(|r| r.story == StoryId::Body).map(|r| r.kind.name()).collect();
            assert!(left.is_empty(), "{name} {twin}: left {left:?}");
            // One undo step puts everything back.
            e.execute(Command::Undo, fonts());
            assert_eq!(body(e.document()), before, "{name} {twin}: undo");
            assert!(e.document().sections[0].blocks == blocks, "{name} {twin}: undo restores the blocks exactly");
            let now: Vec<_> = e.document().sections.iter().map(|s| (s.page(), s.change.clone())).collect();
            assert!(now == pages, "{name} {twin}: undo restores the sections");
        }
    }
}

#[test]
fn next_and_previous_go_through_every_story() {
    let mut e = open("FX-REVIEW-STORIES.docx");
    let list = revisions(e.document(), e.index());
    assert!(list.iter().any(|r| r.story != StoryId::Body), "the fixture has revisions outside the body");
    let mut seen = Vec::new();
    while e.execute(Command::NextRevision { back: false }, fonts()) == Outcome::Moved {
        let (a, b) = e.selection().ordered(e.index()).expect("a text selection");
        seen.push((a, b));
        assert!(seen.len() <= list.len(), "next goes past the last revision");
    }
    let want: Vec<(Position, Position)> = list.iter().map(|r| (r.start, r.end)).collect();
    let same = |x: &[(Position, Position)], y: &[(Position, Position)]| {
        x.len() == y.len() && x.iter().zip(y).all(|(p, q)| p.0.same_place(&q.0) && p.1.same_place(&q.1))
    };
    assert!(same(&seen, &want), "next visits every revision in order: {seen:?}");
    let mut back = Vec::new();
    while e.execute(Command::NextRevision { back: true }, fonts()) == Outcome::Moved {
        back.push(e.selection().ordered(e.index()).expect("a text selection"));
        assert!(back.len() <= list.len(), "previous goes past the first revision");
    }
    back.reverse();
    assert!(same(&back, &want[..want.len().saturating_sub(1)]), "previous visits them back: {back:?}");
}

// ---------------- the recorded results ----------------

fn author(doc: &Document, a: crate::docx::review::AuthorId) -> Value {
    doc.people.get(a).map_or(Value::Null, |p| json!(&*p.name))
}

/// A run's (or mark's) revision flags in the recorded form: insertion and deletion authors, and the move it is in.
fn flags(doc: &Document, r: &RunStyle) -> (Value, Value, Value) {
    let (mut ins, mut del, mut mv) = (Value::Null, Value::Null, Value::Null);
    if let Some(m) = &r.rev.ins {
        match m.kind {
            MarkKind::MoveTo => mv = json!("to"),
            _ => ins = author(doc, m.author),
        }
    }
    if let Some(m) = &r.rev.del {
        match m.kind {
            MarkKind::MoveFrom => mv = json!("from"),
            _ => del = author(doc, m.author),
        }
    }
    (ins, del, mv)
}

/// Runs merged where their flags agree, empty ones dropped.
fn merged(runs: Vec<Value>) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::new();
    for r in runs {
        if r["text"].as_str().is_none_or(str::is_empty) {
            continue;
        }
        match out.last_mut() {
            Some(prev) if ["ins", "del", "move", "fmt"].iter().all(|k| prev[k] == r[k]) => {
                let t = format!("{}{}", prev["text"].as_str().unwrap_or(""), r["text"].as_str().unwrap_or(""));
                prev["text"] = json!(t);
            }
            _ => out.push(r),
        }
    }
    out
}

fn ours_para(doc: &Document, p: &Paragraph) -> Value {
    let mut runs = Vec::new();
    let mut in_code = Vec::new();
    for i in &p.inlines {
        match i {
            Inline::FieldBegin { id, .. } => in_code.push(*id),
            Inline::FieldSep { id, .. } | Inline::FieldEnd { id, .. } => in_code.retain(|x| x != id),
            _ => {}
        }
        let Some(r) = i.run() else { continue };
        if !in_code.is_empty() {
            continue;
        }
        let (ins, del, mv) = flags(doc, r);
        let fmt = r.rev.fmt.as_ref().map_or(Value::Null, |f| author(doc, f.mark.author));
        runs.push(json!({ "text": text_of(i), "ins": ins, "del": del, "move": mv, "fmt": fmt }));
    }
    let runs = merged(runs);
    let (ins, del, mv) = flags(doc, &p.mark);
    let pchange = p.style.change.as_ref().filter(|c| c.previous_raw.0.is_some()).map_or(Value::Null, |c| author(doc, c.mark.author));
    let text: String = runs.iter().filter_map(|r| r["text"].as_str()).collect();
    json!({ "text": text, "runs": runs, "mark": { "ins": ins, "del": del, "move": mv }, "pchange": pchange })
}

fn ours_table(doc: &Document, t: &Table) -> Value {
    let rows: Vec<Value> = t
        .rows
        .iter()
        .map(|r| {
            let cells: Vec<Value> = r
                .cells
                .iter()
                .map(|c| {
                    let (mut ins, mut del, mut merge) = (Value::Null, Value::Null, Value::Null);
                    if let Some((m, k)) = &c.rev.structural {
                        match k {
                            crate::docx::review::CellMarkKind::Insert => ins = author(doc, m.author),
                            crate::docx::review::CellMarkKind::Delete => del = author(doc, m.author),
                            crate::docx::review::CellMarkKind::Merge { .. } => merge = author(doc, m.author),
                        }
                    }
                    let vmerge = match c.v_merge {
                        VMerge::Restart => json!("restart"),
                        VMerge::Continue => json!("continue"),
                        VMerge::None => Value::Null,
                    };
                    json!({ "ins": ins, "del": del, "merge": merge, "vmerge": vmerge, "span": c.span.max(1), "blocks": ours_blocks(doc, &c.blocks) })
                })
                .collect();
            let ins = r.rev.ins.as_ref().map_or(Value::Null, |m| author(doc, m.author));
            let del = r.rev.del.as_ref().map_or(Value::Null, |m| author(doc, m.author));
            json!({ "ins": ins, "del": del, "cells": cells })
        })
        .collect();
    json!({ "table": { "rows": rows } })
}

fn ours_blocks(doc: &Document, blocks: &[Block]) -> Vec<Value> {
    blocks
        .iter()
        .map(|b| match b {
            Block::Para(p) => ours_para(doc, p),
            Block::Table(t) => ours_table(doc, t),
        })
        .collect()
}

/// The recorded form cut down to what [`ours_blocks`] gives.
fn theirs_blocks(blocks: &[Value]) -> Vec<Value> {
    blocks
        .iter()
        .map(|b| match b.get("table") {
            Some(t) => {
                let rows: Vec<Value> = t["rows"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|r| {
                        let cells: Vec<Value> = r["cells"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .map(|c| {
                                let merge = c["merge"].get("author").cloned().unwrap_or(Value::Null);
                                let blocks = theirs_blocks(c["blocks"].as_array().map_or(&[][..], |v| v));
                                json!({ "ins": c["ins"], "del": c["del"], "merge": merge, "vmerge": c["vmerge"], "span": c["span"], "blocks": blocks })
                            })
                            .collect();
                        json!({ "ins": r["ins"], "del": r["del"], "cells": cells })
                    })
                    .collect();
                json!({ "table": { "rows": rows } })
            }
            None => {
                let runs: Vec<Value> = b["runs"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|r| json!({ "text": r["text"], "ins": r["ins"], "del": r["del"], "move": r["move"], "fmt": r["fmt"] }))
                    .collect();
                let runs = merged(runs);
                let m = &b["mark"];
                json!({ "text": b["text"], "runs": runs, "mark": { "ins": m["ins"], "del": m["del"], "move": m["move"] }, "pchange": b["pchange"] })
            }
        })
        .collect()
}

/// The body position at a recorded offset: units, plus one for each paragraph mark (paragraphs outside tables only).
fn at_offset(doc: &Document, offset: u64) -> Option<Position> {
    let mut left = offset;
    for b in doc.sections.iter().flat_map(|s| s.blocks.iter()) {
        let Block::Para(p) = b else { return None };
        let len = crate::editing::step::para_len(p).0 as u64;
        if left <= len {
            return Some(Position::text(StoryId::Body, ParaId(p.id), Unit(left as u32)));
        }
        left -= len + 1;
    }
    None
}

#[test]
fn resolving_gives_the_recorded_results() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/scenarios/review/expected");
    let mut files: Vec<_> = std::fs::read_dir(&dir).expect("the recorded results").flatten().map(|e| e.path()).collect();
    files.retain(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("resolve-") && n.ends_with(".json")));
    files.sort();
    let mut checked = 0;
    for path in files {
        let name = path.file_stem().and_then(|n| n.to_str()).unwrap_or("").to_string();
        // Not compared: the recorded moves resolved in the reference application's own session (`before` is a
        // document that isn't a fixture), and the reference application's table rebuilds when rejecting table
        // properties (a table split by row, or its grid rebuilt) and inserted and deleted cells together (a grid
        // rebuilt).
        let skip = name.starts_with("resolve-recorded-")
            || [
                "resolve-tables-2-table-property-reject",
                "resolve-tables-4-table-property-reject",
                "resolve-tables-reject-all",
                "resolve-cells-4-table-property-reject",
                "resolve-cells-reject-all",
            ]
            .contains(&name.as_str());
        if skip {
            continue;
        }
        let case: Value = serde_json::from_str(&std::fs::read_to_string(&path).expect("readable")).expect("JSON");
        let mut e = open(case["fixture"].as_str().unwrap_or(""));
        let accept = case["op"] == "accept";
        let scope = match case["scope"].as_str().unwrap_or("") {
            "all" => Scope::All,
            "author" => Scope::Author(case["author"].as_str().unwrap_or("").to_string()),
            "one" => {
                let k = case["revision"]["index"].as_u64().unwrap_or(0) as usize;
                let list: Vec<_> = revisions(e.document(), e.index()).into_iter().filter(|r| r.story == StoryId::Body).collect();
                let r = list.get(k.wrapping_sub(1)).unwrap_or_else(|| panic!("{name}: no revision {k} of {}", list.len()));
                let want = case["revision"]["type"].as_str().unwrap_or("").trim_start_matches("wdRevision").to_string();
                assert_eq!(
                    (r.kind.name(), author(e.document(), r.author)),
                    (want.as_str(), case["revision"]["author"].clone()),
                    "{name}: revision {k}"
                );
                Scope::Revision(r.ids[0])
            }
            "range" => {
                // The caret at the start, then the selection extended a character (or mark) at a time.
                let (start, end) = (case["range"]["start"].as_u64().unwrap_or(0), case["range"]["end"].as_u64().unwrap_or(0));
                let (para, u) = at_offset(e.document(), start).and_then(|p| p.para_offset()).unwrap_or_else(|| panic!("{name}: range start"));
                e.execute(Command::SelectRange { para, a: u, b: u }, fonts());
                for _ in start..end {
                    let by = crate::editing::commands::MoveBy::Char;
                    e.execute(Command::Move { by, dir: crate::editing::commands::Dir::Forward, extend: true }, fonts());
                }
                let want = at_offset(e.document(), end).unwrap_or_else(|| panic!("{name}: range end"));
                assert!(e.selection().focus().is_some_and(|f| f.same_place(&want)), "{name}: selection {:?}", e.selection());
                Scope::Selection
            }
            other => panic!("{name}: scope {other}"),
        };
        let out = resolve(&mut e, accept, scope);
        if !case["error"].is_null() {
            assert_eq!(out, Outcome::Unchanged("unavailable"), "{name}: refused");
        }
        let doc = e.document();
        let got = ours_blocks(doc, &doc.sections.iter().flat_map(|s| s.blocks.iter().cloned()).collect::<Vec<_>>());
        let want = theirs_blocks(case["result"]["stories"]["body"].as_array().map_or(&[][..], |v| v));
        assert_eq!(got.len(), want.len(), "{name}: blocks\n{got:#?}");
        for (k, (g, w)) in got.iter().zip(&want).enumerate() {
            assert_eq!(g, w, "{name}: block {k}");
        }
        // The revisions left, kind and author, as the reference application lists them in the body.
        let left: Vec<(String, Value)> = revisions(doc, e.index())
            .into_iter()
            .filter(|r| r.story == StoryId::Body && r.kind.name() != "SectionProperty")
            .map(|r| (format!("wdRevision{}", r.kind.name()), author(doc, r.author)))
            .collect();
        let want_left: Vec<(String, Value)> = case["result"]["revisions"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|r| r.get("story").is_none_or(Value::is_null) && r["type"] != "wdRevisionSectionProperty")
            .map(|r| (r["type"].as_str().unwrap_or("").to_string(), r["author"].clone()))
            .collect();
        // Rejecting the cell merge also drops the reference application's table property change of that row.
        if name != "resolve-cells-3-cell-merge-reject" {
            assert_eq!(left, want_left, "{name}: revisions left");
        }
        checked += 1;
    }
    assert!(checked >= 40, "only {checked} recorded results compared");
}
