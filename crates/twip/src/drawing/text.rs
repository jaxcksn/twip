//! A shape's text body properties (`wps:bodyPr`): insets, vertical anchor, autofit, wrapping, direction (FLOAT-10).

use roxmltree::Node;

use super::units::{Angle, Emu, Pct};
use crate::docx::xml::A;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum VAnchor {
    #[default]
    Top,
    Centre,
    Bottom,
    /// `just`, `dist` and unknown values lay out as top (FLOAT-10).
    Other,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum AutoFit {
    #[default]
    None,
    /// `a:spAutoFit`: the shape's height follows its text.
    Shape,
    /// `a:normAutofit`: kept; the text is laid out at 100 %.
    Normal { font_scale: Pct, line_reduction: Pct },
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

/// `wps:bodyPr` (or `a:bodyPr`).
#[derive(Clone, Debug, PartialEq)]
pub struct BodyProps {
    /// Left, top, right, bottom; absent: 91440 / 45720 / 91440 / 45720 EMU (7.2 / 3.6 pt).
    pub insets: [Emu; 4],
    pub anchor: VAnchor,
    /// `wrap="none"`: one unlimited line per paragraph.
    pub no_wrap: bool,
    pub autofit: AutoFit,
    pub direction: TextDirection,
    /// `@rot`: the text's own rotation on top of the shape's.
    pub rot: Angle,
    pub upright: bool,
    /// A text warp (`a:prstTxWarp`), 3D text or columns: kept on save, not laid out.
    pub kept: bool,
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
        }
    }
}

impl BodyProps {
    pub fn parse(n: Node) -> BodyProps {
        let d = BodyProps::default();
        let ins = |k: &str, v: Emu| n.attribute(k).and_then(Emu::parse).unwrap_or(v);
        let el = |name: &str| n.children().find(|c| c.is_element() && c.tag_name().name() == name && c.tag_name().namespace() == Some(A));
        let autofit = if el("spAutoFit").is_some() {
            AutoFit::Shape
        } else if let Some(nf) = el("normAutofit") {
            let p = |k: &str, v: i32| nf.attribute(k).and_then(Pct::parse).unwrap_or(Pct(v));
            AutoFit::Normal { font_scale: p("fontScale", 100_000), line_reduction: p("lnSpcReduction", 0) }
        } else {
            AutoFit::None
        };
        BodyProps {
            insets: [ins("lIns", d.insets[0]), ins("tIns", d.insets[1]), ins("rIns", d.insets[2]), ins("bIns", d.insets[3])],
            anchor: match n.attribute("anchor") {
                None | Some("t") => VAnchor::Top,
                Some("ctr") => VAnchor::Centre,
                Some("b") => VAnchor::Bottom,
                Some(_) => VAnchor::Other,
            },
            no_wrap: n.attribute("wrap") == Some("none"),
            autofit,
            direction: TextDirection::parse(n.attribute("vert")),
            rot: n.attribute("rot").and_then(Angle::parse).unwrap_or_default(),
            upright: matches!(n.attribute("upright"), Some("1" | "true")),
            kept: el("prstTxWarp").is_some() || el("scene3d").is_some() || el("flatTx").is_some() || n.attribute("numCol").is_some_and(|v| v != "1"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body_properties_parse_with_their_defaults() {
        let xml = format!(
            r#"<wps:bodyPr xmlns:wps="w" xmlns:a="{A}" lIns="0" anchor="ctr" wrap="none" vert="vert270" upright="1"><a:spAutoFit/></wps:bodyPr>"#
        );
        let d = roxmltree::Document::parse(&xml).unwrap();
        let b = BodyProps::parse(d.root_element());
        assert_eq!(b.insets, [Emu(0), Emu(45720), Emu(91440), Emu(45720)]);
        assert_eq!((b.anchor, b.no_wrap, b.autofit, b.direction, b.upright), (VAnchor::Centre, true, AutoFit::Shape, TextDirection::Vert270, true));
    }
}
