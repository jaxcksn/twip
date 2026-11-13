import CTwip
import Foundation
import os

/// One document being edited: its selection, undo history, state and events.
///
/// A new session holds a blank document. Operations are `async` and may be called from any task or actor; they run in
/// the order they're called, one at a time, on the engine's own thread. Each returns once its change has committed,
/// and ``state`` is then at its revision or later. Cancelling the calling task cancels an operation that hasn't started
/// (and an open while it reads), which then throws `CancellationError`; expected failures throw ``TwipError``.
///
/// ```swift
/// let session = EditorSession(options: SessionOptions(author: Author(name: "Ada Lovelace")))
/// try await session.open(contentsOf: url)
/// try await session.run(.selectAll)
/// try await session.run(.setBold(true))
/// print(session.state.formatting.bold.isOn) // true
/// try await session.save(to: url)
/// ```
///
/// A session is headless until a view presents it (`TwipUI`'s `DocumentEditor`).
public final class EditorSession: @unchecked Sendable {
    /// The session's font library.
    public let fonts: FontLibrary

    private let options: SessionOptions
    private let shared = OSAllocatedUnfairLock(initialState: Shared())
    // Engine thread only:
    private var id: UInt32 = 0
    private var sink: Sink?

    private struct Shared {
        var state: EditorState?
        var closed = false
        var subscribers: [UUID: Subscriber] = [:]
        var host: (any SessionHost)?
    }

    /// What a subscriber hears, on the engine thread, in order.
    enum Note {
        case state(EditorState)
        case event(EditorEvent, EditorState)
        case closed
    }

    typealias Subscriber = @Sendable (Note) -> Void

    /// Creates a session over a blank document. The engine starts with the first operation.
    /// - Parameters:
    ///   - options: Who edits, whether it starts read only, the language and dictionaries.
    ///   - fonts: The fonts it lays out with; ``FontLibrary/system`` by default.
    public init(options: SessionOptions = SessionOptions(), fonts: FontLibrary = .system) {
        var options = options
        if options.author.name.isEmpty { options.author.name = NSFullUserName() }
        self.options = options
        self.fonts = fonts
        // The native session starts now, behind any work already queued; operations queue after it.
        EngineThread.shared.post { [weak self] in _ = try? self?.ensureSession() }
    }

    deinit {
        let id = id, engine = fonts.native
        guard id != 0 else { return }
        EngineThread.shared.post {
            engine.unregister(id)
            if let raw = try? engine.handle { twip_session_free(raw, id) }
        }
    }

    // MARK: Documents

    /// Opens a DOCX document, replacing the open one only when it reads successfully.
    /// - Returns: The import's fidelity warnings.
    @discardableResult
    public func open(_ document: Data) async throws -> [Warning] {
        // The engine reads into a held replacement, then swaps it in as a second call: a cancel during the read drops
        // the replacement, so the open document is never partly replaced.
        try await enqueue { id, engine, cancelled in
            var out = TwipBuf()
            let raw = try engine.handle
            let status = withBytes(document) { p, n in withBytes(Data("{}".utf8)) { twip_open_prepare(raw, id, p, n, $0, $1, &out) } }
            let token = try take(&out, status, as: Int.self) ?? 0
            if cancelled.isSet {
                _ = try engine.call(id, TWIP_OPEN_DISCARD, token)
                throw CancellationError()
            }
            return try engine.call(id, TWIP_OPEN_COMMIT, token, as: [Warning].self) ?? []
        }
    }

    /// Opens a DOCX file, replacing the open document only when it reads successfully.
    /// - Returns: The import's fidelity warnings.
    @discardableResult
    public func open(contentsOf url: URL) async throws -> [Warning] {
        try await open(try Data(contentsOf: url))
    }

    /// Replaces the document with a blank one.
    public func newDocument() async throws {
        try await call(TWIP_NEW_DOCUMENT)
    }

    /// Writes the document as DOCX, or as PDF with its options.
    /// - Returns: The file, the revision it holds and the export's fidelity warnings.
    public func export(_ options: ExportOptions = .docx) async throws -> Export {
        struct Meta: Decodable { let revision: Int; let warnings: [Warning] }
        return try await enqueue { id, engine, _ in
            var bytes = TwipBuf(), meta = TwipBuf()
            let raw = try engine.handle
            let status = withBytes(try Wire.encode(options)) { twip_export(raw, id, $0, $1, &bytes, &meta) }
            let data = takeBytes(&bytes)
            guard let info = try take(&meta, status, as: Meta.self) else { throw TwipError(.internal, "the export has no revision") }
            return Export(bytes: data, revision: info.revision, warnings: info.warnings)
        }
    }

    /// Saves the document as DOCX. The file is replaced atomically: a failed save leaves the old one.
    /// - Returns: The revision saved and the export's fidelity warnings.
    @discardableResult
    public func save(to url: URL) async throws -> SaveResult {
        try await write(.docx, to: url)
    }

    /// Exports the document as PDF, laid out completely first. The file is replaced atomically.
    /// - Parameter options: An open password and keywords; none by default.
    /// - Returns: The revision exported and the export's fidelity warnings.
    @discardableResult
    public func exportPDF(to url: URL, options: PDFOptions = PDFOptions()) async throws -> SaveResult {
        try await write(.pdf(options), to: url)
    }

    private func write(_ options: ExportOptions, to url: URL) async throws -> SaveResult {
        let export = try await export(options)
        try export.bytes.write(to: url, options: .atomic)
        return SaveResult(revision: export.revision, warnings: export.warnings)
    }

    // MARK: Editing

    /// Runs one command as one transaction (one undo step), on the selection.
    ///
    /// ```swift
    /// try await session.run(.insertText("Hello"))
    /// try await session.run(.moveSelection(by: .word, direction: .backward, extend: true))
    /// ```
    /// - Returns: The commit: its transaction record, and what the command made (a comment's id, a field's id).
    @discardableResult
    public func run(_ command: Command) async throws -> Commit {
        try await call(TWIP_RUN, command, as: Commit.self) ?? Commit()
    }

    /// Runs commands atomically, as one transaction and one undo step: all of them, or none when one fails.
    /// - Parameter label: The undo label; the first command's by default.
    @discardableResult
    public func batch(_ commands: [Command], label: String? = nil) async throws -> Commit {
        struct Batch: Encodable { let commands: [Command]; let label: String? }
        return try await call(TWIP_BATCH, Batch(commands: commands, label: label), as: Commit.self) ?? Commit()
    }

    // MARK: State and events

    /// The latest state. After an operation returns, it is at that operation's revision or later.
    ///
    /// Read before the session has started (just after `init`, while the engine starts), it waits for the start.
    public var state: EditorState {
        if let state = shared.withLock({ $0.state }) { return state }
        let started = DispatchSemaphore(value: 0)
        EngineThread.shared.post { [self] in
            _ = try? ensureSession()
            started.signal()
        }
        if !EngineThread.shared.isCurrent { started.wait() }
        guard let state = shared.withLock({ $0.state }) else { preconditionFailure("the engine could not start a session") }
        return state
    }

    /// The state after each change, starting with the current one. Coalesced: a slow consumer sees only the latest.
    /// Ends when the session closes.
    public var states: AsyncStream<EditorState> {
        AsyncStream(bufferingPolicy: .bufferingNewest(1)) { c in
            c.yield(state)
            let token = subscribe { note in
                switch note {
                case .state(let s): c.yield(s)
                case .event: break
                case .closed: c.finish()
                }
            }
            c.onTermination = { [weak self] _ in self?.unsubscribe(token) }
        }
    }

    /// The session's events, in commit order, each with the state at its change. Every `documentChanged` comes with
    /// the state at its revision. Ends when the session closes.
    ///
    /// ```swift
    /// for await (event, state) in session.events {
    ///     if case .documentChanged(let record) = event { print(record.revision, state.pages.count) }
    /// }
    /// ```
    public var events: AsyncStream<(event: EditorEvent, state: EditorState)> {
        AsyncStream { c in
            let token = subscribe { note in
                switch note {
                case .event(let e, let s): c.yield((e, s))
                case .state: break
                case .closed: c.finish()
                }
            }
            c.onTermination = { [weak self] _ in self?.unsubscribe(token) }
        }
    }

    /// Hears every state and event on the engine thread as it is delivered. The handler must not block.
    func subscribe(_ subscriber: @escaping Subscriber) -> UUID {
        let token = UUID()
        let closed = shared.withLock {
            if !$0.closed { $0.subscribers[token] = subscriber }
            return $0.closed
        }
        if closed { subscriber(.closed) }
        return token
    }

    func unsubscribe(_ token: UUID) {
        _ = shared.withLock { $0.subscribers.removeValue(forKey: token) }
    }

    /// Every command with its label, group, shortcuts and undo grouping, for menus and command palettes.
    public var catalog: [CommandInfo] {
        get async throws { try await call(TWIP_CATALOG, as: [CommandInfo].self) ?? [] }
    }

    /// Word's Insert > Shapes gallery: its groups and their presets, in Word's order.
    public var shapeGallery: [ShapeGalleryGroup] {
        get async throws { try await call(TWIP_SHAPE_GALLERY, as: [ShapeGalleryGroup].self) ?? [] }
    }

    /// Document statistics, computed on request.
    public func wordCount() async throws -> WordCount {
        guard let count = try await call(TWIP_WORD_COUNT, as: WordCount.self) else { throw TwipError(.internal, "no word count") }
        return count
    }

    // MARK: Settings

    /// Makes the session read only, or editable again.
    public func setReadOnly(_ readOnly: Bool) async throws {
        try await call(TWIP_SET_READ_ONLY, readOnly)
    }

    /// Switches between editing and template design.
    public func setMode(_ mode: Mode) async throws {
        try await call(TWIP_SET_MODE, mode)
    }

    /// Shows or hides the built-in rulers along the view's top and left edges (off by default); `state.view.rulers`
    /// says which show.
    public func setRulers(_ rulers: RulerVisibility) async throws {
        try await call(TWIP_HOST_SET_RULERS, rulers)
    }

    /// Sets the author of later edits, revisions and comments.
    public func setAuthor(_ author: Author) async throws {
        try await call(TWIP_SET_AUTHOR, author)
    }

    // MARK: Lifetime

    /// Ends the session. Event streams finish at once; later operations throw ``ErrorCode/destroyed``. Calling it
    /// again does nothing.
    public func close() async {
        let subscribers: [Subscriber]? = shared.withLock { s in
            if s.closed { return nil }
            s.closed = true
            let all = Array(s.subscribers.values)
            s.subscribers = [:]
            return all
        }
        guard let subscribers else { return await idle() }
        for s in subscribers { s(.closed) }
        let engine = fonts.native
        _ = try? await EngineThread.shared.run { [self] _ in
            guard id != 0 else { return }
            engine.unregister(id)
            twip_session_free(try engine.handle, id)
            id = 0
        }
    }

    /// Whether ``close()`` was called.
    public var isClosed: Bool { shared.withLock { $0.closed } }

    // MARK: Internals

    /// Returns after every operation queued so far has run and its events were delivered.
    func idle() async {
        _ = try? await EngineThread.shared.run { _ in }
    }

    /// The attached host, if any.
    var host: (any SessionHost)? { shared.withLock { $0.host } }

    func attach(_ host: any SessionHost) throws {
        try shared.withLock {
            guard $0.host == nil else { throw TwipError(.unavailable, "another view presents this session") }
            $0.host = host
        }
    }

    func detach(_ host: any SessionHost) {
        shared.withLock { if $0.host === host { $0.host = nil } }
    }

    /// Runs `work` on the engine thread with this session's id, in call order.
    func enqueue<T: Sendable>(_ work: @escaping @Sendable (_ id: UInt32, _ engine: NativeEngine, _ cancelled: CancelFlag) throws -> T) async throws -> T {
        guard !isClosed else { throw TwipError.destroyed() }
        return try await EngineThread.shared.run { [self] cancelled in
            guard !isClosed else { throw TwipError.destroyed() }
            return try work(try ensureSession(), fonts.native, cancelled)
        }
    }

    /// A structured call on this session.
    @discardableResult
    func call<T: Decodable & Sendable>(_ method: TwipMethod, _ request: some Encodable & Sendable, as: T.Type = JSONValue.self) async throws -> T? {
        let body = try Wire.encode(request)
        return try await enqueue { id, engine, _ in try engine.call(id, method, body, as: T.self) }
    }

    /// A structured call with no request.
    @discardableResult
    func call<T: Decodable & Sendable>(_ method: TwipMethod, as: T.Type = JSONValue.self) async throws -> T? {
        try await enqueue { id, engine, _ in try engine.call(id, method, Data(), as: T.self) }
    }

    /// The session's id, creating the native session on first use. Engine thread only.
    func ensureSession() throws -> UInt32 {
        if id != 0 { return id }
        let engine = fonts.native
        let raw = try engine.handle
        let new = withBytes(try Wire.encode(options)) { twip_session_new(raw, $0, $1) }
        guard new != 0 else { throw TwipError(.internal, "the engine could not create a session") }
        id = new
        let sink = Sink(self)
        self.sink = sink
        engine.register(new, sink)
        if let state = try engine.call(new, TWIP_STATE, Data(), as: EditorState.self) {
            shared.withLock { $0.state = state }
        }
        return new
    }

    /// The native session and engine for host calls (input, frames), or nil while there is none. Engine thread only.
    var native: (id: UInt32, engine: OpaquePointer)? {
        guard !isClosed, let id = try? ensureSession(), let raw = try? fonts.native.handle else { return nil }
        return (id, raw)
    }

    fileprivate func onCallback(kind: UInt32, payload: Data) {
        guard !isClosed else { return }
        switch Int32(kind) {
        case TWIP_KIND_STATE:
            guard let state = try? Wire.decode(EditorState.self, from: payload) else { return }
            let subscribers = shared.withLock { s in
                s.state = state
                return Array(s.subscribers.values)
            }
            for s in subscribers { s(Note.state(state)) }
        case TWIP_KIND_EVENT:
            guard let event = try? Wire.decode(EditorEvent.self, from: payload) else { return }
            let (state, subscribers, host) = shared.withLock { ($0.state, Array($0.subscribers.values), $0.host) }
            host?.onEvent(event)
            guard let state else { return }
            for s in subscribers { s(Note.event(event, state)) }
        case TWIP_KIND_SET_TIMER:
            struct Timer: Decodable { let timer: UInt32; let ms: Int }
            guard let timer = try? Wire.decode(Timer.self, from: payload) else { return }
            let session = UnsafeSendable(Weak(self))
            DispatchQueue.global().asyncAfter(deadline: .now() + .milliseconds(timer.ms)) {
                EngineThread.shared.post {
                    guard let self = session.value.value, !self.isClosed, self.id != 0, let raw = try? self.fonts.native.handle else { return }
                    twip_timer_fired(raw, self.id, timer.timer)
                }
            }
        case TWIP_KIND_REQUEST_FRAME:
            host?.onFrameRequested()
        case TWIP_KIND_HOST_REQUEST:
            host?.onHostRequest(payload)
        default:
            break
        }
    }

    /// Holds the session weakly for the engine's callbacks.
    private struct Weak { weak var value: EditorSession?; init(_ v: EditorSession) { value = v } }

    private final class Sink: CallbackSink {
        weak var session: EditorSession?
        init(_ session: EditorSession) { self.session = session }
        func onCallback(kind: UInt32, payload: Data) { session?.onCallback(kind: kind, payload: payload) }
    }
}

/// What a save reports: the revision written and the export's fidelity warnings.
public struct SaveResult: Hashable, Sendable {
    /// The state revision the file holds.
    public var revision: Int
    /// Fidelity notes from writing.
    public var warnings: [Warning]
}

/// A presentation of a session (a view), told about frames, host requests and events on the engine thread.
protocol SessionHost: AnyObject, Sendable {
    func onFrameRequested()
    func onHostRequest(_ payload: Data)
    func onEvent(_ event: EditorEvent)
}
