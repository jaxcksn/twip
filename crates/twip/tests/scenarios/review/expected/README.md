# Expected results for the review scenarios

These files were recorded from the reference word processor: each edit was made there through its object model and
the document was then saved. They are what the review scenarios compare against. Dates and revision ids are left out
because they change on every recording.

## The normalized document state

Wherever a file records a document (`initial`, `result`, `before`, `after_delete` and so on), it has this shape:

```json
{
  "stories": {
    "body": [ <block>, ... ],
    "header-default": [ ... ], "footer-first": [ ... ], "footnotes": [ ... ], "endnotes": [ ... ], "textboxes": [ ... ]
  },
  "sections": [ { "change": <author|null>, "props": [ <prop>, ... ], "was": [ <prop>, ... ] | null } ],
  "revisions": [ { "type": <revision type name>, "author": <name>, "text": <text>, "story": <story name, when not the body> } ],
  "comments": [ { "author", "initials", "text", "scope", "start", "end", "parent", "done" } ]
}
```

- **Stories** come from the saved file's parts. Headers and footers are named by kind (`header-default`,
  `header-first`, `header-even`, and the same for footers). Footnotes and endnotes are each one story: the paragraphs
  of every note in order, with the separators left out. Text box content found inside a story's runs goes to
  `textboxes`.
- A **block** is a paragraph or a table.
- A **paragraph** is:
  - `text`: the visible text, deleted text included.
  - `runs`: `[{ "text", "ins", "del", "move", "fmt", "rpr", "was" }]`.
    - `ins` and `del` are the authors of the insertion or deletion around the run, or `null`.
    - `move` is `"from"` or `"to"` inside a move, otherwise `null`.
    - `fmt` is the author of a formatting change on the run.
    - `rpr` holds the run's visible properties.
    - `was` holds the properties from before the formatting change (`null` when there is none).
    - Adjacent runs with equal flags and properties are merged, and runs with no text are dropped.
  - `mark`: `{ "ins", "del", "move" }`, the paragraph mark's revision authors.
  - `pchange`: the author of a paragraph property change. `pwas` holds the properties from before it.
  - `ppr`: the paragraph properties.
  - `num` and `numchange`: the numbering properties, and the author of a numbering change.
- **Text in runs**:
  - a tab is `\t` and a line break is `\n`;
  - a page break is `\f`;
  - a picture, object, text box anchor or note reference is U+FFFC;
  - a field shows its result, and its code is not included.
- A **table** is `{ "table": { "change", "tblpr", "grid", "grid_change", "rows" } }`:
  - `change` is the author of a table property change. `grid_change` is the author of a grid change, or `""` when
    the change records no author.
  - each row is `{ "ins", "del", "change", "ex_change", "trpr", "cells" }`. `ins` and `del` are the row's revision
    authors, `change` is a row property change, and `ex_change` is a change to the row's table exceptions.
  - each cell is `{ "ins", "del", "merge", "change", "vmerge", "span", "tcpr", "blocks" }`. `ins`, `del` and `merge`
    are the cell's insert, delete and merge revisions (`merge` is `{ "author", "vmerge", "orig" }`). `change` is a
    cell property change, `vmerge` is `"restart"` or `"continue"`, and `span` is the number of grid columns.
- **Properties** (`rpr`, `ppr`, `tcpr`, `trpr`, `tblpr`, section `props`) are sorted strings. Each is the element name,
  then `:attr=value,...` with the attributes sorted. Child elements follow in `[...]`, for example `jc:val=center` or
  `pStyle:val=Heading1`. Toggles that are off are left out.
- **Authors** are names. An empty string means a revision element with no author.
- **`revisions`** is the reference application's own list of revisions, story by story in document order. `type` is
  one of its revision type names: `wdRevisionInsert`, `wdRevisionDelete`, `wdRevisionProperty`,
  `wdRevisionParagraphProperty`, `wdRevisionTableProperty`, `wdRevisionSectionProperty`, `wdRevisionMovedFrom`,
  `wdRevisionMovedTo`, `wdRevisionCellInsertion`, `wdRevisionCellDeletion`, `wdRevisionCellMerge` and so on. `text`
  is its range's text. In that text, `\r` ends a paragraph, U+0007 ends a cell or row, U+0001 is a picture and U+0002
  is a note reference.
- **`comments`** is the application's comment list:
  - `scope` is the anchored text, and `start` and `end` are its offsets;
  - `parent` is the 1-based position of the comment being replied to;
  - `done` is whether the comment is resolved.

**Offsets** (`at`, `start`, `end`, `selection`) count characters of the story the way the reference application does:
deleted text is included, a paragraph mark is one character, and so is each cell-end and row-end mark.

## The files

| Files | Content |
|---|---|
| `record-*.json`, `own-insertion-delete.json` | One recording case: `description`, `setup` (the untracked starting document), `initial` (its state), and `steps`. Each step is `{ "do", "author", "op", "selection", "result" }`, plus `error` when the application refused the step. `op` is a machine-readable form of the edit, such as `{ "op": "type", "at": 6, "text": "new " }` or `{ "op": "delete", "start": 6, "end": 10 }`. Tracking is on for every step unless the op says otherwise |
| `record-*-picture.json` | Picture edits with tracking on. Each step also has `picture`: the size and crop before and after |
| `joined-paragraph-properties.json` | A deleted paragraph mark between two differently formatted paragraphs. It records the paragraphs each view presents, the rendered lines with and without markup, and the result of accepting |
| `resolve-*.json` | One accept or reject: `fixture`, `scope` (`one`, `range`, `all` or `author`), what was resolved (`revision`, `range` or `author`), `op`, `error`, and `result`. `resolve-recorded-move-*` resolve one end of a move the application recorded itself, and `before` is the state before resolving |
| `views-<fixture>.json` | Pages and lines per paragraph in each view. Each view says where its numbers come from (`source`). `layouts` holds the layouts of the tracked document, of every revision accepted and of every revision rejected. A paragraph's `lines` is `null` where it couldn't be measured: it crosses a page, or it sits in a table cell of an export |
| `change-bars.json` | Change bars measured in fixed-format exports with markup. Each bar records its offsets from the body's left edge to its left and right edges, its width, its vertical extent and its colour, in points |
| `author-colours.json` | The revision colour of each author and the order the colours are given in, from exports with markup. Each case ran in a fresh session |
| `body-level-markers.json` | Range markers placed between paragraphs: where each marker is in the file, and where it is after the application opens and saves the file |
| `comments-*.json` | Comment cases in the `record-*` step format. Each step also has `storage`: the comment parts written, and per extended comment entry whether it has a parent and whether it is done |
