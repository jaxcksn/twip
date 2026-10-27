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

When a paragraph's first line ends in a column break, the rest of that paragraph starts the next column with its space-before again ([COL-2](col#col-2)). Any other paragraph at a column top reached by flow drops its space-before.

### PARA-3. Space-after at the bottom {#para-3}

A paragraph's space-after never has to fit at the bottom of a page: a paragraph whose last line fits stays even when its space-after runs past the bottom ([PAGE-2](page#page-2)).

### PARA-4. Contextual spacing {#para-4}

Between two consecutive paragraphs **of the same paragraph style** that **both** have `w:contextualSpacing`, the gap is 0. Different styles keep the normal gap even when both are contextual; a contextual paragraph next to a non-contextual one keeps the normal gap. Inside table cells this applies too, except in compat ≤ 14 with `w:compat/w:allowSpaceOfSameStyleInTable`, where paragraphs in a cell always keep the normal gap.

### PARA-5. Automatic spacing {#para-5}

`w:beforeAutospacing` and `w:afterAutospacing` replace the written value with 14pt, and the result collapses like any other spacing ([PARA-1](para#para-1)). The automatic space-before is 0 on the document's first paragraph; on the first paragraph of a table cell, header, footer, note or text box it stays 14pt.

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
