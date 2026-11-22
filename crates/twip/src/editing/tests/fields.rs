//! Fields: number pictures, results set from rich text, and bookmark texts

use serde_json::{Value, json};

use super::{blank_body, fonts};
use crate::docx::{Block, BreakKind, Inline};
use crate::editing::Editor;

fn field_doc(code: &str, cached: &str) -> Vec<u8> {
    blank_body(&format!(
        r#"<w:p><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve">{code}</w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>{cached}</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r></w:p>"#
    ))
}

fn send(e: &mut Editor, msg: Value) -> Value {
    let mut msg = msg;
    msg["seq"] = json!(e.seq_applied() + 1);
    serde_json::from_str(&e.apply_json(&msg.to_string(), fonts())).expect("a reply")
}

#[test]
fn a_number_picture_pads_with_any_number_of_zeros() {
    let zeros = "0".repeat(65_536);
    let shown = crate::docx::fields::format_field("5", &format!(" SEQ x \\# \"{zeros}\" "), None);
    assert_eq!(shown.len(), 65_536);
    assert!(shown.ends_with("05") && shown.bytes().filter(|b| *b == b'0').count() == 65_535);
    assert_eq!(crate::docx::fields::format_field("-7", " PAGE \\# \"000\" ", None), "-07");
    assert_eq!(crate::docx::fields::format_field("1234", " PAGE \\# \"00\" ", None), "1234");
}

#[test]
fn a_document_with_a_huge_number_picture_opens() {
    let code = format!(" SEQ x \\# \"{}\" ", "0".repeat(65_536));
    let doc = crate::docx::read_editable(&field_doc(&code, "1")).expect("reads");
    let Some(Block::Para(p)) = doc.sections[0].blocks.first() else { panic!("a paragraph") };
    let value = p.inlines.iter().find_map(|i| if let Inline::FieldBegin { value, .. } = i { value.clone() } else { None });
    assert!(value.is_some_and(|v| v.len() == 65_536 && v.ends_with('1')), "the field shows its number");
}

fn result(e: &Editor) -> Vec<Inline> {
    let Some(Block::Para(p)) = e.document().sections[0].blocks.first() else { panic!("a paragraph") };
    let s = crate::docx::fields::spans(&p.inlines)[0];
    p.inlines[s.sep.expect("a separator") + 1..s.end].to_vec()
}

#[test]
fn a_rich_field_result_keeps_its_formatting_and_line_breaks() {
    let mut e = Editor::open(&field_doc(" MERGEFIELD X ", "old"), fonts()).expect("opens");
    let id = send(&mut e, json!({ "type": "query", "q": "elements" }))["answer"][0]["id"].clone();
    let rich = json!({ "paragraphs": [{ "runs": [{ "text": "A", "bold": true }] }, { "runs": [{ "text": "B" }] }] });
    let r = send(&mut e, json!({ "type": "command", "command": { "type": "setFieldResults", "results": [{ "id": id, "display": rich }] } }));
    assert!(r["answer"].get("error").is_none(), "{r}");
    let shown = result(&e);
    assert!(
        matches!(&shown[..], [Inline::Text(a, ra), Inline::Break(BreakKind::Line, _), Inline::Text(b, rb)] if a == "A" && ra.bold && b == "B" && !rb.bold),
        "{shown:?}"
    );
    let plain = send(&mut e, json!({ "type": "command", "command": { "type": "setField", "id": id, "display": "one\ntwo" } }));
    assert!(plain["answer"].get("error").is_none(), "{plain}");
    assert!(matches!(&result(&e)[..], [Inline::Text(a, _), Inline::Break(BreakKind::Line, _), Inline::Text(b, _)] if a == "one" && b == "two"));
}

#[test]
fn bookmarks_in_one_paragraph_are_found_in_linear_time() {
    fn time(n: usize) -> std::time::Duration {
        let starts: String = (0..n).map(|i| format!(r#"<w:bookmarkStart w:id="{i}" w:name="b{i}"/>"#)).collect();
        let ends: String = (0..n).map(|i| format!(r#"<w:bookmarkEnd w:id="{i}"/>"#)).collect();
        // A REF to the last bookmark, so bookmark texts are found.
        let refs = format!(
            r#"<w:p><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> REF b{} </w:instrText></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r></w:p>"#,
            n - 1
        );
        let bytes = blank_body(&format!("<w:p>{starts}<w:r><w:t>x</w:t></w:r>{ends}</w:p>{refs}"));
        let t = std::time::Instant::now();
        let doc = crate::docx::read_editable(&bytes).expect("reads");
        let took = t.elapsed();
        let Some(Block::Para(p)) = doc.sections[0].blocks.get(1) else { panic!("the reference") };
        let value = p.inlines.iter().find_map(|i| if let Inline::FieldBegin { value, .. } = i { value.clone() } else { None });
        assert_eq!(value.as_deref(), Some("x"), "the REF shows its bookmark's text");
        took
    }
    let small = (0..3).map(|_| time(1_000)).min().unwrap_or_default();
    let large = (0..3).map(|_| time(8_000)).min().unwrap_or_default();
    // 8x the bookmarks should cost about 8x; quadratic is about 64x.
    assert!(large < small * 24 + std::time::Duration::from_millis(50), "{small:?} for 1000, {large:?} for 8000");
}

#[test]
fn references_to_many_long_bookmarks_are_found_in_linear_time() {
    fn time(n: usize) -> std::time::Duration {
        // Bookmark i spans runs i.. to the end of the paragraph; each has a REF in a paragraph of its own.
        let marked: String = (0..n).map(|i| format!(r#"<w:bookmarkStart w:id="{i}" w:name="b{i}"/><w:r><w:t>{}</w:t></w:r>"#, i % 10)).collect();
        let ends: String = (0..n).map(|i| format!(r#"<w:bookmarkEnd w:id="{i}"/>"#)).collect();
        let refs: String = (0..n)
            .map(|i| {
                format!(
                    r#"<w:p><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> REF b{i} </w:instrText></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r></w:p>"#
                )
            })
            .collect();
        let bytes = blank_body(&format!("<w:p>{marked}{ends}</w:p>{refs}"));
        let t = std::time::Instant::now();
        let doc = crate::docx::read_editable(&bytes).expect("reads");
        let took = t.elapsed();
        let Some(Block::Para(p)) = doc.sections[0].blocks.get(n - 1) else { panic!("the references") };
        let value = p.inlines.iter().find_map(|i| if let Inline::FieldBegin { value, .. } = i { value.clone() } else { None });
        assert_eq!(value, Some(format!("{}{}", (n - 2) % 10, (n - 1) % 10)), "a REF shows its bookmark's text");
        took
    }
    let small = (0..3).map(|_| time(500)).min().unwrap_or_default();
    let large = (0..3).map(|_| time(4_000)).min().unwrap_or_default();
    // 8x the bookmarks should cost about 8x; quadratic is about 64x.
    assert!(large < small * 24 + std::time::Duration::from_millis(50), "{small:?} for 500, {large:?} for 4000");
}

#[test]
fn a_reference_shows_at_most_the_start_of_a_huge_bookmark() {
    let text = "x".repeat(100_000);
    let body = format!(
        r#"<w:p><w:bookmarkStart w:id="1" w:name="Big"/><w:r><w:t>{text}</w:t></w:r><w:bookmarkEnd w:id="1"/></w:p><w:p><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> REF Big </w:instrText></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r></w:p>"#
    );
    let doc = crate::docx::read_editable(&blank_body(&body)).expect("reads");
    let Some(Block::Para(p)) = doc.sections[0].blocks.get(1) else { panic!("the reference") };
    let value = p.inlines.iter().find_map(|i| if let Inline::FieldBegin { value, .. } = i { value.clone() } else { None });
    assert_eq!(value.map(|v| v.len()), Some(1 << 16));
}

#[test]
fn a_bookmark_is_found_without_case() {
    let body = r#"<w:p><w:bookmarkStart w:id="1" w:name="Total"/><w:r><w:t>42</w:t></w:r><w:bookmarkEnd w:id="1"/></w:p><w:p><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> REF total </w:instrText></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r></w:p>"#;
    let doc = crate::docx::read_editable(&blank_body(body)).expect("reads");
    let Some(Block::Para(p)) = doc.sections[0].blocks.get(1) else { panic!("the reference") };
    let value = p.inlines.iter().find_map(|i| if let Inline::FieldBegin { value, .. } = i { value.clone() } else { None });
    assert_eq!(value.as_deref(), Some("42"));
}
