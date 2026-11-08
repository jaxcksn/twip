import CTwip
import CoreGraphics
import Foundation
import QuartzCore

/// What a view presenting a session hears, on the engine thread. Hop to the main actor before touching the view.
package protocol EditorHostDelegate: AnyObject, Sendable {
    /// The engine asked for something only the view can do: a cursor, the clipboard, the caret, a scroll.
    func host(_ host: EditorHost, didRequest request: HostRequest)
    /// A frame was drawn: on the GPU into the attached layer (`image` nil), or on the CPU into `image`.
    func host(_ host: EditorHost, didDraw frame: FrameInfo, image: CGImage?)
    /// An event of the presented session.
    func host(_ host: EditorHost, didReceive event: EditorEvent)
}

/// A presentation of a session: it forwards raw input, sizes the surface and draws frames. One presents a session at a
/// time; ``detach()`` ends it, and the session keeps its document, selection and history. Coordinates are the view's
/// points (the engine's CSS px), its backing scale the engine's device pixel ratio.
package final class EditorHost: SessionHost, @unchecked Sendable {
    package let session: EditorSession
    private weak var delegate: (any EditorHostDelegate)?
    // Engine thread only:
    private var size = CGSize.zero
    private var scale: CGFloat = 2
    private var layer: CAMetalLayer?
    private var framePending = false
    private var pixels: [UInt8] = []

    /// Presents `session`.
    /// - Throws: ``ErrorCode/unavailable`` when another view presents it.
    package init(session: EditorSession, delegate: any EditorHostDelegate) throws {
        self.session = session
        self.delegate = delegate
        try session.attach(self)
    }

    /// Stops presenting: the surface goes, and the session is headless again.
    package func detach() {
        session.detach(self)
        let session = session
        EngineThread.shared.post { [self] in
            guard let (id, raw) = session.native else { return }
            if layer != nil { twip_detach_window(raw, id) }
            twip_surface(raw, id, 0, 0, Double(scale))
            layer = nil
        }
    }

    // MARK: Surface and frames

    /// Draws with Metal into `layer` when the library has a GPU build and there's a device.
    /// - Returns: The GPU backend (4 for Metal), or nil when frames draw on the CPU.
    package func attach(_ layer: CAMetalLayer) -> Int32? {
        let layer = UnsafeSendable(layer)
        return sync { [self] id, raw in
            let backend = twip_attach_window(raw, id, Unmanaged.passUnretained(layer.value).toOpaque())
            guard backend > 0 else { return nil }
            self.layer = layer.value
            return backend
        } ?? nil
    }

    /// The view's size in points and its backing scale.
    package func resize(width: CGFloat, height: CGFloat, scale: CGFloat) {
        post { [self] id, raw in
            size = CGSize(width: width, height: height)
            self.scale = scale
            twip_surface(raw, id, Double(width), Double(height), Double(scale))
            requestFrame()
        }
    }

    /// Where the viewport's top-left is, in content points.
    package func scroll(to point: CGPoint) {
        post { id, raw in twip_scroll(raw, id, Double(point.x), Double(point.y)) }
    }

    /// Draws a frame now (after the work queued before it).
    package func redraw() {
        post { [self] _, _ in requestFrame() }
    }

    func onFrameRequested() { requestFrame() }

    /// Draws one frame after the work queued so far, however many asked. Engine thread only.
    private func requestFrame() {
        guard !framePending else { return }
        framePending = true
        EngineThread.shared.post { [self] in
            framePending = false
            draw()
        }
    }

    private func draw() {
        guard let (id, raw) = session.native else { return }
        let w = UInt32((size.width * scale).rounded()), h = UInt32((size.height * scale).rounded())
        guard w > 0, h > 0 else { return }
        var info = TwipFrameInfo()
        if layer != nil {
            // The engine sizes the layer's drawables.
            if twip_frame_window(raw, id, w, h, &info) == 0 {
                delegate?.host(self, didDraw: Self.frame(info), image: nil)
                return
            }
            // A lost device: the CPU draws until the view attaches again.
            // ponytail: no reattach; WinForms retries after 1, 5 and 30 seconds.
            twip_detach_window(raw, id)
            self.layer = nil
        }
        let count = Int(w) * Int(h) * 4
        if pixels.count != count { pixels = [UInt8](repeating: 0, count: count) }
        guard twip_frame_rgba(raw, id, &pixels, w, h, &info) == 0 else { return }
        let image = pixels.withUnsafeMutableBytes { buf in
            CGContext(data: buf.baseAddress, width: Int(w), height: Int(h), bitsPerComponent: 8, bytesPerRow: Int(w) * 4,
                      space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)?.makeImage()
        }
        delegate?.host(self, didDraw: Self.frame(info), image: image)
    }

    private static func frame(_ i: TwipFrameInfo) -> FrameInfo {
        FrameInfo(contentWidth: i.content_width, contentHeight: i.content_height, caretX: i.caret_x, caretY: i.caret_y,
                  caretHeight: i.caret_height, caretVisible: i.caret_visible != 0, pages: Int(i.pages))
    }

    // MARK: Input

    /// A key press, by its position (`TWIP_KEY_*`) and modifiers (`TWIP_MOD_*`). Waits for the engine, so the view
    /// knows whether to pass the key on.
    package func key(_ key: UInt32, modifiers: UInt32) -> KeyOutcome {
        let raw = sync { id, raw in twip_key(raw, id, key, modifiers) } ?? 0
        switch raw {
        case 0: return .unhandled
        case 1: return .handled
        default:
            let all = HostCommand.allCases
            let n = Int(raw) - 2
            return n < all.count ? .hostCommand(all[n]) : .unhandled
        }
    }

    /// Typed text.
    package func text(_ text: String) {
        let data = Data(text.utf8)
        post { id, raw in withBytes(data) { twip_text(raw, id, $0, $1) } }
    }

    /// An input method's composition: start 0, update 1, end 2 (the end commits its text).
    package func composition(_ phase: UInt32, _ text: String) {
        let data = Data(text.utf8)
        post { id, raw in withBytes(data) { twip_composition(raw, id, phase, $0, $1) } }
    }

    /// A pointer event: kind down 0, move 1, up 2, cancel 3, leave 4; button primary 0, secondary 1, middle 2.
    package func pointer(_ kind: UInt32, x: CGFloat, y: CGFloat, button: UInt32, clicks: Int, modifiers: UInt32) {
        post { id, raw in twip_pointer(raw, id, kind, Double(x), Double(y), button, UInt32(max(0, clicks)), modifiers) }
    }

    /// A wheel or trackpad scroll the engine handles (a zoom with Cmd held).
    package func wheel(dx: CGFloat, dy: CGFloat, modifiers: UInt32) {
        post { id, raw in twip_wheel(raw, id, Double(dx), Double(dy), modifiers) }
    }

    /// The modifier keys changed with no other input.
    package func modifiers(_ modifiers: UInt32) {
        post { id, raw in twip_modifiers(raw, id, modifiers) }
    }

    /// The view gained or lost the keyboard focus.
    package func focus(_ focused: Bool) {
        post { id, raw in twip_focus(raw, id, focused ? 1 : 0) }
    }

    /// Returns after the input sent so far has been handled and its events delivered.
    package func flush() async { await session.idle() }

    // MARK: Callbacks

    func onHostRequest(_ payload: Data) {
        guard let request = try? Wire.decode(HostRequest.self, from: payload) else { return }
        delegate?.host(self, didRequest: request)
    }

    func onEvent(_ event: EditorEvent) {
        delegate?.host(self, didReceive: event)
    }

    // MARK: Engine thread

    private func post(_ work: @escaping @Sendable (UInt32, OpaquePointer) -> Void) {
        let session = session
        EngineThread.shared.post {
            guard let (id, raw) = session.native else { return }
            work(id, raw)
        }
    }

    /// Runs `work` on the engine thread and waits for it; nil when the session is closed.
    private func sync<T>(_ work: @escaping @Sendable (UInt32, OpaquePointer) -> T) -> T? {
        let session = session
        if EngineThread.shared.isCurrent { return session.native.map { work($0.id, $0.engine) } }
        let done = DispatchSemaphore(value: 0)
        let result = UnsafeBox<T?>(nil)
        EngineThread.shared.post {
            if let (id, raw) = session.native { result.value = work(id, raw) }
            done.signal()
        }
        done.wait()
        return result.value
    }
}

/// A mutable box handed across a semaphore.
private final class UnsafeBox<T>: @unchecked Sendable {
    var value: T
    init(_ value: T) { self.value = value }
}
