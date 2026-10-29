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
