//! Fields: number pictures, results set from rich text, and bookmark texts

use crate::docx::origin::EdgeOrigin;
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
    let (doc, _) = crate::docx::read_editable(&field_doc(&code, "1")).expect("reads");
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
fn a_rich_field_result_links_only_to_addresses_to_follow() {
    let mut e = Editor::open(&field_doc(" MERGEFIELD X ", "old"), fonts()).expect("opens");
    let id = send(&mut e, json!({ "type": "query", "q": "elements" }))["answer"][0]["id"].clone();
    let rich = json!({ "paragraphs": [{ "runs": [{ "text": "bad", "link": "java\tscript:alert(1)" }, { "text": "good", "link": "https://x.test/a\\b" }] }] });
    let r = send(&mut e, json!({ "type": "command", "command": { "type": "setFieldResults", "results": [{ "id": id, "display": rich }] } }));
    assert!(r["answer"].get("error").is_none(), "{r}");
    let shown = result(&e);
    let links: Vec<String> = crate::docx::fields::spans(&shown)
        .into_iter()
        .map(|s| crate::docx::fields::parse(&crate::docx::fields::code_text(&shown, s)).args.first().cloned().unwrap_or_default())
        .collect();
    assert_eq!(links, ["https://x.test/a\\b"], "the script address is text: {shown:?}");
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
        let (doc, _) = crate::docx::read_editable(&bytes).expect("reads");
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
        let (doc, _) = crate::docx::read_editable(&bytes).expect("reads");
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
    let (doc, _) = crate::docx::read_editable(&blank_body(&body)).expect("reads");
    let Some(Block::Para(p)) = doc.sections[0].blocks.get(1) else { panic!("the reference") };
    let value = p.inlines.iter().find_map(|i| if let Inline::FieldBegin { value, .. } = i { value.clone() } else { None });
    assert_eq!(value.map(|v| v.len()), Some(1 << 16));
}

#[test]
fn a_bookmark_is_found_without_case() {
    let body = r#"<w:p><w:bookmarkStart w:id="1" w:name="Total"/><w:r><w:t>42</w:t></w:r><w:bookmarkEnd w:id="1"/></w:p><w:p><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> REF total </w:instrText></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r></w:p>"#;
    let (doc, _) = crate::docx::read_editable(&blank_body(body)).expect("reads");
    let Some(Block::Para(p)) = doc.sections[0].blocks.get(1) else { panic!("the reference") };
    let value = p.inlines.iter().find_map(|i| if let Inline::FieldBegin { value, .. } = i { value.clone() } else { None });
    assert_eq!(value.as_deref(), Some("42"));
}

#[test]
fn a_field_reads_back_its_view_and_staleness_and_an_update_clears_it() {
    let doc = blank_body(
        r#"<w:p><w:r><w:fldChar w:fldCharType="begin" w:dirty="1"/></w:r><w:r><w:instrText xml:space="preserve"> MERGEFIELD X </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>old</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r></w:p>"#,
    );
    let mut e = Editor::open(&doc, fonts()).expect("opens");
    let field = |e: &Editor| {
        let all = crate::editing::elements::enumerate(e.document(), e.index(), &Default::default());
        let f = all.into_iter().find(|x| x.code.is_some()).expect("a field");
        (f.showing_code, f.dirty)
    };
    assert_eq!(field(&e), (false, true), "dirty as read");
    let r = send(&mut e, json!({ "type": "command", "command": { "type": "showFieldCodes", "show": true } }));
    assert!(r["answer"].get("error").is_none(), "{r}");
    assert_eq!(field(&e), (true, true), "showing its code");
    let r = send(&mut e, json!({ "type": "command", "command": { "type": "updateFields" } }));
    assert!(r["answer"].get("error").is_none(), "{r}");
    assert_eq!(field(&e), (false, true), "an update shows the result; the application's answer is still to come");
    let id = send(&mut e, json!({ "type": "query", "q": "elements" }))["answer"][0]["id"].clone();
    let r = send(&mut e, json!({ "type": "command", "command": { "type": "setFieldResults", "results": [{ "id": id, "display": "new" }] } }));
    assert!(r["answer"].get("error").is_none(), "{r}");
    assert_eq!(field(&e), (false, false), "a result set clears the flag");
    let r = send(&mut e, json!({ "type": "command", "command": { "type": "undo" } }));
    assert!(r["answer"].get("error").is_none(), "{r}");
    assert_eq!(field(&e), (false, true), "undoing the result brings the stale flag back");
    let r = send(&mut e, json!({ "type": "command", "command": { "type": "undo" } }));
    assert!(r["answer"].get("error").is_none(), "{r}");
    assert_eq!(field(&e), (true, true));
}

#[test]
fn insert_field_opens_on_the_field_at_the_selection() {
    let body = r#"<w:p><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> SEQ Figure \r 3 </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>3</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r></w:p>"#;
    let mut e = Editor::open(&super::blank_body(body), super::fonts()).expect("opens");
    use crate::editing::position::{Position, StoryId};
    e.selection = crate::editing::selection::Selection::Caret(Position::text(StoryId::Body, super::para_id(&e, 0), crate::layout::Unit(1)));
    let (_, c) = crate::services::fields::choice_here(&e).expect("a field");
    assert_eq!((c.name.as_str(), c.args.as_slice()), ("SEQ", &["Figure".to_string()][..]));
    assert_eq!(c.switches, [("r".to_string(), Some("3".to_string()))]);
}

fn fld(code: &str, shown: &str) -> String {
    format!(
        r#"<w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> {code} </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t xml:space="preserve">{shown}</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r>"#
    )
}

#[test]
fn one_update_renumbers_a_sequence_and_the_reference_to_it() {
    use crate::editing::commands::{Command, Dir};
    let figure = |n: &str| format!(r#"<w:r><w:t xml:space="preserve">Figure </w:t></w:r>{}"#, fld(r"SEQ Figure \* ARABIC", n));
    let body = format!(
        r#"<w:p>{}</w:p><w:p><w:bookmarkStart w:id="1" w:name="_Ref1"/>{}<w:bookmarkEnd w:id="1"/></w:p><w:p><w:r><w:t xml:space="preserve">See </w:t></w:r>{}</w:p>"#,
        figure("1"),
        figure("2"),
        fld(r"REF _Ref1 \h", "Figure 2")
    );
    let mut e = Editor::open(&blank_body(&body), fonts()).expect("opens");
    let (first, second) = (super::para_id(&e, 0), super::para_id(&e, 1));
    let at = |p| crate::editing::position::Position::text(crate::editing::position::StoryId::Body, p, crate::layout::Unit(0));
    e.set_caret_for_benchmark(crate::editing::selection::Selection::range(at(first), at(second), crate::editing::selection::Granularity::Char));
    e.execute(super::del(Dir::Forward, false), fonts());
    e.execute(Command::SelectAll, fonts());
    e.execute(Command::UpdateFields(Default::default()), fonts());
    let shown = super::texts(&e);
    assert!(shown[0].ends_with("\u{fffc}1\u{fffc}") && shown[1].ends_with("\u{fffc}Figure 1\u{fffc}"), "{shown:?}");
}

#[test]
fn spanning_fields_ending_in_one_run_keep_each_its_own_end() {
    let begin = |text: &str| {
        let f = fld("X", "");
        format!("<w:p>{}<w:r><w:t>{text}</w:t></w:r></w:p>", &f[..f.find("<w:r><w:t").expect("its result")])
    };
    let body = format!(
        r#"{}{}<w:p><w:r><w:t>c</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/><w:fldChar w:fldCharType="end"/></w:r></w:p>"#,
        begin("a"),
        begin("b")
    );
    let (doc, _) = crate::docx::read_editable(&blank_body(&body)).expect("reads");
    let ends: Vec<usize> = doc.sections[0]
        .blocks
        .iter()
        .filter_map(|b| if let Block::Para(p) = b { Some(p) } else { None })
        .flat_map(|p| p.inlines.iter())
        .filter_map(|i| match i {
            Inline::FieldEdge { edge, .. } if edge.kind == crate::docx::FieldEdge::End => {
                Some(edge.origin_xml().map_or(1, |x| x.matches("fldCharType=\"end\"").count()))
            }
            _ => None,
        })
        .collect();
    assert_eq!(ends, [1, 1], "each written with its own marker");
}
