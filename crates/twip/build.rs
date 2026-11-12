// `twip_window`: the build draws into native windows (src/render/window.rs), on Windows and macOS.
fn main() {
    println!("cargo::rustc-check-cfg=cfg(twip_window)");
    println!("cargo:rerun-if-changed=build.rs");
    if matches!(std::env::var("CARGO_CFG_TARGET_OS").as_deref(), Ok("windows" | "macos")) {
        println!("cargo:rustc-cfg=twip_window");
    }
}
