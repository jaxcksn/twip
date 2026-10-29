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

When the pen is left of the left indent and the chosen stop (custom or default) is beyond it, the tab goes to the left indent instead, as a left stop without a leader. Default stops before the left indent don't count: with no custom stop past the pen, the tab goes to the left indent. This is what puts the text after a list number, or after a hanging first line's tab, at the indent ([LIST-2](list#list-2)).

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

- **Compat 15, a right, centre or decimal stop** past `rightEdge` whose text runs to the end of the paragraph (no later tab): the stop moves to `rightEdge`. Right-tab text then ends at `rightEdge`; decimal-tab text too, through [TAB-6](tab#tab-6); centre-tab text also *ends* at `rightEdge` (it works as a right tab there) and doesn't wrap.
- **Compat 15, any other custom stop past `rightEdge`** (a left stop, or a right, centre or decimal one whose text another tab follows): the tab ends the line before it and starts the next line. A tab first on its line stays, but the text after it can't fit and starts the next line at the left edge, so each such tab costs a line.
- **Compat ≤ 14**: custom stops are used where they are. Text after a right or decimal stop past `rightEdge` aligns at the stop and may run past the margin, even off the page; after a right stop the fit test uses the stop as the line's limit. Once a tab has gone to a custom stop past `rightEdge` (left, centre or right), the rest of the line doesn't wrap: text after a left stop starts at it, text after a centre stop is centred on it, and later default stops are used where they are, on the same line, even off the page.
- Text after a centre stop at the line's limit can't fit: half of it would pass the limit ([TAB-5](tab#tab-5)'s fit test), so the line ends after the tab and the text wraps to the next line. A centre stop past `rightEdge` follows the bullets above instead.
- **Default stops, every mode**: a tab whose stop is a default stop past `rightEdge` ends the line before it, and starts the next line, where it goes to the next stop from the line's start ([TAB-1](tab#tab-1), [TAB-2](tab#tab-2)). This also holds when trailing spaces have already carried the pen past the edge ([BREAK-2](break#break-2)), but not in compat ≤ 14 on a line where an earlier tab went to a custom stop past `rightEdge` (above). A tab first on its line always stays.

Example (450pt column): "Aa", a tab to a right stop at 480, "Bb": compat 14 ends "Bb" at 480, compat 15 at 450. "A", a tab to a left stop at 480, "Bb", a tab, "Cc": compat 14 keeps one line, "Bb" at 480 and "Cc" at the default stop 504; compat 15 puts the tab alone on line 2, "Bb" on line 3 at 0, the second tab alone on line 4 and "Cc" on line 5 at 0.

Example: "Signed this the 15th day of January, 2026." followed by twelve tabs in a 450pt column with 36pt default stops: the tabs up to 432pt stay on the line, the rest start a second, empty-looking line, so the next paragraph is one line lower.

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
