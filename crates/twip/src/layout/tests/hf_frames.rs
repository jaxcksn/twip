//! Footer frames

use super::{package, page_chars};

const W: &str = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships""#;

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
    let footer = |k: usize| pages[k].iter().filter(|c| c.2 > 700.0).copied().collect::<Vec<_>>();
    let text = |k: usize| footer(k).iter().map(|c| c.0).collect::<String>();
    assert_eq!((text(0), text(1)), ("9".to_string(), "10".to_string()));
    let ten = footer(1);
    assert!(ten[0].2 == ten[1].2 && ten[1].1 > ten[0].1, "{ten:?}");
}

#[test]
fn frame_2_a_frame_without_v_anchor_is_relative_to_the_margin() {
    use crate::docx::model::{Block, Inline};
    let frame = |v: &str| {
        format!(
            r#"<w:p><w:pPr><w:framePr w:w="2000" w:wrap="around" {v} w:hAnchor="page" w:x="1000" w:y="500"/></w:pPr><w:r><w:t>F</w:t></w:r></w:p>"#
        )
    };
    let body = format!(
        r#"<w:document {W}><w:body>{}{}<w:p><w:pPr><w:framePr w:dropCap="drop" w:lines="2" w:wrap="around" w:hAnchor="text"/></w:pPr><w:r><w:t>D</w:t></w:r></w:p><w:p><w:r><w:t>Body</w:t></w:r></w:p></w:body></w:document>"#,
        frame(""),
        frame(r#"w:vAnchor="text""#)
    );
    let Some(doc) = package(&[
        (
            "[Content_Types].xml",
            r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#,
        ),
        (
            "_rels/.rels",
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#,
        ),
        ("word/document.xml", &body),
    ]) else {
        return;
    };
    let v: Vec<&str> = doc.sections[0]
        .blocks
        .iter()
        .filter_map(|b| if let Block::Para(p) = b { Some(p) } else { None })
        .flat_map(|p| p.inlines.iter())
        .filter_map(|i| if let Inline::Float(f, _) = i { Some(f.v_relative.as_str()) } else { None })
        .collect();
    assert_eq!(v, ["margin", "paragraph", "paragraph"]);
}

fn header_float(wrap: &str) -> Option<crate::docx::Document> {
    let words = super::words(600);
    let body = format!(
        r#"<w:document {W}><w:body><w:p><w:r><w:t>{words}</w:t></w:r></w:p><w:sectPr><w:headerReference w:type="default" r:id="rId2"/><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr></w:body></w:document>"#
    );
    let header = format!(
        r#"<w:hdr {W} xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture"><w:p><w:r><w:drawing><wp:anchor distT="0" distB="0" distL="0" distR="0" simplePos="0" relativeHeight="1" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="page"><wp:posOffset>914400</wp:posOffset></wp:positionH><wp:positionV relativeFrom="page"><wp:posOffset>3810000</wp:posOffset></wp:positionV><wp:extent cx="2540000" cy="1270000"/>{wrap}<wp:docPr id="1" name="H"/><a:graphic><a:graphicData><pic:pic><pic:blipFill/></pic:pic></a:graphicData></a:graphic></wp:anchor></w:drawing><w:t>Head</w:t></w:r></w:p></w:hdr>"#
    );
    package(&[
        (
            "[Content_Types].xml",
            r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/header1.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml"/></Types>"#,
        ),
        (
            "_rels/.rels",
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#,
        ),
        (
            "word/_rels/document.xml.rels",
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/header" Target="header1.xml"/></Relationships>"#,
        ),
        ("word/document.xml", &body),
        ("word/header1.xml", &header),
    ])
}

#[test]
fn a_header_float_wraps_the_body_text_of_its_page() {
    let beside = |wrap: &str| {
        let doc = header_float(wrap)?;
        let pages = page_chars(&doc);
        let left = |k: usize| pages[k].iter().filter(|c| c.2 > 305.0 && c.2 < 395.0).map(|c| c.1).fold(f64::INFINITY, f64::min);
        Some((left(0), left(1)))
    };
    let Some((first, second)) = beside(r#"<wp:wrapSquare wrapText="bothSides"/>"#) else { return };
    assert!(first >= 272.0 && second >= 272.0, "beside the float on both pages: {first} {second}");
    let (none, _) = beside("<wp:wrapNone/>").expect("lays out");
    assert!(none < 80.0, "wrapNone doesn't wrap: {none}");
}
