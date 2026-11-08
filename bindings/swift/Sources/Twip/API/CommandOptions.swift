import Foundation

// The option types and enums of the commands in Command.swift (crates/twip/src/api/command.rs).

/// The kinds of section break.
public enum SectionBreakKind: String, Hashable, Sendable, Codable, CaseIterable {
    /// Starts the new section on the next page.
    case nextPage
    /// Starts the new section on the same page.
    case continuous
    /// Starts the new section on the next even page.
    case evenPage
    /// Starts the new section on the next odd page.
    case oddPage
}

/// What `delete` removes when the selection is collapsed.
public enum DeleteUnit: String, Hashable, Sendable, Codable, CaseIterable {
    /// One character (a grapheme).
    case character
    /// To the word's start or end.
    case word
}

/// How `paste` takes the clipboard.
public enum PasteAs: String, Hashable, Sendable, Codable, CaseIterable {
    /// With its formatting.
    case formatted
    /// As plain text, taking the formatting at the selection.
    case text
}

/// What `select` selects at the caret.
public enum SelectUnit: String, Hashable, Sendable, Codable, CaseIterable {
    /// The word at the caret.
    case word
    /// The sentence at the caret.
    case sentence
    /// The paragraph at the caret.
    case paragraph
    /// The table cell at the caret.
    case cell
    /// The table row at the caret.
    case row
    /// The table column at the caret.
    case column
    /// The table at the caret.
    case table
}

/// What `moveSelection` moves by.
public enum MoveUnit: String, Hashable, Sendable, Codable, CaseIterable {
    /// One character.
    case character
    /// One word.
    case word
    /// One line.
    case line
    /// One paragraph.
    case paragraph
    /// Home / End.
    case lineEdge
    /// The document's start or end (Ctrl+Home / Ctrl+End).
    case document
    /// One screen (Page Up / Page Down).
    case screen
    /// One page.
    case page
    /// The first or last cell of the table row.
    case rowEdge
    /// The first or last cell of the table column.
    case columnEdge
}

/// How the selection grows.
public enum SelectionMode: String, Hashable, Sendable, Codable, CaseIterable {
    /// Moving the caret collapses the selection.
    case normal
    /// Moving the caret extends the selection (F8).
    case extend
    /// Moving the caret extends a rectangular block selection (Ctrl+Shift+F8).
    case block
}

/// `changeCase`'s target.
public enum CaseChange: String, Hashable, Sendable, Codable, CaseIterable {
    /// lowercase.
    case lower
    /// UPPERCASE.
    case upper
    /// Sentence case.
    case sentence
    /// Capitalize Each Word.
    case title
    /// tOGGLE cASE.
    case toggle
}

/// `setParagraphBorderLines`'s options: each edge set, removed (`.some(nil)`, `null` on the wire) or kept (`nil`, absent).
public struct ParagraphBorderLines: Hashable, Sendable, Codable {
    /// The top edge.
    public var top: Border??
    /// The bottom edge.
    public var bottom: Border??
    /// The left edge.
    public var left: Border??
    /// The right edge.
    public var right: Border??
    /// The line between paragraphs with the same borders.
    public var between: Border??
    /// A shadow on the bottom and right edges (Shadow setting).
    public var shadow: Bool?

    /// A `ParagraphBorderLines`.
    public init(top: Border?? = nil, bottom: Border?? = nil, left: Border?? = nil, right: Border?? = nil, between: Border?? = nil, shadow: Bool? = nil) {
        self.top = top
        self.bottom = bottom
        self.left = left
        self.right = right
        self.between = between
        self.shadow = shadow
    }

    private enum CodingKeys: String, CodingKey { case top, bottom, left, right, between, shadow }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        top = c.contains(.top) ? .some(try c.decodeIfPresent(Border.self, forKey: .top)) : nil
        bottom = c.contains(.bottom) ? .some(try c.decodeIfPresent(Border.self, forKey: .bottom)) : nil
        left = c.contains(.left) ? .some(try c.decodeIfPresent(Border.self, forKey: .left)) : nil
        right = c.contains(.right) ? .some(try c.decodeIfPresent(Border.self, forKey: .right)) : nil
        between = c.contains(.between) ? .some(try c.decodeIfPresent(Border.self, forKey: .between)) : nil
        shadow = try c.decodeIfPresent(Bool.self, forKey: .shadow)
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        if let top { try c.encode(top, forKey: .top) }
        if let bottom { try c.encode(bottom, forKey: .bottom) }
        if let left { try c.encode(left, forKey: .left) }
        if let right { try c.encode(right, forKey: .right) }
        if let between { try c.encode(between, forKey: .between) }
        try c.encodeIfPresent(shadow, forKey: .shadow)
    }
}

/// What page borders are measured from.
public enum PageBorderOffset: String, Hashable, Sendable, Codable, CaseIterable {
    /// The page's edge.
    case page
    /// The text.
    case text
}

/// The pages a section's borders show on.
public enum PageBorderDisplay: String, Hashable, Sendable, Codable, CaseIterable {
    /// Every page.
    case allPages
    /// The section's first page.
    case firstPage
    /// Every page but the first.
    case notFirstPage
}

/// The sections page borders apply to.
public enum PageBorderScope: String, Hashable, Sendable, Codable, CaseIterable {
    /// Every section.
    case wholeDocument
    /// The selection's section.
    case thisSection
}

/// `setPageBorders`'s options: each edge set, removed (`.some(nil)`, `null` on the wire) or kept (`nil`, absent).
public struct PageBordersChange: Hashable, Sendable, Codable {
    /// The top edge.
    public var top: Border??
    /// The bottom edge.
    public var bottom: Border??
    /// The left edge.
    public var left: Border??
    /// The right edge.
    public var right: Border??
    /// A shadow on the bottom and right edges.
    public var shadow: Bool?
    /// What the borders are measured from.
    public var offsetFrom: PageBorderOffset?
    /// The pages they show on.
    public var display: PageBorderDisplay?
    /// The sections they apply to.
    public var applyTo: PageBorderScope

    /// A `PageBordersChange`.
    public init(top: Border?? = nil, bottom: Border?? = nil, left: Border?? = nil, right: Border?? = nil, shadow: Bool? = nil, offsetFrom: PageBorderOffset? = nil, display: PageBorderDisplay? = nil, applyTo: PageBorderScope = .thisSection) {
        self.top = top
        self.bottom = bottom
        self.left = left
        self.right = right
        self.shadow = shadow
        self.offsetFrom = offsetFrom
        self.display = display
        self.applyTo = applyTo
    }

    private enum CodingKeys: String, CodingKey { case top, bottom, left, right, shadow, offsetFrom, display, applyTo }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        top = c.contains(.top) ? .some(try c.decodeIfPresent(Border.self, forKey: .top)) : nil
        bottom = c.contains(.bottom) ? .some(try c.decodeIfPresent(Border.self, forKey: .bottom)) : nil
        left = c.contains(.left) ? .some(try c.decodeIfPresent(Border.self, forKey: .left)) : nil
        right = c.contains(.right) ? .some(try c.decodeIfPresent(Border.self, forKey: .right)) : nil
        shadow = try c.decodeIfPresent(Bool.self, forKey: .shadow)
        offsetFrom = try c.decodeIfPresent(PageBorderOffset.self, forKey: .offsetFrom)
        display = try c.decodeIfPresent(PageBorderDisplay.self, forKey: .display)
        applyTo = try c.decodeIfPresent(PageBorderScope.self, forKey: .applyTo) ?? .thisSection
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        if let top { try c.encode(top, forKey: .top) }
        if let bottom { try c.encode(bottom, forKey: .bottom) }
        if let left { try c.encode(left, forKey: .left) }
        if let right { try c.encode(right, forKey: .right) }
        try c.encodeIfPresent(shadow, forKey: .shadow)
        try c.encodeIfPresent(offsetFrom, forKey: .offsetFrom)
        try c.encodeIfPresent(display, forKey: .display)
        try c.encode(applyTo, forKey: .applyTo)
    }
}

/// What shading applies to.
public enum ShadingTarget: String, Hashable, Sendable, Codable, CaseIterable {
    /// The selected paragraphs.
    case paragraph
    /// The selected text.
    case text
    /// A table style part's cells (from the style dialog's Format > Border).
    case cell
}

/// A shading pattern: clear (the fill only), solid (the pattern color), or a percentage of the pattern color over the
/// fill.
public enum ShadingPattern: String, Hashable, Sendable, Codable, CaseIterable {
    /// The fill only.
    case clear
    /// The pattern color only.
    case solid
    /// 5%.
    case pct5
    /// 10%.
    case pct10
    /// 20%.
    case pct20
    /// 25%.
    case pct25
    /// 30%.
    case pct30
    /// 40%.
    case pct40
    /// 50%.
    case pct50
    /// 60%.
    case pct60
    /// 70%.
    case pct70
    /// 75%.
    case pct75
    /// 80%.
    case pct80
    /// 90%.
    case pct90
}

/// Whether text wraps around a frame.
public enum FrameWrap: String, Hashable, Sendable, Codable, CaseIterable {
    /// Text stays above and below the frame.
    case none
    /// Text wraps around the frame.
    case around
}

/// How a frame's width is set.
public enum FrameWidthRule: String, Hashable, Sendable, Codable, CaseIterable {
    /// As wide as its text.
    case auto
    /// The width given.
    case exactly
}

/// How a frame's height is set.
public enum FrameHeightRule: String, Hashable, Sendable, Codable, CaseIterable {
    /// As tall as its text.
    case auto
    /// At least the height given.
    case atLeast
    /// The height given.
    case exactly
}

/// A frame's horizontal alignment.
public enum FrameHorizontalAlign: String, Hashable, Sendable, Codable, CaseIterable {
    /// Left.
    case left
    /// Centered.
    case center
    /// Right.
    case right
    /// Inside (left on odd pages).
    case inside
    /// Outside (right on odd pages).
    case outside
}

/// A frame's vertical alignment.
public enum FrameVerticalAlign: String, Hashable, Sendable, Codable, CaseIterable {
    /// Top.
    case top
    /// Centered.
    case center
    /// Bottom.
    case bottom
    /// Inside.
    case inside
    /// Outside.
    case outside
}

/// What a frame's horizontal position is relative to.
public enum FrameHorizontalRelative: String, Hashable, Sendable, Codable, CaseIterable {
    /// The page margins.
    case margin
    /// The page.
    case page
    /// The column.
    case column
}

/// What a frame's vertical position is relative to.
public enum FrameVerticalRelative: String, Hashable, Sendable, Codable, CaseIterable {
    /// The page margins.
    case margin
    /// The page.
    case page
    /// The paragraph it is anchored to (it moves with the text).
    case paragraph
}

/// `setFrame`'s options: a frame's wrapping, size, position and distance from text.
public struct FrameProperties: Hashable, Sendable, Codable {
    /// Text wrapping.
    public var wrap: FrameWrap
    /// How the width is set.
    public var widthRule: FrameWidthRule
    /// The width, when exact.
    public var width: Points
    /// How the height is set.
    public var heightRule: FrameHeightRule
    /// The height, unless automatic.
    public var height: Points
    /// The horizontal alignment, or none for `horizontal`.
    public var horizontalAlign: FrameHorizontalAlign?
    /// The horizontal position, from `horizontalRelative`.
    public var horizontal: Points
    /// What the horizontal position is relative to.
    public var horizontalRelative: FrameHorizontalRelative
    /// The distance from text, left and right.
    public var horizontalDistance: Points
    /// The vertical alignment, or none for `vertical`.
    public var verticalAlign: FrameVerticalAlign?
    /// The vertical position, from `verticalRelative`.
    public var vertical: Points
    /// What the vertical position is relative to.
    public var verticalRelative: FrameVerticalRelative
    /// The distance from text, above and below.
    public var verticalDistance: Points
    /// The anchor stays with its paragraph.
    public var lockAnchor: Bool

    /// A `FrameProperties`.
    public init(wrap: FrameWrap, widthRule: FrameWidthRule, width: Points, heightRule: FrameHeightRule, height: Points, horizontalAlign: FrameHorizontalAlign? = nil, horizontal: Points, horizontalRelative: FrameHorizontalRelative, horizontalDistance: Points, verticalAlign: FrameVerticalAlign? = nil, vertical: Points, verticalRelative: FrameVerticalRelative, verticalDistance: Points, lockAnchor: Bool) {
        self.wrap = wrap
        self.widthRule = widthRule
        self.width = width
        self.heightRule = heightRule
        self.height = height
        self.horizontalAlign = horizontalAlign
        self.horizontal = horizontal
        self.horizontalRelative = horizontalRelative
        self.horizontalDistance = horizontalDistance
        self.verticalAlign = verticalAlign
        self.vertical = vertical
        self.verticalRelative = verticalRelative
        self.verticalDistance = verticalDistance
        self.lockAnchor = lockAnchor
    }
}

/// `setDocumentDefaults`'s options: the formatting text has before any style's; what isn't given is kept.
public struct DocumentDefaults: Hashable, Sendable, Codable {
    /// The font.
    public var font: String?
    /// The size.
    public var size: Points?
    /// The font color.
    public var color: AutoOr<RGBColor>?
    /// The space before and after paragraphs.
    public var spacing: ParagraphSpacing?
    /// The line spacing.
    public var lineSpacing: LineSpacing?
    /// The left and right indents.
    public var indents: Indents?

    /// A `DocumentDefaults`.
    public init(font: String? = nil, size: Points? = nil, color: AutoOr<RGBColor>? = nil, spacing: ParagraphSpacing? = nil, lineSpacing: LineSpacing? = nil, indents: Indents? = nil) {
        self.font = font
        self.size = size
        self.color = color
        self.spacing = spacing
        self.lineSpacing = lineSpacing
        self.indents = indents
    }
}

/// `setParagraphSpacing`'s options.
public struct ParagraphSpacing: Hashable, Sendable, Codable {
    /// Space before, or auto.
    public var before: AutoOr<Points>?
    /// Space after, or auto.
    public var after: AutoOr<Points>?

    /// A `ParagraphSpacing`.
    public init(before: AutoOr<Points>? = nil, after: AutoOr<Points>? = nil) {
        self.before = before
        self.after = after
    }
}

/// `setIndents`'s options (−1584…1584 pt; `firstLine` and `hanging` exclude each other, both ≥ 0).
public struct Indents: Hashable, Sendable, Codable {
    /// The left indent.
    public var left: Points?
    /// The right indent.
    public var right: Points?
    /// The first line's extra indent.
    public var firstLine: Points?
    /// The hanging indent of the lines after the first.
    public var hanging: Points?

    /// A `Indents`.
    public init(left: Points? = nil, right: Points? = nil, firstLine: Points? = nil, hanging: Points? = nil) {
        self.left = left
        self.right = right
        self.firstLine = firstLine
        self.hanging = hanging
    }
}

/// A list level's number style.
public enum NumberFormat: String, Hashable, Sendable, Codable, CaseIterable {
    /// 1, 2, 3.
    case decimal
    /// I, II, III.
    case upperRoman
    /// i, ii, iii.
    case lowerRoman
    /// A, B, C.
    case upperLetter
    /// a, b, c.
    case lowerLetter
    /// 1st, 2nd, 3rd.
    case ordinal
    /// One, Two, Three.
    case cardinalText
    /// First, Second, Third.
    case ordinalText
    /// 01, 02, 03.
    case decimalZero
    /// A bullet: the level's text in its font, or its picture.
    case bullet
    /// No number.
    case none
}

/// How a list level's number aligns at its position.
public enum LevelAlignment: String, Hashable, Sendable, Codable, CaseIterable {
    /// Left.
    case left
    /// Centered.
    case center
    /// Right.
    case right
}

/// What follows a list level's number.
public enum FollowNumber: String, Hashable, Sendable, Codable, CaseIterable {
    /// A tab.
    case tab
    /// A space.
    case space
    /// Nothing.
    case nothing
}

/// A list level: its number or bullet, and where it and the text start.
public struct ListLevel: Hashable, Sendable, Codable {
    /// The number style.
    public var format: NumberFormat
    /// The number format: its text, with `%1` to `%9` for the numbers of this level and the levels above it. A
    /// bullet's character.
    public var text: String
    /// The number it starts at.
    public var start: Int
    /// How the number aligns at `numberPosition`.
    public var alignment: LevelAlignment
    /// What follows the number.
    public var follow: FollowNumber
    /// The tab stop after the number, when `follow` is a tab: none uses the text indent.
    public var tab: Points?
    /// Where the number is, from the margin.
    public var numberPosition: Points
    /// Where the text is, from the margin.
    public var textIndent: Points
    /// The level after which it restarts (1 to 9); 0 never; none after the level above.
    public var restartAfter: Int?
    /// Legal style numbering: the numbers of every level in Arabic numerals.
    public var legal: Bool
    /// The paragraph style linked to the level.
    public var linkedStyle: StyleID?
    /// The number's or bullet's font.
    public var font: String?
    /// A picture bullet's image (PNG, JPEG or GIF).
    public var picture: Data?

    /// A `ListLevel`.
    public init(format: NumberFormat, text: String, start: Int, alignment: LevelAlignment, follow: FollowNumber, tab: Points? = nil, numberPosition: Points, textIndent: Points, restartAfter: Int? = nil, legal: Bool, linkedStyle: StyleID? = nil, font: String? = nil, picture: Data? = nil) {
        self.format = format
        self.text = text
        self.start = start
        self.alignment = alignment
        self.follow = follow
        self.tab = tab
        self.numberPosition = numberPosition
        self.textIndent = textIndent
        self.restartAfter = restartAfter
        self.legal = legal
        self.linkedStyle = linkedStyle
        self.font = font
        self.picture = picture
    }
}

/// What a list definition applies to.
public enum ListApplyTo: String, Hashable, Sendable, Codable, CaseIterable {
    /// The selection's whole list.
    case wholeList
    /// The list from the selection's first paragraph on: a new list.
    case thisPointForward
    /// The selected paragraphs: a new list.
    case currentParagraph
}

/// How Set Numbering Value sets the number.
public enum NumberingMode: String, Hashable, Sendable, Codable, CaseIterable {
    /// Start a new list.
    case startNew
    /// Continue from the previous list.
    case `continue`
}

/// The kinds of list `setList` applies.
public enum ListKind: String, Hashable, Sendable, Codable, CaseIterable {
    /// A bulleted list.
    case bullet
    /// A numbered list.
    case numbered
}

/// One level deeper or shallower.
public enum LevelChange: String, Hashable, Sendable, Codable, CaseIterable {
    /// One level deeper.
    case increase
    /// One level shallower.
    case decrease
}

/// The paragraph border presets.
public enum BorderPreset: String, Hashable, Sendable, Codable, CaseIterable {
    /// A box around the paragraphs.
    case box
    /// A line below the paragraphs.
    case bottom
    /// No borders.
    case none
}

/// What a page number field shows.
public enum PageNumberFormat: String, Hashable, Sendable, Codable, CaseIterable {
    /// The page number.
    case page
    /// "Page X of Y".
    case pageOfTotal
    /// The total page count.
    case total
}

/// Where a page number goes.
public enum PageNumberPlace: String, Hashable, Sendable, Codable, CaseIterable {
    /// At the caret.
    case caret
    /// In the section's header.
    case header
    /// In the section's footer.
    case footer
}

/// Above or below.
public enum VerticalSide: String, Hashable, Sendable, Codable, CaseIterable {
    /// Above the selection.
    case above
    /// Below the selection.
    case below
}

/// Left or right.
public enum HorizontalSide: String, Hashable, Sendable, Codable, CaseIterable {
    /// To the selection's left.
    case left
    /// To the selection's right.
    case right
}

/// Where `deleteCells` shifts the remaining cells.
public enum CellShift: String, Hashable, Sendable, Codable, CaseIterable {
    /// Shifts the cells below up.
    case up
    /// Shifts the cells to the right left.
    case left
}

/// Up or down.
public enum VerticalDirection: String, Hashable, Sendable, Codable, CaseIterable {
    /// Up.
    case up
    /// Down.
    case down
}

/// What `autoFitColumns` fits the columns to.
public enum AutoFit: String, Hashable, Sendable, Codable, CaseIterable {
    /// Fits the columns to their contents.
    case contents
    /// Fits the table to the page's text width.
    case window
    /// Fixes the columns at their current widths.
    case fixed
}

/// The cell edges `setCellBorders` sets.
public enum BorderEdges: String, Hashable, Sendable, Codable, CaseIterable {
    /// Every edge.
    case all
    /// The outside edges.
    case outside
    /// The inside edges.
    case inside
    /// The inside horizontal edges.
    case insideHorizontal
    /// The inside vertical edges.
    case insideVertical
    /// The top edge.
    case top
    /// The bottom edge.
    case bottom
    /// The left edge.
    case left
    /// The right edge.
    case right
    /// Removes the borders.
    case none
}

/// A border line's style.
public enum BorderStyle: String, Hashable, Sendable, Codable, CaseIterable {
    /// A single line.
    case single
    /// A double line.
    case double
    /// A dotted line.
    case dotted
    /// A dashed line.
    case dashed
    /// A thick line.
    case thick
    /// Dots and dashes.
    case dotDash
    /// Two dots and a dash.
    case dotDotDash
    /// Three lines.
    case triple
    /// A thin line and a thick one.
    case thinThickSmallGap
    /// A thick line and a thin one.
    case thickThinSmallGap
    /// Thin, thick, thin.
    case thinThickThinSmallGap
    /// A wave.
    case wave
    /// Two waves.
    case doubleWave
    /// Raised (3-D).
    case threeDEmboss
    /// Sunken (3-D).
    case threeDEngrave
    /// Outset.
    case outset
    /// Inset.
    case inset
}

/// A border line.
public struct Border: Hashable, Sendable, Codable {
    /// The line style.
    public var style: BorderStyle
    /// 0.25…6 pt.
    public var width: Points
    /// The line color, or automatic.
    public var color: AutoOr<RGBColor>

    /// A `Border`.
    public init(style: BorderStyle, width: Points, color: AutoOr<RGBColor>) {
        self.style = style
        self.width = width
        self.color = color
    }
}

/// `setTableStyleOptions`'s options.
public struct TableStyleChange: Hashable, Sendable, Codable {
    /// Formats the first row as a header.
    public var headerRow: Bool?
    /// Formats the last row as a total.
    public var totalRow: Bool?
    /// Formats the first column.
    public var firstColumn: Bool?
    /// Formats the last column.
    public var lastColumn: Bool?
    /// Alternates row shading.
    public var bandedRows: Bool?
    /// Alternates column shading.
    public var bandedColumns: Bool?

    /// A `TableStyleChange`.
    public init(headerRow: Bool? = nil, totalRow: Bool? = nil, firstColumn: Bool? = nil, lastColumn: Bool? = nil, bandedRows: Bool? = nil, bandedColumns: Bool? = nil) {
        self.headerRow = headerRow
        self.totalRow = totalRow
        self.firstColumn = firstColumn
        self.lastColumn = lastColumn
        self.bandedRows = bandedRows
        self.bandedColumns = bandedColumns
    }
}

/// `setTablePosition`'s value: a positioned table's place on each axis, as `setObjectPosition` gives an object's, and
/// its distance from the text around it; what isn't given is kept. A table is measured from the column, the margin or
/// the page across, and from the paragraph, the margin or the page down.
public struct TablePosition: Hashable, Sendable, Codable {
    /// The horizontal place.
    public var horizontal: HorizontalPosition?
    /// The vertical place.
    public var vertical: VerticalPosition?
    /// How far the text around it keeps (0…1584 pt).
    public var wrapDistance: TextDistance?

    /// A `TablePosition`.
    public init(horizontal: HorizontalPosition? = nil, vertical: VerticalPosition? = nil, wrapDistance: TextDistance? = nil) {
        self.horizontal = horizontal
        self.vertical = vertical
        self.wrapDistance = wrapDistance
    }
}

/// One field's computed result.
public struct FieldResult: Hashable, Sendable, Codable, Identifiable {
    /// The field.
    public var id: ElementID
    /// Its result.
    public var display: TextBody

    /// A `FieldResult`.
    public init(id: ElementID, display: TextBody) {
        self.id = id
        self.display = display
    }
}

/// One sub-template reference rule: a field of type `field_type` names, in argument `argument`, the building block
/// in category `category` (and `gallery`) whose body it shows.
public struct ReferenceRule: Hashable, Sendable, Codable {
    /// The field type (`SUBTEMPLATE`), compared without case.
    public var fieldType: String
    /// Which of the field's arguments is the sub-template's name, from 0. Default: 0.
    public var argument: Int?
    /// The building block category the name is looked up in.
    public var category: String
    /// The building block gallery (`custom1`), when it matters.
    public var gallery: String?

    /// A `ReferenceRule`.
    public init(fieldType: String, argument: Int? = nil, category: String, gallery: String? = nil) {
        self.fieldType = fieldType
        self.argument = argument
        self.category = category
        self.gallery = gallery
    }
}

/// `insertControl`'s options.
public struct InsertControl: Hashable, Sendable, Codable {
    /// The kind of control.
    public var kind: ControlKind
    /// The control's tag, for the application.
    public var tag: String?
    /// The control's title, shown on its handle.
    public var title: String?
    /// Whether the control can be deleted or its content edited.
    public var lock: ControlLock?
    /// The prompt text shown while the control is empty.
    public var placeholder: String?
    /// The custom XML data part node the control is bound to.
    public var binding: DataBinding?
    /// The kind's options (list items, date format, checkbox symbols).
    public var options: ControlOptions?
    /// A picture control's placeholder picture; default: the engine's.
    public var placeholderImage: PlaceholderImage?
    /// Wraps the selection instead of inserting an empty control.
    public var wrapSelection: Bool?
    /// A picture control floats: positioned and wrapped as `setObjectWrap` and `setObjectPosition` place an object.
    public var float: ControlFloat?

    /// A `InsertControl`.
    public init(kind: ControlKind, tag: String? = nil, title: String? = nil, lock: ControlLock? = nil, placeholder: String? = nil, binding: DataBinding? = nil, options: ControlOptions? = nil, placeholderImage: PlaceholderImage? = nil, wrapSelection: Bool? = nil, float: ControlFloat? = nil) {
        self.kind = kind
        self.tag = tag
        self.title = title
        self.lock = lock
        self.placeholder = placeholder
        self.binding = binding
        self.options = options
        self.placeholderImage = placeholderImage
        self.wrapSelection = wrapSelection
        self.float = float
    }
}

/// How `insertControl` floats a picture control.
public struct ControlFloat: Hashable, Sendable, Codable {
    /// How text wraps around it; not `inline`.
    public var wrap: ObjectWrap
    /// The horizontal place; where it would sit in line when not given.
    public var horizontal: HorizontalPosition?
    /// The vertical place; where it would sit in line when not given.
    public var vertical: VerticalPosition?

    /// A `ControlFloat`.
    public init(wrap: ObjectWrap, horizontal: HorizontalPosition? = nil, vertical: VerticalPosition? = nil) {
        self.wrap = wrap
        self.horizontal = horizontal
        self.vertical = vertical
    }
}

/// `setControl`'s options. `binding: null` removes the binding.
public struct SetControl: Hashable, Sendable, Codable, Identifiable {
    /// The control.
    public var id: ElementID
    /// The new tag.
    public var tag: String?
    /// The new title.
    public var title: String?
    /// The new lock.
    public var lock: ControlLock?
    /// The new prompt text.
    public var placeholder: String?
    /// The new binding; `.some(nil)` removes it.
    public var binding: DataBinding??
    /// The new options.
    public var options: ControlOptions?
    /// A picture control's new placeholder picture, while it shows its placeholder.
    public var placeholderImage: PlaceholderImage?

    /// A `SetControl`.
    public init(id: ElementID, tag: String? = nil, title: String? = nil, lock: ControlLock? = nil, placeholder: String? = nil, binding: DataBinding?? = nil, options: ControlOptions? = nil, placeholderImage: PlaceholderImage? = nil) {
        self.id = id
        self.tag = tag
        self.title = title
        self.lock = lock
        self.placeholder = placeholder
        self.binding = binding
        self.options = options
        self.placeholderImage = placeholderImage
    }

    private enum CodingKeys: String, CodingKey { case id, tag, title, lock, placeholder, binding, options, placeholderImage }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(ElementID.self, forKey: .id)
        tag = try c.decodeIfPresent(String.self, forKey: .tag)
        title = try c.decodeIfPresent(String.self, forKey: .title)
        lock = try c.decodeIfPresent(ControlLock.self, forKey: .lock)
        placeholder = try c.decodeIfPresent(String.self, forKey: .placeholder)
        binding = c.contains(.binding) ? .some(try c.decodeIfPresent(DataBinding.self, forKey: .binding)) : nil
        options = try c.decodeIfPresent(ControlOptions.self, forKey: .options)
        placeholderImage = try c.decodeIfPresent(PlaceholderImage.self, forKey: .placeholderImage)
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(id, forKey: .id)
        try c.encodeIfPresent(tag, forKey: .tag)
        try c.encodeIfPresent(title, forKey: .title)
        try c.encodeIfPresent(lock, forKey: .lock)
        try c.encodeIfPresent(placeholder, forKey: .placeholder)
        if let binding { try c.encode(binding, forKey: .binding) }
        try c.encodeIfPresent(options, forKey: .options)
        try c.encodeIfPresent(placeholderImage, forKey: .placeholderImage)
    }
}

/// Which element property `replaceInElements` searches.
public enum ElementField: String, Hashable, Sendable, Codable, CaseIterable {
    /// A field's code.
    case code
    /// A control's tag.
    case tag
    /// A control's title.
    case title
}

/// Fields or content controls.
public enum ElementClass: String, Hashable, Sendable, Codable, CaseIterable {
    /// Fields.
    case field
    /// Content controls.
    case control
}

/// Where `replaceFont` rewrites.
public enum ReplaceScope: String, Hashable, Sendable, Codable, CaseIterable {
    /// The whole document: every story, and the tables below the text.
    case document
    /// The selected text.
    case selection
}

/// How serious an element mark is.
public enum Severity: String, Hashable, Sendable, Codable, CaseIterable {
    /// An error.
    case error
    /// A warning.
    case warning
    /// Information.
    case info
}

/// A mark on one element.
public struct ElementMark: Hashable, Sendable, Codable, Identifiable {
    /// The element.
    public var id: ElementID
    /// How serious the mark is.
    public var severity: Severity
    /// The tooltip.
    public var message: String

    /// A `ElementMark`.
    public init(id: ElementID, severity: Severity, message: String) {
        self.id = id
        self.severity = severity
        self.message = message
    }
}

/// The zoom fits.
public enum ZoomFit: String, Hashable, Sendable, Codable, CaseIterable {
    /// The page's width fills the view.
    case pageWidth
    /// A whole page fits in the view.
    case wholePage
}

/// `setViewOptions`'s options.
public struct ViewOptions: Hashable, Sendable, Codable {
    /// Shows paragraph marks, spaces and tabs.
    public var formattingMarks: Bool?
    /// Shows table gridlines.
    public var gridlines: Bool?
    /// Shows text boundaries.
    public var textBoundaries: Bool?
    /// Text wraps around an object again as it is dragged.
    public var liveLayout: Bool?
    /// The chrome's colour scheme.
    public var theme: ChromeTheme?
    /// The host's colours for the light and dark schemes.
    public var themeColors: ThemeColors?
    /// The chrome's accent: selection, handles, guides and active markers. Any colour; the dark scheme lightens it.
    public var accent: RGBColor?
    /// How the page is drawn on screen (dark mode).
    public var appearance: DocumentAppearance?
    /// How much the editor measures while you drag and hover.
    public var detail: MeasureDetail?
    /// When the dot grid objects snap to shows (unrelated to `gridlines`, the table gridlines).
    public var objectGrid: ObjectGrid?
    /// Labels content controls and shown sub-templates while the caret is in them or the pointer is over them.
    /// Default: on. A control whose `w15:appearance` is `hidden` is never labelled.
    public var controlLabels: Bool?
    /// When fields are shaded. Default: when selected.
    public var fieldShading: FieldShading?
    /// Pointer shapes over elements instead of the editor's own, replacing those set before; the first that matches
    /// the hovered element wins, and `[]` clears them.
    public var cursorOverrides: [CursorOverride]?

    /// A `ViewOptions`.
    public init(formattingMarks: Bool? = nil, gridlines: Bool? = nil, textBoundaries: Bool? = nil, liveLayout: Bool? = nil, theme: ChromeTheme? = nil, themeColors: ThemeColors? = nil, accent: RGBColor? = nil, appearance: DocumentAppearance? = nil, detail: MeasureDetail? = nil, objectGrid: ObjectGrid? = nil, controlLabels: Bool? = nil, fieldShading: FieldShading? = nil, cursorOverrides: [CursorOverride]? = nil) {
        self.formattingMarks = formattingMarks
        self.gridlines = gridlines
        self.textBoundaries = textBoundaries
        self.liveLayout = liveLayout
        self.theme = theme
        self.themeColors = themeColors
        self.accent = accent
        self.appearance = appearance
        self.detail = detail
        self.objectGrid = objectGrid
        self.controlLabels = controlLabels
        self.fieldShading = fieldShading
        self.cursorOverrides = cursorOverrides
    }
}

