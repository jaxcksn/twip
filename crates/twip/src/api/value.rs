//! Values that commands, state and services share: lengths, colours, bytes, ids, and the formatting vocabularies
//! (underline styles, alignments, tab stops, wrapping, content control kinds).

use serde::de::{DeserializeOwned, Error as _};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A length in points (1/72 inch). Every length in the API is one.
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Points(pub f64);

/// An RGB colour, `#RRGGBB` on the wire.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Color(pub u32);

impl Color {
    /// The colour a `#RRGGBB` string names.
    pub fn parse(s: &str) -> Option<Color> {
        let hex = s.strip_prefix('#').filter(|h| h.len() == 6)?;
        u32::from_str_radix(hex, 16).ok().map(Color)
    }
}

impl std::fmt::Display for Color {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "#{:06X}", self.0 & 0xFF_FFFF)
    }
}

impl Serialize for Color {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Color {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Color, D::Error> {
        let s = String::deserialize(d)?;
        Color::parse(&s).ok_or_else(|| D::Error::custom(format!("`{s}` is not a #RRGGBB colour")))
    }
}

/// A value or a keyword in its place: `'auto'` ([`AutoOr`]) or `'none'` ([`NoneOr`]). The keyword is read before the
/// value, so a value that spells the keyword reads as the keyword.
fn keyword_or<'de, D: Deserializer<'de>, T: DeserializeOwned>(d: D, keyword: &str) -> Result<Option<T>, D::Error> {
    let v = serde_json::Value::deserialize(d)?;
    if v.as_str() == Some(keyword) {
        return Ok(None);
    }
    T::deserialize(v).map(Some).map_err(D::Error::custom)
}

/// A value, or `'auto'`: the document decides (an automatic colour, automatic spacing, a row's natural height).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum AutoOr<T> {
    /// The document decides.
    #[default]
    Auto,
    /// This value.
    Value(T),
}

impl<T: Serialize> Serialize for AutoOr<T> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            AutoOr::Auto => s.serialize_str("auto"),
            AutoOr::Value(v) => v.serialize(s),
        }
    }
}

impl<'de, T: DeserializeOwned> Deserialize<'de> for AutoOr<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<AutoOr<T>, D::Error> {
        Ok(keyword_or(d, "auto")?.map_or(AutoOr::Auto, AutoOr::Value))
    }
}

/// A value, or `'none'` where state reports its absence explicitly (no shading, no highlight).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NoneOr<T> {
    /// Absent.
    None,
    /// This value.
    Value(T),
}

impl<T: Serialize> Serialize for NoneOr<T> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            NoneOr::None => s.serialize_str("none"),
            NoneOr::Value(v) => v.serialize(s),
        }
    }
}

impl<'de, T: DeserializeOwned> Deserialize<'de> for NoneOr<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<NoneOr<T>, D::Error> {
        Ok(keyword_or(d, "none")?.map_or(NoneOr::None, NoneOr::Value))
    }
}

/// One of the theme's two fonts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ThemeFont {
    /// The heading font (`+Headings`, the theme's major font).
    Headings,
    /// The body font (`+Body`, the theme's minor font).
    Body,
}

/// A paragraph's outline level: body text, or a level from 1 to 9. On the wire: `'body'` or the number.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum OutlineLevel {
    /// Body text, with no outline level.
    #[default]
    Body,
    /// An outline level, 1 to 9.
    Level(u8),
}

impl Serialize for OutlineLevel {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            OutlineLevel::Body => s.serialize_str("body"),
            OutlineLevel::Level(n) => s.serialize_u8(*n),
        }
    }
}

impl<'de> Deserialize<'de> for OutlineLevel {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<OutlineLevel, D::Error> {
        Ok(keyword_or(d, "body")?.map_or(OutlineLevel::Body, OutlineLevel::Level))
    }
}

/// Binary data (pictures, fonts, dictionaries, documents), standard base64 on the wire.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Bytes(pub Vec<u8>);

impl Serialize for Bytes {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&base64(&self.0))
    }
}

impl<'de> Deserialize<'de> for Bytes {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Bytes, D::Error> {
        let s = String::deserialize(d)?;
        from_base64(&s).map(Bytes).ok_or_else(|| D::Error::custom("not standard base64"))
    }
}

pub(crate) use crate::base64::{base64, from_base64};

macro_rules! ids {
    ($($(#[$m:meta])* $name:ident),* $(,)?) => {$(
        $(#[$m])*
        #[derive(Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub String);
    )*};
}

ids! {
    /// A merge-designer element: a field or a content control. Stable while the document is open; undo restores it.
    ElementId,
    /// A comment: a thread's id is its first comment's.
    CommentId,
    /// A revision (a tracked change).
    RevisionId,
    /// A heading the navigation pane lists.
    HeadingId,
    /// A find match.
    MatchId,
    /// A style, by its `w:styleId`.
    StyleId,
    /// A paragraph, as a decimal string: stable while the document is open and the paragraph exists (a split keeps it
    /// on the first half, a join keeps the first paragraph's), not saved in the file.
    ParagraphId,
    /// A floating object, as a decimal string: stable while the document is open and the object exists (undo restores
    /// it), not saved in the file.
    FloatId,
}

/// A floating object, or one of the objects in a floating group.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ObjectId {
    /// The floating object.
    pub float: FloatId,
    /// A group child's place: its index in the group, then in each group within it; empty for the floating object.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub path: Vec<u16>,
}

impl ObjectId {
    /// The floating object `id` itself.
    pub(crate) fn of(id: crate::layout::FloatId) -> ObjectId {
        ObjectId { float: FloatId(id.0.to_string()), path: Vec::new() }
    }

    /// The engine's id for the floating object; `None` when it isn't a number.
    pub(crate) fn float_id(&self) -> Option<crate::layout::FloatId> {
        self.float.0.parse().ok().map(crate::layout::FloatId)
    }
}

/// A place in a paragraph's text.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextPoint {
    /// The paragraph.
    pub paragraph: ParagraphId,
    /// The offset in the paragraph's text, in Unicode scalar values: a field counts as its result, an object or a
    /// note reference as one U+FFFC, and hidden and deleted text count. Past the end reads as the end.
    pub offset: u32,
}

/// A range of text from one place to another.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextRange {
    /// Where it starts. State gives the earlier end first; a command takes the ends either way round.
    pub start: TextPoint,
    /// Where it ends.
    pub end: TextPoint,
}

/// A rectangular block of a table's cells.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CellBlock {
    /// The start of the block's first cell's text: names the table.
    pub start: TextPoint,
    /// The first row, from 0.
    pub first_row: u32,
    /// The first grid column, from 0.
    pub first_column: u32,
    /// The last row.
    pub last_row: u32,
    /// The last grid column.
    pub last_column: u32,
}

/// The way a line of text runs on the page.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LineDirection {
    /// Across the page (the caret is a vertical bar).
    #[default]
    Horizontal,
    /// Down the page, as vertical East Asian text runs (the caret is a horizontal bar).
    Down,
    /// Up the page, as text turned counter-clockwise runs.
    Up,
}

/// The two ways along a sequence.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Direction {
    /// Toward the start.
    Backward,
    /// Toward the end.
    Forward,
}

/// Next or previous, in document order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Order {
    /// The next one.
    Next,
    /// The previous one.
    Previous,
}

/// Where the selection is: the body or another story.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Story {
    /// The main document.
    #[default]
    Body,
    /// A header.
    Header,
    /// A footer.
    Footer,
    /// A footnote.
    Footnote,
    /// An endnote.
    Endnote,
    /// A text box.
    TextBox,
    /// A sub-template's body (a building block), while the editor is on it.
    SubTemplate,
}

/// `w:u` styles.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UnderlineStyle {
    /// No underline.
    #[default]
    None,
    /// A single line.
    Single,
    /// Words only, not the spaces between them.
    Words,
    /// A double line.
    Double,
    /// A thick line.
    Thick,
    /// A dotted line.
    Dotted,
    /// A dashed line.
    Dash,
    /// Alternating dots and dashes.
    DotDash,
    /// A wavy line.
    Wave,
    /// A heavy dotted line.
    DottedHeavy,
    /// A heavy dashed line.
    DashedHeavy,
    /// A line of long dashes.
    DashLong,
    /// A heavy line of long dashes.
    DashLongHeavy,
    /// Heavy alternating dots and dashes.
    DashDotHeavy,
    /// A dash and two dots, repeated.
    DotDotDash,
    /// A heavy dash and two dots, repeated.
    DashDotDotHeavy,
    /// A heavy wavy line.
    WavyHeavy,
    /// Two wavy lines.
    WavyDouble,
}

/// `w:highlight` colours.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HighlightColor {
    /// Yellow.
    Yellow,
    /// Bright green.
    BrightGreen,
    /// Turquoise.
    Turquoise,
    /// Pink.
    Pink,
    /// Blue.
    Blue,
    /// Red.
    Red,
    /// Dark blue.
    DarkBlue,
    /// Teal.
    Teal,
    /// Green.
    Green,
    /// Violet.
    Violet,
    /// Dark red.
    DarkRed,
    /// Dark yellow.
    DarkYellow,
    /// 50% gray.
    Gray50,
    /// 25% gray.
    Gray25,
    /// Black.
    Black,
}

/// A paragraph's alignment.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Alignment {
    /// Aligned left.
    #[default]
    Left,
    /// Centred.
    Center,
    /// Aligned right.
    Right,
    /// Justified: spread to both margins.
    Justify,
}

/// Left, centre or right: tables on the page, page numbers in a header.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HorizontalAlignment {
    /// Left.
    #[default]
    Left,
    /// Centred.
    Center,
    /// Right.
    Right,
}

/// Text in a cell, top to bottom.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CellAlignment {
    /// At the top.
    #[default]
    Top,
    /// Centred vertically.
    Center,
    /// At the bottom.
    Bottom,
}

/// Line spacing: a multiple of single, or a size in points.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "rule", rename_all = "camelCase", deny_unknown_fields)]
pub enum LineSpacing {
    /// `lines` times single spacing (0.06…132).
    Multiple {
        /// The multiple.
        lines: f64,
    },
    /// Exactly `size` (0.7…1584 pt).
    Exact {
        /// The line height.
        size: Points,
    },
    /// At least `size` (0.7…1584 pt).
    AtLeast {
        /// The least line height.
        size: Points,
    },
}

impl Default for LineSpacing {
    fn default() -> LineSpacing {
        LineSpacing::Multiple { lines: 1.0 }
    }
}

/// How a tab stop aligns the text after it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TabAlignment {
    /// Text starts at the stop.
    #[default]
    Left,
    /// Text centres on the stop.
    Center,
    /// Text ends at the stop.
    Right,
    /// The decimal point sits at the stop.
    Decimal,
    /// A vertical bar at the stop, with no effect on the text.
    Bar,
}

/// What fills the space before a tab stop.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TabLeader {
    /// No leader.
    #[default]
    None,
    /// Dots.
    Dots,
    /// Dashes.
    Dashes,
    /// A line.
    Underline,
    /// A heavy line.
    Heavy,
    /// Middle dots.
    MiddleDot,
}

/// A section's page orientation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Orientation {
    /// Taller than wide.
    #[default]
    Portrait,
    /// Wider than tall.
    Landscape,
}

/// How text wraps around a picture or text box.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ObjectWrap {
    /// In line with the text.
    #[default]
    Inline,
    /// Text wraps around its bounding box.
    Square,
    /// Text wraps tightly around its outline.
    Tight,
    /// Text wraps through its open parts.
    Through,
    /// Text sits above and below it, not beside it.
    TopAndBottom,
    /// Behind the text.
    BehindText,
    /// In front of the text.
    InFrontOfText,
}

/// What a floating object's horizontal position is measured from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HorizontalReference {
    /// The column it is anchored in (the default).
    #[default]
    Column,
    /// The area between the left and right margins.
    Margin,
    /// The page's edges.
    Page,
    /// The character its anchor is at.
    Character,
    /// The left margin's area.
    LeftMargin,
    /// The right margin's area.
    RightMargin,
    /// The inside margin's area (left on odd pages, right on even ones).
    InsideMargin,
    /// The outside margin's area.
    OutsideMargin,
}

/// What a floating object's vertical position is measured from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VerticalReference {
    /// Its anchor paragraph's top: it moves with the text (the default).
    #[default]
    Paragraph,
    /// Its anchor line's top: it moves with the text.
    Line,
    /// The area between the top and bottom margins.
    Margin,
    /// The page's edges: it stays where it is on the page.
    Page,
    /// The top margin's area.
    TopMargin,
    /// The bottom margin's area.
    BottomMargin,
    /// The inside margin's area.
    InsideMargin,
    /// The outside margin's area.
    OutsideMargin,
}

/// A floating object aligned across its horizontal reference.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ObjectHorizontalAlignment {
    /// At its left edge.
    #[default]
    Left,
    /// Centred.
    Center,
    /// At its right edge.
    Right,
    /// At its inside edge (left on odd pages).
    Inside,
    /// At its outside edge.
    Outside,
}

/// A floating object aligned across its vertical reference.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ObjectVerticalAlignment {
    /// At its top.
    #[default]
    Top,
    /// Centred.
    Center,
    /// At its bottom.
    Bottom,
    /// At its inside edge.
    Inside,
    /// At its outside edge.
    Outside,
}

/// Which sides of a floating object text wraps on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WrapSide {
    /// Both sides (the default).
    #[default]
    BothSides,
    /// Its left only.
    Left,
    /// Its right only.
    Right,
    /// Whichever side has more room.
    Largest,
}

/// A floating object's horizontal place: aligned across its reference, or an offset from the reference's left edge.
/// Given with neither, the object stays where it is, measured from the new reference.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HorizontalPosition {
    /// What it is measured from.
    pub relative_to: HorizontalReference,
    /// Aligned across the reference.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub align: Option<ObjectHorizontalAlignment>,
    /// Right of the reference's left edge (−1584…1584 pt).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset: Option<Points>,
    /// Right of the reference's left edge by a share of its width (−1000…1000 %, `wp14:pctPosHOffset`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub percent: Option<f64>,
}

/// A floating object's vertical place: aligned across its reference, or an offset below the reference's top. Given
/// with neither, the object stays where it is, measured from the new reference.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VerticalPosition {
    /// What it is measured from.
    pub relative_to: VerticalReference,
    /// Aligned across the reference.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub align: Option<ObjectVerticalAlignment>,
    /// Below the reference's top (−1584…1584 pt).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset: Option<Points>,
    /// Below the reference's top by a share of its height (−1000…1000 %, `wp14:pctPosVOffset`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub percent: Option<f64>,
}

/// A table's preferred width.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum TableWidth {
    /// Sized by its content.
    #[default]
    Auto,
    /// A width in points.
    Points {
        /// The width.
        value: Points,
    },
    /// 1…600 percent of the text width.
    Percent {
        /// The percentage.
        value: f64,
    },
}

/// How a row height applies.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HeightRule {
    /// At least the height: taller when the content needs it.
    #[default]
    AtLeast,
    /// Exactly the height.
    Exact,
}

/// A row height: at least or exactly `height` (0…1584 pt).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RowHeight {
    /// The height.
    pub height: Points,
    /// How it applies.
    pub rule: HeightRule,
}

/// The kinds of content control.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ControlKind {
    /// Formatted text.
    #[default]
    RichText,
    /// Unformatted text.
    PlainText,
    /// A picture.
    Picture,
    /// A checkbox.
    Checkbox,
    /// A date picker.
    Date,
    /// A drop-down list.
    DropDown,
    /// A combo box: a list that also takes typed text.
    ComboBox,
    /// A group of controls, edited only inside its child controls.
    Group,
    /// A section that repeats per item.
    RepeatingSection,
    /// A building block gallery: it offers the building blocks of a gallery and category.
    BuildingBlockGallery,
    /// A building block inserted from a gallery (a cover page, a table of contents); read only.
    BuildingBlock,
    /// A citation; read only.
    Citation,
    /// A bibliography; read only.
    Bibliography,
    /// An equation; read only.
    Equation,
}

/// What a content control's lock keeps: the control itself, its content, or both.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ControlLock {
    /// Not locked.
    #[default]
    None,
    /// The control can't be deleted.
    Control,
    /// The content can't be edited.
    Content,
    /// The control can't be deleted and its content can't be edited.
    Both,
}

/// A content control's binding to a custom XML data part (`w:dataBinding`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DataBinding {
    /// The XPath of the bound node in the data part.
    pub xpath: String,
    /// The data part's item id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub store_item_id: Option<String>,
    /// The XPath's namespace prefixes (`xmlns:ns0='…'`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefix_mappings: Option<String>,
}

/// One entry of a drop-down or combo box.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListItem {
    /// The text shown.
    pub text: String,
    /// The value stored when it is picked.
    pub value: String,
}

/// The settings of one control kind. On the wire the object's fields say which kind it is.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ControlOptions {
    /// A checkbox's state.
    Checkbox(CheckboxOptions),
    /// A date picker's format and locale.
    Date(DateOptions),
    /// A drop-down's or combo box's entries.
    List(ListOptions),
    /// A plain text control's settings.
    PlainText(PlainTextOptions),
    /// A picture control's picture size.
    Picture(PictureOptions),
    /// A repeating section's options.
    RepeatingSection(RepeatingSectionOptions),
    /// A building block gallery's gallery and category.
    Gallery(GalleryOptions),
}

/// The building blocks a building block gallery control offers.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GalleryOptions {
    /// The gallery, by Word's name for it (`Quick Parts`, `AutoText`, `Custom 1`); `None` or `""`: any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gallery: Option<String>,
    /// The category in the gallery; `None` or `""`: any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
}

/// A repeating section control's options.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RepeatingSectionOptions {
    /// What its items are called in its menu (`w15:sectionTitle`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub section_title: Option<String>,
    /// Its items can be added and removed (`false`: `w15:doNotAllowInsertDeleteSection`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allow_insert_delete: Option<bool>,
}

/// A check box control's checked or unchecked symbol.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CheckboxSymbol {
    /// Its font.
    pub font: String,
    /// The character, one.
    pub character: String,
}

/// How a content control shows its edges (`w15:appearance`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ControlAppearance {
    /// A box around it.
    #[default]
    BoundingBox,
    /// Start and end tags.
    Tags,
    /// Nothing.
    Hidden,
}

/// A checkbox's state.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CheckboxOptions {
    /// Whether it is checked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checked: Option<bool>,
    /// The symbol it shows checked (`w14:checkedState`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checked_symbol: Option<CheckboxSymbol>,
    /// The symbol it shows unchecked (`w14:uncheckedState`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unchecked_symbol: Option<CheckboxSymbol>,
}

/// A date picker's display format and locale.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DateOptions {
    /// The display format (`M/d/yyyy`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    /// The locale (`en-US`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,
    /// The calendar its dates are in (default Gregorian).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calendar: Option<CalendarType>,
    /// What a bound date control stores in its data (default the date and time).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub store_as: Option<DateStorage>,
}

/// One of the separators between the text and the notes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NoteSeparatorPart {
    /// Above the notes on the page they start.
    Separator,
    /// Above the notes continued from the page before.
    ContinuationSeparator,
    /// Below notes that continue on the next page.
    ContinuationNotice,
}

/// A group or a person an editable range lets edit: `{ "group": "editors" }` or `{ "user": "ada@example.com" }`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RangeEditorRef {
    /// A group: `everyone`, `administrators`, `contributors`, `editors`, `owners`, `current`.
    Group(String),
    /// One person: a name or email address.
    User(String),
}

/// One of Word's column presets.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ColumnPreset {
    /// One column.
    One,
    /// Two equal columns.
    Two,
    /// Three equal columns.
    Three,
    /// Two, the narrow one on the left.
    Left,
    /// Two, the narrow one on the right.
    Right,
}

/// A date control's calendar.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CalendarType {
    /// Gregorian (localized).
    #[default]
    Gregorian,
    /// Hijri.
    Hijri,
    /// Hebrew lunar.
    Hebrew,
    /// Taiwan.
    Taiwan,
    /// Japanese emperor era.
    Japan,
    /// Thai.
    Thai,
    /// Korean Tangun era.
    Korea,
    /// Saka era.
    Saka,
    /// Gregorian, transliterated English.
    GregorianXlitEnglish,
    /// Gregorian, transliterated French.
    GregorianXlitFrench,
    /// Gregorian, US English.
    GregorianUs,
    /// Gregorian, Middle East French.
    GregorianMeFrench,
    /// Gregorian, Arabic.
    GregorianArabic,
}

impl CalendarType {
    /// Every calendar.
    pub const ALL: &'static [CalendarType] = &[
        CalendarType::Gregorian,
        CalendarType::Hijri,
        CalendarType::Hebrew,
        CalendarType::Taiwan,
        CalendarType::Japan,
        CalendarType::Thai,
        CalendarType::Korea,
        CalendarType::Saka,
        CalendarType::GregorianXlitEnglish,
        CalendarType::GregorianXlitFrench,
        CalendarType::GregorianUs,
        CalendarType::GregorianMeFrench,
        CalendarType::GregorianArabic,
    ];

    /// Its name as documents write it (`gregorianUs`).
    pub fn val(self) -> &'static str {
        match self {
            CalendarType::Gregorian => "gregorian",
            CalendarType::Hijri => "hijri",
            CalendarType::Hebrew => "hebrew",
            CalendarType::Taiwan => "taiwan",
            CalendarType::Japan => "japan",
            CalendarType::Thai => "thai",
            CalendarType::Korea => "korea",
            CalendarType::Saka => "saka",
            CalendarType::GregorianXlitEnglish => "gregorianXlitEnglish",
            CalendarType::GregorianXlitFrench => "gregorianXlitFrench",
            CalendarType::GregorianUs => "gregorianUs",
            CalendarType::GregorianMeFrench => "gregorianMeFrench",
            CalendarType::GregorianArabic => "gregorianArabic",
        }
    }

    /// The calendar named `val`.
    pub fn of_val(val: &str) -> Option<CalendarType> {
        CalendarType::ALL.iter().copied().find(|c| c.val() == val)
    }
}

/// What a bound date control stores in its data.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DateStorage {
    /// The text it shows.
    Text,
    /// The date.
    Date,
    /// The date and time.
    #[default]
    DateTime,
}

/// A drop-down's or combo box's entries.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListOptions {
    /// The entries, in order.
    pub items: Vec<ListItem>,
}

/// Whether a plain text control takes line breaks.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlainTextOptions {
    /// Whether it takes line breaks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multiline: Option<bool>,
}

/// A width and height.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Size {
    /// The width.
    pub width: Points,
    /// The height.
    pub height: Points,
}

/// A picture control's placeholder picture: what it shows while it shows its placeholder (`w:showingPlcHdr`), in the
/// editor, the saved DOCX and a PDF alike.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlaceholderImage {
    /// The picture: PNG, JPEG, GIF, BMP or SVG.
    pub data: Bytes,
    /// Its MIME type (`image/png`, `image/svg+xml`); the data must be one.
    pub mime: String,
}

/// A picture control's picture size (1…1584 pt): its placeholder's, and the picture's once filled (fitted to it).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PictureOptions {
    /// The width; default 144 pt when inserted, else kept.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<Points>,
    /// The height; default 108 pt when inserted, else kept.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<Points>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keywords_read_before_values() {
        let auto: AutoOr<Color> = serde_json::from_str("\"auto\"").expect("auto");
        assert_eq!(auto, AutoOr::Auto);
        let red: AutoOr<Color> = serde_json::from_str("\"#FF0000\"").expect("a colour");
        assert_eq!(red, AutoOr::Value(Color(0xFF0000)));
        assert!(serde_json::from_str::<Color>("\"red\"").is_err());
        assert_eq!(serde_json::to_string(&Color(0x0563C1)).ok().as_deref(), Some("\"#0563C1\""));
    }
}
