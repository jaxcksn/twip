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
