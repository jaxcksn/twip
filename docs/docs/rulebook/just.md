---
title: Justification and alignment
sidebar_position: 8
---

Once a line's content is fixed ([BREAK-1](break#break-1) onward), place it horizontally by the paragraph's `w:jc`. Work on the line's content up to its last non-space piece: trailing spaces and the line's break hang and are left out ([BREAK-2](break#break-2)). `rightEdge` is the text width less the right indent; `lineEnd` is the x where the line's content ends, plus a hyphen's width when the line ends at a soft hyphen ([BREAK-7](break#break-7)). `both`, and the kashida and `thaiDistribute` values, justify; `end` aligns right.

### JUST-1. Expansion {#just-1}

For a justified line with `extra = rightEdge − lineEnd > 0`, share `extra` equally among the line's expandable spaces ([JUST-6](just#just-6)), not in proportion to their widths: each grows by `extra / N`. No-break spaces and letters keep their width.

Example: 3 spaces and 6pt to fill: each space grows by 2pt.

### JUST-2. Compression {#just-2}

Compat 15 only. When a justified line overflows (`overflow = lineEnd − rightEdge > 0`, which only [BREAK-6](break#break-6) produces), each expandable space `i` may shrink by at most `capᵢ = wᵢ / 4`:

```
C     = Σ capᵢ
use   = min(overflow, C)
q     = (C − use) / N
shrinkᵢ = capᵢ − q
```

If the overflow exceeds `C` the line still overflows.

Example: 4 spaces of 2.4868pt (cap 0.6217 each, C = 2.4868) and 1.2pt overflow: q = 1.2868 / 4 = 0.3217, each space shrinks by 0.3; the line ends exactly at the right edge.

### JUST-3. Which justified lines stretch {#just-3}

- A paragraph's last line isn't expanded; it aligns left. In compat 15 it is still compressed when it overflows ([JUST-2](just#just-2), [BREAK-6](break#break-6)).
- A line ending in a line break (`w:br`, `w:cr`) is justified in compat 15, unless `w:compat/w:doNotExpandShiftReturn` is set. In compat ≤ 14 it isn't justified.
- A line ending in a page or column break isn't justified.

### JUST-4. Centre and right {#just-4}

```
free = rightEdge − lineEnd
```

If `free > 0`, shift the whole line right by `free / 2` (centre) or `free` (right); otherwise leave it at the left indent. Every line of the paragraph is aligned this way, the last one too.

Example: a 200pt line in a 468pt column with no indents starts 134pt from the left edge centred, 268pt right-aligned.

A right-to-left paragraph (`w:bidi`) swaps left and right: `w:jc` absent, `left` or `start` aligns it right, `right` or `end` left. A justified (`both`) right-to-left paragraph's lines that don't stretch ([JUST-3](#just-3)), its last line among them, align right.

### JUST-5. Distribute {#just-5}

`w:jc="distribute"`: share `extra = rightEdge − lineEnd` equally among the line's characters (spaces included) and objects other than tabs, excluding the last one: each gains `extra / (n − 1)`. Every line is distributed, the last one too. Spaces don't shrink.

### JUST-6. Where justification starts {#just-6}

The expandable spaces of a line are the spaces after its last tab character (all of the line when it has none), skipping the spaces at the start of that stretch, and before its trailing spaces. Text up to and including the last tab keeps its position.

Example: "Name:⇥ Jane Q Public  " justifies only the two spaces between "Jane", "Q" and "Public".

### JUST-7. wpJustification {#just-7}

Compat ≤ 14 with `w:compat/w:wpJustification`, `w:jc="both"` paragraphs only: every line may run past its right edge by

```
X = round(W × 562 / 14400) twips
```

`W` is the text width less the left and right indents, in twips, at most 31680; the first-line indent isn't subtracted. Lines are broken against `rightEdge + X` ([BREAK-1](break#break-1)); a line that then overflows and is justified ([JUST-3](#just-3)) is squeezed back to `rightEdge`, the overflow shared equally among its expandable spaces ([JUST-6](#just-6)). Compat 15 ignores the option.

Example: a 6.5in column without indents gives X = 365 twips (18.25pt).
