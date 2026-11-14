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
    // The device metrics are Arial's own: a metric-compatible stand-in has other ones.
    if super::fonts().face("Arial", None, false, false).is_none_or(|f| f.family != "Arial") {
        return;
    }
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
        let p = |t: &str, em: &str| {
            format!(r#"<w:p><w:pPr><w:spacing w:line="{line}" w:lineRule="auto"/></w:pPr><w:r><w:rPr>{em}</w:rPr><w:t>{t}</w:t></w:r></w:p>"#)
        };
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

#[test]
fn compat_15_a_paragraph_of_4000_characters_fits_its_region_as_compat_12() {
    let settings =
        r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat>"#;
    let rpr = r#"<w:rFonts w:ascii="Arial" w:hAnsi="Arial"/><w:sz w:val="22"/>"#;
    let line = |len: usize| {
        // `G2 ` + `stuv` fitted to 2700 tw + `|z ` and filler, `len` characters with the mark, on a 3372 tw line.
        let filler = "lorem ".repeat(700)[..len - 1 - 3 - 4 - 3].to_string();
        let body = format!(
            r#"<w:p><w:r><w:t>filler</w:t></w:r></w:p><w:p><w:pPr><w:ind w:right="5988"/></w:pPr><w:r><w:t xml:space="preserve">G2 </w:t></w:r><w:r><w:rPr><w:fitText w:val="2700" w:id="701"/></w:rPr><w:t>stuv</w:t></w:r><w:r><w:t xml:space="preserve">|z {filler}</w:t></w:r></w:p>"#
        );
        let c = chars(settings, rpr, &body, 0)?;
        Some(at(&c, 's').1 - at(&c, 'G').1)
    };
    let Some(short) = line(3999) else { return };
    assert!(short > 1.0, "3999: the region breaks with its first adjustment, on line 2");
    assert!(line(4000).expect("lays out").abs() < 1e-6, "4000: compat 12's width fits line 1");
}

#[test]
fn a_text_borders_sides_widen_and_heighten_its_box() {
    let bdr = r#"<w:bdr w:val="single" w:sz="4" w:space="0" w:color="000000"/>"#;
    let body = format!(
        r#"<w:p><w:r><w:rPr>{bdr}</w:rPr><w:t>b</w:t></w:r><w:r><w:rPr>{bdr}</w:rPr><w:t>c</w:t></w:r><w:r><w:t>d</w:t></w:r></w:p><w:p><w:r><w:t>y</w:t></w:r></w:p>"#
    );
    let Some(c) = chars("", ARIAL_10, &body, 0) else { return };
    // Equal borders make one box: 10 tw a side, drawn as 4 pixels (0.48pt); 0.5pt more above and below.
    let (b, cx, d, y) = (at(&c, 'b'), at(&c, 'c'), at(&c, 'd'), at(&c, 'y'));
    let adv = |n: f64| n * 10.0 / 2048.0;
    assert!((b.0 - 72.48).abs() < 1e-6 && (cx.0 - b.0 - adv(1139.0)).abs() < 1e-6, "{b:?} {cx:?}");
    assert!((d.0 - cx.0 - adv(1024.0) - 0.48).abs() < 1e-6, "{cx:?} {d:?}");
    assert!((b.1 - (72.0 + 0.5 + 1921.0 * 10.0 / 2048.0)).abs() < 0.01 && (y.1 - b.1 - (47100.0 / 4096.0 + 0.5)).abs() < 0.01, "{b:?} {y:?}");
}

#[test]
fn a_multiple_spacings_extra_holds_a_smaller_runs_border() {
    let next = |bdr: &str| {
        let p = |r: &str| format!(r#"<w:p><w:pPr><w:spacing w:line="288" w:lineRule="auto"/></w:pPr>{r}</w:p>"#);
        let first = format!(r#"<w:r><w:t xml:space="preserve">x </w:t></w:r><w:r><w:rPr><w:sz w:val="16"/>{bdr}</w:rPr><w:t>k</w:t></w:r>"#);
        let c = chars("", ARIAL_10, &(p(&first) + &p("<w:r><w:t>y</w:t></w:r>")), 0)?;
        Some(at(&c, 'y').1)
    };
    let Some(plain) = next("") else { return };
    // The 8pt run's border reaches below the 10pt text, by less than the 1.2 lines' extra: the pitch stays.
    let boxed = next(r#"<w:bdr w:val="single" w:sz="4" w:space="1" w:color="000000"/>"#).expect("lays out");
    assert!((boxed - plain).abs() < 1e-6, "{plain} {boxed}");
}

#[test]
fn synthesized_bold_glyphs_are_a_font_unit_wider() {
    if !super::fonts().face("Calibri Light", None, true, false).is_some_and(|f| f.sim_bold) {
        return;
    }
    let rpr = r#"<w:rFonts w:ascii="Calibri Light" w:hAnsi="Calibri Light"/><w:sz w:val="32"/>"#;
    let x = |b: &str| {
        let body = format!(r#"<w:p><w:r><w:rPr>{b}</w:rPr><w:t xml:space="preserve">iiii iiii</w:t></w:r><w:r><w:t>X</w:t></w:r></w:p>"#);
        chars("", rpr, &body, 0).map(|c| at(&c, 'X').0)
    };
    let (Some(regular), Some(bold)) = (x(""), x("<w:b/>")) else { return };
    assert!((bold - regular - 9.0 * 16.0 / 2048.0).abs() < 1e-6, "{regular} {bold}");
}
