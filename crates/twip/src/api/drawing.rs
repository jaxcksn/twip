//! Shape values: preset geometries, colours, fills, outlines, effects and text bodies, as applications set and read
//! them. Colours stay as written (a theme colour and its transforms), never only the resolved RGB.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::value::{Bytes, Color, Points};
use crate::drawing::colour::{Colour, ColourBase, ColourMod, SchemeSlot};
use crate::drawing::geom::Preset;
use crate::drawing::paint::{self, Fill};
use crate::drawing::units::{Angle, Emu, Pct};

/// Degrees, clockwise.
pub type Degrees = f64;

/// A preset geometry by its ECMA-376 name (`"wedgeRectCallout"`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ShapePreset(pub(crate) Preset);

impl ShapePreset {
    /// The ECMA-376 name.
    pub fn name(self) -> &'static str {
        self.0.name()
    }

    /// The preset an ECMA-376 name names.
    pub fn from_name(s: &str) -> Option<ShapePreset> {
        Preset::from_name(s).map(ShapePreset)
    }

    /// Every preset, in the standard's order.
    pub fn all() -> impl Iterator<Item = ShapePreset> {
        Preset::ALL.iter().map(|p| ShapePreset(*p))
    }

    /// Whether it is a line or connector (it has no filled path): inserted 72 x 0 pt with the line template.
    pub fn is_line(self) -> bool {
        let n = self.name();
        n == "line" || n.ends_with("Connector1") || n.contains("Connector")
    }
}

impl Serialize for ShapePreset {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.name())
    }
}

impl<'de> Deserialize<'de> for ShapePreset {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<ShapePreset, D::Error> {
        let s = String::deserialize(d)?;
        ShapePreset::from_name(&s).ok_or_else(|| serde::de::Error::custom(format!("unknown preset geometry {s:?}")))
    }
}

/// A theme colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ThemeColour {
    /// Dark 1 (through the document's colour mapping).
    Text1,
    /// Light 1.
    Background1,
    /// Dark 2.
    Text2,
    /// Light 2.
    Background2,
    /// Accent 1.
    Accent1,
    /// Accent 2.
    Accent2,
    /// Accent 3.
    Accent3,
    /// Accent 4.
    Accent4,
    /// Accent 5.
    Accent5,
    /// Accent 6.
    Accent6,
    /// The hyperlink colour.
    Hyperlink,
    /// The followed hyperlink colour.
    FollowedHyperlink,
}

impl ThemeColour {
    /// Every theme colour, in the theme's order.
    pub const ALL: [ThemeColour; 12] = [
        ThemeColour::Text1,
        ThemeColour::Background1,
        ThemeColour::Text2,
        ThemeColour::Background2,
        ThemeColour::Accent1,
        ThemeColour::Accent2,
        ThemeColour::Accent3,
        ThemeColour::Accent4,
        ThemeColour::Accent5,
        ThemeColour::Accent6,
        ThemeColour::Hyperlink,
        ThemeColour::FollowedHyperlink,
    ];

    pub(crate) fn slot(self) -> SchemeSlot {
        match self {
            ThemeColour::Text1 => SchemeSlot::Tx1,
            ThemeColour::Background1 => SchemeSlot::Bg1,
            ThemeColour::Text2 => SchemeSlot::Tx2,
            ThemeColour::Background2 => SchemeSlot::Bg2,
            ThemeColour::Accent1 => SchemeSlot::Accent(1),
            ThemeColour::Accent2 => SchemeSlot::Accent(2),
            ThemeColour::Accent3 => SchemeSlot::Accent(3),
            ThemeColour::Accent4 => SchemeSlot::Accent(4),
            ThemeColour::Accent5 => SchemeSlot::Accent(5),
            ThemeColour::Accent6 => SchemeSlot::Accent(6),
            ThemeColour::Hyperlink => SchemeSlot::Hlink,
            ThemeColour::FollowedHyperlink => SchemeSlot::FolHlink,
        }
    }

    fn of(s: SchemeSlot) -> Option<ThemeColour> {
        Some(match s {
            SchemeSlot::Tx1 | SchemeSlot::Dk1 => ThemeColour::Text1,
            SchemeSlot::Bg1 | SchemeSlot::Lt1 => ThemeColour::Background1,
            SchemeSlot::Tx2 | SchemeSlot::Dk2 => ThemeColour::Text2,
            SchemeSlot::Bg2 | SchemeSlot::Lt2 => ThemeColour::Background2,
            SchemeSlot::Accent(n) => ThemeColour::ALL[(3 + n.clamp(1, 6)) as usize],
            SchemeSlot::Hlink => ThemeColour::Hyperlink,
            SchemeSlot::FolHlink => ThemeColour::FollowedHyperlink,
            SchemeSlot::Placeholder => return None,
        })
    }
}

/// What a colour transform does.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TransformKind {
    /// Towards white by `1 - value`.
    Tint,
    /// Towards black by `1 - value`.
    Shade,
    /// Luminance times `value`.
    LumMod,
    /// Luminance plus `value`.
    LumOff,
    /// Saturation times `value`.
    SatMod,
    /// Saturation plus `value`.
    SatOff,
    /// Hue times `value`.
    HueMod,
    /// Hue plus `value` degrees.
    HueOff,
    /// Opacity `value`.
    Alpha,
    /// Opacity times `value`.
    AlphaMod,
}

/// One colour transform; `value` is a fraction (0.75), degrees for `hueOff`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ColourTransform {
    /// What it does.
    pub kind: TransformKind,
    /// How much.
    pub value: f64,
}

/// A colour: exactly one base (a theme colour, an RGB value or a preset colour name), then its transforms in order.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ColourValue {
    /// A theme colour.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<ThemeColour>,
    /// An RGB colour.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rgb: Option<Color>,
    /// A preset colour by its CSS name (`"cornflowerBlue"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
    /// The transforms, applied in order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub transforms: Vec<ColourTransform>,
}

impl ColourValue {
    /// An RGB colour.
    pub fn rgb(c: Color) -> ColourValue {
        ColourValue { rgb: Some(c), ..ColourValue::default() }
    }

    /// A theme colour.
    pub fn theme(t: ThemeColour) -> ColourValue {
        ColourValue { theme: Some(t), ..ColourValue::default() }
    }

    pub(crate) fn to_model(&self) -> Result<Colour, String> {
        let base = match (self.theme, self.rgb, &self.preset) {
            (Some(t), None, None) => ColourBase::Scheme(t.slot()),
            (None, Some(c), None) => ColourBase::Srgb(c.0 & 0xFF_FFFF),
            (None, None, Some(name)) => {
                let value = crate::drawing::colour::preset_colour(name).ok_or_else(|| format!("unknown preset colour {name:?}"))?;
                ColourBase::Preset { name: name.clone(), value }
            }
            _ => return Err("a colour has exactly one of theme, rgb and preset".into()),
        };
        let pct = |v: f64| Pct((v * 100000.0).round() as i32);
        let mods = self
            .transforms
            .iter()
            .map(|t| match t.kind {
                TransformKind::Tint => ColourMod::Tint(pct(t.value)),
                TransformKind::Shade => ColourMod::Shade(pct(t.value)),
                TransformKind::LumMod => ColourMod::LumMod(pct(t.value)),
                TransformKind::LumOff => ColourMod::LumOff(pct(t.value)),
                TransformKind::SatMod => ColourMod::SatMod(pct(t.value)),
                TransformKind::SatOff => ColourMod::SatOff(pct(t.value)),
                TransformKind::HueMod => ColourMod::HueMod(pct(t.value)),
                TransformKind::HueOff => ColourMod::HueOff(Angle((t.value * 60000.0).round() as i32)),
                TransformKind::Alpha => ColourMod::Alpha(pct(t.value)),
                TransformKind::AlphaMod => ColourMod::AlphaMod(pct(t.value)),
            })
            .collect();
        Ok(Colour { base, mods })
    }

    /// The colour as written; `placeholder` stands in for `phClr` (a style reference's colour). Transforms the API has
    /// no kind for are left out.
    pub(crate) fn of(c: &Colour, placeholder: Option<&Colour>) -> ColourValue {
        let rgb_of = |base: &ColourBase| {
            let plain = Colour { base: base.clone(), mods: Vec::new() };
            Color(plain.resolve(&crate::drawing::theme::Theme::default(), &crate::drawing::theme::ColourMap::default(), None).to_rgb())
        };
        let mut out = match &c.base {
            ColourBase::Scheme(SchemeSlot::Placeholder) => match placeholder {
                Some(p) => ColourValue::of(p, None),
                None => ColourValue::default(),
            },
            ColourBase::Scheme(s) => ColourValue { theme: ThemeColour::of(*s), ..ColourValue::default() },
            ColourBase::Preset { name, .. } => ColourValue { preset: Some(name.clone()), ..ColourValue::default() },
            ColourBase::Sys { last, .. } => ColourValue::rgb(Color(*last)),
            ColourBase::Srgb(v) => ColourValue::rgb(Color(*v)),
            b => ColourValue::rgb(rgb_of(b)),
        };
        let f = |p: Pct| f64::from(p.0) / 100000.0;
        out.transforms.extend(c.mods.iter().filter_map(|m| {
            let (kind, value) = match *m {
                ColourMod::Tint(p) => (TransformKind::Tint, f(p)),
                ColourMod::Shade(p) => (TransformKind::Shade, f(p)),
                ColourMod::LumMod(p) => (TransformKind::LumMod, f(p)),
                ColourMod::LumOff(p) => (TransformKind::LumOff, f(p)),
                ColourMod::SatMod(p) => (TransformKind::SatMod, f(p)),
                ColourMod::SatOff(p) => (TransformKind::SatOff, f(p)),
                ColourMod::HueMod(p) => (TransformKind::HueMod, f(p)),
                ColourMod::HueOff(a) => (TransformKind::HueOff, a.degrees()),
                ColourMod::Alpha(p) => (TransformKind::Alpha, f(p)),
                ColourMod::AlphaMod(p) => (TransformKind::AlphaMod, f(p)),
                _ => return None,
            };
            Some(ColourTransform { kind, value })
        }));
        out
    }
}

/// A gradient's shape.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GradientShape {
    /// Along a direction (`angle`).
    #[default]
    Linear,
    /// From the centre outward in circles.
    Radial,
    /// From the centre outward in rectangles.
    Rectangular,
    /// Following the shape's outline.
    Path,
}

/// A gradient stop.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GradientStop {
    /// 0 to 1 along the gradient.
    pub position: f64,
    /// The colour there.
    pub colour: ColourValue,
}

/// A fill.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum FillValue {
    /// No fill.
    None,
    /// One colour.
    Solid {
        /// The colour.
        colour: ColourValue,
    },
    /// A gradient.
    Gradient {
        /// Its shape.
        kind: GradientShape,
        /// A linear gradient's direction (0: left to right).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        angle: Option<Degrees>,
        /// Two or more stops.
        stops: Vec<GradientStop>,
    },
    /// A two-colour pattern.
    Pattern {
        /// The ECMA-376 pattern name (`"pct50"`, `"dkDnDiag"`).
        preset: String,
        /// The pattern's colour.
        foreground: ColourValue,
        /// The colour behind it.
        background: ColourValue,
    },
    /// A picture.
    Picture {
        /// The image (PNG, JPEG, GIF, BMP); empty in the state (the selection reports the fill, not its bytes).
        image: Bytes,
        /// Tiled at its own size instead of stretched.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tile: Option<bool>,
    },
    /// The style reference's fill (no explicit fill).
    FromStyle,
}

impl FillValue {
    /// The fill in the model (`None` for `fromStyle`); `image` adds a picture's bytes to the document, giving its
    /// index.
    pub(crate) fn to_model(&self, image: &mut dyn FnMut(&[u8]) -> Option<usize>) -> Result<Option<Fill>, String> {
        let pct = |v: f64| Pct((v * 100000.0).round() as i32);
        Ok(Some(match self {
            FillValue::None => Fill::None,
            FillValue::Solid { colour } => Fill::Solid(colour.to_model()?),
            FillValue::Gradient { kind, angle, stops } => {
                if stops.len() < 2 {
                    return Err("a gradient has two or more stops".into());
                }
                let stops = stops.iter().map(|s| Ok((pct(s.position.clamp(0.0, 1.0)), s.colour.to_model()?))).collect::<Result<_, String>>()?;
                let half = [Pct(50000); 4];
                let kind = match kind {
                    GradientShape::Linear => {
                        paint::GradientKind::Linear { angle: Angle((angle.unwrap_or(0.0).rem_euclid(360.0) * 60000.0).round() as i32), scaled: false }
                    }
                    GradientShape::Radial => paint::GradientKind::Path { path: paint::GradientPath::Circle, focus: half },
                    GradientShape::Rectangular => paint::GradientKind::Path { path: paint::GradientPath::Rect, focus: half },
                    GradientShape::Path => paint::GradientKind::Path { path: paint::GradientPath::Shape, focus: half },
                };
                Fill::Gradient(paint::Gradient { stops, kind, tile: [Pct(0); 4], flip: paint::TileFlip::None, rot_with_shape: true })
            }
            FillValue::Pattern { preset, foreground, background } => Fill::Pattern {
                preset: paint::PatternPreset::parse(preset).ok_or_else(|| format!("unknown pattern {preset:?}"))?,
                fg: foreground.to_model()?,
                bg: background.to_model()?,
            },
            FillValue::Picture { image: bytes, tile } => {
                let index = image(&bytes.0).ok_or("the picture isn't an image the engine reads")?;
                let mode = if tile.unwrap_or(false) {
                    paint::BlipMode::Tile {
                        tx: Emu(0),
                        ty: Emu(0),
                        sx: Pct(100000),
                        sy: Pct(100000),
                        flip: paint::TileFlip::None,
                        align: paint::RectAlign::Tl,
                    }
                } else {
                    paint::BlipMode::Stretch([Pct(0); 4])
                };
                Fill::Blip(paint::BlipFill { image: Some(index), src: [Pct(0); 4], mode, adjust: Vec::new(), rot_with_shape: true })
            }
            FillValue::FromStyle => return Ok(None),
        }))
    }

    /// A model fill as written; `placeholder` stands in for `phClr`; `images` are the document's.
    pub(crate) fn of(f: &Fill, placeholder: Option<&Colour>, images: &[Vec<u8>]) -> FillValue {
        let c = |c: &Colour| ColourValue::of(c, placeholder);
        match f {
            Fill::None => FillValue::None,
            Fill::Solid(x) => FillValue::Solid { colour: c(x) },
            Fill::Gradient(g) => {
                let (kind, angle) = match g.kind {
                    paint::GradientKind::Linear { angle, .. } => (GradientShape::Linear, Some(angle.degrees())),
                    paint::GradientKind::Path { path: paint::GradientPath::Circle, .. } => (GradientShape::Radial, None),
                    paint::GradientKind::Path { path: paint::GradientPath::Rect, .. } => (GradientShape::Rectangular, None),
                    paint::GradientKind::Path { path: paint::GradientPath::Shape, .. } => (GradientShape::Path, None),
                };
                let stops = g.stops.iter().map(|(p, x)| GradientStop { position: f64::from(p.0) / 100000.0, colour: c(x) }).collect();
                FillValue::Gradient { kind, angle, stops }
            }
            Fill::Pattern { preset, fg, bg } => FillValue::Pattern { preset: preset.name().into(), foreground: c(fg), background: c(bg) },
            Fill::Blip(b) => FillValue::Picture {
                image: Bytes(b.image.and_then(|i| images.get(i)).cloned().unwrap_or_default()),
                tile: Some(matches!(b.mode, paint::BlipMode::Tile { .. })),
            },
            // The group's fill: reported as the style's (the shape has none of its own).
            Fill::Group => FillValue::FromStyle,
        }
    }
}

/// How a line is drawn along its length.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Compound {
    /// One line.
    #[default]
    Single,
    /// Two equal lines.
    Double,
    /// A thick line then a thin one.
    ThickThin,
    /// A thin line then a thick one.
    ThinThick,
    /// Three lines.
    Triple,
}

/// A line's ends.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LineCap {
    /// Square, at the end.
    #[default]
    Flat,
    /// Rounded.
    Round,
    /// Square, half the width past the end.
    Square,
}

/// How a line's segments join.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LineJoin {
    /// Rounded.
    #[default]
    Round,
    /// Cut off.
    Bevel,
    /// Pointed.
    Miter,
}

/// An arrowhead's shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ArrowKind {
    /// A filled triangle.
    Triangle,
    /// A notched triangle.
    Stealth,
    /// A diamond.
    Diamond,
    /// An oval.
    Oval,
    /// An open V.
    Arrow,
}

/// An arrowhead's width or length.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ArrowSize {
    /// Small.
    Small,
    /// Medium.
    #[default]
    Medium,
    /// Large.
    Large,
}

/// An arrowhead.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Arrowhead {
    /// Its shape.
    pub kind: ArrowKind,
    /// Its width.
    #[serde(default)]
    pub width: ArrowSize,
    /// Its length.
    #[serde(default)]
    pub length: ArrowSize,
}

/// A line's arrowheads.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LineEnds {
    /// At the start.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head: Option<Arrowhead>,
    /// At the end.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tail: Option<Arrowhead>,
}

/// An outline.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LineValue {
    /// The width (0.75 pt when the document gives none).
    pub width: Points,
    /// `none`, `solid` or `gradient`.
    pub fill: FillValue,
    /// The ECMA-376 dash name (`"solid"`, `"dash"`, `"sysDot"`…); `"custom"` for a custom dash.
    pub dash: String,
    /// Single or several lines.
    pub compound: Compound,
    /// The ends.
    pub cap: LineCap,
    /// The joins.
    pub join: LineJoin,
    /// The arrowheads.
    pub ends: LineEnds,
}

/// `setShapeLine`'s change; what isn't given is kept.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LineChange {
    /// The width (0…1584 pt).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<Points>,
    /// `none`, `solid` or `gradient`; `fromStyle` removes the whole explicit outline.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill: Option<FillValue>,
    /// The ECMA-376 dash name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dash: Option<String>,
    /// Single or several lines.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compound: Option<Compound>,
    /// The ends.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cap: Option<LineCap>,
    /// The joins.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub join: Option<LineJoin>,
    /// The arrowheads: both ends are replaced (an end not given is removed).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ends: Option<LineEnds>,
}

impl LineValue {
    /// A model line as written.
    pub(crate) fn of(l: &paint::Line, placeholder: Option<&Colour>, images: &[Vec<u8>]) -> LineValue {
        let end = |e: &Option<paint::LineEnd>| {
            e.map(|e| Arrowhead {
                kind: match e.kind {
                    paint::EndKind::Triangle => ArrowKind::Triangle,
                    paint::EndKind::Stealth => ArrowKind::Stealth,
                    paint::EndKind::Diamond => ArrowKind::Diamond,
                    paint::EndKind::Oval => ArrowKind::Oval,
                    paint::EndKind::Arrow => ArrowKind::Arrow,
                },
                width: size_of(e.width),
                length: size_of(e.length),
            })
        };
        LineValue {
            width: Points(l.width.map_or(0.75, |w| w.0 as f64 / 12700.0)),
            fill: l.fill.as_ref().map_or(FillValue::FromStyle, |f| FillValue::of(f, placeholder, images)),
            dash: match &l.dash {
                Some(paint::Dash::Preset(p)) => p.name().into(),
                Some(paint::Dash::Custom(_)) => "custom".into(),
                None => "solid".into(),
            },
            compound: match l.compound.unwrap_or_default() {
                paint::Compound::Single => Compound::Single,
                paint::Compound::Double => Compound::Double,
                paint::Compound::ThickThin => Compound::ThickThin,
                paint::Compound::ThinThick => Compound::ThinThick,
                paint::Compound::Triple => Compound::Triple,
            },
            cap: match l.cap {
                Some(paint::Cap::Round) => LineCap::Round,
                Some(paint::Cap::Square) => LineCap::Square,
                _ => LineCap::Flat,
            },
            join: match l.join {
                Some(paint::Join::Bevel) => LineJoin::Bevel,
                Some(paint::Join::Miter(_)) => LineJoin::Miter,
                _ => LineJoin::Round,
            },
            ends: LineEnds { head: end(&l.head), tail: end(&l.tail) },
        }
    }
}

fn size_of(s: paint::EndSize) -> ArrowSize {
    match s {
        paint::EndSize::Small => ArrowSize::Small,
        paint::EndSize::Medium => ArrowSize::Medium,
        paint::EndSize::Large => ArrowSize::Large,
    }
}

fn size_to(s: ArrowSize) -> paint::EndSize {
    match s {
        ArrowSize::Small => paint::EndSize::Small,
        ArrowSize::Medium => paint::EndSize::Medium,
        ArrowSize::Large => paint::EndSize::Large,
    }
}

impl LineChange {
    /// `line` with the change applied (`None`: the style's line again). Errors name a bad value.
    pub(crate) fn apply(&self, line: Option<&paint::Line>, image: &mut dyn FnMut(&[u8]) -> Option<usize>) -> Result<Option<paint::Line>, String> {
        if self.fill == Some(FillValue::FromStyle) {
            return Ok(None);
        }
        let mut l = line.cloned().unwrap_or_default();
        if let Some(w) = self.width {
            if !(0.0..=1584.0).contains(&w.0) {
                return Err("the width is 0 to 1584 pt".into());
            }
            l.width = Some(Emu((w.0 * 12700.0).round() as i64));
        }
        if let Some(f) = &self.fill {
            if matches!(f, FillValue::Pattern { .. } | FillValue::Picture { .. }) {
                return Err("an outline's fill is none, solid or a gradient".into());
            }
            l.fill = f.to_model(image)?;
        }
        if let Some(d) = &self.dash {
            l.dash = Some(paint::Dash::Preset(paint::PresetDash::parse(d).ok_or_else(|| format!("unknown dash {d:?}"))?));
        }
        if let Some(c) = self.compound {
            l.compound = Some(match c {
                Compound::Single => paint::Compound::Single,
                Compound::Double => paint::Compound::Double,
                Compound::ThickThin => paint::Compound::ThickThin,
                Compound::ThinThick => paint::Compound::ThinThick,
                Compound::Triple => paint::Compound::Triple,
            });
        }
        if let Some(c) = self.cap {
            l.cap = Some(match c {
                LineCap::Flat => paint::Cap::Flat,
                LineCap::Round => paint::Cap::Round,
                LineCap::Square => paint::Cap::Square,
            });
        }
        if let Some(j) = self.join {
            l.join = Some(match j {
                LineJoin::Round => paint::Join::Round,
                LineJoin::Bevel => paint::Join::Bevel,
                LineJoin::Miter => paint::Join::Miter(Pct(800000)),
            });
        }
        if let Some(e) = self.ends {
            let to = |a: Option<Arrowhead>| {
                a.map(|a| paint::LineEnd {
                    kind: match a.kind {
                        ArrowKind::Triangle => paint::EndKind::Triangle,
                        ArrowKind::Stealth => paint::EndKind::Stealth,
                        ArrowKind::Diamond => paint::EndKind::Diamond,
                        ArrowKind::Oval => paint::EndKind::Oval,
                        ArrowKind::Arrow => paint::EndKind::Arrow,
                    },
                    width: size_to(a.width),
                    length: size_to(a.length),
                })
            };
            (l.head, l.tail) = (to(e.head), to(e.tail));
        }
        Ok(Some(l))
    }
}

/// A shadow.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Shadow {
    /// Its colour (with its transparency as an `alpha` transform).
    pub colour: ColourValue,
    /// How far it blurs.
    pub blur: Points,
    /// How far from the shape.
    pub distance: Points,
    /// Which way, clockwise from the right.
    pub direction: Degrees,
}

/// A glow.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Glow {
    /// Its colour.
    pub colour: ColourValue,
    /// How far it reaches.
    pub radius: Points,
}

/// A shape's effects.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EffectsValue {
    /// A shadow outside the shape.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outer_shadow: Option<Shadow>,
    /// A shadow inside the shape.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inner_shadow: Option<Shadow>,
    /// A glow around the shape.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub glow: Option<Glow>,
    /// Edges faded over this distance.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub soft_edge: Option<Points>,
    /// The whole shape blurred by this radius.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blur: Option<Points>,
}

/// `setShapeEffects`'s change: each effect given replaces that effect, `null` removes it, what isn't given is kept.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EffectsChange {
    /// A shadow outside the shape.
    #[serde(default, skip_serializing_if = "Option::is_none", with = "crate::api::wire::nullable")]
    pub outer_shadow: Option<Option<Shadow>>,
    /// A shadow inside the shape.
    #[serde(default, skip_serializing_if = "Option::is_none", with = "crate::api::wire::nullable")]
    pub inner_shadow: Option<Option<Shadow>>,
    /// A glow around the shape.
    #[serde(default, skip_serializing_if = "Option::is_none", with = "crate::api::wire::nullable")]
    pub glow: Option<Option<Glow>>,
    /// Edges faded over this distance.
    #[serde(default, skip_serializing_if = "Option::is_none", with = "crate::api::wire::nullable")]
    pub soft_edge: Option<Option<Points>>,
    /// The whole shape blurred by this radius.
    #[serde(default, skip_serializing_if = "Option::is_none", with = "crate::api::wire::nullable")]
    pub blur: Option<Option<Points>>,
}

fn emu(p: Points) -> Emu {
    Emu((p.0 * 12700.0).round() as i64)
}

fn points(e: Emu) -> Points {
    Points(e.0 as f64 / 12700.0)
}

fn angle(d: Degrees) -> Angle {
    Angle((d.rem_euclid(360.0) * 60000.0).round() as i32)
}

impl EffectsValue {
    /// A model effect list as written.
    pub(crate) fn of(e: &paint::EffectList, placeholder: Option<&Colour>) -> EffectsValue {
        let c = |x: &Colour| ColourValue::of(x, placeholder);
        EffectsValue {
            outer_shadow: e.outer.as_ref().map(|s| Shadow {
                colour: c(&s.colour),
                blur: points(s.blur),
                distance: points(s.dist),
                direction: s.dir.degrees(),
            }),
            inner_shadow: e.inner.as_ref().map(|s| Shadow {
                colour: c(&s.colour),
                blur: points(s.blur),
                distance: points(s.dist),
                direction: s.dir.degrees(),
            }),
            glow: e.glow.as_ref().map(|g| Glow { colour: c(&g.colour), radius: points(g.radius) }),
            soft_edge: e.soft_edge.map(points),
            blur: e.blur.map(points),
        }
    }
}

impl EffectsChange {
    /// `effects` with the change applied.
    pub(crate) fn apply(&self, effects: Option<&paint::EffectList>) -> Result<paint::EffectList, String> {
        let mut e = effects.cloned().unwrap_or_default();
        if let Some(s) = &self.outer_shadow {
            e.outer = match s {
                Some(s) => Some(paint::OuterShadow {
                    blur: emu(s.blur),
                    dist: emu(s.distance),
                    dir: angle(s.direction),
                    sx: Pct(100000),
                    sy: Pct(100000),
                    kx: Angle(0),
                    ky: Angle(0),
                    align: paint::RectAlign::B,
                    colour: s.colour.to_model()?,
                    rot_with_shape: false,
                }),
                None => None,
            };
            e.preset = None;
        }
        if let Some(s) = &self.inner_shadow {
            e.inner = match s {
                Some(s) => {
                    Some(paint::InnerShadow { blur: emu(s.blur), dist: emu(s.distance), dir: angle(s.direction), colour: s.colour.to_model()? })
                }
                None => None,
            };
        }
        if let Some(g) = &self.glow {
            e.glow = match g {
                Some(g) => Some(paint::Glow { radius: emu(g.radius), colour: g.colour.to_model()? }),
                None => None,
            };
        }
        if let Some(v) = self.soft_edge {
            e.soft_edge = v.map(emu);
        }
        if let Some(v) = self.blur {
            e.blur = v.map(emu);
        }
        Ok(e)
    }
}

/// Where text sits in a shape vertically.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TextAnchor {
    /// At the top.
    #[default]
    Top,
    /// In the middle.
    Middle,
    /// At the bottom.
    Bottom,
}

/// How a shape fits its text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TextAutofit {
    /// The shape keeps its size.
    #[default]
    None,
    /// The shape's height follows its text.
    Shape,
}

/// Which way a shape's text runs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TextDirection {
    /// Left to right, lines downward.
    #[default]
    Horizontal,
    /// Turned 90° clockwise.
    Vertical,
    /// Turned 270° clockwise.
    Vertical270,
    /// East Asian vertical: CJK upright, other text turned.
    EastAsianVertical,
    /// Mongolian vertical.
    MongolianVertical,
    /// Letters stacked, one per line.
    Stacked,
    /// Letters stacked, right to left.
    StackedRightToLeft,
}

/// A shape's text body as written.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ShapeTextState {
    /// Left, top, right, bottom.
    pub insets: [Points; 4],
    /// Where the text sits.
    pub anchor: TextAnchor,
    /// How the shape fits it.
    pub autofit: TextAutofit,
    /// Lines wrap at the shape's width.
    pub wrap_text: bool,
    /// Which way it runs.
    pub direction: TextDirection,
}

/// `setShapeText`'s change; what isn't given is kept.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ShapeTextChange {
    /// Left, top, right, bottom (0…1584 pt).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub insets: Option<[Points; 4]>,
    /// Where the text sits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<TextAnchor>,
    /// How the shape fits it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub autofit: Option<TextAutofit>,
    /// Lines wrap at the shape's width.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wrap_text: Option<bool>,
    /// Which way it runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction: Option<TextDirection>,
}

impl ShapeTextState {
    pub(crate) fn of(b: &crate::drawing::text::BodyProps) -> ShapeTextState {
        use crate::drawing::text::{self as t, AutoFit, VAnchor};
        ShapeTextState {
            insets: b.insets.map(points),
            anchor: match b.anchor {
                VAnchor::Centre => TextAnchor::Middle,
                VAnchor::Bottom => TextAnchor::Bottom,
                _ => TextAnchor::Top,
            },
            autofit: if matches!(b.autofit, AutoFit::Shape) { TextAutofit::Shape } else { TextAutofit::None },
            wrap_text: !b.no_wrap,
            direction: match b.direction {
                t::TextDirection::Horizontal => TextDirection::Horizontal,
                t::TextDirection::Vert => TextDirection::Vertical,
                t::TextDirection::Vert270 => TextDirection::Vertical270,
                t::TextDirection::EaVert => TextDirection::EastAsianVertical,
                t::TextDirection::MongolianVert => TextDirection::MongolianVertical,
                t::TextDirection::WordArtVert => TextDirection::Stacked,
                t::TextDirection::WordArtVertRtl => TextDirection::StackedRightToLeft,
            },
        }
    }
}

impl ShapeTextChange {
    /// `body` with the change applied.
    pub(crate) fn apply(&self, body: &crate::drawing::text::BodyProps) -> Result<crate::drawing::text::BodyProps, String> {
        use crate::drawing::text::{self as t, AutoFit, VAnchor};
        let mut b = body.clone();
        if let Some(i) = self.insets {
            if i.iter().any(|v| !(0.0..=1584.0).contains(&v.0)) {
                return Err("an inset is 0 to 1584 pt".into());
            }
            b.insets = i.map(emu);
        }
        if let Some(a) = self.anchor {
            b.anchor = match a {
                TextAnchor::Top => VAnchor::Top,
                TextAnchor::Middle => VAnchor::Centre,
                TextAnchor::Bottom => VAnchor::Bottom,
            };
        }
        if let Some(a) = self.autofit {
            b.autofit = if a == TextAutofit::Shape { AutoFit::Shape } else { AutoFit::None };
        }
        if let Some(w) = self.wrap_text {
            b.no_wrap = !w;
        }
        if let Some(d) = self.direction {
            b.direction = match d {
                TextDirection::Horizontal => t::TextDirection::Horizontal,
                TextDirection::Vertical => t::TextDirection::Vert,
                TextDirection::Vertical270 => t::TextDirection::Vert270,
                TextDirection::EastAsianVertical => t::TextDirection::EaVert,
                TextDirection::MongolianVertical => t::TextDirection::MongolianVert,
                TextDirection::Stacked => t::TextDirection::WordArtVert,
                TextDirection::StackedRightToLeft => t::TextDirection::WordArtVertRtl,
            };
        }
        Ok(b)
    }
}

/// `setPictureAdjust`'s change; what isn't given is kept. Fractions are −1…1 (transparency 0…1).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PictureAdjustChange {
    /// How see-through the picture is (0 opaque, 1 invisible).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transparency: Option<f64>,
    /// Shown in shades of grey.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grayscale: Option<bool>,
    /// Lighter (positive) or darker.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub brightness: Option<f64>,
    /// More (positive) or less contrast.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contrast: Option<f64>,
}

impl PictureAdjustChange {
    /// `adjust` with the change applied.
    pub(crate) fn apply(&self, adjust: &[paint::BlipAdjust]) -> Result<Vec<paint::BlipAdjust>, String> {
        use paint::BlipAdjust as A;
        let pct = |v: f64| Pct((v * 100000.0).round() as i32);
        let unit =
            |v: Option<f64>, lo: f64| v.map_or(Ok(()), |v| if (lo..=1.0).contains(&v) { Ok(()) } else { Err(format!("{v} is out of {lo} to 1")) });
        unit(self.transparency, 0.0)?;
        unit(self.brightness, -1.0)?;
        unit(self.contrast, -1.0)?;
        let mut out: Vec<A> = adjust.to_vec();
        if let Some(t) = self.transparency {
            out.retain(|a| !matches!(a, A::AlphaModFix(_)));
            if t > 0.0 {
                out.push(A::AlphaModFix(pct(1.0 - t)));
            }
        }
        if let Some(g) = self.grayscale {
            out.retain(|a| !matches!(a, A::Grayscale));
            if g {
                out.push(A::Grayscale);
            }
        }
        if self.brightness.is_some() || self.contrast.is_some() {
            let (b0, c0) = out
                .iter()
                .find_map(|a| if let A::Lum { bright, contrast } = a { Some((*bright, *contrast)) } else { None })
                .unwrap_or((Pct(0), Pct(0)));
            out.retain(|a| !matches!(a, A::Lum { .. }));
            let (b, c) = (self.brightness.map_or(b0, pct), self.contrast.map_or(c0, pct));
            if b.0 != 0 || c.0 != 0 {
                out.push(A::Lum { bright: b, contrast: c });
            }
        }
        Ok(out)
    }
}

/// `insertShape`'s options.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ShapeInsert {
    /// The preset geometry.
    pub preset: ShapePreset,
    /// The width (72 pt when not given).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<Points>,
    /// The height (72 pt when not given; 0 for lines).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<Points>,
}

/// `setShapeAdjust`'s options.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ShapeAdjust {
    /// The adjust value's index, in the preset's order.
    pub index: u8,
    /// Its new value (the preset's units, usually 100000ths).
    pub value: i64,
}

/// Which way to flip.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FlipAxis {
    /// Left to right.
    Horizontal,
    /// Top to bottom.
    Vertical,
}

/// Where an object goes in the paint order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Arrange {
    /// Over the next object above it.
    Forward,
    /// Under the next object below it.
    Backward,
    /// Over every object.
    ToFront,
    /// Under every object.
    ToBack,
    /// In front of the text.
    InFrontOfText,
    /// Behind the text.
    BehindText,
}

/// A fill as written: the shape's own, or its style reference's.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FillState {
    /// The shape's own fill.
    Explicit(FillValue),
    /// The style reference's fill (the theme's, with the reference's colour).
    FromStyle(FillValue),
}

/// An outline as written: the shape's own, or its style reference's.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LineState {
    /// The shape's own outline.
    Explicit(LineValue),
    /// The style reference's outline.
    FromStyle(LineValue),
}

/// The selected shape's properties, as written.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ShapeState {
    /// The preset geometry; `null` for a custom geometry.
    pub preset: Option<ShapePreset>,
    /// The adjust values in effect, in the preset's order.
    pub adjust: Vec<i64>,
    /// The fill.
    pub fill: FillState,
    /// The outline.
    pub line: LineState,
    /// The effects.
    pub effects: EffectsValue,
    /// The text body; `null` for pictures and groups.
    pub text: Option<ShapeTextState>,
}

/// A group of the shape gallery.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ShapeGalleryGroup {
    /// Its label (`"Basic Shapes"`).
    pub label: String,
    /// Its presets, in Word's order.
    pub presets: Vec<ShapePreset>,
}

/// Word's Insert > Shapes gallery: its groups and their presets, in order (research R13).
const GALLERY: &[(&str, &str)] = &[
    ("Lines", "line straightConnector1 bentConnector3 curvedConnector3"),
    ("Rectangles", "rect roundRect snip1Rect snip2SameRect snip2DiagRect snipRoundRect round1Rect round2SameRect round2DiagRect"),
    (
        "Basic Shapes",
        "ellipse triangle rtTriangle parallelogram trapezoid diamond pentagon hexagon heptagon octagon decagon dodecagon pie chord teardrop frame halfFrame corner diagStripe plus plaque can cube bevel donut noSmoking blockArc foldedCorner smileyFace heart lightningBolt sun moon cloud arc bracketPair bracePair leftBracket rightBracket leftBrace rightBrace",
    ),
    (
        "Block Arrows",
        "rightArrow leftArrow upArrow downArrow leftRightArrow upDownArrow quadArrow leftRightUpArrow bentArrow uturnArrow leftUpArrow bentUpArrow curvedRightArrow curvedLeftArrow curvedUpArrow curvedDownArrow stripedRightArrow notchedRightArrow homePlate chevron rightArrowCallout downArrowCallout leftArrowCallout upArrowCallout leftRightArrowCallout quadArrowCallout circularArrow",
    ),
    ("Equation Shapes", "mathPlus mathMinus mathMultiply mathDivide mathEqual mathNotEqual"),
    (
        "Flowchart",
        "flowChartProcess flowChartAlternateProcess flowChartDecision flowChartInputOutput flowChartPredefinedProcess flowChartInternalStorage flowChartDocument flowChartMultidocument flowChartTerminator flowChartPreparation flowChartManualInput flowChartManualOperation flowChartConnector flowChartOffpageConnector flowChartPunchedCard flowChartPunchedTape flowChartSummingJunction flowChartOr flowChartCollate flowChartSort flowChartExtract flowChartMerge flowChartOnlineStorage flowChartDelay flowChartMagneticTape flowChartMagneticDisk flowChartMagneticDrum flowChartDisplay",
    ),
    (
        "Stars and Banners",
        "irregularSeal1 irregularSeal2 star4 star5 star6 star7 star8 star10 star12 star16 star24 star32 ribbon2 ribbon ellipseRibbon2 ellipseRibbon verticalScroll horizontalScroll wave doubleWave",
    ),
    (
        "Callouts",
        "wedgeRectCallout wedgeRoundRectCallout wedgeEllipseCallout cloudCallout borderCallout1 borderCallout2 borderCallout3 accentCallout1 accentCallout2 accentCallout3 callout1 callout2 callout3 accentBorderCallout1 accentBorderCallout2 accentBorderCallout3",
    ),
];

/// The shape gallery: the presets of Word's Insert > Shapes gallery in its eight groups, in Word's order.
pub fn shape_gallery() -> Vec<ShapeGalleryGroup> {
    GALLERY
        .iter()
        .map(|(label, names)| ShapeGalleryGroup { label: (*label).into(), presets: names.split(' ').filter_map(ShapePreset::from_name).collect() })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_keep_their_theme_and_transforms() {
        let v = ColourValue {
            theme: Some(ThemeColour::Accent1),
            transforms: vec![ColourTransform { kind: TransformKind::LumMod, value: 0.75 }],
            ..ColourValue::default()
        };
        let m = v.to_model().expect("valid");
        assert_eq!(m, Colour::scheme(SchemeSlot::Accent(1)).with(ColourMod::LumMod(Pct(75000))));
        assert_eq!(ColourValue::of(&m, None), v);
        assert!(ColourValue { theme: Some(ThemeColour::Text1), rgb: Some(Color(0)), ..ColourValue::default() }.to_model().is_err());
        let json = serde_json::to_value(&v).expect("serialises");
        assert_eq!(json, serde_json::json!({ "theme": "accent1", "transforms": [{ "kind": "lumMod", "value": 0.75 }] }));
    }

    #[test]
    fn the_gallery_names_only_presets() {
        for (label, names) in GALLERY {
            for n in names.split(' ') {
                assert!(ShapePreset::from_name(n).is_some(), "{label}: {n}");
            }
        }
        assert_eq!(shape_gallery().len(), 8);
    }

    #[test]
    fn presets_go_by_their_names() {
        let p: ShapePreset = serde_json::from_str("\"wedgeRectCallout\"").expect("a preset");
        assert_eq!(p.name(), "wedgeRectCallout");
        assert!(serde_json::from_str::<ShapePreset>("\"nope\"").is_err());
        assert!(ShapePreset::from_name("straightConnector1").expect("a preset").is_line());
        assert!(!ShapePreset::from_name("rect").expect("a preset").is_line());
    }

    #[test]
    fn effect_changes_keep_remove_or_replace() {
        let c: EffectsChange = serde_json::from_str(r#"{"glow": null, "softEdge": 3}"#).expect("parses");
        assert_eq!((c.glow.clone(), c.soft_edge, c.outer_shadow.clone()), (Some(None), Some(Some(Points(3.0))), None));
        let e = c
            .apply(Some(&paint::EffectList { glow: Some(paint::Glow { radius: Emu(1), colour: Colour::srgb(0) }), ..Default::default() }))
            .expect("applies");
        assert_eq!((e.glow, e.soft_edge), (None, Some(Emu(38100))));
    }
}
