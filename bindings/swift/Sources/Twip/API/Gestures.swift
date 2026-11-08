import Foundation

// Shortcuts, key bindings and gestures, from crates/twip/src/api/services.rs.
// Renames: GestureId → GestureID.

// MARK: - Shortcuts and key bindings

/// A key on every platform, or one per platform (`Ctrl+Alt+K`).
public enum ShortcutKeys: Hashable, Sendable, Codable {
    /// The same keys everywhere.
    case same(String)
    /// Different keys per platform: on Windows and Linux, and on macOS.
    case perPlatform(windows: String, mac: String)

    enum CodingKeys: String, CodingKey {
        case windows, mac
    }

    public init(from decoder: any Decoder) throws {
        if let keys = try? decoder.singleValueContainer().decode(String.self) {
            self = .same(keys)
            return
        }
        let c = try decoder.container(keyedBy: CodingKeys.self)
        self = .perPlatform(windows: try c.decode(String.self, forKey: .windows), mac: try c.decode(String.self, forKey: .mac))
    }

    public func encode(to encoder: any Encoder) throws {
        switch self {
        case .same(let keys):
            var c = encoder.singleValueContainer()
            try c.encode(keys)
        case .perPlatform(let windows, let mac):
            var c = encoder.container(keyedBy: CodingKeys.self)
            try c.encode(windows, forKey: .windows)
            try c.encode(mac, forKey: .mac)
        }
    }
}

/// An application shortcut: pressing its keys emits `shortcut` with its id.
public struct Shortcut: Hashable, Sendable, Codable, Identifiable {
    /// The application's id for it.
    public var id: String
    /// Its keys.
    public var keys: ShortcutKeys

    /// A shortcut.
    public init(id: String, keys: ShortcutKeys) {
        self.id = id
        self.keys = keys
    }
}

/// What a key does now.
public enum KeyBinding: Hashable, Sendable, Codable {
    /// Runs a command.
    case command(id: CommandID)
    /// Emits an application shortcut, by its id.
    case shortcut(id: String)
    /// Emits a host command (Save, Print, Find).
    case host(command: HostCommand)

    public init(from decoder: any Decoder) throws {
        let t = try TaggedDecoder(decoder, tag: "kind")
        switch t.tag {
        case "command": self = .command(id: try t["id"])
        case "shortcut": self = .shortcut(id: try t["id"])
        case "host": self = .host(command: try t["command"])
        default: throw t.unknown()
        }
    }

    public func encode(to encoder: any Encoder) throws {
        let t = TaggedEncoder(encoder, tag: "kind")
        switch self {
        case .command(let id): try t.tag("command", ["id": id])
        case .shortcut(let id): try t.tag("shortcut", ["id": id])
        case .host(let command): try t.tag("host", ["command": command])
        }
    }
}

// MARK: - Gestures

/// The pointer gestures the engine recognises.
public enum GestureID: String, Hashable, Sendable, Codable, CaseIterable {
    /// A double-click in the header area: edits the header.
    case headerAreaDoubleClick
    /// A double-click in the footer area: edits the footer.
    case footerAreaDoubleClick
    /// A double-click on the body while editing a header or footer: back to the body.
    case bodyDoubleClickInHeaderFooter
    /// A double-click on text: selects a word.
    case textDoubleClick
    /// A triple-click on text: selects a paragraph.
    case textTripleClick
    /// A Ctrl+click on text.
    case textCtrlClick
    /// A Ctrl+click on a hyperlink.
    case linkActivate
    /// A click in the left margin.
    case marginClick
    /// A double-click in the left margin.
    case marginDoubleClick
    /// A double-click on a table's column border.
    case tableBorderDoubleClick
    /// Dragging a table border: resizes.
    case tableBorderDrag
    /// Dragging a picture or text box.
    case objectDrag
    /// Dragging selected text: moves it.
    case textDrag
    /// A double-click on a field or content control in design mode.
    case elementDoubleClick
    /// A click on an unfilled picture control in editing and read-only modes: activates it.
    case pictureControlClick
    /// A double-click on a picture control in editing and read-only modes: activates it.
    case pictureControlDoubleClick
    /// A click on the ruler: adds a tab stop.
    case rulerClick
    /// Dragging a ruler marker.
    case rulerDrag
    /// Dragging a tab stop off the ruler: removes it.
    case rulerDragOff
    /// A double-click on the ruler: requests the Tabs dialog.
    case rulerDoubleClick
    /// A right-click: the context menu.
    case contextClick
}

/// What a gesture does: its default action, an event for the application, nothing, or another command.
/// On the wire: `"default"`, `"emit"`, `"none"` or `{"op": command}`.
public enum GestureAction: Hashable, Sendable, Codable {
    /// Its built-in action.
    case `default`
    /// Emits `gesture` instead.
    case emit
    /// Does nothing.
    case none
    /// Runs this command instead.
    indirect case op(Command)

    enum CodingKeys: String, CodingKey {
        case op
    }

    public init(from decoder: any Decoder) throws {
        if let name = try? decoder.singleValueContainer().decode(String.self) {
            switch name {
            case "default": self = .default
            case "emit": self = .emit
            case "none": self = .none
            default:
                throw DecodingError.dataCorrupted(.init(codingPath: decoder.codingPath, debugDescription: "unknown gesture action \(name)"))
            }
            return
        }
        self = .op(try decoder.container(keyedBy: CodingKeys.self).decode(Command.self, forKey: .op))
    }

    public func encode(to encoder: any Encoder) throws {
        let name: String
        switch self {
        case .default: name = "default"
        case .emit: name = "emit"
        case .none: name = "none"
        case .op(let command):
            var c = encoder.container(keyedBy: CodingKeys.self)
            try c.encode(command, forKey: .op)
            return
        }
        var c = encoder.singleValueContainer()
        try c.encode(name)
    }
}

/// A gesture's binding, with a label for a settings screen (`Double-click the header area`).
public struct GestureBinding: Hashable, Sendable, Codable {
    /// The gesture.
    public var gesture: GestureID
    /// What it does.
    public var action: GestureAction
    /// For example `Double-click the header area`.
    public var label: String

    /// A binding.
    public init(gesture: GestureID, action: GestureAction = .default, label: String) {
        self.gesture = gesture
        self.action = action
        self.label = label
    }
}

/// Page areas a gesture can be over.
public enum AreaKind: String, Hashable, Sendable, Codable, CaseIterable {
    /// The header area.
    case headerArea
    /// The footer area.
    case footerArea
    /// The left margin.
    case margin
    /// A ruler.
    case ruler
    /// A table border.
    case tableBorder
}

/// What a gesture is over.
public enum GestureTarget: Hashable, Sendable, Codable {
    /// Document content, as a context menu names it.
    case menu(MenuTarget)
    /// A page area, of this kind.
    case area(kind: AreaKind)

    enum CodingKeys: String, CodingKey {
        case kind
    }

    public init(from decoder: any Decoder) throws {
        if let target = try? MenuTarget(from: decoder) {
            self = .menu(target)
            return
        }
        self = .area(kind: try decoder.container(keyedBy: CodingKeys.self).decode(AreaKind.self, forKey: .kind))
    }

    public func encode(to encoder: any Encoder) throws {
        switch self {
        case .menu(let target): try target.encode(to: encoder)
        case .area(let kind):
            var c = encoder.container(keyedBy: CodingKeys.self)
            try c.encode(kind, forKey: .kind)
        }
    }
}

/// A gesture bound to `emit`.
public struct GestureEvent: Hashable, Sendable, Codable {
    /// The gesture.
    public var gesture: GestureID
    /// Where.
    public var at: ClientPoint
    /// The page.
    public var page: Int
    /// What is under the point.
    public var target: GestureTarget

    /// A gesture event.
    public init(gesture: GestureID, at: ClientPoint, page: Int, target: GestureTarget) {
        self.gesture = gesture
        self.at = at
        self.page = page
        self.target = target
    }
}
