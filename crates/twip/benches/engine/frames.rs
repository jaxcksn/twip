//! Frames through the API session as an SDK draws them: a 1000 x 800 CSS px viewport at 1x on `big.docx`, its whole
//! layout done (the background layout timer has run by the time a user scrolls or selects).

use std::cell::{Cell, RefCell};

use divan::Bencher;
use twip::api::engine::{Engine, SessionId, SessionOptions};
use twip::api::fonts::FontLibrary;
use twip::api::input::{Key, Modifiers};

const W: u32 = 1000;
const H: u32 = 800;

/// The engine (it owns the session) and the session, focused and fully laid out, its first frame drawn.
fn session(pixels: &mut [u8]) -> (Engine, SessionId) {
    let mut engine = Engine::new(FontLibrary::system());
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("the session was just created");
    s.open(&super::fixture("big.docx")).expect("the fixture opens");
    s.host().set_size(f64::from(W), f64::from(H), 1.0);
    s.host().set_focus(true);
    while s.host().idle() {}
    s.host().frame_rgba(pixels, W, H).expect("a frame");
    (engine, id)
}

/// An unchanged frame on the first screen and one a few pages in, with the pages' drawing kept (`retained`) or
/// drawn from scratch.
#[divan::bench(args = [(0.0, true), (0.0, false), (3000.0, true), (3000.0, false)])]
fn unchanged(bencher: Bencher, (scroll, retained): (f64, bool)) {
    let mut pixels = vec![0u8; (W * H * 4) as usize];
    let (mut engine, id) = session(&mut pixels);
    let s = engine.session(id).expect("the session");
    s.host().set_scroll(0.0, scroll);
    s.host().set_retained(retained);
    bencher.bench_local(|| s.host().frame_rgba(&mut pixels, W, H).expect("a frame"));
}

/// A frame 200 CSS px further down, from the first page to the last and back. Budget: under 16.7 ms.
#[divan::bench(sample_size = 1)]
fn scroll(bencher: Bencher) {
    let mut pixels = vec![0u8; (W * H * 4) as usize];
    let (mut engine, id) = session(&mut pixels);
    let s = engine.session(id).expect("the session");
    let height = s.host().frame_rgba(&mut pixels, W, H).expect("a frame").content_height;
    let (mut y, mut step) = (0.0, 200.0);
    let (s, pixels) = (RefCell::new(s), RefCell::new(pixels));
    bencher
        .with_inputs(|| {
            if !(0.0..height).contains(&(y + step)) {
                step = -step;
            }
            y += step;
            s.borrow_mut().host().set_scroll(0.0, y);
        })
        .bench_local_values(|()| s.borrow_mut().host().frame_rgba(&mut pixels.borrow_mut(), W, H).expect("a frame"));
}

/// A selection extended a line with Shift+Down (the state and events it makes), then the frame after it; the
/// selection collapses back to the start every 60 lines.
#[divan::bench(args = ["input", "frame"], sample_size = 1)]
fn select_line(bencher: Bencher, part: &str) {
    let mut pixels = vec![0u8; (W * H * 4) as usize];
    let (mut engine, id) = session(&mut pixels);
    let s = engine.session(id).expect("the session");
    let shift = Modifiers { shift: true, ..Modifiers::default() };
    let (s, pixels, n) = (RefCell::new(s), RefCell::new(pixels), Cell::new(0));
    let next = || {
        n.set(n.get() + 1);
        if n.get() % 60 == 0 {
            s.borrow_mut().key(Key::Home, Modifiers { ctrl: true, ..Modifiers::default() });
        }
    };
    let extend = || {
        s.borrow_mut().key(Key::ArrowDown, shift);
    };
    let frame = || {
        s.borrow_mut().host().frame_rgba(&mut pixels.borrow_mut(), W, H).expect("a frame");
    };
    if part == "input" {
        bencher
            .with_inputs(|| {
                next();
                frame();
            })
            .bench_local_values(|()| extend());
    } else {
        bencher
            .with_inputs(|| {
                next();
                extend();
            })
            .bench_local_values(|()| frame());
    }
}
