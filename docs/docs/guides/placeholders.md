---
title: Placeholders
sidebar_position: 7
---

# Placeholders

With `placeholderUi` on, the editor draws a template's placeholders as editor UI instead of plain text: merge fields
and controls showing placeholder text, repeating sections, sub-template references and picture placeholders. It is a
view setting. Nothing it draws is saved, printed or exported, and with it off the editor draws exactly as before.

```ts file=../../examples/web/placeholder-ui.ts
```

```csharp file=../../examples/dotnet/PlaceholderUi.cs#ui
```

## Field marks

In design mode, each merge field (a field with no evaluated kind, such as `MERGEFIELD`) and each control showing its
placeholder text gets a field mark: a tint in the accent behind its text, one rounded segment per line, so a field
that wraps keeps wrapping where it prints. The field's text and its label (`setFieldLabels`) stay as they are, and
lines break exactly as with the marks off. Page numbers, dates, references and other fields the engine evaluates get
no mark.

A hovered field adds a 1 px outline and a caption under its last line: `{ }`, its name and a type. The type defaults to
the field type (`MERGEFIELD`); `setFieldTypes` sets your own per field code, as `setFieldLabels` does for labels. A
selected field (a click selects all of it) gets a stronger tint and a 1.5 px outline; typing replaces it and Backspace
deletes it, each one undo step.

## Hover and selection

Two events tell your application what the user is on, so it can draw its own popover or highlight its own list:

- `placeholderHovered`: the pointer started or stopped being over a placeholder.
- `placeholderSelected`: the selection started or stopped covering exactly one placeholder.

Each sends a `PlaceholderTarget` (`null` when it ends): its element, kind, label, key, code, type text, whether it is
unmapped, a sub-template reference's resolution, and `anchor`, a rectangle in the same space as a popover's. An
anchor that moves with scrolling, zooming or layout is sent again; the same one isn't.

`highlightElement` draws a placeholder hovered until you clear it (`null`), for example while your field list hovers
it. With a placeholder selected, Tab and Shift+Tab (`selectNextPlaceholder`, `selectPreviousPlaceholder`) move to the
next or previous one in document order, and Esc puts the caret after it. Both keys can be rebound.

## Repeating sections

Every repeating section shows a 2 px bar in the left gutter, 8 px further out for each section around it, and a tab
(`each <title>`) where it starts: in a band of room kept above it in design mode, or beside the first row when the
section starts at a table's later row. Rows of a section are tinted, and a press on the tab selects the section. The
band is design mode's only: the output is laid out without it.

## Sub-templates

A collapsed sub-template reference (`setSubTemplateView`) shows in design mode as a card: its name, `sub-template`,
its version and its field count, and Expand. A reference whose sub-template is missing (or only nearly named) shows
a card saying so. A shown body keeps a dashed bar and a tab in every mode, so it never reads as template text: its
name collapses it, and Open edits it, as a double-click does.

`setSubTemplateVersions` gives each sub-template's version text and, optionally, a newer version's notice. A press on
the notice raises `subTemplateUpdateRequested` with the sub-template's name and category; the update is yours.

## Input blocks

Picture placeholders (signatures, initials, fingerprints, photos) become input blocks in every mode: a tinted block
with a label chip, its hint on hover, and buttons when selected. `setPlaceholderLooks` sets each block's look by
element, tag, key prefix or default, as before, and each rule can now add:

- `hint`: what the block says on hover (default: "Click to add");
- `actions`: the buttons a selected block shows, in order; the `primary` one is filled.

With the placeholder UI on, a drawn look's `fill` is the block's base colour: its tint, outlines and primary button
are derived from it, and lightened on a dark page. A press on a button raises `placeholderActivated` with `action`
set to the button's id; a block without buttons activates on a press, as before, and Enter presses the primary
button. A filled block shows the picture; hovered, its tab shows the date it was filled this session and Replace
(`action: "replace"`).

## Unmapped and conditional fields

`setUnmappedFields` names the fields (by their first argument) your data source lacks: their marks turn neutral and
struck through, and their events say `unmapped`. An `IF` field shown as its result is tinted, and shows its condition
and its else text in a tab while hovered or selected.

## Screen readers

Selecting a placeholder says what it is ("Field: Email, merge field", "Sub-template: Appeals notice, collapsed").
Captions, tabs and card text are never read: fields read as their text.
