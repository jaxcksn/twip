//! Sections, headers and footers, notes and frames the commands act on

use super::super::Editor;
use super::super::commands::{Command, Dir, MoveBy, Outcome};
use super::super::position::{Position, StoryId};
use super::super::selection::{CellRef, CellShape, ObjectRef, Selection};
use super::super::text::UnitText;
use super::{fonts, mv, open, para_id, texts};
use crate::api::command::{DropCap, DropCapPosition};
use crate::api::value::Orientation;
use crate::docx::{Block, Inline};
use crate::layout::{FloatId, ParaId, TableId, Unit};

const NS: &str = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships""#;
const SEC: &str = r#"<w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/>"#;
const HEADER_REF: &str = r#"<w:headerReference w:type="default" r:id="rIdH"/>"#;

/// A blank package with `body` as the document; `header` relates `header1.xml` as rIdH, the part itself only when
/// `Some(Some(xml))`.
pub(super) fn package(body: &str, header: Option<Option<&str>>) -> Editor {
    let blank = crate::docx::blank::blank_docx();
    let read = |name: &str| crate::docx::package::read(&blank, &[], name).expect("reads").expect(name);
    let document = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document {NS}><w:body>{body}</w:body></w:document>"#);
    let mut parts = vec![("word/document.xml".to_string(), document.into_bytes())];
    if let Some(h) = header {
        let rels = read("word/_rels/document.xml.rels").replace(
            "</Relationships>",
            r#"<Relationship Id="rIdH" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/header" Target="header1.xml"/></Relationships>"#,
        );
        parts.push(("word/_rels/document.xml.rels".into(), rels.into_bytes()));
        if let Some(xml) = h {
            let types = read("[Content_Types].xml").replace(
                "</Types>",
                r#"<Override PartName="/word/header1.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml"/></Types>"#,
            );
            parts.push(("[Content_Types].xml".into(), types.into_bytes()));
            let part = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:hdr {NS}>{xml}</w:hdr>"#);
            parts.push(("word/header1.xml".into(), part.into_bytes()));
        }
    }
    let bytes = crate::docx::write::repackage(&blank, &parts, zip::CompressionMethod::Deflated).expect("repackages");
    let mut e = Editor::open(&bytes, fonts()).expect("opens");
    e.complete(fonts());
    e
}

fn p(t: &str) -> String {
    format!("<w:p><w:r><w:t>{t}</w:t></w:r></w:p>")
}

fn sect_p(t: &str, refs: &str) -> String {
    format!("<w:p><w:pPr><w:sectPr>{refs}{SEC}</w:sectPr></w:pPr><w:r><w:t>{t}</w:t></w:r></w:p>")
}

fn table(t: &str) -> String {
    format!(
        "<w:tbl><w:tblGrid><w:gridCol w:w=\"4000\"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w=\"4000\" w:type=\"dxa\"/></w:tcPr>{}</w:tc></w:tr></w:tbl>",
        p(t)
    )
}

const HEADER: &str = "<w:p><w:r><w:t>Hdr</w:t></w:r></w:p>";

fn para_of(e: &Editor, prefix: &str) -> ParaId {
    super::texts_ids(e)
        .into_iter()
        .find(|&q| e.index().para(e.document(), q).is_some_and(|x| UnitText::of(x).text.starts_with(prefix)))
        .unwrap_or_else(|| panic!("no paragraph {prefix}"))
}

fn caret(e: &mut Editor, story: StoryId, para: ParaId, at: u32) {
    e.set_caret_for_benchmark(Selection::Caret(Position::text(story, para, Unit(at))));
}

fn three_sections(tail: &str) -> Editor {
    package(&format!("{}{}{}<w:sectPr>{SEC}</w:sectPr>", sect_p("One", ""), sect_p("Two", ""), tail), None)
}

fn landscape(e: &Editor, s: usize) -> bool {
    let sec = &e.document().sections[s];
    sec.page_w > sec.page_h
}

fn violations(e: &Editor) -> Vec<String> {
    super::super::integrity::check(e.document(), e.index(), &e.selection()).iter().map(|v| format!("{v:?}")).collect()
}

fn table_id(e: &Editor, section: usize) -> TableId {
    let t = e.document().sections[section].blocks.iter().find_map(|b| if let Block::Table(t) = b { Some(t.id) } else { None });
    TableId(t.expect("a table"))
}

fn cells(t: TableId) -> Selection {
    Selection::Cells { table: t, anchor: CellRef { row: 0, col: 0 }, focus: CellRef { row: 0, col: 0 }, shape: CellShape::Rect }
}

fn in_header(e: &Editor) -> bool {
    matches!(e.selection().focus().map(|f| f.story), Some(StoryId::HeaderFooter(_)))
}

fn story(e: &Editor) -> Option<StoryId> {
    e.selection().focus().map(|f| f.story)
}

fn json<T: serde::de::DeserializeOwned>(v: serde_json::Value) -> T {
    serde_json::from_value(v).expect("a command's options")
}

fn footnote_para(e: &Editor, id: i64) -> ParaId {
    e.document().footnotes[&id].iter().find_map(|b| if let Block::Para(q) = b { Some(ParaId(q.id)) } else { None }).expect("a paragraph")
}

// Section targeting

#[test]
fn page_setup_on_selected_cells_changes_their_section() {
    let mut e = three_sections(&format!("{}{}", table("Cell"), p("Three")));
    let t = table_id(&e, 2);
    e.set_caret_for_benchmark(cells(t));
    e.execute(Command::SetOrientation(crate::api::command::SectionValue::here(Orientation::Landscape)), fonts());
    assert!(!landscape(&e, 0), "the first section stays");
    assert!(landscape(&e, 2), "the cells' section turns");
}

#[test]
fn page_setup_on_a_selected_picture_changes_its_section() {
    let mut e = three_sections(&p("Three"));
    let three = para_of(&e, "Three");
    e.set_caret_for_benchmark(Selection::Object(ObjectRef::Inline { para: three, offset: Unit(0) }));
    e.execute(Command::SetOrientation(crate::api::command::SectionValue::here(Orientation::Landscape)), fonts());
    assert!(!landscape(&e, 0), "the first section stays");
    assert!(landscape(&e, 2), "the object's section turns");
}

#[test]
fn page_setup_from_a_footnote_changes_its_reference_section() {
    let mut e = three_sections(&p("Three"));
    let three = para_of(&e, "Three");
    caret(&mut e, StoryId::Body, three, 2);
    e.execute(Command::InsertFootnote, fonts());
    assert!(matches!(story(&e), Some(StoryId::Footnote(_))));
    e.execute(Command::SetOrientation(crate::api::command::SectionValue::here(Orientation::Landscape)), fonts());
    assert!(!landscape(&e, 0), "the first section stays");
    assert!(landscape(&e, 2), "the note's reference is in the third section");
}

#[test]
fn unlinking_an_inherited_header_copies_it_into_the_page_section() {
    let body = format!("{}{}{}<w:sectPr>{SEC}</w:sectPr>", sect_p("One", HEADER_REF), sect_p("Two", ""), p("Three"));
    let mut e = package(&body, Some(Some(HEADER)));
    assert!(e.snapshot().info().len() >= 3, "a page per section");
    e.edit_header_footer(Some(2), false, fonts());
    assert!(in_header(&e));
    let out = e.execute(Command::SetHeaderFooterLinked(json(serde_json::json!({ "kind": "header", "pages": "default", "linked": false }))), fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    assert!(e.document().sections[2].headers[0].is_some(), "the third section gets its own copy");
    assert!(e.document().sections[1].headers[0].is_none(), "the second still links");
    assert!(e.document().sections[0].headers[0].is_some(), "the first keeps its header");
    let own = e.document().sections[2].headers[0].clone();
    let Some(StoryId::HeaderFooter(k)) = story(&e) else { panic!("{:?}", e.selection()) };
    assert_eq!(e.index().part_name(k).map(str::to_string), own, "the caret follows into the copy");
}

#[test]
fn different_first_page_shows_the_shown_pages_section_in_a_shared_header() {
    let first = format!("<w:p><w:pPr><w:sectPr>{HEADER_REF}{SEC}<w:titlePg/></w:sectPr></w:pPr><w:r><w:t>One</w:t></w:r></w:p>");
    let mut e = package(&format!("{first}{}<w:sectPr>{SEC}</w:sectPr>", p("Two")), Some(Some(HEADER)));
    e.edit_header_footer(Some(1), false, fonts());
    assert!(in_header(&e));
    let f = e.format_state();
    let checked =
        e.command_states(f.as_ref(), super::super::state::StateFlags { read_only: false })[&Command::SetDifferentFirstPage(true).id()].checked;
    assert_eq!(checked, Some(false), "the second section's, which the command changes");
    e.execute(Command::SetDifferentFirstPage(true), fonts());
    assert!(e.document().sections[1].title_pg);
}

// Headers and footers going away under the caret

fn own_header_in_second_section() -> Editor {
    let body = format!("{}{}<w:sectPr>{HEADER_REF}{SEC}</w:sectPr>", sect_p("One", ""), p("Two"));
    let mut e = package(&body, Some(Some(HEADER)));
    e.edit_header_footer(Some(1), false, fonts());
    assert!(in_header(&e));
    e
}

#[test]
fn link_to_previous_from_inside_the_header_keeps_the_caret_valid() {
    let mut e = own_header_in_second_section();
    let out = e.execute(Command::SetHeaderFooterLinked(json(serde_json::json!({ "kind": "header", "pages": "default", "linked": true }))), fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    assert_eq!(violations(&e), Vec::<String>::new());
    assert_eq!(story(&e), Some(StoryId::Body), "no header is shown now");
}

#[test]
fn remove_header_from_inside_it_keeps_the_caret_in_the_empty_one() {
    let mut e = own_header_in_second_section();
    let out = e.execute(Command::RemoveHeaderFooter(json(serde_json::json!({ "kind": "header", "pages": "default" }))), fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    assert_eq!(violations(&e), Vec::<String>::new());
    assert!(in_header(&e), "{:?}", e.selection());
}

// Provisional footer

#[test]
fn selecting_cells_drops_the_provisional_footer_and_keeps_the_cells() {
    let mut e = package(&format!("{}{}<w:sectPr>{SEC}</w:sectPr>", p("Body"), table("Cell")), None);
    let body = para_of(&e, "Body");
    caret(&mut e, StoryId::Body, body, 1);
    e.execute(Command::EditFooter, fonts());
    assert!(in_header(&e));
    let t = table_id(&e, 0);
    e.set_selection(cells(t));
    e.settle_provisional(fonts());
    assert!(e.document().stories.is_empty(), "the empty footer is dropped");
    assert!(matches!(e.selection(), Selection::Cells { .. }), "{:?}", e.selection());
}

// References to a part that never loaded

#[test]
fn edit_header_replaces_a_reference_to_a_missing_part() {
    let mut e = package(&format!("{}<w:sectPr>{HEADER_REF}{SEC}</w:sectPr>", p("One")), Some(None));
    assert!(e.document().sections[0].headers[0].is_some() && e.document().stories.is_empty(), "a reference with no part");
    let out = e.execute(Command::EditHeader, fonts());
    assert!(in_header(&e), "{out:?}");
    let name = e.document().sections[0].headers[0].clone().expect("a header");
    assert!(e.document().stories.contains_key(&name), "the reference names the new part");
    e.execute(Command::Undo(Default::default()), fonts());
    assert_eq!(e.document().sections[0].headers[0].as_deref(), Some("word/header1.xml"), "undo brings the reference back");
}

#[test]
fn a_new_header_leaves_another_sections_missing_part_its_name() {
    let body = format!("{}{}<w:sectPr>{HEADER_REF}{SEC}</w:sectPr>", sect_p("One", HEADER_REF), p("Two"));
    let mut e = package(&body, Some(None));
    let missing = Some("word/header1.xml");
    assert!(e.document().sections.iter().all(|s| s.headers[0].as_deref() == missing), "both refer to the missing part");
    let out = e.execute(Command::EditHeader, fonts());
    assert!(in_header(&e), "{out:?}");
    let name = e.document().sections[0].headers[0].clone().expect("a header");
    assert_ne!(Some(name.as_str()), missing, "a new part's own name");
    assert_eq!(e.document().sections[1].headers[0].as_deref(), missing, "the other section keeps its reference");
}

#[test]
fn remove_header_over_a_missing_part_gives_an_empty_one() {
    let mut e = package(&format!("{}<w:sectPr>{HEADER_REF}{SEC}</w:sectPr>", p("One")), Some(None));
    let before = e.document().sections[0].headers.clone();
    let out = e.execute(Command::RemoveHeaderFooter(json(serde_json::json!({ "kind": "header", "pages": "default" }))), fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    let name = e.document().sections[0].headers[0].clone().expect("a header");
    assert!(e.document().stories.contains_key(&name));
    e.execute(Command::Undo(Default::default()), fonts());
    assert_eq!(e.document().sections[0].headers, before);
}

#[test]
fn a_refused_header_removal_step_leaves_the_reference() {
    let mut e = package(&format!("{}<w:sectPr>{HEADER_REF}{SEC}</w:sectPr>", p("One")), Some(None));
    let before = e.document().sections[0].headers.clone();
    let step = super::super::step::Step::HeaderPart { name: "word/header1.xml".into(), section: 0, footer: false, slot: 0, blocks: None };
    let mut tx = e.begin("Remove Header");
    assert!(e.step(&mut tx, step).is_err());
    e.abort(tx);
    assert_eq!(e.document().sections[0].headers, before);
}

// Notes

#[test]
fn arrows_between_notes_follow_their_references() {
    let mut e = open("FX-TEXT.docx");
    e.complete(fonts());
    let body = para_id(&e, 1);
    for at in [2, 6, 0] {
        caret(&mut e, StoryId::Body, body, at);
        e.execute(Command::InsertFootnote, fonts());
    }
    // References in order 3, 1, 2.
    let first = footnote_para(&e, 1);
    caret(&mut e, StoryId::Footnote(1), first, 0);
    e.execute(mv(MoveBy::Char, Dir::Back, false), fonts());
    assert_eq!(story(&e), Some(StoryId::Footnote(3)), "left goes to the note before on the page");
    let second = footnote_para(&e, 2);
    caret(&mut e, StoryId::Footnote(2), second, 0);
    e.execute(mv(MoveBy::Char, Dir::Back, false), fonts());
    assert_eq!(story(&e), Some(StoryId::Footnote(1)));
}

#[test]
fn typing_a_note_onto_the_next_page_lays_out_the_carets_page() {
    let lines: String = (0..300).map(|i| p(&format!("Line {i:03}"))).collect();
    let mut e = package(&format!("{lines}<w:sectPr>{SEC}</w:sectPr>"), None);
    let on = |e: &mut Editor, para: ParaId| {
        caret(e, StoryId::Body, para, 0);
        e.caret_view().expect("a caret").page
    };
    let paras: Vec<ParaId> = (0..300).map(|i| para_of(&e, &format!("Line {i:03}"))).collect();
    let first_page = paras.iter().take_while(|&&q| on(&mut e, q) == 0).count();
    // A few lines above the page's end, so the note starts on page 0.
    caret(&mut e, StoryId::Body, paras[first_page - 4], 8);
    e.execute(Command::InsertFootnote, fonts());
    e.complete(fonts());
    assert!(matches!(story(&e), Some(StoryId::Footnote(_))));
    assert_eq!(e.caret_view().expect("the note's caret").page, 0);
    let moved = (0..200).any(|_| {
        e.execute(Command::InsertText("a note that grows ".into()), fonts());
        let c = e.caret_view().expect("the caret shows without completing layout");
        assert!(matches!(e.snapshot().status(), crate::layout::LayoutStatus::Pending { .. }), "laid out only to the caret's page");
        c.page == 1
    });
    assert!(moved, "the note reaches the next page");
}

#[test]
fn a_custom_mark_leads_back_to_its_reference() {
    let mut e = open("FX-TEXT.docx");
    e.complete(fonts());
    let body = para_id(&e, 1);
    caret(&mut e, StoryId::Body, body, 3);
    e.execute(Command::InsertNote(json(serde_json::json!({ "kind": "footnote", "customMark": "*" }))), fonts());
    let Some(StoryId::Footnote(id)) = story(&e) else { panic!("{:?}", e.selection()) };
    let at = Position::text(StoryId::Footnote(id), footnote_para(&e, id), Unit(0));
    assert!(e.note_jump(at), "the mark leads back");
    assert_eq!(e.selection().focus().and_then(|f| f.para_offset()), Some((body, Unit(3))));
    let text = Position::text(StoryId::Footnote(id), footnote_para(&e, id), Unit(2));
    assert!(!e.note_jump(text), "the note's text doesn't");
}

#[test]
fn converted_notes_take_the_other_kinds_styles() {
    let mut e = open("FX-TEXT.docx");
    e.complete(fonts());
    let body = para_id(&e, 1);
    caret(&mut e, StoryId::Body, body, 3);
    e.execute(Command::InsertFootnote, fonts());
    let fresh_endnote = {
        let mut f = open("FX-TEXT.docx");
        f.complete(fonts());
        let host = para_id(&f, 1);
        caret(&mut f, StoryId::Body, host, 3);
        f.execute(Command::InsertEndnote, fonts());
        let Some(Block::Para(q)) = f.document().endnotes.values().next().and_then(|b| b.first()).cloned() else { panic!("no endnote") };
        q
    };
    e.execute(Command::ConvertNotes(json(serde_json::json!("footnotesToEndnotes"))), fonts());
    let Some(Block::Para(q)) = e.document().endnotes.values().next().and_then(|b| b.first()) else { panic!("no endnote") };
    assert_eq!(q.style.style_id.as_deref(), Some("EndnoteText"));
    let mark = |q: &crate::docx::Paragraph| q.inlines.iter().find_map(|i| if let Inline::NoteMark(_, _, r) = i { Some(r.clone()) } else { None });
    assert_eq!(mark(q).map(|r| r.direct.clone()), mark(&fresh_endnote).map(|r| r.direct.clone()), "the mark's run is an inserted endnote's");
    let reference = e.index().para(e.document(), body).and_then(|p| {
        p.inlines.iter().find_map(|i| match i {
            Inline::NoteRef { run, .. } => Some(run.clone()),
            _ => None,
        })
    });
    assert!(reference.is_some_and(|r| r.direct.0.as_deref().is_some_and(|x| x.contains("EndnoteReference"))), "the reference's run");
}

#[test]
fn converting_a_note_restyles_its_cells_and_a_single_quoted_reference() {
    let mut e = open("FX-TEXT.docx");
    e.complete(fonts());
    let body = para_id(&e, 1);
    caret(&mut e, StoryId::Body, body, 3);
    e.execute(Command::InsertFootnote, fonts());
    let Some(Block::Para(note)) = e.doc.footnotes[&1].first().cloned() else { panic!("a note paragraph") };
    let grid = Editor::open(&super::blank_body(&table("x")), fonts()).expect("opens");
    let Some(Block::Table(mut t)) = grid.document().sections[0].blocks.iter().find(|b| matches!(b, Block::Table(_))).cloned() else {
        panic!("a table")
    };
    t.rows[0].cells[0].blocks = vec![Block::Para(crate::docx::Paragraph { id: crate::docx::next_para_id(), ..note })];
    e.doc.footnotes.get_mut(&1).expect("note 1").push(Block::Table(t));
    let Some(Block::Para(q)) = e.doc.sections[0].blocks.iter_mut().find(|b| matches!(b, Block::Para(q) if q.id == body.0)) else {
        panic!("no paragraph")
    };
    for i in &mut q.inlines {
        if let Inline::NoteRef { run, .. } = i {
            let mut r = (**run).clone();
            r.direct = crate::docx::Raw(Some(r#"<w:rPr><w:rStyle w:val='FootnoteReference'/></w:rPr>"#.into()));
            *run = std::rc::Rc::new(r);
        }
    }
    e.index = super::super::index::DocIndex::build_with_parts(&e.doc, e.index.parts());
    e.execute(Command::ConvertNotes(json(serde_json::json!("footnotesToEndnotes"))), fonts());
    let blocks = e.document().endnotes.values().next().expect("an endnote");
    let Some(Block::Table(t)) = blocks.iter().find(|b| matches!(b, Block::Table(_))) else { panic!("the note's table") };
    let Some(Block::Para(cell)) = t.rows[0].cells[0].blocks.first() else { panic!("the cell's paragraph") };
    assert_eq!(cell.style.style_id.as_deref(), Some("EndnoteText"), "the cell's paragraph");
    assert!(cell.inlines.iter().any(|i| matches!(i, Inline::NoteMark(crate::docx::NoteKind::End, ..))), "the cell's mark");
    let reference = e.index().para(e.document(), body).and_then(|p| {
        p.inlines.iter().find_map(|i| match i {
            Inline::NoteRef { run, .. } => Some(run.clone()),
            _ => None,
        })
    });
    assert!(reference.is_some_and(|r| r.direct.0.as_deref().is_some_and(|x| x.contains("EndnoteReference"))), "the reference's run");
}

#[test]
fn a_note_id_at_the_limit_does_not_overflow() {
    let mut e = open("FX-TEXT.docx");
    e.complete(fonts());
    let body = para_id(&e, 1);
    caret(&mut e, StoryId::Body, body, 3);
    e.execute(Command::InsertFootnote, fonts());
    let blocks = e.doc.footnotes.remove(&1).expect("note 1");
    e.doc.footnotes.insert(i64::MAX, blocks);
    let Some(Block::Para(q)) = e.doc.sections[0].blocks.iter_mut().find(|b| matches!(b, Block::Para(q) if q.id == body.0)) else {
        panic!("no paragraph")
    };
    for i in &mut q.inlines {
        if let Inline::NoteRef { id, .. } = i {
            *id = i64::MAX;
        }
    }
    e.index = super::super::index::DocIndex::build_with_parts(&e.doc, e.index.parts());
    let fresh = |e: &Editor| e.document().footnotes.keys().copied().filter(|&k| k != i64::MAX).collect::<Vec<i64>>();
    caret(&mut e, StoryId::Body, body, 0);
    e.execute(Command::InsertFootnote, fonts());
    assert_eq!(fresh(&e), vec![1]);
    caret(&mut e, StoryId::Body, body, 0);
    e.execute(Command::InsertEndnote, fonts());
    e.execute(Command::ConvertNotes(json(serde_json::json!("endnotesToFootnotes"))), fonts());
    let mut ids = fresh(&e);
    ids.sort_unstable();
    assert_eq!(ids, vec![1, 2], "the converted note takes the lowest free id");
}

// Frames and drop caps

#[test]
fn no_drop_cap_inside_a_text_box() {
    let mut e = open("FX-TEXT.docx");
    e.complete(fonts());
    let body = para_id(&e, 1);
    caret(&mut e, StoryId::Body, body, 0);
    e.execute(Command::InsertTextBox, fonts());
    for c in "Hello".chars() {
        e.execute(Command::InsertText(c.to_string()), fonts());
    }
    assert!(matches!(story(&e), Some(StoryId::TextBox(_))), "{:?}", e.selection());
    let para = e.selection().focus().and_then(|f| f.para_offset()).map(|(p, _)| p).expect("a caret");
    let d = DropCap { position: DropCapPosition::Dropped, font: None, lines: Some(3), distance: None };
    let out = e.execute(Command::SetDropCap(d), fonts());
    assert!(!matches!(out, Outcome::Changed(_)), "{out:?}");
    let box_para = e.index().para(e.document(), para).expect("the box's paragraph");
    assert_eq!(UnitText::of(box_para).text, "Hello");
    assert!(!box_para.inlines.iter().any(|i| matches!(i, Inline::Float(..))));
}

#[test]
fn a_frame_before_a_drop_cap_keeps_the_drop_cap_found() {
    let plain = r#"<w:p><w:pPr><w:framePr w:w="2000" w:hSpace="100" w:wrap="around" w:vAnchor="page" w:hAnchor="page" w:x="100" w:y="100"/></w:pPr><w:r><w:t>Side</w:t></w:r></w:p>"#;
    let cap = r#"<w:p><w:pPr><w:framePr w:dropCap="drop" w:lines="3" w:wrap="around" w:vAnchor="text" w:hAnchor="text"/></w:pPr><w:r><w:t>T</w:t></w:r></w:p>"#;
    let mut e = package(&format!("{plain}{cap}{}<w:sectPr>{SEC}</w:sectPr>", p("his is the host paragraph text, long enough to wrap.")), None);
    let host = para_of(&e, "his");
    caret(&mut e, StoryId::Body, host, 2);
    assert_eq!(e.drop_cap().map(|d| d.position), Some(DropCapPosition::Dropped));
    let none = DropCap { position: DropCapPosition::None, font: None, lines: None, distance: None };
    assert!(matches!(e.execute(Command::SetDropCap(none), fonts()), Outcome::Changed(_)));
    assert!(texts(&e).iter().any(|t| t.starts_with("This is the host")), "{:?}", texts(&e));
}

#[test]
fn remove_frame_keeps_a_frame_read_after_its_anchor_after_it() {
    let frame = r#"<w:p><w:pPr><w:framePr w:w="2000" w:wrap="around" w:vAnchor="page" w:hAnchor="page" w:x="100" w:y="2000"/></w:pPr><w:r><w:t>FRAMED</w:t></w:r></w:p>"#;
    let mut e = package(&format!("{}{frame}{}<w:sectPr>{SEC}</w:sectPr>", p("AAA"), table("Cell")), None);
    let f = e.document().sections[0].blocks.iter().find_map(|b| match b {
        Block::Para(q) => q.inlines.iter().find_map(|i| if let Inline::Float(f, _) = i { Some(f.id) } else { None }),
        _ => None,
    });
    e.set_caret_for_benchmark(Selection::Object(ObjectRef::Floating(FloatId(f.expect("a frame anchored in the body")))));
    assert!(matches!(e.execute(Command::RemoveFrame, fonts()), Outcome::Changed(_)));
    let t = texts(&e);
    let (a, framed) = (t.iter().position(|x| x == "AAA"), t.iter().position(|x| x == "FRAMED"));
    assert!(a.is_some() && framed.is_some() && a < framed, "{t:?}");
}

fn frame_p(y: u32, t: &str) -> String {
    format!(
        r#"<w:p><w:pPr><w:framePr w:w="2000" w:wrap="around" w:vAnchor="page" w:hAnchor="page" w:x="100" w:y="{y}"/></w:pPr><w:r><w:t>{t}</w:t></w:r><w:r><w:br w:type="page"/></w:r><w:r><w:t>{t}2</w:t></w:r></w:p>"#
    )
}

/// The ids of the frames anchored in the body, in their anchors' order.
fn frames(e: &Editor) -> Vec<FloatId> {
    e.document().sections[0]
        .blocks
        .iter()
        .filter_map(|b| if let Block::Para(q) = b { Some(q) } else { None })
        .flat_map(|q| q.inlines.iter())
        .filter_map(|i| if let Inline::Float(f, _) = i { Some(FloatId(f.id)) } else { None })
        .collect()
}

fn remove_frame(e: &mut Editor, f: FloatId) {
    e.set_caret_for_benchmark(Selection::Object(ObjectRef::Floating(f)));
    assert!(matches!(e.execute(Command::RemoveFrame, fonts()), Outcome::Changed(_)));
}

#[test]
fn removed_frames_sharing_an_anchor_keep_their_order() {
    let after = format!("{}{}{}{}<w:sectPr>{SEC}</w:sectPr>", p("AAA"), frame_p(2000, "ONE"), frame_p(4000, "TWO"), table("Cell"));
    let before = format!("{}{}{}{}<w:sectPr>{SEC}</w:sectPr>", p("AAA"), frame_p(2000, "ONE"), frame_p(4000, "TWO"), p("BBB"));
    for (body, want) in [(&after, ["AAA", "ONE", "TWO"]), (&before, ["ONE", "TWO", "BBB"])] {
        for reverse in [false, true] {
            let mut e = package(body, None);
            let mut fs = frames(&e);
            assert_eq!(fs.len(), 2, "two frames");
            if reverse {
                fs.reverse();
            }
            for f in fs {
                remove_frame(&mut e, f);
            }
            let t: Vec<String> = texts(&e).into_iter().filter(|t| want.iter().any(|w| t.starts_with(w))).collect();
            assert_eq!(t.iter().map(|t| &t[..3]).collect::<Vec<_>>(), want, "reverse {reverse}: {t:?}");
        }
    }
}

#[test]
fn a_removed_frames_paragraphs_join_the_flow_with_the_caret_in_them() {
    let body = format!("{}{}{}<w:sectPr>{SEC}</w:sectPr>", p("AAA"), frame_p(2000, "ONE"), table("Cell"));
    let mut e = package(&body, None);
    let f = frames(&e)[0];
    remove_frame(&mut e, f);
    let (para, _) = e.selection().focus().and_then(|f| f.para_offset()).expect("a caret");
    let q = e.index().para(e.document(), para).expect("its paragraph");
    assert!(UnitText::of(q).text.starts_with("ONE"), "{:?}", UnitText::of(q).text);
    assert!(!q.style.in_frame, "out of the frame");
    e.complete(fonts());
    assert!(e.snapshot().pages().len() >= 2, "its page break breaks the page");
}

#[test]
fn typing_in_a_header_keeps_the_caret_on_its_page() {
    let pages: String = (0..12).map(|i| format!(r#"<w:p><w:r><w:t>Page {i}</w:t></w:r><w:r><w:br w:type="page"/></w:r></w:p>"#)).collect();
    let mut e = package(&format!("{pages}<w:sectPr>{HEADER_REF}{SEC}</w:sectPr>"), Some(Some(&p("Head"))));
    let eight = para_of(&e, "Page 8");
    caret(&mut e, StoryId::Body, eight, 0);
    assert_eq!(e.caret_view().expect("a caret").page, 8);
    e.execute(Command::EditHeader, fonts());
    assert_eq!(e.caret_view().expect("the header's caret").page, 8);
    e.execute(Command::InsertText("er".into()), fonts());
    assert_eq!(e.caret_view().expect("the header's caret").page, 8, "the header on the caret's page, not the first");
    e.complete(fonts());
    assert_eq!(e.caret_view().expect("the header's caret").page, 8);
}

#[test]
fn typing_in_a_header_lays_out_to_the_carets_page_reusing_the_bodies() {
    use crate::layout::incremental::FULL_JOBS;
    let pages: String = (0..12).map(|i| format!(r#"<w:p><w:r><w:t>Page {i}</w:t></w:r><w:r><w:br w:type="page"/></w:r></w:p>"#)).collect();
    let mut e = package(&format!("{pages}<w:sectPr>{HEADER_REF}{SEC}</w:sectPr>"), Some(Some(&p("Head"))));
    assert!(e.snapshot().pages().len() >= 12);
    e.execute(Command::EditHeader, fonts());
    e.complete(fonts());
    let full = FULL_JOBS.get();
    e.execute(Command::InsertText("er".into()), fonts());
    assert!(
        matches!(e.snapshot().status(), crate::layout::LayoutStatus::Pending { stale_from } if *stale_from <= 3),
        "laid out to the caret's page: {:?}",
        e.snapshot().status()
    );
    e.complete(fonts());
    assert_eq!(FULL_JOBS.get(), full, "no full layout");
    let header: Vec<String> = e
        .document()
        .stories
        .values()
        .flatten()
        .filter_map(|b| if let Block::Para(p) = b { Some(crate::docx::fields::text(p)) } else { None })
        .collect();
    assert!(header.iter().any(|t| t.contains("er")), "{header:?}");
}

#[test]
fn a_header_links_tip_goes_when_the_header_closes_under_a_still_pointer() {
    use super::super::input::PointerKind;
    use super::super::keymap::Mods;
    let link = r#"<w:p><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> HYPERLINK "http://example.com" </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>Link</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r></w:p>"#;
    let mut e = package(&format!("{}<w:sectPr>{HEADER_REF}{SEC}</w:sectPr>", p("Body")), Some(Some(link)));
    e.execute(Command::EditHeader, fonts());
    let Some(StoryId::HeaderFooter(h)) = story(&e) else { panic!("{:?}", e.selection()) };
    let (hp, _) = e.selection().focus().and_then(|f| f.para_offset()).expect("a caret in the header");
    let c = e.caret_at(Position::text(StoryId::HeaderFooter(h), hp, Unit(2))).expect("laid out");
    let (x, y) = e.stack().to_doc(c.page, c.x, (c.top + c.bottom) / 2.0).expect("on the stack");
    e.pointer_input(PointerKind::Move, x, y, 0, Mods::default(), fonts());
    assert!(e.hover_tip().is_some_and(|(k, ..)| *k == super::super::hover::HoverTip::Link), "the header's link");
    e.execute(Command::CloseHeaderFooter, fonts());
    assert!(e.hover_tip().is_none(), "the header is out of reach");
}

#[test]
fn selecting_an_earlier_note_does_not_lay_out_the_rest_of_the_document() {
    let lines: String = (0..600).map(|i| p(&format!("Line {i:03}"))).collect();
    let mut e = package(&format!("{lines}<w:sectPr>{SEC}</w:sectPr>"), None);
    let on = |e: &mut Editor, para: ParaId| {
        caret(e, StoryId::Body, para, 0);
        e.caret_view().expect("a caret").page
    };
    let paras: Vec<ParaId> = (0..600).map(|i| para_of(&e, &format!("Line {i:03}"))).collect();
    let first_page = paras.iter().take_while(|&&q| on(&mut e, q) == 0).count();
    caret(&mut e, StoryId::Body, paras[first_page - 4], 8);
    e.execute(Command::InsertFootnote, fonts());
    let Some(StoryId::Footnote(id)) = story(&e) else { panic!("{:?}", e.selection()) };
    let note = footnote_para(&e, id);
    e.complete(fonts());
    // A change at the top leaves the pages after it to lay out; the caret then reads far on.
    caret(&mut e, StoryId::Body, paras[0], 0);
    e.execute(Command::InsertParagraph, fonts());
    let far = paras[first_page * 2 + 2];
    e.select_range(far, Unit(0), Unit(0), fonts());
    assert!(e.caret_view().expect("a caret").page >= 2);
    assert!(matches!(e.snapshot().status(), crate::layout::LayoutStatus::Pending { .. }), "laid out only to the caret's page");
    e.select_range(note, Unit(0), Unit(0), fonts());
    assert_eq!(e.caret_view().expect("the note's caret").page, 0);
    assert!(matches!(e.snapshot().status(), crate::layout::LayoutStatus::Pending { .. }), "the note's line was on a page already laid out");
}

/// The main part's XML as saved.
fn saved_document(e: &Editor) -> String {
    let saved = crate::docx::write::save(e.document()).expect("saves");
    crate::docx::package::read(&saved, &[], "word/document.xml").expect("reads").expect("the main part")
}

#[test]
fn a_frame_is_selected_moved_and_resized_by_its_border_and_saved_in_its_frame_properties() {
    use crate::editing::input::PointerKind;
    use crate::editing::keymap::Mods;
    let frame = r#"<w:p><w:pPr><w:framePr w:w="2880" w:h="1440" w:wrap="around" w:vAnchor="page" w:hAnchor="page" w:x="1441" w:y="2881"/></w:pPr><w:r><w:t>FRAMED</w:t></w:r></w:p>"#;
    let mut e = package(&format!("{frame}{}<w:sectPr>{SEC}</w:sectPr>", p("Body text after the frame.")), None);
    e.complete(fonts());
    let f = frames(&e)[0];
    let none = Mods::default();
    let at = |e: &Editor, x: f64, y: f64| e.stack().to_doc(0, x, y).expect("on the stack");
    // A press on its border selects it; a drag from there moves it 36 pt right and down.
    let (x, y) = at(&e, 72.5, 180.0);
    e.pointer_input(PointerKind::Down, x, y, 1, none, fonts());
    assert_eq!(e.selection(), Selection::Object(ObjectRef::Floating(f)));
    assert!(e.object_props(fonts()).is_some_and(|p| p.frame && p.stack_index.is_none()));
    for k in 1..=4 {
        let t = f64::from(k) * 9.0;
        let (mx, my) = at(&e, 72.5 + t, 180.0 + t);
        e.pointer_input(PointerKind::Move, mx, my, 0, none, fonts());
    }
    let (ux, uy) = at(&e, 108.5, 216.0);
    e.pointer_input(PointerKind::Up, ux, uy, 0, none, fonts());
    let props = e.frame_properties(None).expect("frame properties");
    assert!((props.horizontal.0 - 108.05).abs() < 0.06 && (props.vertical.0 - 180.05).abs() < 0.06, "{props:?}");
    let xml = saved_document(&e);
    assert!(xml.contains(r#"w:x="2161""#) && xml.contains(r#"w:y="3601""#), "{xml}");
    // Its bottom-right handle widens it by 36 pt.
    e.complete(fonts());
    let (hx, hy) = at(&e, 108.0 + 144.0, 180.0 + 72.0);
    let (tx, ty) = at(&e, 108.0 + 180.0, 180.0 + 72.0);
    e.pointer_input(PointerKind::Down, hx, hy, 1, none, fonts());
    e.pointer_input(PointerKind::Move, tx, ty, 0, none, fonts());
    e.pointer_input(PointerKind::Up, tx, ty, 0, none, fonts());
    assert!((e.frame_properties(None).expect("frame properties").width.0 - 180.0).abs() < 0.06);
    assert!(saved_document(&e).contains(r#"w:w="3600""#));
    // Allow overlap off: suppressOverlap on its paragraph.
    assert_eq!(e.frame_allow_overlap(), Some(true));
    assert!(matches!(e.set_frame_allow_overlap(false, fonts()), Outcome::Changed(_)));
    assert_eq!(e.frame_allow_overlap(), Some(false));
    assert!(saved_document(&e).contains("suppressOverlap"));
}

#[test]
fn note_options_for_this_section_take_every_selected_section() {
    use crate::api::command::{NoteProperties, NoteScope, NoteType, NumberFormat};
    let mut e = three_sections(&p("Three"));
    let (a, b) = (para_of(&e, "One"), para_of(&e, "Two"));
    e.selection = Selection::range(
        Position::text(StoryId::Body, a, Unit(1)),
        Position::text(StoryId::Body, b, Unit(1)),
        crate::editing::selection::Granularity::Char,
    );
    let props = NoteProperties {
        kind: NoteType::Footnote,
        format: Some(NumberFormat::LowerRoman),
        apply_to: Some(NoteScope::ThisSection),
        ..Default::default()
    };
    assert!(matches!(e.execute(Command::SetNoteProperties(props), fonts()), Outcome::Changed(_)));
    let fmts: Vec<Option<String>> = e.document().sections.iter().map(|s| s.notes[0].fmt.clone()).collect();
    assert_eq!(fmts, [Some("lowerRoman".into()), Some("lowerRoman".into()), None], "the selected sections");
}
