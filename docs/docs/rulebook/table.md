---
title: Tables
sidebar_position: 15
---

These rules lay out a `w:tbl`: first the table's properties and its shared column grid, then the column widths (fixed layout, or the autofit algorithm, which works in integer twips), then each cell's place, each row's height, the borders and shading, and finally how rows go onto pages. Work in that order: column widths need nothing but the cells' content widths, row heights need the column widths, and pagination needs the row heights. Geometry is in points (1pt = 20 twips) unless a rule says twips.

## Scope and notation

### TABLE-1. Notation {#table-1}

- `pad` = a cell's left + right padding (margins, or half borders where those are wider, see [TABLE-7](#table-7)).
- `w` = a border's width. A border of `w:sz` N is `N/8` pt wide (no `w:sz`: 0.5pt). A double border counts three times its width ([TABLE-37](#table-37)).
- `textW` = the width between the page margins, or the column width, or (for a nested table) the width of the cell's content box.
- Column widths run from border centre to border centre.
- A table's grid has `n` columns: the larger of the number of `w:gridCol` entries and, over all rows, `w:gridBefore + Σ w:gridSpan + w:gridAfter`. A row's first cell starts at grid column `w:gridBefore`; each cell covers `w:gridSpan` columns (default 1), cut off at the grid's end.
- A floating table (`w:tblpPr`) is laid out like any other table against the column width, but it takes no room in the flow: it sits at its anchored position and the text wraps around it ([TABLE-49](#table-49)).

## Table properties

### TABLE-2. Table style and cell margins {#table-2}

1. The table's style is its `w:tblStyle` if that style exists, else the document's default table style (the table style with `w:default="1"`, normally "Normal Table", whose `w:tblCellMar` gives 108-twip left and right margins and 0 top and bottom).
2. Each property of `w:tblPr` and `w:trPr` is the direct one if written, else the table style's (through `w:basedOn`); see [TABLE-3](#table-3) for cell properties.
3. A cell's margin on each side is its `w:tcMar` value, else the table's `w:tblCellMar` (direct, else from the style). If neither gives one:
   - top and bottom: 0;
   - left and right: 0 if the table has a style (step 1), even one that sets no margins; **10 twips** (0.5pt) if the table has no style at all (no `w:tblStyle` that exists and no default table style).

### TABLE-3. Table-style conditional formatting {#table-3}

A cell's table-style properties (`w:pPr`, `w:rPr`, `w:tcPr`) merge, property by property, the style's whole-table properties and then each **active** conditional region in this order, lowest first (a later one overrides):

`band1Horz, band2Horz, band1Vert, band2Vert, lastCol, firstCol, lastRow, firstRow, seCell, swCell, neCell, nwCell`

Each region is looked up through the `w:basedOn` chain. Toggle properties don't XOR between regions (bold in both `firstRow` and `firstCol` is bold; see [STYLE-3](style#style-3)). Direct `w:tcPr` beats the merged result; where the table style sits against paragraph styles is [STYLE-4](style#style-4).

Which regions are active:

1. Read `w:tblLook`: the attributes `firstRow`, `lastRow`, `firstColumn`, `lastColumn`, `noHBand`, `noVBand`; an attribute that is absent takes its bit from the hex `w:val` (0x0020, 0x0040, 0x0080, 0x0100, 0x0200, 0x0400 in that order). With no `w:tblLook` at all: `firstRow`, `firstColumn` and `noVBand` on, the rest off. The style's own `w:tblLook` is never used.
2. A row or column bit counts only when the style (chain) formats one of its regions: `firstRow` when it defines `firstRow`, or `nwCell` with the `firstColumn` bit, or `neCell` with the `lastColumn` bit; `lastRow` likewise with `lastRow`, `swCell`, `seCell`; `firstColumn` with `firstCol`, `nwCell` (and the `firstRow` bit), `swCell` (and the `lastRow` bit); `lastColumn` with `lastCol`, `neCell`, `seCell`. With `w:useWord2002TableStyleRules` in compat ≤ 14, corner regions don't keep a bit. Below, "bit" means a bit that counts.
3. The first-row run: when the style defines `firstRow` or the `firstRow` bit counts, the table's leading rows that are header rows (`w:tblHeader`) or row 0 with the `firstRow` bit, so header rows right after such a row 0 join the run too.
4. Each cell gets **one** row region: `firstRow` in the run (with the bit); else `lastRow` in the last row (with the bit); else, with `w:tblStyleRowBandSize` = B > 0 and `noHBand` off, `r = rowIndex − run length` and `band1Horz` when `r ≥ 0` and `(r div B)` is even, else `band2Horz` (so a run row without the bit is `band2Horz`).
5. **One** column region the same way: `firstCol` for the first cell (with the bit); else `lastCol` for the last cell (with the bit); else column banding with `w:tblStyleColBandSize`, `noVBand` and `c = cellIndex − (1 with the firstColumn bit)`.
6. At most **one** corner, the first that applies of: `nwCell` (first cell of a run row; firstRow and firstColumn bits), `neCell` (last cell of a run row; firstRow and lastColumn), `swCell` (first cell of the last row; lastRow and firstColumn), `seCell` (last cell of the last row; lastRow and lastColumn). A one-cell table with every bit gets only `nwCell`, even when the style defines no `nwCell`.

Example: B = 2, an active `firstRow`: rows 1–2 are band1, rows 3–4 band2, rows 5–6 band1.

## Column grid

### TABLE-4. One shared grid {#table-4}

A table has one set of column boundaries, shared by every row. A row never gets its own boundaries, even when its `w:tcW` values sum differently from other rows'. `w:tblGrid` widths are only a fallback: any cell with a dxa `w:tcW` overrides the grid for its column, upward or downward ([TABLE-13](#table-13)). The grid widths are used for fixed-layout cells without a width ([TABLE-12](#table-12)) and for `w:tcW type="nil"`.

### TABLE-5. Fixed and autofit layout {#table-5}

Both layouts run the same algorithm ([TABLE-6](#table-6)). `w:tblLayout w:type="fixed"` changes only its inputs:

| | Autofit (default) | Fixed |
|---|---|---|
| Cell without `w:tcW`, or auto | sized by its content | dxa at its `w:tblGrid` width |
| Cell min and max | from the content ([TABLE-7](#table-7)) | `cellPad + 20` twips each; the content is never looked at |
| An unbreakable piece wider than the column | the column widens to hold it | ignored: the column keeps its width and the piece breaks inside ([BREAK-4](break#break-4)) |
| Σ columns > available width A | shrunk to A | kept: the table runs past the margin |
| dxa `w:tblW` below Σ columns | shrink in proportion to (max − min) | the same, with min = padding + 20 |
| `w:tblW` above Σ columns | grow in proportion to max | the same |
| pct cells | their share of A, beating dxa | the same |

A nested table is laid out with its outermost table's layout, whatever its own `w:tblLayout`: inside a fixed table a nested autofit table's auto cells take their grid widths, and inside an autofit table a nested fixed table is fitted to its content.

## Autofit

### TABLE-6. The autofit algorithm and its arithmetic {#table-6}

To get a table's column widths:

1. Compute every cell's **min**, **max** and **hardMin** ([TABLE-7](#table-7) to [TABLE-11](#table-11)) and its width type and preference ([TABLE-12](#table-12)).
2. Add the single-span cells to their columns ([TABLE-13](#table-13)), then the skipped regions ([TABLE-15](#table-15)).
3. Spread the spanning cells over their columns ([TABLE-14](#table-14), [TABLE-16](#table-16)).
4. Resolve the column widths from the table-wide inputs ([TABLE-17](#table-17) to [TABLE-19](#table-19)).
5. Hand out the rounding leftovers ([TABLE-20](#table-20)).
6. Add the cell spacing and build the column edges ([TABLE-21](#table-21)).

Arithmetic:

- Everything is in **integer twips**. A width in points becomes twips by `round(pt × 20)`, except content widths, which round **up** ([TABLE-7](#table-7)).
- `rdiv(a, b, c)` = `a·b/c` rounded half away from zero; 0 when `c = 0`.
- `MAXW` = 31680 twips (22in): no cell min, max or hardMin, and no pct column width, exceeds it.
- "div" is integer division truncating toward zero.

### TABLE-7. Cell min, max and hardMin {#table-7}

For each cell, with `bL`, `bR` = the widths in twips (rounded) of its left and right borders after conflict resolution ([TABLE-36](#table-36)), `mL`, `mR` its left and right margins in twips, and `s` the table's `w:tblCellSpacing` in twips (0 if none):

```
gap(b, m) = s > 0 ? s + b + m : m
pad       = max(gap(bL, mL) + gap(bR, mR),  (bL + bR + 1) div 2)
cellPad   = max(0, pad − 2s)
```

So without cell spacing `pad` is the margins, unless half the borders are wider; with cell spacing the spacing cancels and `cellPad = mL + mR + bL + bR` (the spacing is added back in [TABLE-17](#table-17) and [TABLE-21](#table-21)).

| Margins | Borders | cellPad |
|---|---|---|
| 108 + 108 | 0.5pt | 216 |
| 0 + 0 | 3pt | 60 |
| 20 + 20 | 1.5pt | 40 |
| 0 + 0 | 0.5pt | 10 |

Content widths, from the cell's blocks in order:

- **Paragraph, contentMin**: split the paragraph into unbreakable pieces at its break opportunities ([BREAK-3](break#break-3)): a space, a line, page or column break ends a piece, and so does each break opportunity after a character (as after a hyphen, but see [TABLE-9](#table-9)). A piece also ends **before** a tab: the tab belongs to the text after it and is as wide as it is in the unwrapped lines of contentMax (a tab to a stop at 2714 after text 452 wide makes a piece of 2262 for the tab alone, plus whatever follows it up to the next break). A list number's suffix tab is not such a tab. Each piece's width is the sum of its shaped advances ([FONT-5](font#font-5)) plus the indents of [TABLE-8](#table-8); round it **up** to whole twips. contentMin is the widest.
- **Paragraph, contentMax**: lay the paragraph out at unlimited width (its lines then end only at forced breaks). For each line take its width as laid out, up to the end of its last piece that isn't a space (trailing spaces don't count), plus the indents of [TABLE-8](#table-8); round up. A centred or right-aligned line is taken from its line start, not from where alignment would put it. contentMax is the widest line.
- **Nested table**: [TABLE-11](#table-11).
- The cell's contentMin, contentMax and contentHard are the maximum over its blocks (a paragraph's contentHard is 0). If contentMin and contentMax are both 0, the cell is empty: both become 6.

Then (each capped at MAXW):

```
cell.min     = contentMin + cellPad
cell.max     = max(cell.min, contentMax + cellPad)
cell.hardMin = contentHard + cellPad
```

A vertical-merge continuation cell (`w:vMerge` without `restart`) has no content (0). A cell with vertical text: [TABLE-33](#table-33). A fixed-layout table skips all this: [TABLE-5](#table-5) (min = max = `cellPad + 20`, hardMin = `cellPad`).

### TABLE-8. Paragraph indents in min and max {#table-8}

The paragraph's indents (`left`, `right`, `firstLine`, negative when hanging) enter the content widths of [TABLE-7](#table-7) like this:

| | compat ≤ 14 | compat 15 |
|---|---|---|
| each piece (min) | `+ left + right + min(firstLine, 0)` | `+ left + right` |
| each line (max) | `(lineEnd − lineStart) + left + right + min(firstLine, 0)` | `lineEnd + right`, with `lineEnd` taken from the paragraph's left edge, so it includes `left + firstLine` (first line) or `left` (other lines) |

A numbered paragraph (one with a list number, [LIST-1](list#list-1)) differs:

- compat ≤ 14: the first line is laid out starting at `max(left + firstLine, 0)` (so its suffix tab goes to the stop it reaches from there), and its width for the max is taken from the paragraph's left edge, not from the line start. The number, its suffix and the text's first piece are **one** piece for the min, from the left edge to the end of that piece; the others are pieces as usual, and in them a tab only ends a piece. Both widths then get `left + right + min(firstLine, 0)` as in the table.
- compat 15: the number and its suffix aren't pieces of their own; the text's first piece starts at `left` (whatever the number and suffix cover) and ends where it ends in the unwrapped line.

Example, compat 12: left 720, hanging 360, "1." with a tab suffix, then "786594": the number sits at 360, the tab goes to 720, the text ends at 1390, so min = max = `1390 + 720 − 360 = 1750`. With left 60, hanging 180 and a Symbol bullet: the line starts at 0, the bullet ends past the left indent, so the tab goes to the default stop 720 and the piece is `720 + 670 − 120 = 1270`. The same paragraph in compat 15: `670 + 60 = 730`.

In compat 15, a paragraph whose last line starts left of 0 (its start includes `firstLine` on the first line): that line's max is the larger of the usual one and its full end plus `right`, the full end taking in its trailing spaces and the mark glyph that ends it. The mark is the end-of-cell mark `¤` (U+00A4) in the cell's last paragraph and the pilcrow `¶` otherwise, in the paragraph mark's font and size (Calibri 11pt: 109.6 twips).

Examples, compat 15: `firstLine` 360 adds 360 to the max and nothing to the min. A hanging 360 under `left` 720 adds 720 to the min, and the first line's max is only `360 + text + right`, so the max may be no wider than the min.

### TABLE-9. Hyphen runs in the minimum {#table-9}

The break opportunities after hyphens are used for contentMin ([TABLE-7](#table-7)) except in one case, in compat ≤ 14 only: a break after a piece that begins with a hyphen is not taken when everything after it, up to the paragraph's end, is hyphens and spaces. So a run of hyphens that ends the paragraph stays in one piece together with the letters joined to it before it; a run followed by more text still breaks after each hyphen. Compat 15 breaks after every hyphen. Line breaking in the cell itself ([BREAK-3](break#break-3)) is unaffected.

Examples (compat ≤ 14): `aaaa----` at a paragraph's end is one piece; in `----bbbb` the widest piece is `bbbb`.

### TABLE-10. noWrap doesn't change the widths {#table-10}

`w:noWrap` on a cell changes nothing in its min, max or hardMin: compute them as for any cell.

### TABLE-11. A nested table's min and max {#table-11}

For a table inside a cell, run the whole algorithm on the nested table with `A = MAXW` ([TABLE-17](#table-17)). It gives the nested table's `minT`, its table width `Wt` and `hardT` ([TABLE-19](#table-19)). With `eL`, `eR` = the nested table's own edge allowances ([TABLE-17](#table-17)) and `ind` its `w:tblInd`, all in twips:

```
e           = eL + eR − ind
contentMin  = minT − e
contentMax  = Wt − e
contentHard = hardT − e
```

A nested table is never sloppy, so `eL` and `eR` are the compat 15 allowances (negative half borders) in every mode: the cell needs the nested table's widths plus half its outer borders plus its indent, in compat ≤ 14 too.

### TABLE-12. Cell width types {#table-12}

Each cell gets a type and a preference `pref` from its `w:tcW`:

| `w:tcW` | Autofit | Fixed |
|---|---|---|
| absent, or `type="auto"` | auto | dxa, `pref = grid − s2` |
| `type="dxa"` (or no type), `w` | dxa, `pref = w`, *written* | the same |
| `type="pct"`, `w` | pct, `pref = w` in fiftieths of a percent (5000 = 100%; `"50%"` = 2500) | the same |
| `type="nil"` | dxa, `pref = grid − s2` | the same |

`grid` = the sum of the `w:tblGrid` widths over the cell's columns; `s2` = 2 × cell spacing, and a cell on the table's left or right edge takes `s2 div 2` more off for each such side, so the resolved columns give the grid back ([TABLE-21](#table-21)).

Example: grid 1000 / 3000 / 2000, a cell "x" in each with `type="nil"`: the columns are 50, 150 and 100pt. With no `w:tcW`, an "x" cell is about 15.6pt wide (its content plus 216).

### TABLE-13. Single-span cells into columns {#table-13}

Each grid column holds `min`, `max`, `hardMin`, a type (initially auto), a dxa preference and a pct, all 0 at first. Take the cells with `gridSpan = 1` row by row, left to right. Skip vertical-merge continuation cells, except in the table's first row. For each cell, in its column:

1. The cell's own maximum: dxa cell `cellMax = max(cell.min, pref)` (fixed layout: `cellMax = pref`); other cells `cellMax = cell.max`. A dxa cell's content max is ignored.
2. Type and preference:
   - pct cell: `column.pct = (column is pct) ? max(column.pct, pref) : pref`; the column becomes pct. A pct column stays pct.
   - dxa cell, column not pct: `column.dxa = (column is dxa) ? max(column.dxa, pref) : pref`; the column becomes dxa.
   - auto cell: no change.
3. Widths:
   ```
   column.max     = max(column.max, cell.min, cellMax)     // fixed layout: max(column.max, cellMax)
   column.min     = max(column.min, cell.min)
   column.hardMin = max(column.hardMin, cell.hardMin)
   ```

In fixed layout a cell's minimum (`cellPad + 20`) doesn't raise its column's max here; that happens only at the end of [TABLE-16](#table-16), after the spanning cells. (Two columns with dxa cells of 15 and 1 twips under an 864-twip dxa span: the last column gets 1 + 848 = 849 from the span, and only then do both columns rise to their minimum of 20. Raising them first would give the last column 844.)

So a dxa column's max is the largest of its min, its dxa preferences and the content max of any auto cells in it: a dxa cell never lowers a column's max.

### TABLE-14. Spanning cells: order {#table-14}

After all single-span cells of all rows and the skipped regions ([TABLE-15](#table-15)), take the cells with `gridSpan ≥ 2` (not vertical-merge continuations) in row order, and left to right within a row. Each goes through [TABLE-16](#table-16)'s proportional spread, dxa cells too: there is no rule that gives a dxa span's extra width to its last column. A span that starts in the last grid column is added to it as a single-span cell ([TABLE-13](#table-13)).

Example: a dxa span of 4050 over two columns that later rows give 1005 and 3045 spreads over both in proportion to their min + max; it doesn't leave the first at 1005 and put the rest in the second.

### TABLE-15. Skipped grid columns {#table-15}

A row's skipped regions are `w:gridBefore` (with `w:wBefore`) and `w:gridAfter` (with `w:wAfter`); the grid columns a row doesn't reach at its end count as a gridAfter region too. Each is a dxa segment that ends in the region's last column and spans its columns:

- with a dxa width `w`: that width (0 or another type: no segment);
- with no width element: the sum of its `w:tblGrid` widths.

A region counts only inside the extent of the table's non-auto cells (on the grid): a before region only if some non-auto cell starts left of the region's end, an after region only if some non-auto cell ends right of its start.

**Helper row.** The cells with a dxa `pref > 0` ([TABLE-12](#table-12), not vertical-merge continuations below the first row) are segments as well: ending in their last column, spanning their columns. Keep, per (end column, span), the widest. A column's type is the type of the first single-span segment or cell ending there (regions first, then the cells in row order), else dxa when only spans end there. Then, for columns `g = 0 … n−1` in order:

```
helper[g] = the widest one-column segment ending at g (0 if none)
for each segment ending at g spanning k ≥ 2 columns:
    v = width − Σ helper[g−k+1 … g−1]     (0 if any of those columns has another type than g)
    if v ≠ 0 and g is dxa: v −= (k−1)·s2
    helper[g] = max(helper[g], v)
```

Only if some spanning segment raised a column this way, every dxa column with `helper[g] > 0` gets a single-span dxa cell ([TABLE-13](#table-13)) with `min = 5`, `max = pref = helper[g]`. Columns of other types get nothing.

Then, whatever the helper row did, the last one-column region before the first column (in row order) and the last one-column region after the last column are added the same way to the first and the last column.

Examples (grid 1000 / 1000 / 1000; row 1 `gridBefore 1, wBefore 800` with one 2000 cell; row 2 `gridBefore 2, wBefore 2000` with one auto cell; row 3 three dxa 300 cells): row 1 doesn't reach column 3, an after region of 1000, so column 3 is 1000 wide even though its text needs less. A 6000 dxa span over auto columns raises no dxa column and is spread proportionally ([TABLE-16](#table-16)).

### TABLE-16. The proportional spread {#table-16}

For one spanning cell over `span` columns, with `sp` = 2 × cell spacing and `k = (span − 1)·sp`:

```
sumMin = Σ column.min, sumMax = Σ column.max      (over the span columns)
if sumMax == 0: sumMax = cell.max − k
if sumMin == 0: sumMin = cell.min − k
cMin = cell.min − k
cMax = (dxa ? pref : cell.max) − k
if sumMax < cMin < sumMax + 5: cMin = sumMax       (same for cMax)
cMax = max(cMax, cMin)
```

**pct cell with pref > 0:** `rem = min(pref, 5000) − Σ pct of the span columns that are pct`. If `rem > 0`, each span column that isn't pct gets `pct = column.max · rem div Σ(max of those columns)` (or `rem div count` if that sum is 0). Then every span column becomes pct.

**Min**, only if `cMin > sumMin`. Let `left = cMin` and `rest` = the span columns.

1. If any of them is pct: `pctTot = 5000` if any span column isn't pct, else `min(Σ pct, 5000)`; `pctLeft = pctTot`. For each pct column in order: `p = min(pct, pctLeft)`, `w = cMin · p div pctTot`; remove the column's min and max from sumMin and sumMax; `column.min = max(column.min, w)`; `pctLeft −= p`; `left −= w`; drop it from `rest`.
2. If `rest` is not empty and `cMin > sumMax` (the cell needs more than the columns' max): each column `w = rdiv(left, column.min + column.max, sumMin + sumMax)` (or `rdiv(left, 1, count)` if that sum is 0), `column.min = max(column.min, w)` (set to its share, not added). Then `leftover = left − Σw` (never below 0): add 1 to each column from the first until it is used up (one pass).
3. Else (`sumMin < cMin ≤ sumMax`): `extra = left − sumMin`.
   - Weights `r = (column.min + column.max) / (sumMin + sumMax)`, in floating point.
   - Cap: a column where `column.min + extra·r > column.max` takes `r = (column.max − column.min) / extra`; collect the weight it lost and give it once to the uncapped columns, `r += lost·r / Σr(uncapped)`.
   - Each column in order: `d = min(trunc(extra · r), column.max − column.min)` with the **running** extra; `column.min += d`; `extra −= d`.
   - While `extra > 0`: one whole pass, adding 1 to every span column with `min < max` (and taking 1 from extra for each), even if extra goes negative in the pass; stop if a pass changes nothing.

**Max:** set `column.max = max(column.max, column.min)` for each span column, `s = Σ column.max`. If `cMax > s`: `column.max = rdiv(column.max, cMax, s)` for each (or `rdiv(cMax, 1, span)` if `s = 0`), then add 1 to each column from the first until `cMax` is reached (one pass).

When all spans are done, set `column.max = max(column.min, column.max)` for every column.

Examples:
- Two auto columns (min 100, max 200) under a span with min = max = 1000: `cMin > sumMax`, each gets `rdiv(1000, 300, 600) = 500`; min 500 and max 500 each.
- Columns (min 100, max 500) and (100, 200), span min 400: extra 200, weights 2/3 and 1/3; `d = 133` (extra 67), then `d = 22` (extra 45); 23 passes of +1 follow, the last overshooting by 1: mins 256 and 145.

### TABLE-17. Table-wide inputs {#table-17}

```
s2       = round(2 × w:tblCellSpacing in twips)    (0 if none)
padTotal = s2 × (n + 1)
A        = round(textW − tblInd + eL + eR)          (twips)
```

The **edge allowances** `eL`, `eR` come from the first row: its first cell's left border and left margin, its last cell's right border and right margin, each border being that side's candidate ([TABLE-36](#table-36)): the cell's own if the cell writes that side (a `nil` or `none` one counts as 0), else the table's. A border's width in horizontal geometry (here and in [TABLE-23](#table-23), [TABLE-24](#table-24), [TABLE-39](#table-39)) is its layout width: `w:space` plus its line, as [TABLE-37](#table-37) gives it. With `halfBorder = (borderTwips + 1) div 2` (rounded up):

- compat 15: `e = −halfBorder`;
- compat ≤ 14: `e = +max(halfBorder, cell margin)`; with cell spacing `s`, `e = +max(halfBorder, 2s + border + cell margin)` ([TABLE-39](#table-39)).

Example: compat 15, 0.5pt borders, 6.5in of text: `A = 9360 − 5 − 5 = 9350`. Compat 12, 108-twip margins: `A = 9360 + 108 + 108 = 9576`.

Preferred table width `tblW` from `w:tblW`: auto or nil → 0 (no preference); dxa `w` → `w` (not limited to A); pct → `rdiv(A, pct, 5000)`. Then `tblW = min(tblW, MAXW − tblInd)`.

`noGrow` = true unless `w:growAutofit` is set in the compat settings or `tblW > A`.

### TABLE-18. Column types in the resolver {#table-18}

The resolver uses each column's max whatever its type (a dxa column's max already holds its preference, [TABLE-13](#table-13)); a pct column also brings its pct. Before resolving, sum per type:

- all columns: `sumMin`, `sumMax`, `sumHard`;
- pct columns, in order: `p = min(pct, 5000)`; if the running total plus `p` would pass 5000, the column's pct becomes `5000 − total` and the total 5000, else the total grows by `p` → `sumPct`; `minPct = Σ min`; `ratio` = the largest `max / p` (as an exact fraction) among pct columns with `p > 0`;
- dxa columns: `maxDxa = Σ max`, `minDxa = Σ min`;
- auto columns: `maxAuto = Σ max`, `minAuto = Σ min`.

```
remPct = max(0, 5000 − sumPct)
minT   = sumMin + padTotal
hardT  = sumHard + padTotal
maxT   = max(sumMax + padTotal, minT)
```

### TABLE-19. Resolving the widths {#table-19}

**1. Table width `Wt`.**

```
if tblW > 0:
    clampT = min(maxT, max(minT, tblW))
    Wt     = max(tblW, minT)              // a preferred width below the minimums gives way to them
else:
    clampT = maxT
    if sumPct == 0:
        Wt = max(minT, min(maxT, A))
    else:
        Wt = A
        unless (remPct == 0 and maxAuto + maxDxa != 0):
            if remPct != 0 and ratio < (maxAuto + maxDxa) / remPct:  ratio = (maxAuto + maxDxa) / remPct
            need = rdiv(5000, ratio.numerator, ratio.denominator)
            if need + padTotal < A:  Wt = need + padTotal
        Wt = max(Wt, minT)
fixed layout:  Wt = max(clampT, Wt)
```

**2. Even the minimums don't fit.** If the table is not fixed, `noGrow` holds, `Wt == minT` and `A < Wt`:

- if `A ≤ hardT` or `minT ≤ hardT`: every column = its hardMin; table width `hardT`; stop;
- else `width = hardMin + rdiv(A − hardT, min − hardMin, minT − hardT)` per column, then [TABLE-20](#table-20) with floor = hardMin and target `A − padTotal`; table width `A`; stop.

Pieces wider than their column then break inside ([BREAK-4](break#break-4)). With `w:growAutofit` (or `tblW > A`) this step is skipped, so the table keeps its minimums and runs past the margin.

**3. Split the content width between the types.** `W = Wt − padTotal` (or `Wt` if that is below `padTotal`).

```
nonPct = (maxAuto + maxDxa == 0) ? 0 : min( max(rdiv(remPct, W, 5000), minDxa + minAuto),  W − minPct )
if maxDxa != 0:
    dxa = min(maxDxa, nonPct)                    // dxa columns are served first
    if maxAuto != 0:
        if dxa + minAuto ≤ nonPct:  auto = nonPct − dxa
        else:                       auto = minAuto
                                    dxa  = (minDxa + minAuto ≤ nonPct) ? nonPct − minAuto : minDxa
    else:
        auto = 0;  if dxa < nonPct: dxa = nonPct  // a table of dxa columns only grows
else:
    dxa = 0;  auto = (maxAuto != 0) ? max(minAuto, nonPct) : 0
pctRem = (W != 0) ? rdiv(5000, W − dxa − auto, W) : 0
```

**4. Each column.** For a pct column: `t = (sumPct != 0) ? rdiv(W, rdiv(pctRem, pct, sumPct), 5000) : 0`; width `= min(max(t, min), MAXW)`. For a dxa or auto column, with `X` = `dxa` or `auto` from step 3 and `Smin`, `Smax` that type's sums:

| Case | width |
|---|---|
| `X > Smax` | `max + rdiv(max, X − Smax, Smax)` (grow in proportion to max) |
| `X == Smax` | `max` |
| `X == Smin` | `min` |
| `Smin < X < Smax` | `max − rdiv(max − min, Smax − X, Smax − Smin)` (shrink in proportion to max − min) |
| otherwise | `min + rdiv(max, X − Smin, Smax)`, or 0 if `Smax = 0` |

Then [TABLE-20](#table-20) with floor = min and target `W`. The table width is `W + padTotal`.

In words: with no preferred width the table is its content max, limited to A but never below its content min; dxa columns get up to their preferences first and auto columns the rest; when space is short auto columns shrink first, in proportion to (max − min), and dxa columns only once the auto columns are at their mins; extra space grows auto columns (or, without auto columns, dxa columns) in proportion to their max; pct columns take their share of what's left.

### TABLE-20. Rounding leftovers {#table-20}

Given widths `out`, floors and a target:

```
s = Σ out;  if s == target: done
cnt = number of columns with out > floor
repeat at most 10 times:
    diff = target − s;  if diff > 0: cnt = n          // when adding, every column takes part
    if cnt == 0: stop
    q = diff / cnt (toward 0);  r = diff − q·cnt;  step = sign(r);  carry = 0
    for each column in order, if out > floor or diff > 0:
        carry += q + step;  out += carry
        if out < floor: carry = out − floor; out = floor      // the deficit moves to the next column
        else carry = 0
        r −= step;  if r == 0: step = 0                       // the first |r| columns get the ±1
    s = Σ out;  cnt = number of columns not clamped;  if s == target: stop
```

Leftovers go to the **first** columns. Example: three dxa columns of 3120 (min 300) in A = 9350 shrink to 3117 each (sum 9351); the extra twip comes off the first: 3116, 3117, 3117.

### TABLE-21. Cell spacing and edges {#table-21}

With `s2 > 0`, add the spacing back to the resolved widths: each column `+= min(s2, MAXW − width)`, then the first and the last column each `+= s2 div 2`. The column edges are exact running sums of the widths from the table's left border centre ([TABLE-23](#table-23)), converted to points (÷ 20).

Example, s = 60 twips, two columns: each column = content + both margins + both full borders, + 120 each, + 60 more at each outer column.

### TABLE-22. Worked grid examples {#table-22}

Grid 2000 / 2000 unless stated, compat 15, A = 9350; cell contents short.

| Rows' `w:tcW` | Autofit | Fixed |
|---|---|---|
| 3000, 1000 / 1000, 3000 | 3000, 3000 | 3000, 3000 |
| 1000, 1500 in every row | 1000, 1500 (narrower than the grid) | 1000, 1500 |
| 2000, 2000 / 3500, 500 | 3500, 2000 | 3500, 2000 |
| 3000, 1000 / auto, auto | 3000, 1000 | 3000, 2000 (the auto cell takes its grid 2000) |
| span 2 = 6000 over 1000, 3000, then 1000 | 1000, 5000, 1000 | 1000, 5000, 1000 |
| span 3 = 9000 over 1000, 2000, 3000 | 1000, 2000, 6000 | 1000, 2000, 6000 |

## Geometry

### TABLE-23. Horizontal placement {#table-23}

Place the table's left border centre `x0`, from the left edge of the text column (or of the cell's content box):

- `lw` = the width of the first row's first cell's left border candidate, as in [TABLE-17](#table-17): the cell's own if the cell writes it (`nil` or `none` counts as 0), else the table's left border. `lm` = that cell's left margin.
- compat 15: `x0 = tblInd + lw/2` (the outer border edge sits on the margin).
- compat ≤ 14: `x0 = tblInd − e`, with `e` the left edge allowance of [TABLE-17](#table-17) (`max(lm, halfBorder(lw))`, or with cell spacing `max(halfBorder(lw), 2s + lw + lm)`), with `halfBorder` as in [TABLE-17](#table-17), in points (so the cell text lines up with the margin). A table whose `w:tblInd` is set neither directly nor by its table style's `w:basedOn` chain isn't pulled left: `x0 = 0`, so its text starts `lm` inside the column. A table without `w:tblStyle` (or naming a style that doesn't exist) takes the default table style, whose indent counts as set even when that style writes no `w:tblInd`; only a table naming an existing style can be left unpulled.
- The alignment is the table's `w:jc`; in compat 15 a table that isn't floating takes its first row's `w:trPr/w:jc` instead when that row has one, for every row.
- `w:jc="center"`: `x0 = (textW − Σ columns)/2` (the indent is ignored).
- `w:jc="right"` (or `end`): `x0 = textW − Σ columns − (table right border width)/2`.
- compat ≤ 14, a table that isn't floating: each row is aligned on its own, by its `w:trPr/w:jc`, else the table's. Its width is its own cells' extent (without its `w:gridBefore` and `w:gridAfter` columns) when the settings have `w:alignTablesRowByRow`, else the whole grid's. A centred row's left edge is at `(textW − width)/2`; a right-aligned row's at `textW − width + gL`, with `gL` its first cell's left margin (with cell spacing, plus `2s` and the table's left border). A left-aligned row stays at `x0`; in a centred or right-aligned table it is at `−e` (the indent is ignored).

Each cell then spans from `x0 + edge[col]` to `x0 + edge[col + span]` ([TABLE-21](#table-21)). A row with its own indent: [TABLE-50](#table-50).

Example: compat 15, 0.5pt borders: `x0 = 0.25pt`. Compat 12, 5.4pt margins, 0.5pt borders: `x0 = −5.4pt`.

**Right-to-left tables** (`w:bidiVisual`, not floating). The grid is resolved in logical order (column 0 first) and drawn mirrored: column 0 is on the right. The table's `left`/`right` borders are logical (left = start, drawn on the right), and so are cell borders and cell spacing; cell margins are visual (`w:left` is on each cell's left as drawn, in the content box of [TABLE-24](#table-24) and [TABLE-39](#table-39), whose border and spacing sides swap). The edge allowances of [TABLE-17](#table-17) take the first row's last cell for the left side and its first cell for the right, each against the table's own left and right border. In twips, with `width = Σ columns`, `bS` the first cell's left border against the table's left (the start border), `bE` the last cell's right border against the table's right, `bLL` the last cell's left border against the inside vertical border (the table's left when it is the row's only cell), `gS` the first cell's left margin (with cell spacing plus `s`, or `2s` when it is the row's only cell, and its resolved right border), `gE` likewise for the last cell, and `x0` here the table's left edge as drawn:

- `w:jc` absent, `left`, `start` or `both`: right-aligned, the indent counted from the right. Compat ≤ 14: `x0 = textW − tblInd + max(halfBorder(bS), gS) − width`; compat 15: `x0 = textW − tblInd − (bS + bLL) div 2 − width`.
- `w:jc="right"` or `end`: left-aligned, the indent ignored. Compat ≤ 14: `x0 = −max(halfBorder(bE), gE)`; compat 15: `x0 = 0`.
- `w:jc="center"`: compat ≤ 14: `x0 = (textW − width) div 2`; compat 15: `x0 = (textW − width − (bS + bLL) div 2) div 2`.

Every row takes the table's placement: the compat ≤ 14 row alignment above and a row's own indent ([TABLE-50](#table-50)) don't apply. A nested right-to-left table takes the compat 15 placement in every mode ([TABLE-25](#table-25)). A floating table with `w:bidiVisual` is placed and drawn left to right.

### TABLE-24. The cell's content box {#table-24}

Borders take no horizontal space: column widths don't change with border width. With no cell spacing, a cell of width `w` (border centre to border centre) has its content box at

```
left  = cellX + max(marginL, bL/2)
width = max(0, w − max(marginL, bL/2) − max(marginR, bR/2))
```

where `bL`, `bR` are its resolved left and right borders ([TABLE-36](#table-36)), an art border counting as a single line of its `w:sz` ([TABLE-37](#table-37)). Lay the cell's content out at that width. With cell spacing see [TABLE-39](#table-39).

### TABLE-25. Nested table placement {#table-25}

A table inside a cell is placed as compat 15 places tables ([TABLE-23](#table-23)), even in compat ≤ 14: its outer border edge sits at the cell content box's left plus `tblInd`. In compat ≤ 14, compute its position by the compat ≤ 14 rule, then move it right by

```
shift = lw′/2 + max(lm, halfBorder(lw′))
```

where `lw′` is the first cell's **own** left border width if the cell has a left border element (a `nil` or `none` one counts as 0), else the table's left border, and `lm` the first cell's left margin. Centred and right-aligned nested tables aren't moved.

Example: compat 12, a nested table whose cells have a `nil` left border and whose table border is single 0.5pt: `lw′ = 0`, so shift = the cell margin.

In every compat mode a nested table's negative `tblInd` counts as 0: `tblInd = -720` puts it where `0` does. A positive one is used as it is.

### TABLE-26. The paragraph after a nested table {#table-26}

A cell must end in a paragraph, so a nested table at the end of a cell is followed by one. If that closing paragraph is empty (no content), it takes **no height**: lay the cell out without it. The outer cell's height ends at the nested table's bottom border.

## Row heights

### TABLE-27. Row pitch and borders {#table-27}

Vertically, borders take space:

```
topBorder   = the widest resolved top edge among the row's cells (TABLE-36), by its h() (TABLE-37)
above       = max(W, max over the row's cells of (h(top(c)) + topMargin(c)))
                  top(c) = the cell's own top border, else the table's top (row 0) or insideH, before conflicts
                  W      = the previous row's widest own-else-insideH bottom border, h() as in TABLE-37 (0 on row 0)
contentH    = max over the row's cells that are not vertically merged of (above − topBorder + cell content height)
              + below
              but at least the largest (topMargin + bottomMargin) of any cell in the row
below       = the largest bottomMargin of any cell in the row: one bottom margin for the whole row
rowPitch    = topBorder + contentH                       (auto height; TABLE-28)
firstLineTop = rowTop + above (+ the vAlign offset, TABLE-30), the same for every cell of the row
tableBottom = last row's bottom + the widest resolved bottom edge among the last row's cells, by its h()
```

Example: a 3pt bottom border over a 0.5pt insideH and 2pt top margins gives `above = max(3, 2.5) = 3`.

Example: a three-line cell with no bottom margin beside a one-line cell with a 10pt bottom margin: the row is the three lines plus 10pt.

Cells in a vertical merge (restart or continuation) don't count in `contentH`: see [TABLE-31](#table-31). [TABLE-38](#table-38) can widen `topBorder`, and cell spacing adds to it ([TABLE-39](#table-39)).

Example: one 13.44pt line, top and bottom margins 0: a 0.5pt top border gives a pitch of 13.94pt, a 1.5pt one 14.94pt.

### TABLE-28. Row height rules {#table-28}

`w:trHeight w:val="X"` with `w:hRule` (default `atLeast`):

- no `w:trHeight`, or `hRule="auto"`: `rowPitch = topBorder + contentH`;
- `atLeast`: `rowPitch = topBorder + max(contentH, X)`;
- `exact` with X > 0: `rowPitch = X`, the top border **included**; content that doesn't fit is clipped ([TABLE-34](#table-34)). An exact row of 0 is an auto row.

In compat ≤ 14 a body row's exact height is capped to the whole column (the column's full height, not the space left) less the row's top border and the table's bottom border, so an exact row never asks for more than one column; its content is clipped at the capped height. Compat 15 keeps the height: an exact row taller than the column runs past the column bottom.

### TABLE-29. Cell content {#table-29}

Lay out a cell's blocks top-down at its content width with the normal paragraph rules. Its content height includes its first paragraph's space-before and its last paragraph's space-after, except where contextual spacing drops them against the neighbouring cells or the row's end mark ([PARA-4](para#para-4)). A paragraph's space-after before a nested table applies at that table's top border edge. Floats anchored in a cell: see [FLOAT-14](float#float-14).

### TABLE-30. Vertical alignment {#table-30}

With `free = rowPitch − topBorder − topMargin − bottomMargin − contentHeight` (over a merged region, `rowPitch` is the region's total, [TABLE-32](#table-32)), offset the cell's content by `w:vAlign`: `top` (default) 0, `center` `max(0, free/2)`, `bottom` `max(0, free)`.

A cell that anchors a float laid out in it (`layoutInCell="1"`, [FLOAT-14](float#float-14)), any wrap, picture or text box:

- with `w:compat/w:doNotVertAlignCellWithSp` in compat ≤ 14, its offset is 0, for `center` and `bottom` alike (compat 15 ignores the flag);
- otherwise the float counts in `contentHeight`: `contentHeight = max(contentHeight, bottom of each float in the cell − cell content top)`, the float where a top-aligned cell puts it (for a `paragraph`-relative one, anchor paragraph top + `posOffset` + its height). It makes the row taller only as [FLOAT-14](float#float-14) says (a wrapping `paragraph`- or `line`-relative float).

A compat ≤ 14 float that [FLOAT-14](float#float-14) places outside the cell (`layoutInCell="0"`, not `character`- or `line`-relative) doesn't count; an inline picture counts only as its line.

Example: a 60pt row, a 13.43pt line and a 20pt float 10pt below the paragraph top, without the flag: a `center` cell's offset is `(60 − 30)/2 = 15`, a `bottom` cell's 30.

`contentHeight` of a whole cell is its content's height as the row's height counts it ([TABLE-27](#table-27)), its last paragraph's space-after included. In the part of a split row on a page ([TABLE-41](#table-41)) it is the height of the lines that part holds.

### TABLE-31. Vertically merged cells: heights {#table-31}

A merged region is a cell with `w:vMerge w:val="restart"` and, in the rows below it, the continuation cells (`w:vMerge` without `restart`) at the same grid column.

1. Lay out every row as in [TABLE-27](#table-27); the region's cells don't count in their rows' `contentH`.
2. Then, for each region from the top: `need = topBorder(first row) + topMargin + restart cell's content + bottomMargin`, `have` = the sum of the region's row pitches. If `need > have` and the region's last row isn't exact, add `need − have` to the **last** row's pitch.
3. The last row's own cells keep their text at the top of that row; the extra height goes below it.

### TABLE-32. Vertically merged cells: drawing and pages {#table-32}

- The restart cell's content is its region's content: it flows from the region's top through all its rows. Continuation cells' own content is not laid out.
- `w:vAlign` ([TABLE-30](#table-30)), shading and the side borders act over the whole region (from the restart row's top to the region's bottom).
- There is no border inside the region: continuation cells draw no top border. The border between the region's rows still takes its row's top-border space ([TABLE-27](#table-27)), and the region's bottom border is the last row's.
- If the restart cell's row is exact, the whole region clips at the region's bottom ([TABLE-34](#table-34)).
- Across pages the region's rows split and move like any rows; its content isn't repeated on the next page, it continues there (below).
- When the region's last row splits ([TABLE-41](#table-41)) and its first row is on the same page, the region's content splits with it: it keeps the lines that fit above the room's bottom, tested as a cell's ([TABLE-44](#table-44)), from the region's content top. The rest continues in the region's column of that row's continuation, at its top like any continued cell. The part on this page is as tall as the taller of the row's own kept content and the region's.
- When a page ends between whole rows inside a top-aligned region whose first row is on that page, the region keeps the lines that fit down to the last whole row's bottom (tested as a cell's, from the region's content top), and the rest continues at the top of the region's column in the next page's first row. Every region of a row is cut this way at once.

Example: a two-row region of 14.52pt lines beside a last row holding "(initial)", near the body bottom: three lines stay, with "(initial)" beside the third. The fourth line's text would end 1.8pt above the body bottom but its pitch ends 0.5pt past it, so it starts the next page's continuation with the rest.

### TABLE-33. Vertical text {#table-33}

A cell with `w:textDirection` `btLr`, `tbRl`, `tbRlV` or `tbLrV` is as thick as its lines for autofit: `contentMin = contentMax = 6 + Σ paragraphs (spaceBefore + spaceAfter) + Σ lines ⌈h × 1440 / 294912⌉` twips, hardMin 0, before the padding of [TABLE-7](#table-7). `h` is the line's pitch in layout units plus `em/32` (`em` the line's largest font size, rounded half up to a layout unit): a line multiple `m` scales that too (`(em/32 × w:line + 120) div 240`), an exact line adds none. One line of Calibri 11 is 282 twips. Its content is not laid out and adds nothing to its row's height: an auto row stays as tall as its other cells need.

### TABLE-34. Exact rows clip per glyph {#table-34}

In an exact row ([TABLE-28](#table-28)), the clip line is the row's bottom less the cells' bottom margin, which the row adds below its exact height (for a merged region whose first row is exact, the region's bottom less that margin). For each line of a cell:

1. Find the line's highest ink top (the top of its glyphs' ink, not the line box). If it is at or below `clipLine + 0.06pt` (about one twip of slack), hide the whole line. A line with no ink (no glyphs) hides if its top is at or below the clip line.
2. In a line that shows, drop each glyph whose own ink top is at or below `clipLine + 0.06pt`. A glyph whose ink reaches above it is drawn whole.

Example: a 20pt exact row with 3pt top and 6pt bottom margins is 26pt tall and clips 20pt below its top; a second 13.43pt line, whose ink top is 20.9pt down, is hidden.

So text can show in a row a little shorter than its line: capitals whose ink top is 17.03pt down show in a 17.0pt row. A second line starting 2.1pt above the bottom of a 14.4pt exact row shows only the glyphs whose ink reaches above that bottom.

### TABLE-35. Clipping at the cell box {#table-35}

Cell content is laid out as written, even when a negative indent makes it run past the cell. Then clip each line to the cell's box, from its left border centre to its right border centre, whatever the margins and whatever the next cell holds:

- a glyph whose ink overlaps the box is drawn (whole); one whose ink lies wholly outside is dropped (a glyph without ink is kept if its origin is inside);
- rectangles drawn with the text (underlines, highlights) are cut at the box edges;
- a nested table isn't clipped to the cell that holds it: one wider than its cell is drawn in full, over the neighbouring cells, each of its lines clipped only to its own cell. An exact outer row still clips it at its bottom ([TABLE-34](table#table-34)).

Example: in "…FFFF" running past the edge, the last F whose ink still touches the box is drawn; in "…F02-1", a final "1" whose ink starts past the edge is not.

## Borders and shading

### TABLE-36. Border conflicts {#table-36}

Each cell side has a candidate border: the cell's own (`w:tcBorders`) if the cell writes that side (even as `nil`), else the table-level one (`w:tblBorders`): `top`/`left`/`bottom`/`right` on the table's outer sides (the first row's top, the last row's bottom, a cell starting at grid column 0, a cell reaching the last grid column), `insideH`/`insideV` elsewhere.

Where two cells share an edge, compare this cell's candidate with the neighbour's facing candidate: across a horizontal edge, the cell above (or below) covering this cell's first grid column, whose facing side's table-level border is `insideH`; across a vertical edge, the previous (or next) cell in the row, with `insideV`. The first and last cells' outer sides, the first row's top and the last row's bottom have no neighbour. Exactly one border wins:

1. A border that isn't real (`nil`, `none`, or width 0) loses to a real one; two unreal ones give no border.
2. A cell-level border beats a table-level one.
3. Higher weight wins, `weight = width × factor`: double 3, triple 3.5, dashed, dotted, dashSmallGap, dotDash and dotDotDash 0.75, every other style 1.
4. Same weight: the style later in this order wins: nil, none, single, thick, double, dotted, dashed, dotDash, dotDotDash, triple, thinThickSmallGap, thickThinSmallGap, thinThickThinSmallGap, thinThickMediumGap, thickThinMediumGap, thinThickThinMediumGap, thinThickLargeGap, thickThinLargeGap, thinThickThinLargeGap, wave, doubleWave, dashSmallGap, dashDotStroked, threeDEmboss, threeDEngrave, outset, inset (an unknown style ranks as single).
5. Same weight and style: the darker colour wins, comparing `R + B + 2G`, then `B + 2G`, then `G`, lowest first (`auto` is black); if all tie, this cell's border.

The winning top edges set the row's top-border space ([TABLE-27](#table-27)); the winning left and right edges set the content box ([TABLE-24](#table-24)) and the autofit padding ([TABLE-7](#table-7)).

### TABLE-37. Border thickness and drawing {#table-37}

In layout ([TABLE-27](#table-27)) a border takes `h(b)` = its `w:space` in points plus its line, the line twice with `w:shadow`. The line is the width rounded down to whole twips (`w:sz × 5 div 2` twips: `sz` 3 is 0.35pt); double and compound styles their stripes as below; `wave` 3.75pt at `sz` 12, else 3pt; `doubleWave` 5.25pt; `dashDotStroked` 3pt; `threeDEmboss` and `threeDEngrave` the width plus 1.5pt (plus 3pt from 3pt wide). An art border (any `w:val` that isn't a line style) is `w:sz` points wide (at most 255), in row heights and in horizontal geometry alike, except the cell's content box ([TABLE-24](#table-24)); it is drawn as a line of its width (`w:sz / 8` pt) at the top or left of that band, the rest left empty. Drawing uses the width.

A double border is line + gap + line, each its width `w`: it is `3w` thick in layout (rows grow by `3w`, the padding counts `3w`), and it is drawn as two lines of `w` at the outer sides of that band. Every other style, compound ones included, counts once and is drawn as one band of its width: solid, or for a patterned line dashes along the band, on and off in units of the line width from the band's start: `dotted` 1, 1; `dashed` 3, 1; `dotDash` 3, 1, 1, 1; `dotDotDash` 3, 1, 1, 1, 1, 1; `dashSmallGap` 4, 1 (of the band's thickness). Not done: `wave`, `doubleWave` and `dashDotStroked` are drawn solid.

`threeDEmboss` and `threeDEngrave` are three lines across the band with no gap: `o`, the width, `o` (`o` 0.75pt, 1.5pt from 3pt wide), top or left first light, middle, dark when embossed and dark, middle, light when engraved. The colours come from the border's colour (`auto` as A0A0A0) on the 0–240 hue/lightness/saturation scale, in integers: black gives C0C0C0, 808080, 000000 and white FFFFFF, C0C0C0, 808080; any other colour takes `L' = L + 40` if `L < 60`, `L` if `L < 120`, else `L − 40`, and its lines are at lightness `L' + 40`, `L'`, `L' − 40`; the middle line is drawn in the average of its colour and that colour at `L' − 60`.

With no cell spacing, draw each cell's winning edges as filled bands of their thickness `t`:

- **top**: from the row's top down by `t`, horizontally from the cell's left border centre minus half the left edge's thickness to its right border centre plus half the right edge's; not for vertical-merge continuation cells;
- **left, right**: centred on the cell's edge, over the row's (or merged region's) height;
- **bottom**: only on the table's last row and on the part of a split row left on a page ([TABLE-41](#table-41)): from the row's bottom down by `t`, spanning like the top.

Fill a cell's `w:shd` over its box, border centre to border centre, over the row's (or region's) height, under the borders.

### TABLE-38. A bottom border over part of a wider cell {#table-38}

Without cell spacing: when a cell in the row above has a visible bottom edge ([TABLE-36](#table-36)) and no cell of this row starts at that cell's grid column (a wider cell of this row covers it, starting further left), draw that bottom edge at this row's top, under the cell above only (from its left border centre minus half its left edge to its right border centre plus half its right edge). Skip it when the covering cell is a vertical-merge continuation. Such a border counts in this row's top-border space: `topBorder = max(topBorder, its thickness)`.

Example: a cell with a 0.72pt bottom border above a wide cell whose top is `nil` that starts further left: the border is drawn under the upper cell, and the lower row starts 0.72pt lower.

### TABLE-39. Cell spacing {#table-39}

With `w:tblCellSpacing` = s > 0 (the table's value, in points):

- **Gaps:** a cell side on the table's outer boundary gets 2s, an interior side s. Neighbouring cells are s + s apart; outer cells are 2s from the table's border centre line.
- **Each cell draws its own box**, from `cellX + gapL` to `cellX + w − gapR`, and from `rowTop + gapTop` to `s` above the row's bottom (`2s` above it on the table's last row, whose height holds that gap), where `gapTop = 2s` plus the table's top border on the first row, and `s` on the others. Its four edges are its own candidates ([TABLE-36](#table-36): the cell's border, else the table's), not resolved against the neighbouring cells, and are drawn **inside** that box (top band from the box top down, bottom band up from the box bottom, left and right bands inside the box's sides). Shading fills the box.
- **Cell text** starts at `cellX + gapL + bL + marginL` and is `w − that − marginR − bR − gapR` wide.
- **Row pitch:** `topBorder` of [TABLE-27](#table-27) plus `gapTop`; each cell's content counts `topMargin + content + bottomMargin + (the widest bottom edge in the row, by its h())`, plus `s` on every row but the last.
- **Table bottom:** the table's bottom border width (the last row holds the 2s above it).
- **The table's own border** (`w:tblBorders` outer sides) is drawn as usual around the cells: its top on the first row from the row's top, its sides on every row centred on the table's left and right border centres, its bottom below the last row, 2s under the last cell box.
- Autofit counts the spacing as in [TABLE-7](#table-7), [TABLE-17](#table-17) and [TABLE-21](#table-21). The rule of [TABLE-38](#table-38) doesn't apply.

Example: s = 2pt, 0.5pt borders, margins 0 top and bottom, one 13.43pt line: an interior row's pitch is s + 0.5 + 13.43 + 0.5 + s = 18.43pt.

## Rows across pages

### TABLE-40. When a row fits {#table-40}

Place rows top-down. A row fits if

```
rowTop + rowPitch + tableBottomBorder + noteArea ≤ columnBottom
```

where `tableBottomBorder` is the table's bottom border space ([TABLE-27](#table-27)) whatever the row, and `noteArea` the footnote area the column would have with this row's references ([NOTE-8](note#note-8)). Unlike a paragraph's space-after ([PAGE-2](page#page-2)), the border must fit. After the table's last row, advance by its bottom border.

### TABLE-41. Splitting a row {#table-41}

A row that doesn't fit splits by default ([TABLE-45](#table-45) for the exceptions):

1. `room = columnBottom − noteArea − rowTop − topBorder − tableBottomBorder`.
2. Each cell keeps, from its content, the lines (and nested-table rows, below) that fit in `room − topMargin − bottomMargin` ([TABLE-43](#table-43), [TABLE-44](#table-44) say which), a line that references footnotes only with its notes ([NOTE-8](note#note-8)); the rest goes on.
3. If the split is allowed ([TABLE-42](#table-42)), draw the part on this page with height `topBorder + max over cells of (topMargin + kept height + bottomMargin)`, where a cell whose part ends with a whole paragraph (the end of its content, or a paragraph the cut kept whole) also counts that paragraph's space-after. Give it a closing bottom border (each cell's bottom edge, [TABLE-37](#table-37)). The page is then full.
4. The continuation starts the next column with the row's top border and top margin again, and its height is worked out the same way (a part that ends its cell includes that cell's last space-after, like a whole row). It may split again.
5. A cell whose continuation starts with a new paragraph (the cut fell between paragraphs) keeps at its top what the gap to the previous paragraph ([PARA-1](para#para-1)) leaves after that paragraph's space-after, which stayed on the page before, as after a next-page section break ([PAGE-7](page#page-7)). A cut inside a paragraph keeps nothing.

Example: a cell cut after an empty paragraph with no space-after; the next paragraph has a 9.4pt space-before, so the continuation's first line starts 9.4pt below the cell's top margin (compat 12).

Each cell breaks at its own line boundary.

A nested table is cut by the same rules inside its cell's room. Its rows stay while they fit, as the cell's lines do; a nested row that doesn't fit splits like a table's row (steps 2 to 5 against the room left below its top, [TABLE-42](#table-42)), so each nested cell keeps its leading lines that fit. It moves on whole only when that fails or the row may not split: a cantSplit, exact or atLeast row as in [TABLE-45](#table-45), and in compat ≤ 14 a header row. A nested table cut on a page, between its rows or inside one, ends its part there with its bottom border, which counts in the cell's kept height. In compat 15 that border must fit below the last row kept; in compat ≤ 14 the fit test leaves it out. The continuation starts with the next row, or the split row's rest, with its top border. Nested rows have no keep-with-next, `pageBreakBefore` or repeated header rows ([TABLE-45](#table-45), [TABLE-46](#table-46)).

Example: a cell whose nested table's last row on the page holds two lines in its first cell and one in its second, with room for one line: the first line of the first cell and the one line of the second stay, closed by the nested table's bottom border, and the second line continues on the next page at the top of that row's rest.

### TABLE-42. When a row may split {#table-42}

A row splits only if every cell that has content keeps at least its first line on this page, that line's paragraph's space-before included (an empty cell's empty paragraph is a line like any other), and at least one cell keeps something. Otherwise the whole row moves to the next column.

### TABLE-43. Widow and orphan control at a row split {#table-43}

In compat ≤ 14 a cell's cut is where its lines stop fitting: no widow, orphan or keep rule applies inside a table. In compat 15, where a cell's cut falls inside a paragraph that has widow/orphan control on, apply [PAGE-3](page#page-3) to that paragraph: the cell keeps the number of its lines that rule takes. A paragraph that continues a row already split on an earlier page counts as already begun (no orphan rule for it). If a cell then keeps nothing, [TABLE-42](#table-42) fails and the row moves whole.

Example: a five-line cell paragraph, widow control on, four lines of room: three lines stay, two go on.

Then, in compat 15, the body's keeps ([PAGE-6](page#page-6)) apply inside the cell: the cut doesn't fall inside a keepLines paragraph (it moves to that paragraph's start), and a cut at a paragraph's start moves back over a keepNext paragraph before it, taking its last line (two with widow control) or, with keepLines, all of it; the step goes on over a paragraph moved whole.

### TABLE-44. Space-after at a row split {#table-44}

At a row split a cell line stays only if its full pitch fits, `lineTop + pitch ≤ room`: the extra space multiple line spacing adds below its text counts, with none of [PAGE-2](page#page-2)'s allowance. A cell paragraph's space-after is never dropped either: a line that ends its paragraph stays only if its space-after fits below it too. Each cell keeps what fits by this test, so a short neighbour cell keeps all of its content.

Example: three one-line paragraphs at 1.15 spacing whose last text ends 2pt above the body bottom but whose pitch ends 0.03pt past it: the first two lines stay and the third goes on. A one-line row in that place keeps nothing, so it moves whole ([TABLE-42](#table-42)).

Example: a cell whose first paragraph is one 13.44pt line with 20pt space-after, 15pt of room: the line doesn't stay, so the row moves whole ([TABLE-42](#table-42)).

### TABLE-45. cantSplit, header, keep-with-next and exact rows {#table-45}

- `w:cantSplit`, a header row (`w:tblHeader`) or a keep-with-next row (below): the row never splits; it moves whole to the next column. In compat 15, at the top of an empty column, or as the first row placed in a column when it continues a split, it may split after all, so a cantSplit row taller than a page moves to a new page and splits there. In compat ≤ 14 it never splits: at the top of an empty column it is placed whole and runs past the column's bottom, cut off by the page edge ([PAGE-5](page#page-5)).
- An exact-height row ([TABLE-28](#table-28)) never splits.
- A row whose first cell's first paragraph has `w:keepNext` (in its style or directly) keeps with the next row. A run of such consecutive rows that starts below other content in the column (the content above the table, or rows of this table) moves to the next column with the row after it when that row can't start here, neither whole nor as a split part. A run that starts at the top of the column breaks where it must. In compat 15 header rows are keep-with-next rows too ([TABLE-47](#table-47)).
- A row after the table's first whose first cell's first paragraph has `w:pageBreakBefore` starts the next column, unless it is the first thing in its column; the run of keep-with-next rows before it moves with it. In compat 15 a keep-with-next row right before it cancels the break.
- A row that can't be placed in an empty column (it doesn't fit and can't split) is placed whole anyway ([PAGE-5](page#page-5)).

### TABLE-46. Repeated header rows {#table-46}

The table's leading rows with `w:tblHeader` are its header rows (stop at the first row without it). When the table continues into a new column or page, and the next row to place isn't itself a header row, first draw all the header rows at the top of that column, at their full height, then continue (also above the continuation of a split row). They keep their own formatting, `firstRow` included ([TABLE-3](#table-3)).

### TABLE-47. Header rows never end a page alone {#table-47}

In compat 15, if a table starts below other content in a column and the first row after its header rows doesn't start there (neither whole nor as a split part), don't leave the header rows behind: the whole table moves to the next column, as if its first row didn't fit. Compat ≤ 14 leaves the header rows at the bottom of the column.

### TABLE-48. Paragraphs around a table {#table-48}

- The space-after of the paragraph before a table applies, in full, above the table's top border edge (not if the table starts at the top of an empty column), unless contextual spacing drops it ([PARA-4](para#para-4)).
- The paragraph after a table takes its full space-before, from the table's bottom border edge, unless contextual spacing drops it ([PARA-4](para#para-4)); nothing collapses against the table.
- A `w:keepNext` paragraph before a table keeps with its first row: if the table can't place its first row (or a first part of it) in this column, the keepNext chain gives way as [PAGE-6](page#page-6) says (compat 15: only the last line, or two with widow control, of the paragraph next to the table moves).

### TABLE-49. Floating table position {#table-49}

- **Horizontally**, `w:tblpX` takes `w:tblInd`'s place ([TABLE-23](#table-23)): compat 15, left border centre = `tblpX + hb(bL)`; compat ≤ 14, `tblpX − max(hb(bL), lm) + tblInd` (`lm` the first cell's left margin, `hb` half a border rounded up to a twip, `tblInd` its `dxa` value). `w:tblpXSpec` `left` (and `inside`) is `tblpX` 0. `center` and `right` (`outside`) place the table's **frame** in the reference: centred, or against its right end. The frame:
  - compat 15: from the table's outer box left to the widest row's last cell edge, plus that cell's right border width less its half (truncated to a twip); a cell with no right border at all counts a whole 0.75pt, no half taken off (a `none` one counts 0). The outer box starts at the frame's left.
  - compat ≤ 14: `tblInd` plus the widest row's last-cell content right, from the first cell edge: the borders and padding hang outside it. The first edge sits `tblInd` right of the frame's left (the left border centre `max(hb(bL), lm)` left of that edge).
  - The reference is the column (`w:horzAnchor` `text`, `margin`) or the page (`page`).
  - Compat ≤ 14 with a negative `tblInd`: the obstacle (below) keeps starting at `tblpX`, not at the table that hangs out left of it.
- A floating table takes no room in the flow: the text after it goes on from where the table stands, and wraps around it.
- **Vertically** (`w:vertAnchor`, absent = `margin`):
  - `text`: `tblpY` below where the table stands in the flow.
  - `page`, `margin` (top margin to bottom margin): `tblpY` less one twip from the reference's top, or `w:tblpYSpec` `top`, `center`, `bottom` of the table's height within it.
  - Never above the page's top edge.
  - The position is taken from the page's first layout: when the text before the table re-wraps around it, the table doesn't move again.
- **The obstacle** on each page is the table's part there: its outer box (half the left border, rounded up to a twip, left of the left border centre; half the right border right of the last edge) grown by `w:leftFromText` / `w:rightFromText` (at least 0.5pt each) and `w:topFromText` / `w:bottomFromText` (negatives → 0). Text wraps on both sides of it, as around a square float ([FLOAT-2](float#float-2)). It reaches above where the table stands only when the table itself does (negative `tblpY`, or anchored above): only then does the text before it re-wrap.
  - Compat ≤ 14: a table in the flow (`text`, `tblpY` 0) has no top or bottom distance.
  - The obstacle moves with the frame onto whole 0.12pt device pixels from the page's left edge: the pixel nearest the outer box's left, or, for a numeric `tblpX` against the `margin` or `page`, nearest one twip left of it. Example: an outer box at 116.35pt gives a frame at 116.28pt; the text right of the table moves 0.07pt left with it.
  - A paragraph's first line that meets the obstacle goes to its bottom plus the paragraph's own space before, even at the top of a page: the space before stays above the line wherever it lands.
- **A split** floating table continues at the top of the next page like an inline table; the text after it starts at the top of the page that holds its last part, beside it.
  - Compat ≤ 14 with `w:doNotBreakWrappedTables`: the table isn't split; one that doesn't fit below its anchor moves to the next page (its rows may run past the bottom margin).
- **`w:tblOverlap`**: compat 15, a `never` table that overlaps an earlier floating table's obstacle on the page moves right to its edge (its own left distance included). Compat ≤ 14, every floating table that overlaps an earlier one by 9pt or more, distances included, moves down to the other's obstacle bottom plus its own top distance.
- Example: a page-anchored table at the page top, 280pt tall, then a paragraph with 12pt before: the paragraph's first line starts at 292pt.
- Not done: `inside` / `outside` by page parity, an aligned (`tblpYSpec`) frame kept whole, the 18pt minimum width of a free interval ([FLOAT-7](float#float-7)), floating tables inside table cells, and drawing the table itself at its frame's device pixel.

### TABLE-50. A row's own indent {#table-50}

A row's `w:tblPrEx` (table property exceptions) can carry its own `w:tblInd`. In compat ≤ 14:

- In an **autofit** table, the row moves by `rowInd − tblInd`: all its cells shift by that much, keeping their widths. The other rows keep the table's indent.
- In a **fixed** table (`w:tblLayout w:type="fixed"`), the row's indent is ignored: every row sits at the table's `tblInd`.

Example: an autofit table at `tblInd` 513 twips whose third row has `tblInd` 0 draws that row 25.65pt left of the others; the same rows in a fixed table line up.

This applies to left-aligned tables that don't float, whose exceptions also set `w:tblLayout` fixed. Centred, right-aligned and floating tables, and compat 15, keep the table's indent.

### TABLE-51. Fitted cell text {#table-51}

A cell with `w:tcFitText` lays each paragraph out on one line: it wraps only at line, column and page breaks. The line starts at the left of the cell's content box, whatever the paragraph's alignment, and its characters are spaced so the text ends at the **fit width**: the content box width less the paragraph's left, right and first-line indents (a hanging indent widens it) and 1pt. The line keeps its height. The difference is spread evenly over the line's pieces.

In autofit a fitted cell with a `dxa` width is that wide: its minimum, maximum and hard minimum are all its `w:tcW` ([TABLE-7](#table-7)).
