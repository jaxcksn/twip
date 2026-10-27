//! A counting global allocator: live and peak heap bytes and the number of allocations, for `memprofile`. It counts
//! for every command, so timings stay comparable between runs.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

pub static LIVE: AtomicUsize = AtomicUsize::new(0);
pub static PEAK: AtomicUsize = AtomicUsize::new(0);
pub static COUNT: AtomicUsize = AtomicUsize::new(0);

pub struct Counting;

// SAFETY: every method forwards to `System` with the caller's arguments unchanged; the counters are only atomics.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        let n = LIVE.fetch_add(l.size(), Relaxed) + l.size();
        PEAK.fetch_max(n, Relaxed);
        COUNT.fetch_add(1, Relaxed);
        // SAFETY: the caller upholds `GlobalAlloc::alloc`'s contract for `l`.
        unsafe { System.alloc(l) }
    }

    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        LIVE.fetch_sub(l.size(), Relaxed);
        // SAFETY: `p` was allocated by this allocator (so by `System`) with layout `l`.
        unsafe { System.dealloc(p, l) }
    }

    unsafe fn realloc(&self, p: *mut u8, l: Layout, new: usize) -> *mut u8 {
        if new >= l.size() {
            let n = LIVE.fetch_add(new - l.size(), Relaxed) + new - l.size();
            PEAK.fetch_max(n, Relaxed);
        } else {
            LIVE.fetch_sub(l.size() - new, Relaxed);
        }
        COUNT.fetch_add(1, Relaxed);
        // SAFETY: `p` was allocated by `System` with layout `l`; the caller upholds `realloc`'s contract for `new`.
        unsafe { System.realloc(p, l, new) }
    }
}

#[global_allocator]
static ALLOC: Counting = Counting;

pub fn mb(n: usize) -> f64 {
    n as f64 / 1048576.0
}
