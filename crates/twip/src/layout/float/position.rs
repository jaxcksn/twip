//! Float positioning

use crate::docx::Compat;
use crate::docx::model::Float;

use super::Frames;

fn tw(v: f64) -> i64 {
    (v * 20.0).round() as i64
}

fn mul_div(a: i64, b: i64, c: i64) -> i64 {
    let q = a * b;
    let r = (q.abs() + c.abs() / 2) / c.abs();
    if (q >= 0) == (c > 0) { r } else { -r }
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
        if picture && f.w > 0.0 {
            out.h = f.h * out.w / f.w;
        }
    }
    if let Some((from, p)) = &f.relative.size[1]
        && !picture
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

pub fn h_pos(f: &Float, fr: &Frames) -> f64 {
    let href = h_ref(f, fr);
    if let Some(p) = f.relative.pos[0] {
        let body = fr.page_w - fr.margin[0] - fr.margin[1];
        let (s, base) = match href {
            "page" => (0.0, fr.page_w),
            "margin" => (fr.margin[0], body),
            "leftMargin" => (0.0, fr.margin[0]),
            "rightMargin" => (fr.page_w - fr.margin[1], fr.margin[1]),
            _ => (fr.column.0, body),
        };
        return s + if f.poly.is_none() { f.effect[0] } else { 0.0 } + share(base, p);
    }
    let (x0, w) = match href {
        "page" => (0.0, fr.page_w),
        "margin" => (fr.margin[0], fr.page_w - fr.margin[0] - fr.margin[1]),
        "leftMargin" => (0.0, fr.margin[0]),
        "rightMargin" => (fr.page_w - fr.margin[1], fr.margin[1]),
        _ => fr.column,
    };
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
            None => f.h_offset,
        },
    )
}

pub fn v_pos(f: &Float, h: f64, fr: &Frames, para_top: f64, line_top: f64) -> f64 {
    let vref = v_ref(f, fr);
    if let Some(p) = f.relative.pos[1].filter(|_| !fr.cell) {
        let body = fr.page_h - fr.margin[2] - fr.margin[3];
        let (s, base) = match vref {
            "page" => (0.0, fr.page_h),
            "margin" => (fr.margin[2], body),
            "topMargin" => (0.0, fr.margin[2]),
            "bottomMargin" => (fr.page_h - fr.margin[3], fr.margin[3]),
            "line" => (line_top, body),
            _ => (para_top, body),
        };
        return s + if f.poly.is_none() { f.effect[1] } else { 0.0 } + share(base, p);
    }
    let (y0, ht) = match vref {
        "page" => (0.0, fr.page_h),
        "margin" => (fr.margin[2], fr.page_h - fr.margin[2] - fr.margin[3]),
        "topMargin" => (0.0, fr.margin[2]),
        "bottomMargin" => (fr.page_h - fr.margin[3], fr.margin[3]),
        "line" => (line_top, 0.0),
        _ => return para_top + if f.v_align.is_some() { 0.0 } else { f.v_offset },
    };
    y0 + snap(
        fr,
        match by_parity(f.v_align.as_deref(), fr, ("top", "bottom")) {
            Some("center") => (ht - h) / 2.0,
            Some("bottom") => ht - h,
            Some(_) => 0.0,
            None => f.v_offset,
        },
    )
}
