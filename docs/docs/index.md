---
title: Introduction
sidebar_position: 1
slug: /
---

# Twip

Twip is a DOCX layout, editing and rendering engine. It lays documents out the way a word processor does, line for
line and page for page, and draws them with Skia. It is written in Rust and runs natively on Windows and Linux, and
in the browser through WebAssembly.

Twip is the engine under an editor, not the whole application. It owns the document, the layout, the caret and
selection, editing commands and undo, input on the page and drawing; and it computes the models behind the chrome:
the state for a toolbar, context menus, dialogs, find, spelling, rulers and review. Your application draws the
toolbar, menus, dialogs and panes around the page from those models.

:::note Pre-release

Twip is in its initial development phase (0.x). The API will change between minor versions until 1.0.

:::

## SDKs

| SDK | Package | For |
|---|---|---|
| Web | `@twip/web` on npm | Browsers and Node, through WebAssembly |
| .NET | `Twip.Core` and `Twip.WinForms` on NuGet | Windows and Linux apps and services, Native AOT compatible |
| Rust | [`twip`](https://crates.io/crates/twip) | Rust applications and services |

Each SDK is written by hand for its language, and all of them follow one contract, the Rust crate's `twip::api`:
the same commands, state, events and errors, with each language's naming. A shared suite of contract scripts runs
against every SDK, so they behave the same.
