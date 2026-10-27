---
title: Headers and footers
sidebar_position: 16
---

Each page shows at most one header and one footer, chosen by variant and inherited across sections. Lay them out as unpaginated stories at the page's text width, and their heights decide where the body starts and ends. Page-number fields make a header's content depend on the page it is on, so the header heights, and with them the page breaks, are settled by repeating the layout until they agree ([HDR-10](hdr#hdr-10)).

### HDR-1. Header position {#hdr-1}

Lay the header story out top-down, starting at `y = headerDist` (`w:pgMar/@w:header`) from the page top. Its first paragraph's space-before comes first ([HDR-3](hdr#hdr-3)), then its lines.

### HDR-2. Footer position {#hdr-2}

The footer's bottom, including its last paragraph's space-after, is at `pageHeight − footerDist` (`w:pgMar/@w:footer`). The footer grows upward: lay it out top-down from `pageHeight − footerDist − footerHeight`.

### HDR-3. Story height {#hdr-3}

A header, footer, cell, note or text box is a story laid out without pagination: paragraphs top-down with the normal spacing rules ([PARA-1](para#para-1)). Its height runs from the top of the first paragraph's space-before to the bottom of the last paragraph's space-after.

Example: one 11pt Calibri line (13.43pt) with 8pt space-after has height 21.43pt.

### HDR-4. Body top {#hdr-4}

```
bodyTop = max(topMargin, headerDist + headerHeight)
```

with no extra gap. `topMargin` includes a top gutter ([HDR-13](hdr#hdr-13)). A page with no header uses `topMargin`.

Example: top margin 72, headerDist 36, a 50pt header: bodyTop = max(72, 86) = 86.

### HDR-5. Negative top and bottom margins {#hdr-5}

A negative `w:top` means exactly `|top|`: `bodyTop = −top`, whatever the header's height. The header never pushes the body and may overlap it. A negative `w:bottom` works the same way: `bodyBottom = pageHeight + bottom`.

### HDR-6. Body bottom {#hdr-6}

```
bodyBottom = min(pageHeight − bottomMargin, pageHeight − footerDist − footerHeight)
```

A page with no footer uses `pageHeight − bottomMargin`.

### HDR-7. Which variant a page shows {#hdr-7}

- `w:titlePg` on: the section's first page (its first page that isn't an inserted blank page, [HDR-12](hdr#hdr-12)) shows `first`.
- Otherwise, with `w:evenAndOddHeaders` in the settings: pages with an even page number show `even`, odd ones `default`.
- Otherwise: `default`.

Choose headers and footers independently. Each page's body uses that page's own header and footer heights ([HDR-4](hdr#hdr-4), [HDR-6](hdr#hdr-6)).

### HDR-8. Height limits {#hdr-8}

- **Compat ≤ 14**: keep only the header's (footer's) lines whose bottom is within `pageHeight / 2` of the story's top. Drop the rest, whole lines only. The height is then the bottom of the last kept line.
- **Compat 15**: no cap for layout: the body starts below the whole header, and if nothing fits, [PAGE-5](page#page-5) applies.
- **Compat 15, very tall headers**: if `headerDist + headerHeight > 0.85 × pageHeight`, draw only the header lines whose top is above `0.435 × pageHeight`. The height used for the body stays the full height.

Example: Letter (792pt), compat 15, a 700pt header at headerDist 36: 736 > 673.2, so lines starting at or below y 344.52 aren't drawn.

### HDR-9. Text below the page isn't drawn {#hdr-9}

Don't draw a body glyph whose top (`baseline − fontSize`) is at or below the page bottom. Its line still takes its place in the flow. This happens when a header taller than the page pushes the text area below it and [PAGE-5](page#page-5) places a line there anyway: the page then shows no body text.

### HDR-10. Page fields and convergence {#hdr-10}

- `PAGE`, `NUMPAGES` and `SECTIONPAGES` in headers and footers take each page's values ([FIELD-1](field#field-1)). Lay the line out again with the result, so a right-aligned number keeps its right edge and centred text re-centres.
- A page's number: count pages from 1, blank pages included ([HDR-12](hdr#hdr-12)). `w:pgNumType/@w:start` sets the number of the section's first page. `@w:fmt` sets the format of the number ([FIELD-3](field#field-3)).
- A header's height can depend on its fields, and the body depends on the height. Lay out the document, compose every header and footer with the final values, and if any page's composed heights differ from the ones its body used (or the body's own page-dependent fields changed), lay out again with the new heights and values. Repeat up to four times.
- If the heights still disagree after four passes, lay out once more, reserving for each page the largest height any pass composed for it, and cut each header and footer to whole lines within its reservation.

### HDR-11. Tables in headers and footers {#hdr-11}

Tables in headers and footers follow the table rules ([TABLE-1](table#table-1)). A story never ends with a table in the file: the paragraph after it is laid out as any other and adds its line to the height. Footers still grow upward ([HDR-2](hdr#hdr-2)).

### HDR-12. Sections {#hdr-12}

- A variant (`default`, `first`, `even`) that a section doesn't reference is **inherited from the nearest earlier section that does**, per variant. A `titlePg` section with no `first` reference shows the previous section's `first`. If no section up to this one references the variant, the page has none.
- Lay out an inherited header with the **current** section's headerDist, margins and page size.
- A page's headers and footers come from the section the page **begins in**; a continuous break doesn't change them.
- A continuous section that starts mid-page uses its own left and right margins and columns at once, from where the previous section ended; its top margin and header apply from its next page.
- An `oddPage` or `evenPage` break inserts a blank page when the next page number has the wrong parity, unless the section restarts its numbering with `pgNumType/@w:start`. The blank page has **no header or footer** and no body, but it counts in the page numbering.
- Page numbers continue across sections unless `pgNumType/@w:start` restarts them.

### HDR-13. Gutter and mirror margins {#hdr-13}

Page parity comes from the page number ([HDR-10](hdr#hdr-10)).

- `w:gutterAtTop` (settings): add `w:gutter` to the top margin. Nothing is mirrored, even with `w:mirrorMargins`.
- Otherwise, `w:mirrorMargins`: on odd pages, left = `w:left` + gutter, right = `w:right`. On even pages swap them: left = `w:right`, right = `w:left` + gutter. `w:left` is the inside margin.
- Otherwise: add the gutter to the left margin.
- Headers and footers use the page's horizontal margins. `headerDist` is not shifted by a top gutter.

Letter, margins 72, gutter 36:

| Settings | Odd page text x | Even page text x | Body top |
|---|---|---|---|
| none | 72–540 | 72–540 | 72 |
| gutter | 108–540 | 108–540 | 72 |
| gutter + gutterAtTop | 72–540 | 72–540 | 108 |
| mirror, left 108, right 54 | 108–558 | 54–504 | 72 |
| mirror + gutter | 144–558 | 54–468 | 72 |
| mirror + gutter + gutterAtTop | 108–558 | 108–558 | 108 |
