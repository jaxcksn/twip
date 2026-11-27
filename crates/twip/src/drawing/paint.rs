//! Fills, lines and effects

use super::colour::{Colour, ColourBase, ColourMod, Rgba, TWO_STOP_BLEND, linear_to_srgb};
use super::theme::{ColourMap, Theme};
use super::units::{Angle, Emu, Pct};

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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GradientPath {
    Circle,
    Rect,
    Shape,
}

#[derive(Clone, Copy, Debug, PartialEq, Hash)]
pub enum GradientKind {
    Linear { angle: Angle, scaled: bool },
    Path { path: GradientPath, focus: [Pct; 4] },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TileFlip {
    #[default]
    None,
    X,
    Y,
    Xy,
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
    Lum {
        bright: Pct,
        contrast: Pct,
    },
    BiLevel(Pct),
    Duotone([Colour; 2]),
    /// Set Transparent Colour (`a:clrChange`): `from` is drawn as `to`. With `use_a` (`useA`, on by default) alpha
    /// counts in the match and the result; without it the pixel keeps its own alpha.
    ClrChange {
        from: Colour,
        to: Colour,
        use_a: bool,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct BlipFill {
    pub image: Option<usize>,
    pub src: [Pct; 4],
    pub mode: BlipMode,
    pub adjust: Vec<BlipAdjust>,
    pub rot_with_shape: bool,
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

// Lines

#[derive(Clone, Debug, PartialEq, Hash)]
pub enum Dash {
    Preset(PresetDash),
    Custom(Vec<(Pct, Pct)>),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Compound {
    #[default]
    Single,
    Double,
    ThickThin,
    ThinThick,
    Triple,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Cap {
    Round,
    Square,
    Flat,
}

#[derive(Clone, Copy, Debug, PartialEq, Hash)]
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

/// A reflection (`a:reflection`): the shape's copy, scaled and skewed about `align`, moved `dist` along `dir`, blurred and
/// faded from `start_alpha` at `start_pos` to `end_alpha` at `end_pos` along `fade_dir`.
#[derive(Clone, Debug, PartialEq)]
pub struct Reflection {
    pub blur: Emu,
    pub dist: Emu,
    pub dir: Angle,
    pub fade_dir: Angle,
    pub sx: Pct,
    pub sy: Pct,
    pub kx: Angle,
    pub ky: Angle,
    pub align: RectAlign,
    pub start_alpha: Pct,
    pub start_pos: Pct,
    pub end_alpha: Pct,
    pub end_pos: Pct,
    pub rot_with_shape: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct EffectList {
    pub outer: Option<OuterShadow>,
    pub inner: Option<InnerShadow>,
    pub preset: Option<PresetShadow>,
    pub glow: Option<Glow>,
    pub soft_edge: Option<Emu>,
    pub blur: Option<Emu>,
    pub reflection: Option<Reflection>,
    pub kept: bool,
}

impl EffectList {
    pub fn is_empty(&self) -> bool {
        self.outer.is_none()
            && self.inner.is_none()
            && self.preset.is_none()
            && self.glow.is_none()
            && self.soft_edge.is_none()
            && self.blur.is_none()
            && self.reflection.is_none()
    }
}

// Two-colour gradients

/// A two-colour gradient's stops as Word draws one: each channel runs from its lower value to its higher along the
/// two-stop blend, gamma encoded, whichever way it runs (measured), so the blend is the same for every pair.
pub fn two_colour(from: &Colour, to: &Colour) -> Vec<(Pct, Colour)> {
    let parts = |c: &Colour| {
        let v = match c.base {
            ColourBase::Srgb(v) => v,
            _ => 0,
        };
        let alpha = c.mods.iter().fold(1.0, |a: f64, m| match m {
            ColourMod::Alpha(p) => p.fraction().clamp(0.0, 1.0),
            ColourMod::AlphaOff(p) => (a + p.fraction()).clamp(0.0, 1.0),
            ColourMod::AlphaMod(p) => (a * p.fraction()).clamp(0.0, 1.0),
            _ => a,
        });
        ([16, 8, 0].map(|s| f64::from((v >> s) & 0xFF)), alpha)
    };
    let ((a, aa), (b, ba)) = (parts(from), parts(to));
    (0..=16)
        .map(|k| {
            let (up, down) = (linear_to_srgb(TWO_STOP_BLEND[k]), 1.0 - linear_to_srgb(TWO_STOP_BLEND[16 - k]));
            let ch = |i: usize| {
                let e = if b[i] >= a[i] { up } else { down };
                ((a[i] + (b[i] - a[i]) * e).round().clamp(0.0, 255.0) as u32) << (16 - 8 * i)
            };
            let c = Colour::srgb(ch(0) | ch(1) | ch(2));
            let alpha = aa + (ba - aa) * k as f64 / 16.0;
            (Pct(k as i32 * 100000 / 16), if alpha < 1.0 { c.with(ColourMod::Alpha(Pct::from_fraction(alpha.clamp(0.0, 1.0)))) } else { c })
        })
        .collect()
}

/// Whether a page gradient is two colours as Word blends them, which VML writes as its two colours and opacities.
pub fn is_two_colour(g: &Gradient) -> bool {
    matches!(g.kind, GradientKind::Linear { .. })
        && matches!((g.stops.first(), g.stops.last()), (Some(a), Some(b)) if g.stops == two_colour(&a.1, &b.1))
}

// Style references

#[derive(Clone, Debug, Default, PartialEq)]
pub struct StyleRefs {
    pub line: Option<(u32, Option<Colour>)>,
    pub fill: Option<(u32, Option<Colour>)>,
    pub effect: Option<(u32, Option<Colour>)>,
    pub font: Option<Colour>,
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
    Lum {
        bright: f64,
        contrast: f64,
    },
    BiLevel(f64),
    Duotone([Rgba; 2]),
    /// Pixels within one level of `from` in every channel (alpha too, with `use_a`) are drawn as `to`, keeping their
    /// own alpha without `use_a`.
    ClrChange {
        from: Rgba,
        to: Rgba,
        use_a: bool,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Paint {
    None,
    Solid(Rgba),
    Gradient {
        stops: Vec<(f64, Rgba)>,
        kind: GradientKind,
        tile: [Pct; 4],
        flip: TileFlip,
        rot_with_shape: bool,
    },
    Pattern {
        preset: PatternPreset,
        fg: Rgba,
        bg: Rgba,
    },
    /// A `w:shd` stripe or cross: Word's tile, a 300 dpi pixel a bit, from a pixel above and left of the shape.
    Hatch {
        stripe: crate::document::format::Stripe,
        fg: Rgba,
        bg: Rgba,
    },
    Picture {
        image: Option<usize>,
        src: [Pct; 4],
        mode: BlipMode,
        adjust: Vec<ResolvedBlipAdjust>,
        rot_with_shape: bool,
    },
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
                    BlipAdjust::ClrChange { from, to, use_a } => {
                        ResolvedBlipAdjust::ClrChange { from: p.colour(from, ph), to: p.colour(to, ph), use_a: *use_a }
                    }
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
    pub reflection: Option<ResolvedReflection>,
}

/// A reflection in points, degrees and fractions.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedReflection {
    pub blur: f64,
    pub dist: f64,
    pub dir: f64,
    pub fade_dir: f64,
    pub scale: [f64; 2],
    pub skew: [f64; 2],
    pub align: RectAlign,
    pub start: (f64, f64),
    pub end: (f64, f64),
    pub rot_with_shape: bool,
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
        self.outer.is_none()
            && self.inner.is_none()
            && self.glow.is_none()
            && self.soft_edge.is_none()
            && self.blur.is_none()
            && self.reflection.is_none()
    }
}

/// The largest blur, glow or soft-edge radius, and shadow or reflection distance, drawn, in points. Word's dialogs stop
/// at 200 pt for a shadow's distance and 150 pt for a glow's radius; a file asking for more is drawn at this, so the
/// layers an effect draws in stay bounded.
pub const MAX_EFFECT_PT: f64 = 200.0;

fn radius(e: Emu) -> f64 {
    e.to_pt().clamp(0.0, MAX_EFFECT_PT)
}

fn offset(e: Emu) -> f64 {
    e.to_pt().clamp(-MAX_EFFECT_PT, MAX_EFFECT_PT)
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
        blur: radius(s.blur),
        dist: offset(s.dist),
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
            blur: radius(s.blur),
            dist: offset(s.dist),
            dir: s.dir.degrees(),
            scale: [1.0, 1.0],
            skew: [0.0, 0.0],
            align: RectAlign::Ctr,
            colour: p.colour(&s.colour, ph),
            rot_with_shape: true,
        }),
        glow: e.glow.as_ref().map(|g| (radius(g.radius), p.colour(&g.colour, ph))),
        soft_edge: e.soft_edge.map(radius),
        blur: e.blur.map(radius),
        reflection: e.reflection.as_ref().map(|r| ResolvedReflection {
            blur: radius(r.blur),
            dist: offset(r.dist),
            dir: r.dir.degrees(),
            fade_dir: r.fade_dir.degrees(),
            scale: [r.sx.fraction(), r.sy.fraction()],
            skew: [r.kx.degrees(), r.ky.degrees()],
            align: r.align,
            start: (r.start_pos.fraction().clamp(0.0, 1.0), r.start_alpha.fraction().clamp(0.0, 1.0)),
            end: (r.end_pos.fraction().clamp(0.0, 1.0), r.end_alpha.fraction().clamp(0.0, 1.0)),
            rot_with_shape: r.rot_with_shape,
        }),
    }
}

fn preset_shadow(s: &PresetShadow, colour: Rgba) -> ResolvedShadow {
    ResolvedShadow {
        blur: 0.0,
        dist: offset(s.dist),
        dir: s.dir.degrees(),
        scale: [1.0, 1.0],
        skew: [0.0, 0.0],
        align: RectAlign::B,
        colour,
        rot_with_shape: true,
    }
}
