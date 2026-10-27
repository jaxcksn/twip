---
title: Drawing pages
sidebar_position: 4
---

# Drawing pages

The editor draws the pages under its viewport: you tell it which part of the stacked pages is visible, then paint.

## Coordinates

All positions are **content pixels**: the pages stacked top to bottom with gaps, at the editor's zoom (1.0 = 100%,
96 pixels per inch). `ContentSize()` / `contentSize()` is the size of the whole stack: use it for your scroll range.
The device pixel ratio (`dpr`) scales content pixels to the screen's pixels.

```csharp
editor.SetZoom(1.25);
editor.SetViewport(scrollX, scrollY, viewWidth, viewHeight, dpr);   // content pixels
```

Set the viewport whenever the view scrolls or resizes, then paint.

## CPU raster (every platform)

`Paint` draws into an RGBA buffer, 8 bits per channel, premultiplied alpha, rows packed: `width × height × 4`
bytes for a `width × height` device-pixel image. The page stack's top-left sits at (`originX`, `originY`).

```csharp
var pixels = new byte[width * height * 4];
editor.Paint(pixels, (uint)width, (uint)height, -scrollX, -scrollY, dpr);
// Copy pixels into a WriteableBitmap, an SKBitmap, a PNG encoder, ...
```

In JavaScript `paint` returns the pixels as a new buffer:

```js
const frame = editor.paint(width, height, -scrollX, -scrollY, devicePixelRatio);
const image = new ImageData(new Uint8ClampedArray(frame.asSlice()), width, height);
canvas2d.putImageData(image, 0, 0);
```

## WebGPU and WebGL 2 (browser)

For on-screen editing in a browser, paint on the GPU. Attach a WebGPU device once for every editor, or give each
editor a WebGL 2 canvas:

```js
import { attachWebGpu, webGpuTarget, paintWebGpu, webGlTarget, paintWebGl } from '@twip/web';

const device = await (await navigator.gpu?.requestAdapter())?.requestDevice();
let paint;
if (device && attachWebGpu(device)) {
  const target = webGpuTarget(canvas, device);
  paint = () => paintWebGpu(editor, target, -scrollX, -scrollY);
} else {
  const target = webGlTarget(editor, canvas);
  paint = () => paintWebGl(editor, target, -scrollX, -scrollY);
}
requestAnimationFrame(function frame() { paint(); requestAnimationFrame(frame); });
```

Size the canvas in device pixels (`canvas.width = cssWidth * devicePixelRatio`) and set the editor's viewport to
match. Painting after each input is enough; there is no animation of its own except the caret's blink, which is
yours to time.
