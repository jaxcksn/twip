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
    e.execute(Command::Undo(Default::default()), fonts());
    e.execute(Command::Undo(Default::default()), fonts());
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

mod page_setup {
    use super::super::super::Editor;
    use super::super::super::commands::{Command, Outcome};
    use super::super::{blank_body, blank_with, fonts, open, para_id};
    use crate::api::command::{ColumnBoundaryMove, ColumnWidth, Columns, Margins, PaperSize, SectionBreakKind};
    use crate::api::value::{Orientation, Points};
    use crate::layout::Unit;

    fn blank() -> Editor {
        Editor::open(&blank_with("x"), fonts()).expect("opens")
    }

    fn run(e: &mut Editor, c: Command) -> Outcome {
        e.execute(c, fonts())
    }

    fn changed(o: &Outcome) -> bool {
        matches!(o, Outcome::Changed(_))
    }

    fn margins(e: &Editor, i: usize) -> (f64, f64, f64, f64) {
        let s = &e.document().sections[i];
        (s.top, s.right, s.bottom, s.left)
    }

    fn uneven() -> Margins {
        Margins { top: Some(Points(90.0)), bottom: Some(Points(54.0)), left: Some(Points(72.0)), right: Some(Points(36.0)), ..Default::default() }
    }

    #[test]
    fn turning_to_landscape_and_back_restores_the_margins() {
        let mut e = blank();
        assert!(changed(&run(&mut e, Command::SetMargins(uneven()))));
        let before = margins(&e, 0);
        run(&mut e, Command::SetOrientation(crate::api::command::SectionValue::here(Orientation::Landscape)));
        assert_eq!(margins(&e, 0), (72.0, 90.0, 36.0, 54.0), "the margins turn with the page");
        run(&mut e, Command::SetOrientation(crate::api::command::SectionValue::here(Orientation::Portrait)));
        assert_eq!(margins(&e, 0), before);
    }

    #[test]
    fn orientation_leaves_a_square_page_alone() {
        let mut e = blank();
        run(&mut e, Command::SetPaperSize(PaperSize { width: Points(600.0), height: Points(600.0), apply_to: None }));
        run(&mut e, Command::SetMargins(uneven()));
        for o in [Orientation::Landscape, Orientation::Landscape, Orientation::Portrait] {
            let out = run(&mut e, Command::SetOrientation(crate::api::command::SectionValue::here(o)));
            assert!(!changed(&out), "{o:?}: {out:?}");
        }
    }

    #[test]
    fn page_setup_covers_every_selected_section() {
        let mut e = open("FX-SECTIONS.docx");
        e.complete(fonts());
        let n = e.document().sections.len();
        assert!(n >= 3);
        run(&mut e, Command::SelectAll);
        assert!(changed(&run(&mut e, Command::SetMargins(Margins { left: Some(Points(100.0)), ..Default::default() }))));
        let lefts: Vec<f64> = (0..n).map(|i| margins(&e, i).3).collect();
        assert!(lefts.iter().all(|&l| l == 100.0), "{lefts:?}");
        run(&mut e, Command::SetOrientation(crate::api::command::SectionValue::here(Orientation::Landscape)));
        assert!(e.document().sections.iter().all(|s| s.page_w > s.page_h), "every section turns");
        run(&mut e, Command::Undo(Default::default()));
        assert!(e.document().sections.iter().all(|s| s.page_w < s.page_h), "in one undo step");
    }

    #[test]
    fn a_column_boundary_moves_in_the_carets_section_only() {
        let mut e = open("FX-SECTIONS.docx");
        e.complete(fonts());
        run(&mut e, Command::SelectAll);
        run(&mut e, Command::SetColumns(Columns { count: 2, spacing: None, separator: None, widths: None, apply_to: None, start_new_column: None }));
        let focus = e.focus_section();
        let s = &e.document().sections[focus];
        let to = s.left + s.gutter + 120.0;
        assert!(changed(&run(&mut e, Command::MoveColumnBoundary(ColumnBoundaryMove { column: 0, to: Points(to) }))));
        for (i, s) in e.document().sections.iter().enumerate() {
            assert_eq!(s.columns.equal, i != focus, "section {i} of {}", e.document().sections.len());
        }
    }

    #[test]
    fn columns_take_one_width_each() {
        let mut e = blank();
        let w = |width| ColumnWidth { width: Points(width), spacing: Points(10.0) };
        let cols = |count, widths| {
            Command::SetColumns(Columns { count, spacing: None, separator: None, widths: Some(widths), apply_to: None, start_new_column: None })
        };
        assert!(!changed(&run(&mut e, cols(3, vec![w(100.0), w(100.0)]))), "three columns, two widths");
        assert!(!changed(&run(&mut e, cols(2, vec![w(-5.0), w(100.0)]))), "a negative width");
        assert!(!changed(&run(&mut e, cols(2, vec![w(400.0), w(400.0)]))), "wider than the text");
        assert!(changed(&run(&mut e, cols(2, vec![w(200.0), w(200.0)]))));
        let c = &e.document().sections[0].columns;
        assert_eq!((c.num, c.cols.len(), c.equal), (2, 2, false));
    }

    #[test]
    fn margins_must_fit_the_page() {
        let mut e = blank();
        let tall = Margins { top: Some(Points(700.0)), bottom: Some(Points(700.0)), ..Default::default() };
        assert!(!changed(&run(&mut e, Command::SetMargins(tall))), "taller than the page");
        run(&mut e, Command::SetMargins(Margins { left: Some(Points(72.0)), right: Some(Points(72.0)), ..Default::default() }));
        let out = run(&mut e, Command::SetPaperSize(PaperSize { width: Points(140.0), height: Points(792.0), apply_to: None }));
        assert!(!changed(&out), "a page narrower than its margins: {out:?}");
        let s = &e.document().sections[0];
        assert!(s.page_w - s.left - s.right - s.gutter > 0.0, "{} wide, margins {} {}", s.page_w, s.left, s.right);
    }

    #[test]
    fn a_top_gutter_takes_from_the_page_height() {
        use crate::api::command::MultiplePages;
        let pt = |v| Some(Points(v));
        let margins =
            |side, ends, gutter| Margins { left: pt(side), right: pt(side), top: pt(ends), bottom: pt(ends), gutter: pt(gutter), apply_to: None };
        let small = Command::SetPaperSize(PaperSize { width: Points(200.0), height: Points(300.0), apply_to: None });
        for (m, fits) in [(margins(60.0, 100.0, 90.0), true), (margins(50.0, 120.0, 70.0), false)] {
            let mut e = blank();
            assert!(changed(&run(&mut e, Command::SetMultiplePages(MultiplePages { mode: None, gutter_at_top: Some(true) }))));
            assert!(changed(&run(&mut e, Command::SetMargins(m))));
            let out = run(&mut e, small.clone());
            assert_eq!(changed(&out), fits, "{out:?}");
        }
        let mut e = blank();
        assert!(changed(&run(&mut e, Command::SetMultiplePages(MultiplePages { mode: None, gutter_at_top: Some(true) }))));
        assert!(changed(&run(&mut e, Command::SetMargins(margins(300.0, 72.0, 20.0)))), "a top gutter leaves the width");
        let out = run(&mut e, Command::SetMargins(margins(72.0, 380.0, 40.0)));
        assert!(!changed(&out), "but takes from the height: {out:?}");
    }

    #[test]
    fn a_section_break_leaves_the_page_number_restart_in_the_first_half() {
        let xml = r#"<w:p><w:r><w:t>A1</w:t></w:r></w:p><w:p><w:pPr><w:sectPr><w:pgNumType w:start="5"/></w:sectPr></w:pPr><w:r><w:t>A2 text</w:t></w:r></w:p><w:p><w:r><w:t>B1</w:t></w:r></w:p>"#;
        let mut e = Editor::open(&blank_body(xml), fonts()).expect("opens");
        assert_eq!(e.document().sections[0].pg_start, Some(5));
        let p = para_id(&e, 1);
        e.select_range(p, Unit(2), Unit(2), fonts());
        let out = run(&mut e, Command::InsertSectionBreak(SectionBreakKind::NextPage));
        assert!(changed(&out), "{out:?}");
        let s = &e.document().sections;
        assert_eq!((s[0].pg_start, s[1].pg_start), (Some(5), None));
    }
}

#[test]
fn columns_apply_to_the_selected_text_or_from_this_point_on() {
    use crate::api::command::Columns;
    use crate::editing::section::{ColumnPreset, ColumnsScope};
    let body = "<w:p><w:r><w:t>One</w:t></w:r></w:p><w:p><w:r><w:t>Two</w:t></w:r></w:p><w:p><w:r><w:t>Three</w:t></w:r></w:p>";
    let mut e = Editor::open(&super::blank_body(body), super::fonts()).expect("opens");
    let two = Columns { count: 2, spacing: None, separator: None, widths: None, apply_to: None, start_new_column: None };
    let para = |e: &Editor, k: usize| super::para_id(e, k);
    e.selection = Selection::range(
        Position::text(StoryId::Body, para(&e, 1), Unit(0)),
        Position::text(StoryId::Body, para(&e, 1), Unit(3)),
        crate::editing::selection::Granularity::Char,
    );
    let depth = e.history().undo_depth();
    assert!(matches!(e.set_columns_in(two.clone(), ColumnsScope::SelectedText, false, super::fonts()), super::super::commands::Outcome::Changed(_)));
    let cols: Vec<usize> = e.document().sections.iter().map(|s| s.columns.num).collect();
    assert_eq!(cols, [1, 2, 1], "the selected text alone in two columns");
    assert!(e.document().sections[1..].iter().all(|s| s.break_kind == crate::docx::SectionBreak::Continuous), "continuous breaks");
    assert_eq!(e.history().undo_depth(), depth + 1, "one undo step");
    e.undo_many(true, 1, super::fonts());
    assert_eq!(e.document().sections.len(), 1);
    e.selection = Selection::Caret(Position::text(StoryId::Body, para(&e, 2), Unit(0)));
    assert!(matches!(
        e.set_columns_in(two.clone(), ColumnsScope::ThisPointForward, true, super::fonts()),
        super::super::commands::Outcome::Changed(_)
    ));
    let s = &e.document().sections;
    assert_eq!((s.len(), s[0].columns.num, s[1].columns.num, s[1].break_kind), (2, 1, 2, crate::docx::SectionBreak::NextColumn));
    assert!(
        matches!(e.set_columns_in(two, ColumnsScope::WholeDocument, true, super::fonts()), super::super::commands::Outcome::Refused(_)),
        "a new column only from here on"
    );
    let left = e.column_preset(ColumnPreset::Left);
    let w: Vec<f64> = left.widths.iter().flatten().map(|c| c.width.0).collect();
    assert_eq!(left.count, 2);
    assert!(w[0] < w[1] && (w[0] - (468.0 - 72.0) / 3.0).abs() < 0.01, "{w:?}");
}

fn two_sections() -> Editor {
    let body = r#"<w:p><w:r><w:t>One</w:t></w:r></w:p><w:p><w:pPr><w:sectPr/></w:pPr><w:r><w:t>Two</w:t></w:r></w:p><w:p><w:r><w:t>Three</w:t></w:r></w:p><w:p><w:r><w:t>Four</w:t></w:r></w:p>"#;
    let mut e = Editor::open(&super::blank_body(body), super::fonts()).expect("opens");
    e.complete(super::fonts());
    assert_eq!(e.document().sections.len(), 2);
    e
}

fn select_across(e: &mut Editor, from: (usize, u32), to: (usize, u32)) {
    let at = |e: &Editor, (k, u): (usize, u32)| Position::text(StoryId::Body, super::para_id(e, k), Unit(u));
    e.selection = Selection::range(at(e, from), at(e, to), crate::editing::selection::Granularity::Char);
}

#[test]
fn columns_on_selected_text_reach_every_section_it_spans() {
    use crate::api::command::Columns;
    use crate::editing::commands::Outcome;
    use crate::editing::section::ColumnsScope;
    let two = Columns { count: 2, spacing: None, separator: None, widths: None, apply_to: None, start_new_column: None };
    let cols = |e: &Editor| e.document().sections.iter().map(|s| s.columns.num).collect::<Vec<_>>();
    // From the outer edges: no breaks, both sections change.
    let mut e = two_sections();
    select_across(&mut e, (0, 0), (3, 4));
    assert!(matches!(e.set_columns_in(two.clone(), ColumnsScope::SelectedText, false, super::fonts()), Outcome::Changed(_)));
    assert_eq!(cols(&e), [2, 2]);
    // From inside the first to inside the second: both ends split off.
    let mut e = two_sections();
    select_across(&mut e, (0, 1), (2, 2));
    assert!(matches!(e.set_columns_in(two.clone(), ColumnsScope::SelectedText, false, super::fonts()), Outcome::Changed(_)));
    assert_eq!(cols(&e), [1, 2, 2, 1]);
    let texts: Vec<Vec<String>> = e
        .document()
        .sections
        .iter()
        .map(|s| {
            s.blocks.iter().filter_map(|b| if let crate::docx::Block::Para(p) = b { Some(crate::docx::fields::text(p)) } else { None }).collect()
        })
        .collect();
    assert_eq!(texts, [vec!["O"], vec!["ne", "Two"], vec!["Th"], vec!["ree", "Four"]], "the selected text alone in two columns");
    // Whole paragraphs of the first, up to the second's start: the first alone, split from its own start.
    let mut e = two_sections();
    select_across(&mut e, (0, 1), (2, 0));
    assert!(matches!(e.set_columns_in(two.clone(), ColumnsScope::SelectedText, false, super::fonts()), Outcome::Changed(_)));
    assert_eq!(cols(&e), [1, 2, 1], "no section split off at the second's start");
    // One section keeps to itself.
    let mut e = two_sections();
    select_across(&mut e, (2, 1), (2, 3));
    assert!(matches!(e.set_columns_in(two, ColumnsScope::SelectedText, false, super::fonts()), Outcome::Changed(_)));
    assert_eq!(cols(&e), [1, 1, 2, 1]);
}

#[test]
fn columns_on_selected_text_are_refused_outside_the_body() {
    use crate::api::command::Columns;
    use crate::editing::commands::{Command, Outcome};
    use crate::editing::section::ColumnsScope;
    let hdr = format!(r#"<w:hdr {}><w:p><w:r><w:t>Header</w:t></w:r></w:p></w:hdr>"#, super::empty_stories::NS);
    let bytes = super::empty_stories::build(
        "<w:p><w:r><w:t>Body</w:t></w:r></w:p>",
        r#"<w:headerReference w:type="default" r:id="rIdP0"/>"#,
        &[("header1.xml", "header", "header", &hdr)],
    );
    let mut e = Editor::open(&bytes, super::fonts()).expect("opens");
    e.complete(super::fonts());
    e.execute(Command::EditHeader, super::fonts());
    let Some(end) = e.selection().focus() else { panic!("a caret") };
    assert!(matches!(end.story, StoryId::HeaderFooter(_)));
    let start = Position::text(end.story, end.para_offset().expect("a paragraph").0, Unit(0));
    e.selection = Selection::range(
        start,
        Position::text(end.story, end.para_offset().expect("a paragraph").0, Unit(6)),
        crate::editing::selection::Granularity::Char,
    );
    let before = (e.document().sections.len(), e.history().undo_depth());
    let two = Columns { count: 2, spacing: None, separator: None, widths: None, apply_to: None, start_new_column: None };
    assert!(!matches!(e.set_columns_in(two, ColumnsScope::SelectedText, false, super::fonts()), Outcome::Changed(_)));
    assert_eq!((e.document().sections.len(), e.history().undo_depth()), before);
    assert_eq!(e.document().sections[0].columns.num, 1, "the body's section is untouched");
}

#[test]
fn columns_on_selected_text_ending_inside_a_table_are_refused() {
    use crate::api::command::Columns;
    use crate::editing::commands::Outcome;
    use crate::editing::section::ColumnsScope;
    let row = |t: &str| format!(r#"<w:tr><w:tc><w:p><w:r><w:t>{t}</w:t></w:r></w:p></w:tc></w:tr>"#);
    let body = format!(
        r#"<w:p><w:r><w:t>One</w:t></w:r></w:p><w:tbl><w:tblGrid><w:gridCol w:w="4000"/></w:tblGrid>{}{}</w:tbl><w:p><w:r><w:t>After</w:t></w:r></w:p>"#,
        row("A"),
        row("B")
    );
    let mut e = Editor::open(&super::blank_body(&body), super::fonts()).expect("opens");
    e.complete(super::fonts());
    let table = e.index().table_ids().next().expect("a table");
    let start = Position::text(StoryId::Body, super::para_id(&e, 0), Unit(1));
    e.selection = Selection::range(start, Position::row_end(StoryId::Body, table, 0), crate::editing::selection::Granularity::Char);
    let before = e.document().sections.clone();
    let two = Columns { count: 2, spacing: None, separator: None, widths: None, apply_to: None, start_new_column: None };
    let out = e.set_columns_in(two, ColumnsScope::SelectedText, false, super::fonts());
    assert!(matches!(out, Outcome::Refused(_)), "{out:?}");
    assert_eq!(e.document().sections, before, "nothing split or set");
}

#[test]
fn a_row_end_caret_is_in_its_tables_section() {
    let body = r#"<w:p><w:pPr><w:sectPr/></w:pPr><w:r><w:t>One</w:t></w:r></w:p><w:tbl><w:tblGrid><w:gridCol w:w="4000"/></w:tblGrid><w:tr><w:tc><w:p><w:r><w:t>A</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p><w:r><w:t>After</w:t></w:r></w:p>"#;
    let mut e = Editor::open(&super::blank_body(body), super::fonts()).expect("opens");
    assert_eq!(e.document().sections.len(), 2);
    let table = e.index().table_ids().next().expect("a table");
    e.selection = Selection::Caret(Position::row_end(StoryId::Body, table, 0));
    assert_eq!(e.focus_section(), 1);
}

#[test]
fn a_nested_section_scope_leaves_the_outer_one() {
    use crate::api::services::ApplyTo;
    let body = r#"<w:p><w:pPr><w:sectPr/></w:pPr><w:r><w:t>One</w:t></w:r></w:p><w:p><w:r><w:t>Two</w:t></w:r></w:p>"#;
    let mut e = Editor::open(&super::blank_body(body), super::fonts()).expect("opens");
    let out = e.in_section_scope(
        ApplyTo::WholeDocument,
        "Outer",
        |e| {
            e.in_section_scope(ApplyTo::WholeDocument, "Inner", |_| crate::editing::commands::Outcome::Moved, super::fonts());
            assert_eq!(e.selected_sections(), [0, 1], "still every section");
            crate::editing::commands::Outcome::Moved
        },
        super::fonts(),
    );
    assert!(matches!(out, crate::editing::commands::Outcome::Moved), "{out:?}");
    assert_eq!(e.section_scope, None);
}
