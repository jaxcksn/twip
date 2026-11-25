//! Section and table values as OOXML literals

use super::model::{PageBorderDisplay, VerticalAlignment};

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

/// A cell's `w:vAlign`; an unknown value is `both`, which aligns as top but isn't top.
pub fn cell_v_align(s: &str) -> VerticalAlignment {
    match s {
        "top" => VerticalAlignment::Top,
        "center" => VerticalAlignment::Center,
        "bottom" => VerticalAlignment::Bottom,
        _ => VerticalAlignment::Both,
    }
}

/// A `w:pgBorders/@w:display` value.
pub fn page_border_display_literal(d: PageBorderDisplay) -> &'static str {
    match d {
        PageBorderDisplay::AllPages => "allPages",
        PageBorderDisplay::FirstPage => "firstPage",
        PageBorderDisplay::NotFirstPage => "notFirstPage",
    }
}

/// A `w:pgBorders/@w:display`; an unknown value is every page.
pub fn page_border_display(s: &str) -> PageBorderDisplay {
    match s {
        "firstPage" => PageBorderDisplay::FirstPage,
        "notFirstPage" => PageBorderDisplay::NotFirstPage,
        _ => PageBorderDisplay::AllPages,
    }
}
