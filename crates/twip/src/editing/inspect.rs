//! Test inspection (feature `inspect`): the editor's semantic state as JSON. Paragraphs and
//! tables are numbered by their place in document order (ids aren't stable across opens).

use std::collections::HashMap;

use serde_json::{Value, json};

use crate::docx::Block;
use crate::layout::{LayoutStatus, ParaId, TableId};

use super::Editor;
use super::index::Level;
use super::position::{Affinity, Place, Position, StoryId};
use super::selection::Selection;
use super::text::UnitText;

/// Document-order numbering of paragraphs and tables: the body's first, then the other stories' (headers and footers
/// in part-name order, footnotes and endnotes by id, text boxes in document order), continuing the count.
struct Order {
    paras: HashMap<ParaId, usize>,
    tables: HashMap<TableId, usize>,
    /// Text boxes by document order (float ids aren't stable across opens).
    boxes: HashMap<crate::layout::FloatId, usize>,
    /// Every float (pictures and text boxes) in document order, with its anchor paragraph.
    floats: Vec<(crate::layout::FloatId, ParaId)>,
}

/// The stories besides the body, in numbering order, with their blocks.
fn stories(e: &Editor) -> Vec<(StoryId, &[Block])> {
    let d = e.document();
    let mut out: Vec<(StoryId, &[Block])> = d.stories.values().enumerate().map(|(k, b)| (StoryId::HeaderFooter(k as u32), b.as_slice())).collect();
    let mut notes: Vec<_> = d.footnotes.iter().collect();
    notes.sort_by_key(|(id, _)| **id);
    out.extend(notes.into_iter().map(|(id, b)| (StoryId::Footnote(*id), b.as_slice())));
    let mut ends: Vec<_> = d.endnotes.iter().collect();
    ends.sort_by_key(|(id, _)| **id);
    out.extend(ends.into_iter().map(|(id, b)| (StoryId::Endnote(*id), b.as_slice())));
    fn boxes<'a>(blocks: &'a [Block], out: &mut Vec<(StoryId, &'a [Block])>) {
        for b in blocks {
            match b {
                Block::Para(p) => {
                    for i in &p.inlines {
                        if let crate::docx::Inline::Float(f, _) = i
                            && let Some(tb) = &f.text_box
                        {
                            out.push((StoryId::TextBox(crate::layout::FloatId(f.id)), tb.blocks.as_slice()));
                            boxes(&tb.blocks, out);
                        }
                    }
                }
                Block::Table(t) => t.rows.iter().flat_map(|r| r.cells.iter()).for_each(|c| boxes(&c.blocks, out)),
            }
        }
    }
    let mut tb = Vec::new();
    d.sections.iter().for_each(|s| boxes(&s.blocks, &mut tb));
    out.extend(tb);
    out
}

fn order(e: &Editor) -> Order {
    fn walk(blocks: &[Block], o: &mut Order) {
        for b in blocks {
            match b {
                Block::Para(p) => {
                    let n = o.paras.len();
                    o.paras.insert(ParaId(p.id), n);
                    for i in &p.inlines {
                        if let crate::docx::Inline::Float(f, _) = i {
                            o.floats.push((crate::layout::FloatId(f.id), ParaId(p.id)));
                        }
                    }
                }
                Block::Table(t) => {
                    let n = o.tables.len();
                    o.tables.insert(TableId(t.id), n);
                    t.rows.iter().flat_map(|r| r.cells.iter()).for_each(|c| walk(&c.blocks, o));
                }
            }
        }
    }
    let mut o = Order { paras: HashMap::new(), tables: HashMap::new(), boxes: HashMap::new(), floats: Vec::new() };
    e.document().sections.iter().for_each(|s| walk(&s.blocks, &mut o));
    for (sid, blocks) in stories(e) {
        if let StoryId::TextBox(f) = sid {
            let n = o.boxes.len();
            o.boxes.insert(f, n);
        }
        walk(blocks, &mut o);
    }
    o
}

/// Points to 0.01.
fn r2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

fn story(s: StoryId, o: &Order) -> Value {
    match s {
        StoryId::Body => json!("body"),
        StoryId::HeaderFooter(k) => json!(format!("header-footer:{k}")),
        StoryId::Footnote(id) => json!(format!("footnote:{id}")),
        StoryId::Endnote(id) => json!(format!("endnote:{id}")),
        StoryId::TextBox(f) => json!(format!("text-box:{}", o.boxes.get(&f).map_or(-1, |&n| n as i64))),
    }
}

fn chain(levels: &[Level], o: &Order) -> Value {
    Value::Array(
        levels
            .iter()
            .map(|l| json!({ "table": o.tables.get(&l.table), "row": l.row, "cell": (l.cell != usize::MAX).then_some(l.cell), "depth": l.depth }))
            .collect(),
    )
}

fn position(p: &Position, o: &Order) -> Value {
    match p.place {
        Place::Text { para, offset, affinity } => json!({
            "story": story(p.story, o),
            "para": o.paras.get(&para),
            "offset": offset.0,
            "affinity": if affinity == Affinity::Upstream { "upstream" } else { "downstream" },
        }),
        Place::RowEnd { table, row } => json!({ "story": story(p.story, o), "row_end": { "table": o.tables.get(&table), "row": row } }),
    }
}

/// A paragraph's run formatting summary: (units, flags) with adjacent equal flags merged. Flags: b bold, i italic,
/// u underline, s strike, h hidden, ^ superscript, _ subscript.
fn runs(p: &crate::docx::Paragraph) -> Value {
    let mut out: Vec<(usize, String)> = Vec::new();
    for i in &p.inlines {
        let n = i.units();
        let Some(r) = i.run().filter(|_| n > 0) else { continue };
        let mut f = String::new();
        for (on, c) in [(r.bold, 'b'), (r.italic, 'i'), (r.underline.is_some(), 'u'), (r.strike, 's'), (r.hidden, 'h')] {
            if on {
                f.push(c);
            }
        }
        match r.vert_align {
            crate::docx::props::VertAlign::Superscript => f.push('^'),
            crate::docx::props::VertAlign::Subscript => f.push('_'),
            crate::docx::props::VertAlign::Baseline => {}
        }
        match out.last_mut() {
            Some(last) if last.1 == f => last.0 += n,
            _ => out.push((n, f)),
        }
    }
    json!(out.into_iter().map(|(n, f)| json!([n, f])).collect::<Vec<_>>())
}

/// A run property per unit stretch: (units, value) with adjacent equal values merged.
fn per_run(p: &crate::docx::Paragraph, f: impl Fn(&crate::docx::RunStyle) -> Value) -> Value {
    let mut out: Vec<(usize, Value)> = Vec::new();
    for i in &p.inlines {
        let n = i.units();
        let Some(r) = i.run().filter(|_| n > 0) else { continue };
        let v = f(r);
        match out.last_mut() {
            Some(last) if last.1 == v => last.0 += n,
            _ => out.push((n, v)),
        }
    }
    json!(out.into_iter().map(|(n, v)| json!([n, v])).collect::<Vec<_>>())
}

/// Paragraph metrics paragraph commands change (points).
fn metrics(p: &crate::docx::Paragraph) -> Value {
    let s = &p.style;
    json!({ "ind_left": r2(s.ind_left), "ind_first": r2(s.ind_first), "before": r2(s.space_before), "after": r2(s.space_after), "line": r2(s.line), "keep_next": s.keep_next })
}

fn jc(p: &crate::docx::Paragraph) -> &'static str {
    match p.style.jc {
        crate::docx::props::Jc::Left => "left",
        crate::docx::props::Jc::Center => "center",
        crate::docx::props::Jc::Right => "right",
        crate::docx::props::Jc::Both => "both",
        crate::docx::props::Jc::Distribute => "distribute",
    }
}

/// The sections `what` names (comma-separated; empty: all but geometry and dump).
pub fn inspect(e: &mut Editor, what: &str, settled: bool) -> Value {
    let o = order(e);
    let want = |k: &str| if what.is_empty() { !matches!(k, "geometry" | "dump" | "view") } else { what.split(',').any(|w| w.trim() == k) };
    let mut out = serde_json::Map::new();
    if want("sync") {
        let s = e.snapshot();
        let status = match s.status() {
            LayoutStatus::Pending { .. } => "pending",
            LayoutStatus::Converged => "converged",
            LayoutStatus::Stabilized { .. } => "stabilized",
            LayoutStatus::Inconsistent { .. } => "inconsistent",
        };
        out.insert(
            "sync".into(),
            json!({
                "seq_applied": e.seq_applied(),
                "revision": e.revision(),
                "layout_revision": s.layout_revision().0,
                "status": status,
                "provisional_from": s.info().iter().position(|i| i.provisional),
                "settled": settled,
            }),
        );
    }
    let sel = e.selection();
    if want("caret") {
        let v = match sel.focus() {
            Some(f) => {
                let mut v = position(&f, &o);
                v["chain"] = chain(&e.index().chain_of(&f), &o);
                if let Some((para, off)) = f.para_offset()
                    && let Some(p) = e.index().para(e.document(), para)
                {
                    let t = UnitText::of(p).text;
                    let (a, b): (String, String) = (t.chars().take(off.0 as usize).collect(), t.chars().skip(off.0 as usize).collect());
                    let tail: String = a.chars().rev().take(12).collect::<Vec<_>>().into_iter().rev().collect();
                    v["text_around"] = json!(format!("{}|{}", tail, b.chars().take(12).collect::<String>()));
                }
                v["hidden"] = json!(e.caret_hidden());
                v["rect"] = match e.caret_view() {
                    Some(c) => json!({ "page": c.page, "x": c.x, "top": c.top, "bottom": c.bottom }),
                    None => Value::Null,
                };
                v
            }
            None => Value::Null,
        };
        out.insert("caret".into(), v);
    }
    if want("selection") {
        let v = match sel {
            Selection::Caret(p) => json!({ "kind": "caret", "anchor": position(&p, &o), "focus": position(&p, &o) }),
            Selection::Text { anchor, focus, unit } => {
                json!({ "kind": "text", "anchor": position(&anchor, &o), "focus": position(&focus, &o), "granularity": format!("{unit:?}").to_lowercase() })
            }
            Selection::Cells { table, anchor, focus, shape } => json!({
                "kind": "cells", "table": o.tables.get(&table), "anchor": [anchor.row, anchor.col], "focus": [focus.row, focus.col],
                "shape": format!("{shape:?}").to_lowercase(),
            }),
            Selection::Block { anchor, focus } => json!({ "kind": "block", "anchor": position(&anchor, &o), "focus": position(&focus, &o) }),
            Selection::Object(super::selection::ObjectRef::Inline { para, offset }) => {
                json!({ "kind": "object", "inline": { "para": o.paras.get(&para), "offset": offset.0 } })
            }
            Selection::Object(super::selection::ObjectRef::Floating(f)) => {
                json!({ "kind": "object", "float": o.floats.iter().position(|(g, _)| *g == f) })
            }
        };
        out.insert("selection".into(), v);
    }
    // The view of every page as canonical JSON (regions, tooltip, drop).
    #[cfg(any(test, feature = "inspect"))]
    if want("view") {
        let pages = e.snapshot().pages().len();
        let v = e.view(0..pages);
        out.insert("view".into(), serde_json::from_str(&super::view::canonical(&v)).unwrap_or(Value::Null));
    }
    if want("undo") {
        let h = e.history();
        out.insert(
            "undo".into(),
            json!({ "undo_depth": h.undo_depth(), "redo_depth": h.redo_depth(), "next_undo": h.next_undo(), "next_redo": h.next_redo(), "bytes": h.bytes() }),
        );
    }
    if want("integrity") {
        let v = super::integrity::check(e.document(), e.index(), &sel);
        out.insert("integrity".into(), Value::Array(v.iter().map(|v| json!({ "invariant": v.invariant, "detail": v.detail })).collect()));
    }
    if want("dump") {
        let mut paras = Vec::new();
        let mut tables = Vec::new();
        fn walk(_e: &Editor, blocks: &[Block], depth: usize, paras: &mut Vec<Value>, tables: &mut Vec<Value>) {
            for b in blocks {
                match b {
                    Block::Para(p) => paras.push(json!({
                        "text": UnitText::of(p).text, "depth": depth, "runs": runs(p), "jc": jc(p), "style": p.style.style_id,
                        "label": p.list.as_ref().map(|l| l.text.clone()),
                        "sizes": per_run(p, |r| json!(r2(r.size))),
                        "colors": per_run(p, |r| json!(r.color.map_or("auto".to_string(), |c| format!("{c:06X}")))),
                        "para": metrics(p),
                    })),
                    Block::Table(t) => {
                        tables.push(json!({
                            "rows": t.rows.len(), "cells": t.rows.iter().map(|r| r.cells.len()).collect::<Vec<_>>(), "depth": depth + 1,
                            "grid": t.grid.iter().map(|w| r2(*w)).collect::<Vec<_>>(),
                            "heights": t.rows.iter().map(|r| r.height.as_ref().map(|(h, rule)| json!([r2(*h), rule]))).collect::<Vec<_>>(),
                            "header_rows": t.rows.iter().take_while(|r| r.header).count(),
                            // Per cell: vertical alignment and fill ("-" for none).
                            "cell_fmt": t.rows.iter().map(|r| r.cells.iter().map(|c| format!("{}:{}", c.v_align, c.shading.map_or("-".to_string(), |f| format!("{f:06X}")))).collect::<Vec<_>>()).collect::<Vec<_>>(),
                        }));
                        t.rows.iter().flat_map(|r| r.cells.iter()).for_each(|c| walk(_e, &c.blocks, depth + 1, paras, tables));
                    }
                }
            }
        }
        for s in &e.document().sections {
            walk(e, &s.blocks, 0, &mut paras, &mut tables);
        }
        // The other stories: their paragraphs continue the body's numbering (`index`), their tables too.
        let mut others = Vec::new();
        for (sid, blocks) in stories(e) {
            let first = paras.len() + others.iter().map(|o: &Value| o["paragraphs"].as_array().map_or(0, Vec::len)).sum::<usize>();
            let (mut ps, mut ts) = (Vec::new(), Vec::new());
            walk(e, blocks, 0, &mut ps, &mut ts);
            for (k, p) in ps.iter_mut().enumerate() {
                p["index"] = json!(first + k);
            }
            tables.extend(ts);
            others.push(json!({ "story": story(sid, &o), "paragraphs": ps }));
        }
        // Floats in document order: anchor paragraph, wrap and placement as the model holds them.
        let floats: Vec<Value> = o
            .floats
            .iter()
            .filter_map(|&(f, anchor)| {
                let p = e.index().para(e.document(), anchor)?;
                p.inlines.iter().find_map(|i| match i {
                    crate::docx::Inline::Float(fl, _) if crate::layout::FloatId(fl.id) == f => Some(json!({
                        "anchor": o.paras.get(&anchor), "text_box": fl.text_box.is_some(), "wrap": fl.wrap,
                        "x": r2(fl.h_offset), "y": r2(fl.v_offset), "w": r2(fl.w), "h": r2(fl.h),
                    })),
                    _ => None,
                })
            })
            .collect();
        out.insert("dump".into(), json!({ "paragraphs": paras, "tables": tables, "stories": others, "floats": floats }));
    }
    if want("geometry") {
        let stack = e.stack();
        let mut lines = Vec::new();
        let mut cells = Vec::new();
        let mut floats = Vec::new();
        let mut pictures = Vec::new();
        let mut rows = Vec::new();
        for (page, g) in e.page_geometries() {
            let to = |x: f64, y: f64| stack.to_doc(page, x, y).unwrap_or((x, y));
            for r in g.rows.iter().filter(|r| !r.repeat) {
                let (x0, y0) = to(r.rect[0], r.rect[1]);
                let (x1, y1) = to(r.rect[2], r.rect[3]);
                rows.push(json!({
                    "page": page, "table": o.tables.get(&r.table), "row": r.row, "rect": [x0, y0, x1, y1], "depth": r.outer.len() + 1,
                    "edges": r.edges.iter().map(|(x, k)| json!([to(*x, 0.0).0, k])).collect::<Vec<_>>(),
                }));
            }
            for s in &g.stories {
                let Some(f) = s.float else { continue };
                let (x0, y0) = to(s.rect[0], s.rect[1]);
                let (x1, y1) = to(s.rect[2], s.rect[3]);
                floats.push(json!({ "page": page, "float": o.floats.iter().position(|(g, _)| *g == f), "rect": [x0, y0, x1, y1] }));
            }
            for l in &g.lines {
                // Inline pictures on the line, in order: their boxes (what handles and selection outline).
                for w in l.stops.windows(2).filter(|w| w[1].unit.0 == w[0].unit.0 + 1) {
                    let is_pic = e
                        .index
                        .para(&e.doc, l.para)
                        .and_then(|p| super::stories::inline_at(p, w[0].unit))
                        .is_some_and(|i| matches!(i, crate::docx::Inline::Picture(..)));
                    if let Some(r) = is_pic.then(|| e.inline_box(&g, l.para, w[0].unit)).flatten() {
                        let (x0, y0) = to(r[0], r[1]);
                        let (x1, y1) = to(r[2], r[3]);
                        pictures.push(json!({ "page": page, "para": o.paras.get(&l.para), "offset": w[0].unit.0, "rect": [x0, y0, x1, y1] }));
                    }
                }
                let (x0, y0) = to(l.x0(), l.top);
                let (x1, y1) = to(l.x1(), l.bottom);
                lines.push(json!({
                    "page": page, "story": story(g.stories[l.story].story, &o), "para": o.paras.get(&l.para),
                    "first": l.first().0, "last": l.last().0, "rect": [x0, y0, x1, y1], "repeat": l.repeat,
                    "stops": l.stops.iter().map(|s| json!([to(s.x, 0.0).0, s.unit.0])).collect::<Vec<_>>(),
                }));
            }
            for c in &g.cells {
                let (x0, y0) = to(c.rect[0], c.rect[1]);
                let (x1, y1) = to(c.rect[2], c.rect[3]);
                let last = c.chain.last().copied();
                cells.push(json!({
                    "page": page, "table": last.and_then(|k| o.tables.get(&k.0)), "row": last.map(|k| k.1), "cell": last.map(|k| k.2),
                    "depth": c.chain.len(), "rect": [x0, y0, x1, y1],
                }));
            }
        }
        out.insert("geometry".into(), json!({ "pages": stack.pages, "width": stack.width, "height": stack.height, "lines": lines, "cells": cells, "floats": floats, "pictures": pictures, "rows": rows }));
    }
    if want("config") {
        out.insert(
            "config".into(),
            json!({ "autoscroll": { "formula": "min(40 + 12*d, 2400) css px/s, d css px outside", "dead_zone_px": 4 }, "page_gap_pt": super::view::PAGE_GAP }),
        );
    }
    Value::Object(out)
}
