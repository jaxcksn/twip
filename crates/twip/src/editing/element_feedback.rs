//! Element feedback

use crate::api::services::PressAction;
use crate::docx::{ControlKind, ElementId};
use crate::font::Fonts;
use crate::layout::Unit;

use super::keymap::Mods;
use super::position::Position;
use super::selection::Selection;
use super::{Editor, Mode};

impl Editor {
    /// Where the pointer last hovered, in document points; none after it left or while a drag is under way.
    pub fn hover_point(&self) -> Option<(f64, f64)> {
        if self.dragging() || self.drag.is_some() || self.text_drag.is_some() || self.zone_drag.is_some() {
            return None;
        }
        self.hover
    }

    /// The innermost field or content control at a point, and what a press there does.
    pub fn element_under(&mut self, x: f64, y: f64, mods: Mods, fonts: &Fonts) -> Option<(ElementId, PressAction)> {
        let (p, _, editable) = self.hit_test(x, y, fonts)?;
        let (page, px, py) = self.stack().page_at(x, y)?;
        // A hit snaps to a field's edges, so the elements just before and after it are candidates too; the point
        // must be in one of the element's boxes.
        let mut candidates = super::elements::context(&self.doc, &self.index, &Selection::Caret(p));
        if let Some((para, u)) = p.para_offset() {
            for near in [u.0.checked_sub(1), u.0.checked_add(1)].into_iter().flatten() {
                let at = Position::text(p.story, para, Unit(near));
                candidates.extend(super::elements::context(&self.doc, &self.index, &Selection::Caret(at)));
            }
        }
        let element = candidates
            .into_iter()
            .filter_map(|(e, _)| {
                let rects = self.element_rects(e, page..page + 1);
                let inside = rects.iter().any(|(_, [x0, y0, x1, y1])| *x0 <= px && px <= *x1 && *y0 <= py && py <= *y1);
                inside.then(|| (e, rects.iter().map(|(_, r)| (r[2] - r[0]) * (r[3] - r[1])).sum::<f64>()))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))?
            .0;
        let checkbox = matches!(element, ElementId::Control(c) if self.doc.controls.get(&c).is_some_and(|k| matches!(k.props.kind, ControlKind::Checkbox { .. })));
        let action = if checkbox && self.checkbox_toggles_at(x, y, fonts).is_some() {
            PressAction::Toggle
        } else if (mods.ctrl || mods.meta || self.mode == Mode::ReadOnly) && self.link_under(x, y, fonts).is_some() {
            PressAction::FollowLink
        } else if editable && self.mode != Mode::ReadOnly {
            PressAction::PlaceCaret
        } else {
            PressAction::None
        };
        Some((element, action))
    }

    /// The innermost element the selection is in or covers, and whether the selection is exactly it.
    pub fn selected_element(&self) -> Option<(ElementId, bool)> {
        super::elements::context(&self.doc, &self.index, &self.selection).first().copied()
    }

    /// An element's boxes on `pages`, one per line part, in page points.
    pub fn element_rects(&mut self, e: ElementId, pages: std::ops::Range<usize>) -> Vec<(usize, [f64; 4])> {
        let Some((a, b)) = self.locate(e).and_then(|w| self.element_range(&w)) else { return Vec::new() };
        self.range_rects(a, b, pages)
    }
}
