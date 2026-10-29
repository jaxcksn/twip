# Editor scenarios

One JSON file per scenario, run by the native runner (`crates/twip/src/editing/tests/scenarios.rs`, `cargo test -p twip scenarios`).md §2: an L4 scenario and its native twin).

```json
{
  "id": "E2E-EDIT-01",
  "title": "What the scenario shows",
  "fixture": "FX-TEXT.docx",
  "methods": ["K", "S"],
  "steps": [
    { "key": "ArrowDown", "mods": { "ctrl": true } },
    { "type": "Hello" },
    { "click": { "para": 2, "offset": 5 }, "clicks": 1, "shift": false },
    { "click": { "para": 2, "offset": "end" }, "dx": 40 },
    { "drag": { "from": { "para": 2, "offset": 0 }, "to": { "para": 4, "offset": 3 } } },
    { "command": { "type": "undo" } },
    { "expect": { "caret": { "para": 2, "offset": 5, "affinity": "upstream", "depth": 0 } } },
    { "expect": { "text": { "2": "exact text", "3": { "starts": "prefix" } }, "paragraphs": 25 } },
    { "expect": { "selection": { "kind": "text", "anchor": { "para": 2, "offset": 0 }, "focus": { "para": 4, "offset": 3 } } } },
    { "expect": { "undo_depth": 1, "tables": [{ "rows": 3 }] } }
  ]
}
```

- **Paragraphs** are numbered in document order, cells included (the inspection dump's order), from 0.
- **Pointer targets** name a paragraph and a unit offset (or `"end"`). The runner finds the drawn line showing that unit and aims at its caret stop: `dx`/`dy` add points to that, `"line": "last"` picks the paragraph's last line, and `"page": n` (from 0) a line drawn on that page (a repeated header row's copy). In `caret` expectations, `depth` is the chain's length and `page` the page the caret is drawn on. A target is never a pixel position.
- **`key`** uses DOM `KeyboardEvent.code` names; `type` types each character.
- **`runs` and `jc`** expectations compare a paragraph's run formatting summary (`[[units, flags], ...]`, adjacent equal flags merged; flags `b` bold, `i` italic, `u` underline, `s` strike, `h` hidden, `^` superscript, `_` subscript) its alignment (`left`, `center`, `right`, `both`), and `style` its paragraph style id.
- **`clipboard`** steps (`"copy"`, `"cut"`, `"paste"`) copy the selection and paste what was copied, as the host's clipboard events do; **`paste_text`** pastes plain text from outside the editor.
- A target can be a float instead of text: `{"float": n, "at": "top-left"}` is the n-th float in document order (inspect `dump.floats`), at its centre or 1 pt inside the named edge or corner.
- A table border target: `{"table": t, "row": r, "border": k}` is grid boundary k (the left edge of column k) at row r's middle; `"border": "bottom"` is the row's bottom border at its middle; `"border": "handle"` is the table resize handle, 3 pt past the (last) row's lower right corner. Tables are numbered as in the dump.
- A target's own `dx` and `dy` (points) move the point it names (a drag's end beside a float, say).
- **`drag`** steps take `mods` like keys (held through the drag). **`tables`** expectations can give `grid` (column widths) and `heights` (`[height, rule]` or `null` per row), compared within half a point; `header_rows` (how many rows repeat) and `cell_fmt` (per row, per cell `"valign:FILL"`, `-` for no fill).
- **`sizes`** and **`colors`** expectations are per paragraph like `runs` (`[[units, 11], …]`, `[[units, "FF0000"], …]`); **`para`** gives paragraph metrics (`ind_left`, `ind_first`, `before`, `after`, `line`, `keep_next`), numbers within half a point.
- **`floats`** expectations list every float in document order (`[{}, {"anchor": 6}]`): the count must match, and each given field must equal the inspected one.
- **`tables`** expectations give, per table in dump order, `rows` and optionally `cells` (cells per row).
- **`comments`** expectations list every comment in the order of their threads' ranges, each thread's first comment then its replies: `author`, `initials`, `text` (paragraphs separated by `
`), `scope` (the range's text), `parent` (the 1-based position of the comment replied to), `done` and `active` (on a thread's first comment).
- **A missing fixture fails the scenario**.

## Merge template steps and expectations

- **`command`** steps run one command of the command set, as the API types it: `{ "command": { "type": "insertTable", "rows": 2, "columns": 2 } }`. **`batch`** steps send one batch: `{ "batch": { "label": "Rename", "commands": [{ "type": "replaceInElements", … }] } }`. **`query`** steps send a host query: `{ "query": { "q": "elements", "kind": "field" } }`.
- Editor inputs that aren't commands: **`author`** (`{ "author": "Author A" }`, who revisions are recorded under), **`mode`** (`"editing"`, `"design"` or `"readonly"`), **`drop`** (the external drop caret at a target, or `null`), **`select`** (`{ "para", "start", "end" }`, units of one paragraph, as find selects), **`select_element`** (an element id) and **`replace_all`** (`{ "find", "with", "case" }`).
- **`answer`** and **`mode`** match the last reply of the step; **`context`** matches its context list, innermost first, element by element (`[{ "kind": "field", "code": " MERGEFIELD A " }, { "kind": "repeating_section" }]`).
- **`elements`** matches the step's change lists, merged over its replies: per `added`, `removed` and `changed`, a count or a list of partial elements.
- **`events`** lists events that must appear in that order among the step's events (others may come between).
- **`view`** matches the canonical view of every page (inspect section `view`), for example `{ "drop": { … } }` or `{ "marks": [ … ] }`.
- Matching is **partial**: objects need only the keys given, arrays the same length, numbers within half a point. Element ids are never written in expectations; elements are matched by `kind`, `code` or `tag`.
- **References in arguments** (commands, queries, batches, `drop` and `select_element`): `"$last"` is the id the last answer gave (`{ "id": … }` from `insertField`, `insertControl`, `addComment`); `"$code:<code>"` and `"$tag:<tag>"` are the first element with that code (trimmed) or tag; and a pointer target object (`{ "para", "offset", "page"?, "dx"? }`) is the document point it names (`drop`).
