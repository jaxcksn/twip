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
        // Create Link: the pointer over a box the text can go on in, the arrow elsewhere.
        if self.linking() {
            return if self.link_target_at(x, y, fonts).is_some() { Cursor::Pointer } else { Cursor::Default };
        }
        if !mods.shift
            && let Some((.., hit, _)) = self.wrap_point_at(x, y)
        {
            return match hit {
                super::wrap_points::PolyHit::Vertex(_) => Cursor::Move,
                super::wrap_points::PolyHit::Edge(_) => Cursor::Crosshair,
            };
        }
        if !mods.shift && self.anchor_hit(x, y).is_some() {
            return Cursor::Move;
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
        // Select Objects: the arrow off objects.
        if self.select_objects {
            return Cursor::Default;
        }
        if let Some((_, _, border, .., turn)) = self.border_at(x, y, fonts) {
            return border_cursor(border, turn);
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

    /// The way the text under the pointer runs: an I-beam over a vertical line lies across it.
    pub fn text_dir_at(&mut self, x: f64, y: f64, fonts: &Fonts) -> Option<super::geometry::LineDir> {
        let near = self.near;
        let (page, px, py) = self.stack().page_at(x, y)?;
        self.ensure_page(page, fonts);
        let geo = self.pages.page(page, &self.index)?;
        let ctx = self.editing();
        let hit = geo.hit_text(px, py, &|s| super::stories::reachable(ctx, s))?;
        let i = geo.line_at(hit.para, hit.unit, hit.affinity)?;
        self.near = near;
        Some(geo.lines[i].dir())
    }

    pub(super) fn drag_cursor(&self) -> Option<Cursor> {
        if self.shape_insert.is_some() {
            return Some(Cursor::Crosshair);
        }
        if self.wrap_point_dragging() || self.anchor_drag.is_some() {
            return Some(Cursor::Move);
        }
        if self.marquee.is_some() {
            return Some(Cursor::Default);
        }
        if let Some(d) = &self.border_drag {
            return Some(border_cursor(d.border, d.turn));
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

/// A table border's resize arrow: across the border as the table is drawn.
fn border_cursor(border: Border, turn: crate::layout::vertical::Turn) -> Cursor {
    let across = |v: [f64; 2]| {
        let [x, y] = turn.along(v);
        if x.abs() >= y.abs() { Cursor::ResizeHorizontal } else { Cursor::ResizeVertical }
    };
    match border {
        Border::Column(_) => across([1.0, 0.0]),
        Border::Row(_) => across([0.0, 1.0]),
        Border::Handle => {
            let [x, y] = turn.along([1.0, 1.0]);
            if x * y > 0.0 { Cursor::ResizeNorthWestSouthEast } else { Cursor::ResizeNorthEastSouthWest }
        }
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
