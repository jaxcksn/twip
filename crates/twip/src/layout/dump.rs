//! A layout as text, one line per drawn item, for comparing layouts item by item (`twip dump`, `dumpdiff`). Paragraph and table ids are renumbered in document order and line ids by first
//! use, so the text doesn't depend on process-wide counters.

use std::collections::HashMap;
use std::fmt::Write;

use crate::docx::{Block, Document};

use super::fragment::{BoxFragment, Child, Item, PageChild, PageFragment, RowFragment};
use super::snapshot::Snapshot;
use super::traverse::paint_lines;

/// Paragraph and table ids renumbered in document order: body, stories, footnotes, endnotes, separators.
/// The paragraphs of text boxes (any depth), by id.
fn box_paras(doc: &Document) -> std::collections::HashSet<u64> {
    fn walk(blocks: &[Block], inside: bool, out: &mut std::collections::HashSet<u64>) {
        for b in blocks {
            match b {
                Block::Para(p) => {
                    if inside {
                        out.insert(p.id);
                    }
                    for i in &p.inlines {
                        if let Some(tb) = i.story().map(|(_, tb)| tb) {
                            walk(&tb.blocks, true, out);
                        }
                    }
                }
                Block::Table(t) => t.rows.iter().flat_map(|r| &r.cells).for_each(|c| walk(&c.blocks, inside, out)),
            }
        }
    }
    let mut out = std::collections::HashSet::new();
    doc.sections.iter().for_each(|s| walk(&s.blocks, false, &mut out));
    doc.stories.values().for_each(|b| walk(b, false, &mut out));
    out
}

fn ids(doc: &Document) -> HashMap<u64, usize> {
    fn walk(blocks: &[Block], out: &mut HashMap<u64, usize>) {
        for b in blocks {
            match b {
                Block::Para(p) => {
                    let n = out.len() + 1;
                    out.entry(p.id).or_insert(n);
                    for i in &p.inlines {
                        if let Some(tb) = i.story().map(|(_, tb)| tb) {
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

/// The text boxes of a page: its floats that hold text, and its header and footer, by their boxes (page coordinates)
/// and the paragraphs they hold.
fn text_boxes(page: &PageFragment, out: &mut Vec<(&'static str, [f64; 4], Vec<u64>)>) {
    fn paras(children: &[Child], out: &mut Vec<u64>) {
        for c in children {
            match c {
                Child::Para(p) => out.push(p.para.0),
                Child::Line { para: Some(p), .. } => out.push(p.0),
                Child::Box(b) => paras(&b.children, out),
                Child::Group { children, .. } => paras(children, out),
                Child::Table(t) => t.rows.iter().flat_map(|r| r.cells.iter()).for_each(|c| paras(&c.content, out)),
                _ => {}
            }
        }
    }
    fn walk(children: &[Child], out: &mut Vec<(&'static str, [f64; 4], Vec<u64>)>) {
        for c in children {
            match c {
                Child::Box(b) => {
                    if let super::fragment::BoxKind::Float { rect, .. } = b.kind {
                        let mut ps = Vec::new();
                        paras(&b.children, &mut ps);
                        if !ps.is_empty() {
                            ps.dedup();
                            out.push(("textbox", rect, ps));
                        }
                    }
                    walk(&b.children, out);
                }
                Child::Group { children, .. } => walk(children, out),
                _ => {}
            }
        }
    }
    for c in page.children.iter() {
        let b: &BoxFragment = match c {
            PageChild::Body(b) => b,
            PageChild::Header(b) | PageChild::Footer(b) => {
                // A header or footer, as a box holding its paragraphs: none of them is a body line.
                let mut ps = Vec::new();
                paras(&b.children, &mut ps);
                if !ps.is_empty() {
                    ps.dedup();
                    out.push(("story", [b.offset.0, b.offset.1, b.offset.0 + b.size.0, b.offset.1 + b.size.1], ps));
                }
                b
            }
        };
        walk(&b.children, out);
    }
}

/// The dump text of a snapshot.
pub fn page_text(snapshot: &Snapshot, doc: &Document) -> String {
    let ids = ids(doc);
    let in_box = box_paras(doc);
    let id = |v: u64| if v == 0 { 0 } else { ids.get(&v).copied().unwrap_or(usize::MAX) };
    let mut s = String::new();
    let mut base = 0;
    let f = |v: f64| format!("{:.3}", v + 0.0);
    for (pi, page) in snapshot.pages().iter().enumerate() {
        writeln!(s, "page {pi} {} {} {:?}", f(page.w), f(page.h), page.body_area.map(|(a, b)| (f(a), f(b)))).unwrap();
        let mut boxes = Vec::new();
        text_boxes(page, &mut boxes);
        for (kind, r, ps) in boxes {
            let ps: Vec<String> = ps.iter().map(|&p| id(p).to_string()).collect();
            writeln!(s, "X {kind} {} {} {} {} paras {}", f(r[0]), f(r[1]), f(r[2]), f(r[3]), ps.join(" ")).unwrap();
        }
        // Lines are numbered in paint order, text-box lines after the rest wherever FLOAT-16 paints their boxes. A line
        // without ink (a tab alone, the empty line after a break, a line of spaces) gets an empty glyph record at its
        // baseline and the end of what it holds, so it is listed with its x.
        let mut used = Vec::new();
        let mut boxed = std::collections::HashSet::new();
        let mut bare = Vec::new();
        let mut objects = Vec::new();
        paint_lines(
            page,
            &mut |p| {
                if matches!(p.item, Item::Glyphs(_)) {
                    used.push(p.line);
                }
            },
            &mut |line, para, placed, dx, dy| {
                if para.is_some_and(|p| in_box.contains(&p.0)) {
                    boxed.insert(line);
                }
                // An inline picture (one with a caret stop at its left edge; a float drawn with the line has none) shows
                // as U+FFFC there, on its line's baseline.
                for it in placed.line.items.iter() {
                    let rect = match it {
                        Item::Image { rect, .. } => rect,
                        Item::Shape(s) if s.float == 0 => &s.rect,
                        _ => continue,
                    };
                    if placed.line.atoms.iter().any(|a| !a.brk && (a.x0 - rect[0]).abs() < 0.01) {
                        objects.push((line, dx + placed.x + rect[0], dy + placed.y + placed.line.baseline, para.map_or(0, |p| p.0)));
                    }
                }
                // A line without ink: nothing drawn, or only spaces. Its x is where its spaces, tabs and breaks end.
                let blank = |it: &Item| matches!(it, Item::Glyphs(r) if r.chars.iter().all(|c| c.0.is_whitespace()));
                if placed.line.items.iter().all(blank) {
                    let runs = placed.line.items.iter().filter_map(|it| match it {
                        // An empty run marks the caret: it ends where it starts.
                        Item::Glyphs(r) => Some(r.glyphs.last().map_or(r.x, |&(g, x)| x + r.face.advance(g) * r.face.scale(r.size))),
                        _ => None,
                    });
                    let end = placed.line.atoms.iter().map(|a| a.x1).chain(runs).fold(0.0, f64::max);
                    bare.push((line, dx + placed.x + end, dy + placed.y + placed.line.baseline, para.map_or(0, |p| p.0)));
                }
            },
        );
        used.extend(bare.iter().chain(&objects).map(|b| b.0));
        used.sort_unstable_by_key(|l| (boxed.contains(l), *l));
        used.dedup();
        let order: std::collections::HashMap<u32, usize> = used.iter().enumerate().map(|(i, l)| (*l, i)).collect();
        let number = |line: u32| base + order[&line];
        paint_lines(
            page,
            &mut |p| match p.absolute() {
                Item::Glyphs(r) => {
                    let line = number(p.line);
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
                Item::Turned(t) => writeln!(s, "T turned {} about {} {} items {}", f(t.deg), f(t.centre[0]), f(t.centre[1]), t.items.len()).unwrap(),
                Item::Shape(sh) => {
                    let b = sh.rect;
                    writeln!(s, "S {} {} {} {} rot {} paths {}", f(b[0]), f(b[1]), f(b[2]), f(b[3]), f(sh.rot), sh.geom.paths.len()).unwrap()
                }
            },
            &mut |_, _, _, _, _| {},
        );
        for &(line, x, y, para) in &objects {
            writeln!(
                s,
                "G - size 0.000 y {} x {} color None para {} line {} glyphs [] chars \"\\u{{fffc}}\" cx [{}] src []",
                f(y),
                f(x),
                id(para),
                number(line),
                f(x)
            )
            .unwrap();
        }
        for &(line, x, y, para) in &bare {
            writeln!(s, "G - size 0.000 y {} x {} color None para {} line {} glyphs [] chars \"\" cx [] src []", f(y), f(x), id(para), number(line))
                .unwrap();
        }
        base += used.len();
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
