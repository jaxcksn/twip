//! What an edit leaves in the saved package against what the live document shows

use super::super::Editor;
use super::super::commands::{Command, Dir, MoveBy};
use super::{blank_body, fonts, mv};
use crate::api::command::{Hyperlink, PageBordersChange};
use crate::docx::review::Identity;

const SECT: &str = "<w:sectPr><w:pgSz w:w=\"12240\" w:h=\"15840\"/><w:pgMar w:top=\"1440\" w:right=\"1440\" w:bottom=\"1440\" w:left=\"1440\" w:header=\"720\" w:footer=\"720\" w:gutter=\"0\"/><w:pgBorders w:offsetFrom=\"text\"><w:top w:val=\"single\" w:sz=\"4\" w:space=\"1\" w:color=\"auto\"/></w:pgBorders></w:sectPr>";

fn surround(e: &Editor) -> Vec<Option<bool>> {
    e.document().sections.iter().map(|s| s.page_borders.as_ref().map(|b| b.surround_header)).collect()
}

fn reopened(e: &Editor) -> Editor {
    let saved = crate::docx::write::save(e.document()).expect("saves");
    Editor::open(&saved, fonts()).expect("reopens")
}

#[test]
fn page_borders_around_the_header_follow_the_setting_in_every_section() {
    let body = format!(
        "<w:p><w:pPr>{SECT}</w:pPr><w:r><w:t>one</w:t></w:r></w:p><w:p><w:pPr>{SECT}</w:pPr><w:r><w:t>two</w:t></w:r></w:p><w:p><w:r><w:t>three</w:t></w:r></w:p>"
    );
    let mut e = Editor::open(&blank_body(&body), fonts()).expect("opens");
    assert_eq!(e.document().sections.len(), 3);
    e.execute(Command::SetPageBorders(PageBordersChange { surround_header: Some(false), ..Default::default() }), fonts());
    assert_eq!(surround(&e), [Some(false), Some(false), None]);
    assert_eq!(surround(&e), surround(&reopened(&e)));
}

#[test]
fn new_page_borders_take_the_documents_header_setting() {
    let body = format!("<w:p><w:pPr>{SECT}</w:pPr><w:r><w:t>one</w:t></w:r></w:p><w:p><w:r><w:t>two</w:t></w:r></w:p>");
    let mut e = Editor::open(&blank_body(&body), fonts()).expect("opens");
    e.execute(Command::SetPageBorders(PageBordersChange { surround_header: Some(false), ..Default::default() }), fonts());
    e.execute(mv(MoveBy::Story, Dir::Forward, false), fonts());
    let line = crate::api::command::Border {
        style: crate::api::command::BorderStyle::Single,
        width: crate::api::value::Points(0.5),
        color: crate::api::value::AutoOr::Auto,
    };
    e.execute(Command::SetPageBorders(PageBordersChange { top: Some(Some(line)), ..Default::default() }), fonts());
    assert_eq!(surround(&e), [Some(false), Some(false)]);
    assert_eq!(surround(&e), surround(&reopened(&e)));
}

#[test]
fn a_hyperlink_address_ending_in_a_backslash_round_trips() {
    for address in ["C:\\docs\\", "say \"hi\""] {
        let mut e = Editor::open(&blank_body("<w:p><w:r><w:t>x</w:t></w:r></w:p>"), fonts()).expect("opens");
        e.execute(Command::InsertHyperlink(Hyperlink { address: address.into(), text: Some("folder".into()) }), fonts());
        e.complete(fonts());
        for _ in 0..3 {
            e.execute(mv(MoveBy::Char, Dir::Back, false), fonts());
        }
        assert_eq!(e.link_here().expect("the caret is in the link").address, address);
    }
}

#[test]
fn undoing_a_tracked_edit_leaves_no_new_person_in_the_saved_package() {
    let mut e = Editor::open(&blank_body("<w:p><w:r><w:t>x</w:t></w:r></w:p>"), fonts()).expect("opens");
    e.set_author("New Author", Some(Identity { provider: "AD".into(), user_id: "new@example.com".into() }));
    e.execute(Command::SetTrackChanges(true), fonts());
    e.execute(Command::InsertText("y".into()), fonts());
    let people = |e: &Editor| {
        let saved = crate::docx::write::save(e.document()).expect("saves");
        let z = zip::ZipArchive::new(std::io::Cursor::new(saved)).expect("a package");
        z.file_names().filter_map(|n| n.ok()).any(|n| n.contains("people"))
    };
    assert!(people(&e), "the tracked insertion names its author");
    e.execute(Command::Undo, fonts());
    assert!(!people(&e));
}

#[test]
fn page_borders_set_without_a_shadow_keep_each_edges_own() {
    let edge = |side: &str, shadow: bool| {
        format!(r#"<w:{side} w:val="single" w:sz="4" w:space="24" w:color="auto"{}/>"#, if shadow { r#" w:shadow="1""# } else { "" })
    };
    let sect = format!(
        r#"<w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/><w:pgBorders w:offsetFrom="page">{}{}</w:pgBorders></w:sectPr>"#,
        edge("bottom", false),
        edge("right", true)
    );
    let body = format!("<w:p><w:pPr>{sect}</w:pPr><w:r><w:t>one</w:t></w:r></w:p><w:p><w:r><w:t>two</w:t></w:r></w:p>");
    let mut e = Editor::open(&blank_body(&body), fonts()).expect("opens");
    let shadows = |e: &Editor| {
        e.document().sections[0].page_borders.as_ref().map(|b| (b.bottom.as_ref().map(|x| x.shadow), b.right.as_ref().map(|x| x.shadow)))
    };
    assert_eq!(shadows(&e), Some((Some(false), Some(true))));
    e.execute(Command::SetPageBorders(PageBordersChange { surround_header: Some(false), ..Default::default() }), fonts());
    assert_eq!(shadows(&e), Some((Some(false), Some(true))), "neither edge's shadow given");
}
