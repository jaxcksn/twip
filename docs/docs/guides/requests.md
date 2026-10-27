---
title: Requests and events
sidebar_position: 6
---

# Requests and events

After an input the editor may need something from your app: to scroll the caret into view, change the cursor, put a
copy on the clipboard. It doesn't call back; you take its requests after each input (or each frame) and act on them.

```csharp
using var requests = editor.TakeRequests();
for (nuint i = 0; i < requests.Len; i++)
{
    using var r = requests.Get(i)!;
    switch (r.Kind)
    {
        case RequestKind.ScrollIntoView: ScrollTo(r.Page, r.Rect); break;     // a rectangle on a page, points
        case RequestKind.Cursor: SetCursor(r.Cursor()); break;                // a CSS cursor name: text, move, ...
        case RequestKind.Clipboard: SetClipboard(r.Text(), r.Html()); break;  // Ctrl+C / Ctrl+X
        case RequestKind.CapturePointer: CapturePointer(); break;
        case RequestKind.ReleasePointer: ReleasePointer(); break;
        case RequestKind.Autoscroll: StartAutoscroll(r.VelocityX, r.VelocityY); break;
    }
}
```

```js
const requests = editor.takeRequests();
for (let i = 0; i < requests.len; i++) {
  const r = requests.get(i);
  if (r.kind.value === 'Cursor') canvas.style.cursor = r.cursor();
}
```

## Events

Events are things that happened in the document your application may act on, taken the same way with
`TakeEvents()`:

| Event | Meaning |
|---|---|
| `Placeholder` | An unfilled picture placeholder was activated: supply its image with `FillPlaceholder(key, bytes)`. |
| `Activate` | A field or content control was activated in design mode (`Id()`). |
| `FieldUpdate` | The user asked to update fields: compute each field's result (`FieldId(i)`, `FieldCode(i)`) and pass them back with `Token`. |
| `Command` | A key asked for an application command, such as Find or Hyperlink (`Name()`): open your UI for it. |
