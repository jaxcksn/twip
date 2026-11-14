//! Borders: text boxes, and paragraph borders in every style.

use super::shapes::bytes;
use crate::layout::fragment::Item;

fn rects(body: &str) -> Vec<([f64; 4], u32)> {
    let doc = crate::docx::read(&bytes(body).expect("a package")).expect("reads");
    let snap = crate::layout::layout(&doc, &super::fonts());
    crate::layout::traverse::paint_items(&snap.pages()[0])
        .into_iter()
        .filter_map(|i| match i {
            Item::Rect { rect, colour } => Some((rect, colour)),
            _ => None,
        })
        .collect()
}

#[test]
fn a_text_border_boxes_its_runs_once() {
    let bdr = r#"<w:rPr><w:bdr w:val="single" w:sz="8" w:space="0" w:color="C00000"/></w:rPr>"#;
    let body = format!(
        r#"<w:p><w:r><w:t xml:space="preserve">Plain </w:t></w:r><w:r>{bdr}<w:t xml:space="preserve">boxed </w:t></w:r><w:r>{bdr}<w:t>text</w:t></w:r><w:r><w:t xml:space="preserve"> after.</w:t></w:r></w:p>"#
    );
    let red: Vec<_> = rects(&body).into_iter().filter(|(_, c)| *c == 0xC00000).collect();
    assert_eq!(red.len(), 4, "one box of four edges around both runs: {red:?}");
    let top = red.iter().find(|(r, _)| r[2] - r[0] > 10.0).expect("the top edge").0;
    assert!((top[3] - top[1] - 1.0).abs() < 1e-6, "a 1 pt line");
}

#[test]
fn compat_12_joined_paragraphs_border_each_line_by_its_first_characters_paragraph() {
    let ppr = |boxed: bool, vanish: bool| {
        let bdr = r#"<w:pBdr><w:top w:val="single" w:sz="8" w:space="0" w:color="C00000"/><w:left w:val="single" w:sz="8" w:space="0" w:color="C00000"/><w:bottom w:val="single" w:sz="8" w:space="0" w:color="C00000"/><w:right w:val="single" w:sz="8" w:space="0" w:color="C00000"/></w:pBdr><w:shd w:val="clear" w:color="auto" w:fill="D9D9D9"/>"#;
        let mark = if vanish { "<w:rPr><w:vanish/><w:specVanish/></w:rPr>" } else { "" };
        format!("<w:pPr>{}{mark}</w:pPr>", if boxed { bdr } else { "" })
    };
    // Lines: `OneTwo` (the first paragraph's), `ThreeFour` (the second's, plain), `Five` (the third's).
    let body = format!(
        "<w:p>{}<w:r><w:t>One</w:t></w:r></w:p><w:p>{}<w:r><w:t>Two</w:t><w:br/><w:t>Three</w:t></w:r></w:p><w:p>{}<w:r><w:t>Four</w:t><w:br/><w:t>Five</w:t></w:r></w:p>",
        ppr(true, true),
        ppr(false, true),
        ppr(true, false)
    );
    let r = rects(&body);
    let shaded: Vec<[f64; 4]> = r.iter().filter(|(_, c)| *c == 0xD9D9D9).map(|(r, _)| *r).collect();
    let rules: Vec<[f64; 4]> = r.iter().filter(|(r, c)| *c == 0xC00000 && r[2] - r[0] > 100.0).map(|(r, _)| *r).collect();
    assert_eq!(shaded.len(), 2, "the first and last lines: {shaded:?}");
    assert!(shaded[1][1] - shaded[0][3] > 10.0, "the middle line is plain: {shaded:?}");
    // One top rule above the first line, one bottom rule below the last.
    assert_eq!(rules.len(), 2, "{rules:?}");
    assert!(rules[0][3] <= shaded[0][1] + 1e-6 && rules[1][1] >= shaded[1][3] - 1e-6, "{rules:?} {shaded:?}");
}

#[test]
fn paragraph_borders_draw_their_style() {
    let para = |style: &str| {
        format!(
            r#"<w:p><w:pPr><w:pBdr><w:top w:val="{style}" w:sz="8" w:space="1" w:color="2F5496"/></w:pBdr></w:pPr><w:r><w:t>Text</w:t></w:r></w:p>"#
        )
    };
    let count = |style: &str| rects(&para(style)).into_iter().filter(|(_, c)| *c == 0x2F5496).count();
    assert_eq!(count("single"), 1);
    assert_eq!(count("double"), 2);
    assert_eq!(count("triple"), 3);
    assert_eq!(count("thinThickSmallGap"), 2);
    assert!(count("dotted") > 10, "dots");
    assert!(count("wave") > 10, "a wave");
}

fn page_rects(sect: &str, paras: usize) -> Vec<Vec<([f64; 4], u32)>> {
    let ct = r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#;
    let rels = r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
    let body: String = (0..paras).map(|_| r#"<w:p><w:r><w:t>Text</w:t></w:r></w:p><w:p><w:r><w:br w:type="page"/></w:r></w:p>"#).collect();
    let document = format!(
        r#"<?xml version="1.0"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{body}<w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/>{sect}</w:sectPr></w:body></w:document>"#
    );
    let doc = crate::docx::read(
        &super::package_bytes(&[("[Content_Types].xml", ct), ("_rels/.rels", rels), ("word/document.xml", &document)]).expect("a package"),
    )
    .expect("reads");
    let snap = crate::layout::layout(&doc, &super::fonts());
    snap.pages()
        .iter()
        .map(|p| {
            crate::layout::traverse::paint_items(p)
                .into_iter()
                .filter_map(|i| match i {
                    Item::Rect { rect, colour } if colour == 0x7030A0 => Some((rect, colour)),
                    _ => None,
                })
                .collect()
        })
        .collect()
}

#[test]
fn page_borders_draw_from_the_page_or_the_text_on_the_pages_they_show_on() {
    let side = |s: &str| format!(r#"<w:{s} w:val="single" w:sz="12" w:space="24" w:color="7030A0"/>"#);
    let all: String = ["top", "left", "bottom", "right"].iter().map(|s| side(s)).collect();
    let pages = page_rects(&format!(r#"<w:pgBorders w:offsetFrom="page">{all}</w:pgBorders>"#), 1);
    let top = pages[0].iter().find(|(r, _)| r[2] - r[0] > 100.0 && r[1] < 100.0).expect("the top line").0;
    assert_eq!((top[0], top[1], top[3] - top[1]), (24.0, 24.0, 1.5), "24 pt in from the page's edge, 1.5 pt thick");
    let pages = page_rects(&format!(r#"<w:pgBorders>{all}</w:pgBorders>"#), 1);
    let left = pages[0].iter().find(|(r, _)| r[3] - r[1] > 100.0 && r[0] < 100.0).expect("the left line").0;
    assert_eq!(left[0], 72.0 - 24.0 - 1.5, "24 pt out from the text");
    let pages = page_rects(&format!(r#"<w:pgBorders w:display="firstPage">{all}</w:pgBorders>"#), 2);
    assert!(pages.len() >= 2 && pages[0].len() == 4 && pages[1].is_empty(), "the first page only");
}

/// Shading and side borders as Word draws them (each combination exported from Word for Mac): the shading fills
/// to a right border, and to a left one only with a right one too.
#[test]
fn shading_fills_to_a_right_border_and_to_a_left_one_only_with_a_right() {
    let side = |s: &str| format!(r#"<w:{s} w:val="single" w:sz="36" w:space="8" w:color="0E7C86"/>"#);
    let para = |sides: &[&str]| {
        let bdr: String = sides.iter().map(|s| side(s)).collect();
        format!(
            r#"<w:p><w:pPr><w:pBdr>{bdr}</w:pBdr><w:shd w:val="clear" w:color="auto" w:fill="DCEFF0"/><w:ind w:left="720" w:right="720"/></w:pPr><w:r><w:t>Text</w:t></w:r></w:p>"#
        )
    };
    // Text from 36 to 432 pt; each border 8 pt off it, 4.5 pt thick.
    for (sides, want) in [
        (&["left"][..], [36.0, 432.0]),
        (&["top", "left", "bottom"], [36.0, 432.0]),
        (&["right"], [36.0, 440.0]),
        (&["top", "bottom", "right"], [36.0, 440.0]),
        (&["left", "right"], [28.0, 440.0]),
        (&["top", "left", "bottom", "right"], [28.0, 440.0]),
    ] {
        let shade = rects(&para(sides)).into_iter().find(|(_, c)| *c == 0xDCEFF0).expect("shaded").0;
        let got = [shade[0], shade[2]].map(|v| (v - 72.0).round());
        assert_eq!(got, want, "{sides:?}: {shade:?}");
    }
}
