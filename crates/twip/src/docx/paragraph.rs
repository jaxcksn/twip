//! Paragraph and list label values

pub use crate::document::paragraph::TextAlignment;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_values_read_as_what_layout_gave_them() {
        assert_eq!(text_alignment(""), TextAlignment::Auto);
        assert_eq!(text_alignment("middle"), TextAlignment::Baseline);
    }
}
