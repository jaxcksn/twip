//! Fill, line and effect reading

use roxmltree::Node;

use super::colour::{Ns, child_colour, child_colour_in, colour};
use crate::docx::xml::A;
use crate::drawing::colour::Colour;
use crate::drawing::paint::{
    BlipAdjust, BlipFill, BlipMode, Cap, Compound, Dash, EffectList, EndKind, EndSize, Fill, Glow, Gradient, GradientKind, GradientPath, InnerShadow,
    Join, Line, LineEnd, OuterShadow, PatternPreset, PresetDash, PresetShadow, RectAlign, Reflection, StyleRefs, TileFlip,
};
use crate::drawing::units::{Angle, Emu, Pct};

pub type ImageLookup<'a> = &'a mut dyn FnMut(&str) -> Option<usize>;

fn a_child<'a, 'i>(n: Node<'a, 'i>, name: &str) -> Option<Node<'a, 'i>> {
    n.children().find(|c| c.is_element() && c.tag_name().name() == name && c.tag_name().namespace() == Some(A))
}

fn pct_attr(n: Node, k: &str) -> Option<Pct> {
    pct_in(Ns::A, n, k)
}

fn emu_attr(n: Node, k: &str) -> Option<Emu> {
    emu_in(Ns::A, n, k)
}

fn bool_attr(n: Node, k: &str) -> Option<bool> {
    bool_in(Ns::A, n, k)
}

fn pct_in(ns: Ns, n: Node, k: &str) -> Option<Pct> {
    ns.attr(n, k).and_then(Pct::parse)
}

fn emu_in(ns: Ns, n: Node, k: &str) -> Option<Emu> {
    ns.attr(n, k).and_then(Emu::parse)
}

fn angle_in(ns: Ns, n: Node, k: &str) -> Option<Angle> {
    ns.attr(n, k).and_then(Angle::parse)
}

fn bool_in(ns: Ns, n: Node, k: &str) -> Option<bool> {
    ns.attr(n, k).map(|v| matches!(v, "1" | "true"))
}

fn rel_rect(n: Option<Node>) -> [Pct; 4] {
    rel_rect_in(Ns::A, n)
}

fn rel_rect_in(ns: Ns, n: Option<Node>) -> [Pct; 4] {
    n.map_or([Pct(0); 4], |n| ["l", "t", "r", "b"].map(|k| pct_in(ns, n, k).unwrap_or_default()))
}

/// The most gradient stops a `w14:` fill keeps: Word's dialog holds ten, and a run's stops are resolved for every line
/// it is drawn on.
pub const MAX_W14_STOPS: usize = 32;

// Fills

fn tile_flip(v: Option<&str>) -> TileFlip {
    match v {
        Some("x") => TileFlip::X,
        Some("y") => TileFlip::Y,
        Some("xy") => TileFlip::Xy,
        _ => TileFlip::None,
    }
}

/// The relationship a blip's picture is read from: its SVG (`asvg:svgBlip`, Office 2016) when it has one, else its
/// own (the SVG's PNG fallback).
pub fn blip_embed<'a>(blip: Node<'a, '_>) -> Option<&'a str> {
    let r = (crate::docx::xml::R, "embed");
    blip.descendants().find(|n| n.tag_name().name() == "svgBlip").and_then(|s| s.attribute(r)).or_else(|| blip.attribute(r))
}

pub fn blip_fill(n: Node, image: ImageLookup) -> BlipFill {
    let blip = a_child(n, "blip");
    let rid = blip.and_then(blip_embed);
    let adjust = blip
        .into_iter()
        .flat_map(|b| b.children().filter(Node::is_element))
        .filter_map(|e| {
            Some(match e.tag_name().name() {
                "alphaModFix" => BlipAdjust::AlphaModFix(pct_attr(e, "amt").unwrap_or(Pct::ONE)),
                "grayscl" => BlipAdjust::Grayscale,
                "lum" => BlipAdjust::Lum { bright: pct_attr(e, "bright").unwrap_or_default(), contrast: pct_attr(e, "contrast").unwrap_or_default() },
                "biLevel" => BlipAdjust::BiLevel(pct_attr(e, "thresh").unwrap_or(Pct(50000))),
                "duotone" => {
                    let mut cs = e.children().filter(Node::is_element).filter_map(colour);
                    BlipAdjust::Duotone([cs.next()?, cs.next()?])
                }
                "clrChange" => {
                    let c = |name: &str| e.children().find(|c| c.tag_name().name() == name)?.children().filter(Node::is_element).find_map(colour);
                    BlipAdjust::ClrChange { from: c("clrFrom")?, to: c("clrTo")?, use_a: !matches!(e.attribute("useA"), Some("0" | "false")) }
                }
                _ => return None,
            })
        })
        .collect();
    let mode = if let Some(t) = a_child(n, "tile") {
        BlipMode::Tile {
            tx: emu_attr(t, "tx").unwrap_or_default(),
            ty: emu_attr(t, "ty").unwrap_or_default(),
            sx: pct_attr(t, "sx").unwrap_or(Pct::ONE),
            sy: pct_attr(t, "sy").unwrap_or(Pct::ONE),
            flip: tile_flip(t.attribute("flip")),
            align: rect_align(t.attribute("algn")).unwrap_or(RectAlign::Tl),
        }
    } else {
        BlipMode::Stretch(rel_rect(a_child(n, "stretch").and_then(|s| a_child(s, "fillRect"))))
    };
    BlipFill {
        image: rid.and_then(image),
        src: rel_rect(a_child(n, "srcRect")),
        mode,
        adjust,
        rot_with_shape: bool_attr(n, "rotWithShape").unwrap_or(true),
    }
}

pub fn fill(n: Node) -> Option<Fill> {
    fill_with(n, &mut |_| None)
}

pub fn fill_with(n: Node, image: ImageLookup) -> Option<Fill> {
    fill_in(n, Ns::A, image)
}

/// [`fill_with`] in `ns`. Word 2010's text fills are no fill, a solid and a gradient, the gradient's stops at most
/// [`MAX_W14_STOPS`].
pub fn fill_in(n: Node, ns: Ns, image: ImageLookup) -> Option<Fill> {
    if n.tag_name().namespace() != Some(ns.uri()) {
        return None;
    }
    Some(match n.tag_name().name() {
        "noFill" => Fill::None,
        "solidFill" => Fill::Solid(child_colour_in(n, ns).unwrap_or_else(|| Colour::srgb(0))),
        // Word 2010's gradient without its stops is solid black.
        "gradFill" if ns == Ns::W14 && ns.child(n, "gsLst").is_none() => Fill::Solid(Colour::srgb(0)),
        "gradFill" => {
            let most = if ns == Ns::W14 { MAX_W14_STOPS } else { usize::MAX };
            let stops = ns
                .child(n, "gsLst")
                .into_iter()
                .flat_map(|l| l.children().filter(Node::is_element))
                .filter_map(|gs| Some((pct_in(ns, gs, "pos").unwrap_or_default(), child_colour_in(gs, ns)?)))
                .take(most)
                .collect();
            let kind = if let Some(p) = ns.child(n, "path") {
                let path = match ns.attr(p, "path") {
                    Some("rect") => GradientPath::Rect,
                    Some("shape") => GradientPath::Shape,
                    _ => GradientPath::Circle,
                };
                GradientKind::Path { path, focus: rel_rect_in(ns, ns.child(p, "fillToRect")) }
            } else {
                let lin = ns.child(n, "lin");
                GradientKind::Linear {
                    angle: lin.and_then(|l| angle_in(ns, l, "ang")).unwrap_or_default(),
                    scaled: lin.and_then(|l| bool_in(ns, l, "scaled")).unwrap_or(false),
                }
            };
            Fill::Gradient(Gradient {
                stops,
                kind,
                tile: rel_rect_in(ns, ns.child(n, "tileRect")),
                flip: tile_flip(ns.attr(n, "flip")),
                rot_with_shape: bool_in(ns, n, "rotWithShape").unwrap_or(true),
            })
        }
        _ if ns == Ns::W14 => return None,
        "pattFill" => {
            let c = |k: &str| a_child(n, k).and_then(child_colour);
            Fill::Pattern {
                preset: n.attribute("prst").and_then(PatternPreset::parse).unwrap_or(PatternPreset::Pct5),
                fg: c("fgClr").unwrap_or_else(|| Colour::srgb(0)),
                bg: c("bgClr").unwrap_or_else(|| Colour::srgb(0xFFFFFF)),
            }
        }
        "blipFill" => Fill::Blip(blip_fill(n, image)),
        "grpFill" => Fill::Group,
        _ => return None,
    })
}

pub fn child_fill(n: Node, image: ImageLookup) -> Option<Fill> {
    child_fill_in(n, Ns::A, image)
}

pub fn child_fill_in(n: Node, ns: Ns, image: ImageLookup) -> Option<Fill> {
    n.children().filter(Node::is_element).find_map(|c| fill_in(c, ns, image))
}

fn rect_align(v: Option<&str>) -> Option<RectAlign> {
    v.and_then(RectAlign::parse)
}

/// An alignment in `ns`, `dflt` when absent: Word 2010's `none` (its default) is about the box's origin, its top left.
fn rect_align_in(ns: Ns, v: Option<&str>, dflt: RectAlign) -> RectAlign {
    match (ns, v) {
        (Ns::W14, None | Some("none")) => RectAlign::Tl,
        _ => rect_align(v).unwrap_or(dflt),
    }
}

// Lines

fn line_end(n: Option<Node>) -> Option<LineEnd> {
    let n = n?;
    let kind = match n.attribute("type") {
        Some("triangle") => EndKind::Triangle,
        Some("stealth") => EndKind::Stealth,
        Some("diamond") => EndKind::Diamond,
        Some("oval") => EndKind::Oval,
        Some("arrow") => EndKind::Arrow,
        _ => return None,
    };
    let size = |k: &str| match n.attribute(k) {
        Some("sm") => EndSize::Small,
        Some("lg") => EndSize::Large,
        _ => EndSize::Medium,
    };
    Some(LineEnd { kind, width: size("w"), length: size("len") })
}

pub fn line(n: Node) -> Line {
    line_with(n, &mut |_| None)
}

pub fn line_with(n: Node, image: ImageLookup) -> Line {
    line_in(n, Ns::A, image)
}

/// [`line_with`] in `ns`. Word 2010's text outline has no custom dash and no line ends.
pub fn line_in(n: Node, ns: Ns, image: ImageLookup) -> Line {
    let a = ns == Ns::A;
    let dash = ns.child(n, "prstDash").and_then(|d| ns.attr(d, "val")).and_then(PresetDash::parse).map(Dash::Preset).or_else(|| {
        a_child(n, "custDash").filter(|_| a).map(|c| {
            Dash::Custom(
                c.children()
                    .filter(|d| d.is_element())
                    .map(|d| (pct_attr(d, "d").unwrap_or_default(), pct_attr(d, "sp").unwrap_or_default()))
                    .collect(),
            )
        })
    });
    let join = if ns.child(n, "round").is_some() {
        Some(Join::Round)
    } else if ns.child(n, "bevel").is_some() {
        Some(Join::Bevel)
    } else {
        ns.child(n, "miter").map(|m| Join::Miter(pct_in(ns, m, "lim").unwrap_or(Pct(800_000))))
    };
    Line {
        width: emu_in(ns, n, "w"),
        fill: child_fill_in(n, ns, image),
        dash,
        compound: ns.attr(n, "cmpd").map(|v| match v {
            "dbl" => Compound::Double,
            "thickThin" => Compound::ThickThin,
            "thinThick" => Compound::ThinThick,
            "tri" => Compound::Triple,
            _ => Compound::Single,
        }),
        cap: ns.attr(n, "cap").map(|v| match v {
            "rnd" => Cap::Round,
            "sq" => Cap::Square,
            _ => Cap::Flat,
        }),
        join,
        inset: ns.attr(n, "algn").map(|v| v == "in"),
        head: line_end(a_child(n, "headEnd").filter(|_| a)),
        tail: line_end(a_child(n, "tailEnd").filter(|_| a)),
    }
}

// Effects

/// An `a:outerShdw` or a `w14:shadow`, with its schema's defaults (Word 2010's: no scale, no alignment).
pub fn outer_shadow(c: Node, ns: Ns) -> OuterShadow {
    let emu = |k: &str| emu_in(ns, c, k).unwrap_or_default();
    let ang = |k: &str| angle_in(ns, c, k).unwrap_or_default();
    let w14 = ns == Ns::W14;
    let scale = if w14 { Pct(0) } else { Pct::ONE };
    OuterShadow {
        blur: emu("blurRad"),
        dist: emu("dist"),
        dir: ang("dir"),
        sx: pct_in(ns, c, "sx").unwrap_or(scale),
        sy: pct_in(ns, c, "sy").unwrap_or(scale),
        kx: ang("kx"),
        ky: ang("ky"),
        align: rect_align_in(ns, ns.attr(c, "algn"), RectAlign::B),
        colour: child_colour_in(c, ns).unwrap_or_else(|| Colour::srgb(0)),
        rot_with_shape: bool_in(ns, c, "rotWithShape").unwrap_or(true),
    }
}

/// An `a:glow` or a `w14:glow`.
pub fn glow(c: Node, ns: Ns) -> Glow {
    Glow { radius: emu_in(ns, c, "rad").unwrap_or_default(), colour: child_colour_in(c, ns).unwrap_or_else(|| Colour::srgb(0)) }
}

/// An `a:reflection` or a `w14:reflection`, with its schema's defaults (Word 2010's are all zero, aligned to nothing).
pub fn reflection(c: Node, ns: Ns) -> Reflection {
    let emu = |k: &str| emu_in(ns, c, k).unwrap_or_default();
    let ang = |k: &str| angle_in(ns, c, k).unwrap_or_default();
    let w14 = ns == Ns::W14;
    let pct = |k: &str, d: i32| pct_in(ns, c, k).unwrap_or(Pct(if w14 { 0 } else { d }));
    Reflection {
        blur: emu("blurRad"),
        dist: emu("dist"),
        dir: ang("dir"),
        fade_dir: angle_in(ns, c, "fadeDir").unwrap_or(Angle(if w14 { 0 } else { 5_400_000 })),
        sx: pct("sx", 100_000),
        sy: pct("sy", 100_000),
        kx: ang("kx"),
        ky: ang("ky"),
        align: rect_align_in(ns, ns.attr(c, "algn"), RectAlign::B),
        start_alpha: pct("stA", 100_000),
        start_pos: pct("stPos", 0),
        end_alpha: pct("endA", 0),
        end_pos: pct("endPos", 100_000),
        rot_with_shape: bool_in(ns, c, "rotWithShape").unwrap_or(true),
    }
}

pub fn effect_list(n: Node) -> EffectList {
    let mut e = EffectList::default();
    for c in n.children().filter(Node::is_element) {
        if c.tag_name().namespace() != Some(Ns::A.uri()) {
            e.kept = true;
            continue;
        }
        let colour = || child_colour(c).unwrap_or_else(|| Colour::srgb(0));
        let emu = |k: &str| emu_attr(c, k).unwrap_or_default();
        let ang = |k: &str| angle_in(Ns::A, c, k).unwrap_or_default();
        match c.tag_name().name() {
            "outerShdw" => e.outer = Some(outer_shadow(c, Ns::A)),
            "innerShdw" => e.inner = Some(InnerShadow { blur: emu("blurRad"), dist: emu("dist"), dir: ang("dir"), colour: colour() }),
            "prstShdw" => {
                let preset = c.attribute("prst").and_then(|p| p.strip_prefix("shdw")?.parse().ok()).filter(|n| (1..=20).contains(n)).unwrap_or(1);
                e.preset = Some(PresetShadow { preset, dist: emu("dist"), dir: ang("dir"), colour: colour() })
            }
            "glow" => e.glow = Some(glow(c, Ns::A)),
            "softEdge" => e.soft_edge = Some(emu("rad")),
            "blur" => e.blur = Some(emu("rad")),
            "reflection" => e.reflection = Some(reflection(c, Ns::A)),
            _ => e.kept = true,
        }
    }
    e
}

// Style references

pub fn style_refs(n: Node) -> StyleRefs {
    let r = |k: &str| a_child(n, k).map(|e| (e.attribute("idx").and_then(|v| v.parse().ok()).unwrap_or(0), child_colour(e)));
    StyleRefs { line: r("lnRef"), fill: r("fillRef"), effect: r("effectRef"), font: a_child(n, "fontRef").and_then(child_colour) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::docx::drawing::theme::theme;
    use crate::drawing::colour::{Rgba, SchemeSlot};
    use crate::drawing::paint::{MAX_EFFECT_PT, Paint, Palette, paint, resolve_effects, resolve_fill, resolve_line};
    use crate::drawing::theme::{ColourMap, Theme};

    fn node_xml(xml: &str) -> String {
        format!(r#"<r xmlns:a="{A}" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">{xml}</r>"#)
    }

    #[test]
    fn fills_parse() {
        let x = node_xml(
            r#"<a:gradFill rotWithShape="0"><a:gsLst><a:gs pos="100000"><a:srgbClr val="FFFFFF"/></a:gs><a:gs pos="0"><a:schemeClr val="accent1"/></a:gs></a:gsLst><a:lin ang="5400000" scaled="1"/></a:gradFill>"#,
        );
        let d = roxmltree::Document::parse(&x).unwrap();
        let f = child_fill(d.root_element(), &mut |_| None).unwrap();
        let Fill::Gradient(g) = &f else { panic!() };
        assert_eq!(g.kind, GradientKind::Linear { angle: Angle(5_400_000), scaled: true });
        assert!(!g.rot_with_shape);
        let t = Theme::default();
        let m = ColourMap::default();
        let Paint::Gradient { stops, .. } = paint(&f, Palette { theme: &t, map: &m }, None, &Paint::None) else { panic!() };
        assert_eq!(stops[0], (0.0, t.colour(SchemeSlot::Accent(1))));
    }

    #[test]
    fn explicit_properties_win_over_style_references() {
        let theme_xml = r#"<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:themeElements><a:fmtScheme>
            <a:fillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:fillStyleLst>
            <a:lnStyleLst><a:ln w="6350"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln><a:ln w="12700"><a:solidFill><a:schemeClr val="phClr"><a:shade val="50000"/></a:schemeClr></a:solidFill></a:ln></a:lnStyleLst>
            </a:fmtScheme></a:themeElements></a:theme>"#;
        let t = theme(Some(theme_xml));
        let m = ColourMap::default();
        let p = Palette { theme: &t, map: &m };
        let style =
            StyleRefs { line: Some((2, Some(Colour::srgb(0xFF0000)))), fill: Some((1, Some(Colour::srgb(0x00FF00)))), effect: None, font: None };
        assert_eq!(resolve_fill(None, Some(&style), p, &Paint::None), Paint::Solid(Rgba::rgb(0x00FF00)));
        assert_eq!(resolve_fill(Some(&Fill::None), Some(&style), p, &Paint::None), Paint::None);
        let l = resolve_line(None, Some(&style), p, &Paint::None);
        assert_eq!(l.width, 1.0);
        assert_eq!(l.paint, Paint::Solid(Colour::srgb(0xFF0000).with(crate::drawing::colour::ColourMod::Shade(Pct(50000))).resolve(&t, &m, None)));
        let own = Line { width: Some(Emu(38100)), ..Line::default() };
        assert_eq!(resolve_line(Some(&own), Some(&style), p, &Paint::None).width, 3.0);
        assert!(!resolve_line(None, None, p, &Paint::None).is_visible());
    }

    #[test]
    fn effect_lists_note_what_is_not_drawn() {
        let x = node_xml(
            r#"<a:effectLst><a:outerShdw blurRad="50800" dist="38100" dir="2700000"><a:srgbClr val="000000"/></a:outerShdw><a:fillOverlay blend="over"/></a:effectLst>"#,
        );
        let d = roxmltree::Document::parse(&x).unwrap();
        let e = effect_list(d.root_element().first_element_child().unwrap());
        assert!(e.kept);
        assert_eq!(e.outer.as_ref().map(|s| (s.blur, s.dist)), Some((Emu(50800), Emu(38100))));
    }

    #[test]
    fn a_reflection_reads_with_the_schema_defaults_and_resolves() {
        let x = node_xml(
            r#"<a:effectLst><a:reflection blurRad="6350" stA="50000" endA="300" endPos="35000" dir="5400000" sy="-100000" algn="bl" rotWithShape="0"/><a:reflection/></a:effectLst>"#,
        );
        let d = roxmltree::Document::parse(&x).unwrap();
        let list = d.root_element().first_element_child().unwrap();
        let e = effect_list(list);
        assert!(!e.kept && !e.is_empty(), "a reflection is drawn, not kept");
        let r = e.reflection.as_ref().expect("the last reflection");
        assert_eq!(
            (r.start_alpha, r.end_alpha, r.end_pos, r.fade_dir, r.sy, r.align, r.rot_with_shape),
            (Pct(100_000), Pct(0), Pct(100_000), Angle(5_400_000), Pct(100_000), RectAlign::B, true)
        );
        let one = effect_list(roxmltree::Document::parse(&node_xml(r#"<a:effectLst><a:reflection blurRad="6350" stA="50000" endA="300" endPos="35000" dir="5400000" sy="-100000" algn="bl" rotWithShape="0"/></a:effectLst>"#)).unwrap().root_element().first_element_child().unwrap());
        let (t, m) = (Theme::default(), ColourMap::default());
        let resolved = resolve_effects(Some(&one), None, Palette { theme: &t, map: &m }).reflection.expect("resolved");
        assert_eq!((resolved.blur, resolved.scale, resolved.start, resolved.rot_with_shape), (0.5, [1.0, -1.0], (0.0, 0.5), false));
        assert_eq!(resolved.end, (0.35, 0.003));
    }

    #[test]
    fn word_2010_effects_take_their_own_schema_defaults() {
        let w14 = Ns::W14.uri();
        let x = format!(
            r#"<r xmlns:w14="{w14}"><w14:shadow w14:blurRad="12700"><w14:srgbClr w14:val="000000"/></w14:shadow><w14:reflection w14:stPos="0" w14:sy="-100000"/><w14:gradFill/><w14:gradFill><w14:gsLst><w14:gs w14:pos="0"><w14:srgbClr w14:val="FF0000"/></w14:gs></w14:gsLst></w14:gradFill></r>"#
        );
        let d = roxmltree::Document::parse(&x).unwrap();
        let [shadow, reflection, bare, listed] = d.root_element().children().filter(Node::is_element).collect::<Vec<_>>()[..] else { panic!() };
        let s = outer_shadow(shadow, Ns::W14);
        assert_eq!((s.sx, s.sy, s.align, s.blur), (Pct(0), Pct(0), RectAlign::Tl, Emu(12700)));
        let r = super::reflection(reflection, Ns::W14);
        assert_eq!((r.start_alpha, r.end_pos, r.fade_dir, r.sx, r.sy, r.align), (Pct(0), Pct(0), Angle(0), Pct(0), Pct(-100_000), RectAlign::Tl));
        assert_eq!(fill_in(bare, Ns::W14, &mut |_| None), Some(Fill::Solid(Colour::srgb(0))), "no stops: solid black");
        assert!(matches!(fill_in(listed, Ns::W14, &mut |_| None), Some(Fill::Gradient(g)) if g.stops.len() == 1));
        // DrawingML's defaults stay its own, and a foreign effect is kept, not drawn.
        let a = node_xml(r#"<a:effectLst><a:outerShdw/><x:reflection xmlns:x="urn:x"/></a:effectLst>"#);
        let d = roxmltree::Document::parse(&a).unwrap();
        let e = effect_list(d.root_element().first_element_child().unwrap());
        assert_eq!(e.outer.as_ref().map(|s| (s.sx, s.align)), Some((Pct::ONE, RectAlign::B)));
        assert!(e.kept && e.reflection.is_none());
    }

    #[test]
    fn absurd_effect_sizes_resolve_to_the_clamp() {
        let x = node_xml(
            r#"<a:effectLst><a:outerShdw blurRad="9000000000000" dist="-9000000000000"><a:srgbClr val="000000"/></a:outerShdw><a:innerShdw blurRad="9000000000000" dist="9000000000000"><a:srgbClr val="000000"/></a:innerShdw><a:glow rad="9000000000000"><a:srgbClr val="00FF00"/></a:glow><a:softEdge rad="9000000000000"/><a:blur rad="9000000000000"/><a:reflection blurRad="9000000000000" dist="9000000000000"/></a:effectLst>"#,
        );
        let d = roxmltree::Document::parse(&x).unwrap();
        let e = effect_list(d.root_element().first_element_child().unwrap());
        let (t, m) = (Theme::default(), ColourMap::default());
        let r = resolve_effects(Some(&e), None, Palette { theme: &t, map: &m });
        let (outer, inner, refl) = (r.outer.unwrap(), r.inner.unwrap(), r.reflection.unwrap());
        assert_eq!((outer.blur, outer.dist), (MAX_EFFECT_PT, -MAX_EFFECT_PT));
        assert_eq!((inner.blur, inner.dist), (MAX_EFFECT_PT, MAX_EFFECT_PT));
        assert_eq!((refl.blur, refl.dist), (MAX_EFFECT_PT, MAX_EFFECT_PT));
        assert_eq!((r.glow.map(|g| g.0), r.soft_edge, r.blur), (Some(MAX_EFFECT_PT), Some(MAX_EFFECT_PT), Some(MAX_EFFECT_PT)));
    }
}
