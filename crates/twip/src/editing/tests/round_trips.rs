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
    let saved = crate::docx::write::save(e.document(), e.docx()).expect("saves");
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
        space: None,
        shadow: None,
    };
    e.execute(Command::SetPageBorders(PageBordersChange { top: Some(Some(line)), ..Default::default() }), fonts());
    assert_eq!(surround(&e), [Some(false), Some(false)]);
    assert_eq!(surround(&e), surround(&reopened(&e)));
}

#[test]
fn a_hyperlink_address_ending_in_a_backslash_round_trips() {
    for address in ["C:\\docs\\", "say \"hi\""] {
        let mut e = Editor::open(&blank_body("<w:p><w:r><w:t>x</w:t></w:r></w:p>"), fonts()).expect("opens");
        e.execute(Command::InsertHyperlink(Hyperlink { address: address.into(), text: Some("folder".into()), ..Default::default() }), fonts());
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
        let saved = crate::docx::write::save(e.document(), e.docx()).expect("saves");
        let z = zip::ZipArchive::new(std::io::Cursor::new(saved)).expect("a package");
        z.file_names().filter_map(|n| n.ok()).any(|n| n.contains("people"))
    };
    assert!(people(&e), "the tracked insertion names its author");
    e.execute(Command::Undo(Default::default()), fonts());
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

/// Each section's `w:pgBorders` in the saved main part, as written.
fn saved_page_borders(e: &Editor) -> Vec<String> {
    use std::io::Read;
    let saved = crate::docx::write::save(e.document(), e.docx()).expect("saves");
    let mut xml = String::new();
    zip::ZipArchive::new(std::io::Cursor::new(saved)).expect("a package").by_name("word/document.xml").unwrap().read_to_string(&mut xml).unwrap();
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(k) = xml[at..].find("<w:pgBorders").map(|k| at + k) {
        let end = xml[k..].find("</w:pgBorders>").map_or(xml.len(), |e| k + e + "</w:pgBorders>".len());
        out.push(xml[k..end].to_string());
        at = end;
    }
    out
}

fn changed(o: super::super::commands::Outcome) -> bool {
    matches!(o, super::super::commands::Outcome::Changed(_))
}

const TWO: &str = "<w:p><w:r><w:t>one</w:t></w:r></w:p><w:p><w:r><w:t>two</w:t></w:r></w:p>";

#[test]
fn page_borders_set_on_every_edge_are_written_from_the_page() {
    use crate::api::command::{Border, BorderStyle, PageBorderScope};
    use crate::api::drawing::ColourValue;
    use crate::api::value::{AutoOr, Color, Points};
    let wave =
        Border { style: BorderStyle::Wave, width: Points(0.75), color: AutoOr::Value(ColourValue::rgb(Color(0x7030A0))), space: None, shadow: None };
    let all = PageBordersChange {
        top: Some(Some(wave.clone())),
        bottom: Some(Some(wave.clone())),
        left: Some(Some(wave.clone())),
        right: Some(Some(wave)),
        apply_to: PageBorderScope::WholeDocument,
        ..PageBordersChange::default()
    };
    let mut e = Editor::open(&blank_body(TWO), fonts()).expect("opens");
    assert!(changed(e.execute(Command::SetPageBorders(all.clone()), fonts())));
    let edge = |side: &str| format!(r#"<w:{side} w:val="wave" w:sz="6" w:space="24" w:color="7030A0"/>"#);
    let want = format!(r#"<w:pgBorders w:offsetFrom="page">{}{}{}{}</w:pgBorders>"#, edge("top"), edge("left"), edge("bottom"), edge("right"));
    assert_eq!(saved_page_borders(&e), std::slice::from_ref(&want));
    assert!(!changed(e.execute(Command::SetPageBorders(all.clone()), fonts())), "set again");
    let mut e = reopened(&e);
    assert!(!changed(e.execute(Command::SetPageBorders(all), fonts())), "set again on the borders as saved");
    assert_eq!(saved_page_borders(&e), [want]);
}

#[test]
fn page_borders_with_theme_colours_rounding_and_placement_are_written_as_set() {
    use crate::api::command::{BorderStyle, PageBorderDisplay, PageBorderOffset};
    use crate::api::drawing::{ColourTransform, ColourValue, ThemeColour, TransformKind};
    use crate::api::value::{AutoOr, Color, Points};
    use crate::editing::readback::BorderLine;
    use crate::editing::section::PageBordersEdit;
    let mut tint = ColourValue::theme(ThemeColour::Accent1);
    tint.transforms.push(ColourTransform { kind: TransformKind::Tint, value: 0.4 });
    let mut shade = ColourValue::theme(ThemeColour::Text2);
    shade.transforms.push(ColourTransform { kind: TransformKind::Shade, value: 0.75 });
    let l = |style, width: f64, color, space: f64, shadow| BorderLine { style, width: Points(width), color, space: Points(space), shadow };
    let edges = PageBordersEdit {
        top: Some(Some(l(BorderStyle::Double, 1.5, AutoOr::Value(tint), 10.0, true))),
        bottom: Some(Some(l(BorderStyle::Single, 0.5, AutoOr::Auto, 20.0, false))),
        left: Some(Some(l(BorderStyle::DotDash, 0.3, AutoOr::Value(ColourValue::rgb(Color(0x123456))), 3.6, false))),
        right: Some(Some(l(BorderStyle::ThickThinLargeGap, 6.0, AutoOr::Value(shade), 31.0, true))),
        offset_from: Some(PageBorderOffset::Text),
        display: Some(PageBorderDisplay::NotFirstPage),
        behind_text: Some(true),
        ..PageBordersEdit::default()
    };
    let mut e = Editor::open(&blank_body(TWO), fonts()).expect("opens");
    assert!(changed(e.set_page_border_edges(&edges, fonts())));
    let edges_xml = concat!(
        r#"<w:top w:val="double" w:sz="12" w:space="10" w:color="83CBEB" w:themeColor="accent1" w:themeTint="66" w:shadow="1"/>"#,
        r#"<w:left w:val="dotDash" w:sz="2" w:space="4" w:color="123456"/>"#,
        r#"<w:bottom w:val="single" w:sz="4" w:space="20" w:color="auto"/>"#,
        r#"<w:right w:val="thickThinLargeGap" w:sz="48" w:space="31" w:color="0A1E31" w:themeColor="text2" w:themeShade="BF" w:shadow="1"/>"#,
    );
    assert_eq!(saved_page_borders(&e), [format!(r#"<w:pgBorders w:display="notFirstPage" w:zOrder="back">{edges_xml}</w:pgBorders>"#)]);
    let placed = PageBordersEdit {
        display: Some(PageBorderDisplay::FirstPage),
        offset_from: Some(PageBorderOffset::Page),
        behind_text: Some(false),
        ..PageBordersEdit::default()
    };
    assert!(changed(e.set_page_border_edges(&placed, fonts())));
    assert_eq!(saved_page_borders(&e), [format!(r#"<w:pgBorders w:offsetFrom="page" w:display="firstPage">{edges_xml}</w:pgBorders>"#)]);
}

#[test]
fn page_borders_set_in_one_section_rewrite_its_borders_alone() {
    use crate::api::command::{Border, BorderStyle, PageBorderOffset};
    use crate::api::value::{AutoOr, Points};
    let body = format!(
        "<w:p><w:pPr>{SECT}</w:pPr><w:r><w:t>one</w:t></w:r></w:p><w:p><w:pPr>{SECT}</w:pPr><w:r><w:t>two</w:t></w:r></w:p><w:p><w:r><w:t>three</w:t></w:r></w:p>"
    );
    let read = r#"<w:pgBorders w:offsetFrom="text"><w:top w:val="single" w:sz="4" w:space="1" w:color="auto"/></w:pgBorders>"#;
    let mut e = Editor::open(&blank_body(&body), fonts()).expect("opens");
    let left = Border { style: BorderStyle::Dotted, width: Points(1.0), color: AutoOr::Auto, space: None, shadow: None };
    assert!(changed(
        e.execute(Command::SetPageBorders(PageBordersChange { left: Some(Some(left)), shadow: Some(true), ..Default::default() }), fonts())
    ));
    let set = r#"<w:pgBorders><w:top w:val="single" w:sz="4" w:space="1" w:color="auto"/><w:left w:val="dotted" w:sz="8" w:space="4" w:color="auto"/></w:pgBorders>"#;
    assert_eq!(saved_page_borders(&e), [set, read]);
    // Set to what they are, written otherwise: a change that saves them as read
    let mut e = Editor::open(&blank_body(&body), fonts()).expect("opens");
    let same = PageBordersChange { offset_from: Some(PageBorderOffset::Text), ..PageBordersChange::default() };
    assert!(changed(e.execute(Command::SetPageBorders(same), fonts())));
    assert_eq!(saved_page_borders(&e), [read, read]);
}

#[test]
fn page_borders_edited_lose_what_isnt_modelled_and_kept_keep_it() {
    use crate::api::command::PageBorderScope;
    let read = concat!(
        r#"<w:pgBorders w:offsetFrom="page" w:foo="1"><w:top w:val="apples" w:sz="20" w:space="24" w:color="auto" r:id="rId9"/>"#,
        r#"<w:left w:val="single" w:sz="4" w:space="24" w:color="FF0000" w:themeColor="accent2" w:shadow="0"/>"#,
        r#"<w:bottom w:val="single" w:sz="4.5" w:space="7.6" w:color="00FF00"/></w:pgBorders>"#
    );
    let sect = format!(
        r#"<w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/>{read}</w:sectPr>"#
    );
    let body = format!("<w:p><w:pPr>{sect}</w:pPr><w:r><w:t>one</w:t></w:r></w:p><w:p><w:r><w:t>two</w:t></w:r></w:p>");
    let footer = || Command::SetPageBorders(PageBordersChange { surround_footer: Some(false), ..Default::default() });
    let mut e = Editor::open(&blank_body(&body), fonts()).expect("opens");
    assert_eq!(saved_page_borders(&e), [read]);
    assert!(changed(e.execute(footer(), fonts())));
    let set = concat!(
        r#"<w:pgBorders w:offsetFrom="page"><w:top w:val="apples" w:sz="20" w:space="24" w:color="auto"/>"#,
        r#"<w:left w:val="single" w:sz="4" w:space="24" w:color="E97132" w:themeColor="accent2"/>"#,
        r#"<w:bottom w:val="single" w:sz="5" w:space="8" w:color="00FF00"/></w:pgBorders>"#
    );
    assert_eq!(saved_page_borders(&e), [set]);
    let mut e = Editor::open(&blank_body(&body), fonts()).expect("opens");
    e.execute(mv(MoveBy::Story, Dir::Forward, false), fonts());
    assert!(changed(e.execute(footer(), fonts())), "in the other section");
    assert_eq!(saved_page_borders(&e), [read]);
    let none = PageBordersChange {
        top: Some(None),
        left: Some(None),
        bottom: Some(None),
        right: Some(None),
        apply_to: PageBorderScope::WholeDocument,
        ..Default::default()
    };
    assert!(changed(e.execute(Command::SetPageBorders(none), fonts())));
    assert!(saved_page_borders(&e).is_empty(), "removed");
}

#[test]
fn typed_page_borders_with_no_xml_behind_them_are_written() {
    use crate::docx::page_borders::PageBordersDirect;
    use crate::format::{Color, PageBorderOffset, PageBorderProps, TextBorder};
    let (mut doc, mut docx) = crate::docx::read_editable(&blank_body(TWO)).expect("reads");
    let mut page = doc.sections[0].page();
    let edge =
        TextBorder { style: crate::document::line::BorderStyle::Dotted, width: 1.0, space: 3.0, shadow: false, color: Some(Color::Rgb(0x00B050)) };
    let typed = PageBorderProps { left: Some(edge), right: Some(edge), offset_from: Some(PageBorderOffset::Page), ..PageBorderProps::default() };
    page.page_borders_direct = PageBordersDirect::typed(Some(typed));
    page.page_borders = Some(std::rc::Rc::new(crate::docx::PageBorders { offset_from_page: true, ..Default::default() }));
    doc.sections[0].set_page(page);
    docx.edited_sections.insert(0);
    let saved = crate::docx::write::save(&doc, &docx).expect("saves");
    let e = Editor::open(&saved, fonts()).expect("reopens");
    let edge = |side: &str| format!(r#"<w:{side} w:val="dotted" w:sz="8" w:space="3" w:color="00B050"/>"#);
    assert_eq!(saved_page_borders(&e), [format!(r#"<w:pgBorders w:offsetFrom="page">{}{}</w:pgBorders>"#, edge("left"), edge("right"))]);
    let b = e.document().sections[0].page_borders.as_deref().expect("drawn");
    assert!(b.offset_from_page && b.top.is_none() && b.left.as_ref().is_some_and(|l| l.width == 1.0 && l.color == Some(0x00B050)));
}
