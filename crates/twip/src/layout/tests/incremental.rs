//! Incremental layout against a full layout

use crate::docx::{Block, Document, Inline};
use crate::layout::env::BUILT;
use crate::layout::ids::Revision;
use crate::layout::incremental::{FULL_JOBS, Target};
use crate::layout::{Change, DocumentLayout, dump, layout};

use super::{fonts, package, package_bytes};

const W: &str = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main""#;
const PAGE: &str = r#"<w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/>"#;
const CT: &str = r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#;
const RELS: &str = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;

fn sect() -> String {
    format!("<w:sectPr>{PAGE}</w:sectPr>")
}

fn docx(body: &str) -> Document {
    package(&[
        ("[Content_Types].xml", CT),
        ("_rels/.rels", RELS),
        ("word/document.xml", &format!("<w:document {W}><w:body>{body}</w:body></w:document>")),
    ])
    .expect("document")
}

fn assert_matches_full(dl: &DocumentLayout, d: &Document) {
    let fonts = fonts();
    let (inc, full) = (dump::page_text(&dl.snapshot(), d), dump::page_text(&layout(d, &fonts), d));
    assert_eq!(inc, full, "incremental layout equals a full layout");
}

/// A table of `n` 50pt rows, row 0 a repeated header when asked.
fn table_doc(n: usize, header: bool) -> Document {
    let rows: String = (0..n)
        .map(|i| {
            let h = if i == 0 && header { "<w:tblHeader/>" } else { "" };
            format!(r#"<w:tr><w:trPr><w:trHeight w:val="1000" w:hRule="exact"/>{h}</w:trPr><w:tc><w:p><w:r><w:t>Row {i:03}</w:t></w:r></w:p></w:tc></w:tr>"#)
        })
        .collect();
    let sect = sect();
    docx(&format!(
        r#"<w:tbl><w:tblPr><w:tblW w:w="5000" w:type="pct"/></w:tblPr><w:tblGrid><w:gridCol w:w="9000"/></w:tblGrid>{rows}</w:tbl><w:p/>{sect}"#
    ))
}

fn edit_row(d: &mut Document, r: usize) {
    let Block::Table(t) = &mut d.sections[0].blocks[0] else { panic!("table first") };
    let Block::Para(p) = &mut t.rows[r].cells[0].blocks[0] else { panic!("para") };
    let Some(Inline::Text(s, _)) = p.inlines.iter_mut().find(|i| matches!(i, Inline::Text(..))) else { panic!("text") };
    *s = s.replacen("Row", "Xyz", 1);
}

fn two_row_edits_match_a_full_layout(a: usize, b: usize, header: bool) {
    let fonts = fonts();
    let mut d = table_doc(60, header);
    let mut dl = DocumentLayout::new();
    dl.edit(Revision(1), None);
    dl.complete(&d, &fonts);
    assert!(dl.snapshot().pages().len() >= 3, "the table spans 3 pages or more");
    edit_row(&mut d, a);
    edit_row(&mut d, b);
    dl.edit(Revision(2), Some(Change { section: 0, first: 0, old_end: 1, new_end: 1, row: Some(a) }));
    dl.edit(Revision(3), Some(Change { section: 0, first: 0, old_end: 1, new_end: 1, row: Some(b) }));
    dl.complete(&d, &fonts);
    assert_matches_full(&dl, &d);
}

#[test]
fn edits_to_two_rows_of_a_long_table_relay_both_rows_pages() {
    two_row_edits_match_a_full_layout(2, 55, false);
}

#[test]
fn edits_to_a_header_row_and_a_later_row_relay_every_repeated_header() {
    two_row_edits_match_a_full_layout(0, 55, true);
}

#[test]
fn moving_endnotes_to_section_ends_relays_them() {
    let fonts = fonts();
    let s = sect();
    let body = format!(
        r#"<w:p><w:r><w:t>One</w:t></w:r><w:r><w:endnoteReference w:id="1"/></w:r></w:p><w:p><w:pPr>{s}</w:pPr></w:p><w:p><w:r><w:t>Two</w:t></w:r></w:p>{s}"#
    );
    let ct = CT.replace(
        "</Types>",
        r#"<Override PartName="/word/endnotes.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.endnotes+xml"/></Types>"#,
    );
    let rels = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/endnotes" Target="endnotes.xml"/></Relationships>"#;
    let mut d = package(&[
        ("[Content_Types].xml", &ct),
        ("_rels/.rels", RELS),
        ("word/_rels/document.xml.rels", rels),
        ("word/document.xml", &format!("<w:document {W}><w:body>{body}</w:body></w:document>")),
        (
            "word/endnotes.xml",
            &format!(r#"<w:endnotes {W}><w:endnote w:id="1"><w:p><w:r><w:t>EndnoteText</w:t></w:r></w:p></w:endnote></w:endnotes>"#),
        ),
    ])
    .expect("document");
    assert!(!d.settings.endnotes_sect_end && d.sections.len() == 2);
    let mut dl = DocumentLayout::new();
    dl.edit(Revision(1), None);
    dl.complete(&d, &fonts);
    let before = dump::page_text(&dl.snapshot(), &d);
    d.settings.endnotes_sect_end = true;
    dl.edit(Revision(2), None);
    dl.complete(&d, &fonts);
    assert_ne!(dump::page_text(&layout(&d, &fonts), &d), before, "the setting moves the endnote");
    assert_matches_full(&dl, &d);
}

#[test]
fn laying_out_to_a_late_page_builds_the_environment_a_bounded_number_of_times() {
    let fonts = fonts();
    let n = 200;
    let body: String = (0..n).map(|i| format!(r#"<w:p><w:pPr><w:pageBreakBefore/></w:pPr><w:r><w:t>P{i}</w:t></w:r></w:p>"#)).collect();
    let d = docx(&format!("{body}{}", sect()));
    let mut dl = DocumentLayout::new();
    dl.edit(Revision(1), None);
    let before = BUILT.get();
    dl.through(&d, &fonts, Target::Page(n - 1));
    assert!(BUILT.get() - before < 8, "built {} times for {n} pages", BUILT.get() - before);
}

/// Footnotes restarting each page, so the notes on later pages are renumbered after the first pass.
fn restarting_notes(body: &str) -> Document {
    let foot: String = (1..=3).map(|i| format!(r#"<w:footnote w:id="{i}"><w:p><w:r><w:t>Note {i}</w:t></w:r></w:p></w:footnote>"#)).collect();
    let ct = CT.replace(
        "</Types>",
        r#"<Override PartName="/word/footnotes.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.footnotes+xml"/></Types>"#,
    );
    let rels = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/footnotes" Target="footnotes.xml"/></Relationships>"#;
    let sect = format!(r#"<w:sectPr><w:footnotePr><w:numRestart w:val="eachPage"/></w:footnotePr>{PAGE}</w:sectPr>"#);
    let bytes = package_bytes(&[
        ("[Content_Types].xml", &ct),
        ("_rels/.rels", RELS),
        ("word/_rels/document.xml.rels", rels),
        ("word/document.xml", &format!("<w:document {W}><w:body>{body}{sect}</w:body></w:document>")),
        ("word/footnotes.xml", &format!("<w:footnotes {W}>{foot}</w:footnotes>")),
    ])
    .expect("package");
    crate::docx::read(&bytes).expect("reads")
}

fn noted(text: &str, notes: &[u32]) -> String {
    let refs: String = notes.iter().map(|i| format!(r#"<w:r><w:footnoteReference w:id="{i}"/></w:r>"#)).collect();
    format!(r#"<w:p><w:r><w:t>{text}</w:t></w:r>{refs}</w:p>"#)
}

fn fresh_copy(block: &Block) -> Block {
    let Block::Para(p) = block else { unreachable!("a paragraph") };
    let mut p = p.clone();
    p.src = None;
    p.id = crate::docx::model::next_para_id();
    Block::Para(p)
}

#[test]
fn a_body_page_count_added_incrementally_stays_current_on_later_edits() {
    const BRK: &str = r#"<w:p><w:r><w:br w:type="page"/></w:r></w:p>"#;
    const NUMPAGES: &str = r#"<w:p><w:r><w:t xml:space="preserve">Total </w:t></w:r><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> NUMPAGES </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>1</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r></w:p>"#;
    let base = noted("a", &[1, 2]) + BRK + &noted("b", &[3]);
    let mut d = restarting_notes(&base);
    let numpages = fresh_copy(restarting_notes(&format!("{base}{NUMPAGES}")).sections[0].blocks.last().expect("the field"));
    let brk = fresh_copy(&restarting_notes(BRK).sections[0].blocks[0]);
    let fonts = fonts();
    let mut dl = DocumentLayout::new();
    dl.edit(Revision(1), None);
    dl.complete(&d, &fonts);
    let n = d.sections[0].blocks.len();
    d.sections[0].blocks.push(numpages);
    let full = FULL_JOBS.get();
    dl.edit(Revision(2), Some(Change { section: 0, first: n, old_end: n, new_end: n + 1, row: None }));
    dl.complete(&d, &fonts);
    assert_eq!(FULL_JOBS.get(), full, "the field's insertion stayed incremental");
    for k in 0..3u64 {
        d.sections[0].blocks.insert(0, fresh_copy(&brk));
        dl.edit(Revision(3 + k), Some(Change { section: 0, first: 0, old_end: 0, new_end: 1, row: None }));
        dl.complete(&d, &fonts);
        assert_matches_full(&dl, &d);
    }
}
