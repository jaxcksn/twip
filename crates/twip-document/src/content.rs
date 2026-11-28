//! Content values

/// What a break inline breaks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BreakKind {
    /// The line.
    Line,
    /// The page.
    Page,
    /// The column.
    Column,
    /// The line, continuing below floating objects on the left.
    ClearLeft,
    /// The line, continuing below floating objects on the right.
    ClearRight,
    /// The line, continuing below floating objects on both sides.
    ClearAll,
}

impl BreakKind {
    /// Whether it breaks only the line.
    pub fn is_line(self) -> bool {
        matches!(self, BreakKind::Line | BreakKind::ClearLeft | BreakKind::ClearRight | BreakKind::ClearAll)
    }
}

/// A note's kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NoteKind {
    /// At the foot of the page.
    Foot,
    /// At the end of the section or document.
    End,
}
