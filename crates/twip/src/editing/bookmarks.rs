//! Bookmarks

use std::collections::HashSet;

use crate::docx::{Block, Inline, Paragraph};
use crate::font::Fonts;
use crate::layout::{ParaId, Unit};

use super::Editor;
use super::commands::Outcome;
use super::position::Position;
use super::selection::{Granularity, Selection};
use super::step::{Edges, Step};
use super::view::{FormattingMark, MarkKind};

/// A bookmark's start or end: its paragraph and unit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct End {
    pub para: ParaId,
    pub unit: Unit,
}

/// A bookmark as the document holds it, in document order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Found {
    pub name: String,
    pub start: End,
    pub end: Option<End>,
}

/// A name Word takes for a bookmark: a letter (or `_`, for hidden ones) first, then letters, digits and `_`, 40 at most.
pub fn valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_alphabetic() || c == '_') && chars.all(|c| c.is_alphanumeric() || c == '_') && name.chars().count() <= 40
}

/// The marker's bookmark name, and whether it is its end.
fn marker(i: &Inline) -> Option<(&str, bool)> {
    match i {
        Inline::Bookmark(n) => Some(n.strip_prefix('/').map_or((n.as_str(), false), |n| (n, true))),
        _ => None,
    }
}

/// The paragraph's inlines without `name`'s markers.
fn without(p: &Paragraph, name: &str) -> Option<Vec<Inline>> {
    let keep = |i: &Inline| !marker(i).is_some_and(|(n, _)| n.eq_ignore_ascii_case(name));
    (!p.inlines.iter().all(keep)).then(|| p.inlines.iter().filter(|i| keep(i)).cloned().collect())
}

/// Zero-unit markers at `u`, which an end inserted there goes after.
fn markers_at(p: &Paragraph, u: Unit) -> u32 {
    let mut at = 0u32;
    let mut n = 0;
    for i in &p.inlines {
        if at > u.0 {
            break;
        }
        if at == u.0 && i.units() == 0 && !matches!(i, Inline::Text(..)) {
            n += 1;
        }
        at += i.units() as u32;
    }
    n
}

impl Editor {
    /// Paragraphs in document order: the body's, then every other story's.
    fn paras_in_order(&self) -> Vec<ParaId> {
        fn walk(blocks: &[Block], out: &mut Vec<ParaId>) {
            for b in blocks {
                match b {
                    Block::Para(p) => out.push(ParaId(p.id)),
                    Block::Table(t) => t.rows.iter().flat_map(|r| r.cells.iter()).for_each(|c| walk(&c.blocks, out)),
                }
            }
        }
        let mut out = Vec::new();
        for s in &self.doc.sections {
            walk(&s.blocks, &mut out);
        }
        let body: HashSet<ParaId> = out.iter().copied().collect();
        let mut rest: Vec<ParaId> = self.index.para_ids().filter(|p| !body.contains(p)).collect();
        rest.sort_unstable();
        out.extend(rest);
        out
    }

    /// Every bookmark, in document order (a name read twice, the first).
    pub fn bookmarks(&self) -> Vec<Found> {
        let mut out: Vec<Found> = Vec::new();
        for para in self.paras_in_order() {
            let Some(p) = self.index.para(&self.doc, para) else { continue };
            let mut unit = 0u32;
            for i in &p.inlines {
                if let Some((name, end)) = marker(i) {
                    let at = End { para, unit: Unit(unit) };
                    match out.iter_mut().find(|f| f.name.eq_ignore_ascii_case(name)) {
                        Some(f) if end && f.end.is_none() => f.end = Some(at),
                        None if !end => out.push(Found { name: name.to_string(), start: at, end: None }),
                        _ => {}
                    }
                }
                unit += i.units() as u32;
            }
        }
        out
    }

    pub fn bookmark(&self, name: &str) -> Option<Found> {
        self.bookmarks().into_iter().find(|f| f.name.eq_ignore_ascii_case(name))
    }

    /// Bookmarks the selection: a bookmark of the same name moves here.
    pub(in crate::editing) fn insert_bookmark(&mut self, name: &str, fonts: &Fonts) -> Outcome {
        if !valid_name(name) {
            return Outcome::invalid("a bookmark name starts with a letter and holds letters, digits and underscores, 40 at most");
        }
        let Some((a, b)) = self.selection.ordered(&self.index) else { return Outcome::Unchanged("no selection") };
        let (Some((pa, oa)), Some((pb, ob))) = (a.para_offset(), b.para_offset()) else { return Outcome::unavailable("no text range") };
        let mut tx = self.begin("Bookmark");
        let gone: Vec<(ParaId, Vec<Inline>)> =
            self.index.para_ids().filter_map(|para| Some((para, without(self.index.para(&self.doc, para)?, name)?))).collect();
        for (para, inlines) in gone {
            if self.step(&mut tx, Step::Restyle { para, inlines }).is_err() {
                self.abort(tx);
                return Outcome::Unchanged("stale bookmark");
            }
        }
        let (start, end) = (Inline::Bookmark(name.to_string()), Inline::Bookmark(format!("/{name}")));
        let steps = if pa == pb && oa == ob {
            vec![Step::Text { para: pa, range: oa..oa, with: vec![start, end], edges: Edges::default() }]
        } else {
            let Some(under) = self.index.para(&self.doc, pb).map(|p| markers_at(p, ob)) else { return Outcome::Unchanged("no paragraph") };
            vec![
                Step::Text { para: pb, range: ob..ob, with: vec![end], edges: Edges { under, ..Edges::default() } },
                Step::Text { para: pa, range: oa..oa, with: vec![start], edges: Edges::default() },
            ]
        };
        for s in steps {
            if self.step(&mut tx, s).is_err() {
                self.abort(tx);
                return Outcome::Unchanged("stale selection");
            }
        }
        self.commit(tx, None, fonts)
    }

    pub(in crate::editing) fn delete_bookmark(&mut self, name: &str, fonts: &Fonts) -> Outcome {
        let gone: Vec<(ParaId, Vec<Inline>)> =
            self.index.para_ids().filter_map(|para| Some((para, without(self.index.para(&self.doc, para)?, name)?))).collect();
        if gone.is_empty() {
            return Outcome::invalid("no such bookmark");
        }
        let mut tx = self.begin("Delete Bookmark");
        for (para, inlines) in gone {
            if self.step(&mut tx, Step::Restyle { para, inlines }).is_err() {
                self.abort(tx);
                return Outcome::Unchanged("stale bookmark");
            }
        }
        self.commit(tx, None, fonts)
    }

    /// Selects a bookmark's text (Go To), or with `select` off (a link) puts the caret at its start; `_top`, unless a bookmark has the name, is the
    /// document's start.
    pub(in crate::editing) fn go_to_bookmark(&mut self, name: &str, select: bool, fonts: &Fonts) -> Outcome {
        let top = || self.paras_in_order().first().map(|&para| Found { name: name.into(), start: End { para, unit: Unit(0) }, end: None });
        let Some(f) = self.bookmark(name).or_else(|| if name.eq_ignore_ascii_case("_top") { top() } else { None }) else {
            return Outcome::Unchanged("no such bookmark");
        };
        let at = |e: End| self.index.para_slot(e.para).map(|s| Position::text(self.index.story(s.container), e.para, e.unit));
        let Some(start) = at(f.start) else { return Outcome::Unchanged("no such bookmark") };
        let end = f.end.and_then(at).filter(|e| e.story == start.story && self.index.cmp(&start, e).is_le()).unwrap_or(start);
        self.history.break_typing();
        self.set_selection(if !select || end == start { Selection::Caret(start) } else { Selection::range(start, end, Granularity::Char) });
        self.ensure(&start, fonts);
        self.jump_to_caret();
        Outcome::Moved
    }

    /// The shown bookmarks' brackets on `pages` (hidden bookmarks have none).
    pub fn bookmark_marks(&mut self, pages: std::ops::Range<usize>) -> Vec<FormattingMark> {
        let mut out = Vec::new();
        for page in pages {
            let Some(geo) = self.pages.page(page, &self.index) else { continue };
            for line in &geo.lines {
                for &para in line.paras.iter() {
                    let Some(p) = self.index.para(&self.doc, para) else { continue };
                    let mut unit = 0u32;
                    for i in &p.inlines {
                        if let Some(end) = marker(i).filter(|(n, _)| !n.starts_with('_')).map(|(_, end)| end) {
                            let kind = if end { MarkKind::BookmarkEnd } else { MarkKind::BookmarkStart };
                            if let Some(s) = line.stops.iter().find(|s| s.para == para && s.unit.0 == unit) {
                                out.push(FormattingMark { page, kind, at: [s.x, line.top, s.x, line.bottom] });
                            }
                        }
                        unit += i.units() as u32;
                    }
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use crate::editing::Editor;
    use crate::editing::commands::{Command, Outcome};
    use crate::editing::tests::{blank_body, fonts, para_id};
    use crate::layout::Unit;

    fn names(e: &Editor) -> Vec<String> {
        e.bookmarks().into_iter().map(|b| b.name).collect()
    }

    #[test]
    fn insert_move_go_to_and_delete() {
        let body = r#"<w:p><w:r><w:t>Alpha beta</w:t></w:r></w:p><w:bookmarkStart w:id="1" w:name="_Toc1"/><w:p><w:r><w:t>Gamma</w:t></w:r></w:p><w:bookmarkEnd w:id="1"/>"#;
        let mut e = Editor::open(&blank_body(body), fonts()).expect("opens");
        assert_eq!(names(&e), ["_Toc1"], "a bookmark between blocks is read");
        let (a, b) = (para_id(&e, 0), para_id(&e, 1));
        e.select_between((a, Unit(6)), (b, Unit(2)), fonts());
        assert!(matches!(e.execute(Command::InsertBookmark("Span".into()), fonts()), Outcome::Changed(_)));
        assert_eq!(names(&e), ["Span", "_Toc1"]);
        e.complete(fonts());
        let marks: Vec<_> = e.bookmark_marks(0..1).into_iter().map(|m| m.kind).collect();
        assert_eq!(marks, [super::MarkKind::BookmarkStart, super::MarkKind::BookmarkEnd], "the hidden one has no brackets");
        let f = e.bookmark("span").expect("found without case");
        assert_eq!((f.start.para, f.start.unit, f.end.map(|x| (x.para, x.unit))), (a, Unit(6), Some((b, Unit(2)))));
        // The same name moves.
        e.select_range(a, Unit(0), Unit(0), fonts());
        e.execute(Command::InsertBookmark("SPAN".into()), fonts());
        assert_eq!(names(&e), ["SPAN", "_Toc1"]);
        assert!(e.bookmark("Span").is_some_and(|f| f.end == Some(f.start)), "an empty bookmark");
        // Go To selects the text.
        e.select_range(b, Unit(0), Unit(0), fonts());
        e.execute(Command::GoToBookmark("_Toc1".into()), fonts());
        assert_eq!(e.selected_text(), "Gamma");
        assert!(matches!(e.execute(Command::DeleteBookmark("span".into()), fonts()), Outcome::Changed(_)));
        assert_eq!(names(&e), ["_Toc1"]);
        assert!(matches!(e.execute(Command::DeleteBookmark("Gone".into()), fonts()), Outcome::Refused(_)));
        assert!(matches!(e.execute(Command::InsertBookmark("1st".into()), fonts()), Outcome::Refused(_)));
        let saved = crate::docx::write::save(e.document()).expect("saves");
        let e = Editor::open(&saved, fonts()).expect("reopens");
        assert_eq!(names(&e), ["_Toc1"]);
    }
}
