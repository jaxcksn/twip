import Foundation

// The state snapshot: what an application's controls show, with mixed values explicit. A session rebuilds it after
// every observable change; `sequence` counts those, `revision` counts document changes
// (crates/twip/src/api/state.rs; `Tri` is in Wire.swift).
//
// Renamed from Rust: `State` → `EditorState`, `SELECTION_TEXT_LIMIT` → `SelectionSummary.textLimit`,
// `HeaderFooterInfo.pages` keeps its Rust name (`type` on the wire).

/// Editing, or template design (activations go to the application). Read only is its own flag.
public enum Mode: String, Hashable, Sendable, Codable, CaseIterable {
    /// Editing the document (the default).
    case editing
    /// Designing a template: activations go to the application.
    case design
}

/// What the editor is on: the template, or one of its sub-templates. On the wire its `kind` names it.
public enum EditingTarget: Hashable, Sendable, Codable {
    /// The document itself (the default).
    case template
    /// A sub-template's body: its `name` and building block `category`.
    case subTemplate(name: String, category: String)

    /// Whether it is the template.
    public var isTemplate: Bool { self == .template }

    public init(from decoder: any Decoder) throws {
        let t = try TaggedDecoder(decoder, tag: "kind")
        switch t.tag {
        case "template": self = .template
        case "subTemplate": self = .subTemplate(name: try t["name"], category: try t["category"])
        default: throw t.unknown()
        }
    }

    public func encode(to encoder: any Encoder) throws {
        let t = TaggedEncoder(encoder, tag: "kind")
        switch self {
        case .template: try t.tag("template")
        case .subTemplate(let name, let category): try t.tag("subTemplate", ["name": name, "category": category])
        }
    }
}

/// Whether the document is open and laid out.
public enum LoadState: String, Hashable, Sendable, Codable, CaseIterable {
    /// A document is being opened.
    case opening
    /// Open and laid out (the default).
    case ready
    /// Open, and still being laid out.
    case layingOut
}

/// The state snapshot: what an application's controls show, with mixed values explicit (`Tri`). A session rebuilds
/// it after every observable change; `sequence` counts those, `revision` counts document changes.
public struct EditorState: Hashable, Sendable, Codable {
    /// +1 per committed transaction.
    public var revision: Int
    /// +1 per observable change, selection included.
    public var sequence: Int
    /// Whether a document is opening, ready, or still being laid out.
    public var load: LoadState
    /// Editing or template design.
    public var mode: Mode
    /// The template, or the sub-template being edited (absent on the wire for the template).
    public var editing: EditingTarget
    /// Commands that change the document fail with `read-only`.
    public var readOnly: Bool
    /// Changed since the last open or DOCX export.
    public var modified: Bool
    /// What is selected and where.
    public var selection: SelectionSummary
    /// The selection's character formatting.
    public var formatting: CharacterFormatting
    /// The selection's paragraph formatting.
    public var paragraph: ParagraphFormatting
    /// The selection's section.
    public var page: PageFormatting
    /// Each command's availability and value, by id.
    public var commands: [CommandID: CommandState]
    /// Each dialog's availability: whether `dialogOpen` opens it now.
    public var dialogs: [DialogKind: Bool]
    /// What can be undone and redone.
    public var history: HistoryState
    /// The pages laid out so far, whether that is all of them, and the caret's page.
    public var pages: PagesState
    /// The view settings.
    public var view: ViewState
    /// Track changes and the review counts.
    public var review: ReviewState
    /// Spell checking.
    public var spelling: SpellingState
    /// Font families being fetched, and those no font was found for.
    public var fonts: FontsState
    /// The ruler model for the selection, in points.
    public var ruler: RulerModel
    /// Formatting is held for `pasteFormatting`.
    public var formatPainter: Bool
    /// The document's 12 theme colours, in `ThemeColour` order (text1, background1, text2, background2, accent1 to
    /// accent6, hyperlink, followedHyperlink), for an application's colour picker.
    public var themeColours: [RGBColor]
    /// What typing corrects.
    public var autoCorrect: AutoCorrectOptions

    /// A state snapshot.
    public init(
        revision: Int, sequence: Int, load: LoadState = .ready, mode: Mode = .editing,
        editing: EditingTarget = .template, readOnly: Bool = false, modified: Bool = false,
        selection: SelectionSummary, formatting: CharacterFormatting = CharacterFormatting(),
        paragraph: ParagraphFormatting = ParagraphFormatting(), page: PageFormatting,
        commands: [CommandID: CommandState] = [:], dialogs: [DialogKind: Bool] = [:], history: HistoryState = HistoryState(),
        pages: PagesState, view: ViewState = ViewState(), review: ReviewState = ReviewState(),
        spelling: SpellingState = SpellingState(), fonts: FontsState = FontsState(), ruler: RulerModel,
        formatPainter: Bool = false, themeColours: [RGBColor] = [], autoCorrect: AutoCorrectOptions = AutoCorrectOptions()
    ) {
        self.revision = revision
        self.sequence = sequence
        self.load = load
        self.mode = mode
        self.editing = editing
        self.readOnly = readOnly
        self.modified = modified
        self.selection = selection
        self.formatting = formatting
        self.paragraph = paragraph
        self.page = page
        self.commands = commands
        self.dialogs = dialogs
        self.history = history
        self.pages = pages
        self.view = view
        self.review = review
        self.spelling = spelling
        self.fonts = fonts
        self.ruler = ruler
        self.formatPainter = formatPainter
        self.themeColours = themeColours
        self.autoCorrect = autoCorrect
    }

    enum CodingKeys: String, CodingKey {
        case revision, sequence, load, mode, editing, readOnly, modified, selection, formatting, paragraph, page
        case commands, dialogs, history, pages, view, review, spelling, fonts, ruler, formatPainter, themeColours
        case autoCorrect
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        revision = try c.decode(Int.self, forKey: .revision)
        sequence = try c.decode(Int.self, forKey: .sequence)
        load = try c.decode(LoadState.self, forKey: .load)
        mode = try c.decode(Mode.self, forKey: .mode)
        editing = try c.decodeIfPresent(EditingTarget.self, forKey: .editing) ?? .template
        readOnly = try c.decode(Bool.self, forKey: .readOnly)
        modified = try c.decode(Bool.self, forKey: .modified)
        selection = try c.decode(SelectionSummary.self, forKey: .selection)
        formatting = try c.decode(CharacterFormatting.self, forKey: .formatting)
        paragraph = try c.decode(ParagraphFormatting.self, forKey: .paragraph)
        page = try c.decode(PageFormatting.self, forKey: .page)
        commands = try c.decode([CommandID: CommandState].self, forKey: .commands)
        dialogs = try c.decode([DialogKind: Bool].self, forKey: .dialogs)
        history = try c.decode(HistoryState.self, forKey: .history)
        pages = try c.decode(PagesState.self, forKey: .pages)
        view = try c.decode(ViewState.self, forKey: .view)
        review = try c.decode(ReviewState.self, forKey: .review)
        spelling = try c.decode(SpellingState.self, forKey: .spelling)
        fonts = try c.decode(FontsState.self, forKey: .fonts)
        ruler = try c.decode(RulerModel.self, forKey: .ruler)
        formatPainter = try c.decode(Bool.self, forKey: .formatPainter)
        themeColours = try c.decodeIfPresent([RGBColor].self, forKey: .themeColours) ?? []
        autoCorrect = try c.decodeIfPresent(AutoCorrectOptions.self, forKey: .autoCorrect) ?? AutoCorrectOptions()
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(revision, forKey: .revision)
        try c.encode(sequence, forKey: .sequence)
        try c.encode(load, forKey: .load)
        try c.encode(mode, forKey: .mode)
        if !editing.isTemplate { try c.encode(editing, forKey: .editing) }
        try c.encode(readOnly, forKey: .readOnly)
        try c.encode(modified, forKey: .modified)
        try c.encode(selection, forKey: .selection)
        try c.encode(formatting, forKey: .formatting)
        try c.encode(paragraph, forKey: .paragraph)
        try c.encode(page, forKey: .page)
        try c.encode(commands, forKey: .commands)
        try c.encode(dialogs, forKey: .dialogs)
        try c.encode(history, forKey: .history)
        try c.encode(pages, forKey: .pages)
        try c.encode(view, forKey: .view)
        try c.encode(review, forKey: .review)
        try c.encode(spelling, forKey: .spelling)
        try c.encode(fonts, forKey: .fonts)
        try c.encode(ruler, forKey: .ruler)
        try c.encode(formatPainter, forKey: .formatPainter)
        try c.encode(themeColours, forKey: .themeColours)
        try c.encode(autoCorrect, forKey: .autoCorrect)
    }
}

/// One command's state.
public struct CommandState: Hashable, Sendable, Codable {
    /// Whether running it now would succeed.
    public var enabled: Bool
    /// For on/off commands: whether it is on.
    public var checked: Bool?
    /// The command's state value, where it has one (bold's `Tri`, undo's label).
    public var value: CommandValue?

    /// A command's state.
    public init(enabled: Bool = false, checked: Bool? = nil, value: CommandValue? = nil) {
        self.enabled = enabled
        self.checked = checked
        self.value = value
    }
}

/// A command's state value. On the wire it is the value itself, read as the first case it fits, in this order.
public enum CommandValue: Hashable, Sendable, Codable {
    /// An on/off value (bold).
    case toggle(Tri<Bool>)
    /// A text value (the font family, undo's label).
    case text(Tri<String>)
    /// A length, in points.
    case length(Tri<Points>)
    /// A number (the list level).
    case number(Tri<Double>)
    /// An underline style.
    case underline(Tri<UnderlineStyle>)
    /// A font color, or automatic.
    case color(Tri<AutoOr<RGBColor>>)
    /// A highlight color, or none.
    case highlight(Tri<NoneOr<HighlightColor>>)
    /// A paragraph alignment.
    case alignment(Tri<ParagraphAlignment>)
    /// A line spacing.
    case lineSpacing(Tri<LineSpacing>)
    /// A list kind.
    case list(Tri<ListState>)
    /// The selection mode.
    case selectionMode(SelectionMode)
    /// The markup view.
    case markupView(MarkupView)
    /// The session mode.
    case mode(Mode)
    /// What a click on the ruler adds.
    case tabType(TabSelector)
    /// A positioned table's place (absent for a table in the text).
    case tablePosition(TablePosition)

    public init(from decoder: any Decoder) throws {
        let c = try decoder.singleValueContainer()
        if let v = try? c.decode(Tri<Bool>.self) { self = .toggle(v) }
        else if let v = try? c.decode(Tri<String>.self) { self = .text(v) }
        else if let v = try? c.decode(Tri<Points>.self) { self = .length(v) }
        else if let v = try? c.decode(Tri<Double>.self) { self = .number(v) }
        else if let v = try? c.decode(Tri<UnderlineStyle>.self) { self = .underline(v) }
        else if let v = try? c.decode(Tri<AutoOr<RGBColor>>.self) { self = .color(v) }
        else if let v = try? c.decode(Tri<NoneOr<HighlightColor>>.self) { self = .highlight(v) }
        else if let v = try? c.decode(Tri<ParagraphAlignment>.self) { self = .alignment(v) }
        else if let v = try? c.decode(Tri<LineSpacing>.self) { self = .lineSpacing(v) }
        else if let v = try? c.decode(Tri<ListState>.self) { self = .list(v) }
        else if let v = try? c.decode(SelectionMode.self) { self = .selectionMode(v) }
        else if let v = try? c.decode(MarkupView.self) { self = .markupView(v) }
        else if let v = try? c.decode(Mode.self) { self = .mode(v) }
        else if let v = try? c.decode(TabSelector.self) { self = .tabType(v) }
        else { self = .tablePosition(try c.decode(TablePosition.self)) }
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.singleValueContainer()
        switch self {
        case .toggle(let v): try c.encode(v)
        case .text(let v): try c.encode(v)
        case .length(let v): try c.encode(v)
        case .number(let v): try c.encode(v)
        case .underline(let v): try c.encode(v)
        case .color(let v): try c.encode(v)
        case .highlight(let v): try c.encode(v)
        case .alignment(let v): try c.encode(v)
        case .lineSpacing(let v): try c.encode(v)
        case .list(let v): try c.encode(v)
        case .selectionMode(let v): try c.encode(v)
        case .markupView(let v): try c.encode(v)
        case .mode(let v): try c.encode(v)
        case .tabType(let v): try c.encode(v)
        case .tablePosition(let v): try c.encode(v)
        }
    }
}

/// What is selected and where.
public struct SelectionSummary: Hashable, Sendable, Codable {
    /// The longest selection, in characters, whose `text` the state carries.
    public static let textLimit = 10_000

    /// What kind of selection it is.
    public var kind: SelectionKind
    /// The story the selection is in.
    public var story: Story
    /// The page of the caret, from 1.
    public var page: Int
    /// How the selection extends.
    public var mode: SelectionMode
    /// Nothing is selected (a caret).
    public var empty: Bool
    /// The selection as plain text, paragraphs separated by `\n` (empty for a caret), or nil when it is longer than
    /// `textLimit` characters.
    public var text: String?
    /// The selection's table, or nil outside tables.
    public var table: TableSelection?
    /// The selected picture or text box, or nil.
    public var object: ObjectSelection?
    /// Innermost first: the element the selection is in or covers, then its parents.
    public var context: [ElementContext]
    /// The link at the selection, or nil.
    public var link: LinkInfo?
    /// The thread at the caret.
    public var comment: CommentID?
    /// The revision at the caret.
    public var revision: RevisionID?
    /// Set while a header or footer is being edited.
    public var headerFooter: HeaderFooterInfo?

    /// A selection summary.
    public init(
        kind: SelectionKind = .caret, story: Story, page: Int = 1, mode: SelectionMode, empty: Bool = true,
        text: String? = nil, table: TableSelection? = nil, object: ObjectSelection? = nil,
        context: [ElementContext] = [], link: LinkInfo? = nil, comment: CommentID? = nil,
        revision: RevisionID? = nil, headerFooter: HeaderFooterInfo? = nil
    ) {
        self.kind = kind
        self.story = story
        self.page = page
        self.mode = mode
        self.empty = empty
        self.text = text
        self.table = table
        self.object = object
        self.context = context
        self.link = link
        self.comment = comment
        self.revision = revision
        self.headerFooter = headerFooter
    }
}

/// The kinds of selection.
public enum SelectionKind: String, Hashable, Sendable, Codable, CaseIterable {
    /// A caret, nothing selected (the default).
    case caret
    /// A range of text.
    case text
    /// A block of table cells.
    case cells
    /// A picture or a text box.
    case object
    /// A whole merge-designer element.
    case element
}

/// The selection's table: its shape, the current cell (from 0) and the values the table commands set.
public struct TableSelection: Hashable, Sendable, Codable {
    /// The number of rows.
    public var rows: Int
    /// The number of columns.
    public var columns: Int
    /// The current cell's row, from 0.
    public var row: Int
    /// The current cell's column, from 0.
    public var column: Int
    /// Whether the selected cells can merge.
    public var canMerge: Bool
    /// The first row repeats as a header on each page.
    public var headerRow: Tri<Bool>
    /// Rows can break across pages.
    public var rowCanBreak: Tri<Bool>
    /// The cells' vertical alignment.
    public var cellAlignment: Tri<CellAlignment>
    /// The cells' shading, or none.
    public var cellShading: Tri<NoneOr<RGBColor>>
    /// The rows' height, or automatic.
    public var rowHeight: Tri<AutoOr<RowHeight>>
    /// The column width.
    public var columnWidth: Tri<Points>
    /// The table style, or nil.
    public var style: StyleID?
    /// Which parts of the table style apply.
    public var styleOptions: TableStyleOptions
    /// The table width.
    public var width: TableWidth
    /// The table's alignment between the margins.
    public var alignment: HorizontalAlign
    /// The table indent.
    public var indent: Points

    /// A table selection.
    public init(
        rows: Int, columns: Int, row: Int = 0, column: Int = 0, canMerge: Bool = false,
        headerRow: Tri<Bool> = .unavailable, rowCanBreak: Tri<Bool> = .unavailable,
        cellAlignment: Tri<CellAlignment> = .unavailable, cellShading: Tri<NoneOr<RGBColor>> = .unavailable,
        rowHeight: Tri<AutoOr<RowHeight>> = .unavailable, columnWidth: Tri<Points> = .unavailable,
        style: StyleID? = nil, styleOptions: TableStyleOptions = TableStyleOptions(), width: TableWidth,
        alignment: HorizontalAlign, indent: Points = 0
    ) {
        self.rows = rows
        self.columns = columns
        self.row = row
        self.column = column
        self.canMerge = canMerge
        self.headerRow = headerRow
        self.rowCanBreak = rowCanBreak
        self.cellAlignment = cellAlignment
        self.cellShading = cellShading
        self.rowHeight = rowHeight
        self.columnWidth = columnWidth
        self.style = style
        self.styleOptions = styleOptions
        self.width = width
        self.alignment = alignment
        self.indent = indent
    }
}

/// Which parts of a table style apply.
public struct TableStyleOptions: Hashable, Sendable, Codable {
    /// The header row is formatted.
    public var headerRow: Bool
    /// The total (last) row is formatted.
    public var totalRow: Bool
    /// The first column is formatted.
    public var firstColumn: Bool
    /// The last column is formatted.
    public var lastColumn: Bool
    /// Rows alternate.
    public var bandedRows: Bool
    /// Columns alternate.
    public var bandedColumns: Bool

    /// Table style options.
    public init(
        headerRow: Bool = false, totalRow: Bool = false, firstColumn: Bool = false, lastColumn: Bool = false,
        bandedRows: Bool = false, bandedColumns: Bool = false
    ) {
        self.headerRow = headerRow
        self.totalRow = totalRow
        self.firstColumn = firstColumn
        self.lastColumn = lastColumn
        self.bandedRows = bandedRows
        self.bandedColumns = bandedColumns
    }
}

/// The kinds of selectable object.
public enum ObjectKind: String, Hashable, Sendable, Codable, CaseIterable {
    /// A picture (the default).
    case picture
    /// A text box.
    case textBox
    /// A shape.
    case shape
    /// A line or connector (a shape whose preset has no filled path).
    case line
    /// A group of objects.
    case group
}

/// The selected picture or text box.
public struct ObjectSelection: Hashable, Sendable, Codable {
    /// A picture or a text box.
    public var kind: ObjectKind
    /// How text wraps around it.
    public var wrap: ObjectWrap
    /// The width.
    public var width: Points
    /// The height.
    public var height: Points
    /// The alternative text.
    public var altText: String
    /// Marked decorative.
    public var decorative: Bool
    /// Whether the anchor is locked; nil for inline objects.
    public var anchorLocked: Bool?
    /// Where a floating object is and how text keeps from it; nil for inline objects.
    public var placement: ObjectPlacement?
    /// The shape's properties as written; nil for objects without shape properties (frames) and kept graphics.
    public var shape: ShapeState?
    /// Its rotation, clockwise.
    public var rotation: Degrees
    /// Flipped left to right.
    public var flipH: Bool
    /// Flipped top to bottom.
    public var flipV: Bool
    /// The selected child's place in its group; empty unless a group child is selected.
    public var childPath: [Int]
    /// How many objects are selected (2 or more: a multi-selection, the other fields describing the first).
    public var count: Int

    /// An object selection.
    public init(
        kind: ObjectKind = .picture, wrap: ObjectWrap, width: Points, height: Points, altText: String = "",
        decorative: Bool = false, anchorLocked: Bool? = nil, placement: ObjectPlacement? = nil,
        shape: ShapeState? = nil, rotation: Degrees = 0, flipH: Bool = false, flipV: Bool = false,
        childPath: [Int] = [], count: Int = 0
    ) {
        self.kind = kind
        self.wrap = wrap
        self.width = width
        self.height = height
        self.altText = altText
        self.decorative = decorative
        self.anchorLocked = anchorLocked
        self.placement = placement
        self.shape = shape
        self.rotation = rotation
        self.flipH = flipH
        self.flipV = flipV
        self.childPath = childPath
        self.count = count
    }

    enum CodingKeys: String, CodingKey {
        case kind, wrap, width, height, altText, decorative, anchorLocked, placement, shape, rotation, flipH, flipV
        case childPath, count
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        kind = try c.decode(ObjectKind.self, forKey: .kind)
        wrap = try c.decode(ObjectWrap.self, forKey: .wrap)
        width = try c.decode(Points.self, forKey: .width)
        height = try c.decode(Points.self, forKey: .height)
        altText = try c.decode(String.self, forKey: .altText)
        decorative = try c.decode(Bool.self, forKey: .decorative)
        anchorLocked = try c.decodeIfPresent(Bool.self, forKey: .anchorLocked)
        placement = try c.decodeIfPresent(ObjectPlacement.self, forKey: .placement)
        shape = try c.decodeIfPresent(ShapeState.self, forKey: .shape)
        rotation = try c.decodeIfPresent(Degrees.self, forKey: .rotation) ?? 0
        flipH = try c.decodeIfPresent(Bool.self, forKey: .flipH) ?? false
        flipV = try c.decodeIfPresent(Bool.self, forKey: .flipV) ?? false
        childPath = try c.decodeIfPresent([Int].self, forKey: .childPath) ?? []
        count = try c.decodeIfPresent(Int.self, forKey: .count) ?? 0
    }
}

/// A floating object's place and its wrapping options.
public struct ObjectPlacement: Hashable, Sendable, Codable {
    /// The horizontal place: an alignment or an offset.
    public var horizontal: HorizontalPosition
    /// The vertical place: an alignment or an offset.
    public var vertical: VerticalPosition
    /// Other floating objects may overlap it.
    public var allowOverlap: Bool
    /// In a table cell, it is kept within the cell.
    public var layoutInCell: Bool
    /// The sides text wraps on.
    public var wrapSide: WrapSide
    /// The distance from the text: top, bottom, left and right.
    public var distance: TextDistance

    /// An object placement.
    public init(
        horizontal: HorizontalPosition, vertical: VerticalPosition, allowOverlap: Bool = false,
        layoutInCell: Bool = false, wrapSide: WrapSide, distance: TextDistance = TextDistance()
    ) {
        self.horizontal = horizontal
        self.vertical = vertical
        self.allowOverlap = allowOverlap
        self.layoutInCell = layoutInCell
        self.wrapSide = wrapSide
        self.distance = distance
    }
}

/// How far text keeps from a floating object.
public struct TextDistance: Hashable, Sendable, Codable {
    /// Above it.
    public var top: Points
    /// Below it.
    public var bottom: Points
    /// On its left.
    public var left: Points
    /// On its right.
    public var right: Points

    /// A text distance.
    public init(top: Points = 0, bottom: Points = 0, left: Points = 0, right: Points = 0) {
        self.top = top
        self.bottom = bottom
        self.left = left
        self.right = right
    }
}

/// The kinds of merge-designer element.
public enum ElementKind: String, Hashable, Sendable, Codable, CaseIterable {
    /// A field (the default).
    case field
    /// A rich text control.
    case richText
    /// A plain text control.
    case plainText
    /// A picture control.
    case picture
    /// A checkbox control.
    case checkbox
    /// A date control.
    case date
    /// A drop-down list control.
    case dropDown
    /// A combo box control.
    case comboBox
    /// A group control.
    case group
    /// A repeating section control.
    case repeatingSection
    /// One item of a repeating section (selection context only).
    case repeatingItem
}

/// An element the selection is in or covers.
public struct ElementContext: Hashable, Sendable, Codable, Identifiable {
    /// The element's id.
    public var id: ElementID
    /// What kind of element it is.
    public var kind: ElementKind
    /// The selection is exactly the element.
    public var exact: Bool
    /// A field's code.
    public var code: String?
    /// A control's tag.
    public var tag: String?
    /// A control's title.
    public var title: String?
    /// A picture control's picture size.
    public var size: Size?

    /// An element context.
    public init(
        id: ElementID, kind: ElementKind = .field, exact: Bool = false, code: String? = nil, tag: String? = nil,
        title: String? = nil, size: Size? = nil
    ) {
        self.id = id
        self.kind = kind
        self.exact = exact
        self.code = code
        self.tag = tag
        self.title = title
        self.size = size
    }
}

/// The link at the selection.
public struct LinkInfo: Hashable, Sendable, Codable {
    /// Where the link goes.
    public var address: String
    /// The link's display text.
    public var text: String

    /// A link.
    public init(address: String, text: String) {
        self.address = address
        self.text = text
    }
}

/// A header or a footer.
public enum HeaderFooterKind: String, Hashable, Sendable, Codable, CaseIterable {
    /// A header (the default).
    case header
    /// A footer.
    case footer
}

/// Which pages a header or footer is for.
public enum HeaderFooterType: String, Hashable, Sendable, Codable, CaseIterable {
    /// For every page (the default).
    case `default`
    /// For the first page of the section.
    case first
    /// For even pages.
    case even
}

/// The header or footer being edited.
public struct HeaderFooterInfo: Hashable, Sendable, Codable {
    /// A header or a footer.
    public var kind: HeaderFooterKind
    /// Which pages it is for (`type` on the wire).
    public var pages: HeaderFooterType
    /// Its section, from 1.
    public var section: Int
    /// The page it is being edited on, from 1.
    public var page: Int
    /// `First Page Header -Section 2-`.
    public var label: String

    /// A header or footer being edited.
    public init(kind: HeaderFooterKind, pages: HeaderFooterType = .default, section: Int, page: Int, label: String) {
        self.kind = kind
        self.pages = pages
        self.section = section
        self.page = page
        self.label = label
    }

    enum CodingKeys: String, CodingKey {
        case kind
        case pages = "type"
        case section, page, label
    }
}

/// The selection's character formatting.
public struct CharacterFormatting: Hashable, Sendable, Codable {
    /// The document's family, never the substituted one.
    public var font: Tri<String>
    /// The font size.
    public var size: Tri<Points>
    /// Bold.
    public var bold: Tri<Bool>
    /// Italic.
    public var italic: Tri<Bool>
    /// The underline style.
    public var underline: Tri<UnderlineStyle>
    /// Strikethrough.
    public var strikethrough: Tri<Bool>
    /// Double strikethrough.
    public var doubleStrikethrough: Tri<Bool>
    /// Subscript.
    public var `subscript`: Tri<Bool>
    /// Superscript.
    public var superscript: Tri<Bool>
    /// Small caps.
    public var smallCaps: Tri<Bool>
    /// All caps.
    public var allCaps: Tri<Bool>
    /// Hidden text.
    public var hidden: Tri<Bool>
    /// The font color, or automatic.
    public var color: Tri<AutoOr<RGBColor>>
    /// The highlight color, or none.
    public var highlight: Tri<NoneOr<HighlightColor>>
    /// The character spacing.
    public var characterSpacing: Tri<Points>
    /// The raised (positive) or lowered position.
    public var position: Tri<Points>

    /// Character formatting; every value not given is unavailable.
    public init(
        font: Tri<String> = .unavailable, size: Tri<Points> = .unavailable, bold: Tri<Bool> = .unavailable,
        italic: Tri<Bool> = .unavailable, underline: Tri<UnderlineStyle> = .unavailable,
        strikethrough: Tri<Bool> = .unavailable, doubleStrikethrough: Tri<Bool> = .unavailable,
        subscript: Tri<Bool> = .unavailable, superscript: Tri<Bool> = .unavailable,
        smallCaps: Tri<Bool> = .unavailable, allCaps: Tri<Bool> = .unavailable, hidden: Tri<Bool> = .unavailable,
        color: Tri<AutoOr<RGBColor>> = .unavailable, highlight: Tri<NoneOr<HighlightColor>> = .unavailable,
        characterSpacing: Tri<Points> = .unavailable, position: Tri<Points> = .unavailable
    ) {
        self.font = font
        self.size = size
        self.bold = bold
        self.italic = italic
        self.underline = underline
        self.strikethrough = strikethrough
        self.doubleStrikethrough = doubleStrikethrough
        self.subscript = `subscript`
        self.superscript = superscript
        self.smallCaps = smallCaps
        self.allCaps = allCaps
        self.hidden = hidden
        self.color = color
        self.highlight = highlight
        self.characterSpacing = characterSpacing
        self.position = position
    }
}

/// A paragraph's list.
public enum ListState: String, Hashable, Sendable, Codable, CaseIterable {
    /// A bulleted list.
    case bullet
    /// A numbered list.
    case numbered
    /// Not in a list (the default).
    case none
}

/// A paragraph's borders, as the border presets name them.
public enum BorderState: String, Hashable, Sendable, Codable, CaseIterable {
    /// A box around the paragraph.
    case box
    /// A bottom border only.
    case bottom
    /// No borders (the default).
    case none
    /// Borders no preset makes.
    case other
}

/// The selected paragraphs' indents.
public struct IndentState: Hashable, Sendable, Codable {
    /// The left indent.
    public var left: Tri<Points>
    /// The right indent.
    public var right: Tri<Points>
    /// The first-line indent.
    public var firstLine: Tri<Points>
    /// The hanging indent.
    public var hanging: Tri<Points>

    /// Indents; every value not given is unavailable.
    public init(
        left: Tri<Points> = .unavailable, right: Tri<Points> = .unavailable, firstLine: Tri<Points> = .unavailable,
        hanging: Tri<Points> = .unavailable
    ) {
        self.left = left
        self.right = right
        self.firstLine = firstLine
        self.hanging = hanging
    }
}

/// The selected paragraphs' pagination settings.
public struct FlowState: Hashable, Sendable, Codable {
    /// Keep with the next paragraph.
    public var keepWithNext: Tri<Bool>
    /// Keep the lines together.
    public var keepLinesTogether: Tri<Bool>
    /// Start on a new page.
    public var pageBreakBefore: Tri<Bool>
    /// Widow and orphan control.
    public var widowControl: Tri<Bool>

    /// Pagination settings; every value not given is unavailable.
    public init(
        keepWithNext: Tri<Bool> = .unavailable, keepLinesTogether: Tri<Bool> = .unavailable,
        pageBreakBefore: Tri<Bool> = .unavailable, widowControl: Tri<Bool> = .unavailable
    ) {
        self.keepWithNext = keepWithNext
        self.keepLinesTogether = keepLinesTogether
        self.pageBreakBefore = pageBreakBefore
        self.widowControl = widowControl
    }
}

/// The selection's paragraph formatting.
public struct ParagraphFormatting: Hashable, Sendable, Codable {
    /// The paragraph style.
    public var style: Tri<StyleID>
    /// The alignment.
    public var alignment: Tri<ParagraphAlignment>
    /// The line spacing.
    public var lineSpacing: Tri<LineSpacing>
    /// The space before, or automatic.
    public var spaceBefore: Tri<AutoOr<Points>>
    /// The space after, or automatic.
    public var spaceAfter: Tri<AutoOr<Points>>
    /// The indents.
    public var indents: IndentState
    /// The list kind.
    public var list: Tri<ListState>
    /// The list level, from 0.
    public var listLevel: Tri<Int>
    /// The pagination settings.
    public var flow: FlowState
    /// The borders.
    public var borders: Tri<BorderState>
    /// The shading, or none.
    public var shading: Tri<NoneOr<RGBColor>>

    /// Paragraph formatting; every value not given is unavailable.
    public init(
        style: Tri<StyleID> = .unavailable, alignment: Tri<ParagraphAlignment> = .unavailable,
        lineSpacing: Tri<LineSpacing> = .unavailable, spaceBefore: Tri<AutoOr<Points>> = .unavailable,
        spaceAfter: Tri<AutoOr<Points>> = .unavailable, indents: IndentState = IndentState(),
        list: Tri<ListState> = .unavailable, listLevel: Tri<Int> = .unavailable, flow: FlowState = FlowState(),
        borders: Tri<BorderState> = .unavailable, shading: Tri<NoneOr<RGBColor>> = .unavailable
    ) {
        self.style = style
        self.alignment = alignment
        self.lineSpacing = lineSpacing
        self.spaceBefore = spaceBefore
        self.spaceAfter = spaceAfter
        self.indents = indents
        self.list = list
        self.listLevel = listLevel
        self.flow = flow
        self.borders = borders
        self.shading = shading
    }
}

/// A page size.
public struct PageSize: Hashable, Sendable, Codable {
    /// The width.
    public var width: Points
    /// The height.
    public var height: Points

    /// A page size.
    public init(width: Points, height: Points) {
        self.width = width
        self.height = height
    }
}

/// A section's margins.
public struct PageMargins: Hashable, Sendable, Codable {
    /// The top margin.
    public var top: Points
    /// The bottom margin.
    public var bottom: Points
    /// The left margin.
    public var left: Points
    /// The right margin.
    public var right: Points
    /// The gutter.
    public var gutter: Points

    /// Page margins.
    public init(top: Points, bottom: Points, left: Points, right: Points, gutter: Points = 0) {
        self.top = top
        self.bottom = bottom
        self.left = left
        self.right = right
        self.gutter = gutter
    }
}

/// A section's text columns.
public struct ColumnLayout: Hashable, Sendable, Codable {
    /// The number of columns.
    public var count: Int
    /// The space between columns.
    public var spacing: Points
    /// A line is drawn between columns.
    public var separator: Bool

    /// A column layout.
    public init(count: Int = 1, spacing: Points = 0, separator: Bool = false) {
        self.count = count
        self.spacing = spacing
        self.separator = separator
    }
}

/// The selection's section's page setup.
public struct PageFormatting: Hashable, Sendable, Codable {
    /// The page size.
    public var size: PageSize
    /// Portrait or landscape.
    public var orientation: Orientation
    /// The margins.
    public var margins: PageMargins
    /// The text columns.
    public var columns: ColumnLayout
    /// From the page's top edge to the header.
    public var headerDistance: Points
    /// From the page's bottom edge to the footer.
    public var footerDistance: Points
    /// The first page has its own header and footer.
    public var differentFirstPage: Bool
    /// Odd and even pages have their own headers and footers.
    public var differentOddEven: Bool

    /// A page setup.
    public init(
        size: PageSize, orientation: Orientation, margins: PageMargins, columns: ColumnLayout = ColumnLayout(),
        headerDistance: Points, footerDistance: Points, differentFirstPage: Bool = false,
        differentOddEven: Bool = false
    ) {
        self.size = size
        self.orientation = orientation
        self.margins = margins
        self.columns = columns
        self.headerDistance = headerDistance
        self.footerDistance = footerDistance
        self.differentFirstPage = differentFirstPage
        self.differentOddEven = differentOddEven
    }
}

/// What can be undone and redone.
public struct HistoryState: Hashable, Sendable, Codable {
    /// There is something to undo.
    public var canUndo: Bool
    /// There is something to redo.
    public var canRedo: Bool
    /// What undo would undo (`Typing`), or nil.
    public var undoLabel: String?
    /// What redo would redo, or nil.
    public var redoLabel: String?

    /// A history state.
    public init(canUndo: Bool = false, canRedo: Bool = false, undoLabel: String? = nil, redoLabel: String? = nil) {
        self.canUndo = canUndo
        self.canRedo = canRedo
        self.undoLabel = undoLabel
        self.redoLabel = redoLabel
    }
}

/// The pages laid out so far and the caret's page (from 1). Background layout adds pages until `complete`; each
/// change to `count` or `complete` is a new state with the same revision.
public struct PagesState: Hashable, Sendable, Codable {
    /// The number of pages laid out so far: all of them once `complete`.
    public var count: Int
    /// The caret's page, from 1.
    public var current: Int
    /// Background layout has finished: `count` is the document's page count.
    public var complete: Bool

    /// A pages state.
    public init(count: Int, current: Int = 1, complete: Bool = false) {
        self.count = count
        self.current = current
        self.complete = complete
    }
}

/// Which rulers show.
public struct RulerVisibility: Hashable, Sendable, Codable {
    /// The horizontal ruler shows.
    public var horizontal: Bool
    /// The vertical ruler shows.
    public var vertical: Bool

    /// Ruler visibility.
    public init(horizontal: Bool = false, vertical: Bool = false) {
        self.horizontal = horizontal
        self.vertical = vertical
    }
}

/// The view settings.
public struct ViewState: Hashable, Sendable, Codable {
    /// Percent, after any fit.
    public var zoom: Double
    /// Formatting marks are shown.
    public var formattingMarks: Bool
    /// Table gridlines are shown.
    public var gridlines: Bool
    /// Text boundaries are shown.
    public var textBoundaries: Bool
    /// Text wraps around an object again as it is dragged (otherwise a preview shows where it goes, and the text
    /// wraps on the release).
    public var liveLayout: Bool
    /// Field codes are shown.
    public var fieldCodes: Bool
    /// Which rulers show.
    public var rulers: RulerVisibility
    /// The unit the rulers count in.
    public var rulerUnit: RulerUnit
    /// What a click on the ruler adds.
    public var tabSelector: TabSelector
    /// The room beside the pages for balloons, CSS px (0 by default).
    public var markupPaneWidth: Double
    /// Around the pages (the host's colour, or the theme's desk).
    public var background: RGBColor
    /// The chrome's colour scheme.
    public var theme: ChromeTheme
    /// The host's colours for the light and dark schemes.
    public var themeColors: ThemeColors
    /// The chrome's accent as drawn (the dark scheme's is lightened): for popovers and other host chrome that
    /// matches the editor's.
    public var accent: RGBColor
    /// Text and glyphs drawn on the accent.
    public var accentOn: RGBColor
    /// How the page is drawn on screen.
    public var appearance: DocumentAppearance
    /// How much the editor measures while you drag and hover.
    public var detail: MeasureDetail
    /// When the objects' dot grid shows.
    public var objectGrid: ObjectGrid
    /// Content controls and shown sub-templates are labelled while selected or hovered.
    public var controlLabels: Bool
    /// When fields are shaded.
    public var fieldShading: FieldShading
    /// The pointer shapes shown over elements instead of the editor's own.
    @OmitEmpty public var cursorOverrides: [CursorOverride]
    /// Which fields are sub-template references.
    @OmitEmpty public var subTemplateRules: [ReferenceRule]

    /// View settings; the defaults are the engine's.
    public init(
        zoom: Double = 100, formattingMarks: Bool = false, gridlines: Bool = false, textBoundaries: Bool = false,
        liveLayout: Bool = true, fieldCodes: Bool = false, rulers: RulerVisibility = RulerVisibility(),
        rulerUnit: RulerUnit = .inches, tabSelector: TabSelector = .left, markupPaneWidth: Double = 0,
        background: RGBColor = "#E8E7E3", theme: ChromeTheme = .light, themeColors: ThemeColors = ThemeColors(),
        accent: RGBColor = "#0A74E4", accentOn: RGBColor = "#FFFFFF", appearance: DocumentAppearance = .light,
        detail: MeasureDetail = .standard, objectGrid: ObjectGrid = .whileDragging, controlLabels: Bool = true,
        fieldShading: FieldShading = .whenSelected, cursorOverrides: [CursorOverride] = [],
        subTemplateRules: [ReferenceRule] = []
    ) {
        self.zoom = zoom
        self.formattingMarks = formattingMarks
        self.gridlines = gridlines
        self.textBoundaries = textBoundaries
        self.liveLayout = liveLayout
        self.fieldCodes = fieldCodes
        self.rulers = rulers
        self.rulerUnit = rulerUnit
        self.tabSelector = tabSelector
        self.markupPaneWidth = markupPaneWidth
        self.background = background
        self.theme = theme
        self.themeColors = themeColors
        self.accent = accent
        self.accentOn = accentOn
        self.appearance = appearance
        self.detail = detail
        self.objectGrid = objectGrid
        self.controlLabels = controlLabels
        self.fieldShading = fieldShading
        self.cursorOverrides = cursorOverrides
        self.subTemplateRules = subTemplateRules
    }

    enum CodingKeys: String, CodingKey {
        case zoom, formattingMarks, gridlines, textBoundaries, liveLayout, fieldCodes, rulers, rulerUnit, tabSelector
        case markupPaneWidth, background, theme, themeColors, accent, accentOn, appearance, detail, objectGrid
        case controlLabels, fieldShading, cursorOverrides, subTemplateRules
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        zoom = try c.decode(Double.self, forKey: .zoom)
        formattingMarks = try c.decode(Bool.self, forKey: .formattingMarks)
        gridlines = try c.decode(Bool.self, forKey: .gridlines)
        textBoundaries = try c.decode(Bool.self, forKey: .textBoundaries)
        liveLayout = try c.decode(Bool.self, forKey: .liveLayout)
        fieldCodes = try c.decode(Bool.self, forKey: .fieldCodes)
        rulers = try c.decode(RulerVisibility.self, forKey: .rulers)
        rulerUnit = try c.decode(RulerUnit.self, forKey: .rulerUnit)
        tabSelector = try c.decode(TabSelector.self, forKey: .tabSelector)
        markupPaneWidth = try c.decode(Double.self, forKey: .markupPaneWidth)
        background = try c.decode(RGBColor.self, forKey: .background)
        theme = try c.decode(ChromeTheme.self, forKey: .theme)
        themeColors = try c.decode(ThemeColors.self, forKey: .themeColors)
        accent = try c.decode(RGBColor.self, forKey: .accent)
        accentOn = try c.decode(RGBColor.self, forKey: .accentOn)
        appearance = try c.decode(DocumentAppearance.self, forKey: .appearance)
        detail = try c.decode(MeasureDetail.self, forKey: .detail)
        objectGrid = try c.decode(ObjectGrid.self, forKey: .objectGrid)
        controlLabels = try c.decode(Bool.self, forKey: .controlLabels)
        fieldShading = try c.decode(FieldShading.self, forKey: .fieldShading)
        cursorOverrides = try c.decodeIfPresent([CursorOverride].self, forKey: .cursorOverrides) ?? []
        subTemplateRules = try c.decodeIfPresent([ReferenceRule].self, forKey: .subTemplateRules) ?? []
    }
}

/// Track changes and the review counts.
public struct ReviewState: Hashable, Sendable, Codable {
    /// Changes are tracked.
    public var tracking: Bool
    /// Document protection keeps tracking on.
    public var trackingLocked: Bool
    /// How revisions and comments show.
    public var markupView: MarkupView
    /// The number of revisions.
    public var revisions: Int
    /// The number of comment threads.
    public var comments: Int
    /// The active comment thread, or nil.
    public var activeComment: CommentID?

    /// A review state.
    public init(
        tracking: Bool = false, trackingLocked: Bool = false, markupView: MarkupView = .all, revisions: Int = 0,
        comments: Int = 0, activeComment: CommentID? = nil
    ) {
        self.tracking = tracking
        self.trackingLocked = trackingLocked
        self.markupView = markupView
        self.revisions = revisions
        self.comments = comments
        self.activeComment = activeComment
    }
}

/// Spell checking.
public struct SpellingState: Hashable, Sendable, Codable {
    /// Spell checking is on.
    public var enabled: Bool
    /// The language for text with none (BCP 47).
    public var language: String
    /// The number of misspelled words.
    public var misspellings: Int

    /// A spelling state.
    public init(enabled: Bool = true, language: String = "en-US", misspellings: Int = 0) {
        self.enabled = enabled
        self.language = language
        self.misspellings = misspellings
    }
}

/// The document's fonts the library can't draw as the document asks: per family and style, those a resolver is
/// fetching and those nothing provided, and the characters no font has. Each list is sorted by family, then style.
public struct FontsState: Hashable, Sendable, Codable {
    /// Families a resolver is fetching; they draw with a stand-in meanwhile.
    public var pending: [MissingFont]
    /// Families nothing provided; they draw with a stand-in.
    public var missing: [MissingFont]
    /// Characters that neither their run's font, the character-fallback families nor the symbol fallback has; they
    /// draw as their font's missing-glyph box.
    public var characters: [MissingCharacters]

    /// A fonts report.
    public init(pending: [MissingFont] = [], missing: [MissingFont] = [], characters: [MissingCharacters] = []) {
        self.pending = pending
        self.missing = missing
        self.characters = characters
    }
}

/// A family, in one style, that draws with a stand-in.
public struct MissingFont: Hashable, Sendable, Codable {
    /// The family as the document names it, or the replacement target for `StandIn.original`.
    public var family: String
    /// The bold style.
    public var bold: Bool
    /// The italic style.
    public var italic: Bool
    /// The replacement target and alias targets tried, in order.
    public var tried: [String]
    /// The generic category the family has, from the document's font table, an alias or a resolver.
    public var category: GenericFamily?
    /// The family that draws instead; nil when nothing draws (an empty library with no symbol fallback).
    public var drawnWith: String?
    /// Why that family draws.
    public var by: StandIn
    /// The resolver's failure, when it failed.
    public var error: String?

    /// A missing font.
    public init(
        family: String, bold: Bool = false, italic: Bool = false, tried: [String] = [],
        category: GenericFamily? = nil, drawnWith: String? = nil, by: StandIn, error: String? = nil
    ) {
        self.family = family
        self.bold = bold
        self.italic = italic
        self.tried = tried
        self.category = category
        self.drawnWith = drawnWith
        self.by = by
        self.error = error
    }

    enum CodingKeys: String, CodingKey {
        case family, bold, italic, tried, category, drawnWith, by, error
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        family = try c.decode(String.self, forKey: .family)
        bold = try c.decode(Bool.self, forKey: .bold)
        italic = try c.decode(Bool.self, forKey: .italic)
        tried = try c.decodeIfPresent([String].self, forKey: .tried) ?? []
        category = try c.decodeIfPresent(GenericFamily.self, forKey: .category)
        drawnWith = try c.decodeIfPresent(String.self, forKey: .drawnWith)
        by = try c.decode(StandIn.self, forKey: .by)
        error = try c.decodeIfPresent(String.self, forKey: .error)
    }
}

/// Which stand-in draws a missing family.
public enum StandIn: String, Hashable, Sendable, Codable, CaseIterable {
    /// A replacement's target is missing, so the document's own family draws.
    case original
    /// The default family of its generic category.
    case generic
    /// The general fallback family.
    case fallback
    /// The first face added to the library (or the symbol fallback, in an empty library).
    case lastResort
}

/// Characters no font has, by the family their runs name.
public struct MissingCharacters: Hashable, Sendable, Codable {
    /// The family the runs name.
    public var family: String
    /// The distinct characters, in code point order, at most 64.
    public var characters: String
    /// More than 64 characters are missing.
    public var truncated: Bool

    /// Missing characters.
    public init(family: String, characters: String, truncated: Bool = false) {
        self.family = family
        self.characters = characters
        self.truncated = truncated
    }
}
