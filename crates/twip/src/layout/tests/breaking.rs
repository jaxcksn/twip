//! Line breaking

use crate::docx::blank::blank_docx;
use crate::docx::write::repackage;
use crate::layout::fragment::Item;
use crate::layout::layout;
use crate::layout::traverse::paint;

use super::fonts;

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

fn lines(settings: &str, rpr: &str, body: &str) -> Option<Vec<(f64, String, f64)>> {
    Some(baselines(settings, rpr, body)?.into_iter().map(|(x, t, last, _)| (x, t, last)).collect())
}

fn baselines(settings: &str, rpr: &str, body: &str) -> Option<Vec<(f64, String, f64, f64)>> {
    page_baselines(settings, rpr, body, 0)
}

fn page_baselines(settings: &str, rpr: &str, body: &str, page: usize) -> Option<Vec<(f64, String, f64, f64)>> {
    numbered_page(settings, rpr, body, "", page)
}

/// As [`baselines`], with a numbering part when `numbering` (its `w:abstractNum` and `w:num` elements) isn't empty.
fn numbered_baselines(settings: &str, rpr: &str, body: &str, numbering: &str) -> Option<Vec<(f64, String, f64, f64)>> {
    numbered_page(settings, rpr, body, numbering, 0)
}

fn numbered_page(settings: &str, rpr: &str, body: &str, numbering: &str, page: usize) -> Option<Vec<(f64, String, f64, f64)>> {
    let fonts = fonts();
    let doc = test_doc(settings, rpr, body, numbering)?;
    let snap = layout(&doc, &fonts);
    let mut out: Vec<(u32, f64, String, f64, f64)> = Vec::new();
    paint(&snap.pages()[page], &mut |p| {
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

/// The document of [`numbered_baselines`]: compat 12 unless `settings` sets a mode; `None` without Arial.
pub(super) fn test_doc(settings: &str, rpr: &str, body: &str, numbering: &str) -> Option<crate::docx::Document> {
    fonts().face("Arial", None, false, false)?;
    let compat = if settings.contains("compatibilityMode") {
        ""
    } else {
        r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="12"/></w:compat>"#
    };
    let mut parts = vec![
        (
            "word/document.xml",
            format!(
                r#"<w:document xmlns:w="{W}" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture"><w:body>{body}<w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr></w:body></w:document>"#
            ),
        ),
        ("word/styles.xml", format!(r#"<w:styles xmlns:w="{W}"><w:docDefaults><w:rPrDefault><w:rPr>{rpr}</w:rPr></w:rPrDefault><w:pPrDefault/></w:docDefaults></w:styles>"#)),
        ("word/settings.xml", format!(r#"<w:settings xmlns:w="{W}">{settings}<w:characterSpacingControl w:val="compressPunctuation"/>{compat}</w:settings>"#)),
        (
            "word/_rels/document.xml.rels",
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings" Target="settings.xml"/><Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering" Target="numbering.xml"/></Relationships>"#.to_string(),
        ),
    ];
    if !numbering.is_empty() {
        parts.push(("word/numbering.xml", format!(r#"<w:numbering xmlns:w="{W}">{numbering}</w:numbering>"#)));
    }
    let parts: Vec<(String, Vec<u8>)> = parts.into_iter().map(|(n, x)| (n.to_string(), x.into_bytes())).collect();
    Some(crate::docx::read(&repackage(&blank_docx(), &parts, zip::CompressionMethod::Deflated).expect("repackages")).expect("reads"))
}

const ARIAL_12: &str = r#"<w:rFonts w:ascii="Arial" w:hAnsi="Arial" w:eastAsiaTheme="minorEastAsia"/><w:sz w:val="24"/>"#;

#[test]
fn spaces_after_a_tab_squeeze_a_word_in() {
    let p =
        r#"<w:p><w:r><w:tab/><w:t>You are ordered to appear at a hearing on Monday, April 4, 2025 at 10:00 am with your papers.</w:t></w:r></w:p>"#;
    if !fonts().is_installed("Arial") {
        return;
    }
    let Some(l) = lines("", ARIAL_12, p) else { return };
    assert!(l[0].1.ends_with("am with"), "{l:?}");
}

#[test]
fn squeezed_spaces_shrink_against_their_own_font_not_the_east_asian_one() {
    let p =
        r#"<w:p><w:r><w:t>decorum warrant with status arrest court their which its be business trial for notice records entered is</w:t></w:r></w:p>"#;
    if !fonts().is_installed("Arial") || !fonts().is_installed("MS Mincho") {
        return;
    }
    let rpr = r#"<w:rFonts w:ascii="Arial" w:hAnsi="Arial" w:eastAsia="MS Mincho"/><w:sz w:val="24"/>"#;
    let Some(l) = lines("", rpr, p) else { return };
    assert!(l[0].1.ends_with("records"), "{l:?}");
}

#[test]
fn a_line_breaks_before_and_after_an_inline_picture() {
    let pic = r#"<w:r><w:drawing><wp:inline><wp:extent cx="1333500" cy="476250"/><a:graphic><a:graphicData><pic:pic><pic:blipFill/></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>"#;
    let under = "_".repeat(60);
    let after = format!(r#"<w:p>{pic}<w:r><w:t>{under}</w:t></w:r></w:p>"#);
    let Some(l) = lines("", ARIAL_12, &after) else { return };
    assert_eq!(l[0].1, under, "{l:?}");
    let under = "_".repeat(55);
    let before = format!(r#"<w:p><w:r><w:t>ab {under}</w:t></w:r>{pic}</w:p>"#);
    let l = lines("", ARIAL_12, &before).expect("Arial");
    assert_eq!(l[0].1, format!("ab {under}"), "{l:?}");
}

#[test]
fn default_stops_before_a_hanging_indent_dont_count() {
    let p = r#"<w:p><w:pPr><w:ind w:left="1440" w:hanging="569"/></w:pPr><w:r><w:t>o</w:t></w:r><w:r><w:tab/><w:t>text</w:t></w:r></w:p>"#;
    let Some(l) = lines(r#"<w:defaultTabStop w:val="300"/>"#, ARIAL_12, p) else { return };
    assert!(l[0].1 == "otext" && (l[0].2 - 144.0).abs() < 0.01, "{l:?}");
}

#[test]
fn the_first_line_indent_adds_to_a_floats_edge() {
    let float = r#"<w:r><w:drawing><wp:anchor distT="0" distB="0" distL="0" distR="0" simplePos="0" relativeHeight="0" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="column"><wp:posOffset>0</wp:posOffset></wp:positionH><wp:positionV relativeFrom="paragraph"><wp:posOffset>0</wp:posOffset></wp:positionV><wp:extent cx="2286000" cy="1270000"/><wp:wrapSquare wrapText="bothSides"/><a:graphic><a:graphicData><pic:pic><pic:blipFill/></pic:pic></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r>"#;
    let p = format!(
        r#"<w:p><w:pPr><w:ind w:left="-540" w:firstLine="540"/></w:pPr>{float}<w:r><w:t>Some words that run on for more than one line beside the float that is on the left.</w:t></w:r></w:p>"#
    );
    let Some(l) = lines("", ARIAL_12, &p) else { return };
    assert!((l[0].0 - 279.0).abs() < 0.01 && (l[1].0 - 252.0).abs() < 0.01, "{l:?}");
}

/// The first two lines' left edges of a paragraph with `ind` beside a 100pt float at the column's left edge.
fn beside_left_float(settings: &str, ind: &str) -> Option<(f64, f64)> {
    let float = r#"<w:r><w:drawing><wp:anchor distT="0" distB="0" distL="0" distR="0" simplePos="0" relativeHeight="0" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="column"><wp:posOffset>0</wp:posOffset></wp:positionH><wp:positionV relativeFrom="paragraph"><wp:posOffset>0</wp:posOffset></wp:positionV><wp:extent cx="1270000" cy="1270000"/><wp:wrapSquare wrapText="bothSides"/><a:graphic><a:graphicData><pic:pic><pic:blipFill/></pic:pic></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r>"#;
    let p = format!(r#"<w:p><w:pPr><w:ind {ind}/></w:pPr>{float}<w:r><w:t>{}</w:t></w:r></w:p>"#, "Alpha bravo charlie delta echo. ".repeat(6));
    let l = lines(settings, ARIAL_12, &p)?;
    Some((l[0].0, l[1].0))
}

#[test]
fn a_hanging_first_line_beside_a_float_counts_from_the_piece_in_compat_14_and_stays_in_it_in_compat_15() {
    let c15 = r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat>"#;
    let ind = r#"w:left="1440" w:hanging="720""#;
    let Some(c12) = beside_left_float("", ind) else { return };
    let c15 = beside_left_float(c15, ind).unwrap();
    assert!((c12.0 - 208.0).abs() < 0.01 && (c12.1 - 172.0).abs() < 0.01, "{c12:?}");
    assert!((c15.0 - 172.0).abs() < 0.01 && (c15.1 - 172.0).abs() < 0.01, "{c15:?}");
}

#[test]
fn compat_14_do_not_suppress_indentation_drops_the_left_indent_beside_a_float() {
    let opt = r#"<w:compat><w:doNotSuppressIndentation/><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="12"/></w:compat>"#;
    let Some(plain) = beside_left_float("", r#"w:left="2880""#) else { return };
    let dropped = beside_left_float(opt, r#"w:left="2880""#).unwrap();
    let first = beside_left_float(opt, r#"w:firstLine="720""#).unwrap();
    assert!((plain.0 - 216.0).abs() < 0.01 && (dropped.0 - 172.0).abs() < 0.01 && (dropped.1 - 172.0).abs() < 0.01, "{plain:?} {dropped:?}");
    assert!((first.0 - 208.0).abs() < 0.01 && (first.1 - 172.0).abs() < 0.01, "{first:?}");
}

#[test]
fn a_tight_float_wraps_around_its_polygon_box() {
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
    let float = r#"<w:r><w:drawing><wp:anchor distT="0" distB="0" distL="0" distR="114300" simplePos="0" relativeHeight="0" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="column"><wp:posOffset>0</wp:posOffset></wp:positionH><wp:positionV relativeFrom="paragraph"><wp:posOffset>0</wp:posOffset></wp:positionV><wp:extent cx="2286000" cy="1270000"/><wp:wrapSquare wrapText="bothSides"/><a:graphic><a:graphicData><pic:pic><pic:blipFill/></pic:pic></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r>"#;
    let p = format!(r#"<w:p>{float}<w:r><w:tab/><w:t>x</w:t></w:r></w:p>"#);
    let Some(l) = lines("", ARIAL_12, &p) else { return };
    assert!((l[0].0 - 297.0).abs() < 0.01, "{l:?}");
}

fn beside_float(p: &str) -> Option<Vec<(f64, String, f64, f64)>> {
    let float = r#"<w:r><w:drawing><wp:anchor distT="0" distB="0" distL="114300" distR="114300" simplePos="0" relativeHeight="0" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="column"><wp:posOffset>685800</wp:posOffset></wp:positionH><wp:positionV relativeFrom="paragraph"><wp:posOffset>0</wp:posOffset></wp:positionV><wp:extent cx="4572000" cy="1270000"/><wp:wrapSquare wrapText="bothSides"/><a:graphic><a:graphicData><pic:pic><pic:blipFill/></pic:pic></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r>"#;
    let body = format!(r#"<w:p><w:r><w:t>Hi</w:t></w:r>{float}</w:p>{p}<w:p><w:r><w:t>After</w:t></w:r></w:p>"#);
    baselines("", ARIAL_12, &body)
}

fn inline_pic(w: u32, h: u32) -> String {
    format!(
        r#"<w:r><w:drawing><wp:inline><wp:extent cx="{}" cy="{}"/><a:graphic><a:graphicData><pic:pic><pic:blipFill/></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>"#,
        w * 12700,
        h * 12700
    )
}

#[test]
fn text_alignment_aligns_an_inline_picture_and_a_list_number_like_runs() {
    let m = r#"<w:p><w:pPr><w:spacing w:after="0" w:line="300" w:lineRule="exact"/></w:pPr><w:r><w:t>m</w:t></w:r></w:p>"#;
    let top = |runs: &str, numbered: bool| {
        let num = if numbered { r#"<w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr>"# } else { "" };
        let body = format!(r#"{m}<w:p><w:pPr>{num}<w:spacing w:after="0"/><w:textAlignment w:val="top"/></w:pPr>{runs}</w:p>{m}"#);
        let numbering = r#"<w:abstractNum w:abstractNumId="1"><w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="decimal"/><w:lvlText w:val="%1."/><w:lvlJc w:val="left"/></w:lvl></w:abstractNum><w:num w:numId="1"><w:abstractNumId w:val="1"/></w:num>"#;
        numbered_baselines("", ARIAL_11, &body, numbering).map(|l| l.last().map_or(0.0, |l| l.3) - 72.0 - 15.0 - 12.0)
    };
    let Some(picture) = top(&format!("<w:r><w:t xml:space=\"preserve\">to </w:t></w:r>{}", inline_pic(30, 30)), false) else { return };
    // A picture hangs below the reference like a run whose ascent is its height: the line is the picture and Arial's
    // line gap (and a unit) above it.
    assert!((picture - 30.0 - 67.0 * 11.0 / 2048.0).abs() < 0.05, "{picture}");
    // A list number shifts like the text in its font: it adds nothing above.
    let text = top("<w:r><w:t>to</w:t></w:r>", false).expect("lays out");
    assert!((top("<w:r><w:t>to</w:t></w:r>", true).unwrap() - text).abs() < 1e-6);
}

#[test]
fn a_line_that_fits_no_gap_retries_lower_in_steps_of_its_height() {
    let Some(l) = beside_float(r#"<w:p><w:r><w:t>Supercalifragilisticexpialidocious</w:t></w:r></w:p>"#) else { return };
    let pitch = l[2].3 - l[1].3;
    assert!(((l[1].3 - l[0].3) / pitch - 8.0).abs() < 1e-6, "{l:?}");
}

#[test]
fn tabs_alone_before_a_picture_that_fits_no_gap_retry_with_it() {
    let Some(l) = beside_float(&format!(r#"<w:p><w:r><w:tab/></w:r>{}</w:p>"#, inline_pic(64, 30))) else { return };
    let step = (l[1].3 - l[0].3 - 30.0) / 13.7988;
    assert!((step - step.round()).abs() < 1e-3 && l[1].3 - 30.0 > 72.0 + 100.0, "{l:?}");
}

#[test]
fn a_paragraph_of_one_too_wide_picture_stays_in_the_gap() {
    let Some(l) = beside_float(&format!("<w:p>{}</w:p>", inline_pic(64, 20))) else { return };
    assert!((l[1].3 - l[0].3 - 13.7988 - 20.0).abs() < 0.01, "{l:?}");
}

#[test]
fn a_last_line_of_only_a_short_picture_is_the_marks_height() {
    let body = format!(r#"<w:p><w:r><w:t>Hi</w:t></w:r></w:p><w:p>{}</w:p><w:p><w:r><w:t>After</w:t></w:r></w:p>"#, inline_pic(64, 4));
    let Some(l) = baselines("", ARIAL_12, &body) else { return };
    assert!((l[1].3 - l[0].3 - 2.0 * 13.7988).abs() < 0.01, "{l:?}");
}

fn cell(w: u32, ind: i32, inner: &str) -> String {
    format!(
        r#"<w:tbl><w:tblPr><w:tblW w:w="{w}" w:type="dxa"/><w:tblInd w:w="{ind}" w:type="dxa"/><w:tblLayout w:type="fixed"/></w:tblPr><w:tblGrid><w:gridCol w:w="{w}"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w="{w}" w:type="dxa"/></w:tcPr>{inner}<w:p/></w:tc></w:tr></w:tbl><w:p/>"#
    )
}

#[test]
fn a_nested_tables_negative_indent_counts_as_0() {
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
    let body = cell(3000, 0, &cell(9000, 0, &format!(r#"<w:p><w:r><w:t>{}</w:t></w:r></w:p>"#, "M".repeat(40))));
    let Some(l) = lines("", ARIAL_12, &body) else { return };
    let ms = l.iter().find(|(_, t, _)| t.starts_with('M')).expect("drawn");
    assert_eq!(ms.1.len(), 40, "{l:?}");
}

#[test]
fn a_rows_own_indent_counts_only_when_it_also_fixes_the_rows_layout() {
    let table = |layout: &str, ex: &str| {
        let row = |ex: &str, t: &str| {
            format!(r#"<w:tr>{ex}<w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>{t}</w:t></w:r></w:p></w:tc></w:tr>"#)
        };
        format!(
            r#"<w:tbl><w:tblPr><w:tblW w:w="0" w:type="auto"/><w:tblInd w:w="720" w:type="dxa"/><w:tblLayout w:type="{layout}"/></w:tblPr><w:tblGrid><w:gridCol w:w="3000"/></w:tblGrid>{}{}</w:tbl><w:p/>"#,
            row("", "first"),
            row(&format!(r#"<w:tblPrEx>{ex}<w:tblInd w:w="0" w:type="dxa"/></w:tblPrEx>"#), "second")
        )
    };
    let shift = |layout: &str, ex: &str| {
        let l = lines("", ARIAL_12, &table(layout, ex))?;
        let x = |t: &str| l.iter().find(|(_, s, _)| s == t).expect("drawn").0;
        Some(x("first") - x("second"))
    };
    let Some(autofit) = shift("autofit", "") else { return };
    assert!(autofit.abs() < 0.01, "{autofit}");
    assert!(shift("fixed", "").unwrap().abs() < 0.01);
    let fixes = r#"<w:tblLayout w:type="fixed"/>"#;
    assert!((shift("autofit", fixes).unwrap() - 36.0).abs() < 0.01);
    assert!(shift("fixed", fixes).unwrap().abs() < 0.01);
}

#[test]
fn a_rows_exceptions_give_it_its_own_borders_and_cell_margins() {
    let body = |ex: &str| {
        let row = |ex: &str, t: &str| format!(r#"<w:tr>{ex}<w:tc><w:p><w:r><w:t>{t}</w:t></w:r></w:p></w:tc></w:tr>"#);
        format!(
            r#"<w:tbl><w:tblPr><w:tblW w:w="3000" w:type="dxa"/><w:tblLayout w:type="fixed"/><w:tblCellMar><w:left w:w="100" w:type="dxa"/></w:tblCellMar></w:tblPr><w:tblGrid><w:gridCol w:w="3000"/></w:tblGrid>{}{}{}</w:tbl><w:p/>"#,
            row("", "first"),
            row(ex, "second"),
            row("", "third")
        )
    };
    let x = |ex: &str, t: &str| lines("", ARIAL_12, &body(ex)).map(|l| l.iter().find(|(_, s, _)| s == t).expect("drawn").0);
    let Some(plain) = x("", "second") else { return };
    let wide = r#"<w:tblPrEx><w:tblCellMar><w:left w:w="500" w:type="dxa"/></w:tblCellMar></w:tblPrEx>"#;
    assert!((x(wide, "second").unwrap() - plain - 20.0).abs() < 0.01);
    assert!((x(wide, "third").unwrap() - x("", "third").unwrap()).abs() < 0.01, "only its own row");
    let doc = |ex: &str| {
        let parts = [(
            "word/document.xml".to_string(),
            format!(r#"<w:document xmlns:w="{W}"><w:body>{}<w:sectPr/></w:body></w:document>"#, body(ex)).into_bytes(),
        )];
        crate::docx::read(&repackage(&blank_docx(), &parts, zip::CompressionMethod::Deflated).expect("repackages")).expect("reads")
    };
    let lined = r#"<w:tblPrEx><w:tblBorders><w:insideH w:val="single" w:sz="8" w:space="0" w:color="000000"/></w:tblBorders></w:tblPrEx>"#;
    let d = doc(lined);
    let Some(crate::docx::Block::Table(t)) = d.sections[0].blocks.first() else { panic!("a table") };
    assert!(t.rows[1].borders.as_ref().is_some_and(|b| b[4].is_some()) && t.rows[0].borders.is_none() && t.borders[4].is_none());
    let h = |d: &crate::docx::Document| {
        let s = layout(d, &fonts());
        let mut n = 0;
        paint(&s.pages()[0], &mut |p| {
            if let Item::Rect { rect, .. } = p.absolute() {
                n += usize::from((rect[3] - rect[1]).abs() < 2.0 && (rect[2] - rect[0]) > 100.0);
            }
        });
        n
    };
    assert!(h(&d) > h(&doc("")), "the row draws its exception's inside border");
}

#[test]
fn a_compat_15_section_break_after_a_table_takes_a_line_in_balanced_columns() {
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
    let float = r#"<w:r><w:drawing><wp:anchor distT="0" distB="0" distL="0" distR="0" simplePos="0" relativeHeight="0" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="margin"><wp:posOffset>0</wp:posOffset></wp:positionH><wp:positionV relativeFrom="page"><wp:posOffset>1270000</wp:posOffset></wp:positionV><wp:extent cx="5588000" cy="762000"/><wp:wrapTopAndBottom/><a:graphic><a:graphicData><pic:pic><pic:blipFill/></pic:pic></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r>"#;
    let p = |t: &str| format!(r#"<w:p><w:pPr><w:spacing w:after="0"/></w:pPr><w:r><w:t>{t}</w:t></w:r></w:p>"#);
    let body: String = (0..44).map(|i| p(&format!("L{i:02}"))).collect::<String>()
        + &format!(r#"<w:p><w:pPr><w:spacing w:after="0"/></w:pPr>{float}<w:r><w:t>ANCHOR</w:t></w:r></w:p>"#)
        + &p("AFTER");
    let Some(l) = baselines("", ARIAL_12, &body) else { return };
    let l02 = l.iter().find(|x| x.1 == "L02").map(|x| x.3);
    assert!(l02.is_some_and(|y| y < 120.0) && !l.iter().any(|x| x.1.ends_with("ANCHOR")), "{l:?}");
}

#[test]
fn no_column_balance_leaves_a_continuous_sections_columns_unbalanced() {
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
    let Some(l) = beside_float(r#"<w:p><w:r><w:t>aa</w:t><w:br/><w:t>bb</w:t></w:r></w:p>"#) else { return };
    let (aa, bb) = (l.iter().find(|x| x.1 == "aa").expect("aa"), l.iter().find(|x| x.1 == "bb").expect("bb"));
    assert!((aa.3 - bb.3).abs() < 0.01 && bb.0 > 400.0, "{l:?}");
}

#[test]
fn a_compat_14_float_on_whole_twips_wraps_the_line_whose_space_after_it_starts_in() {
    // The line above ends 0.48 tw into a twip: compat ≤ 14 puts the float on that twip, inside the line's space after.
    let float = r#"<w:r><w:drawing><wp:anchor distT="0" distB="0" distL="0" distR="114300" simplePos="0" relativeHeight="0" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="column"><wp:posOffset>0</wp:posOffset></wp:positionH><wp:positionV relativeFrom="paragraph"><wp:posOffset>0</wp:posOffset></wp:positionV><wp:extent cx="914400" cy="914400"/><wp:wrapSquare wrapText="bothSides"/><a:graphic><a:graphicData><pic:pic><pic:blipFill/></pic:pic></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r>"#;
    let body = format!(r#"<w:p><w:pPr><w:spacing w:after="160"/></w:pPr><w:r><w:t>Before</w:t></w:r></w:p><w:p>{float}<w:r><w:t>Anchor</w:t></w:r></w:p>"#);
    let rpr = r#"<w:rFonts w:ascii="Arial" w:hAnsi="Arial"/><w:sz w:val="21"/>"#;
    let c15 = r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat>"#;
    let Some(c12) = lines("", rpr, &body) else { return };
    let c15 = lines(c15, rpr, &body).unwrap();
    assert!((c12[0].0 - 153.0).abs() < 0.01, "{c12:?}");
    assert!((c15[0].0 - 72.0).abs() < 0.01, "{c15:?}");
    // A 12pt line ends 0.98 tw into a twip, so the float starts below it; between paragraphs of one style with
    // contextual spacing the space after isn't kept, so the float doesn't meet the line there either.
    let contextual = body.replace(r#"<w:spacing w:after="160"/>"#, r#"<w:spacing w:after="160"/><w:contextualSpacing/>"#).replace("<w:p><w:r><w:drawing>", "<w:p><w:pPr><w:contextualSpacing/></w:pPr><w:r><w:drawing>");
    let kept = lines("", ARIAL_12, &contextual).unwrap();
    assert!((kept[0].0 - 72.0).abs() < 0.01, "{kept:?}");
}

#[test]
fn a_float_that_cannot_stay_with_its_anchor_leaves_the_text_before_it_in_place() {
    // A top-and-bottom picture taller than the rest of the page: its anchor paragraph goes to the next page, and the
    // line it met on the whole twip it was placed at stays where it was.
    let float = r#"<w:r><w:drawing><wp:anchor distT="0" distB="0" distL="0" distR="0" simplePos="0" relativeHeight="0" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="column"><wp:posOffset>0</wp:posOffset></wp:positionH><wp:positionV relativeFrom="paragraph"><wp:posOffset>0</wp:posOffset></wp:positionV><wp:extent cx="914400" cy="8255000"/><wp:wrapTopAndBottom/><a:graphic><a:graphicData><pic:pic><pic:blipFill/></pic:pic></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r>"#;
    let body = format!(r#"<w:p><w:r><w:t>Lead</w:t></w:r></w:p><w:p><w:r><w:t>Lead</w:t></w:r></w:p><w:p><w:r><w:t>Before</w:t></w:r></w:p><w:p>{float}<w:r><w:t>Anchor</w:t></w:r></w:p>"#);
    let rpr = r#"<w:rFonts w:ascii="Arial" w:hAnsi="Arial"/><w:sz w:val="21"/>"#;
    let Some(first) = page_baselines("", rpr, &body, 0) else { return };
    assert!(first.iter().any(|l| l.1 == "Before") && !first.iter().any(|l| l.1 == "Anchor"), "{first:?}");
}

#[test]
fn a_compat_14_float_anchored_beside_another_counts_its_column_offset_from_the_lines_piece() {
    let pic = |x: u32, w: u32, h: u32, dr: u32| {
        format!(
            r#"<w:r><w:drawing><wp:anchor distT="0" distB="0" distL="0" distR="{dr}" simplePos="0" relativeHeight="0" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="column"><wp:posOffset>{x}</wp:posOffset></wp:positionH><wp:positionV relativeFrom="paragraph"><wp:posOffset>0</wp:posOffset></wp:positionV><wp:extent cx="{w}" cy="{h}"/><wp:wrapSquare wrapText="bothSides"/><a:graphic><a:graphicData><pic:pic><pic:blipFill/></pic:pic></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r>"#
        )
    };
    let body = format!(
        r#"<w:p>{}<w:r><w:t>One.</w:t></w:r></w:p><w:p>{}<w:r><w:t>{}</w:t></w:r></w:p>"#,
        pic(0, 914400, 1143000, 114300),
        pic(1905000, 635000, 317500, 0),
        "Alpha bravo charlie delta echo. ".repeat(6)
    );
    // Float B 150pt from the column: from the line's piece (153pt, beside A) in compat 12, from the column in compat 15.
    let second_piece = |settings: &str| {
        let l = baselines(settings, ARIAL_12, &body)?;
        let y = l.iter().find(|x| x.1.starts_with("Alpha"))?.3;
        l.iter().filter(|x| (x.3 - y).abs() < 0.01).map(|x| x.0).nth(1)
    };
    let c15 = r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat>"#;
    let Some(c12) = second_piece("") else { return };
    assert!((c12 - 353.0).abs() < 0.01, "{c12}");
    assert!((second_piece(c15).unwrap() - 272.0).abs() < 0.01);
}

#[test]
fn a_float_is_kept_on_the_page_by_its_box_not_its_distances() {
    let first = |offset: i64, settings: &str| {
        let float = format!(
            r#"<w:r><w:drawing><wp:anchor distT="0" distB="0" distL="114300" distR="114300" simplePos="0" relativeHeight="0" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="column"><wp:posOffset>{offset}</wp:posOffset></wp:positionH><wp:positionV relativeFrom="paragraph"><wp:posOffset>0</wp:posOffset></wp:positionV><wp:extent cx="1270000" cy="1270000"/><wp:wrapSquare wrapText="bothSides"/><a:graphic><a:graphicData><pic:pic><pic:blipFill/></pic:pic></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r>"#
        );
        Some(lines(settings, ARIAL_12, &format!(r#"<w:p>{float}<w:r><w:t>{}</w:t></w:r></w:p>"#, "Alpha bravo charlie. ".repeat(8)))?[0].0)
    };
    let c15 = r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat>"#;
    // Compat 15: the box at 5pt stays though its distance runs off the page.
    let Some(kept) = first(-850900, c15) else { return };
    assert!((kept - 114.0).abs() < 0.01, "{kept}");
    // Compat ≤ 14: a box from -8pt is pulled onto the page.
    let clamped = first(-1016000, "").unwrap();
    assert!((clamped - 109.0).abs() < 0.01, "{clamped}");
}

#[test]
fn a_compat_14_column_aligned_float_counts_from_the_paragraph_indent() {
    let float = r#"<w:r><w:drawing><wp:anchor distT="0" distB="0" distL="0" distR="0" simplePos="0" relativeHeight="0" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="column"><wp:align>left</wp:align></wp:positionH><wp:positionV relativeFrom="paragraph"><wp:posOffset>0</wp:posOffset></wp:positionV><wp:extent cx="508000" cy="1270000"/><wp:wrapSquare wrapText="bothSides"/><a:graphic><a:graphicData><pic:pic><pic:blipFill/></pic:pic></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r>"#;
    let p = format!(r#"<w:p><w:pPr><w:ind w:left="720"/></w:pPr>{float}<w:r><w:t>Beside the float.</w:t></w:r></w:p>"#);
    let Some(l) = lines("", ARIAL_12, &p) else { return };
    assert!((l[0].0 - 148.0).abs() < 0.01, "{l:?}");
}

#[test]
fn a_compat_14_line_goes_to_a_floats_bottom_when_another_starts_inside_it() {
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
    let y: Vec<f64> = l.iter().filter(|(x, t, _, _)| *x < 73.0 && t.len() > 2 && text.contains(t.as_str())).map(|l| l.3).collect();
    assert!(y.len() >= 2 && y[1] - y[0] > 13.8 + 0.5, "{l:?}");
}

#[test]
fn a_compat_14_line_pushed_to_a_floats_bottom_goes_below_one_ending_within_its_height_of_its_top() {
    let table = |y: u32, w: u32, borders: &str| {
        format!(
            r#"<w:tbl><w:tblPr><w:tblpPr w:leftFromText="180" w:rightFromText="180" w:vertAnchor="page" w:horzAnchor="page" w:tblpX="4000" w:tblpY="{y}"/><w:tblW w:w="{w}" w:type="dxa"/><w:tblLayout w:type="fixed"/><w:tblCellMar><w:left w:w="108" w:type="dxa"/><w:right w:w="108" w:type="dxa"/></w:tblCellMar></w:tblPr><w:tblGrid><w:gridCol w:w="{w}"/></w:tblGrid><w:tr><w:trPr><w:trHeight w:val="200" w:hRule="exact"/></w:trPr><w:tc><w:tcPr><w:tcW w:w="{w}" w:type="dxa"/>{borders}</w:tcPr><w:p><w:pPr><w:spacing w:before="0" w:after="0" w:line="200" w:lineRule="exact"/></w:pPr><w:r><w:t>T</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#
        )
    };
    let b = |s: &str| format!(r#"<w:{s} w:val="single" w:sz="24" w:space="0" w:color="000000"/>"#);
    let borders = format!("<w:tcBorders>{}{}{}{}</w:tcBorders>", b("top"), b("left"), b("bottom"), b("right"));
    let p = |t: &str| format!(r#"<w:p><w:pPr><w:spacing w:before="0" w:after="0" w:line="300" w:lineRule="exact"/></w:pPr><w:r><w:t>{t}</w:t></w:r></w:p>"#);
    let line = "Body text runs on beside the box so that";
    let test_line = |g: u32| {
        let body = format!("{}{}{}{}{}", p(line), table(1490, 1440, &borders), table(1750 + g, 1640, ""), p(&format!("Test line {line}")), p("After."));
        baselines("", ARIAL_12, &body).map(|l| l.iter().find(|(_, t, _, _)| t.starts_with("Test")).expect("drawn").3)
    };
    // The test line's top (1740 tw) is inside A (ending at 1749 tw); B starts inside the line, g below A.
    let Some(below) = test_line(91) else { return };
    // B ends 300 tw below the line's top, its height: the line goes below B.
    assert!((below - (2040.0 + 240.0) / 20.0).abs() < 0.1, "{below}");
    // B ends 301 tw below it: the line stays at A's bottom.
    let at_a = test_line(92).unwrap();
    assert!((at_a - (1749.0 + 240.0) / 20.0).abs() < 0.1, "{at_a}");
}

#[test]
fn a_floating_tables_obstacle_starts_a_twip_left_of_tblpx() {
    let body = r#"<w:tbl><w:tblPr><w:tblpPr w:vertAnchor="text" w:horzAnchor="margin" w:tblpX="1000" w:tblpY="0"/><w:tblW w:w="3000" w:type="dxa"/><w:tblBorders><w:left w:val="single" w:sz="4" w:space="0" w:color="000000"/><w:right w:val="single" w:sz="4" w:space="0" w:color="000000"/></w:tblBorders><w:tblLayout w:type="fixed"/></w:tblPr><w:tblGrid><w:gridCol w:w="3000"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>cell</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p><w:r><w:t>Left words beside the table on the right side.</w:t></w:r></w:p>"#;
    let Some(l) = lines("", ARIAL_12, body) else { return };
    let right = l.iter().find(|(x, t, _)| *x > 200.0 && t != "cell").expect("text right of the table");
    assert!((right.0 - 272.2).abs() < 0.01, "{l:?}");
    let c15 = r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat>"#;
    let l = lines(c15, ARIAL_12, body).unwrap();
    let right = l.iter().find(|(x, t, _)| *x > 200.0 && t != "cell").expect("text right of the table");
    // Compat 15: from the table's left edge.
    assert!((right.0 - 272.95).abs() < 0.01, "{l:?}");
}

#[test]
fn a_compat_14_floating_tables_indent_widens_its_obstacle_to_the_right() {
    let body = |ind: u32| {
        format!(
            r#"<w:tbl><w:tblPr><w:tblpPr w:leftFromText="180" w:rightFromText="180" w:vertAnchor="text" w:horzAnchor="page" w:tblpX="4000" w:tblpY="0"/><w:tblW w:w="1440" w:type="dxa"/><w:tblInd w:w="{ind}" w:type="dxa"/><w:tblLayout w:type="fixed"/><w:tblCellMar><w:left w:w="108" w:type="dxa"/><w:right w:w="108" w:type="dxa"/></w:tblCellMar></w:tblPr><w:tblGrid><w:gridCol w:w="1440"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w="1440" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>cell</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p><w:r><w:t>{}</w:t></w:r></w:p>"#,
            "a ".repeat(60)
        )
    };
    let pieces = |compat: &str, ind: u32| {
        let l = lines(compat, ARIAL_12, &body(ind))?;
        let left = l.iter().find(|(x, t, _)| *x < 100.0 && t.starts_with('a')).expect("left piece").1.len();
        let right = l.iter().find(|(x, t, _)| *x > 200.0 && t.starts_with('a')).expect("right piece").0;
        Some((left, right))
    };
    let Some((left, right)) = pieces("", 0) else { return };
    // Compat ≤ 14: the obstacle's left edge stays (3696 tw); its right edge moves out by the indent.
    let (l2, r2) = pieces("", 200).unwrap();
    assert!(l2 == left && (r2 - right - 10.0).abs() < 0.01, "{left} {right} {l2} {r2}");
    // Compat 15 ignores the indent.
    let c15 = r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat>"#;
    assert_eq!(pieces(c15, 0), pieces(c15, 200));
}

#[test]
fn a_floating_tables_obstacle_counts_a_missing_border_as_a_thin_one() {
    let body = r#"<w:tbl><w:tblPr><w:tblpPr w:leftFromText="180" w:rightFromText="180" w:vertAnchor="text" w:horzAnchor="margin" w:tblpX="200" w:tblpY="0"/><w:tblW w:w="1440" w:type="dxa"/><w:tblLayout w:type="fixed"/><w:tblCellMar><w:left w:w="108" w:type="dxa"/><w:right w:w="108" w:type="dxa"/></w:tblCellMar></w:tblPr><w:tblGrid><w:gridCol w:w="1440"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w="1440" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>cell</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p><w:r><w:t>Words beside the table on the right side.</w:t></w:r></w:p>"#;
    let right = |compat: &str| lines(compat, ARIAL_12, body).map(|l| l.iter().find(|(_, t, _)| t.starts_with("Words")).expect("text beside the table").0);
    let Some(c14) = right("") else { return };
    // Compat ≤ 14 starts the table its first cell's margin before `tblpX`, and counts 0.75pt either side.
    assert!((c14 - 158.3).abs() < 0.01, "{c14}");
    let c15 = right(r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat>"#).unwrap();
    assert!((c15 - 163.7).abs() < 0.01, "{c15}");
}

#[test]
fn a_floating_tables_obstacle_takes_nothing_for_a_table_nil_and_tblpx_codes_align_it() {
    let table = |x: &str, tbl: &str, tc: &str| {
        format!(
            r#"<w:tbl><w:tblPr><w:tblpPr w:leftFromText="180" w:rightFromText="180" w:vertAnchor="text" w:horzAnchor="margin" w:tblpX="{x}" w:tblpY="0"/><w:tblW w:w="1440" w:type="dxa"/>{tbl}<w:tblLayout w:type="fixed"/><w:tblCellMar><w:left w:w="108" w:type="dxa"/><w:right w:w="108" w:type="dxa"/></w:tblCellMar></w:tblPr><w:tblGrid><w:gridCol w:w="1440"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w="1440" w:type="dxa"/>{tc}</w:tcPr><w:p><w:r><w:t>cell</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p><w:r><w:t>Words beside the table on the right side.</w:t></w:r></w:p>"#
        )
    };
    let c15 = r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat>"#;
    let right = |body: String| lines(c15, ARIAL_12, &body).map(|l| l.iter().find(|(_, t, _)| t.starts_with("Words")).expect("text beside the table").0);
    let nil = |side: &str| format!(r#"<w:{side} w:val="nil"/>"#);
    let Some(table_nil) = right(table("200", &format!("<w:tblBorders>{}</w:tblBorders>", nil("right")), "")) else { return };
    // 1639 + 1440 + 180 tw: no part for the table's nil; a cell's nil is missing, 0.75pt.
    assert!((table_nil - 162.95).abs() < 0.01, "{table_nil}");
    let cell_nil = right(table("200", &format!("<w:tblBorders>{}</w:tblBorders>", nil("right")), &format!("<w:tcBorders>{}</w:tcBorders>", nil("right")))).unwrap();
    assert!((cell_nil - 163.7).abs() < 0.01, "{cell_nil}");
    // `tblpX` -4 centres the table: text goes on left of it.
    let centred = right(table("-4", "", "")).unwrap();
    assert!((centred - 72.0).abs() < 0.01, "{centred}");
}

#[test]
fn a_compat_14_table_in_the_flow_drops_when_its_text_box_meets_the_earlier_tables_obstacle() {
    let table = |x: u32, rows: usize, label: &str| {
        let row = format!(r#"<w:tr><w:trPr><w:trHeight w:val="200" w:hRule="exact"/></w:trPr><w:tc><w:tcPr><w:tcW w:w="1440" w:type="dxa"/></w:tcPr><w:p><w:pPr><w:spacing w:before="0" w:after="0" w:line="200" w:lineRule="exact"/></w:pPr><w:r><w:t>{label}</w:t></w:r></w:p></w:tc></w:tr>"#);
        format!(
            r#"<w:tbl><w:tblPr><w:tblpPr w:leftFromText="0" w:rightFromText="0" w:vertAnchor="text" w:horzAnchor="page" w:tblpX="{x}" w:tblpY="0"/><w:tblW w:w="1440" w:type="dxa"/><w:tblLayout w:type="fixed"/><w:tblCellMar><w:left w:w="108" w:type="dxa"/><w:right w:w="108" w:type="dxa"/></w:tblCellMar></w:tblPr><w:tblGrid><w:gridCol w:w="1440"/></w:tblGrid>{}</w:tbl>"#,
            row.repeat(rows)
        )
    };
    let p = |t: &str| format!(r#"<w:p><w:pPr><w:spacing w:before="0" w:after="0" w:line="300" w:lineRule="exact"/></w:pPr><w:r><w:t>{t}</w:t></w:r></w:p>"#);
    let b_top = |bx: u32| {
        let body = format!("{}{}{}{}{}", p("Lead."), table(4000, 3, "A"), p("Between."), table(bx, 1, "B"), p("After."));
        baselines("", ARIAL_12, &body).map(|l| l.iter().find(|(_, t, _, _)| t == "B").expect("B drawn").3)
    };
    // A's obstacle ends at 5356 tw. B's text box from 5356 only touches it: B stays.
    let Some(stays) = b_top(5357) else { return };
    assert!(stays < 115.0, "{stays}");
    // From 5355 it overlaps it, though B's cells start past A's text: B goes to A's bottom.
    let drops = b_top(5356).unwrap();
    assert!(drops > 120.0, "{drops}");
}

#[test]
fn a_compat_14_table_dropped_in_the_flow_from_the_nearest_twip_stays_at_its_x_when_it_may_not_overlap() {
    let table = |x: u32, label: &str, never: &str| {
        let row = format!(r#"<w:tr><w:trPr><w:trHeight w:val="200" w:hRule="exact"/></w:trPr><w:tc><w:tcPr><w:tcW w:w="2000" w:type="dxa"/></w:tcPr><w:p><w:pPr><w:spacing w:before="0" w:after="0" w:line="200" w:lineRule="exact"/></w:pPr><w:r><w:t>{label}</w:t></w:r></w:p></w:tc></w:tr>"#);
        format!(
            r#"<w:tbl><w:tblPr><w:tblpPr w:leftFromText="0" w:rightFromText="0" w:vertAnchor="text" w:horzAnchor="page" w:tblpX="{x}" w:tblpY="0"/>{never}<w:tblW w:w="2000" w:type="dxa"/><w:tblLayout w:type="fixed"/><w:tblCellMar><w:left w:w="108" w:type="dxa"/><w:right w:w="108" w:type="dxa"/></w:tblCellMar></w:tblPr><w:tblGrid><w:gridCol w:w="2000"/></w:tblGrid>{}</w:tbl>"#,
            row.repeat(3)
        )
    };
    // After 28 single lines the earlier table ends 0.4 tw past a twip: the later one starts at that twip.
    let lead: String = (0..28).map(|i| format!("<w:p><w:r><w:t>Lead {i}.</w:t></w:r></w:p>")).collect();
    let body = format!("{lead}{}{}<w:p><w:r><w:t>After.</w:t></w:r></w:p>", table(4000, "A", ""), table(4500, "B", r#"<w:tblOverlap w:val="never"/>"#));
    let Some(l) = baselines("", ARIAL_11, &body) else { return };
    let at = |t: &str| l.iter().find(|x| x.1 == t).map(|x| (x.0, x.3)).expect("drawn");
    let ((ax, ay), (bx, by)) = (at("A"), at("B"));
    assert!(((by - ay) * 20.0 - 599.6).abs() < 0.05 && (bx - ax - 25.0).abs() < 0.01, "{ax} {ay} {bx} {by}");
}

#[test]
fn a_right_aligned_floating_tables_frame_counts_a_missing_border_whole() {
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
    let float = r#"<w:r><w:drawing><wp:anchor distT="0" distB="0" distL="0" distR="0" simplePos="0" relativeHeight="0" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="column"><wp:posOffset>0</wp:posOffset></wp:positionH><wp:positionV relativeFrom="paragraph"><wp:posOffset>0</wp:posOffset></wp:positionV><wp:extent cx="1270000" cy="762000"/><wp:wrapSquare wrapText="bothSides"/><a:graphic><a:graphicData><pic:pic><pic:blipFill/></pic:pic></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r>"#;
    let p = |t: &str| format!(r#"<w:p><w:pPr><w:spacing w:after="0"/></w:pPr><w:r><w:t>{t}</w:t></w:r></w:p>"#);
    let body = (0..44).map(|i| p(&format!("L{i:02}"))).collect::<String>()
        + &format!(r#"<w:p><w:pPr><w:spacing w:after="0"/></w:pPr>{float}<w:r><w:t>ANCHOR</w:t></w:r></w:p>"#);
    let c14 = r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="14"/></w:compat>"#;
    let Some(l) = lines(c14, ARIAL_12, &body) else { return };
    assert!(l.iter().any(|x| x.1 == "ANCHOR" && (x.0 - 172.0).abs() < 0.01), "{l:?}");
}

#[test]
fn a_paragraph_anchoring_a_float_keeps_its_top_border() {
    let float = r#"<w:r><w:drawing><wp:anchor distT="0" distB="0" distL="0" distR="0" simplePos="0" relativeHeight="0" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="column"><wp:posOffset>4572000</wp:posOffset></wp:positionH><wp:positionV relativeFrom="paragraph"><wp:posOffset>2540000</wp:posOffset></wp:positionV><wp:extent cx="635000" cy="635000"/><wp:wrapSquare wrapText="bothSides"/><a:graphic><a:graphicData><pic:pic><pic:blipFill/></pic:pic></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r>"#;
    let p = |f: &str| {
        format!(
            r#"<w:p><w:pPr><w:pBdr><w:top w:val="single" w:sz="24" w:space="4" w:color="000000"/></w:pBdr></w:pPr>{f}<w:r><w:t>Boxed</w:t></w:r></w:p>"#
        )
    };
    let y = |f: &str| baselines("", ARIAL_12, &p(f)).map(|l| l[0].3);
    let Some(plain) = y("") else { return };
    assert!((y(float).unwrap() - plain).abs() < 0.01, "{plain}");
}

#[test]
fn a_nested_groups_text_boxes_each_draw_in_their_own_box() {
    let emu = |pt: u32| pt * 12700;
    let xfrm = |x: u32, w: u32, cw: u32| {
        format!(
            r#"<a:xfrm><a:off x="{}" y="0"/><a:ext cx="{}" cy="{}"/><a:chOff x="0" y="0"/><a:chExt cx="{}" cy="{}"/></a:xfrm>"#,
            emu(x),
            emu(w),
            emu(50),
            emu(cw),
            emu(50)
        )
    };
    let wsp = |x: u32, t: &str| {
        format!(
            r#"<wps:wsp><wps:spPr><a:xfrm><a:off x="{}" y="0"/><a:ext cx="{}" cy="{}"/></a:xfrm></wps:spPr><wps:txbx><w:txbxContent><w:p><w:r><w:t>{t}</w:t></w:r></w:p></w:txbxContent></wps:txbx><wps:bodyPr lIns="0" tIns="0" rIns="0" bIns="0"/></wps:wsp>"#,
            emu(x),
            emu(80),
            emu(50)
        )
    };
    let group = format!(
        r#"<w:r><w:drawing><wp:anchor distT="0" distB="0" distL="0" distR="0" simplePos="0" relativeHeight="0" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="page"><wp:posOffset>{}</wp:posOffset></wp:positionH><wp:positionV relativeFrom="page"><wp:posOffset>{}</wp:posOffset></wp:positionV><wp:extent cx="{}" cy="{}"/><wp:wrapNone/><a:graphic><a:graphicData><wpg:wgp xmlns:wpg="http://schemas.microsoft.com/office/word/2010/wordprocessingGroup" xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wpg:grpSpPr>{}</wpg:grpSpPr>{}<wpg:grpSp><wpg:grpSpPr>{}</wpg:grpSpPr>{}</wpg:grpSp></wpg:wgp></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r>"#,
        emu(100),
        emu(400),
        emu(100),
        emu(25),
        xfrm(0, 100, 200),
        wsp(0, "Outer"),
        xfrm(100, 100, 100),
        wsp(50, "Inner")
    );
    let Some(l) = lines("", ARIAL_12, &format!("<w:p>{group}<w:r><w:t>Body</w:t></w:r></w:p>")) else { return };
    let x = |t: &str| l.iter().find(|x| x.1 == t).map(|x| x.0);
    assert_eq!((x("Outer"), x("Inner")), (Some(100.0), Some(175.0)), "{l:?}");
}

#[test]
fn text_wraps_around_the_lines_of_a_bare_tight_text_box() {
    let emu = |pt: u32| pt * 12700;
    let tight = r#"<wp:wrapTight wrapText="bothSides"><wp:wrapPolygon edited="0"><wp:start x="0" y="0"/><wp:lineTo x="0" y="21600"/><wp:lineTo x="21600" y="21600"/><wp:lineTo x="21600" y="0"/><wp:lineTo x="0" y="0"/></wp:wrapPolygon></wp:wrapTight>"#;
    let text_box = |fill: &str| {
        format!(
            r#"<w:r><w:drawing><wp:anchor distT="0" distB="0" distL="0" distR="0" simplePos="0" relativeHeight="0" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="page"><wp:posOffset>{}</wp:posOffset></wp:positionH><wp:positionV relativeFrom="page"><wp:posOffset>{}</wp:posOffset></wp:positionV><wp:extent cx="{}" cy="{}"/>{tight}<wp:docPr id="1" name="T"/><a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wps:wsp xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wps:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="{}" cy="{}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom>{fill}<a:ln><a:noFill/></a:ln></wps:spPr><wps:txbx><w:txbxContent><w:p><w:pPr><w:textboxTightWrap w:val="allLines"/></w:pPr><w:r><w:t>Ab</w:t></w:r></w:p></w:txbxContent></wps:txbx><wps:bodyPr lIns="0" tIns="0" rIns="0" bIns="0"/></wps:wsp></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r>"#,
            emu(200),
            emu(72),
            emu(200),
            emu(60),
            emu(200),
            emu(60)
        )
    };
    let body = |fill: &str| format!("<w:p>{}<w:r><w:t>{}</w:t></w:r></w:p>", text_box(fill), "words run on beside the box ".repeat(12));
    let beside = |l: &[(f64, String, f64)]| l.iter().filter(|x| x.0 > 150.0 && x.1 != "Ab").map(|x| x.0).fold(f64::INFINITY, f64::min);
    let Some(bare) = lines("", ARIAL_12, &body("<a:noFill/>")) else { return };
    let filled = lines("", ARIAL_12, &body(r#"<a:solidFill><a:srgbClr val="FFFF00"/></a:solidFill>"#)).unwrap();
    assert!(beside(&bare) < 260.0, "beside the line's ink: {bare:?}");
    assert!(beside(&filled) >= 400.0, "beside the box: {filled:?}");
}

#[test]
fn a_compat_14_line_meets_a_bare_text_boxs_lines_over_its_text_only() {
    let line = |t: &str| format!(r#"<w:p><w:pPr><w:spacing w:after="0" w:line="240" w:lineRule="auto"/><w:textboxTightWrap w:val="allLines"/></w:pPr><w:r><w:t>{t}</w:t></w:r></w:p>"#);
    let text_box = format!(
        r#"<w:r><w:drawing><wp:anchor distT="0" distB="0" distL="0" distR="0" simplePos="0" relativeHeight="0" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="column"><wp:posOffset>0</wp:posOffset></wp:positionH><wp:positionV relativeFrom="paragraph"><wp:posOffset>0</wp:posOffset></wp:positionV><wp:extent cx="2540000" cy="762000"/><wp:wrapTight wrapText="bothSides"><wp:wrapPolygon edited="0"><wp:start x="0" y="0"/><wp:lineTo x="0" y="21600"/><wp:lineTo x="21600" y="21600"/><wp:lineTo x="21600" y="0"/><wp:lineTo x="0" y="0"/></wp:wrapPolygon></wp:wrapTight><wp:docPr id="1" name="T"/><a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wps:wsp xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wps:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="2540000" cy="762000"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:noFill/><a:ln><a:noFill/></a:ln></wps:spPr><wps:txbx><w:txbxContent>{}{}</w:txbxContent></wps:txbx><wps:bodyPr lIns="0" tIns="0" rIns="0" bIns="0"/></wps:wsp></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r>"#,
        line("Ab"),
        line("Abcdefghijklmnopqrstu")
    );
    // A 1.5-spaced body line: its text ends where the box's second line starts, the room its spacing adds below runs
    // past it.
    let body = format!(r#"<w:p><w:pPr><w:spacing w:after="0" w:line="360" w:lineRule="auto"/></w:pPr>{text_box}<w:r><w:t>{}</w:t></w:r></w:p>"#, "words run on beside the box ".repeat(12));
    let Some(l) = lines("", ARIAL_12, &body) else { return };
    let first = l.iter().find(|x| x.1.starts_with("words")).expect("drawn").0;
    // Right of the first line's ink, not the second's.
    assert!(first < 100.0, "{l:?}");
}

#[test]
fn an_empty_section_break_paragraph_after_a_paragraph_takes_no_line() {
    let sect = r#"<w:p><w:pPr><w:sectPr><w:type w:val="continuous"/><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr></w:pPr></w:p>"#;
    let p = |t: &str| format!(r#"<w:p><w:pPr><w:spacing w:after="0"/></w:pPr><w:r><w:t>{t}</w:t></w:r></w:p>"#);
    let body = format!("{}{}{}{sect}", p("A"), sect.replace(r#"<w:type w:val="continuous"/>"#, ""), p("B"));
    for mode in [14, 15] {
        let c = format!(
            r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="{mode}"/></w:compat>"#
        );
        let Some(l) = baselines(&c, ARIAL_12, &body) else { return };
        assert!((l[1].3 - l[0].3 - 13.8).abs() < 0.01, "{l:?}");
    }
}

#[test]
fn an_empty_section_break_paragraph_after_a_table_is_laid_out_in_compat_15_only_without_its_space_before() {
    // The closing paragraph holds a bookmark only: still empty.
    let sect = r#"<w:p><w:pPr><w:spacing w:before="1200" w:after="0"/><w:sectPr><w:type w:val="continuous"/><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr></w:pPr><w:bookmarkStart w:id="0" w:name="b"/><w:bookmarkEnd w:id="0"/></w:p>"#;
    let table = r#"<w:tbl><w:tblGrid><w:gridCol w:w="4000"/></w:tblGrid><w:tr><w:tc><w:p><w:pPr><w:spacing w:after="0"/></w:pPr><w:r><w:t>Cell</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#;
    // "Next" ends a continuous section of its own, so it follows on the same page.
    let close = r#"<w:p><w:pPr><w:sectPr><w:type w:val="continuous"/></w:sectPr></w:pPr></w:p>"#;
    let body = format!(r#"{table}{sect}<w:p><w:pPr><w:spacing w:after="0"/></w:pPr><w:r><w:t>Next</w:t></w:r></w:p>{close}"#);
    let gap = |mode: u32| {
        let c = format!(r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="{mode}"/></w:compat>"#);
        baselines(&c, ARIAL_12, &body).map(|l| {
            let y = |t: &str| l.iter().find(|x| x.1 == t).unwrap_or_else(|| panic!("{l:?}")).3;
            y("Next") - y("Cell")
        })
    };
    let Some(c14) = gap(14) else { return };
    let c15 = gap(15).unwrap();
    assert!(c14 < 20.0, "{c14}");
    assert!((c15 - c14 - 13.8).abs() < 0.5, "{c14} {c15}");
}

#[test]
fn a_continuous_sections_first_paragraph_pushed_to_a_new_page_keeps_its_space_before() {
    // 46 lines fill page 1; the next paragraph (before 12pt, after the previous paragraph's 0) starts page 2.
    let p = |t: &str, ppr: &str| format!(r#"<w:p><w:pPr><w:spacing w:before="0" w:after="0"/>{ppr}</w:pPr><w:r><w:t>{t}</w:t></w:r></w:p>"#);
    let fill: String = (0..45).map(|k| p(&format!("Line {k}"), "")).collect();
    let next = r#"<w:p><w:pPr><w:spacing w:before="240" w:after="0"/></w:pPr><w:r><w:t>First</w:t></w:r></w:p>"#;
    let close = r#"<w:p><w:pPr><w:sectPr><w:type w:val="continuous"/></w:sectPr></w:pPr></w:p>"#;
    let y = |sect: &str| {
        let body = format!("{fill}{}{next}{close}", p("Last", sect));
        page_baselines("", ARIAL_12, &body, 1).map(|l| l.iter().find(|x| x.1 == "First").unwrap_or_else(|| panic!("{l:?}")).3)
    };
    let Some(flow) = y("") else { return };
    let section = y("<w:sectPr/>").unwrap();
    assert!((section - flow - 12.0).abs() < 0.01, "{flow} {section}");
}

#[test]
fn an_empty_deleted_mark_paragraph_before_a_table_isnt_laid_out() {
    let del = r#"w:id="1" w:author="A" w:date="2026-01-01T00:00:00Z""#;
    let gone = format!(
        r#"<w:p><w:pPr><w:spacing w:before="480" w:after="480"/><w:rPr><w:del {del}/></w:rPr></w:pPr><w:del {del}><w:r><w:delText>Deleted</w:delText></w:r></w:del></w:p>"#
    );
    let table = r#"<w:tbl><w:tblGrid><w:gridCol w:w="4000"/></w:tblGrid><w:tr><w:tc><w:p><w:r><w:t>Row</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p/>"#;
    let top = r#"<w:p><w:pPr><w:spacing w:after="0"/></w:pPr><w:r><w:t>Top</w:t></w:r></w:p>"#;
    let gap = |mid: &str| {
        let mut doc = test_doc("", ARIAL_12, &format!("{top}{mid}{table}"), "")?;
        doc.review.view = crate::docx::review::MarkupView::NoMarkup;
        let snap = layout(&doc, &fonts());
        let mut y = std::collections::HashMap::new();
        paint(&snap.pages()[0], &mut |p| {
            if let Item::Glyphs(r) = p.absolute() {
                y.insert(r.chars.iter().map(|c| c.0).collect::<String>(), r.baseline);
            }
        });
        Some(y["Row"] - y["Top"])
    };
    let Some(without) = gap("") else { return };
    assert!((gap(&gone).unwrap() - without).abs() < 0.01);
}

#[test]
fn a_page_break_in_a_table_cell_is_nothing() {
    let body = r#"<w:tbl><w:tblGrid><w:gridCol w:w="4000"/></w:tblGrid><w:tr><w:tc><w:p><w:r><w:t>Before</w:t><w:br w:type="page"/><w:t>After</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p/>"#;
    let Some(l) = lines("", ARIAL_12, body) else { return };
    assert_eq!(l[0].1, "BeforeAfter", "{l:?}");
}

#[test]
fn compat_15_text_after_a_leading_page_break_in_a_cell_goes_on_at_the_left_indent() {
    let body = r#"<w:tbl><w:tblGrid><w:gridCol w:w="6000"/></w:tblGrid><w:tr><w:tc><w:p><w:pPr><w:ind w:left="1440" w:hanging="720"/></w:pPr><w:r><w:br w:type="page"/><w:t>Text</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p/>"#;
    let x = |mode: u32| {
        let c = format!(r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="{mode}"/></w:compat>"#);
        lines(&c, ARIAL_12, body).map(|l| l[0].0)
    };
    let Some(c12) = x(12) else { return };
    // Compat ≤ 14 starts the text again with its hanging line; compat 15 goes on at the left indent.
    assert!((x(15).unwrap() - c12 - 36.0).abs() < 0.01, "{c12}");
}

#[test]
fn spaces_next_to_a_no_break_space_dont_break() {
    let fill = "word ".repeat(14);
    let p = |gap: &str| format!("<w:p><w:r><w:t xml:space=\"preserve\">{fill}carbon.{gap}Risks and more words after them</w:t></w:r></w:p>");
    let first = |gap: &str| lines("", ARIAL_12, &p(gap)).map(|l| l[0].1.clone());
    let Some(plain) = first(" ") else { return };
    assert!(plain.ends_with("carbon."), "{plain}");
    assert!(!first("\u{a0} ").unwrap().ends_with("carbon.\u{a0}"));
}

#[test]
fn a_runs_rpr_after_its_first_text_formats_the_text_after_it() {
    let width = |run: &str| lines("", ARIAL_12, &format!("<w:p>{run}<w:r><w:t>|</w:t></w:r></w:p>")).map(|l| l[0].2);
    let Some(plain) = width("<w:r><w:t>aaaa</w:t><w:t>aaaa</w:t></w:r>") else { return };
    let bold = width("<w:r><w:t>aaaa</w:t><w:rPr><w:b/></w:rPr><w:t>aaaa</w:t></w:r>").unwrap();
    let trailing = width("<w:r><w:t>aaaa</w:t><w:t>aaaa</w:t><w:rPr><w:b/></w:rPr></w:r>").unwrap();
    assert!(bold > plain + 0.1 && (trailing - plain).abs() < 1e-9, "{plain} {bold} {trailing}");
}

#[test]
fn an_inline_pictures_box_takes_its_effect_extents_and_run_position() {
    let pic = |extra: &str| {
        format!(
            r#"<w:p><w:r><w:rPr><w:position w:val="-6"/></w:rPr><w:drawing><wp:inline><wp:extent cx="254000" cy="254000"/>{extra}<a:graphic><a:graphicData><pic:pic><pic:blipFill/></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p><w:p><w:r><w:t>Next</w:t></w:r></w:p>"#
        )
    };
    let y = |extra: &str| baselines("", ARIAL_12, &pic(extra)).map(|l| l.iter().find(|x| x.1 == "Next").unwrap().3);
    let Some(plain) = y("") else { return };
    let grown = y(r#"<wp:effectExtent l="0" t="25400" r="0" b="25400"/>"#).unwrap();
    assert!((grown - plain - 4.0).abs() < 0.01, "{plain} {grown}");
}

#[test]
fn a_hidden_empty_paragraph_before_a_table_takes_no_line() {
    let tbl = r#"<w:tbl><w:tblGrid><w:gridCol w:w="4000"/></w:tblGrid><w:tr><w:tc><w:p><w:pPr><w:spacing w:after="0"/></w:pPr><w:r><w:t>Cell</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p/>"#;
    let body = |hidden: &str| {
        format!(
            r#"<w:p><w:pPr><w:spacing w:after="0"/></w:pPr><w:r><w:t>A</w:t></w:r></w:p><w:p><w:pPr><w:spacing w:after="0"/>{hidden}</w:pPr></w:p>{tbl}"#
        )
    };
    let gap = |hidden: &str| baselines("", ARIAL_12, &body(hidden)).map(|l| l.iter().find(|x| x.1 == "Cell").unwrap().3 - l[0].3);
    let Some(shown) = gap("") else { return };
    let hidden = gap("<w:rPr><w:vanish/></w:rPr>").unwrap();
    assert!((shown - hidden - 13.8).abs() < 0.01, "{shown} {hidden}");
}

#[test]
fn a_missing_paragraph_default_is_words_own() {
    let p = |t: &str| format!("<w:p><w:r><w:t>{t}</w:t></w:r></w:p>");
    let body = p("A") + &p("B");
    let gap = |styles: &str| {
        let parts = [
            (
                "word/document.xml",
                format!(
                    r#"<w:document xmlns:w="{W}"><w:body>{body}<w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr></w:body></w:document>"#
                ),
            ),
            (
                "word/styles.xml",
                format!(
                    r#"<w:styles xmlns:w="{W}"><w:docDefaults><w:rPrDefault><w:rPr>{ARIAL_12}</w:rPr></w:rPrDefault>{styles}</w:docDefaults></w:styles>"#
                ),
            ),
        ];
        let parts: Vec<(String, Vec<u8>)> = parts.into_iter().map(|(n, x)| (n.to_string(), x.into_bytes())).collect();
        let doc = crate::docx::read(&repackage(&blank_docx(), &parts, zip::CompressionMethod::Deflated).expect("repackages")).expect("reads");
        let snap = layout(&doc, &fonts());
        let mut ys = Vec::new();
        paint(&snap.pages()[0], &mut |q| {
            if let Item::Glyphs(r) = q.absolute() {
                ys.push(r.baseline);
            }
        });
        ys[1] - ys[0]
    };
    if fonts().face("Arial", None, false, false).is_none() {
        return;
    }
    assert!((gap("<w:pPrDefault/>") - 13.8).abs() < 0.01);
    assert!(gap("") > 13.8 + 8.0);
}

fn bordered(paras: &[(&str, &str)]) -> Option<Vec<f64>> {
    let body: String = paras.iter().map(|(ppr, t)| format!(r#"<w:p><w:pPr>{ppr}</w:pPr><w:r><w:t>{t}</w:t></w:r></w:p>"#)).collect();
    let l = baselines("", ARIAL_12, &format!(r#"<w:p><w:r><w:t>before</w:t></w:r></w:p>{body}"#))?;
    Some(paras.iter().map(|(_, t)| l.iter().find(|x| x.1 == *t).expect("drawn").3).collect())
}

const BOX: &str = r#"<w:top w:val="single" w:sz="4" w:space="1" w:color="000000"/><w:left w:val="single" w:sz="4" w:space="4" w:color="000000"/><w:bottom w:val="single" w:sz="4" w:space="1" w:color="000000"/><w:right w:val="single" w:sz="4" w:space="4" w:color="000000"/>"#;

fn boxed(extra: &str, ind: &str) -> String {
    format!(r#"<w:pBdr>{BOX}{extra}</w:pBdr>{ind}"#)
}

#[test]
fn paragraphs_group_by_their_four_sides_and_box_edges() {
    let plain = boxed("", "");
    let Some(one) = bordered(&[(&plain, "p1"), (&plain, "p2"), (&plain, "p3")]) else { return };
    let gap = |ys: &[f64]| ys[1] - ys[0];
    let indented = boxed("", r#"<w:ind w:left="720"/>"#);
    let three = bordered(&[(&plain, "p1"), (&indented, "p2"), (&plain, "p3")]).unwrap();
    assert!((gap(&three) - gap(&one) - 3.0).abs() < 0.01, "p1's bottom and p2's top: {one:?} {three:?}");
    let first_line = boxed("", r#"<w:ind w:firstLine="720"/>"#);
    assert!((gap(&bordered(&[(&plain, "p1"), (&first_line, "p2"), (&plain, "p3")]).unwrap()) - gap(&one)).abs() < 0.01);
    let hanging = boxed("", r#"<w:ind w:left="720" w:hanging="720"/>"#);
    assert!((gap(&bordered(&[(&plain, "p1"), (&hanging, "p2"), (&plain, "p3")]).unwrap()) - gap(&one)).abs() < 0.01, "edge 0");
    let between = boxed(r#"<w:between w:val="single" w:sz="4" w:space="1" w:color="000000"/>"#, "");
    assert!(
        (gap(&bordered(&[(&plain, "p1"), (&between, "p2"), (&plain, "p3")]).unwrap()) - gap(&one) - 1.0).abs() < 0.01,
        "between isn't in the key"
    );
}

#[test]
fn a_page_break_before_opens_a_new_box() {
    use crate::layout::line::neighbour_borders;
    let border = crate::docx::props::Border { style: "single".into(), width: 0.5, space: 1.0, shadow: false, color: None };
    let Some(doc) = super::plain_doc(&["x".into()]) else { return };
    let Some(crate::docx::Block::Para(p)) = doc.sections[0].blocks.first() else { return };
    let a = crate::docx::ParaStyle { border_top: Some(border), ..(*p.style).clone() };
    let b = crate::docx::ParaStyle { page_break_before: true, ..a.clone() };
    assert!(neighbour_borders(&a, Some(&a), Some(&a)).0.is_some());
    assert!(neighbour_borders(&b, Some(&a), None).0.is_none() && neighbour_borders(&a, None, Some(&b)).1.is_none());
}

#[test]
fn a_between_border_adds_its_width_and_both_spaces() {
    let x = r#"<w:between w:val="single" w:sz="16" w:space="3" w:color="0000FF"/>"#;
    let (plain, with) = (boxed("", ""), boxed(x, ""));
    let Some(base) = bordered(&[(&plain, "p1"), (&plain, "p2"), (&plain, "p3")]) else { return };
    let e1 = bordered(&[(&with, "p1"), (&with, "p2"), (&with, "p3")]).unwrap();
    assert!((e1[1] - e1[0] - (base[1] - base[0]) - 8.0).abs() < 0.01, "3 + 2 + 3 pt: {e1:?}");
    let e2 = bordered(&[(&plain, "p1"), (&with, "p2"), (&plain, "p3")]).unwrap();
    assert!((e2[1] - e2[0] - (base[1] - base[0]) - 3.0).abs() < 0.01, "the lower's space only: {e2:?}");
    assert!((e2[2] - e2[1] - (base[2] - base[1]) - 5.0).abs() < 0.01, "the upper's line and space: {e2:?}");
}

#[test]
fn a_border_space_keeps_its_low_five_bits_and_a_shadow_adds_a_width() {
    let side = |space: u32, shadow: bool| {
        let s = if shadow { r#" w:shadow="1""# } else { "" };
        format!(r#"<w:pBdr><w:top w:val="single" w:sz="12" w:space="{space}" w:color="000000"{s}/></w:pBdr>"#)
    };
    let Some(eight) = bordered(&[(&side(8, false), "p1")]) else { return };
    assert!((bordered(&[(&side(40, false), "p1")]).unwrap()[0] - eight[0]).abs() < 0.01);
    assert!((bordered(&[(&side(32, false), "p1")]).unwrap()[0] - bordered(&[(&side(0, false), "p1")]).unwrap()[0]).abs() < 0.01);
    assert!((bordered(&[(&side(8, true), "p1")]).unwrap()[0] - eight[0] - 1.5).abs() < 0.01, "a gap of one width above");
}

#[test]
fn a_decimal_tab_ignores_the_documents_decimal_symbol() {
    let p = r#"<w:p><w:pPr><w:tabs><w:tab w:val="decimal" w:pos="4320"/></w:tabs></w:pPr><w:r><w:tab/><w:t>1.250,00</w:t></w:r></w:p>"#;
    let x = |symbol: &str| lines(&format!(r#"<w:decimalSymbol w:val="{symbol}"/>"#), ARIAL_12, p).map(|l| l[0].0);
    let Some(point) = x(".") else { return };
    assert_eq!(x(","), Some(point));
}

fn web_lines(divs: &str, body: &str) -> Option<Vec<(f64, String, f64, f64)>> {
    let fonts = fonts();
    fonts.face("Arial", None, false, false)?;
    let parts = [
        (
            "word/document.xml",
            format!(r#"<w:document xmlns:w="{W}"><w:body>{body}<w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr></w:body></w:document>"#),
        ),
        ("word/styles.xml", format!(r#"<w:styles xmlns:w="{W}"><w:docDefaults><w:rPrDefault><w:rPr>{ARIAL_12}</w:rPr></w:rPrDefault><w:pPrDefault/></w:docDefaults></w:styles>"#)),
        ("word/webSettings.xml", format!(r#"<w:webSettings xmlns:w="{W}"><w:divs>{divs}</w:divs></w:webSettings>"#)),
        (
            "word/_rels/document.xml.rels",
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/webSettings" Target="webSettings.xml"/></Relationships>"#.to_string(),
        ),
    ];
    let parts: Vec<(String, Vec<u8>)> = parts.into_iter().map(|(n, x)| (n.to_string(), x.into_bytes())).collect();
    let doc = crate::docx::read(&repackage(&blank_docx(), &parts, zip::CompressionMethod::Deflated).expect("repackages")).expect("reads");
    let snap = layout(&doc, &fonts);
    let mut out: Vec<(f64, String, f64, f64)> = Vec::new();
    paint(&snap.pages()[0], &mut |p| {
        if let Item::Glyphs(r) = p.absolute() {
            out.push((r.x, r.chars.iter().map(|c| c.0).collect::<String>().trim_end().to_string(), r.x, r.baseline));
        }
    });
    Some(out)
}

fn div(id: u32, inner: &str, extra: &str) -> String {
    format!(
        r#"<w:div w:id="{id}"><w:marLeft w:val="{}"/><w:marRight w:val="0"/><w:marTop w:val="120"/><w:marBottom w:val="0"/>{extra}{inner}</w:div>"#,
        720 * id
    )
}

const DIV_BOX: &str = r#"<w:divBdr><w:top w:val="single" w:sz="12" w:space="6" w:color="000000"/><w:bottom w:val="single" w:sz="12" w:space="6" w:color="000000"/></w:divBdr>"#;

#[test]
fn divisions_add_their_margins_and_nest_their_boxes() {
    let para = |id: u32, t: &str| format!(r#"<w:p><w:pPr><w:divId w:val="{id}"/></w:pPr><w:r><w:t>{t}</w:t></w:r></w:p>"#);
    let body = format!("{}{}<w:p><w:r><w:t>after</w:t></w:r></w:p>", para(1, "outer"), para(2, "inner"));
    let tree = |inner_box: &str| div(1, &format!("<w:divsChild>{}</w:divsChild>", div(2, "", inner_box)), DIV_BOX);
    let Some(boxed) = web_lines(&tree(DIV_BOX), &body) else { return };
    let plain = web_lines(&tree(""), &body).unwrap();
    let at = |l: &[(f64, String, f64, f64)], t: &str| l.iter().find(|x| x.1 == t).map(|x| (x.0, x.3)).expect("drawn");
    assert_eq!((at(&boxed, "outer").0, at(&boxed, "inner").0), (108.0, 180.0), "the margins of every division add");
    let gap = |l: &[(f64, String, f64, f64)]| at(l, "inner").1 - at(l, "outer").1;
    assert!((gap(&boxed) - gap(&plain) - 7.5).abs() < 0.01, "the inner box opens inside the outer one: {}", gap(&boxed) - gap(&plain));
    let tail = |l: &[(f64, String, f64, f64)]| at(l, "after").1 - at(l, "inner").1;
    assert!((tail(&boxed) - tail(&plain) - 7.5).abs() < 0.01, "and closes before it");
}

#[test]
fn a_row_in_a_division_moves_by_its_margins() {
    let table = |row_div: &str| {
        format!(
            r#"<w:tbl><w:tblPr><w:tblW w:w="3000" w:type="dxa"/><w:tblLayout w:type="fixed"/></w:tblPr><w:tblGrid><w:gridCol w:w="3000"/></w:tblGrid><w:tr><w:trPr>{row_div}</w:trPr><w:tc><w:p><w:r><w:t>cell</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p><w:r><w:t>after</w:t></w:r></w:p>"#
        )
    };
    let divs = div(1, "", DIV_BOX);
    let Some(inside) = web_lines(&divs, &table(r#"<w:divId w:val="1"/>"#)) else { return };
    let outside = web_lines(&divs, &table("")).unwrap();
    let at = |l: &[(f64, String, f64, f64)], t: &str| l.iter().find(|x| x.1 == t).map(|x| (x.0, x.3)).expect("drawn");
    assert!((at(&inside, "cell").0 - at(&outside, "cell").0 - 36.0).abs() < 0.01);
    assert!((at(&inside, "cell").1 - at(&outside, "cell").1 - 13.5).abs() < 0.01, "margin, border and space above");
}

#[test]
fn a_cell_paragraph_takes_only_its_own_divisions_margins() {
    // Division 2 (72pt) inside division 1 (36pt): in the body both add; in a cell only division 2.
    let para = r#"<w:p><w:pPr><w:divId w:val="2"/></w:pPr><w:r><w:t>text</w:t></w:r></w:p>"#;
    let table = format!(r#"<w:tbl><w:tblGrid><w:gridCol w:w="6000"/></w:tblGrid><w:tr><w:tc>{}</w:tc></w:tr></w:tbl><w:p/>"#, para.replace("text", "cell"));
    let divs = div(1, &format!("<w:divsChild>{}</w:divsChild>", div(2, "", "")), "");
    let Some(l) = web_lines(&divs, &format!("{para}{table}")) else { return };
    let bare = web_lines("", &format!("{para}{table}")).unwrap();
    let at = |l: &[(f64, String, f64, f64)], t: &str| l.iter().find(|x| x.1 == t).map(|x| x.0).expect("drawn");
    assert!((at(&l, "text") - at(&bare, "text") - 108.0).abs() < 0.01);
    assert!((at(&l, "cell") - at(&bare, "cell") - 72.0).abs() < 0.01, "{l:?}");
}

#[test]
fn a_direct_num_id_0_zeroes_the_style_chains_left_and_first_line_indents() {
    let styles = format!(
        r#"<w:styles xmlns:w="{W}"><w:style w:type="paragraph" w:styleId="Sub"><w:pPr><w:ind w:left="720" w:right="360" w:firstLine="360"/></w:pPr></w:style></w:styles>"#
    );
    let p = |direct: &str| format!(r#"<w:p><w:pPr><w:pStyle w:val="Sub"/>{direct}</w:pPr><w:r><w:t>x</w:t></w:r></w:p>"#);
    let none = r#"<w:numPr><w:ilvl w:val="0"/><w:numId w:val="0"/></w:numPr>"#;
    let body = format!(
        r#"<w:document xmlns:w="{W}"><w:body>{}{}{}</w:body></w:document>"#,
        p(""),
        p(none),
        p(&format!(r#"{none}<w:ind w:hanging="360"/>"#))
    );
    let rels = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"#;
    let Some(doc) = super::package(&[
        (
            "_rels/.rels",
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#,
        ),
        ("word/_rels/document.xml.rels", rels),
        ("word/document.xml", &body),
        ("word/styles.xml", &styles),
    ]) else {
        return;
    };
    let ind: Vec<(f64, f64, f64)> = doc.sections[0]
        .blocks
        .iter()
        .filter_map(|b| if let crate::docx::Block::Para(p) = b { Some((p.style.ind_left, p.style.ind_first, p.style.ind_right)) } else { None })
        .collect();
    assert_eq!(ind, [(36.0, 18.0, 18.0), (0.0, 0.0, 18.0), (0.0, -18.0, 18.0)]);
}

fn three_rows(row2: &str, settings: &str) -> Option<Vec<(f64, String, f64, f64)>> {
    let cell = |t: &str| format!(r#"<w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>{t}</w:t></w:r></w:p></w:tc>"#);
    let body = format!(
        r#"<w:tbl><w:tblPr><w:tblW w:w="6000" w:type="dxa"/><w:tblLayout w:type="fixed"/><w:tblBorders><w:insideH w:val="single" w:sz="4" w:space="0" w:color="000000"/></w:tblBorders></w:tblPr><w:tblGrid><w:gridCol w:w="3000"/><w:gridCol w:w="3000"/></w:tblGrid><w:tr>{}{}</w:tr>{row2}<w:tr>{}{}</w:tr></w:tbl><w:p/>"#,
        cell("one"),
        cell("uno"),
        cell("three"),
        cell("tres")
    );
    baselines(settings, ARIAL_12, &body)
}

fn at(l: &[(f64, String, f64, f64)], t: &str) -> (f64, f64) {
    l.iter().find(|x| x.1.starts_with(t)).map(|x| (x.0, x.3)).expect("drawn")
}

const COMPAT_15: &str =
    r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat>"#;

#[test]
fn a_row_whose_every_character_is_hidden_takes_no_room() {
    let vanished = |t: &str| {
        format!(
            r#"<w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/></w:tcPr><w:p><w:pPr><w:rPr><w:vanish/></w:rPr></w:pPr><w:r><w:rPr><w:vanish/></w:rPr><w:t>{t}</w:t></w:r></w:p></w:tc>"#
        )
    };
    let row = |pr: &str| format!("<w:tr>{pr}{}{}</w:tr>", vanished("two"), vanished("dos"));
    let Some(none) = three_rows("", "") else { return };
    let gap = |l: &[(f64, String, f64, f64)]| at(l, "three").1 - at(l, "one").1;
    let hidden = three_rows(&row(r#"<w:trPr><w:hidden/></w:trPr>"#), "").unwrap();
    assert!((gap(&hidden) - gap(&none)).abs() < 0.01, "skipped: {} {}", gap(&hidden), gap(&none));
    let kept = three_rows(&row(""), "").unwrap();
    assert!(gap(&kept) > gap(&none) + 10.0, "the row mark keeps it");
}

#[test]
fn an_empty_hide_mark_cell_takes_no_line() {
    let empty = |hide: &str| format!(r#"<w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/>{hide}</w:tcPr><w:p/></w:tc>"#);
    let row = |hide: &str| format!("<w:tr>{}{}</w:tr>", empty(hide), empty(hide));
    let Some(none) = three_rows("", "") else { return };
    let gap = |l: &[(f64, String, f64, f64)]| at(l, "three").1 - at(l, "one").1;
    let hidden = three_rows(&row("<w:hideMark/>"), "").unwrap();
    let plain = three_rows(&row(""), "").unwrap();
    assert!((gap(&hidden) - gap(&none) - 0.5).abs() < 0.01, "only its border band: {}", gap(&hidden) - gap(&none));
    assert!(gap(&plain) > gap(&hidden) + 10.0);
}

#[test]
fn a_row_exception_aligns_its_row_in_compat_14_only() {
    let row = r#"<w:tr><w:tblPrEx><w:jc w:val="center"/></w:tblPrEx><w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>two</w:t></w:r></w:p></w:tc><w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>dos</w:t></w:r></w:p></w:tc></w:tr>"#;
    let Some(c12) = three_rows(row, "") else { return };
    assert!(at(&c12, "two").0 > at(&c12, "one").0 + 50.0, "centred: {:?} {:?}", at(&c12, "two"), at(&c12, "one"));
    let c15 = three_rows(row, COMPAT_15).unwrap();
    assert!((at(&c15, "two").0 - at(&c15, "one").0).abs() < 0.01, "the first row's alignment");
}

#[test]
fn a_rows_own_cell_spacing_spaces_that_row() {
    let row = r#"<w:tr><w:trPr><w:tblCellSpacing w:w="60" w:type="dxa"/></w:trPr><w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>two</w:t></w:r></w:p></w:tc><w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>dos</w:t></w:r></w:p></w:tc></w:tr>"#;
    let Some(spaced) = three_rows(row, "") else { return };
    let dx = at(&spaced, "two").0 - at(&spaced, "one").0;
    assert!(dx > 5.0 && dx < 8.0, "the row's cells sit inside their spacing: {dx}");
    let ex = row.replace("<w:trPr>", "<w:tblPrEx>").replace("</w:trPr>", "</w:tblPrEx>");
    let excepted = three_rows(&ex, "").unwrap();
    assert!((at(&excepted, "two").0 - at(&spaced, "two").0).abs() < 0.01, "an exception's spacing is the row's");
}

#[test]
fn a_row_whose_exception_fixes_its_layout_is_a_table_of_its_own() {
    let cell = |t: &str, w: &str| format!(r#"<w:tc><w:tcPr><w:tcW {w}/></w:tcPr><w:p><w:r><w:t>{t}</w:t></w:r></w:p></w:tc>"#);
    let auto = r#"w:w="0" w:type="auto""#;
    let body = format!(
        r#"<w:tbl><w:tblPr><w:tblW w:w="0" w:type="auto"/></w:tblPr><w:tblGrid><w:gridCol w:w="3000"/><w:gridCol w:w="3000"/></w:tblGrid><w:tr>{}{}</w:tr><w:tr><w:tblPrEx><w:tblW w:w="4000" w:type="dxa"/><w:tblLayout w:type="fixed"/></w:tblPrEx>{}{}</w:tr></w:tbl><w:p/>"#,
        cell("one", auto),
        cell("uno", auto),
        cell("two", r#"w:w="2000" w:type="dxa""#),
        cell("dos", r#"w:w="2000" w:type="dxa""#)
    );
    let Some(l) = baselines("", ARIAL_12, &body) else { return };
    let col2 = |t: &str| at(&l, t).0 - at(&l, "one").0;
    assert!((col2("dos") - 100.0).abs() < 0.01, "its own fixed grid: {}", col2("dos"));
    assert!(col2("uno") < 60.0, "the autofit row keeps its narrow grid: {}", col2("uno"));
}

#[test]
fn a_layout_run_merges_the_grid_columns_its_rows_have_no_edge_between() {
    let cell = |t: &str, w: i64, span: i64| {
        format!(r#"<w:tc><w:tcPr><w:tcW w:w="{w}" w:type="dxa"/><w:gridSpan w:val="{span}"/></w:tcPr><w:p><w:r><w:t>{t}</w:t></w:r></w:p></w:tc>"#)
    };
    let body = format!(
        r#"<w:tbl><w:tblPr><w:tblW w:w="0" w:type="auto"/></w:tblPr><w:tblGrid><w:gridCol w:w="1000"/><w:gridCol w:w="1000"/><w:gridCol w:w="1000"/></w:tblGrid><w:tr>{}</w:tr><w:tr>{}{}</w:tr><w:tr><w:tblPrEx><w:tblLayout w:type="fixed"/></w:tblPrEx>{}{}</w:tr></w:tbl><w:p/>"#,
        cell("all", 6000, 3),
        cell("one", 2000, 1),
        cell("uno", 4000, 2),
        cell("two", 2000, 2),
        cell("dos", 1000, 1)
    );
    let Some(l) = baselines("", ARIAL_12, &body) else { return };
    let dx = at(&l, "uno").0 - at(&l, "one").0;
    assert!((dx - 100.0).abs() < 0.01, "the first cell keeps its width: {dx}");
}

#[test]
fn a_first_row_whose_exception_fixes_its_layout_is_a_table_of_its_own() {
    let cell = |t: &str, w: &str| format!(r#"<w:tc><w:tcPr><w:tcW {w}/></w:tcPr><w:p><w:r><w:t>{t}</w:t></w:r></w:p></w:tc>"#);
    let auto = r#"w:w="0" w:type="auto""#;
    let table = |layout: &str| {
        format!(
            r#"<w:tbl><w:tblPr><w:tblW w:w="0" w:type="auto"/></w:tblPr><w:tblGrid><w:gridCol w:w="3000"/><w:gridCol w:w="3000"/></w:tblGrid><w:tr><w:tblPrEx><w:tblW w:w="4000" w:type="dxa"/>{layout}</w:tblPrEx>{}{}</w:tr><w:tr>{}{}</w:tr></w:tbl><w:p/>"#,
            cell("one", r#"w:w="2000" w:type="dxa""#),
            cell("uno", r#"w:w="2000" w:type="dxa""#),
            cell("two", auto),
            cell("dos", auto)
        )
    };
    let Some(l) = baselines("", ARIAL_12, &table(r#"<w:tblLayout w:type="fixed"/>"#)) else { return };
    let col2 = |l: &[(f64, String, f64, f64)], t: &str, first: &str| at(l, t).0 - at(l, first).0;
    assert!((col2(&l, "uno", "one") - 100.0).abs() < 0.01, "its own fixed grid: {}", col2(&l, "uno", "one"));
    assert!(col2(&l, "dos", "two") < 60.0, "the rows after it keep their own width: {}", col2(&l, "dos", "two"));
    let one = baselines("", ARIAL_12, &table("")).unwrap();
    assert!((col2(&one, "dos", "two") - col2(&one, "uno", "one")).abs() < 0.01, "without fixed the exception's width is the table's");
}

#[test]
fn a_spaced_row_that_starts_a_page_takes_the_tables_top_border() {
    let border = r#"w:val="single" w:sz="4" w:space="0" w:color="000000""#;
    let cell = |t: &str, ppr: &str| {
        format!(r#"<w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/></w:tcPr><w:p><w:pPr>{ppr}</w:pPr><w:r><w:t>{t}</w:t></w:r></w:p></w:tc>"#)
    };
    let body = format!(
        r#"<w:tbl><w:tblPr><w:tblCellSpacing w:w="60" w:type="dxa"/><w:tblBorders><w:top {border}/><w:left {border}/><w:bottom {border}/><w:right {border}/><w:insideH {border}/><w:insideV {border}/></w:tblBorders><w:tblLayout w:type="fixed"/><w:tblCellMar><w:top w:w="0" w:type="dxa"/><w:bottom w:w="0" w:type="dxa"/></w:tblCellMar></w:tblPr><w:tblGrid><w:gridCol w:w="3000"/></w:tblGrid><w:tr>{}</w:tr><w:tr>{}</w:tr></w:tbl><w:p/>"#,
        cell("one", ""),
        cell("two", "<w:pageBreakBefore/>")
    );
    let Some(first) = page_baselines("", ARIAL_12, &body, 0) else { return };
    let second = page_baselines("", ARIAL_12, &body, 1).unwrap();
    let dy = at(&second, "two").1 - at(&first, "one").1;
    assert!((dy + 3.0).abs() < 0.01, "the table's top border and one spacing above the row: {dy}");
}

fn square(x: i64, y: i64, w: i64, h: i64, id: u32) -> String {
    let e = |v: i64| v * 12700;
    format!(
        r#"<w:r><w:drawing><wp:anchor distT="0" distB="0" distL="0" distR="0" simplePos="0" relativeHeight="{id}" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="page"><wp:posOffset>{}</wp:posOffset></wp:positionH><wp:positionV relativeFrom="page"><wp:posOffset>{}</wp:posOffset></wp:positionV><wp:extent cx="{}" cy="{}"/><wp:wrapSquare wrapText="bothSides"/><wp:docPr id="{id}" name="F{id}"/><a:graphic><a:graphicData><pic:pic><pic:blipFill/></pic:pic></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r>"#,
        e(x),
        e(y),
        e(w),
        e(h)
    )
}

fn after_clear(compat: u32, floats: &str, before: &str, clear: &str) -> Option<(f64, f64, f64)> {
    let settings = format!(
        r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="{compat}"/></w:compat>"#
    );
    let body = format!(r#"<w:p>{floats}<w:r><w:t xml:space="preserve">{before}</w:t><w:br w:clear="{clear}"/><w:t>bbb</w:t></w:r></w:p>"#);
    let l = baselines(&settings, ARIAL_12, &body)?;
    let a = l.iter().find(|l| l.1.starts_with('a'))?;
    let b = l.iter().find(|l| l.1.contains("bbb"))?;
    Some((a.3, b.3, b.0))
}

#[test]
fn a_text_wrapping_break_moves_its_text_below_the_floats_it_clears() {
    let sides = format!("{}{}", square(72, 72, 100, 100, 1), square(440, 72, 100, 60, 2));
    for compat in [12, 15] {
        let Some((_, left, _)) = after_clear(compat, &sides, "aaa", "left") else { return };
        let (_, right, x) = after_clear(compat, &sides, "aaa", "right").expect("lays out");
        let (_, all, _) = after_clear(compat, &sides, "aaa", "all").expect("lays out");
        assert!(left > 172.0 && left < 186.0, "compat {compat}: left {left}");
        assert!(right > 132.0 && right < 146.0 && x >= 172.0, "compat {compat}: right {right} at {x}");
        assert!(all > 172.0 && all < 186.0, "compat {compat}: all {all}");
    }
}

#[test]
fn compat_12_a_float_starting_in_a_band_inside_the_cut_part_doesnt_move_it() {
    // A float right below another, in the part of the line the upper one cuts away, wraps as one tall float.
    let text = "The quick brown fox jumps over the lazy dog near the riverbank. ".repeat(12);
    let lines = |floats: &str| baselines("", ARIAL_12, &format!(r#"<w:p>{floats}<w:r><w:t>{text}</w:t></w:r></w:p>"#));
    let Some(tall) = lines(&square(120, 72, 50, 201, 1)) else { return };
    let stacked = lines(&format!("{}{}", square(120, 72, 50, 101, 1), square(120, 173, 50, 100, 2))).expect("lays out");
    assert_eq!(stacked, tall);
}

#[test]
fn a_text_wrapping_break_beside_a_middle_float_goes_on_when_its_side_is_clear() {
    let middle = square(222, 72, 100, 100, 1);
    for compat in [12, 15] {
        let Some((a, b, x)) = after_clear(compat, &middle, "aaa ", "left") else { return };
        assert!((a - b).abs() < 0.01 && x >= 322.0, "compat {compat}: same line, right piece: {a} {b} {x}");
        let (_, below, _) = after_clear(compat, &middle, "aaa ", "right").expect("lays out");
        assert!(below > 172.0, "compat {compat}: {below}");
    }
}

#[test]
fn mirrored_indents_swap_on_mirrored_pages() {
    use crate::docx::Block;
    let Some(mut doc) = super::plain_doc(&["Aplain".into(), "Bmirror".into(), "Cplain".into(), "Dmirror".into()]) else { return };
    for (i, b) in doc.sections[0].blocks.iter_mut().enumerate() {
        let Block::Para(p) = b else { unreachable!() };
        let s = std::rc::Rc::make_mut(&mut p.style);
        s.page_break_before = i == 2;
        if i % 2 == 1 {
            (s.ind_left, s.ind_right, s.ind_first, s.mirror_indents) = (72.0, 36.0, 18.0, true);
        }
    }
    let pages = super::page_chars(&doc);
    let indent = |page: usize, plain: char, mirrored: char| {
        let x = |c: char| pages[page].iter().find(|g| g.0 == c).map(|g| g.1).expect("on the page");
        x(mirrored) - x(plain)
    };
    let (p1, p2) = (indent(0, 'A', 'B'), indent(1, 'C', 'D'));
    // Mirrored, the first line starts at the right indent plus the first-line indent.
    let even_mirrors = doc.settings.compat() == crate::docx::Compat::Generic;
    let (want1, want2) = if even_mirrors { (90.0, 54.0) } else { (54.0, 90.0) };
    assert!((p1 - want1).abs() < 0.01 && (p2 - want2).abs() < 0.01, "{p1} {p2}");
}

#[test]
fn book_fold_drops_or_mirrors_the_gutter() {
    use crate::docx::Block;
    let Some(mut doc) = super::plain_doc(&["Afirst".into(), "Bsecond".into()]) else { return };
    let Block::Para(p) = &mut doc.sections[0].blocks[1] else { unreachable!() };
    std::rc::Rc::make_mut(&mut p.style).page_break_before = true;
    let s = &mut doc.sections[0];
    (s.left, s.right, s.gutter) = (108.0, 72.0, 18.0);
    doc.settings.book_fold = Some(false);
    let starts = |doc: &crate::docx::Document| {
        let pages = super::page_chars(doc);
        [pages[0][0].1, pages[1][0].1]
    };
    assert_eq!(starts(&doc), [108.0, 108.0], "portrait: no gutter");
    let s = &mut doc.sections[0];
    (s.page_w, s.page_h) = (s.page_h, s.page_w);
    assert_eq!(starts(&doc), [90.0, 108.0], "landscape: odd pages mirrored, with the gutter");
    doc.settings.book_fold = Some(true);
    assert_eq!(starts(&doc), [108.0, 90.0], "reversed");
}

const ARIAL_11: &str = r#"<w:rFonts w:ascii="Arial" w:hAnsi="Arial"/><w:sz w:val="22"/>"#;

fn ruby(align: &str, raise: u32, annotation: &str, base: &str) -> String {
    format!(
        r#"<w:p><w:r><w:t>Start|</w:t></w:r><w:r><w:ruby><w:rubyPr><w:rubyAlign w:val="{align}"/><w:hps w:val="11"/><w:hpsRaise w:val="{raise}"/><w:hpsBaseText w:val="22"/><w:lid w:val="en-US"/></w:rubyPr><w:rt><w:r><w:rPr><w:sz w:val="11"/></w:rPr><w:t xml:space="preserve">{annotation}</w:t></w:r></w:rt><w:rubyBase><w:r><w:t>{base}</w:t></w:r></w:rubyBase></w:ruby></w:r><w:r><w:t>|end</w:t></w:r></w:p>"#
    )
}

#[test]
fn a_ruby_is_as_wide_as_the_wider_of_its_base_and_annotation() {
    let end = |align: &str, annotation: &str| baselines("", ARIAL_11, &ruby(align, 20, annotation, "MM")).map(|l| l[0].2);
    let (Some(wide), Some(narrow)) = (end("center", "annotation text"), end("center", "ab")) else { return };
    // 721.7 tw of annotation against the 366.6 tw base.
    assert!((wide - narrow - 17.755).abs() < 0.05, "{wide} {narrow}");
}

#[test]
fn a_rubys_raised_annotation_adds_its_ascent_to_the_line() {
    let base = |raise: u32| baselines("", ARIAL_11, &ruby("center", raise, "annotation text", "MM")).map(|l| l[0].3);
    let (Some(flat), Some(raised), Some(low)) = (base(0), base(40), base(10)) else { return };
    // 20pt raise + the 5.5pt annotation's ascent over the 11pt base's.
    assert!((raised - flat - 14.8411).abs() < 0.01, "{flat} {raised}");
    assert!((low - flat).abs() < 1e-9, "a raise the base's ascent covers changes nothing");
}

#[test]
fn a_right_vertical_ruby_turns_its_annotation_beside_the_base() {
    let line = |align: &str, annotation: &str| baselines("", ARIAL_11, &ruby(align, 0, annotation, "M")).map(|l| (l[0].2, l[0].3));
    let (Some((centre, flat)), Some((turned, short))) = (line("center", "ab"), line("rightVertical", "ab")) else { return };
    // M 4506 lu narrower (the 11pt ruby run's 22 half-points), then the 5.5pt annotation's centred vertical
    // height, 2419 font units.
    assert!((turned - centre - (2419.0 * 5.5 / 2048.0 - 4506.0 / 4096.0)).abs() < 0.01, "{centre} {turned}");
    assert!((short - flat).abs() < 1e-9, "a short annotation leaves the line");
    // A 20-character annotation (52pt long): the line's ascent is half of it.
    let (_, tall) = line("rightVertical", "abcdefghijklmnopqrst").expect("lays out");
    assert!(tall - flat > 14.0, "{flat} {tall}");
}

#[test]
fn a_right_vertical_ruby_narrows_its_base_by_the_ruby_runs_size_and_adds_its_spacing() {
    let end = |outer: &str, base_rpr: &str| {
        let body = ruby("rightVertical", 0, "a", "MMMM")
            .replace("<w:r><w:ruby>", &format!("<w:r><w:rPr>{outer}</w:rPr><w:ruby>"))
            .replace("<w:rubyBase><w:r>", &format!("<w:rubyBase><w:r><w:rPr>{base_rpr}</w:rPr>"));
        baselines("", ARIAL_11, &body).map(|l| l[0].2)
    };
    let Some(plain) = end("", "") else { return };
    // 16pt: each of the four characters 2048 lu narrower than at 11pt; 2pt of spacing after the annotation; the
    // base's own spacing ignored.
    assert!((plain - end(r#"<w:sz w:val="32"/>"#, "").unwrap() - 2.0).abs() < 1e-6);
    assert!((end(r#"<w:spacing w:val="40"/>"#, "").unwrap() - plain - 2.0).abs() < 1e-6);
    assert!((end("", r#"<w:spacing w:val="40"/>"#).unwrap() - plain).abs() < 1e-6);
}

#[test]
fn a_right_vertical_rubys_annotation_is_one_font_at_the_rubys_size() {
    let end = |body: String| baselines("", ARIAL_11, &body).map(|l| l[0].2);
    let Some(plain) = end(ruby("rightVertical", 0, "abcd", "M")) else { return };
    // Runs of their own sizes, a bold one, a later one in Times: all Arial at the ruby's 5.5pt.
    let runs = ruby("rightVertical", 0, "ab", "M").replace(
        r#"<w:rPr><w:sz w:val="11"/></w:rPr><w:t xml:space="preserve">ab</w:t></w:r>"#,
        r#"<w:rPr><w:b/><w:sz w:val="32"/></w:rPr><w:t xml:space="preserve">ab</w:t></w:r><w:r><w:rPr><w:rFonts w:ascii="Times New Roman" w:hAnsi="Times New Roman"/><w:sz w:val="64"/></w:rPr><w:t xml:space="preserve">cd</w:t></w:r>"#,
    );
    assert!((end(runs).unwrap() - plain).abs() < 1e-6);
}

#[test]
fn an_at_least_line_keeps_half_a_turned_annotation_below_its_baseline() {
    let y = |rule: &str| {
        let body = ruby("rightVertical", 0, "abcde", "M")
            .replace("<w:p>", &format!("<w:p><w:pPr><w:spacing w:line=\"400\" w:lineRule=\"{rule}\"/></w:pPr>"));
        baselines("", ARIAL_11, &body).map(|l| l[0].3)
    };
    let Some(at_least) = y("atLeast") else { return };
    // The 5.5pt annotation is 5580 font units long: half of it stays below the baseline of the 20pt line.
    assert!((at_least - 72.0 - (20.0 - 5580.0 * 5.5 / 2048.0 / 2.0)).abs() < 0.05, "{at_least}");
}

#[test]
fn a_right_vertical_rubys_base_narrows_to_no_advance_at_most() {
    let end = |base: &str| {
        let body = ruby("rightVertical", 0, "a", base).replace("<w:r><w:ruby>", r#"<w:r><w:rPr><w:sz w:val="72"/></w:rPr><w:ruby>"#);
        baselines("", ARIAL_11, &body).map(|l| l[0].2)
    };
    let Some(one) = end("i") else { return };
    // 36pt narrows each 2.4pt `i` by 3.6pt: four of them take no more room than one.
    assert!((end("iiii").unwrap() - one).abs() < 1e-6);
}

#[test]
fn a_right_vertical_rubys_kana_stand_upright() {
    if !fonts().is_installed("MS Mincho") {
        return;
    }
    let body = ruby("rightVertical", 0, "あい", "MM")
        .replace("<w:rPr><w:sz w:val=\"11\"/>", r#"<w:rPr><w:rFonts w:eastAsia="MS Mincho"/><w:sz w:val="11"/>"#);
    let Some(doc) = test_doc("", ARIAL_11, &body, "") else { return };
    let items = crate::layout::traverse::paint_items(&layout(&doc, &fonts()).pages()[0]);
    let turned = items.iter().find_map(|i| if let Item::Turned(t) = i { Some(t) } else { None }).expect("a turned annotation");
    assert_eq!(turned.items.iter().filter(|i| matches!(i, Item::Turned(u) if u.deg == -90.0)).count(), 2);
}

fn legacy_level(space: u32, indent: u32, jc: &str, suff: &str) -> String {
    format!(
        r#"<w:abstractNum w:abstractNumId="1"><w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="decimal"/><w:suff w:val="{suff}"/><w:lvlText w:val="%1."/><w:legacy w:legacy="1" w:legacySpace="{space}" w:legacyIndent="{indent}"/><w:lvlJc w:val="{jc}"/><w:pPr><w:ind w:left="720" w:hanging="360"/></w:pPr></w:lvl></w:abstractNum><w:num w:numId="1"><w:abstractNumId w:val="1"/></w:num>"#
    )
}

#[test]
fn a_legacy_list_level_puts_its_text_a_fixed_gap_after_the_number() {
    let body = r#"<w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr></w:pPr><w:r><w:t>Text</w:t></w:r></w:p>"#;
    let at = |space, indent, jc, suff| numbered_baselines("", ARIAL_11, body, &legacy_level(space, indent, jc, suff)).map(|l| (l[0].0, l[0].2));
    let Some((number, text)) = at(120, 720, "left", "tab") else { return };
    assert!((number - 90.0).abs() < 0.01 && (text - 126.0).abs() < 0.01, "{number} {text}");
    // The suffix doesn't count; a gap wider than the indent leaves the number's width plus the gap.
    assert_eq!(at(120, 720, "left", "space"), Some((number, text)));
    let (_, past) = at(360, 360, "left", "tab").expect("lays out");
    assert!((past - (90.0 + 9.17 + 18.0)).abs() < 0.01, "{past}");
    // Right: the number moves right by what the gap leaves; the text stays at the indent.
    let (right, text) = at(120, 360, "right", "tab").expect("lays out");
    assert!((right - 92.83).abs() < 0.01 && (text - 108.0).abs() < 0.01, "{right} {text}");
}

#[test]
fn a_word_too_long_after_a_list_number_breaks_on_the_numbers_line() {
    let level = r#"<w:abstractNum w:abstractNumId="1"><w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="decimal"/><w:suff w:val="tab"/><w:lvlText w:val="%1."/><w:lvlJc w:val="left"/><w:pPr><w:ind w:left="200" w:hanging="600"/></w:pPr></w:lvl></w:abstractNum><w:num w:numId="1"><w:abstractNumId w:val="1"/></w:num>"#;
    let word = "x".repeat(150);
    let body = format!(r#"<w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr></w:pPr><w:r><w:t>{word}</w:t></w:r></w:p>"#);
    let Some(l) = numbered_baselines("", ARIAL_11, &body, level) else { return };
    // The number's tab is no break: the word starts on the number's line, at the left indent.
    assert!(l[0].1.starts_with("1.x"), "{l:?}");
}

#[test]
fn compat_14_justified_centre_tab_text_that_wraps_starts_at_the_tab() {
    let words = "aa bb cc dd ee ff gg hh ";
    let long = "x".repeat(90);
    let body = format!(
        r#"<w:p><w:pPr><w:tabs><w:tab w:val="center" w:pos="4680"/></w:tabs><w:jc w:val="both"/></w:pPr><w:r><w:t>C3</w:t><w:tab/><w:t xml:space="preserve">{words}{long}</w:t></w:r></w:p>"#
    );
    let Some(chars) = glyph_xs("", &body) else { return };
    let x = |c: char| chars.iter().find(|g| g.0 == c).map(|g| g.1).expect("drawn");
    // The tab takes no width: the text starts where "C3" ends and is justified from there.
    assert!(x('a') - x('3') < 8.0, "{} {}", x('3'), x('a'));
}

#[test]
fn a_tab_whose_next_word_does_not_fit_starts_the_next_line() {
    // The tab's default stop is inside the line but "Signature" isn't: both go down, and the tab takes the next
    // stop from the line's start there.
    let body = format!(r#"<w:p><w:r><w:t>{}</w:t><w:tab/><w:t>Signature</w:t></w:r></w:p>"#, "x".repeat(78));
    let Some(chars) = glyph_xs("", &body) else { return };
    let s = chars.iter().find(|g| g.0 == 'S').expect("drawn");
    let x = chars.iter().find(|g| g.0 == 'x').expect("drawn");
    assert!(s.2 > x.2 && (s.1 - 108.0).abs() < 0.01, "{s:?}");
}

#[test]
fn compat_14_text_after_a_left_stop_at_the_right_edge_stays_on_the_line() {
    let body = r#"<w:p><w:pPr><w:tabs><w:tab w:val="left" w:pos="9360"/></w:tabs></w:pPr><w:r><w:t>x</w:t><w:tab/><w:t>Yj</w:t></w:r></w:p>"#;
    let Some(chars) = glyph_xs("", body) else { return };
    let y = chars.iter().find(|g| g.0 == 'Y').expect("drawn");
    assert!((y.1 - 540.0).abs() < 0.01 && (y.2 - chars[0].2).abs() < 0.01, "{y:?}");
}

/// Each character on the first page with its x and baseline.
fn glyph_xs(settings: &str, body: &str) -> Option<Vec<(char, f64, f64)>> {
    Some(super::page_chars(&test_doc(settings, ARIAL_11, body, "")?).swap_remove(0))
}

fn fitted(val: u32, text: &str) -> String {
    format!(
        r#"<w:p><w:r><w:t>|</w:t></w:r><w:r><w:rPr><w:fitText w:val="{val}" w:id="1"/></w:rPr><w:t xml:space="preserve">{text}</w:t></w:r><w:r><w:t>|end</w:t></w:r></w:p>"#
    )
}

#[test]
fn fitted_text_spreads_or_squeezes_to_its_width() {
    let width = |val, text| {
        let c = glyph_xs("", &fitted(val, text))?;
        let bars: Vec<f64> = c.iter().filter(|g| g.0 == '|').map(|g| g.1).collect();
        Some((bars[1] - c[1].1, c))
    };
    // `abc def` (721.6 tw) to 2880: 359 tw after each character, 5 after the last.
    let Some((w, c)) = width(2880, "abc def") else { return };
    assert!((w - 2880.6 / 20.0).abs() < 0.03, "{w}");
    // `a` (1139 fu) then 359 tw.
    assert!((c[2].1 - c[1].1 - (1139.0 * 11.0 / 2048.0 + 17.95)).abs() < 0.01, "{:?}", &c[..3]);
    // Within 10 tw of the target: unchanged.
    let (natural, _) = width(1076, "abcdefghi").expect("lays out");
    assert!((natural - 1076.4 / 20.0).abs() < 0.03, "{natural}");
    // `abcdef` (660.5 tw) to 800: 74 %, the overshoot off the last character.
    let (narrow, _) = width(500, "abcdef").expect("lays out");
    assert!((narrow - 25.0).abs() < 0.6, "{narrow}");
}

#[test]
fn fitted_text_corrections_spread_again_and_meet_the_scale_limit() {
    let width = |val, text| {
        let c = glyph_xs("", &fitted(val, text))?;
        let bars: Vec<f64> = c.iter().filter(|g| g.0 == '|').map(|g| g.1).collect();
        Some(bars[1] - c[1].1)
    };
    // A correction over 40 tw spreads again: 1 tw on all 15 characters, not 0 and 15 on the last.
    let Some(w) = width(1348, "a.b.c.d.e.f.g.h") else { return };
    assert!((w - 1348.1 / 20.0).abs() < 0.03, "{w}");
    // 15 %, then the overshoot off the last character: more than the `j` is wide, so the region ends where the `j`
    // starts, whatever the target.
    let w = width(50, "abcdefghij").expect("lays out");
    assert!((w - 141.5 / 20.0).abs() < 0.1, "{w}");
    assert_eq!(width(100, "abcdefghij"), Some(w));
    // Two characters: 409 tw, then 1 on the last.
    let w = width(654, "ab").expect("lays out");
    assert!((w - 654.7 / 20.0).abs() < 0.03, "{w}");
}

#[test]
fn condensed_spacing_takes_a_character_to_no_width_at_most() {
    let body = r#"<w:p><w:r><w:t>|</w:t></w:r><w:r><w:rPr><w:spacing w:val="-300"/></w:rPr><w:t>WWWW</w:t></w:r><w:r><w:t>|end</w:t></w:r></w:p>"#;
    let Some(c) = glyph_xs("", body) else { return };
    let bars: Vec<f64> = c.iter().filter(|g| g.0 == '|').map(|g| g.1).collect();
    assert!((bars[1] - c[1].1).abs() < 1e-9, "{c:?}");
}

#[test]
fn a_fitted_region_never_breaks() {
    // Wider than the line: on a line of its own (the bar glued to it broken off), past the margin.
    let c = glyph_xs("", &fitted(12000, "abc def ghi")).expect("lays out");
    let a = c.iter().find(|g| g.0 == 'a').expect("a");
    let end = c.iter().rfind(|g| g.0 == '|').expect("|end");
    assert!(c[0].2 < a.2 && a.2 < end.2, "{:?} {a:?} {end:?}", c[0]);
    assert!(c.iter().filter(|g| g.2 == a.2).map(|g| g.1).fold(0.0, f64::max) > 540.0);
}

#[test]
fn compat_15_breaks_a_squeezed_region_at_its_scale_without_the_correction() {
    // `abcdefghij` to 30 tw: 15 %, then a correction past the `j`. The line holds 1020 tw: the word with the drawn
    // region (1019.2) fits, with the region at 15 % alone (1026.5) it doesn't.
    let body = r#"<w:p><w:pPr><w:ind w:right="8340"/></w:pPr><w:r><w:t xml:space="preserve">S00 </w:t></w:r><w:r><w:rPr><w:fitText w:val="30" w:id="1"/></w:rPr><w:t>abcdefghij</w:t></w:r><w:r><w:t>|end</w:t></w:r></w:p>"#;
    let c15 = r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat>"#;
    let (Some(l12), Some(l15)) = (lines("", ARIAL_11, body), lines(c15, ARIAL_11, body)) else { return };
    assert_eq!(l12.len(), 1, "{l12:?}");
    assert_eq!(l15.len(), 2, "{l15:?}");
}

#[test]
fn compat_15_fits_a_region_on_the_documents_first_line_as_compat_12() {
    // `S00 ` + `abcdef` fitted to 2880 + `|end` on a 3860 tw line: it fits drawn, not with the first adjustment's 443 tw
    // on every character (438 tw wider); the document's first line takes the compat 12 width.
    let row = r#"<w:p><w:pPr><w:ind w:right="5500"/></w:pPr><w:r><w:t xml:space="preserve">S00 </w:t></w:r><w:r><w:rPr><w:fitText w:val="2880" w:id="1"/></w:rPr><w:t>abcdef</w:t></w:r><w:r><w:t>|end</w:t></w:r></w:p>"#;
    let c15 = r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat>"#;
    let (Some(first), Some(later)) = (lines(c15, ARIAL_11, row), lines(c15, ARIAL_11, &format!("<w:p><w:r><w:t>Before</w:t></w:r></w:p>{row}"))) else {
        return;
    };
    assert_eq!(first.len(), 1, "{first:?}");
    assert_eq!(later.len(), 3, "{later:?}");
}

#[test]
fn compat_15_first_line_test_advances_the_tab_to_its_stop() {
    // With the tab at its 720 tw stop the unadjusted first line can't hold the region and its `|end` (with the tab
    // as nothing it could): the region takes the state of any later line.
    let row = r#"<w:p><w:r><w:t>W1</w:t></w:r><w:r><w:tab/></w:r><w:r><w:t xml:space="preserve">Alpha bravo charlie delta echo foxtrot golf hotel india juliet kilo lim |</w:t></w:r><w:r><w:rPr><w:fitText w:val="2976" w:id="1"/></w:rPr><w:t xml:space="preserve">abc def ghi jkl mno</w:t></w:r><w:r><w:t>|end</w:t></w:r></w:p>"#;
    let c15 = r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat>"#;
    let region = |body: &str| {
        let c = glyph_xs(c15, body)?;
        let a = c.windows(2).position(|w| (w[0].0, w[1].0) == ('a', 'b'))?;
        Some(c[a..a + 15].iter().map(|g| g.1).collect::<Vec<f64>>())
    };
    let (Some(first), Some(later)) = (region(row), region(&format!("<w:p><w:r><w:t>Before</w:t></w:r></w:p>{row}"))) else { return };
    assert!(first.iter().zip(&later).all(|(a, b)| (a - b).abs() < 0.01), "{first:?} {later:?}");
}

#[test]
fn compat_15_draws_a_fitted_region_from_its_second_adjustment() {
    // `ab` to 1440 tw: compat 12 spreads 1195 tw after the `a`; compat 15 draws 54 % and 1307 tw, the fit procedure
    // from the second adjustment's scale.
    let body = format!("<w:p><w:r><w:t>Before</w:t></w:r></w:p>{}", fitted(1440, "ab"));
    let c15 = r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat>"#;
    let gap = |settings: &str| {
        let c = glyph_xs(settings, &body)?;
        let x = |ch: char| c.iter().find(|g| g.0 == ch).map(|g| g.1);
        Some(x('b')? - x('a')?)
    };
    let (Some(c12), Some(c15)) = (gap(""), gap(c15)) else { return };
    assert!((c12 - (1139.0 * 11.0 / 2048.0 + 59.75)).abs() < 0.05, "{c12}");
    assert!((c15 - (615.0 * 11.0 / 2048.0 + 65.35)).abs() < 0.05, "{c15}");
}

#[test]
fn compat_15_fits_a_region_in_a_table_cell_as_compat_12() {
    let cell = format!(r#"<w:tbl><w:tblGrid><w:gridCol w:w="9000"/></w:tblGrid><w:tr><w:tc>{}</w:tc></w:tr></w:tbl><w:p/>"#, fitted(1440, "ab"));
    let body = format!("<w:p><w:r><w:t>Before</w:t></w:r></w:p>{cell}");
    let c15 = r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat>"#;
    let Some(c) = glyph_xs(c15, &body) else { return };
    let x = |ch: char| c.iter().find(|g| g.0 == ch).map(|g| g.1).expect("drawn");
    assert!((x('b') - x('a') - (1139.0 * 11.0 / 2048.0 + 59.75)).abs() < 0.05, "1195 tw after the `a`: {}", x('b') - x('a'));
}

#[test]
fn a_vanished_mark_joins_its_paragraph_to_the_next() {
    let body = format!(
        r#"<w:p><w:r><w:t>Before</w:t></w:r></w:p><w:p><w:pPr><w:ind w:left="720" w:right="720"/><w:jc w:val="center"/><w:spacing w:before="240" w:after="480"/><w:rPr><w:vanish/><w:specVanish/></w:rPr></w:pPr><w:r><w:t>Heading.</w:t></w:r></w:p><w:p><w:pPr><w:ind w:left="1440" w:firstLine="360"/><w:spacing w:before="360"/></w:pPr><w:r><w:t>{}</w:t></w:r></w:p>"#,
        super::words(60)
    );
    let c15 = r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat>"#;
    let (Some(l12), Some(l15)) = (baselines("", ARIAL_11, &body), baselines(c15, ARIAL_11, &body)) else { return };
    for l in [&l12, &l15] {
        assert!(l[1].1.starts_with("Heading.layout") && l.len() > 3, "one paragraph: {l:?}");
        // The first paragraph's space before, not the second's.
        assert!((l[1].3 - l[0].3 - (12.6499 + 12.0)).abs() < 0.01, "{} {}", l[0].3, l[1].3);
    }
    // Compat 15: the first paragraph's indents and centring on every line; compat 12: the second's left indent (not its
    // first-line indent) on the lines that start in it.
    assert!(l15[2..].iter().all(|l| (l.0 - 144.0).abs() > 0.5) && l15[1].0 > 108.0, "{l15:?}");
    assert!((l12[2].0 - 144.0).abs() < 0.01 && l12[1].0 > 108.0, "{l12:?}");
}

#[test]
fn compat_14_a_line_starting_at_a_vanished_mark_is_the_earlier_paragraphs() {
    let long = "m".repeat(48);
    let body = format!(
        r#"<w:p><w:pPr><w:rPr><w:vanish/><w:specVanish/></w:rPr></w:pPr><w:r><w:t xml:space="preserve">Heading. </w:t></w:r></w:p><w:p><w:pPr><w:ind w:left="1440"/></w:pPr><w:r><w:t>{long} {}</w:t></w:r></w:p>"#,
        super::words(30)
    );
    let c15 = r#"<w:trackRevisions/><w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat>"#;
    let (Some(l12), Some(l15)) = (baselines("", ARIAL_11, &body), baselines(c15, ARIAL_11, &body)) else { return };
    // Line 2 starts with the second paragraph's first word, at the vanished mark: the first paragraph's indent.
    assert!(l12[1].1.starts_with('m') && (l12[1].0 - 72.0).abs() < 0.01 && (l12[2].0 - 144.0).abs() < 0.01, "{l12:?}");
    // Revision tracking alone keeps compat 15's lines on the first paragraph's indent.
    assert!(l15.iter().all(|l| (l.0 - 72.0).abs() < 0.01), "{l15:?}");
}

#[test]
fn opentype_number_features_apply_in_compat_15_or_when_enabled() {
    if fonts().face("Calibri", None, false, false).is_none_or(|f| !f.family.eq_ignore_ascii_case("Calibri")) {
        return;
    }
    let body = r#"<w:p><w:r><w:rPr><w14:numSpacing xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml" w14:val="proportional"/></w:rPr><w:t>0123456789|</w:t></w:r></w:p>"#;
    let bar = |settings: &str| {
        let doc = test_doc(settings, r#"<w:rFonts w:ascii="Calibri" w:hAnsi="Calibri"/><w:sz w:val="22"/>"#, body, "").expect("Arial");
        super::page_chars(&doc)[0].iter().find(|c| c.0 == '|').map(|c| c.1).expect("a bar")
    };
    let setting = |name: &str, v: u32| {
        format!(r#"<w:compat><w:compatSetting w:name="{name}" w:uri="http://schemas.microsoft.com/office/word" w:val="{v}"/></w:compat>"#)
    };
    let (c12, c14, c15) = (bar(""), bar(&setting("compatibilityMode", 14)), bar(&setting("compatibilityMode", 15)));
    let enabled = bar(r#"<w:compat><w:compatSetting w:name="enableOpenTypeFeatures" w:uri="http://schemas.microsoft.com/office/word" w:val="1"/><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="12"/></w:compat>"#);
    // Calibri's proportional digits (1087.3 tw) against its tabular default (1115.0 tw).
    assert!((c12 - c14).abs() < 1e-9 && (c12 - c15 - 27.7 / 20.0).abs() < 0.02, "{c12} {c14} {c15}");
    assert!((enabled - c15).abs() < 1e-9, "{enabled}");
}

#[test]
fn east_asian_breaking_rules_apply_from_compat_14_or_with_apply_breaking_rules() {
    if !fonts().is_installed("MS Mincho") {
        return;
    }
    let rpr = SZ_11;
    let p = format!("<w:p><w:r>{MINCHO_JA}<w:t>{}</w:t></w:r></w:p>", "日本語の文章です。句読点「かぎ括弧」、テスト（括弧）です。".repeat(3));
    let mode = |opt: &str, m: u32| {
        format!(
            r#"<w:compat>{opt}<w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="{m}"/></w:compat>"#
        )
    };
    let ends = |settings: &str| lines(settings, rpr, &p).map(|l| l.iter().map(|x| x.1.chars().last().unwrap_or(' ')).collect::<String>());
    let Some(off) = ends(&mode("", 12)) else { return };
    // Off: a line may end with an opening bracket. On: it moves down, and a closing bracket takes the character
    // before it along.
    assert_eq!(off, "「）。");
    assert_eq!(ends(&mode("", 14)).unwrap(), "点括。");
    assert_eq!(ends(&mode("<w:applyBreakingRules/>", 12)).unwrap(), "点括。");
}

const SZ_11: &str = r#"<w:sz w:val="22"/>"#;

/// MS Mincho with Japanese as the East Asian language.
const MINCHO_JA: &str = r#"<w:rPr><w:rFonts w:ascii="MS Mincho" w:hAnsi="MS Mincho" w:eastAsia="MS Mincho"/><w:lang w:eastAsia="ja-JP"/></w:rPr>"#;

/// The line lengths, in characters without trailing spaces, of one paragraph of `runs` in a 41-character MS Mincho
/// 11 pt column, compat 14.
fn mincho_lines(rpr: &str, runs: &str) -> Option<Vec<usize>> {
    if !fonts().is_installed("MS Mincho") {
        return None;
    }
    let c14 = r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="14"/></w:compat>"#;
    let p = format!(r#"<w:p><w:pPr><w:ind w:right="340"/></w:pPr>{runs}</w:p>"#);
    Some(lines(c14, rpr, &p)?.into_iter().map(|l| l.1.trim_end().chars().count()).collect())
}

#[test]
fn kinsoku_classes_follow_the_runs_east_asian_language() {
    let run = |lang: &str, text: &str| {
        format!(
            r#"<w:r><w:rPr><w:rFonts w:ascii="MS Mincho" w:hAnsi="MS Mincho" w:eastAsia="MS Mincho"/><w:lang w:val="en-US" w:eastAsia="{lang}"/></w:rPr><w:t xml:space="preserve">{}{text}</w:t></w:r>"#,
            "字".repeat(40)
        )
    };
    let Some(ja) = mincho_lines(SZ_11, &run("ja-JP", "$字字")) else { return };
    // A half-width `$` doesn't end a Japanese line; in an English run it does.
    assert_eq!(ja[0], 40);
    assert_eq!(mincho_lines(SZ_11, &run("en-US", "$字字")).unwrap()[0], 41);
    // Across a space: no break after it before `）`, none before the space, so between the last two 字; in English after the space.
    assert_eq!(mincho_lines(SZ_11, &run("ja-JP", " ）字字")).unwrap()[0], 39);
    assert_eq!(mincho_lines(SZ_11, &run("en-US", " ）字字")).unwrap()[0], 40);
}

#[test]
fn a_korean_run_wraps_as_words_unless_word_wrap_is_off() {
    let run = |lang: &str| {
        format!(
            r#"<w:r><w:rPr><w:rFonts w:ascii="MS Mincho" w:hAnsi="MS Mincho" w:eastAsia="MS Mincho"/><w:lang w:val="en-US" w:eastAsia="{lang}"/></w:rPr><w:t xml:space="preserve">{} 字字字字字</w:t></w:r>"#,
            "字".repeat(38)
        )
    };
    // Kanji break anywhere; in a Korean run only after the space, unless wordWrap is off.
    let Some(en) = mincho_lines(SZ_11, &run("en-US")) else { return };
    assert_eq!(en[0], 41);
    assert_eq!(mincho_lines(SZ_11, &run("ko-KR")).unwrap()[0], 38);
    let no_wrap = mincho_lines(SZ_11, &format!(r#"</w:p><w:p><w:pPr><w:wordWrap w:val="0"/><w:ind w:right="340"/></w:pPr>{}"#, run("ko-KR")));
    assert_eq!(no_wrap.unwrap()[1], 41);
    // No break at the space before a `)`, none after it: cut where the line is full.
    let paren = run("ko-KR").replace(&format!("{} 字", "字".repeat(38)), &format!("{} )字", "字".repeat(40)));
    assert_eq!(mincho_lines(SZ_11, &paren).unwrap()[0], 42);
}

#[test]
fn a_korean_runs_punctuation_never_breaks_and_its_cut_takes_a_space_down() {
    let run = |lang: &str, text: &str| {
        format!(
            r#"<w:r><w:rPr><w:rFonts w:ascii="MS Mincho" w:hAnsi="MS Mincho" w:eastAsia="MS Mincho"/><w:lang w:val="en-US" w:eastAsia="{lang}"/></w:rPr><w:t xml:space="preserve">{text}</w:t></w:r>"#
        )
    };
    let (k30, k40) = ("字".repeat(30), "字".repeat(40));
    // No break at the closing bracket: the run is cut at the full line.
    let Some(ko) = mincho_lines(SZ_11, &run("ko-KR", &format!("{k30}）{}", "字".repeat(14)))) else { return };
    assert_eq!(ko[0], 41);
    // Not after the space after an opening bracket: the cut falls after the bracket and the space starts line 2.
    assert_eq!(mincho_lines(SZ_11, &run("ko-KR", &format!("{k40}「 字字字字字"))).unwrap(), [41, 6]);
    assert_eq!(mincho_lines(SZ_11, &run("ko-KR", &format!("{k40}） 字字字字字"))).unwrap(), [41, 5]);
}

#[test]
fn a_half_width_voiced_mark_is_a_character_of_its_own() {
    let run = |lang: &str| {
        format!(
            r#"<w:r><w:rPr><w:rFonts w:ascii="MS Mincho" w:hAnsi="MS Mincho" w:eastAsia="MS Mincho"/><w:lang w:val="en-US" w:eastAsia="{lang}"/></w:rPr><w:t>{}ﾞ字字字字字</w:t></w:r>"#,
            "字".repeat(41)
        )
    };
    // A line may start with it; in Japanese it may not, so the 字 before it goes along.
    let Some(en) = mincho_lines(SZ_11, &run("en-US")) else { return };
    assert_eq!(en, [41, 6]);
    assert_eq!(mincho_lines(SZ_11, &run("ja-JP")).unwrap(), [40, 7]);
}

#[test]
fn punctuation_hangs_in_east_asian_paragraphs() {
    let run = format!("<w:r>{MINCHO_JA}<w:t>{}、字字</w:t></w:r>", "字".repeat(41));
    let Some(plain) = mincho_lines(SZ_11, &run) else { return };
    // With an East Asian default font the comma that doesn't fit stays on the line past the margin; else it takes
    // the last 字 down with it.
    let hung = mincho_lines(r#"<w:rFonts w:eastAsia="MS Mincho"/><w:sz w:val="22"/>"#, &run).unwrap();
    assert_eq!((plain[0], hung[0]), (40, 42));
    // Compat 15 hangs none.
    let c15 = r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat>"#;
    let body = format!(r#"<w:p><w:pPr><w:ind w:right="340"/></w:pPr>{run}</w:p>"#);
    let c15 = lines(c15, r#"<w:rFonts w:eastAsia="MS Mincho"/><w:sz w:val="22"/>"#, &body).unwrap();
    assert_eq!(c15[0].1.chars().count(), 40);
}

/// Each character's x (twips) on the first page of distributed MS Mincho paragraphs of `texts` in a 9020 tw column.
fn distributed_xs(settings: &str, ppr: &str, texts: &[&str]) -> Option<Vec<(char, f64)>> {
    if !fonts().is_installed("MS Mincho") {
        return None;
    }
    let body: String = texts
        .iter()
        .map(|t| format!(r#"<w:p><w:pPr><w:jc w:val="distribute"/><w:ind w:right="340"/>{ppr}</w:pPr><w:r>{MINCHO_JA}<w:t xml:space="preserve">{t}</w:t></w:r></w:p>"#))
        .collect();
    let c = super::page_chars(&test_doc(settings, SZ_11, &body, "")?).swap_remove(0);
    Some(c.into_iter().map(|(ch, x, _)| (ch, x * 20.0)).collect())
}

#[test]
fn a_distributed_line_spreads_its_characters_and_spaces_east_asian_text_from_latin() {
    let Some(xs) = distributed_xs("", "", &["字", "字ab字c字"]) else { return };
    let near = |got: &[(char, f64)], want: &[f64]| got.len() == want.len() && got.iter().zip(want).all(|(g, w)| (g.1 - w).abs() < 0.5);
    // Alone, centred; the last line spread to both margins, half an em more at each of the four boundaries.
    assert!(near(&xs, &[5840.0, 1440.0, 3288.0, 4916.0, 6654.0, 8502.0, 10240.0]), "{xs:?}");
    // Without autoSpaceDE every gap is (9020 − 990) / 5.
    let plain = distributed_xs("", r#"<w:autoSpaceDE w:val="0"/>"#, &["字ab字c字"]).expect("lays out");
    assert!(near(&plain, &[1440.0, 3266.0, 4982.0, 6698.0, 8524.0, 10240.0]), "{plain:?}");
    // Compat 15: the space first stretches by half its width, then the gaps are equal.
    let c15 = r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat>"#;
    let xs = distributed_xs(c15, "", &["abc def"]).expect("lays out");
    let g = (9020.0 - 770.0 - 55.0) / 6.0;
    assert!(near(&xs, &[0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0].map(|k| 1440.0 + k * (110.0 + g) + if k > 3.5 { 55.0 } else { 0.0 })), "{xs:?}");
    // A line 110 tw short of the margin with four spaces: each space takes half of its stretch, the letters keep
    // their pitch.
    let a = |n: usize| "a".repeat(n);
    let text = [a(16), a(16), a(16), a(15), a(14)].join(" ");
    let xs = distributed_xs(c15, "", &[&text]).expect("lays out");
    let want: Vec<f64> = text.chars().enumerate().filter(|c| c.1 != ' ').map(|(i, _)| 1440.0 + i as f64 * 110.0 + 27.5 * text[..i].matches(' ').count() as f64).collect();
    let ink: Vec<(char, f64)> = xs.into_iter().filter(|c| c.0 != ' ').collect();
    assert!(near(&ink, &want), "{ink:?}");
}

#[test]
fn top_line_punct_takes_the_left_half_off_an_opening_bracket_at_a_line_start() {
    if !fonts().is_installed("MS Mincho") {
        return;
    }
    let first = |rpr: &str, ppr: &str, text: &str| {
        let body = format!("<w:p><w:pPr>{ppr}</w:pPr><w:r>{MINCHO_JA}<w:t>{text}</w:t></w:r></w:p>");
        test_doc("", rpr, &body, "").map(|d| super::page_chars(&d)[0][..2].iter().map(|c| c.1 * 20.0).collect::<Vec<_>>())
    };
    let ea = r#"<w:rFonts w:eastAsia="MS Mincho"/><w:sz w:val="22"/>"#;
    let Some(cut) = first(ea, "<w:topLinePunct/>", "「字字") else { return };
    assert!((cut[0] - 1330.0).abs() < 0.01 && (cut[1] - 1550.0).abs() < 0.01, "{cut:?}");
    // An opening quote only in the East Asian font slot.
    let hinted = |text: &str| first(ea, "<w:topLinePunct/>", text).map(|x| x[0]);
    let hint = |text: &str| {
        let body = format!(
            r#"<w:p><w:pPr><w:topLinePunct/></w:pPr><w:r><w:rPr><w:rFonts w:hint="eastAsia" w:ascii="MS Mincho" w:hAnsi="MS Mincho" w:eastAsia="MS Mincho"/><w:lang w:eastAsia="ja-JP"/></w:rPr><w:t>{text}</w:t></w:r></w:p>"#
        );
        test_doc("", ea, &body, "").map(|d| super::page_chars(&d)[0][0].1 * 20.0)
    };
    assert!((hinted("“字字").unwrap() - 1440.0).abs() < 0.01);
    assert!((hint("“字字").unwrap() - 1330.0).abs() < 0.01);
    // Not for `(`, not without the option, not outside an East Asian paragraph.
    assert!((first(ea, "<w:topLinePunct/>", "(字字").unwrap()[0] - 1440.0).abs() < 0.01);
    assert!((first(ea, "", "「字字").unwrap()[0] - 1440.0).abs() < 0.01);
    assert!((first(SZ_11, "<w:topLinePunct/>", "「字字").unwrap()[0] - 1440.0).abs() < 0.01);
}

#[test]
fn compat_12_font_metric_options() {
    let c12 = |opt: &str| {
        format!(
            r#"<w:compat>{opt}<w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="12"/></w:compat>"#
        )
    };
    let arial = r#"<w:rFonts w:ascii="Arial" w:hAnsi="Arial"/><w:sz w:val="22"/>"#;
    // noLeading: Arial's line gap (67 of 2048 units) leaves the line pitch.
    let two = r#"<w:p><w:r><w:t>One</w:t></w:r><w:r><w:br/><w:t>Two</w:t></w:r></w:p>"#;
    let pitch = |s: &str| baselines(s, arial, two).map(|l| l[1].3 - l[0].3);
    let Some(plain) = pitch(&c12("")) else { return };
    assert!((plain - pitch(&c12("<w:noLeading/>")).unwrap() - 0.36).abs() < 0.01, "{plain}");
    // suppressTopSpacingWP: a proportional font's line is its size plus 2pt, Courier New's its size.
    assert!((pitch(&c12("<w:suppressTopSpacingWP/>")).unwrap() - 13.0).abs() < 0.01);
    let courier = r#"<w:rFonts w:ascii="Courier New" w:hAnsi="Courier New"/><w:sz w:val="22"/>"#;
    let wp = baselines(&c12("<w:suppressTopSpacingWP/>"), courier, two).map(|l| l[1].3 - l[0].3);
    assert!(wp.is_none_or(|p| (p - 11.0).abs() < 0.01), "{wp:?}");
    // noSpaceRaiseLower: raised text doesn't move the baseline down.
    let raised = r#"<w:p><w:r><w:t>Plain </w:t></w:r><w:r><w:rPr><w:position w:val="12"/></w:rPr><w:t>raised</w:t></w:r></w:p>"#;
    let y = |s: &str, p: &str| baselines(s, arial, p).map(|l| l[0].3).unwrap();
    let flat = y(&c12(""), two);
    assert!(y(&c12(""), raised) > flat + 5.0);
    assert!((y(&c12("<w:noSpaceRaiseLower/>"), raised) - flat).abs() < 1e-6);
    // spacingInWholePoints: 1.3pt spacing counts 1pt; mwSmallCaps: 11pt small capitals at 10pt, not 9pt. The
    // Courier New bar is its own glyph run, so its start is where the run before it ends.
    let end = |s: &str, rpr: &str| {
        let p = format!(
            r#"<w:p><w:r><w:rPr>{rpr}</w:rPr><w:t>abcd</w:t></w:r><w:r><w:rPr><w:rFonts w:ascii="Courier New" w:hAnsi="Courier New"/></w:rPr><w:t>|</w:t></w:r></w:p>"#
        );
        baselines(s, arial, &p).map(|l| l[0].2).unwrap()
    };
    let sp = r#"<w:spacing w:val="26"/>"#;
    assert!((end(&c12(""), sp) - end(&c12("<w:spacingInWholePoints/>"), sp) - 1.2).abs() < 0.01);
    let caps = "<w:smallCaps/>";
    assert!(end(&c12("<w:mwSmallCaps/>"), caps) > end(&c12(""), caps) + 1.0);
    let c15 = r#"<w:compat><w:noLeading/><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat>"#;
    let p15 = |s: &str| baselines(s, arial, two).map(|l| l[1].3 - l[0].3).unwrap();
    assert!((p15(c15) - p15(&c15.replace("<w:noLeading/>", ""))).abs() < 1e-6, "compat 15 clears the options");
}

#[test]
fn compat_12_options_take_the_hanging_indent_stop_from_a_list_numbers_tab() {
    let numbering = r#"<w:abstractNum w:abstractNumId="1"><w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="decimal"/><w:lvlText w:val="%1."/><w:lvlJc w:val="left"/><w:pPr><w:ind w:left="1440" w:hanging="1080"/></w:pPr></w:lvl></w:abstractNum><w:num w:numId="1"><w:abstractNumId w:val="1"/></w:num>"#;
    let body = r#"<w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr></w:pPr><w:r><w:t>Text</w:t></w:r></w:p>"#;
    let text = |opt: &str, mode: u32| {
        let s = format!(
            r#"<w:compat>{opt}<w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="{mode}"/></w:compat>"#
        );
        numbered_baselines(&s, ARIAL_11, body, numbering).map(|l| l[0].2)
    };
    let Some(plain) = text("", 12) else { return };
    assert!((plain - 144.0).abs() < 0.01, "the hanging indent: {plain}");
    // The next default stop after the number instead; compat 15 clears the options.
    for opt in ["<w:noTabHangInd/>", "<w:doNotUseIndentAsNumberingTabStop/>"] {
        assert!((text(opt, 12).unwrap() - 108.0).abs() < 0.01, "{opt}");
        assert!((text(opt, 15).unwrap() - 144.0).abs() < 0.01, "{opt}");
    }
}

#[test]
fn a_column_break_in_a_frame_breaks_its_line_only_with_show_breaks_in_frames() {
    let body = r#"<w:p><w:pPr><w:framePr w:w="3000" w:hSpace="180" w:wrap="around" w:vAnchor="text" w:hAnchor="margin" w:x="0" w:y="0"/></w:pPr><w:r><w:t>before</w:t></w:r><w:r><w:br w:type="column"/><w:t>after</w:t></w:r></w:p><w:p><w:r><w:t>Body</w:t></w:r></w:p>"#;
    let framed = |opt: &str, mode: u32| {
        let s = format!(
            r#"<w:compat>{opt}<w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="{mode}"/></w:compat>"#
        );
        lines(&s, ARIAL_12, body).map(|l| l.into_iter().map(|x| x.1).filter(|t| t != "Body").collect::<Vec<_>>())
    };
    let Some(plain) = framed("", 12) else { return };
    assert_eq!(plain, ["beforeafter"]);
    assert_eq!(framed("<w:showBreaksInFrames/>", 12).unwrap(), ["before", "after"]);
    assert_eq!(framed("<w:showBreaksInFrames/>", 15).unwrap(), ["beforeafter"]);
}

#[test]
fn a_text_anchored_frames_y_is_its_offset_from_the_text() {
    let cut = |y: u32| {
        let sp = r#"<w:spacing w:before="0" w:after="0" w:line="300" w:lineRule="exact"/>"#;
        let body = format!(
            r#"<w:p><w:pPr><w:framePr w:w="1440" w:h="200" w:hRule="exact" w:hSpace="180" w:vSpace="0" w:wrap="around" w:vAnchor="text" w:hAnchor="page" w:x="5000" w:y="{y}"/><w:spacing w:before="0" w:after="0" w:line="200" w:lineRule="exact"/></w:pPr><w:r><w:t>F</w:t></w:r></w:p><w:p><w:pPr>{sp}</w:pPr><w:r><w:t>Anchor</w:t></w:r></w:p><w:p><w:pPr>{sp}</w:pPr><w:r><w:t>Next line text runs on beside the box so that every line shows where the edge is.</w:t></w:r></w:p>"#
        );
        let l = baselines("", ARIAL_12, &body)?;
        let first = l.iter().find(|(_, t, _, _)| t.starts_with("Next")).expect("drawn").3;
        Some(l.iter().filter(|(_, t, _, y)| t != "F" && (y - first).abs() < 0.01).count())
    };
    let Some(above) = cut(100) else { return };
    // Its bottom at the next line's top leaves that line whole; one twip lower cuts it.
    assert_eq!(above, 1);
    assert_eq!(cut(101).unwrap(), 2);
}

const C15: &str = r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat>"#;

/// How many lines of a long body paragraph after a one-line frame (`attrs` added to its `w:framePr`) start beside it.
fn beside_frame(settings: &str, attrs: &str) -> Option<usize> {
    let words = "Hxx aaa bbb ccc ddd eee fff ggg hhh iii jjj kkk lll mmm nnn ooo ppp qqq rrr sss ttt uuu vvv www xxx yyy zzz ".repeat(2);
    let body = format!(
        r#"<w:p><w:pPr><w:framePr w:vAnchor="text" w:hAnchor="margin" w:x="0" w:y="0" w:wrap="around" w:w="3000" {attrs}/></w:pPr><w:r><w:t>f1</w:t></w:r></w:p><w:p><w:r><w:t>{words}</w:t></w:r></w:p>"#
    );
    Some(baselines(settings, ARIAL_12, &body)?.iter().filter(|l| l.1 != "f1" && l.0 > 200.0).count())
}

#[test]
fn a_frames_auto_height_rule_ignores_its_height() {
    let Some(auto) = beside_frame("", r#"w:h="1000" w:hRule="auto""#) else { return };
    assert_eq!(auto, 1);
    assert_eq!(beside_frame("", r#"w:h="1000" w:hRule="atLeast""#).unwrap(), 4);
}

#[test]
fn a_compat_14_frame_keeps_no_vertical_space_below_it() {
    let Some(legacy) = beside_frame("", r#"w:vSpace="200" w:hSpace="200""#) else { return };
    assert_eq!(legacy, 1);
    assert_eq!(beside_frame(C15, r#"w:vSpace="200" w:hSpace="200""#).unwrap(), 2);
}

#[test]
fn a_compat_14_frame_without_a_vertical_position_stays_in_the_flow() {
    let frame_y = |settings: &str, at: &str| {
        let body = format!(
            r#"<w:p><w:r><w:t>Before.</w:t></w:r></w:p><w:p><w:pPr><w:framePr w:w="1440" w:wrap="around" {at} w:hAnchor="margin" w:x="0"/></w:pPr><w:r><w:t>Frame</w:t></w:r></w:p><w:p><w:r><w:t>After.</w:t></w:r></w:p>"#
        );
        let l = baselines(settings, ARIAL_12, &body)?;
        let y = |t: &str| l.iter().find(|x| x.1 == t).map(|x| x.3).expect("drawn");
        Some(y("Frame") - y("Before."))
    };
    for at in [r#"w:vAnchor="margin" w:y="0""#, r#"w:vAnchor="page""#, r#"w:vAnchor="margin" w:yAlign="inline""#] {
        let Some(legacy) = frame_y("", at) else { return };
        assert!(legacy > 10.0, "{at}: {legacy}");
    }
    // Compat 15 puts it at the margin's top, beside the first line.
    assert!(frame_y(C15, r#"w:vAnchor="margin" w:y="0""#).unwrap().abs() < 0.01);
}

#[test]
fn a_frame_in_the_flow_that_meets_an_earlier_one_goes_below_it() {
    let frame = |x: u32, t: &str, apart: bool| {
        let so = if apart { "<w:suppressOverlap/>" } else { "" };
        format!(
            r#"<w:p><w:pPr><w:framePr w:w="2880" w:hSpace="180" w:vSpace="0" w:wrap="around" w:vAnchor="text" w:hAnchor="page" w:x="{x}" w:y="0"/>{so}</w:pPr><w:r><w:t>{t}</w:t></w:r></w:p>"#
        )
    };
    let b = |settings: &str, apart: bool| {
        let body = format!("<w:p><w:r><w:t>Lead.</w:t></w:r></w:p>{}{}<w:p><w:r><w:t>Body text beside the frames.</w:t></w:r></w:p>", frame(2880, "Frame A", false), frame(4320, "Frame B", apart));
        let l = baselines(settings, ARIAL_12, &body)?;
        let at = |t: &str| l.iter().find(|x| x.1 == t).map(|x| (x.0, x.3)).expect("drawn");
        Some((at("Frame A"), at("Frame B")))
    };
    let Some(((_, ay), (bx, by))) = b("", false) else { return };
    // Compat ≤ 14 lays B out below A, at its own x.
    assert!(by > ay + 10.0 && (bx - 216.0).abs() < 0.1, "{ay} {bx} {by}");
    // Compat 15 lets them overlap; B may not: it moves down, not across.
    let c15 = r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat>"#;
    let ((_, ay), (_, by)) = b(c15, false).unwrap();
    assert!((by - ay).abs() < 0.01, "{ay} {by}");
    let ((_, ay), (bx, by)) = b(c15, true).unwrap();
    assert!(by > ay + 10.0 && (bx - 216.0).abs() < 0.1, "{ay} {bx} {by}");
}

#[test]
fn a_compat_14_frame_in_the_flow_drops_when_it_meets_the_earlier_ones_distance_from_the_nearest_twip() {
    let frame = |x: u32, w: u32, h: u32, t: &str, v: &str| {
        format!(
            r#"<w:p><w:pPr><w:framePr w:w="{w}" w:h="{h}" w:hRule="exact" w:hSpace="180" w:vSpace="0" w:wrap="around" {v} w:hAnchor="page" w:x="{x}"/><w:spacing w:before="0" w:after="0" w:line="200" w:lineRule="exact"/></w:pPr><w:r><w:t>{t}</w:t></w:r></w:p>"#
        )
    };
    let lead = |n: usize| (0..n).map(|i| format!("<w:p><w:r><w:t>Lead {i}.</w:t></w:r></w:p>")).collect::<String>();
    let in_flow = r#"w:vAnchor="text" w:y="0""#;
    let pair = |n: usize, a_at: &str, bx: u32, bw: u32| {
        let body = format!("{}{}{}<w:p><w:r><w:t>After.</w:t></w:r></w:p>", lead(n), frame(4000, 1440, 600, "A", a_at), frame(bx, bw, 200, "B", in_flow));
        let l = baselines("", ARIAL_11, &body)?;
        let at = |t: &str| l.iter().find(|x| x.1 == t).map(|x| x.3).expect("drawn");
        Some((at("A"), at("B")))
    };
    let b = |n: usize, bx: u32, bw: u32| pair(n, in_flow, bx, bw);
    // A's box with its distance ends at 5619 tw: B drawn from 5500 meets it and drops to A's bottom.
    let Some((ay, by)) = b(1, 5501, 1440) else { return };
    assert!((by - ay - 30.0).abs() < 0.1, "{ay} {by}");
    // From 5620 it doesn't.
    let (ay, by) = b(1, 5621, 1440).unwrap();
    assert!((by - ay).abs() < 0.01, "{ay} {by}");
    // After 28 single lines A ends 0.4 tw past a twip: B starts at the twip before it.
    let (ay, by) = b(28, 4500, 1000).unwrap();
    assert!(((by - ay) * 20.0 - 599.6).abs() < 0.05, "{ay} {by}");
    // Over an earlier frame placed from the page it stays at its flow y.
    let (ay, by) = pair(1, r#"w:vAnchor="page" w:y="1693""#, 4500, 1000).unwrap();
    assert!(((by - ay) * 20.0 - 0.98).abs() < 0.05, "{ay} {by}");
}

/// Three Arial 11 frame lines from `at`, `x` and `w` in twips, `sup` its `suppressOverlap`.
fn three_line_frame(x: u32, w: u32, at: &str, sup: &str, t: &str) -> String {
    let p = |n: &str| format!(r#"<w:r><w:t>{t} {n}</w:t></w:r>"#);
    format!(
        r#"<w:p><w:pPr><w:framePr w:w="{w}" w:hSpace="180" w:vSpace="0" w:wrap="around" {at} w:hAnchor="page" w:x="{x}"/>{sup}<w:spacing w:before="0" w:after="0" w:line="240" w:lineRule="auto"/></w:pPr>{}<w:r><w:br/></w:r>{}<w:r><w:br/></w:r>{}</w:p>"#,
        p("one"),
        p("two"),
        p("three")
    )
}

const FRAME_BODY: &str = "Body text runs on beside the box so that every line shows where the wrap edge lies, and the words are short enough to fill each gap well. Body text runs on beside the box so that every line shows where the wrap edge lies, and the words are short enough to fill each gap well. Body text runs on beside the box.";

/// The lines of a lead line, `frames` and a body paragraph: x and baseline in twips, and the text.
fn after_frames(frames: &str) -> Option<Vec<(f64, f64, String)>> {
    let body = format!(r#"<w:p><w:r><w:t>Lead paragraph.</w:t></w:r></w:p>{frames}<w:p><w:r><w:t>{FRAME_BODY}</w:t></w:r></w:p>"#);
    let l = baselines(r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="12"/></w:compat>"#, ARIAL_11, &body)?;
    Some(l.into_iter().map(|(x, t, _, y)| (x * 20.0, y * 20.0, t)).collect())
}

#[test]
fn a_frames_paragraph_borders_reach_past_it_by_a_gap_their_width_and_spacing_less_the_indent() {
    let right_piece = |pbdr: &str| {
        let a = three_line_frame(2880, 2880, r#"w:vAnchor="page" w:y="1693""#, pbdr, "A");
        let l = after_frames(&a)?;
        l.iter().filter(|x| x.2.starts_with("Body") || !x.2.starts_with(['A', 'L'])).map(|x| x.0).find(|&x| x > 3000.0)
    };
    let side = |s: &str, sz: u32, space: u32| format!(r#"<w:{s} w:val="single" w:sz="{sz}" w:space="{space}" w:color="000000"/>"#);
    let Some(plain) = right_piece("") else { return };
    // 2879 + 2880 + 180: then 30 tw, the 10 tw line and its spacing on the right.
    assert!((plain - 5939.0).abs() < 1.0, "{plain}");
    let bordered = right_piece(&format!("<w:pBdr>{}{}</w:pBdr>", side("left", 4, 0), side("right", 4, 0))).unwrap();
    assert!((bordered - 5979.0).abs() < 1.0, "{bordered}");
    let spaced = right_piece(&format!(r#"<w:pBdr>{}</w:pBdr><w:ind w:right="20"/>"#, side("right", 4, 2))).unwrap();
    assert!((spaced - 5939.0 - 30.0 - 10.0 - 40.0 + 20.0).abs() < 1.0, "{spaced}");
}

#[test]
fn a_compat_14_frame_keeps_its_place_once_a_later_float_meets_the_text_before_it() {
    let pic = r#"<w:r><w:drawing><wp:anchor distT="0" distB="0" distL="114300" distR="114300" simplePos="0" relativeHeight="0" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="column"><wp:posOffset>1270000</wp:posOffset></wp:positionH><wp:positionV relativeFrom="paragraph"><wp:posOffset>0</wp:posOffset></wp:positionV><wp:extent cx="914400" cy="635000"/><wp:wrapSquare wrapText="bothSides"/><a:graphic><a:graphicData><pic:pic><pic:blipFill/></pic:pic></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r>"#;
    let b_at = |later: &str| {
        let frames = three_line_frame(1440, 2880, r#"w:vAnchor="text" w:y="0""#, "", "A")
            + &three_line_frame(2880, 2880, r#"w:vAnchor="text" w:y="360""#, "<w:suppressOverlap/>", "B");
        let body = format!(r#"<w:p><w:r><w:t>Lead.</w:t></w:r></w:p>{frames}<w:p><w:r><w:t>{FRAME_BODY}</w:t></w:r></w:p>{later}"#);
        let l = baselines("", ARIAL_11, &body)?;
        l.iter().find(|x| x.1 == "B one").map(|x| x.0 * 20.0)
    };
    // Moved clear of A, right of it; a later picture meeting its own first line leaves B a twip left of its x.
    let Some(moved) = b_at("") else { return };
    assert!((moved - 4679.0).abs() < 1.0, "{moved}");
    let kept = b_at(&format!(r#"<w:p>{pic}<w:r><w:t>A later paragraph with a picture.</w:t></w:r></w:p>"#)).unwrap();
    assert!((kept - 2879.0).abs() < 1.0, "{kept}");
}

#[test]
fn a_compat_14_frame_in_the_flow_that_may_not_overlap_goes_a_twip_short_of_a_page_frames_bottom() {
    let a = three_line_frame(2880, 2880, r#"w:vAnchor="page" w:y="1693""#, "", "A");
    let b = three_line_frame(4320, 2880, r#"w:vAnchor="text" w:y="0""#, "<w:suppressOverlap/>", "B");
    let Some(l) = after_frames(&(a + &b)) else { return };
    let at = |t: &str| l.iter().find(|x| x.2 == t).map(|x| x.1).expect("drawn");
    // A is at 1692 tw, 758.94 tw high: B goes to 1692 + 759 - 1.
    assert!((at("B one") - at("A one") - 758.0).abs() < 0.05, "{l:?}");
}

#[test]
fn a_compat_14_line_with_one_piece_left_stays_beside_a_float_starting_above_the_other_ones_bottom() {
    // A (1692 to 2450.94 tw) leaves one piece of the lines beside it; B starts at 2450 in the third body line.
    let frames = |by: u32| three_line_frame(2880, 2880, r#"w:vAnchor="page" w:y="1693""#, "", "A") + &three_line_frame(4320, 2880, &format!(r#"w:vAnchor="page" w:y="{by}""#), "", "B");
    let third = |by: u32| after_frames(&frames(by)).map(|l| l.into_iter().filter(|x| !x.2.starts_with(['A', 'B', 'L']) || x.2.starts_with("Body")).nth(2).expect("drawn"));
    let Some((x, y, _)) = third(2451) else { return };
    // It keeps its top (baseline 2405.2 tw), right of B (7379 tw).
    assert!((y - 2405.2).abs() < 0.5 && (x - 7379.0).abs() < 0.5, "{x} {y}");
    // B starting a twip after A's bottom: the line goes to it.
    let (_, y, _) = third(2452).unwrap();
    assert!((y - 2656.8).abs() < 0.5, "{y}");
}

#[test]
fn a_compat_14_frame_placed_apart_from_others_starts_at_the_nearest_twip() {
    // The flow y after the lead line is 1692.98 tw: a frame that may not overlap starts at 1693 and its bottom covers
    // the fourth body line's top by 0.02 tw; the other frame, placed first, ends at that top.
    let frames = |sup: &str| three_line_frame(2880, 1440, r#"w:vAnchor="text" w:y="0""#, "", "A") + &three_line_frame(7200, 1440, r#"w:vAnchor="text" w:y="0""#, sup, "B");
    let fourth = |sup: &str| after_frames(&frames(sup)).map(|l| l.into_iter().filter(|x| (x.1 - 2659.6).abs() < 3.0).map(|x| x.0).collect::<Vec<f64>>());
    let Some(cut) = fourth("<w:suppressOverlap/>") else { return };
    assert!(cut.len() == 2 && (cut[1] - 8819.0).abs() < 0.5, "{cut:?}");
    assert_eq!(fourth("").unwrap().len(), 1);
}

/// The body lines after a lead line and `frames`: x and baseline in twips, and the text.
fn body_after_frames(frames: &str) -> Option<Vec<(f64, f64, String)>> {
    Some(after_frames(frames)?.into_iter().filter(|x| !x.2.starts_with(['A', 'B', 'L']) || x.2.starts_with("Body")).collect())
}

#[test]
fn a_compat_14_line_keeps_the_end_of_the_float_that_cut_it_first() {
    // A at x 3200 leaves two pieces of the lines beside it; B starts at 2450 in the third body line. With A placed
    // first the right piece ends at A's bottom and the line goes there; with B first it ends at B's and the line stays,
    // right of B.
    let a = three_line_frame(3200, 2880, r#"w:vAnchor="page" w:y="1693""#, "", "A");
    let b = three_line_frame(4640, 2880, r#"w:vAnchor="page" w:y="2451""#, "", "B");
    let third = |frames: String| body_after_frames(&frames).map(|l| l.into_iter().filter(|x| x.1 > 2300.0).map(|x| (x.0, x.1)).next().expect("drawn"));
    let Some((x, y)) = third(b.clone() + &a) else { return };
    assert!((y - 2405.2).abs() < 0.5 && (x - 7699.0).abs() < 0.5, "{x} {y}");
    let (x, y) = third(a + &b).unwrap();
    assert!((y - 2657.2).abs() < 1.5 && (x - 1440.0).abs() < 0.5, "{x} {y}");
}

#[test]
fn a_compat_14_line_after_one_fitted_with_a_slack_stays_in_its_pieces() {
    // B placed first: the third body line stays right of B in a piece ending at B's bottom (3208.94 tw); the next three
    // lines keep that piece, though B alone leaves a piece left of it too.
    let a = three_line_frame(2880, 2880, r#"w:vAnchor="page" w:y="1693""#, "", "A");
    let b = three_line_frame(4320, 2880, r#"w:vAnchor="page" w:y="2451""#, "", "B");
    let Some(l) = body_after_frames(&(b + &a)) else { return };
    let beside: Vec<(f64, f64)> = l.iter().filter(|x| x.1 > 2300.0 && x.1 < 3200.0).map(|x| (x.0, x.1)).collect();
    assert!(beside.len() == 4 && beside.iter().all(|x| (x.0 - 7379.0).abs() < 0.5), "{beside:?}");
}

#[test]
fn a_compat_14_line_after_one_moved_down_goes_below_a_float_it_would_overhang() {
    // The third body line goes to A's bottom (2450.94 tw), 252 tw down; B (2451 to 3209.94) cuts the next three. The
    // fourth line below it would start 0.06 tw inside B: it goes to B's bottom, full width.
    let a = three_line_frame(2880, 2880, r#"w:vAnchor="page" w:y="1693""#, "", "A");
    let b = three_line_frame(4320, 2880, r#"w:vAnchor="page" w:y="2452""#, "", "B");
    let Some(l) = body_after_frames(&(a + &b)) else { return };
    let low: Vec<(f64, f64)> = l.iter().filter(|x| x.1 > 3300.0 && x.1 < 3500.0).map(|x| (x.0, x.1)).collect();
    assert!(low.len() == 1 && (low[0].1 - (3209.94 + 206.4)).abs() < 0.5, "{low:?}");
}

#[test]
fn a_compat_14_line_blocked_by_a_frame_goes_just_below_it() {
    // Frames leaving only a piece too narrow for text: each body line goes to the bottom of the frame it meets.
    let frame = |y: u32| {
        format!(
            r#"<w:p><w:pPr><w:framePr w:w="7925" w:h="200" w:hRule="exact" w:hSpace="180" w:wrap="around" w:vAnchor="page" w:hAnchor="page" w:x="1440" w:y="{y}"/><w:spacing w:before="0" w:after="0" w:line="200" w:lineRule="exact"/></w:pPr><w:r><w:t>F</w:t></w:r></w:p>"#
        )
    };
    let body = |n: u32| format!(r#"<w:p><w:pPr><w:spacing w:before="0" w:after="0" w:line="300" w:lineRule="exact"/></w:pPr><w:r><w:t>Body {n}</w:t></w:r></w:p>"#);
    let doc: String = (0..4).map(|k| frame(1490 + 600 * k) + &body(k)).collect();
    let Some(l) = baselines("", ARIAL_12, &doc) else { return };
    let y: Vec<f64> = l.iter().filter(|(_, t, _, _)| t.starts_with("Body")).map(|l| l.3).collect();
    // The frames end at 1689, 2289, 2889, 3489 twips: one line below each, 30pt apart.
    assert!(y.len() == 4 && y.windows(2).all(|w| (w[1] - w[0] - 30.0).abs() < 0.5), "{y:?}");
}

/// Compat 12 settings with the options in place of `{}`.
const C12_WITH: &str = r#"<w:compat>{}<w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="12"/></w:compat>"#;

#[test]
fn no_extra_line_spacing_puts_an_exact_lines_baseline_at_its_ascent() {
    let p = r#"<w:p><w:pPr><w:spacing w:line="400" w:lineRule="exact"/></w:pPr><w:r><w:t>Exact</w:t></w:r></w:p>"#;
    let Some(off) = baselines("", ARIAL_11, p) else { return };
    let on = baselines(&C12_WITH.replace("{}", "<w:noExtraLineSpacing/>"), ARIAL_11, p).expect("Arial");
    assert!((off[0].3 - 72.0 - 16.0).abs() < 0.01 && (on[0].3 - 72.0 - 10.32).abs() < 0.01, "{off:?} {on:?}");
}

#[test]
fn suppress_bottom_spacing_lets_an_exact_line_into_the_room_under_its_text() {
    let body = r#"<w:p><w:pPr><w:spacing w:after="0" w:line="433" w:lineRule="exact"/></w:pPr><w:r><w:t>x</w:t></w:r></w:p>"#.repeat(31);
    let Some(off) = baselines("", ARIAL_11, &body) else { return };
    let on = baselines(&C12_WITH.replace("{}", "<w:suppressBottomSpacing/>"), ARIAL_11, &body).expect("Arial");
    assert_eq!((off.len(), on.len()), (29, 30));
}

#[test]
fn exact_lines_fill_a_page_in_rounded_layout_units() {
    // 30 lines of 432 twips are the text height in twips but 12 layout units over it; 20 of 648 are 8 under.
    let lines = |n: usize, x: u32| {
        let body = format!(r#"<w:p><w:pPr><w:spacing w:after="0" w:line="{x}" w:lineRule="exact"/></w:pPr><w:r><w:t>x</w:t></w:r></w:p>"#).repeat(n + 2);
        baselines("", ARIAL_11, &body).map(|l| l.len())
    };
    let Some(n) = lines(30, 432) else { return };
    assert_eq!((n, lines(20, 648)), (29, Some(20)));
}

#[test]
fn compat_15_text_after_a_split_floating_table_can_stay_beside_its_first_part() {
    let row = |k: u32| format!(r#"<w:tr><w:tc><w:tcPr><w:tcW w:w="1500" w:type="dxa"/></w:tcPr><w:p><w:pPr><w:spacing w:after="0"/></w:pPr><w:r><w:t>R{k:02}</w:t></w:r></w:p></w:tc></w:tr>"#);
    let rows: String = (1..=70).map(row).collect();
    let table = format!(
        r#"<w:tbl><w:tblPr><w:tblpPr w:leftFromText="180" w:rightFromText="180" w:vertAnchor="text" w:horzAnchor="margin" w:tblpY="0"/><w:tblW w:w="1500" w:type="dxa"/><w:tblLayout w:type="fixed"/></w:tblPr><w:tblGrid><w:gridCol w:w="1500"/></w:tblGrid>{rows}</w:tbl>"#
    );
    let after: String = (1..=5).map(|k| format!(r#"<w:p><w:pPr><w:spacing w:after="0"/></w:pPr><w:r><w:t>A{k:02}</w:t></w:r></w:p>"#)).collect();
    let page_of_a01 = |compat: u32, on: bool| {
        let uri = "http://schemas.microsoft.com/office/word";
        let setting = if on { format!(r#"<w:compatSetting w:name="allowTextAfterFloatingTableBreak" w:uri="{uri}" w:val="1"/>"#) } else { String::new() };
        let settings = format!(r#"<w:compat>{setting}<w:compatSetting w:name="compatibilityMode" w:uri="{uri}" w:val="{compat}"/></w:compat>"#);
        let doc = test_doc(&settings, ARIAL_12, &format!("{table}{after}"), "")?;
        let snap = layout(&doc, &fonts());
        let mut page = None;
        for (k, pg) in snap.pages().iter().enumerate() {
            paint(pg, &mut |p| {
                if let Item::Glyphs(r) = p.absolute()
                    && r.chars.iter().map(|c| c.0).collect::<String>().starts_with("A01")
                {
                    page.get_or_insert(k);
                }
            });
        }
        page
    };
    let Some(on) = page_of_a01(15, true) else { return };
    assert_eq!(on, 0);
    // Without it, and in compat 12, the text after starts on the page of the table's last part.
    assert_eq!(page_of_a01(15, false), Some(1));
    assert_eq!(page_of_a01(12, true), Some(1));
}

#[test]
fn a_page_anchored_table_holds_the_rows_that_fit_below_where_it_stands() {
    // `before` lines of 252 twips, then 70 rows of 263 at `tblpY` against the page: (page, baseline) of R01 and the
    // pages of the first part's last row and the row after it.
    let place = |compat: u32, before: u32, y: u32, last: u32| {
        let p = |t: String| format!(r#"<w:p><w:pPr><w:spacing w:after="0" w:line="252" w:lineRule="exact"/></w:pPr><w:r><w:t>{t}</w:t></w:r></w:p>"#);
        let row = |k: u32| format!(r#"<w:tr><w:trPr><w:trHeight w:val="263" w:hRule="exact"/></w:trPr><w:tc><w:tcPr><w:tcW w:w="1500" w:type="dxa"/></w:tcPr>{}</w:tc></w:tr>"#, p(format!("R{k:02}")));
        let rows: String = (1..=70).map(row).collect();
        let lines: String = (1..=before).map(|k| p(format!("B{k:02}"))).collect();
        let body = format!(
            r#"{lines}<w:tbl><w:tblPr><w:tblpPr w:leftFromText="180" w:rightFromText="180" w:vertAnchor="page" w:horzAnchor="margin" w:tblpY="{y}"/><w:tblW w:w="1500" w:type="dxa"/><w:tblLayout w:type="fixed"/></w:tblPr><w:tblGrid><w:gridCol w:w="1500"/></w:tblGrid>{rows}</w:tbl>{}"#,
            p("A01".into())
        );
        let settings = format!(r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="{compat}"/></w:compat>"#);
        let doc = test_doc(&settings, ARIAL_12, &body, "")?;
        let pages = super::page_chars(&doc);
        let find = |t: &str| {
            pages.iter().enumerate().find_map(|(k, pg)| {
                let s: String = pg.iter().map(|c| c.0).collect();
                s.find(t).map(|i| (k, pg[s[..i].chars().count()].2))
            })
        };
        let r01 = find("R01")?;
        Some((r01.0, r01.1, find(&format!("R{last:02}"))?.0, find(&format!("R{:02}", last + 1))?.0))
    };
    let Some((page, y, a, b)) = place(15, 5, 7200, 38) else { return };
    // The rows that fit from the flow end at the paper's bottom edge: the table moves up from 360pt.
    assert_eq!((page, a, b), (0, 0, 1));
    assert!(y < 300.0, "{y}");
    // Compat 12 draws all those rows, past the bottom margin.
    let (_, _, a, b) = place(12, 5, 7200, 44).unwrap();
    assert_eq!((a, b), (0, 1));
    // Above where it stands and not whole: compat 15 sends it to the next page, compat 12 keeps it.
    assert_eq!(place(15, 10, 1440, 49).map(|r| (r.0, r.2, r.3)), Some((1, 1, 2)));
    assert_eq!(place(12, 10, 1440, 39).map(|r| (r.0, r.2, r.3)), Some((0, 0, 1)));
}

#[test]
fn a_floating_table_in_a_cell_is_wrapped_by_the_cell_text() {
    let nested = r#"<w:tbl><w:tblPr><w:tblpPr w:leftFromText="0" w:rightFromText="0" w:vertAnchor="text" w:horzAnchor="margin" w:tblpX="0" w:tblpY="0"/><w:tblW w:w="2000" w:type="dxa"/><w:tblLayout w:type="fixed"/><w:tblCellMar><w:left w:w="0" w:type="dxa"/><w:right w:w="0" w:type="dxa"/></w:tblCellMar></w:tblPr><w:tblGrid><w:gridCol w:w="2000"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w="2000" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>nested</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#;
    let body = format!(
        r#"<w:tbl><w:tblPr><w:tblW w:w="8000" w:type="dxa"/><w:tblLayout w:type="fixed"/><w:tblCellMar><w:left w:w="0" w:type="dxa"/><w:right w:w="0" w:type="dxa"/></w:tblCellMar></w:tblPr><w:tblGrid><w:gridCol w:w="8000"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w="8000" w:type="dxa"/></w:tcPr>{nested}<w:p><w:r><w:t>beside</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p/>"#
    );
    for settings in [COMPAT_15, ""] {
        let Some(l) = baselines(settings, ARIAL_12, &body) else { return };
        // Beside it on its first line, past its unset right border's 0.75pt, its right distance 0 kept.
        let (x, y) = at(&l, "beside");
        assert!((x - 172.75).abs() < 0.1 && (y - at(&l, "nested").1).abs() < 0.1, "{x} {y}");
    }
}

#[test]
fn a_split_row_keeps_a_line_only_with_the_float_it_holds_in_the_cell() {
    // 50 lines of 252 twips, then a row whose first line fits above the bottom margin but its float, 7.2pt below the
    // line and 36pt tall, doesn't: the row goes to the next page whole.
    let p = |t: &str, run: &str| format!(r#"<w:p><w:pPr><w:spacing w:after="0" w:line="252" w:lineRule="exact"/></w:pPr>{run}<w:r><w:t>{t}</w:t></w:r></w:p>"#);
    let float = r#"<w:r><w:drawing><wp:anchor distT="0" distB="0" distL="0" distR="0" simplePos="0" relativeHeight="0" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="column"><wp:posOffset>0</wp:posOffset></wp:positionH><wp:positionV relativeFrom="paragraph"><wp:posOffset>91440</wp:posOffset></wp:positionV><wp:extent cx="457200" cy="457200"/><wp:wrapSquare wrapText="bothSides"/><a:graphic><a:graphicData><pic:pic><pic:blipFill/></pic:pic></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r>"#;
    let fill: String = (0..50).map(|_| p("fill", "")).collect();
    let cell = |w: u32, body: String| format!(r#"<w:tc><w:tcPr><w:tcW w:w="{w}" w:type="dxa"/></w:tcPr>{body}</w:tc>"#);
    let body = format!(
        r#"{fill}<w:tbl><w:tblPr><w:tblW w:w="6000" w:type="dxa"/><w:tblLayout w:type="fixed"/></w:tblPr><w:tblGrid><w:gridCol w:w="3000"/><w:gridCol w:w="3000"/></w:tblGrid><w:tr>{}{}</w:tr></w:tbl><w:p/>"#,
        cell(3000, p("Float", float)),
        cell(3000, p("One", "") + &p("Two", ""))
    );
    let Some(doc) = test_doc(COMPAT_15, ARIAL_12, &body, "") else { return };
    let pages = super::page_chars(&doc);
    let page = |t: &str| pages.iter().position(|pg| pg.iter().map(|c| c.0).collect::<String>().contains(t));
    assert_eq!((page("Float"), page("One"), page("Two")), (Some(1), Some(1), Some(1)));
}

/// Fixed tables of one row each, `cells` (text, width in twips) per table, back to back; `between` is written
/// between them and `tbl_pr` into the second one's properties.
fn back_to_back(tables: &[&[(&str, u32)]], between: &str, tbl_pr: &str) -> String {
    let table = |(i, cells): (usize, &&[(&str, u32)])| {
        let border = r#"w:val="single" w:sz="4" w:space="0" w:color="000000""#;
        let total: u32 = cells.iter().map(|c| c.1).sum();
        let grid: String = cells.iter().map(|c| format!(r#"<w:gridCol w:w="{}"/>"#, c.1)).collect();
        let tcs: String = cells
            .iter()
            .map(|(t, w)| format!(r#"<w:tc><w:tcPr><w:tcW w:w="{w}" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>{t}</w:t></w:r></w:p></w:tc>"#))
            .collect();
        let own = if i > 0 { tbl_pr } else { "" };
        format!(
            r#"<w:tbl><w:tblPr>{own}<w:tblW w:w="{total}" w:type="dxa"/><w:tblBorders><w:top {border}/><w:left {border}/><w:bottom {border}/><w:right {border}/><w:insideH {border}/><w:insideV {border}/></w:tblBorders><w:tblLayout w:type="fixed"/></w:tblPr><w:tblGrid>{grid}</w:tblGrid><w:tr>{tcs}</w:tr></w:tbl>"#
        )
    };
    let body: Vec<String> = tables.iter().enumerate().map(table).collect();
    format!("<w:p><w:r><w:t>before</w:t></w:r></w:p>{}<w:p><w:r><w:t>after</w:t></w:r></w:p>", body.join(between))
}

#[test]
fn tables_with_nothing_between_them_are_one_table_on_the_union_of_their_edges() {
    let body = back_to_back(&[&[("aa", 2000), ("ab", 2000)], &[("ba", 3000), ("bb", 3000)]], "", "");
    let Some(doc) = test_doc(COMPAT_15, ARIAL_12, &body, "") else { return };
    let tables: Vec<_> = doc.sections[0].blocks.iter().filter_map(|b| if let crate::docx::Block::Table(t) = b { Some(t) } else { None }).collect();
    assert_eq!(tables.len(), 1);
    let t = tables[0];
    assert_eq!(t.grid, vec![100.0, 50.0, 50.0, 100.0]);
    let spans = |r: usize| t.rows[r].cells.iter().map(|c| c.span).collect::<Vec<_>>();
    assert_eq!((spans(0), spans(1)), (vec![1, 2], vec![2, 2]));
    assert_eq!(t.width, crate::docx::props::Width::Dxa(200.0), "the first table's width");
    // Laid out as one fixed table 4000 wide: the later row's second cell starts past the first two columns as they
    // shrink to fit, and the rows share their border.
    let l = baselines(COMPAT_15, ARIAL_12, &body).unwrap();
    let dx = at(&l, "bb").0 - at(&l, "ba").0;
    assert!(dx > 95.0 && dx < 110.0, "{dx}");
    let two = baselines(COMPAT_15, ARIAL_12, &back_to_back(&[&[("aa", 2000), ("ab", 2000)], &[("ba", 3000), ("bb", 3000)]], "<w:p/>", "")).unwrap();
    assert!((at(&two, "bb").0 - at(&two, "ba").0 - 150.0).abs() < 0.01);
}

#[test]
fn compat_14_a_later_table_with_a_smaller_left_margin_moves_its_edges_onto_the_earlier_ones() {
    let mar = r#"<w:tblCellMar><w:left w:w="0" w:type="dxa"/><w:right w:w="0" w:type="dxa"/></w:tblCellMar>"#;
    let body = back_to_back(&[&[("aa", 1600), ("ab", 2400)], &[("ba", 1595), ("bb", 2405)]], "", mar);
    let grid = |mode: &str| {
        let settings = COMPAT_15.replace(r#"w:val="15""#, &format!(r#"w:val="{mode}""#));
        let doc = test_doc(&settings, ARIAL_12, &body, "")?;
        let crate::docx::Block::Table(t) = doc.sections[0].blocks.iter().find(|b| matches!(b, crate::docx::Block::Table(_)))? else { return None };
        Some(t.grid.clone())
    };
    let Some(c15) = grid("15") else { return };
    // The later table pulls 5 less to the left (its margin 0, the first one's 10, plus half its border 5): its first
    // cell's end moves the 5 onto the first table's 1600 edge in compat 12; compat 15 keeps its own edges.
    assert_eq!(c15, vec![79.75, 0.25, 120.0]);
    assert_eq!(grid("12"), Some(vec![80.0, 120.0, 0.25]));
}

#[test]
fn a_floating_or_differently_directed_table_stays_apart_from_the_one_before() {
    let tables = |pr: &str| {
        let body = back_to_back(&[&[("aa", 2000)], &[("ba", 3000)]], "", pr);
        test_doc(COMPAT_15, ARIAL_12, &body, "").map(|d| d.sections[0].blocks.iter().filter(|b| matches!(b, crate::docx::Block::Table(_))).count())
    };
    let Some(plain) = tables("") else { return };
    assert_eq!(plain, 1);
    assert_eq!(tables(r#"<w:tblpPr w:vertAnchor="text" w:tblpY="200"/>"#), Some(2));
    assert_eq!(tables("<w:bidiVisual/>"), Some(2));
    // Two floating tables join when their positions are the same.
    let floats = |second: &str| {
        let first = r#"<w:tblpPr w:vertAnchor="text" w:tblpX="200" w:tblpY="200"/>"#;
        let body = back_to_back(&[&[("aa", 2000)], &[("ba", 2000)]], "", second).replacen("<w:tblPr>", &format!("<w:tblPr>{first}"), 1);
        test_doc(COMPAT_15, ARIAL_12, &body, "").map(|d| d.sections[0].blocks.iter().filter(|b| matches!(b, crate::docx::Block::Table(_))).count())
    };
    assert_eq!(floats(r#"<w:tblpPr w:vertAnchor="text" w:tblpX="200" w:tblpY="200"/>"#), Some(1));
    assert_eq!(floats(r#"<w:tblpPr w:vertAnchor="text" w:tblpX="5000" w:tblpY="200"/>"#), Some(2));
}

#[test]
fn tables_read_as_one_save_as_one() {
    let body = back_to_back(&[&[("aa", 2000), ("ab", 2000)], &[("ba", 3000), ("bb", 3000)]], "", "");
    let w = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
    let doc = format!(r#"<w:document xmlns:w="{w}"><w:body>{body}<w:sectPr/></w:body></w:document>"#);
    let rels = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"/>"#;
    let bytes = super::package_bytes(&[("word/document.xml", &doc), ("word/_rels/document.xml.rels", rels)]).unwrap();
    let saved = crate::docx::write::save(&crate::docx::read_editable(&bytes).expect("reads")).expect("saves");
    let mut xml = String::new();
    std::io::Read::read_to_string(&mut zip::ZipArchive::new(std::io::Cursor::new(&saved[..])).unwrap().by_name("word/document.xml").unwrap(), &mut xml)
        .unwrap();
    assert_eq!(xml.matches("<w:tbl>").count(), 1, "{xml}");
    let grid: Vec<&str> = xml.split(r#"<w:gridCol w:w=""#).skip(1).map(|s| &s[..s.find('"').unwrap()]).collect();
    assert_eq!(grid, ["2000", "1000", "1000", "2000"]);
    assert_eq!(xml.matches(r#"<w:gridSpan w:val="2"/>"#).count(), 3, "{xml}");
    let again = crate::docx::read_editable(&saved).expect("reads again");
    assert!(matches!(&again.sections[0].blocks[1], crate::docx::Block::Table(t) if t.rows.len() == 2 && t.grid.len() == 4));
}

#[test]
fn a_later_tables_alignment_and_margins_are_its_rows_exceptions_and_another_style_keeps_it_apart() {
    let w = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
    let styles = format!(r#"<w:styles xmlns:w="{w}"><w:style w:type="table" w:styleId="Grid"><w:name w:val="Grid"/></w:style></w:styles>"#);
    let rels = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"#;
    let read = |second: &str| {
        let body = back_to_back(&[&[("aa", 2000)], &[("ba", 2000)]], "", second).replacen("<w:tblPr>", r#"<w:tblPr><w:tblStyle w:val="Grid"/>"#, 1);
        let doc = format!(r#"<w:document xmlns:w="{w}"><w:body>{body}<w:sectPr/></w:body></w:document>"#);
        let bytes = super::package_bytes(&[("word/document.xml", &doc), ("word/styles.xml", &styles), ("word/_rels/document.xml.rels", rels)]).unwrap();
        crate::docx::read_editable(&bytes).expect("reads")
    };
    let tables = |d: &crate::docx::Document| d.sections[0].blocks.iter().filter(|b| matches!(b, crate::docx::Block::Table(_))).count();
    assert_eq!(tables(&read("")), 2, "the second has no style");
    let mar = r#"<w:tblCellMar><w:left w:w="500" w:type="dxa"/></w:tblCellMar>"#;
    let doc = read(&format!(r#"<w:tblStyle w:val="Grid"/><w:jc w:val="center"/>{mar}"#));
    assert_eq!(tables(&doc), 1);
    let Some(crate::docx::Block::Table(t)) = doc.sections[0].blocks.get(1) else { panic!("a table") };
    assert_eq!((t.rows[0].jc, t.rows[1].jc), (None, Some(crate::docx::props::Jc::Center)));
    assert_eq!((t.rows[0].cells[0].margins[1], t.rows[1].cells[0].margins[1]), (0.0, 25.0), "{:?}", t.rows[1].cells[0].margins);
    let saved = crate::docx::write::save(&doc).expect("saves");
    let mut xml = String::new();
    std::io::Read::read_to_string(&mut zip::ZipArchive::new(std::io::Cursor::new(&saved[..])).unwrap().by_name("word/document.xml").unwrap(), &mut xml)
        .unwrap();
    assert_eq!(xml.matches("<w:tbl>").count(), 1, "{xml}");
    assert!(xml.contains(&format!(r#"<w:tblPrEx><w:jc w:val="center"/>{mar}</w:tblPrEx>"#)), "{xml}");
    let again = crate::docx::read_editable(&saved).expect("reads again");
    let Some(crate::docx::Block::Table(u)) = again.sections[0].blocks.get(1) else { panic!("a table") };
    assert_eq!((u.rows[1].jc, u.rows[1].cells[0].margins), (t.rows[1].jc, t.rows[1].cells[0].margins));
}

#[test]
fn a_float_below_the_text_before_its_paragraph_doesnt_wrap_that_text_when_the_page_is_filled_again() {
    // The first fill lays the first paragraph out without its own float, so the second paragraph's float starts
    // higher there than where it ends up; it met no earlier text, so the next fill doesn't wrap the first paragraph
    // around it, and the second paragraph keeps its place on the page.
    let c15 = r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat>"#;
    let float = |cy: u32| {
        format!(
            r#"<w:r><w:drawing><wp:anchor distT="0" distB="0" distL="0" distR="0" simplePos="0" relativeHeight="0" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="column"><wp:posOffset>0</wp:posOffset></wp:positionH><wp:positionV relativeFrom="paragraph"><wp:posOffset>0</wp:posOffset></wp:positionV><wp:extent cx="1270000" cy="{cy}"/><wp:wrapSquare wrapText="bothSides"/><a:graphic><a:graphicData><pic:pic><pic:blipFill/></pic:pic></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r>"#
        )
    };
    let body = format!(
        r#"<w:p><w:pPr><w:spacing w:before="0" w:after="0" w:line="8560" w:lineRule="exact"/></w:pPr></w:p><w:p><w:pPr><w:spacing w:before="0"/></w:pPr>{}<w:r><w:t>Short words. {}</w:t></w:r></w:p><w:p>{}<w:r><w:t>Beside the second float.</w:t></w:r></w:p>"#,
        float(762000),
        "x".repeat(65),
        float(1524000)
    );
    let Some(l) = page_baselines(c15, ARIAL_12, &body, 0) else { return };
    let long = l.iter().find(|x| x.1.starts_with("xxx")).expect("the long word");
    assert!((long.3 - 560.0 - 10.86).abs() < 1.0, "{l:?}");
    assert!(l.iter().any(|x| x.1.starts_with("Beside")), "{l:?}");
}

#[test]
fn a_wrap_polygon_cuts_a_line_over_its_text_not_the_room_its_spacing_adds_below() {
    let triangle = r#"<wp:start x="0" y="0"/><wp:lineTo x="0" y="21600"/><wp:lineTo x="21600" y="21600"/><wp:lineTo x="0" y="0"/>"#;
    let float = format!(
        r#"<w:r><w:drawing><wp:anchor distT="0" distB="0" distL="0" distR="0" simplePos="0" relativeHeight="0" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="column"><wp:posOffset>0</wp:posOffset></wp:positionH><wp:positionV relativeFrom="paragraph"><wp:posOffset>0</wp:posOffset></wp:positionV><wp:extent cx="1828800" cy="1828800"/><wp:wrapTight wrapText="bothSides"><wp:wrapPolygon edited="1">{triangle}</wp:wrapPolygon></wp:wrapTight><a:graphic><a:graphicData><pic:pic><pic:blipFill/></pic:pic></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r>"#
    );
    let p = format!(r#"<w:p><w:pPr><w:spacing w:line="480" w:lineRule="auto"/></w:pPr>{float}<w:r><w:t>{}</w:t></w:r></w:p>"#, "Alpha bravo charlie delta echo. ".repeat(6));
    let c15 = r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat>"#;
    let Some(c12) = lines("", ARIAL_12, &p) else { return };
    let c15 = lines(c15, ARIAL_12, &p).unwrap();
    // The triangle's edge runs down at 45°: the first line starts where it is at the text's bottom (13.4pt down), not
    // at the bottom of the double-spaced line (26.8pt down).
    for l in [&c12, &c15] {
        assert!(l[0].0 > 72.0 + 13.0 && l[0].0 < 72.0 + 14.5, "{l:?}");
    }
}

#[test]
fn a_compat_15_line_meets_a_float_over_its_text_only_and_compat_14_over_its_whole_height() {
    // A double-spaced line: its text is 13.4pt tall, the line 26.8pt; the float starts 20pt down, in the room the
    // spacing adds below the first line's text.
    let float = r#"<w:r><w:drawing><wp:anchor distT="0" distB="0" distL="0" distR="0" simplePos="0" relativeHeight="0" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="column"><wp:posOffset>0</wp:posOffset></wp:positionH><wp:positionV relativeFrom="paragraph"><wp:posOffset>254000</wp:posOffset></wp:positionV><wp:extent cx="1270000" cy="1270000"/><wp:wrapSquare wrapText="bothSides"/><a:graphic><a:graphicData><pic:pic><pic:blipFill/></pic:pic></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r>"#;
    let p = format!(r#"<w:p><w:pPr><w:spacing w:line="480" w:lineRule="auto"/></w:pPr>{float}<w:r><w:t>{}</w:t></w:r></w:p>"#, "Alpha bravo charlie delta echo. ".repeat(6));
    let c15 = r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat>"#;
    let Some(c12) = lines("", ARIAL_12, &p) else { return };
    let c15 = lines(c15, ARIAL_12, &p).unwrap();
    assert!((c12[0].0 - 172.0).abs() < 0.5 && (c15[0].0 - 72.0).abs() < 0.5 && (c15[1].0 - 172.0).abs() < 0.5, "{c12:?} {c15:?}");
}

#[test]
fn a_compat_15_heading_between_floating_tables_meets_the_second_over_its_text_only() {
    // Each table is placed at the top of the text, a little above where it stands: the second starts in the room the
    // heading's spacing adds below its text, which the heading's line doesn't meet.
    let table = |t: &str| {
        format!(
            r#"<w:tbl><w:tblPr><w:tblpPr w:vertAnchor="text" w:tblpYSpec="top" w:tblOverlap="never"/><w:tblW w:w="9000" w:type="dxa"/><w:tblLayout w:type="fixed"/></w:tblPr><w:tblGrid><w:gridCol w:w="9000"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w="9000" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>{t}</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#
        )
    };
    let body = format!(
        r#"{}<w:p><w:pPr><w:spacing w:before="240" w:after="0" w:line="360" w:lineRule="auto"/></w:pPr><w:r><w:t>Heading</w:t></w:r></w:p>{}<w:p/>"#,
        table("first"),
        table("second")
    );
    let c15 = r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat>"#;
    let Some(l) = baselines(c15, ARIAL_12, &body) else { return };
    let y = |t: &str| l.iter().find(|l| l.1 == t).expect("drawn").3;
    assert!(y("Heading") < y("second"), "{l:?}");
}

fn mode(m: u32) -> String {
    format!(r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="{m}"/></w:compat>"#)
}

/// Line 1 of `x` × 78, a space, then `AB`, `mid`, `cdefghij end`: `AB` and what follows `mid` don't both fit.
fn first_line(m: u32, mid: &str) -> Option<String> {
    let body = format!(r#"<w:p><w:r><w:t xml:space="preserve">{} AB</w:t></w:r>{mid}<w:r><w:t xml:space="preserve">cdefghij end</w:t></w:r></w:p>"#, "x".repeat(78));
    Some(lines(&mode(m), ARIAL_11, &body)?.swap_remove(0).1)
}

#[test]
fn a_line_breaks_after_dashes_and_break_spaces_only() {
    let t = |c: char| format!("<w:r><w:t>{c}</w:t></w:r>");
    let Some(_) = first_line(12, "") else { return };
    for m in [12, 14, 15] {
        // The four-per-em and zero-width spaces may hang past the line's end.
        for c in ['-', '\u{2013}', '\u{2014}', '\u{2005}', '\u{200B}'] {
            assert!(first_line(m, &t(c)).unwrap().trim_end_matches(c).ends_with("AB"), "c{m} U+{:04X}", c as u32);
        }
        for c in ['\u{2010}', '\u{2012}'] {
            assert_eq!(first_line(m, &t(c)).unwrap().ends_with(&format!("AB{c}")), m >= 14, "c{m} U+{:04X}", c as u32);
        }
        for c in ['/', '?', '\u{2011}', '\u{2212}', '\u{AD}'] {
            assert!(first_line(m, &t(c)).unwrap().ends_with('x'), "c{m} U+{:04X}", c as u32);
        }
        assert!(first_line(m, "<w:r><w:softHyphen/></w:r>").unwrap().ends_with("AB-"), "c{m} optional hyphen");
    }
}

#[test]
fn a_hyphen_after_balanced_spaces_breaks_with_punctuation_compressed() {
    let settings = r#"<w:compat><w:balanceSingleByteDoubleByteWidth/><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat>"#;
    let rpr = r#"<w:rFonts w:ascii="Arial" w:hAnsi="Arial" w:eastAsiaTheme="minorEastAsia"/><w:b/><w:sz w:val="20"/>"#;
    let body = r#"<w:tbl><w:tblPr><w:tblW w:w="2340" w:type="dxa"/><w:tblLayout w:type="fixed"/><w:tblCellMar><w:left w:w="108" w:type="dxa"/><w:right w:w="108" w:type="dxa"/></w:tblCellMar></w:tblPr><w:tblGrid><w:gridCol w:w="2340"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w="2340" w:type="dxa"/></w:tcPr><w:p><w:r><w:t xml:space="preserve">ID#: QZ      -7654321</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p/>"#;
    let Some(l) = lines(settings, rpr, body) else { return };
    assert!(l[0].1.ends_with('-') && l[1].1 == "7654321", "{l:?}");
    let l = lines(&settings.replace(r#"w:val="15""#, r#"w:val="12""#), rpr, &body.replace("2340", "1700")).unwrap();
    assert!(l[0].1.ends_with("QZ"), "compat 12 breaks before the hyphen: {l:?}");
}

#[test]
fn a_leading_page_break_starts_the_paragraph_again_after_it() {
    let text = "Several quiet lanterns glowed beside the harbour while patient workers mended nets. ".repeat(3);
    let numbering = r#"<w:abstractNum w:abstractNumId="0"><w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="decimal"/><w:lvlText w:val="%1."/><w:lvlJc w:val="left"/></w:lvl></w:abstractNum><w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num>"#;
    let body = |num: &str, lead: &str| {
        format!(r#"<w:p><w:pPr><w:ind w:left="1440" w:hanging="720"/>{num}</w:pPr><w:r>{lead}<w:t>{text}</w:t></w:r></w:p>"#)
    };
    let num = r#"<w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr>"#;
    let Some(plain) = numbered_page("", ARIAL_11, &body("", r#"<w:br w:type="page"/>"#), "", 1) else { return };
    assert!((plain[0].0 - 108.0).abs() < 0.01 && (plain[1].0 - 144.0).abs() < 0.01, "{plain:?}");
    let numbered = numbered_page("", ARIAL_11, &body(num, r#"<w:br w:type="page"/>"#), numbering, 1).unwrap();
    assert!(numbered[0].1.starts_with("1.") && (numbered[0].0 - 108.0).abs() < 0.01, "{numbered:?}");
    // A line break doesn't start it again.
    let line = numbered_page("", ARIAL_11, &body("", "<w:br/>"), "", 0).unwrap();
    assert!((line[0].0 - 144.0).abs() < 0.01, "{line:?}");
}

#[test]
fn compat_15_takes_an_empty_spec_vanish_paragraphs_alignment_and_indents() {
    let body = |spec: &str| {
        format!(
            r#"<w:p><w:pPr><w:ind w:left="720"/><w:jc w:val="center"/><w:rPr><w:vanish/>{spec}</w:rPr></w:pPr></w:p><w:p><w:pPr><w:ind w:left="2160"/><w:jc w:val="right"/></w:pPr><w:r><w:t>One line.</w:t></w:r></w:p>"#
        )
    };
    let x = |m: u32, spec: &str| baselines(&mode(m), ARIAL_11, &body(spec)).map(|l| (l[0].0, l[0].2));
    let Some(right) = x(15, "") else { return };
    let centred = x(15, "<w:specVanish/>").unwrap();
    // Centred between 108 and 540: the line as wide as the right-aligned one, which ends at 540.
    assert!((centred.0 - (324.0 - (540.0 - right.0) / 2.0)).abs() < 0.5, "{centred:?} {right:?}");
    assert_eq!(x(12, "<w:specVanish/>").unwrap(), right, "compat 12 skips it");
}

#[test]
fn small_capitals_take_their_runs_own_size_in_the_line() {
    let p = |rpr: &str, t: &str| format!(r#"<w:p><w:pPr><w:spacing w:after="0"/></w:pPr><w:r><w:rPr>{rpr}</w:rPr><w:t>{t}</w:t></w:r></w:p>"#);
    let body = [p("", "Plain"), p("<w:smallCaps/>", "lower only words"), p("", "Plain")].concat();
    let Some(l) = baselines("", ARIAL_12, &body) else { return };
    assert!((l[1].3 - l[0].3 - (l[2].3 - l[1].3)).abs() < 0.01 && l[2].3 - l[1].3 > 13.0, "{l:?}");
}

#[test]
fn compat_14_balanced_columns_grow_by_the_failed_fills_mean_line_and_end_at_the_height_tried() {
    // Seven one-line paragraphs, P2 with 300 tw after and P5 with 500: from half the content the height grows by the
    // first fill's mean line to 5 / 2, and the next section starts at that height, inside P5's space after. Compat 15
    // keeps the tallest column: 4 / 3.
    let sect = |cols: &str| {
        format!(
            r#"<w:sectPr><w:type w:val="continuous"/><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/>{cols}</w:sectPr>"#
        )
    };
    let after = |k: usize| match k {
        2 => r#"<w:spacing w:after="300"/>"#,
        5 => r#"<w:spacing w:after="500"/>"#,
        _ => "",
    };
    let paras: String = (1..=7)
        .map(|k| {
            let s = if k == 7 { sect(r#"<w:cols w:num="2" w:space="720"/>"#) } else { String::new() };
            format!("<w:p><w:pPr>{}{s}</w:pPr><w:r><w:t>P{k}</w:t></w:r></w:p>", after(k))
        })
        .collect();
    let body = format!(r#"{paras}<w:p><w:pPr>{}</w:pPr><w:r><w:t>After</w:t></w:r></w:p>"#, sect(""));
    let c15 = r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat>"#;
    let split = |settings: &str| {
        let l = baselines(settings, ARIAL_12, &body)?;
        let y = |t: &str| l.iter().find(|x| x.1 == t).map(|x| x.3);
        let line = y("P4")? - y("P3")?;
        let first_right = l.iter().filter(|x| x.0 > 200.0).map(|x| x.1.clone()).next()?;
        Some((first_right, (y("After")? - y("P1")?) / line))
    };
    let Some((right, h)) = split("") else { return };
    // In lines: half of 7 lines and 800 tw, then a sixth of the failed fill's two columns.
    assert_eq!(right, "P6");
    assert!(h > 6.4 && h < 6.6, "{h}");
    assert_eq!(split(c15).map(|s| s.0).as_deref(), Some("P5"));
}

#[test]
fn compat_15_balanced_columns_start_from_the_weighted_height_unrounded() {
    // Exact lines of 12, 12, 15 and 15pt in two columns: from 27pt the fill fails and grows by 12pt to 39pt, which
    // holds three lines in the first column. Rounded down to a whole line first (24pt), it would grow to 36pt: 2 / 2.
    let sect = |cols: &str| {
        format!(
            r#"<w:sectPr><w:type w:val="continuous"/><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/>{cols}</w:sectPr>"#
        )
    };
    let paras: String = [240, 240, 300, 300]
        .iter()
        .enumerate()
        .map(|(k, line)| {
            let s = if k == 3 { sect(r#"<w:cols w:num="2" w:space="720"/>"#) } else { String::new() };
            format!(r#"<w:p><w:pPr><w:spacing w:after="0" w:line="{line}" w:lineRule="exact"/>{s}</w:pPr><w:r><w:t>P{}</w:t></w:r></w:p>"#, k + 1)
        })
        .collect();
    let body = format!(r#"{paras}<w:p><w:pPr>{}</w:pPr><w:r><w:t>After</w:t></w:r></w:p>"#, sect(""));
    let c15 = r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat>"#;
    let Some(l) = baselines(c15, ARIAL_12, &body) else { return };
    let right: Vec<&str> = l.iter().filter(|x| x.0 > 200.0).map(|x| x.1.as_str()).collect();
    assert_eq!(right, ["P4"], "{l:?}");
}

/// A legacy form field as Word writes it: `w:ffData` in its begin, no result.
fn form_field(ff: &str, code: &str) -> String {
    format!(
        r#"<w:r><w:fldChar w:fldCharType="begin"><w:ffData><w:name w:val="F1"/><w:enabled/>{ff}</w:ffData></w:fldChar></w:r><w:r><w:instrText xml:space="preserve"> {code} </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r>"#
    )
}

#[test]
fn a_check_box_is_wide_and_tall_and_the_word_after_it_breaks_away() {
    if !fonts().is_installed("Arial") {
        return;
    }
    let cb = form_field(r#"<w:checkBox><w:size w:val="24"/><w:default w:val="0"/></w:checkBox>"#, "FORMCHECKBOX");
    let p = format!(r#"<w:p><w:r><w:t>{}</w:t></w:r>{cb}<w:r><w:t>yyyy</w:t></w:r></w:p>"#, "x".repeat(79));
    let Some(l) = baselines("", ARIAL_11, &p) else { return };
    assert_eq!(l.iter().map(|l| l.1.as_str()).collect::<Vec<_>>(), ["x".repeat(79).as_str(), "yyyy"], "{l:?}");
    // The box's line is Arial's at 12 pt.
    let p = format!(r#"<w:p><w:r><w:t>xx</w:t><w:br/><w:t>xx</w:t></w:r>{cb}<w:r><w:br/><w:t>xx</w:t></w:r></w:p>"#);
    let Some(l) = baselines("", ARIAL_11, &p) else { return };
    assert!((l[1].3 - l[0].3 - 13.56).abs() < 0.1, "{l:?}");
}

#[test]
fn a_drop_down_shows_its_selected_entry_in_its_codes_run() {
    if !fonts().is_installed("Arial") {
        return;
    }
    let dd = form_field(r#"<w:ddList><w:result w:val="1"/><w:listEntry w:val="Alpha"/><w:listEntry w:val="Bravo"/></w:ddList>"#, "FORMDROPDOWN");
    let p = format!(r#"<w:p><w:r><w:t>xxxxx</w:t></w:r>{dd}<w:r><w:t>xxxxx</w:t></w:r></w:p>"#);
    let Some(l) = lines("", ARIAL_11, &p) else { return };
    assert_eq!(l[0].1, "xxxxxBravoxxxxx", "{l:?}");
    // Without entries: a blank five digit zeros wide.
    let dd = form_field(r#"<w:ddList><w:result w:val="0"/></w:ddList>"#, "FORMDROPDOWN");
    let p = format!(r#"<w:p><w:r><w:t>xxxxx</w:t></w:r>{dd}<w:r><w:t>xxxxx</w:t></w:r></w:p>"#);
    let Some(l) = lines("", ARIAL_11, &p) else { return };
    assert!((l[0].2 - (72.0 + 27.5 + 30.58)).abs() < 0.1, "{l:?}");
}

#[test]
fn a_page_frame_anchored_after_a_paragraph_wraps_that_paragraphs_last_line() {
    let fr = |w: u32, y: u32| {
        format!(
            r#"<w:p><w:pPr><w:framePr w:w="{w}" w:h="200" w:hRule="exact" w:hSpace="180" w:vSpace="0" w:wrap="around" w:vAnchor="page" w:hAnchor="page" w:x="1440" w:y="{y}"/><w:spacing w:before="0" w:after="0" w:line="200" w:lineRule="exact"/></w:pPr><w:r><w:t>F</w:t></w:r></w:p>"#
        )
    };
    let p = r#"<w:p><w:pPr><w:spacing w:before="0" w:after="0" w:line="300" w:lineRule="exact"/></w:pPr><w:r><w:t>so that every qq</w:t></w:r></w:p>"#;
    let body = format!("{}{p}{}{p}", fr(7923, 1490), fr(7922, 1790));
    let Some(l) = baselines(C15, ARIAL_11, &body) else { return };
    let text: Vec<(f64, &str)> = l.iter().filter(|l| l.1 != "F").map(|l| ((l.0 * 20.0).round() / 20.0, l.1.as_str())).collect();
    // The second frame, anchored in the second paragraph, meets the first one's second line: that line goes right of it.
    assert_eq!(text, [(477.1, "so that"), (477.05, "every qq"), (72.0, "so that every qq")]);
}

/// The float's drawn rect on page 1 and the first baseline of the line starting with `start`.
fn float_and_line(settings: &str, body: &str, start: &str) -> Option<([f64; 4], f64)> {
    fn find(children: &[crate::layout::fragment::Child]) -> Option<[f64; 4]> {
        children.iter().find_map(|c| match c {
            crate::layout::fragment::Child::Box(b) => match b.kind {
                crate::layout::fragment::BoxKind::Float { rect, .. } => Some(rect),
                _ => find(&b.children),
            },
            _ => None,
        })
    }
    let snap = layout(&test_doc(settings, ARIAL_12, body, "")?, &fonts());
    let page = &snap.pages()[0];
    let rect = page.children.iter().find_map(|c| match c {
        crate::layout::fragment::PageChild::Body(b) => find(&b.children),
        _ => None,
    })?;
    let mut y = None;
    paint(page, &mut |p| {
        if let Item::Glyphs(r) = p.absolute()
            && y.is_none()
            && r.chars.iter().map(|c| c.0).collect::<String>().starts_with(start)
        {
            y = Some(r.baseline);
        }
    });
    Some((rect, y?))
}

#[test]
fn a_float_that_met_earlier_text_keeps_that_place_when_the_text_wraps_it() {
    // B anchors a float 100pt above itself, over A's lines. A wraps it and grows, B moves down, the float stays.
    let body = |wrap: &str| {
        format!(
            r#"<w:p><w:r><w:t>{}</w:t></w:r></w:p><w:p><w:r><w:drawing><wp:anchor distT="0" distB="0" distL="0" distR="0" simplePos="0" relativeHeight="0" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="column"><wp:posOffset>0</wp:posOffset></wp:positionH><wp:positionV relativeFrom="paragraph"><wp:posOffset>-1270000</wp:posOffset></wp:positionV><wp:extent cx="1905000" cy="762000"/>{wrap}<a:graphic><a:graphicData><pic:pic><pic:blipFill/></pic:pic></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r><w:r><w:t>Bravo</w:t></w:r></w:p>"#,
            "Alpha words run on. ".repeat(40)
        )
    };
    for compat in [12, 15] {
        let settings = format!(r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="{compat}"/></w:compat>"#);
        let Some((first, b0)) = float_and_line(&settings, &body("<wp:wrapNone/>"), "Bravo") else { return };
        let (kept, b1) = float_and_line(&settings, &body(r#"<wp:wrapSquare wrapText="bothSides"/>"#), "Bravo").unwrap();
        assert!(b1 > b0 + 1.0, "compat {compat}: A wraps the float and pushes B down: {b0} {b1}");
        assert!((kept[1] - first[1]).abs() < 0.01, "compat {compat}: the float stays where it met A: {first:?} {kept:?}");
    }
}

#[test]
fn a_paragraphs_space_before_is_part_of_its_first_lines_box_beside_a_float() {
    // A 23pt float over the first paragraph's line; the second paragraph's 24pt before reaches it, its text doesn't.
    let float = r#"<w:r><w:drawing><wp:anchor distT="0" distB="0" distL="0" distR="0" simplePos="0" relativeHeight="0" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="margin"><wp:posOffset>0</wp:posOffset></wp:positionH><wp:positionV relativeFrom="paragraph"><wp:posOffset>0</wp:posOffset></wp:positionV><wp:extent cx="1270000" cy="292100"/><wp:wrapSquare wrapText="bothSides"/><a:graphic><a:graphicData><pic:pic><pic:blipFill/></pic:pic></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r>"#;
    let body = format!(
        r#"<w:p><w:pPr><w:spacing w:before="0" w:after="0"/></w:pPr>{float}<w:r><w:t>x</w:t></w:r></w:p><w:p><w:pPr><w:spacing w:before="480" w:after="0"/></w:pPr><w:r><w:t>{}</w:t></w:r></w:p>"#,
        "Alpha bravo charlie delta echo. ".repeat(6)
    );
    for compat in [12, 15] {
        let settings = format!(r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="{compat}"/></w:compat>"#);
        let Some(l) = lines(&settings, ARIAL_12, &body) else { return };
        assert!((l[1].0 - 172.0).abs() < 0.01 && (l[2].0 - 72.0).abs() < 0.01, "compat {compat}: {l:?}");
        // A top-and-bottom float only in the space before doesn't move the line.
        let y = |wrap: &str| {
            let low = body
                .replace(r#"<wp:wrapSquare wrapText="bothSides"/>"#, wrap)
                .replace(r#"<wp:posOffset>0</wp:posOffset></wp:positionV><wp:extent cx="1270000" cy="292100"/>"#, r#"<wp:posOffset>177800</wp:posOffset></wp:positionV><wp:extent cx="1270000" cy="127000"/>"#);
            baselines(&settings, ARIAL_12, &low).unwrap()[1].3
        };
        assert!((y("<wp:wrapTopAndBottom/>") - y("<wp:wrapNone/>")).abs() < 0.01, "compat {compat}");
    }
}

fn anchor(x: u32, y: i64, w: u32, h: u32, overlap: u8, relative: &str) -> String {
    format!(
        r#"<w:r><w:drawing><wp:anchor distT="0" distB="0" distL="0" distR="0" simplePos="0" relativeHeight="0" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="{overlap}"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="column"><wp:posOffset>{}</wp:posOffset></wp:positionH><wp:positionV relativeFrom="{relative}"><wp:posOffset>{}</wp:posOffset></wp:positionV><wp:extent cx="{}" cy="{}"/><wp:wrapSquare wrapText="bothSides"/><a:graphic><a:graphicData><pic:pic><pic:blipFill/></pic:pic></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r>"#,
        x * 12700,
        y * 12700,
        w * 12700,
        h * 12700
    )
}

#[test]
fn compat_15_a_float_with_no_place_sends_its_anchor_line_to_the_next_page() {
    // Two floats that may not overlap and have no place clear of each other: the paragraph after P1 to P3 starts the
    // next page, where its first line goes below both and the page ends after it.
    let short: String = (1..=3).map(|k| format!("<w:p><w:r><w:t>P{k}</w:t></w:r></w:p>")).collect();
    let floats = format!("{}{}", anchor(0, 0, 468, 400, 0, "paragraph"), anchor(0, 50, 80, 300, 0, "paragraph"));
    let body = format!(r#"{short}<w:p>{floats}<w:r><w:t>{}</w:t></w:r></w:p>"#, "Alpha bravo charlie delta echo. ".repeat(20));
    let Some(p1) = page_baselines(C15, ARIAL_12, &body, 0) else { return };
    let p2 = page_baselines(C15, ARIAL_12, &body, 1).unwrap();
    assert_eq!(p1.iter().map(|l| l.1.as_str()).collect::<Vec<_>>(), ["P1", "P2", "P3"]);
    assert!(p2.len() == 1 && p2[0].0 == 72.0 && p2[0].3 > 472.0, "{p2:?}");
    // A float too tall for any page anchored in the third line: the first two lines stay.
    let body = format!(
        r#"<w:p><w:r><w:t>{}</w:t></w:r>{}<w:r><w:t>{}</w:t></w:r></w:p>"#,
        "Alpha bravo charlie delta echo. ".repeat(7),
        anchor(0, 0, 216, 680, 1, "line"),
        "Foxtrot golf hotel india juliet. ".repeat(10)
    );
    let p1 = page_baselines(C15, ARIAL_12, &body, 0).unwrap();
    assert_eq!(p1.len(), 2, "{p1:?}");
}

#[test]
fn compat_14_a_keep_lines_paragraph_that_meets_its_own_float_starts_below_it() {
    let filler: String = (0..10).map(|k| format!("<w:p><w:r><w:t>A{k:02} filler line text</w:t></w:r></w:p>")).collect();
    let words = |c: char, n: usize| (0..n).map(|k| format!("{c}{k:02} ")).collect::<String>();
    let body = |keep: &str| {
        format!(
            r#"{filler}<w:p><w:pPr>{keep}</w:pPr><w:r><w:t xml:space="preserve">{}</w:t></w:r>{}<w:r><w:t>{}</w:t></w:r></w:p><w:p><w:r><w:t>Z00</w:t></w:r></w:p>"#,
            words('B', 90),
            anchor(368, 0, 100, 20, 1, "line"),
            words('b', 30)
        )
    };
    let Some((f, plain)) = float_and_line("", &body(""), "B00") else { return };
    let (kept, b) = float_and_line("", &body("<w:keepLines/>"), "B00").unwrap();
    assert!(plain < f[1], "without keepLines the paragraph stays above its float: {plain} {f:?}");
    assert!(b > kept[3], "with keepLines it starts below it: {b} {kept:?}");
    let Some((_, c15)) = float_and_line(C15, &body("<w:keepLines/>"), "B00") else { return };
    assert!((c15 - float_and_line(C15, &body(""), "B00").unwrap().1).abs() < 0.01, "compat 15 wraps it as usual");
}

#[test]
fn a_float_wraps_the_next_continuous_section_on_its_page() {
    // Section 1 ends with a 150pt float at the column's left; section 2 starts beside it.
    let body = format!(
        r#"<w:p><w:r><w:t>S1</w:t></w:r>{}</w:p><w:p><w:pPr><w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr></w:pPr></w:p><w:p><w:r><w:t>{}</w:t></w:r></w:p>"#,
        anchor(0, 20, 150, 160, 1, "paragraph"),
        "Section two runs on. ".repeat(30)
    );
    for compat in [12, 15] {
        let settings = format!(r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="{compat}"/></w:compat>"#);
        let Some(mut doc) = test_doc(&settings, ARIAL_12, &body, "") else { return };
        doc.sections[1].break_kind = crate::docx::SectionBreak::Continuous;
        let snap = layout(&doc, &fonts());
        let mut first = None;
        paint(&snap.pages()[0], &mut |p| {
            if let Item::Glyphs(r) = p.absolute()
                && first.is_none()
                && r.chars.first().is_some_and(|c| c.0 == 'S')
                && r.chars.get(1).is_some_and(|c| c.0 == 'e')
            {
                first = Some(r.x);
            }
        });
        assert!(first.is_some_and(|x| (x - 222.0).abs() < 0.01), "compat {compat}: {first:?}");
    }
}

#[test]
fn compat_15_a_float_taller_than_the_page_ends_the_page_after_its_one_line_paragraph() {
    let body = format!(r#"<w:p>{}<w:r><w:t>Cj</w:t></w:r></w:p><w:p><w:r><w:t>After Cj.</w:t></w:r></w:p>"#, anchor(0, 0, 200, 740, 1, "paragraph"));
    let Some(p1) = page_baselines(C15, ARIAL_12, &body, 0) else { return };
    let p2 = page_baselines(C15, ARIAL_12, &body, 1).unwrap();
    assert_eq!((p1.len(), p2[0].1.as_str()), (1, "After Cj."), "{p1:?} {p2:?}");
}

#[test]
fn a_first_line_moved_below_a_top_and_bottom_float_keeps_its_share_of_the_space_before() {
    // A 60pt picture 20pt below its paragraph's top; the paragraph's first line goes below it with its share of the gap.
    let pic = |dy: i64, h: i64, wrap: &str| {
        format!(
            r#"<w:r><w:drawing><wp:anchor distT="0" distB="0" distL="0" distR="0" simplePos="0" relativeHeight="0" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="column"><wp:posOffset>0</wp:posOffset></wp:positionH><wp:positionV relativeFrom="paragraph"><wp:posOffset>{}</wp:posOffset></wp:positionV><wp:extent cx="2540000" cy="{}"/>{wrap}<a:graphic><a:graphicData><pic:pic><pic:blipFill/></pic:pic></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r>"#,
            dy * 12700,
            h * 12700
        )
    };
    let body = |after: i64, before: i64, float: &str| {
        format!(
            r#"<w:p><w:pPr><w:spacing w:before="0" w:after="{after}"/></w:pPr><w:r><w:t>Spacer.</w:t></w:r></w:p><w:p><w:pPr><w:spacing w:before="{before}" w:after="0"/></w:pPr>{float}<w:r><w:t>Anchor.</w:t></w:r></w:p>"#
        )
    };
    let tb = "<wp:wrapTopAndBottom/>";
    for compat in [14, 15] {
        let settings = format!(r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="{compat}"/></w:compat>"#);
        let Some(_) = lines(&settings, ARIAL_12, &body(0, 0, "")) else { return };
        let y = |after: i64, before: i64, float: &str| baselines(&settings, ARIAL_12, &body(after, before, float)).unwrap()[1].3;
        // After 10, before 34: the share is 24pt, so the text goes from 24pt to 80 + 24pt below the paragraph's top.
        let d = y(200, 680, &pic(20, 60, tb)) - y(200, 680, "");
        assert!((d - 80.0).abs() < 0.01, "compat {compat}: {d}");
        // A picture of the paragraph's own inside its 36pt space before moves the line: below it plus the 36pt.
        let d = y(0, 720, &pic(6, 12, tb)) - y(0, 720, &pic(6, 12, "<wp:wrapNone/>"));
        assert!((d - 18.0).abs() < 0.01, "compat {compat}: {d}");
    }
}

#[test]
fn a_negative_indent_holds_on_a_line_no_float_meets() {
    // The paragraph anchors a square float 300pt below it: its line, left indent -36pt, starts 36pt left of the margin.
    let float = r#"<w:r><w:drawing><wp:anchor distT="0" distB="0" distL="114300" distR="114300" simplePos="0" relativeHeight="0" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="column"><wp:posOffset>0</wp:posOffset></wp:positionH><wp:positionV relativeFrom="paragraph"><wp:posOffset>3810000</wp:posOffset></wp:positionV><wp:extent cx="1270000" cy="1270000"/><wp:wrapSquare wrapText="bothSides"/><a:graphic><a:graphicData><pic:pic><pic:blipFill/></pic:pic></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r>"#;
    let body = format!(r#"<w:p><w:pPr><w:ind w:left="-720"/></w:pPr>{float}<w:r><w:t>Alpha</w:t></w:r></w:p>"#);
    for compat in [14, 15] {
        let settings = format!(r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="{compat}"/></w:compat>"#);
        let Some(l) = lines(&settings, ARIAL_12, &body) else { return };
        assert!((l[0].0 - 36.0).abs() < 0.01, "compat {compat}: {l:?}");
    }
}

#[test]
fn compat_15_a_floating_tables_top_distance_rewraps_the_line_above_it() {
    // A table in the flow, 10pt top distance: the line before it, within those 10pt, goes beside it in compat 15.
    let body = r#"<w:p><w:r><w:t>Above.</w:t></w:r></w:p><w:tbl><w:tblPr><w:tblpPr w:leftFromText="180" w:rightFromText="180" w:topFromText="200" w:vertAnchor="text" w:horzAnchor="margin" w:tblpX="0" w:tblpY="0"/><w:tblW w:w="3000" w:type="dxa"/><w:tblLayout w:type="fixed"/></w:tblPr><w:tblGrid><w:gridCol w:w="3000"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>cell</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p><w:r><w:t>After.</w:t></w:r></w:p>"#;
    for (compat, beside) in [(14, false), (15, true)] {
        let settings = format!(r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="{compat}"/></w:compat>"#);
        let Some(l) = lines(&settings, ARIAL_12, body) else { return };
        let x = l.iter().find(|l| l.1 == "Above.").expect("the line above").0;
        assert_eq!(x > 200.0, beside, "compat {compat}: {l:?}");
    }
}

#[test]
fn a_first_line_meeting_two_floating_tables_goes_to_the_nearer_bottom() {
    // A one-row table and a three-row one right of it to the column end; the heading after them meets both and goes below the first,
    // beside the second.
    let table = |x: u32, w: u32, rows: &[&str]| {
        let rows: String = rows.iter().map(|t| format!(r#"<w:tr><w:tc><w:tcPr><w:tcW w:w="{w}" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>{t}</w:t></w:r></w:p></w:tc></w:tr>"#)).collect();
        format!(
            r#"<w:tbl><w:tblPr><w:tblpPr w:leftFromText="180" w:rightFromText="180" w:vertAnchor="text" w:horzAnchor="margin" w:tblpX="{x}" w:tblpY="0"/><w:tblW w:w="{w}" w:type="dxa"/><w:tblLayout w:type="fixed"/></w:tblPr><w:tblGrid><w:gridCol w:w="{w}"/></w:tblGrid>{rows}</w:tbl>"#
        )
    };
    let body = format!(
        r#"<w:p><w:r><w:t>Top.</w:t></w:r></w:p>{}{}<w:p><w:pPr><w:spacing w:before="240"/></w:pPr><w:r><w:t>Heading.</w:t></w:r></w:p>"#,
        table(0, 3000, &["A1"]),
        table(3200, 6000, &["B1", "B2", "B3"])
    );
    for compat in [14, 15] {
        let settings = format!(r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="{compat}"/></w:compat>"#);
        let Some(l) = baselines(&settings, ARIAL_12, &body) else { return };
        let y = |t: &str| l.iter().find(|l| l.1 == t).unwrap_or_else(|| panic!("{t}: {l:?}")).3;
        assert!(y("Heading.") < y("B3"), "compat {compat}: {l:?}");
    }
}

/// A bottom-aligned three-row floating table that may not overlap, and a bottom-aligned margin frame with
/// `suppressOverlap` as wide as the column, in the order given; the baselines of the frame's line and of the table's first row.
fn frame_and_table(compat: u32, frame_first: bool) -> Option<(f64, f64)> {
    let frame = r#"<w:p><w:pPr><w:framePr w:w="9360" w:hSpace="181" w:wrap="around" w:hAnchor="margin" w:yAlign="bottom"/><w:suppressOverlap/></w:pPr><w:r><w:t>Framed.</w:t></w:r></w:p>"#;
    let rows: String = (1..=3).map(|i| format!(r#"<w:tr><w:tc><w:tcPr><w:tcW w:w="4000" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>Row{i}</w:t></w:r></w:p></w:tc></w:tr>"#)).collect();
    let table = format!(
        r#"<w:tbl><w:tblPr><w:tblpPr w:leftFromText="181" w:rightFromText="181" w:horzAnchor="margin" w:tblpYSpec="bottom"/><w:tblOverlap w:val="never"/><w:tblW w:w="4000" w:type="dxa"/><w:tblLayout w:type="fixed"/></w:tblPr><w:tblGrid><w:gridCol w:w="4000"/></w:tblGrid>{rows}</w:tbl>"#
    );
    let (a, b) = if frame_first { (frame.to_string(), table) } else { (table, frame.to_string()) };
    let body = format!(r#"<w:p><w:r><w:t>Case.</w:t></w:r></w:p>{a}{b}<w:p><w:r><w:t>After.</w:t></w:r></w:p>"#);
    let settings = format!(r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="{compat}"/></w:compat>"#);
    let l = baselines(&settings, ARIAL_12, &body)?;
    let y = |t: &str| l.iter().find(|l| l.1 == t).unwrap_or_else(|| panic!("{t}: {l:?}")).3;
    Some((y("Framed."), y("Row1")))
}

#[test]
fn a_frame_and_a_floating_table_that_may_not_overlap_the_later_one_goes_above() {
    for compat in [14, 15] {
        // The table first: the frame moves up, onto the table's top.
        let Some((frame, row)) = frame_and_table(compat, false) else { return };
        assert!(frame < row && row > 650.0, "compat {compat}: frame {frame} row {row}");
        // The frame first, before the table: it stays at the bottom and the table goes above it.
        let (frame, row) = frame_and_table(compat, true).unwrap();
        assert!(row < frame && frame > 690.0, "compat {compat}: frame {frame} row {row}");
    }
}
