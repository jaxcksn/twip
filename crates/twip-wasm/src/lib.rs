//! The WebAssembly module the TypeScript SDK loads: the C ABI (twip-capi, whose library is named twip), its functions
//! exported as `twip_*`, with the page's host and clocks behind it (twip-capi's `web.rs`). Built for
//! `wasm32-unknown-unknown` and given its JavaScript glue by wasm-bindgen (build-web.mjs).
pub use twip::*;

/// The module's allocator: talc, growing the module's memory itself, single-threaded (the module has no shared memory).
#[cfg(all(not(target_feature = "atomics"), target_family = "wasm"))]
#[global_allocator]
static TALC: talc::wasm::WasmDynamicTalc = talc::wasm::new_wasm_dynamic_allocator();
