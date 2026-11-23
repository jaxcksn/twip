//! Caret movement

use super::super::Editor;
use super::super::commands::{Command, Dir, MoveBy};
use super::super::position::{Affinity, Place, Position, StoryId};
use super::super::selection::{ObjectRef, Selection};
use super::{blank_body, blank_with, fonts, mv, open, para_id, texts, texts_ids};
use crate::api::command::{SelectUnit, SelectionMode};
use crate::layout::Unit;

fn at_start(text: &str) -> Editor {
    let mut e = Editor::open(&blank_with(text), fonts()).expect("opens");
    let para = para_id(&e, 0);
    e.set_caret_for_benchmark(Selection::Caret(Position::text(StoryId::Body, para, Unit(0))));
    e
}

fn top(e: &mut Editor) -> (f64, Affinity) {
    let c = e.caret_view().expect("caret").top;
    (c, e.selection().focus().expect("focus").affinity())
}

fn offset(e: &Editor) -> u32 {
    let Some(Position { place: Place::Text { offset, .. }, .. }) = e.selection().focus() else { panic!("a text caret: {:?}", e.selection()) };
    offset.0
}

#[test]
fn up_onto_a_shorter_wrapped_line_moves_one_line() {
    // Line 1 is short (the long word does not fit after it); line 2 is full; there is a third line.
    let mut e = at_start(&format!("{}{} {}", "x ".repeat(20), "L".repeat(60), "y ".repeat(200)));
    let (top1, _) = top(&mut e);
    e.execute(mv(MoveBy::Line, Dir::Forward, false), fonts());
    let (top2, _) = top(&mut e);
    assert!(top2 > top1, "line 2 is below line 1");
    e.execute(mv(MoveBy::LineEdge, Dir::Forward, false), fonts());
    assert_eq!(top(&mut e).0, top2);
    e.execute(mv(MoveBy::Line, Dir::Back, false), fonts());
    let (t, aff) = top(&mut e);
    assert_eq!(t, top1, "Up lands on line 1");
    assert_eq!(aff, Affinity::Upstream);
    e.execute(mv(MoveBy::Line, Dir::Forward, false), fonts());
    assert_eq!(top(&mut e).0, top2, "Down goes back to line 2");
}

/// The (top, end x) of the first three lines, found by stepping the caret one character at a time.
fn first_lines(e: &mut Editor) -> Vec<(f64, f64)> {
    let mut lines: Vec<(f64, f64)> = Vec::new();
    for _ in 0..800 {
        let c = e.caret_view().expect("caret");
        if let Some(l) = lines.last_mut().filter(|l| l.0 == c.top) {
            l.1 = c.x;
        } else if lines.len() == 3 {
            break;
        } else {
            lines.push((c.top, c.x));
        }
        e.execute(mv(MoveBy::Char, Dir::Forward, false), fonts());
    }
    lines
}

#[test]
fn down_onto_a_shorter_wrapped_line_does_not_skip_it() {
    // A text whose line 1 is full and line 2 ends well short of it, with a line 3 after.
    let text = |n: usize| format!("{}{} {}", "x ".repeat(n), "L".repeat(60), "y ".repeat(200));
    let n = (40..200)
        .find(|&n| {
            let l = first_lines(&mut at_start(&text(n)));
            l.len() == 3 && l[1].1 < l[0].1 - 20.0
        })
        .expect("a text with a short wrapped line 2");
    let l = first_lines(&mut at_start(&text(n)));
    let mut e = at_start(&text(n));
    e.execute(mv(MoveBy::LineEdge, Dir::Forward, false), fonts());
    e.execute(mv(MoveBy::Line, Dir::Forward, false), fonts());
    assert_eq!(top(&mut e), (l[1].0, Affinity::Upstream), "Down lands at the end of line 2, not on line 3");
}

#[test]
fn an_arrow_with_a_cell_range_selected_collapses_to_a_caret() {
    let mut e = open("FX-TBL-BASIC.docx");
    let p = texts(&e).iter().position(|t| t == "T2 R3 C3").expect("cell");
    let para = para_id(&e, p);
    e.set_caret_for_benchmark(Selection::Caret(Position::text(StoryId::Body, para, Unit(2))));
    e.execute(Command::Select(SelectUnit::Row), fonts());
    assert!(matches!(e.selection(), Selection::Cells { .. }));
    e.execute(mv(MoveBy::Char, Dir::Forward, false), fonts());
    assert!(matches!(e.selection(), Selection::Caret(_)), "an arrow leaves the cell range: {:?}", e.selection());
    e.execute(Command::Select(SelectUnit::Row), fonts());
    e.execute(mv(MoveBy::LineEdge, Dir::Back, false), fonts());
    assert!(matches!(e.selection(), Selection::Caret(_)), "so does Home: {:?}", e.selection());
}

#[test]
fn an_arrow_with_an_inline_picture_selected_steps_off_it() {
    let data = open("FX-OBJ.docx").document().images.last().cloned().expect("an image");
    let mut e = Editor::open(&blank_body(r#"<w:p><w:r><w:t>text</w:t></w:r></w:p>"#), fonts()).expect("opens");
    let p = para_id(&e, 0);
    e.set_caret_for_benchmark(Selection::Caret(Position::text(StoryId::Body, p, Unit(2))));
    let picture = || Command::InsertPicture(crate::api::command::Picture { data: crate::api::value::Bytes(data.clone()), alt_text: None });
    e.execute(picture(), fonts());
    assert!(matches!(e.selection(), Selection::Object(ObjectRef::Inline { offset: Unit(2), .. })), "{:?}", e.selection());
    e.execute(mv(MoveBy::Char, Dir::Forward, false), fonts());
    assert_eq!(offset(&e), 3, "Right: after the picture");
    e.set_selection(Selection::Object(ObjectRef::Inline { para: p, offset: Unit(2) }));
    e.execute(mv(MoveBy::Char, Dir::Back, false), fonts());
    assert_eq!(offset(&e), 2, "Left: before it");
}

#[test]
fn page_down_with_nowhere_to_go_keeps_an_object_selected() {
    let data = open("FX-OBJ.docx").document().images.last().cloned().expect("an image");
    let sec = r#"<w:sectPr><w:headerReference w:type="default" r:id="rIdH"/><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr>"#;
    let mut e = super::stories::package(&format!("<w:p><w:r><w:t>body</w:t></w:r></w:p>{sec}"), Some(Some("<w:p><w:r><w:t>head</w:t></w:r></w:p>")));
    e.edit_header_footer(Some(0), false, fonts());
    let picture = Command::InsertPicture(crate::api::command::Picture { data: crate::api::value::Bytes(data), alt_text: None });
    e.execute(picture, fonts());
    let object = e.selection();
    assert!(matches!(object, Selection::Object(_)), "{object:?}");
    let out = e.execute(mv(MoveBy::Page, Dir::Forward, false), fonts());
    assert!(matches!(out, super::super::commands::Outcome::Unchanged(_)), "the only page: {out:?}");
    assert_eq!(e.selection(), object, "the picture stays selected");
}

#[test]
fn movement_skips_text_the_markup_view_hides() {
    let body = r#"<w:p><w:r><w:t xml:space="preserve">a </w:t></w:r><w:del w:id="1" w:author="A" w:date="2024-01-01T00:00:00Z"><w:r><w:delText xml:space="preserve">deleted </w:delText></w:r></w:del><w:r><w:t>b</w:t></w:r></w:p>"#;
    let mut e = Editor::open(&blank_body(body), fonts()).expect("opens");
    e.execute(Command::SetMarkupView(crate::docx::review::MarkupView::NoMarkup), fonts());
    let para = para_id(&e, 0);
    let unit = |p: Position| match p.place {
        Place::Text { offset, .. } => offset.0,
        Place::RowEnd { .. } => panic!("text"),
    };
    assert_eq!(unit(e.nav().right(Position::text(StoryId::Body, para, Unit(2)), false)), 11, "Right steps over the hidden run and 'b'");
    assert_eq!(unit(e.nav().right(Position::text(StoryId::Body, para, Unit(0)), true)), 10, "Ctrl+Right goes to 'b'");
    assert_eq!(unit(e.nav().left(Position::text(StoryId::Body, para, Unit(10)), false)), 1, "Left steps back over the hidden run");
}

#[test]
fn extend_to_a_character_in_a_long_paragraph_is_linear() {
    let mut e = at_start(&"a".repeat(20_000));
    e.execute(Command::SetSelectionMode(SelectionMode::Extend), fonts());
    let t = std::time::Instant::now();
    e.execute(Command::InsertText("#".into()), fonts());
    assert!(t.elapsed().as_secs() < 2, "F8 and a character took {:?}", t.elapsed());
    let mut e = at_start("abc def");
    e.execute(Command::SetSelectionMode(SelectionMode::Extend), fonts());
    e.execute(Command::InsertText("e".into()), fonts());
    let Some((_, b)) = e.selection().ordered(e.index()) else { panic!("a range") };
    assert_eq!(b.para_offset().map(|(_, u)| u.0), Some(6), "the selection takes in the character");
}

#[test]
fn right_and_left_cross_a_table_with_an_empty_cell() {
    let tbl = r#"<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w="4000"/></w:tblGrid><w:tr><w:tc><w:tcPr/></w:tc></w:tr></w:tbl>"#;
    let mut e =
        Editor::open(&blank_body(&format!(r#"<w:p><w:r><w:t>a</w:t></w:r></w:p>{tbl}<w:p><w:r><w:t>b</w:t></w:r></w:p>"#)), fonts()).expect("opens");
    let ids = texts_ids(&e);
    let (a, b) = (ids[0], *ids.last().unwrap());
    let from = Position::text(StoryId::Body, a, Unit(1));
    let mut p = e.nav().right(from, false);
    assert!(!p.same_place(&from), "Right moves past the paragraph before the table");
    for _ in 0..4 {
        p = e.nav().right(p, false);
    }
    assert_eq!(p.para_offset().map(|(q, _)| q), Some(b), "Right reaches the paragraph after it");
    let back = Position::text(StoryId::Body, b, Unit(0));
    let mut p = e.nav().left(back, false);
    assert!(!p.same_place(&back), "Left moves from the paragraph after the table");
    for _ in 0..4 {
        p = e.nav().left(p, false);
    }
    assert_eq!(p.para_offset().map(|(q, _)| q), Some(a), "Left reaches the paragraph before it");
    // A story that starts with such a table still has a start.
    let mut e = Editor::open(&blank_body(&format!(r#"{tbl}<w:p><w:r><w:t>b</w:t></w:r></w:p>"#)), fonts()).expect("opens");
    assert!(e.nav().story_edge(StoryId::Body, false).is_some());
}

#[test]
fn normal_mode_collapses_a_block_selection_like_escape() {
    let mut e = open("FX-TEXT.docx");
    e.execute(Command::SetSelectionMode(SelectionMode::Block), fonts());
    e.execute(mv(MoveBy::Line, Dir::Forward, true), fonts());
    e.execute(mv(MoveBy::Char, Dir::Forward, true), fonts());
    assert!(matches!(e.selection(), Selection::Block { .. }), "{:?}", e.selection());
    e.execute(Command::SetSelectionMode(SelectionMode::Normal), fonts());
    assert!(matches!(e.selection(), Selection::Caret(_)), "{:?}", e.selection());
}
