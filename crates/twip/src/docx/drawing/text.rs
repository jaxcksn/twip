//! Text body property reading

use roxmltree::Node;

use crate::docx::xml::A;
use crate::drawing::text::{AutoFit, BodyProps, TextDirection, VAnchor};
use crate::drawing::units::{Angle, Emu, Pct};

pub fn body_props(n: Node) -> BodyProps {
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
        kept: el("prstTxWarp").is_some() || el("scene3d").is_some() || el("flatTx").is_some(),
        columns: (
            n.attribute("numCol").and_then(|v| v.parse::<u16>().ok()).unwrap_or(1).clamp(1, 16),
            n.attribute("spcCol").and_then(Emu::parse).unwrap_or_default(),
        ),
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
        let b = body_props(d.root_element());
        assert_eq!(b.insets, [Emu(0), Emu(45720), Emu(91440), Emu(45720)]);
        assert_eq!((b.anchor, b.no_wrap, b.autofit, b.direction, b.upright), (VAnchor::Centre, true, AutoFit::Shape, TextDirection::Vert270, true));
    }
}
