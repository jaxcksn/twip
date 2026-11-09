//! Vertical text

use super::{fonts, package};
use crate::layout::fragment::Item;

const W: &str = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships""#;

fn doc(body: &str) -> Option<crate::docx::Document> {
    let document = format!(
        r#"<w:document {W}><w:body>{body}<w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr></w:body></w:document>"#
    );
    package(&[
        (
            "[Content_Types].xml",
            r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#,
        ),
        (
            "_rels/.rels",
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#,
        ),
        ("word/document.xml", &document),
    ])
}

/// The page's turned runs: text, origin and direction.
fn turned_runs(doc: &crate::docx::Document) -> Vec<(String, [f64; 2], [f64; 2])> {
    let snap = crate::layout::layout(doc, &fonts());
    let mut out = Vec::new();
    for it in crate::layout::traverse::paint_items(&snap.pages()[0]) {
        if let Item::Turned(t) = it {
            for (r, at, dir) in crate::layout::vertical::runs(&t) {
                out.push((r.chars.iter().map(|c| c.0).collect::<String>().trim().to_string(), at, dir));
            }
        }
    }
    out.retain(|r| !r.0.is_empty());
    out
}

#[test]
fn a_frames_own_text_direction_turns_its_text_and_a_paragraphs_elsewhere_does_not() {
    let frame = r#"<w:framePr w:w="1500" w:h="2500" w:hRule="exact" w:wrap="around" w:vAnchor="page" w:hAnchor="page" w:x="1500" w:y="3000"/>"#;
    let body = format!(
        r#"<w:p><w:r><w:t>body</w:t></w:r></w:p><w:p><w:pPr>{frame}<w:textDirection w:val="btLr"/></w:pPr><w:r><w:t>framed words</w:t></w:r></w:p><w:p><w:pPr><w:textDirection w:val="tbRl"/></w:pPr><w:r><w:t>plain</w:t></w:r></w:p>"#
    );
    let Some(doc) = doc(&body) else { return };
    let runs = turned_runs(&doc);
    assert_eq!(runs.len(), 1, "{runs:?}");
    assert_eq!((runs[0].0.as_str(), runs[0].2), ("framed words", [0.0, -1.0]));
    // From the frame's bottom (3000 + 2500 twips) up.
    assert!((runs[0].1[1] - 275.0).abs() < 0.3, "{runs:?}");
}
