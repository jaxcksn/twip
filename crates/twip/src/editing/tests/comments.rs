//! Comments: ranges through deletions, reply threads, and the cost of both

use std::io::{Read, Write};

use super::super::Editor;
use super::super::commands::Command;
use super::super::position::{Position, StoryId};
use super::super::selection::{Granularity, Selection};
use super::{blank_body, fonts, para_id};
use crate::api::command::DeleteOptions;
use crate::api::review::Body;
use crate::api::value::Direction;
use crate::docx::review::{AuthorId, Comment, CommentId, Comments};
use crate::layout::Unit;

fn select(e: &mut Editor, a: (usize, u32), b: (usize, u32)) {
    let at = |e: &Editor, (p, u): (usize, u32)| Position::text(StoryId::Body, para_id(e, p), Unit(u));
    e.selection = Selection::range(at(e, a), at(e, b), Granularity::Char);
}

fn delete(e: &mut Editor) {
    e.execute(Command::Delete(DeleteOptions { direction: Direction::Forward, unit: None }), fonts());
}

fn paras(n: usize) -> Editor {
    let text = ["aaaa bbbb", "cccc dddd", "eeee ffff", "gggg hhhh", "iiii jjjj"];
    let body: String = text[..n].iter().map(|t| format!("<w:p><w:r><w:t>{t}</w:t></w:r></w:p>")).collect();
    Editor::open(&blank_body(&body), fonts()).expect("opens")
}

fn comment(e: &mut Editor, a: (usize, u32), b: (usize, u32)) {
    select(e, a, b);
    e.execute(Command::AddComment(Body::Text("note".into())), fonts());
    assert_eq!(e.comment_threads().len(), 1, "a comment was added");
}

#[test]
fn a_comment_whose_text_across_paragraphs_is_deleted_goes() {
    let mut e = paras(2);
    comment(&mut e, (0, 5), (1, 4));
    select(&mut e, (0, 5), (1, 4));
    delete(&mut e);
    assert!(e.comment_threads().is_empty(), "the comment had no text left");
    assert!(e.document().comments.by_id.is_empty());
}

#[test]
fn a_comment_losing_whole_paragraphs_keeps_its_surviving_text() {
    let mut e = paras(5);
    comment(&mut e, (1, 0), (3, 2));
    select(&mut e, (0, 9), (2, 2));
    delete(&mut e);
    let t = e.comment_threads();
    assert_eq!(t.len(), 1);
    assert_eq!(t[0].scope.as_deref(), Some("ee ffff\ngg"), "from the deletion point to its end");
}

#[test]
fn a_threads_text_is_cut_to_a_limit() {
    let long = "ab".repeat(6_000);
    let mut e =
        Editor::open(&blank_body(&format!("<w:p><w:r><w:t>{long}</w:t></w:r></w:p><w:p><w:r><w:t>tail</w:t></w:r></w:p>")), fonts()).expect("opens");
    comment(&mut e, (0, 0), (1, 4));
    let scope = e.comment_threads()[0].scope.clone().expect("a scope");
    assert_eq!(scope.chars().count(), crate::api::review::ANCHOR_TEXT_LIMIT);
    assert_eq!(scope, long[..crate::api::review::ANCHOR_TEXT_LIMIT]);
}

/// A document of `n` paragraphs, each one word with a comment on it.
fn commented(n: usize) -> Editor {
    let body: String = (0..n)
        .map(|i| {
            format!(
                r#"<w:p><w:commentRangeStart w:id="{i}"/><w:r><w:t>word</w:t></w:r><w:commentRangeEnd w:id="{i}"/><w:r><w:commentReference w:id="{i}"/></w:r></w:p>"#
            )
        })
        .collect();
    let comments: String =
        (0..n).map(|i| format!(r#"<w:comment w:id="{i}" w:author="A" w:initials="A"><w:p><w:r><w:t>c</w:t></w:r></w:p></w:comment>"#)).collect();
    let comments = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:comments xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">{comments}</w:comments>"#
    );
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(blank_body(&body))).unwrap();
    let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for i in 0..z.len() {
        let mut f = z.by_index(i).unwrap();
        let name = f.name().unwrap().to_string();
        let mut data = Vec::new();
        f.read_to_end(&mut data).unwrap();
        let text = String::from_utf8_lossy(&data).into_owned();
        if name == "word/_rels/document.xml.rels" {
            data = text
                .replace(
                    "</Relationships>",
                    r#"<Relationship Id="rIdC" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments" Target="comments.xml"/></Relationships>"#,
                )
                .into_bytes();
        } else if name == "[Content_Types].xml" {
            data = text
                .replace(
                    "</Types>",
                    r#"<Override PartName="/word/comments.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.comments+xml"/></Types>"#,
                )
                .into_bytes();
        }
        out.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
        out.write_all(&data).unwrap();
    }
    out.start_file("word/comments.xml", zip::write::SimpleFileOptions::default()).unwrap();
    out.write_all(comments.as_bytes()).unwrap();
    Editor::open(&out.finish().unwrap().into_inner(), fonts()).expect("opens")
}

#[test]
fn an_edit_over_many_comments_scans_the_document_once() {
    let time = |n: usize| {
        let mut e = commented(n);
        let marks = super::texts(&e).len();
        assert_eq!(marks, n);
        // Half the comments are touched; the other half keep the document large.
        select(&mut e, (0, 0), (n / 2 - 1, 4));
        let t = std::time::Instant::now();
        e.execute(Command::InsertText("x".into()), fonts());
        let took = t.elapsed().as_secs_f64();
        assert!(e.document().sections[0].blocks.len() < n, "the selection was replaced");
        took
    };
    let (small, large) = (time(300).min(time(300)), time(1500).min(time(1500)));
    // Linear is 5x; a document scan per comment is 25x.
    assert!(large < small * 12.0, "300: {small:.3}s, 1500: {large:.3}s");
}

fn reply(id: i64, parent: Option<i64>) -> Comment {
    Comment {
        id: CommentId(id),
        author: AuthorId(0),
        initials: None,
        date: None,
        date_utc: None,
        body: Vec::new(),
        para_id: 0,
        durable_id: None,
        parent: parent.map(CommentId),
        done: false,
        extensible: Default::default(),
    }
}

#[test]
fn a_long_reply_chain_is_one_thread_found_quickly() {
    let n = 20_000;
    let mut c = Comments::default();
    for i in 1..=n {
        c.by_id.insert(CommentId(i), reply(i, (i > 1).then(|| i - 1)));
    }
    let t = std::time::Instant::now();
    let threads = c.threads();
    let took = t.elapsed().as_secs_f64();
    assert_eq!(threads.len(), 1);
    assert_eq!(threads[0].1.len(), n as usize - 1);
    assert_eq!(c.root(CommentId(n)), CommentId(1));
    assert!(took < 1.0, "threads() took {took:.2}s for a {n}-deep chain");
}

#[test]
fn comments_replying_to_each_other_are_one_thread() {
    let mut c = Comments::default();
    c.by_id.insert(CommentId(1), reply(1, Some(2)));
    c.by_id.insert(CommentId(2), reply(2, Some(1)));
    c.by_id.insert(CommentId(3), reply(3, Some(2)));
    assert_eq!(c.threads(), vec![(CommentId(1), vec![CommentId(2), CommentId(3)])]);
    assert_eq!(c.thread_count(), 1);
    assert!([1, 2, 3].iter().all(|&i| c.root(CommentId(i)) == CommentId(1)));
}
