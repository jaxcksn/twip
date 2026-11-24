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
    Alignment, AutoOr, Bytes, CellAlignment, Color, CommentId, ControlLock, ControlOptions, DataBinding, ElementId, HeadingId, HorizontalAlignment,
    LineDirection, LineSpacing, MatchId, NoneOr, Order, Orientation, OutlineLevel, Points, RevisionId, RowHeight, Story, StyleId, TabAlignment,
    TabLeader, TableWidth, UnderlineStyle,
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
    /// Where a right-button drag of text dropped it: Move Here, Copy Here, Link Here, Create Hyperlink Here, Cancel.
    Drop,
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
        /// Shown as a row of icons in the menu itself rather than as a submenu (Paste Options).
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        inline: bool,
    },
    /// An action that is not one command: opening or copying a link, a choice of the drop menu, several commands at
    /// once, a setting.
    Action {
        /// The English label.
        label: String,
        /// Whether it can run now.
        enabled: bool,
        /// Whether it is the current choice, for a choice among several.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        checked: Option<bool>,
        /// What choosing it does.
        action: MenuAction,
    },
    /// A separator line.
    Separator,
}

/// A choice of the menu a right-button drag of text opens where it dropped.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DropChoice {
    /// Move Here: the text moves there (tracked, a move). One undo step.
    Move,
    /// Copy Here. One undo step.
    Copy,
    /// Link Here: a `LINK` field there to the text, which is bookmarked; needs the document's file name. One undo step.
    Link,
    /// Create Hyperlink Here: a link there to a hidden bookmark on the text, showing the text. One undo step.
    Hyperlink,
    /// Cancel: nothing changes.
    Cancel,
}

/// What a menu item does, passed back to `invoke` (opaque to applications).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum MenuAction {
    /// Runs a command.
    Command {
        /// The command.
        command: Box<Command>,
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
    /// Opens a link (emits `linkActivated` with `via: menu`); an address starting `#` goes to that bookmark.
    OpenLink {
        /// The link's address.
        address: String,
    },
    /// Puts a link's address on the clipboard (the `clipboard` host request).
    CopyLink {
        /// The link's address.
        address: String,
    },
    /// Runs several commands as one transaction, one undo step under `label`.
    Commands {
        /// The commands, in order.
        commands: Vec<Command>,
        /// The undo step's label.
        label: String,
    },
    /// Set as Default Layout: pictures inserted later take this wrap. A session setting; nothing changes.
    SetDefaultPictureWrap {
        /// The wrap.
        wrap: crate::api::value::ObjectWrap,
    },
    /// A choice of the drop menu a right-button drag opened.
    Drop {
        /// The choice.
        what: DropChoice,
    },
}

/// The word the selection is in or selects, for a host's thesaurus (the Synonyms submenu).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WordAt {
    /// The word.
    pub word: String,
    /// Its language tag (`en-US`).
    pub language: String,
}

/// Where the selection is on screen, for placing UI beside it (a floating toolbar): client px over the pages in
/// view.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SelectionBounds {
    /// Each selected line's or cell's box; empty for a caret.
    pub rects: Vec<ClientRect>,
    /// Each selected object's frame.
    pub frames: Vec<ObjectFrame>,
    /// The box around every rect and frame, or `None` when nothing selected is in view.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bounds: Option<ClientRect>,
}

/// A selected object's frame as drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ObjectFrame {
    /// Its box before rotation.
    pub rect: ClientRect,
    /// Its rotation, degrees clockwise about the box's centre.
    pub rotation: crate::api::drawing::Degrees,
    /// The upright box around it as turned.
    pub bounds: ClientRect,
}

/// What a drag of the selection out of the editor carries: the host starts its platform's drag with it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DragSource {
    /// The content, as a copy puts it on the clipboard (`internal` names it for a drop in any session).
    pub data: crate::api::input::ClipboardData,
    /// The drag may move the text: the selection can be deleted when the drop takes it.
    pub can_move: bool,
}

/// How a drag out of the editor ended, as the drop target reported it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DropEffect {
    /// Not dropped, or refused.
    #[default]
    None,
    /// Copied: the text stays.
    Copy,
    /// Moved: the text is deleted, as one undo step.
    Move,
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
                $(#[$fm])*
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
pub(crate) fn present<'de, D: Deserializer<'de>, T: Deserialize<'de>>(d: D) -> Result<Option<T>, D::Error> {
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
    /// The AutoCorrect rules.
    AutoCorrect,
    /// Modify Style, and Create New Style from Formatting (`dialog_open_style`).
    Style,
    /// Manage Styles.
    ManageStyles,
    /// Borders and Shading.
    BordersShading,
    /// Frame.
    Frame,
    /// Drop Cap.
    DropCap,
    /// Banding (a table style's band sizes, from the style dialog's Format menu).
    Banding,
    /// Define New Bullet.
    DefineBullet,
    /// Define New Number Format.
    DefineNumberFormat,
    /// Define New Multilevel List.
    DefineMultilevelList,
    /// Set Numbering Value.
    SetNumberingValue,
    /// Adjust List Indents.
    AdjustListIndents,
    /// Bullets and Numbering.
    BulletsNumbering,
    /// Printed Watermark (Custom Watermark).
    PrintedWatermark,
    /// Fill Effects, for the page colour.
    FillEffects,
}

impl DialogKind {
    /// The list dialogs, which read the selection's list.
    pub fn is_list(self) -> bool {
        matches!(
            self,
            DialogKind::DefineBullet
                | DialogKind::DefineNumberFormat
                | DialogKind::DefineMultilevelList
                | DialogKind::SetNumberingValue
                | DialogKind::AdjustListIndents
                | DialogKind::BulletsNumbering
        )
    }
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
    /// A dialog opened from the style dialog's Format menu: the style dialog it changes, not the selection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_style: Option<Box<StyleDialog>>,
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
    /// The style dialog's.
    Style(StyleDialog),
    /// Manage Styles'.
    ManageStyles(ManageStylesDialog),
    /// Borders and Shading's.
    BordersShading(Box<BordersShadingDialog>),
    /// The Frame dialog's.
    Frame(FrameDialog),
    /// The Drop Cap dialog's.
    DropCap(DropCapDialog),
    /// Banding's.
    Banding(BandingDialog),
    /// Define New Bullet's.
    DefineBullet(DefineBulletDialog),
    /// Define New Number Format's.
    DefineNumberFormat(DefineNumberFormatDialog),
    /// Define New Multilevel List's.
    DefineMultilevelList(DefineMultilevelDialog),
    /// Set Numbering Value's.
    SetNumberingValue(SetNumberingValueDialog),
    /// Adjust List Indents's.
    AdjustListIndents(AdjustListIndentsDialog),
    /// Bullets and Numbering's.
    BulletsNumbering(BulletsNumberingDialog),
    /// Printed Watermark's.
    PrintedWatermark(PrintedWatermarkDialog),
    /// Fill Effects'.
    FillEffects(FillEffectsDialog),
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
    /// The style dialog's.
    Style(StylePatch),
    /// Manage Styles'.
    ManageStyles(ManageStylesPatch),
    /// Borders and Shading's.
    BordersShading(Box<BordersShadingPatch>),
    /// The Frame dialog's.
    Frame(FramePatch),
    /// The Drop Cap dialog's.
    DropCap(DropCapPatch),
    /// Banding's.
    Banding(BandingPatch),
    /// Define New Bullet's.
    DefineBullet(DefineBulletPatch),
    /// Define New Number Format's.
    DefineNumberFormat(DefineNumberFormatPatch),
    /// Define New Multilevel List's.
    DefineMultilevelList(DefineMultilevelPatch),
    /// Set Numbering Value's.
    SetNumberingValue(SetNumberingValuePatch),
    /// Adjust List Indents's.
    AdjustListIndents(AdjustListIndentsPatch),
    /// Bullets and Numbering's.
    BulletsNumbering(BulletsNumberingPatch),
    /// Printed Watermark's.
    PrintedWatermark(PrintedWatermarkPatch),
    /// Fill Effects'.
    FillEffects(FillEffectsPatch),
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
            (DialogPatch::Style(p), DialogValues::Style(v)) => p.apply(v),
            (DialogPatch::ManageStyles(p), DialogValues::ManageStyles(v)) => p.apply(v),
            (DialogPatch::BordersShading(p), DialogValues::BordersShading(v)) => p.apply(v),
            (DialogPatch::Frame(p), DialogValues::Frame(v)) => p.apply(v),
            (DialogPatch::DropCap(p), DialogValues::DropCap(v)) => p.apply(v),
            (DialogPatch::Banding(p), DialogValues::Banding(v)) => p.apply(v),
            (DialogPatch::DefineBullet(p), DialogValues::DefineBullet(v)) => p.apply(v),
            (DialogPatch::DefineNumberFormat(p), DialogValues::DefineNumberFormat(v)) => p.apply(v),
            (DialogPatch::DefineMultilevelList(p), DialogValues::DefineMultilevelList(v)) => p.apply(v),
            (DialogPatch::SetNumberingValue(p), DialogValues::SetNumberingValue(v)) => p.apply(v),
            (DialogPatch::AdjustListIndents(p), DialogValues::AdjustListIndents(v)) => p.apply(v),
            (DialogPatch::BulletsNumbering(p), DialogValues::BulletsNumbering(v)) => p.apply(v),
            (DialogPatch::PrintedWatermark(p), DialogValues::PrintedWatermark(v)) => p.apply(v),
            (DialogPatch::FillEffects(p), DialogValues::FillEffects(v)) => p.apply(v),
            _ => return false,
        }
        true
    }
}

/// How the style dialog opens.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StyleOpen {
    /// Create New Style from Formatting: a new style from the selection's formatting.
    #[default]
    Create,
    /// Modify Style.
    Modify(StyleId),
}

/// The kind of style the style dialog makes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StyleDialogType {
    /// A paragraph style.
    #[default]
    Paragraph,
    /// A character style.
    Character,
    /// A paragraph style and a character style, linked.
    Linked,
    /// A table style.
    Table,
    /// A list style.
    List,
}

dialog! {
    /// The style dialog: Modify Style, and Create New Style from Formatting (`short_form`: the name and the preview).
    #[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase", default, deny_unknown_fields)]
    pub struct StyleDialog / StylePatch {
        /// Creating a style, or modifying one.
        pub open: StyleOpen,
        /// The short form: the name and the preview, until "Modify..." turns it off.
        pub short_form: bool,
        /// The style's name.
        pub name: String,
        /// Its type (chosen when creating).
        #[serde(rename = "type")]
        pub kind: StyleDialogType,
        /// The style it is based on.
        pub based_on: Option<StyleId>,
        /// The style of the paragraph after one in this style.
        pub next: Option<StyleId>,
        /// The font.
        pub font: String,
        /// The size.
        pub size: Points,
        /// Bold.
        pub bold: bool,
        /// Italic.
        pub italic: bool,
        /// The underline.
        pub underline: UnderlineStyle,
        /// The font color.
        pub color: AutoOr<Color>,
        /// The alignment.
        pub alignment: Alignment,
        /// The line spacing.
        pub line_spacing: LineSpacing,
        /// The space before paragraphs.
        pub space_before: Points,
        /// The space after paragraphs.
        pub space_after: Points,
        /// The left indent.
        pub left_indent: Points,
        /// Shown in the Styles gallery.
        pub quick: bool,
        /// Direct formatting of a paragraph in the style changes the style.
        pub auto_update: bool,
        /// A table style: the part the formatting applies to.
        pub table_part: TableStylePart,
        /// A table style: the part's cell fill, or none.
        pub fill: Option<Color>,
        /// A table style: the part's cells' vertical alignment.
        pub cell_alignment: CellAlignment,
        /// A table style: the part's border line, on every edge, or none.
        pub border: Option<super::command::Border>,
        /// A table style: the rows in each band.
        pub row_band: u32,
        /// A table style: the columns in each band.
        pub column_band: u32,
        /// A list style: its levels.
        pub list_levels: Vec<super::command::ListLevel>,
        /// A list style: the level shown (0 to 8).
        pub list_level: u8,
        /// The style's formatting in words (written by the editor).
        pub description: String,
        /// The style as changed so far, Format menu dialogs included: give it back unchanged.
        pub pending: String,
    }
}

pub use super::styles::TableStylePart;

/// Manage Styles' tabs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ManageStylesTab {
    /// The styles, to modify, delete or add to.
    #[default]
    Edit,
    /// Which styles show in lists, and in what order.
    Recommend,
    /// Which styles formatting protection permits.
    Restrict,
    /// The document's default formatting.
    SetDefaults,
}

/// How Manage Styles sorts its list.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StyleSort {
    /// By priority, then name.
    #[default]
    AsRecommended,
    /// By name.
    Alphabetical,
    /// By the style each is based on.
    BasedOn,
    /// By type.
    ByType,
}

/// A style in Manage Styles.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManagedStyle {
    /// The style.
    pub id: StyleId,
    /// Its name.
    pub name: String,
    /// Its type.
    #[serde(rename = "type")]
    pub kind: StyleType,
    /// Built in (can't be deleted).
    pub built_in: bool,
    /// Whether it shows in style lists.
    pub visibility: StyleVisibility,
    /// Its sort priority.
    pub priority: u32,
    /// Restricted while formatting is protected.
    pub locked: bool,
}

dialog! {
    /// Manage Styles: its Edit, Recommend, Restrict and Set Defaults tabs. Modify, New Style and Delete are the style
    /// dialog and `deleteStyle`; Import/Export isn't offered.
    #[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase", default, deny_unknown_fields)]
    pub struct ManageStylesDialog / ManageStylesPatch {
        /// The tab shown.
        pub tab: ManageStylesTab,
        /// How the list sorts.
        pub sort: StyleSort,
        /// Only the styles that show in lists.
        pub recommended_only: bool,
        /// The styles, in `sort` order: their visibility, priority and lock are what Recommend and Restrict change.
        pub styles: Vec<ManagedStyle>,
        /// The style selected in the list.
        pub selected: Option<StyleId>,
        /// The selected style's formatting in words (written by the editor).
        pub description: String,
        /// The document's default formatting (Set Defaults).
        pub defaults: super::command::DocumentDefaults,
    }
}

/// Borders and Shading's tabs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BordersTab {
    /// The paragraph's or text's borders.
    #[default]
    Borders,
    /// The page's borders.
    PageBorder,
    /// The shading.
    Shading,
}

/// Borders and Shading's settings: no borders, a box, a box with a shadow, or the edges set one by one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BorderSetting {
    /// No borders.
    #[default]
    None,
    /// The four edges in one line.
    Box,
    /// A box with a shadow on its bottom and right edges.
    Shadow,
    /// The edges set one by one.
    Custom,
}

dialog! {
    /// Borders and Shading: the borders of the selected paragraphs, and the shading of the paragraphs or the text. A
    /// setting (`none`, `box`, `shadow`) sets the edges with the line `style`, `color` and `width`; setting an edge
    /// makes it `custom`. The Page Border tab does the same for the section's pages.
    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    pub struct BordersShadingDialog / BordersShadingPatch {
        /// The tab shown.
        pub tab: BordersTab,
        /// The setting.
        pub setting: BorderSetting,
        /// The line style the setting and new edges take.
        pub style: super::command::BorderStyle,
        /// Their color.
        pub color: AutoOr<Color>,
        /// Their width, 0.25…6 pt.
        pub width: Points,
        /// The top edge, or none.
        pub top: Option<super::command::Border>,
        /// The bottom edge, or none.
        pub bottom: Option<super::command::Border>,
        /// The left edge, or none.
        pub left: Option<super::command::Border>,
        /// The right edge, or none.
        pub right: Option<super::command::Border>,
        /// The line between paragraphs (a table style part's inside horizontal line), or none.
        pub between: Option<super::command::Border>,
        /// A table style part's inside vertical line, or none.
        pub inside_vertical: Option<super::command::Border>,
        /// The cells' diagonal from their top left to their bottom right corner (applied to cells, or a table style's
        /// part), or none.
        pub diagonal_down: Option<super::command::Border>,
        /// The cells' diagonal from their top right to their bottom left corner, or none.
        pub diagonal_up: Option<super::command::Border>,
        /// The paragraphs, or the text (shading only).
        pub apply_to: super::command::ShadingTarget,
        /// The shading's fill, or none.
        pub fill: NoneOr<Color>,
        /// The shading's pattern.
        pub pattern: super::command::ShadingPattern,
        /// The pattern's color.
        pub pattern_color: AutoOr<Color>,
        /// Page Border: the setting.
        pub page_setting: BorderSetting,
        /// Page Border: the top edge, or none.
        pub page_top: Option<super::command::Border>,
        /// Page Border: the bottom edge, or none.
        pub page_bottom: Option<super::command::Border>,
        /// Page Border: the left edge, or none.
        pub page_left: Option<super::command::Border>,
        /// Page Border: the right edge, or none.
        pub page_right: Option<super::command::Border>,
        /// Page Border: what the borders are measured from.
        pub page_offset_from: super::command::PageBorderOffset,
        /// Page Border: the pages they show on.
        pub page_display: super::command::PageBorderDisplay,
        /// Page Border: the sections they apply to.
        pub page_apply_to: super::command::PageBorderScope,
    }
}

dialog! {
    /// The Frame dialog, on the frame the selection is in: its properties (`setFrame`), and Move with text, which is
    /// the vertical position relative to the paragraph. `remove` is its Remove Frame button: applying runs
    /// `removeFrame`.
    #[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase", default, deny_unknown_fields)]
    pub struct FrameDialog / FramePatch {
        /// Text wrapping.
        pub wrap: super::command::FrameWrap,
        /// How the width is set.
        pub width_rule: super::command::FrameWidthRule,
        /// The width, when exact.
        pub width: Points,
        /// How the height is set.
        pub height_rule: super::command::FrameHeightRule,
        /// The height, unless automatic.
        pub height: Points,
        /// The horizontal alignment, or none for `horizontal`.
        pub horizontal_align: Option<super::command::FrameHorizontalAlign>,
        /// The horizontal position.
        pub horizontal: Points,
        /// What it is relative to.
        pub horizontal_relative: super::command::FrameHorizontalRelative,
        /// The distance from text, left and right.
        pub horizontal_distance: Points,
        /// The vertical alignment, or none for `vertical`.
        pub vertical_align: Option<super::command::FrameVerticalAlign>,
        /// The vertical position.
        pub vertical: Points,
        /// What it is relative to.
        pub vertical_relative: super::command::FrameVerticalRelative,
        /// The distance from text, above and below.
        pub vertical_distance: Points,
        /// Move with text: the vertical position is relative to the paragraph.
        pub move_with_text: bool,
        /// Lock anchor.
        pub lock_anchor: bool,
        /// Remove Frame.
        pub remove: bool,
    }
}

impl FrameDialog {
    /// The dialog on a frame's properties.
    pub fn of(p: super::command::FrameProperties) -> FrameDialog {
        FrameDialog {
            wrap: p.wrap,
            width_rule: p.width_rule,
            width: p.width,
            height_rule: p.height_rule,
            height: p.height,
            horizontal_align: p.horizontal_align,
            horizontal: p.horizontal,
            horizontal_relative: p.horizontal_relative,
            horizontal_distance: p.horizontal_distance,
            vertical_align: p.vertical_align,
            vertical: p.vertical,
            vertical_relative: p.vertical_relative,
            vertical_distance: p.vertical_distance,
            move_with_text: p.vertical_relative == super::command::FrameVerticalRelative::Paragraph,
            lock_anchor: p.lock_anchor,
            remove: false,
        }
    }

    /// The frame's properties the dialog sets.
    pub fn properties(&self) -> super::command::FrameProperties {
        super::command::FrameProperties {
            wrap: self.wrap,
            width_rule: self.width_rule,
            width: self.width,
            height_rule: self.height_rule,
            height: self.height,
            horizontal_align: self.horizontal_align,
            horizontal: self.horizontal,
            horizontal_relative: self.horizontal_relative,
            horizontal_distance: self.horizontal_distance,
            vertical_align: self.vertical_align,
            vertical: self.vertical,
            vertical_relative: self.vertical_relative,
            vertical_distance: self.vertical_distance,
            lock_anchor: self.lock_anchor,
            allow_overlap: None,
        }
    }
}

dialog! {
    /// Define New Bullet: the bullet of the selection's list level (or a new list's): its character in its font, or
    /// a picture.
    #[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase", default, deny_unknown_fields)]
    pub struct DefineBulletDialog / DefineBulletPatch {
        /// The bullet's character.
        pub text: String,
        /// Its font.
        pub font: Option<String>,
        /// A picture bullet's image (PNG, JPEG or GIF), instead of the character.
        pub picture: Option<super::value::Bytes>,
        /// Its alignment.
        pub alignment: super::command::LevelAlignment,
    }
}

dialog! {
    /// Define New Number Format: the number of the selection's list level (or a new list's).
    #[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase", default, deny_unknown_fields)]
    pub struct DefineNumberFormatDialog / DefineNumberFormatPatch {
        /// The number style.
        pub format: super::command::NumberFormat,
        /// The number format, with `%1` for the number.
        pub text: String,
        /// The number's font.
        pub font: Option<String>,
        /// Its alignment.
        pub alignment: super::command::LevelAlignment,
    }
}

dialog! {
    /// Define New Multilevel List: the nine levels of the selection's list (or a new list's), the level shown, what
    /// the levels apply to, and the ListNum field name.
    #[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase", default, deny_unknown_fields)]
    pub struct DefineMultilevelDialog / DefineMultilevelPatch {
        /// The levels.
        pub levels: Vec<super::command::ListLevel>,
        /// The level shown (0 to 8).
        pub level: u8,
        /// What the levels apply to.
        pub apply_to: super::command::ListApplyTo,
        /// The level the gallery shows (1 to 9).
        pub gallery_level: u8,
        /// The ListNum field list name.
        pub list_name: String,
    }
}

dialog! {
    /// Set Numbering Value: start a new list at a value, or continue the previous list (skipping to a value).
    #[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase", default, deny_unknown_fields)]
    pub struct SetNumberingValueDialog / SetNumberingValuePatch {
        /// Start a new list, or continue.
        pub mode: super::command::NumberingMode,
        /// The value: the paragraph's number when it opens.
        pub value: u32,
        /// Continuing: skip numbers to `value`.
        pub advance: bool,
    }
}

dialog! {
    /// Adjust List Indents: the selection's list level's indents and what follows its number.
    #[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase", default, deny_unknown_fields)]
    pub struct AdjustListIndentsDialog / AdjustListIndentsPatch {
        /// Where the number is.
        pub number_position: Points,
        /// Where the text is.
        pub text_indent: Points,
        /// What follows the number.
        pub follow: super::command::FollowNumber,
        /// The tab stop after the number; none uses the text indent.
        pub tab: Option<Points>,
    }
}

dialog! {
    /// Bullets and Numbering, from the style dialog's Format menu: the list the style's paragraphs are in, as a
    /// gallery entry (`listGalleries`), or none.
    #[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase", default, deny_unknown_fields)]
    pub struct BulletsNumberingDialog / BulletsNumberingPatch {
        /// The gallery shown.
        pub gallery: ListGalleryKind,
        /// The entry, or none for no list.
        pub entry: Option<ListEntryId>,
    }
}

/// What Printed Watermark puts behind the pages' text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WatermarkKind {
    /// No watermark.
    #[default]
    None,
    /// A picture watermark.
    Picture,
    /// A text watermark.
    Text,
}

dialog! {
    /// Printed Watermark (`setWatermark`, `removeWatermark`): the pages' watermark, or the dialog's defaults when there
    /// is none.
    #[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase", default, deny_unknown_fields)]
    pub struct PrintedWatermarkDialog / PrintedWatermarkPatch {
        /// No watermark, a picture or text.
        pub watermark: WatermarkKind,
        /// The picture chosen; empty keeps the watermark's own.
        pub picture: Bytes,
        /// The picture's scale, percent of its own size; `null` for Auto.
        pub scale: Option<f64>,
        /// The picture is washed out.
        pub washout: bool,
        /// The text.
        pub text: String,
        /// The text's font; empty for the theme's body font.
        pub font: String,
        /// The text's language (a BCP 47 tag), the document's by default.
        pub language: String,
        /// The text's size, 1 to 1584 pt; `null` for Auto.
        pub size: Option<Points>,
        /// The text's colour.
        pub color: Color,
        /// How transparent the text is, percent (0 opaque … 100; Semitransparent is 50).
        pub transparency: f64,
        /// Diagonal or horizontal.
        pub layout: super::command::WatermarkLayout,
        /// Bold.
        pub bold: bool,
        /// Italic.
        pub italic: bool,
    }
}

/// Fill Effects' tabs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FillEffectsTab {
    /// A gradient.
    #[default]
    Gradient,
    /// A picture tiled.
    Texture,
    /// A two-colour pattern.
    Pattern,
    /// A picture stretched over the page.
    Picture,
}

/// A gradient's colours in Fill Effects.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GradientColors {
    /// One colour, darkened or lightened.
    #[default]
    One,
    /// Two colours.
    Two,
    /// A preset's colours.
    Preset,
}

/// How a gradient runs across the page in Fill Effects.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ShadingStyle {
    /// Bands across, changing down the page.
    #[default]
    Horizontal,
    /// Bands down, changing across.
    Vertical,
    /// Bands rising to the right.
    DiagonalUp,
    /// Bands falling to the right.
    DiagonalDown,
    /// Out from a corner.
    FromCorner,
    /// Out from the centre.
    FromCenter,
}

dialog! {
    /// Fill Effects, for the page colour (`setPageColor`): a gradient, texture, pattern or picture.
    #[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase", default, deny_unknown_fields)]
    pub struct FillEffectsDialog / FillEffectsPatch {
        /// The tab: which kind of fill.
        pub tab: FillEffectsTab,
        /// One colour, two or a preset.
        pub colors: GradientColors,
        /// The first colour.
        pub color1: super::drawing::ColourValue,
        /// The second colour (two colours).
        pub color2: super::drawing::ColourValue,
        /// One colour: how light the second end is, 0 (dark) to 1 (light).
        pub lightness: f64,
        /// The preset (preset colours), by name (`earlySunset`).
        pub preset: String,
        /// How the gradient runs.
        pub shading: ShadingStyle,
        /// The shading's variant, 1 to 4 (1 or 2 from the centre).
        pub variant: u32,
        /// The texture's picture, tiled; empty keeps the page's.
        pub texture: Bytes,
        /// The pattern, by its ECMA-376 name (`dkDnDiag`).
        pub pattern: String,
        /// The pattern's colour.
        pub foreground: super::drawing::ColourValue,
        /// The colour behind the pattern.
        pub background: super::drawing::ColourValue,
        /// The picture, stretched over the page; empty keeps the page's.
        pub picture: Bytes,
    }
}

/// A run of characters a font has: code points `first` to `last`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CharRange {
    /// The first code point.
    pub first: u32,
    /// The last code point.
    pub last: u32,
}

dialog! {
    /// Banding: a table style's rows and columns in each band.
    #[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase", default, deny_unknown_fields)]
    pub struct BandingDialog / BandingPatch {
        /// The rows in each band.
        pub row_band: u32,
        /// The columns in each band.
        pub column_band: u32,
    }
}

/// A list gallery: bullets, numbering, or multilevel lists.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ListGalleryKind {
    /// The bullet library.
    #[default]
    Bullet,
    /// The numbering library.
    Numbering,
    /// The multilevel list library.
    Multilevel,
}

/// A section of a list gallery.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ListGallerySectionKind {
    /// The built-in library.
    #[default]
    Library,
    /// The lists applied or defined in this session.
    RecentlyUsed,
    /// The lists in the document.
    Document,
    /// The document's list styles (the multilevel gallery).
    ListStyles,
}

/// A list gallery entry, by its id (`applyListEntry`, `listEntryPreview`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ListEntryId(pub String);

/// A list gallery entry: its id, a short label (its first levels' numbers or its bullet), and its levels.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListEntry {
    /// The entry.
    pub id: ListEntryId,
    /// A short label, such as `1. a. i.` or a bullet's character.
    pub label: String,
    /// Its levels.
    pub levels: Vec<super::command::ListLevel>,
}

/// A section of a list gallery and its entries.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListGallerySection {
    /// Which section.
    pub kind: ListGallerySectionKind,
    /// Its entries.
    pub entries: Vec<ListEntry>,
}

/// The size of a preview picture: CSS px (or device-independent pixels), and the device pixels per one.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreviewSize {
    /// The width.
    pub width: f64,
    /// The height.
    pub height: f64,
    /// Device pixels per px (the device pixel ratio). Default: 1.
    #[serde(default = "one")]
    pub scale: f64,
}

fn one() -> f64 {
    1.0
}

/// A picture the editor drew: a PNG of `width` x `height` device pixels.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreviewPicture {
    /// The PNG.
    pub png: super::value::Bytes,
    /// Its width, device pixels.
    pub width: u32,
    /// Its height, device pixels.
    pub height: u32,
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
    /// A text effect's settings (a fill, an outline, a shadow, a glow or a reflection, as `setTextEffects` takes
    /// them), or `'none'`.
    Effect,
    /// A picture: its image's bytes.
    Picture,
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
        /// Emboss.
        pub emboss: Tri<bool>,
        /// Engrave.
        pub engrave: Tri<bool>,
        /// Outline.
        pub outline: Tri<bool>,
        /// Shadow.
        pub shadow: Tri<bool>,
        /// The character width scale, percent.
        pub scale: Tri<f64>,
        /// Kerning from this size up; 0 for none.
        pub kerning: Tri<Points>,
        /// The OpenType ligatures.
        pub ligatures: Tri<crate::api::command::Ligatures>,
        /// The number spacing.
        pub number_spacing: Tri<crate::api::command::NumberSpacing>,
        /// The number forms.
        pub number_form: Tri<crate::api::command::NumberForm>,
        /// The stylistic set, 1 to 20; 0 for the default.
        pub stylistic_set: Tri<u8>,
        /// Contextual alternates.
        pub contextual_alternates: Tri<bool>,
        /// The text fill (Text Effects), or none of the text's own (the font colour).
        pub text_fill: Tri<NoneOr<Box<crate::api::drawing::FillValue>>>,
        /// The text outline, or none of the text's own.
        pub text_outline: Tri<NoneOr<Box<crate::api::drawing::TextOutline>>>,
        /// The shadow, or none of the text's own.
        pub text_shadow: Tri<NoneOr<Box<crate::api::drawing::TextShadow>>>,
        /// The reflection, or none of the text's own.
        pub text_reflection: Tri<NoneOr<Box<crate::api::drawing::TextReflection>>>,
        /// The glow, or none of the text's own.
        pub text_glow: Tri<NoneOr<Box<crate::api::drawing::Glow>>>,
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
        /// Collapsed by default (with an outline level).
        pub collapsed: Tri<bool>,
        /// Mirror indents: inside and outside instead of left and right.
        pub mirror_indents: Tri<bool>,
        /// Don't add space between paragraphs of the same style.
        pub contextual_spacing: Tri<bool>,
        /// Suppress line numbers.
        pub suppress_line_numbers: Tri<bool>,
        /// Don't hyphenate.
        pub dont_hyphenate: Tri<bool>,
    }
}

dialog! {
    /// The Drop Cap dialog, on the selection's paragraph (`setDropCap`).
    #[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase", default, deny_unknown_fields)]
    pub struct DropCapDialog / DropCapPatch {
        /// Where the letter goes.
        pub position: super::command::DropCapPosition,
        /// The letter's font; empty for the text's.
        pub font: String,
        /// How many lines it drops, 1 to 10.
        pub lines: u32,
        /// How far the text keeps from it.
        pub distance: Points,
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
    /// From the selection on: a new-page section break goes there, and the section after it takes the setup.
    ThisPointForward,
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
        /// Multiple pages: normal, mirror margins or book fold.
        pub multiple_pages: crate::api::command::MultiplePagesMode,
        /// The gutter is at the top.
        pub gutter_at_top: bool,
        /// Where the section starts.
        pub section_start: crate::api::command::SectionBreakKind,
        /// The text's vertical alignment on the page.
        pub vertical_alignment: crate::api::command::PageVerticalAlignment,
        /// Line numbers, or none.
        pub line_numbers: Option<crate::api::command::LineNumbers>,
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
        /// The selected cells' preferred width.
        pub cell_width: Tri<TableWidth>,
        /// The selected cells' vertical alignment.
        pub cell_alignment: Tri<CellAlignment>,
        /// The table's default cell margins.
        pub cell_margins: CellMarginValues,
        /// Allow spacing between cells.
        pub allow_spacing: bool,
        /// The space between cells, when allowed.
        pub cell_spacing: Points,
        /// The selected cells' text wraps.
        pub wrap_text: Tri<bool>,
        /// The selected cells' text is squeezed to fit.
        pub fit_text: Tri<bool>,
        /// The alternative text's title.
        pub title: String,
        /// The alternative text's description.
        pub description: String,
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
        /// A floating object's width as a share of a box on the page; none for an absolute width.
        pub relative_width: Option<crate::api::command::RelativeSize>,
        /// Its height as a share of a box on the page; none for an absolute height.
        pub relative_height: Option<crate::api::command::RelativeSize>,
        /// A picture's crop (Crop from), by fractions of its image from each edge; none for an object that isn't one.
        pub crop: Option<crate::api::command::PictureCrop>,
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
        /// The address: a URL or a file, empty for a place in this document.
        pub address: String,
        /// The bookmark the link goes to (`_top`: the document's start), in this document or, with an address, in
        /// that one. Empty for none.
        pub bookmark: String,
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
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FindQuery {
    /// The text, with Word's `^` codes (`^p` a paragraph mark, `^t` a tab, `^?` any character, `^#` any digit, `^^`
    /// a caret), or a regular expression with `regex`. At most 255 characters.
    pub text: String,
    /// Match case (off when unset).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub match_case: Option<bool>,
    /// Whole words only (off when unset).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub whole_words: Option<bool>,
    /// `text` is a regular expression, searched paragraph by paragraph (off when unset). Whole words, prefix, suffix
    /// and the ignore options don't apply to it; match case does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub regex: Option<bool>,
    /// A match starts at a word's start (off when unset).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub match_prefix: Option<bool>,
    /// A match ends at a word's end (off when unset).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub match_suffix: Option<bool>,
    /// Punctuation is passed over in the text and ignored in `text` (off when unset).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ignore_punctuation: Option<bool>,
    /// White space is passed over in the text and ignored in `text` (off when unset).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ignore_white_space: Option<bool>,
    /// `find` wraps round past the document's end (on when unset); off, it stops there with `found` false.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wrap: Option<bool>,
    /// Only within the selection (its ranges), with no wrapping (off when unset).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub in_selection: Option<bool>,
    /// The formatting a match must have; with an empty `text`, each stretch that has it matches.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<FindFormat>,
}

macro_rules! find_fields {
    ($(#[$m:meta])* pub struct $name:ident => $dialog:ident { $($(#[$fm:meta])* pub $f:ident: $t:ty,)* }) => {
        $(#[$m])*
        #[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        pub struct $name {
            $($(#[$fm])* #[serde(default, skip_serializing_if = "Option::is_none")] pub $f: Option<$t>,)*
        }

        impl $name {
            /// The values given, as the engine's tri-state values: each given one uniform, the rest unavailable (any).
            pub(crate) fn to_tri(&self) -> $dialog {
                $dialog { $($f: self.$f.clone().map_or(Tri::Unavailable, Tri::Uniform),)* }
            }
        }
    };
}

find_fields! {
    /// The character formatting a match must have, or a replacement applies; each value left out is any.
    pub struct FindFont => FontDialog {
        /// The font name (matched ignoring ASCII case).
        pub font: String,
        /// Regular, italic, bold or both: exactly that combination.
        pub style: FontStyle,
        /// The size.
        pub size: Points,
        /// The text colour.
        pub color: AutoOr<Color>,
        /// The underline.
        pub underline: UnderlineStyle,
        /// The underline's colour.
        pub underline_color: AutoOr<Color>,
        /// Strikethrough.
        pub strikethrough: bool,
        /// Double strikethrough.
        pub double_strikethrough: bool,
        /// Superscript.
        pub superscript: bool,
        /// Subscript.
        pub subscript: bool,
        /// Small caps.
        pub small_caps: bool,
        /// All caps.
        pub all_caps: bool,
        /// Hidden text.
        pub hidden: bool,
        /// Extra space between characters; negative condenses.
        pub character_spacing: Points,
        /// The baseline offset; positive raises.
        pub position: Points,
        /// Emboss.
        pub emboss: bool,
        /// Engrave.
        pub engrave: bool,
        /// Outline.
        pub outline: bool,
        /// Shadow.
        pub shadow: bool,
        /// The character width scale, percent.
        pub scale: f64,
        /// Kerning from this size up; 0 for none.
        pub kerning: Points,
        /// The OpenType ligatures.
        pub ligatures: crate::api::command::Ligatures,
        /// The number spacing.
        pub number_spacing: crate::api::command::NumberSpacing,
        /// The number forms.
        pub number_form: crate::api::command::NumberForm,
        /// The stylistic set, 1 to 20; 0 for the default.
        pub stylistic_set: u8,
        /// Contextual alternates.
        pub contextual_alternates: bool,
        /// The text fill, or none of the text's own.
        pub text_fill: NoneOr<Box<crate::api::drawing::FillValue>>,
        /// The text outline, or none of the text's own.
        pub text_outline: NoneOr<Box<crate::api::drawing::TextOutline>>,
        /// The text shadow, or none of the text's own.
        pub text_shadow: NoneOr<Box<crate::api::drawing::TextShadow>>,
        /// The reflection, or none of the text's own.
        pub text_reflection: NoneOr<Box<crate::api::drawing::TextReflection>>,
        /// The glow, or none of the text's own.
        pub text_glow: NoneOr<Box<crate::api::drawing::Glow>>,
    }
}

find_fields! {
    /// The paragraph formatting a match's paragraphs must have, or a replacement applies; each value left out is any.
    pub struct FindParagraph => ParagraphDialog {
        /// The alignment.
        pub alignment: Alignment,
        /// The outline level.
        pub outline_level: OutlineLevel,
        /// The left indent.
        pub left: Points,
        /// The right indent.
        pub right: Points,
        /// The special indent.
        pub special: SpecialIndent,
        /// The special indent's size.
        pub by: Points,
        /// The space before.
        pub before: AutoOr<Points>,
        /// The space after.
        pub after: AutoOr<Points>,
        /// The line spacing.
        pub line_spacing: LineSpacing,
        /// Keep with next.
        pub keep_with_next: bool,
        /// Keep lines together.
        pub keep_lines_together: bool,
        /// Page break before.
        pub page_break_before: bool,
        /// Widow and orphan control.
        pub widow_control: bool,
        /// Collapsed by default.
        pub collapsed: bool,
        /// Mirror indents.
        pub mirror_indents: bool,
        /// Don't add space between paragraphs of the same style.
        pub contextual_spacing: bool,
        /// Suppress line numbers.
        pub suppress_line_numbers: bool,
        /// Don't hyphenate.
        pub dont_hyphenate: bool,
    }
}

/// Formatting a search looks for (each value given must be the text's) and a replacement applies; each value left
/// out is any.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FindFormat {
    /// Character formatting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font: Option<FindFont>,
    /// Paragraph formatting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paragraph: Option<FindParagraph>,
    /// A paragraph style the paragraphs have, or a character style the text has (by the style's kind).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<StyleId>,
    /// Highlighted in any colour, or not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub highlight: Option<bool>,
    /// The colour a replacement highlights in (yellow when unset).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub highlight_color: Option<crate::api::value::HighlightColor>,
    /// The language, a BCP 47 tag, matched ignoring case.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// Spelling and grammar not checked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub no_proof: Option<bool>,
    /// Tab stops the paragraphs have (each of these, among others); a replacement adds them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tabs: Vec<crate::api::command::AddTabStop>,
    /// The frame the paragraphs are in; a replacement frames them so.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame: Option<crate::api::command::FrameProperties>,
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
    /// The search stopped early: matches past its budget of work were given up.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub truncated: bool,
}

/// Every match, for a results list.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FindMatches {
    /// The matches, in document order.
    pub matches: Vec<FindMatch>,
    /// The search stopped early: matches past its budget of work were given up.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub truncated: bool,
}

/// How many matches Replace All replaced or Highlight All highlighted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FindCount {
    /// How many.
    pub count: u32,
    /// The search stopped early: matches past its budget of work were given up.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub truncated: bool,
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
    /// The heading it is under (the nearest listed one at or before it in the body); absent before the first heading
    /// or outside the body.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heading: Option<HeadingId>,
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

/// The document's properties and variables, for File > Info and Advanced Properties.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DocumentProperties {
    /// The title.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// The subject.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    /// The author.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// The keywords.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keywords: Option<String>,
    /// The comments.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comments: Option<String>,
    /// The category.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    /// The status.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// Who saved it last.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_modified_by: Option<String>,
    /// How many times it was saved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    /// When it was made (ISO 8601).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created: Option<String>,
    /// When it was last saved (ISO 8601).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modified: Option<String>,
    /// When it was last printed (ISO 8601).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_printed: Option<String>,
    /// The company.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub company: Option<String>,
    /// The manager.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manager: Option<String>,
    /// The base relative hyperlinks resolve against.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hyperlink_base: Option<String>,
    /// The custom properties.
    pub custom: Vec<super::command::CustomProperty>,
    /// The document variables (`w:docVars`).
    pub variables: Vec<super::command::DocumentVariable>,
}

/// A bookmark, for the Bookmark dialog.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BookmarkInfo {
    /// Its name.
    pub name: String,
    /// Hidden: Word's own (`_Toc`, `_Ref`), named with a leading `_`.
    pub hidden: bool,
    /// Its text holds nothing: it marks a place.
    pub empty: bool,
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
    /// A list (numbering) style.
    List,
}

/// Whether a style shows in style lists.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StyleVisibility {
    /// Shown.
    #[default]
    Show,
    /// Hidden until a paragraph or run uses it (`w:semiHidden` with `w:unhideWhenUsed`).
    HideUntilUsed,
    /// Hidden (`w:semiHidden`).
    Hide,
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
    /// Its other names (`w:name` holds them after the first, comma-separated).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    /// The linked character or paragraph style.
    pub link: Option<StyleId>,
    /// Whether it shows in style lists.
    pub visibility: StyleVisibility,
    /// Restricted: unavailable while the document's formatting is protected.
    pub locked: bool,
    /// How many times the main document uses it: paragraphs for a paragraph style, stretches of runs for a character
    /// style, 0 for table and list styles (Select All Instances' count).
    #[serde(default)]
    pub uses: u32,
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
    /// Only the elements in this sub-template's body.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sub_template: Option<String>,
}

/// A field or content control.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
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
    /// The sub-template whose body it is in, when `story` is `subTemplate`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sub_template: Option<String>,
    /// The element it is in.
    pub parent: Option<ElementId>,
    /// Locked against editing.
    pub locked: bool,
    /// A field shows its code rather than its result.
    #[serde(default)]
    pub showing_code: bool,
    /// A field's result is out of date: the next field update refreshes it.
    #[serde(default)]
    pub dirty: bool,
    /// A date control's date (`YYYY-MM-DDTHH:MM:SSZ`); absent while it shows its placeholder or no date.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    /// A sub-template reference field's sub-template.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference: Option<SubTemplateReference>,
    /// Whether a content control shows its placeholder (`w:showingPlcHdr`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub showing_placeholder: Option<bool>,
    /// A picture control's picture size.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<crate::api::value::Size>,
}

/// A sub-template: a building block in a category the rules declare.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SubTemplateInfo {
    /// Its name.
    pub name: String,
    /// Its building block category.
    pub category: String,
    /// The reference fields naming it in the template's body, headers, footers and notes.
    pub references: u32,
    /// The reference fields naming it in other sub-templates' bodies.
    pub nested_references: u32,
    /// The fields and controls in its body.
    pub elements: Vec<ElementInfo>,
}

/// What a sub-template reference field names and whether its body shows.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SubTemplateReference {
    /// The sub-template's name, as the field gives it.
    pub name: String,
    /// Whether it names a sub-template whose body can show in its place.
    pub resolution: ReferenceResolution,
    /// The sub-template's name when only a name in another case matches (`nearMatch`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub near_match: Option<String>,
    /// Its body shows in its place.
    pub expanded: bool,
}

/// Whether a reference's sub-template resolves, or why its body can't show.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReferenceResolution {
    /// A sub-template has the name.
    #[default]
    Resolved,
    /// No sub-template has the name.
    Missing,
    /// Only a name in another case matches.
    NearMatch,
    /// The sub-template is already being shown around it.
    Cycle,
    /// More than 16 sub-templates deep.
    TooDeep,
    /// The field shares its paragraph with other content.
    NotAlone,
    /// The field is in a header, footer, note or text box.
    NotInBody,
}

/// An element with its settings and children.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
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
    /// The sub-template whose body it is in, when `story` is `subTemplate`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sub_template: Option<String>,
    /// The element it is in.
    pub parent: Option<ElementId>,
    /// Locked against editing.
    pub locked: bool,
    /// A field shows its code rather than its result.
    #[serde(default)]
    pub showing_code: bool,
    /// A field's result is out of date: the next field update refreshes it.
    #[serde(default)]
    pub dirty: bool,
    /// A date control's date (`YYYY-MM-DDTHH:MM:SSZ`); absent while it shows its placeholder or no date.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
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
        /// Its hidden paragraph marks: where each paragraph joined into it by one ends, and its own mark when that is
        /// hidden (it runs into the next paragraph).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        hidden_marks: Vec<HiddenMark>,
    },
    /// A table.
    Table {
        /// Its rows.
        rows: Vec<BlockRow>,
    },
}

/// A hidden paragraph mark in a paragraph: a style separator runs a heading into the paragraph after it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HiddenMark {
    /// Where it is, in characters from the paragraph's start (the paragraph's length for its own mark).
    pub at: u32,
    /// It is a style separator (Ctrl+Alt+Enter), not text formatted hidden.
    pub style_separator: bool,
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

/// A picture control the user or the application activated.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PictureControlActivation {
    /// The picture control.
    pub element: ElementId,
}

// ---------------- accessibility, shortcuts, gestures ----------------

/// An accessibility tree node's identity. It stays the same across edits while the thing it names exists: a
/// paragraph, table, note, comment or story keeps its id when other content changes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AccessibleId(pub u64);

/// An accessibility tree node's role.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AccessibleRole {
    /// The document.
    #[default]
    Document,
    /// A paragraph.
    Paragraph,
    /// A heading (its `level`, 1 to 9).
    Heading,
    /// A list.
    List,
    /// A list item (its `level` and `position`, and a numbered item's `label`).
    ListItem,
    /// A table (its nesting `level`).
    Table,
    /// A table row.
    Row,
    /// A table cell (its `cell` place).
    Cell,
    /// A cell of a header row.
    ColumnHeader,
    /// A cell that heads its row.
    RowHeader,
    /// A picture or shape, named by its alternative text.
    Image,
    /// A hyperlink.
    Link,
    /// A text box or a shape's text.
    TextBox,
    /// A footnote or endnote.
    Note,
    /// A footnote or endnote reference (its `target` is the note).
    NoteRef,
    /// A comment (its `author`), with its replies as children.
    Comment,
    /// A header.
    Header,
    /// A footer.
    Footer,
    /// A named group after the body: headers and footers, footnotes, endnotes, comments.
    Region,
    /// A run of text with the same marks, author, language and comment.
    Text,
}

/// What a text node is marked with.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AccessibleMark {
    /// A tracked insertion.
    Insertion,
    /// A tracked deletion.
    Deletion,
    /// A misspelling.
    Misspelling,
}

/// Where a cell is in its table's grid: its row and column (from 1) and how many it spans.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CellPlace {
    /// Its row, from 1.
    pub row: u32,
    /// Its first grid column, from 1.
    pub col: u32,
    /// How many rows it spans.
    pub row_span: u32,
    /// How many grid columns it spans.
    pub col_span: u32,
}

/// Where a list item is in its list: its index (from 1) among the items at its level, and how many there are.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListPosition {
    /// Its index, from 1.
    pub index: u32,
    /// The number of items.
    pub count: u32,
}

/// The document's accessibility tree.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AccessibleNode {
    /// Its identity.
    pub id: AccessibleId,
    /// Its role.
    pub role: AccessibleRole,
    /// Its accessible name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Its text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// A heading's level, a list item's nesting level, or a table's nesting level, from 1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<u32>,
    /// A numbered list item's number as shown (`3.`, `a)`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// A list item's position in its list.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<ListPosition>,
    /// A cell's place in its table.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cell: Option<CellPlace>,
    /// Review and spelling marks on a text node.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub marks: Vec<AccessibleMark>,
    /// Whose marks or comment it is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// Its language (BCP 47), where it differs from its parent's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lang: Option<String>,
    /// The comment the text is in the range of.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<AccessibleId>,
    /// The note a note reference points to; for a sub-template body shown in place, its reference's paragraph.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<AccessibleId>,
    /// Its children.
    pub children: Vec<AccessibleNode>,
}

/// The text around the caret, for the element that has the keyboard focus while the editor does: the caret's
/// paragraph, the one before it and the ones after it (up to about 2,000 characters), all in the caret's container (the
/// body, a cell, a note, a header or footer, a text box), each split into its laid-out lines; and the selection in it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CaretWindow {
    /// The paragraphs, in order.
    pub paragraphs: Vec<WindowParagraph>,
    /// The selection's anchor, at the window's edge when it is outside.
    pub anchor: WindowPoint,
    /// The selection's focus: the caret.
    pub focus: WindowPoint,
    /// The selection is all of its story.
    pub all: bool,
    /// The caret's language (BCP 47).
    pub lang: String,
    /// The caret is where text can't go in (a locked control, a read-only document).
    pub read_only: bool,
}

/// A paragraph of a caret window.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WindowParagraph {
    /// Its id: the same as its node's in the accessibility tree.
    pub id: AccessibleId,
    /// `paragraph`, `heading` or `listItem`.
    pub role: AccessibleRole,
    /// A heading's level.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<u32>,
    /// A numbered list item's number as shown; it isn't part of the text or its offsets.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Its laid-out lines; a line break ends one too.
    pub lines: Vec<WindowLine>,
}

/// A line of a caret window's paragraph.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WindowLine {
    /// Its text, split where marks, author or language change.
    pub segments: Vec<WindowSegment>,
}

/// Text of a caret window's line with the same marks, author and language.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WindowSegment {
    /// Its text.
    pub text: String,
    /// Its review and spelling marks.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub marks: Vec<AccessibleMark>,
    /// Whose marks they are.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// Its language, where it differs from the document's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lang: Option<String>,
}

/// A place in a caret window: a paragraph (its index in the window) and an offset in its text (characters, not
/// counting a list item's label). A character is a Unicode scalar value: a host whose strings are UTF-16 counts a
/// surrogate pair as one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WindowPoint {
    /// The paragraph's index in the window.
    pub paragraph: u32,
    /// The offset in its text, in Unicode scalar values.
    pub offset: u32,
}

/// A change to the accessibility tree.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum A11yChange {
    /// The node with this node's id, and its subtree, are replaced.
    Replace {
        /// The new node.
        node: Box<AccessibleNode>,
    },
    /// A node's children are now these, in this order. Ids it didn't have before are nodes sent by a `Replace` in
    /// the same update.
    Children {
        /// The node.
        parent: AccessibleId,
        /// Its children's ids.
        ids: Vec<AccessibleId>,
    },
}

/// The accessibility tree, or what changed in it since the last update.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub enum A11yUpdate {
    /// The whole tree.
    Full(Box<AccessibleNode>),
    /// The changes since the last update, applied in order.
    Patch(Vec<A11yChange>),
}

/// What an announcement is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AnnouncementKind {
    /// Entering, leaving or moving through a table.
    Table,
    /// Entering or leaving a list.
    List,
    /// The caret moved to another page.
    Page,
    /// An insertion or deletion at the caret.
    Review,
    /// A commented range at the caret, with the comment.
    Comment,
    /// A misspelled word at the caret.
    Spelling,
    /// Formatting turned on or off, or described on request.
    Formatting,
    /// Entering or leaving a header, footer, note or text box.
    Mode,
    /// Read-only content: entered, or an edit refused there.
    ReadOnly,
    /// What a command did (undo, redo, cut, copy, paste).
    Command,
    /// The selection (select all).
    Selection,
}

impl AnnouncementKind {
    /// Every kind.
    pub const ALL: [AnnouncementKind; 11] = [
        AnnouncementKind::Table,
        AnnouncementKind::List,
        AnnouncementKind::Page,
        AnnouncementKind::Review,
        AnnouncementKind::Comment,
        AnnouncementKind::Spelling,
        AnnouncementKind::Formatting,
        AnnouncementKind::Mode,
        AnnouncementKind::ReadOnly,
        AnnouncementKind::Command,
        AnnouncementKind::Selection,
    ];
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
    /// A Ctrl+click on text: selects the sentence, or, with text selected, adds the sentence to the selection, or takes it out when it is selected.
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
    /// A click on an unfilled picture control in editing and read-only modes: activates it.
    PictureControlClick,
    /// A double-click on a picture control in editing and read-only modes: activates it.
    PictureControlDoubleClick,
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
    /// A double-click on a picture, shape or text box.
    ObjectDoubleClick,
    /// Dragging an object's sizing handle: resizes it.
    ObjectResizeDrag,
    /// Dragging a shape's adjustment or rotation handle.
    ShapeHandleDrag,
    /// Dragging a table's move handle: moves the table.
    TableMoveDrag,
    /// A click in a cell's, row's or column's selection zone: selects it.
    TableZoneClick,
    /// A click on a table's insert mark: inserts a row or column there.
    TableInsertClick,
    /// A double-click on a note reference or a note's number: jumps between them.
    NoteDoubleClick,
    /// A double-click in blank space with Click and Type on: the caret there.
    ClickAndType,
    /// A double-click between pages, or on their meeting line: hides or shows the white space.
    PageGapDoubleClick,
    /// A click on an enabled drop-down form field where forms are filled in: selects it, and by default emits the
    /// gesture so the host opens its list (`fillFormField` picks an entry).
    FormFieldClick,
    /// A double-click on a legacy form field in editing mode outside forms protection: selects it, and by default
    /// emits the gesture so the host opens its options (`setFormField` changes them).
    FormFieldDoubleClick,
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
    /// Between pages (or their meeting line while white space is hidden).
    PageGap,
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

/// What typing corrects (AutoCorrect): each rule on or off, and the texts never corrected again.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutoCorrectOptions {
    /// THe → The.
    pub two_initial_capitals: bool,
    /// The first letter of a sentence.
    pub sentence_capitals: bool,
    /// The first letter of a table cell.
    pub cell_capitals: bool,
    /// monday → Monday.
    pub day_names: bool,
    /// The replacement list and the symbols ((c) → ©).
    pub replace_text: bool,
    /// Straight quotes to curly ones.
    pub smart_quotes: bool,
    /// 1st with the suffix raised.
    pub ordinals: bool,
    /// 1/2 → ½.
    pub fractions: bool,
    /// Hyphens to en and em dashes.
    pub dashes: bool,
    /// `1.` or `1)` then a space or tab at a paragraph's start makes it a numbered list.
    pub numbered_lists: bool,
    /// `*` or `-` then a space or tab at a paragraph's start makes it a bulleted list.
    pub bulleted_lists: bool,
    /// AutoFormat: three or more `-`, `_`, `=`, `*`, `~` or `#` then Enter make the paragraph a bottom border line.
    pub border_lines: bool,
    /// AutoFormat: an Internet or network path (`www.example.com`, `\\server\share`, `name@example.com`) then a space
    /// or Enter becomes a hyperlink.
    pub internet_paths: bool,
    /// AutoFormat: Tab and Backspace at a paragraph's start set its first-line and left indents (and Tab at a list's
    /// first item moves the whole list).
    pub tab_indents: bool,
    /// Texts Never correct added: never corrected again.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub never: Vec<String>,
}

impl Default for AutoCorrectOptions {
    fn default() -> AutoCorrectOptions {
        AutoCorrectOptions {
            two_initial_capitals: true,
            sentence_capitals: true,
            cell_capitals: true,
            day_names: true,
            replace_text: true,
            smart_quotes: true,
            ordinals: true,
            fractions: true,
            dashes: true,
            numbered_lists: true,
            bulleted_lists: true,
            border_lines: true,
            internet_paths: true,
            tab_indents: true,
            never: Vec::new(),
        }
    }
}

/// `setAutoCorrect`'s change: the rules given switch, the lists given replace the shipped ones.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutoCorrectChange {
    /// THe → The.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub two_initial_capitals: Option<bool>,
    /// The first letter of a sentence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sentence_capitals: Option<bool>,
    /// The first letter of a table cell.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cell_capitals: Option<bool>,
    /// Day names.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub day_names: Option<bool>,
    /// The replacement list and the symbols.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replace_text: Option<bool>,
    /// Smart quotes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub smart_quotes: Option<bool>,
    /// Ordinals.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ordinals: Option<bool>,
    /// Fractions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fractions: Option<bool>,
    /// Dashes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dashes: Option<bool>,
    /// Automatic numbered lists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub numbered_lists: Option<bool>,
    /// Automatic bulleted lists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bulleted_lists: Option<bool>,
    /// Automatic border lines.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub border_lines: Option<bool>,
    /// Internet and network paths to hyperlinks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub internet_paths: Option<bool>,
    /// Indents set with Tab and Backspace.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_indents: Option<bool>,
    /// The replacement list, in place of the shipped one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entries: Option<Vec<AutoCorrectEntry>>,
    /// Abbreviations after which a sentence doesn't start (`e.g.`), in place of the shipped ones.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_letter_exceptions: Option<Vec<String>>,
    /// Words whose two initial capitals stay (`IDs`), in place of the shipped ones.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial_caps_exceptions: Option<Vec<String>>,
    /// The texts never corrected.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub never: Option<Vec<String>>,
}

/// One replacement-list entry.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutoCorrectEntry {
    /// What's typed.
    pub from: String,
    /// What it becomes.
    pub to: String,
}

/// The editor chrome's colour scheme: the desk, rulers, handles, guides and readouts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ChromeTheme {
    /// Light chrome.
    #[default]
    Light,
    /// Dark chrome.
    Dark,
    /// The platform's high contrast colours (the SDKs set this when the platform is in high contrast mode).
    HighContrast(SystemColors),
}

/// The platform's colours in high contrast mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SystemColors {
    /// The window background.
    pub canvas: Color,
    /// Text on it.
    pub canvas_text: Color,
    /// Selected items' background.
    pub highlight: Color,
    /// Text on it.
    pub highlight_text: Color,
    /// Disabled and secondary text.
    pub gray_text: Color,
}

/// The host's colours for the light and the dark chrome scheme, to match its own UI: each scheme given replaces the
/// last one set for it (`{}` puts its own colours back). The high contrast scheme always takes the platform's.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ThemeColors {
    /// For the light scheme.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub light: Option<ChromeColors>,
    /// For the dark scheme.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dark: Option<ChromeColors>,
}

/// Chrome colours in place of a scheme's own; each one left out keeps the scheme's.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChromeColors {
    /// Around the pages and the rulers' strips (a background the host sets wins around the pages).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub desk: Option<Color>,
    /// The rulers' strips, and the readouts' and labels' backgrounds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub surface: Option<Color>,
    /// Text, and the ruler numbers and markers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<Color>,
    /// The rulers' unit ticks, and secondary text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secondary_text: Option<Color>,
    /// The rulers' fine ticks, the object grid, and faint text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub faint_text: Option<Color>,
    /// The rulers' outlines and the margins' hatching.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<Color>,
}

/// How the document's page is drawn on screen. Saving, PDF export and printing always use the document's colours.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DocumentAppearance {
    /// In the document's colours.
    #[default]
    Light,
    /// Dark mode: a dark page, with the document's colours adjusted to it.
    Dark,
    /// Dark mode with the page shown in its own colours (the chrome stays dark).
    DarkWithLightPage,
}

/// How much the editor measures while you drag and hover.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MeasureDetail {
    /// A chip with the value only.
    Quiet,
    /// Dimension lines with the value; the full set while Alt is held.
    #[default]
    Standard,
    /// Everything: changes from the start, table sizes and status lines.
    Full,
}

/// When the dot grid objects snap to shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ObjectGrid {
    /// Never.
    Off,
    /// While an object is dragged.
    #[default]
    WhileDragging,
    /// Always.
    Always,
}

/// When fields are shaded: a grey backdrop behind each field's text on screen, never printed, exported or saved.
/// Legacy form fields are shaded the same way whatever this is, unless the document turns that off.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FieldShading {
    /// Never.
    Never,
    /// Always: every field.
    Always,
    /// The field the caret or selection is in or touches.
    #[default]
    WhenSelected,
}

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

/// A popover the engine asks the application to show by `anchor`, with what it offers and the commands that answer
/// it. Only one is open at a time; `popoverClosed` says the engine dismissed it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum Popover {
    /// The tab type picker: answered with `setRulerTabType`.
    TabType {
        /// The tab type box.
        anchor: ClientRect,
        /// The type a click on the ruler adds now.
        current: TabSelector,
        /// Every type, in order, with its name and what it does.
        choices: Vec<TabTypeChoice>,
    },
    /// One tab stop's options: `addTabStop` at its position changes the alignment or leader, `deleteTabStop` clears it.
    TabStop {
        /// The stop's mark on the ruler.
        anchor: ClientRect,
        /// Its position, from the paragraph's left text edge.
        position: Points,
        /// Its alignment.
        alignment: TabAlignment,
        /// Its leader.
        leader: TabLeader,
    },
    /// The options of the correction just made.
    AutoCorrect {
        /// The corrected text.
        anchor: ClientRect,
        /// What was typed.
        from: String,
        /// What it became.
        to: String,
    },
    /// The options of the AutoFormat change just made: `revertAutoCorrect` undoes it (Undo Hyperlink, Undo Border
    /// Line, Undo Automatic Indent), `neverAutoCorrect` undoes it and turns its option off (Stop Automatically Creating
    /// Hyperlinks or Border Lines, Stop Setting Indent on Tabs and Backspaces); the host's AutoCorrect dialog is
    /// Control AutoFormat Options.
    AutoFormat {
        /// The change's options button, or what it made.
        anchor: ClientRect,
        /// What it made.
        change: AutoFormatKind,
        /// What was typed: the address, or the characters a border line replaced; empty for an indent.
        text: String,
    },
    /// The Paste Options button of the paste just made, shown while nothing has changed since (typing, another edit,
    /// undo or Esc closes it): `repaste` pastes it again another way, `dismissPasteOptions` hides it.
    PasteOptions {
        /// The button: below the end of the paste, at its right.
        anchor: ClientRect,
        /// The end of the paste's caret box.
        end: ClientRect,
        /// The ways it can paste again, in Word's order; `current` among them.
        choices: Vec<crate::api::command::PasteAs>,
        /// The way it was pasted.
        current: crate::api::command::PasteAs,
    },
}

/// An item a cross-reference can point to, as Word's Cross-reference dialog lists it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CrossReferenceTarget {
    /// What the list shows: a numbered item's number and text, a heading's text, a bookmark's name, a note's text, a
    /// whole caption.
    pub text: String,
    /// Its number: a list number, a heading's, a note's as laid out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number: Option<String>,
    /// A heading's level.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<u8>,
    /// A bookmark's name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bookmark: Option<String>,
    /// A note's id, as `notes` lists it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<i64>,
}

/// A table of contents or of figures in the document.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TableOfContentsInfo {
    /// Its place among them, in document order: what `index` names in the commands.
    pub index: u32,
    /// It collects captions: a table of figures.
    pub figures: bool,
    /// What it collects and how its entries read, from its code.
    pub options: crate::api::command::TableOfContentsOptions,
    /// Updating and removing it are refused: its field is locked, or a control around it locks its content.
    pub locked: bool,
    /// The paragraph its field starts in.
    pub first_paragraph: crate::api::value::ParagraphId,
    /// The paragraph its field ends in.
    pub last_paragraph: crate::api::value::ParagraphId,
}

/// The document's tables of contents and of figures (`tablesOfContents`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TablesOfContents {
    /// Each one, in document order.
    pub tables: Vec<TableOfContentsInfo>,
    /// The one the selection is in, by `index`; absent when it is in none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub here: Option<u32>,
}

/// An entry a table of contents would collect (`tocEntries`), for a preview.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TocEntryPreview {
    /// Its level, 1 to 9.
    pub level: u8,
    /// Its text.
    pub text: String,
    /// It shows a page number.
    pub page_number: bool,
}

/// What the clipboard can paste as at the selection (`pasteChoices`): Paste Special's list.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PasteChoices {
    /// The ways it can paste, in Word's order: the table choices or Keep Source Formatting, Use Destination Styles and
    /// Merge Formatting, then Keep Text Only, HTML, text and picture. Empty where nothing can be pasted.
    pub choices: Vec<crate::api::command::PasteAs>,
    /// The one Ctrl+V takes (among `choices`); absent when the clipboard holds nothing to paste.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<crate::api::command::PasteAs>,
}

/// What an AutoFormat change as you type made.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AutoFormatKind {
    /// A hyperlink from an Internet or network path.
    Hyperlink,
    /// A border line from a row of characters.
    BorderLine,
    /// An indent from Tab at a paragraph's start.
    Indent,
}

/// One entry of the tab type picker.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TabTypeChoice {
    /// The type.
    pub value: TabSelector,
    /// Its name (`Decimal`).
    pub name: String,
    /// What it does, in one line (`Numbers line up on the decimal point`).
    pub description: String,
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
    /// The gutter across the page, beside the margin it widens, when it has width.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gutter: Option<Span>,
    /// The margin the page's text starts after: left, or with mirrored margins inside or outside (facing pages swap
    /// them).
    #[serde(default)]
    pub start_margin: super::command::MarginSide,
    /// The margin the page's text ends before.
    #[serde(default)]
    pub end_margin: super::command::MarginSide,
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
    /// The gutter down the page (at the top), when it has height.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gutter: Option<Span>,
    /// The paragraph's indents, when its lines run down or up the page (`lines`): measured along the lines from
    /// `text.start` (down) or `text.end` (up).
    #[serde(default)]
    pub indents: RulerIndents,
    /// The paragraph's tab stops, when its lines run down or up the page, along the lines as `indents` are.
    #[serde(default)]
    pub tabs: Vec<TabStopInfo>,
    /// The document's default tab interval, when the lines run down or up the page.
    #[serde(default)]
    pub default_tab_stop: Points,
}

/// The ruler model for the selection, in points, with no viewport mapping.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RulerModel {
    /// The selected paragraphs differ: the first one's values are shown.
    pub mixed: bool,
    /// The way the caret's lines run. Across the page (`horizontal`), the horizontal ruler shows the indents and tab
    /// stops; down or up the page (vertical text), the vertical ruler shows them, `vertical.text` is where the lines
    /// run, and the horizontal ruler shows the text columns across the page in the order the lines stack.
    #[serde(default)]
    pub lines: LineDirection,
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
    /// A ruler marker, the tab type box, or where a click on the ruler adds a stop.
    Ruler,
    /// A footnote or endnote reference: the note's text.
    Note,
    /// Between pages, or on their meeting line while white space is hidden: a double-click hides (shows) it.
    WhiteSpace,
}

/// A tooltip to show over the document or its rulers.
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
    /// Who made the change or the comment (`revision` and `comment` tips).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<crate::api::review::Person>,
    /// When, as the document gives it; absent when it doesn't (`revision` and `comment` tips).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time: Option<crate::api::record::Timestamp>,
    /// The change's kind (`revision` tips).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub change: Option<crate::api::review::RevisionKind>,
    /// What follows the byline: the change's English description (`Formatted: Bold`) or the comment's text, so a host
    /// can build a localized tip from `author`, `time` and this.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
}

/// What a press on an element does.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PressAction {
    /// Checks or clears a checkbox.
    Toggle,
    /// Follows a hyperlink (with Ctrl or Cmd held, or in a read-only editor).
    FollowLink,
    /// Places the caret in the element's text.
    #[default]
    PlaceCaret,
    /// Nothing: the element can't be edited here.
    None,
}

/// The field, content control or merge-designer element under the pointer.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ElementHover {
    /// The element; `exact` is always false.
    pub element: super::state::ElementContext,
    /// Its boxes on screen, one per line it spans.
    pub boxes: Vec<ClientRect>,
    /// What a press there does.
    pub action: PressAction,
}

/// A click on an element, after the editor's own action for it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ElementClick {
    /// The element; `exact` is always false.
    pub element: super::state::ElementContext,
    /// 1 for a click, 2 for a double click, 3 for a triple click.
    pub count: u8,
    /// The button.
    pub button: super::input::PointerButton,
    /// The modifier keys held.
    pub modifiers: super::input::Modifiers,
}

/// The innermost element the selection is in or covers.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ElementSelection {
    /// The element.
    pub element: super::state::ElementContext,
    /// Its boxes on screen, one per line it spans; none while it is scrolled out of view.
    pub boxes: Vec<ClientRect>,
}

/// Which hovered elements a cursor override applies to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CursorTarget {
    /// Elements of a kind.
    Kind(ElementKind),
    /// Elements a press on does this.
    Action(PressAction),
}

/// The pointer shape to show over some elements instead of the editor's own.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CursorOverride {
    /// The elements.
    pub on: CursorTarget,
    /// The shape.
    pub cursor: crate::editing::input::Cursor,
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

/// What `wordCount` counts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WordCountOptions {
    /// The selection's text, when there is a selection (the document's otherwise): its lines are the body lines it
    /// covers part of, and the pages are the document's. The status bar's "N of M words" asks with this on and off.
    #[serde(default)]
    pub selection: bool,
    /// Text boxes, footnotes and endnotes count as well as the body (Word's Include textboxes, footnotes and
    /// endnotes). Default: on.
    #[serde(default = "on")]
    pub include_textboxes_and_notes: bool,
}

fn on() -> bool {
    true
}

impl Default for WordCountOptions {
    fn default() -> WordCountOptions {
        WordCountOptions { selection: false, include_textboxes_and_notes: true }
    }
}

/// The undo and redo lists (Word's Undo and Redo drop-downs).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HistoryLabels {
    /// The steps undo would undo, most recent first.
    pub undo: Vec<String>,
    /// The steps redo would redo, next first.
    pub redo: Vec<String>,
    /// There are more undo steps than listed.
    pub more_undo: bool,
    /// There are more redo steps than listed.
    pub more_redo: bool,
}

/// A footnote or endnote, as a notes list shows it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NoteInfo {
    /// Footnote or endnote.
    pub kind: super::command::NoteType,
    /// Its id as the document numbers it: stable while the document is open.
    pub id: i64,
    /// Its reference mark: its custom mark, else its number as laid out (empty before layout reaches it).
    pub mark: String,
    /// Its mark is a custom one.
    pub custom_mark: bool,
    /// Its text, plain (fields as their results), at most [`NOTE_TEXT_LIMIT`] characters and then `…`.
    pub text: String,
    /// The page showing its reference (from 1); absent while nothing refers to it or layout hasn't reached it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
}

/// The most characters of a note's text a [`NoteInfo`] carries.
pub const NOTE_TEXT_LIMIT: usize = 1024;

/// One note separator's text.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NoteSeparatorState {
    /// Its plain text, paragraphs `\n` apart (empty for the built-in line).
    pub text: String,
    /// It is the built-in one: a line.
    pub builtin: bool,
}

/// The separators of footnotes or endnotes.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NoteSeparators {
    /// Above the notes on the page they start.
    pub separator: NoteSeparatorState,
    /// Above the notes continued from the page before.
    pub continuation_separator: NoteSeparatorState,
    /// Below notes that continue on the next page; absent when there is none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub continuation_notice: Option<NoteSeparatorState>,
}

/// An editable range, as Restrict Editing lists it: the permissions over one stretch of the document.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EditableRangeInfo {
    /// Its permissions' ids, one per editor.
    pub ids: Vec<String>,
    /// Who may edit it.
    pub editors: Vec<super::value::RangeEditorRef>,
    /// The page it starts on (from 1); absent before layout reaches it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
}

/// Insert Field's categories.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FieldCategory {
    /// Date and Time.
    DateAndTime,
    /// Document Automation.
    DocumentAutomation,
    /// Document Information.
    DocumentInformation,
    /// Equations and Formulas.
    EquationsAndFormulas,
    /// Index and Tables.
    IndexAndTables,
    /// Links and References.
    LinksAndReferences,
    /// Mail Merge.
    MailMerge,
    /// Numbering.
    Numbering,
    /// User Information.
    UserInformation,
}

/// What a field argument names.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FieldArgKind {
    /// Any text.
    Text,
    /// A bookmark of the document.
    Bookmark,
    /// A style's name.
    Style,
    /// A document property's name.
    Property,
    /// A document variable's name.
    Variable,
    /// A file name or address.
    File,
    /// A sequence or list name, an entry or a merge field's name.
    Name,
    /// An expression or a comparison.
    Expression,
    /// A whole number.
    Number,
}

/// What a field switch takes after it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SwitchArgKind {
    /// Nothing: it is on or off.
    None,
    /// Text.
    Text,
    /// A whole number.
    Number,
    /// A bookmark.
    Bookmark,
    /// A style's name.
    Style,
    /// A sequence or caption label's name.
    Name,
    /// Heading or outline levels (`1-3`).
    Levels,
}

/// An argument a field takes, in order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FieldArgInfo {
    /// Its name.
    pub name: String,
    /// What it names.
    pub kind: FieldArgKind,
    /// The field needs it.
    pub required: bool,
}

/// A switch of a field's own.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FieldSwitchInfo {
    /// The letter (or `!`) after the backslash.
    pub flag: String,
    /// What it takes after it.
    pub arg: SwitchArgKind,
    /// What it does.
    pub description: String,
}

/// The general switches a field takes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FieldGeneralSwitches {
    /// `\*`: text and number formats.
    pub text: bool,
    /// `\#`: a numeric picture.
    pub numeric: bool,
    /// `\@`: a date-time picture.
    pub date: bool,
}

/// A field as Insert Field describes it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FieldDescription {
    /// Its name as a code writes it (`PAGE`, `=`).
    pub name: String,
    /// The categories it is listed under.
    pub categories: Vec<FieldCategory>,
    /// What it does.
    pub description: String,
    /// Its syntax, as Insert Field shows it under Field codes.
    pub syntax: String,
    /// Its arguments, in order.
    pub args: Vec<FieldArgInfo>,
    /// Its own switches.
    pub switches: Vec<FieldSwitchInfo>,
    /// The general switches it takes.
    pub formats: FieldGeneralSwitches,
    /// The engine computes its result (the others keep theirs, or the host's `setFieldResults` gives it).
    pub evaluated: bool,
}

/// A format a `\*` switch takes, and its name.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FieldFormatOption {
    /// As the code writes it (`Upper`).
    pub value: String,
    /// Its name in Insert Field (`Uppercase`).
    pub label: String,
}

/// The formats Insert Field offers.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FieldFormats {
    /// `\*` text formats.
    pub text_formats: Vec<FieldFormatOption>,
    /// `\*` number formats.
    pub number_formats: Vec<FieldFormatOption>,
    /// `\#` numeric pictures.
    pub numeric_pictures: Vec<String>,
    /// `\@` date-time pictures.
    pub date_pictures: Vec<String>,
}

/// A field switch chosen, with what follows it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FieldSwitchChoice {
    /// The letter (or `!`) after the backslash.
    pub flag: String,
    /// What follows it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
}

/// What a field code is made of: Insert Field's choices.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FieldChoice {
    /// The field's name.
    pub name: String,
    /// Its arguments, in order.
    #[serde(default)]
    pub args: Vec<String>,
    /// Its own switches.
    #[serde(default)]
    pub switches: Vec<FieldSwitchChoice>,
    /// `\*` formats other than MERGEFORMAT: a text or number format.
    #[serde(default)]
    pub formats: Vec<String>,
    /// `\#`: a numeric picture.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub numeric_picture: Option<String>,
    /// `\@`: a date-time picture.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date_picture: Option<String>,
    /// `\* MERGEFORMAT`: the result keeps its formatting through updates.
    #[serde(default)]
    pub preserve_formatting: bool,
}

/// What is wrong with a field code, as Insert Field flags it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase", deny_unknown_fields)]
pub enum FieldProblem {
    /// No field name.
    Empty,
    /// A field Word has no such name for (kept as written, its result too).
    UnknownField {
        /// The name.
        name: String,
    },
    /// A required argument left out.
    MissingArgument {
        /// The argument's name.
        name: String,
    },
    /// A switch the field doesn't take.
    UnknownSwitch {
        /// The switch's flag.
        flag: String,
    },
    /// A switch the field needs left out.
    MissingSwitch {
        /// The switch's flag.
        flag: String,
    },
    /// A switch that takes a value given none.
    MissingSwitchValue {
        /// The switch's flag.
        flag: String,
    },
    /// A switch that takes a whole number given something else.
    NotANumber {
        /// The switch's flag.
        flag: String,
    },
    /// A general switch the field doesn't take (`*`, `#` or `@`).
    FormatNotTaken {
        /// The switch's flag.
        flag: String,
    },
}

/// A field code made from choices, and its problems.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ComposedField {
    /// The code, with Word's spaces around it.
    pub code: String,
    /// What is wrong with it; empty when Insert Field takes it as it is.
    pub problems: Vec<FieldProblem>,
}

/// The field the selection is in, and its code's choices: Insert Field opened on it edits it (`setField`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FieldHere {
    /// The field.
    pub id: ElementId,
    /// Its code's choices.
    pub choice: FieldChoice,
}

/// A page thumbnail's drawn size, in pixels, centred in the buffer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ThumbnailSize {
    /// The page's width.
    pub width: u32,
    /// The page's height.
    pub height: u32,
}
