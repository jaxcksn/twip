# @twip/web

The Twip DOCX editor for the browser and Node: a managed, framework-independent editor. Mount it in a sized
container, open a document, connect your controls, and export DOCX or PDF.

> **Alpha.** The API may change between minor versions before 1.0.

```sh
npm install @twip/web
```

```ts
import { Editor, FontLibrary } from '@twip/web';

// The browser gives the engine no fonts: add every font your documents use, once per page.
await FontLibrary.default().add('/fonts/calibri.ttf');

const editor = await Editor.mount(document.getElementById('editor')!, {
  document: new Uint8Array(await (await fetch('/report.docx')).arrayBuffer()),
});
const stop = editor.subscribe((state) => {
  boldButton.ariaPressed = String(state.formatting.bold === true);
  undoButton.disabled = !state.history.canUndo;
});

await editor.commands.setBold(true);
const { bytes, warnings } = await editor.export({ format: 'docx' });

stop();
await editor.destroy();
```

In Node, workers and tests, `HeadlessEditor.create()` gives the same API with no DOM.

Guides: https://jaxcksn.github.io/twip/docs/guides/web. Licensed under Apache-2.0.
