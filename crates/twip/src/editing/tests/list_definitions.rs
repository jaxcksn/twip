//! Lists outside the body, list definitions and their links to styles, and the numbering part they save.

use super::{fonts, open};
use crate::api::command::*;
use crate::api::value::{Alignment, Points, StyleId};
use crate::docx::{Block, Paragraph};
use crate::editing::Editor;
use crate::editing::position::{Position, StoryId};
use crate::editing::selection::Selection;
use std::io::{Read, Write};

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const NUM_TYPE: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml";
const NUM_REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering";

fn lvl(i: usize, fmt: &str, text: &str, extra: &str) -> String {
    format!(
        "<w:lvl w:ilvl=\"{i}\"><w:start w:val=\"1\"/><w:numFmt w:val=\"{fmt}\"/>{extra}<w:lvlText w:val=\"{text}\"/><w:lvlJc w:val=\"left\"/><w:pPr><w:ind w:left=\"{}\" w:hanging=\"360\"/></w:pPr></w:lvl>",
        720 * (i + 1)
    )
}

fn levels(fmt: &str, text: impl Fn(usize) -> String) -> String {
    (0..9).map(|i| lvl(i, fmt, &text(i), "")).collect()
}

fn abstract_of(id: i64, lvls: &str) -> String {
    format!("<w:abstractNum w:abstractNumId=\"{id}\"><w:multiLevelType w:val=\"hybridMultilevel\"/>{lvls}</w:abstractNum>")
}

fn numbering(inner: &str) -> String {
    format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><w:numbering xmlns:w=\"{W}\">{inner}</w:numbering>")
}

fn num(id: i64, abs: i64) -> String {
    format!("<w:num w:numId=\"{id}\"><w:abstractNumId w:val=\"{abs}\"/></w:num>")
}

/// The blank package with body `paras`, an optional numbering part and extra styles.
fn pkg(paras: &str, numbering: Option<&str>, styles: &str) -> Vec<u8> {
    let src = crate::docx::blank::blank_docx();
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(src)).unwrap();
    let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for i in 0..z.len() {
        let mut f = z.by_index(i).unwrap();
        let name = f.name().unwrap().to_string();
        let mut data = String::new();
        f.read_to_string(&mut data).unwrap();
        match name.as_str() {
            "word/document.xml" => data = data.replacen("<w:p/>", paras, 1),
            "word/styles.xml" => data = data.replacen("</w:styles>", &format!("{styles}</w:styles>"), 1),
            "[Content_Types].xml" if numbering.is_some() => {
                data = data.replacen("</Types>", &format!("<Override PartName=\"/word/numbering.xml\" ContentType=\"{NUM_TYPE}\"/></Types>"), 1)
            }
            "word/_rels/document.xml.rels" if numbering.is_some() => {
                data = data.replacen(
                    "</Relationships>",
                    &format!("<Relationship Id=\"rId9\" Type=\"{NUM_REL}\" Target=\"numbering.xml\"/></Relationships>"),
                    1,
                )
            }
            _ => {}
        }
        out.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
        out.write_all(data.as_bytes()).unwrap();
    }
    if let Some(n) = numbering {
        out.start_file("word/numbering.xml", zip::write::SimpleFileOptions::default()).unwrap();
        out.write_all(n.as_bytes()).unwrap();
    }
    out.finish().unwrap().into_inner()
}

fn editor(bytes: &[u8]) -> Editor {
    Editor::open(bytes, fonts()).expect("opens")
}

fn part(bytes: &[u8], name: &str) -> String {
    let mut s = String::new();
    zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap().by_name(name).unwrap().read_to_string(&mut s).unwrap();
    s
}

fn saved(e: &Editor) -> Vec<u8> {
    crate::docx::write::save(e.document()).expect("saves")
}

fn para(e: &Editor, n: usize) -> &Paragraph {
    e.document().sections.iter().flat_map(|s| s.blocks.iter()).filter_map(|b| if let Block::Para(p) = b { Some(p) } else { None }).nth(n).unwrap()
}

fn label(e: &Editor, n: usize) -> String {
    para(e, n).list.as_ref().map(|l| l.text.clone()).unwrap_or_default()
}

fn caret_in(e: &mut Editor, n: usize) {
    let at = Position::text(StoryId::Body, super::para_id(e, n), crate::layout::Unit(0));
    e.set_caret_for_benchmark(Selection::Caret(at));
}

fn p(text: &str, ppr: &str) -> String {
    format!("<w:p><w:pPr>{ppr}</w:pPr><w:r><w:t>{text}</w:t></w:r></w:p>")
}

fn np(text: &str, id: i64, ilvl: i64) -> String {
    p(text, &format!("<w:numPr><w:ilvl w:val=\"{ilvl}\"/><w:numId w:val=\"{id}\"/></w:numPr>"))
}

fn levels_of(e: &mut Editor, n: usize) -> Vec<ListLevel> {
    caret_in(e, n);
    let (_, num, _) = e.selection_list().expect("a list");
    e.list_levels(num).expect("levels")
}

/// FX-CUSTOM-LISTS with `edit` applied to one part's text.
fn patched(part_name: &str, edit: impl Fn(String) -> String) -> Editor {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/FX-CUSTOM-LISTS.docx");
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(std::fs::read(path).unwrap())).unwrap();
    let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for i in 0..z.len() {
        let mut f = z.by_index(i).unwrap();
        let name = f.name().unwrap().to_string();
        let mut data = Vec::new();
        f.read_to_end(&mut data).unwrap();
        if name == part_name {
            data = edit(String::from_utf8(data).unwrap()).into_bytes();
        }
        out.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
        out.write_all(&data).unwrap();
    }
    editor(&out.finish().unwrap().into_inner())
}

#[test]
fn adjusting_a_levels_indents_keeps_the_picture_bullets_before_it() {
    // Level 1 of the picture-bullet list gets a picture too.
    let mut e = patched("word/numbering.xml", |s| {
        let (a, b) = s.split_once("w:abstractNumId=\"1\">").unwrap();
        let b = b.replacen(
            "<w:lvl w:ilvl=\"1\"><w:start w:val=\"1\"/><w:numFmt w:val=\"bullet\"/>",
            "<w:lvl w:ilvl=\"1\"><w:start w:val=\"1\"/><w:numFmt w:val=\"bullet\"/><w:lvlPicBulletId w:val=\"0\"/>",
            1,
        );
        format!("{a}w:abstractNumId=\"1\">{b}")
    });
    caret_in(&mut e, 5);
    e.execute(Command::SetListLevel(LevelChange::Increase), fonts());
    let before = levels_of(&mut e, 5);
    assert!(before[0].picture.is_some() && before[1].picture.is_some(), "setup: both levels have pictures");
    e.execute(
        Command::AdjustListIndents(ListIndents { number_position: Points(0.0), text_indent: Points(27.0), follow: FollowNumber::Space, tab: None }),
        fonts(),
    );
    let after = levels_of(&mut e, 5);
    assert_eq!(after[1].text_indent, Points(27.0), "level 1 took the indent");
    assert!(after[0].picture.is_some(), "level 0 keeps its picture bullet");
    let xml = part(&saved(&e), "word/numbering.xml");
    let abs1 = xml.split("w:abstractNumId=\"1\">").nth(1).unwrap().split("</w:abstractNum>").next().unwrap();
    let lvl0 = abs1.split("<w:lvl w:ilvl=\"1\"").next().unwrap();
    assert!(lvl0.contains("lvlPicBulletId"), "{lvl0}");
}

fn text_box_para(e: &Editor) -> (u64, crate::layout::ParaId) {
    e.document()
        .sections
        .iter()
        .flat_map(|s| s.blocks.iter())
        .find_map(|b| match b {
            Block::Para(p) => p.inlines.iter().find_map(|i| match i {
                crate::docx::Inline::Float(f, _) => f.text_box.as_ref().and_then(|t| {
                    t.blocks.iter().find_map(|b| if let Block::Para(q) = b { Some((f.id, crate::layout::ParaId(q.id))) } else { None })
                }),
                _ => None,
            }),
            _ => None,
        })
        .expect("a text box")
}

fn text_box_list(e: &Editor) -> Option<String> {
    e.document()
        .sections
        .iter()
        .flat_map(|s| s.blocks.iter())
        .find_map(|b| match b {
            Block::Para(p) => p.inlines.iter().find_map(|i| match i {
                crate::docx::Inline::Float(f, _) => f.text_box.as_ref().and_then(|t| {
                    t.blocks.iter().find_map(|b| if let Block::Para(q) = b { Some(q.list.as_ref().map(|l| l.text.clone())) } else { None })
                }),
                _ => None,
            }),
            _ => None,
        })
        .flatten()
}

#[test]
fn a_text_box_paragraph_takes_and_loses_a_bullet() {
    let mut e = open("FX-STORIES.docx");
    let (id, para) = text_box_para(&e);
    e.set_caret_for_benchmark(Selection::Caret(Position::text(StoryId::TextBox(crate::layout::FloatId(id)), para, crate::layout::Unit(0))));
    e.execute(Command::SetList(Some(ListKind::Bullet)), fonts());
    assert!(e.index().para(e.document(), para).unwrap().style.num.is_some_and(|(n, _)| n != 0), "setup: numPr was set");
    assert!(text_box_list(&e).is_some(), "the text box paragraph shows a bullet");
    e.execute(Command::SetList(None), fonts());
    assert!(text_box_list(&e).is_none(), "the bullet goes");
}

#[test]
fn a_footnote_list_is_defined_and_restarted() {
    let mut e = open("FX-STORIES.docx");
    let fid = *e.document().footnotes.keys().min().unwrap();
    let first = |e: &Editor| match &e.document().footnotes[&fid][..] {
        [Block::Para(p), ..] => (crate::layout::ParaId(p.id), p.style.num),
        _ => panic!("a paragraph"),
    };
    let (para, _) = first(&e);
    e.set_caret_for_benchmark(Selection::Caret(Position::text(StoryId::Footnote(fid), para, crate::layout::Unit(0))));
    e.execute(Command::SetList(Some(ListKind::Numbered)), fonts());
    let (_, before) = first(&e);
    assert!(before.is_some_and(|(n, _)| n != 0), "setup: in a list");
    let mut ls = e.list_levels(before.unwrap().0).unwrap();
    (ls[0].number_position, ls[0].text_indent) = (Points(36.0), Points(54.0));
    e.execute(Command::SetListDefinition(ListDefinitionChange { levels: ls, apply_to: ListApplyTo::WholeList, name: None }), fonts());
    let indent = |e: &Editor| match &e.document().footnotes[&fid][..] {
        [Block::Para(p), ..] => p.style.ind_left,
        _ => panic!("a paragraph"),
    };
    assert_eq!(indent(&e), 54.0, "the footnote item takes the new indent");
    e.execute(Command::RestartNumbering, fonts());
    let (_, after) = first(&e);
    assert_ne!(after.map(|x| x.0), before.map(|x| x.0), "restart gives the footnote item a new numId");
}

#[test]
fn a_label_follows_its_paragraphs_new_style() {
    let styles = "<w:style w:type=\"paragraph\" w:styleId=\"Big\"><w:name w:val=\"Big\"/><w:basedOn w:val=\"Normal\"/><w:rPr><w:sz w:val=\"56\"/></w:rPr></w:style>";
    let nums = numbering(&(abstract_of(0, &levels("bullet", |_| "o".into())) + &num(1, 0)));
    let mut e = editor(&pkg(&np("Item", 1, 0), Some(&nums), styles));
    assert!(para(&e, 0).list.is_some(), "setup: a label");
    caret_in(&mut e, 0);
    e.execute(Command::SetParagraphStyle(StyleId("Big".into())), fonts());
    let (mark, run) = (para(&e, 0).mark.size, para(&e, 0).list.as_ref().unwrap().run.size);
    assert_eq!(mark, 28.0, "setup: the mark is big");
    let again = editor(&saved(&e));
    assert_eq!(run, para(&again, 0).list.as_ref().unwrap().run.size, "the label matches a reload");
}

#[test]
fn a_label_follows_its_paragraph_mark() {
    let nums = numbering(&(abstract_of(0, &levels("decimal", |i| format!("%{}.", i + 1))) + &num(1, 0)));
    let empty = "<w:p><w:pPr><w:numPr><w:ilvl w:val=\"0\"/><w:numId w:val=\"1\"/></w:numPr></w:pPr></w:p>";
    let mut e = editor(&pkg(empty, Some(&nums), ""));
    assert!(!para(&e, 0).list.as_ref().unwrap().run.bold, "setup: a plain number");
    caret_in(&mut e, 0);
    e.execute(Command::SetBold(true), fonts());
    assert!(para(&e, 0).mark.bold, "setup: the mark is bold");
    let again = editor(&saved(&e));
    assert!(para(&again, 0).list.as_ref().unwrap().run.bold, "setup: a reload numbers it in bold");
    assert!(para(&e, 0).list.as_ref().unwrap().run.bold, "the number is bold");
}

#[test]
fn an_empty_section_break_paragraph_stays_out_of_its_list() {
    let brk = "<w:numPr><w:ilvl w:val=\"0\"/><w:numId w:val=\"1\"/></w:numPr><w:sectPr><w:pgSz w:w=\"12240\" w:h=\"15840\"/></w:sectPr>";
    let body = np("A", 1, 0) + &np("B", 1, 0) + &format!("<w:p><w:pPr>{brk}</w:pPr></w:p>") + &np("C", 1, 0);
    let nums = numbering(&(abstract_of(0, &levels("decimal", |i| format!("%{}.", i + 1))) + &num(1, 0)));
    let mut e = editor(&pkg(&body, Some(&nums), ""));
    let all = |e: &Editor| (0..4).map(|n| label(e, n)).collect::<Vec<_>>();
    assert_eq!(all(&e), ["1.", "2.", "", "3."], "setup: as loaded");
    caret_in(&mut e, 0);
    e.execute(Command::SetAlignment(Alignment::Center), fonts());
    assert_eq!(all(&e), ["1.", "2.", "", "3."], "after an edit");
}

#[test]
fn turning_bullets_off_overrides_a_styles_numbering() {
    let styles = "<w:style w:type=\"paragraph\" w:styleId=\"ListBullet\"><w:name w:val=\"List Bullet\"/><w:basedOn w:val=\"Normal\"/><w:pPr><w:numPr><w:numId w:val=\"1\"/></w:numPr></w:pPr></w:style>";
    let nums = numbering(&(abstract_of(0, &levels("bullet", |_| "o".into())) + &num(1, 0)));
    let mut e = editor(&pkg(&p("One", "<w:pStyle w:val=\"ListBullet\"/>"), Some(&nums), styles));
    assert_eq!(para(&e, 0).style.num, Some((1, 0)), "setup: numbered by its style");
    caret_in(&mut e, 0);
    e.execute(Command::SetListLevel(LevelChange::Increase), fonts());
    assert_eq!(para(&e, 0).style.num, Some((1, 1)), "setup: a direct numPr");
    e.execute(Command::SetList(None), fonts());
    assert!(para(&e, 0).style.num.is_none_or(|(n, _)| n == 0), "no longer in a list: {:?}", para(&e, 0).style.num);
}

#[test]
fn a_tracked_earlier_number_is_not_the_paragraphs_own() {
    let cx = crate::docx::write::props::Cx::of_xml(&format!("<w:document xmlns:w=\"{W}\">"));
    let earlier = "<w:pPr><w:pStyle w:val=\"ListBullet\"/><w:pPrChange w:id=\"1\" w:author=\"A\"><w:pPr><w:numPr><w:numId w:val=\"1\"/></w:numPr></w:pPr></w:pPrChange></w:pPr>";
    assert!(!crate::editing::lists::direct_num(Some(earlier), &cx));
    let own = "<w:pPr><w:numPr><w:numId w:val=\"1\"/></w:numPr><w:pPrChange w:id=\"1\" w:author=\"A\"><w:pPr/></w:pPrChange></w:pPr>";
    assert!(crate::editing::lists::direct_num(Some(own), &cx));
}

fn mac_cleanup_numbering() -> String {
    numbering(&(abstract_of(0, &levels("decimal", |i| format!("%{}.", i + 1))) + &num(1, 0) + "<w:numIdMacAtCleanup w:val=\"1\"/>"))
}

#[test]
fn a_new_list_goes_before_num_id_mac_at_cleanup() {
    let mut e = editor(&pkg(&p("A", ""), Some(&mac_cleanup_numbering()), ""));
    caret_in(&mut e, 0);
    e.execute(Command::SetList(Some(ListKind::Bullet)), fonts());
    let xml = part(&saved(&e), "word/numbering.xml");
    let (n, m) = (xml.rfind("<w:num ").expect("a num"), xml.find("<w:numIdMacAtCleanup").unwrap());
    assert!(xml.matches("<w:num ").count() == 2, "setup: a new num: {xml}");
    assert!(n < m, "{xml}");
}

#[test]
fn defining_a_list_edits_its_own_level_overrides() {
    let abs = abstract_of(2, &levels("decimal", |i| format!("%{}.", i + 1)));
    let over = format!(
        "<w:num w:numId=\"5\"><w:abstractNumId w:val=\"2\"/><w:lvlOverride w:ilvl=\"0\">{}</w:lvlOverride></w:num>",
        lvl(0, "upperRoman", "%1)", "")
    );
    let nums = numbering(&(abs + &over + &num(6, 2)));
    let mut e = editor(&pkg(&np("A", 5, 0), Some(&nums), ""));
    let mut ls = e.list_levels(5).unwrap();
    assert_eq!(ls[0].format, NumberFormat::UpperRoman, "setup: the override shows");
    assert_eq!(e.list_levels(6).unwrap()[0].format, NumberFormat::Decimal);
    ls[3].text = "%4]".into();
    caret_in(&mut e, 0);
    e.execute(Command::SetListDefinition(ListDefinitionChange { levels: ls, apply_to: ListApplyTo::WholeList, name: None }), fonts());
    assert_eq!(e.list_levels(6).unwrap()[3].text, "%4]", "setup: level 3 changed for the shared abstract");
    assert_eq!(e.list_levels(6).unwrap()[0].format, NumberFormat::Decimal, "list 6's level 0 is untouched");
    let mut ls = e.list_levels(5).unwrap();
    ls[0].text = "%1}".into();
    e.execute(Command::SetListDefinition(ListDefinitionChange { levels: ls, apply_to: ListApplyTo::WholeList, name: None }), fonts());
    assert_eq!(e.list_levels(5).unwrap()[0].text, "%1}", "list 5's own level 0 changed");
    assert_eq!(e.list_levels(6).unwrap()[0].text, "%1.", "list 6's level 0 is untouched");
    assert_eq!(label(&e, 0), "I}");
}

fn heading_list() -> Vec<u8> {
    let styles = "<w:style w:type=\"paragraph\" w:styleId=\"Heading1\"><w:name w:val=\"heading 1\"/><w:basedOn w:val=\"Normal\"/><w:pPr><w:numPr><w:numId w:val=\"1\"/></w:numPr></w:pPr></w:style>";
    let mut lv = vec![lvl(0, "decimal", "%1.", "<w:pStyle w:val=\"Heading1\"/>")];
    lv.extend((1..9).map(|i| lvl(i, "decimal", &format!("%{}.", i + 1), "")));
    let nums = numbering(&(abstract_of(0, &lv.concat()) + &num(1, 0)));
    pkg(&(p("H one", "<w:pStyle w:val=\"Heading1\"/>") + &p("H two", "<w:pStyle w:val=\"Heading1\"/>")), Some(&nums), styles)
}

#[test]
fn linking_a_level_to_a_style_numbers_that_style() {
    let styles = "<w:style w:type=\"paragraph\" w:styleId=\"Heading1\"><w:name w:val=\"heading 1\"/><w:basedOn w:val=\"Normal\"/></w:style>";
    let mut e = editor(&pkg(&(p("Plain", "") + &p("H one", "<w:pStyle w:val=\"Heading1\"/>")), None, styles));
    caret_in(&mut e, 0);
    let level = ListLevel {
        format: NumberFormat::Decimal,
        text: "%1.".into(),
        start: 1,
        linked_style: Some(StyleId("Heading1".into())),
        ..ListLevel::default()
    };
    e.execute(Command::SetListDefinition(ListDefinitionChange { levels: vec![level], apply_to: Default::default(), name: None }), fonts());
    let bytes = saved(&e);
    let st = part(&bytes, "word/styles.xml");
    let h1 = st.split("w:styleId=\"Heading1\"").nth(1).unwrap().split("</w:style>").next().unwrap();
    assert!(h1.contains("numPr"), "the linked style carries the list: {h1}");
    assert_eq!((label(&e, 0), label(&e, 1)), ("1.".into(), "2.".into()), "the heading is numbered");
    let again = editor(&bytes);
    assert_eq!((label(&again, 0), label(&again, 1)), ("1.".into(), "2.".into()), "as a reload numbers it");
}

#[test]
fn a_heading_library_entry_numbers_the_headings() {
    let styles = "<w:style w:type=\"paragraph\" w:styleId=\"Heading1\"><w:name w:val=\"heading 1\"/><w:basedOn w:val=\"Normal\"/></w:style>";
    let mut e = editor(&pkg(&(p("Plain", "") + &p("H one", "<w:pStyle w:val=\"Heading1\"/>")), None, styles));
    let library = e.list_galleries(crate::api::services::ListGalleryKind::Multilevel);
    let id = library[0].entries.iter().find(|x| x.levels[0].linked_style.is_some()).map(|x| x.id.clone()).expect("a heading entry");
    caret_in(&mut e, 0);
    e.execute(Command::ApplyListEntry(id), fonts());
    assert!(para(&e, 1).list.is_some(), "the heading is numbered");
    let again = editor(&saved(&e));
    assert_eq!(label(&again, 1), label(&e, 1), "as a reload numbers it");
}

#[test]
fn a_gallery_entry_keeps_a_lists_heading_links() {
    let mut e = editor(&heading_list());
    let before = levels_of(&mut e, 0);
    assert!(before[0].linked_style.is_some(), "setup: linked");
    e.execute(Command::ApplyListEntry(crate::api::services::ListEntryId("library:multilevel:2".into())), fonts());
    let after = levels_of(&mut e, 0);
    assert!(after[0].linked_style.is_some(), "the heading link survives");
    assert!(para(&e, 1).list.is_some(), "the second heading is still numbered");
}

#[test]
fn a_copied_list_leaves_its_style_links_to_the_original() {
    let mut e = editor(&heading_list());
    let mut ls = levels_of(&mut e, 1);
    ls[0].text = "%1)".into();
    caret_in(&mut e, 1);
    e.execute(Command::SetListDefinition(ListDefinitionChange { levels: ls, apply_to: ListApplyTo::ThisPointForward, name: None }), fonts());
    let xml = part(&saved(&e), "word/numbering.xml");
    assert_eq!(xml.matches("<w:abstractNum ").count(), 2, "setup: a copy");
    assert_eq!(xml.matches("<w:pStyle w:val=\"Heading1\"/>").count(), 1, "{xml}");
}

#[test]
fn duplicate_list_ids_are_saved_unchanged() {
    let dup =
        abstract_of(0, &levels("decimal", |i| format!("%{}.", i + 1))) + &abstract_of(1, &levels("bullet", |_| "o".into())) + &num(1, 0) + &num(1, 1);
    let original = numbering(&dup);
    let e = editor(&pkg(&p("A", ""), Some(&original), ""));
    assert_eq!(part(&saved(&e), "word/numbering.xml"), original);
}

#[test]
fn a_picture_bullet_definition_adds_its_image_once() {
    let png = open("FX-CUSTOM-LISTS.docx").document().images[0].clone();
    let nums = numbering(&(abstract_of(0, &levels("decimal", |i| format!("%{}.", i + 1))) + &num(1, 0)));
    let mut e = editor(&pkg(&np("A", 1, 0), Some(&nums), ""));
    let mut ls = e.list_levels(1).unwrap();
    ls[0].picture = Some(crate::api::value::Bytes(png));
    // A character XML can't hold, left out when written.
    ls[0].text = "\u{1}".into();
    caret_in(&mut e, 0);
    let images = e.document().images.len();
    let out = e.execute(Command::SetListDefinition(ListDefinitionChange { levels: ls, apply_to: ListApplyTo::WholeList, name: None }), fonts());
    assert!(matches!(out, crate::editing::Outcome::Changed(_)), "{out:?}");
    assert!(e.pending_images.is_empty(), "nothing queued after {out:?}");
    e.execute(Command::SetAlignment(Alignment::Center), fonts());
    assert_eq!(e.document().images.len(), images + 1, "the picture, once");
    e.execute(Command::Undo, fonts());
    e.execute(Command::Undo, fonts());
    assert!(e.document().resolver.as_ref().unwrap().numbering.pic_bullets.is_empty(), "undone");
}

#[test]
fn linking_a_style_to_a_level_ends_its_link_in_a_lists_own_level() {
    let styles = "<w:style w:type=\"paragraph\" w:styleId=\"Sub\"><w:name w:val=\"Sub\"/><w:basedOn w:val=\"Normal\"/><w:pPr><w:numPr><w:ilvl w:val=\"1\"/><w:numId w:val=\"1\"/></w:numPr></w:pPr></w:style>";
    let over = format!(
        "<w:num w:numId=\"1\"><w:abstractNumId w:val=\"0\"/><w:lvlOverride w:ilvl=\"1\">{}</w:lvlOverride></w:num>",
        lvl(1, "lowerLetter", "%2)", "<w:pStyle w:val=\"Sub\"/>")
    );
    let nums = numbering(&(abstract_of(0, &levels("decimal", |i| format!("%{}.", i + 1))) + &over));
    let mut e = editor(&pkg(&np("A", 1, 0), Some(&nums), styles));
    assert_eq!(e.list_levels(1).unwrap()[1].linked_style, Some(StyleId("Sub".into())), "setup: the override links it");
    caret_in(&mut e, 0);
    let level =
        ListLevel { format: NumberFormat::Decimal, text: "%1.".into(), start: 1, linked_style: Some(StyleId("Sub".into())), ..Default::default() };
    e.execute(Command::SetListDefinition(ListDefinitionChange { levels: vec![level], apply_to: ListApplyTo::WholeList, name: None }), fonts());
    let links = |e: &mut Editor| e.list_levels(1).unwrap().iter().map(|l| l.linked_style.clone()).collect::<Vec<_>>();
    let want: Vec<Option<StyleId>> = std::iter::once(Some(StyleId("Sub".into()))).chain(std::iter::repeat_n(None, 8)).collect();
    assert_eq!(links(&mut e), want);
    let bytes = saved(&e);
    let xml = part(&bytes, "word/numbering.xml");
    assert_eq!(xml.matches("<w:pStyle w:val=\"Sub\"/>").count(), 1, "{xml}");
    assert!(xml.contains("<w:lvlOverride w:ilvl=\"1\">"), "the override's other formatting stays: {xml}");
    assert_eq!(links(&mut editor(&bytes)), want, "as saved");
}

#[test]
fn a_new_list_from_a_lists_first_paragraph_keeps_its_start() {
    let abs = abstract_of(0, &levels("decimal", |i| format!("%{}.", i + 1)));
    let num = "<w:num w:numId=\"5\"><w:abstractNumId w:val=\"0\"/><w:lvlOverride w:ilvl=\"0\"><w:startOverride w:val=\"7\"/></w:lvlOverride></w:num>";
    let body = np("A", 5, 0) + &np("B", 5, 0) + &np("C", 5, 0);
    let labels = |e: &Editor| (0..3).map(|n| label(e, n)).collect::<Vec<_>>();
    for (from, want) in [(0, ["7)", "8)", "9)"]), (1, ["7.", "1)", "2)"])] {
        let mut e = editor(&pkg(&body, Some(&numbering(&(abs.clone() + num))), ""));
        assert_eq!(labels(&e), ["7.", "8.", "9."], "setup: the list starts at 7");
        let mut ls = levels_of(&mut e, from);
        ls[0].text = "%1)".into();
        caret_in(&mut e, from);
        e.execute(Command::SetListDefinition(ListDefinitionChange { levels: ls, apply_to: ListApplyTo::ThisPointForward, name: None }), fonts());
        assert_eq!(labels(&e), want, "from paragraph {from}");
        assert_eq!(labels(&editor(&saved(&e))), want, "from paragraph {from}, as saved");
    }
}

#[test]
fn a_style_named_num_id_is_not_direct_numbering() {
    let styles = concat!(
        "<w:style w:type=\"paragraph\" w:styleId=\"Heading1\"><w:name w:val=\"heading 1\"/><w:basedOn w:val=\"Normal\"/><w:pPr><w:numPr><w:numId w:val=\"1\"/></w:numPr></w:pPr></w:style>",
        "<w:style w:type=\"paragraph\" w:customStyle=\"1\" w:styleId=\"numId\"><w:name w:val=\"numId\"/><w:basedOn w:val=\"Normal\"/><w:pPr><w:numPr><w:numId w:val=\"1\"/></w:numPr></w:pPr></w:style>",
    );
    let mut lv = vec![lvl(0, "decimal", "%1.", "<w:pStyle w:val=\"Heading1\"/>")];
    lv.extend((1..9).map(|i| lvl(i, "decimal", &format!("%{}.", i + 1), "")));
    let nums = numbering(&(abstract_of(0, &lv.concat()) + &num(1, 0)));
    let body = p("H", "<w:pStyle w:val=\"Heading1\"/>") + &p("X", "<w:pStyle w:val=\"numId\"/>");
    let mut e = editor(&pkg(&body, Some(&nums), styles));
    assert_eq!((label(&e, 0), label(&e, 1)), ("1.".into(), String::new()), "setup: the level's own style shows the label");
    let mut ls = levels_of(&mut e, 0);
    ls[0].text = "%1)".into();
    caret_in(&mut e, 0);
    e.execute(Command::SetListDefinition(ListDefinitionChange { levels: ls, apply_to: ListApplyTo::WholeList, name: None }), fonts());
    assert_eq!((label(&e, 0), label(&e, 1)), ("1)".into(), String::new()));
    let again = editor(&saved(&e));
    assert_eq!((label(&again, 0), label(&again, 1)), ("1)".into(), String::new()), "as a reload labels them");
}

#[test]
fn numbering_a_bullet_into_a_list_without_its_level_takes_the_lists_level() {
    let nums =
        numbering(&(abstract_of(0, &lvl(0, "decimal", "%1.", "")) + &abstract_of(1, &levels("bullet", |_| "o".into())) + &num(1, 0) + &num(2, 1)));
    let mut e = editor(&pkg(&(np("A", 1, 0) + &np("B", 2, 2)), Some(&nums), ""));
    assert_eq!((label(&e, 0), label(&e, 1)), ("1.".into(), "o".into()), "setup");
    caret_in(&mut e, 1);
    e.execute(Command::SetList(Some(ListKind::Numbered)), fonts());
    assert_eq!(para(&e, 1).style.num, Some((1, 0)), "the list's own level");
    assert_eq!(label(&e, 1), "2.");
}
