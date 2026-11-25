//! Theme colours and fonts on text, and formatting read-back

use std::io::Read;

use super::super::Editor;
use super::super::commands::{Command, LigatureFlags, OpenTypeChange, Outcome};
use super::super::position::{Position, StoryId};
use super::super::readback::{BorderLine, ParagraphBorderEdges, ShadingValue, StylisticSets};
use super::super::selection::Selection;
use super::{blank_body, fonts};
use crate::api::command::{AddTabStop, BorderStyle, Command as C, ShadingPattern, ShadingTarget, Typography};
use crate::api::drawing::{ColourTransform, ColourValue, ThemeColour, TransformKind};
use crate::api::state::Tri;
use crate::api::value::{AutoOr, Color, NoneOr, Points, TabAlignment};
use crate::docx::ThemeFont;
use crate::layout::Unit;

fn doc_xml(e: &Editor) -> String {
    let bytes = crate::docx::write::save(e.document()).expect("saves");
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut s = String::new();
    z.by_name("word/document.xml").unwrap().read_to_string(&mut s).unwrap();
    s
}

fn edit(body: &str) -> Editor {
    let mut e = Editor::open(&blank_body(body), fonts()).expect("opens");
    e.complete(fonts());
    e
}

fn all(body: &str) -> Editor {
    let mut e = edit(body);
    e.execute(Command::SelectAll, fonts());
    e
}

fn reopened(e: &Editor) -> Editor {
    let bytes = crate::docx::write::save(e.document()).expect("saves");
    let mut e = Editor::open(&bytes, fonts()).expect("reopens");
    e.complete(fonts());
    e.execute(Command::SelectAll, fonts());
    e
}

/// The caret in the first paragraph.
fn first(e: &mut Editor) {
    let para = super::texts_ids(e)[0];
    e.selection = Selection::Caret(Position::text(StoryId::Body, para, Unit(1)));
}

const TWO: &str = "<w:p><w:r><w:t>one</w:t></w:r></w:p><w:p><w:r><w:t>two</w:t></w:r></w:p>";

fn accent(t: ThemeColour, kind: TransformKind, value: f64) -> ColourValue {
    let mut c = ColourValue::theme(t);
    c.transforms.push(ColourTransform { kind, value });
    c
}

fn line(color: AutoOr<ColourValue>, space: f64, shadow: bool) -> BorderLine {
    BorderLine { style: BorderStyle::Double, width: Points(1.5), color, space: Points(space), shadow }
}

#[test]
fn a_theme_text_colour_is_written_as_one_and_reads_back_as_one() {
    let mut e = all(TWO);
    let c = accent(ThemeColour::Accent1, TransformKind::Tint, 0.4);
    assert!(matches!(e.set_font_colour(AutoOr::Value(c.clone()), fonts()), Outcome::Changed(_)));
    let xml = doc_xml(&e);
    assert!(xml.contains(r#"w:themeColor="accent1""#) && xml.contains(r#"w:themeTint="66""#), "{xml}");
    assert_eq!(e.character_readback().expect("a readback").color, Tri::Uniform(AutoOr::Value(c.clone())));
    let shown = e.shown_colour(&c).expect("a colour");
    assert_eq!(e.format_state().expect("a state").character.color, Tri::Uniform(AutoOr::Value(shown)), "the state's RGB is what it shows");
    assert_eq!(reopened(&e).character_readback().expect("a readback").color, Tri::Uniform(AutoOr::Value(c)), "it round-trips");
}

#[test]
fn an_rgb_colour_over_a_theme_one_drops_the_theme() {
    let mut e = all(r#"<w:p><w:r><w:rPr><w:color w:val="4472C4" w:themeColor="accent1"/></w:rPr><w:t>x</w:t></w:r></w:p>"#);
    assert_eq!(e.character_readback().expect("a readback").color, Tri::Uniform(AutoOr::Value(ColourValue::theme(ThemeColour::Accent1))));
    e.set_font_colour(AutoOr::Value(ColourValue::rgb(Color(0xFF0000))), fonts());
    assert!(!doc_xml(&e).contains("themeColor"));
    assert_eq!(e.character_readback().expect("a readback").color, Tri::Uniform(AutoOr::Value(ColourValue::rgb(Color(0xFF0000)))));
}

#[test]
fn colours_differing_across_the_selection_read_as_mixed() {
    let e = &mut all(
        r#"<w:p><w:r><w:rPr><w:color w:val="4472C4" w:themeColor="accent1"/></w:rPr><w:t>a</w:t></w:r><w:r><w:rPr><w:color w:val="4472C4"/></w:rPr><w:t>b</w:t></w:r></w:p>"#,
    );
    assert_eq!(e.character_readback().expect("a readback").color, Tri::Mixed, "the same RGB, one a theme colour");
}

#[test]
fn a_theme_colour_takes_one_tint_or_shade() {
    let mut e = all(TWO);
    let mut c = accent(ThemeColour::Accent2, TransformKind::Tint, 0.4);
    c.transforms.push(ColourTransform { kind: TransformKind::Shade, value: 0.5 });
    assert!(matches!(e.set_font_colour(AutoOr::Value(c), fonts()), Outcome::Refused(_)));
}

#[test]
fn a_theme_underline_colour_is_written_on_the_underline() {
    let mut e = all(r#"<w:p><w:r><w:rPr><w:u w:val="single"/></w:rPr><w:t>x</w:t></w:r></w:p>"#);
    let c = accent(ThemeColour::Accent6, TransformKind::Shade, 191.0 / 255.0);
    e.set_underline_colour(AutoOr::Value(c.clone()), fonts());
    assert!(doc_xml(&e).contains(r#"w:themeColor="accent6" w:themeShade="BF""#) || doc_xml(&e).contains(r#"w:themeShade="BF""#));
    assert_eq!(e.character_readback().expect("a readback").underline_color, Tri::Uniform(AutoOr::Value(c)));
    e.set_underline_colour(AutoOr::Auto, fonts());
    assert_eq!(e.character_readback().expect("a readback").underline_color, Tri::Uniform(AutoOr::Auto));
    assert!(!doc_xml(&e).contains("themeShade"));
}

#[test]
fn text_shading_takes_theme_fill_and_pattern_colours_and_reads_back() {
    let mut e = all(TWO);
    let s = ShadingValue {
        fill: NoneOr::Value(accent(ThemeColour::Accent2, TransformKind::Tint, 0.2)),
        pattern: ShadingPattern::Pct20,
        color: AutoOr::Value(ColourValue::theme(ThemeColour::Text1)),
    };
    assert!(matches!(e.set_shading_value(ShadingTarget::Text, &s, fonts()), Outcome::Changed(_)));
    let xml = doc_xml(&e);
    assert!(xml.contains(r#"w:themeFill="accent2""#) && xml.contains(r#"w:themeFillTint="33""#) && xml.contains(r#"w:themeColor="text1""#), "{xml}");
    assert_eq!(e.character_readback().expect("a readback").shading, Tri::Uniform(NoneOr::Value(s.clone())));
    assert_eq!(reopened(&e).character_readback().expect("a readback").shading, Tri::Uniform(NoneOr::Value(s)));
}

#[test]
fn a_text_border_keeps_its_distance_shadow_and_theme_colour() {
    let mut e = all(TWO);
    let b = line(AutoOr::Value(ColourValue::theme(ThemeColour::Accent4)), 4.0, true);
    assert!(matches!(e.set_text_border_line(Some(&b), fonts()), Outcome::Changed(_)));
    let xml = doc_xml(&e);
    assert!(xml.contains(r#"w:space="4""#) && xml.contains(r#"w:shadow="1""#) && xml.contains(r#"w:themeColor="accent4""#), "{xml}");
    assert_eq!(e.character_readback().expect("a readback").border, Tri::Uniform(NoneOr::Value(b.clone())));
    assert_eq!(reopened(&e).character_readback().expect("a readback").border, Tri::Uniform(NoneOr::Value(b)));
    e.set_text_border_line(None, fonts());
    assert_eq!(e.character_readback().expect("a readback").border, Tri::Uniform(NoneOr::None));
}

#[test]
fn a_border_distance_past_31_points_is_refused() {
    let mut e = all(TWO);
    assert!(matches!(e.set_text_border_line(Some(&line(AutoOr::Auto, 32.0, false)), fonts()), Outcome::Refused(_)));
}

#[test]
fn a_theme_font_follows_the_theme_and_reads_back_as_its_slot() {
    let mut e = all(TWO);
    let fonts_of = e.theme_fonts();
    assert!(!fonts_of.body.is_empty() && !fonts_of.heading.is_empty(), "{fonts_of:?}");
    e.execute(C::SetFont("Courier New".into()), fonts());
    assert_eq!(e.character_readback().expect("a readback").font_theme, Tri::Uniform(NoneOr::None));
    assert!(matches!(e.set_theme_font(ThemeFont::Major, fonts()), Outcome::Changed(_)));
    let xml = doc_xml(&e);
    assert!(xml.contains(r#"w:asciiTheme="majorHAnsi""#) && xml.contains(r#"w:hAnsiTheme="majorHAnsi""#), "{xml}");
    assert!(!xml.contains(r#"w:ascii="#) && !xml.contains(r#"w:hAnsi="#), "the named Latin font goes: {xml}");
    assert_eq!(e.character_readback().expect("a readback").font_theme, Tri::Uniform(NoneOr::Value(ThemeFont::Major)));
    assert_eq!(e.format_state().expect("a state").character.font, Tri::Uniform(fonts_of.heading.clone()));
    let (state, character, _) = e.format_readback().expect("a readback");
    assert_eq!((state.character.font, character.font_theme), (Tri::Uniform(fonts_of.heading), Tri::Uniform(NoneOr::Value(ThemeFont::Major))));
}

#[test]
fn fit_text_fits_a_region_and_clears_from_a_caret_in_it() {
    let mut e = all(TWO);
    assert!(matches!(e.set_fit_text(Some(Points(72.0)), fonts()), Outcome::Changed(_)));
    assert!(doc_xml(&e).contains(r#"<w:fitText w:val="1440" w:id="1"/>"#), "{}", doc_xml(&e));
    assert_eq!(e.character_readback().expect("a readback").fit_text, Tri::Uniform(NoneOr::Value(Points(72.0))));
    first(&mut e);
    assert!(matches!(e.set_fit_text(None, fonts()), Outcome::Changed(_)));
    assert!(!e.selection.is_collapsed(), "the caret's region is selected");
    assert_eq!(e.character_readback().expect("a readback").fit_text, Tri::Uniform(NoneOr::None));
    e.execute(Command::SelectAll, fonts());
    assert_eq!(e.character_readback().expect("a readback").fit_text, Tri::Mixed, "only the first paragraph's region is cleared");
    assert!(matches!(e.set_fit_text(Some(Points(0.0)), fonts()), Outcome::Refused(_)));
}

#[test]
fn ligatures_are_four_flags_and_stylistic_sets_a_set() {
    let mut e = all(TWO);
    let more = OpenTypeChange {
        ligatures: Some(LigatureFlags { contextual: true, historical: true, ..LigatureFlags::default() }),
        stylistic_sets: Some(vec![3, 1, 3]),
    };
    assert!(matches!(e.set_typography_with(Typography::default(), &more, fonts()), Outcome::Changed(_)));
    let xml = doc_xml(&e);
    assert!(xml.contains(r#"w14:val="contextualHistorical""#), "{xml}");
    assert!(xml.contains(r#"<w14:styleSet w14:id="1"/><w14:styleSet w14:id="3"/>"#), "{xml}");
    let r = e.character_readback().expect("a readback");
    assert_eq!(r.ligatures, Tri::Uniform(more.ligatures.unwrap()));
    assert_eq!(r.stylistic_sets, Tri::Uniform(StylisticSets(0b101)));
    assert_eq!(StylisticSets(0b101).ids().collect::<Vec<_>>(), [1, 3]);
    let bad = OpenTypeChange { stylistic_sets: Some(vec![21]), ..OpenTypeChange::default() };
    assert!(matches!(e.set_typography_with(Typography::default(), &bad, fonts()), Outcome::Refused(_)));
}

#[test]
fn paragraph_borders_read_back_per_edge_with_distance_and_shadow() {
    let mut e = all(TWO);
    let top = line(AutoOr::Value(ColourValue::theme(ThemeColour::Accent1)), 6.0, false);
    let bottom = line(AutoOr::Auto, 2.0, true);
    let edges = ParagraphBorderEdges { top: Some(Some(top.clone())), bottom: Some(Some(bottom.clone())), ..ParagraphBorderEdges::default() };
    assert!(matches!(e.set_paragraph_border_edges(&edges, fonts()), Outcome::Changed(_)));
    let r = e.paragraph_readback().expect("a readback");
    assert_eq!((r.border_top, r.border_bottom.clone()), (Tri::Uniform(NoneOr::Value(top.clone())), Tri::Uniform(NoneOr::Value(bottom))));
    assert_eq!(r.border_left, Tri::Uniform(NoneOr::None));
    let r = reopened(&e).paragraph_readback().expect("a readback");
    assert_eq!(r.border_top, Tri::Uniform(NoneOr::Value(top)));
    // One paragraph's bottom removed: mixed
    first(&mut e);
    e.set_paragraph_border_edges(&ParagraphBorderEdges { bottom: Some(None), ..ParagraphBorderEdges::default() }, fonts());
    e.execute(Command::SelectAll, fonts());
    assert_eq!(e.paragraph_readback().expect("a readback").border_bottom, Tri::Mixed);
}

#[test]
fn paragraph_shading_reads_back_its_pattern() {
    let mut e = all(TWO);
    let s = ShadingValue { fill: NoneOr::Value(ColourValue::rgb(Color(0xFFFF00))), pattern: ShadingPattern::Pct10, color: AutoOr::Auto };
    e.set_shading_value(ShadingTarget::Paragraph, &s, fonts());
    assert_eq!(e.paragraph_readback().expect("a readback").shading, Tri::Uniform(NoneOr::Value(s)));
}

#[test]
fn tab_stops_read_back_unless_they_differ() {
    let mut e = all(TWO);
    e.execute(C::AddTabStop(AddTabStop { position: Points(72.0), alignment: Some(TabAlignment::Right), leader: None }), fonts());
    let Tri::Uniform(tabs) = e.paragraph_readback().expect("a readback").tabs else { panic!("the same stops") };
    assert_eq!((tabs.len(), tabs[0].position, tabs[0].alignment), (1, Points(72.0), TabAlignment::Right));
    first(&mut e);
    e.execute(C::ClearTabStops, fonts());
    e.execute(Command::SelectAll, fonts());
    assert_eq!(e.paragraph_readback().expect("a readback").tabs, Tri::Mixed);
}

#[test]
fn the_theme_grid_names_each_colour_and_its_variants() {
    let e = edit(TWO);
    let grid = e.theme_colour_grid();
    assert_eq!(grid.len(), 10);
    assert!(grid.iter().all(|c| c.swatches.len() == 6));
    let accent1 = grid.iter().find(|c| c.theme == ThemeColour::Accent1).expect("accent 1");
    let labels: Vec<&str> = accent1.swatches.iter().map(|s| s.label.as_str()).collect();
    assert_eq!(
        labels,
        ["Accent 1", "Accent 1, Lighter 80%", "Accent 1, Lighter 60%", "Accent 1, Lighter 40%", "Accent 1, Darker 25%", "Accent 1, Darker 50%"]
    );
    // Each swatch's colour shows its RGB.
    for s in &accent1.swatches {
        assert_eq!(e.shown_colour(&s.colour).expect("a colour"), s.rgb, "{}", s.label);
    }
    let white = grid.iter().find(|c| c.theme == ThemeColour::Background1).expect("background 1");
    assert_eq!(white.swatches[1].label, "Background 1, Darker 5%");
}

#[test]
fn cell_shading_and_borders_take_theme_colours() {
    let mut e = edit(
        r#"<w:tbl><w:tblPr><w:tblW w:w="0" w:type="auto"/></w:tblPr><w:tblGrid><w:gridCol w:w="2000"/></w:tblGrid><w:tr><w:tc><w:p><w:r><w:t>a</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p/>"#,
    );
    first(&mut e);
    let fill = accent(ThemeColour::Accent5, TransformKind::Tint, 0.4);
    assert!(matches!(e.set_cell_shading(Some(fill), fonts()), Outcome::Changed(_)));
    let b = line(AutoOr::Value(ColourValue::theme(ThemeColour::Accent3)), 0.0, true);
    assert!(matches!(e.set_cell_border_lines(crate::api::command::BorderEdges::All, Some(&b), fonts()), Outcome::Changed(_)));
    let xml = doc_xml(&e);
    assert!(xml.contains(r#"w:themeFill="accent5" w:themeFillTint="66""#), "{xml}");
    assert!(xml.contains(r#"w:themeColor="accent3""#), "{xml}");
    assert!(xml.contains(r#"w:shadow="1""#), "the line's shadow: {xml}");
}

#[test]
fn row_exceptions_take_theme_colours() {
    use crate::editing::commands::RowExceptionsEdit;
    let mut e = edit(
        r#"<w:tbl><w:tblPr><w:tblW w:w="0" w:type="auto"/></w:tblPr><w:tblGrid><w:gridCol w:w="2000"/></w:tblGrid><w:tr><w:tc><w:p><w:r><w:t>a</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p/>"#,
    );
    first(&mut e);
    let x = RowExceptionsEdit {
        shading: Some(crate::editing::readback::ShadingValue {
            fill: crate::api::value::NoneOr::Value(accent(ThemeColour::Accent2, TransformKind::Shade, 191.0 / 255.0)),
            pattern: crate::api::command::ShadingPattern::Clear,
            color: AutoOr::Auto,
        }),
        borders: Some((crate::api::command::BorderEdges::Outside, Some(line(AutoOr::Value(ColourValue::theme(ThemeColour::Accent4)), 0.0, false)))),
        ..RowExceptionsEdit::default()
    };
    assert!(matches!(e.set_row_exceptions(None, Some(&x), fonts()), Outcome::Changed(_)));
    let xml = doc_xml(&e);
    let ex = &xml[xml.find("<w:tblPrEx>").expect("row exceptions")..];
    assert!(ex.contains(r#"w:themeFill="accent2" w:themeFillShade="BF""#), "{xml}");
    assert!(ex.contains(r#"w:themeColor="accent4""#), "{xml}");
    let bad = RowExceptionsEdit { borders: Some((crate::api::command::BorderEdges::All, Some(line(AutoOr::Auto, 40.0, false)))), ..x };
    assert!(matches!(e.set_row_exceptions(None, Some(&bad), fonts()), Outcome::Refused(_)), "the distance is 0 to 31 pt");
}

#[test]
fn document_defaults_take_a_theme_font_and_colour() {
    let mut e = edit(TWO);
    let c = accent(ThemeColour::Accent1, TransformKind::Tint, 0.4);
    let d = crate::api::command::DocumentDefaults::default();
    assert!(matches!(e.set_document_defaults_with(d, Some(ThemeFont::Minor), Some(AutoOr::Value(c)), fonts()), Outcome::Changed(_)));
    let raw = e.document().resolver.as_ref().and_then(|r| r.styles.defaults_raw.clone()).expect("defaults");
    assert!(
        raw.contains(r#"w:asciiTheme="minorHAnsi""#) && raw.contains(r#"w:themeColor="accent1""#) && raw.contains(r#"w:themeTint="66""#),
        "{raw}"
    );
    assert_eq!(e.history().next_undo(), Some("Set Defaults"));
}

#[test]
fn a_text_wrapping_break_clears_both_sides() {
    let mut e = edit(TWO);
    first(&mut e);
    assert!(matches!(e.line_break_clearing(Some(crate::editing::commands::BreakClear::Both), fonts()), Outcome::Changed(_)));
    assert!(doc_xml(&e).contains(r#"<w:br w:clear="all"/>"#), "{}", doc_xml(&e));
    assert_eq!(e.history().next_undo(), Some("Text Wrapping Break"));
}

#[test]
fn page_borders_take_each_edges_distance_and_colour_and_read_back() {
    use crate::api::command::{PageBorderDisplay, PageBorderOffset};
    use crate::editing::section::PageBordersEdit;
    let mut e = edit(TWO);
    let top = line(AutoOr::Value(ColourValue::theme(ThemeColour::Accent2)), 10.0, false);
    let bottom = line(AutoOr::Auto, 20.0, true);
    let c = PageBordersEdit {
        top: Some(Some(top.clone())),
        bottom: Some(Some(bottom.clone())),
        display: Some(PageBorderDisplay::FirstPage),
        behind_text: Some(true),
        ..PageBordersEdit::default()
    };
    assert!(matches!(e.set_page_border_edges(&c, fonts()), Outcome::Changed(_)));
    let xml = doc_xml(&e);
    assert!(xml.contains(r#"w:zOrder="back""#) && xml.contains(r#"w:space="10""#) && xml.contains(r#"w:themeColor="accent2""#), "{xml}");
    let check = |e: &Editor| {
        let r = e.page_borders_of(0).expect("a section");
        assert_eq!((r.top, r.bottom, r.left), (Some(top.clone()), Some(bottom.clone()), None));
        assert_eq!((r.display, r.offset_from, r.behind_text), (PageBorderDisplay::FirstPage, PageBorderOffset::Page, true));
        assert!(r.surround_header && r.surround_footer);
    };
    check(&e);
    check(&reopened(&e));
    let far = line(AutoOr::Auto, 32.0, false);
    assert!(matches!(
        e.set_page_border_edges(&PageBordersEdit { left: Some(Some(far)), ..PageBordersEdit::default() }, fonts()),
        Outcome::Refused(_)
    ));
}

#[test]
fn page_borders_paint_in_front_of_the_text_unless_behind() {
    use crate::editing::section::PageBordersEdit;
    let mut e = edit(TWO);
    let c = |behind| PageBordersEdit { top: Some(Some(line(AutoOr::Auto, 0.0, false))), behind_text: Some(behind), ..PageBordersEdit::default() };
    let last_is_border = |e: &mut Editor| {
        let snap = e.snapshot();
        let page = &snap.pages()[0];
        let borders: Vec<*const crate::layout::fragment::Item> = page.borders.iter().map(|b| b as *const _).collect();
        assert!(!borders.is_empty(), "a border");
        let mut order = Vec::new();
        crate::layout::traverse::paint(page, &mut |p| order.push(p.item as *const _));
        let is = |p: Option<&*const crate::layout::fragment::Item>| p.is_some_and(|p| borders.contains(p));
        (is(order.first()), is(order.last()))
    };
    e.set_page_border_edges(&c(false), fonts());
    assert_eq!(last_is_border(&mut e), (false, true), "in front: painted last");
    e.set_page_border_edges(&c(true), fonts());
    assert_eq!(last_is_border(&mut e), (true, false), "behind: painted first");
}

/// The saved headers' XML.
fn headers_xml(e: &Editor) -> String {
    let bytes = crate::docx::write::save(e.document()).expect("saves");
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut out = String::new();
    for i in 0..z.len() {
        let mut f = z.by_index(i).unwrap();
        if f.name().unwrap().starts_with("word/header") {
            f.read_to_string(&mut out).unwrap();
        }
    }
    out
}

#[test]
fn an_outside_page_number_is_a_frame_on_the_margins_outside_edge() {
    use crate::api::command::PageNumberFormat;
    use crate::editing::commands::{PageNumberAlign, PageNumberSpot};
    let mut e = edit(TWO);
    let out = e.insert_page_number_at(PageNumberFormat::Page, PageNumberSpot::Header, Some(PageNumberAlign::Outside), fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    let xml = headers_xml(&e);
    assert!(xml.contains(r#"w:xAlign="outside""#) && xml.contains(r#"w:hAnchor="margin""#), "{xml}");
    assert!(xml.contains(" PAGE "), "{xml}");
    let framed = |e: &Editor| {
        e.document().stories.values().flatten().any(
            |b| matches!(b, crate::docx::Block::Para(p) if p.inlines.iter().any(|i| matches!(i, crate::docx::Inline::Float(f, _) if f.is_frame()))),
        )
    };
    assert!(framed(&reopened(&e)), "it reopens as a frame");
    assert_eq!(e.history().undo_labels().next(), Some("Page Number"), "one action");
    e.execute(Command::Undo, fonts());
    assert!(!headers_xml(&e).contains(" PAGE "));
}

#[test]
fn a_page_margin_page_number_fills_the_margin_down_the_middle() {
    use crate::api::command::PageNumberFormat;
    use crate::editing::commands::{MarginSide, PageNumberSpot};
    let mut e = edit(TWO);
    assert!(matches!(
        e.insert_page_number_at(PageNumberFormat::Page, PageNumberSpot::Margin(MarginSide::Inside), None, fonts()),
        Outcome::Changed(_)
    ));
    let xml = headers_xml(&e);
    assert!(xml.contains(r#"w:w="1440""#) && xml.contains(r#"w:xAlign="inside""#) && xml.contains(r#"w:yAlign="center""#), "{xml}");
    assert!(xml.contains(r#"w:hAnchor="page""#) && xml.contains(r#"w:vAnchor="page""#), "{xml}");
}

#[test]
fn page_setup_applies_from_this_point_forward_or_to_the_whole_document() {
    use crate::api::command::Margins;
    use crate::api::services::ApplyTo;
    use crate::api::value::Orientation;
    let mut e = edit(TWO);
    let ids = super::texts_ids(&e);
    e.selection = Selection::Caret(Position::text(StoryId::Body, ids[1], Unit(0)));
    let out = e.in_section_scope(ApplyTo::ThisPointForward, "Page Setup", |e| e.execute(C::SetOrientation(Orientation::Landscape), fonts()), fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    let wide = |e: &Editor| e.document().sections.iter().map(|s| s.page_w > s.page_h).collect::<Vec<_>>();
    assert_eq!(wide(&e), [false, true], "a new section from the caret on");
    e.execute(Command::Undo, fonts());
    assert_eq!(wide(&e), [false], "one action");
    e.execute(Command::Redo, fonts());
    assert_eq!(wide(&e), [false, true], "redone");
    first(&mut e);
    let m = Margins { left: Some(Points(90.0)), ..Margins::default() };
    e.in_section_scope(ApplyTo::WholeDocument, "Page Setup", |e| e.execute(C::SetMargins(m), fonts()), fonts());
    assert_eq!(e.document().sections.iter().map(|s| s.left).collect::<Vec<_>>(), [90.0, 90.0], "every section");
}

#[test]
fn a_page_number_is_not_framed_into_a_locked_header_paragraph() {
    use crate::api::command::PageNumberFormat;
    use crate::editing::commands::{MarginSide, PageNumberAlign, PageNumberSpot};
    let header =
        r#"<w:sdt><w:sdtPr><w:lock w:val="contentLocked"/></w:sdtPr><w:sdtContent><w:p><w:r><w:t>locked</w:t></w:r></w:p></w:sdtContent></w:sdt>"#;
    let body = r#"<w:p><w:r><w:t>body</w:t></w:r></w:p><w:sectPr><w:headerReference w:type="default" r:id="rIdH"/></w:sectPr>"#;
    for (spot, align) in [
        (PageNumberSpot::Header, Some(PageNumberAlign::Left)),
        (PageNumberSpot::Header, Some(PageNumberAlign::Outside)),
        (PageNumberSpot::Margin(MarginSide::Left), None),
    ] {
        let mut e = super::stories::package(body, Some(Some(header)));
        let out = e.insert_page_number_at(PageNumberFormat::Page, spot, align, fonts());
        assert!(!matches!(out, Outcome::Changed(_)), "{spot:?} {align:?}: {out:?}");
        assert!(!headers_xml(&e).contains(" PAGE "), "{spot:?} {align:?}");
    }
}

#[test]
fn a_framed_page_number_is_tracked_as_an_insertion() {
    use crate::api::command::PageNumberFormat;
    use crate::editing::commands::{MarginSide, PageNumberSpot};
    let mut e = edit(TWO);
    e.edit_header_footer(None, false, fonts());
    e.execute(C::InsertText("h".into()), fonts());
    e.execute(C::SetTrackChanges(true), fonts());
    let out = e.insert_page_number_at(PageNumberFormat::Page, PageNumberSpot::Margin(MarginSide::Right), None, fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    let xml = headers_xml(&e);
    assert!(xml.contains("<w:ins ") && xml.contains("w:framePr"), "{xml}");
}

#[test]
fn this_point_forward_over_selected_text_keeps_the_text() {
    use crate::api::services::ApplyTo;
    use crate::api::value::Orientation;
    let mut e = edit(TWO);
    let ids = super::texts_ids(&e);
    let a = Position::text(StoryId::Body, ids[1], Unit(1));
    let b = Position::text(StoryId::Body, ids[1], Unit(3));
    e.selection = Selection::range(a, b, crate::editing::selection::Granularity::Char);
    let out = e.in_section_scope(ApplyTo::ThisPointForward, "Page Setup", |e| e.execute(C::SetOrientation(Orientation::Landscape), fonts()), fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    let wide = |e: &Editor| e.document().sections.iter().map(|s| s.page_w > s.page_h).collect::<Vec<_>>();
    assert_eq!(wide(&e), [false, true], "from the selection's start on");
    assert_eq!(super::texts(&e).concat(), "onetwo", "the selected text is kept");
    assert_eq!(e.history().undo_labels().next(), Some("Page Setup"));
    e.execute(Command::Undo, fonts());
    assert_eq!((wide(&e), super::texts(&e).concat()), (vec![false], "onetwo".to_string()), "one action");
}

#[test]
fn a_page_number_left_off_the_first_page_takes_a_different_first_page() {
    use crate::api::command::{ChapterSeparator, NumberFormat, PageNumberFormat};
    use crate::editing::commands::{PageNumberAlign, PageNumberSpot, page_number_sample};
    let mut e = edit(TWO);
    let out = e.insert_page_number_shown(PageNumberFormat::Page, PageNumberSpot::Footer, Some(PageNumberAlign::Center), false, fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    let s = &e.document().sections[0];
    assert!(s.title_pg, "a different first page");
    let holds = |slot: usize| {
        s.footers[slot].as_ref().and_then(|n| e.document().stories.get(n)).is_some_and(|b| {
            b.iter()
                .any(|b| matches!(b, crate::docx::Block::Para(p) if p.inlines.iter().any(|i| matches!(i, crate::docx::Inline::FieldBegin { .. }))))
        })
    };
    assert!(holds(0) && !holds(1), "in the default footer, not the first page's");
    assert_eq!(e.history().undo_labels().next(), Some("Page Number"), "one action");
    e.execute(Command::Undo, fonts());
    assert!(!e.document().sections[0].title_pg);
    assert_eq!(page_number_sample(NumberFormat::Decimal, Some(ChapterSeparator::Hyphen)), "1-1");
    assert_eq!(page_number_sample(NumberFormat::UpperLetter, Some(ChapterSeparator::Hyphen)), "1-A");
    assert_eq!(page_number_sample(NumberFormat::LowerRoman, None), "i");
}

#[test]
fn a_single_quoted_font_attribute_is_replaced_not_duplicated() {
    let mut e = all(
        "<w:p><w:r><w:rPr><w:rFonts w:ascii='Arial' w:asciiTheme='majorHAnsi' w:hAnsi = 'Arial' w:hAnsiTheme='majorHAnsi'/></w:rPr><w:t>one</w:t></w:r></w:p>",
    );
    assert!(matches!(e.set_theme_font(ThemeFont::Minor, fonts()), Outcome::Changed(_)));
    let xml = doc_xml(&e);
    let doc = roxmltree::Document::parse(&xml).expect("well-formed");
    let w = crate::docx::write::props::W_NS;
    let f = doc.descendants().find(|n| n.has_tag_name((w, "rFonts"))).expect("fonts");
    assert_eq!((f.attribute((w, "ascii")), f.attribute((w, "hAnsi"))), (None, None), "{xml}");
    assert_eq!((f.attribute((w, "asciiTheme")), f.attribute((w, "hAnsiTheme"))), (Some("minorHAnsi"), Some("minorHAnsi")), "{xml}");
}

#[test]
fn a_page_number_in_the_text_can_t_be_left_off_the_first_page() {
    use crate::api::command::PageNumberFormat;
    use crate::editing::commands::PageNumberSpot;
    let mut e = edit(TWO);
    let (depth, titled) = (e.history().undo_depth(), e.document().sections[0].title_pg);
    let out = e.insert_page_number_shown(PageNumberFormat::Page, PageNumberSpot::Caret, None, false, fonts());
    assert!(matches!(out, Outcome::Refused(_)), "{out:?}");
    assert_eq!((e.history().undo_depth(), e.document().sections[0].title_pg), (depth, titled), "nothing changed");
    let out = e.insert_page_number_shown(PageNumberFormat::Page, PageNumberSpot::Caret, None, true, fonts());
    assert!(matches!(out, Outcome::Changed(_)), "shown on the first page it's fine: {out:?}");
}

#[test]
fn a_page_number_in_the_first_pages_own_header_can_t_be_left_off_the_first_page() {
    use super::empty_stories::{NS, build};
    use crate::api::command::PageNumberFormat;
    use crate::editing::commands::PageNumberSpot;
    let hdr = |t: &str| format!(r#"<w:hdr {NS}><w:p><w:r><w:t>{t}</w:t></w:r></w:p></w:hdr>"#);
    let (default, first) = (hdr("Default"), hdr("First"));
    let sect = r#"<w:headerReference w:type="default" r:id="rIdP0"/><w:headerReference w:type="first" r:id="rIdP1"/><w:titlePg/>"#;
    let parts = [("header1.xml", "header", "header", default.as_str()), ("header2.xml", "header", "header", first.as_str())];
    let mut e = Editor::open(&build("<w:p><w:r><w:t>Body</w:t></w:r></w:p>", sect, &parts), fonts()).expect("opens");
    e.complete(fonts());
    let caret_in = |e: &mut Editor, slot: usize| {
        let name = e.document().sections[0].headers[slot].clone().expect("a header");
        let story = e.index().header_footer(&name).expect("its story");
        let Some(crate::docx::Block::Para(p)) = e.document().stories[&name].first() else { panic!("a paragraph") };
        e.selection = Selection::Caret(Position::text(story, crate::layout::ParaId(p.id), crate::layout::Unit(0)));
    };
    caret_in(&mut e, 1);
    let depth = e.history().undo_depth();
    let out = e.insert_page_number_shown(PageNumberFormat::Page, PageNumberSpot::Caret, None, false, fonts());
    assert!(matches!(out, Outcome::Refused(_)), "{out:?}");
    assert_eq!((e.history().undo_depth(), e.document().sections[0].title_pg), (depth, true), "nothing changed");
    caret_in(&mut e, 0);
    let out = e.insert_page_number_shown(PageNumberFormat::Page, PageNumberSpot::Caret, None, false, fonts());
    assert!(matches!(out, Outcome::Changed(_)), "the other pages' header takes it: {out:?}");
    assert!(e.document().sections[0].title_pg, "the first page still its own");
}
