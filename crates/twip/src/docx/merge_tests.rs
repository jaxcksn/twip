//! reading and writing: fields as markers, content controls with their properties at every level,
//! repeating regions, data parts, and malformed input, on the FX-MERGE fixtures.

use super::fields::{code_text, parse, spans};
use super::*;

fn fixture(name: &str) -> Vec<u8> {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor").join(name);
    std::fs::read(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

fn paras(blocks: &[Block], out: &mut Vec<Paragraph>) {
    for b in blocks {
        match b {
            Block::Para(p) => out.push(p.clone()),
            Block::Table(t) => t.rows.iter().flat_map(|r| &r.cells).for_each(|c| paras(&c.blocks, out)),
        }
    }
}

fn all_paras(d: &Document) -> Vec<Paragraph> {
    let mut out = Vec::new();
    d.sections.iter().for_each(|s| paras(&s.blocks, &mut out));
    let mut stories: Vec<&String> = d.stories.keys().collect();
    stories.sort();
    for s in stories {
        paras(&d.stories[s], &mut out);
    }
    for n in d.footnotes.values() {
        paras(n, &mut out);
    }
    // Text boxes.
    let floats: Vec<Paragraph> = out
        .iter()
        .flat_map(|p| &p.inlines)
        .filter_map(|i| if let Inline::Float(f, _) = i { f.text_box.as_ref() } else { None })
        .flat_map(|b| {
            let mut v = Vec::new();
            paras(&b.blocks, &mut v);
            v
        })
        .collect();
    out.extend(floats);
    out
}

/// Every field: (own code trimmed, result text, lock, dirty).
fn fields(d: &Document) -> Vec<(String, String, bool, bool)> {
    all_paras(d)
        .iter()
        .flat_map(|p| {
            spans(&p.inlines).into_iter().map(|s| {
                let Inline::FieldBegin { lock, dirty, .. } = &p.inlines[s.begin] else { unreachable!("a span begins at a begin marker") };
                (code_text(&p.inlines, s).trim().to_string(), super::fields::result_text(&p.inlines, s), *lock, *dirty)
            })
        })
        .collect()
}

#[test]
fn fields_read_as_markers() {
    let d = read_editable(&fixture("FX-MERGE-FIELDS.docx")).expect("reads");
    let f = fields(&d);
    let find = |code: &str| f.iter().find(|x| x.0 == code).unwrap_or_else(|| panic!("no field {code}: {f:?}"));
    assert_eq!(find(r#"MERGEFIELD Statement.Cost_Amount \# "$,0.00""#).1, "«Cost_Amount»");
    assert_eq!(find(r#"MERGEFIELD Shipment.Shipment_Number \* Upper \* MERGEFORMAT"#).1, "«Shipment_Number»");
    // The IF's own code leaves its nested fields out; they are fields of their own.
    assert_eq!(find(r#"IF  = "Paid" "" """#).1, "«Paid_Date»");
    find("MERGEFIELD Statement.Status");
    find(r#"MERGEFIELD Statement.Paid_Date \@ "M/d/yyyy""#);
    assert_eq!(find("MERGEFIELD Recipient.Last_Name").1, "«Last_Name»", "the simple field");
    assert!(find("MERGEFIELD Store.Name").2, "locked");
    assert!(find("MERGEFIELD Store.City").3, "dirty");
    assert_eq!(find("MERGEFIELD Statement.Manager").1, "", "an empty result");
    // Header, footer, footnote, text box and the cell at depth 3.
    for code in ["MERGEFIELD Recipient.First_Name", "MERGEFIELD Manager.Badge", "MERGEFIELD Detail.Code", "PAGE", "NUMPAGES"] {
        find(code);
    }
    assert_eq!(f.iter().filter(|x| x.0.contains("Calc.Old_Name")).count(), 40);
    // Kinds: evaluated fields know theirs; merge fields have none.
    let p = all_paras(&d);
    let kinds: Vec<Option<FieldKind>> = p
        .iter()
        .flat_map(|p| p.inlines.iter())
        .filter_map(|i| if let Inline::FieldBegin { kind, .. } = i { Some(kind.clone()) } else { None })
        .collect();
    assert!(kinds.contains(&Some(FieldKind::Page)) && kinds.contains(&Some(FieldKind::NumPages)) && kinds.contains(&Some(FieldKind::If)));
    assert_eq!(parse(&find("MERGEFIELD Statement.Cost_Amount \\# \"$,0.00\"").0).args, ["Statement.Cost_Amount"]);
}

#[test]
fn controls_read_with_their_properties() {
    let d = read_editable(&fixture("FX-MERGE-CONTROLS.docx")).expect("reads");
    let by_tag = |tag: &str| d.controls.values().find(|c| c.props.tag.as_deref() == Some(tag)).unwrap_or_else(|| panic!("no control {tag}"));
    assert_eq!(by_tag("Recipient/Name").props.kind, ControlKind::PlainText { multi_line: false });
    assert_eq!(by_tag("Recipient/Name").props.alias.as_deref(), Some("Recipient name"));
    assert_eq!(by_tag("Recipient/Address").props.kind, ControlKind::PlainText { multi_line: true });
    assert_eq!(by_tag("Content:Complaint").props.kind, ControlKind::RichText);
    assert_eq!(by_tag("Statement/Date").props.kind, ControlKind::Date { format: "M/d/yyyy".into(), lid: Some("en-US".into()) });
    assert!(matches!(by_tag("Check:Done").props.kind, ControlKind::Checkbox { .. }));
    assert_eq!(by_tag("Barcode:7").props.kind, ControlKind::Picture);
    assert_eq!(by_tag("Statement/Status").props.kind, ControlKind::DropDown);
    assert_eq!(by_tag("Group/One").props.kind, ControlKind::Group);
    assert_eq!(by_tag("Lock/Control").props.lock, Lock::ControlLocked);
    assert_eq!(by_tag("Lock/Content").props.lock, Lock::ContentLocked);
    assert_eq!(by_tag("Lock/Both").props.lock, Lock::Both);
    let ph = &by_tag("Recipient/Middle").props;
    assert!(ph.showing_placeholder && ph.placeholder.is_some());
    assert!(by_tag("Store/Bound").props.binding.as_deref().is_some_and(|b| b.contains("storeItemID")));
    // Membership: inline markers, block lists, row and cell lists.
    let p = all_paras(&d);
    let id = |tag: &str| *d.controls.iter().find(|(_, c)| c.props.tag.as_deref() == Some(tag)).map(|(id, _)| id).unwrap_or_else(|| panic!("{tag}"));
    let name = id("Recipient/Name");
    assert!(p.iter().any(|p| p.inlines.contains(&Inline::ControlStart(name)) && p.inlines.contains(&Inline::ControlEnd(name))));
    let complaint = id("Content:Complaint");
    assert_eq!(p.iter().filter(|p| p.controls.contains(&complaint)).count(), 2, "the rich-text control around two paragraphs");
    let t = d.sections[0].blocks.iter().find_map(|b| if let Block::Table(t) = b { Some(t) } else { None }).expect("a table");
    assert_eq!(t.rows[0].cells[1].controls, [id("Cell/One")]);
    assert_eq!(t.rows[1].controls, [id("Row/One")]);
    // The customXml wrapper is a control of kind Other.
    assert!(d.controls.values().any(|c| c.props.kind == ControlKind::Other && c.open.contains("customXml")));
}

#[test]
fn regions_read_as_repeating_sections() {
    let d = read_editable(&fixture("FX-MERGE-REGIONS.docx")).expect("reads");
    let id = |tag: &str| *d.controls.iter().find(|(_, c)| c.props.tag.as_deref() == Some(tag)).map(|(id, _)| id).unwrap_or_else(|| panic!("{tag}"));
    let kind = |id: &ControlId| d.controls[id].props.kind.clone();
    let tables: Vec<&Table> = d.sections[0].blocks.iter().filter_map(|b| if let Block::Table(t) = b { Some(t) } else { None }).collect();
    let charges = id("Statement.Details");
    assert_eq!(kind(&charges), ControlKind::RepeatingSection);
    for r in &tables[0].rows[1..] {
        assert_eq!(r.controls[0], charges);
        assert_eq!(kind(&r.controls[1]), ControlKind::RepeatingItem);
    }
    let Block::Table(inner) = &tables[0].rows[1].cells[1].blocks[1] else { panic!("the inner table") };
    assert!(inner.rows[1..].iter().all(|r| r.controls.first() == Some(&id("Detail.Counts"))));
    let parties = id("Statement.Holders");
    let ps: Vec<&Paragraph> = d.sections[0]
        .blocks
        .iter()
        .filter_map(|b| if let Block::Para(p) = b { Some(p) } else { None })
        .filter(|p| p.controls.first() == Some(&parties))
        .collect();
    assert_eq!(ps.len(), 2, "two paragraphs, one item");
    assert_eq!(kind(&ps[0].controls[1]), ControlKind::RepeatingItem);
    assert_eq!(ps[0].controls, ps[1].controls);
}

#[test]
fn data_parts_read_change_add_and_remove() {
    let bytes = fixture("FX-MERGE-DATA.docx");
    let mut d = read_editable(&bytes).expect("reads");
    assert_eq!(d.data_parts.len(), 2);
    let ns: Vec<Option<&str>> = d.data_parts.iter().map(|p| p.namespace.as_deref()).collect();
    assert!(ns.contains(&Some("urn:store:form-definition")) && ns.contains(&Some("urn:store:form-data")));
    assert!(d.data_parts.iter().all(|p| p.item_id.starts_with('{')));
    // Unedited: the package as read.
    assert_eq!(write::save(&d).expect("saves"), bytes);
    // Change the definition, remove the data part, add a new one.
    let def = d.data_parts.iter().position(|p| p.namespace.as_deref() == Some("urn:store:form-definition")).expect("definition");
    d.data_parts[def].xml = r#"<definition xmlns="urn:store:form-definition"><changed/></definition>"#.into();
    d.data_parts[def].changed = true;
    let data = d.data_parts.iter().position(|p| p.namespace.as_deref() == Some("urn:store:form-data")).expect("data");
    let gone = d.data_parts.remove(data);
    d.removed_data.extend(gone.src);
    d.data_parts.push(DataPart {
        namespace: Some("urn:test:new".into()),
        item_id: "{0000000A-0000-4000-8000-000000000001}".into(),
        xml: r#"<new xmlns="urn:test:new"/>"#.into(),
        src: None,
        changed: true,
    });
    let saved = write::save(&d).expect("saves");
    let again = read_editable(&saved).expect("reads again");
    let mut got: Vec<(Option<String>, String)> = again.data_parts.iter().map(|p| (p.namespace.clone(), p.xml.clone())).collect();
    got.sort();
    assert_eq!(got.len(), 2, "{got:?}");
    assert!(got.iter().any(|(n, x)| n.as_deref() == Some("urn:store:form-definition") && x.contains("<changed/>")));
    assert!(got.iter().any(|(n, _)| n.as_deref() == Some("urn:test:new")));
    assert!(again.data_parts.iter().any(|p| p.item_id == "{0000000A-0000-4000-8000-000000000001}"));
    // The bound control's binding is kept even though its part went (Word shows the saved content).
    assert!(again.controls.values().any(|c| c.props.binding.is_some()));
}

/// A new data part reads back after the ones there were, whatever the package's relationship ids look like
/// (`rIdCustomXml1` in the merge set's templates, numbered ids elsewhere).
#[test]
fn a_new_data_part_reads_back_last() {
    for name in ["FX-MERGE-DATA.docx", "merge-set/inv-6-calculated.docx"] {
        let mut d = read_editable(&fixture(name)).expect("reads");
        let before: Vec<String> = d.data_parts.iter().map(|p| p.item_id.clone()).collect();
        d.data_parts.push(DataPart {
            namespace: Some("urn:test:new".into()),
            item_id: "{0000000A-0000-4000-8000-000000000002}".into(),
            xml: r#"<new xmlns="urn:test:new"/>"#.into(),
            src: None,
            changed: true,
        });
        let again = read_editable(&write::save(&d).expect("saves")).expect("reads again");
        let got: Vec<String> = again.data_parts.iter().map(|p| p.item_id.clone()).collect();
        assert_eq!(got, [before, vec!["{0000000A-0000-4000-8000-000000000002}".to_string()]].concat(), "{name}");
    }
}

#[test]
fn malformed_merge_markup_reads() {
    let blank = blank::blank_docx();
    let doc = |body: &str| -> Vec<u8> {
        let xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{body}<w:sectPr/></w:body></w:document>"#
        );
        write::repackage(&blank, &[("word/document.xml".into(), xml.into_bytes())], zip::CompressionMethod::Deflated).expect("repackages")
    };
    // A field that never ends: its code doesn't show, the text after it does.
    let d = read_editable(&doc(r#"<w:p><w:r><w:t>a</w:t></w:r><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText>MERGEFIELD X</w:instrText></w:r></w:p><w:p><w:r><w:t>b</w:t></w:r></w:p>"#)).expect("reads");
    let texts: Vec<String> = all_paras(&d).iter().map(fields::text).collect();
    assert_eq!(texts, ["a", "b"]);
    // A content control without content, and an end marker with no begin.
    let d = read_editable(&doc(
        r#"<w:p><w:sdt><w:sdtPr><w:tag w:val="x"/></w:sdtPr></w:sdt><w:r><w:fldChar w:fldCharType="end"/></w:r><w:r><w:t>c</w:t></w:r></w:p>"#,
    ))
    .expect("reads");
    assert_eq!(all_paras(&d).iter().map(fields::text).collect::<Vec<_>>(), ["c"]);
}
