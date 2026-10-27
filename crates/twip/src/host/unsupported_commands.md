# Command-bar ids not supported by the new editor

Every id in `command_bar_tabs.json` is either a document command (`Command::from_host`), a host command (dialogs,
files, the clipboard: `command_bar::HOST`), a view setting the host keeps (zoom, ruler, gridlines, the border pen),
a menu that only opens its entries, or listed here. `command_bar::tests::every_id_is_mapped` checks it.

| Id | Why |
|---|---|
| `mail.*` | Mail merge is deferred (user decision, 2026-10-24). |
| `picture.image-field` | Merge image options belong to the deferred mail merge. |
| `layout.break.next-page`, `layout.break.continuous`, `layout.break.even-page`, `layout.break.odd-page` | Section breaks: saving can't add sections yet (`docx::write::main_xml`). |
| `picture.wrap.inline` | Turning a floating picture inline is not done. |
| `picture.position.left`, `picture.position.center`, `picture.position.right` | Float alignment presets are not done. |
| `insert.bookmark` | A new bookmark has no unit of its own for saving to place: zero-width inlines aren't written yet. |
| `insert.placeholder` | Designing placeholder controls belongs with the deferred template and merge work. |
