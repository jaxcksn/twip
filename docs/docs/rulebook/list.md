---
title: Lists and numbering
sidebar_position: 10
---

A numbered or bulleted paragraph starts its first line with the level's number (its label), then a suffix, then the text. Below, `left` and `firstLine` are the paragraph's effective indents after the cascade ([STYLE-1](style#style-1), [STYLE-6](style#style-6)); `firstLine` is negative for a hanging indent. Which paragraphs show a number at all is in [STYLE-6](style#style-6).

### LIST-1. Where the number goes {#list-1}

The first-line position is `left + firstLine` (`left − hanging`). By `w:lvlJc`:

- `left`: the number starts there;
- `right`: the number ends there;
- `center`: the number is centred there.

The suffix and text follow the number's end.

Example: `w:left="720" w:hanging="360"`, a right-aligned "iii." 12.5pt wide: it starts at 36 − 12.5 = 23.5pt and ends at 36pt.

### LIST-2. The suffix {#list-2}

`w:suff`:

- `tab` (the default): a tab character after the number, resolved like any tab ([TAB-1](tab#tab-1) to [TAB-3](tab#tab-3)): the next custom stop past the number's end, else the next default stop; but when the number ends left of `left` and that stop lies beyond `left`, or there is no custom stop past it, the left indent itself (default stops before `left` don't count). A number wider than the hanging indent therefore goes to the next stop after its end.
- `space`: one space in the number's font and size.
- `nothing`: the text starts right after the number.

### LIST-3. Wrapped lines {#list-3}

Every line after the first starts at `left`.

### LIST-4. The paragraph's own indent wins {#list-4}

A paragraph's direct `w:ind` overrides the level's indent, attribute by attribute. A level's indent beats the paragraph style's indent only with direct numbering ([STYLE-6](style#style-6)).

### LIST-5. Lists in table cells {#list-5}

In a table cell, the indents count from the cell's content box ([TABLE-24](table#table-24)), inside the cell margins.

### LIST-6. The number's formatting {#list-6}

Format the number with the paragraph mark's run properties ([STYLE-5](style#style-5)), with the level's `w:rPr` applied on top. The suffix (tab or space) uses the same font and size.

### LIST-7. The number's height {#list-7}

A number contributes its ascent (with external leading) but not its descent to the line ([LINE-1](line#line-1)):

```
lineHeight = max(textAscent, numberAscent) + textDescent
```

This holds for large numbers, other fonts and symbol-font bullets alike. The suffix tab adds nothing ([LINE-5](line#line-5)).

Example: a 20pt Calibri number (A 19.043) before 11pt Calibri text: height 19.043 + 2.954 = 21.997.

### LIST-8. Bullets {#list-8}

A bullet is a level with `w:numFmt="bullet"`: the label is `w:lvlText` as written (for example U+F0B7 in Symbol, [FONT-13](font#font-13)). It is placed and sized like a number.

### LIST-9. Number formats {#list-9}

| `w:numFmt` | 1, 2, 3, 10, 27 |
|---|---|
| `decimal` | 1, 2, 3, 10, 27 |
| `decimalZero` | 01, 02, 03, 10, 27 |
| `lowerLetter`, `upperLetter` | a, b, c, j, aa (then bb, cc: the letter repeated) |
| `lowerRoman`, `upperRoman` | i, ii, iii, x, xxvii |
| `ordinal` | 1st, 2nd, 3rd, 10th, 27th (11th, 12th, 13th) |
| `cardinalText` | One, Two, Three, Ten, Twenty-seven |
| `ordinalText` | First, Second, Third, Tenth, Twenty-seventh |
| `numberInDash` | - 1 -, - 2 -, … |
| `none` | nothing |

Any other format shows decimal.

### LIST-10. Level text {#list-10}

In `w:lvlText`, `%n` is level n's current value in level n's own format; other characters are copied.

Example: level 2 text `%1.%2.` with level 1 in `upperRoman` at 2 and level 2 in `decimal` at 3 gives "II.3.".

### LIST-11. Legal numbering {#list-11}

With `w:isLgl` on a level, every `%n` in that level's text shows as decimal, whatever level n's format.

### LIST-12. Counters {#list-12}

- Counters belong to the `w:abstractNum`: two `w:num` of the same abstract list without overrides continue one sequence. A `w:numStyleLink` is followed to the numbering style's list.
- A level's first value is its `w:start` (0 when absent). `w:lvlOverride/w:startOverride` starts that `w:num`'s level at the value, the first time it is used.
- Each numbered paragraph adds 1 to its level. A level restarts after any higher level appears, unless its `w:lvlRestart` says otherwise: `w:lvlRestart="n"` restarts it only after levels 1 to n (1-based), `0` never restarts it.
- Counting runs through the whole document in order, through tables too.
- A level used before its parents appear counts the parents from their start values: a level-2 item first shows "1.1.", and the next level-1 item is "2.".

### LIST-13. Numbers aren't kerned {#list-13}

Shape the number as a separate object with kerning off: no kerning inside it, and none between it and the text that follows.

Example: "Two)" keeps the unkerned "T" and "w".
