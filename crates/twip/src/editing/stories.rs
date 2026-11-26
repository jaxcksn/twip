//! Header, footer and note stories

use crate::docx::{Block, Inline, NoteKind, Paragraph};
use crate::font::Fonts;
use crate::layout::{ParaId, Unit};

use super::Editor;
use super::commands::Outcome;
use super::geometry::PageGeometry;
use super::position::{Position, StoryId};
use super::selection::{ObjectRef, Selection};
use crate::docx::run_xml;

pub(super) fn reachable(ctx: StoryId, s: StoryId) -> bool {
    matches!(ctx, StoryId::HeaderFooter(_)) == matches!(s, StoryId::HeaderFooter(_))
}

pub(super) fn inline_at(p: &Paragraph, u: Unit) -> Option<&Inline> {
    let mut at = 0;
    for i in &p.inlines {
        let n = i.units();
        if n > 0 && (u.0 as usize) < at + n {
            return Some(i);
        }
        at += n;
    }
    None
}

fn body_band(geo: &PageGeometry) -> Option<(f64, f64)> {
    geo.body_area
}

fn is_note(s: StoryId) -> bool {
    matches!(s, StoryId::Footnote(_) | StoryId::Endnote(_))
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Provisional {
    story: StoryId,
    tx: u64,
}

impl Editor {
    pub(super) fn editing(&self) -> StoryId {
        let s = match self.selection {
            Selection::Cells { table, .. } => self.index.table_slot(table).map(|s| self.index.story(s.container)),
            Selection::Object(ObjectRef::Floating(f)) => Some(StoryId::TextBox(f)),
            Selection::Object(ObjectRef::Inline { para, .. }) => self.index.para_slot(para).map(|s| self.index.story(s.container)),
            _ => self.selection.focus().map(|p| p.story),
        };
        match s.map(|s| self.index.host(s)) {
            Some(s @ StoryId::HeaderFooter(_)) => s,
            _ => StoryId::Body,
        }
    }

    pub(super) fn story_click(&mut self, x: f64, y: f64, clicks: u8, fonts: &Fonts) -> bool {
        let stack = self.stack();
        let Some((page, px, py)) = stack.page_at(x, y) else { return false };
        self.ensure_page(page, fonts);
        let Some(geo) = self.pages.page(page, &self.index) else { return false };
        let in_body = body_band(&geo).is_some_and(|(t, b)| py >= t && py <= b);
        let hf = matches!(self.editing(), StoryId::HeaderFooter(_));
        let hit = |want_hf: bool| geo.hit_text(px, py, &|s| matches!(s, StoryId::HeaderFooter(_)) == want_hf);
        let to = match (hf, in_body) {
            (false, false) if clicks >= 2 => {
                let below = body_band(&geo).is_some_and(|(_, b)| py > b);
                let side = |s: StoryId| geo.stories.iter().any(|g| g.story == s && body_band(&geo).is_some_and(|(t, _)| (g.rect[1] >= t) == below));
                let Some(h) = geo.hit_text(px, py, &|s| matches!(s, StoryId::HeaderFooter(_)) && side(s)) else {
                    let _ = self.edit_header_footer(Some(page), below, fonts);
                    return true;
                };
                self.body_return = Some(self.selection);
                h
            }
            (true, true) if clicks >= 2 => {
                let Some(h) = hit(false) else { return true };
                self.body_return = None;
                h
            }
            // In the body area only a header's objects and the selected one's handles take a press.
            (true, true)
                if self.object_hit(x, y, fonts).is_none() && self.handle_at(x, y, fonts).is_none() && self.shape_handle_at(x, y).is_none() =>
            {
                return true;
            }
            _ => return false,
        };
        self.near = page;
        self.drag = None;
        self.goal = None;
        self.set_selection(Selection::Caret(Position::text(to.story, to.para, to.unit).with_affinity(to.affinity)));
        true
    }

    pub(super) fn escape(&mut self) -> Outcome {
        if self.end_wrap_points() {
            return Outcome::Moved;
        }
        if self.marquee_cancel() || self.end_linking() {
            return Outcome::Moved;
        }
        if self.table_tool != crate::api::command::TableTool::None {
            self.set_table_tool(crate::api::command::TableTool::None);
            return Outcome::Moved;
        }
        if self.select_objects {
            self.object_cancel();
            self.shape_cancel();
            self.set_select_objects(false);
            return Outcome::Moved;
        }
        if self.text_drag.take().is_some() {
            self.requests.push(super::input::Request::ReleasePointer);
            return Outcome::Moved;
        }
        if self.end_modes() {
            return Outcome::Moved;
        }
        // A drag in progress ends before an idle tool does.
        if self.object_cancel() || self.shape_cancel() {
            return Outcome::Moved;
        }
        if self.border_cancel() || self.table_drag.take().is_some() {
            self.requests.push(super::input::Request::ReleasePointer);
            return Outcome::Moved;
        }
        if self.drop_menu.take().is_some() || self.highlighter.take().is_some() {
            return Outcome::Moved;
        }
        if self.shape_insert.take().is_some() {
            return Outcome::Moved;
        }
        if self.selected_child().is_some() {
            self.object_child = None;
            return Outcome::Moved;
        }
        match self.selection {
            Selection::Object(ObjectRef::Inline { para, offset }) => {
                let story = self.index.para_slot(para).map_or(StoryId::Body, |s| self.index.story(s.container));
                self.set_selection(Selection::Caret(Position::text(story, para, offset)));
                return Outcome::Moved;
            }
            Selection::Object(ObjectRef::Floating(f)) => {
                let Some(anchor) = self.index.float_anchor(f) else { return Outcome::Unchanged("no anchor") };
                let story = self.index.para_slot(anchor).map_or(StoryId::Body, |s| self.index.story(s.container));
                self.set_selection(Selection::Caret(Position::text(story, anchor, Unit(0))));
                return Outcome::Moved;
            }
            _ => {}
        }
        match self.selection.focus().map(|p| p.story) {
            Some(StoryId::TextBox(f)) => {
                self.set_selection(Selection::Object(self.story_object(f)));
                Outcome::Moved
            }
            _ if matches!(self.editing(), StoryId::HeaderFooter(_)) => self.close_header_footer(),
            _ => Outcome::Unchanged("nothing to leave"),
        }
    }

    /// An empty header's or footer's paragraph, in the Header or Footer style when the document has it.
    pub(super) fn empty_story_para(&self, footer: bool) -> Option<crate::docx::Paragraph> {
        let res = self.doc.resolver.clone()?;
        let cx = self.cx();
        let w = cx.w.clone();
        let style = if footer { "Footer" } else { "Header" };
        let mut pp = crate::docx::write::props::Props::parse(None, "pPr", &cx);
        if res.styles.by_id.contains_key(style) {
            pp.set("pStyle", format!("<{w}pStyle {w}val=\"{style}\"/>"));
        }
        let r = crate::docx::para_xml::from_xml(&res, pp.xml().as_deref(), None, &cx);
        let run = run_xml::plain(&res, &r.base);
        Some(crate::docx::Paragraph {
            style: std::rc::Rc::new(r.style),
            mark: run,
            list: None,
            inlines: Vec::new(),
            src: None,
            id: crate::docx::next_para_id(),
            div: None,
            controls: Vec::new(),
        })
    }

    /// A part name for a new header or footer, beside the main part: one no story has, had when read, a section
    /// refers to (a part that didn't load), or the package holds (a removed part's name names that part's
    /// relationship).
    pub(super) fn new_story_name(&self, footer: bool) -> String {
        self.new_story_names(std::iter::once(footer)).pop().unwrap_or_default()
    }

    /// [`Editor::new_story_name`] for several new parts at once, each a header or a footer (`true`), all different.
    pub(super) fn new_story_names(&self, footers: impl IntoIterator<Item = bool>) -> Vec<String> {
        let src = self.doc.source.as_ref();
        let dir = src.and_then(|s| s.main.rsplit_once('/')).map_or(String::new(), |(d, _)| format!("{d}/"));
        let package = src.and_then(|s| zip::ZipArchive::new(std::io::Cursor::new(&s.package[..])).ok());
        let referred: std::collections::HashSet<&str> =
            self.doc.sections.iter().flat_map(|s| s.headers.iter().chain(s.footers.iter())).flatten().map(String::as_str).collect();
        let taken = |name: &str| {
            self.doc.stories.contains_key(name)
                || src.is_some_and(|s| s.stories.contains_key(name))
                || referred.contains(name)
                || package.as_ref().is_some_and(|z| z.index_for_name(name).is_some())
        };
        let mut from = 1usize;
        footers
            .into_iter()
            .map(|footer| {
                let kind = if footer { "footer" } else { "header" };
                let n = (from..).find(|n| !taken(&format!("{dir}header{n}.xml")) && !taken(&format!("{dir}footer{n}.xml"))).unwrap_or(from);
                from = n + 1;
                format!("{dir}{kind}{n}.xml")
            })
            .collect()
    }

    /// The step that takes section `section`'s own header or footer `name` away: the part goes with it, unless another
    /// section or slot shows it too (generated documents share one part), or the part never loaded, when only this
    /// reference goes.
    fn drop_story_ref(&self, name: String, section: usize, footer: bool, slot: usize) -> Option<super::step::Step> {
        let refs = self.doc.sections.iter().flat_map(|s| s.headers.iter().chain(s.footers.iter())).flatten().filter(|n| **n == name).count();
        if refs <= 1 && self.doc.stories.contains_key(&name) {
            return Some(super::step::Step::HeaderPart { name, section, footer, slot, blocks: None });
        }
        let s = self.doc.sections.get(section)?;
        let mut page = s.page();
        if footer {
            page.footers[slot] = None
        } else {
            page.headers[slot] = None
        }
        Some(super::step::Step::SectionPage { section, page: Box::new(page), change: s.change.clone() })
    }

    /// Link to Previous for the caret section's header or footer: it takes the section before's (its own part goes), or
    /// gets its own copy of the one it takes.
    pub(super) fn set_header_footer_linked(&mut self, l: crate::api::command::HeaderFooterLink, fonts: &Fonts) -> Outcome {
        let (section, footer, slot) = (self.focus_section(), l.kind == crate::api::state::HeaderFooterKind::Footer, hf_slot(l.pages));
        if section == 0 {
            return Outcome::unavailable("the first section has no section before it");
        }
        let refs = |e: &Self, k: usize| e.doc.sections.get(k).map(|s| if footer { s.footers.clone() } else { s.headers.clone() });
        let Some(own) = refs(self, section).map(|r| r[slot].clone()) else { return Outcome::Unchanged("no section") };
        let here = self.in_shown_part(section, footer, slot);
        let mut tx = self.begin("Link to Previous");
        let step = match (l.linked, own) {
            (true, Some(name)) => match self.drop_story_ref(name, section, footer, slot) {
                Some(step) => step,
                None => {
                    self.abort(tx);
                    return Outcome::Unchanged("no section");
                }
            },
            (false, None) => {
                // A copy of what it shows now: the nearest section before's.
                let inherited = (0..section).rev().find_map(|k| refs(self, k).and_then(|r| r[slot].clone()));
                let blocks = match inherited.and_then(|n| self.doc.stories.get(&n)) {
                    Some(b) => b.iter().map(super::clipboard::fresh).collect(),
                    None => match self.empty_story_para(footer) {
                        Some(p) => vec![crate::docx::Block::Para(p)],
                        None => return Outcome::Unchanged("no styles"),
                    },
                };
                super::step::Step::HeaderPart { name: self.new_story_name(footer), section, footer, slot, blocks: Some(blocks) }
            }
            _ => {
                self.abort(tx);
                return Outcome::Unchanged("already so");
            }
        };
        if self.step(&mut tx, step).is_err() {
            self.abort(tx);
            return Outcome::Unchanged("the header can't change");
        }
        if here {
            self.enter_shown_part(section, footer, slot);
        }
        self.commit(tx, None, fonts)
    }

    /// Empties the caret section's header or footer: its own part's content goes, or it gets an empty one of its own.
    pub(super) fn remove_header_footer(&mut self, h: crate::api::command::HeaderFooterSlot, fonts: &Fonts) -> Outcome {
        let (section, footer, slot) = (self.focus_section(), h.kind == crate::api::state::HeaderFooterKind::Footer, hf_slot(h.pages));
        let Some(s) = self.doc.sections.get(section) else { return Outcome::Unchanged("no section") };
        let own = if footer { s.footers[slot].clone() } else { s.headers[slot].clone() };
        let Some(para) = self.empty_story_para(footer) else { return Outcome::Unchanged("no styles") };
        let here = self.in_shown_part(section, footer, slot);
        let mut tx = self.begin(if footer { "Remove Footer" } else { "Remove Header" });
        let name = match own {
            Some(name) => {
                let step = self.drop_story_ref(name.clone(), section, footer, slot);
                let shared = !matches!(step, Some(super::step::Step::HeaderPart { .. }));
                if !step.is_some_and(|step| self.step(&mut tx, step).is_ok()) {
                    self.abort(tx);
                    return Outcome::Unchanged("the header can't change");
                }
                // A shared part stays the others'; this section gets an empty one of its own.
                if shared { self.new_story_name(footer) } else { name }
            }
            None => self.new_story_name(footer),
        };
        let blocks = Some(vec![crate::docx::Block::Para(para)]);
        if self.step(&mut tx, super::step::Step::HeaderPart { name, section, footer, slot, blocks }).is_err() {
            self.abort(tx);
            return Outcome::Unchanged("the header can't change");
        }
        if here {
            self.enter_shown_part(section, footer, slot);
        }
        self.commit(tx, None, fonts)
    }

    pub(super) fn edit_header_footer(&mut self, page: Option<usize>, footer: bool, fonts: &Fonts) -> Outcome {
        self.end_provisional(fonts);
        let page = page.or_else(|| self.caret_view().map(|c| c.page)).unwrap_or(self.near);
        let Some(info) = self.snapshot().info().get(page).cloned() else { return Outcome::Unchanged("no page") };
        let (slot, part) =
            crate::layout::page::story_ref(&self.doc.sections, self.doc.settings.even_and_odd, info.section, info.section_page, info.number, footer);
        let part = part.filter(|p| self.doc.stories.contains_key(*p)).cloned();
        if !matches!(self.editing(), StoryId::HeaderFooter(_)) {
            self.body_return = Some(self.selection);
        }
        let mut tx = None;
        let name = match part {
            Some(n) => n,
            None => {
                let Some(para) = self.empty_story_para(footer) else { return Outcome::Unchanged("no styles") };
                let name = self.new_story_name(footer);
                let mut t = self.begin(if footer { "Footer" } else { "Header" });
                // A reference to a part that never loaded
                let dangling = (0..=info.section).rev().find_map(|k| {
                    let s = self.doc.sections.get(k)?;
                    Some((k, (if footer { &s.footers } else { &s.headers })[slot].clone()?))
                });
                let section = dangling.as_ref().map_or(0, |(k, _)| *k);
                let cleared = match dangling {
                    Some((k, d)) => self.drop_story_ref(d, k, footer, slot).is_some_and(|step| self.step(&mut t, step).is_ok()),
                    None => true,
                };
                let blocks = Some(vec![crate::docx::Block::Para(para)]);
                if !cleared || self.step(&mut t, super::step::Step::HeaderPart { name: name.clone(), section, footer, slot, blocks }).is_err() {
                    self.abort(t);
                    return Outcome::Unchanged("header failed");
                }
                tx = Some(t);
                name
            }
        };
        let story = self.index.header_footer(&name);
        let first = story
            .and_then(|s| self.index.blocks(&self.doc, super::index::Container::Story(s)))
            .and_then(|b| b.iter().find_map(|b| if let crate::docx::Block::Para(p) = b { Some(p.id) } else { None }));
        let (Some(story), Some(first)) = (story, first) else {
            if let Some(t) = tx {
                self.abort(t);
            }
            return Outcome::Unchanged("no header paragraph");
        };
        self.near = page;
        self.set_selection(Selection::Caret(Position::text(story, ParaId(first), Unit(0))));
        let Some(t) = tx else { return Outcome::Moved };
        let out = self.commit(t, None, fonts);
        if matches!(out, Outcome::Changed(_)) && self.batch.is_none() {
            self.provisional = Some(Provisional { story, tx: self.history.position() });
        }
        out
    }

    pub(super) fn settle_provisional(&mut self, fonts: &Fonts) {
        if self.provisional.is_some_and(|p| self.editing() != p.story) {
            self.end_provisional(fonts);
        }
    }

    fn end_provisional(&mut self, fonts: &Fonts) {
        let Some(p) = self.provisional.filter(|_| self.batch.is_none()) else { return };
        self.provisional = None;
        let kept = (self.editing() != p.story).then_some(self.selection);
        if self.history.position() == p.tx && matches!(self.undo(true, fonts), Outcome::Changed(_)) {
            self.history.pop_redo();
            if let Some(kept) = kept {
                self.set_selection(kept);
            }
        }
    }

    pub(super) fn close_header_footer(&mut self) -> Outcome {
        if !matches!(self.editing(), StoryId::HeaderFooter(_)) {
            return Outcome::Unchanged("not in a header or footer");
        }
        let s = self.body_selection();
        self.set_selection(s);
        Outcome::Moved
    }

    /// What closing a header or footer selects: the selection from before it opened while that still resolves, else
    /// the body's start.
    fn body_selection(&mut self) -> Selection {
        let resolves = |e: &Self, s: &Selection| match *s {
            Selection::Cells { table, .. } => e.index.table(&e.doc, table).is_some(),
            Selection::Object(ObjectRef::Inline { para, .. }) => e.index.para(&e.doc, para).is_some(),
            Selection::Object(ObjectRef::Floating(f)) => e.index.float_anchor(f).is_some(),
            _ => s.focus().and_then(|p| p.para_offset()).is_some_and(|(p, _)| e.index.para(&e.doc, p).is_some()),
        };
        match self.body_return.take().filter(|s| resolves(self, s)) {
            Some(s) => s,
            None => Selection::Caret(self.nav().story_edge(StoryId::Body, false).unwrap_or(Position::text(StoryId::Body, ParaId(0), Unit(0)))),
        }
    }

    /// The header or footer part section `section` shows in `slot`: its own, or the nearest section before's.
    fn shown_part(&self, section: usize, footer: bool, slot: usize) -> Option<&String> {
        self.doc.sections.get(..=section)?.iter().rev().find_map(|s| (if footer { &s.footers } else { &s.headers }).get(slot)?.as_ref())
    }

    /// Whether the selection is in the part section `section` shows in `slot`.
    fn in_shown_part(&self, section: usize, footer: bool, slot: usize) -> bool {
        let StoryId::HeaderFooter(k) = self.editing() else { return false };
        self.index.part_name(k).is_some_and(|n| self.shown_part(section, footer, slot).is_some_and(|s| s == n))
    }

    /// A caret at the start of the part section `section` now shows in `slot`, or back in the body when it shows none.
    fn enter_shown_part(&mut self, section: usize, footer: bool, slot: usize) {
        let at = self.shown_part(section, footer, slot).and_then(|n| self.index.header_footer(n)).and_then(|story| {
            let blocks = self.index.blocks(&self.doc, super::index::Container::Story(story))?;
            let first = blocks.iter().find_map(|b| if let Block::Para(p) = b { Some(p.id) } else { None })?;
            Some(Position::text(story, ParaId(first), Unit(0)))
        });
        let s = match at {
            Some(at) => Selection::Caret(at),
            None => self.body_selection(),
        };
        self.set_selection(s);
    }

    pub(super) fn note_jump(&mut self, p: Position) -> bool {
        let Some(t) = self.note_pair(p) else { return false };
        self.drag = None;
        self.set_selection(Selection::Caret(t));
        true
    }

    /// Where a double-click at `p` on a note reference, or on a note's own mark, goes: the other one.
    pub(super) fn note_pair(&mut self, p: Position) -> Option<Position> {
        let (para, u) = p.para_offset()?;
        let q = self.index.para(&self.doc, para)?;
        let near = [Some(u), u.0.checked_sub(1).map(Unit)];
        let found = near.into_iter().flatten().find_map(|v| match inline_at(q, v) {
            Some(Inline::NoteRef { kind, id, .. }) if !is_note(p.story) => Some((true, *kind, *id)),
            Some(Inline::NoteMark(kind, id, _)) if is_note(p.story) => Some((false, *kind, *id)),
            _ => None,
        });
        match found {
            Some((true, kind, id)) => {
                let story = match kind {
                    NoteKind::Foot => StoryId::Footnote(id),
                    NoteKind::End => StoryId::Endnote(id),
                };
                self.nav().story_edge(story, false)
            }
            Some((false, kind, id)) => self.note_reference(kind, id).map(|(at, _)| at),
            None => self.custom_mark_reference(p.story, q, near),
        }
    }

    /// The reference of the note whose custom mark (the text leading its first paragraph `q`) is at one of `near`.
    fn custom_mark_reference(&self, story: StoryId, q: &Paragraph, near: [Option<Unit>; 2]) -> Option<Position> {
        let (kind, id, blocks) = match story {
            StoryId::Footnote(id) => (NoteKind::Foot, id, self.doc.footnotes.get(&id)?),
            StoryId::Endnote(id) => (NoteKind::End, id, self.doc.endnotes.get(&id)?),
            _ => return None,
        };
        if !matches!(blocks.first(), Some(Block::Para(first)) if first.id == q.id) {
            return None;
        }
        let mark = q.inlines.iter().find(|i| i.units() > 0).filter(|i| matches!(i, Inline::Text(..)))?;
        if !near.into_iter().flatten().any(|v| inline_at(q, v).is_some_and(|i| std::ptr::eq(i, mark))) {
            return None;
        }
        self.note_reference(kind, id).filter(|(_, custom)| *custom).map(|(at, _)| at)
    }

    /// Where the note's reference is, and whether it has a custom mark.
    pub(super) fn note_reference(&self, kind: NoteKind, id: i64) -> Option<(Position, bool)> {
        self.index.para_ids().find_map(|pid| {
            let p = self.index.para(&self.doc, pid)?;
            let mut at = 0u32;
            for i in &p.inlines {
                if let Inline::NoteRef { kind: k, id: n, custom, .. } = i
                    && *k == kind
                    && *n == id
                {
                    let story = self.index.para_slot(pid).map_or(StoryId::Body, |s| self.index.story(s.container));
                    return Some((Position::text(story, pid, Unit(at)), *custom));
                }
                at += i.units() as u32;
            }
            None
        })
    }

    /// The notes of a kind in their references' order, then those the body doesn't refer to in id order.
    pub(super) fn note_stories(&self, kind: NoteKind) -> Vec<StoryId> {
        let map = match kind {
            NoteKind::Foot => &self.doc.footnotes,
            NoteKind::End => &self.doc.endnotes,
        };
        let separator =
            |b: &[Block]| b.iter().any(|b| matches!(b, Block::Para(p) if p.inlines.iter().any(|i| matches!(i, Inline::Separator { .. }))));
        let mut refs = Vec::new();
        for s in &self.doc.sections {
            super::notes::refs_in(&s.blocks, &mut refs);
        }
        let mut seen = std::collections::HashSet::with_capacity(map.len());
        let mut ids: Vec<i64> = refs
            .into_iter()
            .filter(|&(k, id)| k == kind && map.get(&id).is_some_and(|b| !separator(b)) && seen.insert(id))
            .map(|(_, id)| id)
            .collect();
        let mut rest: Vec<i64> = map.iter().filter(|(id, b)| !seen.contains(*id) && !separator(b)).map(|(id, _)| *id).collect();
        rest.sort_unstable();
        ids.append(&mut rest);
        ids.into_iter()
            .map(|id| match kind {
                NoteKind::Foot => StoryId::Footnote(id),
                NoteKind::End => StoryId::Endnote(id),
            })
            .collect()
    }

    pub(super) fn next_note_edge(&mut self, from: StoryId, forward: bool) -> Option<Position> {
        let kind = match from {
            StoryId::Footnote(_) => NoteKind::Foot,
            StoryId::Endnote(_) => NoteKind::End,
            _ => return None,
        };
        let notes = self.note_stories(kind);
        let k = notes.iter().position(|&s| s == from)?;
        let to = if forward { notes.get(k + 1) } else { k.checked_sub(1).and_then(|k| notes.get(k)) }?;
        self.nav().story_edge(*to, !forward)
    }

    /// A line move out of a note or a header or footer to the line beside it on the page, the nearest to `goal`.
    pub(super) fn cross_vertical(&mut self, focus: Position, goal: [f64; 2], down: bool, fonts: &Fonts) -> Option<Position> {
        let ok: fn(StoryId) -> bool = match focus.story {
            StoryId::HeaderFooter(_) => |s| matches!(s, StoryId::HeaderFooter(_)),
            s if is_note(s) => |s| s == StoryId::Body || is_note(s),
            _ => return None,
        };
        let at = self.nav().line_of(focus)?;
        let cur = &at.geo.lines[at.line];
        let band = |l: &super::geometry::GLine| l.band(l.x0(), l.x1());
        let [_, top, _, bottom] = band(cur);
        let x = goal[0];
        let mut page = at.page;
        let mut first = true;
        loop {
            self.ensure_page(page, fonts);
            if let Some(geo) = self.pages.page(page, &self.index) {
                let best = geo
                    .lines
                    .iter()
                    .enumerate()
                    // Never the line moved from, whatever its band: an empty quarter-turned line has none.
                    .filter(|&(k, l)| !(first && k == at.line) && !l.repeat && ok(geo.stories[l.story].story))
                    .filter(|(_, l)| !first || if down { band(l)[1] >= bottom - 0.01 } else { band(l)[3] <= top + 0.01 })
                    .map(|(_, l)| l)
                    .min_by(|a, b| {
                        let (ra, rb) = (band(a), band(b));
                        let ka = if down { ra[1] } else { -ra[3] };
                        let kb = if down { rb[1] } else { -rb[3] };
                        let dx = |r: [f64; 4]| (r[0] - x).max(x - r[2]).max(0.0);
                        ka.total_cmp(&kb).then(dx(ra).total_cmp(&dx(rb)))
                    });
                if let Some(l) = best {
                    let story = geo.stories[l.story].story;
                    let s = l.nearest(l.local(goal)[0])?;
                    self.near = page;
                    return Some(Position::text(story, s.para, s.unit));
                }
            }
            first = false;
            page = if down { page + 1 } else { page.checked_sub(1)? };
            if page >= self.pages.len() {
                return None;
            }
        }
    }

    pub(super) fn page_header_footer(&mut self, focus: Position, forward: bool, fonts: &Fonts) -> Option<Position> {
        let at = self.nav().line_of(focus)?;
        let cur = &at.geo.lines[at.line];
        let above = body_band(&at.geo).is_none_or(|(t, _)| cur.bottom <= t + 0.01);
        let x0 = cur.x0();
        let x = self.nav().caret_x(focus).unwrap_or(x0);
        let page = if forward { at.page + 1 } else { at.page.checked_sub(1)? };
        if page >= self.pages.len() {
            return None;
        }
        self.ensure_page(page, fonts);
        let geo = self.pages.page(page, &self.index)?;
        let band = body_band(&geo);
        let side = |l: &super::geometry::GLine| match band {
            Some((t, b)) => {
                if above {
                    l.bottom <= t + 0.01
                } else {
                    l.top >= b - 0.01
                }
            }
            None => true,
        };
        let l = geo
            .lines
            .iter()
            .filter(|l| matches!(geo.stories[l.story].story, StoryId::HeaderFooter(_)) && side(l))
            .min_by(|a, b| if above { a.top.total_cmp(&b.top) } else { b.top.total_cmp(&a.top) })?;
        self.near = page;
        let s = l.nearest(x)?;
        Some(Position::text(geo.stories[l.story].story, s.para, s.unit))
    }
}

/// The header or footer slot a page kind is ([`crate::docx::HfRefs`] order).
fn hf_slot(t: crate::api::state::HeaderFooterType) -> usize {
    match t {
        crate::api::state::HeaderFooterType::Default => 0,
        crate::api::state::HeaderFooterType::First => 1,
        crate::api::state::HeaderFooterType::Even => 2,
    }
}
