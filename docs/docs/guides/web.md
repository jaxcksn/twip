---
title: Getting started (web)
sidebar_position: 2
---

# Getting started in the browser

Install `@twip/web` (see [Installation](../installation)). The module loads the engine when it's imported.

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
const pdf = Uint8Array.from(editor.exportPdf('Report').asSlice());
```

The same code runs in Node, which is handy for tests and servers.

## Errors

`open` throws when the bytes aren't a document Twip can read; the error message names the reason
(`OpenError.Empty`, `OpenError.Invalid`). The editor keeps its current document then.

## Layout runs in the background

`open` lays out the first pages and returns; `pageCount` grows as layout continues. Call `idle()` from
`requestIdleCallback` until it returns `false`:

```js
function layOut(deadline) {
  let more = true;
  while (more && deadline.timeRemaining() > 2) more = editor.idle();
  if (more) requestIdleCallback(layOut);
}
requestIdleCallback(layOut);
```

## Next

- [Drawing pages](rendering) with WebGPU, WebGL 2 or onto a 2D canvas.
- [Input and commands](input): forwarding keyboard and pointer events, the formatting and table commands.
- [Requests and events](requests): scrolling, the cursor and the clipboard.
