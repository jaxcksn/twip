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

## Serving the engine's files yourself

The engine runs from files in the package's `dist/`: `worker.js`, `twip-wasm.js`, `twip_wasm.wasm`, and the
`transport/` and `worker/` folders the worker imports. By default the SDK finds them beside its own code, through
`new URL(..., import.meta.url)`. If your bundler doesn't follow those (Angular's esbuild builder doesn't), copy
`dist/` as it is to a folder you serve, and say where, once, before the first `FontLibrary` call or editor:

```ts
import { configure } from '@twip/web';

configure({ assets: new URL('twip/', document.baseURI) }); // or a CDN: 'https://cdn.example.com/twip/'
```

`assets` may be on another origin. The worker then starts from a same-origin `blob:` module that imports
`worker.js`, so the other origin must:

- send `Access-Control-Allow-Origin` (your page's origin, or `*`) on every file in the folder, as module scripts and
  the WebAssembly are fetched with CORS;
- serve `twip_wasm.wasm` as `application/wasm`, or the engine compiles it after a full download instead of while
  streaming.

With a Content Security Policy, allow `blob:` in `worker-src`, the other origin in `script-src` and `connect-src`, and
`'wasm-unsafe-eval'` in `script-src` (which the engine needs on any origin).

Guides: https://jaxcksn.github.io/twip/docs/guides/web. Licensed under Apache-2.0.
