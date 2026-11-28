//! The state snapshot: what an application's controls show, with mixed values explicit. A session rebuilds it after
//! every observable change; `sequence` counts those, `revision` counts document changes.

use std::collections::BTreeMap;

use serde::de::{DeserializeOwned, Error as _};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::command::{CommandId, SelectionMode};
use super::fonts::GenericFamily;
use super::services::{DialogKind, RulerModel, RulerUnit, TabSelector};
use super::value::{
    Alignment, AutoOr, CellAlignment, CellBlock, Color, CommentId, ElementId, HighlightColor, HorizontalAlignment, HorizontalPosition, LineSpacing,
    NoneOr, ObjectWrap, Orientation, Points, RevisionId, RowHeight, Story, StyleId, TableWidth, TextPoint, TextRange, UnderlineStyle,
    VerticalPosition, WrapSide,
};

pub use crate::docx::review::MarkupView;

/// A value over the selection: the same everywhere, different in places (`'mixed'` on the wire), or not applicable
/// here (`null`: no table, nothing selected).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Tri<T> {
    /// The same value everywhere.
    Uniform(T),
    /// Different values in places (`'mixed'` on the wire).
    Mixed,
    /// Not applicable here (`null` on the wire).
    #[default]
    Unavailable,
}

impl<T> Tri<T> {
    /// The value changed by `f`, mixed and unavailable kept.
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Tri<U> {
        match self {
            Tri::Uniform(v) => Tri::Uniform(f(v)),
            Tri::Mixed => Tri::Mixed,
            Tri::Unavailable => Tri::Unavailable,
        }
    }
}

impl<T: Serialize> Serialize for Tri<T> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Tri::Uniform(v) => v.serialize(s),
            Tri::Mixed => s.serialize_str("mixed"),
            Tri::Unavailable => s.serialize_none(),
        }
    }
}

impl<'de, T: DeserializeOwned> Deserialize<'de> for Tri<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Tri<T>, D::Error> {
        match serde_json::Value::deserialize(d)? {
            serde_json::Value::Null => Ok(Tri::Unavailable),
            serde_json::Value::String(s) if s == "mixed" => Ok(Tri::Mixed),
            v => T::deserialize(v).map(Tri::Uniform).map_err(D::Error::custom),
        }
    }
}

/// Editing, or template design (activations go to the application). Read only is its own flag.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Mode {
    /// Editing the document (the default).
    #[default]
    Editing,
    /// Designing a template: activations go to the application.
    Design,
}

/// What the editor is on: the template, or one of its sub-templates.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum EditingTarget {
    /// The document itself.
    #[default]
    Template,
    /// A sub-template's body.
    SubTemplate {
        /// Its name.
        name: String,
        /// Its building block category.
        category: String,
    },
}

impl EditingTarget {
    /// Whether it is the template.
    pub fn is_template(&self) -> bool {
        *self == EditingTarget::Template
    }
}

/// Whether the document is open and laid out.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LoadState {
    /// A document is being opened.
    Opening,
    /// Open and laid out (the default).
    #[default]
    Ready,
    /// Open, and still being laid out.
    LayingOut,
}

/// The state snapshot.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct State {
    /// +1 per committed transaction.
    pub revision: u64,
    /// +1 per observable change, selection included.
    pub sequence: u64,
    /// Whether a document is opening, ready, or still being laid out.
    pub load: LoadState,
    /// Editing or template design.
    pub mode: Mode,
    /// The template, or the sub-template being edited (absent on the wire for the template).
    #[serde(default, skip_serializing_if = "EditingTarget::is_template")]
    pub editing: EditingTarget,
    /// Commands that change the document fail with `read-only`.
    pub read_only: bool,
    /// Changed since the last open or DOCX export.
    pub modified: bool,
    /// What is selected and where.
    pub selection: SelectionSummary,
    /// The selection's character formatting.
    pub formatting: CharacterFormatting,
    /// The selection's paragraph formatting.
    pub paragraph: ParagraphFormatting,
    /// The selection's section.
    pub page: PageFormatting,
    /// Each command's availability and value, by id.
    pub commands: BTreeMap<CommandId, CommandState>,
    /// Each dialog's availability: whether `dialog_open` opens it now.
    pub dialogs: BTreeMap<DialogKind, bool>,
    /// What can be undone and redone.
    pub history: HistoryState,
    /// The pages laid out so far, whether that is all of them, and the caret's page.
    pub pages: PagesState,
    /// The view settings.
    pub view: ViewState,
    /// Track changes and the review counts.
    pub review: ReviewState,
    /// Spell checking.
    pub spelling: SpellingState,
    /// Font families being fetched, and those no font was found for.
    pub fonts: FontsState,
    /// The ruler model for the selection, in points.
    pub ruler: RulerModel,
    /// Formatting is held for `pasteFormatting`.
    pub format_painter: bool,
    /// The document's 12 theme colours, in `ThemeColour` order (text1, background1, text2, background2, accent1 to
    /// accent6, hyperlink, followedHyperlink), for an application's colour picker.
    #[serde(default)]
    pub theme_colours: Vec<Color>,
    /// What typing corrects.
    #[serde(default)]
    pub auto_correct: super::services::AutoCorrectOptions,
    /// The cut, copy and paste options.
    #[serde(default)]
    pub paste_options: super::command::PasteSettings,
    /// The caption inserted tables get (AutoCaption); absent when off.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_caption: Option<super::command::AutoCaption>,
    /// The theme's heading and body fonts.
    #[serde(default)]
    pub theme_fonts: ThemeFonts,
}

/// The theme's font families.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ThemeFonts {
    /// The heading font (`+Headings`).
    pub heading: String,
    /// The body font (`+Body`).
    pub body: String,
}

/// One cell of the theme colour grid.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ThemeSwatch {
    /// The colour as commands take it: the theme colour with its tint or shade.
    pub colour: crate::api::drawing::ColourValue,
    /// What it shows.
    pub rgb: Color,
    /// Its name: "Accent 1", "Accent 1, Lighter 40%".
    pub label: String,
}

/// A column of the theme colour grid: the theme colour, then its five lighter and darker variants.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ThemeColumn {
    /// The theme colour.
    pub theme: crate::api::drawing::ThemeColour,
    /// The theme colour itself, then its variants, top to bottom.
    pub swatches: Vec<ThemeSwatch>,
}

/// One command's state.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CommandState {
    /// Whether running it now would succeed.
    pub enabled: bool,
    /// For on/off commands: whether it is on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checked: Option<bool>,
    /// The command's state value, where it has one (bold's `Tri`, undo's label).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<CommandValue>,
}

/// A command's state value. On the wire it is the value itself.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CommandValue {
    /// An on/off value (bold).
    Toggle(Tri<bool>),
    /// A text value (the font family, undo's label).
    Text(Tri<String>),
    /// A length, in points.
    Length(Tri<Points>),
    /// A number (the list level).
    Number(Tri<f64>),
    /// An underline style.
    Underline(Tri<UnderlineStyle>),
    /// A font color, or automatic.
    Color(Tri<AutoOr<Color>>),
    /// A highlight color, or none.
    Highlight(Tri<NoneOr<HighlightColor>>),
    /// A paragraph alignment.
    Alignment(Tri<Alignment>),
    /// A line spacing.
    LineSpacing(Tri<LineSpacing>),
    /// A list kind.
    List(Tri<ListState>),
    /// The selection mode.
    SelectionMode(SelectionMode),
    /// The markup view.
    MarkupView(MarkupView),
    /// The session mode.
    Mode(Mode),
    /// What a click on the ruler adds.
    TabType(crate::api::services::TabSelector),
    /// A positioned table's place (absent for a table in the text).
    TablePosition(crate::api::command::TablePosition),
}

/// What is selected and where.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SelectionSummary {
    /// What kind of selection it is.
    pub kind: SelectionKind,
    /// The story the selection is in.
    pub story: Story,
    /// The page of the caret, from 1.
    pub page: u32,
    /// How the selection extends.
    pub mode: SelectionMode,
    /// Nothing is selected (a caret).
    pub empty: bool,
    /// The selection as plain text, paragraphs separated by `\n` (empty for a caret), or `None` when it is longer
    /// than [`SELECTION_TEXT_LIMIT`] characters.
    pub text: Option<String>,
    /// Every selected range of text in document order: one for a text selection, one per line for a column block,
    /// several for a discontiguous selection (`ranges.length > 1`); empty for a caret, cells or an object. At most
    /// [`SELECTION_RANGES_LIMIT`] of them.
    #[serde(default)]
    pub ranges: Vec<TextRange>,
    /// The selection has more ranges than `ranges` lists.
    #[serde(default)]
    pub ranges_truncated: bool,
    /// The selected blocks of cells of a discontiguous selection, in the order they were added; the selection's own
    /// block is the last when `kind` is `cells`. Empty when at most one block is selected (`table` describes it).
    #[serde(default)]
    pub cell_blocks: Vec<CellBlock>,
    /// The selection's table, or `None` outside tables.
    pub table: Option<TableSelection>,
    /// The selected picture or text box, or `None`.
    pub object: Option<ObjectSelection>,
    /// Innermost first: the element the selection is in or covers, then its parents.
    pub context: Vec<ElementContext>,
    /// The link at the selection, or `None`.
    pub link: Option<LinkInfo>,
    /// The thread at the caret.
    pub comment: Option<CommentId>,
    /// The revision at the caret.
    pub revision: Option<RevisionId>,
    /// Set while a header or footer is being edited.
    pub header_footer: Option<HeaderFooterInfo>,
    /// Set while F2 (`moveText`) or Shift+F2 (`copyText`) waits for the place to move or copy the selection to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_move: Option<PendingMove>,
    /// Set while a list's numbers are selected (a click on a number): character formatting commands then format the
    /// numbers of that list level, and any selection move ends it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub list_numbers: Option<ListNumbers>,
}

/// A list level whose numbers are selected.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListNumbers {
    /// The list: a `ListDefinition` id.
    pub list: i64,
    /// The level, 0 to 8.
    pub level: u8,
}

/// An F2 or Shift+F2 move waiting for its place: a click or the arrow keys choose it, Enter moves the text there, Esc
/// gives up.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PendingMove {
    /// Copies (Shift+F2) rather than moves.
    pub copy: bool,
    /// A place is chosen: Enter moves the text there.
    pub placed: bool,
}

/// The kinds of selection.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SelectionKind {
    /// A caret, nothing selected (the default).
    #[default]
    Caret,
    /// A range of text.
    Text,
    /// A block of table cells.
    Cells,
    /// A picture or a text box.
    Object,
    /// A whole merge-designer element.
    Element,
}

/// The selection's table: its shape, the current cell (from 0) and the values the table commands set.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TableSelection {
    /// The number of rows.
    pub rows: u32,
    /// The number of columns.
    pub columns: u32,
    /// The current cell's row, from 0.
    pub row: u32,
    /// The current cell's column, from 0.
    pub column: u32,
    /// Whether the selected cells can merge.
    pub can_merge: bool,
    /// The first row repeats as a header on each page.
    pub header_row: Tri<bool>,
    /// Rows can break across pages.
    pub row_can_break: Tri<bool>,
    /// The cells' vertical alignment.
    pub cell_alignment: Tri<CellAlignment>,
    /// The cells' shading, or none.
    pub cell_shading: Tri<NoneOr<Color>>,
    /// The rows' height, or automatic.
    pub row_height: Tri<AutoOr<RowHeight>>,
    /// The column width.
    pub column_width: Tri<Points>,
    /// The table style, or `None`.
    pub style: Option<StyleId>,
    /// Which parts of the table style apply.
    pub style_options: TableStyleOptions,
    /// The table width.
    pub width: TableWidth,
    /// The table's alignment between the margins.
    pub alignment: HorizontalAlignment,
    /// The table indent.
    pub indent: Points,
    /// The cells' preferred width.
    pub cell_width: Tri<TableWidth>,
    /// The direction the cells' text runs in.
    pub text_direction: Tri<crate::api::command::TextFlow>,
    /// The cells' text wraps.
    pub wrap_text: Tri<bool>,
    /// The cells' text is squeezed to fit.
    pub fit_text: Tri<bool>,
    /// The rows' end marks are hidden.
    pub row_hidden: Tri<bool>,
    /// The space between the table's cells; none when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cell_spacing: Option<Points>,
    /// The table's alternative text title.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,
    /// The table's alternative text description.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// The rows and columns in each of the table's bands.
    #[serde(default)]
    pub bands: TableBands,
    /// The selected cells' borders, edge by edge: each cell's own line, else its row's or table's for that edge.
    #[serde(default)]
    pub cell_borders: CellBorderLines,
    /// The table's own borders.
    #[serde(default)]
    pub table_borders: TableBorderLines,
    /// The selected cells' shading with its pattern (`cellShading` is its fill alone).
    #[serde(default)]
    pub cell_shading_value: Tri<NoneOr<Shading>>,
    /// The table's shading (behind spaced cells); absent for none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table_shading: Option<Shading>,
    /// The selected cells' margins on each side.
    #[serde(default)]
    pub cell_margins: CellMarginsState,
    /// The table's default cell margins (Table Options).
    #[serde(default)]
    pub default_cell_margins: TableCellMarginsState,
    /// The selected cells' end marks take no height.
    #[serde(default)]
    pub hide_mark: Tri<bool>,
    /// The selected rows' exceptions to the table's properties.
    #[serde(default)]
    pub row_exceptions: RowExceptionsState,
    /// How the table's columns are sized.
    #[serde(default)]
    pub fit: TableFit,
    /// The columns the current cell spans.
    #[serde(default)]
    pub column_span: u32,
    /// The rows the current cell spans (more than one: merged down).
    #[serde(default)]
    pub row_span: u32,
}

/// The selected cells' edges, each the same line over them, mixed, or `none`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CellBorderLines {
    /// The selection's top edge.
    pub top: Tri<NoneOr<crate::api::command::Border>>,
    /// Its left edge.
    pub left: Tri<NoneOr<crate::api::command::Border>>,
    /// Its bottom edge.
    pub bottom: Tri<NoneOr<crate::api::command::Border>>,
    /// Its right edge.
    pub right: Tri<NoneOr<crate::api::command::Border>>,
    /// The lines between the selected rows (`null` with one row).
    pub inside_horizontal: Tri<NoneOr<crate::api::command::Border>>,
    /// The lines between the selected columns (`null` with one column).
    pub inside_vertical: Tri<NoneOr<crate::api::command::Border>>,
    /// The cells' diagonal from top left to bottom right.
    pub diagonal_down: Tri<NoneOr<crate::api::command::Border>>,
    /// The cells' diagonal from bottom left to top right.
    pub diagonal_up: Tri<NoneOr<crate::api::command::Border>>,
}

/// A table's own borders; absent for none.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TableBorderLines {
    /// The top edge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top: Option<crate::api::command::Border>,
    /// The left edge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left: Option<crate::api::command::Border>,
    /// The bottom edge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bottom: Option<crate::api::command::Border>,
    /// The right edge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub right: Option<crate::api::command::Border>,
    /// The lines between rows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inside_horizontal: Option<crate::api::command::Border>,
    /// The lines between columns.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inside_vertical: Option<crate::api::command::Border>,
}

/// The selected cells' margins, each the same over them or mixed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CellMarginsState {
    /// The top margin.
    pub top: Tri<Points>,
    /// The left margin.
    pub left: Tri<Points>,
    /// The bottom margin.
    pub bottom: Tri<Points>,
    /// The right margin.
    pub right: Tri<Points>,
}

/// A table's default cell margins.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TableCellMarginsState {
    /// The top margin.
    pub top: Points,
    /// The left margin.
    pub left: Points,
    /// The bottom margin.
    pub bottom: Points,
    /// The right margin.
    pub right: Points,
}

/// The selected rows' exceptions to their table's properties, each the same over them or mixed; `none` where a row
/// takes the table's.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RowExceptionsState {
    /// The rows carry exceptions at all.
    pub any: Tri<bool>,
    /// The rows' own alignment.
    pub alignment: Tri<NoneOr<HorizontalAlignment>>,
    /// The rows' own indent from the table's left edge.
    pub indent: Tri<NoneOr<Points>>,
    /// The rows' own space between cells.
    pub cell_spacing: Tri<NoneOr<Points>>,
    /// The rows' own shading.
    pub shading: Tri<NoneOr<Shading>>,
    /// The rows' own borders.
    pub borders: Tri<NoneOr<TableBorderLines>>,
}

/// How a table's columns are sized.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TableFit {
    /// Fixed column widths.
    #[default]
    Fixed,
    /// Columns fitted to their contents (AutoFit Contents).
    Contents,
    /// The table fills the width it is in (AutoFit Window).
    Window,
}

/// The rows and columns in each of a table's bands: as it lays out, and its own (over its style's).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TableBands {
    /// Rows in each band.
    pub rows: u32,
    /// Columns in each band.
    pub columns: u32,
    /// The table's own rows in each band; none: its style's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub own_rows: Option<u32>,
    /// The table's own columns in each band.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub own_columns: Option<u32>,
}

/// Which parts of a table style apply.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TableStyleOptions {
    /// The header row is formatted.
    pub header_row: bool,
    /// The total (last) row is formatted.
    pub total_row: bool,
    /// The first column is formatted.
    pub first_column: bool,
    /// The last column is formatted.
    pub last_column: bool,
    /// Rows alternate.
    pub banded_rows: bool,
    /// Columns alternate.
    pub banded_columns: bool,
}

/// The kinds of selectable object.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ObjectKind {
    /// A picture (the default).
    #[default]
    Picture,
    /// A text box.
    TextBox,
    /// A shape.
    Shape,
    /// A line or connector (a shape whose preset has no filled path).
    Line,
    /// A group of objects.
    Group,
    /// A frame: paragraphs placed on the page (a drop cap is one).
    Frame,
}

/// The selected picture or text box.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ObjectSelection {
    /// A picture or a text box.
    pub kind: ObjectKind,
    /// How text wraps around it.
    pub wrap: ObjectWrap,
    /// The width.
    pub width: Points,
    /// The height.
    pub height: Points,
    /// The alternative text.
    pub alt_text: String,
    /// Marked decorative.
    pub decorative: bool,
    /// `null` for inline objects.
    pub anchor_locked: Option<bool>,
    /// Where a floating object is and how text keeps from it; `null` for inline objects.
    pub placement: Option<ObjectPlacement>,
    /// The shape's properties as written; `null` for objects without shape properties (frames) and kept graphics.
    #[serde(default)]
    pub shape: Option<crate::api::drawing::ShapeState>,
    /// Its rotation, clockwise.
    #[serde(default)]
    pub rotation: crate::api::drawing::Degrees,
    /// Flipped left to right.
    #[serde(default)]
    pub flip_h: bool,
    /// Flipped top to bottom.
    #[serde(default)]
    pub flip_v: bool,
    /// The selected child's place in its group; empty unless a group child is selected.
    #[serde(default)]
    pub child_path: Vec<u16>,
    /// How many objects are selected (2 or more: a multi-selection, the other fields describing the first).
    #[serde(default)]
    pub count: u16,
    /// A picture's crop, by fractions of its image from each edge; absent when uncropped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crop: Option<crate::api::command::PictureCrop>,
    /// A picture's Set Transparent Colour; absent for none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transparent_color: Option<Color>,
    /// A floating object's width as a share of a box on the page; absent for an absolute width.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relative_width: Option<crate::api::command::RelativeSize>,
    /// Its height as a share of a box on the page; absent for an absolute height.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relative_height: Option<crate::api::command::RelativeSize>,
    /// A floating object's id (a group child's when one is selected); absent for inline objects and frames.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<crate::api::value::ObjectId>,
    /// Its name, as the Selection Pane lists it.
    #[serde(default)]
    pub name: String,
    /// Its alternative text's title.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,
    /// Hidden: not drawn, and text doesn't wrap around it.
    #[serde(default)]
    pub hidden: bool,
    /// Resizing it keeps its aspect ratio (Lock aspect ratio).
    #[serde(default)]
    pub lock_aspect: bool,
    /// A picture's image at its own size; absent for other objects.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_size: Option<crate::api::value::Size>,
    /// Its place in the stack of its page's floating objects, 0 at the front; absent for inline objects and frames.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stack_index: Option<u32>,
    /// A picture's adjustments; absent for other objects.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub picture_adjust: Option<crate::api::drawing::PictureAdjustState>,
    /// A text box's place in a chain of linked text boxes; absent when it is in none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_box_link: Option<TextBoxLinkState>,
    /// A floating object's wrap polygon; absent when it has none and its wrapping is neither tight nor through.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wrap_polygon: Option<WrapPolygonState>,
    /// A frame's properties; absent for other objects.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame: Option<FrameState>,
}

/// A text box's place in a chain of linked text boxes: its text flows on from the one before into the one after.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextBoxLinkState {
    /// Its place in the chain, from 0.
    pub index: u32,
    /// How many boxes are in the chain.
    pub count: u32,
    /// The box before it; absent for the first.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous: Option<crate::api::value::ObjectId>,
    /// The box after it; absent for the last.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next: Option<crate::api::value::ObjectId>,
}

/// A floating object's wrap polygon: the outline tight and through wrapping keep text to.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WrapPolygonState {
    /// Its vertices, unclosed.
    pub points: Vec<crate::api::drawing::BoxPoint>,
    /// Edited by hand (Edit Wrap Points): kept when the object changes; otherwise made again from its outline.
    pub edited: bool,
}

/// A frame's placement and whether other frames and objects may overlap it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FrameState {
    /// Its wrapping, size, position and distance from text.
    pub properties: crate::api::command::FrameProperties,
    /// Other floating objects may overlap it.
    pub allow_overlap: bool,
}

/// A floating object as the Selection Pane lists it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ObjectListEntry {
    /// The object.
    pub object: crate::api::value::ObjectId,
    /// Its name.
    pub name: String,
    /// What it is.
    pub kind: ObjectKind,
    /// Hidden.
    pub hidden: bool,
    /// A group's objects, front first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<ObjectListEntry>,
}

/// A floating object's place and its wrapping options.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ObjectPlacement {
    /// The horizontal place: an alignment or an offset.
    pub horizontal: HorizontalPosition,
    /// The vertical place: an alignment or an offset.
    pub vertical: VerticalPosition,
    /// Other floating objects may overlap it.
    pub allow_overlap: bool,
    /// In a table cell, it is kept within the cell.
    pub layout_in_cell: bool,
    /// The sides text wraps on.
    pub wrap_side: WrapSide,
    /// The distance from the text: top, bottom, left and right.
    pub distance: TextDistance,
}

/// How far text keeps from a floating object.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextDistance {
    /// Above it.
    pub top: Points,
    /// Below it.
    pub bottom: Points,
    /// On its left.
    pub left: Points,
    /// On its right.
    pub right: Points,
}

/// The kinds of merge-designer element.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ElementKind {
    /// A field (the default).
    #[default]
    Field,
    /// A rich text control.
    RichText,
    /// A plain text control.
    PlainText,
    /// A picture control.
    Picture,
    /// A checkbox control.
    Checkbox,
    /// A date control.
    Date,
    /// A drop-down list control.
    DropDown,
    /// A combo box control.
    ComboBox,
    /// A group control.
    Group,
    /// A repeating section control.
    RepeatingSection,
    /// One item of a repeating section (selection context only).
    RepeatingItem,
    /// A building block gallery control.
    BuildingBlockGallery,
    /// A building block inserted from a gallery (a cover page, a table of contents).
    BuildingBlock,
    /// A citation control.
    Citation,
    /// A bibliography control.
    Bibliography,
    /// An equation control.
    Equation,
}

/// An element the selection is in or covers.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ElementContext {
    /// The element's id.
    pub id: ElementId,
    /// What kind of element it is.
    pub kind: ElementKind,
    /// The selection is exactly the element.
    pub exact: bool,
    /// Locked against editing: a locked field, or a control whose content can't be edited.
    #[serde(default)]
    pub locked: bool,
    /// A field shows its code rather than its result.
    #[serde(default)]
    pub showing_code: bool,
    /// A field's result is out of date: the next field update refreshes it.
    #[serde(default)]
    pub dirty: bool,
    /// A field's code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// A control's tag.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    /// A control's title.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// A picture control's picture size.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<super::value::Size>,
    /// A legacy form field's options and value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub form: Option<Box<FormFieldInfo>>,
    /// A content control's look and options.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub control: Option<Box<ControlInfo>>,
}

/// A content control's look and options; what doesn't apply to its kind is absent.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ControlInfo {
    /// How it shows its edges.
    pub appearance: super::value::ControlAppearance,
    /// Its colour; absent for the default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<Color>,
    /// It goes when its content is edited.
    pub temporary: bool,
    /// The character style text typed into it takes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<StyleId>,
    /// A check box's checked symbol.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checked_symbol: Option<String>,
    /// A check box's unchecked symbol.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unchecked_symbol: Option<String>,
    /// What a repeating section's items are called.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub section_title: Option<String>,
    /// A repeating section's items can be added and removed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allow_insert_delete: Option<bool>,
    /// A building block gallery's entries: the package's building blocks in its gallery and category, by name.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub building_blocks: Vec<String>,
    /// A building block gallery's (or building block's) gallery, by Word's name for it; absent: any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gallery: Option<String>,
    /// A building block gallery's (or building block's) category; absent: any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    /// A date control's date (`YYYY-MM-DDTHH:MM:SSZ`); absent while it shows its placeholder or no date.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    /// A date control's calendar.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calendar: Option<super::value::CalendarType>,
    /// What a date control stores in its data.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub store_as: Option<super::value::DateStorage>,
}

/// A legacy form field's options and value; what doesn't apply to its kind is absent.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FormFieldInfo {
    /// Its kind.
    pub kind: super::command::FormFieldType,
    /// Its bookmark name.
    pub name: String,
    /// It can be filled in.
    pub enabled: bool,
    /// The document's fields update when it is left.
    pub calculate_on_exit: bool,
    /// The help F1 shows.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub help_text: String,
    /// The help the status bar shows.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub status_text: String,
    /// `helpText` names an AutoText entry whose text is the help.
    #[serde(default)]
    pub help_is_auto_text: bool,
    /// `statusText` names an AutoText entry whose text is the help.
    #[serde(default)]
    pub status_is_auto_text: bool,
    /// The macro run on entry.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub entry_macro: String,
    /// The macro run on exit.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub exit_macro: String,
    /// What a text field takes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_type: Option<super::command::TextInputType>,
    /// A text field's default text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_text: Option<String>,
    /// The most characters a text field takes (0: any).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_length: Option<u32>,
    /// A text field's text format (`Uppercase`, a number or date picture).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    /// A check box is checked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checked: Option<bool>,
    /// A check box's size, or its text's (`auto`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<super::value::AutoOr<Points>>,
    /// A check box is checked by default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checked_by_default: Option<bool>,
    /// A drop-down's entries.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entries: Vec<String>,
    /// The entry a drop-down shows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry: Option<u32>,
    /// The entry a drop-down shows by default, from 0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_entry: Option<u32>,
}

/// The link at the selection.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LinkInfo {
    /// Where the link goes: its target, then `#` and the bookmark when it names one.
    pub address: String,
    /// The link's display text.
    pub text: String,
    /// The target without its bookmark: empty for a place in this document.
    #[serde(default)]
    pub target: String,
    /// The place in the target it goes to (a bookmark name).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bookmark: Option<String>,
    /// The ScreenTip shown when it is hovered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screen_tip: Option<String>,
    /// The window or frame it opens in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame: Option<String>,
}

/// A header or a footer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HeaderFooterKind {
    /// A header (the default).
    #[default]
    Header,
    /// A footer.
    Footer,
}

/// Which pages a header or footer is for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HeaderFooterType {
    /// For every page (the default).
    #[default]
    Default,
    /// For the first page of the section.
    First,
    /// For even pages.
    Even,
}

/// The header or footer being edited.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HeaderFooterInfo {
    /// A header or a footer.
    pub kind: HeaderFooterKind,
    /// Which pages it is for (`type` on the wire).
    #[serde(rename = "type")]
    pub pages: HeaderFooterType,
    /// Its section, from 1.
    pub section: u32,
    /// The page it is being edited on, from 1.
    pub page: u32,
    /// `First Page Header -Section 2-`.
    pub label: String,
}

/// The selection's character formatting.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CharacterFormatting {
    /// The document's family, never the substituted one.
    pub font: Tri<String>,
    /// The font size.
    pub size: Tri<Points>,
    /// Bold.
    pub bold: Tri<bool>,
    /// Italic.
    pub italic: Tri<bool>,
    /// The underline style.
    pub underline: Tri<UnderlineStyle>,
    /// Strikethrough.
    pub strikethrough: Tri<bool>,
    /// Double strikethrough.
    pub double_strikethrough: Tri<bool>,
    /// Subscript.
    pub subscript: Tri<bool>,
    /// Superscript.
    pub superscript: Tri<bool>,
    /// Small caps.
    pub small_caps: Tri<bool>,
    /// All caps.
    pub all_caps: Tri<bool>,
    /// Hidden text.
    pub hidden: Tri<bool>,
    /// The font color, or automatic.
    pub color: Tri<AutoOr<Color>>,
    /// The highlight color, or none.
    pub highlight: Tri<NoneOr<HighlightColor>>,
    /// The character spacing.
    pub character_spacing: Tri<Points>,
    /// The raised (positive) or lowered position.
    pub position: Tri<Points>,
    /// The character style, or none.
    pub character_style: Tri<NoneOr<StyleId>>,
    /// The horizontal scale, percent.
    pub scale: Tri<f64>,
    /// Kerning at or above this size; 0: off.
    pub kerning: Tri<Points>,
    /// The ligatures used (a combination Word's dialog doesn't offer reads as mixed).
    pub ligatures: Tri<crate::api::command::Ligatures>,
    /// The digits' widths.
    pub number_spacing: Tri<crate::api::command::NumberSpacing>,
    /// The digits' forms.
    pub number_form: Tri<crate::api::command::NumberForm>,
    /// The stylistic set (1…20), or 0 for the font's default.
    pub stylistic_set: Tri<u8>,
    /// Contextual alternates.
    pub contextual_alternates: Tri<bool>,
    /// The language (a BCP 47 tag).
    pub language: Tri<String>,
    /// Spelling and grammar are not checked.
    pub no_proof: Tri<bool>,
    /// Emboss.
    pub emboss: Tri<bool>,
    /// Engrave.
    pub engrave: Tri<bool>,
    /// Outline.
    pub outline: Tri<bool>,
    /// Shadow.
    pub shadow: Tri<bool>,
    /// The underline's colour, or the text's (`auto`).
    pub underline_color: Tri<AutoOr<Color>>,
    /// The text effects (fill, outline, shadow, glow and reflection), through the character style.
    pub text_effects: Tri<crate::api::drawing::TextEffects>,
    /// The theme font the text's font is, or none (a family by name).
    pub font_theme: Tri<NoneOr<super::value::ThemeFont>>,
    /// The text's colour as written: RGB, or a theme colour with its tint or shade (`color` is what it shows).
    pub color_value: Tri<AutoOr<crate::api::drawing::ColourValue>>,
    /// The underline's colour as written, or the text's (`auto`).
    pub underline_color_value: Tri<AutoOr<crate::api::drawing::ColourValue>>,
    /// The text's border (with its distance and shadow), or none.
    pub text_border: Tri<NoneOr<crate::api::command::Border>>,
    /// The text's shading, or none.
    pub text_shading: Tri<NoneOr<Shading>>,
    /// The width the text is fitted to (Fit Text), or none.
    pub fit_text: Tri<NoneOr<Points>>,
    /// Each kind of OpenType ligature on or off.
    pub ligature_flags: Tri<crate::api::command::LigatureFlags>,
    /// The OpenType stylistic sets on (1…20), in order.
    pub stylistic_sets: Tri<Vec<u8>>,
}

/// Shading: a fill, and a pattern in the pattern colour over it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Shading {
    /// The fill, or none.
    pub fill: NoneOr<crate::api::drawing::ColourValue>,
    /// The pattern.
    pub pattern: crate::api::command::ShadingPattern,
    /// The pattern's colour.
    pub color: AutoOr<crate::api::drawing::ColourValue>,
}

/// A paragraph's list.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ListState {
    /// A bulleted list.
    Bullet,
    /// A numbered list.
    Numbered,
    /// Not in a list (the default).
    #[default]
    None,
}

/// A paragraph's borders, as the border presets name them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BorderState {
    /// A box around the paragraph.
    Box,
    /// A bottom border only.
    Bottom,
    /// No borders (the default).
    #[default]
    None,
    /// Borders no preset makes.
    Other,
}

/// The selected paragraphs' indents.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IndentState {
    /// The left indent.
    pub left: Tri<Points>,
    /// The right indent.
    pub right: Tri<Points>,
    /// The first-line indent.
    pub first_line: Tri<Points>,
    /// The hanging indent.
    pub hanging: Tri<Points>,
    /// Mirror indents (inside and outside on facing pages).
    pub mirror: Tri<bool>,
}

/// The selected paragraphs' pagination settings.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FlowState {
    /// Keep with the next paragraph.
    pub keep_with_next: Tri<bool>,
    /// Keep the lines together.
    pub keep_lines_together: Tri<bool>,
    /// Start on a new page.
    pub page_break_before: Tri<bool>,
    /// Widow and orphan control.
    pub widow_control: Tri<bool>,
    /// Don't hyphenate.
    pub dont_hyphenate: Tri<bool>,
    /// Left out of line numbering.
    pub suppress_line_numbers: Tri<bool>,
    /// In a text box, which lines text around it wraps tightly to.
    pub textbox_tight_wrap: Tri<crate::api::command::TightWrap>,
}

/// The selection's paragraph formatting.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ParagraphFormatting {
    /// The paragraph style.
    pub style: Tri<StyleId>,
    /// The alignment.
    pub alignment: Tri<Alignment>,
    /// The line spacing.
    pub line_spacing: Tri<LineSpacing>,
    /// The space before, or automatic.
    pub space_before: Tri<AutoOr<Points>>,
    /// The space after, or automatic.
    pub space_after: Tri<AutoOr<Points>>,
    /// The indents.
    pub indents: IndentState,
    /// The list kind.
    pub list: Tri<ListState>,
    /// The list level, from 0.
    pub list_level: Tri<u32>,
    /// The pagination settings.
    pub flow: FlowState,
    /// The borders.
    pub borders: Tri<BorderState>,
    /// The shading, or none.
    pub shading: Tri<NoneOr<Color>>,
    /// No space between paragraphs of the same style.
    pub contextual_spacing: Tri<bool>,
    /// The outline level (1…9), or body text (0).
    pub outline_level: Tri<u8>,
    /// Collapsed when the document opens.
    pub collapsed: Tri<bool>,
    /// The text direction.
    pub text_direction: Tri<crate::api::command::TextFlow>,
    /// The selection's paragraph's drop cap, when it has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drop_cap: Option<crate::api::command::DropCap>,
    /// The top border, with its colour as written, distance and shadow, or none.
    pub border_top: Tri<NoneOr<crate::api::command::Border>>,
    /// The left border.
    pub border_left: Tri<NoneOr<crate::api::command::Border>>,
    /// The bottom border.
    pub border_bottom: Tri<NoneOr<crate::api::command::Border>>,
    /// The right border.
    pub border_right: Tri<NoneOr<crate::api::command::Border>>,
    /// The line between paragraphs with the same borders.
    pub border_between: Tri<NoneOr<crate::api::command::Border>>,
    /// The shading in full (`shading` is its fill as shown), or none.
    pub shading_value: Tri<NoneOr<Shading>>,
    /// The tab stops, own and inherited, in position order: the same for every selected paragraph, or mixed.
    pub tabs: Tri<Vec<super::services::TabStopInfo>>,
}

/// A page size.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PageSize {
    /// The width.
    pub width: Points,
    /// The height.
    pub height: Points,
}

/// A section's margins.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PageMargins {
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
}

/// A section's text columns.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ColumnLayout {
    /// The number of columns.
    pub count: u32,
    /// The space between columns.
    pub spacing: Points,
    /// A line is drawn between columns.
    pub separator: bool,
    /// Each column's width and the space after it, when they differ.
    pub widths: Vec<crate::api::command::ColumnWidth>,
    /// The columns are of equal width (`widths` is then empty).
    #[serde(default)]
    pub equal: bool,
}

/// The selection's section's page setup.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PageFormatting {
    /// The page size.
    pub size: PageSize,
    /// Portrait or landscape.
    pub orientation: Orientation,
    /// The margins.
    pub margins: PageMargins,
    /// The text columns.
    pub columns: ColumnLayout,
    /// From the page's top edge to the header.
    pub header_distance: Points,
    /// From the page's bottom edge to the footer.
    pub footer_distance: Points,
    /// The first page has its own header and footer.
    pub different_first_page: bool,
    /// Odd and even pages have their own headers and footers.
    pub different_odd_even: bool,
    /// The page number format.
    pub page_number_format: crate::api::command::NumberFormat,
    /// The section's first page number, or `null` when it continues from the section before.
    pub page_number_start: Option<u32>,
    /// Chapter numbers in page numbers, or `null`.
    pub chapter_numbers: Option<crate::api::command::ChapterNumbers>,
    /// Where text sits between the top and bottom margins.
    pub vertical_alignment: crate::api::command::PageVerticalAlignment,
    /// The line numbering, or `null`.
    pub line_numbers: Option<crate::api::command::LineNumbers>,
    /// The break the section starts with.
    pub break_type: crate::api::command::SectionBreakKind,
    /// The section's text direction.
    pub text_direction: crate::api::command::TextFlow,
    /// How the document's pages pair.
    pub multiple_pages: crate::api::command::MultiplePagesMode,
    /// The gutter is at the top.
    pub gutter_at_top: bool,
    /// Which of the section's headers take the section before's (Link to Previous).
    pub header_links: HeaderFooterLinks,
    /// Which of its footers do.
    pub footer_links: HeaderFooterLinks,
    /// The section's footnote settings.
    pub footnotes: NoteSettings,
    /// The section's endnote settings.
    pub endnotes: NoteSettings,
    /// The footnote area's columns; 0 for the section's own.
    pub footnote_columns: u32,
    /// The page colour (a solid fill), its gradient, pattern or picture, or `null` for none. A picture's image is empty.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<crate::api::drawing::FillValue>,
    /// The watermark the pages show, or `null`. A picture's image is empty.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub watermark: Option<crate::api::command::Watermark>,
    /// The section's page borders.
    #[serde(default)]
    pub page_borders: PageBorders,
}

/// A section's page borders.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PageBorders {
    /// The top edge, if any; its distance is from the page's edge or the text, as `offsetFrom` says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top: Option<crate::api::command::Border>,
    /// The left edge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left: Option<crate::api::command::Border>,
    /// The bottom edge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bottom: Option<crate::api::command::Border>,
    /// The right edge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub right: Option<crate::api::command::Border>,
    /// What the distances are measured from.
    pub offset_from: crate::api::command::PageBorderOffset,
    /// The pages they show on.
    pub display: crate::api::command::PageBorderDisplay,
    /// They go around the header.
    pub surround_header: bool,
    /// They go around the footer.
    pub surround_footer: bool,
    /// They are behind the text.
    pub behind_text: bool,
}

/// A section's footnote or endnote settings.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NoteSettings {
    /// Where the notes go.
    pub position: crate::api::command::NotePosition,
    /// The number format.
    pub format: crate::api::command::NumberFormat,
    /// The first number.
    pub start: u32,
    /// When numbering starts again.
    pub restart: crate::api::command::NoteRestart,
}

/// Which of a section's headers or footers take the section before's.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HeaderFooterLinks {
    /// For every page.
    pub default: bool,
    /// For the first page.
    pub first: bool,
    /// For even pages.
    pub even: bool,
}

/// What can be undone and redone.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HistoryState {
    /// There is something to undo.
    pub can_undo: bool,
    /// There is something to redo.
    pub can_redo: bool,
    /// What undo would undo (`Typing`), or `None`.
    pub undo_label: Option<String>,
    /// What redo would redo, or `None`.
    pub redo_label: Option<String>,
}

/// The pages laid out so far and the caret's page (from 1). Background layout adds pages until `complete`; each change
/// to `count` or `complete` is a new state with the same revision.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PagesState {
    /// The number of pages laid out so far: all of them once `complete`.
    pub count: u32,
    /// The caret's page, from 1.
    pub current: u32,
    /// Background layout has finished: `count` is the document's page count.
    pub complete: bool,
}

/// The longest selection, in characters, whose text the state carries.
pub const SELECTION_TEXT_LIMIT: usize = 10_000;

/// The most ranges `selection.ranges` lists.
pub const SELECTION_RANGES_LIMIT: usize = 1_000;

/// Which rulers show.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RulerVisibility {
    /// The horizontal ruler shows.
    pub horizontal: bool,
    /// The vertical ruler shows.
    pub vertical: bool,
}

/// The view settings.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ViewState {
    /// Percent, after any fit.
    pub zoom: f64,
    /// Formatting marks are shown.
    pub formatting_marks: bool,
    /// Table gridlines are shown.
    pub gridlines: bool,
    /// Text boundaries are shown.
    pub text_boundaries: bool,
    /// Text wraps around an object again as it is dragged (otherwise a preview shows where it goes, and the text
    /// wraps on the release).
    pub live_layout: bool,
    /// Field codes are shown.
    pub field_codes: bool,
    /// Which rulers show.
    pub rulers: RulerVisibility,
    /// The unit the rulers count in.
    pub ruler_unit: RulerUnit,
    /// What a click on the ruler adds.
    pub tab_selector: TabSelector,
    /// The room beside the pages for balloons, CSS px (0 by default).
    pub markup_pane_width: f64,
    /// Around the pages (the host's colour, or the theme's desk).
    pub background: Color,
    /// The chrome's colour scheme.
    pub theme: super::services::ChromeTheme,
    /// The host's colours for the light and dark schemes.
    pub theme_colors: super::services::ThemeColors,
    /// The chrome's accent as drawn (the dark scheme's is lightened): for popovers and other host chrome that matches
    /// the editor's.
    pub accent: Color,
    /// Text and glyphs drawn on the accent.
    pub accent_on: Color,
    /// How the page is drawn on screen.
    pub appearance: super::services::DocumentAppearance,
    /// How much the editor measures while you drag and hover.
    pub detail: super::services::MeasureDetail,
    /// When the objects' dot grid shows.
    pub object_grid: super::services::ObjectGrid,
    /// Content controls and shown sub-templates are labelled while selected or hovered.
    pub control_labels: bool,
    /// When fields are shaded.
    pub field_shading: super::services::FieldShading,
    /// Bookmarks are shown.
    pub bookmarks: bool,
    /// Tab characters are shown (or every mark is).
    pub tab_characters: bool,
    /// Spaces are shown.
    pub spaces: bool,
    /// Paragraph marks and breaks are shown.
    pub paragraph_marks: bool,
    /// Hidden text is shown.
    pub hidden_text: bool,
    /// Optional hyphens are shown.
    pub optional_hyphens: bool,
    /// The selected floating object's anchor is shown.
    pub object_anchors: bool,
    /// Click and Type is on.
    pub click_and_type: bool,
    /// Selected text can be dragged and dropped.
    pub text_drag_and_drop: bool,
    /// The white space between pages is hidden. It opens as the document was saved.
    pub hide_white_space: bool,
    /// Under enforced protection, the regions the author may edit are highlighted.
    #[serde(default)]
    pub show_editable_regions: bool,
    /// The selected floating object's wrap points are being edited.
    pub editing_wrap_points: bool,
    /// Highlight All's highlights show (`find.highlightAll`); they go when the document changes.
    #[serde(default)]
    pub find_highlights: bool,
    /// The highlighter's colour while Highlight with nothing selected turned the pointer into one: it highlights the
    /// text it drags over. `null` when off.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub highlighter: Option<super::value::HighlightColor>,
    /// The pointer shapes shown over elements instead of the editor's own.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cursor_overrides: Vec<super::services::CursorOverride>,
    /// Which fields are sub-template references.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sub_template_rules: Vec<super::command::ReferenceRule>,
    /// Create Link's pointer is on: the next click on an empty text box links the selected one to it.
    #[serde(default)]
    pub linking_text_box: bool,
    /// Select Objects is on.
    #[serde(default)]
    pub selecting_objects: bool,
    /// The Select Objects rectangle being dragged; absent when none is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object_marquee: Option<PageBox>,
    /// The pointer's table tool.
    #[serde(default)]
    pub table_tool: super::command::TableTool,
    /// The line the Draw Table pen draws.
    #[serde(default = "default_pen")]
    pub table_pen: super::command::Border,
    /// What the Draw Table pen's stroke being dragged would draw, for the host to show dashed in the pen's colour;
    /// absent when no stroke is dragged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table_tool_feedback: Option<TableToolFeedback>,
    /// The border under the pointer the eraser would remove, for the host to highlight; absent when there is none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table_eraser_target: Option<PageSegment>,
}

fn default_pen() -> super::command::Border {
    super::command::Border {
        style: super::command::BorderStyle::Single,
        width: Points(0.5),
        color: AutoOr::Auto,
        space: Some(Points(0.0)),
        shadow: Some(false),
    }
}

/// A box on a page, in points from its top left corner.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PageBox {
    /// The page, from 0.
    pub page: u32,
    /// Left, top, right and bottom.
    pub rect: [Points; 4],
}

/// A line on a page, in points from its top left corner.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PageSegment {
    /// The page, from 0.
    pub page: u32,
    /// One end: across and down.
    pub from: [Points; 2],
    /// The other end.
    pub to: [Points; 2],
}

/// What a Draw Table pen stroke would draw.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TableStrokeKind {
    /// A line across or down a table, from `from` to `to`.
    #[default]
    Line,
    /// A cell's diagonal, from `from` to `to`.
    Diagonal,
    /// A new table's outline: the rectangle from `from` (top left) to `to` (bottom right).
    Outline,
}

/// The Draw Table pen's stroke being dragged: what it would draw, and where.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TableToolFeedback {
    /// The page, from 0.
    pub page: u32,
    /// What it would draw.
    pub kind: TableStrokeKind,
    /// Where: a line's or diagonal's start, an outline's top left corner (points).
    pub from: [Points; 2],
    /// A line's or diagonal's end, an outline's bottom right corner.
    pub to: [Points; 2],
}

impl Default for ViewState {
    fn default() -> ViewState {
        ViewState {
            zoom: 100.0,
            formatting_marks: false,
            gridlines: false,
            text_boundaries: false,
            live_layout: true,
            field_codes: false,
            rulers: RulerVisibility::default(),
            ruler_unit: RulerUnit::default(),
            tab_selector: TabSelector::default(),
            markup_pane_width: 0.0,
            background: Color(0xE8E7E3),
            theme: super::services::ChromeTheme::Light,
            theme_colors: super::services::ThemeColors::default(),
            accent: Color(0x0A74E4),
            accent_on: Color(0xFFFFFF),
            appearance: super::services::DocumentAppearance::Light,
            detail: super::services::MeasureDetail::Standard,
            object_grid: super::services::ObjectGrid::WhileDragging,
            control_labels: true,
            field_shading: super::services::FieldShading::WhenSelected,
            bookmarks: false,
            tab_characters: false,
            spaces: false,
            paragraph_marks: false,
            hidden_text: false,
            optional_hyphens: false,
            object_anchors: true,
            click_and_type: true,
            text_drag_and_drop: true,
            hide_white_space: false,
            show_editable_regions: true,
            editing_wrap_points: false,
            find_highlights: false,
            highlighter: None,
            cursor_overrides: Vec::new(),
            sub_template_rules: Vec::new(),
            linking_text_box: false,
            selecting_objects: false,
            object_marquee: None,
            table_tool: super::command::TableTool::None,
            table_pen: default_pen(),
            table_tool_feedback: None,
            table_eraser_target: None,
        }
    }
}

/// Track changes and the review counts.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewState {
    /// Changes are tracked.
    pub tracking: bool,
    /// Document protection keeps tracking on.
    pub tracking_locked: bool,
    /// What the document's protection allows, when it has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protection: Option<ProtectionState>,
    /// How revisions and comments show.
    pub markup_view: MarkupView,
    /// The number of revisions.
    pub revisions: u32,
    /// The number of comment threads.
    pub comments: u32,
    /// The active comment thread, or `None`.
    pub active_comment: Option<CommentId>,
    /// Which kinds of markup show, and whose. It opens as the document was saved.
    #[serde(default)]
    pub show_markup: super::review::ShowMarkup,
    /// Where revisions show in All Markup.
    #[serde(default)]
    pub balloon_mode: super::review::BalloonMode,
    /// How revisions draw.
    #[serde(default)]
    pub markup_style: super::review::MarkupStyle,
    /// Tracking records moves as moves.
    #[serde(default)]
    pub track_moves: bool,
    /// Tracking records formatting changes.
    #[serde(default)]
    pub track_formatting: bool,
    /// The revisions and threads by kind: all, and those Show Markup shows.
    #[serde(default)]
    pub counts: super::review::ReviewCounts,
}

/// A document's protection (Restrict Editing).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProtectionState {
    /// What it allows.
    pub kind: super::command::ProtectionKind,
    /// It is enforced.
    pub enforced: bool,
    /// Formatting is limited to the styles that aren't locked.
    pub formatting: bool,
    /// Stopping it needs a password.
    pub password: bool,
    /// The selection is in a range the author may edit.
    pub in_editable_range: bool,
    /// Filling in forms: the sections (by index) anyone may edit.
    pub unprotected_sections: Vec<u32>,
    /// The editing restriction takes exceptions: editable ranges and who may edit them (read only, comments).
    #[serde(default)]
    pub exceptions_allowed: bool,
    /// The editing restriction leaves chosen sections open (filling in forms).
    #[serde(default)]
    pub sections_allowed: bool,
    /// AutoFormat as you type may override the formatting restriction.
    #[serde(default)]
    pub auto_format_override: bool,
    /// The formatting restriction keeps the theme from changing.
    #[serde(default)]
    pub block_theme_switching: bool,
    /// The formatting restriction keeps the style set from changing.
    #[serde(default)]
    pub block_style_set_switching: bool,
    /// Who may edit the editable ranges holding the selection.
    #[serde(default)]
    pub editors_here: Vec<super::value::RangeEditorRef>,
    /// The number of editable ranges (permissions over one stretch count once).
    #[serde(default)]
    pub range_count: u32,
}

/// Spell checking.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SpellingState {
    /// Spell checking is on.
    pub enabled: bool,
    /// The language for text with none (BCP 47).
    pub language: String,
    /// The number of misspelled words.
    pub misspellings: u32,
}

impl Default for SpellingState {
    fn default() -> SpellingState {
        SpellingState { enabled: true, language: "en-US".into(), misspellings: 0 }
    }
}

/// The document's fonts the library can't draw as the document asks: per family and style, those a resolver is
/// fetching and those nothing provided, and the characters no font has. Each list is sorted by family, then style.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FontsState {
    /// Families a resolver is fetching; they draw with a stand-in meanwhile.
    pub pending: Vec<MissingFont>,
    /// Families nothing provided; they draw with a stand-in.
    pub missing: Vec<MissingFont>,
    /// Characters that neither their run's font, the character-fallback families nor the symbol fallback has; they
    /// draw as their font's missing-glyph box.
    pub characters: Vec<MissingCharacters>,
}

/// A family, in one style, that draws with a stand-in.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MissingFont {
    /// The family as the document names it, or the replacement target for [`StandIn::Original`].
    pub family: String,
    /// The bold style.
    pub bold: bool,
    /// The italic style.
    pub italic: bool,
    /// The replacement target and alias targets tried, in order.
    #[serde(default)]
    pub tried: Vec<String>,
    /// The generic category the family has, from the document's font table, an alias or a resolver.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<GenericFamily>,
    /// The family that draws instead; `None` when nothing draws (an empty library with no symbol fallback).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drawn_with: Option<String>,
    /// Why that family draws.
    pub by: StandIn,
    /// The resolver's failure, when it failed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Which stand-in draws a missing family.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StandIn {
    /// A replacement's target is missing, so the document's own family draws.
    Original,
    /// The default family of its generic category.
    Generic,
    /// The general fallback family.
    Fallback,
    /// The first face added to the library (or the symbol fallback, in an empty library).
    LastResort,
}

/// Characters no font has, by the family their runs name.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MissingCharacters {
    /// The family the runs name.
    pub family: String,
    /// The distinct characters, in code point order, at most 64.
    pub characters: String,
    /// More than 64 characters are missing.
    pub truncated: bool,
}

/// The session settings a state build reports.
pub(crate) struct Flags<'a> {
    pub sequence: u64,
    pub mode: Mode,
    pub read_only: bool,
    pub language: &'a str,
    /// Zoom as a factor (1 is 100%).
    pub zoom: f64,
    pub gridlines: bool,
    /// The session's fonts, for laying out through the caret after a relayout no edit laid out.
    pub fonts: &'a crate::font::Fonts,
}

/// The state of `editor` now: one format state pass, and the command states from it.
// shortcut: the revision count, misspellings and fonts stay at their defaults until their services report them.
fn text_point((para, offset): (crate::layout::ParaId, u32)) -> TextPoint {
    TextPoint { paragraph: super::value::ParagraphId(para.0.to_string()), offset }
}

/// The selection's ranges, at most [`SELECTION_RANGES_LIMIT`], and whether there are more.
fn selection_ranges(editor: &mut crate::editing::Editor) -> (Vec<TextRange>, bool) {
    let (points, whole) = editor.selection_text_ranges(SELECTION_RANGES_LIMIT);
    (points.into_iter().map(|(a, b)| TextRange { start: text_point(a), end: text_point(b) }).collect(), !whole)
}

/// A discontiguous selection's blocks of cells.
fn cell_blocks(editor: &crate::editing::Editor) -> Vec<CellBlock> {
    use crate::editing::selection::Selection;
    if !editor.is_discontiguous() {
        return Vec::new();
    }
    editor
        .cell_blocks()
        .iter()
        .filter_map(|s| {
            let Selection::Cells { anchor, focus, .. } = *s else { return None };
            let start = editor.cells_start(s).and_then(|p| editor.text_point(&p))?;
            let n = |v: usize| u32::try_from(v).unwrap_or(u32::MAX);
            Some(CellBlock {
                start: text_point(start),
                first_row: n(anchor.row.min(focus.row)),
                first_column: n(anchor.col.min(focus.col)),
                last_row: n(anchor.row.max(focus.row)),
                last_column: n(anchor.col.max(focus.col)),
            })
        })
        .collect()
}

pub(crate) fn build(editor: &mut crate::editing::Editor, flags: Flags<'_>) -> State {
    use crate::editing::position::StoryId;
    use crate::editing::selection::Selection;

    editor.ensure_current(flags.fonts);
    let format = editor.format_state();
    let commands = editor.command_states(format.as_ref(), crate::editing::state::StateFlags { read_only: flags.read_only });
    let history = editor.history();
    let history = HistoryState {
        can_undo: history.undo_depth() > 0,
        can_redo: history.redo_depth() > 0,
        undo_label: history.next_undo().map(str::to_string),
        redo_label: history.next_redo().map(str::to_string),
    };
    let object = editor.object_selection().map(|mut o| {
        editor.add_object_props(&mut o, flags.fonts);
        o
    });
    let pages = pages(editor);
    let current = editor.caret_view().map_or(1, |c| c.page as u32 + 1);
    let selection = editor.selection();
    let context = editor.element_context();
    let kind = match selection {
        Selection::Cells { .. } => SelectionKind::Cells,
        Selection::Object(_) => SelectionKind::Object,
        _ if context.first().is_some_and(|c| c.exact) => SelectionKind::Element,
        s if s.is_collapsed() => SelectionKind::Caret,
        _ => SelectionKind::Text,
    };
    let story = match format.as_ref().map_or(StoryId::Body, |f| f.story) {
        StoryId::Body => Story::Body,
        StoryId::HeaderFooter(_) => match editor.header_footer_info(current) {
            Some(h) if h.kind == HeaderFooterKind::Footer => Story::Footer,
            _ => Story::Header,
        },
        StoryId::Footnote(_) => Story::Footnote,
        StoryId::Endnote(_) => Story::Endnote,
        StoryId::TextBox(_) => Story::TextBox,
        StoryId::BuildingBlock(_) => Story::SubTemplate,
    };
    let mode = match commands.get(&CommandId::SetSelectionMode).and_then(|s| s.value.clone()) {
        Some(CommandValue::SelectionMode(m)) => m,
        _ => SelectionMode::Normal,
    };
    let comment = editor.active_comment().map(crate::api::record::comment_id);
    let (ranges, ranges_truncated) = selection_ranges(editor);
    let summary = SelectionSummary {
        kind,
        story,
        page: current,
        mode,
        empty: selection.is_collapsed(),
        text: editor.selected_text_within(SELECTION_TEXT_LIMIT),
        ranges,
        ranges_truncated,
        cell_blocks: cell_blocks(editor),
        table: editor.table_selection(),
        object,
        context,
        link: editor.link_info(),
        comment: comment.clone(),
        revision: editor.revision_here().map(crate::api::record::revision_id),
        header_footer: editor.header_footer_info(current),
        pending_move: editor.move_to_pending().map(|m| PendingMove { copy: m.copy, placed: m.target.is_some() }),
        list_numbers: editor.list_label_selection().map(|l| ListNumbers { list: l.list, level: l.level.clamp(0, 8) as u8 }),
    };
    let doc = editor.document();
    let review = ReviewState {
        tracking: editor.tracking(),
        tracking_locked: doc.settings.protection.as_ref().is_some_and(|p| p.forces_tracking()),
        protection: None,
        markup_view: editor.review().view,
        revisions: 0,
        comments: doc.comments.by_id.len() as u32,
        active_comment: comment,
        ..ReviewState::default()
    };
    let page = format.as_ref().map_or_else(PageFormatting::default, |f| editor.page_formatting(f.section));
    let ruler = format.as_ref().map_or_else(RulerModel::default, |f| crate::services::ruler::model(editor, f.section));
    let (formatting, paragraph) = match format {
        Some(f) => (f.character, f.paragraph),
        None => (CharacterFormatting::default(), ParagraphFormatting::default()),
    };
    let mut state = State {
        revision: editor.revision(),
        sequence: flags.sequence,
        load: if pages.1 { LoadState::Ready } else { LoadState::LayingOut },
        mode: flags.mode,
        read_only: flags.read_only,
        // shortcut: changed since the last open or save, undo included; undoing back to the saved point clears it
        // once history positions are tracked.
        modified: editor.is_dirty(),
        selection: summary,
        formatting,
        paragraph,
        page,
        commands,
        history,
        pages: PagesState { count: pages.0, current, complete: pages.1 },
        view: ViewState { zoom: (flags.zoom * 1000.0).round() / 10.0, gridlines: flags.gridlines, ..ViewState::default() },
        review,
        ruler,
        spelling: SpellingState { language: flags.language.to_string(), ..SpellingState::default() },
        ..State::default()
    };
    if let Some(b) = crate::editing::readback::BorderLine::of(editor.table_pen()) {
        state.view.table_pen = b.into();
    }
    (state.view.object_marquee, state.view.table_tool_feedback, state.view.table_eraser_target) = pointer_feedback(editor, flags.fonts);
    state.view.table_tool = editor.table_tool();
    let reach = crate::services::dialogs::Reach { list: editor.selection_list().is_some(), frame: editor.frame_here().is_some() };
    state.dialogs = crate::services::dialogs::availability(&state, reach);
    state
}

/// What a pointer gesture under way shows with no change to the document: the Select Objects rectangle, the Draw
/// Table pen's stroke and the border the eraser is over.
pub(crate) fn pointer_feedback(
    editor: &mut crate::editing::Editor,
    fonts: &crate::font::Fonts,
) -> (Option<PageBox>, Option<TableToolFeedback>, Option<PageSegment>) {
    let pts = |v: [f64; 2]| v.map(Points);
    let page = |p: usize| u32::try_from(p).unwrap_or(u32::MAX);
    let marquee = editor.marquee_rect().map(|(p, r)| PageBox { page: page(p), rect: r.map(Points) });
    let stroke = editor.table_tool_stroke(fonts).map(|(p, kind, from, to)| {
        use crate::editing::table_draw::StrokeKind as K;
        let kind = match kind {
            K::Line => TableStrokeKind::Line,
            K::Diagonal => TableStrokeKind::Diagonal,
            K::Outline => TableStrokeKind::Outline,
        };
        TableToolFeedback { page: page(p), kind, from: pts(from), to: pts(to) }
    });
    let eraser = editor.table_eraser_hover(fonts).map(|(p, from, to)| PageSegment { page: page(p), from: pts(from), to: pts(to) });
    (marquee, stroke, eraser)
}

/// The pages laid out so far, and whether that is all of them.
pub(crate) fn pages(editor: &crate::editing::Editor) -> (u32, bool) {
    (editor.snapshot().pages().len() as u32, editor.layout_complete())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tri_is_a_value_mixed_or_null() {
        let forms: [(Tri<bool>, &str); 3] = [(Tri::Uniform(true), "true"), (Tri::Mixed, "\"mixed\""), (Tri::Unavailable, "null")];
        for (tri, json) in forms {
            assert_eq!(serde_json::to_string(&tri).ok().as_deref(), Some(json));
            assert_eq!(serde_json::from_str::<Tri<bool>>(json).ok(), Some(tri));
        }
        assert!(serde_json::from_str::<Tri<bool>>("\"maybe\"").is_err());
    }
}
