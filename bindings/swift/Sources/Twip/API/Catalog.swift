import Foundation

// The command catalog: every command's id, label, group, shortcuts and undo grouping, for building toolbars, menus
// and shortcut help (crates/twip/src/api/catalog.rs).

/// Where a command belongs in a UI.
public enum CommandGroup: String, Hashable, Sendable, Codable, CaseIterable {
    /// Typing, breaks and symbols.
    case text
    /// Cut, copy and paste.
    case clipboard
    /// Undo and redo.
    case history
    /// Selecting and moving through the document.
    case selection
    /// Character formatting.
    case character
    /// Paragraph formatting.
    case paragraph
    /// Tab stops.
    case tabs
    /// Page setup: margins, orientation, paper size and columns.
    case page
    /// Headers and footers.
    case headerFooter
    /// Inserting tables, pictures, text boxes, notes, hyperlinks and fields.
    case insert
    /// Tables.
    case table
    /// Pictures and other drawing objects.
    case object
    /// Fields.
    case field
    /// Content controls, repeating sections, elements and placeholders.
    case control
    /// Custom XML data parts.
    case data
    /// Revisions and comments.
    case review
    /// Zoom, view options and mode.
    case view
}

/// How a command's changes undo.
public enum UndoGrouping: String, Hashable, Sendable, Codable, CaseIterable {
    /// One undo step.
    case step
    /// Consecutive typing in one paragraph joins one step.
    case typing
    /// Not an edit: no undo step and no revision.
    case view
}

/// A command's keys, as display strings (`Ctrl+B`, `⌘B`).
public struct CommandShortcuts: Hashable, Sendable, Codable {
    /// The Windows keys.
    public var windows: [String]
    /// The Mac keys.
    public var mac: [String]

    /// A command's keys.
    public init(windows: [String] = [], mac: [String] = []) {
        self.windows = windows
        self.mac = mac
    }
}

/// One catalog entry.
public struct CommandInfo: Hashable, Sendable, Codable, Identifiable {
    /// The command.
    public var id: CommandID
    /// English.
    public var label: String
    /// The group it belongs to, for building menus and ribbons.
    public var group: CommandGroup
    /// Its keys.
    public var shortcuts: CommandShortcuts
    /// Whether it takes arguments.
    public var takesArguments: Bool
    /// Whether it changes the document.
    public var mutates: Bool
    /// Its undo grouping.
    public var undo: UndoGrouping

    /// A catalog entry.
    public init(
        id: CommandID, label: String, group: CommandGroup, shortcuts: CommandShortcuts = CommandShortcuts(),
        takesArguments: Bool, mutates: Bool, undo: UndoGrouping
    ) {
        self.id = id
        self.label = label
        self.group = group
        self.shortcuts = shortcuts
        self.takesArguments = takesArguments
        self.mutates = mutates
        self.undo = undo
    }
}
