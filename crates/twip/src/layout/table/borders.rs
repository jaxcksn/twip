//! Border conflicts and border styles

use crate::docx::model::*;
use crate::docx::props::Border;
use crate::layout::fragment::Item;
use crate::layout::units::EPS;
use std::rc::Rc;

use crate::drawing::colour::Rgba;
use crate::drawing::geom::{Geom, GeomPath, PathFill, Seg};
use crate::drawing::paint::{Join, Paint, ResolvedEffects, ResolvedLine};
use crate::layout::fragment::ShapeItem;

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

/// sz 1, in points.
const HAIRLINE: f64 = 0.125;

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
    if b.width <= 0.0 {
        return b.space.clamp(0.0, 31.0);
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
    // Hairline: drawn as sz 1
    if b.width < HAIRLINE && drawn(b) {
        let h = Border { width: HAIRLINE, ..b.clone() };
        let ([x0, y0, x1, y1], t) = (rect, thick(&h));
        return edge(out, if horizontal { [x0, y0, x1, y0 + t] } else { [x0, y0, x0 + t, y1] }, horizontal, far, &h);
    }
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
        "wave" => return wave(out, rect, horizontal, 1, colour),
        "doubleWave" => return wave(out, rect, horizontal, 2, colour),
        "dashDotStroked" => return hatch(out, rect, horizontal, colour),
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
        if at >= end - EPS || k >= 2 * MAX_SEGMENTS as usize {
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

/// Most segments a stroked border edge is made of; a longer edge gets longer segments.
const MAX_SEGMENTS: f64 = 4096.0;
/// The thinnest line a stroked border keeps, in points (a fixed width the stroke is not scaled below).
const MIN_STROKE: f64 = 0.25;
/// A wave's crest-to-trough height, half period and line width, in points, at Word's size.
const WAVE_HEIGHT: f64 = 2.25;
const WAVE_HALF: f64 = 2.5;
const WAVE_STROKE: f64 = 0.4;
/// A `dashDotStroked` band's height, the run of a stroke along the edge, its width, and the pattern's period with
/// its strokes' offsets in it, in points, at Word's size: five strokes 0.72 apart, a gap, one stroke, a gap.
const HATCH_BAND: f64 = 3.0;
const HATCH_RUN: f64 = 1.4;
const HATCH_STROKE: f64 = 0.3;
const HATCH_PERIOD: f64 = 5.76;
const HATCH_AT: [f64; 6] = [0.0, 0.72, 1.44, 2.16, 2.88, 4.32];

/// A stroked path along an edge, centred across `rect` in a band of `band` (at most the rect's own thickness: the
/// space layout reserved). `segs` are in (along, across) points from the band's corner.
pub(in crate::layout) fn stroked(out: &mut Vec<Item>, rect: [f64; 4], horizontal: bool, band: f64, width: f64, colour: u32, segs: Vec<Seg>) {
    let [x0, y0, x1, y1] = rect;
    let (a0, a1, c0, c1) = if horizontal { (x0, x1, y0, y1) } else { (y0, y1, x0, x1) };
    let at = (c0 + c1 - band) / 2.0;
    let swap = |p: [f64; 2]| if horizontal { p } else { [p[1], p[0]] };
    let segs: Vec<Seg> = segs
        .into_iter()
        .map(|s| match s {
            Seg::Move(p) => Seg::Move(swap(p)),
            Seg::Line(p) => Seg::Line(swap(p)),
            other => other,
        })
        .collect();
    let rect = if horizontal { [a0, at, a1, at + band] } else { [at, a0, at + band, a1] };
    let [r0, r1, r2, r3] = rect;
    let geom = Geom {
        paths: vec![GeomPath { segs, fill: PathFill::None, stroke: true }],
        text_rect: [0.0, 0.0, r2 - r0, r3 - r1],
        handles: vec![],
        cxn: vec![],
    };
    let paint = Paint::Solid(Rgba { r: (colour >> 16) as u8, g: (colour >> 8) as u8, b: colour as u8, a: 255 });
    let shape = Rc::new(ShapeItem {
        rect,
        rot: 0.0,
        flip_h: false,
        flip_v: false,
        geom: Rc::new(geom),
        fill: Paint::None,
        line: ResolvedLine { width, paint, join: Join::Round, ..ResolvedLine::none() },
        effects: ResolvedEffects::default(),
        behind: false,
        float: 0,
        path: Vec::new(),
        picture: false,
        ink: true,
        decoration: true,
        clip_bottom: None,
        text_float: 0,
        of: Default::default(),
        text_path: None,
    });
    // Two cells sharing an edge each draw it: once is enough.
    if !out.iter().rev().take(8).any(|i| matches!(i, Item::Shape(s) if **s == *shape)) {
        out.push(Item::Shape(shape));
    }
}

/// `waves` zigzags of a wave border along the edge, stacked across it; the rect's thickness scales Word's size down
/// when it is smaller.
fn wave(out: &mut Vec<Item>, rect: [f64; 4], horizontal: bool, waves: usize, colour: u32) {
    let [x0, y0, x1, y1] = rect;
    let (len, across) = if horizontal { (x1 - x0, y1 - y0) } else { (y1 - y0, x1 - x0) };
    if !(len > EPS && across > 0.0) {
        return;
    }
    // The line keeps its width as far as the thickness allows; the waves' height and length take the rest.
    let stroke = WAVE_STROKE.min(across / 3.0).max(MIN_STROKE.min(across));
    let height = ((across - stroke) / waves as f64).min(WAVE_HEIGHT);
    let half = (WAVE_HALF * height / WAVE_HEIGHT).max(len / MAX_SEGMENTS);
    let mut segs = Vec::new();
    for w in 0..waves {
        let mid = stroke / 2.0 + height * (w as f64 + 0.5);
        let crest = |i: f64| mid + if i % 2.0 == 0.0 { -height / 2.0 } else { height / 2.0 };
        let steps = (len / half).floor();
        segs.push(Seg::Move([0.0, crest(0.0)]));
        for i in 1..=steps as usize {
            segs.push(Seg::Line([i as f64 * half, crest(i as f64)]));
        }
        let rest = len - steps * half;
        if rest > EPS {
            let (from, to) = (crest(steps), crest(steps + 1.0));
            segs.push(Seg::Line([len, from + (to - from) * rest / half]));
        }
    }
    stroked(out, rect, horizontal, height * waves as f64 + stroke, stroke, colour, segs);
}

/// A `dashDotStroked` border: slanted strokes in Word's pattern filling a band.
fn hatch(out: &mut Vec<Item>, rect: [f64; 4], horizontal: bool, colour: u32) {
    let [x0, y0, x1, y1] = rect;
    let (len, across) = if horizontal { (x1 - x0, y1 - y0) } else { (y1 - y0, x1 - x0) };
    if !(len > EPS && across > 0.0) {
        return;
    }
    let k = (across / HATCH_BAND).min(1.0);
    let (band, run, stroke) = (HATCH_BAND * k, HATCH_RUN * k, HATCH_STROKE.min(across / 3.0).max(MIN_STROKE.min(across)));
    // A very long edge gets a longer period rather than more strokes.
    let scale = (len / HATCH_PERIOD * HATCH_AT.len() as f64 / MAX_SEGMENTS).max(1.0);
    let mut segs = Vec::new();
    let mut base = 0.0;
    while base < len - EPS {
        for at in HATCH_AT {
            let u = base + at * scale;
            if u >= len - EPS {
                break;
            }
            // A stroke runs from the band's far side to its near side, cut where it would pass the edge's end.
            let run = run.min(len - u);
            segs.push(Seg::Move([u, band]));
            segs.push(Seg::Line([u + run, band * (1.0 - run / (HATCH_RUN * k))]));
        }
        base += HATCH_PERIOD * scale;
    }
    stroked(out, rect, horizontal, band, stroke, colour, segs);
}

/// A shadowed border's shadow: black, as thick as the border, below a bottom edge and to the right of a right one.
/// It starts a thickness along when the box has a border before it (`lead`: the left of a bottom edge, the top of a
/// right one) and runs a thickness past the edge's end when it has one after it (`trail`).
pub(in crate::layout) fn shadow(out: &mut Vec<Item>, rect: [f64; 4], horizontal: bool, far: bool, b: &Border, lead: bool, trail: bool) {
    if !b.shadow || !far || !drawn(b) {
        return;
    }
    let [x0, y0, x1, y1] = rect;
    let t = thick(b);
    let (from, to) = (if lead { t } else { 0.0 }, if trail { t } else { 0.0 });
    out.push(Item::rect(if horizontal { [x0 + from, y1, x1 + to, y1 + t] } else { [x1, y0 + from, x1 + t, y1] }, 0));
}

/// A stroked border edge cut to `clip` (page coordinates), or none if nothing of it is inside: its path is clipped
/// segment by segment, a stroke's own width left to overhang as an uncut line's would.
pub(in crate::layout) fn clip_stroke(s: &ShapeItem, clip: [f64; 4]) -> Option<ShapeItem> {
    let [ox, oy, ex, ey] = s.rect;
    let [cx0, cy0, cx1, cy1] = [clip[0].max(ox), clip[1].max(oy), clip[2].min(ex), clip[3].min(ey)];
    if !(cx1 > cx0 && cy1 > cy0) {
        return None;
    }
    let mut segs = Vec::new();
    let mut at: Option<[f64; 2]> = None;
    let mut pen_down = false;
    let start = |p: [f64; 2], segs: &mut Vec<Seg>| segs.push(Seg::Move([p[0] + ox - cx0, p[1] + oy - cy0]));
    let run = |from: [f64; 2], to: [f64; 2], segs: &mut Vec<Seg>, pen_down: &mut bool| {
        // Liang-Barsky against the clip in local coordinates.
        let (lo, hi) = ([cx0 - ox, cy0 - oy], [cx1 - ox, cy1 - oy]);
        let (mut t0, mut t1) = (0.0_f64, 1.0_f64);
        for axis in 0..2 {
            let d = to[axis] - from[axis];
            let (a, b) = (lo[axis] - from[axis], hi[axis] - from[axis]);
            if d.abs() < 1e-12 {
                if a > 0.0 || b < 0.0 {
                    *pen_down = false;
                    return;
                }
            } else {
                let (mut u0, mut u1) = (a / d, b / d);
                if u0 > u1 {
                    std::mem::swap(&mut u0, &mut u1);
                }
                t0 = t0.max(u0);
                t1 = t1.min(u1);
            }
        }
        if t0 > t1 {
            *pen_down = false;
            return;
        }
        let at = |t: f64| [from[0] + (to[0] - from[0]) * t, from[1] + (to[1] - from[1]) * t];
        if !*pen_down || t0 > 0.0 {
            start(at(t0), segs);
        }
        segs.push(Seg::Line([at(t1)[0] + ox - cx0, at(t1)[1] + oy - cy0]));
        *pen_down = t1 >= 1.0;
    };
    for seg in &s.geom.paths.first()?.segs {
        match *seg {
            Seg::Move(p) => {
                at = Some(p);
                pen_down = false;
            }
            Seg::Line(p) => {
                if let Some(from) = at {
                    run(from, p, &mut segs, &mut pen_down);
                }
                at = Some(p);
            }
            _ => {}
        }
    }
    if segs.is_empty() {
        return None;
    }
    let geom = Geom {
        paths: vec![GeomPath { segs, fill: PathFill::None, stroke: true }],
        text_rect: [0.0, 0.0, cx1 - cx0, cy1 - cy0],
        handles: vec![],
        cxn: vec![],
    };
    Some(ShapeItem { rect: [cx0, cy0, cx1, cy1], geom: Rc::new(geom), ..s.clone() })
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

    /// What an edge draws, comparable: rects as rounded numbers, strokes as their segment lists.
    fn described(out: &[Item]) -> Vec<(Vec<i64>, u32)> {
        out.iter()
            .map(|i| match i {
                Item::Rect { rect, colour } => (rect.iter().map(|v| (v * 100.0).round() as i64).collect(), *colour),
                Item::Shape(s) => {
                    let pts = s.geom.paths[0].segs.iter().flat_map(|g| match g {
                        Seg::Move(p) | Seg::Line(p) => p.to_vec(),
                        _ => vec![],
                    });
                    (s.rect.iter().copied().chain(pts).map(|v| (v * 100.0).round() as i64).collect(), 1)
                }
                _ => (vec![], 2),
            })
            .collect()
    }

    fn edge_of(style: &str, far: bool, horizontal: bool, len: f64) -> Vec<Item> {
        let b = Border { style: style.into(), width: 1.0, space: 0.0, shadow: false, color: Some(0x336699) };
        let t = thick(&b);
        let mut out = Vec::new();
        edge(&mut out, if horizontal { [0.0, 0.0, len, t] } else { [0.0, 0.0, t, len] }, horizontal, far, &b);
        out
    }

    #[test]
    fn every_border_style_draws_its_own_way() {
        let styles = [
            "single",
            "double",
            "triple",
            "dotted",
            "dashed",
            "dotDash",
            "dotDotDash",
            "dashSmallGap",
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
            "dashDotStroked",
            "threeDEmboss",
            "threeDEngrave",
            "outset",
            "inset",
        ];
        let all: Vec<_> = styles.iter().map(|s| described(&edge_of(s, false, true, 30.0))).collect();
        for (i, a) in all.iter().enumerate() {
            assert!(!a.is_empty(), "{} draws nothing", styles[i]);
            for (j, b) in all.iter().enumerate().skip(i + 1) {
                assert_ne!(a, b, "{} and {} draw alike", styles[i], styles[j]);
            }
        }
        assert_ne!(described(&edge_of("outset", false, true, 30.0)), described(&edge_of("outset", true, true, 30.0)));
        assert_eq!(described(&edge_of("outset", false, true, 30.0)), described(&edge_of("inset", true, true, 30.0)));
    }

    #[test]
    fn wave_and_hatch_borders_are_stroked_paths_in_both_directions() {
        for style in ["wave", "doubleWave", "dashDotStroked"] {
            for horizontal in [true, false] {
                let out = edge_of(style, false, horizontal, 40.0);
                let [Item::Shape(s)] = out.as_slice() else { panic!("{style} is one stroked path") };
                assert!(s.line.width > 0.0 && s.fill.is_none());
                let [x0, y0, x1, y1] = s.rect;
                let (along, across) = if horizontal { (x1 - x0, y1 - y0) } else { (y1 - y0, x1 - x0) };
                assert!((along - 40.0).abs() < 1e-9, "{style} spans its edge");
                assert!(across > 0.5 && across <= 1.0 + 1e-9, "{style} is a band of {across} inside the 1 pt reserved");
            }
        }
        for (style, band) in [("wave", WAVE_HEIGHT + WAVE_STROKE), ("doubleWave", 2.0 * WAVE_HEIGHT + WAVE_STROKE), ("dashDotStroked", HATCH_BAND)] {
            let b = Border { style: style.into(), width: 1.0, space: 0.0, shadow: false, color: Some(0) };
            let mut out = Vec::new();
            edge(&mut out, [0.0, 0.0, 40.0, 8.0], true, false, &b);
            let [Item::Shape(s)] = out.as_slice() else { panic!() };
            assert!((s.rect[3] - s.rect[1] - band).abs() < 1e-9, "{style} keeps Word's band where there is room, centred");
            assert!((s.rect[1] - (8.0 - band) / 2.0).abs() < 1e-9);
        }
        let wave = edge_of("wave", false, true, 40.0);
        let Item::Shape(s) = &wave[0] else { panic!() };
        let ys: std::collections::BTreeSet<i64> =
            s.geom.paths[0].segs.iter().filter_map(|g| if let Seg::Line(p) = g { Some((p[1] * 100.0).round() as i64) } else { None }).collect();
        assert!((2..=3).contains(&ys.len()), "a wave zigzags between a crest and a trough (and ends part way)");
    }

    #[test]
    fn a_shadow_is_black_below_and_right_of_far_edges_only() {
        let b = Border { style: "single".into(), width: 1.5, space: 0.0, shadow: true, color: Some(0x336699) };
        let mut out = Vec::new();
        shadow(&mut out, [0.0, 10.0, 50.0, 11.5], true, true, &b, true, true);
        shadow(&mut out, [48.5, 0.0, 50.0, 11.5], false, true, &b, true, false);
        shadow(&mut out, [0.0, 0.0, 50.0, 1.5], true, false, &b, true, true);
        shadow(&mut out, [0.0, 10.0, 50.0, 11.5], true, true, &b, false, false);
        assert_eq!(out.len(), 3, "near edges cast none");
        assert_eq!(
            described(&out),
            vec![(vec![150, 1150, 5150, 1300], 0), (vec![5000, 150, 5150, 1150], 0), (vec![0, 1150, 5000, 1300], 0)],
            "a bottom edge alone is shadowed over its own span"
        );
        let plain = Border { shadow: false, ..b };
        let mut none = Vec::new();
        shadow(&mut none, [0.0, 10.0, 50.0, 11.5], true, true, &plain, true, true);
        assert!(none.is_empty());
    }

    #[test]
    fn hostile_lengths_are_bounded() {
        for style in ["wave", "doubleWave", "dashDotStroked"] {
            let out = edge_of(style, true, true, 1e9);
            let Item::Shape(s) = &out[0] else { panic!() };
            assert!(s.geom.paths[0].segs.len() <= 2 * 2 * MAX_SEGMENTS as usize + 8, "{style}");
        }
        let b = Border { style: "dotted".into(), width: 0.25, space: 0.0, shadow: false, color: None };
        let mut out = Vec::new();
        edge(&mut out, [0.0, 0.0, 1e9, 0.25], true, false, &b);
        assert!(out.len() <= MAX_SEGMENTS as usize + 1);
        assert!(edge_of("wave", false, true, 0.0).is_empty() && edge_of("dashDotStroked", false, true, f64::NAN).len() <= 1);
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
