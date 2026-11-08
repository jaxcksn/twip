import Foundation

// Transaction records: who changed what, when, and with which undo label (crates/twip/src/api/record.rs).

/// One committed transaction, as `documentChanged` reports it.
public struct TransactionRecord: Hashable, Sendable, Codable, Identifiable {
    /// Unique in the session, increasing.
    public var id: Int
    /// The document revision this transaction made.
    public var revision: Int
    /// Who started it.
    public var origin: Origin
    /// The session's author at the time.
    public var author: Author
    /// When it committed.
    public var time: Timestamp
    /// The undo label (`Typing`, `Bold`, a batch's label).
    public var label: String
    /// What it touched.
    public var summary: ChangeSummary

    /// A transaction record.
    public init(
        id: Int, revision: Int, origin: Origin, author: Author, time: Timestamp, label: String, summary: ChangeSummary
    ) {
        self.id = id
        self.revision = revision
        self.origin = origin
        self.author = author
        self.time = time
        self.label = label
        self.summary = summary
    }
}

/// What started a transaction.
public enum Origin: String, Hashable, Sendable, Codable, CaseIterable {
    /// Input: keys, text, pointer, clipboard.
    case user
    /// A command or batch the application ran.
    case api
    /// An undo.
    case undo
    /// A redo.
    case redo
}

/// Who edits: the name revisions and comments are recorded under.
public struct Author: Hashable, Sendable, Codable {
    /// The display name.
    public var name: String
    /// The initials shown on comments.
    public var initials: String?
    /// The identity in a directory, written to the document with the author.
    public var identity: Identity?

    /// An author; the engine's default is named `Author`.
    public init(name: String = "Author", initials: String? = nil, identity: Identity? = nil) {
        self.name = name
        self.initials = initials
        self.identity = identity
    }
}

/// Who a person is in an identity provider (`w15:presenceInfo`).
public struct Identity: Hashable, Sendable, Codable {
    /// The identity provider (`AD`).
    public var provider: String
    /// The user's id with that provider.
    public var userID: String

    /// An identity.
    public init(provider: String, userID: String) {
        self.provider = provider
        self.userID = userID
    }

    enum CodingKeys: String, CodingKey {
        case provider
        case userID = "userId"
    }
}

/// A UTC time in whole seconds since 1970, ISO 8601 on the wire (`2026-10-27T09:30:00Z`).
public struct Timestamp: Hashable, Sendable, Codable, Comparable {
    /// Seconds since 1970, UTC.
    public var secondsSince1970: Int

    /// The time `secondsSince1970` seconds after 1970, UTC.
    public init(secondsSince1970: Int) { self.secondsSince1970 = secondsSince1970 }

    /// The time `date` is in, to the whole second.
    public init(_ date: Date) { secondsSince1970 = Int(date.timeIntervalSince1970.rounded(.down)) }

    /// The time as a `Date`.
    public var date: Date { Date(timeIntervalSince1970: TimeInterval(secondsSince1970)) }

    public static func < (a: Timestamp, b: Timestamp) -> Bool { a.secondsSince1970 < b.secondsSince1970 }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.singleValueContainer()
        let s = try c.decode(String.self)
        guard let date = try? Date.ISO8601FormatStyle().parse(s) else {
            throw DecodingError.dataCorruptedError(in: c, debugDescription: "`\(s)` is not an ISO 8601 time")
        }
        self.init(date)
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.singleValueContainer()
        try c.encode(date.formatted(Date.ISO8601FormatStyle()))
    }
}

/// What a transaction touched: how many paragraphs and tables, and which elements, revisions and comments.
public struct ChangeSummary: Hashable, Sendable, Codable {
    /// How many paragraphs it changed.
    public var paragraphs: Int
    /// How many tables it changed.
    public var tables: Int
    /// The elements it changed.
    public var elements: [ElementID]
    /// The revisions it made or changed.
    public var revisions: [RevisionID]
    /// The comments it made or changed.
    public var comments: [CommentID]

    /// A change summary.
    public init(
        paragraphs: Int = 0, tables: Int = 0, elements: [ElementID] = [], revisions: [RevisionID] = [],
        comments: [CommentID] = []
    ) {
        self.paragraphs = paragraphs
        self.tables = tables
        self.elements = elements
        self.revisions = revisions
        self.comments = comments
    }
}
