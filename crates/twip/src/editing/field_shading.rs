//! Field shading

use std::cmp::Ordering;
use std::collections::HashMap;

use crate::api::services::FieldShading;
use crate::docx::Inline;
use crate::docx::fields::spans;
use crate::layout::{ParaId, Unit};

use super::Editor;
use super::position::Position;

/// The outermost fields of a paragraph, as unit ranges from the field's begin to after its end.
fn outermost_fields(inlines: &[Inline]) -> Vec<(Unit, Unit)> {
    let starts: Vec<u32> = inlines.iter().scan(0u32, |at, i| Some(std::mem::replace(at, *at + i.units() as u32))).collect();
    let mut out: Vec<(Unit, Unit)> = Vec::new();
    let mut outer_end = None;
    for s in spans(inlines) {
        if outer_end.is_some_and(|end| s.end < end) {
            continue;
        }
        outer_end = Some(s.end);
        out.push((Unit(starts[s.begin]), Unit(starts[s.end] + 1)));
    }
    out
}

impl Editor {
    /// The rectangles to shade on `pages`, one per line part of each shaded field's shown text, in page points.
    pub fn field_shading(&mut self, pages: std::ops::Range<usize>, mode: FieldShading) -> Vec<(usize, [f64; 4])> {
        let selected = match mode {
            FieldShading::Never => return Vec::new(),
            FieldShading::Always => None,
            FieldShading::WhenSelected => {
                let Some(focus) = self.selection.focus() else { return Vec::new() };
                let anchor = self.selection.anchor().unwrap_or(focus);
                Some(if self.index.cmp(&anchor, &focus) == Ordering::Greater { (focus, anchor) } else { (anchor, focus) })
            }
        };
        let mut fields: HashMap<ParaId, Vec<(Unit, Unit)>> = HashMap::new();
        let mut out = Vec::new();
        for page in pages {
            let Some(geo) = self.pages.page(page, &self.index) else { continue };
            for l in &geo.lines {
                let story = geo.stories[l.story].story;
                for &para in &l.paras {
                    let ranges = fields
                        .entry(para)
                        .or_insert_with(|| self.index.para(&self.doc, para).map(|p| outermost_fields(&p.inlines)).unwrap_or_default());
                    if ranges.is_empty() {
                        continue;
                    }
                    let (Some(lf), Some(ll)) = (l.first_of(para), l.last_of(para)) else { continue };
                    for &(a, b) in ranges.iter() {
                        if let Some((lo, hi)) = selected {
                            let (fa, fb) = (Position::text(story, para, a), Position::text(story, para, b));
                            let touches =
                                lo.story == story && self.index.cmp(&fa, &hi) != Ordering::Greater && self.index.cmp(&lo, &fb) != Ordering::Greater;
                            if !touches {
                                continue;
                            }
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
