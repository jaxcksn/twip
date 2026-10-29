//! The transaction pipeline: every change to the document is one committed transaction with its record (undo and redo
//! too), batches commit once or not at all, the saved position follows undo, and with tracking on every edit path
//! records revisions.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use serde_json::Value;

use super::super::Editor;
use super::super::commands::{Command, Outcome};
use super::scenarios::{Run, scenario_files};
use super::{fonts, open, texts};
use crate::api::record::Origin;

/// The document as saving writes it: what a change to the document is (view settings aren't saved).
fn saved(e: &Editor) -> Vec<u8> {
    crate::docx::write::save(e.document()).unwrap_or_default()
}

/// Runs a scenario's steps, checking after every message that the document changed only with a transaction record,
/// and that the revision rose by the number of records.
fn check(path: &Path) {
    let s: Value = serde_json::from_str(&std::fs::read_to_string(path).expect("readable scenario")).expect("a scenario");
    let id = s["id"].as_str().unwrap_or("?").to_string();
    let mut r = Run::open(s["fixture"].as_str().unwrap_or(""), &id);
    let last = Rc::new(RefCell::new((r.e.revision(), saved(&r.e), r.e.document().settings.track_revisions, 0)));
    let seen = last.clone();
    r.frame = Some(Box::new(move |e: &mut Editor| {
        let commits = e.take_commits();
        let now = saved(e);
        let (revision, before, tracking, n) = &mut *seen.borrow_mut();
        *n += 1;
        assert_eq!(e.revision() - *revision, commits.len() as u64, "message {n}: the revision rose by the number of records");
        // Track Changes is a setting outside the history, as in Word: undo and redo stay exact whatever it is.
        let toggled = std::mem::replace(tracking, e.document().settings.track_revisions) != *tracking;
        assert!(now == *before || !commits.is_empty() || toggled, "message {n}: the document changed with no transaction");
        for (k, c) in commits.iter().enumerate() {
            assert_eq!(c.revision, *revision + 1 + k as u64, "message {n}: each record has the revision it made");
        }
        (*revision, *before) = (e.revision(), now);
    }));
    for step in s["steps"].as_array().into_iter().flatten() {
        r.step(step);
    }
}

#[test]
fn every_change_in_the_corpus_is_a_recorded_transaction() {
    let only = std::env::var("SCENARIO").ok();
    let files: Vec<_> = scenario_files().into_iter().filter(|f| only.as_ref().is_none_or(|o| f.to_string_lossy().contains(o.as_str()))).collect();
    let failed: Vec<String> = super::par_map(&files, |f| {
        std::panic::catch_unwind(|| check(f)).err().map(|e| {
            let why = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
            format!("{}: {}", f.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(), why.lines().next().unwrap_or(""))
        })
    })
    .into_iter()
    .flatten()
    .collect();
    assert!(failed.is_empty(), "{} scenarios changed the document outside a transaction:\n{}", failed.len(), failed.join("\n"));
}

#[test]
fn undo_and_redo_commit_with_their_origins() {
    let mut e = open("FX-EMPTY.docx");
    e.with_origin(Origin::Api, |e| e.execute(Command::InsertText("Hi".into()), fonts()));
    e.execute(Command::Undo, fonts());
    e.execute(Command::Redo, fonts());
    let got: Vec<(u64, Origin)> = e.take_commits().iter().map(|c| (c.revision, c.origin)).collect();
    assert_eq!(got, [(2, Origin::Api), (3, Origin::Undo), (4, Origin::Redo)]);
    assert_eq!(texts(&e), ["Hi"]);
    assert_eq!(e.execute(Command::Redo, fonts()), Outcome::Unchanged("nothing to redo"));
    assert!(e.take_commits().is_empty() && e.revision() == 4, "nothing done commits nothing");
}

#[test]
fn a_batch_commits_once_or_not_at_all() {
    let mut e = open("FX-EMPTY.docx");
    e.begin_batch(Some("Block".into()));
    e.execute(Command::InsertText("One".into()), fonts());
    e.execute(Command::InsertParagraph, fonts());
    e.execute(Command::InsertText("Two".into()), fonts());
    assert_eq!(e.revision(), 1, "nothing commits before the batch ends");
    assert_eq!(e.end_batch(true, fonts()), Outcome::Changed(2));
    let commits = e.take_commits();
    assert_eq!(commits.iter().map(|c| c.label.as_ref()).collect::<Vec<_>>(), ["Block"]);
    assert_eq!(commits[0].targets.paras.len(), 2);
    assert_eq!((e.history().undo_depth(), e.history().next_undo()), (1, Some("Block")));
    e.execute(Command::Undo, fonts());
    assert_eq!(texts(&e), [""], "one undo takes the whole batch");
    // A refused batch is undone: no revision, no record, and redo is still there.
    e.take_commits();
    e.begin_batch(None);
    e.execute(Command::InsertText("Gone".into()), fonts());
    e.begin_batch(None);
    e.execute(Command::InsertParagraph, fonts());
    assert_eq!(e.end_batch(false, fonts()), Outcome::Unchanged("refused"));
    assert_eq!(texts(&e), ["Gone"], "a refused inner batch undoes only itself");
    assert!(matches!(e.end_batch(false, fonts()), Outcome::Unchanged(_)));
    assert_eq!((texts(&e), e.revision()), (vec![String::new()], 3));
    assert!(e.take_commits().is_empty() && e.history().redo_depth() == 1);
}

#[test]
fn the_saved_position_follows_undo_and_redo() {
    let mut e = open("FX-EMPTY.docx");
    assert!(!e.is_dirty());
    e.execute(Command::InsertText("a".into()), fonts());
    e.execute(Command::InsertParagraph, fonts());
    e.mark_saved();
    assert!(!e.is_dirty());
    e.execute(Command::Undo, fonts());
    assert!(e.is_dirty());
    e.execute(Command::Redo, fonts());
    assert!(!e.is_dirty(), "back at the saved position");
    // Typing after a save starts a new step: undoing it comes back to what was saved.
    e.execute(Command::InsertText("b".into()), fonts());
    e.mark_saved();
    e.execute(Command::InsertText("c".into()), fonts());
    assert_eq!(e.history().undo_depth(), 4, "the save ended the typing run");
    assert!(e.is_dirty());
    e.execute(Command::Undo, fonts());
    assert!(!e.is_dirty());
}

#[test]
fn read_only_refuses_edits_before_they_run() {
    let mut e = open("FX-EMPTY.docx");
    e.execute(Command::InsertText("Kept".into()), fonts());
    e.set_mode(super::super::Mode::ReadOnly);
    e.take_commits();
    let refused = |o: Outcome| matches!(o, Outcome::Refused(e) if e.code == crate::api::error::ErrorCode::ReadOnly);
    assert!(refused(e.execute(Command::InsertText("x".into()), fonts())));
    assert!(refused(e.execute(Command::Undo, fonts())));
    assert_eq!((texts(&e), e.revision()), (vec!["Kept".to_string()], 2));
    assert!(e.take_commits().is_empty());
}

#[test]
fn view_settings_move_no_revision() {
    let mut e = open("FX-MERGE-FIELDS.docx");
    e.execute(Command::SetMode(crate::api::state::Mode::Design), fonts());
    e.execute(Command::ShowFieldCodes(crate::api::command::ShowFieldCodes { show: true, ids: None }), fonts());
    assert_eq!(e.revision(), 1, "design labels and field codes are view settings");
    assert!(e.history().undo_depth() == 0 && !e.is_dirty());
}

/// How many revision marks the document's paragraphs and runs carry.
fn tracked(e: &Editor) -> usize {
    let (doc, index) = (e.document(), e.index());
    let marks = |r: &crate::docx::Run| r.rev.ins.is_some() as usize + r.rev.del.is_some() as usize + r.rev.fmt.is_some() as usize;
    index
        .para_ids()
        .filter_map(|id| index.para(doc, id))
        .map(|p| marks(&p.mark) + p.inlines.iter().filter_map(super::super::step::run_of).map(marks).sum::<usize>())
        .sum()
}

/// An editor with text and tracking on.
fn tracking() -> Editor {
    let mut e = open("FX-EMPTY.docx");
    e.execute(Command::InsertText("an apple a day".into()), fonts());
    e.execute(Command::SetTrackChanges(true), fonts());
    assert_eq!(tracked(&e), 0);
    e
}

#[test]
fn a_batch_records_revisions() {
    let mut e = tracking();
    e.begin_batch(Some("Block".into()));
    e.execute(Command::InsertParagraph, fonts());
    e.execute(Command::InsertText("more".into()), fonts());
    e.end_batch(true, fonts());
    assert!(tracked(&e) >= 2, "the paragraph mark and the text are insertions");
    e.execute(Command::Undo, fonts());
    assert_eq!(tracked(&e), 0);
}

#[test]
fn find_and_replace_records_revisions() {
    let mut e = tracking();
    e.replace_all("apple", "pear", false, fonts());
    assert!(tracked(&e) >= 2, "the word replaced is a deletion and an insertion");
}

#[test]
fn a_dialog_apply_records_revisions() {
    // What the Hyperlink dialog's apply runs.
    let mut e = tracking();
    let link = crate::api::command::Hyperlink { address: "https://example.com".into(), text: Some("site".into()) };
    e.execute(Command::InsertHyperlink(link), fonts());
    assert!(tracked(&e) > 0);
}

#[test]
fn a_context_menu_action_records_revisions() {
    // What the table menu's Insert Tab runs.
    let mut e = tracking();
    e.execute(Command::InsertTab, fonts());
    assert!(tracked(&e) > 0);
}
