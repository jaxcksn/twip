//! Font metrics and line heights

use super::breaking::test_doc;

const ARIAL_10: &str = r#"<w:rFonts w:ascii="Arial" w:hAnsi="Arial"/><w:sz w:val="20"/>"#;

/// Each character on page `page` with its x and baseline.
fn chars(settings: &str, rpr: &str, body: &str, page: usize) -> Option<Vec<(char, f64, f64)>> {
    super::page_chars(&test_doc(settings, rpr, body, "")?).into_iter().nth(page)
}

fn at(chars: &[(char, f64, f64)], c: char) -> (f64, f64) {
    chars.iter().find(|g| g.0 == c).map(|g| (g.1, g.2)).expect("drawn")
}

#[test]
fn a_superscript_is_limited_by_the_devices_pixel_metrics() {
    let body = r#"<w:p><w:r><w:t>x</w:t></w:r><w:r><w:rPr><w:vertAlign w:val="superscript"/></w:rPr><w:t>1</w:t></w:r></w:p>"#;
    let Some(c) = chars("", ARIAL_10, body, 0) else { return };
    // Arial 10 at 83 ppem: ascent 74 + leading 3 px is 18 half-points, the 6.5pt size's 49 px 12: raised 6, not 7.
    let raise = at(&c, 'x').1 - at(&c, '1').1;
    assert!((raise - 3.0).abs() < 1e-6, "{raise}");
}
