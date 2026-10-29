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

## Dialogs

`dialogs.open(kind)` returns a dialog session: the selection's current `values` (`'mixed'` where they differ), and
for each field its label, kind, range and options. `update(patch)` recomputes dependent values; `apply()` validates
and commits everything as one undo step; `cancel()` changes nothing. The dialogs are `font`, `paragraph`,
`pageSetup`, `insertTable`, `tableProperties`, `pictureSize`, `altText`, `hyperlink`, `tabs` and `accessibility`.

## Find and replace

`find.next` and `find.previous` select and reveal the next match; `find.all` lists every match with its context and
page, for a results pane; `replace` replaces the current match and finds the next; `replaceAll` is one
transaction. Matching uses the text of the current markup view.

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

A custom .NET host supplies the system clipboard through `IEditorHostAdapter.SetClipboard` and
`GetClipboardAsync` (see [hosting](dotnet#hosting-a-session-in-another-ui-framework)).

## Picture placeholders

A picture content control with no picture shows a placeholder. `setPlaceholderLooks` decides how each looks (a fill,
a label and an SVG icon, or an image), matched by element id, by tag, by tag prefix, or by default. Each call
replaces the looks set before, unless `merge: true` adds to them. It is a view setting: no undo step, and nothing is
written to the document.

A click on an unfilled placeholder selects it and emits `placeholderActivated` with its key (the control's tag) and
element. `fillPlaceholder({ image })` fills the selected placeholder, and `fillPlaceholder({ key, image })` fills
every unfilled placeholder with that key.

```ts file=../../examples/web/placeholders.ts
```

```csharp file=../../examples/dotnet/Placeholders.cs#placeholders
```

## Navigation and styles

`navigation.headings()` lists the document's headings with their level and page, for a navigation pane, and
`navigation.select(id)` goes to one. `styles.list()` lists the styles with what a style gallery needs: name, type,
whether it's quick, priority and a preview of its formatting.

## Elements and data parts

For merge designers: `elements.list()` lists fields and content controls in document order, `get(id)` gives one
in full (lock, placeholder, data binding, options, children), `structure()` gives the body as blocks, and
`select(id)` selects one, entering a header or footer when needed. Mounted, `hitTest`, `dragOver`, `dragLeave` and
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

The engine recognises every pointer gesture (double-click in a header, a triple-click, dragging a table border).
`gestures.bindings()` lists them, and `gestures.set(id, action)` changes one: `'default'`, `'emit'` (emits
`gesture` for you to handle), `'none'`, or an op to run instead.

## Shortcuts and accessibility

`shortcuts.register` claims a key for your application, and pressing it emits `shortcut`. A key a command or another
registration already uses fails with `'unavailable'`, naming the binding. `shortcuts.lookup(keys)` says what a key
does now. The mounted editor keeps its own accessibility DOM for screen readers; `accessibility.tree()` gives the
document's reading structure, and `announcement` events say what it announced. `wordCount()` counts words,
characters, paragraphs, lines and pages on request.
