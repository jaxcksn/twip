//! The accessibility surface: the reading tree the host mirrors into its accessibility DOM (headings,
//! paragraphs, and tables with their rows and cells, header cells marked, at every nesting level; each line's text as
//! the markup view shows it, with its insertions, deletions and commented ranges and their authors) and the context
//! announced when the caret enters, leaves or moves through a table, or enters review marks.

use std::collections::HashMap;

use serde_json::{Value, json};

use super::{Engine, r1};
use crate::docx::review::{AuthorId, CommentId, MarkupView, RangeKind};
use crate::docx::{Block, Document, Inline, Paragraph};
use crate::editing::Editor;
use crate::editing::text::UnitText;
use crate::layout::{ParaId, TableId};

/// Where the caret's context was last announced: its table, row and grid column, and the nesting level, and the
/// review marks on the text after it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CellContext {
    pub table: Option<TableId>,
    pub row: usize,
    pub col: usize,
    pub depth: usize,
    pub review: Vec<ReviewMark>,
}

/// What a review mark says its text is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReviewKind {
    Insertion,
    Deletion,
    Comment,
}

impl ReviewKind {
    fn name(self) -> &'static str {
        match self {
            ReviewKind::Insertion => "insertion",
            ReviewKind::Deletion => "deletion",
            ReviewKind::Comment => "comment",
        }
    }
}

/// An insertion, a deletion or a commented range, and whose it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReviewMark {
    pub kind: ReviewKind,
    pub author: AuthorId,
}

/// A run of units carrying one review mark: (start, end, mark).
type MarkSpan = (u32, u32, ReviewMark);

/// The review marks of `p`'s units that `view` shows. `open`: the comments whose ranges are open at its start, kept
/// up to date for the next paragraph.
fn para_marks(doc: &Document, p: &Paragraph, view: MarkupView, open: &mut Vec<CommentId>) -> Vec<MarkSpan> {
    let mut spans: Vec<MarkSpan> = Vec::new();
    let mut at = 0u32;
    for i in &p.inlines {
        match i {
            Inline::Range(m) if m.kind == RangeKind::CommentStart => open.push(CommentId(m.id)),
            Inline::Range(m) if m.kind == RangeKind::CommentEnd => open.retain(|c| c.0 != m.id),
            _ => {}
        }
        let n = i.units() as u32;
        let rev = i.run().map(|r| &r.rev);
        if n == 0 || rev.is_some_and(|r| view.hides(r)) {
            at += n;
            continue;
        }
        let revs = rev
            .into_iter()
            .flat_map(|r| [r.ins.as_ref().map(|m| (ReviewKind::Insertion, m.author)), r.del.as_ref().map(|m| (ReviewKind::Deletion, m.author))]);
        let comments = open.iter().filter_map(|c| doc.comments.by_id.get(&doc.comments.root(*c))).map(|c| Some((ReviewKind::Comment, c.author)));
        for (kind, author) in revs.chain(comments).flatten() {
            let mark = ReviewMark { kind, author };
            match spans.iter_mut().rev().find(|s| s.2 == mark && s.1 == at) {
                Some(s) => s.1 = at + n,
                None => spans.push((at, at + n, mark)),
            }
        }
        at += n;
    }
    spans
}

/// The review marks of the body's paragraphs, by paragraph (those with none left out).
fn review_marks(doc: &Document) -> HashMap<ParaId, Vec<MarkSpan>> {
    fn walk(doc: &Document, list: &[Block], open: &mut Vec<CommentId>, out: &mut HashMap<ParaId, Vec<MarkSpan>>) {
        for b in list {
            match b {
                Block::Para(p) => {
                    let spans = para_marks(doc, p, doc.review.view, open);
                    if !spans.is_empty() {
                        out.insert(ParaId(p.id), spans);
                    }
                }
                Block::Table(t) => t.rows.iter().flat_map(|r| &r.cells).for_each(|c| walk(doc, &c.blocks, open, out)),
            }
        }
    }
    let (mut open, mut out) = (Vec::new(), HashMap::new());
    for s in &doc.sections {
        walk(doc, &s.blocks, &mut open, &mut out);
    }
    out
}

/// The review marks on the unit after the caret.
fn caret_marks(e: &Editor) -> Vec<ReviewMark> {
    let Some((para, u)) = e.selection().focus().and_then(|p| p.para_offset()) else { return Vec::new() };
    // shortcut: the body's marks are found again per announcement; keep them with the layout if it shows in typing
    // profiles.
    let spans = review_marks(e.document()).remove(&para).unwrap_or_default();
    spans.into_iter().filter(|s| s.0 <= u.0 && u.0 < s.1).map(|s| s.2).collect()
}

fn author(doc: &Document, id: AuthorId) -> &str {
    doc.people.get(id).map_or("", |p| &p.name)
}

impl Engine {
    /// The reading tree as JSON: `{r: role, l: level, a: label, t: [lines], c: [children]}`; each line is its box in
    /// content CSS pixels, its text as the markup view shows it (hidden text as spaces) and its review marks
    /// `m: [{k: insertion | deletion | comment, a: author, s, e}]` (`s..e` in the line's characters).
    pub fn accessible_tree(&mut self) -> String {
        let k = self.px_per_pt();
        let Some(e) = &mut self.editor else { return "{\"r\":\"document\",\"l\":0,\"c\":[],\"t\":[]}".into() };
        let stack = e.stack();
        let marks = review_marks(e.document());
        // Every laid-out body line, by paragraph.
        let mut lines: HashMap<ParaId, Vec<Value>> = HashMap::new();
        for (pi, geo) in e.page_geometries() {
            // Each paragraph's part of each line (a line shows several where the view joins paragraphs).
            for (l, para) in geo.lines.iter().filter(|l| !l.repeat).flat_map(|l| l.paras.iter().map(move |&p| (l, p))) {
                if geo.stories[l.story].story != crate::editing::position::StoryId::Body {
                    continue;
                }
                let doc = e.document();
                let (Some(p), Some(first), Some(last), Some((x0, x1))) =
                    (e.index().para(doc, para), l.first_of(para), l.last_of(para), l.span_of(para))
                else {
                    continue;
                };
                let (a, b) = (first.0, last.0.max(first.0));
                let t = UnitText::of_view(p, doc.review.view);
                let s: String = t.text.chars().skip(a as usize).take((b - a) as usize).collect();
                if s.trim().is_empty() {
                    continue;
                }
                let Some((x, y)) = stack.to_doc(pi, x0, l.top) else { continue };
                let m: Vec<Value> = marks
                    .get(&para)
                    .into_iter()
                    .flatten()
                    .filter(|m| m.0 < b && m.1 > a)
                    .map(|m| json!({ "k": m.2.kind.name(), "a": author(doc, m.2.author), "s": m.0.max(a) - a, "e": m.1.min(b) - a }))
                    .collect();
                lines
                    .entry(para)
                    .or_default()
                    .push(json!({ "x": r1(x * k), "y": r1(y * k), "w": r1((x1 - x0) * k), "h": r1((l.bottom - l.top) * k), "s": s, "m": m }));
            }
        }
        let doc = e.document();
        let mut children = Vec::new();
        for s in &doc.sections {
            blocks(doc, &s.blocks, 0, &mut lines, &mut children);
        }
        json!({ "r": "document", "l": 0, "t": [], "c": children }).to_string()
    }

    /// The announcement the last input calls for (the caret's cell context changed, or it entered review marks:
    /// "insertion by X", "deletion by X", "comment by X"), if any.
    pub fn take_announcement(&mut self) -> Option<String> {
        let e = self.editor.as_mut()?;
        let f = e.format_state()?;
        let review = caret_marks(e);
        let now = match (f.table, f.cell) {
            (Some(t), Some((row, cell))) => {
                let depth = e.selection().focus().map_or(0, |p| e.index().chain_of(&p).len());
                let col = e
                    .index()
                    .table(e.document(), t)
                    .and_then(|tb| crate::editing::table::Grid::of(tb).rows.get(row)?.get(cell).map(|c| c.0))
                    .unwrap_or(cell);
                CellContext { table: Some(t), row, col, depth, review }
            }
            _ => CellContext { review, ..CellContext::default() },
        };
        let was = std::mem::replace(&mut self.cell_context, now.clone());
        let mut parts = Vec::new();
        if (was.table, was.row, was.col, was.depth) != (now.table, now.row, now.col, now.depth) {
            match now.table.and_then(|t| e.index().table(e.document(), t)) {
                None if was.table.is_some() && now.table.is_none() => parts.push("Left table".to_string()),
                None => {}
                Some(tb) => {
                    if was.table != now.table {
                        let cols = crate::editing::table::Grid::of(tb).cols;
                        parts.push(format!("Table, {} rows, {cols} columns", tb.rows.len()));
                        if now.depth > 1 {
                            parts.push(format!("nested, level {}", now.depth));
                        }
                    }
                    parts.push(format!("Row {}, column {}", now.row + 1, now.col + 1));
                    // The column's header cell (a header row's cell over this column), when the caret isn't in it.
                    if tb.rows.first().is_some_and(|r| r.header) && now.row > 0 {
                        let g = crate::editing::table::Grid::of(tb);
                        let header = g.rows.first().and_then(|cells| cells.iter().position(|&(c, s, _)| c <= now.col && now.col < c + s.max(1)));
                        if let Some(text) = header.and_then(|k| tb.rows[0].cells.get(k)).map(cell_text).filter(|s| !s.is_empty()) {
                            parts.push(text);
                        }
                    }
                }
            }
        }
        if was.review != now.review {
            let doc = e.document();
            parts.extend(now.review.iter().filter(|m| !was.review.contains(m)).map(|m| format!("{} by {}", m.kind.name(), author(doc, m.author))));
        }
        (!parts.is_empty()).then(|| parts.join(", "))
    }
}

/// A cell's text (its paragraphs, spaced).
fn cell_text(c: &crate::docx::Cell) -> String {
    c.blocks
        .iter()
        .filter_map(|b| if let Block::Para(p) = b { Some(crate::docx::fields::text(p)) } else { None })
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

/// The reading nodes of `blocks`: headings and paragraphs with their lines, tables with rows and cells.
fn blocks(_doc: &Document, list: &[Block], depth: usize, lines: &mut HashMap<ParaId, Vec<Value>>, out: &mut Vec<Value>) {
    for b in list {
        match b {
            Block::Para(p) => {
                let Some(t) = lines.remove(&ParaId(p.id)) else { continue };
                match p.style.outline_lvl.filter(|l| (0..9).contains(l)) {
                    Some(l) => out.push(json!({ "r": "heading", "l": l + 1, "t": t, "c": [] })),
                    None => out.push(json!({ "r": "paragraph", "l": 0, "t": t, "c": [] })),
                }
            }
            Block::Table(tb) => {
                let level = depth + 1;
                let mut rows = Vec::new();
                for r in &tb.rows {
                    let mut cells = Vec::new();
                    for c in &r.cells {
                        let mut kids = Vec::new();
                        blocks(_doc, &c.blocks, level, lines, &mut kids);
                        cells.push(json!({ "r": if r.header { "columnheader" } else { "cell" }, "l": level, "t": [], "c": kids }));
                    }
                    rows.push(json!({ "r": "row", "l": level, "t": [], "c": cells }));
                }
                let label = if level > 1 { format!("Table, level {level}") } else { "Table".into() };
                out.push(json!({ "r": "table", "l": level, "a": label, "t": [], "c": rows }));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font::Fonts;

    #[test]
    fn tree_has_tables_and_cells_announce() {
        let mut e = Engine::with_fonts(Fonts::system());
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/FX-NEST-DEEP.docx");
        e.open(&std::fs::read(path).unwrap());
        let tree: Value = serde_json::from_str(&e.accessible_tree()).unwrap();
        fn find<'a>(v: &'a Value, role: &str, out: &mut Vec<&'a Value>) {
            if v["r"] == role {
                out.push(v);
            }
            for c in v["c"].as_array().into_iter().flatten() {
                find(c, role, out);
            }
        }
        let mut tables = Vec::new();
        find(&tree, "table", &mut tables);
        assert!(tables.len() >= 2, "tables at every level");
        assert!(tables.iter().any(|t| t["l"].as_u64() > Some(1)), "a nested table has its level");
        let mut rows = Vec::new();
        find(&tree, "row", &mut rows);
        assert!(!rows.is_empty());
        // Into the first table: its size and the cell; then the next cell.
        e.take_announcement();
        e.command("table.select.cell", None);
        let mut moved = false;
        for _ in 0..40 {
            e.key("ArrowDown", false, false, false);
            if let Some(a) = e.take_announcement() {
                assert!(a.contains("Row ") && a.contains("column "), "{a}");
                moved = true;
                break;
            }
        }
        assert!(moved, "entering a table announces its cell");
    }
}
