//! Formatting marks and hidden text display

use super::super::Editor;
use super::super::view::{MarkKind, MarkOptions};
use super::{blank_body, fonts, open, para_id};
use crate::layout::Unit;

fn lay(body: &str) -> Editor {
    let mut e = Editor::open(&blank_body(body), fonts()).expect("opens");
    e.complete(fonts());
    e
}

/// The x of each laid-out unit of paragraph `n` on page 0.
fn stops(e: &mut Editor, n: usize) -> Vec<(Unit, f64)> {
    let para = para_id(e, n);
    let g = e.page_geometries().into_iter().next().expect("a page").1;
    g.lines.iter().flat_map(|l| l.stops.iter()).filter(|s| s.para == para).map(|s| (s.unit, s.x)).collect()
}

fn kinds(e: &mut Editor, o: MarkOptions) -> Vec<MarkKind> {
    e.formatting_marks_with(0..1, o).into_iter().map(|m| m.kind).collect()
}

const HIDDEN: &str = r#"<w:p><w:r><w:t xml:space="preserve">Shown </w:t></w:r><w:r><w:rPr><w:vanish/></w:rPr><w:t>secret</w:t></w:r><w:r><w:t xml:space="preserve"> end</w:t></w:r></w:p>"#;

#[test]
fn hidden_text_is_laid_out_and_underlined_while_it_shows() {
    let mut e = lay(HIDDEN);
    let laid = |e: &mut Editor| stops(e, 0).iter().any(|(u, _)| u.0 == 8);
    assert!(!laid(&mut e), "hidden text isn't laid out");
    assert!(!kinds(&mut e, MarkOptions::ALL).contains(&MarkKind::Hidden));
    e.set_mark_options(MarkOptions { hidden: true, ..MarkOptions::default() });
    e.complete(fonts());
    assert!(laid(&mut e), "hidden text shows");
    let marks: Vec<MarkKind> = e.shown_marks(0..1).into_iter().map(|m| m.kind).collect();
    assert!(marks.contains(&MarkKind::Hidden) && !marks.contains(&MarkKind::Space), "{marks:?}");
    // The caret steps through it.
    let p = para_id(&e, 0);
    e.select_range(p, Unit(6), Unit(6), fonts());
    e.execute(
        super::super::commands::Command::MoveSelection(crate::api::command::MoveSelection {
            by: crate::api::command::MoveUnit::Character,
            direction: crate::api::value::Direction::Forward,
            extend: Some(true),
        }),
        fonts(),
    );
    assert_eq!(e.selection.focus().and_then(|f| f.para_offset()).map(|(_, u)| u), Some(Unit(7)), "into the hidden text");
    e.select_range(p, Unit(0), Unit(16), fonts());
    assert_eq!(e.selected_text(), "Shown secret end", "the selection's text has the shown hidden text");
    let copied = e.copy_fragment().expect("a copy").text();
    assert_eq!(copied, "Shown secret end", "and so does its plain text");
    e.set_mark_options(MarkOptions::default());
    e.select_range(p, Unit(0), Unit(16), fonts());
    assert_eq!(e.selected_text(), "Shown  end", "hidden again, it isn't read");
    assert_eq!(e.copy_fragment().expect("a copy").text(), "Shown  end");
    e.complete(fonts());
    assert!(!laid(&mut e), "hidden again");
}

#[test]
fn optional_hyphens_show_as_not_signs_with_their_width() {
    let mut e = lay("<w:p><w:r><w:t>ab</w:t><w:softHyphen/><w:t>cd</w:t></w:r></w:p>");
    let end = |e: &mut Editor| stops(e, 0).iter().find(|(u, _)| u.0 == 5).map(|s| s.1).expect("laid out");
    let was = end(&mut e);
    e.set_mark_options(MarkOptions { optional_hyphens: true, ..MarkOptions::default() });
    e.complete(fonts());
    assert!(end(&mut e) > was + 1.0, "the not sign's width");
    e.set_mark_options(MarkOptions::default());
    e.complete(fonts());
    assert_eq!(end(&mut e), was);
}

#[test]
fn each_mark_kind_shows_on_its_own_and_breaks_cells_and_anchors_have_marks() {
    let mut e =
        lay(r#"<w:p><w:r><w:t xml:space="preserve">a b	c</w:t></w:r><w:r><w:t>&#160;d&#8209;e</w:t></w:r><w:r><w:br w:type="page"/></w:r></w:p>"#);
    let only = |o: MarkOptions, e: &mut Editor| kinds(e, o);
    assert_eq!(only(MarkOptions::default(), &mut e), []);
    assert_eq!(only(MarkOptions { spaces: true, ..MarkOptions::default() }, &mut e), [MarkKind::Space, MarkKind::NonBreakingSpace]);
    assert_eq!(only(MarkOptions { tabs: true, ..MarkOptions::default() }, &mut e), [MarkKind::Tab]);
    let all: Vec<MarkKind> = e.formatting_marks_with(0..2, MarkOptions::ALL).into_iter().map(|m| m.kind).collect();
    for k in [MarkKind::NonBreakingHyphen, MarkKind::PageBreak, MarkKind::Paragraph] {
        assert!(all.contains(&k), "{k:?} {all:?}");
    }
    let mut e = open("FX-TBL-BASIC.docx");
    e.complete(fonts());
    let all = kinds(&mut e, MarkOptions { paragraphs: true, ..MarkOptions::default() });
    assert!(all.contains(&MarkKind::CellEnd) && all.contains(&MarkKind::RowEnd), "{all:?}");
    let mut e = open("FX-SECTIONS.docx");
    e.complete(fonts());
    let all: Vec<MarkKind> =
        (0..e.page_geometries().len()).flat_map(|p| e.formatting_marks_with(p..p + 1, MarkOptions::ALL)).map(|m| m.kind).collect();
    assert!(all.iter().any(|k| matches!(k, MarkKind::SectionBreak(_))), "{all:?}");
}

#[test]
fn only_the_selected_floats_anchor_shows_while_object_anchors_are_on() {
    use super::super::selection::{ObjectRef, Selection};
    let mut e = open("FX-OBJ.docx");
    e.complete(fonts());
    let f = e.page_geometries().iter().flat_map(|(_, g)| g.stories.iter().filter_map(|s| s.float)).next().expect("a float");
    assert!(e.view(0..1).anchor.is_none(), "no anchor without a selected object");
    e.set_selection(Selection::Object(ObjectRef::Floating(f)));
    assert!(e.view(0..1).anchor.is_some(), "the selected float's anchor shows by default");
    e.set_mark_options(MarkOptions { anchors: false, ..MarkOptions::default() });
    assert!(e.view(0..1).anchor.is_none(), "Object anchors off");
    e.set_mark_options(MarkOptions { all: true, anchors: false, ..MarkOptions::default() });
    assert!(e.view(0..1).anchor.is_some(), "Show/Hide shows it");
}

#[test]
fn a_style_separator_shows_its_paragraphs_apart_while_hidden_text_shows() {
    let body = r#"<w:p><w:pPr><w:jc w:val="center"/><w:spacing w:after="480"/><w:rPr><w:vanish/><w:specVanish/></w:rPr></w:pPr><w:r><w:t>Heading</w:t></w:r></w:p><w:p><w:pPr><w:ind w:left="720"/></w:pPr><w:r><w:t>Body text after it.</w:t></w:r></w:p>"#;
    let mut e = lay(body);
    assert_eq!(super::texts(&e).len(), 1, "the reader joins them");
    let lines = |e: &mut Editor| {
        let para = para_id(e, 0);
        let g = e.page_geometries().into_iter().next().expect("a page").1;
        g.lines
            .iter()
            .filter(|l| l.shows(para))
            .map(|l| (l.top, l.bottom, l.stops.first().map_or(0.0, |s| s.x), l.stops.last().map(|s| s.unit)))
            .collect::<Vec<_>>()
    };
    assert_eq!(lines(&mut e).len(), 1, "one line while hidden text doesn't show");
    e.set_mark_options(MarkOptions { hidden: true, paragraphs: true, ..MarkOptions::default() });
    e.complete(fonts());
    let shown = lines(&mut e);
    assert_eq!(shown.len(), 2, "{shown:?}");
    let (first, second) = (shown[0], shown[1]);
    assert_eq!(first.3, Some(Unit(7)), "the first ends at its vanished mark");
    assert!(second.0 - first.1 >= 24.0 - 0.01, "its space after between them: {shown:?}");
    assert!(first.2 > second.2 + 100.0, "centered over the indented second: {shown:?}");
    assert!((second.2 - 72.0 - 36.0).abs() < 0.5, "the second's own indent: {shown:?}");
    let marks = e.shown_marks(0..1);
    let hidden: Vec<_> = marks.iter().filter(|m| m.kind == MarkKind::HiddenParagraph).collect();
    assert_eq!(hidden.len(), 1, "{marks:?}");
    assert!(marks.iter().any(|m| m.kind == MarkKind::Paragraph), "the last mark");
    e.set_mark_options(MarkOptions::default());
    e.complete(fonts());
    assert_eq!(lines(&mut e).len(), 1, "joined again");
}

#[test]
fn a_mark_hidden_by_editing_shows_its_paragraphs_apart_while_hidden_text_shows() {
    use crate::api::command::{Command, MoveSelection, MoveUnit};
    use crate::api::value::Direction;
    use crate::editing::position::{Position, StoryId};
    use crate::editing::selection::{Granularity, Selection};
    let mut e = lay("<w:p><w:r><w:t>First</w:t></w:r></w:p><w:p><w:r><w:t>Second</w:t></w:r></w:p>");
    let (a, b) = (para_id(&e, 0), para_id(&e, 1));
    let at = |p, u| Position::text(StoryId::Body, p, Unit(u));
    e.set_selection(Selection::range(at(a, 5), at(b, 0), Granularity::Char));
    e.execute(Command::SetHidden(true), fonts());
    let Some(crate::docx::Block::Para(p)) = e.document().sections[0].blocks.first() else { panic!("a paragraph") };
    assert!(p.mark.hidden, "the mark is hidden");
    e.complete(fonts());
    let lines = |e: &mut Editor| {
        let g = e.page_geometries().into_iter().next().expect("a page").1;
        let both = g.lines.iter().any(|l| l.shows(a) && l.shows(b));
        let apart = g.lines.iter().any(|l| l.shows(a) && !l.shows(b)) && g.lines.iter().any(|l| l.shows(b) && !l.shows(a));
        (both, apart)
    };
    assert_eq!(lines(&mut e), (true, false), "joined while hidden text doesn't show");
    e.set_mark_options(MarkOptions { hidden: true, paragraphs: true, ..MarkOptions::default() });
    e.complete(fonts());
    assert_eq!(lines(&mut e), (false, true), "shown apart");
    let marks = e.shown_marks(0..1);
    assert_eq!(marks.iter().filter(|m| m.kind == MarkKind::HiddenParagraph).count(), 1, "{marks:?}");
    assert_eq!(marks.iter().filter(|m| m.kind == MarkKind::Paragraph).count(), 1, "the last mark: {marks:?}");
    e.set_selection(Selection::Caret(at(a, 5)));
    e.execute(Command::MoveSelection(MoveSelection { by: MoveUnit::Character, direction: Direction::Forward, extend: None }), fonts());
    assert_eq!(e.selection(), Selection::Caret(at(b, 0)), "the caret moves past the hidden mark");
    e.set_mark_options(MarkOptions::default());
    e.complete(fonts());
    assert_eq!(lines(&mut e), (true, false), "joined again");
}

#[test]
fn a_chain_of_joins_longer_than_the_parts_bound_stops_adding_parts_and_shows_joined() {
    let n = crate::format::resolve::MAX_PARTS + 40;
    let joined = r#"<w:p><w:pPr><w:rPr><w:vanish/></w:rPr></w:pPr><w:r><w:t>a</w:t></w:r></w:p>"#.repeat(n);
    let mut e = lay(&format!("{joined}<w:p><w:r><w:t>end</w:t></w:r></w:p>"));
    let Some(crate::docx::Block::Para(p)) = e.document().sections[0].blocks.first() else { panic!("a paragraph") };
    assert_eq!(p.src.as_ref().map(|s| s.joined.len()), Some(n), "one paragraph");
    assert_eq!(p.style.parts.len(), crate::format::resolve::MAX_PARTS, "the parts stop growing");
    e.set_mark_options(MarkOptions { hidden: true, ..MarkOptions::default() });
    e.complete(fonts());
    let lines = e.page_geometries().into_iter().map(|(_, g)| g.lines.len()).sum::<usize>();
    assert!(lines < 10, "shown joined: {lines} lines");
}

#[test]
fn a_deletions_stand_in_is_no_space() {
    use crate::docx::review::TextMark;
    let del = r#"w:id="1" w:author="A" w:date="2026-01-01T00:00:00Z""#;
    let mut e =
        lay(&format!(r#"<w:p><w:r><w:t>ab</w:t></w:r><w:del {del}><w:r><w:delText>g</w:delText></w:r></w:del><w:r><w:t>cd</w:t></w:r></w:p>"#));
    let mut style = e.document().review.markup.style.clone();
    style.deletions.mark = TextMark::Caret;
    e.set_markup_style(style);
    e.complete(fonts());
    assert!(stops(&mut e, 0).iter().any(|(u, _)| u.0 == 2), "the caret stands in for the deletion");
    assert!(!kinds(&mut e, MarkOptions::ALL).contains(&MarkKind::Space));
}
