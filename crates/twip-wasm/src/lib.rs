//! The WebAssembly module the TypeScript SDK loads: the C ABI (twip-capi, whose library is named twip), its functions
//! exported as `twip_*`, with the page's host and clocks behind it (twip-capi's `web.rs`). Built for
//! `wasm32-unknown-unknown` and given its JavaScript glue by wasm-bindgen (build-web.mjs).
pub use twip::*;

/// The module's allocator: talc, growing the module's memory itself ([`grow::Grow`]), single-threaded (the module has
/// no shared memory).
#[cfg(all(not(target_feature = "atomics"), target_family = "wasm"))]
#[global_allocator]
static TALC: talc::cell::TalcSyncCell<grow::Grow, talc::wasm::WasmBinning> = talc::cell::TalcSyncCell::new_wasm(grow::Grow { end: None });

#[cfg(all(not(target_feature = "atomics"), target_family = "wasm"))]
mod grow {
    //! Memory growth

    use core::alloc::Layout;
    use core::arch::wasm32::{memory_grow, memory_size};
    use core::ptr::NonNull;
    use talc::base::binning::Binning;
    use talc::base::{CHUNK_UNIT, Talc};

    const PAGE: usize = 64 << 10;

    /// Grows the memory by a quarter of its size or what the allocation needs, whichever is more, up to 64 MiB a
    /// step, and extends the heap over it. Each grow replaces the page's views of the memory, so growing only by what
    /// one allocation needs, as talc's own source does, costs a grow every 64 KiB while a document opens.
    #[derive(Debug)]
    pub struct Grow {
        pub end: Option<NonNull<u8>>,
    }

    // SAFETY: `acquire` only grows the memory and hands talc the new pages; it never allocates.
    unsafe impl talc::source::Source for Grow {
        fn acquire<B: Binning>(talc: &mut Talc<Self, B>, layout: Layout) -> Result<(), ()> {
            // More than talc's chunk for `layout` (its size, a tag, alignment padding) and the heap's own overhead.
            let needed = (layout.size() + layout.align() + 4 * CHUNK_UNIT).div_ceil(PAGE);
            let wanted = needed.max((memory_size::<0>() / 4).min((64 << 20) / PAGE));
            let (start, pages) = match memory_grow::<0>(wanted) {
                usize::MAX if wanted > needed => (memory_grow::<0>(needed), needed),
                start => (start, wanted),
            };
            if start == usize::MAX {
                return Err(());
            }
            let base = (start * PAGE) as *mut u8;
            let end = base.wrapping_add(pages * PAGE);
            if let Some(old) = talc.source.end.take()
                && old.as_ptr() == base
            {
                // SAFETY: `old` is the end of the heap talc was given last, and the pages after it are new.
                talc.source.end = Some(unsafe { talc.extend(old, end) });
                return Ok(());
            }
            // SAFETY: the pages were just grown, so nothing else uses them.
            talc.source.end = unsafe { talc.claim(base, pages * PAGE) };
            talc.source.end.map(|_| ()).ok_or(())
        }
    }
}
