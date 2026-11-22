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

#[test]
fn a_complex_script_run_takes_its_complex_script_size_bold_and_font() {
    let x = |rpr: &str| {
        let body = format!(r#"<w:p><w:r><w:rPr>{rpr}</w:rPr><w:t>mm</w:t></w:r><w:r><w:t>X</w:t></w:r></w:p>"#);
        chars("", ARIAL_10, &body, 0).map(|c| at(&c, 'X').0)
    };
    let (Some(plain), Some(big)) = (x(r#"<w:rFonts w:cs="Times New Roman"/><w:sz w:val="20"/>"#), x(r#"<w:sz w:val="40"/>"#)) else { return };
    // Marked complex script: szCs, bCs and rFonts/@cs for every character; b and sz play no part.
    let cs = x(r#"<w:rFonts w:cs="Times New Roman"/><w:b/><w:sz w:val="40"/><w:szCs w:val="20"/><w:cs/>"#).expect("lays out");
    let tnr = x(r#"<w:rFonts w:ascii="Times New Roman" w:hAnsi="Times New Roman"/><w:sz w:val="20"/>"#).expect("lays out");
    assert!((cs - tnr).abs() < 1e-6 && (plain - tnr).abs() > 0.1, "{cs} {tnr} {plain}");
    // Without w:cs (or with w:val="0") sz and b decide.
    let off = x(r#"<w:sz w:val="40"/><w:szCs w:val="20"/><w:cs w:val="0"/>"#).expect("lays out");
    assert!((off - big).abs() < 1e-6, "{off} {big}");
}

#[test]
fn a_text_borders_side_is_its_styles_lines_and_gaps() {
    let side = |style: &str, sz: f64, space: f64, shadow: bool| {
        super::super::text::border_side(&crate::docx::props::Border { style: style.into(), width: sz / 8.0, space, shadow, color: None })
    };
    let near = |a: (f64, f64), b: (f64, f64)| (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9;
    // Laid out in twips, drawn in 600-dpi pixels: triple 12 is five 30 tw lines, 12 px each.
    assert!(near(side("triple", 12.0, 0.0, false), (7.5, 7.2)), "{:?}", side("triple", 12.0, 0.0, false));
    assert!(near(side("wave", 12.0, 0.0, false), (3.75, 3.72)) && near(side("wave", 4.0, 0.0, false), (3.0, 3.0)));
    // A shadow doubles the lines and gaps, not the space: 60 + 2 × (90 + 2 × 15) tw.
    assert!(near(side("thickThinSmallGap", 36.0, 3.0, true), (15.0, 14.76)), "{:?}", side("thickThinSmallGap", 36.0, 3.0, true));
}

#[test]
fn each_lines_part_of_a_broken_text_border_has_both_sides() {
    let bdr = r#"<w:bdr w:val="single" w:sz="8" w:space="2" w:color="000000"/>"#;
    let body = format!(r#"<w:p><w:r><w:rPr>{bdr}</w:rPr><w:t>{}</w:t></w:r></w:p>"#, "lorem ipsum ".repeat(30).trim_end());
    let Some(c) = chars("", ARIAL_10, &body, 0) else { return };
    let mut rows: Vec<f64> = c.iter().map(|g| g.2).collect();
    rows.sort_by(f64::total_cmp);
    rows.dedup_by(|a, b| (*a - *b).abs() < 0.01);
    let first_x = |y: f64| c.iter().filter(|g| (g.2 - y).abs() < 0.01 && !g.0.is_whitespace()).map(|g| g.1).fold(f64::INFINITY, f64::min);
    // 40 + 20 tw laid out, 16 + 8 px drawn: every line starts after a left side.
    assert!(rows.len() > 2 && (first_x(rows[0]) - 74.88).abs() < 1e-6 && (first_x(rows[1]) - 74.88).abs() < 1e-6, "{rows:?} {}", first_x(rows[1]));
}

#[test]
fn compat_14_fits_a_kerned_run_with_the_kern_tables_pairs() {
    if !super::installed(&["Segoe UI"]) {
        return;
    }
    let Some(face) = super::fonts().face("Segoe UI", None, false, false) else { return };
    let text = "wry try dry ferry wry try dry ferry";
    let glyphs = face.shape(text, true, 0, 0);
    let scale = face.scale(12.0);
    let gpos = glyphs.iter().map(|g| g.advance).sum::<f64>() * scale;
    let next = glyphs.iter().skip(1).map(Some).chain([None]);
    let table = glyphs.iter().zip(next).map(|(g, n)| face.advance(g.id) + n.map_or(0.0, |n| face.kern_pair(g.id, n.id))).sum::<f64>() * scale;
    // Segoe UI's kern table kerns "ry" less than its GPOS: the text is wider for compat ≤ 14's fitting.
    assert!(table - gpos > 2.0, "{gpos} {table}");
    let avail = ((gpos + table) * 10.0).round() as i64;
    let body = format!(r#"<w:p><w:pPr><w:ind w:right="{}"/></w:pPr><w:r><w:t xml:space="preserve">{text} zz</w:t></w:r></w:p>"#, 9360 - avail);
    let rpr = r#"<w:rFonts w:ascii="Segoe UI" w:hAnsi="Segoe UI"/><w:sz w:val="24"/><w:kern w:val="2"/>"#;
    let last_word_on_line_1 = |mode: u32| {
        let settings = format!(
            r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="{mode}"/></w:compat>"#
        );
        let c = chars(&settings, rpr, &body, 0).expect("lays out");
        let z = c.iter().position(|g| g.0 == 'z').expect("drawn");
        (c[z - 2].2 - c[0].2).abs() < 1e-6
    };
    assert!(last_word_on_line_1(15) && !last_word_on_line_1(12));
}

#[test]
fn compat_14_no_leading_leaves_the_external_leading_out_of_a_superscripts_limit() {
    if super::fonts().face("Arial", None, false, false).is_none_or(|f| f.family != "Arial") {
        return;
    }
    let body = r#"<w:p><w:r><w:t>x</w:t></w:r><w:r><w:rPr><w:vertAlign w:val="superscript"/></w:rPr><w:t>1</w:t></w:r></w:p>"#;
    let rpr = r#"<w:rFonts w:ascii="Arial" w:hAnsi="Arial"/><w:sz w:val="22"/>"#;
    let raise = |compat: &str, mode: u32| {
        let settings = format!(
            r#"<w:compat>{compat}<w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="{mode}"/></w:compat>"#
        );
        let c = chars(&settings, rpr, body, 0).expect("lays out");
        at(&c, 'x').1 - at(&c, '1').1
    };
    // Arial 11: ascent and leading 21 half-points, without the leading 20: the limit 7 raises 3.5pt, not 4.
    let nl = raise("<w:noLeading/>", 12);
    assert!((nl - 3.5).abs() < 1e-6 && (raise("", 12) - 4.0).abs() < 1e-6, "{nl}");
    // Compat 15 ignores noLeading.
    assert!((raise("<w:noLeading/>", 15) - 4.0).abs() < 1e-6);
}
