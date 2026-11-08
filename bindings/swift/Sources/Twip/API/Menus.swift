import Foundation

// Client points and the context menu (crates/twip/src/api/services.rs).
//
// Renamed from Rust: `ContextMenu` is `ContextMenuModel` (SwiftUI has a `ContextMenu`).

/// A point relative to the browser viewport (or the control), CSS px: where to place a fixed-position menu.
public struct ClientPoint: Hashable, Sendable, Codable {
    /// From the viewport's left edge.
    public var clientX: Double
    /// From the viewport's top edge.
    public var clientY: Double

    /// The point (`clientX`, `clientY`).
    public init(clientX: Double = 0, clientY: Double = 0) {
        self.clientX = clientX
        self.clientY = clientY
    }
}

/// A rectangle in the `ClientPoint` space.
public struct ClientRect: Hashable, Sendable, Codable {
    /// Its left edge.
    public var left: Double
    /// Its top edge.
    public var top: Double
    /// Its width.
    public var width: Double
    /// Its height.
    public var height: Double

    /// The rectangle.
    public init(left: Double = 0, top: Double = 0, width: Double = 0, height: Double = 0) {
        self.left = left
        self.top = top
        self.width = width
        self.height = height
    }
}

/// A point relative to the editor's balloon layer, CSS px: it scrolls and zooms with the document.
public struct ContentPoint: Hashable, Sendable, Codable {
    /// From the layer's left edge.
    public var x: Double
    /// From the layer's top edge.
    public var y: Double

    /// The point (`x`, `y`).
    public init(x: Double = 0, y: Double = 0) {
        self.x = x
        self.y = y
    }
}

/// The context menu for the selection.
public struct ContextMenuModel: Hashable, Sendable, Codable {
    /// What the menu is for.
    public var target: MenuTarget
    /// Its entries, in order.
    public var items: [MenuItem]

    /// The menu for `target`.
    public init(target: MenuTarget, items: [MenuItem]) {
        self.target = target
        self.items = items
    }
}

/// What the menu is for.
public enum MenuTarget: Hashable, Sendable, Codable {
    /// Text.
    case text
    /// A misspelled word.
    case misspelling(word: String, language: String)
    /// A hyperlink, at `address`.
    case link(address: String)
    /// A table.
    case table
    /// A picture.
    case picture
    /// A text box.
    case textBox
    /// A field or content control.
    case element(id: ElementID)
    /// A tracked change.
    case revision(id: RevisionID)
    /// A comment.
    case comment(id: CommentID)
    /// A header or footer.
    case headerFooter

    public init(from decoder: any Decoder) throws {
        let t = try TaggedDecoder(decoder, tag: "kind")
        switch t.tag {
        case "text": self = .text
        case "misspelling": self = .misspelling(word: try t["word"], language: try t["language"])
        case "link": self = .link(address: try t["address"])
        case "table": self = .table
        case "picture": self = .picture
        case "textBox": self = .textBox
        case "element": self = .element(id: try t["id"])
        case "revision": self = .revision(id: try t["id"])
        case "comment": self = .comment(id: try t["id"])
        case "headerFooter": self = .headerFooter
        default: throw t.unknown()
        }
    }

    public func encode(to encoder: any Encoder) throws {
        let t = TaggedEncoder(encoder, tag: "kind")
        switch self {
        case .text: try t.tag("text")
        case .misspelling(let word, let language): try t.tag("misspelling", ["word": word, "language": language])
        case .link(let address): try t.tag("link", ["address": address])
        case .table: try t.tag("table")
        case .picture: try t.tag("picture")
        case .textBox: try t.tag("textBox")
        case .element(let id): try t.tag("element", ["id": id])
        case .revision(let id): try t.tag("revision", ["id": id])
        case .comment(let id): try t.tag("comment", ["id": id])
        case .headerFooter: try t.tag("headerFooter")
        }
    }
}

/// A menu entry.
public enum MenuItem: Hashable, Sendable, Codable {
    /// A command, as the catalog describes it: its English `label`, whether it is `enabled` now, its toggle state
    /// (`checked`, for a toggle), its display `shortcut` (`Ctrl+X`), and what choosing it does.
    case command(
        command: CommandID, label: String, enabled: Bool, checked: Bool? = nil, shortcut: String? = nil,
        action: MenuAction)
    /// A spelling suggestion, Ignore or Add to Dictionary: its label, and what choosing it does.
    case suggestion(label: String, action: MenuAction)
    /// A submenu: its label and entries.
    case submenu(label: String, items: [MenuItem])
    /// A separator line.
    case separator

    public init(from decoder: any Decoder) throws {
        let t = try TaggedDecoder(decoder, tag: "kind")
        switch t.tag {
        case "command":
            self = .command(
                command: try t["command"], label: try t["label"], enabled: try t["enabled"],
                checked: try t.optional("checked"), shortcut: try t.optional("shortcut"), action: try t["action"])
        case "suggestion": self = .suggestion(label: try t["label"], action: try t["action"])
        case "submenu": self = .submenu(label: try t["label"], items: try t["items"])
        case "separator": self = .separator
        default: throw t.unknown()
        }
    }

    public func encode(to encoder: any Encoder) throws {
        let t = TaggedEncoder(encoder, tag: "kind")
        switch self {
        case .command(let command, let label, let enabled, let checked, let shortcut, let action):
            try t.tag("command", [
                "command": command, "label": label, "enabled": enabled, "checked": checked, "shortcut": shortcut,
                "action": action,
            ])
        case .suggestion(let label, let action): try t.tag("suggestion", ["label": label, "action": action])
        case .submenu(let label, let items): try t.tag("submenu", ["label": label, "items": items])
        case .separator: try t.tag("separator")
        }
    }
}

/// What a menu item does: pass it back to `invoke`.
public enum MenuAction: Hashable, Sendable, Codable {
    /// Runs a command.
    case command(command: Command)
    /// Replaces the misspelling at the selection `with` a word.
    case replaceWord(with: String)
    /// Ignores a word for this session.
    case ignoreWord(word: String)
    /// Adds a word to the user's dictionary (emits `wordAdded`), with its language tag.
    case addWord(word: String, language: String)

    public init(from decoder: any Decoder) throws {
        let t = try TaggedDecoder(decoder)
        switch t.tag {
        case "command": self = .command(command: try t["command"])
        case "replaceWord": self = .replaceWord(with: try t["with"])
        case "ignoreWord": self = .ignoreWord(word: try t["word"])
        case "addWord": self = .addWord(word: try t["word"], language: try t["language"])
        default: throw t.unknown()
        }
    }

    public func encode(to encoder: any Encoder) throws {
        let t = TaggedEncoder(encoder)
        switch self {
        case .command(let command): try t.tag("command", ["command": command])
        case .replaceWord(let with): try t.tag("replaceWord", ["with": with])
        case .ignoreWord(let word): try t.tag("ignoreWord", ["word": word])
        case .addWord(let word, let language): try t.tag("addWord", ["word": word, "language": language])
        }
    }
}

/// How a context menu was opened.
public enum MenuVia: String, Hashable, Sendable, Codable, CaseIterable {
    /// A right-click.
    case pointer
    /// The context-menu key or Shift+F10.
    case keyboard
}

/// A context menu to open at a point.
public struct ContextMenuEvent: Hashable, Sendable, Codable {
    /// The menu.
    public var menu: ContextMenuModel
    /// Where to show it.
    public var at: ClientPoint
    /// A right-click, or the keyboard.
    public var via: MenuVia

    /// The `menu` to open `at` a point, opened `via` a right-click or the keyboard.
    public init(menu: ContextMenuModel, at: ClientPoint, via: MenuVia = .pointer) {
        self.menu = menu
        self.at = at
        self.via = via
    }
}
