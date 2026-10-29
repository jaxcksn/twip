//! The engine's benchmarks: `cargo bench -p twip [-- <filter>]`, e.g. `cargo bench -p twip -- editing::typing`.
//!
//! Each benchmark times one input, layout or frame on a committed fixture; setup between samples (placing the caret,
//! draining background layout) isn't timed. The allocation counts beside each time are Divan's `AllocProfiler`.

mod editing;
mod frames;
mod layout;
mod review;

use twip::internal::docx::Block;
use twip::internal::layout::ParaId;

#[global_allocator]
static ALLOC: divan::AllocProfiler = divan::AllocProfiler::system();

fn main() {
    divan::main();
}

/// A fixture's bytes, by its path under `tests/fixtures`.
fn fixture(name: &str) -> Vec<u8> {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

/// A xorshift generator: the same places on every run.
struct Rng(u64);

impl Rng {
    fn below(&mut self, n: usize) -> usize {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        (x % n.max(1) as u64) as usize
    }
}

/// Body paragraphs with their length in units and their table nesting depth, in document order.
fn paragraphs(blocks: &[Block], depth: usize, out: &mut Vec<(ParaId, u32, usize)>) {
    for b in blocks {
        match b {
            Block::Para(p) => out.push((ParaId(p.id), p.inlines.iter().map(|i| i.units() as u32).sum(), depth)),
            Block::Table(t) => t.rows.iter().flat_map(|r| r.cells.iter()).for_each(|c| paragraphs(&c.blocks, depth + 1, out)),
        }
    }
}
