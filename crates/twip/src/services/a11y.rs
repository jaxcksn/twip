//! The accessibility surface: the reading tree (headings, paragraphs and pictures, and tables with their rows and
//! cells at every nesting level; each paragraph's text as the markup view shows it, marked with its insertions,
//! deletions, commented ranges and misspellings, and naming their authors) and what to announce when the caret
//! enters, leaves or moves through a table, or enters review marks.

use crate::api::services::{AccessibleMark, AccessibleNode, AccessibleRole};
use crate::docx::review::{AuthorId, CommentId, MarkupView, RangeKind};
use crate::docx::{Block, Document, Inline, Paragraph};
use crate::editing::Editor;
use crate::editing::text::{OBJECT, UnitText};
use crate::layout::{ParaId, TableId, Unit};

use super::spelling::Spellers;

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

    fn mark(self) -> AccessibleMark {
        match self {
            ReviewKind::Insertion => AccessibleMark::Insertion,
            ReviewKind::Deletion => AccessibleMark::Deletion,
            ReviewKind::Comment => AccessibleMark::Comment,
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

/// The review marks of the body paragraph `para`, with the comments open before it followed from the start.
fn marks_of(doc: &Document, para: ParaId) -> Vec<MarkSpan> {
    fn walk(doc: &Document, list: &[Block], para: ParaId, open: &mut Vec<CommentId>) -> Option<Vec<MarkSpan>> {
        for b in list {
            match b {
                Block::Para(p) => {
                    let spans = para_marks(doc, p, doc.review.view, open);
                    if p.id == para.0 {
                        return Some(spans);
                    }
                }
                Block::Table(t) => {
                    if let Some(s) = t.rows.iter().flat_map(|r| &r.cells).find_map(|c| walk(doc, &c.blocks, para, open)) {
                        return Some(s);
                    }
                }
            }
        }
        None
    }
    let mut open = Vec::new();
    doc.sections.iter().find_map(|s| walk(doc, &s.blocks, para, &mut open)).unwrap_or_default()
}

/// The review marks on the unit after the caret.
fn caret_marks(e: &Editor) -> Vec<ReviewMark> {
    let Some((para, u)) = e.selection().focus().and_then(|p| p.para_offset()) else { return Vec::new() };
    // shortcut: the body's marks are found again per announcement, up to the caret's paragraph; keep them with the
    // layout if it shows in typing profiles.
    marks_of(e.document(), para).into_iter().filter(|s| s.0 <= u.0 && u.0 < s.1).map(|s| s.2).collect()
}

fn author(doc: &Document, id: AuthorId) -> &str {
    doc.people.get(id).map_or("", |p| &p.name)
}

/// The document's reading tree. `spelling` marks the paragraphs with misspellings (checked in `language` where
/// their runs name none).
pub fn tree(e: &Editor, spelling: &Spellers, language: &str) -> AccessibleNode {
    let doc = e.document();
    let mut open = Vec::new();
    let mut children = Vec::new();
    for s in &doc.sections {
        blocks(e, &s.blocks, 0, &mut open, (spelling, language), &mut children);
    }
    AccessibleNode { role: AccessibleRole::Document, children, ..AccessibleNode::default() }
}

/// The reading nodes of `list`: headings and paragraphs with their text and pictures, tables with rows and cells.
fn blocks(e: &Editor, list: &[Block], depth: usize, open: &mut Vec<CommentId>, spelling: (&Spellers, &str), out: &mut Vec<AccessibleNode>) {
    let doc = e.document();
    for b in list {
        match b {
            Block::Para(p) => {
                let spans = para_marks(doc, p, doc.review.view, open);
                if let Some(node) = paragraph(e, p, &spans, spelling) {
                    out.push(node);
                }
            }
            Block::Table(tb) => {
                let level = depth + 1;
                let rows = tb
                    .rows
                    .iter()
                    .map(|r| {
                        let cells = r
                            .cells
                            .iter()
                            .map(|c| {
                                let mut kids = Vec::new();
                                blocks(e, &c.blocks, level, open, spelling, &mut kids);
                                let name = r.header.then(|| "Column header".to_string());
                                AccessibleNode { role: AccessibleRole::Cell, name, children: kids, ..AccessibleNode::default() }
                            })
                            .collect();
                        AccessibleNode { role: AccessibleRole::Row, children: cells, ..AccessibleNode::default() }
                    })
                    .collect();
                let name = if level > 1 { format!("Table, level {level}") } else { "Table".into() };
                out.push(AccessibleNode {
                    role: AccessibleRole::Table,
                    name: Some(name),
                    level: Some(level as u32),
                    children: rows,
                    ..Default::default()
                });
            }
        }
    }
}

/// A paragraph's node, or none when it shows no text and no pictures. Its marks are named with their authors
/// (`insertion by Ada, comment by Grace`).
fn paragraph(e: &Editor, p: &Paragraph, spans: &[MarkSpan], (spelling, language): (&Spellers, &str)) -> Option<AccessibleNode> {
    let doc = e.document();
    let t = UnitText::of_view(p, doc.review.view);
    let text: String = t
        .text
        .chars()
        .enumerate()
        .filter(|&(_, c)| c != OBJECT)
        .map(|(u, c)| {
            if t.is_hidden(Unit(u as u32)) {
                ' '
            } else if matches!(c, '\u{2028}' | '\u{c}') {
                '\n'
            } else {
                c
            }
        })
        .collect();
    let text = text.trim_end().to_string();
    let pictures: Vec<AccessibleNode> = p
        .inlines
        .iter()
        .filter_map(|i| if let Inline::Picture(pic, _) = i { Some(pic) } else { None })
        .map(|pic| AccessibleNode { role: AccessibleRole::Image, name: Some(pic.alt.to_string()).filter(|a| !a.is_empty()), ..Default::default() })
        .collect();
    if text.trim().is_empty() && pictures.is_empty() {
        return None;
    }
    let mut marks: Vec<AccessibleMark> = Vec::new();
    let mut said: Vec<String> = Vec::new();
    for s in spans {
        if !marks.contains(&s.2.kind.mark()) {
            marks.push(s.2.kind.mark());
        }
        let line = format!("{} by {}", s.2.kind.name(), author(doc, s.2.author));
        if !said.contains(&line) {
            said.push(line);
        }
    }
    if spelling.errors(e, ParaId(p.id), language).is_some_and(|(errs, _)| !errs.is_empty()) {
        marks.push(AccessibleMark::Misspelling);
    }
    let (role, level) = match p.style.outline_lvl.filter(|l| (0..9).contains(l)) {
        Some(l) => (AccessibleRole::Heading, Some(l as u32 + 1)),
        None => (AccessibleRole::Paragraph, None),
    };
    Some(AccessibleNode {
        role,
        // shortcut: the node names its marks' authors; mark spans within the text need a text-run node in the tree.
        name: (!said.is_empty()).then(|| said.join(", ")),
        text: (!text.is_empty()).then_some(text),
        level,
        marks: (!marks.is_empty()).then_some(marks),
        children: pictures,
    })
}

/// What the caret's moves call for saying: it entered, left or moved through a table, or entered review marks.
#[derive(Clone, Debug, Default)]
pub struct Announcer {
    last: CellContext,
}

impl Announcer {
    /// The announcement the caret's context calls for since the last call ("Row 2, column 1", "insertion by Ada"),
    /// if any.
    pub fn next(&mut self, e: &mut Editor) -> Option<String> {
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
        let was = std::mem::replace(&mut self.last, now.clone());
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font::Fonts;

    fn find(n: &AccessibleNode, role: AccessibleRole, out: &mut Vec<AccessibleNode>) {
        if n.role == role {
            out.push(n.clone());
        }
        n.children.iter().for_each(|c| find(c, role, out));
    }

    #[test]
    fn tree_has_tables_and_cells_announce() {
        let fonts = Fonts::system();
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/FX-NEST-DEEP.docx");
        let mut e = Editor::open(&std::fs::read(path).expect("fixture"), &fonts).expect("opens");
        let t = tree(&e, &Spellers::default(), "en-US");
        let mut tables = Vec::new();
        find(&t, AccessibleRole::Table, &mut tables);
        assert!(tables.len() >= 2, "tables at every level");
        assert!(tables.iter().any(|t| t.level > Some(1)), "a nested table has its level");
        let mut rows = Vec::new();
        find(&t, AccessibleRole::Row, &mut rows);
        assert!(!rows.is_empty());
        // Into the first table: its size and the cell.
        let mut a = Announcer::default();
        a.next(&mut e);
        let mut moved = false;
        for _ in 0..40 {
            let down = e.key_command("ArrowDown", crate::editing::keymap::Mods::default()).expect("a command");
            e.execute(down, &fonts);
            if let Some(said) = a.next(&mut e) {
                assert!(said.contains("Row ") && said.contains("column "), "{said}");
                moved = true;
                break;
            }
        }
        assert!(moved, "entering a table announces its cell");
    }
}
