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
