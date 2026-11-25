//! Copy, cut, paste and drag: what pasted content keeps and what it refers to

use super::{blank_body, blank_with, fonts, para_id, texts};
use crate::api::command::TableSize;
use crate::api::review::Body;
use crate::docx::{Block, Inline};
use crate::editing::Editor;
use crate::editing::commands::Command;
use crate::editing::position::{Position, StoryId};
use crate::editing::selection::{CellRef, CellShape, Granularity, Selection};
use crate::layout::{TableId, Unit};

fn t(s: &str) -> String {
    format!("<w:r><w:t xml:space=\"preserve\">{s}</w:t></w:r>")
}

fn ed(body: &str) -> Editor {
    Editor::open(&blank_body(body), fonts()).expect("opens")
}

fn paras(e: &Editor) -> Vec<&crate::docx::Paragraph> {
    e.document().sections[0].blocks.iter().filter_map(|b| if let Block::Para(p) = b { Some(p) } else { None }).collect()
}

fn select(e: &mut Editor, a: (usize, u32), b: (usize, u32)) {
    let p = |e: &Editor, (n, u): (usize, u32)| Position::text(StoryId::Body, para_id(e, n), Unit(u));
    let (pa, pb) = (p(e, a), p(e, b));
    e.set_caret_for_benchmark(Selection::range(pa, pb, Granularity::Char));
}

fn caret(e: &mut Editor, a: (usize, u32)) {
    let p = Position::text(StoryId::Body, para_id(e, a.0), Unit(a.1));
    e.set_caret_for_benchmark(Selection::Caret(p));
}

fn saved(e: &Editor) -> Editor {
    Editor::open(&crate::docx::write::save(e.document()).expect("saves"), fonts()).expect("reopens")
}

#[test]
fn a_pasted_copy_leaves_its_bookmark_with_the_original() {
    let bm = format!(r#"<w:bookmarkStart w:id="1" w:name="Target"/>{}<w:bookmarkEnd w:id="1"/>"#, t("here"));
    let mut e = ed(&format!("<w:p>{}{bm}</w:p>", t("See ")));
    select(&mut e, (0, 4), (0, 8));
    let f = e.copy().expect("copies");
    caret(&mut e, (0, 0));
    e.paste_fragment(&f, fonts());
    let text = texts(&e)[0].clone();
    assert!(text.starts_with("here") && text.ends_with("See here"), "{text:?}");
    let marks: Vec<(usize, String)> = e.bookmarks().into_iter().map(|b| (b.start.unit.0 as usize, b.name)).collect();
    assert_eq!(marks, [(text.chars().count() - 4, "Target".to_string())], "one Target, at the original");
    let n = paras(&e)[0].inlines.iter().filter(|i| matches!(i, Inline::Bookmark(n) if n.trim_start_matches('/') == "Target")).count();
    assert_eq!(n, 2, "its start and end only");
}

#[test]
fn a_copy_holding_half_a_bookmark_leaves_it_out() {
    let bm = format!(r#"<w:bookmarkStart w:id="1" w:name="Half"/>{}<w:bookmarkEnd w:id="1"/>"#, t("abcd"));
    let mut e = ed(&format!("<w:p>{bm}{}</w:p><w:p>{}</w:p>", t(" tail"), t("dest")));
    select(&mut e, (0, 2), (0, 7));
    let f = e.copy().expect("copies");
    assert!(
        !f.blocks.iter().any(|b| matches!(b, Block::Para(p) if p.inlines.iter().any(|i| matches!(i, Inline::Bookmark(_))))),
        "no lone bookmark end in the copy"
    );
}

#[test]
fn copy_joins_a_paragraph_whose_mark_is_deleted() {
    let p1 = format!(r#"<w:p><w:pPr><w:rPr><w:del w:id="1" w:author="a" w:date="2020-01-01T00:00:00Z"/></w:rPr></w:pPr>{}</w:p>"#, t("Hello"));
    let mut e = ed(&format!("{p1}<w:p>{}</w:p>", t("world")));
    select(&mut e, (0, 0), (1, 5));
    let f = e.copy().expect("copies");
    assert_eq!(f.blocks.len(), 1, "one paragraph, as accepting the deletion gives");
    assert_eq!(f.text(), "Helloworld");
}

#[test]
fn a_copy_ending_at_a_paragraph_end_merges_into_the_paste_point() {
    let mut e = ed(&format!("<w:p>{}</w:p><w:p>{}</w:p><w:p>{}</w:p>", t("Hello world"), t("Foo bar"), t("XY")));
    select(&mut e, (0, 6), (1, 7));
    let f = e.copy().expect("copies");
    assert_eq!(f.text(), "world\nFoo bar");
    caret(&mut e, (2, 1));
    e.paste_fragment(&f, fonts());
    assert_eq!(texts(&e).into_iter().skip(2).collect::<Vec<_>>(), ["Xworld", "Foo barY"]);
}

#[test]
fn a_pasted_copy_of_a_move_is_not_part_of_the_move() {
    let d = r#"w:author="A" w:date="2020-01-01T00:00:00Z""#;
    let body = format!(
        r#"<w:p><w:r><w:t>from</w:t></w:r><w:moveFromRangeStart w:id="1" w:name="move1" {d}/><w:moveFrom w:id="2" {d}><w:r><w:t>bbb</w:t></w:r></w:moveFrom><w:moveFromRangeEnd w:id="1"/></w:p><w:p><w:r><w:t xml:space="preserve">aaa </w:t></w:r><w:moveToRangeStart w:id="3" w:name="move1" {d}/><w:moveTo w:id="4" {d}><w:r><w:t>bbb</w:t></w:r></w:moveTo><w:moveToRangeEnd w:id="3"/><w:r><w:t xml:space="preserve"> ccc</w:t></w:r></w:p><w:p><w:r><w:t>dest</w:t></w:r></w:p>"#
    );
    let body = body.as_str();
    let mut e = ed(body);
    let moves = |e: &Editor| {
        paras(e)
            .iter()
            .flat_map(|p| p.inlines.iter())
            .filter(|i| matches!(i, Inline::Range(m) if matches!(m.kind, crate::docx::review::RangeKind::MoveToStart | crate::docx::review::RangeKind::MoveToEnd)))
            .count()
    };
    assert_eq!(moves(&e), 2);
    let p = para_id(&e, 1);
    e.select_range(p, Unit(0), Unit(11), fonts());
    e.execute(Command::Copy, fonts());
    let q = para_id(&e, 2);
    e.select_range(q, Unit(0), Unit(0), fonts());
    e.execute(Command::Paste(Default::default()), fonts());
    assert!(texts(&e)[2].starts_with("aaa bbb ccc"), "pasted: {:?}", texts(&e));
    assert_eq!(moves(&e), 2, "only the original's move range");
}

#[test]
fn a_pasted_editing_permission_takes_an_id_of_its_own() {
    let perm = |inner: &str| format!(r#"<w:permStart w:id="7" w:edGrp="everyone"/>{inner}<w:permEnd w:id="7"/>"#);
    let mut e = ed(&format!("<w:p>{}{}</w:p><w:p>{}</w:p>", t("ab "), perm(&t("open")), t("dest")));
    select(&mut e, (0, 0), (0, 7));
    let f = e.copy().expect("copies");
    caret(&mut e, (1, 0));
    e.paste_fragment(&f, fonts());
    let ids: Vec<(bool, i64)> =
        paras(&e).iter().flat_map(|p| p.inlines.iter()).filter_map(|i| if let Inline::Opaque(o) = i { o.permission() } else { None }).collect();
    assert_eq!(ids.len(), 4, "{ids:?}");
    assert_eq!(ids[0].1, ids[1].1);
    assert_eq!(ids[2].1, ids[3].1);
    assert_ne!(ids[0].1, ids[2].1, "the pasted pair has a new id: {ids:?}");
}

fn table_of(e: &Editor) -> &crate::docx::Table {
    e.document().sections[0].blocks.iter().find_map(|b| if let Block::Table(t) = b { Some(t) } else { None }).unwrap()
}

fn grid_table(rows: &[&[&str]]) -> Editor {
    let mut e = Editor::open(&blank_with(""), fonts()).unwrap();
    e.execute(Command::InsertTable(TableSize { rows: rows.len() as u32, columns: rows[0].len() as u32 }), fonts());
    for (r, row) in rows.iter().enumerate() {
        for (c, t) in row.iter().enumerate() {
            e.execute(Command::InsertText((*t).into()), fonts());
            if r + 1 < rows.len() || c + 1 < row.len() {
                e.execute(Command::MoveToCell(crate::api::value::Order::Next), fonts());
            }
        }
    }
    e
}

fn para_at(e: &Editor, text: &str) -> usize {
    texts(e).iter().position(|t| t == text).unwrap_or_else(|| panic!("{text:?} in {:?}", texts(e)))
}

/// A table whose row "b" has a footnote and a comment.
fn noted() -> Editor {
    let mut e = grid_table(&[&["a1", "a2"], &["b1", "b2"], &["c1", "c2"]]);
    let p = para_id(&e, para_at(&e, "b1"));
    e.select_range(p, Unit(1), Unit(1), fonts());
    e.execute(Command::InsertFootnote, fonts());
    let p = para_id(&e, para_at(&e, "b2"));
    e.select_range(p, Unit(0), Unit(2), fonts());
    e.execute(Command::AddComment(Body::Text("Note".into())), fonts());
    e
}

fn rect(e: &mut Editor, r0: usize, c0: usize, r1: usize, c1: usize, shape: CellShape) {
    let t = TableId(table_of(e).id);
    e.set_selection(Selection::Cells { table: t, anchor: CellRef { row: r0, col: c0 }, focus: CellRef { row: r1, col: c1 }, shape });
}

fn caret_at(e: &mut Editor, text: &str) {
    let p = para_id(e, para_at(e, text));
    e.select_range(p, Unit(0), Unit(0), fonts());
}

/// The note and comment ids the table refers to.
fn refs(e: &Editor) -> (Vec<i64>, Vec<i64>) {
    let (mut n, mut c) = (Vec::new(), Vec::new());
    for r in &table_of(e).rows {
        for cell in &r.cells {
            for b in &cell.blocks {
                if let Block::Para(p) = b {
                    for i in &p.inlines {
                        match i {
                            Inline::NoteRef { id, .. } => n.push(*id),
                            Inline::CommentRef { id, .. } => c.push(id.0),
                            _ => {}
                        }
                    }
                }
            }
        }
    }
    (n, c)
}

#[test]
fn a_cut_and_pasted_row_keeps_its_note_and_comment() {
    let mut e = noted();
    assert_eq!((refs(&e).0.len(), refs(&e).1.len()), (1, 1));
    rect(&mut e, 1, 0, 1, 1, CellShape::Rows);
    e.execute(Command::Cut, fonts());
    assert_eq!(table_of(&e).rows.len(), 2);
    caret_at(&mut e, "c1");
    e.execute(Command::Paste(Default::default()), fonts());
    assert_eq!(table_of(&e).rows.len(), 3);
    let (n, c) = refs(&e);
    assert_eq!((n.len(), c.len()), (1, 1));
    let d = e.document();
    assert!(n.iter().all(|id| d.footnotes.contains_key(id)), "the note's body is back: {n:?}");
    assert!(c.iter().all(|id| d.comments.by_id.contains_key(&crate::docx::review::CommentId(*id))), "the comment is back: {c:?}");
}

#[test]
fn a_copied_and_pasted_row_has_its_own_note_and_comment() {
    let mut e = noted();
    rect(&mut e, 1, 0, 1, 1, CellShape::Rows);
    e.execute(Command::Copy, fonts());
    caret_at(&mut e, "c1");
    e.execute(Command::Paste(Default::default()), fonts());
    assert_eq!(table_of(&e).rows.len(), 4);
    let (mut n, mut c) = refs(&e);
    n.sort();
    c.sort();
    n.dedup();
    c.dedup();
    assert_eq!((n.len(), c.len()), (2, 2), "each pasted reference has its own note and comment");
    let d = e.document();
    assert!(n.iter().all(|id| d.footnotes.contains_key(id)));
    assert_eq!(d.comments.by_id.len(), 2);
}

#[test]
fn a_row_pasted_in_another_document_brings_its_note() {
    let mut a = noted();
    rect(&mut a, 1, 0, 1, 1, CellShape::Rows);
    let frag = a.copy().expect("a fragment");
    let mut b = grid_table(&[&["x1", "x2"], &["y1", "y2"]]);
    caret_at(&mut b, "y1");
    b.paste_fragment(&frag, fonts());
    assert_eq!(table_of(&b).rows.len(), 3);
    let (n, c) = refs(&b);
    assert_eq!((n.len(), c.len()), (1, 1));
    let d = b.document();
    assert!(n.iter().all(|id| d.footnotes.contains_key(id)), "the pasted note has a body: {n:?}");
    assert!(c.iter().all(|id| d.comments.by_id.contains_key(&crate::docx::review::CommentId(*id))), "and the comment is there");
}

#[test]
fn a_comment_pasted_in_another_document_keeps_its_author() {
    let mut a = Editor::open(&blank_with("Hello world"), fonts()).unwrap();
    a.set_author("Alice", None);
    let p = para_id(&a, 0);
    a.select_range(p, Unit(0), Unit(5), fonts());
    a.execute(Command::AddComment(Body::Text("Hi".into())), fonts());
    a.select_range(p, Unit(0), Unit(11), fonts());
    let frag = a.copy().expect("a fragment");
    let mut b = Editor::open(&blank_with("Target"), fonts()).unwrap();
    b.set_author("Bob", None);
    let q = para_id(&b, 0);
    b.select_range(q, Unit(0), Unit(0), fonts());
    b.execute(Command::AddComment(Body::Text("Mine".into())), fonts());
    b.select_range(q, Unit(0), Unit(0), fonts());
    b.paste_fragment(&frag, fonts());
    let d = b.document();
    let names: Vec<String> = d.comments.by_id.values().filter_map(|c| d.people.get(c.author).map(|p| p.name.to_string())).collect();
    assert_eq!(names.len(), 2);
    assert!(names.contains(&"Alice".to_string()) && names.contains(&"Bob".to_string()), "{names:?}");
}

#[test]
fn pasted_comments_never_take_an_id_a_later_one_keeps() {
    let mut a = Editor::open(&blank_with("one two"), fonts()).unwrap();
    let p = para_id(&a, 0);
    for (from, to, text) in [(0, 3, "first"), (4, 7, "second")] {
        a.select_range(p, Unit(from), Unit(to), fonts());
        a.execute(Command::AddComment(Body::Text(text.into())), fonts());
    }
    a.select_range(p, Unit(0), Unit(7), fonts());
    let mut frag = a.copy().expect("a fragment");
    assert_eq!(frag.refs.comments.len(), 2, "both comments go with the copy");
    frag.refs.comments.reverse();
    let mut b = Editor::open(&blank_with("Target"), fonts()).unwrap();
    let q = para_id(&b, 0);
    b.select_range(q, Unit(0), Unit(6), fonts());
    b.execute(Command::AddComment(Body::Text("mine".into())), fonts());
    let taken = *b.document().comments.by_id.keys().next().expect("the target's comment");
    assert!(frag.refs.comments.iter().any(|c| c.id == taken), "one pasted comment collides with the target's");
    b.select_range(q, Unit(6), Unit(6), fonts());
    b.paste_fragment(&frag, fonts());
    let d = b.document();
    let text = |c: &crate::docx::review::Comment| -> String {
        c.body.iter().filter_map(|b| if let Block::Para(p) = b { Some(super::super::text::UnitText::of(p).text) } else { None }).collect()
    };
    let mut bodies: Vec<String> = d.comments.by_id.values().map(text).collect();
    bodies.sort();
    assert_eq!(bodies, ["first", "mine", "second"], "each comment keeps its own id and text");
}

/// Per row, the (kind, id) of each control the row is in.
fn row_controls(e: &Editor) -> Vec<Vec<(String, crate::docx::ControlId)>> {
    let d = e.document();
    table_of(e)
        .rows
        .iter()
        .map(|r| r.controls.iter().map(|id| (format!("{:?}", d.controls.get(id).map(|c| c.props.kind.clone())), *id)).collect())
        .collect()
}

fn item_ids(e: &Editor) -> Vec<crate::docx::ControlId> {
    row_controls(e).into_iter().flatten().filter(|(k, _)| k.contains("RepeatingItem")).map(|(_, id)| id).collect()
}

fn distinct(v: &[crate::docx::ControlId]) -> usize {
    let mut u = v.to_vec();
    u.sort();
    u.dedup();
    u.len()
}

#[test]
fn rows_a_cell_paste_adds_below_a_repeating_section_are_items_of_their_own() {
    let mut e = super::open("FX-REPEAT-REGION.docx");
    rect(&mut e, 4, 0, 4, 2, CellShape::Rows);
    e.execute(Command::Cut, fonts());
    assert_eq!(table_of(&e).rows.len(), 4, "the last row is an item");
    rect(&mut e, 0, 0, 1, 1, CellShape::Rect);
    e.execute(Command::Copy, fonts());
    caret_at(&mut e, "Item 3");
    e.execute(Command::Paste(Default::default()), fonts());
    assert_eq!(table_of(&e).rows.len(), 5, "a row was added");
    let ids = item_ids(&e);
    assert_eq!(distinct(&ids), ids.len(), "every item row has its own control: {:?}", row_controls(&e));
}

#[test]
fn a_row_pasted_inside_a_repeating_section_is_a_new_item_of_it() {
    let mut e = super::open("FX-REPEAT-REGION.docx");
    rect(&mut e, 1, 0, 1, 2, CellShape::Rows);
    e.execute(Command::Copy, fonts());
    caret_at(&mut e, "Item 2");
    e.execute(Command::Paste(Default::default()), fonts());
    assert_eq!(table_of(&e).rows.len(), 6);
    let rc = row_controls(&e);
    let section = rc[1][0].1;
    for (r, row) in rc.iter().enumerate().take(5).skip(1) {
        assert_eq!(row.first().map(|c| c.1), Some(section), "row {r} stays in the one section: {rc:?}");
    }
    let ids = item_ids(&e);
    assert_eq!((ids.len(), distinct(&ids)), (4, 4), "the pasted row is its own item: {rc:?}");
}

#[test]
fn a_pasted_row_keeps_its_content_control() {
    let cell = |t: &str| format!("<w:tc><w:p><w:r><w:t>{t}</w:t></w:r></w:p></w:tc>");
    let tagged = "<w:tc><w:p><w:sdt><w:sdtPr><w:tag w:val=\"T\"/><w:id w:val=\"77\"/><w:text/></w:sdtPr><w:sdtContent><w:r><w:t>val</w:t></w:r></w:sdtContent></w:sdt></w:p></w:tc>";
    let body = format!(
        "<w:tbl><w:tblPr><w:tblW w:w=\"4000\" w:type=\"dxa\"/></w:tblPr><w:tblGrid><w:gridCol w:w=\"2000\"/><w:gridCol w:w=\"2000\"/></w:tblGrid><w:tr>{}{tagged}</w:tr><w:tr>{}{}</w:tr></w:tbl><w:p/>",
        cell("a"),
        cell("b"),
        cell("c")
    );
    let mut e = ed(&body);
    let tags = |e: &Editor| e.document().controls.values().filter(|c| c.props.tag.as_deref() == Some("T")).count();
    assert_eq!(tags(&e), 1);
    rect(&mut e, 0, 0, 0, 1, CellShape::Rows);
    e.execute(Command::Copy, fonts());
    caret_at(&mut e, "b");
    e.execute(Command::Paste(Default::default()), fonts());
    assert_eq!(table_of(&e).rows.len(), 3);
    assert_eq!(tags(&e), 2, "the pasted row has its own tagged control");
    assert_eq!(tags(&saved(&e)), 2, "and saves it");
}

#[test]
fn dragging_a_copy_of_a_tracked_deletion_leaves_the_deleted_text() {
    use serde_json::json;
    let body = "<w:p><w:r><w:t xml:space=\"preserve\">foo </w:t></w:r><w:del w:id=\"1\" w:author=\"A\" w:date=\"2020-01-01T00:00:00Z\"><w:r><w:delText>bar</w:delText></w:r></w:del><w:r><w:t xml:space=\"preserve\"> baz</w:t></w:r></w:p><w:p><w:r><w:t>dest</w:t></w:r></w:p>";
    let mut r = super::scenarios::Run::from_bytes(&blank_body(body), "dnd-del");
    r.step(&json!({ "click": { "para": 0, "offset": 0 } }));
    r.step(&json!({ "key": "End", "mods": { "shift": true } }));
    r.step(&json!({ "drag": { "from": { "para": 0, "offset": 1 }, "to": { "para": 1, "offset": 0 } }, "mods": { "ctrl": true } }));
    let ps = paras(&r.e);
    assert!(ps[1].inlines.iter().any(|i| matches!(i, Inline::Text(t, _) if t.contains("foo"))), "the copy landed");
    let live = ps[1].inlines.iter().any(|i| matches!(i, Inline::Text(t, run) if t.contains("bar") && run.rev.del.is_none()));
    assert!(!live, "the deleted text isn't pasted as live text: {:?}", texts(&r.e));
}

#[test]
fn dragging_a_selection_ending_at_a_control_start_drops_no_lone_marker() {
    use serde_json::json;
    let mut r = super::scenarios::Run::from_bytes(&blank_with("ABCD\nXY"), "dnd-lone-control");
    r.step(&json!({ "click": { "para": 0, "offset": 2 } }));
    for _ in 0..2 {
        r.step(&json!({ "key": "ArrowRight", "mods": { "shift": true } }));
    }
    let wrap: crate::api::command::InsertControl = serde_json::from_value(json!({ "kind": "richText", "tag": "q", "wrapSelection": true })).unwrap();
    r.e.execute(Command::InsertControl(wrap), fonts());
    r.step(&json!({ "click": { "para": 0, "offset": 0 } }));
    for _ in 0..2 {
        r.step(&json!({ "key": "ArrowRight", "mods": { "shift": true } }));
    }
    assert_eq!(r.e.selected_text(), "AB");
    r.step(&json!({ "drag": { "from": { "para": 0, "offset": 1 }, "to": { "para": 1, "offset": 1 } }, "mods": { "ctrl": true } }));
    let markers = paras(&r.e)[1].inlines.iter().filter(|i| matches!(i, Inline::ControlStart(_) | Inline::ControlEnd(_))).count();
    assert_eq!(markers % 2, 0, "controls a drop pastes are whole");
}

#[test]
fn a_cut_and_pasted_note_keeps_the_content_control_in_it() {
    let mut e = Editor::open(&blank_with("Body text"), fonts()).unwrap();
    let p = para_id(&e, 0);
    e.select_range(p, Unit(4), Unit(4), fonts());
    e.execute(Command::InsertFootnote, fonts());
    let ins: crate::api::command::InsertControl = serde_json::from_value(serde_json::json!({ "kind": "plainText", "tag": "InNote" })).unwrap();
    assert!(matches!(e.execute(Command::InsertControl(ins), fonts()), crate::editing::commands::Outcome::Changed(_)));
    let tagged = |e: &Editor| e.document().controls.values().filter(|c| c.props.tag.as_deref() == Some("InNote")).count();
    assert_eq!(tagged(&e), 1, "the note holds a control");
    let p = para_id(&e, 0);
    e.select_range(p, Unit(4), Unit(5), fonts());
    e.execute(Command::Cut, fonts());
    assert!(e.document().footnotes.is_empty(), "the note went with its reference");
    let p = para_id(&e, 0);
    e.select_range(p, Unit(0), Unit(0), fonts());
    e.execute(Command::Paste(Default::default()), fonts());
    assert_eq!(e.document().footnotes.len(), 1, "the note is back");
    assert_eq!(tagged(&e), 1, "with its control");
    assert_eq!(tagged(&saved(&e)), 1, "which saves");
}

/// The package `src` with each XML part as `f` (given its name) makes it.
fn edited(src: &[u8], f: impl Fn(&str, String) -> String) -> Vec<u8> {
    use std::io::{Read, Write};
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(src)).unwrap();
    let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for i in 0..z.len() {
        let mut part = z.by_index(i).unwrap();
        let name = part.name().unwrap().to_string();
        let mut data = Vec::new();
        part.read_to_end(&mut data).unwrap();
        if name.ends_with(".xml") {
            data = f(&name, String::from_utf8(data).unwrap()).into_bytes();
        }
        out.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
        out.write_all(&data).unwrap();
    }
    out.finish().unwrap().into_inner()
}

/// A blank document with `style` added to its styles and `body` for its body.
pub(super) fn with_style(style: &str, body: &str) -> Vec<u8> {
    edited(&blank_body(body), |name, x| if name == "word/styles.xml" { x.replacen("</w:styles>", &format!("{style}</w:styles>"), 1) } else { x })
}

const W_NS: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

/// "plain", "Styled" in style Callout, then "after", copied whole from a document with `style` for Callout and its
/// styles part's root also declaring `ns`, then pasted at the start of another's "Target".
fn paste_callout(style: &str, ns: &str) -> Editor {
    let body = format!(r#"<w:p>{}</w:p><w:p><w:pPr><w:pStyle w:val="Callout"/></w:pPr>{}</w:p><w:p>{}</w:p>"#, t("plain"), t("Styled"), t("after"));
    let src = edited(
        &with_style(style, &body),
        |name, x| if name == "word/styles.xml" { x.replacen("<w:styles ", &format!("<w:styles {ns} "), 1) } else { x },
    );
    let mut a = Editor::open(&src, fonts()).unwrap();
    assert!(a.document().resolver.as_ref().is_some_and(|r| r.styles.get("Callout").is_some()), "the source has the style");
    select(&mut a, (0, 0), (2, 0));
    let frag = a.copy().expect("copies");
    let mut b = ed(&format!("<w:p>{}</w:p>", t("Target")));
    caret(&mut b, (0, 0));
    b.paste_fragment(&frag, fonts());
    b
}

#[test]
fn a_style_written_under_its_parts_own_prefix_comes_with_a_paste() {
    let style = r#"<s:style s:type="paragraph" s:customStyle="1" s:styleId="Callout"><s:name s:val="Callout"/><s:pPr><s:jc s:val="center"/></s:pPr></s:style>"#;
    let b = paste_callout(style, &format!("xmlns:s=\"{W_NS}\""));
    assert!(b.document().resolver.as_ref().is_some_and(|r| r.styles.get("Callout").is_some()), "the style came with it");
    let again = saved(&b);
    let p = &paras(&again)[1];
    assert_eq!((p.style.style_id.as_deref(), p.style.jc), (Some("Callout"), crate::docx::props::Jc::Center), "and saves");
}

#[test]
fn a_pasted_default_style_is_not_the_default_here() {
    let style = r#"<w:style w:type="paragraph" w:default="1" w:customStyle="1" w:styleId="Callout"><w:name w:val="Callout"/><w:pPr><w:jc w:val="center"/></w:pPr></w:style>"#;
    let b = paste_callout(style, "");
    let res = b.document().resolver.clone().expect("styles");
    assert!(res.styles.get("Callout").is_some(), "the style came");
    assert_eq!(res.styles.default_paragraph.as_deref(), Some("Normal"), "this document's default stays");
    assert_eq!(paras(&b)[1].style.style_id.as_deref(), Some("Callout"));
}

#[test]
fn a_list_from_a_document_writing_its_own_prefix_comes_with_a_paste() {
    let mut a = Editor::open(&blank_with("One\nTwo\nafter"), fonts()).unwrap();
    select(&mut a, (0, 0), (1, 3));
    a.execute(Command::SetList(Some(crate::api::command::ListKind::Numbered)), fonts());
    let saved = crate::docx::write::save(a.document()).expect("saves");
    let src = edited(&saved, |name, x| {
        if name == "word/document.xml" {
            x.replace("<w:", "<v:").replace("</w:", "</v:").replace(" w:", " v:").replace("xmlns:w=", "xmlns:v=")
        } else {
            x
        }
    });
    let mut a = Editor::open(&src, fonts()).unwrap();
    assert!(paras(&a)[1].style.num.is_some(), "the source reads its list");
    select(&mut a, (0, 0), (2, 0));
    let frag = a.copy().expect("copies");
    let mut b = ed(&format!("<w:p>{}</w:p>", t("Target")));
    caret(&mut b, (0, 0));
    b.paste_fragment(&frag, fonts());
    let nums: Vec<i64> = b.document().resolver.as_ref().map(|r| r.numbering.nums.keys().copied().collect()).unwrap_or_default();
    assert_eq!(nums.len(), 1, "the list's definition came: {nums:?}");
    let direct = paras(&b)[1].style.direct.0.as_deref().unwrap_or_default().to_string();
    assert!(direct.contains(&format!("numId v:val=\"{}\"", nums[0])), "the pasted paragraph is in it: {direct}");
}

#[test]
fn content_pasted_in_another_document_brings_the_styles_it_uses() {
    let style = r#"<w:style w:type="paragraph" w:customStyle="1" w:styleId="Callout"><w:name w:val="Callout"/><w:basedOn w:val="Normal"/><w:pPr><w:jc w:val="center"/></w:pPr><w:rPr><w:b/></w:rPr></w:style>"#;
    let body = format!(r#"<w:p>{}</w:p><w:p><w:pPr><w:pStyle w:val="Callout"/></w:pPr>{}</w:p><w:p>{}</w:p>"#, t("plain"), t("Styled"), t("after"));
    let mut a = Editor::open(&with_style(style, &body), fonts()).unwrap();
    select(&mut a, (0, 0), (2, 0));
    let frag = a.copy().expect("copies");
    let mut b = ed(&format!("<w:p>{}</w:p>", t("Target")));
    assert!(b.document().resolver.as_ref().is_some_and(|r| r.styles.get("Callout").is_none()));
    caret(&mut b, (0, 0));
    b.paste_fragment(&frag, fonts());
    // The first paragraph's text goes into the paragraph at the caret; the styled one is pasted whole.
    assert_eq!(paras(&b)[1].style.style_id.as_deref(), Some("Callout"));
    assert!(b.document().resolver.as_ref().is_some_and(|r| r.styles.get("Callout").is_some()), "the style came with it");
    let again = saved(&b);
    let p = &paras(&again)[1];
    assert_eq!((p.style.style_id.as_deref(), p.style.jc), (Some("Callout"), crate::docx::props::Jc::Center), "and saves");
}

#[test]
fn a_list_pasted_in_another_document_brings_its_definition() {
    let mut a = Editor::open(&blank_with("One\nTwo\nafter"), fonts()).unwrap();
    select(&mut a, (0, 0), (1, 3));
    a.execute(Command::SetList(Some(crate::api::command::ListKind::Numbered)), fonts());
    assert!(paras(&a)[0].list.is_some(), "a numbered list");
    select(&mut a, (0, 0), (2, 0));
    let frag = a.copy().expect("copies");
    let mut b = ed(&format!("<w:p>{}</w:p>", t("Target")));
    caret(&mut b, (0, 0));
    b.paste_fragment(&frag, fonts());
    // The first paragraph's text goes into the paragraph at the caret; the second is pasted whole.
    let (n, _) = paras(&b)[1].style.num.expect("the pasted paragraph is in a list");
    assert!(b.document().resolver.as_ref().is_some_and(|r| r.numbering.nums.contains_key(&n)), "its list is defined here");
    let again = saved(&b);
    assert_eq!(super::super::text::UnitText::of(paras(&again)[1]).text, "Two");
    assert!(paras(&again)[1].list.is_some(), "and saves numbered");
}

/// The list ids of the table's paragraphs, each once.
fn table_lists(e: &Editor) -> Vec<i64> {
    let mut out: Vec<i64> = table_of(e)
        .rows
        .iter()
        .flat_map(|r| r.cells.iter().flat_map(|c| c.blocks.iter()))
        .filter_map(|b| if let Block::Para(p) = b { Some(p.style.num?.0) } else { None })
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

#[test]
fn cells_pasted_over_a_larger_block_in_another_document_share_their_lists() {
    let mut a = grid_table(&[&["a", "b"]]);
    for cell in ["a", "b"] {
        caret_at(&mut a, cell);
        a.execute(Command::SetList(Some(crate::api::command::ListKind::Numbered)), fonts());
    }
    rect(&mut a, 0, 0, 0, 1, CellShape::Rect);
    let source = table_lists(&a);
    assert!(!source.is_empty(), "the cells are numbered");
    let frag = a.copy().expect("a fragment");
    let mut b = grid_table(&[&["w", "x"], &["y", "z"]]);
    let nums = |e: &Editor| e.document().resolver.as_ref().map_or(0, |r| r.numbering.nums.len());
    let before = nums(&b);
    rect(&mut b, 0, 0, 1, 1, CellShape::Rect);
    b.paste_fragment(&frag, fonts());
    assert_eq!(table_lists(&b).len(), source.len(), "each source list is one list here, over every tile");
    assert_eq!(nums(&b) - before, source.len(), "defined once each");
}

#[test]
fn a_control_filled_from_a_docx_shows_its_pictures() {
    let image = super::open("FX-OBJ.docx").document().images.last().cloned().expect("an image");
    let mut a = Editor::open(&blank_with(""), fonts()).unwrap();
    let pic = crate::api::command::Picture { data: crate::api::value::Bytes(image.clone()), alt_text: None };
    assert!(matches!(a.execute(Command::InsertPicture(pic), fonts()), crate::editing::commands::Outcome::Changed(_)));
    let docx = crate::docx::write::save(a.document()).expect("saves");
    let sdt = format!(
        r#"<w:sdt><w:sdtPr><w:alias w:val="Box"/><w:id w:val="4"/><w:richText/></w:sdtPr><w:sdtContent><w:p>{}</w:p></w:sdtContent></w:sdt><w:p/>"#,
        t("x")
    );
    let mut b = ed(&sdt);
    let c = b.document().controls.iter().find(|(_, k)| k.props.alias.as_deref() == Some("Box")).map(|(k, _)| *k).expect("the control");
    let id = super::super::merge::api_id(crate::docx::ElementId::Control(c));
    let fill = crate::api::command::FillControl { id, text: None, image: None, docx: Some(crate::api::value::Bytes(docx)), building_block: None };
    let out = b.execute(Command::FillControl(fill), fonts());
    assert!(matches!(out, crate::editing::commands::Outcome::Changed(_)), "{out:?}");
    let shown: Vec<usize> =
        paras(&b).iter().flat_map(|p| p.inlines.iter().filter_map(|i| if let Inline::Picture(p, _) = i { p.image } else { None })).collect();
    assert_eq!(shown.len(), 1, "the picture is in the control");
    assert!(b.document().images.get(shown[0]).is_some_and(|d| !d.is_empty()), "with its image: {shown:?} of {}", b.document().images.len());
    let again = saved(&b);
    assert!(paras(&again).iter().any(|p| p.inlines.iter().any(|i| matches!(i, Inline::Picture(p, _) if p.image.is_some()))), "and saves");
}

/// A table of `rows` rows of `cols` cells, the first holding `first` (body XML runs), the rest "x".
fn table_xml(rows: usize, cols: usize, first: &str) -> String {
    let grid: String = (0..cols).map(|_| "<w:gridCol w:w=\"144\"/>").collect();
    let cell = |inner: &str| format!("<w:tc><w:tcPr><w:tcW w:w=\"144\" w:type=\"dxa\"/></w:tcPr><w:p>{inner}</w:p></w:tc>");
    let body: String = (0..rows)
        .map(|r| format!("<w:tr>{}</w:tr>", (0..cols).map(|c| cell(if r + c == 0 { first } else { "<w:r><w:t>x</w:t></w:r>" })).collect::<String>()))
        .collect();
    format!("<w:tbl><w:tblPr><w:tblW w:w=\"0\" w:type=\"auto\"/></w:tblPr><w:tblGrid>{grid}</w:tblGrid>{body}</w:tbl><w:p/>")
}

#[test]
fn cells_pasted_over_the_cell_holding_a_bookmark_keep_it() {
    let bm = format!(r#"<w:bookmarkStart w:id="1" w:name="Target"/>{}<w:bookmarkEnd w:id="1"/>"#, t("here"));
    let mut e = ed(&table_xml(1, 2, &bm));
    rect(&mut e, 0, 0, 0, 0, CellShape::Rect);
    let f = e.copy().expect("copies");
    rect(&mut e, 0, 0, 0, 1, CellShape::Rect);
    e.paste_fragment(&f, fonts());
    let names: Vec<String> = e.bookmarks().into_iter().map(|b| b.name).collect();
    assert_eq!(names, ["Target"], "the replaced cell's bookmark is the pasted one, once");
}

#[test]
fn a_pasted_permission_past_the_highest_id_takes_a_free_one() {
    let perm = |id: i64, inner: &str| format!(r#"<w:permStart w:id="{id}" w:edGrp="everyone"/>{inner}<w:permEnd w:id="{id}"/>"#);
    let mut e = ed(&format!("<w:p>{}</w:p><w:p>{}</w:p><w:p>{}</w:p>", perm(i64::MAX, &t("max")), perm(7, &t("open")), t("dest")));
    select(&mut e, (1, 0), (1, 4));
    let f = e.copy().expect("copies");
    caret(&mut e, (2, 0));
    e.paste_fragment(&f, fonts());
    let ids: Vec<i64> = paras(&e)
        .iter()
        .flat_map(|p| p.inlines.iter())
        .filter_map(|i| if let Inline::Opaque(o) = i { o.permission() } else { None })
        .map(|p| p.1)
        .collect();
    assert_eq!(ids.len(), 6, "{ids:?}");
    assert!(ids.iter().all(|&id| id >= 0), "{ids:?}");
    let mut u = ids.clone();
    u.sort_unstable();
    u.dedup();
    assert_eq!(u.len(), 3, "the pasted pair has an id of its own: {ids:?}");
}

#[test]
fn a_copy_drops_a_paragraph_of_deleted_text_before_a_table() {
    let d = r#"w:id="1" w:author="a" w:date="2020-01-01T00:00:00Z""#;
    let p = format!(r#"<w:p><w:pPr><w:rPr><w:del {d}/></w:rPr></w:pPr><w:del {d}><w:r><w:delText>gone</w:delText></w:r></w:del></w:p>"#);
    let e = ed(&format!("{p}{}", table_xml(1, 1, &t("cell"))));
    let blocks = e.document().sections[0].blocks[..2].to_vec();
    let f = crate::editing::clipboard::Fragment { blocks, ..Default::default() }.accepted();
    assert!(matches!(f.blocks.as_slice(), [Block::Table(_)]), "{:?}", f.blocks);
}

#[test]
fn a_cell_block_pasted_from_another_document_takes_each_cells_note_once() {
    fn time(rows: usize, cols: usize) -> std::time::Duration {
        let mut a = ed(&table_xml(rows, cols, &t("x")));
        rect(&mut a, 0, 0, rows - 1, cols - 1, CellShape::Rect);
        let mut f = a.copy().expect("copies");
        let Some(Block::Table(src)) = f.blocks.first_mut() else { panic!("a table") };
        let mut id = 0;
        for cell in src.rows.iter_mut().flat_map(|r| r.cells.iter_mut()) {
            let Some(Block::Para(p)) = cell.blocks.first_mut() else { panic!("a paragraph") };
            id += 1;
            let run = p.mark.clone();
            p.inlines.push(Inline::NoteRef { kind: crate::docx::NoteKind::Foot, id, custom: false, run });
            let body = vec![Block::Para(p.clone())];
            f.refs.notes.push((crate::docx::NoteKind::Foot, id, body));
        }
        let mut b = ed(&table_xml(rows, cols, &t("y")));
        rect(&mut b, 0, 0, rows - 1, cols - 1, CellShape::Rect);
        let start = std::time::Instant::now();
        b.paste_fragment(&f, fonts());
        let took = start.elapsed();
        let (n, _) = refs(&b);
        assert_eq!(n.len(), rows * cols, "one reference a cell");
        assert_eq!(b.document().footnotes.len(), rows * cols, "one note each");
        took
    }
    let small = (0..2).map(|_| time(16, 16)).min().unwrap_or_default();
    let large = (0..2).map(|_| time(32, 32)).min().unwrap_or_default();
    // 4x the cells should cost about 4x; quadratic is about 16x.
    assert!(large < small * 10 + std::time::Duration::from_millis(50), "{small:?} for 256 cells, {large:?} for 1024");
}

#[test]
fn accepting_a_whole_table_revision_removes_its_deleted_cells() {
    let d = r#"w:id="1" w:author="a" w:date="2020-01-01T00:00:00Z""#;
    let cell = |text: &str, pr: &str| format!(r#"<w:tc><w:tcPr><w:tcW w:w="2000" w:type="dxa"/>{pr}</w:tcPr><w:p>{}</w:p></w:tc>"#, t(text));
    let table = format!(
        r#"<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w="2000"/><w:gridCol w:w="2000"/></w:tblGrid><w:tr>{}{}</w:tr><w:tr>{}{}</w:tr></w:tbl>"#,
        cell("keep", ""),
        cell("gone", &format!("<w:cellDel {d}/>")),
        cell("a", ""),
        cell("b", "")
    );
    let e = ed(&format!("{table}<w:p/>"));
    let blocks = e.document().sections[0].blocks[..1].to_vec();
    let f = crate::editing::clipboard::Fragment { blocks, ..Default::default() }.accepted();
    let [Block::Table(t)] = f.blocks.as_slice() else { panic!("a table: {:?}", f.blocks) };
    let text = |c: &crate::docx::Cell| {
        c.blocks
            .iter()
            .filter_map(|b| if let Block::Para(p) = b { Some(crate::editing::text::UnitText::of(p).text) } else { None })
            .collect::<String>()
    };
    let rows: Vec<Vec<String>> = t.rows.iter().map(|r| r.cells.iter().map(text).collect()).collect();
    assert_eq!(rows[0], ["keep"], "the deleted cell and its text are gone: {rows:?}");
    assert_eq!(rows[1], ["a", "b"]);
}

#[test]
fn a_copy_drops_a_comment_whose_start_was_in_a_deleted_cell() {
    use crate::docx::review::RangeKind;
    let d = r#"w:id="9" w:author="a" w:date="2020-01-01T00:00:00Z""#;
    let cell = |inner: &str, pr: &str| format!(r#"<w:tc><w:tcPr><w:tcW w:w="2000" w:type="dxa"/>{pr}</w:tcPr><w:p>{inner}</w:p></w:tc>"#);
    let whole = |id: i64, text: &str| {
        format!(r#"<w:commentRangeStart w:id="{id}"/>{}<w:commentRangeEnd w:id="{id}"/><w:r><w:commentReference w:id="{id}"/></w:r>"#, t(text))
    };
    let table = format!(
        r#"<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w="2000"/><w:gridCol w:w="2000"/></w:tblGrid><w:tr>{}{}</w:tr></w:tbl>"#,
        cell(&format!(r#"<w:commentRangeStart w:id="1"/>{}"#, t("gone")), &format!("<w:cellDel {d}/>")),
        cell(&format!(r#"{}<w:commentRangeEnd w:id="1"/><w:r><w:commentReference w:id="1"/></w:r>{}"#, t("kept"), whole(2, "own")), "")
    );
    use super::empty_stories::{NS, build};
    let comments: String = [1, 2]
        .map(|c| format!(r#"<w:comment w:id="{c}" w:author="A" w:date="2024-01-01T00:00:00Z"><w:p><w:r><w:t>c{c}</w:t></w:r></w:p></w:comment>"#))
        .concat();
    let xml = format!("<w:comments {NS}>{comments}</w:comments>");
    let mut e = Editor::open(&build(&format!("{table}<w:p/>"), "", &[("comments.xml", "comments", "comments", &xml)]), fonts()).expect("opens");
    rect(&mut e, 0, 0, 0, 1, CellShape::Rect);
    let f = e.copy().expect("copies");
    assert_eq!(f.refs.comments.iter().map(|c| c.id.0).collect::<Vec<_>>(), [2], "only the whole comment goes with it");
    let mut ids = Vec::new();
    fn walk(blocks: &[Block], ids: &mut Vec<i64>) {
        for b in blocks {
            match b {
                Block::Para(p) => ids.extend(p.inlines.iter().filter_map(|i| match i {
                    Inline::Range(r) if matches!(r.kind, RangeKind::CommentStart | RangeKind::CommentEnd) => Some(r.id),
                    Inline::CommentRef { id, .. } => Some(id.0),
                    _ => None,
                })),
                Block::Table(t) => t.rows.iter().flat_map(|r| &r.cells).for_each(|c| walk(&c.blocks, ids)),
            }
        }
    }
    walk(&f.blocks, &mut ids);
    assert_eq!(ids, [2, 2, 2], "the comment cut off by the deleted cell goes, the whole one stays");
}
