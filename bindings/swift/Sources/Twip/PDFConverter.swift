import CTwip
import Foundation

/// The faces ``PDFConverter`` draws with, and how document font names map to them. Fixed once made, and safe to share
/// between tasks: make one and use it for every conversion. Unlike a ``FontLibrary``, which belongs to editor sessions
/// and changes over time, a font set never changes.
public final class PDFFontSet: @unchecked Sendable {
    /// How a font set is made.
    public struct Options: Sendable {
        /// Whether the system's fonts are in the set.
        public var includeSystemFonts: Bool
        /// Font files' bytes: TTF, OTF, TTC, WOFF or WOFF2.
        public var fonts: [Data]
        /// Draw-time replacements: each family drawn with another, even when it is in the set.
        public var replacements: [FontRule]
        /// Aliases: each family that isn't in the set drawn with another, or a category.
        public var aliases: [Alias]
        /// The default family of each generic category, for a missing family of that category.
        public var genericDefaults: GenericDefaults
        /// Whether a document's font table gives a missing family its generic category.
        public var classifyFromDocument: Bool
        /// The family that draws a font nothing else resolves.
        public var fallback: String?
        /// The families searched, in order, for a character a run's font lacks.
        public var characterFallback: [String]
        /// What draws the characters no other font has.
        public var symbolFallback: FontLibrary.SymbolFallback

        /// Options for a set; by default, the system's fonts and nothing else.
        public init(
            includeSystemFonts: Bool = true, fonts: [Data] = [], replacements: [FontRule] = [], aliases: [Alias] = [],
            genericDefaults: GenericDefaults = GenericDefaults(), classifyFromDocument: Bool = true, fallback: String? = nil,
            characterFallback: [String] = [], symbolFallback: FontLibrary.SymbolFallback = .bundled
        ) {
            self.includeSystemFonts = includeSystemFonts
            self.fonts = fonts
            self.replacements = replacements
            self.aliases = aliases
            self.genericDefaults = genericDefaults
            self.classifyFromDocument = classifyFromDocument
            self.fallback = fallback
            self.characterFallback = characterFallback
            self.symbolFallback = symbolFallback
        }
    }

    let raw: OpaquePointer
    /// The faces the set's font files added.
    public let faces: [FaceInfo]

    /// The system's fonts, made on first use and shared.
    public static let system: PDFFontSet = try! PDFFontSet(Options())

    /// Makes a font set.
    /// - Throws: ``ErrorCode/invalidArgument`` when a font doesn't read.
    public init(_ options: Options) throws {
        struct Wire: Encodable {
            let includeSystemFonts: Bool, fontFiles: [String], replacements: [FontRule], aliases: [Alias]
            let genericDefaults: GenericDefaults, classification: Bool, fallback: String?, characterFallback: [String]
            let symbolFallback: JSONValue
        }
        var fonts = options.fonts
        let symbol: JSONValue
        switch options.symbolFallback {
        case .bundled: symbol = .string("bundled")
        case .off: symbol = .string("off")
        case .font(let data):
            fonts.append(data)
            symbol = .object(["font": .number(Double(fonts.count - 1))])
        }
        let json = try Twip.Wire.encode(Wire(
            includeSystemFonts: options.includeSystemFonts, fontFiles: [], replacements: options.replacements,
            aliases: options.aliases, genericDefaults: options.genericDefaults, classification: options.classifyFromDocument,
            fallback: options.fallback, characterFallback: options.characterFallback, symbolFallback: symbol))
        var set: OpaquePointer?
        var out = TwipBuf()
        let status = Self.withPointers(fonts) { pointers, lengths in
            withBytes(json) { twip_pdf_fonts_new($0, $1, pointers, lengths, fonts.count, &set, &out) }
        }
        do {
            faces = try take(&out, status, as: [FaceInfo].self) ?? []
        } catch {
            if let set { twip_pdf_fonts_free(set) }
            throw error
        }
        guard let set else { throw TwipError(.internal, "no font set") }
        raw = set
    }

    deinit { twip_pdf_fonts_free(raw) }

    /// Every family the set can draw.
    public var families: [String] {
        get throws {
            var out = TwipBuf()
            let status = twip_pdf_fonts_families(raw, &out)
            return try take(&out, status, as: [String].self) ?? []
        }
    }

    /// Calls `body` with pointers to each font's bytes and their lengths, valid during the call.
    private static func withPointers<R>(_ fonts: [Data], _ body: (UnsafePointer<UnsafePointer<UInt8>?>?, UnsafePointer<Int>?) -> R) -> R {
        let buffers = fonts.map { d -> UnsafeMutableBufferPointer<UInt8> in
            let b = UnsafeMutableBufferPointer<UInt8>.allocate(capacity: max(d.count, 1))
            _ = b.initialize(from: d)
            return b
        }
        defer { buffers.forEach { $0.deallocate() } }
        let pointers: [UnsafePointer<UInt8>?] = buffers.map { UnsafePointer($0.baseAddress) }
        let lengths = fonts.map(\.count)
        return pointers.withUnsafeBufferPointer { p in lengths.withUnsafeBufferPointer { l in body(p.baseAddress, l.baseAddress) } }
    }
}

/// A finished conversion.
public struct PDFConversion: Sendable {
    /// The PDF.
    public var pdf: Data
    /// What the PDF couldn't keep exactly: the read's approximations, text drawn in another font, and what the options
    /// dropped.
    public var warnings: [Warning]
    /// The fonts the document named that the set doesn't have, and the characters no font in it has.
    public var missingFonts: [MissingFont]
    /// The characters no font in the set has.
    public var missingCharacters: [MissingCharacters]
}

/// Converts DOCX to PDF without an editor session: the final document, without revision marks or comments.
///
/// Conversions run on the cooperative thread pool, never on the editor's engine thread, and any number may run at once.
public enum PDFConverter {
    /// Converts a document.
    /// - Throws: ``ErrorCode/importFailed`` when the bytes aren't a readable DOCX, ``ErrorCode/unsupported`` for an
    ///   encrypted or binary document, ``ErrorCode/exportFailed`` when the font set has no faces, and
    ///   `CancellationError` when the task is cancelled.
    public static func convert(_ docx: Data, fonts: PDFFontSet = .system, options: PDFOptions = PDFOptions()) async throws -> PDFConversion {
        try Task.checkCancellation()
        struct Meta: Decodable {
            struct Fonts: Decodable { let missing: [MissingFont]; let characters: [MissingCharacters] }
            let warnings: [Warning]
            let fonts: Fonts
        }
        let json = try Wire.encode(options)
        guard let cancel = twip_pdf_cancel_new() else { throw TwipError(.internal, "no cancel flag") }
        defer { twip_pdf_cancel_free(cancel) }
        let flag = UnsafeSendable(cancel)
        return try await withTaskCancellationHandler {
            let (status, pdf, meta) = await Task.detached { () -> (Int32, Data, UnsafeSendable<TwipBuf>) in
                var pdf = TwipBuf(), meta = TwipBuf()
                let status = withBytes(docx) { d, dn in
                    withBytes(json) { twip_pdf_convert(d, dn, fonts.raw, $0, $1, flag.value, &pdf, &meta) }
                }
                return (status, takeBytes(&pdf), UnsafeSendable(meta))
            }.value
            var buf = meta.value
            do {
                guard let info = try take(&buf, status, as: Meta.self) else { throw TwipError(.internal, "the conversion returned no result") }
                return PDFConversion(pdf: pdf, warnings: info.warnings, missingFonts: info.fonts.missing, missingCharacters: info.fonts.characters)
            } catch let error as TwipError where error.code == .aborted {
                throw CancellationError()
            }
        } onCancel: {
            twip_pdf_cancel_set(flag.value)
        }
    }

    /// Converts a DOCX file into a PDF file, replaced atomically: a failed conversion leaves an existing file as it was.
    /// - Returns: The conversion's warnings.
    @discardableResult
    public static func convert(contentsOf docx: URL, to pdf: URL, fonts: PDFFontSet = .system, options: PDFOptions = PDFOptions()) async throws -> [Warning] {
        let result = try await convert(try Data(contentsOf: docx), fonts: fonts, options: options)
        try result.pdf.write(to: pdf, options: .atomic)
        return result.warnings
    }
}

/// A value the compiler can't prove sendable, passed where only one task uses it at a time.
struct UnsafeSendable<T>: @unchecked Sendable {
    let value: T
    init(_ value: T) { self.value = value }
}
