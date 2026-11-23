//! Pointer shapes

use std::rc::Rc;

use super::super::Editor;
use super::super::geometry::PageGeometry;
use super::super::input::{Cursor, PointerKind};
use super::super::keymap::Mods;
use super::super::position::StoryId;
use super::{fonts, open};

const CTRL: Mods = Mods { ctrl: true, shift: false, alt: false, meta: false };

fn fixture(name: &str) -> Editor {
    let mut e = open(name);
    e.complete(fonts());
    e
}

fn page(e: &mut Editor, i: usize) -> Rc<PageGeometry> {
    e.page_geometries().into_iter().find(|(p, _)| *p == i).map(|(_, g)| g).expect("the page")
}

fn at(e: &mut Editor, kind: PointerKind, pg: usize, x: f64, y: f64, mods: Mods) {
    let (dx, dy) = e.stack().to_doc(pg, x, y).expect("on the stack");
    e.pointer_input(kind, dx, dy, if kind == PointerKind::Down { 1 } else { 0 }, mods, fonts());
}

fn hover(e: &mut Editor, pg: usize, x: f64, y: f64, mods: Mods) -> Cursor {
    at(e, PointerKind::Move, pg, x, y, mods);
    e.view(0..0).cursor
}

fn drag(e: &mut Editor, pg: usize, from: (f64, f64), to: (f64, f64), mods: Mods) -> Cursor {
    at(e, PointerKind::Down, pg, from.0, from.1, mods);
    at(e, PointerKind::Move, pg, to.0, to.1, mods);
    let c = e.view(0..0).cursor;
    at(e, PointerKind::Up, pg, to.0, to.1, mods);
    c
}

const PX: f64 = 0.75;

#[test]
fn cursor_over_text_is_the_i_beam() {
    let mut e = fixture("FX-TEXT.docx");
    let g = page(&mut e, 0);
    let l = g.lines.iter().find(|l| g.stories[l.story].story == StoryId::Body && l.stops.len() > 4).expect("a line");
    assert_eq!(hover(&mut e, 0, l.stops[2].x + 1.0, (l.top + l.bottom) / 2.0, Mods::default()), Cursor::Text);
}

#[test]
fn cursor_over_the_margin_bar_is_the_row_arrow() {
    let mut e = fixture("FX-TEXT.docx");
    let g = page(&mut e, 0);
    let l = g.lines.iter().find(|l| g.stories[l.story].story == StoryId::Body).expect("a line");
    assert_eq!(hover(&mut e, 0, 20.0, (l.top + l.bottom) / 2.0, Mods::default()), Cursor::RowSelect);
}

#[test]
fn cursor_over_a_link_is_the_hand_with_ctrl_or_read_only() {
    let mut e = fixture("FX-services.docx");
    let pages = e.page_geometries();
    let (pg, x, y) = pages
        .iter()
        .find_map(|(pg, g)| {
            g.lines.iter().find_map(|l| {
                l.stops.windows(2).find_map(|w| {
                    let (x, y) = ((w[0].x + w[1].x) / 2.0, (l.top + l.bottom) / 2.0);
                    let (dx, dy) = e.stack().to_doc(*pg, x, y)?;
                    e.link_under(dx, dy, fonts()).map(|_| (*pg, x, y))
                })
            })
        })
        .expect("a link");
    assert_eq!(hover(&mut e, pg, x, y, Mods::default()), Cursor::Text, "an editable link without Ctrl");
    assert_eq!(hover(&mut e, pg, x, y, CTRL), Cursor::Pointer, "Ctrl, the pointer unmoved");
    assert_eq!(hover(&mut e, pg, x, y, Mods::default()), Cursor::Text, "Ctrl released");
    e.mode = super::super::Mode::ReadOnly;
    assert_eq!(hover(&mut e, pg, x, y, Mods::default()), Cursor::Pointer, "read only");
}

fn float_picture(e: &mut Editor) -> (usize, [f64; 4]) {
    let pages = e.page_geometries();
    pages
        .iter()
        .find_map(|(pg, g)| g.stories.iter().find(|s| s.float.is_some_and(|f| e.index().text_box(e.document(), f).is_none())).map(|s| (*pg, s.rect)))
        .expect("a floating picture")
}

#[test]
fn cursor_over_a_float_moves_and_its_handles_resize_by_direction() {
    let mut e = fixture("FX-OBJ.docx");
    let (pg, [x0, y0, x1, y1]) = float_picture(&mut e);
    let (xm, ym) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    assert_eq!(hover(&mut e, pg, xm, ym, Mods::default()), Cursor::Move, "the body");
    assert_eq!(drag(&mut e, pg, (xm, ym), (xm + 2.0, ym), Mods::default()), Cursor::Move, "moving it");
    e.execute(super::super::commands::Command::Undo, fonts());
    let (pg, [x0, y0, x1, y1]) = float_picture(&mut e);
    let (xm, ym) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    let handles = [
        ((x0, y0), Cursor::ResizeNorthWestSouthEast),
        ((xm, y0), Cursor::ResizeVertical),
        ((x1, y0), Cursor::ResizeNorthEastSouthWest),
        ((x1, ym), Cursor::ResizeHorizontal),
        ((x1, y1), Cursor::ResizeNorthWestSouthEast),
        ((xm, y1), Cursor::ResizeVertical),
        ((x0, y1), Cursor::ResizeNorthEastSouthWest),
        ((x0, ym), Cursor::ResizeHorizontal),
    ];
    for (k, ((hx, hy), want)) in handles.into_iter().enumerate() {
        assert_eq!(hover(&mut e, pg, hx, hy, Mods::default()), want, "handle {k}");
        at(&mut e, PointerKind::Down, pg, hx, hy, Mods::default());
        at(&mut e, PointerKind::Move, pg, hx + 3.0, hy + 3.0, Mods::default());
        assert_eq!(e.view(0..0).cursor, want, "dragging handle {k}");
        at(&mut e, PointerKind::Cancel, pg, hx + 3.0, hy + 3.0, Mods::default());
    }
}

#[test]
fn cursor_over_an_inline_picture_is_the_arrow() {
    let mut e = fixture("FX-OBJ.docx");
    let g = page(&mut e, 0);
    let [x0, y0, x1, y1] = g.lines.iter().find_map(|l| l.pictures.first().map(|p| p.2)).expect("an inline picture");
    assert_eq!(hover(&mut e, 0, (x0 + x1) / 2.0, (y0 + y1) / 2.0, Mods::default()), Cursor::Default);
}

#[test]
fn cursor_over_table_borders_zones_and_handles() {
    let mut e = fixture("FX-TBL-BASIC.docx");
    let g = page(&mut e, 0);
    let table = g.rows.iter().find(|r| r.outer.is_empty() && r.row == 0).map(|r| r.table).expect("a table");
    let rows: Vec<_> = g.rows.iter().filter(|r| r.table == table && !r.repeat).collect();
    let [tx0, ty0, ..] = rows[0].rect;
    let last = rows.last().expect("a row").rect;
    let cell = g.cells.iter().find(|c| c.chain.len() == 1 && c.chain[0] == (table, 0, 0)).expect("the first cell");
    let [cx0, cy0, cx1, cy1] = cell.rect;
    let cym = (cy0 + cy1) / 2.0;
    let none = Mods::default();
    assert_eq!(hover(&mut e, 0, cx1, cym, none), Cursor::ResizeHorizontal, "a column border");
    assert_eq!(hover(&mut e, 0, (cx0 + cx1) / 2.0, rows[0].rect[3], none), Cursor::ResizeVertical, "a row border");
    assert_eq!(hover(&mut e, 0, last[2] + 5.0 * PX, last[3] + 5.0 * PX, none), Cursor::ResizeNorthWestSouthEast, "the resize handle");
    assert_eq!(hover(&mut e, 0, tx0 - 6.0 * PX, ty0 - 6.0 * PX, none), Cursor::Move, "the move handle");
    assert_eq!(hover(&mut e, 0, cx0 + 5.0 * PX, cym, none), Cursor::CellSelect, "a cell's zone");
    assert_eq!(hover(&mut e, 0, (cx0 + cx1) / 2.0, cy0 + 2.0 * PX, none), Cursor::ColumnSelect, "a column's zone");
    assert_eq!(hover(&mut e, 0, tx0 - 6.0 * PX, cym, none), Cursor::RowSelect, "a row's zone");
    assert_eq!(drag(&mut e, 0, (tx0 - 6.0 * PX, cym), (tx0 - 6.0 * PX, last[3] - 2.0), none), Cursor::RowSelect, "dragging rows");
    assert_eq!(drag(&mut e, 0, (cx0 + 5.0 * PX, cym), (cx1 + 30.0, cym), none), Cursor::CellSelect, "dragging cells");
    assert_eq!(drag(&mut e, 0, (cx1, cym), (cx1 + 4.0, cym), none), Cursor::ResizeHorizontal, "dragging a column border");
}

#[test]
fn cursor_over_a_checkbox_is_the_arrow() {
    let mut e = fixture("FX-CHECKBOXES.docx");
    let g = page(&mut e, 0);
    let (x, y) = g
        .lines
        .iter()
        .find_map(|l| {
            l.stops.windows(2).find_map(|w| {
                let (x, y) = ((w[0].x + w[1].x) / 2.0, (l.top + l.bottom) / 2.0);
                let (dx, dy) = e.stack().to_doc(0, x, y)?;
                e.checkbox_toggles_at(dx, dy, fonts()).map(|_| (x, y))
            })
        })
        .expect("a checkbox");
    assert_eq!(hover(&mut e, 0, x, y, Mods::default()), Cursor::Default);
}

#[test]
fn cursor_over_the_selection_is_the_arrow_and_a_refused_drop_is_not_allowed() {
    use crate::api::command::{Command, DocumentProtection, ProtectionKind};
    let mut e = fixture("FX-TEXT.docx");
    let g = page(&mut e, 0);
    let l = g.lines.iter().find(|l| g.stories[l.story].story == StoryId::Body && l.stops.len() > 8).expect("a line");
    let y = (l.top + l.bottom) / 2.0;
    let (from, inside, outside) = (l.stops[1].x, (l.stops[2].x + l.stops[3].x) / 2.0, (l.stops[7].x + l.stops[8].x) / 2.0);
    drag(&mut e, 0, (from + 0.1, y), (l.stops[5].x, y), Mods::default());
    assert_eq!(hover(&mut e, 0, inside, y, Mods::default()), Cursor::Default);
    assert_eq!(hover(&mut e, 0, outside, y, Mods::default()), Cursor::Text);
    let p = DocumentProtection { kind: Some(ProtectionKind::ReadOnly), enforce: Some(true), ..DocumentProtection::default() };
    e.execute(Command::SetDocumentProtection(p), fonts());
    at(&mut e, PointerKind::Down, 0, inside, y, Mods::default());
    at(&mut e, PointerKind::Move, 0, outside, y, Mods::default());
    assert_eq!(e.view(0..0).cursor, Cursor::NotAllowed);
    at(&mut e, PointerKind::Cancel, 0, outside, y, Mods::default());
}

#[test]
fn selection_bounds_cover_the_selected_text_and_a_turned_frame() {
    use super::super::view::Outline;
    let mut e = fixture("FX-TEXT.docx");
    let g = page(&mut e, 0);
    let l = g.lines.iter().find(|l| g.stories[l.story].story == StoryId::Body && l.stops.len() > 8).expect("a line");
    let y = (l.top + l.bottom) / 2.0;
    assert_eq!(e.selection_bounds(0..1), Default::default(), "a caret");
    drag(&mut e, 0, (l.stops[1].x + 0.1, y), (l.stops[5].x, y), Mods::default());
    let b = e.selection_bounds(0..1);
    assert_eq!(b.bounds.len(), 1);
    let r = b.bounds[0].1;
    assert!(r[0] <= l.stops[1].x + 0.5 && r[2] >= l.stops[5].x - 0.5 && r[1] <= y && r[3] >= y, "{r:?}");
    let turned = Outline { page: 0, rect: [0.0, 0.0, 20.0, 10.0], rot: 90.0 }.turned_box();
    assert!(turned.iter().zip([5.0, -5.0, 15.0, 15.0]).all(|(a, b)| (a - b).abs() < 1e-9), "{turned:?}");
}

fn canvas(e: &mut Editor, pg: usize, x: f64, y: f64, clicks: u8) -> Option<super::super::gestures::CanvasGesture> {
    let (dx, dy) = e.stack().to_doc(pg, x, y).expect("on the stack");
    e.canvas_press(dx, dy, clicks, Mods::default(), fonts()).map(|g| g.gesture)
}

fn canvas_drag(e: &mut Editor, pg: usize, from: (f64, f64), to: (f64, f64)) -> Option<super::super::gestures::CanvasGesture> {
    at(e, PointerKind::Down, pg, from.0, from.1, Mods::default());
    at(e, PointerKind::Move, pg, to.0, to.1, Mods::default());
    let (dx, dy) = e.stack().to_doc(pg, to.0, to.1).expect("on the stack");
    let g = e.canvas_release(dx, dy).map(|g| g.gesture);
    at(e, PointerKind::Cancel, pg, to.0, to.1, Mods::default());
    g
}

#[test]
fn canvas_gestures_name_object_table_and_note_presses() {
    use super::super::gestures::CanvasGesture as G;
    let mut e = fixture("FX-OBJ.docx");
    let (pg, [x0, y0, x1, y1]) = float_picture(&mut e);
    let (xm, ym) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    assert_eq!(canvas(&mut e, pg, xm, ym, 1), None);
    assert_eq!(canvas(&mut e, pg, xm, ym, 2), Some(G::ObjectDoubleClick));
    at(&mut e, PointerKind::Down, pg, xm, ym, Mods::default());
    at(&mut e, PointerKind::Up, pg, xm, ym, Mods::default());
    assert_eq!(canvas(&mut e, pg, x1, y1, 1), None, "a handle press starts a drag");
    assert_eq!(canvas_drag(&mut e, pg, (x1, y1), (x1 + 6.0, y1 + 6.0)), Some(G::ObjectResizeDrag));
    assert_eq!(canvas_drag(&mut e, pg, (xm, ym), (xm + 6.0, ym)), None, "a move is objectDrag's");

    let mut e = fixture("FX-TBL-BASIC.docx");
    let g = page(&mut e, 0);
    let table = g.rows.iter().find(|r| r.outer.is_empty() && r.row == 0).map(|r| r.table).expect("a table");
    let [tx0, ty0, ..] = g.rows.iter().find(|r| r.table == table).expect("a row").rect;
    let cell = g.cells.iter().find(|c| c.chain.len() == 1 && c.chain[0] == (table, 0, 0)).expect("the first cell");
    let [cx0, cy0, _, cy1] = cell.rect;
    assert_eq!(canvas(&mut e, 0, cx0 + 5.0 * PX, (cy0 + cy1) / 2.0, 1), Some(G::TableZoneClick));
    let handle = (tx0 - 6.0 * PX, ty0 - 6.0 * PX);
    assert_eq!(hover(&mut e, 0, handle.0, handle.1, Mods::default()), Cursor::Move);
    assert_eq!(canvas(&mut e, 0, handle.0, handle.1, 1), None);
    assert_eq!(canvas_drag(&mut e, 0, handle, (handle.0 + 20.0, handle.1 + 20.0)), Some(G::TableMoveDrag));
}
