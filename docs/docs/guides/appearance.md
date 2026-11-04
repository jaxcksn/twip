---
title: Appearance
sidebar_position: 11
---

# Appearance

The editor's chrome (the desk around the pages, the rulers, handles, guides, readouts and popovers) has a light and a
dark theme, an accent colour, and a high-contrast theme for the platform's high contrast mode. The page itself has its
own setting, dark mode. None of it reaches the document: saving, PDF export and printing always use the document's
colours.

```ts file=../../examples/web/appearance.ts#appearance
```

```csharp file=../../examples/dotnet/Appearance.cs#appearance
```

## Theme and accent

`setViewOptions({ theme, accent })` sets the chrome's scheme, `light` (the default) or `dark`, and its accent. The
accent is any colour; the default is azure, `#0A74E4`. It colours the selection, handles, guides, active markers and
the popovers' current choices. The dark theme draws a lighter, less saturated variant of it so it holds against dark
surfaces, and the state reports what's drawn: `view.accent`, and `view.accentOn` for text on it. Use those two for
chrome of your own that should match the editor's.

On the web, `editor.view.setTheme(theme, accent?)` is the same command. In .NET it's `EditorSession.SetThemeAsync`, or
the `Theme` and `Accent` properties of `DocumentEditor`.

The theme also gives the overlay colours (caret, selection, object outline, drop caret, formatting marks and leader
lines) and the desk colour. `setBackground` and `setOverlayColors` still override them. A background set that way
stays when the theme changes, and so do overlay colours: set all six for a dark theme. Until the application sets a
theme, a dark background also gives the dark theme, as it gave dark rulers before.

To follow the theme again, `setBackground(null)` and `setOverlayColors(null)` put back its colours. In .NET they're
`EditorHost.ResetBackground` and `ResetOverlayColors`, or `DocumentEditor.ResetBackColor` and `ResetOverlayColors`.

## Dark mode

`appearance` is how the page is drawn on screen:

| Value | The page |
|---|---|
| `light` | in the document's colours (the default) |
| `dark` | dark, with the document's colours adjusted to it: white becomes near black and black near white, and the other text, highlight, shading and border colours move with them. Shapes and pictures keep their colours ([DRAW-27](../rulebook/draw.md#draw-27)) |
| `darkWithLightPage` | the chrome stays dark and the page shows in its own colours, as Word's *Switch Modes* does |

`setAppearance(appearance)` on the web view and `SetAppearanceAsync` in .NET wrap the same command. Dark mode moves
nothing: line breaks, page breaks and object positions are the same in every appearance.

## Measuring detail and the object grid

`detail` is how much the editor measures while you drag and hover. `quiet` shows a chip with the value; `standard`
(the default) adds dimension lines, and everything while Alt is held; `full` always shows everything, with the change
from the start, table sizes and status lines. `objectGrid` is when the dot grid objects snap to shows: `off`,
`whileDragging` (the default) or `always`. It isn't the `gridlines` option, which stays the table gridlines. See
[Editing](editing.md).

## High contrast

The engine never reads platform settings. The ready-made editors do it for you:

- **Web.** Under `forced-colors: active` the mounted editor sets the `highContrast` theme from the CSS system colours
  (`Canvas`, `CanvasText`, `Highlight`, `HighlightText`, `GrayText`) and a light page. It follows the mode as it
  changes. A theme or appearance set through `view.setTheme` or `view.setAppearance` meanwhile applies once the mode
  ends.
- **WinForms.** With `SystemInformation.HighContrast` on, `DocumentEditor` sets the `highContrast` theme from
  `SystemColors` (Window, WindowText, Highlight, HighlightText, GrayText) and a light page, and checks again when the
  system's preferences change.

An application drawing its own surface sets `theme: { highContrast: { canvas, canvasText, highlight, highlightText,
grayText } }` itself (`ChromeTheme.HighContrast(new HighContrastColors(...))` in .NET). The accent is then `highlight`,
and text on it `highlightText`.

## Popovers

The tab type picker, a tab stop's options and the AutoCorrect menu are popovers the engine asks for with
`popoverRequested`: a `kind`, the anchor rectangle (client px on the web), and what the popover offers. The ready-made
editors show them in the chrome's colours, keyboard-reachable (arrows, Enter, Esc) and announced as menus. An
application that listens for `popoverRequested` shows its own instead, the same rule as the context menu. It answers
with commands:

| Popover | Offers | Answered with |
|---|---|---|
| `tabType` | `current`, and `choices` with a name and a one-line description | `setRulerTabType(value)` |
| `tabStop` | the stop's `position`, `alignment` and `leader` | `addTabStop` at its position (changes the alignment or leader; the popover stays open), `deleteTabStop` |
| `autoCorrect` | `from` (what was typed) and `to` (what it became) | `revertAutoCorrect`, `neverAutoCorrect`, or your AutoCorrect dialog (see [AutoCorrect](autocorrect.md)) |

`popoverClosed` says the engine dismissed the open one: Esc, a press elsewhere, its target gone, or its answer
(except a `tabStop` change). Opening one closes the one open. When your popover closes on its own, run `closePopover`
so the engine's closes too; with none open it does nothing. From the keyboard, `showTabTypes`, `showTabStop` (the stop nearest the caret) and
`showAutoCorrectOptions` (Alt+Shift+F10) ask for them.
