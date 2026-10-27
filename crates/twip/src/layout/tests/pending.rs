//! Edits while a job is pending: provisional pages are displayed but never
//! restart points or reuse candidates (candidates come only from the baseline), and the completed layout equals a
//! full one.

use std::path::Path;

use crate::docx::{Block, Inline};
use crate::layout::ids::Revision;
use crate::layout::{Budget, Change, DocumentLayout, LayoutStatus, Step, dump, layout};

use super::{doc, fonts};

#[test]
fn edits_while_pending() {
    let Some(mut d) = doc(Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/big.docx"))) else { return };
    let fonts = fonts();
    let mut dl = DocumentLayout::new();
    dl.edit(Revision(1), None);
    dl.complete(&d, &fonts);
    // Enter near the start, laid out only a little: the job stays pending.
    let Block::Para(p) = d.sections[0].blocks[2].clone() else { return };
    let mut q = p;
    q.id = crate::docx::model::next_para_id();
    q.src = None;
    d.sections[0].blocks.insert(3, Block::Para(q));
    dl.edit(Revision(2), Some(Change { section: 0, first: 2, old_end: 3, new_end: 4, row: None }));
    // Budgets in pages: units count cache misses, so how far a unit budget gets depends on what the cache kept.
    assert_eq!(dl.run(&d, &fonts, Budget::Pages(2)), Step::Yielded);
    // Typing near page 90, three times, before the job completes.
    let n = d.sections[0].blocks.len();
    let far = (n * 90 / 107..n)
        .find(|&i| matches!(&d.sections[0].blocks[i], Block::Para(p) if p.inlines.iter().any(|x| matches!(x, Inline::Text(..)))))
        .expect("a text paragraph");
    for (k, rev) in [3u64, 4, 5].into_iter().enumerate() {
        let Block::Para(p) = &mut d.sections[0].blocks[far] else { unreachable!() };
        if let Some(Inline::Text(t, _)) = p.inlines.iter_mut().find(|x| matches!(x, Inline::Text(..))) {
            t.push_str("abc");
        }
        dl.edit(Revision(rev), Some(Change { section: 0, first: far, old_end: far + 1, new_end: far + 1, row: None }));
        if k < 2 {
            dl.run(&d, &fonts, Budget::Pages(2));
            assert!(matches!(dl.snapshot().status(), LayoutStatus::Pending { .. }));
        }
    }
    dl.complete(&d, &fonts);
    let s = dl.snapshot();
    assert_eq!(*s.status(), LayoutStatus::Converged);
    assert_eq!(s.revision(), Revision(5));
    assert_eq!(dump::page_text(&s, &d), dump::page_text(&layout(&d, &fonts), &d));
}
