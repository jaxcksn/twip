//! Picture controls

use crate::docx::{ControlId, ControlKind, Inline, Lock};
use crate::font::Fonts;
use crate::layout::{ParaId, Unit};

use super::Editor;
use super::commands::Outcome;
use super::position::Position;
use super::selection::{ObjectRef, Selection};
use super::step::Step;

impl Editor {
    /// The innermost picture control holding the inline at `offset` in `para`: an open inline one, else the
    /// paragraph's.
    pub(crate) fn picture_control_in(&self, para: ParaId, offset: crate::layout::Unit) -> Option<ControlId> {
        let p = self.index.para(&self.doc, para)?;
        let picture = |c: &ControlId| self.doc.controls.get(c).is_some_and(|k| k.props.kind == ControlKind::Picture);
        let mut open: Vec<ControlId> = Vec::new();
        let mut at = 0;
        for i in &p.inlines {
            let n = i.units();
            if n > 0 && (offset.0 as usize) < at + n {
                break;
            }
            match i {
                Inline::ControlStart(c) => open.push(*c),
                Inline::ControlEnd(c) => {
                    if let Some(k) = open.iter().rposition(|x| x == c) {
                        open.truncate(k);
                    }
                }
                _ => {}
            }
            at += n;
        }
        open.iter().rev().copied().find(picture).or_else(|| p.controls.iter().rev().copied().find(picture))
    }

    /// The picture control holding an object's picture.
    pub(crate) fn picture_control_at(&self, o: ObjectRef) -> Option<ControlId> {
        match o {
            ObjectRef::Inline { para, offset } => {
                let p = self.index.para(&self.doc, para)?;
                matches!(super::stories::inline_at(p, offset), Some(Inline::Picture(..))).then(|| self.picture_control_in(para, offset)).flatten()
            }
            ObjectRef::Floating(f) => {
                if let Some(c) = self.float_of(f).and_then(|fl| fl.picture_control(&self.doc.controls)) {
                    return Some(c);
                }
                let para = self.index.float_anchor(f)?;
                let p = self.index.para(&self.doc, para)?;
                let k = p.inlines.iter().position(|i| matches!(i, Inline::Float(x, _) if crate::layout::FloatId(x.id) == f))?;
                let at: usize = p.inlines[..k].iter().map(|i| i.units()).sum();
                self.picture_control_in(para, crate::layout::Unit(at as u32))
            }
        }
    }

    /// Control `c` keeps its picture: only filling or setting it changes the picture. Its content is locked, or its
    /// tag starts with one of the protected prefixes.
    pub(crate) fn control_protected(&self, c: ControlId) -> bool {
        let Some(k) = self.doc.controls.get(&c) else { return false };
        if matches!(k.props.lock, Lock::ContentLocked | Lock::Both) {
            return true;
        }
        let tag = k.props.tag.as_deref().unwrap_or("");
        self.protected_prefixes.iter().any(|p| tag.get(..p.len()).is_some_and(|h| h.eq_ignore_ascii_case(p)))
    }

    pub(super) fn set_protected_picture_controls(&mut self, p: crate::api::command::ProtectedPictureControls) -> Outcome {
        let mut prefixes: Vec<String> = p.tag_prefixes.iter().map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect();
        prefixes.sort();
        prefixes.dedup();
        if prefixes == self.protected_prefixes {
            return Outcome::Unchanged("same prefixes");
        }
        self.protected_prefixes = prefixes;
        Outcome::Moved
    }

    /// A protected control that held a picture before this transaction lost it or holds another: it is still there
    /// without it, or, outside design mode, it is gone.
    pub(super) fn loses_protected_picture(&self) -> bool {
        let before: Vec<(usize, ControlId)> =
            self.doc.controlled.iter().flat_map(|(i, cs)| cs.iter().map(move |c| (*i, *c))).filter(|(_, c)| self.control_protected(*c)).collect();
        if before.is_empty() {
            return false;
        }
        let now = self.doc.picture_controls();
        before.iter().any(|&(i, c)| {
            let held: Vec<usize> = now.iter().filter(|(_, x)| *x == c).map(|(k, _)| *k).collect();
            match held.is_empty() {
                true => self.doc.controls.contains_key(&c) || self.mode != super::Mode::Design,
                false => !held.contains(&i),
            }
        })
    }

    /// The picture control the picture at `para`/`offset` is all of, and whether it is an inline control (else the
    /// paragraph's).
    pub(super) fn whole_picture_control(&self, para: ParaId, offset: Unit) -> Option<(ControlId, bool)> {
        let c = self.picture_control_in(para, offset)?;
        let p = self.index.para(&self.doc, para)?;
        let k = picture_index(p, offset)?;
        let inline = k > 0
            && matches!(p.inlines[k - 1], Inline::ControlStart(x) if x == c)
            && matches!(p.inlines.get(k + 1), Some(Inline::ControlEnd(x)) if *x == c);
        let block = p.controls.contains(&c) && super::step::para_len(p) == Unit(1);
        (inline || block).then_some((c, inline))
    }

    /// Takes the picture at `para`/`offset` out with its whole control `c`, putting `first` at the paragraph's start.
    pub(super) fn lift_picture_control(
        &mut self,
        tx: &mut super::Tx,
        para: ParaId,
        offset: Unit,
        c: ControlId,
        inline: bool,
        first: Option<Inline>,
    ) -> bool {
        let Some(p) = self.index.para(&self.doc, para).cloned() else { return false };
        if inline {
            let inlines: Vec<Inline> =
                p.inlines.iter().filter(|i| !matches!(i, Inline::ControlStart(x) | Inline::ControlEnd(x) if *x == c)).cloned().collect();
            let lifted = self.step(tx, Step::Restyle { para, inlines }).is_ok()
                && self.step(tx, Step::Text { para, range: offset..Unit(offset.0 + 1), with: Vec::new(), edges: Default::default() }).is_ok();
            let Some(first) = first else { return lifted };
            let Some(mut inlines) = self.index.para(&self.doc, para).map(|p| p.inlines.clone()) else { return false };
            inlines.insert(0, first);
            lifted && self.step(tx, Step::Restyle { para, inlines }).is_ok()
        } else {
            let Some(slot) = self.index.para_slot(para) else { return false };
            let mut q = p;
            q.controls.retain(|x| *x != c);
            q.inlines.retain(|i| !matches!(i, Inline::Picture(..)));
            if let Some(first) = first {
                q.inlines.insert(0, first);
            }
            let with = vec![crate::docx::Block::Para(q)];
            self.step(tx, Step::Blocks { container: slot.container, range: slot.block..slot.block + 1, with }).is_ok()
        }
    }

    /// Moves the picture at `para`/`offset` that is all of a picture control, with its control, to `drop`: `None`
    /// when the picture isn't all of a control.
    pub(super) fn move_picture_control(&mut self, para: ParaId, offset: Unit, drop: Position, fonts: &Fonts) -> Option<Outcome> {
        let (c, inline) = self.whole_picture_control(para, offset)?;
        let p = self.index.para(&self.doc, para)?;
        let picture = p.inlines[picture_index(p, offset)?].clone();
        let (to, at) = drop.para_offset()?;
        if to == para && (at == offset || at.0 == offset.0 + 1) {
            return Some(Outcome::Unchanged("dropped on itself"));
        }
        if self.picture_control_in(to, at).is_some() || self.index.para(&self.doc, to).is_some_and(|q| !q.controls.is_empty() && to != para) {
            return Some(Outcome::Unchanged("can't drop inside a control"));
        }
        let mut tx = self.begin("Move");
        let place = |e: &mut Self, tx: &mut super::Tx| -> bool {
            let with = vec![Inline::ControlStart(c), picture.clone(), Inline::ControlEnd(c)];
            e.step(tx, Step::Text { para: to, range: at..at, with, edges: Default::default() }).is_ok()
        };
        let after = to == para && at.0 > offset.0;
        #[allow(clippy::if_same_then_else, reason = "the order differs: each step moves what the other one sees")]
        let ok = if after {
            place(self, &mut tx) && self.lift_picture_control(&mut tx, para, offset, c, inline, None)
        } else {
            self.lift_picture_control(&mut tx, para, offset, c, inline, None) && place(self, &mut tx)
        };
        if !ok {
            self.abort(tx);
            return Some(Outcome::Unchanged("can't move it there"));
        }
        let landed = if after && inline { Unit(at.0 - 1) } else { at };
        self.selection = Selection::Object(ObjectRef::Inline { para: to, offset: landed });
        Some(self.commit(tx, None, fonts))
    }

    /// The picture in control `c`, as an object selection.
    pub(crate) fn picture_of_control(&self, c: ControlId) -> Option<ObjectRef> {
        for para in self.index.para_ids().collect::<Vec<_>>() {
            let Some(p) = self.index.para(&self.doc, para) else { continue };
            let mut at = 0u32;
            for i in &p.inlines {
                if matches!(i, Inline::Picture(..)) && self.picture_control_in(para, crate::layout::Unit(at)) == Some(c) {
                    return Some(ObjectRef::Inline { para, offset: crate::layout::Unit(at) });
                }
                at += i.units() as u32;
            }
        }
        None
    }

    pub(super) fn activate_picture_control_cmd(&mut self, element: Option<&crate::api::value::ElementId>) -> Outcome {
        if let Some(id) = element {
            let not_one = || Outcome::Refused(crate::api::error::Error::invalid("not a picture control"));
            let Ok((crate::docx::ElementId::Control(c), _)) = self.element(id) else { return not_one() };
            let Some(o) = self.picture_of_control(c) else { return not_one() };
            self.set_selection(Selection::Object(o));
        }
        if !self.activate_picture_control() {
            return Outcome::unavailable("no picture control selected");
        }
        if element.is_some() { Outcome::Moved } else { Outcome::Unchanged("picture control activated") }
    }

    /// The selected picture control shows its placeholder.
    pub(crate) fn selected_picture_unfilled(&self) -> bool {
        self.selected_picture_control().and_then(|c| self.doc.controls.get(&c)).is_some_and(|k| k.props.showing_placeholder)
    }

    /// The picture control holding the selected picture.
    pub(crate) fn selected_picture_control(&self) -> Option<ControlId> {
        let Selection::Object(o) = self.selection else { return None };
        self.picture_control_at(o)
    }

    pub(crate) fn set_activation_suppressed(&mut self, on: bool) {
        self.activation_suppressed = on;
    }

    /// Asks the application to act on the selected picture control.
    pub(crate) fn activate_picture_control(&mut self) -> bool {
        match self.selected_picture_control() {
            Some(control) => {
                self.events.push(super::input::Notice::PictureControl { control });
                true
            }
            None => false,
        }
    }

    /// Fills pictures with `data`, fitted to their boxes; `unwrap` takes away the picture controls around them that
    /// can be deleted, leaving plain pictures.
    pub(super) fn fill_images(
        &mut self,
        images: &[usize],
        data: Vec<u8>,
        size: (f64, f64),
        label: &'static str,
        unwrap: bool,
        fonts: &Fonts,
    ) -> Outcome {
        let (iw, ih) = size;
        let mut restyles: Vec<(ParaId, Vec<Inline>)> = Vec::new();
        for para in self.index.para_ids().collect::<Vec<_>>() {
            let Some(p) = self.index.para(&self.doc, para) else { continue };
            if !p.inlines.iter().any(|i| matches!(i, Inline::Picture(pic, _) if pic.image.is_some_and(|k| images.contains(&k)))) {
                continue;
            }
            let mut inlines = p.inlines.clone();
            for i in inlines.iter_mut() {
                if let Inline::Picture(pic, _) = i
                    && pic.image.is_some_and(|k| images.contains(&k))
                    && iw > 0.0
                    && ih > 0.0
                {
                    let s = (pic.w / iw).min(pic.h / ih);
                    (pic.w, pic.h) = (((iw * s) * 20.0).round() / 20.0, ((ih * s) * 20.0).round() / 20.0);
                }
            }
            if unwrap {
                let mut open: Vec<crate::docx::ControlId> = Vec::new();
                let mut gone: Vec<crate::docx::ControlId> = Vec::new();
                for i in &inlines {
                    match i {
                        Inline::ControlStart(c) => open.push(*c),
                        Inline::ControlEnd(c) => open.retain(|x| x != c),
                        Inline::Picture(pic, _) if pic.image.is_some_and(|k| images.contains(&k)) => gone.extend(open.iter().filter(|c| {
                            self.doc.controls.get(c).is_some_and(|k| k.props.kind == crate::docx::ControlKind::Picture && k.props.lock.deletable())
                        })),
                        _ => {}
                    }
                }
                inlines.retain(|i| !matches!(i, Inline::ControlStart(c) | Inline::ControlEnd(c) if gone.contains(c)));
            }
            restyles.push((para, inlines));
        }
        let mut tx = self.begin(label);
        for &index in images {
            if self.step(&mut tx, Step::ImageData { index, data: data.clone(), filled: true }).is_err() {
                self.abort(tx);
                return Outcome::Unchanged("fill failed");
            }
        }
        for (para, inlines) in restyles {
            if self.step(&mut tx, Step::Restyle { para, inlines }).is_err() {
                self.abort(tx);
                return Outcome::Unchanged("fill failed");
            }
        }
        self.commit(tx, None, fonts)
    }
}

/// The index of the picture at `offset` in `p`'s inlines.
fn picture_index(p: &crate::docx::Paragraph, offset: Unit) -> Option<usize> {
    let mut at = 0;
    p.inlines.iter().position(|i| {
        let hit = at == offset.0 as usize && matches!(i, Inline::Picture(..));
        at += i.units();
        hit
    })
}
