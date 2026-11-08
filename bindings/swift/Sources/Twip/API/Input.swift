import Foundation

// Raw input hosts forward (crates/twip/src/api/input.rs): keys (by `KeyboardEvent.code`), modifiers, pointer and
// composition phases, and the clipboard's contents. The engine recognises shortcuts and gestures from these.
// No renames from Rust. Each enum's `abiValue` is its number in the C ABI.

/// A physical key, named and numbered as in the C ABI's key table. The raw values are `KeyboardEvent.code`'s.
public enum Key: String, Hashable, Sendable, Codable, CaseIterable {
    /// The `KeyA` key.
    case keyA = "KeyA"
    /// The `KeyB` key.
    case keyB = "KeyB"
    /// The `KeyC` key.
    case keyC = "KeyC"
    /// The `KeyD` key.
    case keyD = "KeyD"
    /// The `KeyE` key.
    case keyE = "KeyE"
    /// The `KeyF` key.
    case keyF = "KeyF"
    /// The `KeyG` key.
    case keyG = "KeyG"
    /// The `KeyH` key.
    case keyH = "KeyH"
    /// The `KeyI` key.
    case keyI = "KeyI"
    /// The `KeyJ` key.
    case keyJ = "KeyJ"
    /// The `KeyK` key.
    case keyK = "KeyK"
    /// The `KeyL` key.
    case keyL = "KeyL"
    /// The `KeyM` key.
    case keyM = "KeyM"
    /// The `KeyN` key.
    case keyN = "KeyN"
    /// The `KeyO` key.
    case keyO = "KeyO"
    /// The `KeyP` key.
    case keyP = "KeyP"
    /// The `KeyQ` key.
    case keyQ = "KeyQ"
    /// The `KeyR` key.
    case keyR = "KeyR"
    /// The `KeyS` key.
    case keyS = "KeyS"
    /// The `KeyT` key.
    case keyT = "KeyT"
    /// The `KeyU` key.
    case keyU = "KeyU"
    /// The `KeyV` key.
    case keyV = "KeyV"
    /// The `KeyW` key.
    case keyW = "KeyW"
    /// The `KeyX` key.
    case keyX = "KeyX"
    /// The `KeyY` key.
    case keyY = "KeyY"
    /// The `KeyZ` key.
    case keyZ = "KeyZ"
    /// The `Digit0` key.
    case digit0 = "Digit0"
    /// The `Digit1` key.
    case digit1 = "Digit1"
    /// The `Digit2` key.
    case digit2 = "Digit2"
    /// The `Digit3` key.
    case digit3 = "Digit3"
    /// The `Digit4` key.
    case digit4 = "Digit4"
    /// The `Digit5` key.
    case digit5 = "Digit5"
    /// The `Digit6` key.
    case digit6 = "Digit6"
    /// The `Digit7` key.
    case digit7 = "Digit7"
    /// The `Digit8` key.
    case digit8 = "Digit8"
    /// The `Digit9` key.
    case digit9 = "Digit9"
    /// The `F1` key.
    case f1 = "F1"
    /// The `F2` key.
    case f2 = "F2"
    /// The `F3` key.
    case f3 = "F3"
    /// The `F4` key.
    case f4 = "F4"
    /// The `F5` key.
    case f5 = "F5"
    /// The `F6` key.
    case f6 = "F6"
    /// The `F7` key.
    case f7 = "F7"
    /// The `F8` key.
    case f8 = "F8"
    /// The `F9` key.
    case f9 = "F9"
    /// The `F10` key.
    case f10 = "F10"
    /// The `F11` key.
    case f11 = "F11"
    /// The `F12` key.
    case f12 = "F12"
    /// The `Enter` key.
    case enter = "Enter"
    /// The `Tab` key.
    case tab = "Tab"
    /// The `Backspace` key.
    case backspace = "Backspace"
    /// The `Delete` key.
    case delete = "Delete"
    /// The `Escape` key.
    case escape = "Escape"
    /// The `Space` key.
    case space = "Space"
    /// The `Insert` key.
    case insert = "Insert"
    /// The `Home` key.
    case home = "Home"
    /// The `End` key.
    case end = "End"
    /// The `PageUp` key.
    case pageUp = "PageUp"
    /// The `PageDown` key.
    case pageDown = "PageDown"
    /// The `ArrowLeft` key.
    case arrowLeft = "ArrowLeft"
    /// The `ArrowRight` key.
    case arrowRight = "ArrowRight"
    /// The `ArrowUp` key.
    case arrowUp = "ArrowUp"
    /// The `ArrowDown` key.
    case arrowDown = "ArrowDown"
    /// The `ContextMenu` key.
    case contextMenu = "ContextMenu"
    /// The `Minus` key.
    case minus = "Minus"
    /// The `Equal` key.
    case equal = "Equal"
    /// The `BracketLeft` key.
    case bracketLeft = "BracketLeft"
    /// The `BracketRight` key.
    case bracketRight = "BracketRight"
    /// The `Backslash` key.
    case backslash = "Backslash"
    /// The `Semicolon` key.
    case semicolon = "Semicolon"
    /// The `Quote` key.
    case quote = "Quote"
    /// The `Backquote` key.
    case backquote = "Backquote"
    /// The `Comma` key.
    case comma = "Comma"
    /// The `Period` key.
    case period = "Period"
    /// The `Slash` key.
    case slash = "Slash"
    /// The `IntlBackslash` key.
    case intlBackslash = "IntlBackslash"
    /// The `NumpadEnter` key.
    case numpadEnter = "NumpadEnter"
    /// The `NumpadAdd` key.
    case numpadAdd = "NumpadAdd"
    /// The `NumpadSubtract` key.
    case numpadSubtract = "NumpadSubtract"
    /// The `NumpadMultiply` key.
    case numpadMultiply = "NumpadMultiply"
    /// The `NumpadDivide` key.
    case numpadDivide = "NumpadDivide"
    /// The `NumpadDecimal` key.
    case numpadDecimal = "NumpadDecimal"
    /// The `Numpad0` key.
    case numpad0 = "Numpad0"
    /// The `Numpad1` key.
    case numpad1 = "Numpad1"
    /// The `Numpad2` key.
    case numpad2 = "Numpad2"
    /// The `Numpad3` key.
    case numpad3 = "Numpad3"
    /// The `Numpad4` key.
    case numpad4 = "Numpad4"
    /// The `Numpad5` key.
    case numpad5 = "Numpad5"
    /// The `Numpad6` key.
    case numpad6 = "Numpad6"
    /// The `Numpad7` key.
    case numpad7 = "Numpad7"
    /// The `Numpad8` key.
    case numpad8 = "Numpad8"
    /// The `Numpad9` key.
    case numpad9 = "Numpad9"

    /// The key's `KeyboardEvent.code`.
    public var code: String { rawValue }

    /// The key's number in the C ABI's key table: 1 to 48 for letters, digits and F keys, then from 61.
    var abiValue: UInt32 {
        let i = UInt32(Self.allCases.firstIndex(of: self)!)
        return i < 48 ? i + 1 : i + 13
    }
}

/// The modifier keys held. In the C ABI a bit set: Ctrl 1, Shift 2, Alt 4, Meta (Cmd) 8.
public struct Modifiers: Hashable, Sendable, Codable {
    /// Ctrl.
    public var ctrl: Bool
    /// Shift.
    public var shift: Bool
    /// Alt (Option on a Mac).
    public var alt: Bool
    /// Meta: Cmd on a Mac, the Windows key on Windows.
    public var meta: Bool

    /// The modifiers held; none by default.
    public init(ctrl: Bool = false, shift: Bool = false, alt: Bool = false, meta: Bool = false) {
        self.ctrl = ctrl
        self.shift = shift
        self.alt = alt
        self.meta = meta
    }

    /// The C ABI's bit set.
    var abiValue: UInt32 {
        (ctrl ? 1 : 0) | (shift ? 2 : 0) | (alt ? 4 : 0) | (meta ? 8 : 0)
    }
}

/// A pointer event's kind. In the C ABI: down 0, move 1, up 2, cancel 3, leave 4 (the pointer left the view).
public enum PointerKind: String, Hashable, Sendable, Codable, CaseIterable {
    /// A button went down.
    case down
    /// The pointer moved.
    case move
    /// A button came up.
    case up
    /// The platform cancelled the pointer (a lost capture, say).
    case cancel
    /// The pointer left the view.
    case leave

    /// The number in the C ABI.
    var abiValue: UInt32 { UInt32(Self.allCases.firstIndex(of: self)!) }
}

/// The button a pointer event pressed or released. In the C ABI: primary 0, secondary 1, middle 2.
public enum PointerButton: String, Hashable, Sendable, Codable, CaseIterable {
    /// The main button (left, by default).
    case primary
    /// The context-menu button (right, by default).
    case secondary
    /// The middle button or wheel.
    case middle

    /// The number in the C ABI.
    var abiValue: UInt32 { UInt32(Self.allCases.firstIndex(of: self)!) }
}

/// An IME composition's phase. In the C ABI: start 0, update 1, end 2.
public enum CompositionPhase: String, Hashable, Sendable, Codable, CaseIterable {
    /// A composition began.
    case start
    /// The composition's text changed.
    case update
    /// The composition ended, committed or cancelled.
    case end

    /// The number in the C ABI.
    var abiValue: UInt32 { UInt32(Self.allCases.firstIndex(of: self)!) }
}

/// A point in the editor's viewport, CSS px from its top-left corner.
public struct Point: Hashable, Sendable, Codable {
    /// CSS px from the left edge.
    public var x: Double
    /// CSS px from the top edge.
    public var y: Double

    /// A point.
    public init(x: Double, y: Double) {
        self.x = x
        self.y = y
    }
}

/// What a copy puts on the clipboard, and what a paste takes from it.
public struct ClipboardData: Hashable, Sendable, Codable {
    /// The plain text (`text/plain`).
    public var text: String
    /// The HTML (`text/html`), when there is any.
    public var html: String?
    /// A token naming the engine's internal fragment: a paste in any session of the engine keeps its formatting.
    public var `internal`: String?

    /// Clipboard contents.
    public init(text: String, html: String? = nil, internal: String? = nil) {
        self.text = text
        self.html = html
        self.internal = `internal`
    }
}
