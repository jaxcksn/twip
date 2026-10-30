//! Column separator lines (COL-1): a continuous two-column section's line ends at its own text, not at a footnote at
//! the page bottom whose reference is in the section above.

use super::{package, page_chars};
use crate::layout::fragment::Item;

const W: &str = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships""#;

/// A paragraph with a footnote, then two columns with a separator, then one column again.
fn columns_under_a_note() -> Option<crate::docx::Document> {
    let page = r#"<w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/>"#;
    let body = format!(
        r#"<w:document {W}><w:body><w:p><w:pPr><w:sectPr>{page}</w:sectPr></w:pPr><w:r><w:t>Intro</w:t></w:r><w:r><w:footnoteReference w:id="1"/></w:r></w:p><w:p><w:r><w:t>Left</w:t></w:r></w:p><w:p><w:pPr><w:sectPr><w:type w:val="continuous"/>{page}<w:cols w:num="2" w:sep="1" w:space="720"/></w:sectPr></w:pPr><w:r><w:br w:type="column"/><w:t>Right</w:t></w:r></w:p><w:p><w:r><w:t>After</w:t></w:r></w:p><w:sectPr><w:type w:val="continuous"/>{page}</w:sectPr></w:body></w:document>"#
    );
    let notes = format!(r#"<w:footnotes {W}><w:footnote w:id="1"><w:p><w:r><w:t>The note</w:t></w:r></w:p></w:footnote></w:footnotes>"#);
    package(&[
        (
            "[Content_Types].xml",
            r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/footnotes.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.footnotes+xml"/></Types>"#,
        ),
        (
            "_rels/.rels",
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#,
        ),
        (
            "word/_rels/document.xml.rels",
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/footnotes" Target="footnotes.xml"/></Relationships>"#,
        ),
        ("word/document.xml", &body),
        ("word/footnotes.xml", &notes),
    ])
}

#[test]
fn col_1_a_separator_ends_at_its_sections_text_not_at_a_footnote() {
    let Some(doc) = columns_under_a_note() else { return };
    let chars = &page_chars(&doc)[0];
    let base = |c: char| chars.iter().find(|x| x.0 == c).map(|x| x.2);
    let (Some(right), Some(note)) = (base('R'), base('T')) else { panic!("{chars:?}") };
    assert!(note > 600.0, "the note sits at the page bottom: {note}");
    let snap = crate::layout::layout(&doc, &super::fonts());
    let lines: Vec<[f64; 4]> = crate::layout::traverse::paint_items(&snap.pages()[0])
        .into_iter()
        .filter_map(|i| if let Item::Rect { rect, .. } = i { Some(rect) } else { None })
        .filter(|r| (r[2] - r[0] - 0.72).abs() < 0.01)
        .collect();
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0][3] < right + 12.0, "the line ends below \"Right\" ({right}), not at the note: {:?}", lines[0]);
}
