//! Table shading, and Borders and Shading's values over tables

use super::super::Editor;
use super::super::commands::{Command, Outcome};
use super::super::selection::{CellRef, CellShape, Selection};
use super::fonts;
use crate::api::command::{ShadingChange, ShadingPattern, ShadingTarget};
use crate::api::services::BordersTab;
use crate::api::value::{AutoOr, Color, NoneOr, Points};
use crate::layout::Unit;

/// A 2 x 2 table in `Striped` (red stripes over yellow in its cells) with red double 2 pt borders of its own, then a
/// table in `Tinted` (a table shading) whose row has a striped exception.
fn package() -> Vec<u8> {
    let blank = crate::docx::blank::blank_docx();
    let styles = crate::docx::package::read(&blank, &[], "word/styles.xml").expect("reads").expect("styles").replace(
        "</w:styles>",
        r#"<w:style w:type="table" w:styleId="Striped"><w:name w:val="Striped"/><w:tcPr><w:shd w:val="horzStripe" w:color="FF0000" w:fill="FFFF00"/></w:tcPr></w:style><w:style w:type="table" w:styleId="Tinted"><w:name w:val="Tinted"/><w:tblPr><w:shd w:val="clear" w:color="auto" w:fill="D9E2F3"/></w:tblPr></w:style></w:styles>"#,
    );
    let cell = |t: &str| format!("<w:tc><w:tcPr><w:tcW w:w=\"2000\" w:type=\"dxa\"/></w:tcPr><w:p><w:r><w:t>{t}</w:t></w:r></w:p></w:tc>");
    let grid = r#"<w:tblGrid><w:gridCol w:w="2000"/><w:gridCol w:w="2000"/></w:tblGrid>"#;
    let border = |n: &str| format!(r#"<w:{n} w:val="double" w:sz="16" w:space="0" w:color="FF0000"/>"#);
    let edges: String = ["top", "left", "bottom", "right", "insideH", "insideV"].iter().map(|n| border(n)).collect();
    let body = format!(
        r#"<w:tbl><w:tblPr><w:tblStyle w:val="Striped"/><w:tblW w:w="4000" w:type="dxa"/><w:tblBorders>{edges}</w:tblBorders></w:tblPr>{grid}<w:tr>{}{}</w:tr><w:tr>{}{}</w:tr></w:tbl><w:p><w:r><w:t>between</w:t></w:r></w:p><w:tbl><w:tblPr><w:tblStyle w:val="Tinted"/><w:tblW w:w="2000" w:type="dxa"/></w:tblPr><w:tblGrid><w:gridCol w:w="2000"/></w:tblGrid><w:tr><w:tblPrEx><w:shd w:val="horzStripe" w:color="00FF00" w:fill="FFFFFF"/></w:tblPrEx>{}</w:tr></w:tbl><w:p/>"#,
        cell("a"),
        cell("b"),
        cell("c"),
        cell("d"),
        cell("e")
    );
    let doc = crate::docx::package::read(&blank, &[], "word/document.xml").expect("reads").expect("document").replacen("<w:p/>", &body, 1);
    crate::docx::write::repackage(
        &blank,
        &[("word/styles.xml".into(), styles.into_bytes()), ("word/document.xml".into(), doc.into_bytes())],
        zip::CompressionMethod::Deflated,
    )
    .expect("repackages")
}

fn open() -> Editor {
    Editor::open(&package(), fonts()).expect("opens")
}

fn tables(e: &Editor) -> Vec<&crate::docx::Table> {
    e.document().sections[0].blocks.iter().filter_map(|b| if let crate::docx::Block::Table(t) = b { Some(t) } else { None }).collect()
}

fn select(e: &mut Editor, from: usize, to: usize) {
    let (a, b) = (super::para_id(e, from), super::para_id(e, to));
    e.select_between((a, Unit(0)), (b, Unit(0)), fonts());
}

fn shade(target: ShadingTarget, fill: u32) -> Command {
    Command::SetShading(ShadingChange { target, fill: NoneOr::Value(Color(fill)), pattern: ShadingPattern::Clear, color: AutoOr::Auto })
}

#[test]
fn resetting_or_replacing_a_row_s_exceptions_drops_its_stripes() {
    for command in [
        serde_json::json!({ "type": "setRowProperties", "exceptions": { "reset": true } }),
        serde_json::json!({ "type": "setRowProperties", "exceptions": { "shading": "#112233" } }),
    ] {
        let mut e = open();
        assert!(tables(&e)[1].rows[0].hatch.is_some(), "read as striped");
        select(&mut e, 5, 5);
        let out = e.execute(serde_json::from_value(command.clone()).expect("a command"), fonts());
        assert!(matches!(out, Outcome::Changed(_)), "{command}: {out:?}");
        assert_eq!(tables(&e)[1].rows[0].hatch, None, "{command}");
    }
}

#[test]
fn borders_and_shading_open_on_a_block_of_cells_with_the_tables_pen_and_the_styles_stripes() {
    let mut e = open();
    let table = e.index().table_ids().find(|t| e.index().table(e.document(), *t).is_some_and(|t| t.rows.len() == 2)).expect("the 2 x 2 table");
    e.set_selection(Selection::Cells { table, anchor: CellRef { row: 0, col: 0 }, focus: CellRef { row: 1, col: 1 }, shape: CellShape::Rect });
    assert!(e.cells_selected());
    let v = e.table_borders_shading(ShadingTarget::Cell).expect("the dialog opens on cells");
    assert_eq!(
        (v.style, v.color, v.width),
        (crate::api::command::BorderStyle::Double, AutoOr::Value(Color(0xFF0000)), Points(2.0)),
        "the edges' pen"
    );
    assert_eq!((v.pattern, v.pattern_color, v.fill), (ShadingPattern::HorzStripe, AutoOr::Value(Color(0xFF0000)), NoneOr::Value(Color(0xFFFF00))));
    // The same through the paragraph-based values, which a block of cells has none of.
    assert!(e.borders_shading_values(None, false).is_some());
}

#[test]
fn a_table_shading_the_style_already_has_is_still_written() {
    let mut e = open();
    select(&mut e, 5, 5);
    let out = e.execute(shade(ShadingTarget::Table, 0xD9E2F3), fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    let saved = crate::docx::write::save(e.document()).expect("saves");
    let doc = crate::docx::package::read(&saved, &[], "word/document.xml").expect("reads").expect("document");
    assert!(doc.contains("D9E2F3"), "the table's own shading is written: {doc}");
}

#[test]
fn a_table_style_takes_cell_and_table_shading_in_its_own_xml_and_the_document_stays() {
    let mut e = open();
    select(&mut e, 0, 0);
    let (revision, before) = (e.revision(), tables(&e)[0].rows[0].cells[0].shading);
    let raw = e.document().resolver.as_ref().expect("styles").styles.get("Striped").expect("the style").raw.clone();
    let xml = e.format_style(&raw, vec![shade(ShadingTarget::Cell, 0x00B050), shade(ShadingTarget::Table, 0x112233)], fonts()).expect("formats it");
    assert!(xml.contains("00B050") && xml.contains("112233") && xml.contains("<w:tblPr>"), "{xml}");
    // A table style however its type is written.
    let w = r#"xmlns:x="http://schemas.openxmlformats.org/wordprocessingml/2006/main""#;
    let quoted = raw.replacen(r#"w:type="table""#, "w:type='table'", 1);
    let prefixed = raw
        .replacen("<w:style ", &format!("<x:style {w} "), 1)
        .replacen(r#"w:type="table""#, r#"x:type="table""#, 1)
        .replace("</w:style>", "</x:style>");
    for raw in [quoted, prefixed] {
        let xml = e.format_style(&raw, vec![shade(ShadingTarget::Cell, 0x00B050)], fonts()).expect("formats it");
        assert!(xml.contains("00B050"), "{xml}");
    }
    assert_eq!((e.revision(), tables(&e)[0].rows[0].cells[0].shading), (revision, before), "the document's table didn't change");
    let para = e.document().resolver.as_ref().expect("styles").styles.get("Normal").expect("Normal").raw.clone();
    assert!(e.format_style(&para, vec![shade(ShadingTarget::Cell, 0x00B050)], fonts()).is_err(), "a paragraph style has no cell shading");
    assert_eq!(e.revision(), revision);
}

#[test]
fn the_page_border_tab_keeps_its_pen_in_a_table() {
    let mut e = open();
    select(&mut e, 0, 0);
    let mut v = e.table_borders_shading(ShadingTarget::Cell).expect("values");
    let enabled = |v: &crate::api::services::BordersShadingDialog| {
        let m = crate::services::dialogs::borders_shading_model(v.clone(), true);
        ["style", "color", "width"].map(|k| m.fields[k].enabled)
    };
    assert_eq!(enabled(&v), [false; 3], "cell edges have no pen of their own");
    v.tab = BordersTab::PageBorder;
    assert_eq!(enabled(&v), [true; 3], "a page border's lines do");
}

#[test]
fn a_fill_alone_keeps_the_pattern_the_style_gives() {
    let package = package();
    let read = |name: &str| crate::docx::package::read(&package, &[], name).expect("reads").expect("a part");
    let styles = read("word/styles.xml")
        .replace(r#"<w:shd w:val="clear" w:color="auto" w:fill="D9E2F3"/>"#, r#"<w:shd w:val="diagStripe" w:color="0000FF" w:fill="D9E2F3"/>"#)
        .replace(
            "</w:styles>",
            r#"<w:style w:type="paragraph" w:styleId="Banded"><w:name w:val="Banded"/><w:pPr><w:shd w:val="thinHorzStripe" w:color="0000FF" w:fill="FFFFFF"/></w:pPr></w:style></w:styles>"#,
        );
    let doc = read("word/document.xml").replace("<w:p><w:r><w:t>between", r#"<w:p><w:pPr><w:pStyle w:val="Banded"/></w:pPr><w:r><w:t>between"#);
    let package = crate::docx::write::repackage(
        &package,
        &[("word/styles.xml".into(), styles.into_bytes()), ("word/document.xml".into(), doc.into_bytes())],
        zip::CompressionMethod::Deflated,
    )
    .expect("repackages");
    let mut e = Editor::open(&package, fonts()).expect("opens");
    let fill = |target| {
        Command::SetShading(ShadingChange { target, fill: NoneOr::Value(Color(0x00B050)), pattern: ShadingPattern::Other, color: AutoOr::Auto })
    };
    for (para, target) in [(0, ShadingTarget::Cell), (4, ShadingTarget::Paragraph), (5, ShadingTarget::Table)] {
        select(&mut e, para, para);
        let out = e.execute(fill(target), fonts());
        assert!(matches!(out, Outcome::Changed(_)), "{target:?}: {out:?}");
    }
    let saved = crate::docx::write::save(e.document()).expect("saves");
    let xml = crate::docx::package::read(&saved, &[], "word/document.xml").expect("reads").expect("document");
    for pattern in ["horzStripe", "thinHorzStripe", "diagStripe"] {
        assert!(xml.contains(&format!(r#"<w:shd w:val="{pattern}" w:color="auto" w:fill="00B050"/>"#)), "{pattern}: {xml}");
    }
}

#[test]
fn borders_and_shading_show_a_rows_own_borders() {
    let cell = |t: &str| format!("<w:tc><w:tcPr><w:tcW w:w=\"2000\" w:type=\"dxa\"/></w:tcPr><w:p><w:r><w:t>{t}</w:t></w:r></w:p></w:tc>");
    let xml = format!(
        r#"<w:tbl><w:tblPr><w:tblW w:w="4000" w:type="dxa"/></w:tblPr><w:tblGrid><w:gridCol w:w="2000"/><w:gridCol w:w="2000"/></w:tblGrid><w:tr>{}{}</w:tr><w:tr><w:tblPrEx><w:tblBorders><w:bottom w:val="single" w:sz="12" w:space="0" w:color="FF0000"/></w:tblBorders></w:tblPrEx>{}{}</w:tr></w:tbl><w:p/>"#,
        cell("a"),
        cell("b"),
        cell("c"),
        cell("d")
    );
    let mut e = Editor::open(&super::blank_body(&xml), fonts()).expect("opens");
    let table = e.index().table_ids().next().expect("a table");
    e.set_selection(Selection::Cells { table, anchor: CellRef { row: 1, col: 0 }, focus: CellRef { row: 1, col: 0 }, shape: CellShape::Rect });
    let v = e.table_borders_shading(ShadingTarget::Cell).expect("the dialog opens");
    assert!(v.bottom.is_some(), "the row's own bottom border: {:?}", v.bottom);
    e.set_selection(Selection::Cells { table, anchor: CellRef { row: 0, col: 0 }, focus: CellRef { row: 0, col: 0 }, shape: CellShape::Rect });
    assert!(e.table_borders_shading(ShadingTarget::Cell).expect("the dialog opens").bottom.is_none(), "the first row has none");
}

#[test]
fn borders_and_shading_show_a_merged_cells_last_row_bottom() {
    let cell = |merge: &str, t: &str| {
        format!("<w:tc><w:tcPr><w:tcW w:w=\"2000\" w:type=\"dxa\"/>{merge}</w:tcPr><w:p><w:r><w:t>{t}</w:t></w:r></w:p></w:tc>")
    };
    let borders = r#"<w:tblBorders><w:top w:val="single" w:sz="8" w:space="0" w:color="000000"/><w:bottom w:val="single" w:sz="8" w:space="0" w:color="000000"/></w:tblBorders>"#;
    let xml = format!(
        r#"<w:tbl><w:tblPr><w:tblW w:w="4000" w:type="dxa"/>{borders}</w:tblPr><w:tblGrid><w:gridCol w:w="2000"/><w:gridCol w:w="2000"/></w:tblGrid><w:tr>{}{}</w:tr><w:tr>{}{}</w:tr></w:tbl><w:p/>"#,
        cell(r#"<w:vMerge w:val="restart"/>"#, "a"),
        cell("", "b"),
        cell("<w:vMerge/>", ""),
        cell("", "d")
    );
    let mut e = Editor::open(&super::blank_body(&xml), fonts()).expect("opens");
    let table = e.index().table_ids().next().expect("a table");
    for row in [0, 1] {
        e.set_selection(Selection::Cells { table, anchor: CellRef { row, col: 0 }, focus: CellRef { row, col: 0 }, shape: CellShape::Rect });
        let v = e.table_borders_shading(ShadingTarget::Cell).expect("the dialog opens");
        assert!(v.top.is_some() && v.bottom.is_some(), "row {row}: the merged cell's outer edges: {:?} {:?}", v.top, v.bottom);
    }
}
