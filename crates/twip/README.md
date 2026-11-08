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
use twip::api::command::Command;
use twip::api::engine::{Engine, SessionOptions};
use twip::api::fonts::{Alias, AliasTarget, FontLibrary};
use twip::api::session::ExportOptions;

// The system's fonts, with Calibri drawn in Carlito where Calibri isn't installed.
let fonts = FontLibrary::system();
fonts.set_aliases(vec![Alias { from: "Calibri".into(), bold: None, italic: None, to: AliasTarget::Family("Carlito".into()) }]);

let mut engine = Engine::new(fonts);
let id = engine.create_session(SessionOptions::default());
let session = engine.session(id).expect("the session just created");
session.open(&std::fs::read("report.docx")?)?;
session.run(Command::InsertText(" Added by Twip.".into()))?;
std::fs::write("report-edited.docx", session.export(&ExportOptions::default())?.bytes.0)?;
```

To convert without an editor, on as many threads as you like, use `twip::pdf`: build a `FontSet` once and share it.

```rust
use twip::pdf::{FontSet, PdfOptions, convert};

let fonts = FontSet::system();
let options = PdfOptions { open_password: Some("s3cret".into()), keywords: vec!["notice".into()] };
let pdf = convert(&std::fs::read("report.docx")?, &fonts, &options)?;
std::fs::write("report.pdf", pdf.bytes)?;
```

Guides: https://jaxcksn.github.io/twip/docs/guides/rust. Licensed under Apache-2.0.
