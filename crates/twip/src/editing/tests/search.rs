//! Find's special characters, match options, regular expressions, formatting and scope, and what replacing with them
//! does

use super::empty_stories::{NS, anchored_box, build};
use super::{blank_body, fonts, para_id, texts};
use crate::api::command::Command;
use crate::api::services::FontStyle;
use crate::api::state::Tri;
use crate::editing::Editor;
use crate::editing::commands::Outcome;
use crate::editing::position::{Position, StoryId};
use crate::editing::search::{MatchOptions, SearchError};
use crate::editing::selection::Selection;
use crate::layout::Unit;
use crate::services::find::{FindFormat, FindSpec, Search, replace_with};

fn t(s: &str) -> String {
    format!("<w:r><w:t xml:space=\"preserve\">{s}</w:t></w:r>")
}

fn p(s: &str) -> String {
    format!("<w:p>{}</w:p>", t(s))
}

fn ed(body: &str) -> Editor {
    Editor::open(&blank_body(body), fonts()).expect("opens")
}

fn spec(text: &str) -> FindSpec {
    FindSpec { text: text.into(), ..FindSpec::default() }
}

fn search(s: FindSpec) -> Search {
    Search::new(s).expect("a valid search")
}

/// Every match's text, as Find reads it.
fn found(e: &Editor, s: FindSpec) -> Vec<String> {
    search(s).all(e).into_iter().map(|h| crate::services::find::describe(e, h, 1).text).collect()
}

fn replace_all(e: &mut Editor, s: FindSpec, with: &str) -> usize {
    let s = search(s);
    let hits = s.all(e);
    replace_with(e, &s, &hits, with, None, true, fonts()).expect("replaces").0
}

#[test]
fn special_characters_find_what_they_name() {
    let e = ed(&format!(
        "<w:p>{}<w:r><w:tab/></w:r>{}<w:r><w:br/></w:r>{}</w:p>{}",
        t("Total"),
        t("42 a^b x\u{2014}y \u{a0}z"),
        t("line"),
        p("tab\there  and   gaps")
    ));
    assert_eq!(found(&e, spec("Total^t")), ["Total "]);
    assert_eq!(found(&e, spec("^#^#")), ["42"]);
    assert_eq!(found(&e, spec("a^^b")), ["a^b"]);
    assert_eq!(found(&e, spec("x^+y")), ["x\u{2014}y"]);
    assert_eq!(found(&e, spec("^sz")), ["\u{a0}z"]);
    assert_eq!(found(&e, spec("^lline")), [" line"], "the line break reads as a space in the results list");
    assert_eq!(found(&e, spec("^$^$^$^w")), ["tal ", "tab ", "ere  ", "and   "], "three letters then white space");
    assert_eq!(found(&e, spec("and^wgaps")), ["and   gaps"], "^w takes the whole run");
    assert_eq!(found(&e, spec("^?^?^?^?^?"))[..2], ["Total", " 42 a"], "any character, a tab too");
    assert!(matches!(Search::new(spec("^z")), Err(SearchError::Code(_))));
}

#[test]
fn a_paragraph_mark_matches_across_paragraphs_of_one_story() {
    let mut e = ed(&format!("{}{}{}", p("one end."), p("Two"), p("three")));
    let hits = search(spec("end.^pTwo")).all(&e);
    assert_eq!(hits.len(), 1);
    let h = hits[0];
    assert_eq!((h.para, h.start, h.last, h.end), (para_id(&e, 0), 4, para_id(&e, 1), 3));
    assert_eq!(found(&e, spec("Two^p^pthree")), Vec::<String>::new(), "no empty paragraph between");
    assert_eq!(found(&e, spec("^p")).len(), 2, "every mark but the story's last");
    assert_eq!(replace_all(&mut e, spec("end.^pTwo"), "joined"), 1);
    assert_eq!(texts(&e), ["one joined", "three"]);
    assert_eq!(replace_all(&mut e, spec("joined"), "a^pb"), 1);
    assert_eq!(texts(&e), ["one a", "b", "three"], "a ^p in the replacement splits the paragraph");
    assert_eq!(replace_all(&mut e, spec("^p"), " "), 2);
    assert_eq!(texts(&e), ["one a b three"]);
    assert!(matches!(e.execute(Command::Undo, fonts()), Outcome::Changed(_)));
    assert_eq!(texts(&e), ["one a", "b", "three"], "one undo step");
}

#[test]
fn a_paragraph_mark_doesnt_match_into_a_table() {
    let cell = |s: &str| format!("<w:tc>{}</w:tc>", p(s));
    let table = format!(
        r#"<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w="2000"/><w:gridCol w:w="2000"/></w:tblGrid><w:tr>{}{}</w:tr></w:tbl>"#,
        cell("a"),
        cell("b")
    );
    let e = ed(&format!("{}{table}{}", p("x"), p("y")));
    assert!(found(&e, spec("x^pa")).is_empty(), "across the table's start");
    assert!(found(&e, spec("a^pb")).is_empty(), "across cells");
}

#[test]
fn a_section_break_matches_only_between_sections() {
    let sect = r#"<w:p><w:pPr><w:sectPr><w:type w:val="continuous"/></w:sectPr></w:pPr><w:r><w:t>end</w:t></w:r></w:p>"#;
    let mut e = ed(&format!("{}{sect}{}", p("first"), p("next")));
    assert_eq!(e.document().sections.len(), 2);
    assert_eq!(found(&e, spec("end^bnext")).len(), 1);
    assert!(found(&e, spec("end^pnext")).is_empty(), "a section break isn't a paragraph mark");
    assert!(found(&e, spec("first^bend")).is_empty());
    assert_eq!(found(&e, spec("first^pend")).len(), 1);
    assert_eq!(replace_all(&mut e, spec("^l"), "x"), 0);
}

#[test]
fn page_or_section_break_matches_either() {
    let sect = r#"<w:p><w:pPr><w:sectPr><w:type w:val="continuous"/></w:sectPr></w:pPr><w:r><w:t>end</w:t></w:r></w:p>"#;
    let body = format!("<w:p>{}<w:r><w:br w:type=\"page\"/></w:r>{}</w:p>{sect}{}", t("pg"), t("after"), p("next"));
    let e = ed(&body);
    let all = search(spec("^12")).all(&e);
    assert_eq!(all.len(), 2, "the page break and the section break: {all:?}");
    assert_eq!(all[0].last, all[0].para, "the page break, within its paragraph");
    assert_eq!((all[1].para, all[1].last, all[1].end), (para_id(&e, 1), para_id(&e, 2), 0), "the section break, between sections");
    assert_eq!(found(&e, spec("pg^12after")), ["pg after"]);
    assert_eq!(search(spec("end^12next")).all(&e).len(), 1, "across the section break");
    assert!(search(spec("^m")).all(&e).len() == 1, "^m is the page break only");
    assert!(search(spec("after^12")).all(&e).is_empty(), "a paragraph mark isn't a section break");
}

#[test]
fn objects_breaks_and_fields_match_their_codes() {
    let foot = format!(r#"<w:footnotes {NS}><w:footnote w:id="1"><w:p>{}</w:p></w:footnote></w:footnotes>"#, t("note text"));
    let field = r#"<w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> QUOTE "val" </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>val</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r>"#;
    let body = format!(
        r#"<w:p>{}<w:r><w:footnoteReference w:id="1"/></w:r>{}</w:p><w:p>{}<w:r><w:br w:type="page"/></w:r>{}<w:r><w:br w:type="column"/></w:r>{}</w:p><w:p>{}{field}{}</w:p>"#,
        t("Ref"),
        t(" tail"),
        t("pg"),
        t("col"),
        t("end"),
        t("f["),
        t("]")
    );
    let e = Editor::open(&build(&body, "", &[("footnotes.xml", "footnotes", "footnotes", &foot)]), fonts()).expect("opens");
    assert_eq!(search(spec("Ref^f")).all(&e).len(), 1);
    assert!(search(spec("Ref^e")).all(&e).is_empty());
    assert_eq!(search(spec("pg^m")).all(&e).len(), 1);
    assert!(search(spec("pg^n")).all(&e).is_empty());
    assert_eq!(search(spec("col^n")).all(&e).len(), 1);
    let fields = search(spec("[^d]")).all(&e);
    assert_eq!(fields.len(), 1, "a field whole between its brackets");
    let h = fields[0];
    let len = e.index().para(e.document(), h.para).map_or(0, |p| crate::editing::step::para_len(p).0);
    assert_eq!((h.start, h.end), (1, len), "[, the field's begin, code, separator, result and end, then ]: {h:?}");
    assert!(search(spec("[val]")).all(&e).is_empty(), "the result alone crosses the field's markers");
    assert_eq!(search(spec("note text")).all(&e).len(), 1, "the footnote is searched");
}

#[test]
fn word_edges_and_ignored_characters() {
    let e = ed(&p("pretest tested test, te-st t e s t"));
    let o = |prefix, suffix| FindSpec { options: MatchOptions { prefix, suffix, ..Default::default() }, ..spec("test") };
    assert_eq!(search(o(true, false)).all(&e).len(), 2, "tested and test");
    assert_eq!(search(o(false, true)).all(&e).len(), 2, "pretest and test");
    assert_eq!(search(o(true, true)).all(&e).len(), 1);
    let ignore = |punct, space| FindSpec { options: MatchOptions { ignore_punct: punct, ignore_space: space, ..Default::default() }, ..spec("test") };
    assert_eq!(search(ignore(true, false)).all(&e).len(), 4, "te-st too");
    assert_eq!(search(ignore(false, true)).all(&e).len(), 4, "t e s t too");
    let hit = search(ignore(false, true)).all(&e)[3];
    assert_eq!(hit.end - hit.start, 7, "spanning the spaces passed over");
}

#[test]
fn regular_expressions_find_and_replace_with_groups() {
    let mut e = ed(&format!("{}{}", p("Call 555-1234 or 555-9876."), p("Mail ann@example.com")));
    let re = |text: &str| FindSpec { regex: true, ..spec(text) };
    assert_eq!(found(&e, re(r"\d{3}-\d{4}")), ["555-1234", "555-9876"]);
    assert_eq!(found(&e, re("CALL")), ["Call"], "ignoring case unless asked");
    assert!(found(&e, FindSpec { options: MatchOptions { case: true, ..Default::default() }, ..re("CALL") }).is_empty());
    assert!(matches!(Search::new(re("(unclosed")), Err(SearchError::Regex(_))));
    assert_eq!(replace_all(&mut e, re(r"(\d{3})-(?<n>\d{4})"), "${n}/$1"), 2);
    assert_eq!(texts(&e)[0], "Call 1234/555 or 9876/555.");
    assert_eq!(replace_all(&mut e, re(r"(\w+)@(\w+)\.com"), "$2: $1"), 1);
    assert_eq!(texts(&e)[1], "Mail example: ann");
}

#[test]
fn find_within_the_selection_and_without_wrapping() {
    let mut e = ed(&format!("{}{}{}", p("apple one"), p("apple two"), p("apple three")));
    let (a, b) = (para_id(&e, 1), para_id(&e, 2));
    e.set_caret_for_benchmark(Selection::range(
        Position::text(StoryId::Body, a, Unit(0)),
        Position::text(StoryId::Body, b, Unit(3)),
        Default::default(),
    ));
    let within = e.selected_ranges();
    let inside = FindSpec { within: Some(within.clone()), ..spec("apple") };
    assert_eq!(search(inside.clone()).all(&e).len(), 1, "the third is cut by the selection's end");
    assert_eq!(replace_all(&mut e, inside, "pear"), 1);
    assert_eq!(texts(&e), ["apple one", "pear two", "apple three"]);
    let (s, t) = e.selection().ordered(e.index()).expect("a selection");
    assert_eq!((s.para_offset(), t.para_offset()), (Some((a, Unit(0))), Some((b, Unit(3)))), "kept round what it covered");
    e.set_caret_for_benchmark(Selection::Caret(Position::text(StoryId::Body, b, Unit(3))));
    let down = FindSpec { wrap: false, ..spec("apple") };
    assert!(search(down.clone()).next(&e, false).is_none(), "Down stops at the end");
    let (hit, wrapped) = search(spec("apple")).next(&e, false).expect("All wraps");
    assert_eq!((hit.para, wrapped), (para_id(&e, 0), true));
    assert!(search(down).next(&e, true).is_some_and(|(h, w)| h.para == para_id(&e, 0) && !w), "Up from inside the third's finds the first");
}

#[test]
fn formatting_alone_and_with_text() {
    let bold = |s: &str| format!("<w:r><w:rPr><w:b/></w:rPr><w:t xml:space=\"preserve\">{s}</w:t></w:r>");
    let mut e = ed(&format!("<w:p>{}{}{}</w:p>{}", t("plain apple "), bold("bold apple"), t(" end"), p("apple")));
    let is_bold =
        FindFormat { font: crate::api::services::FontDialog { style: Tri::Uniform(FontStyle::Bold), ..Default::default() }, ..Default::default() };
    let with_format = |text: &str| FindSpec { format: Some(is_bold.clone()), ..spec(text) };
    assert_eq!(found(&e, with_format("apple")), ["apple"]);
    assert_eq!(found(&e, with_format("")), ["bold apple"], "every stretch with the formatting");
    let heading = FindFormat { style: Some(crate::api::value::StyleId("Heading1".into())), ..Default::default() };
    assert!(found(&e, FindSpec { format: Some(heading), ..spec("") }).is_empty());
    // Replace with formatting: italic onto each replacement; with no text, only the formatting.
    let italic =
        FindFormat { font: crate::api::services::FontDialog { style: Tri::Uniform(FontStyle::Italic), ..Default::default() }, ..Default::default() };
    let s = search(spec("apple"));
    let hits = s.all(&e);
    let (n, _) = replace_with(&mut e, &s, &hits, "", Some(&italic), true, fonts()).expect("formats");
    assert_eq!(n, 3);
    assert_eq!(texts(&e)[0], "plain apple bold apple end", "the text kept");
    let is_italic = FindSpec { format: Some(italic.clone()), ..spec("") };
    assert_eq!(found(&e, is_italic.clone()), ["apple", "apple", "apple"]);
    assert!(matches!(e.execute(Command::Undo, fonts()), Outcome::Changed(_)));
    assert!(found(&e, is_italic).is_empty(), "one undo step");
    let s = search(with_format("apple"));
    let hits = s.all(&e);
    replace_with(&mut e, &s, &hits, "pear", Some(&italic), true, fonts()).expect("replaces");
    assert_eq!(texts(&e)[0], "plain apple bold pear end");
    assert_eq!(found(&e, FindSpec { format: Some(italic), ..spec("") }), ["pear"]);
}

#[test]
fn the_found_text_and_the_clipboard_in_a_replacement() {
    let mut e = ed(&format!("<w:p>{}<w:r><w:rPr><w:b/></w:rPr><w:t>bold</w:t></w:r></w:p>{}", t("x "), p("clip")));
    assert_eq!(replace_all(&mut e, spec("bold"), "[^&]"), 1);
    assert_eq!(texts(&e)[0], "x [bold]");
    let is_bold =
        FindFormat { font: crate::api::services::FontDialog { style: Tri::Uniform(FontStyle::Bold), ..Default::default() }, ..Default::default() };
    assert_eq!(found(&e, FindSpec { format: Some(is_bold), ..spec("") }), ["[bold]"], "the replacement takes the found text's formatting");
    let c = para_id(&e, 1);
    e.set_caret_for_benchmark(Selection::range(
        Position::text(StoryId::Body, c, Unit(0)),
        Position::text(StoryId::Body, c, Unit(4)),
        Default::default(),
    ));
    e.copy();
    assert_eq!(replace_all(&mut e, spec("x"), "^c^c"), 1);
    assert_eq!(texts(&e), ["clipclip [bold]", "clip"]);
}

#[test]
fn replace_all_with_tracking_records_each_replacement() {
    let ins = r#"<w:ins w:id="9" w:author="Other" w:date="2020-01-01T00:00:00Z"><w:r><w:t xml:space="preserve">apple inserted </w:t></w:r></w:ins>"#;
    let del = r#"<w:del w:id="8" w:author="Other" w:date="2020-01-01T00:00:00Z"><w:r><w:delText xml:space="preserve">apple deleted </w:delText></w:r></w:del>"#;
    let mut e = ed(&format!("<w:p>{}{ins}{del}{}</w:p>", t("apple "), t("apple")));
    e.set_author("Me", None);
    assert!(matches!(e.execute(Command::SetTrackChanges(true), fonts()), Outcome::Changed(_) | Outcome::Moved | Outcome::Unchanged(_)));
    assert_eq!(search(spec("apple")).all(&e).len(), 4, "All Markup shows the deleted text");
    assert_eq!(replace_all(&mut e, spec("apple"), "pear"), 3, "the deleted one is left");
    let me = e.document().people.persons.iter().position(|a| &*a.name == "Me").expect("an author");
    let p = match &e.document().sections[0].blocks[0] {
        crate::docx::Block::Para(p) => p.clone(),
        _ => panic!("a paragraph"),
    };
    let runs: Vec<(String, bool, bool)> = p
        .inlines
        .iter()
        .filter_map(|i| match i {
            crate::docx::Inline::Text(s, r) => Some((
                s.clone(),
                r.rev.ins.as_ref().is_some_and(|m| m.author.0 as usize == me),
                r.rev.del.as_ref().is_some_and(|m| m.author.0 as usize == me),
            )),
            _ => None,
        })
        .collect();
    let mine_del = runs.iter().filter(|r| r.2).map(|r| r.0.as_str()).collect::<String>();
    let mine_ins = runs.iter().filter(|r| r.1).map(|r| r.0.as_str()).collect::<String>();
    assert_eq!(mine_del.matches("apple").count(), 3, "deleted as mine, the other author's insertion too: {runs:?}");
    assert_eq!(mine_ins.matches("pear").count(), 3, "each replacement inserted as mine: {runs:?}");
    assert!(runs.iter().any(|r| r.0.contains("apple deleted")), "the other author's deletion stays");
    assert!(matches!(e.execute(Command::Undo, fonts()), Outcome::Changed(_)));
    assert_eq!(search(spec("pear")).all(&e).len(), 0, "one undo step");
}

#[test]
fn every_story_is_searched_in_order() {
    let foot = format!(r#"<w:footnotes {NS}><w:footnote w:id="1"><w:p>{}</w:p></w:footnote></w:footnotes>"#, t("kiwi foot"));
    let end = format!(r#"<w:endnotes {NS}><w:endnote w:id="1"><w:p>{}</w:p></w:endnote></w:endnotes>"#, t("kiwi end"));
    let hdr = format!(r#"<w:hdr {NS}><w:p>{}</w:p></w:hdr>"#, t("kiwi header"));
    let ftr = format!(r#"<w:ftr {NS}><w:p>{}</w:p></w:ftr>"#, t("kiwi footer"));
    let body = format!(
        "<w:p>{}<w:r><w:footnoteReference w:id=\"1\"/></w:r><w:r><w:endnoteReference w:id=\"1\"/></w:r>{}</w:p>",
        t("kiwi body"),
        anchored_box(&format!("<w:txbxContent><w:p>{}</w:p></w:txbxContent>", t("kiwi box")))
    );
    let sect = r#"<w:headerReference w:type="default" r:id="rIdP2"/><w:footerReference w:type="default" r:id="rIdP3"/>"#;
    let e = Editor::open(
        &build(
            &body,
            sect,
            &[
                ("footnotes.xml", "footnotes", "footnotes", &foot),
                ("endnotes.xml", "endnotes", "endnotes", &end),
                ("header1.xml", "header", "header", &hdr),
                ("footer1.xml", "footer", "footer", &ftr),
            ],
        ),
        fonts(),
    )
    .expect("opens");
    let after: Vec<String> =
        search(spec("kiwi")).all(&e).into_iter().map(|h| crate::services::find::describe(&e, h, 1).after.trim().to_string()).collect();
    // The body, its text boxes, the headers and footers by part name, the footnotes, the endnotes.
    assert_eq!(after, ["body", "box", "footer", "header", "foot", "end"]);
}

#[test]
fn a_replacement_takes_the_found_texts_formatting() {
    let mut e = ed(&format!("<w:p>{}<w:r><w:rPr><w:b/></w:rPr><w:t>bold</w:t></w:r></w:p>", t("x ")));
    assert_eq!(replace_all(&mut e, spec("bold"), "pear"), 1);
    let is_bold =
        FindFormat { font: crate::api::services::FontDialog { style: Tri::Uniform(FontStyle::Bold), ..Default::default() }, ..Default::default() };
    assert_eq!(found(&e, FindSpec { format: Some(is_bold), ..spec("") }), ["pear"]);
}

#[test]
fn a_replacement_across_paragraph_marks_takes_the_found_texts_formatting() {
    let style =
        |s: FontStyle| FindFormat { font: crate::api::services::FontDialog { style: Tri::Uniform(s), ..Default::default() }, ..Default::default() };
    let italic = "<w:p><w:r><w:rPr><w:i/></w:rPr><w:t>italic</w:t></w:r></w:p>";
    let mut e = ed(&format!("<w:p>{}<w:r><w:rPr><w:b/></w:rPr><w:t>Bold</w:t></w:r></w:p>{italic}", t("plain ")));
    assert_eq!(replace_all(&mut e, spec("Bold^p"), "X"), 1);
    assert_eq!(texts(&e), ["plain Xitalic"]);
    assert_eq!(found(&e, FindSpec { format: Some(style(FontStyle::Bold)), ..spec("") }), ["X"]);
    // An empty paragraph found from its start goes: the replacement takes its mark's formatting.
    let mut e = ed(&format!("{}<w:p><w:pPr><w:rPr><w:b/></w:rPr></w:pPr></w:p>{italic}", p("plain")));
    assert_eq!(replace_all(&mut e, spec("^pitalic"), "X"), 1);
    assert_eq!(texts(&e), ["plain", "X"]);
    assert_eq!(found(&e, FindSpec { format: Some(style(FontStyle::Bold)), ..spec("") }), ["X"]);
}

#[test]
fn comment_marks_find_the_commented_text_and_replacing_them_deletes_the_comments() {
    let c = |i: u32, s: &str| {
        format!(r#"<w:commentRangeStart w:id="{i}"/>{}<w:commentRangeEnd w:id="{i}"/><w:r><w:commentReference w:id="{i}"/></w:r>"#, t(s))
    };
    let comments = format!(
        r#"<w:comments {NS}><w:comment w:id="1" w:author="Ann"><w:p>{}</w:p></w:comment><w:comment w:id="2" w:author="Bob"><w:p>{}</w:p></w:comment></w:comments>"#,
        t("c1"),
        t("c2")
    );
    let body = format!("<w:p>{}{}{}</w:p>", c(1, "ann's"), t(" and "), c(2, "bob's"));
    let mut e = Editor::open(&build(&body, "", &[("comments.xml", "comments", "comments", &comments)]), fonts()).expect("opens");
    assert_eq!(found(&e, spec("^a")), ["ann's", "bob's"]);
    assert_eq!(found(&e, spec("s^a")), ["s", "s"], "a mark after text");
    let (hit, _) = search(spec("^a")).next(&e, false).expect("a comment");
    hit.select(&mut e, fonts());
    assert_eq!(e.selected_text(), "ann's");
    assert_eq!(replace_all(&mut e, spec("^a"), ""), 2);
    assert!(e.document().comments.by_id.is_empty(), "the comments deleted");
    assert_eq!(texts(&e), ["ann's and bob's"], "their text kept");
    assert!(matches!(e.execute(Command::Undo, fonts()), Outcome::Changed(_)));
    assert_eq!(e.document().comments.by_id.len(), 2, "one undo step");
}

#[test]
fn hostile_paragraphs_stay_bounded() {
    // Paragraphs of spaces, where every start of `^?^w` (or of `^?x` passing over white space) runs to the paragraph's
    // end: each is quadratic, and the budget is the whole search's, so many of them give up their matches past it too.
    let e = ed(&p(&" ".repeat(6_000)).repeat(4));
    let ignoring = FindSpec { options: MatchOptions { ignore_space: true, ..Default::default() }, ..spec("^? x") };
    for spec in [spec("^?^wx"), spec("^?^w^12x"), ignoring] {
        let s = search(spec);
        assert!(s.all(&e).is_empty());
        assert!(s.truncated(), "{:?} given up past the budget", s.spec().text);
    }
    assert!(search(spec("^?^w^px")).all(&e).is_empty(), "across paragraphs: one start each");
    let plain = search(spec("x"));
    assert!(plain.all(&e).is_empty() && !plain.truncated());
}

#[test]
fn dense_regular_expression_matches_are_all_found() {
    let e = ed(&p(&"a".repeat(10_000)));
    let s = search(FindSpec { regex: true, ..spec("a") });
    assert_eq!(s.all(&e).len(), 10_000);
    assert!(!s.truncated());
}

#[test]
fn find_next_within_the_selection_starts_at_its_start() {
    let mut e = ed(&format!("{}{}{}{}", p("apple one"), p("two apple"), p("apple three"), p("apple four")));
    let (b, c) = (para_id(&e, 1), para_id(&e, 2));
    let select = |e: &mut Editor| {
        e.set_caret_for_benchmark(Selection::range(
            Position::text(StoryId::Body, b, Unit(0)),
            Position::text(StoryId::Body, c, Unit(11)),
            Default::default(),
        ))
    };
    select(&mut e);
    let s = search(FindSpec { within: Some(e.selected_ranges()), ..spec("apple") });
    let (h, wrapped) = s.next(&e, false).expect("the first in the selection");
    assert_eq!((h.para, h.start, wrapped), (b, 4, false));
    h.select(&mut e, fonts());
    let (h, _) = s.next(&e, false).expect("the next in the selection");
    assert_eq!((h.para, h.start), (c, 0));
    h.select(&mut e, fonts());
    assert!(s.next(&e, false).is_none(), "stops at the selection's end");
    select(&mut e);
    let (h, _) = s.next(&e, true).expect("the last in the selection, going back");
    assert_eq!((h.para, h.start), (c, 0));
    h.select(&mut e, fonts());
    assert_eq!(s.next(&e, true).map(|(h, _)| (h.para, h.start)), Some((b, 4)));
}

#[test]
fn regular_expression_groups_keep_objects_and_breaks() {
    let pic = concat!(
        r#"<w:r><w:drawing><wp:inline xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing"><wp:extent cx="95250" cy="95250"/>"#,
        r#"<wp:docPr id="1" name="Picture 1"/><a:graphic xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">"#,
        r#"<a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture">"#,
        r#"<pic:nvPicPr><pic:cNvPr id="1" name="p.png"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip/><a:stretch><a:fillRect/></a:stretch></pic:blipFill>"#,
        r#"<pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="95250" cy="95250"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr></pic:pic>"#,
        r#"</a:graphicData></a:graphic></wp:inline></w:drawing></w:r>"#
    );
    let mut e = ed(&format!("<w:p>{}{pic}{}<w:r><w:br w:type=\"column\"/></w:r>{}</w:p>", t("a"), t("b c"), t("d")));
    let kinds = |e: &Editor| -> Vec<&'static str> {
        let crate::docx::Block::Para(p) = &e.document().sections[0].blocks[0] else { panic!("a paragraph") };
        p.inlines
            .iter()
            .filter_map(|i| match i {
                crate::docx::Inline::Picture(..) => Some("picture"),
                crate::docx::Inline::Break(crate::docx::BreakKind::Column, _) => Some("column"),
                crate::docx::Inline::Break(..) => Some("other break"),
                _ => None,
            })
            .collect()
    };
    assert_eq!(kinds(&e), ["picture", "column"]);
    let re = |text: &str| FindSpec { regex: true, ..spec(text) };
    assert_eq!(replace_all(&mut e, re("(.+)"), "[$1]$1"), 1);
    assert_eq!(kinds(&e), ["picture", "column", "picture", "column"], "each copy keeps the picture and the column break");
    let text = &texts(&e)[0];
    assert!(text.starts_with('[') && text.contains("b c") && !text.contains('\u{2028}'), "{text:?}");
    let long = Search::new(re("(.+)")).expect("compiles");
    let hits = long.all(&e);
    let huge = "$1".repeat(127);
    assert!(replace_with(&mut e, &long, &hits, &huge, None, true, fonts()).is_ok(), "within the cap");
    let e2 = ed(&p(&"x".repeat(40_000)));
    let hits = long.all(&e2);
    let mut e2 = e2;
    assert!(
        matches!(replace_with(&mut e2, &long, &hits, &huge, None, true, fonts()), Err(err) if err.to_string().contains("copy more than")),
        "past the cap: refused"
    );
}

#[test]
fn a_regular_expression_match_widened_to_a_cluster_is_replaced() {
    let mut e = ed(&p("a\u{1F44D}\u{1F3FD}b"));
    let s = FindSpec { regex: true, ..spec("\u{1F44D}") };
    assert_eq!(found(&e, s.clone()), ["\u{1F44D}\u{1F3FD}"], "the skin tone with it");
    assert_eq!(replace_all(&mut e, s, "X"), 1);
    assert_eq!(texts(&e), ["aXb"]);
}

#[test]
fn paragraph_formatting_tabs_and_frames() {
    use crate::api::command::AddTabStop;
    use crate::api::value::{Alignment, Points};
    let centred =
        "<w:p><w:pPr><w:jc w:val=\"center\"/><w:tabs><w:tab w:val=\"right\" w:pos=\"2880\"/></w:tabs></w:pPr><w:r><w:t>mid</w:t></w:r></w:p>";
    let framed = "<w:p><w:pPr><w:framePr w:w=\"2880\" w:wrap=\"around\" w:vAnchor=\"text\" w:hAnchor=\"page\" w:x=\"1440\"/></w:pPr><w:r><w:t>boxed</w:t></w:r></w:p>";
    let mut e = ed(&format!("{}{centred}{framed}", p("left")));
    let fmt = |f: FindFormat| FindSpec { format: Some(f), ..spec("") };
    let centre = FindFormat {
        paragraph: crate::api::services::ParagraphDialog { alignment: Tri::Uniform(Alignment::Center), ..Default::default() },
        ..Default::default()
    };
    assert_eq!(found(&e, fmt(centre.clone())), ["mid"]);
    let tab = |pt: f64| AddTabStop { position: Points(pt), alignment: Some(crate::api::value::TabAlignment::Right), leader: None };
    assert_eq!(found(&e, fmt(FindFormat { tabs: vec![tab(144.0)], ..Default::default() })), ["mid"]);
    assert!(found(&e, fmt(FindFormat { tabs: vec![tab(72.0)], ..Default::default() })).is_empty());
    let frame = crate::editing::find::frame_of(match &e.document().sections[0].blocks[2] {
        crate::docx::Block::Para(p) => p,
        _ => panic!("a paragraph"),
    });
    assert!(frame.is_some());
    assert_eq!(found(&e, fmt(FindFormat { frame: frame.clone(), ..Default::default() })), ["boxed"]);
    // Replace with paragraph formatting: the paragraph found takes it.
    let s = search(spec("left"));
    let hits = s.all(&e);
    replace_with(&mut e, &s, &hits, "", Some(&centre), true, fonts()).expect("formats");
    assert_eq!(found(&e, fmt(centre)), ["left", "mid"]);
}

#[test]
fn a_crafted_span_is_clamped_or_refused() {
    let hdr = format!(r#"<w:hdr {NS}><w:p>{}</w:p></w:hdr>"#, t("header"));
    let sect = r#"<w:headerReference w:type="default" r:id="rIdP0"/>"#;
    let body = format!("{}{}", p("one"), p("two"));
    let mut e = Editor::open(&build(&body, sect, &[("header1.xml", "header", "header", &hdr)]), fonts()).expect("opens");
    let (b0, b1) = (para_id(&e, 0), para_id(&e, 1));
    let h = search(spec("header")).all(&e)[0].para;
    let refused = |o: Outcome| !matches!(o, Outcome::Moved | Outcome::Changed(_));
    assert!(refused(e.select_span(b0, Unit(0), h, Unit(1), fonts())), "across stories");
    assert!(refused(e.select_span(b1, Unit(0), b0, Unit(1), fonts())), "ending before it starts");
    assert!(matches!(e.select_span(b0, Unit(99), b1, Unit(999), fonts()), Outcome::Moved));
    let (a, b) = e.selection().ordered(e.index()).expect("a range");
    assert_eq!((a.para_offset(), b.para_offset()), (Some((b0, Unit(3))), Some((b1, Unit(3)))), "each end clamped to its paragraph");
}

#[test]
fn copies_are_bounded_across_a_replace() {
    // Single characters, each pasted 127 times: within bounds per match, past them over the pass, in both modes.
    let mut e = ed(&p(&"x".repeat(5_000)).repeat(10));
    let s = search(spec("x"));
    let hits = s.all(&e);
    let with = "^&".repeat(127);
    assert!(matches!(replace_with(&mut e, &s, &hits, &with, None, true, fonts()), Err(err) if err.message.contains("copy more than")));
    // Groups over many paragraphs: each match within bounds, the pass past them.
    let s = search(FindSpec { regex: true, ..spec("(.+)") });
    let hits = s.all(&e);
    assert!(replace_with(&mut e, &s, &hits, &"$1".repeat(90), None, true, fonts()).is_err());
    assert_eq!(texts(&e)[0].len(), 5_000, "nothing replaced");
    // Just under the units copied, past the pieces pasted: refused before anything is copied.
    let mut e = ed(&p(&"x".repeat(1_000)).repeat(33));
    let s = search(spec("x"));
    let hits = s.all(&e);
    assert!(hits.len() * 127 <= crate::editing::search::MAX_EXPANSION);
    let started = std::time::Instant::now();
    assert!(matches!(replace_with(&mut e, &s, &hits, &with, None, true, fonts()), Err(err) if err.message.contains("pieces")));
    assert!(started.elapsed() < std::time::Duration::from_secs(5), "refused at once");
    assert_eq!(texts(&e)[0].len(), 1_000, "nothing replaced");
    // One paste a match costs what typing does: past MAX_PASTES matches, each pasting once, in both modes.
    let e = ed(&p(&"x ".repeat(25)).repeat(3_000));
    let s = search(spec("x"));
    let hits = s.all(&e);
    assert!(hits.len() > crate::editing::search::MAX_PASTES);
    assert!(s.replacements(&e, &hits, "[^&]").is_ok());
    let s = search(FindSpec { regex: true, ..spec(r"(\w+)") });
    let hits = s.all(&e);
    assert!(hits.len() > crate::editing::search::MAX_PASTES);
    assert!(s.replacements(&e, &hits, "\"$1\"").is_ok());
    assert!(s.replacements(&e, &hits, "$1$1").is_err(), "a second paste each is past the bound");
}

#[test]
fn a_selection_over_many_paragraphs_is_searched_in_bounded_time() {
    let mut e = ed(&p(&"a".repeat(25)).repeat(10_000));
    assert!(matches!(e.execute(Command::SelectAll, fonts()), Outcome::Changed(_) | Outcome::Moved));
    let within = e.selected_ranges();
    assert_eq!(within.len(), 10_000, "a range a paragraph");
    let started = std::time::Instant::now();
    let s = search(FindSpec { within: Some(within), ..spec("a") });
    assert_eq!(s.all(&e).len(), 250_000);
    assert!(started.elapsed() < std::time::Duration::from_secs(3), "each match checked against its own paragraph's ranges");
}

#[test]
fn replacing_many_comment_marks_stays_linear() {
    const N: usize = 20_000;
    let mut comments = format!("<w:comments {NS}>");
    let mut body = String::new();
    for i in 0..N {
        comments.push_str(&format!(r#"<w:comment w:id="{i}" w:author="Ann"><w:p>{}</w:p></w:comment>"#, t("c")));
        body.push_str(&format!(
            r#"<w:p><w:commentRangeStart w:id="{i}"/>{}<w:commentRangeEnd w:id="{i}"/><w:r><w:commentReference w:id="{i}"/></w:r></w:p>"#,
            t("x")
        ));
    }
    comments.push_str("</w:comments>");
    let mut e = Editor::open(&build(&body, "", &[("comments.xml", "comments", "comments", &comments)]), fonts()).expect("opens");
    let started = std::time::Instant::now();
    assert_eq!(replace_all(&mut e, spec("^a"), "!"), N);
    assert!(started.elapsed() < std::time::Duration::from_secs(30), "one pass, one transaction");
    assert!(e.document().comments.by_id.is_empty());
    assert_eq!(texts(&e)[N - 1], "x!");
    assert!(matches!(e.execute(Command::Undo, fonts()), Outcome::Changed(_)));
    assert_eq!(e.document().comments.by_id.len(), N, "one undo step");
}

#[test]
fn the_found_text_cant_replace_a_comment_mark() {
    let comments = format!(r#"<w:comments {NS}><w:comment w:id="1" w:author="Ann"><w:p>{}</w:p></w:comment></w:comments>"#, t("c"));
    let body =
        format!(r#"<w:p><w:commentRangeStart w:id="1"/>{}<w:commentRangeEnd w:id="1"/><w:r><w:commentReference w:id="1"/></w:r></w:p>"#, t("text"));
    let mut e = Editor::open(&build(&body, "", &[("comments.xml", "comments", "comments", &comments)]), fonts()).expect("opens");
    let s = search(spec("^a"));
    let hits = s.all(&e);
    for with in ["^&", "x^c"] {
        assert!(replace_with(&mut e, &s, &hits, with, None, true, fonts()).is_err(), "{with}");
    }
    assert_eq!(e.document().comments.by_id.len(), 1);
}

#[test]
fn formatted_replacements_respect_protection_and_locks() {
    let italic =
        FindFormat { font: crate::api::services::FontDialog { style: Tri::Uniform(FontStyle::Italic), ..Default::default() }, ..Default::default() };
    let is_italic = || FindSpec { format: Some(italic.clone()), ..spec("") };
    let locked = format!(
        r#"<w:p><w:sdt><w:sdtPr><w:id w:val="5"/><w:lock w:val="sdtContentLocked"/></w:sdtPr><w:sdtContent>{}</w:sdtContent></w:sdt></w:p>"#,
        t("apple")
    );
    let read_only = crate::api::command::DocumentProtection {
        kind: Some(crate::api::command::ProtectionKind::ReadOnly),
        enforce: Some(true),
        ..Default::default()
    };
    for protect in [false, true] {
        for (with, format) in [("", Some(&italic)), ("[^&]", None), ("[^&]", Some(&italic))] {
            let mut e = ed(&if protect { p("apple") } else { locked.clone() });
            if protect {
                e.execute(Command::SetDocumentProtection(read_only.clone()), fonts());
            }
            let rev = e.revision();
            let s = search(spec("apple"));
            let hits = s.all(&e);
            assert_eq!(hits.len(), 1);
            let (n, out) = replace_with(&mut e, &s, &hits, with, format, true, fonts()).expect("runs");
            assert_eq!(n, 0, "protected {protect}, {with:?}: {out:?}");
            assert!(matches!(out, Outcome::Refused(_) | Outcome::Unchanged(_)), "{out:?}");
            assert_eq!((texts(&e)[0].as_str(), e.revision()), ("apple", rev), "protected {protect}, {with:?}");
            assert!(found(&e, is_italic()).is_empty());
        }
    }
}

#[test]
fn comment_marks_respect_protection_and_locks() {
    let comments = format!(
        r#"<w:comments {NS}><w:comment w:id="1" w:author="Ann"><w:p>{}</w:p></w:comment><w:comment w:id="2" w:author="Ann"><w:p>{}</w:p></w:comment></w:comments>"#,
        t("c"),
        t("d")
    );
    let mark = |id: u32| {
        format!(r#"<w:commentRangeStart w:id="{id}"/>{}<w:commentRangeEnd w:id="{id}"/><w:r><w:commentReference w:id="{id}"/></w:r>"#, t("text"))
    };
    let open = format!("<w:p>{}</w:p>", mark(1));
    let locked = format!(
        r#"<w:p><w:sdt><w:sdtPr><w:id w:val="5"/><w:lock w:val="sdtContentLocked"/></w:sdtPr><w:sdtContent>{}{}</w:sdtContent></w:sdt></w:p>"#,
        mark(2),
        t(".")
    );
    let protect = |e: &mut Editor, kind| {
        let p = crate::api::command::DocumentProtection { kind: Some(kind), enforce: Some(true), ..Default::default() };
        assert!(matches!(e.execute(Command::SetDocumentProtection(p), fonts()), Outcome::Changed(_)));
    };
    let doc = |body: &str| Editor::open(&build(body, "", &[("comments.xml", "comments", "comments", &comments)]), fonts()).expect("opens");
    use crate::api::command::ProtectionKind as K;
    // Read-only: neither deleted nor typed at.
    for with in ["", "x"] {
        let mut e = doc(&open);
        protect(&mut e, K::ReadOnly);
        let (rev, held) = (e.revision(), e.document().comments.by_id.len());
        let s = search(spec("^a"));
        let hits = s.all(&e);
        let (n, out) = replace_with(&mut e, &s, &hits, with, None, true, fonts()).expect("runs");
        assert_eq!(n, 0, "{with:?}");
        assert!(matches!(out, Outcome::Refused(_)), "{with:?}: {out:?}");
        assert_eq!((e.document().comments.by_id.len(), texts(&e)[0].as_str(), e.revision()), (held, "text", rev), "{with:?}");
    }
    // Comments only: a comment may be deleted, not typed at.
    let mut e = doc(&open);
    protect(&mut e, K::Comments);
    let s = search(spec("^a"));
    let hits = s.all(&e);
    let (n, out) = replace_with(&mut e, &s, &hits, "x", None, true, fonts()).expect("runs");
    assert!(n == 0 && matches!(out, Outcome::Refused(_)), "{out:?}");
    assert!(e.document().comments.by_id.keys().any(|c| c.0 == 1));
    assert_eq!(replace_with(&mut e, &s, &hits, "", None, true, fonts()).expect("runs").0, 1);
    assert!(!e.document().comments.by_id.keys().any(|c| c.0 == 1));
    // A reference in a locked control isn't typed at; the open one is.
    let mut e = doc(&format!("{open}{locked}"));
    let s = search(spec("^a"));
    let hits = s.all(&e);
    assert_eq!(hits.len(), 2);
    assert_eq!(replace_with(&mut e, &s, &hits, "x", None, true, fonts()).expect("runs").0, 1);
    assert_eq!(texts(&e), ["textx", "text."]);
    assert_eq!(e.document().comments.by_id.keys().map(|c| c.0).collect::<Vec<_>>(), [2], "the locked one's comment kept");
}

#[test]
fn formatted_replacements_are_tracked_and_undo_in_one_step() {
    let italic =
        FindFormat { font: crate::api::services::FontDialog { style: Tri::Uniform(FontStyle::Italic), ..Default::default() }, ..Default::default() };
    for (with, format) in [("", Some(&italic)), ("[^&]", None), ("[^&]", Some(&italic))] {
        let mut e = ed(&format!("{}{}", p("one apple"), p("two apple")));
        e.set_author("Me", None);
        e.execute(Command::SetTrackChanges(true), fonts());
        let s = search(spec("apple"));
        let hits = s.all(&e);
        let (n, _) = replace_with(&mut e, &s, &hits, with, format, true, fonts()).expect("replaces");
        assert_eq!(n, 2, "{with:?}");
        let me = e.document().people.persons.iter().position(|a| &*a.name == "Me").expect("an author");
        let mine = |m: Option<&std::rc::Rc<crate::docx::review::RevisionMark>>| m.is_some_and(|m| m.author.0 as usize == me);
        let (mut ins, mut del, mut fmt) = (String::new(), String::new(), String::new());
        for b in &e.document().sections[0].blocks {
            let crate::docx::Block::Para(q) = b else { continue };
            for i in &q.inlines {
                if let crate::docx::Inline::Text(s, r) = i {
                    if mine(r.rev.ins.as_ref()) {
                        ins.push_str(s);
                    }
                    if mine(r.rev.del.as_ref()) {
                        del.push_str(s);
                    }
                    if r.rev.fmt.as_ref().is_some_and(|f| mine(Some(&f.mark))) {
                        fmt.push_str(s);
                    }
                }
            }
        }
        if with.is_empty() {
            assert_eq!((ins.as_str(), del.as_str(), fmt.as_str()), ("", "", "appleapple"), "a formatting change each, as mine");
        } else {
            assert_eq!(del, "appleapple", "{with:?}: each found text deleted as mine");
            assert_eq!(ins, "[apple][apple]", "{with:?}: each replacement inserted as mine");
        }
        assert!(matches!(e.execute(Command::Undo, fonts()), Outcome::Changed(_)));
        assert_eq!(texts(&e), ["one apple", "two apple"], "{with:?}: one undo step");
        assert!(found(&e, FindSpec { format: Some(italic.clone()), ..spec("") }).is_empty(), "{with:?}");
        let tracked = e.document().sections[0].blocks.iter().any(|b| match b {
            crate::docx::Block::Para(q) => q.inlines.iter().any(|i| i.run().is_some_and(|r| !r.rev.is_empty())),
            _ => false,
        });
        assert!(!tracked, "{with:?}: no revision left after undo");
    }
}

/// The body's paragraphs' text runs, each with whether it is inserted and deleted as tracked.
fn tracked_runs(e: &Editor) -> Vec<Vec<(String, bool, bool)>> {
    e.document().sections[0]
        .blocks
        .iter()
        .filter_map(|b| match b {
            crate::docx::Block::Para(p) => Some(
                p.inlines
                    .iter()
                    .filter_map(|i| match i {
                        crate::docx::Inline::Text(s, r) => Some((s.clone(), r.rev.ins.is_some(), r.rev.del.is_some())),
                        _ => None,
                    })
                    .collect(),
            ),
            _ => None,
        })
        .collect()
}

#[test]
fn a_tracked_paragraph_mark_replaced_once_stays_replaced() {
    let mut e = ed(&format!("{}{}{}", p("a"), p("b"), p("c")));
    e.set_author("Me", None);
    e.execute(Command::SetTrackChanges(true), fonts());
    assert_eq!(replace_all(&mut e, spec("^p"), " "), 2);
    let runs = tracked_runs(&e);
    assert_eq!(runs[0], [("a".to_string(), false, false), (" ".to_string(), true, false)], "the space goes where the mark was: {runs:?}");
    assert_eq!(runs[1][0].0, "b", "nothing typed before the next paragraph's text: {runs:?}");
    let inserted = |e: &Editor| tracked_runs(e).iter().flatten().filter(|r| r.1).count();
    assert_eq!(inserted(&e), 2);
    assert_eq!(found(&e, spec("^p")).len(), 2, "All Markup still shows the deleted marks");
    assert_eq!(replace_all(&mut e, spec("^p"), " "), 0, "the deleted marks are left");
    assert_eq!(inserted(&e), 2, "no more insertions");
}

#[test]
fn a_paragraph_mark_the_view_hides_isnt_crossed() {
    let mut e = ed(&format!("{}{}{}", p("a"), p("b"), p("c")));
    e.set_author("Me", None);
    e.execute(Command::SetTrackChanges(true), fonts());
    assert_eq!(replace_all(&mut e, spec("a^pb"), "a b"), 1);
    e.set_markup_view(crate::docx::review::MarkupView::NoMarkup);
    assert_eq!(found(&e, spec("^p")).len(), 1, "only b's mark shows: a's, deleted, joins a and b as they read");
    assert_eq!(found(&e, spec("^pc")).len(), 1);
    e.set_markup_view(crate::docx::review::MarkupView::All);
    assert_eq!(found(&e, spec("^p")).len(), 2, "All Markup shows the deleted mark");
}

#[test]
fn paragraph_marks_match_without_overlapping() {
    let blank = "<w:p/>";
    let mut e = ed(&format!("{}{blank}{blank}{blank}{blank}{}", p("x"), p("y")));
    assert_eq!(search(spec("^p^p")).all(&e).len(), 2, "four empty paragraphs");
    let mut e3 = ed(&format!("{}{blank}{blank}{blank}{}", p("x"), p("y")));
    assert_eq!(search(spec("^p^p")).all(&e3).len(), 2, "three empty paragraphs");
    assert_eq!(replace_all(&mut e3, spec("^p^p"), "^p"), 2);
    assert_eq!(texts(&e3), ["x", "", "y"]);
    assert_eq!(replace_all(&mut e, spec("^p^p"), "^p"), 2);
    assert_eq!(texts(&e), ["x", "", "", "y"]);
}

#[test]
fn highlighting_nothing_clears_the_highlights() {
    let mut e = ed(&p("apple pear"));
    assert_eq!(crate::services::find::highlight_all(&mut e, &search(spec("apple"))), 1);
    assert!(e.find_highlights().is_some());
    assert_eq!(crate::services::find::highlight_all(&mut e, &search(spec("kiwi"))), 0);
    assert!(e.find_highlights().is_none());
}

#[test]
fn a_character_code_takes_three_digits_unless_it_starts_with_zero() {
    let e = ed(&p("d1 \u{e9}"));
    assert_eq!(found(&e, spec("^1001")), ["d1"], "^100 then 1");
    assert_eq!(found(&e, spec("^0233")), ["\u{e9}"]);
}

#[test]
fn a_match_starting_inside_the_selection_isnt_lost_to_an_overlapping_one_before_it() {
    let mut e = ed(&p("ababa."));
    assert_eq!(search(spec("aba")).all(&e).iter().map(|h| h.start).collect::<Vec<_>>(), [0], "unlimited, none overlapping");
    let a = para_id(&e, 0);
    e.set_caret_for_benchmark(Selection::range(
        Position::text(StoryId::Body, a, Unit(2)),
        Position::text(StoryId::Body, a, Unit(6)),
        Default::default(),
    ));
    let inside = search(FindSpec { within: Some(e.selected_ranges()), ..spec("aba") });
    assert_eq!(inside.all(&e).iter().map(|h| (h.start, h.end)).collect::<Vec<_>>(), [(2, 5)]);
    assert_eq!(inside.next(&e, false).map(|(h, _)| h.start), Some(2));
}

#[test]
fn find_previous_skips_a_match_across_paragraphs_ending_past_the_caret() {
    let mut e = ed(&format!("{}{}", p("one"), p("two")));
    let up = FindSpec { wrap: false, ..spec("one^ptwo") };
    e.set_caret_for_benchmark(Selection::Caret(Position::text(StoryId::Body, para_id(&e, 1), Unit(0))));
    assert!(search(up.clone()).next(&e, true).is_none(), "it ends after the caret");
    e.set_caret_for_benchmark(Selection::Caret(Position::text(StoryId::Body, para_id(&e, 1), Unit(3))));
    assert!(search(up).next(&e, true).is_some(), "it ends at the caret");
}

#[test]
fn a_comment_reaching_past_the_selection_isnt_found_in_it() {
    let comments = format!(r#"<w:comments {NS}><w:comment w:id="1" w:author="Ann"><w:p>{}</w:p></w:comment></w:comments>"#, t("c"));
    let mut body = format!(r#"<w:p><w:commentRangeStart w:id="1"/>{}</w:p>"#, t("x"));
    (1..257).for_each(|_| body.push_str(&p("x")));
    body.push_str(&format!(r#"<w:p>{}<w:commentRangeEnd w:id="1"/><w:r><w:commentReference w:id="1"/></w:r></w:p>"#, t("x")));
    let mut e = Editor::open(&build(&body, "", &[("comments.xml", "comments", "comments", &comments)]), fonts()).expect("opens");
    assert_eq!(search(spec("^a")).all(&e).len(), 1);
    e.set_caret_for_benchmark(Selection::range(
        Position::text(StoryId::Body, para_id(&e, 0), Unit(0)),
        Position::text(StoryId::Body, para_id(&e, 255), Unit(1)),
        Default::default(),
    ));
    assert!(search(FindSpec { within: Some(e.selected_ranges()), ..spec("^a") }).all(&e).is_empty());
}

#[test]
fn comment_marks_with_formatting() {
    let c = |i: u32, s: &str| {
        format!(r#"<w:commentRangeStart w:id="{i}"/>{s}<w:commentRangeEnd w:id="{i}"/><w:r><w:commentReference w:id="{i}"/></w:r>"#)
    };
    let comments = format!(
        r#"<w:comments {NS}><w:comment w:id="1" w:author="Ann"><w:p>{}</w:p></w:comment><w:comment w:id="2" w:author="Bob"><w:p>{}</w:p></w:comment></w:comments>"#,
        t("c1"),
        t("c2")
    );
    let bold = "<w:r><w:rPr><w:b/></w:rPr><w:t>bold</w:t></w:r>";
    let body = format!("<w:p>{}{}{}</w:p>", c(1, &t("plain")), t(" and "), c(2, bold));
    let mut e = Editor::open(&build(&body, "", &[("comments.xml", "comments", "comments", &comments)]), fonts()).expect("opens");
    let style =
        |s: FontStyle| FindFormat { font: crate::api::services::FontDialog { style: Tri::Uniform(s), ..Default::default() }, ..Default::default() };
    assert_eq!(found(&e, FindSpec { format: Some(style(FontStyle::Bold)), ..spec("^a") }), ["bold"], "the bold comment's alone");
    let s = search(spec("^a"));
    let hits = s.all(&e);
    assert!(replace_with(&mut e, &s, &hits, "x", Some(&style(FontStyle::Italic)), true, fonts()).is_err(), "text and formatting together");
    let (n, _) = replace_with(&mut e, &s, &hits, "", Some(&style(FontStyle::Italic)), true, fonts()).expect("formats");
    assert_eq!(n, 2);
    assert_eq!(e.document().comments.by_id.len(), 2, "formatting alone keeps the comments");
    assert_eq!(found(&e, FindSpec { format: Some(style(FontStyle::Italic)), ..spec("") }), ["plain", "bold"]);
}

#[test]
fn a_regular_expression_match_refused_at_a_field_marker_lets_a_later_overlapping_one_match() {
    let field = r#"<w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> QUOTE "bc" </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>bc</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r>"#;
    let e = ed(&format!("<w:p>{}{field}</w:p>", t("a")));
    assert_eq!(found(&e, FindSpec { regex: true, ..spec("ab|bc") }), ["bc"]);
}

#[test]
fn raw_break_characters_find_the_breaks_replace_puts_in() {
    let e = ed(&format!(r#"<w:p>{}<w:r><w:br w:type="page"/></w:r>{}<w:r><w:br w:type="column"/></w:r>{}</w:p>"#, t("a"), t("b"), t("c")));
    assert_eq!(found(&e, spec("a\u{c}")).len(), 1, "a page break, as ^12");
    assert!(found(&e, spec("b\u{c}")).is_empty(), "not a column break");
    assert_eq!(found(&e, spec("b\u{e}")).len(), 1, "a column break, as ^14");
}

#[test]
fn formatting_holds_over_a_widened_cluster_and_after_plain_white_space() {
    let bold = |s: &str| format!("<w:r><w:rPr><w:b/></w:rPr><w:t xml:space=\"preserve\">{s}</w:t></w:r>");
    let is_bold =
        || FindFormat { font: crate::api::services::FontDialog { style: Tri::Uniform(FontStyle::Bold), ..Default::default() }, ..Default::default() };
    let with = |text: &str| FindSpec { format: Some(is_bold()), ..spec(text) };
    let e = ed(&format!("<w:p>{}{}</w:p>", bold("\u{1F44D}"), t("\u{1F3FD}")));
    assert!(found(&e, with("\u{1F44D}")).is_empty(), "the modifier isn't bold");
    let e = ed(&format!("<w:p>{}</w:p>", bold("\u{1F44D}\u{1F3FD}")));
    assert_eq!(found(&e, with("\u{1F44D}")), ["\u{1F44D}\u{1F3FD}"], "the whole cluster");
    let e = ed(&format!("<w:p>{}{}</w:p>", t(" "), bold(" x")));
    assert_eq!(found(&e, with("^wx")), [" x"]);
}

#[test]
fn highlight_all_shows_a_paragraph_mark_found_alone() {
    let mut e = ed(&format!("{}{}", p("one"), p("two")));
    e.complete(fonts());
    assert_eq!(crate::services::find::highlight_all(&mut e, &search(spec("^p"))), 1);
    let bands = e.find_highlight_bands(0..1);
    assert_eq!(bands.len(), 1, "the mark: {bands:?}");
    let [x0, _, x1, _] = bands[0].1.rect;
    assert!((x1 - x0 - 4.0).abs() < 0.01, "{bands:?}");
    e.set_find_highlights([(para_id(&e, 0), Unit(0), Unit(2)), (para_id(&e, 0), Unit(1), Unit(3))]);
    assert_eq!(e.find_highlight_bands(0..1).len(), 1, "overlapping ranges merged");
}
