//! Content controls bound to custom XML data, and temporary controls

use super::super::Editor;
use super::super::commands::{Command, Outcome};
use super::{fonts, open, texts};
use crate::api::command::{FillControl, SetControl, WriteDataPart};
use crate::api::value::DataBinding;
use crate::docx::ControlId;

fn control(e: &Editor, alias: &str) -> (ControlId, crate::api::value::ElementId) {
    let id = e.document().controls.iter().find(|(_, c)| c.props.alias.as_deref() == Some(alias)).map(|(k, _)| *k).expect(alias);
    (id, super::super::merge::api_id(crate::docx::ElementId::Control(id)))
}

fn run(e: &mut Editor, c: Command) {
    let out = e.execute(c.clone(), fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{c:?}: {out:?}");
}

fn set(id: &crate::api::value::ElementId) -> SetControl {
    serde_json::from_value(serde_json::json!({ "id": id })).expect("a change")
}

fn fill(id: &crate::api::value::ElementId, text: &str) -> Command {
    Command::FillControl(FillControl { id: id.clone(), text: Some(text.into()), image: None, docx: None, building_block: None })
}

fn case(doc: &crate::docx::Document) -> usize {
    doc.data_parts.iter().position(|d| d.xml.contains("urn:case")).expect("the case data")
}

fn line(e: &Editor, start: &str) -> String {
    texts(e).into_iter().find(|t| t.starts_with(start)).unwrap_or_else(|| panic!("no {start}"))
}

#[test]
fn bound_controls_show_their_node_and_write_it_back() {
    let mut e = open("FX-MERGE-CONTROLS.docx");
    let ns = "urn:case";
    let bind = DataBinding { xpath: "/c:case[1]/c:name[1]".into(), store_item_id: None, prefix_mappings: Some(format!("xmlns:c='{ns}'")) };
    let ((_, name), (_, address)) = (control(&e, "Recipient name"), control(&e, "Address"));
    for id in [&name, &address] {
        run(&mut e, Command::SetControl(SetControl { binding: Some(Some(bind.clone())), ..set(id) }));
    }
    run(
        &mut e,
        Command::WriteDataPart(WriteDataPart { namespace: ns.into(), item_id: None, xml: format!("<case xmlns=\"{ns}\"><name>Ada</name></case>") }),
    );
    assert!(texts(&e).iter().filter(|t| t.contains("Ada")).count() >= 2, "both show the node: {:?}", texts(&e));
    run(&mut e, fill(&name, "Grace & Co"));
    let xml = &e.document().data_parts[case(e.document())].xml;
    assert!(xml.contains("<name>Grace &amp; Co</name>"), "{xml}");
    assert!(line(&e, "Address").contains("Grace & Co"), "the other control follows");
    run(&mut e, Command::Undo);
    assert!(e.document().data_parts[case(e.document())].xml.contains("<name>Ada</name>") && line(&e, "Address").contains("Ada"));
    // Saved with other data, the controls show it when opened.
    let mut doc = e.document().clone();
    let k = case(&doc);
    doc.data_parts[k].xml = format!("<case xmlns=\"{ns}\"><name>Lin</name></case>");
    doc.data_parts[k].changed = true;
    let again = Editor::open(&crate::docx::write::save(&doc).expect("saves"), fonts()).expect("reopens");
    assert!(line(&again, "Address").contains("Lin"), "{:?}", texts(&again));
}

#[test]
fn a_temporary_control_goes_when_its_content_is_edited() {
    let mut e = open("FX-MERGE-CONTROLS.docx");
    let (c, name) = control(&e, "Recipient name");
    run(&mut e, Command::SetControl(SetControl { temporary: Some(true), ..set(&name) }));
    run(&mut e, fill(&name, "Grace"));
    assert!(!e.document().controls.contains_key(&c), "the control went");
    assert!(texts(&e).iter().any(|t| t.contains("Grace")), "its content stays");
    run(&mut e, Command::Undo);
    assert!(e.document().controls.contains_key(&c), "undo brings it back");
}

#[test]
fn a_gallery_control_takes_a_building_block_of_its_category() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/FX-SUBTEMPLATES.docx");
    let gallery = r#"<w:sdt><w:sdtPr><w:alias w:val="Pick"/><w:id w:val="9"/><w:docPartList><w:docPartGallery w:val="Custom 1"/><w:docPartCategory w:val="Sub-templates"/></w:docPartList></w:sdtPr><w:sdtContent><w:p><w:r><w:t>Choose</w:t></w:r></w:p></w:sdtContent></w:sdt>"#;
    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{gallery}<w:p><w:r><w:t>After</w:t></w:r></w:p><w:sectPr/></w:body></w:document>"#
    );
    let bytes = crate::docx::write::repackage(
        &std::fs::read(path).expect("fixture"),
        &[("word/document.xml".into(), xml.into_bytes())],
        zip::CompressionMethod::Deflated,
    )
    .expect("repackages");
    let mut e = Editor::open(&bytes, fonts()).expect("opens");
    let (c, id) = control(&e, "Pick");
    let category = e.document().controls[&c].props.gallery().expect("a gallery");
    let names: Vec<String> = e.gallery_entries(category.as_deref()).map(|b| b.name.clone()).collect();
    assert!(names.iter().any(|n| n == "Violation"), "{names:?}");
    run(&mut e, Command::FillControl(FillControl { id: id.clone(), text: None, image: None, docx: None, building_block: Some("Violation".into()) }));
    assert!(texts(&e).iter().any(|t| t == "Violation notice") && !texts(&e).iter().any(|t| t == "Choose"), "{:?}", texts(&e));
    let none = FillControl { id, text: None, image: None, docx: None, building_block: Some("Nope".into()) };
    assert!(matches!(e.execute(Command::FillControl(none), fonts()), Outcome::Refused(_)));
    let saved = crate::docx::write::save(e.document()).expect("saves");
    let again = Editor::open(&saved, fonts()).expect("reopens");
    assert!(texts(&again).iter().any(|t| t == "Violation notice"));
}

#[test]
fn a_bound_control_inside_another_opens_and_shows_its_node() {
    let mut e = open("FX-MERGE-CONTROLS.docx");
    let ns = "urn:case";
    run(
        &mut e,
        Command::WriteDataPart(WriteDataPart { namespace: ns.into(), item_id: None, xml: format!("<case xmlns=\"{ns}\"><name>Ada</name></case>") }),
    );
    let saved = crate::docx::write::save(e.document()).expect("saves");
    let bind = r#"<w:dataBinding w:prefixMappings="xmlns:c='urn:case'" w:xpath="/c:case[1]/c:name[1]"/>"#;
    let inner = r#"<w:r><w:t>b</w:t></w:r><w:r><w:rPr><w:b/></w:rPr><w:t>c</w:t></w:r><w:r><w:rPr><w:i/></w:rPr><w:t>d</w:t></w:r>"#;
    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:sdt><w:sdtPr><w:alias w:val="Outer"/><w:id w:val="11"/>{bind}<w:richText/></w:sdtPr><w:sdtContent><w:r><w:t>a</w:t></w:r><w:sdt><w:sdtPr><w:alias w:val="Inner"/><w:id w:val="12"/>{bind}<w:text/></w:sdtPr><w:sdtContent>{inner}</w:sdtContent></w:sdt><w:r><w:t>e</w:t></w:r></w:sdtContent></w:sdt></w:p><w:sectPr/></w:body></w:document>"#
    );
    let bytes = crate::docx::write::repackage(&saved, &[("word/document.xml".into(), xml.into_bytes())], zip::CompressionMethod::Deflated)
        .expect("repackages");
    let e = Editor::open(&bytes, fonts()).expect("opens");
    assert_eq!(texts(&e), ["aAdae"], "the inner control shows its node; the one holding it keeps its text");
}
