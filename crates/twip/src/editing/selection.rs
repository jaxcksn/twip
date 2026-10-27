//! Selections (insertion point, text range, cell block, object) and their normalization.
//! A selection's kind and extent come only from input and document state, never from rendering.

use crate::layout::{FloatId, ParaId, TableId, Unit};

use super::index::DocIndex;
use super::position::Position;

/// The unit a selection extends by: characters, or what the gesture that started it selected.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Granularity {
    #[default]
    Char,
    Word,
    Sentence,
    Paragraph,
    Line,
}

/// A cell in a table's grid: row and grid column (a merged cell is named by its first grid column and first row).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CellRef {
    pub row: usize,
    pub col: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CellShape {
    /// The rectangle between anchor and focus.
    Rect,
    /// Whole rows from the anchor's to the focus's.
    Rows,
    /// Whole columns.
    Columns,
    Table,
}

/// An inline picture (by its unit) or a floating object.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectRef {
    Inline { para: ParaId, offset: Unit },
    Floating(FloatId),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Selection {
    Caret(Position),
    /// A range in document order of one story; `anchor` stays where it started, `focus` moves.
    Text {
        anchor: Position,
        focus: Position,
        unit: Granularity,
    },
    /// A block of cells of one table at one nesting level.
    Cells {
        table: TableId,
        anchor: CellRef,
        focus: CellRef,
        shape: CellShape,
    },
    Object(ObjectRef),
    /// A vertical block (Ctrl+Shift+F8): the lines from the anchor's to the focus's, each cut between their x.
    Block {
        anchor: Position,
        focus: Position,
    },
}

impl Selection {
    /// The focus: where the caret shows and where Shift+movement continues from.
    pub fn focus(&self) -> Option<Position> {
        match self {
            Selection::Caret(p) => Some(*p),
            Selection::Text { focus, .. } | Selection::Block { focus, .. } => Some(*focus),
            Selection::Cells { .. } | Selection::Object(_) => None,
        }
    }

    pub fn anchor(&self) -> Option<Position> {
        match self {
            Selection::Caret(p) => Some(*p),
            Selection::Text { anchor, .. } | Selection::Block { anchor, .. } => Some(*anchor),
            Selection::Cells { .. } | Selection::Object(_) => None,
        }
    }

    pub fn is_collapsed(&self) -> bool {
        matches!(self, Selection::Caret(_))
    }

    /// A text range from `anchor` to `focus`, or a caret when they are the same place.
    pub fn range(anchor: Position, focus: Position, unit: Granularity) -> Selection {
        if anchor.same_place(&focus) { Selection::Caret(focus) } else { Selection::Text { anchor, focus, unit } }
    }

    /// The text range's ends in document order.
    pub fn ordered(&self, idx: &DocIndex) -> Option<(Position, Position)> {
        match self {
            Selection::Caret(p) => Some((*p, *p)),
            Selection::Text { anchor, focus, .. } => {
                Some(if idx.cmp(anchor, focus) == std::cmp::Ordering::Greater { (*focus, *anchor) } else { (*anchor, *focus) })
            }
            Selection::Cells { .. } | Selection::Object(_) | Selection::Block { .. } => None,
        }
    }
}
