//! VML shapes

use std::fmt::Write;

use roxmltree::Node;

use super::colour::{Colour, ColourMod};
use super::geom::{CustomGeom, Geometry, Preset};
use super::paint::{self, Fill, Line};
use super::read::Ctx;
use super::text::{BodyProps, TextDirection, VAnchor};
use super::units::{Angle, Emu, Pct};
use super::{GroupShape, NonVisual, PicShape, Shape, ShapeKind, ShapeText, SpShape, Spans, Xfrm};

const V: &str = "urn:schemas-microsoft-com:vml";
const O: &str = "urn:schemas-microsoft-com:office:office";
const W10: &str = "urn:schemas-microsoft-com:office:word";
const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

pub struct VmlObject<'a, 'i> {
    pub shape: Shape,
    pub style: Style,
    pub textbox: Option<(Node<'a, 'i>, [f64; 4], u8)>,
    pub wrap: Option<(String, String)>,
    pub alt: String,
    /// `o:allowoverlap`, `o:allowincell` (on when absent).
    pub allow_overlap: bool,
    pub allow_in_cell: bool,
    /// `wrapcoords`: the wrap polygon's points, in 21600ths of the shape's box.
    pub wrap_coords: Option<Vec<(f64, f64)>>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Style {
    pub absolute: bool,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub coords: [f64; 4],
    pub rotation: f64,
    pub flip_h: bool,
    pub flip_v: bool,
    pub z: i64,
    pub hidden: bool,
    pub h_align: Option<String>,
    pub h_relative: Option<String>,
    pub v_align: Option<String>,
    pub v_relative: Option<String>,
    pub text_anchor: Option<String>,
    /// `mso-width-relative`/`mso-width-percent` and the height's, in tenths of a percent.
    pub size_percent: [Option<(String, i64)>; 2],
    /// `mso-left-percent`, `mso-top-percent`.
    pub pos_percent: [Option<i64>; 2],
    /// `mso-wrap-distance-top`, `-bottom`, `-left`, `-right`.
    pub wrap_dist: [Option<f64>; 4],
    /// `layout-flow` and `mso-layout-flow-alt`.
    pub layout_flow: Option<String>,
    pub layout_flow_alt: Option<String>,
}

impl Style {
    /// The text flow `layout-flow` gives, as the DrawingML direction it lays out like; `None` when not given. The
    /// ideographic flows stay horizontal.
    fn direction(&self) -> Option<TextDirection> {
        Some(match (self.layout_flow.as_deref()?, self.layout_flow_alt.as_deref()) {
            ("vertical", Some("bottom-to-top")) => TextDirection::Vert270,
            ("vertical", _) => TextDirection::Vert,
            _ => TextDirection::Horizontal,
        })
    }
}

fn is(n: Node, ns: &str, name: &str) -> bool {
    n.is_element() && n.tag_name().namespace() == Some(ns) && n.tag_name().name() == name
}

fn child<'a, 'i>(n: Node<'a, 'i>, name: &str) -> Option<Node<'a, 'i>> {
    n.children().find(|c| is(*c, V, name))
}

pub fn length(v: &str) -> Option<f64> {
    let v = v.trim();
    let (num, unit) = match v.find(|c: char| c.is_ascii_alphabetic() || c == '%') {
        Some(i) => (&v[..i], &v[i..]),
        None => (v, ""),
    };
    let n: f64 = num.trim().parse().ok()?;
    Some(match unit {
        "pt" => n,
        "in" => n * 72.0,
        "cm" => n * 72.0 / 2.54,
        "mm" => n * 72.0 / 25.4,
        "pc" => n * 12.0,
        "emu" => n / 12700.0,
        "px" | "" => n * 0.75,
        _ => return None,
    })
}

pub fn fraction(v: &str) -> Option<f64> {
    let v = v.trim();
    if let Some(p) = v.strip_suffix('%') {
        p.trim().parse::<f64>().ok().map(|x| x / 100.0)
    } else if let Some(f) = v.strip_suffix('f') {
        f.trim().parse::<f64>().ok().map(|x| x / 65536.0)
    } else {
        v.parse().ok()
    }
}

fn degrees(v: &str) -> Option<f64> {
    let v = v.trim();
    match v.strip_suffix("fd") {
        Some(f) => f.trim().parse::<f64>().ok().map(|x| x / 65536.0),
        None => v.parse().ok(),
    }
}

pub fn style(n: Node) -> Style {
    let mut s = Style::default();
    let Some(css) = n.attribute("style") else { return s };
    for decl in css.split(';') {
        let Some((k, v)) = decl.split_once(':') else { continue };
        let (k, v) = (k.trim().to_ascii_lowercase(), v.trim());
        let num = |v: &str| v.trim().parse::<f64>().ok();
        match k.as_str() {
            "position" => s.absolute = v == "absolute",
            "margin-left" => s.x = length(v).unwrap_or(0.0),
            "margin-top" => s.y = length(v).unwrap_or(0.0),
            "left" => {
                s.coords[0] = num(v).unwrap_or(0.0);
                s.x = length(v).unwrap_or(s.x);
            }
            "top" => {
                s.coords[1] = num(v).unwrap_or(0.0);
                s.y = length(v).unwrap_or(s.y);
            }
            "width" => {
                s.coords[2] = num(v).unwrap_or(0.0);
                s.w = length(v).unwrap_or(0.0);
            }
            "height" => {
                s.coords[3] = num(v).unwrap_or(0.0);
                s.h = length(v).unwrap_or(0.0);
            }
            "rotation" => s.rotation = degrees(v).unwrap_or(0.0),
            "flip" => {
                s.flip_h = v.contains('x');
                s.flip_v = v.contains('y');
            }
            "z-index" => s.z = v.parse().unwrap_or(0),
            "visibility" => s.hidden = v == "hidden",
            "mso-position-horizontal" => s.h_align = Some(v.to_string()),
            "mso-position-horizontal-relative" => s.h_relative = Some(v.to_string()),
            "mso-position-vertical" => s.v_align = Some(v.to_string()),
            "mso-position-vertical-relative" => s.v_relative = Some(v.to_string()),
            "v-text-anchor" => s.text_anchor = Some(v.to_string()),
            "layout-flow" => s.layout_flow = Some(v.to_string()),
            "mso-layout-flow-alt" => s.layout_flow_alt = Some(v.to_string()),
            "mso-width-percent" | "mso-height-percent" => {
                let k = usize::from(k == "mso-height-percent");
                let from = s.size_percent[k].take().map_or_else(|| "margin".to_string(), |(f, _)| f);
                s.size_percent[k] = Some((from, v.parse().unwrap_or(0)));
            }
            "mso-width-relative" | "mso-height-relative" => {
                let k = usize::from(k == "mso-height-relative");
                let pct = s.size_percent[k].take().map_or(0, |(_, p)| p);
                s.size_percent[k] = Some((v.to_string(), pct));
            }
            "mso-left-percent" => s.pos_percent[0] = v.parse().ok(),
            "mso-top-percent" => s.pos_percent[1] = v.parse().ok(),
            "mso-wrap-distance-top" => s.wrap_dist[0] = length(v),
            "mso-wrap-distance-bottom" => s.wrap_dist[1] = length(v),
            "mso-wrap-distance-left" => s.wrap_dist[2] = length(v),
            "mso-wrap-distance-right" => s.wrap_dist[3] = length(v),
            _ => {}
        }
    }
    s
}

pub fn colour(v: &str) -> Option<Colour> {
    let v = v.trim();
    let token = v.split([' ', '[']).next().unwrap_or("");
    if let Some(hex) = token.strip_prefix('#') {
        let hex = if hex.len() == 3 { hex.chars().flat_map(|c| [c, c]).collect::<String>() } else { hex.to_string() };
        return u32::from_str_radix(&hex, 16).ok().filter(|_| hex.len() == 6).map(Colour::srgb);
    }
    let html = match token.to_ascii_lowercase().as_str() {
        "black" => 0x000000,
        "silver" => 0xC0C0C0,
        "gray" | "grey" => 0x808080,
        "white" => 0xFFFFFF,
        "maroon" => 0x800000,
        "red" => 0xFF0000,
        "purple" => 0x800080,
        "fuchsia" => 0xFF00FF,
        "green" => 0x008000,
        "lime" => 0x00FF00,
        "olive" => 0x808000,
        "yellow" => 0xFFFF00,
        "navy" => 0x000080,
        "blue" => 0x0000FF,
        "teal" => 0x008080,
        "aqua" => 0x00FFFF,
        _ => return super::colour::preset_colour(token).map(Colour::srgb),
    };
    Some(Colour::srgb(html))
}

fn with_alpha(c: Colour, opacity: Option<f64>) -> Colour {
    match opacity.filter(|o| *o < 1.0) {
        Some(o) => c.with(ColourMod::Alpha(Pct((o.clamp(0.0, 1.0) * 100000.0).round() as i32))),
        None => c,
    }
}

fn on(v: Option<&str>, dflt: bool) -> bool {
    match v {
        Some(v) => matches!(v.trim(), "t" | "true" | "on" | "1"),
        None => dflt,
    }
}

fn fill(el: Node, image: &mut dyn FnMut(&str) -> Option<usize>) -> Fill {
    let f = child(el, "fill");
    if !on(f.and_then(|f| f.attribute("on")).or(el.attribute("filled")), true) {
        return Fill::None;
    }
    let base = f.and_then(|f| f.attribute("color")).or(el.attribute("fillcolor")).and_then(colour).unwrap_or_else(|| Colour::srgb(0xFFFFFF));
    let opacity = f.and_then(|f| f.attribute("opacity")).and_then(fraction);
    let solid = Fill::Solid(with_alpha(base.clone(), opacity));
    let Some(f) = f else { return solid };
    let color2 = f.attribute("color2").and_then(colour).unwrap_or_else(|| Colour::srgb(0xFFFFFF));
    let opacity2 = f.attribute((O, "opacity2")).or(f.attribute("opacity2")).and_then(fraction);
    let pct = |v: f64| Pct((v * 100000.0).round() as i32);
    match f.attribute("type").unwrap_or("solid") {
        t @ ("gradient" | "gradientRadial") => {
            let (c1, c2) = (with_alpha(base, opacity), with_alpha(color2, opacity2));
            let mut stops: Vec<(Pct, Colour)> = f
                .attribute("colors")
                .map(|cs| {
                    cs.split(';')
                        .filter_map(|e| {
                            let (p, c) = e.trim().split_once(' ')?;
                            Some((pct(fraction(p)?.clamp(0.0, 1.0)), colour(c)?))
                        })
                        .collect()
                })
                .unwrap_or_default();
            if stops.len() < 2 {
                let focus = f.attribute("focus").and_then(fraction).unwrap_or(0.0);
                stops = if focus.abs() < 0.25 {
                    vec![(Pct(0), c1), (Pct(100000), c2)]
                } else if focus.abs() > 0.75 {
                    vec![(Pct(0), c2), (Pct(100000), c1)]
                } else if focus > 0.0 {
                    vec![(Pct(0), c1.clone()), (Pct(50000), c2), (Pct(100000), c1)]
                } else {
                    vec![(Pct(0), c2.clone()), (Pct(50000), c1), (Pct(100000), c2)]
                };
            }
            let kind = if t == "gradientRadial" {
                stops = stops.into_iter().rev().map(|(p, c)| (Pct(100000 - p.0), c)).collect();
                paint::GradientKind::Path { path: paint::GradientPath::Rect, focus: [Pct(50000); 4] }
            } else {
                let a = f.attribute("angle").and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0);
                paint::GradientKind::Linear { angle: Angle(((a + 180.0).rem_euclid(360.0) * 60000.0).round() as i32), scaled: false }
            };
            Fill::Gradient(paint::Gradient { stops, kind, tile: [Pct(0); 4], flip: paint::TileFlip::None, rot_with_shape: true })
        }
        t @ ("tile" | "frame" | "pattern") => {
            let img = f.attribute((R, "id")).or(f.attribute((O, "relid"))).and_then(&mut *image);
            let mut adjust = Vec::new();
            if t == "pattern" {
                adjust.push(paint::BlipAdjust::Duotone([base.clone(), color2.clone()]));
            }
            let scale = if t == "pattern" { Pct(133333) } else { Pct(100000) };
            let mode = if t != "frame" {
                paint::BlipMode::Tile { tx: Emu(0), ty: Emu(0), sx: scale, sy: scale, flip: paint::TileFlip::None, align: paint::RectAlign::Tl }
            } else {
                paint::BlipMode::Stretch([Pct(0); 4])
            };
            if let Some(o) = opacity.filter(|o| *o < 1.0) {
                adjust.push(paint::BlipAdjust::AlphaModFix(pct(o)));
            }
            Fill::Blip(paint::BlipFill { image: img, src: [Pct(0); 4], mode, adjust, rot_with_shape: true })
        }
        _ => solid,
    }
}

fn stroke(el: Node) -> Line {
    let s = child(el, "stroke");
    if !on(s.and_then(|s| s.attribute("on")).or(el.attribute("stroked")), true) {
        return Line { fill: Some(Fill::None), ..Line::default() };
    }
    let c = s.and_then(|s| s.attribute("color")).or(el.attribute("strokecolor")).and_then(colour).unwrap_or_else(|| Colour::srgb(0));
    let opacity = s.and_then(|s| s.attribute("opacity")).and_then(fraction);
    let w = s.and_then(|s| s.attribute("weight")).or(el.attribute("strokeweight")).and_then(length).unwrap_or(0.75);
    let attr = |k: &str| s.and_then(|s| s.attribute(k));
    let dash = attr("dashstyle").map(|d| {
        use paint::PresetDash as P;
        let nums: Vec<f64> = d.split_whitespace().filter_map(|v| v.parse().ok()).collect();
        if nums.len() >= 2 {
            let pct = |v: f64| Pct((v * 100000.0).round() as i32);
            return paint::Dash::Custom(nums.chunks(2).filter(|c| c.len() == 2).map(|c| (pct(c[0]), pct(c[1]))).collect());
        }
        paint::Dash::Preset(match d.to_ascii_lowercase().as_str() {
            "shortdash" => P::SysDash,
            "shortdot" => P::SysDot,
            "shortdashdot" => P::SysDashDot,
            "shortdashdotdot" => P::SysDashDotDot,
            "dot" => P::Dot,
            "dash" => P::Dash,
            "longdash" => P::LgDash,
            "dashdot" => P::DashDot,
            "longdashdot" => P::LgDashDot,
            "longdashdotdot" => P::LgDashDotDot,
            _ => P::Solid,
        })
    });
    let compound = attr("linestyle").map(|l| match l {
        "thinThin" => paint::Compound::Double,
        "thinThick" => paint::Compound::ThinThick,
        "thickThin" => paint::Compound::ThickThin,
        "thickBetweenThin" => paint::Compound::Triple,
        _ => paint::Compound::Single,
    });
    let size = |v: Option<&str>, small: &str, large: &str| match v {
        Some(x) if x == small => paint::EndSize::Small,
        Some(x) if x == large => paint::EndSize::Large,
        _ => paint::EndSize::Medium,
    };
    let end = |kind: Option<&str>, w: Option<&str>, l: Option<&str>| {
        let kind = match kind? {
            "block" => paint::EndKind::Triangle,
            "classic" => paint::EndKind::Stealth,
            "diamond" => paint::EndKind::Diamond,
            "oval" => paint::EndKind::Oval,
            "open" => paint::EndKind::Arrow,
            _ => return None,
        };
        Some(paint::LineEnd { kind, width: size(w, "narrow", "wide"), length: size(l, "short", "long") })
    };
    Line {
        width: Some(Emu((w * 12700.0).round() as i64)),
        fill: Some(Fill::Solid(with_alpha(c, opacity))),
        dash,
        compound,
        cap: attr("endcap").map(|c| match c {
            "round" => paint::Cap::Round,
            "square" => paint::Cap::Square,
            _ => paint::Cap::Flat,
        }),
        join: attr("joinstyle").map(|j| match j {
            "bevel" => paint::Join::Bevel,
            "miter" => paint::Join::Miter(Pct(800000)),
            _ => paint::Join::Round,
        }),
        inset: None,
        head: end(attr("startarrow"), attr("startarrowwidth"), attr("startarrowlength")),
        tail: end(attr("endarrow"), attr("endarrowwidth"), attr("endarrowlength")),
    }
}

fn shadow(el: Node) -> Option<paint::EffectList> {
    let s = child(el, "shadow").filter(|s| on(s.attribute("on"), false))?;
    let off: Vec<f64> = s.attribute("offset").unwrap_or("2pt,2pt").split(',').map(|v| length(v).unwrap_or(2.0)).collect();
    let (dx, dy) = (off.first().copied().unwrap_or(2.0), off.get(1).copied().unwrap_or(2.0));
    let c = s.attribute("color").and_then(colour).unwrap_or_else(|| Colour::srgb(0x808080));
    let emu = |v: f64| Emu((v * 12700.0).round() as i64);
    Some(paint::EffectList {
        outer: Some(paint::OuterShadow {
            blur: Emu(0),
            dist: emu((dx * dx + dy * dy).sqrt()),
            dir: Angle((dy.atan2(dx).to_degrees().rem_euclid(360.0) * 60000.0).round() as i32),
            sx: Pct(100000),
            sy: Pct(100000),
            kx: Angle(0),
            ky: Angle(0),
            align: paint::RectAlign::Tl,
            colour: with_alpha(c, s.attribute("opacity").and_then(fraction)),
            rot_with_shape: false,
        }),
        ..paint::EffectList::default()
    })
}

fn coords(n: Node, fallback: Option<Node>) -> ([f64; 2], [f64; 2]) {
    let pair = |k: &str, d: [f64; 2]| {
        n.attribute(k)
            .or_else(|| fallback.and_then(|f| f.attribute(k)))
            .map(|v| {
                let p: Vec<f64> = v.split(',').map(|x| x.trim().parse().unwrap_or(0.0)).collect();
                [p.first().copied().unwrap_or(d[0]), p.get(1).copied().unwrap_or(d[1])]
            })
            .unwrap_or(d)
    };
    (pair("coordorigin", [0.0, 0.0]), pair("coordsize", [21600.0, 21600.0]))
}

fn shapetype<'a, 'i>(n: Node<'a, 'i>) -> Option<Node<'a, 'i>> {
    let id = n.attribute("type")?.strip_prefix('#')?;
    n.document().descendants().find(|t| is(*t, V, "shapetype") && t.attribute("id") == Some(id))
}

pub fn path_geometry(path: &str, formulas: &[String], adj: &[i64], origin: [f64; 2], size: [f64; 2], textbox: Option<&str>) -> String {
    let mut g = Guides { out: String::new(), n: 0, size, origin };
    let av: String = adj.iter().enumerate().map(|(i, v)| format!(r#"<a:gd name="adj{i}" fmla="val {v}"/>"#)).collect();
    for (i, f) in formulas.iter().enumerate() {
        let f = g.formula(f);
        let _ = write!(g.out, r#"<a:gd name="f{i}" fmla="{f}"/>"#);
    }
    let paths = g.paths(path);
    let rect = textbox
        .and_then(|t| {
            let v: Vec<&str> = t.split(';').next()?.split(',').map(str::trim).collect();
            (v.len() == 4).then(|| v.iter().map(|a| g.coord(a, 0)).collect::<Vec<_>>())
        })
        .map(|v| format!(r#"<a:rect l="{}" t="{}" r="{}" b="{}"/>"#, v[0], v[1], v[2], v[3]))
        .unwrap_or_default();
    format!(
        r#"<a:custGeom xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:avLst>{av}</a:avLst><a:gdLst>{}</a:gdLst><a:ahLst/><a:cxnLst/>{rect}<a:pathLst>{paths}</a:pathLst></a:custGeom>"#,
        g.out
    )
}

struct Guides {
    out: String,
    n: usize,
    size: [f64; 2],
    origin: [f64; 2],
}

impl Guides {
    fn guide(&mut self, fmla: String) -> String {
        let name = format!("g{}", self.n);
        self.n += 1;
        let _ = write!(self.out, r#"<a:gd name="{name}" fmla="{fmla}"/>"#);
        name
    }

    fn arg(&self, a: &str) -> String {
        let a = a.trim();
        if let Some(n) = a.strip_prefix('#') {
            return format!("adj{n}");
        }
        if let Some(n) = a.strip_prefix('@') {
            return format!("f{n}");
        }
        let [w, h] = self.size;
        let [ox, oy] = self.origin;
        let v = match a {
            "" => 0.0,
            "width" => w,
            "height" => h,
            "xcenter" => ox + w / 2.0,
            "ycenter" => oy + h / 2.0,
            "xlimo" => w / 2.0,
            "ylimo" => h / 2.0,
            "hasstroke" | "hasfill" | "lineDrawn" => 1.0,
            "pixelLineWidth" => 1.0,
            "pixelWidth" | "emuWidth" | "emuWidth2" => w,
            "pixelHeight" | "emuHeight" | "emuHeight2" => h,
            _ => a.parse::<f64>().unwrap_or(0.0),
        };
        format!("{}", v.round() as i64)
    }

    fn formula(&mut self, eqn: &str) -> String {
        let p: Vec<&str> = eqn.split_whitespace().collect();
        let a = |i: usize| p.get(i).map_or("0".to_string(), |s| self.arg(s));
        let (x, y, z) = (a(1), a(2), a(3));
        let fd = |g: &mut Guides, v: &str| g.guide(format!("*/ {v} 60000 65536"));
        match p.first().copied().unwrap_or("val") {
            "val" => format!("val {x}"),
            "sum" => format!("+- {x} {y} {z}"),
            "product" | "prod" => format!("*/ {x} {y} {z}"),
            "mid" => format!("+/ {x} {y} 2"),
            "abs" => format!("abs {x}"),
            "min" => format!("min {x} {y}"),
            "max" => format!("max {x} {y}"),
            "if" => format!("?: {x} {y} {z}"),
            "mod" => format!("mod {x} {y} {z}"),
            "sqrt" => format!("sqrt {x}"),
            "atan2" => {
                let t = self.guide(format!("at2 {x} {y}"));
                format!("*/ {t} 65536 60000")
            }
            "sin" => {
                let t = fd(self, &y);
                format!("sin {x} {t}")
            }
            "cos" => {
                let t = fd(self, &y);
                format!("cos {x} {t}")
            }
            "tan" => {
                let t = fd(self, &y);
                format!("tan {x} {t}")
            }
            "sumangle" => {
                let (b, c) = (self.guide(format!("*/ {y} 65536 1")), self.guide(format!("*/ {z} 65536 1")));
                format!("+- {x} {b} {c}")
            }
            "ellipse" => {
                let t1 = self.guide(format!("*/ {x} {z} {y}"));
                let t2 = self.guide(format!("*/ {z} {z} 1"));
                let t3 = self.guide(format!("*/ {t1} {t1} 1"));
                let t4 = self.guide(format!("+- {t2} 0 {t3}"));
                format!("sqrt {t4}")
            }
            "sinatan2" => format!("sat2 {x} {y} {z}"),
            "cosatan2" => format!("cat2 {x} {y} {z}"),
            _ => format!("val {x}"),
        }
    }

    fn coord(&mut self, a: &str, axis: usize) -> String {
        let v = self.arg(a);
        let o = self.origin[axis];
        if o == 0.0 {
            return v;
        }
        match v.parse::<i64>() {
            Ok(n) => format!("{}", n - o.round() as i64),
            Err(_) => self.guide(format!("+- {v} 0 {}", o.round() as i64)),
        }
    }

    fn plus(&mut self, a: &str, b: &str) -> String {
        match (a.parse::<i64>(), b.parse::<i64>()) {
            (Ok(x), Ok(y)) => format!("{}", x + y),
            _ => self.guide(format!("+- {a} {b} 0")),
        }
    }

    fn minus(&mut self, a: &str, b: &str) -> String {
        match (a.parse::<i64>(), b.parse::<i64>()) {
            (Ok(x), Ok(y)) => format!("{}", x - y),
            _ => self.guide(format!("+- {a} 0 {b}")),
        }
    }

    fn paths(&mut self, path: &str) -> String {
        let [w, h] = self.size.map(|v| v.round() as i64);
        let tokens = tokens(path);
        let mut out = String::new();
        let mut cmds = String::new();
        let (mut nofill, mut nostroke) = (false, false);
        let mut cur = ("0".to_string(), "0".to_string());
        let mut start = cur.clone();
        let mut i = 0;
        let flush = |out: &mut String, cmds: &mut String, nofill: &mut bool, nostroke: &mut bool| {
            if !cmds.is_empty() {
                let fill = if *nofill { r#" fill="none""# } else { "" };
                let stroke = if *nostroke { r#" stroke="0""# } else { "" };
                let _ = write!(out, r#"<a:path w="{w}" h="{h}"{fill}{stroke}>{cmds}</a:path>"#);
            }
            cmds.clear();
            (*nofill, *nostroke) = (false, false);
        };
        let pt = |x: &str, y: &str| format!(r#"<a:pt x="{x}" y="{y}"/>"#);
        while i < tokens.len() {
            let Tok::Cmd(c) = &tokens[i] else {
                i += 1;
                continue;
            };
            let c = c.clone();
            i += 1;
            let mut args = Vec::new();
            while let Some(Tok::Arg(a)) = tokens.get(i) {
                args.push(a.clone());
                i += 1;
            }
            match c.as_str() {
                "m" | "t" => {
                    for p in args.chunks(2) {
                        let (mut x, mut y) = (self.coord(&p[0], 0), self.coord(p.get(1).map_or("0", String::as_str), 1));
                        if c == "t" {
                            let (dx, dy) = (self.arg(&p[0]), self.arg(p.get(1).map_or("0", String::as_str)));
                            (x, y) = (self.plus(&cur.0, &dx), self.plus(&cur.1, &dy));
                        }
                        let _ = write!(cmds, "<a:moveTo>{}</a:moveTo>", pt(&x, &y));
                        cur = (x, y);
                        start = cur.clone();
                    }
                }
                "l" | "r" => {
                    for p in args.chunks(2) {
                        let (mut x, mut y) = (self.coord(&p[0], 0), self.coord(p.get(1).map_or("0", String::as_str), 1));
                        if c == "r" {
                            let (dx, dy) = (self.arg(&p[0]), self.arg(p.get(1).map_or("0", String::as_str)));
                            (x, y) = (self.plus(&cur.0, &dx), self.plus(&cur.1, &dy));
                        }
                        let _ = write!(cmds, "<a:lnTo>{}</a:lnTo>", pt(&x, &y));
                        cur = (x, y);
                    }
                }
                "c" | "v" => {
                    for p in args.chunks(6).filter(|p| p.len() == 6) {
                        let mut q: Vec<(String, String)> = (0..3).map(|k| (self.coord(&p[2 * k], 0), self.coord(&p[2 * k + 1], 1))).collect();
                        if c == "v" {
                            q = (0..3)
                                .map(|k| {
                                    let (dx, dy) = (self.arg(&p[2 * k]), self.arg(&p[2 * k + 1]));
                                    (self.plus(&cur.0, &dx), self.plus(&cur.1, &dy))
                                })
                                .collect();
                        }
                        let _ = write!(cmds, "<a:cubicBezTo>{}{}{}</a:cubicBezTo>", pt(&q[0].0, &q[0].1), pt(&q[1].0, &q[1].1), pt(&q[2].0, &q[2].1));
                        cur = q[2].clone();
                    }
                }
                "qb" => {
                    for p in args.chunks(4).filter(|p| p.len() == 4) {
                        let (cx, cy, x, y) = (self.coord(&p[0], 0), self.coord(&p[1], 1), self.coord(&p[2], 0), self.coord(&p[3], 1));
                        let _ = write!(cmds, "<a:quadBezTo>{}{}</a:quadBezTo>", pt(&cx, &cy), pt(&x, &y));
                        cur = (x, y);
                    }
                }
                "x" => {
                    cmds.push_str("<a:close/>");
                    cur = start.clone();
                }
                "e" => flush(&mut out, &mut cmds, &mut nofill, &mut nostroke),
                "nf" => nofill = true,
                "ns" => nostroke = true,
                "ae" | "al" => {
                    for p in args.chunks(6).filter(|p| p.len() == 6) {
                        let (cx, cy) = (self.coord(&p[0], 0), self.coord(&p[1], 1));
                        let (rx, ry) = (self.arg(&p[2]), self.arg(&p[3]));
                        let (st, sw) = (self.arg(&p[4]), self.arg(&p[5]));
                        let st = self.guide(format!("*/ {st} -60000 65536"));
                        let sw = self.guide(format!("*/ {sw} -60000 65536"));
                        let ox = self.guide(format!("cos {rx} {st}"));
                        let oy = self.guide(format!("sin {ry} {st}"));
                        let (x, y) = (self.plus(&cx, &ox), self.plus(&cy, &oy));
                        let tag = if c == "ae" && !cmds.is_empty() { "lnTo" } else { "moveTo" };
                        let _ = write!(cmds, "<a:{tag}>{}</a:{tag}>", pt(&x, &y));
                        let _ = write!(cmds, r#"<a:arcTo wR="{rx}" hR="{ry}" stAng="{st}" swAng="{sw}"/>"#);
                        let en = self.guide(format!("+- {st} {sw} 0"));
                        let ex = self.guide(format!("cos {rx} {en}"));
                        let ey = self.guide(format!("sin {ry} {en}"));
                        cur = (self.plus(&cx, &ex), self.plus(&cy, &ey));
                        if c == "al" {
                            start = (x, y);
                        }
                    }
                }
                "at" | "ar" | "wa" | "wr" => {
                    for p in args.chunks(8).filter(|p| p.len() == 8) {
                        let v: Vec<String> = (0..8).map(|k| self.coord(&p[k], k % 2)).collect();
                        let cx = self.guide(format!("+/ {} {} 2", v[0], v[2]));
                        let cy = self.guide(format!("+/ {} {} 2", v[1], v[3]));
                        let (dw, dh) = (self.minus(&v[2], &v[0]), self.minus(&v[3], &v[1]));
                        let rx = self.guide(format!("+/ {dw} 0 2"));
                        let ry = self.guide(format!("+/ {dh} 0 2"));
                        let angle = |g: &mut Guides, x: &str, y: &str| {
                            let (dx, dy) = (g.minus(x, &cx), g.minus(y, &cy));
                            g.guide(format!("at2 {dx} {dy}"))
                        };
                        let (sa, ea) = (angle(self, &v[4], &v[5]), angle(self, &v[6], &v[7]));
                        let d = self.guide(format!("+- {ea} 0 {sa}"));
                        let sw = if c.starts_with('w') {
                            let up = self.guide(format!("+- {d} 21600000 0"));
                            self.guide(format!("?: {d} {d} {up}"))
                        } else {
                            let down = self.guide(format!("+- {d} 0 21600000"));
                            let neg = self.guide(format!("+- 0 0 {d}"));
                            self.guide(format!("?: {neg} {d} {down}"))
                        };
                        let ox = self.guide(format!("cos {rx} {sa}"));
                        let oy = self.guide(format!("sin {ry} {sa}"));
                        let (x, y) = (self.plus(&cx, &ox), self.plus(&cy, &oy));
                        let tag = if (c == "at" || c == "wa") && !cmds.is_empty() { "lnTo" } else { "moveTo" };
                        let _ = write!(cmds, "<a:{tag}>{}</a:{tag}>", pt(&x, &y));
                        let _ = write!(cmds, r#"<a:arcTo wR="{rx}" hR="{ry}" stAng="{sa}" swAng="{sw}"/>"#);
                        let ex = self.guide(format!("cos {rx} {ea}"));
                        let ey = self.guide(format!("sin {ry} {ea}"));
                        cur = (self.plus(&cx, &ex), self.plus(&cy, &ey));
                        if tag == "moveTo" {
                            start = (x, y);
                        }
                    }
                }
                "qx" | "qy" => {
                    let mut horizontal = c == "qx";
                    for p in args.chunks(2).filter(|p| p.len() == 2) {
                        let (x, y) = (self.coord(&p[0], 0), self.coord(&p[1], 1));
                        let (dx, dy) = (self.minus(&x, &cur.0), self.minus(&y, &cur.1));
                        let (rx, ry) = (self.guide(format!("abs {dx}")), self.guide(format!("abs {dy}")));
                        let sx = self.guide(format!("?: {dx} 1 -1"));
                        let sy = self.guide(format!("?: {dy} 1 -1"));
                        let s = self.guide(format!("*/ {sx} {sy} 1"));
                        let (st, sw) = if horizontal {
                            (self.guide(format!("?: {dy} 16200000 5400000")), self.guide(format!("*/ {s} 5400000 1")))
                        } else {
                            (self.guide(format!("?: {dx} 10800000 0")), self.guide(format!("*/ {s} -5400000 1")))
                        };
                        let _ = write!(cmds, r#"<a:arcTo wR="{rx}" hR="{ry}" stAng="{st}" swAng="{sw}"/>"#);
                        cur = (x, y);
                        horizontal = !horizontal;
                    }
                }
                _ => {}
            }
        }
        flush(&mut out, &mut cmds, &mut nofill, &mut nostroke);
        out
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Cmd(String),
    Arg(String),
}

fn tokens(path: &str) -> Vec<Tok> {
    const CMDS: [&str; 20] = ["nf", "ns", "ae", "al", "at", "ar", "wa", "wr", "qx", "qy", "qb", "m", "l", "c", "x", "e", "t", "r", "v", "h"];
    let s: Vec<char> = path.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    let mut last_comma = false;
    while i < s.len() {
        let c = s[i];
        if c.is_ascii_alphabetic() {
            if last_comma {
                out.push(Tok::Arg("0".into()));
            }
            let two: String = s[i..(i + 2).min(s.len())].iter().collect();
            let cmd = CMDS.iter().find(|k| k.len() == 2 && **k == two).map_or_else(|| c.to_string(), |k| k.to_string());
            i += cmd.len();
            let cmd = if cmd == "h" && i < s.len() {
                i += 1;
                "h".to_string()
            } else {
                cmd
            };
            out.push(Tok::Cmd(cmd));
            last_comma = false;
        } else if c == '@' || c == '#' || c == '-' || c.is_ascii_digit() || c == '.' {
            let st = i;
            i += 1;
            while i < s.len() && (s[i].is_ascii_digit() || s[i] == '.') {
                i += 1;
            }
            out.push(Tok::Arg(s[st..i].iter().collect()));
            last_comma = false;
        } else if c == ',' {
            if last_comma || matches!(out.last(), Some(Tok::Cmd(_))) {
                out.push(Tok::Arg("0".into()));
            }
            last_comma = true;
            i += 1;
        } else {
            i += 1;
        }
    }
    out
}

fn custom(xml: &str) -> Geometry {
    match roxmltree::Document::parse(xml) {
        Ok(d) => Geometry::Custom(std::rc::Rc::new(CustomGeom::parse(d.root_element()))),
        Err(_) => Geometry::Preset { preset: Preset::Rect, adj: Vec::new() },
    }
}

fn polyline(points: &[(f64, f64)], closed: bool) -> (Geometry, [f64; 4]) {
    let b = points.iter().fold([f64::MAX, f64::MAX, f64::MIN, f64::MIN], |b, p| [b[0].min(p.0), b[1].min(p.1), b[2].max(p.0), b[3].max(p.1)]);
    let (w, h) = (((b[2] - b[0]) * 100.0).round().max(1.0) as i64, ((b[3] - b[1]) * 100.0).round().max(1.0) as i64);
    let at = |p: &(f64, f64)| format!(r#"<a:pt x="{}" y="{}"/>"#, ((p.0 - b[0]) * 100.0).round() as i64, ((p.1 - b[1]) * 100.0).round() as i64);
    let mut cmds = String::new();
    for (i, p) in points.iter().enumerate() {
        let tag = if i == 0 { "moveTo" } else { "lnTo" };
        let _ = write!(cmds, "<a:{tag}>{}</a:{tag}>", at(p));
    }
    if closed {
        cmds.push_str("<a:close/>");
    }
    let fill = if closed { "" } else { r#" fill="none""# };
    (
        custom(&format!(
            r#"<a:custGeom xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:pathLst><a:path w="{w}" h="{h}"{fill}>{cmds}</a:path></a:pathLst></a:custGeom>"#
        )),
        b,
    )
}

fn points(v: &str) -> Vec<(f64, f64)> {
    let n: Vec<f64> = v.split([',', ' ']).filter(|s| !s.trim().is_empty()).filter_map(length).collect();
    n.chunks(2).filter(|c| c.len() == 2).map(|c| (c[0], c[1])).collect()
}

fn textbox<'a, 'i>(el: Node<'a, 'i>, st: &Style) -> Option<(Node<'a, 'i>, [f64; 4], u8)> {
    let tb = child(el, "textbox")?;
    let content = tb.descendants().find(|n| n.tag_name().name() == "txbxContent")?;
    let ins: Vec<f64> = tb.attribute("inset").unwrap_or("").split(',').map(|v| length(v).unwrap_or(f64::NAN)).collect();
    let i = |k: usize, d: f64| ins.get(k).copied().filter(|v| !v.is_nan()).unwrap_or(d);
    let anchor = match style(tb).text_anchor.or_else(|| st.text_anchor.clone()).as_deref() {
        Some(a) if a.starts_with("middle") => 1,
        Some(a) if a.starts_with("bottom") => 2,
        _ => 0,
    };
    Some((content, [i(0, 7.2), i(1, 3.6), i(2, 7.2), i(3, 3.6)], anchor))
}

fn element(el: Node, ctx: &mut Ctx) -> Option<Shape> {
    let st = style(el);
    let name = el.tag_name().name();
    let mut image = |rid: &str| ctx.source.image(rid);
    let (geom, rect): (Geometry, Option<[f64; 4]>) = match name {
        "rect" => (Geometry::Preset { preset: Preset::Rect, adj: Vec::new() }, None),
        "roundrect" => {
            let a = el.attribute("arcsize").and_then(fraction).unwrap_or(0.2);
            (Geometry::Preset { preset: Preset::RoundRect, adj: vec![("adj".into(), (a * 100000.0).round() as i64)] }, None)
        }
        "oval" => (Geometry::Preset { preset: Preset::Ellipse, adj: Vec::new() }, None),
        "line" => {
            let (a, b) = (points(el.attribute("from").unwrap_or("0,0")), points(el.attribute("to").unwrap_or("10,10")));
            let (g, r) = polyline(&[a.first().copied().unwrap_or_default(), b.first().copied().unwrap_or_default()], false);
            (g, Some(r))
        }
        "polyline" => {
            let pts = points(el.attribute("points").unwrap_or(""));
            if pts.len() < 2 {
                return None;
            }
            let closed = pts.first() == pts.last();
            let (g, r) = polyline(&pts, closed);
            (g, Some(r))
        }
        "curve" => {
            let p = |k: &str, d: &str| points(el.attribute(k).unwrap_or(d)).first().copied().unwrap_or_default();
            let q = [p("from", "0,0"), p("control1", "0,0"), p("control2", "0,0"), p("to", "0,0")];
            let b = q.iter().fold([f64::MAX, f64::MAX, f64::MIN, f64::MIN], |b, p| [b[0].min(p.0), b[1].min(p.1), b[2].max(p.0), b[3].max(p.1)]);
            let (w, h) = (((b[2] - b[0]) * 100.0).round().max(1.0) as i64, ((b[3] - b[1]) * 100.0).round().max(1.0) as i64);
            let at =
                |p: (f64, f64)| format!(r#"<a:pt x="{}" y="{}"/>"#, ((p.0 - b[0]) * 100.0).round() as i64, ((p.1 - b[1]) * 100.0).round() as i64);
            let xml = format!(
                r#"<a:custGeom xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:pathLst><a:path w="{w}" h="{h}" fill="none"><a:moveTo>{}</a:moveTo><a:cubicBezTo>{}{}{}</a:cubicBezTo></a:path></a:pathLst></a:custGeom>"#,
                at(q[0]),
                at(q[1]),
                at(q[2]),
                at(q[3])
            );
            (custom(&xml), Some(b))
        }
        "arc" => {
            let a0 = el.attribute("startangle").and_then(|v| v.parse::<f64>().ok()).unwrap_or(-90.0);
            let a1 = el.attribute("endangle").and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0);
            let (st, sw) = ((a0 - 90.0) * 60000.0, (a1 - a0) * 60000.0);
            let sx = (st.to_radians() / 60000.0).cos();
            let sy = (st.to_radians() / 60000.0).sin();
            let xml = format!(
                r#"<a:custGeom xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:pathLst><a:path w="2000" h="2000" fill="none"><a:moveTo><a:pt x="{}" y="{}"/></a:moveTo><a:arcTo wR="1000" hR="1000" stAng="{}" swAng="{}"/></a:path></a:pathLst></a:custGeom>"#,
                (1000.0 + 1000.0 * sx).round() as i64,
                (1000.0 + 1000.0 * sy).round() as i64,
                st.round() as i64,
                sw.round() as i64
            );
            (custom(&xml), None)
        }
        "shape" | "image" => {
            let t = shapetype(el);
            let data = child(el, "imagedata").or_else(|| t.and_then(|t| child(t, "imagedata")));
            let rid = data.and_then(|d| d.attribute((R, "id")).or(d.attribute((O, "relid"))));
            if name == "image" || rid.is_some() {
                let rid = rid.or(el.attribute((R, "id")));
                let img = rid.and_then(&mut image);
                let crop = |k: &str| data.and_then(|d| d.attribute(k)).and_then(fraction).map_or(Pct(0), |v| Pct((v * 100000.0).round() as i32));
                let blip = paint::BlipFill {
                    image: img,
                    src: [crop("cropleft"), crop("croptop"), crop("cropright"), crop("cropbottom")],
                    mode: paint::BlipMode::Stretch([Pct(0); 4]),
                    adjust: Vec::new(),
                    rot_with_shape: true,
                };
                let line = (on(el.attribute("stroked"), false) || child(el, "stroke").is_some()).then(|| stroke(el));
                return Some(Shape {
                    nv: NonVisual {
                        name: el.attribute("id").unwrap_or("").to_string(),
                        descr: el.attribute("alt").unwrap_or("").into(),
                        hidden: st.hidden,
                        ..NonVisual::default()
                    },
                    xfrm: xfrm(&st, None),
                    kind: ShapeKind::Pic(Box::new(PicShape {
                        geom: Geometry::Preset { preset: Preset::Rect, adj: Vec::new() },
                        blip,
                        line,
                        effects: shadow(el),
                        kept_3d: false,
                    })),
                    spans: Spans::default(),
                });
            }
            let spt = el.attribute((O, "spt")).or_else(|| t.and_then(|t| t.attribute((O, "spt"))));
            let builtin = match spt.and_then(|v| v.parse::<f64>().ok()).map(|v| v as u32) {
                Some(16) => Some(Preset::Cube),
                Some(84) => Some(Preset::Bevel),
                _ => None,
            };
            if let Some(preset) = builtin {
                let adj =
                    el.attribute("adj").or_else(|| t.and_then(|t| t.attribute("adj"))).and_then(|a| a.split(',').next()?.trim().parse::<f64>().ok());
                let adj = adj.map(|v| vec![("adj".to_string(), (v * 100000.0 / 21600.0).round() as i64)]).unwrap_or_default();
                return finish(el, &st, Geometry::Preset { preset, adj }, None, &mut image);
            }
            let path =
                el.attribute("path").or_else(|| child(el, "path").and_then(|p| p.attribute("v"))).or_else(|| t.and_then(|t| t.attribute("path")));
            match path {
                Some(path) => {
                    let formulas: Vec<String> = child(el, "formulas")
                        .or_else(|| t.and_then(|t| child(t, "formulas")))
                        .map(|f| f.children().filter(|c| is(*c, V, "f")).filter_map(|c| c.attribute("eqn").map(str::to_string)).collect())
                        .unwrap_or_default();
                    let parse =
                        |v: Option<&str>| v.map(|a| a.split(',').map(|x| x.trim().parse::<i64>().ok()).collect::<Vec<_>>()).unwrap_or_default();
                    let (mine, theirs) = (parse(el.attribute("adj")), parse(t.and_then(|t| t.attribute("adj"))));
                    let adj: Vec<i64> = (0..mine.len().max(theirs.len()))
                        .map(|i| mine.get(i).copied().flatten().or_else(|| theirs.get(i).copied().flatten()).unwrap_or(0))
                        .collect();
                    let (origin, size) = coords(el, t);
                    let tbr = child(el, "path")
                        .and_then(|p| p.attribute("textboxrect"))
                        .or_else(|| t.and_then(|t| child(t, "path")).and_then(|p| p.attribute("textboxrect")));
                    (custom(&path_geometry(path, &formulas, &adj, origin, size, tbr)), None)
                }
                None => {
                    let preset = match spt.and_then(|v| v.parse::<f64>().ok()).map(|v| v as u32) {
                        Some(2) => Preset::RoundRect,
                        Some(3) => Preset::Ellipse,
                        Some(20) => Preset::Line,
                        _ => Preset::Rect,
                    };
                    (Geometry::Preset { preset, adj: Vec::new() }, None)
                }
            }
        }
        "group" => {
            let (origin, size) = coords(el, None);
            let mut children = Vec::new();
            for c in el.children().filter(|c| c.is_element() && c.tag_name().namespace() == Some(V)) {
                if let Some(mut s) = element(c, ctx) {
                    let cs = style(c);
                    if !matches!(c.tag_name().name(), "line" | "polyline" | "curve") {
                        s.xfrm.off = [Emu(cs.coords[0].round() as i64), Emu(cs.coords[1].round() as i64)];
                        s.xfrm.ext = [Emu(cs.coords[2].round() as i64), Emu(cs.coords[3].round() as i64)];
                    }
                    children.push(s);
                }
            }
            return Some(Shape {
                nv: NonVisual { name: el.attribute("id").unwrap_or("").to_string(), hidden: st.hidden, ..NonVisual::default() },
                xfrm: xfrm(&st, None),
                kind: ShapeKind::Group(GroupShape {
                    child_off: [Emu(origin[0].round() as i64), Emu(origin[1].round() as i64)],
                    child_ext: [Emu(size[0].round() as i64), Emu(size[1].round() as i64)],
                    fill: None,
                    children,
                    canvas: false,
                }),
                spans: Spans::default(),
            });
        }
        _ => return None,
    };
    finish(el, &st, geom, rect, &mut image)
}

fn finish(el: Node, st: &Style, geom: Geometry, rect: Option<[f64; 4]>, image: &mut dyn FnMut(&str) -> Option<usize>) -> Option<Shape> {
    let name = el.tag_name().name();
    let emu = |v: f64| Emu((v * 12700.0).round() as i64);
    let lines = matches!(name, "line" | "polyline" | "curve" | "arc");
    let text = textbox(el, st).is_some();
    let anchor = match st.text_anchor.as_deref() {
        Some(a) if a.starts_with("middle") => VAnchor::Centre,
        Some(a) if a.starts_with("bottom") => VAnchor::Bottom,
        _ => VAnchor::Top,
    };
    // The text box's own flow, else the shape's.
    let direction = child(el, "textbox").and_then(|tb| style(tb).direction()).or_else(|| st.direction()).unwrap_or_default();
    let ins = child(el, "textbox").map(|tb| {
        let v: Vec<f64> = tb.attribute("inset").unwrap_or("").split(',').map(|v| length(v).unwrap_or(f64::NAN)).collect();
        let i = |k: usize, d: f64| emu(v.get(k).copied().filter(|x| !x.is_nan()).unwrap_or(d));
        [i(0, 7.2), i(1, 3.6), i(2, 7.2), i(3, 3.6)]
    });
    let sp = SpShape {
        geom,
        fill: Some(if lines && name != "polyline" { Fill::None } else { fill(el, image) }),
        line: Some(stroke(el)),
        effects: shadow(el),
        style: None,
        body: BodyProps { anchor, insets: ins.unwrap_or(BodyProps::default().insets), direction, ..BodyProps::default() },
        text: if text { ShapeText::Story } else { ShapeText::None },
        text_box: text,
        chain: None,
        connector: false,
        kept_3d: child(el, "textpath").is_some() || el.children().any(|c| is(c, O, "extrusion")),
    };
    Some(Shape {
        nv: NonVisual {
            name: el.attribute("id").unwrap_or("").to_string(),
            descr: el.attribute("alt").unwrap_or("").into(),
            hidden: st.hidden,
            ..NonVisual::default()
        },
        xfrm: xfrm(st, rect),
        kind: ShapeKind::Sp(Box::new(sp)),
        spans: Spans::default(),
    })
}

fn xfrm(st: &Style, rect: Option<[f64; 4]>) -> Xfrm {
    let emu = |v: f64| Emu((v * 12700.0).round() as i64);
    let (x, y, w, h) = match rect {
        Some(r) => (st.x + r[0], st.y + r[1], r[2] - r[0], r[3] - r[1]),
        None => (st.x, st.y, st.w, st.h),
    };
    Xfrm {
        off: [emu(x), emu(y)],
        ext: [emu(w), emu(h)],
        rot: Angle((st.rotation.rem_euclid(360.0) * 60000.0).round() as i32),
        flip_h: st.flip_h,
        flip_v: st.flip_v,
    }
}

pub fn read<'a, 'i>(pict: Node<'a, 'i>, ctx: &mut Ctx) -> Option<VmlObject<'a, 'i>> {
    let el = pict.children().find(|c| c.is_element() && c.tag_name().namespace() == Some(V) && c.tag_name().name() != "shapetype")?;
    let shape = element(el, ctx)?;
    let mut st = style(el);
    if matches!(el.tag_name().name(), "line" | "polyline" | "curve") {
        st.x = shape.xfrm.off[0].0 as f64 / 12700.0;
        st.y = shape.xfrm.off[1].0 as f64 / 12700.0;
        st.w = shape.xfrm.ext[0].0 as f64 / 12700.0;
        st.h = shape.xfrm.ext[1].0 as f64 / 12700.0;
    }
    let wrap = el
        .children()
        .find(|c| is(*c, W10, "wrap"))
        .map(|w| (w.attribute("type").unwrap_or("").to_string(), w.attribute("side").unwrap_or("both").to_string()));
    let mut shape = shape;
    shape.xfrm.off = [Emu(0); 2];
    let nums: Vec<f64> = el.attribute("wrapcoords").unwrap_or("").split([' ', ',']).filter_map(|v| v.trim().parse().ok()).collect();
    let wrap_coords = (nums.len() >= 4).then(|| nums.as_chunks::<2>().0.iter().map(|p| (p[0] / 21600.0, p[1] / 21600.0)).collect());
    Some(VmlObject {
        textbox: textbox(el, &st),
        shape,
        wrap,
        alt: el.attribute("alt").unwrap_or("").to_string(),
        allow_overlap: on(el.attribute((O, "allowoverlap")), true),
        allow_in_cell: on(el.attribute((O, "allowincell")), true),
        wrap_coords,
        style: st,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lengths_fractions_and_colours() {
        assert_eq!(length("1in"), Some(72.0));
        assert_eq!(length("12pt"), Some(12.0));
        assert_eq!(length("4"), Some(3.0));
        assert_eq!(fraction("50%"), Some(0.5));
        assert_eq!(fraction("32768f"), Some(0.5));
        assert_eq!(colour("#4472c4"), Some(Colour::srgb(0x4472C4)));
        assert_eq!(colour("black [3213]"), Some(Colour::srgb(0)));
        assert_eq!(colour("#f00"), Some(Colour::srgb(0xFF0000)));
    }

    #[test]
    fn styles_place_and_turn() {
        let d = roxmltree::Document::parse(
            r#"<v:rect xmlns:v="urn:schemas-microsoft-com:vml" style="position:absolute;margin-left:72pt;margin-top:1in;width:90pt;height:60pt;rotation:30;flip:x;z-index:-3;mso-position-horizontal-relative:page"/>"#,
        )
        .unwrap();
        let s = style(d.root_element());
        assert!(s.absolute);
        assert_eq!((s.x, s.y, s.w, s.h, s.rotation, s.flip_h, s.z), (72.0, 72.0, 90.0, 60.0, 30.0, true, -3));
        assert_eq!(s.h_relative.as_deref(), Some("page"));
    }

    #[test]
    fn a_layout_flow_reads_as_its_text_direction() {
        let dir = |css: &str| {
            let xml = format!(r#"<v:textbox xmlns:v="urn:schemas-microsoft-com:vml" style="{css}"/>"#);
            style(roxmltree::Document::parse(&xml).unwrap().root_element()).direction()
        };
        assert_eq!(dir("layout-flow:vertical"), Some(TextDirection::Vert));
        assert_eq!(dir("layout-flow:vertical;mso-layout-flow-alt:bottom-to-top"), Some(TextDirection::Vert270));
        assert_eq!(dir("mso-layout-flow-alt:top-to-bottom;layout-flow:vertical"), Some(TextDirection::Vert));
        assert_eq!(dir("layout-flow:vertical-ideographic"), Some(TextDirection::Horizontal));
        assert_eq!(dir("mso-fit-shape-to-text:t"), None);
    }

    #[test]
    fn a_vml_path_with_formulas_evaluates() {
        let xml = path_geometry("m0,0l@0,0,21600,21600xe", &["val #0".into()], &[10800], [0.0, 0.0], [21600.0, 21600.0], None);
        let g = custom(&xml).evaluate(Emu(1270000), Emu(1270000));
        let crate::drawing::geom::Seg::Line(p) = g.paths[0].segs[1] else { panic!("{:?}", g.paths[0].segs) };
        assert!((p[0] - 50.0).abs() < 0.01 && p[1].abs() < 0.01, "{p:?}");
        let xml = path_geometry("m0,10800qx10800,0e", &[], &[], [0.0, 0.0], [21600.0, 21600.0], None);
        let g = custom(&xml).evaluate(Emu(1270000), Emu(1270000));
        assert!(g.paths[0].segs.len() >= 2, "{:?}", g.paths[0].segs);
        assert_eq!(
            tokens("m@0,,l1,2"),
            vec![
                Tok::Cmd("m".into()),
                Tok::Arg("@0".into()),
                Tok::Arg("0".into()),
                Tok::Arg("0".into()),
                Tok::Cmd("l".into()),
                Tok::Arg("1".into()),
                Tok::Arg("2".into())
            ]
        );
        assert_eq!(tokens("qy10800,x"), vec![Tok::Cmd("qy".into()), Tok::Arg("10800".into()), Tok::Arg("0".into()), Tok::Cmd("x".into())]);
    }
}
