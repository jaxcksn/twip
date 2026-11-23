//! Host services on the document as it reads: find, spelling, word count, headings, dialogs and accessibility

use std::io::{Read, Write};

use super::fonts;
use crate::api::command::Command;
use crate::api::engine::{Engine, SessionOptions};
use crate::api::fonts::FontLibrary;
use crate::api::services::{DialogKind, FindQuery, MenuAction, MenuTarget};
use crate::api::session::Session;
use crate::api::state::Tri;
use crate::api::value::{Alignment, Order};
use crate::editing::Editor;
use crate::layout::{ParaId, Unit};

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

/// The blank package with `body` for the body's content, extra `styles`, and `sect` for its section (a letter page
/// when empty).
fn package(body: &str, styles: &str, sect: &str) -> Vec<u8> {
    let src = crate::docx::blank::blank_docx();
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(src)).unwrap();
    let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for i in 0..z.len() {
        let mut f = z.by_index(i).unwrap();
        let name = f.name().unwrap().to_string();
        let mut data = Vec::new();
        f.read_to_end(&mut data).unwrap();
        let mut s = String::from_utf8(data).unwrap();
        match name.as_str() {
            "word/document.xml" => {
                let sect = if sect.is_empty() { "<w:sectPr><w:pgSz w:w=\"12240\" w:h=\"15840\"/></w:sectPr>" } else { sect };
                s = format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><w:document xmlns:w=\"{W}\"><w:body>{body}{sect}</w:body></w:document>");
            }
            "word/styles.xml" => s = s.replace("</w:styles>", &format!("{styles}</w:styles>")),
            _ => {}
        }
        out.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
        out.write_all(s.as_bytes()).unwrap();
    }
    out.finish().unwrap().into_inner()
}

fn with_session<R>(bytes: &[u8], f: impl FnOnce(&mut Session) -> R) -> R {
    let mut engine = Engine::new(FontLibrary::system_recommended());
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("a session");
    s.open(bytes).expect("opens");
    f(s)
}

fn q(text: &str) -> FindQuery {
    FindQuery { text: text.into(), ..FindQuery::default() }
}

fn p(text: &str) -> String {
    format!("<w:p><w:r><w:t xml:space=\"preserve\">{text}</w:t></w:r></w:p>")
}

/// A one-word Hunspell dictionary.
fn dictionary(language: &str, words: &[&str]) -> crate::api::engine::Dictionary {
    let dic = format!("{}\n{}\n", words.len(), words.join("\n"));
    crate::api::engine::Dictionary {
        language: language.into(),
        aff: crate::api::value::Bytes(b"SET UTF-8\n".to_vec()),
        dic: crate::api::value::Bytes(dic.into_bytes()),
        hyphenation: None,
    }
}

#[test]
fn next_misspelling_in_a_body_without_paragraphs_is_none() {
    let e = Editor::open(&package("", "", ""), fonts()).expect("opens");
    assert!(crate::services::spelling::paras_in_order(&e).len() <= 1, "none but the one closing the body");
    let mut s = crate::services::spelling::Spellers::default();
    s.add_dictionary("en-US", b"SET UTF-8\n", b"1\nword\n").expect("loads");
    assert!(s.next(&e, "en-US").is_none());
}

#[test]
fn a_section_has_at_most_45_columns() {
    let sect = "<w:sectPr><w:pgSz w:w=\"12240\" w:h=\"15840\"/><w:cols w:num=\"5000000\" w:space=\"0\"/></w:sectPr>";
    let bytes = package(&p("x"), "", sect);
    let doc = crate::docx::read(&bytes).expect("reads");
    assert_eq!(doc.sections[0].columns.num, 45);
    with_session(&bytes, |s| assert_eq!(s.state().ruler.horizontal.columns.len(), 45));
}

const FIELD: &str = "<w:p><w:r><w:fldChar w:fldCharType=\"begin\"/></w:r><w:r><w:instrText xml:space=\"preserve\"> HYPERLINK \"https://example.com/pricing\" </w:instrText></w:r><w:r><w:fldChar w:fldCharType=\"separate\"/></w:r><w:r><w:t>link</w:t></w:r><w:r><w:fldChar w:fldCharType=\"end\"/></w:r></w:p>";

#[test]
fn field_codes_are_not_text_to_find_count_or_copy() {
    with_session(&package(FIELD, "", ""), |s| {
        assert_eq!(s.find_all(&q("pricing")).len(), 0, "the code isn't found");
        assert_eq!(s.find_all(&q("link")).len(), 1, "the result is");
        assert_eq!(s.replace_all(&q("HYPERLINK"), "x").unwrap_or(0), 0, "nor replaced");
        assert_eq!(s.word_count().words, 1, "only the result counts");
        s.run(Command::SelectAll).expect("selects");
        assert_eq!(s.state().selection.text.as_deref(), Some("link"), "the selection's text is the result");
        assert_eq!(s.copy(false).map(|c| c.text).as_deref(), Some("link"), "so is the copy's");
    });
}

/// `before` as text, then [`FIELD`]'s link, in one paragraph.
fn text_then_link(before: &str) -> String {
    format!("<w:p><w:r><w:t xml:space=\"preserve\">{before}</w:t></w:r>{}", &FIELD["<w:p>".len()..])
}

#[test]
fn a_match_never_takes_part_of_a_field() {
    with_session(&package(&text_then_link("Total "), "", ""), |s| {
        for whole in [false, true] {
            let query = FindQuery { whole_words: Some(whole), ..q("l li") };
            assert_eq!(s.find_all(&query).len(), 0, "no match across the field's begin");
            assert_eq!(s.replace_all(&query, "X").expect("replaces"), 0, "nor a replacement");
        }
        assert_eq!(s.replace_all(&q("in"), "X").expect("replaces"), 1, "a match in the result is replaced");
        let fields = s.with_editor(|e| {
            crate::docx::fields::spans(
                &e.document().sections[0]
                    .blocks
                    .iter()
                    .find_map(|b| match b {
                        crate::docx::Block::Para(p) => Some(p.inlines.clone()),
                        crate::docx::Block::Table(_) => None,
                    })
                    .expect("a paragraph"),
            )
            .len()
        });
        assert_eq!(fields, 1, "the field is whole");
        assert_eq!(s.find_all(&q("lXk")).len(), 1);
    });
}

#[test]
fn a_word_is_checked_apart_from_a_field_beside_it() {
    with_session(&package(&text_then_link("Tota"), "", ""), |s| {
        s.spelling().add_dictionary(dictionary("en-US", &["Tota", "link"])).expect("loads");
        assert!(s.spelling().next_misspelling().is_none(), "Tota and the link's result are words apart");
    });
}

#[test]
fn a_heading_reads_its_fields_results() {
    let styles =
        "<w:style w:type=\"paragraph\" w:styleId=\"Heading1\"><w:name w:val=\"heading 1\"/><w:pPr><w:outlineLvl w:val=\"0\"/></w:pPr></w:style>";
    let body = "<w:p><w:pPr><w:pStyle w:val=\"Heading1\"/></w:pPr><w:r><w:t xml:space=\"preserve\">Figure </w:t></w:r><w:r><w:fldChar w:fldCharType=\"begin\"/></w:r><w:r><w:instrText xml:space=\"preserve\"> SEQ Figure </w:instrText></w:r><w:r><w:fldChar w:fldCharType=\"separate\"/></w:r><w:r><w:t>1</w:t></w:r><w:r><w:fldChar w:fldCharType=\"end\"/></w:r></w:p>";
    with_session(&package(body, styles, ""), |s| {
        let h = s.headings();
        assert_eq!(h.len(), 1);
        assert!(!h[0].text.contains("SEQ"), "{:?}", h[0].text);
    });
}

const CAT_TO_CUT: &str = "<w:p><w:r><w:t>c</w:t></w:r><w:del w:id=\"1\" w:author=\"A\" w:date=\"2024-01-01T00:00:00Z\"><w:r><w:delText>a</w:delText></w:r></w:del><w:ins w:id=\"2\" w:author=\"A\" w:date=\"2024-01-01T00:00:00Z\"><w:r><w:t>u</w:t></w:r></w:ins><w:r><w:t>t</w:t></w:r></w:p>";

#[test]
fn tracked_text_the_view_hides_does_not_split_words() {
    let mut e = Editor::open(&package(CAT_TO_CUT, "", ""), fonts()).expect("opens");
    e.set_markup_view(crate::docx::review::MarkupView::NoMarkup);
    let n = crate::services::word_count(&mut e);
    assert_eq!((n.words, n.characters_with_spaces), (1, 3), "the text reads \"cut\"");
    assert!(e.find("cut", false, false).is_some(), "and is found");
    let mut sp = crate::services::spelling::Spellers::default();
    sp.add_dictionary("en-US", b"SET UTF-8\n", b"1\ncut\n").expect("loads");
    let para = crate::services::spelling::paras_in_order(&e)[0];
    let errors = sp.errors(&e, para, "en-US").expect("checked");
    assert!(errors.is_empty(), "\"cut\" is spelled right: {errors:?}");
}

#[test]
fn find_next_takes_the_whole_grapheme_cluster_it_lands_in() {
    with_session(&package(&p("a\u{1F44D}\u{1F3FD}b \u{1F44D}\u{1F3FD}"), "", ""), |s| {
        let query = q("\u{1F44D}");
        assert!(s.find(&query, Order::Next).found);
        let first = s.state().selection.text.clone();
        assert_eq!(first.as_deref(), Some("\u{1F44D}\u{1F3FD}"), "the cluster is selected");
        let at = s.with_editor(|e| e.selection());
        assert!(s.find(&query, Order::Next).found);
        assert_ne!(s.with_editor(|e| e.selection()), at, "Find Next moves on");
        s.replace(&query, "X").expect("replaces");
        assert_eq!(s.find_all(&q("X")).len(), 1, "Replace replaced the selected match");
    });
}

#[test]
fn the_selection_names_the_revision_it_is_in() {
    let body = "<w:p><w:ins w:id=\"2\" w:author=\"A\" w:date=\"2024-01-01T00:00:00Z\"><w:r><w:t>hello</w:t></w:r></w:ins><w:r><w:t xml:space=\"preserve\"> plain</w:t></w:r></w:p>";
    with_session(&package(body, "", ""), |s| {
        assert!(s.find(&q("ell"), Order::Next).found);
        let id = s.state().selection.revision.clone().expect("the insertion");
        assert_eq!(s.revisions().first().map(|r| r.id.clone()), Some(id.clone()), "as the list names it");
        let menu = s.context_menu();
        assert!(matches!(&menu.target, MenuTarget::Revision { id: m } if *m == id));
        assert!(format!("{:?}", menu.items).contains("Accept Change"));
        assert!(s.find(&q("plain"), Order::Next).found);
        assert_eq!(s.state().selection.revision, None, "outside it");
    });
}

#[test]
fn dialog_availability_agrees_with_dialog_open() {
    with_session(&package(&p("plain text"), "", ""), |s| {
        let dialogs = s.state().dialogs.clone();
        let bad: Vec<_> = dialogs.into_iter().filter(|(k, on)| *on != s.dialog_open(*k).is_ok()).map(|(k, _)| k).collect();
        assert!(bad.is_empty(), "availability disagrees with opening: {bad:?}");
        assert!(!s.state().dialogs[&DialogKind::Frame], "no frame here");
    });
}

#[test]
fn words_in_caseless_scripts_are_checked() {
    let sp = crate::services::spelling::Speller::new("he", b"SET UTF-8\n", "1\n\u{5E9}\u{5DC}\u{5D5}\u{5DD}\n".as_bytes()).expect("loads");
    let wrong: Vec<char> = "\u{5E9}\u{5DC}\u{5D5}\u{5DE}\u{5DE}".chars().collect();
    assert_eq!(sp.errors(&wrong).len(), 1);
    let right: Vec<char> = "\u{5E9}\u{5DC}\u{5D5}\u{5DD}".chars().collect();
    assert!(sp.errors(&right).is_empty());
    let caps: Vec<char> = "NASA".chars().collect();
    assert!(sp.errors(&caps).is_empty(), "words in capitals are still skipped");
}

#[test]
fn the_selection_formatting_reads_every_paragraph() {
    let body: String = (0..3000)
        .map(|i| if i == 2500 { "<w:p><w:pPr><w:jc w:val=\"center\"/></w:pPr><w:r><w:t>c</w:t></w:r></w:p>".to_string() } else { p("l") })
        .collect();
    with_session(&package(&body, "", ""), |s| {
        s.run(Command::SelectAll).expect("selects");
        assert_eq!(s.state().paragraph.alignment, Tri::<Alignment>::Mixed);
    });
}

#[test]
fn the_caret_window_ends_after_runs_of_empty_paragraphs() {
    let body = format!("{}{}", p("x"), "<w:p/>".repeat(50_000));
    let mut e = Editor::open(&package(&body, "", ""), fonts()).expect("opens");
    let w = crate::services::a11y::window(&mut e, &crate::services::spelling::Spellers::default(), "en-US", false).expect("a window");
    assert!(w.window.paragraphs.len() <= 2002, "{} paragraphs", w.window.paragraphs.len());
}

#[test]
fn headings_fall_back_to_the_builtin_heading_style() {
    let styles = "<w:style w:type=\"paragraph\" w:styleId=\"Kop1\"><w:name w:val=\"heading 1\"/></w:style>";
    let body = "<w:p><w:pPr><w:pStyle w:val=\"Kop1\"/></w:pPr><w:r><w:t>Title</w:t></w:r></w:p>";
    with_session(&package(body, styles, ""), |s| {
        let h = s.headings();
        assert_eq!(h.iter().map(|h| (h.level, h.text.as_str())).collect::<Vec<_>>(), [(1, "Title")]);
    });
}

#[test]
fn check_accessibility_skips_decorative_pictures_and_lists_floating_ones() {
    let e = super::open("FX-A11Y-STRUCTURE.docx");
    let issues = crate::services::dialogs::issues(e.document());
    assert!(!issues.iter().any(|i| i.id.starts_with("picture:")), "the decorative picture isn't an issue: {issues:?}");
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/FX-OBJ.docx");
    with_session(&std::fs::read(path).expect("the fixture"), |s| {
        let issues = s.with_editor(|e| crate::services::dialogs::issues(e.document()));
        let float = issues.iter().find(|i| i.id.starts_with("float:")).expect("a floating picture without alt text");
        s.select_accessibility_issue(&float.id).expect("selects it");
        assert!(s.state().selection.object.is_some(), "the picture is selected");
    });
}

#[test]
fn set_defaults_writes_only_what_changed() {
    with_session(&package(&p("x"), "", ""), |s| {
        let raw = |s: &mut Session| s.with_editor(|e| e.document().resolver.as_ref().and_then(|r| r.styles.defaults_raw.clone()).unwrap_or_default());
        let before = raw(s);
        assert!(!before.contains("w:color") && !before.contains("w:ind"), "no colour or indents set: {before}");
        let m = s.dialog_open(DialogKind::ManageStyles).expect("opens");
        let crate::api::services::DialogValues::ManageStyles(mut v) = m.values.clone() else { panic!("{m:?}") };
        v.defaults.size = Some(crate::api::value::Points(15.0));
        s.dialog_apply(crate::api::services::DialogModel { values: crate::api::services::DialogValues::ManageStyles(v), ..m }).expect("applies");
        let after = raw(s);
        assert!(after.contains("w:val=\"30\""), "the size changed: {after}");
        assert!(!after.contains("w:color") && !after.contains("w:ind"), "what wasn't changed stays unset: {after}");
    });
}

#[test]
fn delete_repeated_word_keeps_a_tab_before_it() {
    let body = "<w:p><w:r><w:t>the</w:t></w:r><w:r><w:tab/></w:r><w:r><w:t xml:space=\"preserve\">the the</w:t></w:r></w:p>";
    with_session(&package(body, "", ""), |s| {
        s.spelling().add_dictionary(dictionary("en-US", &["the"])).expect("loads");
        let found = s.spelling().next_misspelling().expect("the doubled word");
        assert_eq!(found.word, "the");
        s.invoke(MenuAction::ReplaceWord { with: String::new() }).expect("deletes it");
        let text: String = s.with_editor(|e| e.read_text(para0(e), Unit(0), Unit(100)));
        assert_eq!(text, "the\tthe", "the tab stays, the second space goes");
    });
}

fn para0(e: &Editor) -> ParaId {
    match &e.document().sections[0].blocks[0] {
        crate::docx::Block::Para(p) => ParaId(p.id),
        crate::docx::Block::Table(_) => panic!("a table"),
    }
}

#[test]
fn the_reading_tree_follows_do_not_check_spelling() {
    with_session(&package(&p("wrongg"), "", ""), |s| {
        s.spelling().add_dictionary(dictionary("en-US", &["word"])).expect("loads");
        let marked = |s: &mut Session| format!("{:?}", s.accessible_tree()).contains("Misspelling");
        assert!(marked(s), "marked misspelled");
        s.run(Command::SelectAll).expect("selects");
        s.run(Command::SetLanguage(crate::api::command::Language { language: None, no_proof: Some(true) })).expect("sets");
        assert!(!marked(s), "no longer checked");
    });
}

#[test]
fn a_dictionary_that_fails_to_load_adds_no_hyphenation() {
    let mut services = crate::services::Services::default();
    let d = crate::api::engine::Dictionary {
        language: "en-US".into(),
        aff: crate::api::value::Bytes(b"FLAG num\n".to_vec()),
        dic: crate::api::value::Bytes(b"1\nword/notanumber\n".to_vec()),
        hyphenation: Some(crate::api::value::Bytes(b"1a1\n".to_vec())),
    };
    assert!(services.add_dictionary(&d).is_err(), "the spelling doesn't load");
    assert!(services.hyphenation.is_none(), "nor do its patterns");
}

#[test]
fn each_run_is_spelled_in_its_language() {
    let body = "<w:p><w:r><w:rPr><w:lang w:val=\"en-US\"/></w:rPr><w:t xml:space=\"preserve\">hello </w:t></w:r><w:r><w:rPr><w:lang w:val=\"fr-FR\"/></w:rPr><w:t>bonjour</w:t></w:r></w:p>";
    with_session(&package(body, "", ""), |s| {
        s.spelling().add_dictionary(dictionary("en-US", &["hello"])).expect("loads");
        s.spelling().add_dictionary(dictionary("fr-FR", &["bonjour"])).expect("loads");
        assert_eq!(s.state().spelling.misspellings, 0, "each word is right in its own language");
        s.spelling().add_dictionary(dictionary("fr-FR", &["salut"])).expect("replaces");
        let found = s.spelling().next_misspelling().expect("the French word");
        assert_eq!(found.word, "bonjour");
    });
}

#[test]
fn a_language_tag_in_another_case_is_the_same_language() {
    let body = "<w:p><w:r><w:rPr><w:lang w:val=\"en-US\"/></w:rPr><w:t>hel</w:t></w:r><w:r><w:rPr><w:lang w:val=\"en-us\"/></w:rPr><w:t>lo</w:t></w:r></w:p>";
    with_session(&package(body, "", ""), |s| {
        s.spelling().add_dictionary(dictionary("en-US", &["hello"])).expect("loads");
        assert_eq!(s.state().spelling.misspellings, 0, "one word, checked whole");
    });
}

#[test]
fn table_properties_start_from_the_cells_margins() {
    let body = "<w:tbl><w:tblPr><w:tblW w:w=\"0\" w:type=\"auto\"/><w:tblCellMar><w:left w:w=\"0\" w:type=\"dxa\"/><w:right w:w=\"200\" w:type=\"dxa\"/></w:tblCellMar></w:tblPr><w:tblGrid><w:gridCol w:w=\"4000\"/></w:tblGrid><w:tr><w:tc><w:p><w:r><w:t>cell</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p/>";
    with_session(&package(body, "", ""), |s| {
        assert!(s.find(&q("cell"), Order::Next).found);
        let m = s.dialog_open(DialogKind::TableProperties).expect("opens");
        let crate::api::services::DialogValues::TableProperties(v) = &m.values else { panic!("{m:?}") };
        assert_eq!((v.cell_margins.left.0, v.cell_margins.right.0), (0.0, 10.0), "the table's margins");
    });
}

#[test]
fn picture_size_scales_from_the_pictures_own_size() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/FX-A11Y-STRUCTURE.docx");
    with_session(&std::fs::read(path).expect("the fixture"), |s| {
        let (natural, size) = s.with_editor(|e| {
            let (para, offset) = e
                .document()
                .sections
                .iter()
                .flat_map(|s| &s.blocks)
                .find_map(|b| {
                    let crate::docx::Block::Para(p) = b else { return None };
                    let mut at = 0;
                    p.inlines.iter().find_map(|i| {
                        let here = (matches!(i, crate::docx::Inline::Picture(..))).then_some((ParaId(p.id), Unit(at)));
                        at += i.units() as u32;
                        here
                    })
                })
                .expect("a picture");
            e.set_selection(super::super::selection::Selection::Object(super::super::selection::ObjectRef::Inline { para, offset }));
            (e.original_object_size().expect("its own size"), e.selected_object().map(|o| (o.width, o.height)).expect("selected"))
        });
        assert!((size.0 - natural.0).abs() > 1.0, "drawn at another size than its own: {size:?} {natural:?}");
        let m = s.dialog_open(DialogKind::PictureSize).expect("opens");
        let crate::api::services::DialogValues::PictureSize(v) = m.values.clone() else { panic!("{m:?}") };
        assert!((v.scale_width - size.0 / natural.0 * 100.0).abs() < 0.06, "{} of {natural:?} for {size:?}", v.scale_width);
    });
}

#[test]
fn find_replace_and_spelling_reach_every_story() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/FX-STORIES.docx");
    with_session(&std::fs::read(path).expect("the fixture"), |s| {
        for (text, story) in [("Text box, second", "a text box"), ("Footnote 1:", "a footnote"), ("odd-page", "a header")] {
            assert_eq!(s.find_all(&q(text)).len(), 1, "{text:?} in {story}");
        }
        assert!(s.find(&q("Footnote 1:"), Order::Next).found);
        assert_eq!(s.state().selection.story, crate::api::value::Story::Footnote, "Find Next goes into the note");
        assert_eq!(s.replace_all(&q("Text box, second"), "Box two").expect("replaces"), 1);
        assert_eq!(s.find_all(&q("Box two")).len(), 1, "the text box was replaced");
        s.spelling().add_dictionary(dictionary("en-US", &["a"])).expect("loads");
        let body = s.with_editor(|e| {
            let mut sp = crate::services::spelling::Spellers::default();
            sp.add_dictionary("en-US", b"SET UTF-8\n", b"1\na\n").expect("loads");
            let paras = crate::services::spelling::paras_in_order(e);
            paras.into_iter().filter_map(|p| sp.errors(e, p, "en-US")).map(|x| x.len() as u32).sum::<u32>()
        });
        assert!(s.state().spelling.misspellings > body, "notes, boxes and headers are counted too");
    });
}

#[test]
fn check_accessibility_reaches_an_inline_shapes_text() {
    use crate::docx::{Block, Inline, Paragraph};
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/FX-OBJ.docx");
    let mut doc = crate::docx::read_editable(&std::fs::read(path).expect("the fixture")).expect("reads");
    let floats = |doc: &crate::docx::Document| crate::services::dialogs::issues(doc).into_iter().filter(|i| i.id.starts_with("float:")).count();
    let before = floats(&doc);
    let paras = || doc.sections.iter().flat_map(|s| &s.blocks).filter_map(|b| if let Block::Para(p) = b { Some(p) } else { None });
    let float =
        paras().flat_map(|p| &p.inlines).find(|i| matches!(i, Inline::Float(f, _) if f.image.is_some())).cloned().expect("a floating picture");
    let (mut pic, run) = paras()
        .flat_map(|p| &p.inlines)
        .find_map(|i| if let Inline::Picture(pc, r) = i { Some((pc.clone(), r.clone())) } else { None })
        .expect("an inline picture");
    let first = paras().next().cloned().expect("a paragraph");
    let inner = Paragraph { inlines: vec![float], ..first };
    let text_box = crate::docx::TextBox { blocks: vec![Block::Para(inner)], insets: [0.0; 4], outline: 0.0, auto_fit: false, v_anchor: 0, flow: 0 };
    pic.text = Some(Box::new(crate::docx::InlineText { id: crate::docx::next_para_id(), text_box }));
    let Some(Block::Para(host)) = doc.sections[0].blocks.iter_mut().find(|b| matches!(b, Block::Para(_))) else { panic!("a paragraph") };
    host.inlines.push(Inline::Picture(pic, run));
    assert!(before > 0, "setup: the fixture's floating picture has no alt text");
    assert_eq!(floats(&doc), before + 1, "the copy in the inline shape's text is listed too");
}
