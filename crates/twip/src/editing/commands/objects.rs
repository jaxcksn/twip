//! Pictures and text boxes: commands on the selected object.

use crate::api::command::{AltText, ObjectSize};
use crate::api::value::ObjectWrap;
use crate::font::Fonts;

use super::{Dir, Outcome};
use crate::editing::Editor;
use crate::editing::objects::ObjectCmd;
use crate::editing::selection::Selection;

impl Editor {
    /// The selected object's size; a dimension not given keeps its own, or follows the other with `lockAspect`.
    pub(in crate::editing) fn set_object_size(&mut self, s: ObjectSize, fonts: &Fonts) -> Outcome {
        let Some(o) = self.selected_object() else { return Outcome::unavailable("no object selected") };
        let (w0, h0) = (o.width, o.height);
        let aspect = s.lock_aspect.unwrap_or(false) && w0 > 0.0 && h0 > 0.0;
        let (w, h) = match (s.width.map(|v| v.0), s.height.map(|v| v.0)) {
            (Some(w), Some(h)) => (w, h),
            (Some(w), None) if aspect => (w, h0 * w / w0),
            (None, Some(h)) if aspect => (w0 * h / h0, h),
            (w, h) => (w.unwrap_or(w0), h.unwrap_or(h0)),
        };
        self.object_cmd(ObjectCmd::Size(w, h), fonts)
    }

    /// Wrap Text: an inline picture floats, a floating picture goes in line, a float's wrapping changes.
    pub(in crate::editing) fn set_object_wrap(&mut self, w: ObjectWrap, fonts: &Fonts) -> Outcome {
        if !matches!(self.selection, Selection::Object(_)) {
            return Outcome::unavailable("no object selected");
        }
        let name = match w {
            ObjectWrap::Inline => "inline",
            ObjectWrap::Square => "square",
            ObjectWrap::Tight => "tight",
            ObjectWrap::Through => "through",
            ObjectWrap::TopAndBottom => "topAndBottom",
            ObjectWrap::BehindText => "behind",
            ObjectWrap::InFrontOfText => "front",
        };
        self.object_cmd(ObjectCmd::Wrap(name.into()), fonts)
    }

    pub(in crate::editing) fn set_anchor_locked(&mut self, on: bool, fonts: &Fonts) -> Outcome {
        self.object_cmd(ObjectCmd::LockAnchor(on), fonts)
    }

    // shortcut: decorative pictures (no alternative text read out) aren't built yet.
    pub(in crate::editing) fn alt_text(&mut self, a: AltText, fonts: &Fonts) -> Outcome {
        if a.decorative == Some(true) {
            return Outcome::unsupported("decorative pictures");
        }
        self.object_cmd(ObjectCmd::AltText(a.text), fonts)
    }

    pub(in crate::editing) fn delete_selected_object(&mut self, fonts: &Fonts) -> Outcome {
        if !matches!(self.selection, Selection::Object(_)) {
            return Outcome::unavailable("no object selected");
        }
        self.delete(Dir::Forward, false, fonts)
    }
}
