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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AutoOr<T> {
    /// The document decides.
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

/// Standard base64 with padding.
pub(crate) fn base64(data: &[u8]) -> String {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for k in 0..4 {
            out.push(if k <= c.len() { T[(n >> (18 - 6 * k) & 63) as usize] as char } else { '=' });
        }
    }
    out
}

/// Standard base64 with optional padding; `None` on any other character.
fn from_base64(s: &str) -> Option<Vec<u8>> {
    let digit = |c: u8| match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    };
    let s = s.trim_end_matches('=').as_bytes();
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    for chunk in s.chunks(4) {
        let mut n = 0u32;
        for (k, &c) in chunk.iter().enumerate() {
            n |= u32::from(digit(c)?) << (18 - 6 * k);
        }
        let bytes = n.to_be_bytes();
        match chunk.len() {
            4 => out.extend_from_slice(&bytes[1..4]),
            3 => out.extend_from_slice(&bytes[1..3]),
            2 => out.push(bytes[1]),
            _ => return None,
        }
    }
    Some(out)
}

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
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
}

/// A checkbox's state.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CheckboxOptions {
    /// Whether it is checked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checked: Option<bool>,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_round_trips() {
        for data in [&b""[..], b"a", b"ab", b"abc", b"abcd", &[0, 255, 128, 7, 9]] {
            assert_eq!(from_base64(&base64(data)).as_deref(), Some(data));
        }
        assert_eq!(from_base64("a$=="), None);
    }

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
