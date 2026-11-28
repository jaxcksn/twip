//! DrawingML colour reading

use roxmltree::Node;

use crate::docx::xml::{A, W14};
use crate::drawing::colour::{Colour, ColourBase, ColourMod, SchemeSlot, preset_colour};
use crate::drawing::units::{Angle, Pct};

/// The namespace a DrawingML reader reads: DrawingML's own, its attributes unqualified, or Word 2010's copy of it for
/// text effects (`w14:`), its attributes qualified too.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ns {
    A,
    W14,
}

impl Ns {
    pub fn uri(self) -> &'static str {
        match self {
            Ns::A => A,
            Ns::W14 => W14,
        }
    }

    pub fn attr<'a>(self, n: Node<'a, '_>, k: &str) -> Option<&'a str> {
        match self {
            Ns::A => n.attribute(k),
            Ns::W14 => n.attribute((W14, k)),
        }
    }

    /// `n`'s first child element `name` in this namespace.
    pub fn child<'a, 'i>(self, n: Node<'a, 'i>, name: &str) -> Option<Node<'a, 'i>> {
        n.children().find(|c| c.is_element() && c.tag_name().name() == name && c.tag_name().namespace() == Some(self.uri()))
    }
}

fn colour_mod(n: Node, ns: Ns) -> Option<ColourMod> {
    use ColourMod::*;
    if ns == Ns::W14 && n.tag_name().namespace() != Some(W14) {
        return None;
    }
    let pct = || ns.attr(n, "val").and_then(Pct::parse);
    let ang = || ns.attr(n, "val").and_then(Angle::parse);
    Some(match n.tag_name().name() {
        "tint" => Tint(pct()?),
        "shade" => Shade(pct()?),
        "comp" => Comp,
        "inv" => Inv,
        "gray" => Gray,
        "alpha" => Alpha(pct()?),
        "alphaOff" => AlphaOff(pct()?),
        "alphaMod" => AlphaMod(pct()?),
        "hue" => Hue(ang()?),
        "hueOff" => HueOff(ang()?),
        "hueMod" => HueMod(pct()?),
        "sat" => Sat(pct()?),
        "satOff" => SatOff(pct()?),
        "satMod" => SatMod(pct()?),
        "lum" => Lum(pct()?),
        "lumOff" => LumOff(pct()?),
        "lumMod" => LumMod(pct()?),
        "red" => Red(pct()?),
        "redOff" => RedOff(pct()?),
        "redMod" => RedMod(pct()?),
        "green" => Green(pct()?),
        "greenOff" => GreenOff(pct()?),
        "greenMod" => GreenMod(pct()?),
        "blue" => Blue(pct()?),
        "blueOff" => BlueOff(pct()?),
        "blueMod" => BlueMod(pct()?),
        "gamma" => Gamma,
        "invGamma" => InvGamma,
        _ => return None,
    })
}

/// The most colour changes a `w14:` colour keeps: Word writes at most four.
pub const MAX_W14_MODS: usize = 16;

pub fn colour(n: Node) -> Option<Colour> {
    colour_in(n, Ns::A)
}

/// [`colour`] in `ns`. A `w14:` colour keeps at most [`MAX_W14_MODS`] changes: a run's colours are resolved for every
/// line it is drawn on.
pub fn colour_in(n: Node, ns: Ns) -> Option<Colour> {
    if n.tag_name().namespace() != Some(ns.uri()) {
        return None;
    }
    let hex = |k: &str| ns.attr(n, k).and_then(|v| u32::from_str_radix(v.trim(), 16).ok());
    let pct = |k: &str| ns.attr(n, k).and_then(Pct::parse).unwrap_or_default();
    let base = match n.tag_name().name() {
        "srgbClr" => ColourBase::Srgb(hex("val")?),
        "scrgbClr" => ColourBase::ScRgb([pct("r"), pct("g"), pct("b")]),
        "hslClr" => ColourBase::Hsl { hue: ns.attr(n, "hue").and_then(Angle::parse).unwrap_or_default(), sat: pct("sat"), lum: pct("lum") },
        "sysClr" => {
            let name = ns.attr(n, "val").unwrap_or("").to_string();
            let last = hex("lastClr").unwrap_or_else(|| system_colour(&name));
            ColourBase::Sys { name, last }
        }
        "prstClr" => {
            let name = ns.attr(n, "val")?.to_string();
            let value = preset_colour(&name)?;
            ColourBase::Preset { name, value }
        }
        "schemeClr" => ColourBase::Scheme(SchemeSlot::parse(ns.attr(n, "val")?)?),
        _ => return None,
    };
    let most = if ns == Ns::W14 { MAX_W14_MODS } else { usize::MAX };
    let mods = n.children().filter(|c| c.is_element()).filter_map(|c| colour_mod(c, ns)).take(most).collect();
    Some(Colour { base, mods })
}

pub fn child_colour(n: Node) -> Option<Colour> {
    child_colour_in(n, Ns::A)
}

pub fn child_colour_in(n: Node, ns: Ns) -> Option<Colour> {
    n.children().filter(|c| c.is_element()).find_map(|c| colour_in(c, ns))
}

fn system_colour(name: &str) -> u32 {
    match name {
        "window" | "highlightText" | "btnHighlight" | "3dLight" | "menu" | "infoBk" => 0xFFFFFF,
        "highlight" | "menuHighlight" => 0x0078D7,
        "btnFace" | "menuBar" | "scrollBar" => 0xF0F0F0,
        "grayText" | "btnShadow" => 0xA0A0A0,
        "activeCaption" | "gradientActiveCaption" => 0x99B4D1,
        "inactiveCaption" | "gradientInactiveCaption" => 0xBFCDDB,
        _ => 0x000000,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawing::theme::{ColourMap, Theme};

    fn parse(xml: &str) -> Colour {
        let xml = format!(r#"<r xmlns:a="{A}">{xml}</r>"#);
        let doc = roxmltree::Document::parse(&xml).unwrap();
        child_colour(doc.root_element()).unwrap()
    }

    fn rgb(xml: &str) -> u32 {
        parse(xml).resolve(&Theme::default(), &ColourMap::default(), None).to_rgb()
    }

    #[test]
    fn colour_models_resolve() {
        assert_eq!(rgb(r#"<a:srgbClr val="FF8000"/>"#), 0xFF8000);
        assert_eq!(rgb(r#"<a:hslClr hue="7200000" sat="100000" lum="50000"/>"#), 0x00FF00);
        assert_eq!(rgb(r#"<a:scrgbClr r="100000" g="0" b="0"/>"#), 0xFF0000);
        assert_eq!(rgb(r#"<a:sysClr val="windowText" lastClr="123456"/>"#), 0x123456);
        assert_eq!(rgb(r#"<a:prstClr val="navy"/>"#), 0x000080);
    }

    #[test]
    fn hsl_transforms_change_luminance_and_hue() {
        assert_eq!(rgb(r#"<a:srgbClr val="FF0000"><a:lumMod val="50000"/></a:srgbClr>"#), 0x800000);
        assert_eq!(rgb(r#"<a:srgbClr val="FF0000"><a:lumMod val="60000"/><a:lumOff val="40000"/></a:srgbClr>"#), 0xFF6666);
        assert_eq!(rgb(r#"<a:srgbClr val="FF0000"><a:comp/></a:srgbClr>"#), 0x00FFFF);
        assert_eq!(rgb(r#"<a:srgbClr val="156082"><a:inv/></a:srgbClr>"#), 0xFEF1E4);
        assert_eq!(rgb(r#"<a:srgbClr val="156082"><a:gray/></a:srgbClr>"#), 0x535353);
        assert_eq!(rgb(r#"<a:srgbClr val="C04020"><a:satMod val="150000"/></a:srgbClr>"#) >> 16, 232);
    }

    #[test]
    fn tint_and_shade_work_in_linear_rgb() {
        assert_eq!(rgb(r#"<a:srgbClr val="FFFFFF"><a:shade val="50000"/></a:srgbClr>"#), 0xBCBCBC);
        assert_eq!(rgb(r#"<a:srgbClr val="000000"><a:tint val="50000"/></a:srgbClr>"#), 0xBCBCBC);
    }

    #[test]
    fn alpha_transforms_compose() {
        let c = parse(r#"<a:srgbClr val="000000"><a:alpha val="50000"/><a:alphaMod val="50000"/></a:srgbClr>"#);
        assert_eq!(c.resolve(&Theme::default(), &ColourMap::default(), None).a, 64);
    }
}
