//! Content controls bound to custom XML data: an edited control writes its node, a changed node shows in its controls

use crate::docx::binding::{self, Span};
use crate::docx::{ControlId, ControlKind, DataPart, Inline};
use crate::layout::{ParaId, Unit};

use super::Editor;
use super::Tx;
use super::position::Position;
use super::position::StoryId;
use super::selection::Selection;
use super::step::{self, Edges, Map, Step, StepError};
use crate::docx::run_xml;
use std::ops::Range;

fn units(inlines: &[Inline], k: usize) -> Unit {
    Unit(inlines[..k].iter().map(|i| i.units() as u32).sum())
}

/// Past this many steps, every control in a paragraph a transaction's text steps touched counts as edited.
const MAX_MAPPED: usize = 256;

/// What the transaction's text steps changed, in each paragraph as it is now: the units put in, or the point where
/// some were taken out.
fn edits(tx: &Tx) -> Vec<(ParaId, Range<u32>)> {
    if tx.maps.len() > MAX_MAPPED {
        // ponytail: whole paragraphs past the budget, as mapping each step through the later ones is quadratic
        return tx.undo.iter().filter_map(|s| if let Step::Text { para, .. } = s { Some((*para, 0..u32::MAX)) } else { None }).collect();
    }
    let mut out = Vec::new();
    for (k, m) in tx.maps.iter().enumerate() {
        let Map::Text { para, start, new_end, .. } = *m else { continue };
        let now = |u| tx.maps[k + 1..].iter().fold(Position::text(StoryId::Body, para, u), |p, m| m.apply(p)).para_offset();
        match (now(start), now(new_end)) {
            (Some((a, x)), Some((b, y))) if a == b => out.push((a, x.0..y.0.max(x.0))),
            (Some((a, x)), Some((b, y))) => out.extend([(a, x.0..u32::MAX), (b, 0..y.0)]),
            _ => {}
        }
    }
    out
}

/// The paragraphs `edits` are in.
fn edited_paras(edits: &[(ParaId, Range<u32>)]) -> Vec<ParaId> {
    let mut paras: Vec<ParaId> = edits.iter().map(|(p, _)| *p).collect();
    paras.sort_unstable();
    paras.dedup();
    paras
}

/// Content `inner` (inline indexes) of paragraph `para` holds or meets one of `edits`.
fn edited(edits: &[(ParaId, Range<u32>)], para: ParaId, inlines: &[Inline], inner: &Range<usize>) -> bool {
    let (s, e) = (units(inlines, inner.start).0, units(inlines, inner.end).0);
    edits.iter().any(|(p, r)| *p == para && if r.is_empty() { s <= r.start && r.start <= e } else { r.start < e && s < r.end })
}

impl Editor {
    /// The bound controls of paragraph `para`.
    fn bound(&self, para: ParaId) -> Vec<Span> {
        self.index.para(&self.doc, para).map_or_else(Vec::new, |p| binding::spans(&p.inlines, &self.doc.controls))
    }

    /// After an edit: each bound control edited writes its node; then, when a data part changed or a control was
    /// bound anew, every bound control shows its node (the edited ones too: a drop-down shows its item's text).
    pub(super) fn sync_bindings(&mut self, tx: &mut Tx) {
        if !self.doc.data_parts.is_empty() {
            self.write_bindings(tx);
        }
        let rebound = |s: &Step| match s {
            Step::DataPart { .. } => true,
            // The inverse of a control added, or of one whose binding changed.
            Step::Control { id, control } => {
                control.as_ref().map(|c| &c.props.binding) != self.doc.controls.get(id).map(|c| &c.props.binding)
                    && self.doc.controls.get(id).is_some_and(|c| c.props.binding.is_some())
            }
            _ => false,
        };
        if !tx.undo.iter().any(rebound) {
            return;
        }
        // Each paragraph's from its end, so earlier ranges stay put; the parts parsed once for them all.
        let mut shows: Vec<(ParaId, Span, String)> = Vec::new();
        let parts = binding::Parts::new(&self.doc.data_parts);
        for para in self.index.para_ids() {
            let mut found = self.bound(para);
            found.sort_by_key(|s| std::cmp::Reverse(s.inner.start));
            for s in found {
                let Some(value) = s.binding.value(&parts) else { continue };
                let Some(ctl) = self.doc.controls.get(&s.control) else { continue };
                let want = binding::shown(&ctl.props, &value);
                if want == s.text || (want.is_empty() && ctl.props.showing_placeholder) {
                    continue;
                }
                shows.push((para, s, want));
            }
        }
        drop(parts);
        let from = tx.maps.len();
        for (para, s, want) in shows {
            self.show_bound(tx, para, s, want);
        }
        // The selection where the shown text put it
        let map = |p: Position| tx.maps[from..].iter().fold(p, |p, m| m.apply(p));
        self.selection = match self.selection {
            Selection::Caret(p) => Selection::Caret(map(p)),
            Selection::Text { anchor, focus, unit } => Selection::range(map(anchor), map(focus), unit),
            other => other,
        };
    }

    /// The date controls the transaction edited keep the full date (`w:fullDate`) their text now shows.
    pub(super) fn sync_dates(&mut self, tx: &mut Tx) {
        if !self.doc.controls.values().any(|c| matches!(c.props.kind, ControlKind::Date { .. })) {
            return;
        }
        let edits = edits(tx);
        let w = self.cx().w;
        let mut changes = Vec::new();
        for para in edited_paras(&edits) {
            let Some(p) = self.index.para(&self.doc, para) else { continue };
            let mut open: Vec<(ControlId, usize)> = Vec::new();
            for (k, i) in p.inlines.iter().enumerate() {
                match i {
                    Inline::ControlStart(c) => open.push((*c, k)),
                    Inline::ControlEnd(c) => {
                        let Some(at) = open.iter().rposition(|(o, _)| o == c) else { continue };
                        let (c, start) = open.remove(at);
                        if !edited(&edits, para, &p.inlines, &(start + 1..k)) {
                            continue;
                        }
                        let Some(ctl) = self.doc.controls.get(&c).filter(|x| !x.props.showing_placeholder) else { continue };
                        let text: String = p.inlines[start + 1..k]
                            .iter()
                            .filter_map(|i| match i {
                                Inline::Text(t, r) if r.rev.del.is_none() => Some(t.as_str()),
                                _ => None,
                            })
                            .collect();
                        if let Some(xml) = binding::with_full_date(&ctl.props, &text, &w) {
                            let mut ctl = ctl.clone();
                            ctl.props.kind_xml = Some(xml);
                            changes.push((c, ctl));
                        }
                    }
                    _ => {}
                }
            }
        }
        for (id, control) in changes {
            let _ = self.step(tx, Step::Control { id, control: Some(control) });
        }
    }

    /// The nodes of the bound controls the transaction edited, written with their text; the parts copied only when one
    /// changes. A control showing its placeholder, or one this transaction added (it shows its node), has nothing to
    /// write; one still showing its node's value as the transaction found it wasn't edited, though another control
    /// bound to the node was.
    fn write_bindings(&mut self, tx: &mut Tx) {
        let edits = edits(tx);
        let added: Vec<ControlId> =
            tx.undo.iter().filter_map(|s| if let Step::Control { id, control: None } = s { Some(*id) } else { None }).collect();
        let found = binding::Parts::new(&self.doc.data_parts);
        let mut writes: Vec<(usize, binding::Span, String)> = Vec::new();
        for para in edited_paras(&edits) {
            let Some(p) = self.index.para(&self.doc, para) else { continue };
            for s in binding::spans(&p.inlines, &self.doc.controls) {
                if !edited(&edits, para, &p.inlines, &s.inner) {
                    continue;
                }
                let Some(ctl) = self.doc.controls.get(&s.control).filter(|c| !c.props.showing_placeholder) else { continue };
                if added.contains(&s.control) {
                    continue;
                }
                let Some(k) = s.binding.part(&found) else { continue };
                let value = binding::stored(&ctl.props, &s.text);
                if s.binding.value(&found).as_deref() == Some(value.as_str()) {
                    continue;
                }
                writes.push((k, s, value));
            }
        }
        drop(found);
        let mut parts: Option<Vec<DataPart>> = None;
        for (k, s, value) in writes {
            let now = parts.as_deref().unwrap_or(&self.doc.data_parts);
            let Some(xml) = binding::write(&now[k].xml, &s.binding.path, &value) else { continue };
            let parts = parts.get_or_insert_with(|| self.doc.data_parts.clone());
            (parts[k].xml, parts[k].changed) = (xml, true);
        }
        let Some(parts) = parts else { return };
        let changed: Vec<DataPart> =
            parts.into_iter().zip(&self.doc.data_parts).filter(|(now, was)| now.xml != was.xml).map(|(now, _)| now).collect();
        for now in changed {
            let _ = self.step(tx, Step::DataPart { item_id: now.item_id.clone(), part: Some(now) });
        }
    }

    /// Control `s` of `para` shows `text`.
    fn show_bound(&mut self, tx: &mut Tx, para: ParaId, s: Span, text: String) {
        let showing = self.doc.controls.get(&s.control).is_some_and(|c| c.props.showing_placeholder);
        let Some(run) = s.run.filter(|_| !showing).or_else(|| self.control_own_run(s.control, para, s.inner.start - 1)) else { return };
        let Some(p) = self.index.para(&self.doc, para) else { return };
        let range = units(&p.inlines, s.inner.start)..units(&p.inlines, s.inner.end);
        // An empty control's text goes before its end
        let under = if range.is_empty() { step::markers_from(p, range.start, |i| matches!(i, Inline::ControlEnd(x) if *x == s.control)) } else { 0 };
        let with = if text.is_empty() { Vec::new() } else { vec![Inline::Text(text, run)] };
        if self.step(tx, Step::Text { para, range, with, edges: Edges { under, ..Edges::default() } }).is_ok() {
            self.placeholder_off(tx, s.control);
        }
    }

    /// A selection inside a text control showing its placeholder takes the whole placeholder, which goes (it isn't
    /// content, so untracked): what replaces the selection goes inside the control, which then shows it.
    pub(super) fn take_placeholder(&mut self, tx: &mut Tx) -> Result<(), StepError> {
        let Some((c, at, para, s, e)) = self.placeholder_here() else { return Ok(()) };
        let start = self.index.para(&self.doc, para).and_then(|p| p.inlines.iter().position(|i| matches!(i, Inline::ControlStart(x) if *x == c)));
        if self.pending.is_none() {
            self.pending = start.and_then(|k| self.control_own_run(c, para, k));
        }
        self.apply_step(tx, Step::Text { para, range: Unit(s)..Unit(e), with: Vec::new(), edges: Edges::default() })?;
        tx.placeholder = Some((para, Unit(s), c));
        self.selection = Selection::Caret(Position::text(at.story, para, Unit(s)));
        Ok(())
    }

    /// The text control showing its placeholder the selection is in (a caret inside it or at its start, or a range
    /// within it): the
    /// control, the selection's start, and its paragraph and content's units.
    pub(super) fn placeholder_here(&self) -> Option<(ControlId, Position, ParaId, u32, u32)> {
        let (a, b) = self.selection.ordered(&self.index)?;
        let (Some((para, ua)), Some((pb, ub))) = (a.para_offset(), b.para_offset()) else { return None };
        let p = self.index.para(&self.doc, para).filter(|_| para == pb)?;
        let typed = |c: &ControlId| {
            self.doc.controls.get(c).is_some_and(|k| {
                k.props.showing_placeholder
                    && matches!(k.props.kind, ControlKind::PlainText { .. } | ControlKind::RichText | ControlKind::Date { .. })
            })
        };
        let mut open: Vec<(ControlId, u32)> = Vec::new();
        let mut at = 0u32;
        let mut hit = None;
        for i in &p.inlines {
            match i {
                Inline::ControlStart(c) => open.push((*c, at)),
                Inline::ControlEnd(c) => {
                    let Some(k) = open.iter().rposition(|(o, _)| o == c) else { continue };
                    let (c, s) = open.remove(k);
                    // A caret at its start is inside
                    let inside = if ua == ub { s <= ua.0 && ua.0 < at } else { s <= ua.0 && ub.0 <= at };
                    if s < at && inside && typed(&c) {
                        hit = Some((c, s, at));
                        break;
                    }
                }
                _ => {}
            }
            at += i.units() as u32;
        }
        hit.map(|(c, s, e)| (c, a, para, s, e))
    }

    /// What is put in where the transaction took a placeholder goes inside its control, before its end.
    pub(super) fn inside_placeholder(&self, tx: &Tx, s: Step) -> Step {
        let Some((at_para, at, c)) = tx.placeholder else { return s };
        match s {
            Step::Text { para, range, with, edges }
                if para == at_para && range.start == at && range.end == at && edges == Edges::default() && with.iter().any(|i| i.units() > 0) =>
            {
                let under =
                    self.index.para(&self.doc, para).map_or(0, |p| step::markers_from(p, at, |i| matches!(i, Inline::ControlEnd(x) if *x == c)));
                Step::Text { para, range, with, edges: Edges { under, ..edges } }
            }
            s => s,
        }
    }

    /// The run control `c` (its start marker inline `start` of `para`) gives what replaces its placeholder
    /// ([`binding::own_run`]).
    pub(super) fn control_own_run(&self, c: ControlId, para: ParaId, start: usize) -> Option<crate::docx::Run> {
        let (ctl, p) = (self.doc.controls.get(&c)?, self.index.para(&self.doc, para)?);
        Some(binding::own_run(&ctl.props, &p.inlines, start, &p.mark, |x| {
            let res = self.doc.resolver.as_deref()?;
            let cx = self.cx();
            let at = crate::layout::Unit(p.inlines[..start.min(p.inlines.len())].iter().map(|i| i.units() as u32).sum());
            let base = self.run_base(para, at, res, &cx)?;
            let direct = crate::docx::char_props::RunDirect::from_xml(Some(x), &cx);
            Some(std::rc::Rc::new(run_xml::resolve(res, &run_xml::base(&base), direct, false)))
        }))
    }

    /// Control `c`, whose placeholder the transaction took in paragraph `para`, holds nothing: nothing replaced it
    /// (a paragraph break, say), so the placeholder stays.
    pub(super) fn left_empty(&self, tx: &Tx, para: ParaId, c: ControlId) -> bool {
        let split = tx.maps.iter().filter_map(|m| if let Map::Split { second, .. } = m { Some(*second) } else { None });
        std::iter::once(para).chain(split).find_map(|para| {
            let p = self.index.para(&self.doc, para)?;
            let k = p.inlines.iter().position(|i| matches!(i, Inline::ControlStart(x) if *x == c))?;
            Some(p.inlines[k + 1..].iter().take_while(|i| !matches!(i, Inline::ControlEnd(x) if *x == c)).all(|i| i.units() == 0))
        }) == Some(true)
    }

    /// A control that showed its placeholder shows its content now.
    pub(super) fn placeholder_off(&mut self, tx: &mut Tx, c: ControlId) {
        let Some(mut ctl) = self.doc.controls.get(&c).cloned().filter(|k| k.props.showing_placeholder) else { return };
        ctl.props.showing_placeholder = false;
        let _ = self.step(tx, Step::Control { id: c, control: Some(ctl) });
    }
}
