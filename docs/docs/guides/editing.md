---
title: Editing on the canvas
sidebar_position: 13
---

# Editing on the canvas

The mounted editor shows what a drag or hover does while it happens: the rulers, objects and tables measure, snap and
offer their actions in the canvas. All of it is drawn by the engine in the chrome's theme (see
[Appearance](appearance.md)), and every action is also a command, for keyboard, touch and applications.

## The ruler

- **Dragging** an indent, margin or tab stop moves its marker and a guide through the page with the pointer, snapped to
  the ruler's step (1/16″, or 1/4 cm), and shows a dimension line with the value. Alt drags freely; pressing or
  releasing Alt mid-drag takes effect at once. The drag commits one undo step on release.
- **Hovering** a marker names it and says what dragging it does.
- **The tab type box** in the rulers' corner opens the tab type picker (`popoverRequested` with `kind: 'tabType'`).
  Pick a type, then click the empty ruler to place a stop of it; a ghost of the stop follows the pointer there.
  `setRulerTabType(type)` sets the type, and `showTabTypes()` opens the picker from the keyboard.
- **A click on a stop** (no drag) opens its options (`kind: 'tabStop'`): the alignment, the leader, and Clear tab stop.
  Each change is one undo step and lays the paragraph out again at once. `showTabStop()` opens it for the stop nearest
  the caret.
- **A stop dragged down** past the removal distance fades and says "Release to remove tab stop"; releasing removes it.
- In a table, the ruler shows the column markers; dragging one resizes the columns.

## Objects

- A selected object has its handles, outline and rotate knob, and a floating one its anchor.
- **Moving** an object snaps its edges and centre to the margins, the page centre, and the edges and centres of the
  tables and objects on the page, with a guide for each line it snapped to. Alt moves it without snapping. The distances
  to the left and right margins show while it moves.
- **The object grid**, `setViewOptions({ objectGrid })`, is a dot grid an object snaps to on the axes where no guide
  applies: `off`, `whileDragging` (the default) or `always`. It isn't `gridlines`, the table gridlines.
- **Resizing and rotating** show the original as a dashed outline and a chip by the pointer with the size, in the ruler
  unit, or the angle. A rotation settles on a multiple of 90° within 4°, and Shift snaps it to 15° steps.

## Fields

A field's result looks like the text around it, so a reader can't tell a previewed merge value from typed text.
`setViewOptions({ fieldShading })` shades fields with a light grey backdrop behind their text, on every line they span:

| Setting | Shades |
|---|---|
| `whenSelected` | the field the caret or selection is in or touches (the default) |
| `always` | every field, in the body, headers and footers, tables, text boxes and notes |
| `never` | nothing |

The shading covers what the field shows: its result, its code while codes are shown, or its label in design mode. A
field nested in another is shaded once, with the outer one. Shading is a view setting: it isn't saved with the
document, and it never reaches a PDF or a print. The state's `view.fieldShading` is the current setting.

## Tables

- Hovering a table shows its move handle and its resize handle. The move handle selects the table on a click and moves
  it on a drag; the resize handle resizes it, with a dashed preview and the table's size.
- Dropping a table moved by its handle positions it where it was dropped, and text wraps around it.
  `setTablePosition(position)` does the same from code: `horizontal` from
  the `column`, `margin` or `page`, `vertical` from the `paragraph`, `margin` or `page`, each an `offset` or an `align`,
  and `wrapDistance` from the text; what isn't given is kept, and `null` puts the table back in the text. The state's
  `commands.setTablePosition.value` is the caret's table's position, absent for a table in the text.
- Hovering just outside a column or row border shows a "+" button and a dashed insertion line: a click inserts a column
  or row there, selected, in one undo step (`insertColumns` or `insertRows`). The select zones select a column, a row or
  the table.
- Dragging a column border shows the affected columns' widths as they change.
- With the table or a span of it selected, the same affordances show without hovering, for touch, pen and keyboard.

## How much is measured

`setViewOptions({ detail })` (on the web also `view.setDetail`) sets how much the editor measures:

| Detail | Shows |
|---|---|
| `quiet` | a chip with the value |
| `standard` | dimension lines with the value; everything while Alt is held (the default) |
| `full` | everything: the change from the start, the margins' distances, table sizes, and a status line for a selected row, column or table |

Every measurement uses the ruler unit: inches with two decimals and ″, centimetres with two decimals and " cm".
