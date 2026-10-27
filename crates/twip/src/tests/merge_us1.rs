//! Fields and controls through the host API: insert a field and a plain-text control, change each one's
//! format, undo and redo, save, reopen. The code, tag and display text read back the same. With `WORD_CHECK_DIR` set,
//! the saved file goes there too, to check it in another application.

use serde_json::{Value, json};

use crate::font::Fonts;
use crate::host::Engine;

fn send(e: &mut Engine, msg: Value) -> Value {
    let r: Value = serde_json::from_str(&e.send_json(&msg.to_string())).expect("a reply");
    assert!(r.get("error").is_none() && r["answer"].get("error").is_none(), "{msg} -> {r}");
    r
}

fn command(e: &mut Engine, id: &str, arg: Value) -> Value {
    send(e, json!({ "type": "command", "id": id, "arg": arg }))
}

/// Each element's kind, code, tag and display text, in document order.
fn elements(e: &mut Engine) -> Vec<(String, Option<String>, Option<String>, String)> {
    let r = send(e, json!({ "type": "query", "q": "elements" }));
    r["answer"]
        .as_array()
        .expect("elements")
        .iter()
        .map(|x| {
            let s = |k: &str| x[k].as_str().map(str::to_string);
            (s("kind").unwrap_or_default(), s("code"), s("tag"), s("display").unwrap_or_default())
        })
        .collect()
}

#[test]
fn insert_change_undo_save_reopen() {
    let bytes = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/FX-TEXT.docx")).unwrap();
    let mut e = Engine::with_fonts(Fonts::system());
    e.open(&bytes);
    let f =
        command(&mut e, "field.insert", json!({ "code": "MERGEFIELD Statement.Cost_Amount", "display": "\u{ab}Cost_Amount\u{bb}" }))["answer"]["id"]
            .clone();
    let c = command(&mut e, "control.insert", json!({ "kind": "plain_text", "tag": "Statement/Cost_Amount", "placeholder": "Cost" }))["answer"]["id"]
        .clone();
    command(&mut e, "field.set", json!({ "id": f, "code": r#"MERGEFIELD Statement.Cost_Amount \# "$,0.00""# }));
    command(&mut e, "control.set", json!({ "id": c, "tag": "Statement/Cost_Amount:currency" }));
    let want = elements(&mut e);
    for _ in 0..2 {
        command(&mut e, "home.undo", Value::Null);
    }
    assert_ne!(elements(&mut e), want, "undone");
    for _ in 0..2 {
        command(&mut e, "home.redo", Value::Null);
    }
    assert_eq!(elements(&mut e), want, "redone");
    assert!(
        want.contains(&("field".into(), Some(r#" MERGEFIELD Statement.Cost_Amount \# "$,0.00" "#.into()), None, "\u{ab}Cost_Amount\u{bb}".into())),
        "{want:?}"
    );
    assert!(want.contains(&("plain_text".into(), None, Some("Statement/Cost_Amount:currency".into()), "Cost".into())), "{want:?}");
    let saved = e.save().unwrap();
    if let Some(d) = std::env::var_os("WORD_CHECK_DIR") {
        std::fs::write(std::path::Path::new(&d).join("US1-HOST.docx"), &saved).unwrap();
    }
    let mut again = Engine::with_fonts(Fonts::system());
    again.open(&saved);
    assert_eq!(elements(&mut again), want, "read back");
}
