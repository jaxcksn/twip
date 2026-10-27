---
title: Fonts and text widths
sidebar_position: 2
---

These rules turn a run of text into glyphs with widths and vertical metrics. Pick the font for each character, shape each stretch of equally formatted text with OpenType shaping (GSUB and GPOS), and read the vertical metrics from the font tables. Everything is in points, `size` is the run's font size in points and `upem` is the font's `head.unitsPerEm`; a value in font units becomes points as `value × size / upem`.

### FONT-1. Vertical metrics {#font-1}

For each run, take ascent **A**, descent **D** and external leading **E** from the font, scaled by `size / upem`:

| | `OS/2.fsSelection` bit 7 (`USE_TYPO_METRICS`) clear | bit 7 set |
|---|---|---|
| A | `OS/2.usWinAscent` | `OS/2.sTypoAscender` |
| D | `OS/2.usWinDescent` | `−OS/2.sTypoDescender` |
| E | `max(0, (hhea.ascender − hhea.descender + hhea.lineGap) − (usWinAscent + usWinDescent))` | `max(0, OS/2.sTypoLineGap)` |

A font without an OS/2 table uses `hhea.ascender`, `−hhea.descender` and `max(0, hhea.lineGap)`. An East Asian font replaces these values ([LINE-14](line#line-14)).

Example: Arial (upem 2048, win 1854/434, hhea 1854/−434/67) at 12pt: A = 10.8633, D = 2.5430, E = 67 × 12/2048 = 0.3926.

### FONT-2. Glyph advances {#font-2}

A glyph's advance is its shaped advance (`hmtx.advanceWidth` plus any GPOS adjustment) × `size / upem`, unrounded: no hinting, no snapping to twips or pixels. A run with `w:w` (character scale, percent) multiplies every advance by `w / 100`.

### FONT-3. Kerning {#font-3}

`w:kern` (half-points) is a threshold: shape with the OpenType `kern` feature on when `w:kern > 0` and the run's effective size (after superscript or subscript, [FONT-8](font#font-8)) is at least the threshold. `w:kern` absent or 0 means no kerning. Kerning values are unrounded font units × `size / upem`.

Example: `w:kern w:val="2"` (1pt) kerns every run of 1pt or more, which in practice is all text.

### FONT-4. Ligatures {#font-4}

`w14:ligatures` selects OpenType features: `standard` turns on `liga` and `clig`, `contextual` turns on `calt`, `historical` turns on `hlig`, `discretional` turns on `dlig`; the combined values (`standardContextual`, `all`, …) turn on each of their parts. With no `w14:ligatures` anywhere in the cascade, shape with all four groups off.

### FONT-5. Run width {#font-5}

A run's width is the sum of its shaped advances (kerning included, [FONT-2](font#font-2), [FONT-3](font#font-3)) plus the run's character spacing `w:spacing` (twips, may be negative) once per character.

Example: a Calibri 11 space is 2.4868pt; with `w:spacing w:val="20"` it is 3.4868pt.

### FONT-6. Small caps {#font-6}

With `w:smallCaps` on (and `w:caps` off), split the run's text:

- lower-case letters (accented ones too) are drawn as capitals at the small size `hp(0.8 × size) / 2` ([UNIT-5](unit#unit-5)): 11pt → 9pt, 12pt → 9.5pt, 13pt → 10.5pt;
- a space that follows a lower-case letter (or another such space) goes with it at the small size;
- capitals, digits, punctuation, and spaces after them stay at the full size.

Each piece is shaped and sized at its own size ([LINE-1](line#line-1)). `w:caps` simply draws every letter as a capital at the run's size.

### FONT-7. Raised and lowered text (`w:position`) {#font-7}

`w:position` is a baseline shift in half-points: `raise = val / 2` points, positive up. It adds to any superscript or subscript shift. It changes the run's metrics:

```
ascent  = max(0, ascent + raise)
descent = max(0, descent − raise)
```

so raised text grows the line above the baseline and lowered text grows it below.

Example: Calibri 11 with `w:position w:val="6"`: raise 3pt, ascent 10.4736 + 3 = 13.4736, descent max(0, 2.9541 − 3) = 0.

### FONT-8. Superscript and subscript size {#font-8}

`w:vertAlign="superscript"` draws at `hp(size × OS/2.ySuperscriptYSize / upem) / 2` points; `subscript` uses `ySubscriptYSize`. Without an OS/2 table, both sizes are 0.65 em. Kerning and widths use this size.

Example: Calibri (ySuperscriptYSize 1331) at 11pt: 11 × 1331/2048 = 7.149 → 14 half-points, 7pt.

### FONT-9. Superscript raise {#font-9}

In half-points, with `A` and `E` at the base size and `Asup` the ascent at the superscript size ([FONT-8](font#font-8)):

```
a     = hp(A + E)
asup  = hp(Asup)
limit = max(a div 3, a − asup)
raise = min(hp(size × OS/2.ySuperscriptYOffset / upem), limit)
```

The run is raised by `raise / 2` points. Its ascent is the superscript font's `Asup` (no external leading) plus the raise, but never more than the base size's `A + E`; its descent is the superscript size's `D` less the raise, never below 0. Without an OS/2 table, the offset is 0.35 em.

Example: Calibri 11: a = hp(10.4736) = 21, asup = hp(1950 × 7/2048 = 6.665) = 13, limit = max(7, 8) = 8; the OS/2 offset (5.24pt) gives 10, so raise = 8 half-points, 4pt.

### FONT-10. Subscript drop {#font-10}

In half-points, with `D` at the base size and `Dsub` the descent at the subscript size:

```
off   = hp(size × OS/2.ySubscriptYOffset / upem)
limit = hp(D) − hp(Dsub)
drop  = limit > 0 ? clamp(off, 0, limit) : max(off, 0)
```

The run is lowered by `drop / 2` points. Its ascent is the subscript size's `A` (no external leading) less the drop, never below 0; its descent is `Dsub` plus the drop, but never more than the base size's `D`. A font with offset 0 doesn't drop. Without an OS/2 table, the offset is 0.14 em.

### FONT-11. Reference metrics {#font-11}

Font units; the single line height is `(A + D + E) × size / upem` ([LINE-1](line#line-1)).

| Font | upem | `USE_TYPO_METRICS` | A | D | E | Single line at 12pt |
|---|---|---|---|---|---|---|
| Calibri | 2048 | 0 | 1950 | 550 | 0 | 14.6484 |
| Arial | 2048 | 0 | 1854 | 434 | 67 | 13.7988 |
| Times New Roman | 2048 | 0 | 1825 | 443 | 87 | 13.7988 |
| Georgia | 2048 | 0 | 1878 | 449 | 0 | 13.6348 |
| Verdana | 2048 | 0 | 2059 | 430 | 0 | 14.5840 |
| Courier New | 2048 | 0 | 1705 | 615 | 0 | 13.5938 |
| Cambria | 2048 | 0 | 1946 | 455 | 0 | 14.0684 |
| Aptos | 2048 | 1 | 1923 | 577 | 0 | 14.6484 |

The pitch scales linearly with size: Calibri gives 12.2070 at 10pt, 13.4277 at 11pt and 15.8691 at 13pt.

### FONT-12. Font slots and the East Asian hint {#font-12}

Pick each character's font from the run's `w:rFonts` slots:

1. East Asian characters (U+1100–11FF, U+2E80–9FFF, U+A960–A97F, U+AC00–D7FF, U+F900–FAFF, U+FE30–FE4F, U+FF00–FFEF, U+20000–2FFFF) use `w:eastAsia`. With `w:hint="eastAsia"`, so do the characters both scripts share: U+00A1, A4, A7, A8, AA, AD, AF, B0–B4, B6–BA, BC–BF, D7, F7 and U+2000–2BFF (except U+200B–200F and U+2028–202E).
2. Otherwise U+0000–007F use `w:ascii` and everything else `w:hAnsi`.

A hinted shared character whose `w:eastAsia` font is not East Asian (its `OS/2.ulCodePageRange1` has none of bits 17–21: Japanese, Chinese, Korean) is drawn in MS Mincho instead; when that font is Times New Roman, it is drawn in the `w:hAnsi` font, as without the hint.

A checkbox content control (`w14:checkbox`) shows one symbol: the first character of its text; else its `w:sym` character, in the `w:sym` font; else the character of the state it is in, `w14:checkedState` or `w14:uncheckedState` (default U+2612 ☒ and U+2610 ☐). A symbol from its text or its state goes through the East Asian slot only when its run has `w:hint="eastAsia"`: it is then the run's `w:eastAsia` font, with the MS Mincho replacement above. Without the hint, or from `w:sym`, it draws in its own font (the run's `w:hAnsi` font, or the `w:sym` font).

Example: ☐ (U+2610) in Segoe UI Symbol with the hint is a 12pt MS Mincho box; without the hint it is a Segoe UI Symbol box.

A character the chosen font has no glyph for (other than whitespace, the soft hyphen and the no-break hyphen) falls back to another installed font that has one.

### FONT-13. Symbol fonts {#font-13}

In Symbol, Wingdings, Wingdings 2, Wingdings 3 and Webdings, a `w:sym` character code below U+0100 means U+F000 + code. A Latin-1 code written in `w:t` in Symbol or Wingdings draws the same glyph as its U+F0xx code. The page's text carries the character the glyph shows, not the code: Symbol "·" (B7) is •, Wingdings "§" (A7) is ▪, Wingdings FC is ✓.

### FONT-14. Balanced space runs {#font-14}

With `w:balanceSingleByteDoubleByteWidth` in the settings, every space of a run of two or more consecutive spaces (U+0020 or U+00A0, across run boundaries) is as wide as the East Asian font's average character, `OS/2.xAvgCharWidth × size / upem` (times `w:w / 100`, plus `w:spacing`). That applies when the run's `w:eastAsia` font is an East Asian font, or when its East Asian slot is a theme font the theme leaves empty (then use the space's own font). A single space, a named non-East-Asian font in the slot, or the setting off leaves spaces as shaped.

Example: MS Gothic's average character is half an em, so at 10pt each space of a run of three is 5pt.

### FONT-15. Decorations {#font-15}

Draw a run's highlight (or run shading) behind its text, from `baseline − A` to `baseline + D` of its font, including trailing spaces. Draw underline and strikethrough over the text, but not under the spaces that end a line:

- underline at `post.underlinePosition` below the baseline, `post.underlineThickness` thick (at least 0.5pt); `double` draws a second line two thicknesses lower, `thick` doubles the thickness, `words` skips spaces, every other style draws as single;
- strikethrough at `OS/2.yStrikeoutPosition` above the baseline, `OS/2.yStrikeoutSize` thick (at least 0.5pt); `w:dstrike` draws two lines one thickness above and below that.

List numbers take no underline and no highlight. Text whose colour is automatic is white over a dark highlight or shading (luma `0.299 R + 0.587 G + 0.114 B < 110`), black otherwise.

### FONT-16. Shaping stretches {#font-16}

Shape consecutive text of equal formatting as one stretch, even across `w:r` boundaries and across things that draw nothing (hidden text, [STYLE-7](style#style-7); field markers and codes; bookmarks; content-control boundaries), so kerning and ligatures work across them. Split a stretch where the font changes ([FONT-12](font#font-12)), and at anything that is drawn or takes space (a tab, a break, a picture, a field's value).
