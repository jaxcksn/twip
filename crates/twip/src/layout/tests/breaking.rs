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
    // Compat 12 unless the settings give a mode.
    let compat = if settings.contains("compatibilityMode") {
        ""
    } else {
        r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="12"/></w:compat>"#
    };
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
    // Arial's own widths: a substitute's (Liberation Sans) miss the 3.6pt.
    if !fonts().is_installed("Arial") {
        return;
    }
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

#[test]
fn a_tight_float_wraps_around_its_polygon_box() {
    // FLOAT-8: a 180 × 100pt picture behind the text whose tight polygon is one point at its top-left corner pushes
    // only the first line, by distR; behindDoc doesn't stop it wrapping.
    let pt = r#"<wp:start x="0" y="0"/><wp:lineTo x="0" y="0"/><wp:lineTo x="0" y="0"/>"#;
    let float = format!(
        r#"<w:r><w:drawing><wp:anchor distT="0" distB="0" distL="114300" distR="114300" simplePos="0" relativeHeight="0" behindDoc="1" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="column"><wp:posOffset>0</wp:posOffset></wp:positionH><wp:positionV relativeFrom="paragraph"><wp:posOffset>0</wp:posOffset></wp:positionV><wp:extent cx="2286000" cy="1270000"/><wp:wrapTight wrapText="bothSides"><wp:wrapPolygon edited="0">{pt}</wp:wrapPolygon></wp:wrapTight><a:graphic><a:graphicData><pic:pic><pic:blipFill/></pic:pic></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r>"#
    );
    let p = format!(
        r#"<w:p>{float}<w:r><w:t>Some words that run on for more than one line beside the point at the top of the picture, and then some more words to fill a second line.</w:t></w:r></w:p>"#
    );
    let Some(l) = lines("", ARIAL_12, &p) else { return };
    assert!((l[0].0 - 81.0).abs() < 0.01 && (l[1].0 - 72.0).abs() < 0.01, "{l:?}");
}

#[test]
fn compat_14_tabs_beside_a_float_count_from_the_interval() {
    // FLOAT-7: beside a float ending at 261pt (180pt + distR 9pt), the default stop is 36pt past the interval start
    // (297pt), not the next 36pt multiple from the margin (288pt).
    let float = r#"<w:r><w:drawing><wp:anchor distT="0" distB="0" distL="0" distR="114300" simplePos="0" relativeHeight="0" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="column"><wp:posOffset>0</wp:posOffset></wp:positionH><wp:positionV relativeFrom="paragraph"><wp:posOffset>0</wp:posOffset></wp:positionV><wp:extent cx="2286000" cy="1270000"/><wp:wrapSquare wrapText="bothSides"/><a:graphic><a:graphicData><pic:pic><pic:blipFill/></pic:pic></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r>"#;
    let p = format!(r#"<w:p>{float}<w:r><w:tab/><w:t>x</w:t></w:r></w:p>"#);
    let Some(l) = lines("", ARIAL_12, &p) else { return };
    assert!((l[0].0 - 297.0).abs() < 0.01, "{l:?}");
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
    // TABLE-25: in compat 12 and 15, the nested table at -720 sits where one at 0 does.
    let c15 = r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat>"#;
    for settings in ["", c15] {
        let at = |ind: i32| {
            let body = cell(6000, 0, &cell(5000, ind, r#"<w:p><w:r><w:t>inner</w:t></w:r></w:p>"#));
            lines(settings, ARIAL_12, &body).map(|l| l.iter().find(|(_, t, _)| t == "inner").expect("drawn").0)
        };
        let Some(zero) = at(0) else { return };
        assert_eq!(at(-720), Some(zero), "{settings}");
    }
}

#[test]
fn a_nested_table_wider_than_its_cell_is_drawn_in_full() {
    // TABLE-35: a nested table wider than its 150pt cell isn't clipped to that cell: all 40 M (Arial 12, 400pt) show.
    let body = cell(3000, 0, &cell(9000, 0, &format!(r#"<w:p><w:r><w:t>{}</w:t></w:r></w:p>"#, "M".repeat(40))));
    let Some(l) = lines("", ARIAL_12, &body) else { return };
    let ms = l.iter().find(|(_, t, _)| t.starts_with('M')).expect("drawn");
    assert_eq!(ms.1.len(), 40, "{l:?}");
}

#[test]
fn a_rows_own_indent_moves_it_in_an_autofit_table_only() {
    // TABLE-50: compat 12; the second row's w:tblPrEx indent of 0 moves it 36pt left of the table's 720 in an autofit
    // table, and changes nothing in a fixed one.
    let table = |layout: &str| {
        let row = |ex: &str, t: &str| {
            format!(r#"<w:tr>{ex}<w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>{t}</w:t></w:r></w:p></w:tc></w:tr>"#)
        };
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

#[test]
fn a_compat_15_section_break_after_a_table_takes_a_line_in_balanced_columns() {
    // COL-6: two continuous columns end with a one-row table; in compat 15 the empty paragraph holding the section
    // break still takes a line below it, so the next section starts a line lower than in compat 12.
    let c15 = r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat>"#;
    let row = r#"<w:tbl><w:tblPr><w:tblW w:w="0" w:type="auto"/></w:tblPr><w:tblGrid><w:gridCol w:w="2000"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w="2000" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>Cell</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#;
    let body = format!(
        r#"<w:p><w:r><w:t>One</w:t></w:r></w:p>{row}<w:p><w:pPr><w:sectPr><w:type w:val="continuous"/><w:cols w:num="2" w:space="720"/><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr></w:pPr></w:p><w:p><w:r><w:t>After</w:t></w:r></w:p><w:p><w:pPr><w:sectPr><w:type w:val="continuous"/></w:sectPr></w:pPr></w:p>"#
    );
    let after = |settings: &str| baselines(settings, ARIAL_12, &body).and_then(|l| l.into_iter().find(|l| l.1 == "After")).map(|l| l.3);
    let (Some(a12), Some(a15)) = (after(""), after(c15)) else { return };
    assert!((a15 - a12 - 13.8).abs() < 0.05, "{a12} {a15}");
}

#[test]
fn a_compat_14_exact_row_is_capped_to_the_column() {
    // TABLE-28: an exact row of 1000pt holding 60 lines: compat 12 caps it to the 648pt column and clips the lines
    // past it; compat 15 keeps 1000pt, past the page bottom, and shows the lines down to the page edge.
    let c15 = r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat>"#;
    let lines_in = (0..60).map(|i| format!("<w:p><w:r><w:t>Line {i}</w:t></w:r></w:p>")).collect::<String>();
    let body = format!(
        r#"<w:tbl><w:tblPr><w:tblW w:w="5000" w:type="dxa"/></w:tblPr><w:tblGrid><w:gridCol w:w="5000"/></w:tblGrid><w:tr><w:trPr><w:trHeight w:val="20000" w:hRule="exact"/></w:trPr><w:tc><w:tcPr><w:tcW w:w="5000" w:type="dxa"/></w:tcPr>{lines_in}</w:tc></w:tr></w:tbl><w:p/>"#
    );
    let count = |settings: &str| lines(settings, ARIAL_12, &body).map(|l| l.iter().filter(|(_, t, _)| t.starts_with("Line")).count());
    let (Some(c12), Some(c15)) = (count(""), count(c15)) else { return };
    assert!(c12 < 50 && c15 > c12 + 4, "{c12} {c15}");
}

#[test]
fn a_float_whose_anchor_its_wrap_pushes_off_the_page_leaves_the_page() {
    // FLOAT-11: a 60pt topAndBottom float at page y 100, anchored in a line that fits page 1 only without the float:
    // the float wraps lines 3 on, which pushes its anchor to page 2. Page 1 is laid out again without the float but
    // ends where that pass did: its lines aren't wrapped and the anchor starts page 2.
    let float = r#"<w:r><w:drawing><wp:anchor distT="0" distB="0" distL="0" distR="0" simplePos="0" relativeHeight="0" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="margin"><wp:posOffset>0</wp:posOffset></wp:positionH><wp:positionV relativeFrom="page"><wp:posOffset>1270000</wp:posOffset></wp:positionV><wp:extent cx="5588000" cy="762000"/><wp:wrapTopAndBottom/><a:graphic><a:graphicData><pic:pic><pic:blipFill/></pic:pic></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r>"#;
    let p = |t: &str| format!(r#"<w:p><w:pPr><w:spacing w:after="0"/></w:pPr><w:r><w:t>{t}</w:t></w:r></w:p>"#);
    let body: String = (0..44).map(|i| p(&format!("L{i:02}"))).collect::<String>()
        + &format!(r#"<w:p><w:pPr><w:spacing w:after="0"/></w:pPr>{float}<w:r><w:t>ANCHOR</w:t></w:r></w:p>"#)
        + &p("AFTER");
    // (`baselines` reads page 1.)
    let Some(l) = baselines("", ARIAL_12, &body) else { return };
    let l02 = l.iter().find(|x| x.1 == "L02").map(|x| x.3);
    assert!(l02.is_some_and(|y| y < 120.0) && !l.iter().any(|x| x.1.ends_with("ANCHOR")), "{l:?}");
}

#[test]
fn no_column_balance_leaves_a_continuous_sections_columns_unbalanced() {
    // COL-5: six one-line paragraphs in two columns before a continuous break balance three and three; compat ≤ 14
    // noColumnBalance keeps them all in the first column.
    let sect = |cols: &str| {
        format!(
            r#"<w:sectPr><w:type w:val="continuous"/><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/>{cols}</w:sectPr>"#
        )
    };
    let paras: String = (1..=6).map(|k| format!("<w:p><w:r><w:t>p{k}</w:t></w:r></w:p>")).collect();
    let body = format!(
        r#"{paras}<w:p><w:pPr>{}</w:pPr></w:p><w:p><w:pPr>{}</w:pPr><w:r><w:t>Tail</w:t></w:r></w:p>"#,
        sect(r#"<w:cols w:num="2" w:space="720"/>"#),
        sect("")
    );
    let right = |settings: &str| lines(settings, ARIAL_12, &body).map(|l| l.iter().filter(|(x, s, _)| s.starts_with('p') && *x > 200.0).count());
    let Some(balanced) = right("") else { return };
    assert_eq!((balanced, right("<w:compat><w:noColumnBalance/></w:compat>").unwrap()), (3, 0));
}

#[test]
fn a_line_break_beside_a_float_ends_only_its_interval() {
    // FLOAT-7: "aa", a text-wrapping break, then "bb", beside a float in mid-column: "aa" fills the left gap and "bb"
    // goes on in the right gap of the same line.
    let Some(l) = beside_float(r#"<w:p><w:r><w:t>aa</w:t><w:br/><w:t>bb</w:t></w:r></w:p>"#) else { return };
    let (aa, bb) = (l.iter().find(|x| x.1 == "aa").expect("aa"), l.iter().find(|x| x.1 == "bb").expect("bb"));
    assert!((aa.3 - bb.3).abs() < 0.01 && bb.0 > 400.0, "{l:?}");
}

#[test]
fn a_compat_14_column_aligned_float_counts_from_the_paragraph_indent() {
    // FLOAT-6 (compat 12): a 40pt float aligned left in the column, anchored in a paragraph indented 36pt, sits at
    // 108 (not 72), so the text beside it starts at 148.
    let float = r#"<w:r><w:drawing><wp:anchor distT="0" distB="0" distL="0" distR="0" simplePos="0" relativeHeight="0" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="column"><wp:align>left</wp:align></wp:positionH><wp:positionV relativeFrom="paragraph"><wp:posOffset>0</wp:posOffset></wp:positionV><wp:extent cx="508000" cy="1270000"/><wp:wrapSquare wrapText="bothSides"/><a:graphic><a:graphicData><pic:pic><pic:blipFill/></pic:pic></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r>"#;
    let p = format!(r#"<w:p><w:pPr><w:ind w:left="720"/></w:pPr>{float}<w:r><w:t>Beside the float.</w:t></w:r></w:p>"#);
    let Some(l) = lines("", ARIAL_12, &p) else { return };
    assert!((l[0].0 - 148.0).abs() < 0.01, "{l:?}");
}

#[test]
fn a_compat_14_line_goes_to_a_floats_bottom_when_another_starts_inside_it() {
    // FLOAT-7: two floating tables in the flow, the second moved below the first; the paragraph's second line would
    // cross the first's bottom with the second starting inside it, so it starts at that bottom instead.
    let table = |x: u32, rows: &str| {
        format!(
            r#"<w:tbl><w:tblPr><w:tblpPr w:leftFromText="180" w:rightFromText="180" w:vertAnchor="text" w:horzAnchor="margin" w:tblpX="{x}" w:tblpY="0"/><w:tblW w:w="3000" w:type="dxa"/><w:tblBorders><w:top w:val="single" w:sz="4" w:space="0" w:color="000000"/><w:bottom w:val="single" w:sz="4" w:space="0" w:color="000000"/></w:tblBorders><w:tblLayout w:type="fixed"/></w:tblPr><w:tblGrid><w:gridCol w:w="3000"/></w:tblGrid>{rows}</w:tbl>"#
        )
    };
    let row = |t: &str| format!(r#"<w:tr><w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>{t}</w:t></w:r></w:p></w:tc></w:tr>"#);
    let text = "Some words that run on for more than one line beside the two tables, and then some more words to fill the lines.";
    let body = format!(
        r#"<w:p><w:r><w:t>Top</w:t></w:r></w:p>{}<w:p/>{}<w:p><w:r><w:t>{text}</w:t></w:r></w:p>"#,
        table(1000, &format!("{}{}", row("A1"), row("A2"))),
        table(1500, &row("B1"))
    );
    let Some(l) = baselines("", ARIAL_12, &body) else { return };
    // The lines starting at the margin, left of the tables: "Some", then "the two" below the first table.
    let y: Vec<f64> = l.iter().filter(|(x, t, _, _)| *x < 73.0 && t.len() > 2 && text.contains(t.as_str())).map(|l| l.3).collect();
    assert!(y.len() >= 2 && y[1] - y[0] > 13.8 + 0.5, "{l:?}");
}

#[test]
fn a_floating_tables_obstacle_sits_on_device_pixels() {
    // TABLE-49: compat 12, a table at tblpX 1000 from the margin: its outer box starts at 116.35pt, the frame on the
    // 0.12pt pixel rounded from one twip left of that (116.28pt), so the text right of it starts 0.07pt left of where
    // the box's right edge and its 0.5pt distance put it (272.27pt).
    let body = r#"<w:tbl><w:tblPr><w:tblpPr w:vertAnchor="text" w:horzAnchor="margin" w:tblpX="1000" w:tblpY="0"/><w:tblW w:w="3000" w:type="dxa"/><w:tblBorders><w:left w:val="single" w:sz="4" w:space="0" w:color="000000"/><w:right w:val="single" w:sz="4" w:space="0" w:color="000000"/></w:tblBorders><w:tblLayout w:type="fixed"/></w:tblPr><w:tblGrid><w:gridCol w:w="3000"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>cell</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p><w:r><w:t>Left words beside the table on the right side.</w:t></w:r></w:p>"#;
    let Some(l) = lines("", ARIAL_12, body) else { return };
    let right = l.iter().find(|(x, t, _)| *x > 200.0 && t != "cell").expect("text right of the table");
    assert!((right.0 - 272.2).abs() < 0.01, "{l:?}");
}

#[test]
fn a_right_aligned_floating_tables_frame_counts_a_missing_border_whole() {
    // TABLE-49: compat 15, a 300pt right-aligned floating table; its frame is 300.75pt wide with no right border at
    // all (the cell's text at 72 + 468 - 300.75 + 5.4), 300pt with a `none` one.
    let at = |borders: &str| {
        let body = format!(
            r#"<w:tbl><w:tblPr><w:tblpPr w:vertAnchor="page" w:tblpXSpec="right" w:tblpY="2000"/><w:tblW w:w="6000" w:type="dxa"/><w:tblBorders>{borders}</w:tblBorders><w:tblLayout w:type="fixed"/><w:tblCellMar><w:left w:w="108" w:type="dxa"/><w:right w:w="108" w:type="dxa"/></w:tblCellMar></w:tblPr><w:tblGrid><w:gridCol w:w="6000"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w="6000" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>cell</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p/>"#
        );
        let c15 = r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat>"#;
        lines(c15, ARIAL_12, &body).map(|l| l.iter().find(|(_, t, _)| t == "cell").expect("drawn").0)
    };
    let Some(missing) = at("") else { return };
    assert!((missing - 244.65).abs() < 0.01, "{missing}");
    let none = at(r#"<w:right w:val="none" w:sz="0" w:space="0" w:color="000000"/>"#).unwrap();
    assert!((none - 245.4).abs() < 0.01, "{none}");
}

#[test]
fn compat_14_wp_justification_lets_a_justified_line_run_past_its_edge() {
    // JUST-7: with wpJustification a compat-14 justified line is broken 18.25pt past the 6.5in column (one more "ab"),
    // then squeezed back into it.
    let p = format!(r#"<w:p><w:pPr><w:jc w:val="both"/></w:pPr><w:r><w:t>{}</w:t></w:r></w:p>"#, "ab ".repeat(80));
    let c14 = r#"<w:compat>{}<w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="14"/></w:compat>"#;
    let first = |opt: &str| lines(&c14.replace("{}", opt), ARIAL_12, &p).map(|l| (l[0].1.matches("ab").count(), l[0].2));
    let Some(plain) = first("") else { return };
    let (wide, last_x) = first("<w:wpJustification/>").unwrap();
    assert_eq!(wide, plain.0 + 1, "{plain:?}");
    assert!(last_x < 540.0, "{last_x}");
}

#[test]
fn compat_14_word97_line_breaks_size_a_named_east_asian_runs_glyphs_at_a_1000_unit_em() {
    // FONT-18: Courier New 12 glyphs are 600/1000 em, 29491 layout units each instead of the design 29496, so with
    // useWord97LineBreakRules the 65 characters of eleven "xxxxx" fit the 468pt line; the design widths overflow it.
    if !fonts().is_installed("Courier New") {
        return;
    }
    let p = format!("<w:p><w:r><w:t>{}</w:t></w:r></w:p>", "xxxxx ".repeat(30));
    let c14 = r#"<w:compat>{}<w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="14"/></w:compat>"#;
    let words = |opt: &str, ea: &str| {
        let rpr = format!(r#"<w:rFonts w:ascii="Courier New" w:hAnsi="Courier New" w:eastAsia="{ea}"/><w:sz w:val="24"/>"#);
        lines(&c14.replace("{}", opt), &rpr, &p).map(|l| l[0].1.matches("xxxxx").count())
    };
    let Some(plain) = words("", "Arial") else { return };
    let opt = "<w:useWord97LineBreakRules/>";
    assert_eq!((plain, words(opt, "Arial").unwrap(), words(opt, "Times New Roman").unwrap()), (10, 11, 10));
}

#[test]
fn a_compat_14_float_past_the_column_bottom_stays_with_its_anchor() {
    // FLOAT-9: a 100 × 60pt square float at the top of a paragraph 41pt above the column bottom runs past it; compat 14
    // neither moves it up nor pushes the paragraph to the next page, so "ANCHOR" stays on page 1 beside the float.
    let float = r#"<w:r><w:drawing><wp:anchor distT="0" distB="0" distL="0" distR="0" simplePos="0" relativeHeight="0" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="column"><wp:posOffset>0</wp:posOffset></wp:positionH><wp:positionV relativeFrom="paragraph"><wp:posOffset>0</wp:posOffset></wp:positionV><wp:extent cx="1270000" cy="762000"/><wp:wrapSquare wrapText="bothSides"/><a:graphic><a:graphicData><pic:pic><pic:blipFill/></pic:pic></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r>"#;
    let p = |t: &str| format!(r#"<w:p><w:pPr><w:spacing w:after="0"/></w:pPr><w:r><w:t>{t}</w:t></w:r></w:p>"#);
    let body = (0..44).map(|i| p(&format!("L{i:02}"))).collect::<String>()
        + &format!(r#"<w:p><w:pPr><w:spacing w:after="0"/></w:pPr>{float}<w:r><w:t>ANCHOR</w:t></w:r></w:p>"#);
    let c14 = r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="14"/></w:compat>"#;
    let Some(l) = lines(c14, ARIAL_12, &body) else { return };
    assert!(l.iter().any(|x| x.1 == "ANCHOR" && (x.0 - 172.0).abs() < 0.01), "{l:?}");
}
