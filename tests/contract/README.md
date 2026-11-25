# Contract scripts

Each `*.json` file here is one scripted scenario. The same script runs against every first-class surface, and they
must all give the same results:

| Runner | Where | Modes |
|---|---|---|
| `rust` | `crates/twip/tests/contract.rs` | the `twip::api` engine, directly (it takes `key` and `text`) |
| `web-main` | `bindings/web/test/contract.test.ts` | `@twip/web` headless, engine on the calling thread |
| `web-worker` | `bindings/web/browser/worker-contract.spec.ts` | `@twip/web` mounted in Chromium, engine in a worker |
| `dotnet-unattached` | `bindings/dotnet/Twip.Tests/ContractTests.cs` | an `EditorSession` with no host |
| `dotnet-attached` | the same | an `EditorSession` attached to a headless test host |

A failure names the runner, the script and the step index. Every runner also checks that each `documentChanged`
arrives with the state already at its revision.

## Coverage

| Area | Checks | Scripts |
|---|---|---|
| Ordering | concurrent commands, commands from inside listeners | `concurrent-order`, `listener-commands`, `events-order`, `burst` |
| State consistency | the state at every event equals the event's revision; mixed values | every script (the runners' `documentChanged` check), `mixed-formatting` |
| Transactions | origins (api, undo, redo), batches atomic and rolled back on error, undo grouping, read only | `origins`, `bold-undo-redo`, `batch`, `undo-grouping`, `read-only`, `author` |
| Documents | failed open keeps the document; abort open and export; export captures its revision; modified and saved | `documents`, `abort`, `open-type-export` |
| PDF options | an open password encrypts the export, an empty one is rejected; keywords reach the metadata | `export-pdf-protected`, `export-pdf-keywords` |
| Errors | every error code reachable | `errors`, `invalid-arguments`, `unavailable-commands` |
| Services | context menu actions, dialog apply and cancel, find and replace counts, spelling add and ignore, the catalog, navigation, the ruler | `context-menu`, `dialogs`, `find-replace`, `spelling`, `catalog`, `navigation`, `ruler` |
| Review | track, accept and reject; comments add, reply, resolve and delete; Show Markup, reviewers, balloon mode, markup style, track options, counts, accept shown, delete all comments | `review`, `author`, `review-markup` |
| Outline | promote and demote, by selection and by heading, with keys; move, delete and insert headings; current heading; find results by heading; page thumbnails and versions | `outline`, `outline-keys` |
| Fields, controls and forms | Insert Field's catalogue and composing; field view and staleness; link ScreenTip, frame and bookmark; notes list and separators; number formats; page number samples; column scopes and presets; date controls, gallery controls, repeating items; form field settings, Tab and F4 | `fields-api`, `controls-api`, `form-field-keys`, `drop-cap-dialog` |
| Protection | Restrict Editing: passwords twice, formatting restrictions, editable ranges by group and person, editing groups, Find Next Region | `protection`, `protection-api` |
| Selection | ranges read back and set, discontiguous selection with blocks of cells, shrink, similar formatting, style instances; Ctrl gestures; vertical text's arrows; ranges and cells of different stories refused | `selection-ranges`, `multi-selection`, `vertical-text`, `selection-stories` |
| Objects | names, alt text titles, hiding, stacking and selecting by id; wrap points; reflection, upright text, picture fills; picture recolour and reset; image queries; linked text boxes; Select Objects; frames that keep apart | `objects`, `selection-pane`, `text-box-links`, `select-objects`, `frame-overlap`, `wrap-points` |
| Tables | Draw Table and the Eraser, by command and by pointer; insert cells; default cell margins; row exceptions; convert options and their preview size | `table-draw`, `table-tools`, `table-cells`, `table-convert` |
| Find | `^` codes, matches across paragraph marks, regular expressions and groups, wrap, prefix and suffix, ignoring punctuation and white space, formatting, in the selection, Highlight All, tracked Replace All, Go To | `find-advanced`, `go-to` |
| Clipboard | pictures pasted and copied, Paste Options and repaste, the paste options, Paste Special's choices and table choices, copies drawn as HTML, Insert Text from File | `paste-image`, `paste-options`, `paste-special`, `copy-object`, `clipboard-html`, `insert-file` |
| References | tables of contents and of figures, updating (F9's question; page numbers keep edited entries), locked tables, field codes shown, removing, Manual Table, marked entries, captions and AutoCaption, cross-references, protection | `toc`, `captions`, `mark-entry`, `cross-references`, `protection-toc`, `reference-keys` |
| Fonts | substitution, unresolved reporting, late font arrival | `fonts` |
| Cleanup | destroy is idempotent, calls after destroy | `cleanup` |

`pending.txt` lists the scripts whose features haven't landed, one name per line. Every runner still runs them, and
reports each as skipped while it fails; a listed script that passes fails the run, so the lane that lands a feature
deletes its script's line. `check.py` checks that every listed name is a script here.

## Format

```json
{
  "name": "bold-then-undo",
  "fixture": "text-basics.docx",
  "fonts": "kit",
  "steps": [
    { "op": "open" },
    { "op": "command", "command": { "type": "selectAll" } },
    { "op": "command", "command": { "type": "setBold", "value": true } },
    { "op": "expectEvents", "match": [ { "type": "documentChanged", "origin": "api" } ] },
    { "op": "command", "command": { "type": "undo" } },
    { "op": "expectState", "match": { "history": { "canRedo": true } } }
  ]
}
```

- `fixture` names a file relative to `crates/twip/tests/fixtures/`. Without one, the session starts blank.
- `fonts` is `"kit"` (the default) or `"empty"`. The kit is the test font kit `crates/twip/tests/fonts/kit.json`
  lists, its faces and aliases, so every runner lays out with the same fonts; `"empty"` is an empty font library, for
  scripts that need no font.
- Commands are the batch op shape of `@twip/web`: the command id as `type`, and a single argument as `value`.
- State and event values are the camelCase wire shapes of `twip::api`.

## Steps

| Step | Does |
|---|---|
| `open { match? }` | opens the script's fixture. `match` is checked against the result, `{ warnings }` |
| `openBytes { base64 }` | opens the given bytes |
| `reopen` | opens the bytes of the last `docx` export |
| `export { format, pdf?, match?, contains? }` | exports `docx` or `pdf`, and records the revision it reports. `pdf` holds a PDF's options (`openPassword`, `keywords`). `match` is checked against `{ revision, warnings, head }`, where `head` is the first four bytes as Latin-1 text (`"%PDF"`). `contains` lists Latin-1 strings the bytes must contain (`"/Encrypt"`) |
| `command { command }` | runs one command and waits for it |
| `batch { commands, label? }` | runs an atomic batch |
| `key { key, mods? }`, `text { text }` | input, on surfaces that take it. `key` is a DOM `KeyboardEvent.key` value (`"s"`, `"F7"`, `"ContextMenu"`), and `mods` lists `ctrl`, `shift`, `alt` or `meta` |
| `click { text, mods?, clicks?, button? }` | input: clicks the middle of the first occurrence of `text` in the document; `clicks` (1 to 3) presses that many times, each counting the ones before, and `button` is `primary` (the default) or `secondary` |
| `hover { text }` | input: moves the pointer, no button down, to the middle of the first occurrence of `text` |
| `drag { from, to?, by?, release?, mods?, button? }` | input: presses at `from`, moves in 8 equal steps to the end (the button held, `mods` on every event) and releases there. `from` is `{ text, select? }`, the text's middle (`select: true` then selects the text, so the drag takes it), or `{ tableColumn: { text, border } }`: a click on `text`, then the ruler's column border `border` of the caret's row (`horizontal.origin + model.horizontal.tableColumns[border] * horizontal.scale`) at the caret's middle height. The end is `to: { text }`, where the first occurrence of the text ends (a place between characters, the same in every runner), or `by: { dx?, dy? }` in points (times `horizontal.scale`). With `release: false` the button stays down for `expect*` steps; a `drag` while one is held fails the script |
| `release` | input: releases the held `drag` where it ended; with none held it fails the script |
| `call { method, args?, result?, save? }` | calls a session member by its TypeScript path, with `args` as its arguments, and waits for it: the state (`getState`), a setting (`setReadOnly`, `setAuthor`), a service (`find.next`, `menus.contextMenu`, `dialogs.open`), a command with its result (`commands.addComment`), a property (`catalog`), or the session's font library (`fonts.setReplacements`). `result` is a partial match against what it returns, and `save` keeps that under a name. A `method` of `@name.member` calls a member of a saved result (`@font.update`, `@font.apply` on a saved dialog) |
| `font { file }` | adds a font file (relative to the fixtures folder) to the session's library |
| `abort { late? }` | the next `open`, `openBytes`, `reopen` or `export` is cancelled (the operations that take a cancellation). With `late: true`, the cancellation arrives only after that operation commits, and must be ignored |
| `expectState { match }` | a partial deep match against the current state |
| `expectEvents { match }` | an ordered partial match against the events since the last `expectEvents` (see below) |
| `expectError { code }` | the previous step failed with this error code |
| `destroy` | ends the session; every later step expects `destroyed` |
| `concurrent { steps }` | submits the operation steps without waiting, then waits for all of them. It fails with the first that fails; listeners' commands run after them |
| `listen { type, command }` | registers a listener that submits `command` once, the first time `type` fires. A failure of `command` fails the script |
| `repeat { count, step }` | runs `step` `count` times, resolving its values on each run |

Finding the text a `click`, `hover` or `drag` points at moves the selection: the runner puts a selection of text, or
of several ranges and blocks of cells, back (`setSelection`) before it sends the pointer, so a Ctrl or Shift gesture
goes on from the selection the script made. A caret or an object isn't put back.

A script with a `key`, `text`, `click`, `hover`, `drag` or `release` step runs only on surfaces that take input. Before each `expect*` step the
runner waits until the session is idle, including commands that listeners submitted. A step that fails must be
followed by `expectError`.

## Matching

A partial match compares only the keys the match names. Arrays match element by element, and an array in a match
may be shorter than the value, comparing its leading elements.

`expectEvents` matches its entries, in order, against a subsequence of the events since the last `expectEvents`: each
entry matches a later event than the one before it, and events no entry matches are skipped (surfaces coalesce
`selectionChanged` differently, and fonts give machine-dependent warnings). An event is its `TwipEvents` payload with
its name added as `type`. To check that nothing happened, match the state's `revision`.

Four objects in a match test the value differently:
- `{ "$exact": value }` matches only an equal value, so `{ "$exact": [] }` matches only an empty array. Numbers
  compare by value, and a missing key equals null;
- `{ "$has": [match, ...] }` matches an array with a different element for each match, in any order. Elements of
  nested `items` arrays (submenus) count too;
- `{ "$type": "string" }` matches any value of that JSON type (`string`, `number`, `boolean`, `array`, `object`,
  `null`);
- `{ "$contains": "text" }` matches a string that holds `text`.

## Values

Two objects stand for a value anywhere in a step (arguments, matches):
- `{ "$file": "dictionaries/en_US.aff" }` is the bytes of that file, relative to the fixtures folder;
- `{ "$saved": "menu", "path": ["items", { "label": "Ignore" }, "action"] }` is a result saved by `call`, or part of
  it. A path step is a key, an index, or a match, which picks the first element of the array (or of its nested `items`)
  that it matches.
