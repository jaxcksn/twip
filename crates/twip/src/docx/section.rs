//! Section and table values as OOXML literals

use super::model::VerticalAlignment;

/// A `w:vAlign` value.
pub fn v_align_literal(v: VerticalAlignment) -> &'static str {
    match v {
        VerticalAlignment::Top => "top",
        VerticalAlignment::Center => "center",
        VerticalAlignment::Both => "both",
        VerticalAlignment::Bottom => "bottom",
    }
}

/// A section's `w:vAlign`; an unknown value is top.
pub fn section_v_align(s: &str) -> VerticalAlignment {
    match s {
        "center" => VerticalAlignment::Center,
        "both" => VerticalAlignment::Both,
        "bottom" => VerticalAlignment::Bottom,
        _ => VerticalAlignment::Top,
    }
}
