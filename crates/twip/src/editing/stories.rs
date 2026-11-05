//! Header, footer and note stories

use crate::docx::{Block, Inline, NoteKind, Paragraph};
use crate::font::Fonts;
use crate::layout::{ParaId, Unit};

use super::Editor;
use super::commands::Outcome;
use super::geometry::PageGeometry;
use super::position::{Position, StoryId};
use super::selection::{ObjectRef, Selection};

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
            (true, true) if self.object_hit(x, y, fonts).is_none() => return true,
            _ => return false,
        };
        self.near = page;
        self.drag = None;
        self.goal_x = None;
        self.set_selection(Selection::Caret(Position::text(to.story, to.para, to.unit).with_affinity(to.affinity)));
        true
    }

    pub(super) fn escape(&mut self) -> Outcome {
        if self.text_drag.take().is_some() {
            self.requests.push(super::input::Request::ReleasePointer);
            return Outcome::Moved;
        }
        if self.end_modes() {
            return Outcome::Moved;
        }
        if self.object_cancel() || self.shape_cancel() || self.shape_insert.take().is_some() {
            return Outcome::Moved;
        }
        if self.border_drag.take().is_some() || self.table_drag.take().is_some() {
            self.requests.push(super::input::Request::ReleasePointer);
            return Outcome::Moved;
        }
        if self.selected_child().is_some() {
            self.object_child = None;
            return Outcome::Moved;
        }
        if self.collapse_after_placeholder() {
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

    pub(super) fn edit_header_footer(&mut self, page: Option<usize>, footer: bool, fonts: &Fonts) -> Outcome {
        self.end_provisional(fonts);
        let page = page.or_else(|| self.caret_view().map(|c| c.page)).unwrap_or(self.near);
        let Some(info) = self.snapshot().info().get(page).cloned() else { return Outcome::Unchanged("no page") };
        let (slot, part) =
            crate::layout::page::story_ref(&self.doc.sections, self.doc.settings.even_and_odd, info.section, info.section_page, info.number, footer);
        let part = part.filter(|p| self.doc.stories.contains_key(*p)).cloned();
        if matches!(self.selection.focus().map(|p| p.story), Some(StoryId::Body)) {
            self.body_return = Some(self.selection);
        }
        let mut tx = None;
        let name = match part {
            Some(n) => n,
            None => {
                let Some(res) = self.doc.resolver.clone() else { return Outcome::Unchanged("no styles") };
                let cx = self.cx();
                let w = cx.w.clone();
                let style = if footer { "Footer" } else { "Header" };
                let mut pp = crate::docx::write::props::Props::parse(None, "pPr", &cx);
                if res.styles.by_id.contains_key(style) {
                    pp.set("pStyle", format!("<{w}pStyle {w}val=\"{style}\"/>"));
                }
                let r = res.para_from_xml(pp.xml().as_deref(), None, &cx);
                let run = res.run_from_xml(&r.base, None, &cx);
                let para = crate::docx::Paragraph {
                    style: std::rc::Rc::new(r.style),
                    mark: run,
                    list: None,
                    inlines: Vec::new(),
                    src: None,
                    id: crate::docx::next_para_id(),
                    controls: Vec::new(),
                };
                let kind = if footer { "footer" } else { "header" };
                let n = (1..)
                    .find(|n| {
                        !self.doc.stories.contains_key(&format!("word/header{n}.xml"))
                            && !self.doc.stories.contains_key(&format!("word/footer{n}.xml"))
                    })
                    .unwrap_or(1);
                let name = format!("word/{kind}{n}.xml");
                let mut t = self.begin(if footer { "Footer" } else { "Header" });
                if self
                    .step(
                        &mut t,
                        super::step::Step::HeaderPart {
                            name: name.clone(),
                            section: 0,
                            footer,
                            slot,
                            blocks: Some(vec![crate::docx::Block::Para(para)]),
                        },
                    )
                    .is_err()
                {
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
        if matches!(out, Outcome::Changed(_)) {
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
        let kept = self.selection;
        if self.history.position() == p.tx && matches!(self.undo(true, fonts), Outcome::Changed(_)) {
            self.history.pop_redo();
            if kept.focus().is_some_and(|f| f.story != p.story) {
                self.set_selection(kept);
            }
        }
    }

    pub(super) fn close_header_footer(&mut self) -> Outcome {
        if !matches!(self.editing(), StoryId::HeaderFooter(_)) {
            return Outcome::Unchanged("not in a header or footer");
        }
        let back =
            self.body_return.take().filter(|s| s.focus().and_then(|p| p.para_offset()).is_some_and(|(p, _)| self.index.para(&self.doc, p).is_some()));
        let s = match back {
            Some(s) => s,
            None => Selection::Caret(self.nav().story_edge(StoryId::Body, false).unwrap_or(Position::text(StoryId::Body, ParaId(0), Unit(0)))),
        };
        self.set_selection(s);
        Outcome::Moved
    }

    pub(super) fn note_jump(&mut self, p: Position) -> bool {
        let Some((para, u)) = p.para_offset() else { return false };
        let Some(q) = self.index.para(&self.doc, para) else { return false };
        let near = [Some(u), u.0.checked_sub(1).map(Unit)];
        let found = near.into_iter().flatten().find_map(|v| match inline_at(q, v) {
            Some(Inline::NoteRef { kind, id, .. }) if !is_note(p.story) => Some((true, *kind, *id)),
            Some(Inline::NoteMark(kind, id, _)) if is_note(p.story) => Some((false, *kind, *id)),
            _ => None,
        });
        let Some((to_note, kind, id)) = found else { return false };
        let target = if to_note {
            let story = match kind {
                NoteKind::Foot => StoryId::Footnote(id),
                NoteKind::End => StoryId::Endnote(id),
            };
            self.nav().story_edge(story, false)
        } else {
            self.note_reference(kind, id)
        };
        let Some(t) = target else { return false };
        self.drag = None;
        self.set_selection(Selection::Caret(t));
        true
    }

    fn note_reference(&self, kind: NoteKind, id: i64) -> Option<Position> {
        self.index.para_ids().find_map(|pid| {
            let p = self.index.para(&self.doc, pid)?;
            let mut at = 0u32;
            for i in &p.inlines {
                if let Inline::NoteRef { kind: k, id: n, .. } = i
                    && *k == kind
                    && *n == id
                {
                    let story = self.index.para_slot(pid).map_or(StoryId::Body, |s| self.index.story(s.container));
                    return Some(Position::text(story, pid, Unit(at)));
                }
                at += i.units() as u32;
            }
            None
        })
    }

    fn notes(&self, kind: NoteKind) -> Vec<StoryId> {
        let map = match kind {
            NoteKind::Foot => &self.doc.footnotes,
            NoteKind::End => &self.doc.endnotes,
        };
        let separator =
            |b: &[Block]| b.iter().any(|b| matches!(b, Block::Para(p) if p.inlines.iter().any(|i| matches!(i, Inline::Separator { .. }))));
        let mut ids: Vec<i64> = map.iter().filter(|(_, b)| !separator(b)).map(|(id, _)| *id).collect();
        ids.sort_unstable();
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
        let notes = self.notes(kind);
        let k = notes.iter().position(|&s| s == from)?;
        let to = if forward { notes.get(k + 1) } else { k.checked_sub(1).and_then(|k| notes.get(k)) }?;
        self.nav().story_edge(*to, !forward)
    }

    pub(super) fn cross_vertical(&mut self, focus: Position, x: f64, down: bool, fonts: &Fonts) -> Option<Position> {
        let ok: fn(StoryId) -> bool = match focus.story {
            StoryId::HeaderFooter(_) => |s| matches!(s, StoryId::HeaderFooter(_)),
            s if is_note(s) => |s| s == StoryId::Body || is_note(s),
            _ => return None,
        };
        let at = self.nav().line_of(focus)?;
        let cur = &at.geo.lines[at.line];
        let (top, bottom) = (cur.top, cur.bottom);
        let mut page = at.page;
        let mut first = true;
        loop {
            self.ensure_page(page, fonts);
            if let Some(geo) = self.pages.page(page, &self.index) {
                let best = geo
                    .lines
                    .iter()
                    .filter(|l| !l.repeat && ok(geo.stories[l.story].story))
                    .filter(|l| !first || if down { l.top >= bottom - 0.01 } else { l.bottom <= top + 0.01 })
                    .min_by(|a, b| {
                        let ka = if down { a.top } else { -a.bottom };
                        let kb = if down { b.top } else { -b.bottom };
                        let dx = |l: &super::geometry::GLine| (l.x0() - x).max(x - l.x1()).max(0.0);
                        ka.total_cmp(&kb).then(dx(a).total_cmp(&dx(b)))
                    });
                if let Some(l) = best {
                    let story = geo.stories[l.story].story;
                    let s = l.nearest(x)?;
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
