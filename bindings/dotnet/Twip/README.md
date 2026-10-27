# Twip

The Twip DOCX engine for .NET: open Word documents, edit them, render pages and save DOCX or PDF. Native AOT
compatible; ships native libraries for Windows x64 and Linux x64 and arm64.

> **Alpha.** The API may change between minor versions before 1.0.

```sh
dotnet add package Twip
```

On Linux, install FreeType and Fontconfig (`libfreetype6 libfontconfig1` on Debian and Ubuntu).

```csharp
using Twip;

using var fonts = Fonts.System();
using var editor = Editor.Create(fonts);

editor.Open(File.ReadAllBytes("report.docx"));
Console.WriteLine($"{editor.PageCount} page(s)");

editor.SelectAll();
editor.Bold();
using var docx = editor.Save();
File.WriteAllBytes("report-edited.docx", docx.ToArray());
```

Every Twip object holds native memory: dispose it (`using`) when you're done.

Guides: https://jaxcksn.github.io/twip/docs/guides/dotnet. Licensed under Apache-2.0.
