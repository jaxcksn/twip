---
title: Getting started (Rust)
sidebar_position: 3
---

# Getting started with Rust

The `twip` crate is the engine itself: the same code the .NET and web SDKs call. Its modules are `docx` (reading and
writing packages), `layout` (pagination), `editing` (the editor), `render` (drawing with Skia), `pdf` (export) and
`host` (what the SDKs drive).

:::caution Unstable surface

Before 1.0 the crate's public items follow the engine's needs, not a designed Rust API. Expect breaking changes in
minor versions. The SDKs' API is the stable one to follow.

:::

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

To lay a document out without an editor, read it and call `layout` directly:

```rust
let doc = twip::docx::read(&bytes)?;
let snapshot = twip::layout::layout(&doc, &Fonts::system());
for page in snapshot.pages() {
    println!("{} x {} pt", page.w, page.h);
}
```

The crate builds Skia from a prebuilt archive, which it downloads on the first build.
