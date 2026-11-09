// The GPU shims skia-safe has no binding for: Graphite on Dawn, and Graphite's image provider (cpp/graphite.cpp). In
// the browser (crates/twip-wasm/build-web.mjs sets the environment) against emdawnwebgpu; the module's link settings
// are in crates/twip-wasm/build.rs (link arguments from a library's build script don't reach the final link). On
// Windows against native Dawn, when TWIP_DAWN_DIR names the unpacked Skia archive; on macOS against Metal, when
// SKIA_INCLUDE_DIR names Skia's source (crates/twip-capi/build-native.mjs sets both). Without them the build draws with
// the CPU only. `twip_window` says the build draws into native windows (src/render/window.rs).
fn main() {
    println!("cargo::rustc-check-cfg=cfg(twip_window)");
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
        }
        Ok("windows") => {
            let Ok(dir) = env("TWIP_DAWN_DIR") else { return };
            for file in ["dawn.cpp", "dawn_native.cpp", "graphite.cpp"] {
                println!("cargo:rerun-if-changed=cpp/{file}");
            }
            cc::Build::new()
                .cpp(true)
                .files(["cpp/dawn.cpp", "cpp/dawn_native.cpp", "cpp/graphite.cpp"])
                .include(&dir)
                .include(format!("{dir}/dawn/include"))
                .flag_if_supported("/std:c++20")
                .flag_if_supported("-std=c++20")
                .define("SK_GRAPHITE", None)
                .define("SK_DAWN", None)
                // The archive's Skia is a release build (its skia-defines.txt): without NDEBUG, a debug build of the
                // shim compiles Skia's headers with SK_DEBUG, whose inline checks disagree with the library.
                .define("NDEBUG", None)
                .define("NOMINMAX", None)
                .define("WIN32_LEAN_AND_MEAN", None)
                .compile("twip_dawn");
            // Dawn, combined into one library by Skia's build. It loads D3D12, D3D11 and Vulkan at run time.
            println!("cargo:rustc-link-search=native={dir}");
            println!("cargo:rustc-link-lib=static=dawn_combined");
            for lib in ["user32", "dxguid", "delayimp", "onecore"] {
                println!("cargo:rustc-link-lib={lib}");
            }
            println!("cargo:rustc-cfg=twip_window");
        }
        Ok("macos") => {
            // Skia's source, as the skia-bindings crate ships it: the headers of the prebuilt Skia it links.
            let Ok(skia) = env("SKIA_INCLUDE_DIR") else { return };
            for file in ["graphite.cpp", "metal.mm"] {
                println!("cargo:rerun-if-changed=cpp/{file}");
            }
            // cc builds for the SDK's own macOS unless told: the shims build for rustc's.
            let min = env("MACOSX_DEPLOYMENT_TARGET")
                .unwrap_or_else(|_| if std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("aarch64") { "11.0" } else { "10.12" }.into());
            cc::Build::new()
                .cpp(true)
                .env("MACOSX_DEPLOYMENT_TARGET", &min)
                .file("cpp/graphite.cpp")
                .include(&skia)
                .flag("-std=c++20")
                .define("SK_GRAPHITE", None)
                .define("SK_METAL", None)
                // rust-skia's prebuilt Skia is a release build: without NDEBUG, a debug build of the shim compiles
                // Skia's headers with SK_DEBUG, whose inline checks disagree with the library.
                .define("NDEBUG", None)
                .compile("twip_graphite");
            let mut metal = cc::Build::new();
            metal.env("MACOSX_DEPLOYMENT_TARGET", &min).file("cpp/metal.mm").flag("-fobjc-arc").compile("twip_metal");
            for framework in ["Metal", "QuartzCore", "Foundation"] {
                println!("cargo:rustc-link-lib=framework={framework}");
            }
            // Skia's Metal backend checks the OS version (`@available`) with compiler-rt's __isPlatformVersionAtLeast,
            // which rustc doesn't link.
            let resources = metal.get_compiler().to_command().arg("--print-resource-dir").output().expect("clang's resource directory");
            let resources = String::from_utf8(resources.stdout).expect("a path");
            println!("cargo:rustc-link-search=native={}/lib/darwin", resources.trim());
            println!("cargo:rustc-link-lib=static=clang_rt.osx");
            println!("cargo:rustc-cfg=twip_window");
        }
        _ => {}
    }
}
