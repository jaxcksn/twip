// emscripten link settings for the module the TypeScript SDK loads: the C ABI's exports, and twip-host.js for the
// engine's callbacks. build-web.mjs sets EMDAWNWEBGPU_DIR.
fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("emscripten") {
        return;
    }
    println!("cargo:rerun-if-env-changed=EMDAWNWEBGPU_DIR");
    println!("cargo:rerun-if-changed=twip-host.js");
    let dawn = std::env::var("EMDAWNWEBGPU_DIR").expect("EMDAWNWEBGPU_DIR is not set; build with build-web.mjs");
    let host = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR")).join("twip-host.js");
    // The C ABI (crates/twip-capi/include/twip.h), as Module._twip_*.
    let exports = [
        "twip_engine_new",
        "twip_engine_free",
        "twip_session_new",
        "twip_session_free",
        "twip_buf_free",
        "twip_alloc",
        "twip_free",
        "twip_call",
        "twip_open",
        "twip_open_prepare",
        "twip_export",
        "twip_font_add",
        "twip_key",
        "twip_text",
        "twip_composition",
        "twip_pointer",
        "twip_wheel",
        "twip_focus",
        "twip_surface",
        "twip_scroll",
        "twip_frame_rgba",
        "twip_gpu_instance",
        "twip_attach_webgpu",
        "twip_frame_webgpu",
        "twip_attach_webgl",
        "twip_frame_webgl",
        "twip_timer_fired",
    ];
    let exports: Vec<String> = exports.iter().map(|e| format!("_{e}")).collect();
    for arg in [
        format!("--use-port={dawn}/emdawnwebgpu.port.py"),
        format!("--js-library={}", host.display()),
        format!("-sEXPORTED_FUNCTIONS={}", exports.join(",")),
        "-sMODULARIZE=1".into(),
        "-sEXPORT_ES6=1".into(),
        "-sEXPORT_NAME=createTwip".into(),
        "-sENVIRONMENT=web,worker,node".into(),
        "-sALLOW_MEMORY_GROWTH=1".into(),
        "-sMIN_WEBGL_VERSION=2".into(),
        "-sMAX_WEBGL_VERSION=2".into(),
        "-sEXPORTED_RUNTIME_METHODS=GL,WebGPU,HEAPU8,HEAPU32".into(),
        // emscripten's default 64 KB stack overflows in the recursive XML parse and in layout.
        "-sSTACK_SIZE=8MB".into(),
    ] {
        println!("cargo:rustc-link-arg={arg}");
    }
}
