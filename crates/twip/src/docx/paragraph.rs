//! Paragraph and list label values

pub use crate::document::paragraph::{ListJc, ListSuffix, TextAlignment};

/// A `w:textAlignment` value (an empty one `auto`, any other unknown one `baseline`).
pub fn text_alignment(v: &str) -> TextAlignment {
    match v {
        "auto" | "" => TextAlignment::Auto,
        "top" => TextAlignment::Top,
        "center" => TextAlignment::Center,
        "bottom" => TextAlignment::Bottom,
        _ => TextAlignment::Baseline,
    }
}

/// A `w:lvlJc` value (an unknown one `start`, which layout treats alike).
pub fn list_jc(v: &str) -> ListJc {
    match v {
        "left" => ListJc::Left,
        "center" => ListJc::Center,
        "right" => ListJc::Right,
        "end" => ListJc::End,
        "both" => ListJc::Both,
        "mediumKashida" => ListJc::MediumKashida,
        "distribute" => ListJc::Distribute,
        "numTab" => ListJc::NumTab,
        "highKashida" => ListJc::HighKashida,
        "lowKashida" => ListJc::LowKashida,
        "thaiDistribute" => ListJc::ThaiDistribute,
        _ => ListJc::Start,
    }
}

/// A `w:suff` value (an unknown one `tab`).
pub fn list_suffix(v: &str) -> ListSuffix {
    match v {
        "space" => ListSuffix::Space,
        "nothing" => ListSuffix::Nothing,
        _ => ListSuffix::Tab,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_values_read_as_what_layout_gave_them() {
        assert_eq!(text_alignment(""), TextAlignment::Auto);
        assert_eq!(text_alignment("middle"), TextAlignment::Baseline);
        assert_eq!(list_jc("middle"), ListJc::Start);
        assert_eq!(list_suffix("dot"), ListSuffix::Tab);
    }
}
