//! Formatting properties as written at one level (docDefaults, a style, direct formatting). Every attribute is optional so
//! levels merge per attribute (STYLE-2). Lengths are points.

use roxmltree::Node;

use super::xml::*;

/// A font attribute pair (`w:ascii` / `w:asciiTheme`): the theme form wins within one element.
#[derive(Clone, Debug, PartialEq)]
pub enum FontRef {
    Name(String),
    Theme(String),
}

/// Toggle properties (STYLE-3), as bits.
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
    pub vert_align: Option<VertAlign>,
    pub position: Option<f64>,
    pub spacing: Option<f64>,
    pub scale: Option<f64>,
    /// `w:kern`: kern at or above this size (points); 0 = off.
    pub kern: Option<f64>,
    /// `w14:ligatures` bits: 1 standard, 2 contextual, 4 historical, 8 discretionary.
    pub ligatures: Option<u8>,
    /// `w:em`: the emphasis mark ("dot", "comma", "circle", "underDot"; "none").
    pub em: Option<String>,
    pub lang: Option<String>,
    pub style: Option<String>,
    pub rtl: Option<bool>,
}

fn font_ref(n: Node, name: &str) -> Option<FontRef> {
    let theme = attr(n, &format!("{name}Theme")).or_else(|| if name == "cs" { attr(n, "cstheme") } else { None });
    if let Some(t) = theme {
        return Some(FontRef::Theme(t.to_string()));
    }
    attr(n, name).filter(|s| !s.is_empty()).map(|s| FontRef::Name(s.to_string()))
}

impl RunProps {
    pub fn parse(rpr: Option<Node>) -> RunProps {
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
                "color" => p.color = Some(color(val(e))),
                "highlight" => {
                    p.highlight = Some(match val(e) {
                        Some("none") | None => None,
                        Some(v) => highlight(v),
                    })
                }
                "shd" => p.shading = Some(shading_fill(e)),
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
                _ => {}
            }
        }
        p
    }

    /// Overrides every attribute `o` sets; toggles are absolute (direct formatting, or within one style chain).
    pub fn merge(&mut self, o: &RunProps) {
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
            ascii, h_ansi, east_asia, cs, hint, size, dstrike, underline, color, highlight, shading, vert_align, position, spacing, scale, kern,
            ligatures, lang, style, rtl, em
        );
        self.toggles = (self.toggles & !o.toggles_set) | (o.toggles & o.toggles_set);
        self.toggles_set |= o.toggles_set;
    }

    /// Merges a style level (STYLE-3): toggles turned on at this level XOR with the levels below; `w:val="0"` does nothing.
    pub fn merge_level(&mut self, level: &RunProps) {
        let on = level.toggles & level.toggles_set;
        let mut rest = level.clone();
        rest.toggles_set = 0;
        self.merge(&rest);
        self.toggles ^= on;
        self.toggles_set |= on;
    }

    /// One style chain folded into a level: ordinary inheritance, except a toggle is on when any style in the chain
    /// turns it on (no XOR within a chain; `w:val="0"` in a style does nothing).
    pub fn fold_chain<'a>(chain: impl Iterator<Item = &'a RunProps>) -> RunProps {
        let mut acc = RunProps::default();
        let mut on = 0;
        for s in chain {
            on |= s.toggles & s.toggles_set;
            acc.merge(s);
        }
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

pub fn highlight(v: &str) -> Option<u32> {
    Some(match v {
        "yellow" => 0xFFFF00,
        "green" => 0x00FF00,
        "cyan" => 0x00FFFF,
        "magenta" => 0xFF00FF,
        "blue" => 0x0000FF,
        "red" => 0xFF0000,
        "darkBlue" => 0x000080,
        "darkCyan" => 0x008080,
        "darkGreen" => 0x008000,
        "darkMagenta" => 0x800080,
        "darkRed" => 0x800000,
        "darkYellow" => 0x808000,
        "darkGray" => 0x808080,
        "lightGray" => 0xC0C0C0,
        "black" => 0x000000,
        "white" => 0xFFFFFF,
        _ => return None,
    })
}

/// `w:shd` → the visible fill (patterns `pctNN` blend the pattern colour over the fill).
pub fn shading_fill(e: Node) -> Option<u32> {
    let fill = color(attr(e, "fill"));
    match val(e) {
        None | Some("clear" | "nil") => fill,
        Some("solid") => Some(color(attr(e, "color")).unwrap_or(0)),
        Some(p) if p.starts_with("pct") => {
            let pct: u32 = p[3..].parse().ok()?;
            let fg = color(attr(e, "color")).unwrap_or(0);
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
    pub color: Option<u32>,
}

impl Border {
    pub fn parse(e: Node) -> Border {
        let style = val(e).unwrap_or("none").to_string();
        Border { width: num(attr(e, "sz")).unwrap_or(4.0) / 8.0, space: num(attr(e, "space")).unwrap_or(0.0), color: color(attr(e, "color")), style }
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
    /// The raw attributes, to tell frames apart (FRAME-1: any difference = separate frames).
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
    pub keep_next: Option<bool>,
    pub keep_lines: Option<bool>,
    pub page_break_before: Option<bool>,
    pub widow_control: Option<bool>,
    pub suppress_auto_hyphens: Option<bool>,
    pub suppress_line_numbers: Option<bool>,
    pub bidi: Option<bool>,
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
    pub frame: Option<FramePr>,
    pub text_alignment: Option<String>,
    pub outline_lvl: Option<i64>,
}

impl ParaProps {
    pub fn parse(ppr: Option<Node>) -> ParaProps {
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
                "keepNext" => p.keep_next = Some(on_off(e)),
                "keepLines" => p.keep_lines = Some(on_off(e)),
                "pageBreakBefore" => p.page_break_before = Some(on_off(e)),
                "widowControl" => p.widow_control = Some(on_off(e)),
                "suppressAutoHyphens" => p.suppress_auto_hyphens = Some(on_off(e)),
                "suppressLineNumbers" => p.suppress_line_numbers = Some(on_off(e)),
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
                    let b = |name| child(e, name).map(Border::parse);
                    p.border_top = b("top");
                    p.border_bottom = b("bottom");
                    p.border_left = b("left").or_else(|| b("start"));
                    p.border_right = b("right").or_else(|| b("end"));
                    p.border_between = b("between");
                }
                "shd" => p.shading = Some(shading_fill(e)),
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
            keep_next,
            keep_lines,
            page_break_before,
            widow_control,
            suppress_auto_hyphens,
            suppress_line_numbers,
            bidi,
            num_id,
            ilvl,
            border_top,
            border_bottom,
            border_left,
            border_right,
            border_between,
            shading,
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
    fn parse(e: Node) -> Borders {
        let b = |n| child(e, n).map(Border::parse);
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
    pub floating: bool,
    pub shading: Option<Option<u32>>,
}

impl TblProps {
    pub fn parse(e: Option<Node>) -> TblProps {
        let mut p = TblProps::default();
        let Some(e) = e else { return p };
        for c in e.children().filter(|c| c.is_element() && c.tag_name().namespace() == Some(W)) {
            match c.tag_name().name() {
                "tblStyle" => p.style = val(c).map(str::to_string),
                "tblW" => p.width = Some(Width::parse(c)),
                "jc" => {
                    p.jc = Some(match val(c) {
                        Some("center") => Jc::Center,
                        Some("right" | "end") => Jc::Right,
                        _ => Jc::Left,
                    })
                }
                "tblInd" => p.ind = Some(if let Width::Dxa(v) = Width::parse(c) { v } else { 0.0 }),
                "tblBorders" => p.borders = Borders::parse(c),
                "tblCellMar" => p.margins = Margins::parse(c),
                "tblLayout" => p.fixed = Some(attr(c, "type") == Some("fixed")),
                "tblCellSpacing" => p.cell_spacing = Some(if let Width::Dxa(v) = Width::parse(c) { v } else { 0.0 }),
                "tblStyleRowBandSize" => p.row_band = int(val(c)),
                "tblStyleColBandSize" => p.col_band = int(val(c)),
                "tblpPr" => p.floating = true,
                "shd" => p.shading = Some(shading_fill(c)),
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
        take!(style, width, jc, ind, fixed, look, cell_spacing, row_band, col_band, shading);
        self.borders.merge(&o.borders);
        self.margins.merge(&o.margins);
        self.floating |= o.floating;
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
    pub margins: Margins,
    pub v_align: Option<String>,
    pub text_direction: Option<String>,
    pub no_wrap: Option<bool>,
}

impl TcProps {
    pub fn parse(e: Option<Node>) -> TcProps {
        let mut p = TcProps::default();
        let Some(e) = e else { return p };
        for c in e.children().filter(|c| c.is_element() && c.tag_name().namespace() == Some(W)) {
            match c.tag_name().name() {
                "tcW" => p.width = Some(Width::parse(c)),
                "gridSpan" => p.grid_span = int(val(c)),
                "vMerge" => p.v_merge = Some(val(c) == Some("restart")),
                "hMerge" => p.h_merge = Some(val(c) == Some("restart")),
                "tcBorders" => p.borders = Borders::parse(c),
                "shd" => p.shading = Some(shading_fill(c)),
                "tcMar" => p.margins = Margins::parse(c),
                "vAlign" => p.v_align = val(c).map(str::to_string),
                "textDirection" => p.text_direction = val(c).map(str::to_string),
                "noWrap" => p.no_wrap = Some(on_off(c)),
                _ => {}
            }
        }
        p
    }

    pub fn merge(&mut self, o: &TcProps) {
        macro_rules! take {
            ($($f:ident),*) => { $( if o.$f.is_some() { self.$f = o.$f.clone(); } )* };
        }
        take!(width, grid_span, v_merge, h_merge, shading, v_align, text_direction, no_wrap);
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
