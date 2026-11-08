import Foundation

// The view's settings: chrome theme and colours, the page's appearance, measuring, the object grid, field shading,
// and the visible pages, from crates/twip/src/api/services.rs.

/// The editor chrome's colour scheme: the desk, rulers, handles, guides and readouts. On the wire: `"light"`,
/// `"dark"` or `{"highContrast": colors}`.
public enum ChromeTheme: Hashable, Sendable, Codable {
    /// Light chrome.
    case light
    /// Dark chrome.
    case dark
    /// The platform's high contrast colours (the SDKs set this when the platform is in high contrast mode).
    case highContrast(SystemColors)

    enum CodingKeys: String, CodingKey {
        case highContrast
    }

    public init(from decoder: any Decoder) throws {
        if let name = try? decoder.singleValueContainer().decode(String.self) {
            switch name {
            case "light": self = .light
            case "dark": self = .dark
            default:
                throw DecodingError.dataCorrupted(.init(codingPath: decoder.codingPath, debugDescription: "unknown chrome theme \(name)"))
            }
            return
        }
        self = .highContrast(try decoder.container(keyedBy: CodingKeys.self).decode(SystemColors.self, forKey: .highContrast))
    }

    public func encode(to encoder: any Encoder) throws {
        let name: String
        switch self {
        case .light: name = "light"
        case .dark: name = "dark"
        case .highContrast(let colors):
            var c = encoder.container(keyedBy: CodingKeys.self)
            try c.encode(colors, forKey: .highContrast)
            return
        }
        var c = encoder.singleValueContainer()
        try c.encode(name)
    }
}

/// The platform's colours in high contrast mode.
public struct SystemColors: Hashable, Sendable, Codable {
    /// The window background.
    public var canvas: RGBColor
    /// Text on it.
    public var canvasText: RGBColor
    /// Selected items' background.
    public var highlight: RGBColor
    /// Text on it.
    public var highlightText: RGBColor
    /// Disabled and secondary text.
    public var grayText: RGBColor

    /// The platform's colours.
    public init(canvas: RGBColor, canvasText: RGBColor, highlight: RGBColor, highlightText: RGBColor, grayText: RGBColor) {
        self.canvas = canvas
        self.canvasText = canvasText
        self.highlight = highlight
        self.highlightText = highlightText
        self.grayText = grayText
    }
}

/// The host's colours for the light and the dark chrome scheme, to match its own UI: each scheme given replaces the
/// last one set for it (`{}` puts its own colours back). The high contrast scheme always takes the platform's.
public struct ThemeColors: Hashable, Sendable, Codable {
    /// For the light scheme.
    public var light: ChromeColors?
    /// For the dark scheme.
    public var dark: ChromeColors?

    /// Colours for either scheme.
    public init(light: ChromeColors? = nil, dark: ChromeColors? = nil) {
        self.light = light
        self.dark = dark
    }
}

/// Chrome colours in place of a scheme's own; each one left out keeps the scheme's.
public struct ChromeColors: Hashable, Sendable, Codable {
    /// Around the pages and the rulers' strips (a background the host sets wins around the pages).
    public var desk: RGBColor?
    /// The rulers' strips, and the readouts' and labels' backgrounds.
    public var surface: RGBColor?
    /// Text, and the ruler numbers and markers.
    public var text: RGBColor?
    /// The rulers' unit ticks, and secondary text.
    public var secondaryText: RGBColor?
    /// The rulers' fine ticks, the object grid, and faint text.
    public var faintText: RGBColor?
    /// The rulers' outlines and the margins' hatching.
    public var line: RGBColor?

    /// Chrome colours; those left nil keep the scheme's.
    public init(
        desk: RGBColor? = nil, surface: RGBColor? = nil, text: RGBColor? = nil, secondaryText: RGBColor? = nil,
        faintText: RGBColor? = nil, line: RGBColor? = nil
    ) {
        self.desk = desk
        self.surface = surface
        self.text = text
        self.secondaryText = secondaryText
        self.faintText = faintText
        self.line = line
    }
}

/// How the document's page is drawn on screen. Saving, PDF export and printing always use the document's colours.
public enum DocumentAppearance: String, Hashable, Sendable, Codable, CaseIterable {
    /// In the document's colours.
    case light
    /// Dark mode: a dark page, with the document's colours adjusted to it.
    case dark
    /// Dark mode with the page shown in its own colours (the chrome stays dark).
    case darkWithLightPage
}

/// How much the editor measures while you drag and hover.
public enum MeasureDetail: String, Hashable, Sendable, Codable, CaseIterable {
    /// A chip with the value only.
    case quiet
    /// Dimension lines with the value; the full set while Alt is held.
    case standard
    /// Everything: changes from the start, table sizes and status lines.
    case full
}

/// When the dot grid objects snap to shows.
public enum ObjectGrid: String, Hashable, Sendable, Codable, CaseIterable {
    /// Never.
    case off
    /// While an object is dragged.
    case whileDragging
    /// Always.
    case always
}

/// When fields are shaded: a grey backdrop behind each field's text on screen, never printed, exported or saved.
public enum FieldShading: String, Hashable, Sendable, Codable, CaseIterable {
    /// Never.
    case never
    /// Always: every field.
    case always
    /// The field the caret or selection is in or touches.
    case whenSelected
}

/// A visible page and where it is.
public struct PageRect: Hashable, Sendable, Codable {
    /// The page's index, from 0.
    public var page: Int
    /// Its box in the viewport.
    public var rect: ClientRect

    /// A visible page.
    public init(page: Int, rect: ClientRect) {
        self.page = page
        self.rect = rect
    }
}
