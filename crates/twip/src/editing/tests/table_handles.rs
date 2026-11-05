//! Table affordances: the zones the pointer shows handles for, the "+" buttons, a border drag's and a resize's
//! readouts, the selection's status line, positioning a table, and moving it by its handle.

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

/// One pixel at 100%, in points.
const PX: f64 = 0.75;
const ALT: Mods = Mods { ctrl: false, shift: false, alt: true, meta: false };
const NONE: Mods = Mods { ctrl: false, shift: false, alt: false, meta: false };

/// A letter page with inch margins: a paragraph, a fixed 3 × 4 table (columns 1, 1.25 and 1.5 in; its first row a
/// header row), and a paragraph.
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
    let body = format!("<w:p><w:r><w:t>Before</w:t></w:r></w:p>{table}<w:p><w:r><w:t>After the table.</w:t></w:r></w:p>");
    let bytes = crate::layout::tests::shapes::bytes(&body).expect("a package");
    let mut e = Editor::open(&bytes, fonts()).expect("opens");
    e.complete(fonts());
    e.view_settings.object_grid = ObjectGrid::Off;
    e
}

/// The table's id, its rows' boxes and its first row's edges (x by grid boundary) on page 0.
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

/// A pointer event at a page 0 point.
fn at(e: &mut Editor, kind: PointerKind, x: f64, y: f64, mods: Mods) {
    let (dx, dy) = e.stack().to_doc(0, x, y).expect("on the stack");
    e.pointer_input(kind, dx, dy, if kind == PointerKind::Down { 1 } else { 0 }, mods, fonts());
}

/// What the pointer shows after moving there with no button.
fn hover(e: &mut Editor, x: f64, y: f64) -> Option<TableHover> {
    at(e, PointerKind::Move, x, y, NONE);
    e.table_hover
}

fn click(e: &mut Editor, x: f64, y: f64) {
    at(e, PointerKind::Down, x, y, NONE);
    at(e, PointerKind::Up, x, y, NONE);
    e.complete(fonts());
}

/// A drag from one page 0 point by `by`, in 8 steps with `mods`; released when `release`.
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
    // From over the table: the "+" zones are live from there.
    let from_table = |e: &mut Editor, x: f64, y: f64| {
        hover(e, cell.0, cell.1);
        hover(e, x, y)
    };
    // Above the table with the pointer coming from elsewhere: no zone, the text above.
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
    // Away from the table: nothing shows, and the zones above it are gone with it.
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
    // The "+" zones aren't: a click on the text above the table reaches the text.
    click(&mut e, d.edges[2], d.top() - 12.0 * PX);
    assert!(matches!(e.selection(), Selection::Caret(_)) && e.history.next_undo().is_none(), "{:?}", e.selection());
    // Hovered from the table, they are, and the "+" shows with its line down the table.
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
    e.execute(Command::Undo, fonts());
    assert_eq!((grid(&e, d.table), e.history.next_undo()), (before.clone(), None), "one step");
    // At the left edge: left of column 1.
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
    e.execute(Command::Undo, fonts());
    assert_eq!((rows(&e), e.history.next_undo()), (4, None), "one step");
}

#[test]
fn a_column_border_drag_shows_the_two_widths_and_the_change() {
    let mut e = page();
    let d = Drawn::of(&mut e);
    let from = (d.edges[1], (d.rows[1][1] + d.rows[1][3]) / 2.0);
    // Each readout's label and tone (a chip has none: muted).
    let readouts = |e: &mut Editor| -> Vec<(String, Tone)> {
        e.view(0..1)
            .measures
            .into_iter()
            .filter_map(|(_, m)| match m {
                Measure::Dimension { label, tone, .. } | Measure::Status { label, tone, .. } => Some((label, tone)),
                Measure::Chip { label, .. } => Some((label, Tone::Muted)),
                Measure::Hint { .. } => None,
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
        horizontal: Some(HorizontalPosition { relative_to: HorizontalReference::Margin, align: None, offset: Some(Points(x)) }),
        vertical: Some(VerticalPosition { relative_to: VerticalReference::Paragraph, align: None, offset: Some(Points(y)) }),
        wrap_distance: None,
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
    e.execute(Command::Undo, fonts());
    assert_eq!((floating(&e, d.table), e.history.next_undo()), (None, None), "one step");
    // A reference a table can't take.
    let mut bad = position(0.0, 0.0);
    bad.horizontal = Some(HorizontalPosition { relative_to: HorizontalReference::Character, align: None, offset: None });
    assert!(matches!(e.execute(Command::SetTablePosition(Some(bad)), fonts()), Outcome::Refused(_)));
}

#[test]
fn a_reference_given_alone_keeps_the_table_where_it_is() {
    let mut e = page();
    let d = Drawn::of(&mut e);
    click(&mut e, (d.edges[0] + d.edges[1]) / 2.0, (d.rows[1][1] + d.rows[1][3]) / 2.0);
    let stay = TablePosition {
        horizontal: Some(HorizontalPosition { relative_to: HorizontalReference::Page, align: None, offset: None }),
        vertical: Some(VerticalPosition { relative_to: VerticalReference::Page, align: None, offset: None }),
        wrap_distance: None,
    };
    assert!(matches!(e.execute(Command::SetTablePosition(Some(stay)), fonts()), Outcome::Changed(_)));
    e.complete(fonts());
    let p = floating(&e, d.table).expect("positioned");
    assert_eq!((p.h_page, p.v_anchor), (true, TableAnchor::Page));
    let now = Drawn::of(&mut e);
    // Within two twips: offsets are whole twips, and a page-anchored tblpY is one twip more than its offset.
    assert!((now.left() - d.left()).abs() < 0.1 && (now.top() - d.top()).abs() < 0.1, "{:?} → {:?}", d.rows[0], now.rows[0]);
    assert_eq!(e.history.next_undo(), Some("Table Position"));
    e.execute(Command::Undo, fonts());
    assert_eq!((floating(&e, d.table), e.history.next_undo()), (None, None), "one step");
}

#[test]
fn the_move_handle_drags_the_table_to_the_snapped_point() {
    let mut e = page();
    let d = Drawn::of(&mut e);
    let handle = (d.left() - 12.0 * PX, d.top() - 12.0 * PX);
    // A click selects the table and changes nothing.
    hover(&mut e, d.edges[1], d.rows[1][1] + 2.0);
    click(&mut e, handle.0, handle.1);
    assert!(matches!(e.selection(), Selection::Cells { shape: CellShape::Table, .. }));
    assert_eq!((floating(&e, d.table), e.history.next_undo()), (None, None));
    // Dragged so its middle comes 2 pt right of the page's: it snaps there.
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
    e.execute(Command::Undo, fonts());
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
