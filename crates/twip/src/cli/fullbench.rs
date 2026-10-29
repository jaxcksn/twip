//! `twip fullbench <doc.docx> [runs]`: full layouts in more detail than `bench`'s first part: time with phases, cached cold and warm layouts, allocations, and the live bytes of
//! the cache and of a snapshot.

use std::sync::atomic::Ordering::Relaxed;

use super::alloc::{COUNT, LIVE, mb};

pub fn run(path: &str, runs: usize) -> Result<(), String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    let fonts = twip::internal::font::Fonts::system();
    let t = std::time::Instant::now();
    let doc = twip::internal::docx::read(&bytes).map_err(|e| format!("{path}: {e:?}"))?;
    println!("read {:.1} ms", t.elapsed().as_secs_f64() * 1e3);
    let report = |label: &str, t: std::time::Instant, pages: usize, allocs: usize| {
        println!("{label}: {:.1} ms, {pages} pages, {allocs} allocations", t.elapsed().as_secs_f64() * 1e3);
        for (name, ms, n) in twip::internal::prof::take() {
            println!("    {name:<10} {ms:8.1} ms {n:8} calls");
        }
    };
    for k in 0..runs {
        let c = COUNT.load(Relaxed);
        let t = std::time::Instant::now();
        let s = twip::internal::layout::layout(&doc, &fonts);
        report(&format!("layout #{k}"), t, s.pages().len(), COUNT.load(Relaxed) - c);
    }
    let mut cache = twip::internal::layout::cache::LayoutCache::new();
    let c = COUNT.load(Relaxed);
    let t = std::time::Instant::now();
    let s = twip::internal::layout::layout_with(&doc, &fonts, &mut cache);
    report("cached cold", t, s.pages().len(), COUNT.load(Relaxed) - c);
    drop(s);
    let c = COUNT.load(Relaxed);
    let t = std::time::Instant::now();
    let s = twip::internal::layout::layout_with(&doc, &fonts, &mut cache);
    report("cached warm", t, s.pages().len(), COUNT.load(Relaxed) - c);
    let (hits, misses) = cache.counts();
    println!("cache hits {hits}, misses {misses}");
    drop(s);
    // Memory: a cache filled by one full layout, and a snapshot on its own.
    let base = LIVE.load(Relaxed);
    let mut cache = twip::internal::layout::cache::LayoutCache::new();
    let s = twip::internal::layout::layout_with(&doc, &fonts, &mut cache);
    let both = LIVE.load(Relaxed).saturating_sub(base);
    drop(cache);
    let snapshot = LIVE.load(Relaxed).saturating_sub(base);
    println!("live: cache + snapshot {:.1} MB, snapshot alone {:.1} MB ({} pages)", mb(both), mb(snapshot), s.pages().len());
    Ok(())
}
