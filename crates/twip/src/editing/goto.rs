//! Go To

use std::cmp::Ordering;
use std::collections::HashMap;

use crate::docx::{Block, FieldEdge, FieldView, Inline, fields};
use crate::font::Fonts;
use crate::layout::{FloatId, ParaId, Unit};

use super::Editor;
use super::bookmarks::End;
use super::commands::Outcome;
use super::find::Stories;
use super::position::{Position, StoryId};
use super::selection::{Granularity, ObjectRef, Selection};

/// What Go To goes to, as Word's Go To what lists it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GoToWhat {
    /// A page's first line.
    Page,
    /// A section's first paragraph.
    Section,
    /// A line of the body.
    Line,
    /// A bookmark, by name or in order.
    Bookmark,
    /// A comment thread, by its reviewer when named.
    Comment,
    /// A footnote's reference.
    Footnote,
    /// An endnote's reference.
    Endnote,
    /// A field, of a kind when named (its code's first word, `PAGE`).
    Field,
    /// A table's first cell.
    Table,
    /// An inline picture or a floating object.
    Graphic,
    /// An equation.
    Equation,
    /// An embedded object, of a kind when named (its program id, `Excel.Sheet`, or the start of one).
    Object,
    /// A paragraph with an outline level.
    Heading,
}

/// Which one: the n-th (from 1), n on from the current one (back, negative), or the next or previous.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GoToTarget {
    /// The n-th, from 1.
    Absolute(u32),
    /// n on (or back, negative) from the current one.
    Relative(i32),
    /// The next.
    Next,
    /// The previous.
    Previous,
}

impl From<crate::api::command::GoToWhat> for GoToWhat {
    fn from(w: crate::api::command::GoToWhat) -> GoToWhat {
        use crate::api::command::GoToWhat as W;
        match w {
            W::Page => GoToWhat::Page,
            W::Section => GoToWhat::Section,
            W::Line => GoToWhat::Line,
            W::Bookmark => GoToWhat::Bookmark,
            W::Comment => GoToWhat::Comment,
            W::Footnote => GoToWhat::Footnote,
            W::Endnote => GoToWhat::Endnote,
            W::Field => GoToWhat::Field,
            W::Table => GoToWhat::Table,
            W::Graphic => GoToWhat::Graphic,
            W::Equation => GoToWhat::Equation,
            W::Object => GoToWhat::Object,
            W::Heading => GoToWhat::Heading,
        }
    }
}

impl From<crate::api::command::GoToTarget> for GoToTarget {
    fn from(t: crate::api::command::GoToTarget) -> GoToTarget {
        use crate::api::command::GoToTarget as T;
        match t {
            T::Absolute(n) => GoToTarget::Absolute(n),
            T::Relative(n) => GoToTarget::Relative(n),
            T::Next => GoToTarget::Next,
            T::Previous => GoToTarget::Previous,
        }
    }
}

/// What Go To does on arriving.
#[derive(Clone, Copy, Debug)]
enum Land {
    /// A caret at the item's start.
    Caret,
    /// Selects up to here.
    Range(Position),
    /// Selects the object.
    Object(ObjectRef),
}

/// What `what` names with `name`: none given, or the same ignoring case (an object's program id: its start, as Word
/// lists `Excel.Sheet` for `Excel.Sheet.12`).
fn named(name: Option<&str>, is: &str, prefix: bool) -> bool {
    match name.map(str::trim).filter(|n| !n.is_empty()) {
        None => true,
        Some(n) if prefix => is.get(..n.len()).is_some_and(|s| s.eq_ignore_ascii_case(n)),
        Some(n) => is.eq_ignore_ascii_case(n),
    }
}

/// The first paragraph of `blocks`, inside tables too. Nesting is bounded by the reader's XML depth.
fn first_para(blocks: &[Block]) -> Option<ParaId> {
    blocks.iter().find_map(|b| match b {
        Block::Para(p) => Some(ParaId(p.id)),
        Block::Table(t) => t.rows.iter().flat_map(|r| &r.cells).find_map(|c| first_para(&c.blocks)),
    })
}

/// The local name of the element `xml` starts with, whatever its prefix.
fn local_name(xml: &str) -> Option<&str> {
    let tag = xml.trim_start().strip_prefix('<')?;
    let name = &tag[..tag.find(|c: char| c.is_whitespace() || c == '>' || c == '/').unwrap_or(tag.len())];
    name.rsplit(':').next()
}

/// An embedded object's program id, from its `w:object` as read (in whatever prefix: a picture's source is a run
/// child of the main namespace or `mc:AlternateContent`): the first `ProgID` attribute, either quote, any spacing round
/// its `=`.
fn prog_id(src: &str) -> Option<&str> {
    if local_name(src) != Some("object") {
        return None;
    }
    let mut rest = src;
    while let Some(i) = rest.find("ProgID") {
        let before = rest[..i].chars().next_back();
        rest = &rest[i + "ProgID".len()..];
        if !before.is_some_and(char::is_whitespace) {
            continue;
        }
        let Some(value) = rest.trim_start().strip_prefix('=').map(str::trim_start) else { continue };
        let Some(quote) = value.chars().next().filter(|&q| q == '"' || q == '\'') else { continue };
        return value[1..].split(quote).next();
    }
    None
}

/// The index `target` picks among `n` items, `before` of which come before where Go To starts and `after` the first
/// after it; `None` when there is no such one (no next after the last).
fn pick(target: GoToTarget, n: usize, before: usize, after: Option<usize>) -> Result<usize, &'static str> {
    if n == 0 {
        return Err("none in the document");
    }
    let last = n - 1;
    match target {
        GoToTarget::Absolute(0) => Err("numbers count from 1"),
        GoToTarget::Absolute(k) => Ok((k as usize - 1).min(last)),
        GoToTarget::Next => after.ok_or("no next one"),
        GoToTarget::Previous => before.checked_sub(1).ok_or("no previous one"),
        GoToTarget::Relative(0) => Err("no move"),
        GoToTarget::Relative(k) if k > 0 => after.map(|a| (a + k as usize - 1).min(last)).ok_or("no next one"),
        GoToTarget::Relative(k) => before.checked_sub(1).map(|b| b.saturating_sub(k.unsigned_abs() as usize - 1)).ok_or("no previous one"),
    }
}

/// The current one's number (from 0) moved by `target` among `n`, clamped: Word's Go To page and section.
fn step(target: GoToTarget, n: usize, cur: usize) -> Result<usize, &'static str> {
    let last = n.checked_sub(1).ok_or("none in the document")?;
    let to = match target {
        GoToTarget::Absolute(0) => return Err("numbers count from 1"),
        GoToTarget::Absolute(k) => k as i64 - 1,
        GoToTarget::Relative(k) => cur as i64 + i64::from(k),
        GoToTarget::Next if cur >= last => return Err("no next one"),
        GoToTarget::Previous if cur == 0 => return Err("no previous one"),
        GoToTarget::Next => cur as i64 + 1,
        GoToTarget::Previous => cur as i64 - 1,
    };
    Ok(to.clamp(0, last as i64) as usize)
}

impl Editor {
    /// Goes to a page, section, line, bookmark, comment, note, field, table, graphic, equation, object or heading of
    /// the body, as Word's Go To does: the n-th (numbers past the last go to the last), n on or back from the current
    /// one, or the next or previous. `name`: a bookmark's name (Absolute counts bookmarks in order), a comment's
    /// reviewer, a field's kind, an object's program id. The caret lands at the start (a page's, a section's, a line's,
    /// a heading's, a table's first cell, before a note's reference); a bookmark, a comment's text and a field are
    /// selected, a graphic or an object is.
    pub fn go_to(&mut self, what: GoToWhat, target: GoToTarget, name: Option<&str>, fonts: &Fonts) -> Outcome {
        match what {
            GoToWhat::Page => return self.go_to_page_at(target, fonts),
            GoToWhat::Line => return self.go_to_line(target, fonts),
            GoToWhat::Section => return self.go_to_section(target, fonts),
            GoToWhat::Bookmark if matches!(target, GoToTarget::Absolute(1..)) && name.is_some_and(|n| !n.trim().is_empty()) => {
                return self.go_to_bookmark(name.unwrap_or_default().trim(), true, fonts);
            }
            _ => {}
        }
        let items = self.go_to_items(what, name);
        // A selected object among them counts from itself: objects at one place (floats take no units) stay apart.
        let current = match self.selection {
            Selection::Object(o) => items.iter().position(|i| matches!(i.1, Land::Object(x) if x == o)),
            _ => None,
        };
        let (before, after) = match current {
            Some(k) => (k, (k + 1 < items.len()).then_some(k + 1)),
            None => {
                let here = self.go_to_from().filter(|a| a.story == StoryId::Body || items.iter().any(|i| i.0.story == a.story));
                let before = here.map_or(0, |h| items.iter().filter(|i| self.index.cmp(&i.0, &h) == Ordering::Less).count());
                let after = match here {
                    Some(h) => items.iter().position(|i| self.index.cmp(&i.0, &h) == Ordering::Greater),
                    None => (!items.is_empty()).then_some(0),
                };
                (before, after)
            }
        };
        let k = match pick(target, items.len(), before, after) {
            Ok(k) => k,
            Err(why) => return Outcome::Unchanged(why),
        };
        let (at, land) = items[k];
        self.history.break_typing();
        self.set_selection(match land {
            Land::Caret => Selection::Caret(at),
            Land::Range(end) if end != at => Selection::range(at, end, Granularity::Char),
            Land::Range(_) => Selection::Caret(at),
            Land::Object(o) => Selection::Object(o),
        });
        self.ensure(&at, fonts);
        if let Some(focus) = self.selection.focus().filter(|f| *f != at) {
            self.ensure(&focus, fonts);
        }
        match land {
            Land::Object(_) => self.jump_to_object(at),
            _ => self.jump_to_caret(),
        }
        Outcome::Moved
    }

    /// After Go To lands on an object: its frame at the top of the view, else the place it is anchored at.
    fn jump_to_object(&mut self, anchor: Position) {
        let Some(c) = self.caret_at(anchor) else { return };
        let frame = self.selection_bounds(c.page..c.page + 1).bounds.first().map(|b| b.1);
        self.requests.push(super::input::Request::ScrollIntoView { page: c.page, rect: frame.unwrap_or_else(|| c.rect()), at_top: true });
    }

    /// Where Go To counts on and back from: the selection's start, a selected object's place.
    fn go_to_from(&self) -> Option<Position> {
        let at = |para: ParaId, u: Unit| self.index.para_slot(para).map(|s| Position::text(self.index.story(s.container), para, u));
        match self.selection {
            Selection::Object(ObjectRef::Inline { para, offset }) => at(para, offset),
            Selection::Object(ObjectRef::Floating(f)) => {
                let para = self.index.float_anchor(f)?;
                let p = self.index.para(&self.doc, para)?;
                let mut u = 0u32;
                for i in &p.inlines {
                    if matches!(i, Inline::Float(fl, _) if fl.id == f.0) {
                        break;
                    }
                    u += i.units() as u32;
                }
                at(para, Unit(u))
            }
            _ => self.selection.ordered(&self.index).map(|(a, _)| a),
        }
    }

    /// The body's items of `what` named `name`, in document order: where each starts and what arriving does.
    fn go_to_items(&mut self, what: GoToWhat, name: Option<&str>) -> Vec<(Position, Land)> {
        let mut out = Vec::new();
        if what == GoToWhat::Comment {
            for r in self.thread_ranges().iter() {
                let author = self.doc.comments.by_id.get(&r.root).and_then(|c| self.doc.people.persons.get(c.author.0 as usize));
                if r.start.story == StoryId::Body && author.is_none_or(|a| named(name, &a.name, false)) {
                    out.push((r.start, Land::Range(r.end)));
                }
            }
            return out;
        }
        if what == GoToWhat::Table {
            fn tables(blocks: &[Block], out: &mut Vec<ParaId>) {
                for b in blocks {
                    if let Block::Table(t) = b {
                        if let Some(p) = t.rows.first().and_then(|r| r.cells.first()).and_then(|c| first_para(&c.blocks)) {
                            out.push(p);
                        }
                        t.rows.iter().flat_map(|r| &r.cells).for_each(|c| tables(&c.blocks, out));
                    }
                }
            }
            let mut firsts = Vec::new();
            self.doc.sections.iter().for_each(|s| tables(&s.blocks, &mut firsts));
            return firsts.into_iter().map(|p| (Position::text(StoryId::Body, p, Unit(0)), Land::Caret)).collect();
        }
        // Each bookmark's end by name without case, the first of a name read twice (as `bookmark` finds it).
        let mut ends: HashMap<String, Option<End>> = HashMap::new();
        if what == GoToWhat::Bookmark {
            for f in self.bookmarks() {
                ends.entry(f.name.to_ascii_lowercase()).or_insert(f.end);
            }
        }
        // Fields across paragraphs by the paragraph each starts in: the start's unit and the end's place.
        let mut spanning: HashMap<ParaId, Vec<(u32, Position)>> = HashMap::new();
        if what == GoToWhat::Field {
            for f in self.span_fields() {
                let Some(blocks) = self.index.blocks(&self.doc, f.container) else { continue };
                let place = |(b, k): (usize, usize)| match blocks.get(b) {
                    Some(Block::Para(p)) => Some((ParaId(p.id), p.inlines.get(..k)?.iter().map(|i| i.units() as u32).sum::<u32>(), p)),
                    _ => None,
                };
                let (Some((sp, su, p)), Some((ep, eu, _))) = (place(f.start), place(f.end)) else { continue };
                let code = match p.inlines.get(f.start.1).and_then(fields::span_edge) {
                    Some(FieldEdge::Start { view: FieldView::Code, .. }) => fields::span_code_text(&p.inlines, f.start.1),
                    Some(FieldEdge::Start { code, .. }) => code.clone(),
                    _ => continue,
                };
                if named(name, fields::words(&code).first().map_or("", String::as_str), false) {
                    spanning.entry(sp).or_default().push((su, Position::text(self.index.story(f.container), ep, Unit(eu))));
                }
            }
        }
        // Each inline's first unit, per paragraph.
        let mut starts: Vec<u32> = Vec::new();
        for para in self.story_paras(Stories::default()) {
            let Some(story) = self.index.para_slot(para).map(|s| self.index.story(s.container)) else { continue };
            let Some(p) = self.index.para(&self.doc, para) else { continue };
            let at = |u: u32| Position::text(story, para, Unit(u));
            if what == GoToWhat::Heading {
                if p.style.outline_lvl.is_some_and(|l| (0..9).contains(&l)) {
                    out.push((at(0), Land::Caret));
                }
                continue;
            }
            if what == GoToWhat::Field {
                starts.clear();
                starts.extend(p.inlines.iter().scan(0u32, |u, i| {
                    let s = *u;
                    *u += i.units() as u32;
                    Some(s)
                }));
                let mut found = spanning.remove(&para).unwrap_or_default();
                for s in fields::spans(&p.inlines) {
                    let code = fields::code_text(&p.inlines, s);
                    let (Some(&a), Some(&b)) = (starts.get(s.begin), starts.get(s.end)) else { continue };
                    if named(name, fields::words(&code).first().map_or("", String::as_str), false) {
                        found.push((a, at(b + 1)));
                    }
                }
                found.sort_by_key(|f| f.0);
                out.extend(found.into_iter().map(|(a, end)| (at(a), Land::Range(end))));
                continue;
            }
            let mut u = 0u32;
            for i in &p.inlines {
                let hit = match (what, i) {
                    (GoToWhat::Footnote, Inline::NoteRef { kind: crate::docx::NoteKind::Foot, .. })
                    | (GoToWhat::Endnote, Inline::NoteRef { kind: crate::docx::NoteKind::End, .. }) => Some(Land::Caret),
                    (GoToWhat::Graphic, Inline::Picture(pic, _)) if prog_id(pic.src.0.as_deref().unwrap_or("")).is_none() => {
                        Some(Land::Object(ObjectRef::Inline { para, offset: Unit(u) }))
                    }
                    (GoToWhat::Graphic, Inline::Float(f, _)) => Some(Land::Object(ObjectRef::Floating(FloatId(f.id)))),
                    (GoToWhat::Object, Inline::Picture(pic, _)) => prog_id(pic.src.0.as_deref().unwrap_or(""))
                        .filter(|id| named(name, id, true))
                        .map(|_| Land::Object(ObjectRef::Inline { para, offset: Unit(u) })),
                    (GoToWhat::Equation, Inline::Opaque(o))
                        if o.xml.0.as_deref().and_then(local_name).is_some_and(|n| n == "oMath" || n == "oMathPara") =>
                    {
                        Some(Land::Caret)
                    }
                    (GoToWhat::Bookmark, Inline::Range(r))
                        if let Some((n, true)) = r.bookmark()
                            && !n.starts_with('_') =>
                    {
                        let end = ends.get(&n.to_ascii_lowercase()).copied().flatten().map(|e| Position::text(story, e.para, e.unit));
                        Some(end.filter(|e| e.story == story).map_or(Land::Caret, Land::Range))
                    }
                    _ => None,
                };
                if let Some(land) = hit {
                    out.push((at(u), land));
                }
                u += i.units() as u32;
            }
        }
        out
    }

    /// Go To page: `target` as [`Editor::go_to`] takes it, from the page the selection starts on; past the last, the last.
    fn go_to_page_at(&mut self, target: GoToTarget, fonts: &Fonts) -> Outcome {
        if let GoToTarget::Absolute(n) = target {
            self.ensure_page((n as usize).saturating_sub(1), fonts);
        }
        let cur = self.go_to_from().and_then(|c| self.page_showing(c)).unwrap_or(0);
        if matches!(target, GoToTarget::Next | GoToTarget::Relative(1..)) {
            self.ensure_page(
                cur + 1
                    + usize::try_from(match target {
                        GoToTarget::Relative(k) => k.max(1) as u32 - 1,
                        _ => 0,
                    })
                    .unwrap_or(0),
                fonts,
            );
        }
        let n = self.layout.snapshot().pages().len();
        match step(target, n, cur) {
            Ok(k) => self.go_to_page(k as u32 + 1, fonts),
            Err(why) => Outcome::Unchanged(why),
        }
    }

    /// Go To section: its first paragraph's start, from the section the selection starts in.
    fn go_to_section(&mut self, target: GoToTarget, fonts: &Fonts) -> Outcome {
        let starts: Vec<Option<ParaId>> = self.doc.sections.iter().map(|s| first_para(&s.blocks)).collect();
        let cur = self.go_to_from().filter(|c| c.story == StoryId::Body).and_then(|c| self.section_at(&c)).unwrap_or(0);
        let k = match step(target, starts.len(), cur) {
            Ok(k) => k,
            Err(why) => return Outcome::Unchanged(why),
        };
        let Some(para) = starts[k] else { return Outcome::Unchanged("an empty section") };
        let at = Position::text(StoryId::Body, para, Unit(0));
        self.history.break_typing();
        self.set_selection(Selection::Caret(at));
        self.ensure(&at, fonts);
        self.jump_to_caret();
        Outcome::Moved
    }

    /// The body's lines on page `page`, in order, as (line index, its first stop's place).
    fn body_lines(&mut self, page: usize, fonts: &Fonts) -> Vec<Position> {
        self.ensure_page(page, fonts);
        let Some(geo) = self.pages.page(page, &self.index) else { return Vec::new() };
        let mut out = Vec::new();
        for (k, l) in geo.lines.iter().enumerate() {
            if l.repeat || geo.stories[l.story].story != StoryId::Body {
                continue;
            }
            if let Some(&stop) = l.stops.first() {
                out.push(self.nav().at_stop(StoryId::Body, page, k, stop));
            }
        }
        out
    }

    /// Go To line: the n-th line of the body (from its first page), or lines on and back from the caret's; past the
    /// last, the last.
    fn go_to_line(&mut self, target: GoToTarget, fonts: &Fonts) -> Outcome {
        let pages = |e: &Editor| e.layout.snapshot().pages().len();
        let found = match target {
            GoToTarget::Absolute(0) => return Outcome::invalid("lines count from 1"),
            GoToTarget::Absolute(n) => {
                let (mut left, mut page, mut last) = (n as usize, 0, None);
                loop {
                    let lines = self.body_lines(page, fonts);
                    if left <= lines.len() {
                        break lines.get(left - 1).copied();
                    }
                    left -= lines.len();
                    last = lines.last().copied().or(last);
                    page += 1;
                    self.ensure_page(page, fonts);
                    if page >= pages(self) {
                        break last;
                    }
                }
            }
            _ => {
                let Some(focus) = self.selection.focus() else { return Outcome::Unchanged("no caret") };
                // Laid out first: its page's lines and its line's start from one layout.
                self.ensure(&focus, fonts);
                let Some(at) = self.nav().line_of(focus) else { return Outcome::Unchanged("the caret's line isn't laid out") };
                let lines = self.body_lines(at.page, fonts);
                let line_start = self.nav().line_edge(focus, false).unwrap_or(focus);
                let Some(mut k) = lines.iter().position(|p| *p == line_start) else { return Outcome::Unchanged("not in the body") };
                let steps: i64 = match target {
                    GoToTarget::Next => 1,
                    GoToTarget::Previous => -1,
                    GoToTarget::Relative(r) => i64::from(r),
                    GoToTarget::Absolute(_) => 0,
                };
                let (mut page, mut lines, mut left) = (at.page, lines, steps.unsigned_abs());
                let forward = steps > 0;
                let mut moved = false;
                while left > 0 {
                    if forward && k + 1 < lines.len() {
                        k += 1;
                    } else if !forward && k > 0 {
                        k -= 1;
                    } else {
                        let next = if forward {
                            page + 1
                        } else {
                            let Some(p) = page.checked_sub(1) else { break };
                            p
                        };
                        if forward {
                            self.ensure_page(next, fonts);
                            if next >= pages(self) {
                                break;
                            }
                        }
                        page = next;
                        let more = self.body_lines(page, fonts);
                        if more.is_empty() {
                            continue;
                        }
                        k = if forward { 0 } else { more.len() - 1 };
                        lines = more;
                    }
                    moved = true;
                    left -= 1;
                }
                if !moved && matches!(target, GoToTarget::Next | GoToTarget::Previous) {
                    return Outcome::Unchanged(if forward { "no next one" } else { "no previous one" });
                }
                lines.get(k).copied()
            }
        };
        let Some(at) = found else { return Outcome::Unchanged("no lines") };
        self.history.break_typing();
        self.set_selection(Selection::Caret(at));
        self.ensure(&at, fonts);
        self.jump_to_caret();
        Outcome::Moved
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_count_from_one_and_clamp() {
        assert_eq!(pick(GoToTarget::Absolute(2), 3, 0, Some(0)), Ok(1));
        assert_eq!(pick(GoToTarget::Absolute(9), 3, 0, Some(0)), Ok(2), "past the last: the last");
        assert!(pick(GoToTarget::Absolute(0), 3, 0, Some(0)).is_err());
        assert_eq!(pick(GoToTarget::Next, 3, 1, Some(1)), Ok(1));
        assert!(pick(GoToTarget::Next, 3, 3, None).is_err(), "none after the last");
        assert_eq!(pick(GoToTarget::Previous, 3, 2, None), Ok(1));
        assert_eq!(pick(GoToTarget::Relative(5), 3, 0, Some(0)), Ok(2));
        assert_eq!(pick(GoToTarget::Relative(-5), 3, 2, None), Ok(0));
        assert_eq!(step(GoToTarget::Relative(2), 5, 1), Ok(3));
        assert_eq!(step(GoToTarget::Relative(-9), 5, 1), Ok(0));
        assert_eq!(step(GoToTarget::Absolute(99), 5, 1), Ok(4));
        assert!(step(GoToTarget::Next, 5, 4).is_err());
    }

    #[test]
    fn program_ids_and_names() {
        assert_eq!(prog_id(r#"<w:object><o:OLEObject Type="Embed" ProgID="Excel.Sheet.12" r:id="x"/></w:object>"#), Some("Excel.Sheet.12"));
        assert_eq!(prog_id("<w:pict/>"), None);
        assert_eq!(prog_id(r#"<wpml:object><o:OLEObject ProgID="Excel.Sheet.12"/></wpml:object>"#), Some("Excel.Sheet.12"), "any prefix");
        assert_eq!(prog_id(r#"<w:objects ProgID="x"/>"#), None);
        assert!(named(Some("excel.sheet"), "Excel.Sheet.12", true));
        assert!(!named(Some("A"), "\u{c4}xcel.Sheet", true), "a name ending inside a character");
        assert_eq!(prog_id("<w:object><o:OLEObject ProgID = 'Word.Document.12'/></w:object>"), Some("Word.Document.12"));
        assert_eq!(prog_id(r#"<w:object><o:OLEObject XProgID="a" ProgID="b"/></w:object>"#), Some("b"));
        assert!(!named(Some("Word"), "Excel.Sheet.12", true));
        assert!(named(None, "x", false) && named(Some(" "), "x", false));
        assert!(named(Some("page"), "PAGE", false) && !named(Some("PAGEREF"), "PAGE", false));
    }
}
