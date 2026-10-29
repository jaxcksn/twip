---
title: Getting started (Rust)
sidebar_position: 3
---

# Getting started with Rust

`twip::api` is the editor's public contract: the engine, sessions, commands, state, events, services, errors and
warnings. The TypeScript and .NET SDKs follow it type for type, and their values cross into the engine as the JSON
these types serialize to. Use it to embed the editor in a Rust application or service.

```rust file=../../../crates/twip/examples/session.rs
```

## The pieces

- `engine::Engine` owns the font library and the sessions of one thread. `set_listener` receives every session's
  events synchronously, in commit order, after the session's state reflects them.
- `session::Session` is one document being edited. `run(Command)` applies one command as one transaction, and
  `batch` applies several atomically. Both return a `Commit` with the transaction's record and the command's value.
- `command::Command` has one variant per command, named after the command id in PascalCase (`setBold` is
  `Command::SetBold`). `command::CommandId::ALL` lists them, and `Session::catalog()` describes them.
- `state::State` is rebuilt after each change. `Tri<T>` is a value that may be mixed across the selection or not
  apply.
- Background layout runs only when the host asks: `session.host().idle()` lays out for a few milliseconds and
  returns true while more is pending. Until then `state.pages.count` counts the pages laid out so far and
  `pages.complete` is false; each call that changes either is a new state with the same revision. Call it when your
  application is idle, or in a loop (`while session.host().idle() {}`) when you need the page count. The SDKs call
  it on the engine's timers.
- `event::Event` is everything the SDKs deliver as events.
- `services` holds the models of menus, dialogs, find, spelling, navigation, styles, elements, rulers and
  accessibility; `review` the revisions and comments.
- `error::Error` carries an `ErrorCode`, the same codes as the SDKs (`InvalidArgument`, `Unavailable`, `ReadOnly`,
  `ImportFailed` and the others), and `warning::Warning` the fidelity warnings.

Every type is `serde` serializable, and `api::wire` holds the JSON rules: camelCase fields, and enums tagged with
`type`.

## Stability

Before 1.0 the API changes between minor versions. The crate's other modules are internal: they are what the
engine is built from, and they carry no compatibility promise.

The crate builds Skia from a prebuilt archive, which it downloads on the first build.
