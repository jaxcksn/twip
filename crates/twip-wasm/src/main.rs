// The emscripten module: links the C ABI (twip-capi, whose library is named twip) so its exports land in the .wasm.
// The SDK calls them, never main.
use twip as _;

fn main() {}
