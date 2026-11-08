//! Document positions

use crate::layout::{FloatId, ParaId, TableId, Unit};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StoryId {
    Body,
    HeaderFooter(u32),
    Footnote(i64),
    Endnote(i64),
    TextBox(FloatId),
    BuildingBlock(u32),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Affinity {
    Upstream,
    #[default]
    Downstream,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Place {
    Text { para: ParaId, offset: Unit, affinity: Affinity },
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

    pub fn same_place(&self, other: &Position) -> bool {
        self.story == other.story
            && match (self.place, other.place) {
                (Place::Text { para: a, offset: x, .. }, Place::Text { para: b, offset: y, .. }) => a == b && x == y,
                (p, q) => p == q,
            }
    }
}
