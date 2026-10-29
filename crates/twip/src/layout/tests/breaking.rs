//! Small documents whose line breaks and line positions follow Word's rules: squeezed lines after a tab (BREAK-9),
//! breaks around inline pictures (BREAK-3), tab stops before a hanging indent (TAB-3), lines beside a float
//! (FLOAT-7) or that can't fit beside one (FLOAT-4), and nested tables (TABLE-25, TABLE-35). Skipped without the system's
//! Arial.

use crate::docx::blank::blank_docx;
use crate::docx::write::repackage;
use crate::layout::fragment::Item;
use crate::layout::layout;
use crate::layout::traverse::paint;

use super::fonts;

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

/// The first page's drawn lines as (left x, text, x of the last glyph run), in drawing order: a compat-12 document with `compressPunctuation`,
/// `settings` added to its settings, docDefaults run properties `rpr`, and `body` (paragraphs and tables, then a Letter
/// section with 1in margins).
fn lines(settings: &str, rpr: &str, body: &str) -> Option<Vec<(f64, String, f64)>> {
    Some(baselines(settings, rpr, body)?.into_iter().map(|(x, t, last, _)| (x, t, last)).collect())
}

/// [`lines`] with each line's baseline.
fn baselines(settings: &str, rpr: &str, body: &str) -> Option<Vec<(f64, String, f64, f64)>> {
    let fonts = fonts();
    fonts.face("Arial", false, false)?;
    let compat = r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="12"/></w:compat>"#;
    let parts = [
        (
            "word/document.xml",
            format!(
                r#"<w:document xmlns:w="{W}" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture"><w:body>{body}<w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr></w:body></w:document>"#
            ),
        ),
        ("word/styles.xml", format!(r#"<w:styles xmlns:w="{W}"><w:docDefaults><w:rPrDefault><w:rPr>{rpr}</w:rPr></w:rPrDefault></w:docDefaults></w:styles>"#)),
        ("word/settings.xml", format!(r#"<w:settings xmlns:w="{W}">{settings}<w:characterSpacingControl w:val="compressPunctuation"/>{compat}</w:settings>"#)),
        (
            "word/_rels/document.xml.rels",
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings" Target="settings.xml"/></Relationships>"#.to_string(),
        ),
    ];
    let parts: Vec<(String, Vec<u8>)> = parts.into_iter().map(|(n, x)| (n.to_string(), x.into_bytes())).collect();
    let doc = crate::docx::read(&repackage(&blank_docx(), &parts, zip::CompressionMethod::Deflated).expect("repackages")).expect("reads");
    let snap = layout(&doc, &fonts);
    let mut out: Vec<(u32, f64, String, f64, f64)> = Vec::new();
    paint(&snap.pages()[0], &mut |p| {
        if let Item::Glyphs(r) = p.absolute() {
            let text: String = r.chars.iter().map(|c| c.0).collect();
            match out.iter_mut().find(|l| l.0 == p.line) {
                Some(l) => {
                    l.1 = l.1.min(r.x);
                    l.2.push_str(&text);
                    l.3 = r.x;
                }
                None => out.push((p.line, r.x, text, r.x, r.baseline)),
            }
        }
    });
    Some(out.into_iter().map(|(_, x, t, last, y)| (x, t.trim_end().to_string(), last, y)).collect())
}

const ARIAL_12: &str = r#"<w:rFonts w:ascii="Arial" w:hAnsi="Arial" w:eastAsiaTheme="minorEastAsia"/><w:sz w:val="24"/>"#;

#[test]
fn spaces_after_a_tab_squeeze_a_word_in() {
    // BREAK-9: up to "with" the text after the tab is 3.6pt too wide for the 432pt left; its spaces give way.
    let p =
        r#"<w:p><w:r><w:tab/><w:t>You are ordered to appear at a hearing on Monday, April 4, 2025 at 10:00 am with your papers.</w:t></w:r></w:p>"#;
    let Some(l) = lines("", ARIAL_12, p) else { return };
    assert!(l[0].1.ends_with("am with"), "{l:?}");
}

#[test]
fn a_line_breaks_before_and_after_an_inline_picture() {
    // BREAK-3: a picture and a run of underscores that don't fit one line go on two lines, the run unsplit.
    let pic = r#"<w:r><w:drawing><wp:inline><wp:extent cx="1333500" cy="476250"/><a:graphic><a:graphicData><pic:pic><pic:blipFill/></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>"#;
    let under = "_".repeat(60);
    let after = format!(r#"<w:p>{pic}<w:r><w:t>{under}</w:t></w:r></w:p>"#);
    let Some(l) = lines("", ARIAL_12, &after) else { return };
    assert_eq!(l[0].1, under, "{l:?}");
    // Text before the picture stays on its line rather than going down with it to the last space.
    let under = "_".repeat(55);
    let before = format!(r#"<w:p><w:r><w:t>ab {under}</w:t></w:r>{pic}</w:p>"#);
    let l = lines("", ARIAL_12, &before).expect("Arial");
    assert_eq!(l[0].1, format!("ab {under}"), "{l:?}");
}

#[test]
fn default_stops_before_a_hanging_indent_dont_count() {
    // TAB-3: with 15pt default stops, the tab after "o" still goes to the 72pt indent.
    let p = r#"<w:p><w:pPr><w:ind w:left="1440" w:hanging="569"/></w:pPr><w:r><w:t>o</w:t></w:r><w:r><w:tab/><w:t>text</w:t></w:r></w:p>"#;
    let Some(l) = lines(r#"<w:defaultTabStop w:val="300"/>"#, ARIAL_12, p) else { return };
    assert!(l[0].1 == "otext" && (l[0].2 - 144.0).abs() < 0.01, "{l:?}");
}

#[test]
fn the_first_line_indent_adds_to_a_floats_edge() {
    // FLOAT-7: beside a 180pt float on the left, the first line starts 27pt past its edge, the next ones at it.
    let float = r#"<w:r><w:drawing><wp:anchor distT="0" distB="0" distL="0" distR="0" simplePos="0" relativeHeight="0" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="column"><wp:posOffset>0</wp:posOffset></wp:positionH><wp:positionV relativeFrom="paragraph"><wp:posOffset>0</wp:posOffset></wp:positionV><wp:extent cx="2286000" cy="1270000"/><wp:wrapSquare wrapText="bothSides"/><a:graphic><a:graphicData><pic:pic><pic:blipFill/></pic:pic></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r>"#;
    let p = format!(
        r#"<w:p><w:pPr><w:ind w:left="-540" w:firstLine="540"/></w:pPr>{float}<w:r><w:t>Some words that run on for more than one line beside the float that is on the left.</w:t></w:r></w:p>"#
    );
    let Some(l) = lines("", ARIAL_12, &p) else { return };
    assert!((l[0].0 - 279.0).abs() < 0.01 && (l[1].0 - 252.0).abs() < 0.01, "{l:?}");
}

/// A 360 × 100pt square float 54pt into the column (9pt left and right of it, a 45pt gap each side), anchored after
/// "Hi" in the first paragraph; then `p` and "After", in Arial 12 (line height 13.8).
fn beside_float(p: &str) -> Option<Vec<(f64, String, f64, f64)>> {
    let float = r#"<w:r><w:drawing><wp:anchor distT="0" distB="0" distL="114300" distR="114300" simplePos="0" relativeHeight="0" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="column"><wp:posOffset>685800</wp:posOffset></wp:positionH><wp:positionV relativeFrom="paragraph"><wp:posOffset>0</wp:posOffset></wp:positionV><wp:extent cx="4572000" cy="1270000"/><wp:wrapSquare wrapText="bothSides"/><a:graphic><a:graphicData><pic:pic><pic:blipFill/></pic:pic></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r>"#;
    let body = format!(r#"<w:p><w:r><w:t>Hi</w:t></w:r>{float}</w:p>{p}<w:p><w:r><w:t>After</w:t></w:r></w:p>"#);
    baselines("", ARIAL_12, &body)
}

/// An inline picture `w` × `h` points.
fn inline_pic(w: u32, h: u32) -> String {
    format!(
        r#"<w:r><w:drawing><wp:inline><wp:extent cx="{}" cy="{}"/><a:graphic><a:graphicData><pic:pic><pic:blipFill/></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>"#,
        w * 12700,
        h * 12700
    )
}

#[test]
fn a_line_that_fits_no_gap_retries_lower_in_steps_of_its_height() {
    // FLOAT-4 (compat 12): the word fits neither 45pt gap, so it retries 13.8 lower each time: 7 steps put its line
    // 96.6 below its own top, 10.6 below the float (compat 15 would put it on the float's bottom).
    let Some(l) = beside_float(r#"<w:p><w:r><w:t>Supercalifragilisticexpialidocious</w:t></w:r></w:p>"#) else { return };
    let pitch = l[2].3 - l[1].3;
    assert!(((l[1].3 - l[0].3) / pitch - 8.0).abs() < 1e-6, "{l:?}");
}

#[test]
fn tabs_alone_before_a_picture_that_fits_no_gap_retry_with_it() {
    // FLOAT-4 (compat 12): the tab fits the left gap, the 64pt picture after it no gap: the line steps by the tab's
    // height (13.8), so "After" is a whole number of steps plus the picture line (60pt) below "Hi".
    let Some(l) = beside_float(&format!(r#"<w:p><w:r><w:tab/></w:r>{}</w:p>"#, inline_pic(64, 30))) else { return };
    let step = (l[1].3 - l[0].3 - 30.0) / 13.7988;
    assert!((step - step.round()).abs() < 1e-3 && l[1].3 - 30.0 > 72.0 + 100.0, "{l:?}");
}

#[test]
fn a_paragraph_of_one_too_wide_picture_stays_in_the_gap() {
    // FLOAT-4 (compat <= 14): its picture goes in the left gap, over the float, and "After" follows at its normal place.
    let Some(l) = beside_float(&format!("<w:p>{}</w:p>", inline_pic(64, 20))) else { return };
    assert!((l[1].3 - l[0].3 - 13.7988 - 20.0).abs() < 0.01, "{l:?}");
}

#[test]
fn a_last_line_of_only_a_short_picture_is_the_marks_height() {
    // LINE-4: a 4pt picture alone in its paragraph is a 13.8 line (Arial 12 mark), not a 4pt one.
    let body = format!(r#"<w:p><w:r><w:t>Hi</w:t></w:r></w:p><w:p>{}</w:p><w:p><w:r><w:t>After</w:t></w:r></w:p>"#, inline_pic(64, 4));
    let Some(l) = baselines("", ARIAL_12, &body) else { return };
    assert!((l[1].3 - l[0].3 - 2.0 * 13.7988).abs() < 0.01, "{l:?}");
}

/// A fixed one-cell table `w` twips wide holding `inner`.
fn cell(w: u32, ind: i32, inner: &str) -> String {
    format!(
        r#"<w:tbl><w:tblPr><w:tblW w:w="{w}" w:type="dxa"/><w:tblInd w:w="{ind}" w:type="dxa"/><w:tblLayout w:type="fixed"/></w:tblPr><w:tblGrid><w:gridCol w:w="{w}"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w="{w}" w:type="dxa"/></w:tcPr>{inner}<w:p/></w:tc></w:tr></w:tbl><w:p/>"#
    )
}

#[test]
fn a_nested_tables_negative_indent_counts_as_0() {
    // TABLE-25: compat 12; the nested table at -720 sits where one at 0 does.
    let at = |ind: i32| {
        let body = cell(6000, 0, &cell(5000, ind, r#"<w:p><w:r><w:t>inner</w:t></w:r></w:p>"#));
        lines("", ARIAL_12, &body).map(|l| l.iter().find(|(_, t, _)| t == "inner").expect("drawn").0)
    };
    let Some(zero) = at(0) else { return };
    assert_eq!(at(-720), Some(zero));
}

#[test]
fn a_nested_table_is_clipped_to_the_cell_holding_it() {
    // TABLE-35: a nested table wider than its 150pt cell shows only what lies inside that cell.
    let body = cell(3000, 0, &cell(9000, 0, &format!(r#"<w:p><w:r><w:t>{}</w:t></w:r></w:p>"#, "M".repeat(40))));
    let Some(l) = lines("", ARIAL_12, &body) else { return };
    let ms = l.iter().find(|(_, t, _)| t.starts_with('M')).expect("drawn");
    // Arial 12 M is 9.996pt: the cell's 150pt box holds 15 of them, the 16th's ink just reaching into it.
    assert!(ms.1.len() < 18, "{l:?}");
}

#[test]
fn a_rows_own_indent_moves_it_in_an_autofit_table_only() {
    // TABLE-50: compat 12; the second row's w:tblPrEx indent of 0 moves it 36pt left of the table's 720 in an autofit
    // table, and changes nothing in a fixed one.
    let table = |layout: &str| {
        let row = |ex: &str, t: &str| format!(r#"<w:tr>{ex}<w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>{t}</w:t></w:r></w:p></w:tc></w:tr>"#);
        let ex = r#"<w:tblPrEx><w:tblInd w:w="0" w:type="dxa"/></w:tblPrEx>"#;
        format!(
            r#"<w:tbl><w:tblPr><w:tblW w:w="0" w:type="auto"/><w:tblInd w:w="720" w:type="dxa"/><w:tblLayout w:type="{layout}"/></w:tblPr><w:tblGrid><w:gridCol w:w="3000"/></w:tblGrid>{}{}</w:tbl><w:p/>"#,
            row("", "first"),
            row(ex, "second")
        )
    };
    let x = |layout: &str, t: &str| lines("", ARIAL_12, &table(layout)).map(|l| l.iter().find(|(_, s, _)| s == t).expect("drawn").0);
    let Some(first) = x("autofit", "first") else { return };
    assert!((first - x("autofit", "second").unwrap() - 36.0).abs() < 0.01);
    assert!((x("fixed", "first").unwrap() - x("fixed", "second").unwrap()).abs() < 0.01);
}
