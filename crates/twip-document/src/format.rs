//! Formatting values

use std::rc::Rc;

use crate::graphic::colour::SchemeSlot;
use crate::graphic::paint::{Fill, Glow, Line, OuterShadow, Reflection};
use crate::graphic::theme::{ColourMap, Theme};
use crate::line::BorderStyle;

/// A run's vertical position relative to the baseline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VertAlign {
    /// On the baseline.
    Baseline,
    /// Raised and smaller.
    Superscript,
    /// Lowered and smaller.
    Subscript,
}

/// An emphasis mark over or under each character.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Emphasis {
    /// No mark, set over an inherited one.
    None,
    /// A dot above.
    Dot,
    /// A comma above.
    Comma,
    /// A circle above.
    Circle,
    /// A dot below.
    UnderDot,
}

/// A stripe or cross of shading, drawn as an 8×8 tile in the pattern colour over the fill.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Stripe {
    /// Horizontal stripes.
    Horz,
    /// Vertical stripes.
    Vert,
    /// Stripes down to the left.
    ReverseDiag,
    /// Stripes down to the right.
    Diag,
    /// A horizontal and vertical cross.
    HorzCross,
    /// A diagonal cross.
    DiagCross,
    /// Thin horizontal stripes.
    ThinHorz,
    /// Thin vertical stripes.
    ThinVert,
    /// Thin stripes down to the left.
    ThinReverseDiag,
    /// Thin stripes down to the right.
    ThinDiag,
    /// A thin horizontal and vertical cross.
    ThinHorzCross,
    /// A thin diagonal cross.
    ThinDiagCross,
}

/// A shading pattern.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ShadingPattern {
    /// No shading, set over inherited shading.
    Nil,
    /// The fill alone.
    Clear,
    /// The pattern colour alone.
    Solid,
    /// This percentage (0 to 100) of the pattern colour over the fill.
    Percent(u8),
    /// A stripe or cross in the pattern colour over the fill.
    Stripe(Stripe),
}

/// A paragraph's alignment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Jc {
    /// To the start edge.
    Left,
    /// Centred.
    Center,
    /// To the end edge.
    Right,
    /// Justified, the last line not.
    Both,
    /// Justified with space between characters too, the last line included.
    Distribute,
}

/// How a paragraph's line spacing is measured.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineRule {
    /// A multiple of the line's natural height.
    Auto,
    /// Exactly the given height.
    Exact,
    /// At least the given height.
    AtLeast,
}

/// A tab stop's kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TabKind {
    /// Text starts at the stop.
    Left,
    /// Text centres on the stop.
    Center,
    /// Text ends at the stop.
    Right,
    /// The decimal point sits at the stop.
    Decimal,
    /// A vertical bar at the stop; text does not stop there.
    Bar,
    /// A list's tab, which stops like a left tab.
    Num,
    /// Clears a stop the paragraph would inherit.
    Clear,
}

/// What fills the space before a tab stop.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Leader {
    /// Nothing.
    None,
    /// Dots.
    Dot,
    /// Hyphens.
    Hyphen,
    /// A line.
    Underscore,
    /// A heavy line (tab stops only; an alignment tab has none).
    Heavy,
    /// Middle dots.
    MiddleDot,
}

impl Leader {
    /// The character the leader repeats, if any.
    pub fn glyph(self) -> Option<char> {
        match self {
            Leader::None => None,
            Leader::Dot => Some('.'),
            Leader::Hyphen => Some('-'),
            Leader::Underscore | Leader::Heavy => Some('_'),
            Leader::MiddleDot => Some('·'),
        }
    }
}

/// A table, cell or indent width.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Width {
    /// Decided by the content.
    Auto,
    /// No width.
    Nil,
    /// Points.
    Dxa(f64),
    /// Fraction (1.0 = 100%).
    Pct(f64),
}

impl Width {
    /// A dxa or pct width as it is, any other as 0 points.
    pub fn sized(self) -> Width {
        match self {
            Width::Dxa(_) | Width::Pct(_) => self,
            _ => Width::Dxa(0.0),
        }
    }

    /// The points of a dxa width, 0 for any other.
    pub fn dxa(self) -> f64 {
        if let Width::Dxa(v) = self { v } else { 0.0 }
    }

    /// The fraction of a pct width.
    pub fn pct(self) -> Option<f64> {
        if let Width::Pct(p) = self { Some(p) } else { None }
    }
}

/// A run's text effects, each as DrawingML describes it. Each is set or not on its own, and a style chain's levels
/// override one at a time.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextEffects {
    /// The letters' fill in place of the text colour: none, a solid or a gradient.
    pub fill: Option<Fill>,
    /// The letters' outline.
    pub outline: Option<Line>,
    /// A shadow outside the letters.
    pub shadow: Option<OuterShadow>,
    /// A glow around the letters.
    pub glow: Option<Glow>,
    /// A reflection below the letters.
    pub reflection: Option<Reflection>,
}

impl TextEffects {
    /// No effect set.
    pub fn is_empty(&self) -> bool {
        *self == TextEffects::default()
    }

    /// The effects that show: without those written as off (a glow of no radius, a shadow or reflection of no size,
    /// an outline with no fill), which is how a run turns off its style's. `None` when none shows.
    pub fn shown(e: &Rc<TextEffects>) -> Option<Rc<TextEffects>> {
        let glow = e.glow.as_ref().is_some_and(|g| g.radius.0 == 0);
        let shadow = e.shadow.as_ref().is_some_and(|s| s.sx.0 == 0 || s.sy.0 == 0);
        let reflection = e.reflection.as_ref().is_some_and(|r| r.sx.0 == 0 || r.sy.0 == 0);
        let outline = e.outline.as_ref().is_some_and(|l| matches!(l.fill, None | Some(Fill::None)));
        let shown = if glow || shadow || reflection || outline {
            let mut s = (**e).clone();
            if glow {
                s.glow = None;
            }
            if shadow {
                s.shadow = None;
            }
            if reflection {
                s.reflection = None;
            }
            if outline {
                s.outline = None;
            }
            Rc::new(s)
        } else {
            e.clone()
        };
        (!shown.is_empty()).then_some(shown)
    }

    /// Each effect `o` sets, over this one's.
    pub fn merge(&mut self, o: &TextEffects) {
        macro_rules! take {
            ($($f:ident),*) => { $( if o.$f.is_some() { self.$f = o.$f.clone(); } )* };
        }
        take!(fill, outline, shadow, glow, reflection);
    }
}

/// Shading: its pattern, the pattern's colour and the fill (none: automatic).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shd {
    /// The pattern.
    pub pattern: ShadingPattern,
    /// The pattern's colour, `0xRRGGBB`.
    pub color: Option<u32>,
    /// The fill, `0xRRGGBB`.
    pub fill: Option<u32>,
    /// The theme colour the pattern colour is.
    pub color_theme: Option<ThemeColourRef>,
    /// The theme colour the fill is.
    pub fill_theme: Option<ThemeColourRef>,
}

impl Shd {
    /// The flat colour it shows: the fill, the pattern colour when solid, a `pctNN` blend of the pattern colour over
    /// the fill; nothing for `nil`. A stripe or cross shows its fill here and its [`Shd::hatch`] over it.
    pub fn flat(&self) -> Option<u32> {
        match self.pattern {
            ShadingPattern::Nil => None,
            ShadingPattern::Solid => Some(self.color.unwrap_or(0)),
            ShadingPattern::Percent(pct) => {
                let (fg, bg) = (self.color.unwrap_or(0), self.fill.unwrap_or(0xFFFFFF));
                let mix = |s: u32| {
                    let (a, b) = ((fg >> s) & 255, (bg >> s) & 255);
                    ((b as i64 + (a as i64 - b as i64) * pct as i64 / 100) as u32) << s
                };
                Some(mix(16) | mix(8) | mix(0))
            }
            ShadingPattern::Clear | ShadingPattern::Stripe(_) => self.fill,
        }
    }

    /// Its stripe or cross in the pattern colour (automatic: black), if it has one.
    pub fn hatch(&self) -> Option<Hatch> {
        match self.pattern {
            ShadingPattern::Stripe(stripe) => Some(Hatch { stripe, colour: self.color.unwrap_or(0) }),
            _ => None,
        }
    }
}

/// A stripe or cross and its colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Hatch {
    /// The stripe or cross.
    pub stripe: Stripe,
    /// Its colour, `0xRRGGBB`.
    pub colour: u32,
}

/// A border line.
#[derive(Clone, Debug, PartialEq)]
pub struct Border {
    /// The line.
    pub style: BorderStyle,
    /// Line width in points.
    pub width: f64,
    /// The space between the line and what it borders, points.
    pub space: f64,
    /// It has a shadow.
    pub shadow: bool,
    /// Its colour, `0xRRGGBB` (none: automatic).
    pub color: Option<u32>,
    /// The theme colour `color` is.
    pub color_theme: Option<ThemeColourRef>,
}

impl Border {
    /// It draws a line.
    pub fn visible(&self) -> bool {
        !matches!(self.style, BorderStyle::None | BorderStyle::Nil)
    }
}

/// A tab stop.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tab {
    /// Its position, points.
    pub pos: f64,
    /// Its kind.
    pub kind: TabKind,
    /// What fills the space before it.
    pub leader: Leader,
}

impl Tab {
    /// It clears an inherited stop.
    pub fn is_clear(&self) -> bool {
        self.kind == TabKind::Clear
    }
}

/// A text frame's or drop cap's placement, its values as written.
#[derive(Clone, Debug, PartialEq)]
pub struct FramePr {
    /// The width, points.
    pub w: Option<f64>,
    /// The height, points.
    pub h: Option<f64>,
    /// How the height applies.
    pub h_rule: Option<String>,
    /// The horizontal position, points.
    pub x: Option<f64>,
    /// The vertical position, points.
    pub y: Option<f64>,
    /// The horizontal alignment, over the position.
    pub x_align: Option<String>,
    /// The vertical alignment, over the position.
    pub y_align: Option<String>,
    /// What the horizontal position measures from.
    pub h_anchor: Option<String>,
    /// What the vertical position measures from.
    pub v_anchor: Option<String>,
    /// How text wraps around it.
    pub wrap: Option<String>,
    /// The space left and right of it, points.
    pub h_space: f64,
    /// The space above and below it, points.
    pub v_space: f64,
    /// The drop cap it is.
    pub drop_cap: Option<String>,
    /// The lines a drop cap spans.
    pub lines: Option<i64>,
    /// Its attributes as one key: paragraphs with equal keys share a frame.
    pub key: String,
}

/// Which of a table style's conditional regions a table shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Look {
    /// The first row's.
    pub first_row: bool,
    /// The last row's.
    pub last_row: bool,
    /// The first column's.
    pub first_col: bool,
    /// The last column's.
    pub last_col: bool,
    /// No row banding.
    pub no_h_band: bool,
    /// No column banding.
    pub no_v_band: bool,
}

/// What a floating table's vertical position measures from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableAnchor {
    /// The text: the paragraph it follows.
    Text,
    /// The margin.
    Margin,
    /// The page.
    Page,
}

/// A floating table's position.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TablePos {
    /// What `y` measures from.
    pub v_anchor: TableAnchor,
    /// The vertical position, points.
    pub y: f64,
    /// `tblpX`, or the `tblpXSpec` alignment (left/center/right) when given.
    pub x: f64,
    /// The horizontal alignment, over `x`.
    pub x_spec: Option<Jc>,
    /// `horzAnchor="page"`: `tblpX` and `tblpXSpec` measure from the page, not the column (`margin`, `text`).
    pub h_page: bool,
    /// `horzAnchor` `text` or absent (not `margin` or `page`).
    pub h_text: bool,
    /// `tblpYSpec` for page and margin anchors: top/inside → Left, center → Center, bottom/outside → Right.
    pub y_spec: Option<Jc>,
    /// `topFromText`, `bottomFromText`, `leftFromText`, `rightFromText` (negatives → 0), points.
    pub dist: [f64; 4],
    /// `w:tblOverlap="never"`.
    pub never_overlap: bool,
}

impl TablePos {
    /// The alignment `tblpXSpec` gives, else the one `tblpX` codes: 0 left, -4 centre, -8 right, -12 inside (left on
    /// a first page), -16 outside.
    // ponytail: inside and outside as on an odd page without mirrored margins.
    pub fn x_align(&self) -> Option<Jc> {
        self.x_spec.or(match (self.x * 20.0).round() as i64 {
            0 | -12 => Some(Jc::Left),
            -4 => Some(Jc::Center),
            -8 | -16 => Some(Jc::Right),
            _ => None,
        })
    }
}

// Theme

/// A theme colour, lightened (`tint`) or darkened (`shade`) by a byte: the fraction of the colour's luminance kept.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ThemeColourRef {
    /// The theme colour.
    pub slot: SchemeSlot,
    /// Lightened by this byte.
    pub tint: Option<u8>,
    /// Darkened by this byte.
    pub shade: Option<u8>,
}

/// The theme's colours a document's text names, `0xRRGGBB` by slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ThemeColours([u32; 16]);

impl Default for ThemeColours {
    fn default() -> ThemeColours {
        ThemeColours::new(&Theme::default(), &ColourMap::default())
    }
}

impl ThemeColours {
    /// The slots, in the order the colours are held.
    pub const SLOTS: [SchemeSlot; 16] = {
        use SchemeSlot::*;
        [Dk1, Lt1, Dk2, Lt2, Accent(1), Accent(2), Accent(3), Accent(4), Accent(5), Accent(6), Hlink, FolHlink, Bg1, Tx1, Bg2, Tx2]
    };

    /// The colours of `theme`, its roles mapped by `map`.
    pub fn new(theme: &Theme, map: &ColourMap) -> ThemeColours {
        ThemeColours(Self::SLOTS.map(|s| {
            let c = theme.colour(map.map(s));
            (u32::from(c.r) << 16) | (u32::from(c.g) << 8) | u32::from(c.b)
        }))
    }

    /// A theme colour with its tint or shade applied.
    pub fn resolve(&self, r: ThemeColourRef) -> u32 {
        let k = Self::SLOTS.iter().position(|s| *s == r.slot).unwrap_or(0);
        let c = self.0[k];
        let byte = |b: Option<u8>| b.map(|v| f64::from(v) / 255.0);
        match (byte(r.tint), byte(r.shade)) {
            (Some(t), _) => with_luminance(c, |l| l * t + (1.0 - t)),
            (None, Some(s)) => with_luminance(c, |l| l * s),
            (None, None) => c,
        }
    }
}

/// `c` with its HSL luminance changed by `f`.
fn with_luminance(c: u32, f: impl Fn(f64) -> f64) -> u32 {
    let [r, g, b] = [16, 8, 0].map(|s| f64::from((c >> s) & 0xFF) / 255.0);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let l = (max + min) / 2.0;
    let d = max - min;
    let sat = if d == 0.0 { 0.0 } else { d / (1.0 - (2.0 * l - 1.0).abs()) };
    let hue = if d == 0.0 {
        0.0
    } else if max == r {
        ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    let l = f(l).clamp(0.0, 1.0);
    let ch = (1.0 - (2.0 * l - 1.0).abs()) * sat;
    let x = ch * (1.0 - (hue.rem_euclid(2.0) - 1.0).abs());
    let (r1, g1, b1) = match hue as u32 {
        0 => (ch, x, 0.0),
        1 => (x, ch, 0.0),
        2 => (0.0, ch, x),
        3 => (0.0, x, ch),
        4 => (x, 0.0, ch),
        _ => (ch, 0.0, x),
    };
    let m = l - ch / 2.0;
    let to = |v: f64| ((v + m).clamp(0.0, 1.0) * 255.0).round() as u32;
    (to(r1) << 16) | (to(g1) << 8) | to(b1)
}

/// The theme's fonts: the major (headings) and minor (body) Latin faces, and their East Asian and complex-script
/// faces when the theme names them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ThemeFonts {
    /// The major Latin face.
    pub major: String,
    /// The minor Latin face.
    pub minor: String,
    /// The major East Asian face.
    pub major_ea: Option<String>,
    /// The minor East Asian face.
    pub minor_ea: Option<String>,
    /// The major complex-script face.
    pub major_cs: Option<String>,
    /// The minor complex-script face.
    pub minor_cs: Option<String>,
}

impl ThemeFonts {
    /// An East Asian theme slot without a typeface ([`Self::resolve`] gives the Latin font).
    pub fn ea_unset(&self, theme: &str) -> bool {
        theme.ends_with("EastAsia") && if theme.starts_with("major") { self.major_ea.is_none() } else { self.minor_ea.is_none() }
    }

    /// A theme font slot's name (`majorHAnsi`, `minorEastAsia`, ...) as a family name.
    pub fn resolve(&self, theme: &str) -> String {
        let major = theme.starts_with("major");
        let (latin, ea, cs) = if major { (&self.major, &self.major_ea, &self.major_cs) } else { (&self.minor, &self.minor_ea, &self.minor_cs) };
        if theme.ends_with("EastAsia") {
            ea.clone().unwrap_or_else(|| latin.clone())
        } else if theme.ends_with("Bidi") {
            cs.clone().unwrap_or_else(|| latin.clone())
        } else {
            latin.clone()
        }
    }
}
