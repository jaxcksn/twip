import Foundation

// The rulers and the popovers the engine asks for, from crates/twip/src/api/services.rs.
// Renames: Span → RulerSpan.

/// The unit rulers show.
public enum RulerUnit: String, Hashable, Sendable, Codable, CaseIterable {
    /// Inches.
    case inches
    /// Centimeters.
    case centimeters
    /// Millimeters.
    case millimeters
    /// Points.
    case points
    /// Picas.
    case picas
}

/// What a click on the ruler adds.
public enum TabSelector: String, Hashable, Sendable, Codable, CaseIterable {
    /// A left tab stop.
    case left
    /// A center tab stop.
    case center
    /// A right tab stop.
    case right
    /// A decimal tab stop.
    case decimal
    /// A bar tab.
    case bar
    /// Sets the first-line indent.
    case firstLineIndent
    /// Sets the hanging indent.
    case hangingIndent
}

/// A popover the engine asks the application to show by `anchor`, with what it offers and the commands that answer
/// it. Only one is open at a time; `popoverClosed` says the engine dismissed it.
public enum Popover: Hashable, Sendable, Codable {
    /// The tab type picker, answered with `setRulerTabType`: the tab type box, the type a click on the ruler adds
    /// now, and every type, in order, with its name and what it does.
    case tabType(anchor: ClientRect, current: TabSelector, choices: [TabTypeChoice])
    /// One tab stop's options: `addTabStop` at its position changes the alignment or leader, `deleteTabStop` clears
    /// it. Its mark on the ruler, its position from the paragraph's left text edge, its alignment and its leader.
    case tabStop(anchor: ClientRect, position: Points, alignment: TabAlignment, leader: TabLeader)
    /// The options of the correction just made: the corrected text, what was typed and what it became.
    case autoCorrect(anchor: ClientRect, from: String, to: String)

    public init(from decoder: any Decoder) throws {
        let t = try TaggedDecoder(decoder, tag: "kind")
        switch t.tag {
        case "tabType": self = .tabType(anchor: try t["anchor"], current: try t["current"], choices: try t["choices"])
        case "tabStop":
            self = .tabStop(anchor: try t["anchor"], position: try t["position"], alignment: try t["alignment"], leader: try t["leader"])
        case "autoCorrect": self = .autoCorrect(anchor: try t["anchor"], from: try t["from"], to: try t["to"])
        default: throw t.unknown()
        }
    }

    public func encode(to encoder: any Encoder) throws {
        let t = TaggedEncoder(encoder, tag: "kind")
        switch self {
        case .tabType(let anchor, let current, let choices):
            try t.tag("tabType", ["anchor": anchor, "current": current, "choices": choices])
        case .tabStop(let anchor, let position, let alignment, let leader):
            try t.tag("tabStop", ["anchor": anchor, "position": position, "alignment": alignment, "leader": leader])
        case .autoCorrect(let anchor, let from, let to):
            try t.tag("autoCorrect", ["anchor": anchor, "from": from, "to": to])
        }
    }
}

/// One entry of the tab type picker.
public struct TabTypeChoice: Hashable, Sendable, Codable {
    /// The type.
    public var value: TabSelector
    /// Its name (`Decimal`).
    public var name: String
    /// What it does, in one line (`Numbers line up on the decimal point`).
    public var description: String

    /// A choice.
    public init(value: TabSelector, name: String, description: String) {
        self.value = value
        self.name = name
        self.description = description
    }
}

/// A span along a ruler.
public struct RulerSpan: Hashable, Sendable, Codable {
    /// Where it starts, from the page edge.
    public var start: Points
    /// Where it ends, from the page edge.
    public var end: Points

    /// A span from `start` to `end`.
    public init(start: Points, end: Points) {
        self.start = start
        self.end = end
    }
}

/// The indent markers. `firstLine` is relative to `left`; negative is hanging.
public struct RulerIndents: Hashable, Sendable, Codable {
    /// The left indent.
    public var left: Points
    /// The right indent.
    public var right: Points
    /// The first line's indent from `left`.
    public var firstLine: Points

    /// The indents.
    public init(left: Points = 0, right: Points = 0, firstLine: Points = 0) {
        self.left = left
        self.right = right
        self.firstLine = firstLine
    }
}

/// A tab stop, measured from the paragraph's left text edge.
public struct TabStopInfo: Hashable, Sendable, Codable {
    /// Its position.
    public var position: Points
    /// Its alignment.
    public var alignment: TabAlignment
    /// Its leader.
    public var leader: TabLeader
    /// From the paragraph's style.
    public var inherited: Bool

    /// A tab stop.
    public init(position: Points, alignment: TabAlignment, leader: TabLeader, inherited: Bool = false) {
        self.position = position
        self.alignment = alignment
        self.leader = leader
        self.inherited = inherited
    }
}

/// The horizontal ruler's model.
public struct HorizontalRuler: Hashable, Sendable, Codable {
    /// The page's width.
    public var pageWidth: Points
    /// The margins, or the cell or text column around the caret.
    public var text: RulerSpan
    /// The paragraph's indents.
    public var indents: RulerIndents
    /// The paragraph's tab stops.
    public var tabs: [TabStopInfo]
    /// The document's default tab interval.
    public var defaultTabStop: Points
    /// The section's text columns.
    public var columns: [RulerSpan]
    /// The cell boundaries of the caret's row.
    public var tableColumns: [Points]

    /// A horizontal ruler.
    public init(
        pageWidth: Points, text: RulerSpan, indents: RulerIndents = RulerIndents(), tabs: [TabStopInfo] = [],
        defaultTabStop: Points, columns: [RulerSpan] = [], tableColumns: [Points] = []
    ) {
        self.pageWidth = pageWidth
        self.text = text
        self.indents = indents
        self.tabs = tabs
        self.defaultTabStop = defaultTabStop
        self.columns = columns
        self.tableColumns = tableColumns
    }
}

/// The vertical ruler's model.
public struct VerticalRuler: Hashable, Sendable, Codable {
    /// The page's height.
    public var pageHeight: Points
    /// The top and bottom margins.
    public var text: RulerSpan
    /// The row boundaries of the caret's table on this page.
    public var tableRows: [Points]

    /// A vertical ruler.
    public init(pageHeight: Points, text: RulerSpan, tableRows: [Points] = []) {
        self.pageHeight = pageHeight
        self.text = text
        self.tableRows = tableRows
    }
}

/// The ruler model for the selection, in points, with no viewport mapping.
public struct RulerModel: Hashable, Sendable, Codable {
    /// The selected paragraphs differ: the first one's values are shown.
    public var mixed: Bool
    /// The horizontal ruler.
    public var horizontal: HorizontalRuler
    /// The vertical ruler.
    public var vertical: VerticalRuler

    /// A ruler model.
    public init(mixed: Bool = false, horizontal: HorizontalRuler, vertical: VerticalRuler) {
        self.mixed = mixed
        self.horizontal = horizontal
        self.vertical = vertical
    }
}

/// A ruler's mapping to the viewport: the client coordinate of the page edge, and CSS px per point.
public struct RulerAxis: Hashable, Sendable, Codable {
    /// The page edge's client coordinate, CSS px.
    public var origin: Double
    /// CSS px per point.
    public var scale: Double

    /// A mapping.
    public init(origin: Double, scale: Double) {
        self.origin = origin
        self.scale = scale
    }
}

/// The ruler model with its viewport mapping, for an application drawing its own ruler.
public struct RulerView: Hashable, Sendable, Codable {
    /// The model.
    public var model: RulerModel
    /// The page the rulers follow (the caret's).
    public var page: Int
    /// The horizontal ruler's mapping.
    public var horizontal: RulerAxis
    /// The vertical ruler's mapping.
    public var vertical: RulerAxis
    /// The unit to label.
    public var unit: RulerUnit

    /// A ruler view.
    public init(model: RulerModel, page: Int, horizontal: RulerAxis, vertical: RulerAxis, unit: RulerUnit = .inches) {
        self.model = model
        self.page = page
        self.horizontal = horizontal
        self.vertical = vertical
        self.unit = unit
    }
}
