//! Float anchoring and wrapping: the DOCX literals of the model's values

use super::model::{FloatWrap, HorizontalRelation, VerticalAlign, WrapText};

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

/// A VML wrap's `type`; anything else doesn't wrap.
pub fn wrap(s: &str) -> FloatWrap {
    match s {
        "square" => FloatWrap::Square,
        "tight" => FloatWrap::Tight,
        "through" => FloatWrap::Through,
        "topAndBottom" => FloatWrap::TopAndBottom,
        _ => FloatWrap::None,
    }
}

/// The VML wrap `type` of `w`.
pub fn wrap_name(w: FloatWrap) -> &'static str {
    match w {
        FloatWrap::Square => "square",
        FloatWrap::Tight => "tight",
        FloatWrap::Through => "through",
        FloatWrap::TopAndBottom => "topAndBottom",
        FloatWrap::None => "none",
    }
}

/// `relativeFrom` in `wp:positionH`.
pub fn h_relative(s: &str) -> HorizontalRelation {
    match s {
        "margin" => HorizontalRelation::Margin,
        "page" => HorizontalRelation::Page,
        "column" => HorizontalRelation::Column,
        "character" => HorizontalRelation::Character,
        "leftMargin" => HorizontalRelation::LeftMargin,
        "rightMargin" => HorizontalRelation::RightMargin,
        "insideMargin" => HorizontalRelation::InsideMargin,
        "outsideMargin" => HorizontalRelation::OutsideMargin,
        _ => HorizontalRelation::Unrecognised,
    }
}

/// The `relativeFrom` literal of `r`; an unrecognised one is written as the column it is measured across.
pub fn h_relative_name(r: HorizontalRelation) -> &'static str {
    match r {
        HorizontalRelation::Margin => "margin",
        HorizontalRelation::Page => "page",
        HorizontalRelation::Column | HorizontalRelation::Unrecognised => "column",
        HorizontalRelation::Character => "character",
        HorizontalRelation::LeftMargin => "leftMargin",
        HorizontalRelation::RightMargin => "rightMargin",
        HorizontalRelation::InsideMargin => "insideMargin",
        HorizontalRelation::OutsideMargin => "outsideMargin",
    }
}
