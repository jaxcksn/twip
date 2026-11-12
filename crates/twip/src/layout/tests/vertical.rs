//! Vertical text

use super::{fonts, package};
use crate::layout::fragment::Item;

const W: &str = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships""#;

fn doc(body: &str) -> Option<crate::docx::Document> {
    sectioned(&format!(r#"{body}<w:sectPr>{MARGINS}</w:sectPr>"#))
}

const MARGINS: &str = r#"<w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/>"#;

fn sectioned(body: &str) -> Option<crate::docx::Document> {
    let document = format!(r#"<w:document {W}><w:body>{body}</w:body></w:document>"#);
    package(&[
        (
            "[Content_Types].xml",
            r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#,
        ),
        (
            "_rels/.rels",
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#,
        ),
        ("word/document.xml", &document),
    ])
}

/// The first page's turned runs: text, origin and direction.
fn turned_runs(doc: &crate::docx::Document) -> Vec<(String, [f64; 2], [f64; 2])> {
    turned_runs_on(doc, 0)
}

fn turned_runs_on(doc: &crate::docx::Document, page: usize) -> Vec<(String, [f64; 2], [f64; 2])> {
    let snap = crate::layout::layout(doc, &fonts());
    let mut out = Vec::new();
    let Some(page) = snap.pages().get(page) else { return out };
    for it in crate::layout::traverse::paint_items(page) {
        if let Item::Turned(t) = it {
            for (r, at, dir) in crate::layout::vertical::runs(&t) {
                out.push((r.chars.iter().map(|c| c.0).collect::<String>().trim().to_string(), at, dir));
            }
        }
    }
    out.retain(|r| !r.0.is_empty());
    out
}

fn table(direction: &str, row: &str, text: &str) -> String {
    let border = r#"w:val="single" w:sz="4" w:space="0" w:color="000000""#;
    format!(
        r#"<w:tbl><w:tblPr><w:tblW w:w="4500" w:type="dxa"/><w:tblBorders><w:top {border}/><w:left {border}/><w:bottom {border}/><w:right {border}/><w:insideH {border}/><w:insideV {border}/></w:tblBorders><w:tblLayout w:type="fixed"/></w:tblPr><w:tblGrid><w:gridCol w:w="1500"/><w:gridCol w:w="3000"/></w:tblGrid><w:tr>{row}<w:tc><w:tcPr><w:tcW w:w="1500" w:type="dxa"/><w:textDirection w:val="{direction}"/></w:tcPr><w:p><w:pPr><w:spacing w:after="0" w:line="240" w:lineRule="auto"/></w:pPr><w:r><w:rPr><w:rFonts w:ascii="Arial" w:hAnsi="Arial"/><w:sz w:val="22"/></w:rPr><w:t>{text}</w:t></w:r></w:p></w:tc><w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/></w:tcPr><w:p><w:pPr><w:spacing w:after="0" w:line="240" w:lineRule="auto"/></w:pPr><w:r><w:rPr><w:rFonts w:ascii="Arial" w:hAnsi="Arial"/><w:sz w:val="22"/></w:rPr><w:t>side cell</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p/>"#
    )
}

const WORDS: &str = "vc amber1 garden2 quietly3 holds4 seven5 bright6 copper7 lanterns8 over9";

#[test]
fn a_tbrl_cell_runs_down_with_its_lines_stacked_from_the_right() {
    let Some(doc) = doc(&table("tbRl", r#"<w:trPr><w:trHeight w:val="2000" w:hRule="exact"/></w:trPr>"#, WORDS)) else { return };
    let runs = turned_runs(&doc);
    assert!(runs.len() >= 3, "{runs:?}");
    assert!(runs.iter().all(|r| r.2 == [0.0, 1.0]), "the text runs down: {runs:?}");
    // The first line against the content's right (1440 + 1500 - 10 twips: no table style, so no pull left), its
    // ascent in.
    let first = runs[0].1;
    assert!((first[0] - (146.5 - 10.49)).abs() < 0.3, "{first:?}");
    // Lines are a vertical line apart: Arial 11 with an em/64 more ascent and descent, 259.85 twips.
    assert!(((runs[0].1[0] - runs[1].1[0]) - 12.99).abs() < 0.05, "{runs:?}");
    // Each line starts at the row's content top and is no longer than the row's content is high.
    assert!(runs.iter().all(|r| (r.1[1] - runs[0].1[1]).abs() < 0.01), "{runs:?}");
}

#[test]
fn a_btlr_cell_runs_up_from_the_content_bottom_with_its_lines_stacked_from_the_left() {
    let Some(doc) = doc(&table("btLr", r#"<w:trPr><w:trHeight w:val="2000" w:hRule="exact"/></w:trPr>"#, WORDS)) else { return };
    let runs = turned_runs(&doc);
    assert!(runs.len() >= 3 && runs.iter().all(|r| r.2 == [0.0, -1.0]), "{runs:?}");
    assert!(runs[1].1[0] > runs[0].1[0], "lines stack to the right: {runs:?}");
    // The first baseline its ascent right of the content's left (1440 + 10 twips).
    assert!((runs[0].1[0] - (72.5 + 10.49)).abs() < 0.3, "{runs:?}");
}

#[test]
fn compat_12_aligns_a_vertical_cells_block_into_its_far_margin() {
    let row = r#"<w:trPr><w:trHeight w:val="2000" w:hRule="exact"/></w:trPr>"#;
    let at = |mode: i64, align: &str| {
        let body = table("tbRl", row, "vc amber1")
            .replace("<w:textDirection", &format!(r#"<w:vAlign w:val="{align}"/><w:textDirection"#))
            .replace("<w:tblLayout", r#"<w:tblCellMar><w:left w:w="108" w:type="dxa"/><w:right w:w="108" w:type="dxa"/></w:tblCellMar><w:tblLayout"#);
        let mut d = doc(&body)?;
        d.settings.compat_mode = mode;
        Some(turned_runs(&d)[0].1[0])
    };
    // How far a block sits from the top-aligned one.
    let shift = |mode: i64, align: &str| Some(at(mode, "top")? - at(mode, align)?);
    let Some(c15) = shift(15, "bottom") else { return };
    // The far (left) margin, 108 twips less half the 10-twip border, takes the block further left.
    assert!((shift(12, "bottom").unwrap() - c15 - 5.15).abs() < 0.05, "{c15} {:?}", shift(12, "bottom"));
    assert!((shift(12, "center").unwrap() - shift(15, "center").unwrap() - 2.575).abs() < 0.05);
}

#[test]
fn a_vertical_cell_adds_nothing_to_an_auto_row() {
    let (Some(v), Some(h)) = (doc(&table("tbRl", "", WORDS)), doc(&table("lrTb", "", "x"))) else { return };
    let rows = |d: &crate::docx::Document| {
        let snap = crate::layout::layout(d, &fonts());
        crate::layout::traverse::paint_items(&snap.pages()[0])
            .into_iter()
            .filter_map(|i| if let Item::Rect { rect, .. } = i { Some(rect[3]) } else { None })
            .fold(0.0, f64::max)
    };
    assert!((rows(&v) - rows(&h)).abs() < 0.01, "{} vs {}", rows(&v), rows(&h));
    // The row is one horizontal line high (253 twips), so words break inside to fit the vertical lines.
    let runs: Vec<String> = turned_runs(&v).into_iter().map(|r| r.0).collect();
    assert_eq!(runs[..5], ["vc", "a", "m", "be", "r1"], "{runs:?}");
}

#[test]
fn strict_spellings_and_unknown_values_read() {
    let Some(d) = doc(&table("lr", r#"<w:trPr><w:trHeight w:val="2000" w:hRule="exact"/></w:trPr>"#, WORDS)) else { return };
    assert!(turned_runs(&d).iter().all(|r| r.2 == [0.0, -1.0]));
    let Some(doc) = doc(&table("tbLr", "", "x")) else { panic!("an unknown direction still opens") };
    assert!(turned_runs(&doc).is_empty(), "it lays out horizontally");
}

#[test]
fn a_frames_own_text_direction_turns_its_text_and_a_paragraphs_elsewhere_does_not() {
    let frame = r#"<w:framePr w:w="1500" w:h="2500" w:hRule="exact" w:wrap="around" w:vAnchor="page" w:hAnchor="page" w:x="1500" w:y="3000"/>"#;
    let body = format!(
        r#"<w:p><w:r><w:t>body</w:t></w:r></w:p><w:p><w:pPr>{frame}<w:textDirection w:val="btLr"/></w:pPr><w:r><w:t>framed words</w:t></w:r></w:p><w:p><w:pPr><w:textDirection w:val="tbRl"/></w:pPr><w:r><w:t>plain</w:t></w:r></w:p>"#
    );
    let Some(doc) = doc(&body) else { return };
    let runs = turned_runs(&doc);
    assert_eq!(runs.len(), 1, "{runs:?}");
    assert_eq!((runs[0].0.as_str(), runs[0].2), ("framed words", [0.0, -1.0]));
    // From the frame's bottom (3000 + 2500 twips) up.
    assert!((runs[0].1[1] - 275.0).abs() < 0.3, "{runs:?}");
}

/// A paragraph of Arial 11, single spaced unless `spacing` says otherwise.
fn para(text: &str, spacing: &str) -> String {
    let spacing = if spacing.is_empty() { r#"<w:spacing w:after="0" w:line="240" w:lineRule="auto"/>"# } else { spacing };
    format!(
        r#"<w:p><w:pPr>{spacing}</w:pPr><w:r><w:rPr><w:rFonts w:ascii="Arial" w:hAnsi="Arial"/><w:sz w:val="22"/></w:rPr><w:t>{text}</w:t></w:r></w:p>"#
    )
}

fn section(direction: &str, extra: &str, body: &str) -> Option<crate::docx::Document> {
    sectioned(&format!(
        r#"{body}<w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1000" w:right="2000" w:bottom="3000" w:left="4000" w:header="600" w:footer="800" w:gutter="{extra}"/><w:textDirection w:val="{direction}"/></w:sectPr>"#
    ))
}

#[test]
fn a_tbrl_section_runs_its_lines_down_the_page_from_the_right_margin() {
    let Some(doc) = section("tbRl", "0", &format!("{}{}", para("first line", ""), para("second", ""))) else { return };
    let runs = turned_runs(&doc);
    assert_eq!(runs.len(), 2, "{runs:?}");
    assert!(runs.iter().all(|r| r.2 == [0.0, 1.0]), "{runs:?}");
    // The first line against the right margin (12240 - 2000 twips), its ascent in; each line from the top margin.
    assert!((runs[0].1[0] - (512.0 - 10.49)).abs() < 0.2 && (runs[0].1[1] - 50.0).abs() < 0.01, "{runs:?}");
    assert!(((runs[0].1[0] - runs[1].1[0]) - 12.99).abs() < 0.05, "{runs:?}");
    // A section's btLr runs down the same way.
    let Some(bt) = section("btLr", "0", &para("first line", "")) else { return };
    assert_eq!(turned_runs(&bt)[0].2, [0.0, 1.0]);
}

#[test]
fn a_tbrlv_section_stacks_its_lines_from_the_left_margin() {
    let Some(doc) = section("tbLrV", "0", &format!("{}{}", para("first line", ""), para("second", ""))) else { return };
    let runs = turned_runs(&doc);
    assert!(runs.iter().all(|r| r.2 == [0.0, 1.0]), "{runs:?}");
    // The first baseline its descent right of the left margin (4000 twips), the next line further right.
    assert!((runs[0].1[0] - (200.0 + 2.5)).abs() < 0.2, "{runs:?}");
    assert!(((runs[1].1[0] - runs[0].1[0]) - 12.99).abs() < 0.05, "{runs:?}");
}

#[test]
fn a_vertical_sections_gutter_is_on_the_right() {
    let Some(doc) = section("tbRl", "720", &para("first line", "")) else { return };
    let runs = turned_runs(&doc);
    assert!((runs[0].1[0] - (512.0 - 36.0 - 10.49)).abs() < 0.2 && (runs[0].1[1] - 50.0).abs() < 0.01, "{runs:?}");
}

#[test]
fn a_horizontal_sections_rtl_gutter_is_on_the_right() {
    let at = |rtl: &str| {
        let body = format!(r#"{}<w:sectPr>{}{rtl}</w:sectPr>"#, para("first", ""), MARGINS.replace(r#"w:gutter="0""#, r#"w:gutter="720""#));
        let snap = crate::layout::layout(&sectioned(&body)?, &fonts());
        crate::layout::traverse::paint_items(&snap.pages()[0]).into_iter().find_map(|i| if let Item::Glyphs(r) = i { Some(r.x) } else { None })
    };
    let Some(left) = at("") else { return };
    // The 36pt gutter goes from the left margin to the right one: the text starts at the plain left margin.
    assert!((left - 108.0).abs() < 0.01 && (at("<w:rtlGutter/>").unwrap() - 72.0).abs() < 0.01, "{left}");
}

#[test]
fn a_table_in_a_vertical_section_stands_upright_where_the_next_line_would_go() {
    let table = r#"<w:tbl><w:tblPr><w:tblLayout w:type="fixed"/></w:tblPr><w:tblGrid><w:gridCol w:w="2000"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w="2000" w:type="dxa"/></w:tcPr><w:p><w:r><w:rPr><w:rFonts w:ascii="Arial" w:hAnsi="Arial"/><w:sz w:val="22"/></w:rPr><w:t>cell</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#;
    let Some(doc) = section("tbRl", "0", &format!("{}{table}{}", para("before", ""), para("after", ""))) else { return };
    let snap = crate::layout::layout(&doc, &fonts());
    // The cell's text isn't turned; its box ends where the next line would start, one line in from the right
    // margin (512pt), and its row starts at the top margin.
    let cell = crate::layout::traverse::paint_items(&snap.pages()[0])
        .into_iter()
        .find_map(|i| if let Item::Glyphs(r) = i { r.chars.iter().any(|c| c.0 == 'c').then_some((r.x, r.baseline)) } else { None });
    let Some((x, baseline)) = cell else { panic!("an upright cell") };
    assert!((x - (512.0 - 12.99 - 100.0)).abs() < 1.5 && baseline > 50.0 && baseline < 65.0, "{x} {baseline}");
    // The paragraph after it takes that stack position too, its line starting past the table along the line.
    let runs = turned_runs(&doc);
    let at = |t: &str| runs.iter().find(|r| r.0 == t).map(|r| r.1).expect("a line");
    let (before, after) = (at("before"), at("after"));
    assert!((before[0] - after[0] - 12.99).abs() < 0.05 && after[1] > baseline, "{before:?} {after:?}");
}

#[test]
fn only_compat_15_moves_a_vertical_sections_table_too_wide_for_the_rest_of_the_page() {
    let table = r#"<w:tbl><w:tblPr><w:tblLayout w:type="fixed"/></w:tblPr><w:tblGrid><w:gridCol w:w="6100"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w="6100" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>cell</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#;
    let first_page = |mode: i64| {
        let mut doc = section("tbRl", "0", &format!("{}{table}{}", para("before", ""), para("after", "")))?;
        doc.settings.compat_mode = mode;
        let snap = crate::layout::layout(&doc, &fonts());
        let items = crate::layout::traverse::paint_items(&snap.pages()[0]);
        Some(items.iter().any(|i| matches!(i, Item::Glyphs(r) if r.chars.iter().any(|c| c.0 == 'c'))))
    };
    // 6240 twips across less one line leave no room for 6110: compat 12 places it anyway.
    let Some(c15) = first_page(15) else { return };
    assert!(!c15 && first_page(12) == Some(true));
}

#[test]
fn adjacent_tables_in_a_vertical_section_are_one_table_from_the_first_ones_left_edge() {
    let table = |w: u32, text: &str| {
        let border = r#"w:val="single" w:sz="4" w:space="0" w:color="000000""#;
        format!(
            r#"<w:tbl><w:tblPr><w:tblBorders><w:top {border}/><w:bottom {border}/></w:tblBorders><w:tblLayout w:type="fixed"/></w:tblPr><w:tblGrid><w:gridCol w:w="{w}"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w="{w}" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>{text}</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#
        )
    };
    let body = format!("{}{}{}{}", para("before", ""), table(3000, "wide"), table(2000, "narrow"), para("after", ""));
    let Some(doc) = section("tbRl", "0", &body) else { return };
    let snap = crate::layout::layout(&doc, &fonts());
    let cell = |t: char| {
        crate::layout::traverse::paint_items(&snap.pages()[0]).into_iter().find_map(|i| match i {
            Item::Glyphs(r) if r.chars.first().is_some_and(|c| c.0 == t) => Some((r.x, r.baseline)),
            _ => None,
        })
    };
    let (Some(wide), Some(narrow)) = (cell('w'), cell('n')) else { panic!("both cells") };
    // The narrow rows start where the wide ones do, below them.
    assert!((wide.0 - narrow.0).abs() < 0.01 && narrow.1 > wide.1, "{wide:?} {narrow:?}");
    // The text beside the whole table starts past its last rows.
    let runs = turned_runs(&doc);
    let after = runs.iter().find(|r| r.0 == "after").map(|r| r.1).expect("a line");
    assert!(after[1] > narrow.1, "{after:?} {narrow:?}");
}

#[test]
fn a_vertical_sections_floating_table_takes_tblpx_along_the_stack_and_tblpyspec_along_the_line() {
    let cell = |pos: &str| {
        let table = format!(
            r#"<w:tbl><w:tblPr><w:tblpPr {pos}/><w:tblLayout w:type="fixed"/></w:tblPr><w:tblGrid><w:gridCol w:w="2000"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w="2000" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>cell</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#
        );
        let doc = section("tbRl", "0", &format!("{}{table}{}", para("before", ""), para("after", "")))?;
        let snap = crate::layout::layout(&doc, &fonts());
        crate::layout::traverse::paint_items(&snap.pages()[0])
            .into_iter()
            .find_map(|i| if let Item::Glyphs(r) = i { r.chars.iter().any(|c| c.0 == 'c').then_some((r.x, r.baseline)) } else { None })
    };
    let Some(text) = cell(r#"w:vertAnchor="text" w:horzAnchor="text""#) else { return };
    // 1000 twips further from the right margin, as printed.
    let x = cell(r#"w:vertAnchor="text" w:horzAnchor="text" w:tblpX="1000""#).unwrap();
    assert!((text.0 - x.0 - 50.0).abs() < 0.01 && (text.1 - x.1).abs() < 0.01, "{text:?} {x:?}");
    // At the bottom of the margins (792 − 150pt) instead of the top.
    let bottom = cell(r#"w:vertAnchor="margin" w:tblpYSpec="bottom""#).unwrap();
    assert!(bottom.1 > 600.0 && bottom.1 < 642.0 && (bottom.0 - text.0).abs() < 0.01, "{bottom:?}");
}

/// A one-column table of `rows` rows `w` twips wide, the first a header row.
fn long_table(rows: usize, w: u32) -> String {
    let row = |i: usize| {
        let header = if i == 0 { "<w:trPr><w:tblHeader/></w:trPr>" } else { "" };
        format!(r#"<w:tr>{header}<w:tc><w:tcPr><w:tcW w:w="{w}" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>r{i}</w:t></w:r></w:p></w:tc></w:tr>"#)
    };
    let rows: String = (0..rows).map(row).collect();
    let mar = r#"<w:tblCellMar><w:left w:w="108" w:type="dxa"/><w:right w:w="108" w:type="dxa"/></w:tblCellMar>"#;
    format!(r#"<w:tbl><w:tblPr>{mar}<w:tblLayout w:type="fixed"/></w:tblPr><w:tblGrid><w:gridCol w:w="{w}"/></w:tblGrid>{rows}</w:tbl>"#)
}

/// Where each cell's text starts on page `page`: text, x, baseline.
fn cells_on(doc: &crate::docx::Document, page: usize) -> Vec<(String, f64, f64)> {
    let snap = crate::layout::layout(doc, &fonts());
    let Some(page) = snap.pages().get(page) else { return Vec::new() };
    crate::layout::traverse::paint_items(page)
        .into_iter()
        .filter_map(|i| match i {
            Item::Glyphs(r) => Some((r.chars.iter().map(|c| c.0).collect::<String>(), r.x, r.baseline)),
            _ => None,
        })
        .collect()
}

#[test]
fn a_line_with_no_room_beside_a_vertical_table_moves_to_its_far_side_and_the_table_stays() {
    let body = format!("{}{}{}{}", para("one", ""), para("two", ""), para("three", ""), long_table(50, 2000));
    let Some(mut doc) = section("tbRl", "0", &body) else { return };
    doc.settings.compat_mode = 12;
    // The table at the stack position after three lines, compat 12's 5.4pt back: its first cell's text 5.4pt in.
    let cells = cells_on(&doc, 0);
    let Some(&(_, x, _)) = cells.iter().find(|c| c.0 == "r0") else { panic!("{cells:?}") };
    let left = 512.0 - 3.0 * 12.9925 + 5.4 - 100.0;
    assert!((x - 5.4 - left).abs() < 0.05, "{x}");
    // Lines two and three meet it 9pt out: two takes its far side, three the next line beyond; one stays.
    let runs = turned_runs(&doc);
    let at = |t: &str| runs.iter().find(|r| r.0 == t).map(|r| r.1[0]).expect("a line");
    assert!((at("one") - (512.0 - 10.49)).abs() < 0.05);
    assert!((at("two") - (left - 9.0 - 10.49)).abs() < 0.05, "{} {runs:?} {:?}", at("two"), &cells[..4]);
    assert!((at("two") - at("three") - 12.99).abs() < 0.05);
}

#[test]
fn a_vertical_table_longer_than_the_line_splits_by_rows_its_header_repeated() {
    let body = format!("{}{}{}", para("one", ""), long_table(40, 2000), para("after", ""));
    let at = |mode: i64, page: usize| {
        let mut doc = section("tbRl", "0", &body)?;
        doc.settings.compat_mode = mode;
        Some(cells_on(&doc, page))
    };
    let first = |cells: &[(String, f64, f64)], t: &str| cells.iter().find(|c| c.0 == t).map(|c| c.1);
    // Compat 15 moves it to the next page, as the line has no room for it after "one".
    let Some(p1) = at(15, 0) else { return };
    assert!(first(&p1, "r0").is_none() && first(&at(15, 1).unwrap(), "r0").is_some());
    // Compat 12 starts it after "one" and goes on where it was across the next page, below the header row again.
    let (p1, p2) = (at(12, 0).unwrap(), at(12, 1).unwrap());
    let (x1, x2) = (first(&p1, "r0").expect("on the first page"), first(&p2, "r0").expect("its header again"));
    assert!((x1 - x2).abs() < 0.01 && first(&p1, "r39").is_none() && first(&p2, "r39") == Some(x1), "{x1} {x2}");
}

#[test]
fn a_continuous_break_into_a_vertical_section_starts_a_page() {
    let body = format!(
        r#"{}<w:p><w:pPr><w:sectPr>{MARGINS}</w:sectPr></w:pPr></w:p>{}<w:sectPr><w:type w:val="continuous"/>{MARGINS}<w:textDirection w:val="tbRl"/></w:sectPr>"#,
        para("horizontal", ""),
        para("vertical", "")
    );
    let Some(doc) = sectioned(&body) else { return };
    assert!(turned_runs_on(&doc, 0).is_empty());
    assert_eq!(turned_runs_on(&doc, 1).first().map(|r| r.0.as_str()), Some("vertical"));
}

#[test]
fn an_exact_line_in_a_centred_flow_draws_its_text_lower() {
    let exact = r#"<w:spacing w:after="0" w:line="600" w:lineRule="exact"/>"#;
    let base = |dir: &str| section(dir, "0", &para("exact", exact)).map(|d| turned_runs(&d)[0].1[0]);
    let (Some(roman), Some(centred)) = (base("btLr"), base("tbRl")) else { return };
    // Both lines are 600 twips from the right margin with the reference baseline 480 in; tbRl keeps East Asian
    // glyphs upright, so its runs are centred and Arial 11 sits 76.27 twips further.
    assert!((roman - (512.0 - 24.0)).abs() < 0.2, "{roman}");
    assert!((roman - centred - 3.81).abs() < 0.05, "{roman} {centred}");
}

#[test]
fn each_run_of_a_centred_line_sits_its_own_drop_below_the_reference_baseline() {
    if fonts().face("Times New Roman", None, false, false).is_none_or(|f| f.family != "Times New Roman") {
        return;
    }
    let mixed = |align: &str| {
        let p = format!(
            r#"<w:p><w:pPr><w:spacing w:after="0" w:line="240" w:lineRule="auto"/>{align}</w:pPr><w:r><w:rPr><w:rFonts w:ascii="Times New Roman" w:hAnsi="Times New Roman"/><w:sz w:val="40"/></w:rPr><w:t xml:space="preserve">big </w:t></w:r><w:r><w:rPr><w:rFonts w:ascii="Arial" w:hAnsi="Arial"/><w:sz w:val="22"/></w:rPr><w:t>small</w:t></w:r></w:p>"#
        );
        let runs = turned_runs(&section("tbRl", "0", &p)?);
        let across = |t: &str| runs.iter().find(|r| r.0 == t).map(|r| r.1[0]);
        Some(across("small")? - across("big")?)
    };
    let Some(centred) = mixed("") else { return };
    // Times New Roman 20 drops 134.96 twips, Arial 11 76.27: the small run sits 58.7 nearer the line top; aligned
    // to the baseline both share it.
    assert!((centred - 2.93).abs() < 0.1, "{centred}");
    assert!(mixed(r#"<w:textAlignment w:val="baseline"/>"#).unwrap().abs() < 1e-6);
}

#[test]
fn text_alignment_sets_each_runs_own_metrics_in_every_flow() {
    if fonts().face("Times New Roman", None, false, false).is_none_or(|f| f.family != "Times New Roman") {
        return;
    }
    let baselines = |spacing: &str, align: &str| {
        let p = format!(
            r#"<w:p><w:pPr><w:spacing w:after="0" {spacing}/><w:textAlignment w:val="{align}"/></w:pPr><w:r><w:rPr><w:rFonts w:ascii="Times New Roman" w:hAnsi="Times New Roman"/><w:sz w:val="40"/></w:rPr><w:t xml:space="preserve">big </w:t></w:r><w:r><w:rPr><w:rFonts w:ascii="Arial" w:hAnsi="Arial"/><w:sz w:val="22"/></w:rPr><w:t>small</w:t></w:r></w:p>"#
        );
        let items = cells_on(&section("lrTb", "0", &p)?, 0);
        let at = |t: &str| items.iter().find(|i| i.0.starts_with(t)).map(|i| i.2);
        Some((at("big")?, at("small")?))
    };
    // Exact 600: the reference baseline 480 twips below the line top, Times New Roman hung its win ascent less a
    // unit below it (1824 × 20 / 2048 pt), Arial its own.
    let Some((big, small)) = baselines(r#"w:line="600" w:lineRule="exact""#, "top") else { return };
    assert!((big - (50.0 + 24.0 + 17.8125)).abs() < 0.01 && (small - (50.0 + 24.0 + 1853.0 * 11.0 / 2048.0)).abs() < 0.01, "{big} {small}");
    // An auto line takes the aligned metrics: a unit and the line gap above the reference baseline.
    let (big, small) = baselines(r#"w:line="240" w:lineRule="auto""#, "top").unwrap();
    assert!((big - small - (1824.0 * 20.0 - 1853.0 * 11.0) / 2048.0).abs() < 0.01, "{big} {small}");
    let (big, small) = baselines(r#"w:line="240" w:lineRule="auto""#, "baseline").unwrap();
    assert!((big - small).abs() < 1e-6);
}

fn east_asian(text: &str) -> String {
    format!(
        r#"<w:p><w:pPr><w:spacing w:after="0" w:line="240" w:lineRule="auto"/></w:pPr><w:r><w:rPr><w:rFonts w:ascii="MS Gothic" w:hAnsi="MS Gothic" w:eastAsia="MS Gothic"/><w:sz w:val="22"/></w:rPr><w:t>{text}</w:t></w:r></w:p>"#
    )
}

#[test]
fn east_asian_glyphs_stand_upright_where_the_flow_keeps_them_so() {
    let Some(upright) = section("tbRl", "0", &east_asian("漢字Ab")) else { return };
    let runs = turned_runs(&upright);
    let dir = |runs: &[(String, [f64; 2], [f64; 2])], t: &str| runs.iter().find(|r| r.0 == t).map(|r| r.2);
    assert_eq!(dir(&runs, "漢"), Some([1.0, 0.0]), "{runs:?}");
    assert_eq!(dir(&runs, "Ab"), Some([0.0, 1.0]), "{runs:?}");
    // By vertical orientation: § stands upright, half-width katakana lie sideways.
    let Some(classes) = section("tbRl", "0", &east_asian("漢§ｱ")) else { return };
    let runs = turned_runs(&classes);
    assert_eq!((dir(&runs, "§"), dir(&runs, "ｱ")), (Some([1.0, 0.0]), Some([0.0, 1.0])), "{runs:?}");
    // A section's btLr turns them with the text.
    let Some(turned) = section("btLr", "0", &east_asian("漢字Ab")) else { return };
    assert!(turned_runs(&turned).iter().all(|r| r.2 == [0.0, 1.0]));
    // lrTbV stays horizontal but turns them counter-clockwise.
    let Some(across) = section("lrTbV", "0", &east_asian("漢字Ab")) else { return };
    let runs = turned_runs(&across);
    assert!(!runs.is_empty() && runs.iter().all(|r| r.2 == [0.0, -1.0]), "{runs:?}");
}

#[test]
fn an_upright_glyphs_em_box_takes_the_base_middle_and_its_vertical_origin() {
    let fonts = fonts();
    let Some(yu) = fonts.face("Yu Gothic", None, false, false).filter(|f| f.family == "Yu Gothic") else { return };
    // Ideographic baseline −246, Roman 0, upem 2048: the em box middle 778 units up, not the win metrics' 699.
    assert_eq!(yu.em_middle, 778.0);
    // 漢 hangs from its vertical origin: top side bearing 116 above its top at 1686.
    let kan = yu.glyph('漢').expect("a glyph");
    assert_eq!(yu.v_origin(kan), Some(1802.0));
    if let Some(arial) = fonts.face("Arial", None, false, false).filter(|f| f.family == "Arial") {
        assert_eq!((arial.em_middle, arial.v_origin(arial.glyph('a').expect("a glyph"))), (710.0, None));
    }
}

#[test]
fn a_vertical_runs_extra_is_an_em_64th_in_whole_font_units() {
    let mut fonts = crate::font::Fonts::default();
    let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/editor/fonts/stand-in-sans.ttf")).expect("the stand-in font");
    let info = fonts.add_bytes(bytes).expect("a font");
    let face = fonts.face(&info[0].family, None, false, false).expect("its face");
    assert_eq!(face.upem, 1000.0);
    // 1000 / 64 = 15.625 units round to 16, scaled to 11pt: 16 × 22 × 2048 / 1000 layout units.
    let e = crate::layout::vertical::extra(Some(&face), 11.0);
    assert!((e - crate::layout::units::Lu((16 * 22 * 2048 + 500) / 1000).to_pt()).abs() < 1e-9, "{e}");
}

/// A one-column table whose rows hold `prefix0`, `prefix1`, …
fn named_table(prefix: &str, rows: usize, w: u32) -> String {
    let row = |i: usize| format!(r#"<w:tr><w:tc><w:tcPr><w:tcW w:w="{w}" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>{prefix}{i}</w:t></w:r></w:p></w:tc></w:tr>"#);
    let rows: String = (0..rows).map(row).collect();
    format!(r#"<w:tbl><w:tblPr><w:tblLayout w:type="fixed"/></w:tblPr><w:tblGrid><w:gridCol w:w="{w}"/></w:tblGrid>{rows}</w:tbl>"#)
}

/// A footnote of `lines` one-line paragraphs `n0`, `n1`, … referenced from the first cell of the section's first
/// table.
fn note_in_table(doc: &mut crate::docx::Document, lines: usize) {
    use crate::docx::{Block, Inline, NoteKind};
    const ID: i64 = 7001;
    let blocks = &mut doc.sections[0].blocks;
    let Some(Block::Table(t)) = blocks.iter_mut().find(|b| matches!(b, Block::Table(_))) else { panic!("a table") };
    let Some(Block::Para(p)) = t.rows[0].cells[0].blocks.first_mut() else { panic!("a cell paragraph") };
    p.inlines.push(Inline::NoteRef { kind: NoteKind::Foot, id: ID, custom: false, run: p.mark.clone() });
    let Some(Block::Para(model)) = doc.sections[0].blocks.iter().find(|b| matches!(b, Block::Para(_))).cloned() else { panic!("a paragraph") };
    let note = (0..lines)
        .map(|k| {
            let mut n = model.clone();
            let run = n.inlines.iter().find_map(|i| if let Inline::Text(_, r) = i { Some(r.clone()) } else { None }).unwrap_or_else(|| n.mark.clone());
            n.inlines = vec![Inline::Text(format!("n{k}"), run)];
            n.id = crate::docx::model::next_para_id();
            Block::Para(n)
        })
        .collect();
    doc.footnotes.insert(ID, note);
}

#[test]
fn a_footnote_in_a_vertical_sections_table_goes_to_the_page_and_runs_on_past_the_text() {
    let body = format!("{}{}{}", para("before", ""), named_table("c", 1, 2000), para("after", ""));
    let Some(mut doc) = section("tbRl", "0", &body) else { return };
    doc.settings.compat_mode = 15;
    note_in_table(&mut doc, 30);
    let on = |page: usize| turned_runs_on(&doc, page).into_iter().map(|r| r.0).collect::<Vec<_>>();
    let (p1, p2) = (on(0), on(1));
    // The page holds 24 lines: the note starts on the table's page and goes on on a page of its own.
    assert!(p1.iter().any(|t| t == "n0") && p1.iter().any(|t| t == "after"), "{p1:?}");
    assert!(p2.iter().any(|t| t == "n29") && !p2.iter().any(|t| t == "after"), "{p2:?}");
}

#[test]
fn a_footnote_cut_after_a_vertical_sections_table_leaves_no_room_for_the_text_after_it() {
    let exact = r#"<w:spacing w:after="0" w:line="300" w:lineRule="exact"/>"#;
    let lines: String = (0..16).map(|k| para(&format!("l{k}"), exact)).collect();
    let pages = |mode: i64, w: u32| {
        let mut doc = section("tbRl", "0", &format!("{lines}{}{}", named_table("c", 1, w), para("after", "")))?;
        doc.settings.compat_mode = mode;
        note_in_table(&mut doc, 6);
        let on = |page: usize| turned_runs_on(&doc, page).into_iter().map(|r| r.0).collect::<Vec<_>>();
        Some((on(0), on(1)))
    };
    let Some((p1, p2)) = pages(15, 200) else { return };
    // Compat 15: the note's first lines go past the table across the page and the rest on the next page; the
    // line after the table, reaching past it, has no room left before them.
    assert!(p1.iter().any(|t| t == "n0") && !p1.iter().any(|t| t == "n5"), "{p1:?}");
    assert!(!p1.iter().any(|t| t == "after") && p2.iter().any(|t| t == "after"), "{p1:?}");
    // A table reaching nearly to the end of the page leaves its note no room past it; compat ≤ 14 counts the
    // room from where the table starts.
    let has = |(p, _): (Vec<String>, Vec<String>), t: &str| p.iter().any(|x| x == t);
    assert!(!has(pages(15, 1000).unwrap(), "n0") && has(pages(12, 1000).unwrap(), "n0"));
}

#[test]
fn compat_14_draws_a_vertical_table_too_long_below_an_earlier_one_from_the_line_start() {
    let body = format!("{}{}{}{}{}", para("pa", ""), named_table("r", 12, 2000), para("pb", ""), named_table("s", 14, 2000), para("pc", ""));
    let at = |mode: i64, page: usize| {
        let mut doc = section("tbRl", "0", &body)?;
        doc.settings.compat_mode = mode;
        Some(cells_on(&doc, page))
    };
    let Some(p1) = at(12, 0) else { return };
    let base = |cells: &[(String, f64, f64)], t: &str| cells.iter().find(|c| c.0 == t).map(|c| c.2);
    // Over the earlier table from the top margin, with only the rows that fit below that table; the rest on the
    // next page.
    let (r0, s0) = (base(&p1, "r0").expect("r0"), base(&p1, "s0").expect("s0"));
    assert!((r0 - s0).abs() < 0.01, "{r0} {s0}");
    let p2 = at(12, 1).unwrap();
    assert!(base(&p1, "s13").is_none() && base(&p2, "s13").is_some(), "{p1:?}");
    let kept = p1.iter().filter(|c| c.0.starts_with('s')).count();
    assert!(kept > 0 && kept + p2.iter().filter(|c| c.0.starts_with('s')).count() == 14, "{kept}");
}

#[test]
fn compat_15_moves_a_vertical_table_too_long_below_an_earlier_one_across_past_it() {
    let body = format!("{}{}{}{}{}", para("pa", ""), named_table("r", 12, 2500), para("pb", ""), named_table("s", 18, 1500), para("pc", ""));
    let Some(mut doc) = section("tbRl", "0", &body) else { return };
    doc.settings.compat_mode = 15;
    let p1 = cells_on(&doc, 0);
    let x = |t: &str| p1.iter().find(|c| c.0 == t).map(|c| c.1);
    // All its rows stay on the page, from the line start, past the earlier table across the stack.
    let (r0, s0) = (x("r0").expect("r0"), x("s0").expect("s0"));
    assert!(x("s17").is_some() && s0 < r0 - 75.0 - 18.0 + 0.01, "{r0} {s0}");
}

#[test]
fn compat_15_moves_a_vertical_table_whose_note_cant_start_past_it_to_the_next_page() {
    let lines: String = (0..18).map(|k| para(&format!("l{k}"), "")).collect();
    let pages = |mode: i64| {
        let mut doc = section("tbRl", "0", &format!("{lines}{}{}", named_table("c", 1, 1300), para("after", "")))?;
        doc.settings.compat_mode = mode;
        note_in_table(&mut doc, 2);
        let on = |page: usize| cells_on(&doc, page).into_iter().map(|r| r.0).chain(turned_runs_on(&doc, page).into_iter().map(|r| r.0)).collect::<Vec<_>>();
        Some((on(0), on(1)))
    };
    let Some((p1, p2)) = pages(15) else { return };
    // Its far side leaves less than the separator and a note line before the page's end: it goes on with its note.
    assert!(!p1.iter().any(|t| t.starts_with("c0")) && p2.iter().any(|t| t.starts_with("c0")) && p2.iter().any(|t| t == "n0"), "{p1:?} {p2:?}");
    // Compat ≤ 14 keeps it.
    let (p1, _) = pages(12).unwrap();
    assert!(p1.iter().any(|t| t.starts_with("c0")), "{p1:?}");
}

/// A one-column table `w` twips wide whose rows, `prefix0`, `prefix1`, …, are exactly as many twips tall as given.
fn exact_table(prefix: &str, heights: &[u32], w: u32) -> String {
    let row = |(i, h): (usize, &u32)| {
        format!(
            r#"<w:tr><w:trPr><w:trHeight w:val="{h}" w:hRule="exact"/></w:trPr><w:tc><w:tcPr><w:tcW w:w="{w}" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>{prefix}{i}</w:t></w:r></w:p></w:tc></w:tr>"#
        )
    };
    let rows: String = heights.iter().enumerate().map(row).collect();
    format!(r#"<w:tbl><w:tblPr><w:tblLayout w:type="fixed"/></w:tblPr><w:tblGrid><w:gridCol w:w="{w}"/></w:tblGrid>{rows}</w:tbl>"#)
}

#[test]
fn compat_14_keeps_the_more_of_a_too_long_vertical_tables_rows_below_the_earlier_one_or_below_its_own_image() {
    // Lines 11840 tw long from 1000 tw; the earlier table ends `t1` rows of 500 tw along them.
    let kept = |t1: usize, t2: &[u32]| {
        let body = format!("{}{}{}{}{}", para("pa", ""), exact_table("r", &vec![500; t1], 2000), para("pb", ""), exact_table("s", t2, 2000), para("pc", ""));
        let mut doc = section("tbRl", "0", &body)?;
        doc.settings.compat_mode = 12;
        Some((cells_on(&doc, 0).iter().filter(|c| c.0.starts_with('s')).count(), turned_runs(&doc)))
    };
    // 1840 tw below the earlier table hold no row of 3000; stacked below its own image twice the table leaves 5840,
    // which hold six rows.
    let mut t2 = vec![3000];
    t2.extend([500; 6]);
    let Some((n, _)) = kept(20, &t2) else { return };
    assert_eq!(n, 6);
    // Nineteen rows fit below an earlier table of four; reaching past its end, they push the lines before them to
    // their end, and then all that fit the line stay.
    let (n, runs) = kept(4, &[500; 30]).unwrap();
    assert_eq!(n, 23);
    let pb = runs.iter().find(|r| r.0 == "pb").expect("pb").1[1];
    assert!((pb - 525.0).abs() < 0.01, "{pb}");
}

/// A Letter `tbRl` section with inch margins in compat 15: lines 12960 tw long from 1440 tw.
fn inch_section(body: &str) -> Option<crate::docx::Document> {
    let mut doc = sectioned(&format!(r#"{body}<w:sectPr>{MARGINS}<w:textDirection w:val="tbRl"/></w:sectPr>"#))?;
    doc.settings.compat_mode = 15;
    Some(doc)
}

#[test]
fn compat_15_draws_a_vertical_table_over_an_earlier_one_when_moving_it_across_would_cross_the_page_edge() {
    let on_first = |w2: u32| {
        let body = format!("{}{}{}{}{}", para("pa", ""), exact_table("r", &[263; 25], 5000), para("pb", ""), exact_table("s", &[263; 30], w2), para("pc", ""));
        let cells = cells_on(&inch_section(&body)?, 0);
        Some(cells.iter().filter(|c| c.0.starts_with('s')).map(|c| c.2).collect::<Vec<f64>>())
    };
    // Across past the earlier table its far distance would cross the page's edge: it stays at its own place, all its
    // rows from the line start.
    let Some(s) = on_first(5100) else { return };
    assert!(s.len() == 30 && s[0] < 90.0, "{s:?}");
    // A narrower one would fit the page there but not the margins: the next page.
    assert!(on_first(4000).unwrap().is_empty());
}

#[test]
fn compat_15_splits_a_vertical_table_longer_than_the_line_once_moved_across() {
    let body = format!("{}{}{}{}{}", para("pa", ""), exact_table("r", &[263; 12], 5000), para("pb", ""), exact_table("s", &[263; 50], 3000), para("pc", ""));
    let Some(doc) = inch_section(&body) else { return };
    assert_eq!(cells_on(&doc, 0).iter().filter(|c| c.0.starts_with('s')).count(), 49);
}

#[test]
fn compat_15_takes_a_vertical_table_along_with_the_lines_before_it_that_jumped_an_earlier_one() {
    // The earlier table fills the line: the lines before the next one go across past it, and the next one follows
    // them, pushing only the line that meets it.
    let body = format!("{}{}{}{}{}", para("pa", ""), exact_table("r", &[263; 49], 4000), para("pb", ""), exact_table("s", &[263; 8], 4000), para("pc", ""));
    let Some(doc) = inch_section(&body) else { return };
    let runs = turned_runs(&doc);
    let at = |t: &str| runs.iter().find(|r| r.0 == t).map(|r| r.1).expect("a line");
    let s0 = cells_on(&doc, 0).iter().find(|c| c.0 == "s0").map(|c| c.1).expect("s0 on the first page");
    assert!((at("pa")[1] - 72.0).abs() < 0.01 && at("pb")[1] > 170.0, "{runs:?}");
    assert!(s0 < at("pb")[0] - 200.0 && s0 > at("pb")[0] - 230.0, "{s0} {runs:?}");
}

#[test]
fn compat_12_breaks_a_word_too_long_for_the_piece_beside_a_vertical_table_and_compat_15_moves_it_across() {
    // The table runs the line but for about 440 tw at its end.
    let body = format!("{}{}", exact_table("r", &[500; 25], 1500), para("pc amber1 garden2", ""));
    let runs = |mode: i64| {
        let mut doc = inch_section(&body)?;
        doc.settings.compat_mode = mode;
        Some(turned_runs(&doc))
    };
    let Some(c12) = runs(12) else { return };
    // Its only piece: "amber1" breaks after the characters that fit there.
    let am = c12.iter().find(|r| r.0.starts_with("am")).expect("a line");
    assert!(am.0.len() < 6 && am.1[1] > 690.0, "{c12:?}");
    // Compat 15 doesn't break it: the line goes across past the table, from the line start.
    let c15 = runs(15).unwrap();
    assert!(c15.iter().any(|r| r.0.starts_with("amber1") && (r.1[1] - 72.0).abs() < 0.01), "{c15:?}");
}

#[test]
fn compat_12_places_a_vertical_table_after_lines_that_jumped_an_earlier_one_by_their_room() {
    let pc = "pc amber1 garden2 quietly3 holds4 seven5 bright6 copper7 lanterns8 over9 narrow10 wooden11 bridges12";
    let fill = |pa: &str, w1: u32, w2: u32| {
        // Cell margins of 108 tw: compat 12 moves each table 5.4pt back.
        let mar = r#"<w:tblPr><w:tblCellMar><w:left w:w="108" w:type="dxa"/><w:right w:w="108" w:type="dxa"/></w:tblCellMar>"#;
        let table = |p: &str, rows: usize, w: u32| exact_table(p, &vec![263; rows], w).replacen("<w:tblPr>", mar, 1);
        let body = format!("{}{}{}{}{}", if pa.is_empty() { String::new() } else { para(pa, "") }, table("r", 49, w1), para("pb", ""), table("s", 8, w2), para(pc, ""));
        let mut doc = inch_section(&body)?;
        doc.settings.compat_mode = 12;
        let s0 = cells_on(&doc, 0).iter().find(|c| c.0 == "s0").map(|c| c.1);
        Some((s0, turned_runs(&doc)))
    };
    let at = |runs: &[(String, [f64; 2], [f64; 2])], t: &str| runs.iter().find(|r| r.0 == t).map(|r| r.1).expect("a line");
    // As wide as the earlier table: it stays after the lines, those that meet it go below it.
    let Some((s0, runs)) = fill("pa", 3000, 3000) else { return };
    assert!(s0.is_some() && at(&runs, "pb")[1] > 170.0, "{s0:?} {runs:?}");
    // Narrower, the earlier table's far edge past the middle across: the lines go across past it again.
    let (s0, runs) = fill("pa", 4000, 3000).unwrap();
    let s0 = s0.expect("on the first page");
    assert!((at(&runs, "pb")[1] - 72.0).abs() < 0.01 && at(&runs, "pb")[0] < s0 - 9.0, "{s0} {runs:?}");
    // Wider still: it goes to the next page and the lines stay past the earlier table.
    let (s0, runs) = fill("pa", 5000, 3000).unwrap();
    assert!(s0.is_none() && (at(&runs, "pb")[1] - 72.0).abs() < 0.01, "{runs:?}");
    // After an earlier table that starts the stack it stays, the text after it below it.
    let (s0, runs) = fill("", 4000, 3000).unwrap();
    assert!(s0.is_some() && runs.iter().any(|r| r.0.starts_with("pc") && r.1[1] > 170.0), "{s0:?} {runs:?}");
}
