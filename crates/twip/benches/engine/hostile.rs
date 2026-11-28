//! Documents built to be slow: inputs whose layout once grew faster than their size, each big enough that it would
//! show again.

use std::cell::RefCell;

use divan::Bencher;
use twip::internal::docx::review::ReviewView;
use twip::internal::docx::{Block, Document, Inline};
use twip::internal::layout::ids::Revision;
use twip::internal::layout::{Budget, Change, DocumentLayout, Step, Target, layout};

const W: &str = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships""#;
const PAGE: &str = r#"<w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/>"#;
const BREAK: &str = r#"<w:p><w:r><w:br w:type="page"/></w:r></w:p>"#;
const SECTIONS: usize = 10_000;

/// A document of `body`, its last section with `sect` properties, and `parts`: each part's name (in `word/`), its
/// relationship and content types' last words, and its XML. Part `k` is relationship `rId{k + 2}`.
fn docx(body: &str, sect: &str, parts: &[(&str, &str, &str, String)]) -> Document {
    use std::io::Write;
    let ns = "http://schemas.openxmlformats.org";
    let mut ct = format!(
        r#"<Types xmlns="{ns}/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>"#
    );
    let mut rels = format!(r#"<Relationships xmlns="{ns}/package/2006/relationships">"#);
    for (k, (name, rel, kind, _)) in parts.iter().enumerate() {
        ct += &format!(
            r#"<Override PartName="/word/{name}" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.{kind}+xml"/>"#
        );
        rels += &format!(r#"<Relationship Id="rId{}" Type="{ns}/officeDocument/2006/relationships/{rel}" Target="{name}"/>"#, k + 2);
    }
    let root = format!(
        r#"<Relationships xmlns="{ns}/package/2006/relationships"><Relationship Id="rId1" Type="{ns}/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#
    );
    let document = format!("<w:document {W}><w:body>{body}<w:sectPr>{sect}{PAGE}</w:sectPr></w:body></w:document>");
    let mut all = vec![
        ("[Content_Types].xml".to_string(), ct + "</Types>"),
        ("_rels/.rels".into(), root),
        ("word/_rels/document.xml.rels".into(), rels + "</Relationships>"),
        ("word/document.xml".into(), document),
    ];
    all.extend(parts.iter().map(|p| (format!("word/{}", p.0), p.3.clone())));
    let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, xml) in &all {
        z.start_file(name.as_str(), zip::write::SimpleFileOptions::default()).expect("a part");
        z.write_all(xml.as_bytes()).expect("written");
    }
    twip::internal::docx::read(&z.finish().expect("a package").into_inner()).expect("the document reads")
}

/// `n` pages, page `k` with `on(k)`'s paragraphs.
fn pages(n: usize, on: impl Fn(usize) -> String) -> String {
    (0..n).map(|k| on(k) + if k + 1 < n { BREAK } else { "" }).collect()
}

/// `n` sections of a paragraph each, a page each.
fn sections(n: usize) -> Document {
    let body: String =
        (0..n).map(|i| format!("<w:p><w:r><w:t>Section {i}</w:t></w:r></w:p><w:p><w:pPr><w:sectPr>{PAGE}</w:sectPr></w:pPr></w:p>")).collect();
    docx(&body, "", &[])
}

/// One paragraph of 40 000 spaces, all hanging at the end of its one line.
#[divan::bench(sample_count = 10)]
fn spaces(bencher: Bencher) {
    let (fonts, doc) = (crate::fonts(), docx(&format!(r#"<w:p><w:r><w:t xml:space="preserve">{}</w:t></w:r></w:p>"#, " ".repeat(40_000)), "", &[]));
    let view = ReviewView::opening(&doc.settings);
    bencher.bench_local(|| layout(&doc, &view, &fonts));
}

/// 10 000 sections: laid out whole; page by page, as an editor opens a document; and a paragraph halfway edited,
/// then laid out until it settles (untimed: the layout before it).
#[divan::bench(args = ["full", "through", "edit"], sample_count = 10, sample_size = 1)]
fn many_sections(bencher: Bencher, how: &str) {
    let (fonts, doc) = (crate::fonts(), RefCell::new(sections(SECTIONS)));
    let view = ReviewView::opening(&doc.borrow().settings);
    match how {
        "full" => bencher.bench_local(|| layout(&doc.borrow(), &view, &fonts)),
        "through" => bencher.bench_local(|| {
            let mut dl = DocumentLayout::new(view.clone());
            dl.edit(Revision(1), None);
            dl.through(&doc.borrow(), &fonts, Target::Page(usize::MAX));
            dl
        }),
        _ => {
            let mut dl = DocumentLayout::new(view.clone());
            let mut rev = 1;
            dl.edit(Revision(rev), None);
            dl.complete(&doc.borrow(), &fonts);
            let (dl, section) = (RefCell::new(dl), SECTIONS / 2);
            bencher
                .with_inputs(|| {
                    let mut d = doc.borrow_mut();
                    if let Some(Block::Para(p)) = d.sections[section].blocks.first_mut()
                        && let Some(Inline::Text(t, _)) = p.inlines.iter_mut().find(|i| matches!(i, Inline::Text(..)))
                    {
                        t.push('x');
                    }
                    rev += 1;
                    dl.borrow_mut().edit(Revision(rev), Some(Change { section, first: 0, old_end: 1, new_end: 1, row: None }));
                })
                .bench_local_values(|()| dl.borrow_mut().complete(&doc.borrow(), &fonts));
        }
    }
}

/// 4 000 pages numbered by chapter, one chapter heading halfway, under a header showing it by `STYLEREF` with the
/// page number: each page's header looks for the heading.
#[divan::bench(sample_count = 10)]
fn chapter_headers(bencher: Bencher) {
    let styles = format!(
        r#"<w:styles {W}><w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style><w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr></w:pPr></w:style></w:styles>"#
    );
    let numbering = format!(
        r#"<w:numbering {W}><w:abstractNum w:abstractNumId="0"><w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="upperRoman"/><w:lvlText w:val="Chapter %1"/></w:lvl></w:abstractNum><w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num></w:numbering>"#
    );
    let field = |code: &str| {
        format!(
            r#"<w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> {code} </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>1</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r>"#
        )
    };
    let header = format!(r#"<w:hdr {W}><w:p>{}{}</w:p></w:hdr>"#, field(r#"STYLEREF "heading 1""#), field("PAGE"));
    let n = 4_000;
    let body = pages(n, |k| match k == n / 2 {
        true => r#"<w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>Middle</w:t></w:r></w:p>"#.into(),
        false => format!("<w:p><w:r><w:t>Page {k}</w:t></w:r></w:p>"),
    });
    let doc = docx(
        &body,
        r#"<w:headerReference w:type="default" r:id="rId4"/><w:pgNumType w:chapStyle="1"/>"#,
        &[
            ("styles.xml", "styles", "styles", styles),
            ("numbering.xml", "numbering", "numbering", numbering),
            ("header1.xml", "header", "header", header),
        ],
    );
    let fonts = crate::fonts();
    let view = ReviewView::opening(&doc.settings);
    bencher.bench_local(|| layout(&doc, &view, &fonts));
}

/// 8 000 pages with a footnote each, numbered from each page, in compat 12 and 15.
#[divan::bench(args = [12, 15], sample_count = 10)]
fn page_numbered_notes(bencher: Bencher, compat: u32) {
    let n = 8_000;
    let notes: String = (1..=n).map(|i| format!(r#"<w:footnote w:id="{i}"><w:p><w:r><w:t>Note {i}</w:t></w:r></w:p></w:footnote>"#)).collect();
    let mode = format!(
        r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="{compat}"/></w:compat>"#
    );
    let body = pages(n, |k| format!(r#"<w:p><w:r><w:t>Page {k}</w:t></w:r><w:r><w:footnoteReference w:id="{}"/></w:r></w:p>"#, k + 1));
    let doc = docx(
        &body,
        r#"<w:footnotePr><w:numRestart w:val="eachPage"/></w:footnotePr>"#,
        &[
            ("footnotes.xml", "footnotes", "footnotes", format!("<w:footnotes {W}>{notes}</w:footnotes>")),
            ("settings.xml", "settings", "settings", format!("<w:settings {W}>{mode}</w:settings>")),
        ],
    );
    let fonts = crate::fonts();
    let view = ReviewView::opening(&doc.settings);
    bencher.bench_local(|| layout(&doc, &view, &fonts));
}

/// 8 000 pages laid out a page a run, each run composing and publishing what it laid out, as an editor's idle time
/// does.
#[divan::bench(sample_count = 10)]
fn idle_composing(bencher: Bencher) {
    let (fonts, doc) = (crate::fonts(), docx(&pages(8_000, |k| format!("<w:p><w:r><w:t>Page {k}</w:t></w:r></w:p>")), "", &[]));
    bencher.bench_local(|| {
        let mut dl = DocumentLayout::new(ReviewView::opening(&doc.settings));
        dl.edit(Revision(1), None);
        while dl.run(&doc, &fonts, Budget::Pages(1)) != Step::Done {}
        dl
    });
}
