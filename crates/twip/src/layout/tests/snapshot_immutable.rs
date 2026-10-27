//! A published snapshot never changes, and provisional pages don't resolve positions until laid out.

use std::path::Path;

use crate::docx::{Block, Inline};
use crate::layout::ids::Revision;
use crate::layout::traverse::{Spatial, spatial};
use crate::layout::{Budget, Change, DocumentLayout, LayoutStatus, Step, Target, dump};

use super::random_edits::Rng;
use super::{doc, fonts};

/// A held snapshot's dump, status and revisions stay the same through 100 later edits.
#[test]
fn snapshot_unchanged_by_later_edits() {
    let Some(mut d) = doc(Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/editor/FX-OBJ.docx"))) else { return };
    let fonts = fonts();
    let mut dl = DocumentLayout::new();
    dl.edit(Revision(1), None);
    dl.complete(&d, &fonts);
    let held = dl.snapshot();
    let (text, status, rev, lrev) = (dump::page_text(&held, &d), held.status().clone(), held.revision(), held.layout_revision());
    let original = d.clone();
    let mut rng = Rng::new(42);
    let paras: Vec<usize> = (0..d.sections[0].blocks.len())
        .filter(|&i| matches!(&d.sections[0].blocks[i], Block::Para(p) if p.inlines.iter().any(|x| matches!(x, Inline::Text(..)))))
        .collect();
    for k in 0..100 {
        let bi = paras[rng.below(paras.len())];
        if let Block::Para(p) = &mut d.sections[0].blocks[bi]
            && let Some(Inline::Text(t, _)) = p.inlines.iter_mut().find(|x| matches!(x, Inline::Text(..)))
        {
            t.push_str(" edit");
        }
        dl.edit(Revision(k + 2), Some(Change { section: 0, first: bi, old_end: bi + 1, new_end: bi + 1, row: None }));
        dl.complete(&d, &fonts);
    }
    assert_ne!(dl.snapshot().layout_revision(), lrev, "later layouts were published");
    assert_eq!(dump::page_text(&held, &original), text);
    assert_eq!(*held.status(), status);
    assert_eq!((held.revision(), held.layout_revision()), (rev, lrev));
}

/// While a job is pending, provisional pages don't resolve spatially; `through` lays them out.
#[test]
fn provisional_pages_do_not_resolve() {
    let Some(mut d) = doc(Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/big.docx"))) else { return };
    let fonts = fonts();
    let mut dl = DocumentLayout::new();
    dl.edit(Revision(1), None);
    dl.complete(&d, &fonts);
    // Enter near the start leaves later pages provisional while the job runs.
    let Block::Para(p) = d.sections[0].blocks[2].clone() else { return };
    let mut q = p;
    q.id = crate::docx::model::next_para_id();
    q.src = None;
    q.inlines.clear();
    d.sections[0].blocks.insert(3, Block::Para(q));
    dl.edit(Revision(2), Some(Change { section: 0, first: 3, old_end: 3, new_end: 4, row: None }));
    // Small steps until a page is committed and a pending snapshot published (a step that commits none publishes
    // nothing).
    for _ in 0..100 {
        assert_eq!(dl.run(&d, &fonts, Budget::Units(1)), Step::Yielded, "the job finished in small steps");
        if matches!(dl.snapshot().status(), LayoutStatus::Pending { .. }) {
            break;
        }
    }
    let s = dl.snapshot();
    let LayoutStatus::Pending { stale_from } = *s.status() else { panic!("pending, got {:?}", s.status()) };
    let i = stale_from.min(s.pages().len() - 1);
    assert!(s.info()[i].provisional);
    assert!(matches!(spatial(&s.pages()[i], s.info()[i].provisional), Spatial::Provisional));
    dl.through(&d, &fonts, Target::Page(i));
    let s = dl.snapshot();
    assert!(!s.info()[i].provisional, "page {i} laid out by through");
    assert!(matches!(spatial(&s.pages()[i], s.info()[i].provisional), Spatial::Page { .. }));
}
