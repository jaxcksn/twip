//! Superlinear inputs

use crate::docx::Document;
use crate::layout::layout;

use super::{fonts, package};

const W: &str = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main""#;
const PAGE: &str = r#"<w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/>"#;
const CT: &str = r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#;
const RELS: &str = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;

fn docx(body: &str) -> Document {
    package(&[
        ("[Content_Types].xml", CT),
        ("_rels/.rels", RELS),
        ("word/document.xml", &format!("<w:document {W}><w:body>{body}<w:sectPr>{PAGE}</w:sectPr></w:body></w:document>")),
    ])
    .expect("document")
}

fn spaces(n: usize) -> Document {
    docx(&format!(r#"<w:p><w:r><w:t xml:space="preserve">{}</w:t></w:r></w:p>"#, " ".repeat(n)))
}

fn sections(n: usize) -> Document {
    docx(
        &(0..n)
            .map(|i| format!("<w:p><w:r><w:t>Section {i}</w:t></w:r></w:p><w:p><w:pPr><w:sectPr>{PAGE}</w:sectPr></w:pPr></w:p>"))
            .collect::<String>(),
    )
}

/// Whether doing `f` at four times the size takes less than eight times as long: linear work takes about four times
/// as long, quadratic sixteen. The sizes take turns three times and the fastest of each counts; a busy machine gets
/// five tries, which a quadratic `f` fails every one of.
fn grows_linearly(name: &str, n: usize, f: &mut dyn FnMut(usize)) {
    let mut time = |n| {
        let t = std::time::Instant::now();
        f(n);
        t.elapsed()
    };
    let mut tries = Vec::new();
    for _ in 0..5 {
        let (mut small, mut large) = (std::time::Duration::MAX, std::time::Duration::MAX);
        for _ in 0..3 {
            small = small.min(time(n));
            large = large.min(time(4 * n));
        }
        let ratio = large.as_secs_f64() / small.as_secs_f64().max(1e-9);
        if ratio < 8.0 {
            return;
        }
        tries.push(format!("{ratio:.1}x ({small:?}, then {large:?})"));
    }
    panic!("{name}: 4x the size took {}", tries.join(", "));
}

#[test]
fn a_long_run_of_spaces_lays_out_in_linear_time() {
    let fonts = fonts();
    let docs: std::collections::HashMap<usize, Document> = [10_000, 40_000].into_iter().map(|n| (n, spaces(n))).collect();
    grows_linearly("spaces", 10_000, &mut |n| assert_eq!(layout(&docs[&n], &fonts).pages().len(), 1));
}

#[test]
fn many_sections_lay_out_and_relay_in_linear_time() {
    use crate::layout::ids::Revision;
    use crate::layout::{Change, DocumentLayout, Target};
    let fonts = fonts();
    let docs: std::collections::HashMap<usize, Document> = [1_000, 4_000].into_iter().map(|n| (n, sections(n))).collect();
    // Page by page, as an editor opens a document, then a paragraph edited halfway.
    grows_linearly("sections", 1_000, &mut |n| {
        let d = &docs[&n];
        let mut dl = DocumentLayout::new();
        dl.edit(Revision(1), None);
        dl.through(d, &fonts, Target::Page(usize::MAX));
        dl.edit(Revision(2), Some(Change { section: n / 2, first: 0, old_end: 1, new_end: 1, row: None }));
        dl.complete(d, &fonts);
        assert_eq!(dl.snapshot().pages().len(), n + 1);
    });
}
