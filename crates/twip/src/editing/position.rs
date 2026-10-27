//! Logical document positions: story, paragraph id, grapheme-safe unit offset and affinity, or an end-of-row mark.
//! Positions name content by identity, so they survive reflow and edits elsewhere; paths are
//! derived from the [`super::index::DocIndex`] when needed, never stored.

use crate::layout::{FloatId, ParaId, TableId, Unit};

/// A story: the body, a header or footer, a note, or a text box's content.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StoryId {
    Body,
    /// A header or footer part, by its index in the document index's story table (stable for the session).
    HeaderFooter(u32),
    Footnote(i64),
    Endnote(i64),
    TextBox(FloatId),
}

/// Which of the two visual places of a position the caret shows at where they differ: `Upstream` at the end
/// of the line before a soft wrap or break, `Downstream` at the start of the next line. Elsewhere it is `Downstream`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Affinity {
    Upstream,
    #[default]
    Downstream,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Place {
    /// Before unit `offset` of the paragraph (`offset` = its length: at its mark), on a grapheme boundary.
    Text { para: ParaId, offset: Unit, affinity: Affinity },
    /// The end-of-row mark of a table row.
    RowEnd { table: TableId, row: usize },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Position {
    pub story: StoryId,
    pub place: Place,
}

impl Position {
    pub fn text(story: StoryId, para: ParaId, offset: Unit) -> Position {
        Position { story, place: Place::Text { para, offset, affinity: Affinity::Downstream } }
    }

    pub fn row_end(story: StoryId, table: TableId, row: usize) -> Position {
        Position { story, place: Place::RowEnd { table, row } }
    }

    /// The paragraph and offset of a text position.
    pub fn para_offset(&self) -> Option<(ParaId, Unit)> {
        match self.place {
            Place::Text { para, offset, .. } => Some((para, offset)),
            Place::RowEnd { .. } => None,
        }
    }

    pub fn affinity(&self) -> Affinity {
        match self.place {
            Place::Text { affinity, .. } => affinity,
            Place::RowEnd { .. } => Affinity::Downstream,
        }
    }

    pub fn with_affinity(self, affinity: Affinity) -> Position {
        match self.place {
            Place::Text { para, offset, .. } => Position { place: Place::Text { para, offset, affinity }, ..self },
            Place::RowEnd { .. } => self,
        }
    }

    /// The same logical place, ignoring affinity: what selections and edits compare.
    pub fn same_place(&self, other: &Position) -> bool {
        self.story == other.story
            && match (self.place, other.place) {
                (Place::Text { para: a, offset: x, .. }, Place::Text { para: b, offset: y, .. }) => a == b && x == y,
                (p, q) => p == q,
            }
    }
}
