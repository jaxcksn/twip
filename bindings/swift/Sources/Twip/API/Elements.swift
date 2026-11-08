import Foundation

// Elements and data parts, sub-templates, the block structure, and element hover, clicks and cursors, from
// crates/twip/src/api/services.rs.

// MARK: - Elements

/// A text to match in an element property.
public struct ElementMatch: Hashable, Sendable, Codable {
    /// The property: a field's code, or a control's tag or title (`in` on the wire).
    public var field: ElementField
    /// The text it contains.
    public var text: String

    /// A match of `text` in `field`.
    public init(field: ElementField, text: String) {
        self.field = field
        self.text = text
    }

    enum CodingKeys: String, CodingKey {
        case field = "in", text
    }
}

/// Which elements `elements` lists.
public struct ElementFilter: Hashable, Sendable, Codable {
    /// Only this kind (any when unset).
    public var kind: ElementKind?
    /// Only those whose property contains a text (`match` on the wire).
    public var matches: ElementMatch?
    /// Only those in this story (any when unset).
    public var story: Story?
    /// Only the elements in this sub-template's body.
    public var subTemplate: String?

    /// A filter; what is left nil doesn't filter.
    public init(kind: ElementKind? = nil, matches: ElementMatch? = nil, story: Story? = nil, subTemplate: String? = nil) {
        self.kind = kind
        self.matches = matches
        self.story = story
        self.subTemplate = subTemplate
    }

    enum CodingKeys: String, CodingKey {
        case kind, matches = "match", story, subTemplate
    }
}

/// A field or content control.
public struct ElementInfo: Hashable, Sendable, Codable, Identifiable {
    /// The element.
    public var id: ElementID
    /// What kind it is.
    public var kind: ElementKind
    /// A field's code.
    public var code: String?
    /// A control's tag.
    public var tag: String?
    /// A control's title.
    public var title: String?
    /// The result or content text, up to 200 characters.
    public var display: String
    /// The story it is in.
    public var story: Story
    /// The sub-template whose body it is in, when `story` is `subTemplate`.
    public var subTemplate: String?
    /// The element it is in.
    public var parent: ElementID?
    /// Locked against editing.
    public var locked: Bool
    /// A sub-template reference field's sub-template.
    public var reference: SubTemplateReference?
    /// Whether a content control shows its placeholder (`w:showingPlcHdr`).
    public var showingPlaceholder: Bool?
    /// A picture control's picture size.
    public var size: Size?

    /// An element.
    public init(
        id: ElementID, kind: ElementKind, code: String? = nil, tag: String? = nil, title: String? = nil, display: String = "",
        story: Story, subTemplate: String? = nil, parent: ElementID? = nil, locked: Bool = false,
        reference: SubTemplateReference? = nil, showingPlaceholder: Bool? = nil, size: Size? = nil
    ) {
        self.id = id
        self.kind = kind
        self.code = code
        self.tag = tag
        self.title = title
        self.display = display
        self.story = story
        self.subTemplate = subTemplate
        self.parent = parent
        self.locked = locked
        self.reference = reference
        self.showingPlaceholder = showingPlaceholder
        self.size = size
    }
}

/// A sub-template: a building block in a category the rules declare.
public struct SubTemplateInfo: Hashable, Sendable, Codable {
    /// Its name.
    public var name: String
    /// Its building block category.
    public var category: String
    /// The reference fields naming it in the template's body, headers, footers and notes.
    public var references: Int
    /// The reference fields naming it in other sub-templates' bodies.
    public var nestedReferences: Int
    /// The fields and controls in its body.
    public var elements: [ElementInfo]

    /// A sub-template.
    public init(name: String, category: String, references: Int = 0, nestedReferences: Int = 0, elements: [ElementInfo] = []) {
        self.name = name
        self.category = category
        self.references = references
        self.nestedReferences = nestedReferences
        self.elements = elements
    }
}

/// What a sub-template reference field names and whether its body shows.
public struct SubTemplateReference: Hashable, Sendable, Codable {
    /// The sub-template's name, as the field gives it.
    public var name: String
    /// Whether it names a sub-template whose body can show in its place.
    public var resolution: ReferenceResolution
    /// The sub-template's name when only a name in another case matches (`nearMatch`).
    public var nearMatch: String?
    /// Its body shows in its place.
    public var expanded: Bool

    /// A reference.
    public init(name: String, resolution: ReferenceResolution = .resolved, nearMatch: String? = nil, expanded: Bool = false) {
        self.name = name
        self.resolution = resolution
        self.nearMatch = nearMatch
        self.expanded = expanded
    }
}

/// Whether a reference's sub-template resolves, or why its body can't show.
public enum ReferenceResolution: String, Hashable, Sendable, Codable, CaseIterable {
    /// A sub-template has the name.
    case resolved
    /// No sub-template has the name.
    case missing
    /// Only a name in another case matches.
    case nearMatch
    /// The sub-template is already being shown around it.
    case cycle
    /// More than 16 sub-templates deep.
    case tooDeep
    /// The field shares its paragraph with other content.
    case notAlone
    /// The field is in a header, footer, note or text box.
    case notInBody
}

/// An element with its settings and children.
public struct ElementDetail: Hashable, Sendable, Codable, Identifiable {
    /// The element.
    public var id: ElementID
    /// What kind it is.
    public var kind: ElementKind
    /// A field's code.
    public var code: String?
    /// A control's tag.
    public var tag: String?
    /// A control's title.
    public var title: String?
    /// The result or content text, up to 200 characters.
    public var display: String
    /// The story it is in.
    public var story: Story
    /// The sub-template whose body it is in, when `story` is `subTemplate`.
    public var subTemplate: String?
    /// The element it is in.
    public var parent: ElementID?
    /// Locked against editing.
    public var locked: Bool
    /// A control's lock.
    public var lock: ControlLock
    /// A control's placeholder text.
    public var placeholder: String?
    /// A control's data binding.
    public var binding: DataBinding?
    /// A control's options.
    public var options: ControlOptions?
    /// The elements inside it.
    public var children: [ElementID]

    /// An element's detail.
    public init(
        id: ElementID, kind: ElementKind, code: String? = nil, tag: String? = nil, title: String? = nil, display: String = "",
        story: Story, subTemplate: String? = nil, parent: ElementID? = nil, locked: Bool = false, lock: ControlLock,
        placeholder: String? = nil, binding: DataBinding? = nil, options: ControlOptions? = nil, children: [ElementID] = []
    ) {
        self.id = id
        self.kind = kind
        self.code = code
        self.tag = tag
        self.title = title
        self.display = display
        self.story = story
        self.subTemplate = subTemplate
        self.parent = parent
        self.locked = locked
        self.lock = lock
        self.placeholder = placeholder
        self.binding = binding
        self.options = options
        self.children = children
    }
}

// MARK: - Blocks

/// A cell's place in a vertical merge.
public enum VerticalMerge: String, Hashable, Sendable, Codable, CaseIterable {
    /// Not merged.
    case none
    /// The first cell of a merge.
    case start
    /// A cell merged into the one above.
    case `continue`
}

/// The document (or an element) as blocks, for checking a template's structure.
public enum Block: Hashable, Sendable, Codable {
    /// A paragraph: its text's length, in characters, and the elements in it.
    case paragraph(textLength: Int, elements: [ElementID] = [])
    /// A table, with its rows.
    case table(rows: [BlockRow])

    public init(from decoder: any Decoder) throws {
        let t = try TaggedDecoder(decoder, tag: "kind")
        switch t.tag {
        case "paragraph": self = .paragraph(textLength: try t["textLength"], elements: try t["elements"])
        case "table": self = .table(rows: try t["rows"])
        default: throw t.unknown()
        }
    }

    public func encode(to encoder: any Encoder) throws {
        let t = TaggedEncoder(encoder, tag: "kind")
        switch self {
        case .paragraph(let textLength, let elements): try t.tag("paragraph", ["textLength": textLength, "elements": elements])
        case .table(let rows): try t.tag("table", ["rows": rows])
        }
    }
}

/// A table row of `Block.table`.
public struct BlockRow: Hashable, Sendable, Codable {
    /// Its cells.
    public var cells: [BlockCell]

    /// A row of `cells`.
    public init(cells: [BlockCell] = []) {
        self.cells = cells
    }
}

/// A cell of a `BlockRow`.
public struct BlockCell: Hashable, Sendable, Codable {
    /// The grid columns it spans.
    public var span: Int
    /// Its place in a vertical merge.
    public var verticalMerge: VerticalMerge
    /// It has no text.
    public var empty: Bool
    /// Its content.
    public var blocks: [Block]

    /// A cell.
    public init(span: Int = 1, verticalMerge: VerticalMerge = .none, empty: Bool = false, blocks: [Block] = []) {
        self.span = span
        self.verticalMerge = verticalMerge
        self.empty = empty
        self.blocks = blocks
    }
}

// MARK: - Data parts

/// A custom XML data part.
public struct DataPart: Hashable, Sendable, Codable {
    /// Its root element's namespace.
    public var namespace: String?
    /// Its item id (`{GUID}`), which data bindings name.
    public var itemID: String
    /// Its XML.
    public var xml: String

    /// A data part.
    public init(namespace: String? = nil, itemID: String, xml: String) {
        self.namespace = namespace
        self.itemID = itemID
        self.xml = xml
    }

    enum CodingKeys: String, CodingKey {
        case namespace, itemID = "itemId", xml
    }
}

/// A custom XML data part, without its content.
public struct DataPartInfo: Hashable, Sendable, Codable {
    /// Its root element's namespace.
    public var namespace: String?
    /// Its item id (`{GUID}`), which data bindings name.
    public var itemID: String

    /// A data part's info.
    public init(namespace: String? = nil, itemID: String) {
        self.namespace = namespace
        self.itemID = itemID
    }

    enum CodingKeys: String, CodingKey {
        case namespace, itemID = "itemId"
    }
}

/// A picture control the user or the application activated.
public struct PictureControlActivation: Hashable, Sendable, Codable {
    /// The picture control.
    public var element: ElementID

    /// An activation of `element`.
    public init(element: ElementID) {
        self.element = element
    }
}

// MARK: - Hover, clicks, cursors

/// What a press on an element does.
public enum PressAction: String, Hashable, Sendable, Codable, CaseIterable {
    /// Checks or clears a checkbox.
    case toggle
    /// Follows a hyperlink (with Ctrl or Cmd held, or in a read-only editor).
    case followLink
    /// Places the caret in the element's text.
    case placeCaret
    /// Nothing: the element can't be edited here.
    case none
}

/// The field, content control or merge-designer element under the pointer.
public struct ElementHover: Hashable, Sendable, Codable {
    /// The element; `exact` is always false.
    public var element: ElementContext
    /// Its boxes on screen, one per line it spans.
    public var boxes: [ClientRect]
    /// What a press there does.
    public var action: PressAction

    /// A hover.
    public init(element: ElementContext, boxes: [ClientRect] = [], action: PressAction = .placeCaret) {
        self.element = element
        self.boxes = boxes
        self.action = action
    }
}

/// A click on an element, after the editor's own action for it.
public struct ElementClick: Hashable, Sendable, Codable {
    /// The element; `exact` is always false.
    public var element: ElementContext
    /// 1 for a click, 2 for a double click, 3 for a triple click.
    public var count: Int
    /// The button.
    public var button: PointerButton
    /// The modifier keys held.
    public var modifiers: Modifiers

    /// A click.
    public init(element: ElementContext, count: Int, button: PointerButton, modifiers: Modifiers) {
        self.element = element
        self.count = count
        self.button = button
        self.modifiers = modifiers
    }
}

/// The innermost element the selection is in or covers.
public struct ElementSelection: Hashable, Sendable, Codable {
    /// The element.
    public var element: ElementContext
    /// Its boxes on screen, one per line it spans; none while it is scrolled out of view.
    public var boxes: [ClientRect]

    /// A selected element.
    public init(element: ElementContext, boxes: [ClientRect] = []) {
        self.element = element
        self.boxes = boxes
    }
}

/// Which hovered elements a cursor override applies to. On the wire: `{"kind": k}` or `{"action": a}`.
public enum CursorTarget: Hashable, Sendable, Codable {
    /// Elements of a kind.
    case kind(ElementKind)
    /// Elements a press on does this.
    case action(PressAction)

    enum CodingKeys: String, CodingKey {
        case kind, action
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        if let kind = try c.decodeIfPresent(ElementKind.self, forKey: .kind) { self = .kind(kind) }
        else { self = .action(try c.decode(PressAction.self, forKey: .action)) }
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .kind(let kind): try c.encode(kind, forKey: .kind)
        case .action(let action): try c.encode(action, forKey: .action)
        }
    }
}

/// The pointer shape to show over some elements instead of the editor's own.
public struct CursorOverride: Hashable, Sendable, Codable {
    /// The elements.
    public var on: CursorTarget
    /// The shape.
    public var cursor: Cursor

    /// An override showing `cursor` over the elements `on` names.
    public init(on: CursorTarget, cursor: Cursor) {
        self.on = on
        self.cursor = cursor
    }
}
