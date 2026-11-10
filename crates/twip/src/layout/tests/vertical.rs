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
    // The first line against the content's right (1440 + 1500 - 108 twips), its ascent in.
    let first = runs[0].1;
    assert!((first[0] - (146.0 - 10.49)).abs() < 0.3, "{first:?}");
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
    // The first baseline its ascent right of the content's left (1440 + 108 twips).
    assert!((runs[0].1[0] - (72.0 + 10.49)).abs() < 0.3, "{runs:?}");
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
