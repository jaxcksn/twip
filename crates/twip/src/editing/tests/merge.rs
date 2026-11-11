//! Elements and change lists

use serde_json::{Value, json};

use super::{fonts, open};
use crate::editing::Editor;

fn send(e: &mut Editor, seq: &mut u64, mut msg: Value) -> Value {
    *seq += 1;
    msg["seq"] = json!(*seq);
    let r: Value = serde_json::from_str(&e.apply_json(&msg.to_string(), fonts())).expect("a reply");
    assert!(r.get("error").is_none(), "{msg} -> {r}");
    r
}

fn elements(e: &mut Editor, seq: &mut u64, args: Value) -> Vec<Value> {
    let mut q = args;
    q["type"] = json!("query");
    q["q"] = json!("elements");
    send(e, seq, q)["answer"].as_array().cloned().expect("an element list")
}

#[test]
fn elements_list_in_document_order_with_parents() {
    let mut e = open("FX-MERGE-FIELDS.docx");
    let mut seq = 0;
    let all = elements(&mut e, &mut seq, json!({}));
    let codes: Vec<String> = all.iter().map(|x| x["code"].as_str().unwrap_or("").trim().to_string()).collect();
    let at = |code: &str| codes.iter().position(|c| c == code).unwrap_or_else(|| panic!("{code} not in {codes:?}"));
    assert!(at(r#"MERGEFIELD Statement.Cost_Amount \# "$,0.00""#) < at("MERGEFIELD Store.Name"));
    assert!(at("PAGE") < at("MERGEFIELD Recipient.First_Name"), "the footer before the footnote");
    assert!(at("MERGEFIELD Recipient.First_Name") < at("MERGEFIELD Manager.Badge"), "the footnote before the text box");
    let iff = at(r#"IF  = "Paid" "" """#);
    let status = at("MERGEFIELD Statement.Status");
    assert_eq!(status, iff + 1);
    assert_eq!(all[status]["parent"], all[iff]["id"]);
    assert_eq!(elements(&mut e, &mut seq, json!({ "match": { "in": "code", "text": "Calc.Old_Name" } })).len(), 40);
    assert_eq!(elements(&mut e, &mut seq, json!({ "kind": "plain_text" })).len(), 0);
    let ctl = open("FX-MERGE-CONTROLS.docx");
    let mut ctl = ctl;
    let pics = elements(&mut ctl, &mut seq, json!({ "kind": "picture" }));
    assert_eq!(pics.len(), 1);
    assert_eq!(pics[0]["tag"], "Barcode:7");
}

#[test]
fn change_lists_follow_edits_undo_and_redo() {
    let mut e = open("FX-TEXT.docx");
    let mut seq = 0;
    let r = send(&mut e, &mut seq, json!({ "type": "key", "code": "End", "mods": { "ctrl": true } }));
    assert!(r.get("elements").is_none(), "moving changes no element");
    let r = send(&mut e, &mut seq, json!({"type": "command", "command": {"type": "insertPageNumber", "format": "page"}}));
    let added = r["elements"]["added"].as_array().cloned().unwrap_or_default();
    assert_eq!(added.len(), 1, "{r}");
    let id = added[0]["id"].clone();
    assert_eq!(added[0]["code"], " PAGE ");
    let r = send(&mut e, &mut seq, json!({ "type": "undo" }));
    assert_eq!(r["elements"]["removed"], json!([id]), "{r}");
    let r = send(&mut e, &mut seq, json!({ "type": "redo" }));
    assert_eq!(r["elements"]["added"][0]["id"], id, "redo brings the same id back");
    let r = send(&mut e, &mut seq, json!({ "type": "text", "text": "x" }));
    assert!(r.get("elements").is_none(), "{r}");
}

#[test]
fn a_keystroke_costs_the_same_whatever_the_document() {
    fn work(name: &str, para_with_fields: usize) -> usize {
        let mut e = open(name);
        let ids: Vec<crate::layout::ParaId> = crate::editing::tests::texts_ids(&e);
        let p = ids[para_with_fields.min(ids.len() - 1)];
        e.track_work = 0;
        let mut seq = 0;
        send(&mut e, &mut seq, json!({ "type": "select", "para": p.0, "start": 0, "end": 0 }));
        send(&mut e, &mut seq, json!({ "type": "text", "text": "x" }));
        e.track_work
    }
    let small = work("FX-MERGE-FIELDS.docx", usize::MAX);
    let large = work("FX-MERGE-LARGE.docx", 1);
    assert!(small > 0 && small == large, "keystroke work {small} vs {large}");
}

fn shape(e: &Editor) -> Vec<String> {
    let tag = |c: &crate::docx::ControlId| {
        e.document()
            .controls
            .get(c)
            .map(|k| k.props.tag.clone().unwrap_or_else(|| format!("{:?}", std::mem::discriminant(&k.props.kind))))
            .unwrap_or("?".into())
    };
    super::texts_ids(e)
        .iter()
        .filter_map(|p| e.index().para(e.document(), *p))
        .map(|p| {
            let mut s: String = p.controls.iter().map(|c| format!("[{}]", tag(c))).collect();
            for i in &p.inlines {
                match i {
                    crate::docx::Inline::Text(t, _) => s.push_str(t),
                    crate::docx::Inline::ControlStart(c) => s.push_str(&format!("<{}|", tag(c))),
                    crate::docx::Inline::ControlEnd(c) => s.push_str(&format!("|{}>", tag(c))),
                    _ => {}
                }
            }
            s
        })
        .collect()
}

#[test]
fn control_edits_save_and_read_back() {
    let mut failed = Vec::new();
    let cases: Vec<(&str, Vec<Value>)> = vec![
        (
            "enter in a block control",
            vec![
                json!({"type":"command","id":"edit.select-range","arg":"@3,15,15"}),
                json!({"type":"key","code":"Enter"}),
                json!({"type":"text","text":"New"}),
            ],
        ),
        (
            "delete a block control's paragraph",
            vec![
                json!({"type":"command","id":"edit.select-range","arg":"@4,0,15"}),
                json!({"type":"key","code":"Delete"}),
                json!({"type":"key","code":"Backspace"}),
            ],
        ),
        (
            "delete an inline control whole",
            vec![json!({"type":"command","id":"edit.select-range","arg":"@1,6,14"}), json!({"type":"key","code":"Delete"})],
        ),
        ("type in an inline control", vec![json!({"type":"command","id":"edit.select-range","arg":"@1,8,8"}), json!({"type":"text","text":"Q"})]),
        (
            "paste an inline control",
            vec![
                json!({"type":"command","id":"edit.select-range","arg":"@1,6,14"}),
                json!({"type":"copy"}),
                json!({"type":"command","id":"edit.select-range","arg":"@20,0,0"}),
                json!({"type":"paste","token":"1","text":"John Doe"}),
            ],
        ),
        (
            "paste a block control",
            vec![
                json!({"type":"command","id":"edit.select-range","arg":"@3,0,15"}),
                json!({"type":"key","code":"ArrowDown","mods":{"shift":true}}),
                json!({"type":"key","code":"ArrowDown","mods":{"shift":true}}),
                json!({"type":"copy"}),
                json!({"type":"command","id":"edit.select-range","arg":"@20,0,0"}),
                json!({"type":"paste","token":"1","text":"x"}),
            ],
        ),
        (
            "type at a control's content start",
            vec![json!({"type":"command","id":"edit.select-range","arg":"@1,6,6"}), json!({"type":"text","text":"Q"})],
        ),
        ("type right after a control", vec![json!({"type":"command","id":"edit.select-range","arg":"@1,14,14"}), json!({"type":"text","text":"Q"})]),
        (
            "delete a control with its markers",
            vec![json!({"type":"command","id":"edit.select-range","arg":"@1,5,15"}), json!({"type":"key","code":"Delete"})],
        ),
        (
            "enter at a block control's end",
            vec![
                json!({"type":"command","id":"edit.select-range","arg":"@4,15,15"}),
                json!({"type":"key","code":"Enter"}),
                json!({"type":"text","text":"Inside"}),
            ],
        ),
        (
            "delete a block control whole",
            vec![
                json!({"type":"command","id":"edit.select-range","arg":"@3,0,0"}),
                json!({"type":"key","code":"ArrowDown","mods":{"shift":true}}),
                json!({"type":"key","code":"ArrowDown","mods":{"shift":true}}),
                json!({"type":"key","code":"Delete"}),
            ],
        ),
        (
            "paste a whole block control",
            vec![
                json!({"type":"command","id":"edit.select-range","arg":"@3,0,0"}),
                json!({"type":"key","code":"ArrowDown","mods":{"shift":true}}),
                json!({"type":"key","code":"ArrowDown","mods":{"shift":true}}),
                json!({"type":"copy"}),
                json!({"type":"command","id":"edit.select-range","arg":"@20,0,0"}),
                json!({"type":"paste","token":"1","text":"x"}),
            ],
        ),
        ("type in a cell control", vec![json!({"type":"command","id":"edit.select-range","arg":"@12,4,4"}), json!({"type":"text","text":"Q"})]),
        ("enter in a row control", vec![json!({"type":"command","id":"edit.select-range","arg":"@13,3,3"}), json!({"type":"key","code":"Enter"})]),
    ];
    for (name, msgs) in cases {
        let mut e = open("FX-MERGE-CONTROLS.docx");
        let mut seq = 0;
        for m in msgs {
            seq += 1;
            let mut m = m;
            m["seq"] = json!(seq);
            if let Some(a) = m["arg"].as_str().and_then(|a| a.strip_prefix('@')) {
                let (k, rest) = a.split_once(',').unwrap();
                let id = super::texts_ids(&e)[k.parse::<usize>().unwrap()].0;
                m["arg"] = json!(format!("{id},{rest}"));
            }
            if m["type"] == "paste"
                && let Some(f) = e.clipboard()
            {
                m["token"] = json!(f.id.to_string());
                m["text"] = json!(f.text());
            }
            let r = e.apply_json(&m.to_string(), fonts());
            if r.contains("\"error\"") {
                eprintln!("{name}: {m} -> {}", &r[..r.len().min(200)]);
            }
        }
        let want = shape(&e);
        let bytes = crate::docx::write::save(e.document()).expect("saves");
        if let Some(d) = std::env::var_os("WORD_CHECK_DIR") {
            std::fs::write(std::path::Path::new(&d).join(format!("CTL-{}.docx", name.replace([' ', '\''], "-"))), &bytes).unwrap();
        }
        let again = Editor::open(&bytes, fonts()).expect("reads");
        let got = shape(&again);
        if want != got {
            let k = (0..want.len().max(got.len())).find(|&k| want.get(k) != got.get(k)).unwrap();
            failed.push(format!("{name}: p{k} {:?} became {:?}", want.get(k), got.get(k)));
        }
    }
    assert!(
        failed.is_empty(),
        "{}",
        failed.join(
            "
"
        )
    );
}

#[test]
fn changed_control_properties_are_written() {
    let bytes = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/FX-MERGE-CONTROLS.docx")).unwrap();
    let mut doc = crate::docx::read_editable(&bytes).unwrap();
    let by_tag = |d: &crate::docx::Document, t: &str| d.controls.iter().find(|(_, c)| c.props.tag.as_deref() == Some(t)).map(|(id, _)| *id);
    for (old, new) in [("Recipient/Name", "Renamed/Inline"), ("Content:Complaint", "Renamed/Block"), ("Group/One", "Renamed/Group")] {
        let id = by_tag(&doc, old).unwrap_or_else(|| panic!("{old} in the fixture"));
        doc.controls.get_mut(&id).unwrap().props.tag = Some(new.into());
    }
    let row = doc.controls.iter().find(|(_, c)| c.props.kind == crate::docx::ControlKind::RepeatingItem).map(|(id, _)| *id);
    if let Some(id) = row {
        doc.controls.get_mut(&id).unwrap().props.alias = Some("Renamed row".into());
    }
    let again = crate::docx::read_editable(&crate::docx::write::save(&doc).unwrap()).unwrap();
    for t in ["Renamed/Inline", "Renamed/Block", "Renamed/Group"] {
        assert!(by_tag(&again, t).is_some(), "{t} written");
    }
    for t in ["Recipient/Name", "Content:Complaint", "Group/One"] {
        assert!(by_tag(&again, t).is_none(), "{t} replaced");
    }
    if row.is_some() {
        assert!(again.controls.values().any(|c| c.props.alias.as_deref() == Some("Renamed row")), "the row control's alias written");
    }
}

#[test]
#[ignore = "writes files for the Word check"]
fn word_check_files() {
    use crate::docx::{Block, Control, ControlId, ControlKind, ControlProps, Inline};
    let dir = std::path::PathBuf::from(std::env::var("WORD_CHECK_DIR").expect("WORD_CHECK_DIR"));
    let bytes = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/FX-TEXT.docx")).unwrap();
    let mut doc = crate::docx::read_editable(&bytes).unwrap();
    let id = ControlId::next();
    let props =
        ControlProps { kind: ControlKind::PlainText { multi_line: false }, tag: Some("R3".into()), showing_placeholder: true, ..Default::default() };
    doc.controls.insert(id, Control::new(props));
    let Some(Block::Para(p)) = doc.sections[0].blocks.first_mut() else { panic!("a first paragraph") };
    let run = p.inlines.iter().find_map(|i| i.run().cloned()).expect("a run");
    p.inlines.splice(0..0, [Inline::ControlStart(id), Inline::Text("Click or tap here to enter text.".into(), run), Inline::ControlEnd(id)]);
    std::fs::write(dir.join("R3-PLACEHOLDER.docx"), crate::docx::write::save(&doc).unwrap()).unwrap();
    let mut all = serde_json::Map::new();
    let mut data = serde_json::Map::new();
    let mut names: Vec<_> =
        std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "docx")).collect();
    names.sort();
    for f in names {
        let Ok(mut e) = Editor::open(&std::fs::read(&f).unwrap(), fonts()) else { continue };
        let mut seq = 0;
        let list: Vec<Value> = elements(&mut e, &mut seq, json!({}))
            .into_iter()
            .map(|x| json!({ "kind": x["kind"], "code": x["code"].as_str().map(str::trim), "tag": x["tag"], "story": x["story"] }))
            .collect();
        let name = f.file_name().unwrap().to_string_lossy().to_string();
        all.insert(name.clone(), Value::Array(list));
        data.insert(name, json!(e.document().data_parts.iter().map(|p| p.namespace.clone()).collect::<Vec<_>>()));
    }
    std::fs::write(dir.join("elements.json"), serde_json::to_string_pretty(&all).unwrap()).unwrap();
    std::fs::write(dir.join("data.json"), serde_json::to_string_pretty(&data).unwrap()).unwrap();
}

#[test]
fn a_selection_partly_over_a_field_takes_it_whole() {
    let mut e = open("FX-MERGE-FIELDS.docx");
    let id = super::texts_ids(&e)[2];
    let mut seq = 0;
    send(&mut e, &mut seq, json!({ "type": "select", "para": id.0, "start": 4, "end": 30 }));
    assert_eq!(e.selection().focus().and_then(|p| p.para_offset()).map(|(_, u)| u.0), Some(90), "the field's end");
    send(&mut e, &mut seq, json!({ "type": "text", "text": "X" }));
    assert_eq!(super::texts(&e)[2], "ShipX issued.");
}

#[test]
fn design_mode_labels_empty_fields() {
    let mut e = open("FX-TEXT.docx");
    let mut seq = 0;
    let id = super::texts_ids(&e)[1];
    send(&mut e, &mut seq, json!({ "type": "select", "para": id.0, "start": 6, "end": 6 }));
    send(&mut e, &mut seq, json!({"type": "command", "command": {"type": "insertField", "code": "MERGEFIELD Cost_Amount"}}));
    let drawn = |e: &mut Editor| {
        e.complete(fonts());
        crate::layout::dump::page_text(&e.snapshot(), e.document())
    };
    assert!(!drawn(&mut e).contains("Cost_Amount"), "no label while editing");
    send(&mut e, &mut seq, json!({"type": "mode", "mode": "design"}));
    assert!(drawn(&mut e).contains("\u{ab}Cost_Amount\u{bb}"), "the first estimate in design mode");
    send(&mut e, &mut seq, json!({"type": "command", "command": {"type": "setFieldLabels", "labels": {"MERGEFIELD Cost_Amount": "Cost amount"}}}));
    assert!(drawn(&mut e).contains("Cost amount"), "the host's label");
    let saved = crate::docx::write::save(e.document()).unwrap();
    let mut xml = String::new();
    std::io::Read::read_to_string(
        &mut zip::ZipArchive::new(std::io::Cursor::new(&saved[..])).unwrap().by_name("word/document.xml").unwrap(),
        &mut xml,
    )
    .unwrap();
    assert!(!xml.contains("Cost amount") && !xml.contains("\u{ab}Cost_Amount"), "labels aren't saved");
    send(&mut e, &mut seq, json!({"type": "command", "command": {"type": "undo"}}));
    send(&mut e, &mut seq, json!({"type": "mode", "mode": "editing"}));
    send(&mut e, &mut seq, json!({"type": "command", "command": {"type": "redo"}}));
    assert!(!drawn(&mut e).contains("Cost"), "no label after leaving design mode");
}

#[test]
fn a_label_wraps_in_an_autofit_cell_rather_than_widen_the_table() {
    let cell = |w: u32, r: &str| format!(r#"<w:tc><w:tcPr><w:tcW w:w="{w}" w:type="dxa"/></w:tcPr><w:p>{r}</w:p></w:tc>"#);
    let field = r#"<w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText>MERGEFIELD Violation.Restitution_Balance_Amount_Long_Name</w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r>"#;
    let row: String = (0..5).map(|_| cell(1800, field)).collect();
    let body = format!(
        r#"<w:tbl><w:tblPr><w:tblW w:w="0" w:type="auto"/></w:tblPr><w:tblGrid>{}</w:tblGrid><w:tr>{row}</w:tr></w:tbl><w:p/>"#,
        r#"<w:gridCol w:w="1800"/>"#.repeat(5)
    );
    let xml = format!(
        r#"<?xml version="1.0"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{body}<w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr></w:body></w:document>"#
    );
    let settings = r#"<?xml version="1.0"?><w:settings xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:compat><w:growAutofit/></w:compat></w:settings>"#;
    let rels = r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings" Target="settings.xml"/></Relationships>"#;
    let bytes = crate::docx::write::repackage(
        &crate::docx::blank::blank_docx(),
        &[
            ("word/document.xml".into(), xml.into_bytes()),
            ("word/settings.xml".into(), settings.as_bytes().to_vec()),
            ("word/_rels/document.xml.rels".into(), rels.as_bytes().to_vec()),
        ],
        zip::CompressionMethod::Deflated,
    )
    .expect("repackages");
    let mut e = Editor::open(&bytes, fonts()).expect("opens");
    let mut seq = 0;
    send(&mut e, &mut seq, json!({"type": "mode", "mode": "design"}));
    e.complete(fonts());
    let text = crate::layout::dump::page_text(&e.snapshot(), e.document());
    assert!(text.contains("Violation.Restituti"), "the labels are drawn, wrapped");
    let xs = text
        .lines()
        .filter_map(|l| l.split_once(" cx ["))
        .flat_map(|(_, r)| r.split(']').next().unwrap_or("").split_whitespace().map(|x| x.parse::<f64>().unwrap()).collect::<Vec<_>>());
    let right = xs.fold(0.0, f64::max);
    assert!(right < 540.0, "the labels wrap inside the margin ({right})");
}

#[test]
fn regions_save_and_read_back() {
    let mut e = open("FX-TBL-BASIC.docx");
    let mut seq = 0;
    let ids = super::texts_ids(&e);
    send(&mut e, &mut seq, json!({ "type": "select", "para": ids[13].0, "start": 0, "end": 0 }));
    send(&mut e, &mut seq, json!({"type": "command", "command": {"type": "select", "value": "table"}}));
    send(&mut e, &mut seq, json!({"type": "command", "command": {"type": "wrapInRepeatingSection", "tag": "Rows"}}));
    send(&mut e, &mut seq, json!({ "type": "select", "para": ids[41].0, "start": 0, "end": 0 }));
    send(&mut e, &mut seq, json!({ "type": "key", "code": "ArrowDown", "mods": { "shift": true } }));
    send(&mut e, &mut seq, json!({ "type": "key", "code": "End", "mods": { "shift": true } }));
    send(&mut e, &mut seq, json!({"type": "command", "command": {"type": "wrapInRepeatingSection", "tag": "Paras"}}));
    let kinds = |e: &mut Editor, seq: &mut u64| -> Vec<(String, String)> {
        elements(e, seq, json!({})).iter().filter(|x| x["kind"] != "field").map(|x| (x["kind"].to_string(), x["tag"].to_string())).collect()
    };
    let want = kinds(&mut e, &mut seq);
    assert_eq!(want.iter().filter(|(k, _)| k == "\"repeating_item\"").count(), 4, "{want:?}");
    let bytes = crate::docx::write::save(e.document()).unwrap();
    if let Some(d) = std::env::var_os("WORD_CHECK_DIR") {
        std::fs::write(std::path::Path::new(&d).join("REGIONS.docx"), &bytes).unwrap();
    }
    let mut again = Editor::open(&bytes, fonts()).unwrap();
    assert_eq!(kinds(&mut again, &mut seq), want);
}

#[test]
fn checkbox_tags_and_floats_survive_control_edits() {
    let mut seq = 0;
    let mut e = open("FX-MERGE-CONTROLS.docx");
    let check = elements(&mut e, &mut seq, json!({ "kind": "checkbox" }))[0]["id"].clone();
    send(&mut e, &mut seq, json!({ "type": "command", "command": { "type": "setControl", "id": check, "tag": "Check:Renamed" } }));
    let saved = crate::docx::write::save(e.document()).unwrap();
    let mut again = Editor::open(&saved, fonts()).unwrap();
    let tags: Vec<Value> = elements(&mut again, &mut seq, json!({ "kind": "checkbox" })).iter().map(|c| c["tag"].clone()).collect();
    assert_eq!(tags, [json!("Check:Renamed")]);

    let mut e = open("FX-NEST-2-BOX.docx");
    let floats = |e: &Editor| {
        e.document()
            .sections
            .iter()
            .flat_map(|s| s.blocks.iter())
            .filter_map(|b| if let crate::docx::Block::Para(p) = b { Some(p) } else { None })
            .flat_map(|p| p.inlines.iter())
            .filter(|i| matches!(i, crate::docx::Inline::Float(..)))
            .count()
    };
    let before = floats(&e);
    assert!(before > 0);
    send(&mut e, &mut seq, json!({ "type": "key", "code": "Home", "mods": { "ctrl": true } }));
    send(&mut e, &mut seq, json!({"type": "command", "command": {"type": "insertControl", "kind": "plainText", "tag": "New"}}));
    let again = Editor::open(&crate::docx::write::save(e.document()).unwrap(), fonts()).unwrap();
    assert_eq!(floats(&again), before);
}

fn document_xml(bytes: &[u8]) -> String {
    use std::io::Read;
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut s = String::new();
    z.by_name("word/document.xml").unwrap().read_to_string(&mut s).unwrap();
    s
}

/// What the model doesn't hold, around and after the control.
const KEPT: &str = concat!(
    r#"<w:bookmarkStart w:id="5" w:name="Keep"/><w:r><w:t xml:space="preserve">Name: </w:t></w:r><w:bookmarkEnd w:id="5"/>"#,
    "{SDT}",
    r#"<w:r><w:t xml:space="preserve"> and </w:t></w:r><w:r><w:rPr><w:vanish/></w:rPr><w:t>secret</w:t></w:r>"#,
    r#"<m:oMath xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math"><m:r><m:t>x</m:t></m:r></m:oMath>"#,
    r#"<w:r><w:ruby><w:rubyPr><w:rubyAlign w:val="center"/><w:hps w:val="10"/><w:hpsRaise w:val="18"/><w:hpsBaseText w:val="20"/><w:lid w:val="ja-JP"/></w:rubyPr><w:rt><w:r><w:t>kan</w:t></w:r></w:rt><w:rubyBase><w:r><w:t>K</w:t></w:r></w:rubyBase></w:ruby></w:r>"#,
    r#"<w:r><w:fldChar w:fldCharType="begin"><w:ffData><w:name w:val="Text1"/><w:enabled/></w:ffData></w:fldChar></w:r><w:r><w:instrText xml:space="preserve"> FORMTEXT </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>form</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r>"#,
);

#[test]
fn filling_a_control_keeps_its_paragraph() {
    use crate::api::command::FillControl;
    use crate::editing::commands::{Command, Outcome};
    let cases = [
        r#"<w:sdt><w:sdtPr><w:tag w:val="Name"/><w:showingPlcHdr/><w:text/></w:sdtPr><w:sdtContent><w:r><w:rPr><w:rStyle w:val="PlaceholderText"/></w:rPr><w:t>Click here</w:t></w:r></w:sdtContent></w:sdt>"#,
        r#"<w:sdt><w:sdtPr><w:tag w:val="Name"/><w:text/></w:sdtPr><w:sdtContent></w:sdtContent></w:sdt>"#,
    ];
    for sdt in cases {
        let bytes = super::blank_body(&format!("<w:p>{}</w:p>", KEPT.replace("{SDT}", sdt)));
        let mut e = Editor::open(&bytes, fonts()).expect("opens");
        let (c, _) = e.document().controls.iter().find(|(_, k)| k.props.tag.as_deref() == Some("Name")).expect("the control");
        let id = crate::api::value::ElementId(crate::editing::elements::id_str(crate::docx::ElementId::Control(*c)));
        let out = e.execute(Command::FillControl(FillControl { id, text: Some("John".into()), image: None, docx: None }), fonts());
        assert!(matches!(out, Outcome::Changed(_)), "{sdt}: {out:?}");
        let saved = crate::docx::write::save(e.document()).expect("saves");
        let xml = document_xml(&saved);
        for kept in ["w:bookmarkStart w:id=\"5\"", "<w:vanish/>", "m:oMath", "w:ruby", "w:ffData", "w:sdtContent"] {
            assert!(xml.contains(kept), "{sdt}: {kept} kept in {xml}");
        }
        assert!(!xml.contains("showingPlcHdr"), "{sdt}: filled in {xml}");
        let sdt = &xml[xml.find("<w:sdt>").unwrap()..xml.find("</w:sdt>").unwrap()];
        assert!(sdt.contains("John"), "the text in the control: {xml}");
        let again = Editor::open(&saved, fonts()).expect("reopens");
        assert_eq!(super::texts(&again), super::texts(&e));
    }
}

#[test]
fn editing_a_link_or_simple_field_keeps_its_element() {
    let body = concat!(
        r#"<w:p><w:bookmarkStart w:id="1" w:name="Target"/><w:r><w:t xml:space="preserve">See </w:t></w:r><w:bookmarkEnd w:id="1"/>"#,
        r#"<w:hyperlink w:anchor="Target" w:tooltip="Tip" w:docLocation="loc" w:history="1"><w:r><w:rPr><w:rStyle w:val="Hyperlink"/></w:rPr><w:t>link text</w:t></w:r></w:hyperlink>"#,
        r#"<w:r><w:t xml:space="preserve"> by </w:t></w:r>"#,
        r#"<w:fldSimple w:instr=" DOCVARIABLE Who " w:dirty="1"><w:r><w:t>Ada</w:t></w:r></w:fldSimple><w:r><w:t>.</w:t></w:r></w:p>"#,
    );
    let bytes = super::blank_body(body);
    for (from, to) in [("link text", "linked text"), ("Ada", "Ada L")] {
        let mut doc = crate::docx::read_editable(&bytes).expect("reads");
        let Some(crate::docx::Block::Para(p)) = doc.sections[0].blocks.first_mut() else { panic!("a paragraph") };
        let i = p.inlines.iter_mut().find(|i| matches!(i, crate::docx::Inline::Text(s, _) if s == from)).expect("the text");
        if let crate::docx::Inline::Text(s, _) = i {
            *s = to.into();
        }
        let xml = document_xml(&crate::docx::write::save(&doc).expect("saves"));
        assert!(xml.contains(r#"<w:hyperlink w:anchor="Target" w:tooltip="Tip" w:docLocation="loc" w:history="1">"#), "{to}: {xml}");
        assert!(xml.contains(r#"<w:fldSimple w:instr=" DOCVARIABLE Who " w:dirty="1">"#), "{to}: {xml}");
        assert!(!xml.contains("fldChar"), "{to}: no complex field in {xml}");
        let again = crate::docx::read_editable(&crate::docx::write::save(&doc).expect("saves")).expect("reads");
        let Some(crate::docx::Block::Para(p)) = again.sections[0].blocks.first() else { panic!("a paragraph") };
        let text: String = p.inlines.iter().filter_map(|i| if let crate::docx::Inline::Text(s, _) = i { Some(s.as_str()) } else { None }).collect();
        assert!(text.contains(to), "{to}: {text}");
    }
}

#[test]
fn an_edited_paragraph_keeps_its_text_wrapping_breaks() {
    let bytes = super::blank_body(r#"<w:p><w:r><w:t>one</w:t><w:br w:clear="left"/><w:t>two</w:t><w:br w:clear="all"/><w:t>three</w:t></w:r></w:p>"#);
    let mut doc = crate::docx::read_editable(&bytes).expect("reads");
    let Some(crate::docx::Block::Para(p)) = doc.sections[0].blocks.first_mut() else { panic!("a paragraph") };
    let Some(crate::docx::Inline::Text(s, _)) = p.inlines.iter_mut().find(|i| matches!(i, crate::docx::Inline::Text(s, _) if s == "two")) else {
        panic!("the text")
    };
    *s = "twice".into();
    let xml = document_xml(&crate::docx::write::save(&doc).expect("saves"));
    assert!(xml.contains(r#"<w:br w:clear="left"/>"#) && xml.contains(r#"<w:br w:clear="all"/>"#) && xml.contains("twice"), "{xml}");
}
