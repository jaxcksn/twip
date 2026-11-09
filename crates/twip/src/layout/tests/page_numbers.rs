//! Chapter page numbers

use super::{package, page_chars};

const W: &str = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships""#;

fn chaptered(sep: &str) -> Option<crate::docx::Document> {
    let page = r#"<w:p><w:r><w:t xml:space="preserve">PG </w:t></w:r><w:r><w:pgNum/></w:r></w:p>"#;
    let heading = |t: &str, direct: &str| format!(r#"<w:p><w:pPr><w:pStyle w:val="Heading1"/>{direct}</w:pPr><w:r><w:t>{t}</w:t></w:r></w:p>"#);
    let brk = r#"<w:p><w:r><w:br w:type="page"/></w:r></w:p>"#;
    let body = format!(
        r#"<w:document {W}><w:body>{page}{brk}{}{page}{brk}{}{page}<w:sectPr><w:pgNumType w:chapStyle="1"{sep}/></w:sectPr></w:body></w:document>"#,
        heading("Alpha", ""),
        heading("Beta", r#"<w:numPr><w:ilvl w:val="0"/><w:numId w:val="2"/></w:numPr>"#),
    );
    let styles = format!(
        r#"<w:styles {W}><w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style><w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr></w:pPr></w:style></w:styles>"#
    );
    let lvl =
        |text: &str, fmt: &str| format!(r#"<w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="{fmt}"/><w:lvlText w:val="{text}"/></w:lvl>"#);
    let numbering = format!(
        r#"<w:numbering {W}><w:abstractNum w:abstractNumId="0">{}</w:abstractNum><w:abstractNum w:abstractNumId="1">{}</w:abstractNum><w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num><w:num w:numId="2"><w:abstractNumId w:val="1"/></w:num></w:numbering>"#,
        lvl("Chapter %1", "upperRoman"),
        lvl("%1.", "decimal"),
    );
    let ct = r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/><Override PartName="/word/numbering.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml"/></Types>"#;
    let rels = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/><Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering" Target="numbering.xml"/></Relationships>"#;
    package(&[
        ("[Content_Types].xml", ct),
        (
            "_rels/.rels",
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#,
        ),
        ("word/_rels/document.xml.rels", rels),
        ("word/document.xml", &body),
        ("word/styles.xml", &styles),
        ("word/numbering.xml", &numbering),
    ])
}

fn numbers(doc: &crate::docx::Document) -> Vec<String> {
    page_chars(doc)
        .iter()
        .map(|p| {
            let s: String = p.iter().map(|c| c.0).collect();
            s.split("PG ").nth(1).unwrap_or_default().chars().take_while(|c| !c.is_alphabetic() || c.is_uppercase()).collect()
        })
        .collect()
}

#[test]
fn page_numbers_lead_with_the_chapter() {
    let Some(doc) = chaptered("") else { return };
    // Before the first heading its number; a directly numbered heading isn't a chapter.
    assert_eq!(numbers(&doc), ["I-1", "I-2", "I-3"]);
    let Some(doc) = chaptered(r#" w:chapSep="emDash""#) else { return };
    assert_eq!(numbers(&doc)[1], "I\u{2014}2");
}

/// Two sections of one paragraph each; S2 starts on a new page with `pg` numbering, with `settings` children.
fn two_sections(pg: &str, settings: &str) -> Option<crate::docx::Document> {
    let field = r#"<w:r><w:t xml:space="preserve">PG </w:t></w:r><w:r><w:pgNum/></w:r>"#;
    let body = format!(
        r#"<w:document {W}><w:body><w:p>{field}<w:pPr><w:sectPr/></w:pPr></w:p><w:p>{field}</w:p><w:sectPr>{pg}</w:sectPr></w:body></w:document>"#
    );
    let ct = r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/settings.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml"/></Types>"#;
    let rels = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId4" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings" Target="settings.xml"/></Relationships>"#;
    package(&[
        ("[Content_Types].xml", ct),
        (
            "_rels/.rels",
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#,
        ),
        ("word/_rels/document.xml.rels", rels),
        ("word/document.xml", &body),
        ("word/settings.xml", &format!("<w:settings {W}>{settings}</w:settings>")),
    ])
}

#[test]
fn facing_pages_keep_page_number_parity() {
    let start1 = r#"<w:pgNumType w:start="1"/>"#;
    let Some(doc) = two_sections(start1, "<w:mirrorMargins/>") else { return };
    assert_eq!(numbers(&doc), ["1", "", "1"], "a blank page before an odd number on an even place");
    let Some(doc) = two_sections(start1, "") else { return };
    assert_eq!(numbers(&doc), ["1", "1"], "no blank page without facing pages");
    let Some(doc) = two_sections(r#"<w:type w:val="oddPage"/><w:pgNumType w:start="2"/>"#, "") else { return };
    assert_eq!(numbers(&doc), ["1", "3"], "an odd-page restart bumps the number, no blank page");
}
