//! Go To: each kind of target, by number, on and back, next and previous

use super::empty_stories::{NS, build};
use super::{fonts, para_id};
use crate::editing::Editor;
use crate::editing::commands::Outcome;
use crate::editing::goto::{GoToTarget as T, GoToWhat as W};
use crate::editing::position::StoryId;
use crate::editing::selection::{ObjectRef, Selection};
use crate::layout::Unit;

fn t(s: &str) -> String {
    format!("<w:r><w:t xml:space=\"preserve\">{s}</w:t></w:r>")
}

fn p(s: &str) -> String {
    format!("<w:p>{}</w:p>", t(s))
}

fn open(body: &str, parts: &[(&str, &str, &str, &str)]) -> Editor {
    let mut e = Editor::open(&build(body, "", parts), fonts()).expect("opens");
    e.complete(fonts());
    e
}

/// Where the caret (or the selection's start) is: paragraph number and unit.
fn at(e: &Editor) -> (usize, u32) {
    let (a, _) = e.selection().ordered(e.index()).expect("a selection");
    let (para, u) = a.para_offset().expect("in text");
    let n = (0..).find(|&k| para_id(e, k) == para).expect("a body paragraph");
    (n, u.0)
}

fn go(e: &mut Editor, what: W, target: T, name: Option<&str>) -> Outcome {
    e.go_to(what, target, name, fonts())
}

#[test]
fn pages_by_number_on_and_back_and_past_the_end() {
    let brk = "<w:r><w:br w:type=\"page\"/></w:r>";
    let mut e = open(&format!("<w:p>{}{brk}</w:p><w:p>{}{brk}</w:p>{}", t("one"), t("two"), p("three")), &[]);
    assert_eq!(e.stack().pages.len(), 3);
    assert!(matches!(go(&mut e, W::Page, T::Absolute(2), None), Outcome::Moved));
    assert_eq!(e.caret().and_then(|c| e.page_showing(c)), Some(1));
    assert!(matches!(go(&mut e, W::Page, T::Absolute(99), None), Outcome::Moved), "past the end: the last page");
    assert_eq!(e.caret().and_then(|c| e.page_showing(c)), Some(2));
    assert!(matches!(go(&mut e, W::Page, T::Next, None), Outcome::Unchanged(_)));
    go(&mut e, W::Page, T::Relative(-2), None);
    assert_eq!(e.caret().and_then(|c| e.page_showing(c)), Some(0));
    go(&mut e, W::Page, T::Relative(1), None);
    assert_eq!(e.caret().and_then(|c| e.page_showing(c)), Some(1));
    assert!(matches!(go(&mut e, W::Page, T::Absolute(0), None), Outcome::Unchanged(_) | Outcome::Refused(_)));
}

#[test]
fn sections_lines_and_headings() {
    let sect = |s: &str| format!("<w:p><w:pPr><w:sectPr><w:type w:val=\"continuous\"/></w:sectPr></w:pPr>{}</w:p>", t(s));
    let heading = |s: &str| format!("<w:p><w:pPr><w:outlineLvl w:val=\"0\"/></w:pPr>{}</w:p>", t(s));
    let body = format!("{}{}{}{}{}{}", heading("H1"), sect("a"), p("b"), sect("c"), heading("H2"), p("d"));
    let mut e = open(&body, &[]);
    go(&mut e, W::Section, T::Absolute(2), None);
    assert_eq!(at(&e), (2, 0));
    go(&mut e, W::Section, T::Next, None);
    assert_eq!(at(&e), (4, 0));
    assert!(matches!(go(&mut e, W::Section, T::Next, None), Outcome::Unchanged(_)));
    go(&mut e, W::Section, T::Absolute(9), None);
    assert_eq!(at(&e), (4, 0), "past the last: the last");
    go(&mut e, W::Line, T::Absolute(4), None);
    assert_eq!(at(&e), (3, 0), "each paragraph one line");
    go(&mut e, W::Line, T::Relative(2), None);
    assert_eq!(at(&e), (5, 0));
    go(&mut e, W::Line, T::Previous, None);
    assert_eq!(at(&e), (4, 0));
    go(&mut e, W::Line, T::Absolute(999), None);
    assert_eq!(at(&e), (5, 0), "past the last: the last");
    go(&mut e, W::Heading, T::Previous, None);
    assert_eq!(at(&e), (4, 0));
    go(&mut e, W::Heading, T::Previous, None);
    assert_eq!(at(&e), (0, 0));
    assert!(matches!(go(&mut e, W::Heading, T::Previous, None), Outcome::Unchanged(_)));
}

#[test]
fn notes_fields_tables_and_bookmarks() {
    let foot = format!(
        r#"<w:footnotes {NS}><w:footnote w:id="1"><w:p>{}</w:p></w:footnote><w:footnote w:id="2"><w:p>{}</w:p></w:footnote></w:footnotes>"#,
        t("f1"),
        t("f2")
    );
    let field = |code: &str, shown: &str| {
        format!(
            r#"<w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> {code} </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r>{}<w:r><w:fldChar w:fldCharType="end"/></w:r>"#,
            t(shown)
        )
    };
    let table = format!(r#"<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w="2000"/></w:tblGrid><w:tr><w:tc>{}</w:tc></w:tr></w:tbl>"#, p("cell"));
    let body = format!(
        r#"<w:p>{}<w:r><w:footnoteReference w:id="1"/></w:r>{}</w:p><w:p>{}{}<w:bookmarkStart w:id="0" w:name="Mark"/>{}<w:bookmarkEnd w:id="0"/></w:p>{table}<w:p>{}<w:r><w:footnoteReference w:id="2"/></w:r>{}</w:p>"#,
        t("ab"),
        field("PAGE", "1"),
        t("x"),
        field("QUOTE \"q\"", "q"),
        t("marked"),
        t("y"),
        field("PAGE", "1")
    );
    let mut e = open(&body, &[("footnotes.xml", "footnotes", "footnotes", &foot)]);
    go(&mut e, W::Footnote, T::Absolute(2), None);
    assert_eq!(at(&e), (3, 1), "before the second reference");
    go(&mut e, W::Footnote, T::Previous, None);
    assert_eq!(at(&e), (0, 2));
    go(&mut e, W::Field, T::Next, Some("page"));
    assert_eq!(at(&e), (0, 3), "the PAGE field after the reference");
    assert!(!e.selection().is_collapsed(), "selected");
    go(&mut e, W::Field, T::Next, Some("PAGE"));
    assert_eq!(at(&e).0, 3, "the next PAGE field, past the QUOTE");
    go(&mut e, W::Field, T::Absolute(2), None);
    assert_eq!(at(&e).0, 1, "any kind, by number");
    go(&mut e, W::Table, T::Absolute(1), None);
    assert_eq!(at(&e), (2, 0), "the first cell");
    go(&mut e, W::Bookmark, T::Absolute(1), Some("mark"));
    assert_eq!(e.selected_text(), "marked");
    e.set_caret_for_benchmark(Selection::Caret(crate::editing::position::Position::text(StoryId::Body, para_id(&e, 0), Unit(0))));
    go(&mut e, W::Bookmark, T::Next, None);
    assert_eq!(e.selected_text(), "marked");
}

#[test]
fn comments_by_reviewer_graphics_objects_and_equations() {
    let c = |i: u32, s: &str| {
        format!(r#"<w:commentRangeStart w:id="{i}"/>{}<w:commentRangeEnd w:id="{i}"/><w:r><w:commentReference w:id="{i}"/></w:r>"#, t(s))
    };
    let comments = format!(
        r#"<w:comments {NS}><w:comment w:id="1" w:author="Ann"><w:p>{}</w:p></w:comment><w:comment w:id="2" w:author="Bob"><w:p>{}</w:p></w:comment></w:comments>"#,
        t("c1"),
        t("c2")
    );
    let w = 12700 * 40;
    let shape = r#"<wps:wsp><wps:cNvSpPr/><wps:spPr><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></wps:spPr><wps:bodyPr/></wps:wsp>"#;
    let inline = format!(
        r#"<w:r><w:drawing><wp:inline distT="0" distB="0" distL="0" distR="0"><wp:extent cx="{w}" cy="{w}"/><wp:effectExtent l="0" t="0" r="0" b="0"/><wp:docPr id="3" name="Shape 3"/><a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape">{shape}</a:graphicData></a:graphic></wp:inline></w:drawing></w:r>"#
    );
    let math = r#"<m:oMath xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math"><m:r><m:t>x</m:t></m:r></m:oMath>"#;
    let body = format!("<w:p>{}{}</w:p><w:p>{}{inline}</w:p><w:p>{}{math}</w:p>", c(1, "ann's"), c(2, "bob's"), t("pic "), t("eq "));
    let mut e = open(&body, &[("comments.xml", "comments", "comments", &comments)]);
    go(&mut e, W::Comment, T::Absolute(1), Some("Bob"));
    assert_eq!(e.selected_text(), "bob's", "the reviewer's comment, its text selected");
    go(&mut e, W::Comment, T::Previous, None);
    assert_eq!(e.selected_text(), "ann's");
    assert!(matches!(go(&mut e, W::Comment, T::Absolute(1), Some("Cy")), Outcome::Unchanged(_)));
    go(&mut e, W::Graphic, T::Next, None);
    assert!(matches!(e.selection(), Selection::Object(ObjectRef::Inline { offset: Unit(4), .. })), "{:?}", e.selection());
    go(&mut e, W::Equation, T::Absolute(1), None);
    assert_eq!(at(&e), (2, 3));
    assert!(matches!(go(&mut e, W::Object, T::Absolute(1), None), Outcome::Unchanged(_)), "no embedded objects");
}

#[test]
fn embedded_objects_by_program_id() {
    let object = |prog: &str| {
        format!(
            r#"<w:r><w:object w:dxaOrig="1440" w:dyaOrig="1440"><v:shape xmlns:v="urn:schemas-microsoft-com:vml" id="s{prog}" style="width:72pt;height:72pt"/><o:OLEObject xmlns:o="urn:schemas-microsoft-com:office:office" Type="Embed" ProgID="{prog}" ShapeID="s{prog}" DrawAspect="Content" ObjectID="_1"/></w:object></w:r>"#
        )
    };
    let body = format!("<w:p>{}{}</w:p><w:p>{}{}</w:p>", t("a"), object("Excel.Sheet.12"), t("b"), object("PowerPoint.Show.12"));
    let mut e = open(&body, &[]);
    assert!(matches!(go(&mut e, W::Object, T::Absolute(1), Some("PowerPoint.Show")), Outcome::Moved));
    assert!(matches!(e.selection(), Selection::Object(ObjectRef::Inline { para, offset: Unit(1) }) if para == para_id(&e, 1)), "{:?}", e.selection());
    go(&mut e, W::Object, T::Previous, None);
    assert!(matches!(e.selection(), Selection::Object(ObjectRef::Inline { para, .. }) if para == para_id(&e, 0)));
    assert!(matches!(go(&mut e, W::Graphic, T::Absolute(1), None), Outcome::Unchanged(_)), "an object isn't a graphic");
    let body = format!("<w:p>{}{}</w:p>", t("a"), object("\u{c4}xcel.Sheet"));
    let mut e = open(&body, &[]);
    assert!(matches!(go(&mut e, W::Object, T::Absolute(1), Some("A")), Outcome::Unchanged(_)), "a name ending inside a character");
    assert!(matches!(go(&mut e, W::Object, T::Absolute(1), Some("\u{c4}")), Outcome::Moved));
}

#[test]
fn a_table_whose_first_cell_starts_with_a_table_and_pages_from_a_range() {
    let tbl = |inner: &str| format!(r#"<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w="2000"/></w:tblGrid><w:tr><w:tc>{inner}</w:tc></w:tr></w:tbl>"#);
    let brk = "<w:r><w:br w:type=\"page\"/></w:r>";
    let nested = tbl(&format!("{}{}", tbl(&p("inner")), p("after")));
    let mut e = open(&format!("<w:p>{}{brk}</w:p>{}{nested}{}{}{}", t("one"), p("two"), p("end"), tbl(&p("last")), p("tail")), &[]);
    assert!(matches!(go(&mut e, W::Table, T::Absolute(1), None), Outcome::Moved));
    let first = at(&e);
    go(&mut e, W::Table, T::Absolute(2), None);
    assert_eq!(at(&e), first, "the outer table and the one in its first cell both start at the inner one's first cell");
    go(&mut e, W::Table, T::Absolute(3), None);
    assert_ne!(at(&e), first, "the third is the last table");
    // A range from page 2 to page 3: the previous page counts from its start.
    let mut e = open(&format!("<w:p>{}{brk}</w:p><w:p>{}{brk}</w:p>{}", t("one"), t("two"), p("three")), &[]);
    let (a, b) = (para_id(&e, 1), para_id(&e, 2));
    e.set_caret_for_benchmark(Selection::range(
        crate::editing::position::Position::text(StoryId::Body, a, Unit(0)),
        crate::editing::position::Position::text(StoryId::Body, b, Unit(1)),
        Default::default(),
    ));
    assert_eq!(e.selection().focus().and_then(|f| e.page_showing(f)), Some(2));
    assert!(matches!(go(&mut e, W::Page, T::Previous, None), Outcome::Moved));
    assert_eq!(e.caret().and_then(|c| e.page_showing(c)), Some(0), "back from the selection's start, not its focus");
}

#[test]
fn pages_and_lines_past_the_laid_out_ones() {
    let brk = "<w:r><w:br w:type=\"page\"/></w:r>";
    let body: String = (0..12).map(|k| format!("<w:p>{}{brk}</w:p>", t(&format!("p{k}")))).collect();
    let fresh = || Editor::open(&build(&format!("{body}{}", p("end")), "", &[]), fonts()).expect("opens");
    let mut e = fresh();
    assert!(!e.layout_complete());
    assert!(matches!(go(&mut e, W::Page, T::Absolute(13), None), Outcome::Moved));
    assert_eq!(e.caret().and_then(|c| e.page_showing(c)), Some(12));
    let mut e = fresh();
    assert!(matches!(go(&mut e, W::Line, T::Absolute(13), None), Outcome::Moved));
    assert_eq!(at(&e), (12, 0), "the last page's line");
}

#[test]
fn a_field_across_paragraphs_named_numbered_and_selected() {
    let fld = |kind: &str| format!(r#"<w:r><w:fldChar w:fldCharType="{kind}"/></w:r>"#);
    let body = format!(
        r#"<w:p>{}{}<w:r><w:instrText xml:space="preserve"> QUOTE "x" </w:instrText></w:r>{}{}</w:p><w:p>{}{}{}<w:r><w:instrText xml:space="preserve"> PAGE </w:instrText></w:r>{}{}{}</w:p>"#,
        t("a"),
        fld("begin"),
        fld("separate"),
        t("one"),
        t("two"),
        fld("end"),
        fld("begin"),
        fld("separate"),
        t("1"),
        fld("end")
    );
    let mut e = open(&body, &[]);
    assert!(matches!(go(&mut e, W::Field, T::Absolute(1), Some("quote")), Outcome::Moved));
    let (s, f) = e.selection().ordered(e.index()).expect("a selection");
    assert_eq!(s.para_offset().map(|(p, u)| (p == para_id(&e, 0), u.0)), Some((true, 1)));
    assert_eq!(f.para_offset().map(|(p, _)| p), Some(para_id(&e, 1)), "to its end in the next paragraph");
    go(&mut e, W::Field, T::Next, None);
    assert_eq!(at(&e).0, 1, "the PAGE field after it");
    go(&mut e, W::Field, T::Absolute(1), None);
    assert_eq!(at(&e), (0, 1), "the first of any kind");
}

#[test]
fn floats_at_one_place_go_one_by_one_and_an_object_scrolls_into_view() {
    let float = super::empty_stories::anchored_box("<w:txbxContent><w:p/></w:txbxContent>");
    let mut e = open(&format!("<w:p>{}{float}{float}</w:p>", t("a")), &[]);
    e.take_requests();
    assert!(matches!(go(&mut e, W::Graphic, T::Absolute(1), None), Outcome::Moved));
    let Selection::Object(first) = e.selection() else { panic!("{:?}", e.selection()) };
    assert!(e.take_requests().iter().any(|r| matches!(r, crate::editing::input::Request::ScrollIntoView { .. })), "scrolled to");
    assert!(matches!(go(&mut e, W::Graphic, T::Next, None), Outcome::Moved));
    let Selection::Object(second) = e.selection() else { panic!("{:?}", e.selection()) };
    assert_ne!(first, second);
    assert!(matches!(go(&mut e, W::Graphic, T::Next, None), Outcome::Unchanged(_)));
    go(&mut e, W::Graphic, T::Previous, None);
    assert_eq!(e.selection(), Selection::Object(first));
}

#[test]
fn a_named_bookmark_by_number_zero_and_an_object_in_another_prefix() {
    let body = format!(r#"<w:p><w:bookmarkStart w:id="0" w:name="Mark"/>{}<w:bookmarkEnd w:id="0"/></w:p>"#, t("marked"));
    let mut e = open(&body, &[]);
    assert!(matches!(go(&mut e, W::Bookmark, T::Absolute(0), Some("Mark")), Outcome::Unchanged(_)));
    assert!(e.selection().is_collapsed());
    let object = r#"<w:r><wpml:object xmlns:wpml="http://schemas.openxmlformats.org/wordprocessingml/2006/main" wpml:dxaOrig="1440" wpml:dyaOrig="1440"><v:shape xmlns:v="urn:schemas-microsoft-com:vml" id="s1" style="width:72pt;height:72pt"/><o:OLEObject xmlns:o="urn:schemas-microsoft-com:office:office" Type="Embed" ProgID="Excel.Sheet.12" ShapeID="s1" DrawAspect="Content" ObjectID="_1"/></wpml:object></w:r>"#;
    let mut e = open(&format!("<w:p>{}{object}</w:p>", t("a")), &[]);
    assert!(matches!(go(&mut e, W::Object, T::Absolute(1), Some("Excel.Sheet")), Outcome::Moved), "{:?}", e.document().sections[0].blocks);
    assert!(matches!(go(&mut e, W::Graphic, T::Absolute(1), None), Outcome::Unchanged(_)), "not a graphic");
}

#[test]
fn equations_in_any_prefix() {
    let math = r#"<mm:oMathPara xmlns:mm="http://schemas.openxmlformats.org/officeDocument/2006/math"><mm:oMath><mm:r><mm:t>x</mm:t></mm:r></mm:oMath></mm:oMathPara>"#;
    let mut e = open(&format!("<w:p>{}</w:p><w:p>{}{math}</w:p>", t("a"), t("eq ")), &[]);
    go(&mut e, W::Equation, T::Absolute(1), None);
    assert_eq!(at(&e), (1, 3));
}
