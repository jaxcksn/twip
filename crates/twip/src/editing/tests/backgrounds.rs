//! Pictures behind the text: a header's page-sized background takes no press, hover or object step meant for the
//! body (and the body's floats none meant for the header), and a body picture behind the text yields to the text over
//! it.

use std::rc::Rc;

use super::super::Editor;
use super::super::commands::Command;
use super::super::geometry::PageGeometry;
use super::super::input::{Cursor, PointerKind};
use super::super::keymap::Mods;
use super::super::position::StoryId;
use super::super::selection::{ObjectRef, Selection};
use super::{fonts, open};
use crate::layout::FloatId;

fn fixture() -> Editor {
    let mut e = open("FX-HEADER-BACKGROUND.docx");
    e.complete(fonts());
    e
}

fn page(e: &mut Editor, i: usize) -> Rc<PageGeometry> {
    e.page_geometries().into_iter().find(|(p, _)| *p == i).map(|(_, g)| g).expect("the page")
}

/// A pointer event at page `pg`'s point.
fn at(e: &mut Editor, kind: PointerKind, pg: usize, (x, y): (f64, f64), clicks: u8) {
    let (dx, dy) = e.stack().to_doc(pg, x, y).expect("on the stack");
    e.pointer_input(kind, dx, dy, clicks, Mods::default(), fonts());
}

fn click(e: &mut Editor, pg: usize, p: (f64, f64), clicks: u8) {
    at(e, PointerKind::Down, pg, p, clicks);
    at(e, PointerKind::Up, pg, p, 0);
}

fn hover(e: &mut Editor, pg: usize, p: (f64, f64)) -> Cursor {
    at(e, PointerKind::Move, pg, p, 0);
    e.view(0..0).cursor
}

/// The page's header background: its float and box.
fn background(g: &PageGeometry) -> (FloatId, [f64; 4]) {
    g.stories.iter().find_map(|s| s.float.filter(|_| matches!(s.host, StoryId::HeaderFooter(_))).map(|f| (f, s.rect))).expect("a header float")
}

/// The body's picture behind its text.
fn behind(g: &PageGeometry) -> (FloatId, [f64; 4]) {
    g.stories.iter().find_map(|s| s.float.filter(|_| s.host == StoryId::Body).map(|f| (f, s.rect))).expect("a body float")
}

/// The middle of a body line's `k`th character, clear of the body's float.
fn on_body_text(g: &PageGeometry, k: usize) -> (f64, f64) {
    let body_floats = || g.stories.iter().filter(|s| s.float.is_some() && s.host == StoryId::Body);
    let clear = |l: &&super::super::geometry::GLine| body_floats().all(|s| l.bottom < s.rect[1] || l.top > s.rect[3]);
    let l = g.lines.iter().filter(|l| g.stories[l.story].story == StoryId::Body && l.stops.len() > 20).find(clear).expect("a body line");
    ((l.stops[k].x + l.stops[k + 1].x) / 2.0, (l.top + l.bottom) / 2.0)
}

/// A point in the body clear of its text: after its shortest line.
fn past_body_text(g: &PageGeometry) -> (f64, f64) {
    let [_, y0, _, y1] = behind_rect(g);
    let (x, l) = g
        .lines
        .iter()
        .filter(|l| g.stories[l.story].story == StoryId::Body && (l.bottom < y0 || l.top > y1))
        .filter_map(|l| l.stops.last().map(|s| (s.x, l)))
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .expect("a body line");
    assert!(x < 480.0, "a short line: {x}");
    (x + 20.0, (l.top + l.bottom) / 2.0)
}

/// The body float's box, or nothing (a page without it).
fn behind_rect(g: &PageGeometry) -> [f64; 4] {
    g.stories.iter().find(|s| s.float.is_some() && s.host == StoryId::Body).map_or([0.0; 4], |s| s.rect)
}

fn body_caret(e: &Editor) -> bool {
    matches!(e.selection(), Selection::Caret(p) if p.story == StoryId::Body)
}

/// A double-click on the page's header text: header editing.
fn edit_header(e: &mut Editor, pg: usize) {
    let g = page(e, pg);
    let l = g.lines.iter().find(|l| matches!(g.stories[l.story].story, StoryId::HeaderFooter(_))).expect("header text");
    click(e, pg, ((l.stops[0].x + l.stops[1].x) / 2.0, (l.top + l.bottom) / 2.0), 2);
    assert!(matches!(e.selection().focus().map(|p| p.story), Some(StoryId::HeaderFooter(_))), "in the header: {:?}", e.selection());
}

#[test]
fn presses_in_the_body_over_the_header_background_place_a_body_caret() {
    let mut e = fixture();
    let (first, second) = (page(&mut e, 0), page(&mut e, 1));
    assert_ne!(background(&first).0, background(&second).0, "the first page's header and the default one");
    for (pg, g) in [(0, first), (1, second)] {
        let p = on_body_text(&g, 3);
        let [x0, y0, x1, y1] = background(&g).1;
        assert!(p.0 > x0 && p.0 < x1 && p.1 > y0 && p.1 < y1, "the background is under the text");
        click(&mut e, pg, p, 1);
        assert!(body_caret(&e), "page {pg}: {:?}", e.selection());
        assert_eq!(hover(&mut e, pg, p), Cursor::Text, "page {pg}");
        // Past the text, as on a page without the background.
        let q = past_body_text(&g);
        assert_eq!(hover(&mut e, pg, q), Cursor::Text, "page {pg}, past the text");
        click(&mut e, pg, q, 1);
        assert!(body_caret(&e), "page {pg}, past the text: {:?}", e.selection());
        // The selection bar.
        assert_eq!(hover(&mut e, pg, (36.0, p.1)), Cursor::RowSelect, "page {pg}, the margin");
    }
}

#[test]
fn a_drag_and_a_double_click_over_the_header_background_select_body_text() {
    let mut e = fixture();
    let g = page(&mut e, 1);
    let (a, b) = (on_body_text(&g, 2), on_body_text(&g, 12));
    at(&mut e, PointerKind::Down, 1, a, 1);
    at(&mut e, PointerKind::Move, 1, b, 0);
    at(&mut e, PointerKind::Up, 1, b, 0);
    let s = e.selection();
    assert!(
        matches!(s, Selection::Text { anchor, focus, .. } if anchor.story == StoryId::Body && focus.story == StoryId::Body && anchor != focus),
        "{s:?}"
    );
    click(&mut e, 1, a, 2);
    assert!(matches!(e.selection(), Selection::Text { focus, .. } if focus.story == StoryId::Body), "a word: {:?}", e.selection());
}

#[test]
fn in_header_editing_the_background_is_selected_and_dragged() {
    let mut e = fixture();
    edit_header(&mut e, 0);
    let (f, [x0, y0, x1, _]) = background(&page(&mut e, 0));
    // Above the header text, at the page's right.
    let p = (x1 - 20.0, y0 + 10.0);
    assert_eq!(hover(&mut e, 0, p), Cursor::Move);
    at(&mut e, PointerKind::Down, 0, p, 1);
    assert_eq!(e.selection(), Selection::Object(ObjectRef::Floating(f)));
    at(&mut e, PointerKind::Move, 0, (p.0 + 10.0, p.1 + 10.0), 0);
    at(&mut e, PointerKind::Up, 0, (p.0 + 10.0, p.1 + 10.0), 0);
    e.complete(fonts());
    let (g, [nx0, ny0, ..]) = background(&page(&mut e, 0));
    assert_eq!(g, f);
    assert!((nx0 - x0 - 10.0).abs() < 0.5 && (ny0 - y0 - 10.0).abs() < 0.5, "moved: {x0},{y0} to {nx0},{ny0}");
    assert_eq!(e.selection(), Selection::Object(ObjectRef::Floating(f)), "still selected");
    assert_eq!(hover(&mut e, 0, (p.0 + 10.0, p.1 + 10.0)), Cursor::Move, "the header stays edited");
}

#[test]
fn in_header_editing_the_body_floats_take_nothing() {
    let mut e = fixture();
    edit_header(&mut e, 0);
    let g = page(&mut e, 0);
    let (header, (body, [_, y0, x1, y1])) = (background(&g).0, behind(&g));
    // On the body's float, over the header's background: the background's.
    click(&mut e, 0, (x1 - 10.0, (y0 + y1) / 2.0), 1);
    assert_eq!(e.selection(), Selection::Object(ObjectRef::Floating(header)));
    // On body text, over the background: the background's too; never the body's float or text.
    click(&mut e, 0, on_body_text(&g, 3), 1);
    assert_eq!(e.selection(), Selection::Object(ObjectRef::Floating(header)));
    let five = e.key_command("Digit5", Mods { ctrl: true, alt: true, ..Mods::default() }).expect("Ctrl+Alt+5");
    e.execute(five, fonts());
    let tab = e.key_command("Tab", Mods::default()).expect("Tab");
    e.execute(tab, fonts());
    assert_eq!(e.selection(), Selection::Object(ObjectRef::Floating(header)), "the header's only float, round");
    assert_ne!(header, body);
}

#[test]
fn text_over_a_behind_picture_takes_the_press_and_the_picture_the_rest() {
    let mut e = fixture();
    let g = page(&mut e, 0);
    let (f, [x0, y0, x1, y1]) = behind(&g);
    // A character of a body line drawn over the picture.
    let on_text = g
        .lines
        .iter()
        .filter(|l| g.stories[l.story].story == StoryId::Body && l.top > y0 && l.bottom < y1)
        .find_map(|l| l.stops.windows(2).find(|w| w[0].x > x0 + 2.0).map(|w| ((w[0].x + w[1].x) / 2.0, (l.top + l.bottom) / 2.0)))
        .expect("text over the picture");
    assert_eq!(hover(&mut e, 0, on_text), Cursor::Text);
    click(&mut e, 0, on_text, 1);
    assert!(body_caret(&e), "{:?}", e.selection());
    // In the margin, where no line is.
    let off_text = (x1 - 10.0, (y0 + y1) / 2.0);
    assert_eq!(hover(&mut e, 0, off_text), Cursor::Move);
    click(&mut e, 0, off_text, 1);
    assert_eq!(e.selection(), Selection::Object(ObjectRef::Floating(f)));
}

#[test]
fn object_steps_from_the_body_skip_the_header_floats() {
    let mut e = fixture();
    let g = page(&mut e, 0);
    let body = behind(&g).0;
    click(&mut e, 0, on_body_text(&g, 3), 1);
    let five = e.key_command("Digit5", Mods { ctrl: true, alt: true, ..Mods::default() }).expect("Ctrl+Alt+5");
    e.execute(five, fonts());
    assert_eq!(e.selection(), Selection::Object(ObjectRef::Floating(body)));
    let tab = e.key_command("Tab", Mods::default()).expect("Tab");
    e.execute(tab, fonts());
    assert_eq!(e.selection(), Selection::Object(ObjectRef::Floating(body)), "the body's only float, round");
    e.execute(Command::Escape, fonts());
    assert!(body_caret(&e), "Esc returns to the body text");
}
