---
title: Footnotes and endnotes
sidebar_position: 17
---

Footnotes sit in an area at the bottom of the column that references them, under a separator; the body shrinks by the area's height. A line that references a note goes on the page only if enough of its note fits there too. What doesn't fit continues in the next column or page. Endnotes follow the document's last paragraph. Multi-column sections add the rules in [COL-3](col#col-3) and [COL-4](col#col-4).

### NOTE-1. The footnote area {#note-1}

Stack a column's footnote area **bottom-up, ending at the body bottom** ([HDR-6](hdr#hdr-6)), not at the page bottom:

```
[separator][note 1][note 2]…
areaTop = bodyBottom − (separatorHeight + Σ noteHeights)
```

Notes are stories ([HDR-3](hdr#hdr-3)) laid out at the column width, with no gaps beyond their own paragraph spacing. The body's usable height shrinks by the area height: text must end above `areaTop` ([PAGE-8](page#page-8)).

Only footnotes take room on the page. An endnote reference reserves no area and brings no separator: a line that references only endnotes is tested as any other line, above whatever footnote area the column already has.

### NOTE-2. The area is drawn 0.15pt low {#note-2}

Draw the whole note area, separator included, 0.15pt lower than [NOTE-1](note#note-1) puts it: `drawTop = areaTop + 0.15`. Pagination uses the unshifted area.

### NOTE-3. Separators {#note-3}

- The separator is the document's footnote separator story (`w:footnote w:type="separator"`), laid out as an ordinary story at the column width. Its `w:separator` element draws a rule **144pt** long from the line's left edge, 0.6pt thick, centred 2.4pt above the baseline.
- An area whose first note continues from an earlier column or page ([NOTE-6](note#note-6)) uses the continuation separator story (`w:type="continuationSeparator"`). Its `w:continuationSeparator` rule is 144pt in compat 15, and spans the line it is on, which is the column width, in compat ≤ 14.
- A document without the separator story gets a default: one line as tall as an 11pt Calibri line (13.43pt), with a 144pt rule from `0.78 × 13.43 − 2.7` to `0.78 × 13.43 − 2.1` below its top (y 7.78 to 8.38).

### NOTE-4. Numbers and reference marks {#note-4}

- Number notes in the order of their first reference in the body, tables included (cells in document order). Footnotes count 1, 2, 3, …; endnotes i, ii, iii, … (lower-case roman), separately.
- A reference with `w:customMarkFollows` shows no number and doesn't take one.
- The reference shows the number in its own run's formatting, usually a superscript character style, so its size and raise come from [FONT-8](font#font-8) and [FONT-9](font#font-9). Example: 11pt Calibri text gives a 7pt mark. The `w:footnoteRef` mark inside the note shows the same number.

### NOTE-5. A referencing line fits only with its notes {#note-5}

For a line holding footnote references, with `lineBottom = lineTop + pitch − below` ([PAGE-2](page#page-2)):

1. If the column has no note area yet, add the separator's height.
2. For each referenced footnote, in order: `room = bodyBottom − lineBottom − areaHeight`. The note's **minimum first part** is 2 lines if its first paragraph has widow control, else 1 (never more than that paragraph's lines).
   - If the minimum part doesn't fit in `room`, the line doesn't fit.
   - If the whole note fits, add it to the area.
   - Otherwise split it ([NOTE-7](note#note-7)), keep at least the minimum part here and carry the rest ([NOTE-6](note#note-6)). A later reference on the same line then can't fit: the line doesn't fit.
3. Then the line itself must fit above the area as in [PAGE-8](page#page-8).

A line that doesn't fit moves to the next column or page with its notes ([PAGE-3](page#page-3) decides what moves with it).

### NOTE-6. Continued notes {#note-6}

- Once a note is split, the paragraph stops after the referencing line: the body text after it moves to the next column or page rather than squeezing the note.
- The rest goes at the **top of the next column's note area** (the next page's first column in a single-column section), under the continuation separator ([NOTE-3](note#note-3)), before that column's own notes. Place it before any body text of that column: it may take the whole column.
- Fill continued parts in order, line by line, as far as they fit in the column. Once one part has to wait, every later part waits for the next column too.

Compat 15 sections with several columns don't continue notes: [COL-3](col#col-3).

### NOTE-7. Splitting a note {#note-7}

Split a note after the last line that fits. If the cut falls inside a paragraph with widow control:

- if fewer than 2 of its lines would follow, move the cut up so 2 follow;
- if then fewer than 2 of its lines stay before the cut (but at least one would), move the whole paragraph on.

A cut between two paragraphs, or between two notes, is unrestricted. Example: a 4-line note paragraph with 3 lines of room splits 2 + 2.

### NOTE-8. Several references, and references in tables {#note-8}

- All the notes referenced on one line go on that page when they fit ([NOTE-5](note#note-5)); if one doesn't, the whole line moves.
- References in table cells behave like body references: a row fits only if its row height, the table's bottom border and the minimum parts of all the notes referenced in it fit.

### NOTE-9. Notes in paragraphs wrapped around floats {#note-9}

A paragraph that wraps around a float ([FLOAT-2](float#float-2)) is placed band by band; each band's references place their notes exactly as a line's do ([NOTE-5](note#note-5), [NOTE-6](note#note-6), [PAGE-8](page#page-8)).

### NOTE-10. Endnotes {#note-10}

Endnotes go **immediately after the document's last paragraph**, in the last section's flow: the endnote separator story's paragraphs, then each endnote's content in the order of first reference. They flow and paginate like body text. Every separator paragraph keeps with the next ([PAGE-6](page#page-6)), so the separator never ends a page without the first endnote line. A document without an endnote separator story gets no separator.

When the last section has two or more columns and the document has endnotes, the section's body ends as before a continuous section break: balance its last page ([COL-5](col#col-5)), then start the endnotes below its tallest column, in its first column, and let them flow through the columns from there.
