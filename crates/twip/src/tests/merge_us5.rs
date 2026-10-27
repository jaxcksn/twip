//! Data parts through the host API: a data part written and the text edited survive a save and reopen; a part the
//! host didn't touch keeps its bytes; a bound control keeps its binding; and data parts don't change the layout.
//! With `WORD_CHECK_DIR` set, the saved file goes there too, to check its custom XML parts in another application.

use serde_json::{Value, json};

use crate::font::Fonts;
use crate::host::Engine;

fn send(e: &mut Engine, msg: Value) -> Value {
    let r: Value = serde_json::from_str(&e.send_json(&msg.to_string())).expect("a reply");
    assert!(r.get("error").is_none() && r["answer"].get("error").is_none(), "{msg} -> {r}");
    r
}

fn part(bytes: &[u8], name: &str) -> Vec<u8> {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut out = Vec::new();
    std::io::Read::read_to_end(&mut z.by_name(name).unwrap(), &mut out).unwrap();
    out
}

fn layout_text(e: &mut Engine) -> String {
    let fonts = Fonts::system();
    let ed = e.editor().unwrap();
    ed.complete(&fonts);
    crate::layout::dump::page_text(&ed.snapshot(), ed.document())
}

#[test]
fn data_parts_survive_edits_and_saves() {
    let bytes = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/FX-MERGE-DATA.docx")).unwrap();
    let mut e = Engine::with_fonts(Fonts::system());
    e.open(&bytes);
    let xml = r#"<calc xmlns="urn:store:calc"><field name="Total">Cost + Fee</field></calc>"#;
    send(&mut e, json!({ "type": "command", "id": "data.write", "arg": { "namespace": "urn:store:calc", "xml": xml } }));
    send(&mut e, json!({ "type": "text", "text": "Edited. " }));
    let saved = e.save().unwrap();
    if let Some(d) = std::env::var_os("WORD_CHECK_DIR") {
        std::fs::write(std::path::Path::new(&d).join("US5-DATA.docx"), &saved).unwrap();
    }
    let mut again = Engine::with_fonts(Fonts::system());
    again.open(&saved);
    let parts = send(&mut again, json!({ "type": "query", "q": "data" }))["answer"].clone();
    assert!(parts.as_array().unwrap().iter().any(|p| p["namespace"] == "urn:store:calc" && p["xml"] == xml), "{parts}");
    // The parts the host didn't touch: their item parts byte for byte.
    let src = crate::docx::read_editable(&bytes).unwrap();
    for p in &src.data_parts {
        let name = p.src.clone().expect("read from the package");
        assert_eq!(part(&saved, &name), part(&bytes, &name), "{name} unchanged");
    }
    // The bound control keeps its binding.
    let els = send(&mut again, json!({ "type": "query", "q": "elements" }))["answer"].clone();
    assert!(els.as_array().unwrap().iter().any(|x| x["binding"] == true), "{els}");
}

#[test]
fn data_parts_dont_change_the_layout() {
    let bytes = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/FX-MERGE-DATA.docx")).unwrap();
    let mut e = Engine::with_fonts(Fonts::system());
    e.open(&bytes);
    let with = layout_text(&mut e);
    for ns in ["urn:store:form-definition", "urn:store:form-data"] {
        send(&mut e, json!({ "type": "command", "id": "data.remove", "arg": { "namespace": ns } }));
    }
    let saved = e.save().unwrap();
    let mut without = Engine::with_fonts(Fonts::system());
    without.open(&saved);
    assert!(send(&mut without, json!({ "type": "query", "q": "data" }))["answer"].as_array().unwrap().is_empty());
    assert_eq!(layout_text(&mut without), with, "data parts don't change the layout");
}
