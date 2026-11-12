// The GPU shims skia-safe has no binding for, in the browser: Graphite on Dawn, and Graphite's image provider
// (cpp/dawn.cpp, cpp/graphite.cpp), against emdawnwebgpu (crates/twip-wasm/build-web.mjs sets the environment); the
// module's link settings are in crates/twip-wasm/build.rs (link arguments from a library's build script don't reach the
// final link). `twip_window` says the build draws into native windows (src/render/window.rs): Windows and macOS.
fn main() {
    println!("cargo::rustc-check-cfg=cfg(twip_window)");
    println!("cargo:rerun-if-changed=build.rs");
    let env = |name: &str| {
        println!("cargo:rerun-if-env-changed={name}");
        std::env::var(name)
    };
    match std::env::var("CARGO_CFG_TARGET_OS").as_deref() {
        Ok("emscripten") => {
            let need = |name: &str| env(name).unwrap_or_else(|_| panic!("{name} is not set; build with crates/twip-wasm/build-web.mjs"));
            println!("cargo:rerun-if-changed=cpp/dawn.cpp");
            println!("cargo:rerun-if-changed=cpp/graphite.cpp");
            cc::Build::new()
                .cpp(true)
                .files(["cpp/dawn.cpp", "cpp/graphite.cpp"])
                .include(need("SKIA_INCLUDE_DIR"))
                .flag("-std=c++20")
                .flag(format!("--use-port={}/emdawnwebgpu.port.py", need("EMDAWNWEBGPU_DIR")))
                .define("SK_GRAPHITE", None)
                .define("SK_DAWN", None)
                .compile("twip_dawn");
            // The crate's own test binaries, which run under Node (build-web.mjs test): the host's files (fixtures and
            // snapshots) through NODERAWFS, and emscripten's default 64 KB stack overflows in layout.
            for arg in [
                format!("--use-port={}/emdawnwebgpu.port.py", need("EMDAWNWEBGPU_DIR")),
                "-sNODERAWFS=1".into(),
                "-sALLOW_MEMORY_GROWTH=1".into(),
                "-sSTACK_SIZE=8MB".into(),
            ] {
                println!("cargo:rustc-link-arg={arg}");
            }
        }
        // Native windows draw with twip-graphics on wgpu (src/render/window.rs).
        Ok("windows" | "macos") => println!("cargo:rustc-cfg=twip_window"),
        _ => {}
    }
}
