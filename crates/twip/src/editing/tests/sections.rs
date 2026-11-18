//! Multi-section pages

use super::super::Editor;
use super::super::commands::{Dir, MoveBy};
use super::super::input::PointerKind;
use super::super::keymap::Mods;
use super::super::position::{Position, StoryId};
use super::super::selection::Selection;
use super::super::text::UnitText;
use super::{fonts, mv, open};
use crate::layout::{ParaId, Unit};

fn fixture() -> Editor {
    let mut e = open("FX-SECTIONS.docx");
    e.complete(fonts());
    e
}

fn text_of(e: &Editor, p: ParaId) -> String {
    e.index().para(e.document(), p).map(|q| UnitText::of(q).text).unwrap_or_default()
}

fn caret_text(e: &Editor) -> String {
    e.selection().focus().and_then(|p| p.para_offset()).map(|(p, _)| text_of(e, p)).unwrap_or_default()
}

fn section(text: &str) -> Option<u32> {
    text.strip_prefix('S')?.split(' ').next()?.parse().ok()
}

fn para(e: &Editor, prefix: &str) -> ParaId {
    super::texts_ids(e).into_iter().find(|&p| text_of(e, p).starts_with(prefix)).unwrap_or_else(|| panic!("no paragraph {prefix}"))
}

fn body_lines(e: &mut Editor) -> Vec<(ParaId, f64, f64, f64, f64)> {
    let (_, g) = e.page_geometries().into_iter().next().expect("a page");
    g.lines
        .iter()
        .filter(|l| !l.repeat && g.stories[l.story].story == StoryId::Body)
        .filter_map(|l| Some((l.stops[0].para, l.stops.first()?.x, l.stops.last()?.x, l.top, l.bottom)))
        .collect()
}

fn click(e: &mut Editor, x: f64, y: f64) {
    let (dx, dy) = e.stack().to_doc(0, x, y).expect("on the stack");
    for kind in [PointerKind::Down, PointerKind::Up] {
        e.pointer_input(kind, dx, dy, 1, Mods::default(), fonts());
    }
}

#[test]
fn sections_fixture_has_four_sections_on_one_page() {
    let mut e = fixture();
    let seen: std::collections::BTreeSet<u32> = body_lines(&mut e).iter().filter_map(|l| section(&text_of(&e, l.0))).collect();
    assert_eq!(seen.into_iter().collect::<Vec<_>>(), vec![1, 2, 3, 4], "every section's lines are on page 1");
}

#[test]
fn sections_click_reaches_each_section() {
    let mut e = fixture();
    for (p, x0, x1, top, bottom) in body_lines(&mut e) {
        click(&mut e, (x0 + x1) / 2.0, (top + bottom) / 2.0);
        let got = e.selection().focus().and_then(|q| q.para_offset()).map(|(q, _)| q);
        assert_eq!(got, Some(p), "a click on a line of {:?} landed in {:?}", text_of(&e, p), caret_text(&e));
    }
}

#[test]
fn sections_click_blank_space_picks_nearest_line() {
    let mut e = fixture();
    let lines = body_lines(&mut e);
    let right = lines.iter().map(|l| l.2).fold(f64::MIN, f64::max);
    for n in 1..=4 {
        let own: Vec<_> = lines.iter().filter(|l| section(&text_of(&e, l.0)) == Some(n)).collect();
        let (top, bottom) = own.iter().fold((f64::MAX, f64::MIN), |(t, b), l| (t.min(l.3), b.max(l.4)));
        click(&mut e, right + 10.0, (top + bottom) / 2.0);
        assert_eq!(section(&caret_text(&e)), Some(n), "beside section {n}");
    }
    let page_h = e.page_geometries()[0].1.h;
    click(&mut e, lines[0].1 + 20.0, page_h - 10.0);
    assert_eq!(section(&caret_text(&e)), Some(4), "below the last section");
}

fn walk(e: &mut Editor, by: MoveBy, dir: Dir) -> Vec<u32> {
    let mut seen = vec![section(&caret_text(e)).expect("in a section")];
    for _ in 0..300 {
        let before = e.selection();
        e.execute(mv(by, dir, false), fonts());
        if e.selection() == before {
            break;
        }
        if let Some(s) = section(&caret_text(e))
            && seen.last() != Some(&s)
        {
            seen.push(s);
        }
    }
    seen
}

#[test]
fn sections_down_up_cross_boundaries() {
    let mut e = fixture();
    let first = para(&e, "S1 P1");
    e.set_selection(Selection::Caret(Position::text(StoryId::Body, first, Unit(0))));
    assert_eq!(walk(&mut e, MoveBy::Line, Dir::Forward), vec![1, 2, 3, 4], "Down visits every section in order");
    assert_eq!(walk(&mut e, MoveBy::Line, Dir::Back), vec![4, 3, 2, 1], "Up returns through every section");
}

#[test]
fn sections_left_right_cross_boundaries() {
    let mut e = fixture();
    let last = para(&e, "S1 P4");
    let first = para(&e, "S2 P1");
    let end = e.index().para(e.document(), last).map(|q| UnitText::of(q).len()).expect("S1 P4");
    e.set_selection(Selection::Caret(Position::text(StoryId::Body, last, end)));
    e.execute(mv(MoveBy::Char, Dir::Forward, false), fonts());
    assert_eq!(e.selection().focus().and_then(|p| p.para_offset()), Some((first, Unit(0))), "Right steps into section 2");
    e.execute(mv(MoveBy::Char, Dir::Back, false), fonts());
    assert_eq!(e.selection().focus().and_then(|p| p.para_offset()), Some((last, end)), "Left steps back");
    assert_eq!(walk(&mut e, MoveBy::Para, Dir::Forward), vec![1, 2, 3, 4], "Ctrl+Down crosses every section");
}

#[test]
fn sections_shift_down_extends_across() {
    let mut e = fixture();
    let last = para(&e, "S1 P4");
    e.set_selection(Selection::Caret(Position::text(StoryId::Body, last, Unit(0))));
    e.execute(mv(MoveBy::Line, Dir::Forward, true), fonts());
    let s = e.selection();
    assert!(!s.is_collapsed(), "a range");
    assert_eq!(section(&caret_text(&e)), Some(2), "the range's focus is in section 2");
}

#[test]
fn sections_drag_across() {
    let mut e = fixture();
    let lines = body_lines(&mut e);
    let in_section = |n: u32| *lines.iter().find(|l| section(&text_of(&e, l.0)) == Some(n)).expect("a line");
    let (a, b) = (in_section(1), in_section(3));
    let stack = e.stack();
    let (ax, ay) = stack.to_doc(0, a.1 + 2.0, (a.3 + a.4) / 2.0).expect("on the stack");
    let (bx, by) = stack.to_doc(0, b.1 + 2.0, (b.3 + b.4) / 2.0).expect("on the stack");
    e.pointer_input(PointerKind::Down, ax, ay, 1, Mods::default(), fonts());
    e.pointer_input(PointerKind::Move, bx, by, 0, Mods::default(), fonts());
    e.pointer_input(PointerKind::Up, bx, by, 0, Mods::default(), fonts());
    let s = e.selection();
    let anchor = s.anchor().and_then(|p| p.para_offset()).map(|(p, _)| text_of(&e, p)).unwrap_or_default();
    assert_eq!((section(&anchor), section(&caret_text(&e))), (Some(1), Some(3)), "the drag spans sections 1 to 3");
}

#[test]
fn a_section_break_splits_its_section_and_saves_a_sectpr_of_its_own() {
    use super::super::commands::{Command, Outcome};
    use crate::api::command::SectionBreakKind;
    use crate::docx::SectionBreak;
    let mut e = fixture();
    let starts = |e: &Editor| e.document().sections.iter().map(|s| s.break_kind).collect::<Vec<_>>();
    let before = starts(&e);
    let texts = |e: &Editor| super::texts(e);
    // Mid-paragraph in the first section, and at the end of the paragraph holding its `w:sectPr`.
    let p2 = super::para_id(&e, texts(&e).iter().position(|t| t.starts_with("S1 P2")).expect("S1 P2"));
    e.select_range(p2, Unit(5), Unit(5), fonts());
    assert!(matches!(e.execute(Command::InsertSectionBreak(SectionBreakKind::Continuous), fonts()), Outcome::Changed(_)));
    assert_eq!(starts(&e).len(), before.len() + 1);
    assert_eq!(starts(&e)[..2], [before[0], SectionBreak::Continuous]);
    assert!(texts(&e).iter().any(|t| t == "S1 P2"), "{:?}", texts(&e));
    let p4 = super::para_id(&e, texts(&e).iter().position(|t| t.starts_with("S1 P4")).expect("S1 P4"));
    let end = Unit(texts(&e).iter().find(|t| t.starts_with("S1 P4")).map_or(0, |t| t.chars().count() as u32));
    e.select_range(p4, end, end, fonts());
    assert!(matches!(e.execute(Command::InsertSectionBreak(SectionBreakKind::NextPage), fonts()), Outcome::Changed(_)));
    let saved = crate::docx::write::save(e.document()).expect("saves");
    let again = Editor::open(&saved, fonts()).expect("reopens");
    assert_eq!(starts(&again), starts(&e));
    assert_eq!(texts(&again), texts(&e));
    let (a, b) = (&again.document().sections[0], &e.document().sections[0]);
    assert_eq!((a.page_w, a.top, a.columns.num), (b.page_w, b.top, b.columns.num));
    let xml = crate::docx::package::read(&saved, &[], "word/document.xml").expect("reads").expect("the main part");
    assert_eq!(xml.matches("<w:sectPr").count(), before.len() + 2, "{xml}");
    // Undone, the document saves as it was read.
    e.execute(Command::Undo, fonts());
    e.execute(Command::Undo, fonts());
    assert_eq!(starts(&e), before);
    let back = Editor::open(&crate::docx::write::save(e.document()).expect("saves"), fonts()).expect("reopens");
    assert_eq!(starts(&back), before);
}

#[test]
fn a_drop_cap_is_made_saved_as_a_framed_paragraph_and_put_back() {
    use super::super::commands::{Command, Outcome};
    use crate::api::command::{DropCap, DropCapPosition};
    use crate::api::value::Points;
    let mut e = fixture();
    let first = super::texts(&e)[0].clone();
    let p = super::para_id(&e, 0);
    e.select_range(p, Unit(2), Unit(2), fonts());
    let dropped = DropCap { position: DropCapPosition::Dropped, font: Some("Georgia".into()), lines: Some(3), distance: Some(Points(4.0)) };
    assert!(matches!(e.execute(Command::SetDropCap(dropped.clone()), fonts()), Outcome::Changed(_)));
    assert_eq!(e.drop_cap(), Some(dropped.clone()));
    assert!(super::texts(&e).iter().any(|t| *t == first[1..]), "the letter left its paragraph");
    let saved = crate::docx::write::save(e.document()).expect("saves");
    if let Some(d) = std::env::var_os("TWIP_DROP_CAP_OUT") {
        std::fs::write(std::path::Path::new(&d).join("drop-cap.docx"), &saved).expect("writes");
    }
    let xml = crate::docx::package::read(&saved, &[], "word/document.xml").expect("reads").expect("the main part");
    let frame = xml.find("w:dropCap=\"drop\"").expect("a framed paragraph");
    assert!(frame < xml.find(&first[1..12]).expect("the paragraph's text"), "before its paragraph");
    let mut again = Editor::open(&saved, fonts()).expect("reopens");
    let q = super::para_id(&again, 0);
    again.select_range(q, Unit(1), Unit(1), fonts());
    assert_eq!(again.drop_cap(), Some(dropped));
    // In the margin, then back in the text.
    let margin = DropCap { position: DropCapPosition::InMargin, font: None, lines: Some(2), distance: None };
    assert!(matches!(e.execute(Command::SetDropCap(margin), fonts()), Outcome::Changed(_)));
    assert_eq!(e.drop_cap().map(|d| (d.position, d.lines)), Some((DropCapPosition::InMargin, Some(2))));
    let none = DropCap { position: DropCapPosition::None, font: None, lines: None, distance: None };
    assert!(matches!(e.execute(Command::SetDropCap(none.clone()), fonts()), Outcome::Changed(_)));
    assert_eq!(super::texts(&e)[0], first, "the letter is back");
    assert!(matches!(e.execute(Command::SetDropCap(none), fonts()), Outcome::Unchanged(_)));
}

#[test]
fn linking_a_section_to_the_previous_keeps_a_header_part_another_section_shares() {
    use super::super::commands::{Command, Outcome};
    let blank = crate::docx::blank::blank_docx();
    let read = |name: &str| crate::docx::package::read(&blank, &[], name).expect("reads").expect(name);
    let rels = read("word/_rels/document.xml.rels").replace(
        "</Relationships>",
        r#"<Relationship Id="rIdH" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/header" Target="header1.xml"/></Relationships>"#,
    );
    let types = read("[Content_Types].xml").replace(
        "</Types>",
        r#"<Override PartName="/word/header1.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml"/></Types>"#,
    );
    let ns = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships""#;
    let header = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:hdr {ns}><w:p><w:r><w:t>Shared</w:t></w:r></w:p></w:hdr>"#);
    let r = r#"<w:headerReference w:type="default" r:id="rIdH"/>"#;
    let document = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document {ns}><w:body><w:p><w:pPr><w:sectPr/></w:pPr><w:r><w:t>One</w:t></w:r></w:p><w:p><w:pPr><w:sectPr>{r}</w:sectPr></w:pPr><w:r><w:t>Two</w:t></w:r></w:p><w:p><w:r><w:t>Three</w:t></w:r></w:p><w:sectPr>{r}</w:sectPr></w:body></w:document>"#
    );
    let bytes = crate::docx::write::repackage(
        &blank,
        &[
            ("word/_rels/document.xml.rels".into(), rels.into_bytes()),
            ("[Content_Types].xml".into(), types.into_bytes()),
            ("word/header1.xml".into(), header.into_bytes()),
            ("word/document.xml".into(), document.into_bytes()),
        ],
        zip::CompressionMethod::Deflated,
    )
    .expect("repackages");
    let mut e = Editor::open(&bytes, fonts()).expect("opens");
    let two = para(&e, "Two");
    e.select_range(two, Unit(0), Unit(0), fonts());
    let link = serde_json::from_value(serde_json::json!({ "kind": "header", "pages": "default", "linked": true })).expect("a link");
    assert!(matches!(e.execute(Command::SetHeaderFooterLinked(link), fonts()), Outcome::Changed(_)));
    let doc = e.document();
    assert!(doc.sections[1].headers[0].is_none(), "the second section takes the first's");
    let name = doc.sections[2].headers[0].clone().expect("the third keeps its header");
    assert!(doc.stories.contains_key(&name), "the part the third shows stays");
    let again = Editor::open(&crate::docx::write::save(e.document()).expect("saves"), fonts()).expect("reopens");
    assert!(again.document().sections[2].headers[0].is_some() && again.document().sections[1].headers[0].is_none());
}
