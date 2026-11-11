# Layout feature fixtures

One document per layout feature of the roadmap's layout phase (chunks 1.2 to 1.11), each small and aimed so that
the feature moves something the layout snapshot (`tests/layout/layout__<name>.txt`) records: a line's start x,
baseline and text, a turned run's origin, direction and text, or a float or cell rectangle, all in twips. A feature
that lays out differently in compat ≤ 14 and compat 15 comes as a pair with the same body, `-c12` / `-c15` (or `-c14`
/ `-c15`); diffing the pair's snapshots shows the mode difference.

`make-layout-fixtures.py` writes every file here: `python -I crates/twip/tests/fixtures/layout/make-layout-fixtures.py`,
then `TWIP_WRITE_LAYOUT=1 cargo test -p twip --lib layout_output` writes the snapshots. Standard library only, the
same bytes on every run. Fonts are Calibri, Times New Roman and MS Mincho; the gate draws them with its bundled
stand-ins (the CJK stand-in has only 漢, so that is the only CJK character used). `hyph-en-US.dic` is a small made-up
pattern set the gate supplies as the host's en-US hyphenation patterns, so the documents that turn automatic
hyphenation on hyphenate in the gate.

## 1.2 Paragraph and table geometry

| Fixture | Bullet | What the snapshot shows |
|---|---|---|
| `para-borders` | Paragraph top and bottom borders add their space and width; bordered paragraphs group and split | Baselines: a bordered paragraph's first line lower by width + space, no extra space inside a group of identical borders, a new box where the indent differs, the between border's space, a double border's 3×, the shadow's extra width; the box left open across the page break (page 2 starts with no top space) |
| `border-space` | Border `w:space` offsets for paragraph and page border lines | Baselines around paragraphs with `w:space` 0, 1, 4, 24, 31, 32 (0), 40 (8), 60 (28), 255 (31). Page border lines aren't in the snapshot and don't move text: the two page-border sections show only that the text stays put |
| `table-geometry-c12`, `-c15` | Table horizontal geometry in integer twips | Cell rectangles and cell text x: compat 15 puts the outer border edge on the margin, compat 12 pulls the table left; thick borders, wide margins, an indent, odd grid widths, a cell's own border, centred and right tables, a nested table |
| `html-divs` | HTML divisions (`divId`, margins and borders from `webSettings`) | Line x from the divisions' left margins (nested division adds its own), baselines from the top margins, borders and spaces; an unknown id changes nothing |
| `decimal-tab` | Decimal tabs align on `.` whatever `decimalSymbol` says | Line start x of each number: aligned on the period (or the number's end), the comma ignored |

## 1.3 Float positioning

| Fixture | Bullet | What the snapshot shows |
|---|---|---|
| `float-sources-c12`, `-c15` | One float input for every source | Float rectangles and the wrapped lines beside a picture, a shape, a text box, a group, a canvas, a chart frame and a VML shape |
| `float-twips` | Float rectangles in integer twips | Float rectangles from EMU offsets and sizes that aren't whole twips (offsets truncated), and a VML float in fractional points |
| `float-margin-areas-c14`, `-c15` | Positions relative to margin areas | Float rectangles against the left, right, top and bottom margin areas, inside and outside (both pages: they swap), `page` left / right (compat 14 puts them in the margin area) and `column` left / right in an indented paragraph (compat 14 counts from the indents) |
| `float-columns-c14`, `-c15` | Column containment, and moving the anchor line | Two-column pages: a tall float near the column bottom (compat 15 moves it up or moves its paragraph to the next column), a float that reaches back over the first column, a float taller than the rest of the column |
| `float-story-areas` | Obstacle registries per page area | Header text wraps the header float but not a body float drawn over the header area; footer text beside the footer's float; the body wraps its own float |
| `header-float-wrap` | Header and footer floats that wrap body text | Body lines on every page cut by the header's square float, pushed past the footer's top-and-bottom float, untouched by the header's wrapNone shape |

## 1.4 Float sizing and wrapping

| Fixture | Bullet | What the snapshot shows |
|---|---|---|
| `float-relative-size` | Relative size and position (`wp14:sizeRelH/V`, `pctPosHOffset`, VML percentages) | Float rectangles sized by a share of the margin, page and left margin, a picture keeping its aspect ratio, a box at 25% of the margin, a VML box by `mso-width-percent` / `mso-left-percent` |
| `wrap-polygon-c14`, `-c15` | Tight and through wrapping around the `wrapPolygon` contour | Line start x stepping along a triangle's diagonal, lines reaching into a notched polygon, a rectangle with 18pt top and bottom distances (compat 14 drops them: the lines above and below move) |
| `clear-break-c14`, `-c15` | Text-wrapping breaks (`w:br` with `clear`) | Baselines of the text after `clear="left"`, `"right"` and `"all"` breaks below the floats; a centred float with the break beside it, where the modes clear differently; a plain break for comparison |
| `suppress-overlap-c14`, `-c15` | `framePr@suppressOverlap` and `textboxTightWrap` | Frame rectangles (a `suppressOverlap` frame moved off the first, two frames without it overlapping), two `allowOverlap="0"` pictures, and body lines wrapped around a bare text box's own lines, the `textboxTightWrap` ones around their ink |

## 1.5 Text box autofit and columns

| Fixture | Bullet | What the snapshot shows |
|---|---|---|
| `textbox-autofit-columns` | `a:normAutofit` and `bodyPr@numCol` read and kept, no layout effect | The text box lines at full size and spacing despite `fontScale` / `lnSpcReduction`, and in one column despite `numCol="2"` |

## 1.6 Table rows and exceptions

| Fixture | Bullet | What the snapshot shows |
|---|---|---|
| `tblprex-c12`, `-c15` | Table property exceptions (`w:tblPrEx`) | Cell rectangles and text x per row: its own borders, cell margins, `jc` (compat 12 centres the row, compat 15 doesn't), cell spacing; an autofit table whose row indent exception counts only with fixed layout |
| `hidden-rows-c12`, `-c15` | Hidden rows | Cell rectangles of hidden middle, first and last rows gone; after a hidden last row compat 15 still reserves the bottom border (the next paragraph lower); a row with only its row mark hidden stays |
| `hidemark` | Empty cells marked `hideMark` collapse | Cell rectangles: a row of empty `hideMark` cells as tall as its borders, the same row unmarked one line tall, a marked cell's empty last paragraph taking no line, an exact row keeping its height |

## 1.7 Notes, sections and page numbers

| Fixture | Bullet | What the snapshot shows |
|---|---|---|
| `note-props-c12`, `-c15` | Footnote and endnote properties in sections | Footnotes beneath the text in section 1 (lower-roman from iii), section-end endnotes (upper-roman), section 2's upper-letter footnotes restarting on each page and lower-letter endnotes from e restarting on each page: note lines' text and baselines |
| `no-endnote` | `noEndnote` sections | Endnote lines: section 1's endnote after section 2, section 3 (noEndnote, the last) still taking its own at the end |
| `mirror-indents-c12`, `-c15` | Mirror indents on facing pages | Line start x on three pages: the mirrored paragraphs swap indents on odd pages in compat 12, even pages in compat 15; the hanging one changes its line width; the plain one never moves |
| `book-fold` | Book fold printing | Line start x: landscape odd pages start at right margin + gutter, even pages at the left margin; the portrait section drops the gutter |
| `chapter-page-numbers` | Chapter page numbers (`chapStyle`, `chapSep`) and `w:pgNum` | Line text: `1-1` before the first heading (the first heading's number), `2-1` after "2. Beta", the footers' `2-1` and `2-2` (the page's last heading), `3—1` with section 2's em-dash separator and restart |

## 1.8 Missing line content

| Fixture | Bullet | What the snapshot shows |
|---|---|---|
| `ruby` | Ruby base text and annotations | The annotation lines above their bases, the base text in the line, a taller line pitch, a long annotation widening its base |
| `legacy-list` | `w:legacy` list levels | Line start x of the centred and right-aligned legacy numbers, and the first line's break (the fixed gap after the number) |
| `fit-text-c12`, `-c15` | `w:fitText` | Line breaks around expanded and squeezed regions: compat 15 breaks with the stale widths (the document's first line, the squeezed and expanded regions on narrow lines) |
| `spec-vanish-c12`, `-c15` | `specVanish`, the hidden paragraph mark of style separators | The heading and the next paragraph on one line; compat 15 keeps the heading's centring and indents on every line, compat 12 the next paragraph's on its lines; `specVanish` without `vanish` ends its paragraph |
| `date-time-elements` | The run date and time elements | Line text at the gate's pinned clock (2026-11-09): `09`, `Monday`, `11`, `November`, `26`, `2026`, and a line broken by their widths |
| `number-forms` | `w14:numForm`, `w14:numSpacing`, `w14:stylisticSets` | Nothing yet: the stand-in fonts have no `onum`, `pnum`, `tnum` or `ssNN` glyphs, so the runs measure as without the properties. The document pins that they still lay out and is ready for a font that has the features |

## 1.10 Automatic hyphenation

| Fixture | Bullet | What the snapshot shows |
|---|---|---|
| `hyphenation-c12`, `-c15` | A hyphenator, with `autoHyphenation`, `hyphenationZone`, `consecutiveHyphenLimit`, `doNotHyphenateCaps`; patterns supplied by the host | Line text ending in hyphens (no more than two in a row), capitals and a `noProof` word not hyphenated, compat 12's 36pt zone, and a page of justified lines whose last line compat 15 doesn't hyphenate |
| `hyphenation-suppress` | Per-paragraph `suppressAutoHyphens` | The same text twice: hyphenated, then not |

Hyphenation settings through the API have no document: the session's hyphenation API is tested in `src/api`.
These documents can't match Word's own layout outside the gate: Word hyphenates with its own patterns, not
`hyph-en-US.dic`.

## 1.11 Vertical text

| Fixture | Bullet | What the snapshot shows |
|---|---|---|
| `vertical-directions` | `textDirection` in `sectPr` and `pPr` (`tbRl`, `btLr`, `lrTbV`, `tbRlV`, `tbLrV`) | Turned runs of one section per direction (origin and direction of each), a frame paragraph's own `tbRl`, and a body paragraph whose `tbRl` is ignored (an ordinary line) |
| `vertical-page` | Lines flow across the page, margins, columns, headers and footers turned | Turned runs of a two-column `tbRl` section with uneven margins, a gutter, a header and a footer |
| `vertical-glyphs` | Latin text turned, East Asian characters upright | Turned runs: Latin runs turned with the line, each 漢 its own upright run, in `tbRl` and `tbLrV` |
| `vertical-model-c12`, `-c15` | One vertical model for sections, paragraphs, cells and text boxes | Turned runs and cell rectangles of `tbRl`, `btLr`, `tbRlV` and `tbLrV` cells in an exact row (compat 12 aligns the block into the far margin), a vertical cell in an auto row, and `vert` / `vert270` text boxes |
