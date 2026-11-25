//! Undo history: grouping, eviction, budget, refused batches

use super::super::commands::{Command, Dir, Outcome};
use super::super::history::{History, MIN_STEPS, SelectionRecord, Transaction};
use super::super::position::{Position, StoryId};
use super::super::selection::Selection;
use super::super::{Editor, Mode};
use super::{blank_with, del, fonts, open, texts};
use crate::api::command::{DropCap, DropCapPosition, InsertSymbol, StyleMetaChange};

fn type_text(e: &mut Editor, s: &str) {
    for c in s.chars() {
        e.execute(Command::InsertText(c.to_string()), fonts());
    }
}

fn opened() -> Editor {
    let mut e = open("FX-TEXT.docx");
    e.complete(fonts());
    e
}

#[test]
fn a_refused_batch_that_opened_a_header_leaves_the_last_edit_alone() {
    let mut e = opened();
    e.execute(Command::InsertText("abc".into()), fonts());
    let (undo, redo, before) = (e.history().undo_depth(), e.history().redo_depth(), texts(&e));
    e.begin_batch(None);
    e.execute(Command::EditHeader, fonts());
    let drop = Command::SetDropCap(DropCap { position: DropCapPosition::Dropped, font: None, lines: Some(3), distance: None });
    assert!(matches!(e.execute(drop, fonts()), Outcome::Refused(_)), "a drop cap on an empty header paragraph");
    e.end_batch(false, fonts());
    e.execute(Command::CloseHeaderFooter, fonts());
    assert_eq!(texts(&e), before);
    assert_eq!((e.history().undo_depth(), e.history().redo_depth()), (undo, redo));
}

#[test]
fn leaving_a_provisional_header_in_read_only_keeps_its_transaction() {
    let mut e = opened();
    e.execute(Command::EditHeader, fonts());
    let n = e.history().undo_depth();
    e.set_mode(Mode::ReadOnly);
    e.execute(Command::CloseHeaderFooter, fonts());
    assert_eq!(e.history().undo_depth(), n);
    e.set_mode(Mode::Editing);
    assert!(matches!(e.execute(Command::Undo, fonts()), Outcome::Changed(_)), "the header can still be undone");
}

#[test]
fn a_batch_refused_in_read_only_is_still_reverted() {
    let mut e = Editor::open(&blank_with("abc"), fonts()).expect("opens");
    let before = texts(&e);
    e.begin_batch(None);
    e.execute(Command::InsertText("x".into()), fonts());
    e.set_mode(Mode::ReadOnly);
    e.end_batch(false, fonts());
    assert_eq!(texts(&e), before);
}

fn tx(bytes: usize) -> Transaction {
    let at = Position::text(StoryId::Body, crate::layout::ParaId(0), crate::layout::Unit(0));
    let rec = SelectionRecord { selection: Selection::Caret(at), chain: Vec::new() };
    Transaction { undo: Vec::new(), before: rec.clone(), after: rec, label: "t".into(), bytes, id: 0 }
}

#[test]
fn undoing_past_evicted_steps_is_not_the_opened_document() {
    let mut h = History::default();
    let n = MIN_STEPS + 5;
    for _ in 0..n {
        h.record(tx(1 << 20), None);
    }
    assert!(h.undo_depth() < n, "the oldest steps were evicted");
    while h.pop_undo().is_some() {}
    assert_ne!(h.position(), 0);
}

#[test]
fn a_style_edit_counts_the_tables_it_keeps() {
    let mut e = opened();
    let id = e.document().resolver.as_ref().and_then(|r| r.styles.by_id.keys().next().cloned()).expect("a style");
    let base = e.history().bytes();
    let change = StyleMetaChange { id: crate::api::value::StyleId(id), visibility: None, priority: Some(5), quick: None, locked: None };
    assert!(matches!(e.execute(Command::SetStyleMeta(change), fonts()), Outcome::Changed(_)));
    let grew = e.history().bytes() - base;
    assert!(grew > 1024, "a retained style table counted as {grew} bytes");
}

#[test]
fn consecutive_backspaces_are_one_step() {
    let mut e = Editor::open(&blank_with(""), fonts()).expect("opens");
    type_text(&mut e, "abcde");
    let d = e.history().undo_depth();
    for _ in 0..3 {
        e.execute(del(Dir::Back, false), fonts());
    }
    assert_eq!(e.history().undo_depth(), d + 1);
    e.execute(Command::Undo, fonts());
    assert_eq!(texts(&e).concat(), "abcde");
}

#[test]
fn consecutive_forward_deletes_are_one_step() {
    let mut e = Editor::open(&blank_with("abcde"), fonts()).expect("opens");
    for _ in 0..3 {
        e.execute(del(Dir::Forward, false), fonts());
    }
    assert_eq!(e.history().undo_depth(), 1);
    e.execute(Command::Undo, fonts());
    assert_eq!(texts(&e).concat(), "abcde");
}

#[test]
fn a_tab_joins_the_typing_around_it() {
    let mut e = Editor::open(&blank_with(""), fonts()).expect("opens");
    type_text(&mut e, "ab");
    e.execute(Command::InsertTab, fonts());
    type_text(&mut e, "cd");
    assert_eq!(e.history().undo_depth(), 1);
}

#[test]
fn a_symbol_is_its_own_step() {
    let mut e = Editor::open(&blank_with(""), fonts()).expect("opens");
    e.execute(Command::InsertSymbol(InsertSymbol { char: '\u{a9}', font: None }), fonts());
    type_text(&mut e, "xy");
    assert!(texts(&e).concat().contains('\u{a9}'), "{:?}", texts(&e));
    e.execute(Command::Undo, fonts());
    assert_eq!(texts(&e).concat(), "\u{a9}");
}

#[test]
fn the_undo_list_undoes_back_to_the_action_chosen_and_redo_does_them_again() {
    let mut e = Editor::open(&blank_with("x"), fonts()).expect("opens");
    e.complete(fonts());
    e.execute(Command::SelectAll, fonts());
    e.execute(Command::SetBold(true), fonts());
    e.execute(Command::SetItalic(true), fonts());
    e.execute(Command::SetUnderline(crate::api::value::UnderlineStyle::Single), fonts());
    let labels: Vec<&str> = e.history().undo_labels().collect();
    assert_eq!(labels, ["Underline", "Italic", "Bold"], "the most recent first");
    assert_eq!(labels[0], e.history().next_undo().unwrap());
    let shown = |e: &mut Editor| {
        let c = e.format_state().expect("a state").character;
        (c.bold, c.italic)
    };
    assert!(matches!(e.undo_many(true, 2, fonts()), Outcome::Changed(_)));
    assert_eq!(e.history().redo_depth(), 2);
    assert_eq!(shown(&mut e), (crate::api::state::Tri::Uniform(true), crate::api::state::Tri::Uniform(false)));
    assert_eq!(e.history().redo_labels().collect::<Vec<_>>(), ["Italic", "Underline"], "the next to redo first");
    assert!(matches!(e.undo_many(true, 0, fonts()), Outcome::Unchanged(_)), "none undoes nothing");
    assert!(matches!(e.undo_many(false, 0, fonts()), Outcome::Unchanged(_)), "none redoes nothing");
    assert_eq!((e.history().undo_depth(), e.history().redo_depth()), (1, 2));
    assert_eq!(shown(&mut e), (crate::api::state::Tri::Uniform(true), crate::api::state::Tri::Uniform(false)));
    assert!(matches!(e.undo_many(false, 9, fonts()), Outcome::Changed(_)), "more than there are redoes them all");
    assert_eq!((e.history().undo_depth(), e.history().redo_depth()), (3, 0));
    assert!(matches!(e.undo_many(false, 1, fonts()), Outcome::Unchanged(_)));
}

#[test]
fn undoing_several_scrolls_to_where_the_first_put_the_caret() {
    use super::super::input::Request;
    let mut e = Editor::open(&blank_with("one two\nthree"), fonts()).expect("opens");
    e.complete(fonts());
    let ids = super::texts_ids(&e);
    e.select_range(ids[0], crate::layout::Unit(0), crate::layout::Unit(3), fonts());
    e.execute(Command::SetBold(true), fonts());
    e.execute(Command::SetItalic(true), fonts());
    e.select_range(ids[1], crate::layout::Unit(0), crate::layout::Unit(0), fonts());
    e.take_requests();
    assert!(matches!(e.undo_many(true, 2, fonts()), Outcome::Changed(_)));
    assert!(e.selection.focus().is_some_and(|p| p.para_offset().map(|(p, _)| p) == Some(ids[0])), "back on the first paragraph");
    assert!(e.take_requests().iter().any(|r| matches!(r, Request::ScrollIntoView { .. })), "the view follows it");
}
