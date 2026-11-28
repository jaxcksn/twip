//! Incremental layout against a full layout

use crate::docx::{Block, Document, Inline};
use crate::layout::env::BUILT;
use crate::layout::ids::Revision;
use crate::layout::incremental::{FULL_JOBS, Target};
use crate::layout::tests::layout;
use crate::layout::{Change, DocumentLayout, dump};

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
    let mut dl = crate::layout::tests::document_layout(&d);
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
    let mut dl = crate::layout::tests::document_layout(&d);
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
    let mut dl = crate::layout::tests::document_layout(&d);
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
    p.renew_identity();
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
    let mut dl = crate::layout::tests::document_layout(&d);
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

#[test]
fn text_box_chains_an_edit_puts_in_are_laid_out() {
    use super::shapes::{anchor, sp};
    let fonts = fonts();
    let story: String =
        (1..=6).map(|i| format!("<w:p><w:r><w:t>P{i} words to fill the boxes in turn, one after another.</w:t></w:r></w:p>")).collect();
    let rect = sp(0, 0, 150, 60, "rect", "", "FFFFFF");
    let head = rect.replace("<wps:bodyPr/>", &format!(r#"<wps:txbx id="1"><w:txbxContent>{story}</w:txbxContent></wps:txbx><wps:bodyPr/>"#));
    let link = rect.replace("<wps:bodyPr/>", r#"<wps:linkedTxbx id="1" seq="1"/><wps:bodyPr/>"#);
    let boxes = format!(
        "<w:p><w:r><w:t>Text</w:t></w:r>{}{}</w:p>",
        anchor(72, 300, 150, 60, 1, false, 1, &head),
        anchor(250, 300, 150, 60, 2, false, 2, &link)
    );
    let full =
        crate::docx::read(&super::shapes::bytes(&format!("<w:p><w:r><w:t>Before</w:t></w:r></w:p>{boxes}")).expect("a package")).expect("reads");
    let mut d = full.clone();
    let kept = d.sections[0].blocks.split_off(1);
    let mut dl = crate::layout::tests::document_layout(&d);
    dl.edit(Revision(1), None);
    dl.complete(&d, &fonts);
    // A chain-free edit keeps the (empty) chains; one putting a chain in finds it.
    let Some(Block::Para(p)) = d.sections[0].blocks.first_mut() else { panic!("a paragraph") };
    p.inlines.push(Inline::Text("more".into(), p.inlines.iter().find_map(|i| i.run().cloned()).expect("a run")));
    dl.edit(Revision(2), Some(Change { section: 0, first: 0, old_end: 1, new_end: 1, row: None }));
    dl.complete(&d, &fonts);
    assert_matches_full(&dl, &d);
    d.sections[0].blocks.extend(kept);
    dl.edit(Revision(3), Some(Change { section: 0, first: 1, old_end: 1, new_end: 2, row: None }));
    dl.complete(&d, &fonts);
    assert_matches_full(&dl, &d);
    assert!(dump::page_text(&dl.snapshot(), &d).contains("P2"), "the second box shows the story");
}

#[test]
fn a_section_setting_changed_outside_steps_lays_out_again() {
    let fonts = fonts();
    let mut d = docx(&format!("<w:p><w:r><w:t>Text</w:t></w:r></w:p>{}", sect()));
    let mut dl = crate::layout::tests::document_layout(&d);
    dl.edit(Revision(1), None);
    dl.complete(&d, &fonts);
    d.sections[0].left += 72.0;
    d.sections[0].v_align = crate::docx::VerticalAlignment::Center;
    dl.edit(Revision(2), Some(Change { section: 0, first: 0, old_end: 1, new_end: 1, row: None }));
    dl.complete(&d, &fonts);
    assert_matches_full(&dl, &d);
}

#[test]
fn laying_out_to_a_block_stops_where_laying_out_to_its_paragraph_does() {
    let fonts = fonts();
    let body: String = (0..120).map(|i| format!("<w:p><w:r><w:t>Paragraph {i} {}</w:t></w:r></w:p>", super::words(40))).collect();
    let plain = docx(&format!("{body}{}", sect()));
    let table = table_doc(60, true);
    let id = |b: &Block| match b {
        Block::Para(p) => p.id,
        Block::Table(_) => panic!("a paragraph"),
    };
    let shown = |d: &Document, t: Target| {
        let mut dl = crate::layout::tests::document_layout(d);
        dl.edit(Revision(1), None);
        dl.through(d, &fonts, t);
        dl.snapshot().pages().len()
    };
    for k in [0, 7, 60, 119] {
        let para = crate::layout::ParaId(id(&plain.sections[0].blocks[k]));
        let by_para = shown(&plain, Target::Position { para, unit: crate::layout::Unit(0) });
        assert_eq!(shown(&plain, Target::Block { section: 0, block: k, row: None }), by_para, "paragraph {k}");
    }
    let Block::Table(t) = &table.sections[0].blocks[0] else { panic!("a table") };
    for row in [0, 30, 59] {
        let Block::Para(p) = &t.rows[row].cells[0].blocks[0] else { panic!("a paragraph") };
        let by_para = shown(&table, Target::Position { para: crate::layout::ParaId(p.id), unit: crate::layout::Unit(0) });
        assert_eq!(shown(&table, Target::Block { section: 0, block: 0, row: Some(row) }), by_para, "row {row}");
    }
}

/// Six pages under a header that shows its page number.
fn headed() -> Document {
    let ct = r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/header1.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml"/></Types>"#;
    let rels = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/header" Target="header1.xml"/></Relationships>"#;
    let header = r#"<w:p><w:r><w:t xml:space="preserve">Head </w:t></w:r><w:fldSimple w:instr=" PAGE "><w:r><w:t>1</w:t></w:r></w:fldSimple></w:p>"#;
    let body: String =
        (0..6).map(|i| format!(r#"<w:p><w:r><w:t>Page {i} {}</w:t></w:r><w:r><w:br w:type="page"/></w:r></w:p>"#, super::words(30))).collect();
    let sect = format!(r#"<w:sectPr><w:headerReference w:type="default" r:id="rId3"/>{PAGE}</w:sectPr>"#);
    package(&[
        ("[Content_Types].xml", ct),
        ("_rels/.rels", RELS),
        ("word/_rels/document.xml.rels", rels),
        (
            "word/document.xml",
            &format!(r#"<w:document {W} xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><w:body>{body}{sect}</w:body></w:document>"#),
        ),
        ("word/header1.xml", &format!("<w:hdr {W}>{header}</w:hdr>")),
    ])
    .expect("document")
}

fn header_blocks(d: &mut Document) -> &mut Vec<Block> {
    d.stories.values_mut().next().expect("the header")
}

#[test]
fn a_header_edit_lays_out_again_reusing_the_bodies() {
    use crate::layout::incremental::REUSED_PAGES;
    let fonts = fonts();
    let mut d = headed();
    let mut dl = crate::layout::tests::document_layout(&d);
    dl.edit(Revision(1), None);
    dl.complete(&d, &fonts);
    let pages = dl.snapshot().pages().len();
    assert!(pages >= 6, "{pages} pages");
    let Block::Para(p) = &mut header_blocks(&mut d)[0] else { panic!("a paragraph") };
    let Some(Inline::Text(s, _)) = p.inlines.iter_mut().find(|i| matches!(i, Inline::Text(..))) else { panic!("text") };
    *s = "Heading ".into();
    let (full, reused) = (FULL_JOBS.get(), REUSED_PAGES.get());
    dl.edit_parts(Revision(2));
    dl.complete(&d, &fonts);
    assert_eq!(FULL_JOBS.get(), full, "no full layout");
    assert!(REUSED_PAGES.get() - reused >= pages as u64 - 1, "every page after the first keeps its body");
    assert_matches_full(&dl, &d);
    // A taller header moves every body down.
    let more = fresh_copy(&header_blocks(&mut d)[0]);
    header_blocks(&mut d).push(more);
    dl.edit_parts(Revision(3));
    dl.complete(&d, &fonts);
    assert_matches_full(&dl, &d);
}

#[test]
fn laying_out_to_a_page_after_a_header_edit_stops_there() {
    let fonts = fonts();
    let mut d = headed();
    let mut dl = crate::layout::tests::document_layout(&d);
    dl.edit(Revision(1), None);
    dl.complete(&d, &fonts);
    let more = fresh_copy(&header_blocks(&mut d)[0]);
    header_blocks(&mut d).push(more);
    dl.edit_parts(Revision(2));
    dl.through(&d, &fonts, Target::Page(1));
    assert!(
        matches!(dl.snapshot().status(), crate::layout::LayoutStatus::Pending { stale_from } if *stale_from <= 3),
        "{:?}",
        dl.snapshot().status()
    );
    dl.complete(&d, &fonts);
    assert_matches_full(&dl, &d);
}

/// Sections going on on a page and starting on an odd one (a blank page before), a bookmark, and a header of
/// `PAGEREF`, `NUMPAGES` and `SECTIONPAGES`, laid out a page a run: each run publishes its pages as the tests' checks
/// in composing and publishing find them, and the last a full layout's.
#[test]
fn a_layout_published_page_by_page_ends_as_a_full_one() {
    use crate::layout::{Budget, Step};
    let fonts = fonts();
    let field = |code: &str| {
        format!(
            r#"<w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> {code} </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>0</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r>"#
        )
    };
    let header = format!(r#"<w:hdr {W}><w:p>{}{}{}</w:p></w:hdr>"#, field("PAGEREF mark"), field("NUMPAGES"), field("SECTIONPAGES"));
    let r = r#" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships""#;
    let hdr = r#"<w:headerReference w:type="default" r:id="rId2"/>"#;
    let para = |t: &str| format!("<w:p><w:r><w:t>{t}</w:t></w:r></w:p>");
    let brk = r#"<w:p><w:r><w:br w:type="page"/></w:r></w:p>"#;
    let sect = |kind: &str| format!(r#"<w:p><w:pPr><w:sectPr>{hdr}<w:type w:val="{kind}"/>{PAGE}</w:sectPr></w:pPr></w:p>"#);
    let mut body = String::new();
    for s in 0..6 {
        for k in 0..3 {
            body += &para(&format!("S{s} P{k}"));
            if s == 2 && k == 1 {
                body += r#"<w:p><w:bookmarkStart w:id="0" w:name="mark"/><w:r><w:t>Mark</w:t></w:r><w:bookmarkEnd w:id="0"/></w:p>"#;
            }
            body += brk;
        }
        body += &sect(["continuous", "oddPage", "nextPage"][s % 3]);
    }
    let ct = CT.replace(
        "</Types>",
        r#"<Override PartName="/word/header1.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml"/></Types>"#,
    );
    let rels = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/header" Target="header1.xml"/></Relationships>"#;
    let d = package(&[
        ("[Content_Types].xml", &ct),
        ("_rels/.rels", RELS),
        ("word/_rels/document.xml.rels", rels),
        ("word/document.xml", &format!("<w:document {W}{r}><w:body>{body}<w:sectPr>{hdr}{PAGE}</w:sectPr></w:body></w:document>")),
        ("word/header1.xml", &header),
    ])
    .expect("document");
    let mut dl = crate::layout::tests::document_layout(&d);
    dl.edit(Revision(1), None);
    let mut runs = 0;
    while dl.run(&d, &fonts, Budget::Pages(1)) != Step::Done {
        runs += 1;
    }
    let full = layout(&d, &fonts);
    assert!(runs + 1 >= full.pages().len() && full.info().iter().any(|i| i.blank), "a run a page, a blank page among them: {runs} runs");
    assert_matches_full(&dl, &d);
}

/// A table of `n` rows of one to three lines over two columns, then text: row 0 a repeated header, the first column
/// merged down rows 20 to 40 (its first cell taller than they are) and 62 to 70, rows 44 to 49 kept with the next.
fn long_table_doc(n: usize) -> Document {
    let rows: String = (0..n)
        .map(|i| {
            let h = if i == 0 { "<w:trPr><w:tblHeader/></w:trPr>" } else { "" };
            let merge = match i {
                20 | 62 => r#"<w:vMerge w:val="restart"/>"#,
                21..=40 | 63..=70 => "<w:vMerge/>",
                _ => "",
            };
            let keep = if (44..50).contains(&i) { "<w:pPr><w:keepNext/></w:pPr>" } else { "" };
            let text = format!("Row {i:03} ").repeat(1 + i % 3 * 6);
            let first = if i == 20 { text.repeat(60) } else { text.clone() };
            format!(
                r#"<w:tr>{h}<w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/>{merge}</w:tcPr><w:p>{keep}<w:r><w:t>{first}</w:t></w:r></w:p></w:tc><w:tc><w:tcPr><w:tcW w:w="6000" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>{text}</w:t></w:r></w:p></w:tc></w:tr>"#
            )
        })
        .collect();
    let borders = r#"<w:tblBorders><w:top w:val="single" w:sz="4"/><w:bottom w:val="single" w:sz="12"/><w:insideH w:val="single" w:sz="4"/><w:insideV w:val="single" w:sz="4"/></w:tblBorders>"#;
    let sect = sect();
    docx(&format!(
        r#"<w:tbl><w:tblPr><w:tblW w:w="0" w:type="auto"/>{borders}</w:tblPr><w:tblGrid><w:gridCol w:w="3000"/><w:gridCol w:w="6000"/></w:tblGrid>{rows}</w:tbl><w:p><w:r><w:t>After</w:t></w:r></w:p>{sect}"#
    ))
}

/// What each page draws.
fn drawn(dl: &DocumentLayout) -> Vec<String> {
    dl.snapshot().pages().iter().map(|p| format!("{:?}", crate::layout::traverse::paint_items(p))).collect()
}

#[test]
fn a_live_drags_table_laid_out_in_parts_draws_as_laid_out_whole() {
    let fonts = fonts();
    for n in [40, 75, 150] {
        let d = long_table_doc(n);
        let Block::Table(t) = &d.sections[0].blocks[0] else { panic!("table first") };
        let mut whole = crate::layout::tests::document_layout(&d);
        whole.edit(Revision(1), None);
        whole.complete(&d, &fonts);
        let mut live = crate::layout::tests::document_layout(&d);
        live.set_live_tables(std::rc::Rc::from([t.id]));
        live.edit(Revision(1), None);
        live.complete(&d, &fonts);
        assert!(whole.snapshot().pages().len() >= n / 25, "{n} rows span pages");
        assert_eq!(drawn(&live), drawn(&whole), "{n} rows");
    }
}
