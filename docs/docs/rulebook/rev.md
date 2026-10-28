---
title: Tracked changes
sidebar_position: 21
---

A document's tracked changes (`w:ins`, `w:del`, `w:moveFrom`, `w:moveTo`, the `w:*PrChange` elements, `w:cellIns`, `w:cellDel` and `w:cellMerge`) show in one of four markup views: All markup, Simple markup, No markup and Original. The view is a display setting, not part of the document. First decide what the view shows ([REV-1](#rev-1)) and with which properties ([REV-2](#rev-2)), join the paragraphs whose marks it hides ([REV-6](#rev-6)), lay that out, then draw the revision marks ([REV-3](#rev-3)), change bars ([REV-4](#rev-4)) and row and cell tints ([REV-5](#rev-5)).

## What each view lays out

### REV-1. Shown and hidden content {#rev-1}

| Content | All markup | Simple markup | No markup | Original |
|---|---|---|---|---|
| Inserted or moved-to text (`w:ins`, `w:moveTo`) | shown | shown | shown | hidden |
| Deleted or moved-from text (`w:del`, `w:moveFrom`) | shown | hidden | hidden | shown |
| Inserted row (`w:trPr/w:ins`) or cell (`w:cellIns`) | shown | shown | shown | hidden |
| Deleted row (`w:trPr/w:del`) or cell (`w:cellDel`) | shown | hidden | hidden | shown |

- Hidden text takes no space and draws nothing, as hidden text does ([STYLE-7](style#style-7)), and its characters keep their offsets in the paragraph. Text inserted and then deleted shows only in All markup.
- A hidden row takes no space. A hidden cell is left out of its row: the cells after it move left into its columns. Lay the table out as if the hidden rows and cells weren't there.
- No markup lays out the document as if every change were accepted, and Original as if every change were rejected.

### REV-2. Properties before the change {#rev-2}

In Original, lay out:

- each run with its formatting before its `w:rPrChange`, and each paragraph mark likewise;
- each paragraph with its properties before its `w:pPrChange`;
- each section with its page before its `w:sectPrChange`: size, margins, header and footer distances, gutter, columns and title page;
- each table with its properties before its `w:tblPrChange` and its grid before its `w:tblGridChange`;
- each row with its properties before its `w:trPrChange`, and each cell with its properties before its `w:tcPrChange` and its vertical merge before its `w:cellMerge` (`w:vMergeOrig`). Where the earlier table properties name no borders, the table keeps its borders.

The other views lay out the properties as they are.

### REV-6. Paragraphs joined by a hidden mark {#rev-6}

A paragraph mark the view hides joins its paragraph with the next one: in Simple markup and No markup a deleted mark (`w:pPr/w:rPr/w:del`, or `w:moveFrom`), in Original an inserted one (`w:pPr/w:rPr/w:ins`, or `w:moveTo`).

- Lay the paragraphs out as one: the first one's content followed directly by the next one's, with nothing between them. Consecutive paragraphs whose marks the view hides all join, up to the first paragraph whose mark it shows.
- The joined paragraph takes the last paragraph's properties and mark: alignment, indents, spacing, borders and shading, keep and page break settings, its list number, and the mark's formatting for an empty line ([STYLE-5](style#style-5)). The first paragraph's properties don't apply.
- Each paragraph's text keeps its own formatting. Text in equal formatting either side of the join is shaped as one stretch ([FONT-16](font#font-16)).
- A hidden mark followed by a table, or by the end of the story it is in (a section's blocks, a table cell, a header or footer, a note, a text box), joins nothing: its paragraph lays out alone.

Example: in No markup, a centred paragraph whose mark is deleted, followed by a left-aligned one, lays out as one left-aligned paragraph holding both texts.

## Drawing the changes

### REV-3. Revision marks and author colours {#rev-3}

In All markup:

- Inserted text is drawn in its author's colour with a single underline, and deleted text in its author's colour struck through once.
- Moved text is drawn in the move colour, `#00A050`: its destination (`w:moveTo`) double underlined and its source (`w:moveFrom`) double struck through.
- Text both inserted and deleted takes the deletion's colour and both marks.
- The marks use the font's underline and strikethrough lines ([FONT-15](font#font-15)), and an insertion's underline replaces the run's own. A formatting change has no mark.

The other views draw the text as it is formatted.

Authors take colours in the order they first appear in the document: `#D13438`, `#0078D4`, `#5C2E91`, `#498205`, `#CC3595`, `#7160E8`, `#038387`, `#6D5700`, `#CF0F1F`, `#4E6AED`, `#B146C2`, `#394146`, then from the first again. An application can give any author another colour.

### REV-4. Change bars {#rev-4}

In All markup and Simple markup, draw a change bar beside each body line that shows a change:

- inserted, deleted, moved or reformatted (`w:rPrChange`) text on the line, including text the view hides where it would be on the line or at its end;
- on a paragraph's last line, an inserted or deleted paragraph mark;
- on every line of a paragraph, a `w:pPrChange` (a `w:numberingChange` alone gets no bar);
- on every line of a table row, an inserted, deleted or changed (`w:trPrChange`) row, or a row holding an inserted, deleted, merged or changed cell.

Section property changes and comments get no bar, and neither do header and footer lines.

A bar is a black filled rectangle 0.72pt wide whose left edge is `min(36pt, left margin ÷ 2)` left of the body's left edge. It is as tall as its line, and reaches down to the next line when the gap to it is no taller than the line. The bars of consecutive changed lines join into one. Example: with a 72pt left margin, the bar runs from 36pt to 36.72pt across the page.

### REV-5. Row and cell tints {#rev-5}

In All markup, fill the cells of an inserted or deleted row, and an inserted or deleted cell, with a quarter of the change's author colour over the cell's shading (white without one): each channel is `(author + 3 × shading) ÷ 4`, in integers. A cell's change takes precedence over its row's, and a row's deletion over its insertion.
