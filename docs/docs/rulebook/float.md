---
title: Floating objects
sidebar_position: 13
---

Floating pictures (`pic:pic` in `wp:anchor`) and text boxes (`wps:wsp` with `wps:txbx`) are positioned against the page, and the body text wraps around them. First place each float from its anchor ([FLOAT-6](float#float-6)), then cut every line band into the free intervals beside the floats it overlaps ([FLOAT-2](float#float-2)), then fill those intervals. A float can re-wrap text placed before it on the page, so fill the page again until the set of floats and their positions stop changing ([FLOAT-11](float#float-11)). Frames (`w:framePr`) become floats too ([FRAME-1](frame#frame-1)).

### FLOAT-1. Half-open overlap {#float-1}

Every overlap test uses half-open intervals: `[a0, a1)` and `[b0, b1)` overlap only if `a0 < b1` and `b0 < a1`. Edges that touch don't overlap: a line whose top equals an exclusion's bottom isn't wrapped.

### FLOAT-2. Free intervals {#float-2}

For a line band `[top, top + height)` in a column `[x0, x1)`, start with one free interval `[x0, x1)`. For every wrapping float whose exclusion ([FLOAT-7](float#float-7)) overlaps the band vertically and the column horizontally, remove the blocked part from every interval ([FLOAT-3](float#float-3)). What remains, left to right, are the intervals the line is filled into. Test the band first with the paragraph's estimated line height, then again with the real height of the line that fills it.

A paragraph that ends at or above a wrapping float's exclusion top (its gap and its unwrapped lines all above it, [FLOAT-1](float#float-1)), and holds no anchor, doesn't wrap: lay it out as if the float weren't there. It keeps its borders and shading ([PARA-7](para#para-7)), which a paragraph wrapped around a float doesn't draw.

### FLOAT-3. Wrap side {#float-3}

`wp:wrapSquare/@wrapText` (and the same attribute on tight and through) decides which side of a float `[l, r)` stays free:

| wrapText | Blocked part |
|---|---|
| `bothSides` (default) | `[l, r)`: text on both sides |
| `left` | `[l, ∞)`: text only left of the float |
| `right` | `(−∞, r)`: text only right of the float |
| `largest` | the side with less space: keep the left if `l − x0 ≥ x1 − r`, else the right (a tie goes left) |

`wrapNone` doesn't block anything; `topAndBottom` blocks the whole band ([FLOAT-8](float#float-8)).

Example: a float at 222–322 in a column 72–540 with `largest`: 150pt on the left, 218pt on the right, so text goes right.

### FLOAT-4. A blocked band moves down {#float-4}

If no free interval in the band can take the next unbreakable piece of text, or a topAndBottom float covers the band, move the band down to the nearest exclusion bottom below its top among the floats overlapping it, and try again. Never move it below the column bottom. A band that has reached the column bottom ignores the floats: set it at the bottom, over them.

**Compat ≤ 14, beside `wrapSquare` floats**, a line whose content fits no interval doesn't jump to the exclusion bottom. It retries lower in fixed steps of a trial height `h` (`y += h`) until the line at `y` overlaps no float or its content fits beside it (the same test as everywhere else). The line placed at the final `y` gets its normal height.

- `h` is the height of what was placed when the fit failed. Tabs alone before the item that fits no gap don't make the line fit: `h` is their height. If nothing was placed, `h` is the failing item's own height: an inline picture's height alone (no font descent), or for text the line height (`A + E + D`) of the font where the gap's width ends inside the word (a word that starts at 11pt and turns 24pt after that width steps by the 11pt height).
- On the paragraph's first line `h` also includes the space before it, which moves with every step.
- A paragraph whose only content is one inline picture wider than the gap is placed in the gap anyway, over the float.

Example: a tab, then a 64pt-wide 30pt-high picture, 14.65 below the top of a 112.4pt float with 45pt gaps: the tab (11pt Calibri, 13.43) fits, the picture fits no gap, so the line moves 8 × 13.43 = 107.4 down. Compat 15 moves the line straight to the exclusion bottom, as above; so do tight and through floats, which haven't been checked.

### FLOAT-5. Inline pictures {#float-5}

An inline picture (`wp:inline`) is a glyph whose bottom sits on the baseline: ascent = picture height, descent 0. The line height follows [LINE-1](line#line-1): `max(pictureHeight, textAscent) + textDescent`. With exact line spacing the line keeps its height ([LINE-12](line#line-12)) and the picture overflows upward, unclipped.

### FLOAT-6. Anchor position {#float-6}

**Horizontal** (`wp:positionH/@relativeFrom`), giving a box `[x0, x0 + w)`:

| relativeFrom | Box |
|---|---|
| `page` | `[0, pageWidth)` |
| `margin` | `[leftMargin, pageWidth − rightMargin)` |
| `leftMargin` | `[0, leftMargin)` |
| `rightMargin` | `[pageWidth − rightMargin, pageWidth)` |
| `column` and everything else (`character`, `insideMargin`, `outsideMargin`) | the column holding the anchor |

Margins are the page's after gutter and mirroring ([HDR-13](hdr#hdr-13)). Then `wp:align`: `center` → `x0 + (w − floatW)/2`; `right` or `outside` → `x0 + w − floatW`; `left`, `inside` → `x0`. Without align, `x0 + wp:posOffset`.

**Vertical** (`wp:positionV/@relativeFrom`):

| relativeFrom | Box |
|---|---|
| `paragraph` (and anything not listed) | from the top of the anchor paragraph's first line box (on this page) |
| `line` | from the top of the line holding the anchor |
| `page` | `[0, pageHeight)` |
| `margin` | `[topMargin, pageHeight − bottomMargin)` |
| `topMargin` | `[0, topMargin)` |
| `bottomMargin` | `[pageHeight − bottomMargin, pageHeight)` |

`wp:align`: `center` → `y0 + (h − floatH)/2`; `bottom` or `outside` → `y0 + h − floatH`; others → `y0`. Without align, `y0 + posOffset`. Offsets may be negative.

**Offsets are truncated to whole twips**: `twips = trunc(EMU / 635)`, `points = twips / 20`. 170688 EMU = 268.8 twips → 268 → 13.40pt.

Floats anchored in table cells: [FLOAT-14](float#float-14).

### FLOAT-7. Square wrap {#float-7}

- The exclusion is the float's box grown by `distT`, `distB`, `distL`, `distR`. A text box's outline width isn't added.
- A line wraps if its line box overlaps the exclusion ([FLOAT-1](float#float-1)).
- Fill the free intervals of a band one after another, each **independently, one unbreakable piece of text at a time** (up to the next break opportunity). If the next piece doesn't fit the current interval, that interval stays as it is and the text continues in the next interval of the same band. There is no minimum interval width.
- Several floats give several intervals.
- An interval that starts at a float's edge (not the column's) takes the place of the left indent: its lines start at `max(edge, left indent)`, and the first line adds its first-line indent to that, so a hanging first line starts left of the edge, over the float's distance or the float itself.

Example: a float's exclusion ending at 255pt beside a paragraph with `w:left="-540" w:firstLine="540"`: the first line starts at 282pt, the next ones at 255pt. With `w:hanging="540"` instead the first line starts at 228pt.

### FLOAT-8. Other wrap types {#float-8}

- `wrapTopAndBottom`: a band that overlaps the exclusion takes no text; it moves down to the exclusion bottom, `floatBottom + distB` ([FLOAT-4](float#float-4)).
- `wrapTight` and `wrapThrough` wrap like `wrapSquare` (the polygon is ignored).
- `wrapNone`, and any float with `behindDoc="1"`, doesn't affect text.

### FLOAT-9. Floats past the column bottom {#float-9}

For a wrapping float positioned relative to `paragraph` or `line` whose bottom is below the column bottom:

1. Move it up so its bottom is on the column bottom, if its top then stays at or below the bottom of the anchor's line box.
2. Otherwise, if the anchor paragraph isn't at the top of an empty column, move the whole anchor paragraph to the next column (or page); the float is placed again from there.
3. Otherwise (nowhere to move it), leave the float where it was requested, let its own paragraph ignore it (no wrap), and continue the anchor paragraph after the anchor's line in the next column.

Floats positioned relative to the page or margins, and wrapNone floats ([FLOAT-13](float#float-13)), are never moved: they may run past the page edge, and text wraps around the part on the page.

### FLOAT-10. Text boxes {#float-10}

- Insets `bodyPr/@lIns`, `tIns`, `rIns`, `bIns` default to 7.2, 3.6, 7.2, 3.6pt. The outline (`a:ln/@w`, 0.75pt when `a:ln` has no `w`; none without `a:ln` or with `a:noFill`) is centred on the box edge.
- Lay the content out as an ordinary story ([HDR-3](hdr#hdr-3)) at `width = boxW − lIns − rIns − outline`, starting at `(boxX + lIns + outline/2, boxY + tIns + outline/2)`.
- `a:spAutoFit`: the box height is `content + tIns + bIns + outline`.
- Fixed height: the content bottom is `boxBottom − bIns − outline/2`. Draw a line only if its line box starts above the content bottom; draw no later line. The content bottom clips the lines drawn like an exact row's bottom ([TABLE-34](table#table-34)): a glyph whose ink top is at or below it is not drawn (its characters go with it); a line whose glyphs all go shows nothing.
- Text wraps around the outer box (with its dist values).

### FLOAT-11. Floats and columns {#float-11}

A float wraps the text of **every column it overlaps**, including earlier columns of the page: a float anchored in column 2 can re-wrap column 1. Only columns it overlaps horizontally are affected, also for topAndBottom. Fill the page, collect the floats it placed, and fill it again around them; repeat until the floats and their positions are the same as the ones the fill started from.

### FLOAT-12. allowOverlap="0" {#float-12}

- Two floats avoid each other if **either** has `allowOverlap="0"`. Wrapping floats avoid wrapping floats; wrapNone floats avoid only wrapNone floats.
- Test the bare float rectangles (dist ignored), half-open.
- The float **later in document order** moves. Against the float it overlaps (the obstacle), try in order, first that fits wins:
  1. **right**: left edge = obstacle's right edge, same y; allowed while its right edge ≤ the page's right edge;
  2. **left**: right edge = obstacle's left edge, same y; allowed while its left edge ≥ 0;
  3. **down**: top = obstacle's bottom, at the **original** x; allowed while its bottom ≤ the column bottom.
- After a move, test again against all placed floats and repeat against the float now hit.
- If no candidate fits, put the float back at its requested position (overlapping) and continue the anchor paragraph after the line holding its anchor in the next column.
- Text wraps around the final positions.

### FLOAT-13. wrapNone floats past the page bottom {#float-13}

A wrapNone float (in front of or behind the text) is never moved by [FLOAT-9](float#float-9): it stays where its position puts it, past the margin or the page edge, and its anchor paragraph stays too.

### FLOAT-14. Floats anchored in table cells {#float-14}

- Floats anchored in a cell are drawn whatever their wrap.
- The cell's vertical alignment offset ([TABLE-30](table#table-30)) moves the floats laid out in the cell with its text: `paragraph` and `line` count from the anchor where the aligned cell puts it. Their bottoms count in the content height that offset aligns, but they don't make the row taller. A compat ≤ 14 `layoutInCell="0"` float isn't moved by the offset: it is placed as in a top-aligned cell (below).
- Position them inside the cell: every horizontal box (`column`, `margin`, `page`, …) is the cell's content box; `paragraph` and `line` count from the anchor paragraph's and line's tops in the cell; `page` and `margin` vertically count from the top of the cell's content.
- Compat 15 ignores `layoutInCell`: the float stays in the cell.
- Compat ≤ 14 with `layoutInCell="0"` (in a cell of a body table, not a nested one): place the float like a body float horizontally and for `page`/`margin` vertically. For `paragraph`/`line` keep its position in the cell, moved up by the row's top border height (the offsets count from the row top, not from the cell's content).
