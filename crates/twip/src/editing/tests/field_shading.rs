//! Field shading

use super::super::Editor;
use super::super::selection::Selection;
use super::{fonts, open, texts};
use crate::api::services::FieldShading;
use crate::layout::ParaId;

type Rects = Vec<(usize, [f64; 4])>;

fn fixture() -> Editor {
    let mut e = open("FX-SHADING.docx");
    e.complete(fonts());
    e
}

fn para(e: &Editor, starts: &str) -> ParaId {
    let at = texts(e).iter().position(|t| t.starts_with(starts)).unwrap_or_else(|| panic!("no paragraph {starts:?}"));
    super::para_id(e, at)
}

/// `para`'s lines: page, top, bottom and the paragraph's left and right on the line.
fn lines(e: &mut Editor, para: ParaId) -> Vec<(usize, [f64; 4])> {
    let geo = e.page_geometries();
    geo.iter()
        .flat_map(|(pg, g)| {
            g.lines_of(para).filter_map(|(_, l)| l.span_of(para).map(|(x0, x1)| (*pg, [x0, l.top, x1, l.bottom]))).collect::<Vec<_>>()
        })
        .collect()
}

/// The rects lying on `para`'s lines.
fn on(e: &mut Editor, rects: &Rects, para: ParaId) -> Rects {
    let lines = lines(e, para);
    rects
        .iter()
        .filter(|(pg, r)| lines.iter().any(|(lp, l)| lp == pg && (l[1] - r[1]).abs() < 0.01 && r[0] < l[2] + 0.01 && l[0] < r[2] + 0.01))
        .copied()
        .collect()
}

fn all_pages(e: &mut Editor, mode: FieldShading) -> Rects {
    let n = e.page_geometries().len();
    e.field_shading(0..n, mode)
}

fn caret_on(e: &mut Editor, (page, r): (usize, [f64; 4])) {
    let (x, y) = e.stack().to_doc(page, (r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0).expect("on the stack");
    let (p, ..) = e.hit_test(x, y, fonts()).expect("a position");
    e.selection = Selection::Caret(p);
}

#[test]
fn always_shades_every_field_and_no_plain_text() {
    let mut e = fixture();
    let all = all_pages(&mut e, FieldShading::Always);
    let two = para(&e, "Plain before");
    let shaded = on(&mut e, &all, two);
    assert_eq!(shaded.len(), 2, "two fields, one rect each: {shaded:?}");
    let line_start = lines(&mut e, two)[0].1[0];
    assert!(shaded.iter().all(|(_, r)| r[0] > line_start + 1.0), "the text before the first field isn't shaded");
    let wraps = para(&e, "Wraps");
    assert!(on(&mut e, &all, wraps).len() >= 2, "a wrapped field gets a rect per line");
    let nested = para(&e, "Nested");
    assert_eq!(on(&mut e, &all, nested).len(), 1, "a nested field shades once, with its outer field");
    for p in ["Highlight", "Cell"] {
        let id = para(&e, p);
        assert_eq!(on(&mut e, &all, id).len(), 1, "{p}");
    }
    for p in ["FX-SHADING", "Plain cell", "Plain text only."] {
        let id = para(&e, p);
        assert!(on(&mut e, &all, id).is_empty(), "{p} has no field");
    }
}

#[test]
fn never_shades_nothing() {
    let mut e = fixture();
    assert!(all_pages(&mut e, FieldShading::Never).is_empty());
}

#[test]
fn when_selected_shades_only_the_field_holding_the_caret() {
    let mut e = fixture();
    let all = all_pages(&mut e, FieldShading::Always);
    let two = para(&e, "Plain before");
    let second = on(&mut e, &all, two)[1];
    caret_on(&mut e, second);
    let revision = e.revision();
    assert_eq!(all_pages(&mut e, FieldShading::WhenSelected), vec![second], "the field holding the caret, and no other");
    assert_eq!(e.revision(), revision, "shading changes nothing in the document");

    let nested = para(&e, "Nested");
    let outer = on(&mut e, &all, nested)[0];
    caret_on(&mut e, outer);
    assert_eq!(all_pages(&mut e, FieldShading::WhenSelected), vec![outer], "the outer field of a nested one");

    let plain = para(&e, "Plain text only.");
    let line = lines(&mut e, plain)[0];
    caret_on(&mut e, line);
    assert!(all_pages(&mut e, FieldShading::WhenSelected).is_empty(), "plain text: no field shaded");
}

#[test]
fn always_shades_fields_in_headers_footers_text_boxes_and_notes() {
    use super::super::position::StoryId;
    let mut e = open("FX-MERGE-FIELDS.docx");
    e.complete(fonts());
    let all = all_pages(&mut e, FieldShading::Always);
    let geo = e.page_geometries();
    let story_of = |(page, r): &(usize, [f64; 4])| {
        let g = &geo.iter().find(|(pg, _)| pg == page).expect("a shaded page").1;
        let l = g.lines.iter().find(|l| (l.top - r[1]).abs() < 0.01 && l.span_of(l.paras[0]).is_some()).expect("a shaded line");
        g.stories[l.story].story
    };
    let stories: Vec<StoryId> = all.iter().map(story_of).collect();
    assert!(stories.iter().any(|s| matches!(s, StoryId::HeaderFooter(_))), "a header or footer field: {stories:?}");
    assert!(stories.iter().any(|s| matches!(s, StoryId::TextBox(_))), "a text box field: {stories:?}");
    assert!(stories.iter().any(|s| matches!(s, StoryId::Footnote(_))), "a footnote field: {stories:?}");
    assert!(stories.iter().any(|s| matches!(s, StoryId::Body)), "a body field");
}
