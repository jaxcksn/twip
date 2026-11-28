//! Float anchoring and wrapping: the DOCX literals of the model's values

use super::model::{FloatWrap, HorizontalAlign, HorizontalRelation, VerticalAlign, VerticalRelation, WrapText};

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

/// `relativeFrom` in `wp:positionV`.
pub fn v_relative(s: &str) -> VerticalRelation {
    match s {
        "margin" => VerticalRelation::Margin,
        "page" => VerticalRelation::Page,
        "paragraph" => VerticalRelation::Paragraph,
        "line" => VerticalRelation::Line,
        "topMargin" => VerticalRelation::TopMargin,
        "bottomMargin" => VerticalRelation::BottomMargin,
        "insideMargin" => VerticalRelation::InsideMargin,
        "outsideMargin" => VerticalRelation::OutsideMargin,
        _ => VerticalRelation::Unrecognised,
    }
}

/// The `relativeFrom` literal of `r`; an unrecognised one is written as the paragraph it is measured down from.
pub fn v_relative_name(r: VerticalRelation) -> &'static str {
    match r {
        VerticalRelation::Margin => "margin",
        VerticalRelation::Page => "page",
        VerticalRelation::Paragraph | VerticalRelation::Unrecognised => "paragraph",
        VerticalRelation::Line => "line",
        VerticalRelation::TopMargin => "topMargin",
        VerticalRelation::BottomMargin => "bottomMargin",
        VerticalRelation::InsideMargin => "insideMargin",
        VerticalRelation::OutsideMargin => "outsideMargin",
    }
}

/// `wp:align` in `wp:positionH`, a frame's `w:xAlign` or VML `mso-position-horizontal`.
pub fn h_align(s: &str) -> HorizontalAlign {
    match s {
        "left" => HorizontalAlign::Left,
        "center" => HorizontalAlign::Center,
        "right" => HorizontalAlign::Right,
        "inside" => HorizontalAlign::Inside,
        "outside" => HorizontalAlign::Outside,
        _ => HorizontalAlign::Unrecognised,
    }
}

/// The `wp:align` literal of `a`; an unrecognised one is written as the left alignment it is placed by.
pub fn h_align_name(a: HorizontalAlign) -> &'static str {
    match a {
        HorizontalAlign::Left | HorizontalAlign::Unrecognised => "left",
        HorizontalAlign::Center => "center",
        HorizontalAlign::Right => "right",
        HorizontalAlign::Inside => "inside",
        HorizontalAlign::Outside => "outside",
    }
}
