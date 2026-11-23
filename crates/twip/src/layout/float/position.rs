//! Float positioning

use crate::docx::Compat;
use crate::docx::model::Float;

use super::Frames;

fn tw(v: f64) -> i64 {
    (v * 20.0).round() as i64
}

/// `a * b / c`, rounded half away from zero; `b` is a wp14 percentage from the file, so the product is wide and the
/// result saturates.
fn mul_div(a: i64, b: i64, c: i64) -> i64 {
    let (q, c) = (i128::from(a) * i128::from(b), i128::from(c));
    let r = (q.abs() + c.abs() / 2) / c.abs();
    let r = if (q >= 0) == (c > 0) { r } else { -r };
    r.clamp(i64::MIN.into(), i64::MAX.into()) as i64
}

fn share(base: f64, tenths: i64) -> f64 {
    mul_div(tw(base), tenths, 1000) as f64 / 20.0
}

// Relative size
pub fn sized(f: &Float, fr: &Frames) -> Option<Float> {
    if fr.cell || f.relative.size == [None, None] {
        return None;
    }
    let [ml, mr, mt, mb] = fr.margin;
    let [el, et, er, eb] = f.effect;
    let fit = |base: f64, p: i64, lead: f64, trail: f64| (mul_div(tw(base), p, 1000) - tw(lead) - tw(trail)).max(20) as f64 / 20.0;
    let picture = f.text_box.is_none() && f.image.is_some();
    let mut out = f.clone();
    if let Some((from, p)) = &f.relative.size[0]
        && !f.auto_w
    {
        let base = match (from.as_str(), fr.odd) {
            ("page", _) => fr.page_w,
            ("leftMargin", _) | ("outsideMargin", true) | ("insideMargin", false) => ml,
            ("rightMargin", _) | ("insideMargin", true) | ("outsideMargin", false) => mr,
            _ => fr.page_w - ml - mr,
        };
        out.w = fit(base, *p, el, er);
    }
    if let Some((from, p)) = &f.relative.size[1]
        && !f.text_box.as_ref().is_some_and(|t| t.auto_fit)
    {
        let base = match (from.as_str(), fr.odd) {
            ("page", _) => fr.page_h,
            ("topMargin", _) | ("insideMargin" | "outsideMargin", true) => mt,
            ("bottomMargin", _) | ("insideMargin" | "outsideMargin", false) => mb,
            _ => fr.page_h - mt - mb,
        };
        out.h = fit(base, *p, et, eb);
    }
    // Each axis is sized on its own; a picture is drawn scaled evenly into the box.
    if picture && f.w > 0.0 && f.h > 0.0 {
        let s = (out.w / f.w).min(out.h / f.h);
        out.relative.drawn = Some((s * f.w, s * f.h));
    }
    Some(out)
}

fn snap(fr: &Frames, offset: f64) -> f64 {
    if fr.compat == Compat::Legacy { (offset * 20.0).round() / 20.0 } else { offset }
}

// Inside and outside
fn h_ref<'a>(f: &'a Float, fr: &Frames) -> &'a str {
    match f.h_relative.as_str() {
        r @ ("insideMargin" | "outsideMargin") => {
            let inside_left = if fr.compat == Compat::Generic { fr.odd } else { !fr.odd };
            if (r == "insideMargin") == inside_left { "leftMargin" } else { "rightMargin" }
        }
        r => r,
    }
}

fn v_ref<'a>(f: &'a Float, fr: &Frames) -> &'a str {
    match (f.v_relative.as_str(), fr.odd) {
        ("insideMargin", true) => "topMargin",
        ("insideMargin", false) => "bottomMargin",
        ("outsideMargin", odd) => {
            if odd == (fr.compat == Compat::Generic) {
                "bottomMargin"
            } else {
                "topMargin"
            }
        }
        (r, _) => r,
    }
}

fn by_parity<'a>(align: Option<&'a str>, fr: &Frames, (min, max): (&'a str, &'a str)) -> Option<&'a str> {
    match align {
        Some("inside") => Some(if fr.odd { min } else { max }),
        Some("outside") => Some(if fr.odd { max } else { min }),
        a => a,
    }
}

/// The axes on which the box with its effect extents is placed: those with an alignment, a relative size or a
/// percentage position. Compat ≤ 14: both, when either is.
fn grown(f: &Float, fr: &Frames) -> [bool; 2] {
    let own = |k: usize| [&f.h_align, &f.v_align][k].is_some() || f.relative.size[k].is_some() || f.relative.pos[k].is_some();
    match [own(0), own(1)] {
        [a, b] if fr.compat == Compat::Legacy => [a || b; 2],
        axes => axes,
    }
}

/// The left edge of horizontal reference `href`'s box and its width.
fn h_box(href: &str, fr: &Frames) -> (f64, f64) {
    match href {
        "page" => (0.0, fr.page_w),
        "margin" => (fr.margin[0], fr.page_w - fr.margin[0] - fr.margin[1]),
        "leftMargin" => (0.0, fr.margin[0]),
        "rightMargin" => (fr.page_w - fr.margin[1], fr.margin[1]),
        _ => fr.column,
    }
}

/// The top of vertical reference `vref`'s box and its height (none for the paragraph and the line).
fn v_box(vref: &str, fr: &Frames, para_top: f64, line_top: f64) -> (f64, f64) {
    match vref {
        "page" => (0.0, fr.page_h),
        "margin" => (fr.margin[2], fr.page_h - fr.margin[2] - fr.margin[3]),
        "topMargin" => (0.0, fr.margin[2]),
        "bottomMargin" => (fr.page_h - fr.margin[3], fr.margin[3]),
        "line" => (line_top, 0.0),
        _ => (para_top, 0.0),
    }
}

/// The horizontal offset that puts `f`, placed by its offset, at `x`.
pub fn h_offset_at(f: &Float, fr: &Frames, x: f64) -> f64 {
    let l = if f.poly.is_none() && grown(f, fr)[0] { f.effect[0] } else { 0.0 };
    x - h_box(h_ref(f, fr), fr).0 - l
}

/// The vertical offset that puts `f`, placed by its offset, at `y`.
pub fn v_offset_at(f: &Float, fr: &Frames, y: f64, para_top: f64, line_top: f64) -> f64 {
    let t = if f.poly.is_none() && grown(f, fr)[1] { f.effect[1] } else { 0.0 };
    y - v_box(v_ref(f, fr), fr, para_top, line_top).0 - t
}

pub fn h_pos(f: &Float, fr: &Frames) -> f64 {
    let href = h_ref(f, fr);
    let (x0, w) = h_box(href, fr);
    if let Some(p) = f.relative.pos[0] {
        let base = if matches!(href, "page" | "margin" | "leftMargin" | "rightMargin") { w } else { fr.page_w - fr.margin[0] - fr.margin[1] };
        return x0 + if f.poly.is_none() { f.effect[0] } else { 0.0 } + share(base, p);
    }
    if fr.compat == Compat::Legacy && f.h_relative == "page" {
        match f.h_align.as_deref() {
            Some("left") if fr.margin[0] >= f.w => return fr.margin[0] - f.w,
            Some("right") if fr.margin[1] >= f.w => return fr.page_w - fr.margin[1],
            _ => {}
        }
    }
    let [l, _, r, _] = if f.poly.is_none() { f.effect } else { [0.0; 4] };
    x0 + snap(
        fr,
        match by_parity(f.h_align.as_deref(), fr, ("left", "right")) {
            Some("center") => (w - f.w) / 2.0 + (l - r) / 2.0,
            Some("right") => w - f.w - r,
            Some(_) => l,
            None => f.h_offset + if grown(f, fr)[0] { l } else { 0.0 },
        },
    )
}

pub fn v_pos(f: &Float, h: f64, fr: &Frames, para_top: f64, line_top: f64) -> f64 {
    let vref = v_ref(f, fr);
    // In a cell an alignment is ignored: the float goes to its reference's top, the cell's content top for every
    // reference but the line and the paragraph.
    if fr.cell && f.v_align.is_some() {
        return match vref {
            "line" => line_top,
            "paragraph" => para_top,
            _ => fr.margin[2],
        };
    }
    let (y0, ht) = v_box(vref, fr, para_top, line_top);
    if let Some(p) = f.relative.pos[1].filter(|_| !fr.cell) {
        let base = if matches!(vref, "page" | "margin" | "topMargin" | "bottomMargin") { ht } else { fr.page_h - fr.margin[2] - fr.margin[3] };
        return y0 + if f.poly.is_none() { f.effect[1] } else { 0.0 } + share(base, p);
    }
    match vref {
        "page" | "margin" | "topMargin" | "bottomMargin" | "line" => {}
        _ if f.v_align.is_some() => return para_top,
        _ => return para_top + f.v_offset + if grown(f, fr)[1] && f.poly.is_none() { f.effect[1] } else { 0.0 },
    }
    let [_, t, _, b] = if f.poly.is_none() { f.effect } else { [0.0; 4] };
    y0 + snap(
        fr,
        match by_parity(f.v_align.as_deref(), fr, ("top", "bottom")) {
            Some("center") => (ht - h) / 2.0 + (t - b) / 2.0,
            Some("bottom") => ht - h - b,
            Some(_) => t,
            None => f.v_offset + if grown(f, fr)[1] { t } else { 0.0 },
        },
    )
}

/// Compat ≤ 14 keeps a wrapping float placed from the column or the paragraph on the page: its box with its effect
/// extents moves left off the right edge and up off the bottom, then right of the left edge and below the top.
/// In a cell, both modes, a wrapping float stays inside the cell's box horizontally, its left edge first.
pub fn page_clamp(f: &Float, (x, y): (f64, f64), h: f64, fr: &Frames) -> (f64, f64) {
    if fr.cell && f.wrap != "none" {
        let [l, _, r, _] = f.effect;
        return ((x - (x + f.w + r - fr.page_w).max(0.0)).max(l), y);
    }
    if fr.compat != Compat::Legacy || fr.cell || f.wrap == "none" || !(f.h_relative == "column" || f.v_relative == "paragraph") {
        return (x, y);
    }
    let [l, t, r, b] = f.effect;
    let x = x - (x + f.w + r - fr.page_w).max(0.0);
    let y = y - (y + h + b - fr.page_h).max(0.0);
    (x.max(l), y.max(t))
}
