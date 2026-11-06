//! Text drag and drop

use std::collections::HashMap;
use std::rc::Rc;

use crate::docx::review::{MarkKind, RangeKind, RangeMark, RevisionId, RevisionMark, RunRevisions};
use crate::docx::{Block, Inline, Run, RunStyle};
use crate::font::Fonts;
use crate::layout::{ParaId, Unit};

use super::Editor;
use super::commands::Outcome;
use super::input::Request;
use super::position::Position;
use super::selection::Selection;
use super::step::Step;
use super::text::UnitText;

#[derive(Clone, Copy, Debug)]
pub(super) struct TextDrag {
    pub start: (f64, f64),
    pub press: Position,
    pub moved: bool,
    pub copy: bool,
    pub drop: Option<Position>,
}

impl Editor {
    pub(super) fn inside_selection(&self, p: &Position) -> bool {
        let Selection::Text { .. } = self.selection else { return false };
        let Some((a, b)) = self.selection.ordered(&self.index) else { return false };
        self.index.cmp(&a, p) == std::cmp::Ordering::Less && self.index.cmp(p, &b) == std::cmp::Ordering::Less
    }

    pub(super) fn text_drag_move(&mut self, x: f64, y: f64, ctrl: bool, fonts: &Fonts) {
        let Some(mut d) = self.text_drag else { return };
        let tol = 2.25 / self.viewport.zoom.max(0.1);
        if !d.moved && ((x - d.start.0).abs() > tol || (y - d.start.1).abs() > tol) {
            d.moved = true;
        }
        if d.moved {
            d.drop = self.hit(x, y, fonts);
            d.copy |= ctrl;
        }
        self.text_drag = Some(d);
    }

    pub(super) fn text_drag_release(&mut self, fonts: &Fonts) -> Option<Outcome> {
        let d = self.text_drag.take()?;
        self.requests.push(Request::ReleasePointer);
        let object = match self.selection {
            s @ Selection::Object(super::selection::ObjectRef::Inline { para, offset }) if d.moved => {
                let at = |u: u32| Position::text(d.press.story, para, Unit(u));
                self.selection = Selection::range(at(offset.0), at(offset.0 + 1), super::selection::Granularity::Char);
                Some(s)
            }
            _ => None,
        };
        if !d.moved {
            if !matches!(self.selection, Selection::Object(_)) {
                self.set_selection(Selection::Caret(d.press));
            }
            return Some(Outcome::Moved);
        }
        let restore = |e: &mut Self| {
            if let Some(o) = object {
                e.selection = o;
            }
        };
        let Some(drop) = d.drop else {
            restore(self);
            return Some(Outcome::Unchanged("no drop place"));
        };
        let Some((a, b)) = self.selection.ordered(&self.index) else { return Some(Outcome::Unchanged("no selection")) };
        let in_box = match drop.story {
            super::position::StoryId::TextBox(f) => self.index.float_anchor(f).and_then(|para| {
                let p = self.index.para(&self.doc, para)?;
                let k = p.inlines.iter().position(|i| matches!(i, crate::docx::Inline::Float(x, _) if crate::layout::FloatId(x.id) == f))?;
                let u: u32 = p.inlines[..k].iter().map(|i| i.units() as u32).sum();
                let story = self.index.para_slot(para).map(|s| self.index.story(s.container))?;
                Some(Position::text(story, para, crate::layout::Unit(u)))
            }),
            _ => None,
        };
        let inside = |p: &Position| self.index.cmp(&a, p) != std::cmp::Ordering::Greater && self.index.cmp(p, &b) != std::cmp::Ordering::Greater;
        if inside(&drop) || in_box.is_some_and(|p| inside(&p)) {
            restore(self);
            return Some(Outcome::Unchanged("dropped on the selection"));
        }
        let mut frag = self.copy_fragment()?;
        // Whole controls dragged go with their properties, as copied ones do.
        frag.controls = super::clipboard::controls_of(&frag.blocks, &self.doc);
        let sel = self.selection;
        let moving = !d.copy && self.tracking() && self.moves_whole();
        let mut marks = (None, None);
        let mut placed: Option<(Position, Position)> = None;
        let paste = |e: &mut Self, placed: &mut Option<(Position, Position)>| {
            e.set_selection(Selection::Caret(drop));
            let start = drop;
            let ok = matches!(e.paste_fragment(&frag, fonts), Outcome::Changed(_));
            if ok && let Some(end) = e.selection.focus() {
                *placed = Some((start, end));
            }
            ok
        };
        let delete = |e: &mut Self, placed: &mut Option<(Position, Position)>| {
            e.set_selection(sel);
            if !moving && object.is_none() {
                e.smart_cut();
            }
            let ok = matches!(e.delete(super::commands::Dir::Forward, false, fonts), Outcome::Changed(_));
            if ok && let Some((s, t)) = placed.as_mut() {
                for m in &e.last_maps {
                    (*s, *t) = (m.apply(*s), m.apply(*t));
                }
            }
            ok
        };
        let after = self.index.cmp(&drop, &b) == std::cmp::Ordering::Greater;
        let mut paste = |e: &mut Self, placed: &mut Option<(Position, Position)>| {
            let ok = paste(e, placed);
            if ok {
                marks.0 = e.recorded.0.clone();
            }
            ok
        };
        let mut delete = |e: &mut Self, placed: &mut Option<(Position, Position)>| {
            let ok = delete(e, placed);
            if ok {
                marks.1 = e.recorded.1.clone();
            }
            ok
        };
        self.begin_batch(Some(if d.copy { "Copy" } else { "Move" }.into()));
        #[allow(clippy::if_same_then_else, reason = "the order differs: each call moves what the other one sees")]
        let steps = if d.copy {
            paste(self, &mut placed) as usize
        } else if after {
            paste(self, &mut placed) as usize + delete(self, &mut placed) as usize
        } else {
            delete(self, &mut placed) as usize + paste(self, &mut placed) as usize
        };
        if steps == 0 {
            self.end_batch(false, fonts);
            restore(self);
            return Some(Outcome::Unchanged("nothing dropped"));
        }
        if moving && let (Some(to), Some(from)) = &marks {
            self.record_move(to, from, fonts);
        }
        if object.is_some()
            && let Some((para, offset)) = placed.and_then(|(s, _)| s.para_offset())
        {
            self.selection = Selection::Object(super::selection::ObjectRef::Inline { para, offset });
        } else if let Some((s, t)) = placed {
            self.selection = Selection::range(s, t, super::selection::Granularity::Char);
        }
        Some(self.end_batch(true, fonts))
    }

    pub(super) fn paste_moved(&mut self, f: &super::clipboard::Fragment, out: &Outcome, fonts: &Fonts) {
        let Some((id, from)) = self.cut_move.take() else { return };
        if id != f.id || !matches!(out, Outcome::Changed(_)) {
            return;
        }
        let Some(to) = self.recorded.0.clone() else { return };
        let sel = self.selection;
        self.record_move(&to, &from, fonts);
        self.selection = sel;
    }

    pub(super) fn moves_whole(&mut self) -> bool {
        let Some((a, b)) = self.selection.ordered(&self.index) else { return false };
        let (Some((pa, oa)), Some((pb, ob))) = (a.para_offset(), b.para_offset()) else { return false };
        let whole = if pa == pb {
            let Some(t) = self.index.para(&self.doc, pa).map(UnitText::of) else { return false };
            let (s, e) = t.sentence_at(oa);
            s == oa && ob > oa && ob <= e && t.text.chars().skip(ob.0 as usize).take((e.0 - ob.0) as usize).all(char::is_whitespace)
        } else {
            oa == Unit(0) && ob == Unit(0) && self.index.para_slot(pa).map(|s| s.container) == self.index.para_slot(pb).map(|s| s.container)
        };
        whole && self.selected_ranges().into_iter().all(|(para, s, e)| self.index.para(&self.doc, para).is_some_and(|p| !has_revisions(p, s, e)))
    }

    pub(super) fn record_move(&mut self, to: &Rc<RevisionMark>, from: &Rc<RevisionMark>, fonts: &Fonts) -> bool {
        let id = self.doc.review_ids.next_revision_id();
        let name: Rc<str> = format!("move{id}").into();
        let mark = |m: &RevisionMark, kind: MarkKind, id: i64| {
            Rc::new(RevisionMark { id: RevisionId(id), kind, move_name: Some(name.clone()), ..m.clone() })
        };
        let to_mark = mark(to, MarkKind::MoveTo, id);
        let from_mark = mark(from, MarkKind::MoveFrom, self.doc.review_ids.next_revision_id());
        let remap = |r: &Run| -> Option<Run> {
            let ins = r.rev.ins.as_ref().filter(|m| Rc::ptr_eq(m, to)).map(|_| to_mark.clone());
            let del = r.rev.del.as_ref().filter(|m| Rc::ptr_eq(m, from)).map(|_| from_mark.clone());
            if ins.is_none() && del.is_none() {
                return None;
            }
            let rev = RunRevisions { ins: ins.or_else(|| r.rev.ins.clone()), del: del.or_else(|| r.rev.del.clone()), fmt: r.rev.fmt.clone() };
            Some(Rc::new(RunStyle { rev, ..(**r).clone() }))
        };
        fn walk(blocks: &[Block], out: &mut Vec<ParaId>) {
            for b in blocks {
                match b {
                    Block::Para(p) => out.push(ParaId(p.id)),
                    Block::Table(t) => t.rows.iter().flat_map(|r| r.cells.iter()).for_each(|c| walk(&c.blocks, out)),
                }
            }
        }
        let mut paras = Vec::new();
        self.doc.sections.iter().for_each(|s| walk(&s.blocks, &mut paras));
        let mut ends: [Option<((ParaId, Unit), (ParaId, Unit))>; 2] = [None, None];
        let mut cache: HashMap<*const RunStyle, Option<Run>> = HashMap::new();
        let mut steps = Vec::new();
        for para in paras {
            let Some(p) = self.index.para(&self.doc, para) else { continue };
            let mut at = 0u32;
            let mut changed = false;
            let mut inlines = Vec::with_capacity(p.inlines.len());
            for i in &p.inlines {
                let n = i.units() as u32;
                let new = i.run().and_then(|r| cache.entry(Rc::as_ptr(r)).or_insert_with(|| remap(r)).clone());
                match new {
                    Some(r) => {
                        let k = usize::from(r.rev.del.as_ref().is_some_and(|m| Rc::ptr_eq(m, &from_mark)));
                        let (first, last) = ((para, Unit(at)), (para, Unit(at + n)));
                        ends[k] = Some(ends[k].map_or((first, last), |(f, _)| (f, last)));
                        inlines.push(super::format::with_run(i, &r));
                        changed = true;
                    }
                    None => inlines.push(i.clone()),
                }
                at += n;
            }
            if changed {
                steps.push(Step::Restyle { para, inlines });
            }
            if let Some(mark) = remap(&p.mark) {
                steps.push(Step::Para { para, style: p.style.clone(), mark, list: p.list.clone() });
            }
        }
        let [Some(to_end), Some(from_end)] = ends else { return false };
        let range = |kind: RangeKind, id: i64| Inline::Range(RangeMark { kind, id, name: Some(name.clone()) });
        let (to_id, from_id) = (self.doc.review_ids.next_revision_id(), self.doc.review_ids.next_revision_id());
        for ((para, u), marker) in [
            (to_end.0, range(RangeKind::MoveToStart, to_id)),
            (to_end.1, range(RangeKind::MoveToEnd, to_id)),
            (from_end.0, range(RangeKind::MoveFromStart, from_id)),
            (from_end.1, range(RangeKind::MoveFromEnd, from_id)),
        ] {
            steps.push(Step::Text { para, range: u..u, with: vec![marker], edges: Default::default() });
        }
        let mut tx = self.begin("Move");
        tx.tracked = false;
        for s in steps {
            if self.step(&mut tx, s).is_err() {
                self.abort(tx);
                return false;
            }
        }
        matches!(self.commit(tx, None, fonts), Outcome::Changed(_))
    }
}

fn has_revisions(p: &crate::docx::Paragraph, a: Unit, b: Unit) -> bool {
    let mut at = 0u32;
    p.inlines.iter().any(|i| {
        let n = i.units() as u32;
        let inside = at < b.0 && at + n > a.0;
        at += n;
        inside && i.run().is_some_and(|r| r.rev.ins.is_some() || r.rev.del.is_some())
    })
}
