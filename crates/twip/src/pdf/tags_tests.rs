use std::rc::Rc;

use super::*;
use crate::api::session::PdfOptions;
use crate::docx::{CellIds, Raw};
use crate::layout::tests::output::{bundled_fonts, root};

const W: &str = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships""#;

fn fixture() -> Vec<u8> {
    std::fs::read(root().join("fixtures/render/pdf-accessible.docx")).expect("the fixture")
}

/// A package of `body`, with `parts` (name, root element) beside it: footnotes, styles.
fn package(body: &str, parts: &[(&str, &str, String)]) -> Vec<u8> {
    let overrides: String = parts
        .iter()
        .map(|(name, kind, _)| {
            format!(r#"<Override PartName="/word/{name}" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.{kind}+xml"/>"#)
        })
        .collect();
    let rels: String = parts
        .iter()
        .enumerate()
        .map(|(i, (name, kind, _))| {
            format!(
                r#"<Relationship Id="rId{}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/{kind}" Target="{name}"/>"#,
                i + 2
            )
        })
        .collect();
    let ct = format!(
        r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>{overrides}</Types>"#
    );
    let document = format!("<w:document {W}><w:body>{body}</w:body></w:document>");
    let mut all: Vec<(&str, &str)> = vec![
        ("[Content_Types].xml", &ct),
        (
            "_rels/.rels",
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#,
        ),
        ("word/document.xml", &document),
    ];
    let rels = format!(r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">{rels}</Relationships>"#);
    all.push(("word/_rels/document.xml.rels", &rels));
    let names: Vec<String> = parts.iter().map(|p| format!("word/{}", p.0)).collect();
    for (name, (_, _, xml)) in names.iter().zip(parts) {
        all.push((name, xml));
    }
    crate::layout::tests::package_bytes(&all).expect("a package")
}

fn export(bytes: &[u8]) -> Vec<u8> {
    let doc = crate::docx::read(bytes).expect("reads");
    let fonts = bundled_fonts();
    super::super::export::export(&doc, &crate::layout::layout(&doc, &fonts), "", &PdfOptions::default(), None).expect("exports").0
}

#[test]
fn the_accessible_fixture_is_linked_and_tagged() {
    let pdf = export(&fixture());
    let text = String::from_utf8_lossy(&pdf);
    for key in [
        // The Title paragraph names the document.
        "/Title(Accessible document)",
        // The external link, and the internal ones to the bookmark, a named destination.
        "/URI(https://example.com/report)",
        "/D(results)",
        "/Dests",
        "/S/Link",
        "/Type/OBJR",
        // The notes, the header cells and the table's alternative text.
        "/S/Note",
        "/ID(footnote-1)",
        "/ID(endnote-1)",
        "/ID(t1-h1)",
        "/Scope/Column",
        "/Headers[(t1-h1)]",
        "/Summary(Results. Quarterly results by region)",
        // The figures' own alternative text.
        "/Alt(A blue rectangle used as a figure)",
        "/Alt(A floating blue rectangle)",
        "<pdfuaid:part>1</pdfuaid:part>",
    ] {
        assert!(text.contains(key), "{key} is written");
    }
    // The decorative picture is an artifact.
    assert_eq!(text.matches("/S/Figure").count(), 2);
    assert_eq!(text.matches("/Subtype/Link").count(), 4, "the report, the table, Table results and its page");
}

#[test]
fn notes_referencing_notes_are_walked_without_recursion() {
    let n = 20_000;
    let notes: String = (1..=n)
        .map(|i| {
            format!(
                r#"<w:footnote w:id="{i}"><w:p><w:r><w:t>Note {i}</w:t></w:r><w:r><w:footnoteReference w:id="{}"/></w:r></w:p></w:footnote>"#,
                i + 1
            )
        })
        .collect();
    let body = r#"<w:p><w:r><w:t>Body</w:t></w:r><w:r><w:footnoteReference w:id="1"/></w:r></w:p>"#;
    let bytes = package(body, &[("footnotes.xml", "footnotes", format!("<w:footnotes {W}>{notes}</w:footnotes>"))]);
    let doc = crate::docx::read(&bytes).expect("reads");
    let tags = Tags::of(&doc, "en-US");
    let note = |n: &Node| matches!(n, Node::Element(e) if e.kind == Kind::Note);
    let notes_after_body = tags.root.iter().filter(|n| note(n)).count();
    assert_eq!(notes_after_body, n - 1, "the first note follows its reference; each next one follows the body");
}

#[test]
fn headings_by_style_header_cells_and_link_targets() {
    let styles = format!(
        r#"<w:styles {W}><w:style w:type="paragraph" w:styleId="Kop3"><w:name w:val="heading 3"/></w:style><w:style w:type="paragraph" w:styleId="Title"><w:name w:val="Title"/></w:style></w:styles>"#
    );
    let cell = |id: &str, headers: &[&str], text: &str| {
        let refs: String = headers.iter().map(|h| format!(r#"<w:header w:val="{h}"/>"#)).collect();
        format!(r#"<w:tc w:id="{id}"><w:tcPr><w:headers>{refs}</w:headers></w:tcPr><w:p><w:r><w:t>{text}</w:t></w:r></w:p></w:tc>"#)
    };
    let body = format!(
        r#"<w:p><w:pPr><w:pStyle w:val="Kop3"/></w:pPr><w:r><w:t>By style</w:t></w:r></w:p><w:tbl><w:tblGrid><w:gridCol w:w="2000"/><w:gridCol w:w="2000"/></w:tblGrid><w:tr>{}{}</w:tr></w:tbl>"#,
        cell("north", &[], "North"),
        cell("v", &["north"], "10")
    );
    let bytes = package(&body, &[("styles.xml", "styles", styles)]);
    let doc = crate::docx::read(&bytes).expect("reads");
    let crate::docx::Block::Para(p) = &doc.sections[0].blocks[0] else { panic!("a paragraph") };
    assert_eq!(crate::services::a11y::roles::heading_level(&doc, p), Some(3), "a built-in heading style by its name");
    let crate::docx::Block::Table(t) = &doc.sections[0].blocks[1] else { panic!("a table") };
    use crate::services::a11y::roles::HeaderScope;
    assert_eq!(crate::services::a11y::roles::header_scopes(t, 0), [Some(HeaderScope::Row), None], "listed by its row: a row header");
    let text = String::from_utf8_lossy(&export(&bytes)).into_owned();
    assert!(text.contains("/S/H1"), "the first heading tag is H1 whatever its level");
    assert!(text.contains("/Scope/Row") && text.contains("/Headers[(t1-north)]") && text.contains("/ID(t1-north)"));

    let target = |code: &str| link_target(&fields::parse(code));
    assert_eq!(target(r#"HYPERLINK "https://example.com" \l "part""#), Some(Target::Uri("https://example.com#part".into())));
    assert_eq!(target(r#"HYPERLINK \l "results""#), Some(Target::Bookmark("results".into())));
    assert_eq!(target(r#"HYPERLINK "report.docx""#), Some(Target::Uri("report.docx".into())));
    assert_eq!(target(r#"HYPERLINK "javascript:alert(1)""#), None, "only the web's, mail's and phone's schemes");
    assert_eq!(target(r#"HYPERLINK "file:///etc/passwd""#), None);
    assert_eq!(target(r#"REF results \h"#), Some(Target::Bookmark("results".into())));
    assert_eq!(target(r#"PAGEREF results"#), None, "a cross-reference links only with \\h");
}

#[test]
fn cell_ids_and_headers_are_written_back() {
    let mut d = crate::docx::read_editable(&fixture()).expect("reads");
    let t = d.sections[0].blocks.iter_mut().find_map(|b| if let crate::docx::Block::Table(t) = b { Some(t) } else { None }).expect("the table");
    // One cell's headers changed as read, another's properties written from the model.
    t.rows[1].cells[0].ids = Some(Rc::new(CellIds { id: Some("n1".into()), headers: vec!["h1".into(), "h2".into()] }));
    t.rows[2].cells[0].direct = Raw::default();
    let again = crate::docx::read_editable(&crate::docx::write::save(&d).expect("saves")).expect("reads again");
    let t = again.sections[0].blocks.iter().find_map(|b| if let crate::docx::Block::Table(t) = b { Some(t) } else { None }).expect("the table");
    let ids = |r: usize| t.rows[r].cells[0].ids.as_deref().cloned().unwrap_or_default();
    assert_eq!(ids(0), CellIds { id: Some("h1".into()), headers: Vec::new() });
    assert_eq!(ids(1), CellIds { id: Some("n1".into()), headers: vec!["h1".into(), "h2".into()] });
    assert_eq!(ids(2), CellIds { id: None, headers: vec!["h1".into()] });
}

#[test]
fn a_rewritten_row_keeps_header_cells_past_the_read_bound() {
    let n = crate::docx::MAX_CELL_HEADERS + 76;
    let refs: String = (0..n).map(|i| format!(r#"<w:header w:val="h{i}"/>"#)).collect();
    let body = format!(
        r#"<w:tbl><w:tblGrid><w:gridCol w:w="2000"/></w:tblGrid><w:tr><w:tc w:id="c"><w:tcPr><w:tcW w:w="2000" w:type="dxa"/><w:headers>{refs}</w:headers></w:tcPr><w:p><w:r><w:t>Cell</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p/>"#
    );
    let mut d = crate::docx::read_editable(&package(&body, &[])).expect("reads");
    let crate::docx::Block::Table(t) = &mut d.sections[0].blocks[0] else { panic!("a table") };
    assert_eq!(t.rows[0].cells[0].ids.as_ref().map(|i| i.headers.len()), Some(crate::docx::MAX_CELL_HEADERS), "reading is bounded");
    // A cell more: the row is written again, its first cell from its properties as read.
    let extra = crate::docx::Cell { ids: None, ..t.rows[0].cells[0].clone() };
    t.rows[0].cells.push(extra);
    let saved = crate::docx::write::save(&d).expect("saves");
    let xml = crate::docx::package::read(&saved, &[], "word/document.xml").expect("reads").expect("the main part");
    let first = &xml[xml.find("<w:tc").expect("a cell")..xml.find("</w:tc>").expect("its end")];
    assert_eq!(first.matches("<w:header ").count(), n, "every header the source lists is kept");
    assert!(first.starts_with(r#"<w:tc w:id="c">"#), "{}", &first[..40]);
}

/// An inline picture in turned text (a text box whose text runs vertically) is its figure, found by the identity it
/// is drawn with.
#[test]
fn an_inline_picture_in_turned_text_is_a_figure() {
    use crate::layout::tests::shapes::{anchor, sp};
    let ns = r#"xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape""#;
    let picture = format!(
        r#"<w:r><w:drawing {ns}><wp:inline distT="0" distB="0" distL="0" distR="0"><wp:extent cx="254000" cy="127000"/><wp:docPr id="9" name="P" descr="A turned red box"/><a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape">{}</a:graphicData></a:graphic></wp:inline></w:drawing></w:r>"#,
        sp(0, 0, 20, 10, "rect", "", "FF0000")
    );
    let text_box = format!(
        r#"<wps:wsp><wps:cNvSpPr txBox="1"/><wps:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="1270000" cy="2540000"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></wps:spPr><wps:txbx><w:txbxContent><w:p><w:r><w:t>Up</w:t></w:r>{picture}<w:r><w:t>down</w:t></w:r></w:p></w:txbxContent></wps:txbx><wps:bodyPr vert="vert"/></wps:wsp>"#
    );
    let body = format!(r#"<w:p><w:r><w:t>Body</w:t></w:r>{}</w:p>"#, anchor(100, 100, 100, 200, 1, false, 2, &text_box))
        .replace("<w:drawing>", &format!("<w:drawing {ns}>"));
    let pdf = export(&package(&body, &[]));
    let text = String::from_utf8_lossy(&pdf);
    assert!(text.contains("/Alt(A turned red box)"), "the figure is tagged");
    let doc = crate::docx::read(&package(&body, &[])).expect("reads");
    let snap = crate::layout::layout(&doc, &bundled_fonts());
    let turned = crate::layout::traverse::paint_items(&snap.pages()[0]).into_iter().any(|i| {
        matches!(&i, crate::layout::fragment::Item::Turned(t) if t.items.iter().any(|i| matches!(i, crate::layout::fragment::Item::Shape(s) if s.of != crate::layout::fragment::PictureOf::None)))
    });
    assert!(turned, "the picture is drawn turned");
}

/// List numbers are labels of numbered lists, text in another language is a span in it, and a picture without
/// alternative text that isn't decorative is a figure without a description, with a warning.
#[test]
fn list_numbers_languages_and_pictures_without_alt_text() {
    let fonts = bundled_fonts();
    let read = |path: &str| crate::docx::read(&std::fs::read(root().join(path)).expect("the fixture")).expect("reads");
    let doc = read("fixtures/editor/FX-A11Y-STRUCTURE.docx");
    let (pdf, _) = super::super::export::export(&doc, &crate::layout::layout(&doc, &fonts), "", &PdfOptions::default(), None).expect("exports");
    let text = String::from_utf8_lossy(&pdf);
    for key in ["/S/Lbl", "/ListNumbering/Decimal", "/ListNumbering/Disc", "/S/Span", "/Lang(fr-FR)", "/Lang(he-IL)"] {
        assert!(text.contains(key), "{key} is written");
    }
    let doc = read("fixtures/objects.docx");
    let (pdf, warnings) =
        super::super::export::export(&doc, &crate::layout::layout(&doc, &fonts), "", &PdfOptions::default(), None).expect("exports");
    let missing = warnings.iter().find(|w| w.code == crate::api::warning::WarningCode::PdfFigureWithoutAlt).expect("a warning");
    let text = String::from_utf8_lossy(&pdf);
    let figures = text.matches("/S/Figure").count();
    assert!(figures >= missing.count as usize && missing.count > 0, "{figures} figures, {} without alternative text", missing.count);
}
