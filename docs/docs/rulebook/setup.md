---
title: Page setup
sidebar_position: 20
---

A section's `w:sectPr` sets the page size and margins its pages use, and `w:vAlign` can move the body down the page once it is filled. Margin arithmetic with headers, footers and gutters is in [HDR-4](hdr#hdr-4) to [HDR-6](hdr#hdr-6) and [HDR-13](hdr#hdr-13).

### SETUP-1. Vertical alignment {#setup-1}

`w:sectPr/w:vAlign` (`top` by default) shifts a page's body after it is filled. Apply it on every page of the section, full pages and pages ending in a page break included, when:

- the section has one column,
- the page holds no other section (the body doesn't start after a continuous break, and no continuous section follows it on the page),
- and no footnote is drawn on the page.

Content runs from the first line box top to the last line box bottom, plus the last paragraph's space-after when that paragraph ends on this page. Body top and bottom come from [HDR-4](hdr#hdr-4) and [HDR-6](hdr#hdr-6); `free = (bodyBottom − bodyTop) − contentHeight`.

- `center`: move everything so the content top is at `bodyTop + free / 2`.
- `bottom`: move everything down so the content bottom is on `bodyBottom`.
- `both`: keep the first line where it is and divide `bodyBottom − contentBottom` equally among the gaps before each later paragraph (and table) start. Line pitch inside a paragraph doesn't change. A page with a single paragraph doesn't move.

Example: a 400pt body area holding 100pt of content in three paragraphs: `center` moves it down 150pt; `both` adds 150pt above the second paragraph and 300pt above the third (150pt per gap).

### SETUP-2. Page size and margins {#setup-2}

- The page is `w:pgSz/@w:w × @w:h` (twips; 12240 × 15840 = 612 × 792pt when absent). Ignore `w:orient`: a landscape page is one whose `w` is larger.
- Use `w:pgMar` as written, not rotated: `top`, `bottom`, `left`, `right` default to 1440 twips (72pt), `header` and `footer` to 720 (36pt), `gutter` to 0.
- Each section has its own size and margins. A page takes the size of the section it begins in.
