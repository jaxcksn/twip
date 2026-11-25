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

/// A border's line.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BorderStyle {
    /// No border, set over an inherited one.
    Nil,
    /// No border.
    None,
    /// A single line.
    Single,
    /// A thick line.
    Thick,
    /// Two lines.
    Double,
    /// Dots.
    Dotted,
    /// Dashes.
    Dashed,
    /// Dot, dash.
    DotDash,
    /// Dot, dot, dash.
    DotDotDash,
    /// Three lines.
    Triple,
    /// Thin then thick, a small gap between.
    ThinThickSmallGap,
    /// Thick then thin, a small gap between.
    ThickThinSmallGap,
    /// Thin, thick, thin, small gaps between.
    ThinThickThinSmallGap,
    /// Thin then thick, a medium gap between.
    ThinThickMediumGap,
    /// Thick then thin, a medium gap between.
    ThickThinMediumGap,
    /// Thin, thick, thin, medium gaps between.
    ThinThickThinMediumGap,
    /// Thin then thick, a large gap between.
    ThinThickLargeGap,
    /// Thick then thin, a large gap between.
    ThickThinLargeGap,
    /// Thin, thick, thin, large gaps between.
    ThinThickThinLargeGap,
    /// A wave.
    Wave,
    /// Two waves.
    DoubleWave,
    /// Dashes, small gaps between.
    DashSmallGap,
    /// Dash, dot, stroked.
    DashDotStroked,
    /// Embossed.
    ThreeDEmboss,
    /// Engraved.
    ThreeDEngrave,
    /// Raised.
    Outset,
    /// Sunken.
    Inset,
    /// An art border.
    Art(ArtBorder),
}

/// An art border's picture: its number in the format's list of them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ArtBorder(pub u8);
