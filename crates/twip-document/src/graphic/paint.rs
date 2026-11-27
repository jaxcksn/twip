//! Fills, lines and effects

use super::colour::{Colour, ColourBase, ColourMod, Rgba, TWO_STOP_BLEND, linear_to_srgb};
use super::theme::{ColourMap, Theme};
use super::units::{Angle, Emu, Pct};

// Fills

/// How an area is filled.
#[derive(Clone, Debug, PartialEq)]
pub enum Fill {
    /// Not filled.
    None,
    /// One colour.
    Solid(Colour),
    /// A gradient.
    Gradient(Gradient),
    /// A preset pattern in two colours.
    Pattern {
        /// The pattern.
        preset: PatternPreset,
        /// Its foreground, the tile's set bits.
        fg: Colour,
        /// Its background.
        bg: Colour,
    },
    /// A picture.
    Blip(BlipFill),
    /// The fill of the group the shape is in.
    Group,
}

/// The shape a path gradient spreads in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GradientPath {
    /// A circle.
    Circle,
    /// A rectangle.
    Rect,
    /// The shape's own outline.
    Shape,
}

/// The way a gradient runs.
#[derive(Clone, Copy, Debug, PartialEq, Hash)]
pub enum GradientKind {
    /// Along a line at `angle`, clockwise from left to right; `scaled` turns it with the box's aspect ratio.
    Linear {
        /// Its direction.
        angle: Angle,
        /// The angle is scaled with the fill's box.
        scaled: bool,
    },
    /// Out from a focus.
    Path {
        /// The shape it spreads in.
        path: GradientPath,
        /// Its focus rectangle, as insets from the box's left, top, right and bottom.
        focus: [Pct; 4],
    },
}

/// How a tiled fill mirrors its tiles.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TileFlip {
    /// Not at all.
    #[default]
    None,
    /// Every other tile across.
    X,
    /// Every other tile down.
    Y,
    /// Every other tile across and down.
    Xy,
}

/// A gradient fill.
#[derive(Clone, Debug, PartialEq)]
pub struct Gradient {
    /// Its colours, each at its place along the gradient.
    pub stops: Vec<(Pct, Colour)>,
    /// The way it runs.
    pub kind: GradientKind,
    /// The tile it fills, as insets from the box's left, top, right and bottom, repeated over the rest.
    pub tile: [Pct; 4],
    /// How its tiles mirror.
    pub flip: TileFlip,
    /// It turns with the shape.
    pub rot_with_shape: bool,
}

/// How a picture fills its box.
#[derive(Clone, Debug, PartialEq)]
pub enum BlipMode {
    /// Stretched over the box, inset from its left, top, right and bottom by these.
    Stretch([Pct; 4]),
    /// Tiled.
    Tile {
        /// The tiles' offset across.
        tx: Emu,
        /// The tiles' offset down.
        ty: Emu,
        /// The tiles' scale across.
        sx: Pct,
        /// The tiles' scale down.
        sy: Pct,
        /// How the tiles mirror.
        flip: TileFlip,
        /// Where the tiling starts in the box.
        align: RectAlign,
    },
}

/// A change to a picture's colours.
#[derive(Clone, Debug, PartialEq)]
pub enum BlipAdjust {
    /// Its alpha scaled by this.
    AlphaModFix(Pct),
    /// Greys.
    Grayscale,
    /// Brightness and contrast.
    Lum {
        /// The brightness, from -100% to 100%.
        bright: Pct,
        /// The contrast, from -100% to 100%.
        contrast: Pct,
    },
    /// Black and white, split at this luminance.
    BiLevel(Pct),
    /// Recoloured between two colours by its luminance: dark to the first, light to the second.
    Duotone([Colour; 2]),
    /// Set Transparent Colour (`a:clrChange`): `from` is drawn as `to`. With `use_a` (`useA`, on by default) alpha
    /// counts in the match and the result; without it the pixel keeps its own alpha.
    ClrChange {
        /// The colour changed.
        from: Colour,
        /// What it is drawn as.
        to: Colour,
        /// Alpha counts.
        use_a: bool,
    },
}

/// A picture fill.
#[derive(Clone, Debug, PartialEq)]
pub struct BlipFill {
    /// The picture, an index into the document's images; `None` for one that couldn't be read.
    pub image: Option<usize>,
    /// The crop, as insets from the picture's left, top, right and bottom (negative ones pad it).
    pub src: [Pct; 4],
    /// How it fills its box.
    pub mode: BlipMode,
    /// Changes to its colours, in order.
    pub adjust: Vec<BlipAdjust>,
    /// It turns with the shape.
    pub rot_with_shape: bool,
}

macro_rules! names {
    ($(#[$meta:meta])* $ty:ident { $($v:ident = $s:literal),* $(,)? }) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum $ty { $(#[doc = concat!("`", $s, "`.")] $v),* }
        impl $ty {
            /// Every value, in the schema's order.
            pub const ALL: &'static [$ty] = &[$($ty::$v),*];
            /// The value named `s`.
            pub fn parse(s: &str) -> Option<$ty> {
                match s { $($s => Some($ty::$v),)* _ => None }
            }
            /// Its name.
            pub fn name(self) -> &'static str {
                match self { $($ty::$v => $s),* }
            }
        }
    };
}

names!(
    /// A preset pattern, by its ECMA-376 name (`ST_PresetPatternVal`).
    PatternPreset {
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

names!(
    /// A preset dash, by its ECMA-376 name (`ST_PresetLineDashVal`).
    PresetDash {
    Solid = "solid", Dot = "dot", Dash = "dash", LgDash = "lgDash", DashDot = "dashDot", LgDashDot = "lgDashDot",
    LgDashDotDot = "lgDashDotDot", SysDash = "sysDash", SysDot = "sysDot", SysDashDot = "sysDashDot", SysDashDotDot = "sysDashDotDot",
});

names!(
    /// A place in a box: a corner, an edge's middle or the centre, by its ECMA-376 name (`ST_RectAlignment`).
    RectAlign { Tl = "tl", T = "t", Tr = "tr", L = "l", Ctr = "ctr", R = "r", Bl = "bl", B = "b", Br = "br" }
);

// Lines

/// A line's dashes.
#[derive(Clone, Debug, PartialEq, Hash)]
pub enum Dash {
    /// A preset dash.
    Preset(PresetDash),
    /// Dashes and the spaces after them, in line widths.
    Custom(Vec<(Pct, Pct)>),
}

/// A line drawn as more than one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Compound {
    /// One line.
    #[default]
    Single,
    /// Two of a width.
    Double,
    /// A thick line and a thin one.
    ThickThin,
    /// A thin line and a thick one.
    ThinThick,
    /// Three: thin, thick, thin.
    Triple,
}

/// A line's ends where it has no line end.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Cap {
    /// Round.
    Round,
    /// Square, past the end by half the width.
    Square,
    /// Flat, at the end.
    Flat,
}

/// How a line's segments meet.
#[derive(Clone, Copy, Debug, PartialEq, Hash)]
pub enum Join {
    /// Rounded.
    Round,
    /// Cut off.
    Bevel,
    /// Pointed, up to this limit of the width.
    Miter(Pct),
}

/// The shape of a line end.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndKind {
    /// A filled triangle.
    Triangle,
    /// A filled arrowhead notched at its back.
    Stealth,
    /// A filled diamond.
    Diamond,
    /// A filled oval.
    Oval,
    /// An open arrowhead: two strokes.
    Arrow,
}

/// A line end's size, against the line's width.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndSize {
    /// Small.
    Small,
    /// Medium.
    Medium,
    /// Large.
    Large,
}

/// An arrowhead or other end on a line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LineEnd {
    /// Its shape.
    pub kind: EndKind,
    /// Its width.
    pub width: EndSize,
    /// Its length.
    pub length: EndSize,
}

/// A line or outline, each property unset where it takes its style's.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Line {
    /// Its width.
    pub width: Option<Emu>,
    /// Its fill.
    pub fill: Option<Fill>,
    /// Its dashes.
    pub dash: Option<Dash>,
    /// Drawn as more than one.
    pub compound: Option<Compound>,
    /// Its ends' cap.
    pub cap: Option<Cap>,
    /// How its segments meet.
    pub join: Option<Join>,
    /// Drawn inside the outline, not on it.
    pub inset: Option<bool>,
    /// Its start's end.
    pub head: Option<LineEnd>,
    /// Its end's end.
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

/// A shadow behind the shape: its copy, scaled and skewed about `align`, moved `dist` along `dir` and blurred.
#[derive(Clone, Debug, PartialEq)]
pub struct OuterShadow {
    /// Its blur radius.
    pub blur: Emu,
    /// How far it is from the shape.
    pub dist: Emu,
    /// Which way it is from the shape.
    pub dir: Angle,
    /// Its scale across.
    pub sx: Pct,
    /// Its scale down.
    pub sy: Pct,
    /// Its skew across.
    pub kx: Angle,
    /// Its skew down.
    pub ky: Angle,
    /// What it is scaled and skewed about.
    pub align: RectAlign,
    /// Its colour.
    pub colour: Colour,
    /// It turns with the shape.
    pub rot_with_shape: bool,
}

/// A shadow inside the shape, cast by its edge.
#[derive(Clone, Debug, PartialEq)]
pub struct InnerShadow {
    /// Its blur radius.
    pub blur: Emu,
    /// How far it is from the edge.
    pub dist: Emu,
    /// Which way it is from the edge.
    pub dir: Angle,
    /// Its colour.
    pub colour: Colour,
}

/// A preset shadow, drawn as an outer one.
#[derive(Clone, Debug, PartialEq)]
pub struct PresetShadow {
    /// Which preset, 1 to 20.
    pub preset: u8,
    /// How far it is from the shape.
    pub dist: Emu,
    /// Which way it is from the shape.
    pub dir: Angle,
    /// Its colour.
    pub colour: Colour,
}

/// A soft coloured glow around the shape.
#[derive(Clone, Debug, PartialEq)]
pub struct Glow {
    /// How far it reaches.
    pub radius: Emu,
    /// Its colour.
    pub colour: Colour,
}

/// A reflection (`a:reflection`): the shape's copy, scaled and skewed about `align`, moved `dist` along `dir`, blurred and
/// faded from `start_alpha` at `start_pos` to `end_alpha` at `end_pos` along `fade_dir`.
#[derive(Clone, Debug, PartialEq)]
pub struct Reflection {
    /// Its blur radius.
    pub blur: Emu,
    /// How far it is from the shape.
    pub dist: Emu,
    /// Which way it is from the shape.
    pub dir: Angle,
    /// The way it fades.
    pub fade_dir: Angle,
    /// Its scale across.
    pub sx: Pct,
    /// Its scale down.
    pub sy: Pct,
    /// Its skew across.
    pub kx: Angle,
    /// Its skew down.
    pub ky: Angle,
    /// What it is scaled and skewed about.
    pub align: RectAlign,
    /// Its alpha where the fade starts.
    pub start_alpha: Pct,
    /// Where the fade starts.
    pub start_pos: Pct,
    /// Its alpha where the fade ends.
    pub end_alpha: Pct,
    /// Where the fade ends.
    pub end_pos: Pct,
    /// It turns with the shape.
    pub rot_with_shape: bool,
}

/// A shape's effects.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EffectList {
    /// Its shadow.
    pub outer: Option<OuterShadow>,
    /// Its inner shadow.
    pub inner: Option<InnerShadow>,
    /// Its preset shadow, drawn when it has no shadow of its own.
    pub preset: Option<PresetShadow>,
    /// Its glow.
    pub glow: Option<Glow>,
    /// How far in its edges soften.
    pub soft_edge: Option<Emu>,
    /// Its blur radius.
    pub blur: Option<Emu>,
    /// Its reflection.
    pub reflection: Option<Reflection>,
    /// It has effects kept but not drawn.
    pub kept: bool,
}

impl EffectList {
    /// It draws nothing.
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

/// The theme's line, fill and effect a shape's style takes, each by its index with the colour its placeholder takes,
/// and its text's colour. An index of 0 is none.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StyleRefs {
    /// Its line.
    pub line: Option<(u32, Option<Colour>)>,
    /// Its fill: 1 to 1000 a theme fill, from 1001 a background fill.
    pub fill: Option<(u32, Option<Colour>)>,
    /// Its effects.
    pub effect: Option<(u32, Option<Colour>)>,
    /// Its text's colour.
    pub font: Option<Colour>,
}

// Resolution

/// What colours resolve against: a theme and the colour map.
#[derive(Clone, Copy)]
pub struct Palette<'a> {
    /// The theme.
    pub theme: &'a Theme,
    /// The colour map.
    pub map: &'a ColourMap,
}

impl Palette<'_> {
    fn colour(&self, c: &Colour, ph: Option<Rgba>) -> Rgba {
        c.resolve(self.theme, self.map, ph)
    }
}

/// A change to a picture's colours, its colours resolved and its amounts fractions.
#[derive(Clone, Debug, PartialEq)]
pub enum ResolvedBlipAdjust {
    /// Its alpha scaled by this, 0 to 1.
    Alpha(f64),
    /// Greys.
    Grayscale,
    /// Brightness and contrast.
    Lum {
        /// The brightness, -1 to 1.
        bright: f64,
        /// The contrast, -1 to 1.
        contrast: f64,
    },
    /// Black and white, split at this luminance.
    BiLevel(f64),
    /// Recoloured between two colours by its luminance.
    Duotone([Rgba; 2]),
    /// Pixels within one level of `from` in every channel (alpha too, with `use_a`) are drawn as `to`, keeping their
    /// own alpha without `use_a`.
    ClrChange {
        /// The colour changed.
        from: Rgba,
        /// What it is drawn as.
        to: Rgba,
        /// Alpha counts.
        use_a: bool,
    },
}

/// A fill, its colours resolved: what is drawn.
#[derive(Clone, Debug, PartialEq)]
pub enum Paint {
    /// Nothing.
    None,
    /// One colour.
    Solid(Rgba),
    /// A gradient.
    Gradient {
        /// Its colours at their places along it, 0 to 1, in order.
        stops: Vec<(f64, Rgba)>,
        /// The way it runs.
        kind: GradientKind,
        /// The tile it fills, as insets from the box's left, top, right and bottom.
        tile: [Pct; 4],
        /// How its tiles mirror.
        flip: TileFlip,
        /// It turns with the shape.
        rot_with_shape: bool,
    },
    /// A preset pattern.
    Pattern {
        /// The pattern.
        preset: PatternPreset,
        /// Its foreground.
        fg: Rgba,
        /// Its background.
        bg: Rgba,
    },
    /// A shading stripe or cross: Word's tile, a 300 dpi pixel a bit, from a pixel above and left of the shape.
    Hatch {
        /// The stripe or cross.
        stripe: crate::format::Stripe,
        /// Its foreground.
        fg: Rgba,
        /// Its background.
        bg: Rgba,
    },
    /// A picture.
    Picture {
        /// The picture, an index into the document's images.
        image: Option<usize>,
        /// The crop, as insets from the picture's left, top, right and bottom.
        src: [Pct; 4],
        /// How it fills its box.
        mode: BlipMode,
        /// Changes to its colours, in order.
        adjust: Vec<ResolvedBlipAdjust>,
        /// It turns with the shape.
        rot_with_shape: bool,
    },
}

impl Paint {
    /// It draws nothing.
    pub fn is_none(&self) -> bool {
        matches!(self, Paint::None)
    }
}

/// `f` resolved in `p`, its placeholder colour `ph`, a group fill taking `group`.
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

/// A theme style list's entry `idx`: 1 to 1000 in `list`, from 1001 in `bg`; 0 is none.
pub fn style_entry<'a, T>(list: &'a [T], bg: Option<&'a [T]>, idx: u32) -> Option<&'a T> {
    match idx {
        0 => None,
        1..=1000 => list.get(idx as usize - 1),
        _ => bg.and_then(|b| b.get(idx as usize - 1001)),
    }
}

/// What a shape's fill draws: its own, else its style's.
pub fn resolve_fill(own: Option<&Fill>, style: Option<&StyleRefs>, p: Palette, group: &Paint) -> Paint {
    if let Some(f) = own {
        return paint(f, p, None, group);
    }
    let Some((idx, colour)) = style.and_then(|s| s.fill.as_ref()) else { return Paint::None };
    let ph = colour.as_ref().map(|c| p.colour(c, None));
    style_entry(&p.theme.fills, Some(&p.theme.bg_fills), *idx).map_or(Paint::None, |f| paint(f, p, ph, group))
}

/// A line as it is drawn: its own properties over its style's, its colours resolved.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedLine {
    /// Its width, points.
    pub width: f64,
    /// Its paint.
    pub paint: Paint,
    /// Its dashes.
    pub dash: Dash,
    /// Drawn as more than one.
    pub compound: Compound,
    /// Its ends' cap.
    pub cap: Cap,
    /// How its segments meet.
    pub join: Join,
    /// Drawn inside the outline, not on it.
    pub inset: bool,
    /// Its start's end.
    pub head: Option<LineEnd>,
    /// Its end's end.
    pub tail: Option<LineEnd>,
}

impl ResolvedLine {
    /// No line.
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

    /// It draws something.
    pub fn is_visible(&self) -> bool {
        !self.paint.is_none()
    }
}

/// What a shape's line draws: its own properties over its style's.
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

/// A shape's effects as they are drawn, in points, degrees and fractions, their sizes bounded by [`MAX_EFFECT_PT`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ResolvedEffects {
    /// Its shadow, or its preset shadow.
    pub outer: Option<ResolvedShadow>,
    /// Its inner shadow.
    pub inner: Option<ResolvedShadow>,
    /// Its glow: its radius and colour.
    pub glow: Option<(f64, Rgba)>,
    /// How far in its edges soften.
    pub soft_edge: Option<f64>,
    /// Its blur radius.
    pub blur: Option<f64>,
    /// Its reflection.
    pub reflection: Option<ResolvedReflection>,
}

/// A reflection in points, degrees and fractions.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedReflection {
    /// Its blur radius.
    pub blur: f64,
    /// How far it is from the shape.
    pub dist: f64,
    /// Which way it is from the shape.
    pub dir: f64,
    /// The way it fades.
    pub fade_dir: f64,
    /// Its scale across and down.
    pub scale: [f64; 2],
    /// Its skew across and down.
    pub skew: [f64; 2],
    /// What it is scaled and skewed about.
    pub align: RectAlign,
    /// Where the fade starts and its alpha there, 0 to 1.
    pub start: (f64, f64),
    /// Where the fade ends and its alpha there, 0 to 1.
    pub end: (f64, f64),
    /// It turns with the shape.
    pub rot_with_shape: bool,
}

/// A shadow in points, degrees and fractions.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedShadow {
    /// Its blur radius.
    pub blur: f64,
    /// How far it is from the shape.
    pub dist: f64,
    /// Which way it is from the shape.
    pub dir: f64,
    /// Its scale across and down.
    pub scale: [f64; 2],
    /// Its skew across and down.
    pub skew: [f64; 2],
    /// What it is scaled and skewed about.
    pub align: RectAlign,
    /// Its colour.
    pub colour: Rgba,
    /// It turns with the shape.
    pub rot_with_shape: bool,
}

impl ResolvedEffects {
    /// They draw nothing.
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

/// What a shape's effects draw: its own, else its style's.
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
