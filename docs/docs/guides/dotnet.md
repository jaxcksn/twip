---
title: Getting started (.NET)
sidebar_position: 2
---

# Getting started with .NET

`Twip.Core` is the editor without a window: `EditorSession` owns one document, with its selection, undo history,
state and events. `Twip.WinForms` adds `DocumentEditor`, a control that presents a session and takes its input.
Install them first (see [Installation](../installation)).

## A session

```csharp file=../../examples/dotnet/Session.cs#session
```

`OpenAsync` and `SaveAsync` take a path or a `Stream`; saving to a path replaces the file safely. `SaveAsync` writes
DOCX, and `ExportPdfAsync` writes PDF. Both return the revision they captured and any fidelity warnings.

Every command is one `...Async` method on `session.Commands`, named after the command (`setBold` is
`SetBoldAsync`). They act on the selection and complete after the change commits. Setters take a value: there are
no toggles. Lengths are `Length` values in points (`Length.FromInches(0.5)`, `Length.FromCm(2)`), colours are
`ColorValue`, and option objects are records (`IndentOptions`, `FieldOptions`). `EditorOperation` has one factory per
command, for `BatchAsync`.

`session.State` is the latest immutable `EditorState`: the selection, formatting, each command's availability, the
undo history, page counts, view and review state. Values that can differ across the selection are `FormatValue<T>`:
`Kind` is `Uniform`, `Mixed` or `Unavailable`, and `IsOn` is true only for a uniform `true`.

`State.Pages.Count` counts the pages laid out so far. The engine lays out the rest in the background, a few
milliseconds at a time on the engine thread, attached or not; each slice that adds pages, and the last one, which sets
`Pages.Complete`, raises `StateChanged` with the same revision. `State.Dialogs` says which dialogs open now (Table
properties needs a table, Picture size and Alt text an object), and `State.Selection.Text` is the selection as plain
text, or null past 10,000 characters.

## Errors

Expected failures throw `EditorException`, whose `Code` says why. A failed open keeps the current document.
Cancellation throws `OperationCanceledException`, and file access keeps the usual `IOException` types.

```csharp file=../../examples/dotnet/Session.cs#errors
```

## Events

Events are standard `EventHandler<T>` events on the session. They're raised on the attached control's UI thread or,
when no control is attached, on an SDK-owned serial executor, so handlers never race each other. Each event's
arguments carry the `Snapshot` of state at its revision. See [Events](events).

```csharp file=../../examples/dotnet/Events.cs#events
```

## WinForms

`DocumentEditor` is a WinForms control. It creates its own session (or takes yours: then disposing the control only
detaches it), draws the pages and the engine's rulers, and handles the keyboard, mouse, IME, clipboard, scrolling
and accessibility. `State` is a `DocumentEditorState`: bindable properties for a toolbar and status bar, raised on
the UI thread. Bind a toggle button's `CheckState` to `BoldCheckState`, `ItalicCheckState` or `UnderlineCheckState`
(`Indeterminate` where the selection is mixed), and its `Enabled` to a command's `Can` property (`CanSetBold`,
`CanPaste`, `CanUndo`, ...); `IsEnabled(CommandId)` covers every command.

```csharp file=../../examples/dotnet/WinForms.cs#form
```

The control's view settings are properties: `ShowHorizontalRuler`, `ShowVerticalRuler`, `RulerUnit`, `TabSelector`,
`MarkupPaneWidth`, and `BackColor` for the background around the pages (the paper stays white). WinForms shows no
system context menu, so `ContextMenuRequested` is where you show yours. Comment balloons are your own controls,
placed by `AttachBalloon`: see [Review](review). The caret blink, overlay colours and the GPU: see [Rendering](rendering).

## Hosting a session in another UI framework

`Twip.Hosting` is the contract a control is built on, for WPF, WinUI, Avalonia or a custom surface. You implement
`IEditorHostAdapter`: a dispatcher for the UI thread, and what the session needs from the window (present a frame,
set the cursor, capture the pointer, the clipboard, the caret for the IME, scrolling, tooltips and accessibility).

```csharp file=../../examples/dotnet/Hosting.cs#adapter
```

`session.Attach(adapter)` returns an `EditorHost`. Forward raw input to it and call `Render()` from your paint. The
engine recognises every gesture, draws the rulers into the frame and hit-tests them, so a control has no editing,
gesture or ruler logic of its own. One host is attached at a time; disposing it detaches, and the session keeps its
document, selection and history.

```csharp file=../../examples/dotnet/Hosting.cs#attach
```

## Native AOT

Both packages are trim and AOT safe. The native engine ships in `Twip.Core` for `win-x64`, `win-arm64`,
`linux-x64` and `linux-arm64`.
