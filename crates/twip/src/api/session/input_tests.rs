//! Input: IME composition, the platform's keys and pasting from other applications

use std::cell::RefCell;
use std::rc::Rc;

use crate::api::command::Command;
use crate::api::engine::{Engine, SessionOptions};
use crate::api::event::Event;
use crate::api::fonts::FontLibrary;
use crate::api::input::{CompositionPhase, Key, Modifiers};
use crate::api::session::Session;
use crate::document::paragraph::{ListId, ListRef};

fn text(s: &mut Session) -> String {
    s.with_editor(|e| {
        e.document()
            .sections
            .iter()
            .flat_map(|s| &s.blocks)
            .filter_map(|b| if let crate::docx::Block::Para(p) = b { Some(crate::docx::fields::text(p)) } else { None })
            .collect::<Vec<_>>()
            .join("\n")
    })
}

#[test]
fn a_composition_shows_at_the_caret_unrecorded_and_commits_as_typing() {
    let mut engine = Engine::new(FontLibrary::system_recommended());
    let events: Rc<RefCell<Vec<Event>>> = Rc::default();
    let sink = events.clone();
    engine.set_listener(move |_, e| sink.borrow_mut().push(e.clone()));
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("a session");
    s.run(Command::InsertText("ab".into())).expect("types");
    let undo = s.state().history.undo_label.clone();
    events.borrow_mut().clear();
    s.composition(CompositionPhase::Start, "", 0);
    s.composition(CompositionPhase::Update, "n", 1);
    s.composition(CompositionPhase::Update, "ni", 1);
    assert_eq!(text(s), "abni", "the composition shows");
    assert!(s.with_editor(|e| e.view(0..1).composition.len()) > 0, "underlined");
    let caret = s.with_editor(|e| e.selection().focus().and_then(|p| p.para_offset()).map(|(_, u)| u.0));
    assert_eq!(caret, Some(3), "the caret is where the composition's cursor is");
    assert_eq!(s.state().history.undo_label, undo, "the history doesn't hold it");
    assert!(!events.borrow().iter().any(|e| matches!(e, Event::DocumentChanged(_))), "nor do the records");
    s.composition(CompositionPhase::End, "\u{4F60}", 1);
    assert_eq!(text(s), "ab\u{4F60}", "the end commits its text");
    assert!(events.borrow().iter().any(|e| matches!(e, Event::DocumentChanged(_))), "as typing");
    s.run(Command::Undo(Default::default())).expect("undoes");
    assert_eq!(text(s), "ab", "one undo takes the committed text");
}

#[test]
fn a_cancelled_composition_leaves_the_text_as_it_was() {
    let mut engine = Engine::new(FontLibrary::system_recommended());
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("a session");
    s.run(Command::InsertText("ab".into())).expect("types");
    s.run(Command::SelectAll).expect("selects");
    s.composition(CompositionPhase::Update, "xy", 2);
    assert_eq!(text(s), "xy", "the composition replaces the selection while it shows");
    s.composition(CompositionPhase::End, "", 0);
    assert_eq!(text(s), "ab");
    assert!(!s.state().history.can_redo, "nothing to redo");
}

#[test]
fn a_composition_cancelled_after_read_only_comes_on_still_goes() {
    let mut engine = Engine::new(FontLibrary::system_recommended());
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("a session");
    s.run(Command::InsertText("ab".into())).expect("types");
    s.composition(CompositionPhase::Update, "n", 1);
    assert_eq!(text(s), "abn");
    s.set_read_only(true);
    s.composition(CompositionPhase::End, "", 0);
    assert_eq!(text(s), "ab", "the composition is taken out");
}

#[test]
fn a_cancelled_composition_keeps_what_can_be_redone() {
    let mut engine = Engine::new(FontLibrary::system_recommended());
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("a session");
    s.run(Command::InsertText("ab".into())).expect("types");
    s.run(Command::Undo(Default::default())).expect("undoes");
    s.composition(CompositionPhase::Update, "n", 1);
    s.composition(CompositionPhase::Update, "ni", 2);
    s.composition(CompositionPhase::End, "", 0);
    assert_eq!(text(s), "");
    assert!(s.state().history.can_redo, "the typing can be redone");
    s.run(Command::Redo(Default::default())).expect("redoes");
    assert_eq!(text(s), "ab");
}

fn caret(s: &mut Session) -> Option<u32> {
    s.with_editor(|e| e.selection().focus().and_then(|p| p.para_offset()).map(|(_, u)| u.0))
}

#[test]
fn a_mac_session_takes_the_mac_keys() {
    let mut engine = Engine::new(FontLibrary::system_recommended());
    let id = engine.create_session(SessionOptions { mac: true, ..SessionOptions::default() });
    let s = engine.session(id).expect("a session");
    s.run(Command::InsertText("one two three".into())).expect("types");
    s.key(Key::ArrowLeft, Modifiers { meta: true, ..Modifiers::default() });
    assert_eq!(caret(s), Some(0), "\u{2318}\u{2190} goes to the line's start");
    s.key(Key::ArrowRight, Modifiers { alt: true, ..Modifiers::default() });
    assert_eq!(caret(s), Some(4), "\u{2325}\u{2192} goes by word");
    s.key(Key::Backspace, Modifiers { alt: true, ..Modifiers::default() });
    assert_eq!(text(s), "two three", "\u{2325}Backspace deletes a word");
    let other = engine.create_session(SessionOptions::default());
    let t = engine.session(other).expect("a session");
    t.run(Command::InsertText("one two".into())).expect("types");
    t.key(Key::Backspace, Modifiers { alt: true, ..Modifiers::default() });
    assert_eq!(text(t), "one two", "elsewhere Alt+Backspace edits nothing");
}

#[test]
fn html_from_another_application_pastes_formatted() {
    let mut engine = Engine::new(FontLibrary::system_recommended());
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("a session");
    let html = "<p><b>bold</b> plain</p><ul><li>one</li><li>two</li></ul><table><tr><td>a</td><td>b</td></tr></table>";
    s.paste(
        &crate::api::input::ClipboardData { text: "bold plain\none\ntwo\na\tb".into(), html: Some(html.into()), internal: None, image: None },
        false,
    );
    let (bold, lists, tables) = s.with_editor(|e| {
        let blocks = &e.document().sections[0].blocks;
        let bold = blocks.iter().any(|b| {
            matches!(b, crate::docx::Block::Para(p) if p.inlines.iter().any(|i| matches!(i, crate::docx::Inline::Text(t, r) if t == "bold" && r.bold)))
        });
        let lists = blocks.iter().filter(|b| matches!(b, crate::docx::Block::Para(p) if p.style.num.is_some_and(|ListRef { list: ListId(n), level: _ }| n != 0))).count();
        let tables = blocks.iter().filter(|b| matches!(b, crate::docx::Block::Table(_))).count();
        (bold, lists, tables)
    });
    assert!(bold, "the bold run stays bold");
    assert_eq!(lists, 2, "the items are a list");
    assert_eq!(tables, 1, "the table is a table");
    s.run(Command::Undo(Default::default())).expect("undoes");
    assert_eq!(text(s), "", "as one step");
}

#[test]
fn a_paste_takes_this_sessions_newest_copy() {
    let mut engine = Engine::new(FontLibrary::system_recommended());
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("a session");
    s.run(Command::InsertText("alpha beta".into())).expect("types");
    let find = |text: &str| crate::api::services::FindQuery { text: text.into(), ..Default::default() };
    s.find(&find("alpha"), crate::api::value::Order::Next).expect("finds");
    assert_eq!(s.copy(false).map(|c| c.text).as_deref(), Some("alpha"), "the engine's clipboard");
    s.find(&find("beta"), crate::api::value::Order::Next).expect("finds");
    s.run(Command::Copy).expect("copies");
    s.run(Command::Paste(Default::default())).expect("pastes");
    assert_eq!(text(s), "alpha beta", "the newer copy pasted over itself");
}

#[test]
fn a_pasted_heading_or_list_item_keeps_its_paragraph() {
    let mut engine = Engine::new(FontLibrary::system_recommended());
    for (html, text) in [("<h1>Title</h1>", "Title"), ("<ul><li>one</li></ul>", "one")] {
        let id = engine.create_session(SessionOptions::default());
        let s = engine.session(id).expect("a session");
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/editor/FX-EMPTY.docx");
        s.open(&std::fs::read(path).expect("the fixture")).expect("opens");
        s.paste(&crate::api::input::ClipboardData { text: text.into(), html: Some(html.into()), internal: None, image: None }, false);
        let (style, listed) = s.with_editor(|e| {
            let p = e.document().sections[0]
                .blocks
                .iter()
                .find_map(|b| match b {
                    crate::docx::Block::Para(p) if crate::docx::fields::text(p) == text => Some(p),
                    _ => None,
                })
                .expect("the pasted paragraph");
            (p.style.style_id.clone(), p.style.num.is_some_and(|ListRef { list: ListId(n), level: _ }| n != 0))
        });
        if text == "Title" {
            assert_eq!(style.as_deref(), Some("Heading1"), "{html}");
        } else {
            assert!(listed, "{html}: in a list");
        }
    }
}

#[test]
fn a_picture_pasted_over_a_picture_controls_picture_fills_it_under_forms_protection() {
    use crate::api::input::{ClipboardData, ClipboardImage};
    use crate::editing::selection::{ObjectRef, Selection};
    let fixture = |p: &str| std::fs::read(format!("{}/tests/fixtures/{p}", env!("CARGO_MANIFEST_DIR"))).expect("a fixture");
    let mut engine = Engine::new(FontLibrary::system_recommended());
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("a session");
    s.open(&fixture("editor/FX-PICTURE-CONTROLS.docx")).expect("opens");
    let forms =
        crate::api::command::DocumentProtection { kind: Some(crate::api::command::ProtectionKind::Forms), enforce: Some(true), ..Default::default() };
    s.run(Command::SetDocumentProtection(forms)).expect("protects");
    let pictures = |s: &mut Session| text(s).matches(crate::editing::text::OBJECT).count();
    let count = pictures(s);
    // The first control's picture
    s.with_editor(|e| {
        let (para, offset) = e
            .document()
            .sections
            .iter()
            .flat_map(|s| &s.blocks)
            .find_map(|b| {
                let crate::docx::Block::Para(p) = b else { return None };
                let mut at = 0u32;
                p.inlines.iter().find_map(|i| {
                    let here = at;
                    at += i.units() as u32;
                    matches!(i, crate::docx::Inline::Picture(..)).then_some((crate::layout::ParaId(p.id), crate::layout::Unit(here)))
                })
            })
            .expect("a picture");
        e.set_caret_for_benchmark(Selection::Object(ObjectRef::Inline { para, offset }));
    });
    let png = ClipboardImage { mime: "image/png".into(), data: crate::api::value::Bytes(fixture("images/square-96.png")) };
    let data = ClipboardData { image: Some(png), ..Default::default() };
    s.paste_with(&data, None).expect("fills the control where forms are filled in");
    assert_eq!(s.state().history.undo_label.as_deref(), Some("Paste"));
    assert_eq!(pictures(s), count, "the control's one picture");
    // Outside a control, a picture is refused there
    s.with_editor(|e| {
        e.set_caret_for_benchmark(Selection::Caret(crate::editing::position::Position::text(
            crate::editing::position::StoryId::Body,
            crate::layout::ParaId(
                e.document().sections[0]
                    .blocks
                    .iter()
                    .find_map(|b| if let crate::docx::Block::Para(p) = b { Some(p.id) } else { None })
                    .expect("a paragraph"),
            ),
            crate::layout::Unit(0),
        )))
    });
    let refused = s.paste_with(&data, None).expect_err("refused outside the controls");
    assert_eq!(refused.code, crate::api::error::ErrorCode::NotAllowed);
}

#[test]
fn alt_shift_o_and_ctrl_alt_v_ask_the_host_for_its_dialogs() {
    use crate::api::event::{HostCommand, KeyOutcome};
    let mut engine = Engine::new(FontLibrary::system_recommended());
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("a session");
    let mods = |ctrl, shift, alt| Modifiers { ctrl, shift, alt, meta: false };
    assert_eq!(s.key(Key::KeyO, mods(false, true, true)), KeyOutcome::HostCommand(HostCommand::MarkTableOfContentsEntry));
    assert_eq!(s.key(Key::KeyV, mods(true, false, true)), KeyOutcome::HostCommand(HostCommand::PasteSpecial));
}
