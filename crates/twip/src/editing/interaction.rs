//! Text drag and drop

use std::collections::{HashMap, HashSet};
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
    /// A right-button drag: its release opens the drop menu.
    pub menu: bool,
}

/// The selection dragged out of the editor: its content, as Copy takes it, and whether the drag may move it (the
/// source can be deleted).
#[derive(Clone, Debug)]
pub struct DragOut {
    /// The content, as Copy takes it.
    pub fragment: super::clipboard::Fragment,
    /// Its plain text.
    pub text: String,
    /// Its HTML.
    pub html: String,
    /// The drag may move it: the selection can be deleted.
    pub can_move: bool,
}

impl Editor {
    /// Turns text drag and drop on or off (on by default; Word's "Allow text to be dragged and dropped"). Off, a press
    /// inside the selection places the caret, and the selection can't be dragged out.
    pub fn set_text_drag_and_drop(&mut self, on: bool) {
        self.text_drag_off = !on;
        if !on {
            self.drop_menu = None;
            self.drag_out = None;
            if self.text_drag.take().is_some() {
                self.requests.push(Request::ReleasePointer);
            }
        }
    }

    /// Whether text drag and drop is on.
    pub fn text_drag_and_drop(&self) -> bool {
        !self.text_drag_off
    }

    /// Whether a text drag can start: drag and drop on, an editable document and no extend mode.
    pub(super) fn text_drags(&self) -> bool {
        !self.text_drag_off && self.mode != super::Mode::ReadOnly && self.modes == super::extend::Modes::default()
    }

    /// Starts dragging the selection out of the editor, to another application or editor: an internal text drag
    /// under way ends, and the selection's content is what the drag carries. Nothing for a caret.
    pub fn drag_out(&mut self) -> Option<DragOut> {
        if self.text_drag.take().is_some() {
            self.requests.push(Request::ReleasePointer);
        }
        if self.selection.is_collapsed() || self.text_drag_off {
            return None;
        }
        let fragment = self.clip()?;
        let source = self.selection.ordered(&self.index);
        let can_move = self.mode != super::Mode::ReadOnly
            && source.is_some_and(|(a, b)| !self.kept(crate::api::command::CommandId::Delete, &a, &b) && self.editable(&a));
        self.drag_out = Some((self.selection, self.revision, fragment.blocks.clone()));
        Some(DragOut { text: fragment.text(), html: fragment.html(), fragment, can_move })
    }

    /// A drag out of the editor came back and dropped at (`x`, `y`): a move, or with `copy` a copy, as a drag inside
    /// the editor makes it.
    pub fn drag_out_drop(&mut self, x: f64, y: f64, copy: bool, fonts: &Fonts) -> Outcome {
        if self.drag_out.take().is_none_or(|(s, r, _)| (s, r) != (self.selection, self.revision)) {
            return Outcome::Unchanged("the selection or the document changed");
        }
        let drop = self.hit(x, y, fonts);
        // Where the drag started stands for the selection's story: a picture's paragraph's, or the drop's for cells.
        let press = match self.selection {
            Selection::Object(super::selection::ObjectRef::Inline { para, offset }) => {
                let story = self.index.para_slot(para).map_or(super::position::StoryId::Body, |s| self.index.story(s.container));
                Some(Position::text(story, para, offset))
            }
            s => s.focus().or(drop),
        };
        let Some(press) = press else { return Outcome::Unchanged("no drop place") };
        self.text_drag = Some(TextDrag { start: (x, y), press, moved: true, copy, drop, menu: false });
        self.text_drag_release(fonts).unwrap_or(Outcome::Unchanged("no drag"))
    }

    /// A drag out of the editor ended elsewhere: a move (`moved`) deletes the dragged selection as one undo step,
    /// unless the selection changed since it started.
    pub fn drag_out_end(&mut self, moved: bool, fonts: &Fonts) -> Outcome {
        let carried = self.drag_out.take().filter(|(s, r, _)| (*s, *r) == (self.selection, self.revision));
        // A batched edit leaves the revision alone: the selection must still hold what the drag carried.
        let dragged = carried.is_some_and(|(.., blocks)| self.clip().is_some_and(|f| f.blocks == blocks));
        if !moved || !dragged {
            return Outcome::Unchanged("nothing moved");
        }
        // Smart cut
        if !matches!(self.selection, Selection::Object(_) | Selection::Cells { .. }) && !(self.tracking() && self.moves_whole()) {
            let dragged = self.selection;
            self.smart_cut();
            // The space it takes along may be out of reach where the dragged text is not.
            let refused = self.selection != dragged
                && self.selection.ordered(&self.index).is_some_and(|(a, b)| self.kept(crate::api::command::CommandId::Delete, &a, &b));
            if refused {
                self.selection = dragged;
            }
        }
        let delete = crate::api::command::DeleteOptions { direction: crate::api::value::Direction::Forward, unit: None };
        self.execute(crate::api::command::Command::Delete(delete), fonts)
    }

    pub(super) fn inside_selection(&self, p: &Position) -> bool {
        let Selection::Text { .. } = self.selection else { return false };
        let Some((a, b)) = self.selection.ordered(&self.index) else { return false };
        self.index.cmp(&a, p) == std::cmp::Ordering::Less && self.index.cmp(p, &b) == std::cmp::Ordering::Less
    }

    /// Whether `p` is strictly inside the selection's own range or another range of a discontiguous selection.
    pub(super) fn inside_any_range(&self, p: &Position) -> bool {
        self.inside_selection(p) || self.other_ranges().iter().any(|(a, b)| self.index.cmp(a, p).is_lt() && self.index.cmp(p, b).is_lt())
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

    /// Whether a moved drag can't drop where it is: the source can't be taken (a move) or the drop place can't take
    /// it, by protection, a lock or the mode.
    pub(super) fn drop_refused(&self, d: &TextDrag) -> bool {
        let Some(drop) = d.drop.filter(|_| d.moved) else { return false };
        let source = match self.selection {
            Selection::Object(super::selection::ObjectRef::Inline { para, offset }) => {
                Some((Position::text(d.press.story, para, offset), Position::text(d.press.story, para, Unit(offset.0 + 1))))
            }
            s => s.ordered(&self.index),
        };
        use crate::api::command::CommandId as C;
        let taken = !d.copy
            && (source.is_none_or(|(a, b)| self.kept(C::Delete, &a, &b)) || self.other_ranges().iter().any(|(a, b)| self.kept(C::Delete, a, b)));
        taken || !self.editable(&drop) || self.kept(C::Paste, &drop, &drop)
    }

    pub(super) fn text_drag_release(&mut self, fonts: &Fonts) -> Option<Outcome> {
        let d = self.text_drag.take()?;
        self.requests.push(Request::ReleasePointer);
        if self.drop_refused(&d) {
            return Some(Outcome::Unchanged("kept"));
        }
        // A picture that is all of a picture control moves with its control.
        if let (Selection::Object(super::selection::ObjectRef::Inline { para, offset }), true, false, Some(drop)) =
            (self.selection, d.moved, d.copy, d.drop)
            && let Some(out) = self.move_picture_control(para, offset, drop, fonts)
        {
            return Some(out);
        }
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
        let Some(drop) = d.drop else {
            if let Some(o) = object {
                self.selection = o;
            }
            return Some(Outcome::Unchanged("no drop place"));
        };
        Some(self.drop_selection(drop, d.copy, object, fonts))
    }

    /// Moves (or copies) the selected text to `drop` as a drag and drop does: one undo step, a tracked move when
    /// changes are tracked, the moved text selected after. `object`: the inline object selection the text stands for.
    /// Every range of a discontiguous selection moves or copies, as Cut (or Copy) and Paste would: the ranges go
    /// first, wherever they are, and the drop place follows their removal.
    pub(super) fn drop_selection(&mut self, drop: Position, copy: bool, object: Option<Selection>, fonts: &Fonts) -> Outcome {
        let restore = |e: &mut Self| {
            if let Some(o) = object {
                e.selection = o;
            }
        };
        // Selected cells and a block are copied, never moved: their fragment is what a drop pastes.
        let source = match self.selection {
            Selection::Cells { .. } | Selection::Block { .. } if !copy => return Outcome::Unchanged("the selection can't be moved"),
            Selection::Cells { .. } | Selection::Block { .. } => None,
            s => match s.ordered(&self.index) {
                Some(r) => Some(r),
                None => return Outcome::Unchanged("no selection"),
            },
        };
        if !self.other_cells().is_empty() {
            restore(self);
            return Outcome::unavailable("cells don't move by dragging");
        }
        let pieces = if source.is_none() { self.selected_ranges() } else { Vec::new() };
        // Where the story dropped in is hosted: a text box's float, a note's reference.
        let in_box = match drop.story {
            super::position::StoryId::TextBox(f) => self.index.float_anchor(f).and_then(|para| {
                let p = self.index.para(&self.doc, para)?;
                let k = p.inlines.iter().position(|i| matches!(i, crate::docx::Inline::Float(x, _) if crate::layout::FloatId(x.id) == f))?;
                let u: u32 = p.inlines[..k].iter().map(|i| i.units() as u32).sum();
                let story = self.index.para_slot(para).map(|s| self.index.story(s.container))?;
                Some(Position::text(story, para, crate::layout::Unit(u)))
            }),
            super::position::StoryId::Footnote(id) => self.note_reference(crate::docx::NoteKind::Foot, id).map(|(at, _)| at),
            super::position::StoryId::Endnote(id) => self.note_reference(crate::docx::NoteKind::End, id).map(|(at, _)| at),
            _ => None,
        };
        // Discontiguous selection
        let multi = self.is_discontiguous();
        let ranges = match source {
            Some(_) if multi => self.selection_ranges(),
            Some(r) => vec![r],
            None => Vec::new(),
        };
        let inside = |p: &Position| {
            ranges.iter().any(|(a, b)| self.index.cmp(a, p) != std::cmp::Ordering::Greater && self.index.cmp(p, b) != std::cmp::Ordering::Greater)
                || p.para_offset().is_some_and(|(q, u)| pieces.iter().any(|&(r, s, t)| r == q && s <= u && u <= t))
        };
        if inside(&drop) || in_box.is_some_and(|p| inside(&p)) {
            restore(self);
            return Outcome::Unchanged("dropped on the selection");
        }
        // What a drop pastes is what Copy takes.
        let Some(mut frag) = self.clip() else { return Outcome::Unchanged("nothing to move") };
        frag.moved = !copy;
        let sel = self.selection;
        let moving = !copy && !multi && self.tracking() && self.moves_whole();
        let mut marks = (Vec::new(), Vec::new());
        let mut placed: Option<(Position, Position)> = None;
        let paste = |e: &mut Self, placed: &mut Option<(Position, Position)>, drop: Position| {
            e.set_selection(Selection::Caret(drop));
            let start = e.selection.focus().unwrap_or(drop);
            // The spaces a smart paste adds around the text aren't the text moved.
            let spaces = match frag.blocks.as_slice() {
                [Block::Para(p)] if frag.open_end => e.smart_spaces(&UnitText::of(p).text),
                _ => (false, false),
            };
            let nudge = |p: Position, by: i32| match p.para_offset() {
                Some((para, u)) => Position::text(p.story, para, Unit(u.0.saturating_add_signed(by))),
                None => p,
            };
            let ok = matches!(e.paste_fragment(&frag, fonts), Outcome::Changed(_));
            if ok && let Some(end) = e.selection.focus() {
                *placed = Some((nudge(start, i32::from(spaces.0)), nudge(end, -i32::from(spaces.1))));
            }
            ok
        };
        let delete = |e: &mut Self, placed: &mut Option<(Position, Position)>| {
            e.set_selection(sel);
            if !moving && !multi && object.is_none() {
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
        let after = source.is_some_and(|(_, b)| self.index.cmp(&drop, &b) == std::cmp::Ordering::Greater);
        let mut paste = |e: &mut Self, placed: &mut Option<(Position, Position)>, drop: Position| {
            let ok = paste(e, placed, drop);
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
        self.begin_batch(Some(if copy { "Copy" } else { "Move" }.into()));
        // Paste and delete, together or not at all
        #[allow(clippy::if_same_then_else, reason = "the order differs: each call moves what the other one sees")]
        let done = if copy {
            paste(self, &mut placed, drop)
        } else if multi {
            delete(self, &mut placed) && {
                let drop = self.last_maps.iter().fold(drop, |p, m| m.apply(p));
                paste(self, &mut placed, drop)
            }
        } else if after {
            paste(self, &mut placed, drop) && delete(self, &mut placed)
        } else {
            delete(self, &mut placed) && paste(self, &mut placed, drop)
        };
        if !done {
            self.end_batch(false, fonts);
            restore(self);
            return Outcome::Unchanged("nothing dropped");
        }
        if moving && !marks.0.is_empty() && !marks.1.is_empty() {
            self.record_move(&marks.0, &marks.1, fonts);
        }
        if object.is_some()
            && let Some((para, offset)) = placed.and_then(|(s, _)| s.para_offset())
        {
            self.selection = Selection::Object(super::selection::ObjectRef::Inline { para, offset });
        } else if let Some((s, t)) = placed {
            self.selection = self.normalize(Selection::range(s, t, super::selection::Granularity::Char));
        }
        self.end_batch(true, fonts)
    }

    pub(super) fn paste_moved(&mut self, f: &super::clipboard::Fragment, out: &Outcome, fonts: &Fonts) {
        let Some((id, from)) = self.cut_move.take() else { return };
        if id != f.id || !matches!(out, Outcome::Changed(_)) {
            return;
        }
        let to = self.recorded.0.clone();
        if to.is_empty() {
            return;
        }
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

    pub(super) fn record_move(&mut self, to: &[Rc<RevisionMark>], from: &[Rc<RevisionMark>], fonts: &Fonts) -> bool {
        let (Some(to_first), Some(from_first)) = (to.first(), from.first()) else { return false };
        let id = self.doc.review_ids.next_revision_id();
        let name: Rc<str> = format!("move{id}").into();
        let mark = |m: &RevisionMark, kind: MarkKind, id: i64| {
            Rc::new(RevisionMark { id: RevisionId(id), kind, move_name: Some(name.clone()), ..m.clone() })
        };
        let to_mark = mark(to_first, MarkKind::MoveTo, id);
        let from_mark = mark(from_first, MarkKind::MoveFrom, self.doc.review_ids.next_revision_id());
        let set = |of: &[Rc<RevisionMark>]| of.iter().map(Rc::as_ptr).collect::<HashSet<_>>();
        let (to, from) = (set(to), set(from));
        let remap = |r: &Run| -> Option<Run> {
            let ins = r.rev.ins.as_ref().filter(|m| to.contains(&Rc::as_ptr(m))).map(|_| to_mark.clone());
            let del = r.rev.del.as_ref().filter(|m| from.contains(&Rc::as_ptr(m))).map(|_| from_mark.clone());
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
        // Word records moves in every story: headers, footers and notes too.
        let mut paras = Vec::new();
        super::elements::stories(&self.doc, &self.index).into_iter().for_each(|(_, blocks)| walk(blocks, &mut paras));
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
