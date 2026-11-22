//! Replacing the document ends what the host was shown of the last one

use std::cell::RefCell;
use std::rc::Rc;

use crate::api::command::Command;
use crate::api::engine::{Engine, SessionId, SessionOptions};
use crate::api::event::Event;
use crate::api::fonts::FontLibrary;
use crate::api::input::{Modifiers, Point, PointerButton as B, PointerKind as K};
use crate::api::review::Body;
use crate::api::session::Session;
use crate::api::value::Points;

type Events = Rc<RefCell<Vec<Event>>>;

fn setup() -> (Engine, SessionId, Events) {
    let mut engine = Engine::new(FontLibrary::system_recommended());
    let events = Events::default();
    let sink = events.clone();
    engine.set_listener(move |_, e| sink.borrow_mut().push(e.clone()));
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("a session");
    s.run(Command::InsertText("Hello".into())).expect("types");
    s.host().set_size(800.0, 600.0, 1.0);
    (engine, id, events)
}

fn frame(s: &mut Session) -> Vec<u8> {
    let mut px = vec![0u8; 800 * 600 * 4];
    s.host().frame_rgba(&mut px, 800, 600).expect("a frame");
    px
}

fn anchor_events(events: &Events) -> Vec<usize> {
    events.borrow().iter().filter_map(|e| if let Event::CommentAnchors(a) = e { Some(a.len()) } else { None }).collect()
}

fn hears_the_anchors_go(replace: impl Fn(&mut Session)) {
    let (mut engine, id, events) = setup();
    let s = engine.session(id).expect("a session");
    s.run(Command::SelectAll).expect("selects");
    s.run(Command::AddComment(Body::Text("Note".into()))).expect("comments");
    frame(s);
    assert_eq!(anchor_events(&events), [1], "the comment's anchor was reported");
    events.borrow_mut().clear();
    replace(s);
    frame(s);
    assert_eq!(anchor_events(&events), [0], "the host hears that the anchors are gone");
}

#[test]
fn a_new_document_reports_the_comment_anchors_gone() {
    hears_the_anchors_go(|s| s.new_document());
}

#[test]
fn opening_reports_the_comment_anchors_gone() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/FX-TEXT.docx");
    let bytes = std::fs::read(path).expect("fixture");
    hears_the_anchors_go(|s| {
        s.open(&bytes).expect("opens");
    });
}

#[test]
fn a_new_document_drops_the_leader_lines() {
    let (mut engine, id, _) = setup();
    let s = engine.session(id).expect("a session");
    s.new_document();
    let plain = frame(s);
    s.host().set_leaders(vec![vec![Point { x: 10.0, y: 150.5 }, Point { x: 390.0, y: 150.5 }]]);
    assert!(frame(s) != plain, "the leader draws");
    s.new_document();
    assert!(frame(s) == plain, "no leader of the old document remains");
}

#[test]
fn a_ruler_drag_does_not_survive_a_new_document() {
    let (mut engine, id, _) = setup();
    let s = engine.session(id).expect("a session");
    s.host().set_rulers(true, true);
    frame(s);
    let x = |pt: f64| 24.0 + (72.0 + pt) / 0.75;
    let at = |s: &mut Session, k: K, x: f64| s.pointer(k, Point { x, y: 5.0 }, B::Primary, u8::from(k == K::Down), Modifiers::default());
    at(s, K::Down, x(0.0));
    at(s, K::Move, x(19.44));
    assert!(s.presentation.ruler_drag.is_some(), "a drag is under way");
    s.new_document();
    assert!(s.presentation.ruler_drag.is_none());
    at(s, K::Up, x(19.44));
    assert_eq!(s.state().ruler.horizontal.indents.first_line, Points(0.0), "the old gesture didn't run on the new document");
    assert_eq!(s.state().history.undo_label, None, "nothing was recorded");
}

#[test]
fn a_popover_closes_with_the_document() {
    let (mut engine, id, events) = setup();
    let s = engine.session(id).expect("a session");
    s.host().set_rulers(true, true);
    frame(s);
    s.pointer(K::Down, Point { x: 5.0, y: 5.0 }, B::Primary, 1, Modifiers::default());
    s.pointer(K::Up, Point { x: 5.0, y: 5.0 }, B::Primary, 0, Modifiers::default());
    assert!(events.borrow().iter().any(|e| matches!(e, Event::PopoverRequested(_))), "a popover opened");
    events.borrow_mut().clear();
    s.new_document();
    assert!(events.borrow().iter().any(|e| matches!(e, Event::PopoverClosed)), "popoverClosed is sent");
    assert!(s.presentation.popover.is_none());
}
