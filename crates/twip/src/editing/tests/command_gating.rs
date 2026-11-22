//! Command states against what running the command does

use super::super::selection::{CellRef, CellShape, Selection};
use crate::api::command::{Command, CommandId as C};
use crate::api::engine::{Engine, SessionOptions};
use crate::api::fonts::FontLibrary;
use crate::api::session::Session;
use crate::api::state::LoadState;

fn engine() -> Engine {
    Engine::new(FontLibrary::system_recommended())
}

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor").join(name)).expect("the fixture")
}

fn on(s: &Session, id: C) -> bool {
    s.state().commands[&id].enabled
}

fn select_cells(s: &mut Session) {
    s.with_editor(|e| {
        let table = e
            .document()
            .sections
            .iter()
            .flat_map(|s| &s.blocks)
            .find_map(|b| if let crate::docx::Block::Table(t) = b { Some(crate::layout::TableId(t.id)) } else { None });
        let table = table.expect("a table");
        e.set_selection(Selection::Cells { table, anchor: CellRef { row: 0, col: 0 }, focus: CellRef { row: 1, col: 1 }, shape: CellShape::Rect });
    });
}

#[test]
fn a_cell_selection_keeps_the_table_commands_and_the_page() {
    let mut engine = engine();
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("a session");
    s.open(&fixture("FX-TBL-BASIC.docx")).expect("opens");
    select_cells(s);
    for c in [C::DeleteRows, C::DeleteColumns, C::Delete, C::InsertText, C::MoveToCell, C::SetBold] {
        assert!(on(s, c), "{c:?}");
    }
    assert!(s.state().page.size.width.0 > 0.0, "the page is the section's");
    assert!(!s.state().ruler.horizontal.columns.is_empty(), "so is the ruler");
    let rows = |s: &mut Session| {
        s.with_editor(|e| {
            e.document()
                .sections
                .iter()
                .flat_map(|s| &s.blocks)
                .find_map(|b| if let crate::docx::Block::Table(t) = b { Some(t.rows.len()) } else { None })
        })
    };
    let before = rows(s).expect("a table");
    s.run(Command::DeleteRows).expect("deletes the rows");
    assert_eq!(rows(s), Some(before - 2), "two rows went");
}

#[test]
fn bold_on_a_cell_selection_bolds_the_cells_text() {
    let mut engine = engine();
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("a session");
    s.open(&fixture("FX-TBL-BASIC.docx")).expect("opens");
    select_cells(s);
    s.run(Command::SetBold(true)).expect("bolds");
    assert_eq!(s.state().formatting.bold, crate::api::state::Tri::Uniform(true));
}

#[test]
fn context_commands_are_disabled_where_they_cannot_apply() {
    let mut engine = engine();
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("a session");
    s.run(Command::InsertText("plain".into())).expect("types");
    for c in [
        C::RestartNumbering,
        C::ContinueNumbering,
        C::SetNumberingValue,
        C::AdjustListIndents,
        C::SetFrame,
        C::RemoveFrame,
        C::ClearTabStops,
        C::SetHeaderFooterLinked,
    ] {
        assert!(!on(s, c), "{c:?} in a plain paragraph of the first section");
    }
    assert!(s.run(Command::RestartNumbering).is_err(), "and refused");
    s.run(Command::SetList(Some(crate::api::command::ListKind::Numbered))).expect("a list");
    assert!(on(s, C::RestartNumbering), "in a list");
    s.run(Command::AddTabStop(crate::api::command::AddTabStop { position: crate::api::value::Points(72.0), alignment: None, leader: None }))
        .expect("a stop");
    assert!(on(s, C::ClearTabStops), "with a stop");
    assert!(s.run(Command::ClearTabStops).is_ok());
}

#[test]
fn notes_and_comments_are_disabled_in_a_header() {
    let mut engine = engine();
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("a session");
    assert!(on(s, C::AddComment) && on(s, C::InsertFootnote), "the body takes them");
    s.run(Command::EditHeader).expect("opens the header");
    for c in [C::AddComment, C::InsertFootnote, C::InsertEndnote, C::InsertNote] {
        assert!(!on(s, c), "{c:?} in a header");
    }
}

#[test]
fn a_footnote_in_a_header_is_refused() {
    let mut engine = engine();
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("a session");
    s.run(Command::EditHeader).expect("opens the header");
    let out = s.with_editor(|e| e.execute(super::super::commands::Command::InsertFootnote, super::fonts()));
    assert!(matches!(out, super::super::Outcome::Refused(_)), "{out:?}");
}

#[test]
fn alignment_with_a_picture_selected_aligns_its_paragraph() {
    let mut engine = engine();
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("a session");
    s.run(Command::InsertText("Shapes".into())).expect("types");
    let preset = crate::api::drawing::ShapePreset::from_name("star5").expect("a preset");
    s.run(Command::InsertShape(crate::api::drawing::ShapeInsert { preset, width: None, height: None })).expect("inserts");
    assert!(s.state().selection.object.is_some(), "an object is selected");
    assert!(on(s, C::SetAlignment));
    let rev = s.state().revision;
    s.run(Command::SetAlignment(crate::api::value::Alignment::Center)).expect("aligns");
    assert_ne!(s.state().revision, rev, "the paragraph changed");
    assert_eq!(s.state().paragraph.alignment, crate::api::state::Tri::Uniform(crate::api::value::Alignment::Center));
}

#[test]
fn the_correction_mark_goes_when_an_edit_moves_its_text() {
    let mut engine = engine();
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("a session");
    for c in "teh ".chars() {
        s.text(&c.to_string());
    }
    assert!(on(s, C::RevertAutoCorrect), "right after the correction");
    s.run(Command::MoveSelection(crate::api::command::MoveSelection {
        by: crate::api::command::MoveUnit::Document,
        direction: crate::api::value::Direction::Backward,
        extend: None,
    }))
    .expect("moves");
    s.run(Command::InsertText("xx".into())).expect("inserts");
    assert!(!on(s, C::RevertAutoCorrect), "the correction's text moved");
    assert!(s.run(Command::RevertAutoCorrect).is_err());
    s.run(Command::SelectAll).expect("selects");
    assert_eq!(s.state().selection.text.as_deref(), Some("xxThe "), "nothing reverted");
}

#[test]
fn paste_is_enabled_only_with_something_to_paste() {
    let mut engine = engine();
    let a = engine.create_session(SessionOptions::default());
    let b = engine.create_session(SessionOptions::default());
    let s = engine.session(a).expect("a session");
    assert!(!on(s, C::Paste), "nothing copied");
    assert!(s.run(Command::Paste(Default::default())).is_err());
    s.run(Command::InsertText("copied".into())).expect("types");
    s.run(Command::SelectAll).expect("selects");
    s.copy(false).expect("copies");
    let t = engine.session(b).expect("another session");
    t.run(Command::Paste(Default::default())).expect("pastes the other session's copy");
    assert!(on(t, C::Paste));
    t.run(Command::SelectAll).expect("selects");
    assert_eq!(t.state().selection.text.as_deref(), Some("copied"));
}

#[test]
fn load_is_laying_out_while_pages_are_incomplete() {
    let mut engine = engine();
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("a session");
    s.open(&fixture("FX-outline.docx")).expect("opens");
    assert!(!s.state().pages.complete, "layout is still going");
    assert_eq!(s.state().load, LoadState::LayingOut);
    s.with_editor(|e| e.complete(super::fonts()));
    assert_eq!(s.state().load, LoadState::Ready);
}

#[test]
fn the_format_painter_copies_a_runs_and_its_paragraphs_formatting() {
    use crate::api::state::Tri;
    use crate::api::value::Alignment;
    let mut engine = engine();
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("a session");
    s.run(Command::InsertText("source".into())).expect("types");
    s.run(Command::SelectAll).expect("selects");
    s.run(Command::SetBold(true)).expect("bolds");
    s.run(Command::SetAlignment(Alignment::Center)).expect("centres");
    s.run(Command::MoveSelection(crate::api::command::MoveSelection {
        by: crate::api::command::MoveUnit::Document,
        direction: crate::api::value::Direction::Forward,
        extend: None,
    }))
    .expect("moves");
    assert!(!on(s, C::PasteFormatting), "nothing copied yet");
    s.run(Command::CopyFormatting).expect("copies");
    assert!(on(s, C::PasteFormatting));
    s.run(Command::InsertParagraph).expect("a paragraph");
    s.run(Command::SetBold(false)).expect("plain");
    s.run(Command::SetAlignment(Alignment::Left)).expect("left");
    s.run(Command::InsertText("target".into())).expect("types");
    let rev = s.state().revision;
    s.run(Command::PasteFormatting).expect("pastes");
    assert_ne!(s.state().revision, rev);
    assert_eq!(s.state().formatting.bold, Tri::Uniform(true), "the word at the caret takes the run's formatting");
    assert_eq!(s.state().paragraph.alignment, Tri::Uniform(Alignment::Center), "and the paragraph the paragraph's");
    s.run(Command::Undo).expect("undoes");
    assert_eq!(s.state().paragraph.alignment, Tri::Uniform(Alignment::Left), "in one step");
}
