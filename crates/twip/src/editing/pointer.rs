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

    /// Where a double-click shows or hides the white space between pages: `Some(true)` hides it (between pages),
    /// `Some(false)` shows it (on the hidden pages' meeting line), as Word's hide and show white space pointers say; at
    /// any `x` across the stack.
    pub fn white_space_toggle_at(&self, y: f64) -> Option<bool> {
        self.stack().toggles_white_space(y, 3.0 / self.viewport.zoom.max(0.1)).then_some(!self.white_space_hidden)
    }

    fn shape_at(&mut self, x: f64, y: f64, mods: Mods, fonts: &Fonts) -> Cursor {
        if !mods.shift
            && let Some((.., hit, _)) = self.wrap_point_at(x, y)
        {
            return match hit {
                super::wrap_points::PolyHit::Vertex(_) => Cursor::Move,
                super::wrap_points::PolyHit::Edge(_) => Cursor::Crosshair,
            };
        }
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
        // Selection drag handle
        if !mods.shift && self.text_drags() && self.hit(x, y, fonts).is_some_and(|p| self.inside_selection(&p)) {
            return Cursor::Default;
        }
        Cursor::Text
    }

    pub(super) fn drag_cursor(&self) -> Option<Cursor> {
        if self.shape_insert.is_some() {
            return Some(Cursor::Crosshair);
        }
        if self.wrap_point_dragging() {
            return Some(Cursor::Move);
        }
        if let Some(d) = &self.border_drag {
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
        Some(if self.drop_refused(&t) {
            Cursor::NotAllowed
        } else if t.copy {
            Cursor::Copy
        } else {
            Cursor::Default
        })
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
