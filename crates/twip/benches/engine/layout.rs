//! Layout without the editor: a whole document from scratch, again with a warm cache, and one keystroke laid out
//! incrementally.

use std::cell::RefCell;

use divan::Bencher;
use twip::internal::docx::{Block, Inline};
use twip::internal::font::Fonts;
use twip::internal::layout::cache::LayoutCache;
use twip::internal::layout::ids::Revision;
use twip::internal::layout::{Change, DocumentLayout, Target, layout, layout_with};

const DOCS: [&str; 2] = ["big.docx", "docx-feature-showcase.docx"];
const LONG: &str = "editor/FX-LONG-50.docx";

fn read(name: &str) -> twip::internal::docx::Document {
    twip::internal::docx::read(&super::fixture(name)).expect("the fixture reads")
}

/// A whole document from scratch. Budget: at least 25% below `main` at 4131d3b on the documents both lay out (spec
/// SC-006).
#[divan::bench(args = ["big.docx", "docx-feature-showcase.docx", "editor/FX-LONG-50.docx", "editor/FX-LONG-100.docx"], sample_count = 20)]
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

/// Enter on page 1 of a 50-page document (a copy of its third paragraph inserted after it), laid out until it settles;
/// the copy is removed again between samples, untimed. Budget: under a quarter of `full` on the same document (spec
/// SC-004).
#[divan::bench(sample_size = 1)]
fn reflow(bencher: Bencher) {
    let (fonts, doc) = (Fonts::system(), RefCell::new(read(LONG)));
    let mut dl = DocumentLayout::new();
    let mut rev = 1;
    dl.edit(Revision(rev), None);
    dl.complete(&doc.borrow(), &fonts);
    let (dl, mut inserted) = (RefCell::new(dl), false);
    bencher
        .with_inputs(|| {
            let (mut d, mut dl) = (doc.borrow_mut(), dl.borrow_mut());
            let blocks = &mut d.sections[0].blocks;
            if std::mem::replace(&mut inserted, true) {
                blocks.remove(3);
                rev += 1;
                dl.edit(Revision(rev), Some(Change { section: 0, first: 2, old_end: 4, new_end: 3, row: None }));
                dl.complete(&d, &fonts);
            }
            let Block::Para(mut copy) = d.sections[0].blocks[2].clone() else { panic!("a paragraph on page 1") };
            copy.id = twip::internal::docx::model::next_para_id();
            copy.src = None;
            d.sections[0].blocks.insert(3, Block::Para(copy));
            rev += 1;
            dl.edit(Revision(rev), Some(Change { section: 0, first: 2, old_end: 3, new_end: 4, row: None }));
        })
        .bench_local_values(|()| dl.borrow_mut().complete(&doc.borrow(), &fonts));
}

/// Opening: the document read from its bytes, then laid out through its first page (what the first frame waits for).
/// Budget: the showcase 150 ms and FX-LONG-100 200 ms, the browser's start-up share included (spec SC-006).
#[divan::bench(args = ["docx-feature-showcase.docx", "editor/FX-LONG-100.docx"], sample_count = 20)]
fn first_page(bencher: Bencher, name: &str) {
    let (fonts, bytes) = (Fonts::system(), super::fixture(name));
    bencher.bench_local(|| {
        let doc = twip::internal::docx::read(&bytes).expect("the fixture reads");
        let mut dl = DocumentLayout::new();
        dl.edit(Revision(1), None);
        dl.through(&doc, &fonts, Target::Page(0));
        dl
    });
}
