//! Recording is reversible. Every editing scenario, and every recording one, replayed with tracking on from its first
//! step: rejecting every revision gives the document it started from, and accepting every revision gives the
//! document the same steps make with tracking off (text, run formatting, paragraph properties, table structure).

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;

use serde_json::{Value, json};

use super::review_resolve::{Shape, body};
use crate::editing::tests::scenarios::{Run, scenario_files};

/// Scenarios whose steps don't make the same edits tracked and untracked, so accepting all can't match the untracked
/// replay. Rejecting all still has to give the original.
const DIFFERENT_EDITS: &[(&str, &str)] = &[
    // Their targets are units of the tracked document, where deleted text still takes its units (or deleted
    // paragraphs their place): untracked, the same targets name other text, or paragraphs that are gone.
    ("RECORD-DELETE-DELETED-TEXT", "targets inside deleted text"),
    ("RECORD-DELETE-UNTRACKED", "targets after deleted text"),
    ("RECORD-KEYS-BACKSPACE-AFTER-DELETION", "targets after deleted text"),
    ("RECORD-KEYS-BACKSPACE-INSIDE-DELETION", "targets inside deleted text"),
    ("RECORD-KEYS-BACKSPACE-TWICE-AFTER-DELETION", "targets after deleted text"),
    ("RECORD-KEYS-CTRL-BACKSPACE-AFTER-DELETION", "targets after deleted text"),
    ("RECORD-KEYS-DELETE-BEFORE-DELETION", "keys over deleted text"),
    ("RECORD-KEYS-DELETE-INSIDE-DELETION", "targets inside deleted text"),
    ("RECORD-KEYS-DELETE-TWICE-BEFORE-DELETION", "keys over deleted text"),
    ("RECORD-TYPE-INSIDE-DELETION", "targets inside deleted text"),
    ("RECORD-MOVE-PARAGRAPH", "targets a paragraph a cut removes"),
    ("RECORD-CUT-PASTE-PARAGRAPH-KEYS", "targets a paragraph a cut removes"),
    // Backspace joining two paragraphs keeps the first one's properties; accepting a deleted mark joins into the
    // next paragraph, which keeps its own.
    ("E2E-EDIT-08", "a join's properties"),
    // Enter at the start of a table's first cell at the document's start adds a paragraph above the table; tracked,
    // the selection it replaces stays (deleted) before the caret, so Enter splits the cell's paragraph.
    ("E2E-EDIT-11", "Enter after deleted text at a cell's start"),
];

fn command(r: &mut Run, id: &str, arg: &str) {
    r.send(json!({ "type": "command", "id": id, "arg": arg }));
}

/// The scenario's steps on its fixture, tracked or not; its own tracking switches are left out. `None` when a step
/// can't run (its target isn't there). Until a scenario names its author, the edits are another author's than those
/// it names, so its later edits don't take what its setup typed for their own insertions.
fn replay(s: &Value, tracked: bool) -> Option<Run> {
    let id = s["id"].as_str().unwrap_or("?");
    let mut r = Run::open(s["fixture"].as_str().unwrap_or(""), id);
    command(&mut r, "review.track", if tracked { "on" } else { "off" });
    command(&mut r, "review.author", "Setup Author");
    let steps = s["steps"].as_array().cloned().unwrap_or_default();
    catch_unwind(AssertUnwindSafe(move || {
        for step in steps.iter().filter(|st| st["command"] != "review.track") {
            r.step(step);
        }
        r
    }))
    .ok()
}

/// The body after accepting or rejecting everything.
fn resolved(r: &mut Run, accept: bool) -> Vec<Shape> {
    command(r, if accept { "review.accept" } else { "review.reject" }, "all");
    body(r.e.document())
}

/// The first block where two bodies differ, or `None` when they are equal.
fn differs(got: &[Shape], want: &[Shape]) -> Option<String> {
    if let Some(k) = (0..got.len().min(want.len())).find(|&k| got[k] != want[k]) {
        // Where their descriptions part, with a little before it.
        let (g, w) = (format!("{:?}", got[k]), format!("{:?}", want[k]));
        let at = g.char_indices().zip(w.chars()).find(|((_, a), b)| a != b).map_or(g.len().min(w.len()), |((i, _), _)| i);
        let from = g.floor_char_boundary(at.saturating_sub(100));
        let part = |s: &str| s.get(from..).unwrap_or("").chars().take(200).collect::<String>();
        return Some(format!("block {k}: …{}\n    want …{}", part(&g), part(&w)));
    }
    (got.len() != want.len()).then(|| format!("{} blocks for {}", got.len(), want.len()))
}

fn scenarios() -> Vec<std::path::PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/scenarios");
    let (edit, review) = (root.join("edit"), root.join("review"));
    scenario_files()
        .into_iter()
        .filter(|f| f.starts_with(&edit) || (f.starts_with(&review) && f.file_name().is_some_and(|n| n.to_string_lossy().starts_with("record-"))))
        .collect()
}

#[test]
fn rejecting_all_restores_and_accepting_all_matches_untracked() {
    let files = scenarios();
    assert!(files.len() > 40, "the editing and recording scenarios: {}", files.len());
    let mut failures = Vec::new();
    for f in &files {
        let s: Value = serde_json::from_str(&std::fs::read_to_string(f).expect("readable scenario")).expect("json");
        let id = s["id"].as_str().unwrap_or("?");
        let mut start = Run::open(s["fixture"].as_str().unwrap_or(""), id);
        let original = resolved(&mut start, false);
        let (Some(mut rejected), Some(mut accepted)) = (replay(&s, true), replay(&s, true)) else {
            failures.push(format!("{id}: a step doesn't run tracked"));
            continue;
        };
        if let Some(d) = differs(&resolved(&mut rejected, false), &original) {
            failures.push(format!("{id}: rejecting all, {d}"));
        }
        if DIFFERENT_EDITS.iter().any(|(k, _)| *k == id) {
            continue;
        }
        let Some(mut untracked) = replay(&s, false) else {
            failures.push(format!("{id}: a step doesn't run untracked"));
            continue;
        };
        if let Some(d) = differs(&resolved(&mut accepted, true), &resolved(&mut untracked, true)) {
            failures.push(format!("{id}: accepting all, {d}"));
        }
    }
    assert!(failures.is_empty(), "{} of {} scenarios:\n{}", failures.len(), files.len(), failures.join("\n"));
}
