//! A layout as text, one line per drawn item, for comparing layouts item by item (`twip dump`, `dumpdiff`). Paragraph and table ids are renumbered in document order and line ids by first
//! use, so the text doesn't depend on process-wide counters.

use std::collections::HashMap;
use std::fmt::Write;

use crate::docx::{Block, Document, Inline};

use super::fragment::{BoxFragment, Child, Item, PageChild, PageFragment, RowFragment};
use super::snapshot::Snapshot;
use super::traverse::paint;

/// Paragraph and table ids renumbered in document order: body, stories, footnotes, endnotes, separators.
fn ids(doc: &Document) -> HashMap<u64, usize> {
    fn walk(blocks: &[Block], out: &mut HashMap<u64, usize>) {
        for b in blocks {
            match b {
                Block::Para(p) => {
                    let n = out.len() + 1;
                    out.entry(p.id).or_insert(n);
                    for i in &p.inlines {
                        if let Inline::Float(f, _) = i
                            && let Some(tb) = &f.text_box
                        {
                            walk(&tb.blocks, out);
                        }
                    }
                }
                Block::Table(t) => {
                    let n = out.len() + 1;
                    out.entry(t.id).or_insert(n);
                    for r in &t.rows {
                        for c in &r.cells {
                            walk(&c.blocks, out);
                        }
                    }
                }
            }
        }
    }
    let mut out = HashMap::new();
    for s in &doc.sections {
        walk(&s.blocks, &mut out);
    }
    for b in doc.stories.values() {
        walk(b, &mut out);
    }
    for notes in [&doc.footnotes, &doc.endnotes] {
        let mut keys: Vec<_> = notes.keys().collect();
        keys.sort();
        for k in keys {
            walk(&notes[k], &mut out);
        }
    }
    let mut keys: Vec<_> = doc.separators.keys().collect();
    keys.sort();
    for k in keys {
        walk(&doc.separators[k], &mut out);
    }
    out
}

/// The table rows of a page, in page coordinates, in flow order.
pub(crate) fn rows<'a>(page: &'a PageFragment, out: &mut Vec<(&'a RowFragment, u64, f64, f64)>) {
    fn walk<'a>(children: &'a [Child], dx: f64, dy: f64, out: &mut Vec<(&'a RowFragment, u64, f64, f64)>) {
        for c in children {
            match c {
                Child::Box(b) => walk(&b.children, dx + b.offset.0, dy + b.offset.1, out),
                Child::Group { offset, children } => walk(children, dx + offset.0, dy + offset.1, out),
                Child::Table(t) => {
                    for r in t.rows.iter() {
                        out.push((r, t.table.0, dx, dy));
                        for cell in r.cells.iter() {
                            walk(&cell.content, dx, dy, out);
                        }
                    }
                }
                _ => {}
            }
        }
    }
    for c in page.children.iter() {
        let b: &BoxFragment = match c {
            PageChild::Body(b) => b,
            PageChild::Header(b) | PageChild::Footer(b) => b,
        };
        walk(&b.children, b.offset.0, b.offset.1, out);
    }
}

/// The dump text of a snapshot.
pub fn page_text(snapshot: &Snapshot, doc: &Document) -> String {
    let ids = ids(doc);
    let id = |v: u64| if v == 0 { 0 } else { ids.get(&v).copied().unwrap_or(usize::MAX) };
    let mut lines: HashMap<(usize, u32), usize> = HashMap::new();
    let mut s = String::new();
    let f = |v: f64| format!("{:.3}", v + 0.0);
    for (pi, page) in snapshot.pages().iter().enumerate() {
        writeln!(s, "page {pi} {} {} {:?}", f(page.w), f(page.h), page.body_area.map(|(a, b)| (f(a), f(b)))).unwrap();
        paint(page, &mut |p| match p.absolute() {
            Item::Glyphs(r) => {
                let n = lines.len();
                let line = *lines.entry((pi, p.line)).or_insert(n);
                let glyphs: Vec<String> = r.glyphs.iter().map(|(g, x)| format!("{g}@{}", f(*x))).collect();
                let chars: String = r.chars.iter().map(|c| c.0).collect();
                let cx: Vec<String> = r.chars.iter().map(|c| f(c.1)).collect();
                let src: Vec<u32> = r.chars.iter().map(|c| c.2.map_or(u32::MAX, |u| u.0)).collect();
                writeln!(
                    s,
                    "G {}/{}/{}/{}/{} size {} y {} x {} color {:?} para {} line {line} glyphs [{}] chars {chars:?} cx [{}] src {:?}",
                    r.face.family,
                    r.face.weight,
                    r.face.italic,
                    r.face.sim_bold,
                    r.face.sim_italic,
                    f(r.size),
                    f(r.baseline),
                    f(r.x),
                    r.colour,
                    id(r.para.map_or(0, |p| p.0)),
                    glyphs.join(" "),
                    cx.join(" "),
                    src
                )
                .unwrap();
            }
            Item::Rect { rect: b, colour: c } => writeln!(s, "R {} {} {} {} {c:06x}", f(b[0]), f(b[1]), f(b[2]), f(b[3])).unwrap(),
            Item::Image { rect: b, key } => {
                writeln!(s, "I {} {} {} {} {}", f(b[0]), f(b[1]), f(b[2]), f(b[3]), key.index.map_or(-1, |i| i as isize)).unwrap()
            }
            Item::Gridline { line: b } => writeln!(s, "L {} {} {} {}", f(b[0]), f(b[1]), f(b[2]), f(b[3])).unwrap(),
        });
        let mut rs = Vec::new();
        rows(page, &mut rs);
        // Rows without cells have no handles (and no edges): no row geometry is written for them.
        for (r, table, dx, dy) in rs.into_iter().filter(|(r, ..)| !r.edges.is_empty()) {
            let edges: Vec<String> = r.edges.iter().map(|(x, k)| format!("{}:{k}", f(*x + dx))).collect();
            writeln!(
                s,
                "T table {} row {} rect {} {} {} {} top {} edges [{}]",
                id(table),
                r.row,
                f(r.rect[0] + dx),
                f(r.rect[1] + dy),
                f(r.rect[2] + dx),
                f(r.rect[3] + dy),
                f(r.top + dy),
                edges.join(" ")
            )
            .unwrap();
        }
    }
    s
}
