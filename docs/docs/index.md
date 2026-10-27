---
title: Introduction
sidebar_position: 1
slug: /
---

# Twip

Twip is a DOCX layout, editing and rendering engine. It lays documents out the way a word processor does, line for
line and page for page, and draws them with Skia. It is written in Rust and runs natively on Windows and Linux, and
in the browser through WebAssembly.

Twip is the engine under an editor, not the editor itself. It owns the document, the layout, the caret, selection,
editing commands and undo, and it draws pages. Your application owns the window, the toolbar and everything else
the user sees around the page.

:::note Pre-release

Twip is in its initial development phase (0.x). The API will change between minor versions until 1.0.

:::

## SDKs

| SDK | Package | For |
|---|---|---|
| Rust | [`twip`](https://crates.io/crates/twip) | Rust applications and services |
| .NET | `Twip` on NuGet | Windows and Linux apps, Native AOT compatible |
| Web | `@twip/web` on npm | Browsers, through WebAssembly |

The .NET and web SDKs are generated from one binding definition, so they expose the same API with each language's
naming. The Rust crate is the engine itself.
