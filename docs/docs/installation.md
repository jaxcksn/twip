---
title: Installation
sidebar_position: 2
---

# Installation

## Rust

```toml title="Cargo.toml"
[dependencies]
twip = "0.1"
```

## .NET

```bash
dotnet add package Twip
```

The package carries the native engine for `win-x64`, `win-arm64`, `linux-x64` and `linux-arm64`. It works with
Native AOT publishing; no extra configuration is needed.

On Linux the engine uses the system's FreeType and Fontconfig libraries. Desktop distributions have them; in a
slim container image install them (`libfreetype6 libfontconfig1` on Debian and Ubuntu), and see [Fonts](guides/fonts)
for the fonts documents expect.

## Web

```bash
pnpm add @twip/web
```

The package ships the engine as a WebAssembly module next to its JavaScript. Bundlers that copy assets referenced
with `new URL(..., import.meta.url)` (Vite, webpack 5, esbuild with the file loader) need no setup.
