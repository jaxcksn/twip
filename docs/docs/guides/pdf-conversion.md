---
title: PDF conversion
sidebar_position: 14
---

# PDF conversion

Twip converts DOCX to PDF without an editor: no session, no window, and no engine thread. Each conversion runs on the
thread that starts it, and any number run at once, so a service can convert as many documents in parallel as it has
cores. The PDF shows the final document, without revision marks or comments, laid out exactly as the editor's PDF
export lays it out.

## .NET

`PdfConverter` is in `Twip.Core`. Build a `PdfFontSet` once and share it: it is fixed once created and safe to use
from any thread.

```csharp file=../../examples/dotnet/PdfConversion.cs#batch
```

`Convert` runs on the calling thread. The `ConvertAsync` overloads take bytes, streams or paths and run on a thread
pool thread. A path overload writes to a temporary file beside the PDF and moves it into place, so a failed or
cancelled conversion leaves an existing file as it was. Streams you pass stay open. With no font set,
`PdfFontSet.System` (the installed fonts) is used.

Disposing a font set while conversions use it is safe: each running conversion holds its own reference, and later
ones throw `ObjectDisposedException`. A font set is not a `FontLibrary`. A `FontLibrary` belongs to editor sessions and
changes over time (resolvers, added faces, new settings); a font set never changes.

Failures throw `EditorException`: `ImportFailed` for bytes that aren't a readable DOCX, `Unsupported` for an encrypted
or binary document, and `ExportFailed` for a font set with no faces. Cancellation throws `OperationCanceledException`.

## Rust

The converter is `twip::pdf` in the `twip` crate.

```rust
use twip::pdf::{FontSet, PdfOptions, convert};

let fonts = FontSet::system();
let documents: Vec<Vec<u8>> = vec![std::fs::read("a.docx")?, std::fs::read("b.docx")?];
let pdfs: Vec<_> = std::thread::scope(|s| {
    let jobs: Vec<_> = documents.iter().map(|d| s.spawn(|| convert(d, &fonts, &PdfOptions::default()))).collect();
    jobs.into_iter().map(|j| j.join().expect("a conversion doesn't panic")).collect()
});
for pdf in pdfs {
    let pdf = pdf?;
    println!("{} bytes, {} warnings", pdf.bytes.len(), pdf.warnings.len());
}
```

`FontSet::builder()` adds font files and bytes, and takes the same settings as a `FontLibrary`: `replacements`,
`aliases`, `generic_defaults`, `classification`, `fallback`, `character_fallback` and `symbol_fallback`. `convert_cancellable` takes
an `AtomicBool` and stops with `Aborted` soon after it is set. Errors are `twip::api::error::Error`, with the codes
above.

## Passwords and keywords

`PdfOptions` sets an open password and keywords. It works the same on a conversion and on the editor's PDF export, in
every SDK.

```csharp file=../../examples/dotnet/PdfConversion.cs#options
```

```ts file=../../examples/web/pdf-options.ts#options
```

- **Open password.** The PDF is encrypted with AES-256 (the PDF 2.0 standard security handler, revision 6). Readers ask
  for the password before showing anything, and text extraction, the bookmarks outline and the accessibility tags
  work as usual once it is entered. An empty password is rejected. A PDF/A file can't be encrypted, so a protected PDF
  leaves out the conformance claim Twip's PDFs otherwise carry, and the export warns with
  `pdf-archival-claim-dropped`. Owner passwords and permission restrictions aren't supported.
- **Keywords.** They go to the document properties readers show (the Info dictionary) and to the XMP metadata, in
  order, joined with commas. Blank entries are dropped.

## Fonts on a server

Twip draws with the fonts you give it, makes no font choices of its own, and doesn't download any. A font set takes
the settings a `FontLibrary` does, resolved in the same order (see [How a family resolves](fonts#how-a-family-resolves)):

| .NET (`PdfFontSetOptions`) | Rust (`FontSetBuilder`) | |
|---|---|---|
| `Replacements` | `replacements` | [draw-time replacements](fonts#draw-time-replacements) |
| `Aliases` | `aliases` | [aliases](fonts#aliases), for families the set doesn't have |
| `GenericDefaults`, `ClassifyFromDocument` | `generic_defaults`, `classification` | [generic defaults](fonts#generic-defaults) |
| `Fallback` | `fallback` | [the general fallback](fonts#the-general-fallback-and-the-last-resort) |
| `CharacterFallback`, `SymbolFallback` | `character_fallback`, `symbol_fallback` | [character fallback](fonts#character-fallback-and-the-symbol-font) |

A conversion has no resolver, so every family the set can't draw is missing. The result's `Fonts` (`fonts` in Rust)
lists them, per family and style, with what drew them and why, and the characters no font had, the same report an
editor session gives. Each also warns with `pdf-font-fallback`. On a server, install the fonts your documents use or
pass their files to the font set, and alias the families you can't ship to metric-compatible ones (Calibri to Carlito,
Cambria to Caladea): see [A recommended setup](fonts#a-recommended-setup).

## Alpine and other containers

`Twip.Core` ships native libraries for Linux musl (Alpine) on x64 and arm64, beside Windows x64 and Linux glibc x64
and arm64. On Linux the library needs FreeType, Fontconfig and libstdc++, and fonts to draw with:

```dockerfile
FROM mcr.microsoft.com/dotnet/runtime:10.0-alpine
RUN apk add --no-cache fontconfig freetype libstdc++ font-liberation
COPY app/ /app/
ENTRYPOINT ["dotnet", "/app/MyService.dll"]
```

On Debian and Ubuntu images, install `libfreetype6 libfontconfig1` and a font package such as `fonts-liberation`. When
a library is missing, loading `Twip.Core` fails with a `DllNotFoundException` that names the missing file and these
packages.
