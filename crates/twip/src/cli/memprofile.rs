//! `twip memprofile <doc.docx> [--soak N]`: live heap by what holds it, measured as deltas between steps, with
//! the allocation count of each step, the layout cache's evictable and live bytes by boundary, `ShapedPara` bytes per
//! paragraph, and with `--soak N` the layout-owned bytes every 100 of N seeded edits.

use std::sync::atomic::Ordering::Relaxed;

use super::alloc::{COUNT, LIVE, PEAK, mb};

/// Profiles `path`.
pub fn run(path: &str, soak: Option<usize>) -> Result<(), String> {
    use twip::docx::{Block, Inline};
    use twip::layout::ids::Revision;
    use twip::layout::{Change, DocumentLayout};
    let bytes = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    let base = LIVE.load(Relaxed);
    let live = || LIVE.load(Relaxed).saturating_sub(base);
    let mut last = 0usize;
    let last_count = std::cell::Cell::new(COUNT.load(Relaxed));
    let step = |what: &str, last: &mut usize| {
        let now = live();
        let count = COUNT.load(Relaxed);
        println!(
            "{what:<58} live {:8.1} MB  delta {:+8.1} MB  peak {:8.1} MB  allocations {count} (+{})",
            mb(now),
            mb(now) - mb(*last),
            mb(PEAK.load(Relaxed).saturating_sub(base)),
            count - last_count.get()
        );
        last_count.set(COUNT.load(Relaxed));
        *last = now;
    };
    let fonts = twip::font::Fonts::system();
    step("font catalog (name tables only)", &mut last);
    let mut doc = twip::docx::read(&bytes).map_err(|e| format!("{path}: {e:?}"))?;
    step("document (layout read: no sources)", &mut last);
    let s = twip::layout::layout(&doc, &fonts);
    step("+ full layout (and font files and faces it loaded)", &mut last);
    drop(s);
    step("- layout dropped (what remains: font files and faces)", &mut last);
    let s = twip::layout::layout(&doc, &fonts);
    step(&format!("+ layout again: {} pages (the snapshot)", s.pages().len()), &mut last);
    drop(s);
    let owned_from = live();
    let mut dl = DocumentLayout::new();
    dl.edit(Revision(1), None);
    dl.complete(&doc, &fonts);
    step("+ DocumentLayout: cache, snapshot, baseline, shadow", &mut last);
    let report = |dl: &DocumentLayout| {
        for (name, n, e, l) in dl.cache().report() {
            println!("    {name:<52} {n:8} evictable {:7.1} MB live {:7.1} MB", mb(e), mb(l));
        }
    };
    report(&dl);
    let shaped = dl.cache().report().into_iter().find(|r| r.0 == "shaped").map_or(0, |r| (r.2 + r.3) / r.1.max(1));
    println!("    ShapedPara bytes per paragraph (estimate)              {shaped:8}");
    let paras: Vec<(usize, usize)> = doc
        .sections
        .iter()
        .enumerate()
        .flat_map(|(si, s)| {
            s.blocks
                .iter()
                .enumerate()
                .filter(|(_, b)| matches!(b, Block::Para(p) if p.inlines.iter().any(|i| matches!(i, Inline::Text(..)))))
                .map(move |(bi, _)| (si, bi))
        })
        .collect();
    let mut rev = 1;
    let mut edit = |doc: &mut twip::docx::Document, dl: &mut DocumentLayout, k: usize| {
        // Documents of tables only have no top-level paragraph to edit.
        let Some(&(si, bi)) = paras.get((k * 7919) % paras.len().max(1)) else { return };
        let blocks = &mut doc.sections[si].blocks;
        if bi >= blocks.len() {
            return;
        }
        let change = if k % 5 == 4 && matches!(blocks[bi], Block::Para(_)) {
            // An Enter: an empty paragraph before the one edited.
            let Block::Para(p) = &blocks[bi] else { unreachable!() };
            let mut empty = p.clone();
            empty.inlines.clear();
            empty.id = twip::docx::model::next_para_id();
            empty.src = None;
            blocks.insert(bi, Block::Para(empty));
            Change { section: si, first: bi, old_end: bi + 1, new_end: bi + 2, row: None }
        } else {
            if let Block::Para(p) = &mut blocks[bi]
                && let Some(Inline::Text(t, _)) = p.inlines.iter_mut().find(|i| matches!(i, Inline::Text(..)))
            {
                t.insert(0, 'x');
            }
            Change { section: si, first: bi, old_end: bi + 1, new_end: bi + 1, row: None }
        };
        rev += 1;
        dl.edit(Revision(rev), Some(change));
        dl.complete(doc, &fonts);
    };
    for k in 0..5 {
        edit(&mut doc, &mut dl, k * 5);
    }
    step("+ 5 typed characters", &mut last);
    let Some(n) = soak else { return Ok(()) };
    for k in 0..n {
        edit(&mut doc, &mut dl, k);
        if (k + 1) % 100 == 0 {
            let (e, l): (usize, usize) = dl.cache().report().iter().fold((0, 0), |(a, b), r| (a + r.2, b + r.3));
            println!(
                "soak {:5}: layout-owned {:7.1} MB (live heap after the document), cache evictable {:6.1} MB, live {:6.1} MB, {} entries",
                k + 1,
                mb(live().saturating_sub(owned_from)),
                mb(e),
                mb(l),
                dl.cache().len()
            );
        }
    }
    Ok(())
}
