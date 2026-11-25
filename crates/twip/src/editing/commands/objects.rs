//! Object commands

use crate::api::command::{AltText, ObjectSize};
use crate::api::value::ObjectWrap;
use crate::font::Fonts;

use super::{Dir, Outcome};
use crate::editing::Editor;
use crate::editing::objects::ObjectCmd;
use crate::editing::selection::Selection;

impl Editor {
    pub(in crate::editing) fn set_object_size(&mut self, s: ObjectSize, fonts: &Fonts) -> Outcome {
        let Some(o) = self.selected_object() else { return Outcome::unavailable("no object selected") };
        if let Selection::Object(obj) = self.selection
            && self.object_fixed(obj)
        {
            return Outcome::Unchanged("fixed size");
        }
        let (w0, h0) = (o.width, o.height);
        let aspect = s.lock_aspect.unwrap_or(false) && w0 > 0.0 && h0 > 0.0;
        let (w, h) = match (s.width.map(|v| v.0), s.height.map(|v| v.0)) {
            (Some(w), Some(h)) => (w, h),
            (Some(w), None) if aspect => (w, h0 * w / w0),
            (None, Some(h)) if aspect => (w0 * h / h0, h),
            (w, h) => (w.unwrap_or(w0), h.unwrap_or(h0)),
        };
        if s.relative_width.is_some() || s.relative_height.is_some() {
            return self.set_relative_size(w, h, [s.relative_width, s.relative_height], fonts);
        }
        let given = [s.width.is_some(), s.height.is_some()];
        let set = if aspect { [given[0] || given[1]; 2] } else { given };
        self.object_cmd(ObjectCmd::Size(w, h, set), fonts)
    }

    /// A floating object's size, and its width and height as shares of boxes on the page.
    fn set_relative_size(
        &mut self,
        w: f64,
        h: f64,
        relative: [Option<crate::api::value::NoneOr<crate::api::command::RelativeSize>>; 2],
        fonts: &Fonts,
    ) -> Outcome {
        use crate::api::command::RelativeSizeReference as R;
        use crate::api::value::NoneOr;
        let Selection::Object(crate::editing::selection::ObjectRef::Floating(f)) = self.selection else {
            return Outcome::unavailable("a relative size is for floating objects");
        };
        // Each axis's own boxes (ST_SizeRelFromH, ST_SizeRelFromV).
        match relative {
            [Some(NoneOr::Value(r)), _] if matches!(r.relative_to, R::TopMargin | R::BottomMargin) => {
                return Outcome::invalid("a width is relative to a horizontal box");
            }
            [_, Some(NoneOr::Value(r))] if matches!(r.relative_to, R::LeftMargin | R::RightMargin) => {
                return Outcome::invalid("a height is relative to a vertical box");
            }
            _ => {}
        }
        let Some(mut n) = self.float_of(f).cloned() else { return Outcome::Unchanged("no float") };
        (n.w, n.h) = ((w.max(1.0) * 20.0).round() / 20.0, (h.max(1.0) * 20.0).round() / 20.0);
        for (k, r) in relative.into_iter().enumerate() {
            match r {
                Some(NoneOr::None) => n.relative.size[k] = None,
                Some(NoneOr::Value(r)) => {
                    let from = serde_json::to_value(r.relative_to).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default();
                    n.relative.size[k] = Some((from, (r.percent * 10.0).round() as i64));
                }
                None => {}
            }
        }
        self.set_float(n, "Size", fonts)
    }

    pub(in crate::editing) fn set_object_wrap(&mut self, w: ObjectWrap, fonts: &Fonts) -> Outcome {
        if !matches!(self.selection, Selection::Object(_)) {
            return Outcome::unavailable("no object selected");
        }
        self.object_cmd(ObjectCmd::Wrap(w), fonts)
    }

    pub(in crate::editing) fn set_anchor_locked(&mut self, on: bool, fonts: &Fonts) -> Outcome {
        self.object_cmd(ObjectCmd::LockAnchor(on), fonts)
    }

    pub(in crate::editing) fn alt_text(&mut self, a: AltText, fonts: &Fonts) -> Outcome {
        self.object_cmd(ObjectCmd::AltText(a.text, a.decorative), fonts)
    }

    pub(in crate::editing) fn delete_selected_object(&mut self, fonts: &Fonts) -> Outcome {
        if !matches!(self.selection, Selection::Object(_)) {
            return Outcome::unavailable("no object selected");
        }
        self.delete(Dir::Forward, false, fonts)
    }
}
