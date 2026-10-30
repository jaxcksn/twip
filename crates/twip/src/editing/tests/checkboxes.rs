//! Checkbox content controls: every click on the box toggles it, with the caret after it and nothing selected; a
//! locked box, design mode and read only leave it alone.

use super::super::commands::Command;
use super::super::input::PointerKind;
use super::super::keymap::Mods;
use super::super::{Editor, Mode};
use super::{fonts, open, texts};
use crate::layout::{ParaId, Unit};

fn fixture() -> Editor {
    let mut e = open("FX-CHECKBOXES.docx");
    e.complete(fonts());
    e
}

/// The paragraph whose text ends with `label`.
fn para(e: &Editor, label: &str) -> ParaId {
    let at = texts(e).iter().position(|t| t.ends_with(label)).unwrap_or_else(|| panic!("no {label}"));
    super::para_id(e, at)
}

/// The box's face: the first character of its paragraph.
fn face(e: &Editor, label: &str) -> char {
    texts(e).into_iter().find(|t| t.ends_with(label)).and_then(|t| t.chars().next()).expect("a face")
}

/// The document point in the middle of the box before `label`.
fn on_box(e: &mut Editor, label: &str) -> (f64, f64) {
    let p = para(e, label);
    let pages = e.page_geometries();
    let (pg, x, y) = pages
        .iter()
        .find_map(|(pg, g)| {
            let (_, l) = g.lines_of(p).next()?;
            let w = l.stops.windows(2).find(|w| w[0].para == p && w[1].para == p)?;
            Some((*pg, (w[0].x + w[1].x) / 2.0, (l.top + l.bottom) / 2.0))
        })
        .expect("the box is drawn");
    e.stack().to_doc(pg, x, y).expect("on the stack")
}

fn click(e: &mut Editor, at: (f64, f64), clicks: u8) {
    e.pointer_input(PointerKind::Down, at.0, at.1, clicks, Mods::default(), fonts());
    e.pointer_input(PointerKind::Up, at.0, at.1, clicks, Mods::default(), fonts());
}

/// Clicks of one multi-click: 1, then 2, ... `n`.
fn clicks(e: &mut Editor, at: (f64, f64), n: u8) {
    for k in 1..=n {
        click(e, at, k);
        assert!(e.selection().is_collapsed(), "click {k} selects nothing");
    }
}

#[test]
fn checkbox_click_toggles_without_selection() {
    let mut e = fixture();
    let at = on_box(&mut e, " Unchecked item");
    click(&mut e, at, 1);
    assert_eq!(face(&e, " Unchecked item"), '☒', "checked");
    assert!(e.selection().is_collapsed(), "no highlight");
    let (p, u) = e.selection().focus().and_then(|f| f.para_offset()).expect("a caret");
    assert_eq!(p, para(&e, " Unchecked item"));
    let start = e
        .index()
        .para(e.document(), p)
        .map(|q| (0..).map(Unit).find(|&v| super::super::stories::inline_at(q, v).is_some_and(|i| matches!(i, crate::docx::Inline::Sym { .. }))));
    assert_eq!(start.flatten().map(|f| Unit(f.0 + 1)), Some(u), "the caret is after the box");
    click(&mut e, at, 1);
    assert_eq!(face(&e, " Unchecked item"), '☐', "cleared again");
}

#[test]
fn checkbox_double_and_triple_click_toggle_each_click() {
    let mut e = fixture();
    let at = on_box(&mut e, " Checked item");
    clicks(&mut e, at, 2);
    assert_eq!(face(&e, " Checked item"), '☒', "two clicks: back where it was");
    clicks(&mut e, at, 3);
    assert_eq!(face(&e, " Checked item"), '☐', "three clicks: toggled");
}

#[test]
fn checkbox_space_after_click_toggles() {
    let mut e = fixture();
    let at = on_box(&mut e, " Unchecked item");
    click(&mut e, at, 1);
    e.execute(Command::InsertText(" ".into()), fonts());
    assert_eq!(face(&e, " Unchecked item"), '☐', "Space toggles it back");
    assert!(texts(&e).iter().any(|t| t == "☐ Unchecked item"), "no space typed");
}

#[test]
fn checkbox_drag_from_face_selects_nothing() {
    let mut e = fixture();
    let (x, y) = on_box(&mut e, " Unchecked item");
    e.pointer_input(PointerKind::Down, x, y, 1, Mods::default(), fonts());
    e.pointer_input(PointerKind::Move, x + 80.0, y, 0, Mods::default(), fonts());
    e.pointer_input(PointerKind::Up, x + 80.0, y, 0, Mods::default(), fonts());
    assert!(e.selection().is_collapsed());
}

#[test]
fn checkbox_locked_does_not_toggle() {
    let mut e = fixture();
    let at = on_box(&mut e, " Locked item");
    click(&mut e, at, 1);
    assert_eq!(face(&e, " Locked item"), '☐');
    assert!(e.selection().focus().and_then(|f| f.para_offset()).is_some_and(|(p, _)| p == para(&e, " Locked item")), "a click on text");
}

#[test]
fn checkbox_read_only_and_design_mode_do_not_toggle() {
    for mode in [Mode::ReadOnly, Mode::Design] {
        let mut e = fixture();
        e.mode = mode;
        let at = on_box(&mut e, " Unchecked item");
        click(&mut e, at, 1);
        assert_eq!(face(&e, " Unchecked item"), '☐', "{mode:?}");
    }
}

#[test]
fn checkbox_undo_reverts_one_toggle() {
    let mut e = fixture();
    let at = on_box(&mut e, " Unchecked item");
    click(&mut e, at, 1);
    click(&mut e, at, 1);
    click(&mut e, at, 1);
    assert_eq!(face(&e, " Unchecked item"), '☒');
    e.execute(Command::Undo, fonts());
    assert_eq!(face(&e, " Unchecked item"), '☐', "one undo, one toggle");
}

#[test]
fn checkbox_in_table_cell_and_custom_faces_toggle() {
    let mut e = fixture();
    let at = on_box(&mut e, " cell box");
    click(&mut e, at, 1);
    assert_eq!(face(&e, " cell box"), '☒');
    let at = on_box(&mut e, " Custom faces");
    click(&mut e, at, 1);
    assert_eq!(face(&e, " Custom faces"), '✔');
}
