//! Fields and controls through the API: insert a field and a plain-text control, change each one's format, undo and
//! redo, save, reopen. The code, tag and display text read back the same. With `WORD_CHECK_DIR` set, the saved file
//! goes there too, to check it in another application. And a copy in one session pastes into another with its fields.

use serde_json::{Value, json};

use crate::api::command::Command;
use crate::api::services::ElementFilter;
use crate::api::session::Session;
use crate::api::state::ElementKind;

/// Runs a command given as JSON; its result as JSON.
fn command(s: &mut Session, op: Value) -> Value {
    let c: Command = serde_json::from_value(op.clone()).unwrap_or_else(|e| panic!("{op}: {e}"));
    let commit = s.run(c).unwrap_or_else(|e| panic!("{op} -> {e:?}"));
    serde_json::to_value(commit.value).expect("a result")
}

/// Each element's kind, code, tag and display text, in document order.
fn elements(s: &Session) -> Vec<(ElementKind, Option<String>, Option<String>, String)> {
    s.elements(&ElementFilter::default()).into_iter().map(|e| (e.kind, e.code, e.tag, e.display)).collect()
}

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor").join(name)).expect("fixture")
}

#[test]
fn insert_change_undo_save_reopen() {
    let (mut engine, id) = super::session(&fixture("FX-TEXT.docx"));
    let e = engine.session(id).expect("a session");
    let f =
        command(e, json!({ "type": "insertField", "code": "MERGEFIELD Statement.Cost_Amount", "display": "\u{ab}Cost_Amount\u{bb}" }))["id"].clone();
    let c = command(e, json!({ "type": "insertControl", "kind": "plainText", "tag": "Statement/Cost_Amount", "placeholder": "Cost" }))["id"].clone();
    command(e, json!({ "type": "setField", "id": f, "code": r#"MERGEFIELD Statement.Cost_Amount \# "$,0.00""# }));
    command(e, json!({ "type": "setControl", "id": c, "tag": "Statement/Cost_Amount:currency" }));
    let want = elements(e);
    for _ in 0..2 {
        command(e, json!({ "type": "undo" }));
    }
    assert_ne!(elements(e), want, "undone");
    for _ in 0..2 {
        command(e, json!({ "type": "redo" }));
    }
    assert_eq!(elements(e), want, "redone");
    assert!(
        want.contains(&(
            ElementKind::Field,
            Some(r#" MERGEFIELD Statement.Cost_Amount \# "$,0.00" "#.into()),
            None,
            "\u{ab}Cost_Amount\u{bb}".into()
        )),
        "{want:?}"
    );
    assert!(want.contains(&(ElementKind::PlainText, None, Some("Statement/Cost_Amount:currency".into()), "Cost".into())), "{want:?}");
    let saved = e.export(&Default::default()).expect("saves").bytes.0;
    if let Some(d) = std::env::var_os("WORD_CHECK_DIR") {
        std::fs::write(std::path::Path::new(&d).join("US1-HOST.docx"), &saved).unwrap();
    }
    let (mut again, id) = super::session(&saved);
    assert_eq!(elements(again.session(id).expect("a session")), want, "read back");
}

/// Two editors: a copy in one pastes into the other with its fields, under new ids, and leaves the first editor's
/// document and history alone.
#[test]
fn paste_between_editors_keeps_fields() {
    use crate::editing::Editor;
    use crate::editing::tests::fonts;
    let bytes = fixture("FX-MERGE-FIELDS.docx");
    let (mut a, mut b) = (Editor::open(&bytes, fonts()).expect("opens"), Editor::open(&bytes, fonts()).expect("opens"));
    let fields = |e: &Editor| {
        crate::editing::elements::enumerate(e.document(), e.index(), &Default::default()).into_iter().filter(|x| x.code.is_some()).collect::<Vec<_>>()
    };
    let before = fields(&b).len();
    let first = fields(&a).remove(0);
    let id = crate::api::value::ElementId(crate::editing::elements::id_str(first.id));
    a.select_element(&id, false, fonts());
    let clip = a.copy().expect("a copy");
    let depth = a.history().undo_depth();
    let end = json!({ "type": "moveSelection", "by": "document", "direction": "forward" });
    b.execute(serde_json::from_value(end).expect("a command"), fonts());
    let token = b.adopt_clipboard(clip.clone()).to_string();
    b.apply_json(&json!({ "type": "paste", "text": clip.text(), "token": token, "seq": b.seq_applied() + 1 }).to_string(), fonts());
    let after = fields(&b);
    assert_eq!(after.len(), before + 1, "the field came across, not its text");
    let pasted = after.iter().find(|f| f.code == first.code && f.id != first.id).expect("same code, a new id");
    assert!(fields(&a).iter().all(|f| f.id != pasted.id));
    assert_eq!(a.history().undo_depth(), depth);
}
