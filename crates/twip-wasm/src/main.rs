// The emscripten module: links the bridge so its exports land in the .wasm. The host calls them, never main.
use twip_ffi as _;

fn main() {}
