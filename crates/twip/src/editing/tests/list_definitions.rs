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

/// The paragraph `n` in reading order, tables too.
fn para_at(e: &Editor, n: usize) -> &Paragraph {
    e.index().para(e.document(), super::para_id(e, n)).expect("para")
}

#[test]
fn increase_indent_moves_the_whole_list_only_from_its_first_item() {
    let nums = numbering(&(abstract_of(0, &levels("decimal", |i| format!("%{}.", i + 1))) + &num(1, 0)));
    let cell = |text: &str| format!("<w:tc><w:tcPr><w:tcW w:w=\"4000\" w:type=\"dxa\"/></w:tcPr>{}</w:tc>", np(text, 1, 0));
    let table = format!(
        "<w:tbl><w:tblPr><w:tblW w:w=\"0\" w:type=\"auto\"/></w:tblPr><w:tblGrid><w:gridCol w:w=\"4000\"/><w:gridCol w:w=\"4000\"/></w:tblGrid><w:tr>{}{}</w:tr></w:tbl><w:p/>",
        cell("one"),
        cell("two")
    );
    // The list runs on into the next cell, and over a section break.
    let section = p("one", "<w:numPr><w:ilvl w:val=\"0\"/><w:numId w:val=\"1\"/></w:numPr><w:sectPr><w:type w:val=\"continuous\"/></w:sectPr>")
        + &np("two", 1, 0);
    for body in [table, section] {
        let mut e = editor(&pkg(&body, Some(&nums), ""));
        let left = |e: &Editor| para_at(e, 0).style.ind_left;
        let was = left(&e);
        caret_in(&mut e, 1);
        e.execute(Command::IncreaseIndent, fonts());
        assert_eq!(para_at(&e, 1).style.num, Some((1, 1)), "a later item goes a level down: {body}");
        assert_eq!(left(&e), was, "the list stays");
        caret_in(&mut e, 0);
        e.execute(Command::IncreaseIndent, fonts());
        assert_eq!((para_at(&e, 0).style.num, left(&e)), (Some((1, 0)), was + 36.0), "the first item moves the list");
    }
}

#[test]
fn moving_a_list_keeps_its_levels_other_tab_stops() {
    let tabs = "<w:tabs><w:tab w:val=\"left\" w:pos=\"720\"/><w:tab w:val=\"right\" w:leader=\"dot\" w:pos=\"9000\"/></w:tabs>";
    let lvls =
        levels("decimal", |i| format!("%{}.", i + 1)).replacen("<w:pPr><w:ind w:left=\"720\"", &format!("<w:pPr>{tabs}<w:ind w:left=\"720\""), 1);
    let nums = numbering(&(abstract_of(0, &lvls) + &num(1, 0)));
    let mut e = editor(&pkg(&np("one", 1, 0), Some(&nums), ""));
    assert_eq!(levels_of(&mut e, 0)[0].tab, Some(Points(36.0)), "setup");
    caret_in(&mut e, 0);
    e.execute(Command::IncreaseIndent, fonts());
    assert_eq!(levels_of(&mut e, 0)[0].tab, Some(Points(72.0)), "the level's tab moves with it");
    let xml = part(&saved(&e), "word/numbering.xml");
    let level = &xml[..xml.find("w:ilvl=\"1\"").expect("level 1")];
    assert!(level.contains("<w:tab w:val=\"left\" w:pos=\"1440\"/>"), "its kind kept: {level}");
    assert!(level.contains("<w:tab w:val=\"right\" w:leader=\"dot\" w:pos=\"9000\"/>"), "the other stop stays: {level}");
}

/// Two items of a list whose first level is `fmt` (and `extra` in its `w:lvl`), then a plain paragraph.
fn list_of(fmt: &str, extra: &str) -> Editor {
    let lvls: String =
        (0..9).map(|i| lvl(i, if i == 0 { fmt } else { "decimal" }, &format!("%{}.", i + 1), if i == 0 { extra } else { "" })).collect();
    let body = format!("{}{}{}", np("one", 1, 0), np("two", 1, 0), p("plain", ""));
    let mut e = editor(&pkg(&body, Some(&numbering(&format!("{}{}", abstract_of(1, &lvls), num(1, 1)))), ""));
    e.complete(fonts());
    e
}

#[test]
fn every_number_style_twip_numbers_with_reads_back_as_itself() {
    use crate::editing::list_defs::NumberStyle;
    for style in NumberStyle::NAMED {
        let mut e = list_of(style.val(), "");
        caret_in(&mut e, 0);
        let d = e.list_definition().expect("a list");
        assert_eq!(d.levels[0].format, style, "{}", style.val());
        assert_eq!(d.levels[0].level.format, style.format());
    }
    let mut e = list_of("aiueoFullWidth", "");
    caret_in(&mut e, 0);
    assert_eq!(e.list_definition().expect("a list").levels[0].format, NumberStyle::Other("aiueoFullWidth".into()), "unlisted: as named");
}

#[test]
fn the_caret_list_reads_without_a_dialog() {
    let mut e = list_of("decimal", "");
    caret_in(&mut e, 1);
    let d = e.list_definition().expect("a list");
    assert_eq!((d.id, d.level, d.bullet, d.levels.len()), (1, 0, false, 9));
    assert_eq!(d.levels[0].level.text, "%1.");
    caret_in(&mut e, 2);
    assert!(e.list_definition().is_none(), "outside a list");
}

#[test]
fn a_level_in_full_sets_its_style_and_its_numbers_formatting() {
    use crate::api::value::{AutoOr, Color, HighlightColor, NoneOr, UnderlineStyle};
    use crate::editing::list_defs::{LabelFormat, ListLevelsChange, NumberStyle};
    let mut e = list_of("decimal", "");
    caret_in(&mut e, 0);
    let mut levels = e.list_definition().expect("a list").levels;
    levels[0].format = NumberStyle::Chicago;
    levels[0].level.text = "%1".into();
    levels[0].label = LabelFormat {
        bold: Some(true),
        italic: Some(false),
        underline: Some(UnderlineStyle::Double),
        size: Some(Points(14.0)),
        color: Some(AutoOr::Value(Color(0xFF0000))),
        highlight: Some(NoneOr::Value(HighlightColor::Yellow)),
    };
    let out = e.set_list_levels(ListLevelsChange { levels: levels[..1].to_vec(), ..Default::default() }, fonts());
    assert!(matches!(out, crate::editing::commands::Outcome::Changed(_)), "{out:?}");
    assert_eq!((label(&e, 0), label(&e, 1)), ("*".to_string(), "†".to_string()), "drawn in the new style");
    let run = &para(&e, 1).list.as_ref().expect("a label").run;
    assert!(run.bold && !run.italic && run.size == 14.0 && run.color == Some(0xFF0000), "the label takes it");
    assert!(!para(&e, 1).mark.bold, "the paragraph doesn't");
    let read = e.list_definition().expect("a list").levels[0].clone();
    assert_eq!((read.format, read.label), (NumberStyle::Chicago, levels[0].label.clone()));
    let xml = part(&saved(&e), "word/numbering.xml");
    for x in [
        "w:val=\"chicago\"",
        "<w:b/>",
        "<w:bCs/>",
        "<w:i w:val=\"0\"/>",
        "<w:u w:val=\"double\"/>",
        "<w:sz w:val=\"28\"/>",
        "w:val=\"FF0000\"",
        "w:val=\"yellow\"",
    ] {
        assert!(xml.contains(x), "{x} in {xml}");
    }
    // Cleared: the paragraph's formatting applies again.
    levels[0].label = LabelFormat::default();
    e.set_list_levels(ListLevelsChange { levels: levels[..1].to_vec(), ..Default::default() }, fonts());
    assert_eq!(e.list_definition().expect("a list").levels[0].label, LabelFormat::default());
    assert!(!part(&saved(&e), "word/numbering.xml").contains("<w:b/>"));
    // Refused: a style name that can't be written, a size out of bounds.
    levels[0].format = NumberStyle::Other("a\"b".into());
    assert!(matches!(
        e.set_list_levels(ListLevelsChange { levels: levels[..1].to_vec(), ..Default::default() }, fonts()),
        crate::editing::commands::Outcome::Refused(_)
    ));
    levels[0].format = NumberStyle::Decimal;
    levels[0].label.size = Some(Points(0.0));
    assert!(matches!(
        e.set_list_levels(ListLevelsChange { levels: levels[..1].to_vec(), ..Default::default() }, fonts()),
        crate::editing::commands::Outcome::Refused(_)
    ));
}

#[test]
fn the_api_level_keeps_a_style_and_number_formatting_it_cant_show() {
    use crate::editing::list_defs::NumberStyle;
    let mut e = list_of("hex", "<w:rPr><w:b/></w:rPr>");
    caret_in(&mut e, 0);
    let mut shown = levels_of(&mut e, 0);
    assert_eq!(shown[0].format, NumberFormat::Decimal, "the API's enum has no hex");
    shown[0].text_indent = Points(54.0);
    e.execute(Command::SetListDefinition(ListDefinitionChange { levels: shown[..1].to_vec(), ..Default::default() }), fonts());
    let d = e.list_definition().expect("a list").levels[0].clone();
    assert_eq!((d.format, d.label.bold, d.level.text_indent), (NumberStyle::Hex, Some(true), Points(54.0)));
    // Choosing a style the API names replaces it.
    shown[0].format = NumberFormat::UpperRoman;
    e.execute(Command::SetListDefinition(ListDefinitionChange { levels: shown[..1].to_vec(), ..Default::default() }), fonts());
    assert_eq!(e.list_definition().expect("a list").levels[0].format, NumberStyle::UpperRoman);
}

#[test]
fn a_click_on_a_number_formats_the_levels_numbers() {
    use crate::editing::input::PointerKind;
    use crate::editing::keymap::Mods;
    let mut e = list_of("decimal", "");
    let id = super::texts_ids(&e)[1];
    let g = e.page_geometries().into_iter().find(|(p, _)| *p == 0).map(|(_, g)| g).expect("page 0");
    let l = g.lines.iter().find(|l| l.shows(id)).expect("a line");
    let (a, b) = l.label.expect("the number's extent");
    let (x, y) = e.stack().to_doc(0, (a + b) / 2.0, (l.top + l.bottom) / 2.0).expect("on the stack");
    e.pointer_input(PointerKind::Down, x, y, 1, Mods::default(), fonts());
    e.pointer_input(PointerKind::Up, x, y, 1, Mods::default(), fonts());
    let sel = e.list_label_selection().cloned().expect("the numbers are selected");
    assert_eq!((sel.para, sel.list, sel.level), (id, 1, 0));
    let view = e.view(0..1);
    assert_eq!(view.list_labels.len(), 2, "both numbers shaded");
    e.execute(Command::SetBold(true), fonts());
    e.execute(Command::SetFontColor(crate::api::value::AutoOr::Value(crate::api::value::Color(0x0000FF))), fonts());
    for n in [0, 1] {
        let run = &para(&e, n).list.as_ref().expect("a label").run;
        assert!(run.bold && run.color == Some(0x0000FF), "number {n} formatted");
        assert!(!para(&e, n).mark.bold, "text {n} isn't");
    }
    assert!(e.list_definition().expect("a list").levels[0].label.bold == Some(true));
    e.execute(Command::Undo, fonts());
    e.execute(Command::Undo, fonts());
    assert!(!para(&e, 0).list.as_ref().expect("a label").run.bold, "one undo step each");
    // Clear Formatting isn't number formatting: it ends the number selection and acts on the text.
    e.pointer_input(PointerKind::Down, x, y, 1, Mods::default(), fonts());
    e.pointer_input(PointerKind::Up, x, y, 1, Mods::default(), fonts());
    assert!(e.list_label_selection().is_some());
    e.execute(Command::ClearFormatting, fonts());
    assert!(e.list_label_selection().is_none(), "Clear Formatting ends it");
    assert!(e.view(0..1).list_labels.is_empty(), "no number shaded");
    // Another selection ends it: formatting goes to the text again.
    caret_in(&mut e, 0);
    assert!(e.list_label_selection().is_none());
    // A click on the text isn't one on the number.
    let x1 = l.stops.iter().find(|s| s.para == id).expect("a stop").x + 4.0;
    let (x, y) = e.stack().to_doc(0, x1, (l.top + l.bottom) / 2.0).expect("on the stack");
    e.pointer_input(PointerKind::Down, x, y, 1, Mods::default(), fonts());
    e.pointer_input(PointerKind::Up, x, y, 1, Mods::default(), fonts());
    assert!(e.list_label_selection().is_none());
}

/// Clicks the middle of the number on paragraph `n`'s first line.
fn click_number(e: &mut Editor, n: usize) {
    let (pg, p) = number_point(e, n);
    click_at(e, pg, p);
}

/// The page and page point at the middle of paragraph `n`'s number.
fn number_point(e: &mut Editor, n: usize) -> (usize, [f64; 2]) {
    let id = super::para_id(e, n);
    for (pg, g) in e.page_geometries() {
        if let Some(l) = g.lines.iter().find(|l| l.shows(id) && l.label.is_some()) {
            let (a, b) = l.label.expect("the number's extent");
            return (pg, l.page((a + b) / 2.0, (l.top + l.bottom) / 2.0));
        }
    }
    panic!("paragraph {n} has no number drawn")
}

fn click_at(e: &mut Editor, pg: usize, p: [f64; 2]) {
    use crate::editing::input::PointerKind;
    use crate::editing::keymap::Mods;
    let (x, y) = e.stack().to_doc(pg, p[0], p[1]).expect("on the stack");
    e.pointer_input(PointerKind::Down, x, y, 1, Mods::default(), fonts());
    e.pointer_input(PointerKind::Up, x, y, 1, Mods::default(), fonts());
}

fn label_run(e: &Editor, n: usize) -> crate::docx::Run {
    para(e, n).list.as_ref().expect("a label").run.clone()
}

#[test]
fn a_numbering_part_with_its_own_prefix_takes_number_formatting() {
    let lvls: String = (0..9)
        .map(|i| {
            format!("<ns0:lvl ns0:ilvl=\"{i}\"><ns0:start ns0:val=\"1\"/><ns0:numFmt ns0:val=\"decimal\"/><ns0:lvlText ns0:val=\"%{}.\"/><ns0:lvlJc ns0:val=\"left\"/><ns0:pPr><ns0:ind ns0:left=\"720\" ns0:hanging=\"360\"/></ns0:pPr></ns0:lvl>", i + 1)
        })
        .collect();
    let numbering = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><ns0:numbering xmlns:ns0=\"{W}\"><ns0:abstractNum ns0:abstractNumId=\"1\">{lvls}</ns0:abstractNum><ns0:num ns0:numId=\"1\"><ns0:abstractNumId ns0:val=\"1\"/></ns0:num></ns0:numbering>"
    );
    let mut e = editor(&pkg(&format!("{}{}", np("one", 1, 0), np("two", 1, 0)), Some(&numbering), ""));
    e.complete(fonts());
    assert_eq!(label(&e, 0), "1.", "setup: numbered");
    click_number(&mut e, 0);
    assert!(e.list_label_selection().is_some(), "the numbers are selected");
    let out = e.execute(Command::SetBold(true), fonts());
    assert!(matches!(out, crate::editing::commands::Outcome::Changed(_)), "{out:?}");
    let lig = Typography { ligatures: Some(Ligatures::All), ..Default::default() };
    let out = e.execute(Command::SetTypography(lig), fonts());
    assert!(matches!(out, crate::editing::commands::Outcome::Changed(_)), "{out:?}");
    assert!(label_run(&e, 1).bold && !para(&e, 1).mark.bold, "the numbers are bold, the text isn't");
    let xml = part(&saved(&e), "word/numbering.xml");
    assert!(xml.contains("<ns0:b/>") && !xml.contains("<w:b"), "in the numbering part's names: {xml}");
    assert!(xml.contains("<ns0:rPr><ns0:b/><ns0:bCs/><w14:ligatures xmlns:w14="), "w14 declared where the part doesn't: {xml}");
    let again = editor(&saved(&e));
    let run = label_run(&again, 1);
    assert!(run.bold && run.typography.ligatures == 15, "a reload draws it: {:?}", run.typography);
}

#[test]
fn a_dot_leader_on_a_numbered_line_is_not_its_number() {
    let leader = "<w:p><w:pPr><w:numPr><w:ilvl w:val=\"0\"/><w:numId w:val=\"1\"/></w:numPr><w:tabs><w:tab w:val=\"right\" w:leader=\"dot\" w:pos=\"8000\"/></w:tabs></w:pPr><w:r><w:t>Definitions</w:t></w:r><w:r><w:tab/><w:t>5</w:t></w:r></w:p>";
    let lvls = levels("decimal", |i| format!("%{}.", i + 1));
    let mut e = editor(&pkg(&format!("{leader}{}", np("two", 1, 0)), Some(&numbering(&format!("{}{}", abstract_of(1, &lvls), num(1, 1)))), ""));
    e.complete(fonts());
    let id = super::para_id(&e, 0);
    let g = e.page_geometries().into_iter().find(|(p, _)| *p == 0).map(|(_, g)| g).expect("page 0");
    let l = g.lines.iter().find(|l| l.shows(id)).expect("a line");
    let (a, b) = l.label.expect("the number's extent");
    let first = l.first_of(id).and_then(|u| l.x_of(id, u)).expect("the text's start");
    assert!(b <= first + 0.01, "the number ends before the text: {a}..{b}, text at {first}");
    assert!(b - a < 20.0, "only the number: {a}..{b}");
    let mid = (l.top + l.bottom) / 2.0;
    // The text, then the leader
    let stops: Vec<f64> = l.stops.iter().filter(|s| s.para == id).map(|s| s.x).collect();
    let (text, dots) = (first + 10.0, (stops[stops.len() - 3] + stops[stops.len() - 2]) / 2.0);
    for x in [text, dots] {
        click_at(&mut e, 0, l.page(x, mid));
        assert!(e.list_label_selection().is_none(), "a click at {x} isn't on the number");
    }
    click_number(&mut e, 0);
    assert!(e.list_label_selection().is_some(), "the number is");
    let view = e.view(0..1);
    let band = &view.list_labels.iter().find(|(p, b)| *p == 0 && (b.rect[1] - l.top).abs() < 0.01).expect("the number's shading").1;
    assert!((band.rect[0] - a).abs() < 0.01 && (band.rect[2] - b).abs() < 0.01, "shading only the number: {:?}", band.rect);
}

#[test]
fn a_click_on_a_number_in_a_tbrl_section_selects_the_numbers() {
    let lvls = levels("decimal", |i| format!("%{}.", i + 1));
    let brk = "<w:p><w:pPr><w:sectPr><w:pgSz w:w=\"12240\" w:h=\"15840\"/><w:pgMar w:top=\"1440\" w:right=\"1440\" w:bottom=\"1440\" w:left=\"1440\" w:header=\"720\" w:footer=\"720\" w:gutter=\"0\"/><w:textDirection w:val=\"tbRl\"/></w:sectPr></w:pPr></w:p>";
    let mut e = editor(&pkg(
        &format!("{}{}{brk}", np("one", 1, 0), np("two", 1, 0)),
        Some(&numbering(&format!("{}{}", abstract_of(1, &lvls), num(1, 1)))),
        "",
    ));
    e.complete(fonts());
    let id = super::para_id(&e, 1);
    let g = e.page_geometries().into_iter().find(|(p, _)| *p == 0).map(|(_, g)| g).expect("page 0");
    let l = g.lines.iter().find(|l| l.shows(id)).expect("a line");
    assert_ne!(l.dir(), crate::editing::geometry::LineDir::Horizontal, "setup: the line runs down the page");
    click_number(&mut e, 1);
    let sel = e.list_label_selection().cloned().expect("the numbers are selected");
    assert_eq!((sel.para, sel.list, sel.level), (id, 1, 0));
}

/// A list whose paragraphs are in a 16 pt bold paragraph style.
fn styled_list() -> Editor {
    let style = "<w:style w:type=\"paragraph\" w:styleId=\"Big\"><w:name w:val=\"Big\"/><w:rPr><w:b/><w:bCs/><w:sz w:val=\"32\"/><w:szCs w:val=\"32\"/></w:rPr></w:style>";
    let lvls = levels("decimal", |i| format!("%{}.", i + 1));
    let big = |t: &str| p(t, "<w:pStyle w:val=\"Big\"/><w:numPr><w:ilvl w:val=\"0\"/><w:numId w:val=\"1\"/></w:numPr>");
    let mut e = editor(&pkg(&format!("{}{}", big("one"), big("two")), Some(&numbering(&format!("{}{}", abstract_of(1, &lvls), num(1, 1)))), style));
    e.complete(fonts());
    e
}

#[test]
fn number_formatting_starts_from_the_number_as_it_draws() {
    use crate::api::state::Tri;
    let mut e = styled_list();
    assert_eq!(label_run(&e, 0).size, 16.0, "setup: the number draws at the style's size");
    click_number(&mut e, 0);
    e.execute(Command::GrowFont, fonts());
    assert_eq!((label_run(&e, 0).size, label_run(&e, 1).size), (18.0, 18.0), "grown from 16 pt");
    e.execute(Command::ShrinkFont, fonts());
    assert_eq!(label_run(&e, 0).size, 16.0, "shrunk from 18 pt");
    assert_eq!(e.format_state().expect("a state").character.bold, Tri::Uniform(true), "the number's formatting: bold, from its style");
    e.execute(Command::SetBold(false), fonts());
    assert!(!label_run(&e, 0).bold && para(&e, 0).mark.bold, "the number isn't bold; the text still is");
    assert!(part(&saved(&e), "word/numbering.xml").contains("<w:b w:val=\"0\"/>"), "turned off over the style");
    assert_eq!(e.format_state().expect("a state").character.bold, Tri::Uniform(false));
    assert!(!label_run(&editor(&saved(&e)), 1).bold, "as a reload draws it");
}

#[test]
fn the_state_shows_the_selected_numbers_formatting() {
    use crate::api::state::Tri;
    let mut e = list_of("decimal", "");
    click_number(&mut e, 0);
    e.execute(Command::SetBold(true), fonts());
    assert_eq!(e.format_state().expect("a state").character.bold, Tri::Uniform(true), "the number's");
    assert!(!para(&e, 0).mark.bold, "the text isn't bold");
    // Home at the paragraph's start leaves the caret where it is, and ends the number selection.
    e.execute(
        Command::MoveSelection(MoveSelection { by: MoveUnit::LineEdge, direction: crate::api::value::Direction::Backward, extend: None }),
        fonts(),
    );
    assert!(e.list_label_selection().is_none(), "navigation ends it");
    assert_eq!(e.format_state().expect("a state").character.bold, Tri::Uniform(false), "the text's again");
}
