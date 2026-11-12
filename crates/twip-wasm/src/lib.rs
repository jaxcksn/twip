//! The WebAssembly module the TypeScript SDK loads: the C ABI (twip-capi, whose library is named twip), its functions
//! exported as `twip_*`, with the page's host and clocks behind it (twip-capi's `web.rs`). Built for
//! `wasm32-unknown-unknown` and given its JavaScript glue by wasm-bindgen (build-web.mjs).
pub use twip::*;
