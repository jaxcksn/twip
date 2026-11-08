# Twip for Swift

The Twip DOCX engine for Swift: open Word documents, edit them with typed commands, observe state and events, save
DOCX or PDF, and present them in a SwiftUI or AppKit editor. macOS 15 and Swift 6.2.

> **Alpha.** The API may change between minor versions before 1.0.

- `Twip`: `EditorSession` (one document: commands, state, events, services, fonts), `PDFConverter` (DOCX to PDF
  without a session, many at once) and every `twip::api` type as a `Codable` value.
- `TwipUI`: `DocumentEditor` (SwiftUI) and `DocumentEditorView` (AppKit), the editor itself. Toolbars, menus and
  status bars are the application's: `EditorSession` is `Observable`, so SwiftUI views that read `session.state`
  redraw when it changes.

```swift
import Twip

let session = EditorSession()
try await session.open(contentsOf: url)
try await session.run(.selectAll)
try await session.run(.setBold(true))
print(session.state.formatting.bold.isOn) // true
try await session.save(to: url)
```

The package links the native engine from this repository: build it first with
`node crates/twip-capi/build-native.mjs` (or `moon run twip-capi:build`), then `swift build` or `swift test` here.
The tests round-trip every wire fixture and run the contract scripts in `tests/contract`. See the Swift guide in
`docs/docs/guides/swift.md`.
