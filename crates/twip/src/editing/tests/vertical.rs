//! Vertical text

use std::rc::Rc;

use super::super::Editor;
use super::super::geometry::{GLine, LineDir, PageGeometry};
use super::super::input::PointerKind;
use super::super::keymap::Mods;
use super::super::position::{Position, StoryId};
use super::super::selection::Selection;
use super::super::view::SelectionView;
use super::fonts;

fn layout_fixture(name: &str) -> Editor {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/layout").join(name);
    let bytes = std::fs::read(&path).expect("the fixture");
    let mut e = Editor::open(&bytes, fonts()).expect("opens");
    e.complete(fonts());
    e
}

fn pages(e: &mut Editor) -> Vec<(usize, Rc<PageGeometry>)> {
    e.page_geometries()
}

/// The first line on page `pg` (of a story `story` picks) whose text starts with `text`, and its index.
fn line_of(e: &mut Editor, pg: usize, text: &str, story: impl Fn(StoryId) -> bool) -> (Rc<PageGeometry>, usize) {
    let g = pages(e).into_iter().find(|(p, _)| *p == pg).map(|(_, g)| g).expect("the page");
    let i = (0..g.lines.len())
        .find(|&i| {
            let l = &g.lines[i];
            story(g.stories[l.story].story) && l.para().is_some_and(|p| starts(e, p, l, text))
        })
        .unwrap_or_else(|| panic!("a line starting {text:?} on page {pg}"));
    (g, i)
}

fn starts(e: &Editor, para: crate::layout::ParaId, l: &GLine, text: &str) -> bool {
    let Some(p) = e.index.para(&e.doc, para) else { return false };
    let t = super::super::text::UnitText::of(p);
    l.first_of(para).is_some_and(|u| t.text.chars().skip(u.0 as usize).collect::<String>().starts_with(text))
}

fn press(e: &mut Editor, pg: usize, p: [f64; 2], mods: Mods) {
    let (x, y) = e.stack().to_doc(pg, p[0], p[1]).expect("on the stack");
    e.pointer_input(PointerKind::Down, x, y, 1, mods, fonts());
    e.pointer_input(PointerKind::Up, x, y, 0, mods, fonts());
}

fn key(e: &mut Editor, code: &str, mods: Mods) {
    let c = e.key_command(code, mods).unwrap_or_else(|| panic!("{code} is bound"));
    e.execute(c, fonts());
}

fn focus(e: &Editor) -> (crate::layout::ParaId, u32) {
    let (p, u) = e.selection().focus().and_then(|f| f.para_offset()).expect("a caret in text");
    (p, u.0)
}

/// A stop in the middle of line `i`, and the page point at it in the middle of the line's band.
fn middle(g: &PageGeometry, i: usize) -> (super::super::geometry::Stop, [f64; 2]) {
    let l = &g.lines[i];
    let s = l.stops[l.stops.len() / 2];
    (s, l.page(s.x, (l.top + l.bottom) / 2.0))
}

/// Clicks the middle of line `i` and checks the caret lands there, drawn across the line.
fn click_lands(e: &mut Editor, pg: usize, g: &PageGeometry, i: usize, dir: LineDir) {
    let l = &g.lines[i];
    assert_eq!(l.dir(), dir, "the line runs {dir:?}");
    let (s, p) = middle(g, i);
    press(e, pg, p, Mods::default());
    assert_eq!(focus(e), (s.para, s.unit.0), "the click lands on the stop under it");
    let c = e.caret_view().expect("a caret");
    let band = l.band(l.x0(), l.x1());
    match dir {
        LineDir::Horizontal => assert!((c.right - c.x).abs() < 1e-6 && c.bottom > c.top, "a vertical bar: {c:?}"),
        _ => {
            assert!((c.bottom - c.top).abs() < 1e-6 && c.right > c.x + 1.0, "a horizontal bar: {c:?}");
            assert!((c.x - band[0]).abs() < 1e-6 && (c.right - band[2]).abs() < 1e-6, "across the whole band: {c:?} {band:?}");
            assert!((c.top - p[1]).abs() < 1e-6, "at the stop: {c:?} {p:?}");
        }
    }
}

fn is_body(s: StoryId) -> bool {
    s == StoryId::Body
}

#[test]
fn vertical_sections_hit_test_and_draw_the_caret_across_their_lines() {
    let mut e = layout_fixture("vertical-directions.docx");
    for (pg, text, dir) in [
        (0, "Section 1", LineDir::Down),
        (1, "Section 2", LineDir::Down),
        (2, "Section 3", LineDir::Horizontal),
        (3, "Section 4", LineDir::Down),
        (4, "Section 5", LineDir::Down),
    ] {
        let (g, i) = line_of(&mut e, pg, text, is_body);
        click_lands(&mut e, pg, &g, i, dir);
    }
}

#[test]
fn a_tbrl_section_stacks_its_lines_right_to_left_and_tblrv_left_to_right() {
    let mut e = layout_fixture("vertical-directions.docx");
    for (pg, text, leftwards) in [(0, "Section 1", true), (4, "Section 5", false)] {
        let (g, i) = line_of(&mut e, pg, text, is_body);
        let (a, b) = (&g.lines[i], &g.lines[i + 1]);
        let (ra, rb) = (a.band(a.x0(), a.x1()), b.band(b.x0(), b.x1()));
        let (next, side) = if leftwards { (rb[0] < ra[0], "left") } else { (rb[0] > ra[0], "right") };
        assert!(next, "page {pg}: the next line lies {side} of the one before: {ra:?} {rb:?}");
        assert!(ra[3] - ra[1] > ra[2] - ra[0], "a line is a tall strip");
        assert!((ra[1] - rb[1]).abs() < 1.0, "lines start at the same top");
    }
}

#[test]
fn vertical_cells_and_text_boxes_are_hit_and_draw_turned_carets() {
    let mut e = layout_fixture("vertical-model-c15.docx");
    let pg = 0;
    let g = pages(&mut e).into_iter().find(|(p, _)| *p == pg).map(|(_, g)| g).expect("page");
    let turned: Vec<usize> = (0..g.lines.len()).filter(|&i| !g.lines[i].chain.is_empty() && g.lines[i].dir() != LineDir::Horizontal).collect();
    let dirs: Vec<LineDir> = turned.iter().map(|&i| g.lines[i].dir()).collect();
    assert!(dirs.contains(&LineDir::Down) && dirs.contains(&LineDir::Up), "cell lines run down and up: {dirs:?}");
    let first = |d: LineDir| *turned.iter().find(|&&i| g.lines[i].dir() == d).expect("a cell line");
    click_lands(&mut e, pg, &g, first(LineDir::Down), LineDir::Down);
    click_lands(&mut e, pg, &g, first(LineDir::Up), LineDir::Up);
    let (g, i) = line_of(&mut e, pg, "A vert text box", |s| matches!(s, StoryId::TextBox(_)));
    click_lands(&mut e, pg, &g, i, LineDir::Down);
    let (g, i) = line_of(&mut e, pg, "A vert270 text box", |s| matches!(s, StoryId::TextBox(_)));
    click_lands(&mut e, pg, &g, i, LineDir::Up);
}

/// Selects line `i` from its second stop to its last but one and checks the highlight is the strip across its band
/// between them.
fn selection_is_a_strip(e: &mut Editor, pg: usize, g: &PageGeometry, i: usize, what: &str) {
    let l = &g.lines[i];
    assert!(l.stops.len() > 3, "{what}: enough stops");
    let para = l.para().expect("a paragraph");
    let story = g.stories[l.story].story;
    let (sa, sb) = (l.stops[1], l.stops[l.stops.len() - 2]);
    e.set_selection(Selection::range(
        Position::text(story, para, sa.unit),
        Position::text(story, para, sb.unit),
        crate::editing::selection::Granularity::Char,
    ));
    let SelectionView::Text(bands) = e.view(pg..pg + 1).selection else { panic!("{what}: a text selection") };
    let [band] = &bands[..] else { panic!("{what}: one strip: {bands:?}") };
    let corners = [l.page(sa.x, l.top), l.page(sa.x, l.bottom), l.page(sb.x, l.top), l.page(sb.x, l.bottom)];
    let want = [
        corners.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min),
        corners.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min),
        corners.iter().map(|p| p[0]).fold(f64::NEG_INFINITY, f64::max),
        corners.iter().map(|p| p[1]).fold(f64::NEG_INFINITY, f64::max),
    ];
    let got = band.1.page();
    assert!(got.iter().zip(want).all(|(a, b)| (a - b).abs() < 1e-6), "{what}: the strip spans the band from stop to stop: {got:?} {want:?}");
    let (start, end) = (l.page(sa.x, (l.top + l.bottom) / 2.0), l.page(sb.x, (l.top + l.bottom) / 2.0));
    assert!(band.1.contains(start[0], start[1]) && band.1.contains(end[0], end[1]), "{what}: it holds both ends");
    let past = l.page(sa.x - 1.0, (l.top + l.bottom) / 2.0);
    assert!(!band.1.contains(past[0], past[1]), "{what}: and stops at the first");
}

#[test]
fn selections_in_vertical_lines_are_strips_along_them() {
    let mut e = layout_fixture("vertical-directions.docx");
    for (pg, text) in [(0, "Section 1"), (1, "Section 2"), (2, "Section 3"), (3, "Section 4"), (4, "Section 5")] {
        let (g, i) = line_of(&mut e, pg, text, is_body);
        selection_is_a_strip(&mut e, pg, &g, i, text);
        selection_is_a_strip(&mut e, pg, &g, i + 1, &format!("{text}, next line"));
    }
    let mut e = layout_fixture("vertical-model-c15.docx");
    let g = pages(&mut e).into_iter().find(|(p, _)| *p == 0).map(|(_, g)| g).expect("page");
    for dir in [LineDir::Down, LineDir::Up] {
        let i =
            (0..g.lines.len()).find(|&i| !g.lines[i].chain.is_empty() && g.lines[i].dir() == dir && g.lines[i].stops.len() > 3).expect("a cell line");
        selection_is_a_strip(&mut e, 0, &g, i, &format!("{dir:?} cell"));
    }
    for text in ["A vert text box", "A vert270 text box"] {
        let (g, i) = line_of(&mut e, 0, text, |s| matches!(s, StoryId::TextBox(_)));
        selection_is_a_strip(&mut e, 0, &g, i, text);
    }
}

#[test]
fn arrows_follow_a_tbrl_line() {
    let mut e = layout_fixture("vertical-directions.docx");
    let (g, i) = line_of(&mut e, 0, "Section 1", is_body);
    let (s, p) = middle(&g, i);
    press(&mut e, 0, p, Mods::default());
    key(&mut e, "ArrowDown", Mods::default());
    assert_eq!(focus(&e), (s.para, s.unit.0 + 1), "Down moves to the next character");
    key(&mut e, "ArrowUp", Mods::default());
    assert_eq!(focus(&e), (s.para, s.unit.0), "Up moves back");
    key(&mut e, "ArrowLeft", Mods::default());
    let next = &g.lines[i + 1];
    let (para, u) = focus(&e);
    assert!(para == s.para && next.first_of(para).is_some_and(|f| u >= f.0), "Left moves to the next line, which lies to the left");
    let c = e.caret_view().expect("a caret");
    assert!((c.top - p[1]).abs() < 8.0, "it keeps its place down the line: {} {}", c.top, p[1]);
    key(&mut e, "ArrowRight", Mods::default());
    assert_eq!(focus(&e), (s.para, s.unit.0), "Right moves back to the line before, the same place along it");
    key(&mut e, "End", Mods::default());
    assert_eq!(focus(&e).1, g.lines[i].stops.last().expect("a stop").unit.0, "End goes to the line's end, at its bottom");
    key(&mut e, "ArrowDown", Mods { shift: true, ..Mods::default() });
    assert!(!e.selection().is_collapsed(), "Shift+Down extends along the flow");
}

#[test]
fn home_ctrl_arrows_and_shift_follow_a_tbrl_line() {
    let mut e = layout_fixture("vertical-directions.docx");
    let (g, i) = line_of(&mut e, 0, "Section 1", is_body);
    let l = &g.lines[i];
    let (s, p) = middle(&g, i);
    let ctrl = Mods { ctrl: true, ..Mods::default() };
    let shift = Mods { shift: true, ..Mods::default() };
    press(&mut e, 0, p, Mods::default());
    key(&mut e, "Home", Mods::default());
    assert_eq!(focus(&e), (s.para, l.stops[0].unit.0), "Home goes to the line's start, at its top");
    // "Section 1, textDirection tbRl.": word stops at 8, 9 (the comma) and 11.
    key(&mut e, "ArrowDown", ctrl);
    assert_eq!(focus(&e).1, 8, "Ctrl+Down moves a word forward along the line");
    key(&mut e, "ArrowDown", ctrl);
    assert_eq!(focus(&e).1, 9, "and on");
    key(&mut e, "ArrowUp", ctrl);
    assert_eq!(focus(&e).1, 8, "Ctrl+Up moves a word back");
    key(&mut e, "ArrowLeft", shift);
    let (a, b) = e.selection().ordered(&e.index).expect("a range");
    assert_eq!(a.para_offset(), Some((s.para, crate::layout::Unit(8))), "Shift+Left extends from where it was");
    let next = &g.lines[i + 1];
    assert!(b.para_offset().is_some_and(|(q, u)| q == s.para && next.first_of(q).is_some_and(|f| u >= f)), "to the next line, on the left");
}

#[test]
fn undo_after_typing_in_a_vertical_line_puts_the_caret_back_across_it() {
    let mut e = layout_fixture("vertical-directions.docx");
    let (g, i) = line_of(&mut e, 0, "Section 1", is_body);
    let (s, p) = middle(&g, i);
    press(&mut e, 0, p, Mods::default());
    let before = e.caret_view().expect("a caret");
    e.execute(crate::api::command::Command::InsertText("x".into()), fonts());
    e.complete(fonts());
    assert_eq!(focus(&e), (s.para, s.unit.0 + 1));
    e.execute(crate::api::command::Command::Undo, fonts());
    e.complete(fonts());
    assert_eq!(focus(&e), (s.para, s.unit.0), "the caret goes back");
    let c = e.caret_view().expect("a caret");
    assert!(c.vertical() && (c.top - before.top).abs() < 1e-6 && (c.x - before.x).abs() < 1e-6, "a bar across the line, where it was: {c:?}");
}

#[test]
fn alt_shift_arrows_move_rows_as_pressed_in_a_vertical_cell() {
    let mut e = editor(&format!("{}<w:p/>", table_xml(2, 2, "<w:textDirection w:val=\"tbRl\"/>", "")));
    let g = pages(&mut e).into_iter().next().map(|(_, g)| g).expect("a page");
    let i = g.lines.iter().position(|l| !l.chain.is_empty() && l.dir() == LineDir::Down).expect("the vertical cell's line");
    let (_, p) = middle(&g, i);
    press(&mut e, 0, p, Mods::default());
    assert!(e.caret_arrows().is_some(), "the caret's line runs down");
    let first_row = |e: &Editor| {
        let crate::docx::Block::Table(t) = &e.document().sections[0].blocks[0] else { panic!("a table") };
        let crate::docx::Block::Para(p) = &t.rows[0].cells[0].blocks[0] else { panic!("a paragraph") };
        p.inlines.iter().filter_map(|i| if let crate::docx::Inline::Text(s, _) = i { Some(s.clone()) } else { None }).collect::<String>()
    };
    key(&mut e, "ArrowDown", Mods { alt: true, shift: true, ..Mods::default() });
    assert_eq!(first_row(&e), "r1c0", "Alt+Shift+Down moves the row down the table");
}

#[test]
fn arrows_follow_btlr_and_tblrv_cells() {
    let mut e = layout_fixture("vertical-model-c15.docx");
    let g = pages(&mut e).into_iter().find(|(p, _)| *p == 0).map(|(_, g)| g).expect("page");
    let up = (0..g.lines.len()).find(|&i| !g.lines[i].chain.is_empty() && g.lines[i].dir() == LineDir::Up).expect("a btLr cell line");
    let (s, p) = middle(&g, up);
    press(&mut e, 0, p, Mods::default());
    key(&mut e, "ArrowUp", Mods::default());
    assert_eq!(focus(&e), (s.para, s.unit.0 + 1), "in btLr, Up moves forward");
    key(&mut e, "ArrowDown", Mods::default());
    assert_eq!(focus(&e), (s.para, s.unit.0), "and Down back");
    let arrows = e.caret_arrows().expect("turned arrows");
    assert_eq!((arrows.along, arrows.stack), ([0, -1], [1, 0]), "btLr lines stack left to right");
}

#[test]
fn a_tblrv_section_moves_to_the_next_line_with_right() {
    let mut e = layout_fixture("vertical-directions.docx");
    let (g, i) = line_of(&mut e, 4, "Section 5", is_body);
    let (s, p) = middle(&g, i);
    press(&mut e, 4, p, Mods::default());
    assert_eq!(e.caret_arrows().map(|a| (a.along, a.stack)), Some(([0, 1], [1, 0])));
    key(&mut e, "ArrowRight", Mods::default());
    let (para, u) = focus(&e);
    assert!(para == s.para && g.lines[i + 1].first_of(para).is_some_and(|f| u >= f.0), "Right moves to the next line");
    key(&mut e, "ArrowLeft", Mods::default());
    assert_eq!(focus(&e), (s.para, s.unit.0), "Left moves back");
}

#[test]
fn horizontal_text_keeps_its_arrows() {
    let mut e = layout_fixture("vertical-directions.docx");
    let (g, i) = line_of(&mut e, 2, "Section 3", is_body);
    let (_, p) = middle(&g, i);
    press(&mut e, 2, p, Mods::default());
    assert_eq!(e.caret_arrows(), None, "lrTbV runs left to right");
}

fn drag(e: &mut Editor, pg: usize, from: [f64; 2], to: [f64; 2], mods: Mods) {
    let (x, y) = e.stack().to_doc(pg, from[0], from[1]).expect("on the stack");
    e.pointer_input(PointerKind::Down, x, y, 1, mods, fonts());
    let (x, y) = e.stack().to_doc(pg, to[0], to[1]).expect("on the stack");
    e.pointer_input(PointerKind::Move, x, y, 0, mods, fonts());
    e.pointer_input(PointerKind::Up, x, y, 0, mods, fonts());
}

#[test]
fn dragging_down_a_tbrl_line_selects_along_it() {
    let mut e = layout_fixture("vertical-directions.docx");
    let (g, i) = line_of(&mut e, 0, "Section 1", is_body);
    let l = &g.lines[i];
    let at = |k: usize| l.page(l.stops[k].x, (l.top + l.bottom) / 2.0);
    // Inside "Section", the line's first word.
    drag(&mut e, 0, at(1), at(5), Mods::default());
    let (a, b) = e.selection().ordered(&e.index).expect("a range");
    assert_eq!((a.para_offset().map(|p| p.1), b.para_offset().map(|p| p.1)), (Some(l.stops[1].unit), Some(l.stops[5].unit)));
    // Across to the next line's band, the drag ends on that line, as far down it as the pointer. It starts outside the
    // selection, which a press inside would drag and drop.
    let next = &g.lines[i + 1];
    // An interior stop before a space: past the first word a drag takes words whole, so it ends past the space
    let text = crate::editing::text::UnitText::of(e.index().para(e.document(), next.para().expect("para")).expect("its paragraph")).text;
    let stop = next.stops[1..next.stops.len() - 1]
        .iter()
        .copied()
        .find(|s| s.unit.0 > next.stops[0].unit.0 + 2 && text.chars().nth(s.unit.0 as usize) == Some(' '))
        .expect("an interior stop before a space");
    let to = next.page(stop.x, (next.top + next.bottom) / 2.0);
    drag(&mut e, 0, at(7), to, Mods::default());
    let (a, b) = e.selection().ordered(&e.index).expect("a range");
    assert_eq!(a.para_offset().map(|p| p.1), Some(l.stops[7].unit), "from where it was pressed");
    assert_eq!(
        b.para_offset(),
        Some((stop.para, crate::layout::Unit(stop.unit.0 + 1))),
        "the drag ends on the next line, at the word under the pointer"
    );
}

#[test]
fn a_double_click_in_a_vertical_line_selects_the_word() {
    let mut e = layout_fixture("vertical-directions.docx");
    let (g, i) = line_of(&mut e, 0, "Section 1", is_body);
    let l = &g.lines[i];
    // "Section" is the line's first word: its third letter.
    let p = l.page((l.stops[2].x + l.stops[3].x) / 2.0, (l.top + l.bottom) / 2.0);
    let (x, y) = e.stack().to_doc(0, p[0], p[1]).expect("on the stack");
    for clicks in [1, 2] {
        e.pointer_input(PointerKind::Down, x, y, clicks, Mods::default(), fonts());
        e.pointer_input(PointerKind::Up, x, y, 0, Mods::default(), fonts());
    }
    let (a, b) = e.selection().ordered(&e.index).expect("a range");
    assert_eq!((a.para_offset().map(|p| p.1.0), b.para_offset().map(|p| p.1.0)), (Some(0), Some(8)), "\"Section \"");
}

#[test]
fn dragged_text_shows_its_drop_caret_across_the_vertical_line() {
    let mut e = layout_fixture("vertical-directions.docx");
    let (g, i) = line_of(&mut e, 0, "Section 1", is_body);
    let l = &g.lines[i];
    let at = |k: usize| l.page(l.stops[k].x, (l.top + l.bottom) / 2.0);
    drag(&mut e, 0, at(0), at(7), Mods::default());
    let inside = l.page((l.stops[3].x + l.stops[4].x) / 2.0, (l.top + l.bottom) / 2.0);
    let next = &g.lines[i + 1];
    let target = next.page(next.stops[4].x, (next.top + next.bottom) / 2.0);
    let (x, y) = e.stack().to_doc(0, inside[0], inside[1]).expect("on the stack");
    e.pointer_input(PointerKind::Down, x, y, 1, Mods::default(), fonts());
    let (x, y) = e.stack().to_doc(0, target[0], target[1]).expect("on the stack");
    e.pointer_input(PointerKind::Move, x, y, 0, Mods::default(), fonts());
    let drop = e.view(0..1).drop.expect("a drop caret");
    assert!(drop.vertical() && drop.right > drop.x + 1.0 && (drop.bottom - drop.top).abs() < 1e-6, "a bar across the line: {drop:?}");
    let band = next.band(next.x0(), next.x1());
    assert!((drop.x - band[0]).abs() < 1e-6 && (drop.top - target[1]).abs() < 1e-6, "at the pointer's place on the line: {drop:?}");
    e.pointer_input(PointerKind::Up, x, y, 0, Mods::default(), fonts());
    let text = &super::texts(&e)[0];
    assert!(text.starts_with("1, ") && text.contains("Section"), "the word moved down the next line: {text:?}");
}

/// A table of `cols` 2000-twip cells of text `r{row}c{col}`, `rows` rows, with every border; `inner` (body XML) is put
/// in the first cell after its paragraph, which has `tc_pr` in its properties.
fn table_xml(rows: usize, cols: usize, tc_pr: &str, inner: &str) -> String {
    let cell = |r: usize, c: usize| {
        let (pr, more) = if r + c == 0 { (tc_pr, inner) } else { ("", "") };
        format!("<w:tc><w:tcPr><w:tcW w:w=\"2000\" w:type=\"dxa\"/>{pr}</w:tcPr><w:p><w:r><w:t>r{r}c{c}</w:t></w:r></w:p>{more}</w:tc>")
    };
    let row = |r: usize| format!("<w:tr>{}</w:tr>", (0..cols).map(|c| cell(r, c)).collect::<String>());
    let sides = ["top", "left", "bottom", "right", "insideH", "insideV"].map(|s| format!("<w:{s} w:val=\"single\" w:sz=\"4\"/>")).concat();
    let grid = "<w:gridCol w:w=\"2000\"/>".repeat(cols);
    format!(
        "<w:tbl><w:tblPr><w:tblW w:w=\"{}\" w:type=\"dxa\"/><w:tblBorders>{sides}</w:tblBorders></w:tblPr><w:tblGrid>{grid}</w:tblGrid>{}</w:tbl>",
        2000 * cols,
        (0..rows).map(row).collect::<String>()
    )
}

fn editor(body: &str) -> Editor {
    let mut e = Editor::open(&super::blank_body(body), fonts()).expect("opens");
    e.complete(fonts());
    e
}

#[test]
fn a_table_in_a_tbrl_section_stands_upright() {
    let mut e = editor(&format!("<w:p><w:r><w:t>Before the table</w:t></w:r></w:p>{}<w:p/>", table_xml(2, 3, "", "")));
    e.execute(crate::api::command::Command::SetSectionTextDirection(crate::api::command::TextFlow::TbRl), fonts());
    e.complete(fonts());
    let g = pages(&mut e).into_iter().next().map(|(_, g)| g).expect("a page");
    assert!(g.lines.iter().any(|l| l.chain.is_empty() && l.dir() == LineDir::Down), "the section's text runs down");
    assert!(g.rows.iter().all(|r| r.turn == crate::layout::vertical::Turn::NONE), "its table is drawn upright, as Word draws it");
    let cell = g.lines.iter().position(|l| !l.chain.is_empty()).expect("a cell line");
    click_lands(&mut e, 0, &g, cell, LineDir::Horizontal);
}

/// A one-cell table whose cell runs tbRl and holds a 1 × 2 table: the inner table is laid out with the cell's lines
/// and drawn turned with them.
fn turned_table() -> Editor {
    editor(&format!(
        "{}<w:p/>",
        table_xml(1, 1, "<w:textDirection w:val=\"tbRl\"/><w:vAlign w:val=\"top\"/>", &format!("{}<w:p/>", table_xml(1, 2, "", "")))
    ))
}

#[test]
fn a_table_turned_in_a_vertical_cell_turns_its_borders_and_handles() {
    let mut e = turned_table();
    let g = pages(&mut e).into_iter().next().map(|(_, g)| g).expect("a page");
    let row = g.rows.iter().find(|r| r.turn != crate::layout::vertical::Turn::NONE).expect("the inner table's row, turned");
    let pr = row.page_rect();
    assert!(pr[3] - pr[1] > pr[2] - pr[0], "its row runs down the page: {pr:?}");
    let inner = g.lines.iter().position(|l| l.chain.len() == 2).expect("an inner cell's line");
    click_lands(&mut e, 0, &g, inner, LineDir::Down);
    // The border between its columns runs across the page; it is hit there and resizes up and down.
    let &(edge, _) = row.edges.iter().find(|(_, k)| *k == 1).expect("an inner edge");
    let mid = (row.rect[1] + row.rect[3]) / 2.0;
    let on = row.turn.at([edge, mid]);
    let (x, y) = e.stack().to_doc(0, on[0], on[1]).expect("on the stack");
    assert_eq!(e.hover(x, y, Mods::default(), fonts()), super::super::input::Cursor::ResizeVertical, "a column border across the page");
    let grid = |e: &Editor| {
        let crate::docx::Block::Table(outer) = &e.document().sections[0].blocks[0] else { panic!("a table") };
        outer.rows[0].cells[0]
            .blocks
            .iter()
            .find_map(|b| if let crate::docx::Block::Table(t) = b { Some(t.grid.clone()) } else { None })
            .expect("the inner table")
    };
    let before = grid(&e);
    drag(&mut e, 0, on, [on[0], on[1] + 18.0], Mods::default());
    let after = grid(&e);
    assert!((after[0] - before[0] - 18.0).abs() < 1.0, "dragging down the page widens the column before: {before:?} {after:?}");
    // Laid out again: the inner paragraph's line moved with the wider column.
    let g = pages(&mut e).into_iter().next().map(|(_, g)| g).expect("a page");
    let row = g.rows.iter().find(|r| r.turn != crate::layout::vertical::Turn::NONE).expect("the inner table's row");
    let inner = g.lines.iter().position(|l| l.chain.len() == 2).expect("an inner cell's line");
    press(&mut e, 0, middle(&g, inner).1, Mods::default());
    let para = g.lines[inner].para().expect("a paragraph");
    assert!(matches!(e.selection(), Selection::Caret(c) if c.para_offset().map(|p| p.0) == Some(para)), "in the inner cell");
    let t = e.view(0..1).table.expect("the caret's table shows its handles");
    assert_eq!(t.turn, row.turn, "the inner table's handles, drawn turned as it is");
}

#[test]
fn moving_a_turned_table_previews_it_on_the_page_and_moves_it_as_laid_out() {
    let mut e = turned_table();
    let g = pages(&mut e).into_iter().next().map(|(_, g)| g).expect("a page");
    let row = g.rows.iter().find(|r| r.turn != crate::layout::vertical::Turn::NONE).expect("the inner table's row, turned");
    let (table, rect, turn) = (row.table, row.rect, row.turn);
    let on = turn.at([rect[0], rect[1]]);
    let (x, y) = e.stack().to_doc(0, on[0], on[1]).expect("on the stack");
    e.table_press(table, x, y);
    let alt = Mods { alt: true, ..Mods::default() };
    let (x, y) = e.stack().to_doc(0, on[0], on[1] + 18.0).expect("on the stack");
    e.table_drag_move(x, y, alt);
    let page = turn.rect(rect);
    let preview = e.view(0..1).table_preview.expect("a preview").1;
    let want = [page[0], page[1] + 18.0, page[2], page[3] + 18.0];
    assert!(preview.iter().zip(want).all(|(a, b)| (a - b).abs() < 1e-6), "drawn where the pointer took it: {preview:?} {want:?}");
    let d = e.table_drag.expect("dragging");
    let [dx, dy] = turn.inverse().along([0.0, 18.0]);
    let local = d.to();
    assert!((local[0] - rect[0] - dx).abs() < 1e-6 && (local[1] - rect[1] - dy).abs() < 1e-6, "moved as laid out: {local:?} {rect:?}");
}

#[test]
fn spelling_and_marks_over_vertical_text_turn_with_it() {
    let mut e = layout_fixture("vertical-directions.docx");
    let (g, i) = line_of(&mut e, 0, "Section 1", is_body);
    let l = &g.lines[i];
    let (para, a, b) = (l.para().expect("a paragraph"), l.stops[1].unit, l.stops[4].unit);
    let bands = e.unit_rects(para, a, b, 0..1);
    let (_, band) = bands.first().expect("a band");
    let (turn, local) = (band.turn, band.rect);
    assert_eq!(turn, l.turn);
    assert!((local[0] - l.stops[1].x).abs() < 1e-6 && (local[2] - l.stops[4].x).abs() < 1e-6, "back along the line: {local:?}");
    assert!((local[1] - l.top).abs() < 1e-6 && (local[3] - l.bottom).abs() < 1e-6, "and across it: {local:?}");
}

#[test]
fn the_selection_bar_of_a_tbrl_section_is_above_its_lines() {
    let mut e = layout_fixture("vertical-directions.docx");
    let (g, i) = line_of(&mut e, 0, "Section 1", is_body);
    let l = &g.lines[i];
    let above = l.page(l.x0() - 20.0, (l.top + l.bottom) / 2.0);
    assert!(above[1] < l.band(l.x0(), l.x1())[1], "above the line's start");
    let (x, y) = e.stack().to_doc(0, above[0], above[1]).expect("on the stack");
    assert_eq!(e.hover(x, y, Mods::default(), fonts()), super::super::input::Cursor::RowSelect);
    press(&mut e, 0, above, Mods::default());
    let (a, b) = e.selection().ordered(&e.index).expect("the line");
    assert_eq!(a.para_offset().map(|p| p.1), l.first_of(l.para().expect("para")), "the line from its start");
    assert!(b.para_offset().is_some_and(|(_, u)| u >= l.stops.last().expect("a stop").unit), "to its end");
}

#[test]
fn upright_east_asian_glyphs_have_a_stop_each() {
    let mut e = layout_fixture("vertical-directions.docx");
    for pg in [2, 3] {
        let (g, i) = line_of(&mut e, pg, "Second paragraph.", is_body);
        let para = g.lines[i].para().expect("para");
        let text = super::super::text::UnitText::of(e.index.para(&e.doc, para).expect("para")).text;
        let cjk: Vec<u32> = text.chars().enumerate().filter(|(_, c)| *c == '漢').map(|(k, _)| k as u32).collect();
        assert!(!cjk.is_empty());
        // Every line of the paragraph drawn on the page
        let mut seen: Vec<u32> = Vec::new();
        for l in g.lines.iter().filter(|l| l.paras.contains(&para)) {
            let (first, last) = (l.first_of(para).expect("a first unit").0, l.last_of(para).expect("a last unit").0);
            for &u in cjk.iter().filter(|&&u| first <= u && u < last) {
                let n = l.stops.iter().filter(|s| s.para == para && s.unit.0 == u).count();
                assert_eq!(n, 1, "page {pg}: the upright glyph at {u} has its own stop");
                seen.push(u);
            }
            let xs: Vec<f64> = l.stops.iter().filter(|s| s.para == para && cjk.contains(&s.unit.0)).map(|s| s.x).collect();
            assert!(xs.windows(2).all(|w| w[1] > w[0] + 1.0), "page {pg}: upright glyphs' stops advance along the line: {xs:?}");
        }
        assert_eq!(seen, cjk, "page {pg}: every upright glyph of the paragraph has a stop, in order");
    }
}

/// `name`'s first text box frame turned a quarter.
fn rotated_box(name: &str) -> Editor {
    rotated_box_by(name, 5_400_000)
}

/// `name`'s first text box frame turned `rot` sixty-thousandths of a degree.
fn rotated_box_by(name: &str, rot: u32) -> Editor {
    use std::io::{Read, Write};
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/layout").join(name);
    let src = std::fs::read(path).expect("the fixture");
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(src)).expect("a zip");
    let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for i in 0..z.len() {
        let mut f = z.by_index(i).expect("an entry");
        let name = f.name().expect("a name").to_string();
        let mut data = Vec::new();
        f.read_to_end(&mut data).expect("reads");
        if name == "word/document.xml" {
            let xml = String::from_utf8(data).expect("utf-8");
            let k = xml.find("<wps:wsp>").expect("a shape");
            let at = k + xml[k..].find("<a:xfrm").expect("its frame") + "<a:xfrm".len();
            data = format!("{} rot=\"{rot}\"{}", &xml[..at], &xml[at..]).into_bytes();
        }
        out.start_file(name, zip::write::SimpleFileOptions::default()).expect("writes");
        out.write_all(&data).expect("writes");
    }
    let bytes = out.finish().expect("a zip").into_inner();
    let mut e = Editor::open(&bytes, fonts()).expect("opens");
    e.complete(fonts());
    e
}

#[test]
fn a_turned_text_box_turns_its_vertical_text_further_and_keys_keep_to_the_flow() {
    let mut e = rotated_box("vertical-model-c15.docx");
    let (g, i) = line_of(&mut e, 0, "A vert text box", |s| matches!(s, StoryId::TextBox(_)));
    // Down the box, the box turned a quarter clockwise: the text runs right to left on the page.
    assert_eq!(g.lines[i].turn.along([1.0, 0.0]), [-1.0, 0.0]);
    click_lands(&mut e, 0, &g, i, LineDir::Horizontal);
    let (s, _) = middle(&g, i);
    assert_eq!(e.caret_arrows().map(|a| (a.along, a.stack)), Some(([0, 1], [-1, 0])), "the keys follow the box's text flow");
    key(&mut e, "ArrowDown", Mods::default());
    assert_eq!(focus(&e), (s.para, s.unit.0 + 1));
}

#[test]
fn the_text_under_the_pointer_says_which_way_it_runs() {
    let mut e = layout_fixture("vertical-directions.docx");
    for (pg, text, dir) in [(0, "Section 1", LineDir::Down), (2, "Section 3", LineDir::Horizontal)] {
        let (g, i) = line_of(&mut e, pg, text, is_body);
        let (_, p) = middle(&g, i);
        let (x, y) = e.stack().to_doc(pg, p[0], p[1]).expect("on the stack");
        assert_eq!(e.text_dir_at(x, y, fonts()), Some(dir));
    }
}

#[test]
fn a_repeated_header_s_vertical_text_says_it_runs_down() {
    let cell =
        |pr: &str, t: &str| format!("<w:tc><w:tcPr><w:tcW w:w=\"2000\" w:type=\"dxa\"/>{pr}</w:tcPr><w:p><w:r><w:t>{t}</w:t></w:r></w:p></w:tc>");
    let header = format!("<w:tr><w:trPr><w:tblHeader/></w:trPr>{}{}</w:tr>", cell("<w:textDirection w:val=\"tbRl\"/>", "Head"), cell("", "h"));
    let rows: String = (0..80).map(|r| format!("<w:tr>{}{}</w:tr>", cell("", &format!("a{r}")), cell("", "b"))).collect();
    let grid = "<w:gridCol w:w=\"2000\"/>".repeat(2);
    let mut e =
        editor(&format!("<w:tbl><w:tblPr><w:tblW w:w=\"4000\" w:type=\"dxa\"/></w:tblPr><w:tblGrid>{grid}</w:tblGrid>{header}{rows}</w:tbl><w:p/>"));
    let g = pages(&mut e).into_iter().find(|(p, _)| *p == 1).map(|(_, g)| g).expect("a second page");
    let i = g.lines.iter().position(|l| l.repeat && l.dir() == LineDir::Down).expect("the header's vertical line, repeated");
    let (_, p) = middle(&g, i);
    let (x, y) = e.stack().to_doc(1, p[0], p[1]).expect("on the stack");
    assert_eq!(e.text_dir_at(x, y, fonts()), Some(LineDir::Down));
}

#[test]
fn a_text_box_turned_30_degrees_is_hit_selected_and_draws_a_slanted_caret() {
    for (name, text) in [("textbox-autofit-columns.docx", "The meeting forecast"), ("vertical-model-c15.docx", "A vert text box")] {
        let mut e = rotated_box_by(name, 1_800_000);
        let (g, i) = line_of(&mut e, 0, text, |s| matches!(s, StoryId::TextBox(_)));
        let l = &g.lines[i];
        assert!(!l.turn.is_quarter(), "{name}: the line is slanted");
        let (s, p) = middle(&g, i);
        press(&mut e, 0, p, Mods::default());
        assert_eq!(focus(&e), (s.para, s.unit.0), "{name}: the click lands on the stop under it");
        let c = e.caret_view().expect("a caret");
        assert!(c.slanted(), "{name}: the bar is slanted with the line: {c:?}");
        let [a, b] = c.ends;
        let (top, bottom) = (l.page(s.x, l.top), l.page(s.x, l.bottom));
        assert!(
            (a[0] - top[0]).hypot(a[1] - top[1]) < 1e-6 && (b[0] - bottom[0]).hypot(b[1] - bottom[1]) < 1e-6,
            "{name}: across the line at the stop"
        );
        let para = l.para().expect("a paragraph");
        let story = g.stories[l.story].story;
        let (a, b) = (Position::text(story, para, l.stops[1].unit), Position::text(story, para, l.stops[l.stops.len() - 2].unit));
        e.set_selection(Selection::range(a, b, crate::editing::selection::Granularity::Char));
        let SelectionView::Text(bands) = e.view(0..1).selection else { panic!("a text selection") };
        let (_, band) = bands.first().expect("a band");
        assert_eq!(band.turn, l.turn, "{name}: the strip is turned with its line");
        assert!(band.contains(p[0], p[1]), "{name}: and covers the text under it");
        let corner = l.page(l.stops[1].x - 0.5, l.top);
        assert!(!band.contains(corner[0], corner[1]), "{name}: but not past its start");
        let st = &g.stories[l.story];
        let r = st.rect;
        let inside = st.turn.at([r[0] + 2.0, r[1] + 2.0]);
        let in_box = |p: [f64; 2]| g.hit_text(p[0], p[1], &|_| true).map(|h| h.story) == Some(story);
        assert!(!(r[0]..=r[2]).contains(&inside[0]) || !(r[1]..=r[3]).contains(&inside[1]), "{name}: the turned corner sticks out");
        assert!(in_box(inside), "{name}: the box is hit where it is drawn");
        assert!(!in_box([r[0] + 2.0, r[1] + 2.0]), "{name}: not where it is not");
    }
}

/// A tbRl section of eight paragraphs, the first holding a 60 pt square floated at (300, 300) from the page.
fn tbrl_with_float() -> Editor {
    use crate::layout::tests::shapes::{anchor, bytes, sp};
    let a = anchor(300, 300, 60, 60, 1, false, 1, &sp(0, 0, 60, 60, "rect", "", "0000FF"));
    let words = "Alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron pi rho sigma tau.";
    let paras: String = (0..8).map(|k| format!("<w:p><w:r><w:t>P{k} {words}</w:t></w:r>{}</w:p>", if k == 0 { &a } else { "" })).collect();
    let mut e = Editor::open(&bytes(&paras).expect("a package"), fonts()).expect("opens");
    e.complete(fonts());
    e.execute(crate::api::command::Command::SetSectionTextDirection(crate::api::command::TextFlow::TbRl), fonts());
    e.complete(fonts());
    e
}

#[test]
fn a_float_dragged_in_a_tbrl_section_moves_with_the_pointer_and_anchors_to_the_line_under_it() {
    let mut e = tbrl_with_float();
    let f = crate::layout::FloatId(1);
    let (_, before) = e.float_box(f, fonts()).expect("drawn");
    let g = pages(&mut e).into_iter().next().map(|(_, g)| g).expect("a page");
    let l = g.lines.iter().filter(|l| l.chain.is_empty()).nth(4).expect("a fifth line");
    let band = l.band(l.x0(), l.x1());
    // The float's right edge, where its lines start stacking, onto the middle of the line.
    let (dx, dy) = ((band[0] + band[2]) / 2.0 - before[2], 15.0);
    e.set_selection(Selection::Object(super::super::selection::ObjectRef::Floating(f)));
    e.complete(fonts());
    let c = [(before[0] + before[2]) / 2.0, (before[1] + before[3]) / 2.0];
    drag(&mut e, 0, c, [c[0] + dx, c[1] + dy], Mods { alt: true, ..Mods::default() });
    e.complete(fonts());
    let (_, after) = e.float_box(f, fonts()).expect("drawn");
    assert!((after[0] - before[0] - dx).abs() < 0.1 && (after[1] - before[1] - dy).abs() < 0.1, "moved with the pointer: {before:?} -> {after:?}");
    assert_eq!(e.index.float_anchor(f), l.para(), "anchored to the line its frame starts on");
}

#[test]
fn an_inline_picture_floated_in_a_tbrl_section_stays_where_it_was_drawn() {
    let data = super::open("FX-OBJ.docx").document().images.last().cloned().expect("an image");
    let mut e = editor("<w:p><w:r><w:t>Body text here</w:t></w:r></w:p>");
    e.execute(crate::api::command::Command::SetSectionTextDirection(crate::api::command::TextFlow::TbRl), fonts());
    e.complete(fonts());
    e.set_caret_for_benchmark(Selection::Caret(Position::text(StoryId::Body, super::para_id(&e, 0), crate::layout::Unit(2))));
    let pic = crate::api::command::Picture { data: crate::api::value::Bytes(data), alt_text: None };
    e.execute(crate::api::command::Command::InsertPicture(pic), fonts());
    e.complete(fonts());
    let g = pages(&mut e).into_iter().next().map(|(_, g)| g).expect("a page");
    let (l, r) = g.lines.iter().find_map(|l| l.pictures.first().map(|p| (l, p.2))).expect("the inline picture");
    let drawn = l.turn.rect(r);
    let out = e.execute(crate::api::command::Command::SetObjectWrap(crate::api::value::ObjectWrap::Square), fonts());
    assert!(matches!(out, super::super::commands::Outcome::Changed(_)), "{out:?}");
    let Selection::Object(super::super::selection::ObjectRef::Floating(f)) = e.selection() else { panic!("a float") };
    let (_, now) = e.float_box(f, fonts()).expect("drawn");
    assert!((now[0] - drawn[0]).abs() < 1.0 && (now[1] - drawn[1]).abs() < 1.0, "where it lay in line: {drawn:?} -> {now:?}");
}

/// Four paragraphs in a section running `flow`, the first with a footnote.
fn vertical_with_footnote(flow: crate::api::command::TextFlow) -> Editor {
    use crate::api::command::Command;
    let words = "Alpha beta gamma delta epsilon zeta eta theta iota kappa.";
    let mut e = editor(&(0..4).map(|k| format!("<w:p><w:r><w:t>P{k} {words}</w:t></w:r></w:p>")).collect::<String>());
    e.set_caret_for_benchmark(Selection::Caret(Position::text(StoryId::Body, super::para_id(&e, 0), crate::layout::Unit(2))));
    e.execute(Command::InsertFootnote, fonts());
    e.execute(Command::InsertText("Note text runs here.".into()), fonts());
    e.execute(Command::SetSectionTextDirection(flow), fonts());
    e.complete(fonts());
    e
}

#[test]
fn clicks_in_a_vertical_section_with_a_footnote_land_in_the_story_under_them() {
    use crate::api::command::TextFlow;
    for flow in [TextFlow::TbRl, TextFlow::TbLrV, TextFlow::BtLr, TextFlow::TbRlV] {
        let mut e = vertical_with_footnote(flow);
        let g = pages(&mut e).into_iter().next().map(|(_, g)| g).expect("a page");
        let is_note = |s: StoryId| matches!(s, StoryId::Footnote(_));
        let note = (0..g.lines.len()).find(|&i| is_note(g.stories[g.lines[i].story].story)).expect("the footnote's line");
        assert_ne!(g.lines[note].dir(), LineDir::Horizontal, "{flow:?}: the note runs with the section");
        let body: Vec<usize> = (0..g.lines.len()).filter(|&i| g.stories[g.lines[i].story].story == StoryId::Body).collect();
        assert!(body.len() > 1, "{flow:?}: body lines");
        for &i in &body {
            let l = &g.lines[i];
            for s in [l.stops[0], l.stops[l.stops.len() / 2], l.stops[l.stops.len() - 1]] {
                let p = l.page(s.x, (l.top + l.bottom) / 2.0);
                assert_eq!(g.hit_text(p[0], p[1], &|_| true).map(|h| h.story), Some(StoryId::Body), "{flow:?}: body line {i} at {p:?}");
            }
        }
        let l = &g.lines[note];
        let story = g.stories[l.story].story;
        for s in l.stops.iter() {
            let p = l.page(s.x, (l.top + l.bottom) / 2.0);
            let hit = g.hit_text(p[0], p[1], &|_| true).expect("a hit");
            assert_eq!((hit.story, hit.unit), (story, s.unit), "{flow:?}: the note's stop at {p:?}");
        }
        let (_, p) = middle(&g, note);
        press(&mut e, 0, p, Mods::default());
        assert_eq!(e.selection().focus().map(|f| f.story), Some(story), "{flow:?}: a click puts the caret in the note");
    }
}

/// The text of the paragraph the caret is in.
fn caret_para_text(e: &Editor) -> String {
    let (p, _) = focus(e);
    super::super::text::UnitText::of(e.index.para(&e.doc, p).expect("the paragraph")).text
}

#[test]
fn a_line_move_past_a_vertical_cell_goes_to_the_next_row() {
    for (flow, next, prev) in [
        ("tbRl", "ArrowLeft", "ArrowRight"),
        ("tbRlV", "ArrowLeft", "ArrowRight"),
        ("btLr", "ArrowRight", "ArrowLeft"),
        ("tbLrV", "ArrowRight", "ArrowLeft"),
    ] {
        let cell = |r: usize| {
            format!(
                "<w:tc><w:tcPr><w:tcW w:w=\"2000\" w:type=\"dxa\"/><w:textDirection w:val=\"{flow}\"/></w:tcPr><w:p><w:r><w:t>r{r}c0</w:t></w:r></w:p></w:tc>"
            )
        };
        let rows: String = (0..3).map(|r| format!("<w:tr><w:trPr><w:trHeight w:val=\"1400\"/></w:trPr>{}</w:tr>", cell(r))).collect();
        let mut e = editor(&format!(
            "<w:tbl><w:tblPr><w:tblW w:w=\"2000\" w:type=\"dxa\"/></w:tblPr><w:tblGrid><w:gridCol w:w=\"2000\"/></w:tblGrid>{rows}</w:tbl><w:p/>"
        ));
        let g = pages(&mut e).into_iter().next().map(|(_, g)| g).expect("a page");
        let i = (0..g.lines.len())
            .find(|&i| !g.lines[i].chain.is_empty() && starts(&e, g.lines[i].para().expect("para"), &g.lines[i], "r1c0"))
            .expect("the middle cell's line");
        assert_ne!(g.lines[i].dir(), LineDir::Horizontal, "{flow}: the cell's text is vertical");
        let (_, p) = middle(&g, i);
        press(&mut e, 0, p, Mods::default());
        key(&mut e, next, Mods::default());
        assert_eq!(caret_para_text(&e), "r2c0", "{flow}: {next} past the cell's last line goes to the row below");
        press(&mut e, 0, p, Mods::default());
        key(&mut e, prev, Mods::default());
        assert_eq!(caret_para_text(&e), "r0c0", "{flow}: {prev} past its first line goes to the row above");
    }
}

#[test]
fn a_line_move_out_of_an_empty_turned_note_line_leaves_it_or_stops() {
    let ns = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main""#;
    let foot = format!(r#"<w:footnotes {ns}><w:footnote w:id="1"><w:p/></w:footnote></w:footnotes>"#);
    let body = r#"<w:p><w:r><w:t>Body</w:t></w:r><w:r><w:footnoteReference w:id="1"/></w:r></w:p>"#;
    let sect = r#"<w:textDirection w:val="tbRl"/>"#;
    let bytes = super::empty_stories::build(body, sect, &[("footnotes.xml", "footnotes", "footnotes", &foot)]);
    let mut e = Editor::open(&bytes, fonts()).expect("opens");
    e.complete(fonts());
    let g = pages(&mut e)[0].1.clone();
    let line = g.lines.iter().find(|l| matches!(g.stories[l.story].story, StoryId::Footnote(_))).expect("a note line");
    assert_ne!(line.dir(), LineDir::Horizontal, "the note's line is turned");
    let band = line.band(line.x0(), line.x1());
    assert!((band[3] - band[1]).abs() < 0.01, "an empty line has no extent along the page's height: {band:?}");
    let s = line.stops[0];
    let focus = Position::text(g.stories[line.story].story, s.para, s.unit);
    let goal = line.page(s.x, (line.top + line.bottom) / 2.0);
    for down in [true, false] {
        let to = e.cross_vertical(focus, goal, down, fonts());
        assert_ne!(to, Some(focus), "down: {down}: not the place it started from");
    }
}

#[test]
fn a_composition_in_a_tbrl_line_is_underlined_along_it() {
    let mut e = layout_fixture("vertical-directions.docx");
    let (g, i) = line_of(&mut e, 0, "Section 1", is_body);
    let (l, story) = (&g.lines[i], g.stories[g.lines[i].story].story);
    let (para, at) = (l.para().expect("a paragraph"), l.stops[1].unit);
    e.set_selection(Selection::Caret(Position::text(story, para, at)));
    e.compose("ab", 2, fonts());
    let end = crate::layout::Unit(at.0 + 2);
    let g = pages(&mut e).into_iter().find(|(p, _)| *p == 0).map(|(_, g)| g).expect("the page");
    let l = g.lines.iter().find(|l| l.x_of(para, at).is_some() && l.x_of(para, end).is_some()).expect("the composition's line");
    assert_eq!(l.dir(), LineDir::Down, "a tbRl line");
    let (xa, xb) = (l.x_of(para, at).expect("its start"), l.x_of(para, end).expect("its end"));
    let corners = [l.page(xa, l.top), l.page(xa, l.bottom), l.page(xb, l.top), l.page(xb, l.bottom)];
    let want = [
        corners.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min),
        corners.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min),
        corners.iter().map(|p| p[0]).fold(f64::NEG_INFINITY, f64::max),
        corners.iter().map(|p| p[1]).fold(f64::NEG_INFINITY, f64::max),
    ];
    let bands = e.view(0..1).composition;
    let [(0, band)] = &bands[..] else { panic!("one band on the page: {bands:?}") };
    assert_eq!(band.turn, l.turn, "drawn as the line is");
    let got = band.page();
    assert!(got.iter().zip(want).all(|(a, b)| (a - b).abs() < 1e-6), "the band spans the composed text down the line: {got:?} {want:?}");
    e.end_composition(fonts());
    assert!(e.view(0..1).composition.is_empty(), "gone with the composition");
}

#[test]
fn a_dragged_anchor_in_a_tbrl_section_shows_where_its_paragraphs_anchor_does() {
    let mut e = tbrl_with_float();
    let f = crate::layout::FloatId(1);
    e.set_selection(Selection::Object(super::super::selection::ObjectRef::Floating(f)));
    e.complete(fonts());
    let (_, [ax, ay]) = e.view(0..1).anchor.expect("the anchor shows");
    let u = 0.75 / e.viewport.zoom.max(0.1);
    let ring = [(ax - 18.0 * u).max(10.0 * u), ay + 8.0 * u];
    let g = pages(&mut e).into_iter().next().map(|(_, g)| g).expect("a page");
    let l = g.lines.iter().filter(|l| l.chain.is_empty()).nth(4).expect("a fifth line");
    let band = l.band(l.x0(), l.x1());
    let to = [(band[0] + band[2]) / 2.0, (band[1] + band[3]) / 2.0];
    let (x, y) = e.stack().to_doc(0, ring[0], ring[1]).expect("on the stack");
    e.pointer_input(PointerKind::Down, x, y, 1, Mods::default(), fonts());
    assert!(e.anchor_drag.is_some(), "the anchor taken");
    let (x, y) = e.stack().to_doc(0, to[0], to[1]).expect("on the stack");
    e.pointer_input(PointerKind::Move, x, y, 0, Mods::default(), fonts());
    let para = l.para().expect("a paragraph's line");
    let shown = e.view(0..1).anchor.expect("the dragged anchor shows");
    assert_eq!(shown, (0, e.anchor_point(0, para).expect("its anchor")), "turned onto the page as its line is");
}

#[test]
fn a_fixed_vertical_text_boxs_overflow_holds_no_caret_places() {
    use crate::layout::tests::shapes::{PT, anchor, bytes};
    let ps: String = (0..12).map(|k| format!("<w:p><w:r><w:t>Line {k}</w:t></w:r></w:p>")).collect();
    let wsp = format!(
        r#"<wps:wsp><wps:cNvSpPr txBox="1"/><wps:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="{w}" cy="{h}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></wps:spPr><wps:txbx><w:txbxContent>{ps}</w:txbxContent></wps:txbx><wps:bodyPr vert="vert" lIns="0" tIns="0" rIns="0" bIns="0"/></wps:wsp>"#,
        w = 60 * PT,
        h = 120 * PT
    );
    let body = format!("<w:p><w:r><w:t>Text</w:t></w:r>{}</w:p>", anchor(144, 200, 60, 120, 1, false, 1, &wsp));
    let mut e = Editor::open(&bytes(&body).expect("a package"), fonts()).expect("opens");
    e.complete(fonts());
    let g = pages(&mut e).into_iter().next().map(|(_, g)| g).expect("a page");
    let s = g.stories.iter().position(|s| matches!(s.story, StoryId::TextBox(_))).expect("the box's story");
    let [x0, _, x1, _] = g.stories[s].rect;
    let xs: Vec<f64> =
        g.lines.iter().filter(|l| l.story == s).flat_map(|l| l.stops.iter().map(|st| l.page(st.x, (l.top + l.bottom) / 2.0)[0])).collect();
    assert!(!xs.is_empty(), "the lines that fit");
    assert!(xs.iter().all(|&x| x >= x0 - 2.0 && x <= x1 + 2.0), "no caret place past the box ({x0} .. {x1}): {xs:?}");
}
