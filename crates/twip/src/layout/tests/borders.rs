//! Borders: text boxes, and paragraph borders in every style.

use super::shapes::bytes;
use crate::layout::fragment::Item;

/// A page's stroked border paths: their rects.
fn strokes(body: &str, page: usize) -> Vec<[f64; 4]> {
    let doc = crate::docx::read(&bytes(body).expect("a package")).expect("reads");
    let snap = crate::layout::layout(&doc, &super::fonts());
    crate::layout::traverse::paint_items(&snap.pages()[page])
        .into_iter()
        .filter_map(|i| match i {
            Item::Shape(s) if s.ink => Some(s.rect),
            _ => None,
        })
        .collect()
}

fn rects(body: &str) -> Vec<([f64; 4], u32)> {
    let doc = crate::docx::read(&bytes(body).expect("a package")).expect("reads");
    let snap = crate::layout::layout(&doc, &super::fonts());
    crate::layout::traverse::paint_items(&snap.pages()[0])
        .into_iter()
        .filter_map(|i| match i {
            Item::Rect { rect, colour, .. } => Some((rect, colour)),
            // Paragraph shading, a filled decoration.
            Item::Shape(s) => match s.fill {
                crate::drawing::paint::Paint::Solid(c) if s.decoration => Some((s.rect, u32::from(c.r) << 16 | u32::from(c.g) << 8 | u32::from(c.b))),
                _ => None,
            },
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
    let doc = crate::docx::read(&bytes(&para("dotted")).expect("a package")).expect("reads");
    let items = crate::layout::traverse::paint_items(&crate::layout::layout(&doc, &super::fonts()).pages()[0]);
    let dotted = items.iter().filter(|i| matches!(i, Item::Rect { dash: crate::layout::fragment::Dash::Dotted, .. })).count();
    assert_eq!(dotted, 1, "one dotted rect, its dots drawn on device pixels");
    assert_eq!(count("wave"), 0, "a wave is a stroked path, not rects");
    let strokes = |style: &str| strokes(&para(style), 0).len();
    assert_eq!(strokes("wave"), 1, "a wave is one stroked path");
    assert_eq!(strokes("doubleWave"), 1);
    assert_eq!(strokes("dashDotStroked"), 1);
    assert_eq!(strokes("single"), 0);
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
                    Item::Rect { rect, colour, .. } if colour == 0x7030A0 => Some((rect, colour)),
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

#[test]
fn a_zero_size_border_draws_a_hairline_that_takes_no_space() {
    let edges = |sz: &str| ["top", "bottom"].map(|side| format!(r#"<w:{side} w:val="dotted"{sz} w:space="0" w:color="C00000"/>"#)).concat();
    let para = |sz: &str| {
        let ppr = if sz == "-" { String::new() } else { format!("<w:pBdr>{}</w:pBdr>", edges(sz)) };
        format!(r#"<w:p><w:pPr>{ppr}</w:pPr><w:r><w:t>Boxed</w:t></w:r></w:p><w:p><w:r><w:t>After</w:t></w:r></w:p>"#)
    };
    let row = |sz: &str| {
        let pr = if sz == "-" { String::new() } else { format!("<w:tcBorders>{}</w:tcBorders>", edges(sz)) };
        let tr = |pr: &str, t: &str| {
            format!(r#"<w:tr><w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/>{pr}</w:tcPr><w:p><w:r><w:t>{t}</w:t></w:r></w:p></w:tc></w:tr>"#)
        };
        format!(
            r#"<w:tbl><w:tblPr><w:tblW w:w="3000" w:type="dxa"/></w:tblPr><w:tblGrid><w:gridCol w:w="3000"/></w:tblGrid>{}{}</w:tbl><w:p/>"#,
            tr(&pr, "Boxed"),
            tr("", "After")
        )
    };
    let after = |body: &str| {
        let doc = crate::docx::read(&bytes(body).expect("a package")).expect("reads");
        super::page_chars(&doc)[0].iter().find(|c| c.0 == 'A').expect("After").2
    };
    let red = |body: &str| rects(body).into_iter().filter(|(_, c)| *c == 0xC00000).map(|(r, _)| [r[0], r[3] - r[1]]).collect::<Vec<_>>();
    let one = r#" w:sz="1""#;
    for body in [&para as &dyn Fn(&str) -> String, &row] {
        let plain = after(&body("-"));
        assert!(after(&body(one)) > plain, "sz 1 takes its width");
        for sz in [r#" w:sz="0""#, ""] {
            assert_eq!(after(&body(sz)), plain, "{sz:?} takes no space");
            assert_eq!(red(&body(sz)), red(&body(one)), "{sz:?} draws as sz 1");
        }
    }
}

fn overlapping(a: &[f64; 4], b: &[f64; 4]) -> bool {
    a[0] < b[2] - 0.01 && b[0] < a[2] - 0.01 && a[1] < b[3] - 0.01 && b[1] < a[3] - 0.01
}

#[test]
fn a_wave_bordered_table_split_across_pages_draws_each_edge_once() {
    let rows: String = (0..90)
        .map(|i| format!(r#"<w:tr><w:tc><w:tcPr><w:tcW w:w="4000" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>row {i}</w:t></w:r></w:p></w:tc></w:tr>"#))
        .collect();
    let wave = |sz: u32| format!(r#"w:val="wave" w:sz="{sz}" w:space="0" w:color="1F4E79""#);
    let body = format!(
        r#"<w:tbl><w:tblPr><w:tblW w:w="4000" w:type="dxa"/><w:tblLayout w:type="fixed"/><w:tblBorders><w:top {0}/><w:bottom {1}/><w:insideH {0}/></w:tblBorders></w:tblPr><w:tblGrid><w:gridCol w:w="4000"/></w:tblGrid>{rows}</w:tbl><w:p/>"#,
        wave(12),
        wave(24)
    );
    let doc = crate::docx::read(&bytes(&body).expect("a package")).expect("reads");
    let snap = crate::layout::layout(&doc, &super::fonts());
    assert!(snap.pages().len() > 1, "the table splits");
    for (n, page) in snap.pages().iter().enumerate() {
        let wide: Vec<[f64; 4]> = crate::layout::traverse::paint_items(page)
            .into_iter()
            .filter_map(|i| match i {
                Item::Shape(s) if s.ink && s.rect[2] - s.rect[0] > 100.0 => Some(s.rect),
                _ => None,
            })
            .collect();
        assert!(!wide.is_empty(), "page {n} has wave edges");
        for (i, a) in wide.iter().enumerate() {
            for b in &wide[i + 1..] {
                assert!(!overlapping(a, b), "page {n}: {a:?} and {b:?} are drawn over each other");
            }
        }
    }
}

#[test]
fn wave_edges_are_clipped_with_their_cell() {
    let wave = r#"w:val="wave" w:sz="12" w:space="0" w:color="1F4E79""#;
    let edges = format!(r#"<w:left {wave}/><w:right {wave}/><w:top {wave}/><w:bottom {wave}/>"#);
    let single = r#"w:val="single" w:sz="4" w:space="0" w:color="000000""#;
    let cell = |inner: &str| {
        format!(
            r#"<w:tbl><w:tblPr><w:tblW w:w="3000" w:type="dxa"/><w:tblLayout w:type="fixed"/><w:tblBorders><w:left {single}/><w:right {single}/></w:tblBorders></w:tblPr><w:tblGrid><w:gridCol w:w="3000"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/></w:tcPr>{inner}</w:tc></w:tr></w:tbl><w:p/>"#
        )
    };
    let spilling = format!(r#"<w:p><w:pPr><w:pBdr>{edges}</w:pBdr><w:ind w:left="-1440" w:right="-1440"/></w:pPr><w:r><w:t>text</w:t></w:r></w:p>"#);
    let nested = format!(
        r#"<w:tbl><w:tblPr><w:tblW w:w="6000" w:type="dxa"/><w:tblInd w:w="-1440" w:type="dxa"/><w:tblLayout w:type="fixed"/><w:tblBorders>{edges}</w:tblBorders></w:tblPr><w:tblGrid><w:gridCol w:w="6000"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w="6000" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>in</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p/>"#
    );
    // The cell's clip opens with the text before the nested table.
    let nested = format!(r#"<w:p><w:r><w:t>lead</w:t></w:r></w:p>{nested}"#);
    for (what, inner) in [("a paragraph", spilling), ("a nested table", nested)] {
        // The cell's sides: the table's black left and right borders.
        let sides: Vec<[f64; 4]> = rects(&cell(&inner)).into_iter().filter(|(r, c)| *c == 0 && r[3] - r[1] > 5.0).map(|(r, _)| r).collect();
        let (left, right) = (sides.iter().map(|r| r[0]).fold(f64::MAX, f64::min), sides.iter().map(|r| r[2]).fold(f64::MIN, f64::max));
        assert!(right - left > 100.0, "{what}: the cell's sides are found: {sides:?}");
        let found = strokes(&cell(&inner), 0);
        assert!(!found.is_empty(), "{what} draws wave edges");
        for r in &found {
            assert!(r[0] >= left - 0.5 && r[2] <= right + 0.5, "{what}: {r:?} runs outside its cell, {left} to {right}");
        }
    }
}

/// The point where a stroked border's path starts or ends, on the page.
fn stroke_end(s: &crate::layout::fragment::ShapeItem, last: bool) -> [f64; 2] {
    use crate::drawing::geom::Seg;
    let segs = &s.geom.paths[0].segs;
    let p = match if last { segs.last() } else { segs.first() }.expect("a path") {
        Seg::Move(p) | Seg::Line(p) => *p,
        other => panic!("{other:?}"),
    };
    [s.rect[0] + p[0], s.rect[1] + p[1]]
}

#[test]
fn a_wave_border_runs_on_from_line_to_line() {
    // A 3 pt wave is 2.5 pt per half period; 14 pt lines end partway through one.
    let wave = r#"w:val="wave" w:sz="12" w:space="0" w:color="1F4E79""#;
    let body = format!(
        r#"<w:p><w:pPr><w:pBdr><w:left {wave}/><w:right {wave}/></w:pBdr></w:pPr><w:r><w:t>one</w:t><w:br/><w:t>two</w:t><w:br/><w:t>three</w:t><w:br/><w:t>four</w:t></w:r></w:p>"#
    );
    let doc = crate::docx::read(&bytes(&body).expect("a package")).expect("reads");
    let snap = crate::layout::layout(&doc, &super::fonts());
    let mut sides: Vec<std::rc::Rc<crate::layout::fragment::ShapeItem>> = crate::layout::traverse::paint_items(&snap.pages()[0])
        .into_iter()
        .filter_map(|i| match i {
            Item::Shape(s) if s.ink && s.rect[2] - s.rect[0] < 10.0 && s.rect[0] < 200.0 => Some(s),
            _ => None,
        })
        .collect();
    sides.sort_by(|a, b| a.rect[1].total_cmp(&b.rect[1]));
    assert!(sides.len() >= 4, "a left side on each line: {}", sides.len());
    for pair in sides.windows(2) {
        let (end, start) = (stroke_end(&pair[0], true), stroke_end(&pair[1], false));
        assert!((end[1] - start[1]).abs() < 0.01, "the lines meet: {end:?} {start:?}");
        assert!((end[0] - start[0]).abs() < 0.01, "the wave goes on across the break: {end:?} {start:?}");
    }
}

#[test]
fn a_dashed_border_runs_on_from_line_to_line() {
    let dashed = r#"w:val="dashed" w:sz="12" w:space="0" w:color="1F4E79""#;
    let body = format!(
        r#"<w:p><w:pPr><w:pBdr><w:left {dashed}/></w:pBdr></w:pPr><w:r><w:t>one</w:t><w:br/><w:t>two</w:t><w:br/><w:t>three</w:t></w:r></w:p>"#
    );
    let doc = crate::docx::read(&bytes(&body).expect("a package")).expect("reads");
    let snap = crate::layout::layout(&doc, &super::fonts());
    let mut sides: Vec<([f64; 4], f32)> = crate::layout::traverse::paint_items(&snap.pages()[0])
        .into_iter()
        .filter_map(|i| match i {
            Item::Rect { rect, phase, dash: crate::layout::fragment::Dash::Dashed, .. } if rect[0] < 200.0 => Some((rect, phase)),
            _ => None,
        })
        .collect();
    sides.sort_by(|a, b| a.0[1].total_cmp(&b.0[1]));
    assert_eq!(sides.len(), 3, "a left side on each line");
    for (rect, phase) in &sides {
        let top = rect[1] - sides[0].0[1];
        assert!((f64::from(*phase) - top).abs() < 0.01, "the dashes carry on from the line above: {phase} at {top}");
    }
}

#[test]
fn a_float_in_an_exact_row_is_clipped_to_the_cell() {
    let float = r#"<w:r><w:drawing><wp:anchor xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" distT="0" distB="0" distL="0" distR="0" simplePos="0" relativeHeight="1" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="column"><wp:posOffset>0</wp:posOffset></wp:positionH><wp:positionV relativeFrom="paragraph"><wp:posOffset>0</wp:posOffset></wp:positionV><wp:extent cx="254000" cy="1016000"/><wp:wrapNone/><wp:docPr id="1" name="Box"/><a:graphic xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wps:wsp xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wps:spPr><a:prstGeom prst="rect"/><a:solidFill><a:srgbClr val="FF0000"/></a:solidFill></wps:spPr><wps:bodyPr/></wps:wsp></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r>"#;
    let body = format!(
        r#"<w:tbl><w:tblPr><w:tblW w:w="5000" w:type="dxa"/></w:tblPr><w:tblGrid><w:gridCol w:w="5000"/></w:tblGrid><w:tr><w:trPr><w:trHeight w:val="200" w:hRule="exact"/></w:trPr><w:tc><w:tcPr><w:tcW w:w="5000" w:type="dxa"/></w:tcPr><w:p>{float}</w:p></w:tc></w:tr></w:tbl><w:p/>"#
    );
    let doc = crate::docx::read(&bytes(&body).expect("a package")).expect("reads");
    let snap = crate::layout::layout(&doc, &super::fonts());
    let mut inked = Vec::new();
    crate::layout::traverse::paint(&snap.pages()[0], &mut |p| {
        if p.float.is_some() && matches!(p.item, Item::Shape(_)) {
            inked.push(p.clip);
        }
    });
    assert!(!inked.is_empty(), "the float draws");
    for clip in inked {
        let clip = clip.expect("clipped to the cell");
        assert!(clip[3] - clip[1] <= 10.0 + 1e-6, "the row is 10 pt: {clip:?}");
    }
}

#[test]
fn reflected_text_keeps_the_clip_of_its_exact_row() {
    // A 12 pt line in a 10 pt exact row, in a reflected text box: its glyphs cross the row's bottom.
    let row = r#"<w:tbl><w:tblPr><w:tblW w:w="2000" w:type="dxa"/></w:tblPr><w:tblGrid><w:gridCol w:w="2000"/></w:tblGrid><w:tr><w:trPr><w:trHeight w:val="200" w:hRule="exact"/></w:trPr><w:tc><w:tcPr><w:tcW w:w="2000" w:type="dxa"/></w:tcPr><w:p><w:pPr><w:spacing w:before="0" w:after="0" w:line="240" w:lineRule="exact"/></w:pPr><w:r><w:t>Tall</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p/>"#;
    let float = format!(
        r#"<w:p><w:r><w:drawing><wp:anchor xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" distT="0" distB="0" distL="0" distR="0" simplePos="0" relativeHeight="1" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="column"><wp:posOffset>0</wp:posOffset></wp:positionH><wp:positionV relativeFrom="paragraph"><wp:posOffset>0</wp:posOffset></wp:positionV><wp:extent cx="1905000" cy="1270000"/><wp:wrapNone/><wp:docPr id="1" name="Box"/><a:graphic xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wps:wsp xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wps:spPr><a:prstGeom prst="rect"/><a:effectLst><a:reflection blurRad="0" stA="50000" endA="0" endPos="100000" dir="5400000" sy="-100000" algn="bl" rotWithShape="0"/></a:effectLst></wps:spPr><wps:txbx><w:txbxContent>{row}</w:txbxContent></wps:txbx><wps:bodyPr lIns="0" tIns="0" rIns="0" bIns="0"/></wps:wsp></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r></w:p>"#
    );
    let doc = crate::docx::read(&bytes(&float).expect("a package")).expect("reads");
    let snap = crate::layout::layout(&doc, &super::fonts());
    let items = crate::layout::traverse::paint_items(&snap.pages()[0]);
    let text: Vec<_> = items
        .iter()
        .filter_map(|i| match i {
            Item::Shape(s) => s.text.as_ref(),
            _ => None,
        })
        .collect();
    assert!(!text.is_empty(), "the reflection holds the box's text");
    // Every glyph the row holds.
    let clips: Vec<Option<[f64; 4]>> =
        text.iter().flat_map(|t| t.0.iter()).filter(|(i, _)| matches!(i, Item::Glyphs(g) if !g.glyphs.is_empty())).map(|(_, c)| *c).collect();
    assert!(!clips.is_empty() && clips.iter().all(|c| c.is_some_and(|c| c[3] - c[1] <= 10.0 + 1e-6)), "{clips:?}");
}
