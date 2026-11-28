//! Tables of contents and tables of figures

use super::super::Editor;
use super::super::commands::{Command, Dir, Outcome};
use super::super::toc::{TocFrame, TocLook, TocMark, TocOptions, TocUpdate};
use super::{fonts, texts, texts_ids};
use crate::docx::{Block, FieldEdge, FieldKind, Inline, Paragraph};
use crate::layout::{ParaId, Unit};

fn part(pkg: &[u8], name: &str) -> String {
    use std::io::Read;
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(pkg)).unwrap();
    let mut s = String::new();
    z.by_name(name).unwrap().read_to_string(&mut s).unwrap();
    s
}

/// The blank document with `body` and the built-in styles `styles` (by id) defined.
pub(super) fn doc_with(body: &str, styles: &[&str]) -> Vec<u8> {
    doc_with_numbering(body, styles, None)
}

/// [`doc_with`], with a numbering part holding `numbering` (its `w:abstractNum` and `w:num` elements).
pub(super) fn doc_with_numbering(body: &str, styles: &[&str], numbering: Option<&str>) -> Vec<u8> {
    use crate::services::styles::builtin::{built_in, definition};
    let cx = crate::docx::write::props::Cx::of_xml(&format!("<w:styles xmlns:w=\"{}\">", crate::docx::write::props::W_NS));
    let defs: String =
        styles.iter().map(|id| crate::docx::style_xml::serialize(&definition(built_in(id).unwrap()), &Default::default(), &cx)).collect();
    doc_from(body, &defs, numbering)
}

/// The blank document with `body`, the style definitions `defs` and a numbering part holding `numbering`.
fn doc_from(body: &str, defs: &str, numbering: Option<&str>) -> Vec<u8> {
    use std::io::{Read, Write};
    let src = crate::docx::blank::blank_docx();
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(src)).unwrap();
    let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for i in 0..z.len() {
        let mut f = z.by_index(i).unwrap();
        let name = f.name().unwrap().to_string();
        let mut data = Vec::new();
        f.read_to_end(&mut data).unwrap();
        if name == "word/document.xml" {
            data = String::from_utf8(data).unwrap().replacen("<w:p/>", body, 1).into_bytes();
        }
        if name == "word/styles.xml" {
            data = String::from_utf8(data).unwrap().replacen("</w:styles>", &format!("{defs}</w:styles>"), 1).into_bytes();
        }
        if numbering.is_some() && name == "word/_rels/document.xml.rels" {
            let rel = r#"<Relationship Id="rId9" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering" Target="numbering.xml"/>"#;
            data = String::from_utf8(data).unwrap().replacen("</Relationships>", &format!("{rel}</Relationships>"), 1).into_bytes();
        }
        if numbering.is_some() && name == "[Content_Types].xml" {
            let t = r#"<Override PartName="/word/numbering.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml"/>"#;
            data = String::from_utf8(data).unwrap().replacen("</Types>", &format!("{t}</Types>"), 1).into_bytes();
        }
        out.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
        out.write_all(&data).unwrap();
    }
    if let Some(n) = numbering {
        out.start_file("word/numbering.xml", zip::write::SimpleFileOptions::default()).unwrap();
        let x = format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">{n}</w:numbering>"#
        );
        out.write_all(x.as_bytes()).unwrap();
    }
    out.finish().unwrap().into_inner()
}

pub(super) fn para(style: &str, text: &str) -> String {
    format!(r#"<w:p><w:pPr><w:pStyle w:val="{style}"/></w:pPr><w:r><w:t xml:space="preserve">{text}</w:t></w:r></w:p>"#)
}

const PAGE: &str = r#"<w:p><w:r><w:br w:type="page"/></w:r></w:p>"#;

const HEADINGS: &[&str] = &["Heading1", "Heading2", "Heading3", "Heading4", "Caption"];

/// A read table of contents: its field over two entries, its end in a paragraph of its own.
const READ_TOC: &str = concat!(
    r#"<w:p><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> TOC \o "1-3" \h </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>One</w:t></w:r></w:p>"#,
    r#"<w:p><w:r><w:t>Two</w:t></w:r></w:p>"#,
    r#"<w:p><w:r><w:fldChar w:fldCharType="end"/></w:r></w:p>"#,
    r#"<w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>One</w:t></w:r></w:p>"#,
);

fn balance(xml: &str) -> (usize, usize, usize) {
    let n = |t: &str| xml.matches(&format!("fldCharType=\"{t}\"")).count();
    (n("begin"), n("separate"), n("end"))
}

fn saved(e: &Editor) -> String {
    part(&crate::docx::write::save(e.document(), e.docx()).unwrap(), "word/document.xml")
}

fn body_paras(e: &Editor) -> Vec<&Paragraph> {
    e.document().sections.iter().flat_map(|s| s.blocks.iter()).filter_map(|b| if let Block::Para(p) = b { Some(p) } else { None }).collect()
}

fn edges(e: &Editor) -> Vec<Vec<FieldEdge>> {
    body_paras(e).iter().map(|p| p.inlines.iter().filter_map(|i| super::super::toc::edge(i).cloned()).collect()).collect()
}

fn shown(p: &Paragraph) -> String {
    crate::docx::fields::visible(&p.inlines)
}

fn style_of(p: &Paragraph) -> String {
    p.style.style_id.clone().unwrap_or_default()
}

/// The PAGEREF results of the paragraph, in order.
fn page_results(p: &Paragraph) -> Vec<String> {
    crate::docx::fields::spans(&p.inlines)
        .into_iter()
        .filter(|s| matches!(&p.inlines[s.begin], Inline::FieldBegin { kind: Some(FieldKind::PageRef(_)), .. }))
        .map(|s| crate::docx::fields::result_text(&p.inlines, s))
        .collect()
}

fn codes(p: &Paragraph) -> Vec<String> {
    crate::docx::fields::spans(&p.inlines).into_iter().map(|s| crate::docx::fields::code_text(&p.inlines, s).trim().to_string()).collect()
}

fn insert(e: &mut Editor, o: &TocOptions) -> Outcome {
    e.insert_table_of_contents(o, TocLook::default(), &TocFrame::default(), fonts())
}

fn at_start(e: &mut Editor) {
    let p = texts_ids(e)[0];
    e.select_range(p, Unit(0), Unit(0), fonts());
}

#[test]
fn a_read_table_keeps_its_field_across_paragraphs() {
    let e = Editor::open(&doc_with(READ_TOC, HEADINGS), fonts()).unwrap();
    let ed = edges(&e);
    assert_eq!(ed[0], [FieldEdge::Start { code: r#" TOC \o "1-3" \h "#.into(), lock: false, dirty: false, view: crate::docx::FieldView::Result }]);
    assert_eq!(ed[2], [FieldEdge::End]);
    let t = e.tables_of_contents();
    assert_eq!(t.len(), 1);
    assert_eq!((t[0].first, t[0].last), (0, 2));
    assert_eq!(t[0].options(',').headings, Some((1, 3)));
    // Saved as read, and after edits in and around it.
    assert_eq!(balance(&saved(&e)), (1, 1, 1));
}

#[test]
fn editing_a_tables_entries_keeps_one_field() {
    for case in 0..3 {
        let mut e = Editor::open(&doc_with(READ_TOC, HEADINGS), fonts()).unwrap();
        let ps = texts_ids(&e);
        match case {
            0 => e.select_range(ps[0], Unit(1), Unit(1), fonts()),
            1 => e.select_range(ps[1], Unit(0), Unit(1), fonts()),
            _ => e.select_range(ps[0], Unit(0), Unit(0), fonts()),
        };
        e.execute(Command::InsertText("x".into()), fonts());
        let xml = saved(&e);
        assert_eq!(balance(&xml), (1, 1, 1), "case {case}: {xml}");
        let again = Editor::open(&crate::docx::write::save(e.document(), e.docx()).unwrap(), fonts()).unwrap();
        assert_eq!(again.tables_of_contents().len(), 1, "case {case}");
    }
}

#[test]
fn deleting_a_tables_first_entry_moves_its_start_to_the_next() {
    let mut e = Editor::open(&doc_with(READ_TOC, HEADINGS), fonts()).unwrap();
    let ps = texts_ids(&e);
    e.select_between((ps[0], Unit(0)), (ps[1], Unit(0)), fonts());
    e.execute(super::del(Dir::Back, false), fonts());
    assert_eq!(texts(&e)[0], "Two");
    assert!(matches!(edges(&e)[0].as_slice(), [FieldEdge::Start { .. }]), "{:?}", edges(&e));
    assert_eq!(balance(&saved(&e)), (1, 1, 1));
    assert_eq!(e.tables_of_contents().len(), 1);
}

#[test]
fn deleting_a_whole_table_takes_both_edges() {
    let mut e = Editor::open(&doc_with(READ_TOC, HEADINGS), fonts()).unwrap();
    let ps = texts_ids(&e);
    e.select_between((ps[0], Unit(0)), (ps[2], Unit(0)), fonts());
    e.execute(super::del(Dir::Back, false), fonts());
    assert!(edges(&e).iter().all(Vec::is_empty), "{:?}", edges(&e));
    assert_eq!(balance(&saved(&e)), (0, 0, 0));
    assert!(e.tables_of_contents().is_empty());
}

#[test]
fn a_copy_of_half_a_table_holds_no_edge() {
    let mut e = Editor::open(&doc_with(READ_TOC, HEADINGS), fonts()).unwrap();
    let ps = texts_ids(&e);
    e.select_between((ps[0], Unit(0)), (ps[1], Unit(2)), fonts());
    e.execute(Command::Copy, fonts());
    e.select_range(ps[3], Unit(3), Unit(3), fonts());
    e.execute(Command::Paste(Default::default()), fonts());
    let all: Vec<String> = body_paras(&e).iter().map(|p| shown(p)).collect();
    assert_eq!(all[3..], ["OneOne", "Tw"], "{all:?}");
    assert_eq!(balance(&saved(&e)), (1, 1, 1));
    assert_eq!(e.tables_of_contents().len(), 1);
}

fn sample() -> Vec<u8> {
    let body = [
        r#"<w:p/>"#.to_string(),
        para("Heading1", "Intro"),
        para("Normal", "Text"),
        PAGE.into(),
        para("Heading2", "Detail"),
        para("Heading3", "Deeper"),
        para("Heading4", "Too deep"),
        PAGE.into(),
        para("Heading1", "End"),
    ]
    .concat();
    doc_with(&body, HEADINGS)
}

#[test]
fn inserting_a_table_lists_headings_with_links_and_page_numbers() {
    let mut e = Editor::open(&sample(), fonts()).unwrap();
    at_start(&mut e);
    let out = insert(&mut e, &TocOptions::word_default());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    let ps = body_paras(&e);
    let entries: Vec<(String, String)> = ps[..4].iter().map(|p| (style_of(p), shown(p))).collect();
    assert_eq!(
        entries.iter().map(|(s, t)| (s.as_str(), t.split('\t').next().unwrap())).collect::<Vec<_>>(),
        [("TOC1", "Intro"), ("TOC2", "Detail"), ("TOC3", "Deeper"), ("TOC1", "End")]
    );
    // The heading 4 isn't in "1-3".
    assert!(!entries.iter().any(|(_, t)| t.contains("Too deep")));
    // Links to hidden bookmarks on the headings, and their pages.
    assert_eq!(codes(ps[0])[0].split_whitespace().take(2).collect::<Vec<_>>(), ["HYPERLINK", "\\l"]);
    assert!(codes(ps[0])[1].starts_with("PAGEREF _Toc"), "{:?}", codes(ps[0]));
    let pages: Vec<String> = ps[..4].iter().flat_map(|p| page_results(p)).collect();
    assert_eq!(pages, ["1", "2", "2", "3"]);
    let heading = ps.iter().find(|p| shown(p) == "Intro" && style_of(p) == "Heading1").unwrap();
    assert!(heading.inlines.first().and_then(Inline::bookmark).is_some_and(|(n, start)| start && n.starts_with("_Toc")));
    // A right tab with dots at the margin.
    let tab = ps[0].style.tabs.iter().find(|t| t.kind == crate::docx::props::TabKind::Right).expect("a right tab");
    assert_eq!(tab.leader, crate::docx::props::Leader::Dot);
    // One field, saved and read back.
    let saved = crate::docx::write::save(e.document(), e.docx()).unwrap();
    assert_eq!(balance(&part(&saved, "word/document.xml")).2, 1 + 4 * 2);
    let again = Editor::open(&saved, fonts()).unwrap();
    let t = again.tables_of_contents();
    assert_eq!(t.len(), 1);
    assert_eq!(t[0].options(','), TocOptions::word_default());
    // One undo step.
    e.execute(Command::Undo(Default::default()), fonts());
    assert!(e.tables_of_contents().is_empty());
    assert!(!body_paras(&e).iter().any(|p| p.inlines.iter().any(|i| i.bookmark().is_some())));
}

#[test]
fn page_numbers_show_their_sections_format_and_chapter() {
    let sect = |pg: &str| {
        format!(
            r#"<w:p><w:pPr><w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/>{pg}</w:sectPr></w:pPr></w:p>"#
        )
    };
    let body =
        ["<w:p/>".to_string(), para("Heading1", "Alpha"), sect(r#"<w:pgNumType w:fmt="lowerRoman" w:chapStyle="1"/>"#), para("Heading1", "Beta")]
            .concat();
    let heading = r#"<w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:pPr><w:outlineLvl w:val="0"/><w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr></w:pPr></w:style>"#;
    let numbering = r#"<w:abstractNum w:abstractNumId="0"><w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="decimal"/><w:lvlText w:val="%1."/></w:lvl></w:abstractNum><w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num>"#;
    let mut e = Editor::open(&doc_from(&body, heading, Some(numbering)), fonts()).unwrap();
    at_start(&mut e);
    insert(&mut e, &TocOptions::word_default());
    let t = e.tables_of_contents();
    let pages: Vec<String> = body_paras(&e)[t[0].first..=t[0].last].iter().flat_map(|p| page_results(p)).collect();
    assert_eq!(pages, ["1-i", "2"]);
}

#[test]
fn updating_page_numbers_keeps_edited_entries() {
    let mut e = Editor::open(&sample(), fonts()).unwrap();
    at_start(&mut e);
    insert(&mut e, &TocOptions::word_default());
    // An entry edited by hand, and a page break after the body text.
    let first = texts_ids(&e)[0];
    e.select_range(first, Unit(0), Unit(0), fonts());
    e.execute(Command::InsertText("My ".into()), fonts());
    let text = body_paras(&e).iter().find(|p| shown(p) == "Text").map(|p| crate::layout::ParaId(p.id)).unwrap();
    e.select_range(text, Unit(4), Unit(4), fonts());
    e.execute(Command::InsertPageBreak, fonts());
    let out = e.update_table_of_contents(TocUpdate::Pages, fonts());
    let ps = body_paras(&e);
    assert!(matches!(out, Outcome::Changed(_)), "{out:?} {:?} {:?}", ps.iter().map(|p| shown(p)).collect::<Vec<_>>(), e.tables_of_contents());
    assert!(shown(ps[0]).contains("My Intro"));
    assert_eq!(page_results(ps[1]), ["3"]);
    assert_eq!(page_results(ps[3]), ["4"]);
    // Entire: the edit goes.
    e.update_table_of_contents(TocUpdate::Entire, fonts());
    let ps = body_paras(&e);
    assert!(!shown(ps[0]).contains("My "));
    assert_eq!(page_results(ps[3]), ["4"]);
    assert_eq!(e.tables_of_contents().len(), 1);
    assert_eq!(balance(&saved(&e)).0, 1 + 4 * 2);
}

#[test]
fn an_entire_update_collects_new_headings_and_drops_unused_marks() {
    let mut e = Editor::open(&sample(), fonts()).unwrap();
    at_start(&mut e);
    insert(&mut e, &TocOptions::word_default());
    // The "Detail" heading becomes body text; a new heading comes.
    let detail = body_paras(&e).iter().filter(|p| style_of(p) == "Heading2").map(|p| crate::layout::ParaId(p.id)).next().unwrap();
    e.select_range(detail, Unit(0), Unit(0), fonts());
    e.execute(Command::SetParagraphStyle(crate::api::value::StyleId("Normal".into())), fonts());
    let n_before = e.bookmarks().len();
    let out = e.update_table_of_contents(TocUpdate::Entire, fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    let t = e.tables_of_contents();
    let ps = body_paras(&e);
    let names: Vec<String> = ps[t[0].first..=t[0].last].iter().map(|p| shown(p).split('\t').next().unwrap().to_string()).collect();
    assert_eq!(names, ["Intro", "Deeper", "End", ""]);
    assert_eq!(e.bookmarks().len(), n_before - 1, "the unused mark goes");
    // One undo step back.
    e.execute(Command::Undo(Default::default()), fonts());
    assert_eq!(e.bookmarks().len(), n_before);
}

#[test]
fn a_locked_table_refuses_updates() {
    let toc = READ_TOC.replace(r#"<w:fldChar w:fldCharType="begin"/>"#, r#"<w:fldChar w:fldCharType="begin" w:fldLock="1"/>"#);
    let mut e = Editor::open(&doc_with(&toc, HEADINGS), fonts()).unwrap();
    assert!(e.tables_of_contents()[0].lock);
    assert!(!matches!(e.update_table_of_contents(TocUpdate::Entire, fonts()), Outcome::Changed(_)));
    assert_eq!(texts(&e)[..2], ["One", "Two"]);
}

#[test]
fn an_update_clears_dirty_and_saves_one_start() {
    let toc = READ_TOC.replace(r#"<w:fldChar w:fldCharType="begin"/>"#, r#"<w:fldChar w:fldCharType="begin" w:dirty="1"/>"#);
    for mode in [TocUpdate::Pages, TocUpdate::Entire] {
        let mut e = Editor::open(&doc_with(&toc, HEADINGS), fonts()).unwrap();
        assert!(saved(&e).contains("w:dirty"));
        e.update_table_of_contents(mode, fonts());
        let xml = saved(&e);
        assert!(!xml.contains("w:dirty"), "{mode:?}: {xml}");
        assert_eq!(balance(&xml).0, balance(&xml).2, "{mode:?}: {xml}");
        assert_eq!(xml.matches("TOC \\o").count(), 1, "{mode:?}: {xml}");
        e.execute(Command::Undo(Default::default()), fonts());
        assert!(saved(&e).contains("w:dirty"), "{mode:?}");
    }
}

#[test]
fn hidden_in_web_view_marks_the_leader_and_page_number() {
    let mut e = Editor::open(&sample(), fonts()).unwrap();
    at_start(&mut e);
    insert(&mut e, &TocOptions::word_default());
    let xml = saved(&e);
    let first = &xml[..xml.find("</w:p>").unwrap()];
    // The entry's text isn't, its tab and page number field are.
    assert!(first.contains(r#"<w:r><w:t xml:space="preserve">Intro</w:t></w:r>"#), "{first}");
    let hidden = &first[first.find("<w:webHidden/>").expect("hidden runs")..];
    let tab = hidden.find("<w:tab/>").unwrap();
    assert!(tab < hidden.find("PAGEREF").unwrap() && hidden.find("PAGEREF").unwrap() < hidden.find(">1<").unwrap(), "{first}");
    assert!(!hidden[..tab].contains("</w:r>"), "{first}");
    let again = Editor::open(&crate::docx::write::save(e.document(), e.docx()).unwrap(), fonts()).unwrap();
    assert_eq!(again.tables_of_contents().len(), 1);
}

#[test]
fn an_empty_table_shows_words_notice() {
    let mut e = Editor::open(&doc_with("<w:p/>", HEADINGS), fonts()).unwrap();
    at_start(&mut e);
    insert(&mut e, &TocOptions::word_default());
    assert_eq!(texts(&e)[0], "No table of contents entries found.");
    insert(&mut e, &TocOptions::figures("Figure"));
    assert!(texts(&e).iter().any(|t| t == "No table of figures entries found."), "{:?}", texts(&e));
}

#[test]
fn switches_round_trip_through_codes() {
    for code in [
        r#" TOC \o "1-3" \h \z \u "#,
        r#" TOC \h \z \c "Figure" "#,
        r#" TOC \f \l "1-2" \n "2-3" \p "-" \s "Chapter" \d ":" \b "Part" \h \w \x \t "Title,1,Quote,2" "#,
        r#" TOC \f "A" \a "Table" "#,
    ] {
        let o = TocOptions::of_code(code, ',');
        assert_eq!(TocOptions::of_code(&o.code(','), ','), o, "{code}");
    }
    let o = TocOptions::of_code(r#" TOC \t "Title;1;Quote;2" "#, ';');
    assert_eq!(o.styles, [("Title".to_string(), 1), ("Quote".to_string(), 2)]);
    assert_eq!(TocOptions::of_code(" TOC \\o ", ',').headings, Some((1, 9)));
    assert_eq!(TocOptions::of_code(" TOC \\n ", ',').omit_pages, Some((1, 9)));
}

fn entries(body: &str, code: &str) -> Vec<(u8, String, bool)> {
    let e = Editor::open(&doc_with(body, &["Heading1", "Heading2", "Heading3", "Title", "Quote", "Caption"]), fonts()).unwrap();
    e.toc_entries(&TocOptions::of_code(code, ',')).into_iter().map(|x| (x.level, x.text, x.page)).collect()
}

#[test]
fn styles_outline_levels_and_page_number_switches() {
    let body = [
        para("Title", "Book"),
        para("Heading1", "Chapter"),
        para("Quote", "Saying"),
        r#"<w:p><w:pPr><w:outlineLvl w:val="1"/></w:pPr><w:r><w:t>Outlined</w:t></w:r></w:p>"#.to_string(),
        para("Heading3", "Small"),
    ]
    .concat();
    let t = |l: u8, s: &str, p: bool| (l, s.to_string(), p);
    assert_eq!(entries(&body, r#" TOC \t "Title,1,Quote,3" "#), [t(1, "Book", true), t(3, "Saying", true)]);
    assert_eq!(entries(&body, r#" TOC \o "1-2" "#), [t(1, "Chapter", true)]);
    assert_eq!(entries(&body, r#" TOC \o "1-2" \u "#), [t(1, "Chapter", true), t(2, "Outlined", true)]);
    assert_eq!(entries(&body, r#" TOC \u "#), [t(1, "Chapter", true), t(2, "Outlined", true), t(3, "Small", true)]);
    assert_eq!(entries(&body, r#" TOC \u \n "2-3" "#), [t(1, "Chapter", true), t(2, "Outlined", false), t(3, "Small", false)]);
    assert_eq!(entries(&body, r#" TOC \u \n "#).iter().filter(|x| x.2).count(), 0);
}

#[test]
fn a_bookmark_limits_what_a_table_collects() {
    let body = [
        para("Heading1", "Before"),
        r#"<w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:bookmarkStart w:id="1" w:name="Part"/><w:r><w:t>Inside</w:t></w:r></w:p>"#.into(),
        para("Heading2", "Also"),
        r#"<w:p><w:r><w:t>end</w:t></w:r><w:bookmarkEnd w:id="1"/></w:p>"#.into(),
        para("Heading1", "After"),
    ]
    .concat();
    let names: Vec<String> = entries(&body, r#" TOC \o "1-3" \b Part "#).into_iter().map(|x| x.1).collect();
    assert_eq!(names, ["Inside", "Also"]);
}

#[test]
fn entry_fields_by_identifier_and_level() {
    let tc = |code: &str| {
        format!(
            r#"<w:p><w:r><w:t>x</w:t></w:r><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve">{code}</w:instrText></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r></w:p>"#
        )
    };
    let body = [tc(r#" TC "Plain" \l 1 "#), tc(r#" TC "Second" \l 2 \n "#), tc(r#" TC "Other" \f B \l 1 "#), tc(r#" TC "Deep" \l 4 "#)].concat();
    let t = |l: u8, s: &str, p: bool| (l, s.to_string(), p);
    assert_eq!(entries(&body, r#" TOC \f "#), [t(1, "Plain", true), t(2, "Second", false), t(4, "Deep", true)]);
    assert_eq!(entries(&body, r#" TOC \f B "#), [t(1, "Other", true)]);
    assert_eq!(entries(&body, r#" TOC \f \l "1-2" "#), [t(1, "Plain", true), t(2, "Second", false)]);
}

fn caption(n: &str, text: &str) -> String {
    format!(
        r#"<w:p><w:pPr><w:pStyle w:val="Caption"/></w:pPr><w:r><w:t xml:space="preserve">Figure </w:t></w:r><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> SEQ Figure \* ARABIC </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>{n}</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r><w:r><w:t xml:space="preserve">: {text}</w:t></w:r></w:p>"#
    )
}

#[test]
fn captions_with_and_without_their_labels() {
    let body = [caption("1", "A cat"), para("Normal", "Text"), caption("2", "A dog")].concat();
    let names = |code: &str| entries(&body, code).into_iter().map(|x| x.1).collect::<Vec<_>>();
    assert_eq!(names(r#" TOC \h \z \c "Figure" "#), ["Figure 1: A cat", "Figure 2: A dog"]);
    assert_eq!(names(r#" TOC \h \z \a "Figure" "#), ["A cat", "A dog"]);
    assert!(names(r#" TOC \c "Table" "#).is_empty());
}

#[test]
fn a_table_of_figures_uses_its_style() {
    let body = ["<w:p/>".to_string(), caption("1", "A cat"), PAGE.into(), caption("2", "A dog")].concat();
    let mut e = Editor::open(&doc_with(&body, HEADINGS), fonts()).unwrap();
    at_start(&mut e);
    insert(&mut e, &TocOptions::figures("Figure"));
    let ps = body_paras(&e);
    assert_eq!(style_of(ps[0]), "TableofFigures");
    assert!(shown(ps[0]).starts_with("Figure 1: A cat\t"));
    assert_eq!(page_results(ps[1]), ["2"]);
}

#[test]
fn sequence_prefixes_and_kept_tabs_and_breaks() {
    let chapter = |n: &str| {
        format!(
            r#"<w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> SEQ Chapter </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>{n}</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r><w:r><w:tab/><w:t>Part</w:t><w:br/><w:t>two</w:t></w:r></w:p>"#
        )
    };
    let body = [chapter("1"), chapter("2")].concat();
    let e = Editor::open(&doc_with(&body, HEADINGS), fonts()).unwrap();
    let got = e.toc_entries(&TocOptions::of_code(r#" TOC \o "1-1" \s Chapter "#, ','));
    assert_eq!(
        got.iter().map(|x| (x.text.as_str(), x.prefix.as_deref())).collect::<Vec<_>>(),
        [("1 Part two", Some("1")), ("2 Part two", Some("2"))]
    );
    let kept = e.toc_entries(&TocOptions::of_code(r#" TOC \o "1-1" \w \x "#, ','));
    assert_eq!(kept[0].text, "1\tPart\ntwo");
}

#[test]
fn text_wrapping_breaks_count_as_line_breaks() {
    let body = r#"<w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>Alpha</w:t><w:br w:clear="left"/><w:t>Beta</w:t></w:r></w:p>"#;
    let names = |code: &str| entries(body, code).into_iter().map(|x| x.1).collect::<Vec<_>>();
    assert_eq!(names(r#" TOC \o "1-1" "#), ["Alpha Beta"]);
    assert_eq!(names(r#" TOC \o "1-1" \x "#), ["Alpha\nBeta"]);
}

#[test]
fn marking_an_entry_puts_a_hidden_entry_field() {
    let mut e = Editor::open(&doc_with(&para("Normal", "Here"), HEADINGS), fonts()).unwrap();
    let p = texts_ids(&e)[0];
    e.select_range(p, Unit(4), Unit(4), fonts());
    let m = TocMark { text: "Marked \"one\"".into(), level: 2, identifier: Some("B".into()), page_number: false };
    assert!(matches!(e.mark_toc_entry(&m, fonts()), Outcome::Changed(_)));
    let ps = body_paras(&e);
    let code = codes(ps[0]).remove(0);
    assert_eq!(TocMark::of_code(&code).unwrap(), m);
    assert_eq!(shown(ps[0]), "Here", "the field shows nothing");
    let begin = ps[0].inlines.iter().find_map(|i| if let Inline::FieldBegin { run, .. } = i { Some(run.clone()) } else { None }).unwrap();
    assert!(begin.hidden);
    let got = e.toc_entries(&TocOptions::of_code(r#" TOC \f B "#, ','));
    assert_eq!(got.iter().map(|x| (x.level, x.text.as_str(), x.page)).collect::<Vec<_>>(), [(2, "Marked \"one\"", false)]);
}

#[test]
fn a_frame_adds_a_title_and_a_control() {
    let mut e = Editor::open(&sample(), fonts()).unwrap();
    at_start(&mut e);
    let frame = TocFrame { title: Some("Contents".into()), control: true };
    e.insert_table_of_contents(&TocOptions::word_default(), TocLook::default(), &frame, fonts());
    let ps = body_paras(&e);
    assert_eq!((style_of(ps[0]).as_str(), texts(&e)[0].as_str()), ("TOCHeading", "Contents"));
    let c = ps[0].controls.first().copied().expect("a control");
    // The end in a paragraph of its own inside the control, the caret's paragraph after it.
    assert!(ps[1].controls.contains(&c) && ps[5].controls.contains(&c) && !ps[6].controls.contains(&c));
    assert_eq!(edges(&e)[5], [FieldEdge::End]);
    assert_eq!(ps[5].inlines.len(), 1);
    let xml = saved(&e);
    assert!(xml.contains("docPartGallery w:val=\"Table of Contents\""), "{xml}");
    let end = xml.rfind("fldCharType=\"end\"").unwrap();
    assert!(end < xml.find("</w:sdtContent>").unwrap(), "{xml}");
    assert_eq!(e.tables_of_contents().len(), 1);
    // Read back: the same table, the caret in its end paragraph still in it.
    let mut again = Editor::open(&crate::docx::write::save(e.document(), e.docx()).unwrap(), fonts()).unwrap();
    let t = again.tables_of_contents().remove(0);
    assert_eq!((t.first, t.last), (1, 5));
    let p = crate::layout::ParaId(body_paras(&again)[5].id);
    again.select_range(p, Unit(0), Unit(0), fonts());
    assert!(again.toc_at_selection().is_some());
    // Updated and replaced, the end stays inside.
    again.update_table_of_contents(TocUpdate::Entire, fonts());
    let x = saved(&again);
    assert!(x.rfind("fldCharType=\"end\"").unwrap() < x.find("</w:sdtContent>").unwrap(), "{x}");
    again.insert_table_of_contents(&TocOptions::word_default(), TocLook::default(), &frame, fonts());
    let x = saved(&again);
    assert_eq!(again.tables_of_contents().len(), 1);
    assert!(x.rfind("fldCharType=\"end\"").unwrap() < x.find("</w:sdtContent>").unwrap(), "{x}");
    assert_eq!(x.matches("<w:sdt>").count(), 1, "{x}");
}

#[test]
fn a_caret_in_the_paragraph_after_a_table_is_outside_it() {
    let body = [para("Heading1", "Intro"), para("Heading1", "End")].concat();
    let mut e = Editor::open(&doc_with(&body, HEADINGS), fonts()).unwrap();
    at_start(&mut e);
    insert(&mut e, &TocOptions::word_default());
    // The heading after the table holds its end.
    let t = e.tables_of_contents().remove(0);
    let heading = crate::layout::ParaId(body_paras(&e)[t.last].id);
    e.select_range(heading, Unit(2), Unit(2), fonts());
    assert!(e.toc_at_selection().is_none());
    insert(&mut e, &TocOptions::word_default());
    assert_eq!(e.tables_of_contents().len(), 2);
    let (b, s, n) = balance(&saved(&e));
    assert!(b == s && s == n, "{b} {s} {n}");
}

#[test]
fn inserting_in_a_table_replaces_it() {
    let mut e = Editor::open(&sample(), fonts()).unwrap();
    at_start(&mut e);
    let frame = TocFrame { title: Some("Contents".into()), control: true };
    e.insert_table_of_contents(&TocOptions::word_default(), TocLook::default(), &frame, fonts());
    let n = body_paras(&e).len();
    // The caret in the title, then in an entry.
    for k in [0, 2] {
        let p = texts_ids(&e)[k];
        e.select_range(p, Unit(1), Unit(1), fonts());
        let out = e.insert_table_of_contents(&TocOptions::word_default(), TocLook::default(), &frame, fonts());
        assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
        assert_eq!(e.tables_of_contents().len(), 1);
        assert_eq!(body_paras(&e).len(), n, "{:?}", texts(&e));
        assert_eq!(body_paras(&e).iter().filter(|p| style_of(p) == "TOCHeading").count(), 1);
        assert_eq!(balance(&saved(&e)), (9, 9, 9));
    }
    // One undo step back to the first table.
    e.execute(Command::Undo(Default::default()), fonts());
    assert_eq!(e.tables_of_contents().len(), 1);
    assert_eq!(body_paras(&e).len(), n);
}

#[test]
fn removing_a_table_takes_its_title_and_control() {
    let mut e = Editor::open(&sample(), fonts()).unwrap();
    let before = texts(&e);
    at_start(&mut e);
    let frame = TocFrame { title: Some("Contents".into()), control: true };
    e.insert_table_of_contents(&TocOptions::word_default(), TocLook::default(), &frame, fonts());
    let t = e.tables_of_contents().remove(0);
    assert!(matches!(e.remove_table_of_contents(&t, fonts()), Outcome::Changed(_)));
    assert!(e.tables_of_contents().is_empty());
    assert_eq!(texts(&e), before);
    assert!(body_paras(&e).iter().all(|p| p.controls.is_empty()));
    assert_eq!(balance(&saved(&e)), (0, 0, 0));
    e.execute(Command::Undo(Default::default()), fonts());
    assert_eq!(e.tables_of_contents().len(), 1);
    // A read one, its end in a paragraph of its own: that paragraph's mark stays.
    let mut e = Editor::open(&doc_with(READ_TOC, HEADINGS), fonts()).unwrap();
    let t = e.tables_of_contents().remove(0);
    e.remove_table_of_contents(&t, fonts());
    assert_eq!(texts(&e), ["", "One"]);
    // In Word's building block control, with it.
    let sdt = |inner: &str| {
        format!(
            r#"<w:sdt><w:sdtPr><w:docPartObj><w:docPartGallery w:val="Table of Contents"/><w:docPartUnique/></w:docPartObj></w:sdtPr><w:sdtContent>{inner}</w:sdtContent></w:sdt>"#
        )
    };
    let body = sdt(&(para("TOCHeading", "Contents") + &READ_TOC[..READ_TOC.rfind("<w:p><w:pPr>").unwrap()])) + &para("Heading1", "One");
    let mut e = Editor::open(&doc_with(&body, &["Heading1", "TOCHeading"]), fonts()).unwrap();
    let t = e.tables_of_contents().remove(0);
    e.remove_table_of_contents(&t, fonts());
    assert_eq!(texts(&e), ["One"]);
    assert!(!saved(&e).contains("w:sdt"));
    assert_eq!(balance(&saved(&e)), (0, 0, 0));
}

#[test]
fn removing_a_table_keeps_what_shares_its_paragraphs() {
    let toc = READ_TOC
        .replacen(r#"<w:r><w:fldChar w:fldCharType="begin"/>"#, r#"<w:r><w:t>Lead</w:t></w:r><w:r><w:fldChar w:fldCharType="begin"/>"#, 1)
        .replacen(
            r#"<w:r><w:fldChar w:fldCharType="end"/></w:r></w:p>"#,
            r#"<w:r><w:fldChar w:fldCharType="end"/></w:r><w:r><w:t>Tail</w:t></w:r></w:p>"#,
            1,
        );
    let mut e = Editor::open(&doc_with(&toc, HEADINGS), fonts()).unwrap();
    let t = e.tables_of_contents().remove(0);
    e.remove_table_of_contents(&t, fonts());
    assert_eq!(texts(&e), ["LeadTail", "One"]);
    assert_eq!(balance(&saved(&e)), (0, 0, 0), "{}", saved(&e));
}

#[test]
fn no_right_tab_without_right_alignment() {
    let mut e = Editor::open(&sample(), fonts()).unwrap();
    at_start(&mut e);
    let look = TocLook { right_align: false, leader: crate::docx::props::Leader::None };
    e.insert_table_of_contents(&TocOptions::word_default(), look, &TocFrame::default(), fonts());
    assert!(body_paras(&e)[0].style.tabs.iter().all(|t| t.kind != crate::docx::props::TabKind::Right));
    // An entire update keeps the look.
    e.update_table_of_contents(TocUpdate::Entire, fonts());
    assert!(body_paras(&e)[0].style.tabs.iter().all(|t| t.kind != crate::docx::props::TabKind::Right));
}

// Sequences and references

fn field(code: &str, shown: &str) -> String {
    format!(
        r#"<w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> {code} </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>{shown}</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r>"#
    )
}

fn values(e: &Editor) -> Vec<String> {
    body_paras(e)
        .iter()
        .flat_map(|p| p.inlines.iter())
        .filter_map(|i| if let Inline::FieldBegin { value: Some(v), .. } = i { Some(v.clone()) } else { None })
        .collect()
}

/// Outline numbering: "Chapter %1" at level 1, "%1.%2" at 2, "%3)" (letters) at 3, on Heading 1 to 3.
const OUTLINE: &str = concat!(
    r#"<w:abstractNum w:abstractNumId="1"><w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="decimal"/><w:lvlText w:val="Chapter %1"/><w:pStyle w:val="Heading1"/></w:lvl>"#,
    r#"<w:lvl w:ilvl="1"><w:start w:val="1"/><w:numFmt w:val="decimal"/><w:lvlText w:val="%1.%2."/><w:pStyle w:val="Heading2"/></w:lvl>"#,
    r#"<w:lvl w:ilvl="2"><w:start w:val="1"/><w:numFmt w:val="lowerLetter"/><w:lvlText w:val="%3)"/><w:pStyle w:val="Heading3"/></w:lvl></w:abstractNum>"#,
    r#"<w:num w:numId="1"><w:abstractNumId w:val="1"/></w:num>"#,
);

fn numbered(level: u8, text: &str, mark: Option<&str>) -> String {
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(100);
    let (a, b) = match mark {
        Some(m) => {
            let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            (format!(r#"<w:bookmarkStart w:id="{id}" w:name="{m}"/>"#), format!(r#"<w:bookmarkEnd w:id="{id}"/>"#))
        }
        None => (String::new(), String::new()),
    };
    format!(
        r#"<w:p><w:pPr><w:pStyle w:val="Heading{level}"/><w:numPr><w:ilvl w:val="{}"/><w:numId w:val="1"/></w:numPr></w:pPr>{a}<w:r><w:t>{text}</w:t></w:r>{b}</w:p>"#,
        level - 1
    )
}

#[test]
fn sequences_start_again_after_headings_and_skip_deleted_ones() {
    let seq = |code: &str| format!("<w:p>{}</w:p>", field(code, "?"));
    let deleted = r#"<w:p><w:del w:id="9" w:author="A"><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:delInstrText> SEQ Figure \s 1 </w:delInstrText></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r></w:del></w:p>"#.to_string();
    let body = [
        para("Heading1", "One"),
        seq(r#"SEQ Figure \s 1"#),
        seq(r#"SEQ Figure \s 1"#),
        deleted,
        para("Heading2", "Sub"),
        seq(r#"SEQ Figure \s 1"#),
        para("Heading1", "Two"),
        seq(r#"SEQ Figure \s 1 \* ROMAN"#),
        seq(r#"SEQ Figure \c"#),
        seq(r#"SEQ Table \* alphabetic"#),
        seq(r#"SEQ Table \* CardText \* Caps"#),
        seq(r#"SEQ Table \* OrdText"#),
        seq(r#"SEQ Table \* DollarText"#),
        seq(r#"SEQ Table \* ordinal"#),
    ]
    .concat();
    let e = Editor::open(&doc_with(&body, HEADINGS), fonts()).unwrap();
    assert_eq!(values(&e), ["1", "2", "2", "3", "I", "1", "a", "Two", "third", "four and 00/100", "5th"]);
}

#[test]
fn references_show_paragraph_numbers_in_their_contexts() {
    let refs = |code: &str| format!("<w:p>{}</w:p>", field(code, "?"));
    let body = [
        numbered(1, "Intro", None),
        numbered(2, "Scope", None),
        numbered(3, "Terms", Some("Terms")),
        numbered(1, "Body", Some("Body")),
        refs("REF Terms \\n"),
        refs("REF Terms \\w"),
        refs("REF Terms \\r"),
        refs("REF Terms \\p"),
        refs("REF Terms \\w \\p"),
        refs("REF Body \\n \\t"),
        refs("REF Terms"),
        refs("STYLEREF 1 \\s"),
        refs("STYLEREF 1 \\n"),
        refs("STYLEREF 1"),
        refs("REF Terms \\w \\d \"-\""),
    ]
    .concat();
    let e = Editor::open(&doc_with_numbering(&body, HEADINGS, Some(OUTLINE)), fonts()).unwrap();
    assert_eq!(
        values(&e),
        ["a)", "Chapter 1.1.a)", "Chapter 1.1.a)", "above", "Chapter 1.1.a) above", "2", "Terms", "2", "Chapter 2", "Body", "Chapter 1-1-a)"]
    );
}

// Captions

use super::super::captions::{CaptionFormat, CaptionOptions, CaptionPosition, ChapterNumber, ChapterSeparator};

fn caption_opts(label: &str, text: &str) -> CaptionOptions {
    CaptionOptions { text: text.into(), ..CaptionOptions::new(label) }
}

#[test]
fn a_caption_goes_below_its_paragraph_and_numbers_its_label() {
    let body = [para("Normal", "One"), caption("1", "A cat"), para("Normal", "Two")].concat();
    let mut e = Editor::open(&doc_with(&body, HEADINGS), fonts()).unwrap();
    let ps = texts_ids(&e);
    e.select_range(ps[0], Unit(1), Unit(1), fonts());
    let out = e.insert_caption(&caption_opts("Figure", ": First"), fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    let shown_all: Vec<String> = body_paras(&e).iter().map(|p| shown(p)).collect();
    // The later caption renumbers.
    assert_eq!(shown_all, ["One", "Figure 1: First", "Figure 2: A cat", "Two"]);
    assert_eq!(style_of(body_paras(&e)[1]), "Caption");
    // One undo step.
    e.execute(Command::Undo(Default::default()), fonts());
    assert_eq!(body_paras(&e).iter().map(|p| shown(p)).collect::<Vec<_>>(), ["One", "Figure 1: A cat", "Two"]);
}

#[test]
fn a_table_caption_goes_above_with_its_format_and_chapter() {
    let table = r#"<w:tbl><w:tblGrid><w:gridCol w:w="2000"/></w:tblGrid><w:tr><w:tc><w:p><w:r><w:t>cell</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#;
    let body = [numbered(1, "Intro", None), numbered(1, "Body", None), table.to_string(), para("Normal", "After")].concat();
    let mut e = Editor::open(&doc_with_numbering(&body, HEADINGS, Some(OUTLINE)), fonts()).unwrap();
    let cell = texts_ids(&e)[2];
    e.select_range(cell, Unit(1), Unit(1), fonts());
    let c = CaptionOptions {
        format: CaptionFormat::UpperRoman,
        chapter: Some(ChapterNumber { level: 1, separator: ChapterSeparator::EnDash }),
        ..caption_opts("Table", " Sizes")
    };
    assert!(matches!(e.insert_caption(&c, fonts()), Outcome::Changed(_)));
    let blocks = &e.document().sections[0].blocks;
    let Block::Para(cap) = &blocks[2] else { panic!("the caption goes above the table") };
    assert_eq!(shown(cap), "Table 2\u{2013}I Sizes");
    assert!(cap.style.keep_next, "above, it keeps with the table");
    assert!(codes(cap).iter().any(|c| c == r#"SEQ Table \* ROMAN \s 1"#), "{:?}", codes(cap));
}

#[test]
fn excluding_the_label_and_placing_above_a_paragraph() {
    let mut e = Editor::open(&doc_with(&para("Normal", "One"), HEADINGS), fonts()).unwrap();
    at_start(&mut e);
    let c = CaptionOptions { exclude_label: true, position: Some(CaptionPosition::Above), ..caption_opts("Equation", "") };
    e.insert_caption(&c, fonts());
    assert_eq!(body_paras(&e).iter().map(|p| shown(p)).collect::<Vec<_>>(), ["1", "One"]);
    assert!(!matches!(e.insert_caption(&caption_opts("Bad-label", ""), fonts()), Outcome::Changed(_)));
}

#[test]
fn caption_labels_are_built_in_custom_and_used() {
    let body = [caption("1", "x"), "<w:p>".to_string() + &field("SEQ Chart", "1") + "</w:p>"].concat();
    let e = Editor::open(&doc_with(&body, HEADINGS), fonts()).unwrap();
    assert_eq!(e.caption_labels(&["Plate".into(), "figure".into()]), ["Equation", "Figure", "Table", "Plate", "Chart"]);
}

#[test]
fn a_label_with_spaces_numbers_in_its_underscored_sequence() {
    let mut e = Editor::open(&doc_with(&[para("Normal", "One"), para("Normal", "Two")].concat(), HEADINGS), fonts()).unwrap();
    for (k, text) in [(0, ": a"), (2, ": b")] {
        let p = texts_ids(&e)[k];
        e.select_range(p, Unit(1), Unit(1), fonts());
        assert!(matches!(e.insert_caption(&caption_opts("My Plate", text), fonts()), Outcome::Changed(_)));
    }
    let ps = body_paras(&e);
    assert_eq!(shown(ps[1]), "My Plate 1: a");
    assert_eq!(shown(ps[3]), "My Plate 2: b");
    assert!(codes(ps[1]).iter().any(|c| c.starts_with("SEQ My_Plate ")), "{:?}", codes(ps[1]));
    assert_eq!(e.caption_labels(&[]).last().map(String::as_str), Some("My Plate"));
    assert_eq!(e.toc_entries(&TocOptions::figures("My Plate")).len(), 2);
    assert_eq!(e.cross_reference_targets(&RefKind::Caption("My Plate".into())).len(), 2);
    for bad in [" Lead", "Trail ", "1st", "a-b"] {
        assert!(matches!(e.insert_caption(&caption_opts(bad, ""), fonts()), Outcome::Refused(_)), "{bad}");
    }
}

#[test]
fn auto_caption_captions_inserted_tables() {
    let mut e = Editor::open(&doc_with(&para("Normal", "One"), HEADINGS), fonts()).unwrap();
    e.set_auto_caption(Some(caption_opts("Table", "")));
    let p = texts_ids(&e)[0];
    e.select_range(p, Unit(3), Unit(3), fonts());
    let out = e.execute(Command::InsertTable(serde_json::from_value(serde_json::json!({"rows": 2, "columns": 2})).unwrap()), fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    let caps: Vec<String> = body_paras(&e).iter().filter(|p| style_of(p) == "Caption").map(|p| shown(p)).collect();
    assert_eq!(caps, ["Table 1"]);
    e.execute(Command::Undo(Default::default()), fonts());
    assert_eq!(body_paras(&e).len(), 1, "one undo step");
}

// Cross-references

use super::super::xref::{CrossReference, RefContent, RefKind};

fn xref(content: RefContent) -> CrossReference {
    CrossReference { content, hyperlink: true, above_below: false }
}

#[test]
fn cross_reference_targets_by_kind() {
    let note = r#"<w:p><w:r><w:t>Noted</w:t></w:r><w:r><w:rPr><w:vertAlign w:val="superscript"/></w:rPr><w:footnoteReference w:id="1"/></w:r></w:p>"#;
    let body = [
        numbered(1, "Intro", None),
        numbered(2, "Scope", None),
        r#"<w:p><w:bookmarkStart w:id="5" w:name="Zed"/><w:bookmarkStart w:id="6" w:name="_Hidden"/><w:r><w:t>marked</w:t></w:r><w:bookmarkEnd w:id="5"/><w:bookmarkEnd w:id="6"/><w:bookmarkStart w:id="7" w:name="alpha"/><w:bookmarkEnd w:id="7"/></w:p>"#.into(),
        caption("1", "A cat"),
        note.into(),
    ]
    .concat();
    let e = Editor::open(&doc_with_numbering(&body, HEADINGS, Some(OUTLINE)), fonts()).unwrap();
    let texts = |k: RefKind| e.cross_reference_targets(&k).into_iter().map(|t| t.text).collect::<Vec<_>>();
    assert_eq!(texts(RefKind::NumberedItem), ["Chapter 1 Intro", "1.1. Scope"]);
    assert_eq!(texts(RefKind::Heading), ["Intro", "Scope"]);
    assert_eq!(texts(RefKind::Bookmark), ["alpha", "Zed"]);
    assert_eq!(texts(RefKind::Caption("figure".into())), ["Figure 1: A cat"]);
    let notes = e.cross_reference_targets(&RefKind::Footnote);
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].note, Some(1));
}

#[test]
fn inserting_cross_references_marks_their_targets() {
    let body = [numbered(1, "Intro", None), numbered(2, "Scope", None), caption("1", "A cat"), PAGE.into(), para("Normal", "See ")].concat();
    let mut e = Editor::open(&doc_with_numbering(&body, HEADINGS, Some(OUTLINE)), fonts()).unwrap();
    let last = *texts_ids(&e).last().unwrap();
    let put = |e: &mut Editor, kind: RefKind, k: usize, r: CrossReference| {
        let t = e.cross_reference_targets(&kind).remove(k);
        let end = crate::editing::step::para_len(e.index().para(e.document(), last).unwrap());
        e.select_range(last, end, end, fonts());
        let out = e.insert_cross_reference(&t, &r, fonts());
        assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    };
    put(&mut e, RefKind::Heading, 1, xref(RefContent::Text));
    put(&mut e, RefKind::Heading, 1, xref(RefContent::NumberFullContext));
    put(&mut e, RefKind::Heading, 1, xref(RefContent::PageNumber));
    put(&mut e, RefKind::Caption("Figure".into()), 0, xref(RefContent::LabelAndNumber));
    put(&mut e, RefKind::Caption("Figure".into()), 0, xref(RefContent::CaptionText));
    put(&mut e, RefKind::Caption("Figure".into()), 0, CrossReference { content: RefContent::Text, hyperlink: false, above_below: true });
    let p = e.index().para(e.document(), last).unwrap();
    assert_eq!(shown(p), "See ScopeChapter 1.11Figure 1A catFigure 1: A cat above");
    let cs = codes(p);
    assert!(cs[0].starts_with("REF _Ref") && cs[0].ends_with("\\h"), "{cs:?}");
    assert_eq!(cs[0].split_whitespace().nth(1), cs[1].split_whitespace().nth(1), "one bookmark for the heading");
    assert!(cs[2].starts_with("PAGEREF _Ref"));
    assert!(cs[6].ends_with("\\p"), "{cs:?}");
    let hidden = e.bookmarks().into_iter().filter(|b| b.name.starts_with("_Ref")).count();
    assert_eq!(hidden, 4, "the heading's, and the caption's three ranges");
    // One undo step each.
    e.execute(Command::Undo(Default::default()), fonts());
    assert_eq!(codes(e.index().para(e.document(), last).unwrap()).len(), 5);
}

#[test]
fn updating_fields_computes_references_and_sequences_in_the_engine() {
    let body = [
        r#"<w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:bookmarkStart w:id="1" w:name="Target"/><w:r><w:t>Old</w:t></w:r><w:bookmarkEnd w:id="1"/></w:p>"#.to_string(),
        format!("<w:p>{}{}</w:p>", field("REF Target", "Old"), field("SEQ Figure", "1")),
    ]
    .concat();
    let mut e = Editor::open(&doc_with(&body, HEADINGS), fonts()).unwrap();
    assert_eq!(e.bookmarks().len(), 1, "read");
    let ps = texts_ids(&e);
    e.select_range(ps[0], Unit(1), Unit(1), fonts());
    e.execute(Command::InsertText("New ".into()), fonts());
    assert_eq!(e.bookmarks().len(), 1, "{:?}", e.index().para(e.document(), ps[0]).unwrap().inlines.len());
    e.select_range(ps[1], Unit(0), Unit(0), fonts());
    e.execute(Command::InsertField(crate::api::command::InsertField { code: " SEQ Figure ".into(), display: None, show_code: None }), fonts());
    // Select the paragraph's fields and update them.
    let len = crate::editing::step::para_len(e.index().para(e.document(), ps[1]).unwrap());
    e.select_range(ps[1], Unit(0), len, fonts());
    e.execute(Command::UpdateFields(Default::default()), fonts());
    let p = e.index().para(e.document(), ps[1]).unwrap();
    assert_eq!(shown(p), "1ONew ld2", "{:?} {:?}", e.bookmarks(), e.index().para(e.document(), ps[0]).unwrap().inlines);
    assert!(e.take_notices().iter().all(|n| !matches!(n, super::super::input::Notice::FieldUpdate { .. })), "nothing left for the host");
}

#[test]
fn the_manual_table_is_typed_entries_in_a_control() {
    let mut e = Editor::open(&doc_with(&para("Normal", "Body"), HEADINGS), fonts()).unwrap();
    at_start(&mut e);
    assert!(matches!(e.insert_manual_table_of_contents(fonts()), Outcome::Changed(_)));
    let t = texts(&e);
    assert_eq!(t[0], "Table of Contents");
    assert_eq!(t[1], "Type chapter title (level 1)\t1");
    assert_eq!(t[6], "Type chapter title (level 3)\t6");
    assert_eq!(t[7], "Body");
    assert!(e.tables_of_contents().is_empty(), "no field");
    assert_eq!(body_paras(&e)[3].style.style_id.as_deref(), Some("TOC3"));
    e.execute(Command::Undo(Default::default()), fonts());
    assert_eq!(texts(&e), ["Body"]);
}

#[test]
fn levels_alone_collect_entry_fields() {
    let tc = |code: &str| {
        format!(
            r#"<w:p><w:r><w:t>x</w:t></w:r><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve">{code}</w:instrText></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r></w:p>"#
        )
    };
    let body = [tc(r#" TC "Plain" \l 1 "#), tc(r#" TC "Deep" \l 4 "#), tc(r#" TC "Other" \f B \l 1 "#)].concat();
    assert_eq!(entries(&body, r#" TOC \l "1-2" "#), [(1, "Plain".to_string(), true)]);
}

#[test]
fn hidden_names_past_the_largest_number_dont_overflow() {
    let body = [
        r#"<w:p/>"#.to_string(),
        r#"<w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:bookmarkStart w:id="1" w:name="_Toc18446744073709551615"/><w:r><w:t>Intro</w:t></w:r><w:bookmarkEnd w:id="1"/></w:p>"#.into(),
        r#"<w:p><w:bookmarkStart w:id="2" w:name="_Ref18446744073709551615"/><w:r><w:t>Text</w:t></w:r><w:bookmarkEnd w:id="2"/></w:p>"#.into(),
        para("Heading1", "End"),
    ]
    .concat();
    let mut e = Editor::open(&doc_with(&body, HEADINGS), fonts()).unwrap();
    at_start(&mut e);
    assert!(matches!(insert(&mut e, &TocOptions::word_default()), Outcome::Changed(_)));
    let t = e.cross_reference_targets(&RefKind::Heading);
    let end = t.iter().find(|t| t.text == "End").unwrap().clone();
    at_start(&mut e);
    assert!(matches!(e.insert_cross_reference(&end, &xref(RefContent::Text), fonts()), Outcome::Changed(_)));
    let names: Vec<String> = e.bookmarks().into_iter().map(|b| b.name).collect();
    assert!(names.iter().any(|n| n == "_Toc000000000"), "{names:?}");
    assert!(names.iter().any(|n| n == "_Ref000000000"), "{names:?}");
}

#[test]
fn a_notes_text_isnt_a_cross_reference() {
    let note = r#"<w:p><w:r><w:t>Noted</w:t></w:r><w:r><w:footnoteReference w:id="1"/></w:r></w:p>"#;
    let mut e = Editor::open(&doc_with(&[note, &para("Normal", "See ")].concat(), HEADINGS), fonts()).unwrap();
    let t = e.cross_reference_targets(&RefKind::Footnote).remove(0);
    assert!(matches!(e.insert_cross_reference(&t, &xref(RefContent::Text), fonts()), Outcome::Refused(_)));
}

#[test]
fn typing_in_many_spanning_fields_keeps_them_whole() {
    // Many fields across paragraphs, then typing and deleting in the first one's first paragraph.
    let one = READ_TOC.split("<w:p><w:pPr>").next().unwrap();
    let body = one.repeat(2000) + &para("Heading1", "One");
    let mut e = Editor::open(&doc_with(&body, HEADINGS), fonts()).unwrap();
    let p = texts_ids(&e)[0];
    e.select_range(p, Unit(2), Unit(2), fonts());
    e.execute(Command::InsertText("x".into()), fonts());
    e.execute(super::del(Dir::Back, false), fonts());
    e.execute(super::del(Dir::Back, false), fonts());
    assert_eq!(e.tables_of_contents().len(), 2000);
    let (b, _, end) = balance(&saved(&e));
    assert_eq!(b, end);
}

#[test]
fn many_numbered_references_to_nothing() {
    let one = format!("{}<w:p>{}</w:p>", numbered(1, "Intro", None), field(r"REF missing \r", "x"));
    let body = one.repeat(5_000);
    let e = Editor::open(&doc_with_numbering(&body, HEADINGS, Some(OUTLINE)), fonts()).unwrap();
    assert_eq!(values(&e).len(), 5_000);
    assert!(values(&e).iter().all(|v| v.starts_with("Error!")));
}

#[test]
fn footnote_sequences_number_in_reference_order() {
    let body = format!("<w:p>{}{}</w:p><w:p><w:r><w:t>refs</w:t></w:r></w:p>", field("SEQ Figure", "9"), field("SEQ Figure", "9"));
    let e = Editor::open(&doc_with(&body, HEADINGS), fonts()).unwrap();
    let mut doc = e.document().clone();
    let Block::Para(seqs) = doc.sections[0].blocks[0].clone() else { panic!() };
    doc.footnotes.insert(7, vec![Block::Para(seqs.clone())]);
    doc.footnotes.insert(3, vec![Block::Para(seqs)]);
    let Block::Para(refs) = &mut doc.sections[0].blocks[1] else { panic!() };
    let run = refs.mark.clone();
    for id in [7, 3] {
        refs.inlines.push(Inline::NoteRef { kind: crate::docx::NoteKind::Foot, id, custom: false, run: run.clone() });
    }
    crate::docx::fields::evaluate(&mut doc, 0);
    let vals = |id: i64| {
        let Block::Para(p) = &doc.footnotes[&id][0] else { panic!() };
        p.inlines.iter().filter_map(|i| if let Inline::FieldBegin { value: Some(v), .. } = i { Some(v.clone()) } else { None }).collect::<Vec<_>>()
    };
    assert_eq!(vals(7), ["1", "2"]);
    assert_eq!(vals(3), ["3", "4"]);
}

#[test]
fn a_reference_in_a_footnote_goes_to_the_host() {
    let body = r#"<w:p><w:bookmarkStart w:id="1" w:name="Target"/><w:r><w:t>Old</w:t></w:r><w:bookmarkEnd w:id="1"/></w:p>"#;
    let mut e = Editor::open(&doc_with(body, HEADINGS), fonts()).unwrap();
    let ps = texts_ids(&e);
    e.select_range(ps[0], Unit(3), Unit(3), fonts());
    e.execute(Command::InsertFootnote, fonts());
    e.execute(Command::InsertField(crate::api::command::InsertField { code: " REF Target ".into(), display: None, show_code: None }), fonts());
    e.take_notices();
    e.execute(Command::SelectAll, fonts());
    e.execute(Command::UpdateFields(Default::default()), fonts());
    assert!(e.take_notices().iter().any(|n| matches!(n, super::super::input::Notice::FieldUpdate { .. })), "the host updates it");
}

fn control(lock: &str, inner: &str) -> String {
    format!(r#"<w:sdt><w:sdtPr>{lock}<w:id w:val="5"/></w:sdtPr><w:sdtContent>{inner}</w:sdtContent></w:sdt>"#)
}

#[test]
fn a_table_inside_a_control_stays_in_it() {
    let body = control("", &[para("Normal", "A"), para("Normal", "B")].concat()) + &para("Heading1", "One");
    let mut e = Editor::open(&doc_with(&body, HEADINGS), fonts()).unwrap();
    let b = texts_ids(&e)[1];
    e.select_range(b, Unit(0), Unit(0), fonts());
    let frame = TocFrame { title: Some("Contents".into()), control: true };
    assert!(matches!(e.insert_table_of_contents(&TocOptions::word_default(), TocLook::default(), &frame, fonts()), Outcome::Changed(_)));
    let outer = body_paras(&e)[0].controls[0];
    let inside: Vec<bool> = body_paras(&e).iter().map(|p| p.controls.first() == Some(&outer)).collect();
    // A, the title, the entry, the end's paragraph, B: in the control; the heading after it not.
    assert_eq!(inside, [true, true, true, true, true, false], "{:?}", body_paras(&e).iter().map(|p| p.controls.clone()).collect::<Vec<_>>());
    assert_eq!(body_paras(&e)[1].controls.len(), 2, "the building block inside it");
    assert_eq!(body_paras(&e)[3].controls.len(), 2, "the end inside the building block");
}

#[test]
fn locked_controls_refuse_captions_and_updates() {
    let locked = r#"<w:lock w:val="contentLocked"/>"#;
    let body = control(locked, &para("Normal", "Text")) + &para("Heading1", "One");
    let mut e = Editor::open(&doc_with(&body, HEADINGS), fonts()).unwrap();
    let p = texts_ids(&e)[0];
    e.select_range(p, Unit(1), Unit(1), fonts());
    assert!(!matches!(e.insert_caption(&caption_opts("Figure", ""), fonts()), Outcome::Changed(_)));
    assert_eq!(texts(&e).len(), 2);
    let body = control(locked, READ_TOC.rsplit_once("<w:p><w:pPr>").unwrap().0) + &para("Heading1", "One");
    let mut e = Editor::open(&doc_with(&body, HEADINGS), fonts()).unwrap();
    assert_eq!(e.tables_of_contents().len(), 1);
    assert!(!matches!(e.update_toc(&e.tables_of_contents()[0], TocUpdate::Entire, fonts()), Outcome::Changed(_)));
}

#[test]
fn captions_in_text_boxes_are_entries_and_targets() {
    let boxed = super::empty_stories::anchored_box(&format!("<w:txbxContent>{}</w:txbxContent>", caption("1", "Boxed")));
    let body = format!(r#"<w:p>{boxed}<w:r><w:t>anchor</w:t></w:r></w:p>{}"#, caption("2", "Plain"));
    let e = Editor::open(&super::empty_stories::build(&body, "", &[]), fonts()).unwrap();
    let names: Vec<String> = e.toc_entries(&TocOptions::figures("Figure")).into_iter().map(|x| x.text).collect();
    assert_eq!(names, ["Figure 1: Boxed", "Figure 2: Plain"]);
    let targets: Vec<String> = e.cross_reference_targets(&RefKind::Caption("Figure".into())).into_iter().map(|t| t.text).collect();
    assert_eq!(targets, ["Figure 1: Boxed", "Figure 2: Plain"]);
    // A heading in a text box isn't an entry.
    let boxed = super::empty_stories::anchored_box(&format!("<w:txbxContent>{}</w:txbxContent>", para("Heading1", "Inside")));
    let e = Editor::open(&super::empty_stories::build(&format!("<w:p>{boxed}</w:p>"), "", &[]), fonts()).unwrap();
    assert!(e.toc_entries(&TocOptions::word_default()).is_empty());
}

#[test]
fn undo_and_redo_insert_and_update_one_step_each() {
    let mut e = Editor::open(&sample(), fonts()).unwrap();
    at_start(&mut e);
    insert(&mut e, &TocOptions::word_default());
    let xml = saved(&e);
    e.execute(Command::Undo(Default::default()), fonts());
    assert!(e.tables_of_contents().is_empty());
    e.execute(Command::Redo(Default::default()), fonts());
    assert_eq!(saved(&e), xml);
    e.update_table_of_contents(TocUpdate::Entire, fonts());
    e.execute(Command::Undo(Default::default()), fonts());
    e.execute(Command::Redo(Default::default()), fonts());
    assert_eq!(e.tables_of_contents().len(), 1);
    assert_eq!(balance(&saved(&e)), (9, 9, 9));
    let t = e.tables_of_contents().remove(0);
    e.remove_table_of_contents(&t, fonts());
    e.execute(Command::Undo(Default::default()), fonts());
    e.execute(Command::Redo(Default::default()), fonts());
    assert!(e.tables_of_contents().is_empty());
    assert_eq!(balance(&saved(&e)), (0, 0, 0));
}

#[test]
fn a_table_in_a_cell_is_found_and_updated_there() {
    let table =
        format!(r#"<w:tbl><w:tblGrid><w:gridCol w:w="6000"/></w:tblGrid><w:tr><w:tc><w:p/></w:tc></w:tr></w:tbl>{}"#, para("Heading1", "Intro"));
    let mut e = Editor::open(&doc_with(&table, HEADINGS), fonts()).unwrap();
    at_start(&mut e);
    assert!(matches!(insert(&mut e, &TocOptions::word_default()), Outcome::Changed(_)));
    let t = e.tables_of_contents();
    assert_eq!(t.len(), 1);
    assert!(matches!(t[0].container, super::super::index::Container::Cell { .. }), "{:?}", t[0].container);
    assert!(matches!(e.update_table_of_contents(TocUpdate::Entire, fonts()), Outcome::Changed(_)));
    let again = Editor::open(&crate::docx::write::save(e.document(), e.docx()).unwrap(), fonts()).unwrap();
    assert_eq!(again.tables_of_contents().len(), 1);
}

#[test]
fn a_table_takes_so_many_entries_at_most() {
    let n = super::super::toc::MAX_ENTRIES + 5;
    let body: String = (0..n).map(|k| para("Heading1", &format!("H{k}"))).collect();
    let e = Editor::open(&doc_with(&body, HEADINGS), fonts()).unwrap();
    assert_eq!(e.toc_entries(&TocOptions::word_default()).len(), super::super::toc::MAX_ENTRIES);
    // Entry fields in one paragraph too.
    let tc = TocMark { text: "x".into(), level: 1, identifier: None, page_number: true }.code();
    let body = format!("<w:p>{}</w:p>", field(tc.trim(), "").repeat(n));
    let e = Editor::open(&doc_with(&body, HEADINGS), fonts()).unwrap();
    let o = TocOptions { entry_fields: Some("C".into()), ..TocOptions::default() };
    assert_eq!(e.toc_entries(&o).len(), super::super::toc::MAX_ENTRIES);
}

#[test]
fn every_table_updates_in_one_step_the_locked_ones_left() {
    let locked = READ_TOC.replace(r#"<w:fldChar w:fldCharType="begin"/>"#, r#"<w:fldChar w:fldCharType="begin" w:fldLock="1"/>"#);
    let body = [READ_TOC, READ_TOC, &locked].concat();
    let mut e = Editor::open(&doc_with(&body, HEADINGS), fonts()).unwrap();
    let t = e.tables_of_contents();
    assert_eq!(t.len(), 3);
    assert_eq!(t.iter().map(|t| e.toc_locked(t)).collect::<Vec<_>>(), [false, false, true]);
    assert_eq!(e.update_all_tables(TocUpdate::Entire, fonts()), 2);
    assert_eq!(e.tables_of_contents().len(), 3);
    e.execute(Command::Undo(Default::default()), fonts());
    assert_eq!(texts(&e).iter().filter(|t| *t == "Two").count(), 3, "one undo step back");
    // At the selection.
    let p = texts_ids(&e)[1];
    e.select_range(p, Unit(0), Unit(0), fonts());
    assert_eq!(e.toc_at_selection().map(|t| t.0), Some(0));
}

#[test]
fn a_reference_with_notes_brings_them_again_as_its_own() {
    let body = [para("Normal", "Alpha"), format!("<w:p>{}</w:p>", field("REF Mark \\f", "old"))].concat();
    let mut e = Editor::open(&doc_with(&body, HEADINGS), fonts()).unwrap();
    let alpha = texts_ids(&e)[0];
    e.select_range(alpha, Unit(5), Unit(5), fonts());
    e.execute(Command::InsertFootnote, fonts());
    e.execute(Command::InsertText("Said.".into()), fonts());
    e.select_range(alpha, Unit(0), Unit(6), fonts());
    e.execute(Command::InsertBookmark("Mark".into()), fonts());
    let at_ref = texts_ids(&e)[1];
    let count = |e: &Editor| e.document().footnotes.len();
    let before = count(&e);
    let result_notes = |e: &Editor| -> Vec<i64> {
        body_paras(e)[1].inlines.iter().filter_map(|i| if let Inline::NoteRef { id, .. } = i { Some(*id) } else { None }).collect()
    };
    for round in 0..2 {
        e.select_range(at_ref, Unit(0), Unit(0), fonts());
        e.execute(Command::UpdateFields(Default::default()), fonts());
        let made = result_notes(&e);
        assert_eq!(made.len(), 1, "round {round}: {:?}", body_paras(&e)[1].inlines);
        assert_eq!(count(&e), before + 1, "round {round}: the old copy goes");
        let Some(Block::Para(p)) = e.document().footnotes[&made[0]].first() else { panic!() };
        assert!(shown(p).ends_with("Said."), "{}", shown(p));
        assert!(shown(body_paras(&e)[1]).starts_with("Alpha"), "{}", shown(body_paras(&e)[1]));
    }
    let xml = saved(&e);
    assert_eq!(xml.matches("<w:footnoteReference").count(), 2, "{xml}");
    let again = Editor::open(&crate::docx::write::save(e.document(), e.docx()).unwrap(), fonts()).unwrap();
    assert_eq!(again.document().footnotes.len(), before + 1);
    e.execute(Command::Undo(Default::default()), fonts());
    assert_eq!(count(&e), before + 1);
}

#[test]
fn a_table_before_a_heading_still_lists_it() {
    let body = [para("Heading1", "Intro"), para("Heading1", "End")].concat();
    let mut e = Editor::open(&doc_with(&body, HEADINGS), fonts()).unwrap();
    at_start(&mut e);
    insert(&mut e, &TocOptions::word_default());
    let names = |e: &Editor| {
        let t = e.tables_of_contents().remove(0);
        body_paras(e)[t.first..=t.last].iter().map(|p| shown(p).split('\t').next().unwrap().to_string()).collect::<Vec<_>>()
    };
    assert_eq!(names(&e), ["Intro", "End", "Intro"]);
    let heading = body_paras(&e)[2];
    assert!(matches!(heading.inlines.first(), Some(i) if super::super::toc::edge(i).is_some()), "the end first: {:?}", heading.inlines.first());
    e.update_table_of_contents(TocUpdate::Entire, fonts());
    assert_eq!(names(&e), ["Intro", "End", "Intro"]);
}

// Tracked changes

fn tracked(e: &mut Editor) {
    e.execute(Command::SetTrackChanges(true), fonts());
}

fn resolve(e: &mut Editor, accept: bool) {
    e.execute(if accept { Command::AcceptAllRevisions(Default::default()) } else { Command::RejectAllRevisions(Default::default()) }, fonts());
}

#[test]
fn a_tracked_insert_is_one_inserted_field() {
    for accept in [true, false] {
        let mut e = Editor::open(&sample(), fonts()).unwrap();
        let before = texts(&e);
        tracked(&mut e);
        at_start(&mut e);
        insert(&mut e, &TocOptions::word_default());
        let xml = saved(&e);
        assert!(xml.contains(r#"<w:ins "#) && balance(&xml) == (9, 9, 9), "{xml}");
        let again = Editor::open(&crate::docx::write::save(e.document(), e.docx()).unwrap(), fonts()).unwrap();
        assert_eq!(again.tables_of_contents().len(), 1);
        resolve(&mut e, accept);
        let xml = saved(&e);
        assert!(!xml.contains("<w:ins "), "{xml}");
        if accept {
            assert_eq!(e.tables_of_contents().len(), 1);
            assert_eq!(balance(&xml), (9, 9, 9), "{xml}");
        } else {
            assert!(e.tables_of_contents().is_empty(), "{:?}", edges(&e));
            assert_eq!(balance(&xml), (0, 0, 0), "{xml}");
            assert_eq!(texts(&e), before);
        }
    }
}

#[test]
fn a_tracked_update_deletes_the_old_field_and_inserts_the_new() {
    for accept in [true, false] {
        let mut e = Editor::open(&doc_with(READ_TOC, HEADINGS), fonts()).unwrap();
        tracked(&mut e);
        assert!(matches!(e.update_table_of_contents(TocUpdate::Entire, fonts()), Outcome::Changed(_)));
        let xml = saved(&e);
        let (b, s, n) = balance(&xml);
        assert!(b == n && s == b && xml.contains("<w:del ") && xml.contains("<w:ins "), "{xml}");
        let again = Editor::open(&crate::docx::write::save(e.document(), e.docx()).unwrap(), fonts()).unwrap();
        assert!(!again.tables_of_contents().is_empty());
        resolve(&mut e, accept);
        let xml = saved(&e);
        assert_eq!(e.tables_of_contents().len(), 1, "{accept}: {:?}", edges(&e));
        let (b, s, n) = balance(&xml);
        assert!(b == n && s == b, "{accept}: {xml}");
        let ps = body_paras(&e);
        let t = &e.tables_of_contents()[0];
        let entries: Vec<String> = ps[t.first..=t.last].iter().map(|p| shown(p).split('\t').next().unwrap().to_string()).collect();
        if accept {
            assert_eq!(entries[0], "One", "{entries:?}");
            assert_eq!(page_results(ps[t.first]), ["1"]);
        } else {
            assert_eq!(entries[..2], ["One", "Two"], "{entries:?}");
        }
    }
}

#[test]
fn a_tracked_deletion_of_a_whole_table_takes_its_field() {
    for accept in [true, false] {
        let mut e = Editor::open(&doc_with(READ_TOC, HEADINGS), fonts()).unwrap();
        tracked(&mut e);
        let ps = texts_ids(&e);
        e.select_between((ps[0], Unit(0)), (ps[2], Unit(0)), fonts());
        e.execute(super::del(Dir::Back, false), fonts());
        let xml = saved(&e);
        assert!(xml.contains("<w:delInstrText"), "{xml}");
        // Read back, the deletion holds the field's markers.
        let mut again = Editor::open(&crate::docx::write::save(e.document(), e.docx()).unwrap(), fonts()).unwrap();
        for e in [&mut e, &mut again] {
            resolve(e, accept);
            let xml = saved(e);
            if accept {
                assert!(e.tables_of_contents().is_empty(), "{:?}", edges(e));
                assert_eq!(balance(&xml), (0, 0, 0), "{xml}");
            } else {
                assert_eq!(e.tables_of_contents().len(), 1);
                assert_eq!(balance(&xml), (1, 1, 1), "{xml}");
            }
        }
    }
}

#[test]
fn a_tracked_removal_deletes_the_field_whole() {
    let mut e = Editor::open(&sample(), fonts()).unwrap();
    at_start(&mut e);
    insert(&mut e, &TocOptions::word_default());
    tracked(&mut e);
    let t = e.tables_of_contents().remove(0);
    assert!(matches!(e.remove_table_of_contents(&t, fonts()), Outcome::Changed(_)));
    assert_eq!(balance(&saved(&e)), (9, 9, 9));
    resolve(&mut e, true);
    assert!(e.tables_of_contents().is_empty(), "{:?}", edges(&e));
    assert_eq!(balance(&saved(&e)), (0, 0, 0));
}

// Hostile documents

#[test]
fn many_references_with_notes_update_in_one_pass() {
    let n = 2_000;
    let refs = format!("<w:p>{}</w:p>", field("REF Mark \\f", "old")).repeat(n);
    let body = [para("Normal", "Alpha"), refs].concat();
    let mut e = Editor::open(&doc_with(&body, HEADINGS), fonts()).unwrap();
    let alpha = texts_ids(&e)[0];
    e.select_range(alpha, Unit(5), Unit(5), fonts());
    e.execute(Command::InsertFootnote, fonts());
    e.execute(Command::InsertText("Said.".into()), fonts());
    e.select_range(alpha, Unit(0), Unit(6), fonts());
    e.execute(Command::InsertBookmark("Mark".into()), fonts());
    let before = e.document().footnotes.len();
    let source: Vec<i64> = e.document().footnotes.keys().copied().collect();
    // Twice: the second round's old copies go.
    for round in 0..2 {
        e.execute(Command::SelectAll, fonts());
        e.execute(Command::UpdateFields(Default::default()), fonts());
        assert_eq!(e.document().footnotes.len(), before + n, "round {round}");
        // Each result owns a note of its own, and every note is referred to.
        let made: Vec<i64> = body_paras(&e)[1..]
            .iter()
            .flat_map(|p| p.inlines.iter())
            .filter_map(|i| if let Inline::NoteRef { id, .. } = i { Some(*id) } else { None })
            .collect();
        let distinct: std::collections::HashSet<i64> = made.iter().copied().collect();
        assert_eq!((made.len(), distinct.len()), (n, n), "round {round}");
        assert!(made.iter().all(|id| e.document().footnotes.contains_key(id) && !source.contains(id)), "round {round}");
        assert!(e.document().footnotes.keys().all(|k| distinct.contains(k) || source.contains(k)), "round {round}");
    }
}

#[test]
fn an_entire_update_drops_many_orphan_marks_in_one_walk() {
    let n = 3_000;
    let refs: String = (0..n).map(|k| field(&format!("PAGEREF _Toc{k:09} \\h"), "1")).collect();
    let toc = READ_TOC.replacen("<w:r><w:t>Two</w:t></w:r>", &format!("<w:r><w:t>Two</w:t></w:r>{refs}"), 1);
    let marked: String = (0..n)
        .map(|k| format!(r#"<w:p><w:bookmarkStart w:id="{k}" w:name="_Toc{k:09}"/><w:r><w:t>x</w:t></w:r><w:bookmarkEnd w:id="{k}"/></w:p>"#))
        .collect();
    let body = toc + &marked;
    let mut e = Editor::open(&doc_with(&body, HEADINGS), fonts()).unwrap();
    let hidden = |e: &Editor| e.bookmarks().into_iter().filter(|b| b.name.starts_with("_Toc")).count();
    assert_eq!(hidden(&e), n);
    let t = e.tables_of_contents().remove(0);
    assert!(matches!(e.update_toc(&t, TocUpdate::Entire, fonts()), Outcome::Changed(_)));
    assert_eq!(e.tables_of_contents().len(), 1);
    // The new table's one mark, on its heading.
    assert_eq!(hidden(&e), 1);
    assert!(!saved(&e).contains("_Toc000000001"));
}

#[test]
fn updating_many_tables_walks_the_body_once_per_rebuild_at_most() {
    let n = 1_500;
    let body = format!("<w:p>{}</w:p>", field(r#"TOC \o "1-1""#, "x")).repeat(n);
    let mut e = Editor::open(&doc_with(&body, HEADINGS), fonts()).unwrap();
    assert_eq!(e.tables_of_contents().len(), n);
    assert_eq!(e.update_all_tables(TocUpdate::Pages, fonts()), n);
    assert_eq!(e.update_all_tables(TocUpdate::Entire, fonts()), super::super::toc::MAX_REBUILT);
    assert_eq!(e.tables_of_contents().len(), n);
}

#[test]
fn a_copied_table_holding_a_table_of_contents_keeps_it() {
    let table =
        format!(r#"<w:tbl><w:tblGrid><w:gridCol w:w="6000"/></w:tblGrid><w:tr><w:tc><w:p/></w:tc></w:tr></w:tbl>{}"#, para("Heading1", "Intro"));
    let mut e = Editor::open(&doc_with(&table, HEADINGS), fonts()).unwrap();
    at_start(&mut e);
    insert(&mut e, &TocOptions::word_default());
    assert_eq!(e.tables_of_contents().len(), 1);
    // The table, copied whole: both edges in its cell.
    let table = e.document().sections[0].blocks[..1].to_vec();
    let f = e.clip_blocks(table);
    let Some(Block::Table(t)) = f.blocks.first() else { panic!("a table") };
    let in_cell = t.rows[0].cells[0].blocks.iter().filter_map(|b| if let Block::Para(p) = b { Some(p) } else { None });
    assert_eq!(in_cell.flat_map(|p| p.inlines.iter().filter_map(|i| super::super::toc::edge(i).cloned())).count(), 2);
    let last = *texts_ids(&e).last().unwrap();
    e.select_range(last, Unit(5), Unit(5), fonts());
    e.execute(Command::InsertParagraph, fonts());
    e.paste_fragment(&f, fonts());
    assert_eq!(e.tables_of_contents().len(), 2);
    let (b, s, n) = balance(&saved(&e));
    assert!(b == s && s == n, "{b} {s} {n}");
}

#[test]
fn page_and_note_references_read_above_below_or_on_page() {
    let body = [para("Normal", "Before "), para("Heading1", "Target"), para("Normal", "Near "), PAGE.into(), para("Normal", "Far ")].concat();
    let mut e = Editor::open(&doc_with(&body, HEADINGS), fonts()).unwrap();
    let ps = texts_ids(&e);
    // A footnote in the heading.
    e.select_range(ps[1], Unit(6), Unit(6), fonts());
    e.execute(Command::InsertFootnote, fonts());
    let (before, near, far) = (ps[0], ps[2], *texts_ids(&e).last().unwrap());
    let put = |e: &mut Editor, at: crate::layout::ParaId, kind: RefKind, content: RefContent| {
        let t = e.cross_reference_targets(&kind).remove(0);
        let end = crate::editing::step::para_len(e.index().para(e.document(), at).unwrap());
        e.select_range(at, end, end, fonts());
        let out = e.insert_cross_reference(&t, &CrossReference { content, hyperlink: true, above_below: true }, fonts());
        assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    };
    for p in [before, near, far] {
        put(&mut e, p, RefKind::Heading, RefContent::PageNumber);
        e.execute(Command::InsertText(" ".into()), fonts());
        put(&mut e, p, RefKind::Footnote, RefContent::NoteNumber);
    }
    let para_of = |e: &Editor, id| e.index().para(e.document(), id).unwrap().clone();
    let p = para_of(&e, far);
    let cs = codes(&p);
    assert_eq!(cs.len(), 2, "the switch on the field itself: {cs:?}");
    assert!(cs[0].starts_with("PAGEREF _Ref") && cs[0].contains("\\p"), "{cs:?}");
    assert!(cs[1].starts_with("NOTEREF _Ref") && cs[1].contains("\\p"), "{cs:?}");
    // The cached results.
    assert_eq!(shown(&para_of(&e, before)), "Before below 1 below");
    assert_eq!(shown(&para_of(&e, near)), "Near above 1 above");
    assert_eq!(shown(&p), "Far on page 1 1 above");
    // As laid out.
    let pages: Vec<String> = crate::layout::tests::page_chars(e.document())
        .into_iter()
        .map(|mut p| {
            p.sort_by(|a, b| a.2.total_cmp(&b.2).then(a.1.total_cmp(&b.1)));
            p.into_iter().map(|c| c.0).filter(|c| !c.is_whitespace()).collect()
        })
        .collect();
    assert!(pages[0].contains("Beforebelow1below") && pages[0].contains("Nearabove1above"), "{pages:?}");
    assert!(pages[1].contains("Faronpage11above"), "{pages:?}");
}

#[test]
fn a_reference_after_its_bookmark_in_one_paragraph_reads_above() {
    let body = format!(
        r#"<w:p>{}<w:bookmarkStart w:id="1" w:name="Mark"/><w:r><w:t>here</w:t></w:r><w:bookmarkEnd w:id="1"/>{}</w:p>"#,
        field(r"REF Mark \p", "x"),
        field(r"REF Mark \p", "x")
    );
    let e = Editor::open(&doc_with(&body, HEADINGS), fonts()).unwrap();
    assert_eq!(values(&e), ["below", "above"]);
}

#[test]
fn an_aligned_floating_pictures_caption_goes_below_it() {
    use crate::layout::tests::shapes::{anchor, bytes, sp};
    let a = anchor(100, 300, 60, 30, 1, false, 5, &sp(0, 0, 60, 30, "rect", "", "0000FF"));
    let aligned = a.replacen(
        &a[a.find("<wp:positionV").unwrap()..a.find("</wp:positionV>").unwrap() + "</wp:positionV>".len()],
        r#"<wp:positionV relativeFrom="margin"><wp:align>bottom</wp:align></wp:positionV>"#,
        1,
    );
    let mut e = Editor::open(&bytes(&format!("<w:p><w:r><w:t>Text</w:t></w:r>{aligned}</w:p>")).unwrap(), fonts()).unwrap();
    e.complete(fonts());
    let mut ids = Vec::new();
    super::super::paste::tests_each_float(&e.document().sections[0].blocks, &mut |f| ids.push(crate::layout::FloatId(f)));
    let f = ids[0];
    e.select_float(f);
    let (_, pic) = e.float_box(f, fonts()).unwrap();
    assert!(matches!(e.insert_caption(&caption_opts("Figure", ""), fonts()), Outcome::Changed(_)));
    e.complete(fonts());
    let mut ids = Vec::new();
    super::super::paste::tests_each_float(&e.document().sections[0].blocks, &mut |f| ids.push(crate::layout::FloatId(f)));
    let boxed = *ids.iter().find(|&&g| g != f).unwrap();
    let (_, cap) = e.float_box(boxed, fonts()).unwrap();
    assert!((cap[1] - (pic[3] + 3.6)).abs() < 0.1, "picture {pic:?}, caption {cap:?}");
}

#[test]
fn a_caption_in_an_editable_range_renumbers_only_what_protection_leaves_open() {
    use crate::api::command::{DocumentProtection, ProtectionKind};
    let open = r#"<w:p><w:permStart w:id="1" w:edGrp="everyone"/><w:r><w:t>Pic</w:t></w:r><w:permEnd w:id="1"/></w:p>"#.to_string();
    let body = [open, caption("1", "Later")].concat();
    let mut e = Editor::open(&doc_with(&body, HEADINGS), fonts()).unwrap();
    let p = DocumentProtection { kind: Some(ProtectionKind::ReadOnly), enforce: Some(true), ..Default::default() };
    e.execute(Command::SetDocumentProtection(p), fonts());
    let pic = texts_ids(&e)[0];
    e.select_range(pic, Unit(1), Unit(1), fonts());
    assert!(matches!(e.insert_caption(&caption_opts("Figure", ""), fonts()), Outcome::Changed(_)));
    let caps: Vec<String> = body_paras(&e).iter().filter(|p| style_of(p) == "Caption").map(|p| shown(p)).collect();
    // The later caption is outside the range: it keeps its number.
    assert_eq!(caps, ["Figure 1", "Figure 1: Later"]);
}

/// The text drawn on page 0, a string per line (glyph runs on one baseline, left to right).
fn drawn_lines(e: &mut Editor) -> Vec<String> {
    e.complete(fonts());
    let snap = e.snapshot();
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

fn show_codes(e: &mut Editor, show: bool) -> Outcome {
    e.execute(Command::ShowFieldCodes(crate::api::command::ShowFieldCodes { show, ids: None }), fonts())
}

fn view_of(e: &Editor) -> crate::docx::FieldView {
    edges(e).iter().flatten().find_map(|x| if let FieldEdge::Start { view, .. } = x { Some(*view) } else { None }).expect("a start")
}

fn folds(e: &Editor) -> usize {
    body_paras(e).iter().filter(|p| crate::docx::fields::folded(p)).count()
}

#[test]
fn alt_f9_shows_a_tables_code_on_one_line_with_its_result_folded() {
    let mut e = Editor::open(&doc_with(READ_TOC, HEADINGS), fonts()).unwrap();
    assert!(drawn_lines(&mut e).iter().any(|l| l == "Two"), "{:?}", drawn_lines(&mut e));
    let toggle = e.field_codes_toggle(true).expect("a toggle");
    assert!(toggle.show && toggle.ids.is_none());
    assert!(matches!(e.execute(Command::ShowFieldCodes(toggle), fonts()), Outcome::Changed(_)));
    assert_eq!(view_of(&e), crate::docx::FieldView::Code);
    assert_eq!(folds(&e), 2, "the result's paragraphs fold into the code's line");
    let lines = drawn_lines(&mut e);
    assert_eq!(lines, [r#"{ TOC \o "1-3" \h }"#, "One"], "the code alone, the heading after it");
    let again = e.field_codes_toggle(true).expect("a toggle");
    assert!(!again.show, "every field shows its code");
    assert!(matches!(show_codes(&mut e, false), Outcome::Changed(_)));
    assert_eq!(view_of(&e), crate::docx::FieldView::Result);
    assert_eq!(folds(&e), 0);
    assert_eq!(drawn_lines(&mut e), ["One", "Two", "One"]);
    assert_eq!(
        edges(&e)[0],
        [FieldEdge::Start { code: r#" TOC \o "1-3" \h "#.into(), lock: false, dirty: false, view: crate::docx::FieldView::Result }]
    );
}

#[test]
fn a_hidden_codes_braces_are_hidden_with_it() {
    let hidden = READ_TOC
        .replacen(r#"<w:r><w:fldChar w:fldCharType="begin"/>"#, r#"<w:r><w:rPr><w:vanish/></w:rPr><w:fldChar w:fldCharType="begin"/>"#, 1)
        .replacen("<w:r><w:instrText", "<w:r><w:rPr><w:vanish/></w:rPr><w:instrText", 1);
    let mut e = Editor::open(&doc_with(&hidden, HEADINGS), fonts()).unwrap();
    assert!(matches!(show_codes(&mut e, true), Outcome::Changed(_)));
    assert_eq!(drawn_lines(&mut e), ["One"]);
}

#[test]
fn shift_f9_in_a_table_toggles_it_and_undo_leaves_the_view() {
    let mut e = Editor::open(&doc_with(READ_TOC, HEADINGS), fonts()).unwrap();
    let two = texts_ids(&e)[1];
    e.select_range(two, Unit(1), Unit(1), fonts());
    let toggle = e.field_codes_toggle(false).expect("the table at the caret");
    assert_eq!(toggle.ids, Some(Vec::new()), "the field across paragraphs at the selection");
    e.execute(Command::ShowFieldCodes(toggle), fonts());
    assert_eq!(view_of(&e), crate::docx::FieldView::Code);
    // The caret goes before the code's closing brace.
    let first = texts_ids(&e)[0];
    let code = r#" TOC \o "1-3" \h "#.chars().count() as u32;
    assert_eq!(e.selection().focus().and_then(|p| p.para_offset()), Some((first, Unit(code + 1))));
    let back = e.field_codes_toggle(false).expect("the table at the caret");
    assert!(!back.show);
    assert_eq!(e.history().undo_depth(), 0, "a view, not an edit");
    assert!(!e.is_dirty());
    assert!(!matches!(e.execute(Command::Undo(Default::default()), fonts()), Outcome::Changed(_)), "nothing to undo");
    assert_eq!(view_of(&e), crate::docx::FieldView::Code);
}

/// [`READ_TOC`] with a PAGE field in a paragraph after it.
fn toc_and_page() -> Editor {
    let page = r#"<w:p><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> PAGE </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>1</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r></w:p>"#;
    Editor::open(&doc_with(&[READ_TOC, page, r#"<w:p><w:r><w:t>Tail</w:t></w:r></w:p>"#].concat(), HEADINGS), fonts()).unwrap()
}

fn page_view(e: &Editor) -> crate::docx::FieldView {
    body_paras(e)
        .iter()
        .flat_map(|p| &p.inlines)
        .find_map(|i| if let Inline::FieldBegin { view, .. } = i { Some(*view) } else { None })
        .expect("the PAGE field")
}

fn caret_at_end(e: &mut Editor, k: usize) {
    let id = texts_ids(e)[k];
    let len = e.index.para(e.document(), id).map(super::super::step::para_len).unwrap();
    e.select_range(id, len, len, fonts());
}

fn type_at_end(e: &mut Editor, k: usize, s: &str) {
    caret_at_end(e, k);
    e.execute(Command::InsertText(s.into()), fonts());
}

#[test]
fn undo_after_alt_f9_takes_back_the_edit_before_and_keeps_every_code_shown() {
    let mut e = toc_and_page();
    type_at_end(&mut e, 5, "x");
    e.mark_saved();
    let (depth, label) = (e.history().undo_depth(), e.history().next_undo().map(str::to_string));
    assert!(matches!(show_codes(&mut e, true), Outcome::Changed(_)));
    assert_eq!((view_of(&e), page_view(&e)), (crate::docx::FieldView::Code, crate::docx::FieldView::Code));
    assert_eq!((e.history().undo_depth(), e.history().next_undo().map(str::to_string)), (depth, label), "no undo step");
    assert!(!e.is_dirty(), "a view doesn't modify the document");
    assert!(matches!(e.execute(Command::Undo(Default::default()), fonts()), Outcome::Changed(_)));
    assert_eq!(texts(&e).last().map(String::as_str), Some("Tail"), "the typing went");
    assert_eq!((view_of(&e), page_view(&e)), (crate::docx::FieldView::Code, crate::docx::FieldView::Code), "both still show codes");
    assert_eq!(folds(&e), 2);
    e.execute(Command::Redo(Default::default()), fonts());
    assert_eq!(texts(&e).last().map(String::as_str), Some("Tailx"));
    assert_eq!((view_of(&e), folds(&e)), (crate::docx::FieldView::Code, 2));
    assert!(!e.is_dirty());
    show_codes(&mut e, false);
    assert_eq!((view_of(&e), folds(&e), page_view(&e)), (crate::docx::FieldView::Result, 0, crate::docx::FieldView::Result));
    assert_eq!(texts(&e)[..2], ["One", "Two"]);
}

#[test]
fn an_edit_in_a_shown_code_undoes_and_redoes_in_any_view() {
    let mut e = Editor::open(&doc_with(READ_TOC, HEADINGS), fonts()).unwrap();
    type_at_end(&mut e, 3, "!");
    show_codes(&mut e, true);
    let first = texts_ids(&e)[0];
    let code = r#" TOC \o "1-3" \h "#.chars().count() as u32;
    e.select_range(first, Unit(code + 1), Unit(code + 1), fonts());
    e.execute(Command::InsertText("\\z ".into()), fonts());
    let edited = r#" TOC \o "1-3" \h \z "#;
    assert_eq!(e.tables_of_contents()[0].code, edited);
    // Shown as results, then the code's edit undone and redone
    show_codes(&mut e, false);
    e.execute(Command::Undo(Default::default()), fonts());
    assert_eq!((view_of(&e), folds(&e)), (crate::docx::FieldView::Result, 0), "the view stays");
    assert_eq!(e.tables_of_contents()[0].code, r#" TOC \o "1-3" \h "#);
    e.execute(Command::Redo(Default::default()), fonts());
    assert_eq!((view_of(&e), folds(&e)), (crate::docx::FieldView::Result, 0));
    assert_eq!(e.tables_of_contents()[0].code, edited);
    // Back past the code's edit and the typing before it: codes shown, as now
    show_codes(&mut e, true);
    e.execute(Command::Undo(Default::default()), fonts());
    e.execute(Command::Undo(Default::default()), fonts());
    assert_eq!(texts(&e).last().map(String::as_str), Some("One"));
    assert_eq!((view_of(&e), folds(&e)), (crate::docx::FieldView::Code, 2));
    assert_eq!(drawn_lines(&mut e)[0], r#"{ TOC \o "1-3" \h }"#);
    show_codes(&mut e, false);
    e.execute(Command::Redo(Default::default()), fonts());
    e.execute(Command::Redo(Default::default()), fonts());
    assert_eq!((view_of(&e), folds(&e)), (crate::docx::FieldView::Result, 0));
    assert_eq!(e.tables_of_contents()[0].code, edited);
    assert_eq!(texts(&e).last().map(String::as_str), Some("One!"));
    assert_eq!(e.history().redo_depth(), 0);
    show_codes(&mut e, true);
    assert_eq!(drawn_lines(&mut e)[0], r#"{ TOC \o "1-3" \h \z }"#);
}

#[test]
fn a_composition_taken_back_leaves_the_view_and_the_redo_steps() {
    let mut e = Editor::open(&doc_with(READ_TOC, HEADINGS), fonts()).unwrap();
    // An entry's edit, undone: redone where the result shows
    type_at_end(&mut e, 0, "!");
    e.execute(Command::Undo(Default::default()), fonts());
    show_codes(&mut e, true);
    caret_at_end(&mut e, 3);
    e.compose("k", 1, fonts());
    e.end_composition(fonts());
    assert_eq!((e.history().undo_depth(), e.history().redo_depth()), (0, 1));
    assert_eq!((view_of(&e), folds(&e)), (crate::docx::FieldView::Code, 2));
    e.execute(Command::Redo(Default::default()), fonts());
    assert_eq!((view_of(&e), folds(&e)), (crate::docx::FieldView::Code, 2));
    assert_eq!(drawn_lines(&mut e)[0], r#"{ TOC \o "1-3" \h }"#);
    show_codes(&mut e, false);
    assert_eq!(texts(&e)[..2], ["One!", "Two"]);
}

#[test]
fn a_shown_code_is_edited_as_text_and_saved_as_the_fields_code() {
    let mut e = Editor::open(&doc_with(READ_TOC, HEADINGS), fonts()).unwrap();
    show_codes(&mut e, true);
    let first = texts_ids(&e)[0];
    let code = r#" TOC \o "1-3" \h "#.chars().count() as u32;
    e.select_range(first, Unit(code + 1), Unit(code + 1), fonts());
    e.execute(Command::InsertText("\\z ".into()), fonts());
    assert_eq!(drawn_lines(&mut e)[0], r#"{ TOC \o "1-3" \h \z }"#);
    assert_eq!(e.tables_of_contents()[0].code, r#" TOC \o "1-3" \h \z "#, "the table reads its code as shown");
    // Saved showing the code: the field's code, its result, no braces or code text.
    let xml = saved(&e);
    assert!(xml.contains(r#" TOC \o "1-3" \h \z "#), "{xml}");
    assert!(!xml.contains("{") && !xml.contains(r#"<w:t xml:space="preserve"> TOC"#), "{xml}");
    assert_eq!(balance(&xml), (1, 1, 1));
    let back = Editor::open(&crate::docx::write::save(e.document(), e.docx()).unwrap(), fonts()).unwrap();
    assert_eq!(texts(&back)[..2], ["One", "Two"]);
    show_codes(&mut e, false);
    assert_eq!(
        edges(&e)[0],
        [FieldEdge::Start { code: r#" TOC \o "1-3" \h \z "#.into(), lock: false, dirty: false, view: crate::docx::FieldView::Result }]
    );
    assert_eq!(texts(&e)[..2], ["One", "Two"]);
}

#[test]
fn the_caret_steps_over_a_shown_codes_folded_result() {
    let mut e = Editor::open(&doc_with(READ_TOC, HEADINGS), fonts()).unwrap();
    show_codes(&mut e, true);
    let ids = texts_ids(&e);
    let (first, end_para) = (ids[0], ids[2]);
    let code = r#" TOC \o "1-3" \h "#.chars().count() as u32;
    let caret = |e: &Editor| e.selection().focus().and_then(|p| p.para_offset());
    e.select_range(first, Unit(code + 1), Unit(code + 1), fonts());
    e.execute(super::super::commands::moving(super::super::commands::MoveBy::Char, Dir::Forward, false), fonts());
    assert_eq!(caret(&e), Some((end_para, Unit(0))), "past the closing brace: after the field's end");
    e.execute(super::super::commands::moving(super::super::commands::MoveBy::Char, Dir::Back, false), fonts());
    assert_eq!(caret(&e), Some((first, Unit(code + 1))), "back before the brace");
    // A place in the folded result shows after the field's end.
    e.select_range(ids[1], Unit(1), Unit(1), fonts());
    assert_eq!(caret(&e), Some((end_para, Unit(0))));
    // Enter in the code leaves it on its line.
    e.select_range(first, Unit(3), Unit(3), fonts());
    assert!(matches!(e.execute(Command::InsertParagraph, fonts()), Outcome::Unchanged(_)));
    // Backspace after the field selects it whole, a second deletes it.
    e.select_range(end_para, Unit(0), Unit(0), fonts());
    e.execute(super::del(Dir::Back, false), fonts());
    let (a, b) = e.selection().ordered(&e.index).expect("a range");
    assert_eq!((a.para_offset(), b.para_offset()), (Some((first, Unit(0))), Some((end_para, Unit(0)))));
    e.execute(super::del(Dir::Back, false), fonts());
    assert!(edges(&e).iter().all(|x| x.is_empty()), "the field went whole");
    assert_eq!(folds(&e), 0);
}

#[test]
fn updating_a_table_showing_its_code_shows_its_result() {
    let mut e = Editor::open(&doc_with(READ_TOC, HEADINGS), fonts()).unwrap();
    show_codes(&mut e, true);
    let t = e.tables_of_contents().remove(0);
    assert!(matches!(e.update_toc(&t, TocUpdate::Entire, fonts()), Outcome::Changed(_)));
    assert_eq!(view_of(&e), crate::docx::FieldView::Result);
    assert_eq!(folds(&e), 0);
    assert!(drawn_lines(&mut e).iter().filter(|l| l.starts_with("One")).count() >= 2);
}

#[test]
fn alt_f9_on_a_made_table_shows_only_its_code() {
    let body = [para("Heading1", "Alpha"), para("Heading2", "Beta")].concat();
    let mut e = Editor::open(&doc_with(&body, HEADINGS), fonts()).unwrap();
    at_start(&mut e);
    insert(&mut e, &TocOptions::word_default());
    show_codes(&mut e, true);
    // The end is at the heading's start: the heading's text follows the code on its line, as Word shows it.
    let lines = drawn_lines(&mut e);
    assert_eq!(lines, [r#"{ TOC \o "1-3" \h \z \u }Alpha"#, "Beta"], "the inner fields are in the folded result");
}

#[test]
fn shift_f9_over_a_tables_entries_shows_the_tables_code() {
    use super::super::position::{Position, StoryId};
    use super::super::selection::{Granularity, Selection};
    let body = [para("Heading1", "Alpha"), para("Heading2", "Beta")].concat();
    let mut e = Editor::open(&doc_with(&body, HEADINGS), fonts()).unwrap();
    at_start(&mut e);
    insert(&mut e, &TocOptions::word_default());
    let ids = texts_ids(&e);
    // A caret in a \h entry: its HYPERLINK, the innermost field.
    e.select_range(ids[0], Unit(1), Unit(1), fonts());
    let one = e.field_codes_toggle(false).expect("the entry's field");
    assert!(one.ids.as_ref().is_some_and(|i| !i.is_empty()));
    // A selection across entries: the table.
    e.selection = Selection::range(Position::text(StoryId::Body, ids[0], Unit(1)), Position::text(StoryId::Body, ids[1], Unit(1)), Granularity::Char);
    let toggle = e.field_codes_toggle(false).expect("the table");
    assert_eq!(toggle.ids, Some(Vec::new()));
    assert!(toggle.show);
    assert!(matches!(e.execute(Command::ShowFieldCodes(toggle), fonts()), Outcome::Changed(_)));
    assert_eq!(view_of(&e), crate::docx::FieldView::Code);
    assert_eq!(drawn_lines(&mut e), [r#"{ TOC \o "1-3" \h \z \u }Alpha"#, "Beta"]);
}

#[test]
fn a_copy_of_a_table_showing_its_code_holds_its_result() {
    let mut e = Editor::open(&doc_with(READ_TOC, HEADINGS), fonts()).unwrap();
    show_codes(&mut e, true);
    let f = e.clip_blocks(e.document().sections[0].blocks[..3].to_vec());
    let paras: Vec<&Paragraph> = f.blocks.iter().filter_map(|b| if let Block::Para(p) = b { Some(p) } else { None }).collect();
    assert!(paras.iter().all(|p| !crate::docx::fields::shows_span_code(&p.inlines)));
    assert_eq!(shown(paras[0]), "One");
    let start = paras[0].inlines.iter().find_map(|i| super::super::toc::edge(i).cloned());
    assert_eq!(
        start,
        Some(FieldEdge::Start { code: r#" TOC \o "1-3" \h "#.into(), lock: false, dirty: false, view: crate::docx::FieldView::Result })
    );
}

#[test]
fn a_click_after_a_shown_codes_brace_puts_the_caret_after_the_field() {
    use super::super::input::PointerKind;
    use super::super::keymap::Mods;
    let mut e = Editor::open(&doc_with(READ_TOC, HEADINGS), fonts()).unwrap();
    show_codes(&mut e, true);
    e.complete(fonts());
    let ids = texts_ids(&e);
    let (first, end_para) = (ids[0], ids[2]);
    let pages = e.page_geometries();
    let (pg, g) = &pages[0];
    let (_, l) = g.lines_of(first).next().expect("the code's line");
    let last = l.stops.iter().filter(|s| s.para == first).map(|s| s.x).fold(f64::MIN, f64::max);
    let at = e.stack().to_doc(*pg, last + 30.0, (l.top + l.bottom) / 2.0).expect("on the stack");
    e.pointer_input(PointerKind::Down, at.0, at.1, 1, Mods::default(), fonts());
    e.pointer_input(PointerKind::Up, at.0, at.1, 1, Mods::default(), fonts());
    assert_eq!(e.selection().focus().and_then(|p| p.para_offset()), Some((end_para, Unit(0))));
    e.execute(Command::InsertText("x".into()), fonts());
    assert_eq!(drawn_lines(&mut e)[0], r#"{ TOC \o "1-3" \h }x"#, "typed after the field");
}

/// A field across paragraphs whose result holds a table: a DATABASE field's.
const READ_DATABASE: &str = concat!(
    r#"<w:p><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> DATABASE \d "x.mdb" </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r></w:p>"#,
    r#"<w:tbl><w:tblGrid><w:gridCol w:w="4000"/></w:tblGrid><w:tr><w:tc><w:p><w:r><w:t>Cell</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#,
    r#"<w:tbl><w:tblGrid><w:gridCol w:w="4000"/></w:tblGrid><w:tr><w:tc><w:p><w:r><w:t>More</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#,
    r#"<w:p><w:r><w:fldChar w:fldCharType="end"/></w:r></w:p>"#,
    r#"<w:p><w:r><w:t>After</w:t></w:r></w:p>"#,
);

#[test]
fn alt_f9_on_a_field_whose_result_holds_tables_shows_only_its_code() {
    let mut e = Editor::open(&doc_with(READ_DATABASE, HEADINGS), fonts()).unwrap();
    assert_eq!(drawn_lines(&mut e), ["Cell", "More", "After"]);
    show_codes(&mut e, true);
    assert_eq!(drawn_lines(&mut e), [r#"{ DATABASE \d "x.mdb" }"#, "After"], "the tables fold with the result");
    // A caret in a hidden table's cell shows after the field's end.
    let end_para = texts_ids(&e)[3];
    let Some(Block::Table(t)) = e.document().sections[0].blocks.get(1) else { panic!("a table") };
    let Some(Block::Para(cell)) = t.rows[0].cells[0].blocks.first() else { panic!("a cell paragraph") };
    let cell = ParaId(cell.id);
    e.select_range(cell, Unit(1), Unit(1), fonts());
    assert_eq!(e.selection().focus().and_then(|p| p.para_offset()), Some((end_para, Unit(0))));
    show_codes(&mut e, false);
    assert_eq!(drawn_lines(&mut e), ["Cell", "More", "After"]);
}

// Note copies, note targets and table handles

/// `body` with the footnotes and endnotes `(id, text)`.
fn with_notes(body: &str, foot: &[(i64, &str)], end: &[(i64, &str)]) -> Vec<u8> {
    use super::empty_stories::{NS, build};
    let notes = |tag: &str, list: &[(i64, &str)]| -> String {
        let inner: String = list.iter().map(|(id, t)| format!(r#"<w:{tag} w:id="{id}"><w:p><w:r><w:t>{t}</w:t></w:r></w:p></w:{tag}>"#)).collect();
        format!(r#"<w:{tag}s {NS}>{inner}</w:{tag}s>"#)
    };
    let (f, n) = (notes("footnote", foot), notes("endnote", end));
    build(body, "", &[("footnotes.xml", "footnotes", "footnotes", &f), ("endnotes.xml", "endnotes", "endnotes", &n)])
}

fn foot_ref(id: i64) -> String {
    format!(r#"<w:r><w:footnoteReference w:id="{id}"/></w:r>"#)
}

fn note_ids(p: &Paragraph) -> Vec<i64> {
    p.inlines.iter().filter_map(|i| if let Inline::NoteRef { id, .. } = i { Some(*id) } else { None }).collect()
}

fn note_text(e: &Editor, id: i64) -> String {
    match e.document().footnotes.get(&id).and_then(|b| b.first()) {
        Some(Block::Para(p)) => shown(p),
        _ => String::new(),
    }
}

fn update_all(e: &mut Editor) {
    e.execute(Command::SelectAll, fonts());
    e.execute(Command::UpdateFields(Default::default()), fonts());
}

#[test]
fn note_copies_take_ids_no_note_has_past_the_largest() {
    let max = i64::from(i32::MAX);
    let body = [
        format!(
            r#"<w:p><w:bookmarkStart w:id="1" w:name="Mark"/><w:r><w:t>A</w:t></w:r>{}{}<w:bookmarkEnd w:id="1"/></w:p>"#,
            foot_ref(2),
            foot_ref(max)
        ),
        format!("<w:p>{}</w:p>", field("REF Mark \\f", "old")),
    ]
    .concat();
    let mut e = Editor::open(&with_notes(&body, &[(2, "two"), (max, "last")], &[]), fonts()).unwrap();
    update_all(&mut e);
    let made = note_ids(body_paras(&e)[1]);
    assert_eq!(made.len(), 2, "{made:?}");
    assert!(made[0] != made[1] && made.iter().all(|id| *id != 2 && *id != max), "{made:?}");
    assert_eq!((note_text(&e, 2), note_text(&e, max)), ("two".to_string(), "last".to_string()));
    assert_eq!((note_text(&e, made[0]), note_text(&e, made[1])), ("two".to_string(), "last".to_string()));
}

#[test]
fn a_note_copy_has_fields_of_its_own() {
    let body = [
        format!(r#"<w:p><w:bookmarkStart w:id="1" w:name="Mark"/><w:r><w:t>A</w:t></w:r>{}<w:bookmarkEnd w:id="1"/></w:p>"#, foot_ref(1)),
        format!("<w:p>{}</w:p>", field("REF Mark \\f", "old")),
    ]
    .concat();
    let page = r#"a</w:t></w:r><w:fldSimple w:instr=" PAGE "><w:r><w:t>1</w:t></w:r></w:fldSimple><w:r><w:t>b"#;
    let mut e = Editor::open(&with_notes(&body, &[(1, page)], &[]), fonts()).unwrap();
    update_all(&mut e);
    assert_eq!(e.document().footnotes.len(), 2);
    let ids: Vec<crate::docx::FieldId> = e
        .document()
        .footnotes
        .values()
        .flatten()
        .filter_map(|b| if let Block::Para(p) = b { Some(p) } else { None })
        .flat_map(|p| &p.inlines)
        .filter_map(|i| if let Inline::FieldBegin { id, .. } = i { Some(*id) } else { None })
        .collect();
    assert_eq!(ids.len(), 2);
    assert_ne!(ids[0], ids[1]);
}

#[test]
fn a_tracked_update_keeps_the_note_its_old_result_still_holds() {
    let old = format!(
        r#"<w:p><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> REF Mark \f </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r>{}<w:r><w:fldChar w:fldCharType="end"/></w:r></w:p>"#,
        foot_ref(5)
    );
    let mark = format!(r#"<w:p><w:bookmarkStart w:id="1" w:name="Mark"/><w:r><w:t>A</w:t></w:r>{}<w:bookmarkEnd w:id="1"/></w:p>"#, foot_ref(1));
    let body = mark + &old;
    for tracked in [false, true] {
        let mut e = Editor::open(&with_notes(&body, &[(1, "one"), (5, "five")], &[]), fonts()).unwrap();
        e.execute(Command::SetTrackChanges(tracked), fonts());
        update_all(&mut e);
        assert_eq!(e.document().footnotes.contains_key(&5), tracked, "tracked {tracked}");
        if tracked {
            e.execute(Command::RejectAllRevisions(Default::default()), fonts());
            assert!(note_ids(body_paras(&e)[1]).contains(&5));
            assert_eq!(note_text(&e, 5), "five");
        }
    }
}

#[test]
fn a_reference_whose_notes_dont_all_fit_keeps_its_result_and_later_ones_fill() {
    let limit = super::super::xref::MAX_NOTE_COPIES;
    let mark = |name: &str, id: u32, refs: &[i64]| {
        let refs: String = refs.iter().map(|&r| foot_ref(r)).collect();
        format!(r#"<w:p><w:bookmarkStart w:id="{id}" w:name="{name}"/><w:r><w:t>{name}</w:t></w:r>{refs}<w:bookmarkEnd w:id="{id}"/></w:p>"#)
    };
    // One copy, then two each: the last of those has one copy left of the two it needs, which a field of one takes.
    let pairs = limit / 2;
    let body = [
        mark("One", 1, &[1]),
        mark("Two", 2, &[2, 3]),
        format!("<w:p>{}</w:p>", field("REF One \\f", "old")),
        format!("<w:p>{}</w:p>", field("REF Two \\f", "old")).repeat(pairs),
        format!("<w:p>{}</w:p>", field("REF One \\f", "old")),
    ]
    .concat();
    let mut e = Editor::open(&with_notes(&body, &[(1, "a"), (2, "b"), (3, "c")], &[]), fonts()).unwrap();
    update_all(&mut e);
    let ps = body_paras(&e);
    assert_eq!(note_ids(ps[ps.len() - 3]).len(), 2);
    let full = ps[ps.len() - 2];
    assert!(note_ids(full).is_empty() && shown(full) == "old", "{}", shown(full));
    assert_eq!(note_ids(ps[ps.len() - 1]).len(), 1);
    assert_eq!(e.document().footnotes.len(), 3 + limit);
}

#[test]
fn a_cross_reference_to_an_endnote_marks_the_endnote() {
    let body = format!(
        r#"<w:p><w:r><w:t>Both</w:t></w:r>{}<w:r><w:endnoteReference w:id="1"/></w:r></w:p><w:p><w:r><w:t xml:space="preserve">See </w:t></w:r></w:p>"#,
        foot_ref(1)
    );
    let mut e = Editor::open(&with_notes(&body, &[(1, "foot")], &[(1, "end")]), fonts()).unwrap();
    let t = e.cross_reference_targets(&RefKind::Endnote).remove(0);
    let see = texts_ids(&e)[1];
    e.select_range(see, Unit(4), Unit(4), fonts());
    assert!(matches!(e.insert_cross_reference(&t, &xref(RefContent::NoteNumber), fonts()), Outcome::Changed(_)));
    let mark = e.bookmarks().into_iter().find(|b| b.name.starts_with("_Ref")).unwrap();
    // After "Both" and the footnote's mark.
    assert_eq!(mark.start.unit, Unit(5));
}

#[test]
fn non_ascii_bookmark_names_are_no_hidden_marks() {
    let body = format!(
        r#"<w:p><w:bookmarkStart w:id="1" w:name="abcé"/><w:r><w:t>x</w:t></w:r><w:bookmarkEnd w:id="1"/></w:p>{}<w:p><w:r><w:t>See</w:t></w:r></w:p>"#,
        para("Heading1", "One")
    );
    let mut e = Editor::open(&doc_with(&body, HEADINGS), fonts()).unwrap();
    let t = e.cross_reference_targets(&RefKind::Heading).remove(0);
    let see = texts_ids(&e)[2];
    e.select_range(see, Unit(3), Unit(3), fonts());
    assert!(matches!(e.insert_cross_reference(&t, &xref(RefContent::Text), fonts()), Outcome::Changed(_)));
    at_start(&mut e);
    assert!(matches!(insert(&mut e, &TocOptions::default()), Outcome::Changed(_)));
}

#[test]
fn a_missing_bookmark_limits_a_table_to_nothing() {
    let body = [para("Heading1", "One"), para("Heading2", "Two")].concat();
    assert!(entries(&body, r#" TOC \o "1-3" \b Missing "#).is_empty());
    assert_eq!(entries(&body, r#" TOC \o "1-3" "#).len(), 2);
}

#[test]
fn a_hidden_run_before_a_captions_number_keeps_its_title_whole() {
    let hidden = r#"<w:r><w:rPr><w:vanish/></w:rPr><w:t xml:space="preserve">Secret </w:t></w:r>"#;
    let body = caption("1", "A cat").replacen("</w:pPr>", &format!("</w:pPr>{hidden}"), 1);
    let names: Vec<String> = entries(&body, r#" TOC \h \z \a "Figure" "#).into_iter().map(|x| x.1).collect();
    assert_eq!(names, ["A cat"]);
}

#[test]
fn a_table_pasted_after_another_fields_start_is_found() {
    let quote = r#"<w:p><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> QUOTE x </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>abcd</w:t></w:r></w:p><w:p><w:r><w:fldChar w:fldCharType="end"/></w:r></w:p>"#;
    let mut e = Editor::open(&doc_with(&[quote, READ_TOC].concat(), HEADINGS), fonts()).unwrap();
    assert_eq!(e.tables_of_contents().len(), 1);
    let f = e.clip_blocks(e.document().sections[0].blocks[2..5].to_vec());
    let p = texts_ids(&e)[0];
    e.select_range(p, Unit(2), Unit(2), fonts());
    e.paste_fragment(&f, fonts());
    // Both start in the first paragraph, the other field's first.
    assert_eq!(edges(&e)[0].len(), 2, "{:?}", edges(&e));
    assert_eq!(e.tables_of_contents().len(), 2);
}

#[test]
fn page_numbers_update_only_inside_the_table() {
    let after = format!(r#"<w:r><w:t xml:space="preserve">See </w:t></w:r>{}"#, field("PAGEREF Mark", "99"));
    let end = r#"<w:p><w:r><w:fldChar w:fldCharType="end"/></w:r></w:p>"#;
    let body = READ_TOC.replacen(end, &format!(r#"<w:p><w:r><w:fldChar w:fldCharType="end"/></w:r>{after}</w:p>"#), 1).replacen(
        "<w:r><w:t>Two</w:t></w:r>",
        &format!("<w:r><w:t>Two</w:t></w:r>{}", field("PAGEREF Mark", "7")),
        1,
    ) + r#"<w:p><w:bookmarkStart w:id="1" w:name="Mark"/><w:r><w:t>Here</w:t></w:r><w:bookmarkEnd w:id="1"/></w:p>"#;
    let mut e = Editor::open(&doc_with(&body, HEADINGS), fonts()).unwrap();
    let t = e.tables_of_contents().remove(0);
    assert!(matches!(e.update_toc(&t, TocUpdate::Pages, fonts()), Outcome::Changed(_)));
    assert_eq!((page_results(body_paras(&e)[1]), page_results(body_paras(&e)[2])), (vec!["1".to_string()], vec!["99".to_string()]));
}

#[test]
fn a_stale_table_handle_is_refused() {
    let body = [format!(r#"<w:p><w:r><w:t xml:space="preserve">Lead </w:t></w:r>{}</w:p>"#, field(r#"TOC \o "1-3""#, "x")), para("Heading1", "One")]
        .concat();
    let mut e = Editor::open(&doc_with(&body, HEADINGS), fonts()).unwrap();
    let t = e.tables_of_contents().remove(0);
    at_start(&mut e);
    e.execute(Command::InsertText("\t".into()), fonts());
    assert_ne!(e.tables_of_contents()[0], t, "the edit moved the field");
    assert!(!matches!(e.remove_table_of_contents(&t, fonts()), Outcome::Changed(_)));
    assert!(!matches!(e.update_toc(&t, TocUpdate::Entire, fonts()), Outcome::Changed(_)));
    assert_eq!(e.tables_of_contents().len(), 1);
    let now = e.tables_of_contents().remove(0);
    assert!(matches!(e.remove_table_of_contents(&now, fonts()), Outcome::Changed(_)));
    assert!(e.tables_of_contents().is_empty());
}

#[test]
fn a_mark_a_link_goes_to_stays() {
    let refs = field("PAGEREF _Toc000000001 \\h", "1");
    let toc = READ_TOC.replacen("<w:r><w:t>Two</w:t></w:r>", &format!("<w:r><w:t>Two</w:t></w:r>{refs}"), 1);
    let body = [
        toc,
        r#"<w:p><w:bookmarkStart w:id="1" w:name="_Toc000000001"/><w:r><w:t>x</w:t></w:r><w:bookmarkEnd w:id="1"/></w:p>"#.into(),
        format!("<w:p>{}</w:p>", field(r#"HYPERLINK \l "_Toc000000001""#, "go")),
    ]
    .concat();
    let mut e = Editor::open(&doc_with(&body, HEADINGS), fonts()).unwrap();
    let t = e.tables_of_contents().remove(0);
    assert!(matches!(e.update_toc(&t, TocUpdate::Entire, fonts()), Outcome::Changed(_)));
    assert!(e.bookmark("_Toc000000001").is_some());
}

#[test]
fn auto_caption_captions_a_table_made_in_a_cell() {
    let table = r#"<w:tbl><w:tblGrid><w:gridCol w:w="4000"/></w:tblGrid><w:tr><w:tc><w:p><w:r><w:t>cell</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#;
    let mut e = Editor::open(&doc_with(&[table, &para("Normal", "After")].concat(), HEADINGS), fonts()).unwrap();
    e.set_auto_caption(Some(caption_opts("Table", "")));
    let cell = texts_ids(&e)[0];
    e.select_range(cell, Unit(4), Unit(4), fonts());
    let out = e.execute(Command::InsertTable(serde_json::from_value(serde_json::json!({"rows": 1, "columns": 1})).unwrap()), fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    assert!(body_paras(&e).iter().all(|p| style_of(p) != "Caption"), "no caption beside the outer table");
    let Some(Block::Table(t)) = e.document().sections[0].blocks.first() else { panic!("the outer table first") };
    let in_cell: Vec<String> = t.rows[0].cells[0]
        .blocks
        .iter()
        .filter_map(|b| if let Block::Para(p) = b { Some(p) } else { None })
        .filter(|p| style_of(p) == "Caption")
        .map(shown)
        .collect();
    assert_eq!(in_cell, ["Table 1"]);
}

#[test]
fn a_percent_placed_floats_caption_takes_its_place_across() {
    use crate::layout::tests::shapes::{anchor, bytes, sp};
    let a = anchor(100, 300, 60, 30, 1, false, 5, &sp(0, 0, 60, 30, "rect", "", "0000FF"));
    let pct = r#"<wp:positionH relativeFrom="margin"><wp14:pctPosHOffset xmlns:wp14="http://schemas.microsoft.com/office/word/2010/wordprocessingDrawing">40000</wp14:pctPosHOffset></wp:positionH>"#;
    let placed = a.replacen(&a[a.find("<wp:positionH").unwrap()..a.find("</wp:positionH>").unwrap() + "</wp:positionH>".len()], pct, 1);
    let mut e = Editor::open(&bytes(&format!("<w:p><w:r><w:t>Text</w:t></w:r>{placed}</w:p>")).unwrap(), fonts()).unwrap();
    e.complete(fonts());
    let mut ids = Vec::new();
    super::super::paste::tests_each_float(&e.document().sections[0].blocks, &mut |f| ids.push(crate::layout::FloatId(f)));
    let f = ids[0];
    e.select_float(f);
    let (_, pic) = e.float_box(f, fonts()).unwrap();
    // 40% across the 468pt between the 72pt margins.
    assert!((pic[0] - (72.0 + 0.4 * 468.0)).abs() < 0.1, "picture {pic:?}");
    assert!(matches!(e.insert_caption(&caption_opts("Figure", ""), fonts()), Outcome::Changed(_)));
    e.complete(fonts());
    let mut ids = Vec::new();
    super::super::paste::tests_each_float(&e.document().sections[0].blocks, &mut |f| ids.push(crate::layout::FloatId(f)));
    let boxed = *ids.iter().find(|&&g| g != f).unwrap();
    let (_, cap) = e.float_box(boxed, fonts()).unwrap();
    assert!((cap[0] - pic[0]).abs() < 0.1, "picture {pic:?}, caption {cap:?}");
}

#[test]
fn a_table_of_contents_title_is_out_of_numbered_headings_list() {
    let heading = r#"<w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:basedOn w:val="Normal"/><w:pPr><w:numPr><w:numId w:val="1"/></w:numPr><w:outlineLvl w:val="0"/></w:pPr></w:style>"#;
    let numbering = r#"<w:abstractNum w:abstractNumId="1"><w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="decimal"/><w:lvlText w:val="Chapter %1"/><w:pStyle w:val="Heading1"/></w:lvl></w:abstractNum><w:num w:numId="1"><w:abstractNumId w:val="1"/></w:num>"#;
    let body = ["<w:p/>".to_string(), para("Heading1", "Intro")].concat();
    let mut e = Editor::open(&doc_from(&body, heading, Some(numbering)), fonts()).unwrap();
    at_start(&mut e);
    let frame = TocFrame { title: Some("Contents".into()), ..TocFrame::default() };
    assert!(matches!(e.insert_table_of_contents(&TocOptions::default(), TocLook::default(), &frame, fonts()), Outcome::Changed(_)));
    let again = Editor::open(&crate::docx::write::save(e.document(), e.docx()).unwrap(), fonts()).unwrap();
    let ps = body_paras(&again);
    let title = ps.iter().find(|p| style_of(p) == "TOCHeading").unwrap();
    assert!(title.list.as_ref().is_none_or(|l| l.text.is_empty()), "{:?}", title.list);
    let intro = ps.iter().find(|p| style_of(p) == "Heading1").unwrap();
    assert_eq!(intro.list.as_ref().map(|l| l.text.as_str()), Some("Chapter 1"));
}

#[test]
fn a_deletion_into_a_table_keeps_the_edges_of_fields_it_doesnt_take_whole() {
    let table = r#"<w:tbl><w:tblGrid><w:gridCol w:w="4000"/></w:tblGrid><w:tr><w:tc><w:p><w:r><w:t>r1</w:t></w:r></w:p></w:tc></w:tr><w:tr><w:tc><w:p><w:r><w:t>r2</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#;
    // The table's start in the paragraph the deletion begins in, its end after the table.
    let body = READ_TOC.replacen(r#"<w:p><w:r><w:t>Two</w:t></w:r></w:p>"#, &format!(r#"<w:p><w:r><w:t>Two</w:t></w:r></w:p>{table}"#), 1);
    for rows_left in [true, false] {
        let mut e = Editor::open(&doc_with(&body, HEADINGS), fonts()).unwrap();
        assert_eq!(e.tables_of_contents().len(), 1);
        let ps = texts_ids(&e);
        let into = if rows_left { ps[2] } else { ps[3] };
        e.select_between((ps[0], Unit(0)), (into, Unit(1)), fonts());
        let out =
            e.execute(Command::Delete(crate::api::command::DeleteOptions { direction: crate::api::value::Direction::Forward, unit: None }), fonts());
        assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
        assert_eq!(balance(&saved(&e)), (1, 1, 1), "rows left {rows_left}");
        assert_eq!(e.tables_of_contents().len(), 1, "rows left {rows_left}: {:?}", edges(&e));
    }
}

#[test]
fn a_deletion_into_a_table_leaves_a_fields_start_before_it_out_of_the_cell() {
    let table = r#"<w:tbl><w:tblGrid><w:gridCol w:w="4000"/></w:tblGrid><w:tr><w:tc><w:p><w:r><w:t>r1</w:t></w:r></w:p></w:tc></w:tr><w:tr><w:tc><w:p><w:r><w:t>r2</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#;
    let body = READ_TOC.replacen(r#"<w:p><w:r><w:t>Two</w:t></w:r></w:p>"#, &format!(r#"<w:p><w:r><w:t>Two</w:t></w:r></w:p>{table}"#), 1);
    let mut e = Editor::open(&doc_with(&body, HEADINGS), fonts()).unwrap();
    let ps = texts_ids(&e);
    e.select_between((ps[0], Unit(1)), (ps[2], Unit(1)), fonts());
    let out =
        e.execute(Command::Delete(crate::api::command::DeleteOptions { direction: crate::api::value::Direction::Forward, unit: None }), fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    assert_eq!(balance(&saved(&e)), (1, 1, 1));
    assert_eq!(e.tables_of_contents().len(), 1, "{:?}", edges(&e));
}

/// A table of contents in one paragraph whose one entry reads page `page` of bookmark `mark`.
fn inline_toc(mark: &str, page: &str) -> String {
    format!(
        concat!(
            r#"<w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> TOC \o "1-3" </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r>"#,
            r#"<w:r><w:t xml:space="preserve">{mark} </w:t></w:r><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> PAGEREF {mark} </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>{page}</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r>"#,
            r#"<w:r><w:fldChar w:fldCharType="end"/></w:r>"#
        ),
        mark = mark,
        page = page
    )
}

fn marked(style: &str, mark: &str) -> String {
    format!(
        r#"<w:p><w:pPr><w:pStyle w:val="{style}"/></w:pPr><w:bookmarkStart w:id="{mark}0" w:name="{mark}"/><w:r><w:t>{mark}</w:t></w:r><w:bookmarkEnd w:id="{mark}0"/></w:p>"#
    )
}

#[test]
fn a_page_update_fills_the_table_it_names_in_a_shared_paragraph() {
    let body =
        [format!("<w:p>{}{}</w:p>", inline_toc("A", "9"), inline_toc("B", "9")), marked("Heading1", "A"), PAGE.into(), marked("Heading1", "B")]
            .concat();
    let mut e = Editor::open(&doc_with(&body, HEADINGS), fonts()).unwrap();
    let t = e.tables_of_contents();
    assert_eq!(t.len(), 2);
    assert!(matches!(e.update_toc(&t[1], TocUpdate::Pages, fonts()), Outcome::Changed(_)));
    assert_eq!(page_results(body_paras(&e)[0]), ["9", "2"]);
}

#[test]
fn a_table_starting_after_another_ends_in_its_paragraph_is_found() {
    let start = |code: &str| {
        format!(
            r#"<w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve">{code}</w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r>"#
        )
    };
    let end = r#"<w:r><w:fldChar w:fldCharType="end"/></w:r>"#;
    let body = [
        format!(r#"<w:p>{}<w:r><w:t>One</w:t></w:r></w:p>"#, start(r#" TOC \o "1-3" "#)),
        format!(r#"<w:p>{end}{}<w:r><w:t>Two</w:t></w:r></w:p>"#, start(r#" TOC \c "Figure" "#)),
        format!(r#"<w:p>{end}</w:p>"#),
    ]
    .concat();
    let e = Editor::open(&doc_with(&body, HEADINGS), fonts()).unwrap();
    let t = e.tables_of_contents();
    assert_eq!(t.iter().map(|t| (t.first, t.last, t.code.trim())).collect::<Vec<_>>(), [(0, 1, r#"TOC \o "1-3""#), (1, 2, r#"TOC \c "Figure""#)]);
}

#[test]
fn a_handle_doesnt_name_the_table_that_takes_its_place() {
    let p = format!("<w:p>{}</w:p>", inline_toc("A", "1"));
    let body = [p.clone(), p, marked("Heading1", "A")].concat();
    let mut e = Editor::open(&doc_with(&body, HEADINGS), fonts()).unwrap();
    let old = e.tables_of_contents().remove(0);
    let ps = texts_ids(&e);
    e.select_between((ps[0], Unit(0)), (ps[1], Unit(0)), fonts());
    e.execute(super::del(Dir::Back, false), fonts());
    assert_eq!(e.tables_of_contents().len(), 1);
    assert!(!matches!(e.remove_table_of_contents(&old, fonts()), Outcome::Changed(_)));
    assert_eq!(e.tables_of_contents().len(), 1);
}

#[test]
fn a_deletion_across_sections_keeps_a_fields_end_with_its_start() {
    let sect = r#"<w:p><w:pPr><w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr></w:pPr><w:r><w:t>Plain</w:t></w:r></w:p>"#;
    let mut e = Editor::open(&doc_with(&format!("{sect}{READ_TOC}"), HEADINGS), fonts()).unwrap();
    assert_eq!(e.document().sections.len(), 2);
    let ps = texts_ids(&e);
    e.select_between((ps[0], Unit(0)), (ps[2], Unit(0)), fonts());
    assert!(matches!(e.execute(super::del(Dir::Back, false), fonts()), Outcome::Changed(_)));
    assert_eq!(texts(&e)[0], "Two");
    assert_eq!(balance(&saved(&e)), (1, 1, 1), "{:?}", edges(&e));
    assert_eq!(e.tables_of_contents().len(), 1);
}

#[test]
fn an_entire_update_keeps_a_mark_a_locked_reference_reads() {
    let toc = READ_TOC.replacen("<w:r><w:t>Two</w:t></w:r>", &format!("<w:r><w:t>Two</w:t></w:r>{}", field("PAGEREF _Toc000000005 \\h", "1")), 1);
    let locked = r#"<w:p><w:r><w:fldChar w:fldCharType="begin" w:fldLock="1"/></w:r><w:r><w:instrText xml:space="preserve"> REF _Toc000000005 </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>x</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r></w:p>"#;
    let mark = r#"<w:p><w:bookmarkStart w:id="5" w:name="_Toc000000005"/><w:r><w:t>x</w:t></w:r><w:bookmarkEnd w:id="5"/></w:p>"#;
    for lock in [false, true] {
        let body = [toc.as_str(), mark, if lock { locked } else { "" }].concat();
        let mut e = Editor::open(&doc_with(&body, HEADINGS), fonts()).unwrap();
        let t = e.tables_of_contents().remove(0);
        assert!(matches!(e.update_toc(&t, TocUpdate::Entire, fonts()), Outcome::Changed(_)));
        assert_eq!(e.bookmarks().iter().any(|b| b.name == "_Toc000000005"), lock, "locked {lock}");
    }
}

#[test]
fn a_long_style_list_reads_back_against_many_styles() {
    const N: usize = 10_000;
    let defs: String = (0..N)
        .map(|k| format!(r#"<w:style w:type="paragraph" w:customStyle="1" w:styleId="S{k}"><w:name w:val="Style {k},Alias {k}"/></w:style>"#))
        .collect();
    let list: Vec<String> = (0..N).map(|k| if k % 2 == 0 { format!("style {k},1") } else { format!("ALIAS {k},2") }).collect();
    let code = format!(r#" TOC \t "{},Missing,3" "#, list.join(","));
    let body = format!(
        r#"<w:p><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve">{code}</w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>One</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r></w:p>{}{}"#,
        para("S9998", "Even"),
        para("S0", "First"),
    ) + &para("S1", "By alias");
    let e = Editor::open(&doc_from(&body, &defs, None), fonts()).unwrap();
    let tables = e.tables_of_contents();
    assert_eq!(tables.len(), 1);
    let o = e.toc_options_api(&tables[0].options(','), &std::cell::OnceCell::new());
    assert_eq!(o.style_levels.len(), N, "the missing name left out");
    assert_eq!(o.style_levels[0].style.0, "S0");
    assert_eq!((o.style_levels[1].style.0.as_str(), o.style_levels[1].level), ("S1", 2));
    let found: Vec<(u8, String)> = e.toc_entries(&tables[0].options(',')).into_iter().map(|x| (x.level, x.text)).collect();
    assert_eq!(found, [(1, "Even".to_string()), (1, "First".to_string()), (2, "By alias".to_string())], "as the readback maps them");
}

#[test]
fn locking_the_selected_fields_locks_a_table() {
    let mut e = Editor::open(&doc_with(READ_TOC, HEADINGS), fonts()).unwrap();
    let lock = |e: &mut Editor, locked: bool| e.execute(Command::SetFieldsLocked(crate::api::command::FieldsLocked { locked, ids: None }), fonts());
    e.execute(Command::SelectAll, fonts());
    assert!(matches!(lock(&mut e, true), Outcome::Changed(_)));
    assert!(e.tables_of_contents()[0].lock);
    assert!(saved(&e).contains(r#"w:fldLock="1""#));
    assert!(!matches!(e.update_table_of_contents(TocUpdate::Entire, fonts()), Outcome::Changed(_)));
    assert!(matches!(lock(&mut e, false), Outcome::Changed(_)));
    assert!(!e.tables_of_contents()[0].lock);
    // A caret in an entry's text, no other field there: the table's.
    let first = texts_ids(&e)[0];
    e.select_range(first, Unit(1), Unit(1), fonts());
    assert!(matches!(lock(&mut e, true), Outcome::Changed(_)));
    assert!(e.tables_of_contents()[0].lock);
    e.execute(Command::Undo(Default::default()), fonts());
    assert!(!e.tables_of_contents()[0].lock);
}

fn unlink(e: &mut Editor) -> Outcome {
    e.execute(Command::UnlinkFields(crate::api::command::ElementIds { ids: None }), fonts())
}

#[test]
fn unlinking_the_selected_fields_turns_a_table_into_text() {
    let mut e = Editor::open(&sample(), fonts()).unwrap();
    at_start(&mut e);
    insert(&mut e, &TocOptions::word_default());
    let before: Vec<String> = body_paras(&e)[..4].iter().map(|p| shown(p)).collect();
    e.execute(Command::SelectAll, fonts());
    assert!(matches!(unlink(&mut e), Outcome::Changed(_)));
    assert!(e.tables_of_contents().is_empty());
    assert_eq!(balance(&saved(&e)), (0, 0, 0), "the links and page numbers too");
    assert_eq!(body_paras(&e)[..4].iter().map(|p| shown(p)).collect::<Vec<_>>(), before);
    e.execute(Command::Undo(Default::default()), fonts());
    assert_eq!(e.tables_of_contents().len(), 1);
    assert_eq!(balance(&saved(&e)).2, 1 + 4 * 2);
    // Showing its code: its result stays.
    show_codes(&mut e, true);
    e.execute(Command::SelectAll, fonts());
    assert!(matches!(unlink(&mut e), Outcome::Changed(_)));
    assert!(e.tables_of_contents().is_empty());
    assert_eq!(body_paras(&e)[..4].iter().map(|p| shown(p)).collect::<Vec<_>>(), before);
}

const BEGIN: &str = r#"<w:fldChar w:fldCharType="begin"/>"#;
const LOCKED_BEGIN: &str = r#"<w:fldChar w:fldCharType="begin" w:fldLock="1"/>"#;

#[test]
fn a_locked_field_at_the_caret_stays_linked() {
    for (begin, linked) in [(LOCKED_BEGIN, true), (BEGIN, false)] {
        let body = format!("<w:p>{}</w:p>", field("DATE", "1/1/2026").replacen(BEGIN, begin, 1));
        let mut e = Editor::open(&doc_with(&body, HEADINGS), fonts()).unwrap();
        e.select_range(texts_ids(&e)[0], Unit(0), Unit(0), fonts());
        let out = unlink(&mut e);
        assert_eq!(matches!(out, Outcome::Unchanged("the field is locked")), linked, "{out:?}");
        assert_eq!(balance(&saved(&e)), if linked { (1, 1, 1) } else { (0, 0, 0) });
    }
}

#[test]
fn a_locked_table_keeps_the_fields_inside_it() {
    let entry = field(r#"HYPERLINK \l "_Toc1""#, "Two") + &field(r#"PAGEREF _Toc1 \h"#, "1");
    let toc = READ_TOC.replacen(BEGIN, LOCKED_BEGIN, 1).replacen("<w:r><w:t>Two</w:t></w:r>", &entry, 1);
    let mut e = Editor::open(&doc_with(&toc, HEADINGS), fonts()).unwrap();
    let before = saved(&e);
    e.execute(Command::SelectAll, fonts());
    assert!(matches!(unlink(&mut e), Outcome::Unchanged("the field is locked")));
    e.select_range(texts_ids(&e)[1], Unit(1), Unit(1), fonts());
    assert!(matches!(unlink(&mut e), Outcome::Unchanged("the field is locked")));
    assert_eq!(saved(&e), before);
    assert_eq!(balance(&before), (3, 3, 3));
    assert_eq!(e.tables_of_contents().len(), 1);
    // A locked page number in an unlocked table stays a field.
    let entry = field(r#"HYPERLINK \l "_Toc1""#, "Two") + &field(r#"PAGEREF _Toc1 \h"#, "1").replacen(BEGIN, LOCKED_BEGIN, 1);
    let toc = READ_TOC.replacen("<w:r><w:t>Two</w:t></w:r>", &entry, 1);
    let mut e = Editor::open(&doc_with(&toc, HEADINGS), fonts()).unwrap();
    e.execute(Command::SelectAll, fonts());
    assert!(matches!(unlink(&mut e), Outcome::Changed(_)));
    assert!(e.tables_of_contents().is_empty());
    let after = saved(&e);
    assert_eq!(balance(&after), (1, 1, 1));
    assert!(after.contains("PAGEREF") && !after.contains("HYPERLINK"));
}

#[test]
fn unlinking_at_the_caret_takes_its_table_unless_locked() {
    let mut e = Editor::open(&doc_with(READ_TOC, HEADINGS), fonts()).unwrap();
    let first = texts_ids(&e)[0];
    e.select_range(first, Unit(1), Unit(1), fonts());
    e.execute(Command::SetFieldsLocked(crate::api::command::FieldsLocked { locked: true, ids: None }), fonts());
    assert!(!matches!(unlink(&mut e), Outcome::Changed(_)));
    assert_eq!(e.tables_of_contents().len(), 1);
    e.execute(Command::Undo(Default::default()), fonts());
    e.select_range(first, Unit(1), Unit(1), fonts());
    assert!(matches!(unlink(&mut e), Outcome::Changed(_)));
    assert!(e.tables_of_contents().is_empty());
    assert!(edges(&e).iter().all(|x| x.is_empty()));
    assert_eq!(body_paras(&e)[..2].iter().map(|p| shown(p)).collect::<Vec<_>>(), ["One", "Two"]);
    e.execute(Command::Undo(Default::default()), fonts());
    assert_eq!(e.tables_of_contents().len(), 1);
}
