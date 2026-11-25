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

#[test]
fn typing_where_an_empty_range_follows_a_ranges_end_stays_in_a_range() {
    let read_only = DocumentProtection { kind: Some(ProtectionKind::ReadOnly), enforce: Some(true), ..Default::default() };
    let p = r#"<w:p><w:permStart w:id="1" w:edGrp="everyone"/><w:r><w:t>ab</w:t></w:r><w:permEnd w:id="1"/><w:permStart w:id="2" w:edGrp="everyone"/><w:permEnd w:id="2"/></w:p>"#;
    with_session(Some(blank_body(p)), |s| {
        s.run(Command::SetDocumentProtection(read_only)).expect("protects");
        s.with_editor(|e| e.selection = Selection::Caret(at(e, 0, 2)));
        s.run(Command::InsertText("x".into())).expect("types at the ends");
        s.run(Command::InsertText("y".into())).expect("types again");
        s.with_editor(|e| assert_eq!(texts(e), vec!["abxy".to_string()]));
    });
}

fn protected(body: &str) -> Editor {
    let mut e = Editor::open(&blank_body(body), fonts()).expect("opens");
    e.complete(fonts());
    let read_only = DocumentProtection { kind: Some(ProtectionKind::ReadOnly), enforce: Some(true), ..Default::default() };
    e.execute(Command::SetDocumentProtection(read_only), fonts());
    e
}

fn forward() -> Command {
    Command::Delete(DeleteOptions { direction: Direction::Forward, unit: None })
}

#[test]
fn delete_at_a_ranges_end_cant_join_the_next_sections_paragraph() {
    let sec = r#"<w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr>"#;
    let body = format!(
        r#"<w:p><w:pPr>{sec}</w:pPr><w:permStart w:id="1" w:edGrp="everyone"/><w:r><w:t>abc</w:t></w:r><w:permEnd w:id="1"/></w:p><w:p><w:r><w:t>def</w:t></w:r></w:p>"#
    );
    let mut e = protected(&body);
    assert_eq!(e.document().sections.len(), 2);
    e.selection = Selection::Caret(at(&e, 0, 3));
    e.execute(forward(), fonts());
    assert_eq!(e.document().sections.len(), 2, "the section break stays");
    assert_eq!(texts(&e), vec!["abc".to_string(), "def".to_string()]);
}

#[test]
fn delete_before_a_hidden_deletion_cant_take_the_protected_text_after_it() {
    let del = r#"<w:del w:id="9" w:author="B" w:date="2020-01-01T00:00:00Z"><w:r><w:delText>c</w:delText></w:r></w:del>"#;
    let body =
        format!(r#"<w:p><w:permStart w:id="1" w:edGrp="everyone"/><w:r><w:t>ab</w:t></w:r>{del}<w:permEnd w:id="1"/><w:r><w:t>d</w:t></w:r></w:p>"#);
    let mut e = protected(&body);
    e.doc.review.view = crate::docx::review::MarkupView::NoMarkup;
    e.selection = Selection::Caret(at(&e, 0, 2));
    e.execute(forward(), fonts());
    assert!(texts(&e)[0].ends_with('d'), "{:?}", texts(&e));
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

fn perm(id: u32, text: &str) -> String {
    format!(r#"<w:p><w:permStart w:id="{id}" w:edGrp="everyone"/><w:r><w:t>{text}</w:t></w:r><w:permEnd w:id="{id}"/></w:p>"#)
}

#[test]
fn an_editable_paragraph_cant_move_past_a_read_only_one() {
    let body = format!("<w:p><w:r><w:t>one</w:t></w:r></w:p>{}<w:p><w:r><w:t>three</w:t></w:r></w:p>", perm(1, "two"));
    for tracked in [false, true] {
        let mut e = protected(&body);
        e.set_tracking(tracked);
        assert_eq!(e.tracking(), tracked);
        for up in [true, false] {
            e.selection = Selection::Caret(at(&e, 1, 1));
            let o = e.move_paragraphs(up, fonts());
            assert!(matches!(o, super::super::commands::Outcome::Unchanged("locked")), "{tracked} {up}: {o:?}");
            assert_eq!(texts(&e), ["one", "two", "three"], "{tracked} {up}");
        }
    }
}

#[test]
fn an_editable_paragraph_moves_past_an_editable_one() {
    let body = format!("<w:p><w:r><w:t>one</w:t></w:r></w:p>{}{}", perm(1, "two"), perm(2, "three"));
    let mut e = protected(&body);
    e.selection = Selection::Caret(at(&e, 2, 1));
    e.move_paragraphs(true, fonts());
    assert_eq!(texts(&e), ["one", "three", "two"]);
}

#[test]
fn enforcing_asks_for_the_password_twice_and_something_to_enforce() {
    use crate::api::command::{DocumentProtection, ProtectionKind};
    use crate::editing::commands::Outcome;
    use crate::editing::protection::restriction_options;
    let mut e = Editor::open(&super::blank_body("<w:p><w:r><w:t>Text</w:t></w:r></w:p>"), super::fonts()).expect("opens");
    assert!(matches!(e.start_protection("", "", super::fonts()), Outcome::Refused(_)), "nothing to enforce");
    let kind = DocumentProtection { kind: Some(ProtectionKind::ReadOnly), ..Default::default() };
    assert!(matches!(e.execute(crate::editing::commands::Command::SetDocumentProtection(kind), super::fonts()), Outcome::Changed(_)));
    assert!(matches!(e.start_protection("pw", "wp", super::fonts()), Outcome::Refused(_)), "the passwords differ");
    assert!(matches!(e.start_protection("pw", "pw", super::fonts()), Outcome::Changed(_)));
    let p = e.document().settings.protection.as_ref().expect("protected");
    assert!(p.enforced && p.password.as_ref().is_some_and(|pw| pw.matches("pw")));
    assert_eq!((restriction_options(ProtectionKind::ReadOnly).exceptions, restriction_options(ProtectionKind::Forms).sections), (true, true));
    assert!(!restriction_options(ProtectionKind::TrackedChanges).exceptions);
}

#[test]
fn editable_ranges_list_their_editors_and_take_groups() {
    use crate::editing::commands::Outcome;
    use crate::editing::protection::RangeEditor;
    let mut e = Editor::open(&blank_body("<w:p><w:r><w:t>one two three</w:t></w:r></w:p>"), fonts()).expect("opens");
    e.complete(fonts());
    e.selection = Selection::range(at(&e, 0, 4), at(&e, 0, 7), Granularity::Char);
    for who in [RangeEditor::Group("everyone".into()), RangeEditor::User("Ada".into()), RangeEditor::Group("editors".into())] {
        assert!(matches!(e.set_range_editor(who, true, fonts()), Outcome::Changed(_)));
    }
    assert!(matches!(e.set_range_editor(RangeEditor::User("Ada".into()), true, fonts()), Outcome::Unchanged(_)), "already one");
    let list = e.editable_range_list();
    assert_eq!(list.len(), 1, "one stretch: {list:?}");
    assert_eq!(list[0].ids.len(), 3);
    assert_eq!(e.page_showing(list[0].start), Some(0), "on the first page");
    assert_eq!(e.editors_here().len(), 3);
    assert!(matches!(e.set_range_editor(RangeEditor::Group("everyone".into()), false, fonts()), Outcome::Changed(_)));
    assert_eq!(e.editors_here(), [RangeEditor::User("Ada".into()), RangeEditor::Group("editors".into())]);
    let protect = DocumentProtection { kind: Some(ProtectionKind::ReadOnly), enforce: Some(true), ..Default::default() };
    assert!(matches!(e.execute(Command::SetDocumentProtection(protect), fonts()), Outcome::Changed(_)));
    e.selection = Selection::Caret(at(&e, 0, 0));
    assert!(!e.in_editable_range("Author"));
    assert!(matches!(e.select_next_editable_range(true), Outcome::Unchanged(_)), "not one the author may edit");
    e.set_editing_groups(vec!["Editors".into()]);
    assert!(matches!(e.select_next_editable_range(true), Outcome::Moved), "a member of its group");
    assert!(e.in_editable_range("Author"));
    let (bands, marks) = e.editable_region_views(0..1);
    assert!(!bands.is_empty() && marks.len() == 2, "{bands:?} {marks:?}");
    e.selection = Selection::Caret(at(&e, 0, 0));
    assert!(matches!(e.select_all_editable_ranges(), Outcome::Changed(_) | Outcome::Moved));
    assert!(e.in_editable_range("Author"));
}

fn grant_doc() -> Editor {
    let mut e = Editor::open(&blank_body("<w:p><w:r><w:t>one two three four</w:t></w:r></w:p>"), fonts()).expect("opens");
    e.complete(fonts());
    e.selection = Selection::range(at(&e, 0, 4), at(&e, 0, 7), Granularity::Char);
    e
}

fn markers(e: &Editor) -> usize {
    inlines(e).iter().filter(|i| matches!(i, Inline::Opaque(_))).count()
}

#[test]
fn a_grant_to_a_person_isn_t_one_to_a_group_of_that_name() {
    use crate::editing::commands::Outcome;
    use crate::editing::protection::RangeEditor;
    let mut e = grant_doc();
    assert!(matches!(e.set_range_editor(RangeEditor::User("Ada".into()), true, fonts()), Outcome::Changed(_)));
    let protect = DocumentProtection { kind: Some(ProtectionKind::ReadOnly), enforce: Some(true), ..Default::default() };
    assert!(matches!(e.execute(Command::SetDocumentProtection(protect), fonts()), Outcome::Changed(_)));
    e.set_editing_groups(vec!["Ada".into()]);
    assert!(!e.in_editable_range("Bob"), "a member of a group called Ada isn't the person Ada");
    e.selection = Selection::Caret(at(&e, 0, 0));
    assert!(matches!(e.select_next_editable_range(true), Outcome::Unchanged(_)), "and navigation lists the same regions");
    e.selection = Selection::range(at(&e, 0, 4), at(&e, 0, 7), Granularity::Char);
    assert!(e.in_editable_range("ada"), "names match without case");
}

#[test]
fn a_group_grant_writes_a_canonical_group_and_refuses_others() {
    use crate::editing::commands::Outcome;
    use crate::editing::protection::RangeEditor;
    let mut e = grant_doc();
    assert!(matches!(e.set_range_editor(RangeEditor::Group("Editors".into()), true, fonts()), Outcome::Changed(_)));
    let xml = inlines(&e)
        .iter()
        .find_map(|i| if let Inline::Opaque(o) = i { o.xml.0.as_deref().filter(|x| x.contains("permStart")).map(str::to_string) } else { None });
    assert!(xml.is_some_and(|x| x.contains(r#"edGrp="editors""#)));
    let before = markers(&e);
    assert!(!matches!(e.set_range_editor(RangeEditor::Group("Marketing".into()), true, fonts()), Outcome::Changed(_)));
    assert_eq!(markers(&e), before, "no markers for a group the format doesn't have");
}

#[test]
fn a_person_s_grant_is_added_and_revoked_without_regard_to_case() {
    use crate::editing::commands::Outcome;
    use crate::editing::protection::RangeEditor;
    let mut e = grant_doc();
    assert!(matches!(e.set_range_editor(RangeEditor::User("Ada".into()), true, fonts()), Outcome::Changed(_)));
    assert!(matches!(e.set_range_editor(RangeEditor::User("ada".into()), true, fonts()), Outcome::Unchanged(_)), "no second grant");
    assert_eq!(markers(&e), 2);
    assert!(matches!(e.set_range_editor(RangeEditor::User("ADA".into()), false, fonts()), Outcome::Changed(_)));
    assert!(e.editors_here().is_empty());
    assert_eq!(markers(&e), 0);
}

#[test]
fn a_grant_reaches_every_selected_span_that_lacks_it() {
    use crate::editing::commands::Outcome;
    use crate::editing::protection::RangeEditor;
    let mut e = grant_doc();
    let ada = RangeEditor::User("Ada".into());
    assert!(matches!(e.set_range_editor(ada.clone(), true, fonts()), Outcome::Changed(_)));
    // The primary span has the grant; a second selected span doesn't.
    e.select_ranges(&[(at(&e, 0, 12), at(&e, 0, 15)), (at(&e, 0, 4), at(&e, 0, 7))]);
    assert!(e.is_discontiguous());
    assert!(matches!(e.set_range_editor(ada.clone(), true, fonts()), Outcome::Changed(_)), "the other span gets one");
    assert_eq!(markers(&e), 4, "one range each, the first not doubled");
    let spans: Vec<_> = e.editable_range_list().into_iter().map(|r| (r.start, r.end, r.ids.len(), r.editors)).collect();
    assert_eq!(
        spans,
        [(at(&e, 0, 4), at(&e, 0, 7), 1, vec![ada.clone()]), (at(&e, 0, 12), at(&e, 0, 15), 1, vec![ada.clone()])],
        "each span granted once"
    );
    assert!(matches!(e.set_range_editor(ada, true, fonts()), Outcome::Unchanged(_)), "all granted: nothing to do");
}

#[test]
fn a_new_permission_takes_an_id_word_reads() {
    use crate::editing::commands::Outcome;
    use crate::editing::protection::RangeEditor;
    let perm = |id: i64, t: &str| format!(r#"<w:permStart w:id="{id}" w:edGrp="everyone"/><w:r><w:t>{t}</w:t></w:r><w:permEnd w:id="{id}"/>"#);
    let body = format!("<w:p>{}{}<w:r><w:t> one two three</w:t></w:r></w:p>", perm(i64::from(i32::MAX), "a"), perm(i64::MAX, "b"));
    let mut e = Editor::open(&blank_body(&body), fonts()).expect("opens");
    e.complete(fonts());
    e.select_ranges(&[(at(&e, 0, 3), at(&e, 0, 6)), (at(&e, 0, 7), at(&e, 0, 10))]);
    assert!(matches!(e.set_range_editor(RangeEditor::User("Ada".into()), true, fonts()), Outcome::Changed(_)));
    let mut ids: Vec<i64> = inlines(&e).iter().filter_map(|i| if let Inline::Opaque(o) = i { o.permission() } else { None }).map(|p| p.1).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids, [1, 2, i64::from(i32::MAX), i64::MAX], "the free ids from the first, past none Word reads");
}

#[test]
fn permissions_for_one_person_named_in_two_cases_list_them_once() {
    let span =
        r#"<w:permStart w:id="1" w:ed="Ada"/><w:permStart w:id="2" w:ed="ada"/><w:r><w:t>one</w:t></w:r><w:permEnd w:id="2"/><w:permEnd w:id="1"/>"#;
    let e = Editor::open(&blank_body(&format!("<w:p>{span}</w:p>")), fonts()).expect("opens");
    let list = e.editable_range_list();
    let [r] = &list[..] else { panic!("one range: {list:?}") };
    assert_eq!((r.ids.len(), r.editors.len()), (2, 1), "both permissions, one editor: {r:?}");
}
