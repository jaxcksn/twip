//! Float positions against the page, margins, column, paragraph and line (FLOAT-6).

use crate::docx::Compat;
use crate::docx::model::Float;

use super::Frames;

/// Horizontal position (FLOAT-6): `relativeFrom` column / margin / page, `align` left/center/right or an offset.
pub fn h_pos(f: &Float, fr: &Frames) -> f64 {
    let (x0, w) = match f.h_relative.as_str() {
        "page" => (0.0, fr.page_w),
        "margin" => (fr.margin[0], fr.page_w - fr.margin[0] - fr.margin[1]),
        "leftMargin" => (0.0, fr.margin[0]),
        "rightMargin" => (fr.page_w - fr.margin[1], fr.margin[1]),
        // shortcut: character / insideMargin / outsideMargin are not measured; treated as the column (FLOAT-6).
        _ => fr.column,
    };
    // Compat ≤ 14: left or right of the page puts the float in the margin area when it fits there.
    if fr.compat == Compat::Legacy && f.h_relative == "page" {
        match f.h_align.as_deref() {
            Some("left") if fr.margin[0] >= f.w => return fr.margin[0] - f.w,
            Some("right") if fr.margin[1] >= f.w => return fr.page_w - fr.margin[1],
            _ => {}
        }
    }
    // FLOAT-6: an alignment aligns the box grown by its effect extents (a wrap polygon's float: the bare box); an
    // offset places the bare box.
    let [l, _, r, _] = if f.poly.is_none() { f.effect } else { [0.0; 4] };
    match f.h_align.as_deref() {
        Some("center") => x0 + (w - f.w) / 2.0 + (l - r) / 2.0,
        Some("right" | "outside") => x0 + w - f.w - r,
        Some(_) => x0 + l,
        None => x0 + f.h_offset,
    }
}

/// Vertical position (FLOAT-6). `para_top` = the anchor paragraph's first line box top, `line_top` = the anchor line's.
pub fn v_pos(f: &Float, h: f64, fr: &Frames, para_top: f64, line_top: f64) -> f64 {
    let (y0, ht) = match f.v_relative.as_str() {
        "page" => (0.0, fr.page_h),
        "margin" => (fr.margin[2], fr.page_h - fr.margin[2] - fr.margin[3]),
        "topMargin" => (0.0, fr.margin[2]),
        "bottomMargin" => (fr.page_h - fr.margin[3], fr.margin[3]),
        "line" => (line_top, 0.0),
        // An alignment against the paragraph puts the float at its top.
        _ => return para_top + if f.v_align.is_some() { 0.0 } else { f.v_offset },
    };
    match f.v_align.as_deref() {
        Some("center") => y0 + (ht - h) / 2.0,
        Some("bottom" | "outside") => y0 + ht - h,
        Some(_) => y0,
        None => y0 + f.v_offset,
    }
}
