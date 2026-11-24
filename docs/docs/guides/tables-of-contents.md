---
title: Tables of contents
sidebar_position: 15
---

# Tables of contents, captions and cross-references

Word's References tab is commands and two queries: the engine collects the entries, numbers the captions, keeps the
cross-references current and draws them; your application draws the gallery, the dialogs and the panes.

## Tables of contents

`insertTableOfContents()` puts Word's default table (headings 1 to 3 and outline levels, linked to their headings,
page numbers hidden in a web view) in place of the selection: one paragraph per entry in the TOC 1 to TOC 9 styles
(added when the document lacks them), page numbers from the finished layout. Its options are the Table of Contents
dialog's:

| Option | Collects or shows |
|---|---|
| `headingLevels: { from, to }` | paragraphs in the built-in heading styles of those levels |
| `outlineLevels` | paragraphs with an outline level, as their level |
| `styleLevels: [{ style, level }]` | paragraphs in these styles, at these levels |
| `entryFields: { identifier?, levels? }` | entries marked with `markTableOfContentsEntry` (Alt+Shift+O emits `hostCommand` `markTableOfContentsEntry` for your dialog) |
| `captions: { label, includeLabelAndNumber }` | captions of a label: a table of figures |
| `bookmark` | only what the bookmark marks |
| `hyperlinks`, `pageNumbers`, `separator`, `chapterNumbers`, `hideInWebView`, `keepTabs`, `keepLineBreaks` | how the entries read |

`layout` sets the right-aligned page numbers and their tab leader; `frame: { title, buildingBlock }` adds the title
and the building block control of Word's gallery tables (Automatic Table 1 is `{ title: 'Contents', buildingBlock:
true }`). `insertTableOfFigures({ label })` is the same for a caption label; `insertManualTableOfContents()` is Word's
Manual Table, with entries the user types. With the selection in a table of contents, inserting one replaces it (ask
first, as Word does).

`references.tablesOfContents()` lists the document's tables with their options (to prefill your dialog from the
table at the caret), whether each is locked, and `here`, the one the selection is in; ask on selection change.
`references.tocEntries(options)` previews what a table would collect. `updateTableOfContents({ mode, index })`
updates the page numbers (`pageNumbers`, entry edits kept) or every entry (`entireTable`);
`updateAllTablesOfContents(mode)` updates them all, as before printing, and says how many; `removeTableOfContents()`
removes one with its title and control. F9 in a table of contents emits `tableOfContentsUpdateRequested` instead of
updating: ask the user which update they want and answer with its `index`. Alt+F9 shows a table's field code as
Word does, and showing codes is never an undo step. `setFieldsLocked({ locked })` with the selection in a table locks
it (Update refuses it, Remove still works), and `unlinkFields()` turns it into text in one undo step.

## Captions

`insertCaption({ label, text })` captions the selected table (above it), picture or object (below it), or the caret's
paragraph: the label, its sequence number and your text (put your own separator first: `': Totals'`). `position`,
`excludeLabel`, `format` (decimal, letters or roman numerals) and `chapter: { level, separator }` are the Caption
dialog's; inserting renumbers the label's other captions. `references.captionLabels(custom)` lists Equation, Figure
and Table, your own labels, then the ones the document's captions use: labels are yours to keep, and Delete Label is
your list dropping one. `setAutoCaption(options)` captions every table inserted from then on (`null` turns it off);
`state.autoCaption` reads it back. It is a session setting, never saved.

## Cross-references

`references.crossReferenceTargets(kind)` lists what a cross-reference can point to, as Word's dialog lists it:
`numberedItem`, `heading`, `bookmark`, `footnote`, `endnote` or `{ caption: label }`.
`insertCrossReference({ target: { kind, index }, content })` inserts one to the `index`-th of that list, showing its
`text`, `pageNumber`, `number` (relative, without context or with full context), `aboveBelow`, `noteNumber`,
`labelAndNumber` or `captionText`; `hyperlink` and `aboveBelow` are the dialog's check boxes. The engine bookmarks the
target (a hidden `_Ref` bookmark, also in a protected document) and evaluates the field; `updateFields` refreshes
REF, PAGEREF, NOTEREF, SEQ and STYLEREF itself, with no `fieldsUpdateRequested` for them.

Under document protection these commands run where the selection may be edited: an editable range for read only and
comments, an open section for filling in forms; an update or removal needs the whole table editable, and
`updateAllTablesOfContents` leaves the tables it may not change. Tracked changes record them as insertions and
deletions of whole fields.
