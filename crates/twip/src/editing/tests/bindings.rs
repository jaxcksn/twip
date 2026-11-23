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

fn json_cmd(e: &mut Editor, c: serde_json::Value) -> serde_json::Value {
    let msg = serde_json::json!({ "type": "command", "command": c, "seq": e.seq_applied() + 1 });
    serde_json::from_str(&e.apply_json(&msg.to_string(), fonts())).expect("a reply")
}

fn refused(r: &serde_json::Value) -> bool {
    r.get("error").is_some() || r["answer"].get("error").is_some()
}

fn ok(r: &serde_json::Value) {
    assert!(!refused(r), "{r}");
}

fn case_xml(e: &Editor) -> String {
    e.document().data_parts[case(e.document())].xml.clone()
}

/// A document whose paragraph holds the controls `sdts` (each a `w:sdt`) after "Before ".
fn with_controls(sdts: &str) -> Editor {
    let body = format!("<w:p><w:r><w:t xml:space=\"preserve\">Before </w:t></w:r>{sdts}</w:p><w:p><w:r><w:t>Other</w:t></w:r></w:p>");
    Editor::open(&super::blank_body(&body), fonts()).expect("opens")
}

fn sdt(alias: &str, id: u32, props: &str, content: &str) -> String {
    format!("<w:sdt><w:sdtPr><w:alias w:val=\"{alias}\"/><w:id w:val=\"{id}\"/>{props}</w:sdtPr><w:sdtContent>{content}</w:sdtContent></w:sdt>")
}

/// Binds the controls `aliases` to `node` of the case data, which becomes `xml`.
fn bind_to(e: &mut Editor, aliases: &[&str], node: &str, xml: &str) {
    let binding = serde_json::json!({ "xpath": format!("/c:case[1]/c:{node}[1]"), "prefixMappings": "xmlns:c='urn:case'" });
    for a in aliases {
        let id = control(e, a).1;
        ok(&json_cmd(e, serde_json::json!({ "type": "setControl", "id": id, "binding": binding })));
    }
    ok(&json_cmd(e, serde_json::json!({ "type": "writeDataPart", "namespace": "urn:case", "xml": xml })));
}

#[test]
fn inserting_a_bound_control_shows_its_node_and_leaves_it() {
    let mut e = open("FX-MERGE-CONTROLS.docx");
    bind_to(&mut e, &["Recipient name"], "name", "<case xmlns=\"urn:case\"><name>Ada</name></case>");
    assert!(texts(&e).iter().any(|t| t.contains("Ada")));
    let bind = serde_json::json!({ "xpath": "/c:case[1]/c:name[1]", "prefixMappings": "xmlns:c='urn:case'" });
    ok(&json_cmd(&mut e, serde_json::json!({ "type": "insertControl", "kind": "plainText", "binding": bind })));
    assert!(case_xml(&e).contains("<name>Ada</name>"), "node kept: {}", case_xml(&e));
    assert_eq!(texts(&e).iter().map(|t| t.matches("Ada").count()).sum::<usize>(), 2, "both controls show Ada: {:?}", texts(&e));
}

#[test]
fn a_new_placeholder_on_a_bound_control_leaves_its_node() {
    let mut e = with_controls(&sdt("C", 5, "<w:showingPlcHdr/><w:text/>", "<w:r><w:t>Type here</w:t></w:r>"));
    ok(&json_cmd(
        &mut e,
        serde_json::json!({ "type": "writeDataPart", "namespace": "urn:case", "xml": "<case xmlns=\"urn:case\"><name>Ada</name></case>" }),
    ));
    let id = control(&e, "C").1;
    let binding = serde_json::json!({ "xpath": "/c:case[1]/c:name[1]", "prefixMappings": "xmlns:c='urn:case'" });
    ok(&json_cmd(&mut e, serde_json::json!({ "type": "setControl", "id": id, "binding": binding })));
    ok(&json_cmd(&mut e, serde_json::json!({ "type": "setControl", "id": id, "placeholder": "Name" })));
    assert!(case_xml(&e).contains("<name>Ada</name>"), "node kept: {}", case_xml(&e));
    assert!(texts(&e)[0].contains("Ada"), "the control shows its node: {:?}", texts(&e));
}

#[test]
fn typing_in_one_of_two_controls_bound_to_a_node_writes_it() {
    let mut e =
        with_controls(&format!("{}{}", sdt("A", 5, "<w:text/>", "<w:r><w:t>x</w:t></w:r>"), sdt("B", 6, "<w:text/>", "<w:r><w:t>y</w:t></w:r>")));
    bind_to(&mut e, &["A", "B"], "name", "<case xmlns=\"urn:case\"><name>Ada</name></case>");
    assert_eq!(texts(&e)[0], "Before AdaAda");
    e.selection = super::super::selection::Selection::Caret(super::super::position::Position::text(
        super::super::position::StoryId::Body,
        super::para_id(&e, 0),
        crate::layout::Unit(8),
    ));
    run(&mut e, Command::InsertText("x".into()));
    assert!(case_xml(&e).contains("<name>Axda</name>"), "{}", case_xml(&e));
    assert_eq!(texts(&e)[0], "Before AxdaAxda", "the other control follows");
}

#[test]
fn filling_a_bound_date_writes_its_date() {
    let date = r#"<w:date w:fullDate="2026-01-01T00:00:00Z"><w:dateFormat w:val="M/d/yyyy"/><w:lid w:val="en-US"/><w:storeMappedDataAs w:val="dateTime"/><w:calendar w:val="gregorian"/></w:date>"#;
    let mut e = with_controls(&sdt("C", 5, date, "<w:r><w:t>1/1/2026</w:t></w:r>"));
    bind_to(&mut e, &["C"], "d", "<case xmlns=\"urn:case\"><d>2026-01-01T00:00:00Z</d></case>");
    let (c, id) = control(&e, "C");
    run(&mut e, fill(&id, "3/5/2025"));
    assert!(case_xml(&e).contains("<d>2025-03-05T00:00:00Z</d>"), "{}", case_xml(&e));
    let full = e.document().controls[&c].props.kind_xml.clone().unwrap_or_default();
    assert!(full.contains(r#"w:fullDate="2025-03-05T00:00:00Z""#), "{full}");
    let again = Editor::open(&crate::docx::write::save(e.document()).expect("saves"), fonts()).expect("reopens");
    assert!(texts(&again)[0].contains("3/5/2025"), "{:?}", texts(&again));
    run(&mut e, fill(&id, "not a date"));
    assert!(case_xml(&e).contains("<d>not a date</d>"), "text that isn't a date is stored as it is: {}", case_xml(&e));
    assert!(!e.document().controls[&c].props.kind_xml.clone().unwrap_or_default().contains("fullDate"), "and has no full date");
}

#[test]
fn typing_beside_a_date_keeps_its_date() {
    let date = r#"<w:date w:fullDate="2025-03-05T00:00:00Z"><w:dateFormat w:val="d MMMM yyyy"/><w:lid w:val="fr-FR"/><w:storeMappedDataAs w:val="dateTime"/><w:calendar w:val="gregorian"/></w:date>"#;
    let mut e = with_controls(&sdt("C", 5, date, "<w:r><w:t>5 mars 2025</w:t></w:r>"));
    let c = control(&e, "C").0;
    for at in [0, 19] {
        e.selection = super::super::selection::Selection::Caret(super::super::position::Position::text(
            super::super::position::StoryId::Body,
            super::para_id(&e, 0),
            crate::layout::Unit(at),
        ));
        run(&mut e, Command::InsertText("x".into()));
        let xml = e.document().controls[&c].props.kind_xml.clone().unwrap_or_default();
        assert!(xml.contains(r#"w:fullDate="2025-03-05T00:00:00Z""#), "typing at {at}, outside it, leaves its date: {xml}");
    }
    assert_eq!(texts(&e)[0], "xBefore 5 mars 2025x");
}

#[test]
fn typing_in_a_bound_controls_placeholder_writes_its_node() {
    let mut e = with_controls(&sdt("C", 5, "<w:showingPlcHdr/><w:text/>", "<w:r><w:t>Type here</w:t></w:r>"));
    bind_to(&mut e, &["C"], "name", "<case xmlns=\"urn:case\"><name/></case>");
    e.selection = super::super::selection::Selection::Caret(super::super::position::Position::text(
        super::super::position::StoryId::Body,
        super::para_id(&e, 0),
        crate::layout::Unit(10),
    ));
    run(&mut e, Command::InsertText("Ada".into()));
    assert_eq!(texts(&e)[0], "Before Ada", "the placeholder is replaced");
    assert!(!e.document().controls[&control(&e, "C").0].props.showing_placeholder);
    assert!(case_xml(&e).contains("<name>Ada</name>"), "{}", case_xml(&e));
}

#[test]
fn writing_a_data_part_matches_its_item_id_as_a_guid() {
    let mut e = open("FX-MERGE-CONTROLS.docx");
    let xml = |v: &str| format!("<case xmlns=\"urn:case\"><name>{v}</name></case>");
    ok(&json_cmd(&mut e, serde_json::json!({ "type": "writeDataPart", "namespace": "urn:case", "xml": xml("a") })));
    let existing = e.document().data_parts[case(e.document())].item_id.clone();
    let n = e.document().data_parts.len();
    let variant = format!("{{{}}}", existing.trim_matches(['{', '}']).to_ascii_lowercase());
    ok(&json_cmd(&mut e, serde_json::json!({ "type": "writeDataPart", "namespace": "urn:case", "itemId": variant, "xml": xml("b") })));
    assert_eq!(e.document().data_parts.len(), n, "the same item, case aside, is updated, not duplicated");
    assert!(case_xml(&e).contains("<name>b</name>"));
}

#[test]
fn an_item_id_two_data_parts_answer_to_is_refused() {
    let mut e = open("FX-MERGE-CONTROLS.docx");
    let xml = |v: &str| format!("<case xmlns=\"urn:case\"><name>{v}</name></case>");
    ok(&json_cmd(&mut e, serde_json::json!({ "type": "writeDataPart", "namespace": "urn:case", "xml": xml("a") })));
    let first = e.document().data_parts[case(e.document())].clone();
    let variant = format!("{{{}}}", first.item_id.trim_matches(['{', '}']).to_ascii_lowercase());
    e.doc.data_parts.push(crate::docx::DataPart { item_id: variant.clone(), ..first });
    let r = json_cmd(&mut e, serde_json::json!({ "type": "writeDataPart", "namespace": "urn:case", "itemId": variant, "xml": xml("b") }));
    assert!(refused(&r), "{r}");
    assert!(e.document().data_parts.iter().all(|d| !d.xml.contains("<name>b</name>")), "neither part changed");
}

#[test]
fn a_new_data_parts_item_id_is_a_guid() {
    let mut e = open("FX-MERGE-CONTROLS.docx");
    let n = e.document().data_parts.len();
    for bad in ["a\"b", "orders", "{1234}"] {
        let r = json_cmd(&mut e, serde_json::json!({ "type": "writeDataPart", "namespace": "urn:q", "itemId": bad, "xml": "<q xmlns=\"urn:q\"/>" }));
        assert!(refused(&r), "{bad}: {r}");
    }
    assert_eq!(e.document().data_parts.len(), n);
    let guid = "8a2d6c3e-1f4b-4c5d-9e6f-0a1b2c3d4e5f";
    ok(&json_cmd(&mut e, serde_json::json!({ "type": "writeDataPart", "namespace": "urn:q", "itemId": guid, "xml": "<q xmlns=\"urn:q\"/>" })));
    let saved = crate::docx::write::save(e.document()).expect("saves");
    let again = crate::docx::read_editable(&saved).expect("reads back");
    assert!(
        again.data_parts.iter().any(|d| crate::docx::binding::same_item(&d.item_id, guid)),
        "{:?}",
        again.data_parts.iter().map(|d| &d.item_id).collect::<Vec<_>>()
    );
}
