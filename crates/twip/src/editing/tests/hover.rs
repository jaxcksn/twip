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

/// A paragraph of a hyperlink with a ScreenTip, frame and bookmark, then a cross-reference made a link by `\h`, then
/// the bookmarked paragraph.
fn linked() -> Editor {
    let field = |code: &str, shown: &str| {
        format!(
            r#"<w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve">{code}</w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>{shown}</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r>"#
        )
    };
    let body = format!(
        r#"<w:p>{}<w:r><w:t xml:space="preserve"> and </w:t></w:r>{}</w:p><w:p><w:bookmarkStart w:id="1" w:name="Target"/><w:r><w:t>Here</w:t></w:r><w:bookmarkEnd w:id="1"/></w:p>"#,
        field(r#" HYPERLINK "https://a.test/" \l "top" \o "Read the guide" \t "_blank" "#, "guide"),
        field(r#" REF Target \h "#, "Here"),
    );
    let mut e = Editor::open(&super::blank_body(&body), fonts()).expect("opens");
    e.complete(fonts());
    e
}

/// The position `after` units into the `n`th field of the first paragraph, and its place on the stack.
fn in_field(e: &mut Editor, n: usize, after: u32) -> (Position, f64, f64) {
    let Some(Block::Para(p)) = e.document().sections[0].blocks.first() else { panic!("a paragraph") };
    let s = crate::docx::fields::spans(&p.inlines)[n];
    let u: u32 = p.inlines[..s.begin].iter().map(|i| i.units() as u32).sum();
    let at = Position::text(StoryId::Body, ParaId(p.id), Unit(u + after));
    let c = e.caret_at(at).expect("laid out");
    let (x, y) = e.stack().to_doc(c.page, c.x + 1.0, (c.top + c.bottom) / 2.0).expect("on the stack");
    (at, x, y)
}

#[test]
fn a_link_reads_back_its_parts_and_shows_its_screen_tip() {
    let mut e = linked();
    let (at, x, y) = in_field(&mut e, 0, 1);
    let link = e.link_at(at).expect("a link");
    assert_eq!(
        (link.address.as_str(), link.target.as_str(), link.bookmark.as_deref(), link.tip.as_deref(), link.frame.as_deref(), link.reference),
        ("https://a.test/#top", "https://a.test/", Some("top"), Some("Read the guide"), Some("_blank"), false)
    );
    e.pointer_input(PointerKind::Move, x, y, 0, Mods::default(), fonts());
    assert_eq!(tip(&mut e), Some((HoverTip::Link, "Read the guide".into())), "the tip, not the address");
}

#[test]
fn a_linked_cross_reference_is_a_link_that_ctrl_click_follows() {
    let mut e = linked();
    let (at, x, y) = in_field(&mut e, 1, 1);
    let link = e.link_at(at).expect("a link");
    assert_eq!((link.address.as_str(), link.reference), ("#Target", true));
    e.selection = crate::editing::selection::Selection::Caret(at);
    assert!(e.link_here().is_none(), "not a hyperlink to edit or remove");
    e.pointer_input(PointerKind::Move, x, y, 0, Mods::default(), fonts());
    assert_eq!(tip(&mut e), Some((HoverTip::Link, "#Target".into())));
    let ctrl = Mods { ctrl: true, ..Mods::default() };
    e.pointer_input(PointerKind::Down, x, y, 1, ctrl, fonts());
    e.pointer_input(PointerKind::Up, x, y, 1, ctrl, fonts());
    let second = match &e.document().sections[0].blocks[1] {
        Block::Para(p) => ParaId(p.id),
        Block::Table(_) => panic!("a paragraph"),
    };
    assert_eq!(e.selection().anchor().and_then(|a| a.para_offset()).map(|p| p.0), Some(second), "at the bookmark");
}

#[test]
fn editing_a_link_address_keeps_its_screen_tip() {
    let mut e = linked();
    let (at, ..) = in_field(&mut e, 0, 1);
    e.selection = crate::editing::selection::Selection::Caret(at);
    let edit = crate::api::command::HyperlinkEdit { address: Some("https://b.test/".into()), text: None };
    assert!(matches!(e.edit_hyperlink(edit, fonts()), crate::editing::Outcome::Changed(_)));
    let link = e.link_here().expect("still a link");
    assert_eq!((link.address.as_str(), link.bookmark, link.tip.as_deref()), ("https://b.test/", None, Some("Read the guide")));
    let mut e = linked();
    e.selection = crate::editing::selection::Selection::Caret(Position::text(
        StoryId::Body,
        ParaId(match &e.document().sections[0].blocks[1] {
            Block::Para(p) => p.id,
            Block::Table(_) => panic!("a paragraph"),
        }),
        Unit(0),
    ));
    let parts = crate::editing::commands::LinkParts { bookmark: Some("Target".into()), tip: Some("Go".into()), ..Default::default() };
    assert_eq!(parts.code(), r#" HYPERLINK \l "Target" \o "Go" "#);
    assert!(matches!(e.insert_link(parts, Some("go".into()), fonts()), crate::editing::Outcome::Changed(_)));
}

#[test]
fn the_notes_list_gives_each_notes_mark_text_reference_and_page() {
    use crate::api::command::{NoteInsert, NoteType};
    use crate::docx::NoteKind;
    let mut e = Editor::open(&super::blank_body("<w:p><w:r><w:t>Body</w:t></w:r></w:p>"), fonts()).expect("opens");
    let body = |e: &Editor| match &e.document().sections[0].blocks[0] {
        Block::Para(p) => ParaId(p.id),
        Block::Table(_) => panic!("a paragraph"),
    };
    let at_end = |e: &mut Editor| e.selection = crate::editing::selection::Selection::Caret(Position::text(StoryId::Body, body(e), Unit(4)));
    for (kind, custom, text) in [(NoteType::Footnote, None, "First"), (NoteType::Footnote, Some("*"), "Starred"), (NoteType::Endnote, None, "Last")] {
        at_end(&mut e);
        e.execute(super::super::commands::Command::InsertNote(NoteInsert { kind, custom_mark: custom.map(Into::into) }), fonts());
        e.execute(super::super::commands::Command::InsertText(text.into()), fonts());
    }
    e.complete(fonts());
    let notes = e.notes();
    let got: Vec<(NoteKind, &str, bool, &str, Option<usize>)> =
        notes.iter().map(|n| (n.kind, n.mark.as_str(), n.custom, n.text.as_str(), n.page)).collect();
    assert_eq!(
        got,
        [
            (NoteKind::Foot, "*", true, "Starred", Some(0)),
            (NoteKind::Foot, "1", false, "First", Some(0)),
            (NoteKind::End, "i", false, "Last", Some(0))
        ],
        "in their references' order, each inserted before the last"
    );
    assert!(notes.iter().all(|n| n.reference.and_then(|r| r.para_offset()).map(|p| p.0) == Some(body(&e))), "referenced from the body");
}

#[test]
fn a_note_separator_set_in_a_document_without_notes_is_saved() {
    use crate::docx::{NoteKind, NoteSeparator as S};
    // With a settings part, and without one
    let docs = [open("FX-EMPTY.docx"), Editor::open(&super::blank_body("<w:p><w:r><w:t>Text</w:t></w:r></w:p>"), fonts()).expect("opens")];
    for mut e in docs {
        assert!(e.document().footnotes.is_empty(), "no footnotes");
        assert!(matches!(e.set_note_separator(NoteKind::Foot, S::Notice, Some("Continued"), fonts()), crate::editing::Outcome::Changed(_)));
        let saved = crate::docx::write::save(e.document()).expect("saves");
        let again = Editor::open(&saved, fonts()).expect("reopens");
        assert_eq!(again.note_separator(NoteKind::Foot, S::Notice).map(|s| s.text), Some("Continued".into()), "saved");
        assert_eq!(again.note_separator(NoteKind::End, S::Notice), None, "no endnote part's notice");
    }
}

#[test]
fn note_separators_are_set_read_back_saved_and_reset() {
    use crate::docx::{NoteKind, NoteSeparator as S};
    let mut e = open("FX-STORIES.docx");
    e.complete(fonts());
    let sep = |e: &Editor, part| e.note_separator(NoteKind::Foot, part).map(|s| (s.text, s.builtin));
    assert_eq!(sep(&e, S::Separator), Some((String::new(), true)), "Word's line");
    assert_eq!(sep(&e, S::Notice), None, "no notice");
    let depth = e.history().undo_depth();
    assert!(matches!(e.set_note_separator(NoteKind::Foot, S::Separator, Some("Notes"), fonts()), crate::editing::Outcome::Changed(_)));
    assert!(matches!(
        e.set_note_separator(NoteKind::Foot, S::Notice, Some("Continued on the next page"), fonts()),
        crate::editing::Outcome::Changed(_)
    ));
    assert_eq!(e.history().undo_depth(), depth + 2);
    assert_eq!(sep(&e, S::Separator), Some(("Notes".into(), false)));
    e.complete(fonts());
    let saved = crate::docx::write::save(e.document()).expect("saves");
    let mut again = Editor::open(&saved, fonts()).expect("reopens");
    assert_eq!(sep(&again, S::Separator), Some(("Notes".into(), false)), "saved");
    assert_eq!(sep(&again, S::Notice), Some(("Continued on the next page".into(), false)), "the new notice listed in the settings");
    assert_eq!(sep(&again, S::Continuation), Some((String::new(), true)), "the other kept");
    assert!(matches!(again.set_note_separator(NoteKind::Foot, S::Notice, None, fonts()), crate::editing::Outcome::Changed(_)));
    assert!(matches!(again.set_note_separator(NoteKind::Foot, S::Separator, None, fonts()), crate::editing::Outcome::Changed(_)));
    let twice = Editor::open(&crate::docx::write::save(again.document()).expect("saves"), fonts()).expect("reopens");
    assert_eq!((sep(&twice, S::Separator), sep(&twice, S::Notice)), (Some((String::new(), true)), None), "reset");
    e.undo_many(true, 2, fonts());
    assert_eq!((sep(&e, S::Separator), sep(&e, S::Notice)), (Some((String::new(), true)), None), "undone");
}

fn separator_doc(foot: &str, settings: &str) -> Editor {
    use super::empty_stories::NS;
    let foot = format!(r#"<w:footnotes {NS}>{foot}</w:footnotes>"#);
    let settings = format!(r#"<w:settings {NS}>{settings}</w:settings>"#);
    let body = "<w:p><w:r><w:t>Text</w:t></w:r></w:p>";
    let parts = [("footnotes.xml", "footnotes", "footnotes", foot.as_str()), ("settings.xml", "settings", "settings", settings.as_str())];
    let mut e = Editor::open(&super::empty_stories::build(body, "", &parts), fonts()).expect("opens");
    e.complete(fonts());
    e
}

fn separator_note(id: i64, text: &str) -> String {
    format!(r#"<w:footnote w:type="separator" w:id="{id}"><w:p><w:r><w:t>{text}</w:t></w:r></w:p></w:footnote>"#)
}

#[test]
fn a_separator_edit_updates_the_story_the_reader_uses() {
    use crate::docx::{NoteKind, NoteSeparator as S};
    // -1 is unlisted; -3 is listed and the active one.
    let foot = separator_note(-1, "Unlisted") + &separator_note(-3, "Listed");
    let mut e = separator_doc(&foot, r#"<w:footnotePr><w:footnote w:id="-3"/></w:footnotePr>"#);
    let text = |e: &Editor| e.note_separator(NoteKind::Foot, S::Separator).map(|s| s.text);
    assert_eq!(text(&e), Some("Listed".into()));
    assert!(matches!(e.set_note_separator(NoteKind::Foot, S::Separator, Some("New"), fonts()), crate::editing::Outcome::Changed(_)));
    let saved = crate::docx::write::save(e.document()).expect("saves");
    assert_eq!(text(&Editor::open(&saved, fonts()).expect("reopens")), Some("New".into()), "the edit survives");
}

#[test]
fn a_separator_reset_removes_every_listed_story_of_the_type() {
    use crate::docx::{NoteKind, NoteSeparator as S};
    let foot = separator_note(-1, "A") + &separator_note(-3, "B");
    let mut e = separator_doc(&foot, r#"<w:footnotePr><w:footnote w:id="-1"/><w:footnote w:id="-3"/></w:footnotePr>"#);
    assert!(matches!(e.set_note_separator(NoteKind::Foot, S::Notice, None, fonts()), crate::editing::Outcome::Unchanged(_)));
    assert!(matches!(e.set_note_separator(NoteKind::Foot, S::Separator, None, fonts()), crate::editing::Outcome::Changed(_)));
    let saved = crate::docx::write::save(e.document()).expect("saves");
    let again = Editor::open(&saved, fonts()).expect("reopens");
    assert_eq!(again.note_separator(NoteKind::Foot, S::Separator).map(|s| (s.text, s.builtin)), Some((String::new(), true)));
}

#[test]
fn a_separator_edit_keeps_the_note_properties_namespace_declarations() {
    use crate::docx::{NoteKind, NoteSeparator as S};
    let foot = separator_note(-1, "Old");
    let pr = r#"<w:footnotePr xmlns:p="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><p:numFmt p:val="upperRoman"/></w:footnotePr>"#;
    let mut e = separator_doc(&foot, pr);
    assert!(matches!(e.set_note_separator(NoteKind::Foot, S::Separator, Some("New"), fonts()), crate::editing::Outcome::Changed(_)));
    let saved = crate::docx::write::save(e.document()).expect("saves a settings part with a bound prefix");
    let again = Editor::open(&saved, fonts()).expect("reopens");
    assert_eq!(again.note_separator(NoteKind::Foot, S::Separator).map(|s| s.text), Some("New".into()));
}

#[test]
fn a_separator_with_a_table_reports_its_cells_text() {
    use crate::docx::{NoteKind, NoteSeparator as S};
    let table = r#"<w:tbl><w:tblGrid><w:gridCol w:w="2000"/><w:gridCol w:w="2000"/></w:tblGrid><w:tr><w:tc><w:p><w:r><w:t>Notes</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>here</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p><w:r><w:t>after</w:t></w:r></w:p>"#;
    let foot = format!(r#"<w:footnote w:type="separator" w:id="-1">{table}</w:footnote>"#);
    let e = separator_doc(&foot, r#"<w:footnotePr><w:footnote w:id="-1"/></w:footnotePr>"#);
    assert_eq!(e.note_separator(NoteKind::Foot, S::Separator).map(|s| s.text), Some("Notes\nhere\nafter".into()));
}
