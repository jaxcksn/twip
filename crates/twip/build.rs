// `twip_gpu`: the build has a GPU screen (src/render/window.rs): Windows and macOS windows, and the browser's canvas.
// `twip_window`: it draws into native windows: Windows and macOS.
fn main() {
    println!("cargo::rustc-check-cfg=cfg(twip_window, twip_gpu)");
    println!("cargo:rerun-if-changed=build.rs");
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let family = std::env::var("CARGO_CFG_TARGET_FAMILY").unwrap_or_default();
    if matches!(os.as_str(), "windows" | "macos") {
        println!("cargo:rustc-cfg=twip_window");
        println!("cargo:rustc-cfg=twip_gpu");
    }
    if family == "wasm" && os == "unknown" {
        println!("cargo:rustc-cfg=twip_gpu");
    }
}
