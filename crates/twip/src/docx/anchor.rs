//! Float anchoring and wrapping: the DOCX literals of the model's values

use super::model::{VerticalAlign, WrapText};

/// `wrapText`, or a VML wrap's `side`; anything else wraps both sides.
pub fn wrap_text(s: &str) -> WrapText {
    match s {
        "left" => WrapText::Left,
        "right" => WrapText::Right,
        "largest" => WrapText::Largest,
        _ => WrapText::BothSides,
    }
}

/// The `wrapText` literal of `w`.
pub fn wrap_text_name(w: WrapText) -> &'static str {
    match w {
        WrapText::BothSides => "bothSides",
        WrapText::Left => "left",
        WrapText::Right => "right",
        WrapText::Largest => "largest",
    }
}

/// `wp:align` in `wp:positionV`, a frame's `w:yAlign` or VML `mso-position-vertical`; anything else aligns at the top.
pub fn v_align(s: &str) -> VerticalAlign {
    match s {
        "center" => VerticalAlign::Center,
        "bottom" => VerticalAlign::Bottom,
        "inside" => VerticalAlign::Inside,
        "outside" => VerticalAlign::Outside,
        _ => VerticalAlign::Top,
    }
}

/// The `wp:align` literal of `a`.
pub fn v_align_name(a: VerticalAlign) -> &'static str {
    match a {
        VerticalAlign::Top => "top",
        VerticalAlign::Center => "center",
        VerticalAlign::Bottom => "bottom",
        VerticalAlign::Inside => "inside",
        VerticalAlign::Outside => "outside",
    }
}
