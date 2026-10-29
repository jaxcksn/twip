---
title: Getting started (web)
sidebar_position: 1
---

# Getting started in the browser

`@twip/web` is a complete editor for a page: you mount it in an element, and it owns input, rendering, scrolling,
the clipboard and the accessibility DOM inside that element. Your application draws everything around it (toolbar,
menus, dialogs, panes) from the editor's state and services. Install it first (see [Installation](../installation)).

## Mount an editor

```ts file=../../examples/web/mount.ts
```

`Editor.mount` resolves once the editor accepts commands. The container must have a size; the editor fills it.
`MountOptions` also sets the read-only flag, the mode (`'editing'` or `'design'`), the language, spelling
dictionaries, track changes, the rulers and their unit, the markup pane width, the zoom and the background around
the pages. Mounting never takes focus: call `focus()` when the editor should have it.

Without a `document`, the editor starts with a blank one. `open(bytes)` replaces the document later, only once the
new one has been read, and `newDocument()` starts a blank one. Both take an `AbortSignal`.

## Commands

Every edit is a command, one method each on `editor.commands`. They act on the selection, like their buttons in a
word processor, and resolve after the change commits.

```ts file=../../examples/web/commands.ts
```

Setters take a value: there are no toggles. To make a Bold button, read `formatting.bold` and pass the opposite.
Lengths are points; `inches()`, `cm()`, `mm()` and `pt()` convert. A command that can't run now rejects with an
`EditorError`: `'unavailable'` (its availability rule is false), `'read-only'`, or `'invalid-argument'`.

`editor.catalog` lists every command with its English label, group, shortcuts and undo grouping, for a command
palette, menus or a shortcut sheet. The API reference lists every command with its arguments.

## State

`getState()` returns the latest state: one immutable snapshot, read synchronously. `subscribe` calls your listener
with it now and after every change. A toolbar and status bar need nothing else.

```ts file=../../examples/web/state.ts
```

The state holds the selection, character, paragraph and page formatting, each command's `enabled`, `checked` and
value, the undo history, page counts, view settings, review counts, spelling, fonts and the ruler model. Values that
differ across the selection are `'mixed'`, and values that don't apply (no table selected) are `null`.

`revision` counts committed transactions and `sequence` counts every change, selection moves included. When a
command resolves, `getState()` is already at its revision.

`pages.count` counts the pages laid out so far. The engine lays out the rest in the background, a few milliseconds
at a time on timers it arms, mounted or headless; each slice that adds pages, and the last one, which sets
`pages.complete`, is a new state with the same revision. Show the count as provisional until `complete`.
`dialogs` says which dialogs `dialogs.open` opens now (Table properties needs a table, Picture size and Alt text an
object), and `selection.text` is the selection as plain text, or `null` past 10,000 characters.

## Events

`on(type, listener)` listens to things that happen, such as a commit, a link click, a key your application owns or
a context menu request. See [Events](events).

## Headless

`HeadlessEditor.create()` gives the same API with no DOM, in Node, a worker, a test or a server: documents,
commands, batches, state, events and services. The mounted extras (the view, gestures, rulers, balloons, hit
testing) are on `Editor` only.

```ts file=../../examples/web/headless.ts
```

## Worker mode

By default (`execution: 'auto'`), a mounted editor runs the engine in a worker where the browser supports module
workers and `OffscreenCanvas`, and on the page's thread otherwise. Every editor on the page shares one worker, with
one font library and one clipboard. The API doesn't change: operations are promises in both modes, and `getState()`
stays synchronous because the SDK keeps the latest snapshot on the page. `'main'` and `'worker'` force a mode.

```ts file=../../examples/web/worker.ts
```

The worker is `@twip/web/worker.js`, beside the package's other files. Bundlers that copy assets referenced with
`new URL(..., import.meta.url)` (Vite, webpack 5, esbuild with the file loader) need no setup.

## Destroy

`destroy()` releases the session, the worker's share of it and everything the editor added to the container. It is
idempotent. After it, operations reject with `'destroyed'`.

## Errors

Operations reject with `EditorError`, whose `code` is one of `'destroyed'`, `'invalid-argument'`, `'unavailable'`,
`'read-only'`, `'import-failed'`, `'export-failed'`, `'aborted'`, `'not-allowed'`, `'unsupported'` or
`'internal'`. A failed `open` keeps the current document. An error with `fatal: true` is a fault: the editor
rejects new operations, and `destroy()` still works.

## Next

- [Events](events) and [Services](services): menus, dialogs, find, spelling, the clipboard, placeholders and more.
- [Review](review): track changes and comments.
- [Fonts](fonts): what the browser needs to lay documents out right.
- [React and Angular](adapters).
