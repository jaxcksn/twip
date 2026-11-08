import SwiftUI
import Twip
import TwipUI

// #region app
@main
struct ExampleApp: App {
    var body: some Scene {
        WindowGroup("Twip") { EditorWindow() }
            .commands { DocumentCommands() }
    }
}

struct EditorWindow: View {
    @State private var session = EditorSession()

    var body: some View {
        DocumentEditor(session: session)
            .toolbar {
                Toggle("Bold", systemImage: "bold", isOn: Binding(
                    get: { session.state.formatting.bold.isOn },
                    set: { bold in Task { try await session.run(.setBold(bold)) } }
                ))
                Button("Undo", systemImage: "arrow.uturn.backward") { Task { try await session.run(.undo) } }
                    .disabled(!session.state.history.canUndo)
            }
            .navigationSubtitle("\(session.state.pages.count) pages")
            .focusedSceneValue(\.documentSession, session)
            .task {
                if let url = CommandLine.arguments.dropFirst().first.map({ URL(filePath: $0) }) {
                    _ = try? await session.open(contentsOf: url)
                }
            }
    }
}

extension FocusedValues {
    /// The session of the window in front.
    @Entry var documentSession: EditorSession?
}

/// File ▸ Save for the document in front.
struct DocumentCommands: Commands {
    @FocusedValue(\.documentSession) private var session

    var body: some Commands {
        CommandGroup(replacing: .saveItem) {
            Button("Save") { Task { try await session?.save(to: URL(filePath: "edited.docx")) } }
                .keyboardShortcut("s")
                .disabled(session == nil)
        }
    }
}
// #endregion
