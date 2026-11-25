//! Table read-back and table tools

use std::io::Read;

use super::super::Editor;
use super::super::commands::{Command, Outcome, RowExceptionsEdit};
use super::super::readback::ShadingValue;
use super::super::selection::{CellRef, CellShape, Selection};
use super::super::table_readback::TableFit;
use super::{blank_body, fonts};
use crate::api::command::{BorderStyle, ShadingPattern};
use crate::api::drawing::ColourValue;
use crate::api::state::Tri;
use crate::api::value::{AutoOr, HorizontalAlignment, NoneOr, Points};
use crate::docx::{Block, Table};

fn doc_xml(e: &Editor) -> String {
    let bytes = crate::docx::write::save(e.document()).expect("saves");
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut s = String::new();
    z.by_name("word/document.xml").unwrap().read_to_string(&mut s).unwrap();
    s
}

fn open(body: &str) -> Editor {
    let mut e = Editor::open(&blank_body(body), fonts()).expect("opens");
    e.complete(fonts());
    e
}

fn reopened(e: &Editor) -> Editor {
    let bytes = crate::docx::write::save(e.document()).expect("saves");
    let mut e = Editor::open(&bytes, fonts()).expect("reopens");
    e.complete(fonts());
    e
}

fn table(e: &Editor) -> &Table {
    e.document().sections[0].blocks.iter().find_map(|b| if let Block::Table(t) = b { Some(t) } else { None }).expect("a table")
}

fn cells(e: &mut Editor, a: (usize, usize), f: (usize, usize)) {
    let table = crate::layout::TableId(table(e).id);
    e.set_selection(Selection::Cells {
        table,
        anchor: CellRef { row: a.0, col: a.1 },
        focus: CellRef { row: f.0, col: f.1 },
        shape: CellShape::Rect,
    });
}

fn tc(extra: &str, text: &str) -> String {
    format!("<w:tc><w:tcPr><w:tcW w:w=\"1440\" w:type=\"dxa\"/>{extra}</w:tcPr><w:p><w:r><w:t>{text}</w:t></w:r></w:p></w:tc>")
}

/// A 2×2 table with single ½ pt table borders.
fn grid2(extra: [&str; 4], tbl: &str, rows: [&str; 2]) -> String {
    let line = |n: &str| format!("<w:{n} w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"000000\"/>");
    let borders: String = ["top", "left", "bottom", "right", "insideH", "insideV"].map(line).concat();
    format!(
        "<w:tbl><w:tblPr><w:tblW w:w=\"0\" w:type=\"auto\"/><w:tblBorders>{borders}</w:tblBorders>{tbl}</w:tblPr><w:tblGrid><w:gridCol w:w=\"1440\"/><w:gridCol w:w=\"1440\"/></w:tblGrid><w:tr>{}{}{}</w:tr><w:tr>{}{}{}</w:tr></w:tbl><w:p/>",
        rows[0],
        tc(extra[0], "a"),
        tc(extra[1], "b"),
        rows[1],
        tc(extra[2], "c"),
        tc(extra[3], "d"),
    )
}

#[test]
fn borders_read_back_per_edge_inside_and_diagonal() {
    let thick = r#"<w:tcBorders><w:bottom w:val="double" w:sz="12" w:space="0" w:color="FF0000"/><w:tl2br w:val="single" w:sz="8" w:space="0" w:color="auto"/></w:tcBorders>"#;
    let mut e = open(&grid2([thick, "", "", ""], "", ["", ""]));
    cells(&mut e, (0, 0), (1, 1));
    let r = e.table_readback().expect("in a table");
    let single = |b: &Tri<NoneOr<crate::editing::readback::BorderLine>>| matches!(b, Tri::Uniform(NoneOr::Value(l)) if l.style == BorderStyle::Single && l.width == Points(0.5));
    assert!(single(&r.cell_borders.top) && single(&r.cell_borders.left) && single(&r.cell_borders.bottom) && single(&r.cell_borders.right));
    assert_eq!(r.cell_borders.inside_horizontal, Tri::Mixed, "a's bottom is double, b's the table's line");
    assert!(single(&r.cell_borders.inside_vertical));
    assert_eq!(r.cell_borders.diagonal_down, Tri::Mixed);
    assert_eq!(r.cell_borders.diagonal_up, Tri::Uniform(NoneOr::None));
    assert!(r.table_borders.inside_horizontal.is_some() && r.table_borders.top.is_some());
    // One cell: its own bottom and diagonal, no inside lines.
    cells(&mut e, (0, 0), (0, 0));
    let r = e.table_readback().expect("in a table");
    assert!(matches!(&r.cell_borders.bottom, Tri::Uniform(NoneOr::Value(l)) if l.style == BorderStyle::Double && l.width == Points(1.5)));
    assert!(matches!(&r.cell_borders.diagonal_down, Tri::Uniform(NoneOr::Value(l)) if l.width == Points(1.0)));
    assert_eq!(r.cell_borders.inside_horizontal, Tri::Unavailable);
    assert_eq!(r.cell_borders.inside_vertical, Tri::Unavailable);
}

#[test]
fn shading_reads_back_its_pattern_for_cells_table_and_rows() {
    let shd = r#"<w:shd w:val="pct25" w:color="0000FF" w:fill="FFFF00"/>"#;
    let ex = r#"<w:tblPrEx><w:shd w:val="horzStripe" w:color="00FF00" w:fill="auto"/></w:tblPrEx>"#;
    let mut e = open(&grid2([shd, shd, "", ""], r#"<w:shd w:val="clear" w:color="auto" w:fill="DDDDDD"/>"#, [ex, ""]));
    cells(&mut e, (0, 0), (0, 1));
    let r = e.table_readback().expect("in a table");
    let Tri::Uniform(NoneOr::Value(s)) = &r.cell_shading else { panic!("{:?}", r.cell_shading) };
    assert_eq!(
        (s.pattern, &s.fill, &s.color),
        (
            ShadingPattern::Pct25,
            &NoneOr::Value(ColourValue::rgb(crate::api::value::Color(0xFFFF00))),
            &AutoOr::Value(ColourValue::rgb(crate::api::value::Color(0x0000FF)))
        )
    );
    assert_eq!(r.table_shading.as_ref().map(|s| s.fill.clone()), Some(NoneOr::Value(ColourValue::rgb(crate::api::value::Color(0xDDDDDD)))));
    assert!(matches!(&r.row_exceptions.shading, Tri::Uniform(NoneOr::Value(s)) if s.pattern == ShadingPattern::HorzStripe));
    assert_eq!(r.row_exceptions.any, Tri::Uniform(true));
    cells(&mut e, (0, 0), (1, 0));
    let r = e.table_readback().expect("in a table");
    assert_eq!(r.cell_shading, Tri::Mixed);
    assert_eq!(r.row_exceptions.any, Tri::Mixed);
}

#[test]
fn margins_hide_mark_fit_and_spans_read_back() {
    let mar = r#"<w:tcMar><w:left w:w="200" w:type="dxa"/></w:tcMar><w:hideMark/>"#;
    let tbl = r#"<w:tblStyleRowBandSize w:val="2"/><w:tblLayout w:type="fixed"/><w:tblCellMar><w:top w:w="40" w:type="dxa"/><w:left w:w="100" w:type="dxa"/><w:right w:w="100" w:type="dxa"/></w:tblCellMar>"#;
    let mut e = open(&grid2([mar, "", "", ""], tbl, ["", ""]));
    cells(&mut e, (0, 0), (0, 1));
    let r = e.table_readback().expect("in a table");
    assert_eq!(r.cell_margins.top, Tri::Uniform(Points(2.0)));
    assert_eq!(r.cell_margins.left, Tri::Mixed);
    assert_eq!(r.cell_margins.right, Tri::Uniform(Points(5.0)));
    assert_eq!((r.default_cell_margins.top, r.default_cell_margins.left, r.default_cell_margins.bottom), (Points(2.0), Points(5.0), Points(0.0)));
    assert_eq!(r.hide_mark, Tri::Mixed);
    assert_eq!(r.fit, TableFit::Fixed);
    assert_eq!(r.allow_overlap, None);
    assert_eq!((r.column_span, r.row_span), (1, 1));
}

#[test]
fn a_merged_cell_reads_back_its_spans() {
    let span = r#"<w:gridSpan w:val="2"/><w:vMerge w:val="restart"/>"#;
    let body = format!(
        "<w:tbl><w:tblPr><w:tblW w:w=\"5000\" w:type=\"pct\"/></w:tblPr><w:tblGrid><w:gridCol w:w=\"1440\"/><w:gridCol w:w=\"1440\"/></w:tblGrid><w:tr>{}</w:tr><w:tr>{}</w:tr></w:tbl><w:p/>",
        tc(span, "a"),
        tc(r#"<w:gridSpan w:val="2"/><w:vMerge/>"#, "")
    );
    let mut e = open(&body);
    cells(&mut e, (1, 1), (1, 1));
    let r = e.table_readback().expect("in a table");
    assert_eq!((r.column_span, r.row_span), (2, 2));
    assert_eq!(r.fit, TableFit::Window);
}

#[test]
fn table_default_margins_set_save_and_undo() {
    let mut e = open(&grid2(["", "", "", ""], "", ["", ""]));
    cells(&mut e, (0, 0), (0, 0));
    assert!(matches!(e.set_table_cell_margins([Some(Points(3.0)), Some(Points(9.0)), None, None], fonts()), Outcome::Changed(_)));
    assert_eq!(table(&e).rows[1].cells[1].margins, [3.0, 9.0, 0.0, 0.5], "cells without their own take them");
    let xml = doc_xml(&e);
    assert!(xml.contains(r#"<w:tblCellMar><w:top w:w="60" w:type="dxa"/><w:left w:w="180" w:type="dxa"/>"#), "{xml}");
    let mut again = reopened(&e);
    cells(&mut again, (0, 0), (0, 0));
    assert_eq!(again.table_readback().map(|r| r.default_cell_margins.left), Some(Points(9.0)));
    assert!(matches!(e.set_table_cell_margins([Some(Points(-1.0)), None, None, None], fonts()), Outcome::Refused(_)));
    e.execute(Command::Undo, fonts());
    assert_eq!(table(&e).rows[1].cells[1].margins, [0.0, 0.5, 0.0, 0.5]);
}

#[test]
fn overlap_sets_and_saves() {
    let pos = r#"<w:tblpPr w:leftFromText="180" w:rightFromText="180" w:vertAnchor="text" w:tblpY="1"/>"#;
    let mut e = open(&grid2(["", "", "", ""], pos, ["", ""]));
    cells(&mut e, (0, 0), (0, 0));
    assert_eq!(e.table_readback().and_then(|r| r.allow_overlap), Some(true));
    assert!(matches!(e.set_table_overlap(false, fonts()), Outcome::Changed(_)));
    let xml = doc_xml(&e);
    assert!(xml.contains(r#"<w:tblOverlap w:val="never"/>"#), "{xml}");
    let mut again = reopened(&e);
    cells(&mut again, (0, 0), (0, 0));
    let r = again.table_readback().expect("in a table");
    assert_eq!(r.allow_overlap, Some(false));
    let mut inline = open(&grid2(["", "", "", ""], "", ["", ""]));
    cells(&mut inline, (0, 0), (0, 0));
    assert!(matches!(inline.set_table_overlap(false, fonts()), Outcome::Refused(_)));
}

#[test]
fn row_exceptions_take_an_indent_and_a_patterned_shading() {
    let mut e = open(&grid2(["", "", "", ""], "", ["", ""]));
    cells(&mut e, (1, 0), (1, 1));
    let x = RowExceptionsEdit {
        alignment: Some(HorizontalAlignment::Left),
        indent: Some(Points(18.0)),
        shading: Some(ShadingValue {
            fill: NoneOr::Value(ColourValue::rgb(crate::api::value::Color(0xFFEEDD))),
            pattern: ShadingPattern::DiagCross,
            color: AutoOr::Value(ColourValue::rgb(crate::api::value::Color(0x112233))),
        }),
        ..RowExceptionsEdit::default()
    };
    assert!(matches!(e.set_row_exceptions(None, Some(&x), fonts()), Outcome::Changed(_)));
    assert_eq!(table(&e).rows[1].ind, Some(18.0));
    let r = e.table_readback().expect("in a table");
    assert_eq!(r.row_exceptions.indent, Tri::Uniform(Some(Points(18.0))));
    assert_eq!(r.row_exceptions.alignment, Tri::Uniform(Some(HorizontalAlignment::Left)));
    assert!(matches!(&r.row_exceptions.shading, Tri::Uniform(NoneOr::Value(s)) if s.pattern == ShadingPattern::DiagCross));
    let xml = doc_xml(&e);
    let ex = &xml[xml.find("<w:tblPrEx>").expect("row exceptions")..];
    assert!(ex.contains(r#"<w:tblInd w:w="360" w:type="dxa"/>"#) && ex.contains(r#"w:val="diagCross""#), "{xml}");
    let mut again = reopened(&e);
    cells(&mut again, (1, 0), (1, 0));
    assert_eq!(again.table_readback().map(|r| r.row_exceptions.indent), Some(Tri::Uniform(Some(Points(18.0)))));
}

fn texts(e: &Editor) -> Vec<Vec<String>> {
    let text = |c: &crate::docx::Cell| {
        c.blocks.iter().map(|b| if let Block::Para(p) = b { crate::editing::text::UnitText::of(p).text } else { String::new() }).collect::<String>()
    };
    table(e).rows.iter().map(|r| r.cells.iter().map(text).collect()).collect()
}

#[test]
fn insert_cells_shift_right_grows_the_rows_at_the_selected_widths() {
    use crate::editing::table_cells::InsertShift;
    let mut e = open(&grid2(["", "", "", ""], "", ["", ""]));
    cells(&mut e, (0, 1), (0, 1));
    assert!(matches!(e.insert_cells(InsertShift::Right, fonts()), Outcome::Changed(_)));
    assert_eq!(texts(&e), [vec!["a", "", "b"], vec!["c", "d"]]);
    let t = table(&e);
    assert_eq!(t.grid, [72.0, 72.0, 72.0]);
    assert_eq!((t.rows[1].grid_after, t.rows[0].grid_after), (1, 0), "the other row keeps its width");
    let mut again = reopened(&e);
    assert_eq!(texts(&again), [vec!["a", "", "b"], vec!["c", "d"]]);
    again.execute(Command::Undo, fonts());
    e.execute(Command::Undo, fonts());
    assert_eq!(texts(&e), [vec!["a", "b"], vec!["c", "d"]]);
    assert_eq!(table(&e).grid, [72.0, 72.0]);
}

#[test]
fn insert_cells_shift_down_moves_the_column_over_a_new_row() {
    use crate::editing::table_cells::InsertShift;
    let mut e = open(&grid2(["", "", "", ""], "", ["", ""]));
    cells(&mut e, (0, 0), (0, 0));
    assert!(matches!(e.insert_cells(InsertShift::Down, fonts()), Outcome::Changed(_)));
    assert_eq!(texts(&e), [vec!["", "b"], vec!["a", "d"], vec!["c", ""]]);
    e.execute(Command::Undo, fonts());
    assert_eq!(texts(&e), [vec!["a", "b"], vec!["c", "d"]]);
}

#[test]
fn a_column_boundary_move_keeps_a_rows_grid_before() {
    let body = format!(
        "<w:tbl><w:tblPr><w:tblW w:w=\"0\" w:type=\"auto\"/></w:tblPr><w:tblGrid><w:gridCol w:w=\"1440\"/><w:gridCol w:w=\"1440\"/><w:gridCol w:w=\"1440\"/></w:tblGrid><w:tr>{}{}{}</w:tr><w:tr><w:trPr><w:gridBefore w:val=\"1\"/><w:wBefore w:w=\"1440\" w:type=\"dxa\"/></w:trPr>{}{}</w:tr></w:tbl><w:p/>",
        tc("", "a"),
        tc("", "b"),
        tc("", "c"),
        tc("", "d"),
        tc("", "e")
    );
    let mut e = open(&body);
    cells(&mut e, (1, 2), (1, 2));
    let p = match &table(&e).rows[1].cells[1].blocks[0] {
        Block::Para(p) => crate::layout::ParaId(p.id),
        Block::Table(_) => unreachable!(),
    };
    e.set_selection(Selection::Caret(crate::editing::position::Position::text(crate::editing::position::StoryId::Body, p, crate::layout::Unit(0))));
    let edges = e.table_edges().expect("the caret's row");
    let at = edges.columns[1];
    assert!(matches!(e.move_table_column_boundary(1, at + 18.0, fonts()), Outcome::Changed(_)));
    let t = table(&e);
    assert_eq!(t.rows[1].grid_before, 1, "the row still starts a column in");
    assert_eq!(t.rows[0].cells.len() + t.rows[1].cells.len(), 5);
    let again = reopened(&e);
    assert_eq!(table(&again).rows[1].grid_before, 1);
}

fn cell_rect(e: &mut Editor, row: usize, cell: usize) -> [f64; 4] {
    e.complete(fonts());
    let geo = e.page_geometries().into_iter().find(|(p, _)| *p == 0).map(|(_, g)| g).expect("page 0");
    geo.cells.iter().find(|c| c.chain.last().is_some_and(|k| (k.1, k.2) == (row, cell))).map(|c| c.rect).expect("the cell")
}

#[test]
fn the_pen_splits_a_cell_where_it_draws() {
    use crate::editing::table_draw::Stroke;
    let mut e = open(&grid2(["", "", "", ""], "", ["", ""]));
    let r = cell_rect(&mut e, 0, 0);
    let x = r[0] + 24.0;
    assert!(matches!(e.draw_table_stroke(Stroke { page: 0, from: [x, r[1] + 1.0], to: [x, r[3] - 1.0] }, fonts()), Outcome::Changed(_)));
    assert_eq!(table(&e).rows[0].cells.len(), 3, "down: two columns");
    let left = cell_rect(&mut e, 0, 0);
    assert!((left[2] - x).abs() < 0.5, "the line where it was drawn: {left:?} {x}");
    assert_eq!(table(&e).rows[1].cells.len(), 2, "the other row keeps its cells");
    e.execute(Command::Undo, fonts());
    assert_eq!(table(&e).rows[0].cells.len(), 2, "one undo step");
    let r = cell_rect(&mut e, 1, 1);
    let y = (r[1] + r[3]) / 2.0;
    assert!(matches!(e.draw_table_stroke(Stroke { page: 0, from: [r[0] + 1.0, y], to: [r[2] - 1.0, y] }, fonts()), Outcome::Changed(_)));
    assert_eq!(table(&e).rows.len(), 3, "across: two rows");
    assert!(
        matches!(e.draw_table_stroke(Stroke { page: 0, from: [1.0, 1.0], to: [10.0, 10.0] }, fonts()), Outcome::Refused(_)),
        "too small an outline"
    );
}

fn shape(e: &Editor) -> Vec<Vec<(usize, crate::docx::VMerge)>> {
    table(e).rows.iter().map(|r| r.cells.iter().map(|c| (c.span, c.v_merge)).collect()).collect()
}

#[test]
fn the_pen_splits_every_cell_its_line_crosses() {
    use crate::docx::VMerge::None as N;
    use crate::editing::table_draw::Stroke;
    let mut e = open(&grid2(["", "", "", ""], "", ["", ""]));
    let (a, b) = (cell_rect(&mut e, 0, 0), cell_rect(&mut e, 0, 1));
    let y = (a[1] + a[3]) / 2.0;
    assert!(matches!(e.draw_table_stroke(Stroke { page: 0, from: [a[0] + 1.0, y], to: [b[2] - 1.0, y] }, fonts()), Outcome::Changed(_)));
    assert_eq!(shape(&e), [vec![(1, N), (1, N)], vec![(1, N), (1, N)], vec![(1, N), (1, N)]], "a row line everywhere");
    assert_eq!(texts(&e), [vec!["a", "b"], vec!["", ""], vec!["c", "d"]]);
    e.execute(Command::Undo, fonts());
    let (a, c) = (cell_rect(&mut e, 0, 0), cell_rect(&mut e, 1, 0));
    let x = a[0] + 30.0;
    assert!(matches!(e.draw_table_stroke(Stroke { page: 0, from: [x, a[1] + 1.0], to: [x, c[3] - 1.0] }, fonts()), Outcome::Changed(_)));
    assert_eq!(shape(&e), [vec![(1, N); 3], vec![(1, N); 3]], "a column line in both rows");
    assert!((table(&e).grid[0] - 30.0).abs() < 0.6, "{:?}", table(&e).grid);
    let pen = |c: &crate::docx::Cell| c.right.as_ref().map(|b| b.style.clone());
    assert_eq!(pen(&table(&e).rows[1].cells[0]).as_deref(), Some("single"), "drawn in the pen's line");
}

#[test]
fn a_line_down_part_of_a_spanning_cell_splits_it_at_a_column_edge() {
    use crate::docx::VMerge::None as N;
    use crate::editing::table_draw::Stroke;
    let mut e = open(&table_of(2, &[&[(SPAN2, "A")], &[("", "b"), ("", "c")]]));
    let (top, below) = (cell_rect(&mut e, 0, 0), cell_rect(&mut e, 1, 0));
    let x = below[2];
    assert!(matches!(e.draw_table_stroke(Stroke { page: 0, from: [x, top[1] + 1.0], to: [x, top[3] - 1.0] }, fonts()), Outcome::Changed(_)));
    assert_eq!(shape(&e), [vec![(1, N), (1, N)], vec![(1, N), (1, N)]], "the existing column edge, no new column");
    assert_eq!(table(&e).grid.len(), 2);
}

#[test]
fn the_pen_draws_a_cells_diagonal_corner_to_corner() {
    use crate::editing::table_draw::Stroke;
    let mut e = open(&grid2(["", "", "", ""], "", ["", ""]));
    let r = cell_rect(&mut e, 0, 1);
    assert!(matches!(
        e.draw_table_stroke(Stroke { page: 0, from: [r[0] + 1.0, r[1] + 1.0], to: [r[2] - 1.0, r[3] - 1.0] }, fonts()),
        Outcome::Changed(_)
    ));
    let t = table(&e);
    assert_eq!(t.rows.len(), 2, "no split");
    let d = t.rows[0].cells[1].diagonals.as_deref().cloned().unwrap_or_default();
    assert!(d[0].is_some() && d[1].is_none(), "top left to bottom right");
    assert!(doc_xml(&e).contains("<w:tl2br w:val=\"single\""), "{}", doc_xml(&e));
    assert!(matches!(
        e.draw_table_stroke(Stroke { page: 0, from: [r[0] + 1.0, r[3] - 1.0], to: [r[2] - 1.0, r[1] + 1.0] }, fonts()),
        Outcome::Changed(_)
    ));
    assert!(table(&e).rows[0].cells[1].diagonals.as_deref().is_some_and(|d| d[1].is_some()), "bottom left to top right");
}

#[test]
fn a_pen_click_draws_nothing() {
    use crate::editing::table_draw::Stroke;
    let mut e = open(&grid2(["", "", "", ""], "", ["", ""]));
    let r = cell_rect(&mut e, 0, 0);
    let p = [(r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0];
    assert!(matches!(e.draw_table_stroke(Stroke { page: 0, from: p, to: p }, fonts()), Outcome::Unchanged(_)));
    assert!(matches!(e.draw_table_stroke(Stroke { page: 0, from: p, to: [p[0] + 1.0, p[1] + 1.0] }, fonts()), Outcome::Unchanged(_)), "a jitter");
    assert_eq!(shape(&e).len(), 2);
}

#[test]
fn the_pen_redraws_a_border_it_follows() {
    use crate::editing::table_draw::Stroke;
    let mut e = open(&grid2(["", "", "", ""], "", ["", ""]));
    e.set_table_pen(crate::docx::props::Border { style: "double".into(), width: 1.5, space: 0.0, shadow: false, color: None, color_theme: None });
    let r = cell_rect(&mut e, 0, 0);
    assert!(matches!(e.draw_table_stroke(Stroke { page: 0, from: [r[0] + 2.0, r[3]], to: [r[2] - 2.0, r[3]] }, fonts()), Outcome::Changed(_)));
    let t = table(&e);
    assert_eq!(t.rows.len(), 2, "no split");
    assert_eq!(t.rows[0].cells[0].bottom.as_ref().map(|b| b.style.as_str()), Some("double"));
    assert_eq!(t.rows[1].cells[0].top.as_ref().map(|b| b.style.as_str()), Some("double"));
    assert_ne!(t.rows[0].cells[1].bottom.as_ref().map(|b| b.style.as_str()), Some("double"), "only where it was drawn");
}

#[test]
fn a_drawn_table_over_text_leaves_the_text_alone() {
    use crate::editing::table_draw::Stroke;
    let mut e = open("<w:p><w:r><w:t>one two three four five six seven eight nine ten</w:t></w:r></w:p>");
    let line = e.page_geometries().into_iter().find(|(p, _)| *p == 0).and_then(|(_, g)| g.lines.first().map(|l| (l.top, l.bottom))).expect("a line");
    let y = (line.0 + line.1) / 2.0;
    assert!(matches!(e.draw_table_stroke(Stroke { page: 0, from: [150.0, y], to: [300.0, y + 60.0] }, fonts()), Outcome::Changed(_)));
    let paras: Vec<String> = e.document().sections[0]
        .blocks
        .iter()
        .filter_map(|b| if let Block::Para(p) = b { Some(crate::editing::text::UnitText::of(p).text) } else { None })
        .collect();
    assert_eq!(paras, ["one two three four five six seven eight nine ten"], "one paragraph, as it was");
    assert!(table(&e).floating.is_some());
}

#[test]
fn a_line_in_a_drawn_outline_keeps_the_rows_where_drawn() {
    use crate::editing::table_draw::Stroke;
    let mut e = open("<w:p><w:r><w:t>text</w:t></w:r></w:p>");
    assert!(matches!(e.draw_table_stroke(Stroke { page: 0, from: [100.0, 300.0], to: [250.0, 360.0] }, fonts()), Outcome::Changed(_)));
    let r = cell_rect(&mut e, 0, 0);
    let y = r[1] + 20.0;
    assert!(matches!(e.draw_table_stroke(Stroke { page: 0, from: [r[0] + 1.0, y], to: [r[2] - 1.0, y] }, fonts()), Outcome::Changed(_)));
    let h: Vec<(f64, String)> = table(&e).rows.iter().map(|r| r.height.clone().expect("a height")).collect();
    assert!((h[0].0 - 20.0).abs() < 0.6 && (h[1].0 - 40.0).abs() < 0.6, "{h:?}");
    assert!(h.iter().all(|x| x.1 == "atLeast"));
}

#[test]
fn the_eraser_refuses_cells_that_dont_line_up() {
    let mut e = open(&table_of(3, &[&[(SPAN2, "A"), ("", "b")], &[("", "c"), (SPAN2, "D")]]));
    let a = cell_rect(&mut e, 0, 0);
    assert!(matches!(e.erase_table_border(0, [a[0] + 10.0, a[3]], fonts()), Outcome::Refused(_)));
    assert_eq!(shape(&e).iter().map(Vec::len).collect::<Vec<_>>(), [2, 2], "nothing merged");
}

#[test]
fn the_eraser_merges_the_cells_either_side_of_a_border() {
    let mut e = open(&grid2(["", "", "", ""], "", ["", ""]));
    let r = cell_rect(&mut e, 0, 0);
    assert!(matches!(e.erase_table_border(0, [r[2], (r[1] + r[3]) / 2.0], fonts()), Outcome::Changed(_)));
    assert_eq!((table(&e).rows[0].cells.len(), table(&e).rows[0].cells[0].span), (1, 2));
    e.execute(Command::Undo, fonts());
    let r = cell_rect(&mut e, 0, 1);
    assert!(matches!(e.erase_table_border(0, [(r[0] + r[2]) / 2.0, r[3]], fonts()), Outcome::Changed(_)));
    assert_eq!(table(&e).rows[1].cells[1].v_merge, crate::docx::VMerge::Continue, "merged down");
    let r = cell_rect(&mut e, 0, 0);
    assert!(matches!(e.erase_table_border(0, [(r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0], fonts()), Outcome::Refused(_)), "inside a cell");
}

#[test]
fn the_pen_draws_a_new_table_outside_one() {
    use crate::editing::table_draw::Stroke;
    let mut e = open("<w:p><w:r><w:t>text</w:t></w:r></w:p>");
    assert!(matches!(e.draw_table_stroke(Stroke { page: 0, from: [250.0, 300.0], to: [100.0, 360.0] }, fonts()), Outcome::Changed(_)));
    let t = table(&e);
    assert_eq!((t.grid.clone(), t.rows[0].height.clone()), (vec![150.0], Some((60.0, "atLeast".to_string()))));
    let pos = t.floating.expect("positioned where it was drawn");
    assert_eq!((pos.x, pos.y, pos.h_page, pos.v_anchor), (100.0, 300.0, true, crate::docx::props::TableAnchor::Page));
    e.execute(Command::Undo, fonts());
    assert!(!e.document().sections[0].blocks.iter().any(|b| matches!(b, Block::Table(_))), "one undo step");
}

/// A table of `cols` 1440-twip columns and `rows`, each row's cells as (`tcPr` extra, text).
fn table_of(cols: usize, rows: &[&[(&str, &str)]]) -> String {
    let grid = "<w:gridCol w:w=\"1440\"/>".repeat(cols);
    let rows: String = rows.iter().map(|r| format!("<w:tr>{}</w:tr>", r.iter().map(|(x, t)| tc(x, t)).collect::<String>())).collect();
    format!("<w:tbl><w:tblPr><w:tblW w:w=\"0\" w:type=\"auto\"/></w:tblPr><w:tblGrid>{grid}</w:tblGrid>{rows}</w:tbl><w:p/>")
}

const SPAN2: &str = r#"<w:gridSpan w:val="2"/>"#;

#[test]
fn insert_cells_shift_down_refuses_columns_whose_rows_split_them_differently() {
    use crate::editing::table_cells::InsertShift;
    for rows in [[&[("", "a"), ("", "b")][..], &[(SPAN2, "C")][..]], [&[(SPAN2, "C")][..], &[("", "a"), ("", "b")][..]]] {
        let mut e = open(&table_of(2, &rows));
        let before = texts(&e);
        cells(&mut e, (0, 0), (0, 1));
        assert!(matches!(e.insert_cells(InsertShift::Down, fonts()), Outcome::Refused(_)));
        assert_eq!(texts(&e), before, "nothing lost");
    }
}

#[test]
fn insert_cells_shift_down_moves_spanning_cells_whole() {
    use crate::editing::table_cells::InsertShift;
    let mut e = open(&table_of(3, &[&[(SPAN2, "A"), ("", "b")], &[(SPAN2, "C"), ("", "d")]]));
    cells(&mut e, (0, 0), (0, 1));
    assert!(matches!(e.insert_cells(InsertShift::Down, fonts()), Outcome::Changed(_)));
    assert_eq!(texts(&e), [vec!["", "b"], vec!["A", "d"], vec!["C", ""]]);
    assert!(table(&e).rows.iter().all(|r| r.cells[0].span == 2));
}

#[test]
fn insert_cells_shift_right_keeps_vertical_merges_whole() {
    use crate::editing::table_cells::InsertShift;
    let (restart, cont) = (r#"<w:vMerge w:val="restart"/>"#, "<w:vMerge/>");
    // The caret in the merged cell: a new cell as tall beside it.
    let mut e = open(&table_of(2, &[&[(restart, "a"), ("", "b")], &[(cont, ""), ("", "d")]]));
    cells(&mut e, (0, 0), (0, 0));
    assert!(matches!(e.insert_cells(InsertShift::Right, fonts()), Outcome::Changed(_)));
    let t = table(&e);
    assert_eq!((t.rows[0].cells.len(), t.rows[1].cells.len()), (3, 3), "both rows of the merge grow");
    assert_eq!([t.rows[1].cells[0].v_merge, t.rows[1].cells[1].v_merge], [crate::docx::VMerge::Continue; 2]);
    assert_eq!(texts(&e), [vec!["", "a", "b"], vec!["", "", "d"]]);
    // A merge right of the selection moves in all its rows.
    let mut e = open(&table_of(2, &[&[("", "a"), (restart, "b")], &[("", "c"), (cont, "")]]));
    cells(&mut e, (0, 0), (0, 0));
    assert!(matches!(e.insert_cells(InsertShift::Right, fonts()), Outcome::Changed(_)));
    let t = table(&e);
    let g = crate::docx::grid::Grid::of(t);
    assert_eq!((g.rows[0][2].0, g.rows[1][2].0), (2, 2), "the merge's cells stay over each other");
    assert_eq!(t.rows[1].cells[2].v_merge, crate::docx::VMerge::Continue);
}

#[test]
fn insert_cells_shift_right_stops_at_the_column_limit() {
    use crate::editing::table_cells::InsertShift;
    let row: Vec<(&str, &str)> = vec![("", "x"); 63];
    let mut e = open(&table_of(63, &[&row]));
    cells(&mut e, (0, 0), (0, 0));
    assert!(matches!(e.insert_cells(InsertShift::Right, fonts()), Outcome::Refused(_)));
    assert_eq!(table(&e).rows[0].cells.len(), 63);
}

#[test]
fn insert_cells_track_as_inserted_cells_and_moved_text() {
    use crate::editing::table_cells::InsertShift;
    for (shift, sel, after) in [
        (InsertShift::Right, (0, 1), vec![vec!["a", "", "b"], vec!["c", "d"]]),
        (InsertShift::Down, (0, 0), vec![vec!["", "b"], vec!["a", "d"], vec!["c", ""]]),
    ] {
        for accept in [true, false] {
            let mut e = open(&grid2(["", "", "", ""], "", ["", ""]));
            e.execute(Command::SetTrackChanges(true), fonts());
            cells(&mut e, sel, sel);
            assert!(matches!(e.insert_cells(shift, fonts()), Outcome::Changed(_)));
            assert!(crate::docx::write::save(e.document()).is_ok());
            e.execute(
                if accept { Command::AcceptAllRevisions(Default::default()) } else { Command::RejectAllRevisions(Default::default()) },
                fonts(),
            );
            let want = if accept { after.clone() } else { vec![vec!["a", "b"], vec!["c", "d"]] };
            assert_eq!(texts(&e), want, "{shift:?} accept {accept}");
        }
    }
}

#[test]
fn the_pen_splits_a_vertically_merged_cell_whole() {
    use crate::docx::VMerge::{Continue as C, None as N, Restart as R};
    use crate::editing::table_draw::Stroke;
    let (restart, cont) = (r#"<w:vMerge w:val="restart"/>"#, "<w:vMerge/>");
    let mut e = open(&table_of(2, &[&[(restart, "a"), ("", "b")], &[(cont, ""), ("", "d")]]));
    let top = cell_rect(&mut e, 0, 1);
    let x = top[0] - 36.0;
    assert!(matches!(e.draw_table_stroke(Stroke { page: 0, from: [x, top[1] + 1.0], to: [x, top[3] - 1.0] }, fonts()), Outcome::Changed(_)));
    assert_eq!(shape(&e), [vec![(1, R), (1, R), (1, N)], vec![(1, C), (1, C), (1, N)]], "down the whole merged cell");
    e.execute(Command::Undo, fonts());
    // Across the merged cell where its rows meet: two cells again.
    let below = cell_rect(&mut e, 1, 1);
    let y = below[1];
    assert!(matches!(e.draw_table_stroke(Stroke { page: 0, from: [top[0] - 70.0, y], to: [top[0] - 2.0, y] }, fonts()), Outcome::Changed(_)));
    assert_eq!(shape(&e), [vec![(1, N), (1, N)], vec![(1, N), (1, N)]]);
}

#[test]
fn insert_and_delete_cells_shift_vertically_under_a_spanning_title_row() {
    use crate::editing::table_cells::InsertShift;
    let body = table_of(2, &[&[(SPAN2, "T")], &[("", "a"), ("", "b")], &[("", "c"), ("", "d")]]);
    let mut e = open(&body);
    cells(&mut e, (1, 0), (1, 0));
    assert!(matches!(e.insert_cells(InsertShift::Down, fonts()), Outcome::Changed(_)));
    assert_eq!(texts(&e), [vec!["T"], vec!["", "b"], vec!["a", "d"], vec!["c", ""]]);
    assert_eq!(table(&e).rows[0].cells[0].span, 2, "the title row stays");
    let mut e = open(&body);
    cells(&mut e, (1, 0), (1, 0));
    assert!(matches!(e.execute(Command::DeleteCells(crate::api::command::CellShift::Up), fonts()), Outcome::Changed(_)));
    assert_eq!(texts(&e), [vec!["T"], vec!["c", "b"], vec!["", "d"]]);
}

/// Where cell `(row, cell)` of page 0's header table is.
fn header_cell(e: &mut Editor, row: usize, cell: usize) -> [f64; 4] {
    e.complete(fonts());
    let geo = e.page_geometries().into_iter().find(|(p, _)| *p == 0).map(|(_, g)| g).expect("page 0");
    geo.cells
        .iter()
        .find(|c| {
            matches!(geo.stories[c.story].host, crate::editing::position::StoryId::HeaderFooter(_))
                && c.chain.last().is_some_and(|k| (k.1, k.2) == (row, cell))
        })
        .map(|c| c.rect)
        .expect("the header's cell")
}

#[test]
fn the_pen_and_eraser_leave_a_header_table_alone_while_editing_the_body() {
    use crate::editing::table_draw::Stroke;
    let sec = r#"<w:sectPr><w:headerReference w:type="default" r:id="rIdH"/><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr>"#;
    let header = grid2(["", "", "", ""], "", ["", ""]);
    let mut e = super::stories::package(&format!("<w:p><w:r><w:t>body</w:t></w:r></w:p>{sec}"), Some(Some(&header)));
    let r = header_cell(&mut e, 0, 0);
    let rev = e.revision();
    let x = r[0] + 24.0;
    let y = (r[1] + r[3]) / 2.0;
    assert!(!matches!(e.draw_table_stroke(Stroke { page: 0, from: [x, r[1] + 1.0], to: [x, r[3] - 1.0] }, fonts()), Outcome::Changed(_)));
    assert!(!matches!(e.draw_table_stroke(Stroke { page: 0, from: [r[0] + 1.0, y], to: [r[2] - 1.0, y] }, fonts()), Outcome::Changed(_)));
    assert!(!matches!(e.erase_table_border(0, [r[2], y], fonts()), Outcome::Changed(_)));
    assert_eq!(e.revision(), rev, "nothing changed");
    assert!(!e.document().sections[0].blocks.iter().any(|b| matches!(b, Block::Table(_))), "no table drawn in the body");
}

fn first_para(e: &Editor) -> crate::layout::ParaId {
    match &e.document().sections[0].blocks[0] {
        Block::Para(p) => crate::layout::ParaId(p.id),
        Block::Table(_) => unreachable!(),
    }
}

#[test]
fn the_pen_and_eraser_are_refused_where_protection_keeps_the_table() {
    use crate::api::command::{DocumentProtection, ProtectionKind};
    use crate::editing::position::{Position, StoryId};
    use crate::editing::table_draw::Stroke;
    let range = r#"<w:p><w:permStart w:id="1" w:edGrp="everyone"/><w:r><w:t>in</w:t></w:r><w:permEnd w:id="1"/></w:p>"#;
    let mut e = open(&format!("{range}{}", grid2(["", "", "", ""], "", ["", ""])));
    let read_only = DocumentProtection { kind: Some(ProtectionKind::ReadOnly), enforce: Some(true), ..Default::default() };
    e.execute(Command::SetDocumentProtection(read_only), fonts());
    let para = first_para(&e);
    e.set_selection(Selection::Caret(Position::text(StoryId::Body, para, crate::layout::Unit(1))));
    assert!(e.in_editable_range("someone"), "the caret's in the editing range");
    let r = cell_rect(&mut e, 0, 0);
    let rev = e.revision();
    let x = r[0] + 24.0;
    assert!(matches!(e.draw_table_stroke(Stroke { page: 0, from: [x, r[1] + 1.0], to: [x, r[3] - 1.0] }, fonts()), Outcome::Refused(_)));
    assert!(matches!(e.erase_table_border(0, [r[2], (r[1] + r[3]) / 2.0], fonts()), Outcome::Refused(_)));
    assert!(matches!(e.draw_table_stroke(Stroke { page: 0, from: [100.0, 500.0], to: [250.0, 560.0] }, fonts()), Outcome::Refused(_)), "a new table");
    assert_eq!(e.revision(), rev);
}

#[test]
fn the_pen_and_eraser_are_refused_in_a_contents_locked_control() {
    use crate::editing::table_draw::Stroke;
    let table = grid2(["", "", "", ""], "", ["", ""]);
    let body =
        format!(r#"<w:sdt><w:sdtPr><w:id w:val="5"/><w:lock w:val="contentLocked"/></w:sdtPr><w:sdtContent>{table}</w:sdtContent></w:sdt><w:p/>"#);
    let mut e = open(&body);
    let r = cell_rect(&mut e, 0, 0);
    let rev = e.revision();
    let x = r[0] + 24.0;
    let y = (r[1] + r[3]) / 2.0;
    assert!(matches!(e.draw_table_stroke(Stroke { page: 0, from: [x, r[1] + 1.0], to: [x, r[3] - 1.0] }, fonts()), Outcome::Refused(_)));
    assert!(matches!(e.draw_table_stroke(Stroke { page: 0, from: [r[0] + 1.0, y], to: [r[2] - 1.0, y] }, fonts()), Outcome::Refused(_)));
    assert!(matches!(e.erase_table_border(0, [r[2], y], fonts()), Outcome::Refused(_)));
    assert_eq!(e.revision(), rev);
    assert_eq!(texts(&e), [vec!["a", "b"], vec!["c", "d"]]);
}

#[test]
fn a_band_size_alone_changes_a_read_table_and_saves() {
    let mut e = open(&grid2(["", "", "", ""], "", ["", ""]));
    cells(&mut e, (0, 0), (0, 0));
    assert!(matches!(e.set_table_band_sizes(Some(Some(2)), None, fonts()), Outcome::Changed(_)));
    assert!(doc_xml(&e).contains(r#"<w:tblStyleRowBandSize w:val="2"/>"#), "{}", doc_xml(&e));
    let mut again = reopened(&e);
    cells(&mut again, (0, 0), (0, 0));
    assert_eq!(again.table_band_sizes().map(|b| b.own_rows), Some(Some(2)));
}

/// Each row's grid columns, before, cells and after, add up to the grid.
fn rows_fit_the_grid(e: &Editor) {
    let t = table(e);
    for (r, row) in t.rows.iter().enumerate() {
        let n = row.grid_before + row.cells.iter().map(|c| c.span.max(1)).sum::<usize>() + row.grid_after;
        assert_eq!(n, t.grid.len(), "row {r}: {:?} over {:?}", row.cells.iter().map(|c| c.span).collect::<Vec<_>>(), t.grid);
    }
}

fn tcw(w: u32, text: &str) -> String {
    format!("<w:tc><w:tcPr><w:tcW w:w=\"{w}\" w:type=\"dxa\"/></w:tcPr><w:p><w:r><w:t>{text}</w:t></w:r></w:p></w:tc>")
}

#[test]
fn insert_cells_keeps_a_zero_width_column_apart() {
    use crate::editing::table_cells::InsertShift;
    let body = format!(
        "<w:tbl><w:tblPr><w:tblW w:w=\"0\" w:type=\"auto\"/></w:tblPr><w:tblGrid><w:gridCol w:w=\"1440\"/><w:gridCol w:w=\"0\"/></w:tblGrid><w:tr>{}{}</w:tr></w:tbl><w:p/>",
        tcw(1440, "a"),
        tcw(0, "z")
    );
    let mut e = open(&body);
    cells(&mut e, (0, 0), (0, 0));
    assert!(matches!(e.insert_cells(InsertShift::Right, fonts()), Outcome::Changed(_)));
    assert_eq!(texts(&e), [vec!["", "a", "z"]]);
    assert_eq!(table(&e).grid, [72.0, 72.0, 0.0]);
    rows_fit_the_grid(&e);
    // Columns the grid leaves out, 0 wide: the other row keeps its cells over its columns.
    let row = format!("<w:tr>{}{}{}</w:tr>", tcw(1440, "a"), tcw(1440, "b"), tcw(0, "c"));
    let body = format!(
        "<w:tbl><w:tblPr><w:tblW w:w=\"0\" w:type=\"auto\"/></w:tblPr><w:tblGrid><w:gridCol w:w=\"1440\"/><w:gridCol w:w=\"1440\"/></w:tblGrid>{row}{}</w:tbl><w:p/>",
        row.replace(">a<", ">d<").replace(">b<", ">e<").replace(">c<", ">f<")
    );
    let mut e = open(&body);
    cells(&mut e, (0, 0), (0, 0));
    assert!(matches!(e.insert_cells(InsertShift::Right, fonts()), Outcome::Changed(_)));
    assert_eq!(texts(&e), [vec!["", "a", "b", "c"], vec!["d", "e", "f"]]);
    rows_fit_the_grid(&e);
    assert_eq!(table(&e).rows[1].cells.iter().map(|c| c.span).collect::<Vec<_>>(), [1, 1, 1]);
}

const RED: &str = r#"<w:shd w:val="clear" w:color="auto" w:fill="FF0000"/>"#;

#[test]
fn shifting_cells_vertically_moves_their_formatting() {
    use crate::editing::table_cells::InsertShift;
    let mut e = open(&grid2([RED, "", "", ""], "", ["", ""]));
    cells(&mut e, (0, 0), (0, 0));
    assert!(matches!(e.insert_cells(InsertShift::Down, fonts()), Outcome::Changed(_)));
    assert_eq!(texts(&e), [vec!["", "b"], vec!["a", "d"], vec!["c", ""]]);
    let shading = |e: &Editor, r: usize| table(e).rows[r].cells[0].shading;
    assert_eq!((shading(&e, 1), shading(&e, 2)), (Some(0xFF0000), None), "a's shading goes down with it, c's none with c");
    let mut e = open(&grid2(["", "", RED, ""], "", ["", ""]));
    cells(&mut e, (0, 0), (0, 0));
    assert!(matches!(e.execute(Command::DeleteCells(crate::api::command::CellShift::Up), fonts()), Outcome::Changed(_)));
    assert_eq!(texts(&e), [vec!["c", "b"], vec!["", "d"]]);
    assert_eq!(shading(&e, 0), Some(0xFF0000), "c's shading goes up with it");
}

#[test]
fn a_tracked_shift_down_brings_the_paragraphs_formatting() {
    use crate::editing::table_cells::InsertShift;
    let body =
        grid2(["", "", "", ""], "", ["", ""]).replace("<w:p><w:r><w:t>a</w:t>", "<w:p><w:pPr><w:jc w:val=\"center\"/></w:pPr><w:r><w:t>a</w:t>");
    for accept in [true, false] {
        let mut e = open(&body);
        e.execute(Command::SetTrackChanges(true), fonts());
        cells(&mut e, (0, 0), (0, 0));
        assert!(matches!(e.insert_cells(InsertShift::Down, fonts()), Outcome::Changed(_)));
        e.execute(if accept { Command::AcceptAllRevisions(Default::default()) } else { Command::RejectAllRevisions(Default::default()) }, fonts());
        let (r, text, jc) = if accept { (1, "a", crate::docx::props::Jc::Center) } else { (1, "c", crate::docx::props::Jc::Left) };
        assert_eq!(texts(&e)[r][0], text, "accept {accept}");
        let Some(Block::Para(p)) = table(&e).rows[r].cells[0].blocks.first() else { panic!("a paragraph") };
        assert_eq!(p.style.jc, jc, "accept {accept}");
    }
}

#[test]
fn a_tracked_shift_down_keeps_a_moved_notes_text() {
    use crate::editing::table_cells::InsertShift;
    let ns = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main""#;
    let foot = format!(r#"<w:footnotes {ns}><w:footnote w:id="1"><w:p><w:r><w:t>Note text</w:t></w:r></w:p></w:footnote></w:footnotes>"#);
    let body = grid2(["", "", "", ""], "", ["", ""]).replace("<w:t>a</w:t></w:r>", r#"<w:t>a</w:t></w:r><w:r><w:footnoteReference w:id="1"/></w:r>"#);
    let bytes = super::empty_stories::build(&body, "", &[("footnotes.xml", "footnotes", "footnotes", &foot)]);
    let mut e = Editor::open(&bytes, fonts()).expect("opens");
    e.complete(fonts());
    e.execute(Command::SetTrackChanges(true), fonts());
    cells(&mut e, (0, 0), (0, 0));
    assert!(matches!(e.insert_cells(InsertShift::Down, fonts()), Outcome::Changed(_)));
    e.execute(Command::AcceptAllRevisions(Default::default()), fonts());
    let note: String = e
        .document()
        .footnotes
        .get(&1)
        .expect("the note")
        .iter()
        .map(|b| match b {
            Block::Para(p) => crate::editing::text::UnitText::of(p).text,
            Block::Table(_) => String::new(),
        })
        .collect();
    assert!(note.contains("Note text"), "the note keeps its text: {note:?}");
    let Some(Block::Para(p)) = table(&e).rows[1].cells[0].blocks.first() else { panic!("a paragraph") };
    assert!(
        p.inlines.iter().any(|i| matches!(i, crate::docx::Inline::NoteRef { kind: crate::docx::NoteKind::Foot, id: 1, .. })),
        "the moved paragraph keeps its reference"
    );
}

#[test]
fn inside_borders_read_back_both_cells_sides() {
    let top = r#"<w:tcBorders><w:top w:val="double" w:sz="12" w:space="0" w:color="FF0000"/></w:tcBorders>"#;
    let left = r#"<w:tcBorders><w:left w:val="double" w:sz="12" w:space="0" w:color="FF0000"/></w:tcBorders>"#;
    let double =
        |b: &Tri<NoneOr<crate::editing::readback::BorderLine>>| matches!(b, Tri::Uniform(NoneOr::Value(l)) if l.style == BorderStyle::Double);
    let mut e = open(&grid2(["", "", top, top], "", ["", ""]));
    cells(&mut e, (0, 0), (1, 1));
    let r = e.table_readback().expect("in a table");
    assert!(double(&r.cell_borders.inside_horizontal), "the lower cells' tops: {:?}", r.cell_borders.inside_horizontal);
    let mut e = open(&grid2(["", left, "", left], "", ["", ""]));
    cells(&mut e, (0, 0), (1, 1));
    let r = e.table_readback().expect("in a table");
    assert!(double(&r.cell_borders.inside_vertical), "the right cells' lefts: {:?}", r.cell_borders.inside_vertical);
}

#[test]
fn the_pen_draws_down_a_rows_own_borders_where_it_is_indented() {
    use crate::editing::table_draw::Stroke;
    // A row's own indent, which Word takes in a fixed-layout row
    let ind = r#"<w:tblPrEx><w:tblInd w:w="720" w:type="dxa"/><w:tblLayout w:type="fixed"/></w:tblPrEx>"#;
    let mut e = open(&grid2(["", "", "", ""], "", ["", ind]));
    let (c, d) = (cell_rect(&mut e, 1, 0), cell_rect(&mut e, 1, 1));
    assert!(c[0] - cell_rect(&mut e, 0, 0)[0] > 30.0, "the row is indented: {c:?}");
    // Along the indented row's own border: redrawn, nothing split.
    let x = d[0];
    let out = e.draw_table_stroke(Stroke { page: 0, from: [x, d[1] + 1.0], to: [x, d[3] - 1.0] }, fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    assert_eq!((table(&e).grid.len(), table(&e).rows[1].cells.len(), table(&e).rows[0].cells.len()), (2, 2, 2));
    e.execute(Command::Undo, fonts());
    // Across its first cell where the row above has a border: split there in this row.
    let x = cell_rect(&mut e, 0, 0)[2];
    assert!(x > c[0] + 4.0 && x < c[2] - 4.0);
    assert!(matches!(e.draw_table_stroke(Stroke { page: 0, from: [x, c[1] + 1.0], to: [x, c[3] - 1.0] }, fonts()), Outcome::Changed(_)));
    assert_eq!(table(&e).rows[1].cells.len(), 3, "the indented row's cell splits");
    assert_eq!(table(&e).rows[0].cells.len(), 2, "the row above keeps its cells");
    assert!((cell_rect(&mut e, 1, 0)[2] - x).abs() < 0.5, "where it was drawn");
    rows_fit_the_grid(&e);
}

#[test]
fn a_pen_stroke_from_a_table_out_over_the_text_draws_no_new_table() {
    use crate::editing::table_draw::Stroke;
    let after: String = (0..20).map(|_| "<w:p><w:r><w:t>Body text under the table, line after line.</w:t></w:r></w:p>").collect();
    let mut e = open(&format!("{}{after}", grid2(["", "", "", ""], "", ["", ""])));
    let a = cell_rect(&mut e, 0, 0);
    let rev = e.revision();
    let out = e.draw_table_stroke(Stroke { page: 0, from: [a[0] + 5.0, a[1] + 5.0], to: [a[0] + 400.0, a[1] + 300.0] }, fonts());
    assert!(!matches!(out, Outcome::Changed(_)), "{out:?}");
    assert_eq!(e.revision(), rev, "no table drawn");
}

#[test]
fn the_pen_redraws_no_border_where_a_page_cuts_a_row() {
    use crate::editing::table_draw::Stroke;
    let long: String = (0..80).map(|k| format!("<w:p><w:r><w:t>Line {k}</w:t></w:r></w:p>")).collect();
    let body = grid2(["", "", "", ""], "", ["", ""]).replace("<w:p><w:r><w:t>c</w:t></w:r></w:p>", &long);
    let mut e = open(&body);
    let geo = e.page_geometries().into_iter().find(|(p, _)| *p == 1).map(|(_, g)| g).expect("page 1");
    let part = geo.rows.iter().find(|r| r.row == 1).map(|r| r.rect).expect("row 1 goes on");
    let rev = e.revision();
    let y = part[1];
    let out = e.draw_table_stroke(Stroke { page: 1, from: [part[0] + 2.0, y], to: [part[2] - 2.0, y] }, fonts());
    assert!(!matches!(out, Outcome::Changed(_)), "the top of a row's continuation isn't a border: {out:?}");
    assert_eq!(e.revision(), rev);
    // The rows' real border still redraws.
    let b = cell_rect(&mut e, 1, 1);
    let y = b[1];
    assert!(matches!(e.draw_table_stroke(Stroke { page: 0, from: [b[0] + 2.0, y], to: [b[2] - 2.0, y] }, fonts()), Outcome::Changed(_)));
}

#[test]
fn table_options_are_refused_where_protection_refuses_their_commands() {
    use crate::api::command::{BorderEdges, DocumentProtection, ProtectionKind};
    let pos = r#"<w:tblpPr w:leftFromText="180" w:rightFromText="180" w:vertAnchor="text" w:tblpY="1"/>"#;
    let protected = |p: DocumentProtection| {
        let mut e = open(&grid2(["", "", "", ""], pos, ["", ""]));
        assert!(matches!(e.execute(Command::SetDocumentProtection(p), fonts()), Outcome::Changed(_)));
        cells(&mut e, (0, 0), (0, 0));
        e
    };
    let x = RowExceptionsEdit { indent: Some(Points(18.0)), ..RowExceptionsEdit::default() };
    let mut e = protected(DocumentProtection { formatting: Some(true), enforce: Some(true), ..Default::default() });
    assert!(matches!(e.set_cell_border_lines(BorderEdges::All, None, fonts()), Outcome::Refused(_)), "cell borders are formatting");
    assert!(matches!(e.set_table_cell_margins([Some(Points(3.0)), None, None, None], fonts()), Outcome::Refused(_)));
    assert!(matches!(e.set_table_overlap(false, fonts()), Outcome::Changed(_)), "positioning isn't formatting");
    let mut e = protected(DocumentProtection { kind: Some(ProtectionKind::ReadOnly), enforce: Some(true), ..Default::default() });
    assert!(matches!(e.set_cell_border_lines(BorderEdges::All, None, fonts()), Outcome::Refused(_)));
    assert!(matches!(e.set_row_exceptions(None, Some(&x), fonts()), Outcome::Refused(_)));
    assert!(matches!(e.set_table_cell_margins([Some(Points(3.0)), None, None, None], fonts()), Outcome::Refused(_)));
    assert!(matches!(e.set_table_overlap(true, fonts()), Outcome::Refused(_)));
    assert!(table(&e).rows[0].ind.is_none(), "nothing changed");
}

#[test]
fn a_rows_cell_spacing_is_0_to_1584_points() {
    let mut e = open(&grid2(["", "", "", ""], "", ["", ""]));
    cells(&mut e, (0, 0), (0, 0));
    for bad in [-1.0, 1585.0, f64::NAN, f64::INFINITY] {
        let x = RowExceptionsEdit { cell_spacing: Some(Points(bad)), ..RowExceptionsEdit::default() };
        assert!(matches!(e.set_row_exceptions(None, Some(&x), fonts()), Outcome::Refused(_)), "{bad}");
    }
    let x = RowExceptionsEdit { cell_spacing: Some(Points(4.0)), ..RowExceptionsEdit::default() };
    assert!(matches!(e.set_row_exceptions(None, Some(&x), fonts()), Outcome::Changed(_)));
    assert_eq!(table(&e).rows[0].cell_spacing, Some(4.0));
}

#[test]
fn table_default_margins_write_only_the_sides_given() {
    let mut e = open(&grid2(["", "", "", ""], "", ["", ""]));
    cells(&mut e, (0, 0), (0, 0));
    assert!(matches!(e.set_table_cell_margins([None; 4], fonts()), Outcome::Unchanged(_)));
    assert!(matches!(e.set_table_cell_margins([Some(Points(3.0)), None, None, None], fonts()), Outcome::Changed(_)));
    assert!(matches!(e.set_table_cell_margins([None, None, Some(Points(4.0)), None], fonts()), Outcome::Changed(_)));
    let xml = doc_xml(&e);
    let at = xml.find("<w:tblCellMar>").expect("the table's margins");
    let mar = &xml[at..at + xml[at..].find("</w:tblCellMar>").expect("closed")];
    assert_eq!(mar, r#"<w:tblCellMar><w:top w:w="60" w:type="dxa"/><w:bottom w:w="80" w:type="dxa"/>"#, "the other sides inherited");
}

#[test]
fn a_continuation_starting_in_another_column_isnt_counted_in_a_row_span() {
    let restart = r#"<w:vMerge w:val="restart"/>"#;
    let body = format!(
        "<w:tbl><w:tblPr><w:tblW w:w=\"0\" w:type=\"auto\"/></w:tblPr><w:tblGrid><w:gridCol w:w=\"1440\"/><w:gridCol w:w=\"1440\"/></w:tblGrid><w:tr>{}{}</w:tr><w:tr>{}</w:tr></w:tbl><w:p/>",
        tc("", "x"),
        tc(restart, "y"),
        tc(r#"<w:gridSpan w:val="2"/><w:vMerge/>"#, "")
    );
    let mut e = open(&body);
    let Some(Block::Para(y)) = table(&e).rows[0].cells[1].blocks.first() else { panic!("a paragraph") };
    let y = crate::layout::ParaId(y.id);
    e.set_selection(Selection::Caret(crate::editing::position::Position::text(crate::editing::position::StoryId::Body, y, crate::layout::Unit(0))));
    assert_eq!(e.table_readback().map(|r| r.row_span), Some(1), "the cell below starts left of it");
}

#[test]
fn resetting_row_exceptions_keeps_the_rows_own_alignment() {
    let ex = r#"<w:tblPrEx><w:jc w:val="right"/></w:tblPrEx>"#;
    let own = format!(r#"{ex}<w:trPr><w:jc w:val="center"/></w:trPr>"#);
    let mut e = open(&grid2(["", "", "", ""], "", [&own, ex]));
    use crate::docx::props::Jc;
    assert_eq!((table(&e).rows[0].jc, table(&e).rows[1].jc), (Some(Jc::Center), Some(Jc::Right)));
    cells(&mut e, (0, 0), (1, 1));
    let x = RowExceptionsEdit { reset: true, ..RowExceptionsEdit::default() };
    assert!(matches!(e.set_row_exceptions(None, Some(&x), fonts()), Outcome::Changed(_)));
    assert_eq!((table(&e).rows[0].jc, table(&e).rows[1].jc), (Some(Jc::Center), None), "the exceptions' alignment gone");
    let again = reopened(&e);
    assert_eq!((table(&again).rows[0].jc, table(&again).rows[1].jc), (Some(Jc::Center), None), "as reopened");
}

#[test]
fn insert_cells_shift_right_puts_the_caret_in_the_new_cell_when_the_grid_changes() {
    use crate::editing::table_cells::InsertShift;
    // The grid's second column is no row's: rebuilt without it.
    let mut e = open(&table_of(3, &[&[(SPAN2, "a"), ("", "b")], &[(SPAN2, "c"), ("", "d")]]));
    cells(&mut e, (0, 2), (0, 2));
    assert!(matches!(e.insert_cells(InsertShift::Right, fonts()), Outcome::Changed(_)));
    assert_eq!(texts(&e), [vec!["a", "", "b"], vec!["c", "d"]]);
    let Some(Block::Para(p)) = table(&e).rows[0].cells[1].blocks.first() else { panic!("a paragraph") };
    let caret = e.selection().focus().and_then(|f| f.para_offset()).map(|(p, _)| p);
    assert_eq!(caret, Some(crate::layout::ParaId(p.id)), "in the new cell");
}

#[test]
fn the_pen_keeps_the_widths_of_the_cells_it_doesnt_split() {
    use crate::editing::table_draw::Stroke;
    let pct = |t: &str| {
        (
            format!(r#"<w:tcW w:w="1440" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>{t}</w:t>"#),
            format!(r#"<w:tcW w:w="2500" w:type="pct"/></w:tcPr><w:p><w:r><w:t>{t}</w:t>"#),
        )
    };
    let mut body = grid2(["", "", "", ""], "", ["", ""]);
    for t in ["c", "d"] {
        let (from, to) = pct(t);
        body = body.replace(&from, &to);
    }
    let mut e = open(&body);
    let below: Vec<_> = table(&e).rows[1].cells.iter().map(|c| c.width).collect();
    assert!(below.iter().all(|w| matches!(w, Some(crate::docx::props::Width::Pct(_)))), "{below:?}");
    let r = cell_rect(&mut e, 0, 0);
    let x = r[0] + 24.0;
    assert!(matches!(e.draw_table_stroke(Stroke { page: 0, from: [x, r[1] + 1.0], to: [x, r[3] - 1.0] }, fonts()), Outcome::Changed(_)));
    assert_eq!(table(&e).rows[0].cells.len(), 3);
    assert_eq!(table(&e).rows[1].cells.iter().map(|c| c.width).collect::<Vec<_>>(), below, "the row below keeps its widths");
}

#[test]
fn a_legacy_tables_unset_indent_takes_its_first_rows_left_border() {
    let ex = r#"<w:tblPrEx><w:tblBorders><w:left w:val="single" w:sz="48" w:space="0" w:color="000000"/></w:tblBorders></w:tblPrEx>"#;
    let mar = r#"<w:tblCellMar><w:left w:w="0" w:type="dxa"/><w:right w:w="0" w:type="dxa"/></w:tblCellMar>"#;
    let e = open(&grid2(["", "", "", ""], mar, [ex, ""]));
    let t = table(&e);
    assert!(!t.ind_set);
    let ind = crate::layout::table::placed_indent(t, crate::docx::Compat::Legacy);
    assert!((ind - 3.0).abs() < 0.05, "half the row's 6 pt border: {ind}");
}
