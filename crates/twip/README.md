# twip

A DOCX layout, editing and rendering engine: it paginates Word documents, edits them, and draws them with Skia or
exports PDF.

> **Alpha.** Before 1.0 the crate's public items follow the engine's needs rather than a designed Rust API; expect
> breaking changes in minor versions.

```sh
cargo add twip
```

On Linux, Skia links FreeType and Fontconfig: install `libfreetype6-dev libfontconfig1-dev` (Debian and Ubuntu).
The first build downloads a prebuilt Skia.

```rust
use twip::font::Fonts;
use twip::host::Engine;

let mut engine = Engine::with_fonts(Fonts::system());
engine.try_open(&std::fs::read("report.docx")?)?;
println!("{} page(s)", engine.page_sizes().len());

engine.key("End", true, false, false); // Ctrl+End
engine.text(" Added by Twip.");
std::fs::write("report-edited.docx", engine.save()?)?;
std::fs::write("report.pdf", engine.export_pdf("Report"))?;
```

Guides: https://jaxcksn.github.io/twip/docs/guides/rust. Licensed under Apache-2.0.
