import AppKit
import Carbon.HIToolbox
import CTwip
import QuartzCore
import Twip

/// An AppKit view that presents a session: it draws the pages (with Metal, or the CPU where there's no GPU), takes the
/// keyboard, pointer, trackpad and input methods, and handles the clipboard, cursors, scrolling and context menus. The
/// engine draws every pixel, rulers included. For SwiftUI, use ``DocumentEditor``.
@MainActor
public final class DocumentEditorView: NSView, @preconcurrency NSTextInputClient {
    /// The presented session.
    public let session: EditorSession
    /// Called for the keys the application owns (save, print, find, ...); the view does nothing with them.
    public var onHostCommand: ((HostCommand) -> Void)?
    /// Called before the engine's context menu shows, to change it or return nil to show none (or your own).
    public var contextMenu: ((ContextMenuModel) -> ContextMenuModel?)? = { $0 }

    private let host: EditorHost
    private var metal: CAMetalLayer?
    /// The viewport's top-left in content points; negative x centres content narrower than the view.
    private var scroll = CGPoint.zero
    private var sent = CGPoint(x: CGFloat.nan, y: .nan)
    /// A `scroll` request (a zoom), applied after the next frame sizes the content for it.
    private var pendingScroll: CGPoint?
    private var content = CGSize.zero
    private var caret = CGRect.zero
    private var cursor = NSCursor.iBeam
    private var marked: String?
    /// The secondary button for a Control-click, kept for its release.
    private var pressed: UInt32 = 0
    private let relay = Relay()

    /// Presents `session`.
    /// - Throws: ``ErrorCode/unavailable`` when another view presents it.
    public init(session: EditorSession = EditorSession()) throws {
        self.session = session
        host = try EditorHost(session: session, delegate: relay)
        super.init(frame: .zero)
        relay.view = self
        wantsLayer = true
        layerContentsRedrawPolicy = .never
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is not supported") }

    isolated deinit { host.detach() }

    public override var isFlipped: Bool { true }
    public override var acceptsFirstResponder: Bool { true }

    // MARK: Surface and frames

    public override func layout() {
        super.layout()
        metal?.frame = bounds
        surface()
    }

    public override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        guard window != nil, metal == nil, let layer else { return surface() }
        let m = CAMetalLayer()
        m.isOpaque = true
        m.frame = bounds
        m.contentsScale = window?.backingScaleFactor ?? 2
        if host.attach(m) != nil {
            layer.addSublayer(m)
            metal = m
        }
        surface()
    }

    public override func viewDidChangeBackingProperties() {
        super.viewDidChangeBackingProperties()
        surface()
    }

    private func surface() {
        let scale = window?.backingScaleFactor ?? 2
        layer?.contentsScale = scale
        metal?.contentsScale = scale
        host.resize(width: bounds.width, height: bounds.height, scale: scale)
    }

    fileprivate func drew(_ frame: FrameInfo, _ image: CGImage?) {
        if let image {
            metal?.isHidden = true
            layer?.contents = image
        }
        content = CGSize(width: frame.contentWidth, height: frame.contentHeight)
        if let p = pendingScroll {
            pendingScroll = nil
            scroll = p
            sent = p
        }
        clampScroll()
    }

    // MARK: Scrolling (the view owns it: the engine draws from wherever it is told)

    private func clampScroll() {
        scroll.x = content.width <= bounds.width ? -((bounds.width - content.width) / 2).rounded(.down)
            : min(max(scroll.x, 0), content.width - bounds.width)
        scroll.y = min(max(scroll.y, 0), max(0, content.height - bounds.height))
        guard scroll != sent else { return }
        sent = scroll
        host.scroll(to: scroll)
    }

    public override func scrollWheel(with event: NSEvent) {
        let mods = Self.mods(event.modifierFlags)
        if mods & (TWIP_MOD_CTRL | TWIP_MOD_META) != 0 {
            // Cmd+wheel zooms: the engine takes 100 a notch for 10%.
            host.wheel(dx: 0, dy: -event.scrollingDeltaY * (event.hasPreciseScrollingDeltas ? 1 : 100), modifiers: mods)
            return
        }
        let line: CGFloat = event.hasPreciseScrollingDeltas ? 1 : 40
        scroll.x -= event.scrollingDeltaX * line
        scroll.y -= event.scrollingDeltaY * line
        clampScroll()
    }

    public override func magnify(with event: NSEvent) {
        // A pinch of m zooms by 1 + m; the engine zooms 1.1^(-dy/100).
        host.wheel(dx: 0, dy: -100 * log(max(0.01, 1 + event.magnification)) / log(1.1), modifiers: TWIP_MOD_CTRL)
    }

    /// The least scrolling that shows a rectangle in content points.
    private func scroll(toShow r: CGRect) {
        if r.minY < scroll.y { scroll.y = r.minY } else if r.maxY > scroll.y + bounds.height { scroll.y = r.maxY - bounds.height }
        if r.minX < scroll.x { scroll.x = r.minX } else if r.maxX > scroll.x + bounds.width { scroll.x = r.maxX - bounds.width }
        clampScroll()
    }

    // MARK: The engine's requests

    fileprivate func handle(_ request: HostRequest) {
        switch request {
        case .cursor(let shape):
            cursor = Self.cursor(shape)
            window?.invalidateCursorRects(for: self)
            cursor.set()
        case .clipboard(let data):
            Self.write(data)
        case .caret(let x, let y, let height, _):
            caret = CGRect(x: x, y: y, width: 1, height: height)
        case .scroll(let x, let y):
            pendingScroll = CGPoint(x: x, y: y)
        case .scrollTo(let x, let y, let width, let height):
            scroll(toShow: CGRect(x: x, y: y, width: width, height: height))
        default:
            // capture and release: AppKit already sends a drag to the view it started in.
            // ponytail: the accessibility tree (a11y, caretText) isn't bridged to NSAccessibility yet.
            break
        }
    }

    fileprivate func handle(_ event: EditorEvent) {
        if case .contextMenu(let e) = event, let menu = contextMenu?(e.menu) {
            let at = CGPoint(x: e.at.clientX, y: e.at.clientY)
            NSMenu.popUpContextMenu(Self.menu(menu.items, invoke: invoke), with: NSApp.currentEvent ?? Self.event(at, in: self), for: self)
        }
    }

    private func invoke(_ action: MenuAction) {
        Task { try? await session.menus.invoke(action) }
    }

    private static func menu(_ items: [MenuItem], invoke: @escaping @MainActor (MenuAction) -> Void) -> NSMenu {
        let menu = NSMenu()
        menu.autoenablesItems = false
        for item in items {
            switch item {
            case .command(_, let label, let enabled, let checked, _, let action):
                let i = ActionItem(label, action, invoke)
                i.isEnabled = enabled
                i.state = checked == true ? .on : .off
                menu.addItem(i)
            case .suggestion(let label, let action):
                menu.addItem(ActionItem(label, action, invoke))
            case .submenu(let label, let items):
                let i = NSMenuItem(title: label, action: nil, keyEquivalent: "")
                i.submenu = Self.menu(items, invoke: invoke)
                menu.addItem(i)
            case .separator:
                menu.addItem(.separator())
            }
        }
        return menu
    }

    private static func event(_ at: CGPoint, in view: NSView) -> NSEvent {
        let p = view.convert(at, to: nil)
        return NSEvent.mouseEvent(with: .rightMouseDown, location: p, modifierFlags: [], timestamp: 0,
                                  windowNumber: view.window?.windowNumber ?? 0, context: nil, eventNumber: 0, clickCount: 1, pressure: 1)!
    }

    public override func resetCursorRects() {
        addCursorRect(bounds, cursor: cursor)
    }

    private static func cursor(_ shape: Cursor) -> NSCursor {
        switch shape {
        case .text: .iBeam
        case .pointer: .pointingHand
        case .move: .openHand
        case .resizeHorizontal: .columnResize
        case .resizeVertical: .rowResize
        case .resizeNorthWestSouthEast: .frameResize(position: .topLeft, directions: .all)
        case .resizeNorthEastSouthWest: .frameResize(position: .topRight, directions: .all)
        case .crosshair: .crosshair
        case .notAllowed: .operationNotAllowed
        case .copy: .dragCopy
        // ponytail: cell, column and row select and rotate show the arrow; custom NSImage cursors when they matter.
        default: .arrow
        }
    }

    // MARK: Focus

    public override func becomeFirstResponder() -> Bool {
        host.focus(true)
        return true
    }

    public override func resignFirstResponder() -> Bool {
        host.focus(false)
        return true
    }

    // MARK: Pointer

    public override func updateTrackingAreas() {
        super.updateTrackingAreas()
        trackingAreas.forEach(removeTrackingArea)
        addTrackingArea(NSTrackingArea(rect: .zero, options: [.mouseMoved, .mouseEnteredAndExited, .activeInKeyWindow, .inVisibleRect], owner: self))
    }

    /// kind: down 0, move 1, up 2, leave 4. button: primary 0, secondary 1, middle 2.
    private func pointer(_ kind: UInt32, _ event: NSEvent, button: UInt32 = 0) {
        let p = convert(event.locationInWindow, from: nil)
        let clicks = event.type == .mouseMoved || event.type == .mouseExited ? 0 : event.clickCount
        host.pointer(kind, x: p.x, y: p.y, button: button, clicks: clicks, modifiers: Self.mods(event.modifierFlags))
    }

    public override func mouseDown(with event: NSEvent) {
        window?.makeFirstResponder(self)
        pressed = event.modifierFlags.contains(.control) ? 1 : 0 // Control-click is the secondary button
        pointer(0, event, button: pressed)
    }

    public override func mouseDragged(with event: NSEvent) { pointer(1, event, button: pressed) }
    public override func mouseUp(with event: NSEvent) { pointer(2, event, button: pressed) }
    public override func mouseMoved(with event: NSEvent) { pointer(1, event) }
    public override func mouseExited(with event: NSEvent) { pointer(4, event) }
    public override func rightMouseDown(with event: NSEvent) {
        window?.makeFirstResponder(self)
        pointer(0, event, button: 1)
    }
    public override func rightMouseDragged(with event: NSEvent) { pointer(1, event, button: 1) }
    public override func rightMouseUp(with event: NSEvent) { pointer(2, event, button: 1) }
    public override func otherMouseDown(with event: NSEvent) { pointer(0, event, button: 2) }
    public override func otherMouseUp(with event: NSEvent) { pointer(2, event, button: 2) }
    public override func flagsChanged(with event: NSEvent) { host.modifiers(Self.mods(event.modifierFlags)) }

    // MARK: Keys

    public override func keyDown(with event: NSEvent) {
        // An input method composing owns the keys until it commits.
        if marked != nil { return interpretKeyEvents([event]) }
        if let key = Keys.byCode[Int(event.keyCode)] {
            let (k, m) = Keys.macChord(key, Self.mods(event.modifierFlags))
            switch host.key(k, modifiers: m) {
            case .handled, .shortcut: return
            case .hostCommand(let command):
                onHostCommand?(command)
                return
            case .unhandled: break
            }
        }
        // Unhandled Cmd and Ctrl chords go up the responder chain (the menus); everything else is typing.
        if event.modifierFlags.intersection([.command, .control]).isEmpty {
            interpretKeyEvents([event])
        } else {
            super.keyDown(with: event)
        }
    }

    static func mods(_ f: NSEvent.ModifierFlags) -> UInt32 {
        (f.contains(.control) ? TWIP_MOD_CTRL : 0) | (f.contains(.shift) ? TWIP_MOD_SHIFT : 0)
            | (f.contains(.option) ? TWIP_MOD_ALT : 0) | (f.contains(.command) ? TWIP_MOD_META : 0)
    }

    // MARK: Clipboard: the Edit menu's actions; the keyboard's cut and copy come back as a clipboard request

    /// Copies the selection to the general pasteboard.
    @objc public func copy(_ sender: Any?) { copy(cut: false) }
    /// Cuts the selection to the general pasteboard.
    @objc public func cut(_ sender: Any?) { copy(cut: true) }
    /// Pastes the general pasteboard's content.
    @objc public func paste(_ sender: Any?) { paste(asText: false) }
    /// Pastes the general pasteboard's text alone.
    @objc public func pasteAsPlainText(_ sender: Any?) { paste(asText: true) }
    /// Selects the whole story.
    @objc public override func selectAll(_ sender: Any?) { Task { try? await session.run(.selectAll) } }
    /// Undoes the last step.
    @objc public func undo(_ sender: Any?) { Task { try? await session.run(.undo) } }
    /// Redoes the last undone step.
    @objc public func redo(_ sender: Any?) { Task { try? await session.run(.redo) } }

    private func copy(cut: Bool) {
        Task {
            if let data = try? await session.clipboard.copy(cut: cut) { Self.write(data) }
        }
    }

    /// The last copy's text and token: the pasteboard has no place for the token, so a paste brings it back.
    private static var lastCopy: (text: String, internal: String)?

    private static func write(_ data: ClipboardData) {
        let pb = NSPasteboard.general
        pb.clearContents()
        pb.setString(data.text, forType: .string)
        if let html = data.html { pb.setString(html, forType: .html) }
        lastCopy = data.internal.map { (data.text, $0) }
    }

    private func paste(asText: Bool) {
        let pb = NSPasteboard.general
        guard let text = pb.string(forType: .string) else { return }
        var data = ClipboardData(text: text, html: pb.string(forType: .html))
        if let c = Self.lastCopy, c.text == text { data.internal = c.internal }
        Task { try? await session.clipboard.paste(data, asText: asText) }
    }

    // MARK: NSTextInputClient: typing, dead keys, the accent menu and input methods

    public func insertText(_ string: Any, replacementRange: NSRange) {
        let text = (string as? NSAttributedString)?.string ?? string as? String ?? ""
        if marked != nil {
            marked = nil
            host.composition(2, text)
        } else if !text.isEmpty {
            host.text(text)
        }
    }

    public func setMarkedText(_ string: Any, selectedRange: NSRange, replacementRange: NSRange) {
        let text = (string as? NSAttributedString)?.string ?? string as? String ?? ""
        if marked == nil { host.composition(0, "") }
        marked = text
        host.composition(1, text)
    }

    public func unmarkText() {
        guard let text = marked else { return }
        marked = nil
        host.composition(2, text)
    }

    public func hasMarkedText() -> Bool { marked != nil }
    public func markedRange() -> NSRange { marked.map { NSRange(location: 0, length: $0.utf16.count) } ?? NSRange(location: NSNotFound, length: 0) }
    public func selectedRange() -> NSRange { NSRange(location: marked?.utf16.count ?? 0, length: 0) }
    public func validAttributesForMarkedText() -> [NSAttributedString.Key] { [] }
    public func attributedSubstring(forProposedRange range: NSRange, actualRange: NSRangePointer?) -> NSAttributedString? { nil }
    public func characterIndex(for point: NSPoint) -> Int { NSNotFound }

    /// Where the input method's window goes: under the caret, in screen coordinates.
    public func firstRect(forCharacterRange range: NSRange, actualRange: NSRangePointer?) -> NSRect {
        guard let window else { return .zero }
        return window.convertToScreen(convert(caret.offsetBy(dx: -scroll.x, dy: -scroll.y), to: nil))
    }

    /// Editing keys reach the engine as keys; the selectors `interpretKeyEvents` maps them to do nothing.
    public override func doCommand(by selector: Selector) {}
}

/// A menu item that runs an engine menu action.
@MainActor
private final class ActionItem: NSMenuItem {
    private let menuAction: MenuAction
    private let invoke: @MainActor (MenuAction) -> Void

    init(_ title: String, _ action: MenuAction, _ invoke: @escaping @MainActor (MenuAction) -> Void) {
        menuAction = action
        self.invoke = invoke
        super.init(title: title, action: #selector(choose), keyEquivalent: "")
        target = self
    }

    @available(*, unavailable)
    required init(coder: NSCoder) { fatalError("init(coder:) is not supported") }

    @objc private func choose() { invoke(menuAction) }
}

/// Hands the engine thread's callbacks to the view on the main actor.
private final class Relay: EditorHostDelegate, @unchecked Sendable {
    @MainActor weak var view: DocumentEditorView?

    func host(_ host: EditorHost, didRequest request: HostRequest) {
        Task { @MainActor in view?.handle(request) }
    }

    func host(_ host: EditorHost, didDraw frame: FrameInfo, image: CGImage?) {
        let image = UnsafeImage(image)
        Task { @MainActor in view?.drew(frame, image.value) }
    }

    func host(_ host: EditorHost, didReceive event: EditorEvent) {
        Task { @MainActor in view?.handle(event) }
    }
}

/// A finished frame's image, handed to the main actor once.
private struct UnsafeImage: @unchecked Sendable {
    let value: CGImage?
    init(_ value: CGImage?) { self.value = value }
}

/// macOS virtual key codes as the engine's keys (`KeyboardEvent.code`: the key's position, not its character).
enum Keys {
    /// The Mac's own text-navigation chords as the engine's keymap names them.
    // ponytail: remapped in the view; the engine's keymap should own a Mac layout so every Mac host gets it.
    static func macChord(_ key: UInt32, _ mods: UInt32) -> (UInt32, UInt32) {
        let shift = mods & TWIP_MOD_SHIFT
        let left = TWIP_KEY_ARROW_LEFT.rawValue, right = TWIP_KEY_ARROW_RIGHT.rawValue
        let up = TWIP_KEY_ARROW_UP.rawValue, down = TWIP_KEY_ARROW_DOWN.rawValue
        let horizontal = key == left || key == right, vertical = key == up || key == down
        switch mods & ~TWIP_MOD_SHIFT {
        case TWIP_MOD_ALT where horizontal || vertical || key == TWIP_KEY_BACKSPACE.rawValue || key == TWIP_KEY_DELETE.rawValue:
            return (key, TWIP_MOD_CTRL | shift) // Option+arrow moves by word, up and down by paragraph
        case TWIP_MOD_META where horizontal:
            return (key == left ? TWIP_KEY_HOME.rawValue : TWIP_KEY_END.rawValue, shift)
        case TWIP_MOD_META where vertical:
            return (key == up ? TWIP_KEY_HOME.rawValue : TWIP_KEY_END.rawValue, TWIP_MOD_CTRL | shift)
        default:
            return (key, mods)
        }
    }

    static let byCode: [Int: UInt32] = {
        var m: [Int: UInt32] = [:]
        let letters = [kVK_ANSI_A, kVK_ANSI_B, kVK_ANSI_C, kVK_ANSI_D, kVK_ANSI_E, kVK_ANSI_F, kVK_ANSI_G, kVK_ANSI_H,
                       kVK_ANSI_I, kVK_ANSI_J, kVK_ANSI_K, kVK_ANSI_L, kVK_ANSI_M, kVK_ANSI_N, kVK_ANSI_O, kVK_ANSI_P,
                       kVK_ANSI_Q, kVK_ANSI_R, kVK_ANSI_S, kVK_ANSI_T, kVK_ANSI_U, kVK_ANSI_V, kVK_ANSI_W, kVK_ANSI_X,
                       kVK_ANSI_Y, kVK_ANSI_Z]
        for (i, k) in letters.enumerated() { m[k] = TWIP_KEY_A.rawValue + UInt32(i) }
        let digits = [kVK_ANSI_0, kVK_ANSI_1, kVK_ANSI_2, kVK_ANSI_3, kVK_ANSI_4, kVK_ANSI_5, kVK_ANSI_6, kVK_ANSI_7, kVK_ANSI_8, kVK_ANSI_9]
        for (i, k) in digits.enumerated() { m[k] = TWIP_KEY_DIGIT0.rawValue + UInt32(i) }
        let fkeys = [kVK_F1, kVK_F2, kVK_F3, kVK_F4, kVK_F5, kVK_F6, kVK_F7, kVK_F8, kVK_F9, kVK_F10, kVK_F11, kVK_F12]
        for (i, k) in fkeys.enumerated() { m[k] = TWIP_KEY_F1.rawValue + UInt32(i) }
        let pad = [kVK_ANSI_Keypad0, kVK_ANSI_Keypad1, kVK_ANSI_Keypad2, kVK_ANSI_Keypad3, kVK_ANSI_Keypad4,
                   kVK_ANSI_Keypad5, kVK_ANSI_Keypad6, kVK_ANSI_Keypad7, kVK_ANSI_Keypad8, kVK_ANSI_Keypad9]
        for (i, k) in pad.enumerated() { m[k] = TWIP_KEY_NUMPAD0.rawValue + UInt32(i) }
        let named: [(Int, TwipKey)] = [
            (kVK_Return, TWIP_KEY_ENTER), (kVK_Tab, TWIP_KEY_TAB), (kVK_Delete, TWIP_KEY_BACKSPACE),
            (kVK_ForwardDelete, TWIP_KEY_DELETE), (kVK_Escape, TWIP_KEY_ESCAPE), (kVK_Space, TWIP_KEY_SPACE),
            (kVK_Help, TWIP_KEY_INSERT), (kVK_Home, TWIP_KEY_HOME), (kVK_End, TWIP_KEY_END),
            (kVK_PageUp, TWIP_KEY_PAGE_UP), (kVK_PageDown, TWIP_KEY_PAGE_DOWN), (kVK_LeftArrow, TWIP_KEY_ARROW_LEFT),
            (kVK_RightArrow, TWIP_KEY_ARROW_RIGHT), (kVK_UpArrow, TWIP_KEY_ARROW_UP), (kVK_DownArrow, TWIP_KEY_ARROW_DOWN),
            (kVK_ANSI_Minus, TWIP_KEY_MINUS), (kVK_ANSI_Equal, TWIP_KEY_EQUAL),
            (kVK_ANSI_LeftBracket, TWIP_KEY_BRACKET_LEFT), (kVK_ANSI_RightBracket, TWIP_KEY_BRACKET_RIGHT),
            (kVK_ANSI_Backslash, TWIP_KEY_BACKSLASH), (kVK_ANSI_Semicolon, TWIP_KEY_SEMICOLON),
            (kVK_ANSI_Quote, TWIP_KEY_QUOTE), (kVK_ANSI_Grave, TWIP_KEY_BACKQUOTE), (kVK_ANSI_Comma, TWIP_KEY_COMMA),
            (kVK_ANSI_Period, TWIP_KEY_PERIOD), (kVK_ANSI_Slash, TWIP_KEY_SLASH), (kVK_ISO_Section, TWIP_KEY_INTL_BACKSLASH),
            (kVK_ANSI_KeypadEnter, TWIP_KEY_NUMPAD_ENTER), (kVK_ANSI_KeypadPlus, TWIP_KEY_NUMPAD_ADD),
            (kVK_ANSI_KeypadMinus, TWIP_KEY_NUMPAD_SUBTRACT), (kVK_ANSI_KeypadMultiply, TWIP_KEY_NUMPAD_MULTIPLY),
            (kVK_ANSI_KeypadDivide, TWIP_KEY_NUMPAD_DIVIDE), (kVK_ANSI_KeypadDecimal, TWIP_KEY_NUMPAD_DECIMAL),
        ]
        for (k, v) in named { m[k] = v.rawValue }
        return m
    }()
}
