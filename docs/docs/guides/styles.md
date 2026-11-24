---
title: Styles
sidebar_position: 15
---

# Styles

The editor models the style dialog, and your application draws it, as with the other dialogs (see
[Services](services.md)). `dialogs.openStyle` (.NET: `Dialogs.OpenStyleAsync`) opens it in one of two ways:

- `{ modify: id }` opens **Modify Style** on an existing style.
- `'create'` opens **Create New Style from Formatting**, filled from the selection: its paragraph's style as the base, and
  its paragraph's and first run's direct formatting as the style's. It starts in its short form, with only the name and
  the preview enabled. Setting `shortForm` to `false` gives the full dialog, as its "Modify..." button does, and keeps
  every value.

## The values

| Value | What it is |
|---|---|
| `name` | The style's name. Renaming a built-in style keeps its name and adds the new one as an alias. |
| `type` | Paragraph, character or linked (paragraph and character); chosen when creating. |
| `basedOn`, `next` | The style it is based on, and the style of the paragraph after one in it. |
| `font`, `size`, `bold`, `italic`, `underline`, `color` | The formatting shortcuts for characters. |
| `alignment`, `lineSpacing`, `spaceBefore`, `spaceAfter`, `leftIndent` | The shortcuts for paragraphs, disabled for character styles. The dialog's buttons that step spacing and indents set these values. |
| `quick`, `autoUpdate` | Add to the Styles gallery; Automatically update. |
| `description` | The style's formatting in words, as the dialog shows it under its preview. It is kept up to date on every change. |
| `pending` | The style as changed so far. Give it back unchanged: the Format menu's changes are in it. |

Each `update` reads the values again from the changed style, so dependent values and the description stay consistent.
Nothing in the document changes until `apply`.

## Applying

`apply` checks the name first. It refuses an empty name, one another style already has (as a name or an alias, compared
without case), and a built-in style's name (`invalid-argument`). Then it adds or changes the style as one undo step. A new
paragraph or linked style is applied to the selected paragraphs, and a new character style to the selected text. A
linked style also gets its character style, named `<Name> Char`. Every paragraph and run that uses the style, or a style
based on it, is formatted again.

Each style's `uses` counts how often the main document uses it (paragraphs, or stretches of runs for a character
style), and `selectStyleInstances(id)` selects them all, Word's Select All Instances.

## The Format menu

`openFormat(kind)` (.NET: `OpenFormatAsync`) opens a Format menu dialog on the style's formatting, not the
selection's: `font`, `paragraph`, `tabs`, `bordersShading`, `frame` and `bulletsNumbering`, and for a table style
`tableProperties` and `banding`. Borders and Shading on a character style shades its text. Frame on a style
without a frame starts from a new frame's values. Bullets and Numbering is in [Lists](lists.md). Its `apply` changes the style dialog and resolves with it; the document doesn't
change until the style dialog is applied.

## Table and list styles

A table style (`type: 'table'`) formats the parts of a table. `tablePart` chooses one: `wholeTable`, `headerRow`,
`totalRow`, `firstColumn`, `lastColumn`, the odd and even banded rows and columns, or a corner cell. The font,
emphasis, color and alignment values, and the part's `fill`, `cellAlignment` and `border` (one line on every edge),
show and set that part's formatting. `rowBand` and `columnBand` are the rows and columns in each band. Its Format
menu's Borders and Shading edits the part's cells (`applyTo: 'cell'`): their edges, inside lines (`between`,
`insideVertical`), diagonals (`diagonalDown`, `diagonalUp`) and shading. Table Properties sets its alignment, indent, cell margins, row breaking and cell
alignment, and Banding its band sizes. The preview
(`dialogs.preview`) is a 5 by 5 sample table with every part on. Applying a changed table style lays out every table
that uses it. A new one applies to the selected table.

A list style (`type: 'list'`) holds its list's levels in `listLevels` (see [Lists](lists.md)); `listLevel` is the level
shown. The levels are the dialog's until it applies. Then the style's own list (its definition linked to the style)
takes them, and its paragraphs number again. A new list style applies to the selected paragraphs.

## Updating styles from text

- `updateStyleToSelection(id)` changes a style to the selection's formatting: the paragraph's and first run's direct
  formatting become the style's, in one undo step.
- A style with **Automatically update** (`autoUpdate`) changes whenever paragraph formatting is applied to paragraphs in
  it: the style takes the formatting, and every paragraph in it follows. Applying another style to such a paragraph
  doesn't change the style.

## Styles without a dialog

The same work runs as commands and queries, for an application that draws its own style panes:

- `createStyle({ category, fromSelection, apply, change })` (.NET: `CreateStyleAsync(NewStyle)`) makes a style and
  resolves with its id (`{ styleId }`; .NET: the `StyleId`). `category` is `paragraph`, `linked`, `character`,
  `table` or `list`; `fromSelection` starts from the selection's formatting as Create New Style from Formatting does,
  and `apply` applies it, both in the same undo step.
- `modifyStyle({ id, change })` changes one; a built-in style the document doesn't define joins it.
- A change (`StyleChange`) gives what it sets, and `null` removes: `name` (a built-in style keeps its own and gets an
  alias), `basedOn`, `next` (`null`: the style itself), `link` (both sides; their old partners are unlinked, and a
  linked style's character formatting goes to its other half), `autoUpdate`, `quick`, `list` (`{ list, level }`, a
  `lists.definition()` id; every paragraph in the style numbers again), a list style's `listLevels`, `clear` (its own
  formatting goes first), and `formatting`: ops run on the style as on text. A character style takes character ops
  only, a list style none. A table style's whole table also takes `setCellShading`, `setShading` (cells or the table),
  `setCellBorders`, `setCellAlignment`, `setCellMargins`, `setRowCanBreak`, `setTableAlignment` and `setTableIndent`;
  `parts` formats its parts (`{ part, clear, formatting }`, no alignment or indent), and `rowBand`, `columnBand` its
  bands.
- Either is `invalid-argument` for an empty, taken or built-in name, a base of another type, itself or one based on
  it, formatting the style's type doesn't take, or a band out of 1 to 32767; `not-allowed` while formatting is
  protected.
- `styles.definition(id)` (.NET: `Styles.GetDefinitionAsync`) reads a style: its names and aliases, category, whether
  it is built in and defined, base, next, link, flags and list; `character`, `paragraph` and `tabs` as text in it
  resolves them, and `ownCharacter`, `ownParagraph` with only what the style sets itself (the rest `null`); Word's
  `description`; a table style's `table` (alignment, indent, fill, bands, and each of its 13 parts as its cells show
  it); a list style's `listLevels`. It is `null` for an id that is neither the document's nor built in.
- `styles.builtIn()` lists every built-in style, defined or not, with its list flags. `setStyleMeta` sets the flags of
  one the document doesn't define too, as its latent settings.
- `setDefaultFormatting(ops)` sets the document's defaults with any character and paragraph op but frames, as one undo
  step, and `styles.documentDefaults()` reads them as `styles.definition` reads a style.

## Managing styles

`dialogs.open('manageStyles')` (.NET: `Dialogs.OpenManageStylesAsync`) models Manage Styles:

| Tab | What it holds |
|---|---|
| Edit | The style list in `sort` order (`asRecommended`, `alphabetical`, `basedOn`, `byType`), or only the styles that show in lists with `recommendedOnly`, and the `selected` style's `description`. Its Modify and New Style buttons are `openStyle`; Delete is `deleteStyle`. |
| Recommend | Each style's `visibility` (`show`, `hideUntilUsed`, `hide`) and `priority`. |
| Restrict | Each style's `locked` flag. |
| Set Defaults | The document's default `font`, `size`, `color`, `spacing`, `lineSpacing` and `indents`. |

`apply` sets what changed as one undo step. The same changes are commands: `setStyleMeta({ id, visibility, priority,
quick, locked })` and `setDocumentDefaults({ ... })`. Import/Export isn't offered.

`deleteStyle(id)` deletes a style the document made, in one undo step: its paragraphs take the default paragraph style,
its runs lose it, and the styles based on it take its base. Built-in styles can't be deleted (`not-allowed`).

When a document's protection is enforced with formatting restricted, locked styles can't be applied and direct
formatting is refused (`not-allowed`). Turning protection on and off is outside the editor.
