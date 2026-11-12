//! Vertical spacing

use std::rc::Rc;

use crate::docx::model::BreakKind;
use crate::docx::{Block, Document, Inline};
use crate::layout::fragment::{Child, PageChild};
use crate::layout::layout as layout_doc;

use super::{fonts, page_chars, plain_doc, words};

fn para_mut(doc: &mut Document, i: usize) -> &mut crate::docx::Paragraph {
    let Block::Para(p) = &mut doc.sections[0].blocks[i] else { unreachable!() };
    p
}

fn first_para(doc: &Document, k: usize) -> (f64, usize) {
    fn first(children: &[Child]) -> Option<(f64, usize)> {
        children.iter().find_map(|c| match c {
            Child::Para(p) => p.lines.first().map(|l| (l.y, p.lines.len())),
            Child::Box(b) => first(&b.children),
            _ => None,
        })
    }
    let snap = layout_doc(doc, &fonts());
    snap.pages()[k].children.iter().find_map(|c| if let PageChild::Body(b) = c { first(&b.children) } else { None }).expect("a paragraph")
}

fn page_2_top(doc: &Document) -> f64 {
    first_para(doc, 1).0
}

fn broken(after: f64, before: f64) -> Option<Document> {
    let mut doc = plain_doc(&[words(3), words(3)])?;
    Rc::make_mut(&mut para_mut(&mut doc, 0).style).space_after = after;
    let p = para_mut(&mut doc, 1);
    Rc::make_mut(&mut p.style).space_before = before;
    let Inline::Text(_, run) = p.inlines[0].clone() else { unreachable!() };
    p.inlines.insert(0, Inline::Break(BreakKind::Page, run));
    Some(doc)
}

#[test]
fn para_2_a_page_break_first_line_keeps_what_space_after_leaves_of_space_before() {
    let (Some(none), Some(kept), Some(eaten)) = (broken(0.0, 0.0), broken(0.0, 6.0), broken(6.0, 6.0)) else { return };
    let top = page_2_top(&none);
    assert!((page_2_top(&kept) - top - 6.0).abs() < 1e-6, "no space-after before it: all 6pt stay");
    assert!((page_2_top(&eaten) - top).abs() < 1e-6, "a 6pt space-after takes all of it");
}

#[test]
fn para_2_a_page_break_after_text_drops_the_space_before() {
    let (Some(none), Some(mut mid)) = (broken(0.0, 0.0), broken(0.0, 6.0)) else { return };
    let p = para_mut(&mut mid, 1);
    let first = p.inlines[1].clone();
    p.inlines.swap(0, 1);
    p.inlines.push(first);
    assert!((page_2_top(&mid) - page_2_top(&none)).abs() < 1e-6, "text before the break: the 6pt go");
}

#[test]
fn an_at_least_line_at_the_page_top_rises_with_the_top_spacing_options() {
    let top = |mode: i64, option: &str| {
        // A page break ends the first paragraph; the second starts page 2 without its space-before.
        let mut doc = broken(0.0, 12.0)?;
        let brk = para_mut(&mut doc, 1).inlines.remove(0);
        para_mut(&mut doc, 0).inlines.push(brk);
        doc.settings.compat_mode = mode;
        if !option.is_empty() {
            doc.settings.compat.insert(option.to_string());
        }
        let s = Rc::make_mut(&mut para_mut(&mut doc, 1).style);
        (s.line_rule, s.line) = (crate::docx::props::LineRule::AtLeast, 40.0);
        Some(page_2_top(&doc))
    };
    let Some(plain) = top(12, "") else { return };
    // The 40pt line keeps 9.6pt of its height (both modes); the Mac rule drops what it has beyond its text (30.48pt).
    for mode in [12, 15] {
        assert!((plain - top(mode, "suppressTopSpacing").unwrap() - 30.4).abs() < 1e-6, "compat {mode}");
    }
    let mac = plain - top(12, "suppressSpacingAtTopOfPage").unwrap();
    assert!((mac - 9.52).abs() < 0.01, "{mac}");
    assert!((top(15, "suppressSpacingAtTopOfPage").unwrap() - top(15, "").unwrap()).abs() < 1e-6);
}

#[test]
fn a_page_top_line_that_keeps_its_space_before_rises_too() {
    let top = |option: &str, page: usize| {
        // Paragraph 2 starts with a page break: its text line keeps its 12pt space-before on page 2. Compat 12.
        let mut doc = broken(0.0, 12.0)?;
        doc.settings.compat_mode = 12;
        if !option.is_empty() {
            doc.settings.compat.insert(option.to_string());
        }
        for i in 0..2 {
            let s = Rc::make_mut(&mut para_mut(&mut doc, i).style);
            (s.line_rule, s.line) = (crate::docx::props::LineRule::AtLeast, 40.0);
        }
        Some(first_para(&doc, page).0)
    };
    let (Some(first), Some(plain)) = (top("", 0), top("", 1)) else { return };
    // The document's first paragraph and the kept-space line both rise by the line's height beyond 9.6pt.
    assert!((first - top("suppressTopSpacing", 0).unwrap() - 30.4).abs() < 1e-6);
    assert!((plain - top("suppressTopSpacing", 1).unwrap() - 30.4).abs() < 1e-6);
    // `suppressSpBfAfterPgBrk`: the space-before after the leading page break goes.
    assert!((plain - top("suppressSpBfAfterPgBrk", 1).unwrap() - 12.0).abs() < 1e-6);
}

fn after_continuous_break(after: f64) -> Option<f64> {
    let mut doc = plain_doc(&[words(3), words(3)])?;
    Rc::make_mut(&mut para_mut(&mut doc, 0).style).space_after = after;
    Rc::make_mut(&mut para_mut(&mut doc, 1).style).space_before = 0.0;
    let mut next = doc.sections[0].clone();
    next.break_kind = crate::docx::model::SectionBreak::Continuous;
    next.blocks = vec![doc.sections[0].blocks.remove(1)];
    doc.sections.push(next);
    let chars = &page_chars(&doc)[0];
    let first = chars.first()?.2;
    chars.iter().map(|c| c.2).find(|&y| y > first + 1.0)
}

#[test]
fn page_10_a_continuous_section_keeps_the_space_after_before_it() {
    let (Some(none), Some(twelve)) = (after_continuous_break(0.0), after_continuous_break(12.0)) else { return };
    assert!((twelve - none - 12.0).abs() < 1e-6, "{none} {twelve}");
}

#[test]
fn para_4_contextual_spacing_across_cells_rows_and_table_edges() {
    let ys = |ctx: bool| {
        let c = if ctx { "<w:contextualSpacing/>" } else { "" };
        let p = |t: char, sp: &str| format!(r#"<w:p><w:pPr><w:spacing {sp}/>{c}</w:pPr><w:r><w:t>{t}</w:t></w:r></w:p>"#);
        let plain = |t: char| format!(r#"<w:p><w:pPr><w:spacing w:after="0"/></w:pPr><w:r><w:t>{t}</w:t></w:r></w:p>"#);
        let tc = |x: String| format!(r#"<w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/></w:tcPr>{x}</w:tc>"#);
        let before = r#"w:before="240" w:after="0""#;
        let body = format!(
            r#"{}<w:tbl><w:tblGrid><w:gridCol w:w="3000"/><w:gridCol w:w="3000"/></w:tblGrid><w:tr>{}{}</w:tr><w:tr>{}{}</w:tr></w:tbl>{}"#,
            p('a', r#"w:after="120""#),
            tc(plain('b')),
            tc(p('c', before)),
            tc(p('e', before)),
            tc(p('f', r#"w:after="120""#)),
            p('d', before)
        );
        let doc = crate::docx::read(&super::rows::with_body(&crate::docx::blank::blank_docx(), &body)).expect("reads");
        let chars = &super::rows::glyphs(&doc)[0];
        let y = |ch: char| chars.iter().find(|x| x.0 == ch).map(|x| x.1).expect("drawn");
        (y('b') - y('a'), y('c') - y('b'), y('e') - y('b'), y('d') - y('e'))
    };
    let (plain, ctx) = (ys(false), ys(true));
    assert!((plain.0 - ctx.0 - 6.0).abs() < 1e-6, "a's space-after goes against b: {plain:?} {ctx:?}");
    assert!((plain.1 - 12.0).abs() < 1e-6 && ctx.1.abs() < 1e-6, "c's space-before goes against b: {plain:?} {ctx:?}");
    assert!((plain.2 - ctx.2 - 24.0).abs() < 1e-6, "the first row loses c's 12pt, e its own: {plain:?} {ctx:?}");
    assert!((plain.3 - ctx.3 - 12.0).abs() < 1e-6, "f's space-after and d's space-before go: {plain:?} {ctx:?}");
}

#[test]
fn tab_7_a_default_stop_past_the_right_edge_starts_a_line() {
    let lines = |compat: i64| {
        let mut doc = plain_doc(&[words(3)])?;
        doc.settings.compat_mode = compat;
        let p = para_mut(&mut doc, 0);
        Rc::make_mut(&mut p.style).tabs.clear();
        let Inline::Text(_, run) = p.inlines[0].clone() else { unreachable!() };
        p.inlines.extend((0..20).map(|_| Inline::Tab(run.clone())));
        Some(first_para(&doc, 0).1)
    };
    let (Some(old), Some(new)) = (lines(14), lines(15)) else { return };
    assert_eq!((old, new), (2, 2));
}

fn custom_past_edge(compat: i64, kinds: &[crate::docx::props::TabKind]) -> Option<usize> {
    use crate::docx::props::{Leader, Tab};
    let mut doc = plain_doc(&["A".to_string()])?;
    doc.settings.compat_mode = compat;
    let p = para_mut(&mut doc, 0);
    Rc::make_mut(&mut p.style).tabs = kinds.iter().map(|&kind| Tab { pos: 1000.0, kind, leader: Leader::None }).collect();
    let Inline::Text(_, run) = p.inlines[0].clone() else { unreachable!() };
    for _ in kinds {
        p.inlines.extend([Inline::Tab(run.clone()), Inline::Text("Bb".into(), run.clone())]);
    }
    Some(first_para(&doc, 0).1)
}

#[test]
fn tab_7_compat_14_text_after_a_custom_stop_past_the_edge_stays_on_the_line() {
    use crate::docx::props::TabKind::{Center, Left};
    let (Some(left), Some(centre)) = (custom_past_edge(14, &[Left, Left]), custom_past_edge(14, &[Center])) else { return };
    assert_eq!((left, centre), (1, 1));
}

#[test]
fn tab_7_compat_15_a_custom_left_stop_past_the_edge_costs_a_line() {
    let Some(n) = custom_past_edge(15, &[crate::docx::props::TabKind::Left, crate::docx::props::TabKind::Left]) else { return };
    assert_eq!(n, 5);
}

#[test]
fn tab_7_compat_15_a_centre_stop_past_the_edge_moves_to_it_unless_a_tab_follows() {
    use crate::docx::props::TabKind::{Center, Right};
    let (Some(centre), Some(right)) = (custom_past_edge(15, &[Center]), custom_past_edge(15, &[Right, Right])) else { return };
    assert_eq!((centre, right), (1, 3));
}

#[test]
fn tab_6_text_after_a_centre_stop_near_the_edge_ends_at_it_on_the_line() {
    use crate::docx::props::{Leader, Tab, TabKind};
    let lines = |compat: i64| {
        let mut doc = plain_doc(&["A".to_string()])?;
        doc.settings.compat_mode = compat;
        let p = para_mut(&mut doc, 0);
        Rc::make_mut(&mut p.style).tabs = vec![Tab { pos: 450.0, kind: TabKind::Center, leader: Leader::None }];
        let Inline::Text(_, run) = p.inlines[0].clone() else { unreachable!() };
        p.inlines.extend([Inline::Tab(run.clone()), Inline::Text(words(3), run)]);
        Some(first_para(&doc, 0).1)
    };
    let (Some(old), Some(new)) = (lines(14), lines(15)) else { return };
    assert_eq!((old, new), (1, 1));
}

fn decimal_x(value: &str, c: char) -> Option<f64> {
    use crate::docx::props::{Leader, Tab, TabKind};
    let mut doc = plain_doc(&["Item".to_string()])?;
    let p = para_mut(&mut doc, 0);
    Rc::make_mut(&mut p.style).tabs = vec![Tab { pos: 200.0, kind: TabKind::Decimal, leader: Leader::None }];
    let Inline::Text(_, run) = p.inlines[0].clone() else { unreachable!() };
    p.inlines.extend([Inline::Tab(run.clone()), Inline::Text(value.into(), run)]);
    let chars = &page_chars(&doc)[0];
    Some(chars.iter().find(|ch| ch.0 == c)?.1 - chars.first()?.1)
}

#[test]
fn tab_5_a_decimal_tab_aligns_the_point_not_a_comma() {
    let (Some(point), Some(comma)) = (decimal_x("1,250.00", '.'), decimal_x("1,250", ',')) else { return };
    assert!((point - 200.0).abs() < 1e-6, "{point}");
    assert!(comma < 195.0, "{comma}");
}

#[test]
fn line_14_the_extra_height_is_whole_font_units() {
    let fonts = fonts();
    if !fonts.is_installed("MS Mincho") {
        return;
    }
    let Some(f) = fonts.face("MS Mincho", None, false, false) else { return };
    assert_eq!((f.ascent, f.descent, f.leading), (258.0, 74.0, 0.0));
}
