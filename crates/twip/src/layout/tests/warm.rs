//! A warm cache gives the same layout as a fresh one, also after an edit inside a table, where cells,
//! tables and their ancestors are cached by their children's identity. Verify mode (on in tests) also recomputes
//! every hit.

use std::path::Path;

use super::{doc, fonts};
use crate::docx::{Block, Inline};
use crate::layout::cache::LayoutCache;
use crate::layout::{dump, layout, layout_with};

const DOCS: [&str; 2] = [
    concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/tables.docx"),
    concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/table-style-normal.docx"),
];

/// Appends `s` to the first text run of the first table cell found (nested tables included), depth first.
fn edit_first_cell(blocks: &mut [Block], s: &str) -> bool {
    for b in blocks {
        if let Block::Table(t) = b {
            for r in &mut t.rows {
                for c in &mut r.cells {
                    if edit_first_cell(&mut c.blocks, s) {
                        return true;
                    }
                    for cb in &mut c.blocks {
                        if let Block::Para(p) = cb
                            && let Some(Inline::Text(text, _)) = p.inlines.iter_mut().find(|i| matches!(i, Inline::Text(..)))
                        {
                            text.push_str(s);
                            return true;
                        }
                    }
                }
            }
        }
    }
    false
}

#[test]
fn warm_cache_equals_fresh_layout_after_a_cell_edit() {
    let fonts = fonts();
    for path in DOCS {
        let Some(mut d) = doc(Path::new(path)) else { continue };
        let mut cache = LayoutCache::new();
        let fresh = dump::page_text(&layout(&d, &fonts), &d);
        assert_eq!(dump::page_text(&layout_with(&d, &fonts, &mut cache), &d), fresh, "{path}: cold cache");
        assert_eq!(dump::page_text(&layout_with(&d, &fonts, &mut cache), &d), fresh, "{path}: warm cache");
        let edited = d.sections.iter_mut().any(|s| edit_first_cell(&mut s.blocks, " and some more words in this cell"));
        assert!(edited, "{path} has a table cell with text");
        let fresh = dump::page_text(&layout(&d, &fonts), &d);
        assert_eq!(dump::page_text(&layout_with(&d, &fonts, &mut cache), &d), fresh, "{path}: warm cache after a cell edit");
    }
}

/// With a small budget, least recently used entries are evicted: results stay equal to a fresh layout,
/// and once enforced the cache's bytes are within the budget.
#[test]
fn eviction_keeps_results_and_budget() {
    let Some(mut d) = doc(Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/tables.docx"))) else { return };
    let fonts = fonts();
    const BUDGET: usize = 64 << 10;
    let mut cache = LayoutCache::with_budget(BUDGET);
    for k in 0..3 {
        assert_eq!(dump::page_text(&layout_with(&d, &fonts, &mut cache), &d), dump::page_text(&layout(&d, &fonts), &d), "layout {k}");
        d.sections.iter_mut().any(|s| edit_first_cell(&mut s.blocks, " more"));
    }
    let before: usize = cache.report().iter().map(|r| r.2 + r.3).sum();
    cache.enforce_budget();
    let after: usize = cache.report().iter().map(|r| r.2 + r.3).sum();
    assert!(before > BUDGET, "the layouts filled the cache past the budget ({before} bytes)");
    assert!(after <= BUDGET, "cache bytes after eviction: {after} > {BUDGET}");
    assert_eq!(dump::page_text(&layout_with(&d, &fonts, &mut cache), &d), dump::page_text(&layout(&d, &fonts), &d), "after eviction");
}
