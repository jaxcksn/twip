//! Float anchoring and wrapping: the DOCX literals of the model's values

use super::model::WrapText;

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
