---
title: Style resolution
sidebar_position: 3
---

Before laying anything out, resolve every paragraph's and every run's formatting to one complete set of values. Each property comes from the highest level of the cascade that sets it; a few properties (toggles, tab stops, list indents, East Asian theme fonts) follow special rules given here.

### STYLE-1. The cascade {#style-1}

Paragraph properties, lowest level first:

```
docDefaults/pPrDefault → table style (base, then its conditional regions) → paragraph style chain → direct w:pPr
```

Run properties, lowest level first:

```
docDefaults/rPrDefault → table style (base, then its conditional regions) → paragraph style chain → character style chain (w:rStyle) → direct w:rPr
```

A list level's `w:pPr` slots into the paragraph cascade as [STYLE-6](style#style-6) describes. Table-style regions and their order are in [TABLE-3](table#table-3). Properties nothing sets take the OOXML defaults: 10pt text, no spacing, single line spacing, left alignment.

### STYLE-2. Style chains and defaults {#style-2}

- `w:basedOn` is ordinary inheritance: the derived style overrides what it sets and inherits the rest, for paragraph and character styles alike.
- A style with no `w:basedOn` sits directly on `docDefaults`, not on Normal.
- Merge per attribute, not per element: a direct `<w:spacing w:after="0"/>` keeps the style's `w:before`; a direct `<w:ind w:hanging="360"/>` keeps the style's `w:left` and `w:right`; a direct `<w:rFonts w:hAnsi="Arial"/>` keeps the style's `w:ascii`.
- A paragraph with no `w:pStyle`, or naming a style that doesn't exist, uses the paragraph style marked `w:default="1"` (the last one if several are marked); with none marked, the paragraph style whose id is `Normal`.
- The default character style is never applied implicitly, only where a run names it.
- A document whose styles part has no `w:docDefaults` gets fixed defaults: Aptos 12pt in every font slot, `w:kern` 1pt, space after 8pt, line spacing 278 auto.

### STYLE-3. Toggle properties {#style-3}

`b`, `i`, `caps`, `smallCaps`, `strike`, `vanish`, `bCs`, `iCs`, `emboss`, `imprint`, `outline` and `shadow` are toggles:

- **Within one style chain** (a style and its `basedOn` ancestors) a toggle is on if any style in the chain turns it on: bold based on bold is bold.
- **Across levels** (docDefaults, the table style, the paragraph style chain, the character style chain) each level that turns a toggle on flips it: a bold paragraph style plus a bold character style is not bold; table style, paragraph style and character style all bold is bold.
- The table style's base and conditional regions count as one chain: no flipping between regions.
- `w:val="0"` in a style does nothing; it doesn't turn the toggle off.
- **Direct formatting is absolute**: `<w:b/>` is bold, `<w:b w:val="0"/>` is not bold, whatever the styles say.

`dstrike` and `u` are not toggles; they override like any other property.

### STYLE-4. Table styles sit below the paragraph style {#style-4}

The table style is lower in the cascade than the paragraph style, including Normal (which a paragraph without `w:pStyle` uses, [STYLE-2](style#style-2)). Whatever Normal sets (size, spacing, …) beats the table style and its regions, even when Normal sets it to the docDefaults value. Table-style properties Normal doesn't set (colour, `w:jc`, …) apply. With an empty Normal the table style's values show.

One exception, for the font size, in compat ≤ 14 only. When all of these hold:

- the cell paragraph's size comes from the default paragraph style itself (no `w:pStyle`, or a style based on it that sets no size; a size set by the paragraph style, even 12pt, or by the run, blocks it);
- that size is exactly 12pt;
- the table (not the default table style) has a table style whose `w:basedOn` chain sets at least one paragraph or run property, or has a conditional region (`w:tblStylePr`). Only `w:tblPr`, or an empty `w:rPr`, doesn't count;

the cell text takes the table style's size instead: the active conditional regions' size, else the table style's `w:sz`, else docDefaults'. If that is 10pt, or there is none, the 12pt stays. `useWord2002TableStyleRules` changes nothing.

Example: compat 12, docDefaults 11pt, Normal 12pt, a table style setting only `w:rFonts`: cell text without `w:pStyle` is 11pt. With a table style `w:sz` 28 it is 14pt; with docDefaults 10pt, or Normal 11pt, it keeps Normal's size.

### STYLE-5. Paragraph-mark run properties {#style-5}

The paragraph mark's run properties (`w:pPr/w:rPr`, through its `w:rStyle` and the paragraph's cascade) don't change a line that holds text. They are used for:

- the height of an empty paragraph's line, and of any line holding nothing but spaces and tabs ([LINE-4](line#line-4));
- the paragraph's last line when it holds only superscript or subscript text ([LINE-2](line#line-2));
- the text height on a picture line under multiple spacing ([LINE-13](line#line-13));
- the list number's formatting ([LIST-6](list#list-6)).

### STYLE-6. Numbering and styles {#style-6}

- **Direct `w:numPr`**: the level's `w:pPr` (its indent) sits above the paragraph style: `… → paragraph style chain → level → direct`. A direct `w:ind` still beats the level ([LIST-4](list#list-4)).
- **`w:numPr` in a paragraph style** applies the list, and the style's own `w:ind` beats the level's: `… → level → paragraph style chain → direct`.
- A direct `w:numId` of 0 removes the list; a direct `w:numPr` with another `w:numId` replaces it. A direct `w:ilvl` overrides the style's.
- **One style per list level.** A level's number is drawn only in paragraphs whose list comes from direct numbering, or whose style is the one linked to that level: the level's `w:pStyle`, else the first paragraph style in `styles.xml` order whose `w:numPr` points at that level (through any `w:numId` of the same `w:abstractNum`). Other styles pointing there get the level's indent but no number, and don't advance its counter ([LIST-12](list#list-12)).

### STYLE-7. Hidden text {#style-7}

- A run with `w:vanish` takes no space and draws nothing.
- A paragraph whose runs are all hidden but whose mark isn't is an empty paragraph: one line from the mark ([LINE-4](line#line-4)).
- A hidden paragraph mark joins its paragraph to the next one; lay them out as one paragraph. It keeps the second paragraph's properties and mark, and the first one's list number.

### STYLE-8. An empty East Asian theme slot {#style-8}

When a level sets `w:eastAsiaTheme` over a lower level that names an East Asian font, and the theme leaves that slot empty, the East Asian font is the one named below: the empty slot is transparent. With no named font below, the empty slot falls back to the theme's Latin font of the same group (minor or major), and the run still counts as East Asian for [FONT-14](font#font-14).

Example: docDefaults `w:eastAsia="Times New Roman"` under Normal's `w:eastAsiaTheme="minorEastAsia"`, with an empty minor East Asian font in the theme: the East Asian font is Times New Roman.

A named East Asian font at a higher level doesn't fill an empty slot that has no named font under it: the run still counts as East Asian with its own font for [FONT-14](font#font-14), [BREAK-8](break#break-8) and [BREAK-9](break#break-9), as if it named nothing (an East Asian font it names is used as that font).

Example: docDefaults `w:eastAsiaTheme="minorEastAsia"` (empty in the theme) and a run with `w:ascii="Arial" w:eastAsia="Times New Roman"`: in compat 12 with `compressPunctuation` its Arial spaces squeeze ([BREAK-9](break#break-9)). With docDefaults `w:eastAsia="Times New Roman"` under Normal's theme slot, the same run's spaces don't.

### STYLE-9. Tab stops accumulate {#style-9}

Tab stops don't replace each other level by level; they accumulate through the paragraph cascade (docDefaults, table style, paragraph style chain, list level, direct). A stop at the same position as an inherited one (within 0.025pt) replaces it; `w:val="clear"` removes the inherited stop at its position. Sort the result by position.

Example: a style with stops at 1in and 3in, and direct `w:tab w:val="clear" w:pos="4320"` plus a right stop at 6in: the paragraph's stops are 1in and 6in.
