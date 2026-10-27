//! Break tokens: where layout continues, and the state carried across the break. Tokens are compared only
//! for equality.

use std::rc::Rc;

use crate::docx::{NoteKind, ParaStyle};

use super::story::StoryUnit;

/// Where the flow is in a block.
#[derive(Clone, Debug, Default)]
pub enum At {
    /// The block's start.
    #[default]
    Start,
    /// A paragraph continued from an earlier column: the piece its next line starts at.
    Piece(usize),
    /// A table: the next row.
    Row(usize),
    /// A table row split across columns (TABLE-41): the row and each cell's remaining units, rebased to 0. Shared by
    /// identity: two tokens are equal only if they continue the same split.
    RowPart { row: usize, rest: Rc<Vec<Vec<StoryUnit>>> },
}

impl PartialEq for At {
    fn eq(&self, o: &Self) -> bool {
        match (self, o) {
            (At::Start, At::Start) => true,
            (At::Piece(a), At::Piece(b)) | (At::Row(a), At::Row(b)) => a == b,
            (At::RowPart { row: a, rest: x }, At::RowPart { row: b, rest: y }) => a == b && Rc::ptr_eq(x, y),
            _ => false,
        }
    }
}

impl At {
    /// The piece a paragraph's next line starts at, or a table's next row (0 at the start).
    pub fn index(&self) -> usize {
        match self {
            At::Start => 0,
            At::Piece(i) | At::Row(i) => *i,
            At::RowPart { row, .. } => *row,
        }
    }

    /// Whether the flow is at the block's start (no part of it placed yet).
    pub fn at_start(&self) -> bool {
        self.index() == 0 && !matches!(self, At::RowPart { .. })
    }

    /// A split row's remaining units.
    pub fn rest(&self) -> Option<&Rc<Vec<Vec<StoryUnit>>>> {
        match self {
            At::RowPart { rest, .. } => Some(rest),
            _ => None,
        }
    }
}

/// Where the flow is in a section's blocks.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FlowToken {
    pub block: usize,
    pub at: At,
}

impl FlowToken {
    /// The next block's start.
    pub fn next_block(&mut self) {
        self.block += 1;
        self.at = At::Start;
    }
}

/// State carried from one column into the next.
#[derive(Clone, Debug)]
pub struct FlowState {
    pub prev: Option<Rc<ParaStyle>>,
    pub prev_after: f64,
    /// How the next column top was reached: true = the next paragraph keeps its space-before (PARA-2, COL-2).
    pub keep_top: bool,
    pub first_para: bool,
    /// Footnote parts that continue on the next page (NOTE-6).
    pub note_rest: Vec<NotePart>,
    /// Body lines counted for line numbering in the current restart scope (LNUM-2).
    pub line_no: i64,
    /// COL-2: the space-before of a paragraph whose first line ended in a column break, kept by its continuation.
    pub break_before: f64,
}

impl Default for FlowState {
    fn default() -> Self {
        FlowState { prev: None, prev_after: 0.0, keep_top: true, first_para: true, note_rest: Vec::new(), line_no: 0, break_before: 0.0 }
    }
}

impl PartialEq for FlowState {
    /// Equal when the next column would start the same way. States carrying note parts never compare equal (they
    /// continue specific notes).
    fn eq(&self, o: &Self) -> bool {
        let style = match (&self.prev, &o.prev) {
            (None, None) => true,
            (Some(x), Some(y)) => Rc::ptr_eq(x, y) || **x == **y,
            _ => false,
        };
        style
            && self.prev_after == o.prev_after
            && self.keep_top == o.keep_top
            && self.first_para == o.first_para
            && self.note_rest.is_empty()
            && o.note_rest.is_empty()
            && self.line_no == o.line_no
            && self.break_before == o.break_before
    }
}

/// A note, or the part of one, in a note area: which note it is and its units (a continuation's rebased to 0).
#[derive(Clone, Debug)]
pub struct NotePart {
    pub kind: NoteKind,
    pub id: i64,
    pub units: Vec<StoryUnit>,
}

impl NotePart {
    /// The part's height: its last unit's bottom.
    pub fn height(&self) -> f64 {
        self.units.last().map_or(0.0, |u| u.top + u.height)
    }
}

/// Where a page's body starts: the section, the flow position and the carried state. No page number: parity and
/// number enter only through the values they produce.
#[derive(Clone, Debug, PartialEq)]
pub struct PageToken {
    pub section: usize,
    pub flow: FlowToken,
    pub state: FlowState,
}
