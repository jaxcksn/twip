---
title: Services
sidebar_position: 5
---

# Services

The engine computes the models behind a word processor's chrome, and your application draws them: a context menu,
a dialog's fields, find results, spelling suggestions, a navigation pane. Each service is a property of the editor
(`editor.menus`, `editor.find`, ...) or the session (`session.Menus`, `session.Find`, ...). The examples here are
TypeScript; the .NET names are the same in PascalCase with `Async`.

```ts file=../../examples/web/services.ts
```

## Context menu

`contextMenu` (`ContextMenuRequested`) carries the menu for what was clicked and where to show it, in client pixels.
Items are commands (with their label, state and shortcut), spelling suggestions, submenus and separators. Pass an
item's `action` back to `menus.invoke` to run it as one transaction. `menus.contextMenu()` builds the menu without a
click, as the context-menu key would. You can also build your own menu from `menu.target` and the catalog.

In the browser, while a `contextMenu` listener is registered, the browser's own menu never opens over the document.
With no listener, it does.

The menu follows Word's: Cut, Copy and the Paste Options submenu (`inline: true`: show it as a row of icons), then the
target's own groups (a field's Update Field and Toggle Field Codes; a link's Open, Copy and Remove Hyperlink; a numbered
list's Restart and Continue; a table's Insert, Delete, Merge, Distribute, Cell Alignment and AutoFit; an object's Group,
Bring to Front, Send to Back and Wrap Text with Edit Wrap Points and Set as Default Layout; review and header and footer
actions), Synonyms, then New Comment. Items of kind `action` aren't one command (opening or copying a link, a Cell
Alignment pair, a setting); pass their `action` to `menus.invoke` like any other. Items that would open a dialog aren't
in the model: add your own.

**Synonyms.** The engine has no thesaurus. `menus.wordAtSelection()` gives the word at the selection and its language;
look it up in yours and pass the words to `menus.contextMenu({ synonyms })` (at most 8 show; `[]` shows an empty
submenu for your "(No Suggestions)"). Choosing one replaces the word, one undo step. Shift+F7 emits the `thesaurus`
host command.

**Right-button drags.** A right-button drag of selected text opens the drop menu where it lands (`target: { kind:
'drop' }`): Move Here, Copy Here, Link Here (enabled when the document has a file name), Create Hyperlink Here and
Cancel. A right click that doesn't drag opens the context menu on release, as Word does. The drop menu closes when the
document changes; invoking it then fails with `'unavailable'`.

## Dialogs

`dialogs.open(kind)` returns a dialog session: the selection's current `values` (`'mixed'` where they differ), and
for each field its label, kind, range and options. `update(patch)` recomputes dependent values; `apply()` validates
and commits everything as one undo step; `cancel()` changes nothing. The dialogs are `font`, `paragraph`,
`pageSetup`, `insertTable`, `tableProperties`, `pictureSize`, `altText`, `hyperlink`, `tabs`, `accessibility`,
`bordersShading`, `frame`, `printedWatermark` and `fillEffects`; the style dialogs are in [Styles](styles.md).

### Borders and Shading

`bordersShading` (.NET: `Dialogs.OpenBordersShadingAsync`) opens on the selection's paragraph: its edges (`top`,
`bottom`, `left`, `right`, `between`, each a line or `null`) and its shading as written (`fill`, `pattern`,
`patternColor`). A `setting` of `none`, `box` or `shadow` sets the edges with the line in `style`, `color` and `width`;
setting an edge yourself (to that line, or `null`) makes the setting `custom`. `applyTo` is `paragraph`, or `text` when
the selection is within one paragraph. Text has one border, a box (`none`, `box` or `shadow`). Changing `applyTo` shows
that target's borders and shading. Applied to cells, `diagonalDown` and `diagonalUp` are the first cell's diagonals
(top left to bottom right, top right to bottom left); a diagonal makes the setting `custom`, and `none` removes them.

The Page Border tab (`pageSetting`, `pageTop`, `pageBottom`, `pageLeft`, `pageRight`) works the same way for the
section's pages. `pageOffsetFrom` measures them from the page's edge or the text, `pageDisplay` picks the pages they
show on, and `pageApplyTo` the section or the whole document.

`apply` runs what changed: `setParagraphBorderLines`, `setTextBorder`, `setShading` and `setPageBorders`. Every border
style draws, on paragraphs, text, cells and pages, and every shading pattern, on text, paragraphs, cells and tables. Art page
borders are kept but not drawn. A cell's diagonals draw as Word draws them: one line in any style (`thick` twice and
`double` three times as wide, the dashed styles as hairlines). Apply to Cell and Apply to Table set shading only, so
their border controls are disabled; `setCellBorders` sets the diagonals.

### Printed Watermark and Fill Effects

`printedWatermark` reads the section's watermark and `apply` runs `setWatermark` (or `removeWatermark`): none, a picture
with its scale and washout, or text with its font (the theme's body font by default), language, size, colour, transparency and layout. `fillEffects` edits the page
colour (gradient, pattern, texture or picture) and `apply` runs `setPageColor`.

### Frame

`frame` (.NET: `Dialogs.OpenFrameAsync`) opens on the frame the selection is in, and is `unavailable` elsewhere. Its
values are `setFrame`'s: text wrapping, width and height with their rules, the horizontal and vertical position (an
alignment, or a distance when the alignment is `null`) with what each is relative to, the distance from text, and
Lock anchor. **Move with text** (`moveWithText`) is the vertical position relative to the paragraph: turning it off
makes it relative to the margin. Setting `remove` (its Remove Frame button) makes `apply` run `removeFrame`, which puts
the frame's paragraphs back in the text before the paragraph it was anchored to.

## Find and replace

`find.next` and `find.previous` select and reveal the next match; `find.all` lists every match with its context and
page, for a results pane (`truncated` when the search gave up on matches past its budget); `replace` replaces the current match and finds the next; `replaceAll` is one
transaction. Matching uses the text of the current markup view.

The query is Word's Find and Replace dialog as values:

- `text` takes Word's special characters: `^p` a paragraph mark, `^t` a tab, `^?` any character, `^#` any digit,
  `^$` any letter, `^12` a page or section break, `^g` a graphic, `^^` a caret. A match can cross paragraph marks
  (its id is then `paragraph:start:lastParagraph:end`). With `regex: true` the text is a regular expression, matched
  paragraph by paragraph.
- `matchCase`, `wholeWords`, `matchPrefix`, `matchSuffix`, `ignorePunctuation` and `ignoreWhiteSpace` are the
  dialog's options; `wrap: false` stops at the document's end (`found: false`) so you can ask before going round, as
  Word's Up and Down do; `inSelection: true` searches the selection, and keeps searching it while the match it selected
  stays selected.
- `format` is the formatting a match must have (`font`, `paragraph`, `style`, `highlight`, `language`, `noProof`,
  `tabs`, `frame`), each value given; an empty `text` with a format finds each stretch that has it.

The replacement takes `^&` (the text found), `^c` (the editor's last copy) and the other codes, or a regular
expression's `$1` and `${name}`; `replace` and `replaceAll` take a third argument, the formatting the replacement gets
(with an empty replacement the matches keep their text and only take it). A query that doesn't parse fails
`invalid-argument` with the reason. A search gives up matches past its budget of work on a hostile document: the
result's `truncated` says so, and your pane can say the search stopped early.

`find.highlightAll(query)` highlights every match on the pages (Word's Reading Highlight) and gives the count;
`state.view.findHighlights` is on until the document changes or `find.clearHighlights()`.

`commands.goTo({ what, target, name })` is Go To: a page, section, line, bookmark, comment, footnote, endnote, field,
table, graphic, equation, embedded object or heading, the n-th (`{ absolute: n }`, numbers past the last go to the
last), n on or back (`{ relative: n }`), `'next'` or `'previous'`; `name` narrows comments to a reviewer, fields to a
kind, objects to a program. It fails `unavailable` when there is none to go to; Ctrl+G emits `hostCommand` `goTo` for
your dialog.

## Spelling

Add a Hunspell dictionary (`.aff` and `.dic` bytes) per language with `spelling.addDictionary`, or pass them in the
session options. `next()` selects the next misspelling with its suggestions, `ignore` ignores a word for the
session, and `add` adds it to the user's dictionary and emits `wordAdded` so you can save it. Give saved words back
with `loadUserWords`.

## The clipboard

A mounted editor (or a `DocumentEditor`) owns the clipboard: Ctrl+C, Ctrl+X and Ctrl+V work with no code, with the
document's formatting kept between Twip editors and plain text and HTML for other applications. Buttons and menus
call the commands:

```ts file=../../examples/web/clipboard.ts
```

- `copy` copies the selection, also when the editor is read only. `cut` copies and deletes it.
- `paste` reads the system clipboard; `{ as: 'text' }` (Ctrl+Shift+V) drops the formatting. In the browser,
  reading the clipboard from a button needs the page's clipboard permission, and fails with `'not-allowed'` when the
  browser refuses. Keyboard paste always works.
- `pasteText(text)` inserts text you supply, at the selection, as one undo step.
- `copyFormatting` and `pasteFormatting` are the format painter; `formatPainter` in state says formatting is held.

A paste takes what the clipboard holds as Word does: the editor's own copy, HTML, a picture (PNG, JPEG, GIF, BMP, TIFF,
SVG, EMF or WMF: a browser's Copy Image, a screenshot) or plain text. When nothing says how, the cut, copy and paste
options decide (`setPasteOptions`; `state.pasteOptions`, Word's defaults until you change them): `within`, `between`,
`conflict` and `other` take `keepSource`, `merge` or `textOnly` (`conflict` also `destinationStyles`), and `smart`
holds the smart cut and paste adjustments (`wordSpacing`, `tables`, `mergeLists`, ...). They are a session setting,
never saved.

After a paste the engine asks for the Paste Options button: `popoverRequested` with kind `pasteOptions`, its `anchor`,
the `choices` it offers and the `current` one. `commands.repaste(as)` pastes it again that way (one undo step takes the
document back to before the paste), `dismissPasteOptions` hides it, and typing, any other edit, undo or Esc closes it
(`popoverClosed`). Word opens the button's menu on a lone Ctrl press: that key is yours to watch.

Paste Special (Ctrl+Alt+V emits `hostCommand` `pasteSpecial`) is yours too: `pasteChoices()` lists what the clipboard
can paste as at the selection, in Word's order, with the one Ctrl+V takes, and `commands.paste({ as })` pastes it that
way (`keepSource`, `merge`, `destinationStyles`, `textOnly`, `html`, `text`, `picture`, and for copied cells
`nestTable`, `mergeTable`, `insertRows`, `overwriteCells`). An application that owns the clipboard itself uses
`copyContent()` (what to put on it: text, HTML, and a copied object's picture as `image/png`), `clipboardHtml(token)`
(the copy's HTML with its objects drawn, asked when a target wants HTML) and `pasteContent(data, { as })`.
`commands.insertFile({ from })` is Insert Text from File: a DOCX's content in place of the selection, its styles and
sections with it.

A custom .NET host supplies the system clipboard through `IEditorHostAdapter.SetClipboard` and
`GetClipboardAsync` (see [hosting](dotnet#hosting-a-session-in-another-ui-framework)).

## Picture controls

A picture content control with no picture shows its placeholder picture (`w:showingPlcHdr`): the editor, the saved
DOCX and the PDF all show that picture. `insertControl({ kind: 'picture', placeholderImage })` sets it (PNG, JPEG,
GIF, BMP or SVG), and `setControl` replaces it while the control still shows it. A document's own JPEG XR or EMF+ pictures draw as they do elsewhere. `elements.list()` reports each
control's tag, size and `showingPlaceholder`, so the application finds the unfilled ones and decides what each tag
means; `fillControl({ id, image })` fills one, fitted to its box.

In design mode a picture control resizes from its handles (a corner keeps the aspect, a side stretches) and drags to
another place in the text, its control going with it. Outside design mode, or when its control is locked, it can't
be moved or resized.

`setProtectedPictureControls({ tagPrefixes })` protects the pictures of the controls whose tag starts with one of the
prefixes (compared without case), as a content lock does: editing can't delete, paste over or replace them, only
`fillControl` and `setControl` change them, and in design mode deleting one removes its whole control. The setting
belongs to the session and isn't saved.

A picture control floats with `insertControl({ kind: 'picture', float: { wrap, horizontal, vertical } })`, or with
`setObjectWrap` on a selected one; `setObjectWrap('inline')` puts it back in line. A floating picture control drags,
resizes and takes the wrap and position commands as any floating object does. It is saved as a floating text box with
no line, no fill and no insets holding the inline picture control at the box's size, with no VML copy, and a text box
of that shape reads back as one.

A click selects a picture control. Activation, which asks the application to act on it, is separate:

| Mode | `pictureControlActivated` comes from |
|---|---|
| editing, read only | a click on an unfilled picture control (the `pictureControlClick` gesture), a double-click on one (`pictureControlDoubleClick`), Enter while one is selected, `activatePictureControl` |
| design | `activatePictureControl` only; a double-click or Enter emits `elementActivated` for the control |

Binding `pictureControlClick` to `'none'` with `gestures.set` makes a click select only; the double-click and Enter
still activate. A double-click activates once, even when its first click already did.

```ts file=../../examples/web/picture-controls.ts
```

```csharp file=../../examples/dotnet/PictureControls.cs#picture-controls
```

## Navigation and styles

`navigation.headings()` lists the document's headings with their level and page, for a navigation pane, and
`navigation.select(id)` goes to one. `navigation.currentHeading()` is the heading the selection is under (ask on
`selectionChanged`, to highlight it), and `find.all` gives each result the `heading` it is under, for the Headings
tab's highlighting. The pane's commands take a heading's id: `promoteHeading` and `demoteHeading` (its subheadings
move with it unless `subtopics: false`), `moveHeading` (before another heading or after its content, optionally at
a new level; tracked as a move while tracking), `deleteHeading`, `insertHeadingBefore`, `insertHeadingAfter`,
`insertSubheading` and `selectHeadingContent` (Print Heading and Content prints the selection). On the selected
paragraphs, `promoteOutline` (Alt+Shift+Left), `demoteOutline` (Alt+Shift+Right), `promoteToHeading1` and
`demoteToBodyText` change the outline level: a direct outline level changes in place, else the paragraph takes
built-in Heading N (added to the document in the same step); body text promoted takes the level of the heading
before it. With `subtopics: true` the headings under each one changed follow by as many levels, each kept to 1 to 9.
For the Pages tab, `pageThumbnail(page, width, height)` draws a page into pixels, and `pageVersions()` gives each
laid-out page a version that changes only when the page is laid out again: key cached thumbnails by it.

`notes()` lists the footnotes and endnotes for a notes pane: each one's mark, text and page.
`noteSeparators(kind)` reads the separators `setNoteSeparator` sets (a text, or the built-in line). `styles.list()` lists the styles with what a style gallery needs: name, type,
whether it's quick, priority and a preview of its formatting.

## Elements and data parts

For merge designers: `elements.list()` lists fields and content controls in document order, `get(id)` gives one
in full (lock, placeholder, data binding, options, children), `structure()` gives the body as blocks, and
`select(id)` selects one, entering a header or footer when needed. Each element says whether a field shows its code
(`showingCode`) or is out of date (`dirty`), and a date control its `date`.

For an Insert Field dialog, `elements.fieldCatalogue(category?)` lists Word's fields with their description, syntax,
arguments and switches, `fieldFormats()` the formats and pictures, `composeField(choice)` makes the code (and its
problems, cheap enough to call on every keystroke), `parseField(code)` reads a code back into choices, and
`fieldHere()` gives the field at the selection, which `setField` then changes. `dateFormats(locale)` lists a date
control's display formats. Mounted, `hitTest`, `dragOver`, `dragLeave` and
`drop` let a field list beside the editor drag fields in. `data.list()` and `data.get()` read the custom XML parts;
the `writeDataPart` and `removeDataPart` commands change them.

## Rulers

Set `rulers: true` when mounting (`ShowHorizontalRuler` and `ShowVerticalRuler` on `DocumentEditor`) and the engine
draws the rulers and handles them: dragging indents, margins, tab stops and table boundaries, clicking to add a tab,
dragging a tab off to delete it, and double-clicking for the Tabs dialog. `ruler.setUnit` and `ruler.setTabSelector`
change the unit and what a click adds.

To draw your own ruler, leave them off, read `ruler.view()` (the model with its mapping to the viewport) and the
`rulerChanged` event, and call the tab, indent, margin and boundary commands. `ruler.model()` and `state.ruler`
give the model in points, also headless.

## View and gestures

Mounted, `view.revealSelection()` scrolls the selection into view, `view.setBackground(color)` sets the colour
around the pages for a dark theme (the paper stays white), and `view.visiblePages()` gives the pages on screen. The
zoom and formatting marks are the `setZoom` and `setViewOptions` commands.

`setViewOptions` also sets Word's Display options, each read back in `state.view`: `formattingMarks` (Show/Hide ¶,
Ctrl+Shift+8) or each mark on its own (`tabCharacters`, `spaces`, `paragraphMarks`, `hiddenText`, `optionalHyphens`,
`objectAnchors`); `clickAndType` (a double-click in blank space puts the caret there, adding paragraphs, a tab stop or
the alignment it needs; `setClickAndTypeStyle` picks the style of the paragraphs it adds); `textDragAndDrop`; and
`hideWhiteSpace` (each page shows only its body, the pages meeting at a line; a double-click between pages toggles it,
and `visiblePages()` gives the shown bands). Showing hidden text lays the document out again; exports print it only
with `hiddenText` in the PDF options.

`selectionBounds()` gives where the selection is on screen (each line's or cell's box, each object's frame, and the box
around them), for a floating toolbar; ask again on `stateChanged` and after a scroll. `setDefaultPictureWrap` is Set as
Default Layout: the wrap pictures inserted later take.

**Dragging out.** When a drag of selected text leaves the view, call `dragOut()` and start the platform's drag with its
`data` (plain text, HTML and the engine's `internal` token); `dragEnd('move')` deletes the text as one step when the
drop target moved it, and `dropDragged(at, copy)` handles it coming back.

The engine recognises every pointer gesture (double-click in a header, a triple-click, dragging a table border).
`gestures.bindings()` lists them, and `gestures.set(id, action)` changes one: `'default'`, `'emit'` (emits
`gesture` for you to handle), `'none'`, or an op to run instead. The canvas's own gestures are bindable too: double-clicking
an object, a note reference, blank space (Click and Type) or the gap between pages, dragging a sizing, shape or table
move handle, and clicking a table's selection zone or insert mark. `textCtrlClick` is a Ctrl+click on text: by default
it selects the sentence, or adds it to the selection (takes it out when it is selected). Ctrl+drag and Alt+drag are
selection drags, not rebindable gestures (see [Editing on the canvas](editing.md#selecting)).

## Shortcuts and accessibility

`shortcuts.register` claims a key for your application, and pressing it emits `shortcut`. A key a command or another
registration already uses fails with `'unavailable'`, naming the binding. `shortcuts.lookup(keys)` says what a key
does now. The mounted editor keeps its own accessibility DOM for screen readers; `accessibility.tree()` gives the
document's reading structure, `accessibility.go(id)` puts the caret at a node, `accessibility.setAnnouncements` picks
what it announces, and `announcement` events say what it announced. [Accessibility](accessibility) says what screen
reader users get and what your application does. `wordCount()` counts words,
characters, paragraphs, lines and pages on request.
