---
title: Lists
sidebar_position: 16
---

# Lists

The editor models the list galleries and dialogs, and your application draws them, as with the other dialogs (see
[Services](services.md)). Each list edit is one undo step, and the lists save in the document's numbering part. A
definition edits only what changed, so what the dialogs don't show stays as it was written.

## A list level

A level (`ListLevel`) is what the Define dialogs edit:

| Value | What it is |
|---|---|
| `format` | The number style: `decimal`, `upperRoman`, `lowerRoman`, `upperLetter`, `lowerLetter`, `ordinal`, `cardinalText`, `ordinalText`, `decimalZero`, `bullet` or `none`. |
| `text` | The number format, with `%1` to `%9` for the numbers of this level and the levels above it (`Section %1.%2`). For a bullet, its character. |
| `start` | The number it starts at. |
| `alignment`, `numberPosition`, `textIndent` | How the number aligns, where it is, and where the text is, from the margin. |
| `follow`, `tab` | What follows the number (`tab`, `space`, `nothing`), and the tab stop after it. |
| `restartAfter` | The level after which it restarts; 0 never. |
| `legal` | Legal style numbering: every level's number in Arabic numerals. |
| `linkedStyle` | The paragraph style linked to the level. |
| `font`, `picture` | The number's or bullet's font, and a picture bullet's image. |

## Galleries

`lists.galleries(kind)` (.NET: `Lists.GalleriesAsync`) gives the bullet, numbering or multilevel gallery in sections:

- the built-in library (without its picture bullet);
- the lists applied in this session;
- the document's lists of that kind;
- for multilevel lists, the document's list styles.

Each entry has an `id`, a short `label` and its `levels`. `lists.preview(id, size)` draws an entry as PNG.

The `applyListEntry(id)` command applies an entry:

- a bullet or numbering entry changes the current level of the selection's list;
- a multilevel entry changes every level of it;
- outside a list, either entry starts a new list;
- a document list or list style continues that list.

## The dialogs

| Dialog | What it applies |
|---|---|
| `defineBullet` | The selection's level as a bullet: its character and font, or a picture, and its alignment. |
| `defineNumberFormat` | The selection's level's number style, format, font and alignment. |
| `defineMultilevelList` | Every level, for the `wholeList`, `thisPointForward` (a new list from the paragraph) or the `currentParagraph`; `listName` is the ListNum field name. |
| `setNumberingValue` | Start a new list at `value`, or continue the previous list, skipping to `value` with `advance`. It opens on the paragraph's number. |
| `adjustListIndents` | The level's number position, text indent, what follows the number, and its tab stop. |

The Define dialogs have a preview (`dialogs.preview`). Outside a list, the Define dialogs start a new list, and the
others are `unavailable`. The same edits are commands: `setListDefinition`, `setNumberingValue`, `restartNumbering`,
`continueNumbering` and `adjustListIndents`.

On the style dialog, `openFormat('bulletsNumbering')` sets the list the style's paragraphs are in: a gallery entry
(`gallery`, `entry`), or none. The list is made when it applies to the style dialog.

## Picture bullets

A picture bullet draws its image at the text's size, on screen and in PDF. A level's `picture` (PNG, JPEG or GIF)
makes one; the image is saved with the numbering part. An image the engine can't decode draws as an empty box and
warns `picture-bullet-undecodable`.

## A symbol picker

Define New Bullet's Symbol button is your application's symbol picker:

1. `lists.fontCharacters(family)` (.NET: `Lists.FontCharactersAsync`) lists the characters a font has, as runs of
   code points.
2. `lists.characterPicture(family, character, size)` draws one, to fit a tile.
3. The chosen character and font are the dialog's `text` and `font`.
