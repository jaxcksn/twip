use std::cell::RefCell;
use std::rc::Rc;

use crate::api::command::Command;
use crate::api::engine::{Engine, SessionOptions};
use crate::api::error::ErrorCode;
use crate::api::event::Event;
use crate::api::fonts::FontLibrary;
use crate::api::record::Origin;
use crate::api::session::{ExportFormat, ExportOptions};
use crate::api::state::Tri;
use crate::api::value::Points;

fn engine() -> (Engine, Rc<RefCell<Vec<Event>>>) {
    let mut engine = Engine::new(FontLibrary::empty());
    let events = Rc::new(RefCell::new(Vec::new()));
    let sink = events.clone();
    engine.set_listener(move |_, e| sink.borrow_mut().push(e.clone()));
    (engine, events)
}

#[test]
fn bold_undo_redo_report_state_then_events() {
    let (mut engine, events) = engine();
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("a session");
    s.run(Command::InsertText("Hello".into())).expect("types");
    s.run(Command::SelectAll).expect("selects");
    let commit = s.run(Command::SetBold(true)).expect("bolds");
    assert_eq!(s.state().formatting.bold, Tri::Uniform(true));
    assert_eq!(commit.record.map(|r| r.origin), Some(Origin::Api));
    let undo = s.run(Command::Undo).expect("undoes");
    assert_eq!(undo.record.as_ref().map(|r| (r.origin, r.label.as_str())), Some((Origin::Undo, "Bold")));
    assert_eq!(s.state().formatting.bold, Tri::Uniform(false));
    assert!(s.state().history.can_redo);
    s.run(Command::Redo).expect("redoes");
    assert_eq!(s.state().formatting.bold, Tri::Uniform(true));
    let events = events.borrow();
    let changes: Vec<usize> = events.iter().enumerate().filter(|(_, e)| matches!(e, Event::DocumentChanged(_))).map(|(i, _)| i).collect();
    assert_eq!(changes.len(), 4, "{events:?}");
    for i in changes {
        let (Event::StateChanged(state), Event::DocumentChanged(record)) = (&events[i - 1], &events[i]) else { panic!("state before each change") };
        assert_eq!(state.revision, record.revision);
    }
}

#[test]
fn refusals_have_their_codes() {
    let (mut engine, _) = engine();
    let id = engine.create_session(SessionOptions { read_only: true, ..SessionOptions::default() });
    let s = engine.session(id).expect("a session");
    assert_eq!(s.run(Command::InsertText("x".into())).map_err(|e| e.code).err(), Some(ErrorCode::ReadOnly));
    s.set_read_only(false);
    assert_eq!(s.run(Command::Undo).map_err(|e| e.code).err(), Some(ErrorCode::Unavailable));
    assert_eq!(s.run(Command::InsertText(String::new())).map_err(|e| e.code).err(), Some(ErrorCode::InvalidArgument));
    assert_eq!(s.run(Command::CopyFormatting).map_err(|e| e.code).err(), Some(ErrorCode::Unsupported));
    assert_eq!(s.run(Command::SetFontSize(Points(-3.0))).map_err(|e| e.code).err(), Some(ErrorCode::InvalidArgument));
    assert_eq!(s.run(Command::MergeCells).map_err(|e| e.code).err(), Some(ErrorCode::Unavailable));
    assert_eq!(s.open(b"not a docx").map_err(|e| e.code).err(), Some(ErrorCode::ImportFailed));
    engine.close_session(id);
    assert!(engine.session(id).is_none());
}

#[test]
fn export_captures_the_revision_and_clears_modified() {
    let (mut engine, _) = engine();
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("a session");
    s.run(Command::InsertText("Hi".into())).expect("types");
    assert!(s.state().modified);
    let export = s.export(&ExportOptions { format: ExportFormat::Docx }).expect("exports");
    assert_eq!(export.revision, s.state().revision);
    assert!(!s.state().modified);
    let warnings = s.open(&export.bytes.0).expect("reads back");
    // The library is empty, so only the font warning.
    assert!(warnings.iter().all(|w| w.code == crate::api::warning::WarningCode::FontUnresolved), "{warnings:?}");
}

#[test]
fn a_typical_state_is_under_9_kb() {
    let (mut engine, _) = engine();
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("a session");
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/FX-TBL-BASIC.docx");
    s.open(&std::fs::read(path).expect("the fixture")).expect("opens");
    s.run(Command::SelectAll).expect("selects");
    // The selected text comes on top, up to its limit.
    let mut state = s.state().clone();
    let text = state.selection.text.take().expect("a short selection's text");
    assert!(text.contains('\n'), "{text:?}");
    let json = serde_json::to_string(&state).expect("serializes");
    assert!(json.len() < 9 * 1024, "{} bytes", json.len());
}

#[test]
fn keys_set_from_the_selection_and_selection_changes_are_events() {
    use crate::api::input::{Key, Modifiers};
    let (mut engine, events) = engine();
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("a session");
    s.text("Hello");
    s.run(Command::SelectAll).expect("selects");
    let ctrl = Modifiers { ctrl: true, ..Modifiers::default() };
    s.key(Key::KeyB, ctrl);
    assert_eq!(s.state().formatting.bold, Tri::Uniform(true));
    assert_eq!(s.state().commands[&crate::api::command::CommandId::SetBold].checked, Some(true));
    s.key(Key::KeyB, ctrl);
    assert_eq!(s.state().formatting.bold, Tri::Uniform(false));
    assert!(events.borrow().iter().any(|e| matches!(e, Event::SelectionChanged(_))));
    assert_eq!(s.key(Key::KeyD, ctrl), crate::api::event::KeyOutcome::Handled);
    assert!(events.borrow().iter().any(|e| matches!(e, Event::DialogRequested { .. })));
}

#[test]
fn a_batch_is_one_transaction_and_one_undo_step() {
    let (mut engine, events) = engine();
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("a session");
    s.run(Command::InsertText("Hello".into())).expect("types");
    events.borrow_mut().clear();
    let commit = s.batch(vec![Command::InsertText(" world".into()), Command::SelectAll, Command::SetBold(true)], Some("Block".into())).expect("runs");
    let record = commit.record.expect("a record");
    assert_eq!((record.revision, record.origin, record.label.as_str(), record.summary.paragraphs), (3, Origin::Api, "Block", 1));
    let changes = events.borrow().iter().filter(|e| matches!(e, Event::DocumentChanged(_))).count();
    assert_eq!(changes, 1);
    assert_eq!(s.state().history.undo_label.as_deref(), Some("Block"));
    s.run(Command::Undo).expect("undoes");
    assert_eq!((s.state().revision, s.state().formatting.bold), (4, Tri::Uniform(false)));
    // Checked before anything runs: a bad command changes nothing.
    events.borrow_mut().clear();
    let bad = s.batch(vec![Command::InsertText("never".into()), Command::InsertText(String::new())], None);
    assert_eq!(bad.map_err(|e| e.code).err(), Some(ErrorCode::InvalidArgument));
    assert_eq!(s.state().revision, 4);
    assert!(events.borrow().is_empty(), "{:?}", events.borrow());
}

#[test]
fn modes_switch_without_a_revision() {
    let (mut engine, _) = engine();
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("a session");
    let before = (s.state().revision, s.state().sequence);
    s.set_mode(crate::api::state::Mode::Design);
    s.set_read_only(true);
    assert_eq!(s.state().revision, before.0);
    assert_eq!(s.state().sequence, before.1 + 2);
    assert_eq!(s.batch(vec![Command::InsertText("x".into())], None).map_err(|e| e.code).err(), Some(ErrorCode::ReadOnly));
}

#[test]
fn undoing_to_the_save_clears_modified() {
    let (mut engine, _) = engine();
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("a session");
    s.run(Command::InsertText("Hi".into())).expect("types");
    s.export(&ExportOptions { format: ExportFormat::Docx }).expect("exports");
    s.run(Command::SelectAll).expect("selects");
    s.run(Command::SetBold(true)).expect("bolds");
    assert!(s.state().modified);
    s.run(Command::Undo).expect("undoes");
    assert!(!s.state().modified, "back where it was saved");
}

#[test]
fn a_pdf_export_keeps_modified_and_warns_of_fallbacks() {
    let mut engine = Engine::new(FontLibrary::system());
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("a session");
    s.run(Command::InsertText("Hi".into())).expect("types");
    s.run(Command::SelectAll).expect("selects");
    s.run(Command::SetFont("No Such Family Here".into())).expect("sets the font");
    let pdf = s.export(&ExportOptions { format: ExportFormat::Pdf }).expect("exports");
    assert!(pdf.bytes.0.starts_with(b"%PDF"));
    assert_eq!(pdf.revision, s.state().revision);
    assert!(s.state().modified, "a PDF doesn't save the document");
    assert_eq!(pdf.warnings.iter().map(|w| w.code).collect::<Vec<_>>(), [crate::api::warning::WarningCode::PdfFontFallback]);
}

#[test]
fn copies_paste_formatted_in_any_session_and_as_text() {
    let (mut engine, _) = engine();
    let (a, b) = (engine.create_session(SessionOptions::default()), engine.create_session(SessionOptions::default()));
    let s = engine.session(a).expect("a session");
    s.run(Command::InsertText("Hello".into())).expect("types");
    s.run(Command::SelectAll).expect("selects");
    s.run(Command::SetBold(true)).expect("bolds");
    // A cut takes the text in one transaction.
    assert_eq!(s.copy(true).map(|d| d.text), Some("Hello".into()));
    s.run(Command::SelectAll).expect("selects");
    assert_eq!(s.copy(false), None, "nothing left to copy");
    s.run(Command::Undo).expect("undoes the cut");
    s.run(Command::SelectAll).expect("selects");
    let data = s.copy(false).expect("a copy");
    assert_eq!(data.text, "Hello");
    assert!(data.html.as_deref().is_some_and(|h| h.contains("Hello")) && data.internal.is_some());

    let s = engine.session(b).expect("a session");
    s.paste(&data, false);
    s.run(Command::SelectAll).expect("selects");
    assert_eq!(s.state().formatting.bold, Tri::Uniform(true), "the copy keeps its formatting in another session");
    s.run(Command::Undo).expect("one step");
    s.run(Command::SelectAll).expect("selects");
    assert_eq!(s.copy(false), None);
    s.paste(&data, true);
    s.run(Command::SelectAll).expect("selects");
    assert_eq!(s.state().formatting.bold, Tri::Uniform(false), "as text");
    // Another engine's copy with the same token but other text is text.
    s.paste(&crate::api::input::ClipboardData { text: "Other".into(), ..data.clone() }, false);
    s.run(Command::SelectAll).expect("selects");
    assert_eq!(s.copy(false).map(|d| d.text), Some("Other".into()));
}

#[test]
fn ctrl_wheel_zooms_and_a_plain_wheel_does_not() {
    use crate::api::input::Modifiers;
    let (mut engine, _) = engine();
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("a session");
    let zoom = |s: &mut crate::api::session::Session| s.state().view.zoom;
    s.wheel(0.0, 100.0, Modifiers::default());
    assert_eq!(zoom(s), 100.0);
    s.wheel(0.0, -100.0, Modifiers { ctrl: true, ..Modifiers::default() });
    assert!((zoom(s) - 110.0).abs() < 1e-9, "{}", zoom(s));
    s.wheel(0.0, 1e6, Modifiers { meta: true, ..Modifiers::default() });
    assert_eq!(zoom(s), 10.0);
}

#[test]
fn background_layout_grows_the_page_count_in_new_states() {
    let (mut engine, events) = engine();
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("a session");
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/FX-outline.docx");
    s.open(&std::fs::read(path).expect("the fixture")).expect("opens");
    let (opened, revision) = (s.state().pages.clone(), s.state().revision);
    assert!(!opened.complete, "{opened:?}");
    events.borrow_mut().clear();
    while s.host().idle() {}
    let pages = s.state().pages.clone();
    assert!(pages.complete && pages.count > opened.count, "{opened:?} then {pages:?}");
    let states: Vec<_> = events.borrow().iter().filter_map(|e| if let Event::StateChanged(s) = e { Some(s.clone()) } else { None }).collect();
    assert!(!states.is_empty() && states.iter().all(|st| st.revision == revision), "the same revision");
    assert!(states.windows(2).all(|w| w[0].sequence < w[1].sequence && w[0].pages != w[1].pages), "a state per change only");
    assert!(!s.host().idle(), "nothing left");
}

#[test]
fn the_state_carries_the_selected_text_up_to_its_limit() {
    use crate::api::state::SELECTION_TEXT_LIMIT;
    let (mut engine, _) = engine();
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("a session");
    assert_eq!(s.state().selection.text.as_deref(), Some(""), "a caret");
    s.run(Command::InsertText("ab".into())).expect("types");
    s.run(Command::InsertParagraph).expect("breaks");
    s.run(Command::InsertText("x".repeat(SELECTION_TEXT_LIMIT - 3))).expect("types");
    s.run(Command::SelectAll).expect("selects");
    assert_eq!(s.state().selection.text.as_ref().map(|t| t.chars().count()), Some(SELECTION_TEXT_LIMIT));
    assert!(s.state().selection.text.as_ref().is_some_and(|t| t.starts_with("ab\nxx")));
    s.run(Command::MoveSelection(crate::api::command::MoveSelection {
        by: crate::api::command::MoveUnit::Document,
        direction: crate::api::value::Direction::Forward,
        extend: None,
    }))
    .expect("moves");
    s.run(Command::InsertText("y".into())).expect("types");
    s.run(Command::SelectAll).expect("selects");
    assert_eq!(s.state().selection.text, None, "over the limit");
}

#[test]
fn a_relayout_no_edit_laid_out_is_laid_out_before_the_caret_and_clicks_read_it() {
    use crate::api::input::{Modifiers, Point, PointerButton, PointerKind};
    let mut engine = Engine::new(FontLibrary::empty());
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("a session");
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../bindings/hello.docx");
    s.open(&std::fs::read(path).expect("the fixture")).expect("opens");
    s.host().attach(|_| {});
    s.host().set_size(800.0, 600.0, 1.0);
    s.host().set_focus(true);
    assert!(!s.host().frame_info().caret_visible, "no font: no lines, no caret");
    // A font arrives: the whole document lays out again, and no edit asks for it.
    let font = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/fonts/stand-in-serif.ttf");
    engine.fonts().add(&std::fs::read(font).expect("the font")).expect("adds");
    engine.sync_fonts();
    let s = engine.session(id).expect("a session");
    let start = s.host().frame_info();
    assert!(start.caret_visible && start.caret_x > 0.0 && start.caret_y > 0.0, "{start:?}");
    // A click further along the first line moves the caret there.
    let at = Point { x: start.caret_x + 30.0, y: start.caret_y + start.caret_height / 2.0 };
    for kind in [PointerKind::Down, PointerKind::Up] {
        s.pointer(kind, at, PointerButton::Primary, 1, Modifiers::default());
    }
    let clicked = s.host().frame_info();
    assert!(clicked.caret_x > start.caret_x, "{start:?} then {clicked:?}");
    while s.host().idle() {}
    assert!(s.state().pages.complete);
}

#[test]
fn the_paragraph_dialog_sets_a_first_line_or_hanging_indent() {
    use crate::api::services::{DialogKind, DialogPatch, ParagraphPatch, SpecialIndent};
    let (mut engine, _) = engine();
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("a session");
    s.run(Command::InsertText("Hello".into())).expect("types");
    for (special, first_line, hanging) in [
        (SpecialIndent::FirstLine, Points(36.0), Points(0.0)),
        (SpecialIndent::Hanging, Points(0.0), Points(18.0)),
        (SpecialIndent::None, Points(0.0), Points(0.0)),
    ] {
        let by = if special == SpecialIndent::Hanging { hanging } else { first_line };
        let m = s.dialog_open(DialogKind::Paragraph).expect("opens");
        let patch = ParagraphPatch { special: Some(Tri::Uniform(special)), by: Some(Tri::Uniform(by)), ..ParagraphPatch::default() };
        let m = s.dialog_update(m, DialogPatch::Paragraph(patch)).expect("updates");
        s.dialog_apply(m).expect("applies");
        let i = &s.state().paragraph.indents;
        assert_eq!((&i.first_line, &i.hanging), (&Tri::Uniform(first_line), &Tri::Uniform(hanging)), "{special:?}");
    }
}

#[test]
fn applying_a_font_checks_it_against_the_library() {
    let (mut engine, events) = engine();
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("a session");
    s.run(Command::InsertText("Hello".into())).expect("types");
    s.run(Command::SelectAll).expect("selects");
    events.borrow_mut().clear();
    s.run(Command::SetFont("Zorble Sans".into())).expect("sets the font");
    let events = events.borrow();
    assert!(
        events.iter().any(|e| matches!(e, Event::FontsChanged { unresolved, .. } if unresolved.iter().any(|f| f == "Zorble Sans"))),
        "{events:?}"
    );
    assert!(events.iter().any(|e| matches!(e, Event::Warning(w) if w.message.contains("Zorble Sans"))), "{events:?}");
    assert!(s.state().fonts.unresolved.iter().any(|f| f == "Zorble Sans"), "the state has it too");
}

#[test]
fn verified_frames_match_their_drawing_from_scratch() {
    let (mut engine, _) = engine();
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("a session");
    s.run(Command::InsertText("Hello".into())).expect("types");
    s.host().set_size(400.0, 300.0, 1.0);
    s.host().set_verify_frames(true);
    let mut pixels = vec![0u8; 400 * 300 * 4];
    for _ in 0..3 {
        s.host().frame_rgba(&mut pixels, 400, 300).expect("a frame");
    }
    assert_eq!(s.host().verify_failures(), 0);
    let stats = s.host().frame_stats();
    assert_eq!(stats.frames, 3, "verification draws don't count: {stats:?}");
}

/// The darkest pixel of a frame's column `x` over rows `y0..y1`, RGBA rows of `width` pixels.
fn darkest(pixels: &[u8], width: usize, x: usize, y0: usize, y1: usize) -> u8 {
    (y0..y1).map(|y| pixels[(y * width + x) * 4]).min().unwrap_or(255)
}

#[test]
fn the_caret_is_drawn_and_blinks_while_focused() {
    use crate::api::session::CaretBlink;
    // System fonts: with none, an empty document has no line for a caret.
    let mut engine = Engine::new(FontLibrary::system());
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("a session");
    s.host().set_size(400.0, 300.0, 1.0);
    let mut pixels = vec![0u8; 400 * 300 * 4];
    let caret_at = |s: &mut crate::api::session::Session, pixels: &mut Vec<u8>| {
        let info = s.host().frame_rgba(pixels, 400, 300).expect("a frame");
        let (x, y) = (info.caret_x.floor() as usize, info.caret_y as usize);
        // The bar covers the pixel its left edge falls in, or the next one.
        let ink = (x.saturating_sub(1)..=x + 1).map(|x| darkest(pixels, 400, x, y + 2, y + info.caret_height as usize - 2)).min();
        (info.caret_visible, ink.unwrap_or(255))
    };
    // Unfocused: no caret, no blink.
    let (shown, ink) = caret_at(s, &mut pixels);
    assert!(!shown && ink > 200, "unfocused: no caret ({ink})");
    assert!(s.host().take_timers().is_empty());
    s.host().set_focus(true);
    assert!(s.host().take_frame_request(), "focus asks for a frame");
    assert_eq!(s.host().take_timers(), [(2, 530)], "the blink's first phase");
    let (shown, ink) = caret_at(s, &mut pixels);
    assert!(shown && ink < 60, "focused: the caret is drawn ({ink})");
    // A faster blink, to see a phase end.
    s.host().set_caret_blink(CaretBlink { interval_ms: Some(100), stop_after_ms: None });
    std::thread::sleep(std::time::Duration::from_millis(110));
    s.host().take_frame_request();
    s.host().timer_fired(2); // the 530 ms arm fires late: the 100 ms phase has ended
    assert!(s.host().take_frame_request(), "a phase change asks for a frame");
    assert_eq!(s.host().take_timers(), [(2, 100)], "the next phase");
    let (shown, ink) = caret_at(s, &mut pixels);
    assert!(!shown && ink > 200, "the hidden phase draws no caret ({ink})");
    // Typing shows it at once.
    s.run(Command::InsertText("Hi".into())).expect("types");
    let (shown, ink) = caret_at(s, &mut pixels);
    assert!(shown && ink < 60, "input shows the caret ({ink})");
    // A range: no caret.
    s.run(Command::SelectAll).expect("selects");
    assert!(!s.host().frame_info().caret_visible, "a range draws no caret");
    // Steady: no blink timers, and the caret stays.
    s.run(Command::MoveSelection(crate::api::command::MoveSelection {
        by: crate::api::command::MoveUnit::Document,
        direction: crate::api::value::Direction::Forward,
        extend: None,
    }))
    .expect("moves");
    s.host().take_timers();
    s.host().set_caret_blink(CaretBlink { interval_ms: None, stop_after_ms: None });
    assert!(s.host().take_timers().is_empty(), "steady: no timer");
    s.host().timer_fired(2);
    assert!(s.host().frame_info().caret_visible, "steady: always drawn");
    // Losing focus hides it, and a late timer arms nothing.
    s.host().set_caret_blink(CaretBlink::default());
    s.host().set_focus(false);
    s.host().take_timers();
    s.host().timer_fired(2);
    assert!(s.host().take_timers().is_empty(), "unfocused: the blink stops");
    assert!(!s.host().frame_info().caret_visible);
}

#[test]
fn formatting_marks_and_text_boundaries_draw_when_shown() {
    use crate::api::command::ViewOptions;
    use crate::editing::view::MarkKind;
    let mut engine = Engine::new(FontLibrary::system());
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("a session");
    s.run(Command::InsertText("a b\tc".into())).expect("types");
    s.host().set_size(500.0, 300.0, 1.0);
    let mut plain = vec![0u8; 500 * 300 * 4];
    s.host().frame_rgba(&mut plain, 500, 300).expect("a frame");
    s.run(Command::SetViewOptions(ViewOptions { formatting_marks: Some(true), text_boundaries: Some(true), ..ViewOptions::default() }))
        .expect("shows the marks");
    assert!(s.state().view.formatting_marks && s.state().view.text_boundaries);
    let kinds: Vec<MarkKind> = s.editor.formatting_marks(0..1).iter().map(|m| m.kind).collect();
    for k in [MarkKind::Space, MarkKind::Tab, MarkKind::Paragraph] {
        assert!(kinds.contains(&k), "{k:?} in {kinds:?}");
    }
    assert!(!s.editor.text_boundaries(0..1).is_empty(), "the body's text area");
    let mut marked = vec![0u8; 500 * 300 * 4];
    s.host().frame_rgba(&mut marked, 500, 300).expect("a frame");
    assert_ne!(plain, marked, "the marks and boundaries are drawn");
    s.run(Command::SetViewOptions(ViewOptions { formatting_marks: Some(false), text_boundaries: Some(false), ..ViewOptions::default() }))
        .expect("hides");
    s.host().frame_rgba(&mut marked, 500, 300).expect("a frame");
    assert_eq!(plain, marked, "off again: the frame as before");
}

#[test]
fn frames_from_retained_pages_equal_frames_drawn_from_scratch() {
    use crate::api::command::Zoom;
    let mut engine = Engine::new(FontLibrary::system());
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("a session");
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/objects.docx");
    s.open(&std::fs::read(path).expect("the fixture")).expect("opens");
    s.host().set_verify_frames(true);
    s.host().set_focus(true);
    let mut frames = 0;
    for (dpr, zoom, scroll) in [(1.0, 100.0, 0.0), (1.5, 100.0, 333.5), (2.0, 125.0, 120.25), (1.0, 75.0, 900.0), (2.0, 100.0, 1500.0)] {
        let (w, h) = (700u32, 500u32);
        s.host().set_size(f64::from(w) / dpr, f64::from(h) / dpr, dpr);
        s.run(Command::SetZoom(Zoom::Percent(zoom))).expect("zooms");
        s.host().set_scroll(0.0, scroll);
        let mut pixels = vec![0u8; (w * h * 4) as usize];
        for step in 0..3 {
            if step == 1 {
                s.run(Command::SelectAll).expect("selects");
            }
            if step == 2 {
                s.run(Command::InsertText("edited ".into())).expect("types");
            }
            s.host().frame_rgba(&mut pixels, w, h).expect("a frame");
            frames += 1;
        }
    }
    assert_eq!(s.host().verify_failures(), 0, "{frames} frames verified");
    let stats = s.host().frame_stats();
    assert!(stats.pages_reused > 0 && stats.pages_recorded > 0, "{stats:?}");
}

#[test]
fn zoom_keeps_the_caret_line_or_the_viewport_centre_in_place() {
    use crate::api::command::Zoom;
    use crate::api::session::HostRequest;
    let mut engine = Engine::new(FontLibrary::system());
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("a session");
    for _ in 0..60 {
        s.run(Command::InsertText("Line".into())).expect("types");
        s.run(Command::InsertParagraph).expect("splits");
    }
    let scrolls = Rc::new(RefCell::new(Vec::new()));
    let sink = scrolls.clone();
    s.host().attach(move |r| {
        if let HostRequest::Scroll { x, y } = r {
            sink.borrow_mut().push((x, y));
        }
    });
    s.host().set_size(800.0, 600.0, 1.0);
    while s.host().idle() {}
    // The host carries out the scroll request, as the SDKs do.
    let zoom = |s: &mut crate::api::session::Session, percent: f64| {
        s.run(Command::SetZoom(Zoom::Percent(percent))).expect("zooms");
        let (x, y) = scrolls.borrow_mut().pop().expect("a scroll request");
        s.host().set_scroll(x, y);
        y
    };
    // The caret in view, above the centre: its line's screen y stays.
    let caret = s.host().frame_info().caret_y;
    s.host().set_scroll(0.0, caret - 150.0);
    for percent in [200.0, 50.0, 100.0] {
        let y = zoom(s, percent);
        let on_screen = s.host().frame_info().caret_y - y;
        assert!((on_screen - 150.0).abs() <= 1.0, "at {percent}%: the caret line moved to {on_screen}");
    }
    // The caret out of view: the content point at the viewport's centre stays.
    s.host().set_scroll(0.0, 400.0);
    let mut centre = (400.0 + 300.0) / (96.0 / 72.0);
    for percent in [200.0, 50.0] {
        let y = zoom(s, percent);
        let k = percent / 100.0 * 96.0 / 72.0;
        assert!((y + 300.0 - centre * k).abs() <= 1.0, "at {percent}%: the centre moved to {}", (y + 300.0) / k);
        centre = (y + 300.0) / k;
    }
}

#[test]
fn retained_pages_record_once_and_again_only_where_layout_changed() {
    use crate::api::command::Zoom;
    use crate::api::session::PaintBudget;
    let mut engine = Engine::new(FontLibrary::system());
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("a session");
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/big.docx");
    s.open(&std::fs::read(path).expect("the fixture")).expect("opens");
    while s.host().idle() {}
    // Tall enough for two pages at 50%.
    let (w, h) = (900u32, 1400u32);
    s.host().set_size(f64::from(w), f64::from(h), 1.0);
    s.run(Command::SetZoom(Zoom::Percent(50.0))).expect("zooms");
    let mut pixels = vec![0u8; (w * h * 4) as usize];
    let mut frame = |s: &mut crate::api::session::Session| {
        s.host().frame_rgba(&mut pixels, w, h).expect("a frame");
        s.host().frame_stats()
    };
    let first = frame(s);
    assert!(first.pages_recorded >= 2, "two pages on screen: {first:?}");
    let again = frame(s);
    assert_eq!(again.pages_recorded, first.pages_recorded, "nothing changed: nothing recorded");
    assert!(again.pages_reused > first.pages_reused, "the pages replayed");
    // Typing on the first page records that page again, not the second.
    s.run(Command::InsertText("x".into())).expect("types");
    // Layout makes the pages after the edit anew; those that draw the same lines keep their recordings.
    while s.host().idle() {}
    let typed = frame(s);
    assert_eq!(typed.pages_recorded, again.pages_recorded + 1, "{again:?} then {typed:?}");
    // A zoom replays the same recordings at the new scale.
    s.run(Command::SetZoom(Zoom::Percent(60.0))).expect("zooms");
    let zoomed = frame(s);
    assert_eq!(zoomed.pages_recorded, typed.pages_recorded, "a zoom records nothing");
    assert!(zoomed.pages_reused > typed.pages_reused, "it replays them");
    // A small budget: the recordings stay within it after the frame.
    s.host().set_paint_budget(PaintBudget { recordings: 8 << 10 });
    s.run(Command::SetZoom(Zoom::Percent(100.0))).expect("zooms");
    let small = frame(s);
    assert!(small.recording_bytes <= 8 << 10, "trimmed to the budget: {small:?}");
}

#[test]
fn leaders_draw_over_the_frame_until_cleared() {
    use crate::api::input::Point;
    let (mut engine, _) = engine();
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("a session");
    s.host().set_size(400.0, 300.0, 1.0);
    let mut plain = vec![0u8; 400 * 300 * 4];
    s.host().frame_rgba(&mut plain, 400, 300).expect("a frame");
    s.host().set_leaders(vec![vec![Point { x: 10.0, y: 150.5 }, Point { x: 390.0, y: 150.5 }]]);
    assert!(s.host().take_frame_request(), "new leaders ask for a frame");
    let mut lined = vec![0u8; 400 * 300 * 4];
    s.host().frame_rgba(&mut lined, 400, 300).expect("a frame");
    let px = |b: &[u8], x: usize, y: usize| [b[(y * 400 + x) * 4], b[(y * 400 + x) * 4 + 1], b[(y * 400 + x) * 4 + 2]];
    assert_eq!(px(&lined, 200, 150), [0x76, 0x76, 0x76], "the palette's leader colour");
    s.host().set_leaders(Vec::new());
    s.host().frame_rgba(&mut lined, 400, 300).expect("a frame");
    assert_eq!(plain, lined, "cleared: the frame as before");
}
