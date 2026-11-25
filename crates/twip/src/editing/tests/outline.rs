//! Outline levels and headings: promote, demote, a heading's content, moving, deleting and inserting headings

use super::super::Editor;
use super::super::commands::Outcome;
use super::super::outline::{Beside, NewHeading, OutlineChange};
use super::super::position::{Position, StoryId};
use super::super::selection::Selection;
use super::super::text::UnitText;
use super::style_model::{open_pkg, pkg, reopened};
use super::{fonts, texts, texts_ids};
use crate::api::command::{AuthorFilter, Command as C};
use crate::layout::{ParaId, Unit};

fn heading_style(n: u32) -> String {
    format!(
        r#"<w:style w:type="paragraph" w:styleId="Heading{n}"><w:name w:val="heading {n}"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:qFormat/><w:pPr><w:keepNext/><w:outlineLvl w:val="{}"/></w:pPr></w:style>"#,
        n - 1
    )
}

fn styles() -> String {
    let mut s = String::from(r#"<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style>"#);
    s.extend((1..=3).map(heading_style));
    s
}

fn p(style: &str, text: &str) -> String {
    let ppr = if style.is_empty() { String::new() } else { format!(r#"<w:pPr><w:pStyle w:val="{style}"/></w:pPr>"#) };
    format!("<w:p>{ppr}<w:r><w:t>{text}</w:t></w:r></w:p>")
}

fn doc(paras: &[(&str, &str)]) -> Editor {
    let body: String = paras.iter().map(|(s, t)| p(s, t)).collect();
    open_pkg(&pkg(&styles(), &body))
}

/// A Heading 1 A, its body text, Heading 2 B with a Heading 3 C under it, Heading 2 D, Heading 1 E and its body text.
fn outline() -> Editor {
    doc(&[("Heading1", "A"), ("", "a1"), ("Heading2", "B"), ("", "b1"), ("Heading3", "C"), ("Heading2", "D"), ("Heading1", "E"), ("", "e1")])
}

fn id(e: &Editor, text: &str) -> ParaId {
    texts_ids(e)
        .into_iter()
        .find(|&p| e.index().para(e.document(), p).is_some_and(|q| UnitText::of(q).text == text))
        .unwrap_or_else(|| panic!("no {text}"))
}

fn style(e: &Editor, text: &str) -> Option<String> {
    let p = e.index().para(e.document(), id(e, text))?;
    p.style.style_id.clone()
}

fn level(e: &Editor, text: &str) -> Option<u32> {
    e.outline_level(id(e, text))
}

fn caret(e: &mut Editor, text: &str) {
    e.selection = Selection::Caret(Position::text(StoryId::Body, id(e, text), Unit(0)));
}

fn changed(o: &Outcome) -> bool {
    matches!(o, Outcome::Changed(_))
}

fn marks(e: &Editor) -> Vec<crate::docx::review::MarkKind> {
    e.document().sections[0]
        .blocks
        .iter()
        .filter_map(|b| if let crate::docx::Block::Para(p) = b { Some(p) } else { None })
        .flat_map(|p| p.inlines.iter().filter_map(|i| i.run()).flat_map(|r| [&r.rev.ins, &r.rev.del].into_iter().flatten().map(|m| m.kind)))
        .collect()
}

#[test]
fn promote_and_demote_change_the_heading_style() {
    let mut e = outline();
    caret(&mut e, "B");
    assert!(changed(&e.change_outline(OutlineChange::Promote, false, fonts())));
    assert_eq!(style(&e, "B").as_deref(), Some("Heading1"));
    assert_eq!(level(&e, "C"), Some(3), "without subtopics the heading alone");
    assert!(changed(&e.change_outline(OutlineChange::Demote, false, fonts())));
    assert!(changed(&e.change_outline(OutlineChange::Demote, false, fonts())));
    assert_eq!(style(&e, "B").as_deref(), Some("Heading3"));
    e.execute(C::Undo, fonts());
    assert_eq!(style(&e, "B").as_deref(), Some("Heading2"), "each one undo step");
    caret(&mut e, "A");
    assert!(!changed(&e.change_outline(OutlineChange::Promote, false, fonts())), "Heading 1 is the top");
    caret(&mut e, "a1");
    assert!(!changed(&e.change_outline(OutlineChange::Demote, false, fonts())), "body text is the bottom");
}

#[test]
fn heading_9_stays_and_missing_heading_styles_join_the_document() {
    let mut e = outline();
    caret(&mut e, "C");
    assert!(changed(&e.change_outline(OutlineChange::Demote, false, fonts())));
    assert_eq!(style(&e, "C").as_deref(), Some("Heading4"), "the built-in style is added");
    assert!(e.document().resolver.as_ref().is_some_and(|r| r.styles.by_id.contains_key("Heading4")));
    e.execute(C::Undo, fonts());
    assert_eq!(style(&e, "C").as_deref(), Some("Heading3"));
    assert!(!e.document().resolver.as_ref().is_some_and(|r| r.styles.by_id.contains_key("Heading4")), "the style with it, one step");
    for _ in 0..10 {
        e.change_outline(OutlineChange::Demote, false, fonts());
    }
    assert_eq!((style(&e, "C").as_deref(), level(&e, "C")), (Some("Heading9"), Some(9)));
}

#[test]
fn promoted_body_text_takes_the_level_of_the_heading_before_it() {
    let mut e = outline();
    caret(&mut e, "b1");
    e.change_outline(OutlineChange::Promote, false, fonts());
    assert_eq!(style(&e, "b1").as_deref(), Some("Heading2"));
    let mut e = doc(&[("", "intro"), ("Heading1", "A")]);
    caret(&mut e, "intro");
    e.change_outline(OutlineChange::Promote, false, fonts());
    assert_eq!(style(&e, "intro").as_deref(), Some("Heading1"), "with no heading before it, Heading 1");
}

#[test]
fn subtopics_move_with_their_heading() {
    let mut e = outline();
    caret(&mut e, "B");
    assert!(changed(&e.change_outline(OutlineChange::Promote, true, fonts())));
    assert_eq!((level(&e, "B"), level(&e, "C"), level(&e, "D")), (Some(1), Some(2), Some(2)), "up to the next heading of its level");
    assert_eq!(level(&e, "b1"), None, "body text stays body text");
    e.execute(C::Undo, fonts());
    assert_eq!((level(&e, "B"), level(&e, "C")), (Some(2), Some(3)), "one undo step");
    caret(&mut e, "A");
    e.change_outline(OutlineChange::Demote, true, fonts());
    assert_eq!([level(&e, "A"), level(&e, "B"), level(&e, "C"), level(&e, "D"), level(&e, "E")], [2, 3, 4, 3, 1].map(Some));
    caret(&mut e, "A");
    assert!(changed(&e.change_outline(OutlineChange::Heading1, true, fonts())));
    assert_eq!([level(&e, "A"), level(&e, "B"), level(&e, "C")], [1, 2, 3].map(Some), "Promote to Heading 1 moves them by as many");
}

#[test]
fn a_selection_of_paragraphs_changes_together() {
    let mut e = outline();
    let (b, c) = (id(&e, "B"), id(&e, "C"));
    e.selection = Selection::range(Position::text(StoryId::Body, b, Unit(0)), Position::text(StoryId::Body, c, Unit(1)), Default::default());
    assert!(changed(&e.change_outline(OutlineChange::Demote, false, fonts())));
    assert_eq!((level(&e, "B"), level(&e, "b1"), level(&e, "C")), (Some(3), None, Some(4)));
    e.execute(C::Undo, fonts());
    assert_eq!((level(&e, "B"), level(&e, "C")), (Some(2), Some(3)), "one undo step");
}

#[test]
fn heading_1_and_body_text() {
    let mut e = outline();
    caret(&mut e, "C");
    e.change_outline(OutlineChange::Heading1, false, fonts());
    assert_eq!(style(&e, "C").as_deref(), Some("Heading1"));
    e.change_outline(OutlineChange::BodyText, false, fonts());
    assert_eq!((style(&e, "C").as_deref(), level(&e, "C")), (Some("Normal"), None));
}

/// Body text with a direct outline level: Word lists it with the headings, and changes the level where it is set.
#[test]
fn a_direct_outline_level_is_a_heading_level() {
    let body = concat!(
        r#"<w:p><w:pPr><w:outlineLvl w:val="1"/></w:pPr><w:r><w:t>direct</w:t></w:r></w:p>"#,
        r#"<w:p><w:pPr><w:outlineLvl w:val="9"/></w:pPr><w:r><w:t>body nine</w:t></w:r></w:p>"#,
        r#"<w:p><w:pPr><w:outlineLvl w:val="12"/></w:pPr><w:r><w:t>past nine</w:t></w:r></w:p>"#,
        r#"<w:p><w:pPr><w:pStyle w:val="Heading1"/><w:outlineLvl w:val="9"/></w:pPr><w:r><w:t>heading as body</w:t></w:r></w:p>"#,
        r#"<w:p><w:pPr><w:outlineLvl w:val="-3"/></w:pPr><w:r><w:t>negative</w:t></w:r></w:p>"#,
    );
    let mut e = open_pkg(&pkg(&styles(), body));
    assert_eq!(level(&e, "direct"), Some(2));
    for t in ["body nine", "past nine", "heading as body", "negative"] {
        assert_eq!(level(&e, t), None, "{t}");
    }
    let listed: Vec<(String, u32)> = crate::services::headings(&mut e).into_iter().map(|h| (h.text, h.level)).collect();
    assert_eq!(listed, [("direct".to_string(), 2)], "the headings list holds it alone");
    caret(&mut e, "direct");
    e.change_outline(OutlineChange::Promote, false, fonts());
    assert_eq!((style(&e, "direct").as_deref(), level(&e, "direct")), (Some("Normal"), Some(1)), "the level where it's set");
    caret(&mut e, "heading as body");
    e.change_outline(OutlineChange::Promote, false, fonts());
    assert_eq!(level(&e, "heading as body"), Some(1), "body text by its direct level takes the heading's level there");
    assert!(!changed(&e.execute(C::SetOutline(crate::api::command::Outline { level: Some(10), collapsed: None }), fonts())), "levels end at 9");
}

#[test]
fn a_heading_s_content_runs_to_the_next_heading_of_its_level() {
    let e = outline();
    let text = |e: &Editor, at: super::super::outline::At| {
        let blocks = &e.document().sections[at.part].blocks;
        match &blocks[at.block] {
            crate::docx::Block::Para(p) => UnitText::of(p).text,
            _ => String::new(),
        }
    };
    for (h, last) in [("A", "D"), ("B", "C"), ("C", "C"), ("E", "e1")] {
        let c = e.heading_content(id(&e, h)).expect("a heading");
        assert_eq!((text(&e, c.first), text(&e, c.last)), (h.to_string(), last.to_string()), "{h}");
    }
    assert!(e.heading_content(id(&e, "a1")).is_err(), "body text has none");
}

#[test]
fn a_table_with_a_heading_ends_content() {
    let table = r#"<w:tbl><w:tblGrid><w:gridCol w:w="2000"/></w:tblGrid><w:tr><w:tc><w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>in a cell</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#;
    let body = format!("{}{}{table}{}", p("Heading1", "A"), p("", "a1"), p("", "after"));
    let mut e = open_pkg(&pkg(&styles(), &body));
    let c = e.heading_content(id(&e, "A")).expect("a heading");
    assert_eq!(c.last.block, 1, "the table's heading ends A's content");
    assert!(e.heading_content(id(&e, "in a cell")).is_err(), "a heading in a table moves with its table");
    caret(&mut e, "in a cell");
    e.change_outline(OutlineChange::Demote, false, fonts());
    assert_eq!(level(&e, "in a cell"), Some(2), "its level changes though");
}

#[test]
fn headings_move_with_their_content() {
    let mut e = outline();
    let ids = texts_ids(&e);
    let (b, d, a, ee) = (id(&e, "B"), id(&e, "D"), id(&e, "A"), id(&e, "E"));
    assert!(changed(&e.move_heading(b, d, Beside::After, None, fonts())));
    assert_eq!(texts(&e), ["A", "a1", "D", "B", "b1", "C", "E", "e1"]);
    let mut moved = texts_ids(&e);
    moved.sort();
    let mut was = ids.clone();
    was.sort();
    assert_eq!(moved, was, "the paragraphs themselves move");
    assert_eq!(e.selection(), Selection::Caret(Position::text(StoryId::Body, b, Unit(0))), "the caret at the heading");
    e.execute(C::Undo, fonts());
    assert_eq!(texts(&e), ["A", "a1", "B", "b1", "C", "D", "E", "e1"], "one undo step");
    assert!(changed(&e.move_heading(ee, a, Beside::Before, None, fonts())));
    assert_eq!(texts(&e), ["E", "e1", "A", "a1", "B", "b1", "C", "D"], "as Move Paragraphs moves the story's last");
    e.execute(C::Undo, fonts());
    assert!(changed(&e.move_heading(a, ee, Beside::After, None, fonts())));
    assert_eq!(texts(&e), ["E", "e1", "A", "a1", "B", "b1", "C", "D"]);
}

#[test]
fn a_moved_heading_can_take_a_level() {
    let mut e = outline();
    let (b, ee) = (id(&e, "B"), id(&e, "E"));
    assert!(changed(&e.move_heading(b, ee, Beside::After, Some(1), fonts())));
    assert_eq!(texts(&e), ["A", "a1", "D", "E", "e1", "B", "b1", "C"]);
    assert_eq!((level(&e, "B"), level(&e, "C")), (Some(1), Some(2)), "its subtopics by as many");
    e.execute(C::Undo, fonts());
    assert_eq!((texts(&e)[2].as_str(), level(&e, "B"), level(&e, "C")), ("B", Some(2), Some(3)), "move and levels one undo step");
    assert!(changed(&e.move_heading(b, b, Beside::Before, Some(3), fonts())), "in place, a level change alone");
    assert_eq!(level(&e, "B"), Some(3));
}

#[test]
fn a_heading_doesn_t_move_into_its_own_content() {
    let mut e = outline();
    let (a, c) = (id(&e, "A"), id(&e, "C"));
    assert!(matches!(e.move_heading(a, c, Beside::Before, None, fonts()), Outcome::Refused(_)));
    assert!(matches!(e.move_heading(a, id(&e, "a1"), Beside::Before, None, fonts()), Outcome::Refused(_)), "body text isn't a target");
    assert_eq!(texts(&e), ["A", "a1", "B", "b1", "C", "D", "E", "e1"]);
}

#[test]
fn deleting_a_heading_deletes_its_content() {
    let mut e = outline();
    assert!(changed(&e.delete_heading(id(&e, "B"), fonts())));
    assert_eq!(texts(&e), ["A", "a1", "D", "E", "e1"]);
    e.execute(C::Undo, fonts());
    assert_eq!(texts(&e), ["A", "a1", "B", "b1", "C", "D", "E", "e1"]);
    assert!(changed(&e.delete_heading(id(&e, "A"), fonts())));
    assert_eq!(texts(&e), ["E", "e1"]);
    let ee = id(&e, "E");
    assert!(changed(&e.delete_heading(ee, fonts())));
    assert_eq!(texts(&e), [""], "the story keeps a paragraph");
    let mut e = outline();
    assert!(changed(&e.delete_heading(id(&e, "E"), fonts())));
    assert_eq!(texts(&e), ["A", "a1", "B", "b1", "C", "D", ""], "the story's last mark stays");
}

#[test]
fn new_headings_go_beside_a_heading() {
    let mut e = outline();
    let b = id(&e, "B");
    assert!(changed(&e.insert_heading(b, NewHeading::After, fonts())));
    assert_eq!(texts(&e), ["A", "a1", "B", "b1", "C", "", "D", "E", "e1"]);
    let new = texts_ids(&e)[5];
    assert_eq!((e.outline_level(new), e.selection()), (Some(2), Selection::Caret(Position::text(StoryId::Body, new, Unit(0)))));
    e.execute(C::Undo, fonts());
    assert!(changed(&e.insert_heading(b, NewHeading::Subheading, fonts())));
    assert_eq!(e.outline_level(texts_ids(&e)[5]), Some(3));
    e.execute(C::Undo, fonts());
    assert_eq!(texts(&e).len(), 8, "one undo step");
    assert!(changed(&e.insert_heading(b, NewHeading::Before, fonts())));
    assert_eq!(e.outline_level(texts_ids(&e)[2]), Some(2));
    let ee = id(&e, "E");
    assert!(changed(&e.insert_heading(ee, NewHeading::After, fonts())));
    assert_eq!(texts(&e).last().map(String::as_str), Some(""), "after the story's last paragraph");
    assert_eq!(e.outline_level(*texts_ids(&e).last().expect("a paragraph")), Some(1));
}

#[test]
fn a_tracked_heading_move_is_a_move() {
    let mut e = outline();
    e.execute(C::SetTrackChanges(true), fonts());
    let (b, d) = (id(&e, "B"), id(&e, "D"));
    assert!(changed(&e.move_heading(b, d, Beside::After, None, fonts())));
    use crate::docx::review::MarkKind;
    let m = marks(&e);
    assert!(m.contains(&MarkKind::MoveFrom) && m.contains(&MarkKind::MoveTo), "{m:?}");
    e.execute(C::Undo, fonts());
    assert_eq!(texts(&e), ["A", "a1", "B", "b1", "C", "D", "E", "e1"], "one undo step");
    assert!(marks(&e).is_empty(), "{:?}", marks(&e));
    e.execute(C::Redo, fonts());
    let m = marks(&e);
    assert!(m.contains(&MarkKind::MoveFrom) && m.contains(&MarkKind::MoveTo), "redone: {m:?}");
    let mut accepted = reopened(&e);
    accepted.execute(C::AcceptAllRevisions(AuthorFilter::default()), fonts());
    assert_eq!(texts(&accepted), ["A", "a1", "D", "B", "b1", "C", "E", "e1"]);
    e.execute(C::RejectAllRevisions(AuthorFilter::default()), fonts());
    assert_eq!(texts(&e), ["A", "a1", "B", "b1", "C", "D", "E", "e1"]);
}

#[test]
fn a_tracked_move_of_the_story_s_end_keeps_its_last_mark() {
    let mut e = outline();
    e.execute(C::SetTrackChanges(true), fonts());
    let (a, ee) = (id(&e, "A"), id(&e, "E"));
    assert!(changed(&e.move_heading(ee, a, Beside::Before, None, fonts())));
    e.execute(C::Undo, fonts());
    assert_eq!(texts(&e), ["A", "a1", "B", "b1", "C", "D", "E", "e1"], "one undo step");
    assert!(marks(&e).is_empty(), "{:?}", marks(&e));
    e.execute(C::Redo, fonts());
    assert!(!marks(&e).is_empty(), "redone");
    e.execute(C::AcceptAllRevisions(AuthorFilter::default()), fonts());
    assert_eq!(texts(&e), ["E", "e1", "A", "a1", "B", "b1", "C", "D"], "the paragraph passed ends in the story's last mark");
    let mut e = outline();
    e.execute(C::SetTrackChanges(true), fonts());
    let (a, ee) = (id(&e, "A"), id(&e, "E"));
    assert!(changed(&e.move_heading(a, ee, Beside::After, None, fonts())));
    e.execute(C::AcceptAllRevisions(AuthorFilter::default()), fonts());
    assert_eq!(texts(&e), ["E", "e1", "A", "a1", "B", "b1", "C", "D"]);
}

#[test]
fn a_tracked_heading_delete_keeps_its_text_marked() {
    let mut e = outline();
    e.execute(C::SetTrackChanges(true), fonts());
    assert!(changed(&e.delete_heading(id(&e, "B"), fonts())));
    assert_eq!(texts(&e).len(), 8, "still there, marked deleted");
    e.execute(C::Undo, fonts());
    assert!(marks(&e).is_empty(), "undone: {:?}", marks(&e));
    e.execute(C::Redo, fonts());
    assert!(!marks(&e).is_empty(), "redone");
    e.execute(C::AcceptAllRevisions(AuthorFilter::default()), fonts());
    assert_eq!(texts(&e), ["A", "a1", "D", "E", "e1"]);
}

/// Three sections: A (landscape), B (two columns' width, 10000 twips), C (the body's).
fn sections() -> Editor {
    let brk = |w: u32| {
        format!(
            r#"<w:sectPr><w:pgSz w:w="{w}" w:h="12240"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr>"#
        )
    };
    let body = format!(
        "{}<w:p><w:pPr>{}</w:pPr><w:r><w:t>a1</w:t></w:r></w:p>{}<w:p><w:pPr>{}</w:pPr><w:r><w:t>b1</w:t></w:r></w:p>{}{}",
        p("Heading1", "A"),
        brk(15840),
        p("Heading1", "B"),
        brk(10000),
        p("Heading1", "C"),
        p("", "c1"),
    );
    open_pkg(&pkg(&styles(), &body))
}

fn layout_of(e: &Editor) -> Vec<(Vec<String>, f64)> {
    e.document()
        .sections
        .iter()
        .map(|s| {
            let t = s.blocks.iter().filter_map(|b| if let crate::docx::Block::Para(p) = b { Some(UnitText::of(p).text) } else { None }).collect();
            (t, s.page_w)
        })
        .collect()
}

fn strs(v: &[(&[&str], f64)]) -> Vec<(Vec<String>, f64)> {
    v.iter().map(|(t, w)| (t.iter().map(|s| s.to_string()).collect(), *w)).collect()
}

#[test]
fn section_breaks_move_with_the_content_that_ends_in_them() {
    let mut e = sections();
    assert_eq!(layout_of(&e), strs(&[(&["A", "a1"], 792.0), (&["B", "b1"], 500.0), (&["C", "c1"], 612.0)]));
    let (a, b, c) = (id(&e, "A"), id(&e, "B"), id(&e, "C"));
    assert!(changed(&e.move_heading(b, a, Beside::Before, None, fonts())));
    assert_eq!(layout_of(&e), strs(&[(&["B", "b1"], 500.0), (&["A", "a1"], 792.0), (&["C", "c1"], 612.0)]));
    let saved = reopened(&e);
    assert_eq!(layout_of(&saved), layout_of(&e), "saved as moved");
    e.execute(C::Undo, fonts());
    assert_eq!(layout_of(&e), strs(&[(&["A", "a1"], 792.0), (&["B", "b1"], 500.0), (&["C", "c1"], 612.0)]));
    e.execute(C::Redo, fonts());
    assert_eq!(layout_of(&e), strs(&[(&["B", "b1"], 500.0), (&["A", "a1"], 792.0), (&["C", "c1"], 612.0)]), "redone");
    assert_eq!(layout_of(&reopened(&e)), layout_of(&e), "saved as redone");
    e.execute(C::Undo, fonts());
    assert!(changed(&e.move_heading(a, c, Beside::After, None, fonts())));
    assert_eq!(layout_of(&e), strs(&[(&["B", "b1"], 500.0), (&["C", "c1", "A", "a1"], 792.0), (&["",], 612.0)]));
    assert_eq!(layout_of(&reopened(&e)), layout_of(&e), "saved as moved");
    e.execute(C::Undo, fonts());
    assert!(changed(&e.move_heading(c, a, Beside::Before, None, fonts())));
    let moved = layout_of(&e);
    assert_eq!(moved, strs(&[(&["C", "c1", "A", "a1"], 792.0), (&["B", "b1"], 500.0), (&[""], 612.0)]), "the body's last paragraph stays");
    e.execute(C::Undo, fonts());
    e.execute(C::Redo, fonts());
    assert_eq!(layout_of(&e), moved, "redone");
    assert_eq!(layout_of(&reopened(&e)), moved, "saved as redone");
}

#[test]
fn deleting_a_heading_takes_the_breaks_in_its_content() {
    let mut e = sections();
    assert!(changed(&e.delete_heading(id(&e, "A"), fonts())));
    assert_eq!(layout_of(&e), strs(&[(&["B", "b1"], 500.0), (&["C", "c1"], 612.0)]));
    assert_eq!(layout_of(&reopened(&e)), layout_of(&e));
    e.execute(C::Undo, fonts());
    assert_eq!(layout_of(&e), strs(&[(&["A", "a1"], 792.0), (&["B", "b1"], 500.0), (&["C", "c1"], 612.0)]));
    e.execute(C::Redo, fonts());
    assert_eq!(layout_of(&e), strs(&[(&["B", "b1"], 500.0), (&["C", "c1"], 612.0)]), "redone");
    assert_eq!(layout_of(&reopened(&e)), layout_of(&e), "saved as redone");
    e.execute(C::Undo, fonts());
    assert!(changed(&e.delete_heading(id(&e, "C"), fonts())));
    assert_eq!(layout_of(&e), strs(&[(&["A", "a1"], 792.0), (&["B", "b1"], 500.0), (&[""], 612.0)]));
    e.execute(C::Undo, fonts());
    e.execute(C::Redo, fonts());
    assert_eq!(layout_of(&e), strs(&[(&["A", "a1"], 792.0), (&["B", "b1"], 500.0), (&[""], 612.0)]), "redone");
    assert_eq!(layout_of(&reopened(&e)), layout_of(&e), "saved as redone");
}

#[test]
fn a_locked_control_keeps_a_heading_where_it_is() {
    let locked =
        r#"<w:sdt><w:sdtPr><w:lock w:val="contentLocked"/></w:sdtPr><w:sdtContent><w:p><w:r><w:t>b1</w:t></w:r></w:p></w:sdtContent></w:sdt>"#;
    let body = format!("{}{}{}{locked}{}", p("Heading1", "A"), p("", "a1"), p("Heading1", "B"), p("Heading1", "C"));
    let mut e = open_pkg(&pkg(&styles(), &body));
    let (a, b) = (id(&e, "A"), id(&e, "B"));
    assert!(!changed(&e.delete_heading(b, fonts())));
    assert!(!changed(&e.move_heading(b, a, Beside::Before, None, fonts())));
    assert_eq!(texts(&e), ["A", "a1", "B", "b1", "C"]);
    assert!(changed(&e.move_heading(a, id(&e, "C"), Beside::Before, None, fonts())), "the control moves whole with what passes it");
    assert_eq!(texts(&e), ["B", "b1", "A", "a1", "C"]);
}

#[test]
fn a_bookmark_into_the_text_passed_keeps_its_text() {
    let body = format!(
        "{}{}{}{}",
        p("Heading1", "A"),
        r#"<w:p><w:bookmarkStart w:id="0" w:name="span"/><w:r><w:t>a1</w:t></w:r></w:p>"#,
        p("Heading1", "B"),
        r#"<w:p><w:r><w:t>b1</w:t></w:r><w:bookmarkEnd w:id="0"/></w:p>"#,
    );
    let mut e = open_pkg(&pkg(&styles(), &body));
    let (a, b) = (id(&e, "A"), id(&e, "B"));
    assert!(changed(&e.move_heading(a, b, Beside::After, None, fonts())));
    assert_eq!(texts(&e), ["B", "b1", "A", "a1"]);
    let f = e.bookmark("span").expect("the bookmark");
    let end = f.end.expect("its end");
    assert!(e.index().cmp(&Position::text(StoryId::Body, f.start.para, f.start.unit), &Position::text(StoryId::Body, end.para, end.unit)).is_le());
    assert_eq!(f.start.para, id(&e, "B"), "its start where its text was");
}

#[test]
fn heading_content_is_selected_whole() {
    let mut e = outline();
    assert!(matches!(e.select_heading_content(id(&e, "B")), Outcome::Moved));
    let (a, b) = e.selection().ordered(e.index()).expect("a range");
    assert_eq!((a.para_offset(), b.para_offset()), (Some((id(&e, "B"), Unit(0))), Some((id(&e, "C"), Unit(1)))));
}

#[test]
fn heading_of_finds_the_heading_a_paragraph_is_under() {
    let under = |e: &Editor, text: &str| e.heading_of(id(e, text), |p| crate::services::a11y::roles::heading_level(e.document(), p).is_some());
    let e = outline();
    assert_eq!(under(&e, "b1"), Some(id(&e, "B")));
    assert_eq!(under(&e, "D"), Some(id(&e, "D")));
    let e = doc(&[("", "intro"), ("Heading1", "A")]);
    assert_eq!(under(&e, "intro"), None);
}

#[test]
fn the_current_heading_is_the_listed_one_the_caret_is_under() {
    let mut e = doc(&[("", "intro"), ("Heading1", "A"), ("Heading2", ""), ("", "a1")]);
    let current = |e: &Editor| crate::services::current_heading(e).map(|h| h.0);
    caret(&mut e, "a1");
    assert_eq!(current(&e), Some(id(&e, "A").0.to_string()), "an empty heading isn't listed");
    caret(&mut e, "A");
    assert_eq!(current(&e), Some(id(&e, "A").0.to_string()));
    caret(&mut e, "intro");
    assert_eq!(current(&e), None);
}

/// A Heading 1 over 3000 Heading 2s: its subtopics, content and a move are each one pass.
#[test]
fn many_subtopics_change_together() {
    let mut paras = vec![("Heading1", "top".to_string())];
    paras.extend((0..3000).map(|k| ("Heading2", format!("h{k}"))));
    paras.push(("Heading1", "end".to_string()));
    let refs: Vec<(&str, &str)> = paras.iter().map(|(s, t)| (*s, t.as_str())).collect();
    let mut e = doc(&refs);
    caret(&mut e, "top");
    assert!(changed(&e.change_outline(OutlineChange::Demote, true, fonts())));
    assert_eq!((level(&e, "top"), level(&e, "h0"), level(&e, "h2999"), level(&e, "end")), (Some(2), Some(3), Some(3), Some(1)));
    let (top, end) = (id(&e, "top"), id(&e, "end"));
    assert!(changed(&e.move_heading(end, top, Beside::Before, None, fonts())));
    assert_eq!(texts(&e)[..2], ["end".to_string(), "top".to_string()]);
}

#[test]
fn find_results_group_by_the_heading_they_are_under() {
    let e = doc(&[("", "intro"), ("Heading1", "A"), ("", "a1"), ("Heading2", "B"), ("", "b1")]);
    let paras = [id(&e, "b1"), id(&e, "intro"), id(&e, "a1"), id(&e, "B")];
    let got: Vec<Option<String>> = crate::services::headings_over(&e, &paras).into_iter().map(|h| h.map(|h| h.0)).collect();
    let want = |t: &str| Some(id(&e, t).0.to_string());
    assert_eq!(got, [want("B"), None, want("A"), want("B")]);
    let e = doc(&[("Heading1", "A"), ("Heading1", ""), ("", "a1")]);
    let got = crate::services::headings_over(&e, &[id(&e, "a1")]);
    assert_eq!(got[0].as_ref().map(|h| h.0.clone()), Some(id(&e, "A").0.to_string()), "an empty heading isn't listed, so it doesn't group");
}

#[test]
fn a_table_moves_with_its_heading() {
    let table = r#"<w:tbl><w:tblGrid><w:gridCol w:w="2000"/></w:tblGrid><w:tr><w:tc><w:p><w:r><w:t>cell</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#;
    let body = format!("{}{}{table}{}{}", p("Heading1", "A"), p("", "a1"), p("Heading1", "B"), p("", "b1"));
    for tracked in [false, true] {
        let mut e = open_pkg(&pkg(&styles(), &body));
        e.execute(C::SetTrackChanges(tracked), fonts());
        let (a, b) = (id(&e, "A"), id(&e, "B"));
        assert!(changed(&e.move_heading(a, b, Beside::After, None, fonts())), "tracked {tracked}");
        e.execute(C::AcceptAllRevisions(AuthorFilter::default()), fonts());
        assert_eq!(texts(&e), ["B", "b1", "A", "a1", "cell", ""], "tracked {tracked}: the story ends in a paragraph");
        let blocks = &e.document().sections[0].blocks;
        assert!(matches!(blocks[blocks.len() - 2], crate::docx::Block::Table(_)), "tracked {tracked}");
    }
}

#[test]
fn a_tracked_move_across_a_section_break_leaves_the_break() {
    let mut e = sections();
    e.execute(C::SetTrackChanges(true), fonts());
    let (a, c) = (id(&e, "A"), id(&e, "C"));
    assert!(changed(&e.move_heading(c, a, Beside::Before, None, fonts())));
    e.execute(C::AcceptAllRevisions(AuthorFilter::default()), fonts());
    assert_eq!(layout_of(&e), strs(&[(&["C", "c1", "A", "a1"], 792.0), (&["B", "b1"], 500.0), (&[""], 612.0)]), "as the untracked move");
}

#[test]
fn protection_keeps_headings_where_they_are() {
    let body = format!("{}{}{}", p("Heading1", "A"), p("Heading1", "B"), r#"<w:p><w:r><w:t>b1</w:t></w:r></w:p>"#);
    let mut e = open_pkg(&pkg(&styles(), &body));
    use crate::api::command::{DocumentProtection, ProtectionKind};
    let lock = DocumentProtection { kind: Some(ProtectionKind::ReadOnly), enforce: Some(true), ..Default::default() };
    assert!(changed(&e.execute(C::SetDocumentProtection(lock), fonts())));
    let (a, b) = (id(&e, "A"), id(&e, "B"));
    assert!(!changed(&e.move_heading(b, a, Beside::Before, None, fonts())));
    assert!(!changed(&e.delete_heading(b, fonts())));
    assert!(!changed(&e.insert_heading(b, NewHeading::After, fonts())));
    assert!(!changed(&e.change_heading_outline(b, OutlineChange::Demote, true, fonts())));
    assert_eq!(texts(&e), ["A", "B", "b1"]);
    assert_eq!(level(&e, "B"), Some(1));
}

#[test]
fn a_new_heading_after_goes_in_an_editable_range_ending_with_the_content() {
    let body = format!(
        r#"<w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:permStart w:id="1" w:edGrp="everyone"/><w:r><w:t>A</w:t></w:r></w:p><w:p><w:r><w:t>a1</w:t></w:r><w:permEnd w:id="1"/></w:p>{}"#,
        p("Heading1", "B")
    );
    let mut e = open_pkg(&pkg(&styles(), &body));
    use crate::api::command::{DocumentProtection, ProtectionKind};
    let lock = DocumentProtection { kind: Some(ProtectionKind::ReadOnly), enforce: Some(true), ..Default::default() };
    assert!(changed(&e.execute(C::SetDocumentProtection(lock), fonts())));
    let a = id(&e, "A");
    assert!(changed(&e.insert_heading(a, NewHeading::After, fonts())));
    assert_eq!(texts(&e), ["A", "a1", "", "B"]);
    assert!(changed(&e.execute(C::InsertText("New".into()), fonts())), "typed at its caret, in the range");
    assert_eq!(texts(&e), ["A", "a1", "New", "B"]);
    assert!(!changed(&e.insert_heading(id(&e, "B"), NewHeading::After, fonts())), "outside the range");
}

#[test]
fn headings_in_a_text_box_move_delete_and_change_level_in_their_story() {
    use super::empty_stories::{anchored_box, build, floats};
    let inner: String = [("Heading1", "A"), ("", "a1"), ("Heading1", "B"), ("", "b1")].iter().map(|(s, t)| p(s, t)).collect();
    let body = format!(
        r#"<w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>Text</w:t></w:r>{}</w:p>"#,
        anchored_box(&format!("<w:txbxContent>{inner}</w:txbxContent>"))
    );
    let styles = format!(r#"<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">{}</w:styles>"#, styles());
    let bytes =
        crate::docx::write::repackage(&build(&body, "", &[]), &[("word/styles.xml".into(), styles.into_bytes())], zip::CompressionMethod::Deflated)
            .expect("repackages");
    let mut e = open_pkg(&bytes);
    let in_box = |e: &Editor| -> Vec<(ParaId, String)> {
        let f = floats(&e.document().sections[0].blocks);
        let blocks = f[0].text_box.as_ref().map_or(&[][..], |t| &t.blocks[..]);
        blocks.iter().filter_map(|b| if let crate::docx::Block::Para(p) = b { Some((ParaId(p.id), UnitText::of(p).text)) } else { None }).collect()
    };
    let texts_in = |e: &Editor| in_box(e).into_iter().map(|(_, t)| t).collect::<Vec<_>>();
    let find = |e: &Editor, t: &str| in_box(e).into_iter().find(|(_, s)| s == t).map(|(p, _)| p).expect("in the box");
    let (a, b) = (find(&e, "A"), find(&e, "B"));
    assert!(changed(&e.move_heading(b, a, Beside::Before, None, fonts())));
    assert_eq!(texts_in(&e), ["B", "b1", "A", "a1"]);
    assert_eq!(texts_in(&reopened(&e)), ["B", "b1", "A", "a1"], "saved as moved");
    assert_eq!(texts(&e), ["Text"], "the body stays");
    e.execute(C::Undo, fonts());
    assert_eq!(texts_in(&e), ["A", "a1", "B", "b1"]);
    assert!(changed(&e.change_heading_outline(b, OutlineChange::Demote, true, fonts())));
    assert_eq!(e.outline_level(b), Some(2));
    assert!(changed(&e.delete_heading(a, fonts())));
    assert_eq!(texts_in(&e), [""], "B under A goes with it; the story's last mark stays");
    e.execute(C::Undo, fonts());
    e.execute(C::Undo, fonts());
    assert_eq!(e.outline_level(b), Some(1));
    assert!(changed(&e.delete_heading(a, fonts())));
    assert_eq!(texts_in(&e), ["B", "b1"]);
    assert_eq!(texts_in(&reopened(&e)), ["B", "b1"], "saved as deleted");
    e.execute(C::Undo, fonts());
    assert_eq!(texts_in(&e), ["A", "a1", "B", "b1"]);
    assert_eq!(e.move_heading(b, id(&e, "Text"), Beside::Before, None, fonts()), Outcome::unavailable("the headings aren't in one story"));
}

#[test]
fn a_style_being_edited_keeps_the_pane_s_outline_off_the_document() {
    let mut e = outline();
    e.style_target = Some(String::new());
    let b = id(&e, "B");
    assert!(matches!(e.change_heading_outline(b, OutlineChange::Promote, true, fonts()), Outcome::Refused(_)));
    e.style_target = None;
    assert_eq!(level(&e, "B"), Some(2));
}

#[test]
fn a_level_change_in_place_is_a_move_heading_step() {
    let mut e = outline();
    let (b, d) = (id(&e, "B"), id(&e, "D"));
    assert!(changed(&e.move_heading(d, b, Beside::After, Some(3), fonts())));
    assert_eq!(level(&e, "D"), Some(3));
    assert_eq!(e.history().undo_labels().next(), Some("Move Heading"));
}

#[test]
fn a_move_to_where_the_heading_is_changes_nothing() {
    for track in [false, true] {
        let mut e = outline();
        e.execute(C::SetTrackChanges(track), fonts());
        let ee = id(&e, "E");
        assert_eq!(e.move_heading(ee, ee, Beside::After, None, fonts()), Outcome::Unchanged("in place already"), "tracked {track}");
        assert!(marks(&e).is_empty(), "tracked {track}: {:?}", marks(&e));
        assert_eq!(e.history().undo_labels().next(), None, "tracked {track}: no step");
        assert!(changed(&e.move_heading(ee, ee, Beside::After, Some(2), fonts())));
        assert_eq!(texts(&e), ["A", "a1", "B", "b1", "C", "D", "E", "e1"], "tracked {track}");
        assert_eq!(level(&e, "E"), Some(2));
        assert!(!marks(&e).iter().any(|m| matches!(m, crate::docx::review::MarkKind::MoveFrom | crate::docx::review::MarkKind::MoveTo)));
    }
}

#[test]
fn a_level_kept_keeps_a_custom_heading_style() {
    let custom = |id: &str, lvl: u32| {
        format!(
            r#"<w:style w:type="paragraph" w:styleId="{id}"><w:name w:val="{id}"/><w:basedOn w:val="Normal"/><w:pPr><w:outlineLvl w:val="{lvl}"/></w:pPr></w:style>"#
        )
    };
    let styles = format!("{}{}{}", styles(), custom("Appendix", 0), custom("Deep", 8));
    let body: String = [("Appendix", "X"), ("Deep", "x9"), ("Heading1", "E")].iter().map(|(s, t)| p(s, t)).collect();
    let mut e = open_pkg(&pkg(&styles, &body));
    let (x, ee) = (id(&e, "X"), id(&e, "E"));
    assert!(!changed(&e.move_heading(x, x, Beside::Before, Some(1), fonts())), "its own level");
    assert_eq!(style(&e, "X").as_deref(), Some("Appendix"));
    assert!(changed(&e.move_heading(x, ee, Beside::After, Some(1), fonts())));
    assert_eq!((style(&e, "X").as_deref(), style(&e, "x9").as_deref()), (Some("Appendix"), Some("Deep")), "moved at its own level");
    assert!(changed(&e.move_heading(x, x, Beside::Before, Some(2), fonts())));
    assert_eq!((style(&e, "X").as_deref(), level(&e, "X")), (Some("Heading2"), Some(2)));
    assert_eq!((style(&e, "x9").as_deref(), level(&e, "x9")), (Some("Deep"), Some(9)), "a subtopic at 9 stays as it is");
}

#[test]
fn a_moved_heading_s_level_change_leaves_the_headings_it_lands_before() {
    for tracked in [false, true] {
        let mut e = doc(&[("Heading1", "A"), ("Heading2", "B"), ("Heading2", "D"), ("Heading3", "C"), ("Heading1", "E")]);
        e.execute(C::SetTrackChanges(tracked), fonts());
        let (b, c) = (id(&e, "B"), id(&e, "C"));
        assert!(changed(&e.move_heading(b, c, Beside::Before, Some(1), fonts())), "tracked {tracked}");
        e.execute(C::AcceptAllRevisions(AuthorFilter::default()), fonts());
        assert_eq!(texts(&e), ["A", "D", "B", "C", "E"], "tracked {tracked}");
        let levels: Vec<_> = ["A", "D", "B", "C", "E"].iter().map(|t| level(&e, t)).collect();
        assert_eq!(levels, [Some(1), Some(2), Some(1), Some(3), Some(1)], "tracked {tracked}: only B changed");
    }
}

#[test]
fn a_selected_child_takes_heading_1_when_its_parent_already_has_it() {
    let mut e = outline();
    let (a, b) = (id(&e, "A"), id(&e, "B"));
    e.selection = Selection::range(Position::text(StoryId::Body, a, Unit(0)), Position::text(StoryId::Body, b, Unit(1)), Default::default());
    assert!(changed(&e.change_outline(OutlineChange::Heading1, true, fonts())));
    assert_eq!([level(&e, "A"), level(&e, "B"), level(&e, "C"), level(&e, "D")], [1, 1, 2, 2].map(Some), "B, and C under it by as many");
}

/// A one-row table of two cells, the first holding `first`.
fn table_of(first: &str) -> String {
    format!(
        r#"<w:tbl><w:tblPr><w:tblW w:w="2000" w:type="dxa"/></w:tblPr><w:tblGrid><w:gridCol w:w="1000"/><w:gridCol w:w="1000"/></w:tblGrid><w:tr><w:tc>{first}</w:tc><w:tc><w:p><w:r><w:t>c2</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#
    )
}

#[test]
fn a_heading_s_trailing_table_is_selected_and_copied_whole() {
    let body = format!("{}{}{}{}", p("Heading1", "A"), p("", "a1"), table_of(&p("", "c1")), p("Heading1", "B"));
    let mut e = open_pkg(&pkg(&styles(), &body));
    assert!(matches!(e.select_heading_content(id(&e, "A")), Outcome::Moved));
    let (_, end) = e.selection().ordered(e.index()).expect("a range");
    assert_eq!(end.para_offset(), Some((id(&e, "B"), Unit(0))), "up to the next heading, none of its text");
    let f = e.copy_fragment().expect("a copy");
    assert!(f.blocks.iter().any(|b| matches!(b, crate::docx::Block::Table(_))), "the table copied as a table");
    let shown: Vec<String> =
        f.blocks.iter().filter_map(|b| if let crate::docx::Block::Para(p) = b { Some(UnitText::of(p).text) } else { None }).collect();
    assert_eq!(shown, ["A", "a1", ""], "no text of B");
}

#[test]
fn selecting_a_heading_drops_the_other_ranges() {
    let mut e = outline();
    let (b, e1) = (id(&e, "B"), id(&e, "e1"));
    assert!(matches!(e.select_heading_content(b), Outcome::Moved));
    let was = e.selection;
    e.multi = super::super::multi::Multi {
        primary: Some(was),
        others: vec![(Position::text(StoryId::Body, e1, Unit(0)), Position::text(StoryId::Body, e1, Unit(1)))],
        cells: Vec::new(),
    };
    assert!(matches!(e.select_heading_content(b), Outcome::Moved), "one range now");
    assert!(e.multi.others.is_empty());
}

#[test]
fn a_tracked_move_relevels_the_headings_in_its_trailing_table() {
    let body = format!("{}{}{}{}", p("Heading1", "A"), p("Heading2", "B"), table_of(&p("Heading3", "T")), p("Heading1", "E"));
    for (side, order) in [(Beside::Before, ["B", "T", "c2", "A", "E"]), (Beside::After, ["A", "E", "B", "T", "c2"])] {
        let mut e = open_pkg(&pkg(&styles(), &body));
        e.execute(C::SetTrackChanges(true), fonts());
        let target = id(&e, if side == Beside::Before { "A" } else { "E" });
        assert!(changed(&e.move_heading(id(&e, "B"), target, side, Some(1), fonts())));
        e.execute(C::AcceptAllRevisions(AuthorFilter::default()), fonts());
        assert_eq!(texts(&e)[..5], order, "{side:?}");
        assert_eq!((level(&e, "B"), level(&e, "T")), (Some(1), Some(2)), "{side:?}: the table's heading moved a level with it");
    }
}

#[test]
fn a_move_to_level_9_leaves_a_subtopic_at_9_a_peer() {
    let styles = format!("{}{}{}", styles(), heading_style(8), heading_style(9));
    let body: String = [("Heading1", "A"), ("Heading8", "H"), ("Heading9", "h9"), ("Heading1", "E")].iter().map(|(s, t)| p(s, t)).collect();
    let mut e = open_pkg(&pkg(&styles, &body));
    let h = id(&e, "H");
    assert!(changed(&e.move_heading(h, h, Beside::Before, Some(9), fonts())));
    assert_eq!((level(&e, "H"), level(&e, "h9")), (Some(9), Some(9)), "kept within 9");
}
