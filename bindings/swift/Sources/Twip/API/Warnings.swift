import Foundation

// Warnings (crates/twip/src/api/warning.rs, from crates/twip/src/docx/diagnostics.rs): what reading or writing a
// document couldn't keep or make sense of, one per code and part with how many times it happened. Recoverable
// problems never fail a read. No renames from Rust.

/// The kinds of problem a read or a write reports. Each has a stable name on the wire (`unsupported-element`).
public enum WarningCode: String, Hashable, Sendable, Codable, CaseIterable {
    /// An element the reader doesn't keep: it was left out, counted per element name.
    case unsupportedElement = "unsupported-element"
    /// An image whose data isn't a format the engine decodes: it draws as an empty box.
    case imageUndecodable = "image-undecodable"
    /// A font family the library can't draw as the document asks: a stand-in draws it.
    case fontUnresolved = "font-unresolved"
    /// Content a write had to rewrite, losing bookmarks, comments or hyperlinks inside it.
    case rewrittenContent = "rewritten-content"
    /// Text a PDF export drew with a fallback font.
    case pdfFontFallback = "pdf-font-fallback"
    /// A password-protected PDF, which can't claim PDF/A conformance: the claim was left out.
    case pdfArchivalClaimDropped = "pdf-archival-claim-dropped"
    /// A revision without a `w:id`: it was given a new one.
    case revisionIDMissing = "revision-id-missing"
    /// A comment or move range end with no start, or a start with no end.
    case rangeUnpaired = "range-unpaired"
    /// A revision or comment with no author: it was attributed to an empty name.
    case authorMissing = "author-missing"
    /// A comment no range or reference in the document points to.
    case commentUnanchored = "comment-unanchored"
    /// A comment reference or range naming a comment the comments part doesn't have.
    case commentReferenceOrphan = "comment-reference-orphan"
    /// A shape's preset geometry the engine has no definition for: it draws as a rectangle and is kept on save.
    case unknownPreset = "unknown-preset"
    /// A glossary building block (or the glossary part) that couldn't be read: it is kept as written.
    case glossaryUnreadable = "glossary-unreadable"
    /// Two building blocks with one name in one category: the first is the one a name finds.
    case duplicateBuildingBlock = "duplicate-building-block"
    /// A sub-template reference inside a body already shown around it: it shows its field.
    case subTemplateCycle = "sub-template-cycle"
    /// A sub-template reference more than 16 sub-templates deep: it shows its field.
    case subTemplateTooDeep = "sub-template-too-deep"
    /// A sub-template reference whose name matches a sub-template only in another case: it doesn't resolve.
    case subTemplateNearMatch = "sub-template-near-match"
    /// A picture bullet whose image isn't a format the engine decodes: it draws as an empty box.
    case pictureBulletUndecodable = "picture-bullet-undecodable"
}

/// Where a warning was first found.
public struct Location: Hashable, Sendable, Codable {
    /// The package part (`word/document.xml`, `word/comments.xml`, ...).
    public var part: String
    /// The paragraph's index in the part's body, when the warning is about one.
    public var paragraph: Int?

    /// A place in a part.
    public init(part: String, paragraph: Int? = nil) {
        self.part = part
        self.paragraph = paragraph
    }
}

/// One kind of problem in one part, with how many times it happened.
public struct Warning: Hashable, Sendable, Codable {
    /// The kind of problem.
    public var code: WarningCode
    /// The first occurrence's description.
    public var message: String
    /// How many times it happened.
    public var count: Int
    /// Where it was first found, when it is in a part.
    public var location: Location?

    /// A warning.
    public init(code: WarningCode, message: String, count: Int = 1, location: Location? = nil) {
        self.code = code
        self.message = message
        self.count = count
        self.location = location
    }
}
