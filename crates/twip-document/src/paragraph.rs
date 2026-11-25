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
