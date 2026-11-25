//! Line styles

/// How text is underlined.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Underline {
    /// No underline.
    None,
    /// A single line.
    Single,
    /// A single line under words, not the spaces between them.
    Words,
    /// Two lines.
    Double,
    /// A thick line.
    Thick,
    /// Dots.
    Dotted,
    /// Heavy dots.
    DottedHeavy,
    /// Dashes.
    Dash,
    /// Heavy dashes.
    DashedHeavy,
    /// Long dashes.
    DashLong,
    /// Heavy long dashes.
    DashLongHeavy,
    /// Dot, dash.
    DotDash,
    /// Heavy dot, dash.
    DashDotHeavy,
    /// Dot, dot, dash.
    DotDotDash,
    /// Heavy dot, dot, dash.
    DashDotDotHeavy,
    /// A wave.
    Wave,
    /// A heavy wave.
    WavyHeavy,
    /// Two waves.
    WavyDouble,
}
