import Foundation

// A session's wire values (crates/twip/src/api/session/*.rs): exports, commits, host requests, cursors, frames and
// the overlay's settings. No renames from Rust. Identity (Record.swift) and MarkupView (Review.swift) are defined with
// the api modules that re-export them.

// MARK: - Documents

/// The formats a document exports to.
public enum ExportFormat: String, Hashable, Sendable, Codable, CaseIterable {
    /// Office Open XML.
    case docx
    /// PDF, laid out completely first.
    case pdf
}

/// What to export: the format, and for a PDF what it writes beyond the pages. On the wire tagged by `format`.
public enum ExportOptions: Hashable, Sendable, Codable {
    /// Office Open XML.
    case docx
    /// PDF, laid out completely first.
    case pdf(PDFOptions = PDFOptions())

    /// The format written.
    public var format: ExportFormat {
        switch self {
        case .docx: .docx
        case .pdf: .pdf
        }
    }

    /// The format with default options.
    public init(_ format: ExportFormat) {
        self = format == .docx ? .docx : .pdf()
    }

    public init(from decoder: any Decoder) throws {
        let t = try TaggedDecoder(decoder, tag: "format")
        switch t.tag {
        case "docx": self = .docx
        case "pdf": self = .pdf(try t.spread())
        default: throw t.unknown()
        }
    }

    public func encode(to encoder: any Encoder) throws {
        let t = TaggedEncoder(encoder, tag: "format")
        switch self {
        case .docx: try t.tag("docx")
        case .pdf(let options): try t.tag("pdf", spread: options)
        }
    }
}

/// What a PDF writes beyond the pages. The default writes neither a password nor keywords. Its description never
/// shows the password.
public struct PDFOptions: Hashable, Sendable, Codable, CustomStringConvertible {
    /// Encrypts the PDF (AES-256): readers ask for this password before showing anything. Must not be empty.
    public var openPassword: String?
    /// Written to the PDF's keywords metadata, in order. Blank entries are dropped.
    @OmitEmpty public var keywords: [String]

    /// A PDF's options.
    public init(openPassword: String? = nil, keywords: [String] = []) {
        self.openPassword = openPassword
        self.keywords = keywords
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        openPassword = try c.decodeIfPresent(String.self, forKey: .openPassword)
        keywords = try c.decodeIfPresent([String].self, forKey: .keywords) ?? []
    }

    public var description: String {
        "PDFOptions(openPassword: \(openPassword == nil ? "nil" : "***"), keywords: \(keywords))"
    }
}

/// An exported document, the revision it captures, and what writing it couldn't keep.
public struct Export: Hashable, Sendable, Codable {
    /// The file. The C ABI returns it apart from the rest, as raw bytes.
    public var bytes: Data
    /// The state revision the file holds.
    public var revision: Int
    /// Fidelity notes from writing.
    public var warnings: [Warning]

    /// An exported document.
    public init(bytes: Data, revision: Int, warnings: [Warning] = []) {
        self.bytes = bytes
        self.revision = revision
        self.warnings = warnings
    }
}

// MARK: - Editing

/// What a command did: the transaction it committed (none for selection and view commands, or when nothing changed),
/// and what it made.
public struct Commit: Hashable, Sendable, Codable {
    /// The committed transaction, as `documentChanged` reported it.
    public var record: TransactionRecord?
    /// The command's result, for the commands that create something.
    public var value: CommandResult?

    /// What a command did.
    public init(record: TransactionRecord? = nil, value: CommandResult? = nil) {
        self.record = record
        self.value = value
    }
}

// MARK: - Hosting

/// A pointer shape the editor asks the host to show.
public enum Cursor: String, Hashable, Sendable, Codable, CaseIterable {
    /// The platform's arrow.
    case `default`
    /// The text I-beam.
    case text
    /// A pointing hand: over a link.
    case pointer
    /// Moving an object.
    case move
    /// Resizing left and right.
    case resizeHorizontal
    /// Resizing up and down.
    case resizeVertical
    /// Resizing along the top-left to bottom-right diagonal.
    case resizeNorthWestSouthEast
    /// Resizing along the top-right to bottom-left diagonal.
    case resizeNorthEastSouthWest
    /// A crosshair: drawing a shape.
    case crosshair
    /// What's under the pointer refuses the action.
    case notAllowed
    /// A drag that copies.
    case copy
    /// Selecting a table cell.
    case cellSelect
    /// Selecting a table column.
    case columnSelect
    /// Selecting a table row.
    case rowSelect
    /// Rotating an object.
    case rotate
}

/// Something only the host showing the session can do. Positions are content CSS px: they scroll with the document.
/// On the wire it is the `host_request` callback, tagged by `type` in snake_case.
public enum HostRequest: Hashable, Sendable, Codable {
    /// Show this pointer shape.
    case cursor(Cursor)
    /// Send every pointer event to the view until `release` (a drag started).
    case capture
    /// End the pointer capture.
    case release
    /// Put this on the system clipboard: a cut or copy from the keyboard.
    case clipboard(data: ClipboardData)
    /// The caret moved, for the IME window and the system caret: its left, top and height, and whether it shows (a
    /// collapsed selection shows a caret).
    case caret(x: Double, y: Double, height: Double, visible: Bool)
    /// Scroll so the viewport's top-left is at this point: a zoom keeps the caret's line in place. The engine already
    /// draws from there. The content takes the size the next frame reports, so a host scrolls after that frame.
    case scroll(x: Double, y: Double)
    /// Scroll so this rectangle shows.
    case scrollTo(x: Double, y: Double, width: Double, height: Double)
    /// The text around the caret and the selection in it changed: for the element with the keyboard focus, which a
    /// screen reader reads as the caret moves. It goes with the `caret` request.
    case caretText(window: CaretWindow)
    /// The accessibility tree changed: the whole tree, or the changes since the last `a11y` request. The first after
    /// attaching or opening is the whole tree; while edits keep coming, one goes at most every 250 ms.
    case a11y(update: A11yUpdate)

    public init(from decoder: any Decoder) throws {
        let t = try TaggedDecoder(decoder)
        switch t.tag {
        case "cursor": self = .cursor(try t["cursor"])
        case "capture": self = .capture
        case "release": self = .release
        case "clipboard": self = .clipboard(data: try t["data"])
        case "caret": self = .caret(x: try t["x"], y: try t["y"], height: try t["height"], visible: try t["visible"])
        case "scroll": self = .scroll(x: try t["x"], y: try t["y"])
        case "scroll_to": self = .scrollTo(x: try t["x"], y: try t["y"], width: try t["width"], height: try t["height"])
        case "caret_text": self = .caretText(window: try t["window"])
        case "a11y": self = .a11y(update: try t["update"])
        default: throw t.unknown()
        }
    }

    public func encode(to encoder: any Encoder) throws {
        let t = TaggedEncoder(encoder)
        switch self {
        case .cursor(let cursor): try t.tag("cursor", ["cursor": cursor])
        case .capture: try t.tag("capture")
        case .release: try t.tag("release")
        case .clipboard(let data): try t.tag("clipboard", ["data": data])
        case .caret(let x, let y, let height, let visible): try t.tag("caret", ["x": x, "y": y, "height": height, "visible": visible])
        case .scroll(let x, let y): try t.tag("scroll", ["x": x, "y": y])
        case .scrollTo(let x, let y, let width, let height): try t.tag("scroll_to", ["x": x, "y": y, "width": width, "height": height])
        case .caretText(let window): try t.tag("caret_text", ["window": window])
        case .a11y(let update): try t.tag("a11y", ["update": update])
        }
    }
}

/// What a host keeps about the frame it drew: the document's size and the caret, for scrollbars and the IME.
public struct FrameInfo: Hashable, Sendable, Codable {
    /// The page stack's width, CSS px.
    public var contentWidth: Double
    /// The page stack's height, CSS px.
    public var contentHeight: Double
    /// The caret's left, content CSS px.
    public var caretX: Double
    /// The caret's top, content CSS px.
    public var caretY: Double
    /// The caret's height, CSS px.
    public var caretHeight: Double
    /// A collapsed selection shows a caret.
    public var caretVisible: Bool
    /// The pages laid out so far.
    public var pages: Int

    /// A frame's facts.
    public init(
        contentWidth: Double = 0, contentHeight: Double = 0, caretX: Double = 0, caretY: Double = 0, caretHeight: Double = 0,
        caretVisible: Bool = false, pages: Int = 0
    ) {
        self.contentWidth = contentWidth
        self.contentHeight = contentHeight
        self.caretX = caretX
        self.caretY = caretY
        self.caretHeight = caretHeight
        self.caretVisible = caretVisible
        self.pages = pages
    }
}

/// The colours of what the editor draws over the document. Setting them wins over the chrome theme's; the defaults
/// are the light theme's with the default accent: lines and marks at least 3:1 against the page, and the selection a
/// light tint black text keeps at least 4.5:1 over. The caret is always black or white, whichever stands out more on
/// the page.
public struct OverlayPalette: Hashable, Sendable, Codable {
    /// The selection, blended under the content (multiply), so black text stays black over it. Default `#9DC7F4`,
    /// the accent's light tint.
    public var selection: RGBColor
    /// A selected object's outline and handles. Default `#0A74E4`, the accent.
    public var object: RGBColor
    /// The drop caret of a drag. Default `#62656C`.
    public var drop: RGBColor
    /// Formatting marks and text boundaries. Default `#0A74E4`.
    public var marks: RGBColor
    /// The markup pane's leader lines. Default `#62656C`.
    public var leaders: RGBColor

    /// A palette; what isn't given is the default.
    public init(
        selection: RGBColor = "#9DC7F4", object: RGBColor = "#0A74E4", drop: RGBColor = "#62656C", marks: RGBColor = "#0A74E4",
        leaders: RGBColor = "#62656C"
    ) {
        self.selection = selection
        self.object = object
        self.drop = drop
        self.marks = marks
        self.leaders = leaders
    }
}

/// The memory a session keeps its pages' drawing in: past it, what was drawn longest ago goes first.
public struct PaintBudget: Hashable, Sendable, Codable {
    /// The pages' recordings, bytes. Default 32 MB.
    public var recordings: Int

    /// A budget.
    public init(recordings: Int = 32 << 20) { self.recordings = recordings }
}

/// How the caret blinks.
public struct CaretBlink: Hashable, Sendable, Codable {
    /// A phase's length, ms (at least 100); nil is a steady caret. Default 530.
    public var intervalMs: Int?
    /// Blinking stops after this long without input, ms, the caret left visible; nil (the default) is never.
    public var stopAfterMs: Int?

    /// How the caret blinks.
    public init(intervalMs: Int? = 530, stopAfterMs: Int? = nil) {
        self.intervalMs = intervalMs
        self.stopAfterMs = stopAfterMs
    }
}

/// What the frames have cost so far: for benchmarks and tests. Counts run from the session's start.
public struct FrameStats: Hashable, Sendable, Codable {
    /// Frames drawn.
    public var frames: Int
    /// Page recordings made: an unchanged page is recorded once.
    public var pagesRecorded: Int
    /// Pages drawn again from their recordings.
    public var pagesReused: Int
    /// The page recordings kept now, bytes.
    public var recordingBytes: Int
    /// The last frame's time, ms.
    public var lastFrameMs: Double
    /// The 95th percentile of the last 120 frames' times, ms.
    public var p95FrameMs: Double

    /// Frame costs.
    public init(
        frames: Int = 0, pagesRecorded: Int = 0, pagesReused: Int = 0, recordingBytes: Int = 0, lastFrameMs: Double = 0,
        p95FrameMs: Double = 0
    ) {
        self.frames = frames
        self.pagesRecorded = pagesRecorded
        self.pagesReused = pagesReused
        self.recordingBytes = recordingBytes
        self.lastFrameMs = lastFrameMs
        self.p95FrameMs = p95FrameMs
    }
}
