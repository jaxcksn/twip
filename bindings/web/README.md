# @twip/web

The Twip DOCX engine for the browser and Node: open Word documents, edit them, paint pages (WebGPU, WebGL 2 or a
CPU raster) and save DOCX or PDF.

> **Alpha.** The API may change between minor versions before 1.0.

```sh
npm install @twip/web
```

```js
import { Editor, Fonts } from '@twip/web';

// The browser gives the engine no fonts: add every font your documents use.
const fonts = new Fonts();
fonts.addFont(new Uint8Array(await (await fetch('/fonts/calibri.ttf')).arrayBuffer()));

const editor = new Editor(fonts);
editor.open(new Uint8Array(await (await fetch('/report.docx')).arrayBuffer()));
console.log(`${editor.pageCount} page(s)`);

editor.selectAll();
editor.bold();
const docx = Uint8Array.from(editor.save().asSlice());
```

Guides: https://jaxcksn.github.io/twip/docs/guides/web. Licensed under Apache-2.0.
