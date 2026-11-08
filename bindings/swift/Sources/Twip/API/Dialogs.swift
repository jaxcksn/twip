import Foundation

// Dialogs (crates/twip/src/api/services.rs): the dialogs the engine models, their values and patches, and the
// list galleries, previews and fields they use.
//
// Renamed from Rust: `ListEntryId` is `ListEntryID`. A `kind` field the wire calls `type` is `type` here
// (`StyleDialog.type`, `ManagedStyle.type`).

/// The dialogs the engine models.
public enum DialogKind: String, Hashable, Sendable, Codable, CaseIterable, CodingKeyRepresentable {
    /// Font.
    case font
    /// Paragraph.
    case paragraph
    /// Page Setup.
    case pageSetup
    /// Insert Table.
    case insertTable
    /// Table Properties.
    case tableProperties
    /// Picture Size.
    case pictureSize
    /// Alt Text.
    case altText
    /// Hyperlink.
    case hyperlink
    /// Tabs.
    case tabs
    /// The accessibility checker.
    case accessibility
    /// The AutoCorrect rules.
    case autoCorrect
    /// Modify Style, and Create New Style from Formatting (`dialogOpenStyle`).
    case style
    /// Manage Styles.
    case manageStyles
    /// Borders and Shading.
    case bordersShading
    /// Frame.
    case frame
    /// Banding (a table style's band sizes, from the style dialog's Format menu).
    case banding
    /// Define New Bullet.
    case defineBullet
    /// Define New Number Format.
    case defineNumberFormat
    /// Define New Multilevel List.
    case defineMultilevelList
    /// Set Numbering Value.
    case setNumberingValue
    /// Adjust List Indents.
    case adjustListIndents
    /// Bullets and Numbering.
    case bulletsNumbering

    /// The list dialogs, which read the selection's list.
    public var isList: Bool {
        switch self {
        case .defineBullet, .defineNumberFormat, .defineMultilevelList, .setNumberingValue, .adjustListIndents,
            .bulletsNumbering:
            true
        default: false
        }
    }
}

/// An open dialog: its values for the selection, and each field's label and state.
public struct DialogModel: Hashable, Sendable, Codable {
    /// Which dialog.
    public var kind: DialogKind
    /// Its values for the selection.
    public var values: DialogValues
    /// By field name (the values' camelCase keys).
    public var fields: [String: FieldInfo]
    /// A dialog opened from the style dialog's Format menu: the style dialog it changes, not the selection.
    public var onStyle: StyleDialog?

    /// The dialog `kind` with its values and fields.
    public init(kind: DialogKind, values: DialogValues, fields: [String: FieldInfo] = [:], onStyle: StyleDialog? = nil) {
        self.kind = kind
        self.values = values
        self.fields = fields
        self.onStyle = onStyle
    }
}

/// How the style dialog opens. On the wire: `"create"`, or `{"modify": id}`.
public enum StyleOpen: Hashable, Sendable, Codable {
    /// Create New Style from Formatting: a new style from the selection's formatting.
    case create
    /// Modify Style.
    case modify(StyleID)

    public init(from decoder: any Decoder) throws {
        if (try? decoder.singleValueContainer().decode(String.self)) == "create" {
            self = .create
        } else {
            self = .modify(try decoder.container(keyedBy: WireKey.self).decode(StyleID.self, forKey: "modify"))
        }
    }

    public func encode(to encoder: any Encoder) throws {
        switch self {
        case .create:
            var c = encoder.singleValueContainer()
            try c.encode("create")
        case .modify(let id):
            var c = encoder.container(keyedBy: WireKey.self)
            try c.encode(id, forKey: "modify")
        }
    }
}

/// The kind of style the style dialog makes.
public enum StyleDialogType: String, Hashable, Sendable, Codable, CaseIterable {
    /// A paragraph style.
    case paragraph
    /// A character style.
    case character
    /// A paragraph style and a character style, linked.
    case linked
    /// A table style.
    case table
    /// A list style.
    case list
}

/// The parts of a table a table style formats.
public enum TableStylePart: String, Hashable, Sendable, Codable, CaseIterable {
    /// The whole table.
    case wholeTable
    /// The header row.
    case headerRow
    /// The total row.
    case totalRow
    /// The first column.
    case firstColumn
    /// The last column.
    case lastColumn
    /// Odd banded rows.
    case oddBandedRows
    /// Even banded rows.
    case evenBandedRows
    /// Odd banded columns.
    case oddBandedColumns
    /// Even banded columns.
    case evenBandedColumns
    /// The top left cell.
    case topLeftCell
    /// The top right cell.
    case topRightCell
    /// The bottom left cell.
    case bottomLeftCell
    /// The bottom right cell.
    case bottomRightCell
}

/// Manage Styles' tabs.
public enum ManageStylesTab: String, Hashable, Sendable, Codable, CaseIterable {
    /// The styles, to modify, delete or add to.
    case edit
    /// Which styles show in lists, and in what order.
    case recommend
    /// Which styles formatting protection permits.
    case restrict
    /// The document's default formatting.
    case setDefaults
}

/// How Manage Styles sorts its list.
public enum StyleSort: String, Hashable, Sendable, Codable, CaseIterable {
    /// By priority, then name.
    case asRecommended
    /// By name.
    case alphabetical
    /// By the style each is based on.
    case basedOn
    /// By type.
    case byType
}

/// A style in Manage Styles.
public struct ManagedStyle: Hashable, Sendable, Codable, Identifiable {
    /// The style.
    public var id: StyleID
    /// Its name.
    public var name: String
    /// Its type.
    public var type: StyleType
    /// Built in (can't be deleted).
    public var builtIn: Bool
    /// Whether it shows in style lists.
    public var visibility: StyleVisibility
    /// Its sort priority.
    public var priority: Int
    /// Restricted while formatting is protected.
    public var locked: Bool

    /// The style `id`, as Manage Styles lists it.
    public init(
        id: StyleID, name: String = "", type: StyleType, builtIn: Bool = false, visibility: StyleVisibility,
        priority: Int = 0, locked: Bool = false
    ) {
        self.id = id
        self.name = name
        self.type = type
        self.builtIn = builtIn
        self.visibility = visibility
        self.priority = priority
        self.locked = locked
    }
}

/// Borders and Shading's tabs.
public enum BordersTab: String, Hashable, Sendable, Codable, CaseIterable {
    /// The paragraph's or text's borders.
    case borders
    /// The page's borders.
    case pageBorder
    /// The shading.
    case shading
}

/// Borders and Shading's settings: no borders, a box, a box with a shadow, or the edges set one by one.
public enum BorderSetting: String, Hashable, Sendable, Codable, CaseIterable {
    /// No borders.
    case none
    /// The four edges in one line.
    case box
    /// A box with a shadow on its bottom and right edges.
    case shadow
    /// The edges set one by one.
    case custom
}

/// A run of characters a font has: code points `first` to `last`.
public struct CharRange: Hashable, Sendable, Codable {
    /// The first code point.
    public var first: Int
    /// The last code point.
    public var last: Int

    /// The code points `first` to `last`.
    public init(first: Int, last: Int) {
        self.first = first
        self.last = last
    }
}

/// A list gallery: bullets, numbering, or multilevel lists.
public enum ListGalleryKind: String, Hashable, Sendable, Codable, CaseIterable {
    /// The bullet library.
    case bullet
    /// The numbering library.
    case numbering
    /// The multilevel list library.
    case multilevel
}

/// A section of a list gallery.
public enum ListGallerySectionKind: String, Hashable, Sendable, Codable, CaseIterable {
    /// The built-in library.
    case library
    /// The lists applied or defined in this session.
    case recentlyUsed
    /// The lists in the document.
    case document
    /// The document's list styles (the multilevel gallery).
    case listStyles
}

/// A list gallery entry, by its id (`applyListEntry`, `listEntryPreview`).
public struct ListEntryID: WireID {
    public let rawValue: String
    /// The entry `rawValue`.
    public init(rawValue: String) { self.rawValue = rawValue }
}

/// A list gallery entry: its id, a short label (its first levels' numbers or its bullet), and its levels.
public struct ListEntry: Hashable, Sendable, Codable, Identifiable {
    /// The entry.
    public var id: ListEntryID
    /// A short label, such as `1. a. i.` or a bullet's character.
    public var label: String
    /// Its levels.
    public var levels: [ListLevel]

    /// The entry `id`.
    public init(id: ListEntryID, label: String = "", levels: [ListLevel] = []) {
        self.id = id
        self.label = label
        self.levels = levels
    }
}

/// A section of a list gallery and its entries.
public struct ListGallerySection: Hashable, Sendable, Codable {
    /// Which section.
    public var kind: ListGallerySectionKind
    /// Its entries.
    public var entries: [ListEntry]

    /// The section `kind` with its entries.
    public init(kind: ListGallerySectionKind = .library, entries: [ListEntry] = []) {
        self.kind = kind
        self.entries = entries
    }
}

/// The size of a preview picture: CSS px (or device-independent pixels), and the device pixels per one.
public struct PreviewSize: Hashable, Sendable, Codable {
    /// The width.
    public var width: Double
    /// The height.
    public var height: Double
    /// Device pixels per px (the device pixel ratio). Default: 1.
    public var scale: Double

    /// A `width` x `height` px preview at `scale` device pixels per px.
    public init(width: Double, height: Double, scale: Double = 1) {
        self.width = width
        self.height = height
        self.scale = scale
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        width = try c.decode(Double.self, forKey: .width)
        height = try c.decode(Double.self, forKey: .height)
        scale = try c.decodeIfPresent(Double.self, forKey: .scale) ?? 1
    }
}

/// A picture the editor drew: a PNG of `width` x `height` device pixels.
public struct PreviewPicture: Hashable, Sendable, Codable {
    /// The PNG.
    public var png: Data
    /// Its width, device pixels.
    public var width: Int
    /// Its height, device pixels.
    public var height: Int

    /// The PNG `png` of `width` x `height` device pixels.
    public init(png: Data, width: Int, height: Int) {
        self.png = png
        self.width = width
        self.height = height
    }
}

/// How a dialog field is edited.
public enum FieldKind: String, Hashable, Sendable, Codable, CaseIterable {
    /// A plain number.
    case number
    /// A length in points, shown in the ruler unit.
    case length
    /// Text.
    case text
    /// One of `FieldInfo.options`.
    case choice
    /// A check box.
    case toggle
    /// A color.
    case color
    /// A list of values.
    case list
}

/// One choice of a choice field.
public struct ChoiceOption: Hashable, Sendable, Codable {
    /// The value the field takes.
    public var value: String
    /// The English label.
    public var label: String

    /// The choice `value`, labelled `label`.
    public init(value: String, label: String) {
        self.value = value
        self.label = label
    }
}

/// A dialog field's label, kind, range and state.
public struct FieldInfo: Hashable, Sendable, Codable {
    /// The English label.
    public var label: String
    /// Whether it can be changed now.
    public var enabled: Bool
    /// How it is edited.
    public var kind: FieldKind
    /// The smallest value.
    public var min: Double?
    /// The largest value.
    public var max: Double?
    /// The step.
    public var step: Double?
    /// A choice's options.
    public var options: [ChoiceOption]?

    /// The field labelled `label`.
    public init(
        label: String = "", enabled: Bool = false, kind: FieldKind = .number, min: Double? = nil, max: Double? = nil,
        step: Double? = nil, options: [ChoiceOption]? = nil
    ) {
        self.label = label
        self.enabled = enabled
        self.kind = kind
        self.min = min
        self.max = max
        self.step = step
        self.options = options
    }
}

/// The Font dialog's style box.
public enum FontStyle: String, Hashable, Sendable, Codable, CaseIterable {
    /// Regular.
    case regular
    /// Italic.
    case italic
    /// Bold.
    case bold
    /// Bold italic.
    case boldItalic
}

/// The Paragraph dialog's special indent.
public enum SpecialIndent: String, Hashable, Sendable, Codable, CaseIterable {
    /// No first-line indent.
    case none
    /// The first line indents further.
    case firstLine
    /// The lines after the first indent further.
    case hanging
}

/// Where page setup applies.
public enum ApplyTo: String, Hashable, Sendable, Codable, CaseIterable {
    /// The selection's section.
    case section
    /// Every section.
    case wholeDocument
}

/// Cell margins, all four set.
public struct CellMarginValues: Hashable, Sendable, Codable {
    /// The top margin.
    public var top: Points
    /// The bottom margin.
    public var bottom: Points
    /// The left margin.
    public var left: Points
    /// The right margin.
    public var right: Points

    /// The four margins.
    public init(top: Points = 0, bottom: Points = 0, left: Points = 0, right: Points = 0) {
        self.top = top
        self.bottom = bottom
        self.left = left
        self.right = right
    }
}

/// The Tabs dialog's buttons, applied by `update`.
public enum TabsAction: String, Hashable, Sendable, Codable, CaseIterable {
    /// No button.
    case none
    /// Set: adds or changes the stop at `position`.
    case set
    /// Clear: clears the stop at `position`.
    case clear
    /// Clear All.
    case clearAll
}

/// One accessibility issue.
public struct AccessibilityIssue: Hashable, Sendable, Codable, Identifiable {
    /// The issue's id.
    public var id: String
    /// Its description.
    public var text: String
    /// A tip rather than an error.
    public var tip: Bool

    /// The issue `id`, described by `text`.
    public init(id: String, text: String, tip: Bool = false) {
        self.id = id
        self.text = text
        self.tip = tip
    }
}

/// A dialog's values, tagged with its kind.
public enum DialogValues: Hashable, Sendable, Codable {
    /// The Font dialog's.
    case font(FontDialog)
    /// The Paragraph dialog's.
    case paragraph(ParagraphDialog)
    /// The Page Setup dialog's.
    case pageSetup(PageSetupDialog)
    /// The Insert Table dialog's.
    case insertTable(InsertTableDialog)
    /// The Table Properties dialog's.
    case tableProperties(TablePropertiesDialog)
    /// The Picture Size dialog's.
    case pictureSize(PictureSizeDialog)
    /// The Alt Text dialog's.
    case altText(AltTextDialog)
    /// The Hyperlink dialog's.
    case hyperlink(HyperlinkDialog)
    /// The Tabs dialog's.
    case tabs(TabsDialog)
    /// The accessibility checker's.
    case accessibility(AccessibilityDialog)
    /// The style dialog's.
    case style(StyleDialog)
    /// Manage Styles'.
    case manageStyles(ManageStylesDialog)
    /// Borders and Shading's.
    case bordersShading(BordersShadingDialog)
    /// The Frame dialog's.
    case frame(FrameDialog)
    /// Banding's.
    case banding(BandingDialog)
    /// Define New Bullet's.
    case defineBullet(DefineBulletDialog)
    /// Define New Number Format's.
    case defineNumberFormat(DefineNumberFormatDialog)
    /// Define New Multilevel List's.
    case defineMultilevelList(DefineMultilevelDialog)
    /// Set Numbering Value's.
    case setNumberingValue(SetNumberingValueDialog)
    /// Adjust List Indents's.
    case adjustListIndents(AdjustListIndentsDialog)
    /// Bullets and Numbering's.
    case bulletsNumbering(BulletsNumberingDialog)

    public init(from decoder: any Decoder) throws {
        let t = try TaggedDecoder(decoder, tag: "kind")
        switch t.tag {
        case "font": self = .font(try t.spread())
        case "paragraph": self = .paragraph(try t.spread())
        case "pageSetup": self = .pageSetup(try t.spread())
        case "insertTable": self = .insertTable(try t.spread())
        case "tableProperties": self = .tableProperties(try t.spread())
        case "pictureSize": self = .pictureSize(try t.spread())
        case "altText": self = .altText(try t.spread())
        case "hyperlink": self = .hyperlink(try t.spread())
        case "tabs": self = .tabs(try t.spread())
        case "accessibility": self = .accessibility(try t.spread())
        case "style": self = .style(try t.spread())
        case "manageStyles": self = .manageStyles(try t.spread())
        case "bordersShading": self = .bordersShading(try t.spread())
        case "frame": self = .frame(try t.spread())
        case "banding": self = .banding(try t.spread())
        case "defineBullet": self = .defineBullet(try t.spread())
        case "defineNumberFormat": self = .defineNumberFormat(try t.spread())
        case "defineMultilevelList": self = .defineMultilevelList(try t.spread())
        case "setNumberingValue": self = .setNumberingValue(try t.spread())
        case "adjustListIndents": self = .adjustListIndents(try t.spread())
        case "bulletsNumbering": self = .bulletsNumbering(try t.spread())
        default: throw t.unknown()
        }
    }

    public func encode(to encoder: any Encoder) throws {
        let t = TaggedEncoder(encoder, tag: "kind")
        switch self {
        case .font(let v): try t.tag("font", spread: v)
        case .paragraph(let v): try t.tag("paragraph", spread: v)
        case .pageSetup(let v): try t.tag("pageSetup", spread: v)
        case .insertTable(let v): try t.tag("insertTable", spread: v)
        case .tableProperties(let v): try t.tag("tableProperties", spread: v)
        case .pictureSize(let v): try t.tag("pictureSize", spread: v)
        case .altText(let v): try t.tag("altText", spread: v)
        case .hyperlink(let v): try t.tag("hyperlink", spread: v)
        case .tabs(let v): try t.tag("tabs", spread: v)
        case .accessibility(let v): try t.tag("accessibility", spread: v)
        case .style(let v): try t.tag("style", spread: v)
        case .manageStyles(let v): try t.tag("manageStyles", spread: v)
        case .bordersShading(let v): try t.tag("bordersShading", spread: v)
        case .frame(let v): try t.tag("frame", spread: v)
        case .banding(let v): try t.tag("banding", spread: v)
        case .defineBullet(let v): try t.tag("defineBullet", spread: v)
        case .defineNumberFormat(let v): try t.tag("defineNumberFormat", spread: v)
        case .defineMultilevelList(let v): try t.tag("defineMultilevelList", spread: v)
        case .setNumberingValue(let v): try t.tag("setNumberingValue", spread: v)
        case .adjustListIndents(let v): try t.tag("adjustListIndents", spread: v)
        case .bulletsNumbering(let v): try t.tag("bulletsNumbering", spread: v)
        }
    }
}

/// What `dialogUpdate` changes: the kind-tagged values with only the changed fields.
public enum DialogPatch: Hashable, Sendable, Codable {
    /// The Font dialog's.
    case font(FontPatch)
    /// The Paragraph dialog's.
    case paragraph(ParagraphPatch)
    /// The Page Setup dialog's.
    case pageSetup(PageSetupPatch)
    /// The Insert Table dialog's.
    case insertTable(InsertTablePatch)
    /// The Table Properties dialog's.
    case tableProperties(TablePropertiesPatch)
    /// The Picture Size dialog's.
    case pictureSize(PictureSizePatch)
    /// The Alt Text dialog's.
    case altText(AltTextPatch)
    /// The Hyperlink dialog's.
    case hyperlink(HyperlinkPatch)
    /// The Tabs dialog's.
    case tabs(TabsPatch)
    /// The accessibility checker's.
    case accessibility(AccessibilityPatch)
    /// The style dialog's.
    case style(StylePatch)
    /// Manage Styles'.
    case manageStyles(ManageStylesPatch)
    /// Borders and Shading's.
    case bordersShading(BordersShadingPatch)
    /// The Frame dialog's.
    case frame(FramePatch)
    /// Banding's.
    case banding(BandingPatch)
    /// Define New Bullet's.
    case defineBullet(DefineBulletPatch)
    /// Define New Number Format's.
    case defineNumberFormat(DefineNumberFormatPatch)
    /// Define New Multilevel List's.
    case defineMultilevelList(DefineMultilevelPatch)
    /// Set Numbering Value's.
    case setNumberingValue(SetNumberingValuePatch)
    /// Adjust List Indents's.
    case adjustListIndents(AdjustListIndentsPatch)
    /// Bullets and Numbering's.
    case bulletsNumbering(BulletsNumberingPatch)

    public init(from decoder: any Decoder) throws {
        let t = try TaggedDecoder(decoder, tag: "kind")
        switch t.tag {
        case "font": self = .font(try t.spread())
        case "paragraph": self = .paragraph(try t.spread())
        case "pageSetup": self = .pageSetup(try t.spread())
        case "insertTable": self = .insertTable(try t.spread())
        case "tableProperties": self = .tableProperties(try t.spread())
        case "pictureSize": self = .pictureSize(try t.spread())
        case "altText": self = .altText(try t.spread())
        case "hyperlink": self = .hyperlink(try t.spread())
        case "tabs": self = .tabs(try t.spread())
        case "accessibility": self = .accessibility(try t.spread())
        case "style": self = .style(try t.spread())
        case "manageStyles": self = .manageStyles(try t.spread())
        case "bordersShading": self = .bordersShading(try t.spread())
        case "frame": self = .frame(try t.spread())
        case "banding": self = .banding(try t.spread())
        case "defineBullet": self = .defineBullet(try t.spread())
        case "defineNumberFormat": self = .defineNumberFormat(try t.spread())
        case "defineMultilevelList": self = .defineMultilevelList(try t.spread())
        case "setNumberingValue": self = .setNumberingValue(try t.spread())
        case "adjustListIndents": self = .adjustListIndents(try t.spread())
        case "bulletsNumbering": self = .bulletsNumbering(try t.spread())
        default: throw t.unknown()
        }
    }

    public func encode(to encoder: any Encoder) throws {
        let t = TaggedEncoder(encoder, tag: "kind")
        switch self {
        case .font(let v): try t.tag("font", spread: v)
        case .paragraph(let v): try t.tag("paragraph", spread: v)
        case .pageSetup(let v): try t.tag("pageSetup", spread: v)
        case .insertTable(let v): try t.tag("insertTable", spread: v)
        case .tableProperties(let v): try t.tag("tableProperties", spread: v)
        case .pictureSize(let v): try t.tag("pictureSize", spread: v)
        case .altText(let v): try t.tag("altText", spread: v)
        case .hyperlink(let v): try t.tag("hyperlink", spread: v)
        case .tabs(let v): try t.tag("tabs", spread: v)
        case .accessibility(let v): try t.tag("accessibility", spread: v)
        case .style(let v): try t.tag("style", spread: v)
        case .manageStyles(let v): try t.tag("manageStyles", spread: v)
        case .bordersShading(let v): try t.tag("bordersShading", spread: v)
        case .frame(let v): try t.tag("frame", spread: v)
        case .banding(let v): try t.tag("banding", spread: v)
        case .defineBullet(let v): try t.tag("defineBullet", spread: v)
        case .defineNumberFormat(let v): try t.tag("defineNumberFormat", spread: v)
        case .defineMultilevelList(let v): try t.tag("defineMultilevelList", spread: v)
        case .setNumberingValue(let v): try t.tag("setNumberingValue", spread: v)
        case .adjustListIndents(let v): try t.tag("adjustListIndents", spread: v)
        case .bulletsNumbering(let v): try t.tag("bulletsNumbering", spread: v)
        }
    }
}

/// The Font dialog.
public struct FontDialog: Hashable, Sendable, Codable {
    /// The font name.
    public var font: Tri<String>
    /// Regular, italic, bold or both.
    public var style: Tri<FontStyle>
    /// The size.
    public var size: Tri<Points>
    /// The text color.
    public var color: Tri<AutoOr<RGBColor>>
    /// The underline.
    public var underline: Tri<UnderlineStyle>
    /// The underline's color.
    public var underlineColor: Tri<AutoOr<RGBColor>>
    /// Strikethrough.
    public var strikethrough: Tri<Bool>
    /// Double strikethrough.
    public var doubleStrikethrough: Tri<Bool>
    /// Superscript.
    public var superscript: Tri<Bool>
    /// Subscript.
    public var `subscript`: Tri<Bool>
    /// Small caps.
    public var smallCaps: Tri<Bool>
    /// All caps.
    public var allCaps: Tri<Bool>
    /// Hidden text.
    public var hidden: Tri<Bool>
    /// Extra space between characters; negative condenses.
    public var characterSpacing: Tri<Points>
    /// The baseline offset; positive raises.
    public var position: Tri<Points>

    /// The values.
    public init(
        font: Tri<String> = .unavailable,
        style: Tri<FontStyle> = .unavailable,
        size: Tri<Points> = .unavailable,
        color: Tri<AutoOr<RGBColor>> = .unavailable,
        underline: Tri<UnderlineStyle> = .unavailable,
        underlineColor: Tri<AutoOr<RGBColor>> = .unavailable,
        strikethrough: Tri<Bool> = .unavailable,
        doubleStrikethrough: Tri<Bool> = .unavailable,
        superscript: Tri<Bool> = .unavailable,
        `subscript`: Tri<Bool> = .unavailable,
        smallCaps: Tri<Bool> = .unavailable,
        allCaps: Tri<Bool> = .unavailable,
        hidden: Tri<Bool> = .unavailable,
        characterSpacing: Tri<Points> = .unavailable,
        position: Tri<Points> = .unavailable
    ) {
        self.font = font
        self.style = style
        self.size = size
        self.color = color
        self.underline = underline
        self.underlineColor = underlineColor
        self.strikethrough = strikethrough
        self.doubleStrikethrough = doubleStrikethrough
        self.superscript = superscript
        self.subscript = `subscript`
        self.smallCaps = smallCaps
        self.allCaps = allCaps
        self.hidden = hidden
        self.characterSpacing = characterSpacing
        self.position = position
    }
}

/// The fields of `FontDialog` that changed: each is nil when unchanged.
public struct FontPatch: Hashable, Sendable, Codable {
    /// The font name, when changed.
    public var font: Tri<String>?
    /// Regular, italic, bold or both, when changed.
    public var style: Tri<FontStyle>?
    /// The size, when changed.
    public var size: Tri<Points>?
    /// The text color, when changed.
    public var color: Tri<AutoOr<RGBColor>>?
    /// The underline, when changed.
    public var underline: Tri<UnderlineStyle>?
    /// The underline's color, when changed.
    public var underlineColor: Tri<AutoOr<RGBColor>>?
    /// Strikethrough, when changed.
    public var strikethrough: Tri<Bool>?
    /// Double strikethrough, when changed.
    public var doubleStrikethrough: Tri<Bool>?
    /// Superscript, when changed.
    public var superscript: Tri<Bool>?
    /// Subscript, when changed.
    public var `subscript`: Tri<Bool>?
    /// Small caps, when changed.
    public var smallCaps: Tri<Bool>?
    /// All caps, when changed.
    public var allCaps: Tri<Bool>?
    /// Hidden text, when changed.
    public var hidden: Tri<Bool>?
    /// Extra space between characters; negative condenses, when changed.
    public var characterSpacing: Tri<Points>?
    /// The baseline offset; positive raises, when changed.
    public var position: Tri<Points>?

    /// A patch changing the given fields.
    public init(
        font: Tri<String>? = nil,
        style: Tri<FontStyle>? = nil,
        size: Tri<Points>? = nil,
        color: Tri<AutoOr<RGBColor>>? = nil,
        underline: Tri<UnderlineStyle>? = nil,
        underlineColor: Tri<AutoOr<RGBColor>>? = nil,
        strikethrough: Tri<Bool>? = nil,
        doubleStrikethrough: Tri<Bool>? = nil,
        superscript: Tri<Bool>? = nil,
        `subscript`: Tri<Bool>? = nil,
        smallCaps: Tri<Bool>? = nil,
        allCaps: Tri<Bool>? = nil,
        hidden: Tri<Bool>? = nil,
        characterSpacing: Tri<Points>? = nil,
        position: Tri<Points>? = nil
    ) {
        self.font = font
        self.style = style
        self.size = size
        self.color = color
        self.underline = underline
        self.underlineColor = underlineColor
        self.strikethrough = strikethrough
        self.doubleStrikethrough = doubleStrikethrough
        self.superscript = superscript
        self.subscript = `subscript`
        self.smallCaps = smallCaps
        self.allCaps = allCaps
        self.hidden = hidden
        self.characterSpacing = characterSpacing
        self.position = position
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        font = c.contains(.font) ? .some(try c.decode(Tri<String>.self, forKey: .font)) : nil
        style = c.contains(.style) ? .some(try c.decode(Tri<FontStyle>.self, forKey: .style)) : nil
        size = c.contains(.size) ? .some(try c.decode(Tri<Points>.self, forKey: .size)) : nil
        color = c.contains(.color) ? .some(try c.decode(Tri<AutoOr<RGBColor>>.self, forKey: .color)) : nil
        underline = c.contains(.underline) ? .some(try c.decode(Tri<UnderlineStyle>.self, forKey: .underline)) : nil
        underlineColor = c.contains(.underlineColor) ? .some(try c.decode(Tri<AutoOr<RGBColor>>.self, forKey: .underlineColor)) : nil
        strikethrough = c.contains(.strikethrough) ? .some(try c.decode(Tri<Bool>.self, forKey: .strikethrough)) : nil
        doubleStrikethrough = c.contains(.doubleStrikethrough) ? .some(try c.decode(Tri<Bool>.self, forKey: .doubleStrikethrough)) : nil
        superscript = c.contains(.superscript) ? .some(try c.decode(Tri<Bool>.self, forKey: .superscript)) : nil
        `subscript` = c.contains(.`subscript`) ? .some(try c.decode(Tri<Bool>.self, forKey: .`subscript`)) : nil
        smallCaps = c.contains(.smallCaps) ? .some(try c.decode(Tri<Bool>.self, forKey: .smallCaps)) : nil
        allCaps = c.contains(.allCaps) ? .some(try c.decode(Tri<Bool>.self, forKey: .allCaps)) : nil
        hidden = c.contains(.hidden) ? .some(try c.decode(Tri<Bool>.self, forKey: .hidden)) : nil
        characterSpacing = c.contains(.characterSpacing) ? .some(try c.decode(Tri<Points>.self, forKey: .characterSpacing)) : nil
        position = c.contains(.position) ? .some(try c.decode(Tri<Points>.self, forKey: .position)) : nil
    }
}

/// The Paragraph dialog.
public struct ParagraphDialog: Hashable, Sendable, Codable {
    /// The alignment.
    public var alignment: Tri<ParagraphAlignment>
    /// The outline level.
    public var outlineLevel: Tri<OutlineLevel>
    /// The left indent.
    public var left: Tri<Points>
    /// The right indent.
    public var right: Tri<Points>
    /// The special indent.
    public var special: Tri<SpecialIndent>
    /// The special indent's size.
    public var by: Tri<Points>
    /// The space before.
    public var before: Tri<AutoOr<Points>>
    /// The space after.
    public var after: Tri<AutoOr<Points>>
    /// The line spacing.
    public var lineSpacing: Tri<LineSpacing>
    /// Keep with next.
    public var keepWithNext: Tri<Bool>
    /// Keep lines together.
    public var keepLinesTogether: Tri<Bool>
    /// Page break before.
    public var pageBreakBefore: Tri<Bool>
    /// Widow and orphan control.
    public var widowControl: Tri<Bool>

    /// The values.
    public init(
        alignment: Tri<ParagraphAlignment> = .unavailable,
        outlineLevel: Tri<OutlineLevel> = .unavailable,
        left: Tri<Points> = .unavailable,
        right: Tri<Points> = .unavailable,
        special: Tri<SpecialIndent> = .unavailable,
        by: Tri<Points> = .unavailable,
        before: Tri<AutoOr<Points>> = .unavailable,
        after: Tri<AutoOr<Points>> = .unavailable,
        lineSpacing: Tri<LineSpacing> = .unavailable,
        keepWithNext: Tri<Bool> = .unavailable,
        keepLinesTogether: Tri<Bool> = .unavailable,
        pageBreakBefore: Tri<Bool> = .unavailable,
        widowControl: Tri<Bool> = .unavailable
    ) {
        self.alignment = alignment
        self.outlineLevel = outlineLevel
        self.left = left
        self.right = right
        self.special = special
        self.by = by
        self.before = before
        self.after = after
        self.lineSpacing = lineSpacing
        self.keepWithNext = keepWithNext
        self.keepLinesTogether = keepLinesTogether
        self.pageBreakBefore = pageBreakBefore
        self.widowControl = widowControl
    }
}

/// The fields of `ParagraphDialog` that changed: each is nil when unchanged.
public struct ParagraphPatch: Hashable, Sendable, Codable {
    /// The alignment, when changed.
    public var alignment: Tri<ParagraphAlignment>?
    /// The outline level, when changed.
    public var outlineLevel: Tri<OutlineLevel>?
    /// The left indent, when changed.
    public var left: Tri<Points>?
    /// The right indent, when changed.
    public var right: Tri<Points>?
    /// The special indent, when changed.
    public var special: Tri<SpecialIndent>?
    /// The special indent's size, when changed.
    public var by: Tri<Points>?
    /// The space before, when changed.
    public var before: Tri<AutoOr<Points>>?
    /// The space after, when changed.
    public var after: Tri<AutoOr<Points>>?
    /// The line spacing, when changed.
    public var lineSpacing: Tri<LineSpacing>?
    /// Keep with next, when changed.
    public var keepWithNext: Tri<Bool>?
    /// Keep lines together, when changed.
    public var keepLinesTogether: Tri<Bool>?
    /// Page break before, when changed.
    public var pageBreakBefore: Tri<Bool>?
    /// Widow and orphan control, when changed.
    public var widowControl: Tri<Bool>?

    /// A patch changing the given fields.
    public init(
        alignment: Tri<ParagraphAlignment>? = nil,
        outlineLevel: Tri<OutlineLevel>? = nil,
        left: Tri<Points>? = nil,
        right: Tri<Points>? = nil,
        special: Tri<SpecialIndent>? = nil,
        by: Tri<Points>? = nil,
        before: Tri<AutoOr<Points>>? = nil,
        after: Tri<AutoOr<Points>>? = nil,
        lineSpacing: Tri<LineSpacing>? = nil,
        keepWithNext: Tri<Bool>? = nil,
        keepLinesTogether: Tri<Bool>? = nil,
        pageBreakBefore: Tri<Bool>? = nil,
        widowControl: Tri<Bool>? = nil
    ) {
        self.alignment = alignment
        self.outlineLevel = outlineLevel
        self.left = left
        self.right = right
        self.special = special
        self.by = by
        self.before = before
        self.after = after
        self.lineSpacing = lineSpacing
        self.keepWithNext = keepWithNext
        self.keepLinesTogether = keepLinesTogether
        self.pageBreakBefore = pageBreakBefore
        self.widowControl = widowControl
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        alignment = c.contains(.alignment) ? .some(try c.decode(Tri<ParagraphAlignment>.self, forKey: .alignment)) : nil
        outlineLevel = c.contains(.outlineLevel) ? .some(try c.decode(Tri<OutlineLevel>.self, forKey: .outlineLevel)) : nil
        left = c.contains(.left) ? .some(try c.decode(Tri<Points>.self, forKey: .left)) : nil
        right = c.contains(.right) ? .some(try c.decode(Tri<Points>.self, forKey: .right)) : nil
        special = c.contains(.special) ? .some(try c.decode(Tri<SpecialIndent>.self, forKey: .special)) : nil
        by = c.contains(.by) ? .some(try c.decode(Tri<Points>.self, forKey: .by)) : nil
        before = c.contains(.before) ? .some(try c.decode(Tri<AutoOr<Points>>.self, forKey: .before)) : nil
        after = c.contains(.after) ? .some(try c.decode(Tri<AutoOr<Points>>.self, forKey: .after)) : nil
        lineSpacing = c.contains(.lineSpacing) ? .some(try c.decode(Tri<LineSpacing>.self, forKey: .lineSpacing)) : nil
        keepWithNext = c.contains(.keepWithNext) ? .some(try c.decode(Tri<Bool>.self, forKey: .keepWithNext)) : nil
        keepLinesTogether = c.contains(.keepLinesTogether) ? .some(try c.decode(Tri<Bool>.self, forKey: .keepLinesTogether)) : nil
        pageBreakBefore = c.contains(.pageBreakBefore) ? .some(try c.decode(Tri<Bool>.self, forKey: .pageBreakBefore)) : nil
        widowControl = c.contains(.widowControl) ? .some(try c.decode(Tri<Bool>.self, forKey: .widowControl)) : nil
    }
}

/// The Page Setup dialog.
public struct PageSetupDialog: Hashable, Sendable, Codable {
    /// The top margin.
    public var top: Points
    /// The bottom margin.
    public var bottom: Points
    /// The left margin.
    public var left: Points
    /// The right margin.
    public var right: Points
    /// The gutter.
    public var gutter: Points
    /// Portrait or landscape.
    public var orientation: Orientation
    /// The paper width.
    public var width: Points
    /// The paper height.
    public var height: Points
    /// The header's distance from the page's top edge.
    public var headerDistance: Points
    /// The footer's distance from the page's bottom edge.
    public var footerDistance: Points
    /// A different first-page header and footer.
    public var differentFirstPage: Bool
    /// Different odd and even headers and footers.
    public var differentOddEven: Bool
    /// Where it applies.
    public var applyTo: ApplyTo

    /// The values.
    public init(
        top: Points = 0,
        bottom: Points = 0,
        left: Points = 0,
        right: Points = 0,
        gutter: Points = 0,
        orientation: Orientation,
        width: Points = 0,
        height: Points = 0,
        headerDistance: Points = 0,
        footerDistance: Points = 0,
        differentFirstPage: Bool = false,
        differentOddEven: Bool = false,
        applyTo: ApplyTo = .section
    ) {
        self.top = top
        self.bottom = bottom
        self.left = left
        self.right = right
        self.gutter = gutter
        self.orientation = orientation
        self.width = width
        self.height = height
        self.headerDistance = headerDistance
        self.footerDistance = footerDistance
        self.differentFirstPage = differentFirstPage
        self.differentOddEven = differentOddEven
        self.applyTo = applyTo
    }
}

/// The fields of `PageSetupDialog` that changed: each is nil when unchanged.
public struct PageSetupPatch: Hashable, Sendable, Codable {
    /// The top margin, when changed.
    public var top: Points?
    /// The bottom margin, when changed.
    public var bottom: Points?
    /// The left margin, when changed.
    public var left: Points?
    /// The right margin, when changed.
    public var right: Points?
    /// The gutter, when changed.
    public var gutter: Points?
    /// Portrait or landscape, when changed.
    public var orientation: Orientation?
    /// The paper width, when changed.
    public var width: Points?
    /// The paper height, when changed.
    public var height: Points?
    /// The header's distance from the page's top edge, when changed.
    public var headerDistance: Points?
    /// The footer's distance from the page's bottom edge, when changed.
    public var footerDistance: Points?
    /// A different first-page header and footer, when changed.
    public var differentFirstPage: Bool?
    /// Different odd and even headers and footers, when changed.
    public var differentOddEven: Bool?
    /// Where it applies, when changed.
    public var applyTo: ApplyTo?

    /// A patch changing the given fields.
    public init(
        top: Points? = nil,
        bottom: Points? = nil,
        left: Points? = nil,
        right: Points? = nil,
        gutter: Points? = nil,
        orientation: Orientation? = nil,
        width: Points? = nil,
        height: Points? = nil,
        headerDistance: Points? = nil,
        footerDistance: Points? = nil,
        differentFirstPage: Bool? = nil,
        differentOddEven: Bool? = nil,
        applyTo: ApplyTo? = nil
    ) {
        self.top = top
        self.bottom = bottom
        self.left = left
        self.right = right
        self.gutter = gutter
        self.orientation = orientation
        self.width = width
        self.height = height
        self.headerDistance = headerDistance
        self.footerDistance = footerDistance
        self.differentFirstPage = differentFirstPage
        self.differentOddEven = differentOddEven
        self.applyTo = applyTo
    }
}

/// The Insert Table dialog.
public struct InsertTableDialog: Hashable, Sendable, Codable {
    /// The number of rows.
    public var rows: Int
    /// The number of columns.
    public var columns: Int

    /// The values.
    public init(
        rows: Int = 0,
        columns: Int = 0
    ) {
        self.rows = rows
        self.columns = columns
    }
}

/// The fields of `InsertTableDialog` that changed: each is nil when unchanged.
public struct InsertTablePatch: Hashable, Sendable, Codable {
    /// The number of rows, when changed.
    public var rows: Int?
    /// The number of columns, when changed.
    public var columns: Int?

    /// A patch changing the given fields.
    public init(
        rows: Int? = nil,
        columns: Int? = nil
    ) {
        self.rows = rows
        self.columns = columns
    }
}

/// The Table Properties dialog.
public struct TablePropertiesDialog: Hashable, Sendable, Codable {
    /// The table's preferred width.
    public var width: TableWidth
    /// The table's alignment on the page.
    public var alignment: HorizontalAlign
    /// The table's indent from the left margin.
    public var indent: Points
    /// The selected rows' height.
    public var rowHeight: Tri<AutoOr<RowHeight>>
    /// Whether the selected rows can break across pages.
    public var rowCanBreak: Tri<Bool>
    /// Whether the selected rows repeat as a header on each page.
    public var headerRow: Tri<Bool>
    /// The selected columns' width.
    public var columnWidth: Tri<Points>
    /// The selected cells' vertical alignment.
    public var cellAlignment: Tri<CellAlignment>
    /// The table's default cell margins.
    public var cellMargins: CellMarginValues

    /// The values.
    public init(
        width: TableWidth,
        alignment: HorizontalAlign,
        indent: Points = 0,
        rowHeight: Tri<AutoOr<RowHeight>> = .unavailable,
        rowCanBreak: Tri<Bool> = .unavailable,
        headerRow: Tri<Bool> = .unavailable,
        columnWidth: Tri<Points> = .unavailable,
        cellAlignment: Tri<CellAlignment> = .unavailable,
        cellMargins: CellMarginValues = CellMarginValues()
    ) {
        self.width = width
        self.alignment = alignment
        self.indent = indent
        self.rowHeight = rowHeight
        self.rowCanBreak = rowCanBreak
        self.headerRow = headerRow
        self.columnWidth = columnWidth
        self.cellAlignment = cellAlignment
        self.cellMargins = cellMargins
    }
}

/// The fields of `TablePropertiesDialog` that changed: each is nil when unchanged.
public struct TablePropertiesPatch: Hashable, Sendable, Codable {
    /// The table's preferred width, when changed.
    public var width: TableWidth?
    /// The table's alignment on the page, when changed.
    public var alignment: HorizontalAlign?
    /// The table's indent from the left margin, when changed.
    public var indent: Points?
    /// The selected rows' height, when changed.
    public var rowHeight: Tri<AutoOr<RowHeight>>?
    /// Whether the selected rows can break across pages, when changed.
    public var rowCanBreak: Tri<Bool>?
    /// Whether the selected rows repeat as a header on each page, when changed.
    public var headerRow: Tri<Bool>?
    /// The selected columns' width, when changed.
    public var columnWidth: Tri<Points>?
    /// The selected cells' vertical alignment, when changed.
    public var cellAlignment: Tri<CellAlignment>?
    /// The table's default cell margins, when changed.
    public var cellMargins: CellMarginValues?

    /// A patch changing the given fields.
    public init(
        width: TableWidth? = nil,
        alignment: HorizontalAlign? = nil,
        indent: Points? = nil,
        rowHeight: Tri<AutoOr<RowHeight>>? = nil,
        rowCanBreak: Tri<Bool>? = nil,
        headerRow: Tri<Bool>? = nil,
        columnWidth: Tri<Points>? = nil,
        cellAlignment: Tri<CellAlignment>? = nil,
        cellMargins: CellMarginValues? = nil
    ) {
        self.width = width
        self.alignment = alignment
        self.indent = indent
        self.rowHeight = rowHeight
        self.rowCanBreak = rowCanBreak
        self.headerRow = headerRow
        self.columnWidth = columnWidth
        self.cellAlignment = cellAlignment
        self.cellMargins = cellMargins
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        width = try c.decodeIfPresent(TableWidth.self, forKey: .width)
        alignment = try c.decodeIfPresent(HorizontalAlign.self, forKey: .alignment)
        indent = try c.decodeIfPresent(Points.self, forKey: .indent)
        rowHeight = c.contains(.rowHeight) ? .some(try c.decode(Tri<AutoOr<RowHeight>>.self, forKey: .rowHeight)) : nil
        rowCanBreak = c.contains(.rowCanBreak) ? .some(try c.decode(Tri<Bool>.self, forKey: .rowCanBreak)) : nil
        headerRow = c.contains(.headerRow) ? .some(try c.decode(Tri<Bool>.self, forKey: .headerRow)) : nil
        columnWidth = c.contains(.columnWidth) ? .some(try c.decode(Tri<Points>.self, forKey: .columnWidth)) : nil
        cellAlignment = c.contains(.cellAlignment) ? .some(try c.decode(Tri<CellAlignment>.self, forKey: .cellAlignment)) : nil
        cellMargins = try c.decodeIfPresent(CellMarginValues.self, forKey: .cellMargins)
    }
}

/// The Picture Size dialog.
public struct PictureSizeDialog: Hashable, Sendable, Codable {
    /// The width.
    public var width: Points
    /// The height.
    public var height: Points
    /// Percent of the original size.
    public var scaleWidth: Double
    /// Percent of the original height.
    public var scaleHeight: Double
    /// Width and height change together.
    public var lockAspect: Bool
    /// Back to the original size on apply.
    public var reset: Bool

    /// The values.
    public init(
        width: Points = 0,
        height: Points = 0,
        scaleWidth: Double = 0,
        scaleHeight: Double = 0,
        lockAspect: Bool = false,
        reset: Bool = false
    ) {
        self.width = width
        self.height = height
        self.scaleWidth = scaleWidth
        self.scaleHeight = scaleHeight
        self.lockAspect = lockAspect
        self.reset = reset
    }
}

/// The fields of `PictureSizeDialog` that changed: each is nil when unchanged.
public struct PictureSizePatch: Hashable, Sendable, Codable {
    /// The width, when changed.
    public var width: Points?
    /// The height, when changed.
    public var height: Points?
    /// Percent of the original size, when changed.
    public var scaleWidth: Double?
    /// Percent of the original height, when changed.
    public var scaleHeight: Double?
    /// Width and height change together, when changed.
    public var lockAspect: Bool?
    /// Back to the original size on apply, when changed.
    public var reset: Bool?

    /// A patch changing the given fields.
    public init(
        width: Points? = nil,
        height: Points? = nil,
        scaleWidth: Double? = nil,
        scaleHeight: Double? = nil,
        lockAspect: Bool? = nil,
        reset: Bool? = nil
    ) {
        self.width = width
        self.height = height
        self.scaleWidth = scaleWidth
        self.scaleHeight = scaleHeight
        self.lockAspect = lockAspect
        self.reset = reset
    }
}

/// The Alt Text dialog.
public struct AltTextDialog: Hashable, Sendable, Codable {
    /// The description.
    public var text: String
    /// Marked decorative: screen readers skip it.
    public var decorative: Bool

    /// The values.
    public init(
        text: String = "",
        decorative: Bool = false
    ) {
        self.text = text
        self.decorative = decorative
    }
}

/// The fields of `AltTextDialog` that changed: each is nil when unchanged.
public struct AltTextPatch: Hashable, Sendable, Codable {
    /// The description, when changed.
    public var text: String?
    /// Marked decorative: screen readers skip it, when changed.
    public var decorative: Bool?

    /// A patch changing the given fields.
    public init(
        text: String? = nil,
        decorative: Bool? = nil
    ) {
        self.text = text
        self.decorative = decorative
    }
}

/// The Hyperlink dialog.
public struct HyperlinkDialog: Hashable, Sendable, Codable {
    /// The text to display.
    public var text: String
    /// The address.
    public var address: String

    /// The values.
    public init(
        text: String = "",
        address: String = ""
    ) {
        self.text = text
        self.address = address
    }
}

/// The fields of `HyperlinkDialog` that changed: each is nil when unchanged.
public struct HyperlinkPatch: Hashable, Sendable, Codable {
    /// The text to display, when changed.
    public var text: String?
    /// The address, when changed.
    public var address: String?

    /// A patch changing the given fields.
    public init(
        text: String? = nil,
        address: String? = nil
    ) {
        self.text = text
        self.address = address
    }
}

/// The Tabs dialog.
public struct TabsDialog: Hashable, Sendable, Codable {
    /// The paragraph's tab stops.
    public var stops: [TabStopInfo]
    /// The document's default tab interval.
    public var defaultStop: Points
    /// The stop being edited.
    public var position: Points?
    /// The edited stop's alignment.
    public var alignment: TabAlignment
    /// The edited stop's leader.
    public var leader: TabLeader
    /// Stops to clear on apply.
    public var cleared: [Points]
    /// The button pressed.
    public var action: TabsAction

    /// The values.
    public init(
        stops: [TabStopInfo] = [],
        defaultStop: Points = 0,
        position: Points? = nil,
        alignment: TabAlignment,
        leader: TabLeader,
        cleared: [Points] = [],
        action: TabsAction = .none
    ) {
        self.stops = stops
        self.defaultStop = defaultStop
        self.position = position
        self.alignment = alignment
        self.leader = leader
        self.cleared = cleared
        self.action = action
    }
}

/// The fields of `TabsDialog` that changed: each is nil when unchanged.
public struct TabsPatch: Hashable, Sendable, Codable {
    /// The paragraph's tab stops, when changed.
    public var stops: [TabStopInfo]?
    /// The document's default tab interval, when changed.
    public var defaultStop: Points?
    /// The stop being edited, when changed.
    public var position: Points??
    /// The edited stop's alignment, when changed.
    public var alignment: TabAlignment?
    /// The edited stop's leader, when changed.
    public var leader: TabLeader?
    /// Stops to clear on apply, when changed.
    public var cleared: [Points]?
    /// The button pressed, when changed.
    public var action: TabsAction?

    /// A patch changing the given fields.
    public init(
        stops: [TabStopInfo]? = nil,
        defaultStop: Points? = nil,
        position: Points?? = nil,
        alignment: TabAlignment? = nil,
        leader: TabLeader? = nil,
        cleared: [Points]? = nil,
        action: TabsAction? = nil
    ) {
        self.stops = stops
        self.defaultStop = defaultStop
        self.position = position
        self.alignment = alignment
        self.leader = leader
        self.cleared = cleared
        self.action = action
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        stops = try c.decodeIfPresent([TabStopInfo].self, forKey: .stops)
        defaultStop = try c.decodeIfPresent(Points.self, forKey: .defaultStop)
        position = c.contains(.position) ? .some(try c.decode(Points?.self, forKey: .position)) : nil
        alignment = try c.decodeIfPresent(TabAlignment.self, forKey: .alignment)
        leader = try c.decodeIfPresent(TabLeader.self, forKey: .leader)
        cleared = try c.decodeIfPresent([Points].self, forKey: .cleared)
        action = try c.decodeIfPresent(TabsAction.self, forKey: .action)
    }
}

/// The Accessibility dialog (read only).
public struct AccessibilityDialog: Hashable, Sendable, Codable {
    /// The issues found.
    public var issues: [AccessibilityIssue]

    /// The values.
    public init(
        issues: [AccessibilityIssue] = []
    ) {
        self.issues = issues
    }
}

/// The fields of `AccessibilityDialog` that changed: each is nil when unchanged.
public struct AccessibilityPatch: Hashable, Sendable, Codable {
    /// The issues found, when changed.
    public var issues: [AccessibilityIssue]?

    /// A patch changing the given fields.
    public init(
        issues: [AccessibilityIssue]? = nil
    ) {
        self.issues = issues
    }
}

/// The style dialog: Modify Style, and Create New Style from Formatting (`shortForm`: the name and the preview).
public struct StyleDialog: Hashable, Sendable, Codable {
    /// Creating a style, or modifying one.
    public var open: StyleOpen
    /// The short form: the name and the preview, until "Modify..." turns it off.
    public var shortForm: Bool
    /// The style's name.
    public var name: String
    /// Its type (chosen when creating).
    public var type: StyleDialogType
    /// The style it is based on.
    public var basedOn: StyleID?
    /// The style of the paragraph after one in this style.
    public var next: StyleID?
    /// The font.
    public var font: String
    /// The size.
    public var size: Points
    /// Bold.
    public var bold: Bool
    /// Italic.
    public var italic: Bool
    /// The underline.
    public var underline: UnderlineStyle
    /// The font color.
    public var color: AutoOr<RGBColor>
    /// The alignment.
    public var alignment: ParagraphAlignment
    /// The line spacing.
    public var lineSpacing: LineSpacing
    /// The space before paragraphs.
    public var spaceBefore: Points
    /// The space after paragraphs.
    public var spaceAfter: Points
    /// The left indent.
    public var leftIndent: Points
    /// Shown in the Styles gallery.
    public var quick: Bool
    /// Direct formatting of a paragraph in the style changes the style.
    public var autoUpdate: Bool
    /// A table style: the part the formatting applies to.
    public var tablePart: TableStylePart
    /// A table style: the part's cell fill, or none.
    public var fill: RGBColor?
    /// A table style: the part's cells' vertical alignment.
    public var cellAlignment: CellAlignment
    /// A table style: the part's border line, on every edge, or none.
    public var border: Border?
    /// A table style: the rows in each band.
    public var rowBand: Int
    /// A table style: the columns in each band.
    public var columnBand: Int
    /// A list style: its levels.
    public var listLevels: [ListLevel]
    /// A list style: the level shown (0 to 8).
    public var listLevel: Int
    /// The style's formatting in words (written by the editor).
    public var description: String
    /// The style as changed so far, Format menu dialogs included: give it back unchanged.
    public var pending: String

    /// The values.
    public init(
        open: StyleOpen = .create,
        shortForm: Bool = false,
        name: String = "",
        type: StyleDialogType = .paragraph,
        basedOn: StyleID? = nil,
        next: StyleID? = nil,
        font: String = "",
        size: Points = 0,
        bold: Bool = false,
        italic: Bool = false,
        underline: UnderlineStyle,
        color: AutoOr<RGBColor> = .auto,
        alignment: ParagraphAlignment,
        lineSpacing: LineSpacing,
        spaceBefore: Points = 0,
        spaceAfter: Points = 0,
        leftIndent: Points = 0,
        quick: Bool = false,
        autoUpdate: Bool = false,
        tablePart: TableStylePart = .wholeTable,
        fill: RGBColor? = nil,
        cellAlignment: CellAlignment,
        border: Border? = nil,
        rowBand: Int = 0,
        columnBand: Int = 0,
        listLevels: [ListLevel] = [],
        listLevel: Int = 0,
        description: String = "",
        pending: String = ""
    ) {
        self.open = open
        self.shortForm = shortForm
        self.name = name
        self.type = type
        self.basedOn = basedOn
        self.next = next
        self.font = font
        self.size = size
        self.bold = bold
        self.italic = italic
        self.underline = underline
        self.color = color
        self.alignment = alignment
        self.lineSpacing = lineSpacing
        self.spaceBefore = spaceBefore
        self.spaceAfter = spaceAfter
        self.leftIndent = leftIndent
        self.quick = quick
        self.autoUpdate = autoUpdate
        self.tablePart = tablePart
        self.fill = fill
        self.cellAlignment = cellAlignment
        self.border = border
        self.rowBand = rowBand
        self.columnBand = columnBand
        self.listLevels = listLevels
        self.listLevel = listLevel
        self.description = description
        self.pending = pending
    }
}

/// The fields of `StyleDialog` that changed: each is nil when unchanged.
public struct StylePatch: Hashable, Sendable, Codable {
    /// Creating a style, or modifying one, when changed.
    public var open: StyleOpen?
    /// The short form: the name and the preview, until "Modify..." turns it off, when changed.
    public var shortForm: Bool?
    /// The style's name, when changed.
    public var name: String?
    /// Its type (chosen when creating), when changed.
    public var type: StyleDialogType?
    /// The style it is based on, when changed.
    public var basedOn: StyleID??
    /// The style of the paragraph after one in this style, when changed.
    public var next: StyleID??
    /// The font, when changed.
    public var font: String?
    /// The size, when changed.
    public var size: Points?
    /// Bold, when changed.
    public var bold: Bool?
    /// Italic, when changed.
    public var italic: Bool?
    /// The underline, when changed.
    public var underline: UnderlineStyle?
    /// The font color, when changed.
    public var color: AutoOr<RGBColor>?
    /// The alignment, when changed.
    public var alignment: ParagraphAlignment?
    /// The line spacing, when changed.
    public var lineSpacing: LineSpacing?
    /// The space before paragraphs, when changed.
    public var spaceBefore: Points?
    /// The space after paragraphs, when changed.
    public var spaceAfter: Points?
    /// The left indent, when changed.
    public var leftIndent: Points?
    /// Shown in the Styles gallery, when changed.
    public var quick: Bool?
    /// Direct formatting of a paragraph in the style changes the style, when changed.
    public var autoUpdate: Bool?
    /// A table style: the part the formatting applies to, when changed.
    public var tablePart: TableStylePart?
    /// A table style: the part's cell fill, or none, when changed.
    public var fill: RGBColor??
    /// A table style: the part's cells' vertical alignment, when changed.
    public var cellAlignment: CellAlignment?
    /// A table style: the part's border line, on every edge, or none, when changed.
    public var border: Border??
    /// A table style: the rows in each band, when changed.
    public var rowBand: Int?
    /// A table style: the columns in each band, when changed.
    public var columnBand: Int?
    /// A list style: its levels, when changed.
    public var listLevels: [ListLevel]?
    /// A list style: the level shown (0 to 8), when changed.
    public var listLevel: Int?
    /// The style's formatting in words (written by the editor), when changed.
    public var description: String?
    /// The style as changed so far, Format menu dialogs included: give it back unchanged, when changed.
    public var pending: String?

    /// A patch changing the given fields.
    public init(
        open: StyleOpen? = nil,
        shortForm: Bool? = nil,
        name: String? = nil,
        type: StyleDialogType? = nil,
        basedOn: StyleID?? = nil,
        next: StyleID?? = nil,
        font: String? = nil,
        size: Points? = nil,
        bold: Bool? = nil,
        italic: Bool? = nil,
        underline: UnderlineStyle? = nil,
        color: AutoOr<RGBColor>? = nil,
        alignment: ParagraphAlignment? = nil,
        lineSpacing: LineSpacing? = nil,
        spaceBefore: Points? = nil,
        spaceAfter: Points? = nil,
        leftIndent: Points? = nil,
        quick: Bool? = nil,
        autoUpdate: Bool? = nil,
        tablePart: TableStylePart? = nil,
        fill: RGBColor?? = nil,
        cellAlignment: CellAlignment? = nil,
        border: Border?? = nil,
        rowBand: Int? = nil,
        columnBand: Int? = nil,
        listLevels: [ListLevel]? = nil,
        listLevel: Int? = nil,
        description: String? = nil,
        pending: String? = nil
    ) {
        self.open = open
        self.shortForm = shortForm
        self.name = name
        self.type = type
        self.basedOn = basedOn
        self.next = next
        self.font = font
        self.size = size
        self.bold = bold
        self.italic = italic
        self.underline = underline
        self.color = color
        self.alignment = alignment
        self.lineSpacing = lineSpacing
        self.spaceBefore = spaceBefore
        self.spaceAfter = spaceAfter
        self.leftIndent = leftIndent
        self.quick = quick
        self.autoUpdate = autoUpdate
        self.tablePart = tablePart
        self.fill = fill
        self.cellAlignment = cellAlignment
        self.border = border
        self.rowBand = rowBand
        self.columnBand = columnBand
        self.listLevels = listLevels
        self.listLevel = listLevel
        self.description = description
        self.pending = pending
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        open = try c.decodeIfPresent(StyleOpen.self, forKey: .open)
        shortForm = try c.decodeIfPresent(Bool.self, forKey: .shortForm)
        name = try c.decodeIfPresent(String.self, forKey: .name)
        type = try c.decodeIfPresent(StyleDialogType.self, forKey: .type)
        basedOn = c.contains(.basedOn) ? .some(try c.decode(StyleID?.self, forKey: .basedOn)) : nil
        next = c.contains(.next) ? .some(try c.decode(StyleID?.self, forKey: .next)) : nil
        font = try c.decodeIfPresent(String.self, forKey: .font)
        size = try c.decodeIfPresent(Points.self, forKey: .size)
        bold = try c.decodeIfPresent(Bool.self, forKey: .bold)
        italic = try c.decodeIfPresent(Bool.self, forKey: .italic)
        underline = try c.decodeIfPresent(UnderlineStyle.self, forKey: .underline)
        color = try c.decodeIfPresent(AutoOr<RGBColor>.self, forKey: .color)
        alignment = try c.decodeIfPresent(ParagraphAlignment.self, forKey: .alignment)
        lineSpacing = try c.decodeIfPresent(LineSpacing.self, forKey: .lineSpacing)
        spaceBefore = try c.decodeIfPresent(Points.self, forKey: .spaceBefore)
        spaceAfter = try c.decodeIfPresent(Points.self, forKey: .spaceAfter)
        leftIndent = try c.decodeIfPresent(Points.self, forKey: .leftIndent)
        quick = try c.decodeIfPresent(Bool.self, forKey: .quick)
        autoUpdate = try c.decodeIfPresent(Bool.self, forKey: .autoUpdate)
        tablePart = try c.decodeIfPresent(TableStylePart.self, forKey: .tablePart)
        fill = c.contains(.fill) ? .some(try c.decode(RGBColor?.self, forKey: .fill)) : nil
        cellAlignment = try c.decodeIfPresent(CellAlignment.self, forKey: .cellAlignment)
        border = c.contains(.border) ? .some(try c.decode(Border?.self, forKey: .border)) : nil
        rowBand = try c.decodeIfPresent(Int.self, forKey: .rowBand)
        columnBand = try c.decodeIfPresent(Int.self, forKey: .columnBand)
        listLevels = try c.decodeIfPresent([ListLevel].self, forKey: .listLevels)
        listLevel = try c.decodeIfPresent(Int.self, forKey: .listLevel)
        description = try c.decodeIfPresent(String.self, forKey: .description)
        pending = try c.decodeIfPresent(String.self, forKey: .pending)
    }
}

/// Manage Styles: its Edit, Recommend, Restrict and Set Defaults tabs. Modify, New Style and Delete are the style
/// dialog and `deleteStyle`; Import/Export isn't offered.
public struct ManageStylesDialog: Hashable, Sendable, Codable {
    /// The tab shown.
    public var tab: ManageStylesTab
    /// How the list sorts.
    public var sort: StyleSort
    /// Only the styles that show in lists.
    public var recommendedOnly: Bool
    /// The styles, in `sort` order: their visibility, priority and lock are what Recommend and Restrict change.
    public var styles: [ManagedStyle]
    /// The style selected in the list.
    public var selected: StyleID?
    /// The selected style's formatting in words (written by the editor).
    public var description: String
    /// The document's default formatting (Set Defaults).
    public var defaults: DocumentDefaults

    /// The values.
    public init(
        tab: ManageStylesTab = .edit,
        sort: StyleSort = .asRecommended,
        recommendedOnly: Bool = false,
        styles: [ManagedStyle] = [],
        selected: StyleID? = nil,
        description: String = "",
        defaults: DocumentDefaults
    ) {
        self.tab = tab
        self.sort = sort
        self.recommendedOnly = recommendedOnly
        self.styles = styles
        self.selected = selected
        self.description = description
        self.defaults = defaults
    }
}

/// The fields of `ManageStylesDialog` that changed: each is nil when unchanged.
public struct ManageStylesPatch: Hashable, Sendable, Codable {
    /// The tab shown, when changed.
    public var tab: ManageStylesTab?
    /// How the list sorts, when changed.
    public var sort: StyleSort?
    /// Only the styles that show in lists, when changed.
    public var recommendedOnly: Bool?
    /// The styles, in `sort` order: their visibility, priority and lock are what Recommend and Restrict change, when changed.
    public var styles: [ManagedStyle]?
    /// The style selected in the list, when changed.
    public var selected: StyleID??
    /// The selected style's formatting in words (written by the editor), when changed.
    public var description: String?
    /// The document's default formatting (Set Defaults), when changed.
    public var defaults: DocumentDefaults?

    /// A patch changing the given fields.
    public init(
        tab: ManageStylesTab? = nil,
        sort: StyleSort? = nil,
        recommendedOnly: Bool? = nil,
        styles: [ManagedStyle]? = nil,
        selected: StyleID?? = nil,
        description: String? = nil,
        defaults: DocumentDefaults? = nil
    ) {
        self.tab = tab
        self.sort = sort
        self.recommendedOnly = recommendedOnly
        self.styles = styles
        self.selected = selected
        self.description = description
        self.defaults = defaults
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        tab = try c.decodeIfPresent(ManageStylesTab.self, forKey: .tab)
        sort = try c.decodeIfPresent(StyleSort.self, forKey: .sort)
        recommendedOnly = try c.decodeIfPresent(Bool.self, forKey: .recommendedOnly)
        styles = try c.decodeIfPresent([ManagedStyle].self, forKey: .styles)
        selected = c.contains(.selected) ? .some(try c.decode(StyleID?.self, forKey: .selected)) : nil
        description = try c.decodeIfPresent(String.self, forKey: .description)
        defaults = try c.decodeIfPresent(DocumentDefaults.self, forKey: .defaults)
    }
}

/// Borders and Shading: the borders of the selected paragraphs, and the shading of the paragraphs or the text. A
/// setting (`none`, `box`, `shadow`) sets the edges with the line `style`, `color` and `width`; setting an edge
/// makes it `custom`. The Page Border tab does the same for the section's pages.
public struct BordersShadingDialog: Hashable, Sendable, Codable {
    /// The tab shown.
    public var tab: BordersTab
    /// The setting.
    public var setting: BorderSetting
    /// The line style the setting and new edges take.
    public var style: BorderStyle
    /// Their color.
    public var color: AutoOr<RGBColor>
    /// Their width, 0.25...6 pt.
    public var width: Points
    /// The top edge, or none.
    public var top: Border?
    /// The bottom edge, or none.
    public var bottom: Border?
    /// The left edge, or none.
    public var left: Border?
    /// The right edge, or none.
    public var right: Border?
    /// The line between paragraphs (a table style part's inside horizontal line), or none.
    public var between: Border?
    /// A table style part's inside vertical line, or none.
    public var insideVertical: Border?
    /// The paragraphs, or the text (shading only).
    public var applyTo: ShadingTarget
    /// The shading's fill, or none.
    public var fill: NoneOr<RGBColor>
    /// The shading's pattern.
    public var pattern: ShadingPattern
    /// The pattern's color.
    public var patternColor: AutoOr<RGBColor>
    /// Page Border: the setting.
    public var pageSetting: BorderSetting
    /// Page Border: the top edge, or none.
    public var pageTop: Border?
    /// Page Border: the bottom edge, or none.
    public var pageBottom: Border?
    /// Page Border: the left edge, or none.
    public var pageLeft: Border?
    /// Page Border: the right edge, or none.
    public var pageRight: Border?
    /// Page Border: what the borders are measured from.
    public var pageOffsetFrom: PageBorderOffset
    /// Page Border: the pages they show on.
    public var pageDisplay: PageBorderDisplay
    /// Page Border: the sections they apply to.
    public var pageApplyTo: PageBorderScope

    /// The values.
    public init(
        tab: BordersTab,
        setting: BorderSetting,
        style: BorderStyle,
        color: AutoOr<RGBColor>,
        width: Points,
        top: Border? = nil,
        bottom: Border? = nil,
        left: Border? = nil,
        right: Border? = nil,
        between: Border? = nil,
        insideVertical: Border? = nil,
        applyTo: ShadingTarget,
        fill: NoneOr<RGBColor>,
        pattern: ShadingPattern,
        patternColor: AutoOr<RGBColor>,
        pageSetting: BorderSetting,
        pageTop: Border? = nil,
        pageBottom: Border? = nil,
        pageLeft: Border? = nil,
        pageRight: Border? = nil,
        pageOffsetFrom: PageBorderOffset,
        pageDisplay: PageBorderDisplay,
        pageApplyTo: PageBorderScope
    ) {
        self.tab = tab
        self.setting = setting
        self.style = style
        self.color = color
        self.width = width
        self.top = top
        self.bottom = bottom
        self.left = left
        self.right = right
        self.between = between
        self.insideVertical = insideVertical
        self.applyTo = applyTo
        self.fill = fill
        self.pattern = pattern
        self.patternColor = patternColor
        self.pageSetting = pageSetting
        self.pageTop = pageTop
        self.pageBottom = pageBottom
        self.pageLeft = pageLeft
        self.pageRight = pageRight
        self.pageOffsetFrom = pageOffsetFrom
        self.pageDisplay = pageDisplay
        self.pageApplyTo = pageApplyTo
    }
}

/// The fields of `BordersShadingDialog` that changed: each is nil when unchanged.
public struct BordersShadingPatch: Hashable, Sendable, Codable {
    /// The tab shown, when changed.
    public var tab: BordersTab?
    /// The setting, when changed.
    public var setting: BorderSetting?
    /// The line style the setting and new edges take, when changed.
    public var style: BorderStyle?
    /// Their color, when changed.
    public var color: AutoOr<RGBColor>?
    /// Their width, 0.25...6 pt, when changed.
    public var width: Points?
    /// The top edge, or none, when changed.
    public var top: Border??
    /// The bottom edge, or none, when changed.
    public var bottom: Border??
    /// The left edge, or none, when changed.
    public var left: Border??
    /// The right edge, or none, when changed.
    public var right: Border??
    /// The line between paragraphs (a table style part's inside horizontal line), or none, when changed.
    public var between: Border??
    /// A table style part's inside vertical line, or none, when changed.
    public var insideVertical: Border??
    /// The paragraphs, or the text (shading only), when changed.
    public var applyTo: ShadingTarget?
    /// The shading's fill, or none, when changed.
    public var fill: NoneOr<RGBColor>?
    /// The shading's pattern, when changed.
    public var pattern: ShadingPattern?
    /// The pattern's color, when changed.
    public var patternColor: AutoOr<RGBColor>?
    /// Page Border: the setting, when changed.
    public var pageSetting: BorderSetting?
    /// Page Border: the top edge, or none, when changed.
    public var pageTop: Border??
    /// Page Border: the bottom edge, or none, when changed.
    public var pageBottom: Border??
    /// Page Border: the left edge, or none, when changed.
    public var pageLeft: Border??
    /// Page Border: the right edge, or none, when changed.
    public var pageRight: Border??
    /// Page Border: what the borders are measured from, when changed.
    public var pageOffsetFrom: PageBorderOffset?
    /// Page Border: the pages they show on, when changed.
    public var pageDisplay: PageBorderDisplay?
    /// Page Border: the sections they apply to, when changed.
    public var pageApplyTo: PageBorderScope?

    /// A patch changing the given fields.
    public init(
        tab: BordersTab? = nil,
        setting: BorderSetting? = nil,
        style: BorderStyle? = nil,
        color: AutoOr<RGBColor>? = nil,
        width: Points? = nil,
        top: Border?? = nil,
        bottom: Border?? = nil,
        left: Border?? = nil,
        right: Border?? = nil,
        between: Border?? = nil,
        insideVertical: Border?? = nil,
        applyTo: ShadingTarget? = nil,
        fill: NoneOr<RGBColor>? = nil,
        pattern: ShadingPattern? = nil,
        patternColor: AutoOr<RGBColor>? = nil,
        pageSetting: BorderSetting? = nil,
        pageTop: Border?? = nil,
        pageBottom: Border?? = nil,
        pageLeft: Border?? = nil,
        pageRight: Border?? = nil,
        pageOffsetFrom: PageBorderOffset? = nil,
        pageDisplay: PageBorderDisplay? = nil,
        pageApplyTo: PageBorderScope? = nil
    ) {
        self.tab = tab
        self.setting = setting
        self.style = style
        self.color = color
        self.width = width
        self.top = top
        self.bottom = bottom
        self.left = left
        self.right = right
        self.between = between
        self.insideVertical = insideVertical
        self.applyTo = applyTo
        self.fill = fill
        self.pattern = pattern
        self.patternColor = patternColor
        self.pageSetting = pageSetting
        self.pageTop = pageTop
        self.pageBottom = pageBottom
        self.pageLeft = pageLeft
        self.pageRight = pageRight
        self.pageOffsetFrom = pageOffsetFrom
        self.pageDisplay = pageDisplay
        self.pageApplyTo = pageApplyTo
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        tab = try c.decodeIfPresent(BordersTab.self, forKey: .tab)
        setting = try c.decodeIfPresent(BorderSetting.self, forKey: .setting)
        style = try c.decodeIfPresent(BorderStyle.self, forKey: .style)
        color = try c.decodeIfPresent(AutoOr<RGBColor>.self, forKey: .color)
        width = try c.decodeIfPresent(Points.self, forKey: .width)
        top = c.contains(.top) ? .some(try c.decode(Border?.self, forKey: .top)) : nil
        bottom = c.contains(.bottom) ? .some(try c.decode(Border?.self, forKey: .bottom)) : nil
        left = c.contains(.left) ? .some(try c.decode(Border?.self, forKey: .left)) : nil
        right = c.contains(.right) ? .some(try c.decode(Border?.self, forKey: .right)) : nil
        between = c.contains(.between) ? .some(try c.decode(Border?.self, forKey: .between)) : nil
        insideVertical = c.contains(.insideVertical) ? .some(try c.decode(Border?.self, forKey: .insideVertical)) : nil
        applyTo = try c.decodeIfPresent(ShadingTarget.self, forKey: .applyTo)
        fill = try c.decodeIfPresent(NoneOr<RGBColor>.self, forKey: .fill)
        pattern = try c.decodeIfPresent(ShadingPattern.self, forKey: .pattern)
        patternColor = try c.decodeIfPresent(AutoOr<RGBColor>.self, forKey: .patternColor)
        pageSetting = try c.decodeIfPresent(BorderSetting.self, forKey: .pageSetting)
        pageTop = c.contains(.pageTop) ? .some(try c.decode(Border?.self, forKey: .pageTop)) : nil
        pageBottom = c.contains(.pageBottom) ? .some(try c.decode(Border?.self, forKey: .pageBottom)) : nil
        pageLeft = c.contains(.pageLeft) ? .some(try c.decode(Border?.self, forKey: .pageLeft)) : nil
        pageRight = c.contains(.pageRight) ? .some(try c.decode(Border?.self, forKey: .pageRight)) : nil
        pageOffsetFrom = try c.decodeIfPresent(PageBorderOffset.self, forKey: .pageOffsetFrom)
        pageDisplay = try c.decodeIfPresent(PageBorderDisplay.self, forKey: .pageDisplay)
        pageApplyTo = try c.decodeIfPresent(PageBorderScope.self, forKey: .pageApplyTo)
    }
}

/// The Frame dialog, on the frame the selection is in: its properties (`setFrame`), and Move with text, which is
/// the vertical position relative to the paragraph. `remove` is its Remove Frame button: applying runs
/// `removeFrame`.
public struct FrameDialog: Hashable, Sendable, Codable {
    /// Text wrapping.
    public var wrap: FrameWrap
    /// How the width is set.
    public var widthRule: FrameWidthRule
    /// The width, when exact.
    public var width: Points
    /// How the height is set.
    public var heightRule: FrameHeightRule
    /// The height, unless automatic.
    public var height: Points
    /// The horizontal alignment, or none for `horizontal`.
    public var horizontalAlign: FrameHorizontalAlign?
    /// The horizontal position.
    public var horizontal: Points
    /// What it is relative to.
    public var horizontalRelative: FrameHorizontalRelative
    /// The distance from text, left and right.
    public var horizontalDistance: Points
    /// The vertical alignment, or none for `vertical`.
    public var verticalAlign: FrameVerticalAlign?
    /// The vertical position.
    public var vertical: Points
    /// What it is relative to.
    public var verticalRelative: FrameVerticalRelative
    /// The distance from text, above and below.
    public var verticalDistance: Points
    /// Move with text: the vertical position is relative to the paragraph.
    public var moveWithText: Bool
    /// Lock anchor.
    public var lockAnchor: Bool
    /// Remove Frame.
    public var remove: Bool

    /// The values.
    public init(
        wrap: FrameWrap,
        widthRule: FrameWidthRule,
        width: Points = 0,
        heightRule: FrameHeightRule,
        height: Points = 0,
        horizontalAlign: FrameHorizontalAlign? = nil,
        horizontal: Points = 0,
        horizontalRelative: FrameHorizontalRelative,
        horizontalDistance: Points = 0,
        verticalAlign: FrameVerticalAlign? = nil,
        vertical: Points = 0,
        verticalRelative: FrameVerticalRelative,
        verticalDistance: Points = 0,
        moveWithText: Bool = false,
        lockAnchor: Bool = false,
        remove: Bool = false
    ) {
        self.wrap = wrap
        self.widthRule = widthRule
        self.width = width
        self.heightRule = heightRule
        self.height = height
        self.horizontalAlign = horizontalAlign
        self.horizontal = horizontal
        self.horizontalRelative = horizontalRelative
        self.horizontalDistance = horizontalDistance
        self.verticalAlign = verticalAlign
        self.vertical = vertical
        self.verticalRelative = verticalRelative
        self.verticalDistance = verticalDistance
        self.moveWithText = moveWithText
        self.lockAnchor = lockAnchor
        self.remove = remove
    }
}

/// The fields of `FrameDialog` that changed: each is nil when unchanged.
public struct FramePatch: Hashable, Sendable, Codable {
    /// Text wrapping, when changed.
    public var wrap: FrameWrap?
    /// How the width is set, when changed.
    public var widthRule: FrameWidthRule?
    /// The width, when exact, when changed.
    public var width: Points?
    /// How the height is set, when changed.
    public var heightRule: FrameHeightRule?
    /// The height, unless automatic, when changed.
    public var height: Points?
    /// The horizontal alignment, or none for `horizontal`, when changed.
    public var horizontalAlign: FrameHorizontalAlign??
    /// The horizontal position, when changed.
    public var horizontal: Points?
    /// What it is relative to, when changed.
    public var horizontalRelative: FrameHorizontalRelative?
    /// The distance from text, left and right, when changed.
    public var horizontalDistance: Points?
    /// The vertical alignment, or none for `vertical`, when changed.
    public var verticalAlign: FrameVerticalAlign??
    /// The vertical position, when changed.
    public var vertical: Points?
    /// What it is relative to, when changed.
    public var verticalRelative: FrameVerticalRelative?
    /// The distance from text, above and below, when changed.
    public var verticalDistance: Points?
    /// Move with text: the vertical position is relative to the paragraph, when changed.
    public var moveWithText: Bool?
    /// Lock anchor, when changed.
    public var lockAnchor: Bool?
    /// Remove Frame, when changed.
    public var remove: Bool?

    /// A patch changing the given fields.
    public init(
        wrap: FrameWrap? = nil,
        widthRule: FrameWidthRule? = nil,
        width: Points? = nil,
        heightRule: FrameHeightRule? = nil,
        height: Points? = nil,
        horizontalAlign: FrameHorizontalAlign?? = nil,
        horizontal: Points? = nil,
        horizontalRelative: FrameHorizontalRelative? = nil,
        horizontalDistance: Points? = nil,
        verticalAlign: FrameVerticalAlign?? = nil,
        vertical: Points? = nil,
        verticalRelative: FrameVerticalRelative? = nil,
        verticalDistance: Points? = nil,
        moveWithText: Bool? = nil,
        lockAnchor: Bool? = nil,
        remove: Bool? = nil
    ) {
        self.wrap = wrap
        self.widthRule = widthRule
        self.width = width
        self.heightRule = heightRule
        self.height = height
        self.horizontalAlign = horizontalAlign
        self.horizontal = horizontal
        self.horizontalRelative = horizontalRelative
        self.horizontalDistance = horizontalDistance
        self.verticalAlign = verticalAlign
        self.vertical = vertical
        self.verticalRelative = verticalRelative
        self.verticalDistance = verticalDistance
        self.moveWithText = moveWithText
        self.lockAnchor = lockAnchor
        self.remove = remove
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        wrap = try c.decodeIfPresent(FrameWrap.self, forKey: .wrap)
        widthRule = try c.decodeIfPresent(FrameWidthRule.self, forKey: .widthRule)
        width = try c.decodeIfPresent(Points.self, forKey: .width)
        heightRule = try c.decodeIfPresent(FrameHeightRule.self, forKey: .heightRule)
        height = try c.decodeIfPresent(Points.self, forKey: .height)
        horizontalAlign = c.contains(.horizontalAlign) ? .some(try c.decode(FrameHorizontalAlign?.self, forKey: .horizontalAlign)) : nil
        horizontal = try c.decodeIfPresent(Points.self, forKey: .horizontal)
        horizontalRelative = try c.decodeIfPresent(FrameHorizontalRelative.self, forKey: .horizontalRelative)
        horizontalDistance = try c.decodeIfPresent(Points.self, forKey: .horizontalDistance)
        verticalAlign = c.contains(.verticalAlign) ? .some(try c.decode(FrameVerticalAlign?.self, forKey: .verticalAlign)) : nil
        vertical = try c.decodeIfPresent(Points.self, forKey: .vertical)
        verticalRelative = try c.decodeIfPresent(FrameVerticalRelative.self, forKey: .verticalRelative)
        verticalDistance = try c.decodeIfPresent(Points.self, forKey: .verticalDistance)
        moveWithText = try c.decodeIfPresent(Bool.self, forKey: .moveWithText)
        lockAnchor = try c.decodeIfPresent(Bool.self, forKey: .lockAnchor)
        remove = try c.decodeIfPresent(Bool.self, forKey: .remove)
    }
}

/// Banding: a table style's rows and columns in each band.
public struct BandingDialog: Hashable, Sendable, Codable {
    /// The rows in each band.
    public var rowBand: Int
    /// The columns in each band.
    public var columnBand: Int

    /// The values.
    public init(
        rowBand: Int = 0,
        columnBand: Int = 0
    ) {
        self.rowBand = rowBand
        self.columnBand = columnBand
    }
}

/// The fields of `BandingDialog` that changed: each is nil when unchanged.
public struct BandingPatch: Hashable, Sendable, Codable {
    /// The rows in each band, when changed.
    public var rowBand: Int?
    /// The columns in each band, when changed.
    public var columnBand: Int?

    /// A patch changing the given fields.
    public init(
        rowBand: Int? = nil,
        columnBand: Int? = nil
    ) {
        self.rowBand = rowBand
        self.columnBand = columnBand
    }
}

/// Define New Bullet: the bullet of the selection's list level (or a new list's): its character in its font, or
/// a picture.
public struct DefineBulletDialog: Hashable, Sendable, Codable {
    /// The bullet's character.
    public var text: String
    /// Its font.
    public var font: String?
    /// A picture bullet's image (PNG, JPEG or GIF), instead of the character.
    public var picture: Data?
    /// Its alignment.
    public var alignment: LevelAlignment

    /// The values.
    public init(
        text: String = "",
        font: String? = nil,
        picture: Data? = nil,
        alignment: LevelAlignment
    ) {
        self.text = text
        self.font = font
        self.picture = picture
        self.alignment = alignment
    }
}

/// The fields of `DefineBulletDialog` that changed: each is nil when unchanged.
public struct DefineBulletPatch: Hashable, Sendable, Codable {
    /// The bullet's character, when changed.
    public var text: String?
    /// Its font, when changed.
    public var font: String??
    /// A picture bullet's image (PNG, JPEG or GIF), instead of the character, when changed.
    public var picture: Data??
    /// Its alignment, when changed.
    public var alignment: LevelAlignment?

    /// A patch changing the given fields.
    public init(
        text: String? = nil,
        font: String?? = nil,
        picture: Data?? = nil,
        alignment: LevelAlignment? = nil
    ) {
        self.text = text
        self.font = font
        self.picture = picture
        self.alignment = alignment
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        text = try c.decodeIfPresent(String.self, forKey: .text)
        font = c.contains(.font) ? .some(try c.decode(String?.self, forKey: .font)) : nil
        picture = c.contains(.picture) ? .some(try c.decode(Data?.self, forKey: .picture)) : nil
        alignment = try c.decodeIfPresent(LevelAlignment.self, forKey: .alignment)
    }
}

/// Define New Number Format: the number of the selection's list level (or a new list's).
public struct DefineNumberFormatDialog: Hashable, Sendable, Codable {
    /// The number style.
    public var format: NumberFormat
    /// The number format, with `%1` for the number.
    public var text: String
    /// The number's font.
    public var font: String?
    /// Its alignment.
    public var alignment: LevelAlignment

    /// The values.
    public init(
        format: NumberFormat,
        text: String = "",
        font: String? = nil,
        alignment: LevelAlignment
    ) {
        self.format = format
        self.text = text
        self.font = font
        self.alignment = alignment
    }
}

/// The fields of `DefineNumberFormatDialog` that changed: each is nil when unchanged.
public struct DefineNumberFormatPatch: Hashable, Sendable, Codable {
    /// The number style, when changed.
    public var format: NumberFormat?
    /// The number format, with `%1` for the number, when changed.
    public var text: String?
    /// The number's font, when changed.
    public var font: String??
    /// Its alignment, when changed.
    public var alignment: LevelAlignment?

    /// A patch changing the given fields.
    public init(
        format: NumberFormat? = nil,
        text: String? = nil,
        font: String?? = nil,
        alignment: LevelAlignment? = nil
    ) {
        self.format = format
        self.text = text
        self.font = font
        self.alignment = alignment
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        format = try c.decodeIfPresent(NumberFormat.self, forKey: .format)
        text = try c.decodeIfPresent(String.self, forKey: .text)
        font = c.contains(.font) ? .some(try c.decode(String?.self, forKey: .font)) : nil
        alignment = try c.decodeIfPresent(LevelAlignment.self, forKey: .alignment)
    }
}

/// Define New Multilevel List: the nine levels of the selection's list (or a new list's), the level shown, what
/// the levels apply to, and the ListNum field name.
public struct DefineMultilevelDialog: Hashable, Sendable, Codable {
    /// The levels.
    public var levels: [ListLevel]
    /// The level shown (0 to 8).
    public var level: Int
    /// What the levels apply to.
    public var applyTo: ListApplyTo
    /// The level the gallery shows (1 to 9).
    public var galleryLevel: Int
    /// The ListNum field list name.
    public var listName: String

    /// The values.
    public init(
        levels: [ListLevel] = [],
        level: Int = 0,
        applyTo: ListApplyTo,
        galleryLevel: Int = 0,
        listName: String = ""
    ) {
        self.levels = levels
        self.level = level
        self.applyTo = applyTo
        self.galleryLevel = galleryLevel
        self.listName = listName
    }
}

/// The fields of `DefineMultilevelDialog` that changed: each is nil when unchanged.
public struct DefineMultilevelPatch: Hashable, Sendable, Codable {
    /// The levels, when changed.
    public var levels: [ListLevel]?
    /// The level shown (0 to 8), when changed.
    public var level: Int?
    /// What the levels apply to, when changed.
    public var applyTo: ListApplyTo?
    /// The level the gallery shows (1 to 9), when changed.
    public var galleryLevel: Int?
    /// The ListNum field list name, when changed.
    public var listName: String?

    /// A patch changing the given fields.
    public init(
        levels: [ListLevel]? = nil,
        level: Int? = nil,
        applyTo: ListApplyTo? = nil,
        galleryLevel: Int? = nil,
        listName: String? = nil
    ) {
        self.levels = levels
        self.level = level
        self.applyTo = applyTo
        self.galleryLevel = galleryLevel
        self.listName = listName
    }
}

/// Set Numbering Value: start a new list at a value, or continue the previous list (skipping to a value).
public struct SetNumberingValueDialog: Hashable, Sendable, Codable {
    /// Start a new list, or continue.
    public var mode: NumberingMode
    /// The value: the paragraph's number when it opens.
    public var value: Int
    /// Continuing: skip numbers to `value`.
    public var advance: Bool

    /// The values.
    public init(
        mode: NumberingMode,
        value: Int = 0,
        advance: Bool = false
    ) {
        self.mode = mode
        self.value = value
        self.advance = advance
    }
}

/// The fields of `SetNumberingValueDialog` that changed: each is nil when unchanged.
public struct SetNumberingValuePatch: Hashable, Sendable, Codable {
    /// Start a new list, or continue, when changed.
    public var mode: NumberingMode?
    /// The value: the paragraph's number when it opens, when changed.
    public var value: Int?
    /// Continuing: skip numbers to `value`, when changed.
    public var advance: Bool?

    /// A patch changing the given fields.
    public init(
        mode: NumberingMode? = nil,
        value: Int? = nil,
        advance: Bool? = nil
    ) {
        self.mode = mode
        self.value = value
        self.advance = advance
    }
}

/// Adjust List Indents: the selection's list level's indents and what follows its number.
public struct AdjustListIndentsDialog: Hashable, Sendable, Codable {
    /// Where the number is.
    public var numberPosition: Points
    /// Where the text is.
    public var textIndent: Points
    /// What follows the number.
    public var follow: FollowNumber
    /// The tab stop after the number; none uses the text indent.
    public var tab: Points?

    /// The values.
    public init(
        numberPosition: Points = 0,
        textIndent: Points = 0,
        follow: FollowNumber,
        tab: Points? = nil
    ) {
        self.numberPosition = numberPosition
        self.textIndent = textIndent
        self.follow = follow
        self.tab = tab
    }
}

/// The fields of `AdjustListIndentsDialog` that changed: each is nil when unchanged.
public struct AdjustListIndentsPatch: Hashable, Sendable, Codable {
    /// Where the number is, when changed.
    public var numberPosition: Points?
    /// Where the text is, when changed.
    public var textIndent: Points?
    /// What follows the number, when changed.
    public var follow: FollowNumber?
    /// The tab stop after the number; none uses the text indent, when changed.
    public var tab: Points??

    /// A patch changing the given fields.
    public init(
        numberPosition: Points? = nil,
        textIndent: Points? = nil,
        follow: FollowNumber? = nil,
        tab: Points?? = nil
    ) {
        self.numberPosition = numberPosition
        self.textIndent = textIndent
        self.follow = follow
        self.tab = tab
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        numberPosition = try c.decodeIfPresent(Points.self, forKey: .numberPosition)
        textIndent = try c.decodeIfPresent(Points.self, forKey: .textIndent)
        follow = try c.decodeIfPresent(FollowNumber.self, forKey: .follow)
        tab = c.contains(.tab) ? .some(try c.decode(Points?.self, forKey: .tab)) : nil
    }
}

/// Bullets and Numbering, from the style dialog's Format menu: the list the style's paragraphs are in, as a
/// gallery entry (`listGalleries`), or none.
public struct BulletsNumberingDialog: Hashable, Sendable, Codable {
    /// The gallery shown.
    public var gallery: ListGalleryKind
    /// The entry, or none for no list.
    public var entry: ListEntryID?

    /// The values.
    public init(
        gallery: ListGalleryKind = .bullet,
        entry: ListEntryID? = nil
    ) {
        self.gallery = gallery
        self.entry = entry
    }
}

/// The fields of `BulletsNumberingDialog` that changed: each is nil when unchanged.
public struct BulletsNumberingPatch: Hashable, Sendable, Codable {
    /// The gallery shown, when changed.
    public var gallery: ListGalleryKind?
    /// The entry, or none for no list, when changed.
    public var entry: ListEntryID??

    /// A patch changing the given fields.
    public init(
        gallery: ListGalleryKind? = nil,
        entry: ListEntryID?? = nil
    ) {
        self.gallery = gallery
        self.entry = entry
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        gallery = try c.decodeIfPresent(ListGalleryKind.self, forKey: .gallery)
        entry = c.contains(.entry) ? .some(try c.decode(ListEntryID?.self, forKey: .entry)) : nil
    }
}

