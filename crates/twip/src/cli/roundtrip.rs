//! `twip roundtrip [--edits all|merge] [--keep <dir>] <docx or folders...>`.
//! Without edits: reading and saving an unedited document gives its bytes back. With `--edits all`: each edit family
//! runs through the editor on a fresh copy, then the document is saved, read back and its dump compared with the
//! edited one. `--edits merge`: field, control, region and data part edits, sent as a host sends
//! them, compared the same way and also by their elements and data parts. `--keep` writes every saved file there,
//! to open them in other applications.

use std::path::{Path, PathBuf};

use twip::docx::props::Jc;
use twip::editing::commands::{Command, Dir, MoveBy};
use twip::editing::format::Toggle;
use twip::editing::position::{Position, StoryId};
use twip::editing::selection::{ObjectRef, Selection};
use twip::editing::{Editor, inspect};
use twip::font::Fonts;
use twip::layout::Unit;

fn files(paths: &[String]) -> Vec<PathBuf> {
    fn walk(p: &Path, out: &mut Vec<PathBuf>) {
        if p.is_dir() {
            let mut v: Vec<PathBuf> = std::fs::read_dir(p).into_iter().flatten().flatten().map(|e| e.path()).collect();
            v.sort();
            v.iter().for_each(|q| walk(q, out));
        } else if p.extension().is_some_and(|x| x == "docx") && !p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("~$")) {
            out.push(p.to_path_buf());
        }
    }
    let mut out = Vec::new();
    paths.iter().for_each(|p| walk(Path::new(p), &mut out));
    out
}

/// The edit families: a name and the commands that make it, run from the document start.
fn families() -> Vec<(&'static str, Vec<Command>)> {
    vec![
        ("typing", vec![Command::InsertText("Typed text. ".into())]),
        (
            "enter",
            vec![Command::Move { by: MoveBy::Word, dir: Dir::Forward, extend: false }, Command::Enter, Command::InsertText("New paragraph".into())],
        ),
        ("delete", vec![Command::Move { by: MoveBy::Word, dir: Dir::Forward, extend: true }, Command::Delete { dir: Dir::Back, word: false }]),
        ("join", vec![Command::Move { by: MoveBy::Para, dir: Dir::Forward, extend: false }, Command::Delete { dir: Dir::Back, word: false }]),
        ("breaks", vec![Command::LineBreak, Command::InsertTab, Command::PageBreak]),
        ("select-all-typing", vec![Command::SelectAll, Command::InsertText("Replaced".into())]),
        (
            "bold",
            vec![Command::Move { by: MoveBy::Word, dir: Dir::Forward, extend: true }, Command::Toggle(Toggle::Bold), Command::InsertText("x".into())],
        ),
        ("italic-all", vec![Command::SelectAll, Command::Toggle(Toggle::Italic)]),
        ("align", vec![Command::Align(Jc::Center), Command::Move { by: MoveBy::Para, dir: Dir::Forward, extend: true }, Command::Align(Jc::Right)]),
    ]
}

/// Moves the caret into the first table cell, if the document has one.
fn in_table(e: &mut Editor, fonts: &Fonts) -> bool {
    for _ in 0..400 {
        if e.selection().focus().is_some_and(|p| !e.index().chain_of(&p).is_empty()) {
            return true;
        }
        e.execute(Command::Move { by: MoveBy::Para, dir: Dir::Forward, extend: false }, fonts);
    }
    false
}

/// Runs to the first table cell and adds a row there (Tab in the table's last cell), typing into it.
fn table_family(e: &mut Editor, fonts: &Fonts) -> bool {
    for _ in 0..400 {
        if e.selection().focus().is_some_and(|p| !e.index().chain_of(&p).is_empty()) {
            break;
        }
        e.execute(Command::Move { by: MoveBy::Para, dir: Dir::Forward, extend: false }, fonts);
    }
    if !e.selection().focus().is_some_and(|p| !e.index().chain_of(&p).is_empty()) {
        return false;
    }
    // Tab to the innermost table's last cell, then once more: a new row.
    for _ in 0..2000 {
        let before = e.history().undo_depth();
        e.execute(Command::NextCell { back: false }, fonts);
        if e.history().undo_depth() > before {
            e.execute(Command::InsertText("row".into()), fonts);
            return true;
        }
    }
    false
}

/// The first paragraph of every story besides the body: headers and footers, notes and text boxes.
fn story_starts(e: &Editor) -> Vec<Position> {
    use twip::docx::{Block, Inline};
    use twip::layout::{FloatId, ParaId};
    let first = |b: &[Block]| b.iter().find_map(|b| if let Block::Para(p) = b { Some(ParaId(p.id)) } else { None });
    let d = e.document();
    let mut out = Vec::new();
    for (name, b) in &d.stories {
        if let (Some(s), Some(p)) = (e.index().header_footer(name), first(b)) {
            out.push(Position::text(s, p, Unit(0)));
        }
    }
    for (id, b) in &d.footnotes {
        out.extend(first(b).map(|p| Position::text(StoryId::Footnote(*id), p, Unit(0))));
    }
    for (id, b) in &d.endnotes {
        out.extend(first(b).map(|p| Position::text(StoryId::Endnote(*id), p, Unit(0))));
    }
    for b in d.sections.iter().flat_map(|s| s.blocks.iter()) {
        if let Block::Para(p) = b {
            for i in &p.inlines {
                if let Inline::Float(f, _) = i
                    && let Some(tb) = &f.text_box
                {
                    out.extend(first(&tb.blocks).map(|q| Position::text(StoryId::TextBox(FloatId(f.id)), q, Unit(0))));
                }
            }
        }
    }
    out
}

/// The body's floats (document order).
fn floats(e: &Editor) -> Vec<twip::layout::FloatId> {
    use twip::docx::{Block, Inline};
    e.document()
        .sections
        .iter()
        .flat_map(|s| s.blocks.iter())
        .filter_map(|b| if let Block::Para(p) = b { Some(p) } else { None })
        .flat_map(|p| p.inlines.iter())
        .filter_map(|i| if let Inline::Float(f, _) = i { Some(twip::layout::FloatId(f.id)) } else { None })
        .collect()
}

fn dump(e: &mut Editor) -> serde_json::Value {
    inspect::inspect(e, "dump", true)["dump"].clone()
}

/// Sends one host message; `pending` is set when the editor doesn't know the command yet.
fn send(e: &mut Editor, fonts: &Fonts, mut msg: serde_json::Value, pending: &mut bool) -> serde_json::Value {
    // Each editor wants increasing sequence numbers; one counter for all of them does.
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    msg["seq"] = serde_json::json!(SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
    let r: serde_json::Value = serde_json::from_str(&e.apply_json(&msg.to_string(), fonts)).unwrap_or_default();
    if std::env::var_os("RT_TRACE").is_some() {
        eprintln!("  {msg} -> {}", r.get("error").or(r.get("answer")).map(|v| v.to_string()).unwrap_or_default());
    }
    *pending |= r["answer"]["error"] == "unknown command";
    r
}

/// The first field's (or control's) id, from the elements query.
fn first_element(e: &mut Editor, fonts: &Fonts, field: bool) -> Option<serde_json::Value> {
    let r = send(e, fonts, serde_json::json!({ "type": "query", "q": "elements" }), &mut false);
    r["answer"].as_array()?.iter().find(|x| (x["kind"] == "field") == field).map(|x| x["id"].clone())
}

/// The edit families.
const MERGE: &[&str] = &["field-insert", "field-code", "field-remove", "control-insert", "control-tag", "region-wrap", "data-write", "data-remove"];

/// Runs merge family `label`; whether it changed the document.
fn merge_family(e: &mut Editor, fonts: &Fonts, label: &str, pending: &mut bool) -> bool {
    use serde_json::json;
    let command = |id: &str, arg: serde_json::Value| json!({ "type": "command", "id": id, "arg": arg });
    let before = e.history().undo_depth();
    let msg = match label {
        "field-insert" => command("field.insert", json!({ "code": "MERGEFIELD Roundtrip.Name" })),
        "field-code" => match first_element(e, fonts, true) {
            Some(id) => command("field.set", json!({ "id": id, "code": "MERGEFIELD Roundtrip.Changed" })),
            None => return false,
        },
        "field-remove" => match first_element(e, fonts, true) {
            Some(id) => command("element.remove", json!({ "id": id })),
            None => return false,
        },
        "control-insert" => command("control.insert", json!({ "kind": "plain_text", "tag": "Roundtrip" })),
        "control-tag" => match first_element(e, fonts, false) {
            Some(id) => command("control.set", json!({ "id": id, "tag": "Roundtrip:changed" })),
            None => return false,
        },
        "region-wrap" => {
            if !in_table(e, fonts) {
                return false;
            }
            e.execute(Command::SelectCells(twip::editing::selection::CellShape::Rows), fonts);
            command("region.wrap", json!({ "tag": "Roundtrip:rows" }))
        }
        "data-write" => {
            command("data.write", json!({ "namespace": "urn:roundtrip", "xml": "<roundtrip xmlns=\"urn:roundtrip\"><a>1</a></roundtrip>" }))
        }
        "data-remove" => {
            let r = send(e, fonts, json!({ "type": "query", "q": "data" }), &mut false);
            match r["answer"].as_array().and_then(|v| v.first()).map(|p| p["namespace"].clone()) {
                Some(ns) => command("data.remove", json!({ "namespace": ns })),
                None => return false,
            }
        }
        _ => return false,
    };
    send(e, fonts, msg, pending);
    e.history().undo_depth() > before
}

/// What a merge edit must keep through a save: the elements (with their codes, tags and kinds) and the data parts.
fn merge_state(e: &mut Editor, fonts: &Fonts) -> serde_json::Value {
    let mut strip = |q: &str| {
        let mut v = send(e, fonts, serde_json::json!({ "type": "query", "q": q }), &mut false)["answer"].take();
        // Ids are the session's: a reread numbers them anew.
        for x in v.as_array_mut().into_iter().flatten() {
            if let Some(o) = x.as_object_mut() {
                o.retain(|k, _| !["id", "parent"].contains(&k.as_str()));
                // A text box's story is named with its session id too.
                if let Some(s) = o.get_mut("story").filter(|s| s.as_str().is_some_and(|s| s.starts_with("text box "))) {
                    *s = "text box".into();
                }
            }
        }
        v
    };
    serde_json::json!({ "elements": strip("elements"), "data": strip("data") })
}

/// Where two [`merge_state`]s first differ: the list, the index, and the entry before and after.
fn merge_difference(a: &serde_json::Value, b: &serde_json::Value) -> String {
    for k in ["elements", "data"] {
        let (x, y) = (a[k].as_array().cloned().unwrap_or_default(), b[k].as_array().cloned().unwrap_or_default());
        if let Some(i) = (0..x.len().max(y.len())).find(|&i| x.get(i) != y.get(i)) {
            let at = |v: &[serde_json::Value]| v.get(i).map_or("nothing".into(), |e| e.to_string().chars().take(400).collect::<String>());
            return format!("{k}[{i}] (of {} and {}): {} became {}", x.len(), y.len(), at(&x), at(&y));
        }
    }
    String::new()
}

pub fn run(args: &[String]) -> Result<(), String> {
    let edits = args.iter().position(|a| a == "--edits").map(|i| args.get(i + 1).cloned().unwrap_or_default());
    let keep = args.iter().position(|a| a == "--keep").and_then(|i| args.get(i + 1)).map(PathBuf::from);
    let skip: Vec<usize> = args.iter().enumerate().filter(|(_, a)| *a == "--edits" || *a == "--keep").flat_map(|(i, _)| [i, i + 1]).collect();
    let paths: Vec<String> = args.iter().enumerate().filter(|(i, _)| !skip.contains(i)).map(|(_, a)| a.clone()).collect();
    if let Some(k) = &keep {
        std::fs::create_dir_all(k).map_err(|e| format!("{}: {e}", k.display()))?;
    }
    let fonts = Fonts::system();
    let (mut ok, mut failed, mut skipped) = (0, Vec::new(), 0);
    // Merge families whose command the editor doesn't have yet.
    let mut pending: Vec<&str> = Vec::new();
    for f in files(&paths) {
        let bytes = std::fs::read(&f).map_err(|e| format!("{}: {e}", f.display()))?;
        let name = f.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        if edits.is_none() {
            match twip::docx::read_editable(&bytes).and_then(|d| twip::docx::write::save(&d)) {
                Ok(out) if out == bytes => ok += 1,
                Ok(_) => failed.push(format!("{}: unedited save changed the bytes", f.display())),
                Err(e) => failed.push(format!("{}: {e}", f.display())),
            }
            continue;
        }
        let Ok(probe) = Editor::open(&bytes, &fonts) else {
            skipped += 1;
            continue;
        };
        drop(probe);
        let mut run_family = |label: &str, apply: &mut dyn FnMut(&mut Editor) -> bool| {
            if std::env::var_os("RT_TRACE").is_some() {
                eprintln!("{} {label}", f.display());
            }
            let Ok(mut e) = Editor::open(&bytes, &fonts) else { return };
            if !apply(&mut e) {
                return;
            }
            let want = dump(&mut e);
            let want_merge = (edits.as_deref() == Some("merge")).then(|| merge_state(&mut e, &fonts));
            let saved = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| twip::docx::write::save(e.document()))).unwrap_or_else(|p| {
                Err(format!(
                    "the writer panicked: {}",
                    p.downcast_ref::<String>().cloned().or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default()
                ))
            });
            match saved {
                Ok(out) => {
                    if let Some(k) = &keep {
                        let _ = std::fs::write(k.join(format!("{name}.{label}.docx")), &out);
                    }
                    match Editor::open(&out, &fonts).map(|mut again| (dump(&mut again), want_merge.as_ref().map(|_| merge_state(&mut again, &fonts))))
                    {
                        Ok((got, merge)) if got == want && merge == want_merge => ok += 1,
                        Ok((got, merge)) if got == want => failed.push(format!(
                            "{} {label}: the saved elements or data parts read back differently: {}",
                            f.display(),
                            merge_difference(
                                want_merge.as_ref().unwrap_or(&serde_json::Value::Null),
                                merge.as_ref().unwrap_or(&serde_json::Value::Null)
                            )
                        )),
                        Ok((got, _)) => {
                            let (a, b) = (
                                want["paragraphs"].as_array().cloned().unwrap_or_default(),
                                got["paragraphs"].as_array().cloned().unwrap_or_default(),
                            );
                            let at = (0..a.len().max(b.len())).find(|&i| a.get(i) != b.get(i));
                            // Each paragraph's JSON from a little before where the two first differ.
                            let (sa, sb) = (
                                at.and_then(|i| a.get(i)).map(|p| p.to_string()).unwrap_or_default(),
                                at.and_then(|i| b.get(i)).map(|p| p.to_string()).unwrap_or_default(),
                            );
                            let from = sa.chars().zip(sb.chars()).take_while(|(x, y)| x == y).count().saturating_sub(40);
                            let show = |v: &[serde_json::Value], i: Option<usize>| {
                                i.and_then(|i| v.get(i)).map_or("-".to_string(), |p| p.to_string().chars().skip(from).take(160).collect())
                            };
                            // The paragraphs agree: the first other part of the dump that doesn't, from where it differs.
                            let other = || {
                                let keys = want.as_object().map(|o| o.keys().cloned().collect::<Vec<_>>()).unwrap_or_default();
                                let k = keys.iter().find(|k| *k != "paragraphs" && want[k.as_str()] != got[k.as_str()])?;
                                let (x, y) = (want[k.as_str()].to_string(), got[k.as_str()].to_string());
                                let from = x.chars().zip(y.chars()).take_while(|(p, q)| p == q).count().saturating_sub(60);
                                let cut = |s: &str| s.chars().skip(from).take(240).collect::<String>();
                                Some(format!("{k}: {} became {}", cut(&x), cut(&y)))
                            };
                            let what = match at {
                                Some(_) => {
                                    format!("paragraph {at:?} {} became {} ({} → {} paragraphs)", show(&a, at), show(&b, at), a.len(), b.len())
                                }
                                None => other().unwrap_or_default(),
                            };
                            failed.push(format!("{} {label}: the saved document reads back differently: {what}", f.display()))
                        }
                        Err(err) => failed.push(format!("{} {label}: the saved document doesn't read: {err}", f.display())),
                    }
                }
                Err(err) => failed.push(format!("{} {label}: {err}", f.display())),
            }
        };
        if edits.as_deref() == Some("merge") {
            for label in MERGE {
                let mut not_yet = false;
                run_family(label, &mut |e| merge_family(e, &fonts, label, &mut not_yet));
                if not_yet && !pending.contains(label) {
                    pending.push(label);
                }
            }
            continue;
        }
        for (label, cmds) in families() {
            run_family(label, &mut |e| {
                for c in &cmds {
                    e.execute(c.clone(), &fonts);
                }
                e.history().undo_depth() > 0
            });
        }
        run_family("table-row", &mut |e| table_family(e, &fonts));
        for (label, cmd) in [
            ("insert-column", Command::InsertColumns { right: true }),
            ("delete-row", Command::DeleteRows),
            ("split-cell", Command::SplitCell { cols: 2 }),
            ("nested-table", Command::InsertTable { rows: 2, cols: 2 }),
        ] {
            run_family(label, &mut |e| {
                in_table(e, &fonts) && {
                    let before = e.history().undo_depth();
                    e.execute(cmd.clone(), &fonts);
                    e.execute(Command::InsertText("t".into()), &fonts);
                    e.history().undo_depth() > before
                }
            });
        }
        run_family("stories", &mut |e| {
            for p in story_starts(e) {
                e.set_caret_for_benchmark(Selection::Caret(p));
                e.execute(Command::InsertText("Story ".into()), &fonts);
            }
            e.history().undo_depth() > 0
        });
        for (label, del) in [("float-nudge", false), ("float-delete", true)] {
            run_family(label, &mut |e| {
                let Some(&f) = floats(e).first() else { return false };
                e.set_caret_for_benchmark(Selection::Object(ObjectRef::Floating(f)));
                let c = if del {
                    Command::Delete { dir: Dir::Forward, word: false }
                } else {
                    Command::Move { by: MoveBy::Char, dir: Dir::Forward, extend: false }
                };
                e.execute(c, &fonts);
                e.history().undo_depth() > 0
            });
        }
        run_family("new-table", &mut |e| {
            e.execute(Command::InsertTable { rows: 2, cols: 3 }, &fonts);
            e.execute(Command::InsertText("cell".into()), &fonts);
            e.history().undo_depth() > 0
        });
    }
    for f in &failed {
        println!("FAIL {f}");
    }
    if !pending.is_empty() {
        println!("pending (command not implemented yet): {}", pending.join(", "));
    }
    println!("{ok} ok, {} failed, {skipped} not readable", failed.len());
    if failed.is_empty() { Ok(()) } else { Err(format!("{} round trips failed", failed.len())) }
}
