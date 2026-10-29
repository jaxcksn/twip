---
title: Rendering
sidebar_position: 9
---

# Rendering

The engine draws the whole editing surface: the pages, and over them the caret, the selection, the object outlines
and handles, the drop caret of a drag, formatting marks, and the comment balloons' leader lines. Your application
draws nothing over the document, so the frame stays on the GPU wherever there is one.

## The caret

The caret shows while the editor has the focus and the selection is collapsed. It blinks, and any input shows it
solid at once. The blink follows the platform until you set it:

- In the browser, a steady caret under `prefers-reduced-motion: reduce`, else a 530 ms phase.
- In WinForms, Windows' caret blink rate (the keyboard settings in Control Panel); steady when Windows doesn't
  blink the caret.

A phase is at least 100 ms. `caretBlinkStopAfter` (web) and `EditorHost.CaretBlinkStopAfter` (.NET) stop the blink
after a time without input, leaving the caret visible. An unfocused editor, or a blink that stopped, draws no frames
at all.

## Overlay colours

`OverlayColors` names the colour of each thing drawn over the document: `caret`, `selection`, `object` (a selected
object's outline and handles), `drop`, `marks` (formatting marks and text boundaries) and `leaders`. Each default
has at least 3:1 contrast against the page. The selection is blended under the text (multiply), so black text stays
black over it.

Under forced colours (web) or high contrast (WinForms) the editor uses the system's colours, until you set your own.

## Web

```ts file=../../examples/web/rendering.ts
```

The options are also methods on `editor.view`, for a settings page: `setCaretBlink` and `setOverlayColors`. Once you
call them, the editor stops following the platform's setting.

The editor draws with WebGPU, then WebGL 2, then the CPU. If the GPU device or the WebGL context is lost, the editor
gets a new one and draws the next frame on it; in worker mode too. Each lost WebGPU device costs the engine some
memory it can't free, so after the third the editor draws CPU pixels and copies them onto the canvas.

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
