//! Tracked edits that remove or rebuild structure: notes, cells, columns, controls, moves

use serde_json::{Value, json};

use super::super::Editor;
use super::super::commands::{Command, Outcome};
use super::super::selection::{CellRef, CellShape, Selection};
use super::{fonts, open};
use crate::docx::Block;

fn mv(by: &str, forward: bool, extend: bool) -> Value {
    json!({ "type": "moveSelection", "by": by, "direction": if forward { "forward" } else { "backward" }, "extend": extend })
}

/// Runs the commands on the fixture, each of which must take effect, checking the document's integrity after each,
/// then undoes and redoes them all.
fn replay(fixture: &str, cmds: &[Value]) -> Editor {
    let mut e = open(fixture);
    e.complete(fonts());
    for (k, c) in cmds.iter().enumerate() {
        let cmd: Command = serde_json::from_value(c.clone()).unwrap_or_else(|err| panic!("bad command {c}: {err}"));
        let out = e.execute(cmd, fonts());
        assert!(matches!(out, Outcome::Changed(_) | Outcome::Moved), "#{k} {c} did nothing: {out:?}");
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

/// A table of two rows, `a | b` over `c | d`, then a paragraph.
fn two_by_two() -> Editor {
    let cell = |t: &str| format!("<w:tc><w:tcPr><w:tcW w:w=\"1440\" w:type=\"dxa\"/></w:tcPr><w:p><w:r><w:t>{t}</w:t></w:r></w:p></w:tc>");
    let body = format!(
        "<w:tbl><w:tblPr><w:tblW w:w=\"2880\" w:type=\"dxa\"/></w:tblPr><w:tblGrid><w:gridCol w:w=\"1440\"/><w:gridCol w:w=\"1440\"/></w:tblGrid><w:tr>{}{}</w:tr><w:tr>{}{}</w:tr></w:tbl><w:p><w:r><w:t>end</w:t></w:r></w:p>",
        cell("a"),
        cell("b"),
        cell("c"),
        cell("d")
    );
    Editor::open(&super::blank_body(&body), fonts()).expect("opens")
}

/// Whether row `row` of the first table is marked deleted, then whether each of its cells' paragraphs has a deleted
/// run or mark.
fn deleted_in(e: &Editor, row: usize) -> Vec<bool> {
    let Some(Block::Table(t)) = e.document().sections[0].blocks.first() else { panic!("a table") };
    let r = &t.rows[row];
    std::iter::once(r.rev.del.is_some())
        .chain(r.cells.iter().flat_map(|c| c.blocks.iter()).map(|b| match b {
            Block::Para(p) => p.mark.rev.del.is_some() || p.inlines.iter().any(|i| i.run().is_some_and(|r| r.rev.del.is_some())),
            Block::Table(_) => false,
        }))
        .collect()
}

#[test]
fn rejecting_a_row_deletion_from_one_cell_restores_every_cell() {
    let mut e = two_by_two();
    e.set_tracking(true);
    let table = e.index().table_ids().next().expect("a table");
    let one = Selection::Cells { table, anchor: CellRef { row: 0, col: 0 }, focus: CellRef { row: 0, col: 0 }, shape: CellShape::Rect };
    e.set_selection(one);
    let out = e.execute(Command::DeleteRows, fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    assert_eq!(deleted_in(&e, 0), [true, true, true], "the row and its cells' text are marked deleted");
    e.set_selection(one);
    let out = e.execute(Command::RejectRevisionsInSelection, fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    assert_eq!(deleted_in(&e, 0), [false, false, false], "the whole row is back");
    assert!(super::super::integrity::check(e.document(), e.index(), &e.selection()).is_empty());
}

#[test]
fn the_revision_at_the_caret_is_the_one_the_selection_resolves() {
    let mut e = open("FX-TEXT.docx");
    e.complete(fonts());
    e.set_tracking(true);
    e.execute(Command::InsertText("xyz".into()), fonts());
    let list = super::super::revisions::index::revisions(e.document(), e.index());
    assert_eq!(list.len(), 1, "{list:?}");
    assert_eq!(e.revision_here(), list[0].ids.first().copied(), "the caret at the insertion's end is in it");
    let out = e.execute(Command::RejectRevisionsInSelection, fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    assert!(super::super::revisions::index::revisions(e.document(), e.index()).is_empty());
    assert_eq!(e.revision_here(), None);
}
