// Browser build only (crates/twip-wasm/build-web.mjs sets the environment): compiles the Graphite/Dawn shim against
// the prebuilt Skia's headers. The module's link settings are in crates/twip-wasm/build.rs (link arguments from a
// library's build script don't reach the final link). Native builds need nothing here.
fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("emscripten") {
        return;
    }
    let env = |name: &str| {
        println!("cargo:rerun-if-env-changed={name}");
        std::env::var(name).unwrap_or_else(|_| panic!("{name} is not set; build with crates/twip-wasm/build-web.mjs"))
    };
    println!("cargo:rerun-if-changed=cpp/dawn.cpp");
    cc::Build::new()
        .cpp(true)
        .file("cpp/dawn.cpp")
        .include(env("SKIA_INCLUDE_DIR"))
        .flag("-std=c++20")
        .flag(format!("--use-port={}/emdawnwebgpu.port.py", env("EMDAWNWEBGPU_DIR")))
        .define("SK_GRAPHITE", None)
        .define("SK_DAWN", None)
        .compile("twip_dawn");
}
