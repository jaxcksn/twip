//! Commands' arguments and the kinds of what they name

use crate::api::command::*;
use crate::api::engine::{Engine, SessionId, SessionOptions};
use crate::api::error::ErrorCode;
use crate::api::fonts::FontLibrary;
use crate::api::services::StyleType;
use crate::api::session::Session;
use crate::api::value::{ControlKind, ControlOptions, ListItem, ListOptions, Points, StyleId};

fn setup() -> (Engine, SessionId) {
    let mut engine = Engine::new(FontLibrary::system_recommended());
    let id = engine.create_session(SessionOptions::default());
    (engine, id)
}

fn code<T: std::fmt::Debug>(r: Result<T, crate::api::error::Error>) -> Option<ErrorCode> {
    r.err().map(|e| e.code)
}

#[test]
fn set_columns_checks_its_widths() {
    let w = |width, spacing| ColumnWidth { width: Points(width), spacing: Points(spacing) };
    for (count, widths) in [
        (3, vec![w(200.0, 20.0), w(200.0, 20.0)]),
        (2, vec![w(-50.0, 0.0), w(0.0, -10.0)]),
        (3, vec![w(100.0, 0.0)]),
        (2, vec![w(f64::INFINITY, 0.0), w(100.0, 0.0)]),
        (2, vec![w(400.0, 20.0), w(400.0, 0.0)]),
    ] {
        let (mut engine, id) = setup();
        let s = engine.session(id).unwrap();
        let r = s.run(Command::SetColumns(Columns { count, spacing: None, separator: None, widths: Some(widths.clone()) }));
        assert_eq!(code(r), Some(ErrorCode::InvalidArgument), "{count} {widths:?}");
    }
    let (mut engine, id) = setup();
    let s = engine.session(id).unwrap();
    s.run(Command::SetColumns(Columns { count: 2, spacing: None, separator: None, widths: Some(vec![w(200.0, 20.0), w(200.0, 0.0)]) }))
        .expect("widths that fit");
    assert_eq!(s.state().page.columns.count, 2);
}

#[test]
fn a_drop_down_with_an_item_named_combo_box_stays_a_drop_down() {
    let (mut engine, id) = setup();
    let s = engine.session(id).unwrap();
    s.run(Command::InsertControl(InsertControl {
        kind: ControlKind::DropDown,
        tag: None,
        title: None,
        lock: None,
        placeholder: None,
        binding: None,
        options: Some(ControlOptions::List(ListOptions { items: vec![ListItem { text: "comboBox".into(), value: "1".into() }] })),
        placeholder_image: None,
        wrap_selection: None,
        float: None,
    }))
    .expect("inserts");
    let all = s.elements(&Default::default());
    assert_eq!(all.first().map(|e| e.kind), Some(crate::api::state::ElementKind::DropDown), "{all:?}");
}

fn kind_id(s: &Session, kind: StyleType) -> StyleId {
    s.styles(Some(kind)).into_iter().next().expect("a style").id
}

#[test]
fn style_commands_take_only_their_kind_of_style() {
    let (mut engine, id) = setup();
    let s = engine.session(id).unwrap();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/FX-STYLES.docx");
    s.open(&std::fs::read(path).unwrap()).unwrap();
    let para = kind_id(s, StyleType::Paragraph);
    let table = kind_id(s, StyleType::Table);
    s.run(Command::InsertTable(TableSize { rows: 1, columns: 1 })).unwrap();
    assert_eq!(code(s.run(Command::SetTableStyle(para.clone()))), Some(ErrorCode::InvalidArgument), "a paragraph style as a table style");
    assert_eq!(code(s.run(Command::SetParagraphStyle(table.clone()))), Some(ErrorCode::InvalidArgument), "a table style as a paragraph style");
    s.run(Command::SetTableStyle(table)).expect("a table style");
    s.run(Command::SetParagraphStyle(para)).expect("a paragraph style");
}

#[test]
fn a_table_column_boundary_moves() {
    let (mut engine, id) = setup();
    let s = engine.session(id).unwrap();
    s.run(Command::InsertTable(TableSize { rows: 1, columns: 3 })).unwrap();
    let before = s.state().ruler.horizontal.table_columns.clone();
    assert_eq!(before.len(), 4, "{before:?}");
    let to = before[1].0 + 30.0;
    s.run(Command::MoveTableColumnBoundary(BoundaryMove { index: 1, to: Points(to) })).expect("moves");
    let after = &s.state().ruler.horizontal.table_columns;
    assert!((after[1].0 - to).abs() < 0.6, "the boundary is there: {after:?}");
    assert!((after[3].0 - before[3].0).abs() < 0.6, "the table's right edge stays: {after:?}");
    let left = after[0].0 + 18.0;
    s.run(Command::MoveTableColumnBoundary(BoundaryMove { index: 0, to: Points(left) })).expect("moves the left edge");
    let edges = s.state().ruler.horizontal.table_columns.clone();
    assert!((edges[0].0 - left).abs() < 0.6 && (edges[1].0 - to).abs() < 0.6, "only the left edge moved: {edges:?}");
}

#[test]
fn a_text_column_boundary_moves() {
    let (mut engine, id) = setup();
    let s = engine.session(id).unwrap();
    s.run(Command::SetColumns(Columns { count: 2, spacing: Some(Points(36.0)), separator: None, widths: None })).unwrap();
    let before = s.state().ruler.horizontal.columns.clone();
    assert_eq!(before.len(), 2);
    let to = before[0].end.0 - 40.0;
    s.run(Command::MoveColumnBoundary(ColumnBoundaryMove { column: 0, to: Points(to) })).expect("moves");
    let after = s.state().ruler.horizontal.columns.clone();
    assert!((after[0].end.0 - to).abs() < 0.6, "the first column ends there: {after:?}");
    assert!((after[1].start.0 - after[0].end.0 - 36.0).abs() < 0.6, "the gap keeps its width: {after:?}");
    assert!((after[1].end.0 - before[1].end.0).abs() < 0.6, "the text's right edge stays: {after:?}");
}

#[test]
fn a_table_row_boundary_moves() {
    let (mut engine, id) = setup();
    let s = engine.session(id).unwrap();
    s.run(Command::InsertTable(TableSize { rows: 2, columns: 1 })).unwrap();
    let before = s.state().ruler.vertical.table_rows.clone();
    assert_eq!(before.len(), 3, "{before:?}");
    let to = before[1].0 + 20.0;
    s.run(Command::MoveTableRowBoundary(BoundaryMove { index: 1, to: Points(to) })).expect("moves");
    let after = s.state().ruler.vertical.table_rows.clone();
    assert!((after[1].0 - to).abs() < 0.6, "the first row ends there: {after:?}");
    let top = Command::MoveTableRowBoundary(BoundaryMove { index: 0, to: Points(10.0) });
    assert_eq!(code(s.run(top)), Some(ErrorCode::InvalidArgument), "the top edge stays");
}

#[test]
fn boundary_moves_stay_within_the_page() {
    let (mut engine, id) = setup();
    let s = engine.session(id).unwrap();
    s.run(Command::InsertTable(TableSize { rows: 2, columns: 2 })).unwrap();
    for to in [1e7, -1e7, f64::NAN, f64::INFINITY] {
        let moves = [
            Command::MoveTableRowBoundary(BoundaryMove { index: 1, to: Points(to) }),
            Command::MoveTableColumnBoundary(BoundaryMove { index: 2, to: Points(to) }),
            Command::MoveColumnBoundary(ColumnBoundaryMove { column: 0, to: Points(to) }),
        ];
        for m in moves {
            assert_eq!(code(s.run(m.clone())), Some(ErrorCode::InvalidArgument), "{m:?}");
        }
    }
    s.run(Command::MoveTableRowBoundary(BoundaryMove { index: 1, to: Points(1584.0) })).expect("moves");
    s.run(Command::MoveTableColumnBoundary(BoundaryMove { index: 2, to: Points(1584.0) })).expect("moves");
    s.with_editor(|e| {
        let t = e.document().sections[0].blocks.iter().find_map(|b| match b {
            crate::docx::Block::Table(t) => Some(t.clone()),
            crate::docx::Block::Para(_) => None,
        });
        let t = t.expect("a table");
        assert!(t.grid.iter().all(|w| *w <= 1584.0), "{:?}", t.grid);
        assert!(t.rows.iter().all(|r| r.height.as_ref().is_none_or(|h| h.0 <= 1584.0)), "a row 22 in at most");
    });
}
