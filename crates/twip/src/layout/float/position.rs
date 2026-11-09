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
        let base = match from.as_str() {
            "page" => fr.page_w,
            "leftMargin" | "outsideMargin" => ml,
            "rightMargin" | "insideMargin" => mr,
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
        let base = match from.as_str() {
            "page" => fr.page_h,
            "topMargin" | "insideMargin" | "outsideMargin" => mt,
            "bottomMargin" => mb,
            _ => fr.page_h - mt - mb,
        };
        out.h = fit(base, *p, et, eb);
    }
    Some(out)
}

pub fn h_pos(f: &Float, fr: &Frames) -> f64 {
    if let Some(p) = f.relative.pos[0] {
        let body = fr.page_w - fr.margin[0] - fr.margin[1];
        let (s, base) = match f.h_relative.as_str() {
            "page" => (0.0, fr.page_w),
            "margin" => (fr.margin[0], body),
            "leftMargin" => (0.0, fr.margin[0]),
            "rightMargin" => (fr.page_w - fr.margin[1], fr.margin[1]),
            _ => (fr.column.0, body),
        };
        return s + if f.poly.is_none() { f.effect[0] } else { 0.0 } + share(base, p);
    }
    let (x0, w) = match f.h_relative.as_str() {
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
    match f.h_align.as_deref() {
        Some("center") => x0 + (w - f.w) / 2.0 + (l - r) / 2.0,
        Some("right" | "outside") => x0 + w - f.w - r,
        Some(_) => x0 + l,
        None => x0 + f.h_offset,
    }
}

pub fn v_pos(f: &Float, h: f64, fr: &Frames, para_top: f64, line_top: f64) -> f64 {
    if let Some(p) = f.relative.pos[1].filter(|_| !fr.cell) {
        let body = fr.page_h - fr.margin[2] - fr.margin[3];
        let (s, base) = match f.v_relative.as_str() {
            "page" => (0.0, fr.page_h),
            "margin" => (fr.margin[2], body),
            "topMargin" => (0.0, fr.margin[2]),
            "bottomMargin" => (fr.page_h - fr.margin[3], fr.margin[3]),
            "line" => (line_top, body),
            _ => (para_top, body),
        };
        return s + if f.poly.is_none() { f.effect[1] } else { 0.0 } + share(base, p);
    }
    let (y0, ht) = match f.v_relative.as_str() {
        "page" => (0.0, fr.page_h),
        "margin" => (fr.margin[2], fr.page_h - fr.margin[2] - fr.margin[3]),
        "topMargin" => (0.0, fr.margin[2]),
        "bottomMargin" => (fr.page_h - fr.margin[3], fr.margin[3]),
        "line" => (line_top, 0.0),
        _ => return para_top + if f.v_align.is_some() { 0.0 } else { f.v_offset },
    };
    match f.v_align.as_deref() {
        Some("center") => y0 + (ht - h) / 2.0,
        Some("bottom" | "outside") => y0 + ht - h,
        Some(_) => y0,
        None => y0 + f.v_offset,
    }
}
