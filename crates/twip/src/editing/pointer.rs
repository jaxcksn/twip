//! The pointer state machine: click counts, hit zones, drags, capture and auto-scroll requests. Here: the pointer's
//! shape over each target and during each drag.

use crate::font::Fonts;

use super::geometry::Zone;
use super::input::Cursor;
use super::keymap::Mods;
use super::resize::Border;
use super::selection::{CellShape, ObjectRef, Selection};
use super::{Editor, Mode};

impl Editor {
    /// The pointer's shape over a point with no drag: what a press there would act on, asked in the order a press
    /// asks (`pointer`), changing nothing (the hit tests note the page they hit; hovering keeps the caret's).
    pub(super) fn hover(&mut self, x: f64, y: f64, mods: Mods, fonts: &Fonts) -> Cursor {
        let near = self.near;
        let shape = self.shape_at(x, y, mods, fonts);
        self.near = near;
        shape
    }

    fn shape_at(&mut self, x: f64, y: f64, mods: Mods, fonts: &Fonts) -> Cursor {
        if !mods.shift {
            if let Some((.., k)) = self.handle_at(x, y, fonts) {
                return handle_cursor(k);
            }
            match self.object_hit(x, y, fonts) {
                Some(Selection::Object(o @ ObjectRef::Floating(_))) => return if self.object_fixed(o) { Cursor::Default } else { Cursor::Move },
                Some(Selection::Object(ObjectRef::Inline { .. })) => return Cursor::Default,
                // A text box's left band selects its line.
                Some(_) => return Cursor::RowSelect,
                None => {}
            }
            if self.checkbox_toggles_at(x, y, fonts).is_some() {
                return Cursor::Default;
            }
            if (mods.ctrl || mods.meta || self.mode == Mode::ReadOnly) && self.link_under(x, y, fonts).is_some() {
                return Cursor::Pointer;
            }
        }
        match self.border_at(x, y, fonts) {
            Some((_, _, Border::Column(_), ..)) => return Cursor::ResizeHorizontal,
            Some((_, _, Border::Row(_), ..)) => return Cursor::ResizeVertical,
            Some((_, _, Border::Handle, ..)) => return Cursor::ResizeNorthWestSouthEast,
            None => {}
        }
        match self.zone_hit(x, y, fonts) {
            Some(Zone::Cell(..)) => return Cursor::CellSelect,
            Some(Zone::Column(..)) => return Cursor::ColumnSelect,
            Some(Zone::Row(..)) => return Cursor::RowSelect,
            Some(Zone::Table(_)) => return Cursor::Move,
            None => {}
        }
        if self.bar_hit(x, y, fonts).is_some() {
            return Cursor::RowSelect;
        }
        Cursor::Text
    }

    /// The pointer's shape during a drag: the border, handle, zone or text drag's; `None` with no drag.
    pub(super) fn drag_cursor(&self) -> Option<Cursor> {
        if let Some(d) = self.border_drag {
            return Some(match d.border {
                Border::Column(_) => Cursor::ResizeHorizontal,
                Border::Row(_) => Cursor::ResizeVertical,
                Border::Handle => Cursor::ResizeNorthWestSouthEast,
            });
        }
        if let Some(d) = self.object_drag {
            return Some(d.handle.map_or(Cursor::Move, handle_cursor));
        }
        if let Some((.., shape)) = self.zone_drag {
            return Some(match shape {
                CellShape::Rows => Cursor::RowSelect,
                CellShape::Columns => Cursor::ColumnSelect,
                CellShape::Rect | CellShape::Table => Cursor::CellSelect,
            });
        }
        let t = self.text_drag.filter(|d| d.moved)?;
        Some(if t.copy { Cursor::Copy } else { Cursor::Default })
    }
}

/// An object resize handle's shape, handles clockwise from the top-left corner (0 top-left ... 7 left).
fn handle_cursor(k: usize) -> Cursor {
    match k % 4 {
        0 => Cursor::ResizeNorthWestSouthEast,
        1 => Cursor::ResizeVertical,
        2 => Cursor::ResizeNorthEastSouthWest,
        _ => Cursor::ResizeHorizontal,
    }
}
