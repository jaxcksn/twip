//! The text services follow the markup view (REV-1): find, the word count, spelling, the reading structure and the
//! input mirror act on the text the view shows; copy takes the accepted text in every view; and PDF export draws the
//! view's layout without the comment tints.

use std::path::Path;

use serde_json::Value;

use crate::docx::review::MarkupView;
use crate::docx::{Block, Document, Inline, Paragraph};
use crate::editing::Editor;
use crate::editing::commands::Command;
use crate::editing::tests::fonts;
use crate::editing::text::UnitText;
use crate::font::Fonts;
use crate::host::Engine;
use crate::layout::fragment::Item;
use crate::layout::{ParaId, Unit, traverse};

/// The views, with their `view.markup` arguments.
const VIEWS: [(MarkupView, &str); 4] =
    [(MarkupView::All, "all"), (MarkupView::Simple, "simple"), (MarkupView::NoMarkup, "none"), (MarkupView::Original, "original")];

fn bytes(name: &str) -> Vec<u8> {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/review").join(format!("{name}.docx"));
    std::fs::read(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

fn editor(name: &str, view: MarkupView) -> Editor {
    let mut e = Editor::open(&bytes(name), fonts()).expect("opens");
    e.set_markup_view(view);
    e
}

fn engine(name: &str, view: &str) -> Engine {
    let mut e = Engine::with_fonts(Fonts::system());
    e.open(&bytes(name));
    e.command("view.markup", Some(view));
    e
}

/// The body's paragraphs in document order, cells' included.
fn paragraphs(doc: &Document) -> Vec<&Paragraph> {
    fn walk<'d>(blocks: &'d [Block], out: &mut Vec<&'d Paragraph>) {
        for b in blocks {
            match b {
                Block::Para(p) => out.push(p),
                Block::Table(t) => t.rows.iter().flat_map(|r| &r.cells).for_each(|c| walk(&c.blocks, out)),
            }
        }
    }
    let mut out = Vec::new();
    doc.sections.iter().for_each(|s| walk(&s.blocks, &mut out));
    out
}

/// The words of a document read as it is, revisions and all.
fn words(doc: &Document) -> Vec<String> {
    let mut w: Vec<String> =
        paragraphs(doc).iter().flat_map(|p| UnitText::of(p).text.split_whitespace().map(String::from).collect::<Vec<_>>()).collect();
    w.sort();
    w
}

/// The document the view lays out as: every revision accepted, every one rejected, or (All markup) the document as
/// it is.
fn twin(view: MarkupView) -> Document {
    let name = match view {
        MarkupView::All => "FX-REVIEW-RUNS",
        MarkupView::Simple | MarkupView::NoMarkup => "FX-REVIEW-RUNS-ACCEPTED",
        MarkupView::Original => "FX-REVIEW-RUNS-REJECTED",
    };
    crate::docx::read(&bytes(name)).expect("a review fixture")
}

#[test]
fn find_skips_text_the_view_hides() {
    // Matches in All, Simple, No markup and Original: an insertion deleted again, deletions, insertions.
    let cases = [("accessibility tax.", [1, 0, 0, 0]), ("division date.", [2, 0, 0, 2]), ("brief usage.", [3, 3, 3, 0])];
    for (k, (view, _)) in VIEWS.into_iter().enumerate() {
        let e = editor("FX-REVIEW-RUNS", view);
        for (needle, counts) in cases {
            assert_eq!(e.count_matches(needle, false), counts[k], "{needle:?} in {view:?}");
            assert_eq!(e.find(needle, false, false).is_some(), counts[k] > 0, "{needle:?} found in {view:?}");
        }
    }
}

#[test]
fn word_count_leaves_out_hidden_words() {
    let all = words(&twin(MarkupView::All)).len();
    for (view, arg) in VIEWS {
        let mut e = engine("FX-REVIEW-RUNS", arg);
        // All markup counts the accepted text.
        let want = words(&twin(if view == MarkupView::All { MarkupView::Simple } else { view })).len();
        assert_eq!(e.status_numbers()[2], want, "{view:?}");
        assert!(want < all, "{view:?} leaves words out");
    }
}

#[test]
fn spelling_skips_hidden_and_deleted_text() {
    for (view, arg) in VIEWS {
        let mut e = engine("FX-REVIEW-RUNS", arg);
        let ed = e.editor().expect("a document");
        let mut masked = 0;
        for p in paragraphs(ed.document()) {
            let want: Vec<char> = p
                .inlines
                .iter()
                .flat_map(|i| match i {
                    Inline::Text(t, r) if !(r.hidden || r.rev.del.is_some() || view.hides(&r.rev)) => t.chars().collect::<Vec<_>>(),
                    other => vec![' '; other.units()],
                })
                .collect();
            masked += p.inlines.iter().filter(|i| i.run().is_some_and(|r| r.rev.del.is_some() && i.units() > 0)).count();
            assert_eq!(crate::host::spelling::check_text(ed, ParaId(p.id)), want, "{view:?}");
        }
        assert!(masked > 0, "the fixture has deletions");
    }
}

#[test]
fn copy_takes_the_accepted_text() {
    let copied = |mut e: Editor| {
        e.execute(Command::SelectAll, fonts());
        e.copy().expect("a copy")
    };
    let want = copied(editor("FX-REVIEW-RUNS-ACCEPTED", MarkupView::All)).text();
    for (view, _) in VIEWS {
        let f = copied(editor("FX-REVIEW-RUNS", view));
        assert_eq!(f.text(), want, "{view:?}");
        let marked = f.blocks.iter().any(|b| matches!(b, Block::Para(p) if p.inlines.iter().any(|i| i.run().is_some_and(|r| !r.rev.is_empty()))));
        assert!(!marked, "{view:?}: the copy keeps no revision marks");
    }
}

#[test]
fn pdf_shows_the_view_without_comment_tints() {
    for (view, _) in VIEWS {
        let mut e = editor("FX-REVIEW-RUNS", view);
        e.complete(fonts());
        let snapshot = e.snapshot();
        assert!(crate::pdf::export(e.document(), &snapshot, "Review").starts_with(b"%PDF-"));
        // The export draws these items, and only these.
        let runs: Vec<String> = snapshot
            .pages()
            .iter()
            .flat_map(|p| traverse::paint_items(p))
            .filter_map(|i| if let Item::Glyphs(r) = i { Some(r.chars.iter().map(|c| c.0).collect()) } else { None })
            .collect();
        let mut drawn: Vec<String> = runs.join(" ").split_whitespace().map(String::from).collect();
        drawn.sort();
        assert_eq!(drawn, words(&twin(view)), "{view:?}: the PDF's text is the view's");
    }
    let mut e = editor("FX-REVIEW-COMMENTS", MarkupView::All);
    e.complete(fonts());
    let pages = e.snapshot().pages().len();
    assert!(!e.view(0..pages).comments.is_empty(), "the editor tints the commented text");
    let tinted = e
        .snapshot()
        .pages()
        .iter()
        .flat_map(|p| traverse::paint_items(p))
        .any(|i| matches!(i, Item::Rect { colour, .. } if colour & 0xFF_FFFF == 0xFF_B900));
    assert!(!tinted, "no comment tint among the printed items");
}

/// Every review mark on the reading tree's lines: (kind, author).
fn tree_marks(e: &mut Engine) -> Vec<(String, String)> {
    fn walk(v: &Value, out: &mut Vec<(String, String)>) {
        for l in v["t"].as_array().into_iter().flatten() {
            for m in l["m"].as_array().into_iter().flatten() {
                out.push((m["k"].as_str().unwrap_or("").into(), m["a"].as_str().unwrap_or("").into()));
            }
        }
        v["c"].as_array().into_iter().flatten().for_each(|c| walk(c, out));
    }
    let tree: Value = serde_json::from_str(&e.accessible_tree()).expect("tree JSON");
    let mut out = Vec::new();
    walk(&tree, &mut out);
    out
}

#[test]
fn reading_structure_marks_revisions_and_comments_with_authors() {
    let has = |m: &[(String, String)], k: &str, a: &str| m.iter().any(|x| x.0 == k && x.1 == a);
    for (view, arg) in VIEWS {
        let m = tree_marks(&mut engine("FX-REVIEW-RUNS", arg));
        assert_eq!(has(&m, "insertion", "Author A"), view != MarkupView::Original, "{view:?}: insertions");
        assert_eq!(has(&m, "deletion", "Author B"), matches!(view, MarkupView::All | MarkupView::Original), "{view:?}: deletions");
    }
    let m = tree_marks(&mut engine("FX-REVIEW-COMMENTS", "all"));
    assert!(m.iter().any(|x| x.0 == "comment" && !x.1.is_empty()), "commented ranges with their authors: {m:?}");
}

#[test]
fn caret_entering_review_marks_announces_them() {
    let mut e = engine("FX-REVIEW-RUNS", "all");
    let ed = e.editor().expect("a document");
    let p = paragraphs(ed.document())[0];
    let (id, mut at, mut ins, mut del) = (p.id, 0u32, None, None);
    for i in &p.inlines {
        if let Some(r) = i.run().filter(|_| i.units() > 0) {
            match (&r.rev.ins, &r.rev.del) {
                (Some(_), None) => ins = ins.or(Some(at + 1)),
                (None, Some(_)) => del = del.or(Some(at + 1)),
                _ => {}
            }
        }
        at += i.units() as u32;
    }
    let caret = |e: &mut Engine, u: u32| {
        e.command("edit.select-range", Some(&format!("{id},{u},{u}")));
        e.take_announcement()
    };
    caret(&mut e, 0);
    assert_eq!(caret(&mut e, ins.expect("an insertion")).as_deref(), Some("insertion by Author A"));
    assert_eq!(caret(&mut e, del.expect("a deletion")).as_deref(), Some("deletion by Author B"));
    assert_eq!(caret(&mut e, 0), None, "leaving says nothing");
}

#[test]
fn input_mirror_shows_the_view() {
    let text = |view: &str| {
        let mut e = engine("FX-REVIEW-RUNS", view);
        let ed = e.editor().expect("a document");
        let id = paragraphs(ed.document())[0].id;
        e.command("edit.select-range", Some(&format!("{id},0,0")));
        let v: Value = serde_json::from_str(&e.input_mirror()).expect("mirror JSON");
        v["text"].as_str().unwrap_or("").to_string()
    };
    let (all, simple) = (text("all"), text("simple"));
    assert!(all.contains("distribution filed logistics."), "All markup shows the deletion");
    assert!(!simple.contains("distribution filed logistics."), "Simple markup doesn't");
    assert_eq!(all.chars().count(), simple.chars().count(), "offsets stay the paragraph's");
    assert_eq!(UnitText::of_view(paragraphs(&twin(MarkupView::All))[0], MarkupView::Simple).len(), Unit(simple.chars().count() as u32));
}
