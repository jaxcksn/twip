//! Pages holding several sections (continuous breaks between one, two and three columns): clicks, drags and caret
//! movement reach and cross every section.

use super::super::Editor;
use super::super::commands::{Dir, MoveBy};
use super::super::input::PointerKind;
use super::super::keymap::Mods;
use super::super::position::{Position, StoryId};
use super::super::selection::Selection;
use super::super::text::UnitText;
use super::{fonts, mv, open};
use crate::layout::{ParaId, Unit};

fn fixture() -> Editor {
    let mut e = open("FX-SECTIONS.docx");
    e.complete(fonts());
    e
}

fn text_of(e: &Editor, p: ParaId) -> String {
    e.index().para(e.document(), p).map(|q| UnitText::of(q).text).unwrap_or_default()
}

/// The caret's (focus's) paragraph text.
fn caret_text(e: &Editor) -> String {
    e.selection().focus().and_then(|p| p.para_offset()).map(|(p, _)| text_of(e, p)).unwrap_or_default()
}

/// The section number a paragraph's text names (`S3 P1 ...` is 3).
fn section(text: &str) -> Option<u32> {
    text.strip_prefix('S')?.split(' ').next()?.parse().ok()
}

/// The body paragraph whose text starts with `prefix`.
fn para(e: &Editor, prefix: &str) -> ParaId {
    super::texts_ids(e).into_iter().find(|&p| text_of(e, p).starts_with(prefix)).unwrap_or_else(|| panic!("no paragraph {prefix}"))
}

/// Every body line on the first page: its paragraph and its box (page points).
fn body_lines(e: &mut Editor) -> Vec<(ParaId, f64, f64, f64, f64)> {
    let (_, g) = e.page_geometries().into_iter().next().expect("a page");
    g.lines
        .iter()
        .filter(|l| !l.repeat && g.stories[l.story].story == StoryId::Body)
        .filter_map(|l| Some((l.stops[0].para, l.stops.first()?.x, l.stops.last()?.x, l.top, l.bottom)))
        .collect()
}

fn click(e: &mut Editor, x: f64, y: f64) {
    let (dx, dy) = e.stack().to_doc(0, x, y).expect("on the stack");
    for kind in [PointerKind::Down, PointerKind::Up] {
        e.pointer_input(kind, dx, dy, 1, Mods::default(), fonts());
    }
}

#[test]
fn sections_fixture_has_four_sections_on_one_page() {
    let mut e = fixture();
    let seen: std::collections::BTreeSet<u32> = body_lines(&mut e).iter().filter_map(|l| section(&text_of(&e, l.0))).collect();
    assert_eq!(seen.into_iter().collect::<Vec<_>>(), vec![1, 2, 3, 4], "every section's lines are on page 1");
}

#[test]
fn sections_click_reaches_each_section() {
    let mut e = fixture();
    for (p, x0, x1, top, bottom) in body_lines(&mut e) {
        click(&mut e, (x0 + x1) / 2.0, (top + bottom) / 2.0);
        let got = e.selection().focus().and_then(|q| q.para_offset()).map(|(q, _)| q);
        assert_eq!(got, Some(p), "a click on a line of {:?} landed in {:?}", text_of(&e, p), caret_text(&e));
    }
}

#[test]
fn sections_click_blank_space_picks_nearest_line() {
    let mut e = fixture();
    let lines = body_lines(&mut e);
    let right = lines.iter().map(|l| l.2).fold(f64::MIN, f64::max);
    for n in 1..=4 {
        let own: Vec<_> = lines.iter().filter(|l| section(&text_of(&e, l.0)) == Some(n)).collect();
        let (top, bottom) = own.iter().fold((f64::MAX, f64::MIN), |(t, b), l| (t.min(l.3), b.max(l.4)));
        // Right of the text (beside a short column, or past a short line), halfway down the section's lines.
        click(&mut e, right + 10.0, (top + bottom) / 2.0);
        assert_eq!(section(&caret_text(&e)), Some(n), "beside section {n}");
    }
    // Below the last line on the page: the last section.
    let page_h = e.page_geometries()[0].1.h;
    click(&mut e, lines[0].1 + 20.0, page_h - 10.0);
    assert_eq!(section(&caret_text(&e)), Some(4), "below the last section");
}

/// The sections the caret visits pressing `dir` by `by` until it stops moving (at most 300 presses).
fn walk(e: &mut Editor, by: MoveBy, dir: Dir) -> Vec<u32> {
    let mut seen = vec![section(&caret_text(e)).expect("in a section")];
    for _ in 0..300 {
        let before = e.selection();
        e.execute(mv(by, dir, false), fonts());
        if e.selection() == before {
            break;
        }
        if let Some(s) = section(&caret_text(e))
            && seen.last() != Some(&s)
        {
            seen.push(s);
        }
    }
    seen
}

#[test]
fn sections_down_up_cross_boundaries() {
    let mut e = fixture();
    let first = para(&e, "S1 P1");
    e.set_selection(Selection::Caret(Position::text(StoryId::Body, first, Unit(0))));
    assert_eq!(walk(&mut e, MoveBy::Line, Dir::Forward), vec![1, 2, 3, 4], "Down visits every section in order");
    assert_eq!(walk(&mut e, MoveBy::Line, Dir::Back), vec![4, 3, 2, 1], "Up returns through every section");
}

#[test]
fn sections_left_right_cross_boundaries() {
    let mut e = fixture();
    let last = para(&e, "S1 P4");
    let first = para(&e, "S2 P1");
    let end = e.index().para(e.document(), last).map(|q| UnitText::of(q).len()).expect("S1 P4");
    e.set_selection(Selection::Caret(Position::text(StoryId::Body, last, end)));
    e.execute(mv(MoveBy::Char, Dir::Forward, false), fonts());
    assert_eq!(e.selection().focus().and_then(|p| p.para_offset()), Some((first, Unit(0))), "Right steps into section 2");
    e.execute(mv(MoveBy::Char, Dir::Back, false), fonts());
    assert_eq!(e.selection().focus().and_then(|p| p.para_offset()), Some((last, end)), "Left steps back");
    assert_eq!(walk(&mut e, MoveBy::Para, Dir::Forward), vec![1, 2, 3, 4], "Ctrl+Down crosses every section");
}

#[test]
fn sections_shift_down_extends_across() {
    let mut e = fixture();
    let last = para(&e, "S1 P4");
    e.set_selection(Selection::Caret(Position::text(StoryId::Body, last, Unit(0))));
    e.execute(mv(MoveBy::Line, Dir::Forward, true), fonts());
    let s = e.selection();
    assert!(!s.is_collapsed(), "a range");
    assert_eq!(section(&caret_text(&e)), Some(2), "the range's focus is in section 2");
}

#[test]
fn sections_drag_across() {
    let mut e = fixture();
    let lines = body_lines(&mut e);
    let in_section = |n: u32| *lines.iter().find(|l| section(&text_of(&e, l.0)) == Some(n)).expect("a line");
    let (a, b) = (in_section(1), in_section(3));
    let stack = e.stack();
    let (ax, ay) = stack.to_doc(0, a.1 + 2.0, (a.3 + a.4) / 2.0).expect("on the stack");
    let (bx, by) = stack.to_doc(0, b.1 + 2.0, (b.3 + b.4) / 2.0).expect("on the stack");
    e.pointer_input(PointerKind::Down, ax, ay, 1, Mods::default(), fonts());
    e.pointer_input(PointerKind::Move, bx, by, 0, Mods::default(), fonts());
    e.pointer_input(PointerKind::Up, bx, by, 0, Mods::default(), fonts());
    let s = e.selection();
    let anchor = s.anchor().and_then(|p| p.para_offset()).map(|(p, _)| text_of(&e, p)).unwrap_or_default();
    assert_eq!((section(&anchor), section(&caret_text(&e))), (Some(1), Some(3)), "the drag spans sections 1 to 3");
}
