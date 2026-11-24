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
- In vertical text (`ruler.lines` is `'down'` or `'up'`), the indents and tab stops are on the vertical ruler, measured
  along the lines from where they start (the top margin, or the bottom one for lines running up), and the horizontal
  ruler shows the text columns across the page in the order the lines stack (right to left for `tbRl`).

## Selecting

A selection can hold several pieces, as in Word: Ctrl+drag (Command on a Mac) adds what the drag goes over, Ctrl+click
adds the sentence, Ctrl+double-click the word and Ctrl+triple-click the paragraph. The same gestures on selected text
take what they cover out of the selection. A Ctrl+drag across cells adds a block of cells. Alt+drag (Option) selects a
column block, a rectangle of text across lines.

`selection.ranges` lists every selected range of text in document order, each from a `start` to an `end` text point (a
paragraph id and an offset in Unicode scalar values: a field counts as its result, an object or note reference as one
character, hidden and deleted text count); a column block gives one range per line. `selection.cellBlocks` lists the
blocks of cells of a discontiguous selection. At most 1,000 ranges are listed (`rangesTruncated` says there are more).

`setSelection({ ranges, cells?, activeCells? })` selects ranges and blocks of cells of one story from code; the last
range (or the `activeCells` block) is the one later gestures go on from. Formatting, Copy, Cut and Delete act on every
piece; typing and Paste replace the last one. Commands Word refuses on a multiple selection (a comment, a bookmark, a
table's rows and columns) fail `unavailable` and are disabled in `commands`.

`selectSimilarFormatting()` selects every stretch of text formatted as the selection's first character, and
`selectStyleInstances(style)` every use of a style in the main document (`StyleInfo.uses` counts them). In extend mode
(F8), Shift+F8 (`shrinkSelection`) steps the selection back down: the paragraph, the sentence, the word, the caret.
Ctrl+Alt+Page Up and Page Down (`moveSelection` by `windowEdge`) go to the top or bottom line of the window.

## Vertical text

In a vertical section (`setSectionTextDirection`), a vertical cell or a vertical text box, the caret is a bar across
the line. The host's caret request and the frame give its box with `line` (`caretLine`) `'down'` or `'up'`: `width` is
the line's thickness and `height` is 0, so an IME can place its window beside the line and run its text vertically; the
SDKs do this for their input element and the Windows IME. A text box turned by other than quarter turns has a slanted
caret, given by its two `ends`. The pointer over vertical text is the vertical I-beam (`verticalText`).

The arrow keys follow the text: in `tbRl` text Down and Up move along the line and Left goes to the next line, Right to
the one before. A table in a vertical section stands upright, and its cells keep the horizontal keys.

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

Fields are inserted and changed by code (`insertField`, `setField`); the elements service's field catalogue and
`composeField` build the code an Insert Field dialog would (see Services). A hyperlink takes a `bookmark` (the place
in its target), a `screenTip` and the `frame` it opens in; `state.selection.link` reads them back with the `target`
apart from the bookmark. A date control takes a date (`fillControl({ id, date })`, shown in its format), its
`calendar` and what it stores (`storeAs`) as options; a building block gallery control (`insertControl` kind
`buildingBlockGallery`) its `gallery` and `category`. `addRepeatingItem` adds an item to a repeating section, a copy
of one of its items.

Legacy form fields fill in as Word's do where forms are protected: Tab and Shift+Tab move between them
(`selectNextFormField`), Space or X toggles a selected check box, and F4 or Alt+Down on a drop-down emits
`hostCommand` `openFormFieldList` for the host's list. A click on a drop-down there, or a double-click on a form field
in an unprotected document, emits the `formFieldClick` or `formFieldDoubleClick` gesture. `state.selection.context`
gives each form field's settings (macros, AutoText help, a check box's size and default, a drop-down's default).

## Restrict Editing

`setDocumentProtection` sets the editing restriction (`kind`), the formatting restriction (`formatting`, with
`autoFormatOverride`, `blockThemeSwitching` and `blockStyleSetSwitching`) and starts or stops enforcing it;
starting takes the password twice (`password`, `confirmPassword`) and is refused with nothing restricted.
`state.review.protection` reads it all back, with whether the restriction takes exceptions (`exceptionsAllowed`) or
sections (`sectionsAllowed`). Exceptions are editable ranges: `setEditableRangeEditor({ editor: { group } | { user },
allowed })` lets a group or a person edit the selection, or stops them, and `editableRanges()` lists them;
`editorsHere` and `rangeCount` say who may edit at the selection and how many there are. `setEditingGroups` tells
the engine which groups the author is in. `selectNextEditableRange` and `selectEditableRanges` are Find Next Region
I Can Edit and Show All Regions I Can Edit, and `setViewOptions({ showEditableRegions })` highlights the regions the
author may edit under enforced protection. `recommendedMinimumStyles()` is Formatting Restrictions' Recommended
Minimum.

## Content control labels

As in Word, the content control holding the caret and the one under the pointer show a dashed box with a tab above
it. The tab shows the control's title, or its tag when it has no title. Every repeating section around the caret is
boxed, and so is a shown sub-template, whose tab shows its name. A click on a tab selects the control. Nothing is
shown while read-only. `setViewOptions({ controlLabels: false })` turns the labels off for the whole document, and a
control whose `w15:appearance` is `hidden` (Word's Show as None) never shows one. The state's `view.controlLabels`
is the current setting.

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

### Draw Table and the Eraser

`setTableTool('pen')` is Word's Draw Table and `setTableTool('eraser')` its Eraser; `'none'` ends either, as do Esc
and any other edit. `state.view.tableTool` says which is on, and the pointer shows a pencil or an eraser.

- The pen: a drag across or down a table splits every cell it crosses there, a drag from a cell's corner to its
  opposite corner draws the cell's diagonal, a drag along a border redraws it, and a drag outside every table draws a
  new one. While dragging, `state.view.tableToolFeedback` gives what it would draw (`line`, `diagonal` or `outline`)
  and where, in page points, for the host to draw dashed. The pen draws in `state.view.tablePen` (single ½ pt automatic
  until `setTablePen(line)` changes it).
- The eraser: a press on the border between two cells merges them, and so does each border the pointer crosses while
  pressed. `state.view.tableEraserTarget` gives the border under the pointer that it would remove.

`drawTableStroke({ page, from, to })` and `eraseTableBorder({ page, at })` do the same from code, in page points. Each
stroke is one undo step; a stroke on a protected or locked table fails as other table edits do.

## How much is measured

`setViewOptions({ detail })` (on the web also `view.setDetail`) sets how much the editor measures:

| Detail | Shows |
|---|---|
| `quiet` | a chip with the value |
| `standard` | dimension lines with the value; everything while Alt is held (the default) |
| `full` | everything: the change from the start, the margins' distances, table sizes, and a status line for a selected row, column or table |

Every measurement uses the ruler unit: inches with two decimals and ″, centimetres with two decimals and " cm".

## Keyboard

The keys are Word's, and `catalog` lists each command's (Windows and Mac). Besides the formatting keys, the editor
takes Ctrl+] and Ctrl+[ (`growFontOnePoint`, `shrinkFontOnePoint`), Ctrl+T and Ctrl+Shift+T (`increaseHangingIndent`,
`decreaseHangingIndent`), Ctrl+Q (`clearParagraphFormatting`), Alt+X (`toggleCharacterCode`), Alt+Shift+Up and Down
(`moveParagraphs`; in a table, `moveRows`), F11 and Shift+F11 (`selectField`), Ctrl+Alt+Enter
(`insertStyleSeparator`, read back as a paragraph's `hiddenMarks` in `elements.structure()`), Ctrl+Shift+8
(formatting marks), Ctrl+Y (`repeat`, which redoes first), and F2 and Shift+F2 (`moveText`, `copyText`: the selection
moves to where the next click or arrow keys put the place, `selection.pendingMove`, when Enter is pressed). With
nothing selected, Highlight turns the pointer into the highlighter (`view.highlighter`) until Esc or Highlight again.
Ctrl+Shift+C copies formatting and the first Ctrl+Shift+V paste after it pastes the formatting; a browser may take
Ctrl+Shift+C for its inspector, so forward it.
