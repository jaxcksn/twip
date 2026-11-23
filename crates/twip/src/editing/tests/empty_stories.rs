//! Empty stories, tables without rows and dangling references

use super::super::Editor;
use super::super::commands::{Command, Outcome};
use super::super::position::{Position, StoryId};
use super::super::selection::{ObjectRef, Selection};
use super::super::text::UnitText;
use super::{fonts, para_id, texts};
use crate::api::command::{DeleteOptions, MoveSelection, MoveUnit, TableSize};
use crate::api::value::{Direction, Order};
use crate::docx::{Block, Inline};
use crate::layout::{FloatId, Unit};

const NS: &str = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape""#;
const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

/// The blank package with `body` and `sect` children for the section, plus parts `(file, rel type, content type, xml)`
/// related as `rIdP0`, `rIdP1`, ...
pub(super) fn build(body: &str, sect: &str, parts: &[(&str, &str, &str, &str)]) -> Vec<u8> {
    let blank = crate::docx::blank::blank_docx();
    let read = |name: &str| crate::docx::package::read(&blank, &[], name).expect("reads").expect(name);
    let mut rels = read("word/_rels/document.xml.rels");
    let mut types = read("[Content_Types].xml");
    let mut out: Vec<(String, Vec<u8>)> = Vec::new();
    for (i, (file, rel, ct, xml)) in parts.iter().enumerate() {
        rels = rels.replace("</Relationships>", &format!(r#"<Relationship Id="rIdP{i}" Type="{REL}/{rel}" Target="{file}"/></Relationships>"#));
        types = types.replace(
            "</Types>",
            &format!(
                r#"<Override PartName="/word/{file}" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.{ct}+xml"/></Types>"#
            ),
        );
        out.push((format!("word/{file}"), xml.as_bytes().to_vec()));
    }
    let document = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document {NS}><w:body>{body}<w:sectPr>{sect}<w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr></w:body></w:document>"#
    );
    out.push(("word/_rels/document.xml.rels".into(), rels.into_bytes()));
    out.push(("[Content_Types].xml".into(), types.into_bytes()));
    out.push(("word/document.xml".into(), document.into_bytes()));
    crate::docx::write::repackage(&blank, &out, zip::CompressionMethod::Deflated).expect("repackages")
}

fn open(bytes: &[u8]) -> Editor {
    let mut e = Editor::open(bytes, fonts()).expect("opens");
    e.complete(fonts());
    e
}

fn reopen(e: &Editor) -> Editor {
    open(&crate::docx::write::save(e.document()).expect("saves"))
}

fn p(text: &str) -> String {
    format!(r#"<w:p><w:r><w:t>{text}</w:t></w:r></w:p>"#)
}

fn text_of(blocks: &[Block]) -> String {
    let mut out = String::new();
    for b in blocks {
        match b {
            Block::Para(p) => out.push_str(&UnitText::of(p).text),
            Block::Table(t) => t.rows.iter().flat_map(|r| r.cells.iter()).for_each(|c| out.push_str(&text_of(&c.blocks))),
        }
    }
    out
}

fn header_text(e: &Editor) -> String {
    e.document().stories.values().map(|b| text_of(b)).collect()
}

fn floats(blocks: &[Block]) -> Vec<&crate::docx::Float> {
    blocks
        .iter()
        .filter_map(|b| if let Block::Para(p) = b { Some(p) } else { None })
        .flat_map(|p| p.inlines.iter())
        .filter_map(|i| if let Inline::Float(f, _) = i { Some(&**f) } else { None })
        .collect()
}

fn anchored_box(content: &str) -> String {
    let w = 12700 * 100;
    format!(
        r#"<w:r><w:drawing><wp:anchor distT="0" distB="0" distL="0" distR="0" simplePos="0" relativeHeight="5" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="page"><wp:posOffset>{w}</wp:posOffset></wp:positionH><wp:positionV relativeFrom="page"><wp:posOffset>{w}</wp:posOffset></wp:positionV><wp:extent cx="{w}" cy="{w}"/><wp:effectExtent l="0" t="0" r="0" b="0"/><wp:wrapNone/><wp:docPr id="7" name="S7"/><a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wps:wsp><wps:cNvSpPr/><wps:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="{w}" cy="{w}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></wps:spPr><wps:txbx>{content}</wps:txbx><wps:bodyPr/></wps:wsp></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r>"#
    )
}

fn row_less_table_in_cell(first: bool) -> (Editor, crate::layout::ParaId) {
    let empty = r#"<w:tbl><w:tblPr/><w:tblGrid/></w:tbl><w:p/>"#;
    let cells =
        if first { format!("<w:tc>{empty}</w:tc><w:tc>{}</w:tc>", p("two")) } else { format!("<w:tc>{}</w:tc><w:tc>{empty}</w:tc>", p("one")) };
    let body = format!(
        r#"<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w="4000"/><w:gridCol w:w="4000"/></w:tblGrid><w:tr>{cells}</w:tr></w:tbl>{}"#,
        p("after")
    );
    let e = open(&build(&body, "", &[]));
    let Some(Block::Table(t)) = e.document().sections[0].blocks.first() else { panic!("a table") };
    assert!(t.rows[0].cells.iter().all(|c| c.blocks.iter().all(|b| matches!(b, Block::Para(_)))), "the table without rows is dropped");
    let at = texts(&e).iter().position(|t| t == if first { "two" } else { "one" }).expect("the text cell");
    let id = para_id(&e, at);
    (e, id)
}

#[test]
fn tab_into_a_cell_holding_a_table_without_rows_enters_the_cell() {
    let (mut e, one) = row_less_table_in_cell(false);
    e.selection = Selection::Caret(Position::text(StoryId::Body, one, Unit(0)));
    e.execute(Command::MoveToCell(Order::Next), fonts());
    assert_eq!(e.selection().focus().and_then(|f| f.para_offset()).map(|(p, _)| p), Some(para_id(&e, 1)), "{:?}", e.selection());
}

#[test]
fn shift_tab_into_a_cell_holding_a_table_without_rows_enters_the_cell() {
    let (mut e, two) = row_less_table_in_cell(true);
    e.selection = Selection::Caret(Position::text(StoryId::Body, two, Unit(0)));
    e.execute(Command::MoveToCell(Order::Previous), fonts());
    assert_eq!(e.selection().focus().and_then(|f| f.para_offset()).map(|(p, _)| p), Some(para_id(&e, 0)), "{:?}", e.selection());
}

#[test]
fn a_body_with_no_blocks_takes_typing_enter_and_tables() {
    let bytes = build("", "", &[]);
    for c in [Command::InsertText("a".into()), Command::InsertParagraph, Command::InsertTable(TableSize { rows: 1, columns: 1 })] {
        let mut e = open(&bytes);
        let out = e.execute(c.clone(), fonts());
        assert!(matches!(out, Outcome::Changed(_)), "{c:?}: {out:?}");
    }
    let mut e = open(&bytes);
    e.execute(Command::InsertText("a".into()), fonts());
    assert_eq!(texts(&e), ["a"]);
    assert_eq!(texts(&reopen(&e)), ["a"]);
}

#[test]
fn typing_after_a_body_s_closing_table_saves() {
    let body = r#"<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w="4000"/></w:tblGrid><w:tr><w:tc><w:p/></w:tc></w:tr></w:tbl>"#;
    let mut e = open(&build(body, "", &[]));
    e.execute(Command::MoveSelection(MoveSelection { by: MoveUnit::Document, direction: Direction::Forward, extend: None }), fonts());
    let out = e.execute(Command::InsertText("zz".into()), fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    assert_eq!(texts(&e), ["", "zz"]);
    assert_eq!(texts(&reopen(&e)), ["", "zz"]);
}

#[test]
fn edit_header_enters_a_header_part_without_a_paragraph() {
    let table =
        format!(r#"<w:hdr {NS}><w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w="2000"/></w:tblGrid><w:tr><w:tc><w:p/></w:tc></w:tr></w:tbl></w:hdr>"#);
    for hdr in [format!("<w:hdr {NS}/>"), table] {
        let bytes = build(&p("Body"), r#"<w:headerReference w:type="default" r:id="rIdP0"/>"#, &[("header1.xml", "header", "header", &hdr)]);
        let mut e = open(&bytes);
        let out = e.execute(Command::EditHeader, fonts());
        let story = e.selection().focus().map(|p| p.story);
        assert!(matches!(story, Some(StoryId::HeaderFooter(_))), "caret not in the header: {out:?} {story:?}");
        // Ctrl+End reaches the paragraph after a closing table too.
        e.execute(Command::MoveSelection(MoveSelection { by: MoveUnit::Document, direction: Direction::Forward, extend: None }), fonts());
        let out = e.execute(Command::InsertText("Head".into()), fonts());
        assert!(matches!(out, Outcome::Changed(_)), "{hdr}: {out:?}");
        assert_eq!(header_text(&e), "Head");
        assert_eq!(header_text(&reopen(&e)), "Head", "{hdr}");
    }
}

#[test]
fn jumping_to_an_empty_footnote_moves_the_caret_into_it() {
    let foot = format!(r#"<w:footnotes {NS}><w:footnote w:id="1"/></w:footnotes>"#);
    let body = r#"<w:p><w:r><w:t>a</w:t></w:r><w:r><w:footnoteReference w:id="1"/></w:r></w:p>"#;
    let bytes = build(body, "", &[("footnotes.xml", "footnotes", "footnotes", &foot)]);
    let mut e = open(&bytes);
    let id = para_id(&e, 0);
    let moved = [1, 2].into_iter().any(|u| e.note_jump(Position::text(StoryId::Body, id, Unit(u))));
    assert!(moved, "note_jump did nothing");
    assert!(matches!(e.selection().focus().map(|p| p.story), Some(StoryId::Footnote(1))), "{:?}", e.selection());
    let out = e.execute(Command::InsertText("note".into()), fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    assert_eq!(text_of(&reopen(&e).document().footnotes[&1]), "note");
}

#[test]
fn typing_into_a_selected_empty_text_box_fills_it() {
    let body = format!("<w:p><w:r><w:t>Text</w:t></w:r>{}</w:p>", anchored_box("<w:txbxContent/>"));
    let mut e = open(&build(&body, "", &[]));
    let f = floats(&e.document().sections[0].blocks)[0].id;
    e.selection = Selection::Object(ObjectRef::Floating(FloatId(f)));
    let out = e.execute(Command::InsertText("in".into()), fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    let box_text = |e: &Editor| text_of(&floats(&e.document().sections[0].blocks)[0].text_box.as_ref().expect("a text box").blocks);
    assert_eq!((box_text(&e), texts(&e)), ("in".to_string(), vec!["Text".to_string()]));
    assert_eq!(box_text(&reopen(&e)), "in");
}

#[test]
fn typing_into_a_selected_emptied_text_box_fills_it() {
    let body = format!("<w:p><w:r><w:t>Text</w:t></w:r>{}</w:p>", anchored_box("<w:txbxContent><w:p><w:r><w:t>x</w:t></w:r></w:p></w:txbxContent>"));
    let mut e = open(&build(&body, "", &[]));
    let f = FloatId(floats(&e.document().sections[0].blocks)[0].id);
    let para = floats(&e.document().sections[0].blocks)[0].text_box.as_ref().and_then(|t| t.blocks.first()).map(|b| match b {
        crate::docx::Block::Para(p) => crate::layout::ParaId(p.id),
        crate::docx::Block::Table(_) => panic!("a paragraph"),
    });
    let para = para.expect("the box's paragraph");
    e.selection = Selection::Caret(Position::text(StoryId::TextBox(f), para, Unit(1)));
    assert!(matches!(e.execute(Command::InsertParagraph, fonts()), Outcome::Changed(_)));
    e.selection = Selection::Caret(Position::text(StoryId::TextBox(f), para, Unit(0)));
    for _ in 0..2 {
        assert!(matches!(e.execute(Command::Delete(DeleteOptions { direction: Direction::Forward, unit: None }), fonts()), Outcome::Changed(_)));
    }
    let tb = floats(&e.document().sections[0].blocks)[0].text_box.clone().expect("a text box");
    let [Block::Para(p)] = &tb.blocks[..] else { panic!("one paragraph: {:?}", tb.blocks.len()) };
    assert!(!p.inlines.is_empty() && p.inlines.iter().all(|i| matches!(i, Inline::Text(t, _) if t.is_empty())), "{:?}", p.inlines);
    e.selection = Selection::Object(ObjectRef::Floating(f));
    let out = e.execute(Command::InsertText("in".into()), fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    let box_text = |e: &Editor| text_of(&floats(&e.document().sections[0].blocks)[0].text_box.as_ref().expect("a text box").blocks);
    assert_eq!((box_text(&e), texts(&e)), ("in".to_string(), vec!["Text".to_string()]));
}

#[test]
fn comment_anchor_text_is_cut_to_its_limit() {
    use crate::api::review::ANCHOR_TEXT_LIMIT;
    const N: usize = 60;
    let line = "x".repeat(200);
    let mut body = String::new();
    for i in 0..300 {
        let (s, en) = match i {
            0 => ((0..N).map(|c| format!(r#"<w:commentRangeStart w:id="{c}"/>"#)).collect::<String>(), String::new()),
            299 => (
                String::new(),
                (0..N).map(|c| format!(r#"<w:commentRangeEnd w:id="{c}"/><w:r><w:commentReference w:id="{c}"/></w:r>"#)).collect::<String>(),
            ),
            _ => (String::new(), String::new()),
        };
        body.push_str(&format!(r#"<w:p>{s}<w:r><w:t>{line}</w:t></w:r>{en}</w:p>"#));
    }
    body.push_str(r#"<w:p><w:commentRangeStart w:id="99"/><w:r><w:t>short</w:t></w:r><w:commentRangeEnd w:id="99"/><w:r><w:commentReference w:id="99"/></w:r></w:p>"#);
    let comments: String = (0..N)
        .chain([99])
        .map(|c| format!(r#"<w:comment w:id="{c}" w:author="A" w:date="2024-01-01T00:00:00Z"><w:p><w:r><w:t>c{c}</w:t></w:r></w:p></w:comment>"#))
        .collect();
    let xml = format!("<w:comments {NS}>{comments}</w:comments>");
    let bytes = build(&body, "", &[("comments.xml", "comments", "comments", &xml)]);
    let mut engine = crate::api::engine::Engine::new(crate::api::fonts::FontLibrary::empty());
    let id = engine.create_session(Default::default());
    let s = engine.session(id).expect("a session");
    s.open(&bytes).expect("opens");
    let threads = s.comments();
    assert_eq!(threads.len(), N + 1);
    let cut: String = format!("{line}\n").repeat(ANCHOR_TEXT_LIMIT / 200 + 1).chars().take(ANCHOR_TEXT_LIMIT).collect();
    for t in &threads[..N] {
        assert_eq!(t.anchor_text, cut);
    }
    assert_eq!(threads[N].anchor_text, "short");
}

#[test]
fn a_new_footnote_takes_a_number_no_reference_uses() {
    let body = r#"<w:p><w:r><w:t>a</w:t></w:r><w:r><w:footnoteReference w:id="1"/></w:r></w:p><w:p><w:r><w:t>second</w:t></w:r></w:p>"#;
    let mut e = open(&build(body, "", &[]));
    assert!(e.document().footnotes.is_empty());
    let second = para_id(&e, 1);
    e.selection = Selection::Caret(Position::text(StoryId::Body, second, Unit(2)));
    let out = e.execute(Command::InsertFootnote, fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    let refs: Vec<(i64, bool)> = e.document().sections[0]
        .blocks
        .iter()
        .filter_map(|b| if let Block::Para(p) = b { Some(p) } else { None })
        .flat_map(|p| p.inlines.iter())
        .filter_map(|i| if let Inline::NoteRef { id, .. } = i { Some((*id, e.document().footnotes.contains_key(id))) } else { None })
        .collect();
    assert_eq!(refs, [(1, false), (2, true)], "the reference without a note keeps none");
}

#[test]
fn deleting_a_text_box_drops_the_notes_referenced_inside_it() {
    let boxed = anchored_box(r#"<w:txbxContent><w:p><w:r><w:t>in</w:t></w:r><w:r><w:footnoteReference w:id="2"/></w:r></w:p></w:txbxContent>"#);
    let foot = format!(r#"<w:footnotes {NS}><w:footnote w:id="2"><w:p><w:r><w:t>note</w:t></w:r></w:p></w:footnote></w:footnotes>"#);
    let body = format!("<w:p><w:r><w:t>Text</w:t></w:r>{boxed}</w:p>");
    let mut e = open(&build(&body, "", &[("footnotes.xml", "footnotes", "footnotes", &foot)]));
    assert!(e.document().footnotes.contains_key(&2), "the note reads");
    let f = floats(&e.document().sections[0].blocks)[0].id;
    e.selection = Selection::Object(ObjectRef::Floating(FloatId(f)));
    let out = e.execute(Command::Delete(DeleteOptions { direction: Direction::Forward, unit: None }), fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    assert!(!e.document().footnotes.contains_key(&2), "the note outlives its only reference");
    e.execute(Command::Undo, fonts());
    assert!(e.document().footnotes.contains_key(&2), "undo restores the note");
}
