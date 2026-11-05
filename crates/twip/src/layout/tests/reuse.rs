//! Reuse report

use std::path::Path;

use crate::docx::{Block, Inline};
use crate::layout::ids::Revision;
use crate::layout::incremental::FULL_JOBS;
use crate::layout::{Change, DocumentLayout, dump, layout};
use crate::prof::{Outcome, Piece};

use super::{doc, fonts};

fn at(r: &[[u64; 3]; 3], piece: Piece, outcome: Outcome) -> u64 {
    r[piece as usize][outcome as usize]
}

#[test]
fn prof_reuse_counts_one_keystroke() {
    let Some(mut d) = doc(Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/big.docx"))) else { return };
    let fonts = fonts();
    let mut dl = DocumentLayout::new();
    dl.edit(Revision(1), None);
    dl.complete(&d, &fonts);
    let bi = d.sections[0].blocks.len() / 2;
    let Block::Para(p) = &mut d.sections[0].blocks[bi] else { return };
    let Some(Inline::Text(t, _)) = p.inlines.iter_mut().find(|x| matches!(x, Inline::Text(..))) else { return };
    t.push('x');
    crate::prof::take_reuse();
    dl.edit(Revision(2), Some(Change { section: 0, first: bi, old_end: bi + 1, new_end: bi + 1, row: None }));
    dl.complete(&d, &fonts);
    let r = crate::prof::take_reuse();
    eprintln!("reuse after one keystroke (para, row, page × reused, moved, laid): {r:?}");
    assert!(at(&r, Piece::Para, Outcome::Laid) >= 1, "the edited paragraph is laid out: {r:?}");
    assert!(at(&r, Piece::Page, Outcome::Reused) > 0, "later pages are reused: {r:?}");
}

#[test]
fn multi_pass_baseline_typing_is_incremental() {
    let Some(mut d) = doc(Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/docx-feature-showcase.docx"))) else { return };
    let fonts = fonts();
    let mut dl = DocumentLayout::new();
    dl.edit(Revision(1), None);
    dl.complete(&d, &fonts);
    let pages = dl.snapshot().pages().len();
    let Some(si) = d.sections.iter().position(|s| s.columns.num > 1) else { return };
    let Some(bi) = d.sections[si].blocks.iter().position(|b| matches!(b, Block::Para(p) if p.inlines.iter().any(|x| matches!(x, Inline::Text(..)))))
    else {
        return;
    };
    let Block::Para(p) = &mut d.sections[si].blocks[bi] else { unreachable!() };
    let Some(Inline::Text(t, _)) = p.inlines.iter_mut().find(|x| matches!(x, Inline::Text(..))) else { return };
    t.push('x');
    let full = FULL_JOBS.get();
    crate::prof::take_reuse();
    crate::prof::take();
    dl.edit(Revision(2), Some(Change { section: si, first: bi, old_end: bi + 1, new_end: bi + 1, row: None }));
    dl.complete(&d, &fonts);
    let r = crate::prof::take_reuse();
    let why: Vec<_> = crate::prof::take().into_iter().filter(|t| t.0.starts_with("full:")).collect();
    eprintln!("reuse after one keystroke in the showcase's columns: {r:?} of {pages} pages; full layouts: {why:?}");
    assert_eq!(FULL_JOBS.get(), full, "the edit didn't fall back to a full layout");
    assert!((at(&r, Piece::Page, Outcome::Laid) as usize) < pages / 2, "pages laid out again: {r:?} of {pages}");
    assert_eq!(dump::page_text(&dl.snapshot(), &d), dump::page_text(&layout(&d, &fonts), &d));
}

#[test]
fn consecutive_keystrokes_stay_incremental() {
    let Some(mut d) = doc(Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/docx-feature-showcase.docx"))) else { return };
    let fonts = fonts();
    let mut dl = DocumentLayout::new();
    dl.edit(Revision(1), None);
    dl.complete(&d, &fonts);
    let section = d.sections.len() / 2;
    let blocks = &d.sections[section].blocks;
    let mid = blocks.len() / 2;
    let Some(at) = (mid..blocks.len()).find(|&i| matches!(&blocks[i], Block::Para(p) if p.inlines.iter().any(|x| matches!(x, Inline::Text(..)))))
    else {
        return;
    };
    let full = FULL_JOBS.get();
    for k in 0..20 {
        let Block::Para(p) = &mut d.sections[section].blocks[at] else { unreachable!() };
        let Some(Inline::Text(t, _)) = p.inlines.iter_mut().find(|x| matches!(x, Inline::Text(..))) else { unreachable!() };
        t.insert(0, 'x');
        dl.edit(Revision(k + 2), Some(Change { section, first: at, old_end: at + 1, new_end: at + 1, row: None }));
        dl.complete(&d, &fonts);
        assert_eq!(FULL_JOBS.get(), full, "keystroke {k} fell back to a full layout");
        if k % 5 == 4 {
            assert_eq!(dump::page_text(&dl.snapshot(), &d), dump::page_text(&layout(&d, &fonts), &d), "keystroke {k}");
        }
    }
}

#[test]
fn same_height_edit_lays_out_one_paragraph() {
    let Some(d) = doc(Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/editor/FX-LONG-50.docx"))) else { return };
    let fonts = fonts();
    let blocks = &d.sections[0].blocks;
    let body = blocks
        .iter()
        .enumerate()
        .filter(|(_, b)| matches!(b, Block::Para(p) if p.inlines.iter().any(|x| matches!(x, Inline::Text(..)))))
        .nth(2)
        .map(|(i, _)| i);
    let table = blocks.iter().position(|b| matches!(b, Block::Table(_)));
    for (what, bi, row) in [("body", body, None), ("cell", table, Some(0))] {
        let Some(bi) = bi else { continue };
        let mut d = d.clone();
        let mut dl = DocumentLayout::new();
        dl.edit(Revision(1), None);
        dl.complete(&d, &fonts);
        let text = match &mut d.sections[0].blocks[bi] {
            Block::Para(p) => p.inlines.iter_mut().find_map(|x| if let Inline::Text(t, _) = x { Some(t) } else { None }),
            Block::Table(t) => t.rows[0].cells[0].blocks.iter_mut().find_map(|b| match b {
                Block::Para(p) => p.inlines.iter_mut().find_map(|x| if let Inline::Text(t, _) = x { Some(t) } else { None }),
                _ => None,
            }),
        };
        text.expect("text").insert(0, 'x');
        crate::prof::take_reuse();
        dl.edit(Revision(2), Some(Change { section: 0, first: bi, old_end: bi + 1, new_end: bi + 1, row }));
        dl.complete(&d, &fonts);
        let r = crate::prof::take_reuse();
        assert_eq!(at(&r, Piece::Para, Outcome::Laid), 1, "{what}: only the edited paragraph: {r:?}");
        assert_eq!(at(&r, Piece::Page, Outcome::Laid), 1, "{what}: only its page: {r:?}");
        assert!(at(&r, Piece::Row, Outcome::Laid) <= 1, "{what}: at most the edited row: {r:?}");
        assert_eq!(dump::page_text(&dl.snapshot(), &d), dump::page_text(&layout(&d, &fonts), &d), "{what}");
    }
}

#[test]
fn reflow_moves_later_paragraphs() {
    let Some(mut d) = doc(Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/editor/FX-LONG-50.docx"))) else { return };
    let fonts = fonts();
    let mut dl = DocumentLayout::new();
    dl.edit(Revision(1), None);
    dl.complete(&d, &fonts);
    let Block::Para(mut copy) = d.sections[0].blocks[2].clone() else { return };
    copy.id = crate::docx::model::next_para_id();
    copy.src = None;
    d.sections[0].blocks.insert(3, Block::Para(copy));
    crate::prof::take_reuse();
    dl.edit(Revision(2), Some(Change { section: 0, first: 2, old_end: 3, new_end: 4, row: None }));
    dl.complete(&d, &fonts);
    let r = crate::prof::take_reuse();
    let laid = at(&r, Piece::Para, Outcome::Laid) + at(&r, Piece::Row, Outcome::Laid);
    let after: u64 = d.sections[0].blocks[4..]
        .iter()
        .map(|b| match b {
            Block::Para(_) => 1,
            Block::Table(t) => t.rows.len() as u64,
        })
        .sum();
    assert!(laid * 100 <= after * 5, "{laid} of the {after} paragraphs and rows after the edit laid out again: {r:?}");
    assert_eq!(dump::page_text(&dl.snapshot(), &d), dump::page_text(&layout(&d, &fonts), &d));
}
