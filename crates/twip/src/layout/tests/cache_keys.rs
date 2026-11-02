//! A dependency left out of a cache key is caught by verify mode: with the fault flag on, the lines key omits
//! `ParaSpace.width`, so the second width hits the first width's lines and the recomputation differs.

use crate::docx::Block;
use crate::layout::cache::LayoutCache;
use crate::layout::env::LayoutEnv;
use crate::layout::job::{Memos, Meter};
use crate::layout::line::{OMIT_WIDTH, break_lines, neighbour_borders};
use crate::layout::space::{ParaSpace, ShapeSpace};
use crate::layout::text::shape;

use super::{fonts, plain_doc, words};

/// `catch_unwind` rather than `#[should_panic]`: the test skips itself without fixture fonts, as the others do.
#[test]
fn omitted_key_is_caught() {
    let Some(doc) = plain_doc(&[words(300)]) else { return };
    let Block::Para(p) = &doc.sections[0].blocks[0] else { unreachable!() };
    let fonts = fonts();
    let env = LayoutEnv::new(&doc);
    let mut cache = LayoutCache::new();
    let mut meter = Meter::unlimited();
    let mut memos = Memos { cache: &mut cache, job: None, meter: &mut meter };
    let shaped = shape(p, &ShapeSpace::default(), &fonts, &mut memos).expect("unlimited");
    OMIT_WIDTH.set(true);
    let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        for width in [468.0, 300.0] {
            let space = ParaSpace { width, start: 0, borders: neighbour_borders(&p.style, None, None), x0: 0.0, env: env.text.clone(), fit: false };
            break_lines(&shaped, p, &space, &fonts, &mut memos).expect("unlimited");
        }
    }));
    OMIT_WIDTH.set(false);
    let message = caught.expect_err("verify mode catches the omitted width");
    let message = message.downcast_ref::<String>().map(String::as_str).unwrap_or_default();
    assert!(message.contains("boundary `lines`"), "{message}");
}

/// Shaping depends on the markup view (deleted text shapes in one view and not in another), so the view is in
/// the shaping key, and in every key that holds the text settings.
#[test]
fn markup_view_is_in_the_keys() {
    use crate::docx::review::MarkupView;
    use crate::layout::env::TextEnv;
    use crate::layout::text::{FieldValues, shape_space};
    let Some(mut doc) = plain_doc(&[words(3)]) else { return };
    let Block::Para(p) = doc.sections[0].blocks[0].clone() else { unreachable!() };
    let all = TextEnv::new(&doc);
    doc.review.view = MarkupView::NoMarkup;
    let none = TextEnv::new(&doc);
    assert_ne!(all, none);
    let notes = Default::default();
    assert_ne!(shape_space(&p, &FieldValues::default(), &notes, &all, None), shape_space(&p, &FieldValues::default(), &notes, &none, None));
}

/// Live layout's preview is in the shape keys of the paragraphs it concerns only: the one anchoring it in the preview
/// (the float in place of its own, or added) and its anchor in the document (left out there), so every other
/// paragraph keeps its caches.
#[test]
fn a_preview_is_in_the_keys_of_its_paragraphs_only() {
    use crate::docx::{Inline, Paragraph};
    use crate::layout::text::{FieldValues, shape_space};
    use crate::layout::{ParaId, Preview};
    let Some(doc) = super::doc(std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/editor/FX-OBJ.docx"))) else { return };
    let doc = &doc;
    let paras: Vec<&Paragraph> = doc.sections[0].blocks.iter().filter_map(|b| if let Block::Para(p) = b { Some(p) } else { None }).collect();
    let float_of = |p: &Paragraph| p.inlines.iter().find_map(|i| if let Inline::Float(f, _) = i { Some((**f).clone()) } else { None });
    let (home, f) = paras.iter().find_map(|p| float_of(p).map(|f| (*p, f))).expect("a float");
    let other = *paras.iter().find(|p| float_of(p).is_none()).expect("a paragraph without one");
    let env = LayoutEnv::new(doc);
    let (fields, notes) = (FieldValues::default(), Default::default());
    let space = |p: &Paragraph, v: Option<&Preview>| shape_space(p, &fields, &notes, &env.text, v);
    let mut moved = f.clone();
    moved.v_offset += 36.0;
    // In place: its paragraph's key changes, no other's.
    let here = Preview { float: moved.clone(), anchor: ParaId(home.id) };
    assert_ne!(space(home, Some(&here)), space(home, None));
    assert_eq!(space(other, Some(&here)), space(other, None));
    // Re-anchored: the new anchor adds it, the old one leaves it out.
    let there = Preview { float: moved, anchor: ParaId(other.id) };
    assert_eq!(space(other, Some(&there)).float.map(|g| g.id), Some(f.id));
    assert_eq!(space(home, Some(&there)).hide, Some(f.id));
}
