//! Document protection: undo across enforcement, editable ranges' ends

use super::super::Editor;
use super::super::commands::Command;
use super::super::position::{Position, StoryId};
use super::super::selection::{Granularity, Selection};
use super::form_fields::with_session;
use super::{blank_body, fonts, para_id, texts};
use crate::api::command::{DeleteOptions, DocumentProtection, ProtectionKind};
use crate::api::session::Session;
use crate::api::value::Direction;
use crate::docx::{Block, Inline};
use crate::layout::Unit;

fn at(e: &Editor, para: usize, unit: u32) -> Position {
    Position::text(StoryId::Body, para_id(e, para), Unit(unit))
}

fn inlines(e: &Editor) -> Vec<Inline> {
    match &e.document().sections[0].blocks[0] {
        Block::Para(p) => p.inlines.clone(),
        Block::Table(_) => panic!("a paragraph"),
    }
}

fn enforced(s: &mut Session) -> bool {
    s.with_editor(|e| e.document().settings.protection.as_ref().is_some_and(|p| p.enforced))
}

#[test]
fn undo_cannot_lift_enforced_protection_or_its_edits() {
    with_session(None, |s| {
        s.run(Command::InsertText("hello".into())).expect("types");
        let on = DocumentProtection { kind: Some(ProtectionKind::ReadOnly), enforce: Some(true), password: Some("pw".into()), ..Default::default() };
        s.run(Command::SetDocumentProtection(on)).expect("protects");
        assert!(enforced(s));
        assert!(s.run(Command::InsertText("x".into())).is_err(), "protected");
        let _ = s.run(Command::Undo);
        let _ = s.run(Command::Undo);
        assert!(enforced(s), "protection survives Undo");
        assert_eq!(s.with_editor(|e| texts(e)), vec!["hello".to_string()], "text survives Undo");
        let off = DocumentProtection { enforce: Some(false), password: Some("pw".into()), ..Default::default() };
        s.run(Command::SetDocumentProtection(off)).expect("stops with the password");
        let _ = s.run(Command::Undo);
        let _ = s.run(Command::Redo);
        assert!(!enforced(s), "undo can't put protection back to lift again");
        assert!(s.with_editor(|e| e.is_dirty()), "still modified");
    });
}

#[test]
fn typing_at_an_editable_ranges_end_stays_in_it() {
    let read_only = DocumentProtection { kind: Some(ProtectionKind::ReadOnly), enforce: Some(true), ..Default::default() };
    let perm_end_last = |e: &Editor| matches!(inlines(e).last(), Some(Inline::Opaque(o)) if o.permission() == Some((false, 1)));
    let range = r#"<w:p><w:permStart w:id="1" w:edGrp="everyone"/><w:r><w:t>abc</w:t></w:r><w:permEnd w:id="1"/></w:p>"#;
    with_session(Some(blank_body(range)), |s| {
        s.run(Command::SetDocumentProtection(read_only.clone())).expect("protects");
        s.with_editor(|e| e.selection = Selection::Caret(at(e, 0, 3)));
        s.run(Command::InsertText("d".into())).expect("types at the end");
        s.run(Command::InsertText("e".into())).expect("types again");
        s.with_editor(|e| {
            assert_eq!(texts(e), vec!["abcde".to_string()]);
            assert!(perm_end_last(e), "the range's end is after the text");
            assert!(e.in_editable_range("someone"), "the caret is still in the range");
        });
    });
    let empty = r#"<w:p><w:r><w:t>ab</w:t></w:r><w:permStart w:id="1" w:edGrp="everyone"/><w:permEnd w:id="1"/></w:p>"#;
    with_session(Some(blank_body(empty)), |s| {
        s.run(Command::SetDocumentProtection(read_only)).expect("protects");
        s.with_editor(|e| e.selection = Selection::Caret(at(e, 0, 2)));
        s.run(Command::InsertText("x".into())).expect("types in the empty range");
        s.run(Command::InsertText("y".into())).expect("types again");
        s.with_editor(|e| {
            assert_eq!(texts(e), vec!["abxy".to_string()]);
            assert!(perm_end_last(e), "the range holds what was typed");
        });
    });
}

#[test]
fn typing_between_two_editable_ranges_goes_in_the_one_the_author_may_edit() {
    let read_only = DocumentProtection { kind: Some(ProtectionKind::ReadOnly), enforce: Some(true), ..Default::default() };
    let p = r#"<w:p><w:permStart w:id="1" w:edGrp="administrators"/><w:r><w:t>abc</w:t></w:r><w:permEnd w:id="1"/><w:permStart w:id="2" w:edGrp="everyone"/><w:r><w:t>def</w:t></w:r><w:permEnd w:id="2"/></w:p>"#;
    with_session(Some(blank_body(p)), |s| {
        s.run(Command::SetDocumentProtection(read_only)).expect("protects");
        s.with_editor(|e| e.selection = Selection::Caret(at(e, 0, 3)));
        s.run(Command::InsertText("Z".into())).expect("types at the second range's start");
        s.with_editor(|e| {
            let inl = inlines(e);
            let start2 = inl.iter().position(|i| matches!(i, Inline::Opaque(o) if o.permission() == Some((true, 2)))).expect("range 2");
            let z = inl.iter().position(|i| matches!(i, Inline::Text(t, _) if t.contains('Z'))).expect("typed");
            assert!(z > start2, "the text is in range 2, not range 1: {inl:?}");
            assert_eq!(texts(e), vec!["abcZdef".to_string()]);
        });
    });
}

fn delete(e: &mut Editor, a: u32, b: u32) {
    e.selection = Selection::range(at(e, 0, a), at(e, 0, b), Granularity::Char);
    e.execute(Command::Delete(DeleteOptions { direction: Direction::Forward, unit: None }), fonts());
}

fn bookmarked() -> Editor {
    let p = r#"<w:p><w:r><w:t xml:space="preserve">See </w:t></w:r><w:bookmarkStart w:id="1" w:name="B"/><w:r><w:t>Figure 1</w:t></w:r><w:bookmarkEnd w:id="1"/><w:r><w:t>: Title text</w:t></w:r></w:p>"#;
    Editor::open(&blank_body(p), fonts()).expect("opens")
}

#[test]
fn deleting_a_bookmarks_end_shrinks_it() {
    let mut e = bookmarked();
    let b = e.bookmark("B").expect("bookmark");
    assert_eq!((b.start.unit, b.end.map(|x| x.unit)), (Unit(4), Some(Unit(12))));
    delete(&mut e, 9, 17);
    let b = e.bookmark("B").expect("B survives");
    assert_eq!((b.start.unit, b.end.map(|x| x.unit)), (Unit(4), Some(Unit(9))), "its end moves to the deletion point");
    delete(&mut e, 2, 12);
    assert!(e.bookmark("B").is_none(), "deleted whole, it goes");
    assert!(!inlines(&e).iter().any(|i| matches!(i, Inline::Bookmark(_))), "no marker left");
}

#[test]
fn deleting_a_bookmarks_start_shrinks_it() {
    let mut e = bookmarked();
    delete(&mut e, 2, 8);
    let b = e.bookmark("B").expect("B survives");
    assert_eq!((b.start.unit, b.end.map(|x| x.unit)), (Unit(2), Some(Unit(6))), "its start moves to the deletion point");
}

#[test]
fn deleting_an_editable_ranges_end_shrinks_it() {
    let p = r#"<w:p><w:r><w:t xml:space="preserve">aaa </w:t></w:r><w:permStart w:id="1" w:edGrp="everyone"/><w:r><w:t>bbbb</w:t></w:r><w:permEnd w:id="1"/><w:r><w:t xml:space="preserve"> cccc</w:t></w:r></w:p>"#;
    let mut e = Editor::open(&blank_body(p), fonts()).expect("opens");
    delete(&mut e, 6, 11);
    let ends = inlines(&e).iter().filter(|i| matches!(i, Inline::Opaque(o) if o.permission().is_some_and(|(s, _)| !s))).count();
    assert_eq!(ends, 1, "the permission end survives");
    e.selection = Selection::Caret(at(&e, 0, 5));
    assert!(e.in_editable_range("someone"), "what is left of the range is editable");
}
