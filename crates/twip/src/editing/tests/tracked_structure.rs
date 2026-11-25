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

/// In Balloons mode a wholly deleted paragraph that shows nothing still has its balloon: before a table, joined to
/// the next paragraph, at the body's end and at a footnote's end, and a deletion of more paragraphs than the walk
/// between neighbours reaches, before a table and at the body's end.
#[test]
fn wholly_deleted_paragraphs_have_balloons() {
    use super::empty_stories::{NS, build};
    use crate::docx::review::BalloonMode;
    use crate::editing::revisions::index::{RevisionKind, revisions};
    let del = |id: u32, text: &str| {
        format!(
            r#"<w:p><w:pPr><w:rPr><w:del w:id="{id}" w:author="A" w:date="2026-01-0{id}T00:00:00Z"/></w:rPr></w:pPr><w:del w:id="{}" w:author="A" w:date="2026-01-0{id}T00:00:00Z"><w:r><w:delText>{text}</w:delText></w:r></w:del></w:p>"#,
            id + 10
        )
    };
    let p = |t: &str| format!("<w:p><w:r><w:t>{t}</w:t></w:r></w:p>");
    // One deletion by B of `n` whole paragraphs, its mark ids from `id`
    let long = |id: u32, n: u32| {
        let at = r#"w:author="B" w:date="2026-01-05T00:00:00Z""#;
        (0..n)
            .map(|i| {
                let k = id + 2 * i;
                format!(
                    r#"<w:p><w:pPr><w:rPr><w:del w:id="{k}" {at}/></w:rPr></w:pPr><w:del w:id="{}" {at}><w:r><w:delText>gone {i}</w:delText></w:r></w:del></w:p>"#,
                    k + 1
                )
            })
            .collect::<String>()
    };
    let table = format!(
        r#"<w:tbl><w:tblPr><w:tblW w:w="0" w:type="auto"/></w:tblPr><w:tblGrid><w:gridCol w:w="4000"/></w:tblGrid><w:tr><w:tc>{}</w:tc></w:tr></w:tbl>"#,
        p("cell")
    );
    let body = format!(
        r#"{}{}{table}{}{}<w:p><w:r><w:t>noted</w:t></w:r><w:r><w:footnoteReference w:id="1"/></w:r></w:p>{}{}{table}{}{}{}"#,
        p("Before"),
        del(1, "Gone before the table"),
        del(2, "Gone joined"),
        p("Kept"),
        p("Long before the table"),
        long(100, 70),
        p("Long at the end"),
        del(3, "Gone at the end"),
        long(300, 70),
    );
    let foot = format!(
        r#"<w:footnotes {NS}><w:footnote w:id="1"><w:p><w:r><w:t>note</w:t></w:r></w:p>{}</w:footnote></w:footnotes>"#,
        del(4, "Gone in the note")
    );
    let mut e = Editor::open(&build(&body, "", &[("footnotes.xml", "footnotes", "footnotes", &foot)]), fonts()).expect("opens");
    e.complete(fonts());
    e.set_balloon_mode(BalloonMode::Balloons);
    e.complete(fonts());
    let mut want: Vec<_> =
        revisions(e.document(), e.index()).iter().filter(|r| r.kind == RevisionKind::Delete).flat_map(|r| r.ids.first().copied()).collect();
    assert_eq!(want.len(), 6, "six deletions");
    let pages = e.snapshot().pages().len();
    let balloons = e.view(0..pages).revision_balloons;
    let mut got: Vec<_> = balloons.iter().filter(|b| b.kind == RevisionKind::Delete).map(|b| b.revision).collect();
    want.sort();
    got.sort();
    assert_eq!(got, want, "one balloon per deletion: {balloons:?}");
    assert!(balloons.iter().all(|b| b.at[1] < b.at[2] && b.page < pages), "anchored on a line: {balloons:?}");
    // The note's deletion, which shows nothing, on a line the note shows
    let note = revisions(e.document(), e.index())
        .into_iter()
        .find(|r| r.kind == RevisionKind::Delete && matches!(r.story, crate::editing::position::StoryId::Footnote(_)))
        .expect("the note's deletion");
    let b = balloons.iter().find(|b| Some(b.revision) == note.ids.first().copied()).expect("its balloon");
    let lines: Vec<[f64; 4]> = e
        .page_geometries()
        .into_iter()
        .filter(|(page, _)| *page == b.page)
        .flat_map(|(_, g)| g.lines.iter().filter(|l| g.stories[l.story].story == note.story).map(|l| l.band(0.0, 0.0)).collect::<Vec<_>>())
        .collect();
    assert!(lines.iter().any(|r| (r[1] - b.at[1]).abs() < 0.01 && (r[3] - b.at[2]).abs() < 0.01), "on a line of the note: {b:?} {lines:?}");
}

#[test]
fn accepting_the_shown_changes_leaves_hidden_moves_alone() {
    let d = r#"w:author="a" w:date="2020-01-01T00:00:00Z""#;
    let body = format!(
        r#"<w:p><w:moveFromRangeStart w:id="1" w:name="m1" {d}/><w:moveFrom w:id="2" {d}><w:r><w:t>moved</w:t></w:r></w:moveFrom><w:moveFromRangeEnd w:id="1"/></w:p><w:p><w:moveToRangeStart w:id="3" w:name="m1" {d}/><w:moveTo w:id="4" {d}><w:r><w:t>moved</w:t></w:r></w:moveTo><w:moveToRangeEnd w:id="3"/></w:p><w:p><w:r><w:rPr><w:b/><w:rPrChange w:id="5" {d}><w:rPr/></w:rPrChange></w:rPr><w:t>fmt</w:t></w:r></w:p>"#
    );
    let mut e = Editor::open(&super::blank_body(&body), fonts()).expect("opens");
    e.complete(fonts());
    let marks = |e: &Editor| {
        let s = String::from_utf8({
            use std::io::Read;
            let mut z = zip::ZipArchive::new(std::io::Cursor::new(crate::docx::write::save(e.document()).expect("saves"))).unwrap();
            let mut v = Vec::new();
            z.by_name("word/document.xml").unwrap().read_to_end(&mut v).unwrap();
            v
        })
        .unwrap();
        (s.contains("<w:moveFrom ") && s.contains("<w:moveTo "), s.contains("rPrChange"))
    };
    assert_eq!(marks(&e), (true, true));
    e.set_show_markup(false, true, true);
    assert!(!e.document().review.markup.show.insertions_deletions, "insertions and deletions hidden");
    assert!(matches!(e.resolve_shown(true, fonts()), Outcome::Changed(_)));
    assert_eq!(marks(&e), (true, false), "the shown formatting change is accepted, the hidden move stays");
}

#[test]
fn accepting_a_selection_that_ends_at_a_bookmark_keeps_the_rest() {
    use crate::editing::position::{Position, StoryId};
    use crate::layout::Unit;
    let d = r#"w:author="a" w:date="2020-01-01T00:00:00Z""#;
    let body = format!(
        r#"<w:p><w:ins w:id="1" {d}><w:r><w:t>ab</w:t></w:r></w:ins><w:bookmarkStart w:id="0" w:name="m"/><w:bookmarkEnd w:id="0"/><w:ins w:id="2" {d}><w:r><w:t>cd</w:t></w:r></w:ins></w:p>"#
    );
    let mut e = super::Editor::open(&super::blank_body(&body), fonts()).expect("opens");
    e.complete(fonts());
    let para = super::para_id(&e, 0);
    e.set_selection(Selection::range(
        Position::text(StoryId::Body, para, Unit(0)),
        Position::text(StoryId::Body, para, Unit(2)),
        crate::editing::selection::Granularity::Char,
    ));
    let out = e.execute(Command::AcceptRevisionsInSelection, fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    assert_eq!(super::texts(&e), ["abcd"]);
    let left = crate::editing::revisions::index::revisions(e.document(), e.index());
    assert_eq!(left.len(), 1, "the insertion after the bookmark stays: {left:?}");
}

#[test]
fn a_paragraph_property_change_over_two_paragraphs_ends_in_the_second() {
    use crate::editing::revisions::index::{RevisionKind, revisions};
    let d = r#"w:author="a" w:date="2020-01-01T00:00:00Z""#;
    let p = |id: u32, t: &str| {
        format!(r#"<w:p><w:pPr><w:jc w:val="center"/><w:pPrChange w:id="{id}" {d}><w:pPr/></w:pPrChange></w:pPr><w:r><w:t>{t}</w:t></w:r></w:p>"#)
    };
    let body = format!("{}{}<w:p><w:r><w:t>after</w:t></w:r></w:p>", p(1, "one"), p(2, "two"));
    let e = super::Editor::open(&super::blank_body(&body), fonts()).expect("opens");
    let list: Vec<_> = revisions(e.document(), e.index()).into_iter().filter(|r| r.kind == RevisionKind::ParagraphProperty).collect();
    assert_eq!(list.len(), 1, "one revision over both: {list:?}");
    let marks = crate::editing::revisions::marks::Marks::build(e.document(), e.index());
    let s = marks.starts_in(super::para_id(&e, 0)).find(|s| s.kind == RevisionKind::ParagraphProperty).expect("its start");
    let end = marks.end_of(e.document(), e.index(), &s);
    assert_eq!(end.para_offset().map(|p| p.0), Some(super::para_id(&e, 1)), "the second paragraph's: {end:?}");
    assert_eq!(end, list[0].end, "where the revision list ends it");
}
