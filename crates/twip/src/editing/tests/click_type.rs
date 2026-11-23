//! Click and Type

use super::super::Editor;
use super::super::commands::Command;
use super::super::input::PointerKind;
use super::super::keymap::Mods;
use super::super::position::StoryId;
use super::{blank_with, fonts, texts};

fn open_one() -> Editor {
    let mut e = Editor::open(&blank_with("Hello."), fonts()).expect("opens");
    e.complete(fonts());
    e
}

/// The body text's left, right and first line's middle on page 0, and the page's body bottom.
fn frame(e: &mut Editor) -> (f64, f64, f64, f64) {
    let g = e.page_geometries().into_iter().next().expect("a page").1;
    let body = g.stories.iter().find(|s| s.story == StoryId::Body && s.float.is_none()).expect("a body");
    let line = g.lines.iter().find(|l| g.stories[l.story].story == StoryId::Body).expect("a line");
    (body.rect[0], body.rect[2], (line.top + line.bottom) / 2.0, g.body_area.expect("a body area").1)
}

fn double_click(e: &mut Editor, x: f64, y: f64) {
    let (x, y) = e.stack().to_doc(0, x, y).expect("on the stack");
    for clicks in [1, 2] {
        e.pointer_input(PointerKind::Down, x, y, clicks, Mods::default(), fonts());
        e.pointer_input(PointerKind::Up, x, y, clicks, Mods::default(), fonts());
    }
}

#[test]
fn a_double_click_below_the_end_adds_paragraphs_down_to_it_centered_as_one_step() {
    let mut e = open_one();
    let (l, r, line, _) = frame(&mut e);
    let y = line + 200.0;
    let (dx, dy) = e.stack().to_doc(0, (l + r) / 2.0, y).expect("on the stack");
    let t = e.click_and_type_at(dx, dy, fonts()).expect("a blank area");
    assert_eq!((t.align, t.below), (super::super::click_type::ClickAlign::Center, true));
    double_click(&mut e, (l + r) / 2.0, y);
    let n = texts(&e).len();
    assert!(n > 5, "paragraphs down to the click: {n}");
    let caret = e.selection.focus().and_then(|p| e.caret_at(p)).expect("a caret");
    assert!(caret.top <= y + 1.0 && caret.bottom + 20.0 >= y, "{caret:?} {y}");
    let last = e.document().sections[0].blocks.iter().rev().find_map(|b| if let crate::docx::Block::Para(p) = b { Some(p.style.jc) } else { None });
    assert_eq!(last, Some(crate::docx::props::Jc::Center));
    e.execute(Command::Undo, fonts());
    assert_eq!(texts(&e), ["Hello."], "one undo step");
}

#[test]
fn a_double_click_beside_a_line_tabs_out_to_it_and_it_can_be_turned_off() {
    let mut e = open_one();
    let (l, _, line, _) = frame(&mut e);
    double_click(&mut e, l + 200.0, line);
    assert_eq!(texts(&e), ["Hello.\t"]);
    let tabs =
        e.document().sections[0].blocks.iter().find_map(|b| if let crate::docx::Block::Para(p) = b { Some(p.style.tabs.clone()) } else { None });
    let left = e.document().sections[0].left;
    assert_eq!(tabs.map(|t| t.iter().map(|t| t.pos).collect::<Vec<_>>()), Some(vec![200.0 - left]), "at the click, from the left margin");
    e.execute(Command::Undo, fonts());
    e.set_click_and_type(false);
    double_click(&mut e, l + 200.0, line);
    assert_eq!(texts(&e), ["Hello."], "a plain double-click");
}

#[test]
fn a_double_click_between_pages_hides_the_white_space_and_on_their_line_shows_it() {
    use super::super::view::PAGE_GAP;
    let mut e = super::open("FX-LONG-50.docx");
    e.complete(fonts());
    let stack = e.stack();
    assert!(stack.pages.len() > 2 && stack.bands.is_empty());
    let [x, y, w, h] = stack.pages[0];
    let (gx, gy) = (x + w / 2.0, y + h + PAGE_GAP / 2.0);
    assert_eq!(e.white_space_toggle_at(gy), Some(true), "the hide white space pointer");
    e.pointer_input(PointerKind::Move, gx, gy, 0, Mods::default(), fonts());
    let tip = e.hover_tip().cloned().expect("a tip");
    assert_eq!((tip.0, tip.1.text.as_str()), (super::super::hover::HoverTip::WhiteSpace, "Double-click to hide white space"));
    assert_eq!(e.white_space_toggle_at(y + h / 2.0), None, "on a page");
    let g = e.canvas_press(gx, gy, 2, Mods::default(), fonts()).map(|g| g.gesture);
    assert_eq!(g, Some(super::super::gestures::CanvasGesture::PageGapDoubleClick));
    for clicks in [1, 2] {
        e.pointer_input(PointerKind::Down, gx, gy, clicks, Mods::default(), fonts());
        e.pointer_input(PointerKind::Up, gx, gy, clicks, Mods::default(), fonts());
    }
    assert!(e.white_space_hidden());
    let hidden = e.stack();
    let (a, b) = (hidden.shown(0).expect("page 1"), hidden.shown(1).expect("page 2"));
    assert!((a[1] + a[3] - b[1]).abs() < 1e-9, "the pages meet: {a:?} {b:?}");
    assert!(a[3] < h, "the margins are hidden");
    assert!(hidden.height < stack.height);
    let (page, _, py) = hidden.page_at(gx, b[1] + 5.0).expect("a page");
    assert_eq!(page, 1);
    assert!(py >= hidden.bands[1][0], "below its body's top");
    assert_eq!(e.white_space_toggle_at(b[1]), Some(false), "the show white space pointer");
    for clicks in [1, 2] {
        e.pointer_input(PointerKind::Down, gx, b[1], clicks, Mods::default(), fonts());
        e.pointer_input(PointerKind::Up, gx, b[1], clicks, Mods::default(), fonts());
    }
    assert!(!e.white_space_hidden(), "shown again");
}

#[test]
fn hidden_white_space_ends_where_a_tall_header_pushes_the_body() {
    let sec = r#"<w:sectPr><w:headerReference w:type="default" r:id="rIdH"/><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr>"#;
    let header: String = (0..12).map(|i| format!("<w:p><w:r><w:t>Header line {i}</w:t></w:r></w:p>")).collect();
    let mut e = super::stories::package(&format!("<w:p><w:r><w:t>Body</w:t></w:r></w:p>{sec}"), Some(Some(&header)));
    let body_top = e.snapshot().pages()[0].body_area.expect("a body").0;
    assert!(body_top > 72.0 + 20.0, "the header pushes the body down: {body_top}");
    e.execute(Command::EditHeader, fonts());
    assert!(matches!(e.selection().focus().map(|f| f.story), Some(StoryId::HeaderFooter(_))));
    e.set_white_space_hidden(true);
    assert_eq!(e.selection().focus().map(|f| f.story), Some(StoryId::Body), "hiding closes the header");
    let stack = e.stack();
    assert!((stack.bands[0][0] - body_top).abs() < 1e-9, "hidden down to the body: {:?}", stack.bands[0]);
    let bottom = e.snapshot().pages()[0].body_area.expect("a body").1;
    assert!((stack.bands[0][1] - bottom).abs() < 1e-9);
}

#[test]
fn a_double_click_on_footnote_text_below_the_end_selects_a_word() {
    let ns = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main""#;
    let foot =
        format!(r#"<w:footnotes {ns}><w:footnote w:id="1"><w:p><w:r><w:t>Some footnote words here</w:t></w:r></w:p></w:footnote></w:footnotes>"#);
    let body = r#"<w:p><w:r><w:t>Body</w:t></w:r><w:r><w:footnoteReference w:id="1"/></w:r></w:p>"#;
    let bytes = super::empty_stories::build(body, "", &[("footnotes.xml", "footnotes", "footnotes", &foot)]);
    let mut e = Editor::open(&bytes, fonts()).expect("opens");
    e.complete(fonts());
    let g = e.page_geometries()[0].1.clone();
    let line = g.lines.iter().find(|l| matches!(g.stories[l.story].story, StoryId::Footnote(_))).expect("a footnote line");
    let stop = line.stops.iter().filter(|s| !s.end).nth(8).expect("a stop in its text");
    let (x, y) = e.stack().to_doc(0, stop.x + 1.0, (line.top + line.bottom) / 2.0).expect("on the stack");
    assert!(e.click_and_type_at(x, y, fonts()).is_none(), "not a blank area");
    let (sx, sy) = e.stack().to_doc(0, stop.x + 1.0, line.top - 2.0).expect("on the stack");
    assert!(e.click_and_type_at(sx, sy, fonts()).is_none(), "not the separator");
    let (bx, by) = e.stack().to_doc(0, stop.x + 1.0, line.top - 100.0).expect("on the stack");
    assert!(e.click_and_type_at(bx, by, fonts()).is_some(), "the blank area above the notes");
    for clicks in [1, 2] {
        e.pointer_input(PointerKind::Down, x, y, clicks, Mods::default(), fonts());
        e.pointer_input(PointerKind::Up, x, y, clicks, Mods::default(), fonts());
    }
    assert_eq!(texts(&e).len(), 1, "no paragraphs added");
    let sel = e.selection();
    assert!(matches!(sel.focus().map(|f| f.story), Some(StoryId::Footnote(_))) && !sel.is_collapsed(), "a word: {sel:?}");
}

#[test]
fn a_protected_document_has_no_click_and_type() {
    use crate::api::command::{DocumentProtection, ProtectionKind};
    for kind in [ProtectionKind::ReadOnly, ProtectionKind::Comments, ProtectionKind::Forms] {
        let mut e = open_one();
        let p = DocumentProtection { kind: Some(kind), enforce: Some(true), ..Default::default() };
        e.execute(Command::SetDocumentProtection(p), fonts());
        let (l, r, line, _) = frame(&mut e);
        let (dx, dy) = e.stack().to_doc(0, (l + r) / 2.0, line + 200.0).expect("on the stack");
        assert!(e.click_and_type_at(dx, dy, fonts()).is_none(), "{kind:?} below the end");
        let (bx, by) = e.stack().to_doc(0, l + 200.0, line).expect("on the stack");
        assert!(e.click_and_type_at(bx, by, fonts()).is_none(), "{kind:?} beside a line");
        double_click(&mut e, (l + r) / 2.0, line + 200.0);
        assert_eq!(texts(&e), ["Hello."], "{kind:?}");
    }
}

#[test]
fn below_a_centered_end_the_left_zone_adds_left_aligned_paragraphs_in_the_click_and_type_style() {
    let body = r#"<w:p><w:pPr><w:jc w:val="center"/><w:ind w:left="720"/><w:tabs><w:tab w:val="left" w:pos="2880"/></w:tabs></w:pPr><w:r><w:t>Hello.</w:t></w:r></w:p>"#;
    let style = r#"<w:style w:type="paragraph" w:styleId="Typed"><w:name w:val="Typed"/><w:next w:val="Normal"/><w:pPr><w:spacing w:after="0"/></w:pPr></w:style>"#;
    let mut e = Editor::open(&super::clipboard::with_style(style, body), fonts()).expect("opens");
    e.complete(fonts());
    e.set_click_and_type_style(Some("Typed".into()));
    let (_, _, line, _) = frame(&mut e);
    let l = e.document().sections[0].left;
    let (dx, dy) = e.stack().to_doc(0, l + 2.0, line + 200.0).expect("on the stack");
    let t = e.click_and_type_at(dx, dy, fonts()).expect("a blank area");
    assert_eq!((t.align, t.below), (super::super::click_type::ClickAlign::Left, true));
    double_click(&mut e, l + 2.0, line + 200.0);
    let paras: Vec<_> = e.document().sections[0]
        .blocks
        .iter()
        .filter_map(|b| if let crate::docx::Block::Para(p) = b { Some(p.style.clone()) } else { None })
        .collect();
    assert!(paras.len() > 3, "paragraphs down to the click: {}", paras.len());
    for p in &paras[1..] {
        assert_eq!(p.jc, crate::docx::props::Jc::Left, "left-aligned");
        assert_eq!((p.ind_left, p.tabs.len()), (0.0, 0), "none of the last paragraph's indents or tabs");
        assert_eq!(p.style_id.as_deref(), Some("Typed"), "every new paragraph has the Click and Type style");
    }
    e.execute(Command::Undo, fonts());
    assert_eq!(texts(&e), ["Hello."], "one undo step");
    assert_eq!(e.document().sections[0].blocks.len(), 1);
}

#[test]
fn below_the_end_near_the_left_edge_indents_the_first_line_to_the_click() {
    let mut e = open_one();
    let (_, _, line, _) = frame(&mut e);
    let l = e.document().sections[0].left;
    let (dx, dy) = e.stack().to_doc(0, l + 20.0, line + 100.0).expect("on the stack");
    let t = e.click_and_type_at(dx, dy, fonts()).expect("a blank area");
    assert_eq!(t.align, super::super::click_type::ClickAlign::LeftIndent);
    double_click(&mut e, l + 20.0, line + 100.0);
    let last =
        e.document().sections[0].blocks.iter().rev().find_map(|b| if let crate::docx::Block::Para(p) = b { Some(p.style.clone()) } else { None });
    let last = last.expect("a paragraph");
    assert_eq!((last.ind_first, last.tabs.len()), (20.0, 0), "a first-line indent, no tab");
    assert!(texts(&e).iter().all(|t| !t.contains('\t')));
    let (dx, dy) = e.stack().to_doc(0, l + 60.0, line + 300.0).expect("on the stack");
    assert_eq!(e.click_and_type_at(dx, dy, fonts()).map(|t| t.align), Some(super::super::click_type::ClickAlign::Left), "further right, a tab");
}
