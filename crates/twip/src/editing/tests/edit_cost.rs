//! Work per edit on large documents

use std::time::Instant;

use super::super::commands::{Command, Dir, MoveBy};
use super::super::selection::Selection;
use super::{blank_body, blank_with, del, fonts, mv};
use crate::docx::{Block, Document, Inline};
use crate::editing::Editor;
use crate::editing::text::UnitText;
use crate::layout::{ParaId, Unit};
use crate::services::spelling::{Speller, Spellers};

/// The fastest of five runs, in ms.
fn ms(mut f: impl FnMut()) -> f64 {
    (0..5)
        .map(|_| {
            let t = Instant::now();
            f();
            t.elapsed().as_secs_f64() * 1e3
        })
        .fold(f64::MAX, f64::min)
}

/// How many times `f` ran the profiled span `name`.
fn calls(name: &str, f: impl FnOnce()) -> u64 {
    crate::prof::take();
    f();
    crate::prof::take().iter().find(|t| t.0 == name).map_or(0, |t| t.2)
}

fn editor(body: &str) -> Editor {
    let mut e = Editor::open(&blank_body(body), fonts()).expect("opens");
    e.complete(fonts());
    e
}

fn document(body: &str) -> Document {
    crate::docx::read_editable(&blank_body(body)).expect("reads")
}

fn dict(name: &str) -> Vec<u8> {
    std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/dictionaries").join(name)).expect("dictionary")
}

fn spellers() -> Spellers {
    let mut sp = Spellers::default();
    sp.add_dictionary("en-US", &dict("en_US.aff"), &dict("en_US.dic")).expect("loads");
    sp
}

fn paras(n: usize) -> String {
    (0..n).map(|i| format!("<w:p><w:r><w:t>w{i}x text</w:t></w:r></w:p>")).collect()
}

fn first_para(doc: &Document) -> &crate::docx::Paragraph {
    match &doc.sections[0].blocks[0] {
        Block::Para(p) => p,
        _ => panic!("a paragraph"),
    }
}

// Spelling

#[test]
fn spelling_count_after_an_edit_checks_only_the_edited_paragraph() {
    let mut e = editor(&paras(3000));
    let mut sp = spellers();
    assert_eq!(calls("spelling:paragraph", || _ = sp.count(&e, "en-US")), 3000, "the first count checks every paragraph");
    e.execute(Command::InsertText("a".into()), fonts());
    assert_eq!(calls("spelling:paragraph", || _ = sp.count(&e, "en-US")), 1, "then only the edited one");
    e.execute(mv(MoveBy::Char, Dir::Forward, false), fonts());
    assert_eq!(calls("spelling:paragraph", || _ = sp.count(&e, "en-US")), 0, "and none for a caret move");

    // What it counts follows edits, undo and the markup view.
    let mut e = editor(&paras(4));
    let fresh = |e: &Editor| spellers().count(e, "en-US");
    let before = sp.count(&e, "en-US");
    assert_eq!(before, fresh(&e));
    e.execute(Command::InsertText("qwzx ".into()), fonts());
    assert_eq!(sp.count(&e, "en-US"), before + 1, "the new misspelling");
    e.execute(Command::SetTrackChanges(true), fonts());
    e.execute(Command::InsertText("zqxw ".into()), fonts());
    assert_eq!(sp.count(&e, "en-US"), before + 2);
    e.set_markup_view(crate::docx::review::MarkupView::Original);
    assert_eq!(sp.count(&e, "en-US"), before + 1, "the original hides the tracked insertion");
    e.set_markup_view(crate::docx::review::MarkupView::All);
    e.execute(Command::SetTrackChanges(false), fonts());
    e.execute(Command::SelectAll, fonts());
    e.execute(del(Dir::Forward, false), fonts());
    assert_eq!(sp.count(&e, "en-US"), fresh(&e), "what the deletion leaves");
    e.execute(Command::Undo, fonts());
    assert_eq!((sp.count(&e, "en-US"), fresh(&e)), (before + 2, before + 2), "undo brings the paragraphs back");
}

#[test]
fn speller_is_linear_in_a_long_token() {
    let sp = Speller::new("en-US", &dict("en_US.aff"), &dict("en_US.dic")).expect("loads");
    // 20000 words between two spaces: seconds when each word scans and copies the whole token.
    let text: Vec<char> = "a.".repeat(20_000).chars().collect();
    let t = Instant::now();
    assert!(sp.errors(&text).is_empty());
    let took = t.elapsed().as_secs_f64() * 1e3;
    assert!(took < 1000.0, "{took:.1} ms for 20000 words in one token");

    // Addresses are skipped whole, other tokens word by word.
    let text: Vec<char> = "see www.exmple.com, WWW.qwzx.org and nmae@stroe.com or http://qzx.io/zqx then dog.qwzx.cat".chars().collect();
    let words: Vec<String> = sp.errors(&text).iter().map(|e| text[e.start..e.end()].iter().collect()).collect();
    assert_eq!(words, ["qwzx"]);
}

// Block selection

fn block(text: &str, lines: usize) -> Editor {
    let mut e = Editor::open(&blank_with(text), fonts()).expect("opens");
    e.complete(fonts());
    e.block_mode();
    for _ in 0..3 {
        e.execute(mv(MoveBy::Char, Dir::Forward, true), fonts());
    }
    for _ in 0..lines {
        e.execute(mv(MoveBy::Line, Dir::Forward, true), fonts());
    }
    assert!(matches!(e.selection, Selection::Block { .. }));
    e
}

#[test]
fn block_text_does_not_rebuild_the_paragraph_per_line() {
    let mut e = block(&"abcdefghi ".repeat(2000), 30);
    let mut text = None;
    assert_eq!(calls("block:paragraph", || text = e.selected_text_within(usize::MAX)), 1, "31 lines of one paragraph");
    let text = text.expect("the block's text");
    let lines: Vec<&str> = text.split('\n').collect();
    assert_eq!(lines.len(), 31, "{text:?}");
    assert!(lines.iter().all(|l| !l.is_empty() && *l == lines[0]), "the same columns of each line: {text:?}");
    let n = text.chars().count();
    assert_eq!(e.selected_text_within(n).as_deref(), Some(text.as_str()));
    assert_eq!(e.selected_text_within(n - 1), None, "nothing past the limit");

    // A block that starts pages in.
    let words: String = (0..4000).map(|k| format!("w{:04} ", k % 10_000)).collect();
    let mut e = Editor::open(&blank_with(&words), fonts()).expect("opens");
    e.complete(fonts());
    let at = crate::layout::Unit(4000 * 6 - 30 * 6);
    let para = e.document().sections[0].blocks.iter().find_map(|b| if let Block::Para(p) = b { Some(ParaId(p.id)) } else { None });
    e.select_range(para.expect("a paragraph"), at, at, fonts());
    e.block_mode();
    e.execute(mv(MoveBy::Char, Dir::Forward, true), fonts());
    e.execute(mv(MoveBy::Line, Dir::Forward, true), fonts());
    let text = e.selected_text_within(usize::MAX).expect("the block's text");
    assert_eq!(text.split('\n').count(), 2, "{text:?}");
    assert!(text.split('\n').all(|l| l.chars().count() == 1), "{text:?}");
}

// Words

fn field_words(fields: usize) -> String {
    let field = "<w:fldSimple w:instr=\" PAGEREF x \"><w:r><w:t>5</w:t></w:r></w:fldSimple>";
    format!("<w:p>{}</w:p>", format!("{field}<w:r><w:t xml:space=\"preserve\"> a b </w:t></w:r>").repeat(fields))
}

#[test]
fn word_movement_is_linear_in_fields() {
    // 8000 fields among 24000 words: seconds when each field is merged into the word list on its own.
    let doc = document(&field_words(8000));
    let t = UnitText::of(first_para(&doc));
    let took = ms(|| {
        t.next_word(Unit(0));
    });
    assert!(took < 1000.0, "{took:.1} ms for a word step past 8000 fields");

    // A field shown as its result is one piece: the stops are each field, then "a" and "b".
    let doc = document(&field_words(2));
    let t = UnitText::of(first_para(&doc));
    let mut stops = vec![Unit(0)];
    while let Some(&u) = stops.last().filter(|u| **u < t.len()) {
        stops.push(t.next_word(u));
    }
    let words: Vec<(Unit, Unit)> = stops[..stops.len() - 1].iter().map(|&u| t.word_at(u)).collect();
    let field = t.word_at(Unit(0));
    assert_eq!(words.len(), 6, "{words:?} in {:?}", t.text);
    assert_eq!((words[0], words[3].1.0 - words[3].0.0), (field, field.1.0 - field.0.0), "{words:?}");
    assert_eq!(t.prev_word(words[3].0), words[2].0);
}

// Index

#[test]
fn removing_tables_does_not_scan_every_element() {
    use crate::editing::index::{Container, DocIndex};
    let time = |fields: usize| {
        let tbl = "<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w=\"2000\"/></w:tblGrid><w:tr><w:tc><w:p/></w:tc></w:tr></w:tbl><w:p/>";
        let link = "<w:fldSimple w:instr=\" PAGEREF x \"><w:r><w:t>5</w:t></w:r></w:fldSimple>";
        let doc = document(&format!("{}<w:p>{}</w:p>", tbl.repeat(1000), link.repeat(fields)));
        let removed: Vec<Block> = doc.sections[0].blocks.iter().filter(|b| matches!(b, Block::Table(_))).cloned().collect();
        assert_eq!(removed.len(), 1000);
        let mut idx = DocIndex::build(&doc);
        ms(|| idx.replaced_blocks(&doc, Container::Section(0), &removed))
    };
    // A thousand tables go: hundreds of ms when each one scans the 5000 fields' locations.
    let (none, fields) = (time(0), time(5000));
    assert!(fields < none * 2.0 + 20.0, "{none:.2} ms with no fields, {fields:.2} ms with 5000");

    // Row and cell controls leave the index with their rows: every edit checks it against a fresh build.
    let sdt = |k: usize, content: &str| format!("<w:sdt><w:sdtPr><w:id w:val=\"{k}\"/></w:sdtPr><w:sdtContent>{content}</w:sdtContent></w:sdt>");
    let cell = |k: usize| sdt(k, &format!("<w:tc><w:p><w:r><w:t>c{k}</w:t></w:r></w:p></w:tc>"));
    let row = |k: usize| sdt(k, &format!("<w:tr>{}{}</w:tr>", cell(k + 1), cell(k + 2)));
    let grid = "<w:tblGrid><w:gridCol w:w=\"2000\"/><w:gridCol w:w=\"2000\"/></w:tblGrid>";
    let table = |k: usize| format!("<w:tbl><w:tblPr/>{grid}{}{}</w:tbl><w:p/>", row(k), row(k + 10));
    let mut e = editor(&format!("<w:p><w:r><w:t>top</w:t></w:r></w:p>{}{}", table(100), table(200)));
    let tables = |e: &Editor| e.document().sections[0].blocks.iter().filter(|b| matches!(b, Block::Table(_))).count();
    let Some(Block::Table(t)) = e.document().sections[0].blocks.get(1) else { panic!("a table") };
    assert!(!t.rows[0].controls.is_empty() && !t.rows[0].cells[0].controls.is_empty(), "row and cell controls are read");
    e.execute(Command::SelectAll, fonts());
    e.execute(del(Dir::Forward, false), fonts());
    assert_eq!(tables(&e), 0);
    e.execute(Command::Undo, fonts());
    e.execute(Command::Redo, fonts());
    e.execute(Command::Undo, fonts());
    assert_eq!(tables(&e), 2);
}

#[test]
fn typing_beside_text_boxes_leaves_their_stories_indexed_as_they_are() {
    use crate::editing::index::DocIndex;
    use crate::editing::step::{Edges, Step};
    use crate::layout::tests::shapes::{anchor, bytes, sp};
    let story = r#"<wps:txbx><w:txbxContent><w:p><w:r><w:t>Inside</w:t></w:r></w:p></w:txbxContent></wps:txbx><wps:bodyPr/>"#;
    let boxed = sp(0, 0, 40, 20, "rect", "", "FFFFFF").replace("<wps:bodyPr/>", story);
    let time = |inlines: &str, stories: usize| {
        let body = format!("<w:p><w:r><w:t>words</w:t></w:r>{inlines}</w:p>");
        let mut doc = crate::docx::read_editable(&bytes(&body).expect("a package")).expect("reads");
        let p = first_para(&doc);
        assert_eq!(p.inlines.iter().filter(|i| i.story().is_some()).count(), stories, "the text boxes are read");
        let (para, run) = (ParaId(p.id), p.inlines.iter().find_map(|i| i.run().cloned()).expect("a run"));
        let mut idx = DocIndex::build(&doc);
        let t = ms(|| {
            for _ in 0..20 {
                let s = Step::Text { para, range: Unit(0)..Unit(0), with: vec![Inline::Text("a".into(), run.clone())], edges: Edges::default() };
                s.apply(&mut doc, &mut idx).expect("applies");
            }
        });
        assert_eq!(idx, DocIndex::build(&doc), "the same index as a fresh build");
        t
    };
    // As many inlines each time: tabs, or text boxes, whose stories a keystroke copied and indexed again.
    let boxes: String = (0..1500).map(|k| anchor(72, 72, 40, 20, k + 1, false, k + 1, &boxed)).collect();
    let (tabs, boxes) = (time(&"<w:r><w:tab/></w:r>".repeat(1500), 0), time(&boxes, 1500));
    assert!(boxes < tabs * 2.0 + 2.0, "{tabs:.2} ms beside 1500 tabs, {boxes:.2} ms beside 1500 text boxes");
}

// Session state

fn session(body: &str) -> (crate::api::engine::Engine, crate::api::engine::SessionId) {
    use crate::api::engine::{Engine, SessionOptions};
    let mut engine = Engine::new(crate::api::fonts::FontLibrary::system_recommended());
    let id = engine.create_session(SessionOptions::default());
    engine.session(id).expect("a session").open(&blank_body(body)).expect("opens");
    (engine, id)
}

/// The spans named over ten keystrokes: their calls and mean ms.
fn keystrokes<const N: usize>(s: &mut crate::api::session::Session, spans: [&str; N]) -> [(u64, f64); N] {
    s.run(Command::InsertText("a".into())).expect("types");
    crate::prof::take();
    for _ in 0..10 {
        s.run(Command::InsertText("a".into())).expect("types");
    }
    let took = crate::prof::take();
    spans.map(|name| took.iter().find(|t| t.0 == name).map_or((0, 0.0), |t| (t.2, t.1 / t.2 as f64)))
}

#[test]
fn keystrokes_do_not_compare_every_comment() {
    use crate::docx::review::{AuthorId, Comment, CommentId};
    let text = "comment body ".repeat(40);
    let with = |n: i64| {
        let (mut engine, id) = session(&paras(3));
        let s = engine.session(id).expect("a session");
        let body = Editor::open(&blank_with(&text), fonts()).expect("opens").document().sections[0].blocks.clone();
        s.with_editor(|e| {
            for k in 0..n {
                let c = Comment {
                    id: CommentId(k),
                    author: AuthorId(0),
                    initials: None,
                    date: None,
                    date_utc: None,
                    body: body.clone(),
                    para_id: k as u32,
                    durable_id: None,
                    parent: None,
                    done: false,
                    extensible: Default::default(),
                };
                e.doc.comments.by_id.insert(c.id, c);
            }
        });
        let typed = keystrokes(s, ["comments:ranges", "session:review"]);
        let body = crate::api::review::Body::Text("Check.".into());
        let added = calls("comments:ranges", || {
            s.run(Command::AddComment(body)).expect("comments");
            s.run(mv(MoveBy::Char, Dir::Back, false)).expect("moves");
        });
        (typed, added)
    };
    let (([_, (_, review)], _), ([(ranges, _), (events, many)], added)) = (with(0), with(5000));
    assert_eq!(ranges, 0, "typing where no comment is keeps the threads' ranges");
    assert_eq!((events, added), (10, 1), "a comment added makes them again");
    // Comparing 5000 comments of 500 characters takes ms.
    assert!(many < review * 2.0 + 0.2, "review events {review:.3} ms without comments, {many:.3} ms with 5000");
}

#[test]
fn state_build_after_a_keystroke_does_not_walk_the_document() {
    let (mut engine, id) = session(&paras(2000));
    let [(walks, _)] = keystrokes(engine.session(id).expect("a session"), ["review:revisions"]);
    assert_eq!(walks, 0, "no revisions to count");

    // The revision count follows the edits that make or join revisions.
    let (mut engine, id) = session(&paras(3));
    let s = engine.session(id).expect("a session");
    let fresh =
        |s: &mut crate::api::session::Session| s.with_editor(|e| crate::editing::revisions::index::revisions(e.document(), e.index()).len() as u32);
    let count = |s: &crate::api::session::Session| s.state().review.revisions;
    s.run(Command::InsertText("plain ".into())).expect("types");
    assert_eq!(count(s), 0);
    s.run(Command::SetTrackChanges(true)).expect("tracks");
    s.run(Command::InsertText("one".into())).expect("types");
    assert_eq!(count(s), 1);
    // Past the paragraph's own "w0x ".
    for _ in 0..4 {
        s.run(mv(MoveBy::Char, Dir::Forward, false)).expect("moves");
    }
    s.run(Command::InsertText("two".into())).expect("types");
    assert_eq!((count(s), fresh(s)), (2, 2), "two insertions with untracked text between");
    s.run(Command::SetTrackChanges(false)).expect("stops");
    assert_eq!(calls("review:revisions", || _ = s.run(Command::InsertText("x".into())).expect("types")), 1, "with revisions, counted again");
    s.run(del(Dir::Back, false)).expect("deletes");
    for _ in 0..3 {
        s.run(mv(MoveBy::Char, Dir::Back, false)).expect("moves");
    }
    for _ in 0..4 {
        s.run(del(Dir::Back, false)).expect("deletes");
    }
    assert_eq!((count(s), fresh(s)), (1, 1), "deleting the untracked text between them joins them");
    s.run(Command::Undo).expect("undoes");
    assert_eq!(count(s), fresh(s));
    s.run(mv(MoveBy::Story, Dir::Forward, false)).expect("moves");
    let typed = calls("review:revisions", || _ = s.run(Command::InsertText("x".into())).expect("types"));
    assert_eq!((typed, count(s), fresh(s)), (0, 2, 2), "typing in a paragraph without marks keeps the count");
}
