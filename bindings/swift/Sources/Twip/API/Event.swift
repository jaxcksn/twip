import Foundation

// Events: what the engine tells its host, in commit order. A session's state is rebuilt before any event of the same
// change is delivered (crates/twip/src/api/event.rs).
//
// Renamed from Rust: `Event` → `EditorEvent`, its `Fault` variant → `error` (its wire name).

/// An event. On the wire its `type` is the event name (`documentChanged`), and its payload's fields sit beside it;
/// a payload that isn't an object (a list, a value or `null`) travels in `value`.
public enum EditorEvent: Hashable, Sendable, Codable {
    /// The state changed. SDKs receive it as their state snapshot, never as an event.
    case stateChanged(EditorState)
    /// Exactly one per commit, in order, never coalesced.
    case documentChanged(TransactionRecord)
    /// The selection moved (coalesced per frame).
    case selectionChanged(SelectionSummary)
    /// A link was clicked (read only) or Ctrl+clicked: its `address`, and `via` how it was activated.
    case linkActivated(address: String, via: LinkVia)
    /// A picture control was activated: clicked while unfilled, double-clicked or given Enter in editing and
    /// read-only modes, or named by `activatePictureControl`.
    case pictureControlActivated(PictureControlActivation)
    /// A context menu to open.
    case contextMenu(ContextMenuEvent)
    /// A click or command asked for a popover by its anchor. Opening one closes the one open.
    case popoverRequested(Popover)
    /// The engine dismissed the open popover: Esc, a press elsewhere, or its target gone.
    case popoverClosed
    /// A key or gesture asked for the dialog `kind`.
    case dialogRequested(kind: DialogKind)
    /// A key the application owns was pressed: `command` is the key's command.
    case hostCommand(command: HostCommand)
    /// An application shortcut was pressed: `id` is the id the application registered it with.
    case shortcut(id: String)
    /// A gesture bound to `emit`.
    case gesture(GestureEvent)
    /// A word was added to the user's dictionary: persist it. `language` is its language tag (`en-US`).
    case wordAdded(word: String, language: String)
    /// The document's families that draw with a stand-in (being fetched, or nothing provided them), or its
    /// characters no font has, changed. Carries the whole report, as `EditorState.fonts` has it.
    case fontsChanged(FontsState)
    /// Design mode: Enter or a double-click on the element `id`.
    case elementActivated(id: ElementID)
    /// Elements came (`added`), went (`removed`) or had their properties or content changed (`changed`).
    case elementsChanged(added: [ElementInfo], removed: [ElementID], changed: [ElementInfo])
    /// Fields the application computes need results: answer with `setFieldResults` and the `token`.
    case fieldsUpdateRequested(token: Int, fields: [FieldRequest])
    /// Revisions came, went or changed.
    case revisionsChanged
    /// Comments came, went or changed.
    case commentsChanged
    /// The visible comments' anchors, after layout, zoom or scroll.
    case commentAnchors([CommentAnchor])
    /// Where the SDK placed the attached balloons.
    case balloonsLaidOut([BalloonPlacement])
    /// The ruler view changed (coalesced per frame).
    case rulerChanged(RulerView)
    /// The editor switched between the template and a sub-template, or between sub-templates: where it is now.
    case editingTargetChanged(EditingTarget)
    /// Typing, pasting or dropping was refused: why, and the element concerned (the reference field whose
    /// sub-template's body shows).
    case editRefused(reason: RefusalReason, element: ElementID? = nil)
    /// Hover over a link, element mark, revision or comment; nil hides the tooltip.
    case tooltip(Tooltip?)
    /// The pointer moved onto another field, content control or merge-designer element, or the hovered one's boxes
    /// moved (at most once a frame); nil when it left.
    case elementHover(ElementHover?)
    /// An element was clicked.
    case elementClick(ElementClick)
    /// The selection moved into another element, or its boxes moved (at most once a frame); nil when it is in none.
    case elementSelect(ElementSelection?)
    /// What the editor said to screen readers: the announced `text`, and what it is about (when one change says
    /// several things, what the first is about).
    case announcement(text: String, kind: AnnouncementKind)
    /// A fidelity note from reading or writing.
    case warning(Warning)
    /// A fault: the engine failed (`code`, an English `message` for logs, structured `details` when there are
    /// any). With `fatal`, the session is unusable and rejects later operations with `internal`.
    case error(code: ErrorCode, message: String, details: JSONValue? = nil, fatal: Bool)

    public init(from decoder: any Decoder) throws {
        let t = try TaggedDecoder(decoder)
        switch t.tag {
        case "stateChanged": self = .stateChanged(try t.spread())
        case "documentChanged": self = .documentChanged(try t.spread())
        case "selectionChanged": self = .selectionChanged(try t.spread())
        case "linkActivated": self = .linkActivated(address: try t["address"], via: try t["via"])
        case "pictureControlActivated": self = .pictureControlActivated(try t.spread())
        case "contextMenu": self = .contextMenu(try t.spread())
        case "popoverRequested": self = .popoverRequested(try t.spread())
        case "popoverClosed": self = .popoverClosed
        case "dialogRequested": self = .dialogRequested(kind: try t["kind"])
        case "hostCommand": self = .hostCommand(command: try t["command"])
        case "shortcut": self = .shortcut(id: try t["id"])
        case "gesture": self = .gesture(try t.spread())
        case "wordAdded": self = .wordAdded(word: try t["word"], language: try t["language"])
        case "fontsChanged": self = .fontsChanged(try t.spread())
        case "elementActivated": self = .elementActivated(id: try t["id"])
        case "elementsChanged":
            self = .elementsChanged(added: try t["added"], removed: try t["removed"], changed: try t["changed"])
        case "fieldsUpdateRequested": self = .fieldsUpdateRequested(token: try t["token"], fields: try t["fields"])
        case "revisionsChanged": self = .revisionsChanged
        case "commentsChanged": self = .commentsChanged
        case "commentAnchors": self = .commentAnchors(try t.value())
        case "balloonsLaidOut": self = .balloonsLaidOut(try t.value())
        case "rulerChanged": self = .rulerChanged(try t.spread())
        case "editingTargetChanged": self = .editingTargetChanged(try t.spread())
        case "editRefused": self = .editRefused(reason: try t["reason"], element: try t.optional("element"))
        case "tooltip": self = .tooltip(try t.optional("value"))
        case "elementHover": self = .elementHover(try t.optional("value"))
        case "elementClick": self = .elementClick(try t.spread())
        case "elementSelect": self = .elementSelect(try t.optional("value"))
        case "announcement": self = .announcement(text: try t["text"], kind: try t["kind"])
        case "warning": self = .warning(try t.spread())
        case "error":
            self = .error(
                code: try t["code"], message: try t["message"], details: try t.optional("details"),
                fatal: try t["fatal"])
        default: throw t.unknown()
        }
    }

    public func encode(to encoder: any Encoder) throws {
        let t = TaggedEncoder(encoder)
        switch self {
        case .stateChanged(let v): try t.tag("stateChanged", spread: v)
        case .documentChanged(let v): try t.tag("documentChanged", spread: v)
        case .selectionChanged(let v): try t.tag("selectionChanged", spread: v)
        case .linkActivated(let address, let via): try t.tag("linkActivated", ["address": address, "via": via])
        case .pictureControlActivated(let v): try t.tag("pictureControlActivated", spread: v)
        case .contextMenu(let v): try t.tag("contextMenu", spread: v)
        case .popoverRequested(let v): try t.tag("popoverRequested", spread: v)
        case .popoverClosed: try t.tag("popoverClosed")
        case .dialogRequested(let kind): try t.tag("dialogRequested", ["kind": kind])
        case .hostCommand(let command): try t.tag("hostCommand", ["command": command])
        case .shortcut(let id): try t.tag("shortcut", ["id": id])
        case .gesture(let v): try t.tag("gesture", spread: v)
        case .wordAdded(let word, let language): try t.tag("wordAdded", ["word": word, "language": language])
        case .fontsChanged(let v): try t.tag("fontsChanged", spread: v)
        case .elementActivated(let id): try t.tag("elementActivated", ["id": id])
        case .elementsChanged(let added, let removed, let changed):
            try t.tag("elementsChanged", ["added": added, "removed": removed, "changed": changed])
        case .fieldsUpdateRequested(let token, let fields):
            try t.tag("fieldsUpdateRequested", ["token": token, "fields": fields])
        case .revisionsChanged: try t.tag("revisionsChanged")
        case .commentsChanged: try t.tag("commentsChanged")
        case .commentAnchors(let v): try t.tag("commentAnchors", value: v)
        case .balloonsLaidOut(let v): try t.tag("balloonsLaidOut", value: v)
        case .rulerChanged(let v): try t.tag("rulerChanged", spread: v)
        case .editingTargetChanged(let v): try t.tag("editingTargetChanged", spread: v)
        case .editRefused(let reason, let element): try t.tag("editRefused", ["reason": reason, "element": element])
        case .tooltip(let v): try t.tag("tooltip", value: v)
        case .elementHover(let v): try t.tag("elementHover", value: v)
        case .elementClick(let v): try t.tag("elementClick", spread: v)
        case .elementSelect(let v): try t.tag("elementSelect", value: v)
        case .announcement(let text, let kind): try t.tag("announcement", ["text": text, "kind": kind])
        case .warning(let v): try t.tag("warning", spread: v)
        case .error(let code, let message, let details, let fatal):
            try t.tag("error", ["code": code, "message": message, "details": details, "fatal": fatal])
        }
    }
}

/// Why typing, pasting or dropping was refused.
public enum RefusalReason: Hashable, Sendable, Codable {
    /// Into a sub-template's body shown in place of its reference: edit the sub-template `name` instead.
    case subTemplate(name: String)
    /// Into a control whose content is locked.
    case locked

    public init(from decoder: any Decoder) throws {
        let t = try TaggedDecoder(decoder, tag: "kind")
        switch t.tag {
        case "subTemplate": self = .subTemplate(name: try t["name"])
        case "locked": self = .locked
        default: throw t.unknown()
        }
    }

    public func encode(to encoder: any Encoder) throws {
        let t = TaggedEncoder(encoder, tag: "kind")
        switch self {
        case .subTemplate(let name): try t.tag("subTemplate", ["name": name])
        case .locked: try t.tag("locked")
        }
    }
}

/// How a link was activated.
public enum LinkVia: String, Hashable, Sendable, Codable, CaseIterable {
    /// Ctrl+click (Cmd+click on a Mac).
    case ctrlClick
    /// A plain click, in a read-only document.
    case click
}

/// A field whose result the application computes.
public struct FieldRequest: Hashable, Sendable, Codable, Identifiable {
    /// The field.
    public var id: ElementID
    /// Its field code (`DOCPROPERTY Title`).
    public var code: String

    /// A field request.
    public init(id: ElementID, code: String) {
        self.id = id
        self.code = code
    }
}

/// Keys whose UI is the application's: pressing one never edits, and emits `hostCommand`. `allCases` is in
/// declaration order, the order the C ABI numbers them from 0.
public enum HostCommand: String, Hashable, Sendable, Codable, CaseIterable {
    /// Ctrl+S.
    case save
    /// F12.
    case saveAs
    /// Ctrl+P.
    case print
    /// Ctrl+O.
    case open
    /// Ctrl+N.
    case new
    /// Ctrl+F.
    case find
    /// Ctrl+H.
    case replace
    /// Ctrl+G.
    case goTo
    /// Ctrl+K.
    case insertHyperlink
    /// F7.
    case nextMisspelling
    /// Ctrl+Shift+F.
    case focusFont
    /// Ctrl+Shift+P.
    case focusFontSize
    /// F1.
    case help
}

/// What a key press did.
public enum KeyOutcome: Hashable, Sendable, Codable {
    /// The editor used it: the host suppresses the platform's default.
    case handled
    /// A key the application owns (also emitted as `hostCommand`).
    case hostCommand(HostCommand)
    /// An application shortcut (also emitted as `shortcut`).
    case shortcut(String)
    /// Not the editor's: the platform keeps it.
    case unhandled

    public init(from decoder: any Decoder) throws {
        let t = try TaggedDecoder(decoder)
        switch t.tag {
        case "handled": self = .handled
        case "hostCommand": self = .hostCommand(try t.value())
        case "shortcut": self = .shortcut(try t.value())
        case "unhandled": self = .unhandled
        default: throw t.unknown()
        }
    }

    public func encode(to encoder: any Encoder) throws {
        let t = TaggedEncoder(encoder)
        switch self {
        case .handled: try t.tag("handled")
        case .hostCommand(let v): try t.tag("hostCommand", value: v)
        case .shortcut(let v): try t.tag("shortcut", value: v)
        case .unhandled: try t.tag("unhandled")
        }
    }
}
