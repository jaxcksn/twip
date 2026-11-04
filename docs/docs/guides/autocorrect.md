---
title: AutoCorrect
sidebar_position: 12
---

# AutoCorrect

Typing corrects as it goes, as Word's default AutoCorrect does. Only typed input is corrected: text inserted through
the API (`insertText`) and pasted text never are. Each correction is its own undo step after the typing's, so Ctrl+Z
(Cmd+Z) right after one undoes only the correction. With track changes on, a correction is tracked as an ordinary
deletion and insertion.

```ts file=../../examples/web/appearance.ts#autocorrect
```

```csharp file=../../examples/dotnet/Appearance.cs#autocorrect
```

## The rules

Every rule is on by default. `state.autoCorrect` reports them.

| Option | Corrects |
|---|---|
| `twoInitialCapitals` | `THe` → `The` (the initial-caps exceptions such as `IDs` stay) |
| `sentenceCapitals` | the first letter of a sentence (not after an abbreviation in the first-letter exceptions, such as `e.g.`) |
| `cellCapitals` | the first letter of a table cell |
| `dayNames` | `monday` → `Monday` |
| `replaceText` | the replacement list and the symbols: `teh` → `the`, `(c)` → `©` |
| `smartQuotes` | straight quotes to curly ones |
| `ordinals` | `1st` with the suffix raised |
| `fractions` | `1/2` → `½` |
| `dashes` | hyphens to en and em dashes |

A correction runs when you finish what it corrects: a space, punctuation, Tab or Enter after it.

## Options

`setAutoCorrect(change)` switches the rules given and replaces the lists given. It's a session setting, not a document
change, so it has no undo step and isn't saved in the document.

- the rules above, each `true` or `false`;
- `entries: [{ from, to }]`, in place of the shipped replacement list;
- `firstLetterExceptions` and `initialCapsExceptions`, in place of the shipped exception lists;
- `never`, the texts never corrected (what *Never correct* adds to). `state.autoCorrect.never` lists them; an
  application that keeps them between sessions sets them again at start.

## Revert and Never

While the caret is at a correction, the corrected text has a dashed accent underline and the engine draws a small
options button by it. A click on the button, or Alt+Shift+F10 (`showAutoCorrectOptions`), asks for the AutoCorrect
menu: `popoverRequested` with `kind: 'autoCorrect'`, `from` (what was typed) and `to` (what it became). The menu offers:

| Entry | Does |
|---|---|
| Revert to *from* | `revertAutoCorrect`: puts back what was typed. Right after the correction it undoes it (`documentChanged` with origin `undo`); later it is one undo step of its own |
| Never correct *from* | `neverAutoCorrect`: reverts it as Revert does, adds *from* to `never`, and announces "Won't autocorrect *from* again" |
| AutoCorrect rules… | the application's own dialog: `dialogRequested` with `kind: 'autoCorrect'`. The engine supplies no values for it; build it on `state.autoCorrect` and `setAutoCorrect` |

The commands are available while there's a correction to act on (`state.commands.revertAutoCorrect.enabled`). The
mark goes once you type on or move away.

The ready-made editors show the menu in the chrome's colours (see [Appearance](appearance.md#popovers)). An
application that listens for `popoverRequested` shows its own.
