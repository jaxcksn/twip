//! Tracked edits that remove or rebuild structure: notes, cells, columns, controls, moves

use serde_json::{Value, json};

use super::super::Editor;
use super::super::commands::Command;
use super::{fonts, open};

fn mv(by: &str, forward: bool, extend: bool) -> Value {
    json!({ "type": "moveSelection", "by": by, "direction": if forward { "forward" } else { "backward" }, "extend": extend })
}

/// Runs the commands on the fixture, checking the document's integrity after each, then undoes and redoes them all.
fn replay(fixture: &str, cmds: &[Value]) -> Editor {
    let mut e = open(fixture);
    e.complete(fonts());
    for (k, c) in cmds.iter().enumerate() {
        let cmd: Command = serde_json::from_value(c.clone()).unwrap_or_else(|err| panic!("bad command {c}: {err}"));
        e.execute(cmd, fonts());
        while e.idle(fonts()) {}
        let st = super::super::inspect::inspect(&mut e, "integrity", true);
        assert_eq!(st["integrity"], json!([]), "integrity after #{k} {c}");
        let saved = crate::docx::write::save(e.document()).expect("saves");
        Editor::open(&saved, fonts()).unwrap_or_else(|err| panic!("reopens after #{k} {c}: {err}"));
    }
    while e.history().undo_depth() > 0 {
        e.execute(Command::Undo, fonts());
    }
    while e.history().redo_depth() > 0 {
        e.execute(Command::Redo, fonts());
    }
    e
}

fn track() -> Value {
    json!({ "type": "setTrackChanges", "value": true })
}

#[test]
fn rejecting_a_tracked_footnote_moves_the_caret_out_of_it() {
    for reject in ["rejectRevisionsInSelection", "rejectAllRevisions"] {
        replay("FX-TEXT.docx", &[track(), json!({ "type": "insertFootnote" }), json!({ "type": reject })]);
    }
}

#[test]
fn deleting_a_tracked_footnotes_reference_from_inside_the_note_moves_the_caret_out() {
    replay(
        "FX-TEXT.docx",
        &[track(), json!({ "type": "insertFootnote" }), mv("document", false, true), json!({ "type": "delete", "direction": "backward" })],
    );
}

#[test]
fn clearing_cells_under_tracking_leaves_the_caret_in_the_table() {
    replay(
        "FX-TEXT.docx",
        &[
            json!({ "type": "insertTable", "rows": 1, "columns": 3 }),
            track(),
            mv("paragraph", true, true),
            json!({ "type": "delete", "direction": "forward" }),
        ],
    );
}

#[test]
fn pasting_cells_under_tracking_leaves_the_caret_in_the_table() {
    replay(
        "FX-NEST-MIXED.docx",
        &[
            json!({ "type": "insertTable", "rows": 2, "columns": 3 }),
            json!({ "type": "select", "value": "cell" }),
            track(),
            json!({ "type": "copy" }),
            json!({ "type": "insertRows", "where": "below" }),
            json!({ "type": "paste" }),
        ],
    );
}

#[test]
fn tracked_column_deletion_keeps_rows_within_the_grid() {
    replay(
        "FX-TBL-MERGED.docx",
        &[
            mv("paragraph", true, false),
            json!({ "type": "insertColumns", "where": "right" }),
            track(),
            mv("document", true, false),
            mv("lineEdge", false, false),
            mv("word", false, false),
            json!({ "type": "deleteColumns" }),
        ],
    );
}

#[test]
fn accepting_a_tracked_deletion_of_a_block_control_drops_its_definition() {
    replay(
        "FX-CONTROLS.docx",
        &[track(), mv("document", true, true), json!({ "type": "delete", "direction": "backward" }), json!({ "type": "acceptAllRevisions" })],
    );
    replay(
        "FX-CONTROLS.docx",
        &[track(), json!({ "type": "selectAll" }), json!({ "type": "insertLineBreak" }), json!({ "type": "acceptAllRevisions" })],
    );
}

#[test]
fn deleting_moved_text_keeps_both_ends_of_the_move() {
    replay(
        "FX-TEXT.docx",
        &[
            json!({ "type": "insertLineBreak" }),
            mv("character", false, true),
            track(),
            json!({ "type": "cut" }),
            json!({ "type": "paste" }),
            json!({ "type": "selectAll" }),
            json!({ "type": "insertText", "value": "a" }),
        ],
    );
}

#[test]
fn nested_tables_of_centred_list_paragraphs_keep_their_cached_measures_fresh() {
    replay(
        "FX-TEXT.docx",
        &[
            json!({ "type": "setList", "value": "bullet" }),
            json!({ "type": "setAlignment", "value": "center" }),
            json!({ "type": "insertTable", "rows": 1, "columns": 1 }),
            json!({ "type": "insertTable", "rows": 1, "columns": 1 }),
            json!({ "type": "insertFootnote" }),
        ],
    );
    replay(
        "FX-LISTS.docx",
        &[
            json!({ "type": "insertFootnote" }),
            json!({ "type": "insertTable", "rows": 2, "columns": 2 }),
            json!({ "type": "insertTable", "rows": 1, "columns": 2 }),
            json!({ "type": "insertField", "code": "PAGE" }),
            mv("character", false, false),
            json!({ "type": "insertText", "value": "hello " }),
            json!({ "type": "insertParagraph" }),
        ],
    );
}
