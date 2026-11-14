---
title: Fonts
sidebar_position: 7
---

# Fonts

Layout measures text with real font files, so the fonts available decide where lines break and pages end. Twip makes
no font choices of its own: your application decides which fonts exist, what draws a family a document names, and
what happens when nothing does. Every session draws with a `FontLibrary`. Share one: in the browser
`FontLibrary.default()` is the page's library, and in .NET `FontLibrary.System` is the process's.

Twip bundles three fonts, none of which a document can name:

- **A symbol font** that draws bullets, checkboxes, arrows, check marks, card suits and common math signs when no other
  font has them. You can [replace it or turn it off](#character-fallback-and-the-symbol-font).
- **An East Asian punctuation font.** In text marked East Asian (`w:hint="eastAsia"`) whose East Asian font isn't an
  East Asian font, shared characters such as quotes, the section sign and arrows are drawn in MS Mincho. Twip draws
  them with MS Mincho when your library has it, or an alias or replacement for it; otherwise with this font, which has
  MS Mincho's widths and line height, so the text lays out the same. Its outlines are Noto Serif JP's.
- **An interface font** that draws the ruler's numbers and overlay labels. It never draws document text.

## How a family resolves

For each family and style a document uses, the library tries these in order and stops at the first family it has:

1. **A replacement** for the family ([`setReplacements`](#draw-time-replacements)), even when the family is in the
   library. The replacement's target goes through steps 2 and 3.
2. **The family itself.**
3. **Its aliases** ([`setAliases`](#aliases)), followed up to 8 hops.
4. **What your resolver answered** for it ([a family or a category](#resolvers)). While the resolver is still working,
   the family is pending and this step is skipped.
5. **The default of its generic category** ([`setGenericDefaults`](#generic-defaults)), when you set one.
6. **The general fallback** ([`setFallback`](#the-general-fallback-and-the-last-resort)).
7. **The first face added to the library.** In an empty library, the symbol font draws text as boxes.

A character the resulting font doesn't have comes from your [character-fallback families](#character-fallback-and-the-symbol-font),
then the symbol font, and otherwise draws as the font's missing-glyph box.

For example, take a document with bold Garamond text, a library without Garamond, and a serif default of Tinos. Steps
1 to 3 find nothing. The document's font table says Garamond is a serif (`w:family="roman"`), so step 5 draws it in
Tinos Bold, and the [report](#missing-font-reports) lists Garamond (bold) as missing, drawn with Tinos, by `generic`.

Nothing here changes the document. Saved files and the formatting a session reports (the font box) keep the family
the document names. To change the document itself, see [Rewriting a document's fonts](#rewriting-a-documents-fonts).

## Adding fonts

`add` (`AddAsync`) takes TrueType, OpenType, a collection, WOFF or WOFF2, as bytes, a URL or a path, and returns the
faces it added.

```ts file=../../examples/web/fonts.ts#adding
```

```csharp file=../../examples/dotnet/Fonts.cs#adding
```

- **Variable fonts.** Each named instance of a variable font is a face, named as Windows names it ("Bahnschrift
  SemiBold", "Segoe UI Variable Display Semib"), so documents find them by those names, and bold picks the font's Bold
  instance. PDF export draws variable faces as outlines (Type 3 fonts): the text stays selectable, but isn't hinted.
- **System fonts** (.NET and Rust). `FontLibrary.System` loads the machine's fonts; `FontLibrary.CreateEmpty()` starts
  with none. The browser gives the engine no fonts, so a web page adds what its documents use, or resolves them on
  demand.
- **Linux.** On Debian and Ubuntu,
  `fonts-crosextra-carlito fonts-crosextra-caladea fonts-liberation` installs the metric-compatible families the
  [recommended setup](#a-recommended-setup) names. On Alpine, install a font package such as
  `font-liberation`.

`families()` (`GetFamiliesAsync`) lists every family the library can draw, for a font box.

## Aliases

An alias draws a family the library doesn't have with another, such as Arial with Arimo. It applies only when the
family is missing: where Arial is installed, Arial draws. Aliases match names case-insensitively, can apply to one style
only (`bold`, `italic`), chain up to 8 hops (A → B → C), and can point at a [generic category](#generic-defaults)
instead of a family.

```ts file=../../examples/web/fonts.ts#aliases
```

```csharp file=../../examples/dotnet/Fonts.cs#aliases
```

Setting aliases replaces the earlier ones, lays every open document out again, and asks your resolver again about
families it had answered.

## Draw-time replacements

A replacement draws a family with another even when the family is installed: to draw a brand font in place of a
document's, or to make every machine draw the same fonts. When the replacement's target isn't in the library, the
document's own family draws, and the target is [reported](#missing-font-reports) missing with `by: 'original'`.

```ts file=../../examples/web/fonts.ts#replacements
```

```csharp file=../../examples/dotnet/Fonts.cs#replacements
```

## Generic defaults

Generic defaults are opt-in. Set a family for any of five categories (serif, sans serif, monospace, script,
decorative), and a missing family of that category draws with it. With none set, no family draws through a category.

A missing family's category comes from what the document says about it. Documents describe each font they use in
their font table, and Twip reads only what that description states, never the family's name:

| The description says | Category |
|---|---|
| fixed pitch (`w:pitch="fixed"`), or a monospaced PANOSE proportion | Monospace |
| `w:family="roman"` | Serif |
| `w:family="swiss"` | Sans serif |
| `w:family="modern"` | Monospace |
| `w:family="script"` | Script |
| `w:family="decorative"` | Decorative |
| no class (`auto`): a Latin-text PANOSE with a sans serif style | Sans serif |
| no class: a Latin-text PANOSE with any other serif style | Serif |
| no class: a hand-written or decorative PANOSE family kind | Script, Decorative |
| nothing, or nothing that decides | none: the general fallback draws |

Fixed pitch wins over the class, so a family the table calls `swiss` but fixed-pitch is monospace.

```ts file=../../examples/web/fonts.ts#genericDefaults
```

```csharp file=../../examples/dotnet/Fonts.cs#genericDefaults
```

With classification off, only an alias to a category or a resolver's category answer gives a family its category.

## The general fallback and the last resort

The general fallback draws any family nothing else resolves. When it isn't set, or isn't in the library, the first face
added draws. An empty library draws text with the symbol font's boxes, so pages still lay out; with the symbol font off
as well, text takes no space.

```ts file=../../examples/web/fonts.ts#fallback
```

```csharp file=../../examples/dotnet/Fonts.cs#fallback
```

## Character fallback and the symbol font

A character the run's font doesn't have (a symbol, CJK text, an emoji) is drawn from the first of your
character-fallback families that has it, then from the symbol font. Twip searches no families of its own.

```ts file=../../examples/web/fonts.ts#characterFallback
```

```csharp file=../../examples/dotnet/Fonts.cs#characterFallback
```

The symbol font is bundled so bullets and checkboxes draw in an empty library. Replace it with a font of yours (its
first face draws), or turn it off, and characters no font has draw as their font's missing-glyph box and are
[reported](#missing-font-reports). A font that holds no face is refused with `invalidArgument`, and the setting stays as
it was.

```ts file=../../examples/web/fonts.ts#symbolFallback
```

```csharp file=../../examples/dotnet/Fonts.cs#symbolFallback
```

## Resolvers

A resolver fetches fonts on demand. The library asks it once per family and style that a document uses and the library
doesn't have. While it runs, the family is in `state.fonts.pending` and draws with a stand-in; when it answers, the
document is laid out again. In .NET the resolver runs on the thread pool; in the browser it runs on the page, whichever
thread the engine is on.

```ts file=../../examples/web/fonts.ts#resolver
```

```csharp file=../../examples/dotnet/Fonts.cs#resolver
```

A resolver can also decide how to draw a family instead of fetching it. The request carries the document's description
of the family (`description`: its class, pitch, PANOSE and character set, each absent when the document doesn't state
it) and the category that description gives (`proposed`), whether or not classification is on. Answer with:

| Answer | Web | .NET | Then |
|---|---|---|---|
| A font | bytes, a `URL` or a URL string | `FontSource.FromBytes`, `FromUri` | added; the family resolves if the font has it |
| Another family | `{ family }` | `FontSource.FromFamily` | drawn like an alias, for every style |
| A category | `{ category }` | `FontSource.FromCategory` | drawn with that category's default, for every style |
| Nothing | `null` | `null` | reported missing |
| A failure | throw or reject | throw | reported missing, with the error |

```ts file=../../examples/web/fonts.ts#resolverCategory
```

```csharp file=../../examples/dotnet/Fonts.cs#resolverCategory
```

Family and category answers hold until you set aliases or replacements again, which asks about those families anew.

## Missing-font reports

`state.fonts` (`State.Fonts`) and the `fontsChanged` event (`FontsChanged`) report what the document can't draw as it
asks:

- **`missing`**: each family and style that draws with a stand-in. It names the family, the style, the replacement and
  alias targets tried (`tried`), its category, the family that draws instead (`drawnWith`), and why (`by`): the
  `original` family because a replacement's target is missing, a `generic` default, the `fallback`, or the
  `lastResort`. A resolver's failure is in `error`.
- **`pending`**: the same, for families a resolver is still fetching.
- **`characters`**: characters no font has, by the family their runs name (up to 64 per family; `truncated` says there
  were more).

A family that draws through an alias or a replacement whose target the library has isn't missing.

```ts file=../../examples/web/fonts.ts#reports
```

```csharp file=../../examples/dotnet/Fonts.cs#reports
```

The `font-unresolved` warning names each family the first time it goes missing in a document, with what draws it.

## Rewriting a document's fonts

`replaceFont` (`ReplaceFontAsync`) changes the document itself, so the saved file names the new family. It is one undo
step, and returns how many places changed and how many it skipped.

- **Document scope, no style filter** rewrites the text (each run's fonts, paragraph marks, symbol characters) in every
  story, and the tables below it: styles, document defaults, numbering levels, the theme's font slots and the font
  table.
- **Selection scope, or a `bold`/`italic` filter,** rewrites only the text. A matching run whose family comes from a
  style or the theme gets the new family as its own.
- **Tracked changes:** with tracking on, the text's changes are recorded as formatting revisions.
- **Locked content controls:** text inside them is skipped and counted in `skipped`. A selection that includes one is
  refused.
- A rewrite that changes nothing adds no undo step and leaves the document unmodified.

```ts file=../../examples/web/fonts.ts#replaceFont
```

```csharp file=../../examples/dotnet/Fonts.cs#replaceFont
```

## A recommended setup

Documents mostly use Calibri, Cambria, Arial, Times New Roman and Courier New. Where you can't ship those, alias them to
their metric-compatible open stand-ins, which have the same widths and so the same line breaks: Carlito, Caladea, Arimo
(or Liberation Sans), Tinos (or Liberation Serif) and Cousine (or Liberation Mono). Add freely licensed generic defaults
for everything else. Copy this and add the fonts it names; Twip doesn't apply it for you.

```ts file=../../examples/web/fonts.ts#recommended
```

```csharp file=../../examples/dotnet/Fonts.cs#recommended
```

## Google Fonts through a resolver

Twip doesn't contact any font service. To take families from Google Fonts, resolve them yourself:

```ts file=../../examples/web/fonts.ts#googleFonts
```

```csharp file=../../examples/dotnet/Fonts.cs#googleFonts
```

## Migrating

| Before | Now |
|---|---|
| `useGoogleFonts()` / `UseGoogleFonts()` | a resolver: [Google Fonts through a resolver](#google-fonts-through-a-resolver) |
| `substitute(...)` / `Substitute(...)`, `SubstitutionRule` | `setReplacements` / `SetReplacements`, `FontRule` |
| the built-in substitutes (Calibri → Carlito, Aptos → Calibri, ...) | your aliases: [A recommended setup](#a-recommended-setup) |
| the built-in fallback families (Segoe UI Symbol, Noto Sans, ...) | `setCharacterFallback` / `SetCharacterFallback` |
| `setDefault(family)` / `SetDefault(family)` | `setFallback` / `SetFallback` (null for none) |
| `fonts.unresolved` / `Unresolved` (family names) | `fonts.missing` / `Missing` (per family and style, with what draws it) |
| `FontLibrary::mark_unresolved` (Rust) | `FontLibrary::answer(&request, FontAnswer::Missing)` |
| resolver errors logged or ignored | `MissingFont.error` in the report |
| `PdfFontSetOptions.Substitutions`, `DefaultFamily` | `Replacements`, `Fallback` |

## PDF conversion

Headless PDF conversion doesn't use a `FontLibrary`: it takes a font set (`PdfFontSet` in .NET, `twip::pdf::FontSet` in
Rust) that is fixed once built, so many threads can share it. It takes the same settings, and its result reports the
same missing families and characters. See [PDF conversion](pdf-conversion).

Font files are licensed separately from Twip. Check that you may ship the fonts you add.
