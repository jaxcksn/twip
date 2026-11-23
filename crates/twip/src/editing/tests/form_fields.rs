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
