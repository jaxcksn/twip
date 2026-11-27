//! Text body properties

use super::units::{Angle, Emu, Pct};

/// Where a shape's text sits up and down its box.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum VAnchor {
    /// At the top.
    #[default]
    Top,
    /// In the middle.
    Centre,
    /// At the bottom.
    Bottom,
    /// Another anchor (justified or distributed), laid out at the top.
    Other,
}

/// How a shape and its text fit each other.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum AutoFit {
    /// Neither changes.
    #[default]
    None,
    /// The shape grows or shrinks to its text.
    Shape,
    /// The text shrinks to the shape.
    Normal {
        /// The text's size, of its own.
        font_scale: Pct,
        /// How much less its line spacing is.
        line_reduction: Pct,
    },
}

/// The way a shape's text runs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextDirection {
    /// Across.
    #[default]
    Horizontal,
    /// Down, turned a quarter clockwise.
    Vert,
    /// Up, turned a quarter counterclockwise.
    Vert270,
    /// Down, East Asian characters upright.
    EaVert,
    /// Down, lines left to right, as Mongolian is.
    MongolianVert,
    /// Down, each character upright, stacked.
    WordArtVert,
    /// Down, each character upright, stacked, lines right to left.
    WordArtVertRtl,
}

impl TextDirection {
    /// The direction named `v` (`ST_TextVerticalType`), across when none is.
    pub fn parse(v: Option<&str>) -> TextDirection {
        match v {
            Some("vert") => TextDirection::Vert,
            Some("vert270") => TextDirection::Vert270,
            Some("eaVert") => TextDirection::EaVert,
            Some("mongolianVert") => TextDirection::MongolianVert,
            Some("wordArtVert") => TextDirection::WordArtVert,
            Some("wordArtVertRtl") => TextDirection::WordArtVertRtl,
            _ => TextDirection::Horizontal,
        }
    }

    /// Its name.
    pub fn name(self) -> &'static str {
        match self {
            TextDirection::Horizontal => "horz",
            TextDirection::Vert => "vert",
            TextDirection::Vert270 => "vert270",
            TextDirection::EaVert => "eaVert",
            TextDirection::MongolianVert => "mongolianVert",
            TextDirection::WordArtVert => "wordArtVert",
            TextDirection::WordArtVertRtl => "wordArtVertRtl",
        }
    }
}

/// How a shape's text is laid out in it.
#[derive(Clone, Debug, PartialEq)]
pub struct BodyProps {
    /// The space between the box's edges and the text: left, top, right, bottom.
    pub insets: [Emu; 4],
    /// Where the text sits up and down.
    pub anchor: VAnchor,
    /// The text doesn't wrap.
    pub no_wrap: bool,
    /// How the shape and its text fit each other.
    pub autofit: AutoFit,
    /// The way the text runs.
    pub direction: TextDirection,
    /// The text's turn, beside the shape's.
    pub rot: Angle,
    /// The text stays upright when the shape turns.
    pub upright: bool,
    /// It has properties kept but not drawn (a text warp, 3-D).
    pub kept: bool,
    /// Text columns (`numCol`, `spcCol`): how many, and the space between them.
    pub columns: (u16, Emu),
}

impl Default for BodyProps {
    fn default() -> BodyProps {
        BodyProps {
            insets: [Emu(91440), Emu(45720), Emu(91440), Emu(45720)],
            anchor: VAnchor::Top,
            no_wrap: false,
            autofit: AutoFit::None,
            direction: TextDirection::Horizontal,
            rot: Angle(0),
            upright: false,
            kept: false,
            columns: (1, Emu(0)),
        }
    }
}
