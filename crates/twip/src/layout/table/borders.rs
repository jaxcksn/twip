//! Border conflicts and border styles

use crate::docx::model::*;
use crate::docx::props::Border;
use crate::layout::fragment::Item;
use crate::layout::units::EPS;

// Borders
fn style_factor(style: &str) -> f64 {
    match style {
        "double" => 3.0,
        "triple" => 3.5,
        "dashed" | "dotted" | "dashSmallGap" | "dotDash" | "dotDotDash" => 0.75,
        _ => 1.0,
    }
}

const ORDER: [&str; 27] = [
    "nil",
    "none",
    "single",
    "thick",
    "double",
    "dotted",
    "dashed",
    "dotDash",
    "dotDotDash",
    "triple",
    "thinThickSmallGap",
    "thickThinSmallGap",
    "thinThickThinSmallGap",
    "thinThickMediumGap",
    "thickThinMediumGap",
    "thinThickThinMediumGap",
    "thinThickLargeGap",
    "thickThinLargeGap",
    "thinThickThinLargeGap",
    "wave",
    "doubleWave",
    "dashSmallGap",
    "dashDotStroked",
    "threeDEmboss",
    "threeDEngrave",
    "outset",
    "inset",
];

fn style_rank(style: &str) -> usize {
    ORDER.iter().position(|s| *s == style).unwrap_or(2)
}

/// A border a style draws: not an art border.
pub(in crate::layout) fn drawn(b: &Border) -> bool {
    art(b).is_none()
}

fn art(b: &Border) -> Option<f64> {
    (!ORDER.contains(&b.style.as_str())).then(|| (b.width * 8.0).min(255.0))
}

pub(super) fn conflict(a: (Option<&Border>, bool), b: (Option<&Border>, bool)) -> Option<Border> {
    winner(a, b).cloned()
}

fn winner<'a>(a: (Option<&'a Border>, bool), b: (Option<&'a Border>, bool)) -> Option<&'a Border> {
    let real = |x: Option<&'a Border>| x.filter(|b| b.visible());
    let (ra, rb) = (real(a.0), real(b.0));
    match (ra, rb) {
        (None, None) => None,
        (Some(x), None) => Some(x),
        (None, Some(y)) => Some(y),
        (Some(x), Some(y)) => {
            if a.1 != b.1 {
                return Some(if a.1 { x } else { y });
            }
            let (wx, wy) = (x.width * style_factor(&x.style), y.width * style_factor(&y.style));
            if (wx - wy).abs() > EPS {
                return Some(if wx > wy { x } else { y });
            }
            let (sx, sy) = (style_rank(&x.style), style_rank(&y.style));
            if sx != sy {
                return Some(if sx > sy { x } else { y });
            }
            let key = |c: Option<u32>| {
                let c = c.unwrap_or(0);
                let (r, g, b) = ((c >> 16) & 255, (c >> 8) & 255, c & 255);
                (r + b + 2 * g, b + 2 * g, g)
            };
            Some(if key(x.color) <= key(y.color) { x } else { y })
        }
    }
}

pub(super) fn bw(b: &Option<Border>) -> f64 {
    b.as_ref().filter(|b| b.visible()).map_or(0.0, thick)
}

pub(in crate::layout) fn thick(b: &Border) -> f64 {
    stripes(b).iter().sum()
}

pub(super) fn space_h(b: &Option<Border>) -> f64 {
    let Some(b) = b.as_ref().filter(|b| b.visible()) else { return 0.0 };
    if let Some(a) = art(b) {
        return b.space.clamp(0.0, 31.0) + if b.shadow { 2.0 } else { 1.0 } * a;
    }
    let w = (b.width.clamp(0.25, 12.0) * 20.0 + 1e-6).floor() / 20.0;
    let line = match b.style.as_str() {
        "wave" if (b.width - 1.5).abs() < EPS => 3.75,
        "wave" | "dashDotStroked" => 3.0,
        "doubleWave" => 5.25,
        "threeDEmboss" | "threeDEngrave" => w + if b.width < 3.0 { 1.5 } else { 3.0 },
        _ if stripes(b).len() == 1 => w,
        _ => thick(b),
    };
    b.space.clamp(0.0, 31.0) + if b.shadow { 2.0 } else { 1.0 } * line
}

pub(super) fn box_h(b: &Option<Border>) -> f64 {
    match b.as_ref().filter(|b| b.visible() && art(b).is_some()) {
        Some(b) => b.space.clamp(0.0, 31.0) + if b.shadow { 2.0 } else { 1.0 } * ((b.width * 20.0 + 1e-6).floor() / 20.0).max(0.05),
        None => space_h(b),
    }
}

fn stripes(b: &Border) -> Vec<f64> {
    let (w, s) = (b.width, b.style.as_str());
    let (thin, gap, thick) = if s.ends_with("SmallGap") {
        (0.75, 0.75, w)
    } else if s.ends_with("MediumGap") {
        (w / 2.0, w / 2.0, w)
    } else if s.ends_with("LargeGap") {
        (0.75, w, 1.5)
    } else {
        (w, w, w)
    };
    if let Some(a) = art(b) {
        return vec![w, (a - w).max(0.0)];
    }
    match s {
        "threeDEmboss" | "threeDEngrave" => {
            let o = if w < 3.0 { 0.75 } else { 1.5 };
            vec![o, w, o]
        }
        "double" => vec![w, w, w],
        "triple" => vec![w; 5],
        _ if s.starts_with("thinThickThin") => vec![thin, gap, thick, gap, thin],
        _ if s.starts_with("thinThick") => vec![thick, gap, thin],
        _ if s.starts_with("thickThin") => vec![thin, gap, thick],
        _ => vec![w],
    }
}

pub(in crate::layout) fn edge(out: &mut Vec<Item>, rect: [f64; 4], horizontal: bool, far: bool, b: &Border) {
    let colour = b.color.unwrap_or(0);
    let [x0, y0, x1, y1] = rect;
    let st = stripes(b);
    if let Some(cs) = three_d(b) {
        let mut at = if horizontal { y0 } else { x0 };
        for (w, colour) in st.iter().zip(cs) {
            out.push(Item::rect(if horizontal { [x0, at, x1, at + w] } else { [at, y0, at + w, y1] }, colour));
            at += w;
        }
        return;
    }
    if st.len() == 1 && !dash_pattern(&b.style).is_empty() {
        out.extend(dashes(&b.style, rect, horizontal).into_iter().map(|rect| Item::rect(rect, colour)));
        return;
    }
    match b.style.as_str() {
        "wave" => return wave(out, rect, horizontal, 0.0, colour),
        "doubleWave" => {
            let gap = if horizontal { y1 - y0 } else { x1 - x0 }.max(0.25) * 2.0;
            wave(out, rect, horizontal, -gap / 2.0, colour);
            return wave(out, rect, horizontal, gap / 2.0, colour);
        }
        // Outset is light at the top and left and dark at the bottom and right; inset the other way.
        "outset" | "inset" => {
            let [light, _, dark] = tones(b.color.unwrap_or(0xA0A0A0));
            let lit = (b.style == "outset") != far;
            out.push(Item::rect(rect, if lit { light } else { dark }));
            return;
        }
        _ => {}
    }
    if st.len() == 1 {
        out.push(Item::rect(rect, colour));
        return;
    }
    let mut at = if horizontal { y0 } else { x0 };
    for (k, w) in st.iter().enumerate() {
        if k % 2 == 0 {
            out.push(Item::rect(if horizontal { [x0, at, x1, at + w] } else { [at, y0, at + w, y1] }, colour));
        }
        at += w;
    }
}

pub(in crate::layout) fn dash_pattern(style: &str) -> &'static [f64] {
    match style {
        "dotted" => &[1.0, 1.0],
        "dashed" => &[3.0, 1.0],
        "dotDash" => &[3.0, 1.0, 1.0, 1.0],
        "dotDotDash" => &[3.0, 1.0, 1.0, 1.0, 1.0, 1.0],
        "dashSmallGap" => &[4.0, 1.0],
        _ => &[],
    }
}

pub(in crate::layout) fn dashes(style: &str, rect: [f64; 4], horizontal: bool) -> Vec<[f64; 4]> {
    let [x0, y0, x1, y1] = rect;
    let unit = if horizontal { y1 - y0 } else { x1 - x0 };
    let (start, end) = if horizontal { (x0, x1) } else { (y0, y1) };
    if unit <= EPS {
        return vec![rect];
    }
    let mut out = Vec::new();
    let mut at = start;
    for (k, len) in dash_pattern(style).iter().cycle().enumerate() {
        if at >= end - EPS {
            break;
        }
        let to = (at + len * unit).min(end);
        if k % 2 == 0 {
            out.push(if horizontal { [at, y0, to, y1] } else { [x0, at, x1, to] });
        }
        at = to;
    }
    out
}

/// A wave along the edge, `shift` from its middle: steps of a sine as thin as the line, two line widths high.
fn wave(out: &mut Vec<Item>, rect: [f64; 4], horizontal: bool, shift: f64, colour: u32) {
    let [x0, y0, x1, y1] = rect;
    let t = if horizontal { y1 - y0 } else { x1 - x0 }.max(0.25);
    let mid = if horizontal { (y0 + y1) / 2.0 } else { (x0 + x1) / 2.0 } + shift;
    let (start, end) = if horizontal { (x0, x1) } else { (y0, y1) };
    let (amp, period, step) = (t, 6.0 * t, (t / 2.0).max(0.25));
    let mut at = start;
    while at < end - EPS {
        let to = (at + step).min(end);
        let c = mid + amp * (std::f64::consts::TAU * (at - start) / period).sin();
        out.push(Item::rect(if horizontal { [at, c - t / 2.0, to, c + t / 2.0] } else { [c - t / 2.0, at, c + t / 2.0, to] }, colour));
        at = to;
    }
}

fn three_d(b: &Border) -> Option<[u32; 3]> {
    let engrave = match b.style.as_str() {
        "threeDEmboss" => false,
        "threeDEngrave" => true,
        _ => return None,
    };
    let [light, middle, dark] = tones(b.color.unwrap_or(0xA0A0A0));
    Some(if engrave { [dark, middle, light] } else { [light, middle, dark] })
}

/// A 3-D border's light, middle and dark tones of its color.
fn tones(c: u32) -> [u32; 3] {
    let (light, middle, dark) = match c {
        0x000000 => (0xC0C0C0, 0x808080, 0x000000),
        0xFFFFFF => (0xFFFFFF, 0xC0C0C0, 0x808080),
        _ => {
            let (h, l, s) = to_hls(c);
            let l2 = if l < 60 {
                l + 40
            } else if l < 120 {
                l
            } else {
                l - 40
            }
            .clamp(0, 255);
            let mid = from_hls(h, l2, s);
            let shade = from_hls(h, (l2 - 60).max(0), s);
            let avg = (0..3).fold(0, |acc, k| {
                let (a, b) = ((mid >> (8 * k)) & 0xFF, (shade >> (8 * k)) & 0xFF);
                acc | (((a * 8 + b * 8 + 8) >> 4) << (8 * k))
            });
            (from_hls(h, (l2 + 40).min(240), s), avg, from_hls(h, (l2 - 40).max(0), s))
        }
    };
    [light, middle, dark]
}

fn to_hls(c: u32) -> (i64, i64, i64) {
    let (r, g, b) = (i64::from((c >> 16) & 0xFF), i64::from((c >> 8) & 0xFF), i64::from(c & 0xFF));
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let l = ((max + min) * 240 + 255) / 510;
    if max == min {
        return (160, l, 0);
    }
    let d = max - min;
    let s = if l < 121 { ((max + min) / 2 + d * 240) / (max + min) } else { ((510 - max - min) / 2 + d * 240) / (510 - max - min) };
    let part = |v: i64| (d / 2 + (max - v) * 40) / d;
    let h = if r == max {
        part(b) - part(g)
    } else if g == max {
        part(r) - part(b) + 80
    } else {
        part(g) - part(r) + 160
    };
    (h.rem_euclid(240), l, s)
}

fn from_hls(h: i64, l: i64, s: i64) -> u32 {
    let scale = |v: i64| ((v * 255 + 120) / 240).clamp(0, 255) as u32;
    if s == 0 {
        let v = scale(l);
        return (v << 16) | (v << 8) | v;
    }
    let m2 = if l < 121 { ((s + 240) * l + 120) / 240 } else { l - (l * s + 120) / 240 + s };
    let m1 = 2 * l - m2;
    let channel = |h: i64| {
        let h = h.rem_euclid(240);
        if h < 40 {
            m1 + ((m2 - m1) * h + 20) / 40
        } else if h < 120 {
            m2
        } else if h < 160 {
            m1 + ((m2 - m1) * (160 - h) + 20) / 40
        } else {
            m1
        }
    };
    (scale(channel(h + 80)) << 16) | (scale(channel(h)) << 8) | scale(channel(h - 80))
}

/// The indent a table is placed by: an unset one (none, or not dxa) places nothing.
pub(super) fn placed_ind(t: &Table) -> f64 {
    if t.ind_set { t.ind } else { 0.0 }
}

pub(super) fn half_border(w: f64) -> f64 {
    let tw = (w * 20.0).round() as i64;
    ((tw + 1) / 2) as f64 / 20.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dotted_border_is_drawn_as_dots() {
        let b = Border { style: "dotted".into(), width: 1.0, space: 0.0, shadow: false, color: None };
        let mut out = Vec::new();
        edge(&mut out, [0.0, 0.0, 9.0, 1.0], true, false, &b);
        let xs: Vec<f64> = out.iter().map(|i| if let Item::Rect { rect, .. } = i { rect[0] } else { -1.0 }).collect();
        assert_eq!(xs, vec![0.0, 2.0, 4.0, 6.0, 8.0]);
    }

    #[test]
    fn every_border_style_draws_its_own_way() {
        let b = |style: &str| Border { style: style.into(), width: 1.0, space: 0.0, shadow: false, color: Some(0x336699) };
        let drawn = |style: &str, far: bool| {
            let mut out = Vec::new();
            edge(&mut out, [0.0, 0.0, 30.0, thick(&b(style))], true, far, &b(style));
            out.iter()
                .map(|i| if let Item::Rect { rect, colour } = i { (rect.map(|v| (v * 100.0).round() as i64), *colour) } else { ([0; 4], 0) })
                .collect::<Vec<_>>()
        };
        let styles = [
            "single",
            "double",
            "triple",
            "dotted",
            "dashed",
            "dotDash",
            "dotDotDash",
            "thinThickSmallGap",
            "thickThinSmallGap",
            "thinThickThinSmallGap",
            "wave",
            "doubleWave",
            "threeDEmboss",
            "threeDEngrave",
            "outset",
            "inset",
        ];
        let all: Vec<_> = styles.iter().map(|s| drawn(s, false)).collect();
        for (i, a) in all.iter().enumerate() {
            for (j, b) in all.iter().enumerate().skip(i + 1) {
                assert_ne!(a, b, "{} and {} draw alike", styles[i], styles[j]);
            }
        }
        let ys: std::collections::BTreeSet<i64> = drawn("wave", false).iter().map(|(r, _)| r[1]).collect();
        assert!(ys.len() > 3, "a wave goes up and down");
        assert_ne!(drawn("outset", false), drawn("outset", true), "outset is light on one side and dark on the other");
        assert_eq!(drawn("outset", false), drawn("inset", true));
    }

    #[test]
    fn a_3d_border_is_three_lines_in_light_middle_and_dark() {
        let b = |style: &str| Border { style: style.into(), width: 1.0, space: 0.0, shadow: false, color: Some(0) };
        let drawn = |style: &str| {
            let mut out = Vec::new();
            edge(&mut out, [0.0, 0.0, 10.0, 2.5], true, false, &b(style));
            out.iter().map(|i| if let Item::Rect { rect, colour, .. } = i { (rect[3] - rect[1], *colour) } else { (0.0, 0) }).collect::<Vec<_>>()
        };
        assert_eq!(drawn("threeDEmboss"), vec![(0.75, 0xC0C0C0), (1.0, 0x808080), (0.75, 0x000000)]);
        assert_eq!(drawn("threeDEngrave"), vec![(0.75, 0x000000), (1.0, 0x808080), (0.75, 0xC0C0C0)]);
        assert_eq!(from_hls(to_hls(0x808080).0, to_hls(0x808080).1, to_hls(0x808080).2), 0x808080);
    }
}
