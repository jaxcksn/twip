---
title: Line numbering
sidebar_position: 19
---

`w:sectPr/w:lnNumType` numbers the body lines of a section in the left margin of each column. Count lines as they are placed on pages, and draw the numbers that the count interval selects beside them.

### LNUM-1. Placement {#lnum-1}

- Read `w:countBy` (default 1), `w:start` (default 0), `w:distance` (twips, default 0) and `w:restart` (default `newPage`).
- Draw each number **right-aligned, ending at `columnLeft − distance`**. A distance of 0 means auto: 18pt (0.25in) with one column, 9pt (0.125in) with several.
- Count from the **column's** left edge: paragraph indents don't move the numbers, each column has its own, and with mirror margins they stay left of the text.
- The number's baseline is the text line's baseline (for a line wrapped around a float, the baseline of its band).
- Font: the `LineNumber` character style (styleId `LineNumber`) applied over the document's default run properties, or the default run properties alone when the style doesn't exist. The size doesn't follow the line's text.
- Don't draw a number that would start left of the page edge (x < 0).

Example: one column at x 72, auto distance: the number ends at x 54.

### LNUM-2. Counting {#lnum-2}

- Every body line placed on a page counts: wrapped lines, empty paragraphs, and the endnotes, which flow with the body ([NOTE-10](note#note-10)).
- Not numbered **and not counted**: paragraphs with `w:suppressLineNumbers`, every line in a table, headers and footers, footnotes, text boxes and frames.
- The number of a line is `start + count`, so the first line is `start + 1`. Draw it only when it is a multiple of `countBy`; every line still counts.
- `restart`: `newPage` restarts the count on every page; `newSection` at the start of every section; `continuous` never. A section that starts on a new page restarts the count unless its restart is `continuous`. Counting continues from column to column.
- A section without `w:lnNumType` has no numbers, and its lines don't count.

Example: `countBy="5"`, `start="0"`: numbers 5, 10, 15 beside the 5th, 10th and 15th lines of each page.

### LNUM-3. Pleading paper {#lnum-3}

Pleading layouts number every line with `w:lnNumType` and exact 24pt body lines. The numbers line up with the text only because every line has the same exact pitch, with the baseline at `top + 0.8 × 24` ([LINE-10](line#line-10)).

**Don't snap body lines to the numbers.** Any paragraph spacing in the body moves its lines off the 24pt grid, and they stay off it.
