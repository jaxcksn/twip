# Merge template scenarios

One folder per feature area, IDs `MRG-<area>-NN`. They run natively (`cargo test -p twip scenarios`, filter with `SCENARIO=merge`).spec.ts`).

- `us1/`: insert and change placeholders (fields and content controls), drops, selection context, design-mode labels.
- `us2/`: list, mark, select and rename placeholders; change lists.
- `us3/`: repeating regions, their table rules and the structure query; `us3/matrix/` holds the structural matrix with regions and controls (generated).
- `us4/`: host looks, design-mode activation, fills and locks.
- `us5/`: data parts.
- `us6/`: read-only documents .
- `us7/`: field codes edited in the document.

Steps and expectations beyond tests/scenarios/README.md (`query`, `batch`, `expect.answer`, `expect.context`, `expect.elements`, `expect.events`, `expect.mode`, `expect.view`) are documented there.
