//! Field shading

use std::cmp::Ordering;
use std::collections::HashMap;

use crate::api::services::FieldShading;
use crate::docx::Inline;
use crate::docx::fields::spans;
use crate::layout::{ParaId, Unit};

use super::Editor;
use super::position::Position;

/// The outermost fields of a paragraph, as unit ranges from the field's begin to after its end, and whether each is a
/// legacy form field.
fn outermost_fields(inlines: &[Inline]) -> Vec<(Unit, Unit, bool)> {
    let starts: Vec<u32> = inlines.iter().scan(0u32, |at, i| Some(std::mem::replace(at, *at + i.units() as u32))).collect();
    let mut out: Vec<(Unit, Unit, bool)> = Vec::new();
    let mut outer_end = None;
    for s in spans(inlines) {
        if outer_end.is_some_and(|end| s.end < end) {
            continue;
        }
        outer_end = Some(s.end);
        let form = matches!(inlines[s.begin], Inline::FieldBegin { form: Some(_), .. });
        out.push((Unit(starts[s.begin]), Unit(starts[s.end] + 1), form));
    }
    out
}

impl Editor {
    /// The rectangles to shade on `pages`, one per line part of each shaded field's shown text, in page points. Legacy
    /// form fields are shaded whatever `mode`, unless the document turns that off.
    pub fn field_shading(&mut self, pages: std::ops::Range<usize>, mode: FieldShading) -> Vec<(usize, [f64; 4])> {
        let forms = !self.doc.settings.no_form_shading;
        let selected = match (mode, self.selection.focus()) {
            (FieldShading::WhenSelected, Some(focus)) => {
                let anchor = self.selection.anchor().unwrap_or(focus);
                Some(if self.index.cmp(&anchor, &focus) == Ordering::Greater { (focus, anchor) } else { (anchor, focus) })
            }
            _ => None,
        };
        if !forms && (mode == FieldShading::Never || (mode == FieldShading::WhenSelected && selected.is_none())) {
            return Vec::new();
        }
        let mut fields: HashMap<ParaId, Vec<(Unit, Unit, bool)>> = HashMap::new();
        let mut out = Vec::new();
        for page in pages {
            let Some(geo) = self.pages.page(page, &self.index) else { continue };
            for l in &geo.lines {
                let story = geo.stories[l.story].story;
                for &para in l.paras.iter() {
                    let ranges = fields
                        .entry(para)
                        .or_insert_with(|| self.index.para(&self.doc, para).map(|p| outermost_fields(&p.inlines)).unwrap_or_default());
                    if ranges.is_empty() {
                        continue;
                    }
                    let (Some(lf), Some(ll)) = (l.first_of(para), l.last_of(para)) else { continue };
                    for &(a, b, form) in ranges.iter() {
                        let by_mode = match (mode, selected) {
                            (FieldShading::Always, _) => true,
                            (FieldShading::WhenSelected, Some((lo, hi))) => {
                                let (fa, fb) = (Position::text(story, para, a), Position::text(story, para, b));
                                lo.story == story && self.index.cmp(&fa, &hi) != Ordering::Greater && self.index.cmp(&lo, &fb) != Ordering::Greater
                            }
                            _ => false,
                        };
                        if !(by_mode || (forms && form)) {
                            continue;
                        }
                        let (from, to) = (a.max(lf), b.min(ll));
                        if from >= to {
                            continue;
                        }
                        let (Some(x0), Some(x1)) = (l.x_of(para, from), l.x_of(para, to)) else { continue };
                        if x1 > x0 + 0.01 {
                            out.push((page, [x0, l.top, x1, l.bottom]));
                        }
                    }
                }
            }
        }
        out
    }
}
