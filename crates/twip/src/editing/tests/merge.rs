//! Elements: enumeration order and parents, change lists across edits, undo and redo, and the
//! change-list cost of a keystroke.

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
    // Body before the header and footer, then the footnote, then the text box.
    assert!(at(r#"MERGEFIELD Statement.Cost_Amount \# "$,0.00""#) < at("MERGEFIELD Store.Name"));
    assert!(at("PAGE") < at("MERGEFIELD Recipient.First_Name"), "the footer before the footnote");
    assert!(at("MERGEFIELD Recipient.First_Name") < at("MERGEFIELD Manager.Badge"), "the footnote before the text box");
    // The fields nested in the IF list after it, with it as their parent.
    let iff = at(r#"IF  = "Paid" "" """#);
    let status = at("MERGEFIELD Statement.Status");
    assert_eq!(status, iff + 1);
    assert_eq!(all[status]["parent"], all[iff]["id"]);
    // Filters: by kind, by code text.
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
    let r = send(&mut e, &mut seq, json!({ "type": "command", "id": "insert.page-number.page" }));
    let added = r["elements"]["added"].as_array().cloned().unwrap_or_default();
    assert_eq!(added.len(), 1, "{r}");
    let id = added[0]["id"].clone();
    assert_eq!(added[0]["code"], " PAGE ");
    let r = send(&mut e, &mut seq, json!({ "type": "undo" }));
    assert_eq!(r["elements"]["removed"], json!([id]), "{r}");
    let r = send(&mut e, &mut seq, json!({ "type": "redo" }));
    assert_eq!(r["elements"]["added"][0]["id"], id, "redo brings the same id back");
    // Typing next to it changes nothing about it.
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
        send(&mut e, &mut seq, json!({ "type": "command", "id": "edit.select-range", "arg": format!("{},0,0", p.0) }));
        send(&mut e, &mut seq, json!({ "type": "text", "text": "x" }));
        e.track_work
    }
    // A paragraph holding five placeholders in a two-page and a sixty-page document.
    let small = work("FX-MERGE-FIELDS.docx", usize::MAX);
    let large = work("FX-MERGE-LARGE.docx", 1);
    assert!(small > 0 && small == large, "keystroke work {small} vs {large}");
}

/// Each paragraph's text with its control markers and block controls, by control tag (ids are the session's).
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

/// Control edits through the editor survive a save and a reread: each paragraph's text, inline markers and
/// block controls read back as they were.
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
        // Kept for the Word check (`word_check_files`).
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

/// Changed properties of read controls are written: an inline control's in its rewritten paragraph, a block's
/// and a row's in place.
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

/// For a word-processor check, into `WORD_CHECK_DIR`: a new plain-text control
/// showing placeholder text from model values alone (content and `showingPlcHdr`, no `docPart`), and
/// `elements.json`, every file's elements there (kind, code, tag) to compare with Word's enumeration. Run
/// `control_edits_save_and_read_back` and `roundtrip --edits all --keep` into the folder first.
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

/// A selection ending inside a field (a host's range) widens to the whole field, so typing over it replaces it.
#[test]
fn a_selection_partly_over_a_field_takes_it_whole() {
    let mut e = open("FX-MERGE-FIELDS.docx");
    let id = super::texts_ids(&e)[2];
    let mut seq = 0;
    send(&mut e, &mut seq, json!({ "type": "command", "id": "edit.select-range", "arg": format!("{},4,30", id.0) }));
    assert_eq!(e.selection().focus().and_then(|p| p.para_offset()).map(|(_, u)| u.0), Some(90), "the field's end");
    send(&mut e, &mut seq, json!({ "type": "text", "text": "X" }));
    assert_eq!(super::texts(&e)[2], "ShipX issued.");
}

/// Design mode draws an empty field's label: its first argument in «», or the host's label for its
/// code; leaving design mode takes it away, and a save never writes it.
#[test]
fn design_mode_labels_empty_fields() {
    let mut e = open("FX-TEXT.docx");
    let mut seq = 0;
    let id = super::texts_ids(&e)[1];
    send(&mut e, &mut seq, json!({ "type": "command", "id": "edit.select-range", "arg": format!("{},6,6", id.0) }));
    send(&mut e, &mut seq, json!({ "type": "command", "id": "field.insert", "arg": { "code": "MERGEFIELD Cost_Amount" } }));
    let drawn = |e: &mut Editor| {
        e.complete(fonts());
        crate::layout::dump::page_text(&e.snapshot(), e.document())
    };
    assert!(!drawn(&mut e).contains("Cost_Amount"), "no label while editing");
    send(&mut e, &mut seq, json!({ "type": "command", "id": "view.mode", "arg": "design" }));
    assert!(drawn(&mut e).contains("\u{ab}Cost_Amount\u{bb}"), "the first estimate in design mode");
    send(&mut e, &mut seq, json!({ "type": "command", "id": "field.labels", "arg": { "labels": { "MERGEFIELD Cost_Amount": "Cost amount" } } }));
    assert!(drawn(&mut e).contains("Cost amount"), "the host's label");
    let saved = crate::docx::write::save(e.document()).unwrap();
    let mut xml = String::new();
    std::io::Read::read_to_string(
        &mut zip::ZipArchive::new(std::io::Cursor::new(&saved[..])).unwrap().by_name("word/document.xml").unwrap(),
        &mut xml,
    )
    .unwrap();
    assert!(!xml.contains("Cost amount") && !xml.contains("\u{ab}Cost_Amount"), "labels aren't saved");
    // Undo in design mode, then leave it: no label comes back with the field.
    send(&mut e, &mut seq, json!({ "type": "command", "id": "home.undo" }));
    send(&mut e, &mut seq, json!({ "type": "command", "id": "view.mode", "arg": "editing" }));
    send(&mut e, &mut seq, json!({ "type": "command", "id": "home.redo" }));
    assert!(!drawn(&mut e).contains("Cost"), "no label after leaving design mode");
}

/// Regions survive a save: rows of a table and a paragraph run wrapped, saved and read back
/// with the same sections and items; the files go to `WORD_CHECK_DIR` too, to open in Word.
#[test]
fn regions_save_and_read_back() {
    let mut e = open("FX-TBL-BASIC.docx");
    let mut seq = 0;
    let ids = super::texts_ids(&e);
    // T2's three rows, and the two paragraphs between the tables after it.
    send(&mut e, &mut seq, json!({ "type": "command", "id": "edit.select-range", "arg": format!("{},0,0", ids[13].0) }));
    send(&mut e, &mut seq, json!({ "type": "command", "id": "table.select.table" }));
    send(&mut e, &mut seq, json!({ "type": "command", "id": "region.wrap", "arg": { "tag": "Rows" } }));
    send(&mut e, &mut seq, json!({ "type": "command", "id": "edit.select-range", "arg": format!("{},0,0", ids[40].0) }));
    send(&mut e, &mut seq, json!({ "type": "key", "code": "ArrowDown", "mods": { "shift": true } }));
    send(&mut e, &mut seq, json!({ "type": "key", "code": "End", "mods": { "shift": true } }));
    send(&mut e, &mut seq, json!({ "type": "command", "id": "region.wrap", "arg": { "tag": "Paras" } }));
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

/// A checkbox control's new tag is saved (a checkbox is its face, with no start and end markers in the paragraph), and
/// a float anchored in a paragraph written again for a new control is kept (`roundtrip --edits merge`).
#[test]
fn checkbox_tags_and_floats_survive_control_edits() {
    let mut seq = 0;
    let mut e = open("FX-MERGE-CONTROLS.docx");
    let check = elements(&mut e, &mut seq, json!({ "kind": "checkbox" }))[0]["id"].clone();
    send(&mut e, &mut seq, json!({ "type": "command", "id": "control.set", "arg": { "id": check, "tag": "Check:Renamed" } }));
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
    send(&mut e, &mut seq, json!({ "type": "command", "id": "control.insert", "arg": { "kind": "plain_text", "tag": "New" } }));
    let again = Editor::open(&crate::docx::write::save(e.document()).unwrap(), fonts()).unwrap();
    assert_eq!(floats(&again), before);
}
