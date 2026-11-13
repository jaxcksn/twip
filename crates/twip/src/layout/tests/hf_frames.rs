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

#[test]
fn frame_3_an_explicit_position_is_one_twip_less_and_beats_the_alignment() {
    use crate::docx::model::{Block, Inline};
    let frame = |a: &str| format!(r#"<w:p><w:pPr><w:framePr w:w="1440" w:wrap="around" w:hAnchor="margin" w:vAnchor="page" {a}/></w:pPr><w:r><w:t>F</w:t></w:r></w:p>"#);
    let frames: String = [r#"w:x="2880" w:xAlign="right" w:y="6000" w:yAlign="top""#, r#"w:x="-8" w:y="-12""#, r#"w:x="0" w:y="1""#]
        .map(frame)
        .concat();
    let body = format!(r#"<w:document {W}><w:body>{frames}<w:p><w:r><w:t>Body</w:t></w:r></w:p></w:body></w:document>"#);
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
    let got: Vec<_> = doc.sections[0]
        .blocks
        .iter()
        .filter_map(|b| if let Block::Para(p) = b { Some(p) } else { None })
        .flat_map(|p| p.inlines.iter())
        .filter_map(|i| if let Inline::Float(f, _) = i { Some(f) } else { None })
        .map(|f| (f.h_align.clone(), (f.h_offset * 20.0).round(), f.v_align.clone(), (f.v_offset * 20.0).round()))
        .collect();
    let s = |a: &str| Some(a.to_string());
    assert_eq!(got, [(None, 2879.0, None, 5999.0), (s("right"), 0.0, s("bottom"), 0.0), (None, 0.0, None, 0.0)]);
}

fn header_float(wrap: &str, y: i64, head: &str, compat: u32) -> Option<crate::docx::Document> {
    let words = super::words(600);
    let body = format!(
        r#"<w:document {W}><w:body><w:p><w:r><w:t>{words}</w:t></w:r></w:p><w:sectPr><w:headerReference w:type="default" r:id="rId2"/><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr></w:body></w:document>"#
    );
    let header = format!(
        r#"<w:hdr {W} xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture"><w:p><w:r><w:drawing><wp:anchor distT="0" distB="0" distL="0" distR="0" simplePos="0" relativeHeight="1" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="page"><wp:posOffset>914400</wp:posOffset></wp:positionH><wp:positionV relativeFrom="page"><wp:posOffset>{y}</wp:posOffset></wp:positionV><wp:extent cx="2540000" cy="1270000"/>{wrap}<wp:docPr id="1" name="H"/><a:graphic><a:graphicData><pic:pic><pic:blipFill/></pic:pic></a:graphicData></a:graphic></wp:anchor></w:drawing><w:t>{head}</w:t></w:r></w:p></w:hdr>"#
    );
    with_header(&body, &header, compat)
}

fn with_header(body: &str, header: &str, compat: u32) -> Option<crate::docx::Document> {
    let settings = format!(
        r#"<w:settings {W}><w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="{compat}"/></w:compat></w:settings>"#
    );
    package(&[
        (
            "[Content_Types].xml",
            r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/header1.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml"/><Override PartName="/word/settings.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml"/></Types>"#,
        ),
        (
            "_rels/.rels",
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#,
        ),
        (
            "word/_rels/document.xml.rels",
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/header" Target="header1.xml"/><Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings" Target="settings.xml"/></Relationships>"#,
        ),
        ("word/document.xml", body),
        ("word/header1.xml", header),
        ("word/settings.xml", &settings),
    ])
}

#[test]
fn a_paragraph_float_counts_from_above_the_paragraphs_space_before() {
    // A text box 0pt below its paragraph, in a header's and in the body's second paragraph: the paragraph's space
    // before moves its text, not the box.
    let ns = r#"xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape""#;
    let boxed = |c: char| {
        format!(
            r#"<w:r><w:drawing><wp:anchor distT="0" distB="0" distL="0" distR="0" simplePos="0" relativeHeight="1" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="column"><wp:posOffset>3048000</wp:posOffset></wp:positionH><wp:positionV relativeFrom="paragraph"><wp:posOffset>0</wp:posOffset></wp:positionV><wp:extent cx="1270000" cy="635000"/><wp:effectExtent l="0" t="0" r="0" b="0"/><wp:wrapNone/><wp:docPr id="1" name="B"/><a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wps:wsp><wps:cNvSpPr txBox="1"/><wps:spPr><a:prstGeom prst="rect"/></wps:spPr><wps:txbx><w:txbxContent><w:p><w:r><w:t>{c}</w:t></w:r></w:p></w:txbxContent></wps:txbx><wps:bodyPr/></wps:wsp></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r>"#
        )
    };
    let paras = |before: u32, text: char, c: char| {
        format!(
            r#"<w:p><w:pPr><w:spacing w:before="0" w:after="0"/></w:pPr><w:r><w:t>a</w:t></w:r></w:p><w:p><w:pPr><w:spacing w:before="{before}" w:after="0"/></w:pPr>{}<w:r><w:t>{text}</w:t></w:r></w:p>"#,
            boxed(c)
        )
    };
    let ys = |before: u32| {
        let body = format!(
            r#"<w:document {W} {ns}><w:body>{}<w:sectPr><w:headerReference w:type="default" r:id="rId2"/><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="2880" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr></w:body></w:document>"#,
            paras(before, 'b', 'Y')
        );
        let header = format!(r#"<w:hdr {W} {ns}>{}</w:hdr>"#, paras(before, 'h', 'Z'));
        let doc = with_header(&body, &header, 15).expect("reads");
        let page = &super::page_chars(&doc)[0];
        let y = |ch: char| page.iter().find(|c| c.0 == ch).map(|c| c.2).expect("drawn");
        [y('b'), y('Y'), y('h'), y('Z')]
    };
    let (flat, spaced) = (ys(0), ys(480));
    assert!((spaced[0] - flat[0] - 24.0).abs() < 0.01 && (spaced[2] - flat[2] - 24.0).abs() < 0.01, "{flat:?} {spaced:?}");
    assert!((spaced[1] - flat[1]).abs() < 0.01, "body box: {flat:?} {spaced:?}");
    assert!((spaced[3] - flat[3]).abs() < 0.01, "header box: {flat:?} {spaced:?}");
}

#[test]
fn a_header_float_wraps_the_body_text_of_its_page() {
    let beside = |wrap: &str| {
        let doc = header_float(wrap, 3810000, "Head", 12)?;
        let pages = page_chars(&doc);
        let left = |k: usize| pages[k].iter().filter(|c| c.2 > 305.0 && c.2 < 395.0).map(|c| c.1).fold(f64::INFINITY, f64::min);
        Some((left(0), left(1)))
    };
    let Some((first, second)) = beside(r#"<wp:wrapSquare wrapText="bothSides"/>"#) else { return };
    assert!(first >= 272.0 && second >= 272.0, "beside the float on both pages: {first} {second}");
    let (none, _) = beside("<wp:wrapNone/>").expect("lays out");
    assert!(none < 80.0, "wrapNone doesn't wrap: {none}");
}

#[test]
fn a_header_wraps_its_own_float() {
    let Some(doc) = header_float(r#"<wp:wrapSquare wrapText="bothSides"/>"#, 457200, &super::words(40), 15) else { return };
    let pages = page_chars(&doc);
    let beside: Vec<f64> = pages[0].iter().filter(|c| c.2 < 136.0).map(|c| c.1).collect();
    assert!(!beside.is_empty() && beside.iter().all(|&x| x >= 272.0), "header lines start right of the float: {beside:?}");
}

#[test]
fn compat_14_header_text_runs_over_its_own_float() {
    let Some(doc) = header_float(r#"<wp:wrapSquare wrapText="bothSides"/>"#, 457200, &super::words(40), 14) else { return };
    let first = page_chars(&doc)[0].iter().filter(|c| c.2 < 60.0).map(|c| c.1).fold(f64::INFINITY, f64::min);
    assert!(first < 80.0, "the header's lines start at the margin, over the float: {first}");
}

/// A header of a floating table 324pt wide and 18pt tall, then a paragraph with `text` and a picture 10pt above its top
/// over the room beside the table, to the right margin.
fn header_table(compat: u32, text: &str) -> Option<crate::docx::Document> {
    let body = format!(
        r#"<w:document {W}><w:body><w:p><w:r><w:t>{}</w:t></w:r></w:p><w:sectPr><w:headerReference w:type="default" r:id="rId2"/><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr></w:body></w:document>"#,
        super::words(200)
    );
    let header = format!(
        r#"<w:hdr {W} xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture"><w:tbl><w:tblPr><w:tblpPr w:leftFromText="180" w:rightFromText="180" w:vertAnchor="text"/><w:tblW w:w="6480" w:type="dxa"/></w:tblPr><w:tblGrid><w:gridCol w:w="6480"/></w:tblGrid><w:tr><w:trPr><w:trHeight w:val="360" w:hRule="exact"/></w:trPr><w:tc><w:p/></w:tc></w:tr></w:tbl><w:p><w:r>{text}<w:drawing><wp:anchor distT="0" distB="0" distL="0" distR="0" simplePos="0" relativeHeight="1" behindDoc="1" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="column"><wp:posOffset>4318000</wp:posOffset></wp:positionH><wp:positionV relativeFrom="paragraph"><wp:posOffset>-127000</wp:posOffset></wp:positionV><wp:extent cx="1625600" cy="508000"/><wp:wrapSquare wrapText="bothSides"/><wp:docPr id="1" name="H"/><a:graphic><a:graphicData><pic:pic><pic:blipFill/></pic:pic></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r></w:p></w:hdr>"#
    );
    with_header(&body, &header, compat)
}

#[test]
fn a_header_float_beside_a_floating_table_keeps_its_first_place() {
    // Compat 15: the paragraph after the floating table starts at the header top; its float, placed from there over
    // the only room beside the table, pushes the paragraph below the table but stays above the body.
    let Some(doc) = header_table(15, "") else { return };
    let page = &page_chars(&doc)[0];
    let first = page.iter().map(|c| c.2).filter(|&y| y > 72.0).fold(f64::INFINITY, f64::min);
    let right = page.iter().filter(|c| (c.2 - first).abs() < 0.01).map(|c| c.1).fold(0.0, f64::max);
    assert!(right > 430.0, "the float above the body leaves its first line whole: {first} {right}");
}

#[test]
fn a_header_table_placed_from_the_page_wraps_the_body_text() {
    let body = format!(
        r#"<w:document {W}><w:body><w:p><w:r><w:t>{}</w:t></w:r></w:p><w:sectPr><w:headerReference w:type="default" r:id="rId2"/><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr></w:body></w:document>"#,
        super::words(600)
    );
    let header = format!(
        r#"<w:hdr {W}><w:tbl><w:tblPr><w:tblpPr w:leftFromText="0" w:rightFromText="0" w:vertAnchor="page" w:horzAnchor="margin" w:tblpY="6000"/><w:tblW w:w="4000" w:type="dxa"/><w:tblLayout w:type="fixed"/></w:tblPr><w:tblGrid><w:gridCol w:w="4000"/></w:tblGrid><w:tr><w:trPr><w:trHeight w:val="720" w:hRule="exact"/></w:trPr><w:tc><w:p><w:r><w:t>T</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p><w:r><w:t>Head</w:t></w:r></w:p></w:hdr>"#
    );
    for compat in [14, 15] {
        let Some(doc) = with_header(&body, &header, compat) else { return };
        let page = &page_chars(&doc)[0];
        // The table at 300pt on the page, the header's text at its top, the body beside the table.
        let t = page.iter().find(|c| c.0 == 'T').expect("drawn");
        assert!(t.2 > 300.0 && t.2 < 320.0, "{t:?}");
        let head = page.iter().find(|c| c.0 == 'H').expect("drawn");
        assert!(head.1 < 80.0 && head.2 < 60.0, "{head:?}");
        let left = page.iter().filter(|c| c.2 > 315.0 && c.2 < 330.0).map(|c| c.1).fold(f64::INFINITY, f64::min);
        assert!(left > 270.0, "compat {compat}: {left}");
    }
}

#[test]
fn compat_14_header_text_runs_over_its_floating_table() {
    let q = |compat: u32| {
        let doc = header_table(compat, "<w:t>Q</w:t>")?;
        page_chars(&doc)[0].iter().find(|c| c.0 == 'Q').map(|c| (c.1, c.2))
    };
    let Some((x, y)) = q(14) else { return };
    assert!(x < 80.0 && y < 60.0, "compat 14: at the header top, over the table: {x} {y}");
    let (x, y) = q(15).expect("lays out");
    assert!(x < 80.0 && y > 54.0, "compat 15: below the table, left of the float: {x} {y}");
}

#[test]
fn frame_4_a_frame_ending_the_body_anchors_in_the_paragraph_before_it() {
    use crate::docx::model::{Block, Inline};
    let frame = |v: &str| format!(r#"<w:p><w:pPr><w:framePr w:w="1440" w:wrap="around" w:hAnchor="page" {v} w:x="3000" w:y="3000"/></w:pPr><w:r><w:t>F</w:t></w:r></w:p>"#);
    let body = |v: &str| format!(r#"<w:document {W}><w:body><w:p><w:r><w:t>Body</w:t></w:r></w:p>{}</w:body></w:document>"#, frame(v));
    let doc = |v: &str| {
        package(&[
            (
                "[Content_Types].xml",
                r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#,
            ),
            (
                "_rels/.rels",
                r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#,
            ),
            ("word/document.xml", &body(v)),
        ])
    };
    let shape = |d: crate::docx::Document| {
        d.sections[0]
            .blocks
            .iter()
            .map(|b| match b {
                Block::Para(p) => p.inlines.iter().filter(|i| matches!(i, Inline::Float(..))).count(),
                Block::Table(_) => 9,
            })
            .collect::<Vec<_>>()
    };
    let Some(page) = doc(r#"w:vAnchor="page""#) else { return };
    assert_eq!(shape(page), [1], "a float of the body paragraph");
    assert_eq!(shape(doc(r#"w:vAnchor="text""#).unwrap()), [0, 0], "a text-anchored one stays in the flow");
}

#[test]
fn compat_15_the_body_gives_way_to_a_footer_wrapping_its_float_as_laid_out_from_the_bottom_margin() {
    // A 200 × 100pt picture at page (72, 656) beside the footer's text: laid out from the bottom margin line (720) the
    // footer is 5 lines, so the body ends at 756 − 63.25; laid out from there it is 6 lines, drawn from 756 − 75.9.
    let lines: String = (1..=60).map(|k| format!("<w:p><w:r><w:t>L{k:02}</w:t></w:r></w:p>")).collect();
    let body = format!(
        r#"<w:document {W}><w:body>{lines}<w:sectPr><w:footerReference w:type="default" r:id="rId2"/><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr></w:body></w:document>"#
    );
    let pic = r#"<w:r><w:drawing><wp:anchor distT="0" distB="0" distL="114300" distR="114300" simplePos="0" relativeHeight="1" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="page"><wp:posOffset>914400</wp:posOffset></wp:positionH><wp:positionV relativeFrom="page"><wp:posOffset>8331200</wp:posOffset></wp:positionV><wp:extent cx="2540000" cy="1270000"/><wp:wrapSquare wrapText="bothSides"/><wp:docPr id="5" name="S5"/><a:graphic><a:graphicData><pic:pic><pic:blipFill/></pic:pic></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r>"#;
    let footer = format!(
        r#"<w:ftr {W} xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture"><w:p>{pic}<w:r><w:t>Bramble jasper quill copper heron otter cedar juniper quartz. Yarrow glacier nutmeg upland elm lagoon tundra garnet nectar. Umber cobalt kelp ridge beacon ivy prairie delta kestrel. Raven zephyr hollow opal vale fjord moss amber harbor. Orchard velvet dune lichen.</w:t></w:r></w:p></w:ftr>"#
    );
    let styles = format!(
        r#"<w:styles {W}><w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Arial" w:hAnsi="Arial"/><w:sz w:val="22"/></w:rPr></w:rPrDefault><w:pPrDefault><w:pPr><w:spacing w:after="0" w:line="240" w:lineRule="auto"/></w:pPr></w:pPrDefault></w:docDefaults></w:styles>"#
    );
    let settings = format!(
        r#"<w:settings {W}><w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat></w:settings>"#
    );
    let Some(doc) = package(&[
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
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/footer" Target="footer1.xml"/><Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/><Relationship Id="rId4" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings" Target="settings.xml"/></Relationships>"#,
        ),
        ("word/document.xml", &body),
        ("word/footer1.xml", &footer),
        ("word/styles.xml", &styles),
        ("word/settings.xml", &settings),
    ]) else {
        return;
    };
    if super::fonts().face("Arial", None, false, false).is_none() {
        return;
    }
    let page = &page_chars(&doc)[0];
    let mut body: Vec<f64> = page.iter().filter(|c| c.0 == 'L').map(|c| c.2).collect();
    body.dedup();
    let first = page.iter().filter(|c| c.0 == 'B').map(|c| c.2).fold(f64::INFINITY, f64::min);
    assert!(body.len() == 49 && (first - 690.44).abs() < 0.05, "{} {first}", body.len());
}
