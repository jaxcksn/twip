//! VML shapes

use std::fmt::Write;

use roxmltree::Node;

use super::colour::{Colour, ColourMod};
use super::geom::{CustomGeom, Geometry, Preset};
use super::paint::{self, Fill, Line};
use super::read::Ctx;
use super::text::{BodyProps, TextDirection, VAnchor};
use super::units::{Angle, Emu, Pct};
use super::{GroupShape, NonVisual, PicShape, Shape, ShapeKind, ShapeText, SpShape, Spans, TextPath, Xfrm};

const V: &str = "urn:schemas-microsoft-com:vml";
const O: &str = "urn:schemas-microsoft-com:office:office";
const W10: &str = "urn:schemas-microsoft-com:office:word";
const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

pub struct VmlObject<'a, 'i> {
    pub shape: Shape,
    pub style: Style,
    /// Its text, insets, vertical anchor and whether the shape fits the text.
    pub textbox: Option<(Node<'a, 'i>, [f64; 4], u8, bool)>,
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
    /// `mso-fit-shape-to-text`: the shape grows or shrinks to its text box's text.
    pub fit_to_text: bool,
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
            "mso-fit-shape-to-text" => s.fit_to_text = matches!(v, "t" | "true"),
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

/// A picture's `v:imagedata` colour controls as DrawingML adjustments. `gain` is the contrast slope (1 is none) and
/// `blacklevel` half the brightness, so Word's washout (`gain` 0.3, `blacklevel` 0.35) is DrawingML's (contrast −70%,
/// brightness +70%): a slope `k` at or under 1 is contrast `k − 1`, above 1 it is `1 − 1/k`.
fn imagedata_adjust(d: Node) -> Vec<paint::BlipAdjust> {
    let value = |k: &str| d.attribute(k).and_then(fraction).filter(|v| v.is_finite());
    let mut out = Vec::new();
    let (gain, black) = (value("gain").unwrap_or(1.0).max(0.0), value("blacklevel").unwrap_or(0.0));
    if gain != 1.0 || black != 0.0 {
        let contrast = if gain <= 1.0 { gain - 1.0 } else { 1.0 - 1.0 / gain };
        out.push(paint::BlipAdjust::Lum { bright: Pct::from_fraction((2.0 * black).clamp(-1.0, 1.0)), contrast: Pct::from_fraction(contrast) });
    }
    if on(d.attribute("grayscale"), false) {
        out.push(paint::BlipAdjust::Grayscale);
    }
    if on(d.attribute("bilevel"), false) {
        out.push(paint::BlipAdjust::BiLevel(Pct(50000)));
    }
    out
}

/// A `w:background`'s `v:background` fill over the page colour `page` (its first colour when it names none): its
/// gradient, pattern or picture; `None` for a plain colour, which the page colour is.
pub fn background_fill(background: Node, page: Option<u32>, image: &mut dyn FnMut(&str) -> Option<usize>) -> Option<Fill> {
    let el = background.children().find(|c| is(*c, V, "background"))?;
    child(el, "fill")?;
    match fill_over(el, Colour::srgb(page.unwrap_or(0xFF_FFFF)), image) {
        Fill::None | Fill::Solid(_) => None,
        f => Some(f),
    }
}

fn fill(el: Node, image: &mut dyn FnMut(&str) -> Option<usize>) -> Fill {
    fill_over(el, Colour::srgb(0xFF_FFFF), image)
}

/// `el`'s fill, its first colour `dflt` when it names none.
fn fill_over(el: Node, dflt: Colour, image: &mut dyn FnMut(&str) -> Option<usize>) -> Fill {
    let f = child(el, "fill");
    if !on(f.and_then(|f| f.attribute("on")).or(el.attribute("filled")), true) {
        return Fill::None;
    }
    let base = f.and_then(|f| f.attribute("color")).or(el.attribute("fillcolor")).and_then(colour).unwrap_or(dflt);
    let opacity = f.and_then(|f| f.attribute("opacity")).and_then(fraction);
    let solid = Fill::Solid(with_alpha(base.clone(), opacity));
    let Some(f) = f else { return solid };
    let color2 = f.attribute("color2").and_then(|v| derived(v, &base).or_else(|| colour(v))).unwrap_or_else(|| Colour::srgb(0xFFFFFF));
    let opacity2 = f.attribute((O, "opacity2")).or(f.attribute("opacity2")).and_then(fraction);
    let pct = |v: f64| Pct((v * 100000.0).round() as i32);
    match f.attribute("type").unwrap_or("solid") {
        t @ ("gradient" | "gradientRadial") => {
            let (c1, c2) = (with_alpha(base, opacity), with_alpha(color2, opacity2));
            let mut list: Vec<(f64, Colour)> = f
                .attribute("colors")
                .map(|cs| {
                    cs.split(';')
                        .take(MAX_COLOURS)
                        .filter_map(|e| {
                            let (p, c) = e.trim().split_once(' ')?;
                            Some((fraction(p)?.clamp(0.0, 1.0), colour(c)?))
                        })
                        .collect()
                })
                .unwrap_or_default();
            let two = list.len() < 2;
            if two {
                list = vec![(0.0, c1), (1.0, c2)];
            }
            // Word turns a negative angle half a turn, and its focus with it (measured).
            let a = f.attribute("angle").and_then(degrees).filter(|a| a.is_finite()).unwrap_or(0.0);
            let focus = f.attribute("focus").and_then(fraction).filter(|v| v.is_finite()).unwrap_or(0.0).clamp(-1.0, 1.0);
            let (a, focus) = if a < 0.0 { (a + 180.0, -focus) } else { (a, focus) };
            // Angle 0 runs from the first colour at the bottom up to the last, and it turns counterclockwise, from corner
            // to corner of the box.
            let theta = (270.0 - a).rem_euclid(360.0);
            let linear = paint::GradientKind::Linear { angle: Angle((theta * 60000.0).round() as i32), scaled: true };
            if t == "gradient" && two {
                let near = |v: f64| (focus.abs() - v).abs() < 1e-9;
                let (first, second) = if focus >= 0.0 { (&list[0].1, &list[1].1) } else { (&list[1].1, &list[0].1) };
                if near(0.0) || near(1.0) {
                    let (from, to) = if near(0.0) { (&list[0].1, &list[1].1) } else { (&list[1].1, &list[0].1) };
                    let stops = two_colour(from, to);
                    return Fill::Gradient(paint::Gradient {
                        stops,
                        kind: linear,
                        tile: [Pct(0); 4],
                        flip: paint::TileFlip::None,
                        rot_with_shape: true,
                    });
                }
                if near(0.5) {
                    // Out and back: the half the gradient starts in, mirrored over the other.
                    let (dx, dy) = (theta.to_radians().cos(), theta.to_radians().sin());
                    let half = |d: f64| if d > 1e-9 { Pct(50000) } else { Pct(0) };
                    let tile = [half(-dx), half(-dy), half(dx), half(dy)];
                    let stops = two_colour(first, second);
                    return Fill::Gradient(paint::Gradient { stops, kind: linear, tile, flip: paint::TileFlip::Xy, rot_with_shape: true });
                }
            }
            let mut stops = focus_stops(&list, focus);
            let kind = if t == "gradientRadial" {
                stops = stops.into_iter().rev().map(|(p, c)| (Pct(100000 - p.0), c)).collect();
                // Where it starts, and the size of that: the centre unless placed (an empty number is 0).
                let pair = |k: &str| -> Option<[f64; 2]> {
                    let v = f.attribute(k)?;
                    let mut it = v.split(',').map(|v| if v.trim().is_empty() { Some(0.0) } else { fraction(v.trim()).filter(|v| v.is_finite()) });
                    let x = it.next()??;
                    Some([x, it.next().unwrap_or(Some(0.0))?].map(|v| v.clamp(0.0, 1.0)))
                };
                let [x, y] = pair("focusposition").unwrap_or([0.5, 0.5]);
                let [w, h] = pair("focussize").unwrap_or([0.0, 0.0]);
                let focus = [pct(x), pct(y), pct((1.0 - x - w).max(0.0)), pct((1.0 - y - h).max(0.0))];
                paint::GradientKind::Path { path: paint::GradientPath::Rect, focus }
            } else {
                linear
            };
            Fill::Gradient(paint::Gradient { stops, kind, tile: [Pct(0); 4], flip: paint::TileFlip::None, rot_with_shape: true })
        }
        t @ ("tile" | "frame" | "pattern") => {
            let img = f.attribute((R, "id")).or(f.attribute((O, "relid"))).and_then(&mut *image);
            let mut adjust = Vec::new();
            if t == "pattern" {
                // The pattern's black is the second colour (its foreground), its white the first.
                adjust.push(paint::BlipAdjust::Duotone([color2.clone(), base.clone()]));
            }
            let mode = if t != "frame" {
                let one = Pct(100000);
                paint::BlipMode::Tile { tx: Emu(0), ty: Emu(0), sx: one, sy: one, flip: paint::TileFlip::None, align: paint::RectAlign::Tl }
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

/// A colour of the fill's own (`fill`, `fill darken(n)`, `fill lighten(n)`: a one-colour gradient's second colour).
fn derived(v: &str, base: &Colour) -> Option<Colour> {
    let rest = v.trim().strip_prefix("fill")?.trim();
    let super::colour::ColourBase::Srgb(rgb) = base.base else { return None };
    if rest.is_empty() {
        return Some(Colour::srgb(rgb));
    }
    let (op, n) = rest.split_once('(')?;
    let n = n.trim_end_matches(')').trim().parse::<f64>().ok().filter(|n| n.is_finite())?.clamp(0.0, 255.0).round() as u8;
    Some(Colour::srgb(match op.trim() {
        "darken" => darken(rgb, n),
        "lighten" => lighten(rgb, n),
        _ => return None,
    }))
}

/// VML's `darken(n)`: each channel scaled by `n`/255.
pub fn darken(c: u32, n: u8) -> u32 {
    [16, 8, 0].iter().map(|s| ((((c >> s) & 0xFF) * u32::from(n) + 127) / 255) << s).sum()
}

/// VML's `lighten(n)`: each channel's distance from white scaled by `n`/255.
pub fn lighten(c: u32, n: u8) -> u32 {
    0xFF_FFFF - darken(0xFF_FFFF - c, n)
}

/// The `darken(n)` (`false`) or `lighten(n)` (`true`) that makes `to` of `from`, when one does.
pub fn shade_of(from: u32, to: u32) -> Option<(bool, u8)> {
    (0..=255u8).rev().find_map(|n| {
        if darken(from, n) == to {
            Some((false, n))
        } else if lighten(from, n) == to {
            Some((true, n))
        } else {
            None
        }
    })
}

/// A two-colour gradient's stops as Word draws one: each channel runs from its lower value to its higher along the
/// two-stop blend, gamma encoded, whichever way it runs (measured), so the blend is the same for every pair.
pub(crate) fn two_colour(from: &Colour, to: &Colour) -> Vec<(Pct, Colour)> {
    use super::colour::{ColourBase, TWO_STOP_BLEND, linear_to_srgb};
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
            (Pct(k as i32 * 100000 / 16), if alpha < 1.0 { with_alpha(c, Some(alpha)) } else { c })
        })
        .collect()
}

/// Whether a page gradient is two colours as Word blends them, which VML writes as its two colours and opacities.
pub(crate) fn is_two_colour(g: &paint::Gradient) -> bool {
    matches!(g.kind, paint::GradientKind::Linear { .. })
        && matches!((g.stops.first(), g.stops.last()), (Some(a), Some(b)) if g.stops == two_colour(&a.1, &b.1))
}

/// The most `colors` entries a gradient reads.
const MAX_COLOURS: usize = 64;

/// A gradient's stops from its colours (`list`, positions from 0 to 1) and its `focus`: from 0 to 1, the colours run
/// up to `1 - focus` and back; from 0 to -1, back up to `-focus` and on. So 0 is the colours as listed, 1 (or -1)
/// reversed, 0.5 out and back, and -0.5 back and out.
fn focus_stops(list: &[(f64, Colour)], focus: f64) -> Vec<(Pct, Colour)> {
    let pct = |v: f64| Pct((v.clamp(0.0, 1.0) * 100000.0).round() as i32);
    let at = if focus >= 0.0 { 1.0 - focus } else { -focus };
    let forward = list.iter().map(|(p, c)| (p * at, c));
    let back = list.iter().rev().map(|(p, c)| (at + (1.0 - p) * (1.0 - at), c));
    let back_first = list.iter().rev().map(|(p, c)| ((1.0 - p) * at, c));
    let then = list.iter().map(|(p, c)| (at + p * (1.0 - at), c));
    let all: Vec<(f64, &Colour)> = match (focus >= 0.0, at > 0.0, at < 1.0) {
        (true, true, true) => forward.chain(back).collect(),
        (true, true, false) => forward.collect(),
        (true, false, _) => back.collect(),
        (false, true, true) => back_first.chain(then).collect(),
        (false, true, false) => back_first.collect(),
        (false, false, _) => then.collect(),
    };
    all.into_iter().map(|(p, c)| (pct(p), c.clone())).collect()
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
        // Only a number follows the marks: the rest of the file's text would go into the attribute unescaped.
        if let Some(n) = a.strip_prefix('#').and_then(|n| n.parse::<u32>().ok()) {
            return format!("adj{n}");
        }
        if let Some(n) = a.strip_prefix('@').and_then(|n| n.parse::<u32>().ok()) {
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

fn textbox<'a, 'i>(el: Node<'a, 'i>, st: &Style) -> Option<(Node<'a, 'i>, [f64; 4], u8, bool)> {
    let tb = child(el, "textbox")?;
    let content = tb.descendants().find(|n| n.tag_name().name() == "txbxContent")?;
    let ins: Vec<f64> = tb.attribute("inset").unwrap_or("").split(',').map(|v| length(v).unwrap_or(f64::NAN)).collect();
    let i = |k: usize, d: f64| ins.get(k).copied().filter(|v| !v.is_nan()).unwrap_or(d);
    let anchor = match style(tb).text_anchor.or_else(|| st.text_anchor.clone()).as_deref() {
        Some(a) if a.starts_with("middle") => 1,
        Some(a) if a.starts_with("bottom") => 2,
        _ => 0,
    };
    Some((content, [i(0, 7.2), i(1, 3.6), i(2, 7.2), i(3, 3.6)], anchor, style(tb).fit_to_text))
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
                    adjust: data.map_or_else(Vec::new, imagedata_adjust),
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
        kept_3d: el.children().any(|c| is(c, O, "extrusion")),
        text_path: text_path(el),
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

/// The most characters of a text path's string that are read.
const MAX_TEXT_PATH: usize = 1024;

/// The shape's `v:textpath`, its attributes falling back to its shapetype's; `None` when it isn't on.
fn text_path(el: Node) -> Option<Box<TextPath>> {
    let own = child(el, "textpath");
    let typed = shapetype(el).and_then(|t| child(t, "textpath"));
    let attr = |k: &str| own.and_then(|n| n.attribute(k)).or_else(|| typed.and_then(|n| n.attribute(k)));
    if !on(attr("on"), false) {
        return None;
    }
    let mut tp = TextPath {
        string: attr("string").unwrap_or("").chars().take(MAX_TEXT_PATH).collect(),
        family: "Arial".into(),
        size: 36.0,
        fit_shape: on(attr("fitshape"), false),
        fit_path: on(attr("fitpath"), false),
        trim: on(attr("trim"), false),
        ..TextPath::default()
    };
    // The shapetype's style, then the shape's over it.
    for css in [typed, own].into_iter().flatten().filter_map(|n| n.attribute("style")) {
        for decl in css.split(';') {
            let Some((k, v)) = decl.split_once(':') else { continue };
            let v = v.trim();
            match k.trim().to_ascii_lowercase().as_str() {
                "font-family" => {
                    let first = v.split(',').next().unwrap_or("").trim().trim_matches(['"', '\'']).trim();
                    if !first.is_empty() {
                        tp.family = first.chars().take(64).collect();
                    }
                }
                "font-size" => tp.size = length(v).filter(|s| s.is_finite() && *s > 0.0).map_or(tp.size, |s| s.min(1584.0)),
                "font-weight" => {
                    tp.bold = v.eq_ignore_ascii_case("bold") || v.eq_ignore_ascii_case("bolder") || v.parse::<u32>().is_ok_and(|w| w >= 600)
                }
                "font-style" => tp.italic = v.eq_ignore_ascii_case("italic") || v.eq_ignore_ascii_case("oblique"),
                _ => {}
            }
        }
    }
    Some(Box::new(tp))
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

    fn imagedata(attrs: &str) -> Vec<paint::BlipAdjust> {
        let xml = format!(r#"<v:imagedata xmlns:v="urn:schemas-microsoft-com:vml" {attrs}/>"#);
        imagedata_adjust(roxmltree::Document::parse(&xml).unwrap().root_element())
    }

    #[test]
    fn imagedata_gain_and_blacklevel_are_contrast_and_brightness() {
        let lum = |bright, contrast| paint::BlipAdjust::Lum { bright: Pct(bright), contrast: Pct(contrast) };
        assert_eq!(imagedata(""), []);
        assert_eq!(imagedata(r#"gain="65536f" blacklevel="0""#), [], "the identity pair is no adjustment");
        // Word's washout is DrawingML's: brightness +70%, contrast -70%.
        let washout = &imagedata(r#"gain="19661f" blacklevel="22938f""#)[0];
        let paint::BlipAdjust::Lum { bright, contrast } = washout else { panic!("{washout:?}") };
        assert!((bright.0 - 70_000).abs() < 20 && (contrast.0 + 70_000).abs() < 20, "{washout:?}");
        // A slope over 1 is contrast 1 - 1/k; a slope under, k - 1.
        assert_eq!(imagedata(r#"gain="131072f""#), [lum(0, 50_000)]);
        assert_eq!(imagedata(r#"gain="32768f" blacklevel="-6554f""#)[0], lum(-20_001, -50_000));
        assert_eq!(imagedata(r#"grayscale="t" bilevel="t""#), [paint::BlipAdjust::Grayscale, paint::BlipAdjust::BiLevel(Pct(50_000))]);
        assert_eq!(imagedata(r#"grayscale="f""#), []);
    }

    #[test]
    fn hostile_imagedata_controls_stay_finite() {
        for attrs in [
            r#"gain="nan""#,
            r#"gain="inf" blacklevel="-inf""#,
            r#"gain="1e308f" blacklevel="1e308""#,
            r#"gain="-5""#,
            r#"blacklevel="99999999999f""#,
        ] {
            for a in imagedata(attrs) {
                if let paint::BlipAdjust::Lum { bright, contrast } = a {
                    assert!((-100_000..=100_000).contains(&bright.0) && (-100_000..=100_000).contains(&contrast.0), "{attrs}");
                }
            }
        }
    }

    struct NoImages;

    impl super::super::read::Source for NoImages {
        fn image(&mut self, _: &str) -> Option<usize> {
            None
        }

        fn diagram(&mut self, _: &str) -> Option<Shape> {
            None
        }
    }

    fn pict(inner: &str) -> Shape {
        let xml =
            format!(r#"<w:pict xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:v="{V}" xmlns:o="{O}">{inner}</w:pict>"#);
        let doc = roxmltree::Document::parse(&xml).unwrap();
        let mut ctx = Ctx { source: &mut NoImages, unknown_presets: Vec::new() };
        read(doc.root_element(), &mut ctx).expect("a shape").shape
    }

    const WATERMARK_TYPE: &str = r#"<v:shapetype id="_x0000_t136" coordsize="21600,21600" o:spt="136" adj="10800" path="m@7,l@8,m@5,21600l@6,21600e"><v:path textpathok="t"/><v:textpath on="t" fitshape="t"/></v:shapetype>"#;

    #[test]
    fn a_text_path_reads_its_string_font_and_fitting() {
        let s = pict(&format!(
            r##"{WATERMARK_TYPE}<v:shape type="#_x0000_t136" style="width:468pt;height:117pt;rotation:315" fillcolor="silver" stroked="f"><v:fill opacity=".5"/><v:textpath style="font-family:&quot;Calibri&quot;;font-size:1pt;font-weight:bold;font-style:italic" string="DRAFT" trim="t"/></v:shape>"##
        ));
        let sp = s.sp().unwrap();
        let tp = sp.text_path.as_deref().expect("a text path");
        assert_eq!((tp.string.as_str(), tp.family.as_str(), tp.size), ("DRAFT", "Calibri", 1.0));
        assert!(tp.bold && tp.italic && tp.fit_shape && tp.trim && !tp.fit_path);
        assert!(!sp.kept_3d, "a text path is not 3-D");
        assert_eq!(s.xfrm.rot, Angle(315 * 60000));
        // Turned off on the shape, it is no text path.
        let off = pict(&format!(
            r##"{WATERMARK_TYPE}<v:shape type="#_x0000_t136" style="width:10pt;height:10pt"><v:textpath on="f" string="X"/></v:shape>"##
        ));
        assert!(off.sp().unwrap().text_path.is_none());
        let plain = pict(r#"<v:rect style="width:10pt;height:10pt"/>"#);
        assert!(plain.sp().unwrap().text_path.is_none());
    }

    #[test]
    fn a_hostile_text_path_is_bounded() {
        let long = "W".repeat(100_000);
        let s = pict(&format!(
            r##"{WATERMARK_TYPE}<v:shape type="#_x0000_t136" style="width:10pt;height:10pt"><v:textpath style="font-family:'{long}';font-size:1e300pt" string="{long}"/></v:shape>"##
        ));
        let tp = s.sp().unwrap().text_path.clone().unwrap();
        assert_eq!(tp.string.chars().count(), MAX_TEXT_PATH);
        assert_eq!(tp.family.chars().count(), 64);
        assert!(tp.size.is_finite() && tp.size <= 1584.0);
        for size in ["nan", "-4pt", "0"] {
            let s = pict(&format!(r##"{WATERMARK_TYPE}<v:shape type="#_x0000_t136"><v:textpath style="font-size:{size}" string="X"/></v:shape>"##));
            assert_eq!(s.sp().unwrap().text_path.as_ref().unwrap().size, 36.0, "{size}");
        }
    }

    fn gradient(fill: &str) -> paint::Gradient {
        let xml = format!(r##"<v:rect xmlns:v="{V}" xmlns:o="{O}" fillcolor="#ff0000"><v:fill type="gradient" color2="#0000ff" {fill}/></v:rect>"##);
        match super::fill(roxmltree::Document::parse(&xml).unwrap().root_element(), &mut |_| None) {
            Fill::Gradient(g) => g,
            f => panic!("{f:?}"),
        }
    }

    fn linear(g: &paint::Gradient) -> f64 {
        let paint::GradientKind::Linear { angle, scaled: true } = g.kind else { panic!("{:?}", g.kind) };
        angle.0 as f64 / 60000.0
    }

    #[test]
    fn vml_gradients_turn_and_focus_as_word_draws_them() {
        let red = Colour::srgb(0xFF_0000);
        let blue = Colour::srgb(0x00_00FF);
        // Angle 0 runs up from the first colour; angles turn counterclockwise.
        let g = gradient(r#"angle="0""#);
        assert_eq!(linear(&g), 270.0);
        assert_eq!((g.stops.len(), &g.stops[0].1, &g.stops[16].1), (17, &red, &blue));
        assert_eq!(linear(&gradient(r#"angle="90""#)), 180.0);
        assert_eq!(linear(&gradient(r#"angle="45""#)), 225.0);
        // Full focus reverses the colours; a negative angle is half a turn on with its focus reversed.
        assert_eq!(gradient(r#"focus="100%""#).stops[0].1, blue);
        let g = gradient(r#"angle="-45" focus="50%""#);
        assert_eq!(linear(&g), 135.0);
        assert_eq!((&g.stops[0].1, &g.stops[16].1, g.flip), (&blue, &red, paint::TileFlip::Xy));
        // Out and back: the first half of the box along the gradient's way (up and to the left), mirrored.
        assert_eq!(g.tile, [Pct(50000), Pct(0), Pct(0), Pct(50000)]);
        // Each channel blends the same way between any two colours: halfway is 73% of the way up from its low end.
        let mid = |c: &Colour| match c.base {
            super::super::colour::ColourBase::Srgb(v) => v,
            _ => panic!(),
        };
        assert_eq!(mid(&gradient("").stops[8].1), 0xBA_00BA);
        // A list of colours is drawn as listed, focus reversing it.
        let g = gradient(r#"colors="0 #ff0000;.5 #00ff00;1 #0000ff" focus="100%""#);
        assert_eq!(g.stops.iter().map(|(p, c)| (p.0, mid(c))).collect::<Vec<_>>(), [(0, 0xFF), (50000, 0xFF00), (100000, 0xFF_0000)]);
        let g = gradient(r#"colors="0 #ff0000;1 #0000ff" focus="50%""#);
        assert_eq!(g.stops.iter().map(|(p, _)| p.0).collect::<Vec<_>>(), [0, 50000, 50000, 100000]);
    }

    #[test]
    fn hostile_vml_gradients_are_bounded() {
        let many = (0..10_000).map(|i| format!("{} #00ff00", i as f64 / 10_000.0)).collect::<Vec<_>>().join(";");
        assert!(gradient(&format!(r#"colors="{many}""#)).stops.len() <= 2 * MAX_COLOURS);
        for attrs in [r#"angle="nan" focus="inf""#, r#"angle="1e308" focus="-1e308""#, r#"angle="-1e308fd""#] {
            let g = gradient(attrs);
            assert!((0.0..360.0).contains(&linear(&g)), "{attrs}");
            assert!(g.stops.iter().all(|(p, _)| (0..=100000).contains(&p.0)), "{attrs}");
        }
    }

    #[test]
    fn a_pattern_fill_draws_its_black_in_the_second_colour() {
        let xml =
            format!(r##"<v:rect xmlns:v="{V}" xmlns:r="{R}" fillcolor="#2e75b6"><v:fill type="pattern" r:id="rId1" color2="#ffe699"/></v:rect>"##);
        let Fill::Blip(b) = super::fill(roxmltree::Document::parse(&xml).unwrap().root_element(), &mut |_| Some(0)) else { panic!() };
        assert_eq!(b.adjust[0], paint::BlipAdjust::Duotone([Colour::srgb(0xFF_E699), Colour::srgb(0x2E_75B6)]));
        let paint::BlipMode::Tile { sx, sy, .. } = b.mode else { panic!() };
        assert_eq!((sx, sy), (Pct(100000), Pct(100000)), "a tile pixel to a pixel, as a picture tile");
    }

    #[test]
    fn a_page_background_fill_is_its_gradient_pattern_or_picture() {
        let read = |inner: &str| {
            let xml = format!(
                r#"<w:background xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:v="{V}" w:color="00FF00">{inner}</w:background>"#
            );
            background_fill(roxmltree::Document::parse(&xml).unwrap().root_element(), Some(0x00_FF00), &mut |_| Some(2))
        };
        assert_eq!(read(""), None);
        assert_eq!(read(r#"<v:background fillcolor="red"/>"#), None, "a colour alone is the page colour");
        assert_eq!(read(r#"<v:background><v:fill type="solid"/></v:background>"#), None);
        let Some(Fill::Gradient(g)) = read(r#"<v:background><v:fill type="gradient" color2="black"/></v:background>"#) else { panic!() };
        assert_eq!(g.stops[0].1, Colour::srgb(0x00_FF00), "the page colour is its first colour");
        assert!(matches!(
            read(r#"<v:background><v:fill type="tile" r:id="rId9" xmlns:r="{R}"/></v:background>"#.replace("{R}", R).as_str()),
            Some(Fill::Blip(_))
        ));
    }

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
