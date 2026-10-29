//! Selection and navigation: none of these change the document.

use crate::api::command::{MoveSelection, MoveUnit, SelectUnit, SelectionMode};
use crate::api::value::{Direction, Order};
use crate::font::Fonts;
use crate::layout::Unit;

use super::{Dir, MoveBy, Outcome};
use crate::editing::position::{Place, Position, StoryId};
use crate::editing::selection::{CellShape, Granularity, ObjectRef, Selection};
use crate::editing::{Editor, extend};

impl Editor {
    /// The whole story the selection is in.
    pub(in crate::editing) fn select_all(&mut self) -> Outcome {
        let story = self.selection.focus().map_or(self.editing(), |p| p.story);
        let (Some(a), Some(b)) = (self.nav().story_edge(story, false), self.nav().story_edge(story, true)) else {
            return Outcome::Unchanged("empty story");
        };
        self.set_selection(Selection::range(a, b, Granularity::Char));
        Outcome::Moved
    }

    pub(in crate::editing) fn select_unit(&mut self, unit: SelectUnit) -> Outcome {
        let granularity = match unit {
            SelectUnit::Word => Granularity::Word,
            SelectUnit::Sentence => Granularity::Sentence,
            SelectUnit::Paragraph => Granularity::Paragraph,
            SelectUnit::Cell => return self.select_cells(CellShape::Rect),
            SelectUnit::Row => return self.select_cells(CellShape::Rows),
            SelectUnit::Column => return self.select_cells(CellShape::Columns),
            SelectUnit::Table => return self.select_table(),
        };
        let Some(p) = self.caret() else { return Outcome::Unchanged("no caret") };
        let (a, b) = self.span(p, granularity);
        self.set_selection(Selection::range(a, b, granularity));
        Outcome::Moved
    }

    pub(in crate::editing) fn move_selection(&mut self, m: MoveSelection, fonts: &Fonts) -> Outcome {
        let dir = if m.direction == Direction::Forward { Dir::Forward } else { Dir::Back };
        let extend = m.extend.unwrap_or(false);
        let by = match m.by {
            MoveUnit::Character => MoveBy::Char,
            MoveUnit::Word => MoveBy::Word,
            MoveUnit::Line => MoveBy::Line,
            MoveUnit::Paragraph => MoveBy::Para,
            MoveUnit::LineEdge => MoveBy::LineEdge,
            MoveUnit::Document => MoveBy::Story,
            MoveUnit::Screen => MoveBy::Screen,
            MoveUnit::Page => MoveBy::Page,
            MoveUnit::RowEdge => MoveBy::RowEdge,
            MoveUnit::ColumnEdge => MoveBy::ColumnEdge,
        };
        // Extending to a row's or column's edge in a table selects the cells there.
        if extend && matches!(by, MoveBy::RowEdge | MoveBy::ColumnEdge) {
            return self.select_to_edge(by == MoveBy::RowEdge, dir == Dir::Forward);
        }
        // A selected float moves by the arrows (Up and Down by lines, Left and Right by characters).
        if !extend && let Selection::Object(o @ ObjectRef::Floating(f)) = self.selection {
            if self.object_fixed(o) {
                return Outcome::Unchanged("fixed position");
            }
            let d = if dir == Dir::Forward { 0.75 } else { -0.75 };
            return match by {
                MoveBy::Char | MoveBy::Word => self.nudge(f, d, 0.0, fonts),
                MoveBy::Line | MoveBy::Para => self.nudge(f, 0.0, d, fonts),
                _ => Outcome::Unchanged("not a nudge"),
            };
        }
        let out = self.move_caret(by, dir, extend, fonts);
        // A paragraph with no lines laid out (no font draws it yet): Home and End go to its edges.
        if by == MoveBy::LineEdge
            && out == Outcome::Unchanged("no place to move to")
            && let Some(focus) = self.selection.focus()
            && let Some((para, _)) = focus.para_offset()
        {
            let len = self.index.para(&self.doc, para).map_or(Unit(0), crate::editing::step::para_len);
            let to = Position::text(focus.story, para, if dir == Dir::Forward { len } else { Unit(0) });
            let anchor = self.selection.anchor().unwrap_or(focus);
            self.set_selection(if extend { Selection::range(anchor, to, Granularity::Char) } else { Selection::Caret(to) });
            return Outcome::Moved;
        }
        out
    }

    pub(in crate::editing) fn selection_mode(&mut self, mode: SelectionMode) -> Outcome {
        match mode {
            SelectionMode::Extend => self.extend_mode(),
            SelectionMode::Block => self.block_mode(),
            SelectionMode::Normal => {
                self.modes = extend::Modes::default();
                Outcome::Moved
            }
        }
    }

    /// The first floating object, or from a selected one the next or previous.
    pub(in crate::editing) fn next_object(&mut self, o: Order) -> Outcome {
        match self.selection {
            Selection::Object(ObjectRef::Floating(_)) => self.object_focus(Some(o == Order::Previous)),
            _ => self.object_focus(None),
        }
    }

    /// The caret at the start of page `page` (from 1).
    pub(in crate::editing) fn go_to_page(&mut self, page: u32, fonts: &Fonts) -> Outcome {
        let Some(k) = (page as usize).checked_sub(1) else { return Outcome::invalid("pages count from 1") };
        self.ensure_page(k, fonts);
        let stack = self.stack();
        let Some(&r) = stack.pages.get(k) else { return Outcome::invalid("no such page") };
        let top = self.snapshot().pages().get(k).and_then(|p| p.body_area).map_or(72.0, |a| a.0);
        let Some((page, px, py)) = stack.page_at(r[0] + 1.0, r[1] + top + 1.0) else { return Outcome::invalid("no such page") };
        let Some(geo) = self.pages.page(page, &self.index) else { return Outcome::Unchanged("page not laid out") };
        let Some(hit) = geo.hit_text(px, py, &|s| s == StoryId::Body) else { return Outcome::Unchanged("no text on the page") };
        let at = Position { story: hit.story, place: Place::Text { para: hit.para, offset: hit.unit, affinity: hit.affinity } };
        let at = self.nav().line_edge(at, false).unwrap_or(at);
        self.set_selection(Selection::Caret(at));
        self.scroll_to_caret(fonts);
        Outcome::Moved
    }
}
