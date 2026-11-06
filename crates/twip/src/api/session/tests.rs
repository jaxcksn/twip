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

// The PDF's dates and ids come from the export's time: without them, two exports compare.
use crate::pdf::tests::undated;

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
    let export = s.export(&ExportOptions::Docx).expect("exports");
    assert_eq!(export.revision, s.state().revision);
    assert!(!s.state().modified);
    let warnings = s.open(&export.bytes.0).expect("reads back");
    // The library is empty, so only the font warning.
    assert!(warnings.iter().all(|w| w.code == crate::api::warning::WarningCode::FontUnresolved), "{warnings:?}");
}

#[test]
fn a_typical_state_is_under_10_kb() {
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
    assert!(json.len() < 10 * 1024, "{} bytes", json.len());
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
    s.export(&ExportOptions::Docx).expect("exports");
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
    let pdf = s.export(&ExportOptions::Pdf(Default::default())).expect("exports");
    assert!(pdf.bytes.0.starts_with(b"%PDF"));
    assert_eq!(pdf.revision, s.state().revision);
    assert!(s.state().modified, "a PDF doesn't save the document");
    assert_eq!(pdf.warnings.iter().map(|w| w.code).collect::<Vec<_>>(), [crate::api::warning::WarningCode::PdfFontFallback]);
}

#[test]
fn dark_mode_changes_neither_the_pdf_nor_the_layout() {
    let mut engine = Engine::new(FontLibrary::system());
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("a session");
    s.run(Command::InsertText("Dark and light".into())).expect("types");
    s.run(Command::SelectAll).expect("selects");
    s.run(Command::SetFontColor(crate::api::value::AutoOr::Value(crate::api::value::Color(0x1F3864)))).expect("colours");
    let export = |s: &mut crate::api::session::Session| {
        let mut b = s.export(&ExportOptions::Pdf(Default::default())).expect("exports").bytes.0;
        // Two exports differ in their creation time and their XMP packet's and trailer's random ids: cut those out.
        let find = |b: &[u8], n: &[u8]| b.windows(n.len()).position(|w| w == n);
        for (from, to) in [(&b"<?xpacket begin"[..], &b"<?xpacket end"[..]), (b"/ID [", b"]")] {
            if let Some(i) = find(&b, from)
                && let Some(j) = find(&b[i..], to)
            {
                b.drain(i..i + j);
            }
        }
        b.into_iter().filter(|c| !c.is_ascii_digit()).collect::<Vec<u8>>()
    };
    let light = export(s);
    assert_eq!(export(s), light, "two exports compare equal");
    let pages = s.state().pages.clone();
    let dark = crate::api::command::ViewOptions { appearance: Some(crate::api::services::DocumentAppearance::Dark), ..Default::default() };
    s.run(Command::SetViewOptions(dark)).expect("dark");
    assert_eq!(s.state().view.appearance, crate::api::services::DocumentAppearance::Dark);
    assert_eq!(export(s), light, "the PDF keeps the document's colours");
    assert_eq!(s.state().pages, pages, "nothing moves");
}

/// The body's paragraphs as text.
fn paragraphs(s: &crate::api::session::Session) -> Vec<String> {
    s.editor
        .document()
        .sections
        .iter()
        .flat_map(|sec| &sec.blocks)
        .filter_map(|b| if let crate::docx::Block::Para(p) = b { Some(crate::editing::text::UnitText::of(p).text) } else { None })
        .collect()
}

fn typed(s: &mut crate::api::session::Session, text: &str) {
    for c in text.chars() {
        s.text(&c.to_string());
    }
}

#[test]
fn typing_corrects_what_it_finishes_and_one_undo_takes_the_correction_back() {
    let mut engine = Engine::new(FontLibrary::system());
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("a session");
    typed(s, "Word teh ");
    assert_eq!(paragraphs(s)[0], "Word the ");
    // Right after it, an undo takes back only the correction.
    s.run(Command::Undo).expect("undoes");
    assert_eq!(paragraphs(s)[0], "Word teh ");
    typed(s, "(c)");
    assert_eq!(paragraphs(s)[0], "Word teh \u{a9}");
    s.run(Command::Undo).expect("undoes");
    assert_eq!(paragraphs(s)[0], "Word teh (c)");
    // An ordinal's suffix is raised.
    s.run(Command::SelectAll).expect("selects");
    typed(s, "Word 1st ");
    let p = s.editor.document().sections[0]
        .blocks
        .iter()
        .find_map(|b| if let crate::docx::Block::Para(p) = b { Some(p.clone()) } else { None })
        .expect("a paragraph");
    let raised: String = p
        .inlines
        .iter()
        .filter_map(|i| {
            if let crate::docx::Inline::Text(t, r) = i {
                (r.vert_align == crate::docx::props::VertAlign::Superscript).then(|| t.clone())
            } else {
                None
            }
        })
        .collect();
    assert_eq!(raised, "st");
    // Text the API inserts isn't corrected.
    s.run(Command::SelectAll).expect("selects");
    s.run(Command::InsertText("Word teh ".into())).expect("inserts");
    assert_eq!(paragraphs(s)[0], "Word teh ");
}

#[test]
fn enter_corrects_the_paragraph_it_ends_and_tracking_marks_corrections() {
    use crate::api::input::{Key, Modifiers};
    let mut engine = Engine::new(FontLibrary::system());
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("a session");
    typed(s, "Word word--");
    s.key(Key::Enter, Modifiers::default());
    assert_eq!(paragraphs(s)[0], "Word word\u{2014}");
    s.run(Command::SetTrackChanges(true)).expect("tracks");
    typed(s, "Word teh ");
    // The correction replaces the author's own insertion: the corrected word is an insertion too.
    let inserted = s.editor.document().sections[0].blocks.iter().any(|b| {
        matches!(b, crate::docx::Block::Para(p) if p.inlines.iter().any(|i| matches!(i, crate::docx::Inline::Text(t, r) if t.contains("the") && r.rev.ins.is_some())))
    });
    assert!(inserted, "the correction is tracked");
}

#[test]
fn the_correction_mark_stays_while_the_caret_moves_onto_its_word() {
    use crate::api::input::{Key, Modifiers};
    let mut engine = Engine::new(FontLibrary::system());
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("a session");
    typed(s, "Word teh ");
    // As Word: Ctrl+Left onto the corrected word, then Alt+Shift+F10 offers its options.
    s.key(Key::ArrowLeft, Modifiers { ctrl: true, ..Modifiers::default() });
    assert!(s.editor.correction().is_some(), "on the word");
    s.key(Key::ArrowLeft, Modifiers::default());
    assert!(s.editor.correction().is_none(), "before it, it goes");
}

#[test]
fn a_correction_offers_its_options_and_never_correct_stops_it() {
    use crate::api::command::CommandId;
    use crate::api::event::Event;
    use crate::api::services::Popover;
    let mut engine = Engine::new(FontLibrary::system());
    let events: std::rc::Rc<std::cell::RefCell<Vec<Event>>> = Default::default();
    let sink = events.clone();
    engine.set_listener(move |_, e| sink.borrow_mut().push(e.clone()));
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("a session");
    s.host().set_size(800.0, 600.0, 1.0);
    assert!(!s.state().commands.get(&CommandId::RevertAutoCorrect).is_some_and(|c| c.enabled), "nothing corrected yet");
    typed(s, "Word teh ");
    assert!(s.state().commands.get(&CommandId::RevertAutoCorrect).is_some_and(|c| c.enabled));
    // The mark and its button show while the caret is just after it.
    let view = s.editor.correction_view(0..1).expect("a mark");
    assert!(!view.rects.is_empty() && view.button.is_some());
    events.borrow_mut().clear();
    s.run(Command::ShowAutoCorrectOptions).expect("shows");
    let asked = events.borrow().iter().find_map(|e| if let Event::PopoverRequested(p) = e { Some(p.clone()) } else { None });
    assert!(matches!(asked, Some(Popover::AutoCorrect { ref from, ref to, .. }) if from == "teh" && to == "the"), "{asked:?}");
    // Never correct puts it back, says so, and keeps it from happening again.
    s.run(Command::NeverAutoCorrect).expect("never");
    assert_eq!(paragraphs(s)[0], "Word teh ");
    assert!(events.borrow().iter().any(|e| matches!(e, Event::Announcement { text, .. } if text.contains("teh"))));
    assert_eq!(s.state().auto_correct.never, ["teh"]);
    typed(s, "teh ");
    assert_eq!(paragraphs(s)[0], "Word teh teh ");
    // Typing on past the mark drops it.
    typed(s, "(c) ");
    typed(s, "and more words here ");
    assert!(s.editor.correction().is_none(), "typed past");
    // setAutoCorrect switches a rule off.
    s.run(Command::SetAutoCorrect(crate::api::services::AutoCorrectChange { replace_text: Some(false), ..Default::default() })).expect("sets");
    assert!(!s.state().auto_correct.replace_text);
    typed(s, "adn ");
    assert!(paragraphs(s)[0].ends_with("adn "));
}

#[test]
fn a_typed_list_start_begins_a_list_and_one_undo_puts_it_back() {
    use crate::api::input::{Key, Modifiers};
    use crate::api::state::{ListState, Tri};
    let mut engine = Engine::new(FontLibrary::system());
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("a session");
    typed(s, "1. ");
    assert_eq!(paragraphs(s)[0], "");
    assert_eq!(s.state().paragraph.list, Tri::Uniform(ListState::Numbered));
    typed(s, "First");
    s.key(Key::Enter, Modifiers::default());
    typed(s, "Second");
    assert_eq!(s.state().paragraph.list, Tri::Uniform(ListState::Numbered), "Enter continues the list");
    // One undo takes back the list, with the marker as typed.
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("a session");
    typed(s, "* ");
    assert_eq!(s.state().paragraph.list, Tri::Uniform(ListState::Bullet));
    s.run(Command::Undo).expect("undoes");
    assert_eq!(paragraphs(s)[0], "* ");
    assert_eq!(s.state().paragraph.list, Tri::Uniform(ListState::None));
    // Not inside a paragraph's text, nor when the rule is off.
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("a session");
    typed(s, "Step 1. ");
    assert_eq!(s.state().paragraph.list, Tri::Uniform(ListState::None));
    s.key(Key::Enter, Modifiers::default());
    s.run(Command::SetAutoCorrect(crate::api::services::AutoCorrectChange { numbered_lists: Some(false), ..Default::default() })).expect("sets");
    typed(s, "1. ");
    assert_eq!(paragraphs(s)[1], "1. ");
    assert_eq!(s.state().paragraph.list, Tri::Uniform(ListState::None));
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
fn dark_frames_from_retained_pages_equal_frames_drawn_from_scratch_and_switching_records_again() {
    use crate::api::command::ViewOptions;
    use crate::api::services::{ChromeTheme, DocumentAppearance};
    let mut engine = Engine::new(FontLibrary::system());
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("a session");
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/objects.docx");
    s.open(&std::fs::read(path).expect("the fixture")).expect("opens");
    s.host().set_verify_frames(true);
    let (w, h) = (700u32, 500u32);
    s.host().set_size(f64::from(w), f64::from(h), 1.0);
    let mut light = vec![0u8; (w * h * 4) as usize];
    s.host().frame_rgba(&mut light, w, h).expect("a frame");
    for appearance in [DocumentAppearance::Dark, DocumentAppearance::DarkWithLightPage, DocumentAppearance::Dark] {
        let options = ViewOptions { appearance: Some(appearance), theme: Some(ChromeTheme::Dark), ..ViewOptions::default() };
        s.run(Command::SetViewOptions(options)).expect("dark");
        let mut pixels = vec![0u8; (w * h * 4) as usize];
        s.host().frame_rgba(&mut pixels, w, h).expect("a frame");
        assert_ne!(pixels, light, "{appearance:?} draws differently");
    }
    assert_eq!(s.host().verify_failures(), 0, "retained dark pages draw as pages drawn from scratch");
}

#[test]
fn a_new_accent_draws_without_recording_the_pages_again() {
    // The accent colours only what's drawn over the pages.
    use crate::api::command::ViewOptions;
    let mut engine = Engine::new(FontLibrary::system());
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("a session");
    s.run(Command::InsertText("Accent".into())).expect("types");
    s.run(Command::SelectAll).expect("selects");
    let (w, h) = (400u32, 300u32);
    s.host().set_size(f64::from(w), f64::from(h), 1.0);
    let mut before = vec![0u8; (w * h * 4) as usize];
    s.host().frame_rgba(&mut before, w, h).expect("a frame");
    let recorded = s.host().frame_stats().pages_recorded;
    s.run(Command::SetViewOptions(ViewOptions { accent: Some(crate::api::value::Color(0xC0_2060)), ..ViewOptions::default() })).expect("accent");
    let mut after = vec![0u8; (w * h * 4) as usize];
    s.host().frame_rgba(&mut after, w, h).expect("a frame");
    assert_ne!(before, after, "the selection takes the new accent");
    assert_eq!(s.host().frame_stats().pages_recorded, recorded, "no page drawn again");
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
    assert_eq!(px(&lined, 200, 150), [0x62, 0x65, 0x6c], "the palette's leader colour");
    s.host().set_leaders(Vec::new());
    s.host().frame_rgba(&mut lined, 400, 300).expect("a frame");
    assert_eq!(plain, lined, "cleared: the frame as before");
}

#[test]
fn the_default_selection_keeps_black_text_readable() {
    // WCAG relative luminance: black text over the default selection (multiplied over white: the colour itself).
    let luminance = |c: u32| {
        let lin = |v: u32| {
            let s = v as f64 / 255.0;
            if s <= 0.03928 { s / 12.92 } else { ((s + 0.055) / 1.055).powf(2.4) }
        };
        0.2126 * lin(c >> 16 & 0xFF) + 0.7152 * lin(c >> 8 & 0xFF) + 0.0722 * lin(c & 0xFF)
    };
    let selection = super::host::OverlayPalette::default().selection.0;
    let contrast = (luminance(selection) + 0.05) / 0.05;
    assert!(contrast >= 4.5, "black on the selection: {contrast:.1}:1");
    assert_eq!(crate::render::Palette::default().selection, crate::render::rgb(selection), "the renderer's default is the API's");
}

#[test]
fn an_inserted_shape_is_the_selected_object() {
    let (mut engine, _) = engine();
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("a session");
    s.run(Command::InsertText("Shapes".into())).expect("types");
    let preset = crate::api::drawing::ShapePreset::from_name("star5").expect("a preset");
    s.run(Command::InsertShape(crate::api::drawing::ShapeInsert { preset, width: None, height: None })).expect("inserts");
    let o = s.state().selection.object.clone();
    assert!(o.is_some(), "{:?}", s.state().selection);
    assert_eq!(o.and_then(|o| o.shape).and_then(|s| s.preset), Some(preset));
}

#[test]
fn the_detail_and_object_grid_options_reach_the_editors_drag_feedback() {
    use crate::api::command::ViewOptions;
    use crate::api::services::{MeasureDetail, ObjectGrid};
    let mut engine = Engine::new(FontLibrary::empty());
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("a session");
    s.run(Command::SetViewOptions(ViewOptions {
        detail: Some(MeasureDetail::Full),
        object_grid: Some(ObjectGrid::Always),
        ..ViewOptions::default()
    }))
    .expect("sets the options");
    s.host().set_size(200.0, 200.0, 1.0);
    let mut px = vec![0u8; 200 * 200 * 4];
    s.host().frame_rgba(&mut px, 200, 200).expect("a frame");
    let v = s.editor.view_settings;
    assert_eq!((v.detail, v.object_grid), (MeasureDetail::Full, ObjectGrid::Always));
}

#[test]
fn application_field_types_keep_their_results_and_show_their_labels() {
    use crate::api::command::{ApplicationFieldTypes, ElementIds, FieldLabels};
    // Fields of the application's types: two with their results cleared, one with a result.
    let field = |code: &str, result: &str| {
        format!(
            "<w:r><w:fldChar w:fldCharType=\"begin\"/></w:r><w:r><w:instrText xml:space=\"preserve\"> {code} </w:instrText></w:r>\
             <w:r><w:fldChar w:fldCharType=\"separate\"/></w:r><w:r><w:t>{result}</w:t></w:r><w:r><w:fldChar w:fldCharType=\"end\"/></w:r>"
        )
    };
    let body = format!(
        "<w:p><w:r><w:t xml:space=\"preserve\">Invoice </w:t></w:r>{}</w:p><w:p>{}</w:p><w:p>{}</w:p>",
        field("CUSTOMVALUE InvoiceNumber", ""),
        field("CUSTOMTEXT Customer", ""),
        field("CUSTOMCOUNT Lines", "3")
    );
    // System fonts, so design mode's labels draw.
    let (mut engine, events) = (Engine::new(FontLibrary::system()), Rc::new(RefCell::new(Vec::new())));
    let sink = events.clone();
    engine.set_listener(move |_, e| sink.borrow_mut().push(e.clone()));
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("a session");
    s.open(&crate::editing::tests::blank_body(&body)).expect("opens");
    // The paragraphs as they show, fields as their values.
    let shown = |s: &crate::api::session::Session| -> Vec<String> {
        let blocks = s.editor.document().sections.iter().flat_map(|sec| &sec.blocks);
        blocks.filter_map(|b| if let crate::docx::Block::Para(p) = b { Some(crate::docx::fields::text(p)) } else { None }).collect()
    };
    let errors = |s: &crate::api::session::Session| shown(s).iter().filter(|p| p.contains("Error! Bookmark not defined.")).count();
    assert_eq!(errors(s), 2, "with no list, unknown types are bookmark references");
    assert!(!s.state().modified);

    let types = |t: &[&str]| Command::SetApplicationFieldTypes(ApplicationFieldTypes { types: t.iter().map(|x| x.to_string()).collect() });
    s.run(types(&["customvalue", "CustomText", "CUSTOMCOUNT"])).expect("sets the types");
    assert_eq!(errors(s), 0);
    assert_eq!(shown(s), ["Invoice ", "", "3"], "results stay as they are");
    assert!(!s.state().modified, "setting the list isn't an edit");

    s.set_mode(crate::api::state::Mode::Design);
    let labels = FieldLabels { labels: [("CUSTOMTEXT Customer".into(), "Customer name".into())].into(), merge: None };
    s.run(Command::SetFieldLabels(labels)).expect("sets the labels");
    s.editor.complete(&s.fonts.fonts());
    let drawn = crate::layout::dump::page_text(&s.editor.snapshot(), s.editor.document());
    assert!(drawn.contains("\u{ab}InvoiceNumber\u{bb}") && drawn.contains("Customer name") && !drawn.contains("Error!"), "{drawn}");
    assert!(!s.state().modified);
    // One inserted later is of the type too.
    let owner = crate::api::command::InsertField { code: "CUSTOMVALUE Owner".into(), display: None, show_code: None };
    s.run(Command::InsertField(owner)).expect("inserts");
    s.editor.complete(&s.fonts.fonts());
    assert!(crate::layout::dump::page_text(&s.editor.snapshot(), s.editor.document()).contains("\u{ab}Owner\u{bb}"));

    s.run(Command::SelectAll).expect("selects all");
    events.borrow_mut().clear();
    s.run(Command::UpdateFields(ElementIds { ids: None })).expect("updates");
    let requested: Vec<String> = events
        .borrow()
        .iter()
        .filter_map(|e| if let Event::FieldsUpdateRequested { fields, .. } = e { Some(fields.clone()) } else { None })
        .flatten()
        .map(|f| f.code.trim().to_string())
        .collect();
    assert_eq!(requested, ["CUSTOMVALUE Owner", "CUSTOMVALUE InvoiceNumber", "CUSTOMTEXT Customer", "CUSTOMCOUNT Lines"]);
    assert_eq!(errors(s), 0, "updating doesn't read them as bookmarks either");

    s.set_mode(crate::api::state::Mode::Editing);
    s.run(types(&[])).expect("clears the types");
    assert_eq!(errors(s), 2, "an empty list puts the bookmark reading back");
}

#[test]
fn sub_template_bodies_shown_inline_never_reach_the_output() {
    use crate::api::command::{ReferenceRule, SubTemplateRules, SubTemplateView};
    let mut engine = Engine::new(FontLibrary::system());
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("a session");
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/FX-SUBTEMPLATES.docx");
    s.open(&std::fs::read(path).expect("fixture")).expect("opens");
    let rule = ReferenceRule { field_type: "SUBTEMPLATE".into(), argument: None, category: "Sub-templates".into(), gallery: None };
    s.run(Command::SetSubTemplateRules(SubTemplateRules { rules: vec![rule] })).expect("sets the rules");
    let export = |s: &mut crate::api::session::Session, f| undated(s.export(&ExportOptions::from(f)).expect("exports").bytes.0);
    let (docx, pdf) = (export(s, ExportFormat::Docx), export(s, ExportFormat::Pdf));
    assert!(s.editor.shows_expansions(), "the bodies show");
    s.run(Command::SetSubTemplateView(SubTemplateView { ids: None, expanded: false })).expect("collapses");
    assert!(!s.editor.shows_expansions());
    assert_eq!(export(s, ExportFormat::Docx), docx);
    assert_eq!(export(s, ExportFormat::Pdf), pdf);
}

/// A sub-template being edited doesn't change what an export writes: the whole document, template and every
/// sub-template, as after closing it.
#[test]
fn exports_while_a_sub_template_is_edited_write_the_whole_document() {
    use crate::api::command::{ReferenceRule, SubTemplateName, SubTemplateRules};
    let mut engine = Engine::new(FontLibrary::system());
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("a session");
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/FX-SUBTEMPLATES.docx");
    s.open(&std::fs::read(path).expect("fixture")).expect("opens");
    let rule = ReferenceRule { field_type: "SUBTEMPLATE".into(), argument: None, category: "Sub-templates".into(), gallery: None };
    s.run(Command::SetSubTemplateRules(SubTemplateRules { rules: vec![rule] })).expect("sets the rules");
    s.run(Command::EditSubTemplate(SubTemplateName { name: "Violation".into(), category: None })).expect("edits it");
    s.run(Command::InsertText("x".into())).expect("types");
    let export = |s: &mut crate::api::session::Session, f| undated(s.export(&ExportOptions::from(f)).expect("exports").bytes.0);
    let (docx, pdf) = (export(s, ExportFormat::Docx), export(s, ExportFormat::Pdf));
    assert!(s.state().editing != crate::api::state::EditingTarget::Template, "still on the sub-template");
    s.run(Command::CloseSubTemplate).expect("closes");
    assert_eq!(export(s, ExportFormat::Docx), docx);
    assert_eq!(export(s, ExportFormat::Pdf), pdf);
}

#[test]
fn the_editing_target_is_reported_once_per_change() {
    use crate::api::command::{ReferenceRule, SubTemplateName, SubTemplateRules};
    use crate::api::state::EditingTarget;
    let (mut engine, events) = engine();
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("a session");
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/FX-SUBTEMPLATES.docx");
    s.open(&std::fs::read(path).expect("fixture")).expect("opens");
    let rule = ReferenceRule { field_type: "SUBTEMPLATE".into(), argument: None, category: "Sub-templates".into(), gallery: None };
    s.run(Command::SetSubTemplateRules(SubTemplateRules { rules: vec![rule] })).expect("sets the rules");
    let targets = |events: &Rc<RefCell<Vec<Event>>>| -> Vec<EditingTarget> {
        events.borrow_mut().drain(..).filter_map(|e| if let Event::EditingTargetChanged(t) = e { Some(t) } else { None }).collect()
    };
    targets(&events);
    let violation = EditingTarget::SubTemplate { name: "Violation".into(), category: "Sub-templates".into() };
    s.run(Command::EditSubTemplate(SubTemplateName { name: "Violation".into(), category: None })).expect("edits it");
    assert_eq!(targets(&events), std::slice::from_ref(&violation));
    assert_eq!(s.state().editing, violation);
    s.run(Command::InsertText("x".into())).expect("types");
    assert_eq!(targets(&events), [], "edits within it don't switch");
    s.run(Command::CloseSubTemplate).expect("closes");
    assert_eq!(targets(&events), [EditingTarget::Template]);
    s.run(Command::Undo).expect("undoes");
    assert_eq!(targets(&events), std::slice::from_ref(&violation), "undo goes back to the sub-template");
    s.run(Command::Redo).expect("redoes");
    assert_eq!(targets(&events), [], "redo stays on it");
    assert_eq!(
        s.run(Command::EditSubTemplate(SubTemplateName { name: "Nope".into(), category: None })).map_err(|e| e.code).err(),
        Some(ErrorCode::InvalidArgument)
    );
    assert_eq!(s.state().editing, violation);
}

#[test]
fn sub_templates_list_with_their_references_and_elements() {
    use crate::api::command::{ReferenceRule, SubTemplateRules};
    use crate::api::services::ElementFilter;
    let (mut engine, _) = engine();
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("a session");
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/FX-SUBTEMPLATES.docx");
    s.open(&std::fs::read(path).expect("fixture")).expect("opens");
    assert!(s.sub_templates().is_empty(), "none without rules");
    let rule = ReferenceRule { field_type: "SUBTEMPLATE".into(), argument: None, category: "Sub-templates".into(), gallery: None };
    s.run(Command::SetSubTemplateRules(SubTemplateRules { rules: vec![rule] })).expect("sets the rules");
    let list = s.sub_templates();
    let names: Vec<&str> = list.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, ["Violation", "FeeRow", "Appeals notice", "Cycle-A", "Cycle-B"], "not the AutoText block");
    let violation = &list[0];
    assert_eq!((violation.references, violation.nested_references), (4, 0), "two body paragraphs, the shared one and the footer");
    assert_eq!(violation.elements.len(), 2, "its field and control");
    assert_eq!((list[2].references, list[2].nested_references), (0, 0));
    assert_eq!((list[4].references, list[4].nested_references), (0, 1), "Cycle-B is named inside Cycle-A");
    let only = s.elements(&ElementFilter { sub_template: Some("Violation".into()), ..ElementFilter::default() });
    assert_eq!(only.len(), 2);
    assert!(only.iter().all(|e| e.sub_template.as_deref() == Some("Violation")));
}

#[test]
fn switching_to_a_sub_template_is_announced() {
    use crate::api::command::{ReferenceRule, SubTemplateName, SubTemplateRules};
    let (mut engine, events) = engine();
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("a session");
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/FX-SUBTEMPLATES.docx");
    s.open(&std::fs::read(path).expect("fixture")).expect("opens");
    let rule = ReferenceRule { field_type: "SUBTEMPLATE".into(), argument: None, category: "Sub-templates".into(), gallery: None };
    s.run(Command::SetSubTemplateRules(SubTemplateRules { rules: vec![rule] })).expect("sets the rules");
    events.borrow_mut().clear();
    s.run(Command::EditSubTemplate(SubTemplateName { name: "Violation".into(), category: None })).expect("edits it");
    s.run(Command::CloseSubTemplate).expect("closes");
    let said: Vec<String> =
        events.borrow().iter().filter_map(|e| if let Event::Announcement { text, .. } = e { Some(text.clone()) } else { None }).collect();
    assert!(said.contains(&"Editing sub-template Violation".to_string()) && said.contains(&"Back to the template".to_string()), "{said:?}");
}

#[test]
fn commands_say_what_they_did_and_kinds_can_be_turned_off() {
    use crate::api::services::AnnouncementKind as K;
    let (mut engine, events) = engine();
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("a session");
    s.run(Command::InsertText("Hello".into())).expect("types");
    s.run(Command::SelectAll).expect("selects");
    let said = |events: &Rc<RefCell<Vec<Event>>>| -> Vec<(K, String)> {
        events.borrow_mut().drain(..).filter_map(|e| if let Event::Announcement { text, kind } = e { Some((kind, text)) } else { None }).collect()
    };
    said(&events);
    s.run(Command::SetBold(true)).expect("bolds");
    assert_eq!(said(&events), [(K::Formatting, "Bold on".to_string())]);
    s.run(Command::Undo).expect("undoes");
    assert_eq!(said(&events), [(K::Command, "Undo Bold".to_string())]);
    s.run(Command::Redo).expect("redoes");
    assert_eq!(said(&events), [(K::Command, "Redo Bold".to_string())]);
    s.run(Command::Copy).expect("copies");
    assert_eq!(said(&events), [(K::Command, "Copied".to_string())], "a copy changes nothing and still says so");
    // The host's clipboard keys (the browser's copy and cut events) say the same.
    s.copy(false).expect("copies");
    assert_eq!(said(&events), [(K::Command, "Copied".to_string())]);
    s.run(Command::DescribeFormatting).expect("describes");
    let described = said(&events);
    assert!(matches!(&described[..], [(K::Formatting, text)] if text.contains("point") && text.contains("bold")), "{described:?}");
    // Formatting turned off: not said, not emitted.
    s.set_announcements(&[K::Table, K::Command]);
    s.run(Command::SetBold(false)).expect("unbolds");
    assert!(said(&events).is_empty());
    s.set_announcements(&K::ALL);
    // An edit refused where text can't go says why.
    s.set_read_only(true);
    assert!(s.run(Command::InsertText("x".into())).is_err());
    assert_eq!(said(&events), [(K::ReadOnly, "Read only".to_string())]);
    // Each refused keystroke says it again, as Word does.
    s.text("x");
    s.text("y");
    assert_eq!(said(&events), [(K::ReadOnly, "Read only".to_string()), (K::ReadOnly, "Read only".to_string())]);
}

#[test]
fn the_caret_window_goes_with_the_caret_and_the_host_can_move_it() {
    use crate::api::services::{CaretWindow, WindowPoint};
    use crate::api::session::HostRequest;
    let (mut engine, _) = engine();
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("a session");
    s.run(Command::InsertText("First".into())).expect("types");
    s.run(Command::InsertParagraph).expect("splits");
    s.run(Command::InsertText("Second".into())).expect("types");
    let windows: Rc<RefCell<Vec<CaretWindow>>> = Rc::default();
    let sink = windows.clone();
    s.host().attach(move |r| {
        if let HostRequest::CaretText { window } = r {
            sink.borrow_mut().push(window);
        }
    });
    let last = |w: &Rc<RefCell<Vec<CaretWindow>>>| w.borrow().last().cloned().expect("a window");
    let w = last(&windows);
    assert_eq!(w.paragraphs.len(), 2, "both paragraphs: {w:?}");
    assert_eq!(w.focus, WindowPoint { paragraph: 1, offset: 6 }, "the caret after Second");
    // Typing sends the window again; the same window isn't sent twice.
    s.run(Command::InsertText("!".into())).expect("types");
    let count = windows.borrow().len();
    assert_eq!(last(&windows).focus.offset, 7);
    s.host().set_scroll(0.0, 0.0);
    s.run(Command::SetZoom(crate::api::command::Zoom::Percent(100.0))).expect("zooms");
    assert_eq!(windows.borrow().len(), count, "nothing changed in the window");
    // The screen reader moves the caret to the first paragraph's start.
    let start = WindowPoint { paragraph: 0, offset: 0 };
    s.host().caret_window_select(start, start).expect("moves");
    assert_eq!(last(&windows).focus, start);
    let outside = WindowPoint { paragraph: 9, offset: 0 };
    assert_eq!(s.host().caret_window_select(outside, outside).map_err(|e| e.code), Err(ErrorCode::InvalidArgument));
}

#[test]
fn the_accessibility_tree_goes_whole_then_as_throttled_changes() {
    use crate::api::services::A11yUpdate;
    use crate::api::session::HostRequest;
    let (mut engine, _) = engine();
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("a session");
    s.run(Command::InsertText("First".into())).expect("types");
    let updates = Rc::new(RefCell::new(Vec::new()));
    let sink = updates.clone();
    s.host().attach(move |r| {
        if let HostRequest::A11y { update } = r {
            sink.borrow_mut().push(update);
        }
    });
    assert!(matches!(updates.borrow_mut().drain(..).collect::<Vec<_>>()[..], [A11yUpdate::Full(_)]), "attaching sends the whole tree");
    // Edits arm one timer and send nothing yet; a second edit doesn't re-arm it.
    s.run(Command::InsertText(" line".into())).expect("types");
    assert_eq!(s.host().take_timers().iter().filter(|t| t.0 == 3).count(), 1, "the first edit arms the timer");
    s.run(Command::InsertParagraph).expect("splits");
    s.run(Command::InsertText("Second".into())).expect("types");
    assert!(s.host().take_timers().iter().all(|t| t.0 != 3), "later edits don't re-arm it");
    assert!(updates.borrow().is_empty(), "nothing goes before the timer");
    s.host().timer_fired(3);
    let sent = updates.borrow_mut().drain(..).collect::<Vec<_>>();
    assert!(matches!(sent[..], [A11yUpdate::Patch(_)]), "the changes go as a patch: {sent:?}");
    // The next edit arms it again.
    s.run(Command::InsertText("!".into())).expect("types");
    assert_eq!(s.host().take_timers().iter().filter(|t| t.0 == 3).count(), 1);
}

#[test]
fn a_sub_template_body_shown_in_place_is_read_as_a_group_and_goes_to_its_reference() {
    use crate::api::command::{ReferenceRule, SubTemplateRules};
    use crate::api::services::{AccessibleNode, AccessibleRole};
    let mut engine = Engine::new(FontLibrary::system());
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("a session");
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/FX-SUBTEMPLATES.docx");
    s.open(&std::fs::read(path).expect("fixture")).expect("opens");
    let rule = ReferenceRule { field_type: "SUBTEMPLATE".into(), argument: None, category: "Sub-templates".into(), gallery: None };
    s.run(Command::SetSubTemplateRules(SubTemplateRules { rules: vec![rule] })).expect("sets the rules");
    let tree = s.accessible_tree();
    fn all<'a>(n: &'a AccessibleNode, out: &mut Vec<&'a AccessibleNode>) {
        out.push(n);
        n.children.iter().for_each(|c| all(c, out));
    }
    let mut nodes = Vec::new();
    all(&tree, &mut nodes);
    let bodies: Vec<_> = nodes.iter().filter(|n| n.name.as_deref().is_some_and(|t| t.starts_with("Sub-template Violation"))).collect();
    assert!(bodies.len() >= 2, "each shown reference: {:?}", nodes.iter().filter_map(|n| n.name.as_deref()).collect::<Vec<_>>());
    assert!(bodies.iter().all(|b| b.role == AccessibleRole::TextBox && !b.children.is_empty()));
    let ids: std::collections::HashSet<_> = nodes.iter().map(|n| n.id).collect();
    assert_eq!(ids.len(), nodes.len(), "no id twice, though the body shows at several references");
    // A host gets the bodies too: the rules and the view aren't edits, and still send the tree again.
    use crate::api::session::HostRequest;
    let sent = Rc::new(RefCell::new(0));
    let count = sent.clone();
    s.host().attach(move |r| {
        if let HostRequest::A11y { .. } = r {
            *count.borrow_mut() += 1;
        }
    });
    *sent.borrow_mut() = 0;
    s.run(Command::SetSubTemplateView(crate::api::command::SubTemplateView { ids: None, expanded: false })).expect("collapses");
    assert_eq!(s.host().take_timers().iter().filter(|t| t.0 == 3).count(), 1, "collapsing the bodies arms the tree's timer");
    s.host().timer_fired(3);
    assert_eq!(*sent.borrow(), 1, "and sends the change");
    s.run(Command::SetSubTemplateView(crate::api::command::SubTemplateView { ids: None, expanded: true })).expect("shows them");
    // Activating the body's text starts editing at its reference.
    let inside = bodies[0].children[0].id;
    let reference = bodies[0].target.expect("the reference's paragraph");
    s.accessible_go(inside).expect("goes");
    assert_eq!(s.editor.selection().focus().and_then(|p| p.para_offset()).map(|(p, _)| p.0), Some(reference.0));
}

/// The blue pixels of the session's first page as drawn (the adaptive placeholder's colour).
fn blue_pixels(s: &mut crate::api::session::Session) -> usize {
    use crate::render::{Frame, Renderer};
    let fonts = s.fonts.fonts();
    s.editor.complete(&fonts);
    let view = s.editor.view(0..1);
    let images = s.editor.document().images.clone();
    let f = Frame { origin_x: 0.0, origin_y: 0.0, width: 700.0, height: 900.0, dpr: 1.0, px_per_pt: 1.0, caret: false };
    let mut surface = skia_safe::surfaces::raster_n32_premul((700, 900)).expect("a raster surface");
    Renderer::default().draw(surface.canvas(), &view, &images, &f);
    let info = skia_safe::ImageInfo::new((700, 900), skia_safe::ColorType::RGBA8888, skia_safe::AlphaType::Unpremul, None);
    let mut px = vec![0u8; 700 * 900 * 4];
    assert!(surface.read_pixels(&info, &mut px, 700 * 4, (0, 0)));
    px.chunks(4).filter(|p| p[0].abs_diff(0x3F) < 10 && p[1].abs_diff(0x51) < 10 && p[2].abs_diff(0xB5) < 10).count()
}

/// The application's placeholder picture: an SVG one is drawn, saved as Word saves one (a PNG fallback and an
/// `asvg:svgBlip`), read back preferring the SVG, and replaced only while the control shows its placeholder.
#[test]
fn picture_control_placeholder_images() {
    use serde_json::json;
    use std::io::Read;
    let run = |s: &mut crate::api::session::Session, op: serde_json::Value| s.run(serde_json::from_value(op).expect("a command"));
    let svg = crate::render::svg::tests::ADAPTIVE.as_bytes().to_vec();
    let red = br##"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"><rect width="8" height="8" fill="#C00000"/></svg>"##;
    let png = crate::api::value::base64(&crate::render::svg::fallback_png(red, 8, 8).expect("a PNG"));
    let open = |engine: &mut Engine, bytes: &[u8]| {
        let id = engine.create_session(SessionOptions::default());
        engine.session(id).expect("a session").open(bytes).expect("opens");
        id
    };
    let mut engine = Engine::new(FontLibrary::system());
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/FX-TEXT.docx");
    let id = open(&mut engine, &std::fs::read(path).expect("fixture"));
    let s = engine.session(id).expect("a session");
    let insert = json!({
        "type": "insertControl", "kind": "picture", "tag": "Photo", "options": { "width": 250, "height": 125 },
        "placeholderImage": { "data": crate::api::value::base64(&svg), "mime": "image/svg+xml" },
    });
    let c = serde_json::to_value(run(s, insert).expect("inserts").value).expect("json")["id"].clone();
    assert!(blue_pixels(s) > 1000, "the SVG is drawn");
    let saved = s.export(&ExportOptions::Docx).expect("saves").bytes.0;
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(saved.clone())).expect("a package");
    let names: Vec<String> = z.file_names().filter_map(|n| n.ok().map(|n| n.to_string())).collect();
    assert!(names.iter().any(|n| n.ends_with(".svg")) && names.iter().any(|n| n.ends_with("f.png")), "{names:?}");
    let mut xml = String::new();
    z.by_name("word/document.xml").unwrap().read_to_string(&mut xml).unwrap();
    assert!(xml.contains("asvg:svgBlip") && xml.contains("showingPlcHdr"), "an svgBlip in a placeholder");

    let again = open(&mut engine, &saved);
    let a = engine.session(again).expect("a session");
    assert!(a.editor.document().images.contains(&svg), "read back as the SVG");
    assert!(blue_pixels(a) > 1000, "drawn again from the DOCX");

    let s = engine.session(id).expect("a session");
    let set =
        |s: &mut crate::api::session::Session, image: serde_json::Value| run(s, json!({ "type": "setControl", "id": c, "placeholderImage": image }));
    assert!(set(s, json!({ "data": png, "mime": "image/png" })).is_ok(), "replaced while it shows the placeholder");
    assert!(blue_pixels(s) < 50, "the PNG shows instead");
    assert!(set(s, json!({ "data": png, "mime": "image/svg+xml" })).is_err(), "the data must be what the type says");
    assert!(set(s, json!({ "data": png, "mime": "image/webp" })).is_err(), "not a type it takes");
    run(s, json!({ "type": "fillControl", "id": c, "image": png })).expect("fills");
    assert!(set(s, json!({ "data": png, "mime": "image/png" })).is_err(), "filled: no placeholder to replace");
}

#[test]
fn an_activated_placeholder_names_its_control() {
    use crate::api::input::{Key, Modifiers, Point, PointerButton, PointerKind};
    let (mut engine, events) = {
        let mut engine = Engine::new(FontLibrary::system());
        let events = Rc::new(RefCell::new(Vec::new()));
        let sink = events.clone();
        engine.set_listener(move |_, e| sink.borrow_mut().push(e.clone()));
        (engine, events)
    };
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("a session");
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/FX-PLACEHOLDER.docx");
    s.open(&std::fs::read(path).expect("fixture")).expect("opens");
    s.host().attach(|_| {});
    s.host().set_size(2000.0, 4000.0, 1.0);
    let listed = s.placeholders();
    assert_eq!(listed.len(), 1, "{listed:?}");
    assert!(listed[0].element.is_some(), "{listed:?}");
    let k = s.presentation.zoom() * 96.0 / 72.0;
    let pictures = crate::editing::inspect::inspect(&mut s.editor, "geometry", true)["geometry"]["pictures"].as_array().cloned().unwrap_or_default();
    let activated = |events: &Rc<RefCell<Vec<Event>>>| -> Vec<_> {
        events.borrow_mut().drain(..).filter_map(|e| if let Event::PlaceholderActivated(p) = e { Some(p) } else { None }).collect()
    };
    let click = |s: &mut crate::api::session::Session, r: &serde_json::Value| {
        let r: Vec<f64> = r.as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
        let at = Point { x: (r[0] + r[2]) / 2.0 * k, y: (r[1] + r[3]) / 2.0 * k };
        for kind in [PointerKind::Down, PointerKind::Up] {
            s.pointer(kind, at, PointerButton::Primary, 1, Modifiers::default());
        }
    };
    let rect = pictures
        .iter()
        .map(|p| p["rect"].clone())
        .find(|r| {
            click(s, r);
            s.editor.selected_placeholder().is_some()
        })
        .expect("a drawn placeholder");
    // Editable, then read only (a form locked after its first signature): a click, Enter and Space each name it.
    for read_only in [false, true] {
        s.set_read_only(read_only);
        activated(&events);
        click(s, &rect);
        s.key(Key::Enter, Modifiers::default());
        s.text(" ");
        assert_eq!(activated(&events), vec![listed[0].clone(); 3], "read only: {read_only}");
    }
    s.set_read_only(false);
    let data = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/FX-OBJ.docx")).expect("fixture");
    let image = crate::editing::Editor::open(&data, &s.fonts.fonts()).expect("opens").document().images.last().cloned().expect("an image");
    s.run(Command::FillPlaceholder(crate::api::command::FillPlaceholder {
        key: Some(listed[0].key.clone()),
        image: crate::api::value::Bytes(image),
    }))
    .expect("fills");
    activated(&events);
    click(s, &rect);
    s.key(Key::Enter, Modifiers::default());
    assert!(activated(&events).is_empty(), "filled: nothing");
}
