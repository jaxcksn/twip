//! Text body properties

use super::units::{Angle, Emu, Pct};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum VAnchor {
    #[default]
    Top,
    Centre,
    Bottom,
    Other,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum AutoFit {
    #[default]
    None,
    Shape,
    Normal {
        font_scale: Pct,
        line_reduction: Pct,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextDirection {
    #[default]
    Horizontal,
    Vert,
    Vert270,
    EaVert,
    MongolianVert,
    WordArtVert,
    WordArtVertRtl,
}

impl TextDirection {
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

#[derive(Clone, Debug, PartialEq)]
pub struct BodyProps {
    pub insets: [Emu; 4],
    pub anchor: VAnchor,
    pub no_wrap: bool,
    pub autofit: AutoFit,
    pub direction: TextDirection,
    pub rot: Angle,
    pub upright: bool,
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
