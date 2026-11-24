---
title: Formatting
sidebar_position: 14
---

# Formatting

Formatting is commands, and what they set reads back in the state, the same over the selection or `'mixed'`. This
page covers what goes beyond on and off: colours that follow the theme, borders and shading, fitted text, OpenType
features, page setup across sections, and the history and counts an application shows around them.

## Colours

Text and table colours (`setFontColor`, `setUnderlineColor`, `setShading`, `setParagraphShading`, `setCellShading`,
border lines, row shading, `setDocumentDefaults`) take an RGB colour, `'#1F4E79'` or `{ rgb }`, or a theme colour with at
most one tint or shade: `{ theme: 'accent1', transforms: [{ kind: 'tint', value: 0.4 }] }` is Word's "Accent 1, Lighter
60%". A theme colour follows the theme: change the theme and the text changes with it. More than one transform, or
other kinds, is `invalid-argument`. .NET: `ColorValue.FromTheme(ThemeColor.Accent1, new ColorTransform(ColorTransformKind.Tint, 0.4))`.

`themeColourGrid()` (.NET: `GetThemeColourGridAsync`) gives Word's picker grid: a column for each of Background 1, Text 1,
Background 2, Text 2 and Accent 1 to 6, each the colour and its five lighter and darker variants, with the value to
pass, the RGB it shows and its name. It is a query; `state.themeColours` (the theme's colours) changes when the theme
does. `state.themeFonts` names the heading and body fonts, and `setFont({ theme: 'headings' })` (or `'body'`) sets a
theme font.

## What the state reads back

`state.formatting` keeps `color` and `underlineColor` as the RGB the text shows, and adds the colours as written,
`colorValue` and `underlineColorValue`; `fontTheme` (`'headings'`, `'body'` or `'none'`); `textBorder` and
`textShading`; `fitText`; `ligatureFlags` and `stylisticSets`. `state.paragraph` adds each border edge
(`borderTop`, `borderLeft`, `borderBottom`, `borderRight`, `borderBetween`) with its colour, distance and shadow,
`shadingValue` (fill, pattern and pattern colour) and `tabs`. `state.page.pageBorders` gives the section's page
borders, and `state.selection.table.bands` its table's band sizes: the rows and columns in each band, and the
table's own (`ownRows`, `ownColumns`) where it overrides its style. `setTableBandSizes({ rows, columns })` (.NET
`SetTableBandSizesAsync`) sets them, 1 to 32767 each, or `null` to leave one to the style; one left out is kept.

## Tables

`state.selection.table` reads back the selection's cells and table as written:

- `cellBorders` (each edge, the inside lines and both diagonals; mixed across the cells) and `tableBorders`;
- `cellShadingValue` (fill, pattern and pattern colour; `cellShading` is the fill alone) and `tableShading`;
- `cellMargins` (mixed across the cells) and `defaultCellMargins`, the table's (Table Options);
- `hideMark`, `rowExceptions` (a row's own alignment, indent, spacing, shading and borders, and whether it has any),
  `fit` (`fixed`, `contents` or `window`), and the focus cell's `columnSpan` and `rowSpan`.

| Command | Does |
|---|---|
| `setTableCellMargins({ top, left, bottom, right })` | the table's default cell margins, 0 to 1584 pt; one left out is kept |
| `insertCells(shift)` | Word's Insert Cells: `right` shifts the cells right, `down` shifts them down (adding a row); a whole row or column is `insertRows` or `insertColumns` |
| `setRowProperties({ exceptions })` | a row's own `indent` (−1584 to 1584 pt) and `shading` (`{ fill, pattern, color }`, or a colour alone) among the other row exceptions |
| `convertTextToTable({ …, autofit, columnWidth })` | `autofit` `fixed` (each column `columnWidth`, 1 to 1584 pt, or shared out without one), `contents` or `window` |
| `convertTableToText({ …, convertNested })` | with `convertNested`, the tables inside the cells become text too |

`textToTableSize({ separator, character, columns })` (.NET `GetTextToTableSizeAsync`) answers the rows and columns
`convertTextToTable` would make of the selection with those options, for Convert Text to Table's Number of rows and
columns; `null` when it can't convert it.

## Borders and shading

A border line (`BorderLine`; .NET `CellBorder`) is a style, a width (0.25 to 6 pt) and a colour, and optionally its
`space` from what it borders (0 to 31 pt) and a `shadow`. Without them a line takes Word's: no distance for text and
cells, 1 pt for paragraphs, and for a page border 24 pt from the page's edge, or 1 pt (top and bottom) and 4 pt (left
and right) from the text; a page border edge that is kept keeps its own. The older `shadow` flags of `setTextBorder`,
`setParagraphBorderLines` and `setPageBorders` still apply to the lines that give none. `setPageBorders` also takes
`behindText`: off is Word's Always display in front.

## Text

- `setFitText(width)` squeezes or stretches the selected text to a width (Fit Text); `null` lets it take its own. A
  caret in fitted text acts on its whole region.
- `setTypography` takes `ligatureFlags` (`standard`, `contextual`, `historical`, `discretionary`, each on or off) and
  `stylisticSets` (each 1 to 20) beside its presets, and they win over `ligatures` and `stylisticSet`.
- `insertLineBreak({ clear: 'both' })` is a text wrapping break: the text after it goes below the floating objects on
  that side (`left`, `right`, `both`).

## Page setup

The page setup commands (`setMargins`, `setPaperSize`, `setOrientation`, `setColumns`, `setLineNumbering`,
`setPageVerticalAlignment`, `setPageNumbering`, `setSectionTextDirection`, `setHeaderFooterDistance`,
`setSectionBreakType`) take `applyTo`: the selection's sections (the default), `wholeDocument`, or `thisPointForward`,
which puts a new-page section break at the selection's start (selected text stays) and sets the sections from there
on, as one undo step. From a cell, block or object selection `thisPointForward` is `unavailable`. On the web the
single-value ones take it as a second argument: `setOrientation('landscape', { applyTo: 'wholeDocument' })`.

`insertPageNumber` puts a number in a page margin with `where: { margin: { side } }` (`left`, `right`, `inside`,
`outside`): in a frame anchored in the header, as wide as the margin. In a header or footer, `alignment` `inside` and
`outside` frame it on the facing page's inside or outside edge. `showOnFirstPage: false` gives the section a different
first page whose header and footer don't hold the number. `pageNumberSample(format, chapterSeparator?)` is Format
Page Numbers' example (`1-I`).

`setColumns` applies where the Columns dialog does: `applyTo` `section` (the default), `wholeDocument`,
`thisPointForward` (a continuous section break at the selection's start, or a column break with
`startNewColumn: true`) or `selectedText` (continuous breaks around the selected text). `state.page.columns.equal`
says whether the columns are of equal width, and `columnPreset('two')` (`one`, `three`, `left`, `right`) gives the
columns Word's presets would set.

With mirrored margins the ruler shows each page's own margins: `state.ruler.horizontal.startMargin` and `endMargin`
say which margin each edge is (`inside`, `outside`), and `gutter` where the gutter is; dragging an edge sets that
margin.

## History and counts

`undo({ count })` and `redo({ count })` take back, or do again, several steps as one change: the Undo and Redo lists'
choice. `historyLabels({ limit })` (.NET: `GetHistoryLabelsAsync`) gives the lists themselves, at most 100 labels
each, most recent first, and whether there are more. The state keeps only whether there is something to undo and the
next labels, as it is rebuilt on every keystroke.

`wordCount({ selection, includeTextboxesAndNotes })` counts the document, or with `selection` the selected text (the
document's when nothing is selected); text boxes, footnotes and endnotes count unless `includeTextboxesAndNotes` is
`false`. A count walks the whole document, so it is a query: a status bar's "N of M words" asks with `selection` and
without when it refreshes, debounced on its side.
