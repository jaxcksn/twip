//! Data parts through the API: a data part written and the text edited survive a save and reopen; a part the host
//! didn't touch keeps its bytes; a bound control keeps its binding; and data parts don't change the layout. With
//! `WORD_CHECK_DIR` set, the saved file goes there too, to check its custom XML parts in another application.

use crate::api::command::{Command, DataPartRef, WriteDataPart};
use crate::api::session::Session;
use crate::font::Fonts;

fn part(bytes: &[u8], name: &str) -> Vec<u8> {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut out = Vec::new();
    std::io::Read::read_to_end(&mut z.by_name(name).unwrap(), &mut out).unwrap();
    out
}

fn layout_text(s: &mut Session) -> String {
    s.with_editor(|ed| {
        ed.complete(&Fonts::system());
        crate::layout::dump::page_text(&ed.snapshot(), ed.document())
    })
}

fn fixture() -> Vec<u8> {
    std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/FX-MERGE-DATA.docx")).unwrap()
}

#[test]
fn data_parts_survive_edits_and_saves() {
    let bytes = fixture();
    let (mut engine, id) = super::session(&bytes);
    let e = engine.session(id).expect("a session");
    let xml = r#"<calc xmlns="urn:store:calc"><field name="Total">Cost + Fee</field></calc>"#;
    e.run(Command::WriteDataPart(WriteDataPart { namespace: "urn:store:calc".into(), item_id: None, xml: xml.into() })).expect("writes");
    e.text("Edited. ");
    let saved = e.export(&Default::default()).expect("saves").bytes.0;
    if let Some(d) = std::env::var_os("WORD_CHECK_DIR") {
        std::fs::write(std::path::Path::new(&d).join("US5-DATA.docx"), &saved).unwrap();
    }
    let (mut again, id) = super::session(&saved);
    let again = again.session(id).expect("a session");
    let parts = again.data_parts(None, None);
    assert!(parts.iter().any(|p| p.namespace.as_deref() == Some("urn:store:calc") && p.xml == xml), "{parts:?}");
    // The parts the host didn't touch: their item parts byte for byte.
    let src = crate::docx::read_editable(&bytes).unwrap();
    for p in &src.data_parts {
        let name = p.src.clone().expect("read from the package");
        assert_eq!(part(&saved, &name), part(&bytes, &name), "{name} unchanged");
    }
    // The bound control keeps its binding.
    let els = again.elements(&Default::default());
    assert!(els.iter().any(|x| again.element(x.id.clone()).is_ok_and(|d| d.binding.is_some())), "{els:?}");
}

#[test]
fn data_parts_dont_change_the_layout() {
    let (mut engine, id) = super::session(&fixture());
    let e = engine.session(id).expect("a session");
    let with = layout_text(e);
    for ns in ["urn:store:form-definition", "urn:store:form-data"] {
        e.run(Command::RemoveDataPart(DataPartRef { namespace: ns.into(), item_id: None })).expect("removes");
    }
    let saved = e.export(&Default::default()).expect("saves").bytes.0;
    let (mut without, id) = super::session(&saved);
    let without = without.session(id).expect("a session");
    assert!(without.data_parts(None, None).is_empty());
    assert_eq!(layout_text(without), with, "data parts don't change the layout");
}
