//! Fills, lines and effects

use roxmltree::Node;

use super::colour::{Colour, Rgba};
use super::theme::{ColourMap, Theme};
use super::units::{Angle, Emu, Pct};
use crate::docx::xml::A;

pub type ImageLookup<'a> = &'a mut dyn FnMut(&str) -> Option<usize>;

fn a_child<'a, 'i>(n: Node<'a, 'i>, name: &str) -> Option<Node<'a, 'i>> {
    n.children().find(|c| c.is_element() && c.tag_name().name() == name && c.tag_name().namespace() == Some(A))
}

fn pct_attr(n: Node, k: &str) -> Option<Pct> {
    n.attribute(k).and_then(Pct::parse)
}

fn emu_attr(n: Node, k: &str) -> Option<Emu> {
    n.attribute(k).and_then(Emu::parse)
}

fn angle_attr(n: Node, k: &str) -> Option<Angle> {
    n.attribute(k).and_then(Angle::parse)
}

fn bool_attr(n: Node, k: &str) -> Option<bool> {
    n.attribute(k).map(|v| matches!(v, "1" | "true"))
}

fn rel_rect(n: Option<Node>) -> [Pct; 4] {
    n.map_or([Pct(0); 4], |n| ["l", "t", "r", "b"].map(|k| pct_attr(n, k).unwrap_or_default()))
}

// Fills

#[derive(Clone, Debug, PartialEq)]
pub enum Fill {
    None,
    Solid(Colour),
    Gradient(Gradient),
    Pattern { preset: PatternPreset, fg: Colour, bg: Colour },
    Blip(BlipFill),
    Group,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GradientPath {
    Circle,
    Rect,
    Shape,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GradientKind {
    Linear { angle: Angle, scaled: bool },
    Path { path: GradientPath, focus: [Pct; 4] },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TileFlip {
    #[default]
    None,
    X,
    Y,
    Xy,
}

impl TileFlip {
    fn parse(v: Option<&str>) -> TileFlip {
        match v {
            Some("x") => TileFlip::X,
            Some("y") => TileFlip::Y,
            Some("xy") => TileFlip::Xy,
            _ => TileFlip::None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Gradient {
    pub stops: Vec<(Pct, Colour)>,
    pub kind: GradientKind,
    pub tile: [Pct; 4],
    pub flip: TileFlip,
    pub rot_with_shape: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum BlipMode {
    Stretch([Pct; 4]),
    Tile { tx: Emu, ty: Emu, sx: Pct, sy: Pct, flip: TileFlip, align: RectAlign },
}

#[derive(Clone, Debug, PartialEq)]
pub enum BlipAdjust {
    AlphaModFix(Pct),
    Grayscale,
    Lum { bright: Pct, contrast: Pct },
    BiLevel(Pct),
    Duotone([Colour; 2]),
}

#[derive(Clone, Debug, PartialEq)]
pub struct BlipFill {
    pub image: Option<usize>,
    pub src: [Pct; 4],
    pub mode: BlipMode,
    pub adjust: Vec<BlipAdjust>,
    pub rot_with_shape: bool,
}

/// The relationship a blip's picture is read from: its SVG (`asvg:svgBlip`, Office 2016) when it has one, else its
/// own (the SVG's PNG fallback).
pub fn blip_embed<'a>(blip: Node<'a, '_>) -> Option<&'a str> {
    let r = (crate::docx::xml::R, "embed");
    blip.descendants().find(|n| n.tag_name().name() == "svgBlip").and_then(|s| s.attribute(r)).or_else(|| blip.attribute(r))
}

impl BlipFill {
    pub fn parse(n: Node, image: ImageLookup) -> BlipFill {
        let blip = a_child(n, "blip");
        let rid = blip.and_then(blip_embed);
        let adjust = blip
            .into_iter()
            .flat_map(|b| b.children().filter(Node::is_element))
            .filter_map(|e| {
                Some(match e.tag_name().name() {
                    "alphaModFix" => BlipAdjust::AlphaModFix(pct_attr(e, "amt").unwrap_or(Pct::ONE)),
                    "grayscl" => BlipAdjust::Grayscale,
                    "lum" => {
                        BlipAdjust::Lum { bright: pct_attr(e, "bright").unwrap_or_default(), contrast: pct_attr(e, "contrast").unwrap_or_default() }
                    }
                    "biLevel" => BlipAdjust::BiLevel(pct_attr(e, "thresh").unwrap_or(Pct(50000))),
                    "duotone" => {
                        let mut cs = e.children().filter(Node::is_element).filter_map(Colour::parse);
                        BlipAdjust::Duotone([cs.next()?, cs.next()?])
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
                flip: TileFlip::parse(t.attribute("flip")),
                align: RectAlign::parse_opt(t.attribute("algn")).unwrap_or(RectAlign::Tl),
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
}

impl Fill {
    pub fn parse(n: Node) -> Option<Fill> {
        Fill::parse_with(n, &mut |_| None)
    }

    pub fn parse_with(n: Node, image: ImageLookup) -> Option<Fill> {
        if n.tag_name().namespace() != Some(A) {
            return None;
        }
        Some(match n.tag_name().name() {
            "noFill" => Fill::None,
            "solidFill" => Fill::Solid(Colour::child_of(n).unwrap_or_else(|| Colour::srgb(0))),
            "gradFill" => {
                let stops = a_child(n, "gsLst")
                    .into_iter()
                    .flat_map(|l| l.children().filter(Node::is_element))
                    .filter_map(|gs| Some((pct_attr(gs, "pos").unwrap_or_default(), Colour::child_of(gs)?)))
                    .collect();
                let kind = if let Some(p) = a_child(n, "path") {
                    let path = match p.attribute("path") {
                        Some("rect") => GradientPath::Rect,
                        Some("shape") => GradientPath::Shape,
                        _ => GradientPath::Circle,
                    };
                    GradientKind::Path { path, focus: rel_rect(a_child(p, "fillToRect")) }
                } else {
                    let lin = a_child(n, "lin");
                    GradientKind::Linear {
                        angle: lin.and_then(|l| angle_attr(l, "ang")).unwrap_or_default(),
                        scaled: lin.and_then(|l| bool_attr(l, "scaled")).unwrap_or(false),
                    }
                };
                Fill::Gradient(Gradient {
                    stops,
                    kind,
                    tile: rel_rect(a_child(n, "tileRect")),
                    flip: TileFlip::parse(n.attribute("flip")),
                    rot_with_shape: bool_attr(n, "rotWithShape").unwrap_or(true),
                })
            }
            "pattFill" => {
                let c = |k: &str| a_child(n, k).and_then(Colour::child_of);
                Fill::Pattern {
                    preset: n.attribute("prst").and_then(PatternPreset::parse).unwrap_or(PatternPreset::Pct5),
                    fg: c("fgClr").unwrap_or_else(|| Colour::srgb(0)),
                    bg: c("bgClr").unwrap_or_else(|| Colour::srgb(0xFFFFFF)),
                }
            }
            "blipFill" => Fill::Blip(BlipFill::parse(n, image)),
            "grpFill" => Fill::Group,
            _ => return None,
        })
    }

    pub fn child_of(n: Node, image: ImageLookup) -> Option<Fill> {
        n.children().filter(Node::is_element).find_map(|c| Fill::parse_with(c, image))
    }
}

macro_rules! names {
    ($ty:ident { $($v:ident = $s:literal),* $(,)? }) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum $ty { $($v),* }
        impl $ty {
            pub const ALL: &'static [$ty] = &[$($ty::$v),*];
            pub fn parse(s: &str) -> Option<$ty> {
                match s { $($s => Some($ty::$v),)* _ => None }
            }
            pub fn name(self) -> &'static str {
                match self { $($ty::$v => $s),* }
            }
        }
    };
}

names!(PatternPreset {
    Pct5 = "pct5", Pct10 = "pct10", Pct20 = "pct20", Pct25 = "pct25", Pct30 = "pct30", Pct40 = "pct40", Pct50 = "pct50",
    Pct60 = "pct60", Pct70 = "pct70", Pct75 = "pct75", Pct80 = "pct80", Pct90 = "pct90", Horz = "horz", Vert = "vert",
    LtHorz = "ltHorz", LtVert = "ltVert", DkHorz = "dkHorz", DkVert = "dkVert", NarHorz = "narHorz", NarVert = "narVert",
    DashHorz = "dashHorz", DashVert = "dashVert", Cross = "cross", DnDiag = "dnDiag", UpDiag = "upDiag", LtDnDiag = "ltDnDiag",
    LtUpDiag = "ltUpDiag", DkDnDiag = "dkDnDiag", DkUpDiag = "dkUpDiag", WdDnDiag = "wdDnDiag", WdUpDiag = "wdUpDiag",
    DashDnDiag = "dashDnDiag", DashUpDiag = "dashUpDiag", DiagCross = "diagCross", SmCheck = "smCheck", LgCheck = "lgCheck",
    SmGrid = "smGrid", LgGrid = "lgGrid", DotGrid = "dotGrid", SmConfetti = "smConfetti", LgConfetti = "lgConfetti",
    HorzBrick = "horzBrick", DiagBrick = "diagBrick", SolidDmnd = "solidDmnd", OpenDmnd = "openDmnd", DotDmnd = "dotDmnd",
    Plaid = "plaid", Sphere = "sphere", Weave = "weave", Divot = "divot", Shingle = "shingle", Wave = "wave", Trellis = "trellis",
    ZigZag = "zigZag",
});

names!(PresetDash {
    Solid = "solid", Dot = "dot", Dash = "dash", LgDash = "lgDash", DashDot = "dashDot", LgDashDot = "lgDashDot",
    LgDashDotDot = "lgDashDotDot", SysDash = "sysDash", SysDot = "sysDot", SysDashDot = "sysDashDot", SysDashDotDot = "sysDashDotDot",
});

names!(RectAlign { Tl = "tl", T = "t", Tr = "tr", L = "l", Ctr = "ctr", R = "r", Bl = "bl", B = "b", Br = "br" });

impl RectAlign {
    fn parse_opt(v: Option<&str>) -> Option<RectAlign> {
        v.and_then(RectAlign::parse)
    }
}

// Lines

#[derive(Clone, Debug, PartialEq)]
pub enum Dash {
    Preset(PresetDash),
    Custom(Vec<(Pct, Pct)>),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Compound {
    #[default]
    Single,
    Double,
    ThickThin,
    ThinThick,
    Triple,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cap {
    Round,
    Square,
    Flat,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Join {
    Round,
    Bevel,
    Miter(Pct),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndKind {
    Triangle,
    Stealth,
    Diamond,
    Oval,
    Arrow,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndSize {
    Small,
    Medium,
    Large,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LineEnd {
    pub kind: EndKind,
    pub width: EndSize,
    pub length: EndSize,
}

impl LineEnd {
    fn parse(n: Option<Node>) -> Option<LineEnd> {
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
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Line {
    pub width: Option<Emu>,
    pub fill: Option<Fill>,
    pub dash: Option<Dash>,
    pub compound: Option<Compound>,
    pub cap: Option<Cap>,
    pub join: Option<Join>,
    pub inset: Option<bool>,
    pub head: Option<LineEnd>,
    pub tail: Option<LineEnd>,
}

impl Line {
    pub fn parse(n: Node) -> Line {
        Line::parse_with(n, &mut |_| None)
    }

    pub fn parse_with(n: Node, image: ImageLookup) -> Line {
        let dash = a_child(n, "prstDash").and_then(|d| d.attribute("val")).and_then(PresetDash::parse).map(Dash::Preset).or_else(|| {
            a_child(n, "custDash").map(|c| {
                Dash::Custom(
                    c.children()
                        .filter(|d| d.is_element())
                        .map(|d| (pct_attr(d, "d").unwrap_or_default(), pct_attr(d, "sp").unwrap_or_default()))
                        .collect(),
                )
            })
        });
        let join = if a_child(n, "round").is_some() {
            Some(Join::Round)
        } else if a_child(n, "bevel").is_some() {
            Some(Join::Bevel)
        } else {
            a_child(n, "miter").map(|m| Join::Miter(pct_attr(m, "lim").unwrap_or(Pct(800_000))))
        };
        Line {
            width: emu_attr(n, "w"),
            fill: Fill::child_of(n, image),
            dash,
            compound: n.attribute("cmpd").map(|v| match v {
                "dbl" => Compound::Double,
                "thickThin" => Compound::ThickThin,
                "thinThick" => Compound::ThinThick,
                "tri" => Compound::Triple,
                _ => Compound::Single,
            }),
            cap: n.attribute("cap").map(|v| match v {
                "rnd" => Cap::Round,
                "sq" => Cap::Square,
                _ => Cap::Flat,
            }),
            join,
            inset: n.attribute("algn").map(|v| v == "in"),
            head: LineEnd::parse(a_child(n, "headEnd")),
            tail: LineEnd::parse(a_child(n, "tailEnd")),
        }
    }

    fn over(&self, base: &Line) -> Line {
        Line {
            width: self.width.or(base.width),
            fill: self.fill.clone().or_else(|| base.fill.clone()),
            dash: self.dash.clone().or_else(|| base.dash.clone()),
            compound: self.compound.or(base.compound),
            cap: self.cap.or(base.cap),
            join: self.join.or(base.join),
            inset: self.inset.or(base.inset),
            head: self.head.or(base.head),
            tail: self.tail.or(base.tail),
        }
    }
}

// Effects

#[derive(Clone, Debug, PartialEq)]
pub struct OuterShadow {
    pub blur: Emu,
    pub dist: Emu,
    pub dir: Angle,
    pub sx: Pct,
    pub sy: Pct,
    pub kx: Angle,
    pub ky: Angle,
    pub align: RectAlign,
    pub colour: Colour,
    pub rot_with_shape: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct InnerShadow {
    pub blur: Emu,
    pub dist: Emu,
    pub dir: Angle,
    pub colour: Colour,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PresetShadow {
    pub preset: u8,
    pub dist: Emu,
    pub dir: Angle,
    pub colour: Colour,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Glow {
    pub radius: Emu,
    pub colour: Colour,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct EffectList {
    pub outer: Option<OuterShadow>,
    pub inner: Option<InnerShadow>,
    pub preset: Option<PresetShadow>,
    pub glow: Option<Glow>,
    pub soft_edge: Option<Emu>,
    pub blur: Option<Emu>,
    pub kept: bool,
}

impl EffectList {
    pub fn parse(n: Node) -> EffectList {
        let mut e = EffectList::default();
        for c in n.children().filter(Node::is_element) {
            let colour = || Colour::child_of(c).unwrap_or_else(|| Colour::srgb(0));
            let emu = |k: &str| emu_attr(c, k).unwrap_or_default();
            let ang = |k: &str| angle_attr(c, k).unwrap_or_default();
            match c.tag_name().name() {
                "outerShdw" => {
                    e.outer = Some(OuterShadow {
                        blur: emu("blurRad"),
                        dist: emu("dist"),
                        dir: ang("dir"),
                        sx: pct_attr(c, "sx").unwrap_or(Pct::ONE),
                        sy: pct_attr(c, "sy").unwrap_or(Pct::ONE),
                        kx: ang("kx"),
                        ky: ang("ky"),
                        align: RectAlign::parse_opt(c.attribute("algn")).unwrap_or(RectAlign::B),
                        colour: colour(),
                        rot_with_shape: bool_attr(c, "rotWithShape").unwrap_or(true),
                    })
                }
                "innerShdw" => e.inner = Some(InnerShadow { blur: emu("blurRad"), dist: emu("dist"), dir: ang("dir"), colour: colour() }),
                "prstShdw" => {
                    let preset = c.attribute("prst").and_then(|p| p.strip_prefix("shdw")?.parse().ok()).filter(|n| (1..=20).contains(n)).unwrap_or(1);
                    e.preset = Some(PresetShadow { preset, dist: emu("dist"), dir: ang("dir"), colour: colour() })
                }
                "glow" => e.glow = Some(Glow { radius: emu("rad"), colour: colour() }),
                "softEdge" => e.soft_edge = Some(emu("rad")),
                "blur" => e.blur = Some(emu("rad")),
                _ => e.kept = true,
            }
        }
        e
    }

    pub fn is_empty(&self) -> bool {
        self.outer.is_none()
            && self.inner.is_none()
            && self.preset.is_none()
            && self.glow.is_none()
            && self.soft_edge.is_none()
            && self.blur.is_none()
    }
}

// Style references

#[derive(Clone, Debug, Default, PartialEq)]
pub struct StyleRefs {
    pub line: Option<(u32, Option<Colour>)>,
    pub fill: Option<(u32, Option<Colour>)>,
    pub effect: Option<(u32, Option<Colour>)>,
    pub font: Option<Colour>,
}

impl StyleRefs {
    pub fn parse(n: Node) -> StyleRefs {
        let r = |k: &str| a_child(n, k).map(|e| (e.attribute("idx").and_then(|v| v.parse().ok()).unwrap_or(0), Colour::child_of(e)));
        StyleRefs { line: r("lnRef"), fill: r("fillRef"), effect: r("effectRef"), font: a_child(n, "fontRef").and_then(Colour::child_of) }
    }
}

// Resolution

#[derive(Clone, Copy)]
pub struct Palette<'a> {
    pub theme: &'a Theme,
    pub map: &'a ColourMap,
}

impl Palette<'_> {
    fn colour(&self, c: &Colour, ph: Option<Rgba>) -> Rgba {
        c.resolve(self.theme, self.map, ph)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ResolvedBlipAdjust {
    Alpha(f64),
    Grayscale,
    Lum { bright: f64, contrast: f64 },
    BiLevel(f64),
    Duotone([Rgba; 2]),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Paint {
    None,
    Solid(Rgba),
    Gradient { stops: Vec<(f64, Rgba)>, kind: GradientKind, tile: [Pct; 4], flip: TileFlip, rot_with_shape: bool },
    Pattern { preset: PatternPreset, fg: Rgba, bg: Rgba },
    Picture { image: Option<usize>, src: [Pct; 4], mode: BlipMode, adjust: Vec<ResolvedBlipAdjust>, rot_with_shape: bool },
}

impl Paint {
    pub fn is_none(&self) -> bool {
        matches!(self, Paint::None)
    }
}

pub fn paint(f: &Fill, p: Palette, ph: Option<Rgba>, group: &Paint) -> Paint {
    match f {
        Fill::None => Paint::None,
        Fill::Solid(c) => Paint::Solid(p.colour(c, ph)),
        Fill::Gradient(g) => {
            let mut stops: Vec<(f64, Rgba)> = g.stops.iter().map(|(pos, c)| (pos.fraction().clamp(0.0, 1.0), p.colour(c, ph))).collect();
            stops.sort_by(|a, b| a.0.total_cmp(&b.0));
            Paint::Gradient { stops, kind: g.kind, tile: g.tile, flip: g.flip, rot_with_shape: g.rot_with_shape }
        }
        Fill::Pattern { preset, fg, bg } => Paint::Pattern { preset: *preset, fg: p.colour(fg, ph), bg: p.colour(bg, ph) },
        Fill::Blip(b) => Paint::Picture {
            image: b.image,
            src: b.src,
            mode: b.mode.clone(),
            adjust: b
                .adjust
                .iter()
                .map(|a| match a {
                    BlipAdjust::AlphaModFix(v) => ResolvedBlipAdjust::Alpha(v.fraction().clamp(0.0, 1.0)),
                    BlipAdjust::Grayscale => ResolvedBlipAdjust::Grayscale,
                    BlipAdjust::Lum { bright, contrast } => ResolvedBlipAdjust::Lum { bright: bright.fraction(), contrast: contrast.fraction() },
                    BlipAdjust::BiLevel(t) => ResolvedBlipAdjust::BiLevel(t.fraction()),
                    BlipAdjust::Duotone([a, b]) => ResolvedBlipAdjust::Duotone([p.colour(a, ph), p.colour(b, ph)]),
                })
                .collect(),
            rot_with_shape: b.rot_with_shape,
        },
        Fill::Group => group.clone(),
    }
}

pub fn style_entry<'a, T>(list: &'a [T], bg: Option<&'a [T]>, idx: u32) -> Option<&'a T> {
    match idx {
        0 => None,
        1..=1000 => list.get(idx as usize - 1),
        _ => bg.and_then(|b| b.get(idx as usize - 1001)),
    }
}

pub fn resolve_fill(own: Option<&Fill>, style: Option<&StyleRefs>, p: Palette, group: &Paint) -> Paint {
    if let Some(f) = own {
        return paint(f, p, None, group);
    }
    let Some((idx, colour)) = style.and_then(|s| s.fill.as_ref()) else { return Paint::None };
    let ph = colour.as_ref().map(|c| p.colour(c, None));
    style_entry(&p.theme.fills, Some(&p.theme.bg_fills), *idx).map_or(Paint::None, |f| paint(f, p, ph, group))
}

#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedLine {
    pub width: f64,
    pub paint: Paint,
    pub dash: Dash,
    pub compound: Compound,
    pub cap: Cap,
    pub join: Join,
    pub inset: bool,
    pub head: Option<LineEnd>,
    pub tail: Option<LineEnd>,
}

impl ResolvedLine {
    pub fn none() -> ResolvedLine {
        ResolvedLine {
            width: 0.0,
            paint: Paint::None,
            dash: Dash::Preset(PresetDash::Solid),
            compound: Compound::Single,
            cap: Cap::Flat,
            join: Join::Round,
            inset: false,
            head: None,
            tail: None,
        }
    }

    pub fn is_visible(&self) -> bool {
        !self.paint.is_none()
    }
}

pub fn resolve_line(own: Option<&Line>, style: Option<&StyleRefs>, p: Palette, group: &Paint) -> ResolvedLine {
    let reference = style.and_then(|s| s.line.as_ref());
    let ph = reference.and_then(|(_, c)| c.as_ref()).map(|c| p.colour(c, None));
    let base = reference.and_then(|(idx, _)| style_entry(&p.theme.lines, None, *idx)).cloned().unwrap_or_default();
    let has_ref = reference.is_some_and(|(idx, _)| *idx > 0);
    let line = match own {
        Some(l) => l.over(&base),
        None if has_ref => base,
        None => return ResolvedLine::none(),
    };
    let paint = line.fill.as_ref().map_or(Paint::None, |f| paint(f, p, ph, group));
    ResolvedLine {
        width: line.width.map_or(0.75, Emu::to_pt),
        paint,
        dash: line.dash.unwrap_or(Dash::Preset(PresetDash::Solid)),
        compound: line.compound.unwrap_or_default(),
        cap: line.cap.unwrap_or(Cap::Flat),
        join: line.join.unwrap_or(Join::Round),
        inset: line.inset.unwrap_or(false),
        head: line.head,
        tail: line.tail,
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ResolvedEffects {
    pub outer: Option<ResolvedShadow>,
    pub inner: Option<ResolvedShadow>,
    pub glow: Option<(f64, Rgba)>,
    pub soft_edge: Option<f64>,
    pub blur: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedShadow {
    pub blur: f64,
    pub dist: f64,
    pub dir: f64,
    pub scale: [f64; 2],
    pub skew: [f64; 2],
    pub align: RectAlign,
    pub colour: Rgba,
    pub rot_with_shape: bool,
}

impl ResolvedEffects {
    pub fn is_empty(&self) -> bool {
        self.outer.is_none() && self.inner.is_none() && self.glow.is_none() && self.soft_edge.is_none() && self.blur.is_none()
    }
}

pub fn resolve_effects(own: Option<&EffectList>, style: Option<&StyleRefs>, p: Palette) -> ResolvedEffects {
    let (list, ph) = match own {
        Some(e) => (Some(e), None),
        None => {
            let r = style.and_then(|s| s.effect.as_ref());
            let ph = r.and_then(|(_, c)| c.as_ref()).map(|c| p.colour(c, None));
            (r.and_then(|(idx, _)| style_entry(&p.theme.effects, None, *idx)), ph)
        }
    };
    let Some(e) = list else { return ResolvedEffects::default() };
    let outer = e.outer.as_ref().map(|s| ResolvedShadow {
        blur: s.blur.to_pt(),
        dist: s.dist.to_pt(),
        dir: s.dir.degrees(),
        scale: [s.sx.fraction(), s.sy.fraction()],
        skew: [s.kx.degrees(), s.ky.degrees()],
        align: s.align,
        colour: p.colour(&s.colour, ph),
        rot_with_shape: s.rot_with_shape,
    });
    let outer = outer.or_else(|| e.preset.as_ref().map(|s| preset_shadow(s, p.colour(&s.colour, ph))));
    ResolvedEffects {
        outer,
        inner: e.inner.as_ref().map(|s| ResolvedShadow {
            blur: s.blur.to_pt(),
            dist: s.dist.to_pt(),
            dir: s.dir.degrees(),
            scale: [1.0, 1.0],
            skew: [0.0, 0.0],
            align: RectAlign::Ctr,
            colour: p.colour(&s.colour, ph),
            rot_with_shape: true,
        }),
        glow: e.glow.as_ref().map(|g| (g.radius.to_pt(), p.colour(&g.colour, ph))),
        soft_edge: e.soft_edge.map(Emu::to_pt),
        blur: e.blur.map(Emu::to_pt),
    }
}

fn preset_shadow(s: &PresetShadow, colour: Rgba) -> ResolvedShadow {
    ResolvedShadow {
        blur: 0.0,
        dist: s.dist.to_pt(),
        dir: s.dir.degrees(),
        scale: [1.0, 1.0],
        skew: [0.0, 0.0],
        align: RectAlign::B,
        colour,
        rot_with_shape: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawing::colour::SchemeSlot;

    fn node_xml(xml: &str) -> String {
        format!(r#"<r xmlns:a="{A}" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">{xml}</r>"#)
    }

    #[test]
    fn fills_parse() {
        let x = node_xml(
            r#"<a:gradFill rotWithShape="0"><a:gsLst><a:gs pos="100000"><a:srgbClr val="FFFFFF"/></a:gs><a:gs pos="0"><a:schemeClr val="accent1"/></a:gs></a:gsLst><a:lin ang="5400000" scaled="1"/></a:gradFill>"#,
        );
        let d = roxmltree::Document::parse(&x).unwrap();
        let f = Fill::child_of(d.root_element(), &mut |_| None).unwrap();
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
        let t = Theme::parse(Some(theme_xml));
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
            r#"<a:effectLst><a:outerShdw blurRad="50800" dist="38100" dir="2700000"><a:srgbClr val="000000"/></a:outerShdw><a:reflection/></a:effectLst>"#,
        );
        let d = roxmltree::Document::parse(&x).unwrap();
        let e = EffectList::parse(d.root_element().first_element_child().unwrap());
        assert!(e.kept);
        assert_eq!(e.outer.as_ref().map(|s| (s.blur, s.dist)), Some((Emu(50800), Emu(38100))));
    }
}
