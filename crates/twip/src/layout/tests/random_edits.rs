//! Seeded random edits through `DocumentLayout`: after every edit, the
//! completed incremental layout equals a fresh `layout(doc, fonts)` item for item, status included.
#![allow(clippy::single_element_loop, reason = "document lists with one shipped document; add more as they ship")]

use std::path::Path;
use std::rc::Rc;

use crate::docx::model::next_para_id;
use crate::docx::props::Border;
use crate::docx::{Block, BreakKind, Document, Inline, Paragraph};
use crate::font::Fonts;
use crate::layout::ids::Revision;
use crate::layout::incremental::{FORCE_FIRST_PAGE, FULL_JOBS, REUSED_PAGES};
use crate::layout::{Budget, Change, DocumentLayout, LayoutStatus, Step, dump, layout};

use super::{doc, fonts};

/// xorshift64: deterministic, seedable.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed.max(1))
    }
    pub fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    pub fn below(&mut self, n: usize) -> usize {
        if n == 0 { 0 } else { (self.next() % n as u64) as usize }
    }
}

/// The first text inline of a paragraph.
fn text_of(p: &mut Paragraph) -> Option<&mut String> {
    p.inlines.iter_mut().find_map(|i| if let Inline::Text(t, _) = i { Some(t) } else { None })
}

/// A paragraph (and its block index) in section `si` with text, in a cell when `cell`.
fn pick_para(d: &mut Document, si: usize, rng: &mut Rng) -> Option<usize> {
    let blocks = &d.sections[si].blocks;
    let paras: Vec<usize> =
        (0..blocks.len()).filter(|&i| matches!(&blocks[i], Block::Para(p) if p.inlines.iter().any(|x| matches!(x, Inline::Text(..))))).collect();
    (!paras.is_empty()).then(|| paras[rng.below(paras.len())])
}

/// What one edit did: its hint, and whether it must force a full layout.
struct Edit {
    hint: Option<Change>,
    forces_full: bool,
    what: String,
}

fn change(si: usize, first: usize, old_end: usize, new_end: usize) -> Option<Change> {
    Some(Change { section: si, first, old_end, new_end, row: None })
}

/// Applies one random edit to `d`.
fn random_edit(d: &mut Document, rng: &mut Rng) -> Option<Edit> {
    let si = rng.below(d.sections.len());
    let n = d.sections[si].blocks.len();
    if rng.below(20) == 0 {
        // Changes that must force a full layout.
        let what = match rng.below(4) {
            0 => {
                d.settings.default_tab += 9.0;
                "default tab"
            }
            1 => {
                d.sections[si].left += 6.0;
                "section margin"
            }
            2 => {
                let blocks = d.footnotes.values_mut().next()?;
                let Some(Block::Para(p)) = blocks.iter_mut().find(|b| matches!(b, Block::Para(_))) else { return None };
                let t = text_of(p)?;
                t.push_str(" more note text");
                "footnote text"
            }
            _ => {
                // A style's spacing: every paragraph sharing the first paragraph's style.
                let Some(Block::Para(first)) = d.sections[si].blocks.iter().find(|b| matches!(b, Block::Para(_))) else { return None };
                let old = first.style.clone();
                let mut st = (*old).clone();
                st.space_after += 7.0;
                let new = Rc::new(st);
                for s in &mut d.sections {
                    for b in &mut s.blocks {
                        if let Block::Para(p) = b
                            && Rc::ptr_eq(&p.style, &old)
                        {
                            p.style = new.clone();
                        }
                    }
                }
                return Some(Edit { hint: None, forces_full: false, what: "style spacing".into() });
            }
        };
        return Some(Edit { hint: None, forces_full: true, what: what.into() });
    }
    let kind = rng.below(10);
    match kind {
        0 | 1 => {
            let bi = pick_para(d, si, rng)?;
            let Block::Para(p) = &mut d.sections[si].blocks[bi] else { unreachable!() };
            let t = text_of(p)?;
            if kind == 0 || t.chars().count() < 2 {
                t.push_str(" typed");
            } else {
                let k = t.char_indices().nth(t.chars().count() / 2).map_or(0, |c| c.0);
                t.remove(k);
            }
            Some(Edit { hint: change(si, bi, bi + 1, bi + 1), forces_full: false, what: format!("type in block {bi}") })
        }
        2 => {
            // Split a paragraph (Enter).
            let bi = pick_para(d, si, rng)?;
            let Block::Para(p) = &d.sections[si].blocks[bi] else { unreachable!() };
            let mut second = p.clone();
            second.id = next_para_id();
            second.src = None;
            d.sections[si].blocks.insert(bi + 1, Block::Para(second));
            Some(Edit { hint: change(si, bi, bi + 1, bi + 2), forces_full: false, what: format!("split block {bi}") })
        }
        3 => {
            // Join a paragraph with the next one.
            let bi = pick_para(d, si, rng)?;
            if !matches!(d.sections[si].blocks.get(bi + 1), Some(Block::Para(_))) {
                return None;
            }
            let Block::Para(next) = d.sections[si].blocks.remove(bi + 1) else { unreachable!() };
            let Block::Para(p) = &mut d.sections[si].blocks[bi] else { unreachable!() };
            p.inlines.extend(next.inlines);
            Some(Edit { hint: change(si, bi, bi + 2, bi + 1), forces_full: false, what: format!("join block {bi}") })
        }
        4 => {
            // Insert or delete 30–60 paragraphs (whole pages).
            let k = 30 + rng.below(31);
            let bi = pick_para(d, si, rng)?;
            if rng.below(2) == 0 || n < k + 2 {
                let Block::Para(p) = &d.sections[si].blocks[bi] else { unreachable!() };
                let p = p.clone();
                for j in 0..k {
                    let mut q = p.clone();
                    q.id = next_para_id();
                    q.src = None;
                    d.sections[si].blocks.insert(bi + 1 + j, Block::Para(q));
                }
                Some(Edit { hint: change(si, bi + 1, bi + 1, bi + 1 + k), forces_full: false, what: format!("insert {k} after block {bi}") })
            } else {
                let at = bi.min(n - k - 1);
                d.sections[si].blocks.drain(at..at + k);
                Some(Edit { hint: change(si, at, at + k, at), forces_full: false, what: format!("delete {k} at block {at}") })
            }
        }
        5 => {
            // Edit text in a table cell.
            let tables: Vec<usize> = (0..n).filter(|&i| matches!(d.sections[si].blocks[i], Block::Table(_))).collect();
            if tables.is_empty() {
                return None;
            }
            let bi = tables[rng.below(tables.len())];
            let Block::Table(t) = &mut d.sections[si].blocks[bi] else { unreachable!() };
            let r = rng.below(t.rows.len());
            let row = &mut t.rows[r];
            if row.cells.is_empty() {
                return None;
            }
            let c = rng.below(row.cells.len());
            let p = row.cells[c].blocks.iter_mut().find_map(|b| if let Block::Para(p) = b { Some(p) } else { None })?;
            match text_of(p) {
                Some(t) => t.push_str(" cell"),
                None => {
                    let run = p.mark.clone();
                    p.inlines.push(Inline::Text("cell".into(), run));
                }
            }
            Some(Edit {
                hint: Some(Change { section: si, first: bi, old_end: bi + 1, new_end: bi + 1, row: Some(r) }),
                forces_full: false,
                what: format!("cell edit in block {bi}"),
            })
        }
        6 => {
            let bi = pick_para(d, si, rng)?;
            let Block::Para(p) = &mut d.sections[si].blocks[bi] else { unreachable!() };
            let mut st = (*p.style).clone();
            st.keep_next = !st.keep_next;
            p.style = Rc::new(st);
            Some(Edit { hint: change(si, bi, bi + 1, bi + 1), forces_full: false, what: format!("toggle keepNext on block {bi}") })
        }
        7 => {
            let bi = pick_para(d, si, rng)?;
            let Block::Para(p) = &mut d.sections[si].blocks[bi] else { unreachable!() };
            let mut st = (*p.style).clone();
            st.border_bottom = match st.border_bottom {
                Some(_) => None,
                None => Some(Border { style: "single".into(), width: 1.0, space: 1.0, color: None }),
            };
            p.style = Rc::new(st);
            Some(Edit { hint: change(si, bi, bi + 1, bi + 1), forces_full: false, what: format!("toggle border on block {bi}") })
        }
        8 => {
            let bi = pick_para(d, si, rng)?;
            let Block::Para(p) = &mut d.sections[si].blocks[bi] else { unreachable!() };
            let run = p.mark.clone();
            p.inlines.insert(0, Inline::Break(BreakKind::Page, run));
            Some(Edit { hint: change(si, bi, bi + 1, bi + 1), forces_full: false, what: format!("page break in block {bi}") })
        }
        _ => {
            // A header story edit.
            let blocks = d.stories.values_mut().next()?;
            let p = blocks.iter_mut().find_map(|b| if let Block::Para(p) = b { Some(p) } else { None })?;
            match text_of(p) {
                Some(t) => t.push_str(" h"),
                None => {
                    let run = p.mark.clone();
                    p.inlines.push(Inline::Text("header".into(), run));
                }
            }
            Some(Edit { hint: None, forces_full: false, what: "header story edit".into() })
        }
    }
}

/// Runs `steps` random edits on `d` from `seed`, checking every completed layout against a fresh one.
fn run_edits(name: &str, d: Document, fonts: &Fonts, seed: u64, steps: usize) {
    run_edits_with(name, d, fonts, seed, steps, Budget::Unlimited);
}

/// [`run_edits`], completing each job in runs of `budget`.
pub(super) fn run_edits_with(name: &str, mut d: Document, fonts: &Fonts, seed: u64, steps: usize, budget: Budget) {
    let mut rng = Rng::new(seed);
    let mut dl = DocumentLayout::new();
    dl.edit(Revision(1), None);
    dl.complete(&d, fonts);
    let mut rev = 1;
    let mut done = 0;
    let mut attempts = 0;
    while done < steps && attempts < steps * 4 {
        attempts += 1;
        let Some(e) = random_edit(&mut d, &mut rng) else { continue };
        rev += 1;
        done += 1;
        let hint = if done % 2 == 0 { e.hint } else { None };
        let full_before = FULL_JOBS.get();
        dl.edit(Revision(rev), hint);
        let mut runs = 0;
        while dl.run(&d, fonts, budget) == Step::Yielded {
            runs += 1;
            assert!(runs < 1_000_000, "{name} seed {seed} step {done}: the job makes no progress");
            let s = dl.snapshot();
            assert!(
                matches!(s.status(), LayoutStatus::Pending { .. }) || s.revision() < Revision(rev),
                "a yielded run publishes only pending snapshots"
            );
            assert_eq!(s.info().len(), s.pages().len());
        }
        let got = dl.snapshot();
        let want = layout(&d, fonts);
        let (a, b) = (dump::page_text(&got, &d), dump::page_text(&want, &d));
        if a != b || got.status() != want.status() {
            let line = a.lines().zip(b.lines()).position(|(x, y)| x != y).unwrap_or(0);
            panic!(
                "{name} seed {seed} step {done} ({}): incremental layout differs from a full layout at dump line {line}:\n  incremental: {}\n  full:        {}\n  status {:?} vs {:?}",
                e.what,
                a.lines().nth(line).unwrap_or("<end>"),
                b.lines().nth(line).unwrap_or("<end>"),
                got.status(),
                want.status()
            );
        }
        if e.forces_full {
            assert!(FULL_JOBS.get() > full_before, "{name} seed {seed} step {done}: {} didn't force a full layout", e.what);
        }
    }
}

pub(super) fn documents() -> Vec<(String, Document)> {
    let mut out = Vec::new();
    for p in [concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/tables.docx")] {
        if let Some(d) = doc(Path::new(p)) {
            out.push((p.to_string(), d));
        }
    }
    for rel in [
        r"HF9\HF9_s_titlepg.docx",
        r"HF9\HF9_s_cont.docx",
        r"HF9\HF9_f_pagenum.docx",
        r"FN1\FN1a_one.docx",
        r"K1\K1b_keepnext.docx",
        r"P1\P1a_contextual.docx",
        r"T5\T5e_vmerge.docx",
        r"T6\T6c_vmerge_break.docx",
    ] {
        if let Some(d) = super::fixture(rel) {
            out.push((rel.to_string(), d));
        }
    }
    out
}

#[test]
fn random_edits_equal_full_layout() {
    let fonts = fonts();
    for (name, d) in documents() {
        for seed in [1, 2] {
            run_edits(&name, d.clone(), &fonts, seed, 24);
        }
    }
}

#[test]
fn random_edits_on_long_documents() {
    let fonts = fonts();
    for p in [concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/big.docx")] {
        if let Some(d) = doc(Path::new(p)) {
            run_edits(p, d, &fonts, 7, 10);
        }
    }
}

/// The same edits with every job restarting from the first page.
#[test]
fn restart_from_first_page() {
    let fonts = fonts();
    FORCE_FIRST_PAGE.set(true);
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        for (name, d) in documents() {
            run_edits(&name, d, &fonts, 3, 16);
        }
    }));
    FORCE_FIRST_PAGE.set(false);
    r.unwrap_or_else(|e| std::panic::resume_unwind(e));
}

/// Typing reuses pages (the point of incremental layout).
#[test]
fn typing_reuses_pages() {
    let Some(mut d) = doc(Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/big.docx"))) else { return };
    let fonts = fonts();
    let mut dl = DocumentLayout::new();
    dl.edit(Revision(1), None);
    dl.complete(&d, &fonts);
    let pages = dl.snapshot().pages().len();
    let bi = d.sections[0].blocks.len() / 2;
    let Block::Para(p) = &mut d.sections[0].blocks[bi] else { return };
    let Some(t) = text_of(p) else { return };
    t.push('x');
    let before = REUSED_PAGES.get();
    dl.edit(Revision(2), Some(Change { section: 0, first: bi, old_end: bi + 1, new_end: bi + 1, row: None }));
    dl.complete(&d, &fonts);
    let reused = REUSED_PAGES.get() - before;
    assert!(reused as usize + 5 >= pages / 2, "typing mid-document reused {reused} of {pages} pages");
}

/// Many seeds (run with `--ignored`): a stress run.
#[test]
#[ignore]
fn random_edits_stress() {
    let fonts = fonts();
    for (name, d) in documents() {
        for seed in 1..=20 {
            run_edits(&name, d.clone(), &fonts, seed, 40);
        }
    }
    for p in [concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/big.docx")] {
        if let Some(d) = doc(Path::new(p)) {
            for seed in 11..=13 {
                run_edits(p, d.clone(), &fonts, seed, 20);
            }
        }
    }
}

/// Hinted edits in chosen rows of every table (header rows, merged regions, rows on later pages): the row exception
/// reuses pages after the edited row only when they lay out the same.
#[test]
fn table_row_edits() {
    let fonts = fonts();
    let mut docs: Vec<(String, Document)> = Vec::new();
    for p in [concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/tables.docx")] {
        if let Some(d) = doc(Path::new(p)) {
            docs.push((p.into(), d));
        }
    }
    for rel in [r"T5\T5e_vmerge.docx", r"T6\T6c_vmerge_break.docx"] {
        if let Some(d) = super::fixture(rel) {
            docs.push((rel.into(), d));
        }
    }
    for (name, mut d) in docs {
        let mut dl = DocumentLayout::new();
        dl.edit(Revision(1), None);
        dl.complete(&d, &fonts);
        let mut rev = 1;
        let tables: Vec<(usize, usize)> = d
            .sections
            .iter()
            .enumerate()
            .flat_map(|(si, s)| s.blocks.iter().enumerate().filter(|(_, b)| matches!(b, Block::Table(_))).map(move |(bi, _)| (si, bi)))
            .collect();
        for (si, bi) in tables {
            let Block::Table(t) = &d.sections[si].blocks[bi] else { unreachable!() };
            let n = t.rows.len();
            let mut rows = vec![0, 1, n / 2, n.saturating_sub(1)];
            rows.dedup();
            for r in rows.into_iter().filter(|&r| r < n) {
                let Block::Table(t) = &mut d.sections[si].blocks[bi] else { unreachable!() };
                let Some(p) =
                    t.rows[r].cells.iter_mut().flat_map(|c| c.blocks.iter_mut()).find_map(|b| if let Block::Para(p) = b { Some(p) } else { None })
                else {
                    continue;
                };
                match text_of(p) {
                    // One character: the row keeps its height, so later pages keep their breaks and are candidates.
                    Some(t) => t.insert(0, 'x'),
                    None => {
                        let run = p.mark.clone();
                        p.inlines.push(Inline::Text("x".into(), run));
                    }
                }
                rev += 1;
                dl.edit(Revision(rev), Some(Change { section: si, first: bi, old_end: bi + 1, new_end: bi + 1, row: Some(r) }));
                dl.complete(&d, &fonts);
                let (a, b) = (dump::page_text(&dl.snapshot(), &d), dump::page_text(&layout(&d, &fonts), &d));
                if a != b {
                    let line = a.lines().zip(b.lines()).position(|(x, y)| x != y).unwrap_or(0);
                    panic!(
                        "{name}: edit in table block {bi} row {r} differs at dump line {line}:\n  incremental: {}\n  full:        {}",
                        a.lines().nth(line).unwrap_or("<end>"),
                        b.lines().nth(line).unwrap_or("<end>")
                    );
                }
            }
        }
    }
}
