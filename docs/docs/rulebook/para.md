---
title: Paragraph spacing and borders
sidebar_position: 6
---

These rules place paragraphs one after another: the gap between two paragraphs, what happens to space-before at the top of a page or column, contextual and automatic spacing, and the room paragraph borders take. A paragraph's lines are sized first ([LINE-1](line#line-1) onward); the gap goes between the previous paragraph's last line and this paragraph's first line.

### PARA-1. The gap between paragraphs {#para-1}

```
gap = max(spaceAfter(previous), spaceBefore(next))
```

With `w:compat/w:doNotUseHTMLParagraphAutoSpacing` the gap is the sum, `spaceAfter + spaceBefore`. The same gap applies in table cells, headers, footers, notes and text boxes.

Example: after 10pt, then before 6pt: a 10pt gap (16pt with `doNotUseHTMLParagraphAutoSpacing`).

### PARA-2. Space-before at the top of a page or column {#para-2}

When a paragraph starts at the top of an empty page or column:

| How it got there | Compat 15 | Compat ≤ 14 |
|---|---|---|
| it is the first paragraph of the document | kept | kept |
| the previous page or column overflowed | dropped | dropped |
| the previous paragraph ended with a page-break character | dropped | dropped |
| it has `w:pageBreakBefore` | dropped | kept |
| it is the first paragraph of a next-page section ([PAGE-7](page#page-7)) | kept | kept |

A kept space-before is what the gap to the previous paragraph ([PARA-1](para#para-1)) leaves after that paragraph's space-after, which stayed on the page before ([PAGE-7](page#page-7)); the document's first paragraph keeps all of it.

When a paragraph's **first character** is a column break, the rest of that paragraph starts the next column with its space-before again ([COL-2](col#col-2)). When its first character is a page break, the rest starts the next page with what the previous paragraph's space-after left of its space-before, as after a next-page section break ([PAGE-7](page#page-7)): all of it with `doNotUseHTMLParagraphAutoSpacing`, else `max(0, spaceBefore − spaceAfter(previous))`. A page or column break that follows text in the same paragraph drops the space-before, and so does any other paragraph at a column top reached by flow. Compat 12 and 15, one column or two, are the same.

Example: a paragraph with 6pt before that starts with a page break starts the next page 6pt down after a paragraph with no space-after, and at the top after one with 6pt after. With text before the break, it starts at the top.

### PARA-3. Space-after at the bottom {#para-3}

A paragraph's space-after never has to fit at the bottom of a page: a paragraph whose last line fits stays even when its space-after runs past the bottom ([PAGE-2](page#page-2)).

### PARA-4. Contextual spacing {#para-4}

Split the gap ([PARA-1](para#para-1)) into the previous paragraph's space-after and what the next one's space-before adds to it: `max(0, spaceBefore − spaceAfter)`, or all of `spaceBefore` with `doNotUseHTMLParagraphAutoSpacing`. Between two consecutive paragraphs **of the same paragraph style**, each one with `w:contextualSpacing` drops its own part:

```
gap = (previous contextual ? 0 : spaceAfter) + (next contextual ? 0 : added spaceBefore)
```

So two contextual paragraphs have no gap; a contextual paragraph before a plain one keeps only what the plain one's space-before adds (its own dropped space-after still reduces it); a plain one before a contextual one keeps its space-after. Different styles keep the normal gap even when both are contextual.

**Across cell, row and table edges** the neighbour is the paragraph next to it in document order, and nothing collapses there: each side is dropped or kept whole.

- A cell's first paragraph drops its space-before when it is contextual and the paragraph before it has its style: the previous cell's last paragraph; in a row's first cell, the previous row's end mark, which has the default paragraph style; in the table's first row, the paragraph before the table.
- A cell's last paragraph drops its space-after (from the row height, [TABLE-29](table#table-29)) when it is contextual and the paragraph after it has its style: the next cell's first paragraph; in the row's last cell, the row's end mark (default paragraph style), even at the end of the table.
- The paragraph before a table drops its space-after ([TABLE-48](table#table-48)) when it is contextual and has the style of the table's first paragraph.
- The paragraph after a table drops its space-before when it is contextual and has the default paragraph style (its previous paragraph is the last row's end mark).

Only the paragraph whose side is dropped needs `w:contextualSpacing`. In compat ≤ 14 with `w:compat/w:allowSpaceOfSameStyleInTable`, paragraphs in table cells always keep the normal gap, their space-before and their space-after; body paragraphs aren't affected. Compat 15 ignores the flag.

Example: after 10pt, then 6pt before: 0 when the first is contextual (the 6pt add nothing to the 10pt); after 0pt it would be 6pt. After 8pt, then a contextual paragraph with 0 before: 8pt. A contextual Normal paragraph with 12pt before, in the second cell of a row whose first cell ends with a Normal paragraph: no space above it.

### PARA-5. Automatic spacing {#para-5}

`w:beforeAutospacing` and `w:afterAutospacing` replace the written value with 14pt, and the result collapses like any other spacing ([PARA-1](para#para-1)). The automatic space-before is 0 on the document's first paragraph; on the first paragraph of a table cell, header, footer, note or text box it stays 14pt.

Two consecutive paragraphs are in the same list when both have numbering whose `w:num` uses the same `w:abstractNum`, whatever their `w:numId`s and levels. Between them, a side that comes from autospacing is 0 (the first paragraph's automatic space-after, the second's automatic space-before); an explicit value on the other side stays, and PARA-1 applies. The first item's automatic space-before and the last item's automatic space-after stay. Different abstract lists, or a list paragraph next to a plain one, keep 14pt.

With `w:compat/w:doNotUseHTMLParagraphAutoSpacing` the automatic value is 5pt, the two sides add (PARA-1) and lists make no difference. Compat 12 and 15 are the same.

Example: two items with automatic spacing on both sides, numIds 1 and 2 on the same abstractNum: no gap (10pt with `doNotUseHTMLParagraphAutoSpacing`). With an explicit 12pt before on the second: 12pt.

### PARA-6. A worked default {#para-6}

The common default paragraph formatting `w:spacing w:after="160" w:line="259" w:lineRule="auto"` gives each line a pitch of `H · 259/240` and 8pt after the paragraph.

Example: Calibri 11: lines 13.4277 × 259/240 = 14.4907 apart, and 8pt more between paragraphs.

### PARA-7. Paragraph borders {#para-7}

Border widths are `w:sz / 8` points, `w:space` is in points. A `double` border is a line, a gap and a line, each that width: it takes 3× the width.

- **Vertically**, a bordered paragraph occupies: top border width + top `w:space` + its lines + bottom `w:space` + bottom border width. Add the top part above the first line (it moves the first line's baseline down by the same amount) and the bottom part below the last line. The paragraph spacing ([PARA-1](para#para-1)) stays outside the box.
- **Consecutive paragraphs with identical border settings** (all of top, left, bottom, right and between) form one box: the first has the top border, the last the bottom border, and nothing is drawn or added between them. Paragraphs whose settings differ in any way are separate boxes. `w:between` borders are never drawn.
- **Horizontally**, the box runs from `left − (left space + left width)` to `rightEdge + right space + right width`, where `left = leftIndent + min(0, firstLineIndent)` is the paragraph's leftmost indent (a hanging first line, like a heading's number, is inside the box) and `rightEdge` is the text width less the right indent. Draw the left and right borders down the full pitch of every line of the box.
- **Shading** (`w:shd`) fills the box inside its borders: from the inner edge of the left border to the inner edge of the right one, and from below the top border to above the bottom one, so the `w:space` between the borders and the text is shaded too. Without a left or right border it runs from `left` to `rightEdge`. It covers the lines (and the border space added to them), not the paragraph spacing.

Example: one Calibri 11 line with 0.5pt top and bottom borders (`w:sz="4"`) and `w:space="1"`: pitch 13.4277 + 1.5 + 1.5 = 16.4277, baseline 1.5 + 10.4736 = 11.9736 below the line's top.
