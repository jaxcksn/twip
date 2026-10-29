//! The state snapshot: what an application's controls show, with mixed values explicit. A session rebuilds it after
//! every observable change; `sequence` counts those, `revision` counts document changes.

use std::collections::BTreeMap;

use serde::de::{DeserializeOwned, Error as _};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::command::{CommandId, SelectionMode};
use super::services::{DialogKind, RulerModel, RulerUnit, TabSelector};
use super::value::{
    Alignment, AutoOr, CellAlignment, Color, CommentId, ElementId, HighlightColor, HorizontalAlignment, LineSpacing, NoneOr, ObjectWrap, Orientation,
    Points, RevisionId, RowHeight, Story, StyleId, TableWidth, UnderlineStyle,
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
}

/// An element the selection is in or covers.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ElementContext {
    /// The element's id.
    pub id: ElementId,
    /// What kind of element it is.
    pub kind: ElementKind,
    /// The selection is exactly the element.
    pub exact: bool,
    /// A field's code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// A control's tag.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    /// A control's title.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

/// The link at the selection.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LinkInfo {
    /// Where the link goes.
    pub address: String,
    /// The link's display text.
    pub text: String,
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
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ColumnLayout {
    /// The number of columns.
    pub count: u32,
    /// The space between columns.
    pub spacing: Points,
    /// A line is drawn between columns.
    pub separator: bool,
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
    /// Around the pages; the paper stays white.
    pub background: Color,
}

impl Default for ViewState {
    fn default() -> ViewState {
        ViewState {
            zoom: 100.0,
            formatting_marks: false,
            gridlines: false,
            text_boundaries: false,
            field_codes: false,
            rulers: RulerVisibility::default(),
            ruler_unit: RulerUnit::default(),
            tab_selector: TabSelector::default(),
            markup_pane_width: 0.0,
            background: Color(0xE6E6E6),
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
    /// How revisions and comments show.
    pub markup_view: MarkupView,
    /// The number of revisions.
    pub revisions: u32,
    /// The number of comment threads.
    pub comments: u32,
    /// The active comment thread, or `None`.
    pub active_comment: Option<CommentId>,
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

/// Font families being fetched, and those no font could be found for.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FontsState {
    /// Families being fetched.
    pub pending: Vec<String>,
    /// Families no font was found for.
    pub unresolved: Vec<String>,
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
// shortcut: the revision at the caret, the revision count, misspellings and fonts stay at their defaults until their
// services report them.
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
    };
    let mode = match commands.get(&CommandId::SetSelectionMode).and_then(|s| s.value.clone()) {
        Some(CommandValue::SelectionMode(m)) => m,
        _ => SelectionMode::Normal,
    };
    let comment = editor.active_comment().map(crate::api::record::comment_id);
    let summary = SelectionSummary {
        kind,
        story,
        page: current,
        mode,
        empty: selection.is_collapsed(),
        text: editor.selected_text_within(SELECTION_TEXT_LIMIT),
        table: editor.table_selection(),
        object: editor.object_selection(),
        context,
        link: editor.link_info(),
        comment: comment.clone(),
        revision: None,
        header_footer: editor.header_footer_info(current),
    };
    let doc = editor.document();
    let review = ReviewState {
        tracking: editor.tracking(),
        tracking_locked: doc.settings.protection.as_ref().is_some_and(|p| p.forces_tracking()),
        markup_view: doc.review.view,
        revisions: 0,
        comments: doc.comments.by_id.len() as u32,
        active_comment: comment,
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
        load: LoadState::Ready,
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
    state.dialogs = crate::services::dialogs::availability(&state);
    state
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
