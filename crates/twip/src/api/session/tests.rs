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
fn a_typical_state_is_under_12_kb() {
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
    assert!(json.len() < 12 * 1024, "{} bytes", json.len());
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
    let mut engine = Engine::new(FontLibrary::system_recommended());
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
    let mut engine = Engine::new(FontLibrary::system_recommended());
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
    let mut engine = Engine::new(FontLibrary::system_recommended());
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
    let mut engine = Engine::new(FontLibrary::system_recommended());
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
    let mut engine = Engine::new(FontLibrary::system_recommended());
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
    let mut engine = Engine::new(FontLibrary::system_recommended());
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
    let mut engine = Engine::new(FontLibrary::system_recommended());
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
    engine.fonts().set_symbol_fallback(crate::api::fonts::SymbolFallback::Off).expect("off");
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("a session");
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../bindings/hello.docx");
    s.open(&std::fs::read(path).expect("the fixture")).expect("opens");
    s.host().attach(|_| {});
    s.host().set_size(800.0, 600.0, 1.0);
    s.host().set_focus(true);
    assert!(!s.host().frame_info().caret_visible, "no face at all: no lines, no caret");
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
    assert!(events.iter().any(|e| matches!(e, Event::FontsChanged(state) if state.missing.iter().any(|m| m.family == "Zorble Sans"))), "{events:?}");
    assert!(events.iter().any(|e| matches!(e, Event::Warning(w) if w.message.contains("Zorble Sans"))), "{events:?}");
    assert!(s.state().fonts.missing.iter().any(|m| m.family == "Zorble Sans"), "the state has it too");
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
    let mut engine = Engine::new(FontLibrary::system_recommended());
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
    let mut engine = Engine::new(FontLibrary::system_recommended());
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
    let mut engine = Engine::new(FontLibrary::system_recommended());
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
    let mut engine = Engine::new(FontLibrary::system_recommended());
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
    let mut engine = Engine::new(FontLibrary::system_recommended());
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
    let mut engine = Engine::new(FontLibrary::system_recommended());
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
    let mut engine = Engine::new(FontLibrary::system_recommended());
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("a session");
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/FX-PAGES.docx");
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
    let (mut engine, events) = (Engine::new(FontLibrary::system_recommended()), Rc::new(RefCell::new(Vec::new())));
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
fn field_settings_in_the_options_apply_from_each_open() {
    use crate::api::command::ReferenceRule;
    let field = |code: &str| {
        format!(
            "<w:p><w:r><w:fldChar w:fldCharType=\"begin\"/></w:r><w:r><w:instrText xml:space=\"preserve\"> {code} </w:instrText></w:r>\
             <w:r><w:fldChar w:fldCharType=\"separate\"/></w:r><w:r><w:fldChar w:fldCharType=\"end\"/></w:r></w:p>"
        )
    };
    let body = format!("{}{}", field("CUSTOMTEXT Customer"), field("CUSTOMVALUE Total"));
    let options = SessionOptions {
        mode: crate::api::state::Mode::Design,
        field_labels: [("CUSTOMTEXT Customer".into(), "Customer name".into())].into(),
        application_field_types: vec!["CUSTOMTEXT".into(), "CUSTOMVALUE".into()],
        sub_template_rules: vec![ReferenceRule { field_type: "SUBTEMPLATE".into(), argument: None, category: "Sub-templates".into(), gallery: None }],
        ..SessionOptions::default()
    };
    let mut engine = Engine::new(FontLibrary::system_recommended());
    let id = engine.create_session(options);
    let s = engine.session(id).expect("a session");
    for _ in 0..2 {
        s.open(&crate::editing::tests::blank_body(&body)).expect("opens");
        s.editor.complete(&s.fonts.fonts());
        let drawn = crate::layout::dump::page_text(&s.editor.snapshot(), s.editor.document());
        assert!(drawn.contains("Customer name") && drawn.contains("\u{ab}Total\u{bb}") && !drawn.contains("Error!"), "{drawn}");
        assert!(!s.state().modified, "the settings aren't edits");
    }
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/FX-SUBTEMPLATES.docx");
    s.open(&std::fs::read(fixture).expect("the fixture")).expect("opens");
    assert!(s.editor.references().by_field.values().any(|r| r.name == "Violation"), "the rules hold for the new document");
}

#[test]
fn sub_template_bodies_shown_inline_never_reach_the_output() {
    use crate::api::command::{ReferenceRule, SubTemplateRules, SubTemplateView};
    let mut engine = Engine::new(FontLibrary::system_recommended());
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
    let mut engine = Engine::new(FontLibrary::system_recommended());
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
    let mut engine = Engine::new(FontLibrary::system_recommended());
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
    let mut engine = Engine::new(FontLibrary::system_recommended());
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

/// The editor fixture `name`.
fn editor_fixture(name: &str) -> Vec<u8> {
    std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor").join(name)).expect("the fixture")
}

/// The stand-in font `file`.
fn stand_in(file: &str) -> Vec<u8> {
    std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/fonts").join(file)).expect("the font")
}

/// An engine with only Stand-in Sans, and a session with `FX-FONT-TABLE` open.
fn font_table() -> (Engine, crate::api::engine::SessionId, Rc<RefCell<Vec<Event>>>) {
    let (mut engine, events) = engine();
    engine.fonts().add(&stand_in("stand-in-sans.ttf")).expect("adds");
    let id = engine.create_session(SessionOptions::default());
    engine.session(id).expect("a session").open(&editor_fixture("FX-FONT-TABLE.docx")).expect("opens");
    (engine, id, events)
}

fn missing_of(state: &crate::api::state::FontsState) -> Vec<(String, bool, crate::api::state::StandIn, Option<String>)> {
    state.missing.iter().map(|m| (m.family.clone(), m.bold, m.by, m.drawn_with.clone())).collect()
}

#[test]
fn missing_fonts_are_reported_per_style_with_what_draws_them() {
    use crate::api::state::StandIn::LastResort;
    let (mut engine, id, events) = font_table();
    let s = engine.session(id).expect("a session");
    let sans = Some("Stand-in Sans".to_string());
    assert_eq!(
        missing_of(&s.state().fonts),
        vec![
            ("Consolas".into(), false, LastResort, sans.clone()),
            ("Garamond".into(), false, LastResort, sans.clone()),
            ("Garamond".into(), true, LastResort, sans.clone()),
            ("Mystery Sans".into(), false, LastResort, sans.clone()),
            ("Zzyzx Unknown".into(), false, LastResort, sans.clone()),
        ]
    );
    let chars = &s.state().fonts.characters;
    assert!(chars.iter().any(|c| c.family == "Stand-in Sans" && c.characters.contains('\u{6f22}')), "{chars:?}");
    let opened = events.borrow().iter().filter(|e| matches!(e, Event::FontsChanged(_))).count();
    assert_eq!(opened, 1, "one report for the open");
    // A font for one family arrives: that family's entry goes, and the report is sent again.
    engine.fonts().add(&stand_in("zzyzx-unknown.ttf")).expect("adds");
    engine.sync_fonts();
    let s = engine.session(id).expect("a session");
    assert!(!s.state().fonts.missing.iter().any(|m| m.family == "Zzyzx Unknown"));
    let last = events.borrow().iter().rev().find_map(|e| match e {
        Event::FontsChanged(state) => Some(state.clone()),
        _ => None,
    });
    assert_eq!(last.as_ref(), Some(&s.state().fonts));
    // A library change that moves no list sends no report.
    let before = events.borrow().len();
    engine.fonts().set_fallback(Some("Not Installed"));
    engine.sync_fonts();
    assert!(!events.borrow()[before..].iter().any(|e| matches!(e, Event::FontsChanged(_))));
}

#[test]
fn the_missing_font_warning_names_each_family_and_what_draws_it_once() {
    let (mut engine, events) = engine();
    engine.fonts().add(&stand_in("stand-in-sans.ttf")).expect("adds");
    let id = engine.create_session(SessionOptions::default());
    let warnings = engine.session(id).expect("a session").open(&editor_fixture("FX-FONT-TABLE.docx")).expect("opens");
    let font = warnings.iter().find(|w| w.code == crate::api::warning::WarningCode::FontUnresolved).expect("a warning");
    assert!(font.message.contains("Garamond (bold); Stand-in Sans draws it"), "{}", font.message);
    assert_eq!(font.count, 5);
    events.borrow_mut().clear();
    engine.fonts().set_fallback(None);
    engine.sync_fonts();
    assert!(!events.borrow().iter().any(|e| matches!(e, Event::Warning(_))), "not warned again");
}

#[test]
fn incremental_character_coverage_equals_a_full_rebuild() {
    use crate::api::command::DeleteOptions;
    use crate::api::value::Direction;
    struct Rng(u64);
    impl Rng {
        fn below(&mut self, n: usize) -> usize {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            (self.0 % n.max(1) as u64) as usize
        }
    }
    let (mut engine, id, _) = font_table();
    let s = engine.session(id).expect("a session");
    s.run(Command::SelectAll).expect("selects");
    s.run(Command::Delete(DeleteOptions { direction: Direction::Backward, unit: None })).expect("clears");
    let mut rng = Rng(0x5EED_F0E7);
    let texts = ["a", "\u{6f22}", "\u{F0B7}", "\u{2603}", "bc", "\u{201c}"];
    for _ in 0..200 {
        match rng.below(4) {
            0 | 1 => s.run(Command::InsertText(texts[rng.below(texts.len())].into())).map(drop),
            2 => s.run(Command::Delete(DeleteOptions { direction: Direction::Backward, unit: None })).map(drop),
            _ => s.run(Command::InsertParagraph).map(drop),
        }
        .expect("edits");
    }
    let incremental = (s.watch.coverage.clone(), s.state().fonts.characters.clone());
    s.cover_all();
    let full = s.watch.coverage.clone();
    assert_eq!(incremental.0, full, "per-paragraph coverage");
    assert_eq!(incremental.1, super::fonts::characters(&full), "reported characters");
}

fn alias(from: &str, to: &str, bold: Option<bool>) -> crate::api::fonts::Alias {
    crate::api::fonts::Alias { from: from.into(), bold, italic: None, to: crate::api::fonts::AliasTarget::Family(to.into()) }
}

fn missing_names(state: &crate::api::state::FontsState) -> Vec<(String, bool)> {
    state.missing.iter().map(|m| (m.family.clone(), m.bold)).collect()
}

#[test]
fn an_alias_draws_a_missing_family_and_the_document_keeps_its_name() {
    let (mut engine, id, _) = font_table();
    engine.fonts().add(&stand_in("stand-in-serif.ttf")).expect("adds");
    engine.fonts().set_aliases(vec![alias("Garamond", "Stand-in Serif", None)]);
    engine.sync_fonts();
    let s = engine.session(id).expect("a session");
    let missing = missing_names(&s.state().fonts);
    assert!(!missing.iter().any(|(f, _)| f == "Garamond"), "{missing:?}");
    assert_eq!(s.fonts().fonts().face("Garamond", None, false, false).map(|f| f.family.clone()).as_deref(), Some("Stand-in Serif"));
    s.find(&crate::api::services::FindQuery { text: "Garamond text".into(), ..Default::default() }, crate::api::services::FindDirection::Next);
    assert_eq!(s.state().formatting.font, crate::api::state::Tri::Uniform("Garamond".into()), "the font box names the document's family");
    let saved = s.export(&ExportOptions::default()).expect("saves");
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(saved.bytes.0)).expect("a package");
    let mut xml = String::new();
    std::io::Read::read_to_string(&mut zip.by_name("word/document.xml").expect("the body"), &mut xml).expect("reads");
    assert!(xml.contains(r#"w:ascii="Garamond""#), "the saved file keeps the family");
}

#[test]
fn an_alias_applies_per_style_and_reports_its_missing_target() {
    let (mut engine, id, _) = font_table();
    engine.fonts().add(&stand_in("stand-in-serif.ttf")).expect("adds");
    engine.fonts().set_aliases(vec![alias("Garamond", "Stand-in Serif", Some(true)), alias("Consolas", "Nowhere", None)]);
    engine.sync_fonts();
    let s = engine.session(id).expect("a session");
    let state = s.state().fonts.clone();
    let missing = missing_names(&state);
    assert!(missing.contains(&("Garamond".into(), false)) && !missing.contains(&("Garamond".into(), true)), "{missing:?}");
    let consolas = state.missing.iter().find(|m| m.family == "Consolas").expect("Consolas");
    assert_eq!(consolas.tried, ["Nowhere"]);
    // An alias for a family the library has doesn't apply.
    engine.fonts().set_aliases(vec![alias("Stand-in Sans", "Stand-in Serif", None)]);
    engine.sync_fonts();
    let s = engine.session(id).expect("a session");
    assert_eq!(s.fonts().fonts().face("Stand-in Sans", None, false, false).map(|f| f.family.clone()).as_deref(), Some("Stand-in Sans"));
}

fn defaults(serif: Option<&str>, monospace: Option<&str>) -> crate::api::fonts::GenericDefaults {
    crate::api::fonts::GenericDefaults { serif: serif.map(Into::into), monospace: monospace.map(Into::into), ..Default::default() }
}

fn by_family(
    state: &crate::api::state::FontsState,
) -> Vec<(String, crate::api::state::StandIn, Option<crate::api::fonts::GenericFamily>, Option<String>)> {
    state.missing.iter().filter(|m| !m.bold).map(|m| (m.family.clone(), m.by, m.category, m.drawn_with.clone())).collect()
}

#[test]
fn generic_defaults_draw_missing_families_by_what_the_font_table_says() {
    use crate::api::fonts::GenericFamily::{Monospace, SansSerif, Serif};
    use crate::api::state::StandIn::{Generic, LastResort};
    let (mut engine, id, _) = font_table();
    for font in ["stand-in-serif.ttf", "stand-in-mono.ttf"] {
        engine.fonts().add(&stand_in(font)).expect("adds");
    }
    engine.fonts().set_fallback(Some("Stand-in Sans"));
    engine.fonts().set_generic_defaults(defaults(Some("Stand-in Serif"), Some("Stand-in Mono")));
    engine.sync_fonts();
    let s = engine.session(id).expect("a session");
    let sans = Some("Stand-in Sans".to_string());
    use crate::api::state::StandIn::Fallback;
    assert_eq!(
        by_family(&s.state().fonts),
        vec![
            ("Consolas".into(), Generic, Some(Monospace), Some("Stand-in Mono".into())),
            ("Garamond".into(), Generic, Some(Serif), Some("Stand-in Serif".into())),
            ("Mystery Sans".into(), Fallback, Some(SansSerif), sans.clone()),
            ("Zzyzx Unknown".into(), Fallback, None, sans.clone()),
        ]
    );
    // Off: only aliases to a category give one.
    engine.fonts().set_classification(false);
    engine.fonts().set_aliases(vec![crate::api::fonts::Alias {
        from: "Garamond".into(),
        bold: None,
        italic: None,
        to: crate::api::fonts::AliasTarget::Category { category: Serif },
    }]);
    engine.sync_fonts();
    let s = engine.session(id).expect("a session");
    let state = by_family(&s.state().fonts);
    assert!(state.contains(&("Garamond".into(), Generic, Some(Serif), Some("Stand-in Serif".into()))), "{state:?}");
    assert!(state.iter().any(|(f, by, c, _)| f == "Consolas" && *by == Fallback && c.is_none()), "{state:?}");
    // A category whose default is missing falls to the fallback; with no fallback, the first face.
    engine.fonts().set_classification(true);
    engine.fonts().set_aliases(Vec::new());
    engine.fonts().set_generic_defaults(defaults(Some("Not Installed"), None));
    engine.fonts().set_fallback(None);
    engine.sync_fonts();
    let s = engine.session(id).expect("a session");
    let state = by_family(&s.state().fonts);
    assert!(state.contains(&("Garamond".into(), LastResort, Some(Serif), sans.clone())), "{state:?}");
}

#[test]
fn with_no_generic_defaults_layout_is_the_same_as_without_the_feature() {
    let docx = editor_fixture("FX-FONT-TABLE.docx");
    let dump = |classify: bool| {
        let mut fonts = crate::font::Fonts::default();
        for font in ["stand-in-sans.ttf", "stand-in-serif.ttf", "stand-in-mono.ttf"] {
            fonts.add_bytes(stand_in(font)).expect("adds");
        }
        fonts.set_settings(|s| s.classify_from_document = classify);
        let doc = crate::docx::read(&docx).expect("reads");
        let text = format!("{:?}", crate::layout::layout(&doc, &fonts).pages());
        // Ids come from process-wide counters: integers are masked, floats compared as written.
        let mut out = String::with_capacity(text.len());
        let mut digits = false;
        for (k, c) in text.char_indices() {
            let float = text[k..].starts_with(|c: char| c.is_ascii_digit())
                && (text[..k].ends_with('.') || text[k..].trim_start_matches(|c: char| c.is_ascii_digit()).starts_with('.'));
            if c.is_ascii_digit() && !float {
                if !digits {
                    out.push('#');
                }
                digits = true;
            } else {
                digits = false;
                out.push(c);
            }
        }
        out
    };
    assert_eq!(dump(true), dump(false));
}

#[test]
fn a_replacement_draws_even_when_the_family_is_installed_and_the_document_is_unchanged() {
    use crate::api::fonts::FontRule;
    let (mut engine, id, _) = font_table();
    engine.fonts().add(&stand_in("stand-in-serif.ttf")).expect("adds");
    let before = engine.session(id).expect("a session").export(&ExportOptions::default()).expect("saves").bytes.0;
    engine.fonts().set_replacements(vec![FontRule { from: "Stand-in Sans".into(), bold: None, italic: None, to: "Stand-in Serif".into() }]);
    engine.sync_fonts();
    let s = engine.session(id).expect("a session");
    assert_eq!(s.fonts().fonts().face("Stand-in Sans", None, false, false).map(|f| f.family.clone()).as_deref(), Some("Stand-in Serif"));
    assert_eq!(s.export(&ExportOptions::default()).expect("saves").bytes.0, before, "the saved document is unchanged");
    s.find(&crate::api::services::FindQuery { text: "CJK".into(), ..Default::default() }, crate::api::services::FindDirection::Next);
    assert_eq!(s.state().formatting.font, crate::api::state::Tri::Uniform("Stand-in Sans".into()), "the font box names the document's family");
    // A replacement whose target is missing: the document's family draws, and the target is reported.
    engine.fonts().set_replacements(vec![FontRule { from: "Stand-in Sans".into(), bold: None, italic: None, to: "Nowhere Serif".into() }]);
    engine.sync_fonts();
    let s = engine.session(id).expect("a session");
    let state = s.state().fonts.clone();
    let target = state.missing.iter().find(|m| m.family == "Nowhere Serif").expect("the target is reported");
    assert_eq!((target.by, target.drawn_with.as_deref()), (crate::api::state::StandIn::Original, Some("Stand-in Sans")));
    assert!(!state.missing.iter().any(|m| m.family == "Stand-in Sans"), "the family itself draws");
}

/// An engine with Stand-in Sans and a session with `FX-FONT-REWRITE` open.
fn font_rewrite() -> (Engine, crate::api::engine::SessionId) {
    let (mut engine, _) = engine();
    engine.fonts().add(&stand_in("stand-in-sans.ttf")).expect("adds");
    let id = engine.create_session(SessionOptions::default());
    engine.session(id).expect("a session").open(&editor_fixture("FX-FONT-REWRITE.docx")).expect("opens");
    (engine, id)
}

fn rewrite(from: &str, to: &str) -> crate::api::command::ReplaceFont {
    crate::api::command::ReplaceFont { from: from.into(), to: to.into(), scope: Default::default(), bold: None, italic: None }
}

/// A part of the session's saved document.
fn saved_part(s: &mut super::Session, name: &str) -> String {
    let saved = s.export(&ExportOptions::default()).expect("saves");
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(saved.bytes.0)).expect("a package");
    let mut xml = String::new();
    std::io::Read::read_to_string(&mut zip.by_name(name).expect("the part"), &mut xml).expect("reads");
    xml
}

/// The `w:r` element whose text is `text`.
fn run_xml<'a>(xml: &'a str, text: &str) -> &'a str {
    let at = xml.find(&format!(">{text}<")).unwrap_or_else(|| panic!("no run with {text}"));
    let start = xml[..at].rfind("<w:r>").or_else(|| xml[..at].rfind("<w:r ")).expect("a run");
    &xml[start..at]
}

#[test]
fn replace_font_rewrites_the_texts_family_skips_locked_controls_and_undoes_in_one_step() {
    use crate::api::command::CommandResult;
    let (mut engine, id) = font_rewrite();
    let s = engine.session(id).expect("a session");
    let before = saved_part(s, "word/document.xml");
    let commit = s.run(Command::ReplaceFont(rewrite("Calibri", "Stand-in Sans"))).expect("replaces");
    let Some(CommandResult::FontReplaced { changed, skipped }) = commit.value else { panic!("{:?}", commit.value) };
    assert!(changed >= 5 && skipped == 1, "changed {changed}, skipped {skipped}");
    let xml = saved_part(s, "word/document.xml");
    for text in ["Direct Calibri.", "Bold Calibri."] {
        assert!(run_xml(&xml, text).contains(r#"w:ascii="Stand-in Sans""#), "{text}: {}", run_xml(&xml, text));
    }
    assert!(run_xml(&xml, "Locked Calibri.").contains(r#"w:ascii="Calibri""#), "the locked control keeps its font");
    assert!(!xml.contains(r#"w:font="Calibri""#), "the symbol no longer names Calibri");
    assert!(run_xml(&xml, "Times New Roman.").contains("Times New Roman"));
    s.run(Command::Undo).expect("undoes");
    assert_eq!(saved_part(s, "word/document.xml"), before, "one undo restores the text");
}

#[test]
fn replace_font_records_formatting_revisions_and_honours_its_filters() {
    let (mut engine, id) = font_rewrite();
    let s = engine.session(id).expect("a session");
    s.run(Command::SetTrackChanges(true)).expect("tracks");
    let bold = crate::api::command::ReplaceFont { bold: Some(true), ..rewrite("Calibri", "Stand-in Sans") };
    s.run(Command::ReplaceFont(bold)).expect("replaces");
    let xml = saved_part(s, "word/document.xml");
    assert!(run_xml(&xml, "Bold Calibri.").contains("w:rPrChange"), "{}", run_xml(&xml, "Bold Calibri."));
    assert!(run_xml(&xml, "Direct Calibri.").contains(r#"w:ascii="Calibri""#), "the filter keeps regular text");
}

#[test]
fn replace_font_in_a_selection_gives_theme_text_its_own_family() {
    let (mut engine, id) = font_rewrite();
    let s = engine.session(id).expect("a session");
    s.find(&crate::api::services::FindQuery { text: "Theme Calibri.".into(), ..Default::default() }, crate::api::services::FindDirection::Next);
    let selection = crate::api::command::ReplaceFont { scope: crate::api::command::ReplaceScope::Selection, ..rewrite("Calibri", "Stand-in Sans") };
    s.run(Command::ReplaceFont(selection)).expect("replaces");
    let xml = saved_part(s, "word/document.xml");
    let run = run_xml(&xml, "Theme Calibri.");
    assert!(run.contains(r#"w:ascii="Stand-in Sans""#) && !run.contains("asciiTheme"), "{run}");
    assert!(run_xml(&xml, "Direct Calibri.").contains(r#"w:ascii="Calibri""#), "outside the selection");
}

#[test]
fn replace_font_with_nothing_to_change_adds_no_undo_step_and_a_read_only_session_refuses() {
    use crate::api::command::CommandResult;
    let (mut engine, id) = font_rewrite();
    let s = engine.session(id).expect("a session");
    let commit = s.run(Command::ReplaceFont(rewrite("Nope", "Other"))).expect("runs");
    assert_eq!(commit.value, Some(CommandResult::FontReplaced { changed: 0, skipped: 0 }));
    assert!(!s.state().history.can_undo && commit.record.is_none(), "no undo step");
    assert_eq!(s.run(Command::ReplaceFont(rewrite("Calibri", "calibri"))).map(drop).expect_err("same family").code, ErrorCode::InvalidArgument);
    s.set_read_only(true);
    assert_eq!(s.run(Command::ReplaceFont(rewrite("Calibri", "Arial"))).map(drop).expect_err("read-only").code, ErrorCode::ReadOnly);
}

#[test]
fn replace_font_rewrites_the_tables_below_the_text_and_undo_restores_every_part() {
    let (mut engine, id) = font_rewrite();
    let s = engine.session(id).expect("a session");
    let parts = ["word/document.xml", "word/styles.xml", "word/numbering.xml", "word/theme/theme1.xml", "word/fontTable.xml"];
    let before: Vec<String> = parts.iter().map(|p| saved_part(s, p)).collect();
    s.run(Command::ReplaceFont(rewrite("Calibri", "Stand-in Sans"))).expect("replaces");
    for part in &parts[1..] {
        let xml = saved_part(s, part);
        assert!(!xml.contains("\"Calibri\""), "{part} still names Calibri");
        assert!(xml.contains("\"Stand-in Sans\""), "{part} names the new family");
    }
    let doc = saved_part(s, "word/document.xml");
    let locked = run_xml(&doc, "Locked Calibri.");
    assert_eq!(doc.matches("\"Calibri\"").count(), locked.matches("\"Calibri\"").count(), "only the locked control keeps Calibri");
    // The run that follows the theme's minor font now resolves to the new family, and matches a fresh read.
    let family_of = |doc: &crate::docx::Document, text: &str| {
        let index = crate::editing::index::DocIndex::build(doc);
        index
            .para_ids()
            .filter_map(|id| index.para(doc, id))
            .flat_map(|p| p.inlines.iter())
            .find_map(|i| match i {
                crate::docx::Inline::Text(t, run) if t == text => Some((run.ascii.clone(), run.h_ansi.clone())),
                _ => None,
            })
            .expect("the run")
    };
    let saved = s.export(&ExportOptions::default()).expect("saves").bytes.0;
    let fresh = crate::docx::read(&saved).expect("reads");
    for text in ["Theme Calibri.", "Styled Calibri.", "Default Calibri."] {
        let live = family_of(s.editor.document(), text);
        assert_eq!(live.0, "Stand-in Sans", "{text}");
        assert_eq!(live, family_of(&fresh, text), "{text}: incremental equals a fresh read");
    }
    s.run(Command::Undo).expect("undoes");
    let after: Vec<String> = parts.iter().map(|p| saved_part(s, p)).collect();
    assert_eq!(after, before, "one undo restores every part");
}

#[test]
fn character_fallback_draws_characters_the_runs_font_lacks() {
    let (mut engine, id, _) = font_table();
    let s = engine.session(id).expect("a session");
    assert!(s.state().fonts.characters.iter().any(|c| c.characters.contains('\u{6f22}')), "missing before");
    engine.fonts().add(&stand_in("stand-in-cjk.ttf")).expect("adds");
    engine.fonts().set_character_fallback(vec!["Stand-in CJK".into()]);
    engine.sync_fonts();
    let s = engine.session(id).expect("a session");
    assert!(!s.state().fonts.characters.iter().any(|c| c.characters.contains('\u{6f22}')), "{:?}", s.state().fonts.characters);
    let fonts = s.fonts().fonts();
    assert_eq!(fonts.fallback('\u{6f22}', false, false).map(|f| f.family.clone()).as_deref(), Some("Stand-in CJK"));
}

/// `FX-FONT-TABLE` laid out with Stand-in Sans and `extra`, as a dump with integers (ids, glyph ids) masked.
fn font_table_layout(extra: Option<Vec<u8>>, aliases: Vec<crate::api::fonts::Alias>) -> String {
    let mut fonts = crate::font::Fonts::default();
    fonts.add_bytes(stand_in("stand-in-sans.ttf")).expect("adds");
    if let Some(bytes) = extra {
        fonts.add_bytes(bytes).expect("adds");
    }
    fonts.set_settings(|s| s.aliases = aliases);
    let doc = crate::docx::read(&editor_fixture("FX-FONT-TABLE.docx")).expect("reads");
    let text = format!("{:?}", crate::layout::layout(&doc, &fonts).pages());
    let mut out = String::with_capacity(text.len());
    let mut digits = false;
    for (k, c) in text.char_indices() {
        let float = text[k..].starts_with(|c: char| c.is_ascii_digit())
            && (text[..k].ends_with('.') || text[k..].trim_start_matches(|c: char| c.is_ascii_digit()).starts_with('.'));
        if c.is_ascii_digit() && !float {
            if !digits {
                out.push('#');
            }
            digits = true;
        } else {
            digits = false;
            out.push(c);
        }
    }
    out
}

#[test]
fn shared_east_asian_characters_lay_out_as_with_ms_mincho_without_it() {
    let bundled = font_table_layout(None, Vec::new());
    // An alias to a font of the application's wins over the bundled face.
    let aliased = font_table_layout(Some(stand_in("stand-in-serif.ttf")), vec![alias("MS Mincho", "Stand-in Serif", None)]);
    assert_ne!(bundled, aliased, "the alias draws instead");
    let path = std::path::Path::new(&std::env::var("LOCALAPPDATA").unwrap_or_default()).join(r"Microsoft\Windows\Fonts\MSMINCHO.TTF");
    let Ok(mincho) = std::fs::read(&path) else { return };
    // Only the face's name differs: every position and width is the same.
    let real = font_table_layout(Some(mincho), Vec::new()).replace("Face(MS Mincho", "Face(Twip East Asian Punctuation");
    assert!(bundled == real, "the bundled face lays out as MS Mincho does");
}

#[test]
fn an_activated_picture_control_names_it() {
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
    let listed: Vec<_> = s
        .elements(&crate::api::services::ElementFilter::default())
        .into_iter()
        .filter(|e| e.size.is_some() && e.showing_placeholder == Some(true))
        .map(|e| crate::api::services::PictureControlActivation { element: e.id })
        .collect();
    assert_eq!(listed.len(), 1, "{listed:?}");
    let k = s.presentation.zoom() * 96.0 / 72.0;
    let pictures = crate::editing::inspect::inspect(&mut s.editor, "geometry", true)["geometry"]["pictures"].as_array().cloned().unwrap_or_default();
    let activated = |events: &Rc<RefCell<Vec<Event>>>| -> Vec<_> {
        events.borrow_mut().drain(..).filter_map(|e| if let Event::PictureControlActivated(p) = e { Some(p) } else { None }).collect()
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
            s.editor.selected_picture_unfilled()
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
    s.run(Command::FillControl(crate::api::command::FillControl {
        id: listed[0].element.clone(),
        text: None,
        image: Some(crate::api::value::Bytes(image)),
        docx: None,
    }))
    .expect("fills");
    activated(&events);
    let pictures = crate::editing::inspect::inspect(&mut s.editor, "geometry", true)["geometry"]["pictures"].as_array().cloned().unwrap_or_default();
    let found = pictures.iter().any(|p| {
        click(s, &p["rect"]);
        s.editor.selected_picture_control().is_some()
    });
    assert!(found, "the filled picture control");
    assert!(activated(&events).is_empty(), "filled: a click selects only");
    s.key(Key::Enter, Modifiers::default());
    assert_eq!(activated(&events), vec![listed[0].clone()], "filled: Enter still activates");
}

/// A click selects a picture control whatever its gesture is bound to; activation comes from the click's default, a
/// double-click (once) or Enter in editing and read-only modes, and from `activatePictureControl` in any mode. In design
/// mode a double-click or Enter activates the element instead.
#[test]
fn picture_control_activation() {
    use crate::api::input::{Key, Modifiers, Point, PointerButton, PointerKind};
    use crate::api::services::{GestureAction, GestureId};
    let (mut engine, events) = {
        let mut engine = Engine::new(FontLibrary::system());
        let events = Rc::new(RefCell::new(Vec::new()));
        let sink = events.clone();
        engine.set_listener(move |_, e| sink.borrow_mut().push(e.clone()));
        (engine, events)
    };
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("a session");
    s.open(&editor_fixture("FX-PICTURE-CONTROLS.docx")).expect("opens");
    s.run(Command::InsertControl(serde_json::from_value(serde_json::json!({ "kind": "plainText", "tag": "Text" })).unwrap()))
        .expect("a text control");
    s.host().attach(|_| {});
    s.host().set_size(2000.0, 4000.0, 1.0);
    let k = s.presentation.zoom() * 96.0 / 72.0;
    let sig = s
        .elements(&crate::api::services::ElementFilter::default())
        .into_iter()
        .find(|e| e.tag.as_deref() == Some("Placeholder:Signature.X"))
        .expect("the signature control")
        .id;
    let tally = |events: &Rc<RefCell<Vec<Event>>>| -> (Vec<crate::api::value::ElementId>, usize, usize) {
        let all: Vec<Event> = events.borrow_mut().drain(..).collect();
        let activated = all.iter().filter_map(|e| if let Event::PictureControlActivated(p) = e { Some(p.element.clone()) } else { None }).collect();
        let gestures = all
            .iter()
            .filter(|e| matches!(e, Event::Gesture(g) if matches!(g.gesture, GestureId::PictureControlClick | GestureId::PictureControlDoubleClick)))
            .count();
        let elements = all.iter().filter(|e| matches!(e, Event::ElementActivated { .. })).count();
        (activated, gestures, elements)
    };
    let press = |s: &mut crate::api::session::Session, r: &serde_json::Value, clicks: u8| {
        let r: Vec<f64> = r.as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
        let at = Point { x: (r[0] + r[2]) / 2.0 * k, y: (r[1] + r[3]) / 2.0 * k };
        for kind in [PointerKind::Down, PointerKind::Up] {
            s.pointer(kind, at, PointerButton::Primary, clicks, Modifiers::default());
        }
    };
    let pictures = crate::editing::inspect::inspect(&mut s.editor, "geometry", true)["geometry"]["pictures"].as_array().cloned().unwrap_or_default();
    let rect = pictures
        .iter()
        .map(|p| p["rect"].clone())
        .find(|r| {
            press(s, r, 1);
            s.editor
                .selected_picture_control()
                .is_some_and(|c| crate::api::value::ElementId(crate::editing::elements::id_str(crate::docx::ElementId::Control(c))) == sig)
        })
        .expect("the drawn signature");
    let selected = |s: &crate::api::session::Session| {
        s.editor
            .selected_picture_control()
            .map(|c| crate::api::value::ElementId(crate::editing::elements::id_str(crate::docx::ElementId::Control(c))))
    };
    for read_only in [false, true] {
        s.set_read_only(read_only);
        for (binding, click) in [(GestureAction::Default, 1), (GestureAction::None, 0), (GestureAction::Emit, 0)] {
            s.gestures().set(GestureId::PictureControlClick, binding.clone()).expect("binds");
            s.run(Command::SelectAll).expect("selects");
            tally(&events);
            press(s, &rect, 1);
            let (a, g, _) = tally(&events);
            assert_eq!(selected(s), Some(sig.clone()), "{read_only} {binding:?}: the click selects");
            assert_eq!(a.len(), click, "{read_only} {binding:?}: the click activates");
            assert_eq!(g, (binding == GestureAction::Emit) as usize, "{read_only} {binding:?}: the gesture event");
            press(s, &rect, 1);
            press(s, &rect, 2);
            let (a, _, _) = tally(&events);
            assert_eq!(a, vec![sig.clone()], "{read_only} {binding:?}: one activation per double-click");
            s.key(Key::Enter, Modifiers::default());
            assert_eq!(tally(&events).0, vec![sig.clone()], "{read_only} {binding:?}: Enter");
        }
    }
    s.set_read_only(false);
    s.gestures().set(GestureId::PictureControlClick, GestureAction::Default).expect("binds");
    s.run(Command::SetMode(crate::api::state::Mode::Design)).expect("design");
    tally(&events);
    press(s, &rect, 1);
    press(s, &rect, 2);
    s.key(Key::Enter, Modifiers::default());
    let (a, _, elements) = tally(&events);
    assert!(a.is_empty() && elements >= 1, "design mode activates the element: {a:?} {elements}");
    for mode in [crate::api::state::Mode::Design, crate::api::state::Mode::Editing] {
        s.run(Command::SetMode(mode)).expect("mode");
        tally(&events);
        s.run(Command::ActivatePictureControl(crate::api::command::ActivatePictureControl { element: Some(sig.clone()) })).expect("activates");
        assert_eq!((tally(&events).0, selected(s)), (vec![sig.clone()], Some(sig.clone())), "{mode:?}: by the API");
        s.run(Command::ActivatePictureControl(Default::default())).expect("the selected one");
        assert_eq!(tally(&events).0, vec![sig.clone()]);
    }
    s.run(Command::SelectAll).expect("selects");
    let none = s.run(Command::ActivatePictureControl(Default::default())).unwrap_err();
    assert_eq!(none.code, crate::api::error::ErrorCode::Unavailable);
    let text = s
        .elements(&crate::api::services::ElementFilter::default())
        .into_iter()
        .find(|e| e.tag.as_deref() == Some("Text"))
        .expect("the text control")
        .id;
    let bad = s.run(Command::ActivatePictureControl(crate::api::command::ActivatePictureControl { element: Some(text) })).unwrap_err();
    assert_eq!(bad.code, crate::api::error::ErrorCode::InvalidArgument);
}

/// A click on a floating picture control activates it in editing mode, naming its control.
#[test]
fn floating_picture_control_activates() {
    use crate::api::input::{Modifiers, Point, PointerButton, PointerKind};
    let (mut engine, events) = {
        let mut engine = Engine::new(FontLibrary::system());
        let events = Rc::new(RefCell::new(Vec::new()));
        let sink = events.clone();
        engine.set_listener(move |_, e| sink.borrow_mut().push(e.clone()));
        (engine, events)
    };
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("a session");
    s.open(&editor_fixture("FX-PICTURE-CONTROLS.docx")).expect("opens");
    s.host().attach(|_| {});
    s.host().set_size(2000.0, 4000.0, 1.0);
    let secure = s
        .elements(&crate::api::services::ElementFilter::default())
        .into_iter()
        .find(|e| e.tag.as_deref() == Some("Placeholder:Secure.Y"))
        .expect("the floating control")
        .id;
    let k = s.presentation.zoom() * 96.0 / 72.0;
    let floats = crate::editing::inspect::inspect(&mut s.editor, "geometry", true)["geometry"]["floats"].as_array().cloned().unwrap_or_default();
    let r: Vec<f64> = floats[0]["rect"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
    let at = Point { x: (r[0] + r[2]) / 2.0 * k, y: (r[1] + r[3]) / 2.0 * k };
    events.borrow_mut().clear();
    for kind in [PointerKind::Down, PointerKind::Up] {
        s.pointer(kind, at, PointerButton::Primary, 1, Modifiers::default());
    }
    let activated: Vec<_> =
        events.borrow().iter().filter_map(|e| if let Event::PictureControlActivated(p) = e { Some(p.element.clone()) } else { None }).collect();
    assert_eq!(activated, vec![secure]);
}

/// Field shading draws on screen only: the frame changes, the exports don't.
#[test]
fn field_shading_draws_on_screen_and_never_reaches_the_output() {
    use crate::api::command::ViewOptions;
    use crate::api::services::FieldShading;
    let mut engine = Engine::new(FontLibrary::system());
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("a session");
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/FX-SHADING.docx");
    s.open(&std::fs::read(path).expect("fixture")).expect("opens");
    let shading = |s: &mut crate::api::session::Session, f| {
        s.run(Command::SetViewOptions(ViewOptions { field_shading: Some(f), ..ViewOptions::default() })).expect("sets the shading");
    };
    let export = |s: &mut crate::api::session::Session, f| undated(s.export(&ExportOptions::from(f)).expect("exports").bytes.0);
    s.host().set_size(800.0, 600.0, 1.0);
    let frame = |s: &mut crate::api::session::Session| {
        let mut px = vec![0u8; 800 * 600 * 4];
        s.host().frame_rgba(&mut px, 800, 600).expect("a frame");
        px
    };
    assert_eq!(s.state().view.field_shading, FieldShading::WhenSelected, "the default");
    shading(s, FieldShading::Never);
    let (plain, docx, pdf) = (frame(s), export(s, ExportFormat::Docx), export(s, ExportFormat::Pdf));
    shading(s, FieldShading::Always);
    assert_eq!(s.state().view.field_shading, FieldShading::Always);
    assert_ne!(frame(s), plain, "the fields are shaded");
    assert_eq!(export(s, ExportFormat::Docx), docx, "not saved");
    assert_eq!(export(s, ExportFormat::Pdf), pdf, "not exported");
    shading(s, FieldShading::Never);
    assert_eq!(frame(s), plain, "off again: the frame as before");
}

/// A session over an editor fixture, with a host, sized and drawn once, and its events.
fn hosted(name: &str) -> (Engine, crate::api::engine::SessionId, Rc<RefCell<Vec<Event>>>) {
    let mut engine = Engine::new(FontLibrary::system());
    let events = Rc::new(RefCell::new(Vec::new()));
    let sink = events.clone();
    engine.set_listener(move |_, e| sink.borrow_mut().push(e.clone()));
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("a session");
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor").join(name);
    s.open(&std::fs::read(path).expect("fixture")).expect("opens");
    s.host().attach(|_| {});
    s.host().set_size(900.0, 700.0, 1.0);
    let mut px = vec![0u8; 900 * 700 * 4];
    s.host().frame_rgba(&mut px, 900, 700).expect("a frame");
    (engine, id, events)
}

/// The element whose field code or control tag contains `text`, and the middle of its first box on screen.
fn element_point(s: &mut crate::api::session::Session, text: &str) -> (crate::docx::ElementId, crate::api::input::Point) {
    let all = crate::editing::elements::enumerate(s.editor.document(), s.editor.index(), &Default::default());
    let e = all
        .iter()
        .find(|e| {
            e.code.as_deref().is_some_and(|c| c.contains(text)) || e.props.as_ref().and_then(|p| p.tag.as_deref()).is_some_and(|t| t.contains(text))
        })
        .unwrap_or_else(|| panic!("no element {text}"))
        .id;
    let b = s.element_boxes_on_screen(e)[0];
    (e, crate::api::input::Point { x: b.left + b.width / 2.0, y: b.top + b.height / 2.0 })
}

fn hovers(events: &Rc<RefCell<Vec<Event>>>) -> Vec<Option<String>> {
    events
        .borrow()
        .iter()
        .filter_map(|e| if let Event::ElementHover(h) = e { Some(h.as_ref().map(|h| h.element.id.0.clone())) } else { None })
        .collect()
}

#[test]
fn element_hover_is_sent_once_per_element_entered_and_left() {
    use crate::api::input::{Modifiers, Point, PointerButton, PointerKind};
    let (mut engine, id, events) = hosted("FX-SHADING.docx");
    let s = engine.session(id).expect("a session");
    let (first, at_first) = element_point(s, "First");
    let (second, at_second) = element_point(s, "Second");
    let mv = |s: &mut crate::api::session::Session, at: Point| s.pointer(PointerKind::Move, at, PointerButton::Primary, 0, Modifiers::default());
    events.borrow_mut().clear();
    mv(s, at_first);
    mv(s, Point { x: at_first.x + 1.0, ..at_first });
    mv(s, at_second);
    mv(s, Point { x: 5.0, y: 5.0 });
    mv(s, Point { x: 6.0, y: 5.0 });
    let (f, g) = (merge_id(first), merge_id(second));
    assert_eq!(hovers(&events), vec![Some(f), Some(g), None], "entered, moved straight to another, left; nothing within one or outside");
    let hover = events.borrow().iter().find_map(|e| if let Event::ElementHover(Some(h)) = e { Some(h.clone()) } else { None }).expect("a hover");
    assert_eq!(hover.element.kind, crate::api::state::ElementKind::Field);
    assert!(hover.element.code.as_deref().is_some_and(|c| c.contains("MERGEFIELD First")), "{hover:?}");
    assert_eq!(hover.action, crate::api::services::PressAction::PlaceCaret);
    assert_eq!(hover.boxes.len(), 1);
}

fn merge_id(e: crate::docx::ElementId) -> String {
    crate::editing::elements::id_str(e)
}

#[test]
fn element_clicks_follow_the_press_and_cursor_overrides_apply() {
    use crate::api::command::ViewOptions;
    use crate::api::input::{Modifiers, PointerButton, PointerKind};
    use crate::api::services::{CursorOverride, CursorTarget, PressAction};
    use crate::editing::input::Cursor;
    let cursors = Rc::new(RefCell::new(Vec::new()));
    let (mut engine, id, events) = hosted("FX-CHECKBOXES.docx");
    let s = engine.session(id).expect("a session");
    let seen = cursors.clone();
    s.host().attach(move |r| {
        if let crate::api::session::HostRequest::Cursor { cursor } = r {
            seen.borrow_mut().push(cursor);
        }
    });
    let all = crate::editing::elements::enumerate(s.editor.document(), s.editor.index(), &Default::default());
    let checkbox =
        all.iter().find(|e| e.props.as_ref().is_some_and(|p| matches!(p.kind, crate::docx::ControlKind::Checkbox { .. }))).expect("a checkbox").id;
    let b = s.element_boxes_on_screen(checkbox)[0];
    let at = crate::api::input::Point { x: b.left + b.width / 2.0, y: b.top + b.height / 2.0 };
    s.run(Command::SetViewOptions(ViewOptions {
        cursor_overrides: Some(vec![CursorOverride { on: CursorTarget::Action(PressAction::Toggle), cursor: Cursor::Pointer }]),
        ..ViewOptions::default()
    }))
    .expect("sets the overrides");
    events.borrow_mut().clear();
    s.pointer(PointerKind::Move, at, PointerButton::Primary, 0, Modifiers::default());
    assert_eq!(cursors.borrow().last(), Some(&Cursor::Pointer), "the application's cursor over a checkbox");
    let hover = events.borrow().iter().find_map(|e| if let Event::ElementHover(Some(h)) = e { Some(h.clone()) } else { None }).expect("a hover");
    assert_eq!(hover.action, PressAction::Toggle);
    let revision = s.state().revision;
    for clicks in [1, 2] {
        s.pointer(PointerKind::Down, at, PointerButton::Primary, clicks, Modifiers::default());
        s.pointer(PointerKind::Up, at, PointerButton::Primary, clicks, Modifiers::default());
    }
    assert!(s.state().revision > revision, "the clicks toggled the checkbox");
    let counts: Vec<u8> = events.borrow().iter().filter_map(|e| if let Event::ElementClick(c) = e { Some(c.count) } else { None }).collect();
    assert_eq!(counts, vec![1, 2], "one click event per click, after the toggle");
    assert_eq!(s.element_boxes(&crate::api::value::ElementId(merge_id(checkbox))).expect("boxes").len(), 1);
    assert!(s.element_boxes(&crate::api::value::ElementId("control-99999".into())).is_err(), "an unknown element");
    s.run(Command::SetViewOptions(ViewOptions { cursor_overrides: Some(vec![]), ..ViewOptions::default() })).expect("clears them");
    s.pointer(PointerKind::Move, crate::api::input::Point { x: at.x + 0.5, ..at }, PointerButton::Primary, 0, Modifiers::default());
    assert_eq!(cursors.borrow().last(), Some(&Cursor::Default), "the editor's own cursor again");
}

#[test]
fn selecting_a_field_and_clicking_away_reports_the_selected_element() {
    use crate::api::input::{Modifiers, Point, PointerButton, PointerKind};
    let (mut engine, id, events) = hosted("FX-SHADING.docx");
    let s = engine.session(id).expect("a session");
    let (first, _) = element_point(s, "First");
    events.borrow_mut().clear();
    s.select_element(crate::api::value::ElementId(merge_id(first)), false).expect("selects the field");
    for kind in [PointerKind::Down, PointerKind::Up] {
        s.pointer(kind, Point { x: 5.0, y: 5.0 }, PointerButton::Primary, 1, Modifiers::default());
    }
    let selects: Vec<Option<String>> = events
        .borrow()
        .iter()
        .filter_map(|e| if let Event::ElementSelect(x) = e { Some(x.as_ref().map(|x| x.element.id.0.clone())) } else { None })
        .collect();
    assert_eq!(selects.first(), Some(&Some(merge_id(first))), "the field was selected: {selects:?}");
    assert_eq!(selects.last(), Some(&None), "and the click away left it: {selects:?}");
}

fn styles_session(engine: &mut Engine) -> crate::api::engine::SessionId {
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("a session");
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/FX-STYLES.docx");
    s.open(&std::fs::read(path).expect("fixture")).expect("opens");
    id
}

fn style_values(m: &crate::api::services::DialogModel) -> crate::api::services::StyleDialog {
    match &m.values {
        crate::api::services::DialogValues::Style(v) => v.clone(),
        other => panic!("not the style dialog: {other:?}"),
    }
}

fn style_patch(p: crate::api::services::StylePatch) -> crate::api::services::DialogPatch {
    crate::api::services::DialogPatch::Style(p)
}

#[test]
fn modify_style_shows_the_style_changes_it_on_apply_and_undoes_in_one_step() {
    use crate::api::services::{StyleOpen, StylePatch};
    use crate::api::value::{AutoOr, Color, StyleId};
    let mut engine = Engine::new(FontLibrary::system());
    let id = styles_session(&mut engine);
    let s = engine.session(id).expect("a session");
    let revision = s.state().revision;
    let m = s.dialog_open_style(StyleOpen::Modify(StyleId("Heading1".into()))).expect("opens");
    let v = style_values(&m);
    assert_eq!((v.name.as_str(), v.size, v.bold, v.quick), ("Heading 1", Points(16.0), true, true));
    assert_eq!((v.based_on.as_ref().map(|b| b.0.as_str()), v.next.as_ref().map(|n| n.0.as_str())), (Some("Normal"), Some("Normal")));
    assert!(v.description.starts_with("Font: 16 pt, Bold, Space"), "{}", v.description);
    assert!(m.fields["alignment"].enabled && !m.fields["type"].enabled, "a paragraph style being modified");
    let m = s
        .dialog_update(m, style_patch(StylePatch { size: Some(Points(20.0)), color: Some(AutoOr::Value(Color(0xC00000))), ..StylePatch::default() }))
        .expect("updates");
    let v = style_values(&m);
    assert!(v.description.starts_with("Font: 20 pt, Bold, Font color: Dark Red"), "{}", v.description);
    assert_eq!(s.state().revision, revision, "nothing applied yet");
    s.dialog_apply(m).expect("applies");
    let size =
        |s: &mut crate::api::session::Session| style_values(&s.dialog_open_style(StyleOpen::Modify(StyleId("Heading2".into()))).expect("opens")).size;
    assert_eq!(size(s), Points(13.0), "Heading 2 keeps its own size");
    let h1 = style_values(&s.dialog_open_style(StyleOpen::Modify(StyleId("Heading1".into()))).expect("opens"));
    assert_eq!((h1.size, h1.color), (Points(20.0), AutoOr::Value(Color(0xC00000))));
    s.run(Command::Undo).expect("undoes");
    let h1 = style_values(&s.dialog_open_style(StyleOpen::Modify(StyleId("Heading1".into()))).expect("opens"));
    assert_eq!(h1.size, Points(16.0), "one undo step");
}

#[test]
fn a_format_menu_dialog_changes_the_style_dialog_not_the_document() {
    use crate::api::services::{DialogKind, DialogPatch, FontPatch, FontStyle, StyleOpen};
    use crate::api::value::StyleId;
    let mut engine = Engine::new(FontLibrary::system());
    let id = styles_session(&mut engine);
    let s = engine.session(id).expect("a session");
    let revision = s.state().revision;
    let style = style_values(&s.dialog_open_style(StyleOpen::Modify(StyleId("Quote".into()))).expect("opens"));
    let font = s.dialog_open_on_style(DialogKind::Font, style).expect("opens Format > Font");
    assert!(font.on_style.is_some());
    let font = s
        .dialog_update(font, DialogPatch::Font(FontPatch { style: Some(Tri::Uniform(FontStyle::BoldItalic)), ..FontPatch::default() }))
        .expect("updates");
    assert!(s.dialog_apply(font.clone()).is_err(), "it applies to the style dialog");
    let back = style_values(&s.dialog_apply_to_style(font).expect("applies to the style dialog"));
    assert!(back.bold && back.italic, "{back:?}");
    assert!(back.description.contains("Bold"), "{}", back.description);
    assert_eq!(s.state().revision, revision, "the document didn't change");
}

#[test]
fn create_style_refuses_taken_names_and_applies_a_new_one_to_the_selection() {
    use crate::api::services::{FindQuery, StyleOpen, StylePatch};
    use crate::api::value::Order;
    let mut engine = Engine::new(FontLibrary::system());
    let id = styles_session(&mut engine);
    let s = engine.session(id).expect("a session");
    s.find(&FindQuery { text: "Callout with direct".into(), match_case: None, whole_words: None }, Order::Next);
    let m = s.dialog_open_style(StyleOpen::Create).expect("opens");
    let v = style_values(&m);
    assert_eq!((v.name.as_str(), v.short_form, v.based_on.as_ref().map(|b| b.0.as_str())), ("Style1", true, Some("Callout")));
    assert!(v.italic, "the selection's direct formatting");
    assert!(m.fields["name"].enabled && !m.fields["font"].enabled, "the short form: the name and the preview");
    for (name, why) in [("callout", "already exists"), ("Heading 4", "reserved"), ("  ", "needs a name")] {
        let named = s.dialog_update(m.clone(), style_patch(StylePatch { name: Some(name.into()), ..StylePatch::default() })).expect("updates");
        let e = s.dialog_apply(named).expect_err("refused");
        assert!(e.message.contains(why), "{name}: {}", e.message);
    }
    let named = s
        .dialog_update(m, style_patch(StylePatch { name: Some("Sidebar".into()), short_form: Some(false), ..StylePatch::default() }))
        .expect("updates");
    s.dialog_apply(named).expect("creates it");
    assert!(s.styles(None).iter().any(|st| st.name == "Sidebar"), "listed");
    assert_eq!(s.state().paragraph.style, Tri::Uniform(crate::api::value::StyleId("Sidebar".into())), "applied to the selection");
    s.run(Command::Undo).expect("undoes");
    assert!(!s.styles(None).iter().any(|st| st.name == "Sidebar"), "one undo step");
    assert_eq!(s.state().paragraph.style, Tri::Uniform(crate::api::value::StyleId("Callout".into())), "the paragraph's style back");
}

#[test]
fn renaming_a_built_in_style_adds_an_alias() {
    use crate::api::services::{StyleOpen, StylePatch};
    use crate::api::value::StyleId;
    let mut engine = Engine::new(FontLibrary::system());
    let id = styles_session(&mut engine);
    let s = engine.session(id).expect("a session");
    let m = s.dialog_open_style(StyleOpen::Modify(StyleId("Heading1".into()))).expect("opens");
    let m = s.dialog_update(m, style_patch(StylePatch { name: Some("Chapter".into()), ..StylePatch::default() })).expect("updates");
    s.dialog_apply(m).expect("applies");
    let h1 = s.styles(None).into_iter().find(|st| st.id.0 == "Heading1").expect("Heading 1");
    assert_eq!((h1.name.as_str(), h1.aliases.as_slice()), ("Heading 1", ["Chapter".to_string()].as_slice()));
}

#[test]
fn update_style_to_match_selection_takes_the_selections_direct_formatting() {
    use crate::api::services::{FindQuery, StyleOpen};
    use crate::api::value::{Order, StyleId};
    let mut engine = Engine::new(FontLibrary::system());
    let id = styles_session(&mut engine);
    let s = engine.session(id).expect("a session");
    s.find(&FindQuery { text: "Callout with direct".into(), match_case: None, whole_words: None }, Order::Next);
    assert!(!style_values(&s.dialog_open_style(StyleOpen::Modify(StyleId("Callout".into()))).expect("opens")).italic);
    s.run(Command::UpdateStyleToSelection(StyleId("Callout".into()))).expect("updates the style");
    assert!(
        style_values(&s.dialog_open_style(StyleOpen::Modify(StyleId("Callout".into()))).expect("opens")).italic,
        "the run's italic is the style's"
    );
}

#[test]
fn formatting_a_paragraph_in_an_automatically_updating_style_changes_the_style() {
    use crate::api::services::{FindQuery, StyleOpen};
    use crate::api::value::{Alignment, Order, StyleId};
    let mut engine = Engine::new(FontLibrary::system());
    let id = styles_session(&mut engine);
    let s = engine.session(id).expect("a session");
    let callout = |s: &mut crate::api::session::Session| {
        style_values(&s.dialog_open_style(StyleOpen::Modify(StyleId("Callout".into()))).expect("opens")).alignment
    };
    s.find(&FindQuery { text: "Callout: an automatically".into(), match_case: None, whole_words: None }, Order::Next);
    s.run(Command::SetAlignment(Alignment::Center)).expect("centres");
    assert_eq!(callout(s), Alignment::Center, "the style took it");
    s.find(&FindQuery { text: "Callout with direct".into(), match_case: None, whole_words: None }, Order::Next);
    assert_eq!(s.state().paragraph.alignment, Tri::Uniform(Alignment::Center), "so did the other paragraph in it");
    s.run(Command::Undo).expect("undoes");
    assert_eq!(callout(s), Alignment::Left, "one undo step");
    s.run(Command::SetParagraphStyle(StyleId("Normal".into()))).expect("applies Normal");
    assert_eq!(s.state().paragraph.style, Tri::Uniform(StyleId("Normal".into())), "applying a style isn't formatting the style");
    assert_eq!(callout(s), Alignment::Left);
}

#[test]
fn style_previews_are_pngs_of_the_size_asked_that_follow_the_dialog_and_the_appearance() {
    use crate::api::command::ViewOptions;
    use crate::api::services::{DocumentAppearance, PreviewSize, StyleOpen, StylePatch};
    use crate::api::value::StyleId;
    let mut engine = Engine::new(FontLibrary::system());
    let id = styles_session(&mut engine);
    let s = engine.session(id).expect("a session");
    let png = |p: &crate::api::services::PreviewPicture| p.png.0.starts_with(b"\x89PNG");
    let tile = s.style_tile(&StyleId("Heading1".into()), PreviewSize { width: 100.0, height: 60.0, scale: 2.0 }).expect("a tile");
    assert!(png(&tile) && (tile.width, tile.height) == (200, 120), "{} x {}", tile.width, tile.height);
    assert_ne!(tile.png, s.style_tile(&StyleId("Normal".into()), PreviewSize { width: 100.0, height: 60.0, scale: 2.0 }).expect("a tile").png);
    let m = s.dialog_open_style(StyleOpen::Modify(StyleId("Quote".into()))).expect("opens");
    let size = PreviewSize { width: 300.0, height: 160.0, scale: 1.0 };
    let before = s.dialog_preview(&m, size).expect("a preview");
    assert!(png(&before) && (before.width, before.height) == (300, 160));
    let bigger = s.dialog_update(m.clone(), style_patch(StylePatch { size: Some(Points(20.0)), ..StylePatch::default() })).expect("updates");
    assert_ne!(s.dialog_preview(&bigger, size).expect("a preview").png, before.png, "the change shows before it is applied");
    s.run(Command::SetViewOptions(ViewOptions { appearance: Some(DocumentAppearance::Dark), ..ViewOptions::default() })).expect("dark");
    assert_ne!(s.dialog_preview(&m, size).expect("a preview").png, before.png, "dark");
    assert!(s.style_tile(&StyleId("Nope".into()), size).is_err());
    assert!(s.dialog_preview(&m, PreviewSize { width: 0.0, height: 10.0, scale: 1.0 }).is_err());
    if let Ok(dir) = std::env::var("DUMP_DIR") {
        std::fs::write(format!("{dir}/tile.png"), &tile.png.0).expect("writes");
        std::fs::write(format!("{dir}/preview.png"), &s.dialog_preview(&bigger, size).expect("a preview").png.0).expect("writes");
    }
}

#[test]
fn deleting_a_style_refuses_built_ins_and_undoes_in_one_step() {
    use crate::api::value::StyleId;
    let mut engine = Engine::new(FontLibrary::system());
    let id = styles_session(&mut engine);
    let s = engine.session(id).expect("a session");
    let e = s.run(Command::DeleteStyle(StyleId("Heading1".into()))).expect_err("refused");
    assert_eq!(e.code, ErrorCode::NotAllowed);
    s.run(Command::DeleteStyle(StyleId("Callout".into()))).expect("deletes");
    assert!(!s.styles(None).iter().any(|st| st.id.0 == "Callout"));
    s.run(Command::Undo).expect("undoes");
    assert!(s.styles(None).iter().any(|st| st.id.0 == "Callout"), "one undo step");
}

#[test]
fn style_options_and_document_defaults_change_and_save() {
    use crate::api::command::{DocumentDefaults, StyleMetaChange};
    use crate::api::services::{FindQuery, StyleVisibility};
    use crate::api::value::{Order, StyleId};
    let mut engine = Engine::new(FontLibrary::system());
    let id = styles_session(&mut engine);
    let s = engine.session(id).expect("a session");
    let change = StyleMetaChange {
        id: StyleId("Draft".into()),
        visibility: Some(StyleVisibility::Show),
        priority: Some(5),
        quick: Some(true),
        locked: Some(true),
    };
    s.run(Command::SetStyleMeta(change)).expect("sets them");
    let draft = s.styles(None).into_iter().find(|st| st.id.0 == "Draft").expect("Draft");
    assert_eq!((draft.visibility, draft.priority, draft.quick, draft.locked), (StyleVisibility::Show, 5, true, true));
    s.run(Command::SetDocumentDefaults(DocumentDefaults { size: Some(Points(14.0)), ..DocumentDefaults::default() })).expect("sets them");
    s.find(&FindQuery { text: "Normal text,".into(), match_case: None, whole_words: None }, Order::Next);
    assert_eq!(s.state().formatting.size, Tri::Uniform(Points(14.0)), "text with no other size takes the default");
    let saved = s.export(&ExportOptions::Docx).expect("exports").bytes.0;
    let doc = crate::docx::read_editable(&saved).expect("reads again");
    let styles = &doc.resolver.expect("styles").styles;
    assert_eq!(styles.doc_rpr.size, Some(14.0));
    assert!(styles.get("Draft").is_some_and(|d| d.meta.locked && !d.meta.semi_hidden && d.quick));
}

#[test]
fn manage_styles_lists_sorts_and_applies_visibility_priority_and_defaults() {
    use crate::api::services::{DialogKind, DialogPatch, DialogValues, ManageStylesPatch, StyleSort, StyleVisibility};
    let mut engine = Engine::new(FontLibrary::system());
    let id = styles_session(&mut engine);
    let s = engine.session(id).expect("a session");
    let values = |m: &crate::api::services::DialogModel| match &m.values {
        DialogValues::ManageStyles(v) => v.clone(),
        other => panic!("{other:?}"),
    };
    let m = s.dialog_open(DialogKind::ManageStyles).expect("opens");
    let v = values(&m);
    assert!(v.styles.iter().any(|st| st.id.0 == "Callout") && v.selected.is_some() && !v.description.is_empty());
    let m = s
        .dialog_update(m, DialogPatch::ManageStyles(ManageStylesPatch { sort: Some(StyleSort::Alphabetical), ..ManageStylesPatch::default() }))
        .expect("sorts");
    let names: Vec<String> = values(&m).styles.iter().map(|st| st.name.to_lowercase()).collect();
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(names, sorted);
    let mut v = values(&m);
    for st in v.styles.iter_mut().filter(|st| st.id.0 == "Draft") {
        (st.visibility, st.priority) = (StyleVisibility::Show, 3);
    }
    v.defaults.size = Some(Points(13.0));
    let m = s
        .dialog_update(
            m,
            DialogPatch::ManageStyles(ManageStylesPatch { styles: Some(v.styles), defaults: Some(v.defaults), ..ManageStylesPatch::default() }),
        )
        .expect("updates");
    s.dialog_apply(m).expect("applies");
    let draft = s.styles(None).into_iter().find(|st| st.id.0 == "Draft").expect("Draft");
    assert_eq!((draft.visibility, draft.priority), (StyleVisibility::Show, 3));
    assert_eq!(s.editor.document().resolver.as_ref().and_then(|r| r.styles.doc_rpr.size), Some(13.0));
}

#[test]
fn formatting_protection_refuses_locked_styles_and_direct_formatting() {
    use crate::api::value::StyleId;
    let mut engine = Engine::new(FontLibrary::system());
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("a session");
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/FX-STYLES-LOCKED.docx");
    s.open(&std::fs::read(path).expect("fixture")).expect("opens");
    assert_eq!(s.run(Command::SetParagraphStyle(StyleId("Locked".into()))).expect_err("restricted").code, ErrorCode::NotAllowed);
    assert_eq!(s.run(Command::SetBold(true)).expect_err("restricted").code, ErrorCode::NotAllowed);
    s.run(Command::SetParagraphStyle(StyleId("Heading1".into()))).expect("an unlocked style applies");
}

#[test]
fn border_lines_and_shading_edit_the_paragraph_and_the_text() {
    use crate::api::command::{Border, BorderStyle, ParagraphBorderLines, ShadingChange, ShadingPattern, ShadingTarget};
    use crate::api::value::{AutoOr, Color, NoneOr};
    use std::io::Read;
    let mut engine = Engine::new(FontLibrary::system());
    let id = styles_session(&mut engine);
    let s = engine.session(id).expect("a session");
    let line = Border { style: BorderStyle::ThickThinSmallGap, width: Points(1.5), color: AutoOr::Value(Color(0x1F3864)) };
    s.run(Command::SetParagraphBorderLines(ParagraphBorderLines {
        top: Some(Some(line.clone())),
        bottom: Some(Some(line)),
        ..ParagraphBorderLines::default()
    }))
    .expect("borders");
    s.run(Command::SetParagraphBorderLines(ParagraphBorderLines { top: Some(None), ..ParagraphBorderLines::default() })).expect("removes one");
    let shade = |target| ShadingChange { target, fill: NoneOr::Value(Color(0xFFF2CC)), pattern: ShadingPattern::Pct20, color: AutoOr::Auto };
    s.run(Command::SetShading(shade(ShadingTarget::Paragraph))).expect("shades the paragraph");
    s.run(Command::SelectAll).expect("selects");
    s.run(Command::SetShading(shade(ShadingTarget::Text))).expect("shades the text");
    let saved = s.export(&ExportOptions::Docx).expect("saves").bytes.0;
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(saved)).expect("a package");
    let mut xml = String::new();
    z.by_name("word/document.xml").unwrap().read_to_string(&mut xml).unwrap();
    assert!(xml.contains(r#"<w:bottom w:val="thickThinSmallGap" w:sz="12" w:space="1" w:color="1F3864"/>"#), "the bottom line");
    assert!(!xml.contains("<w:top w:val=\"thickThinSmallGap\""), "the top line removed");
    assert!(xml.contains(r#"<w:shd w:val="pct20" w:color="auto" w:fill="FFF2CC"/>"#), "the shading");
    assert!(xml.matches("w:val=\"pct20\"").count() >= 2, "paragraph and run shading");
    s.run(Command::Undo).expect("undoes");
    s.run(Command::Undo).expect("undoes");
}

#[test]
fn borders_and_shading_sets_a_box_reopens_on_it_and_formats_a_style() {
    use crate::api::command::{BorderStyle, ShadingPattern};
    use crate::api::services::{BorderSetting, BordersShadingPatch, DialogKind, DialogPatch, DialogValues, StyleOpen};
    use crate::api::value::{Color, NoneOr, StyleId};
    let values = |m: &crate::api::services::DialogModel| match &m.values {
        DialogValues::BordersShading(v) => v.clone(),
        other => panic!("not Borders and Shading: {other:?}"),
    };
    let mut engine = Engine::new(FontLibrary::system());
    let id = styles_session(&mut engine);
    let s = engine.session(id).expect("a session");
    let m = s.dialog_open(DialogKind::BordersShading).expect("opens");
    assert_eq!(values(&m).setting, BorderSetting::None);
    assert_eq!(s.dialog_apply(m.clone()).expect("applies").record, None, "nothing changed, nothing applied");
    let patch = BordersShadingPatch { style: Some(BorderStyle::Double), setting: Some(BorderSetting::Box), ..BordersShadingPatch::default() };
    let m = s.dialog_update(m, DialogPatch::BordersShading(patch)).expect("updates");
    let v = values(&m);
    assert!(v.top.as_ref().is_some_and(|b| b.style == BorderStyle::Double) && v.left == v.right && v.between.is_none(), "{v:?}");
    let m =
        s.dialog_update(m, DialogPatch::BordersShading(BordersShadingPatch { top: Some(None), ..BordersShadingPatch::default() })).expect("updates");
    assert_eq!(values(&m).setting, BorderSetting::Custom, "an edge set one by one");
    let m = s
        .dialog_update(m, DialogPatch::BordersShading(BordersShadingPatch { setting: Some(BorderSetting::Box), ..BordersShadingPatch::default() }))
        .expect("updates");
    s.dialog_apply(m).expect("applies");
    let again = values(&s.dialog_open(DialogKind::BordersShading).expect("opens"));
    assert_eq!((again.setting, again.style), (BorderSetting::Box, BorderStyle::Double), "it reopens on the box");

    let style = style_values(&s.dialog_open_style(StyleOpen::Modify(StyleId("Quote".into()))).expect("opens"));
    let revision = s.state().revision;
    let b = s.dialog_open_on_style(DialogKind::BordersShading, style).expect("opens Format > Border");
    let patch =
        BordersShadingPatch { fill: Some(NoneOr::Value(Color(0xDEEAF6))), pattern: Some(ShadingPattern::Pct10), ..BordersShadingPatch::default() };
    let b = s.dialog_update(b, DialogPatch::BordersShading(patch)).expect("updates");
    let back = style_values(&s.dialog_apply_to_style(b).expect("applies to the style dialog"));
    assert!(back.pending.contains("w:fill=\"DEEAF6\"") && back.pending.contains("pct10"), "{}", back.pending);
    let f = s.dialog_open_on_style(DialogKind::Frame, back).expect("opens Format > Frame");
    let patch = crate::api::services::FramePatch {
        width_rule: Some(crate::api::command::FrameWidthRule::Exactly),
        width: Some(Points(200.0)),
        ..Default::default()
    };
    let f = s.dialog_update(f, DialogPatch::Frame(patch)).expect("updates");
    let back = style_values(&s.dialog_apply_to_style(f).expect("applies to the style dialog"));
    assert!(back.pending.contains("<w:framePr w:w=\"4000\"") && back.pending.contains("w:fill=\"DEEAF6\""), "{}", back.pending);
    assert_eq!(s.state().revision, revision, "the document didn't change");
}

#[test]
fn the_frame_dialog_moves_a_frame_saves_it_and_removes_it() {
    use crate::api::command::{FrameHorizontalAlign, FrameHorizontalRelative, FrameWrap};
    use crate::api::services::{DialogKind, DialogPatch, DialogValues, FramePatch};
    use std::io::Read;
    let frame = r#"<w:p><w:pPr><w:framePr w:w="2880" w:hSpace="180" w:wrap="around" w:vAnchor="text" w:hAnchor="text" w:x="601" w:y="1"/></w:pPr><w:r><w:t>Framed text</w:t></w:r></w:p>"#;
    let body = format!("{frame}<w:p><w:r><w:t>Body text after the frame.</w:t></w:r></w:p>");
    let open = |engine: &mut Engine, bytes: &[u8]| {
        let id = engine.create_session(SessionOptions::default());
        engine.session(id).expect("a session").open(bytes).expect("opens");
        id
    };
    let mut engine = Engine::new(FontLibrary::system());
    let id = open(&mut engine, &crate::layout::tests::shapes::bytes(&body).expect("a package"));
    let s = engine.session(id).expect("a session");
    assert!(s.dialog_open(DialogKind::Frame).is_err(), "unavailable outside a frame");
    // Find doesn't search text boxes: the caret goes in the frame's first paragraph.
    fn into_frame(s: &mut crate::api::session::Session) {
        use crate::docx::{Block, Inline};
        use crate::editing::position::{Position, StoryId};
        let doc = s.editor.document();
        let (f, p) = doc
            .sections
            .iter()
            .flat_map(|s| s.blocks.iter())
            .filter_map(|b| if let Block::Para(p) = b { Some(p) } else { None })
            .flat_map(|p| p.inlines.iter())
            .find_map(|i| match i {
                Inline::Float(f, _) if f.frame_src.is_some() => match f.text_box.as_ref()?.blocks.first()? {
                    Block::Para(q) => Some((f.id, q.id)),
                    _ => None,
                },
                _ => None,
            })
            .expect("a frame");
        let at = Position::text(StoryId::TextBox(crate::layout::FloatId(f)), crate::layout::ParaId(p), crate::layout::Unit(0));
        s.editor.set_caret_for_benchmark(crate::editing::selection::Selection::Caret(at));
    }
    into_frame(s);
    let values = |m: &crate::api::services::DialogModel| match &m.values {
        DialogValues::Frame(v) => v.clone(),
        other => panic!("not the Frame dialog: {other:?}"),
    };
    let m = s.dialog_open(DialogKind::Frame).expect("opens in the frame");
    let v = values(&m);
    assert_eq!((v.wrap, v.width, v.horizontal, v.move_with_text), (FrameWrap::Around, Points(144.0), Points(30.05), true), "{v:?}");
    let patch = FramePatch {
        wrap: Some(FrameWrap::None),
        horizontal_align: Some(Some(FrameHorizontalAlign::Center)),
        horizontal_relative: Some(FrameHorizontalRelative::Page),
        move_with_text: Some(false),
        lock_anchor: Some(true),
        ..FramePatch::default()
    };
    let m = s.dialog_update(m, DialogPatch::Frame(patch)).expect("updates");
    s.dialog_apply(m).expect("applies");
    let saved = s.export(&ExportOptions::Docx).expect("saves").bytes.0;
    let mut xml = String::new();
    zip::ZipArchive::new(std::io::Cursor::new(saved.clone()))
        .expect("a package")
        .by_name("word/document.xml")
        .unwrap()
        .read_to_string(&mut xml)
        .unwrap();
    assert!(
        xml.contains(r#"<w:framePr w:w="2880" w:hSpace="180" w:wrap="notBeside" w:vAnchor="margin" w:hAnchor="page" w:xAlign="center" w:y="1" w:anchorLock="1"/>"#),
        "{xml}"
    );
    assert_eq!(xml.matches("Framed text").count(), 1);
    let again = open(&mut engine, &saved);
    let a = engine.session(again).expect("a session");
    into_frame(a);
    let v = values(&a.dialog_open(DialogKind::Frame).expect("opens"));
    assert_eq!((v.wrap, v.horizontal_align, v.lock_anchor, v.move_with_text), (FrameWrap::None, Some(FrameHorizontalAlign::Center), true, false));

    let s = engine.session(id).expect("a session");
    let m = s.dialog_open(DialogKind::Frame).expect("opens");
    let m = s.dialog_update(m, DialogPatch::Frame(FramePatch { remove: Some(true), ..FramePatch::default() })).expect("updates");
    s.dialog_apply(m).expect("removes the frame");
    assert!(s.dialog_open(DialogKind::Frame).is_err(), "no frame now");
    let saved = s.export(&ExportOptions::Docx).expect("saves").bytes.0;
    let mut xml = String::new();
    zip::ZipArchive::new(std::io::Cursor::new(saved)).expect("a package").by_name("word/document.xml").unwrap().read_to_string(&mut xml).unwrap();
    assert!(!xml.contains("framePr") && xml.matches("Framed text").count() == 1, "{xml}");
    assert!(xml.find("Framed text") < xml.find("Body text"), "the paragraph before its anchor");
    s.run(Command::Undo).expect("undoes");
    into_frame(s);
    assert!(s.dialog_open(DialogKind::Frame).is_ok(), "the frame is back");
}

#[test]
fn the_list_dialogs_open_on_the_selections_list_and_apply_their_commands() {
    use crate::api::command::{FollowNumber, NumberFormat};
    use crate::api::services::{
        AdjustListIndentsPatch, DefineBulletPatch, DefineMultilevelPatch, DefineNumberFormatPatch, DialogKind, DialogPatch, DialogValues,
        PreviewSize, SetNumberingValuePatch,
    };
    let mut engine = Engine::new(FontLibrary::system());
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("a session");
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/FX-CUSTOM-LISTS.docx");
    s.open(&std::fs::read(path).expect("fixture")).expect("opens");
    fn caret(s: &mut crate::api::session::Session, n: usize) {
        let para = crate::editing::tests::para_id(&s.editor, n);
        let at = crate::editing::position::Position::text(crate::editing::position::StoryId::Body, para, crate::layout::Unit(0));
        s.editor.set_caret_for_benchmark(crate::editing::selection::Selection::Caret(at));
    }
    fn label(s: &crate::api::session::Session, n: usize) -> String {
        fn walk<'a>(blocks: &'a [crate::docx::Block], out: &mut Vec<&'a crate::docx::Paragraph>) {
            for b in blocks {
                match b {
                    crate::docx::Block::Para(p) => out.push(p),
                    crate::docx::Block::Table(t) => t.rows.iter().flat_map(|r| r.cells.iter()).for_each(|c| walk(&c.blocks, out)),
                }
            }
        }
        let mut paras = Vec::new();
        s.editor.document().sections.iter().for_each(|x| walk(&x.blocks, &mut paras));
        paras[n].list.as_ref().map(|l| l.text.clone()).unwrap_or_default()
    }
    // Define New Number Format on "Section 1.".
    caret(s, 6);
    let m = s.dialog_open(DialogKind::DefineNumberFormat).expect("opens");
    let DialogValues::DefineNumberFormat(v) = &m.values else { panic!("{m:?}") };
    assert_eq!((v.format, v.text.as_str()), (NumberFormat::Decimal, "Section %1."));
    let m = s
        .dialog_update(m, DialogPatch::DefineNumberFormat(DefineNumberFormatPatch { text: Some("Clause %1:".into()), ..Default::default() }))
        .expect("updates");
    let size = PreviewSize { width: 120.0, height: 60.0, scale: 1.0 };
    assert!(s.dialog_preview(&m, size).is_ok_and(|p| p.png.0.starts_with(b"\x89PNG")), "a preview");
    s.dialog_apply(m).expect("applies");
    assert_eq!((label(s, 6), label(s, 7)), ("Clause 1:".into(), "Clause 2:".into()));
    // Set Numbering Value opens on the paragraph's number.
    caret(s, 7);
    let m = s.dialog_open(DialogKind::SetNumberingValue).expect("opens");
    assert!(matches!(&m.values, DialogValues::SetNumberingValue(v) if v.value == 2), "{m:?}");
    let m = s.dialog_update(m, DialogPatch::SetNumberingValue(SetNumberingValuePatch { value: Some(7), ..Default::default() })).expect("updates");
    s.dialog_apply(m).expect("applies");
    assert_eq!(label(s, 7), "Clause 7:");
    // Adjust List Indents: a space after the number turns the tab stop off.
    caret(s, 16);
    let m = s.dialog_open(DialogKind::AdjustListIndents).expect("opens");
    let m = s
        .dialog_update(m, DialogPatch::AdjustListIndents(AdjustListIndentsPatch { follow: Some(FollowNumber::Space), ..Default::default() }))
        .expect("updates");
    assert!(!m.fields["tab"].enabled);
    s.dialog_apply(m).expect("applies");
    // Define New Multilevel List names the list.
    caret(s, 10);
    let m = s.dialog_open(DialogKind::DefineMultilevelList).expect("opens");
    assert!(matches!(&m.values, DialogValues::DefineMultilevelList(v) if v.levels.len() == 9 && v.level == 1), "{m:?}");
    let m = s
        .dialog_update(m, DialogPatch::DefineMultilevelList(DefineMultilevelPatch { list_name: Some("Legal".into()), ..Default::default() }))
        .expect("updates");
    s.dialog_apply(m).expect("applies");
    let res = s.editor.document().resolver.clone().expect("lists");
    assert!(res.numbering.abstracts.values().any(|a| a.name.as_deref() == Some("Legal")));
    // Define New Bullet outside a list makes one.
    caret(s, 0);
    let m = s.dialog_open(DialogKind::DefineBullet).expect("opens");
    let m = s
        .dialog_update(
            m,
            DialogPatch::DefineBullet(DefineBulletPatch {
                text: Some("\u{F0FC}".into()),
                font: Some(Some("Wingdings".into())),
                ..Default::default()
            }),
        )
        .expect("updates");
    s.dialog_apply(m).expect("applies");
    assert_eq!(label(s, 0), "\u{F0FC}");
    assert!(s.dialog_open(DialogKind::AdjustListIndents).is_ok());
    caret(s, 18);
    assert!(s.dialog_open(DialogKind::SetNumberingValue).is_err(), "not in a list");
}

#[test]
fn bullets_and_numbering_on_a_style_numbers_its_paragraphs() {
    use crate::api::services::{BulletsNumberingPatch, DialogKind, DialogPatch, DialogValues, ListEntryId, StyleOpen};
    use crate::api::value::StyleId;
    let mut engine = Engine::new(FontLibrary::system());
    let id = styles_session(&mut engine);
    let s = engine.session(id).expect("a session");
    let style = style_values(&s.dialog_open_style(StyleOpen::Modify(StyleId("Quote".into()))).expect("opens"));
    let m = s.dialog_open_on_style(DialogKind::BulletsNumbering, style).expect("opens Format > Numbering");
    assert!(matches!(&m.values, DialogValues::BulletsNumbering(v) if v.entry.is_none()), "{m:?}");
    let m = s
        .dialog_update(
            m,
            DialogPatch::BulletsNumbering(BulletsNumberingPatch { entry: Some(Some(ListEntryId("library:bullet:2".into()))), ..Default::default() }),
        )
        .expect("updates");
    let back = style_values(&s.dialog_apply_to_style(m).expect("applies to the style dialog"));
    assert!(back.pending.contains("<w:numPr><w:numId"), "{}", back.pending);
    s.dialog_apply(crate::services::dialogs::style_model(back)).expect("applies the style");
    let labelled = s.editor.document().sections[0].blocks.iter().any(|b| match b {
        crate::docx::Block::Para(p) => p.style.style_id.as_deref() == Some("Quote") && p.list.as_ref().is_some_and(|l| l.text == "o"),
        _ => false,
    });
    assert!(labelled, "the style's paragraphs are bulleted");
}

#[test]
fn font_characters_list_a_fonts_characters_and_draw_one() {
    use crate::api::services::PreviewSize;
    let mut engine = Engine::new(FontLibrary::system());
    let id = styles_session(&mut engine);
    let s = engine.session(id).expect("a session");
    let family =
        s.fonts.families().into_iter().find(|f| s.font_characters(f).is_ok_and(|r| r.iter().any(|c| c.first <= 'A' as u32 && 'A' as u32 <= c.last)));
    let Some(family) = family else { return };
    let ranges = s.font_characters(&family).expect("its characters");
    assert!(ranges.windows(2).all(|w| w[0].last + 1 < w[1].first), "runs, in order, apart");
    let p = s.character_picture(&family, 'A', PreviewSize { width: 32.0, height: 32.0, scale: 2.0 }).expect("a picture");
    assert_eq!((p.width, p.height), (64, 64));
    assert!(s.font_characters("No Such Font At All").is_err());
}

/// The caret in the first cell of the document's first table.
fn caret_in_table(s: &mut crate::api::session::Session) {
    let para = s
        .editor
        .document()
        .sections
        .iter()
        .flat_map(|x| x.blocks.iter())
        .find_map(|b| match b {
            crate::docx::Block::Table(t) => match t.rows[0].cells[0].blocks.first()? {
                crate::docx::Block::Para(p) => Some(p.id),
                _ => None,
            },
            _ => None,
        })
        .expect("a table");
    let at = crate::editing::position::Position::text(crate::editing::position::StoryId::Body, crate::layout::ParaId(para), crate::layout::Unit(0));
    s.editor.set_caret_for_benchmark(crate::editing::selection::Selection::Caret(at));
}

/// The first table's first-row cells' shading.
fn header_fill(s: &crate::api::session::Session) -> Option<u32> {
    s.editor.document().sections.iter().flat_map(|x| x.blocks.iter()).find_map(|b| match b {
        crate::docx::Block::Table(t) => Some(t.rows[0].cells[1].shading),
        _ => None,
    })?
}

#[test]
fn table_styles_format_their_parts_and_relay_the_tables_using_them() {
    use crate::api::services::{PreviewSize, StyleDialogType, StyleOpen, StylePatch, TableStylePart};
    use crate::api::value::{Color, StyleId};
    let mut engine = Engine::new(FontLibrary::system());
    let id = styles_session(&mut engine);
    let s = engine.session(id).expect("a session");
    assert_eq!(header_fill(s), Some(0x2E75B6));
    // Modify GridAccent's header row.
    let m = s.dialog_open_style(StyleOpen::Modify(StyleId("GridAccent".into()))).expect("opens");
    assert_eq!(style_values(&m).kind, StyleDialogType::Table);
    let m = s.dialog_update(m, style_patch(StylePatch { table_part: Some(TableStylePart::HeaderRow), ..StylePatch::default() })).expect("updates");
    let v = style_values(&m);
    assert!(v.bold && v.fill == Some(Color(0x2E75B6)), "the header row's formatting: {v:?}");
    let m = s
        .dialog_update(m, style_patch(StylePatch { fill: Some(Some(Color(0xC00000))), italic: Some(true), ..StylePatch::default() }))
        .expect("updates");
    let v = style_values(&m);
    assert!(v.pending.contains("w:fill=\"C00000\"") && v.italic, "{}", v.pending);
    assert!(
        s.dialog_preview(&m, PreviewSize { width: 200.0, height: 120.0, scale: 1.0 }).is_ok_and(|p| p.png.0.starts_with(b"\x89PNG")),
        "a sample table"
    );
    s.dialog_apply(m).expect("applies");
    assert_eq!(header_fill(s), Some(0xC00000), "the table using it changes");
    // A new table style applies to the selected table.
    caret_in_table(s);
    let m = s.dialog_open_style(StyleOpen::Create).expect("opens");
    let m = s
        .dialog_update(
            m,
            style_patch(StylePatch {
                short_form: Some(false),
                kind: Some(StyleDialogType::Table),
                name: Some("Grid Accent 2".into()),
                ..StylePatch::default()
            }),
        )
        .expect("updates");
    let m = s.dialog_update(m, style_patch(StylePatch { table_part: Some(TableStylePart::HeaderRow), ..StylePatch::default() })).expect("updates");
    let m = s
        .dialog_update(m, style_patch(StylePatch { bold: Some(true), fill: Some(Some(Color(0xD9E2F3))), ..StylePatch::default() }))
        .expect("updates");
    let m =
        s.dialog_update(m, style_patch(StylePatch { table_part: Some(TableStylePart::OddBandedRows), ..StylePatch::default() })).expect("updates");
    let m = s.dialog_update(m, style_patch(StylePatch { fill: Some(Some(Color(0xF2F2F2))), ..StylePatch::default() })).expect("updates");
    s.dialog_apply(m).expect("applies");
    assert_eq!(header_fill(s), Some(0xD9E2F3), "the table takes the new style");
    let saved = s.export(&ExportOptions::Docx).expect("saves").bytes.0;
    let doc = crate::docx::read_editable(&saved).expect("reads again");
    let st = doc.resolver.as_ref().expect("styles").styles.get("GridAccent2").expect("the new style").raw.clone();
    assert!(st.contains("w:type=\"table\"") && st.contains("firstRow") && st.contains("D9E2F3") && st.contains("band1Horz"), "{st}");
}

#[test]
fn list_styles_define_their_list_and_number_the_paragraphs() {
    use crate::api::command::NumberFormat;
    use crate::api::services::{PreviewSize, StyleDialogType, StyleOpen, StylePatch};
    use crate::api::value::StyleId;
    let mut engine = Engine::new(FontLibrary::system());
    let id = styles_session(&mut engine);
    let s = engine.session(id).expect("a session");
    let label = |s: &crate::api::session::Session| {
        s.editor.document().sections[0].blocks.iter().find_map(|b| match b {
            crate::docx::Block::Para(p) => p.list.as_ref().map(|l| l.text.clone()),
            _ => None,
        })
    };
    let m = s.dialog_open_style(StyleOpen::Create).expect("opens");
    let m = s
        .dialog_update(
            m,
            style_patch(StylePatch {
                short_form: Some(false),
                kind: Some(StyleDialogType::List),
                name: Some("Outline Steps".into()),
                ..StylePatch::default()
            }),
        )
        .expect("updates");
    let mut levels = style_values(&m).list_levels;
    assert_eq!(levels.len(), 9);
    (levels[0].format, levels[0].text) = (NumberFormat::UpperLetter, "%1)".into());
    let m = s.dialog_update(m, style_patch(StylePatch { list_levels: Some(levels), ..StylePatch::default() })).expect("updates");
    assert_eq!(style_values(&m).list_levels[0].text, "%1)", "the levels are kept until applied");
    assert!(s.dialog_preview(&m, PreviewSize { width: 120.0, height: 80.0, scale: 1.0 }).is_ok());
    s.dialog_apply(m).expect("applies");
    assert_eq!(label(s).as_deref(), Some("A)"), "the selection takes the new list style");
    // Modifying the style's levels renumbers its paragraphs.
    let m = s.dialog_open_style(StyleOpen::Modify(StyleId("OutlineSteps".into()))).expect("opens");
    let mut levels = style_values(&m).list_levels;
    assert_eq!(levels[0].text, "%1)");
    levels[0].text = "Step %1:".into();
    let m = s.dialog_update(m, style_patch(StylePatch { list_levels: Some(levels), ..StylePatch::default() })).expect("updates");
    s.dialog_apply(m).expect("applies");
    assert_eq!(label(s).as_deref(), Some("Step A:"));
    let saved = s.export(&ExportOptions::Docx).expect("saves").bytes.0;
    let doc = crate::docx::read_editable(&saved).expect("reads again");
    let res = doc.resolver.as_ref().expect("styles");
    assert!(res.numbering.abstracts.values().any(|a| a.style_link.as_deref() == Some("OutlineSteps")), "the definition is the style's");
}

#[test]
fn borders_and_shading_on_text_boxes_it_and_reopens_on_patterns() {
    use crate::api::command::{BorderStyle, ShadingChange, ShadingPattern, ShadingTarget};
    use crate::api::services::{BorderSetting, BordersShadingPatch, DialogKind, DialogPatch, DialogValues, FindQuery};
    use crate::api::value::{AutoOr, Color, NoneOr, Order};
    use std::io::Read;
    let values = |m: &crate::api::services::DialogModel| match &m.values {
        DialogValues::BordersShading(v) => v.clone(),
        other => panic!("not Borders and Shading: {other:?}"),
    };
    let mut engine = Engine::new(FontLibrary::system());
    let id = styles_session(&mut engine);
    let s = engine.session(id).expect("a session");
    s.run(Command::SetShading(ShadingChange {
        target: ShadingTarget::Paragraph,
        fill: NoneOr::Value(Color(0xFFF2CC)),
        pattern: ShadingPattern::Pct20,
        color: AutoOr::Value(Color(0x2F5496)),
    }))
    .expect("shades");
    let v = values(&s.dialog_open(DialogKind::BordersShading).expect("opens"));
    assert_eq!(
        (v.pattern, v.pattern_color, v.fill),
        (ShadingPattern::Pct20, AutoOr::Value(Color(0x2F5496)), NoneOr::Value(Color(0xFFF2CC))),
        "the pattern as written"
    );
    s.find(&FindQuery { text: "Normal text,".into(), match_case: None, whole_words: None }, Order::Next);
    let m = s.dialog_open(DialogKind::BordersShading).expect("opens");
    assert_eq!(values(&m).apply_to, ShadingTarget::Text);
    assert!(m.fields["setting"].options.as_ref().is_some_and(|o| o.len() == 3), "no custom setting for text");
    let patch = BordersShadingPatch { style: Some(BorderStyle::Double), setting: Some(BorderSetting::Shadow), ..BordersShadingPatch::default() };
    let m = s.dialog_update(m, DialogPatch::BordersShading(patch)).expect("updates");
    s.dialog_apply(m).expect("applies");
    let v = values(&s.dialog_open(DialogKind::BordersShading).expect("opens"));
    assert_eq!((v.setting, v.style), (BorderSetting::Shadow, BorderStyle::Double), "it reopens on the text's box");
    // Switching to the paragraph shows the paragraph's.
    let m = s.dialog_open(DialogKind::BordersShading).expect("opens");
    let m = s
        .dialog_update(
            m,
            DialogPatch::BordersShading(BordersShadingPatch { apply_to: Some(ShadingTarget::Paragraph), ..BordersShadingPatch::default() }),
        )
        .expect("updates");
    assert_eq!(values(&m).setting, BorderSetting::None);
    let saved = s.export(&ExportOptions::Docx).expect("saves").bytes.0;
    let mut xml = String::new();
    zip::ZipArchive::new(std::io::Cursor::new(saved)).expect("a package").by_name("word/document.xml").unwrap().read_to_string(&mut xml).unwrap();
    assert!(xml.contains(r#"<w:bdr w:val="double" w:sz="4" w:space="0" w:color="auto" w:shadow="1"/>"#), "the text's border");
}

#[test]
fn page_borders_set_reopen_remove_and_keep_what_they_dont_change() {
    use crate::api::command::{Border, BorderStyle, Margins, PageBorderScope, PageBordersChange};
    use crate::api::services::{BorderSetting, BordersShadingPatch, BordersTab, DialogKind, DialogPatch, DialogValues};
    use crate::api::value::{AutoOr, Color};
    use std::io::Read;
    let document = |s: &mut crate::api::session::Session| {
        let saved = s.export(&ExportOptions::Docx).expect("saves").bytes.0;
        let mut xml = String::new();
        zip::ZipArchive::new(std::io::Cursor::new(saved)).expect("a package").by_name("word/document.xml").unwrap().read_to_string(&mut xml).unwrap();
        xml
    };
    let values = |m: &crate::api::services::DialogModel| match &m.values {
        DialogValues::BordersShading(v) => v.clone(),
        other => panic!("not Borders and Shading: {other:?}"),
    };
    let mut engine = Engine::new(FontLibrary::system());
    let id = styles_session(&mut engine);
    let s = engine.session(id).expect("a session");
    let line = Border { style: BorderStyle::Wave, width: Points(0.75), color: AutoOr::Value(Color(0x7030A0)) };
    let all = PageBordersChange {
        top: Some(Some(line.clone())),
        bottom: Some(Some(line.clone())),
        left: Some(Some(line.clone())),
        right: Some(Some(line)),
        apply_to: PageBorderScope::WholeDocument,
        ..PageBordersChange::default()
    };
    s.run(Command::SetPageBorders(all)).expect("sets them");
    let xml = document(s);
    assert!(xml.contains(r#"<w:pgBorders w:offsetFrom="page"><w:top w:val="wave" w:sz="6" w:space="24" w:color="7030A0"/>"#), "{xml}");
    let m = s.dialog_open(DialogKind::BordersShading).expect("opens");
    assert_eq!(values(&m).page_setting, BorderSetting::Box, "the Page Border tab opens on them");
    let patch = BordersShadingPatch { tab: Some(BordersTab::PageBorder), page_setting: Some(BorderSetting::None), ..BordersShadingPatch::default() };
    let m = s.dialog_update(m, DialogPatch::BordersShading(patch)).expect("updates");
    s.dialog_apply(m).expect("applies");
    assert!(!document(s).contains("pgBorders"), "removed");
    s.run(Command::Undo).expect("undoes");
    let raw = document(s);
    s.run(Command::SetMargins(Margins { top: Some(Points(90.0)), ..Margins::default() })).expect("a margin");
    let after = document(s);
    let pg = |x: &str| x[x.find("<w:pgBorders").expect("borders")..x.find("</w:pgBorders>").expect("borders")].to_string();
    assert_eq!(pg(&after), pg(&raw), "another section edit keeps them as written");
}

#[test]
fn a_table_styles_format_menu_sets_part_borders_properties_and_banding() {
    use crate::api::command::{BorderStyle, ShadingPattern, ShadingTarget};
    use crate::api::services::{
        BandingPatch, BorderSetting, BordersShadingPatch, DialogKind, DialogPatch, DialogValues, StyleOpen, StylePatch, TablePropertiesPatch,
        TableStylePart,
    };
    use crate::api::value::{CellAlignment, Color, HorizontalAlignment, NoneOr, StyleId};
    let mut engine = Engine::new(FontLibrary::system());
    let id = styles_session(&mut engine);
    let s = engine.session(id).expect("a session");
    let m = s.dialog_open_style(StyleOpen::Modify(StyleId("GridAccent".into()))).expect("opens");
    let m = s.dialog_update(m, style_patch(StylePatch { table_part: Some(TableStylePart::HeaderRow), ..StylePatch::default() })).expect("updates");
    let style = style_values(&m);
    let b = s.dialog_open_on_style(DialogKind::BordersShading, style.clone()).expect("opens Format > Border");
    let DialogValues::BordersShading(v) = &b.values else { panic!("{b:?}") };
    assert_eq!((v.apply_to, v.fill), (ShadingTarget::Cell, NoneOr::Value(Color(0x2E75B6))), "the header row's cells");
    assert!(b.fields["insideVertical"].enabled);
    let patch = BordersShadingPatch {
        style: Some(BorderStyle::Double),
        setting: Some(BorderSetting::Box),
        pattern: Some(ShadingPattern::Pct20),
        ..BordersShadingPatch::default()
    };
    let b = s.dialog_update(b, DialogPatch::BordersShading(patch)).expect("updates");
    let style = style_values(&s.dialog_apply_to_style(b).expect("applies to the style dialog"));
    let first = &style.pending[style.pending.find("w:type=\"firstRow\"").expect("the header row")..];
    let first = &first[..first.find("</w:tblStylePr>").unwrap()];
    assert!(first.contains("<w:tcBorders><w:top w:val=\"double\"") && first.contains("w:val=\"pct20\""), "{first}");
    let t = s.dialog_open_on_style(DialogKind::TableProperties, style.clone()).expect("opens Format > Table Properties");
    let patch = TablePropertiesPatch {
        alignment: Some(HorizontalAlignment::Center),
        cell_alignment: Some(Tri::Uniform(CellAlignment::Center)),
        ..Default::default()
    };
    let t = s.dialog_update(t, DialogPatch::TableProperties(patch)).expect("updates");
    let style = style_values(&s.dialog_apply_to_style(t).expect("applies"));
    assert!(style.pending.contains("<w:jc w:val=\"center\"/>") && style.pending.contains("<w:vAlign w:val=\"center\"/>"), "{}", style.pending);
    let band = s.dialog_open_on_style(DialogKind::Banding, style.clone()).expect("opens Format > Banding");
    assert!(matches!(&band.values, DialogValues::Banding(v) if v.row_band == 1));
    let band = s.dialog_update(band, DialogPatch::Banding(BandingPatch { row_band: Some(2), ..Default::default() })).expect("updates");
    let style = style_values(&s.dialog_apply_to_style(band).expect("applies"));
    assert!(style.pending.contains("<w:tblStyleRowBandSize w:val=\"2\"/>"), "{}", style.pending);
    let quote = style_values(&s.dialog_open_style(StyleOpen::Modify(StyleId("Quote".into()))).expect("opens"));
    assert!(s.dialog_open_on_style(DialogKind::Banding, quote).is_err(), "banding is a table style's");
}

/// Window frames are drawn on the GPU from the pictures the CPU screen draws: the two agree (a pixel may differ by 2
/// in each channel; beyond that, at most 0.02% of the pixels, as twip-graphics' own comparisons allow), on the first
/// frame and on one scrolled, whose pages the GPU draws from what it kept; and after the device is lost, on the new
/// device the next frame makes. Nothing to compare without a GPU. (One test: the GPU device is the process's.)
#[test]
fn gpu_frames_draw_as_cpu_frames() {
    for (fixture, lose) in [("objects.docx", false), ("docx-feature-showcase.docx", true)] {
        let mut engine = Engine::new(FontLibrary::system_recommended());
        let id = engine.create_session(SessionOptions::default());
        let s = engine.session(id).expect("a session");
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(fixture);
        s.open(&std::fs::read(path).expect("the fixture")).expect("opens");
        s.host().set_graphics_screen(true);
        let (w, h) = (700u32, 500u32);
        s.host().set_size(f64::from(w), f64::from(h), 1.5);
        let (mut cpu, mut gpu) = (vec![0u8; (w * h * 4) as usize], vec![0u8; (w * h * 4) as usize]);
        for scroll in [0.0, 37.0, 37.0] {
            if lose && scroll > 0.0 {
                s.host().lose_gpu_device();
            }
            s.host().set_scroll(0.0, scroll);
            s.host().frame_rgba(&mut cpu, w, h).expect("a frame");
            if s.host().frame_gpu_rgba(&mut gpu, w, h).is_err() {
                eprintln!("no GPU: skipped");
                return;
            }
            let diffs = cpu.chunks(4).zip(gpu.chunks(4)).map(|(a, b)| a.iter().zip(b).map(|(x, y)| x.abs_diff(*y)).max().unwrap_or(0));
            let (over, worst) = diffs.fold((0usize, 0u8), |(n, m), d| (n + usize::from(d > 2), m.max(d)));
            eprintln!("{fixture} at {scroll}: {over} pixels over 2, worst {worst}");
            assert!(over <= (w * h) as usize / 5000, "{fixture} at {scroll}: {over} pixels differ by more than 2 (worst {worst})");
        }
    }
}
