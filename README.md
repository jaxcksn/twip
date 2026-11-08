<div align="center">
  <br><br>
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset=".github/Twip-dark.png">
    <source media="(prefers-color-scheme: light)" srcset=".github/Twip.png">
    <img src=".github/Twip.png" alt="Twip" width="240">
  </picture>
  <br><br>

  <p>
    <a href="#"><img src="https://img.shields.io/badge/version-0.1.0-8c8c8c?style=flat-square" alt="Version"></a>
    <a href="#"><img src="https://img.shields.io/badge/build-passing-8c8c8c?style=flat-square" alt="Build"></a>
    <a href="#"><img src="https://img.shields.io/badge/license-Apache_2.0-8c8c8c?style=flat-square" alt="License"></a>
    <a href="#"><img src="https://img.shields.io/badge/downloads-0-8c8c8c?style=flat-square" alt="Downloads"></a>
  </p>

  <p>A DOCX engine focused on performance and fidelity, ready to integrate on native and web.</p>
  
  ---
  <br>
</div>

Twip is a DOCX layout, editing and rendering engine written in Rust. It reads a document, lays it out line for line and
page for page, draws it with Skia, and edits it in place, with undo, track changes and a complete command set. The
same engine runs natively on Windows and Linux and in the browser through WebAssembly, with SDKs for the web, .NET and
Rust.

Twip is the engine under an editor, not the whole application. It owns the document, the layout, the caret and the
selection, every editing command, input on the page and the drawing of the page. It also computes the models behind
your chrome (toolbar state, context menus, dialogs, find, spelling, rulers and review), so your application draws its
own toolbar, menus and panes around a page the engine already handles.

## Why Twip

- **Faithful layout.** Pagination, line breaking, justification, tab stops, tables, floating objects, columns, notes,
  headers and footers are laid out from the document's own settings, so text, lines and pages fall where the
  document's author put them.
- **Fast by design.** Layout is incremental: an edit lays out again only what it changed and reuses every page it
  didn't touch. Long documents paginate in the background, a few milliseconds at a time, while the first pages are
  already editable. Typing has a latency budget: on a machine with a GPU, a keystroke's frame paints within one
  display refresh (16.7 ms).
- **GPU rendering everywhere.** Skia Graphite on Dawn draws with Direct3D 12, Direct3D 11 or Vulkan natively and with
  WebGPU in the browser, falling back to WebGL 2 and then the CPU. A lost GPU device is replaced and drawing carries on.
- **One engine, every platform.** A single Rust core serves the browser, Node, Windows and Linux desktops and
  services, with the same layout everywhere. The .NET SDK is Native AOT compatible, and the web SDK runs in a worker
  or headless in Node.
- **One contract, three SDKs.** The web, .NET and Rust SDKs are written by hand for their languages, and all of them
  follow `twip::api`: the same commands, state, events and errors. A shared suite of contract scripts runs against
  every SDK, so they behave the same.
- **Headless or mounted.** Run a session with no UI to generate, fill and convert documents on a server, or mount
  the full editor in a page or a WinForms form.

## Features

### Text and paragraphs

- Fonts from the system, from files (TrueType, OpenType, collections, WOFF and WOFF2) or from a resolver you supply,
  with named instances of variable fonts as faces of their own. Your application decides every font choice: aliases,
  replacements, generic defaults, fallbacks, and reports of what's missing
- Styles, with paragraph, character, table and numbering styles and the document defaults
- Line breaking, justification, tab stops with leaders, line and paragraph spacing, borders, shading and the document
  grid
- Lists and multi-level numbering, line numbers, and keep-with-next, keep-lines-together and widow control

### Pages and sections

- Sections with their own page size, orientation, margins, columns and page numbering
- Columns with balancing and separators, page and column breaks
- Headers and footers (first page, odd and even), footnotes and endnotes, and change bars
- Text frames, and vertical alignment of a section's pages

### Tables

- Autofit and fixed column widths, preferred widths, cell spacing and margins
- Merged cells, nested tables, repeated header rows and rows that split across pages
- Floating tables with text wrapped around them
- Hover handles to move and resize a table, "+" buttons to insert rows and columns, and live column widths while a
  border is dragged

### Pictures and shapes

- Inline and floating pictures, with square and top-and-bottom wrapping or placed behind or in front of the text
- DrawingML shapes: all 187 preset geometries of ECMA-376 and custom geometry, with fills, outlines, theme styles,
  shadows, glow, soft edges, blur and text inside
- VML shapes from older documents, read into the same model
- Text boxes, linked text boxes, groups, and handles to move, resize and rotate objects, with snapping to margins,
  the page centre, other objects and an optional grid

### Editing

- A complete command set: typing, formatting, paragraphs, lists, tables, pages, objects and navigation, each one a
  transaction with undo and redo; batches apply several commands atomically as one undo step
- A command catalog with labels, groups and shortcuts, for menus, command palettes and shortcut sheets
- Rulers with draggable indents, margins, tab stops and table columns
- Clipboard with rich text, find and replace, spelling with your dictionaries, and AutoCorrect with shipped English
  lists
- Bookmarks, hyperlinks and cross-references

### Review

- Tracked insertions, deletions, formatting and moves, with accept and reject
- Comments with replies, resolution and rich text, and balloons placed beside their threads by the SDK or by you

### Templates and data

- Fields, including merge fields, page numbers and cross-references, shown as codes or results
- Content controls: plain and rich text, check boxes, dates, drop-downs, pictures, groups and repeating sections,
  with data bound to custom XML parts
- A merge designer surface: list, select and drag fields and content controls, and read and write the custom XML parts
- Sub-templates: building blocks kept once and referenced from a template, shown inline and edited in place

### Appearance and accessibility

- Light and dark chrome themes with an accent colour, dark mode for the page, and high contrast from the platform
- Formatting marks, zoom and markup pane, all drawn by the engine on the GPU
- An accessibility tree for screen readers, kept by the mounted editor in the browser

### Import and export

- Opens DOCX and saves DOCX
- Exports PDF, laid out completely first

## SDKs

| SDK | Package | For |
|---|---|---|
| Web | `@twip/web` on npm | Browsers and Node, through WebAssembly |
| .NET | `Twip.Core` and `Twip.WinForms` on NuGet | Windows and Linux apps and services (`win-x64`, `win-arm64`, `linux-x64`, `linux-arm64`), Native AOT compatible |
| Rust | `twip` on crates.io | Rust applications and services |

React and Angular adapters live in `packages/`; `@twip/web` itself depends on no framework.

## Quick start

### Web

```bash
pnpm add @twip/web
```

```ts
import { Editor } from '@twip/web';

const container = document.querySelector<HTMLElement>('#editor')!;
const response = await fetch('/report.docx');

const editor = await Editor.mount(container, {
  document: new Uint8Array(await response.arrayBuffer()),
  author: { name: 'Ada Lovelace', initials: 'AL' },
  rulers: true,
  zoom: 'pageWidth',
});
editor.focus();

await editor.commands.selectAll();
await editor.commands.setBold(true);
console.log(editor.getState().formatting.bold); // true

const { bytes } = await editor.export({ format: 'docx' });
```

### .NET

```bash
dotnet add package Twip.Core       # the session: headless, any .NET app
dotnet add package Twip.WinForms   # the DocumentEditor control
```

```csharp
using Twip;

await using var session = new EditorSession(new EditorSessionOptions(Author: new Author("Report bot")));
await session.OpenAsync("template.docx");

await session.Commands.MoveSelectionAsync(MoveUnit.Document, Direction.Forward);
await session.Commands.InsertTextAsync("Generated by Twip.");

await session.SaveAsync("report.docx");
await session.ExportPdfAsync("report.pdf");
```

### Rust

```toml
[dependencies]
twip = "0.1"
```

```rust
use twip::api::command::Command;
use twip::api::engine::{Engine, SessionOptions};
use twip::api::fonts::FontLibrary;
use twip::api::session::{ExportFormat, ExportOptions};

let mut engine = Engine::new(FontLibrary::system());
let id = engine.create_session(SessionOptions::default());
let session = engine.session(id).expect("the session was just created");

session.open(&std::fs::read("report.docx")?)?;
for command in [Command::SelectAll, Command::SetBold(true)] {
    session.run(command)?;
}
let export = session.export(&ExportOptions { format: ExportFormat::Docx })?;
```

The full examples are in `docs/examples` and `crates/twip/examples`, and are compiled in CI.

## Documentation

The docs site covers installation, a getting-started guide for each SDK, and guides for editing on the canvas,
events, services, fonts, shapes, review, sub-templates, rendering, appearance and AutoCorrect, with the API reference
of every SDK. Run it locally with `pnpm moon run docs:dev`.

## Repository

| Path | What |
|---|---|
| `crates/twip` | The engine: document model, layout, editing, rendering and PDF. `twip::api` is the contract every SDK follows. |
| `crates/twip-capi` | The C ABI over `twip::api`, the native library the .NET SDK loads |
| `crates/twip-wasm` | The C ABI built as a WebAssembly module for the web |
| `crates/twip-tools` | Developer tools: dumps, round trips, renders and comparisons |
| `bindings/web` | `@twip/web`, the TypeScript SDK |
| `bindings/dotnet` | `Twip.Core` and `Twip.WinForms`, the .NET SDK |
| `packages/*` | React and Angular adapters |
| `tests/contract` | The contract scripts every SDK runs |
| `docs` | The docs site, its examples and the API references |

Building, testing and contributing are covered in [CONTRIBUTING](.github/CONTRIBUTING.md). The road to 1.0 is in
[the roadmap](docs/docs/roadmap.mdx).

---

<p align="center">
<sub>
Copyright 2026 Jackson Casey<br >This product is licensed under the Apache License, Version 2.0. See the LICENSE file distributed with this work for the full license text.
</sub>
</p>
