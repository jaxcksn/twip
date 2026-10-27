// emscripten link settings for the module the TypeScript SDK loads (bindings/web/src/diplomat-wasm.mjs).
// build-web.mjs sets EMDAWNWEBGPU_DIR.
fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("emscripten") {
        return;
    }
    println!("cargo:rerun-if-env-changed=EMDAWNWEBGPU_DIR");
    let dawn = std::env::var("EMDAWNWEBGPU_DIR").expect("EMDAWNWEBGPU_DIR is not set; build with build-web.mjs");
    for arg in [
        format!("--use-port={dawn}/emdawnwebgpu.port.py"),
        "-sMODULARIZE=1".into(),
        "-sEXPORT_ES6=1".into(),
        "-sEXPORT_NAME=createTwip".into(),
        "-sENVIRONMENT=web,worker,node".into(),
        "-sALLOW_MEMORY_GROWTH=1".into(),
        "-sMIN_WEBGL_VERSION=2".into(),
        "-sMAX_WEBGL_VERSION=2".into(),
        "-sEXPORTED_RUNTIME_METHODS=GL,WebGPU,HEAPU8".into(),
        // emscripten's default 64 KB stack overflows in the recursive XML parse and in layout.
        "-sSTACK_SIZE=8MB".into(),
    ] {
        println!("cargo:rustc-link-arg={arg}");
    }
}
