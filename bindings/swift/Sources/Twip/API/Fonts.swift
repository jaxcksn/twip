import Foundation

// Font values (crates/twip/src/api/fonts.rs, and the font table types of crates/twip/src/docx/font_table.rs): the
// faces a font file adds, requests for missing faces and their answers, replacements, aliases and generic defaults.
// No renames from Rust.

/// A generic font category, like the CSS generic families.
public enum GenericFamily: String, Hashable, Sendable, Codable, CaseIterable {
    /// Fonts with serifs (`roman` in a document's font table).
    case serif
    /// Fonts without serifs (`swiss`).
    case sansSerif
    /// Fixed-pitch fonts (`modern`, or any font the document says is fixed pitch).
    case monospace
    /// Script and handwriting fonts (`script`).
    case script
    /// Decorative and display fonts (`decorative`).
    case decorative
}

/// A font table's general class of a family.
public enum FontClass: String, Hashable, Sendable, Codable, CaseIterable {
    /// Serif (`roman`).
    case roman
    /// Sans serif (`swiss`).
    case swiss
    /// Fixed pitch (`modern`).
    case modern
    /// Script (`script`).
    case script
    /// Decorative (`decorative`).
    case decorative
    /// Not stated (`auto`).
    case auto
}

/// A font table's pitch of a family.
public enum FontPitch: String, Hashable, Sendable, Codable, CaseIterable {
    /// Every character has the same width.
    case fixed
    /// Widths vary.
    case variable
    /// Not stated.
    case `default`
}

/// A document's description of a font family, from its font table (`w:font`). Each field is nil when the document
/// doesn't state it, or states a value this reader doesn't know.
public struct FontDescription: Hashable, Sendable, Codable {
    /// The general class (`w:family`).
    public var `class`: FontClass?
    /// The pitch (`w:pitch`).
    public var pitch: FontPitch?
    /// The PANOSE classification (`w:panose1`): 10 bytes, 20 hex digits on the wire.
    public var panose: [UInt8]?
    /// The character set (`w:charset`).
    public var charset: Int?

    enum CodingKeys: String, CodingKey {
        case `class`, pitch, panose, charset
    }

    /// A family's description.
    public init(class: FontClass? = nil, pitch: FontPitch? = nil, panose: [UInt8]? = nil, charset: Int? = nil) {
        self.class = `class`
        self.pitch = pitch
        self.panose = panose
        self.charset = charset
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        `class` = try c.decodeIfPresent(FontClass.self, forKey: .class)
        pitch = try c.decodeIfPresent(FontPitch.self, forKey: .pitch)
        charset = try c.decodeIfPresent(Int.self, forKey: .charset)
        // Like the engine, a PANOSE that isn't 20 hex digits reads as none.
        panose = nil
        if let hex = try c.decodeIfPresent(String.self, forKey: .panose), hex.utf8.count == 20 {
            let digits = Array(hex.utf8)
            let bytes = stride(from: 0, to: 20, by: 2).compactMap { UInt8(String(decoding: digits[$0..<$0 + 2], as: UTF8.self), radix: 16) }
            if bytes.count == 10 { panose = bytes }
        }
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encodeIfPresent(`class`, forKey: .class)
        try c.encodeIfPresent(pitch, forKey: .pitch)
        try c.encodeIfPresent(panose.map { $0.map { String(format: "%02X", $0) }.joined() }, forKey: .panose)
        try c.encodeIfPresent(charset, forKey: .charset)
    }
}

/// A face a font file added.
public struct FaceInfo: Hashable, Sendable, Codable {
    /// The family name the font file gives.
    public var family: String
    /// The face is bold.
    public var bold: Bool
    /// The face is italic.
    public var italic: Bool

    /// A face.
    public init(family: String, bold: Bool = false, italic: Bool = false) {
        self.family = family
        self.bold = bold
        self.italic = italic
    }
}

/// A face layout needs and the library doesn't have: what a resolver is asked for, once per family and style.
public struct FontRequest: Hashable, Sendable, Codable {
    /// The family, as the document names it.
    public var family: String
    /// A bold face is needed.
    public var bold: Bool
    /// An italic face is needed.
    public var italic: Bool
    /// The document's description of the family (its font table), when it has one.
    public var description: FontDescription?
    /// The generic category that description gives, whether or not classification is on.
    public var proposed: GenericFamily?

    /// A request for a face.
    public init(family: String, bold: Bool = false, italic: Bool = false, description: FontDescription? = nil, proposed: GenericFamily? = nil) {
        self.family = family
        self.bold = bold
        self.italic = italic
        self.description = description
        self.proposed = proposed
    }
}

/// A resolver's answer to a `FontRequest`. On the wire tagged by `kind`.
public enum FontAnswer: Hashable, Sendable, Codable {
    /// The font was added (to the library); the family is missing still if the font doesn't have it.
    case font
    /// Draw the family with this other family, as an alias for every style would, until aliases or replacements
    /// change.
    case family(String)
    /// Draw the family with the default of this generic category, as an alias to it would.
    case category(GenericFamily)
    /// Nothing provides it.
    case missing
    /// The resolver failed (it threw, a download failed, or the bytes weren't a font): what failed, as the report's
    /// `error` says it.
    case failed(message: String)

    public init(from decoder: any Decoder) throws {
        let t = try TaggedDecoder(decoder, tag: "kind")
        switch t.tag {
        case "font": self = .font
        case "family": self = .family(try t["family"])
        case "category": self = .category(try t["category"])
        case "missing": self = .missing
        case "failed": self = .failed(message: try t["message"])
        default: throw t.unknown()
        }
    }

    public func encode(to encoder: any Encoder) throws {
        let t = TaggedEncoder(encoder, tag: "kind")
        switch self {
        case .font: try t.tag("font")
        case .family(let family): try t.tag("family", ["family": family])
        case .category(let category): try t.tag("category", ["category": category])
        case .missing: try t.tag("missing")
        case .failed(let message): try t.tag("failed", ["message": message])
        }
    }
}

/// A draw-time replacement: `from` (optionally only its bold or italic faces) is drawn with `to`, even when `from`
/// is in the library. The document keeps naming `from`: saved files and the formatting a session reports are
/// unchanged. When `to` isn't in the library, `from` resolves as if there were no rule.
public struct FontRule: Hashable, Sendable, Codable {
    /// The family to replace, case-insensitive.
    public var from: String
    /// Only its bold (`true`) or regular-weight (`false`) faces; nil for both.
    public var bold: Bool?
    /// Only its italic (`true`) or upright (`false`) faces; nil for both.
    public var italic: Bool?
    /// The family that draws instead.
    public var to: String

    /// A replacement.
    public init(from: String, bold: Bool? = nil, italic: Bool? = nil, to: String) {
        self.from = from
        self.bold = bold
        self.italic = italic
        self.to = to
    }
}

/// An alias: when `from` (optionally only its bold or italic faces) isn't in the library, it is drawn with `to`.
/// Aliases chain (`A → B → C`) up to 8 hops. The document keeps naming `from`.
public struct Alias: Hashable, Sendable, Codable {
    /// The family the document names, case-insensitive.
    public var from: String
    /// Only its bold (`true`) or regular-weight (`false`) faces; nil for both.
    public var bold: Bool?
    /// Only its italic (`true`) or upright (`false`) faces; nil for both.
    public var italic: Bool?
    /// The family, or the generic category, that draws instead.
    public var to: AliasTarget

    /// An alias.
    public init(from: String, bold: Bool? = nil, italic: Bool? = nil, to: AliasTarget) {
        self.from = from
        self.bold = bold
        self.italic = italic
        self.to = to
    }
}

/// What an `Alias` draws with: another family, or the default family of a generic category (`GenericDefaults`). On
/// the wire the family's name, or `{"category": …}`.
public enum AliasTarget: Hashable, Sendable, Codable {
    /// Another family; its own aliases apply when it is missing too.
    case family(String)
    /// The default family of a generic category. When that default is unset or missing, the general fallback draws.
    case category(GenericFamily)

    public init(from decoder: any Decoder) throws {
        if let family = try? decoder.singleValueContainer().decode(String.self) {
            self = .family(family)
        } else {
            self = .category(try decoder.container(keyedBy: WireKey.self).decode(GenericFamily.self, forKey: "category"))
        }
    }

    public func encode(to encoder: any Encoder) throws {
        switch self {
        case .family(let family):
            var c = encoder.singleValueContainer()
            try c.encode(family)
        case .category(let category):
            var c = encoder.container(keyedBy: WireKey.self)
            try c.encode(category, forKey: "category")
        }
    }
}

/// The family that draws a missing font of each generic category. Opt-in: every category is unset by default, and
/// with none set no missing font is drawn through a category. A missing family's category comes from the document's
/// description of it (its font table), from an `Alias` to a category, or from a resolver's answer.
public struct GenericDefaults: Hashable, Sendable, Codable {
    /// The family for missing serif fonts.
    public var serif: String?
    /// The family for missing sans-serif fonts.
    public var sansSerif: String?
    /// The family for missing fixed-pitch fonts.
    public var monospace: String?
    /// The family for missing script fonts.
    public var script: String?
    /// The family for missing decorative fonts.
    public var decorative: String?

    /// The defaults; none set turns the feature off.
    public init(serif: String? = nil, sansSerif: String? = nil, monospace: String? = nil, script: String? = nil, decorative: String? = nil) {
        self.serif = serif
        self.sansSerif = sansSerif
        self.monospace = monospace
        self.script = script
        self.decorative = decorative
    }

    /// The family set for `category`.
    public subscript(category: GenericFamily) -> String? {
        switch category {
        case .serif: serif
        case .sansSerif: sansSerif
        case .monospace: monospace
        case .script: script
        case .decorative: decorative
        }
    }
}
