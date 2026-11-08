import Foundation

// The wire: twip::api values as JSON (crates/twip/src/api/wire.rs). Fields are camelCase, tagged enums carry their
// variant in `type` or `kind`, a single non-object argument travels in `value`, optional fields are left out when
// unset, lengths are points, colours `#RRGGBB` and bytes base64.

/// A length in points (1/72 inch). Every length in the API is one.
public typealias Points = Double

/// Any JSON value: the `details` of an error, a menu action's payload.
public enum JSONValue: Hashable, Sendable, Codable {
    /// `null`.
    case null
    /// `true` or `false`.
    case bool(Bool)
    /// A number.
    case number(Double)
    /// A string.
    case string(String)
    /// An array.
    case array([JSONValue])
    /// An object.
    case object([String: JSONValue])

    public init(from decoder: any Decoder) throws {
        let c = try decoder.singleValueContainer()
        if c.decodeNil() { self = .null }
        else if let b = try? c.decode(Bool.self) { self = .bool(b) }
        else if let n = try? c.decode(Double.self) { self = .number(n) }
        else if let s = try? c.decode(String.self) { self = .string(s) }
        else if let a = try? c.decode([JSONValue].self) { self = .array(a) }
        else { self = .object(try c.decode([String: JSONValue].self)) }
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.singleValueContainer()
        switch self {
        case .null: try c.encodeNil()
        case .bool(let b): try c.encode(b)
        case .number(let n): try c.encode(n)
        case .string(let s): try c.encode(s)
        case .array(let a): try c.encode(a)
        case .object(let o): try c.encode(o)
        }
    }

    /// The member `key` of an object, or nil.
    public subscript(key: String) -> JSONValue? {
        if case .object(let o) = self { return o[key] }
        return nil
    }

    /// The string, or nil.
    public var stringValue: String? {
        if case .string(let s) = self { return s }
        return nil
    }
}

/// A coding key for any string: the tagged enums' fields.
struct WireKey: CodingKey, ExpressibleByStringLiteral {
    let stringValue: String
    init(_ string: String) { stringValue = string }
    init(stringLiteral value: String) { stringValue = value }
    init?(stringValue: String) { self.stringValue = stringValue }
    var intValue: Int? { nil }
    init?(intValue: Int) { nil }
}

/// Reads a tagged enum: its tag (`type`, `kind`, `rule`, ...) and the fields beside it.
struct TaggedDecoder {
    let decoder: any Decoder
    let container: KeyedDecodingContainer<WireKey>
    /// The variant's name.
    let tag: String

    init(_ decoder: any Decoder, tag key: String = "type") throws {
        self.decoder = decoder
        container = try decoder.container(keyedBy: WireKey.self)
        tag = try container.decode(String.self, forKey: WireKey(key))
    }

    /// The field `key`.
    subscript<T: Decodable>(_ key: String) -> T {
        get throws { try container.decode(T.self, forKey: WireKey(key)) }
    }

    /// The field `key`, or nil when it is absent or null.
    func optional<T: Decodable>(_ key: String) throws -> T? {
        try container.decodeIfPresent(T.self, forKey: WireKey(key))
    }

    /// The single argument, in `value`.
    func value<T: Decodable>() throws -> T { try self["value"] }

    /// An options object whose fields sit beside the tag.
    func spread<T: Decodable>() throws -> T { try T(from: decoder) }

    /// The error for a tag this SDK doesn't know.
    func unknown() -> DecodingError {
        .dataCorruptedError(forKey: "type", in: container, debugDescription: "unknown variant \(tag)")
    }
}

/// Writes a tagged enum: its tag and the fields beside it.
struct TaggedEncoder {
    let encoder: any Encoder
    let key: String

    init(_ encoder: any Encoder, tag key: String = "type") {
        self.encoder = encoder
        self.key = key
    }

    /// The tag, with `fields` beside it; nil fields are left out.
    func tag(_ name: String, _ fields: KeyValuePairs<String, (any Encodable)?> = [:]) throws {
        var c = encoder.container(keyedBy: WireKey.self)
        try c.encode(name, forKey: WireKey(key))
        for (k, v) in fields {
            if let v { try c.encode(v, forKey: WireKey(k)) }
        }
    }

    /// The tag, with the single argument in `value`.
    func tag(_ name: String, value: some Encodable) throws {
        try tag(name, ["value": value])
    }

    /// The tag, with an options object's fields beside it.
    func tag(_ name: String, spread options: some Encodable) throws {
        try options.encode(to: encoder)
        try tag(name)
    }
}

/// Reads a string wire value that may be a keyword: `"auto"`, `"none"`, `"mixed"`, `"body"`.
private func isKeyword(_ decoder: any Decoder, _ keyword: String) -> Bool {
    (try? decoder.singleValueContainer().decode(String.self)) == keyword
}

/// A value over the selection: the same everywhere, different in places (`"mixed"`), or not applicable here (`null`:
/// no table, nothing selected).
public enum Tri<Value> {
    /// The same value everywhere.
    case uniform(Value)
    /// Different values in places.
    case mixed
    /// Not applicable here.
    case unavailable

    /// The value when it is uniform, or nil.
    public var value: Value? {
        if case .uniform(let v) = self { return v }
        return nil
    }

    /// Whether the value differs across the selection.
    public var isMixed: Bool {
        if case .mixed = self { return true }
        return false
    }
}

extension Tri: Equatable where Value: Equatable {}
extension Tri: Hashable where Value: Hashable {}
extension Tri: Sendable where Value: Sendable {}

extension Tri where Value == Bool {
    /// True only when it is on everywhere.
    public var isOn: Bool { value == true }
}

extension Tri: Codable where Value: Codable {
    public init(from decoder: any Decoder) throws {
        let c = try decoder.singleValueContainer()
        if c.decodeNil() { self = .unavailable }
        else if isKeyword(decoder, "mixed") { self = .mixed }
        else { self = .uniform(try c.decode(Value.self)) }
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.singleValueContainer()
        switch self {
        case .uniform(let v): try c.encode(v)
        case .mixed: try c.encode("mixed")
        case .unavailable: try c.encodeNil()
        }
    }
}

/// A value, or `"auto"`: the document decides (an automatic colour, automatic spacing, a row's natural height).
public enum AutoOr<Value> {
    /// The document decides.
    case auto
    /// This value.
    case value(Value)
}

extension AutoOr: Equatable where Value: Equatable {}
extension AutoOr: Hashable where Value: Hashable {}
extension AutoOr: Sendable where Value: Sendable {}

extension AutoOr: Codable where Value: Codable {
    public init(from decoder: any Decoder) throws {
        self = isKeyword(decoder, "auto") ? .auto : .value(try decoder.singleValueContainer().decode(Value.self))
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.singleValueContainer()
        switch self {
        case .auto: try c.encode("auto")
        case .value(let v): try c.encode(v)
        }
    }
}

/// A value, or `"none"` where state reports its absence explicitly (no shading, no highlight).
public enum NoneOr<Value> {
    /// Absent.
    case none
    /// This value.
    case value(Value)
}

extension NoneOr: Equatable where Value: Equatable {}
extension NoneOr: Hashable where Value: Hashable {}
extension NoneOr: Sendable where Value: Sendable {}

extension NoneOr: Codable where Value: Codable {
    public init(from decoder: any Decoder) throws {
        self = isKeyword(decoder, "none") ? .none : .value(try decoder.singleValueContainer().decode(Value.self))
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.singleValueContainer()
        switch self {
        case .none: try c.encode("none")
        case .value(let v): try c.encode(v)
        }
    }
}

/// A paragraph's outline level: body text, or a level from 1 to 9. On the wire: `"body"` or the number.
public enum OutlineLevel: Hashable, Sendable, Codable {
    /// Body text, with no outline level.
    case body
    /// An outline level, 1 to 9.
    case level(Int)

    public init(from decoder: any Decoder) throws {
        self = isKeyword(decoder, "body") ? .body : .level(try decoder.singleValueContainer().decode(Int.self))
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.singleValueContainer()
        switch self {
        case .body: try c.encode("body")
        case .level(let n): try c.encode(n)
        }
    }
}

/// An RGB colour, `#RRGGBB` on the wire. A string literal reads as one: `"#1F4E79"`.
public struct RGBColor: Hashable, Sendable, Codable, ExpressibleByStringLiteral, CustomStringConvertible {
    /// The colour as 0xRRGGBB.
    public var rgb: UInt32

    /// The colour 0xRRGGBB.
    public init(rgb: UInt32) { self.rgb = rgb & 0xFF_FFFF }

    /// A colour from its components.
    public init(red: UInt8, green: UInt8, blue: UInt8) {
        rgb = UInt32(red) << 16 | UInt32(green) << 8 | UInt32(blue)
    }

    /// The colour a `#RRGGBB` (or `RRGGBB`) string names, or nil.
    public init?(hex: String) {
        let digits = hex.hasPrefix("#") ? hex.dropFirst() : Substring(hex)
        guard digits.count == 6, let n = UInt32(digits, radix: 16) else { return nil }
        rgb = n
    }

    /// A `#RRGGBB` literal. It must be one.
    public init(stringLiteral value: String) {
        guard let c = RGBColor(hex: value) else { preconditionFailure("\(value) is not a #RRGGBB colour") }
        self = c
    }

    /// The red component.
    public var red: UInt8 { UInt8(rgb >> 16 & 0xFF) }
    /// The green component.
    public var green: UInt8 { UInt8(rgb >> 8 & 0xFF) }
    /// The blue component.
    public var blue: UInt8 { UInt8(rgb & 0xFF) }

    /// `#RRGGBB`.
    public var description: String {
        let hex = String(rgb, radix: 16, uppercase: true)
        return "#" + String(repeating: "0", count: 6 - hex.count) + hex
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.singleValueContainer()
        let s = try c.decode(String.self)
        guard let color = RGBColor(hex: s), s.hasPrefix("#") else {
            throw DecodingError.dataCorruptedError(in: c, debugDescription: "\(s) is not a #RRGGBB colour")
        }
        self = color
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.singleValueContainer()
        try c.encode(description)
    }
}

/// An identifier the engine gives out: a string on the wire, compared by value.
public protocol WireID: RawRepresentable, Hashable, Sendable, Codable, ExpressibleByStringLiteral, CustomStringConvertible
where RawValue == String {
    /// The identifier.
    init(rawValue: String)
}

extension WireID {
    public init(stringLiteral value: String) { self.init(rawValue: value) }
    public var description: String { rawValue }
}

/// The JSON coders of the wire.
enum Wire {
    static func encode(_ value: some Encodable) throws -> Data {
        try JSONEncoder().encode(value)
    }

    static func decode<T: Decodable>(_ type: T.Type, from data: Data) throws -> T {
        try JSONDecoder().decode(type, from: data)
    }
}

/// A collection the wire leaves out when it is empty, and reads as empty when it is absent.
@propertyWrapper
public struct OmitEmpty<Value: RangeReplaceableCollection & Codable & Hashable & Sendable>: Hashable, Sendable, Codable {
    /// The collection.
    public var wrappedValue: Value

    /// Wraps a collection.
    public init(wrappedValue: Value) { self.wrappedValue = wrappedValue }

    public init(from decoder: any Decoder) throws {
        wrappedValue = try decoder.singleValueContainer().decode(Value.self)
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.singleValueContainer()
        try c.encode(wrappedValue)
    }
}

extension KeyedDecodingContainer {
    func decode<V>(_ type: OmitEmpty<V>.Type, forKey key: Key) throws -> OmitEmpty<V> {
        OmitEmpty(wrappedValue: try decodeIfPresent(V.self, forKey: key) ?? V())
    }
}

extension KeyedEncodingContainer {
    mutating func encode<V>(_ value: OmitEmpty<V>, forKey key: Key) throws {
        if !value.wrappedValue.isEmpty { try encode(value.wrappedValue, forKey: key) }
    }
}
