// The module's stack: 8 MB, as the engine's recursive XML parse and layout need (wasm's default is 1 MB).
fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_FAMILY").as_deref() == Ok("wasm") {
        println!("cargo:rustc-link-arg=-zstack-size=8388608");
    }
}
