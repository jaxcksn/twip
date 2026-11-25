//! Float anchoring and wrapping

/// The sides of a wrapping float text flows on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WrapText {
    /// Both sides.
    BothSides,
    /// The left side only.
    Left,
    /// The right side only.
    Right,
    /// The side with more room.
    Largest,
}

/// Where a float is aligned down its vertical reference.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VerticalAlign {
    /// At the top.
    Top,
    /// Centred.
    Center,
    /// At the bottom.
    Bottom,
    /// At the inside edge.
    Inside,
    /// At the outside edge.
    Outside,
}

/// How text wraps a float.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FloatWrap {
    /// Around its box.
    Square,
    /// Around its outline.
    Tight,
    /// Around its outline and through its open parts.
    Through,
    /// Above and below it, not beside it.
    TopAndBottom,
    /// Not at all.
    None,
}

/// What a float's horizontal position is measured from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HorizontalRelation {
    /// The area between the left and right margins.
    Margin,
    /// The page's edges.
    Page,
    /// The column it is anchored in.
    Column,
    /// The character its anchor is at.
    Character,
    /// The left margin's area.
    LeftMargin,
    /// The right margin's area.
    RightMargin,
    /// The inside margin's area.
    InsideMargin,
    /// The outside margin's area.
    OutsideMargin,
    /// A value the file gave that is not in the schema; behaves as today's fallback: measured across the column, but
    /// none of the rules for a float placed from the column apply.
    Unrecognised,
}

/// What a float's vertical position is measured from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VerticalRelation {
    /// The area between the top and bottom margins.
    Margin,
    /// The page's edges.
    Page,
    /// Its anchor paragraph's top.
    Paragraph,
    /// Its anchor line's top.
    Line,
    /// The top margin's area.
    TopMargin,
    /// The bottom margin's area.
    BottomMargin,
    /// The inside margin's area.
    InsideMargin,
    /// The outside margin's area.
    OutsideMargin,
    /// A value the file gave that is not in the schema, or none given; behaves as today's fallback: measured down
    /// from the paragraph, but none of the rules for a float placed from the paragraph apply.
    Unrecognised,
}

/// Where a float is aligned across its horizontal reference.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HorizontalAlign {
    /// At the left edge.
    Left,
    /// Centred.
    Center,
    /// At the right edge.
    Right,
    /// At the inside edge.
    Inside,
    /// At the outside edge.
    Outside,
    /// A value the file gave that is not in the schema; behaves as today's fallback: aligned at the left edge, but
    /// none of the rules for a left-aligned float apply.
    Unrecognised,
}
