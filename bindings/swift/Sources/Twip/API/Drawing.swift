import Foundation

// Shape values (crates/twip/src/api/drawing.rs): preset geometries, colours, fills, outlines, effects and text bodies,
// as applications set and read them. Colours stay as written (a theme colour and its transforms), never only the
// resolved RGB. No renames from Rust; `Degrees` is a typealias of `Double`.

/// Degrees, clockwise.
public typealias Degrees = Double

/// A preset geometry by its ECMA-376 name (`"wedgeRectCallout"`). A string literal reads as one.
public struct ShapePreset: WireID {
    /// The ECMA-376 name.
    public let rawValue: String
    /// The preset this ECMA-376 name names; the engine refuses a name it doesn't know.
    public init(rawValue: String) { self.rawValue = rawValue }

    /// Whether it is a line or connector (it has no filled path): inserted 72 x 0 pt with the line template.
    public var isLine: Bool { rawValue == "line" || rawValue.contains("Connector") }
}

/// A theme colour.
public enum ThemeColour: String, Hashable, Sendable, Codable, CaseIterable {
    /// Dark 1 (through the document's colour mapping).
    case text1
    /// Light 1.
    case background1
    /// Dark 2.
    case text2
    /// Light 2.
    case background2
    /// Accent 1.
    case accent1
    /// Accent 2.
    case accent2
    /// Accent 3.
    case accent3
    /// Accent 4.
    case accent4
    /// Accent 5.
    case accent5
    /// Accent 6.
    case accent6
    /// The hyperlink colour.
    case hyperlink
    /// The followed hyperlink colour.
    case followedHyperlink
}

/// What a colour transform does.
public enum TransformKind: String, Hashable, Sendable, Codable, CaseIterable {
    /// Towards white by `1 - value`.
    case tint
    /// Towards black by `1 - value`.
    case shade
    /// Luminance times `value`.
    case lumMod
    /// Luminance plus `value`.
    case lumOff
    /// Saturation times `value`.
    case satMod
    /// Saturation plus `value`.
    case satOff
    /// Hue times `value`.
    case hueMod
    /// Hue plus `value` degrees.
    case hueOff
    /// Opacity `value`.
    case alpha
    /// Opacity times `value`.
    case alphaMod
}

/// One colour transform; `value` is a fraction (0.75), degrees for `hueOff`.
public struct ColourTransform: Hashable, Sendable, Codable {
    /// What it does.
    public var kind: TransformKind
    /// How much.
    public var value: Double

    /// A transform.
    public init(kind: TransformKind, value: Double) {
        self.kind = kind
        self.value = value
    }
}

/// A colour: exactly one base (a theme colour, an RGB value or a preset colour name), then its transforms in order.
public struct ColourValue: Hashable, Sendable, Codable {
    /// A theme colour.
    public var theme: ThemeColour?
    /// An RGB colour.
    public var rgb: RGBColor?
    /// A preset colour by its CSS name (`"cornflowerBlue"`).
    public var preset: String?
    /// The transforms, applied in order.
    @OmitEmpty public var transforms: [ColourTransform]

    /// A colour; give exactly one of `theme`, `rgb` and `preset`.
    public init(theme: ThemeColour? = nil, rgb: RGBColor? = nil, preset: String? = nil, transforms: [ColourTransform] = []) {
        self.theme = theme
        self.rgb = rgb
        self.preset = preset
        self.transforms = transforms
    }

    /// An RGB colour.
    public static func rgb(_ color: RGBColor) -> ColourValue { ColourValue(rgb: color) }

    /// A theme colour.
    public static func theme(_ theme: ThemeColour) -> ColourValue { ColourValue(theme: theme) }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        theme = try c.decodeIfPresent(ThemeColour.self, forKey: .theme)
        rgb = try c.decodeIfPresent(RGBColor.self, forKey: .rgb)
        preset = try c.decodeIfPresent(String.self, forKey: .preset)
        transforms = try c.decodeIfPresent([ColourTransform].self, forKey: .transforms) ?? []
    }
}

/// A gradient's shape.
public enum GradientShape: String, Hashable, Sendable, Codable, CaseIterable {
    /// Along a direction (`angle`).
    case linear
    /// From the centre outward in circles.
    case radial
    /// From the centre outward in rectangles.
    case rectangular
    /// Following the shape's outline.
    case path
}

/// A gradient stop.
public struct GradientStop: Hashable, Sendable, Codable {
    /// 0 to 1 along the gradient.
    public var position: Double
    /// The colour there.
    public var colour: ColourValue

    /// A stop.
    public init(position: Double, colour: ColourValue) {
        self.position = position
        self.colour = colour
    }
}

/// A fill. On the wire tagged by `type`.
public enum FillValue: Hashable, Sendable, Codable {
    /// No fill.
    case none
    /// One colour.
    case solid(colour: ColourValue)
    /// A gradient: its shape, a linear gradient's direction (0: left to right), and two or more stops.
    case gradient(kind: GradientShape, angle: Degrees? = nil, stops: [GradientStop])
    /// A two-colour pattern: the ECMA-376 pattern name (`"pct50"`, `"dkDnDiag"`), the pattern's colour and the colour
    /// behind it.
    case pattern(preset: String, foreground: ColourValue, background: ColourValue)
    /// A picture (PNG, JPEG, GIF, BMP; empty in the state, which reports the fill, not its bytes), tiled at its own
    /// size instead of stretched when `tile`.
    case picture(image: Data, tile: Bool? = nil)
    /// The style reference's fill (no explicit fill).
    case fromStyle

    public init(from decoder: any Decoder) throws {
        let t = try TaggedDecoder(decoder)
        switch t.tag {
        case "none": self = .none
        case "solid": self = .solid(colour: try t["colour"])
        case "gradient": self = .gradient(kind: try t["kind"], angle: try t.optional("angle"), stops: try t["stops"])
        case "pattern": self = .pattern(preset: try t["preset"], foreground: try t["foreground"], background: try t["background"])
        case "picture": self = .picture(image: try t["image"], tile: try t.optional("tile"))
        case "fromStyle": self = .fromStyle
        default: throw t.unknown()
        }
    }

    public func encode(to encoder: any Encoder) throws {
        let t = TaggedEncoder(encoder)
        switch self {
        case .none: try t.tag("none")
        case .solid(let colour): try t.tag("solid", ["colour": colour])
        case .gradient(let kind, let angle, let stops): try t.tag("gradient", ["kind": kind, "angle": angle, "stops": stops])
        case .pattern(let preset, let foreground, let background):
            try t.tag("pattern", ["preset": preset, "foreground": foreground, "background": background])
        case .picture(let image, let tile): try t.tag("picture", ["image": image, "tile": tile])
        case .fromStyle: try t.tag("fromStyle")
        }
    }
}

/// How a line is drawn along its length.
public enum Compound: String, Hashable, Sendable, Codable, CaseIterable {
    /// One line.
    case single
    /// Two equal lines.
    case double
    /// A thick line then a thin one.
    case thickThin
    /// A thin line then a thick one.
    case thinThick
    /// Three lines.
    case triple
}

/// A line's ends.
public enum LineCap: String, Hashable, Sendable, Codable, CaseIterable {
    /// Square, at the end.
    case flat
    /// Rounded.
    case round
    /// Square, half the width past the end.
    case square
}

/// How a line's segments join.
public enum LineJoin: String, Hashable, Sendable, Codable, CaseIterable {
    /// Rounded.
    case round
    /// Cut off.
    case bevel
    /// Pointed.
    case miter
}

/// An arrowhead's shape.
public enum ArrowKind: String, Hashable, Sendable, Codable, CaseIterable {
    /// A filled triangle.
    case triangle
    /// A notched triangle.
    case stealth
    /// A diamond.
    case diamond
    /// An oval.
    case oval
    /// An open V.
    case arrow
}

/// An arrowhead's width or length.
public enum ArrowSize: String, Hashable, Sendable, Codable, CaseIterable {
    /// Small.
    case small
    /// Medium.
    case medium
    /// Large.
    case large
}

/// An arrowhead.
public struct Arrowhead: Hashable, Sendable, Codable {
    /// Its shape.
    public var kind: ArrowKind
    /// Its width.
    public var width: ArrowSize
    /// Its length.
    public var length: ArrowSize

    /// An arrowhead, medium unless given.
    public init(kind: ArrowKind, width: ArrowSize = .medium, length: ArrowSize = .medium) {
        self.kind = kind
        self.width = width
        self.length = length
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        kind = try c.decode(ArrowKind.self, forKey: .kind)
        width = try c.decodeIfPresent(ArrowSize.self, forKey: .width) ?? .medium
        length = try c.decodeIfPresent(ArrowSize.self, forKey: .length) ?? .medium
    }
}

/// A line's arrowheads.
public struct LineEnds: Hashable, Sendable, Codable {
    /// At the start.
    public var head: Arrowhead?
    /// At the end.
    public var tail: Arrowhead?

    /// A line's arrowheads.
    public init(head: Arrowhead? = nil, tail: Arrowhead? = nil) {
        self.head = head
        self.tail = tail
    }
}

/// An outline.
public struct LineValue: Hashable, Sendable, Codable {
    /// The width (0.75 pt when the document gives none).
    public var width: Points
    /// `none`, `solid` or `gradient`.
    public var fill: FillValue
    /// The ECMA-376 dash name (`"solid"`, `"dash"`, `"sysDot"`…); `"custom"` for a custom dash.
    public var dash: String
    /// Single or several lines.
    public var compound: Compound
    /// The ends.
    public var cap: LineCap
    /// The joins.
    public var join: LineJoin
    /// The arrowheads.
    public var ends: LineEnds

    /// An outline.
    public init(
        width: Points = 0.75, fill: FillValue, dash: String = "solid", compound: Compound = .single, cap: LineCap = .flat,
        join: LineJoin = .round, ends: LineEnds = LineEnds()
    ) {
        self.width = width
        self.fill = fill
        self.dash = dash
        self.compound = compound
        self.cap = cap
        self.join = join
        self.ends = ends
    }
}

/// `setShapeLine`'s change; what isn't given is kept.
public struct LineChange: Hashable, Sendable, Codable {
    /// The width (0…1584 pt).
    public var width: Points?
    /// `none`, `solid` or `gradient`; `fromStyle` removes the whole explicit outline.
    public var fill: FillValue?
    /// The ECMA-376 dash name.
    public var dash: String?
    /// Single or several lines.
    public var compound: Compound?
    /// The ends.
    public var cap: LineCap?
    /// The joins.
    public var join: LineJoin?
    /// The arrowheads: both ends are replaced (an end not given is removed).
    public var ends: LineEnds?

    /// A change to an outline.
    public init(
        width: Points? = nil, fill: FillValue? = nil, dash: String? = nil, compound: Compound? = nil, cap: LineCap? = nil,
        join: LineJoin? = nil, ends: LineEnds? = nil
    ) {
        self.width = width
        self.fill = fill
        self.dash = dash
        self.compound = compound
        self.cap = cap
        self.join = join
        self.ends = ends
    }
}

/// A shadow.
public struct Shadow: Hashable, Sendable, Codable {
    /// Its colour (with its transparency as an `alpha` transform).
    public var colour: ColourValue
    /// How far it blurs.
    public var blur: Points
    /// How far from the shape.
    public var distance: Points
    /// Which way, clockwise from the right.
    public var direction: Degrees

    /// A shadow.
    public init(colour: ColourValue, blur: Points, distance: Points, direction: Degrees) {
        self.colour = colour
        self.blur = blur
        self.distance = distance
        self.direction = direction
    }
}

/// A glow.
public struct Glow: Hashable, Sendable, Codable {
    /// Its colour.
    public var colour: ColourValue
    /// How far it reaches.
    public var radius: Points

    /// A glow.
    public init(colour: ColourValue, radius: Points) {
        self.colour = colour
        self.radius = radius
    }
}

/// A shape's effects.
public struct EffectsValue: Hashable, Sendable, Codable {
    /// A shadow outside the shape.
    public var outerShadow: Shadow?
    /// A shadow inside the shape.
    public var innerShadow: Shadow?
    /// A glow around the shape.
    public var glow: Glow?
    /// Edges faded over this distance.
    public var softEdge: Points?
    /// The whole shape blurred by this radius.
    public var blur: Points?

    /// A shape's effects.
    public init(outerShadow: Shadow? = nil, innerShadow: Shadow? = nil, glow: Glow? = nil, softEdge: Points? = nil, blur: Points? = nil) {
        self.outerShadow = outerShadow
        self.innerShadow = innerShadow
        self.glow = glow
        self.softEdge = softEdge
        self.blur = blur
    }
}

/// `setShapeEffects`'s change: each effect given replaces that effect, `.some(nil)` (`null`) removes it, `nil` (left
/// out) keeps it.
public struct EffectsChange: Hashable, Sendable, Codable {
    /// A shadow outside the shape.
    public var outerShadow: Shadow??
    /// A shadow inside the shape.
    public var innerShadow: Shadow??
    /// A glow around the shape.
    public var glow: Glow??
    /// Edges faded over this distance.
    public var softEdge: Points??
    /// The whole shape blurred by this radius.
    public var blur: Points??

    enum CodingKeys: String, CodingKey {
        case outerShadow, innerShadow, glow, softEdge, blur
    }

    /// A change to a shape's effects.
    public init(outerShadow: Shadow?? = nil, innerShadow: Shadow?? = nil, glow: Glow?? = nil, softEdge: Points?? = nil, blur: Points?? = nil) {
        self.outerShadow = outerShadow
        self.innerShadow = innerShadow
        self.glow = glow
        self.softEdge = softEdge
        self.blur = blur
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        func field<T: Decodable>(_ key: CodingKeys) throws -> T?? {
            guard c.contains(key) else { return nil }
            return try c.decodeNil(forKey: key) ? .some(nil) : .some(c.decode(T.self, forKey: key))
        }
        outerShadow = try field(.outerShadow)
        innerShadow = try field(.innerShadow)
        glow = try field(.glow)
        softEdge = try field(.softEdge)
        blur = try field(.blur)
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        func field<T: Encodable>(_ value: T??, _ key: CodingKeys) throws {
            switch value {
            case .none: break
            case .some(.none): try c.encodeNil(forKey: key)
            case .some(.some(let v)): try c.encode(v, forKey: key)
            }
        }
        try field(outerShadow, .outerShadow)
        try field(innerShadow, .innerShadow)
        try field(glow, .glow)
        try field(softEdge, .softEdge)
        try field(blur, .blur)
    }
}

/// Where text sits in a shape vertically.
public enum TextAnchor: String, Hashable, Sendable, Codable, CaseIterable {
    /// At the top.
    case top
    /// In the middle.
    case middle
    /// At the bottom.
    case bottom
}

/// How a shape fits its text.
public enum TextAutofit: String, Hashable, Sendable, Codable, CaseIterable {
    /// The shape keeps its size.
    case none
    /// The shape's height follows its text.
    case shape
}

/// Which way a shape's text runs.
public enum TextDirection: String, Hashable, Sendable, Codable, CaseIterable {
    /// Left to right, lines downward.
    case horizontal
    /// Turned 90° clockwise.
    case vertical
    /// Turned 270° clockwise.
    case vertical270
    /// East Asian vertical: CJK upright, other text turned.
    case eastAsianVertical
    /// Mongolian vertical.
    case mongolianVertical
    /// Letters stacked, one per line.
    case stacked
    /// Letters stacked, right to left.
    case stackedRightToLeft
}

/// A shape's text body as written.
public struct ShapeTextState: Hashable, Sendable, Codable {
    /// Left, top, right, bottom.
    public var insets: [Points]
    /// Where the text sits.
    public var anchor: TextAnchor
    /// How the shape fits it.
    public var autofit: TextAutofit
    /// Lines wrap at the shape's width.
    public var wrapText: Bool
    /// Which way it runs.
    public var direction: TextDirection

    /// A text body.
    public init(
        insets: [Points] = [0, 0, 0, 0], anchor: TextAnchor = .top, autofit: TextAutofit = .none, wrapText: Bool = false,
        direction: TextDirection = .horizontal
    ) {
        self.insets = insets
        self.anchor = anchor
        self.autofit = autofit
        self.wrapText = wrapText
        self.direction = direction
    }
}

/// `setShapeText`'s change; what isn't given is kept.
public struct ShapeTextChange: Hashable, Sendable, Codable {
    /// Left, top, right, bottom (0…1584 pt).
    public var insets: [Points]?
    /// Where the text sits.
    public var anchor: TextAnchor?
    /// How the shape fits it.
    public var autofit: TextAutofit?
    /// Lines wrap at the shape's width.
    public var wrapText: Bool?
    /// Which way it runs.
    public var direction: TextDirection?

    /// A change to a text body.
    public init(
        insets: [Points]? = nil, anchor: TextAnchor? = nil, autofit: TextAutofit? = nil, wrapText: Bool? = nil,
        direction: TextDirection? = nil
    ) {
        self.insets = insets
        self.anchor = anchor
        self.autofit = autofit
        self.wrapText = wrapText
        self.direction = direction
    }
}

/// `setPictureAdjust`'s change; what isn't given is kept. Fractions are −1…1 (transparency 0…1).
public struct PictureAdjustChange: Hashable, Sendable, Codable {
    /// How see-through the picture is (0 opaque, 1 invisible).
    public var transparency: Double?
    /// Shown in shades of grey.
    public var grayscale: Bool?
    /// Lighter (positive) or darker.
    public var brightness: Double?
    /// More (positive) or less contrast.
    public var contrast: Double?

    /// A change to a picture's adjustments.
    public init(transparency: Double? = nil, grayscale: Bool? = nil, brightness: Double? = nil, contrast: Double? = nil) {
        self.transparency = transparency
        self.grayscale = grayscale
        self.brightness = brightness
        self.contrast = contrast
    }
}

/// `insertShape`'s options.
public struct ShapeInsert: Hashable, Sendable, Codable {
    /// The preset geometry.
    public var preset: ShapePreset
    /// The width (72 pt when not given).
    public var width: Points?
    /// The height (72 pt when not given; 0 for lines).
    public var height: Points?

    /// A shape to insert.
    public init(preset: ShapePreset, width: Points? = nil, height: Points? = nil) {
        self.preset = preset
        self.width = width
        self.height = height
    }
}

/// `setShapeAdjust`'s options.
public struct ShapeAdjust: Hashable, Sendable, Codable {
    /// The adjust value's index, in the preset's order.
    public var index: Int
    /// Its new value (the preset's units, usually 100000ths).
    public var value: Int

    /// An adjust value.
    public init(index: Int, value: Int) {
        self.index = index
        self.value = value
    }
}

/// Which way to flip.
public enum FlipAxis: String, Hashable, Sendable, Codable, CaseIterable {
    /// Left to right.
    case horizontal
    /// Top to bottom.
    case vertical
}

/// Where an object goes in the paint order.
public enum Arrange: String, Hashable, Sendable, Codable, CaseIterable {
    /// Over the next object above it.
    case forward
    /// Under the next object below it.
    case backward
    /// Over every object.
    case toFront
    /// Under every object.
    case toBack
    /// In front of the text.
    case inFrontOfText
    /// Behind the text.
    case behindText
}

/// A fill as written: the shape's own, or its style reference's. On the wire `{"explicit": fill}` or
/// `{"fromStyle": fill}`.
public enum FillState: Hashable, Sendable, Codable {
    /// The shape's own fill.
    case explicit(FillValue)
    /// The style reference's fill (the theme's, with the reference's colour).
    case fromStyle(FillValue)

    /// The fill, whichever it is.
    public var fill: FillValue {
        switch self {
        case .explicit(let f), .fromStyle(let f): f
        }
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: WireKey.self)
        if let f = try c.decodeIfPresent(FillValue.self, forKey: "explicit") { self = .explicit(f) }
        else { self = .fromStyle(try c.decode(FillValue.self, forKey: "fromStyle")) }
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: WireKey.self)
        switch self {
        case .explicit(let f): try c.encode(f, forKey: "explicit")
        case .fromStyle(let f): try c.encode(f, forKey: "fromStyle")
        }
    }
}

/// An outline as written: the shape's own, or its style reference's. On the wire `{"explicit": line}` or
/// `{"fromStyle": line}`.
public enum LineState: Hashable, Sendable, Codable {
    /// The shape's own outline.
    case explicit(LineValue)
    /// The style reference's outline.
    case fromStyle(LineValue)

    /// The outline, whichever it is.
    public var line: LineValue {
        switch self {
        case .explicit(let l), .fromStyle(let l): l
        }
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: WireKey.self)
        if let l = try c.decodeIfPresent(LineValue.self, forKey: "explicit") { self = .explicit(l) }
        else { self = .fromStyle(try c.decode(LineValue.self, forKey: "fromStyle")) }
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: WireKey.self)
        switch self {
        case .explicit(let l): try c.encode(l, forKey: "explicit")
        case .fromStyle(let l): try c.encode(l, forKey: "fromStyle")
        }
    }
}

/// The selected shape's properties, as written.
public struct ShapeState: Hashable, Sendable, Codable {
    /// The preset geometry; nil for a custom geometry.
    public var preset: ShapePreset?
    /// The adjust values in effect, in the preset's order.
    public var adjust: [Int]
    /// The fill.
    public var fill: FillState
    /// The outline.
    public var line: LineState
    /// The effects.
    public var effects: EffectsValue
    /// The text body; nil for pictures and groups.
    public var text: ShapeTextState?

    /// A shape's properties.
    public init(
        preset: ShapePreset? = nil, adjust: [Int] = [], fill: FillState, line: LineState, effects: EffectsValue = EffectsValue(),
        text: ShapeTextState? = nil
    ) {
        self.preset = preset
        self.adjust = adjust
        self.fill = fill
        self.line = line
        self.effects = effects
        self.text = text
    }
}

/// A group of the shape gallery.
public struct ShapeGalleryGroup: Hashable, Sendable, Codable {
    /// Its label (`"Basic Shapes"`).
    public var label: String
    /// Its presets, in Word's order.
    public var presets: [ShapePreset]

    /// A gallery group.
    public init(label: String, presets: [ShapePreset]) {
        self.label = label
        self.presets = presets
    }
}
