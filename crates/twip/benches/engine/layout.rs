//! Layout without the editor: a whole document from scratch, again with a warm cache, and one keystroke laid out
//! incrementally.

use std::cell::RefCell;

use divan::Bencher;
use twip::internal::docx::{Block, Inline};
use twip::internal::font::Fonts;
use twip::internal::layout::cache::LayoutCache;
use twip::internal::layout::ids::Revision;
use twip::internal::layout::{Change, DocumentLayout, layout, layout_with};

const DOCS: [&str; 2] = ["big.docx", "docx-feature-showcase.docx"];

fn read(name: &str) -> twip::internal::docx::Document {
    twip::internal::docx::read(&super::fixture(name)).expect("the fixture reads")
}

#[divan::bench(args = DOCS, sample_count = 20)]
fn full(bencher: Bencher, name: &str) {
    let (fonts, doc) = (Fonts::system(), read(name));
    bencher.bench_local(|| layout(&doc, &fonts));
}

#[divan::bench(args = DOCS, sample_count = 20)]
fn cached(bencher: Bencher, name: &str) {
    let (fonts, doc) = (Fonts::system(), read(name));
    let mut cache = LayoutCache::new();
    layout_with(&doc, &fonts, &mut cache);
    bencher.bench_local(|| layout_with(&doc, &fonts, &mut cache));
}

/// The first text in a block (a paragraph or, depth-first, a table cell's).
fn first_text(b: &mut Block) -> Option<&mut String> {
    match b {
        Block::Para(p) => p.inlines.iter_mut().find_map(|i| match i {
            Inline::Text(t, _) => Some(t),
            _ => None,
        }),
        Block::Table(t) => t.rows.iter_mut().flat_map(|r| r.cells.iter_mut()).flat_map(|c| c.blocks.iter_mut()).find_map(first_text),
    }
}

/// A character inserted in the middle paragraph (untimed), then laid out incrementally with the editor's hint.
#[divan::bench(args = DOCS, sample_size = 1)]
fn keystroke(bencher: Bencher, name: &str) {
    let (fonts, doc) = (Fonts::system(), RefCell::new(read(name)));
    let mut dl = DocumentLayout::new();
    let mut rev = 1;
    dl.edit(Revision(rev), None);
    dl.complete(&doc.borrow(), &fonts);
    let (section, at) = {
        let d = doc.borrow();
        let section = d.sections.len() / 2;
        let blocks = &d.sections[section].blocks;
        let mid = blocks.len() / 2;
        (section, (mid..blocks.len()).find(|&i| matches!(blocks[i], Block::Para(_))).unwrap_or(mid))
    };
    let dl = RefCell::new(dl);
    bencher
        .with_inputs(|| {
            if let Some(t) = doc.borrow_mut().sections[section].blocks.get_mut(at).and_then(first_text) {
                t.insert(0, 'x');
            }
            rev += 1;
            dl.borrow_mut().edit(Revision(rev), Some(Change { section, first: at, old_end: at + 1, new_end: at + 1, row: None }));
        })
        .bench_local_values(|()| dl.borrow_mut().complete(&doc.borrow(), &fonts));
}
