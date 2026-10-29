---
title: Fonts
sidebar_position: 7
---

# Fonts

Layout measures text with real font files, so the fonts available decide where lines break and pages end. Every
session draws with a `FontLibrary`. Share one: in the browser `FontLibrary.default()` is the page's library, and in
.NET `FontLibrary.System` is the process's.

```ts file=../../examples/web/fonts.ts
```

```csharp file=../../examples/dotnet/Fonts.cs#fonts
```

## Where fonts come from

- **Files you add.** `add` (`AddAsync`) takes TrueType, OpenType, a collection, WOFF or WOFF2, as bytes, a URL or a
  path, and returns the faces it added.
- **Variable fonts.** Each named instance of a variable font is a face, named as Windows names it ("Bahnschrift
  SemiBold", "Segoe UI Variable Display Semib"), so documents saved by Word find them, and bold picks the font's Bold
  instance. PDF export draws variable faces as outlines (Type 3 fonts): the text stays selectable, but isn't hinted.
- **The system** (.NET and Rust). `FontLibrary.System` loads the machine's fonts. The browser gives the engine
  none, so a web page adds what its documents use, or resolves them on demand.
- **A resolver.** `setResolver` is called the first time a document needs a family the library doesn't have. Return
  the font's bytes or URL, or `null`. While it runs, the family is in `state.fonts.pending` and text is laid out with a
  fallback; when it arrives, the document is laid out again. Families nothing resolves are in
  `state.fonts.unresolved`, and `fontsChanged` reports both lists.
- **Google Fonts.** `useGoogleFonts()` fetches families the resolver doesn't have from Google Fonts, optionally only
  `families` you list.

## Substitution

Documents mostly use Calibri, Cambria, Arial, Times New Roman and Courier New. Where you can't ship those, use their
metric-compatible stand-ins, which have the same widths and so the same line breaks: Carlito, Caladea, Liberation
Sans (or Arimo), Liberation Serif (or Tinos) and Liberation Mono (or Cousine). `substitute` maps a family to
another, optionally per weight and style (`{ from, bold, italic, to }`). `setDefault` names the family for text
nothing else resolves.

The state's formatting always shows the family the document names, never the substitute, so a font box shows what
the document asks for.

## Linux

Native builds use the system's FreeType and Fontconfig. Install the stand-ins above with
`fonts-crosextra-carlito fonts-crosextra-caladea fonts-liberation` on Debian and Ubuntu; Twip uses them for those
names automatically.

Font files are licensed separately from Twip. Check that you may ship the fonts you add.
