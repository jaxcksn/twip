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
    FindQuery { text: text.into(), match_case: None, whole_words: whole.then_some(true) }
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
