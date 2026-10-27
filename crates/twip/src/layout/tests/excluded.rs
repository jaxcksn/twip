//! Text pushed entirely below its page (HDR-9) is recorded in the snapshot's exclusion list and left out by every
//! traversal.

use super::{fixture, fonts};
use crate::layout::fragment::Item;
use crate::layout::layout;
use crate::layout::traverse::{Spatial, paint_items, semantic_chars, spatial};

#[test]
fn pushed_below_page_is_excluded_everywhere() {
    let Some(doc) = fixture(r"HF3\HF3_h60.docx") else { return };
    let s = layout(&doc, &fonts());
    assert!(!s.excluded().is_empty(), "HF3_h60 pushes body text below its page");
    for &(page, para, ref units) in s.excluded() {
        let p = &s.pages()[page];
        let mut chars = Vec::new();
        semantic_chars(p, &mut chars);
        assert!(!chars.iter().any(|&(q, u, _)| q == para && units.contains(&u)), "semantic traversal yields excluded text");
        let Spatial::Page { lines, .. } = spatial(p, false) else { unreachable!("not provisional") };
        assert!(
            !lines.iter().any(|l| l.para == Some(para) && l.units.is_some_and(|(a, b)| units.contains(&a) || units.contains(&b))),
            "spatial traversal yields excluded text"
        );
        let painted = paint_items(p)
            .into_iter()
            .any(|it| matches!(it, Item::Glyphs(r) if r.para == Some(para) && r.chars.iter().any(|c| c.2.is_some_and(|u| units.contains(&u)))));
        assert!(!painted, "paint traversal yields excluded text");
    }
}
