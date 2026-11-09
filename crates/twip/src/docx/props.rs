//! Formatting properties

use roxmltree::Node;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VertAlign {
    Baseline,
    Superscript,
    Subscript,
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
    /// Toggles this level sets (`toggles_set`) and their values.
    pub toggles_set: u16,
    pub toggles: u16,
    pub dstrike: Option<bool>,
    pub underline: Option<String>,
    /// `Some(None)` = auto.
    pub color: Option<Option<u32>>,
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
    /// `w:em`: the emphasis mark ("dot", "comma", "circle", "underDot"; "none").
    pub em: Option<String>,
    pub lang: Option<String>,
    pub style: Option<String>,
    pub rtl: Option<bool>,
    /// `w:fitText`: the width to fit the text to (points) and its region id.
    pub fit_text: Option<(f64, i64)>,
}

fn font_ref(n: Node, name: &str) -> Option<FontRef> {
    let theme = attr(n, &format!("{name}Theme")).or_else(|| if name == "cs" { attr(n, "cstheme") } else { None });
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
                "dstrike" => p.dstrike = Some(on_off(e)),
                "u" => p.underline = Some(val(e).unwrap_or("single").to_string()),
                "color" => p.color = Some(tc.colour(e, "val", "themeColor", "themeTint", "themeShade")),
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
                "em" => p.em = Some(val(e).unwrap_or("none").to_string()),
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
                "lang" => p.lang = val(e).map(str::to_string),
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
            ascii, h_ansi, east_asia, cs, hint, size, dstrike, underline, color, highlight, shading, shd, border, vert_align, position, spacing,
            scale, kern, ligatures, cntxt_alts, lang, style, rtl, em, fit_text
        );
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

/// `w14:val` or `w:val`.
fn val_any<'a>(n: Node<'a, '_>) -> Option<&'a str> {
    n.attribute((W14, "val")).or_else(|| val(n))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ThemeColours([u32; 16]);

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
        use crate::drawing::colour::SchemeSlot::*;
        let slots = [Dk1, Lt1, Dk2, Lt2, Accent(1), Accent(2), Accent(3), Accent(4), Accent(5), Accent(6), Hlink, FolHlink, Bg1, Tx1, Bg2, Tx2];
        ThemeColours(slots.map(|s| {
            let c = theme.colour(map.map(s));
            (u32::from(c.r) << 16) | (u32::from(c.g) << 8) | u32::from(c.b)
        }))
    }

    pub fn colour(&self, e: Node, hex: &str, theme: &str, tint: &str, shade: &str) -> Option<u32> {
        let Some(c) = attr(e, theme).and_then(|t| THEME_COLOUR_NAMES.iter().position(|n| *n == t)).map(|i| self.0[i]) else {
            return color(attr(e, hex));
        };
        let byte = |a: &str| attr(e, a).and_then(|v| u8::from_str_radix(v, 16).ok()).map(|v| f64::from(v) / 255.0);
        Some(match (byte(tint), byte(shade)) {
            (Some(t), _) => with_luminance(c, |l| l * t + (1.0 - t)),
            (None, Some(s)) => with_luminance(c, |l| l * s),
            (None, None) => c,
        })
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

/// `w:shd` → the visible fill (patterns `pctNN` blend the pattern colour over the fill).
/// A `w:shd` as written: its pattern (`w:val`), the pattern's color and the fill (none: automatic).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shd {
    pub pattern: String,
    pub color: Option<u32>,
    pub fill: Option<u32>,
}

impl Shd {
    pub fn parse(e: Node, tc: &ThemeColours) -> Shd {
        Shd {
            pattern: val(e).unwrap_or("clear").to_string(),
            color: tc.colour(e, "color", "themeColor", "themeTint", "themeShade"),
            fill: tc.colour(e, "fill", "themeFill", "themeFillTint", "themeFillShade"),
        }
    }
}

pub fn shading_fill(e: Node, tc: &ThemeColours) -> Option<u32> {
    let fill = tc.colour(e, "fill", "themeFill", "themeFillTint", "themeFillShade");
    let pattern = || tc.colour(e, "color", "themeColor", "themeTint", "themeShade");
    match val(e) {
        None | Some("clear" | "nil") => fill,
        Some("solid") => Some(pattern().unwrap_or(0)),
        Some(p) if p.starts_with("pct") => {
            let pct: u32 = p[3..].parse().ok()?;
            let fg = pattern().unwrap_or(0);
            let bg = fill.unwrap_or(0xFFFFFF);
            let mix = |s: u32| {
                let (a, b) = ((fg >> s) & 255, (bg >> s) & 255);
                ((b as i64 + (a as i64 - b as i64) * pct as i64 / 100) as u32) << s
            };
            Some(mix(16) | mix(8) | mix(0))
        }
        _ => fill,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Jc {
    Left,
    Center,
    Right,
    Both,
    Distribute,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineRule {
    Auto,
    Exact,
    AtLeast,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Border {
    /// ST_Border value ("single", "double", "none", ...).
    pub style: String,
    /// Line width in points (`w:sz` eighths).
    pub width: f64,
    pub space: f64,
    pub shadow: bool,
    pub color: Option<u32>,
}

impl Border {
    pub fn parse(e: Node, tc: &ThemeColours) -> Border {
        let style = val(e).unwrap_or("none").to_string();
        Border {
            width: num(attr(e, "sz")).unwrap_or(4.0) / 8.0,
            space: num(attr(e, "space")).unwrap_or(0.0),
            shadow: matches!(attr(e, "shadow"), Some("1" | "true" | "on")),
            color: tc.colour(e, "color", "themeColor", "themeTint", "themeShade"),
            style,
        }
    }

    pub fn visible(&self) -> bool {
        !matches!(self.style.as_str(), "none" | "nil") && self.width > 0.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TabKind {
    Left,
    Center,
    Right,
    Decimal,
    Bar,
    Num,
    Clear,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Leader {
    None,
    Dot,
    Hyphen,
    Underscore,
    MiddleDot,
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
    pub text_alignment: Option<String>,
    pub outline_lvl: Option<i64>,
    pub mark_size: Option<f64>,
}

impl ParaProps {
    pub fn parse(ppr: Option<Node>, tc: &ThemeColours) -> ParaProps {
        let mut p = ParaProps::default();
        let Some(ppr) = ppr else { return p };
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
                                Some("underscore" | "heavy") => Leader::Underscore,
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
                "textAlignment" => p.text_alignment = val(e).map(str::to_string),
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
            text_alignment,
            outline_lvl
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

/// A table width (`w:tblW`, `w:tcW`, `w:tblInd`, ...).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Width {
    Auto,
    Nil,
    /// Points.
    Dxa(f64),
    /// Fraction (1.0 = 100%).
    Pct(f64),
}

impl Width {
    pub fn parse(e: Node) -> Width {
        let w = attr(e, "w");
        match attr(e, "type") {
            Some("nil") => Width::Nil,
            Some("pct") => {
                // 50ths of a percent, or "50%".
                let s = w.unwrap_or("0");
                if let Some(p) = s.strip_suffix('%') {
                    Width::Pct(p.parse::<f64>().unwrap_or(0.0) / 100.0)
                } else {
                    Width::Pct(s.parse::<f64>().unwrap_or(0.0) / 5000.0)
                }
            }
            Some("auto") => Width::Auto,
            _ => match twips(w) {
                Some(v) => Width::Dxa(v),
                None => Width::Auto,
            },
        }
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
        let m = |n| child(e, n).map(Width::parse).map(|w| if let Width::Dxa(v) = w { v } else { 0.0 });
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
        }
    }

    pub fn merge(&mut self, o: &Borders) {
        macro_rules! take {
            ($($f:ident),*) => { $( if o.$f.is_some() { self.$f = o.$f.clone(); } )* };
        }
        take!(top, left, bottom, right, inside_h, inside_v);
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
    pub ind: Option<f64>,
    pub borders: Borders,
    pub margins: Margins,
    pub fixed: Option<bool>,
    pub look: Option<Look>,
    pub cell_spacing: Option<f64>,
    pub row_band: Option<i64>,
    pub col_band: Option<i64>,
    pub floating: Option<TablePos>,
    pub shading: Option<Option<u32>>,
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

impl TblProps {
    pub fn parse(e: Option<Node>, tc: &ThemeColours) -> TblProps {
        let mut p = TblProps::default();
        let Some(e) = e else { return p };
        for c in e.children().filter(|c| c.is_element() && c.tag_name().namespace() == Some(W)) {
            match c.tag_name().name() {
                "tblStyle" => p.style = val(c).map(str::to_string),
                "tblW" => p.width = Some(Width::parse(c)),
                "bidiVisual" => p.bidi_visual = Some(on_off(c)),
                "jc" => {
                    p.jc = Some(match val(c) {
                        Some("center") => Jc::Center,
                        Some("right" | "end") => Jc::Right,
                        _ => Jc::Left,
                    })
                }
                "tblInd" => p.ind = Some(if let Width::Dxa(v) = Width::parse(c) { v } else { 0.0 }),
                "tblBorders" => p.borders = Borders::parse(c, tc),
                "tblCellMar" => p.margins = Margins::parse(c),
                "tblLayout" => p.fixed = Some(attr(c, "type") == Some("fixed")),
                "tblCellSpacing" => p.cell_spacing = Some(if let Width::Dxa(v) = Width::parse(c) { v } else { 0.0 }),
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
                "shd" => p.shading = Some(shading_fill(c, tc)),
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
        take!(style, width, jc, ind, fixed, look, cell_spacing, row_band, col_band, floating, shading, bidi_visual);
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
    pub v_align: Option<String>,
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
                "tcW" => p.width = Some(Width::parse(c)),
                "gridSpan" => p.grid_span = int(val(c)),
                "vMerge" => p.v_merge = Some(val(c) == Some("restart")),
                "hMerge" => p.h_merge = Some(val(c) == Some("restart")),
                "tcBorders" => p.borders = Borders::parse(c, tc),
                "shd" => {
                    p.shading = Some(shading_fill(c, tc));
                    p.shd = Some(Shd::parse(c, tc));
                }
                "tcMar" => p.margins = Margins::parse(c),
                "vAlign" => p.v_align = val(c).map(str::to_string),
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
    /// Height and rule ("atLeast" default, "exact", "auto").
    pub height: Option<(f64, String)>,
    pub cant_split: Option<bool>,
    pub header: Option<bool>,
    pub jc: Option<Jc>,
    pub grid_before: Option<i64>,
    pub grid_after: Option<i64>,
    /// `w:wBefore` / `w:wAfter`: the width of the grid columns a row skips (points, dxa only).
    pub w_before: Option<f64>,
    pub w_after: Option<f64>,
    pub cell_spacing: Option<f64>,
}

impl TrProps {
    pub fn parse(e: Option<Node>) -> TrProps {
        let mut p = TrProps::default();
        let Some(e) = e else { return p };
        for c in e.children().filter(|c| c.is_element() && c.tag_name().namespace() == Some(W)) {
            match c.tag_name().name() {
                "trHeight" => p.height = Some((twips(val(c)).unwrap_or(0.0), attr(c, "hRule").unwrap_or("atLeast").to_string())),
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
                "wBefore" => p.w_before = Some(if let Width::Dxa(v) = Width::parse(c) { v } else { 0.0 }),
                "wAfter" => p.w_after = Some(if let Width::Dxa(v) = Width::parse(c) { v } else { 0.0 }),
                "tblCellSpacing" => p.cell_spacing = Some(if let Width::Dxa(v) = Width::parse(c) { v } else { 0.0 }),
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
}
