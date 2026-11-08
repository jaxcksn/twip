import CTwip
import Foundation

/// The one thread per process that owns the native engines (the C ABI's engine thread). Work runs in submission order;
/// callbacks an engine makes during a call are copied, then handed to their sessions after the call returns, so a
/// session never calls the engine from inside a callback. A work item completes after that hand-off, so state is
/// published before the caller resumes.
final class EngineThread: @unchecked Sendable {
    static let shared = EngineThread()

    private let condition = NSCondition()
    private var queue: [() -> Void] = [] // guarded by condition
    private var thread: Thread!
    /// The callbacks of the call in progress. Engine thread only.
    fileprivate var pending: [(engine: NativeEngine, session: UInt32, kind: UInt32, payload: Data)] = []

    private init() {
        thread = Thread { [unowned self] in run() }
        thread.name = "Twip engine"
        thread.qualityOfService = .userInitiated
        thread.stackSize = 16 << 20 // layout recurses through nested tables
        thread.start()
    }

    var isCurrent: Bool { Thread.current == thread }

    /// Runs `work` on the engine thread, in order, without waiting.
    func post(_ work: @escaping @Sendable () -> Void) {
        condition.lock()
        queue.append(work)
        condition.signal()
        condition.unlock()
    }

    /// Runs `work` on the engine thread, in order, and returns its result. A task cancelled before `work` starts
    /// throws `CancellationError` and `work` never runs; `work` itself may check `cancelled` between its steps.
    func run<T: Sendable>(_ work: @escaping @Sendable (_ cancelled: CancelFlag) throws -> T) async throws -> T {
        try Task.checkCancellation()
        let flag = CancelFlag()
        return try await withTaskCancellationHandler {
            try await withCheckedThrowingContinuation { (done: CheckedContinuation<T, any Error>) in
                post {
                    if flag.isSet { return done.resume(throwing: CancellationError()) }
                    let result = Result { try work(flag) }
                    self.deliver()
                    done.resume(with: result)
                }
            }
        } onCancel: {
            flag.set()
        }
    }

    private func run() {
        while true {
            condition.lock()
            while queue.isEmpty { condition.wait() }
            let work = queue.removeFirst()
            condition.unlock()
            work()
            deliver()
        }
    }

    /// Hands the callbacks of the last call to their sessions, in the order the engine made them.
    private func deliver() {
        while !pending.isEmpty {
            let batch = pending
            pending.removeAll()
            for c in batch { c.engine.deliver(session: c.session, kind: c.kind, payload: c.payload) }
        }
    }
}

/// Set once when the task waiting for a work item is cancelled.
final class CancelFlag: @unchecked Sendable {
    private let lock = NSLock()
    private var value = false
    var isSet: Bool { lock.withLock { value } }
    func set() { lock.withLock { value = true } }
}

/// Receives a session's engine callbacks, on the engine thread, after the call that made them.
protocol CallbackSink: AnyObject {
    func onCallback(kind: UInt32, payload: Data)
}

/// `{"ok": value}` or `{"error": {...}}`.
private struct Envelope<T: Decodable>: Decodable {
    let ok: T?
    let error: TwipError?
}

/// One native engine: a font library and the sessions that lay out with it. Engine thread only.
final class NativeEngine: @unchecked Sendable {
    private let fonts: String
    private var raw: OpaquePointer?
    private var sinks: [UInt32: any CallbackSink] = [:]
    /// Font requests (kind 5) go to the library, not to a session.
    var onFontRequest: ((Data) -> Void)?

    init(fonts: String) { self.fonts = fonts }

    /// The engine handle; creates the engine on first use.
    var handle: OpaquePointer {
        get throws {
            if let raw { return raw }
            let options = Data(#"{"fonts":"\#(fonts)"}"#.utf8)
            let ctx = Unmanaged.passUnretained(self).toOpaque() // the engine lives for the process, as its library does
            raw = options.withUnsafeBytes { twip_engine_new($0.baseAddress?.assumingMemoryBound(to: UInt8.self), $0.count, twipCallback, ctx) }
            guard let raw else { throw TwipError(.internal, "the engine failed to start") }
            return raw
        }
    }

    func register(_ session: UInt32, _ sink: any CallbackSink) { sinks[session] = sink }
    func unregister(_ session: UInt32) { sinks[session] = nil }

    fileprivate func deliver(session: UInt32, kind: UInt32, payload: Data) {
        if kind == UInt32(TWIP_KIND_FONT_REQUEST) { onFontRequest?(payload) }
        else { sinks[session]?.onCallback(kind: kind, payload: payload) }
    }

    /// A structured call (`TWIP_*` method): the request as JSON, the response's `ok` value as `T`.
    func call<T: Decodable>(_ session: UInt32, _ method: TwipMethod, _ request: Data = Data(), as: T.Type = JSONValue.self) throws -> T? {
        let engine = try handle
        var out = TwipBuf()
        let status = request.withUnsafeBytes {
            twip_call(engine, session, method.rawValue, $0.baseAddress?.assumingMemoryBound(to: UInt8.self), $0.count, &out)
        }
        return try take(&out, status)
    }

    /// A structured call with an encodable request.
    func call<T: Decodable>(_ session: UInt32, _ method: TwipMethod, _ request: some Encodable, as: T.Type = JSONValue.self) throws -> T? {
        try call(session, method, try Wire.encode(request), as: T.self)
    }
}

/// Reads `{"ok": value}` or `{"error": ...}` from `buf` and frees it.
func take<T: Decodable>(_ buf: inout TwipBuf, _ status: Int32, as: T.Type = T.self) throws -> T? {
    defer { twip_buf_free(&buf) }
    guard let ptr = buf.ptr, buf.len > 0 else {
        if status != 0 { throw TwipError(ErrorCode(status: status) ?? .internal, "the engine failed (\(status))") }
        return nil
    }
    let envelope = try Wire.decode(Envelope<T>.self, from: Data(bytes: ptr, count: buf.len))
    if let error = envelope.error { throw error }
    return envelope.ok
}

/// Copies and frees a buffer of raw bytes.
func takeBytes(_ buf: inout TwipBuf) -> Data {
    defer { twip_buf_free(&buf) }
    guard let ptr = buf.ptr else { return Data() }
    return Data(bytes: ptr, count: buf.len)
}

/// Calls `body` with a pointer to `data`'s bytes (null when empty) and their count.
func withBytes<R>(_ data: Data, _ body: (UnsafePointer<UInt8>?, Int) throws -> R) rethrows -> R {
    try data.withUnsafeBytes { try body($0.baseAddress?.assumingMemoryBound(to: UInt8.self), $0.count) }
}

/// The engine's callback: copies the payload (valid only during the call) for delivery after the call.
private let twipCallback: twip_callback = { ctx, session, kind, ptr, len in
    guard let ctx else { return }
    let engine = Unmanaged<NativeEngine>.fromOpaque(ctx).takeUnretainedValue()
    let payload = ptr.map { Data(bytes: $0, count: len) } ?? Data()
    EngineThread.shared.pending.append((engine, session, kind, payload))
}
