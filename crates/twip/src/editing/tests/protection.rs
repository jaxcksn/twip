//! Locks and document protection on the edit paths that aren't a command's

use std::time::Instant;

use serde_json::json;

use super::super::Editor;
use super::super::commands::{Command, Outcome};
use super::super::input::PointerKind;
use super::super::keymap::Mods;
use super::super::position::{Position, StoryId};
use super::super::selection::Selection;
use super::{blank_body, fonts, open, para_id, texts};
use crate::api::command::{
    CaseChange, DocumentProtection, InsertSymbol, ObjectSize, ProtectedPictureControls, ProtectionKind, RelativeSize, RelativeSizeReference,
    SectionBreakKind,
};
use crate::api::value::{Alignment, NoneOr, Points};
use crate::docx::Inline;
use crate::layout::Unit;

fn lay(body: &str) -> Editor {
    let mut e = Editor::open(&blank_body(body), fonts()).expect("opens");
    e.complete(fonts());
    e
}

fn refused(o: &Outcome) -> bool {
    matches!(o, Outcome::Unchanged(_) | Outcome::Refused(_))
}

/// The stops (x, unit) of `para` on page 0 and its line's middle, in page coordinates.
fn stops(e: &mut Editor, para: usize) -> (Vec<(f64, Unit)>, f64) {
    let id = para_id(e, para);
    let g = e.page_geometries().into_iter().find(|(p, _)| *p == 0).map(|(_, g)| g).expect("page 0");
    let l = g.lines.iter().find(|l| l.shows(id)).expect("a line");
    (l.stops.iter().filter(|s| s.para == id).map(|s| (s.x, s.unit)).collect(), (l.top + l.bottom) / 2.0)
}

/// The document point of the `k`th stop of `para`, nudged right by `dx`.
fn nth(e: &mut Editor, para: usize, k: usize, dx: f64) -> (f64, f64) {
    let (s, y) = stops(e, para);
    e.stack().to_doc(0, s[k].0 + dx, y).expect("on the stack")
}

/// The document point of `para`'s unit `u`, nudged right by `dx`.
fn at(e: &mut Editor, para: usize, u: u32, dx: f64) -> (f64, f64) {
    let k = stops(e, para).0.iter().position(|s| s.1 == Unit(u)).unwrap_or_else(|| panic!("no stop {u}"));
    nth(e, para, k, dx)
}

fn drag(e: &mut Editor, from: (f64, f64), to: (f64, f64)) {
    let m = Mods::default();
    e.pointer_input(PointerKind::Down, from.0, from.1, 1, m, fonts());
    e.pointer_input(PointerKind::Move, to.0, to.1, 0, m, fonts());
    e.pointer_input(PointerKind::Up, to.0, to.1, 0, m, fonts());
}

fn select(e: &mut Editor, para: usize, a: u32, b: u32) {
    let id = para_id(e, para);
    e.select_range(id, Unit(a), Unit(b), fonts());
}

const LOCKED: &str = r#"<w:sdt><w:sdtPr><w:id w:val="5"/><w:lock w:val="sdtContentLocked"/><w:text/></w:sdtPr><w:sdtContent><w:r><w:t>Locked</w:t></w:r></w:sdtContent></w:sdt>"#;

fn run(t: &str) -> String {
    format!(r#"<w:r><w:t xml:space="preserve">{t}</w:t></w:r>"#)
}

#[test]
fn a_text_drag_in_a_read_only_document_edits_nothing() {
    let mut e = lay("<w:p><w:r><w:t>Hello Type</w:t></w:r></w:p><w:p><w:r><w:t>Other line</w:t></w:r></w:p>");
    let p = DocumentProtection { kind: Some(ProtectionKind::ReadOnly), enforce: Some(true), ..Default::default() };
    e.execute(Command::SetDocumentProtection(p), fonts());
    let (before, rev) = (texts(&e), e.revision());
    select(&mut e, 0, 6, 10);
    let (from, to) = (at(&mut e, 0, 7, 1.0), at(&mut e, 1, 3, 0.0));
    drag(&mut e, from, to);
    assert_eq!((texts(&e), e.revision()), (before, rev));
}

#[test]
fn a_move_into_a_locked_control_loses_nothing() {
    let mut e = lay(&format!("<w:p><w:r><w:t>Hello Type</w:t></w:r></w:p><w:p>{LOCKED}</w:p>"));
    let before = texts(&e);
    select(&mut e, 0, 6, 10);
    let from = at(&mut e, 0, 7, 1.0);
    let k = stops(&mut e, 1).0.len() / 2;
    let to = nth(&mut e, 1, k, 0.5);
    drag(&mut e, from, to);
    assert_eq!(texts(&e), before);
}

#[test]
fn a_move_out_of_a_locked_control_copies_nothing() {
    let mut e = lay(&format!("<w:p><w:r><w:t>Free text</w:t></w:r></w:p><w:p>{LOCKED}</w:p>"));
    let before = texts(&e);
    let (s, _) = stops(&mut e, 1);
    let n = s.len();
    select(&mut e, 1, s[1].1.0, s[n - 2].1.0);
    let from = nth(&mut e, 1, 2, 1.0);
    let to = at(&mut e, 0, 4, 0.0);
    drag(&mut e, from, to);
    assert_eq!(texts(&e), before);
}

#[test]
fn breaks_over_or_in_a_locked_control_are_refused() {
    let control =
        r#"<w:sdt><w:sdtPr><w:lock w:val="contentLocked"/><w:text/></w:sdtPr><w:sdtContent><w:r><w:t>cannot edit</w:t></w:r></w:sdtContent></w:sdt>"#;
    let xml = format!(r#"<w:p><w:r><w:t xml:space="preserve">Hello </w:t></w:r>{control}</w:p>"#);
    // Across its start, to its end from inside, at a caret in it
    for sel in [(3u32, 9u32), (9, 17), (9, 9)] {
        for cmd in [Command::InsertParagraph, Command::InsertColumnBreak, Command::InsertSectionBreak(SectionBreakKind::NextPage)] {
            let mut e = Editor::open(&blank_body(&xml), fonts()).expect("opens");
            let before = texts(&e);
            select(&mut e, 0, sel.0, sel.1);
            let out = e.execute(cmd.clone(), fonts());
            assert!(refused(&out) && texts(&e) == before, "{cmd:?} at {sel:?}: {out:?} {:?}", texts(&e));
        }
    }
}

#[test]
fn change_case_and_formatting_are_refused_in_a_locked_control() {
    let sdt = r#"<w:p><w:sdt><w:sdtPr><w:id w:val="5"/><w:lock w:val="sdtContentLocked"/></w:sdtPr><w:sdtContent><w:r><w:t>abc</w:t></w:r></w:sdtContent></w:sdt></w:p>"#;
    for cmd in
        [Command::ChangeCase(CaseChange::Upper), Command::SetBold(true), Command::SetFontSize(Points(20.0)), Command::SetAlignment(Alignment::Center)]
    {
        let mut e = Editor::open(&blank_body(sdt), fonts()).expect("opens");
        select(&mut e, 0, 1, 2);
        let rev = e.revision();
        let out = e.execute(cmd.clone(), fonts());
        assert!(matches!(&out, Outcome::Refused(err) if err.message == "locked"), "{cmd:?}: {out:?}");
        assert_eq!((texts(&e)[0].as_str(), e.revision()), ("abc", rev), "{cmd:?}");
    }
}

#[test]
fn replace_all_skips_content_locked_controls_and_keeps_markers() {
    let locked = format!(
        r#"<w:sdt><w:sdtPr><w:id w:val="5"/><w:lock w:val="sdtContentLocked"/><w:richText/></w:sdtPr><w:sdtContent>{}</w:sdtContent></w:sdt>"#,
        run("John")
    );
    let mut e = Editor::open(&blank_body(&format!("<w:p>{locked}</w:p>")), fonts()).expect("opens");
    e.replace_all("John", "Jim", false, fonts());
    assert_eq!(texts(&e)[0], "John", "locked content is untouched");

    let open = format!(r#"<w:sdt><w:sdtPr><w:id w:val="5"/><w:richText/></w:sdtPr><w:sdtContent>{}</w:sdtContent></w:sdt>"#, run("John"));
    let mut e = Editor::open(&blank_body(&format!("<w:p>{}{open}</w:p>", run("Dear "))), fonts()).expect("opens");
    let (n, _) = e.replace_all("r J", "r_J", false, fonts());
    let p = e.index().para(e.document(), para_id(&e, 0)).expect("the paragraph");
    let marks = |f: fn(&Inline) -> bool| p.inlines.iter().filter(|i| f(i)).count();
    let marks = (marks(|i| matches!(i, Inline::ControlStart(_))), marks(|i| matches!(i, Inline::ControlEnd(_))));
    assert_eq!((n, marks), (1, (1, 1)), "{:?}", p.inlines);
}

#[test]
fn a_symbol_leaves_no_typing_font() {
    let mut e = Editor::open(&blank_body(r#"<w:p><w:r><w:t>text</w:t></w:r></w:p>"#), fonts()).expect("opens");
    let p = para_id(&e, 0);
    e.set_caret_for_benchmark(Selection::Caret(Position::text(StoryId::Body, p, Unit(2))));
    let out = e.execute(Command::InsertSymbol(InsertSymbol { char: '\u{B7}', font: Some("Symbol".into()) }), fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    assert!(e.pending.is_none(), "the symbol's font is the typing font");
}

#[test]
fn a_relative_size_on_a_fixed_picture_control_is_refused() {
    use super::scenarios::Run;
    let mut r = Run::open("FX-PICTURE-CONTROLS.docx", "relative size");
    r.e.execute(Command::SetProtectedPictureControls(ProtectedPictureControls { tag_prefixes: vec!["Placeholder:".into()] }), fonts());
    r.step(&json!({ "click": { "float": 0 } }));
    let abs =
        r.e.execute(Command::SetObjectSize(ObjectSize { width: Some(Points(20.0)), height: Some(Points(20.0)), ..Default::default() }), fonts());
    assert!(refused(&abs), "{abs:?}");
    let rel = Some(NoneOr::Value(RelativeSize { percent: 50.0, relative_to: RelativeSizeReference::Page }));
    let out = r.e.execute(Command::SetObjectSize(ObjectSize { relative_width: rel, ..Default::default() }), fonts());
    assert!(refused(&out), "{out:?}");
}

#[test]
fn a_content_locked_checkbox_is_not_toggled() {
    let mut e = open("FX-CHECKBOXES.docx");
    let boxes: Vec<String> = e
        .document()
        .controls
        .iter()
        .filter(|(_, c)| matches!(c.props.kind, crate::docx::ControlKind::Checkbox { .. }))
        .map(|(k, _)| crate::editing::api_id(crate::docx::ElementId::Control(*k)).0)
        .collect();
    assert!(!boxes.is_empty());
    let mut seq = 0;
    let mut cmd = |e: &mut Editor, c: serde_json::Value| -> serde_json::Value {
        seq += 1;
        serde_json::from_str(&e.apply_json(&json!({ "type": "command", "seq": seq, "command": c }).to_string(), fonts())).expect("a reply")
    };
    for b in &boxes {
        let r = cmd(&mut e, json!({ "type": "setControl", "id": b, "lock": "content" }));
        assert!(r.get("error").is_none(), "{r}");
    }
    let before = texts(&e);
    for b in &boxes {
        cmd(&mut e, json!({ "type": "setCheckbox", "id": b, "checked": true }));
    }
    assert_eq!(texts(&e), before);
}

#[test]
fn a_group_keeps_its_own_content_and_lets_nested_controls_change() {
    let fill = format!(r#"<w:sdt><w:sdtPr><w:id w:val="7"/><w:richText/></w:sdtPr><w:sdtContent>{}</w:sdtContent></w:sdt>"#, run("fill"));
    let group = format!(
        r#"<w:sdt><w:sdtPr><w:id w:val="6"/><w:group/></w:sdtPr><w:sdtContent><w:p>{}{fill}{}</w:p></w:sdtContent></w:sdt>"#,
        run("Dear "),
        run(" end")
    );
    let mut e = Editor::open(&blank_body(&group), fonts()).expect("opens");
    let before = texts(&e);
    assert_eq!(before[0], "Dear fill end");
    let typing = || Command::InsertText("x".into());
    for (a, b, cmd) in [(2, 2, typing()), (1, 3, typing()), (13, 13, typing()), (4, 6, Command::Cut)] {
        select(&mut e, 0, a, b);
        let out = e.execute(cmd.clone(), fonts());
        assert!(refused(&out) && texts(&e) == before, "{cmd:?} at {a}..{b}: {out:?} {:?}", texts(&e));
    }
    // After the nested control's end the text is the group's again.
    select(&mut e, 0, 9, 9);
    let out = e.execute(Command::InsertText("x".into()), fonts());
    assert!(refused(&out) && texts(&e) == before, "{out:?} {:?}", texts(&e));
    select(&mut e, 0, 6, 6);
    let out = e.execute(Command::InsertText("x".into()), fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    assert_eq!(texts(&e)[0], "Dear fxill end");
}

#[test]
fn a_range_between_two_controls_in_a_group_is_refused() {
    let control =
        |id: u32, t: &str| format!(r#"<w:sdt><w:sdtPr><w:id w:val="{id}"/><w:richText/></w:sdtPr><w:sdtContent>{}</w:sdtContent></w:sdt>"#, run(t));
    let group = format!(
        r#"<w:sdt><w:sdtPr><w:id w:val="6"/><w:group/></w:sdtPr><w:sdtContent><w:p>{}{}{}{}{}</w:p></w:sdtContent></w:sdt><w:p>{}</w:p>"#,
        run("Dear "),
        control(7, "fill"),
        run(" and "),
        control(8, "more"),
        run(" end"),
        run("after")
    );
    let mut e = Editor::open(&blank_body(&group), fonts()).expect("opens");
    let before = texts(&e);
    assert_eq!(before[0], "Dear fill and more end");
    let delete = || Command::Delete(crate::api::command::DeleteOptions { direction: crate::api::value::Direction::Backward, unit: None });
    // From one nested control into the other, and to the group's end
    for (para, a, b) in [(0, 6, 16), (0, 6, 22)] {
        select(&mut e, para, a, b);
        let out = e.execute(delete(), fonts());
        assert!(refused(&out) && texts(&e) == before, "{a}..{b}: {out:?} {:?}", texts(&e));
    }
    select(&mut e, 0, 6, 8);
    let out = e.execute(delete(), fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    assert_eq!(texts(&e)[0], "Dear fl and more end");
}

#[test]
fn formatting_applies_over_a_control_that_cannot_be_deleted() {
    let control = r#"<w:sdt><w:sdtPr><w:id w:val="5"/><w:lock w:val="sdtLocked"/><w:richText/></w:sdtPr><w:sdtContent><w:r><w:t>kept</w:t></w:r></w:sdtContent></w:sdt>"#;
    let xml = format!(r#"<w:p>{}{control}{}</w:p>"#, run("Hello "), run(" tail"));
    for cmd in
        [Command::SetBold(true), Command::SetFontSize(Points(20.0)), Command::SetAlignment(Alignment::Center), Command::ChangeCase(CaseChange::Upper)]
    {
        let mut e = Editor::open(&blank_body(&xml), fonts()).expect("opens");
        e.execute(Command::SelectAll, fonts());
        let out = e.execute(cmd.clone(), fonts());
        assert!(matches!(out, Outcome::Changed(_)), "{cmd:?}: {out:?}");
    }
    let mut e = Editor::open(&blank_body(&xml), fonts()).expect("opens");
    e.execute(Command::SelectAll, fonts());
    let out =
        e.execute(Command::Delete(crate::api::command::DeleteOptions { direction: crate::api::value::Direction::Backward, unit: None }), fonts());
    assert!(refused(&out), "{out:?}");
}

#[test]
fn editable_ranges_follow_typing_before_them() {
    let body = format!(r#"<w:p>{}<w:permStart w:id="1" w:edGrp="everyone"/>{}<w:permEnd w:id="1"/></w:p>"#, run("out "), run("in"));
    let mut e = Editor::open(&blank_body(&body), fonts()).expect("opens");
    select(&mut e, 0, 5, 5);
    assert!(e.in_editable_range("someone"));
    select(&mut e, 0, 1, 1);
    assert!(matches!(e.execute(Command::InsertText("abc".into()), fonts()), Outcome::Changed(_)));
    // "oabcut in": the range moved three units on.
    select(&mut e, 0, 5, 5);
    assert!(!e.in_editable_range("someone"), "the range didn't move with the text before it");
    select(&mut e, 0, 8, 8);
    assert!(e.in_editable_range("someone"));
}

#[test]
fn typing_away_from_the_editable_ranges_keeps_them() {
    let body = format!(r#"<w:p>{}</w:p><w:p><w:permStart w:id="1" w:edGrp="everyone"/>{}<w:permEnd w:id="1"/></w:p>"#, run("free"), run("in"));
    let mut e = Editor::open(&blank_body(&body), fonts()).expect("opens");
    let found = e.editable_ranges();
    select(&mut e, 0, 2, 2);
    assert!(matches!(e.execute(Command::InsertText("x".into()), fonts()), Outcome::Changed(_)));
    assert!(std::rc::Rc::ptr_eq(&found, &e.editable_ranges()), "a keystroke found the ranges again");
    select(&mut e, 1, 1, 1);
    assert!(matches!(e.execute(Command::InsertText("x".into()), fonts()), Outcome::Changed(_)));
    assert!(!std::rc::Rc::ptr_eq(&found, &e.editable_ranges()), "typing in a range kept its old ends");
}

#[test]
fn editable_ranges_match_in_linear_time() {
    let editor = |n: usize| {
        let starts: String = (0..n).map(|i| format!("<w:p><w:permStart w:id=\"{i}\" w:edGrp=\"everyone\"/><w:r><w:t>x</w:t></w:r></w:p>")).collect();
        let ends: String = (0..n).map(|i| format!("<w:p><w:permEnd w:id=\"{i}\"/><w:r><w:t>y</w:t></w:r></w:p>")).collect();
        Editor::open(&blank_body(&format!("{starts}{ends}")), fonts()).expect("opens")
    };
    let (mut small, mut big) = (editor(4000), editor(16000));
    let time = |e: &mut Editor| {
        e.perm_generation += 1;
        let t = Instant::now();
        e.in_editable_range("someone");
        t.elapsed().as_secs_f64() * 1e3
    };
    // The fastest of a few finds from scratch, taken in turns; again, a few times, on a busy machine.
    let (mut a, mut b) = (f64::MAX, f64::MAX);
    for _ in 0..5 {
        for _ in 0..5 {
            a = a.min(time(&mut small));
            b = b.min(time(&mut big));
        }
        if b < a * 6.0 {
            return;
        }
    }
    panic!("{a:.2} ms for 4000 ranges, {b:.2} ms for 16000 (4x)");
}
