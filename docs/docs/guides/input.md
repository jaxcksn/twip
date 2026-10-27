---
title: Input and commands
sidebar_position: 5
---

# Input and commands

The editor takes raw input from your app (keys, text, pointer events) and turns it into editing: caret movement,
selection, typing, the keyboard shortcuts. Commands do the rest: formatting, tables, fields and content controls.

## Keys and text

Pass each key press with its DOM `KeyboardEvent.code` name (`KeyB`, `ArrowLeft`, `Enter`, `Backspace`, ...) and the
modifiers held. `Key` returns `true` when the editor used it (stop the platform's default then). Pass typed or
composed text (an IME's committed text) separately with `Text`.

```js
canvas.addEventListener('keydown', (e) => {
  const mods = { ctrl: e.ctrlKey, shift: e.shiftKey, alt: e.altKey, meta: e.metaKey };
  if (editor.key(e.code, mods)) e.preventDefault();
});
canvas.addEventListener('beforeinput', (e) => {
  if (e.data) { editor.text(e.data); e.preventDefault(); }
});
```

On .NET map your UI framework's key to the same names (`VirtualKey.Left` → `ArrowLeft`, `Key.B` → `KeyB`).

## Pointer

Send pointer down, move and up with content-pixel coordinates (see [Drawing pages](rendering)) and the click count.
`Pointer` returns `true` when the editor wants the pointer captured (a drag).

```csharp
if (editor.Pointer(PointerKind.Down, x + scrollX, y + scrollY, clickCount, mods)) CapturePointer();
```

## Clipboard

`Copy(cut)` returns the selection as text, HTML and a token, or nothing when no text is selected. Put the text and
HTML on the system clipboard and keep the token; when the user pastes, pass the clipboard's text and the token back:
the editor pastes its exact content when the token is its own, else the text.

```csharp
using var clip = editor.Copy(cut: false);
if (clip != null) SetClipboard(clip.Text(), clip.Html(), clip.Token());
editor.Paste(ClipboardText(), lastToken);
```

## Commands

Formatting, paragraph, table, picture, field and content-control commands are methods on the editor, one per
command: `Bold()`, `Italic()`, `SetFontSize(12)`, `SetAlignment(Alignment.Center)`, `InsertTable(3, 4)`, `MergeCells()`,
`InsertPageBreak()`, `SetTableWidth(50, TableWidthUnit.Percent)`, `Undo()`, `Redo()`, and about a hundred more.
They act on the selection, like their buttons in a word processor. The SDK's reference (IntelliSense, or the
TypeScript declarations) lists them all with their parameters.

## Selection and status

`Selection()` gives the selection's kind and, for text, its anchor and focus as paragraph ids and character offsets;
`SelectRange(paragraph, start, end)` selects within a paragraph. `Status()` has the caret's page, the page count, the
word count and the zoom. `AccessibleTree()` is the document's reading structure for screen readers, and
`Elements(kind)` lists its fields and content controls.
