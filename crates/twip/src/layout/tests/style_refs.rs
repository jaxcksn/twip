//! STYLEREF in headers

use super::{package, page_chars};

const W: &str = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships""#;

/// Three pages of headings, a character style and a late style under a header of `STYLEREF`s.
fn doc() -> Option<crate::docx::Document> {
    let para = |style: &str, runs: &str| format!(r#"<w:p><w:pPr><w:pStyle w:val="{style}"/></w:pPr>{runs}</w:p>"#);
    let run = |style: &str, t: &str| format!(r#"<w:r><w:rPr><w:rStyle w:val="{style}"/></w:rPr><w:t xml:space="preserve">{t}</w:t></w:r>"#);
    let brk = r#"<w:p><w:r><w:br w:type="page"/></w:r></w:p>"#;
    let body = [
        para("Heading1", &run("", "Alpha")),
        para("Heading1", &run("", "Beta")),
        para("Normal", &(run("", "x ") + &run("Key", "kfirst") + &run("", " y ") + &run("Key", "klast"))),
        brk.into(),
        para("Normal", &run("", "page two")),
        brk.into(),
        para("Late", &run("", "Late three")),
        para("Heading1", r#"<w:r><w:tab/><w:t xml:space="preserve">  Gamma  </w:t></w:r>"#),
    ]
    .concat();
    let field = |mark: &str, code: &str| {
        format!(
            r#"<w:p><w:r><w:t xml:space="preserve">{mark}:</w:t></w:r><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> STYLEREF {code} </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>cached</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r></w:p>"#
        )
    };
    let header = [
        field("A", r#""heading 1""#),
        field("L", r#""heading 1" \l"#),
        field("D", "1"),
        field("F", r#""Late Style""#),
        field("E", r#""Never Used""#),
        field("U", r#""No Such""#),
        field("K", r#""Key Word""#),
        field("KL", r#""Key Word" \l"#),
    ]
    .concat();
    let style = |kind: &str, id: &str, name: &str| format!(r#"<w:style w:type="{kind}" w:styleId="{id}"><w:name w:val="{name}"/></w:style>"#);
    let styles = format!(
        r#"<w:styles {W}>{}{}{}{}{}</w:styles>"#,
        style("paragraph", "Normal", "Normal"),
        style("paragraph", "Heading1", "heading 1"),
        style("paragraph", "Late", "Late Style"),
        style("paragraph", "Never", "Never Used"),
        style("character", "Key", "Key Word"),
    );
    let ct = r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/><Override PartName="/word/header1.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml"/></Types>"#;
    let rels = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/><Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/header" Target="header1.xml"/></Relationships>"#;
    let sect = r#"<w:sectPr><w:headerReference w:type="default" r:id="rId3"/><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="4320" w:right="1440" w:bottom="1440" w:left="1440" w:header="360" w:footer="720" w:gutter="0"/></w:sectPr>"#;
    package(&[
        ("[Content_Types].xml", ct),
        (
            "_rels/.rels",
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#,
        ),
        ("word/_rels/document.xml.rels", rels),
        ("word/document.xml", &format!("<w:document {W}><w:body>{body}{sect}</w:body></w:document>")),
        ("word/styles.xml", &styles),
        ("word/header1.xml", &format!("<w:hdr {W}>{header}</w:hdr>")),
    ])
}

#[test]
fn a_header_styleref_finds_its_style_by_name_on_its_page() {
    let Some(doc) = doc() else { return };
    let pages: Vec<String> = page_chars(&doc)
        .into_iter()
        .map(|mut p| {
            p.sort_by(|a, b| a.2.total_cmp(&b.2).then(a.1.total_cmp(&b.1)));
            p.into_iter().map(|c| c.0).filter(|c| !c.is_whitespace()).collect()
        })
        .collect();
    let has = |k: usize, s: &str| assert!(pages[k].contains(s), "page {}: {s} in {}", k + 1, pages[k]);
    has(0, "A:AlphaL:BetaD:AlphaF:LatethreeE:Error!Notextofspecifiedstyleindocument.");
    has(0, "U:Error!UsetheHometabtoapplyNoSuchtothetextthatyouwanttoappearhere.K:kfirstKL:klast");
    has(1, "A:BetaL:BetaD:BetaF:Latethree");
    has(1, "K:klastKL:klast");
    has(2, "A:GammaL:GammaD:Gamma");
}
