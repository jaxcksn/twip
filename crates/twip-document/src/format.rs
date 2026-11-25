//! Formatting values

/// A run's vertical position relative to the baseline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VertAlign {
    /// On the baseline.
    Baseline,
    /// Raised and smaller.
    Superscript,
    /// Lowered and smaller.
    Subscript,
}

/// An emphasis mark over or under each character.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Emphasis {
    /// No mark, set over an inherited one.
    None,
    /// A dot above.
    Dot,
    /// A comma above.
    Comma,
    /// A circle above.
    Circle,
    /// A dot below.
    UnderDot,
}

/// A stripe or cross of shading, drawn as an 8×8 tile in the pattern colour over the fill.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Stripe {
    /// Horizontal stripes.
    Horz,
    /// Vertical stripes.
    Vert,
    /// Stripes down to the left.
    ReverseDiag,
    /// Stripes down to the right.
    Diag,
    /// A horizontal and vertical cross.
    HorzCross,
    /// A diagonal cross.
    DiagCross,
    /// Thin horizontal stripes.
    ThinHorz,
    /// Thin vertical stripes.
    ThinVert,
    /// Thin stripes down to the left.
    ThinReverseDiag,
    /// Thin stripes down to the right.
    ThinDiag,
    /// A thin horizontal and vertical cross.
    ThinHorzCross,
    /// A thin diagonal cross.
    ThinDiagCross,
}

/// A shading pattern.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ShadingPattern {
    /// No shading, set over inherited shading.
    Nil,
    /// The fill alone.
    Clear,
    /// The pattern colour alone.
    Solid,
    /// This percentage (0 to 100) of the pattern colour over the fill.
    Percent(u8),
    /// A stripe or cross in the pattern colour over the fill.
    Stripe(Stripe),
}

/// A paragraph's alignment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Jc {
    /// To the start edge.
    Left,
    /// Centred.
    Center,
    /// To the end edge.
    Right,
    /// Justified, the last line not.
    Both,
    /// Justified with space between characters too, the last line included.
    Distribute,
}

/// How a paragraph's line spacing is measured.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineRule {
    /// A multiple of the line's natural height.
    Auto,
    /// Exactly the given height.
    Exact,
    /// At least the given height.
    AtLeast,
}

/// A tab stop's kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TabKind {
    /// Text starts at the stop.
    Left,
    /// Text centres on the stop.
    Center,
    /// Text ends at the stop.
    Right,
    /// The decimal point sits at the stop.
    Decimal,
    /// A vertical bar at the stop; text does not stop there.
    Bar,
    /// A list's tab, which stops like a left tab.
    Num,
    /// Clears a stop the paragraph would inherit.
    Clear,
}

/// What fills the space before a tab stop.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Leader {
    /// Nothing.
    None,
    /// Dots.
    Dot,
    /// Hyphens.
    Hyphen,
    /// A line.
    Underscore,
    /// A heavy line (tab stops only; an alignment tab has none).
    Heavy,
    /// Middle dots.
    MiddleDot,
}

impl Leader {
    /// The character the leader repeats, if any.
    pub fn glyph(self) -> Option<char> {
        match self {
            Leader::None => None,
            Leader::Dot => Some('.'),
            Leader::Hyphen => Some('-'),
            Leader::Underscore | Leader::Heavy => Some('_'),
            Leader::MiddleDot => Some('·'),
        }
    }
}

/// A table, cell or indent width.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Width {
    /// Decided by the content.
    Auto,
    /// No width.
    Nil,
    /// Points.
    Dxa(f64),
    /// Fraction (1.0 = 100%).
    Pct(f64),
}

impl Width {
    /// A dxa or pct width as it is, any other as 0 points.
    pub fn sized(self) -> Width {
        match self {
            Width::Dxa(_) | Width::Pct(_) => self,
            _ => Width::Dxa(0.0),
        }
    }

    /// The points of a dxa width, 0 for any other.
    pub fn dxa(self) -> f64 {
        if let Width::Dxa(v) = self { v } else { 0.0 }
    }

    /// The fraction of a pct width.
    pub fn pct(self) -> Option<f64> {
        if let Width::Pct(p) = self { Some(p) } else { None }
    }
}
