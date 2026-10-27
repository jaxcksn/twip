//! A paragraph continued on the next page has the lines it would have unsplit (a continuation slices
//! shaped pieces).

use crate::layout::cache::LayoutCache;
use crate::layout::fragment::{Child, Item, LineFragment, PageChild, PageFragment};
use crate::layout::ids::{ParaId, Unit};
use crate::layout::{layout, layout_with};

use super::{fonts, plain_doc, words};

/// A line's characters by caret unit, with their x relative to the line's first character.
fn units(line: &LineFragment) -> Vec<(Unit, i64)> {
    let mut out = Vec::new();
    for it in line.items.iter() {
        if let Item::Glyphs(r) = it {
            out.extend(r.chars.iter().filter_map(|&(_, x, u)| u.map(|u| (u, (x * 100.0).round() as i64))));
        }
    }
    let x0 = out.first().map_or(0, |c| c.1);
    out.iter().map(|&(u, x)| (u, x - x0)).collect()
}

/// The lines of paragraph `para` on a page, in order.
fn lines_of(page: &PageFragment, para: ParaId) -> Vec<Vec<(Unit, i64)>> {
    fn walk(children: &[Child], para: ParaId, out: &mut Vec<Vec<(Unit, i64)>>) {
        for c in children {
            match c {
                Child::Para(p) if p.para == para => out.extend(p.lines.iter().map(|l| units(&l.line))),
                Child::Box(b) => walk(&b.children, para, out),
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    for c in page.children.iter() {
        if let PageChild::Body(b) = c {
            walk(&b.children, para, &mut out);
        }
    }
    out
}

#[test]
fn continuation_equals_unsplit() {
    // A short filler paragraph first, so the long paragraph after it doesn't start at the top of page 1 and is
    // split across pages.
    let Some(doc) = plain_doc(&[words(10), words(200)]) else { return };
    let crate::docx::Block::Para(p) = &doc.sections[0].blocks[1] else { unreachable!() };
    let id = ParaId(p.id);
    let fonts = fonts();
    let mut cache = LayoutCache::new();
    let split = layout_with(&doc, &fonts, &mut cache);
    // Misses, not shaping calls: verify mode (on in tests) shapes again on every hit to check it.
    assert_eq!(cache.shaped.counts().1, 2, "each paragraph is shaped once, the continuation included");
    let parts: Vec<_> = split.pages().iter().map(|pg| lines_of(pg, id)).filter(|l| !l.is_empty()).collect();
    assert!(parts.len() >= 2, "the paragraph is split across pages");
    // The unsplit lines: the same paragraph broken at the same width on a page tall enough to hold it.
    let mut whole_doc = doc.clone();
    whole_doc.sections[0].blocks.remove(0);
    whole_doc.sections[0].page_h *= 4.0;
    let whole = layout(&whole_doc, &fonts);
    assert_eq!(whole.pages().len(), 1, "the tall page holds the whole paragraph");
    let unsplit = lines_of(&whole.pages()[0], id);
    let together: Vec<_> = parts.into_iter().flatten().collect();
    assert_eq!(together, unsplit, "the continued lines are the unsplit paragraph's lines");
}
