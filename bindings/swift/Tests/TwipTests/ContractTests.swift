import CTwip
import CoreGraphics
import Foundation
import Testing
@testable import Twip

/// The shared contract scripts in `tests/contract` (format in its README), on an unattached session
/// (`swift-unattached`) and on one presented by a headless host (`swift-attached`). Scripts listed in `pending.txt`
/// pass while they fail, and fail once they pass.
@Suite struct ContractTests {
    static let dir = Repo.path("tests", "contract")

    static let scripts: [String] = (try? FileManager.default.contentsOfDirectory(atPath: dir.path))?
        .filter { $0.hasSuffix(".json") }.sorted() ?? []

    static let pending: Set<String> = Set(
        ((try? String(contentsOf: dir.appendingPathComponent("pending.txt"), encoding: .utf8)) ?? "")
            .split(separator: "\n").map { $0.trimmingCharacters(in: .whitespaces) }
            .filter { !$0.isEmpty && !$0.hasPrefix("#") })

    @Test(arguments: ["swift-unattached", "swift-attached"], scripts)
    func scriptPasses(_ runner: String, _ file: String) async throws {
        let data = try Data(contentsOf: Self.dir.appendingPathComponent(file))
        let script = try #require(try JSONSerialization.jsonObject(with: data) as? [String: Any])
        let name = String(file.dropLast(5))
        let attached = runner == "swift-attached"
        // An unattached session takes no input.
        if !attached, (script["steps"] as? [Any] ?? []).contains(where: Script.takesInput) { return }
        let run = Script(runner: runner, name: name, script: script, attached: attached)
        guard Self.pending.contains(name) else { return try await run.run() }
        do { try await run.run() } catch { return } // pending: it fails until its feature lands
        Issue.record("\(runner): \(name): passes, but pending.txt lists it: delete its line")
    }
}

struct ScriptFailure: Error, CustomStringConvertible {
    let description: String
}

final class Script: @unchecked Sendable {
    let runner: String, name: String, script: [String: Any], attached: Bool
    private let lock = NSLock()
    private var events: [[String: Any]] = []
    private var background: [Task<Void, Never>] = []
    private var listeners: [(type: String, command: Any, at: String)] = []
    private var inconsistent: String?
    private var listenerFailure: String?
    private var saved: [String: Any] = [:]
    private var dialogs: [String: DialogModel] = [:]
    private var session: EditorSession!
    private var host: EditorHost?
    private var testHost: TestHost?
    private var at = ""
    private var failed: (error: any Error, at: String)?
    private var abort: String?
    private var exported: Data?

    init(runner: String, name: String, script: [String: Any], attached: Bool) {
        self.runner = runner
        self.name = name
        self.script = script
        self.attached = attached
    }

    static func takesInput(_ step: Any) -> Bool {
        guard let step = step as? [String: Any] else { return false }
        switch step["op"] as? String {
        case "key", "text", "click": return true
        case "concurrent": return (step["steps"] as? [Any] ?? []).contains(where: takesInput)
        case "repeat": return takesInput(step["step"] as Any)
        default: return false
        }
    }

    private static func fixture(_ file: String) -> URL { Repo.path("crates", "twip", "tests", "fixtures", file) }

    func run() async throws {
        let fonts = script["fonts"] as? String == "empty" ? FontLibrary.empty() : .system
        session = EditorSession(fonts: fonts)
        if attached {
            let t = TestHost()
            testHost = t
            host = try EditorHost(session: session, delegate: t)
            host!.resize(width: 800, height: 600, scale: 1)
        }
        _ = session.subscribe { [weak self] note in self?.observe(note) }
        defer { host?.detach() }
        do {
            for (i, step) in (script["steps"] as? [[String: Any]] ?? []).enumerated() {
                try await step_(step, at: "step \(i) (\(step["op"] as? String ?? "?"))")
            }
            if let f = failed { try fail(f.at, "failed: \(Self.describe(f.error))") }
            await idle()
            try checkEvents()
        } catch {
            await session.close()
            throw error
        }
        await session.close()
    }

    private func step_(_ raw: [String: Any], at: String) async throws {
        self.at = at
        let op = raw["op"] as? String ?? ""
        if let f = failed, op != "expectError" { try fail(f.at, "failed: \(Self.describe(f.error))") }
        let step = op == "repeat" ? raw : try resolve(raw) as! [String: Any]
        if op.hasPrefix("expect") {
            await idle()
            try checkEvents()
        }
        switch op {
        case "expectState":
            let state = try Self.json(session.state)
            if let why = Match.mismatch(step["match"] as Any, state, "state") { try fail(at, why) }
        case "expectEvents":
            let since = lock.withLock { () -> [[String: Any]] in
                defer { events.removeAll() }
                return events
            }
            if let why = Match.subsequence(step["match"] as? [Any] ?? [], since) { try fail(at, why) }
        case "expectError":
            let want = step["code"] as? String ?? ""
            let got = failed.map { Self.code($0.error) } ?? "no error"
            if got != want {
                try fail(at, "expected the previous step to fail \(want), it \(failed.map { "failed: \(Self.describe($0.error))" } ?? "succeeded")")
            }
            failed = nil
        case "abort":
            abort = step["late"] as? Bool == true ? "late" : "early"
        case "listen":
            lock.withLock { listeners.append((step["type"] as? String ?? "", step["command"] as Any, at)) }
        case "repeat":
            for i in 0..<(step["count"] as? Int ?? 0) {
                try await step_(step["step"] as? [String: Any] ?? [:], at: "\(at), repeat \(i)")
            }
        case "concurrent":
            let steps = step["steps"] as? [[String: Any]] ?? []
            var tasks: [Task<Void, any Error>] = []
            for (i, s) in steps.enumerated() {
                let s = UnsafeSendable(s)
                // Each starts at once, so the session queues them in this order.
                if #available(macOS 26, *) {
                    tasks.append(Task.immediate { [self] in try await start(s.value, at: "\(at).\(i)") })
                } else {
                    try await start(s.value, at: "\(at).\(i)")
                }
            }
            do {
                for t in tasks { try await t.value }
            } catch let e as ScriptFailure {
                throw e
            } catch {
                failed = (error, at)
            }
        default:
            do { try await start(step, at: at) } catch let e as ScriptFailure { throw e } catch { failed = (error, at) }
        }
    }

    /// Waits until the session is idle, including the commands that listeners submitted, and its layout is complete.
    private func idle() async {
        var n = -1
        while true {
            let pending = lock.withLock { background }
            if pending.count == n { break }
            n = pending.count
            for t in pending { await t.value }
            await session.idle()
        }
        while !session.isClosed && !session.state.pages.complete {
            try? await Task.sleep(for: .milliseconds(5))
            await session.idle()
        }
    }

    private func checkEvents() throws {
        if let why = lock.withLock({ inconsistent ?? listenerFailure }) { try fail(at, why) }
    }

    /// The next operation, cancelled before it runs ("early"), or only after it settles ("late").
    private func withAbort(_ op: @escaping @Sendable () async throws -> Void) async throws {
        let how = abort
        abort = nil
        switch how {
        case "early":
            try await Task {
                withUnsafeCurrentTask { $0?.cancel() }
                try await op()
            }.value
        case "late":
            let task = Task { try await op() }
            defer { task.cancel() }
            try await task.value
        default:
            try await op()
        }
    }

    private func start(_ step: [String: Any], at: String) async throws {
        let step = UnsafeSendable(step).value
        switch step["op"] as? String {
        case "open":
            guard let fixture = script["fixture"] as? String else { try fail(at, "the script has no fixture") }
            try await withAbort { [self] in
                let warnings = try await session.open(contentsOf: Self.fixture(fixture))
                try check(step["match"], ["warnings": try Self.json(warnings)], "open", at)
            }
        case "openBytes":
            let bytes = Data(base64Encoded: step["base64"] as? String ?? "") ?? Data()
            try await withAbort { [self] in try await session.open(bytes) }
        case "reopen":
            guard let bytes = exported else { try fail(at, "reopen before any docx export") }
            try await withAbort { [self] in try await session.open(bytes) }
        case "export":
            let format = step["format"] as? String ?? "docx"
            try await withAbort { [self] in
                var options = try Self.decode(ExportOptions.self, ["format": format])
                if let pdf = step["pdf"] { options = .pdf(try Self.decode(PDFOptions.self, pdf)) }
                let export = try await session.export(options)
                let now = session.state.revision
                if export.revision != now { try fail(at, "the export reports revision \(export.revision), the state is at \(now)") }
                let latin1 = String(decoding: export.bytes.map { UInt16($0) }, as: UTF16.self)
                try check(step["match"], ["revision": export.revision, "warnings": try Self.json(export.warnings), "head": String(latin1.prefix(4))], "export", at)
                for want in step["contains"] as? [String] ?? [] where !latin1.contains(want) {
                    try fail(at, "the export has no \"\(want)\"")
                }
                if format == "docx" { exported = export.bytes }
            }
        case "command":
            try await session.run(try Self.decode(Command.self, step["command"] as Any))
        case "batch":
            let commands = try (step["commands"] as? [Any] ?? []).map { try Self.decode(Command.self, $0) }
            try await session.batch(commands, label: step["label"] as? String)
        case "call":
            let method = step["method"] as? String ?? ""
            let result = try await call(method, step["args"] as? [Any] ?? [])
            try check(step["result"], result, "result", at)
            if let save = step["save"] as? String {
                saved[save] = result
                if let result, method == "dialogs.open" || method == "dialogs.openStyle" || method.hasSuffix(".update") {
                    dialogs[save] = try Self.decode(DialogModel.self, result)
                }
            }
        case "font":
            try await session.fonts.add(contentsOf: Self.fixture(step["file"] as? String ?? ""))
        case "key":
            let key = try Self.key(step["key"] as? String ?? "")
            let mods = try modifiers(step, at)
            _ = host!.key(key, modifiers: mods)
            await host!.flush()
        case "text":
            host!.text(step["text"] as? String ?? "")
            await host!.flush()
        case "click":
            try await click(step["text"] as? String ?? "", modifiers(step, at), at)
        case "destroy":
            await session.close()
        case let other:
            try fail(at, "unknown step \(other ?? "nil")")
        }
    }

    /// A session member by its TypeScript path, with its result in the wire form.
    private func call(_ method: String, _ args: [Any]) async throws -> Any? {
        func arg(_ i: Int) -> Any? { i < args.count && !(args[i] is NSNull) ? args[i] : nil }
        func parse<T: Decodable>(_ i: Int, _ type: T.Type = T.self) throws -> T { try Self.decode(T.self, arg(i) ?? NSNull()) }
        func json(_ v: some Encodable) throws -> Any { try Self.json(v) }
        if method.hasPrefix("@") {
            let parts = method.dropFirst().split(separator: ".", maxSplits: 1).map(String.init)
            guard let model = dialogs[parts[0]] else { try fail(at, "no dialog saved as \(parts[0])"); return nil }
            switch parts[1] {
            case "update":
                var patch = arg(0) as? [String: Any] ?? [:]
                patch["kind"] = model.kind.rawValue
                return try json(try await session.dialogs.update(model, with: try Self.decode(DialogPatch.self, patch)))
            case "apply":
                try await session.dialogs.apply(model)
                return nil
            default: return nil // cancel needs no call
            }
        }
        switch method {
        case "dialogs.open": return try json(try await session.dialogs.open(try parse(0, DialogKind.self)))
        case "dialogs.openStyle": return try json(try await session.dialogs.open(style: try parse(0, StyleOpen.self)))
        case "setReadOnly": try await session.setReadOnly(try parse(0)); return nil
        case "setAuthor": try await session.setAuthor(try parse(0)); return nil
        case "catalog": return try json(try await session.catalog)
        case "fonts.setReplacements":
            if arg(0) is [String: Any] { session.fonts.setReplacements(try parse(0, [String: String].self)) }
            else { session.fonts.setReplacements(try parse(0, [FontRule].self)) }
            return nil
        case "fonts.setAliases":
            if arg(0) is [String: Any] { session.fonts.setAliases(try parse(0, [String: String].self)) }
            else { session.fonts.setAliases(try parse(0, [Alias].self)) }
            return nil
        case "fonts.setGenericDefaults": session.fonts.setGenericDefaults(try parse(0)); return nil
        case "fonts.setClassification": session.fonts.setClassification(fromDocument: (arg(0) as? [String: Any])?["fromDocument"] as? Bool ?? true); return nil
        case "fonts.setFallback": session.fonts.setFallback(arg(0) as? String); return nil
        case "fonts.setCharacterFallback": session.fonts.setCharacterFallback(try parse(0)); return nil
        case "fonts.setSymbolFallback": try await session.fonts.setSymbolFallback(arg(0) as? String == "off" ? .off : .bundled); return nil
        case "find.next": return try json(try await session.find.next(try parse(0)))
        case "find.previous": return try json(try await session.find.previous(try parse(0)))
        case "find.all": return try json(try await session.find.all(try parse(0)))
        case "find.select": try await session.find.select(try parse(0)); return nil
        case "find.replace": return try json(try await session.find.replace(try parse(0), with: try parse(1)))
        case "find.replaceAll": return ["count": try await session.find.replaceAll(try parse(0), with: try parse(1))]
        case "menus.contextMenu": return try json(try await session.menus.contextMenu())
        case "menus.invoke": try await session.menus.invoke(try parse(0)); return nil
        case "spelling.addDictionary": try await session.spelling.addDictionary(try parse(0)); return nil
        case "spelling.setEnabled": try await session.spelling.setEnabled(try parse(0)); return nil
        case "spelling.setLanguage": try await session.spelling.setLanguage(try parse(0)); return nil
        case "spelling.ignore": try await session.spelling.ignore(try parse(0)); return nil
        case "spelling.add": try await session.spelling.add(try parse(0)); return nil
        case "spelling.loadUserWords": try await session.spelling.loadUserWords(try parse(1), language: try parse(0)); return nil
        case "spelling.suggestions": return try json(try await session.spelling.suggestions(for: try parse(0), language: arg(1) as? String))
        case "spelling.next": return try (try await session.spelling.next()).map(json)
        case "accessibility.setAnnouncements": try await session.accessibility.setAnnouncements(try parse(0)); return nil
        case "navigation.headings": return try json(try await session.navigation.headings())
        case "navigation.select": try await session.navigation.select(try parse(0)); return nil
        case "styles.list":
            let type = try ((arg(0) as? [String: Any])?["type"]).map { try Self.decode(StyleType.self, $0) }
            return try json(try await session.styles.list(type))
        case "lists.galleries": return try json(try await session.lists.galleries(try parse(0)))
        case "elements.list": return try json(try await session.elements.list(arg(0) == nil ? nil : try parse(0, ElementFilter.self)))
        case "elements.boxes": return try json(try await session.elements.boxes(try parse(0)))
        case "elements.select":
            try await session.elements.select(try parse(0), reveal: (arg(1) as? [String: Any])?["reveal"] as? Bool ?? true)
            return nil
        case "subTemplates.list": return try json(try await session.subTemplates.list())
        case "subTemplates.edit":
            try await run(["type": "editSubTemplate", "name": arg(0) as Any, "category": arg(1) as Any])
            return nil
        case "subTemplates.close": try await run(["type": "closeSubTemplate"]); return nil
        case "subTemplates.remove": try await run(["type": "removeSubTemplate", "name": arg(0) as Any]); return nil
        case "review.revisions": return try json(try await session.review.revisions())
        case "review.comments": return try json(try await session.review.comments())
        default: break
        }
        if method.hasPrefix("commands.") {
            // One argument is the command's value; an options object is spread into it.
            var op: [String: Any] = ["type": String(method.dropFirst("commands.".count))]
            if let fields = arg(0) as? [String: Any] { op.merge(fields) { $1 } } else if !args.isEmpty { op["value"] = args[0] }
            return try await run(op)
        }
        try fail(at, "no call method \(method) in this runner")
        return nil
    }

    /// Runs a command given as wire JSON (nulls dropped); returns its result in the wire form.
    @discardableResult
    private func run(_ op: [String: Any]) async throws -> Any? {
        let commit = try await session.run(try Self.decode(Command.self, op.filter { !($0.value is NSNull) && !(Optional<Any>.none == nil && "\($0.value)" == "nil") }))
        return try commit.value.map(Self.json)
    }

    /// `v` with every `$file` replaced by its bytes (base64) and every `$saved` by the saved value.
    private func resolve(_ v: Any) throws -> Any {
        if let o = v as? [String: Any] {
            if let file = o["$file"] as? String { return try Data(contentsOf: Self.fixture(file)).base64EncodedString() }
            if let key = o["$saved"] as? String {
                guard var value = saved[key] ?? nil else { try fail(at, "nothing saved as \(key)"); return NSNull() }
                for step in o["path"] as? [Any] ?? [] {
                    let next: Any? = switch step {
                    case let s as String: (value as? [String: Any])?[s]
                    case let n as Int: (value as? [Any]).flatMap { n < $0.count ? $0[n] : nil }
                    default: Match.items(value).first { Match.mismatch(step, $0, "") == nil }
                    }
                    guard let next else { try fail(at, "$saved \(key): no \(step) in it"); return NSNull() }
                    value = next
                }
                return value
            }
            return try o.mapValues(resolve)
        }
        if let a = v as? [Any] { return try a.map(resolve) }
        return v
    }

    /// Clicks the middle of the first occurrence of `text` on its line: found, then the caret the host reports at its
    /// end and, collapsed backward, at its start gives the line.
    private func click(_ text: String, _ mods: UInt32, _ at: String) async throws {
        guard try await session.find.next(FindQuery(text: text)).found else { try fail(at, "no \(text) to click") }
        await host!.flush()
        let end = testHost!.caret
        try await session.run(try Self.decode(Command.self, ["type": "moveSelection", "by": "character", "direction": "backward"]))
        await host!.flush()
        let start = testHost!.caret
        let scroll = testHost!.scroll
        let x = (start.minY == end.minY ? (start.minX + end.minX) / 2 : start.minX + 2) - scroll.x
        let y = start.minY + start.height / 2 - scroll.y
        host!.pointer(0, x: x, y: y, button: 0, clicks: 1, modifiers: mods)
        host!.pointer(2, x: x, y: y, button: 0, clicks: 1, modifiers: mods)
        await host!.flush()
    }

    private func check(_ match: Any?, _ actual: Any?, _ what: String, _ at: String) throws {
        guard let match, !(match is NSNull) else { return }
        if let why = Match.mismatch(match, actual as Any, what) { try fail(at, why) }
    }

    private func modifiers(_ step: [String: Any], _ at: String) throws -> UInt32 {
        try (step["mods"] as? [String] ?? []).reduce(0) { m, x in
            switch x {
            case "ctrl": m | TWIP_MOD_CTRL
            case "shift": m | TWIP_MOD_SHIFT
            case "alt": m | TWIP_MOD_ALT
            case "meta": m | TWIP_MOD_META
            default: throw ScriptFailure(description: "\(runner): \(name), \(at): unknown modifier \(x)")
            }
        }
    }

    /// The engine's key for a `KeyboardEvent.key` value.
    static func key(_ key: String) throws -> UInt32 {
        let punctuation = ["-": TWIP_KEY_MINUS, "=": TWIP_KEY_EQUAL, "[": TWIP_KEY_BRACKET_LEFT, "]": TWIP_KEY_BRACKET_RIGHT,
                           "\\": TWIP_KEY_BACKSLASH, ";": TWIP_KEY_SEMICOLON, "'": TWIP_KEY_QUOTE, "`": TWIP_KEY_BACKQUOTE,
                           ",": TWIP_KEY_COMMA, ".": TWIP_KEY_PERIOD, "/": TWIP_KEY_SLASH, " ": TWIP_KEY_SPACE]
        if let k = punctuation[key] { return k.rawValue }
        if key.count == 1, let c = key.lowercased().unicodeScalars.first {
            if ("a"..."z").contains(c) { return TWIP_KEY_A.rawValue + c.value - 97 }
            if ("0"..."9").contains(c) { return TWIP_KEY_DIGIT0.rawValue + c.value - 48 }
        }
        if key.hasPrefix("F"), let n = UInt32(key.dropFirst()), (1...12).contains(n) { return TWIP_KEY_F1.rawValue + n - 1 }
        let named = ["Enter": TWIP_KEY_ENTER, "Tab": TWIP_KEY_TAB, "Backspace": TWIP_KEY_BACKSPACE, "Delete": TWIP_KEY_DELETE,
                     "Escape": TWIP_KEY_ESCAPE, "Insert": TWIP_KEY_INSERT, "Home": TWIP_KEY_HOME, "End": TWIP_KEY_END,
                     "PageUp": TWIP_KEY_PAGE_UP, "PageDown": TWIP_KEY_PAGE_DOWN, "ArrowLeft": TWIP_KEY_ARROW_LEFT,
                     "ArrowRight": TWIP_KEY_ARROW_RIGHT, "ArrowUp": TWIP_KEY_ARROW_UP, "ArrowDown": TWIP_KEY_ARROW_DOWN,
                     "ContextMenu": TWIP_KEY_CONTEXT_MENU]
        guard let k = named[key] else { throw ScriptFailure(description: "no engine key for \(key)") }
        return k.rawValue
    }

    /// Records each event in its wire form, checks `documentChanged`'s state, and runs listeners.
    private func observe(_ note: EditorSession.Note) {
        guard case .event(let event, let state) = note, var e = try? Self.json(event) as? [String: Any] else { return }
        if case .documentChanged(let record) = event {
            let now = session.state.revision
            if state.revision != record.revision || now < record.revision {
                lock.withLock {
                    inconsistent = inconsistent ?? "documentChanged at revision \(record.revision): its state is at \(state.revision) and the session's at \(now)"
                }
            }
        }
        let type = e["type"] as? String ?? ""
        e["type"] = type
        lock.withLock {
            events.append(e)
            guard let i = listeners.firstIndex(where: { $0.type == type }) else { return }
            let (_, command, at) = listeners.remove(at: i)
            let session = session!
            background.append(Task { [weak self] in
                do { try await session.run(try Self.decode(Command.self, command)) } catch {
                    self?.lock.withLock { self?.listenerFailure = self?.listenerFailure ?? "\(at) (listener): failed: \(Self.describe(error))" }
                }
            })
        }
    }

    private static func json(_ value: some Encodable) throws -> Any {
        try JSONSerialization.jsonObject(with: Wire.encode(value), options: .fragmentsAllowed)
    }

    private static func decode<T: Decodable>(_ type: T.Type, _ json: Any) throws -> T {
        try Wire.decode(T.self, from: JSONSerialization.data(withJSONObject: json, options: .fragmentsAllowed))
    }

    static func code(_ e: any Error) -> String {
        switch e {
        case let t as TwipError: t.code.rawValue
        case is CancellationError: "aborted"
        // The Swift form of an invalid argument: the SDK can't express it, so it is rejected before the engine.
        case is DecodingError: "invalid-argument"
        default: "\(type(of: e))"
        }
    }

    static func describe(_ e: any Error) -> String { "\(code(e)): \(e)" }

    private func fail(_ at: String, _ why: String) throws -> Never {
        throw ScriptFailure(description: "\(runner): \(name), \(at): \(why)")
    }
}

/// A headless view: it keeps the caret and scroll the engine reports.
final class TestHost: EditorHostDelegate, @unchecked Sendable {
    private let lock = NSLock()
    private var _caret = CGRect.zero
    private var _scroll = CGPoint.zero
    var caret: CGRect { lock.withLock { _caret } }
    var scroll: CGPoint { lock.withLock { _scroll } }

    func host(_ host: EditorHost, didRequest request: HostRequest) {
        lock.withLock {
            switch request {
            case .caret(let x, let y, let height, _): _caret = CGRect(x: x, y: y, width: 1, height: height)
            case .scroll(let x, let y): _scroll = CGPoint(x: x, y: y)
            default: break
            }
        }
    }

    func host(_ host: EditorHost, didDraw frame: FrameInfo, image: CGImage?) {}
    func host(_ host: EditorHost, didReceive event: EditorEvent) {}
}

/// The contract's partial match (tests/contract/README.md, "Matching").
enum Match {
    /// Why `got` doesn't match `want`, or nil.
    static func mismatch(_ want: Any, _ got: Any, _ path: String) -> String? {
        let shown = describe(got)
        if let special = want as? [String: Any], special.count == 1 {
            if let exact = special["$exact"] {
                return equal(exact, got) ? nil : "\(path): expected exactly \(describe(exact)), got \(shown)"
            }
            if let type = special["$type"] as? String {
                return typeName(got) == type ? nil : "\(path): expected a \(type), got \(shown)"
            }
            if let has = special["$has"] as? [Any] {
                let all = items(got)
                return got is [Any] && assign(has[...], all, Array(repeating: false, count: all.count)) ? nil
                    : "\(path): \(shown) doesn't have each of \(describe(has))"
            }
        }
        switch want {
        case let w as [String: Any]:
            guard let g = got as? [String: Any] else { return "\(path): expected an object, got \(shown)" }
            for (k, v) in w {
                if let why = mismatch(v, g[k] ?? NSNull(), "\(path).\(k)") { return why }
            }
            return nil
        case let w as [Any]:
            guard let g = got as? [Any] else { return "\(path): expected an array, got \(shown)" }
            guard g.count >= w.count else { return "\(path): expected at least \(w.count) elements, got \(g.count): \(shown)" }
            for (i, v) in w.enumerated() {
                if let why = mismatch(v, g[i], "\(path)[\(i)]") { return why }
            }
            return nil
        default:
            return scalarEqual(want, got) ? nil : "\(path): expected \(describe(want)), got \(shown)"
        }
    }

    /// `wants`, in order, against a subsequence of `events`.
    static func subsequence(_ wants: [Any], _ events: [[String: Any]]) -> String? {
        var next = 0
        for (i, want) in wants.enumerated() {
            while next < events.count, mismatch(want, events[next], "") != nil { next += 1 }
            if next == events.count {
                return "events[\(i)]: no later event matches \(describe(want)); the events were \(describe(events))"
            }
            next += 1
        }
        return nil
    }

    /// An array's elements and those of their nested `items` arrays (submenus).
    static func items(_ v: Any) -> [Any] {
        guard let a = v as? [Any] else { return [] }
        return a.flatMap { [$0] + items(($0 as? [String: Any])?["items"] as Any) }
    }

    private static func assign(_ wants: ArraySlice<Any>, _ got: [Any], _ used: [Bool]) -> Bool {
        guard let first = wants.first else { return true }
        var used = used
        for i in got.indices where !used[i] && mismatch(first, got[i], "") == nil {
            used[i] = true
            if assign(wants.dropFirst(), got, used) { return true }
            used[i] = false
        }
        return false
    }

    /// Exact equality; a missing key equals null.
    private static func equal(_ a: Any, _ b: Any) -> Bool {
        switch (a, b) {
        case let (x as [String: Any], y as [String: Any]):
            return Set(x.keys).union(y.keys).allSatisfy { equal(x[$0] ?? NSNull(), y[$0] ?? NSNull()) }
        case let (x as [Any], y as [Any]):
            return x.count == y.count && zip(x, y).allSatisfy(equal)
        default:
            return scalarEqual(a, b)
        }
    }

    private static func scalarEqual(_ a: Any, _ b: Any) -> Bool {
        switch (a, b) {
        case (is NSNull, is NSNull): return true
        case let (x as String, y as String): return x == y
        case let (x as NSNumber, y as NSNumber): return isBool(x) == isBool(y) && x == y
        default: return false
        }
    }

    private static func isBool(_ n: NSNumber) -> Bool { CFGetTypeID(n) == CFBooleanGetTypeID() }

    private static func typeName(_ v: Any) -> String {
        switch v {
        case is NSNull: "null"
        case is [String: Any]: "object"
        case is [Any]: "array"
        case is String: "string"
        case let n as NSNumber: isBool(n) ? "boolean" : "number"
        default: "null"
        }
    }

    static func describe(_ v: Any) -> String {
        (try? JSONSerialization.data(withJSONObject: v, options: [.fragmentsAllowed, .sortedKeys]))
            .map { String(decoding: $0, as: UTF8.self) } ?? "\(v)"
    }
}
