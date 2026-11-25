//! Formatting properties

use roxmltree::Node;

pub use crate::document::format::{Emphasis, Jc, Leader, LineRule, ShadingPattern, Stripe, TabKind, VertAlign, Width};
pub use crate::document::line::{ArtBorder, BorderStyle};

use super::xml::*;

/// A font attribute pair (`w:ascii` / `w:asciiTheme`): the theme form wins within one element.
#[derive(Clone, Debug, PartialEq)]
pub enum FontRef {
    Name(String),
    Theme(String),
}

pub mod toggle {
    pub const B: u16 = 1;
    pub const I: u16 = 2;
    pub const CAPS: u16 = 4;
    pub const SMALL_CAPS: u16 = 8;
    pub const STRIKE: u16 = 16;
    pub const VANISH: u16 = 32;
    pub const B_CS: u16 = 64;
    pub const I_CS: u16 = 128;
    pub const EMBOSS: u16 = 256;
    pub const IMPRINT: u16 = 512;
    pub const OUTLINE: u16 = 1024;
    pub const SHADOW: u16 = 2048;
    pub const NAMES: [(&str, u16); 12] = [
        ("b", B),
        ("i", I),
        ("caps", CAPS),
        ("smallCaps", SMALL_CAPS),
        ("strike", STRIKE),
        ("vanish", VANISH),
        ("bCs", B_CS),
        ("iCs", I_CS),
        ("emboss", EMBOSS),
        ("imprint", IMPRINT),
        ("outline", OUTLINE),
        ("shadow", SHADOW),
    ];
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RunProps {
    pub ascii: Option<FontRef>,
    pub h_ansi: Option<FontRef>,
    pub east_asia: Option<FontRef>,
    /// The named East Asian font a theme reference above replaced: Word uses it when that theme slot is empty
    /// (BF: docDefaults eastAsia="Times New Roman" under Normal's eastAsiaTheme, the slot empty).
    pub east_asia_below: Option<FontRef>,
    pub east_asia_theme: Option<FontRef>,
    pub cs: Option<FontRef>,
    pub hint: Option<String>,
    pub size: Option<f64>,
    /// `w:szCs`: the complex-script size.
    pub size_cs: Option<f64>,
    /// `w:cs`: the run is laid out in its complex-script properties.
    pub complex: Option<bool>,
    /// Toggles this level sets (`toggles_set`) and their values.
    pub toggles_set: u16,
    pub toggles: u16,
    pub dstrike: Option<bool>,
    /// `w:noProof`.
    pub no_proof: Option<bool>,
    pub underline: Option<super::model::Underline>,
    /// `w:u`'s colour; `Some(None)` = the text's.
    pub underline_color: Option<Option<u32>>,
    /// The theme colour `underline_color` is, set with it.
    pub underline_color_theme: Option<super::model::ThemeColourRef>,
    /// `Some(None)` = auto.
    pub color: Option<Option<u32>>,
    /// The theme colour `color` is, set with it.
    pub color_theme: Option<super::model::ThemeColourRef>,
    pub highlight: Option<Option<u32>>,
    pub shading: Option<Option<u32>>,
    /// The `w:shd` as written: its pattern, pattern color and fill.
    pub shd: Option<Shd>,
    /// `w:bdr`: the text's border.
    pub border: Option<Border>,
    pub vert_align: Option<VertAlign>,
    pub position: Option<f64>,
    pub spacing: Option<f64>,
    pub scale: Option<f64>,
    /// `w:kern`: kern at or above this size (points); 0 = off.
    pub kern: Option<f64>,
    /// `w14:ligatures` bits: 1 standard, 2 contextual, 4 historical, 8 discretionary.
    pub ligatures: Option<u8>,
    pub cntxt_alts: Option<bool>,
    /// `w14:numForm`: 1 lining, 2 old style, 0 default.
    pub num_form: Option<u32>,
    /// `w14:numSpacing`: 4 proportional, 8 tabular, 0 default.
    pub num_spacing: Option<u32>,
    /// `w14:stylisticSets`: bit n − 1 for set n.
    pub style_sets: Option<u32>,
    /// `w:em`: the emphasis mark.
    pub em: Option<Emphasis>,
    pub lang: Option<String>,
    /// `w:lang w:eastAsia`.
    pub lang_east_asia: Option<String>,
    pub style: Option<String>,
    pub rtl: Option<bool>,
    /// `w:fitText`: the width to fit the text to (points) and its region id.
    pub fit_text: Option<(f64, i64)>,
    /// The Word 2010 text effects this level sets.
    pub effects: Option<std::rc::Rc<super::model::TextEffects>>,
}

fn font_ref(n: Node, name: &str) -> Option<FontRef> {
    let theme = match name {
        "ascii" => attr(n, "asciiTheme"),
        "hAnsi" => attr(n, "hAnsiTheme"),
        "eastAsia" => attr(n, "eastAsiaTheme"),
        _ => attr(n, "csTheme").or_else(|| attr(n, "cstheme")),
    };
    if let Some(t) = theme {
        return Some(FontRef::Theme(t.to_string()));
    }
    attr(n, name).filter(|s| !s.is_empty()).map(|s| FontRef::Name(s.to_string()))
}

impl RunProps {
    pub fn parse(rpr: Option<Node>, tc: &ThemeColours) -> RunProps {
        let mut p = RunProps::default();
        let Some(rpr) = rpr else { return p };
        for e in rpr.children().filter(|c| c.is_element()) {
            let name = e.tag_name().name();
            if e.tag_name().namespace() == Some(W14) {
                if name == "ligatures" {
                    p.ligatures = Some(match val_any(e) {
                        Some("standard") => 1,
                        Some("contextual") => 2,
                        Some("historical") => 4,
                        Some("discretional") => 8,
                        Some("standardContextual") => 3,
                        Some("standardHistorical") => 5,
                        Some("contextualHistorical") => 6,
                        Some("standardDiscretional") => 9,
                        Some("contextualDiscretional") => 10,
                        Some("historicalDiscretional") => 12,
                        Some("standardContextualHistorical") => 7,
                        Some("standardContextualDiscretional") => 11,
                        Some("standardHistoricalDiscretional") => 13,
                        Some("contextualHistoricalDiscretional") => 14,
                        Some("all") => 15,
                        _ => 0,
                    });
                } else if name == "cntxtAlts" {
                    p.cntxt_alts = Some(!matches!(val_any(e), Some("0" | "false" | "off")));
                } else if name == "numForm" {
                    p.num_form = Some(match val_any(e) {
                        Some("lining") => 1,
                        Some("oldStyle") => 2,
                        _ => 0,
                    });
                } else if name == "numSpacing" {
                    p.num_spacing = Some(match val_any(e) {
                        Some("proportional") => 4,
                        Some("tabular") => 8,
                        _ => 0,
                    });
                } else if matches!(name, "textFill" | "textOutline" | "shadow" | "glow" | "reflection") {
                    read_text_effect(e, std::rc::Rc::make_mut(p.effects.get_or_insert_with(Default::default)));
                } else if name == "stylisticSets" {
                    let on = |s: &Node| !matches!(s.attribute((W14, "val")), Some("0" | "false" | "off"));
                    let ids = e.children().filter(|s| is(*s, W14, "styleSet") && on(s)).filter_map(|s| s.attribute((W14, "id"))?.parse::<u32>().ok());
                    p.style_sets = Some(ids.filter(|n| (1..=20).contains(n)).fold(0, |m, n| m | 1 << (n - 1)));
                }
                continue;
            }
            if e.tag_name().namespace() != Some(W) {
                continue;
            }
            if let Some(&(_, bit)) = toggle::NAMES.iter().find(|(n, _)| *n == name) {
                p.toggles_set |= bit;
                if on_off(e) {
                    p.toggles |= bit;
                } else {
                    p.toggles &= !bit;
                }
                continue;
            }
            match name {
                "rFonts" => {
                    p.ascii = font_ref(e, "ascii");
                    p.h_ansi = font_ref(e, "hAnsi");
                    p.east_asia = font_ref(e, "eastAsia");
                    p.cs = font_ref(e, "cs");
                    p.hint = attr(e, "hint").map(str::to_string);
                }
                "sz" => p.size = half_points(val(e)),
                "szCs" => p.size_cs = half_points(val(e)),
                "cs" => p.complex = Some(on_off(e)),
                "dstrike" => p.dstrike = Some(on_off(e)),
                "noProof" => p.no_proof = Some(on_off(e)),
                "u" => {
                    p.underline = Some(val(e).map_or(super::model::Underline::Single, super::line_style::underline));
                    p.underline_color = Some(tc.colour(e, "color", "themeColor", "themeTint", "themeShade"));
                    p.underline_color_theme = theme_ref(e, "themeColor", "themeTint", "themeShade");
                }
                "color" => {
                    p.color = Some(tc.colour(e, "val", "themeColor", "themeTint", "themeShade"));
                    p.color_theme = theme_ref(e, "themeColor", "themeTint", "themeShade");
                }
                "highlight" => {
                    p.highlight = Some(match val(e) {
                        Some("none") | None => None,
                        Some(v) => highlight(v),
                    })
                }
                "shd" => {
                    p.shading = Some(shading_fill(e, tc));
                    p.shd = Some(Shd::parse(e, tc));
                }
                "bdr" => p.border = Some(Border::parse(e, tc)),
                "em" => p.em = Some(val(e).map_or(Emphasis::None, super::line_style::emphasis)),
                "vertAlign" => {
                    p.vert_align = Some(match val(e) {
                        Some("superscript") => VertAlign::Superscript,
                        Some("subscript") => VertAlign::Subscript,
                        _ => VertAlign::Baseline,
                    })
                }
                "position" => p.position = half_points(val(e)),
                "spacing" => p.spacing = twips(val(e)),
                "w" => p.scale = num(val(e).map(|v| v.trim_end_matches('%'))),
                "kern" => p.kern = half_points(val(e)),
                "lang" => {
                    p.lang = val(e).map(str::to_string);
                    p.lang_east_asia = attr(e, "eastAsia").map(str::to_string);
                }
                "rStyle" => p.style = val(e).map(str::to_string),
                "rtl" => p.rtl = Some(on_off(e)),
                "fitText" => p.fit_text = twips(val(e)).map(|w| (w, int(attr(e, "id")).unwrap_or(0))),
                _ => {}
            }
        }
        p
    }

    /// Overrides every attribute `o` sets; toggles are absolute (direct formatting, or within one style chain).
    pub fn merge(&mut self, o: &RunProps) {
        let named = matches!(self.east_asia, Some(FontRef::Name(_))) || self.east_asia_below.is_some();
        match (&self.east_asia, &o.east_asia) {
            (Some(FontRef::Theme(_)), Some(FontRef::Name(_))) if self.east_asia_below.is_none() => self.east_asia_theme = self.east_asia.clone(),
            (_, Some(_)) => self.east_asia_theme = None,
            _ => {}
        }
        if o.east_asia_theme.is_some() && !named {
            self.east_asia_theme = o.east_asia_theme.clone();
        }
        match (&self.east_asia, &o.east_asia) {
            (Some(FontRef::Name(_)), Some(FontRef::Theme(_))) => self.east_asia_below = self.east_asia.clone(),
            (_, Some(FontRef::Name(_))) => self.east_asia_below = None,
            _ => {}
        }
        if o.east_asia_below.is_some() {
            self.east_asia_below = o.east_asia_below.clone();
        }
        macro_rules! take {
            ($($f:ident),*) => { $( if o.$f.is_some() { self.$f = o.$f.clone(); } )* };
        }
        take!(
            ascii,
            h_ansi,
            east_asia,
            cs,
            hint,
            size,
            size_cs,
            complex,
            dstrike,
            no_proof,
            underline,
            underline_color,
            color,
            highlight,
            shading,
            shd,
            border,
            vert_align,
            position,
            spacing,
            scale,
            kern,
            ligatures,
            cntxt_alts,
            num_form,
            num_spacing,
            style_sets,
            lang,
            lang_east_asia,
            style,
            rtl,
            em,
            fit_text
        );
        if o.color.is_some() {
            self.color_theme = o.color_theme;
        }
        if o.underline_color.is_some() {
            self.underline_color_theme = o.underline_color_theme;
        }
        match (&mut self.effects, &o.effects) {
            (_, None) => {}
            (None, Some(e)) => self.effects = Some(e.clone()),
            (Some(mine), Some(e)) => std::rc::Rc::make_mut(mine).merge(e),
        }
        self.toggles = (self.toggles & !o.toggles_set) | (o.toggles & o.toggles_set);
        self.toggles_set |= o.toggles_set;
    }

    pub fn merge_level(&mut self, level: &RunProps) {
        let on = level.toggles & level.toggles_set;
        let mut rest = level.clone();
        rest.toggles_set = 0;
        self.merge(&rest);
        self.toggles ^= on;
        self.toggles_set |= on;
    }

    pub fn fold_chain<'a>(chain: impl Iterator<Item = &'a RunProps>) -> RunProps {
        let mut acc = RunProps::default();
        for s in chain {
            acc.merge(s);
        }
        let on = acc.toggles & acc.toggles_set;
        acc.toggles = on;
        acc.toggles_set = on;
        acc
    }

    pub fn toggle(&self, bit: u16) -> bool {
        self.toggles & bit != 0
    }
}

/// Reads a Word 2010 text effect element (`w14:textFill`, `w14:textOutline`, `w14:shadow`, `w14:glow`,
/// `w14:reflection`) into `t`, with DrawingML's readers in the `w14` namespace.
fn read_text_effect(e: Node, t: &mut super::model::TextEffects) {
    use crate::drawing::colour::Ns;
    use crate::drawing::paint::{Fill, Glow, Line, OuterShadow, Reflection};
    match e.tag_name().name() {
        "textFill" => t.fill = Fill::child_of_in(e, Ns::W14, &mut |_| None).or(t.fill.take()),
        "textOutline" => t.outline = Some(Line::parse_in(e, Ns::W14, &mut |_| None)),
        "shadow" => t.shadow = Some(OuterShadow::parse(e, Ns::W14)),
        "glow" => t.glow = Some(Glow::parse(e, Ns::W14)),
        "reflection" => t.reflection = Some(Reflection::parse(e, Ns::W14)),
        _ => {}
    }
}

/// `w14:val` or `w:val`.
fn val_any<'a>(n: Node<'a, '_>) -> Option<&'a str> {
    n.attribute((W14, "val")).or_else(|| val(n))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ThemeColours([u32; 16]);

/// 22 inches in points: Word's largest page side, and the bound on a table width, margin or border.
const MAX_MEASURE: f64 = 1584.0;
/// 10000% as a fraction: far past any table share, small enough that sums of shares can't overflow.
const MAX_PCT: f64 = 100.0;

const THEME_COLOUR_NAMES: [&str; 16] = [
    "dark1",
    "light1",
    "dark2",
    "light2",
    "accent1",
    "accent2",
    "accent3",
    "accent4",
    "accent5",
    "accent6",
    "hyperlink",
    "followedHyperlink",
    "background1",
    "text1",
    "background2",
    "text2",
];

impl Default for ThemeColours {
    fn default() -> ThemeColours {
        ThemeColours::new(&crate::drawing::theme::Theme::default(), &crate::drawing::theme::ColourMap::default())
    }
}

impl ThemeColours {
    pub fn new(theme: &crate::drawing::theme::Theme, map: &crate::drawing::theme::ColourMap) -> ThemeColours {
        ThemeColours(THEME_SLOTS.map(|s| {
            let c = theme.colour(map.map(s));
            (u32::from(c.r) << 16) | (u32::from(c.g) << 8) | u32::from(c.b)
        }))
    }

    pub fn colour(&self, e: Node, hex: &str, theme: &str, tint: &str, shade: &str) -> Option<u32> {
        match theme_ref(e, theme, tint, shade) {
            Some(r) => Some(self.resolve(r)),
            None => color(attr(e, hex)),
        }
    }

    /// A theme colour with its tint or shade applied.
    pub fn resolve(&self, r: super::model::ThemeColourRef) -> u32 {
        let k = THEME_SLOTS.iter().position(|s| *s == r.slot).unwrap_or(0);
        let c = self.0[k];
        let byte = |b: Option<u8>| b.map(|v| f64::from(v) / 255.0);
        match (byte(r.tint), byte(r.shade)) {
            (Some(t), _) => with_luminance(c, |l| l * t + (1.0 - t)),
            (None, Some(s)) => with_luminance(c, |l| l * s),
            (None, None) => c,
        }
    }
}

/// The slots of [`THEME_COLOUR_NAMES`], in its order.
const THEME_SLOTS: [crate::drawing::colour::SchemeSlot; 16] = {
    use crate::drawing::colour::SchemeSlot::*;
    [Dk1, Lt1, Dk2, Lt2, Accent(1), Accent(2), Accent(3), Accent(4), Accent(5), Accent(6), Hlink, FolHlink, Bg1, Tx1, Bg2, Tx2]
};

/// The theme colour `e` names in its attribute `theme`, with its `tint` or `shade` byte.
pub fn theme_ref(e: Node, theme: &str, tint: &str, shade: &str) -> Option<super::model::ThemeColourRef> {
    let k = attr(e, theme).and_then(|t| THEME_COLOUR_NAMES.iter().position(|n| *n == t))?;
    let byte = |a: &str| attr(e, a).and_then(|v| u8::from_str_radix(v, 16).ok());
    Some(super::model::ThemeColourRef { slot: THEME_SLOTS[k], tint: byte(tint), shade: byte(shade) })
}

/// A theme slot's `w:themeColor` name.
pub fn theme_name(slot: crate::drawing::colour::SchemeSlot) -> Option<&'static str> {
    THEME_SLOTS.iter().position(|s| *s == slot).map(|k| THEME_COLOUR_NAMES[k])
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

pub fn highlight(v: &str) -> Option<u32> {
    Some(match v {
        "yellow" => 0xFFFF00,
        "green" => 0x00FF00,
        "cyan" => 0x00FFFF,
        "magenta" => 0xFF00FF,
        "blue" => 0x0000FF,
        "red" => 0xFF0000,
        "darkBlue" => 0x00008B,
        "darkCyan" => 0x008B8B,
        "darkGreen" => 0x006400,
        "darkMagenta" => 0x800080,
        "darkRed" => 0x8B0000,
        "darkYellow" => 0x808000,
        "darkGray" => 0xA9A9A9,
        "lightGray" => 0xD3D3D3,
        "black" => 0x000000,
        "white" => 0xFFFFFF,
        _ => return None,
    })
}

/// A `w:shd` as written: its pattern (`w:val`), the pattern's color and the fill (none: automatic).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shd {
    pub pattern: ShadingPattern,
    pub color: Option<u32>,
    pub fill: Option<u32>,
    /// The theme colours the pattern colour and the fill are (`w:themeColor`, `w:themeFill`).
    pub color_theme: Option<super::model::ThemeColourRef>,
    pub fill_theme: Option<super::model::ThemeColourRef>,
}

impl Shd {
    pub fn parse(e: Node, tc: &ThemeColours) -> Shd {
        Shd {
            pattern: val(e).map_or(ShadingPattern::Clear, super::line_style::shading_pattern),
            color: tc.colour(e, "color", "themeColor", "themeTint", "themeShade"),
            fill: tc.colour(e, "fill", "themeFill", "themeFillTint", "themeFillShade"),
            color_theme: theme_ref(e, "themeColor", "themeTint", "themeShade"),
            fill_theme: theme_ref(e, "themeFill", "themeFillTint", "themeFillShade"),
        }
    }

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

pub fn shading_fill(e: Node, tc: &ThemeColours) -> Option<u32> {
    Shd::parse(e, tc).flat()
}

/// A stripe or cross and its colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Hatch {
    pub stripe: Stripe,
    pub colour: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Border {
    /// The line.
    pub style: BorderStyle,
    /// Line width in points (`w:sz` eighths).
    pub width: f64,
    pub space: f64,
    pub shadow: bool,
    pub color: Option<u32>,
    /// The theme colour `color` is.
    pub color_theme: Option<super::model::ThemeColourRef>,
}

impl Border {
    pub fn parse(e: Node, tc: &ThemeColours) -> Border {
        let style = val(e).map_or(BorderStyle::None, super::line_style::border_style);
        // Eighths of a point: a fraction of 1 is 1; 0 or none is a hairline that takes no space.
        let sz = match num(attr(e, "sz")) {
            Some(v) if v > 0.0 && v < 1.0 => 1.0,
            Some(v) => v.clamp(0.0, MAX_MEASURE * 8.0),
            None => 0.0,
        };
        Border {
            width: sz / 8.0,
            space: num(attr(e, "space")).unwrap_or(0.0),
            shadow: matches!(attr(e, "shadow"), Some("1" | "true" | "on")),
            color: tc.colour(e, "color", "themeColor", "themeTint", "themeShade"),
            color_theme: theme_ref(e, "themeColor", "themeTint", "themeShade"),
            style,
        }
    }

    pub fn visible(&self) -> bool {
        !matches!(self.style, BorderStyle::None | BorderStyle::Nil)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tab {
    pub pos: f64,
    pub kind: TabKind,
    pub leader: Leader,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FramePr {
    pub w: Option<f64>,
    pub h: Option<f64>,
    pub h_rule: Option<String>,
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub x_align: Option<String>,
    pub y_align: Option<String>,
    pub h_anchor: Option<String>,
    pub v_anchor: Option<String>,
    pub wrap: Option<String>,
    pub h_space: f64,
    pub v_space: f64,
    pub drop_cap: Option<String>,
    pub lines: Option<i64>,
    pub key: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ParaProps {
    pub style: Option<String>,
    pub jc: Option<Jc>,
    pub ind_left: Option<f64>,
    pub ind_right: Option<f64>,
    /// First-line offset from `ind_left`: + firstLine, − hanging.
    pub ind_first: Option<f64>,
    pub space_before: Option<f64>,
    pub space_after: Option<f64>,
    pub before_auto: Option<bool>,
    pub after_auto: Option<bool>,
    /// `w:spacing/@w:line` as written: 240ths of a line (auto) or twips.
    pub line: Option<f64>,
    pub line_rule: Option<LineRule>,
    pub contextual: Option<bool>,
    pub snap_to_grid: Option<bool>,
    pub keep_next: Option<bool>,
    pub keep_lines: Option<bool>,
    pub page_break_before: Option<bool>,
    pub widow_control: Option<bool>,
    pub suppress_auto_hyphens: Option<bool>,
    pub kinsoku: Option<bool>,
    pub overflow_punct: Option<bool>,
    pub auto_space_de: Option<bool>,
    pub auto_space_dn: Option<bool>,
    pub top_line_punct: Option<bool>,
    pub word_wrap: Option<bool>,
    pub tight_wrap: Option<u8>,
    pub suppress_overlap: Option<bool>,
    pub suppress_line_numbers: Option<bool>,
    pub mirror_indents: Option<bool>,
    pub bidi: Option<bool>,
    /// `w:divId`: the HTML division (`webSettings.xml`) the paragraph is in.
    pub div_id: Option<i64>,
    /// Tab stops this level sets and clears, in order.
    pub tabs: Vec<Tab>,
    pub num_id: Option<i64>,
    pub ilvl: Option<i64>,
    pub border_top: Option<Border>,
    pub border_bottom: Option<Border>,
    pub border_left: Option<Border>,
    pub border_right: Option<Border>,
    pub border_between: Option<Border>,
    pub shading: Option<Option<u32>>,
    /// The `w:shd` as written.
    pub shd: Option<Shd>,
    pub frame: Option<FramePr>,
    pub text_flow: Option<u8>,
    pub text_alignment: Option<super::paragraph::TextAlignment>,
    pub outline_lvl: Option<i64>,
    /// `w15:collapsed`: the heading opens collapsed.
    pub collapsed: Option<bool>,
    pub mark_size: Option<f64>,
}

impl ParaProps {
    pub fn parse(ppr: Option<Node>, tc: &ThemeColours) -> ParaProps {
        let mut p = ParaProps::default();
        let Some(ppr) = ppr else { return p };
        if let Some(c) = ppr.children().find(|c| is(*c, W15, "collapsed")) {
            p.collapsed = Some(!matches!(c.attribute((W15, "val")), Some("0" | "false" | "off")));
        }
        for e in ppr.children().filter(|c| c.is_element() && c.tag_name().namespace() == Some(W)) {
            match e.tag_name().name() {
                "pStyle" => p.style = val(e).map(str::to_string),
                "jc" => {
                    p.jc = Some(match val(e) {
                        Some("center") => Jc::Center,
                        Some("right" | "end") => Jc::Right,
                        Some("both" | "lowKashida" | "mediumKashida" | "highKashida" | "thaiDistribute") => Jc::Both,
                        Some("distribute") => Jc::Distribute,
                        _ => Jc::Left,
                    })
                }
                "ind" => {
                    p.ind_left = attr_twips(e, "left").or_else(|| attr_twips(e, "start"));
                    p.ind_right = attr_twips(e, "right").or_else(|| attr_twips(e, "end"));
                    p.ind_first = attr_twips(e, "hanging").map(|h| -h).or_else(|| attr_twips(e, "firstLine"));
                }
                "spacing" => {
                    p.space_before = attr_twips(e, "before");
                    p.space_after = attr_twips(e, "after");
                    p.before_auto = attr(e, "beforeAutospacing").map(|v| on_off_str(Some(v)));
                    p.after_auto = attr(e, "afterAutospacing").map(|v| on_off_str(Some(v)));
                    p.line = num(attr(e, "line"));
                    p.line_rule = attr(e, "lineRule").map(|r| match r {
                        "exact" => LineRule::Exact,
                        "atLeast" => LineRule::AtLeast,
                        _ => LineRule::Auto,
                    });
                }
                "contextualSpacing" => p.contextual = Some(on_off(e)),
                "snapToGrid" => p.snap_to_grid = Some(on_off(e)),
                "keepNext" => p.keep_next = Some(on_off(e)),
                "keepLines" => p.keep_lines = Some(on_off(e)),
                "pageBreakBefore" => p.page_break_before = Some(on_off(e)),
                "widowControl" => p.widow_control = Some(on_off(e)),
                "suppressAutoHyphens" => p.suppress_auto_hyphens = Some(on_off(e)),
                "kinsoku" => p.kinsoku = Some(on_off(e)),
                "overflowPunct" => p.overflow_punct = Some(on_off(e)),
                "autoSpaceDE" => p.auto_space_de = Some(on_off(e)),
                "autoSpaceDN" => p.auto_space_dn = Some(on_off(e)),
                "topLinePunct" => p.top_line_punct = Some(on_off(e)),
                "wordWrap" => p.word_wrap = Some(on_off(e)),
                "suppressOverlap" => p.suppress_overlap = Some(on_off(e)),
                "textboxTightWrap" => {
                    p.tight_wrap = Some(match val(e) {
                        Some("allLines") => 1,
                        Some("firstAndLastLine") => 2,
                        Some("firstLineOnly") => 3,
                        Some("lastLineOnly") => 4,
                        _ => 0,
                    })
                }
                "divId" => p.div_id = int(val(e)),
                "suppressLineNumbers" => p.suppress_line_numbers = Some(on_off(e)),
                "mirrorIndents" => p.mirror_indents = Some(on_off(e)),
                "bidi" => p.bidi = Some(on_off(e)),
                "tabs" => {
                    for t in children(e, "tab") {
                        let Some(pos) = attr_twips(t, "pos") else { continue };
                        p.tabs.push(Tab {
                            pos,
                            kind: match val(t) {
                                Some("center") => TabKind::Center,
                                Some("right" | "end") => TabKind::Right,
                                Some("decimal") => TabKind::Decimal,
                                Some("bar") => TabKind::Bar,
                                Some("num") => TabKind::Num,
                                Some("clear") => TabKind::Clear,
                                _ => TabKind::Left,
                            },
                            leader: match attr(t, "leader") {
                                Some("dot") => Leader::Dot,
                                Some("hyphen") => Leader::Hyphen,
                                Some("underscore") => Leader::Underscore,
                                Some("heavy") => Leader::Heavy,
                                Some("middleDot") => Leader::MiddleDot,
                                _ => Leader::None,
                            },
                        });
                    }
                }
                "numPr" => {
                    p.num_id = child(e, "numId").and_then(|n| int(val(n)));
                    p.ilvl = child(e, "ilvl").and_then(|n| int(val(n)));
                }
                "pBdr" => {
                    let b = |name| child(e, name).map(|n| Border::parse(n, tc));
                    p.border_top = b("top");
                    p.border_bottom = b("bottom");
                    p.border_left = b("left").or_else(|| b("start"));
                    p.border_right = b("right").or_else(|| b("end"));
                    p.border_between = b("between");
                }
                "shd" => {
                    p.shading = Some(shading_fill(e, tc));
                    p.shd = Some(Shd::parse(e, tc));
                }
                "framePr" => {
                    let a = |n| attr(e, n).map(str::to_string);
                    let mut key: Vec<String> = e.attributes().map(|a| format!("{}={}", a.name(), a.value())).collect();
                    key.sort();
                    p.frame = Some(FramePr {
                        w: attr_twips(e, "w"),
                        h: attr_twips(e, "h"),
                        h_rule: a("hRule"),
                        x: attr_twips(e, "x"),
                        y: attr_twips(e, "y"),
                        x_align: a("xAlign"),
                        y_align: a("yAlign"),
                        h_anchor: a("hAnchor"),
                        v_anchor: a("vAnchor"),
                        wrap: a("wrap"),
                        h_space: attr_twips(e, "hSpace").unwrap_or(0.0),
                        v_space: attr_twips(e, "vSpace").unwrap_or(0.0),
                        drop_cap: a("dropCap"),
                        lines: int(attr(e, "lines")),
                        key: key.join(" "),
                    });
                }
                "textAlignment" => p.text_alignment = val(e).map(super::paragraph::text_alignment),
                "textDirection" => p.text_flow = val(e).map(super::model::text_flow),
                "outlineLvl" => p.outline_lvl = int(val(e)),
                "rPr" => p.mark_size = child(e, "sz").and_then(|s| half_points(val(s))),
                _ => {}
            }
        }
        p
    }

    /// Overrides every attribute `o` sets; tab stops accumulate (a `clear` removes the inherited stop there).
    pub fn merge(&mut self, o: &ParaProps) {
        macro_rules! take {
            ($($f:ident),*) => { $( if o.$f.is_some() { self.$f = o.$f.clone(); } )* };
        }
        take!(
            style,
            jc,
            ind_left,
            ind_right,
            ind_first,
            space_before,
            space_after,
            before_auto,
            after_auto,
            line,
            line_rule,
            contextual,
            snap_to_grid,
            keep_next,
            keep_lines,
            page_break_before,
            widow_control,
            suppress_auto_hyphens,
            kinsoku,
            overflow_punct,
            auto_space_de,
            auto_space_dn,
            top_line_punct,
            word_wrap,
            tight_wrap,
            suppress_overlap,
            suppress_line_numbers,
            mirror_indents,
            bidi,
            div_id,
            num_id,
            ilvl,
            border_top,
            border_bottom,
            border_left,
            border_right,
            border_between,
            shading,
            shd,
            frame,
            text_flow,
            text_alignment,
            outline_lvl,
            collapsed
        );
        self.tabs.extend(o.tabs.iter().copied());
    }

    pub fn merge_indent(&mut self, o: &ParaProps) {
        if o.ind_left.is_some() {
            self.ind_left = o.ind_left;
        }
        if o.ind_right.is_some() {
            self.ind_right = o.ind_right;
        }
        if o.ind_first.is_some() {
            self.ind_first = o.ind_first;
        }
    }

    /// The effective tab stops: later levels replace stops at the same position; `clear` removes them.
    pub fn resolved_tabs(&self) -> Vec<Tab> {
        let mut out: Vec<Tab> = Vec::new();
        for t in &self.tabs {
            out.retain(|x| (x.pos - t.pos).abs() >= 0.025);
            if t.kind != TabKind::Clear {
                out.push(*t);
            }
        }
        out.sort_by(|a, b| a.pos.total_cmp(&b.pos));
        out
    }
}

/// A table width (`w:tblW`, `w:tcW`, `w:tblInd`, ...), its measures clamped.
pub fn width(e: Node) -> Width {
    match width_raw(e) {
        Width::Dxa(v) => Width::Dxa(v.clamp(-MAX_MEASURE, MAX_MEASURE)),
        Width::Pct(f) => Width::Pct(f.clamp(-MAX_PCT, MAX_PCT)),
        w => w,
    }
}

fn width_raw(e: Node) -> Width {
    let w = attr(e, "w");
    match attr(e, "type") {
        Some("nil") => Width::Nil,
        Some("pct") => {
            // 50ths of a percent, or "50%".
            let s = w.unwrap_or("0");
            if let Some(p) = s.strip_suffix('%') {
                Width::Pct(num(Some(p)).unwrap_or(0.0) / 100.0)
            } else {
                Width::Pct(num(Some(s)).unwrap_or(0.0) / 5000.0)
            }
        }
        Some("auto") => Width::Auto,
        _ => match twips(w) {
            Some(v) => Width::Dxa(v),
            None => Width::Auto,
        },
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Margins {
    pub top: Option<f64>,
    pub left: Option<f64>,
    pub bottom: Option<f64>,
    pub right: Option<f64>,
}

impl Margins {
    fn parse(e: Node) -> Margins {
        let m = |n| child(e, n).map(width).map(|w| if let Width::Dxa(v) = w { v } else { 0.0 });
        Margins { top: m("top"), left: m("left").or_else(|| m("start")), bottom: m("bottom"), right: m("right").or_else(|| m("end")) }
    }

    pub fn merge(&mut self, o: &Margins) {
        if o.top.is_some() {
            self.top = o.top;
        }
        if o.left.is_some() {
            self.left = o.left;
        }
        if o.bottom.is_some() {
            self.bottom = o.bottom;
        }
        if o.right.is_some() {
            self.right = o.right;
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Borders {
    pub top: Option<Border>,
    pub left: Option<Border>,
    pub bottom: Option<Border>,
    pub right: Option<Border>,
    pub inside_h: Option<Border>,
    pub inside_v: Option<Border>,
    /// A cell's diagonals: top left to bottom right, top right to bottom left.
    pub tl2br: Option<Border>,
    pub tr2bl: Option<Border>,
}

impl Borders {
    fn parse(e: Node, tc: &ThemeColours) -> Borders {
        let b = |n| child(e, n).map(|n| Border::parse(n, tc));
        Borders {
            top: b("top"),
            left: b("left").or_else(|| b("start")),
            bottom: b("bottom"),
            right: b("right").or_else(|| b("end")),
            inside_h: b("insideH"),
            inside_v: b("insideV"),
            tl2br: b("tl2br"),
            tr2bl: b("tr2bl"),
        }
    }

    pub fn merge(&mut self, o: &Borders) {
        macro_rules! take {
            ($($f:ident),*) => { $( if o.$f.is_some() { self.$f = o.$f.clone(); } )* };
        }
        take!(top, left, bottom, right, inside_h, inside_v, tl2br, tr2bl);
    }
}

/// `w:tblLook`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Look {
    pub first_row: bool,
    pub last_row: bool,
    pub first_col: bool,
    pub last_col: bool,
    pub no_h_band: bool,
    pub no_v_band: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TblProps {
    pub style: Option<String>,
    pub width: Option<Width>,
    pub jc: Option<Jc>,
    /// Dxa or pct.
    pub ind: Option<Width>,
    pub borders: Borders,
    pub margins: Margins,
    pub fixed: Option<bool>,
    pub look: Option<Look>,
    pub cell_spacing: Option<f64>,
    pub row_band: Option<i64>,
    pub col_band: Option<i64>,
    pub floating: Option<TablePos>,
    pub shading: Option<Option<u32>>,
    /// The `w:shd` as written.
    pub shd: Option<Shd>,
    pub bidi_visual: Option<bool>,
}

/// What `w:tblpVertAnchor` measures a floating table's `tblpY` from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableAnchor {
    Text,
    Margin,
    Page,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TablePos {
    pub v_anchor: TableAnchor,
    pub y: f64,
    /// `tblpX`, or the `tblpXSpec` alignment (left/center/right) when given.
    pub x: f64,
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

impl TblProps {
    pub fn parse(e: Option<Node>, tc: &ThemeColours) -> TblProps {
        let mut p = TblProps::default();
        let Some(e) = e else { return p };
        for c in e.children().filter(|c| c.is_element() && c.tag_name().namespace() == Some(W)) {
            match c.tag_name().name() {
                "tblStyle" => p.style = val(c).map(str::to_string),
                "tblW" => p.width = Some(width(c)),
                "bidiVisual" => p.bidi_visual = Some(on_off(c)),
                "jc" => {
                    p.jc = Some(match val(c) {
                        Some("center") => Jc::Center,
                        Some("right" | "end") => Jc::Right,
                        _ => Jc::Left,
                    })
                }
                "tblInd" => p.ind = Some(width(c)),
                "tblBorders" => p.borders = Borders::parse(c, tc),
                "tblCellMar" => p.margins = Margins::parse(c),
                "tblLayout" => p.fixed = Some(attr(c, "type") == Some("fixed")),
                "tblCellSpacing" => p.cell_spacing = Some(if let Width::Dxa(v) = width(c) { v } else { 0.0 }),
                "tblStyleRowBandSize" => p.row_band = int(val(c)),
                "tblStyleColBandSize" => p.col_band = int(val(c)),
                "tblpPr" => {
                    p.floating = Some(TablePos {
                        v_anchor: match attr(c, "vertAnchor") {
                            Some("page") => TableAnchor::Page,
                            Some("text") => TableAnchor::Text,
                            _ => TableAnchor::Margin,
                        },
                        y: match (attr(c, "vertAnchor"), attr(c, "tblpYSpec")) {
                            (Some("text"), Some(s)) => {
                                (match s {
                                    "center" => -8.0,
                                    "bottom" => -12.0,
                                    "inside" => -16.0,
                                    "outside" => -20.0,
                                    _ => -4.0,
                                }) / 20.0
                            }
                            _ => attr_twips(c, "tblpY").unwrap_or(0.0),
                        },
                        x: attr_twips(c, "tblpX").unwrap_or(0.0),
                        x_spec: match attr(c, "tblpXSpec") {
                            Some("center") => Some(Jc::Center),
                            Some("right" | "outside") => Some(Jc::Right),
                            Some(_) => Some(Jc::Left),
                            None => None,
                        },
                        h_page: attr(c, "horzAnchor") == Some("page"),
                        h_text: !matches!(attr(c, "horzAnchor"), Some("margin" | "page")),
                        y_spec: match attr(c, "tblpYSpec") {
                            Some("center") => Some(Jc::Center),
                            Some("bottom" | "outside") => Some(Jc::Right),
                            Some(_) => Some(Jc::Left),
                            None => None,
                        },
                        dist: ["topFromText", "bottomFromText", "leftFromText", "rightFromText"].map(|a| attr_twips(c, a).unwrap_or(0.0).max(0.0)),
                        never_overlap: false,
                    })
                }
                // tblOverlap follows tblpPr in tblPr.
                "tblOverlap" => {
                    if let Some(f) = &mut p.floating {
                        f.never_overlap = val(c) == Some("never");
                    }
                }
                "shd" => {
                    p.shading = Some(shading_fill(c, tc));
                    p.shd = Some(Shd::parse(c, tc));
                }
                "tblLook" => {
                    let bit = |name: &str, mask: i64| match attr(c, name) {
                        Some(v) => on_off_str(Some(v)),
                        None => i64::from_str_radix(val(c).unwrap_or("0"), 16).unwrap_or(0) & mask != 0,
                    };
                    p.look = Some(Look {
                        first_row: bit("firstRow", 0x20),
                        last_row: bit("lastRow", 0x40),
                        first_col: bit("firstColumn", 0x80),
                        last_col: bit("lastColumn", 0x100),
                        no_h_band: bit("noHBand", 0x200),
                        no_v_band: bit("noVBand", 0x400),
                    });
                }
                _ => {}
            }
        }
        p
    }

    pub fn merge(&mut self, o: &TblProps) {
        macro_rules! take {
            ($($f:ident),*) => { $( if o.$f.is_some() { self.$f = o.$f.clone(); } )* };
        }
        take!(style, width, jc, ind, fixed, look, cell_spacing, row_band, col_band, floating, shading, shd, bidi_visual);
        self.borders.merge(&o.borders);
        self.margins.merge(&o.margins);
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TcProps {
    pub width: Option<Width>,
    pub grid_span: Option<i64>,
    /// `Some(true)` restart, `Some(false)` continue.
    pub v_merge: Option<bool>,
    pub h_merge: Option<bool>,
    pub borders: Borders,
    pub shading: Option<Option<u32>>,
    /// The `w:shd` as written.
    pub shd: Option<Shd>,
    pub margins: Margins,
    pub v_align: Option<super::VerticalAlignment>,
    pub text_direction: Option<String>,
    pub no_wrap: Option<bool>,
    pub fit_text: Option<bool>,
}

impl TcProps {
    pub fn parse(e: Option<Node>, tc: &ThemeColours) -> TcProps {
        let mut p = TcProps::default();
        let Some(e) = e else { return p };
        for c in e.children().filter(|c| c.is_element() && c.tag_name().namespace() == Some(W)) {
            match c.tag_name().name() {
                "tcW" => p.width = Some(width(c)),
                "gridSpan" => p.grid_span = int(val(c)),
                "vMerge" => p.v_merge = Some(val(c) == Some("restart")),
                "hMerge" => p.h_merge = Some(val(c) == Some("restart")),
                "tcBorders" => p.borders = Borders::parse(c, tc),
                "shd" => {
                    p.shading = Some(shading_fill(c, tc));
                    p.shd = Some(Shd::parse(c, tc));
                }
                "tcMar" => p.margins = Margins::parse(c),
                "vAlign" => p.v_align = val(c).map(super::section::cell_v_align),
                "textDirection" => p.text_direction = val(c).map(str::to_string),
                "noWrap" => p.no_wrap = Some(on_off(c)),
                "tcFitText" => p.fit_text = Some(on_off(c)),
                _ => {}
            }
        }
        p
    }

    pub fn merge(&mut self, o: &TcProps) {
        macro_rules! take {
            ($($f:ident),*) => { $( if o.$f.is_some() { self.$f = o.$f.clone(); } )* };
        }
        take!(width, grid_span, v_merge, h_merge, shading, shd, v_align, text_direction, no_wrap, fit_text);
        self.borders.merge(&o.borders);
        self.margins.merge(&o.margins);
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TrProps {
    /// Height and rule (at least by default).
    pub height: Option<(f64, super::HeightRule)>,
    pub cant_split: Option<bool>,
    pub header: Option<bool>,
    pub jc: Option<Jc>,
    pub grid_before: Option<i64>,
    pub grid_after: Option<i64>,
    /// `w:wBefore` / `w:wAfter`: the width of the grid columns a row skips, dxa or pct.
    pub w_before: Option<Width>,
    pub w_after: Option<Width>,
    pub cell_spacing: Option<f64>,
}

impl TrProps {
    pub fn parse(e: Option<Node>) -> TrProps {
        let mut p = TrProps::default();
        let Some(e) = e else { return p };
        for c in e.children().filter(|c| c.is_element() && c.tag_name().namespace() == Some(W)) {
            match c.tag_name().name() {
                "trHeight" => {
                    p.height = Some((twips(val(c)).unwrap_or(0.0), attr(c, "hRule").map_or(super::HeightRule::AtLeast, super::section::height_rule)))
                }
                "cantSplit" => p.cant_split = Some(on_off(c)),
                "tblHeader" => p.header = Some(on_off(c)),
                "jc" => {
                    p.jc = Some(match val(c) {
                        Some("center") => Jc::Center,
                        Some("right" | "end") => Jc::Right,
                        _ => Jc::Left,
                    })
                }
                "gridBefore" => p.grid_before = int(val(c)),
                "gridAfter" => p.grid_after = int(val(c)),
                "wBefore" => p.w_before = Some(width(c).sized()),
                "wAfter" => p.w_after = Some(width(c).sized()),
                "tblCellSpacing" => p.cell_spacing = Some(if let Width::Dxa(v) = width(c) { v } else { 0.0 }),
                _ => {}
            }
        }
        p
    }

    pub fn merge(&mut self, o: &TrProps) {
        macro_rules! take {
            ($($f:ident),*) => { $( if o.$f.is_some() { self.$f = o.$f.clone(); } )* };
        }
        take!(height, cant_split, header, jc, grid_before, grid_after, w_before, w_after, cell_spacing);
    }
}

#[cfg(test)]
mod tests {
    use super::{RunProps, toggle};

    #[test]
    fn theme_colours_win_over_rgb_with_their_tint_or_shade() {
        let rpr = |x: &str| {
            let xml = format!(r#"<w:rPr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">{x}</w:rPr>"#);
            RunProps::parse(Some(roxmltree::Document::parse(&xml).unwrap().root_element()), &super::ThemeColours::default())
        };
        assert_eq!(rpr(r#"<w:color w:val="1F3864" w:themeColor="accent1" w:themeShade="80"/>"#).color, Some(Some(0x0B3041)));
        assert_eq!(rpr(r#"<w:color w:val="1F3864" w:themeColor="accent1"/>"#).color, Some(Some(0x156082)));
        assert_eq!(rpr(r#"<w:color w:val="1F3864" w:themeColor="nonsense"/>"#).color, Some(Some(0x1F3864)));
        assert_eq!(rpr(r#"<w:color w:val="auto" w:themeColor="text1"/>"#).color, Some(Some(0x000000)));
        // A tint moves toward white: background1 (FFFFFF) stays white, dark1 at tint 80 is a mid grey.
        assert_eq!(rpr(r#"<w:color w:themeColor="dark1" w:themeTint="80"/>"#).color, Some(Some(0x7F7F7F)));
        let shd = rpr(r#"<w:shd w:val="clear" w:color="auto" w:fill="FFFFFF" w:themeFill="accent2"/>"#);
        assert_eq!(shd.shading, Some(Some(0xE97132)));
    }

    #[test]
    fn shading_shows_its_fill_a_blend_or_nothing_and_stripes_their_hatch() {
        let shd = |v: &str| super::Shd {
            pattern: super::super::line_style::shading_pattern(v),
            color: Some(0xFF0000),
            fill: Some(0x0000FF),
            color_theme: None,
            fill_theme: None,
        };
        assert_eq!(shd("nil").flat(), None, "nil draws nothing, whatever its fill");
        assert_eq!(shd("clear").flat(), Some(0x0000FF));
        assert_eq!(shd("solid").flat(), Some(0xFF0000));
        assert_eq!(shd("pct50").flat(), Some(0x7F0080));
        assert_eq!(shd("horzStripe").flat(), Some(0x0000FF), "a stripe's fill under it");
        assert_eq!(shd("horzStripe").hatch(), Some(super::Hatch { stripe: super::Stripe::Horz, colour: 0xFF0000 }));
        assert_eq!(shd("pct50").hatch(), None);
        // Hostile percentages: none past 100 (each channel would spill into the next), and none that isn't a number.
        for v in ["pct500", "pct4294967295", "pct-5", "pct", "pctx"] {
            assert_eq!(shd(v).flat(), None, "{v}");
        }
        assert_eq!(shd("bogus").flat(), Some(0x0000FF), "an unknown pattern shows its fill");
    }

    #[test]
    fn the_nearest_style_in_a_chain_decides_a_toggle() {
        let rpr = |x: &str| {
            let xml = format!(r#"<w:rPr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">{x}</w:rPr>"#);
            RunProps::parse(Some(roxmltree::Document::parse(&xml).unwrap().root_element()), &super::ThemeColours::default())
        };
        let (on, off, none) = (rpr("<w:b/>"), rpr(r#"<w:b w:val="0"/>"#), rpr(""));
        let bold = |chain: &[&RunProps]| RunProps::fold_chain(chain.iter().copied()).toggle(toggle::B);
        assert_eq!((bold(&[&on, &off]), bold(&[&on, &off, &none]), bold(&[&on, &off, &on])), (false, false, true));
        let mut para = RunProps::fold_chain([&on].into_iter());
        para.merge_level(&RunProps::fold_chain([&on, &off].into_iter()));
        assert!(para.toggle(toggle::B));
        para.merge_level(&RunProps::fold_chain([&on].into_iter()));
        assert!(!para.toggle(toggle::B));
    }

    fn w14_rpr(x: &str) -> RunProps {
        let xml = format!(
            r#"<w:rPr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml">{x}</w:rPr>"#
        );
        RunProps::parse(Some(roxmltree::Document::parse(&xml).unwrap().root_element()), &super::ThemeColours::default())
    }

    #[test]
    fn text_effects_read_with_qualified_attributes_and_merge_one_at_a_time() {
        use crate::drawing::colour::{ColourBase, ColourMod, SchemeSlot};
        use crate::drawing::paint::{Fill, GradientKind};
        use crate::drawing::units::{Angle, Emu, Pct};
        let style = w14_rpr(
            r#"<w14:glow w14:rad="101600"><w14:schemeClr w14:val="accent1"><w14:alpha w14:val="40000"/></w14:schemeClr></w14:glow><w14:textFill><w14:noFill/></w14:textFill>"#,
        );
        let direct = w14_rpr(
            r#"<w14:textFill><w14:gradFill><w14:gsLst><w14:gs w14:pos="0"><w14:srgbClr w14:val="C00000"/></w14:gs><w14:gs w14:pos="100000"><w14:srgbClr w14:val="2E75B6"/></w14:gs></w14:gsLst><w14:lin w14:ang="5400000" w14:scaled="0"/></w14:gradFill></w14:textFill><w14:textOutline w14:w="9525" w14:cap="rnd"><w14:solidFill><w14:srgbClr w14:val="FF0000"/></w14:solidFill><w14:prstDash w14:val="dash"/><w14:bevel/></w14:textOutline><w14:reflection w14:stA="52000" w14:endPos="35000" w14:sy="-100000" w14:algn="bl"/>"#,
        );
        let e = RunProps::fold_chain([&style, &direct].into_iter()).effects.expect("effects");
        let glow = e.glow.as_ref().expect("the style's glow");
        assert_eq!(
            (glow.radius, &glow.colour.base, &glow.colour.mods[..]),
            (Emu(101600), &ColourBase::Scheme(SchemeSlot::Accent(1)), &[ColourMod::Alpha(Pct(40000))][..])
        );
        let Some(Fill::Gradient(g)) = &e.fill else { panic!("the direct gradient over the style's no fill: {:?}", e.fill) };
        assert_eq!((g.stops.len(), g.kind), (2, GradientKind::Linear { angle: Angle(5_400_000), scaled: false }));
        let o = e.outline.as_ref().expect("an outline");
        assert_eq!((o.width, o.cap, o.join), (Some(Emu(9525)), Some(crate::drawing::paint::Cap::Round), Some(crate::drawing::paint::Join::Bevel)));
        let r = e.reflection.as_ref().expect("a reflection");
        // What it leaves out is Word 2010's default (MS-DOCX CT_Reflection): no fade turn, no horizontal scale.
        assert_eq!((r.start_alpha, r.end_pos, r.sy, r.fade_dir, r.sx), (Pct(52000), Pct(35000), Pct(-100000), Angle(0), Pct(0)));
        assert!(w14_rpr(r#"<w:b/>"#).effects.is_none(), "a run without effects has none");
    }

    #[test]
    fn hostile_text_effects_are_bounded() {
        let stops: String = (0..10_000).map(|i| format!(r#"<w14:gs w14:pos="{i}"><w14:srgbClr w14:val="FF0000"/></w14:gs>"#)).collect();
        let mods: String = (0..10_000).map(|_| r#"<w14:lumMod w14:val="99000"/>"#).collect();
        let r = w14_rpr(&format!(
            r#"<w14:textFill><w14:gradFill><w14:gsLst>{stops}</w14:gsLst></w14:gradFill></w14:textFill><w14:glow w14:rad="99999999999"><w14:srgbClr w14:val="00FF00">{mods}</w14:srgbClr></w14:glow><w14:textOutline><w14:custDash><w14:ds w14:d="1" w14:sp="1"/></w14:custDash></w14:textOutline>"#
        ));
        let e = r.effects.expect("effects");
        let Some(crate::drawing::paint::Fill::Gradient(g)) = &e.fill else { panic!("a gradient") };
        assert_eq!(g.stops.len(), crate::drawing::paint::MAX_W14_STOPS);
        assert_eq!(e.glow.as_ref().expect("a glow").colour.mods.len(), crate::drawing::colour::MAX_W14_MODS);
        assert_eq!(e.outline.as_ref().expect("an outline").dash, None, "no custom dash in the w14 schema");
        // The drawn glow is bounded as a shape's is.
        let (t, m) = (crate::drawing::theme::Theme::default(), crate::drawing::theme::ColourMap::default());
        let list = crate::drawing::paint::EffectList { glow: e.glow.clone(), ..Default::default() };
        let fx = crate::drawing::paint::resolve_effects(Some(&list), None, crate::drawing::paint::Palette { theme: &t, map: &m });
        assert_eq!(fx.glow.map(|g| g.0), Some(200.0));
    }
}
