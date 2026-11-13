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

#[test]
fn a_check_box_is_its_fonts_line_height_in_whole_device_pixels_wide() {
    let body = r#"<w:p><w:r><w:fldChar w:fldCharType="begin"><w:ffData><w:checkBox><w:size w:val="24"/></w:checkBox></w:ffData></w:fldChar></w:r><w:r><w:instrText xml:space="preserve"> FORMCHECKBOX </w:instrText></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r><w:r><w:t>b</w:t></w:r></w:p>"#;
    let Some(c) = chars("", ARIAL_10, body, 0) else { return };
    // Arial at 12pt: 46104 + 10416 lu = 115 pixels of 0.12pt.
    let w = at(&c, 'b').0 - 72.0;
    assert!((w - 13.8).abs() < 1e-6, "{w}");
}

#[test]
fn a_line_of_only_spaces_takes_their_height_and_one_ending_in_a_break_the_breaks() {
    let first = |end: &str| {
        let spaces = format!(r#"<w:r><w:rPr><w:sz w:val="40"/></w:rPr><w:t xml:space="preserve">{}</w:t></w:r>"#, " ".repeat(10));
        let c = chars("", ARIAL_10, &format!(r#"<w:p>{spaces}<w:r>{end}<w:t>{}</w:t></w:r></w:p>"#, "m".repeat(50)), 0)?;
        Some(at(&c, 'm').1 - 72.0 - 1921.0 * 10.0 / 2048.0)
    };
    // Ten 20pt spaces, then a word that doesn't fit beside them: line 1 is the spaces' (22.998), not the mark's.
    let Some(spaces) = first("") else { return };
    assert!((spaces - 2355.0 * 20.0 / 2048.0).abs() < 0.01, "{spaces}");
    // The same spaces before a 10pt break: the break's line (11.499).
    let brk = first("<w:br/>").expect("lays out");
    assert!((brk - 47100.0 / 4096.0).abs() < 0.01, "{brk}");
}

#[test]
fn an_emphasis_mark_is_a_quarter_of_the_line_and_fits_in_the_spacing_extra() {
    let lines = |line: u32| {
        let p = |t: &str, em: &str| format!(r#"<w:p><w:pPr><w:spacing w:line="{line}" w:lineRule="auto"/></w:pPr><w:r><w:rPr>{em}</w:rPr><w:t>{t}</w:t></w:r></w:p>"#);
        let c = chars("", ARIAL_10, &(p("x", r#"<w:em w:val="dot"/>"#) + &p("y", "")), 0)?;
        Some((at(&c, 'x').1, at(&c, 'y').1))
    };
    let Some((x, y)) = lines(240) else { return };
    // Arial 10: H = 47100 lu, the mark 11775 lu above the text.
    let (h, a, k) = (47100.0 / 4096.0, 1921.0 * 10.0 / 2048.0, 11775.0 / 4096.0);
    assert!((x - (72.0 + a + k)).abs() < 0.01 && (y - (72.0 + h + k + a)).abs() < 0.01, "{x} {y}");
    // 1.5 lines: the extra (H / 2) holds the mark: the pitch stays, the text moves down by it.
    let (x, y) = lines(360).expect("lays out");
    assert!((x - (72.0 + a + k)).abs() < 0.01 && (y - (72.0 + 1.5 * h + a)).abs() < 0.01, "{x} {y}");
}
