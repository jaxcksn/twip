import Foundation

// Revisions, comments and people, as applications read them, and where comment cards go beside the pages
// (crates/twip/src/api/review.rs).
//
// Renamed from Rust: `Body` → `TextBody`.

/// The markup view: how revisions show. A view setting, never saved.
public enum MarkupView: String, Hashable, Sendable, Codable, CaseIterable {
    /// Insertions and deletions shown in place, in their author's colour, with change bars (the default).
    case all
    /// The changes made, with change bars.
    case simple
    /// The changes made, with no markup.
    case noMarkup
    /// The document before the changes.
    case original
}

/// The kinds of revision.
public enum RevisionKind: String, Hashable, Sendable, Codable, CaseIterable {
    /// Inserted text.
    case insertion
    /// Deleted text.
    case deletion
    /// The origin of moved text.
    case moveFrom
    /// The destination of moved text.
    case moveTo
    /// Changed character formatting.
    case formatting
    /// Changed paragraph properties.
    case paragraphProperties
    /// Changed section properties.
    case sectionProperties
    /// Changed table properties.
    case tableProperties
    /// An inserted table row.
    case rowInsertion
    /// A deleted table row.
    case rowDeletion
    /// An inserted table cell.
    case cellInsertion
    /// A deleted table cell.
    case cellDeletion
    /// Merged table cells.
    case cellMerge
}

/// One revision, for a reviewing pane.
public struct RevisionInfo: Hashable, Sendable, Codable, Identifiable {
    /// The revision.
    public var id: RevisionID
    /// What changed.
    public var kind: RevisionKind
    /// Who made it.
    public var author: Person
    /// When; nil when the document gives no readable date.
    public var time: Timestamp?
    /// The inserted, deleted or moved text, when there is some.
    public var text: String
    /// What changed, for formatting changes (`Formatted: Bold, Italic`).
    public var description: String
    /// The page it starts on.
    public var page: Int
    /// The story it is in.
    public var story: Story
    /// A move's other half.
    public var pair: RevisionID?

    /// A revision.
    public init(
        id: RevisionID, kind: RevisionKind, author: Person, time: Timestamp? = nil, text: String = "",
        description: String = "", page: Int, story: Story, pair: RevisionID? = nil
    ) {
        self.id = id
        self.kind = kind
        self.author = author
        self.time = time
        self.text = text
        self.description = description
        self.page = page
        self.story = story
        self.pair = pair
    }
}

/// A comment thread: the first comment and its replies.
public struct CommentThread: Hashable, Sendable, Codable, Identifiable {
    /// The first comment's id.
    public var id: CommentID
    /// Whether the thread is resolved.
    public var done: Bool
    /// The commented text.
    public var anchorText: String
    /// The page it starts on.
    public var page: Int
    /// The first comment, then its replies.
    public var comments: [Comment]

    /// A comment thread.
    public init(id: CommentID, done: Bool = false, anchorText: String, page: Int, comments: [Comment]) {
        self.id = id
        self.done = done
        self.anchorText = anchorText
        self.page = page
        self.comments = comments
    }
}

/// One comment.
public struct Comment: Hashable, Sendable, Codable, Identifiable {
    /// The comment.
    public var id: CommentID
    /// Who wrote it.
    public var author: Person
    /// When; nil when the document gives no readable date.
    public var time: Timestamp?
    /// The body.
    public var body: RichText
    /// The body as plain text.
    public var plainText: String

    /// A comment.
    public init(id: CommentID, author: Person, time: Timestamp? = nil, body: RichText, plainText: String) {
        self.id = id
        self.author = author
        self.time = time
        self.body = body
        self.plainText = plainText
    }
}

/// Someone who made revisions or comments, with the colour they show in.
public struct Person: Hashable, Sendable, Codable {
    /// The display name.
    public var name: String
    /// The initials.
    public var initials: String?
    /// The identity in a directory.
    public var identity: Identity?
    /// The colour the editor marks their changes with.
    public var color: RGBColor

    /// A person.
    public init(name: String, initials: String? = nil, identity: Identity? = nil, color: RGBColor) {
        self.name = name
        self.initials = initials
        self.identity = identity
        self.color = color
    }
}

/// Formatted text: paragraphs of runs (comment bodies, field results).
public struct RichText: Hashable, Sendable, Codable {
    /// The paragraphs.
    public var paragraphs: [RichParagraph]

    /// Formatted text.
    public init(paragraphs: [RichParagraph] = []) { self.paragraphs = paragraphs }
}

/// One paragraph of `RichText`.
public struct RichParagraph: Hashable, Sendable, Codable {
    /// The runs.
    public var runs: [RichRun]

    /// A paragraph.
    public init(runs: [RichRun] = []) { self.runs = runs }
}

/// A run of `RichText`: text with its formatting.
public struct RichRun: Hashable, Sendable, Codable {
    /// The text.
    public var text: String
    /// Bold.
    public var bold: Bool?
    /// Italic.
    public var italic: Bool?
    /// Underlined.
    public var underline: Bool?
    /// Struck through.
    public var strikethrough: Bool?
    /// A link's address.
    public var link: String?

    /// A run.
    public init(
        text: String, bold: Bool? = nil, italic: Bool? = nil, underline: Bool? = nil, strikethrough: Bool? = nil,
        link: String? = nil
    ) {
        self.text = text
        self.bold = bold
        self.italic = italic
        self.underline = underline
        self.strikethrough = strikethrough
        self.link = link
    }
}

/// Plain or formatted text, where a command takes either. On the wire: a string, or a `RichText` object.
public enum TextBody: Hashable, Sendable, Codable, ExpressibleByStringLiteral {
    /// Plain text.
    case text(String)
    /// Formatted text.
    case rich(RichText)

    /// Plain text from a literal.
    public init(stringLiteral value: String) { self = .text(value) }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.singleValueContainer()
        if let s = try? c.decode(String.self) { self = .text(s) }
        else { self = .rich(try c.decode(RichText.self)) }
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.singleValueContainer()
        switch self {
        case .text(let s): try c.encode(s)
        case .rich(let r): try c.encode(r)
        }
    }
}

/// Where a visible comment's range is, for placing its card (content coordinates, CSS px).
public struct CommentAnchor: Hashable, Sendable, Codable {
    /// The thread.
    public var thread: CommentID
    /// The page.
    public var page: Int
    /// Whether it is the active thread.
    public var active: Bool
    /// Whether the thread is resolved.
    public var done: Bool
    /// The range's first character, at its baseline.
    public var start: ContentPoint
    /// The range's last character, at its baseline.
    public var end: ContentPoint
    /// The first line's top, for a card level with it.
    public var lineTop: Double
    /// The first line's bottom.
    public var lineBottom: Double
    /// The x where a leader line leaves the page.
    public var pageEdge: Double

    /// A comment anchor.
    public init(
        thread: CommentID, page: Int, active: Bool = false, done: Bool = false, start: ContentPoint,
        end: ContentPoint, lineTop: Double, lineBottom: Double, pageEdge: Double
    ) {
        self.thread = thread
        self.page = page
        self.active = active
        self.done = done
        self.start = start
        self.end = end
        self.lineTop = lineTop
        self.lineBottom = lineBottom
        self.pageEdge = pageEdge
    }
}

/// Where the SDK placed an attached card, and its leader line (a polyline).
public struct BalloonPlacement: Hashable, Sendable, Codable {
    /// The thread.
    public var thread: CommentID
    /// The card's top.
    public var top: Double
    /// The card's left.
    public var left: Double
    /// The leader line, a polyline from the range to the card.
    public var leader: [ContentPoint]

    /// A balloon placement.
    public init(thread: CommentID, top: Double, left: Double, leader: [ContentPoint] = []) {
        self.thread = thread
        self.top = top
        self.left = left
        self.leader = leader
    }
}

/// Card tops that don't overlap, `gap` apart: card `i` (`heights[i]` tall) is for `anchors[i]`, in the order given.
/// The first active card sits level with its line (`lineTop`); the cards before it rise above it only as far as they
/// must, and the cards after it fall below it. No card goes above 0: when the cards before the active one don't fit
/// above it, they start at 0 and push it down. With no active card, each card is level with its line or just below
/// the card before it. The engine's `stack_balloons`, checked by `tests/wire/balloons.json`.
public func stackBalloons(_ anchors: [CommentAnchor], heights: [Double], gap: Double) -> [Double] {
    let n = min(anchors.count, heights.count)
    var tops = anchors.prefix(n).map(\.lineTop)
    let active = anchors.prefix(n).firstIndex(where: \.active) ?? 0
    for i in stride(from: active - 1, through: 0, by: -1) {
        tops[i] = min(tops[i], tops[i + 1] - gap - heights[i])
    }
    var bottom = -Double.infinity
    for i in 0..<n {
        tops[i] = max(tops[i], 0, bottom + gap)
        bottom = tops[i] + heights[i]
    }
    return tops
}
