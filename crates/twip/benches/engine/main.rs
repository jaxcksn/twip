//! The engine's benchmarks: `cargo bench -p twip [-- <filter>]`, e.g. `cargo bench -p twip -- editing::typing`.
//!
//! Each benchmark times one input, layout or frame on a committed fixture; setup between samples (placing the caret,
//! draining background layout) isn't timed. The allocation counts beside each time are Divan's `AllocProfiler`.

mod drags;
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

/// With `--features prof`, a diagnostic run: the stage times and the reuse report of a benchmark's timed parts,
/// printed once after its samples. Untimed setup is discarded (`skip`); without the feature everything is empty and
/// nothing prints.
#[derive(Default)]
struct Prof {
    times: Vec<(&'static str, f64, u64)>,
    reuse: [[u64; 3]; 3],
    samples: usize,
}

impl Prof {
    /// Drops what untimed setup counted.
    fn skip() {
        twip::internal::prof::take();
        twip::internal::prof::take_reuse();
    }

    /// Adds what a timed sample counted.
    fn add(&mut self) {
        for (name, ms, calls) in twip::internal::prof::take() {
            match self.times.iter_mut().find(|t| t.0 == name) {
                Some(t) => (t.1, t.2) = (t.1 + ms, t.2 + calls),
                None => self.times.push((name, ms, calls)),
            }
        }
        let r = twip::internal::prof::take_reuse();
        for (row, add) in self.reuse.iter_mut().zip(r) {
            for (c, a) in row.iter_mut().zip(add) {
                *c += a;
            }
        }
        self.samples += 1;
    }

    fn print(&mut self, name: &str) {
        if self.times.is_empty() && self.reuse == [[0; 3]; 3] {
            return;
        }
        let n = self.samples.max(1) as f64;
        self.times.sort_by(|a, b| b.1.total_cmp(&a.1));
        let stages: Vec<String> = self.times.iter().map(|(s, ms, calls)| format!("{s} {:.3} ms x{:.1}", ms / n, *calls as f64 / n)).collect();
        let per = |row: [u64; 3]| row.map(|c| format!("{:.1}", c as f64 / n)).join("/");
        eprintln!(
            "prof {name}: {} | reused/moved/laid: para {} row {} page {}",
            stages.join(", "),
            per(self.reuse[0]),
            per(self.reuse[1]),
            per(self.reuse[2])
        );
    }
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
