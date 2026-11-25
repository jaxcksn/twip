//! Section and table values as OOXML literals

use super::model::{HeightRule, LineNumberingRestart, PageBorderDisplay, VerticalAlignment};

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

/// A `w:trHeight/@w:hRule` value.
pub fn height_rule_literal(r: HeightRule) -> &'static str {
    match r {
        HeightRule::Auto => "auto",
        HeightRule::Exact => "exact",
        HeightRule::AtLeast => "atLeast",
    }
}

/// A `w:trHeight/@w:hRule`; an unknown value is at least.
pub fn height_rule(s: &str) -> HeightRule {
    match s {
        "auto" => HeightRule::Auto,
        "exact" => HeightRule::Exact,
        _ => HeightRule::AtLeast,
    }
}

/// A `w:lnNumType/@w:restart` value; an unrecognised one is written as the schema's default, `newPage`.
pub fn line_numbering_restart_literal(r: LineNumberingRestart) -> &'static str {
    match r {
        LineNumberingRestart::NewPage | LineNumberingRestart::Unrecognised => "newPage",
        LineNumberingRestart::NewSection => "newSection",
        LineNumberingRestart::Continuous => "continuous",
    }
}

/// A `w:lnNumType/@w:restart`.
pub fn line_numbering_restart(s: &str) -> LineNumberingRestart {
    match s {
        "newPage" => LineNumberingRestart::NewPage,
        "newSection" => LineNumberingRestart::NewSection,
        "continuous" => LineNumberingRestart::Continuous,
        _ => LineNumberingRestart::Unrecognised,
    }
}
