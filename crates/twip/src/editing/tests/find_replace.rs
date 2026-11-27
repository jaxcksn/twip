//! Find and replace: where matches are and what replacing them changes

use super::{blank_body, fonts, para_id, texts};
use crate::api::services::FindQuery;
use crate::editing::Editor;
use crate::editing::position::{Position, StoryId};
use crate::editing::selection::Selection;
use crate::layout::{ParaId, Unit};

fn t(s: &str) -> String {
    format!("<w:r><w:t xml:space=\"preserve\">{s}</w:t></w:r>")
}

fn ed(body: &str) -> Editor {
    Editor::open(&blank_body(body), fonts()).expect("opens")
}

fn query(text: &str, whole: bool) -> FindQuery {
    FindQuery { text: text.into(), match_case: None, whole_words: whole.then_some(true), ..FindQuery::default() }
}

#[test]
fn find_from_outside_the_body_starts_at_its_top() {
    let mut e = ed(&format!("<w:p>{}</w:p><w:p>{}</w:p>", t("apple pie"), t("apple")));
    // A caret in a story the search doesn't cover (a header's paragraph), past the first match's offset.
    let elsewhere = Position::text(StoryId::Body, ParaId(u64::MAX - 1), Unit(20));
    e.set_caret_for_benchmark(Selection::Caret(elsewhere));
    let (hit, wrapped) = crate::services::find::next(&e, &query("apple", false), false).expect("a match");
    assert_eq!((hit.para, hit.start, wrapped), (para_id(&e, 0), 0, false));
    let (hit, wrapped) = crate::services::find::next(&e, &query("apple", false), true).expect("a match");
    assert_eq!((hit.para, hit.start, wrapped), (para_id(&e, 1), 0, false), "going back, from the end");
}

#[test]
fn find_from_a_header_goes_on_past_the_body() {
    let sec = r#"<w:sectPr><w:headerReference w:type="default" r:id="rIdH"/><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr>"#;
    let header = format!("<w:p>{}</w:p>", t("a header line long enough"));
    let mut e = super::stories::package(&format!("<w:p>{}</w:p><w:p>{}</w:p>{sec}", t("apple pie"), t("apple")), Some(Some(&header)));
    let header = *super::parts_named(&e, "header1.xml").first().expect("the header part");
    let k = e.document().stories.keys().position(|n| *n == header).expect("the header part");
    let Some(crate::docx::Block::Para(p)) = e.document().stories.values().nth(k).and_then(|b| b.first()) else { panic!("a header paragraph") };
    let in_header = Position::text(StoryId::HeaderFooter(k as u32), ParaId(p.id), Unit(20));
    e.set_caret_for_benchmark(Selection::Caret(in_header));
    // The headers come after the body: on from one wraps to the top, back from it finds the body's last match.
    let (hit, wrapped) = crate::services::find::next(&e, &query("apple", false), false).expect("a match");
    assert_eq!((hit.para, hit.start, wrapped), (para_id(&e, 0), 0, true));
    let (hit, wrapped) = crate::services::find::next(&e, &query("apple", false), true).expect("a match");
    assert_eq!((hit.para, hit.start, wrapped), (para_id(&e, 1), 0, false));
}

#[test]
fn whole_words_dont_match_before_a_combining_mark() {
    let e = ed(&format!("<w:p>{}</w:p>", t("cafe\u{301} x\u{301}cafe")));
    assert!(crate::services::find::all(&e, &query("cafe", true)).is_empty(), "cafe\u{301} and x\u{301}cafe are other words");
}

#[test]
fn replace_all_never_leaves_a_combining_mark_behind() {
    let mut e = ed(&format!("<w:p>{}</w:p>", t("cafe\u{301} cafe")));
    let (n, _) = e.replace_all("cafe", "tea", false, fonts());
    assert_eq!(n, 1, "only the match that ends on a character's end");
    assert_eq!(texts(&e)[0], "cafe\u{301} tea");
}

#[test]
fn a_selected_match_overlapping_an_earlier_one_is_still_a_match() {
    let mut e = ed(&format!("<w:p>{}</w:p>", t("aaa")));
    let p = para_id(&e, 0);
    e.set_caret_for_benchmark(Selection::range(
        Position::text(StoryId::Body, p, Unit(1)),
        Position::text(StoryId::Body, p, Unit(3)),
        Default::default(),
    ));
    assert!(e.selection_matches("aa", false));
    let hit = crate::services::find::at_selection(&e, &query("aa", false)).expect("the selection is a match");
    assert_eq!((hit.start, hit.end), (1, 3));
}

#[test]
fn a_match_refused_across_a_field_marker_leaves_the_next_one() {
    let field = r#"<w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> QUOTE "aa" </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>aa</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r>"#;
    let e = ed(&format!("<w:p>{}{field}</w:p>", t("a")));
    assert_eq!(crate::services::find::all(&e, &query("aa", false)).len(), 1, "the field's result is found");
}
