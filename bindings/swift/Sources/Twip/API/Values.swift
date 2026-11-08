import Foundation

// Values that commands, state and services share (crates/twip/src/api/value.rs): ids, and the formatting
// vocabularies (underline styles, alignments, tab stops, wrapping, content control kinds). Points, colours, bytes,
// `AutoOr`, `NoneOr` and `OutlineLevel` are in Wire.swift.
//
// Renames from Rust: ElementId → ElementID, CommentId → CommentID, RevisionId → RevisionID, HeadingId → HeadingID,
// MatchId → MatchID, StyleId → StyleID, Alignment → ParagraphAlignment, HorizontalAlignment → HorizontalAlign.

// MARK: - Ids

/// A merge-designer element: a field or a content control. Stable while the document is open; undo restores it.
public struct ElementID: WireID {
    public let rawValue: String
    public init(rawValue: String) { self.rawValue = rawValue }
}

/// A comment: a thread's id is its first comment's.
public struct CommentID: WireID {
    public let rawValue: String
    public init(rawValue: String) { self.rawValue = rawValue }
}

/// A revision (a tracked change).
public struct RevisionID: WireID {
    public let rawValue: String
    public init(rawValue: String) { self.rawValue = rawValue }
}

/// A heading the navigation pane lists.
public struct HeadingID: WireID {
    public let rawValue: String
    public init(rawValue: String) { self.rawValue = rawValue }
}

/// A find match.
public struct MatchID: WireID {
    public let rawValue: String
    public init(rawValue: String) { self.rawValue = rawValue }
}

/// A style, by its `w:styleId`.
public struct StyleID: WireID {
    public let rawValue: String
    public init(rawValue: String) { self.rawValue = rawValue }
}

// MARK: - Directions and stories

/// The two ways along a sequence.
public enum Direction: String, Hashable, Sendable, Codable, CaseIterable {
    /// Toward the start.
    case backward
    /// Toward the end.
    case forward
}

/// Next or previous, in document order.
public enum Order: String, Hashable, Sendable, Codable, CaseIterable {
    /// The next one.
    case next
    /// The previous one.
    case previous
}

/// Where the selection is: the body or another story.
public enum Story: String, Hashable, Sendable, Codable, CaseIterable {
    /// The main document.
    case body
    /// A header.
    case header
    /// A footer.
    case footer
    /// A footnote.
    case footnote
    /// An endnote.
    case endnote
    /// A text box.
    case textBox
    /// A sub-template's body (a building block), while the editor is on it.
    case subTemplate
}

// MARK: - Character and paragraph formatting

/// `w:u` styles.
public enum UnderlineStyle: String, Hashable, Sendable, Codable, CaseIterable {
    /// No underline.
    case none
    /// A single line.
    case single
    /// Words only, not the spaces between them.
    case words
    /// A double line.
    case double
    /// A thick line.
    case thick
    /// A dotted line.
    case dotted
    /// A dashed line.
    case dash
    /// Alternating dots and dashes.
    case dotDash
    /// A wavy line.
    case wave
}

/// `w:highlight` colours.
public enum HighlightColor: String, Hashable, Sendable, Codable, CaseIterable {
    /// Yellow.
    case yellow
    /// Bright green.
    case brightGreen
    /// Turquoise.
    case turquoise
    /// Pink.
    case pink
    /// Blue.
    case blue
    /// Red.
    case red
    /// Dark blue.
    case darkBlue
    /// Teal.
    case teal
    /// Green.
    case green
    /// Violet.
    case violet
    /// Dark red.
    case darkRed
    /// Dark yellow.
    case darkYellow
    /// 50% gray.
    case gray50
    /// 25% gray.
    case gray25
    /// Black.
    case black
}

/// A paragraph's alignment.
public enum ParagraphAlignment: String, Hashable, Sendable, Codable, CaseIterable {
    /// Aligned left.
    case left
    /// Centred.
    case center
    /// Aligned right.
    case right
    /// Justified: spread to both margins.
    case justify
}

/// Left, centre or right: tables on the page, page numbers in a header.
public enum HorizontalAlign: String, Hashable, Sendable, Codable, CaseIterable {
    /// Left.
    case left
    /// Centred.
    case center
    /// Right.
    case right
}

/// Text in a cell, top to bottom.
public enum CellAlignment: String, Hashable, Sendable, Codable, CaseIterable {
    /// At the top.
    case top
    /// Centred vertically.
    case center
    /// At the bottom.
    case bottom
}

/// Line spacing: a multiple of single, or a size in points. On the wire tagged by `rule`.
public enum LineSpacing: Hashable, Sendable, Codable {
    /// `lines` times single spacing (0.06…132).
    case multiple(lines: Double)
    /// Exactly `size` (0.7…1584 pt).
    case exact(size: Points)
    /// At least `size` (0.7…1584 pt).
    case atLeast(size: Points)

    /// Single spacing.
    public static let single = LineSpacing.multiple(lines: 1)

    public init(from decoder: any Decoder) throws {
        let t = try TaggedDecoder(decoder, tag: "rule")
        switch t.tag {
        case "multiple": self = .multiple(lines: try t["lines"])
        case "exact": self = .exact(size: try t["size"])
        case "atLeast": self = .atLeast(size: try t["size"])
        default: throw t.unknown()
        }
    }

    public func encode(to encoder: any Encoder) throws {
        let t = TaggedEncoder(encoder, tag: "rule")
        switch self {
        case .multiple(let lines): try t.tag("multiple", ["lines": lines])
        case .exact(let size): try t.tag("exact", ["size": size])
        case .atLeast(let size): try t.tag("atLeast", ["size": size])
        }
    }
}

// MARK: - Tab stops

/// How a tab stop aligns the text after it.
public enum TabAlignment: String, Hashable, Sendable, Codable, CaseIterable {
    /// Text starts at the stop.
    case left
    /// Text centres on the stop.
    case center
    /// Text ends at the stop.
    case right
    /// The decimal point sits at the stop.
    case decimal
    /// A vertical bar at the stop, with no effect on the text.
    case bar
}

/// What fills the space before a tab stop.
public enum TabLeader: String, Hashable, Sendable, Codable, CaseIterable {
    /// No leader.
    case none
    /// Dots.
    case dots
    /// Dashes.
    case dashes
    /// A line.
    case underline
    /// A heavy line.
    case heavy
    /// Middle dots.
    case middleDot
}

// MARK: - Pages and floating objects

/// A section's page orientation.
public enum Orientation: String, Hashable, Sendable, Codable, CaseIterable {
    /// Taller than wide.
    case portrait
    /// Wider than tall.
    case landscape
}

/// How text wraps around a picture or text box.
public enum ObjectWrap: String, Hashable, Sendable, Codable, CaseIterable {
    /// In line with the text.
    case inline
    /// Text wraps around its bounding box.
    case square
    /// Text wraps tightly around its outline.
    case tight
    /// Text wraps through its open parts.
    case through
    /// Text sits above and below it, not beside it.
    case topAndBottom
    /// Behind the text.
    case behindText
    /// In front of the text.
    case inFrontOfText
}

/// What a floating object's horizontal position is measured from.
public enum HorizontalReference: String, Hashable, Sendable, Codable, CaseIterable {
    /// The column it is anchored in (the default).
    case column
    /// The area between the left and right margins.
    case margin
    /// The page's edges.
    case page
    /// The character its anchor is at.
    case character
    /// The left margin's area.
    case leftMargin
    /// The right margin's area.
    case rightMargin
    /// The inside margin's area (left on odd pages, right on even ones).
    case insideMargin
    /// The outside margin's area.
    case outsideMargin
}

/// What a floating object's vertical position is measured from.
public enum VerticalReference: String, Hashable, Sendable, Codable, CaseIterable {
    /// Its anchor paragraph's top: it moves with the text (the default).
    case paragraph
    /// Its anchor line's top: it moves with the text.
    case line
    /// The area between the top and bottom margins.
    case margin
    /// The page's edges: it stays where it is on the page.
    case page
    /// The top margin's area.
    case topMargin
    /// The bottom margin's area.
    case bottomMargin
    /// The inside margin's area.
    case insideMargin
    /// The outside margin's area.
    case outsideMargin
}

/// A floating object aligned across its horizontal reference.
public enum ObjectHorizontalAlignment: String, Hashable, Sendable, Codable, CaseIterable {
    /// At its left edge.
    case left
    /// Centred.
    case center
    /// At its right edge.
    case right
    /// At its inside edge (left on odd pages).
    case inside
    /// At its outside edge.
    case outside
}

/// A floating object aligned across its vertical reference.
public enum ObjectVerticalAlignment: String, Hashable, Sendable, Codable, CaseIterable {
    /// At its top.
    case top
    /// Centred.
    case center
    /// At its bottom.
    case bottom
    /// At its inside edge.
    case inside
    /// At its outside edge.
    case outside
}

/// Which sides of a floating object text wraps on.
public enum WrapSide: String, Hashable, Sendable, Codable, CaseIterable {
    /// Both sides (the default).
    case bothSides
    /// Its left only.
    case left
    /// Its right only.
    case right
    /// Whichever side has more room.
    case largest
}

/// A floating object's horizontal place: aligned across its reference, or an offset from the reference's left edge.
/// Given with neither, the object stays where it is, measured from the new reference.
public struct HorizontalPosition: Hashable, Sendable, Codable {
    /// What it is measured from.
    public var relativeTo: HorizontalReference
    /// Aligned across the reference.
    public var align: ObjectHorizontalAlignment?
    /// Right of the reference's left edge (−1584…1584 pt).
    public var offset: Points?

    /// A horizontal place.
    public init(relativeTo: HorizontalReference = .column, align: ObjectHorizontalAlignment? = nil, offset: Points? = nil) {
        self.relativeTo = relativeTo
        self.align = align
        self.offset = offset
    }
}

/// A floating object's vertical place: aligned across its reference, or an offset below the reference's top. Given
/// with neither, the object stays where it is, measured from the new reference.
public struct VerticalPosition: Hashable, Sendable, Codable {
    /// What it is measured from.
    public var relativeTo: VerticalReference
    /// Aligned across the reference.
    public var align: ObjectVerticalAlignment?
    /// Below the reference's top (−1584…1584 pt).
    public var offset: Points?

    /// A vertical place.
    public init(relativeTo: VerticalReference = .paragraph, align: ObjectVerticalAlignment? = nil, offset: Points? = nil) {
        self.relativeTo = relativeTo
        self.align = align
        self.offset = offset
    }
}

// MARK: - Tables

/// A table's preferred width. On the wire tagged by `kind`, the number in `value`.
public enum TableWidth: Hashable, Sendable, Codable {
    /// Sized by its content.
    case auto
    /// A width in points.
    case points(Points)
    /// 1…600 percent of the text width.
    case percent(Double)

    public init(from decoder: any Decoder) throws {
        let t = try TaggedDecoder(decoder, tag: "kind")
        switch t.tag {
        case "auto": self = .auto
        case "points": self = .points(try t.value())
        case "percent": self = .percent(try t.value())
        default: throw t.unknown()
        }
    }

    public func encode(to encoder: any Encoder) throws {
        let t = TaggedEncoder(encoder, tag: "kind")
        switch self {
        case .auto: try t.tag("auto")
        case .points(let v): try t.tag("points", value: v)
        case .percent(let v): try t.tag("percent", value: v)
        }
    }
}

/// How a row height applies.
public enum HeightRule: String, Hashable, Sendable, Codable, CaseIterable {
    /// At least the height: taller when the content needs it.
    case atLeast
    /// Exactly the height.
    case exact
}

/// A row height: at least or exactly `height` (0…1584 pt).
public struct RowHeight: Hashable, Sendable, Codable {
    /// The height.
    public var height: Points
    /// How it applies.
    public var rule: HeightRule

    /// A row height.
    public init(height: Points, rule: HeightRule = .atLeast) {
        self.height = height
        self.rule = rule
    }
}

// MARK: - Content controls

/// The kinds of content control.
public enum ControlKind: String, Hashable, Sendable, Codable, CaseIterable {
    /// Formatted text.
    case richText
    /// Unformatted text.
    case plainText
    /// A picture.
    case picture
    /// A checkbox.
    case checkbox
    /// A date picker.
    case date
    /// A drop-down list.
    case dropDown
    /// A combo box: a list that also takes typed text.
    case comboBox
    /// A group of controls, edited only inside its child controls.
    case group
    /// A section that repeats per item.
    case repeatingSection
}

/// What a content control's lock keeps: the control itself, its content, or both.
public enum ControlLock: String, Hashable, Sendable, Codable, CaseIterable {
    /// Not locked.
    case none
    /// The control can't be deleted.
    case control
    /// The content can't be edited.
    case content
    /// The control can't be deleted and its content can't be edited.
    case both
}

/// A content control's binding to a custom XML data part (`w:dataBinding`).
public struct DataBinding: Hashable, Sendable, Codable {
    /// The XPath of the bound node in the data part.
    public var xpath: String
    /// The data part's item id.
    public var storeItemID: String?
    /// The XPath's namespace prefixes (`xmlns:ns0='…'`).
    public var prefixMappings: String?

    enum CodingKeys: String, CodingKey {
        case xpath
        case storeItemID = "storeItemId"
        case prefixMappings
    }

    /// A binding to the node at `xpath`.
    public init(xpath: String, storeItemID: String? = nil, prefixMappings: String? = nil) {
        self.xpath = xpath
        self.storeItemID = storeItemID
        self.prefixMappings = prefixMappings
    }
}

/// One entry of a drop-down or combo box.
public struct ListItem: Hashable, Sendable, Codable {
    /// The text shown.
    public var text: String
    /// The value stored when it is picked.
    public var value: String

    /// An entry.
    public init(text: String, value: String) {
        self.text = text
        self.value = value
    }
}

/// The settings of one control kind. On the wire the object's fields say which kind it is.
public enum ControlOptions: Hashable, Sendable, Codable {
    /// A checkbox's state.
    case checkbox(CheckboxOptions)
    /// A date picker's format and locale.
    case date(DateOptions)
    /// A drop-down's or combo box's entries.
    case list(ListOptions)
    /// A plain text control's settings.
    case plainText(PlainTextOptions)
    /// A picture control's picture size.
    case picture(PictureOptions)

    public init(from decoder: any Decoder) throws {
        // The engine refuses unknown fields, so the first kind whose fields hold every key wins, in its order.
        let keys = Set(try decoder.container(keyedBy: WireKey.self).allKeys.map(\.stringValue))
        if keys.isSubset(of: ["checked"]) { self = .checkbox(try CheckboxOptions(from: decoder)) }
        else if keys.isSubset(of: ["format", "locale"]) { self = .date(try DateOptions(from: decoder)) }
        else if keys == ["items"] { self = .list(try ListOptions(from: decoder)) }
        else if keys.isSubset(of: ["multiline"]) { self = .plainText(try PlainTextOptions(from: decoder)) }
        else if keys.isSubset(of: ["width", "height"]) { self = .picture(try PictureOptions(from: decoder)) }
        else {
            throw DecodingError.dataCorrupted(.init(codingPath: decoder.codingPath, debugDescription: "unknown control options \(keys.sorted())"))
        }
    }

    public func encode(to encoder: any Encoder) throws {
        switch self {
        case .checkbox(let o): try o.encode(to: encoder)
        case .date(let o): try o.encode(to: encoder)
        case .list(let o): try o.encode(to: encoder)
        case .plainText(let o): try o.encode(to: encoder)
        case .picture(let o): try o.encode(to: encoder)
        }
    }
}

/// A checkbox's state.
public struct CheckboxOptions: Hashable, Sendable, Codable {
    /// Whether it is checked.
    public var checked: Bool?

    /// A checkbox's state.
    public init(checked: Bool? = nil) { self.checked = checked }
}

/// A date picker's display format and locale.
public struct DateOptions: Hashable, Sendable, Codable {
    /// The display format (`M/d/yyyy`).
    public var format: String?
    /// The locale (`en-US`).
    public var locale: String?

    /// A date picker's settings.
    public init(format: String? = nil, locale: String? = nil) {
        self.format = format
        self.locale = locale
    }
}

/// A drop-down's or combo box's entries.
public struct ListOptions: Hashable, Sendable, Codable {
    /// The entries, in order.
    public var items: [ListItem]

    /// A list's entries.
    public init(items: [ListItem] = []) { self.items = items }
}

/// Whether a plain text control takes line breaks.
public struct PlainTextOptions: Hashable, Sendable, Codable {
    /// Whether it takes line breaks.
    public var multiline: Bool?

    /// A plain text control's settings.
    public init(multiline: Bool? = nil) { self.multiline = multiline }
}

/// A width and height.
public struct Size: Hashable, Sendable, Codable {
    /// The width.
    public var width: Points
    /// The height.
    public var height: Points

    /// A size.
    public init(width: Points, height: Points) {
        self.width = width
        self.height = height
    }
}

/// A picture control's placeholder picture: what it shows while it shows its placeholder (`w:showingPlcHdr`), in the
/// editor, the saved DOCX and a PDF alike.
public struct PlaceholderImage: Hashable, Sendable, Codable {
    /// The picture: PNG, JPEG, GIF, BMP or SVG.
    public var data: Data
    /// Its MIME type (`image/png`, `image/svg+xml`); the data must be one.
    public var mime: String

    /// A placeholder picture.
    public init(data: Data, mime: String) {
        self.data = data
        self.mime = mime
    }
}

/// A picture control's picture size (1…1584 pt): its placeholder's, and the picture's once filled (fitted to it).
public struct PictureOptions: Hashable, Sendable, Codable {
    /// The width; default 144 pt when inserted, else kept.
    public var width: Points?
    /// The height; default 108 pt when inserted, else kept.
    public var height: Points?

    /// A picture control's size.
    public init(width: Points? = nil, height: Points? = nil) {
        self.width = width
        self.height = height
    }
}
