//! Selections

use crate::layout::{FloatId, ParaId, TableId, Unit};

use super::index::DocIndex;
use super::position::Position;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Granularity {
    #[default]
    Char,
    Word,
    Sentence,
    Paragraph,
    Line,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CellRef {
    pub row: usize,
    pub col: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CellShape {
    Rect,
    Rows,
    Columns,
    Table,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectRef {
    Inline { para: ParaId, offset: Unit },
    Floating(FloatId),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Selection {
    Caret(Position),
    Text { anchor: Position, focus: Position, unit: Granularity },
    Cells { table: TableId, anchor: CellRef, focus: CellRef, shape: CellShape },
    Object(ObjectRef),
    Block { anchor: Position, focus: Position },
}

impl Selection {
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

    pub fn range(anchor: Position, focus: Position, unit: Granularity) -> Selection {
        if anchor.same_place(&focus) { Selection::Caret(focus) } else { Selection::Text { anchor, focus, unit } }
    }

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
