//! Content controls: placeholders, filling, removing, wrapping and their options

use serde_json::{Value, json};

use super::super::commands::{Command, Outcome};
use super::super::position::{Position, StoryId};
use super::super::selection::Selection;
use super::{blank_body, fonts, para_id, texts};
use crate::api::command::DeleteOptions;
use crate::api::value::Direction;
use crate::docx::{Block, ControlId, ControlKind};
use crate::editing::Editor;
use crate::layout::Unit;

fn send(e: &mut Editor, msg: Value) -> Value {
    let mut msg = msg;
    msg["seq"] = json!(e.seq_applied() + 1);
    serde_json::from_str(&e.apply_json(&msg.to_string(), fonts())).expect("a reply")
}

fn cmd(e: &mut Editor, c: Value) -> Value {
    send(e, json!({ "type": "command", "command": c }))
}

fn refused(r: &Value) -> bool {
    r.get("error").is_some() || r["answer"].get("error").is_some()
}

fn ok(r: &Value) {
    assert!(!refused(r), "{r}");
}

fn ctl(e: &Editor, f: impl Fn(&crate::docx::Control) -> bool) -> ControlId {
    *e.document().controls.iter().find(|(_, c)| f(c)).map(|(k, _)| k).expect("the control")
}

fn api(c: ControlId) -> String {
    crate::editing::elements::id_str(crate::docx::ElementId::Control(c))
}

fn by_alias(e: &Editor, a: &str) -> ControlId {
    ctl(e, |c| c.props.alias.as_deref() == Some(a))
}

fn caret(e: &mut Editor, para: usize, unit: u32) {
    e.selection = Selection::Caret(Position::text(StoryId::Body, para_id(e, para), Unit(unit)));
}

fn doc_xml(e: &Editor) -> String {
    use std::io::Read;
    let bytes = crate::docx::write::save(e.document()).expect("saves");
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut s = String::new();
    z.by_name("word/document.xml").unwrap().read_to_string(&mut s).unwrap();
    s
}

/// A document whose first paragraph is "Before " and control `C` (`props`, `content`), then "Other".
fn with_control(props: &str, content: &str) -> Editor {
    let body = format!(
        "<w:p><w:r><w:t xml:space=\"preserve\">Before </w:t></w:r><w:sdt><w:sdtPr><w:alias w:val=\"C\"/><w:id w:val=\"5\"/>{props}</w:sdtPr><w:sdtContent>{content}</w:sdtContent></w:sdt></w:p><w:p><w:r><w:t>Other</w:t></w:r></w:p>"
    );
    Editor::open(&blank_body(&body), fonts()).expect("opens")
}

/// "Hi " with a plain text control inserted at its end, showing its placeholder.
fn placeholder_control() -> (Editor, ControlId) {
    let mut e = Editor::open(&blank_body("<w:p><w:r><w:t xml:space=\"preserve\">Hi </w:t></w:r></w:p>"), fonts()).expect("opens");
    caret(&mut e, 0, 3);
    ok(&cmd(&mut e, json!({ "type": "insertControl", "kind": "plainText" })));
    let c = ctl(&e, |_| true);
    assert!(e.document().controls[&c].props.showing_placeholder);
    assert_ne!(texts(&e), vec!["Hi ".to_string()], "the placeholder shows");
    (e, c)
}

#[test]
fn typing_in_a_placeholder_replaces_it() {
    let (mut e, c) = placeholder_control();
    caret(&mut e, 0, 10);
    e.execute(Command::InsertText("John".into()), fonts());
    assert_eq!(texts(&e), vec!["Hi John".to_string()]);
    assert!(!e.document().controls[&c].props.showing_placeholder, "it shows its content");
    let Some(Block::Para(p)) = e.document().sections[0].blocks.first() else { panic!("a paragraph") };
    let k = p.inlines.iter().position(|i| matches!(i, crate::docx::Inline::ControlStart(x) if *x == c)).expect("its start");
    assert!(matches!(&p.inlines[k + 1], crate::docx::Inline::Text(t, _) if t == "John"), "the text is in the control");
    assert!(matches!(p.inlines.get(k + 2), Some(crate::docx::Inline::ControlEnd(x)) if *x == c));
    e.execute(Command::Undo, fonts());
    assert!(e.document().controls[&c].props.showing_placeholder, "one undo step brings the placeholder back");
    assert!(texts(&e)[0].starts_with("Hi Click"), "{:?}", texts(&e));
}

#[test]
fn typing_at_a_placeholders_start_replaces_it() {
    let mut e = Editor::open(
        &blank_body(r#"<w:p><w:sdt><w:sdtPr><w:id w:val="5"/><w:showingPlcHdr/><w:text/></w:sdtPr><w:sdtContent><w:r><w:t>Click here</w:t></w:r></w:sdtContent></w:sdt><w:r><w:t xml:space="preserve"> after</w:t></w:r></w:p>"#),
        fonts(),
    )
    .expect("opens");
    let c = ctl(&e, |_| true);
    caret(&mut e, 0, 0);
    e.execute(Command::InsertText("J".into()), fonts());
    assert_eq!(texts(&e), vec!["J after".to_string()]);
    assert!(!e.document().controls[&c].props.showing_placeholder, "it shows its content");
    let Some(Block::Para(p)) = e.document().sections[0].blocks.first() else { panic!("a paragraph") };
    assert!(matches!(&p.inlines[1], crate::docx::Inline::Text(t, _) if t == "J"), "the text is in the control: {:?}", p.inlines);
}

#[test]
fn backspace_and_enter_at_a_placeholders_start_leave_it() {
    let (mut e, c) = placeholder_control();
    caret(&mut e, 0, 3);
    e.execute(Command::Delete(DeleteOptions { direction: Direction::Backward, unit: None }), fonts());
    assert!(texts(&e)[0].starts_with("HiClick"), "Backspace deletes what is before it: {:?}", texts(&e));
    assert!(e.document().controls[&c].props.showing_placeholder);
    let before = texts(&e);
    caret(&mut e, 0, 2);
    e.execute(Command::InsertParagraph, fonts());
    assert!(e.document().controls[&c].props.showing_placeholder, "Enter leaves the placeholder");
    assert!(texts(&e).concat().contains("Click"), "{:?} from {before:?}", texts(&e));
}

#[test]
fn deleting_in_a_placeholder_leaves_it() {
    let (mut e, c) = placeholder_control();
    let before = texts(&e);
    caret(&mut e, 0, 10);
    let out = e.execute(Command::Delete(DeleteOptions { direction: Direction::Backward, unit: None }), fonts());
    assert!(matches!(out, Outcome::Unchanged(_)), "{out:?}");
    assert_eq!(texts(&e), before);
    assert!(e.document().controls[&c].props.showing_placeholder);
}

#[test]
fn removing_a_placeholder_control_keeping_its_contents_leaves_no_prompt() {
    let (mut e, c) = placeholder_control();
    ok(&cmd(&mut e, json!({ "type": "removeControl", "id": api(c), "keepContent": true })));
    assert_eq!(texts(&e), vec!["Hi ".to_string()], "the placeholder goes with the control");
}

#[test]
fn filling_a_placeholder_drops_the_placeholder_style() {
    let mut e = with_control("<w:showingPlcHdr/><w:text/>", r#"<w:r><w:rPr><w:rStyle w:val="PlaceholderText"/></w:rPr><w:t>Click here</w:t></w:r>"#);
    let id = api(by_alias(&e, "C"));
    ok(&cmd(&mut e, json!({ "type": "fillControl", "id": id, "text": "John" })));
    let xml = doc_xml(&e);
    assert!(xml.contains("John") && !xml.contains("PlaceholderText"), "{xml}");
    let mut e = with_control(
        r#"<w:rPr><w:b/></w:rPr><w:showingPlcHdr/><w:text/>"#,
        r#"<w:r><w:rPr><w:rStyle w:val="PlaceholderText"/></w:rPr><w:t>Click here</w:t></w:r>"#,
    );
    let id = api(by_alias(&e, "C"));
    ok(&cmd(&mut e, json!({ "type": "fillControl", "id": id, "text": "John" })));
    let Some(Block::Para(p)) = e.document().sections[0].blocks.first() else { panic!("a paragraph") };
    let run = p.inlines.iter().find_map(|i| if let crate::docx::Inline::Text(t, r) = i { (t == "John").then(|| r.clone()) } else { None });
    assert!(run.is_some_and(|r| r.bold), "the control's own formatting");
}

#[test]
fn removing_a_repeating_section_keeps_a_nested_sections_items() {
    let mut e = Editor::open(&blank_body("<w:p><w:r><w:t>Row</w:t></w:r></w:p>"), fonts()).expect("opens");
    let select = |e: &mut Editor| {
        send(e, json!({ "type": "key", "code": "Home", "mods": { "ctrl": true } }));
        send(e, json!({ "type": "key", "code": "End", "mods": { "shift": true } }));
    };
    select(&mut e);
    ok(&cmd(&mut e, json!({ "type": "wrapInRepeatingSection", "tag": "Outer" })));
    select(&mut e);
    ok(&cmd(&mut e, json!({ "type": "wrapInRepeatingSection", "tag": "Inner" })));
    let kinds = |e: &Editor| match e.document().sections[0].blocks.first() {
        Some(Block::Para(p)) => p.controls.iter().map(|c| e.document().controls[c].props.kind.clone()).collect::<Vec<_>>(),
        _ => panic!("a paragraph"),
    };
    let outer = ctl(&e, |c| c.props.tag.as_deref() == Some("Outer"));
    assert_eq!(kinds(&e).iter().filter(|k| **k == ControlKind::RepeatingSection).count(), 2, "two sections: {:?}", kinds(&e));
    ok(&cmd(&mut e, json!({ "type": "removeControl", "id": api(outer), "keepContent": true })));
    let left = kinds(&e);
    assert_eq!(left, vec![ControlKind::RepeatingSection, ControlKind::RepeatingItem], "the inner section keeps its item");
}

fn para_controls(e: &Editor) -> Vec<Vec<ControlId>> {
    fn walk(blocks: &[Block], out: &mut Vec<Vec<ControlId>>) {
        for b in blocks {
            match b {
                Block::Para(p) => out.push(p.controls.clone()),
                Block::Table(t) => t.rows.iter().flat_map(|r| r.cells.iter()).for_each(|c| walk(&c.blocks, out)),
            }
        }
    }
    let mut out = Vec::new();
    e.document().sections.iter().for_each(|s| walk(&s.blocks, &mut out));
    out
}

const NESTED: &str = r#"<w:sdt><w:sdtPr><w:alias w:val="C"/><w:id w:val="1"/></w:sdtPr><w:sdtContent><w:sdt><w:sdtPr><w:alias w:val="Y"/><w:id w:val="2"/></w:sdtPr><w:sdtContent><w:p><w:r><w:t>P1</w:t></w:r></w:p></w:sdtContent></w:sdt><w:p><w:r><w:t>P2</w:t></w:r></w:p></w:sdtContent></w:sdt>"#;

#[test]
fn filling_a_block_control_drops_the_controls_inside_it() {
    let mut e = Editor::open(&blank_body(&format!("{NESTED}<w:p><w:r><w:t>After</w:t></w:r></w:p>")), fonts()).expect("opens");
    let (c, y) = (by_alias(&e, "C"), by_alias(&e, "Y"));
    ok(&cmd(&mut e, json!({ "type": "fillControl", "id": api(c), "text": "x" })));
    assert_eq!(para_controls(&e), vec![vec![c], vec![]], "the new paragraph is the control's alone");
    assert!(!e.document().controls.contains_key(&y), "the inner control is gone");
}

#[test]
fn removing_a_whole_bodys_control_drops_the_controls_inside_it() {
    let last = r#"<w:sdt><w:sdtPr><w:alias w:val="C"/><w:id w:val="1"/></w:sdtPr><w:sdtContent><w:p><w:r><w:t>P1</w:t></w:r></w:p><w:sdt><w:sdtPr><w:alias w:val="Y"/><w:id w:val="2"/></w:sdtPr><w:sdtContent><w:p><w:r><w:t>P2</w:t></w:r></w:p></w:sdtContent></w:sdt></w:sdtContent></w:sdt>"#;
    let mut e = Editor::open(&blank_body(last), fonts()).expect("opens");
    let (c, y) = (by_alias(&e, "C"), by_alias(&e, "Y"));
    ok(&cmd(&mut e, json!({ "type": "removeControl", "id": api(c), "keepContent": false })));
    assert_eq!(para_controls(&e), vec![Vec::<ControlId>::new()], "the paragraph left has no control");
    assert!(!e.document().controls.contains_key(&y));
}

#[test]
fn wrapping_a_selection_at_an_inline_controls_edge_nests_it() {
    let body = r#"<w:p><w:sdt><w:sdtPr><w:alias w:val="N"/><w:id w:val="1"/><w:text/></w:sdtPr><w:sdtContent><w:r><w:t>Name</w:t></w:r></w:sdtContent></w:sdt><w:r><w:t xml:space="preserve"> lives here</w:t></w:r></w:p>"#;
    for (from, to) in [(0, 15), (0, 4), (0, 6)] {
        let mut e = Editor::open(&blank_body(body), fonts()).expect("opens");
        let n = by_alias(&e, "N");
        let p = para_id(&e, 0);
        e.selection = Selection::range(
            Position::text(StoryId::Body, p, Unit(from)),
            Position::text(StoryId::Body, p, Unit(to)),
            super::super::selection::Granularity::Char,
        );
        ok(&cmd(&mut e, json!({ "type": "insertControl", "kind": "group", "wrapSelection": true })));
        let g = ctl(&e, |c| c.props.kind == ControlKind::Group);
        let Some(Block::Para(q)) = e.document().sections[0].blocks.first() else { panic!("a paragraph") };
        let at = |x: ControlId, start: bool| {
            q.inlines
                .iter()
                .position(|i| match i {
                    crate::docx::Inline::ControlStart(c) if start => *c == x,
                    crate::docx::Inline::ControlEnd(c) if !start => *c == x,
                    _ => false,
                })
                .unwrap_or_else(|| panic!("{from}..{to}: no {} of {x:?}", if start { "start" } else { "end" }))
        };
        assert!(at(g, true) < at(n, true) && at(n, false) < at(g, false), "{from}..{to}: the group holds the control");
    }
}

#[test]
fn a_controls_preview_takes_its_characters_up_to_the_limit() {
    let long = "é".repeat(101);
    let paras = format!(r#"<w:p><w:r><w:t>{long}</w:t></w:r></w:p><w:p><w:r><w:t>tail</w:t></w:r></w:p>"#);
    let sdt = |tag: &str, id: u32, content: &str| {
        format!(r#"<w:sdt><w:sdtPr><w:tag w:val="{tag}"/><w:id w:val="{id}"/></w:sdtPr><w:sdtContent>{content}</w:sdtContent></w:sdt>"#)
    };
    let cell = format!(r#"<w:tc><w:tcPr><w:tcW w:w="4000" w:type="dxa"/></w:tcPr>{paras}</w:tc>"#);
    let table = format!(r#"<w:tbl><w:tblGrid><w:gridCol w:w="4000"/></w:tblGrid><w:tr>{}</w:tr></w:tbl><w:p/>"#, sdt("Cell", 6, &cell));
    let e = Editor::open(&blank_body(&format!("{}{table}", sdt("Block", 5, &paras))), fonts()).expect("opens");
    let all = crate::editing::elements::enumerate(e.document(), e.index(), &Default::default());
    let shown = |tag: &str| {
        all.iter()
            .find(|x| x.props.as_ref().is_some_and(|p| p.tag.as_deref() == Some(tag)))
            .map(|x| x.display.clone())
            .unwrap_or_else(|| panic!("no {tag}"))
    };
    assert_eq!(shown("Block"), format!("{long}\ntail"));
    assert_eq!(shown("Cell"), format!("{long} tail"));
}

fn table_body() -> String {
    let cell = "<w:tc><w:tcPr><w:tcW w:w=\"2000\" w:type=\"dxa\"/></w:tcPr><w:p><w:r><w:t>c</w:t></w:r></w:p></w:tc>";
    let row = format!("<w:tr>{cell}{cell}</w:tr>");
    format!(
        "<w:p><w:r><w:t>Intro</w:t></w:r></w:p><w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w=\"2000\"/><w:gridCol w:w=\"2000\"/></w:tblGrid>{row}{row}</w:tbl><w:p><w:r><w:t>After</w:t></w:r></w:p>"
    )
}

fn table_controls(e: &Editor) -> Vec<ControlId> {
    e.document().sections[0].blocks.iter().find_map(|b| if let Block::Table(t) = b { Some(t.controls.clone()) } else { None }).expect("the table")
}

#[test]
fn wrapping_a_selected_table_wraps_it_whole() {
    let mut e = Editor::open(&blank_body(&table_body()), fonts()).expect("opens");
    let Some(Block::Table(t)) = e.document().sections[0].blocks.get(1) else { panic!("a table") };
    let Some(Block::Para(cell)) = t.rows[0].cells[0].blocks.first() else { panic!("a cell paragraph") };
    let first = cell.id;
    send(&mut e, json!({ "type": "select", "para": first, "start": 0, "end": 0 }));
    ok(&cmd(&mut e, json!({ "type": "select", "value": "table" })));
    ok(&cmd(&mut e, json!({ "type": "insertControl", "kind": "richText", "wrapSelection": true })));
    let c = ctl(&e, |c| c.props.kind == ControlKind::RichText);
    assert_eq!(table_controls(&e), vec![c]);
}

#[test]
fn wrapping_paragraphs_ending_in_a_table_takes_the_table() {
    let mut e = Editor::open(&blank_body(&table_body()), fonts()).expect("opens");
    let Some(Block::Table(t)) = e.document().sections[0].blocks.get(1) else { panic!("a table") };
    let Some(Block::Para(last)) = t.rows[1].cells[1].blocks.first() else { panic!("a cell paragraph") };
    let (intro, last) = (para_id(&e, 0), crate::layout::ParaId(last.id));
    e.selection = Selection::range(
        Position::text(StoryId::Body, intro, Unit(0)),
        Position::text(StoryId::Body, last, Unit(1)),
        super::super::selection::Granularity::Char,
    );
    ok(&cmd(&mut e, json!({ "type": "insertControl", "kind": "group", "wrapSelection": true })));
    let c = ctl(&e, |c| c.props.kind == ControlKind::Group);
    assert_eq!(table_controls(&e), vec![c], "the table is in the control");
    assert_eq!(para_controls(&e).first(), Some(&vec![c]), "the paragraph before it too");
    assert_eq!(para_controls(&e).last(), Some(&vec![]), "not the one after");
}

#[test]
fn changing_a_date_format_keeps_its_calendar_mapping_and_date() {
    let date = r#"<w:date w:fullDate="2026-01-01T00:00:00Z"><w:dateFormat w:val="M/d/yyyy"/><w:lid w:val="en-US"/><w:storeMappedDataAs w:val="text"/><w:calendar w:val="hijri"/></w:date>"#;
    let mut e = with_control(date, "<w:r><w:t>1/1/2026</w:t></w:r>");
    let id = api(by_alias(&e, "C"));
    ok(&cmd(&mut e, json!({ "type": "setControl", "id": id, "options": { "format": "d MMMM yyyy", "locale": "en-GB" } })));
    let xml = doc_xml(&e);
    for keep in [
        r#"w:dateFormat w:val="d MMMM yyyy""#,
        r#"w:lid w:val="en-GB""#,
        r#"w:val="hijri""#,
        r#"w:storeMappedDataAs w:val="text""#,
        "w:fullDate=\"2026-01-01T00:00:00Z\"",
    ] {
        assert!(xml.contains(keep), "{keep} in {xml}");
    }
}

#[test]
fn setting_a_drop_downs_entries_replaces_them() {
    let list = r#"<w:comboBox w:lastValue="b"><w:listItem w:displayText="A" w:value="a"/><w:listItem w:displayText="B" w:value="b"/></w:comboBox>"#;
    let mut e = with_control(list, "<w:r><w:t>B</w:t></w:r>");
    let c = by_alias(&e, "C");
    let items = json!({ "items": [{ "text": "One", "value": "1" }, { "text": "Two & more", "value": "2" }] });
    ok(&cmd(&mut e, json!({ "type": "setControl", "id": api(c), "options": items })));
    let x = e.document().controls[&c].props.kind_xml.clone().unwrap_or_default();
    assert!(x.starts_with(r#"<w:comboBox w:lastValue="b">"#) && x.ends_with("</w:comboBox>"), "a combo box still: {x}");
    assert!(x.contains(r#"w:displayText="One" w:value="1""#) && x.contains(r#"w:displayText="Two &amp; more" w:value="2""#), "{x}");
    assert!(!x.contains(r#"w:value="a""#), "the old entries are gone: {x}");
    let again = Editor::open(&crate::docx::write::save(e.document()).expect("saves"), fonts()).expect("reopens");
    let x = again.document().controls[&by_alias(&again, "C")].props.kind_xml.clone().unwrap_or_default();
    assert!(x.contains(r#"w:displayText="One""#), "saved: {x}");
}

#[test]
fn wrapping_rows_across_a_row_controls_boundary_is_refused() {
    let cell = "<w:tc><w:tcPr><w:tcW w:w=\"2000\" w:type=\"dxa\"/></w:tcPr><w:p><w:r><w:t>c</w:t></w:r></w:p></w:tc>";
    let row = format!("<w:tr>{cell}</w:tr>");
    let body = format!(
        r#"<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w="2000"/></w:tblGrid><w:sdt><w:sdtPr><w:alias w:val="R"/><w:id w:val="9"/><w:richText/></w:sdtPr><w:sdtContent>{row}{row}</w:sdtContent></w:sdt>{row}</w:tbl><w:p/>"#
    );
    let mut e = Editor::open(&blank_body(&body), fonts()).expect("opens");
    let r = by_alias(&e, "R");
    let rows = |e: &Editor| {
        e.document().sections[0]
            .blocks
            .iter()
            .find_map(|b| if let Block::Table(t) = b { Some(t.rows.iter().map(|r| r.controls.clone()).collect::<Vec<_>>()) } else { None })
    };
    assert_eq!(rows(&e), Some(vec![vec![r], vec![r], vec![]]));
    let table = e.index().table_ids().next().expect("a table");
    let cells = |r0: usize, r1: usize| Selection::Cells {
        table,
        anchor: super::super::selection::CellRef { row: r0, col: 0 },
        focus: super::super::selection::CellRef { row: r1, col: 0 },
        shape: super::super::selection::CellShape::Rows,
    };
    e.selection = cells(1, 2);
    assert!(refused(&cmd(&mut e, json!({ "type": "insertControl", "kind": "richText", "wrapSelection": true }))));
    assert_eq!(rows(&e), Some(vec![vec![r], vec![r], vec![]]), "nothing changed");
    e.selection = cells(0, 1);
    ok(&cmd(&mut e, json!({ "type": "insertControl", "kind": "richText", "wrapSelection": true })));
    let rs = rows(&e).expect("a table");
    assert!(rs[0].len() == 2 && rs[0][0] == r && rs[0] == rs[1], "the new control nests inside the one both rows are in: {rs:?}");
}
