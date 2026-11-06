# Changelog

## Unreleased

### Features

* **web:** `configure({ assets })` sets where the engine's files (`dist/`) are served from, for applications whose bundler doesn't follow `new URL(..., import.meta.url)`; the folder may be on another origin with CORS (README: "Serving the engine's files yourself")
* **editing:** `setApplicationFieldTypes({ types })` declares the application's own field types: their fields keep their results and show their design labels instead of "Error! Bookmark not defined."
* **editing:** picture content controls take a size: `insertControl({ kind: 'picture', options: { width, height } })` (points) sizes the placeholder, `setControl` resizes it (or the filled picture), and `elements.get` reports it
* **editing:** `subTemplates.create(name, { copyOf })` copies a sub-template's body; `subTemplates.export(name)` gives one body as a standalone DOCX that `create(name, { from })` reads back unchanged
* **editing:** `replaceBody({ from })` replaces the template's body with a DOCX's, keeping its sub-templates, custom XML data parts and settings, in one undo step
* **editing:** `insertControl({ kind: 'picture', placeholderImage: { data, mime } })` sets a picture control's placeholder picture (PNG, JPEG, GIF, BMP or SVG), shown in the editor, the saved DOCX and the PDF; `setControl` replaces it while the control shows it
* **docx:** SVG pictures (`asvg:svgBlip`) are read (the SVG over its PNG fallback), drawn on the canvas and in PDFs with the picture's box as the viewport, and written with a PNG fallback as Word does
* **editing:** picture controls report their size (`elements.list`, the selection's context); a corner handle keeps a picture control's aspect; sizing and placeholder pictures work for floating pictures in controls too, which the mouse moves as it does plain floats

### ⚠ BREAKING CHANGES

* **editing:** `setPlaceholderLooks` and `PlaceholderLook` are removed (web, .NET, wire); a placeholder draws as its own picture. Set it with `placeholderImage`

### Bug Fixes

* **docx:** content controls in blocks written afresh (a new sub-template, header, footer or note) are no longer dropped on save

## 0.1.0-alpha.1 (2026-10-27)


### Features

* the twip engine, its .NET and web SDKs and the docs site ([244f232](https://github.com/jaxcksn/twip/commit/244f2325747d9fbc2275269d2030ff0af8d080b4))
