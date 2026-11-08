import Foundation

/// Why an operation failed. Each has a stable name on the wire (`invalid-argument`).
public enum ErrorCode: String, Hashable, Sendable, Codable, CaseIterable {
    /// The session was closed.
    case destroyed
    /// An argument is out of its range, or names something that doesn't exist.
    case invalidArgument = "invalid-argument"
    /// The operation can't run here now (nothing to undo, no table at the selection).
    case unavailable
    /// The operation changes the document, and the session is read only.
    case readOnly = "read-only"
    /// The bytes aren't a document the engine reads. The open document stays.
    case importFailed = "import-failed"
    /// The document couldn't be written in the requested format.
    case exportFailed = "export-failed"
    /// The operation was cancelled.
    case aborted
    /// The platform refused.
    case notAllowed = "not-allowed"
    /// The engine doesn't do this yet.
    case unsupported
    /// A fault in the engine. The session rejects later operations.
    case `internal`

    /// The code for a C ABI status (a negated `TWIP_E_*`), in the api's order.
    init?(status: Int32) {
        let all = Self.allCases
        guard status < 0, Int(-status) <= all.count else { return nil }
        self = all[Int(-status) - 1]
    }
}

/// A failed operation: its code, a message for developers, and details where the code needs them.
///
/// Cancelling the task an operation runs in throws `CancellationError` instead.
public struct TwipError: Error, Hashable, Sendable, Codable, CustomStringConvertible {
    /// Why it failed.
    public var code: ErrorCode
    /// A readable message for developers, not for end users.
    public var message: String
    /// More about the failure, where the code needs it (the argument that was invalid).
    public var details: JSONValue?

    /// An error with a code and a message.
    public init(_ code: ErrorCode, _ message: String, details: JSONValue? = nil) {
        self.code = code
        self.message = message
        self.details = details
    }

    public var description: String { "\(code.rawValue): \(message)" }

    static func destroyed() -> TwipError { TwipError(.destroyed, "the session was closed") }
}
