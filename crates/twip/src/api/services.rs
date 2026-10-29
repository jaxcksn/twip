//! The host services' models: menus, dialogs, find, spelling, navigation, styles, elements and data parts,
//! accessibility, shortcuts, gestures, rulers and word count. The engine computes them; applications draw them.

use std::collections::BTreeMap;

use serde::de::Error as _;
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::command::{Command, CommandId, ElementField};
use super::event::HostCommand;
use super::state::{ElementKind, Tri};
use super::value::{
    Alignment, AutoOr, CellAlignment, Color, CommentId, ControlLock, ControlOptions, DataBinding, ElementId, HeadingId, HorizontalAlignment,
    LineSpacing, MatchId, Order, Orientation, OutlineLevel, Points, RevisionId, RowHeight, Story, StyleId, TabAlignment, TabLeader, TableWidth,
    UnderlineStyle,
};

/// A point relative to the browser viewport (or the control), CSS px: where to place a fixed-position menu.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClientPoint {
    /// From the viewport's left edge.
    pub client_x: f64,
    /// From the viewport's top edge.
    pub client_y: f64,
}

/// A rectangle in the [`ClientPoint`] space.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClientRect {
    /// Its left edge.
    pub left: f64,
    /// Its top edge.
    pub top: f64,
    /// Its width.
    pub width: f64,
    /// Its height.
    pub height: f64,
}

/// A point relative to the editor's balloon layer, CSS px: it scrolls and zooms with the document.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContentPoint {
    /// From the layer's left edge.
    pub x: f64,
    /// From the layer's top edge.
    pub y: f64,
}

// ---------------- context menu ----------------

/// The context menu for the selection.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContextMenu {
    /// What the menu is for.
    pub target: MenuTarget,
    /// Its entries, in order.
    pub items: Vec<MenuItem>,
}

/// What the menu is for.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum MenuTarget {
    /// Text.
    Text,
    /// A misspelled word.
    Misspelling {
        /// The word.
        word: String,
        /// Its language tag (`en-US`).
        language: String,
    },
    /// A hyperlink.
    Link {
        /// Its address.
        address: String,
    },
    /// A table.
    Table,
    /// A picture.
    Picture,
    /// A text box.
    TextBox,
    /// A field or content control.
    Element {
        /// The element.
        id: ElementId,
    },
    /// A tracked change.
    Revision {
        /// The revision.
        id: RevisionId,
    },
    /// A comment.
    Comment {
        /// The comment.
        id: CommentId,
    },
    /// A header or footer.
    HeaderFooter,
}

/// A menu entry.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum MenuItem {
    /// A command, as the catalog describes it.
    #[serde(rename_all = "camelCase")]
    Command {
        /// The command.
        command: CommandId,
        /// The English label.
        label: String,
        /// Whether it can run now.
        enabled: bool,
        /// Its toggle state, for a toggle.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        checked: Option<bool>,
        /// The display string (`Ctrl+X`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        shortcut: Option<String>,
        /// What choosing it does.
        action: MenuAction,
    },
    /// A spelling suggestion, Ignore or Add to Dictionary.
    Suggestion {
        /// The label.
        label: String,
        /// What choosing it does.
        action: MenuAction,
    },
    /// A submenu.
    Submenu {
        /// The label.
        label: String,
        /// Its entries.
        items: Vec<MenuItem>,
    },
    /// A separator line.
    Separator,
}

/// What a menu item does, passed back to `invoke` (opaque to applications).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum MenuAction {
    /// Runs a command.
    Command {
        /// The command.
        command: Command,
    },
    /// Replaces the misspelling at the selection.
    ReplaceWord {
        /// The replacement.
        with: String,
    },
    /// Ignores a word for this session.
    IgnoreWord {
        /// The word.
        word: String,
    },
    /// Adds a word to the user's dictionary (emits `wordAdded`).
    AddWord {
        /// The word.
        word: String,
        /// Its language tag.
        language: String,
    },
}

/// How a context menu was opened.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MenuVia {
    /// A right-click.
    #[default]
    Pointer,
    /// The context-menu key or Shift+F10.
    Keyboard,
}

/// A context menu to open at a point.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContextMenuEvent {
    /// The menu.
    pub menu: ContextMenu,
    /// Where to show it.
    pub at: ClientPoint,
    /// A right-click, or the keyboard.
    pub via: MenuVia,
}

// ---------------- dialogs ----------------

/// A dialog's values, and its patch: the same fields, each present only when changed.
macro_rules! dialog {
    ($(#[$m:meta])* pub struct $name:ident / $patch:ident { $($(#[$fm:meta])* pub $f:ident: $t:ty,)* }) => {
        $(#[$m])*
        pub struct $name {
            $($(#[$fm])* pub $f: $t,)*
        }

        #[doc = concat!("The fields of [`", stringify!($name), "`] that changed.")]
        #[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        pub struct $patch {
            $(
                #[doc = concat!("[`", stringify!($name), "::", stringify!($f), "`], when changed.")]
                #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "present")] pub $f: Option<$t>,
            )*
        }

        impl $patch {
            /// Sets the changed fields on `values`.
            pub fn apply(self, values: &mut $name) {
                $(if let Some(v) = self.$f { values.$f = v; })*
            }
        }
    };
}

/// A patch field that is present: `null` is its value, not its absence.
fn present<'de, D: Deserializer<'de>, T: Deserialize<'de>>(d: D) -> Result<Option<T>, D::Error> {
    T::deserialize(d).map(Some)
}

/// The dialogs the engine models.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DialogKind {
    /// Font.
    Font,
    /// Paragraph.
    Paragraph,
    /// Page Setup.
    PageSetup,
    /// Insert Table.
    InsertTable,
    /// Table Properties.
    TableProperties,
    /// Picture Size.
    PictureSize,
    /// Alt Text.
    AltText,
    /// Hyperlink.
    Hyperlink,
    /// Tabs.
    Tabs,
    /// The accessibility checker.
    Accessibility,
}

/// An open dialog: its values for the selection, and each field's label and state.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DialogModel {
    /// Which dialog.
    pub kind: DialogKind,
    /// Its values for the selection.
    pub values: DialogValues,
    /// By field name (the values' camelCase keys).
    pub fields: BTreeMap<String, FieldInfo>,
}

/// A dialog's values, tagged with its kind.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum DialogValues {
    /// The Font dialog's.
    Font(FontDialog),
    /// The Paragraph dialog's.
    Paragraph(ParagraphDialog),
    /// The Page Setup dialog's.
    PageSetup(PageSetupDialog),
    /// The Insert Table dialog's.
    InsertTable(InsertTableDialog),
    /// The Table Properties dialog's.
    TableProperties(TablePropertiesDialog),
    /// The Picture Size dialog's.
    PictureSize(PictureSizeDialog),
    /// The Alt Text dialog's.
    AltText(AltTextDialog),
    /// The Hyperlink dialog's.
    Hyperlink(HyperlinkDialog),
    /// The Tabs dialog's.
    Tabs(TabsDialog),
    /// The accessibility checker's.
    Accessibility(AccessibilityDialog),
}

/// What `dialog_update` changes: the kind-tagged values with only the changed fields.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum DialogPatch {
    /// The Font dialog's.
    Font(FontPatch),
    /// The Paragraph dialog's.
    Paragraph(ParagraphPatch),
    /// The Page Setup dialog's.
    PageSetup(PageSetupPatch),
    /// The Insert Table dialog's.
    InsertTable(InsertTablePatch),
    /// The Table Properties dialog's.
    TableProperties(TablePropertiesPatch),
    /// The Picture Size dialog's.
    PictureSize(PictureSizePatch),
    /// The Alt Text dialog's.
    AltText(AltTextPatch),
    /// The Hyperlink dialog's.
    Hyperlink(HyperlinkPatch),
    /// The Tabs dialog's.
    Tabs(TabsPatch),
    /// The accessibility checker's.
    Accessibility(AccessibilityPatch),
}

impl DialogPatch {
    /// Sets the changed fields on `values`; `false` when they are another dialog's.
    pub fn apply(self, values: &mut DialogValues) -> bool {
        match (self, values) {
            (DialogPatch::Font(p), DialogValues::Font(v)) => p.apply(v),
            (DialogPatch::Paragraph(p), DialogValues::Paragraph(v)) => p.apply(v),
            (DialogPatch::PageSetup(p), DialogValues::PageSetup(v)) => p.apply(v),
            (DialogPatch::InsertTable(p), DialogValues::InsertTable(v)) => p.apply(v),
            (DialogPatch::TableProperties(p), DialogValues::TableProperties(v)) => p.apply(v),
            (DialogPatch::PictureSize(p), DialogValues::PictureSize(v)) => p.apply(v),
            (DialogPatch::AltText(p), DialogValues::AltText(v)) => p.apply(v),
            (DialogPatch::Hyperlink(p), DialogValues::Hyperlink(v)) => p.apply(v),
            (DialogPatch::Tabs(p), DialogValues::Tabs(v)) => p.apply(v),
            (DialogPatch::Accessibility(p), DialogValues::Accessibility(v)) => p.apply(v),
            _ => return false,
        }
        true
    }
}

/// How a dialog field is edited.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FieldKind {
    /// A plain number.
    #[default]
    Number,
    /// A length in points, shown in the ruler unit.
    Length,
    /// Text.
    Text,
    /// One of [`FieldInfo::options`].
    Choice,
    /// A check box.
    Toggle,
    /// A color.
    Color,
    /// A list of values.
    List,
}

/// One choice of a choice field.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChoiceOption {
    /// The value the field takes.
    pub value: String,
    /// The English label.
    pub label: String,
}

/// A dialog field's label, kind, range and state.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FieldInfo {
    /// The English label.
    pub label: String,
    /// Whether it can be changed now.
    pub enabled: bool,
    /// How it is edited.
    pub kind: FieldKind,
    /// The smallest value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    /// The largest value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    /// The step.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<f64>,
    /// A choice's options.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options: Option<Vec<ChoiceOption>>,
}

/// The Font dialog's style box.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FontStyle {
    /// Regular.
    #[default]
    Regular,
    /// Italic.
    Italic,
    /// Bold.
    Bold,
    /// Bold italic.
    BoldItalic,
}

dialog! {
    /// The Font dialog.
    #[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase", default, deny_unknown_fields)]
    pub struct FontDialog / FontPatch {
        /// The font name.
        pub font: Tri<String>,
        /// Regular, italic, bold or both.
        pub style: Tri<FontStyle>,
        /// The size.
        pub size: Tri<Points>,
        /// The text color.
        pub color: Tri<AutoOr<Color>>,
        /// The underline.
        pub underline: Tri<UnderlineStyle>,
        /// The underline's color.
        pub underline_color: Tri<AutoOr<Color>>,
        /// Strikethrough.
        pub strikethrough: Tri<bool>,
        /// Double strikethrough.
        pub double_strikethrough: Tri<bool>,
        /// Superscript.
        pub superscript: Tri<bool>,
        /// Subscript.
        pub subscript: Tri<bool>,
        /// Small caps.
        pub small_caps: Tri<bool>,
        /// All caps.
        pub all_caps: Tri<bool>,
        /// Hidden text.
        pub hidden: Tri<bool>,
        /// Extra space between characters; negative condenses.
        pub character_spacing: Tri<Points>,
        /// The baseline offset; positive raises.
        pub position: Tri<Points>,
    }
}

/// The Paragraph dialog's special indent.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SpecialIndent {
    /// No first-line indent.
    #[default]
    None,
    /// The first line indents further.
    FirstLine,
    /// The lines after the first indent further.
    Hanging,
}

dialog! {
    /// The Paragraph dialog.
    #[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase", default, deny_unknown_fields)]
    pub struct ParagraphDialog / ParagraphPatch {
        /// The alignment.
        pub alignment: Tri<Alignment>,
        /// The outline level.
        pub outline_level: Tri<OutlineLevel>,
        /// The left indent.
        pub left: Tri<Points>,
        /// The right indent.
        pub right: Tri<Points>,
        /// The special indent.
        pub special: Tri<SpecialIndent>,
        /// The special indent's size.
        pub by: Tri<Points>,
        /// The space before.
        pub before: Tri<AutoOr<Points>>,
        /// The space after.
        pub after: Tri<AutoOr<Points>>,
        /// The line spacing.
        pub line_spacing: Tri<LineSpacing>,
        /// Keep with next.
        pub keep_with_next: Tri<bool>,
        /// Keep lines together.
        pub keep_lines_together: Tri<bool>,
        /// Page break before.
        pub page_break_before: Tri<bool>,
        /// Widow and orphan control.
        pub widow_control: Tri<bool>,
    }
}

/// Where page setup applies.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ApplyTo {
    /// The selection's section.
    #[default]
    Section,
    /// Every section.
    WholeDocument,
}

dialog! {
    /// The Page Setup dialog.
    #[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase", default, deny_unknown_fields)]
    pub struct PageSetupDialog / PageSetupPatch {
        /// The top margin.
        pub top: Points,
        /// The bottom margin.
        pub bottom: Points,
        /// The left margin.
        pub left: Points,
        /// The right margin.
        pub right: Points,
        /// The gutter.
        pub gutter: Points,
        /// Portrait or landscape.
        pub orientation: Orientation,
        /// The paper width.
        pub width: Points,
        /// The paper height.
        pub height: Points,
        /// The header's distance from the page's top edge.
        pub header_distance: Points,
        /// The footer's distance from the page's bottom edge.
        pub footer_distance: Points,
        /// A different first-page header and footer.
        pub different_first_page: bool,
        /// Different odd and even headers and footers.
        pub different_odd_even: bool,
        /// Where it applies.
        pub apply_to: ApplyTo,
    }
}

dialog! {
    /// The Insert Table dialog.
    #[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase", default, deny_unknown_fields)]
    pub struct InsertTableDialog / InsertTablePatch {
        /// The number of rows.
        pub rows: u32,
        /// The number of columns.
        pub columns: u32,
    }
}

/// Cell margins, all four set.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CellMarginValues {
    /// The top margin.
    pub top: Points,
    /// The bottom margin.
    pub bottom: Points,
    /// The left margin.
    pub left: Points,
    /// The right margin.
    pub right: Points,
}

dialog! {
    /// The Table Properties dialog.
    #[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase", default, deny_unknown_fields)]
    pub struct TablePropertiesDialog / TablePropertiesPatch {
        /// The table's preferred width.
        pub width: TableWidth,
        /// The table's alignment on the page.
        pub alignment: HorizontalAlignment,
        /// The table's indent from the left margin.
        pub indent: Points,
        /// The selected rows' height.
        pub row_height: Tri<AutoOr<RowHeight>>,
        /// Whether the selected rows can break across pages.
        pub row_can_break: Tri<bool>,
        /// Whether the selected rows repeat as a header on each page.
        pub header_row: Tri<bool>,
        /// The selected columns' width.
        pub column_width: Tri<Points>,
        /// The selected cells' vertical alignment.
        pub cell_alignment: Tri<CellAlignment>,
        /// The table's default cell margins.
        pub cell_margins: CellMarginValues,
    }
}

dialog! {
    /// The Picture Size dialog.
    #[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase", default, deny_unknown_fields)]
    pub struct PictureSizeDialog / PictureSizePatch {
        /// The width.
        pub width: Points,
        /// The height.
        pub height: Points,
        /// Percent of the original size.
        pub scale_width: f64,
        /// Percent of the original height.
        pub scale_height: f64,
        /// Width and height change together.
        pub lock_aspect: bool,
        /// Back to the original size on apply.
        pub reset: bool,
    }
}

dialog! {
    /// The Alt Text dialog.
    #[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase", default, deny_unknown_fields)]
    pub struct AltTextDialog / AltTextPatch {
        /// The description.
        pub text: String,
        /// Marked decorative: screen readers skip it.
        pub decorative: bool,
    }
}

dialog! {
    /// The Hyperlink dialog.
    #[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase", default, deny_unknown_fields)]
    pub struct HyperlinkDialog / HyperlinkPatch {
        /// The text to display.
        pub text: String,
        /// The address.
        pub address: String,
    }
}

/// The Tabs dialog's buttons, applied by `update`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TabsAction {
    /// No button.
    #[default]
    None,
    /// Set: adds or changes the stop at `position`.
    Set,
    /// Clear: clears the stop at `position`.
    Clear,
    /// Clear All.
    ClearAll,
}

dialog! {
    /// The Tabs dialog.
    #[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase", default, deny_unknown_fields)]
    pub struct TabsDialog / TabsPatch {
        /// The paragraph's tab stops.
        pub stops: Vec<TabStopInfo>,
        /// The document's default tab interval.
        pub default_stop: Points,
        /// The stop being edited.
        pub position: Option<Points>,
        /// The edited stop's alignment.
        pub alignment: TabAlignment,
        /// The edited stop's leader.
        pub leader: TabLeader,
        /// Stops to clear on apply.
        pub cleared: Vec<Points>,
        /// The button pressed.
        pub action: TabsAction,
    }
}

/// One accessibility issue.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AccessibilityIssue {
    /// The issue's id.
    pub id: String,
    /// Its description.
    pub text: String,
    /// A tip rather than an error.
    pub tip: bool,
}

dialog! {
    /// The Accessibility dialog (read only).
    #[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase", default, deny_unknown_fields)]
    pub struct AccessibilityDialog / AccessibilityPatch {
        /// The issues found.
        pub issues: Vec<AccessibilityIssue>,
    }
}

// ---------------- find, spelling, navigation, styles ----------------

/// What to find.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FindQuery {
    /// The text.
    pub text: String,
    /// Match case (off when unset).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub match_case: Option<bool>,
    /// Whole words only (off when unset).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub whole_words: Option<bool>,
}

/// Which way `find` searches.
pub type FindDirection = Order;

/// Whether a match was found, and whether the search wrapped past the end.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FindResult {
    /// A match was found and selected.
    pub found: bool,
    /// The search wrapped around the document's end.
    pub wrapped: bool,
}

/// One match with its context, for a results list.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FindMatch {
    /// The match, for selecting it.
    pub id: MatchId,
    /// Some text before it.
    pub before: String,
    /// The matched text.
    pub text: String,
    /// Some text after it.
    pub after: String,
    /// Its page.
    pub page: u32,
}

/// A misspelled word and what to replace it with.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Misspelling {
    /// The word.
    pub word: String,
    /// Replacements, best first.
    pub suggestions: Vec<String>,
}

/// A heading, for a navigation pane.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Heading {
    /// The heading.
    pub id: HeadingId,
    /// Its outline level, from 1.
    pub level: u32,
    /// Its text.
    pub text: String,
    /// Its page.
    pub page: u32,
}

/// The kinds of style.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StyleType {
    /// A paragraph style.
    #[default]
    Paragraph,
    /// A character style.
    Character,
    /// A table style.
    Table,
}

/// How a style gallery tile draws.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StylePreview {
    /// The font name.
    pub font: String,
    /// The size.
    pub size: Points,
    /// Bold.
    pub bold: bool,
    /// Italic.
    pub italic: bool,
    /// The text color.
    pub color: AutoOr<Color>,
}

impl Default for StylePreview {
    fn default() -> StylePreview {
        StylePreview { font: String::new(), size: Points(11.0), bold: false, italic: false, color: AutoOr::Auto }
    }
}

/// A style, for a style gallery or list.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StyleInfo {
    /// The style.
    pub id: StyleId,
    /// Its display name.
    pub name: String,
    /// Paragraph, character or table.
    #[serde(rename = "type")]
    pub kind: StyleType,
    /// Built in.
    pub built_in: bool,
    /// Shown in a style gallery (`w:qFormat`).
    pub quick: bool,
    /// The sort priority.
    pub priority: u32,
    /// Hidden from style lists.
    pub hidden: bool,
    /// The style it is based on.
    pub based_on: Option<StyleId>,
    /// For drawing a gallery tile.
    pub preview: StylePreview,
}

// ---------------- elements and data parts ----------------

/// A text to match in an element property.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ElementMatch {
    /// The property: a field's code, or a control's tag or title.
    #[serde(rename = "in")]
    pub field: ElementField,
    /// The text it contains.
    pub text: String,
}

/// Which elements `elements` lists.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ElementFilter {
    /// Only this kind (any when unset).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<ElementKind>,
    /// Only those whose property contains a text.
    #[serde(rename = "match", default, skip_serializing_if = "Option::is_none")]
    pub matches: Option<ElementMatch>,
    /// Only those in this story (any when unset).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub story: Option<Story>,
}

/// A field or content control.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ElementInfo {
    /// The element.
    pub id: ElementId,
    /// What kind it is.
    pub kind: ElementKind,
    /// A field's code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// A control's tag.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    /// A control's title.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// The result or content text, up to 200 characters.
    pub display: String,
    /// The story it is in.
    pub story: Story,
    /// The element it is in.
    pub parent: Option<ElementId>,
    /// Locked against editing.
    pub locked: bool,
}

/// An element with its settings and children.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ElementDetail {
    /// The element.
    pub id: ElementId,
    /// What kind it is.
    pub kind: ElementKind,
    /// A field's code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// A control's tag.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    /// A control's title.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// The result or content text, up to 200 characters.
    pub display: String,
    /// The story it is in.
    pub story: Story,
    /// The element it is in.
    pub parent: Option<ElementId>,
    /// Locked against editing.
    pub locked: bool,
    /// A control's lock.
    pub lock: ControlLock,
    /// A control's placeholder text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    /// A control's data binding.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binding: Option<DataBinding>,
    /// A control's options.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options: Option<ControlOptions>,
    /// The elements inside it.
    pub children: Vec<ElementId>,
}

/// A cell's place in a vertical merge.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VerticalMerge {
    /// Not merged.
    #[default]
    None,
    /// The first cell of a merge.
    Start,
    /// A cell merged into the one above.
    Continue,
}

/// The document (or an element) as blocks, for checking a template's structure.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum Block {
    /// A paragraph.
    #[serde(rename_all = "camelCase")]
    Paragraph {
        /// Its text's length, in characters.
        text_length: u32,
        /// The elements in it.
        elements: Vec<ElementId>,
    },
    /// A table.
    Table {
        /// Its rows.
        rows: Vec<BlockRow>,
    },
}

/// A table row of [`Block::Table`].
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BlockRow {
    /// Its cells.
    pub cells: Vec<BlockCell>,
}

/// A cell of [`BlockRow`].
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BlockCell {
    /// The grid columns it spans.
    pub span: u32,
    /// Its place in a vertical merge.
    pub vertical_merge: VerticalMerge,
    /// It has no text.
    pub empty: bool,
    /// Its content.
    pub blocks: Vec<Block>,
}

/// A custom XML data part.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DataPart {
    /// Its root element's namespace.
    pub namespace: Option<String>,
    /// Its item id (`{GUID}`), which data bindings name.
    pub item_id: String,
    /// Its XML.
    pub xml: String,
}

/// A custom XML data part, without its content.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DataPartInfo {
    /// Its root element's namespace.
    pub namespace: Option<String>,
    /// Its item id (`{GUID}`), which data bindings name.
    pub item_id: String,
}

/// An unfilled picture placeholder the user activated.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlaceholderInfo {
    /// The placeholder's key.
    pub key: String,
    /// The content control it is in.
    pub element: Option<ElementId>,
}

// ---------------- accessibility, shortcuts, gestures ----------------

/// An accessibility tree node's role.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AccessibleRole {
    /// The document.
    #[default]
    Document,
    /// A page.
    Page,
    /// A paragraph.
    Paragraph,
    /// A heading.
    Heading,
    /// A list.
    List,
    /// A list item.
    ListItem,
    /// A table.
    Table,
    /// A table row.
    Row,
    /// A table cell.
    Cell,
    /// A picture.
    Image,
    /// A hyperlink.
    Link,
    /// A text box.
    TextBox,
    /// A footnote, endnote or comment.
    Note,
    /// A header.
    Header,
    /// A footer.
    Footer,
}

/// What an accessibility node is marked with.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AccessibleMark {
    /// A tracked insertion.
    Insertion,
    /// A tracked deletion.
    Deletion,
    /// A comment.
    Comment,
    /// A misspelling.
    Misspelling,
}

/// The document's accessibility tree.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AccessibleNode {
    /// Its role.
    pub role: AccessibleRole,
    /// Its accessible name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Its text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// A heading's or list item's level.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<u32>,
    /// Review and spelling marks on it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub marks: Option<Vec<AccessibleMark>>,
    /// Its children.
    pub children: Vec<AccessibleNode>,
}

/// A key on every platform, or one per platform (`Ctrl+Alt+K`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ShortcutKeys {
    /// The same keys everywhere.
    Same(String),
    /// Different keys per platform.
    PerPlatform {
        /// On Windows and Linux.
        windows: String,
        /// On macOS.
        mac: String,
    },
}

/// An application shortcut: pressing its keys emits `shortcut` with its id.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Shortcut {
    /// The application's id for it.
    pub id: String,
    /// Its keys.
    pub keys: ShortcutKeys,
}

/// What a key does now.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum KeyBinding {
    /// Runs a command.
    Command {
        /// The command.
        id: CommandId,
    },
    /// Emits an application shortcut.
    Shortcut {
        /// The shortcut's id.
        id: String,
    },
    /// Emits a host command (Save, Print, Find).
    Host {
        /// The host command.
        command: HostCommand,
    },
}

/// The pointer gestures the engine recognises.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GestureId {
    /// A double-click in the header area: edits the header.
    HeaderAreaDoubleClick,
    /// A double-click in the footer area: edits the footer.
    FooterAreaDoubleClick,
    /// A double-click on the body while editing a header or footer: back to the body.
    BodyDoubleClickInHeaderFooter,
    /// A double-click on text: selects a word.
    TextDoubleClick,
    /// A triple-click on text: selects a paragraph.
    TextTripleClick,
    /// A Ctrl+click on text.
    TextCtrlClick,
    /// A Ctrl+click on a hyperlink.
    LinkActivate,
    /// A click in the left margin.
    MarginClick,
    /// A double-click in the left margin.
    MarginDoubleClick,
    /// A double-click on a table's column border.
    TableBorderDoubleClick,
    /// Dragging a table border: resizes.
    TableBorderDrag,
    /// Dragging a picture or text box.
    ObjectDrag,
    /// Dragging selected text: moves it.
    TextDrag,
    /// A double-click on a field or content control in design mode.
    ElementDoubleClick,
    /// A click on a picture placeholder.
    PlaceholderClick,
    /// A click on the ruler: adds a tab stop.
    RulerClick,
    /// Dragging a ruler marker.
    RulerDrag,
    /// Dragging a tab stop off the ruler: removes it.
    RulerDragOff,
    /// A double-click on the ruler: requests the Tabs dialog.
    RulerDoubleClick,
    /// A right-click: the context menu.
    ContextClick,
}

/// What a gesture does: its default action, an event for the application, nothing, or another command.
/// On the wire: `'default'`, `'emit'`, `'none'` or `{ "op": command }`.
#[derive(Clone, Debug, PartialEq)]
#[allow(clippy::large_enum_variant, reason = "bindings are a handful per session, set rarely")]
pub enum GestureAction {
    /// Its built-in action.
    Default,
    /// Emits `gesture` instead.
    Emit,
    /// Does nothing.
    None,
    /// Runs this command instead.
    Op(Command),
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct OpAction {
    op: Command,
}

impl Serialize for GestureAction {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            GestureAction::Default => s.serialize_str("default"),
            GestureAction::Emit => s.serialize_str("emit"),
            GestureAction::None => s.serialize_str("none"),
            GestureAction::Op(op) => {
                let mut m = s.serialize_map(Some(1))?;
                m.serialize_entry("op", op)?;
                m.end()
            }
        }
    }
}

impl<'de> Deserialize<'de> for GestureAction {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<GestureAction, D::Error> {
        match serde_json::Value::deserialize(d)? {
            serde_json::Value::String(s) => match s.as_str() {
                "default" => Ok(GestureAction::Default),
                "emit" => Ok(GestureAction::Emit),
                "none" => Ok(GestureAction::None),
                other => Err(D::Error::custom(format!("unknown gesture action `{other}`"))),
            },
            v => OpAction::deserialize(v).map(|a| GestureAction::Op(a.op)).map_err(D::Error::custom),
        }
    }
}

/// A gesture's binding, with a label for a settings screen (`Double-click the header area`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GestureBinding {
    /// The gesture.
    pub gesture: GestureId,
    /// What it does.
    pub action: GestureAction,
    /// For example `Double-click the header area`.
    pub label: String,
}

/// Page areas a gesture can be over.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AreaKind {
    /// The header area.
    HeaderArea,
    /// The footer area.
    FooterArea,
    /// The left margin.
    Margin,
    /// A ruler.
    Ruler,
    /// A table border.
    TableBorder,
}

/// What a gesture is over.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum GestureTarget {
    /// Document content, as a context menu names it.
    Menu(MenuTarget),
    /// A page area.
    Area {
        /// Which area.
        kind: AreaKind,
    },
}

/// A gesture bound to `emit`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GestureEvent {
    /// The gesture.
    pub gesture: GestureId,
    /// Where.
    pub at: ClientPoint,
    /// The page.
    pub page: u32,
    /// What is under the point.
    pub target: GestureTarget,
}

// ---------------- rulers, view, hit testing ----------------

/// The unit rulers show.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RulerUnit {
    /// Inches.
    #[default]
    Inches,
    /// Centimeters.
    Centimeters,
    /// Millimeters.
    Millimeters,
    /// Points.
    Points,
    /// Picas.
    Picas,
}

/// What a click on the ruler adds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TabSelector {
    /// A left tab stop.
    #[default]
    Left,
    /// A center tab stop.
    Center,
    /// A right tab stop.
    Right,
    /// A decimal tab stop.
    Decimal,
    /// A bar tab.
    Bar,
    /// Sets the first-line indent.
    FirstLineIndent,
    /// Sets the hanging indent.
    HangingIndent,
}

/// A span along a ruler.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Span {
    /// Where it starts, from the page edge.
    pub start: Points,
    /// Where it ends, from the page edge.
    pub end: Points,
}

/// The indent markers. `first_line` is relative to `left`; negative is hanging.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RulerIndents {
    /// The left indent.
    pub left: Points,
    /// The right indent.
    pub right: Points,
    /// The first line's indent from `left`.
    pub first_line: Points,
}

/// A tab stop, measured from the paragraph's left text edge.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TabStopInfo {
    /// Its position.
    pub position: Points,
    /// Its alignment.
    pub alignment: TabAlignment,
    /// Its leader.
    pub leader: TabLeader,
    /// From the paragraph's style.
    pub inherited: bool,
}

/// The horizontal ruler's model.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HorizontalRuler {
    /// The page's width.
    pub page_width: Points,
    /// The margins, or the cell or text column around the caret.
    pub text: Span,
    /// The paragraph's indents.
    pub indents: RulerIndents,
    /// The paragraph's tab stops.
    pub tabs: Vec<TabStopInfo>,
    /// The document's default tab interval.
    pub default_tab_stop: Points,
    /// The section's text columns.
    pub columns: Vec<Span>,
    /// The cell boundaries of the caret's row.
    pub table_columns: Vec<Points>,
}

/// The vertical ruler's model.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VerticalRuler {
    /// The page's height.
    pub page_height: Points,
    /// The top and bottom margins.
    pub text: Span,
    /// The row boundaries of the caret's table on this page.
    pub table_rows: Vec<Points>,
}

/// The ruler model for the selection, in points, with no viewport mapping.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RulerModel {
    /// The selected paragraphs differ: the first one's values are shown.
    pub mixed: bool,
    /// The horizontal ruler.
    pub horizontal: HorizontalRuler,
    /// The vertical ruler.
    pub vertical: VerticalRuler,
}

/// A ruler's mapping to the viewport: the client coordinate of the page edge, and CSS px per point.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RulerAxis {
    /// The page edge's client coordinate, CSS px.
    pub origin: f64,
    /// CSS px per point.
    pub scale: f64,
}

/// The ruler model with its viewport mapping, for an application drawing its own ruler.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RulerView {
    /// The model.
    pub model: RulerModel,
    /// The page the rulers follow (the caret's).
    pub page: u32,
    /// The horizontal ruler's mapping.
    pub horizontal: RulerAxis,
    /// The vertical ruler's mapping.
    pub vertical: RulerAxis,
    /// The unit to label.
    pub unit: RulerUnit,
}

/// A visible page and where it is.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PageRect {
    /// The page's index, from 0.
    pub page: u32,
    /// Its box in the viewport.
    pub rect: ClientRect,
}

/// What is under a point.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HitResult {
    /// The innermost field or content control there.
    pub element: Option<ElementId>,
    /// Text there can be edited.
    pub editable: bool,
}

/// What a tooltip is about.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TooltipKind {
    /// A hyperlink.
    #[default]
    Link,
    /// A marked range.
    Mark,
    /// A tracked change.
    Revision,
    /// A comment.
    Comment,
}

/// A tooltip to show over the document.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Tooltip {
    /// What is hovered.
    pub kind: TooltipKind,
    /// The text to show.
    pub text: String,
    /// The pointer.
    pub at: ClientPoint,
    /// The hovered thing's box.
    pub rect: ClientRect,
}

/// The document's counts, computed on request.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WordCount {
    /// Words.
    pub words: u32,
    /// Characters, without spaces.
    pub characters: u32,
    /// Characters, with spaces.
    pub characters_with_spaces: u32,
    /// Paragraphs.
    pub paragraphs: u32,
    /// Lines.
    pub lines: u32,
    /// Pages.
    pub pages: u32,
}
