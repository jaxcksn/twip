---
title: Line breaking
sidebar_position: 7
---

These rules decide where each line of a paragraph ends. Shape the paragraph first ([FONT-1](font#font-1) onward) into pieces, one per glyph cluster, tab, break or inline object, each with its width. Then fill lines from the left indent (plus the first-line indent on the first line) to the right edge, `textWidth − rightIndent`. Left, centred and right-aligned text, and justified text in compat ≤ 14, are broken greedily ([BREAK-5](break#break-5)); justified text in compat 15 chooses its line ends per line ([BREAK-6](break#break-6)). In what follows, "spaces" are U+0020, U+3000, U+2002 and U+2003; a no-break space is ordinary text.

### BREAK-1. The fit test {#break-1}

A character or object fits when its right edge is at or left of the line's right edge: `pen + width ≤ rightEdge`. Equality fits. Spaces are never tested ([BREAK-2](break#break-2)). Text after a pending right or centre tab is tested where its alignment puts it ([TAB-5](tab#tab-5)).

### BREAK-2. Trailing spaces hang {#break-2}

After a break at a space, every space that follows stays on the line. These trailing spaces don't take part in the fit test or in alignment and justification ([JUST-1](just#just-1), [JUST-4](just#just-4)), and may run past the right edge.

### BREAK-3. Break opportunities {#break-3}

A line may end after:

- a space;
- a hyphen-minus `-`, a hyphen U+2010 or an en dash U+2013, also before digits and also in table cells (except as [BREAK-8](break#break-8) says);
- a soft hyphen ([BREAK-7](break#break-7));
- a tab character;
- before and after an inline picture, even with no space next to it.

Nowhere else: not after a no-break space or a no-break hyphen, an em dash or a slash, and not between ideographs. Line breaks (`w:br`, `w:cr`) always end the line; page and column breaks end it too and move on to the next page or column ([COL-2](col#col-2)).

### BREAK-4. When no break opportunity fits {#break-4}

When the first character that doesn't fit has no break opportunity before it on the line, end the line before that character, splitting the text there. A line always takes at least one character or object, so a single glyph wider than the line still goes on it.

### BREAK-5. Which breaker runs {#break-5}

| Alignment | Compat | Breaker |
|---|---|---|
| left, centre, right, distribute | any | greedy: fill to the last break opportunity that fits; spaces never shrink |
| justified (`both`) | ≤ 14 | greedy; spaces only expand ([JUST-1](just#just-1)) |
| justified (`both`) | 15 | [BREAK-6](break#break-6) |

### BREAK-6. Compat-15 justified breaking {#break-6}

Break each stretch between hard line breaks separately. Split it into items (the text between spaces, also ended by a break opportunity inside it: after a hyphen, [BREAK-3](break#break-3), but not a soft hyphen; the next item then follows with no gap) with their widths, and the width of each gap of spaces before an item. `column` is the line's available width (first-line indent on the first line). Then, for each line starting at item `i`:

```
jmax = the most items that fit with every gap shrunk by 25%:
       Σ item widths + 0.75 · Σ gaps ≤ column                (hard limit)
if the rest of the stretch fits unshrunk: it is the last line, stop
for each end e in i+1 … jmax  (g = Σ gaps inside the line, excess = natural width − column):
    r(e) = excess / (0.25 · g)       if excess > 0     (compression)
    r(e) = −excess / (0.50 · g)      otherwise         (expansion)
    a one-item line is a candidate only when nothing longer fits
end the line at the e with the smallest r; on a tie take more items
```

- The line ends after the chosen item, with its following spaces hanging ([BREAK-2](break#break-2)).
- A line that can only expand stays expanded however loose; no line is ever compressed past 25% ([JUST-2](just#just-2)).
- The last line is taken as it is when it fits without shrinking; otherwise it goes through the same choice, so it can end up compressed, or split leaving a short last line. A last line is never expanded ([JUST-3](just#just-3)).
- Use each gap's own width, so mixed fonts work.
- Fall back to greedy breaking ([BREAK-5](break#break-5)) for the whole paragraph when it contains a tab, or when a single item is wider than a line.
- The choice is per line, never a paragraph-wide optimisation. Only the spaces change width; the spacing inside items never changes.
- Being per line, it applies to a free interval beside a float too ([FLOAT-7](float#float-7)): `column` is the interval's width. An interval whose stretch holds a tab is filled greedily.

Example: with `column` 468, items of natural line width 470.2 at e = jmax (8 gaps of 2.4868, g = 19.894) and 455.1 at e = jmax − 1 (7 gaps, g = 17.408): r(jmax) = 2.2 / 4.974 = 0.44, r(jmax − 1) = 12.9 / 8.704 = 1.48; end at jmax and compress.

### BREAK-7. Hyphens {#break-7}

- A soft hyphen (`w:softHyphen`, U+00AD) is a break opportunity with no width. When the line breaks there, draw a hyphen at the line's end; the break is usable only if the hyphen fits too. Otherwise draw nothing.
- A no-break hyphen (`w:noBreakHyphen`, U+2011) is drawn as an ordinary hyphen and is never a break opportunity.
- An ordinary hyphen-minus allows a break after it ([BREAK-3](break#break-3)).
- There is no automatic hyphenation: `w:autoHyphenation`, `w:hyphenationZone`, `w:consecutiveHyphenLimit`, `w:doNotHyphenateCaps` and `w:suppressAutoHyphens` change nothing.

### BREAK-8. No break after hyphens in East Asian runs {#break-8}

With `w:characterSpacingControl` set to `compressPunctuation`, there is no break after a hyphen-minus or an en dash in a run whose East Asian font is an East Asian font (or whose East Asian slot is the theme's empty one, [STYLE-8](style#style-8)). The line breaks at the space before instead.

Example: "DL#: TX      -46541561" in such a run wraps at the spaces, never after the "-".

### BREAK-9. Squeezed lines {#break-9}

In compat ≤ 14 with `compressPunctuation`, a line whose runs are East Asian as in [BREAK-8](break#break-8) may squeeze in the next unbreakable stretch that crosses the right edge:

1. Only single spaces (not part of a run of spaces) of such runs, before that stretch, can give way. Each may shrink to half its font's average character width: `shrink = max(0, width − xAvgCharWidth × size / upem / 2)`.
2. If the stretch fits with all that shrink used (`rightEdge + Σ shrink`), it stays on the line. Shrink the spaces in proportion, `space −= shrink × overflow / Σ shrink`, so the last character ends at the right edge.
3. Only the first stretch to cross squeezes in: a second crossing one wraps, so a line takes at most one squeezed stretch.
4. The first stretch on a line, or one with a pending right or centre tab, never squeezes. After a tab, only the spaces between the line's last tab and the stretch can give way.

Example: Arial spaces can lose about 20% of their width this way, Courier New spaces 50%.

Example: Arial 12 "⇥You are ordered … at 10:00 am with", 3.7pt wider than the 504pt after the tab, keeps "with": its 17 spaces after the tab can give 0.686pt each.
