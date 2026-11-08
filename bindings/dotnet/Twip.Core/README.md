# Twip.Core

The Twip DOCX engine for .NET: open Word documents, edit them through typed commands, observe state, and save DOCX or
PDF, or convert DOCX to PDF headlessly on many threads at once. Native AOT compatible; ships native libraries for
Windows x64, Linux x64 and arm64, and Linux musl (Alpine) x64 and arm64. Also targets .NET Framework 4.8 on Windows,
in 64-bit processes (build the app for x64).

> **Alpha.** The API may change between minor versions before 1.0.

```sh
dotnet add package Twip.Core
```

On Linux, install FreeType and Fontconfig (`libfreetype6 libfontconfig1` on Debian and Ubuntu; `fontconfig freetype
libstdc++` and a font package such as `font-liberation` on Alpine).

```csharp
using Twip;

await using var session = new EditorSession();
session.DocumentChanged += (_, e) => Console.WriteLine($"revision {e.Record.Revision} ({e.Record.Origin})");

await session.OpenAsync("report.docx");
await session.Commands.SelectAllAsync();
await session.Commands.SetBoldAsync(true);
Console.WriteLine(session.State.Formatting.Bold.IsOn);   // True

await session.SaveAsync("report-edited.docx");
```

To convert without an editor session, share one font set across conversions:

```csharp
using var fonts = PdfFontSet.Create(new PdfFontSetOptions { IncludeSystemFonts = true });
var pdfs = await Task.WhenAll(files.Select(f => PdfConverter.ConvertAsync(f, f + ".pdf", fonts)));
```

Operations can be called from any thread and run in order. Each completes once its change has committed and
`State` is at its revision. For a desktop control, see `Twip.WinForms`.
