import AppKit
import Observation
import SwiftUI
import Twip

/// A session's state on the main actor, for SwiftUI: toolbars, menus and status bars read ``state`` and redraw when it
/// changes.
///
/// ```swift
/// @State private var editor = EditorModel()
///
/// var body: some View {
///     DocumentEditor(editor)
///         .toolbar {
///             Toggle("Bold", systemImage: "bold", isOn: editor.toggle(\.formatting.bold, Command.setBold))
///             Button("Undo", systemImage: "arrow.uturn.backward") { editor.perform(.undo) }
///                 .disabled(!editor.canRun(.undo))
///         }
///         .task { try? await editor.session.open(contentsOf: url) }
/// }
/// ```
@MainActor
@Observable
public final class EditorModel {
    /// The session.
    public let session: EditorSession
    /// The session's latest state.
    public private(set) var state: EditorState
    /// The last error a ``perform(_:)`` threw, until the next one succeeds.
    public private(set) var lastError: (any Error)?

    @ObservationIgnored private var watch: Task<Void, Never>?

    /// A model over `session`; a new blank session by default.
    public init(session: EditorSession = EditorSession()) {
        self.session = session
        state = session.state
        let states = session.states
        watch = Task { [weak self] in
            for await state in states {
                guard let self else { return }
                self.state = state
            }
        }
    }

    isolated deinit { watch?.cancel() }

    /// Runs a command, for a button: in order with the others, its error kept in ``lastError``.
    public func perform(_ command: Command) {
        Task {
            do {
                try await session.run(command)
                lastError = nil
            } catch {
                lastError = error
            }
        }
    }

    /// Whether a command can run now, from the state.
    public func canRun(_ command: CommandID) -> Bool {
        state.commands[command]?.enabled ?? false
    }

    /// A binding for a toggle: on when the state's value is on everywhere; setting it runs the command.
    ///
    /// `Toggle("Italic", isOn: editor.toggle(\.formatting.italic, Command.setItalic))`
    public func toggle(_ value: KeyPath<EditorState, Tri<Bool>>, _ command: @escaping (Bool) -> Command) -> Binding<Bool> {
        Binding { self.state[keyPath: value].isOn } set: { self.perform(command($0)) }
    }
}

/// A document editor: the pages of a session, drawn by the engine, with the keyboard, pointer, trackpad, input
/// methods, clipboard and context menus.
///
/// ```swift
/// DocumentEditor(editor)
///     .onHostCommand { command in if command == .save { save() } }
/// ```
public struct DocumentEditor: NSViewRepresentable {
    private let session: EditorSession
    private var onHostCommand: ((HostCommand) -> Void)?
    private var contextMenu: ((ContextMenuModel) -> ContextMenuModel?)? = { $0 }

    /// An editor presenting a model's session.
    public init(_ model: EditorModel) { session = model.session }

    /// An editor presenting a session.
    public init(session: EditorSession) { self.session = session }

    /// Handles the keys the application owns: save, print, open, new, find, replace, go to, help, ...
    public func onHostCommand(_ action: @escaping (HostCommand) -> Void) -> DocumentEditor {
        var copy = self
        copy.onHostCommand = action
        return copy
    }

    /// Changes the engine's context menu before it shows, or returns nil to show none (to present your own).
    public func contextMenu(_ transform: @escaping (ContextMenuModel) -> ContextMenuModel?) -> DocumentEditor {
        var copy = self
        copy.contextMenu = transform
        return copy
    }

    public func makeNSView(context: Context) -> NSView {
        guard let view = try? DocumentEditorView(session: session) else {
            // Another view presents this session.
            return NSHostingView(rootView: Text("This document is open in another editor.").foregroundStyle(.secondary))
        }
        update(view)
        return view
    }

    public func updateNSView(_ view: NSView, context: Context) {
        if let view = view as? DocumentEditorView { update(view) }
    }

    private func update(_ view: DocumentEditorView) {
        view.onHostCommand = onHostCommand
        view.contextMenu = contextMenu
    }
}
