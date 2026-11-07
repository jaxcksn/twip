//! Pointer shape

use crate::font::Fonts;

use super::geometry::Zone;
use super::input::Cursor;
use super::keymap::Mods;
use super::resize::Border;
use super::selection::{CellShape, ObjectRef, Selection};
use super::{Editor, Mode};

impl Editor {
    pub(super) fn hover(&mut self, x: f64, y: f64, mods: Mods, fonts: &Fonts) -> Cursor {
        let near = self.near;
        let shape = self.shape_at(x, y, mods, fonts);
        self.near = near;
        shape
    }

    fn shape_at(&mut self, x: f64, y: f64, mods: Mods, fonts: &Fonts) -> Cursor {
        if !mods.shift {
            if !mods.ctrl
                && let Some((_, h, _)) = self.shape_handle_at(x, y)
            {
                return shape_cursor(h);
            }
            if let Some((.., rot, _, k)) = self.handle_at(x, y, fonts) {
                return handle_cursor(k, rot);
            }
            match self.object_hit(x, y, fonts) {
                Some(Selection::Object(o @ ObjectRef::Floating(_))) => return if self.object_fixed(o) { Cursor::Default } else { Cursor::Move },
                Some(Selection::Object(ObjectRef::Inline { .. })) => return Cursor::Default,
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
            Some(Zone::InsertColumn(..) | Zone::InsertRow(..)) => return Cursor::Pointer,
            None => {}
        }
        if self.bar_hit(x, y, fonts).is_some() {
            return Cursor::RowSelect;
        }
        Cursor::Text
    }

    pub(super) fn drag_cursor(&self) -> Option<Cursor> {
        if self.shape_insert.is_some() {
            return Some(Cursor::Crosshair);
        }
        if let Some(d) = self.border_drag {
            return Some(match d.border {
                Border::Column(_) => Cursor::ResizeHorizontal,
                Border::Row(_) => Cursor::ResizeVertical,
                Border::Handle => Cursor::ResizeNorthWestSouthEast,
            });
        }
        if let Some(d) = &self.shape_drag {
            return Some(shape_cursor(d.handle));
        }
        if let Some(d) = self.object_drag {
            return Some(d.handle.map_or(Cursor::Move, |k| handle_cursor(k, d.rot)));
        }
        if self.table_drag.is_some() {
            return Some(Cursor::Move);
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

fn shape_cursor(h: super::shapes::ShapeHandle) -> Cursor {
    match h {
        super::shapes::ShapeHandle::Rotate => Cursor::Rotate,
        super::shapes::ShapeHandle::Adjust(_) => Cursor::Default,
        super::shapes::ShapeHandle::Move => Cursor::Move,
    }
}

/// Handle `k`'s resize arrow, turned with its frame to the nearest of the four.
fn handle_cursor(k: usize, rot: f64) -> Cursor {
    match (k as i64 + (rot / 45.0).round() as i64).rem_euclid(4) {
        0 => Cursor::ResizeNorthWestSouthEast,
        1 => Cursor::ResizeVertical,
        2 => Cursor::ResizeNorthEastSouthWest,
        _ => Cursor::ResizeHorizontal,
    }
}
