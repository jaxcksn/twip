---
title: Compatibility modes
sidebar_position: 18
---

A document's compatibility mode selects between two sets of layout behaviour: mode 15 and modes 14 and below. A few `w:compat` options change layout further. Rules elsewhere in the book that differ by mode say so; this page says how to find the mode and lists the differences in one place.

### COMPAT-1. The mode {#compat-1}

Read the mode from `w:settings/w:compat/w:compatSetting[@w:name="compatibilityMode"]/@w:val`. The values are 11, 12, 14 and 15. A document without that setting (or without a settings part) is mode **12**.

Apply mode-15 behaviour to values ≥ 15 and the ≤ 14 behaviour to all lower values; 11, 12 and 14 lay out alike.

### COMPAT-2. What differs between 15 and ≤ 14 {#compat-2}

| Behaviour | Mode 15 | Mode ≤ 14 | Rule |
|---|---|---|---|
| Justified line breaking | per-line choice with up to 25% space shrink | greedy, expand only | [BREAK-5](break#break-5), [BREAK-6](break#break-6) |
| Justified line ending in `w:br` | justified unless `doNotExpandShiftReturn` | not justified | [JUST-3](just#just-3) |
| Space-before of a `pageBreakBefore` paragraph at the page top | dropped | kept | [PARA-2](para#para-2) |
| Right, center and decimal tab stops past the right indent | text ends at the right indent | text aligns at the stop, past the margin | [TAB-7](tab#tab-7) |
| Table horizontal position | outer border edge at margin + `tblInd` | pulled left by `max(halfBorder, first cell's left margin)` | [TABLE-3](table#table-3) |
| Nested table position | outer border edge at the cell content left + `tblInd` | the same as mode 15 | [TABLE-25](table#table-25) |
| Header and footer height | no cap | capped at pageHeight / 2 | [HDR-8](hdr#hdr-8) |
| Footnotes in several columns | one stream per page, no continuation | per-column areas, continued in the next column | [COL-3](col#col-3), [COL-4](col#col-4) |
| Continuation separator | 144pt | the column width | [NOTE-3](note#note-3) |
| Floats anchored in cells | always in the cell | `layoutInCell="0"` places them like body floats | [FLOAT-14](float#float-14) |
| Multiple spacing on a line with a picture, paragraph's last line | the paragraph mark's height counts when taller than the text | the text's height (the mark's only on a line without text) | [LINE-13](line#line-13) |
| `compressPunctuation` with East Asian text: the text crossing the right edge | wraps | stays if the line's spaces can shrink enough | [BREAK-9](break#break-9) |
| Paragraph indents in a cell's autofit widths | left + right added to the minimum; the maximum as laid out | left + right + min(firstLine, 0) added to both | [TABLE-8](table#table-8) |
| A hyphen run ending its paragraph, in a cell's autofit minimum | breaks after each hyphen | kept whole | [TABLE-9](table#table-9) |

### COMPAT-3. w:compat options {#compat-3}

Only these options change layout; ignore every other child of `w:compat`.

| Option | Modes | Effect |
|---|---|---|
| `doNotExpandShiftReturn` | 15 | a justified line ending in `w:br` isn't justified ([JUST-3](just#just-3)) |
| `doNotUseHTMLParagraphAutoSpacing` | all | the gap between paragraphs is space-after + space-before, not the larger of the two ([PARA-1](para#para-1)) |
| `splitPgBreakAndParaMark` | ≤ 14 | the paragraph mark after a page break that ends its paragraph moves to the next page as an empty line |
| `allowSpaceOfSameStyleInTable` | ≤ 14 | contextual spacing isn't applied inside table cells ([PARA-4](para#para-4)) |
| `growAutofit` | all | an autofit table may grow wider than the available width |
| `balanceSingleByteDoubleByteWidth` | all | runs of two or more spaces widen to the East Asian font's average character width ([FONT-14](font#font-14)) |

`w:characterSpacingControl` with a `compressPunctuation` value (a settings element, not a `w:compat` child) changes East Asian line breaking ([BREAK-8](break#break-8), [BREAK-9](break#break-9)).
