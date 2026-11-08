import CTwip
import Foundation
import os

/// The fonts sessions lay out and draw with. Share one per process: fonts added after sessions exist reach every
/// session that uses the library.
///
/// The setters apply on the engine thread before any operation called after them.
public final class FontLibrary: Sendable {
    let native: NativeEngine
    private let resolver = OSAllocatedUnfairLock<(@Sendable (FontRequest) async throws -> FontSource?)?>(initialState: nil)

    private init(system: Bool) {
        native = NativeEngine(fonts: system ? "system" : "empty")
        native.onFontRequest = { [unowned self] in onFontRequest($0) }
    }

    /// The system's fonts, shared per process.
    public static let system = FontLibrary(system: true)

    /// A new library with no fonts, for applications that supply every font.
    public static func empty() -> FontLibrary { FontLibrary(system: false) }

    /// Adds the fonts in a font file: TTF, OTF, TTC, WOFF or WOFF2.
    /// - Returns: The faces added.
    /// - Throws: ``ErrorCode/invalidArgument`` when the bytes hold no font.
    @discardableResult
    public func add(_ font: Data) async throws -> [FaceInfo] {
        try await EngineThread.shared.run { [native] _ in try Self.add(font, to: native) }
    }

    /// Adds the fonts in a font file: TTF, OTF, TTC, WOFF or WOFF2.
    /// - Returns: The faces added.
    @discardableResult
    public func add(contentsOf url: URL) async throws -> [FaceInfo] {
        try await add(try Data(contentsOf: url))
    }

    /// Every family the library can draw now, for a font box.
    public func families() async throws -> [String] {
        try await EngineThread.shared.run { [native] _ in try native.call(0, TWIP_FONT_FAMILIES, Data(), as: [String].self) ?? [] }
    }

    /// Sets the resolver: the engine asks it, once per family and style, for a font a document uses that the library
    /// doesn't have. Return the font's bytes or a URL to fetch, another family or a category to draw it with, or nil.
    /// While it runs, the family is pending and draws with a stand-in; a thrown error, a failed download or bytes that
    /// aren't a font report the family missing with the error.
    public func setResolver(_ resolve: @escaping @Sendable (FontRequest) async throws -> FontSource?) {
        resolver.withLock { $0 = resolve }
        send(TWIP_FONT_SET_RESOLVER, true)
    }

    /// Sets the draw-time replacements, replacing earlier ones: each family is drawn with another (Times New Roman
    /// with Tinos), even when the replaced family is in the library. Saved documents keep the original name.
    public func setReplacements(_ replacements: [String: String]) {
        setReplacements(replacements.sorted { $0.key < $1.key }.map { FontRule(from: $0.key, to: $0.value) })
    }

    /// Sets the draw-time replacements per style, replacing earlier ones.
    public func setReplacements(_ rules: [FontRule]) {
        send(TWIP_FONT_SET_REPLACEMENTS, rules)
    }

    /// Sets the aliases, replacing earlier ones: each family that isn't in the library is drawn with another (Arial
    /// with Arimo). Saved documents keep the original name.
    public func setAliases(_ aliases: [String: String]) {
        setAliases(aliases.sorted { $0.key < $1.key }.map { Alias(from: $0.key, to: .family($0.value)) })
    }

    /// Sets the aliases per style, or to a generic category, replacing earlier ones. An alias applies only when its
    /// family isn't in the library; aliases chain up to 8 hops.
    public func setAliases(_ aliases: [Alias]) {
        send(TWIP_FONT_SET_ALIASES, aliases)
    }

    /// Sets the default family of each generic category, for a missing family of that category. `GenericDefaults()`
    /// turns it off.
    public func setGenericDefaults(_ defaults: GenericDefaults) {
        send(TWIP_FONT_SET_GENERIC_DEFAULTS, defaults)
    }

    /// Sets whether a document's font table gives a missing family its generic category (on by default).
    public func setClassification(fromDocument: Bool) {
        send(TWIP_FONT_SET_CLASSIFICATION, fromDocument)
    }

    /// Sets the families searched, in order, for a character a run's font lacks, before the symbol fallback.
    public func setCharacterFallback(_ families: [String]) {
        send(TWIP_FONT_SET_CHARACTER_FALLBACK, families)
    }

    /// Sets the general fallback: the family that draws a font nothing else resolves. With nil, or when it isn't in
    /// the library, the first face added draws.
    public func setFallback(_ family: String?) {
        send(TWIP_FONT_SET_FALLBACK, family)
    }

    /// What draws the characters no other font has.
    public enum SymbolFallback: Sendable {
        /// The bundled symbol font (the default).
        case bundled
        /// Nothing: such characters draw as missing glyphs.
        case off
        /// A font file's first face.
        case font(Data)
    }

    /// Sets the face for characters no other font has, after the character-fallback families.
    /// - Throws: ``ErrorCode/invalidArgument`` when the font holds no face; the setting is unchanged.
    public func setSymbolFallback(_ fallback: SymbolFallback) async throws {
        try await EngineThread.shared.run { [native] _ in
            switch fallback {
            case .bundled: _ = try native.call(0, TWIP_FONT_SET_SYMBOL_FALLBACK, "bundled")
            case .off: _ = try native.call(0, TWIP_FONT_SET_SYMBOL_FALLBACK, "off")
            case .font(let data):
                var out = TwipBuf()
                let raw = try native.handle
                let status = withBytes(data) { twip_font_symbol_fallback(raw, $0, $1, &out) }
                _ = try take(&out, status, as: JSONValue.self)
            }
        }
    }

    private func send(_ method: TwipMethod, _ request: some Encodable & Sendable) {
        guard let body = try? Wire.encode(request) else { return }
        EngineThread.shared.post { [native] in _ = try? native.call(0, method, body) }
    }

    private static func add(_ font: Data, to native: NativeEngine) throws -> [FaceInfo] {
        var out = TwipBuf()
        let raw = try native.handle
        let status = withBytes(font) { twip_font_add(raw, $0, $1, &out) }
        return try take(&out, status, as: [FaceInfo].self) ?? []
    }

    /// On the engine thread: the engine needs a font. Runs the resolver in a task, then answers.
    private func onFontRequest(_ payload: Data) {
        struct Answer: Encodable { let request: FontRequest; let answer: FontAnswer }
        guard let request = try? Wire.decode(FontRequest.self, from: payload) else { return }
        let native = native
        let answer = { @Sendable (answer: FontAnswer, font: Data?) in
            EngineThread.shared.post {
                var answer = answer
                if let font {
                    do { _ = try Self.add(font, to: native) } catch { answer = .failed(message: "\(error)") }
                }
                _ = try? native.call(0, TWIP_FONT_ANSWER, Answer(request: request, answer: answer))
            }
        }
        guard let resolve = resolver.withLock({ $0 }) else { return answer(.missing, nil) }
        Task.detached {
            do {
                switch try await resolve(request) {
                case nil: answer(.missing, nil)
                case .family(let family): answer(.family(family), nil)
                case .category(let category): answer(.category(category), nil)
                case .data(let data): answer(.font, data)
                case .url(let url):
                    let data = url.isFileURL ? try Data(contentsOf: url) : try await URLSession.shared.data(from: url).0
                    answer(.font, data)
                }
            } catch is CancellationError {
                answer(.missing, nil)
            } catch {
                answer(.failed(message: "\(error)"), nil)
            }
        }
    }
}

/// A font resolver's answer.
public enum FontSource: Sendable {
    /// The font file's bytes.
    case data(Data)
    /// A font file to read or download.
    case url(URL)
    /// Draw the requested family with another, for every style.
    case family(String)
    /// Draw the requested family with the default of a generic category, for every style.
    case category(GenericFamily)
}
