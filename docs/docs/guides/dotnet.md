---
title: Getting started (.NET)
sidebar_position: 1
---

# Getting started with .NET

Install the package (see [Installation](../installation)), then open a document, edit it and save it:

```csharp
using Twip;

using var fonts = Fonts.System();           // the fonts installed on this machine
using var editor = Editor.Create(fonts);    // starts with an empty US Letter document

editor.Open(File.ReadAllBytes("report.docx"));
Console.WriteLine($"{editor.PageCount} page(s)");

editor.SelectAll();
editor.Bold();
editor.Key("End", new Modifiers { Ctrl = true });   // to the end of the document
editor.Text(" Added by Twip.");

using var docx = editor.Save();
File.WriteAllBytes("report-edited.docx", docx.ToArray());

using var pdf = editor.ExportPdf("Report");
File.WriteAllBytes("report.pdf", pdf.ToArray());
```

Every Twip object holds native memory: dispose it (`using`) when you're done.

## Errors

`Open` throws `OpenException` when the bytes aren't a document Twip can read; `Inner` says why
(`OpenError.Empty`, `OpenError.Invalid`). The editor keeps its current document then. `Save` throws
`SaveException`.

```csharp
try { editor.Open(bytes); }
catch (OpenException e) when (e.Inner == OpenError.Invalid) { /* not a .docx */ }
```

## Layout runs in the background

`Open` lays out the first pages and returns. `PageCount` grows as layout continues; call `Idle()` when your app has
nothing else to do, until it returns `false`:

```csharp
while (editor.Idle()) { }
```

Saving and PDF export lay out the whole document first, so you don't need to wait for them.

## Next

- [Drawing pages](rendering) into your window.
- [Input and commands](input): keys, pointer, the formatting and table commands.
- [Requests and events](requests): what the editor asks of your app after each input.
