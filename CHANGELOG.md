# Changelog

## Unreleased

### Features

* **web:** `configure({ assets })` sets where the engine's files (`dist/`) are served from, for applications whose bundler doesn't follow `new URL(..., import.meta.url)`; the folder may be on another origin with CORS (README: "Serving the engine's files yourself")
* **editing:** `setApplicationFieldTypes({ types })` declares the application's own field types: their fields keep their results and show their design labels instead of "Error! Bookmark not defined."
* **editing:** picture content controls take a size: `insertControl({ kind: 'picture', options: { width, height } })` (points) sizes the placeholder, `setControl` resizes it (or the filled picture), and `elements.get` reports it

## 0.1.0-alpha.1 (2026-10-27)


### Features

* the twip engine, its .NET and web SDKs and the docs site ([244f232](https://github.com/jaxcksn/twip/commit/244f2325747d9fbc2275269d2030ff0af8d080b4))
