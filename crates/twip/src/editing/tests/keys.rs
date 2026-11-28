//! Word's keyboard surface: shortcuts and what they do

use super::super::Editor;
use super::super::commands::Outcome;
use super::super::keymap::Mods;
use super::super::position::{Position, StoryId};
use super::super::selection::Selection;
use super::{blank_body, fonts, texts, texts_ids};
use crate::api::command::{CaseChange, Command as C, InsertField};
use crate::document::paragraph::{ListId, ListRef};
use crate::docx::Inline;
use crate::docx::para_props::ParaXml;
use crate::layout::Unit;

const STYLES: &str = concat!(
    r#"<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/><w:rPr><w:sz w:val="22"/></w:rPr></w:style>"#,
    r#"<w:style w:type="paragraph" w:styleId="Title"><w:name w:val="Title"/><w:basedOn w:val="Normal"/><w:pPr><w:spacing w:after="80"/></w:pPr><w:rPr><w:sz w:val="56"/></w:rPr></w:style>"#,
);

fn styled(body: &str) -> Editor {
    super::style_model::open_pkg(&super::style_model::pkg(STYLES, body))
}

fn edit(body: &str) -> Editor {
    let mut e = Editor::open(&blank_body(body), fonts()).expect("opens");
    e.complete(fonts());
    e
}

const CTRL: Mods = Mods { ctrl: true, shift: false, alt: false, meta: false };
const CTRL_SHIFT: Mods = Mods { ctrl: true, shift: true, alt: false, meta: false };
const CTRL_ALT: Mods = Mods { ctrl: true, shift: false, alt: true, meta: false };
const ALT_SHIFT: Mods = Mods { ctrl: false, shift: true, alt: true, meta: false };
const SHIFT: Mods = Mods { ctrl: false, shift: true, alt: false, meta: false };

fn select(e: &mut Editor, para: usize, a: u32, b: u32) {
    let id = texts_ids(e)[para];
    e.selection = Selection::range(Position::text(StoryId::Body, id, Unit(a)), Position::text(StoryId::Body, id, Unit(b)), Default::default());
}

fn caret(e: &mut Editor, para: usize, at: u32) {
    e.selection = Selection::Caret(Position::text(StoryId::Body, texts_ids(e)[para], Unit(at)));
}

fn press(e: &mut Editor, code: &str, mods: Mods) {
    let c = e.key_command(code, mods).unwrap_or_else(|| panic!("{code} {mods:?} is bound"));
    e.execute(c, fonts());
}

fn para(e: &Editor, n: usize) -> &crate::docx::Paragraph {
    e.index().para(e.document(), texts_ids(e)[n]).expect("para")
}

fn sizes(e: &Editor, n: usize) -> Vec<f64> {
    para(e, n).inlines.iter().filter_map(|i| if let Inline::Text(_, r) = i { Some(r.size) } else { None }).collect()
}

#[test]
fn documented_shortcuts_are_bound() {
    let mut e = edit("<w:p><w:r><w:t>one two</w:t></w:r></w:p>");
    assert_eq!(e.key_command("KeyM", CTRL_ALT), Some(C::AddComment(crate::api::review::Body::Text(String::new()))));
    assert_eq!(e.key_command("KeyC", CTRL_SHIFT), Some(C::CopyFormatting));
    assert_eq!(e.key_command("KeyM", CTRL), Some(C::IncreaseIndent), "Ctrl+M stays");
    caret(&mut e, 0, 1);
    press(&mut e, "KeyM", CTRL_ALT);
    assert_eq!(e.document().comments.by_id.len(), 1, "a comment on the word at the caret");
}

#[test]
fn special_characters_and_fields_have_their_keys() {
    let mut e = edit("<w:p><w:r><w:t>x</w:t></w:r></w:p>");
    let text = |t: &str| Some(C::InsertText(t.into()));
    assert_eq!(e.key_command("Space", CTRL_SHIFT), text("\u{A0}"));
    assert_eq!(e.key_command("Minus", CTRL_SHIFT), text("\u{2011}"));
    assert_eq!(e.key_command("Minus", CTRL), text("\u{AD}"));
    assert_eq!(e.key_command("NumpadSubtract", CTRL), text("\u{2013}"));
    assert_eq!(e.key_command("NumpadSubtract", CTRL_ALT), text("\u{2014}"));
    assert_eq!(e.key_command("KeyC", CTRL_ALT), text("\u{A9}"));
    assert_eq!(e.key_command("KeyR", CTRL_ALT), text("\u{AE}"));
    assert_eq!(e.key_command("KeyT", CTRL_ALT), text("\u{2122}"));
    assert_eq!(e.key_command("Period", CTRL_ALT), text("\u{2026}"));
    let field = |code: &str| Some(C::InsertField(InsertField { code: code.into(), display: None, show_code: None }));
    assert_eq!(e.key_command("KeyD", ALT_SHIFT), field("DATE"));
    assert_eq!(e.key_command("KeyT", ALT_SHIFT), field("TIME"));
    assert_eq!(e.key_command("KeyP", ALT_SHIFT), field("PAGE"));
    assert_eq!(e.key_command("KeyL", CTRL_ALT), field("LISTNUM"));
    caret(&mut e, 0, 1);
    press(&mut e, "KeyT", CTRL_ALT);
    assert_eq!(texts(&e)[0], "x\u{2122}");
}

#[test]
fn shift_f3_cycles_lower_upper_and_capitalised() {
    let mut e = edit("<w:p><w:r><w:t>hello world</w:t></w:r></w:p>");
    select(&mut e, 0, 0, 11);
    let mut seen = Vec::new();
    for _ in 0..4 {
        press(&mut e, "F3", SHIFT);
        seen.push(texts(&e)[0].clone());
    }
    assert_eq!(seen, ["HELLO WORLD", "Hello World", "hello world", "HELLO WORLD"]);
    let mut e = edit("<w:p><w:r><w:t>HELLO WORLD.</w:t></w:r></w:p>");
    select(&mut e, 0, 0, 12);
    press(&mut e, "F3", SHIFT);
    assert_eq!(texts(&e)[0], "Hello world.", "a sentence takes Sentence case");
}

#[test]
fn shift_f3_ignores_field_codes_when_it_picks_the_case() {
    let mut e =
        edit(&format!(r#"<w:p><w:r><w:t xml:space="preserve">hello </w:t></w:r>{}</w:p>"#, field_xml(r#"HYPERLINK "http://X.COM""#, "world")));
    let n = crate::editing::step::para_len(para(&e, 0)).0;
    select(&mut e, 0, 0, n);
    press(&mut e, "F3", SHIFT);
    assert_eq!(
        texts(&e)[0],
        "HELLO \u{FFFC} HYPERLINK \"http://X.COM\" \u{FFFC}WORLD\u{FFFC}",
        "lowercase text goes to upper, whatever the code's case"
    );
}

#[test]
fn change_case_at_a_caret_changes_its_word() {
    let mut e = edit("<w:p><w:r><w:t>one two three</w:t></w:r></w:p>");
    caret(&mut e, 0, 5);
    press(&mut e, "F3", SHIFT);
    assert_eq!(texts(&e)[0], "one TWO three");
    assert_eq!(e.selection, Selection::Caret(Position::text(StoryId::Body, texts_ids(&e)[0], Unit(5))), "the caret stays");
    caret(&mut e, 0, 3);
    e.execute(C::ChangeCase(CaseChange::Title), fonts());
    assert_eq!(texts(&e)[0], "One TWO three", "the word the caret ends");
    caret(&mut e, 0, 13);
    e.execute(C::Undo(Default::default()), fonts());
    assert_eq!(texts(&e)[0], "one TWO three", "one undo step");
}

#[test]
fn double_and_word_underlines_toggle() {
    let mut e = edit("<w:p><w:r><w:t>one two</w:t></w:r></w:p>");
    let underline = |e: &Editor| para(e, 0).inlines.iter().find_map(|i| i.run().map(|r| r.underline)).flatten();
    select(&mut e, 0, 0, 7);
    press(&mut e, "KeyD", CTRL_SHIFT);
    assert_eq!(underline(&e), Some(crate::docx::Underline::Double));
    press(&mut e, "KeyD", CTRL_SHIFT);
    assert_eq!(underline(&e), None);
    press(&mut e, "KeyW", CTRL_SHIFT);
    assert_eq!(underline(&e), Some(crate::docx::Underline::Words));
    press(&mut e, "KeyQ", CTRL_SHIFT);
    assert!(para(&e, 0).inlines.iter().filter_map(|i| i.run()).all(|r| r.ascii == "Symbol"));
}

#[test]
fn font_size_steps_a_point_per_run() {
    let mut e =
        edit(r#"<w:p><w:r><w:rPr><w:sz w:val="22"/></w:rPr><w:t>ab</w:t></w:r><w:r><w:rPr><w:sz w:val="29"/></w:rPr><w:t>cd</w:t></w:r></w:p>"#);
    select(&mut e, 0, 0, 4);
    e.step_font_size(true, fonts());
    assert_eq!(sizes(&e, 0), [12.0, 15.5]);
    e.step_font_size(false, fonts());
    e.step_font_size(false, fonts());
    assert_eq!(sizes(&e, 0), [10.0, 13.5]);
    let mut e = edit(r#"<w:p><w:r><w:rPr><w:sz w:val="2"/></w:rPr><w:t>ab</w:t></w:r></w:p>"#);
    select(&mut e, 0, 0, 2);
    e.step_font_size(false, fonts());
    assert_eq!(sizes(&e, 0), [1.0], "no smaller than a point");
}

#[test]
fn increase_indent_steps_along_the_default_tab_stops() {
    let mut e = edit(r#"<w:p><w:pPr><w:ind w:left="200" w:firstLine="360"/></w:pPr><w:r><w:t>x</w:t></w:r></w:p>"#);
    caret(&mut e, 0, 0);
    press(&mut e, "KeyM", CTRL);
    assert_eq!((para(&e, 0).style.ind_left, para(&e, 0).style.ind_first), (36.0, 18.0), "the next stop; the first line keeps its indent");
    e.execute(C::IncreaseIndent, fonts());
    assert_eq!(para(&e, 0).style.ind_left, 72.0);
    e.execute(C::SetDefaultTabStop(crate::api::value::Points(50.0)), fonts());
    e.execute(C::DecreaseIndent, fonts());
    assert_eq!(para(&e, 0).style.ind_left, 50.0, "the document's default tab stop");
    e.execute(C::DecreaseIndent, fonts());
    e.execute(C::DecreaseIndent, fonts());
    assert_eq!(para(&e, 0).style.ind_left, 0.0);
}

#[test]
fn increase_indent_changes_a_list_items_level_or_moves_the_whole_list() {
    let mut e = edit("<w:p><w:r><w:t>one</w:t></w:r></w:p><w:p><w:r><w:t>two</w:t></w:r></w:p>");
    select(&mut e, 0, 0, 3);
    let id = |e: &Editor| texts_ids(e)[1];
    e.selection = Selection::range(
        Position::text(StoryId::Body, texts_ids(&e)[0], Unit(0)),
        Position::text(StoryId::Body, id(&e), Unit(3)),
        Default::default(),
    );
    e.execute(C::SetList(Some(crate::api::command::ListKind::Bullet)), fonts());
    let level = |e: &Editor, n: usize| para(e, n).style.num.map(|ListRef { list: ListId(_), level: l }| l);
    caret(&mut e, 1, 0);
    e.execute(C::IncreaseIndent, fonts());
    assert_eq!((level(&e, 0), level(&e, 1)), (Some(0), Some(1)), "a later item goes a level down");
    e.execute(C::DecreaseIndent, fonts());
    assert_eq!(level(&e, 1), Some(0));
    let left = |e: &Editor, n: usize| para(e, n).style.ind_left;
    let (a, b) = (left(&e, 0), left(&e, 1));
    caret(&mut e, 0, 0);
    e.execute(C::IncreaseIndent, fonts());
    assert_eq!((level(&e, 0), level(&e, 1)), (Some(0), Some(0)), "the first item moves the list");
    assert_eq!((left(&e, 0), left(&e, 1)), (a + 36.0, b + 36.0));
    e.execute(C::Undo(Default::default()), fonts());
    assert_eq!(left(&e, 0), a, "one undo step");
}

#[test]
fn increase_indent_on_list_items_and_plain_paragraphs_together() {
    let mut e = edit("<w:p><w:r><w:t>one</w:t></w:r></w:p><w:p><w:r><w:t>two</w:t></w:r></w:p><w:p><w:r><w:t>plain</w:t></w:r></w:p>");
    e.selection = Selection::range(
        Position::text(StoryId::Body, texts_ids(&e)[0], Unit(0)),
        Position::text(StoryId::Body, texts_ids(&e)[1], Unit(3)),
        Default::default(),
    );
    e.execute(C::SetList(Some(crate::api::command::ListKind::Bullet)), fonts());
    let level = |e: &Editor, n: usize| para(e, n).style.num.map(|ListRef { list: ListId(_), level: l }| l);
    let item_left = para(&e, 1).style.ind_left;
    e.selection = Selection::range(
        Position::text(StoryId::Body, texts_ids(&e)[1], Unit(0)),
        Position::text(StoryId::Body, texts_ids(&e)[2], Unit(5)),
        Default::default(),
    );
    e.execute(C::IncreaseIndent, fonts());
    assert_eq!(level(&e, 1), Some(1), "the item goes a level down");
    let item = para(&e, 1).style.direct.xml(&Default::default(), &e.cx()).map(std::borrow::Cow::into_owned);
    assert!(item.is_none_or(|x| !x.contains("w:ind ")), "no direct indent on the item");
    assert_eq!(para(&e, 2).style.ind_left, 36.0, "the plain paragraph steps to the next tab stop");
    e.execute(C::Undo(Default::default()), fonts());
    assert_eq!((level(&e, 1), para(&e, 1).style.ind_left, para(&e, 2).style.ind_left), (Some(0), item_left, 0.0), "one undo step");
}

#[test]
fn hanging_indent_keys_move_the_other_lines_a_tab_stop() {
    let mut e = edit("<w:p><w:r><w:t>x</w:t></w:r></w:p>");
    caret(&mut e, 0, 0);
    let ind = |e: &Editor| (para(e, 0).style.ind_left, para(e, 0).style.ind_first);
    e.hanging_indent(true, fonts());
    assert_eq!(ind(&e), (36.0, -36.0));
    e.hanging_indent(true, fonts());
    assert_eq!(ind(&e), (72.0, -72.0), "the first line stays at the margin");
    e.hanging_indent(false, fonts());
    e.hanging_indent(false, fonts());
    assert_eq!(ind(&e), (0.0, 0.0));
}

#[test]
fn clearing_paragraph_formatting_keeps_the_style() {
    let mut e = styled(
        r#"<w:p><w:pPr><w:pStyle w:val="Title"/><w:jc w:val="center"/><w:ind w:left="720"/><w:spacing w:after="480"/></w:pPr><w:r><w:rPr><w:b/><w:highlight w:val="yellow"/></w:rPr><w:t>x</w:t></w:r></w:p>"#,
    );
    caret(&mut e, 0, 0);
    e.clear_paragraph_formatting(fonts());
    let p = para(&e, 0);
    assert_eq!(p.style.style_id.as_deref(), Some("Title"));
    assert_eq!((p.style.ind_left, p.style.jc, p.style.space_after), (0.0, crate::docx::props::Jc::Left, 4.0), "the style's spacing");
    let run = p.inlines.iter().find_map(|i| i.run()).expect("a run");
    assert!(run.bold, "character formatting stays");
}

#[test]
fn clear_all_formatting_leaves_plain_text_and_its_highlight() {
    let mut e = styled(
        r#"<w:p><w:pPr><w:pStyle w:val="Title"/><w:jc w:val="center"/></w:pPr><w:r><w:rPr><w:b/><w:sz w:val="40"/><w:highlight w:val="yellow"/></w:rPr><w:t>xy</w:t></w:r></w:p>"#,
    );
    select(&mut e, 0, 0, 2);
    e.clear_all_formatting(fonts());
    let p = para(&e, 0);
    assert_eq!(p.style.style_id.as_deref(), Some("Normal"), "the default paragraph style");
    assert_eq!(p.style.jc, crate::docx::props::Jc::Left);
    let run = p.inlines.iter().find_map(|i| i.run()).expect("a run");
    assert!(!run.bold && run.size == 11.0 && run.highlight.is_some(), "{run:?}");
    e.execute(C::Undo(Default::default()), fonts());
    assert_eq!(para(&e, 0).style.style_id.as_deref(), Some("Title"), "one undo step");
}

#[test]
fn list_bullet_comes_with_its_bullet() {
    let mut e = edit("<w:p><w:r><w:t>x</w:t></w:r></w:p>");
    caret(&mut e, 0, 0);
    press(&mut e, "KeyL", CTRL_SHIFT);
    let p = para(&e, 0);
    assert_eq!(p.style.style_id.as_deref(), Some("ListBullet"));
    assert!(p.style.num.is_some_and(|ListRef { list: ListId(id), level: _ }| id != 0), "{:?}", p.style.num);
    assert_eq!(p.list.as_ref().map(|l| l.text.as_str()), Some("\u{F0B7}"));
    assert_eq!(p.style.ind_left, 18.0);
    e.execute(C::Undo(Default::default()), fonts());
    assert!(!e.document().resolver.as_ref().expect("styles").has_style("ListBullet"), "one undo step");
}

#[test]
fn alt_x_toggles_the_code_before_the_caret() {
    let mut e = edit("<w:p><w:r><w:t>caf00e9 x</w:t></w:r></w:p>");
    caret(&mut e, 0, 7);
    e.toggle_character_code(fonts());
    assert_eq!(texts(&e)[0], "ca\u{F00E9} x", "the longest code there is: af00e9 is past U+10FFFF");
    e.execute(C::Undo(Default::default()), fonts());
    let mut e = edit("<w:p><w:r><w:t>a U+00e9.</w:t></w:r></w:p>");
    caret(&mut e, 0, 8);
    e.toggle_character_code(fonts());
    assert_eq!(texts(&e)[0], "a é.");
    assert_eq!(e.selection, Selection::Caret(Position::text(StoryId::Body, texts_ids(&e)[0], Unit(3))));
    e.toggle_character_code(fonts());
    assert_eq!(texts(&e)[0], "a 00E9.", "and back");
    e.execute(C::Undo(Default::default()), fonts());
    assert_eq!(texts(&e)[0], "a é.", "each its own undo step");
}

#[test]
fn alt_x_undo_restores_the_original_caret() {
    let mut e = edit("<w:p><w:r><w:t>a 00e9</w:t></w:r></w:p>");
    caret(&mut e, 0, 6);
    let before = e.selection;
    e.toggle_character_code(fonts());
    e.execute(C::Undo(Default::default()), fonts());
    assert_eq!(texts(&e)[0], "a 00e9");
    assert_eq!(e.selection, before, "the caret, not the code selected");
}

fn field_xml(code: &str, result: &str) -> String {
    format!(
        r#"<w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> {code} </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>{result}</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r>"#
    )
}

#[test]
fn f11_selects_the_next_and_previous_field() {
    let body = format!(
        r#"<w:p><w:r><w:t xml:space="preserve">a </w:t></w:r>{}<w:r><w:t xml:space="preserve"> b </w:t></w:r>{}</w:p><w:p><w:r><w:t xml:space="preserve">c </w:t></w:r>{}</w:p>"#,
        field_xml("PAGE", "1"),
        field_xml("NUMPAGES", "2"),
        field_xml("DATE", "today"),
    );
    let mut e = edit(&body);
    caret(&mut e, 0, 0);
    let selected = |e: &Editor| e.selection.ordered(e.index()).map(|(a, b)| (a.para_offset().unwrap(), b.para_offset().unwrap()));
    let ids = texts_ids(&e);
    let mut starts = Vec::new();
    for _ in 0..4 {
        e.select_field(crate::api::value::Order::Next, fonts());
        let ((pa, a), (pb, b)) = selected(&e).expect("a selection");
        assert_eq!(pa, pb);
        assert!(a < b, "a whole field");
        starts.push((ids.iter().position(|&p| p == pa).unwrap(), a.0));
    }
    assert_eq!(starts.len(), 4);
    assert_eq!(starts[2], starts[3], "no further than the last");
    assert!(starts[0] < starts[1] && starts[1] < starts[2] && starts[2].0 == 1);
    e.select_field(crate::api::value::Order::Previous, fonts());
    let ((pa, a), _) = selected(&e).unwrap();
    assert_eq!((ids.iter().position(|&p| p == pa).unwrap(), a.0), starts[1]);
}

#[test]
fn f11_skips_nested_fields_and_walks_many_paragraphs() {
    let nested = format!(
        r#"<w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> IF </w:instrText></w:r>{}<w:r><w:instrText xml:space="preserve"> = 1 "y" "n" </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>y</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r>"#,
        field_xml("PAGE", "1")
    );
    let mut body = format!("<w:p><w:r><w:t xml:space=\"preserve\">a </w:t></w:r>{nested}</w:p>");
    for k in 0..2000 {
        body.push_str(&format!("<w:p><w:r><w:t>p{k} </w:t></w:r>{}</w:p>", field_xml("PAGE", "1")));
    }
    let mut e = edit(&body);
    let ids = texts_ids(&e);
    caret(&mut e, 0, 0);
    let selected = |e: &Editor| e.selection.ordered(e.index()).map(|(a, b)| (a.para_offset().unwrap(), b.para_offset().unwrap())).unwrap();
    e.select_field(crate::api::value::Order::Next, fonts());
    let ((pa, a), (pb, b)) = selected(&e);
    assert_eq!((pa, pb, a), (ids[0], ids[0], Unit(2)));
    assert_eq!(
        b,
        Unit(e.index().para(e.document(), ids[0]).map(|p| p.inlines.iter().map(|i| i.units() as u32).sum()).unwrap()),
        "the outer field, whole"
    );
    e.select_field(crate::api::value::Order::Next, fonts());
    assert_eq!(selected(&e).0.0, ids[1], "past the field nested in it");
    caret(&mut e, 1500, 0);
    e.select_field(crate::api::value::Order::Previous, fonts());
    assert_eq!(selected(&e).0.0, ids[1499]);
    e.select_field(crate::api::value::Order::Previous, fonts());
    assert_eq!(selected(&e).0.0, ids[1498]);
}

#[test]
fn paragraphs_move_up_and_down_whole() {
    let mut e = edit("<w:p><w:r><w:t>one</w:t></w:r></w:p><w:p><w:r><w:t>two</w:t></w:r></w:p><w:p><w:r><w:t>three</w:t></w:r></w:p>");
    select(&mut e, 1, 1, 2);
    let sel = e.selection;
    e.move_paragraphs(true, fonts());
    assert_eq!(texts(&e), ["two", "one", "three"]);
    assert_eq!(e.selection, sel, "the paragraph stays selected");
    e.move_paragraphs(true, fonts());
    assert_eq!(texts(&e), ["two", "one", "three"], "at the story's start");
    e.move_paragraphs(false, fonts());
    e.move_paragraphs(false, fonts());
    assert_eq!(texts(&e), ["one", "three", "two"], "the story's last paragraph too");
    e.execute(C::Undo(Default::default()), fonts());
    assert_eq!(texts(&e), ["one", "two", "three"], "each move its own undo step");
}

#[test]
fn a_paragraph_moves_past_a_table_as_one() {
    let table = r#"<w:tbl><w:tblGrid><w:gridCol w:w="2000"/></w:tblGrid><w:tr><w:tc><w:p><w:r><w:t>cell</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#;
    let mut e = edit(&format!("<w:p><w:r><w:t>above</w:t></w:r></w:p>{table}<w:p><w:r><w:t>below</w:t></w:r></w:p>"));
    caret(&mut e, 2, 0);
    e.move_paragraphs(true, fonts());
    assert_eq!(texts(&e), ["above", "below", "cell", ""], "an empty paragraph ends the story");
}

#[test]
fn a_tracked_paragraph_move_is_a_move() {
    let mut e = edit("<w:p><w:r><w:t>one</w:t></w:r></w:p><w:p><w:r><w:t>two</w:t></w:r></w:p><w:p><w:r><w:t>three</w:t></w:r></w:p>");
    e.execute(C::SetTrackChanges(true), fonts());
    caret(&mut e, 1, 0);
    e.move_paragraphs(true, fonts());
    let moved = e.document().sections[0].blocks.iter().filter_map(|b| if let crate::docx::Block::Para(p) = b { Some(p) } else { None }).any(|p| {
        p.inlines.iter().filter_map(|i| i.run()).any(|r| {
            [&r.rev.ins, &r.rev.del]
                .into_iter()
                .flatten()
                .any(|m| matches!(m.kind, crate::docx::review::MarkKind::MoveFrom | crate::docx::review::MarkKind::MoveTo))
        })
    });
    assert!(moved, "moved text is marked as moved");
    let visible: Vec<String> = texts(&e);
    assert!(visible.iter().position(|t| t == "two") < visible.iter().position(|t| t == "one"), "{visible:?}");
    e.execute(C::Undo(Default::default()), fonts());
    assert_eq!(texts(&e), ["one", "two", "three"]);
}

/// Three paragraphs, the middle one centred, changes tracked.
fn tracked_three() -> Editor {
    let mut e = edit(concat!(
        "<w:p><w:r><w:t>one</w:t></w:r></w:p>",
        r#"<w:p><w:pPr><w:jc w:val="center"/></w:pPr><w:r><w:t>two</w:t></w:r></w:p>"#,
        "<w:p><w:r><w:t>three</w:t></w:r></w:p>",
    ));
    e.execute(C::SetTrackChanges(true), fonts());
    e
}

fn has_moves(e: &Editor) -> (bool, bool) {
    let (mut to, mut from) = (false, false);
    for b in &e.document().sections[0].blocks {
        let crate::docx::Block::Para(p) = b else { continue };
        for r in p.inlines.iter().filter_map(|i| i.run()) {
            to |= r.rev.ins.as_ref().is_some_and(|m| m.kind == crate::docx::review::MarkKind::MoveTo);
            from |= r.rev.del.as_ref().is_some_and(|m| m.kind == crate::docx::review::MarkKind::MoveFrom);
        }
    }
    (to, from)
}

fn centred(e: &Editor, text: &str) -> bool {
    e.document().sections[0].blocks.iter().any(|b| {
        matches!(b, crate::docx::Block::Para(p) if super::super::text::UnitText::of(p).text == text && p.style.jc == crate::docx::props::Jc::Center)
    })
}

#[test]
fn a_tracked_move_of_the_last_paragraph_up_keeps_the_story_s_mark() {
    for accept in [true, false] {
        let mut e = tracked_three();
        caret(&mut e, 2, 2);
        assert!(matches!(e.move_paragraphs(true, fonts()), crate::editing::commands::Outcome::Changed(_)));
        assert_eq!(has_moves(&e), (true, true), "moved to and from");
        let ids = texts_ids(&e);
        let at = e.selection.focus().and_then(|p| p.para_offset()).unwrap();
        assert_eq!((texts(&e)[ids.iter().position(|&p| p == at.0).unwrap()].as_str(), at.1), ("three", Unit(2)), "the caret goes with it");
        if accept {
            e.execute(C::AcceptAllRevisions(Default::default()), fonts());
            assert_eq!(texts(&e), ["one", "three", "two"]);
            assert!(centred(&e, "two"), "the paragraph passed keeps its properties");
        } else {
            e.execute(C::RejectAllRevisions(Default::default()), fonts());
            assert_eq!(texts(&e), ["one", "two", "three"]);
            assert!(centred(&e, "two") && !centred(&e, "three"));
        }
    }
    let mut e = tracked_three();
    caret(&mut e, 2, 0);
    e.move_paragraphs(true, fonts());
    e.execute(C::Undo(Default::default()), fonts());
    assert_eq!(texts(&e), ["one", "two", "three"], "one undo step");
    assert_eq!(has_moves(&e), (false, false));
}

#[test]
fn a_tracked_move_down_past_the_last_paragraph() {
    for accept in [true, false] {
        let mut e = tracked_three();
        caret(&mut e, 1, 1);
        assert!(matches!(e.move_paragraphs(false, fonts()), crate::editing::commands::Outcome::Changed(_)));
        assert_eq!(has_moves(&e), (true, true));
        if accept {
            e.execute(C::AcceptAllRevisions(Default::default()), fonts());
            assert_eq!(texts(&e), ["one", "three", "two"]);
            assert!(centred(&e, "two") && !centred(&e, "three"), "each keeps its properties");
        } else {
            e.execute(C::RejectAllRevisions(Default::default()), fonts());
            assert_eq!(texts(&e), ["one", "two", "three"]);
            assert!(centred(&e, "two") && !centred(&e, "three"));
        }
    }
}

#[test]
fn a_tracked_move_past_a_table() {
    let table = r#"<w:tbl><w:tblGrid><w:gridCol w:w="2000"/></w:tblGrid><w:tr><w:tc><w:p><w:r><w:t>cell</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#;
    let body = format!("<w:p><w:r><w:t>above</w:t></w:r></w:p>{table}<w:p><w:r><w:t>below</w:t></w:r></w:p><w:p><w:r><w:t>end</w:t></w:r></w:p>");
    for (para, up, accepted) in [
        (2, true, vec!["above", "below", "cell", "end"]),
        (0, false, vec!["cell", "above", "below", "end"]),
        (3, true, vec!["above", "cell", "end", "below"]),
    ] {
        for accept in [true, false] {
            let mut e = edit(&body);
            e.execute(C::SetTrackChanges(true), fonts());
            caret(&mut e, para, 0);
            assert!(matches!(e.move_paragraphs(up, fonts()), crate::editing::commands::Outcome::Changed(_)), "{para} {up}");
            assert_eq!(has_moves(&e), (true, true), "{para} {up}");
            e.execute(if accept { C::AcceptAllRevisions(Default::default()) } else { C::RejectAllRevisions(Default::default()) }, fonts());
            let want: Vec<&str> = if accept { accepted.clone() } else { vec!["above", "cell", "below", "end"] };
            assert_eq!(texts(&e), want, "{para} {up} accepted {accept}");
        }
    }
    // The story's last paragraph above a table leaves the story's mark after it.
    let mut e = edit(&format!("<w:p><w:r><w:t>above</w:t></w:r></w:p>{table}<w:p><w:r><w:t>below</w:t></w:r></w:p>"));
    e.execute(C::SetTrackChanges(true), fonts());
    caret(&mut e, 2, 0);
    e.move_paragraphs(true, fonts());
    e.execute(C::AcceptAllRevisions(Default::default()), fonts());
    assert_eq!(texts(&e), ["above", "below", "cell", ""], "as untracked");
}

#[test]
fn bookmarks_and_comments_into_the_paragraph_passed_keep_their_text_there() {
    let body = concat!(
        r#"<w:p><w:r><w:t>one</w:t></w:r><w:bookmarkStart w:id="1" w:name="mark"/><w:r><w:t>two</w:t></w:r></w:p>"#,
        r#"<w:p><w:r><w:t>three</w:t></w:r><w:bookmarkEnd w:id="1"/></w:p>"#,
    );
    let open = |tracked: bool| {
        let mut e = edit(body);
        e.selection = Selection::range(
            Position::text(StoryId::Body, texts_ids(&e)[0], Unit(3)),
            Position::text(StoryId::Body, texts_ids(&e)[1], Unit(5)),
            Default::default(),
        );
        e.execute(C::AddComment(crate::api::review::Body::Text("c".into())), fonts());
        if tracked {
            e.execute(C::SetTrackChanges(true), fonts());
        }
        e
    };
    let mut e = open(false);
    caret(&mut e, 1, 0);
    assert!(matches!(e.move_paragraphs(true, fonts()), crate::editing::commands::Outcome::Changed(_)), "the move isn't refused");
    assert_eq!(texts(&e), ["three", "onetwo"]);
    let kinds = |e: &Editor, n: usize| -> Vec<String> {
        para(e, n)
            .inlines
            .iter()
            .filter_map(|i| match i {
                Inline::Range(r) if let Some((b, start)) = r.bookmark() => Some(if start { b.to_string() } else { format!("/{b}") }),
                Inline::Range(r) => Some(format!("{:?}", r.kind)),
                _ => None,
            })
            .collect()
    };
    assert!(kinds(&e, 0).is_empty(), "nothing stays in the moved paragraph: {:?}", kinds(&e, 0));
    assert_eq!(kinds(&e, 1), ["mark", "CommentStart", "CommentEnd", "/mark"], "the ends close where the paragraph was");
    e.execute(C::Undo(Default::default()), fonts());
    assert_eq!(texts(&e), ["onetwo", "three"]);
    // Down: the starts stay where the paragraph was, before the one passed.
    caret(&mut e, 0, 0);
    e.move_paragraphs(false, fonts());
    assert_eq!(texts(&e), ["three", "onetwo"]);
    assert_eq!(kinds(&e, 0), ["mark", "CommentStart", "CommentEnd", "/mark"]);
    // Tracked, the moved-from text keeps them.
    let mut e = open(true);
    caret(&mut e, 1, 0);
    assert!(matches!(e.move_paragraphs(true, fonts()), crate::editing::commands::Outcome::Changed(_)));
    e.execute(C::AcceptAllRevisions(Default::default()), fonts());
    assert_eq!(texts(&e), ["three", "onetwo"]);
    let all: Vec<String> = (0..2).flat_map(|n| kinds(&e, n)).collect();
    assert_eq!(all.iter().filter(|k| k.as_str() == "mark").count(), 1, "one bookmark: {all:?}");
    assert_eq!(all.iter().filter(|k| k.as_str() == "CommentEnd").count(), 1, "one comment end: {all:?}");
}

/// Types `text` as the session does: Enter and Tab as their commands, each followed by AutoCorrect.
fn type_in(e: &mut Editor, text: &str) {
    use crate::editing::autocorrect::{AutoCorrect, Trigger, may_trigger};
    let ac = AutoCorrect::default();
    for c in text.chars() {
        let (cmd, trigger) = match c {
            '\n' => (C::InsertParagraph, Some(Trigger::ParagraphEnd)),
            '\t' => (C::InsertTab, Some(Trigger::Typed)),
            c => (C::InsertText(c.to_string()), may_trigger(c).then_some(Trigger::Typed)),
        };
        e.execute(cmd, fonts());
        if let Some(t) = trigger {
            e.auto_correct(&ac, t, fonts());
        }
    }
}

fn link_codes(e: &Editor, n: usize) -> Vec<String> {
    let p = para(e, n);
    crate::docx::fields::spans(&p.inlines).into_iter().map(|s| crate::docx::fields::code_text(&p.inlines, s).trim().to_string()).collect()
}

#[test]
fn typed_addresses_become_links() {
    for (typed, code, shown) in [
        ("See www.example.com. ", r#"HYPERLINK "http://www.example.com""#, "See www.example.com. "),
        ("Go https://example.com/a?b=1 now", r#"HYPERLINK "https://example.com/a?b=1""#, "Go https://example.com/a?b=1 now"),
        ("Mail jo@example.org\n", r#"HYPERLINK "mailto:jo@example.org""#, "Mail jo@example.org"),
        (r"At \\server\share ", r#"HYPERLINK "file:///\\\\server\\share""#, r"At \\server\share "),
    ] {
        let mut e = edit("<w:p/>");
        caret(&mut e, 0, 0);
        type_in(&mut e, typed);
        assert_eq!(link_codes(&e, 0), [code], "typing {typed:?}");
        assert_eq!(
            texts(&e)[0].replacen(&format!(" {code} "), "", 1).replace('\u{FFFC}', ""),
            shown,
            "typing {typed:?}: the code, then the text as typed"
        );
    }
    let mut e = edit("<w:p/>");
    caret(&mut e, 0, 0);
    type_in(&mut e, "Www.example.com ");
    e.execute(C::Undo(Default::default()), fonts());
    assert!(link_codes(&e, 0).is_empty(), "Ctrl+Z right after takes the link out");
    assert_eq!(texts(&e)[0], "Www.example.com ");
    let mut e = edit("<w:p/>");
    caret(&mut e, 0, 0);
    type_in(&mut e, "www. example.com x.y ");
    assert!(link_codes(&e, 0).is_empty(), "not an address");
}

#[test]
fn border_lines_typed_then_enter_become_a_bottom_border() {
    for (typed, line, size) in [
        ("---", "single", 6),
        ("___", "single", 12),
        ("===", "double", 6),
        ("***", "dotted", 24),
        ("~~~~", "wave", 6),
        ("###", "thinThickThinSmallGap", 24),
    ] {
        let mut e = edit("<w:p><w:r><w:t>above</w:t></w:r></w:p><w:p/>");
        caret(&mut e, 1, 0);
        type_in(&mut e, &format!("{typed}\n"));
        let p = para(&e, 1);
        assert_eq!(texts(&e), ["above", "", ""], "{typed}");
        let b = p.style.border_bottom.as_ref().unwrap_or_else(|| panic!("{typed}: a border"));
        let xml = p.style.direct.xml(&Default::default(), &e.cx()).map(std::borrow::Cow::into_owned).unwrap_or_default();
        assert!(xml.contains(&format!(r#"w:val="{line}" w:sz="{size}""#)), "{typed}: {xml}");
        let _ = b;
        assert!(para(&e, 2).style.border_bottom.is_none(), "the new paragraph has none");
        e.execute(C::Undo(Default::default()), fonts());
        assert_eq!(texts(&e), ["above", typed, ""], "{typed}: Ctrl+Z gives the characters back");
    }
    let mut e = edit("<w:p/>");
    caret(&mut e, 0, 0);
    type_in(&mut e, "--=\n");
    assert!(para(&e, 0).style.border_bottom.is_none());
}

#[test]
fn tab_at_a_paragraphs_start_indents_it() {
    let mut e = edit("<w:p><w:r><w:t>text</w:t></w:r></w:p>");
    caret(&mut e, 0, 0);
    let ind = |e: &Editor| (para(e, 0).style.ind_left, para(e, 0).style.ind_first);
    type_in(&mut e, "\t");
    assert_eq!((texts(&e)[0].as_str(), ind(&e)), ("text", (0.0, 36.0)), "a first-line indent in place of the tab");
    type_in(&mut e, "\t");
    assert_eq!(ind(&e), (36.0, 36.0), "then the left indent");
    e.execute(C::Delete(crate::api::command::DeleteOptions { direction: crate::api::value::Direction::Backward, unit: None }), fonts());
    assert_eq!(ind(&e), (0.0, 36.0), "Backspace goes back");
    e.execute(C::Delete(crate::api::command::DeleteOptions { direction: crate::api::value::Direction::Backward, unit: None }), fonts());
    assert_eq!(ind(&e), (0.0, 0.0));
    type_in(&mut e, "\t");
    e.execute(C::Undo(Default::default()), fonts());
    assert_eq!((texts(&e)[0].as_str(), ind(&e)), ("\ttext", (0.0, 0.0)), "Ctrl+Z right after gives the tab back");
    let mut e = edit("<w:p/>");
    caret(&mut e, 0, 0);
    type_in(&mut e, "\t");
    assert_eq!(texts(&e)[0], "\t", "an empty paragraph takes the tab");
    e.indent_keys = false;
    let mut e2 = edit("<w:p><w:r><w:t>text</w:t></w:r></w:p>");
    e2.indent_keys = false;
    caret(&mut e2, 0, 0);
    type_in(&mut e2, "\t");
    assert_eq!(texts(&e2)[0], "\ttext", "the option off");
}

#[test]
fn internet_paths_keep_balanced_brackets_and_encode_mailboxes() {
    use crate::editing::autocorrect::internet_path;
    let path = |s: &str| internet_path(&s.chars().collect::<Vec<_>>());
    assert_eq!(path("https://example.com/Topic_(detail)"), Some((0, 34, "https://example.com/Topic_(detail)".into())));
    assert_eq!(path("https://example.com/Topic_(detail))."), Some((0, 34, "https://example.com/Topic_(detail)".into())));
    assert_eq!(path("(https://example.com)"), Some((1, 19, "https://example.com".into())));
    assert_eq!(path("a?b@example.com").map(|p| p.2), Some("mailto:a%3Fb@example.com".into()));
    assert_eq!(path("mailto:a?b@example.com").map(|p| p.2), Some("mailto:a?b@example.com".into()), "a typed mailto: stays as typed");
}

#[test]
fn hyperlink_autoformat_undoes_to_the_typing_caret() {
    let mut e = edit("<w:p/>");
    caret(&mut e, 0, 0);
    type_in(&mut e, "See www.example.com ");
    let typed = e.selection;
    e.execute(C::Undo(Default::default()), fonts());
    assert!(link_codes(&e, 0).is_empty());
    assert_eq!(e.selection, Selection::Caret(Position::text(StoryId::Body, texts_ids(&e)[0], Unit(20))), "after the space, not the address selected");
    e.execute(C::Redo(Default::default()), fonts());
    assert_eq!(e.selection, typed, "redo returns to the caret after the link and space");
}

#[test]
fn a_revert_that_cannot_run_keeps_the_record_and_the_option() {
    use crate::editing::autocorrect::AutoCorrect;
    let mut e = edit("<w:p/>");
    caret(&mut e, 0, 0);
    type_in(&mut e, "See www.example.com ");
    let mut ac = AutoCorrect::default();
    e.begin_batch(None);
    let out = e.stop_auto_format(&mut ac, fonts());
    assert!(!matches!(out, Outcome::Changed(_)));
    assert!(ac.internet_paths, "the option stays on");
    assert!(e.auto_format_change().is_some(), "the record stays");
    e.end_batch(true, fonts());
    e.stop_auto_format(&mut ac, fonts());
    assert!(!ac.internet_paths && link_codes(&e, 0).is_empty());
}

#[test]
fn tab_and_backspace_step_off_grid_indents_along_the_same_grid() {
    let mut e = edit(r#"<w:p><w:pPr><w:ind w:left="400" w:firstLine="720"/></w:pPr><w:r><w:t>text</w:t></w:r></w:p>"#);
    caret(&mut e, 0, 0);
    type_in(&mut e, "\t");
    assert_eq!(para(&e, 0).style.ind_left, 36.0, "the next tab stop, as Increase Indent goes");
    e.execute(C::Delete(crate::api::command::DeleteOptions { direction: crate::api::value::Direction::Backward, unit: None }), fonts());
    assert_eq!(para(&e, 0).style.ind_left, 0.0, "Backspace back along the same grid");
}

#[test]
fn autoformat_changes_offer_their_undo_and_stop() {
    use crate::editing::CorrectionKind as K;
    use crate::editing::autocorrect::AutoCorrect;
    // Undo Hyperlink, right after it and once more is typed
    for more in ["", "x"] {
        let mut e = edit("<w:p/>");
        caret(&mut e, 0, 0);
        type_in(&mut e, "See www.example.com ");
        assert_eq!(e.auto_format_change().map(|c| (c.kind.clone(), c.from.as_str())), Some((K::Hyperlink, "www.example.com")));
        assert!(e.correction().is_none(), "not a text correction");
        assert!(e.correction_view(0..1).is_some_and(|v| !v.rects.is_empty() && v.button.is_some()), "its mark and options button");
        type_in(&mut e, more);
        e.revert_correction(fonts());
        assert!(link_codes(&e, 0).is_empty(), "{more:?}: the link goes");
        assert_eq!(texts(&e)[0], format!("See www.example.com {more}"));
        assert!(e.auto_format_change().is_none());
    }
    let mut ac = AutoCorrect::default();
    let mut e = edit("<w:p/>");
    caret(&mut e, 0, 0);
    type_in(&mut e, "See www.example.com ");
    e.stop_auto_format(&mut ac, fonts());
    assert!(link_codes(&e, 0).is_empty() && !ac.internet_paths, "Stop Automatically Creating Hyperlinks");
    // Undo Border Line, Stop Automatically Creating Border Lines
    let mut e = edit("<w:p><w:r><w:t>above</w:t></w:r></w:p><w:p/>");
    caret(&mut e, 1, 0);
    type_in(&mut e, "---\n");
    assert!(matches!(e.auto_format_change().map(|c| &c.kind), Some(K::BorderLine { borders }) if borders.borders.is_empty()));
    assert!(e.correction_view(0..1).is_some_and(|v| v.button.is_some()), "its button, the caret in the next paragraph");
    type_in(&mut e, "y");
    e.revert_correction(fonts());
    assert_eq!(texts(&e), ["above", "---", "y"]);
    assert!(para(&e, 1).style.border_bottom.is_none(), "the border goes");
    let mut e = edit("<w:p/>");
    caret(&mut e, 0, 0);
    type_in(&mut e, "***\n");
    e.stop_auto_format(&mut ac, fonts());
    assert!(!ac.border_lines && para(&e, 0).style.border_bottom.is_none());
    // Undo Automatic Indent, Stop
    let mut e = edit("<w:p><w:r><w:t>text</w:t></w:r></w:p>");
    caret(&mut e, 0, 0);
    type_in(&mut e, "\t");
    assert!(matches!(e.auto_format_change().map(|c| &c.kind), Some(K::Indent { ind }) if ind.indent_left.is_none() && ind.indent_first.is_none()));
    type_in(&mut e, "z");
    e.revert_correction(fonts());
    assert_eq!((texts(&e)[0].as_str(), para(&e, 0).style.ind_first), ("\tztext", 0.0));
    let mut e = edit("<w:p><w:r><w:t>text</w:t></w:r></w:p>");
    caret(&mut e, 0, 0);
    type_in(&mut e, "\t");
    e.stop_auto_format(&mut ac, fonts());
    assert!(!e.indent_keys && texts(&e)[0] == "\ttext");
}

/// Document coordinates of unit `u` of paragraph `para` (its line's middle), `dx` to its right.
fn at(e: &mut Editor, para: usize, u: u32, dx: f64) -> (f64, f64) {
    let id = texts_ids(e)[para];
    let g = e.page_geometries().into_iter().find(|(p, _)| *p == 0).map(|(_, g)| g).expect("page 0");
    let l = g.lines.iter().find(|l| l.shows(id)).expect("a line");
    let x = l.stops.iter().find(|s| s.para == id && s.unit == Unit(u)).expect("a stop").x;
    e.stack().to_doc(0, x + dx, (l.top + l.bottom) / 2.0).expect("on the stack")
}

#[test]
fn f2_moves_the_selection_to_the_place_chosen() {
    use super::super::input::PointerKind;
    let mut e = edit("<w:p><w:r><w:t>one two</w:t></w:r></w:p><w:p><w:r><w:t>three</w:t></w:r></w:p>");
    select(&mut e, 0, 0, 4);
    let sel = e.selection;
    e.begin_move_to(false);
    let to = at(&mut e, 1, 5, 0.0);
    e.pointer_input(PointerKind::Down, to.0, to.1, 1, Mods::default(), fonts());
    e.pointer_input(PointerKind::Up, to.0, to.1, 1, Mods::default(), fonts());
    assert_eq!(e.selection, sel, "the click chooses the place, the text stays selected");
    assert!(e.move_to_pending().and_then(|m| m.target).is_some());
    e.execute(
        C::MoveSelection(crate::api::command::MoveSelection {
            by: crate::api::command::MoveUnit::Character,
            direction: crate::api::value::Direction::Backward,
            extend: None,
        }),
        fonts(),
    );
    assert_eq!(e.selection, sel, "an arrow moves the place");
    e.execute(C::InsertParagraph, fonts());
    assert_eq!(texts(&e), ["two", "thre one e"], "Enter moves the text, smart spaces and all");
    assert!(e.move_to_pending().is_none());
    select(&mut e, 0, 0, 3);
    e.begin_move_to(true);
    e.execute(C::Escape, fonts());
    e.execute(C::InsertParagraph, fonts());
    assert_eq!(texts(&e).len(), 3, "Esc gave up: Enter is Enter again");
}

#[test]
fn a_right_drag_opens_the_drop_menu() {
    use super::super::input::PointerKind;
    use super::super::move_to::DropChoice;
    for (choice, want) in
        [(DropChoice::Move, ["two", "three one "]), (DropChoice::Copy, ["one two", "three one "]), (DropChoice::Cancel, ["one two", "three"])]
    {
        let mut e = edit("<w:p><w:r><w:t>one two</w:t></w:r></w:p><w:p><w:r><w:t>three</w:t></w:r></w:p>");
        select(&mut e, 0, 0, 4);
        let (from, to) = (at(&mut e, 0, 1, 0.0), at(&mut e, 1, 5, 0.0));
        assert!(e.right_press(from.0, from.1, fonts()));
        e.pointer_input(PointerKind::Move, to.0, to.1, 0, Mods::default(), fonts());
        let drop = e.right_release().expect("the menu opens where it dropped");
        assert_eq!(e.drop_menu(), Some(drop));
        e.choose_drop(choice, fonts());
        assert_eq!(texts(&e), want, "{choice:?}");
        assert!(e.drop_menu().is_none());
    }
    let mut e = edit("<w:p><w:r><w:t>one two</w:t></w:r></w:p>");
    select(&mut e, 0, 0, 4);
    let from = at(&mut e, 0, 1, 0.0);
    assert!(e.right_press(from.0, from.1, fonts()));
    assert_eq!(e.right_release(), None, "a click, not a drag: the context menu's");
}

#[test]
fn create_hyperlink_here_links_to_a_hidden_bookmark_on_the_text() {
    use super::super::input::PointerKind;
    use super::super::move_to::DropChoice;
    let mut e = edit("<w:p><w:r><w:t>one two</w:t></w:r></w:p><w:p><w:r><w:t>three</w:t></w:r></w:p>");
    select(&mut e, 0, 0, 3);
    let (from, to) = (at(&mut e, 0, 1, 0.0), at(&mut e, 1, 5, 0.0));
    assert!(e.right_press(from.0, from.1, fonts()));
    e.pointer_input(PointerKind::Move, to.0, to.1, 0, Mods::default(), fonts());
    e.right_release().expect("dropped");
    e.choose_drop(DropChoice::Hyperlink, fonts());
    assert_eq!(texts(&e)[0], "one two", "the text stays");
    let name = e.bookmarks().into_iter().map(|b| b.name).find(|n| n.starts_with("_Hlk")).expect("a hidden bookmark on the source");
    let codes = link_codes(&e, 1);
    assert!(codes.len() == 1 && codes[0].starts_with("HYPERLINK") && codes[0].contains(&format!(r#"\l "{name}""#)), "{codes:?}");
    e.execute(C::Undo(Default::default()), fonts());
    assert_eq!(texts(&e), ["one two", "three"], "one undo step");
    assert!(e.bookmarks().is_empty());
}

#[test]
fn the_drop_menu_closes_when_the_document_changes_or_drags_are_off() {
    use super::super::input::PointerKind;
    use super::super::move_to::DropChoice;
    let mut e = edit("<w:p><w:r><w:t>one two</w:t></w:r></w:p><w:p><w:r><w:t>three</w:t></w:r></w:p>");
    select(&mut e, 0, 0, 3);
    let sel = e.selection;
    let (from, to) = (at(&mut e, 0, 1, 0.0), at(&mut e, 1, 5, 0.0));
    let drag = |e: &mut Editor| {
        assert!(e.right_press(from.0, from.1, fonts()));
        e.pointer_input(PointerKind::Move, to.0, to.1, 0, Mods::default(), fonts());
        e.right_release().expect("dropped");
    };
    drag(&mut e);
    e.execute(C::InsertText("1".into()), fonts());
    assert!(e.drop_menu().is_none(), "the document changed");
    assert!(matches!(e.choose_drop(DropChoice::Move, fonts()), super::super::commands::Outcome::Unchanged(_)));
    assert_eq!(texts(&e), ["1 two", "three"]);
    e.execute(C::Undo(Default::default()), fonts());
    e.selection = sel;
    drag(&mut e);
    e.set_text_drag_and_drop(false);
    assert!(e.drop_menu().is_none(), "drags turned off");
    assert!(!e.right_press(from.0, from.1, fonts()), "no right drag while off");
    e.set_text_drag_and_drop(true);
    e.execute(C::SetSelectionMode(crate::api::command::SelectionMode::Extend), fonts());
    assert!(!e.right_press(from.0, from.1, fonts()), "no right drag in extend mode");
}

#[test]
fn link_here_bookmarks_the_text_and_links_to_it() {
    use super::super::input::PointerKind;
    let mut e = edit("<w:p><w:r><w:t>one two</w:t></w:r></w:p><w:p><w:r><w:t>three</w:t></w:r></w:p>");
    select(&mut e, 0, 0, 3);
    let (from, to) = (at(&mut e, 0, 1, 0.0), at(&mut e, 1, 5, 0.0));
    e.right_press(from.0, from.1, fonts());
    e.pointer_input(PointerKind::Move, to.0, to.1, 0, Mods::default(), fonts());
    e.right_release();
    let out = e.choose_drop(super::super::move_to::DropChoice::Link, fonts());
    assert!(matches!(out, super::super::commands::Outcome::Refused(_)), "no file name: {out:?}");
    e.execute(C::SetFileName("Report.docx".into()), fonts());
    e.right_press(from.0, from.1, fonts());
    e.pointer_input(PointerKind::Move, to.0, to.1, 0, Mods::default(), fonts());
    e.right_release();
    e.choose_drop(super::super::move_to::DropChoice::Link, fonts());
    assert_eq!(link_codes(&e, 1), [r#"LINK Word.Document.12 "Report.docx" "OLE_LINK1" \a \r"#]);
    assert!(para(&e, 0).inlines.iter().any(|i| i.bookmark() == Some(("OLE_LINK1", true))));
    e.execute(C::Undo(Default::default()), fonts());
    assert!(link_codes(&e, 1).is_empty() && !para(&e, 0).inlines.iter().any(|i| i.bookmark().is_some()), "one undo step");
}

#[test]
fn change_back_leaves_an_indent_set_after_the_automatic_one() {
    let mut e = edit("<w:p><w:r><w:t>text</w:t></w:r></w:p>");
    caret(&mut e, 0, 0);
    type_in(&mut e, "\t");
    assert!(e.auto_format_change().is_some());
    e.execute(C::IncreaseIndent, fonts());
    let (left, first) = (para(&e, 0).style.ind_left, para(&e, 0).style.ind_first);
    assert!(e.auto_format_change().is_none(), "the indent it made is no longer there");
    assert!(matches!(e.revert_correction(fonts()), Outcome::Unchanged(_) | Outcome::Refused(_)));
    assert_eq!((texts(&e)[0].as_str(), para(&e, 0).style.ind_left, para(&e, 0).style.ind_first), ("text", left, first));
}

#[test]
fn the_highlighter_highlights_what_it_drags_over() {
    use super::super::input::PointerKind;
    let mut e = edit("<w:p><w:r><w:t>one two</w:t></w:r></w:p>");
    caret(&mut e, 0, 0);
    e.toggle_highlighter(Some(crate::api::value::HighlightColor::Yellow));
    assert_eq!(e.highlighter(), Some("yellow"));
    let (from, to) = (at(&mut e, 0, 0, 0.0), at(&mut e, 0, 3, 0.0));
    let m = Mods::default();
    e.pointer_input(PointerKind::Down, from.0, from.1, 1, m, fonts());
    e.pointer_input(PointerKind::Move, to.0, to.1, 0, m, fonts());
    e.pointer_input(PointerKind::Up, to.0, to.1, 0, m, fonts());
    let lit: Vec<(String, bool)> =
        para(&e, 0).inlines.iter().filter_map(|i| if let Inline::Text(t, r) = i { Some((t.clone(), r.highlight.is_some())) } else { None }).collect();
    assert_eq!(lit, [("one".to_string(), true), (" two".to_string(), false)]);
    assert!(e.selection.is_collapsed(), "the pen stays a pen");
    e.execute(C::Escape, fonts());
    assert_eq!(e.highlighter(), None, "Esc puts it down");
}

fn protect(e: &mut Editor) {
    let p = crate::api::command::DocumentProtection {
        kind: Some(crate::api::command::ProtectionKind::ReadOnly),
        enforce: Some(true),
        ..Default::default()
    };
    assert!(matches!(e.execute(C::SetDocumentProtection(p), fonts()), super::super::commands::Outcome::Changed(_)));
}

#[test]
fn shift_f2_copies_kept_text_into_an_editable_range() {
    let range = r#"<w:p><w:permStart w:id="1" w:edGrp="everyone"/><w:r><w:t>three</w:t></w:r><w:permEnd w:id="1"/></w:p>"#;
    let mut e = edit(&format!("<w:p><w:r><w:t>one two</w:t></w:r></w:p>{range}"));
    protect(&mut e);
    select(&mut e, 0, 0, 3);
    e.begin_move_to(true);
    let to = at(&mut e, 1, 5, 0.0);
    e.pointer_input(super::super::input::PointerKind::Down, to.0, to.1, 1, Mods::default(), fonts());
    e.pointer_input(super::super::input::PointerKind::Up, to.0, to.1, 1, Mods::default(), fonts());
    let out = e.execute(C::InsertParagraph, fonts());
    assert!(matches!(out, super::super::commands::Outcome::Changed(_)), "{out:?}");
    assert_eq!(texts(&e), ["one two", "three one"], "copied at the place in the range");
    select(&mut e, 0, 0, 3);
    e.begin_move_to(false);
    e.pointer_input(super::super::input::PointerKind::Down, to.0, to.1, 1, Mods::default(), fonts());
    e.pointer_input(super::super::input::PointerKind::Up, to.0, to.1, 1, Mods::default(), fonts());
    e.execute(C::InsertParagraph, fonts());
    assert_eq!(texts(&e), ["one two", "three one"], "F2 can't take the kept text");
    caret(&mut e, 0, 1);
    assert!(!matches!(e.execute(C::InsertParagraph, fonts()), super::super::commands::Outcome::Changed(_)), "Enter outside the range");
}

#[test]
fn f2_gives_up_when_the_selection_changes_before_enter() {
    let mut e = edit("<w:p><w:r><w:t>one two</w:t></w:r></w:p><w:p><w:r><w:t>three</w:t></w:r></w:p>");
    select(&mut e, 0, 0, 3);
    e.begin_move_to(false);
    let to = at(&mut e, 1, 5, 0.0);
    e.pointer_input(super::super::input::PointerKind::Down, to.0, to.1, 1, Mods::default(), fonts());
    e.pointer_input(super::super::input::PointerKind::Up, to.0, to.1, 1, Mods::default(), fonts());
    let para = texts_ids(&e)[0];
    e.select_range(para, Unit(4), Unit(7), fonts());
    e.execute(C::InsertParagraph, fonts());
    assert_eq!(texts(&e), ["one two", "three"], "two isn't moved to one's place");
}

#[test]
fn the_drop_menu_closes_when_the_selection_changes() {
    use super::super::input::PointerKind;
    use super::super::move_to::DropChoice;
    let mut e = edit("<w:p><w:r><w:t>one two</w:t></w:r></w:p><w:p><w:r><w:t>three</w:t></w:r></w:p>");
    select(&mut e, 0, 0, 3);
    let (from, to) = (at(&mut e, 0, 1, 0.0), at(&mut e, 1, 5, 0.0));
    assert!(e.right_press(from.0, from.1, fonts()));
    e.pointer_input(PointerKind::Move, to.0, to.1, 0, Mods::default(), fonts());
    e.right_release().expect("dropped");
    let para = texts_ids(&e)[0];
    e.select_range(para, Unit(4), Unit(7), fonts());
    assert!(e.drop_menu().is_none(), "another selection");
    assert!(matches!(e.choose_drop(DropChoice::Move, fonts()), super::super::commands::Outcome::Unchanged(_)));
    assert_eq!(texts(&e), ["one two", "three"]);
}

#[test]
fn link_here_takes_a_bookmark_name_unused_in_any_case() {
    use super::super::input::PointerKind;
    let mut e = edit(
        r#"<w:p><w:bookmarkStart w:id="0" w:name="ole_link1"/><w:r><w:t>one</w:t></w:r><w:bookmarkEnd w:id="0"/><w:r><w:t xml:space="preserve"> two</w:t></w:r></w:p><w:p><w:r><w:t>three</w:t></w:r></w:p>"#,
    );
    e.execute(C::SetFileName("Report.docx".into()), fonts());
    select(&mut e, 0, 4, 7);
    let (from, to) = (at(&mut e, 0, 5, 0.0), at(&mut e, 1, 5, 0.0));
    e.right_press(from.0, from.1, fonts());
    e.pointer_input(PointerKind::Move, to.0, to.1, 0, Mods::default(), fonts());
    e.right_release().expect("dropped");
    e.choose_drop(super::super::move_to::DropChoice::Link, fonts());
    assert_eq!(link_codes(&e, 1), [r#"LINK Word.Document.12 "Report.docx" "OLE_LINK2" \a \r"#]);
    let names: Vec<String> = e.bookmarks().into_iter().map(|b| b.name).collect();
    assert_eq!(names, ["ole_link1", "OLE_LINK2"], "the old link's bookmark stays");
}

#[test]
fn link_here_past_the_last_bookmark_number_is_refused() {
    use super::super::input::PointerKind;
    let mut e = edit(
        r#"<w:p><w:bookmarkStart w:id="0" w:name="OLE_LINK4294967295"/><w:r><w:t>one</w:t></w:r><w:bookmarkEnd w:id="0"/><w:r><w:t xml:space="preserve"> two</w:t></w:r></w:p><w:p><w:r><w:t>three</w:t></w:r></w:p>"#,
    );
    e.execute(C::SetFileName("Report.docx".into()), fonts());
    select(&mut e, 0, 4, 7);
    let (from, to) = (at(&mut e, 0, 5, 0.0), at(&mut e, 1, 5, 0.0));
    e.right_press(from.0, from.1, fonts());
    e.pointer_input(PointerKind::Move, to.0, to.1, 0, Mods::default(), fonts());
    e.right_release().expect("dropped");
    let out = e.choose_drop(super::super::move_to::DropChoice::Link, fonts());
    assert!(matches!(out, super::super::commands::Outcome::Unchanged(_)), "{out:?}");
    assert!(link_codes(&e, 1).is_empty());
}

#[test]
fn the_highlighter_keeps_to_document_protection() {
    use super::super::input::PointerKind;
    let mut e = edit("<w:p><w:r><w:t>one two</w:t></w:r></w:p>");
    protect(&mut e);
    caret(&mut e, 0, 0);
    e.toggle_highlighter(Some(crate::api::value::HighlightColor::Yellow));
    let (from, to) = (at(&mut e, 0, 0, 0.0), at(&mut e, 0, 3, 0.0));
    let m = Mods::default();
    e.pointer_input(PointerKind::Down, from.0, from.1, 1, m, fonts());
    e.pointer_input(PointerKind::Move, to.0, to.1, 0, m, fonts());
    e.pointer_input(PointerKind::Up, to.0, to.1, 0, m, fonts());
    assert!(para(&e, 0).inlines.iter().all(|i| !matches!(i, Inline::Text(_, r) if r.highlight.is_some())), "nothing highlighted");
}

#[test]
fn ctrl_shift_v_pastes_formatting_until_the_clipboard_changes() {
    let mut e = edit("<w:p><w:r><w:rPr><w:b/></w:rPr><w:t>bold</w:t></w:r><w:r><w:t xml:space=\"preserve\"> plain</w:t></w:r></w:p>");
    assert!(!e.paste_takes_formatting("x", None), "no formatting held");
    caret(&mut e, 0, 1);
    press(&mut e, "KeyC", CTRL_SHIFT);
    assert!(e.paste_takes_formatting("x", None), "the clipboard Copy Formatting saw");
    assert!(e.paste_takes_formatting("x", None));
    assert!(!e.paste_takes_formatting("y", None), "another copy since");
    assert!(!e.paste_takes_formatting("x", None), "for good");
    press(&mut e, "KeyC", CTRL_SHIFT);
    select(&mut e, 0, 5, 10);
    e.execute(C::Copy, fonts());
    assert!(!e.paste_takes_formatting("plain", None), "a copy here changes the clipboard");
    // The clipboard the host had at Copy Formatting: another one by the first Ctrl+Shift+V is a copy made elsewhere.
    e.execute(C::CopyFormatting, fonts());
    e.clipboard_at_copy_formatting("before", None);
    assert!(!e.paste_takes_formatting("copied elsewhere", None));
    e.execute(C::CopyFormatting, fonts());
    e.clipboard_at_copy_formatting("before", None);
    assert!(e.paste_takes_formatting("before", None));
}

#[test]
fn a_style_separator_reads_back() {
    let mut e = edit(concat!(
        r#"<w:p><w:pPr><w:rPr><w:vanish/><w:specVanish/></w:rPr></w:pPr><w:r><w:t>Head</w:t></w:r></w:p>"#,
        r#"<w:p><w:r><w:t xml:space="preserve"> body</w:t></w:r></w:p><w:p><w:r><w:t>next</w:t></w:r></w:p>"#,
    ));
    assert_eq!(texts(&e), ["Head body", "next"], "one paragraph as read");
    let ids = texts_ids(&e);
    assert_eq!(e.hidden_marks(ids[0]), [(Unit(4), true)]);
    assert!(e.hidden_marks(ids[1]).is_empty());
    caret(&mut e, 0, 0);
}

#[test]
fn a_hidden_mark_with_spec_vanish_off_is_no_style_separator() {
    for (spec, separator) in [(r#"<w:specVanish w:val="0"/>"#, false), ("<w:specVanish/>", true)] {
        let e = edit(&format!(
            r#"<w:p><w:r><w:t>one</w:t></w:r></w:p><w:p><w:pPr><w:rPr><w:vanish/>{spec}</w:rPr></w:pPr><w:r><w:t>end</w:t></w:r></w:p>"#
        ));
        assert_eq!(e.hidden_marks(texts_ids(&e)[1]), [(Unit(3), separator)], "{spec}");
    }
}

/// Whether one laid-out line on page 0 shows both paragraphs.
fn one_line(e: &mut Editor, a: crate::layout::ParaId, b: crate::layout::ParaId) -> bool {
    let g = e.page_geometries().into_iter().find(|(p, _)| *p == 0).map(|(_, g)| g).expect("page 0");
    g.lines.iter().any(|l| l.shows(a) && l.shows(b))
}

#[test]
fn ctrl_alt_enter_inserts_a_style_separator() {
    let mut e = edit(concat!(
        r#"<w:p><w:pPr><w:jc w:val="center"/></w:pPr><w:r><w:t xml:space="preserve">Head body</w:t></w:r></w:p>"#,
        "<w:p><w:r><w:t>next</w:t></w:r></w:p>",
    ));
    caret(&mut e, 0, 4);
    assert!(matches!(e.insert_style_separator(fonts()), crate::editing::commands::Outcome::Changed(_)));
    assert_eq!(texts(&e), ["Head", " body", "next"], "the paragraph splits at the caret");
    let ids = texts_ids(&e);
    assert_eq!(e.hidden_marks(ids[0]), [(Unit(4), true)], "its mark is a style separator");
    assert!(para(&e, 0).mark.hidden);
    assert_eq!(e.selection, Selection::Caret(Position::text(StoryId::Body, ids[1], Unit(0))), "the caret after it");
    e.complete(fonts());
    assert!(one_line(&mut e, ids[0], ids[1]), "the two lay out as one");
    assert!(!one_line(&mut e, ids[1], ids[2]));
    // Saved and opened again, it reads as one paragraph with a style separator.
    let mut again = Editor::open(&crate::docx::write::save(e.document()).expect("saves"), fonts()).expect("reopens");
    again.complete(fonts());
    assert_eq!(texts(&again), ["Head body", "next"]);
    assert_eq!(again.hidden_marks(texts_ids(&again)[0]), [(Unit(4), true)]);
    e.execute(C::Undo(Default::default()), fonts());
    assert_eq!(texts(&e), ["Head body", "next"], "one undo step");
    assert!(!para(&e, 0).mark.hidden);
    // At a paragraph's end the mark itself becomes one, joining the next paragraph.
    caret(&mut e, 0, 9);
    e.insert_style_separator(fonts());
    assert_eq!(texts(&e), ["Head body", "next"]);
    let ids = texts_ids(&e);
    assert_eq!(e.hidden_marks(ids[0]), [(Unit(9), true)]);
    let after = Selection::Caret(Position::text(StoryId::Body, ids[1], Unit(0)));
    assert_eq!(e.selection, after, "the caret after it, in the next paragraph");
    e.complete(fonts());
    assert!(one_line(&mut e, ids[0], ids[1]));
    e.execute(C::Undo(Default::default()), fonts());
    assert_eq!(e.selection, Selection::Caret(Position::text(StoryId::Body, ids[0], Unit(9))), "undo puts the caret back");
    e.execute(C::Redo(Default::default()), fonts());
    assert_eq!(e.selection, after, "redo puts it after the separator again");
}

#[test]
fn a_style_separator_in_a_lists_empty_last_item_splits_it() {
    let mut e = edit("<w:p><w:r><w:t>one</w:t></w:r></w:p>");
    caret(&mut e, 0, 3);
    e.execute(C::SetList(Some(crate::api::command::ListKind::Numbered)), fonts());
    e.execute(C::InsertParagraph, fonts());
    assert!(para(&e, 1).style.num.is_some_and(|ListRef { list: ListId(id), level: _ }| id != 0), "setup: an empty last item");
    assert!(matches!(e.insert_style_separator(fonts()), crate::editing::commands::Outcome::Changed(_)));
    assert_eq!(texts(&e), ["one", "", ""], "the empty item splits at the caret");
    let ids = texts_ids(&e);
    assert!(e.hidden_marks(ids[0]).is_empty(), "the item before keeps its mark");
    assert_eq!(e.hidden_marks(ids[1]), [(Unit(0), true)], "the first part's mark is the separator");
    assert!(para(&e, 1).style.num.is_some_and(|ListRef { list: ListId(id), level: _ }| id != 0), "the item stays in its list");
    e.execute(C::Undo(Default::default()), fonts());
    assert_eq!(texts(&e), ["one", ""], "one undo step");
    assert!(!para(&e, 1).mark.hidden && para(&e, 1).style.num.is_some_and(|ListRef { list: ListId(id), level: _ }| id != 0));
}

#[test]
fn clearing_formatting_keeps_a_style_separator() {
    let body = concat!(
        r#"<w:p><w:r><w:rPr><w:b/><w:vanish/></w:rPr><w:t xml:space="preserve">Head body</w:t></w:r></w:p>"#,
        "<w:p><w:r><w:t>next</w:t></w:r></w:p>",
    );
    let ctrl_space = |e: &mut Editor| {
        e.execute(C::ClearFormatting, fonts());
    };
    let clear_all = |e: &mut Editor| {
        e.clear_all_formatting(fonts());
    };
    for (clear, name) in [(&ctrl_space as &dyn Fn(&mut Editor), "Ctrl+Space"), (&clear_all, "Clear All Formatting")] {
        let mut e = edit(body);
        caret(&mut e, 0, 4);
        e.insert_style_separator(fonts());
        let ids = texts_ids(&e);
        e.selection =
            Selection::range(Position::text(StoryId::Body, ids[0], Unit(0)), Position::text(StoryId::Body, ids[1], Unit(5)), Default::default());
        clear(&mut e);
        assert_eq!(e.hidden_marks(ids[0]), [(Unit(4), true)], "{name}: the separator stays");
        let run = para(&e, 0).inlines.iter().find_map(|i| i.run()).expect("a run");
        assert!(!run.hidden && !run.bold, "{name}: hidden text is formatting: {run:?}");
    }
}

#[test]
fn tab_on_a_lists_first_item_moves_the_list() {
    let mut e = edit("<w:p><w:r><w:t>one</w:t></w:r></w:p><w:p><w:r><w:t>two</w:t></w:r></w:p>");
    e.selection = Selection::range(
        Position::text(StoryId::Body, texts_ids(&e)[0], Unit(0)),
        Position::text(StoryId::Body, texts_ids(&e)[1], Unit(3)),
        Default::default(),
    );
    e.execute(C::SetList(Some(crate::api::command::ListKind::Numbered)), fonts());
    let (left, level) = (
        |e: &Editor, n: usize| para(e, n).style.ind_left,
        |e: &Editor, n: usize| para(e, n).style.num.map(|ListRef { list: ListId(_), level: l }| l),
    );
    let a = left(&e, 1);
    caret(&mut e, 1, 0);
    press(&mut e, "Tab", Mods::default());
    assert_eq!(level(&e, 1), Some(1), "a later item goes a level down");
    press(&mut e, "Tab", SHIFT);
    assert_eq!((level(&e, 1), left(&e, 1)), (Some(0), a));
    caret(&mut e, 0, 0);
    press(&mut e, "Tab", Mods::default());
    assert_eq!((level(&e, 0), left(&e, 1)), (Some(0), a + 36.0), "the first item takes the list along");
    press(&mut e, "Tab", SHIFT);
    assert_eq!(left(&e, 1), a);
}

#[test]
fn turning_drags_off_cancels_a_right_drag_under_way() {
    use super::super::input::PointerKind;
    let mut e = edit("<w:p><w:r><w:t>one two</w:t></w:r></w:p><w:p><w:r><w:t>three</w:t></w:r></w:p>");
    select(&mut e, 0, 0, 3);
    let (from, to) = (at(&mut e, 0, 1, 0.0), at(&mut e, 1, 5, 0.0));
    assert!(e.right_press(from.0, from.1, fonts()));
    e.pointer_input(PointerKind::Move, to.0, to.1, 0, Mods::default(), fonts());
    e.set_text_drag_and_drop(false);
    assert_eq!(e.right_release(), None, "the drag was cancelled");
    assert!(e.drop_menu().is_none());
}

#[test]
fn f11_goes_on_from_a_row_end() {
    let body = format!(
        r#"<w:p><w:r><w:t>top</w:t></w:r>{}</w:p><w:tbl><w:tr><w:tc><w:p><w:r><w:t>cell</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p>{}<w:r><w:t>end</w:t></w:r></w:p>"#,
        field_xml("DATE", "d"),
        field_xml("PAGE", "1"),
    );
    let mut e = edit(&body);
    let ids = texts_ids(&e);
    let cell = ids.iter().copied().find(|&p| texts(&e)[ids.iter().position(|&q| q == p).unwrap()] == "cell").expect("the cell");
    let row_end = e.nav().right(Position::text(StoryId::Body, cell, Unit(4)), false);
    assert!(row_end.para_offset().is_none(), "{row_end:?}");
    let field = |e: &Editor| e.selection.ordered(e.index()).and_then(|(a, _)| a.para_offset()).map(|(p, u)| (p, u.0));
    e.selection = Selection::Caret(row_end);
    assert!(matches!(e.select_field(crate::api::value::Order::Next, fonts()), super::super::commands::Outcome::Moved));
    assert_eq!(field(&e), Some((*ids.last().unwrap(), 0)), "the field at the start of the next paragraph");
    e.selection = Selection::Caret(row_end);
    assert!(matches!(e.select_field(crate::api::value::Order::Previous, fonts()), super::super::commands::Outcome::Moved));
    assert_eq!(field(&e), Some((ids[0], 3)), "the field above the table");
}

#[test]
fn a_drag_out_move_does_not_widen_past_an_editable_range() {
    let mut e = edit(
        r#"<w:p><w:permStart w:id="1" w:edGrp="everyone"/><w:r><w:t>cat</w:t></w:r><w:permEnd w:id="1"/><w:r><w:t xml:space="preserve"> dog</w:t></w:r></w:p>"#,
    );
    let on = crate::api::command::DocumentProtection {
        kind: Some(crate::api::command::ProtectionKind::ReadOnly),
        enforce: Some(true),
        ..Default::default()
    };
    e.execute(C::SetDocumentProtection(on), fonts());
    select(&mut e, 0, 0, 3);
    assert!(e.drag_out().is_some_and(|o| o.can_move));
    let out = e.drag_out_end(true, fonts());
    assert!(matches!(out, super::super::commands::Outcome::Changed(_)), "{out:?}");
    assert_eq!(texts(&e), [" dog"], "cat is gone, the protected space stays");
}

#[test]
fn a_drag_out_move_leaves_text_that_replaced_the_dragged_content() {
    let mut e = edit("<w:p><w:r><w:t>one two three</w:t></w:r></w:p>");
    select(&mut e, 0, 4, 7);
    assert!(e.drag_out().is_some_and(|o| o.text == "two"));
    // An edit that doesn't advance the revision, then the same offsets selected again.
    e.begin_batch(None);
    e.execute(C::InsertText("six".into()), fonts());
    select(&mut e, 0, 4, 7);
    assert_eq!(texts(&e), ["one six three"]);
    let out = e.drag_out_end(true, fonts());
    assert!(matches!(out, super::super::commands::Outcome::Unchanged(_)), "{out:?}");
    assert_eq!(texts(&e), ["one six three"]);
}

#[test]
fn a_drag_out_dropped_back_leaves_text_that_replaced_the_dragged_content() {
    let mut e = edit("<w:p><w:r><w:t>one two three</w:t></w:r></w:p>");
    select(&mut e, 0, 4, 7);
    assert!(e.drag_out().is_some_and(|o| o.text == "two"));
    // An edit that doesn't advance the revision, then the same offsets selected again.
    e.begin_batch(None);
    e.execute(C::InsertText("six".into()), fonts());
    select(&mut e, 0, 4, 7);
    let c = e.caret_at(Position::text(StoryId::Body, texts_ids(&e)[0], Unit(13))).expect("laid out");
    let (x, y) = e.stack().to_doc(c.page, c.x, (c.top + c.bottom) / 2.0).expect("on the stack");
    let out = e.drag_out_drop(x, y, false, fonts());
    assert!(matches!(out, super::super::commands::Outcome::Unchanged(_)), "{out:?}");
    assert_eq!(texts(&e), ["one six three"]);
}

#[test]
fn a_tracked_move_of_an_empty_paragraph() {
    for accept in [true, false] {
        let mut e = edit(concat!(
            "<w:p><w:r><w:t>one</w:t></w:r></w:p>",
            r#"<w:p><w:pPr><w:jc w:val="center"/></w:pPr></w:p>"#,
            "<w:p><w:r><w:t>three</w:t></w:r></w:p>",
        ));
        e.execute(C::SetTrackChanges(true), fonts());
        caret(&mut e, 1, 0);
        let out = e.move_paragraphs(true, fonts());
        assert!(matches!(out, crate::editing::commands::Outcome::Changed(_)), "{out:?}");
        if accept {
            e.execute(C::AcceptAllRevisions(Default::default()), fonts());
            assert_eq!(texts(&e), ["", "one", "three"]);
            assert!(centred(&e, ""), "it keeps its properties");
        } else {
            e.execute(C::RejectAllRevisions(Default::default()), fonts());
            assert_eq!(texts(&e), ["one", "", "three"]);
            assert!(centred(&e, ""));
        }
    }
}

#[test]
fn formatting_restrictions_keep_auto_format_unless_it_may_override_them() {
    use crate::api::command::DocumentProtection;
    use crate::editing::commands::Outcome;
    for allow in [false, true] {
        let mut e = edit("<w:p><w:r><w:t>above</w:t></w:r></w:p><w:p/>");
        assert!(matches!(e.set_formatting_restrictions(Some(allow), Some(true), None, fonts()), Outcome::Changed(_) | Outcome::Unchanged(_)));
        let protect = DocumentProtection { formatting: Some(true), enforce: Some(true), ..Default::default() };
        assert!(matches!(e.execute(C::SetDocumentProtection(protect), fonts()), Outcome::Changed(_)));
        assert!(matches!(e.set_formatting_restrictions(Some(!allow), None, None, fonts()), Outcome::Refused(_)), "not while enforced");
        assert!(e.style_switch_blocked(true) && !e.style_switch_blocked(false));
        caret(&mut e, 1, 0);
        type_in(&mut e, "---\n");
        assert_eq!(para(&e, 1).style.border_bottom.is_some(), allow, "override {allow}");
        let saved = crate::docx::write::save(e.document()).expect("saves");
        let again = Editor::open(&saved, fonts()).expect("reopens");
        let s = &again.document().settings;
        assert_eq!((s.auto_format_override, s.style_lock_theme, s.style_lock_style_set), (allow, true, false));
    }
    let e = edit("<w:p><w:r><w:t>a</w:t></w:r></w:p>");
    let min = e.recommended_minimum_styles();
    assert!(min.iter().any(|s| s == "Normal"), "{min:?}");
}

/// Track moves off: a tracked paragraph move records an insertion and a deletion; Track formatting off: formatting
/// applies untracked while text edits are still tracked. Both are saved settings.
#[test]
fn track_moves_and_formatting_options() {
    let mut e = tracked_three();
    assert_eq!(e.track_options(), (true, true));
    assert_eq!(e.set_track_options(false, false), crate::editing::commands::Outcome::Moved);
    assert!(e.is_dirty(), "a saved setting");
    caret(&mut e, 1, 0);
    e.move_paragraphs(true, fonts());
    assert_eq!(has_moves(&e), (false, false), "an insertion and a deletion");
    // "two" both inserted and deleted, as ordinary revisions
    use crate::docx::review::MarkKind;
    let marked = |e: &Editor, kind: MarkKind| {
        e.document().sections[0].blocks.iter().filter_map(|b| if let crate::docx::Block::Para(p) = b { Some(p) } else { None }).any(|p| {
            p.inlines.iter().any(|i| match i {
                crate::docx::Inline::Text(t, r) if t == "two" => [&r.rev.ins, &r.rev.del].into_iter().flatten().any(|m| m.kind == kind),
                _ => false,
            })
        })
    };
    assert!(marked(&e, MarkKind::Insert) && marked(&e, MarkKind::Delete), "still tracked, as an insertion and a deletion");
    assert!(!marked(&e, MarkKind::MoveTo) && !marked(&e, MarkKind::MoveFrom), "not as a move");
    e.execute(C::SelectAll, fonts());
    e.execute(C::SetBold(true), fonts());
    e.execute(C::SetAlignment(crate::api::value::Alignment::Right), fonts());
    for b in &e.document().sections[0].blocks {
        let crate::docx::Block::Para(p) = b else { continue };
        assert!(p.style.change.is_none(), "no paragraph change recorded");
        assert_eq!(p.style.jc, crate::docx::props::Jc::Right, "right-aligned");
        assert!(p.inlines.iter().filter_map(|i| i.run()).all(|r| r.rev.fmt.is_none()), "no formatting change recorded");
        assert!(
            p.inlines.iter().filter(|i| matches!(i, crate::docx::Inline::Text(t, _) if !t.is_empty())).filter_map(|i| i.run()).all(|r| r.bold),
            "bold"
        );
    }
    let bytes = crate::docx::write::save(e.document()).expect("saves");
    let back = crate::docx::read_editable(&bytes).expect("reads back");
    assert!(back.settings.do_not_track_moves && back.settings.do_not_track_formatting, "written as the two settings");
    let mut e = crate::editing::Editor::with_document(back, fonts());
    assert_eq!(e.set_track_options(true, true), crate::editing::commands::Outcome::Moved);
    let again = crate::docx::read_editable(&crate::docx::write::save(e.document()).expect("saves")).expect("reads back");
    assert!(!again.settings.do_not_track_moves && !again.settings.do_not_track_formatting, "and cleared");
}

/// Tracked typing in a long document follows each edit in the revision marks balloons and the review counts come
/// from: they are found from the whole document once.
#[test]
fn tracked_typing_follows_the_marks_without_finding_them_again() {
    let para = |i: usize| {
        format!(
            r#"<w:p><w:ins w:id="{i}" w:author="A"><w:r><w:t>new</w:t></w:r></w:ins><w:r><w:t> text</w:t></w:r><w:del w:id="{}" w:author="B"><w:r><w:delText>gone</w:delText></w:r></w:del></w:p>"#,
            i + 1000
        )
    };
    let mut e = edit(&(0..300).map(para).collect::<String>());
    e.set_markup_view(crate::docx::review::MarkupView::All);
    e.set_balloon_mode(crate::docx::review::BalloonMode::Balloons);
    e.set_tracking(true);
    let pages = e.snapshot().pages().len();
    let (all, _) = e.review_counts();
    assert_eq!((all.insertions, all.deletions), (300, 300));
    let balloons = e.view(0..1).revision_balloons.len();
    assert!(balloons > 0);
    for (k, at) in [(5, 3), (5, 4), (0, 8), (120, 0), (299, 2)] {
        caret(&mut e, k, at);
        e.execute(C::InsertText("x".into()), fonts());
        let _ = e.view(0..1);
        e.check_review_marks().expect("followed");
    }
    let (all, _) = e.review_counts();
    assert!(all.insertions > 300 && all.deletions == 300, "the typing counted: {all:?}");
    assert_eq!(e.mark_builds, 1, "found once");
    e.balloon_lookups = 0;
    let _ = e.view(0..1);
    assert!(e.balloon_lookups <= balloons + 2, "{} lookups beside one page", e.balloon_lookups);
    assert!(e.view(0..0).revision_balloons.is_empty() && pages > 1);
}

/// The review counts are kept while typing in a paragraph without marks, and counted again after a comment is added.
#[test]
fn review_counts_are_kept_while_typing_elsewhere() {
    let mut e = edit(concat!(
        r#"<w:p><w:ins w:id="1" w:author="A"><w:r><w:t>new</w:t></w:r></w:ins><w:r><w:t> text</w:t></w:r></w:p>"#,
        "<w:p><w:r><w:t>plain</w:t></w:r></w:p>",
    ));
    let (all, _) = e.review_counts();
    assert_eq!((all.insertions, all.comments), (1, 0));
    caret(&mut e, 1, 5);
    e.execute(C::InsertText("!".into()), fonts());
    assert_eq!(e.review_counts().0, all);
    caret(&mut e, 0, 4);
    e.execute(C::InsertText("x".into()), fonts());
    assert_eq!(e.review_counts().0, all);
    assert_eq!(e.mark_builds, 1, "kept, then followed");
    e.add_comment(&crate::editing::comments::RichText::plain("Note"), "A", fonts());
    assert_eq!(e.review_counts().0.comments, 1, "counted again");
}

/// Seeded edits, tracked and not, in paragraphs, table cells and across paragraph marks: the revision marks followed
/// edit by edit are always those of the whole document.
#[test]
fn followed_revision_marks_match_the_document() {
    let body = concat!(
        r#"<w:p><w:ins w:id="1" w:author="A"><w:r><w:t>new</w:t></w:r></w:ins><w:r><w:t> text </w:t></w:r><w:del w:id="2" w:author="B"><w:r><w:delText>gone</w:delText></w:r></w:del><w:r><w:t> end</w:t></w:r></w:p>"#,
        "<w:p><w:r><w:t>plain words here</w:t></w:r></w:p>",
        r#"<w:p><w:pPr><w:jc w:val="center"/><w:pPrChange w:id="3" w:author="A"><w:pPr/></w:pPrChange></w:pPr><w:r><w:t>centred</w:t></w:r></w:p>"#,
        r#"<w:p><w:pPr><w:jc w:val="right"/><w:pPrChange w:id="4" w:author="A"><w:pPr/></w:pPrChange></w:pPr><w:r><w:rPr><w:b/><w:rPrChange w:id="5" w:author="B"><w:rPr/></w:rPrChange></w:rPr><w:t>bold</w:t></w:r></w:p>"#,
        r#"<w:tbl><w:tblGrid><w:gridCol w:w="3000"/><w:gridCol w:w="3000"/></w:tblGrid>"#,
        r#"<w:tr><w:tc><w:p><w:ins w:id="6" w:author="A"><w:r><w:t>cell</w:t></w:r></w:ins></w:p></w:tc><w:tc><w:p><w:ins w:id="7" w:author="A"><w:r><w:t>next</w:t></w:r></w:ins></w:p><w:p><w:r><w:t>two</w:t></w:r></w:p></w:tc></w:tr>"#,
        r#"<w:tr><w:tc><w:p/></w:tc><w:tc><w:p><w:del w:id="8" w:author="B"><w:r><w:delText>old</w:delText></w:r></w:del></w:p></w:tc></w:tr>"#,
        "</w:tbl>",
        r#"<w:p><w:ins w:id="9" w:author="A"><w:r><w:t>after</w:t></w:r></w:ins></w:p>"#,
        r#"<w:p><w:pPr><w:rPr><w:ins w:id="10" w:author="A"/></w:rPr></w:pPr><w:ins w:id="11" w:author="A"><w:r><w:t>tail</w:t></w:r></w:ins></w:p>"#,
        r#"<w:p><w:ins w:id="12" w:author="A"><w:r><w:t>head</w:t></w:r></w:ins><w:r><w:t> rest</w:t></w:r></w:p>"#,
        "<w:p/>",
    );
    let mut seed = 0x9e37_79b9_7f4a_7c15_u64;
    let mut below = |n: usize| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed % n.max(1) as u64) as usize
    };
    let back = || C::Delete(crate::api::command::DeleteOptions { direction: crate::api::value::Direction::Backward, unit: None });
    let forward = || C::Delete(crate::api::command::DeleteOptions { direction: crate::api::value::Direction::Forward, unit: None });
    let mut e = edit(body);
    e.set_markup_view(crate::docx::review::MarkupView::All);
    e.set_balloon_mode(crate::docx::review::BalloonMode::Balloons);
    for step in 0..400 {
        let _ = e.review_counts();
        if step % 7 == 0 {
            let _ = e.view(0..1);
        }
        e.set_tracking(below(3) > 0);
        let ids = texts_ids(&e);
        let id = ids[below(ids.len())];
        let len = e.index().para(e.document(), id).map_or(0, |p| crate::editing::step::para_len(p).0);
        let (a, b) = (below(len as usize + 1) as u32, below(len as usize + 1) as u32);
        let at = |u: u32| Position::text(StoryId::Body, id, Unit(u));
        e.selection = Selection::Caret(at(a));
        let c = match below(9) {
            0..=2 => C::InsertText(["x", "ab ", " "][below(3)].into()),
            3 => back(),
            4 => forward(),
            5 => C::InsertParagraph,
            6 => {
                e.selection = Selection::range(at(a.min(b)), at(a.max(b)), Default::default());
                C::SetBold(below(2) == 0)
            }
            7 => C::Undo(Default::default()),
            _ => C::Redo(Default::default()),
        };
        e.execute(c.clone(), fonts());
        e.check_review_marks().unwrap_or_else(|why| panic!("step {step} ({c:?} at {a}..{b}): {why}"));
    }
    assert!(e.mark_builds < 20, "found again {} times", e.mark_builds);
}

/// Seeded edits, tracked and not, in the last paragraph of an inserted, deleted or merged cell and of a section with a
/// changed layout follow the spans those changes cover: the marks are found from the document once.
#[test]
fn edits_ending_cell_and_section_changes_follow_the_marks() {
    fn ends(blocks: &[crate::docx::Block], out: &mut Vec<crate::layout::ParaId>) {
        for b in blocks {
            let crate::docx::Block::Table(t) = b else { continue };
            for c in t.rows.iter().flat_map(|r| &r.cells) {
                if c.rev.structural.is_some()
                    && let Some(crate::docx::Block::Para(p)) = c.blocks.last()
                {
                    out.push(crate::layout::ParaId(p.id));
                }
                ends(&c.blocks, out);
            }
        }
    }
    let back = || C::Delete(crate::api::command::DeleteOptions { direction: crate::api::value::Direction::Backward, unit: None });
    let forward = || C::Delete(crate::api::command::DeleteOptions { direction: crate::api::value::Direction::Forward, unit: None });
    let mut seed = 0x2545_f491_4f6c_dd1d_u64;
    let mut below = |n: usize| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed % n.max(1) as u64) as usize
    };
    for name in ["review/FX-REVIEW-CELLS.docx", "review/FX-REVIEW-SECTIONS.docx"] {
        let mut e = super::open(name);
        e.complete(fonts());
        e.set_markup_view(crate::docx::review::MarkupView::All);
        e.set_balloon_mode(crate::docx::review::BalloonMode::Balloons);
        let _ = e.review_counts();
        if let Some(crate::docx::Block::Para(p)) = e.document().sections.last().and_then(|s| s.blocks.last()) {
            // Untracked typing at a changed section's end joins and parts no marks
            let id = crate::layout::ParaId(p.id);
            let len = crate::editing::step::para_len(p).0;
            let was = e.revision_marks();
            e.set_tracking(false);
            e.selection = Selection::Caret(Position::text(StoryId::Body, id, Unit(len)));
            e.execute(C::InsertText("typed".into()), fonts());
            assert_eq!(e.revision_marks(), was, "{name}: no marks touched");
            let _ = e.review_counts();
            e.check_review_marks().unwrap_or_else(|why| panic!("{name} typed at the end: {why}"));
        }
        for step in 0..150 {
            let mut targets = Vec::new();
            for s in &e.document().sections {
                ends(&s.blocks, &mut targets);
                if s.change.is_some()
                    && let Some(crate::docx::Block::Para(p)) = s.blocks.last()
                {
                    targets.push(crate::layout::ParaId(p.id));
                }
            }
            assert!(!targets.is_empty(), "{name}: spans to edit");
            e.set_tracking(below(2) == 0);
            let id = targets[below(targets.len())];
            let len = e.index().para(e.document(), id).map_or(0, |p| crate::editing::step::para_len(p).0);
            let (a, b) = (below(len as usize + 1) as u32, below(len as usize + 1) as u32);
            let at = |u: u32| Position::text(StoryId::Body, id, Unit(u));
            e.selection = Selection::Caret(at(a));
            let c = match below(8) {
                0..=2 => C::InsertText(["x", "ab ", " "][below(3)].into()),
                3 if a > 0 => back(),
                4 if a < len => forward(),
                5 => C::InsertParagraph,
                6 => {
                    e.selection = Selection::range(at(a.min(b)), at(a.max(b)), Default::default());
                    C::SetBold(below(2) == 0)
                }
                3 | 4 => C::Undo(Default::default()),
                _ => C::Redo(Default::default()),
            };
            e.execute(c.clone(), fonts());
            let _ = e.review_counts();
            if step % 5 == 0 {
                let _ = e.view(0..1);
            }
            e.check_review_marks().unwrap_or_else(|why| panic!("{name} step {step} ({c:?} at {a}..{b}): {why}"));
        }
        assert_eq!(e.mark_builds, 1, "{name}: found once");
    }
}

/// A view of one page looks up anchors only for the balloons whose revisions start on it; deleted text at a line's
/// start has its balloon where the text after it starts.
#[test]
fn balloons_look_up_only_the_visible_pages() {
    let page = |i: usize| {
        format!(
            r#"<w:p><w:del w:id="{i}" w:author="A" w:date="2024-01-01T00:00:00Z"><w:r><w:delText>gone</w:delText></w:r></w:del><w:r><w:t> kept</w:t></w:r></w:p><w:p><w:r><w:br w:type="page"/></w:r></w:p>"#
        )
    };
    let mut e = edit(&(1..=6).map(page).collect::<String>());
    e.set_markup_view(crate::docx::review::MarkupView::All);
    e.set_balloon_mode(crate::docx::review::BalloonMode::Balloons);
    e.complete(fonts());
    let pages = e.snapshot().pages().len();
    let all = e.view(0..pages).revision_balloons;
    let first = e.view(0..1).revision_balloons.len();
    let per_page: Vec<usize> = (0..pages).map(|p| all.iter().filter(|b| b.page == p).count()).collect();
    assert_eq!((pages, per_page, first), (6, vec![1; 6], 1), "a balloon on each page, for deleted text at a line's start too");
    e.balloon_lookups = 0;
    let _ = e.view(0..1);
    assert!(e.balloon_lookups <= first + 1, "{} lookups for {first} balloons", e.balloon_lookups);
}
