//! Table border drags through the API session, as a host drives them: a 1000 x 800 CSS px viewport at 1x, the
//! document fully laid out. A drag's cost is what the user waits for: each pointer move with the frame that shows
//! it, and the release with the frame that shows the committed layout.

use std::cell::RefCell;

use divan::Bencher;
use twip::api::command::{Command, MoveSelection, MoveUnit};
use twip::api::engine::{Engine, SessionId, SessionOptions};
use twip::api::fonts::FontLibrary;
use twip::api::input::{Modifiers, Point, PointerButton, PointerKind};
use twip::api::services::FindQuery;
use twip::api::value::{Direction, Order};

const W: u32 = 1000;
const H: u32 = 800;
const DOCS: [&str; 2] = ["editor/FX-TBL-LARGE.docx", "editor/FX-TBL-LARGE-NESTED.docx"];
const NONE: Modifiers = Modifiers { ctrl: false, shift: false, alt: false, meta: false };

/// Which border a drag moves.
#[derive(Clone, Copy)]
enum Border {
    /// The column border left of the third column, dragged sideways.
    Column,
    /// The first body row's bottom border, dragged down.
    Row,
}

/// The session on `name`, laid out with a frame drawn, and the place (client px) of a border: the column border left
/// of the cell `1.3 value` (its text's start less the 5.4 pt cell margin), or the bottom border of that cell's row.
fn session(name: &str, border: Border, pixels: &mut [u8]) -> (Engine, SessionId, Point) {
    let mut engine = Engine::new(FontLibrary::system());
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("the session was just created");
    s.open(&super::fixture(name)).expect("the fixture opens");
    s.host().set_size(f64::from(W), f64::from(H), 1.0);
    s.host().set_focus(true);
    while s.host().idle() {}
    s.host().frame_rgba(pixels, W, H).expect("a frame");
    assert!(s.find(&FindQuery { text: "1.3 value".into(), ..FindQuery::default() }, Order::Next).found, "the cell text");
    let back = MoveSelection { by: MoveUnit::Character, direction: Direction::Backward, extend: None };
    s.run(Command::MoveSelection(back)).expect("the caret moves");
    let c = s.host().frame_info();
    let at = match border {
        Border::Column => Point { x: c.caret_x - 5.4 * 4.0 / 3.0, y: c.caret_y + c.caret_height / 2.0 },
        Border::Row => Point { x: c.caret_x + 10.0, y: c.caret_y + c.caret_height + 1.0 },
    };
    (engine, id, at)
}

/// `at` moved `d` px along the drag's direction.
fn along(at: Point, border: Border, d: f64) -> Point {
    match border {
        Border::Column => Point { x: at.x + d, ..at },
        Border::Row => Point { y: at.y + d, ..at },
    }
}

/// One pointer move of a pressed border, alternating between 10 and 30 px from where it was pressed, with the frame
/// that shows it. Budget: p95 at most 2 ms of engine time, inside one 8.3 ms frame.
fn moving(bencher: Bencher, name: &str, border: Border) {
    let mut pixels = vec![0u8; (W * H * 4) as usize];
    let (mut engine, id, at) = session(name, border, &mut pixels);
    let s = engine.session(id).expect("the session");
    s.pointer(PointerKind::Down, at, PointerButton::Primary, 1, NONE);
    let (s, pixels, mut far) = (RefCell::new(s), RefCell::new(pixels), false);
    bencher
        .with_inputs(|| {
            far = !far;
            along(at, border, if far { 30.0 } else { 10.0 })
        })
        .bench_local_values(|to| {
            let mut s = s.borrow_mut();
            s.pointer(PointerKind::Move, to, PointerButton::Primary, 0, NONE);
            s.host().frame_rgba(&mut pixels.borrow_mut(), W, H).expect("a frame")
        });
}

/// A column border's pointer move with its frame. Budget: p95 at most 2 ms of engine time.
#[divan::bench(args = DOCS, sample_size = 1)]
fn column_border(bencher: Bencher, name: &str) {
    moving(bencher, name, Border::Column);
}

/// A row border's pointer move with its frame. Budget: p95 at most 2 ms of engine time.
#[divan::bench(args = DOCS, sample_size = 1)]
fn row_border(bencher: Bencher, name: &str) {
    moving(bencher, name, Border::Row);
}

/// A column border dragged 20 px (untimed), then released: the resize committed and laid out through the visible
/// pages, with the frame that shows it. Undone between samples. Budget: p95 at most 8.3 ms, one frame.
#[divan::bench(args = DOCS, sample_size = 1)]
fn release(bencher: Bencher, name: &str) {
    let mut pixels = vec![0u8; (W * H * 4) as usize];
    let (mut engine, id, at) = session(name, Border::Column, &mut pixels);
    let s = engine.session(id).expect("the session");
    let to = along(at, Border::Column, 20.0);
    // The drag hits the border: one release commits the resize (the revision moves), undone.
    let revision = s.state().revision;
    s.pointer(PointerKind::Down, at, PointerButton::Primary, 1, NONE);
    s.pointer(PointerKind::Move, to, PointerButton::Primary, 0, NONE);
    s.pointer(PointerKind::Up, to, PointerButton::Primary, 1, NONE);
    assert!(s.state().revision > revision, "the release committed the resize");
    s.run(Command::Undo).expect("undoes");
    let (s, pixels, mut dragged, prof) = (RefCell::new(s), RefCell::new(pixels), false, RefCell::new(super::Prof::default()));
    bencher
        .with_inputs(|| {
            let mut s = s.borrow_mut();
            if std::mem::replace(&mut dragged, true) {
                s.run(Command::Undo).expect("undoes");
            }
            while s.host().idle() {}
            s.pointer(PointerKind::Down, at, PointerButton::Primary, 1, NONE);
            s.pointer(PointerKind::Move, to, PointerButton::Primary, 0, NONE);
            super::Prof::skip();
        })
        .bench_local_values(|()| {
            let mut s = s.borrow_mut();
            s.pointer(PointerKind::Up, to, PointerButton::Primary, 1, NONE);
            let frame = s.host().frame_rgba(&mut pixels.borrow_mut(), W, H).expect("a frame");
            prof.borrow_mut().add();
            frame
        });
    prof.borrow_mut().print(&format!("release {name}"));
}
