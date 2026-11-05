//! What to announce as the caret moves: entering, leaving or moving through a table; entering or leaving a list, a
//! header, footer, note or text box, or read-only content; another page; review marks and comments; a misspelled word;
//! the whole story selected.

use crate::api::services::AnnouncementKind;
use crate::docx::Block;
use crate::editing::Editor;
use crate::editing::position::StoryId;
use crate::layout::{ParaId, TableId};

use super::super::spelling::Spellers;
use super::tree::heading_level;
use super::{ReviewKind, ReviewMark, author, marks_of};

/// Where the caret's context was last announced.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CellContext {
    /// Its table, row and grid column, and the nesting level.
    pub table: Option<TableId>,
    pub row: usize,
    pub col: usize,
    pub depth: usize,
    /// The review marks and comment ranges on the text after it.
    pub review: Vec<ReviewMark>,
    /// The list its paragraph is in (the `w:abstractNum`) and that paragraph's level.
    pub list: Option<(i64, i64)>,
    /// Its page, from 0.
    pub page: Option<usize>,
    /// Its story.
    pub story: Option<StoryId>,
    /// Text can't go in there.
    pub read_only: bool,
    /// The misspelled word it is on: its paragraph and where the word starts.
    pub misspelled: Option<(ParaId, usize)>,
    /// The selection is all of its story.
    pub all: bool,
}

/// The review marks on the unit after the caret.
fn caret_marks(e: &Editor) -> Vec<ReviewMark> {
    let Some((para, u)) = e.selection().focus().and_then(|p| p.para_offset()) else { return Vec::new() };
    // shortcut: the body's marks are found again per announcement, up to the caret's paragraph; keep them with the
    // layout if it shows in typing profiles.
    marks_of(e.document(), para).into_iter().filter(|s| s.0 <= u.0 && u.0 < s.1).map(|s| s.2).collect()
}

/// The caret paragraph's list and level, and how many items at its level the list has around it (in its container,
/// without a paragraph that isn't an item between).
fn caret_list(e: &Editor) -> Option<((i64, i64), usize)> {
    let (para, _) = e.selection().focus()?.para_offset()?;
    let p = e.index().para(e.document(), para)?;
    let (list, level) = (p.style.list?, p.style.num?.1);
    if heading_level(p).is_some() {
        return None;
    }
    let slot = e.index().para_slot(para)?;
    let blocks = e.index().blocks(e.document(), slot.container)?;
    // Items of any list run on (a nested list is inside the item before it); those of this list at this level count.
    let item = |b: &Block| match b {
        Block::Para(q) if q.style.num.is_some() && heading_level(q).is_none() => Some((q.style.list, q.style.num.map_or(0, |n| n.1))),
        _ => None,
    };
    let before = blocks[..slot.block].iter().rev().map_while(item);
    let after = blocks[slot.block..].iter().map_while(item);
    let count = before.chain(after).filter(|&(l, n)| l == Some(list) && n == level).count();
    Some(((list, level), count))
}

/// A story's name, as the caret enters it.
fn story_name(e: &Editor, s: StoryId) -> &'static str {
    match s {
        StoryId::Body => "Body",
        StoryId::HeaderFooter(i) => match e.document().stories.keys().nth(i as usize) {
            Some(name) if name.contains("footer") => "Footer",
            _ => "Header",
        },
        StoryId::Footnote(_) => "Footnote",
        StoryId::Endnote(_) => "Endnote",
        StoryId::TextBox(_) => "Text box",
    }
}

/// What the caret's moves call for saying.
#[derive(Clone, Debug, Default)]
pub struct Announcer {
    last: CellContext,
    /// The first context was seen: until then there is nothing to compare with, and nothing is said.
    primed: bool,
}

impl Announcer {
    /// What the caret's context calls for saying since the last call, in order, each with what it is about
    /// ("Row 2, column 1": table, "insertion by Ada": review). `spelling` and `language` find misspellings;
    /// `read_only`: the session takes no edits.
    pub fn next(&mut self, e: &mut Editor, spelling: &Spellers, language: &str, read_only: bool) -> Vec<(AnnouncementKind, String)> {
        let Some(f) = e.format_state() else { return Vec::new() };
        let mut now = match (f.table, f.cell) {
            (Some(t), Some((row, cell))) => {
                let depth = e.selection().focus().map_or(0, |p| e.index().chain_of(&p).len());
                let col = e
                    .index()
                    .table(e.document(), t)
                    .and_then(|tb| crate::editing::table::Grid::of(tb).rows.get(row)?.get(cell).map(|c| c.0))
                    .unwrap_or(cell);
                CellContext { table: Some(t), row, col, depth, ..CellContext::default() }
            }
            _ => CellContext::default(),
        };
        now.review = caret_marks(e);
        let list = caret_list(e);
        now.list = list.map(|l| l.0);
        now.page = e.caret_view().map(|c| c.page);
        now.story = e.selection().focus().map(|p| p.story);
        now.read_only = read_only || !e.caret_editable();
        now.misspelled = spelling.at_selection(e, language).map(|m| (m.para, m.error.start));
        now.all = e.selects_story();
        let was = std::mem::replace(&mut self.last, now.clone());
        if !std::mem::replace(&mut self.primed, true) {
            return Vec::new();
        }

        let mut said = Vec::new();
        if was.story != now.story
            && let Some(s) = now.story
        {
            said.push((AnnouncementKind::Mode, story_name(e, s).to_string()));
        }
        if was.page != now.page
            && let (Some(_), Some(p)) = (was.page, now.page)
        {
            said.push((AnnouncementKind::Page, format!("Page {}", p + 1)));
        }
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
        if !parts.is_empty() {
            said.push((AnnouncementKind::Table, parts.join(", ")));
        }
        match (was.list, list) {
            (w, Some((l, count))) if w.map(|w| w.0) != Some(l.0) => said.push((AnnouncementKind::List, format!("List, {count} items"))),
            (Some(_), None) => said.push((AnnouncementKind::List, "Out of list".into())),
            _ => {}
        }
        if now.read_only && !was.read_only {
            said.push((AnnouncementKind::ReadOnly, "Read only".into()));
        }
        if was.review != now.review {
            let doc = e.document();
            let entered: Vec<&ReviewMark> = now.review.iter().filter(|m| !was.review.contains(m)).collect();
            let marks: Vec<String> = entered
                .iter()
                .filter(|m| !matches!(m.kind, ReviewKind::Comment(_)))
                .map(|m| format!("{} by {}", m.kind.name(), author(doc, m.author)))
                .collect();
            if !marks.is_empty() {
                said.push((AnnouncementKind::Review, marks.join(", ")));
            }
            for m in &entered {
                let ReviewKind::Comment(c) = m.kind else { continue };
                let text = doc.comments.by_id.get(&c).map(|c| comment_text(&c.body)).unwrap_or_default();
                let who = author(doc, m.author);
                said.push((
                    AnnouncementKind::Comment,
                    if text.is_empty() { format!("Comment by {who}") } else { format!("Comment by {who}: {text}") },
                ));
            }
        }
        if now.misspelled.is_some() && now.misspelled != was.misspelled {
            said.push((AnnouncementKind::Spelling, "Misspelled".into()));
        }
        if now.all && !was.all {
            said.push((AnnouncementKind::Selection, "Selected all".into()));
        }
        said
    }
}

/// A comment's text: its paragraphs, spaced.
fn comment_text(body: &[Block]) -> String {
    body.iter()
        .filter_map(|b| if let Block::Para(p) = b { Some(crate::docx::fields::text(p)) } else { None })
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
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
    use crate::api::services::{AccessibleNode, AccessibleRole};
    use crate::font::Fonts;
    use crate::services::a11y::tree;
    use crate::services::spelling::Spellers;

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
        a.next(&mut e, &Spellers::default(), "en-US", false);
        let mut moved = false;
        for _ in 0..40 {
            let down = e.key_command("ArrowDown", crate::editing::keymap::Mods::default()).expect("a command");
            e.execute(down, &fonts);
            if let Some((_, said)) = a.next(&mut e, &Spellers::default(), "en-US", false).into_iter().find(|s| s.0 == AnnouncementKind::Table) {
                assert!(said.contains("Row ") && said.contains("column "), "{said}");
                moved = true;
                break;
            }
        }
        assert!(moved, "entering a table announces its cell");
    }

    fn open(name: &str) -> (Editor, Fonts) {
        let fonts = Fonts::system();
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor").join(name);
        let e = Editor::open(&std::fs::read(path).expect("fixture"), &fonts).expect("opens");
        (e, fonts)
    }

    fn spellers() -> Spellers {
        let d = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/dictionaries");
        let mut s = Spellers::default();
        if let (Ok(aff), Ok(dic)) = (std::fs::read(d.join("en_US.aff")), std::fs::read(d.join("en_US.dic"))) {
            s.add_dictionary("en-US", &aff, &dic).expect("loads");
        }
        s
    }

    /// The paragraph (anywhere in the tree) whose text has `text`, and where it starts in it.
    fn place(e: &Editor, text: &str) -> (ParaId, u32) {
        fn walk(n: &AccessibleNode, text: &str) -> Option<(ParaId, u32)> {
            if matches!(n.role, AccessibleRole::Paragraph | AccessibleRole::Heading | AccessibleRole::ListItem) {
                let p: String = n.children.iter().flat_map(|c| std::iter::once(c).chain(&c.children)).filter_map(|c| c.text.as_deref()).collect();
                if let Some(at) = p.find(text) {
                    return Some((ParaId(n.id.0), p[..at].chars().count() as u32));
                }
            }
            n.children.iter().find_map(|c| walk(c, text))
        }
        walk(&tree(e, &Spellers::default(), "en-US"), text).expect("the text")
    }

    /// Moves the caret into `text` (`into` characters in) and says what that calls for.
    fn go(a: &mut Announcer, e: &mut Editor, fonts: &Fonts, s: &Spellers, text: &str, into: u32) -> Vec<(AnnouncementKind, String)> {
        let (para, at) = place(e, text);
        e.select_range(para, crate::layout::Unit(at + into), crate::layout::Unit(at + into), fonts);
        a.next(e, s, "en-US", false)
    }

    fn kinds(said: &[(AnnouncementKind, String)], kind: AnnouncementKind) -> Vec<&str> {
        said.iter().filter(|s| s.0 == kind).map(|s| s.1.as_str()).collect()
    }

    #[test]
    fn review_marks_comments_and_misspellings_are_said_once() {
        let (mut e, fonts) = open("FX-A11Y-REVIEW.docx");
        let s = spellers();
        let mut a = Announcer::default();
        go(&mut a, &mut e, &fonts, &s, "The board", 0);
        assert_eq!(kinds(&go(&mut a, &mut e, &fonts, &s, "unanimously", 1), AnnouncementKind::Review), ["insertion by Ada"]);
        assert!(kinds(&go(&mut a, &mut e, &fonts, &s, "unanimously", 3), AnnouncementKind::Review).is_empty(), "not again inside it");
        let comment = go(&mut a, &mut e, &fonts, &s, "publish the figures", 2);
        assert_eq!(kinds(&comment, AnnouncementKind::Comment), ["Comment by Grace: Please confirm the figures."]);
        if s.speller("en-US").is_some() {
            assert_eq!(kinds(&go(&mut a, &mut e, &fonts, &s, "recieve", 2), AnnouncementKind::Spelling), ["Misspelled"]);
        }
        // The last page: a page change.
        let page = go(&mut a, &mut e, &fonts, &s, "Appendix B", 0);
        assert!(kinds(&page, AnnouncementKind::Page).first().is_some_and(|p| p.starts_with("Page ")), "{page:?}");
    }

    #[test]
    fn lists_stories_and_select_all_are_said() {
        let (mut e, fonts) = open("FX-A11Y-STRUCTURE.docx");
        let s = Spellers::default();
        let mut a = Announcer::default();
        go(&mut a, &mut e, &fonts, &s, "Lists", 0);
        assert_eq!(kinds(&go(&mut a, &mut e, &fonts, &s, "First item", 0), AnnouncementKind::List), ["List, 3 items"]);
        assert!(kinds(&go(&mut a, &mut e, &fonts, &s, "Second item", 0), AnnouncementKind::List).is_empty(), "the same list");
        assert_eq!(kinds(&go(&mut a, &mut e, &fonts, &s, "Tables", 0), AnnouncementKind::List), ["Out of list"]);
        // Into a footnote and back.
        assert_eq!(kinds(&go(&mut a, &mut e, &fonts, &s, "The first footnote.", 0), AnnouncementKind::Mode), ["Footnote"]);
        assert_eq!(kinds(&go(&mut a, &mut e, &fonts, &s, "Last paragraph.", 0), AnnouncementKind::Mode), ["Body"]);
        e.execute(crate::api::command::Command::SelectAll, &fonts);
        assert_eq!(kinds(&a.next(&mut e, &s, "en-US", false), AnnouncementKind::Selection), ["Selected all"]);
        // A read-only session: entering it says so.
        assert_eq!(kinds(&a.next(&mut e, &s, "en-US", true), AnnouncementKind::ReadOnly), ["Read only"]);
    }
}
