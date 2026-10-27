---
title: Tab stops
sidebar_position: 9
---

A tab character moves the pen to a tab stop. Stop positions count from the text area's left edge (the column, or the cell's content box), not from the indent. The paragraph's stops are the accumulated custom stops of its cascade ([STYLE-9](style#style-9)), sorted by position; `w:defaultTabStop` (720 twips, 36pt, when absent or not positive) sets the default grid. `rightEdge` is the text width less the right indent.

### TAB-1. The next custom stop {#tab-1}

The tab goes to the first custom stop whose position is greater than the pen, ignoring `bar` and `clear` stops. A `num` stop acts as a left stop.

### TAB-2. The default grid {#tab-2}

With no custom stop past the pen, the tab goes to the next multiple of the default tab stop strictly greater than the pen, as a left stop without a leader:

```
stop = (⌊pen / inc⌋ + 1) · inc
```

Example: pen at 36pt with a 36pt grid: the stop is 72pt, not 36pt.

### TAB-3. The left indent acts as a stop {#tab-3}

When the pen is left of the left indent and the chosen stop (custom or default) is beyond it, the tab goes to the left indent instead, as a left stop without a leader. This is what puts the text after a list number, or after a hanging first line's tab, at the indent ([LIST-2](list#list-2)).

### TAB-4. Left tabs {#tab-4}

`tabWidth = stop − pen`.

### TAB-5. Right, centre and decimal tabs {#tab-5}

These tabs wait for the text after them, up to the next tab, the line's break or the line's end. With `w` that text's width and `trail` its trailing spaces:

| Kind | `d` |
|---|---|
| right | `w − trail` |
| centre | `(w − trail) / 2` |
| decimal | the width up to the first `.` or `,`; `w − trail` if there is none |

```
tabWidth = (stop − penAtTab) − d
```

While the text is pending, test each character's fit ([BREAK-1](break#break-1)) at its aligned position: the text's right edge is `max(penAtTab + w, stop)` after a right tab and `max(penAtTab + w, stop + w / 2)` after a centre tab.

Example: a right stop at 6in (432pt) and "Page 12" of width 34.5 after a pen at 120: the tab is 432 − 120 − 34.5 = 277.5pt wide.

### TAB-6. Narrowing at the right edge {#tab-6}

When the stop is at or before `rightEdge` but the aligned text would end past it, narrow the tab so the text ends at `rightEdge`:

```
tabWidth = rightEdge − (penAtTab + w) + trail
```

### TAB-7. Stops past the right edge {#tab-7}

- **Compat 15**: a right, centre or decimal stop past `rightEdge` moves to `rightEdge`, keeping its kind. Right-tab text then ends at `rightEdge`; decimal-tab text too, through [TAB-6](tab#tab-6).
- **Compat ≤ 14**: the stop is used where it is. Text after a right or decimal stop past `rightEdge` aligns at the stop and may run past the margin, even off the page; after a right or centre stop the fit test uses the stop as the line's limit.
- In either mode, text after a centre stop at the line's limit can't fit: half of it would pass the limit ([TAB-5](tab#tab-5)'s fit test), so the line ends after the tab and the text wraps to the next line.
- Left stops and default stops are used where they are in every mode.

### TAB-8. No negative tabs {#tab-8}

A computed tab width below 0 becomes 0.

### TAB-9. Leaders {#tab-9}

`w:leader` fills the tab's gap: `dot` `.`, `hyphen` `-`, `underscore` and `heavy` `_`, `middleDot` `·`.

- Use the tab character's run font and size, and that run's raise.
- The grid pitch `p` is the leader glyph's advance rounded to whole 1/600 in units: `p = round(advance × 600 / 72) × 72 / 600`.
- Glyph k sits at `x = k · p`, counted from the page's left edge for body text, or from the left edge of the box the text is laid out in for table cells, headers, footers, notes and text boxes. So leaders line up between lines whatever the indents and text lengths.
- Draw whole glyphs only: the first in the first slot at or after the tab's start, the last one ending at or before the tab's end.
- Left tabs take leaders too. A tab that falls back to the left indent ([TAB-3](tab#tab-3)) or the default grid ([TAB-2](tab#tab-2)) has none.

Example: Calibri 11 "." has an advance of 2.776pt, so the pitch is round(23.13) / 600 in = 2.76pt.

### TAB-10. Positional tabs {#tab-10}

A positional tab (`w:ptab`) goes to an absolute position, whatever custom or default stops it passes (no [TAB-1](tab#tab-1) to [TAB-3](tab#tab-3), no [TAB-7](tab#tab-7)). `w:relativeTo` picks the span: `margin` (the default) is the text area, `0` to the text width; `indent` runs from the left indent to the text width less the right indent. `w:alignment` puts the stop at the span's left edge (`left`), its middle (`center`) or its right edge (`right`), and the tab then works as a left, centre or right tab at that stop ([TAB-4](tab#tab-4), [TAB-5](tab#tab-5)). `w:leader` (`dot`, `hyphen`, `underscore`, `middleDot`; `none` by default) fills its gap as in [TAB-9](tab#tab-9).

Example: a header line "Title" then `w:ptab w:relativeTo="margin" w:alignment="right"` and a page number: the number ends at the right margin, whatever the paragraph's stops and indents.
