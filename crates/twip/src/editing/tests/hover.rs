//! Hover tooltips

use super::super::Editor;
use super::super::hover::HoverTip;
use super::super::input::PointerKind;
use super::super::keymap::Mods;
use super::super::position::{Position, StoryId};
use super::super::revisions::index::RevisionKind;
use super::{fonts, open};
use crate::docx::{Block, Inline};
use crate::layout::{ParaId, Unit};

/// Hovers the character `after` units past the first body inline `want` takes, and returns the tooltip there.
fn hover_after(name: &str, want: impl Fn(&Inline) -> bool, after: u32) -> Option<(HoverTip, String)> {
    tip(&mut hovered(name, want, after))
}

/// [`hover_after`]'s editor, with the pointer there.
fn hovered(name: &str, want: impl Fn(&Inline) -> bool, after: u32) -> Editor {
    hovered_by(name, want, after, 1.0)
}

/// [`hovered`] with the pointer `dx` points right of the caret there.
fn hovered_by(name: &str, want: impl Fn(&Inline) -> bool, after: u32, dx: f64) -> Editor {
    let mut e = open(name);
    e.complete(fonts());
    let at = e.document().sections.iter().flat_map(|s| &s.blocks).find_map(|b| {
        let Block::Para(p) = b else { return None };
        let mut u = 0u32;
        for i in &p.inlines {
            if want(i) {
                return Some(Position::text(StoryId::Body, ParaId(p.id), Unit(u + after)));
            }
            u += i.units() as u32;
        }
        None
    });
    let c = e.caret_at(at.expect("found")).expect("laid out");
    let (x, y) = e.stack().to_doc(c.page, c.x + dx, (c.top + c.bottom) / 2.0).expect("on the stack");
    e.pointer_input(PointerKind::Move, x, y, 0, Mods::default(), fonts());
    e
}

fn tip(e: &mut Editor) -> Option<(HoverTip, String)> {
    e.hover_tip().map(|(k, t, _)| (*k, t.text.clone()))
}

#[test]
fn a_note_reference_shows_its_note_text() {
    let (kind, text) = hover_after("FX-STORIES.docx", |i| matches!(i, Inline::NoteRef { .. }), 0).expect("a tooltip");
    assert_eq!(kind, HoverTip::Note);
    assert!(!text.is_empty() && !text.contains('\n'), "{text}");
}

#[test]
fn a_tracked_change_shows_its_author_and_kind() {
    let (kind, text) = hover_after("review/FX-REVIEW-RUNS.docx", |i| i.run().is_some_and(|r| r.rev.ins.is_some()), 1).expect("a tooltip");
    assert_eq!(kind, HoverTip::Revision);
    assert!(text.ends_with("\nInserted"), "{text}");
}

#[test]
fn a_tip_shows_over_a_changes_last_character_right_half() {
    let ins = |i: &Inline| i.run().is_some_and(|r| r.rev.ins.is_some());
    let (kind, text) = tip(&mut hovered_by("review/FX-REVIEW-RUNS.docx", ins, 67, -1.0)).expect("a tooltip");
    assert_eq!(kind, HoverTip::Revision);
    assert!(text.ends_with("\nInserted"), "{text}");
}

#[test]
fn a_tip_shows_over_a_comments_last_character_right_half() {
    let inner = |i: &Inline| matches!(i, Inline::Range(m) if m.kind == crate::docx::review::RangeKind::CommentStart && m.id == 3);
    let (kind, text) = tip(&mut hovered_by("review/FX-REVIEW-COMMENTS.docx", inner, 29, -1.0)).expect("a tooltip");
    assert_eq!(kind, HoverTip::Comment);
    let again = tip(&mut hovered_by("review/FX-REVIEW-COMMENTS.docx", inner, 28, 1.0)).expect("a tooltip");
    assert_eq!(text, again.1, "the same comment, the innermost, from either half");
}

#[test]
fn a_commented_range_shows_its_comment() {
    let start = |i: &Inline| matches!(i, Inline::Range(m) if m.kind == crate::docx::review::RangeKind::CommentStart);
    let (kind, text) = hover_after("review/FX-REVIEW-COMMENTS.docx", start, 1).expect("a tooltip");
    assert_eq!(kind, HoverTip::Comment);
    let (byline, body) = text.split_once('\n').expect("two lines");
    assert!(!byline.is_empty() && !body.is_empty(), "{text}");
}

#[test]
fn a_double_click_on_a_note_reference_is_a_note_gesture_and_jumps() {
    let mut e = open("FX-STORIES.docx");
    e.complete(fonts());
    let at = e.document().sections.iter().flat_map(|s| &s.blocks).find_map(|b| {
        let Block::Para(p) = b else { return None };
        let mut u = 0u32;
        for i in &p.inlines {
            if matches!(i, Inline::NoteRef { kind: crate::docx::NoteKind::Foot, .. }) {
                return Some(Position::text(StoryId::Body, ParaId(p.id), Unit(u)));
            }
            u += i.units() as u32;
        }
        None
    });
    let c = e.caret_at(at.expect("a footnote reference")).expect("laid out");
    let (x, y) = e.stack().to_doc(c.page, c.x + 1.0, (c.top + c.bottom) / 2.0).expect("on the stack");
    let g = e.canvas_press(x, y, 2, Mods::default(), fonts()).map(|g| g.gesture);
    assert_eq!(g, Some(super::super::gestures::CanvasGesture::NoteDoubleClick));
    e.pointer_input(PointerKind::Down, x, y, 1, Mods::default(), fonts());
    e.pointer_input(PointerKind::Up, x, y, 1, Mods::default(), fonts());
    e.pointer_input(PointerKind::Down, x, y, 2, Mods::default(), fonts());
    assert!(matches!(e.selection.focus().map(|p| p.story), Some(StoryId::Footnote(_))));
}

#[test]
fn a_huge_note_tip_is_clipped_and_leaves_out_field_codes() {
    let ns = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main""#;
    let field = r#"<w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText>PAGE</w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>7</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r>"#;
    let big = "word ".repeat(20_000);
    let foot = format!(
        r#"<w:footnotes {ns}><w:footnote w:id="1"><w:p><w:r><w:t>Page</w:t></w:r>{field}</w:p><w:p><w:r><w:t>{big}</w:t></w:r></w:p></w:footnote></w:footnotes>"#
    );
    let body = r#"<w:p><w:r><w:t>Body</w:t></w:r><w:r><w:footnoteReference w:id="1"/></w:r></w:p>"#;
    let bytes = super::empty_stories::build(body, "", &[("footnotes.xml", "footnotes", "footnotes", &foot)]);
    let mut e = Editor::open(&bytes, fonts()).expect("opens");
    e.complete(fonts());
    let c = e.caret_at(Position::text(StoryId::Body, super::para_id(&e, 0), Unit(4))).expect("laid out");
    let (x, y) = e.stack().to_doc(c.page, c.x + 1.0, (c.top + c.bottom) / 2.0).expect("on the stack");
    e.pointer_input(PointerKind::Move, x, y, 0, Mods::default(), fonts());
    let (kind, text) = tip(&mut e).expect("a tooltip");
    assert_eq!(kind, HoverTip::Note);
    assert!(text.starts_with("Page7\nword word"), "{}", &text[..40]);
    assert_eq!(text.chars().count(), 1025, "clipped with an ellipsis");
    assert!(text.ends_with('…'));
}

#[test]
fn a_change_and_a_comment_tip_give_their_parts_and_hide_once_the_document_changes() {
    let mut e = hovered("review/FX-REVIEW-RUNS.docx", |i| i.run().is_some_and(|r| r.rev.ins.is_some()), 1);
    let (_, t, d) = e.hover_tip().cloned().expect("a tooltip");
    let d = d.expect("its parts");
    assert_eq!((d.change, d.body.as_str()), (Some(RevisionKind::Insert), "Inserted"));
    let name = e.document().people.get(d.author).map(|p| p.name.to_string()).expect("an author");
    assert!(!name.is_empty() && t.text.starts_with(&name) && t.text.ends_with("\nInserted"), "{}", t.text);
    e.execute(super::super::commands::Command::InsertText("x".into()), fonts());
    assert!(e.hover_tip().is_none(), "typing hides it");
    let start = |i: &Inline| matches!(i, Inline::Range(m) if m.kind == crate::docx::review::RangeKind::CommentStart);
    let mut e = hovered("review/FX-REVIEW-COMMENTS.docx", start, 1);
    let (_, t, d) = e.hover_tip().cloned().expect("a tooltip");
    let d = d.expect("its parts");
    assert_eq!(d.change, None);
    assert!(!d.body.is_empty() && t.text.ends_with(&format!("\n{}", d.body)), "{}", t.text);
    assert_eq!(
        t.text.split_once('\n').map(|(b, _)| b.contains(", ")),
        Some(d.time.as_ref().is_some_and(|t| t.utc.is_some())),
        "a byline with its time"
    );
}

#[test]
fn a_tip_is_looked_up_again_only_once_the_pointer_or_the_document_changes() {
    let mut e = hovered("review/FX-REVIEW-RUNS.docx", |i| i.run().is_some_and(|r| r.rev.ins.is_some()), 1);
    let first = e.hover_tip().cloned().expect("a tooltip");
    let looked = e.hover_lookups;
    for key in ["ArrowRight", "ArrowLeft"] {
        let cmd = e.key_command(key, Mods { shift: true, ..Mods::default() }).expect("a selection move");
        e.execute(cmd, fonts());
        assert_eq!(e.hover_tip(), Some(&first), "a selection move keeps the tip");
    }
    assert_eq!(e.hover_lookups, looked, "no lookup while nothing it reads changed");
    let (x, y) = e.hover_point().expect("hovering");
    e.set_markup_view(crate::docx::review::MarkupView::Simple);
    e.pointer_input(PointerKind::Move, x, y, 0, Mods::default(), fonts());
    assert!(e.hover_tip().is_none(), "no change tips outside All Markup");
    assert_eq!(e.hover_lookups, looked + 1);
}
