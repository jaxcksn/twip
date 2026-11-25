//! Table affordances

use super::super::Editor;
use super::super::commands::{Command, Outcome};
use super::super::input::{Cursor, PointerKind};
use super::super::keymap::Mods;
use super::super::selection::{CellRef, CellShape, Selection};
use super::super::view::{Axis, Measure, TableHover, TableMark, TableZone, Tone};
use super::fonts;
use crate::api::command::TablePosition;
use crate::api::services::{MeasureDetail, ObjectGrid};
use crate::api::value::{HorizontalPosition, HorizontalReference, Points, VerticalPosition, VerticalReference};
use crate::docx::props::TableAnchor;
use crate::layout::TableId;

const PX: f64 = 0.75;
const ALT: Mods = Mods { ctrl: false, shift: false, alt: true, meta: false };
const NONE: Mods = Mods { ctrl: false, shift: false, alt: false, meta: false };

fn page() -> Editor {
    let cell = |w: u32| format!(r#"<w:tc><w:tcPr><w:tcW w:w="{w}" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>x</w:t></w:r></w:p></w:tc>"#);
    let row = |header: bool| {
        let pr = if header { "<w:trPr><w:tblHeader/></w:trPr>" } else { "" };
        format!("<w:tr>{pr}{}{}{}</w:tr>", cell(1440), cell(1800), cell(2160))
    };
    let table = format!(
        r#"<w:tbl><w:tblPr><w:tblW w:w="5400" w:type="dxa"/><w:tblLayout w:type="fixed"/></w:tblPr><w:tblGrid><w:gridCol w:w="1440"/><w:gridCol w:w="1800"/><w:gridCol w:w="2160"/></w:tblGrid>{}{}{}{}</w:tbl>"#,
        row(true),
        row(false),
        row(false),
        row(false)
    );
    open(&table)
}

/// `table` between two paragraphs, laid out.
fn open(table: &str) -> Editor {
    let body = format!("<w:p><w:r><w:t>Before</w:t></w:r></w:p>{table}<w:p><w:r><w:t>After the table.</w:t></w:r></w:p>");
    let bytes = crate::layout::tests::shapes::bytes(&body).expect("a package");
    let mut e = Editor::open(&bytes, fonts()).expect("opens");
    e.complete(fonts());
    e.view_settings.object_grid = ObjectGrid::Off;
    e
}

struct Drawn {
    table: TableId,
    rows: Vec<[f64; 4]>,
    edges: Vec<f64>,
}

impl Drawn {
    fn of(e: &mut Editor) -> Drawn {
        let geo = e.page_geometries().into_iter().find(|(p, _)| *p == 0).map(|(_, g)| g).expect("page 0");
        let first = geo.rows.iter().find(|r| r.row == 0 && !r.repeat).expect("the first row");
        let mut rows: Vec<_> = geo.rows.iter().filter(|r| r.table == first.table && !r.repeat).map(|r| (r.row, r.rect)).collect();
        rows.sort_by_key(|r| r.0);
        let mut edges = first.edges.clone();
        edges.sort_by_key(|e| e.1);
        Drawn { table: first.table, rows: rows.into_iter().map(|r| r.1).collect(), edges: edges.into_iter().map(|e| e.0).collect() }
    }

    fn left(&self) -> f64 {
        self.rows[0][0]
    }

    fn top(&self) -> f64 {
        self.rows[0][1]
    }
}

fn at(e: &mut Editor, kind: PointerKind, x: f64, y: f64, mods: Mods) {
    let (dx, dy) = e.stack().to_doc(0, x, y).expect("on the stack");
    e.pointer_input(kind, dx, dy, if kind == PointerKind::Down { 1 } else { 0 }, mods, fonts());
}

fn hover(e: &mut Editor, x: f64, y: f64) -> Option<TableHover> {
    at(e, PointerKind::Move, x, y, NONE);
    e.table_hover
}

fn click(e: &mut Editor, x: f64, y: f64) {
    at(e, PointerKind::Down, x, y, NONE);
    at(e, PointerKind::Up, x, y, NONE);
    e.complete(fonts());
}

fn drag(e: &mut Editor, from: (f64, f64), by: (f64, f64), mods: Mods, release: bool) {
    at(e, PointerKind::Down, from.0, from.1, NONE);
    for k in 1..=8 {
        let t = f64::from(k) / 8.0;
        at(e, PointerKind::Move, from.0 + by.0 * t, from.1 + by.1 * t, mods);
    }
    if release {
        at(e, PointerKind::Up, from.0 + by.0, from.1 + by.1, mods);
        e.complete(fonts());
    }
}

fn grid(e: &Editor, table: TableId) -> Vec<f64> {
    e.index().table(e.document(), table).expect("the table").grid.clone()
}

fn statuses(e: &mut Editor) -> Vec<String> {
    e.view(0..1).measures.into_iter().filter_map(|(_, m)| if let Measure::Status { label, .. } = m { Some(label) } else { None }).collect()
}

#[test]
fn each_zone_from_a_point() {
    let mut e = page();
    let d = Drawn::of(&mut e);
    let (t, [x0, y0, ..]) = (d.table, d.rows[0]);
    let z = |zone| Some(TableHover { table: t, zone: Some(zone) });
    let mid = |r: [f64; 4]| (r[1] + r[3]) / 2.0;
    let cell = ((d.edges[1] + d.edges[2]) / 2.0, mid(d.rows[1]));
    let from_table = |e: &mut Editor, x: f64, y: f64| {
        hover(e, cell.0, cell.1);
        hover(e, x, y)
    };
    assert_eq!(hover(&mut e, d.edges[1], y0 - 12.0 * PX), None, "not shown yet");
    assert_eq!(hover(&mut e, cell.0, cell.1), Some(TableHover { table: t, zone: None }), "over a cell");
    assert_eq!(hover(&mut e, d.edges[1], y0 - 12.0 * PX), z(TableZone::InsertColumn(1)), "above border 1");
    assert_eq!(e.view(0..1).cursor, Cursor::Pointer, "a \"+\" is a button");
    assert_eq!(from_table(&mut e, d.edges[0] + 3.0 * PX, y0 - 12.0 * PX), z(TableZone::InsertColumn(0)), "above the left edge");
    assert_eq!(from_table(&mut e, cell.0, y0 - 12.0 * PX), None, "above column 2, between the \"+\" zones: the text");
    assert_eq!(hover(&mut e, cell.0, y0 + 2.0 * PX), z(TableZone::SelectColumn(1)), "the top of column 2");
    assert_eq!(from_table(&mut e, x0 - 12.0 * PX, d.rows[1][3]), z(TableZone::InsertRow(2)), "left of row 2's bottom border");
    assert_eq!(hover(&mut e, x0 - 12.0 * PX, mid(d.rows[2])), z(TableZone::SelectRow(2)), "left of row 3");
    assert_eq!(hover(&mut e, x0 - 12.0 * PX, y0 - 12.0 * PX), z(TableZone::Move), "the move handle");
    let last = d.rows[3];
    assert_eq!(hover(&mut e, last[2] + 3.0 * PX, last[3] + 3.0 * PX), z(TableZone::Resize), "the resize handle");
    assert_eq!(hover(&mut e, d.edges[1], mid(d.rows[1])), z(TableZone::Border(1)), "column border 1");
    // A row border is marked like a column border: a dashed line across the table.
    assert_eq!(hover(&mut e, cell.0, d.rows[1][3]), z(TableZone::RowBorder(1)), "row 2's bottom border");
    let mark = e.view(0..1).table.and_then(|h| h.mark);
    assert_eq!(mark, Some(crate::editing::view::TableMark::RowBorder { y: d.rows[1][3], left: x0, right: d.rows[1][2] }));
    assert_eq!(hover(&mut e, 300.0, 700.0), None, "away");
    assert_eq!(e.view(0..1).table, None, "no handles");
    assert_eq!(hover(&mut e, d.edges[1], y0 - 12.0 * PX), None, "above, hidden again");
}

#[test]
fn the_caret_in_a_table_shows_its_handles_without_hover() {
    let mut e = page();
    let d = Drawn::of(&mut e);
    click(&mut e, (d.edges[1] + d.edges[2]) / 2.0, (d.rows[1][1] + d.rows[1][3]) / 2.0);
    at(&mut e, PointerKind::Move, 300.0, 700.0, NONE);
    let v = e.view(0..1).table.expect("handles");
    assert_eq!((v.page, v.corner, v.end, v.mark), (0, Some([d.rows[0][0], d.rows[0][1]]), Some([d.rows[3][2], d.rows[3][3]]), None));
    click(&mut e, d.edges[2], d.top() - 12.0 * PX);
    assert!(matches!(e.selection(), Selection::Caret(_)) && e.history.next_undo().is_none(), "{:?}", e.selection());
    hover(&mut e, d.edges[2], (d.rows[1][1] + d.rows[1][3]) / 2.0);
    assert_eq!(hover(&mut e, d.edges[2], d.top() - 12.0 * PX).and_then(|h| h.zone), Some(TableZone::InsertColumn(2)));
    let mark = e.view(0..1).table.and_then(|t| t.mark);
    assert_eq!(mark, Some(TableMark::InsertColumn { x: d.edges[2], top: d.top(), bottom: d.rows[3][3] }));
}

#[test]
fn a_plus_inserts_a_column_selected_in_one_step() {
    let mut e = page();
    let d = Drawn::of(&mut e);
    let before = grid(&e, d.table);
    hover(&mut e, d.edges[1], d.rows[1][1] + 2.0);
    click(&mut e, d.edges[1], d.top() - 12.0 * PX);
    assert_eq!(grid(&e, d.table), vec![72.0, 72.0, 90.0, 108.0], "as wide as column 1, right of it");
    let col = |c: usize| Selection::Cells {
        table: d.table,
        anchor: CellRef { row: 0, col: c },
        focus: CellRef { row: 3, col: c },
        shape: CellShape::Columns,
    };
    assert_eq!(e.selection(), col(1), "the new column");
    assert_eq!(e.history.next_undo(), Some("Insert Columns"));
    e.execute(Command::Undo(Default::default()), fonts());
    assert_eq!((grid(&e, d.table), e.history.next_undo()), (before.clone(), None), "one step");
    hover(&mut e, d.edges[1], d.rows[1][1] + 2.0);
    click(&mut e, d.edges[0] + 3.0 * PX, d.top() - 12.0 * PX);
    assert_eq!((grid(&e, d.table), e.selection()), (vec![72.0, 72.0, 90.0, 108.0], col(0)));
}

#[test]
fn a_plus_inserts_a_row_selected_in_one_step() {
    let mut e = page();
    let d = Drawn::of(&mut e);
    hover(&mut e, d.edges[1], d.rows[1][1] + 2.0);
    click(&mut e, d.left() - 12.0 * PX, d.rows[1][3]);
    let rows = |e: &Editor| e.index().table(e.document(), d.table).expect("the table").rows.len();
    assert_eq!(rows(&e), 5);
    let sel = Selection::Cells { table: d.table, anchor: CellRef { row: 2, col: 0 }, focus: CellRef { row: 2, col: 2 }, shape: CellShape::Rows };
    assert_eq!(e.selection(), sel, "the new row, below row 2");
    assert_eq!(e.history.next_undo(), Some("Insert Rows"));
    e.execute(Command::Undo(Default::default()), fonts());
    assert_eq!((rows(&e), e.history.next_undo()), (4, None), "one step");
}

#[test]
fn a_column_border_drag_shows_the_two_widths_and_the_change() {
    let mut e = page();
    let d = Drawn::of(&mut e);
    let from = (d.edges[1], (d.rows[1][1] + d.rows[1][3]) / 2.0);
    let readouts = |e: &mut Editor| -> Vec<(String, Tone)> {
        e.view(0..1)
            .measures
            .into_iter()
            .map(|(_, m)| match m {
                Measure::Dimension { label, tone, .. } | Measure::Status { label, tone, .. } => (label, tone),
                Measure::Chip { label, .. } => (label, Tone::Muted),
            })
            .collect()
    };
    let accent = |s: &str| (s.to_string(), Tone::Accent);
    drag(&mut e, from, (36.0, 0.0), NONE, false);
    assert_eq!(readouts(&mut e), vec![accent("1.50"), accent("0.75")], "standard: the widths 15 px above the table");
    let dim = e.view(0..1).measures.into_iter().find_map(|(_, m)| if let Measure::Dimension { from, .. } = m { Some(from[1].0) } else { None });
    assert_eq!(dim, Some(d.top() - 12.0 * PX));
    at(&mut e, PointerKind::Move, from.0 + 36.0, from.1, ALT);
    assert_eq!(readouts(&mut e), vec![accent("1.50"), accent("0.75"), accent("Δ +0.50″ · free")], "Alt: the change");
    e.view_settings.detail = MeasureDetail::Full;
    at(&mut e, PointerKind::Move, from.0 + 36.0, from.1, NONE);
    assert_eq!(readouts(&mut e), vec![accent("1.50"), accent("0.75"), accent("Δ +0.50″ · ⌥ free")], "full: the change");
    e.view_settings.detail = MeasureDetail::Quiet;
    assert_eq!(readouts(&mut e), vec![("1.50″".to_string(), Tone::Muted)], "quiet: a chip");
    assert_eq!(e.view(0..1).table, None, "no handles while dragging");
}

#[test]
fn a_table_resize_previews_its_box_and_size() {
    let mut e = page();
    let d = Drawn::of(&mut e);
    let last = d.rows[3];
    drag(&mut e, (last[2] + 3.0 * PX, last[3] + 3.0 * PX), (72.0, 18.0), NONE, false);
    let v = e.view(0..1);
    let want = [d.left(), d.top(), last[2] + 72.0, last[3] + 18.0];
    assert_eq!(v.table_preview, Some((0, want)));
    let label = format!("{:.2} × {:.2} in", (want[2] - want[0]) / 72.0, (want[3] - want[1]) / 72.0);
    let status = v.measures.iter().find_map(|(_, m)| if let Measure::Status { label, at, .. } = m { Some((label.clone(), at[1].0)) } else { None });
    assert_eq!(status, Some((label, want[3] + 10.0 * PX)));
}

/// A fixed table whose `w:tblGrid` has one column under rows of three cells.
fn short_grid() -> Editor {
    let cell = r#"<w:tc><w:tcPr><w:tcW w:w="1440" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>x</w:t></w:r></w:p></w:tc>"#.repeat(3);
    open(&format!(
        r#"<w:tbl><w:tblPr><w:tblW w:w="4320" w:type="dxa"/><w:tblLayout w:type="fixed"/></w:tblPr><w:tblGrid><w:gridCol w:w="1440"/></w:tblGrid><w:tr>{cell}</w:tr><w:tr>{cell}</w:tr></w:tbl>"#
    ))
}

#[test]
fn border_drags_on_a_short_grid_do_not_panic() {
    let mut e = short_grid();
    let d = Drawn::of(&mut e);
    let last = d.rows[1];
    drag(&mut e, (last[2] + 3.0 * PX, last[3] + 3.0 * PX), (72.0, 18.0), NONE, true);
    assert_eq!(grid(&e, d.table).len(), 3, "the resize fills in the grid");
    for k in 1..d.edges.len() {
        let mut e = short_grid();
        let d = Drawn::of(&mut e);
        drag(&mut e, (d.edges[k], d.rows[0][1] + 6.0), (-9.0, 0.0), NONE, true);
        assert_eq!(e.history.next_undo(), Some("Column Width"), "border {k}");
        let mut e = short_grid();
        e.set_selection(Selection::Cells {
            table: d.table,
            anchor: CellRef { row: 0, col: 0 },
            focus: CellRef { row: 1, col: 2 },
            shape: CellShape::Rect,
        });
        drag(&mut e, (d.edges[k], d.rows[0][1] + 6.0), (-9.0, 0.0), NONE, true);
        assert_eq!(e.history.next_undo(), Some("Column Width"), "border {k} of a selection");
    }
    // A table growing out of a cell of a table with no grid
    let inner = r#"<w:tbl><w:tblPr><w:tblW w:w="1440" w:type="dxa"/><w:tblLayout w:type="fixed"/></w:tblPr><w:tblGrid><w:gridCol w:w="1440"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w="1440" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>in</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#;
    let mut e = open(&format!(
        r#"<w:tbl><w:tblPr><w:tblW w:w="0" w:type="auto"/></w:tblPr><w:tblGrid/><w:tr><w:tc><w:tcPr><w:tcW w:w="2880" w:type="dxa"/></w:tcPr>{inner}<w:p><w:r><w:t>after</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#
    ));
    let geo = e.page_geometries().into_iter().find(|(p, _)| *p == 0).map(|(_, g)| g).expect("page 0");
    let r = geo.rows.iter().find(|r| !r.outer.is_empty()).expect("the inner row").rect;
    click(&mut e, r[0] + 4.0, r[1] + 4.0);
    drag(&mut e, (r[2] + 3.0 * PX, r[3] + 3.0 * PX), (144.0, 0.0), NONE, true);
    assert_eq!(e.history.next_undo(), Some("Resize Table"));
}

#[test]
fn a_resized_table_keeps_its_width_through_inserting_rows() {
    let mut e = page();
    let d = Drawn::of(&mut e);
    let last = d.rows[3];
    drag(&mut e, (last[2] + 3.0 * PX, last[3] + 3.0 * PX), (72.0, 0.0), NONE, true);
    let width = |e: &Editor| e.index().table(e.document(), d.table).map(|t| (t.width, t.rows.len()));
    let (resized, n) = width(&e).expect("the table");
    assert_ne!(resized, crate::docx::props::Width::Dxa(270.0), "resized");
    click(&mut e, (d.edges[0] + d.edges[1]) / 2.0, (d.rows[1][1] + d.rows[1][3]) / 2.0);
    let below = crate::api::command::InsertRows { side: crate::api::command::VerticalSide::Below, count: Some(1) };
    e.execute(Command::InsertRows(below), fonts());
    assert_eq!(width(&e), Some((resized, n + 1)));
}

#[test]
fn a_selection_shows_its_status_line_at_the_full_level() {
    let mut e = page();
    let d = Drawn::of(&mut e);
    let t = d.table;
    let cells =
        |r0, c0, r1, c1, shape| Selection::Cells { table: t, anchor: CellRef { row: r0, col: c0 }, focus: CellRef { row: r1, col: c1 }, shape };
    e.set_selection(cells(0, 1, 3, 1, CellShape::Columns));
    assert_eq!(statuses(&mut e), Vec::<String>::new(), "standard: none");
    e.view_settings.detail = MeasureDetail::Full;
    assert_eq!(statuses(&mut e), vec!["col 2 · 1.25″ · 4 cells · ⌫ delete".to_string()]);
    e.set_selection(cells(2, 0, 2, 2, CellShape::Rows));
    assert_eq!(statuses(&mut e), vec!["row 3 · 3 cells · ⌫ delete".to_string()]);
    e.set_selection(cells(0, 0, 0, 2, CellShape::Rows));
    assert_eq!(statuses(&mut e), vec!["row 1 · 3 cells".to_string()], "a header row: no delete");
    e.set_selection(cells(0, 0, 3, 2, CellShape::Table));
    assert_eq!(statuses(&mut e), vec!["table · 3 × 4".to_string()]);
    let at = e.view(0..1).measures.into_iter().find_map(|(_, m)| if let Measure::Status { at, .. } = m { Some([at[0].0, at[1].0]) } else { None });
    assert_eq!(at, Some([d.left(), d.rows[3][3] + 10.0 * PX]), "10 px under the table");
}

fn position(x: f64, y: f64) -> TablePosition {
    TablePosition {
        horizontal: Some(HorizontalPosition { relative_to: HorizontalReference::Margin, align: None, offset: Some(Points(x)), percent: None }),
        vertical: Some(VerticalPosition { relative_to: VerticalReference::Paragraph, align: None, offset: Some(Points(y)), percent: None }),
        wrap_distance: None,
        allow_overlap: None,
    }
}

fn floating(e: &Editor, t: TableId) -> Option<crate::docx::props::TablePos> {
    e.index().table(e.document(), t).and_then(|t| t.floating)
}

#[test]
fn set_table_position_saves_reopens_and_undoes() {
    let mut e = page();
    let d = Drawn::of(&mut e);
    click(&mut e, (d.edges[0] + d.edges[1]) / 2.0, (d.rows[1][1] + d.rows[1][3]) / 2.0);
    assert_eq!(e.table_position(), None);
    assert!(matches!(e.execute(Command::SetTablePosition(Some(position(72.0, 36.0))), fonts()), Outcome::Changed(_)));
    let p = floating(&e, d.table).expect("positioned");
    assert_eq!((p.x, p.y, p.v_anchor, p.h_page, p.h_text, p.dist), (72.0, 36.0, TableAnchor::Text, false, false, [0.0, 0.0, 9.0, 9.0]));
    let mut want = position(72.0, 36.0);
    want.wrap_distance = Some(crate::api::state::TextDistance { top: Points(0.0), bottom: Points(0.0), left: Points(9.0), right: Points(9.0) });
    want.allow_overlap = Some(true);
    assert_eq!(e.table_position(), Some(want), "the state's value");
    let bytes = crate::docx::write::save(e.document()).expect("saves");
    let mut xml = String::new();
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(&bytes[..])).expect("a package");
    std::io::Read::read_to_string(&mut zip.by_name("word/document.xml").expect("the document part"), &mut xml).expect("text");
    assert!(
        xml.contains(r#"<w:tblpPr w:leftFromText="180" w:rightFromText="180" w:topFromText="0" w:bottomFromText="0" w:vertAnchor="text" w:horzAnchor="margin" w:tblpX="1440" w:tblpY="720"/>"#),
        "{xml}"
    );
    let mut again = Editor::open(&bytes, fonts()).expect("reopens");
    again.complete(fonts());
    let t = Drawn::of(&mut again).table;
    assert_eq!(floating(&again, t), Some(p), "read back");
    e.execute(Command::Undo(Default::default()), fonts());
    assert_eq!((floating(&e, d.table), e.history.next_undo()), (None, None), "one step");
    let mut bad = position(0.0, 0.0);
    bad.horizontal = Some(HorizontalPosition { relative_to: HorizontalReference::Character, align: None, offset: None, percent: None });
    assert!(matches!(e.execute(Command::SetTablePosition(Some(bad)), fonts()), Outcome::Refused(_)));
}

#[test]
fn a_reference_given_alone_keeps_the_table_where_it_is() {
    let mut e = page();
    let d = Drawn::of(&mut e);
    click(&mut e, (d.edges[0] + d.edges[1]) / 2.0, (d.rows[1][1] + d.rows[1][3]) / 2.0);
    let stay = TablePosition {
        horizontal: Some(HorizontalPosition { relative_to: HorizontalReference::Page, align: None, offset: None, percent: None }),
        vertical: Some(VerticalPosition { relative_to: VerticalReference::Page, align: None, offset: None, percent: None }),
        wrap_distance: None,
        allow_overlap: None,
    };
    assert!(matches!(e.execute(Command::SetTablePosition(Some(stay)), fonts()), Outcome::Changed(_)));
    e.complete(fonts());
    let p = floating(&e, d.table).expect("positioned");
    assert_eq!((p.h_page, p.v_anchor), (true, TableAnchor::Page));
    let now = Drawn::of(&mut e);
    assert!((now.left() - d.left()).abs() < 0.1 && (now.top() - d.top()).abs() < 0.1, "{:?} → {:?}", d.rows[0], now.rows[0]);
    assert_eq!(e.history.next_undo(), Some("Table Position"));
    e.execute(Command::Undo(Default::default()), fonts());
    assert_eq!((floating(&e, d.table), e.history.next_undo()), (None, None), "one step");
}

#[test]
fn the_move_handle_drags_the_table_to_the_snapped_point() {
    let mut e = page();
    let d = Drawn::of(&mut e);
    let handle = (d.left() - 12.0 * PX, d.top() - 12.0 * PX);
    hover(&mut e, d.edges[1], d.rows[1][1] + 2.0);
    click(&mut e, handle.0, handle.1);
    assert!(matches!(e.selection(), Selection::Cells { shape: CellShape::Table, .. }));
    assert_eq!((floating(&e, d.table), e.history.next_undo()), (None, None));
    let [x0, y0, x1, y1] = [d.left(), d.top(), d.rows[0][2], d.rows[3][3]];
    let dx = 306.0 - (x0 + x1) / 2.0 + 2.0;
    drag(&mut e, handle, (dx, 100.0), NONE, false);
    let v = e.view(0..1);
    let shift = dx - 2.0;
    assert_eq!(v.table_preview, Some((0, [x0 + shift, y0 + 100.0, x1 + shift, y1 + 100.0])), "the preview, snapped");
    let guide = v.guides.iter().flat_map(|(_, g)| g.lines.iter()).find(|l| l.axis == Axis::X).map(|l| l.at.0);
    assert_eq!(guide, Some(306.0), "the page centre's guide");
    assert_eq!(v.table, None, "no handles while moving");
    at(&mut e, PointerKind::Up, handle.0 + dx, handle.1 + 100.0, NONE);
    e.complete(fonts());
    let now = Drawn::of(&mut e);
    assert!((now.left() - (x0 + shift)).abs() < 0.06 && (now.top() - (y0 + 100.0)).abs() < 0.06, "{:?} → {:?}", d.rows[0], now.rows[0]);
    let p = floating(&e, d.table).expect("positioned");
    assert_eq!((p.v_anchor, p.h_page, p.h_text, p.dist), (TableAnchor::Text, false, false, [0.0, 0.0, 9.0, 9.0]));
    assert!(matches!(e.selection(), Selection::Cells { shape: CellShape::Table, .. }), "still selected");
    assert_eq!(e.history.next_undo(), Some("Move Table"));
    e.execute(Command::Undo(Default::default()), fonts());
    assert_eq!((floating(&e, d.table), e.history.next_undo()), (None, None), "one step");
}

#[test]
fn alt_moves_the_table_freely() {
    let mut e = page();
    let d = Drawn::of(&mut e);
    hover(&mut e, d.edges[1], d.rows[1][1] + 2.0);
    let handle = (d.left() - 12.0 * PX, d.top() - 12.0 * PX);
    let dx = 306.0 - (d.left() + d.rows[0][2]) / 2.0 + 2.0;
    drag(&mut e, handle, (dx, 100.0), ALT, true);
    let now = Drawn::of(&mut e);
    assert!((now.left() - (d.left() + dx)).abs() < 0.06 && (now.top() - (d.top() + 100.0)).abs() < 0.06, "{:?} → {:?}", d.rows[0], now.rows[0]);
}

fn opened(body: &str) -> Editor {
    let mut e = Editor::open(&crate::layout::tests::shapes::bytes(body).expect("a package"), fonts()).expect("opens");
    e.complete(fonts());
    e
}

/// A fixed table whose grid (`tblGrid`, possibly empty) lists fewer columns than its row of three cells reaches.
fn short_grid_of(grid: &str) -> Editor {
    let cell = r#"<w:tc><w:tcPr><w:tcW w:w="2000" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>x</w:t></w:r></w:p></w:tc>"#;
    opened(&format!(
        r#"<w:p><w:r><w:t>Before</w:t></w:r></w:p><w:tbl><w:tblPr><w:tblW w:w="6000" w:type="dxa"/><w:tblLayout w:type="fixed"/></w:tblPr>{grid}<w:tr>{cell}{cell}{cell}</w:tr></w:tbl><w:p><w:r><w:t>After</w:t></w:r></w:p>"#
    ))
}

fn first_table(e: &Editor) -> &crate::docx::Table {
    e.document().sections[0].blocks.iter().find_map(|b| if let crate::docx::Block::Table(t) = b { Some(t) } else { None }).expect("a table")
}

fn cell_widths(e: &Editor) -> Vec<f64> {
    first_table(e)
        .rows
        .iter()
        .flat_map(|r| r.cells.iter())
        .map(|c| if let Some(crate::docx::props::Width::Dxa(v)) = c.width { v } else { 0.0 })
        .collect()
}

#[test]
fn a_column_border_drag_widens_a_grid_shorter_than_its_cells() {
    let mut e = short_grid_of(r#"<w:tblGrid><w:gridCol w:w="2000"/></w:tblGrid>"#);
    let geo = e.page_geometries().into_iter().find(|(p, _)| *p == 0).map(|(_, g)| g).expect("page 0");
    let row = geo.rows.iter().find(|r| !r.repeat).expect("a row");
    let (x, _) = *row.edges.iter().find(|(_, k)| *k == 1).expect("edge 1");
    let mut edges = row.edges.clone();
    edges.sort_by_key(|e| e.1);
    let was: Vec<f64> = edges.windows(2).map(|w| w[1].0 - w[0].0).collect();
    drag(&mut e, (x, (row.rect[1] + row.rect[3]) / 2.0), (20.0, 0.0), ALT, true);
    let widths = cell_widths(&e);
    assert_eq!(widths.len(), 3);
    assert!(widths.iter().all(|w| *w > 0.0), "{widths:?}");
    let moved = [was[0] + 20.0, was[1] - 20.0, was[2]];
    assert!(widths.iter().zip(moved).all(|(w, m)| (w - m).abs() < 0.1), "{widths:?} from {was:?}: the columns at edge 1 share the move");
    assert_eq!(first_table(&e).grid.len(), 3, "the grid has a column for each cell");
}

/// The caret in the first table's first cell.
fn caret_in_table(e: &mut Editor) {
    let d = Drawn::of(e);
    click(e, d.rows[0][0] + 10.0, (d.rows[0][1] + d.rows[0][3]) / 2.0);
    assert!(e.table_edges().is_some(), "the caret is in the table");
}

/// A table 100% wide whose grid lists 50pt columns: its columns draw far wider than its grid says.
fn percent_wide() -> Editor {
    let cell = r#"<w:tc><w:tcPr><w:tcW w:w="1000" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>x</w:t></w:r></w:p></w:tc>"#;
    opened(&format!(
        r#"<w:p><w:r><w:t>Before</w:t></w:r></w:p><w:tbl><w:tblPr><w:tblW w:w="5000" w:type="pct"/><w:tblInd w:w="0" w:type="dxa"/></w:tblPr><w:tblGrid><w:gridCol w:w="1000"/><w:gridCol w:w="1000"/></w:tblGrid><w:tr>{cell}{cell}</w:tr><w:tr>{cell}{cell}</w:tr></w:tbl><w:p><w:r><w:t>After</w:t></w:r></w:p>"#
    ))
}

#[test]
fn a_row_border_move_keeps_the_tables_width_and_grid() {
    let mut e = percent_wide();
    caret_in_table(&mut e);
    let (width, grid) = (first_table(&e).width, first_table(&e).grid.clone());
    let edges = e.table_edges().expect("edges");
    let (at, _) = edges.rows[1];
    let out = e.execute(Command::MoveTableRowBoundary(crate::api::command::BoundaryMove { index: 1, to: Points(at + 20.0) }), fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    let t = first_table(&e);
    assert!(t.rows[0].height.is_some(), "the row takes the height");
    assert_eq!((t.width, &t.grid), (width, &grid), "the width and columns stay as they were");
}

#[test]
fn an_indent_set_on_a_table_read_without_one_moves_it_at_once() {
    let mut e = page();
    caret_in_table(&mut e);
    let before = Drawn::of(&mut e).left();
    let out = e.execute(Command::SetTableIndent(Points(36.0)), fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    e.complete(fonts());
    assert!((Drawn::of(&mut e).left() - before - 36.0).abs() <= 1.0, "the table moves by the indent");
    e.execute(Command::Undo(Default::default()), fonts());
    e.complete(fonts());
    assert!((Drawn::of(&mut e).left() - before).abs() < 0.5, "and back on undo");
}

#[test]
fn moving_the_left_edge_moves_the_indent_by_as_much() {
    let mut e = percent_wide();
    caret_in_table(&mut e);
    let indent = first_table(&e).ind;
    let at = e.table_edges().expect("edges").columns[0];
    let out = e.execute(Command::MoveTableColumnBoundary(crate::api::command::BoundaryMove { index: 0, to: Points(at + 10.0) }), fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    assert!((first_table(&e).ind - indent - 10.0).abs() < 0.1, "indent {indent} → {}", first_table(&e).ind);
    e.complete(fonts());
    let now = e.table_edges().expect("edges").columns[0];
    assert!((now - at - 10.0).abs() < 0.1, "the left edge {at} → {now}");
}

#[test]
fn a_boundary_move_where_a_short_grid_ends_keeps_the_tables_width() {
    let mut e = short_grid_of(r#"<w:tblGrid><w:gridCol w:w="2000"/></w:tblGrid>"#);
    caret_in_table(&mut e);
    let was = e.table_edges().expect("edges").columns;
    assert_eq!(was.len(), 4, "{was:?}");
    let out = e.execute(Command::MoveTableColumnBoundary(crate::api::command::BoundaryMove { index: 1, to: Points(was[1] + 20.0) }), fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    let now = e.table_edges().expect("edges").columns;
    assert!((now[1] - was[1] - 20.0).abs() < 0.1, "{was:?} → {now:?}");
    assert!((now[2] - was[2]).abs() < 0.1 && (now[3] - was[3]).abs() < 0.1, "the boundaries right of it stay: {was:?} → {now:?}");
}

#[test]
fn the_resize_handle_stops_a_row_at_the_longest_height() {
    let cell = r#"<w:tc><w:tcPr><w:tcW w:w="2000" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>x</w:t></w:r></w:p></w:tc>"#;
    let mut e = opened(&format!(
        r#"<w:p><w:r><w:t>Before</w:t></w:r></w:p><w:tbl><w:tblPr><w:tblW w:w="2000" w:type="dxa"/><w:tblLayout w:type="fixed"/></w:tblPr><w:tblGrid><w:gridCol w:w="2000"/></w:tblGrid><w:tr><w:trPr><w:trHeight w:val="6000" w:hRule="exact"/></w:trPr>{cell}</w:tr></w:tbl><w:p><w:r><w:t>After</w:t></w:r></w:p>"#
    ));
    let d = Drawn::of(&mut e);
    let r = d.rows[0];
    drag(&mut e, (r[2] + 2.25, r[3] + 2.25), (0.0, 2000.0), NONE, true);
    let (h, rule) = first_table(&e).rows[0].height.clone().expect("a height");
    assert_eq!(rule, "exact");
    assert!(h > 300.0 && h <= 1584.0, "{h}");
}

#[test]
fn the_resize_handle_scales_a_table_without_a_grid() {
    let mut e = short_grid_of("");
    let geo = e.page_geometries().into_iter().find(|(p, _)| *p == 0).map(|(_, g)| g).expect("page 0");
    let row = geo.rows.iter().find(|r| !r.repeat).expect("a row");
    let before = row.rect[2] - row.rect[0];
    drag(&mut e, (row.rect[2] + 2.25, row.rect[3] + 2.25), (30.0, 10.0), NONE, true);
    let widths = cell_widths(&e);
    assert!(widths.iter().all(|w| *w > 0.0), "{widths:?}");
    assert!((widths.iter().sum::<f64>() - before - 30.0).abs() < 1.0, "{widths:?} from {before}");
}

#[test]
fn a_nested_table_grows_its_cell_in_a_grid_shorter_than_its_cells() {
    let cell = |inner: &str| format!(r#"<w:tc><w:tcPr><w:tcW w:w="1500" w:type="dxa"/></w:tcPr>{inner}<w:p><w:r><w:t>x</w:t></w:r></w:p></w:tc>"#);
    let nested = r#"<w:tbl><w:tblPr><w:tblW w:w="900" w:type="dxa"/></w:tblPr><w:tblGrid><w:gridCol w:w="900"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w="900" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>y</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#;
    let mut e = opened(&format!(
        r#"<w:p><w:r><w:t>Before</w:t></w:r></w:p><w:tbl><w:tblPr><w:tblW w:w="0" w:type="auto"/></w:tblPr><w:tblGrid><w:gridCol w:w="1500"/></w:tblGrid><w:tr>{}{}{}</w:tr></w:tbl><w:p><w:r><w:t>After</w:t></w:r></w:p>"#,
        cell(""),
        cell(""),
        cell(nested)
    ));
    let geo = e.page_geometries().into_iter().find(|(p, _)| *p == 0).map(|(_, g)| g).expect("page 0");
    let inner = geo.rows.iter().filter(|r| !r.repeat).min_by(|a, b| (a.rect[2] - a.rect[0]).total_cmp(&(b.rect[2] - b.rect[0]))).expect("a row");
    let (table, from) = (inner.table, (inner.rect[2] + 2.25, inner.rect[3] + 2.25));
    // The caret in the nested table shows its handle.
    click(&mut e, (inner.rect[0] + inner.rect[2]) / 2.0, (inner.rect[1] + inner.rect[3]) / 2.0);
    let was: f64 = grid(&e, table).iter().sum();
    drag(&mut e, from, (60.0, 0.0), NONE, true);
    let now: f64 = grid(&e, table).iter().sum();
    assert!(now > was + 30.0, "the nested table grew: {was} → {now}");
    assert_eq!(first_table(&e).grid.len(), 3, "the outer grid has a column for each cell");
}

/// The first table's widths on page 0 as laid out now, column by column.
fn laid_widths(e: &mut Editor) -> Vec<f64> {
    e.complete(fonts());
    Drawn::of(e).edges.windows(2).map(|w| w[1] - w[0]).collect()
}

#[test]
fn a_column_border_drag_lays_the_table_out_live_and_commits_once() {
    let mut e = page();
    let d = Drawn::of(&mut e);
    let (was, depth) = (grid(&e, d.table), e.history.undo_depth());
    let widths = laid_widths(&mut e);
    let from = (d.edges[1], (d.rows[1][1] + d.rows[1][3]) / 2.0);
    drag(&mut e, from, (36.0, 0.0), NONE, false);
    let live = laid_widths(&mut e);
    assert!((live[0] - widths[0] - 36.0).abs() < 0.1 && (live[1] - widths[1] + 36.0).abs() < 0.1, "{live:?} from {widths:?}");
    assert_eq!((grid(&e, d.table), e.history.undo_depth()), (was.clone(), depth), "the model waits for the release");
    at(&mut e, PointerKind::Up, from.0 + 36.0, from.1, NONE);
    assert_eq!(e.history.undo_depth(), depth + 1, "one transaction");
    assert_eq!(laid_widths(&mut e), live, "the release lays out what the drag showed");
    let mut off = page();
    off.live_layout = false;
    drag(&mut off, from, (36.0, 0.0), NONE, false);
    assert_eq!(laid_widths(&mut off), widths, "without live layout only the guide moves");
    at(&mut off, PointerKind::Up, from.0 + 36.0, from.1, NONE);
    assert_eq!(first_table(&off).grid, grid(&e, d.table), "live or not, the same resize");
    e.execute(Command::Undo(Default::default()), fonts());
    assert_eq!((grid(&e, d.table), laid_widths(&mut e)), (was, widths), "undone in one step");
}

#[test]
fn escape_cancels_a_border_drag_before_it_puts_the_highlighter_down() {
    let mut e = page();
    let d = Drawn::of(&mut e);
    let was = grid(&e, d.table);
    e.toggle_highlighter(Some(crate::api::value::HighlightColor::Yellow));
    let from = (d.edges[1], (d.rows[1][1] + d.rows[1][3]) / 2.0);
    drag(&mut e, from, (36.0, 0.0), NONE, false);
    assert!(e.border_drag.is_some(), "the drag is on");
    e.execute(Command::Escape, fonts());
    assert!(e.border_drag.is_none(), "Esc ends the drag");
    at(&mut e, PointerKind::Up, from.0 + 36.0, from.1, NONE);
    assert_eq!((grid(&e, d.table), e.history.undo_depth()), (was, 0), "the release changes nothing");
}

#[test]
fn a_cancelled_live_drag_lays_the_table_out_as_it_was() {
    let mut e = page();
    let d = Drawn::of(&mut e);
    let widths = laid_widths(&mut e);
    drag(&mut e, (d.edges[2], (d.rows[1][1] + d.rows[1][3]) / 2.0), (-30.0, 0.0), NONE, false);
    assert_ne!(laid_widths(&mut e), widths);
    at(&mut e, PointerKind::Cancel, d.edges[2] - 30.0, d.rows[1][1], NONE);
    assert_eq!(laid_widths(&mut e), widths);
    assert_eq!(e.history.undo_depth(), 0);
}

#[test]
fn a_row_border_drag_grows_the_row_live() {
    let mut e = page();
    let d = Drawn::of(&mut e);
    let height = |e: &mut Editor| {
        e.complete(fonts());
        let r = Drawn::of(e).rows[1];
        r[3] - r[1]
    };
    let h = height(&mut e);
    drag(&mut e, (d.rows[1][0] + 20.0, d.rows[1][3] + 0.5 * PX), (0.0, 24.0), NONE, false);
    assert!((height(&mut e) - h - 24.0).abs() < 0.5, "{} from {h}", height(&mut e));
    assert!(first_table(&e).rows[1].height.is_none(), "the model waits for the release");
    at(&mut e, PointerKind::Up, d.rows[1][0] + 20.0, d.rows[1][3] + 0.5 * PX + 24.0, NONE);
    assert!((height(&mut e) - h - 24.0).abs() < 0.5);
    assert!(first_table(&e).rows[1].height.is_some());
}

#[test]
fn an_edit_during_a_live_drag_lays_out_the_model() {
    let mut e = page();
    let d = Drawn::of(&mut e);
    click(&mut e, d.rows[2][0] + 10.0, (d.rows[2][1] + d.rows[2][3]) / 2.0);
    let from = (d.edges[1], (d.rows[1][1] + d.rows[1][3]) / 2.0);
    drag(&mut e, from, (36.0, 0.0), NONE, false);
    e.execute(Command::InsertText("typed".into()), fonts());
    e.complete(fonts());
    let geo = e.page_geometries().into_iter().find(|(p, _)| *p == 0).map(|(_, g)| g).expect("page 0");
    assert!(geo.rows.iter().all(|r| r.table == d.table), "laid out");
    assert!(
        first_table(&e).rows[2].cells[0]
            .blocks
            .iter()
            .any(|b| matches!(b, crate::docx::Block::Para(p) if crate::docx::fields::text(p).contains("typed")))
    );
    at(&mut e, PointerKind::Move, from.0 + 30.0, from.1, NONE);
    at(&mut e, PointerKind::Up, from.0 + 30.0, from.1, NONE);
    let t = first_table(&e);
    assert!(
        t.rows[2].cells[0].blocks.iter().any(|b| matches!(b, crate::docx::Block::Para(p) if crate::docx::fields::text(p).contains("typed"))),
        "the release keeps the edit"
    );
}

#[test]
fn a_nested_tables_live_drag_widens_the_cell_holding_it() {
    let cell = |inner: &str| format!(r#"<w:tc><w:tcPr><w:tcW w:w="1500" w:type="dxa"/></w:tcPr>{inner}<w:p><w:r><w:t>x</w:t></w:r></w:p></w:tc>"#);
    let nested = r#"<w:tbl><w:tblPr><w:tblW w:w="900" w:type="dxa"/></w:tblPr><w:tblGrid><w:gridCol w:w="900"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w="900" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>y</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#;
    let doc = format!(
        r#"<w:p><w:r><w:t>Before</w:t></w:r></w:p><w:tbl><w:tblPr><w:tblW w:w="0" w:type="auto"/></w:tblPr><w:tblGrid><w:gridCol w:w="1500"/><w:gridCol w:w="1500"/><w:gridCol w:w="1500"/></w:tblGrid><w:tr>{}{}{}</w:tr></w:tbl><w:p><w:r><w:t>After</w:t></w:r></w:p>"#,
        cell(""),
        cell(""),
        cell(nested)
    );
    let rows = |e: &mut Editor| {
        e.complete(fonts());
        let geo = e.page_geometries().into_iter().find(|(p, _)| *p == 0).map(|(_, g)| g).expect("page 0");
        let mut rows: Vec<[f64; 4]> = geo.rows.iter().filter(|r| !r.repeat).map(|r| r.rect).collect();
        rows.sort_by(|a, b| (a[2] - a[0]).total_cmp(&(b[2] - b[0])));
        rows
    };
    let mut e = opened(&doc);
    let was = rows(&mut e);
    let (inner, outer) = (was[0], was[1]);
    let from = (inner[2], (inner[1] + inner[3]) / 2.0);
    let model = first_table(&e).clone();
    drag(&mut e, from, (60.0, 0.0), NONE, false);
    let live = rows(&mut e);
    assert!(live[0][2] - live[0][0] > inner[2] - inner[0] + 50.0, "the nested table: {live:?} from {was:?}");
    assert!(live[1][2] - live[1][0] > outer[2] - outer[0] + 20.0, "the table holding it: {live:?} from {was:?}");
    assert_eq!(first_table(&e), &model, "the model waits for the release");
    at(&mut e, PointerKind::Up, from.0 + 60.0, from.1, NONE);
    assert_eq!(rows(&mut e), live, "the release lays out what the drag showed");
    e.execute(Command::Undo(Default::default()), fonts());
    assert_eq!(first_table(&e).grid, model.grid);
    assert_eq!(rows(&mut e), was);
}

#[test]
fn a_tracked_nested_tables_drag_keeps_its_revisions() {
    let cell = |inner: &str| format!(r#"<w:tc><w:tcPr><w:tcW w:w="1500" w:type="dxa"/></w:tcPr>{inner}<w:p><w:r><w:t>x</w:t></w:r></w:p></w:tc>"#);
    let nested = r#"<w:tbl><w:tblPr><w:tblW w:w="900" w:type="dxa"/></w:tblPr><w:tblGrid><w:gridCol w:w="900"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w="900" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>y</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#;
    let mut e = opened(&format!(
        r#"<w:p><w:r><w:t>Before</w:t></w:r></w:p><w:tbl><w:tblPr><w:tblW w:w="0" w:type="auto"/></w:tblPr><w:tblGrid><w:gridCol w:w="1500"/><w:gridCol w:w="1500"/><w:gridCol w:w="1500"/></w:tblGrid><w:tr>{}{}{}</w:tr></w:tbl><w:p><w:r><w:t>After</w:t></w:r></w:p>"#,
        cell(""),
        cell(""),
        cell(nested)
    ));
    let inner_of = |e: &Editor| -> crate::docx::Table {
        first_table(e).rows[0].cells[2]
            .blocks
            .iter()
            .find_map(|b| if let crate::docx::Block::Table(t) = b { Some(t.clone()) } else { None })
            .expect("nested")
    };
    let geo = e.page_geometries().into_iter().find(|(p, _)| *p == 0).map(|(_, g)| g).expect("page 0");
    let inner = geo.rows.iter().filter(|r| !r.repeat).min_by(|a, b| (a.rect[2] - a.rect[0]).total_cmp(&(b.rect[2] - b.rect[0]))).expect("a row").rect;
    let model = first_table(&e).clone();
    e.execute(Command::SetTrackChanges(true), fonts());
    drag(&mut e, (inner[2], (inner[1] + inner[3]) / 2.0), (60.0, 0.0), NONE, true);
    let t = inner_of(&e);
    assert!(t.grid.iter().sum::<f64>() > 45.0 + 50.0, "the nested table grew: {:?}", t.grid);
    let marked = t.change.is_some()
        || t.grid_change.is_some()
        || t.rows.iter().any(|r| r.rev.change.is_some() || r.cells.iter().any(|c| c.rev.change.is_some()));
    assert!(marked, "the nested table's change is tracked: {t:?}");
    e.execute(Command::Undo(Default::default()), fonts());
    assert_eq!(first_table(&e), &model, "undo restores both tables");
}

#[test]
fn a_column_inserted_during_a_border_drag_ends_it() {
    use crate::api::command::{HorizontalSide, InsertColumns};
    let mut e = page();
    let d = Drawn::of(&mut e);
    click(&mut e, d.rows[2][0] + 10.0, (d.rows[2][1] + d.rows[2][3]) / 2.0);
    let from = (d.edges[1], (d.rows[1][1] + d.rows[1][3]) / 2.0);
    drag(&mut e, from, (36.0, 0.0), NONE, false);
    e.execute(Command::InsertColumns(InsertColumns { side: HorizontalSide::Left, count: None }), fonts());
    let (inserted, depth) = (grid(&e, d.table), e.history.undo_depth());
    assert_eq!(inserted.len(), 4);
    at(&mut e, PointerKind::Move, from.0 + 30.0, from.1, NONE);
    at(&mut e, PointerKind::Up, from.0 + 30.0, from.1, NONE);
    assert_eq!((grid(&e, d.table), e.history.undo_depth()), (inserted, depth), "the drag ended with the insert");
    e.execute(Command::Undo(Default::default()), fonts());
    assert_eq!(grid(&e, d.table).len(), 3, "the insert undoes alone");
}

/// A live drag of the first column border of the table drawn first on page 0: the widths follow it, the release is
/// one transaction and its undo puts the widths back.
fn live_drags_one_transaction(e: &mut Editor) {
    let d = Drawn::of(e);
    let (was, depth) = (grid(e, d.table), e.history.undo_depth());
    let widths = laid_widths(e);
    let full = crate::layout::incremental::FULL_JOBS.get();
    let from = (d.edges[1], (d.rows[0][1] + d.rows[0][3]) / 2.0);
    drag(e, from, (20.0, 0.0), NONE, false);
    let live = laid_widths(e);
    assert!((live[0] - widths[0] - 20.0).abs() < 0.1 && (live[1] - widths[1] + 20.0).abs() < 0.1, "{live:?} from {widths:?}");
    assert_eq!((grid(e, d.table), e.history.undo_depth()), (was.clone(), depth), "the model waits for the release");
    at(e, PointerKind::Up, from.0 + 20.0, from.1, NONE);
    assert_eq!((e.history.undo_depth(), e.history.next_undo()), (depth + 1, Some("Column Width")), "one transaction");
    assert_eq!(laid_widths(e), live, "the release lays out what the drag showed");
    assert_eq!(crate::layout::incremental::FULL_JOBS.get(), full, "no move or release lays the whole document out");
    e.execute(Command::Undo(Default::default()), fonts());
    assert_eq!((grid(e, d.table), laid_widths(e)), (was, widths), "undone in one step");
}

#[test]
fn a_header_tables_border_drag_lays_it_out_live() {
    let mut e = opened("<w:p><w:r><w:t>Body</w:t></w:r></w:p>");
    e.execute(Command::EditHeader, fonts());
    e.execute(Command::InsertTable(crate::api::command::TableSize { rows: 1, columns: 3 }), fonts());
    e.complete(fonts());
    live_drags_one_transaction(&mut e);
}

#[test]
fn a_text_box_tables_border_drag_lays_it_out_live() {
    use crate::layout::tests::shapes::{anchor, sp};
    let cell = r#"<w:tc><w:tcPr><w:tcW w:w="1500" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>x</w:t></w:r></w:p></w:tc>"#;
    let table = format!(
        r#"<w:tbl><w:tblPr><w:tblW w:w="4500" w:type="dxa"/><w:tblLayout w:type="fixed"/></w:tblPr><w:tblGrid><w:gridCol w:w="1500"/><w:gridCol w:w="1500"/><w:gridCol w:w="1500"/></w:tblGrid><w:tr>{cell}{cell}{cell}</w:tr></w:tbl><w:p/>"#
    );
    let story = format!(r#"<wps:txbx><w:txbxContent>{table}</w:txbxContent></wps:txbx><wps:bodyPr/>"#);
    let boxed = sp(0, 0, 300, 100, "rect", "", "FFFFFF").replace("<wps:bodyPr/>", &story);
    let mut e = opened(&format!("<w:p><w:r><w:t>Anchor</w:t></w:r>{}</w:p>", anchor(72, 200, 300, 100, 1, false, 1, &boxed)));
    let d = Drawn::of(&mut e);
    click(&mut e, d.rows[0][0] + 10.0, (d.rows[0][1] + d.rows[0][3]) / 2.0);
    live_drags_one_transaction(&mut e);
}

#[test]
fn a_live_table_preview_is_not_the_document_as_printed() {
    let mut e = page();
    e.doc.review.view = crate::docx::review::MarkupView::NoMarkup;
    assert!(e.lays_out_as_printed(false));
    let d = Drawn::of(&mut e);
    let from = (d.edges[1], (d.rows[1][1] + d.rows[1][3]) / 2.0);
    drag(&mut e, from, (36.0, 0.0), NONE, false);
    assert!(!e.table_preview.is_empty());
    assert!(!e.lays_out_as_printed(false), "laid out as the drag would leave it");
}

#[test]
fn a_text_box_tables_hint_has_no_row_of_the_anchors_blocks() {
    use crate::layout::tests::shapes::{anchor, sp};
    let cell = r#"<w:tc><w:p><w:r><w:t>x</w:t></w:r></w:p></w:tc>"#;
    let table = format!(r#"<w:tbl><w:tblGrid><w:gridCol w:w="1500"/></w:tblGrid><w:tr>{cell}</w:tr><w:tr>{cell}</w:tr></w:tbl><w:p/>"#);
    let story = format!(r#"<wps:txbx><w:txbxContent>{table}</w:txbxContent></wps:txbx><wps:bodyPr/>"#);
    let boxed = sp(0, 0, 300, 100, "rect", "", "FFFFFF").replace("<wps:bodyPr/>", &story);
    let e = opened(&format!("<w:p><w:r><w:t>Anchor</w:t></w:r>{}</w:p>", anchor(72, 200, 300, 100, 1, false, 1, &boxed)));
    let id = e.index.table_ids().next().expect("the box's table");
    let hint = super::super::step::table_hint(&e.index, id, Some(1)).expect("a hint");
    assert_eq!(hint.row, None, "row 1 of the box's table is not a row of the anchor's block");
}

#[test]
fn the_left_outer_border_drags_the_first_column_and_the_indent() {
    let mut e = page();
    let d = Drawn::of(&mut e);
    let from = (d.edges[0], (d.rows[1][1] + d.rows[1][3]) / 2.0);
    assert!(matches!(hover(&mut e, from.0, from.1), Some(TableHover { zone: Some(TableZone::Border(0)), .. })), "the left edge is a border");
    drag(&mut e, from, (36.0, 0.0), ALT, true);
    let t = e.index().table(e.document(), d.table).expect("the table");
    assert_eq!(
        (t.grid.clone(), t.ind),
        (vec![36.0, 90.0, 108.0], 36.5),
        "the indent from where the edge was drawn (compat 14: an unset indent sits its cell margin right)"
    );
    let after = Drawn::of(&mut e);
    assert!((after.edges[0] - (d.edges[0] + 36.0)).abs() < 0.1 && (after.edges[1] - d.edges[1]).abs() < 0.1, "{:?} {:?}", d.edges, after.edges);
    e.execute(crate::editing::commands::Command::Undo(Default::default()), fonts());
    let t = e.index().table(e.document(), d.table).expect("the table");
    assert_eq!((t.grid.clone(), t.ind), (vec![72.0, 90.0, 108.0], 0.0), "one undo step");
}

#[test]
fn a_positioned_tables_left_border_drag_moves_the_table() {
    let cell = |w: u32| format!(r#"<w:tc><w:tcPr><w:tcW w:w="{w}" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>x</w:t></w:r></w:p></w:tc>"#);
    let row = format!("<w:tr>{}{}</w:tr>", cell(1440), cell(1800));
    let mut e = open(&format!(
        r#"<w:tbl><w:tblPr><w:tblpPr w:horzAnchor="page" w:vertAnchor="page" w:tblpX="2880" w:tblpY="2880"/><w:tblW w:w="3240" w:type="dxa"/><w:tblLayout w:type="fixed"/></w:tblPr><w:tblGrid><w:gridCol w:w="1440"/><w:gridCol w:w="1800"/></w:tblGrid>{row}{row}</w:tbl>"#
    ));
    let d = Drawn::of(&mut e);
    drag(&mut e, (d.edges[0], (d.rows[1][1] + d.rows[1][3]) / 2.0), (36.0, 0.0), ALT, true);
    let t = e.index().table(e.document(), d.table).expect("the table");
    assert_eq!((t.grid.clone(), t.floating.map(|p| p.x)), (vec![36.0, 90.0], Some(180.0)));
    let after = Drawn::of(&mut e);
    assert!((after.edges[0] - (d.edges[0] + 36.0)).abs() < 0.1 && (after.edges[1] - d.edges[1]).abs() < 0.1, "{:?} {:?}", d.edges, after.edges);
}

#[test]
fn the_left_border_drag_moves_only_the_selected_rows() {
    let mut e = page();
    let d = Drawn::of(&mut e);
    e.set_selection(Selection::Cells {
        table: d.table,
        anchor: CellRef { row: 1, col: 0 },
        focus: CellRef { row: 1, col: 0 },
        shape: CellShape::Rect,
    });
    drag(&mut e, (d.edges[0], (d.rows[1][1] + d.rows[1][3]) / 2.0), (36.0, 0.0), ALT, true);
    let t = e.index().table(e.document(), d.table).expect("the table");
    assert_eq!((t.rows[1].grid_before, t.rows[0].grid_before, t.ind), (1, 0, 0.0), "only row 2 starts further in");
    let geo = e.page_geometries().into_iter().find(|(p, _)| *p == 0).map(|(_, g)| g).expect("page 0");
    let first =
        |r: usize| geo.cells.iter().filter(|c| c.chain.last().is_some_and(|k| k.1 == r && k.2 == 0)).map(|c| c.rect[0]).next().expect("a cell");
    assert!((first(1) - first(2) - 36.0).abs() < 0.6, "{} {}", first(1), first(2));
    // Moved left: the other rows keep their place, the table's indent moving out.
    let mut e = page();
    let d = Drawn::of(&mut e);
    e.set_selection(Selection::Cells {
        table: d.table,
        anchor: CellRef { row: 2, col: 0 },
        focus: CellRef { row: 2, col: 0 },
        shape: CellShape::Rect,
    });
    drag(&mut e, (d.edges[0], (d.rows[2][1] + d.rows[2][3]) / 2.0), (-18.0, 0.0), ALT, true);
    let t = e.index().table(e.document(), d.table).expect("the table");
    assert_eq!((t.rows[1].grid_before, t.rows[2].grid_before), (1, 0));
    let after = Drawn::of(&mut e);
    assert!((after.rows[1][0] - d.rows[1][0]).abs() < 0.6, "{:?} {:?}", d.rows[1], after.rows[1]);
}

#[test]
fn a_selected_rows_left_edge_drag_keeps_its_cell_within_the_widest() {
    let tc = |w: u32| format!(r#"<w:tc><w:tcPr><w:tcW w:w="{w}" w:type="dxa"/></w:tcPr><w:p/></w:tc>"#);
    let row = format!("<w:tr>{}{}</w:tr>", tc(31600), tc(1440));
    let mut e = open(&format!(
        r#"<w:tbl><w:tblPr><w:tblLayout w:type="fixed"/></w:tblPr><w:tblGrid><w:gridCol w:w="31600"/><w:gridCol w:w="1440"/></w:tblGrid>{row}{row}</w:tbl>"#
    ));
    let d = Drawn::of(&mut e);
    e.set_selection(Selection::Cells {
        table: d.table,
        anchor: CellRef { row: 1, col: 0 },
        focus: CellRef { row: 1, col: 0 },
        shape: CellShape::Rect,
    });
    drag(&mut e, (d.edges[0], (d.rows[1][1] + d.rows[1][3]) / 2.0), (-18.0, 0.0), ALT, true);
    let t = e.index().table(e.document(), d.table).expect("the table");
    let g = crate::docx::grid::Grid::of(t);
    let (c, s, _) = g.rows[1][0];
    let width: f64 = t.grid[c..c + s.max(1)].iter().sum();
    assert!((width - 1584.0).abs() < 0.06, "the moved cell widened to 22 inches, no more: {width} over {:?}", t.grid);
}

#[test]
fn the_left_edge_goes_in_by_each_cells_whole_width_and_never_out() {
    let tc = |w: u32, span: u32| {
        let span = if span > 1 { format!(r#"<w:gridSpan w:val="{span}"/>"#) } else { String::new() };
        format!(r#"<w:tc><w:tcPr><w:tcW w:w="{w}" w:type="dxa"/>{span}</w:tcPr><w:p/></w:tc>"#)
    };
    let table = |w0: u32, row: &str| {
        format!(
            r#"<w:tbl><w:tblPr><w:tblLayout w:type="fixed"/></w:tblPr><w:tblGrid><w:gridCol w:w="{w0}"/><w:gridCol w:w="1800"/><w:gridCol w:w="2160"/></w:tblGrid><w:tr>{row}</w:tr><w:tr>{row}</w:tr></w:tbl>"#
        )
    };
    let edge = |e: &mut Editor| {
        let d = Drawn::of(e);
        (d.table, (d.edges[0], (d.rows[0][1] + d.rows[0][3]) / 2.0))
    };
    // A first column narrower than its cells' margins: a drag in leaves it.
    let mut e = open(&table(100, &format!("{}{}{}", tc(100, 1), tc(1800, 1), tc(2160, 1))));
    let (t, from) = edge(&mut e);
    let was = grid(&e, t);
    drag(&mut e, from, (20.0, 0.0), ALT, true);
    assert!(grid(&e, t)[0] <= was[0] + 0.01, "not widened by a drag in: {was:?} → {:?}", grid(&e, t));
    // A cell over the first two columns narrows past the first one's margins.
    let mut e = open(&table(720, &format!("{}{}", tc(2520, 2), tc(2160, 1))));
    let (t, from) = edge(&mut e);
    drag(&mut e, from, (30.0, 0.0), ALT, true);
    let now = grid(&e, t);
    assert!((now[0] - 6.0).abs() < 0.1, "36 pt less 30: {now:?}");
}
