//! Paragraph and list label values

/// How a line's runs sit on it vertically.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextAlignment {
    /// Chosen by the text flow.
    Auto,
    /// On the baseline.
    Baseline,
    /// To the top.
    Top,
    /// Centred.
    Center,
    /// To the bottom.
    Bottom,
}

/// A list level's alignment of its label at the number position.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListJc {
    /// Left.
    Left,
    /// Centred.
    Center,
    /// Right.
    Right,
    /// The start side.
    Start,
    /// The end side.
    End,
    /// Justified.
    Both,
    /// Justified with medium kashida.
    MediumKashida,
    /// Distributed.
    Distribute,
    /// At a numbering tab.
    NumTab,
    /// Justified with high kashida.
    HighKashida,
    /// Justified with low kashida.
    LowKashida,
    /// Thai distributed.
    ThaiDistribute,
}
