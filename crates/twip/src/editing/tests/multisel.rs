//! Discontiguous and block selection

use serde_json::json;

use super::scenarios::Run;
use super::{blank_in, blank_with, fonts, texts};
use crate::api::command::Command;
use crate::editing::Editor;
use crate::editing::position::{Position, StoryId};
use crate::editing::selection::Selection;
use crate::layout::{ParaId, Unit};

fn para(e: &Editor, k: usize) -> ParaId {
    super::texts_ids(e)[k]
}

fn at(e: &Editor, k: usize, u: u32) -> Position {
    Position::text(StoryId::Body, para(e, k), Unit(u))
}

fn sel(r: &mut Run, k: usize, a: u32, b: u32) {
    let p = para(&r.e, k);
    r.e.select_range(p, Unit(a), Unit(b), fonts());
}

fn bold_text(e: &Editor, k: usize) -> String {
    let crate::docx::Block::Para(p) = &e.document().sections[0].blocks[k] else { panic!() };
    p.inlines.iter().filter_map(|i| if let crate::docx::Inline::Text(t, r) = i { r.bold.then(|| t.clone()) } else { None }).collect()
}

/// "quick" selected, then "fox" added with Ctrl and a drag.
fn two_ranges() -> Run {
    let mut r = Run::from_bytes(&blank_with("The quick brown fox jumps.\nOver the lazy dog."), "multisel");
    sel(&mut r, 0, 4, 9);
    assert_eq!(r.e.selected_text(), "quick");
    r.step(&json!({ "drag": { "from": { "para": 0, "offset": 16 }, "to": { "para": 0, "offset": 19 } }, "mods": { "ctrl": true } }));
    r
}

#[test]
fn ctrl_drag_adds_a_range() {
    let mut r = two_ranges();
    assert!(r.e.is_discontiguous(), "{:?}", r.e.selection());
    assert_eq!(r.e.selection_ranges().len(), 2);
    assert_eq!(r.e.selected_text(), "quickfox", "the ranges' text in document order, run together");
    // A plain click drops the other ranges.
    r.step(&json!({ "click": { "para": 1, "offset": 2 } }));
    assert!(!r.e.is_discontiguous());
    assert!(r.e.selection().is_collapsed());
}

#[test]
fn formatting_applies_to_every_range() {
    let mut r = two_ranges();
    r.e.execute(Command::SetBold(true), fonts());
    assert_eq!(bold_text(&r.e, 0), "quickfox");
    assert!(r.e.is_discontiguous(), "formatting keeps the selection");
    assert_eq!(r.e.format_state().map(|f| f.character.bold), Some(crate::api::state::Tri::Uniform(true)));
    r.e.execute(Command::SetItalic(true), fonts());
    r.e.execute(Command::Undo, fonts());
    assert!(r.e.is_discontiguous(), "undo puts the selection back");
    assert_eq!(bold_text(&r.e, 0), "quickfox");
    r.e.execute(Command::Undo, fonts());
    assert_eq!(bold_text(&r.e, 0), "");
}

#[test]
fn delete_removes_every_range_in_one_step() {
    let mut r = two_ranges();
    let before = texts(&r.e);
    r.e.execute(super::del(crate::editing::commands::Dir::Forward, false), fonts());
    assert_eq!(texts(&r.e)[0], "The  brown  jumps.");
    assert_eq!(r.e.selection(), Selection::Caret(at(&r.e, 0, 11)), "where the range made last was");
    r.e.execute(Command::Undo, fonts());
    assert_eq!(texts(&r.e), before);
}

#[test]
fn typing_replaces_the_last_range() {
    let mut r = two_ranges();
    r.e.execute(Command::InsertText("cat".into()), fonts());
    assert_eq!(texts(&r.e)[0], "The quick brown cat jumps.");
    assert!(!r.e.is_discontiguous());
}

#[test]
fn copy_runs_the_ranges_together() {
    let mut r = two_ranges();
    let f = r.e.copy().expect("copies");
    assert_eq!(f.text(), "quickfox");
    assert_eq!(f.blocks.len(), 1);
    r.step(&json!({ "click": { "para": 1, "offset": 0 } }));
    r.e.paste_fragment(&f, fonts());
    assert!(texts(&r.e)[1].starts_with("quickfox"), "{:?}", texts(&r.e));
}

#[test]
fn ranges_across_paragraphs_copy_and_delete() {
    let mut r = Run::from_bytes(&blank_with("one two\nthree four\nfive six"), "multisel-paras");
    sel(&mut r, 0, 4, 7);
    let e = &mut r.e;
    let (a, b) = (at(e, 1, 6), at(e, 2, 4));
    assert_eq!(e.add_selection_range(a, b), crate::editing::commands::Outcome::Moved);
    assert_eq!(e.selected_text(), "twofour\nfive");
    let f = e.copy().expect("copies");
    assert_eq!(f.text(), "twofour\nfive");
    e.execute(super::del(crate::editing::commands::Dir::Back, false), fonts());
    assert_eq!(texts(e), ["one ", "three  six"]);
}

#[test]
fn ctrl_double_click_adds_a_word_and_ctrl_click_a_sentence() {
    let doc = blank_with("First one here. Second one there. Third one.");
    let mut r = Run::from_bytes(&doc, "multisel-words");
    sel(&mut r, 0, 0, 5);
    r.step(&json!({ "click": { "para": 0, "offset": 37 }, "clicks": 2, "mods": { "ctrl": true } }));
    assert_eq!(r.e.selected_text(), "FirstThird ", "{:?}", r.e.selection_ranges());
    let mut r = Run::from_bytes(&doc, "multisel-sentence");
    sel(&mut r, 0, 0, 5);
    r.step(&json!({ "click": { "para": 0, "offset": 37 }, "mods": { "ctrl": true } }));
    assert_eq!(r.e.selected_text(), "FirstThird one.");
    // A range that overlaps another joins it.
    r.step(&json!({ "drag": { "from": { "para": 0, "offset": 8 }, "to": { "para": 0, "offset": 2 } }, "mods": { "ctrl": true } }));
    assert_eq!(r.e.selection_ranges().len(), 2);
    assert_eq!(r.e.selected_text(), "First one Third one.", "the drag takes whole words");
}

#[test]
fn ctrl_click_without_a_selection_selects_a_sentence() {
    let mut r = Run::from_bytes(&blank_with("First one here. Second one there."), "multisel-plain");
    r.step(&json!({ "click": { "para": 0, "offset": 20 }, "mods": { "ctrl": true } }));
    assert!(!r.e.is_discontiguous());
    assert_eq!(r.e.selected_text(), "Second one there.");
}

#[test]
fn the_view_highlights_every_range() {
    let mut r = two_ranges();
    let v = r.e.view(0..1);
    let crate::editing::view::SelectionView::Text(rects) = v.selection else { panic!("{:?}", v.selection) };
    assert_eq!(rects.len(), 2, "{rects:?}");
}

#[test]
fn alt_drag_selects_a_column_block() {
    let text = "abcdefgh\nijklmnop\nqrstuvwx\nyz";
    let mut r = Run::from_bytes(&blank_in(text, "Courier New"), "multisel-block");
    r.step(&json!({ "drag": { "from": { "para": 0, "offset": 2 }, "to": { "para": 2, "offset": 5 } }, "mods": { "alt": true } }));
    assert!(matches!(r.e.selection(), Selection::Block { .. }), "{:?}", r.e.selection());
    assert_eq!(r.e.selected_text(), "cde\nklm\nstu");
    assert_eq!(r.e.selection_ranges().len(), 3);
    r.e.execute(Command::SetBold(true), fonts());
    assert_eq!(bold_text(&r.e, 1), "klm");
    // A plain arrow collapses it: no block mode stays on.
    r.step(&json!({ "key": "ArrowRight" }));
    assert!(r.e.selection().is_collapsed());
}

#[test]
fn alt_click_puts_the_caret() {
    let mut r = Run::from_bytes(&blank_with("abc def"), "multisel-alt-click");
    r.step(&json!({ "click": { "para": 0, "offset": 4 }, "mods": { "alt": true } }));
    assert_eq!(r.e.selection(), Selection::Caret(at(&r.e, 0, 4)));
}

#[test]
fn shrink_steps_back_through_the_extend_levels() {
    let mut r = Run::from_bytes(&blank_with("One two three. Four five six.\nNext paragraph."), "multisel-shrink");
    r.step(&json!({ "click": { "para": 0, "offset": 21 } }));
    for _ in 0..5 {
        r.step(&json!({ "key": "F8" }));
    }
    assert_eq!(r.e.selected_text(), "One two three. Four five six.\nNext paragraph.");
    let mut seen = Vec::new();
    for _ in 0..4 {
        assert_eq!(r.e.shrink_selection(), crate::editing::commands::Outcome::Moved);
        seen.push(r.e.selected_text());
    }
    assert_eq!(seen, ["One two three. Four five six.\n", "Four five six.", "five ", ""]);
    assert_eq!(r.e.selection(), Selection::Caret(at(&r.e, 0, 21)));
    assert!(matches!(r.e.shrink_selection(), crate::editing::commands::Outcome::Unchanged(_)));
    // Still extending.
    r.step(&json!({ "key": "ArrowRight" }));
    assert_eq!(r.e.selected_text(), "i");
}

#[test]
fn shrink_needs_extend_mode() {
    let mut r = Run::from_bytes(&blank_with("abc def"), "multisel-shrink-off");
    sel(&mut r, 0, 0, 3);
    assert!(matches!(r.e.shrink_selection(), crate::editing::commands::Outcome::Unchanged(_)));
    assert_eq!(r.e.selected_text(), "abc");
}

#[test]
fn window_edges() {
    let text: Vec<String> = (0..200).map(|k| format!("Line {k}")).collect();
    let mut e = Editor::open(&blank_with(&text.join("\n")), fonts()).unwrap();
    e.set_viewport(crate::editing::input::Viewport { left: 0.0, top: 0.0, width: 800.0, height: 400.0, zoom: 1.0, dpr: 1.0 });
    e.move_to_window_edge(true, false, fonts());
    let bottom = e.selection().focus().and_then(|p| p.para_offset()).map(|(p, _)| p).unwrap();
    let k = super::texts_ids(&e).iter().position(|&p| p == bottom).unwrap();
    assert!(k > 5 && k < 40, "the last line in the window, not the document's: {k}");
    e.move_to_window_edge(false, true, fonts());
    assert!(!e.selection().is_collapsed(), "extends");
    let top = e.selection().focus().and_then(|p| p.para_offset()).map(|(p, _)| p).unwrap();
    assert_eq!(top, para(&e, 0));
}

fn styled() -> Vec<u8> {
    let r = |t: &str, rpr: &str| format!("<w:r><w:rPr>{rpr}</w:rPr><w:t xml:space=\"preserve\">{t}</w:t></w:r>");
    let styles = r#"<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style><w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:basedOn w:val="Normal"/></w:style><w:style w:type="character" w:styleId="Strong"><w:name w:val="Strong"/><w:rPr><w:b/></w:rPr></w:style>"#;
    super::style_model::pkg(
        styles,
        &format!(
            "<w:p>{}{}{}</w:p><w:p><w:pPr><w:pStyle w:val=\"Heading1\"/></w:pPr>{}</w:p><w:p>{}{}</w:p><w:p><w:pPr><w:pStyle w:val=\"Heading1\"/></w:pPr>{}</w:p><w:p><w:pPr><w:pStyle w:val=\"Heading1\"/></w:pPr>{}</w:p>",
            r("plain ", ""),
            r("bold", "<w:b/>"),
            r(" plain", ""),
            r("Head", ""),
            r("more bold", "<w:b/>"),
            r(" end", "<w:rStyle w:val=\"Strong\"/>"),
            r("Head two", ""),
            r("Head three", ""),
        ),
    )
}

#[test]
fn select_text_with_similar_formatting() {
    let mut e = Editor::open(&styled(), fonts()).unwrap();
    e.select_range(para(&e, 0), Unit(7), Unit(8), fonts());
    assert_eq!(e.select_similar_formatting(), crate::editing::commands::Outcome::Moved);
    assert_eq!(e.selected_text(), "boldmore bold");
    e.execute(Command::SetItalic(true), fonts());
    let f = e.format_state().unwrap();
    assert_eq!(f.character.italic, crate::api::state::Tri::Uniform(true));
}

#[test]
fn select_all_instances_of_a_style_and_count_them() {
    let mut e = Editor::open(&styled(), fonts()).unwrap();
    let counts = e.style_use_counts();
    assert_eq!(counts.get("Heading1"), Some(&3));
    assert_eq!(counts.get("Strong"), Some(&1));
    assert_eq!(counts.get("Normal"), Some(&2));
    assert_eq!(e.select_style_instances("Heading1"), crate::editing::commands::Outcome::Moved);
    assert_eq!(e.selection_ranges().len(), 2, "the two headings in a row are one range");
    assert_eq!(e.selected_text(), "Head\nHead two\nHead three");
    assert_eq!(e.select_style_instances("Strong"), crate::editing::commands::Outcome::Moved);
    assert_eq!(e.selected_text(), " end");
    assert!(matches!(e.select_style_instances("NoSuchStyle"), crate::editing::commands::Outcome::Refused(_)));
}

#[test]
fn select_ranges_joins_and_orders() {
    let mut e = Editor::open(&blank_with("abcdefghij"), fonts()).unwrap();
    let p = |e: &Editor, u| at(e, 0, u);
    let ranges = [(p(&e, 8), p(&e, 9)), (p(&e, 0), p(&e, 2)), (p(&e, 1), p(&e, 4)), (p(&e, 6), p(&e, 5))];
    e.select_ranges(&ranges);
    let got: Vec<(u32, u32)> = e.selection_ranges().iter().map(|(a, b)| (a.para_offset().unwrap().1.0, b.para_offset().unwrap().1.0)).collect();
    assert_eq!(got, [(0, 4), (5, 6), (8, 9)]);
    assert_eq!(e.selection().anchor(), Some(p(&e, 6)), "the last range given is the selection, its way round");
}

#[test]
fn text_points_count_scalars() {
    let mut e = Editor::open(&blank_with("a😀b"), fonts()).unwrap();
    let p = para(&e, 0);
    let end = e.text_position(p, 99).unwrap();
    assert_eq!(e.text_point(&end), Some((p, 3)));
    let b = e.text_position(p, 2).unwrap();
    assert_eq!(e.text_point(&b), Some((p, 2)));
    e.select_range(p, Unit(0), Unit(1), fonts());
}

#[test]
fn a_locked_range_keeps_every_range_from_changing() {
    let body = r#"<w:p><w:r><w:t xml:space="preserve">free text </w:t></w:r><w:sdt><w:sdtPr><w:lock w:val="contentLocked"/></w:sdtPr><w:sdtContent><w:r><w:t>locked</w:t></w:r></w:sdtContent></w:sdt></w:p>"#;
    let mut e = Editor::open(&super::blank_body(body), fonts()).unwrap();
    let before = texts(&e);
    let (a, b) = (at(&e, 0, 0), at(&e, 0, 4));
    e.select_ranges(&[(a, b)]);
    let p = para(&e, 0);
    let len = crate::editing::step::para_len(e.index().para(e.document(), p).unwrap()).0;
    e.add_selection_range(at(&e, 0, len - 4), at(&e, 0, len - 2));
    assert!(e.is_discontiguous());
    e.execute(super::del(crate::editing::commands::Dir::Forward, false), fonts());
    assert_eq!(texts(&e), before);
    e.execute(Command::SetBold(true), fonts());
    assert_eq!(bold_text(&e, 0), "");
    e.select_ranges(&[(a, b)]);
    e.execute(Command::SetBold(true), fonts());
    assert_eq!(bold_text(&e, 0), "free", "the free range alone formats");
}

fn offsets(e: &mut Editor) -> Vec<(u32, u32)> {
    e.selection_ranges().iter().map(|(a, b)| (a.para_offset().unwrap().1.0, b.para_offset().unwrap().1.0)).collect()
}

#[test]
fn ctrl_click_on_the_selection_takes_its_sentence_out() {
    let mut r = Run::from_bytes(&blank_with("One two. Three four. Five six."), "multisel-deselect-sentence");
    sel(&mut r, 0, 0, 30);
    r.step(&json!({ "click": { "para": 0, "offset": 11 }, "mods": { "ctrl": true } }));
    assert_eq!(offsets(&mut r.e), [(0, 9), (21, 30)], "the middle of the range goes, leaving two");
    assert_eq!(r.e.selected_text(), "One two. Five six.");
    // A Ctrl+double-click on a selected word takes the word out, in place of the sentence its first click took.
    let mut r = Run::from_bytes(&blank_with("One two. Three four. Five six."), "multisel-deselect-word");
    sel(&mut r, 0, 0, 30);
    r.step(&json!({ "click": { "para": 0, "offset": 16 }, "clicks": 2, "mods": { "ctrl": true } }));
    assert_eq!(r.e.selected_text(), "One two. Three . Five six.", "{:?}", offsets(&mut r.e));
}

#[test]
fn ctrl_double_click_on_a_whole_range_removes_it() {
    let mut r = two_ranges();
    r.step(&json!({ "click": { "para": 0, "offset": 6 }, "clicks": 2, "mods": { "ctrl": true } }));
    assert!(!r.e.is_discontiguous(), "{:?}", r.e.selection_ranges());
    assert_eq!(r.e.selected_text(), "fox");
}

#[test]
fn ctrl_drag_over_another_range_takes_what_it_goes_over_out() {
    let mut r = Run::from_bytes(&blank_with("abc def ghi jkl mno\nOver the lazy dog."), "multisel-deselect-drag");
    sel(&mut r, 0, 0, 19);
    let e = &mut r.e;
    let (a, b) = (at(e, 1, 0), at(e, 1, 4));
    e.add_selection_range(a, b);
    r.step(&json!({ "drag": { "from": { "para": 0, "offset": 4 }, "to": { "para": 0, "offset": 11 } }, "mods": { "ctrl": true } }));
    assert_eq!(r.e.selected_text(), "abc jkl mnoOver", "the drag takes whole words: {:?}", r.e.selection_ranges());
    assert_eq!(r.e.selection_ranges().len(), 3);
    assert_eq!(r.e.selection().ordered(r.e.index()), Some((at(&r.e, 1, 0), at(&r.e, 1, 4))), "the range made last goes on");
}

#[test]
fn ctrl_drag_inside_the_selection_still_copies() {
    let mut r = Run::from_bytes(&blank_with("abc def\nxyz"), "multisel-ctrl-copy");
    sel(&mut r, 0, 0, 3);
    r.step(&json!({ "drag": { "from": { "para": 0, "offset": 1 }, "to": { "para": 1, "offset": 3 } }, "mods": { "ctrl": true } }));
    assert_eq!(texts(&r.e), ["abc def", "xyz abc"]);
}

#[test]
fn a_cancelled_ctrl_drag_puts_the_ranges_back() {
    let mut r = two_ranges();
    let before = r.e.selection_ranges();
    r.step(&json!({ "press": { "para": 1, "offset": 0 }, "mods": { "ctrl": true } }));
    r.step(&json!({ "move_to": { "para": 1, "offset": 8 }, "mods": { "ctrl": true } }));
    assert_eq!(r.e.selection_ranges().len(), 3, "the drag shows its range with the others");
    r.send(json!({ "type": "pointer", "kind": "cancel", "x": 0, "y": 0 }));
    assert_eq!(r.e.selection_ranges(), before);
    let mut r = two_ranges();
    let before = r.e.selection_ranges();
    r.step(&json!({ "press": { "para": 0, "offset": 5 }, "mods": { "ctrl": true } }));
    r.step(&json!({ "move_to": { "para": 0, "offset": 12 }, "mods": { "ctrl": true } }));
    assert_eq!(r.e.selected_text(), "fox", "a removing drag shows what is left");
    r.send(json!({ "type": "pointer", "kind": "cancel", "x": 0, "y": 0 }));
    assert_eq!(r.e.selection_ranges(), before);
}

#[test]
fn ctrl_triple_click_adds_a_paragraph() {
    let mut r = Run::from_bytes(&blank_with("First one.\nSecond two.\nThird three."), "multisel-para");
    sel(&mut r, 0, 0, 5);
    r.step(&json!({ "click": { "para": 2, "offset": 3 }, "clicks": 3, "mods": { "ctrl": true } }));
    assert_eq!(r.e.selection_ranges().len(), 2);
    let t = r.e.selected_text();
    assert!(t.starts_with("FirstThird three.") && !t.contains("Second"), "{t:?}");
}

#[test]
fn command_drag_adds_a_range_on_a_mac() {
    let mut r = Run::from_bytes(&blank_with("The quick brown fox jumps."), "multisel-mac");
    r.e.set_mac(true);
    sel(&mut r, 0, 4, 9);
    r.step(&json!({ "drag": { "from": { "para": 0, "offset": 16 }, "to": { "para": 0, "offset": 19 } }, "mods": { "meta": true } }));
    assert_eq!(r.e.selected_text(), "quickfox");
}

#[test]
fn tracked_delete_marks_every_range() {
    let mut r = two_ranges();
    r.e.execute(Command::SetTrackChanges(true), fonts());
    r.e.execute(super::del(crate::editing::commands::Dir::Forward, false), fonts());
    let crate::docx::Block::Para(p) = &r.e.document().sections[0].blocks[0] else { panic!() };
    let deleted: Vec<&str> = p
        .inlines
        .iter()
        .filter_map(|i| if let crate::docx::Inline::Text(t, run) = i { run.rev.del.is_some().then_some(t.as_str()) } else { None })
        .collect();
    assert_eq!(deleted, ["quick", "fox"]);
    // The caret is where the same delete of the last range alone puts it.
    let mut one = Run::from_bytes(&blank_with("The quick brown fox jumps.\nOver the lazy dog."), "multisel-tracked-one");
    one.e.execute(Command::SetTrackChanges(true), fonts());
    sel(&mut one, 0, 16, 19);
    one.e.execute(super::del(crate::editing::commands::Dir::Forward, false), fonts());
    let offset = |e: &Editor| e.selection().focus().and_then(|p| p.para_offset()).map(|(_, u)| u);
    assert!(r.e.selection().is_collapsed() && one.e.selection().is_collapsed());
    assert_eq!(offset(&r.e), offset(&one.e));
}

#[test]
fn paragraph_formatting_applies_to_every_range() {
    use crate::api::value::{Alignment, StyleId};
    let mut e = Editor::open(&styled(), fonts()).unwrap();
    let (a, b) = (at(&e, 0, 0), at(&e, 0, 2));
    e.select_ranges(&[(a, b), (at(&e, 2, 0), at(&e, 2, 2))]);
    let blocks = |e: &Editor| -> Vec<crate::docx::Paragraph> {
        e.document().sections[0].blocks.iter().filter_map(|b| if let crate::docx::Block::Para(p) = b { Some(p.clone()) } else { None }).collect()
    };
    e.execute(Command::SetAlignment(Alignment::Center), fonts());
    let centred: Vec<bool> = blocks(&e).iter().map(|p| p.style.jc == crate::docx::props::Jc::Center).collect();
    assert_eq!(centred, [true, false, true, false, false]);
    e.execute(Command::SetParagraphStyle(StyleId("Heading1".into())), fonts());
    let styles: Vec<Option<String>> = blocks(&e).iter().map(|p| p.style.style_id.clone()).collect();
    assert_eq!(styles[0].as_deref(), Some("Heading1"));
    assert_eq!(styles[2].as_deref(), Some("Heading1"));
    assert_ne!(styles[1], styles[0].clone().filter(|_| false), "unchanged");
    e.execute(Command::SetList(Some(crate::api::command::ListKind::Numbered)), fonts());
    let listed: Vec<bool> = blocks(&e).iter().map(|p| p.list.is_some()).collect();
    assert_eq!(&listed[..3], [true, false, true]);
    assert!(e.is_discontiguous());
}

#[test]
fn cut_takes_every_range_in_one_step_and_redoes() {
    let mut r = two_ranges();
    let before = texts(&r.e);
    r.step(&json!({ "clipboard": "cut" }));
    assert_eq!(r.e.clipboard().map(|f| f.text()).as_deref(), Some("quickfox"));
    assert_eq!(texts(&r.e)[0], "The  brown  jumps.");
    let after = texts(&r.e);
    r.e.execute(Command::Undo, fonts());
    assert_eq!(texts(&r.e), before, "one step");
    r.e.execute(Command::Redo, fonts());
    assert_eq!(texts(&r.e), after);
}

#[test]
fn delete_of_several_ranges_redoes() {
    let mut r = two_ranges();
    r.e.execute(super::del(crate::editing::commands::Dir::Back, false), fonts());
    let after = (texts(&r.e), r.e.selection());
    r.e.execute(Command::Undo, fonts());
    r.e.execute(Command::Redo, fonts());
    assert_eq!((texts(&r.e), r.e.selection()), after);
}

#[test]
fn ranges_in_a_header_and_another_story_starting_over() {
    let mut e = super::open("FX-STORIES.docx");
    let header = e.document().stories.keys().min().cloned().expect("a header");
    let story = e.index().header_footer(&header).unwrap();
    let para = e.document().stories[&header]
        .iter()
        .find_map(|b| if let crate::docx::Block::Para(p) = b { (crate::editing::step::para_len(p).0 >= 6).then_some(ParaId(p.id)) } else { None })
        .expect("a header paragraph with text");
    let p = |u| Position::text(story, para, Unit(u));
    e.select_ranges(&[(p(0), p(2)), (p(4), p(6))]);
    assert!(e.is_discontiguous());
    e.execute(Command::SetBold(true), fonts());
    let bold: u32 = e
        .index()
        .para(e.document(), para)
        .unwrap()
        .inlines
        .iter()
        .filter_map(|i| if let crate::docx::Inline::Text(t, r) = i { r.bold.then(|| t.chars().count() as u32) } else { None })
        .sum();
    assert!(bold >= 4, "both ranges bold: {bold}");
    let body = Position::text(StoryId::Body, para_id_of_body(&e), Unit(0));
    e.add_selection_range(body, Position::text(StoryId::Body, para_id_of_body(&e), Unit(1)));
    assert!(!e.is_discontiguous(), "a range in another story starts over");
    assert_eq!(e.selection().focus().map(|f| f.story), Some(StoryId::Body));
}

fn para_id_of_body(e: &Editor) -> ParaId {
    super::texts_ids(e).into_iter().find(|&p| e.index().para(e.document(), p).is_some_and(|q| crate::editing::step::para_len(q).0 > 1)).unwrap()
}

#[test]
fn typing_over_the_free_last_range_beside_a_locked_one() {
    let body = r#"<w:p><w:r><w:t xml:space="preserve">free text </w:t></w:r><w:sdt><w:sdtPr><w:lock w:val="contentLocked"/></w:sdtPr><w:sdtContent><w:r><w:t>locked</w:t></w:r></w:sdtContent></w:sdt></w:p>"#;
    let mut e = Editor::open(&super::blank_body(body), fonts()).unwrap();
    let p = para(&e, 0);
    let len = crate::editing::step::para_len(e.index().para(e.document(), p).unwrap()).0;
    e.select_ranges(&[(at(&e, 0, len - 4), at(&e, 0, len - 2)), (at(&e, 0, 0), at(&e, 0, 4))]);
    assert!(e.is_discontiguous());
    e.execute(Command::InsertText("X".into()), fonts());
    assert!(texts(&e)[0].starts_with("X text"), "{:?}", texts(&e));
}

#[test]
fn other_ranges_follow_the_edit_maps() {
    use crate::editing::step::Map;
    let mut e = Editor::open(&blank_with("abcdefghij\nklmnop"), fonts()).unwrap();
    let (p0, p1) = (para(&e, 0), para(&e, 1));
    e.select_ranges(&[(at(&e, 0, 4), at(&e, 0, 9)), (at(&e, 1, 0), at(&e, 1, 2))]);
    let second = ParaId(p0.0 + 1000);
    e.map_multi(&[
        Map::Same,
        Map::Text { para: p0, start: Unit(0), old_end: Unit(0), new_end: Unit(3) },
        Map::Text { para: p1, start: Unit(0), old_end: Unit(0), new_end: Unit(5) },
        Map::Split { para: p0, at: Unit(8), len: Unit(13), second },
        Map::Text { para: second, start: Unit(0), old_end: Unit(0), new_end: Unit(1) },
    ]);
    let others = e.other_ranges();
    assert_eq!(others, [(Position::text(StoryId::Body, p0, Unit(7)), Position::text(StoryId::Body, second, Unit(5)))]);
}

#[test]
fn text_ranges_read_each_paragraph_once_and_cap() {
    let mut e = Editor::open(&blank_with("abcdefghij"), fonts()).unwrap();
    e.select_ranges(&[(at(&e, 0, 0), at(&e, 0, 1)), (at(&e, 0, 3), at(&e, 0, 4)), (at(&e, 0, 6), at(&e, 0, 8))]);
    let p = para(&e, 0);
    let (all, whole) = e.selection_text_ranges(10);
    assert!(whole);
    assert_eq!(all, [((p, 0), (p, 1)), ((p, 3), (p, 4)), ((p, 6), (p, 8))]);
    let (some, whole) = e.selection_text_ranges(2);
    assert!(!whole);
    assert_eq!(some.len(), 2);
}

#[test]
fn shrink_goes_around_the_anchor_once_the_selection_left_the_start() {
    let mut r = Run::from_bytes(&blank_with("One two three. Four five six."), "multisel-shrink-moved");
    r.step(&json!({ "click": { "para": 0, "offset": 5 } }));
    r.step(&json!({ "key": "F8" }));
    r.step(&json!({ "key": "F8" }));
    assert_eq!(r.e.selected_text(), "two ");
    for _ in 0..5 {
        r.step(&json!({ "key": "ArrowLeft" }));
    }
    assert_eq!(offsets(&mut r.e), [(3, 4)], "off where extending started");
    assert_eq!(r.e.shrink_selection(), crate::editing::commands::Outcome::Moved);
    assert_eq!(r.e.selection(), Selection::Caret(at(&r.e, 0, 4)), "the anchor, not the start it left");
}

/// "quick" and "fox" (the range made last) dragged from inside "fox" to `to`, with Ctrl for a copy.
fn drag_both(to: u32, ctrl: bool) -> Run {
    let mut r = two_ranges();
    r.step(&json!({ "drag": { "from": { "para": 0, "offset": 17 }, "to": { "para": 0, "offset": to } }, "mods": { "ctrl": ctrl } }));
    r
}

#[test]
fn dragging_a_discontiguous_selection_moves_every_range() {
    for (to, want) in [(1, "T quickfox he  brown  jumps."), (12, "The  br quickfox own  jumps."), (25, "The  brown  jumps quickfox.")] {
        let mut r = drag_both(to, false);
        assert_eq!(texts(&r.e)[0], want, "dropped at {to}, with smart paste's spaces");
        assert!(!r.e.is_discontiguous());
        assert_eq!(r.e.selected_text(), "quickfox", "the text dropped is selected");
        r.e.execute(Command::Undo, fonts());
        assert_eq!(texts(&r.e)[0], "The quick brown fox jumps.", "one step");
    }
}

#[test]
fn ctrl_dragging_a_discontiguous_selection_copies_every_range() {
    let r = drag_both(1, true);
    assert_eq!(texts(&r.e)[0], "T quickfox he quick brown fox jumps.");
    let r = drag_both(25, true);
    assert_eq!(texts(&r.e)[0], "The quick brown fox jumps quickfox.");
}

#[test]
fn a_drop_on_another_range_changes_nothing() {
    let r = drag_both(6, false);
    assert_eq!(texts(&r.e)[0], "The quick brown fox jumps.");
}

#[test]
fn a_change_of_case_that_changes_the_length_keeps_the_ranges() {
    let mut e = Editor::open(&blank_with("straße and end."), fonts()).unwrap();
    e.select_ranges(&[(at(&e, 0, 0), at(&e, 0, 6)), (at(&e, 0, 11), at(&e, 0, 14))]);
    let out = e.execute(Command::ChangeCase(crate::api::command::CaseChange::Upper), fonts());
    assert!(matches!(out, crate::editing::commands::Outcome::Changed(_)), "{out:?}");
    assert_eq!(texts(&e)[0], "STRASSE and END.");
    assert!(e.is_discontiguous(), "{:?}", e.selection());
    assert_eq!(e.selected_text(), "STRASSEEND");
}

#[test]
fn alt_drag_on_the_selection_selects_a_block() {
    let mut r = Run::from_bytes(&blank_with("abcdef\nghijkl\nmnopqr"), "multisel-alt-on-selection");
    sel(&mut r, 0, 0, 6);
    r.step(&json!({ "drag": { "from": { "para": 0, "offset": 1 }, "to": { "para": 2, "offset": 3 } }, "mods": { "alt": true } }));
    assert!(matches!(r.e.selection(), Selection::Block { .. }), "{:?}", r.e.selection());
    assert_eq!(texts(&r.e), ["abcdef", "ghijkl", "mnopqr"], "nothing moved");
}

/// "free in out" under read-only protection, "in" an editing permission's range, with "free" and "in" (the range
/// made last) selected.
fn protected_ranges() -> Editor {
    use crate::api::command::{DocumentProtection, ProtectionKind};
    let run = |t: &str| format!(r#"<w:r><w:t xml:space="preserve">{t}</w:t></w:r>"#);
    let body = format!(r#"<w:p>{}<w:permStart w:id="1" w:edGrp="everyone"/>{}<w:permEnd w:id="1"/>{}</w:p>"#, run("free "), run("in"), run(" out"));
    let mut e = Editor::open(&super::blank_body(&body), fonts()).unwrap();
    let p = DocumentProtection { kind: Some(ProtectionKind::ReadOnly), enforce: Some(true), ..Default::default() };
    assert!(matches!(e.execute(Command::SetDocumentProtection(p), fonts()), crate::editing::commands::Outcome::Changed(_)));
    e.select_ranges(&[(at(&e, 0, 0), at(&e, 0, 4)), (at(&e, 0, 5), at(&e, 0, 7))]);
    assert!(e.is_discontiguous());
    e
}

#[test]
fn protection_lets_an_edit_of_the_last_range_alone_through() {
    use crate::api::command::CommandId as C;
    let mut e = protected_ranges();
    let f = e.format_state();
    let states = e.command_states(f.as_ref(), crate::editing::state::StateFlags { read_only: false });
    for (id, allowed) in [(C::InsertText, true), (C::Paste, true), (C::PasteText, true), (C::Delete, false), (C::SetBold, false)] {
        assert_eq!(e.protection_refuses(id).is_none(), allowed, "{id:?}");
        assert_eq!(states[&id].enabled, allowed, "{id:?}: the state agrees");
    }
    let before = texts(&e);
    assert!(!matches!(e.execute(super::del(crate::editing::commands::Dir::Forward, false), fonts()), crate::editing::commands::Outcome::Changed(_)));
    assert!(!matches!(e.execute(Command::SetBold(true), fonts()), crate::editing::commands::Outcome::Changed(_)));
    assert_eq!(texts(&e), before);
    assert_eq!(bold_text(&e, 0), "");
    let out = e.execute(Command::InsertText("X".into()), fonts());
    assert!(matches!(out, crate::editing::commands::Outcome::Changed(_)), "{out:?}");
    assert_eq!(texts(&e)[0], "free X out");
    let mut e = protected_ranges();
    let out = e.execute(Command::PasteText("Y".into()), fonts());
    assert!(matches!(out, crate::editing::commands::Outcome::Changed(_)), "{out:?}");
    assert_eq!(texts(&e)[0], "free Y out");
}

#[test]
fn a_context_press_on_any_range_keeps_the_selection() {
    let mut r = two_ranges();
    let (x, y) = r.point(&json!({ "para": 0, "offset": 6 }), 0.0, 0.0);
    r.e.context_press(x, y, fonts());
    assert!(r.e.is_discontiguous(), "inside the first range: {:?}", r.e.selection());
    assert_eq!(r.e.selected_text(), "quickfox");
    let (x, y) = r.point(&json!({ "para": 0, "offset": 12 }), 0.0, 0.0);
    r.e.context_press(x, y, fonts());
    assert!(r.e.selection().is_collapsed(), "outside every range");
}

#[test]
fn a_drag_from_any_range_moves_every_range() {
    let mut r = two_ranges();
    r.step(&json!({ "drag": { "from": { "para": 0, "offset": 6 }, "to": { "para": 0, "offset": 25 } } }));
    assert_eq!(texts(&r.e)[0], "The  brown  jumps quickfox.");
    assert_eq!(r.e.selected_text(), "quickfox");
    // A plain click on another range puts the caret there, as on the range made last.
    let mut r = two_ranges();
    r.step(&json!({ "click": { "para": 0, "offset": 6 } }));
    assert_eq!(r.e.selection(), Selection::Caret(at(&r.e, 0, 6)));
    // A Ctrl+click on it takes its sentence out.
    let mut r = Run::from_bytes(&blank_with("One two. Three four. Five six."), "multisel-deselect-other");
    sel(&mut r, 0, 0, 7);
    let (a, b) = (at(&r.e, 0, 21), at(&r.e, 0, 30));
    r.e.add_selection_range(a, b);
    r.step(&json!({ "click": { "para": 0, "offset": 3 }, "mods": { "ctrl": true } }));
    assert_eq!(r.e.selected_text(), "Five six.", "{:?}", offsets(&mut r.e));
}

/// "Hi " then a plain text control showing its placeholder "Click here", then " and more".
fn with_placeholder() -> Editor {
    let body = "<w:p><w:r><w:t xml:space=\"preserve\">Hi </w:t></w:r><w:sdt><w:sdtPr><w:id w:val=\"5\"/><w:showingPlcHdr/><w:text/></w:sdtPr><w:sdtContent><w:r><w:t>Click here</w:t></w:r></w:sdtContent></w:sdt><w:r><w:t xml:space=\"preserve\"> and more</w:t></w:r></w:p>";
    let e = Editor::open(&super::blank_body(body), fonts()).expect("opens");
    assert_eq!(texts(&e)[0], "Hi Click here and more");
    e
}

#[test]
fn a_delete_keeps_a_range_within_a_placeholder() {
    use crate::editing::commands::{Dir, Outcome};
    let mut e = with_placeholder();
    e.select_ranges(&[(at(&e, 0, 4), at(&e, 0, 8)), (at(&e, 0, 18), at(&e, 0, 22))]);
    assert!(matches!(e.execute(super::del(Dir::Forward, false), fonts()), Outcome::Changed(_)));
    assert_eq!(texts(&e)[0], "Hi Click here and ", "the placeholder stays");
    let mut e = with_placeholder();
    e.select_ranges(&[(at(&e, 0, 18), at(&e, 0, 22)), (at(&e, 0, 4), at(&e, 0, 8))]);
    assert!(matches!(e.execute(super::del(Dir::Back, false), fonts()), Outcome::Changed(_)));
    assert_eq!(texts(&e)[0], "Hi Click here and ");
    assert_eq!(e.selection(), Selection::Caret(at(&e, 0, 4)), "where the range made last was");
    let mut e = with_placeholder();
    e.select_ranges(&[(at(&e, 0, 3), at(&e, 0, 5)), (at(&e, 0, 7), at(&e, 0, 9))]);
    assert_eq!(e.execute(super::del(Dir::Forward, false), fonts()), Outcome::Unchanged("a placeholder"));
    assert_eq!(texts(&e)[0], "Hi Click here and more");
}

/// "Intro text here.", a table of two rows of three cells (a1 b1 c1 / a2 b2 c2), "After.": paragraphs 0, 1-6, 7.
fn with_table() -> Run {
    let cell = |t: &str| format!("<w:tc><w:tcPr><w:tcW w:w=\"2000\" w:type=\"dxa\"/></w:tcPr><w:p><w:r><w:t>{t}</w:t></w:r></w:p></w:tc>");
    let row = |r: u32| format!("<w:tr>{}{}{}</w:tr>", cell(&format!("a{r}")), cell(&format!("b{r}")), cell(&format!("c{r}")));
    let grid = "<w:gridCol w:w=\"2000\"/>".repeat(3);
    let body = format!(
        "<w:p><w:r><w:t>Intro text here.</w:t></w:r></w:p><w:tbl><w:tblPr><w:tblW w:w=\"6000\" w:type=\"dxa\"/></w:tblPr><w:tblGrid>{grid}</w:tblGrid>{}{}</w:tbl><w:p><w:r><w:t>After.</w:t></w:r></w:p>",
        row(1),
        row(2)
    );
    Run::from_bytes(&super::blank_body(&body), "multisel-table")
}

fn table_cell(e: &Editor, r: usize, c: usize) -> &crate::docx::Cell {
    let crate::docx::Block::Table(t) = &e.document().sections[0].blocks[1] else { panic!("no table") };
    &t.rows[r].cells[c]
}

fn cell_bold(e: &Editor, r: usize, c: usize) -> bool {
    let crate::docx::Block::Para(p) = &table_cell(e, r, c).blocks[0] else { panic!() };
    p.inlines.iter().any(|i| matches!(i, crate::docx::Inline::Text(_, run) if run.bold))
}

fn shaded(e: &Editor) -> Vec<(usize, usize)> {
    (0..2).flat_map(|r| (0..3).map(move |c| (r, c))).filter(|&(r, c)| table_cell(e, r, c).shading.is_some()).collect()
}

fn block(e: &Editor) -> Option<(usize, usize, usize, usize)> {
    bounds_of(e.selection())
}

fn bounds_of(s: Selection) -> Option<(usize, usize, usize, usize)> {
    match s {
        Selection::Cells { anchor, focus, .. } => {
            Some((anchor.row.min(focus.row), anchor.col.min(focus.col), anchor.row.max(focus.row), anchor.col.max(focus.col)))
        }
        _ => None,
    }
}

fn red() -> Command {
    Command::SetCellShading(Some(crate::api::value::Color(0xFF0000)))
}

#[test]
fn ctrl_drag_across_cells_adds_a_block_of_cells() {
    let mut r = with_table();
    sel(&mut r, 0, 6, 10);
    r.step(&json!({ "drag": { "from": { "para": 1, "offset": 0 }, "to": { "para": 5, "offset": 1 } }, "mods": { "ctrl": true } }));
    assert_eq!(block(&r.e), Some((0, 0, 1, 1)), "{:?}", r.e.selection());
    assert!(r.e.is_discontiguous());
    assert_eq!(r.e.selection_ranges(), [(at(&r.e, 0, 6), at(&r.e, 0, 10))], "the text range stays");
    let crate::editing::view::SelectionView::Text(rects) = r.e.view(0..1).selection else { panic!() };
    assert_eq!(rects.len(), 5, "the range and the four cells are highlighted: {rects:?}");
    r.e.execute(Command::SetBold(true), fonts());
    assert_eq!(bold_text(&r.e, 0), "text");
    assert!(cell_bold(&r.e, 0, 0) && cell_bold(&r.e, 0, 1) && cell_bold(&r.e, 1, 0) && cell_bold(&r.e, 1, 1));
    assert!(!cell_bold(&r.e, 0, 2) && !cell_bold(&r.e, 1, 2));
    assert!(r.e.is_discontiguous(), "formatting keeps the selection");
    assert!(matches!(r.e.execute(red(), fonts()), crate::editing::commands::Outcome::Changed(_)));
    assert_eq!(shaded(&r.e), [(0, 0), (0, 1), (1, 0), (1, 1)], "cell shading goes on every selected cell");
    assert!(r.e.is_discontiguous());
    r.e.execute(Command::Undo, fonts());
    assert!(shaded(&r.e).is_empty(), "in one undo step");
}

#[test]
fn ctrl_adds_cells_to_selected_cells() {
    let mut r = with_table();
    r.step(&json!({ "drag": { "from": { "para": 1, "offset": 0 }, "to": { "para": 4, "offset": 1 } } }));
    assert_eq!(block(&r.e), Some((0, 0, 1, 0)));
    r.step(&json!({ "drag": { "from": { "para": 3, "offset": 0 }, "to": { "para": 6, "offset": 1 } }, "mods": { "ctrl": true } }));
    assert_eq!(block(&r.e), Some((0, 2, 1, 2)), "the block made last is the selection");
    assert_eq!(r.e.cell_blocks().len(), 2);
    r.e.execute(red(), fonts());
    assert_eq!(shaded(&r.e), [(0, 0), (0, 2), (1, 0), (1, 2)]);
    // A Ctrl+click in text with cells selected adds the sentence.
    r.step(&json!({ "click": { "para": 7, "offset": 2 }, "mods": { "ctrl": true } }));
    assert_eq!(r.e.selection_ranges().len(), 1);
    assert_eq!(r.e.cell_blocks().len(), 2, "the cells stay selected");
    assert_eq!(r.e.selected_text(), "After.");
}

#[test]
fn ctrl_click_in_the_selection_zone_adds_cells() {
    let mut r = with_table();
    sel(&mut r, 0, 0, 5);
    r.step(&json!({ "click": { "table": 0, "row": 1, "border": 2, "dx_px": 5 }, "mods": { "ctrl": true } }));
    assert_eq!(block(&r.e), Some((1, 2, 1, 2)), "{:?}", r.e.selection());
    assert_eq!(r.e.selection_ranges().len(), 1, "with the text range before it");
    r.e.execute(red(), fonts());
    assert_eq!(shaded(&r.e), [(1, 2)]);
}

#[test]
fn ctrl_click_on_a_selected_cell_takes_it_out() {
    let mut r = with_table();
    sel(&mut r, 0, 0, 5);
    r.step(&json!({ "drag": { "from": { "para": 1, "offset": 0 }, "to": { "para": 5, "offset": 1 } }, "mods": { "ctrl": true } }));
    r.step(&json!({ "click": { "para": 1, "offset": 2 }, "mods": { "ctrl": true } }));
    let mut blocks: Vec<_> = r.e.cell_blocks().into_iter().filter_map(bounds_of).collect();
    blocks.sort();
    assert_eq!(blocks, [(0, 1, 0, 1), (1, 0, 1, 1)], "the rest of the block");
    assert_eq!(r.e.selection_ranges().len(), 1);
}

#[test]
fn ctrl_drag_from_body_text_into_a_table_adds_whole_rows() {
    let mut r = with_table();
    sel(&mut r, 0, 0, 5);
    r.step(&json!({ "drag": { "from": { "para": 0, "offset": 11 }, "to": { "para": 2, "offset": 1 } }, "mods": { "ctrl": true } }));
    let ranges = r.e.selection_ranges();
    assert_eq!(ranges.len(), 2, "{ranges:?}");
    assert_eq!(ranges[1].0, at(&r.e, 0, 11));
    assert!(ranges[1].1.para_offset().is_none(), "to the first row's end, as a plain drag goes");
    r.e.execute(Command::SetBold(true), fonts());
    assert_eq!(bold_text(&r.e, 0), "Introhere.");
    assert!(cell_bold(&r.e, 0, 0) && cell_bold(&r.e, 0, 2) && !cell_bold(&r.e, 1, 0));
}

#[test]
fn delete_and_copy_take_cells_and_ranges() {
    let mut r = with_table();
    sel(&mut r, 0, 6, 11);
    r.step(&json!({ "drag": { "from": { "para": 3, "offset": 0 }, "to": { "para": 6, "offset": 1 } }, "mods": { "ctrl": true } }));
    let f = r.e.copy().expect("copies");
    assert_eq!(f.blocks.len(), 2, "the range's text, then the cells as a table");
    assert!(matches!(f.blocks[1], crate::docx::Block::Table(_)) && f.cells.is_none());
    let before = texts(&r.e);
    r.e.execute(super::del(crate::editing::commands::Dir::Forward, false), fonts());
    let now = texts(&r.e);
    assert_eq!(now[0], "Intro here.");
    assert_eq!((now[3].as_str(), now[6].as_str(), now[2].as_str()), ("", "", "b1"), "the cells cleared, the others kept");
    r.e.execute(Command::Undo, fonts());
    assert_eq!(texts(&r.e), before, "in one undo step");
    // Cut with the block of cells made last: every piece, not the block's rows.
    sel(&mut r, 0, 6, 11);
    r.step(&json!({ "drag": { "from": { "para": 3, "offset": 0 }, "to": { "para": 6, "offset": 1 } }, "mods": { "ctrl": true } }));
    r.e.execute(Command::Cut, fonts());
    assert_eq!(texts(&r.e), now, "as Delete, and the table keeps its rows");
}

fn refused(o: crate::editing::commands::Outcome) -> bool {
    matches!(o, crate::editing::commands::Outcome::Refused(e) if e.code == crate::api::error::ErrorCode::Unavailable)
}

fn enabled(e: &mut Editor, id: crate::api::command::CommandId) -> bool {
    let f = e.format_state();
    e.command_states(f.as_ref(), crate::editing::state::StateFlags { read_only: false })[&id].enabled
}

#[test]
fn commands_on_one_place_are_unavailable_for_multiple_selections() {
    use crate::api::command::CommandId as C;
    let mut r = two_ranges();
    for id in [C::AddComment, C::InsertBookmark] {
        assert!(!enabled(&mut r.e, id), "{id:?}");
    }
    assert!(refused(r.e.execute(Command::AddComment(crate::api::review::Body::Text("Note".into())), fonts())));
    assert!(refused(r.e.execute(Command::InsertBookmark("here".into()), fonts())));
    assert!(r.e.is_discontiguous(), "the selection stays");
    assert!(r.e.document().comments.by_id.is_empty() && r.e.bookmarks().is_empty());
    let mut r = with_table();
    sel(&mut r, 0, 0, 5);
    r.step(&json!({ "drag": { "from": { "para": 1, "offset": 0 }, "to": { "para": 2, "offset": 1 } }, "mods": { "ctrl": true } }));
    assert!(!enabled(&mut r.e, C::InsertRows) && !enabled(&mut r.e, C::MergeCells));
    assert!(enabled(&mut r.e, C::SetCellShading), "cell formatting goes on every block");
    sel(&mut r, 0, 0, 5);
    r.step(&json!({ "drag": { "from": { "para": 1, "offset": 0 }, "to": { "para": 2, "offset": 1 } }, "mods": { "ctrl": true } }));
    let crate::docx::Block::Table(t) = &r.e.document().sections[0].blocks[1] else { panic!() };
    let rows = t.rows.len();
    assert!(refused(
        r.e.execute(Command::InsertRows(crate::api::command::InsertRows { side: crate::api::command::VerticalSide::Below, count: None }), fonts())
    ));
    let crate::docx::Block::Table(t) = &r.e.document().sections[0].blocks[1] else { panic!() };
    assert_eq!(t.rows.len(), rows);
}

#[test]
fn accept_in_the_selection_takes_every_range() {
    let mut r = Run::from_bytes(&blank_with("The quick brown fox jumps."), "multisel-accept");
    r.e.execute(Command::SetTrackChanges(true), fonts());
    for (a, b) in [(4, 9), (10, 15), (16, 19)] {
        sel(&mut r, 0, a, b);
        r.e.execute(super::del(crate::editing::commands::Dir::Forward, false), fonts());
    }
    let ranges = [(at(&r.e, 0, 4), at(&r.e, 0, 9)), (at(&r.e, 0, 16), at(&r.e, 0, 19))];
    r.e.select_ranges(&ranges);
    assert!(r.e.is_discontiguous());
    r.e.execute(Command::AcceptRevisionsInSelection, fonts());
    let crate::docx::Block::Para(p) = &r.e.document().sections[0].blocks[0] else { panic!() };
    let text: String = p.inlines.iter().filter_map(|i| if let crate::docx::Inline::Text(t, _) = i { Some(t.as_str()) } else { None }).collect();
    let deleted: Vec<&str> = p
        .inlines
        .iter()
        .filter_map(|i| if let crate::docx::Inline::Text(t, run) = i { run.rev.del.is_some().then_some(t.as_str()) } else { None })
        .collect();
    assert_eq!(deleted, ["brown"], "the change between the ranges stays");
    assert_eq!(text, "The  brown  jumps.");
}

#[test]
fn an_editable_range_goes_on_every_range() {
    let mut r = two_ranges();
    r.e.execute(Command::AddEditableRange(Default::default()), fonts());
    let crate::docx::Block::Para(p) = &r.e.document().sections[0].blocks[0] else { panic!() };
    let perms: Vec<_> = p.inlines.iter().filter_map(|i| if let crate::docx::Inline::Opaque(o) = i { o.permission() } else { None }).collect();
    assert_eq!(perms.len(), 4, "a start and an end on each range: {perms:?}");
    r.e.execute(Command::RemoveEditableRanges, fonts());
    let crate::docx::Block::Para(p) = &r.e.document().sections[0].blocks[0] else { panic!() };
    assert!(!p.inlines.iter().any(|i| matches!(i, crate::docx::Inline::Opaque(o) if o.permission().is_some())), "and both go again");
}

#[test]
fn repeat_of_a_command_on_one_place_is_unavailable_for_multiple_selections() {
    use crate::api::command::CommandId as C;
    let mut r = with_table();
    let rows = |e: &Editor| match &e.document().sections[0].blocks[1] {
        crate::docx::Block::Table(t) => t.rows.len(),
        _ => panic!("no table"),
    };
    r.step(&json!({ "click": { "para": 1, "offset": 0 } }));
    let insert = Command::InsertRows(crate::api::command::InsertRows { side: crate::api::command::VerticalSide::Below, count: None });
    assert!(matches!(r.e.execute(insert, fonts()), crate::editing::commands::Outcome::Changed(_)));
    assert_eq!(rows(&r.e), 3);
    assert!(enabled(&mut r.e, C::Repeat));
    sel(&mut r, 0, 0, 5);
    r.step(&json!({ "drag": { "from": { "para": 1, "offset": 0 }, "to": { "para": 1, "offset": 1 } }, "mods": { "ctrl": true } }));
    assert!(r.e.is_discontiguous());
    assert_eq!(block(&r.e), Some((0, 0, 0, 0)), "the last-made piece is a cell of the table");
    assert!(!enabled(&mut r.e, C::Repeat));
    assert!(refused(r.e.execute(Command::Repeat, fonts())));
    assert_eq!(rows(&r.e), 3);
    assert!(r.e.is_discontiguous(), "the selection stays");
}

#[test]
fn ranges_and_cells_in_different_stories_are_refused() {
    use super::empty_stories::{NS, build};
    let hdr = format!(
        r#"<w:hdr {NS}><w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w="2000"/></w:tblGrid><w:tr><w:tc><w:p><w:r><w:t>h</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p/></w:hdr>"#
    );
    let bytes = build(
        "<w:p><w:r><w:t>Body text</w:t></w:r></w:p>",
        r#"<w:headerReference w:type="default" r:id="rIdP0"/>"#,
        &[("header1.xml", "header", "header", &hdr)],
    );
    let mut e = Editor::open(&bytes, fonts()).expect("opens");
    e.complete(fonts());
    let table = e
        .document()
        .stories
        .values()
        .flatten()
        .find_map(|b| if let crate::docx::Block::Table(t) = b { Some(crate::layout::TableId(t.id)) } else { None })
        .expect("a header table");
    let cells = Selection::Cells {
        table,
        anchor: crate::editing::selection::CellRef { row: 0, col: 0 },
        focus: crate::editing::selection::CellRef { row: 0, col: 0 },
        shape: crate::editing::selection::CellShape::Rect,
    };
    let body = para_id_of_body(&e);
    let range = (Position::text(StoryId::Body, body, Unit(0)), Position::text(StoryId::Body, body, Unit(4)));
    let before = e.selection();
    let invalid = |o| matches!(o, crate::editing::commands::Outcome::Refused(e) if e.code == crate::api::error::ErrorCode::InvalidArgument);
    assert!(invalid(e.select_pieces(&[range], vec![cells], None)));
    assert!(invalid(e.select_pieces(&[range], Vec::new(), Some(cells))));
    assert_eq!(e.selection(), before);
    assert!(!e.is_discontiguous());
}
