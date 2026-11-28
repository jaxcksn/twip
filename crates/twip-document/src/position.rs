//! Document positions

use crate::ident::{FloatId, ParaId, TableId, Unit};

/// The story a position is in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StoryId {
    /// The body.
    Body,
    /// A header or footer, by its index among the document's header and footer stories.
    HeaderFooter(u32),
    /// A footnote, by its id.
    Footnote(i64),
    /// An endnote, by its id.
    Endnote(i64),
    /// A floating object's text box.
    TextBox(FloatId),
    /// A building block's body, by its index.
    BuildingBlock(u32),
}

/// Which side of a line break a position at it belongs to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Affinity {
    /// The end of the line before.
    Upstream,
    /// The start of the line after.
    #[default]
    Downstream,
}

/// Where in a story a position is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Place {
    /// An offset in a paragraph.
    Text {
        /// The paragraph.
        para: ParaId,
        /// Units from its start.
        offset: Unit,
        /// The side of a line break it belongs to.
        affinity: Affinity,
    },
    /// The end of a table row, after its last cell.
    RowEnd {
        /// The table.
        table: TableId,
        /// The row's index.
        row: usize,
    },
}

/// A place in a story, relative to the ids of the paragraphs and tables in it, so it moves with edits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Position {
    /// The story.
    pub story: StoryId,
    /// The place in it.
    pub place: Place,
}

impl Position {
    /// An offset in a paragraph, downstream.
    pub fn text(story: StoryId, para: ParaId, offset: Unit) -> Position {
        Position { story, place: Place::Text { para, offset, affinity: Affinity::Downstream } }
    }

    /// The end of a table row.
    pub fn row_end(story: StoryId, table: TableId, row: usize) -> Position {
        Position { story, place: Place::RowEnd { table, row } }
    }

    /// The paragraph and offset, for a position in text.
    pub fn para_offset(&self) -> Option<(ParaId, Unit)> {
        match self.place {
            Place::Text { para, offset, .. } => Some((para, offset)),
            Place::RowEnd { .. } => None,
        }
    }

    /// The side of a line break it belongs to (downstream at a row end).
    pub fn affinity(&self) -> Affinity {
        match self.place {
            Place::Text { affinity, .. } => affinity,
            Place::RowEnd { .. } => Affinity::Downstream,
        }
    }

    /// The same position with `affinity` (a row end is unchanged).
    pub fn with_affinity(self, affinity: Affinity) -> Position {
        match self.place {
            Place::Text { para, offset, .. } => Position { place: Place::Text { para, offset, affinity }, ..self },
            Place::RowEnd { .. } => self,
        }
    }

    /// Whether the two are at the same place, whatever their affinities.
    pub fn same_place(&self, other: &Position) -> bool {
        self.story == other.story
            && match (self.place, other.place) {
                (Place::Text { para: a, offset: x, .. }, Place::Text { para: b, offset: y, .. }) => a == b && x == y,
                (p, q) => p == q,
            }
    }
}
