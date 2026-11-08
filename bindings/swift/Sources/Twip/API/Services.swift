import Foundation

// The host services' models from crates/twip/src/api/services.rs: find, spelling, navigation, styles, hit testing,
// tooltips, AutoCorrect and word count. Elements, accessibility, gestures, rulers and the view live beside it in
// Elements.swift, Accessibility.swift, Gestures.swift, Ruler.swift and View.swift.

// MARK: - Find, spelling, navigation

/// What to find.
public struct FindQuery: Hashable, Sendable, Codable {
    /// The text.
    public var text: String
    /// Match case (off when unset).
    public var matchCase: Bool?
    /// Whole words only (off when unset).
    public var wholeWords: Bool?

    /// A query for `text`.
    public init(text: String, matchCase: Bool? = nil, wholeWords: Bool? = nil) {
        self.text = text
        self.matchCase = matchCase
        self.wholeWords = wholeWords
    }
}

/// Which way `find` searches.
public typealias FindDirection = Order

/// Whether a match was found, and whether the search wrapped past the end.
public struct FindResult: Hashable, Sendable, Codable {
    /// A match was found and selected.
    public var found: Bool
    /// The search wrapped around the document's end.
    public var wrapped: Bool

    /// A find result.
    public init(found: Bool, wrapped: Bool) {
        self.found = found
        self.wrapped = wrapped
    }
}

/// One match with its context, for a results list.
public struct FindMatch: Hashable, Sendable, Codable, Identifiable {
    /// The match, for selecting it.
    public var id: MatchID
    /// Some text before it.
    public var before: String
    /// The matched text.
    public var text: String
    /// Some text after it.
    public var after: String
    /// Its page.
    public var page: Int

    /// A match.
    public init(id: MatchID, before: String, text: String, after: String, page: Int) {
        self.id = id
        self.before = before
        self.text = text
        self.after = after
        self.page = page
    }
}

/// A misspelled word and what to replace it with.
public struct Misspelling: Hashable, Sendable, Codable {
    /// The word.
    public var word: String
    /// Replacements, best first.
    public var suggestions: [String]

    /// A misspelling.
    public init(word: String, suggestions: [String] = []) {
        self.word = word
        self.suggestions = suggestions
    }
}

/// A heading, for a navigation pane.
public struct Heading: Hashable, Sendable, Codable, Identifiable {
    /// The heading.
    public var id: HeadingID
    /// Its outline level, from 1.
    public var level: Int
    /// Its text.
    public var text: String
    /// Its page.
    public var page: Int

    /// A heading.
    public init(id: HeadingID, level: Int, text: String, page: Int) {
        self.id = id
        self.level = level
        self.text = text
        self.page = page
    }
}

// MARK: - Styles

/// The kinds of style.
public enum StyleType: String, Hashable, Sendable, Codable, CaseIterable {
    /// A paragraph style.
    case paragraph
    /// A character style.
    case character
    /// A table style.
    case table
    /// A list (numbering) style.
    case list
}

/// Whether a style shows in style lists.
public enum StyleVisibility: String, Hashable, Sendable, Codable, CaseIterable {
    /// Shown.
    case show
    /// Hidden until a paragraph or run uses it (`w:semiHidden` with `w:unhideWhenUsed`).
    case hideUntilUsed
    /// Hidden (`w:semiHidden`).
    case hide
}

/// How a style gallery tile draws.
public struct StylePreview: Hashable, Sendable, Codable {
    /// The font name.
    public var font: String
    /// The size.
    public var size: Points
    /// Bold.
    public var bold: Bool
    /// Italic.
    public var italic: Bool
    /// The text color.
    public var color: AutoOr<RGBColor>

    /// A preview; by default 11 pt, regular, in the automatic colour.
    public init(font: String = "", size: Points = 11, bold: Bool = false, italic: Bool = false, color: AutoOr<RGBColor> = .auto) {
        self.font = font
        self.size = size
        self.bold = bold
        self.italic = italic
        self.color = color
    }
}

/// A style, for a style gallery or list.
public struct StyleInfo: Hashable, Sendable, Codable, Identifiable {
    /// The style.
    public var id: StyleID
    /// Its display name.
    public var name: String
    /// Paragraph, character or table (`type` on the wire).
    public var kind: StyleType
    /// Built in.
    public var builtIn: Bool
    /// Shown in a style gallery (`w:qFormat`).
    public var quick: Bool
    /// The sort priority.
    public var priority: Int
    /// Hidden from style lists.
    public var hidden: Bool
    /// The style it is based on.
    public var basedOn: StyleID?
    /// For drawing a gallery tile.
    public var preview: StylePreview
    /// Its other names (`w:name` holds them after the first, comma-separated).
    @OmitEmpty public var aliases: [String]
    /// The linked character or paragraph style.
    public var link: StyleID?
    /// Whether it shows in style lists.
    public var visibility: StyleVisibility
    /// Restricted: unavailable while the document's formatting is protected.
    public var locked: Bool

    /// A style.
    public init(
        id: StyleID, name: String, kind: StyleType = .paragraph, builtIn: Bool = false, quick: Bool = false,
        priority: Int = 0, hidden: Bool = false, basedOn: StyleID? = nil, preview: StylePreview = StylePreview(),
        aliases: [String] = [], link: StyleID? = nil, visibility: StyleVisibility = .show, locked: Bool = false
    ) {
        self.id = id
        self.name = name
        self.kind = kind
        self.builtIn = builtIn
        self.quick = quick
        self.priority = priority
        self.hidden = hidden
        self.basedOn = basedOn
        self.preview = preview
        self.aliases = aliases
        self.link = link
        self.visibility = visibility
        self.locked = locked
    }

    enum CodingKeys: String, CodingKey {
        case id, name, kind = "type", builtIn, quick, priority, hidden, basedOn, preview, aliases, link, visibility, locked
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(StyleID.self, forKey: .id)
        name = try c.decode(String.self, forKey: .name)
        kind = try c.decode(StyleType.self, forKey: .kind)
        builtIn = try c.decode(Bool.self, forKey: .builtIn)
        quick = try c.decode(Bool.self, forKey: .quick)
        priority = try c.decode(Int.self, forKey: .priority)
        hidden = try c.decode(Bool.self, forKey: .hidden)
        basedOn = try c.decodeIfPresent(StyleID.self, forKey: .basedOn)
        preview = try c.decode(StylePreview.self, forKey: .preview)
        aliases = try c.decodeIfPresent([String].self, forKey: .aliases) ?? []
        link = try c.decodeIfPresent(StyleID.self, forKey: .link)
        visibility = try c.decode(StyleVisibility.self, forKey: .visibility)
        locked = try c.decode(Bool.self, forKey: .locked)
    }
}

// MARK: - Hit testing, tooltips, word count

/// What is under a point.
public struct HitResult: Hashable, Sendable, Codable {
    /// The innermost field or content control there.
    public var element: ElementID?
    /// Text there can be edited.
    public var editable: Bool

    /// A hit result.
    public init(element: ElementID? = nil, editable: Bool = false) {
        self.element = element
        self.editable = editable
    }
}

/// What a tooltip is about.
public enum TooltipKind: String, Hashable, Sendable, Codable, CaseIterable {
    /// A hyperlink.
    case link
    /// A marked range.
    case mark
    /// A tracked change.
    case revision
    /// A comment.
    case comment
    /// A ruler marker, the tab type box, or where a click on the ruler adds a stop.
    case ruler
}

/// A tooltip to show over the document or its rulers.
public struct Tooltip: Hashable, Sendable, Codable {
    /// What is hovered.
    public var kind: TooltipKind
    /// The text to show.
    public var text: String
    /// The pointer.
    public var at: ClientPoint
    /// The hovered thing's box.
    public var rect: ClientRect

    /// A tooltip.
    public init(kind: TooltipKind, text: String, at: ClientPoint, rect: ClientRect) {
        self.kind = kind
        self.text = text
        self.at = at
        self.rect = rect
    }
}

/// The document's counts, computed on request.
public struct WordCount: Hashable, Sendable, Codable {
    /// Words.
    public var words: Int
    /// Characters, without spaces.
    public var characters: Int
    /// Characters, with spaces.
    public var charactersWithSpaces: Int
    /// Paragraphs.
    public var paragraphs: Int
    /// Lines.
    public var lines: Int
    /// Pages.
    public var pages: Int

    /// The counts.
    public init(words: Int = 0, characters: Int = 0, charactersWithSpaces: Int = 0, paragraphs: Int = 0, lines: Int = 0, pages: Int = 0) {
        self.words = words
        self.characters = characters
        self.charactersWithSpaces = charactersWithSpaces
        self.paragraphs = paragraphs
        self.lines = lines
        self.pages = pages
    }
}

// MARK: - AutoCorrect

/// What typing corrects (AutoCorrect): each rule on or off, and the texts never corrected again.
public struct AutoCorrectOptions: Hashable, Sendable, Codable {
    /// THe → The.
    public var twoInitialCapitals: Bool
    /// The first letter of a sentence.
    public var sentenceCapitals: Bool
    /// The first letter of a table cell.
    public var cellCapitals: Bool
    /// monday → Monday.
    public var dayNames: Bool
    /// The replacement list and the symbols ((c) → ©).
    public var replaceText: Bool
    /// Straight quotes to curly ones.
    public var smartQuotes: Bool
    /// 1st with the suffix raised.
    public var ordinals: Bool
    /// 1/2 → ½.
    public var fractions: Bool
    /// Hyphens to en and em dashes.
    public var dashes: Bool
    /// `1.` or `1)` then a space or tab at a paragraph's start makes it a numbered list.
    public var numberedLists: Bool
    /// `*` or `-` then a space or tab at a paragraph's start makes it a bulleted list.
    public var bulletedLists: Bool
    /// Texts Never correct added: never corrected again.
    @OmitEmpty public var never: [String]

    /// The options; by default every rule is on.
    public init(
        twoInitialCapitals: Bool = true, sentenceCapitals: Bool = true, cellCapitals: Bool = true, dayNames: Bool = true,
        replaceText: Bool = true, smartQuotes: Bool = true, ordinals: Bool = true, fractions: Bool = true,
        dashes: Bool = true, numberedLists: Bool = true, bulletedLists: Bool = true, never: [String] = []
    ) {
        self.twoInitialCapitals = twoInitialCapitals
        self.sentenceCapitals = sentenceCapitals
        self.cellCapitals = cellCapitals
        self.dayNames = dayNames
        self.replaceText = replaceText
        self.smartQuotes = smartQuotes
        self.ordinals = ordinals
        self.fractions = fractions
        self.dashes = dashes
        self.numberedLists = numberedLists
        self.bulletedLists = bulletedLists
        self.never = never
    }

    enum CodingKeys: String, CodingKey {
        case twoInitialCapitals, sentenceCapitals, cellCapitals, dayNames, replaceText, smartQuotes, ordinals, fractions
        case dashes, numberedLists, bulletedLists, never
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        twoInitialCapitals = try c.decode(Bool.self, forKey: .twoInitialCapitals)
        sentenceCapitals = try c.decode(Bool.self, forKey: .sentenceCapitals)
        cellCapitals = try c.decode(Bool.self, forKey: .cellCapitals)
        dayNames = try c.decode(Bool.self, forKey: .dayNames)
        replaceText = try c.decode(Bool.self, forKey: .replaceText)
        smartQuotes = try c.decode(Bool.self, forKey: .smartQuotes)
        ordinals = try c.decode(Bool.self, forKey: .ordinals)
        fractions = try c.decode(Bool.self, forKey: .fractions)
        dashes = try c.decode(Bool.self, forKey: .dashes)
        numberedLists = try c.decode(Bool.self, forKey: .numberedLists)
        bulletedLists = try c.decode(Bool.self, forKey: .bulletedLists)
        never = try c.decodeIfPresent([String].self, forKey: .never) ?? []
    }
}

/// `setAutoCorrect`'s change: the rules given switch, the lists given replace the shipped ones.
public struct AutoCorrectChange: Hashable, Sendable, Codable {
    /// THe → The.
    public var twoInitialCapitals: Bool?
    /// The first letter of a sentence.
    public var sentenceCapitals: Bool?
    /// The first letter of a table cell.
    public var cellCapitals: Bool?
    /// Day names.
    public var dayNames: Bool?
    /// The replacement list and the symbols.
    public var replaceText: Bool?
    /// Smart quotes.
    public var smartQuotes: Bool?
    /// Ordinals.
    public var ordinals: Bool?
    /// Fractions.
    public var fractions: Bool?
    /// Dashes.
    public var dashes: Bool?
    /// Automatic numbered lists.
    public var numberedLists: Bool?
    /// Automatic bulleted lists.
    public var bulletedLists: Bool?
    /// The replacement list, in place of the shipped one.
    public var entries: [AutoCorrectEntry]?
    /// Abbreviations after which a sentence doesn't start (`e.g.`), in place of the shipped ones.
    public var firstLetterExceptions: [String]?
    /// Words whose two initial capitals stay (`IDs`), in place of the shipped ones.
    public var initialCapsExceptions: [String]?
    /// The texts never corrected.
    public var never: [String]?

    /// A change; what is left nil stays as it is.
    public init(
        twoInitialCapitals: Bool? = nil, sentenceCapitals: Bool? = nil, cellCapitals: Bool? = nil, dayNames: Bool? = nil,
        replaceText: Bool? = nil, smartQuotes: Bool? = nil, ordinals: Bool? = nil, fractions: Bool? = nil,
        dashes: Bool? = nil, numberedLists: Bool? = nil, bulletedLists: Bool? = nil, entries: [AutoCorrectEntry]? = nil,
        firstLetterExceptions: [String]? = nil, initialCapsExceptions: [String]? = nil, never: [String]? = nil
    ) {
        self.twoInitialCapitals = twoInitialCapitals
        self.sentenceCapitals = sentenceCapitals
        self.cellCapitals = cellCapitals
        self.dayNames = dayNames
        self.replaceText = replaceText
        self.smartQuotes = smartQuotes
        self.ordinals = ordinals
        self.fractions = fractions
        self.dashes = dashes
        self.numberedLists = numberedLists
        self.bulletedLists = bulletedLists
        self.entries = entries
        self.firstLetterExceptions = firstLetterExceptions
        self.initialCapsExceptions = initialCapsExceptions
        self.never = never
    }
}

/// One replacement-list entry.
public struct AutoCorrectEntry: Hashable, Sendable, Codable {
    /// What's typed.
    public var from: String
    /// What it becomes.
    public var to: String

    /// An entry replacing `from` with `to`.
    public init(from: String, to: String) {
        self.from = from
        self.to = to
    }
}
