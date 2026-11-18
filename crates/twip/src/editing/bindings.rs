//! Content controls bound to custom XML data: an edited control writes its node, a changed node shows in its controls

use crate::docx::binding::{self, Span};
use crate::docx::{ControlId, DataPart, Inline};
use crate::layout::{ParaId, Unit};

use super::Editor;
use super::Tx;
use super::step::{Edges, Step};

fn units(inlines: &[Inline], k: usize) -> Unit {
    Unit(inlines[..k].iter().map(|i| i.units() as u32).sum())
}

impl Editor {
    /// The bound controls of paragraph `para`.
    fn bound(&self, para: ParaId) -> Vec<Span> {
        self.index.para(&self.doc, para).map_or_else(Vec::new, |p| binding::spans(&p.inlines, &self.doc.controls))
    }

    /// After an edit: each bound control edited writes its node; then, when a data part changed, every control
    /// bound into it shows its node (the edited ones too: a drop-down shows its item's text).
    pub(super) fn sync_bindings(&mut self, tx: &mut Tx) {
        if !self.doc.data_parts.is_empty() {
            self.write_bindings(tx);
        }
        if !tx.undo.iter().any(|s| matches!(s, Step::DataPart { .. })) {
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
        for (para, s, want) in shows {
            self.show_bound(tx, para, s, want);
        }
    }

    /// The nodes of the bound controls the transaction edited, written with their text; the parts copied only when one
    /// changes.
    fn write_bindings(&mut self, tx: &mut Tx) {
        let edited: Vec<ParaId> = tx.undo.iter().filter_map(|s| if let Step::Text { para, .. } = s { Some(*para) } else { None }).collect();
        let mut parts: Option<Vec<DataPart>> = None;
        for para in edited {
            for s in self.bound(para) {
                let Some(ctl) = self.doc.controls.get(&s.control) else { continue };
                let now = parts.as_deref().unwrap_or(&self.doc.data_parts);
                let parsed = binding::Parts::new(now);
                let Some(k) = s.binding.part(&parsed) else { continue };
                let value = binding::stored(&ctl.props, &s.text);
                if s.binding.value(&parsed).as_deref() == Some(value.as_str()) {
                    continue;
                }
                let Some(xml) = binding::write(&now[k].xml, &s.binding.path, &value) else { continue };
                drop(parsed);
                let parts = parts.get_or_insert_with(|| self.doc.data_parts.clone());
                (parts[k].xml, parts[k].changed) = (xml, true);
            }
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
        let Some(p) = self.index.para(&self.doc, para) else { return };
        let run = s.run.or_else(|| p.inlines[..s.inner.start].iter().rev().find_map(|i| i.run().cloned())).unwrap_or_else(|| p.mark.clone());
        let range = units(&p.inlines, s.inner.start)..units(&p.inlines, s.inner.end);
        let with = if text.is_empty() { Vec::new() } else { vec![Inline::Text(text, run)] };
        if self.step(tx, Step::Text { para, range, with, edges: Edges::default() }).is_ok() {
            self.placeholder_off(tx, s.control);
        }
    }

    /// A control that showed its placeholder shows its content now.
    fn placeholder_off(&mut self, tx: &mut Tx, c: ControlId) {
        let Some(mut ctl) = self.doc.controls.get(&c).cloned().filter(|k| k.props.showing_placeholder) else { return };
        ctl.props.showing_placeholder = false;
        let _ = self.step(tx, Step::Control { id: c, control: Some(ctl) });
    }
}
