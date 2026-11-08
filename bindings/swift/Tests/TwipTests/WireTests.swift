import Foundation
import Testing
@testable import Twip

/// A wire fixture: `crates/twip/tests/wire/<name>.json`, and the Swift type its examples round-trip through.
struct Fixture: Sendable, CustomStringConvertible {
    let name: String
    let roundTrip: @Sendable (Data) throws -> Data
    var description: String { name }
}

func fixture<T: Codable>(_ name: String, _ type: T.Type) -> Fixture {
    Fixture(name: name) { try Wire.encode(try Wire.decode(T.self, from: $0)) }
}

/// Every fixture, from the per-module lists in `Fixtures/`.
enum Fixtures {
    static var all: [Fixture] { core + values + commands + menusAndDialogs + services + state + drawing }

    static let core: [Fixture] = [
        fixture("error-code", ErrorCode.self),
        fixture("error", TwipError.self),
        fixture("tri", Tri<Bool>.self),
        fixture("auto-or", AutoOr<RGBColor>.self),
        fixture("none-or", NoneOr<RGBColor>.self),
        fixture("outline-level", OutlineLevel.self),
        fixture("color", RGBColor.self),
        fixture("points", Points.self),
        fixture("bytes", Data.self),
    ]
}

enum Repo {
    /// A path in the repository.
    static func path(_ parts: String...) -> URL {
        var url = URL(fileURLWithPath: #filePath)
        for _ in 0..<5 { url.deleteLastPathComponent() } // bindings/swift/Tests/TwipTests/WireTests.swift
        return parts.reduce(url) { $0.appendingPathComponent($1) }
    }
}

/// The wire fixtures round-trip through the Swift types: a re-encoded value has the original's fields, and may add
/// only `null` where the engine leaves a field out.
@Suite struct WireTests {
    static let dir = Repo.path("crates", "twip", "tests", "wire")

    @Test(arguments: Fixtures.all)
    func roundTrips(_ fixture: Fixture) throws {
        let data = try Data(contentsOf: Self.dir.appendingPathComponent("\(fixture.name).json"))
        let items = try #require(try JSONSerialization.jsonObject(with: data, options: .fragmentsAllowed) as? [Any])
        for (i, item) in items.enumerated() {
            let one = try JSONSerialization.data(withJSONObject: item, options: .fragmentsAllowed)
            let again = try fixture.roundTrip(one)
            let back = try JSONSerialization.jsonObject(with: again, options: .fragmentsAllowed)
            if let why = JSONDiff.difference(item, back, at: "\(fixture.name)[\(i)]") {
                Issue.record("\(why)\n  was:   \(String(decoding: one, as: UTF8.self))\n  wrote: \(String(decoding: again, as: UTF8.self))")
            }
        }
    }

    /// Every fixture file has a Swift type (balloons.json holds stackBalloons cases, not a type).
    @Test func everyFixtureHasAType() throws {
        let names = Set(Fixtures.all.map(\.name))
        let files = try FileManager.default.contentsOfDirectory(atPath: Self.dir.path)
            .filter { $0.hasSuffix(".json") && $0 != "balloons.json" }
            .map { String($0.dropLast(5)) }
        #expect(Set(files).subtracting(names).sorted() == [])
        #expect(names.subtracting(files).sorted() == [])
    }
}

enum JSONDiff {
    /// Why `b` (re-encoded) differs from `a` (the fixture), or nil.
    static func difference(_ a: Any, _ b: Any, at path: String) -> String? {
        switch (a, b) {
        case let (a as [String: Any], b as [String: Any]):
            for (k, v) in a where !(v is NSNull) {
                guard let w = b[k] else { return "\(path).\(k): missing" }
                if let why = difference(v, w, at: "\(path).\(k)") { return why }
            }
            for (k, w) in b where a[k] == nil && !isDefault(w) {
                return "\(path).\(k): extra field \(w)"
            }
            return nil
        case let (a as [Any], b as [Any]):
            guard a.count == b.count else { return "\(path): \(a.count) items, wrote \(b.count)" }
            for (i, (v, w)) in zip(a, b).enumerated() {
                if let why = difference(v, w, at: "\(path)[\(i)]") { return why }
            }
            return nil
        case (is NSNull, is NSNull):
            return nil
        case let (a as NSNumber, b as NSNumber):
            // Booleans and numbers are both NSNumber: tell them apart.
            let boolA = CFGetTypeID(a) == CFBooleanGetTypeID(), boolB = CFGetTypeID(b) == CFBooleanGetTypeID()
            return boolA == boolB && a == b ? nil : "\(path): \(a), wrote \(b)"
        case let (a as String, b as String):
            return a == b ? nil : "\(path): \"\(a)\", wrote \"\(b)\""
        default:
            return "\(path): \(a), wrote \(b)"
        }
    }

    private static func isDefault(_ v: Any) -> Bool { v is NSNull }
}
