//! A frame in a footer: the page number gallery's framed PAGE field shows each page's number, and a frame without a
//! width is as wide as that number (FRAME-1, FIELD-1).

use super::{package, page_chars};

const W: &str = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships""#;

/// Two pages numbered from 9, the footer's number in a frame with no width (its saved result "1").
fn numbered() -> Option<crate::docx::Document> {
    let body = format!(
        r#"<w:document {W}><w:body><w:p><w:r><w:t>One</w:t></w:r></w:p><w:p><w:r><w:br w:type="page"/><w:t>Two</w:t></w:r></w:p><w:sectPr><w:footerReference w:type="default" r:id="rId2"/><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/><w:pgNumType w:start="9"/></w:sectPr></w:body></w:document>"#
    );
    let footer = format!(
        r#"<w:ftr {W}><w:p><w:pPr><w:framePr w:wrap="none" w:vAnchor="text" w:hAnchor="page" w:x="774" w:y="-500"/></w:pPr><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> PAGE </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>1</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r></w:p><w:p/></w:ftr>"#
    );
    package(&[
        (
            "[Content_Types].xml",
            r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/footer1.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml"/></Types>"#,
        ),
        (
            "_rels/.rels",
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#,
        ),
        (
            "word/_rels/document.xml.rels",
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/footer" Target="footer1.xml"/></Relationships>"#,
        ),
        ("word/document.xml", &body),
        ("word/footer1.xml", &footer),
    ])
}

#[test]
fn frame_1_a_framed_page_field_in_a_footer_shows_the_page_number_on_one_line() {
    let Some(doc) = numbered() else { return };
    let pages = page_chars(&doc);
    // The footer is below the body's last line (y > 700).
    let footer = |k: usize| pages[k].iter().filter(|c| c.2 > 700.0).copied().collect::<Vec<_>>();
    let text = |k: usize| footer(k).iter().map(|c| c.0).collect::<String>();
    assert_eq!((text(0), text(1)), ("9".to_string(), "10".to_string()));
    let ten = footer(1);
    assert!(ten[0].2 == ten[1].2 && ten[1].1 > ten[0].1, "{ten:?}");
}
