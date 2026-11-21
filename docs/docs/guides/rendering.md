---
title: Rendering
sidebar_position: 9
---

# Rendering

The engine draws the whole editing surface: the pages, and over them the caret, the selection, the object outlines
and handles, the drop caret of a drag, formatting marks, and the comment balloons' leader lines. Your application
draws nothing over the document, so the frame stays on the GPU wherever there is one.

## The caret

The caret shows while the editor has the focus and the selection is collapsed. It's black or white, whichever stands
out more on the page as drawn (white on a dark page), and no theme or overlay colour changes it. It blinks, and any input shows it
solid at once. The blink follows the platform until you set it:

- In the browser, a steady caret under `prefers-reduced-motion: reduce`, else a 530 ms phase.
- In WinForms, Windows' caret blink rate (the keyboard settings in Control Panel); steady when Windows doesn't
  blink the caret.

A phase is at least 100 ms. `caretBlinkStopAfter` (web) and `EditorHost.CaretBlinkStopAfter` (.NET) stop the blink
after a time without input, leaving the caret visible. An unfocused editor, or a blink that stopped, draws no frames
at all.

## Overlay colours

`OverlayColors` names the colour of each thing drawn over the document: `selection`, `object` (a selected
object's outline and handles), `drop`, `marks` (formatting marks and text boundaries) and `leaders`. Each default
has at least 3:1 contrast against the page, except `selection`: the light blue of a system text selection
(`#B3D7FF`). The selection is blended under the text (multiply), so black text stays black over it.

Under forced colours (web) or high contrast (WinForms) the editor uses the system's colours, until you set your own.

## Text

Glyphs are drawn from the font's outlines, the same way on the GPU, on the CPU and in every SDK, so a page looks the
same wherever it is drawn.

- **Hinted:** text upright or turned by a quarter, a half or three quarters (vertical sections, turned table cells)
  at an even scale. Hinting is light: it snaps edges across the baseline to the pixel grid and keeps every glyph's
  origin and advance, so line breaks are the same hinted or not and zoom never reflows a line. Along the baseline an
  edge can still move by up to half a pixel, where the font's own hinting instructions move it.
- **Not hinted:** text at any other angle, mirrored or stretched, synthetic italic, and colour glyphs (emoji).
- **Positions:** glyphs land on quarter pixels along the baseline. For upright and quarter-turned text the baseline
  itself sits on a whole pixel, hinted or not (skewed and stretched text too); at any other angle glyphs land on
  quarter pixels both ways.
- **Weight:** coverage goes through a gamma chosen by the text colour, so dark text on light and light text on dark
  look the same weight. Glyphs more than 256 pixels per em are drawn from their outlines instead of a cached mask,
  through the same gamma.

Text decoration and effects draw as Word draws them: every underline style and colour, emboss, engrave, outline and
shadow, and the Word 2010 text effects (glow, shadow, reflection, gradient or no fill, outline; `setTextEffects`).

A PDF export embeds the fonts, and the viewer draws their glyphs its own way.

## Pictures, shading and backgrounds

- **Metafiles:** EMF and WMF pictures, and OLE object previews, draw as vectors at any zoom, not as a raster. A dual
  EMF and EMF+ file draws its EMF records. EMF+ records, JPEG XR pictures and double-byte text in metafiles aren't
  drawn yet.
- **Picture effects:** set transparent colour, brightness, contrast, washout, grayscale, black and white, and
  reflection. A linked picture draws its embedded copy when it has one, else a light placeholder frame: the engine
  fetches nothing.
- **Shading and borders:** every shading pattern (stripes and crosses as tiles that repeat on whole device pixels,
  percentages flat) on text, paragraphs, cells and tables; shadowed borders and the wave, double wave and
  dash-dot-stroked lines.
- **Watermarks and page colour:** text and picture watermarks in the headers, and the page colour (a colour with a
  theme tint or shade, a gradient, a pattern or a picture), draw on screen. A page colour shows only when the document
  displays background shapes, as in Word.

The same drawing is the source for the GPU, the CPU and PDF, and a regression gate compares them: pixel snapshots of
a fixture corpus, the GPU against the CPU, and the PDF's text, structure and drawing commands.

## Web

```ts file=../../examples/web/rendering.ts
```

The options are also methods on `editor.view`, for a settings page: `setCaretBlink` and `setOverlayColors`. Once you
call them, the editor stops following the platform's setting.

The editor draws with WebGPU, else the CPU. If the GPU device is lost, the editor gets a new one and draws the next
frame on it; in worker mode too. Each lost WebGPU device costs the engine some memory it can't free, so after the third
the editor draws CPU pixels and copies them onto the canvas.

## WinForms

```csharp file=../../examples/dotnet/Rendering.cs#rendering
```

`DocumentEditor` follows `SystemEvents.UserPreferenceChanged`, so a change to the blink rate or to high contrast
applies while the application runs. After a GPU device loss it tries the GPU again after 1, 5 and 30 seconds, then
stays on the CPU for the session; `GpuBackend` says which it uses. `DrawToBitmap` always works, from CPU pixels.

The engine draws the comment balloons' leader lines while no `PaintMarkupPane` handler is attached. Attaching one
hands the markup pane back to your GDI drawing, and the frames go to the CPU while it is attached.

## Memory and frame statistics

The editor keeps each page's drawing between frames, so a frame that only moves the selection or blinks the caret
replays the pages instead of drawing their text again. An edit draws again only the pages it changed. The drawings
are kept within a memory budget, 32 MB by default: `paintBudget` (web) and `EditorHost.PaintBudget` (.NET). Past the
budget, the pages drawn least recently go first.

`view.frameStats()` (web) and `EditorHost.GetFrameStatsAsync` (.NET) report the frames drawn, the pages drawn and
reused, the recordings' bytes, and the last frame's time and the 95th percentile of the last 120. They are meant for
benchmarks and tests.
