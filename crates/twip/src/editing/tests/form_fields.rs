//! Legacy form fields: kinds, options, names and filling in

use super::super::Editor;
use super::super::commands::Command;
use super::super::position::{Position, StoryId};
use super::super::selection::Selection;
use super::{blank_body, fonts, para_id};
use crate::api::command::{
    CheckBoxFormOptions, DocumentProtection, DropDownFormOptions, FormFieldChange, FormFieldFill, FormFieldType, ProtectionKind,
};
use crate::api::engine::{Engine, SessionOptions};
use crate::api::fonts::FontLibrary;
use crate::api::session::Session;
use crate::docx::forms::{FormData, FormKind};
use crate::docx::{Block, FieldKind, Inline};
use crate::layout::Unit;

pub(super) fn with_session<R>(doc: Option<Vec<u8>>, f: impl FnOnce(&mut Session) -> R) -> R {
    let mut engine = Engine::new(FontLibrary::empty());
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("a session");
    if let Some(d) = doc {
        s.open(&d).expect("opens");
    }
    f(s)
}

fn forms(e: &Editor) -> Vec<(Option<FieldKind>, FormData)> {
    e.document().sections[0]
        .blocks
        .iter()
        .filter_map(|b| if let Block::Para(p) = b { Some(p) } else { None })
        .flat_map(|p| p.inlines.iter())
        .filter_map(|i| match i {
            Inline::FieldBegin { kind, form: Some(f), .. } => Some((kind.clone(), f.data.clone())),
            _ => None,
        })
        .collect()
}

fn first_form(s: &mut Session) -> (Option<FieldKind>, FormData) {
    s.with_editor(|e| forms(e)).remove(0)
}

const CHECK_BOX: &str = r#"<w:p><w:r><w:fldChar w:fldCharType="begin"><w:ffData><w:name w:val="Check1"/><w:enabled/><w:checkBox><w:size w:val="24"/></w:checkBox></w:ffData></w:fldChar></w:r><w:r><w:instrText xml:space="preserve"> FORMCHECKBOX </w:instrText></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r><w:r><w:t>b</w:t></w:r></w:p>"#;

#[test]
fn inserted_check_boxes_and_drop_downs_keep_their_kind() {
    with_session(None, |s| {
        s.run(Command::InsertFormField(FormFieldType::CheckBox)).expect("inserts");
        assert!(matches!(first_form(s).0, Some(FieldKind::CheckBox { .. })), "check box kind: {:?}", first_form(s).0);
    });
    with_session(None, |s| {
        s.run(Command::InsertFormField(FormFieldType::DropDown)).expect("inserts");
        assert!(matches!(first_form(s).0, Some(FieldKind::DropDown)), "drop-down kind: {:?}", first_form(s).0);
    });
}

#[test]
fn typing_beside_a_check_box_keeps_its_kind() {
    let mut e = Editor::open(&blank_body(CHECK_BOX), fonts()).expect("opens");
    assert!(matches!(forms(&e)[0].0, Some(FieldKind::CheckBox { .. })), "read as a check box");
    let end = super::super::text::UnitText::of(match &e.document().sections[0].blocks[0] {
        Block::Para(p) => p,
        Block::Table(_) => panic!("a paragraph"),
    })
    .text
    .chars()
    .count() as u32;
    e.selection = Selection::Caret(Position::text(StoryId::Body, para_id(&e, 0), Unit(end)));
    e.execute(Command::InsertText("x".into()), fonts());
    assert!(matches!(forms(&e)[0].0, Some(FieldKind::CheckBox { .. })), "after typing: {:?}", forms(&e)[0].0);
}

#[test]
fn shrinking_a_drop_downs_entries_keeps_its_default_in_them() {
    with_session(None, |s| {
        s.run(Command::InsertFormField(FormFieldType::DropDown)).expect("inserts");
        let entries = |entries: Vec<&str>, default| {
            Command::SetFormField(FormFieldChange {
                drop_down: Some(DropDownFormOptions { entries: Some(entries.into_iter().map(String::from).collect()), default }),
                ..Default::default()
            })
        };
        s.run(entries(vec!["A", "B", "C"], Some(2))).expect("sets");
        s.run(entries(vec!["A"], None)).expect("shrinks");
        let FormKind::DropDown { entries, default, .. } = first_form(s).1.kind else { panic!("a drop-down") };
        assert!((default as usize) < entries.len(), "default {default} of {} entries", entries.len());
    });
}

#[test]
fn a_form_fields_common_options_apply_beside_another_kinds_options() {
    with_session(None, |s| {
        s.run(Command::InsertFormField(FormFieldType::Text)).expect("inserts");
        s.run(Command::SetFormField(FormFieldChange {
            name: Some("Agree".into()),
            check_box: Some(CheckBoxFormOptions { checked_by_default: Some(true), ..Default::default() }),
            ..Default::default()
        }))
        .expect("sets");
        assert_eq!(first_form(s).1.name, "Agree");
    });
}

#[test]
fn an_inserted_form_fields_name_is_a_bookmark_around_it() {
    with_session(None, |s| {
        s.run(Command::InsertFormField(FormFieldType::CheckBox)).expect("inserts");
        let name = first_form(s).1.name;
        let b = s.with_editor(|e| e.bookmark(&name)).unwrap_or_else(|| panic!("no bookmark {name}"));
        assert!(b.start.unit == Unit(0) && b.end.is_some_and(|e| e.unit > b.start.unit), "around the field: {b:?}");
    });
}

#[test]
fn renaming_a_form_field_renames_its_bookmark() {
    with_session(None, |s| {
        s.run(Command::InsertFormField(FormFieldType::Text)).expect("inserts");
        let was = first_form(s).1.name;
        s.run(Command::SetFormField(FormFieldChange { name: Some("Client".into()), ..Default::default() })).expect("renames");
        assert!(s.with_editor(|e| e.bookmark("Client")).is_some(), "the new name is a bookmark");
        assert!(s.with_editor(|e| e.bookmark(&was)).is_none(), "the old one is gone");
    });
}

#[test]
fn renaming_a_form_field_leaves_another_bookmark_of_its_name() {
    let field = r#"<w:p><w:r><w:fldChar w:fldCharType="begin"><w:ffData><w:name w:val="Text1"/><w:enabled/><w:textInput/></w:ffData></w:fldChar></w:r><w:r><w:instrText xml:space="preserve"> FORMTEXT </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>abc</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r></w:p>"#;
    let other =
        r#"<w:p><w:r><w:t>x </w:t></w:r><w:bookmarkStart w:id="7" w:name="Text1"/><w:r><w:t>other</w:t></w:r><w:bookmarkEnd w:id="7"/></w:p>"#;
    let mut e = Editor::open(&blank_body(&format!("{field}{other}")), fonts()).expect("opens");
    let f = e.document().sections[0]
        .blocks
        .iter()
        .filter_map(|b| if let Block::Para(p) = b { Some(p) } else { None })
        .flat_map(|p| p.inlines.iter())
        .find_map(|i| if let Inline::FieldBegin { id, form: Some(_), .. } = i { Some(*id) } else { None })
        .expect("a form field");
    let id = Some(super::super::merge::api_id(crate::docx::ElementId::Field(f)));
    let out = e.execute(Command::SetFormField(FormFieldChange { id, name: Some("Customer".into()), ..Default::default() }), fonts());
    assert!(matches!(out, super::super::commands::Outcome::Changed(_)), "{out:?}");
    let other = e.bookmark("Text1").expect("the other bookmark stays");
    assert_eq!(other.start.para, para_id(&e, 1), "where it was");
    let own = e.bookmark("Customer").expect("the field's own bookmark");
    assert_eq!((own.start.para, own.start.unit), (para_id(&e, 0), Unit(0)), "around the field");
}

#[test]
fn a_form_fields_name_is_unique() {
    with_session(None, |s| {
        s.run(Command::InsertFormField(FormFieldType::Text)).expect("inserts");
        s.run(Command::InsertFormField(FormFieldType::Text)).expect("inserts");
        let names: Vec<String> = s.with_editor(|e| forms(e)).into_iter().map(|f| f.1.name).collect();
        assert_eq!(names.len(), 2);
        assert_ne!(names[0], names[1]);
        let out = s.run(Command::SetFormField(FormFieldChange { name: Some(names[0].clone()), ..Default::default() }));
        assert!(out.is_err(), "another field's name is refused");
        let now: Vec<String> = s.with_editor(|e| forms(e)).into_iter().map(|f| f.1.name).collect();
        assert_eq!(now, names);
    });
}

#[test]
fn a_disabled_form_field_is_not_filled_in() {
    let protect = DocumentProtection { kind: Some(ProtectionKind::Forms), enforce: Some(true), ..Default::default() };
    let fill = Command::FillFormField(FormFieldFill { checked: Some(true), ..Default::default() });
    let checked = |s: &mut Session| matches!(first_form(s).1.kind, FormKind::CheckBox { checked: Some(true), .. });
    with_session(None, |s| {
        s.run(Command::InsertFormField(FormFieldType::CheckBox)).expect("inserts");
        s.run(Command::SetDocumentProtection(protect.clone())).expect("protects");
        s.run(fill.clone()).expect("fills");
        assert!(checked(s), "an enabled field fills under forms protection");
    });
    with_session(None, |s| {
        s.run(Command::InsertFormField(FormFieldType::CheckBox)).expect("inserts");
        s.run(Command::SetFormField(FormFieldChange { enabled: Some(false), ..Default::default() })).expect("disables");
        s.run(Command::SetDocumentProtection(protect)).expect("protects");
        assert!(s.run(fill).is_err(), "refused");
        assert!(!checked(s), "a disabled field is not filled");
    });
}

#[test]
fn a_form_fields_every_option_reads_back() {
    with_session(None, |s| {
        s.run(Command::InsertFormField(FormFieldType::CheckBox)).expect("inserts");
        s.run(Command::SetFormField(FormFieldChange {
            entry_macro: Some("OnEnter".into()),
            exit_macro: Some("OnExit".into()),
            check_box: Some(CheckBoxFormOptions {
                size: Some(crate::api::value::AutoOr::Value(crate::api::value::Points(12.0))),
                checked_by_default: Some(true),
            }),
            ..Default::default()
        }))
        .expect("sets");
        let d = s.with_editor(|e| {
            let f = e.document().sections[0]
                .blocks
                .iter()
                .filter_map(|b| if let Block::Para(p) = b { Some(p) } else { None })
                .flat_map(|p| p.inlines.iter())
                .find_map(|i| if let Inline::FieldBegin { id, form: Some(_), .. } = i { Some(*id) } else { None })
                .expect("a form field");
            e.form_data(Some(&crate::editing::merge::api_id(crate::docx::ElementId::Field(f)))).cloned()
        });
        let d = d.expect("a form field");
        assert_eq!((d.entry_macro.as_str(), d.exit_macro.as_str()), ("OnEnter", "OnExit"));
        assert!(matches!(d.kind, FormKind::CheckBox { size: Some(12.0), default: true, .. }), "{:?}", d.kind);
    });
    let text = r#"<w:p><w:r><w:fldChar w:fldCharType="begin"><w:ffData><w:name w:val="T"/><w:enabled/><w:helpText w:type="autoText" w:val="Entry"/><w:textInput><w:format w:val="UPPERCASE"/></w:textInput></w:ffData></w:fldChar></w:r><w:r><w:instrText xml:space="preserve"> FORMTEXT </w:instrText></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r></w:p>"#;
    let mut e = Editor::open(&blank_body(text), fonts()).expect("opens");
    e.selection = Selection::Caret(Position::text(StoryId::Body, para_id(&e, 0), Unit(0)));
    let d = e.form_data(None).expect("the field at the selection");
    assert_eq!(d.help, (true, "Entry".to_string()), "an AutoText help entry");
    assert!(matches!(&d.kind, FormKind::Text { format, .. } if format == "UPPERCASE"));
}

/// A legacy form field's runs: its `data` options, its `code` and its `result`.
fn form_field_xml(data: &str, code: &str, result: &str) -> String {
    let result =
        if result.is_empty() { String::new() } else { format!(r#"<w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>{result}</w:t></w:r>"#) };
    format!(
        r#"<w:r><w:fldChar w:fldCharType="begin"><w:ffData><w:enabled/>{data}</w:ffData></w:fldChar></w:r><w:r><w:instrText xml:space="preserve"> {code} </w:instrText></w:r>{result}<w:r><w:fldChar w:fldCharType="end"/></w:r>"#
    )
}

/// A paragraph with a text form field, a check box and a drop-down, each after a letter.
fn three_fields() -> Editor {
    let field = form_field_xml;
    let body = format!(
        r#"<w:p><w:r><w:t xml:space="preserve">A </w:t></w:r>{}<w:r><w:t xml:space="preserve"> B </w:t></w:r>{}<w:r><w:t xml:space="preserve"> C </w:t></w:r>{}</w:p>"#,
        field(TEXT_FIELD, "FORMTEXT", "abc"),
        field(CHECK_BOX_FIELD, "FORMCHECKBOX", ""),
        field(r#"<w:name w:val="D"/><w:ddList><w:listEntry w:val="One"/><w:listEntry w:val="Two"/></w:ddList>"#, "FORMDROPDOWN", "One"),
    );
    let mut e = Editor::open(&blank_body(&body), fonts()).expect("opens");
    e.complete(fonts());
    e
}

const TEXT_FIELD: &str = r#"<w:name w:val="T"/><w:textInput/>"#;
const CHECK_BOX_FIELD: &str = r#"<w:name w:val="K"/><w:checkBox><w:sizeAuto/></w:checkBox>"#;

fn protect(e: &mut Editor) {
    let p = DocumentProtection { kind: Some(ProtectionKind::Forms), enforce: Some(true), ..Default::default() };
    assert!(matches!(e.execute(Command::SetDocumentProtection(p), fonts()), super::super::commands::Outcome::Changed(_)));
}

/// The unit the `n`th field of the first paragraph starts at, and a point on it.
fn field_at(e: &mut Editor, n: usize) -> (u32, f64, f64) {
    let Some(Block::Para(p)) = e.document().sections[0].blocks.first() else { panic!("a paragraph") };
    let s = crate::docx::fields::spans(&p.inlines)[n];
    let u: u32 = p.inlines[..s.begin].iter().map(|i| i.units() as u32).sum();
    let c = e.caret_at(Position::text(StoryId::Body, para_id(e, 0), Unit(u))).expect("laid out");
    let (x, y) = e.stack().to_doc(c.page, c.x + 2.0, (c.top + c.bottom) / 2.0).expect("on the stack");
    (u, x, y)
}

fn check_box_checked(e: &Editor) -> bool {
    forms(e).iter().any(|(_, d)| matches!(d.kind, FormKind::CheckBox { checked: Some(true), .. }))
}

#[test]
fn a_legacy_check_box_toggles_by_click_space_and_x_only_where_forms_are_filled_in() {
    use super::super::input::PointerKind;
    use super::super::keymap::Mods;
    let mut e = three_fields();
    let (_, x, y) = field_at(&mut e, 1);
    let click = |e: &mut Editor| {
        e.pointer_input(PointerKind::Down, x, y, 1, Mods::default(), fonts());
        e.pointer_input(PointerKind::Up, x, y, 1, Mods::default(), fonts());
    };
    click(&mut e);
    assert!(!check_box_checked(&e), "unprotected: a click doesn't toggle it");
    protect(&mut e);
    click(&mut e);
    assert!(check_box_checked(&e), "a click toggles it");
    e.execute(Command::InsertText(" ".into()), fonts());
    assert!(!check_box_checked(&e), "Space toggles it, still selected");
    e.execute(Command::InsertText("x".into()), fonts());
    assert!(check_box_checked(&e), "so does X");
    assert_eq!(super::texts(&e)[0].matches(" B ").count(), 1, "no text typed");
}

#[test]
fn tab_moves_between_the_fields_to_fill_in_and_round() {
    let mut e = three_fields();
    protect(&mut e);
    e.selection = Selection::Caret(Position::text(StoryId::Body, para_id(&e, 0), Unit(0)));
    assert!(e.tab_moves_between_form_fields());
    let selected = |e: &Editor| e.selection.ordered(&e.index).map(|(a, b)| (a.para_offset().unwrap().1.0, b.para_offset().unwrap().1.0));
    let (t, _, _) = field_at(&mut e, 0);
    let (k, _, _) = field_at(&mut e, 1);
    let (d, _, _) = field_at(&mut e, 2);
    let mut seen = Vec::new();
    for _ in 0..4 {
        e.select_next_form_field(true);
        seen.push(selected(&e).expect("a selection"));
    }
    let text_result = (t + " FORMTEXT ".chars().count() as u32 + 2, t + " FORMTEXT ".chars().count() as u32 + 5);
    assert_eq!(seen[0], text_result, "the text field's result");
    assert_eq!(seen[1].0, k, "the check box, whole");
    assert!(seen[2].0 > d, "the drop-down's result");
    assert_eq!(seen[3], seen[0], "round to the first");
    e.select_next_form_field(false);
    assert_eq!(selected(&e), Some(seen[2]), "Shift+Tab goes back round");
}

#[test]
fn tab_visits_fields_over_paragraphs_and_table_cells_in_document_order() {
    let field = |n: usize| {
        let (data, code, result) = match n % 3 {
            0 => (format!(r#"<w:name w:val="T{n}"/><w:textInput/>"#), "FORMTEXT", "abc"),
            1 => (format!(r#"<w:name w:val="K{n}"/><w:checkBox><w:sizeAuto/></w:checkBox>"#), "FORMCHECKBOX", ""),
            _ => (format!(r#"<w:name w:val="D{n}"/><w:ddList><w:listEntry w:val="One"/></w:ddList>"#), "FORMDROPDOWN", "One"),
        };
        format!(r#"<w:p><w:r><w:t xml:space="preserve">A </w:t></w:r>{}</w:p>"#, form_field_xml(&data, code, result))
    };
    // Paragraphs 0-3, a table cell's (4), then 5-9
    let mut body: String = (0..4).map(field).collect();
    body += &format!(r#"<w:tbl><w:tblGrid><w:gridCol w:w="4000"/></w:tblGrid><w:tr><w:tc>{}</w:tc></w:tr></w:tbl>"#, field(4));
    body.extend((5..10).map(field));
    let mut e = Editor::open(&blank_body(&body), fonts()).expect("opens");
    e.complete(fonts());
    protect(&mut e);
    e.selection = Selection::Caret(Position::text(StoryId::Body, para_id(&e, 0), Unit(0)));
    let para = |e: &Editor| e.selection.ordered(&e.index).and_then(|(a, _)| a.para_offset()).map(|p| p.0);
    let order: Vec<_> = (0..10).map(|n| para_id(&e, n)).collect();
    let mut seen = Vec::new();
    for _ in 0..11 {
        e.select_next_form_field(true);
        seen.push(para(&e).expect("a selection"));
    }
    assert_eq!(seen[..10], order[..], "Tab in document order");
    assert_eq!(seen[10], order[0], "round to the first");
    let mut back = Vec::new();
    for _ in 0..10 {
        e.select_next_form_field(false);
        back.push(para(&e).expect("a selection"));
    }
    let expected: Vec<_> = order.iter().rev().copied().collect();
    assert_eq!(back, expected, "Shift+Tab back round in reverse order");
}

#[test]
fn unprotected_typing_over_a_selected_check_box_replaces_it() {
    for typed in ["x", " "] {
        let body = format!(
            r#"<w:p><w:r><w:t xml:space="preserve">A </w:t></w:r>{}<w:r><w:t xml:space="preserve"> B</w:t></w:r></w:p>"#,
            form_field_xml(CHECK_BOX_FIELD, "FORMCHECKBOX", "")
        );
        let mut e = Editor::open(&blank_body(&body), fonts()).expect("opens");
        e.complete(fonts());
        let Some(Block::Para(p)) = e.document().sections[0].blocks.first() else { panic!("a paragraph") };
        let s = crate::docx::fields::spans(&p.inlines)[0];
        let units = |r: std::ops::Range<usize>| p.inlines[r].iter().map(|i| i.units() as u32).sum::<u32>();
        let (start, end) = (units(0..s.begin), units(0..s.end + 1));
        let para = para_id(&e, 0);
        e.selection = Selection::range(
            Position::text(StoryId::Body, para, Unit(start)),
            Position::text(StoryId::Body, para, Unit(end)),
            super::super::selection::Granularity::Char,
        );
        assert!(e.selected_form_check_box().is_none(), "no forms protection");
        e.execute(Command::InsertText(typed.into()), fonts());
        assert!(forms(&e).is_empty(), "{typed:?} replaces the field");
        assert_eq!(super::texts(&e)[0], format!("A {typed} B"));
    }
}

#[test]
fn f4_finds_the_drop_down_at_the_selection_only_where_forms_are_filled_in() {
    let mut e = three_fields();
    let drop_down = |e: &Editor| {
        let Some(Block::Para(p)) = e.document().sections[0].blocks.first() else { panic!("a paragraph") };
        match &p.inlines[crate::docx::fields::spans(&p.inlines)[2].begin] {
            Inline::FieldBegin { id, .. } => *id,
            _ => panic!("a field"),
        }
    };
    let (d, _, _) = field_at(&mut e, 2);
    e.selection = Selection::Caret(Position::text(StoryId::Body, para_id(&e, 0), Unit(d + 3)));
    assert_eq!(e.selected_form_drop_down(), None, "unprotected");
    protect(&mut e);
    e.selection = Selection::Caret(Position::text(StoryId::Body, para_id(&e, 0), Unit(0)));
    assert_eq!(e.selected_form_drop_down(), None, "not in one");
    for _ in 0..3 {
        e.select_next_form_field(true);
    }
    assert_eq!(e.selected_form_drop_down(), Some(drop_down(&e)), "Tab selected its result");
    e.select_next_form_field(true);
    assert_eq!(e.selected_form_drop_down(), None, "the text field");
}

#[test]
fn a_drop_down_click_and_a_form_field_double_click_ask_the_host() {
    use super::super::forms::FormPress;
    let mut e = three_fields();
    let (_, x, y) = field_at(&mut e, 2);
    assert_eq!(e.form_field_press_at(x, y, 1, fonts()).map(|p| p.1), None, "unprotected: a click places the caret");
    assert_eq!(e.form_field_press_at(x, y, 2, fonts()).map(|p| p.1), Some(FormPress::Options));
    let (_, tx, ty) = field_at(&mut e, 0);
    assert_eq!(e.form_field_press_at(tx, ty, 2, fonts()).map(|p| p.1), Some(FormPress::Options), "any form field's options");
    protect(&mut e);
    assert_eq!(e.form_field_press_at(x, y, 1, fonts()).map(|p| p.1), Some(FormPress::OpenList));
    assert_eq!(e.form_field_press_at(tx, ty, 1, fonts()).map(|p| p.1), None, "a text field is typed in");
}
