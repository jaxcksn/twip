import Foundation

// The accessibility tree, the caret window and announcements, from crates/twip/src/api/services.rs.
// Renames: AccessibleId → AccessibleID.

/// An accessibility tree node's identity. It stays the same across edits while the thing it names exists: a
/// paragraph, table, note, comment or story keeps its id when other content changes. A number on the wire.
public struct AccessibleID: RawRepresentable, Hashable, Sendable, Codable, Comparable, ExpressibleByIntegerLiteral {
    /// The number.
    public var rawValue: Int

    /// The id `rawValue`.
    public init(rawValue: Int) { self.rawValue = rawValue }

    /// The id `value`.
    public init(integerLiteral value: Int) { rawValue = value }

    public init(from decoder: any Decoder) throws {
        rawValue = try decoder.singleValueContainer().decode(Int.self)
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.singleValueContainer()
        try c.encode(rawValue)
    }

    public static func < (a: AccessibleID, b: AccessibleID) -> Bool { a.rawValue < b.rawValue }
}

/// An accessibility tree node's role.
public enum AccessibleRole: String, Hashable, Sendable, Codable, CaseIterable {
    /// The document.
    case document
    /// A paragraph.
    case paragraph
    /// A heading (its `level`, 1 to 9).
    case heading
    /// A list.
    case list
    /// A list item (its `level` and `position`, and a numbered item's `label`).
    case listItem
    /// A table (its nesting `level`).
    case table
    /// A table row.
    case row
    /// A table cell (its `cell` place).
    case cell
    /// A cell of a header row.
    case columnHeader
    /// A cell that heads its row.
    case rowHeader
    /// A picture or shape, named by its alternative text.
    case image
    /// A hyperlink.
    case link
    /// A text box or a shape's text.
    case textBox
    /// A footnote or endnote.
    case note
    /// A footnote or endnote reference (its `target` is the note).
    case noteRef
    /// A comment (its `author`), with its replies as children.
    case comment
    /// A header.
    case header
    /// A footer.
    case footer
    /// A named group after the body: headers and footers, footnotes, endnotes, comments.
    case region
    /// A run of text with the same marks, author, language and comment.
    case text
}

/// What a text node is marked with.
public enum AccessibleMark: String, Hashable, Sendable, Codable, CaseIterable {
    /// A tracked insertion.
    case insertion
    /// A tracked deletion.
    case deletion
    /// A misspelling.
    case misspelling
}

/// Where a cell is in its table's grid: its row and column (from 1) and how many it spans.
public struct CellPlace: Hashable, Sendable, Codable {
    /// Its row, from 1.
    public var row: Int
    /// Its first grid column, from 1.
    public var col: Int
    /// How many rows it spans.
    public var rowSpan: Int
    /// How many grid columns it spans.
    public var colSpan: Int

    /// A cell place.
    public init(row: Int, col: Int, rowSpan: Int = 1, colSpan: Int = 1) {
        self.row = row
        self.col = col
        self.rowSpan = rowSpan
        self.colSpan = colSpan
    }
}

/// Where a list item is in its list: its index (from 1) among the items at its level, and how many there are.
public struct ListPosition: Hashable, Sendable, Codable {
    /// Its index, from 1.
    public var index: Int
    /// The number of items.
    public var count: Int

    /// A list position.
    public init(index: Int, count: Int) {
        self.index = index
        self.count = count
    }
}

/// The document's accessibility tree.
public struct AccessibleNode: Hashable, Sendable, Codable, Identifiable {
    /// Its identity.
    public var id: AccessibleID
    /// Its role.
    public var role: AccessibleRole
    /// Its accessible name.
    public var name: String?
    /// Its text.
    public var text: String?
    /// A heading's level, a list item's nesting level, or a table's nesting level, from 1.
    public var level: Int?
    /// A numbered list item's number as shown (`3.`, `a)`).
    public var label: String?
    /// A list item's position in its list.
    public var position: ListPosition?
    /// A cell's place in its table.
    public var cell: CellPlace?
    /// Review and spelling marks on a text node.
    @OmitEmpty public var marks: [AccessibleMark]
    /// Whose marks or comment it is.
    public var author: String?
    /// Its language (BCP 47), where it differs from its parent's.
    public var lang: String?
    /// The comment the text is in the range of.
    public var details: AccessibleID?
    /// The note a note reference points to; for a sub-template body shown in place, its reference's paragraph.
    public var target: AccessibleID?
    /// Its children.
    public var children: [AccessibleNode]

    /// A node.
    public init(
        id: AccessibleID, role: AccessibleRole, name: String? = nil, text: String? = nil, level: Int? = nil,
        label: String? = nil, position: ListPosition? = nil, cell: CellPlace? = nil, marks: [AccessibleMark] = [],
        author: String? = nil, lang: String? = nil, details: AccessibleID? = nil, target: AccessibleID? = nil,
        children: [AccessibleNode] = []
    ) {
        self.id = id
        self.role = role
        self.name = name
        self.text = text
        self.level = level
        self.label = label
        self.position = position
        self.cell = cell
        self.marks = marks
        self.author = author
        self.lang = lang
        self.details = details
        self.target = target
        self.children = children
    }

    enum CodingKeys: String, CodingKey {
        case id, role, name, text, level, label, position, cell, marks, author, lang, details, target, children
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(AccessibleID.self, forKey: .id)
        role = try c.decode(AccessibleRole.self, forKey: .role)
        name = try c.decodeIfPresent(String.self, forKey: .name)
        text = try c.decodeIfPresent(String.self, forKey: .text)
        level = try c.decodeIfPresent(Int.self, forKey: .level)
        label = try c.decodeIfPresent(String.self, forKey: .label)
        position = try c.decodeIfPresent(ListPosition.self, forKey: .position)
        cell = try c.decodeIfPresent(CellPlace.self, forKey: .cell)
        marks = try c.decodeIfPresent([AccessibleMark].self, forKey: .marks) ?? []
        author = try c.decodeIfPresent(String.self, forKey: .author)
        lang = try c.decodeIfPresent(String.self, forKey: .lang)
        details = try c.decodeIfPresent(AccessibleID.self, forKey: .details)
        target = try c.decodeIfPresent(AccessibleID.self, forKey: .target)
        children = try c.decode([AccessibleNode].self, forKey: .children)
    }
}

/// A change to the accessibility tree.
public enum A11yChange: Hashable, Sendable, Codable {
    /// The node with this node's id, and its subtree, are replaced by `node`.
    case replace(node: AccessibleNode)
    /// A node's children are now these, in this order. Ids it didn't have before are nodes sent by a `replace` in
    /// the same update.
    case children(parent: AccessibleID, ids: [AccessibleID])

    public init(from decoder: any Decoder) throws {
        let t = try TaggedDecoder(decoder)
        switch t.tag {
        case "replace": self = .replace(node: try t["node"])
        case "children": self = .children(parent: try t["parent"], ids: try t["ids"])
        default: throw t.unknown()
        }
    }

    public func encode(to encoder: any Encoder) throws {
        let t = TaggedEncoder(encoder)
        switch self {
        case .replace(let node): try t.tag("replace", ["node": node])
        case .children(let parent, let ids): try t.tag("children", ["parent": parent, "ids": ids])
        }
    }
}

/// The accessibility tree, or what changed in it since the last update. On the wire: `{"full": node}` or
/// `{"patch": [change]}`.
public enum A11yUpdate: Hashable, Sendable, Codable {
    /// The whole tree.
    case full(AccessibleNode)
    /// The changes since the last update, applied in order.
    case patch([A11yChange])

    enum CodingKeys: String, CodingKey {
        case full, patch
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        if let node = try c.decodeIfPresent(AccessibleNode.self, forKey: .full) { self = .full(node) }
        else { self = .patch(try c.decode([A11yChange].self, forKey: .patch)) }
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .full(let node): try c.encode(node, forKey: .full)
        case .patch(let changes): try c.encode(changes, forKey: .patch)
        }
    }
}

// MARK: - Caret window

/// The text around the caret, for the element that has the keyboard focus while the editor does: the caret's
/// paragraph, the one before it and the ones after it (up to about 2,000 characters), all in the caret's container (the
/// body, a cell, a note, a header or footer, a text box), each split into its laid-out lines; and the selection in it.
public struct CaretWindow: Hashable, Sendable, Codable {
    /// The paragraphs, in order.
    public var paragraphs: [WindowParagraph]
    /// The selection's anchor, at the window's edge when it is outside.
    public var anchor: WindowPoint
    /// The selection's focus: the caret.
    public var focus: WindowPoint
    /// The selection is all of its story.
    public var all: Bool
    /// The caret's language (BCP 47).
    public var lang: String
    /// The caret is where text can't go in (a locked control, a read-only document).
    public var readOnly: Bool

    /// A caret window.
    public init(paragraphs: [WindowParagraph], anchor: WindowPoint, focus: WindowPoint, all: Bool = false, lang: String, readOnly: Bool = false) {
        self.paragraphs = paragraphs
        self.anchor = anchor
        self.focus = focus
        self.all = all
        self.lang = lang
        self.readOnly = readOnly
    }
}

/// A paragraph of a caret window.
public struct WindowParagraph: Hashable, Sendable, Codable, Identifiable {
    /// Its id: the same as its node's in the accessibility tree.
    public var id: AccessibleID
    /// `paragraph`, `heading` or `listItem`.
    public var role: AccessibleRole
    /// A heading's level.
    public var level: Int?
    /// A numbered list item's number as shown; it isn't part of the text or its offsets.
    public var label: String?
    /// Its laid-out lines; a line break ends one too.
    public var lines: [WindowLine]

    /// A paragraph.
    public init(id: AccessibleID, role: AccessibleRole = .paragraph, level: Int? = nil, label: String? = nil, lines: [WindowLine] = []) {
        self.id = id
        self.role = role
        self.level = level
        self.label = label
        self.lines = lines
    }
}

/// A line of a caret window's paragraph.
public struct WindowLine: Hashable, Sendable, Codable {
    /// Its text, split where marks, author or language change.
    public var segments: [WindowSegment]

    /// A line of `segments`.
    public init(segments: [WindowSegment] = []) {
        self.segments = segments
    }
}

/// Text of a caret window's line with the same marks, author and language.
public struct WindowSegment: Hashable, Sendable, Codable {
    /// Its text.
    public var text: String
    /// Its review and spelling marks.
    @OmitEmpty public var marks: [AccessibleMark]
    /// Whose marks they are.
    public var author: String?
    /// Its language, where it differs from the document's.
    public var lang: String?

    /// A segment.
    public init(text: String, marks: [AccessibleMark] = [], author: String? = nil, lang: String? = nil) {
        self.text = text
        self.marks = marks
        self.author = author
        self.lang = lang
    }

    enum CodingKeys: String, CodingKey {
        case text, marks, author, lang
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        text = try c.decode(String.self, forKey: .text)
        marks = try c.decodeIfPresent([AccessibleMark].self, forKey: .marks) ?? []
        author = try c.decodeIfPresent(String.self, forKey: .author)
        lang = try c.decodeIfPresent(String.self, forKey: .lang)
    }
}

/// A place in a caret window: a paragraph (its index in the window) and an offset in its text (characters, not
/// counting a list item's label).
public struct WindowPoint: Hashable, Sendable, Codable {
    /// The paragraph's index in the window.
    public var paragraph: Int
    /// The offset in its text.
    public var offset: Int

    /// A place.
    public init(paragraph: Int, offset: Int) {
        self.paragraph = paragraph
        self.offset = offset
    }
}

// MARK: - Announcements

/// What an announcement is about.
public enum AnnouncementKind: String, Hashable, Sendable, Codable, CaseIterable {
    /// Entering, leaving or moving through a table.
    case table
    /// Entering or leaving a list.
    case list
    /// The caret moved to another page.
    case page
    /// An insertion or deletion at the caret.
    case review
    /// A commented range at the caret, with the comment.
    case comment
    /// A misspelled word at the caret.
    case spelling
    /// Formatting turned on or off, or described on request.
    case formatting
    /// Entering or leaving a header, footer, note or text box.
    case mode
    /// Read-only content: entered, or an edit refused there.
    case readOnly
    /// What a command did (undo, redo, cut, copy, paste).
    case command
    /// The selection (select all).
    case selection
}
