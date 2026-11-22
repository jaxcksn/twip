//! Character and paragraph formatting: what it changes, what it keeps

use std::io::Read;

use super::super::Editor;
use super::super::commands::Command;
use super::super::position::{Position, StoryId};
use super::super::selection::{CellRef, CellShape, Selection};
use super::{blank_body, fonts, texts_ids};
use crate::api::command::{AddTabStop, Command as C, Indents};
use crate::api::value::{Alignment, Points};
use crate::docx::{Block, Inline};
use crate::layout::{TableId, Unit};

fn doc_xml(e: &Editor) -> String {
    let bytes = crate::docx::write::save(e.document()).expect("saves");
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut s = String::new();
    z.by_name("word/document.xml").unwrap().read_to_string(&mut s).unwrap();
    s
}

fn edit(body: &str) -> Editor {
    let mut e = Editor::open(&blank_body(body), fonts()).expect("opens");
    e.complete(fonts());
    e
}

fn first_para(e: &Editor) -> &crate::docx::Paragraph {
    e.document().sections[0].blocks.iter().find_map(|b| if let Block::Para(p) = b { Some(p) } else { None }).expect("a paragraph")
}

const DEL: &str = r#"<w:del w:id="91" w:author="B" w:date="2020-01-01T00:00:00Z"><w:r><w:delText>gone</w:delText></w:r></w:del>"#;

#[test]
fn untracked_bold_keeps_another_authors_deletion() {
    let mut e = edit(&format!("<w:p><w:r><w:t>keep </w:t></w:r>{DEL}</w:p>"));
    assert!(doc_xml(&e).contains("<w:del "), "the fixture has a deletion");
    e.execute(Command::SelectAll, fonts());
    e.execute(C::SetBold(true), fonts());
    let deleted = first_para(&e).inlines.iter().filter_map(|i| i.run()).filter(|r| r.rev.del.is_some()).count();
    assert_eq!(deleted, 1, "the deleted run stays deleted");
    assert!(doc_xml(&e).contains("<w:del "));
}

#[test]
fn untracked_alignment_keeps_a_tracked_paragraph_property_change() {
    let mut e = edit(
        r#"<w:p><w:pPr><w:jc w:val="center"/><w:pPrChange w:id="5" w:author="B" w:date="2020-01-01T00:00:00Z"><w:pPr/></w:pPrChange></w:pPr><w:r><w:t>x</w:t></w:r></w:p>"#,
    );
    assert!(first_para(&e).style.change.is_some(), "the fixture has a property change");
    e.execute(C::SetAlignment(Alignment::Right), fonts());
    assert!(first_para(&e).style.change.is_some(), "the change stays");
    assert!(doc_xml(&e).contains("pPrChange"));
}

#[test]
fn formatting_a_table_of_contents_entry_keeps_it_one() {
    let mut e = edit(
        r#"<w:p><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> TOC \o "1-3" \h </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:rPr><w:rStyle w:val="Hyperlink"/></w:rPr><w:t>Entry</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r></w:p>"#,
    );
    let toc = |e: &Editor| first_para(e).inlines.iter().find_map(|i| if let Inline::Text(t, r) = i { (t == "Entry").then_some(r.toc) } else { None });
    assert_eq!(toc(&e), Some(true), "a table of contents' entry");
    e.execute(Command::SelectAll, fonts());
    e.execute(C::SetBold(true), fonts());
    assert_eq!(toc(&e), Some(true));
}

#[test]
fn a_range_formats_the_marks_it_holds_and_empty_paragraphs() {
    let mut e = edit("<w:p><w:r><w:t>a</w:t></w:r></w:p><w:p/><w:p><w:r><w:t>b</w:t></w:r></w:p>");
    e.execute(Command::SelectAll, fonts());
    e.execute(C::SetFontSize(Points(24.0)), fonts());
    let ids = texts_ids(&e);
    let mark = |e: &Editor, i: usize| e.index().para(e.document(), ids[i]).expect("para").mark.size;
    assert_eq!((mark(&e, 0), mark(&e, 1), mark(&e, 2)), (24.0, 24.0, 24.0));
    e.execute(Command::Undo, fonts());
    assert_eq!(mark(&e, 1), 11.0, "one undo step");
}

#[test]
fn a_range_ending_inside_a_paragraph_leaves_its_mark() {
    let mut e = edit("<w:p><w:r><w:t>ab</w:t></w:r></w:p><w:p><w:r><w:t>cd</w:t></w:r></w:p>");
    let ids = texts_ids(&e);
    e.selection =
        Selection::range(Position::text(StoryId::Body, ids[0], Unit(1)), Position::text(StoryId::Body, ids[1], Unit(1)), Default::default());
    e.execute(C::SetFontSize(Points(24.0)), fonts());
    let mark = |i: usize| e.index().para(e.document(), ids[i]).expect("para").mark.size;
    assert_eq!((mark(0), mark(1)), (24.0, 11.0), "the first mark is selected, the second isn't");
}

#[test]
fn indents_drop_the_character_unit_ones_they_replace() {
    let mut e = edit(
        r#"<w:p><w:pPr><w:ind w:firstLineChars="200" w:firstLine="480"/><w:spacing w:beforeLines="50" w:before="120"/></w:pPr><w:r><w:t>x</w:t></w:r></w:p>"#,
    );
    e.execute(C::SetIndents(Indents { left: None, right: None, first_line: Some(Points(36.0)), hanging: None, mirror: None }), fonts());
    let x = doc_xml(&e);
    assert!(x.contains("w:firstLine=\"720\"") && !x.contains("firstLineChars"), "{x}");
    let spacing = crate::api::command::ParagraphSpacing { before: Some(crate::api::value::AutoOr::Value(Points(12.0))), ..Default::default() };
    e.execute(C::SetParagraphSpacing(spacing), fonts());
    let x = doc_xml(&e);
    assert!(x.contains("w:before=\"240\"") && !x.contains("beforeLines"), "{x}");
}

#[test]
fn a_range_to_the_next_paragraphs_start_formats_the_mark_it_passes() {
    let mut e = edit("<w:p><w:r><w:t>ab</w:t></w:r></w:p><w:p><w:r><w:t>cd</w:t></w:r></w:p>");
    let ids = texts_ids(&e);
    e.selection =
        Selection::range(Position::text(StoryId::Body, ids[0], Unit(0)), Position::text(StoryId::Body, ids[1], Unit(0)), Default::default());
    e.execute(C::SetFontSize(Points(24.0)), fonts());
    let mark = |i: usize| e.index().para(e.document(), ids[i]).expect("para").mark.size;
    assert_eq!((mark(0), mark(1)), (24.0, 11.0));
}

const TABLE: &str = r#"<w:tbl><w:tblPr><w:tblW w:w="0" w:type="auto"/></w:tblPr><w:tblGrid><w:gridCol w:w="2000"/><w:gridCol w:w="2000"/></w:tblGrid><w:tr><w:tc><w:p><w:r><w:t>a</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>b</w:t></w:r></w:p></w:tc></w:tr><w:tr><w:tc><w:p><w:r><w:t>c</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>d</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p/>"#;

fn cell_runs(e: &Editor) -> Vec<(String, bool)> {
    let t = e.index().table_ids().next().expect("a table");
    let t = e.index().table(e.document(), t).expect("the table");
    t.rows
        .iter()
        .flat_map(|r| &r.cells)
        .flat_map(|c| &c.blocks)
        .filter_map(|b| if let Block::Para(p) = b { Some(p) } else { None })
        .flat_map(|p| p.inlines.iter())
        .filter_map(|i| if let Inline::Text(t, r) = i { Some((t.clone(), r.bold)) } else { None })
        .collect()
}

fn cells(e: &Editor, from: (usize, usize), to: (usize, usize)) -> Selection {
    let table: TableId = e.index().table_ids().next().expect("a table");
    Selection::Cells { table, anchor: CellRef { row: from.0, col: from.1 }, focus: CellRef { row: to.0, col: to.1 }, shape: CellShape::Rect }
}

#[test]
fn selected_cells_take_character_and_paragraph_formatting() {
    let mut e = edit(TABLE);
    e.selection = cells(&e, (0, 1), (1, 1));
    let f = e.format_state();
    assert!(e.command_states(f.as_ref(), super::super::state::StateFlags { read_only: false })[&C::SetBold(true).id()].enabled, "bold applies");
    e.execute(C::SetBold(true), fonts());
    let runs = cell_runs(&e);
    assert_eq!(runs, vec![("a".into(), false), ("b".into(), true), ("c".into(), false), ("d".into(), true)]);
    e.execute(C::SetAlignment(Alignment::Center), fonts());
    let t = e.index().table_ids().next().expect("a table");
    let jc: Vec<_> = e.index().table(e.document(), t).expect("the table").rows[0]
        .cells
        .iter()
        .map(|c| if let Block::Para(p) = &c.blocks[0] { p.style.jc } else { unreachable!() })
        .collect();
    assert_eq!(jc, vec![crate::docx::props::Jc::Left, crate::docx::props::Jc::Center]);
    e.execute(Command::Undo, fonts());
    e.execute(Command::Undo, fonts());
    assert!(cell_runs(&e).iter().all(|r| !r.1), "one undo step each");
}

const ANCHORED: &str = r#"<w:p><w:r><w:t>a</w:t></w:r><w:r><w:rPr><w:rFonts w:ascii="Arial" w:hAnsi="Arial"/></w:rPr><w:drawing><wp:anchor xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" distT="0" distB="0" distL="0" distR="0" simplePos="0" relativeHeight="1" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="column"><wp:posOffset>0</wp:posOffset></wp:positionH><wp:positionV relativeFrom="paragraph"><wp:posOffset>0</wp:posOffset></wp:positionV><wp:extent cx="914400" cy="914400"/><wp:wrapNone/><wp:docPr id="1" name="Shape 1"/><a:graphic xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wps:wsp xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wps:spPr><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></wps:spPr><wps:bodyPr/></wps:wsp></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r><w:r><w:t>b</w:t></w:r></w:p>"#;

#[test]
fn replacing_a_font_in_the_document_keeps_one_of_each_anchored_object() {
    let mut e = edit(ANCHORED);
    let floats = |e: &Editor| first_para(e).inlines.iter().filter(|i| matches!(i, Inline::Float(..))).count();
    let font = |e: &Editor| first_para(e).inlines.iter().find_map(|i| if let Inline::Float(_, r) = i { Some(r.ascii.clone()) } else { None });
    assert_eq!((floats(&e), font(&e).as_deref()), (1, Some("Arial")), "the fixture's object, in Arial");
    let r = crate::api::command::ReplaceFont {
        from: "Arial".into(),
        to: "Calibri".into(),
        scope: crate::api::command::ReplaceScope::Document,
        bold: None,
        italic: None,
    };
    e.execute(C::ReplaceFont(r), fonts());
    assert_eq!((floats(&e), font(&e).as_deref()), (1, Some("Calibri")));
    e.execute(Command::Undo, fonts());
    assert_eq!((floats(&e), font(&e).as_deref()), (1, Some("Arial")), "one undo step");
}

const RUBY: &str = r#"<w:ruby><w:rubyPr><w:rubyAlign w:val="center"/><w:hps w:val="10"/><w:hpsRaise w:val="18"/><w:hpsBaseText w:val="20"/><w:lid w:val="ja-JP"/></w:rubyPr><w:rt><w:r><w:t>kan</w:t></w:r></w:rt><w:rubyBase><w:r><w:t>K</w:t></w:r></w:rubyBase></w:ruby>"#;

fn ruby(e: &Editor) -> (std::rc::Rc<crate::docx::Ruby>, crate::docx::Run) {
    e.index()
        .para_ids()
        .filter_map(|id| e.index().para(e.document(), id))
        .flat_map(|p| p.inlines.iter())
        .find_map(|i| if let Inline::Ruby(r, run) = i { Some((r.clone(), run.clone())) } else { None })
        .expect("a ruby")
}

#[test]
fn bold_reaches_a_rubys_base_text_and_saves() {
    let mut e = edit(&format!(r#"<w:p><w:r><w:t>ab</w:t></w:r><w:r>{RUBY}</w:r></w:p>"#));
    e.execute(Command::SelectAll, fonts());
    e.execute(C::SetBold(true), fonts());
    let (r, _) = ruby(&e);
    assert!(r.base.iter().all(|(_, run)| run.bold), "the base text is bold");
    assert!(r.text.iter().all(|(_, run)| !run.bold), "the annotation keeps its own formatting");
    let again = Editor::open(&crate::docx::write::save(e.document()).expect("saves"), fonts()).expect("reads");
    let (r, _) = ruby(&again);
    assert!(r.base.iter().all(|(_, run)| run.bold), "as saved");
}

#[test]
fn accepting_every_revision_clears_a_tracked_ruby() {
    let mut e =
        edit(&format!(r#"<w:p><w:r><w:t>ab</w:t></w:r><w:ins w:id="1" w:author="a" w:date="2024-01-01T00:00:00Z"><w:r>{RUBY}</w:r></w:ins></w:p>"#));
    assert!(ruby(&e).1.rev.ins.is_some(), "the fixture's ruby is inserted");
    e.execute(Command::AcceptAllRevisions(Default::default()), fonts());
    assert!(ruby(&e).1.rev.ins.is_none());
}

#[test]
fn a_tracked_delete_over_a_ruby_marks_it_deleted() {
    let mut e = edit(&format!(r#"<w:p><w:r><w:t>ab</w:t></w:r><w:r>{RUBY}</w:r><w:r><w:t>cd</w:t></w:r></w:p>"#));
    e.execute(Command::SetTrackChanges(true), fonts());
    e.execute(Command::SelectAll, fonts());
    e.execute(super::del(super::super::commands::Dir::Back, false), fonts());
    assert!(ruby(&e).1.rev.del.is_some());
}

const AUTO: &str = r#"<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style><w:style w:type="paragraph" w:styleId="Auto"><w:name w:val="Auto"/><w:basedOn w:val="Normal"/><w:autoRedefine/></w:style>"#;

#[test]
fn tab_stops_in_a_style_that_updates_itself_change_the_style() {
    let body = r#"<w:p><w:pPr><w:pStyle w:val="Auto"/></w:pPr><w:r><w:t>x</w:t></w:r></w:p>"#;
    let mut e = super::style_model::open_pkg(&super::style_model::pkg(AUTO, body));
    let undo = e.history().undo_depth();
    let out = e.execute(C::AddTabStop(AddTabStop { position: Points(144.0), alignment: None, leader: None }), fonts());
    assert!(matches!(out, super::super::commands::Outcome::Changed(_)), "{out:?}");
    let raw = &e.document().resolver.as_ref().expect("styles").styles.get("Auto").expect("Auto").raw;
    assert!(raw.contains("w:pos=\"2880\""), "{raw}");
    assert_eq!(e.history().undo_depth(), undo + 1);
    assert!(first_para(&e).style.tabs.iter().any(|t| (t.pos - 144.0).abs() < 0.01), "the paragraph takes it");
}

#[test]
fn formatting_a_self_updating_style_already_has_records_nothing() {
    let body = r#"<w:p><w:pPr><w:pStyle w:val="Auto"/></w:pPr><w:r><w:t>x</w:t></w:r></w:p>"#;
    let mut e = super::style_model::open_pkg(&super::style_model::pkg(AUTO, body));
    let undo = e.history().undo_depth();
    let out = e.execute(C::SetAlignment(Alignment::Left), fonts());
    assert!(matches!(out, super::super::commands::Outcome::Unchanged(_)), "{out:?}");
    assert_eq!(e.history().undo_depth(), undo);
}

#[test]
fn a_font_size_past_words_limit_is_refused_wherever_it_comes_from() {
    let mut e = edit("<w:p><w:r><w:t>x</w:t></w:r></w:p>");
    let d = crate::api::command::DocumentDefaults { size: Some(Points(1e30)), ..Default::default() };
    assert!(matches!(e.execute(C::SetDocumentDefaults(d), fonts()), super::super::commands::Outcome::Refused(_)));
    let normal = e.document().resolver.as_ref().expect("styles").styles.get("Normal").expect("Normal").raw.clone();
    assert!(e.format_style(&normal, vec![C::SetFontSize(Points(1e12))], fonts()).is_err());
    let line =
        crate::api::command::Border { style: crate::api::command::BorderStyle::Single, width: Points(1e6), color: crate::api::value::AutoOr::Auto };
    let b = crate::api::command::TextBorderChange { border: Some(line), shadow: false };
    e.execute(Command::SelectAll, fonts());
    assert!(matches!(e.execute(C::SetTextBorder(b), fonts()), super::super::commands::Outcome::Refused(_)));
}

#[test]
fn growing_a_font_stops_at_words_largest_size() {
    let mut e = edit(r#"<w:p><w:r><w:rPr><w:sz w:val="3270"/></w:rPr><w:t>x</w:t></w:r></w:p>"#);
    e.execute(Command::SelectAll, fonts());
    for _ in 0..3 {
        e.execute(C::GrowFont, fonts());
    }
    let size = first_para(&e).inlines.iter().find_map(|i| if let Inline::Text(_, r) = i { Some(r.size) } else { None });
    assert_eq!(size, Some(1638.0));
}

fn text_runs(e: &Editor, para: usize) -> Vec<(String, bool, bool, Option<u32>)> {
    let id = texts_ids(e)[para];
    e.index()
        .para(e.document(), id)
        .expect("para")
        .inlines
        .iter()
        .filter_map(|i| if let Inline::Text(t, r) = i { Some((t.clone(), r.bold, r.italic, r.color)) } else { None })
        .collect()
}

#[test]
fn the_format_painter_gives_text_another_runs_formatting_in_place_of_its_own() {
    let mut e = edit(
        r#"<w:p><w:r><w:rPr><w:b/><w:color w:val="C00000"/></w:rPr><w:t>red</w:t></w:r></w:p><w:p><w:r><w:rPr><w:i/></w:rPr><w:t>plain</w:t></w:r></w:p>"#,
    );
    let ids = texts_ids(&e);
    e.selection =
        Selection::range(Position::text(StoryId::Body, ids[0], Unit(0)), Position::text(StoryId::Body, ids[0], Unit(2)), Default::default());
    let f = e.format_state();
    assert!(
        !e.command_states(f.as_ref(), super::super::state::StateFlags { read_only: false })[&C::PasteFormatting.id()].enabled,
        "nothing held yet"
    );
    e.execute(C::CopyFormatting, fonts());
    e.selection =
        Selection::range(Position::text(StoryId::Body, ids[1], Unit(0)), Position::text(StoryId::Body, ids[1], Unit(5)), Default::default());
    let undo = e.history().undo_depth();
    assert!(matches!(e.execute(C::PasteFormatting, fonts()), super::super::commands::Outcome::Changed(_)));
    assert_eq!(text_runs(&e, 1), vec![("plain".into(), true, false, Some(0xC00000))], "bold and red, no longer italic");
    assert_eq!(e.history().undo_depth(), undo + 1);
    assert_eq!(first_para(&e).style.jc, crate::docx::props::Jc::Left, "part of a paragraph holds no paragraph formatting");
    e.execute(Command::Undo, fonts());
    assert_eq!(text_runs(&e, 1), vec![("plain".into(), false, true, None)]);
}

#[test]
fn the_format_painter_takes_a_whole_paragraphs_formatting_too() {
    let mut e = edit(
        r#"<w:p><w:pPr><w:jc w:val="center"/><w:spacing w:after="480"/></w:pPr><w:r><w:t>one</w:t></w:r></w:p><w:p><w:r><w:t>two</w:t></w:r></w:p><w:p><w:r><w:t>three</w:t></w:r></w:p>"#,
    );
    let ids = texts_ids(&e);
    e.selection =
        Selection::range(Position::text(StoryId::Body, ids[0], Unit(0)), Position::text(StoryId::Body, ids[1], Unit(0)), Default::default());
    e.execute(C::CopyFormatting, fonts());
    e.selection = Selection::Caret(Position::text(StoryId::Body, ids[2], Unit(1)));
    e.execute(C::PasteFormatting, fonts());
    let p = e.index().para(e.document(), ids[2]).expect("para");
    assert_eq!((p.style.jc, p.style.space_after), (crate::docx::props::Jc::Center, 24.0));
    assert!(doc_xml(&e).matches("<w:jc w:val=\"center\"/>").count() == 2);
}
