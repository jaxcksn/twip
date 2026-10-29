---
title: Events
sidebar_position: 4
---

# Events

State tells you how things are; events tell you what happened. In TypeScript, `editor.on(type, listener)` returns
the unsubscribe function. In .NET each event is an `EventHandler<T>` on `EditorSession`.

```ts file=../../examples/web/events.ts
```

```csharp file=../../examples/dotnet/Events.cs#events
```

## Delivery

- **Ordered.** Events arrive in the order things happened. `documentChanged` arrives exactly once per commit, in
  commit order, and is never merged with another.
- **After the state.** When an event arrives, `getState()` (`session.State`) already reflects it.
- **Isolated.** A listener that throws doesn't stop the others or the editor. Its exception goes to the `error`
  event (`Faulted` in .NET, with `IsFatal` false), or to `console.error` when nothing listens.
- **Re-entrant safely.** A command called from a listener runs after the current dispatch, not inside it.
- **Stopped by destroy.** Nothing is delivered after `destroy()` (`Dispose()`).

A fault is different: an `error` with `fatal: true` (`Faulted` with `IsFatal`) means the engine stopped. Pending
operations reject, new ones reject, and `destroy()` still works.

## The events

| TypeScript | .NET | When |
|---|---|---|
| `documentChanged` | `DocumentChanged` | A transaction committed. Carries its record: revision, origin (`user`, `api`, `undo`, `redo`), author, time, undo label and a summary of what it touched. |
| — | `StateChanged` | The state changed. Coalesced: the latest snapshot after one or more changes. (TypeScript: `subscribe`.) |
| `selectionChanged` | `SelectionChanged` | The selection moved. Coalesced per frame. |
| `linkActivated` | `LinkActivated` | Ctrl+click on a link, or a click when read only. Open the address yourself. |
| `placeholderActivated` | `PlaceholderActivated` | A click on an unfilled picture placeholder. See [placeholders](services#picture-placeholders). |
| `contextMenu` | `ContextMenuRequested` | Right-click, the context-menu key or Shift+F10. Carries the menu model and where to show it. |
| `dialogRequested` | `DialogRequested` | A key or gesture asked for a dialog (Ctrl+D asks for `font`). |
| `hostCommand` | `HostCommandRequested` | A key the application owns: save, save as, print, open, new, find, replace, go to, insert hyperlink, next misspelling, focus the font or size box, help. The browser's default is suppressed. |
| `shortcut` | `ShortcutPressed` | An application shortcut from `shortcuts.register`. |
| `gesture` | `GestureRequested` | A pointer gesture you bound to `emit`. Mounted or attached only. |
| `wordAdded` | `WordAdded` | The user added a word to the dictionary: persist it. |
| `fontsChanged` | `FontsChanged` | The families being fetched, or that nothing could resolve, changed. |
| `elementActivated` | `ElementActivated` | Design mode: Enter or a double-click on a field or content control. |
| `elementsChanged` | `ElementsChanged` | Fields or content controls were added, removed or changed. |
| `fieldsUpdateRequested` | `FieldsUpdateRequested` | The user asked to update fields. Compute their results and answer with `setFieldResults`, passing the token. |
| `revisionsChanged` | `RevisionsChanged` | The tracked changes changed: refresh a revisions pane. |
| `commentsChanged` | `CommentsChanged` | The comments changed: refresh a comments pane. |
| `commentAnchors` | `CommentAnchorsChanged` | Where visible comments sit, after layout, zoom or scrolling. Mounted or attached only. |
| `balloonsLaidOut` | `BalloonsLaidOut` (on `DocumentEditor`) | The SDK placed the attached balloons. Mounted only. |
| `rulerChanged` | `RulerChanged` | The ruler model or its mapping to the viewport changed. Coalesced per frame. Mounted or attached only. |
| `tooltip` | `TooltipChanged` | The pointer is over a link, an element mark, a revision or a comment (`null` hides it). Mounted or attached only. |
| `announcement` | `AccessibilityAnnouncement` | What the editor said to screen readers. |
| `warning` | `Warning` | A fidelity warning, such as an unsupported element or a substituted font. |
| `error` | `Faulted` | A listener's exception, or a fault. |
