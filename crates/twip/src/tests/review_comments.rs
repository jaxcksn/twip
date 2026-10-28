//! Comments edited in the editor: threads added, replied to, resolved and edited keep their authors, bodies, ranges
//! and done flags through a save and a fresh read; ranges follow edits; and every state passes the integrity checks.

use std::path::Path;

use crate::docx::review::CommentId;
use crate::docx::{read_editable, write};
use crate::editing::Editor;
use crate::editing::commands::{Command, Dir, Outcome};
use crate::editing::comments::{RichParagraph, RichRun, RichText};
use crate::editing::position::StoryId;
use crate::editing::tests::fonts;
use crate::layout::Unit;

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor").join(name)).expect("a fixture")
}

/// A new document holding one paragraph, "Alpha beta gamma.".
fn alpha() -> Editor {
    let mut e = Editor::open(&fixture("FX-EMPTY.docx"), fonts()).expect("opens");
    e.execute(Command::InsertText("Alpha beta gamma.".into()), fonts());
    e
}

fn select(e: &mut Editor, a: u32, b: u32) {
    let para = e.selection().focus().and_then(|p| p.para_offset()).expect("a caret").0;
    e.execute(Command::SelectRange { para, a: Unit(a), b: Unit(b) }, fonts());
}

/// The threads as (scope, comments as (author, text, done)).
fn threads(e: &mut Editor) -> Vec<(Option<String>, Vec<(String, String, bool)>)> {
    e.comment_threads()
        .into_iter()
        .map(|t| {
            let doc = e.document();
            let comments = std::iter::once(t.root)
                .chain(t.replies)
                .filter_map(|id| doc.comments.by_id.get(&id))
                .map(|c| {
                    let author = doc.people.get(c.author).map_or(String::new(), |p| p.name.to_string());
                    (author, RichText::from_blocks(&c.body).to_plain(), c.done)
                })
                .collect();
            (t.scope, comments)
        })
        .collect()
}

fn reread(e: &Editor) -> Editor {
    let bytes = write::save(e.document()).expect("saves");
    Editor::with_document(read_editable(&bytes).expect("reads back"), fonts())
}

fn changed(o: Outcome) {
    assert!(matches!(o, Outcome::Changed(_)), "{o:?}");
}

fn only_root(e: &mut Editor) -> CommentId {
    e.comment_threads().first().expect("a thread").root
}

#[test]
fn threads_survive_a_save_and_a_fresh_read() {
    let mut e = alpha();
    select(&mut e, 6, 10);
    changed(e.add_comment(&RichText::plain("Root"), "Author A", fonts()));
    let root = only_root(&mut e);
    changed(e.reply_comment(root, &RichText::plain("Reply one"), "Author A", fonts()));
    changed(e.reply_comment(root, &RichText::plain("Reply two"), "Author B", fonts()));
    changed(e.resolve_comment(root, true, fonts()));
    let want = vec![(
        Some("beta".to_string()),
        vec![("Author A".into(), "Root".into(), true), ("Author A".into(), "Reply one".into(), true), ("Author B".into(), "Reply two".into(), true)],
    )];
    assert_eq!(threads(&mut e), want);
    let mut back = reread(&e);
    assert_eq!(threads(&mut back), want);
    let v = crate::editing::integrity::check(back.document(), back.index(), &back.selection());
    assert!(v.is_empty(), "{v:?}");
    let c = &back.document().comments.by_id;
    assert!(c.values().all(|c| c.durable_id.is_some() && c.date.is_some()), "ids and dates are written");
    assert!(c.values().any(|c| c.initials.as_deref() == Some("AB")));

    // Reopened and edited, then saved again.
    let root = only_root(&mut back);
    changed(back.resolve_comment(root, false, fonts()));
    changed(back.edit_comment(root, &RichText::plain("Root, edited"), fonts()));
    let mut again = reread(&back);
    let t = threads(&mut again);
    assert_eq!(t[0].1[0], ("Author A".into(), "Root, edited".into(), false));
    assert!(t[0].1.iter().all(|c| !c.2), "reopening reopens the whole thread");
}

#[test]
fn a_collapsed_selection_anchors_to_the_word_at_the_caret() {
    let mut e = alpha();
    for (at, text) in [(8, "Inside"), (11, "Start"), (5, "Space")] {
        select(&mut e, at, at);
        changed(e.add_comment(&RichText::plain(text), "Author A", fonts()));
    }
    let scopes: Vec<(Option<String>, String)> = threads(&mut e).into_iter().map(|(s, c)| (s, c[0].1.clone())).collect();
    let want = [("Alpha", "Space"), ("beta", "Inside"), ("gamma", "Start")];
    assert_eq!(scopes, want.map(|(s, t)| (Some(s.to_string()), t.to_string())));
}

#[test]
fn ranges_follow_edits_and_go_with_all_their_text() {
    let mut e = alpha();
    select(&mut e, 6, 10);
    changed(e.add_comment(&RichText::plain("On beta"), "Author A", fonts()));
    // Typing inside grows the range; deleting part of it shrinks it.
    select(&mut e, 8, 8);
    e.execute(Command::InsertText("XY".into()), fonts());
    assert_eq!(e.comment_threads()[0].scope.as_deref(), Some("beXYta"));
    select(&mut e, 5, 9);
    changed(e.execute(Command::Delete { dir: Dir::Forward, word: false }, fonts()));
    assert_eq!(e.comment_threads()[0].scope.as_deref(), Some("Yta"));
    // Split and joined again.
    select(&mut e, 6, 6);
    changed(e.execute(Command::Enter, fonts()));
    assert_eq!(e.comment_threads()[0].scope.as_deref(), Some("Y\nta"));
    changed(e.execute(Command::Delete { dir: Dir::Back, word: false }, fonts()));
    assert_eq!(e.comment_threads()[0].scope.as_deref(), Some("Yta"));
    // All of its text deleted: the comment goes with it, and undo brings both back.
    select(&mut e, 5, 8);
    changed(e.execute(Command::Delete { dir: Dir::Forward, word: false }, fonts()));
    assert!(e.document().comments.by_id.is_empty());
    changed(e.execute(Command::Undo, fonts()));
    assert_eq!(e.comment_threads()[0].scope.as_deref(), Some("Yta"));
}

#[test]
fn deleting_a_root_deletes_its_replies_and_undo_restores_them() {
    let mut e = alpha();
    select(&mut e, 0, 5);
    changed(e.add_comment(&RichText::plain("Root"), "Author A", fonts()));
    let root = only_root(&mut e);
    changed(e.reply_comment(root, &RichText::plain("First"), "Author B", fonts()));
    changed(e.reply_comment(root, &RichText::plain("Second"), "Author B", fonts()));
    let first = e.comment_threads()[0].replies[0];
    changed(e.delete_comment(first, fonts()));
    assert_eq!(threads(&mut e)[0].1.len(), 2, "a reply goes alone");
    changed(e.delete_comment(root, fonts()));
    assert!(e.document().comments.by_id.is_empty());
    assert!(e.comment_threads().is_empty());
    changed(e.execute(Command::Undo, fonts()));
    assert_eq!(threads(&mut e)[0].1.len(), 2);
    let back = reread(&e);
    assert_eq!(back.document().comments.by_id.len(), 2);
}

#[test]
fn rich_bodies_round_trip() {
    let mut e = alpha();
    select(&mut e, 6, 10);
    let run = |text: &str| RichRun { text: text.into(), ..RichRun::default() };
    let body = RichText {
        paragraphs: vec![
            RichParagraph {
                runs: vec![
                    run("A "),
                    RichRun { bold: true, ..run("bold") },
                    run(" and "),
                    RichRun { italic: true, underline: true, ..run("styled") },
                ],
            },
            RichParagraph {
                runs: vec![
                    RichRun { strike: true, ..run("struck") },
                    run(", "),
                    RichRun { link: Some("https://example.com/".into()), ..run("a link") },
                ],
            },
        ],
    };
    changed(e.add_comment(&body, "Author A", fonts()));
    let back = reread(&e);
    let c = back.document().comments.by_id.values().next().expect("the comment");
    assert_eq!(RichText::from_blocks(&c.body), body);
}

#[test]
fn headers_take_no_new_comments() {
    let mut e = Editor::open(&fixture("FX-STORIES.docx"), fonts()).expect("opens");
    e.execute(Command::EditHeaderFooter { footer: false }, fonts());
    assert!(matches!(e.selection().focus().map(|p| p.story), Some(StoryId::HeaderFooter(_))));
    assert_eq!(e.add_comment(&RichText::plain("In header"), "Author A", fonts()), Outcome::Unchanged("unavailable"));
    assert!(e.document().comments.by_id.is_empty());
}

#[test]
fn the_view_tints_ranges_and_reports_their_anchors() {
    let mut e = alpha();
    select(&mut e, 6, 10);
    changed(e.add_comment(&RichText::plain("On beta"), "Author A", fonts()));
    select(&mut e, 0, 5);
    changed(e.add_comment(&RichText::plain("On Alpha"), "Author A", fonts()));
    // The caret in "beta": its thread is active.
    select(&mut e, 7, 7);
    let v = e.view(0..1);
    assert_eq!(v.comments.len(), 2);
    let active: Vec<bool> = v.comments.iter().map(|c| c.active).collect();
    assert_eq!(active, [false, true], "the thread at the caret");
    assert!(v.comments.iter().all(|c| c.rects.len() == 1 && c.rects[0].0 == 0));
    assert_eq!(v.comment_anchors.len(), 2);
    let (alpha, beta) = (v.comment_anchors[0], v.comment_anchors[1]);
    assert_eq!((alpha.1, beta.1), (0, 0));
    assert!(alpha.2[0] < beta.2[0] && (alpha.2[1] - beta.2[1]).abs() < 0.01, "both on the first line, Alpha first");
    assert!((v.comments[1].rects[0].1[0] - beta.2[0]).abs() < 0.01, "the anchor is the range's start");
    // The host's choice holds until the selection moves.
    let first = v.comments[0].thread;
    e.execute(Command::Comment(crate::editing::comments::CommentCmd::Activate(Some(first))), fonts());
    assert_eq!(e.active_comment(), Some(first));
    select(&mut e, 8, 8);
    assert_ne!(e.active_comment(), Some(first));
}
