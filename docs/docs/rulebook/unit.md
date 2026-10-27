---
title: Units, precision and conventions
sidebar_position: 1
---

These conventions hold on every page of the rulebook. Convert every length in the file to points as you read it, keep all arithmetic in floating point, and round only where a rule names the rounding. The worked numbers at the end let you check a hand layout of a plain page before you go further.

### UNIT-1. The compatibility mode {#unit-1}

Read the mode from `w:settings/w:compat/w:compatSetting[@w:name="compatibilityMode"]/@w:val`. The rules describe mode 15; where a rule says "compat ≤ 14" it means any lower mode. A document that names no mode lays out as mode 12. See [COMPAT-1](compat#compat-1) for the modes themselves.

### UNIT-2. The line box {#unit-2}

A line occupies a box that starts at the line's top and is as tall as its pitch. The baseline sits inside it at the offset its line-spacing rule gives ([LINE-6](line#line-6) to [LINE-11](line#line-11)). Lines stack: the next line's top is this line's top plus its pitch.

### UNIT-3. Units {#unit-3}

| Unit | Size | Where it appears |
|---|---|---|
| twip (`dxa`) | 1/20 pt | page size, margins, indents, paragraph spacing, tab stops, `w:spacing/@w:line` for `atLeast` and `exact`, run `w:spacing`, table widths |
| half-point | 1/2 pt | `w:sz`, `w:position`, `w:kern` |
| eighth-point | 1/8 pt | border `w:sz` (absent: 4, a 0.5pt line) |
| point | 1 pt | border `w:space` |
| 240ths of a line | 1/240 line | `w:spacing/@w:line` for `auto` |
| EMU | 1/12700 pt | DrawingML sizes and offsets |
| fiftieth of a percent | 5000 = 100% | `w:type="pct"` table widths (a value written as `"50%"` is a plain percentage) |

### UNIT-4. Precision {#unit-4}

Lay out in floating-point points. Never round a position, width or pitch to twips, half-points or device pixels. Glyph advances, kerning, line heights and spacing all stay unrounded unless a rule says otherwise.

### UNIT-5. Rounding to half-points {#unit-5}

Where a rule rounds a size or an offset to half-points, use `hp(x) = round(2x)`, rounding halves away from zero (not to even). Integer division in those rules truncates: `A div 3` with `A = 31` is 10.

Example: a 7.25pt result gives `hp = round(14.5) = 15` half-points, 7.5pt.

### UNIT-6. Cumulative positions {#unit-6}

Keep positions cumulative: a run's x is the running pen from the line's left edge, a line's y is the running total of gaps and pitches from the top of its area. Never add up rounded widths. If you convert to device units for drawing, convert each absolute position and take differences, so rounding error never builds up along a line or down a page.

### UNIT-7. Output grids don't move the layout {#unit-7}

A 600-dpi output device draws on a 1/600 in (0.12pt) grid, so a drawn coordinate can sit up to one grid step from its layout value. That snapping never changes a pitch, a line break or a page break; lay out as if it didn't exist. The one layout value taken from the grid is the tab leader pitch ([TAB-9](tab#tab-9)).

### UNIT-8. Each run has its own baseline shift {#unit-8}

A line has one baseline, but each run draws at `baseline − raise`, where `raise` comes from superscript and subscript ([FONT-9](font#font-9), [FONT-10](font#font-10)) and from `w:position` ([FONT-7](font#font-7)). One line can hold runs on several shifted baselines; give each run its own y.

### UNIT-9. Worked numbers {#unit-9}

Calibri 11pt on a Letter page (612 × 792pt) with 72pt margins, single spacing, no space before or after:

| Quantity | Value | How |
|---|---|---|
| Single line pitch | 13.4277 | `A + E` = 1950 × 11/2048 = 10.4736, `D` = 550 × 11/2048 = 2.9541 ([LINE-1](line#line-1)) |
| First body baseline | 82.47 | 72 + 10.4736 |
| Lines per page | 48 | body height 648 / 13.4277 = 48.26 |
| Space width | 2.4868 | 463 × 11/2048 |
| Header baseline, `w:header` 720 | 46.47 | 36 + 10.4736 |
| One-line footer baseline, `w:footer` 720 | 753.05 | 792 − 36 − 2.9541 |
| Two equal columns, 36pt apart | width 216, second at x 324 | (468 − 36)/2; 72 + 216 + 36 |
