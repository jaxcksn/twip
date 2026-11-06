# Changelog

## Unreleased

### Features

* **web:** `configure({ assets })` sets where the engine's files (`dist/`) are served from, for applications whose bundler doesn't follow `new URL(..., import.meta.url)`; the folder may be on another origin with CORS (README: "Serving the engine's files yourself")

## 0.1.0-alpha.1 (2026-10-27)


### Features

* the twip engine, its .NET and web SDKs and the docs site ([244f232](https://github.com/jaxcksn/twip/commit/244f2325747d9fbc2275269d2030ff0af8d080b4))
