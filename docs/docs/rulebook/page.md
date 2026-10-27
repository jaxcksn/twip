---
title: Pagination
sidebar_position: 11
---

These rules decide which lines of the body go on a page (or in a column, which fills the same way, see [COL-2](col#col-2)). Fill the text area top-down, one line at a time. For each line, decide whether it fits, then apply widow/orphan control and the keep properties to the paragraph as a whole. All positions are in points, from the page top. Compare them exactly, without rounding.

### PAGE-1. The fit test {#page-1}

A line fits if `lineTop + lineHeight ≤ bodyBottom`, compared exactly (no rounding to a grid, no tolerance beyond floating-point noise). `bodyBottom` is the bottom of the text area ([HDR-6](hdr#hdr-6)), less any footnote area already on the page ([NOTE-1](note#note-1)).

Example: a line whose bottom lands at 146.484pt fits under a bottom of 146.50pt and not under 146.45pt.

### PAGE-2. Space that needn't fit at the bottom {#page-2}

At a bare page bottom (no footnote area on the page, no balancing), two kinds of space don't have to fit:

- the paragraph's **space-after**: never count it when testing the paragraph's last line;
- the extra space **below** the text that multiple line spacing adds ([LINE-7](line#line-7)): test each line with `lineTop + pitch − below ≤ bodyBottom`.

Example: a 13.44pt single line under 1.5 spacing has pitch 20.16pt, of which 6.72pt is below the text. It fits when `lineTop + 13.44 ≤ bodyBottom`.

Footnote areas change this: see [PAGE-8](page#page-8). Balanced columns change it too: see [COL-5](col#col-5).

### PAGE-3. Widow/orphan control {#page-3}

`w:widowControl` on sets `min = 2`; off sets `min = 1`. A paragraph has `n` lines and `fit` of them fit in the column ([PAGE-1](page#page-1)), with `rest = n − fit`:

```
if fit == n, or one of the fitting lines ends in a page or column break:
    take fit
elif this is the paragraph's first part and fit < min:
    take 0                               (the whole paragraph moves on)
elif rest < min:
    k = fit − (min − rest)
    if k < (min for a first part, 1 for a continuation): take 0
    else: take k
else:
    take fit
```

Taking 0 moves the paragraph (or its remaining part) to the next column; [PAGE-5](page#page-5) handles an empty column.

Five-line paragraph, widow control on: 4 lines of room → 3 + 2; 1 line of room → the whole paragraph moves. Off: 4 + 1 and 1 + 4. A continuation already split from the previous page may leave a single line at the bottom (3 remaining lines, 2 of room → 1 + 2).

In table cells the same control applies to the paragraph a row split cuts through ([TABLE-43](table#table-43)). In notes it applies inside each note paragraph ([NOTE-7](note#note-7)).

### PAGE-4. widowControl defaults to on {#page-4}

Resolve `w:widowControl` through the style cascade ([STYLE-1](style#style-1)); the last level that writes it wins. If no level writes it (not docDefaults, not the style chain, not the paragraph), it is **on**. Only an explicit `w:val="0"` turns it off.

### PAGE-5. Forced progress {#page-5}

If nothing can be placed in an empty column (the first line doesn't fit, or widow/orphan control or keepLines would take 0 lines), place at least one line there regardless of its height: take `max(fit, 1)` lines, ignoring the rules that asked for 0. A table row that doesn't fit an empty column is placed whole in the same way.

Example: a header taller than the page leaves no text area; each page then still takes one body line (which may be invisible, see [HDR-9](hdr#hdr-9)).

### PAGE-6. Keeps {#page-6}

- **`w:keepNext`**: after placing a keepNext paragraph, test whether the next block's first part fits below it: for a paragraph, its gap ([PARA-1](para#para-1)) plus its first line (2 lines with widow control, all lines with keepLines), tested as in [PAGE-2](page#page-2); for a table, the preceding space-after plus its first row plus the table's bottom border. If not, move the whole chain of consecutive keepNext paragraphs, from its first member, to the next column.
- A chain whose first member is at the top of an empty column is never moved: it breaks normally. So a chain longer than a column breaks where it must.
- A keepNext paragraph before a table keeps with the table's first row: if the table can't start in this column, the chain moves with it.
- **`w:keepLines`**: a paragraph's first part that doesn't fit whole moves whole. Its continuation (when even an empty column couldn't hold it) splits normally.
- **`w:pageBreakBefore`**: the paragraph starts a new page, unless it already starts the first column of an empty page. Its space-before then follows [PARA-2](para#para-2).
- A keep or widow rule that can't be met in an empty column is dropped ([PAGE-5](page#page-5)).

### PAGE-7. Space-before after a next-page section break {#page-7}

The first paragraph of a section that starts on a new page (any break type but `continuous`) keeps its space-before at the page top, less the previous paragraph's space-after, which stayed on the page before:

```
kept = max(0, gap(spaceAfter(previous), spaceBefore) − spaceAfter(previous))      (gap: PARA-1)
```

With `doNotUseHTMLParagraphAutoSpacing` that is all of the space-before; otherwise it is what the space-after leaves of it, `max(0, spaceBefore − spaceAfter)`.

Example: after 10pt, a space-before of 12pt keeps 2pt at the top of the new page (12pt with `doNotUseHTMLParagraphAutoSpacing`).

Elsewhere at a page top reached by natural flow, space-before is dropped ([PARA-2](para#para-2)).

### PAGE-8. Above a footnote area nothing is suppressed {#page-8}

When the column already has a footnote area (or the line being tested brings one), [PAGE-2](page#page-2) doesn't apply. A line fits only if

```
lineTop + pitch + (space-after, if it is the paragraph's last line) + noteArea ≤ bodyBottom
```

where `noteArea` is the area's full height, separator included ([NOTE-1](note#note-1)).

Example: a line at y 690 with pitch 13.44 and space-after 8, above a 40pt note area, needs a bottom of at least 751.44; at a bare bottom it would need only 703.44.

### PAGE-9. Keep chains and footnotes {#page-9}

When testing whether a keepNext paragraph's successor fits ([PAGE-6](page#page-6)), subtract the column's current footnote area from the bottom first. If a chain member then places no line at all, rewind to the chain's first member and move the whole chain to the next column, as when the fit test fails. A chain that starts at the top of an empty column is still never moved.
