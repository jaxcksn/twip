---
title: Line height and spacing
sidebar_position: 5
---

These rules size each line once its content is known ([BREAK-1](break#break-1) onward): first the single line height from the runs' font metrics ([FONT-1](font#font-1)), then the pitch and baseline from the paragraph's `w:spacing/@w:line` and `@w:lineRule`. Below, `A`, `D` and `E` are a run's ascent, descent and external leading in points, after any raise or drop ([FONT-7](font#font-7) to [FONT-10](font#font-10)).

### LINE-1. Single line height {#line-1}

Over the text runs and inline pictures on the line:

```
Abase = max(A + E)        D = max(D)        H = Abase + D
```

Take the two maxima separately: mixed fonts give the largest ascent-with-leading plus the largest descent, not the tallest font's height. An inline picture counts with ascent = its height and descent 0 ([FLOAT-5](float#float-5)). A list number counts only its ascent ([LIST-7](list#list-7)).

Example: 11pt Calibri (A + E 10.4736, D 2.9541) beside 11pt Arial (A + E 9.9580 + 0.3599, D 2.3311) gives H = 10.4736 + 2.9541 = 13.4277.

### LINE-2. Lines of only superscript or subscript text {#line-2}

A superscript or subscript run contributes its own size's ascent and descent with no external leading, shifted and capped as in [FONT-9](font#font-9) and [FONT-10](font#font-10). When a paragraph's last line holds text and all of it is superscript or subscript (no pictures), the paragraph mark's font at the mark's size also takes part: `Abase = max(Abase, Amark + Emark)`, `D = max(D, Dmark)`. Other lines of shifted text use only their own runs.

### LINE-3. The baseline {#line-3}

With single spacing the baseline is `Abase` below the line's top: external leading goes above the text.

### LINE-4. Lines without text {#line-4}

A line with no text, picture or list number (an empty paragraph, or a line of only spaces and tabs) takes its height from the paragraph mark's run properties ([STYLE-5](style#style-5)): `Abase = Amark + Emark`, `D = Dmark` for the mark's font and size. So does the empty line that follows a paragraph-final line break or column break. The mark never changes a line that holds text, except as [LINE-2](line#line-2) says.

Example: an empty paragraph whose mark is 24pt Calibri is a 29.2969pt line, whatever the paragraph's style size.

### LINE-5. Spaces and tabs add no height {#line-5}

Spaces (U+0020, U+3000, U+2002, U+2003; a no-break space counts as text) and tab characters never raise a line: leading, between other text, trailing, or alone before a break. Leave them out of both maxima in [LINE-1](line#line-1). A tab's leader is still drawn at the tab run's size ([TAB-9](tab#tab-9)).

Example: 24pt spaces beside 11pt Calibri text leave the line at 13.4277.

### LINE-6. Single spacing, and the units of `w:line` {#line-6}

`w:lineRule="auto"` (the default when `w:line` is given without a rule) reads `w:line` in 240ths of a line: `m = line / 240`. `atLeast` and `exact` read it in twips: `X = line / 20` points. With no `w:line` at all the paragraph is single spaced.

Single spacing (`auto`, 240): pitch `H`, baseline `Abase`. No rounding: keep the exact values ([UNIT-4](unit#unit-4)).

### LINE-7. Multiple spacing, m ≥ 1 {#line-7}

```
pitch    = m · H
baseline = Abase
below    = pitch − H
```

The extra space goes below the text. It doesn't have to fit at the bottom of a page ([PAGE-2](page#page-2)).

Example: Calibri 11 at 1.5 lines (360): pitch 20.1416, baseline 10.4736, below 6.7139.

### LINE-8. Multiple spacing, m < 1 {#line-8}

```
pitch    = m · H
baseline = pitch − m · D  ( = m · Abase )
```

The cut comes off the top: the text rises toward or past the line's top.

Example: Calibri 11 at `w:line="200"` (m = 0.8333): pitch 11.1898, baseline 8.728.

### LINE-9. At least {#line-9}

```
pitch    = max(X, H)
baseline = Abase      if X ≤ H
           X − D      otherwise
```

The extra space goes above the text.

### LINE-10. Exactly {#line-10}

```
pitch    = X
baseline = 0.8 · X     (whatever the font, unless w:textAlignment moves it, LINE-11)
```

Glyphs may overflow the line box above or below.

Example: exactly 12pt: pitch 12, baseline 9.6 below the line's top.

### LINE-11. Exact spacing with `w:textAlignment` {#line-11}

With `exact` spacing, the paragraph's `w:textAlignment` adds an offset to the 0.8 · X baseline, using the line's `Abase` and `D`:

| `w:textAlignment` | Offset |
|---|---|
| `auto`, `baseline` | 0 |
| `top` | `+Abase` |
| `center` | `+(Abase − D) / 2` |
| `bottom` | `−D / 2` |

Other line rules ignore `w:textAlignment`.

Example: Calibri 11, exactly 18pt, `top`: baseline 14.4 + 10.4736 = 24.8736.

### LINE-12. Exact spacing never grows {#line-12}

An exact line keeps its pitch whatever it holds: taller text or an inline picture overflows upward from the baseline, and the next line still starts one pitch down.

### LINE-13. Multiple spacing on a line with a picture {#line-13}

On a line holding an inline picture, multiple spacing scales only the text's height, not the picture's:

```
h        = Abase + D               (picture included, LINE-1)
Ht       = max text (A + E) + max text D
pitch    = h + (m − 1) · Ht
baseline = Abase                   if m ≥ 1 (below = pitch − h)
           pitch − m · D           if m < 1
```

`Ht` is the mark's single height (`Amark + Emark + Dmark`) when the line has no text. In compat 15, on the paragraph's last line, `Ht = max(Ht, mark height)`. A line of text only keeps `Ht = H`: the mark joins it only as [LINE-2](line#line-2) and [LINE-4](line#line-4) say.

On every line, the height a multiple scales leaves out raised or lowered text's shift (`w:position`, [FONT-7](font#font-7)): such a run counts in `Ht` (and in `H` on a text-only line, so `pitch = h + (m − 1) · H`) with its unshifted ascent and descent, so the line grows by the shift once, unscaled. A superscript's or subscript's own shift ([FONT-9](font#font-9), [FONT-10](font#font-10)) is scaled.

Example: a 60pt picture alone on the line, an 11pt Calibri mark, `w:line="225"` (m = 0.9375): h = 60, Ht = 13.4277, pitch = 60 − 0.0625 · 13.4277 = 59.1608; baseline 59.1608, so the picture's top rises 0.84pt above the line's top.

### LINE-14. East Asian font height {#line-14}

For an East Asian font (`OS/2.ulCodePageRange1` has any of bits 17–21), with `W = usWinAscent + usWinDescent`:

```
A = usWinAscent + 0.15 · W
D = usWinDescent + 0.15 · W
E = 0
```

so the font's single line is `1.3 · W × size / upem`, the extra split evenly above and below, and the font's line gap is never used. Line spacing then scales this height like any other.

### LINE-15. Emphasis marks {#line-15}

A run with `w:em` (`dot`, `comma`, `circle`, `underDot`; `none` is no mark) puts a mark on each of its characters, which takes room in the line:

```
mark = 3.4 / 11 · size        (0.3091 pt per point of the marked text's size)
```

- `dot`, `comma` and `circle` sit above the line's tallest text: add the largest `mark` over the line's marked runs to `Abase`, so the baseline moves down by it.
- `underDot` sits below: add its largest `mark` to `D`.
- The mark's height is added once, after [LINE-1](line#line-1) to [LINE-4](line#line-4): multiple spacing doesn't scale it (`pitch = h + (m − 1) · Ht` with `h` including it, [LINE-13](line#line-13)); at-least spacing compares `X` with the height including it; exact spacing keeps its pitch ([LINE-12](line#line-12)).

Example: 11pt Calibri with `w:em="dot"`, single spaced: 10.4736 + 3.4 = 13.8736 above the baseline, a 16.8277pt line.
