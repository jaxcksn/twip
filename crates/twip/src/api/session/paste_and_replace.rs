//! Replacing in links, and pasting a copy made in another session

use crate::api::command::{Command, Picture};
use crate::api::engine::{Engine, SessionId, SessionOptions};
use crate::api::fonts::FontLibrary;
use crate::api::services::FindQuery;
use crate::api::session::ExportOptions;
use crate::api::value::{Bytes, Order};
use crate::docx::{Block, Inline};

fn t(s: &str) -> String {
    format!("<w:r><w:t xml:space=\"preserve\">{s}</w:t></w:r>")
}

fn engine_with(body: &str) -> (Engine, SessionId) {
    let mut engine = Engine::new(FontLibrary::system_recommended());
    let id = engine.create_session(SessionOptions::default());
    engine.session(id).expect("a session").open(&crate::editing::tests::blank_body(body)).expect("opens");
    (engine, id)
}

fn paras(s: &super::Session) -> Vec<crate::docx::Paragraph> {
    s.editor
        .document()
        .sections
        .iter()
        .flat_map(|sec| sec.blocks.iter())
        .filter_map(|b| if let Block::Para(p) = b { Some(p.clone()) } else { None })
        .collect()
}

fn text_of(p: &crate::docx::Paragraph) -> String {
    crate::docx::fields::visible(&p.inlines)
}

fn query(text: &str, whole: bool) -> FindQuery {
    FindQuery { text: text.into(), match_case: None, whole_words: whole.then_some(true), ..FindQuery::default() }
}

fn link(text: &str) -> String {
    format!(
        r#"<w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> HYPERLINK "http://example.com/a/long/address" </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r>{}<w:r><w:fldChar w:fldCharType="end"/></w:r>"#,
        t(text)
    )
}

fn fields(s: &super::Session) -> usize {
    paras(s)[0].inlines.iter().filter(|i| matches!(i, Inline::FieldBegin { .. })).count()
}

#[test]
fn whole_word_replace_all_changes_a_links_text_and_keeps_the_link() {
    let (mut engine, id) = engine_with(&format!("<w:p>{}{}{}</w:p>", t("Click "), link("here and here"), t(" for help")));
    let s = engine.session(id).expect("a session");
    assert_eq!(s.replace_all(&query("here", true), "there", None).map(|c| c.count).expect("replaces"), 2);
    assert_eq!(fields(s), 1, "the link stays: {:?}", paras(s)[0].inlines);
    assert_eq!(text_of(&paras(s)[0]), "Click there and there for help");
}

#[test]
fn replace_changes_a_match_in_a_links_text_and_goes_to_the_next() {
    let (mut engine, id) = engine_with(&format!("<w:p>{}{}{}</w:p>", t("Click "), link("here and here"), t(" for help")));
    let s = engine.session(id).expect("a session");
    let q = query("here", false);
    assert!(s.find(&q, Order::Next).expect("finds").found);
    let next = s.replace(&q, "there", None).expect("replaces");
    assert_eq!(text_of(&paras(s)[0]), "Click there and here for help");
    assert!(next.found, "the second match in the link is next");
    s.replace(&q, "there", None).expect("replaces");
    assert_eq!(text_of(&paras(s)[0]), "Click there and there for help");
    assert_eq!(fields(s), 1, "the link stays");
}

#[test]
fn a_picture_pasted_in_another_session_comes_with_its_image() {
    let (mut engine, a) = engine_with(&format!("<w:p>{}</w:p>", t("pic")));
    let svg = crate::editing::tests::open("FX-OBJ.docx").document().images.last().map(|(_, d)| d.to_vec()).expect("an image");
    let data = {
        let s = engine.session(a).expect("a session");
        s.run(Command::InsertPicture(Picture { data: Bytes(svg.clone()), alt_text: None })).expect("inserts");
        s.run(Command::SelectAll).expect("selects");
        s.copy(false).expect("copies")
    };
    let b = engine.create_session(SessionOptions::default());
    let s = engine.session(b).expect("a session");
    s.open(&crate::editing::tests::blank_body("<w:p/>")).expect("opens");
    s.paste(&data, false);
    let images = |s: &super::Session| -> Vec<Option<crate::docx::ResourceId>> {
        paras(s)
            .iter()
            .flat_map(|p| p.inlines.iter().filter_map(|i| if let Inline::Picture(p, _) = i { Some(p.image) } else { None }).collect::<Vec<_>>())
            .collect()
    };
    let pasted = images(s);
    assert_eq!(pasted.len(), 1, "a picture was pasted");
    let k = pasted[0].expect("it shows an image");
    assert_eq!(s.editor.document().images.get(k), Some(&svg[..]), "its own image");
    let saved = s.export(&ExportOptions::Docx).expect("the document saves with the pasted picture");
    let again = crate::editing::Editor::open(&saved.bytes.0, crate::editing::tests::fonts()).expect("reopens");
    let k = again
        .document()
        .sections
        .iter()
        .flat_map(|sec| sec.blocks.iter())
        .find_map(|b| {
            if let Block::Para(p) = b { p.inlines.iter().find_map(|i| if let Inline::Picture(p, _) = i { p.image } else { None }) } else { None }
        })
        .expect("a picture with its image");
    assert!(!again.document().images.get(k).unwrap().is_empty(), "the saved picture has its image");
}

#[test]
fn find_and_replace_leave_a_links_address_alone() {
    let (mut engine, id) = engine_with(&format!("<w:p>{}{}</w:p>", t("Click "), link("here")));
    let s = engine.session(id).expect("a session");
    let all = |s: &super::Session| crate::editing::text::UnitText::of(&paras(s)[0]).text;
    let before = all(s);
    for whole in [true, false] {
        let q = query("long", whole);
        assert!(!s.find(&q, Order::Next).expect("finds").found, "the address is not searched");
        assert_eq!(s.replace_all(&q, "short", None).map(|c| c.count).expect("replaces"), 0);
        s.replace(&q, "short", None).expect("replaces");
    }
    assert_eq!(all(s), before, "the link's address is unchanged");
    assert_eq!(fields(s), 1);
}
