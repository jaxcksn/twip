//! Warm cache

use std::path::Path;

use super::{doc, fonts};
use crate::docx::{Block, Inline};
use crate::layout::cache::LayoutCache;
use crate::layout::dump;
use crate::layout::tests::{layout, layout_with};

const DOCS: [&str; 2] = [
    concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/tables.docx"),
    concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/table-style-normal.docx"),
];

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

#[test]
fn warm_cache_equals_fresh_layout_after_editing_a_turned_cell() {
    let fonts = fonts();
    let cell =
        |pr: &str, t: &str| format!(r#"<w:tc><w:tcPr><w:tcW w:w="2000" w:type="dxa"/>{pr}</w:tcPr><w:p><w:r><w:t>{t}</w:t></w:r></w:p></w:tc>"#);
    let turned = r#"<w:textDirection w:val="btLr"/>"#;
    let body = format!(
        r#"<w:tbl><w:tblPr><w:tblW w:w="4000" w:type="dxa"/><w:tblLayout w:type="fixed"/></w:tblPr><w:tblGrid><w:gridCol w:w="2000"/><w:gridCol w:w="2000"/></w:tblGrid><w:tr><w:trPr><w:trHeight w:val="2000"/></w:trPr>{}{}</w:tr></w:tbl><w:p/>"#,
        cell(turned, "Hello"),
        cell("", "plain")
    );
    let mut d = crate::docx::read(&super::rows::with_body(&crate::docx::blank::blank_docx(), &body)).expect("reads");
    let mut cache = LayoutCache::new();
    let _ = layout_with(&d, &fonts, &mut cache);
    let Some(Block::Table(t)) = d.sections[0].blocks.iter_mut().find(|b| matches!(b, Block::Table(_))) else { panic!("a table") };
    let Block::Para(p) = &mut t.rows[0].cells[0].blocks[0] else { panic!("a paragraph") };
    let Some(Inline::Text(text, _)) = p.inlines.iter_mut().find(|i| matches!(i, Inline::Text(..))) else { panic!("text") };
    *text = "World".into();
    assert_eq!(dump::page_text(&layout_with(&d, &fonts, &mut cache), &d), dump::page_text(&layout(&d, &fonts), &d));
}
