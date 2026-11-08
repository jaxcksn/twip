//! Custom lists: numbering as the reference numbers it, the list edits and their dialogs.

use super::open;
use crate::docx::Block;

/// Each paragraph's text and list label, in document order.
fn labels(e: &crate::editing::Editor) -> Vec<(String, String)> {
    fn walk(blocks: &[Block], out: &mut Vec<(String, String)>) {
        for b in blocks {
            match b {
                Block::Para(p) => {
                    let text = crate::editing::text::UnitText::of(p).text;
                    out.push((text, p.list.as_ref().map(|l| l.text.clone()).unwrap_or_default()));
                }
                Block::Table(t) => t.rows.iter().flat_map(|r| r.cells.iter()).for_each(|c| walk(&c.blocks, out)),
            }
        }
    }
    let mut out = Vec::new();
    e.document().sections.iter().for_each(|s| walk(&s.blocks, &mut out));
    out
}

/// The reference labels for FX-CUSTOM-LISTS.docx (its row-end mark left out).
const WORD: [(&str, &str); 27] = [
    ("FX-CUSTOM-LISTS", ""),
    ("Wingdings bullet one", "\u{F076}"),
    ("Wingdings bullet two", "\u{F076}"),
    ("Courier sub-bullet", "o"),
    ("Picture bullet one", "\u{F0B7}"),
    ("Picture bullet two", "\u{F0B7}"),
    ("Section format one", "Section 1."),
    ("Section format two", "Section 2."),
    ("Section format sub-item", "a."),
    ("Legal article one", "Article I."),
    ("Legal section one", "Section 1.01"),
    ("Legal clause one", "(a)"),
    ("Legal clause two", "(b)"),
    ("Legal section two", "Section 1.02"),
    ("Legal article two", "Article II."),
    ("Legal section three", "Section 2.01"),
    ("Decimal one", "1."),
    ("Decimal two", "2."),
    ("Cell A", ""),
    ("Cell B", ""),
    ("Decimal three, continued after the table", "3."),
    ("Restarted at five", "5."),
    ("Restarted six", "6."),
    ("Part override one", "Part G:"),
    ("Part override two", "Part H:"),
    ("Restarted at one", "1."),
    ("Restarted two", "2."),
];

#[test]
fn custom_lists_number_as_word_numbers_them() {
    let e = open("FX-CUSTOM-LISTS.docx");
    let want: Vec<(String, String)> = WORD.iter().map(|(t, l)| (t.to_string(), l.to_string())).collect();
    assert_eq!(labels(&e), want);
}

#[test]
fn custom_lists_save_unchanged_byte_for_byte() {
    let e = open("FX-CUSTOM-LISTS.docx");
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/FX-CUSTOM-LISTS.docx");
    let read = |bytes: &[u8], name: &str| {
        use std::io::Read;
        let mut s = String::new();
        zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap().by_name(name).unwrap().read_to_string(&mut s).unwrap();
        s
    };
    let saved = crate::docx::write::save(e.document()).expect("saves");
    let original = std::fs::read(path).expect("fixture");
    for part in ["word/numbering.xml", "word/document.xml", "word/styles.xml"] {
        assert_eq!(read(&saved, part), read(&original, part), "{part}");
    }
}

fn caret_in(e: &mut crate::editing::Editor, n: usize) {
    use crate::editing::position::{Position, StoryId};
    let at = Position::text(StoryId::Body, super::para_id(e, n), crate::layout::Unit(0));
    e.set_caret_for_benchmark(crate::editing::selection::Selection::Caret(at));
}

fn label(e: &crate::editing::Editor, text: &str) -> String {
    labels(e).into_iter().find(|(t, _)| t == text).map(|(_, l)| l).unwrap_or_else(|| panic!("no paragraph {text:?}"))
}

fn read_again(e: &crate::editing::Editor) -> crate::editing::Editor {
    let bytes = crate::docx::write::save(e.document()).expect("saves");
    crate::editing::Editor::open(&bytes, super::fonts()).expect("reads again")
}

fn levels_of(e: &mut crate::editing::Editor, n: usize) -> Vec<crate::api::command::ListLevel> {
    caret_in(e, n);
    let (_, num, _) = e.selection_list().expect("a list");
    e.list_levels(num).expect("levels")
}

#[test]
fn defining_a_list_changes_the_whole_list_from_a_point_or_a_paragraph_and_saves() {
    use super::fonts;
    use crate::api::command::{Command, ListApplyTo, ListDefinitionChange, NumberFormat};
    let mut e = open("FX-CUSTOM-LISTS.docx");
    let before = labels(&e);
    // The whole list: "Section 1." becomes "Chapter I:".
    let mut levels = levels_of(&mut e, 6);
    assert_eq!((levels[0].format, levels[0].text.as_str()), (NumberFormat::Decimal, "Section %1."));
    (levels[0].format, levels[0].text) = (NumberFormat::UpperRoman, "Chapter %1:".into());
    e.execute(
        Command::SetListDefinition(ListDefinitionChange { levels: levels[..1].to_vec(), apply_to: ListApplyTo::WholeList, name: None }),
        fonts(),
    );
    assert_eq!(
        (label(&e, "Section format one"), label(&e, "Section format two"), label(&e, "Section format sub-item")),
        ("Chapter I:".into(), "Chapter II:".into(), "a.".into())
    );
    // From this point on: "Decimal two" and after start a new list.
    let mut levels = levels_of(&mut e, 17);
    (levels[0].format, levels[0].text) = (NumberFormat::LowerLetter, "%1)".into());
    e.execute(
        Command::SetListDefinition(ListDefinitionChange { levels: levels[..1].to_vec(), apply_to: ListApplyTo::ThisPointForward, name: None }),
        fonts(),
    );
    assert_eq!(
        (label(&e, "Decimal one"), label(&e, "Decimal two"), label(&e, "Decimal three, continued after the table")),
        ("1.".into(), "a)".into(), "b)".into())
    );
    // The current paragraph only.
    let mut levels = levels_of(&mut e, 2);
    (levels[0].text, levels[0].font) = ("\u{F0FC}".into(), Some("Wingdings".into()));
    e.execute(
        Command::SetListDefinition(ListDefinitionChange { levels: levels[..1].to_vec(), apply_to: ListApplyTo::CurrentParagraph, name: None }),
        fonts(),
    );
    assert_eq!((label(&e, "Wingdings bullet one"), label(&e, "Wingdings bullet two")), ("\u{F076}".into(), "\u{F0FC}".into()));
    let now = labels(&e);
    assert_eq!(labels(&read_again(&e)), now, "the saved lists number the same read again");
    for _ in 0..3 {
        e.execute(Command::Undo, fonts());
    }
    assert_eq!(labels(&e), before, "each is one undo step");
}

#[test]
fn a_list_defined_outside_a_list_is_new_and_takes_the_selection() {
    use super::fonts;
    use crate::api::command::{Command, ListDefinitionChange, ListLevel, NumberFormat};
    let mut e = open("FX-CUSTOM-LISTS.docx");
    caret_in(&mut e, 0);
    let bullet =
        ListLevel { format: NumberFormat::Bullet, text: "\u{F0D8}".into(), start: 1, font: Some("Wingdings".into()), ..ListLevel::default() };
    e.execute(Command::SetListDefinition(ListDefinitionChange { levels: vec![bullet], apply_to: Default::default(), name: None }), fonts());
    assert_eq!(label(&e, "FX-CUSTOM-LISTS"), "\u{F0D8}");
    assert_eq!(labels(&read_again(&e)), labels(&e));
    e.execute(Command::Undo, fonts());
    assert_eq!(label(&e, "FX-CUSTOM-LISTS"), "", "one undo step");
}

#[test]
fn numbering_values_restart_continue_and_indents_adjust() {
    use super::fonts;
    use crate::api::command::{Command, FollowNumber, ListIndents, NumberingMode, NumberingValue};
    use crate::api::value::Points;
    let mut e = open("FX-CUSTOM-LISTS.docx");
    caret_in(&mut e, 7);
    e.execute(Command::SetNumberingValue(NumberingValue { mode: NumberingMode::StartNew, value: 10, advance: false }), fonts());
    assert_eq!((label(&e, "Section format one"), label(&e, "Section format two")), ("Section 1.".into(), "Section 10.".into()));
    caret_in(&mut e, 17);
    e.execute(Command::RestartNumbering, fonts());
    assert_eq!((label(&e, "Decimal two"), label(&e, "Decimal three, continued after the table")), ("1.".into(), "2.".into()));
    caret_in(&mut e, 25);
    e.execute(Command::ContinueNumbering, fonts());
    assert_eq!((label(&e, "Restarted at one"), label(&e, "Restarted two")), ("Part I:".into(), "Part J:".into()), "continues the Part list");
    caret_in(&mut e, 16);
    e.execute(
        Command::AdjustListIndents(ListIndents { number_position: Points(0.0), text_indent: Points(27.0), follow: FollowNumber::Space, tab: None }),
        fonts(),
    );
    let levels = levels_of(&mut e, 16);
    assert_eq!((levels[0].number_position, levels[0].text_indent, levels[0].follow), (Points(0.0), Points(27.0), FollowNumber::Space));
    let p = e.document().sections[0].blocks.iter().find_map(|b| match b {
        Block::Para(p) if crate::editing::text::UnitText::of(p).text == "Decimal one" => Some(p.style.ind_left),
        _ => None,
    });
    assert_eq!(p, Some(27.0), "the paragraphs take the level's indent");
    assert_eq!(labels(&read_again(&e)), labels(&e));
}

#[test]
fn galleries_list_the_library_recent_and_document_lists_and_apply_them() {
    use super::fonts;
    use crate::api::command::Command;
    use crate::api::services::{ListEntryId, ListGalleryKind, ListGallerySectionKind};
    let mut e = open("FX-CUSTOM-LISTS.docx");
    let section = |e: &crate::editing::Editor, k, s| e.list_galleries(k).into_iter().find(|x| x.kind == s).map(|x| x.entries).unwrap_or_default();
    assert_eq!(section(&e, ListGalleryKind::Bullet, ListGallerySectionKind::Library).len(), 6);
    assert_eq!(section(&e, ListGalleryKind::Numbering, ListGallerySectionKind::Library).len(), 7);
    let multi = section(&e, ListGalleryKind::Multilevel, ListGallerySectionKind::Library);
    assert_eq!((multi.len(), multi[1].label.as_str()), (7, "1. 1.1. 1.1.1."));
    let bullets: Vec<String> = section(&e, ListGalleryKind::Bullet, ListGallerySectionKind::Document).into_iter().map(|x| x.label).collect();
    assert_eq!(bullets, ["\u{F076}", "\u{F0B7}"], "the document's bullet lists");
    let multi_doc = section(&e, ListGalleryKind::Multilevel, ListGallerySectionKind::Document);
    assert_eq!(multi_doc.iter().map(|x| x.label.as_str()).collect::<Vec<_>>(), ["Article I. Section 1.01 (a)"]);
    // A numbering entry in a list: the current level takes it, for the whole list.
    caret_in(&mut e, 16);
    e.execute(Command::ApplyListEntry(ListEntryId("library:numbering:3".into())), fonts());
    assert_eq!((label(&e, "Decimal one"), label(&e, "Decimal two")), ("I.".into(), "II.".into()));
    assert_eq!(section(&e, ListGalleryKind::Numbering, ListGallerySectionKind::RecentlyUsed).len(), 1, "now recently used");
    // A bullet outside a list: a new list.
    caret_in(&mut e, 0);
    e.execute(Command::ApplyListEntry(ListEntryId("library:bullet:1".into())), fonts());
    assert_eq!(label(&e, "FX-CUSTOM-LISTS"), "\u{F0B7}");
    // A document list continues it.
    caret_in(&mut e, 4);
    e.execute(Command::ApplyListEntry(ListEntryId("document:1".into())), fonts());
    assert_eq!(label(&e, "Picture bullet one"), "\u{F076}");
    assert!(e.list_entry_preview(&ListEntryId("library:multilevel:2".into())).is_some_and(|b| b.len() == 3));
    assert!(e.list_entry_preview(&ListEntryId("library:bullet:9".into())).is_none());
}

/// The images the first page draws, by image index.
fn drawn_images(doc: &crate::docx::Document) -> Vec<usize> {
    let snap = crate::layout::layout(doc, super::fonts());
    let page = &snap.pages()[0];
    crate::layout::traverse::paint_items(page)
        .into_iter()
        .filter_map(|i| match i {
            crate::layout::fragment::Item::Image { key, .. } => key.index,
            _ => None,
        })
        .collect()
}

#[test]
fn picture_bullets_draw_their_image_and_a_new_one_saves_with_its_part() {
    use super::fonts;
    use crate::api::command::{Command, ListApplyTo, ListDefinitionChange};
    use crate::api::value::Bytes;
    let mut e = open("FX-CUSTOM-LISTS.docx");
    let levels = levels_of(&mut e, 4);
    let png = levels[0].picture.clone().expect("the picture bullet's image");
    assert!(png.0.starts_with(b"\x89PNG"));
    let image = e.document().images.iter().position(|i| *i == png.0).expect("an image");
    assert!(drawn_images(e.document()).contains(&image), "the label draws the image");
    // A new picture bullet for the Wingdings list.
    let mut levels = levels_of(&mut e, 1);
    // Another image: the fixture's with a chunk the decoder skips.
    let mut picture = png.0.clone();
    let end = picture.len() - 12;
    picture.splice(end..end, [0, 0, 0, 0, b't', b'E', b'X', b't', 0xD7, 0x6D, 0x9F, 0x6F]);
    levels[0].picture = Some(Bytes(picture.clone()));
    e.execute(
        Command::SetListDefinition(ListDefinitionChange { levels: levels[..1].to_vec(), apply_to: ListApplyTo::WholeList, name: None }),
        fonts(),
    );
    let at = e.document().images.iter().position(|i| *i == picture).expect("the new image");
    assert!(drawn_images(e.document()).contains(&at), "the new bullet draws");
    let saved = crate::docx::write::save(e.document()).expect("saves");
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(saved.clone())).expect("a package");
    let names: Vec<String> = z.file_names().filter_map(|n| n.ok().map(|n| n.to_string())).collect();
    let mut rels = String::new();
    std::io::Read::read_to_string(&mut z.by_name("word/_rels/numbering.xml.rels").expect("numbering rels"), &mut rels).unwrap();
    assert!(rels.contains("media/ceimage") && names.iter().any(|n| n.starts_with("word/media/ceimage")), "{rels} {names:?}");
    let mut again = crate::editing::Editor::open(&saved, fonts()).expect("reads again");
    assert_eq!(levels_of(&mut again, 1)[0].picture.as_ref().map(|b| &b.0), Some(&picture), "the bullet's image reads again");
    assert_eq!(labels(&again), labels(&e));
}

#[test]
fn an_undecodable_picture_bullet_warns() {
    use std::io::{Read, Write};
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/FX-CUSTOM-LISTS.docx");
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(std::fs::read(path).expect("fixture"))).expect("a package");
    let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for i in 0..z.len() {
        let mut f = z.by_index(i).unwrap();
        let name = f.name().map(|n| n.to_string()).unwrap();
        let mut data = Vec::new();
        f.read_to_end(&mut data).unwrap();
        if name == "word/media/bullet1.png" {
            data = b"not an image".to_vec();
        }
        out.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
        out.write_all(&data).unwrap();
    }
    let bytes = out.finish().unwrap().into_inner();
    let doc = crate::docx::read_editable(&bytes).expect("reads");
    assert_eq!(doc.diagnostics.count(crate::api::warning::WarningCode::PictureBulletUndecodable), 1);
}
