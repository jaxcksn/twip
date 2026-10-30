---
title: Frames
sidebar_position: 14
---

A body paragraph with `w:pPr/w:framePr` is laid out in a frame: a text box positioned like a float, which the following text wraps around. Turn each run of framed paragraphs into one float anchored at the start of the next ordinary paragraph, then lay it out with the float rules ([FLOAT-6](float#float-6) to [FLOAT-12](float#float-12)). Drop caps are frames too.

### FRAME-1. Grouping and geometry {#frame-1}

- Consecutive paragraphs whose `w:framePr` attributes are **identical** share one frame. Any difference, even 1 twip, starts a separate frame.
- **Width**: `w:w` if present and positive. Without it, the frame is as wide as its content: lay its paragraphs out at the column width and take the right edge of the widest line, trailing spaces excluded, at most the column width. A frame in a header or footer is sized the same way, with the values its fields show on that page (a framed `PAGE` reading `10` is two digits wide).
- **Height**: the content height, including the paragraphs' spacing ([HDR-3](hdr#hdr-3)). `w:h` with `w:hRule="exact"` fixes the height at `h`; `w:h` with `atLeast` or no `hRule` is a minimum: `max(h, content)`.
- **Horizontal**: `w:hAnchor="margin"` or `"page"` counts from that box; `"text"` (the default) from the column's text edge, not the next paragraph's indent. Then `w:x` (twips) or `w:xAlign` (left, center, right, inside, outside, as [FLOAT-6](float#float-6)).
- **Vertical**: `w:vAnchor="margin"` or `"page"` counts `w:y` from that box's top. `"text"` (the default) counts from where the frame paragraph would start in the flow, which is the top of the paragraph after it. `w:yAlign` places the frame inside the box; `inline` is ignored (`w:y` applies).
- The frame takes **no space in the flow**: the next paragraph starts where the frame paragraph would have started and wraps beside the frame ([FRAME-3](frame#frame-3)).
- Frames never avoid each other: they may overlap.

### FRAME-2. Where a frame goes {#frame-2}

- A frame followed by an ordinary paragraph is a float anchored at the start of that paragraph. Several frames in a row before the same paragraph all anchor there, in order.
- A frame followed by a table, or at the end of the body, isn't a frame: lay its paragraphs out in the flow as ordinary paragraphs. A frame before a table therefore pushes the table down.

### FRAME-3. Frame wrap {#frame-3}

| `w:wrap` | Wrap |
|---|---|
| `around`, `auto`, absent, anything else | square, both sides ([FLOAT-7](float#float-7)) |
| `notBeside` | top and bottom ([FLOAT-8](float#float-8)) |
| `none` | no wrap: text overlaps the frame |

The exclusion is the frame grown by `w:hSpace` left and right and `w:vSpace` top and bottom. `vSpace` above the frame can wrap the line before it.

Paragraph borders widen the frame **to the right only**: add `space + width` of the left border and `space + width` of the right border to the frame's right edge. The text stays at the frame's x and keeps its width.

Example: `w:w` 2880 (144pt), left and right borders of 0.5pt at 4pt space: the text starts at x and is laid out 144pt wide; the frame, and its exclusion, is 153pt wide.

### FRAME-4. Drop caps {#frame-4}

`w:framePr w:dropCap="drop"|"margin" w:lines="N"` marks a drop cap. The letter is stored as its own frame paragraph, with exact line spacing, an explicit `w:sz` and a negative `w:position`. Lay it out with those stored values as any frame: it is about N body lines tall, the letter's advance (plus `hSpace`) wide, and the next paragraph's first lines wrap beside it ([FRAME-1](frame#frame-1), [FRAME-3](frame#frame-3)).

`dropCap="margin"`: put the frame in the left margin, its right edge on the margin edge (horizontal box `[0, leftMargin)`, aligned right). The text isn't indented.

### FRAME-5. Drop-cap baseline {#frame-5}

The letter's baseline follows from the stored values: `frameTop + 0.8 × exactLine` ([LINE-10](line#line-10)), lowered by the run's `w:position` ([FONT-7](font#font-7)). With the frame at the anchor paragraph's top (`vAnchor="text"`, `y` 0):

```
baseline = paragraphTop + 0.8 × exactLine + |position| / 2
```

where `position` is in half-points. Example: exact line 40.5pt (810 twips), `w:position="-10"`: baseline = top + 32.4 + 5 = top + 37.4.
