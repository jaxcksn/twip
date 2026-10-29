// The GPU shims skia-safe has no binding for: Graphite on Dawn. In the browser (crates/twip-wasm/build-web.mjs sets the
// environment) against emdawnwebgpu; the module's link settings are in crates/twip-wasm/build.rs (link arguments from
// a library's build script don't reach the final link). On Windows against native Dawn, when TWIP_DAWN_DIR names the
// unpacked Windows Skia archive (crates/twip-capi/build-native.mjs); without it the build draws with the CPU only.
fn main() {
    println!("cargo::rustc-check-cfg=cfg(twip_dawn)");
    let env = |name: &str| {
        println!("cargo:rerun-if-env-changed={name}");
        std::env::var(name)
    };
    match std::env::var("CARGO_CFG_TARGET_OS").as_deref() {
        Ok("emscripten") => {
            let need = |name: &str| env(name).unwrap_or_else(|_| panic!("{name} is not set; build with crates/twip-wasm/build-web.mjs"));
            println!("cargo:rerun-if-changed=cpp/dawn.cpp");
            cc::Build::new()
                .cpp(true)
                .file("cpp/dawn.cpp")
                .include(need("SKIA_INCLUDE_DIR"))
                .flag("-std=c++20")
                .flag(format!("--use-port={}/emdawnwebgpu.port.py", need("EMDAWNWEBGPU_DIR")))
                .define("SK_GRAPHITE", None)
                .define("SK_DAWN", None)
                .compile("twip_dawn");
        }
        Ok("windows") => {
            let Ok(dir) = env("TWIP_DAWN_DIR") else { return };
            println!("cargo:rerun-if-changed=cpp/dawn.cpp");
            println!("cargo:rerun-if-changed=cpp/dawn_native.cpp");
            cc::Build::new()
                .cpp(true)
                .files(["cpp/dawn.cpp", "cpp/dawn_native.cpp"])
                .include(&dir)
                .include(format!("{dir}/dawn/include"))
                .flag_if_supported("/std:c++20")
                .flag_if_supported("-std=c++20")
                .define("SK_GRAPHITE", None)
                .define("SK_DAWN", None)
                .define("NOMINMAX", None)
                .define("WIN32_LEAN_AND_MEAN", None)
                .compile("twip_dawn");
            // Dawn, combined into one library by Skia's build; it loads D3D12, D3D11 and Vulkan at run time.
            println!("cargo:rustc-link-search=native={dir}");
            println!("cargo:rustc-link-lib=static=dawn_combined");
            for lib in ["user32", "dxguid", "delayimp", "onecore"] {
                println!("cargo:rustc-link-lib={lib}");
            }
            println!("cargo:rustc-cfg=twip_dawn");
        }
        _ => {}
    }
}
