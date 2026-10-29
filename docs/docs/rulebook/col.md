---
title: Columns
sidebar_position: 12
---

A section's `w:cols` splits its text area into columns. Each column fills like a page ([PAGE-1](page#page-1) to [PAGE-9](page#page-9)), then the flow moves to the next column, then to the next page. These rules give the column boxes, how text and footnotes flow through them, and how a section that ends in a continuous break is balanced.

### COL-1. Column geometry {#col-1}

Work from the body area ([HDR-4](hdr#hdr-4), [HDR-6](hdr#hdr-6)): its left edge `left` and width `textW`.

- **Equal columns** (`w:equalWidth` absent or on, or no `w:col` children): `n = w:num` (at least 1), `space = w:space` (default 720 twips = 36pt).
  ```
  width = (textW − (n − 1) · space) / n
  x(k)  = left + k · (width + space)          k = 0 … n − 1
  ```
  Example: Letter with 1in margins (`textW` 468), two columns, space 36 → width 216, columns at x 72 and 324.
- **Unequal columns** (`w:equalWidth="0"` with `w:col` children): one column per `w:col`, its `w:w` used exactly as written, starting at `left`, each followed by its own `w:space`. Don't scale them; they may run past the right margin. The last column's space is unused.
- Every column runs from the body top (or from where the section starts on the page) to the body bottom.
- **Separator** (`w:sep`): between columns k − 1 and k, draw a vertical line 0.72pt wide centred in the gap, `x = (right edge of column k − 1 + left edge of column k) / 2`. Draw it only if column k holds text on this page. It runs from the section's top on the page down to the lowest text baseline in any of its columns plus a quarter of that text's font size.

### COL-2. Column flow {#col-2}

- A column that is full continues in the next column; after the last column, on the next page.
- `w:br w:type="column"` ends the column; the text after it starts the next column. A column break that ends its paragraph leaves the paragraph's empty remainder as the first line of the next column.
- Space-before at a column top: dropped after natural flow ([PARA-2](para#para-2)). If a paragraph's first character is a column break, the rest of that paragraph keeps the paragraph's space-before at the top of the next column; a column break after text in the paragraph drops it.
- Widow/orphan control and keeps apply at every column bottom, not only at page bottoms.
- Lay out tables against the column width (`pct` 5000 = the column width). A fixed table wider than the column overflows it, unclipped.
- A section break of type `nextColumn` acts as a next-page break.
- Footnotes in a section with several columns: compat 15 uses one stream for the page ([COL-3](col#col-3)); compat ≤ 14 gives each column its own area at the column's width ([COL-4](col#col-4)).

### COL-3. Compat 15: one note stream per page {#col-3}

In compat 15, when the section has two or more columns, the page's footnotes form **one stream** laid out at column width and spread over the columns.

1. **Stream height.** For a set of notes, find the smallest height `h` at which they fill the columns in order (column 1 first, splits inside a note paragraph obeying [NOTE-7](note#note-7)). Every column reserves the same area: `separator + h`, at its bottom.
2. **Fit test.** A line referencing notes fits only if the stream, with its new notes added, balances within the room below the line: `lineBottom + separator + h ≤ bodyBottom`. If it doesn't, the line moves on to the next column (or page) with its notes. The stream never continues on the next page.
3. **Refill.** Reserving the area shortens every column, including ones already filled. Fill the page again with the new reserved height, and repeat until the height no longer changes.
4. **Drawing.** The area ends at the body bottom. Put one separator at the left of column 1, then the stream's columns, all starting at the same y below it (top-aligned; a shorter last column ends higher).

Example: notes of 3 lines in two columns fill 2 / 1; 13 lines fill 7 / 6.

A balanced section doesn't use the stream: see [COL-5](col#col-5).

### COL-4. Compat ≤ 14: per-column notes and their continuation {#col-4}

In compat ≤ 14 (and in any single-column section), each column has its own note area for the notes referenced in it, stacked as in [NOTE-1](note#note-1) and laid out at the column's width.

A note that doesn't fit ([NOTE-6](note#note-6)) continues at the bottom of the **next column**, on the same page or the next page's first column:

- Place the continued part first in that column's area, before the column's own notes, under the continuation separator, which spans the column width in compat ≤ 14 ([NOTE-3](note#note-3)).
- Place the continuation before any body text: it may fill the whole column. Body text gets the room left above it.

### COL-5. Balancing before a continuous break {#col-5}

Balance a section when it has two or more columns, the **next** section starts with a `continuous` break, and the section ends on this page. The document's last section is balanced only when endnotes follow it: its body balances as here, and the endnotes start below its tallest column, in its first column ([NOTE-10](note#note-10)). Only the page where the section ends is balanced.

1. Find the **smallest column height `h`** (from the section's top on the page) at which the section's remaining content fills its columns in order with all the normal rules (fit, widow/orphan control, keeps, space-before dropped at column tops). Bisect between 0 and the full column height. A height fails if the content doesn't end within the n columns, or if any column had to drop a keep or widow rule ([PAGE-5](page#page-5)).
2. While balancing, nothing is suppressed at a column bottom: every line needs its whole pitch, and a paragraph's last line needs its space-after too, unless the paragraph starts the column.
3. Fill the columns in order at `h`, so the leftover goes to the last columns.
4. The next section starts at `sectionTop + h`, below the tallest column including its last space-after.
5. If no height works, leave the section unbalanced: the next section starts at the full column bottom.
6. Notes referenced in a balanced section go in per-column areas ([COL-4](col#col-4)), also in compat 15.

Single-line paragraphs, 2 columns: 3 → 2 / 1, 5 → 3 / 2, 7 → 4 / 3. Three columns: 5 → 2 / 2 / 1. Heights are balanced, not line counts.

### COL-6. The section-break paragraph takes no space when balancing {#col-6}

When balancing ([COL-5](col#col-5)), if the section's last paragraph is empty and the section has other content, it doesn't count: the section is complete once everything before it is placed.

Example: six one-line paragraphs followed by the empty paragraph holding `w:sectPr` balance 3 / 3.
