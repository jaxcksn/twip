//! Saved packages: well-formed parts, kept properties, parts and relationships

use super::super::Editor;
use super::super::commands::{Command, Outcome};
use super::{blank_body, fonts, open, texts_ids};
use crate::docx::Block;
use crate::layout::Unit;
use std::io::Read;

fn part(pkg: &[u8], name: &str) -> String {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(pkg)).unwrap();
    let mut s = String::new();
    z.by_name(name).unwrap().read_to_string(&mut s).unwrap();
    s
}

fn cmd(v: serde_json::Value) -> Command {
    serde_json::from_value(v).expect("a command")
}

fn first_cell(e: &mut Editor) {
    let p = texts_ids(e)
        .into_iter()
        .find(|p| e.index().para_slot(*p).is_some_and(|s| matches!(s.container, crate::editing::index::Container::Cell { .. })))
        .expect("a cell");
    e.select_range(p, Unit(0), Unit(0), fonts());
}

#[test]
fn control_characters_keep_the_part_well_formed() {
    let mut e = Editor::open(&blank_body("<w:p/>"), fonts()).unwrap();
    e.execute(Command::InsertText("a\u{1}b\u{1b}c\u{0}d".into()), fonts());
    let saved = crate::docx::write::save(e.document()).unwrap();
    let xml = part(&saved, "word/document.xml");
    assert!(roxmltree::Document::parse(&xml).is_ok(), "not well-formed: {xml}");
}

#[test]
fn control_characters_keep_the_saved_file_readable() {
    let mut e = Editor::open(&blank_body("<w:p/>"), fonts()).unwrap();
    e.execute(Command::InsertText("x\u{1b}y".into()), fonts());
    let saved = crate::docx::write::save(e.document()).unwrap();
    assert!(crate::docx::read_editable(&saved).is_ok());
}

#[test]
fn checkbox_face_is_escaped_when_toggled() {
    let src = blank_body(concat!(
        "<w:p xmlns:w14=\"http://schemas.microsoft.com/office/word/2010/wordml\"><w:sdt><w:sdtPr><w14:checkbox><w14:checked w14:val=\"0\"/><w14:checkedState w14:val=\"0026\" w14:font=\"MS Gothic\"/>",
        "<w14:uncheckedState w14:val=\"2610\" w14:font=\"MS Gothic\"/></w14:checkbox></w:sdtPr><w:sdtContent>",
        "<w:r><w:t>\u{2610}</w:t></w:r></w:sdtContent></w:sdt><w:r><w:t> Label</w:t></w:r></w:p>"
    ));
    let mut e = Editor::open(&src, fonts()).unwrap();
    e.complete(fonts());
    let p = texts_ids(&e)[0];
    e.select_range(p, Unit(0), Unit(1), fonts());
    e.execute(Command::InsertText(" ".into()), fonts());
    let saved = crate::docx::write::save(e.document()).unwrap();
    let xml = part(&saved, "word/document.xml");
    assert!(roxmltree::Document::parse(&xml).is_ok(), "not well-formed: {xml}");
}

fn table_edit_saved(c: serde_json::Value) -> (String, crate::docx::Document) {
    let mut e = open("FX-TBL-BASIC.docx");
    first_cell(&mut e);
    let out = e.execute(cmd(c.clone()), fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{c}: {out:?}");
    let saved = crate::docx::write::save(e.document()).unwrap();
    (part(&saved, "word/document.xml"), crate::docx::read_editable(&saved).unwrap())
}

fn first_table(d: &crate::docx::Document) -> &crate::docx::Table {
    d.sections.iter().flat_map(|s| s.blocks.iter()).find_map(|b| if let Block::Table(t) = b { Some(t) } else { None }).expect("a table")
}

#[test]
fn row_can_break_alone_is_saved() {
    let (xml, d) = table_edit_saved(serde_json::json!({"type":"setRowCanBreak","value":false}));
    assert!(xml.contains("<w:cantSplit"), "cantSplit lost");
    assert!(first_table(&d).rows[0].cant_split);
}

#[test]
fn table_cell_spacing_alone_is_saved() {
    let (xml, _) = table_edit_saved(serde_json::json!({"type":"setTableCellSpacing","value":2}));
    assert!(xml.contains("tblCellSpacing"), "cell spacing lost");
}

#[test]
fn table_alt_text_alone_is_saved() {
    let (xml, _) = table_edit_saved(serde_json::json!({"type":"setTableAltText","title":"T"}));
    assert!(xml.contains("tblCaption"), "alt text lost");
}

#[test]
fn hidden_row_alone_is_saved() {
    let (xml, _) = table_edit_saved(serde_json::json!({"type":"setRowProperties","hidden":true}));
    assert!(xml.contains("<w:hidden"), "hidden lost");
}

#[test]
fn header_row_survives_insert_column() {
    let cell = "<w:tc><w:tcPr><w:tcW w:w=\"2000\" w:type=\"dxa\"/></w:tcPr><w:p><w:r><w:t>c</w:t></w:r></w:p></w:tc>";
    let row = format!("<w:tr><w:trPr><w:trHeight w:val=\"400\"/></w:trPr>{cell}{cell}</w:tr>");
    let tbl = format!(
        "<w:tbl><w:tblPr><w:tblW w:w=\"0\" w:type=\"auto\"/></w:tblPr><w:tblGrid><w:gridCol w:w=\"2000\"/><w:gridCol w:w=\"2000\"/></w:tblGrid>{row}{row}</w:tbl><w:p/>"
    );
    let mut e = Editor::open(&blank_body(&tbl), fonts()).unwrap();
    first_cell(&mut e);
    let out = e.execute(cmd(serde_json::json!({"type":"setHeaderRow","value":true})), fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    let out = e.execute(cmd(serde_json::json!({"type":"insertColumns","where":"right"})), fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    let saved = crate::docx::write::save(e.document()).unwrap();
    let d = crate::docx::read_editable(&saved).unwrap();
    assert!(first_table(&d).rows[0].header, "header flag lost: {}", part(&saved, "word/document.xml"));
}

const NS: &str = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships""#;
const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

/// The blank package with `document` as its main part, extra relationships, content types and parts.
fn pkg(document: &str, rels: &str, types: &str, parts: Vec<(String, Vec<u8>)>) -> Vec<u8> {
    let blank = crate::docx::blank::blank_docx();
    let read = |name: &str| crate::docx::package::read(&blank, &[], name).expect("reads").expect(name);
    let mut all = vec![
        (
            "word/_rels/document.xml.rels".to_string(),
            read("word/_rels/document.xml.rels").replace("</Relationships>", &format!("{rels}</Relationships>")).into_bytes(),
        ),
        ("[Content_Types].xml".to_string(), read("[Content_Types].xml").replace("</Types>", &format!("{types}</Types>")).into_bytes()),
        (
            "word/document.xml".to_string(),
            format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document {NS}><w:body>{document}</w:body></w:document>"#)
                .into_bytes(),
        ),
    ];
    all.extend(parts);
    crate::docx::write::repackage(&blank, &all, zip::CompressionMethod::Deflated).expect("repackages")
}

/// The images the main part's blips show, in order.
fn blip_images(saved: &[u8]) -> Vec<Vec<u8>> {
    let xml = part(saved, "word/document.xml");
    let rels = part(saved, "word/_rels/document.xml.rels");
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(saved)).unwrap();
    xml.split("r:embed=\"")
        .skip(1)
        .filter_map(|s| s.split('"').next())
        .map(|id| {
            let rel = rels.split("<Relationship ").find(|r| r.contains(&format!("Id=\"{id}\""))).expect("its relationship");
            let target = rel.split("Target=\"").nth(1).and_then(|s| s.split('"').next()).unwrap();
            let mut got = Vec::new();
            z.by_name(&format!("word/{target}")).unwrap().read_to_end(&mut got).unwrap();
            got
        })
        .collect()
}

fn a_picture() -> Vec<u8> {
    open("FX-OBJ.docx").document().images.last().cloned().expect("an image")
}

fn insert_picture(e: &mut Editor, data: &[u8]) {
    let out =
        e.execute(Command::InsertPicture(crate::api::command::Picture { data: crate::api::value::Bytes(data.to_vec()), alt_text: None }), fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
}

#[test]
fn a_new_picture_does_not_reuse_an_existing_relationship_id() {
    let (old, new) = (b"\x89PNG\r\n\x1a\nan earlier picture".to_vec(), a_picture());
    let rels = format!(
        r#"<Relationship Id="rIdCeImg0" Type="{REL}/image" Target="media/ceimage0.png"/><Relationship Id="rIdCeImg1" Type="{REL}/image" Target="media/ceimage1.png"/>"#
    );
    let types = r#"<Default Extension="png" ContentType="image/png"/>"#;
    let media = vec![("word/media/ceimage0.png".into(), old.clone()), ("word/media/ceimage1.png".into(), old)];
    let bytes = pkg("<w:p><w:r><w:t>Hi</w:t></w:r></w:p>", &rels, types, media);
    let mut e = Editor::open(&bytes, fonts()).unwrap();
    insert_picture(&mut e, &new);
    assert_eq!(blip_images(&crate::docx::write::save(e.document()).unwrap()), [new], "the blip shows the earlier image");
}

#[test]
fn a_new_picture_beside_one_an_earlier_save_named_keeps_its_own_image() {
    let (old, new) = (open("FX-OBJ.docx").document().images[0].clone(), a_picture());
    assert_ne!(old, new);
    // An earlier save named its picture's relationship as a new one now would be (the next image's index).
    let rels = format!(r#"<Relationship Id="rIdCeImg1" Type="{REL}/image" Target="media/ceimage1.png"/>"#);
    let pic = concat!(
        r#"<w:r><w:drawing><wp:inline xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing"><wp:extent cx="95250" cy="95250"/>"#,
        r#"<wp:docPr id="1" name="Picture 1"/><a:graphic xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">"#,
        r#"<a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture">"#,
        r#"<pic:nvPicPr><pic:cNvPr id="1" name="p.png"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed="rIdCeImg1"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill>"#,
        r#"<pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="95250" cy="95250"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr></pic:pic>"#,
        r#"</a:graphicData></a:graphic></wp:inline></w:drawing></w:r>"#
    );
    let types = r#"<Default Extension="png" ContentType="image/png"/>"#;
    let bytes = pkg(&format!("<w:p>{pic}<w:r><w:t>Hi</w:t></w:r></w:p>"), &rels, types, vec![("word/media/ceimage1.png".into(), old.clone())]);
    let mut e = Editor::open(&bytes, fonts()).unwrap();
    assert_eq!(e.document().images.len(), 1);
    e.select_range(super::para_id(&e, 0), Unit(3), Unit(3), fonts());
    insert_picture(&mut e, &new);
    assert!(blip_images(&crate::docx::write::save(e.document()).unwrap()) == [old, new], "each picture shows its own image");
}

fn sect_refs(xml: &str) -> Vec<usize> {
    let d = roxmltree::Document::parse(xml).expect("well-formed");
    d.descendants()
        .filter(|n| n.tag_name().name() == "sectPr")
        .map(|s| {
            s.children()
                .filter(|c| c.tag_name().name() == "headerReference" && c.attributes().any(|a| a.name() == "type" && a.value() == "default"))
                .count()
        })
        .collect()
}

fn shared_header_package() -> Vec<u8> {
    let rels = format!(r#"<Relationship Id="rIdH" Type="{REL}/header" Target="header1.xml"/>"#);
    let types = r#"<Override PartName="/word/header1.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml"/>"#;
    let header = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:hdr {NS}><w:p><w:r><w:t>Shared</w:t></w:r></w:p></w:hdr>"#);
    let r = r#"<w:headerReference w:type="default" r:id="rIdH"/>"#;
    let doc = format!(
        r#"<w:p><w:pPr><w:sectPr>{r}</w:sectPr></w:pPr><w:r><w:t>One</w:t></w:r></w:p><w:p><w:pPr><w:sectPr>{r}</w:sectPr></w:pPr><w:r><w:t>Two</w:t></w:r></w:p><w:p><w:r><w:t>Three</w:t></w:r></w:p><w:sectPr/>"#
    );
    pkg(&doc, &rels, types, vec![("word/header1.xml".into(), header.into_bytes())])
}

fn caret_in(e: &mut Editor, text: &str) {
    let at = super::texts(e).iter().position(|t| t == text).expect("a paragraph");
    e.select_range(super::para_id(e, at), Unit(0), Unit(0), fonts());
}

#[test]
fn removing_a_shared_header_leaves_one_reference_per_type() {
    let mut e = Editor::open(&shared_header_package(), fonts()).unwrap();
    caret_in(&mut e, "Two");
    let out = e.execute(cmd(serde_json::json!({"type":"removeHeaderFooter","kind":"header","pages":"default"})), fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    let saved = crate::docx::write::save(e.document()).unwrap();
    let xml = part(&saved, "word/document.xml");
    assert!(sect_refs(&xml).iter().all(|n| *n <= 1), "two default references in one sectPr: {xml}");
}

#[test]
fn a_header_made_after_another_was_removed_is_referenced() {
    // Sections 1 and 2 own header1 and header2; section 2 links to the previous (header2 goes), then section 1 gains a first-page header.
    let rels = format!(
        r#"<Relationship Id="rIdH1" Type="{REL}/header" Target="header1.xml"/><Relationship Id="rIdH2" Type="{REL}/header" Target="header2.xml"/>"#
    );
    let ct = |n: u8| {
        format!(
            r#"<Override PartName="/word/header{n}.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml"/>"#
        )
    };
    let hdr = |t: &str| {
        format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:hdr {NS}><w:p><w:r><w:t>{t}</w:t></w:r></w:p></w:hdr>"#).into_bytes()
    };
    let doc = r#"<w:p><w:pPr><w:sectPr><w:headerReference w:type="default" r:id="rIdH1"/></w:sectPr></w:pPr><w:r><w:t>One</w:t></w:r></w:p><w:p><w:r><w:t>Two</w:t></w:r></w:p><w:sectPr><w:headerReference w:type="default" r:id="rIdH2"/></w:sectPr>"#;
    let bytes = pkg(doc, &rels, &(ct(1) + &ct(2)), vec![("word/header1.xml".into(), hdr("H1")), ("word/header2.xml".into(), hdr("H2"))]);
    let mut e = Editor::open(&bytes, fonts()).unwrap();
    caret_in(&mut e, "Two");
    let link = cmd(serde_json::json!({"type":"setHeaderFooterLinked","kind":"header","pages":"default","linked":true}));
    assert!(matches!(e.execute(link, fonts()), Outcome::Changed(_)));
    let first = cmd(serde_json::json!({"type":"removeHeaderFooter","kind":"header","pages":"first"}));
    let out = e.execute(first, fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    assert!(e.document().sections[1].headers[1].is_some());
    let again = Editor::open(&crate::docx::write::save(e.document()).unwrap(), fonts()).unwrap();
    assert!(again.document().sections[1].headers[1].is_some(), "section 2 lost its first-page header");
}

#[test]
fn a_stray_sectpr_in_a_cell_does_not_take_a_section_edit() {
    let cell = r#"<w:tbl><w:tblPr><w:tblW w:w="0" w:type="auto"/></w:tblPr><w:tblGrid><w:gridCol w:w="2000"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w="2000" w:type="dxa"/></w:tcPr><w:p><w:pPr><w:sectPr><w:pgSz w:w="10000" w:h="10000"/></w:sectPr></w:pPr><w:r><w:t>cell</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#;
    let doc = format!(
        r#"{cell}<w:p><w:pPr><w:sectPr><w:pgSz w:w="12240" w:h="15840"/></w:sectPr></w:pPr><w:r><w:t>One</w:t></w:r></w:p><w:p><w:r><w:t>Two</w:t></w:r></w:p><w:sectPr><w:pgSz w:w="12240" w:h="15840"/></w:sectPr>"#
    );
    let mut e = Editor::open(&pkg(&doc, "", "", vec![]), fonts()).unwrap();
    assert_eq!(e.document().sections.len(), 2);
    caret_in(&mut e, "One");
    let out = e.execute(cmd(serde_json::json!({"type":"setOrientation","value":"landscape"})), fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    let xml = part(&crate::docx::write::save(e.document()).unwrap(), "word/document.xml");
    let d = roxmltree::Document::parse(&xml).unwrap();
    let attr = |n: roxmltree::Node, k: &str| n.attributes().find(|a| a.name() == k).unwrap().value().to_string();
    let sizes: Vec<(String, String)> = d.descendants().filter(|n| n.tag_name().name() == "pgSz").map(|n| (attr(n, "w"), attr(n, "h"))).collect();
    assert_eq!(sizes[0], ("10000".into(), "10000".into()), "the cell's sectPr was edited: {sizes:?}");
    assert_eq!(sizes[1], ("15840".into(), "12240".into()), "the real section was not rotated: {sizes:?}");
}

#[test]
fn an_edited_style_with_a_duplicate_id_reads_back_edited() {
    let blank = crate::docx::blank::blank_docx();
    let styles = crate::docx::package::read(&blank, &[], "word/styles.xml").unwrap().unwrap();
    let h = |extra: &str| {
        format!(r#"<w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:basedOn w:val="Normal"/>{extra}</w:style>"#)
    };
    let styles = styles.replace("</w:styles>", &format!("{}{}</w:styles>", h(""), h(r#"<w:uiPriority w:val="40"/>"#)));
    let bytes = pkg(
        r#"<w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>T</w:t></w:r></w:p>"#,
        "",
        "",
        vec![("word/styles.xml".into(), styles.into_bytes())],
    );
    let mut e = Editor::open(&bytes, fonts()).unwrap();
    let out = e.execute(cmd(serde_json::json!({"type":"setStyleMeta","id":"Heading1","priority":5})), fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    let saved = crate::docx::write::save(e.document()).unwrap();
    let xml = part(&saved, "word/styles.xml");
    let d = roxmltree::Document::parse(&xml).unwrap();
    let last = d
        .descendants()
        .rfind(|n| n.tag_name().name() == "style" && n.attributes().any(|a| a.name() == "styleId" && a.value() == "Heading1"))
        .unwrap();
    let prio = last
        .children()
        .find(|c| c.tag_name().name() == "uiPriority")
        .and_then(|c| c.attributes().find(|a| a.name() == "val").map(|a| a.value().to_string()));
    assert_eq!(prio.as_deref(), Some("5"), "the last Heading1, which the reader keeps, is stale: {xml}");
}

#[test]
fn a_new_header_beside_a_main_part_outside_word_is_referenced() {
    use std::io::Write;
    let blank = crate::docx::blank::blank_docx();
    let styles = crate::docx::package::read(&blank, &[], "word/styles.xml").unwrap().unwrap();
    let files = [
        (
            "[Content_Types].xml",
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/docs/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/docs/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/></Types>"#.to_string(),
        ),
        (
            "_rels/.rels",
            format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{REL}/officeDocument" Target="docs/document.xml"/></Relationships>"#),
        ),
        (
            "docs/_rels/document.xml.rels",
            format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{REL}/styles" Target="styles.xml"/></Relationships>"#),
        ),
        ("docs/styles.xml", styles),
        (
            "docs/document.xml",
            format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document {NS}><w:body><w:p><w:r><w:t>Body</w:t></w:r></w:p><w:sectPr/></w:body></w:document>"#),
        ),
    ];
    let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (n, d) in files {
        z.start_file(n, zip::write::SimpleFileOptions::default()).unwrap();
        z.write_all(d.as_bytes()).unwrap();
    }
    let bytes = z.finish().unwrap().into_inner();
    let mut e = Editor::open(&bytes, fonts()).expect("opens");
    let out = e.execute(cmd(serde_json::json!({"type":"removeHeaderFooter","kind":"header","pages":"default"})), fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    assert!(e.document().sections[0].headers[0].is_some());
    let saved = crate::docx::write::save(e.document()).unwrap();
    let rels = part(&saved, "docs/_rels/document.xml.rels");
    let target = rels.split("Target=\"").skip(1).filter_map(|t| t.split('"').next()).find(|t| t.contains("header")).expect("a header relationship");
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(&saved)).unwrap();
    let resolved = if let Some(abs) = target.strip_prefix('/') { abs.to_string() } else { format!("docs/{target}") };
    assert!(z.by_name(&resolved).is_ok(), "the target {target} resolves to {resolved}, which is not a part: {rels}");
    let again = Editor::open(&saved, fonts()).expect("reopens");
    assert!(again.document().sections[0].headers[0].is_some(), "the header's reference is broken");
}

fn shape_package(sp_pr: &str) -> Vec<u8> {
    let ns = r#"xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape""#;
    let drawing = format!(
        r#"<w:drawing {ns}><wp:anchor distT="0" distB="0" distL="0" distR="0" simplePos="0" relativeHeight="1" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="page"><wp:posOffset>914400</wp:posOffset></wp:positionH><wp:positionV relativeFrom="page"><wp:posOffset>914400</wp:posOffset></wp:positionV><wp:extent cx="1143000" cy="762000"/><wp:effectExtent l="0" t="0" r="0" b="0"/><wp:wrapNone/><wp:docPr id="1" name="Box"/><a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wps:wsp><wps:cNvSpPr/>{sp_pr}<wps:bodyPr/></wps:wsp></a:graphicData></a:graphic></wp:anchor></w:drawing>"#
    );
    pkg(&format!("<w:p><w:r><w:t>Before</w:t></w:r><w:r>{drawing}</w:r></w:p>"), "", "", vec![])
}

fn recoloured(sp_pr: &str) -> String {
    use crate::docx::Inline;
    use crate::drawing::colour::Colour;
    use crate::drawing::paint::{Fill, Line};
    let mut d = crate::docx::read_editable(&shape_package(sp_pr)).expect("reads");
    let Block::Para(p) = &mut d.sections[0].blocks[0] else { panic!() };
    let Some(Inline::Float(f, _)) = p.inlines.iter_mut().find(|i| matches!(i, Inline::Float(..))) else { panic!("a float") };
    let sp = f.shape.as_deref_mut().and_then(|s| s.sp_mut()).expect("a shape");
    sp.fill = Some(Fill::Solid(Colour::srgb(0x00FF00)));
    sp.line = Some(Line { width: Some(crate::drawing::units::Emu(12700)), fill: Some(Fill::Solid(Colour::srgb(0x0000FF))), ..Line::default() });
    part(&crate::docx::write::save(&d).unwrap(), "word/document.xml")
}

fn sp_pr_children(xml: &str) -> Vec<String> {
    let d = roxmltree::Document::parse(xml).expect("well-formed");
    let sp = d.descendants().find(|n| n.tag_name().name() == "spPr").expect("spPr");
    sp.children().filter(|c| c.is_element()).map(|c| c.tag_name().name().to_string()).collect()
}

#[test]
fn a_shape_given_fill_and_line_keeps_schema_order() {
    let xml = recoloured(
        r#"<wps:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="1143000" cy="762000"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></wps:spPr>"#,
    );
    assert_eq!(sp_pr_children(&xml), ["xfrm", "prstGeom", "solidFill", "ln"], "{xml}");
}

#[test]
fn a_shape_with_an_empty_sppr_gets_its_fill_inside_it() {
    let xml = recoloured("<wps:spPr/>");
    assert!(sp_pr_children(&xml).contains(&"solidFill".to_string()), "the fill is outside spPr: {xml}");
}

fn illegal(xml: &str) -> Vec<char> {
    xml.chars().filter(|&c| !matches!(c, '\t' | '\n' | '\r' | ' '..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..='\u{10FFFF}')).collect()
}

#[test]
fn typed_and_pasted_control_characters_never_reach_the_saved_part() {
    for c in [
        Command::InsertText("a\u{8}b\u{1b}c\u{0}d\u{FFFE}".into()),
        Command::PasteText("p\u{1b}[0mq".into()),
        Command::InsertText("e\u{c}f\u{b}g\u{1}".into()),
    ] {
        let mut e = Editor::open(&super::blank_with("x"), fonts()).unwrap();
        e.execute(c.clone(), fonts());
        let saved = crate::docx::write::save(e.document()).unwrap();
        let xml = part(&saved, "word/document.xml");
        assert!(illegal(&xml).is_empty(), "{c:?}: {:?}", illegal(&xml));
        assert!(!xml.contains("<w:br"), "{c:?}: text came back as a break: {xml}");
    }
}

#[test]
fn control_characters_in_model_text_are_not_written() {
    use crate::docx::Inline;
    let mut d = crate::docx::read_editable(&super::blank_with("x")).unwrap();
    let Block::Para(p) = &mut d.sections[0].blocks[0] else { panic!() };
    let run = p.inlines.iter().find_map(|i| i.run().cloned()).unwrap();
    p.inlines.push(Inline::Text("a\u{c}b\u{1b}c\u{b}d".into(), run));
    let xml = part(&crate::docx::write::save(&d).unwrap(), "word/document.xml");
    assert!(roxmltree::Document::parse(&xml).is_ok(), "{xml}");
    assert!(!xml.contains("<w:br"), "text came back as a break: {xml}");
    assert!(xml.contains("abcd"), "{xml}");
}

fn json(e: &mut Editor, c: serde_json::Value) -> serde_json::Value {
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
    let msg = serde_json::json!({ "type": "command", "command": c, "seq": seq });
    serde_json::from_str(&e.apply_json(&msg.to_string(), fonts())).expect("a reply")
}

fn refused(r: &serde_json::Value) -> bool {
    r.get("error").is_some() || r["answer"].get("error").is_some()
}

#[test]
fn control_strings_with_control_characters_keep_the_saved_document_readable() {
    let mut e = Editor::open(&blank_body("<w:p><w:r><w:t>Hi</w:t></w:r></w:p>"), fonts()).unwrap();
    let r = json(&mut e, serde_json::json!({ "type": "insertControl", "kind": "plainText", "tag": "a\u{1}b", "title": "x\u{8}" }));
    assert!(!refused(&r), "{r}");
    let saved = crate::docx::write::save(e.document()).unwrap();
    let xml = part(&saved, "word/document.xml");
    assert!(illegal(&xml).is_empty(), "{xml}");
    crate::docx::read_editable(&saved).expect("the saved document reads back");
}

#[test]
fn a_multi_line_fill_keeps_the_data_part_well_formed() {
    let bytes = blank_body(concat!(
        "<w:p><w:r><w:t xml:space=\"preserve\">Before </w:t></w:r><w:sdt><w:sdtPr><w:alias w:val=\"C\"/><w:id w:val=\"5\"/><w:text w:multiLine=\"1\"/>",
        "</w:sdtPr><w:sdtContent><w:r><w:t>x</w:t></w:r></w:sdtContent></w:sdt></w:p><w:p><w:r><w:t>Other</w:t></w:r></w:p>"
    ));
    let mut e = Editor::open(&bytes, fonts()).unwrap();
    let k = *e.document().controls.iter().find(|(_, c)| c.props.alias.as_deref() == Some("C")).map(|(k, _)| k).unwrap();
    let id = crate::editing::elements::id_str(crate::docx::ElementId::Control(k));
    let bind = serde_json::json!({ "xpath": "/c:case[1]/c:m[1]", "prefixMappings": "xmlns:c='urn:case'" });
    assert!(!refused(&json(&mut e, serde_json::json!({ "type": "setControl", "id": id, "binding": bind }))));
    let data = "<case xmlns=\"urn:case\"><m>x</m></case>";
    assert!(!refused(&json(&mut e, serde_json::json!({ "type": "writeDataPart", "namespace": "urn:case", "xml": data }))));
    let r = json(&mut e, serde_json::json!({ "type": "fillControl", "id": id, "text": "a\nb" }));
    assert!(!refused(&r), "{r}");
    let xml = e.document().data_parts.iter().find(|d| d.xml.contains("urn:case")).unwrap().xml.clone();
    assert!(roxmltree::Document::parse(&xml).is_ok(), "ill-formed: {xml:?}");
    assert!(xml.contains("<m>a\nb</m>"), "{xml:?}");
    let saved = crate::docx::write::save(e.document()).unwrap();
    assert!(part(&saved, "word/document.xml").contains("<w:br/>"), "the line break is written as one");
    let again = Editor::open(&saved, fonts()).unwrap();
    assert!(super::texts(&again).iter().any(|t| t.contains("a\u{2028}b")), "{:?}", super::texts(&again));
}

/// The blank document without the main part's relationships: no styles, settings or notes parts.
fn without_main_rels() -> Vec<u8> {
    let blank = crate::docx::blank::blank_docx();
    crate::docx::write::repackage_without(&blank, &[], &["word/_rels/document.xml.rels".into()], zip::CompressionMethod::Deflated)
        .expect("repackages")
}

#[test]
fn a_first_footnote_is_saved_without_main_part_relationships() {
    let mut e = Editor::open(&without_main_rels(), fonts()).unwrap();
    assert!(matches!(e.execute(Command::InsertFootnote, fonts()), Outcome::Changed(_)));
    e.execute(Command::InsertText("Note".into()), fonts());
    let saved = crate::docx::write::save(e.document()).expect("saves");
    assert!(part(&saved, "word/_rels/document.xml.rels").contains("relationships/footnotes"));
    let again = Editor::open(&saved, fonts()).unwrap();
    assert!(again.document().footnotes.values().any(|b| !b.is_empty()));
}

#[test]
fn defaults_set_without_a_styles_part_are_saved() {
    let mut e = Editor::open(&without_main_rels(), fonts()).unwrap();
    let d = crate::api::command::DocumentDefaults { size: Some(crate::api::value::Points(13.0)), ..Default::default() };
    assert!(matches!(e.execute(Command::SetDocumentDefaults(d), fonts()), Outcome::Changed(_)));
    let saved = crate::docx::write::save(e.document()).expect("saves");
    let rels = part(&saved, "word/_rels/document.xml.rels");
    assert!(rels.contains("relationships/styles"), "{rels}");
    let target = rels.split("relationships/styles\" Target=\"").nth(1).and_then(|s| s.split('"').next()).expect("a target");
    assert!(part(&saved, &format!("word/{target}")).contains("docDefaults"));
}

#[test]
fn defaults_added_to_a_styles_part_go_after_its_whole_start_tag() {
    let styles = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:styles {NS} xmlns:x="urn:x" x:note="a>b"><w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style></w:styles>"#
    );
    let bytes = pkg("<w:p><w:r><w:t>Hi</w:t></w:r></w:p>", "", "", vec![("word/styles.xml".into(), styles.into_bytes())]);
    let mut e = Editor::open(&bytes, fonts()).unwrap();
    let d = crate::api::command::DocumentDefaults { size: Some(crate::api::value::Points(13.0)), ..Default::default() };
    let out = e.execute(Command::SetDocumentDefaults(d), fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    let xml = part(&crate::docx::write::save(e.document()).expect("saves"), "word/styles.xml");
    assert!(xml.contains(r#"x:note="a>b"><w:docDefaults>"#), "{xml}");
}

#[test]
fn a_drop_cap_made_on_a_split_paragraph_goes_before_that_paragraph() {
    use crate::api::command::{DropCap, DropCapPosition};
    let mut e = Editor::open(&blank_body("<w:p><w:r><w:t>Alpha Bravo</w:t></w:r></w:p>"), fonts()).unwrap();
    let p = texts_ids(&e)[0];
    e.select_range(p, Unit(6), Unit(6), fonts());
    e.execute(Command::InsertParagraph, fonts());
    let d = DropCap { position: DropCapPosition::Dropped, font: None, lines: Some(3), distance: None };
    let out = e.execute(Command::SetDropCap(d), fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    let xml = part(&crate::docx::write::save(e.document()).unwrap(), "word/document.xml");
    let (alpha, frame) = (xml.find("Alpha").expect("the first paragraph"), xml.find("w:dropCap").expect("the drop cap"));
    assert!(alpha < frame && frame < xml.find("ravo").expect("the second paragraph"), "{xml}");
}
