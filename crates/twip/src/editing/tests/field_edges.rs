//! Fields across paragraphs: copies, codes, undo, saving and layout

use super::super::Editor;
use super::super::commands::{Command, Dir, MoveBy, Outcome, moving};
use super::super::toc::{TocFrame, TocLook, TocOptions};
use super::toc::{doc_with, para};
use super::{blank_body, fonts, texts, texts_ids};
use crate::docx::{Block, FieldEdge, FieldView, Inline, Paragraph};
use crate::layout::Unit;

const HEADINGS: &[&str] = &["Heading1", "Heading2", "Heading3"];

fn begin(code: &str) -> String {
    format!(
        r#"<w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve">{code}</w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r>"#
    )
}

fn run(t: &str) -> String {
    format!(r#"<w:r><w:t xml:space="preserve">{t}</w:t></w:r>"#)
}

const END: &str = r#"<w:r><w:fldChar w:fldCharType="end"/></w:r>"#;

const CODE: &str = r#" TOC \o "1-3" "#;

/// A table of contents over "One" and "Two", its end in a paragraph of its own.
fn toc() -> String {
    format!("<w:p>{}{}</w:p><w:p>{}</w:p><w:p>{END}</w:p>", begin(CODE), run("One"), run("Two"))
}

fn paras(blocks: &[Block]) -> Vec<&Paragraph> {
    blocks.iter().filter_map(|b| if let Block::Para(p) = b { Some(p) } else { None }).collect()
}

fn edges_of(blocks: &[Block]) -> Vec<FieldEdge> {
    paras(blocks).iter().flat_map(|p| &p.inlines).filter_map(|i| crate::docx::fields::span_edge(i).cloned()).collect()
}

fn body(e: &Editor) -> &[Block] {
    &e.document().sections[0].blocks
}

fn show_codes(e: &mut Editor, show: bool) -> Outcome {
    e.execute(Command::ShowFieldCodes(crate::api::command::ShowFieldCodes { show, ids: None }), fonts())
}

fn views(e: &Editor) -> Vec<FieldView> {
    edges_of(body(e)).into_iter().filter_map(|x| if let FieldEdge::Start { view, .. } = x { Some(view) } else { None }).collect()
}

fn caret(e: &Editor) -> Option<(crate::layout::ParaId, Unit)> {
    e.selection().focus().and_then(|p| p.para_offset())
}

fn step(e: &mut Editor, dir: Dir) {
    e.execute(moving(MoveBy::Char, dir, false), fonts());
}

/// The text drawn on the first page of `d`, a string per line.
fn lines_of(d: &crate::docx::Document) -> Vec<String> {
    let snap = crate::layout::layout(d, fonts());
    let mut runs: Vec<(f64, f64, String)> = crate::layout::traverse::paint_items(&snap.pages()[0])
        .iter()
        .filter_map(|it| match it {
            crate::layout::fragment::Item::Glyphs(g) => Some((g.baseline, g.x, g.chars.iter().map(|c| c.0).collect::<String>())),
            _ => None,
        })
        .collect();
    runs.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
    let mut out: Vec<(f64, String)> = Vec::new();
    for (y, _, t) in runs {
        match out.last_mut() {
            Some(l) if (l.0 - y).abs() < 0.5 => l.1.push_str(&t),
            _ => out.push((y, t)),
        }
    }
    out.into_iter().map(|l| l.1).filter(|l| !l.is_empty()).collect()
}

#[test]
fn a_copy_over_a_whole_table_keeps_both_its_edges() {
    let mut e = Editor::open(&doc_with(&[para("Normal", "Before"), toc(), para("Heading1", "After")].concat(), HEADINGS), fonts()).unwrap();
    let ps = texts_ids(&e);
    e.select_between((ps[0], Unit(1)), (ps[4], Unit(1)), fonts());
    e.execute(Command::Copy, fonts());
    let f = e.clipboard().expect("copied");
    let edges = edges_of(&f.blocks);
    assert!(matches!(edges.as_slice(), [FieldEdge::Start { .. }, FieldEdge::End]), "{edges:?}");
}

#[test]
fn a_copy_of_a_deleted_table_holds_neither_edge() {
    let mut e = Editor::open(&doc_with(&[para("Normal", "Before"), toc(), para("Heading1", "After")].concat(), HEADINGS), fonts()).unwrap();
    e.execute(Command::SetTrackChanges(true), fonts());
    let ps = texts_ids(&e);
    e.select_between((ps[1], Unit(0)), (ps[3], Unit(0)), fonts());
    e.execute(super::del(Dir::Back, false), fonts());
    assert!(edges_of(body(&e)).len() == 2, "the deletion keeps the edges, marked");
    e.select_between((ps[0], Unit(1)), (*texts_ids(&e).last().unwrap(), Unit(1)), fonts());
    e.execute(Command::Copy, fonts());
    let f = e.clipboard().expect("copied");
    assert!(edges_of(&f.blocks).is_empty(), "accepted, the deleted field goes: {:?}", edges_of(&f.blocks));
}

#[test]
fn rejecting_an_inserted_table_showing_its_code_takes_it_whole() {
    let mut e = Editor::open(&doc_with(&[para("Heading1", "Alpha"), para("Heading2", "Beta")].concat(), HEADINGS), fonts()).unwrap();
    let before = texts(&e);
    e.execute(Command::SetTrackChanges(true), fonts());
    let first = texts_ids(&e)[0];
    e.select_between((first, Unit(0)), (first, Unit(0)), fonts());
    e.insert_table_of_contents(&TocOptions::word_default(), TocLook::default(), &TocFrame::default(), fonts());
    show_codes(&mut e, true);
    assert!(views(&e).contains(&FieldView::Code));
    let out = e.execute(Command::RejectAllRevisions(Default::default()), fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    assert!(e.tables_of_contents().is_empty(), "{:?}", edges_of(body(&e)));
    assert_eq!(texts(&e), before);
    assert!(body(&e).iter().all(|b| !matches!(b, Block::Para(p) if crate::docx::fields::folded(p))));
}

#[test]
fn rejecting_part_of_an_insertion_after_a_start_at_a_paragraphs_start() {
    let ins = r#"<w:ins w:id="3" w:author="A" w:date="2024-01-01T00:00:00Z"><w:r><w:t>One</w:t></w:r></w:ins>"#;
    let doc = format!("<w:p>{}{ins}</w:p><w:p>{}</w:p><w:p>{END}</w:p>", begin(CODE), run("Two"));
    let mut e = Editor::open(&doc_with(&doc, HEADINGS), fonts()).unwrap();
    let p = texts_ids(&e)[0];
    e.select_between((p, Unit(0)), (p, Unit(2)), fonts());
    assert!(matches!(e.execute(Command::RejectRevisionsInSelection, fonts()), Outcome::Changed(_)));
    assert_eq!(texts(&e)[0], "e");
    assert_eq!(edges_of(body(&e)).len(), 2);
}

#[test]
fn hiding_a_code_leaves_the_fold_of_a_field_starting_at_its_end() {
    let doc = format!(
        "<w:p>{}{}</w:p><w:p>{}</w:p><w:p>{END}{}{}</w:p><w:p>{}</w:p><w:p>{END}</w:p>",
        begin(CODE),
        run("One"),
        run("Two"),
        begin(r#" TOC \o "1-2" "#),
        run("Three"),
        run("Four")
    );
    let mut e = Editor::open(&doc_with(&doc, HEADINGS), fonts()).unwrap();
    show_codes(&mut e, true);
    let folded = |e: &Editor| paras(body(e)).iter().map(|p| crate::docx::fields::folded(p)).collect::<Vec<_>>();
    assert_eq!(folded(&e), [true, true, true, true, false]);
    let first = e.span_fields().into_iter().find(|f| f.start.0 == 0).expect("the first table");
    // In a batch: the view's own steps, not shown again from scratch
    e.begin_batch(None);
    assert!(matches!(e.set_span_views(&[first], false, fonts()), Outcome::Changed(_)));
    e.end_batch(true, fonts());
    assert_eq!(views(&e), [FieldView::Result, FieldView::Code]);
    assert_eq!(folded(&e), [false, false, true, true, false], "the second table's folds stay");
}

/// One paragraph holding two fields across paragraphs after their paragraphs were joined, each showing its code:
/// `{code}One[end]Mid{code}Two[end]Tail`.
fn joined_codes() -> Editor {
    let doc = format!(
        "<w:p>{}{}</w:p><w:p>{END}{}{}{}</w:p><w:p>{END}{}</w:p>",
        begin(CODE),
        run("One"),
        run("Mid"),
        begin(r#" TOC \o "1-2" "#),
        run("Two"),
        run("Tail")
    );
    let mut e = Editor::open(&doc_with(&doc, HEADINGS), fonts()).unwrap();
    for _ in 0..2 {
        let ps = texts_ids(&e);
        let len = e.index.para(e.document(), ps[0]).map(super::super::step::para_len).unwrap();
        e.select_between((ps[0], len), (ps[1], Unit(0)), fonts());
        e.execute(super::del(Dir::Forward, false), fonts());
    }
    assert_eq!(texts_ids(&e).len(), 1, "{:?}", texts(&e));
    show_codes(&mut e, true);
    assert_eq!(views(&e), [FieldView::Code, FieldView::Code]);
    e
}

/// The units of the braces of the first paragraph: each opening one's and closing one's.
fn braces(e: &Editor) -> (Vec<u32>, Vec<u32>) {
    let (mut open, mut close, mut at) = (Vec::new(), Vec::new(), 0u32);
    for i in &paras(body(e))[0].inlines {
        match crate::docx::fields::span_edge(i) {
            Some(FieldEdge::Start { view: FieldView::Code, .. }) => open.push(at),
            Some(FieldEdge::Code) => close.push(at),
            _ => {}
        }
        at += i.units() as u32;
    }
    (open, close)
}

#[test]
fn enter_in_a_second_shown_code_of_a_paragraph_leaves_it_whole() {
    let mut e = joined_codes();
    let p = texts_ids(&e)[0];
    let at = braces(&e).0[1];
    e.select_between((p, Unit(at + 3)), (p, Unit(at + 3)), fonts());
    assert!(matches!(e.execute(Command::InsertParagraph, fonts()), Outcome::Unchanged(_)));
    assert_eq!(texts_ids(&e).len(), 1);
}

#[test]
fn right_from_a_fields_end_steps_on_into_what_follows() {
    let mut e = joined_codes();
    let p = texts_ids(&e)[0];
    // The first field's end: after its closing brace and its hidden "One".
    let close = braces(&e).1[0];
    let end = close + 1 + 3;
    e.select_between((p, Unit(end)), (p, Unit(end)), fonts());
    step(&mut e, Dir::Forward);
    assert_eq!(caret(&e), Some((p, Unit(end + 1))), "into \"Mid\"");
    step(&mut e, Dir::Back);
    step(&mut e, Dir::Back);
    assert_eq!(caret(&e), Some((p, Unit(close))), "back before the closing brace");
}

#[test]
fn left_after_nested_ends_reaches_the_shown_codes_brace() {
    let doc = format!(
        "<w:p>{}{}</w:p><w:p>{}{}{}</w:p><w:p>{END}{END}{}</w:p>",
        begin(CODE),
        run("One"),
        run("Two"),
        begin(r#" TOC \o "1-2" "#),
        run("Inner"),
        run("After")
    );
    let mut e = Editor::open(&doc_with(&doc, HEADINGS), fonts()).unwrap();
    let outer = e.span_fields().into_iter().find(|f| f.start.0 == 0).expect("the outer table");
    e.set_span_views(&[outer], true, fonts());
    assert_eq!(views(&e), [FieldView::Code, FieldView::Result]);
    let ps = texts_ids(&e);
    e.select_between((ps[2], Unit(0)), (ps[2], Unit(0)), fonts());
    step(&mut e, Dir::Back);
    assert_eq!(caret(&e), Some((ps[0], Unit(CODE.chars().count() as u32 + 1))), "before the outer code's closing brace");
}

#[test]
fn undoing_a_join_before_a_shown_code_keeps_it_shown() {
    let mut e = Editor::open(&doc_with(&[para("Normal", "Lead"), toc()].concat(), HEADINGS), fonts()).unwrap();
    let ps = texts_ids(&e);
    e.select_between((ps[0], Unit(4)), (ps[1], Unit(0)), fonts());
    e.execute(super::del(Dir::Forward, false), fonts());
    let starts = |e: &Editor| {
        paras(body(e)).iter().position(|p| p.inlines.iter().any(|i| matches!(crate::docx::fields::span_edge(i), Some(FieldEdge::Start { .. }))))
    };
    assert_eq!(starts(&e), Some(0), "the join took the start to the first paragraph");
    show_codes(&mut e, true);
    e.execute(Command::Undo, fonts());
    assert_eq!(starts(&e), Some(1));
    assert_eq!(views(&e), [FieldView::Code], "the split off paragraph's field still shows its code");
    e.execute(Command::Redo, fonts());
    assert_eq!(starts(&e), Some(0));
    assert_eq!(views(&e), [FieldView::Code]);
}

#[test]
fn a_made_end_saves_after_its_paragraph_is_edited() {
    // The end's run holds text too: reading makes its edge.
    let doc = format!(
        r#"<w:p>{}{}</w:p><w:p><w:r><w:t>Two</w:t><w:fldChar w:fldCharType="end"/></w:r></w:p>{}"#,
        begin(CODE),
        run("One"),
        para("Heading1", "After")
    );
    let mut e = Editor::open(&doc_with(&doc, HEADINGS), fonts()).unwrap();
    assert_eq!(edges_of(body(&e)).len(), 2);
    let ps = texts_ids(&e);
    e.select_between((ps[1], Unit(1)), (ps[1], Unit(1)), fonts());
    e.execute(Command::InsertText("x".into()), fonts());
    let saved = crate::docx::write::save(e.document()).unwrap();
    let xml = {
        use std::io::Read;
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(&saved)).unwrap();
        let mut s = String::new();
        z.by_name("word/document.xml").unwrap().read_to_string(&mut s).unwrap();
        s
    };
    let n = |t: &str| xml.matches(&format!("fldCharType=\"{t}\"")).count();
    assert_eq!((n("begin"), n("separate"), n("end")), (1, 1, 1), "{xml}");
    let again = Editor::open(&saved, fonts()).unwrap();
    assert_eq!(again.tables_of_contents().len(), 1);
}

#[test]
fn a_relative_page_reference_in_an_if_compares_as_above() {
    let doc = format!(
        r#"<w:p><w:bookmarkStart w:id="1" w:name="Mark"/>{}<w:bookmarkEnd w:id="1"/></w:p><w:p>{}{}{}{}{END}<w:r><w:instrText xml:space="preserve"> = "above" "yes" "no" </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r>{}{END}</w:p>"#,
        run("Target"),
        run("R:"),
        r#"<w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> IF </w:instrText></w:r>"#,
        begin(r" PAGEREF Mark \p "),
        run("zzz"),
        run("cached")
    );
    let e = Editor::open(&blank_body(&doc), fonts()).unwrap();
    let lines = lines_of(e.document());
    assert!(lines.iter().any(|l| l == "R:yes"), "{lines:?}");
}

#[test]
fn a_reference_inside_its_bookmark_reads_as_an_error() {
    let field = |code: &str| format!("{}{}{END}", begin(code), run("?"));
    let doc = format!(
        r#"<w:p>{}<w:bookmarkStart w:id="1" w:name="Mark"/>{}{}<w:bookmarkEnd w:id="1"/>{}</w:p>"#,
        field(r" REF Mark \p "),
        run("here"),
        field(r" REF Mark \p "),
        field(r" REF Mark \p ")
    );
    let e = Editor::open(&blank_body(&doc), fonts()).unwrap();
    let values: Vec<String> = paras(body(&e))
        .iter()
        .flat_map(|p| &p.inlines)
        .filter_map(|i| if let Inline::FieldBegin { value: Some(v), .. } = i { Some(v.clone()) } else { None })
        .collect();
    assert_eq!(values, ["below", "Error! Not a valid bookmark self-reference.", "above"]);
}

/// An empty field across paragraphs before a table, its end's paragraph mark deleted.
fn empty_before_table() -> Editor {
    let deleted_mark = r#"<w:pPr><w:rPr><w:del w:id="5" w:author="A" w:date="2024-01-01T00:00:00Z"/></w:rPr></w:pPr>"#;
    let doc = format!(
        r#"<w:p>{}</w:p><w:p>{deleted_mark}{END}</w:p><w:tbl><w:tblGrid><w:gridCol w:w="4000"/></w:tblGrid><w:tr><w:tc><w:p>{}</w:p></w:tc></w:tr></w:tbl><w:p/>"#,
        r#"<w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r>"#,
        run("Cell")
    );
    Editor::open(&blank_body(&doc), fonts()).unwrap()
}

#[test]
fn an_empty_codes_braces_draw_before_a_table_its_deleted_paragraphs_hide() {
    let mut e = empty_before_table();
    assert_eq!(edges_of(body(&e)).len(), 2);
    let in_view = |e: &Editor| {
        let mut d = e.document().clone();
        d.review.view = crate::docx::review::MarkupView::NoMarkup;
        lines_of(&d)
    };
    assert_eq!(in_view(&e), ["Cell"], "the empty field shows nothing: its paragraphs aren't laid out");
    show_codes(&mut e, true);
    assert_eq!(views(&e), [FieldView::Code]);
    let lines = in_view(&e);
    assert!(lines.len() == 2 && lines[0].contains('{') && lines[0].contains('}') && lines[1] == "Cell", "{lines:?}");
}

#[test]
fn a_shown_codes_inserted_braces_take_the_insertions_look() {
    use crate::docx::review::TextMark;
    let mut e = Editor::open(&doc_with(&[para("Heading1", "Alpha")].concat(), HEADINGS), fonts()).unwrap();
    e.execute(Command::SetTrackChanges(true), fonts());
    let first = texts_ids(&e)[0];
    e.select_between((first, Unit(0)), (first, Unit(0)), fonts());
    e.insert_table_of_contents(&TocOptions::word_default(), TocLook::default(), &TocFrame::default(), fonts());
    show_codes(&mut e, true);
    let mut d = e.document().clone();
    std::rc::Rc::make_mut(&mut d.review.markup).style.insertions.mark = TextMark::Bold;
    let snap = crate::layout::layout(&d, fonts());
    let items = crate::layout::traverse::paint_items(&snap.pages()[0]);
    let face_of = |ch: char| {
        items.iter().find_map(|it| match it {
            crate::layout::fragment::Item::Glyphs(g) if g.chars.iter().any(|c| c.0 == ch) => Some(g.face.clone()),
            _ => None,
        })
    };
    let (brace, code) = (face_of('{').expect("the opening brace"), face_of('T').expect("the code"));
    assert!(std::rc::Rc::ptr_eq(&brace, &code), "the brace in the code's bold face");
}
