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
fn test_doc(settings: &str, rpr: &str, body: &str, numbering: &str) -> Option<crate::docx::Document> {
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
fn a_floating_tables_obstacle_sits_on_device_pixels() {
    let body = r#"<w:tbl><w:tblPr><w:tblpPr w:vertAnchor="text" w:horzAnchor="margin" w:tblpX="1000" w:tblpY="0"/><w:tblW w:w="3000" w:type="dxa"/><w:tblBorders><w:left w:val="single" w:sz="4" w:space="0" w:color="000000"/><w:right w:val="single" w:sz="4" w:space="0" w:color="000000"/></w:tblBorders><w:tblLayout w:type="fixed"/></w:tblPr><w:tblGrid><w:gridCol w:w="3000"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>cell</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p><w:r><w:t>Left words beside the table on the right side.</w:t></w:r></w:p>"#;
    let Some(l) = lines("", ARIAL_12, body) else { return };
    let right = l.iter().find(|(x, t, _)| *x > 200.0 && t != "cell").expect("text right of the table");
    assert!((right.0 - 272.2).abs() < 0.01, "{l:?}");
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
fn a_page_break_in_a_table_cell_is_nothing() {
    let body = r#"<w:tbl><w:tblGrid><w:gridCol w:w="4000"/></w:tblGrid><w:tr><w:tc><w:p><w:r><w:t>Before</w:t><w:br w:type="page"/><w:t>After</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p/>"#;
    let Some(l) = lines("", ARIAL_12, body) else { return };
    assert_eq!(l[0].1, "BeforeAfter", "{l:?}");
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
fn a_fitted_region_never_breaks() {
    // Wider than the line: on a line of its own (the bar glued to it broken off), past the margin.
    let c = glyph_xs("", &fitted(12000, "abc def ghi")).expect("lays out");
    let a = c.iter().find(|g| g.0 == 'a').expect("a");
    let end = c.iter().rfind(|g| g.0 == '|').expect("|end");
    assert!(c[0].2 < a.2 && a.2 < end.2, "{:?} {a:?} {end:?}", c[0]);
    assert!(c.iter().filter(|g| g.2 == a.2).map(|g| g.1).fold(0.0, f64::max) > 540.0);
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
