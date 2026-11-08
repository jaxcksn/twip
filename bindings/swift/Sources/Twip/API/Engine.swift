import Foundation

// The engine's wire values (crates/twip/src/api/engine.rs): session ids, spelling dictionaries and how a session
// starts. Renames from Rust: SessionId → SessionID, Dictionary → SpellingDictionary.

/// A session's id in its engine. Ids are never reused. A number on the wire.
public struct SessionID: RawRepresentable, Hashable, Sendable, Codable, CustomStringConvertible {
    /// The number.
    public let rawValue: Int
    /// The session numbered `rawValue`.
    public init(rawValue: Int) { self.rawValue = rawValue }
    public var description: String { String(rawValue) }
}

/// A Hunspell dictionary.
public struct SpellingDictionary: Hashable, Sendable, Codable {
    /// BCP 47 (`en-US`).
    public var language: String
    /// The affix file (`.aff`).
    public var aff: Data
    /// The word list (`.dic`).
    public var dic: Data

    /// A dictionary for `language`.
    public init(language: String, aff: Data, dic: Data) {
        self.language = language
        self.aff = aff
        self.dic = dic
    }
}

/// How a session starts. Every field has a default, so `{}` is a valid wire value.
public struct SessionOptions: Hashable, Sendable, Codable {
    /// Who edits: tracked changes and comments are attributed to them.
    public var author: Author
    /// Starts read only: commands that change the document fail with `read-only`.
    public var readOnly: Bool
    /// Editing (the default) or template design.
    public var mode: Mode
    /// BCP 47; text with no language is checked in it.
    public var language: String
    /// Spelling dictionaries to load, one per language.
    public var dictionaries: [SpellingDictionary]
    /// Turns tracking on after each open, unless the document's protection decides.
    public var trackChanges: Bool
    /// What design mode shows for each field code, as `setFieldLabels` sets them, from the first layout of each open.
    public var fieldLabels: [String: String]
    /// The application's own field types, as `setApplicationFieldTypes` sets them, from each open.
    public var applicationFieldTypes: [String]
    /// Which fields are sub-template references, as `setSubTemplateRules` sets them, from each open.
    public var subTemplateRules: [ReferenceRule]

    /// How a session starts: by default as `Author`, editable, editing, in `en-US`, with no dictionaries and tracking
    /// off, and no field labels, application field types or sub-template rules.
    public init(
        author: Author = Author(name: "Author"), readOnly: Bool = false, mode: Mode = .editing, language: String = "en-US",
        dictionaries: [SpellingDictionary] = [], trackChanges: Bool = false, fieldLabels: [String: String] = [:],
        applicationFieldTypes: [String] = [], subTemplateRules: [ReferenceRule] = []
    ) {
        self.author = author
        self.readOnly = readOnly
        self.mode = mode
        self.language = language
        self.dictionaries = dictionaries
        self.trackChanges = trackChanges
        self.fieldLabels = fieldLabels
        self.applicationFieldTypes = applicationFieldTypes
        self.subTemplateRules = subTemplateRules
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        let d = SessionOptions()
        author = try c.decodeIfPresent(Author.self, forKey: .author) ?? d.author
        readOnly = try c.decodeIfPresent(Bool.self, forKey: .readOnly) ?? d.readOnly
        mode = try c.decodeIfPresent(Mode.self, forKey: .mode) ?? d.mode
        language = try c.decodeIfPresent(String.self, forKey: .language) ?? d.language
        dictionaries = try c.decodeIfPresent([SpellingDictionary].self, forKey: .dictionaries) ?? d.dictionaries
        trackChanges = try c.decodeIfPresent(Bool.self, forKey: .trackChanges) ?? d.trackChanges
        fieldLabels = try c.decodeIfPresent([String: String].self, forKey: .fieldLabels) ?? d.fieldLabels
        applicationFieldTypes = try c.decodeIfPresent([String].self, forKey: .applicationFieldTypes) ?? d.applicationFieldTypes
        subTemplateRules = try c.decodeIfPresent([ReferenceRule].self, forKey: .subTemplateRules) ?? d.subTemplateRules
    }
}
