---
title: Getting started (Swift)
sidebar_position: 3.5
---

# Getting started with Swift

The Swift package has two libraries. `Twip` is the editor without a window: an `EditorSession` owns one document,
with its selection, undo history, state and events, and `PDFConverter` turns documents into PDFs with no session.
`TwipUI` adds `DocumentEditor`, a SwiftUI view that presents a session and takes its input, and `EditorModel`, its
state for your toolbars. Install them first (see [Installation](../installation)).

## A session

```swift file=../../examples/swift-examples/Sources/Examples/Session.swift#session
```

Every operation is `async`. Operations run in the order you call them, one at a time, on the engine's own thread, so
you can call a session from any task or actor. Each returns once its change has committed, and `session.state` is
then at its revision or later.

Commands are values of the `Command` enum, run with `run(_:)` or atomically with `batch(_:label:)`: `.insertText(_:)`,
`.setBold(_:)`, `.moveSelection(by:direction:extend:)`, and one case for each of the engine's commands. Setters take
a value: there are no toggles. Lengths are `Points` (a `Double`), colours are `RGBColor` (a `"#1F4E79"` literal reads
as one), and identifiers are their own types (`CommentID`, `ElementID`). A command that makes something returns it in
its `Commit` (`commit.value`: a comment's id, a field's id).

`session.state` is the latest `EditorState`: the selection, formatting, each command's availability, the undo
history, page counts, view and review state. Values that can differ across the selection are `Tri`: `.uniform(value)`,
`.mixed` or `.unavailable`, and `isOn` is true only for a uniform `true`. `state.commands[.undo]?.enabled` says whether
a command can run.

`state.pages.count` counts the pages laid out so far; the engine lays out the rest in the background, a few
milliseconds at a time, and `state.pages.complete` turns true when it's done.

## Errors and cancellation

Expected failures throw `TwipError`, whose `code` says why. A failed open keeps the current document. Cancelling the
calling task cancels an operation that hasn't started, or an open while it reads, which throws `CancellationError`; a cancel that arrives after a change committed is ignored.

```swift file=../../examples/swift-examples/Sources/Examples/Session.swift#errors
```

## Events and state

`session.events` is an `AsyncStream` of the session's events, each with the state at its change, in commit order.
Each stream gets every event from when you ask for it (`session.events`), so ask before the operations you want
to hear about; it ends when the session closes. See
[Events](events) for what each one means.

```swift file=../../examples/swift-examples/Sources/Examples/Events.swift#events
```

`session.states` yields the state after each change, starting with the current one, and skips to the newest when you
read it slower than it changes.

```swift file=../../examples/swift-examples/Sources/Examples/Events.swift#states
```

## SwiftUI

`DocumentEditor` draws the pages (with Metal, or the CPU where there's no GPU) and handles the keyboard, pointer,
trackpad zoom, input methods, the clipboard, cursors, scrolling and the engine's context menu. `EditorModel` is
`@Observable` and keeps the session's state on the main actor, so a toolbar redraws when the selection changes.
`toggle(_:_:)` makes a `Binding<Bool>` from a state value and the command that sets it.

```swift file=../../examples/swift-examples/Sources/Examples/App.swift#app
```

The keys the application owns (Cmd+S, Cmd+P, Cmd+F, ...) never edit: handle them with `onHostCommand`. The Edit
menu's Cut, Copy, Paste, Select All, Undo and Redo reach the view through the responder chain. In AppKit, use
`DocumentEditorView`, the `NSView` behind `DocumentEditor`. One view presents a session at a time; the session keeps
its document, selection and history when the view goes.

## Fonts

A session lays out with a `FontLibrary`: the system's fonts by default. See [Fonts](fonts) for aliases, replacements,
fallbacks and resolvers.

```swift file=../../examples/swift-examples/Sources/Examples/Fonts.swift#fonts
```

## PDF conversion

`PDFConverter` converts DOCX to PDF without a session, with any number of conversions at once. A `PDFFontSet` is
fixed once made and shared between them. See [PDF conversion](pdf-conversion).

```swift file=../../examples/swift-examples/Sources/Examples/PDF.swift#convert
```
