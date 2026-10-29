//! `twip bench <doc.docx> [runs]`: full layouts, cached layouts, repeated keystrokes,
//! and keystrokes through the document with `DocumentLayout`, each checked against a full layout, with the pages
//! reused per edit. `NOHINT=1` edits without hints (the change is found by comparison);
//! `NOHINT=alt` alternates.

use twip::internal::docx::{Block, Inline};
use twip::internal::layout::ids::Revision;
use twip::internal::layout::{Change, DocumentLayout, Snapshot, dump, layout, layout_with};

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

fn report(label: &str, t: std::time::Instant, s: &Snapshot) {
    println!("{label}: {:.1} ms, {} pages", t.elapsed().as_secs_f64() * 1e3, s.pages().len());
    for (name, ms, n) in twip::internal::prof::take() {
        println!("    {name:<10} {ms:8.1} ms {n:8} calls");
    }
}

pub fn run(path: &str, runs: usize) -> Result<(), String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    let t = std::time::Instant::now();
    let fonts = twip::internal::font::Fonts::system();
    println!("fonts {:.1} ms", t.elapsed().as_secs_f64() * 1e3);
    let t = std::time::Instant::now();
    let doc = twip::internal::docx::read(&bytes).map_err(|e| format!("{path}: {e:?}"))?;
    println!("read {:.1} ms", t.elapsed().as_secs_f64() * 1e3);
    for k in 0..runs {
        let t = std::time::Instant::now();
        let s = layout(&doc, &fonts);
        report(&format!("layout #{k}"), t, &s);
    }
    let mut cache = twip::internal::layout::cache::LayoutCache::new();
    let t = std::time::Instant::now();
    let s = layout_with(&doc, &fonts, &mut cache);
    report("cached cold", t, &s);
    let t = std::time::Instant::now();
    let s = layout_with(&doc, &fonts, &mut cache);
    report("cached warm", t, &s);
    // Typing: one character inserted in the middle paragraph, laid out incrementally, repeated.
    let mut dl = DocumentLayout::new();
    let mut rev = 1;
    dl.edit(Revision(rev), None);
    dl.complete(&doc, &fonts);
    twip::internal::prof::take();
    let mut edited = doc.clone();
    let si = edited.sections.len() / 2;
    let mid = edited.sections[si].blocks.len() / 2;
    let at = (mid..edited.sections[si].blocks.len()).find(|&i| matches!(edited.sections[si].blocks[i], Block::Para(_))).unwrap_or(mid);
    for k in 0..runs {
        if let Some(t) = edited.sections[si].blocks.get_mut(at).and_then(first_text) {
            t.insert(0, 'x');
        }
        rev += 1;
        dl.edit(Revision(rev), Some(Change { section: si, first: at, old_end: at + 1, new_end: at + 1, row: None }));
        let t = std::time::Instant::now();
        dl.complete(&edited, &fonts);
        let (hits, misses) = dl.cache().counts();
        let (reused, pages) = dl.reused_pages();
        report(&format!("keystroke #{k} (hits {hits}, misses {misses}, reused {reused} of {pages} pages)"), t, &dl.snapshot());
    }
    // Incremental: keystrokes in successive paragraphs through the document, each checked against a full layout.
    let mut doc2 = doc.clone();
    let mut dl = DocumentLayout::new();
    dl.edit(Revision(rev), None);
    dl.complete(&doc2, &fonts);
    twip::internal::prof::take();
    let nohint = std::env::var("NOHINT").ok();
    let nsec = doc2.sections.len();
    let (mut worst, mut total, mut bad, mut reused_total) = (0.0f64, 0.0, 0, 0);
    let steps = 40;
    let mut done = 0;
    for k in 0..steps * 4 {
        if done == steps {
            break;
        }
        let s = (k * 7) % nsec;
        let blocks = &mut doc2.sections[s].blocks;
        let at = (k * 7919) % blocks.len().max(1);
        // A table edit names its row, as the editor's hints do: the first row with text.
        let row = match blocks.get_mut(at) {
            Some(Block::Table(t)) => {
                t.rows.iter_mut().position(|r| r.cells.iter_mut().flat_map(|c| c.blocks.iter_mut()).any(|b| first_text(b).is_some()))
            }
            _ => None,
        };
        let Some(t) = blocks.get_mut(at).and_then(first_text) else { continue };
        t.insert_str(0, if k % 3 == 0 { "wider words " } else { "x" });
        done += 1;
        let change = Change { section: s, first: at, old_end: at + 1, new_end: at + 1, row };
        let hint = match nohint.as_deref() {
            Some("alt") if k % 2 == 1 => None,
            Some("alt") => Some(change),
            Some(_) => None,
            None => Some(change),
        };
        rev += 1;
        let t = std::time::Instant::now();
        dl.edit(Revision(rev), hint);
        dl.complete(&doc2, &fonts);
        let ms = t.elapsed().as_secs_f64() * 1e3;
        let phases = twip::internal::prof::take();
        if ms > 5.0 {
            let list: Vec<String> = phases.iter().map(|(n, t, c)| format!("{n} {t:.1}/{c}")).collect();
            println!("  step {k}: {ms:.1} ms: {}", list.join(", "));
        }
        worst = worst.max(ms);
        total += ms;
        reused_total += dl.reused_pages().0;
        let full = layout(&doc2, &fonts);
        twip::internal::prof::take();
        let (a, b) = (dump::page_text(&dl.snapshot(), &doc2), dump::page_text(&full, &doc2));
        if a != b {
            bad += 1;
            let line = a.lines().zip(b.lines()).position(|(x, y)| x != y).unwrap_or(0);
            println!(
                "  step {k}: MISMATCH at dump line {line}:
    incremental {}
    full        {}",
                a.lines().nth(line).unwrap_or("<end>"),
                b.lines().nth(line).unwrap_or("<end>")
            );
        }
    }
    println!(
        "incremental keystrokes: avg {:.2} ms, worst {:.2} ms, {bad} of {done} differ from a full layout, {:.1} pages reused per edit",
        total / done.max(1) as f64,
        worst,
        reused_total as f64 / done.max(1) as f64
    );
    Ok(())
}
