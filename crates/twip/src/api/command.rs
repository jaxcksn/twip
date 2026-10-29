//! Commands: one typed variant per operation an application can run, applied at the selection.
//!
//! On the wire a command is an object with the command id as `type`. A command with one argument carries it in
//! `value` (`{"type":"setBold","value":true}`), and a command with an options object carries its fields beside `type`
//! (`{"type":"insertTable","rows":3,"columns":2}`). Unset optional fields keep their current value.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::review::{Body, MarkupView};
use super::state::Mode;
use super::value::{
    Alignment, AutoOr, Bytes, CellAlignment, Color, CommentId, ControlKind, ControlLock, ControlOptions, DataBinding, Direction, ElementId,
    HighlightColor, HorizontalAlignment, LineSpacing, ObjectWrap, Order, Orientation, Points, RevisionId, RowHeight, StyleId, TabAlignment,
    TabLeader, TableWidth, UnderlineStyle,
};
use super::wire::value;

macro_rules! commands {
    ($($(#[doc = $doc:literal])* $name:ident $(($(#[$field:meta])* $arg:ty))?,)*) => {
        /// A command. Each variant's documentation states its undo grouping: one step, typing (consecutive typing in
        /// one paragraph joins one step), or view (no undo step and no revision).
        #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
        #[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
        #[non_exhaustive]
        pub enum Command {
            $($(#[doc = $doc])* $name $(($(#[$field])* $arg))?,)*
        }

        /// A command's id: its catalog id, its key in the state's command map, and its op's `type`, all one spelling
        /// (`setBold`).
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(rename_all = "camelCase")]
        #[non_exhaustive]
        pub enum CommandId {
            $($(#[doc = $doc])* $name,)*
        }

        impl Command {
            /// This command's id.
            pub fn id(&self) -> CommandId {
                match self {
                    $(Command::$name { .. } => CommandId::$name,)*
                }
            }
        }

        impl CommandId {
            /// Every command id, in the order of the command set.
            pub const ALL: &'static [CommandId] = &[$(CommandId::$name,)*];
        }
    };
}

commands! {
    // Text
    /// Types `value` (non-empty) at the selection, replacing it. Undo: typing.
    InsertText(#[serde(with = "value")] String),
    /// Starts a new paragraph (Enter). Undo: one step.
    InsertParagraph,
    /// A line break (Shift+Enter). Undo: one step.
    InsertLineBreak,
    /// A page break, in the body outside tables. Undo: one step.
    InsertPageBreak,
    /// A column break, in the body outside tables. Undo: one step.
    InsertColumnBreak,
    /// A section break of the given kind, in the body outside tables. Undo: one step.
    InsertSectionBreak(#[serde(with = "value")] SectionBreakKind),
    /// A tab character. Undo: typing.
    InsertTab,
    /// One character, optionally in a symbol font. Undo: one step.
    InsertSymbol(InsertSymbol),
    /// Deletes backward or forward by a character or a word, or the selection. Undo: typing.
    Delete(DeleteOptions),

    // Clipboard and history
    /// Cuts the selection to the clipboard. Undo: one step.
    Cut,
    /// Copies the selection (also when read only). No undo step.
    Copy,
    /// Pastes the system clipboard, formatted or as text. Undo: one step.
    Paste(PasteOptions),
    /// Pastes `value` as plain text. Undo: one step.
    PasteText(#[serde(with = "value")] String),
    /// Undoes the last step. State value: the undo label.
    Undo,
    /// Redoes the last undone step. State value: the redo label.
    Redo,
    /// Redoes, or repeats the last repeatable action (F4). Undo: one step.
    Repeat,

    // Selection and navigation
    /// Selects the whole story. Undo: view.
    SelectAll,
    /// Selects the unit at the caret. Undo: view.
    Select(#[serde(with = "value")] SelectUnit),
    /// Moves or extends the selection. Undo: view.
    MoveSelection(MoveSelection),
    /// Moves to the next or previous cell; `next` from the last cell adds a row (then one step).
    MoveToCell(#[serde(with = "value")] Order),
    /// Normal, extend (F8) or block (Ctrl+Shift+F8) selection. Undo: view.
    SetSelectionMode(#[serde(with = "value")] SelectionMode),
    /// Back to an earlier edit location (Shift+F5). Undo: view.
    GoBack,
    /// Leaves a header or footer, a text box, or object selection (Esc). Undo: view.
    Escape,
    /// Selects the next or previous floating object. Undo: view.
    SelectNextObject(#[serde(with = "value")] Order),
    /// Moves the caret to the start of page `value` (1…count). Undo: view.
    GoToPage(#[serde(with = "value")] u32),

    // Character formatting
    /// Bold on or off. Undo: one step.
    SetBold(#[serde(with = "value")] bool),
    /// Italic on or off. Undo: one step.
    SetItalic(#[serde(with = "value")] bool),
    /// The underline style (`none` removes it). Undo: one step.
    SetUnderline(#[serde(with = "value")] UnderlineStyle),
    /// Strikethrough on or off. Undo: one step.
    SetStrikethrough(#[serde(with = "value")] bool),
    /// Double strikethrough on or off. Undo: one step.
    SetDoubleStrikethrough(#[serde(with = "value")] bool),
    /// Subscript on or off. Undo: one step.
    SetSubscript(#[serde(with = "value")] bool),
    /// Superscript on or off. Undo: one step.
    SetSuperscript(#[serde(with = "value")] bool),
    /// Small capitals on or off. Undo: one step.
    SetSmallCaps(#[serde(with = "value")] bool),
    /// All capitals on or off. Undo: one step.
    SetAllCaps(#[serde(with = "value")] bool),
    /// Hidden text on or off. Undo: one step.
    SetHidden(#[serde(with = "value")] bool),
    /// The font family (non-empty). Undo: one step.
    SetFont(#[serde(with = "value")] String),
    /// The font size (1…1638 pt, in 0.5 steps). Undo: one step.
    SetFontSize(#[serde(with = "value")] Points),
    /// The next size up in the standard list. Undo: one step.
    GrowFont,
    /// The next size down in the standard list. Undo: one step.
    ShrinkFont,
    /// The font colour, or automatic. Undo: one step.
    SetFontColor(#[serde(with = "value")] AutoOr<Color>),
    /// The highlight colour, or none (`null`). Undo: one step.
    SetHighlight(#[serde(with = "value")] Option<HighlightColor>),
    /// Character spacing, expanded when positive (−1584…1584 pt). Undo: one step.
    SetCharacterSpacing(#[serde(with = "value")] Points),
    /// The raised (positive) or lowered position (−1584…1584 pt). Undo: one step.
    SetCharacterPosition(#[serde(with = "value")] Points),
    /// Removes direct character formatting. Undo: one step.
    ClearFormatting,
    /// Changes the case of the selected text. Undo: one step.
    ChangeCase(#[serde(with = "value")] CaseChange),
    /// Holds the selection's formatting for `pasteFormatting` (the format painter). Undo: view.
    CopyFormatting,
    /// Applies the held formatting to the selection. Undo: one step.
    PasteFormatting,

    // Paragraph formatting
    /// The selected paragraphs' alignment. Undo: one step.
    SetAlignment(#[serde(with = "value")] Alignment),
    /// The selected paragraphs' line spacing. Undo: one step.
    SetLineSpacing(LineSpacing),
    /// Space before and after the selected paragraphs (0…1584 pt, or auto). Undo: one step.
    SetParagraphSpacing(ParagraphSpacing),
    /// The selected paragraphs' indents. Undo: one step.
    SetIndents(Indents),
    /// Indents the selected paragraphs to the next stop. Undo: one step.
    IncreaseIndent,
    /// Outdents the selected paragraphs to the previous stop. Undo: one step.
    DecreaseIndent,
    /// An existing paragraph style. Undo: one step.
    SetParagraphStyle(#[serde(with = "value")] StyleId),
    /// Bullets, numbering, or no list (`null`). Undo: one step.
    SetList(#[serde(with = "value")] Option<ListKind>),
    /// One list level deeper or shallower, in a list. Undo: one step.
    SetListLevel(#[serde(with = "value")] LevelChange),
    /// Keep with next, keep lines together, page break before and widow control. Undo: one step.
    SetParagraphFlow(ParagraphFlow),
    /// A box, a bottom border or none (a ½ pt single line). Undo: one step.
    SetParagraphBorders(#[serde(with = "value")] BorderPreset),
    /// Paragraph shading, or none (`null`). Undo: one step.
    SetParagraphShading(#[serde(with = "value")] Option<Color>),

    // Tab stops
    /// Adds a tab stop to the selected paragraphs, replacing one at the same position. Undo: one step.
    AddTabStop(AddTabStop),
    /// Moves the stop at `from` (within 0.5 pt) to `to`. Undo: one step.
    MoveTabStop(MoveTabStop),
    /// Deletes the stop at `value`; an inherited stop is cleared in the paragraph. Undo: one step.
    DeleteTabStop(#[serde(with = "value")] Points),
    /// Clears the paragraphs' own and inherited stops. Undo: one step.
    ClearTabStops,
    /// The document's default tab spacing (1…1584 pt). Undo: one step.
    SetDefaultTabStop(#[serde(with = "value")] Points),

    // Page layout
    /// The selection's sections' margins (0…1584 pt; left + right + gutter < page width). Undo: one step.
    SetMargins(Margins),
    /// Portrait or landscape. Undo: one step.
    SetOrientation(#[serde(with = "value")] Orientation),
    /// The paper size, in portrait (36…1584 pt). Undo: one step.
    SetPaperSize(PaperSize),
    /// Text columns (1…45), their spacing and separator. Undo: one step.
    SetColumns(Columns),
    /// The header and footer distances from the page edge. Undo: one step.
    SetHeaderFooterDistance(HeaderFooterDistance),
    /// A different first page header and footer. Undo: one step.
    SetDifferentFirstPage(#[serde(with = "value")] bool),
    /// Different odd and even page headers and footers. Undo: one step.
    SetDifferentOddEvenPages(#[serde(with = "value")] bool),
    /// Moves a text column boundary (from the ruler). Undo: one step.
    MoveColumnBoundary(ColumnBoundaryMove),

    // Headers and footers
    /// Edits the caret's page's header, creating it when the page has none. Undo: one step when it creates one.
    EditHeader,
    /// Edits the caret's page's footer, creating it when the page has none. Undo: one step when it creates one.
    EditFooter,
    /// Back to the body. Undo: view.
    CloseHeaderFooter,
    /// A page number field at the caret, or in the header or footer. Undo: one step.
    InsertPageNumber(PageNumber),

    // Insert
    /// A table (1…32767 rows, 1…63 columns). Undo: one step.
    InsertTable(TableSize),
    /// A picture (PNG, JPEG, GIF, BMP, TIFF). Undo: one step.
    InsertPicture(Picture),
    /// A text box. Undo: one step.
    InsertTextBox,
    /// A footnote at the caret. Undo: one step.
    InsertFootnote,
    /// An endnote at the caret. Undo: one step.
    InsertEndnote,
    /// A hyperlink to `address`; `text` replaces the selection (default: the selection, or the address). Undo: one step.
    InsertHyperlink(Hyperlink),
    /// Changes the link at the selection. Undo: one step.
    EditHyperlink(HyperlinkEdit),
    /// Removes the link at the selection, keeping its text. Undo: one step.
    RemoveHyperlink,
    /// A field with `code`; result `{ id }`. With a selection and an empty code, wraps it. Undo: one step.
    InsertField(InsertField),

    // Tables
    /// Rows above or below the selection (1…32767). Undo: one step.
    InsertRows(InsertRows),
    /// Columns left or right of the selection (1…63). Undo: one step.
    InsertColumns(InsertColumns),
    /// Deletes the selected rows. Undo: one step.
    DeleteRows,
    /// Deletes the selected columns. Undo: one step.
    DeleteColumns,
    /// Deletes the table. Undo: one step.
    DeleteTable,
    /// Deletes the selected cells, shifting the rest up or left. Undo: one step.
    DeleteCells(#[serde(with = "value")] CellShift),
    /// Merges a rectangular block of cells. Undo: one step.
    MergeCells,
    /// Splits the selected cells (1…63 columns, 1…32767 rows). Undo: one step.
    SplitCells(SplitCells),
    /// Splits the table above the selection. Undo: one step.
    SplitTable,
    /// Gives the selected rows equal heights. Undo: one step.
    DistributeRows,
    /// Gives the selected columns equal widths. Undo: one step.
    DistributeColumns,
    /// Moves the selected rows up or down. Undo: one step.
    MoveRows(#[serde(with = "value")] VerticalDirection),
    /// Fits the columns to their contents, the window, or fixed widths. Undo: one step.
    AutoFitColumns(#[serde(with = "value")] AutoFit),
    /// The selected cells' vertical alignment. Undo: one step.
    SetCellAlignment(#[serde(with = "value")] CellAlignment),
    /// Cell shading, or none (`null`). Undo: one step.
    SetCellShading(#[serde(with = "value")] Option<Color>),
    /// Borders on the selected cells' edges. Undo: one step.
    SetCellBorders(CellBorders),
    /// The selected cells' margins (0…1584 pt). Undo: one step.
    SetCellMargins(CellMargins),
    /// Repeat the selected rows as header rows. Undo: one step.
    SetHeaderRow(#[serde(with = "value")] bool),
    /// The selected rows' height, or automatic. Undo: one step.
    SetRowHeight(#[serde(with = "value")] AutoOr<RowHeight>),
    /// Whether the selected rows can break across pages. Undo: one step.
    SetRowCanBreak(#[serde(with = "value")] bool),
    /// The selected columns' width (1…1584 pt). Undo: one step.
    SetColumnWidth(#[serde(with = "value")] Points),
    /// Moves a column boundary of the caret's row (from the rulers and cell edges). Undo: one step.
    MoveTableColumnBoundary(BoundaryMove),
    /// Moves a row boundary of the caret's table. Undo: one step.
    MoveTableRowBoundary(BoundaryMove),
    /// The table's preferred width. Undo: one step.
    SetTableWidth(TableWidth),
    /// The table's alignment on the page. Undo: one step.
    SetTableAlignment(#[serde(with = "value")] HorizontalAlignment),
    /// The table's indent (−1584…1584 pt). Undo: one step.
    SetTableIndent(#[serde(with = "value")] Points),
    /// An existing table style. Undo: one step.
    SetTableStyle(#[serde(with = "value")] StyleId),
    /// Which parts of the table style apply. Undo: one step.
    SetTableStyleOptions(TableStyleChange),

    // Pictures and text boxes
    /// The selected object's size (1…1584 pt). Undo: one step.
    SetObjectSize(ObjectSize),
    /// The selected picture at its original size. Undo: one step.
    ResetObjectSize,
    /// How text wraps around the selected object. Undo: one step.
    SetObjectWrap(#[serde(with = "value")] ObjectWrap),
    /// Locks a floating object's anchor. Undo: one step.
    SetAnchorLocked(#[serde(with = "value")] bool),
    /// The selected object's alternative text. Undo: one step.
    SetAltText(AltText),
    /// Deletes the selected object. Undo: one step.
    DeleteObject,

    // Fields
    /// Changes a field's code or result. Undo: one step.
    SetField(SetField),
    /// The results the application computed; `token` joins the update a `fieldsUpdateRequested` started. Undo: one step.
    SetFieldResults(FieldResults),
    /// Updates fields (default: the selection's), asking the application for the ones it computes. Undo: one step.
    UpdateFields(ElementIds),
    /// Locks or unlocks fields. Undo: one step.
    SetFieldsLocked(FieldsLocked),
    /// Replaces fields with their results. Undo: one step.
    UnlinkFields(ElementIds),
    /// Shows field codes or results. Undo: view.
    ShowFieldCodes(ShowFieldCodes),
    /// What design mode shows for each field code. Undo: view.
    SetFieldLabels(FieldLabels),

    // Content controls and regions
    /// A content control at the selection; result `{ id }`. Undo: one step.
    InsertControl(InsertControl),
    /// Changes a content control's properties. Undo: one step.
    SetControl(SetControl),
    /// Removes a content control, keeping or deleting its content. Undo: one step.
    RemoveControl(RemoveControl),
    /// Fills a control with exactly one of text, an image or a document. Undo: one step.
    FillControl(FillControl),
    /// Checks or clears a checkbox. Undo: one step.
    SetCheckbox(SetCheckbox),
    /// Wraps the selection in a repeating section; result `{ id }`. Undo: one step.
    WrapInRepeatingSection(RepeatingSection),
    /// Removes a field, or a control with its content. Undo: one step.
    RemoveElement(#[serde(with = "value")] ElementId),
    /// Replaces text in element codes, tags or titles, case-sensitive; result `{ changed }`. Undo: one step.
    ReplaceInElements(ReplaceInElements),
    /// Wavy marks with a tooltip on elements. Undo: view.
    MarkElements(MarkElements),
    /// How unfilled picture placeholders look. Undo: view.
    SetPlaceholderLooks(PlaceholderLooks),
    /// Fills the selected placeholder, or every unfilled one with `key`. Undo: one step.
    FillPlaceholder(FillPlaceholder),

    // Data parts
    /// Writes a custom XML data part (well-formed); result `{ itemId }`. Undo: one step.
    WriteDataPart(WriteDataPart),
    /// Removes a custom XML data part. Undo: one step.
    RemoveDataPart(DataPartRef),

    // Review
    /// Track changes on or off. Undo: one step.
    SetTrackChanges(#[serde(with = "value")] bool),
    /// How revisions show. Undo: view.
    SetMarkupView(#[serde(with = "value")] MarkupView),
    /// Accepts a revision (a move as a pair). Undo: one step.
    AcceptRevision(#[serde(with = "value")] RevisionId),
    /// Rejects a revision (a move as a pair). Undo: one step.
    RejectRevision(#[serde(with = "value")] RevisionId),
    /// Accepts the revisions in the selection. Undo: one step.
    AcceptRevisionsInSelection,
    /// Rejects the revisions in the selection. Undo: one step.
    RejectRevisionsInSelection,
    /// Accepts every revision, or one author's. Undo: one step.
    AcceptAllRevisions(AuthorFilter),
    /// Rejects every revision, or one author's. Undo: one step.
    RejectAllRevisions(AuthorFilter),
    /// Selects the next or previous revision. Undo: view.
    SelectRevision(#[serde(with = "value")] Order),
    /// A comment on the selection, or the word at the caret; result `{ id }`. Undo: one step.
    AddComment(#[serde(with = "value")] Body),
    /// A reply in a thread; result `{ id }`. Undo: one step.
    ReplyToComment(CommentReply),
    /// Changes a comment's body. Undo: one step.
    EditComment(CommentEdit),
    /// Deletes a reply, or the whole thread when it's the first comment. Undo: one step.
    DeleteComment(#[serde(with = "value")] CommentId),
    /// Marks a thread done or open. Undo: one step.
    ResolveComment(CommentResolve),
    /// The thread the application shows as active, or none. Undo: view.
    SetActiveComment(#[serde(with = "value")] Option<CommentId>),
    /// Selects and reveals a comment's range. Undo: view.
    SelectComment(#[serde(with = "value")] CommentId),
    /// The colour an author's revisions and comments show in. Undo: view.
    SetAuthorColor(AuthorColor),

    // View
    /// The zoom (10…500 percent), or a fit. Undo: view.
    SetZoom(#[serde(with = "value")] Zoom),
    /// Formatting marks, gridlines and text boundaries. Undo: view.
    SetViewOptions(ViewOptions),
    /// Editing or design mode. Undo: view.
    SetMode(#[serde(with = "value")] Mode),
}

/// The kinds of section break.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SectionBreakKind {
    /// Starts the new section on the next page.
    NextPage,
    /// Starts the new section on the same page.
    Continuous,
    /// Starts the new section on the next even page.
    EvenPage,
    /// Starts the new section on the next odd page.
    OddPage,
}

/// `insertSymbol`'s options.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InsertSymbol {
    /// The character to insert.
    pub char: char,
    /// The symbol font (Symbol, Wingdings); `None` keeps the font at the selection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font: Option<String>,
}

/// What `delete` removes when the selection is collapsed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DeleteUnit {
    /// One character (a grapheme).
    Character,
    /// To the word's start or end.
    Word,
}

/// `delete`'s options.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeleteOptions {
    /// Backward (Backspace) or forward (Delete).
    pub direction: Direction,
    /// Default: character.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<DeleteUnit>,
}

/// How `paste` takes the clipboard.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PasteAs {
    /// With its formatting.
    Formatted,
    /// As plain text, taking the formatting at the selection.
    Text,
}

/// `paste`'s options.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PasteOptions {
    /// Default: formatted.
    #[serde(rename = "as", default, skip_serializing_if = "Option::is_none")]
    pub as_: Option<PasteAs>,
}

/// What `select` selects at the caret.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SelectUnit {
    /// The word at the caret.
    Word,
    /// The sentence at the caret.
    Sentence,
    /// The paragraph at the caret.
    Paragraph,
    /// The table cell at the caret.
    Cell,
    /// The table row at the caret.
    Row,
    /// The table column at the caret.
    Column,
    /// The table at the caret.
    Table,
}

/// What `moveSelection` moves by.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MoveUnit {
    /// One character.
    Character,
    /// One word.
    Word,
    /// One line.
    Line,
    /// One paragraph.
    Paragraph,
    /// Home / End.
    LineEdge,
    /// The document's start or end (Ctrl+Home / Ctrl+End).
    Document,
    /// One screen (Page Up / Page Down).
    Screen,
    /// One page.
    Page,
    /// The first or last cell of the table row.
    RowEdge,
    /// The first or last cell of the table column.
    ColumnEdge,
}

/// `moveSelection`'s options.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MoveSelection {
    /// The unit to move by.
    pub by: MoveUnit,
    /// Which way to move.
    pub direction: Direction,
    /// Extends the selection instead of moving the caret. Default: false.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extend: Option<bool>,
}

/// How the selection grows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SelectionMode {
    /// Moving the caret collapses the selection.
    #[default]
    Normal,
    /// Moving the caret extends the selection (F8).
    Extend,
    /// Moving the caret extends a rectangular block selection (Ctrl+Shift+F8).
    Block,
}

/// `changeCase`'s target.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CaseChange {
    /// lowercase.
    Lower,
    /// UPPERCASE.
    Upper,
    /// Sentence case.
    Sentence,
    /// Capitalize Each Word.
    Title,
    /// tOGGLE cASE.
    Toggle,
}

/// `setParagraphSpacing`'s options.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ParagraphSpacing {
    /// Space before, or auto.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<AutoOr<Points>>,
    /// Space after, or auto.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<AutoOr<Points>>,
}

/// `setIndents`'s options (−1584…1584 pt; `firstLine` and `hanging` exclude each other, both ≥ 0).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Indents {
    /// The left indent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left: Option<Points>,
    /// The right indent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub right: Option<Points>,
    /// The first line's extra indent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_line: Option<Points>,
    /// The hanging indent of the lines after the first.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hanging: Option<Points>,
}

/// The kinds of list `setList` applies.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ListKind {
    /// A bulleted list.
    Bullet,
    /// A numbered list.
    Numbered,
}

/// One level deeper or shallower.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LevelChange {
    /// One level deeper.
    Increase,
    /// One level shallower.
    Decrease,
}

/// `setParagraphFlow`'s options.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ParagraphFlow {
    /// Keeps the paragraph on the same page as the next one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keep_with_next: Option<bool>,
    /// Keeps the paragraph's lines on one page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keep_lines_together: Option<bool>,
    /// Starts the paragraph on a new page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_break_before: Option<bool>,
    /// Prevents a single first or last line on a page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub widow_control: Option<bool>,
}

/// The paragraph border presets.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BorderPreset {
    /// A box around the paragraphs.
    Box,
    /// A line below the paragraphs.
    Bottom,
    /// No borders.
    None,
}

/// `addTabStop`'s options.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AddTabStop {
    /// From the paragraph's left text edge (0…1584 pt).
    pub position: Points,
    /// Default: left.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alignment: Option<TabAlignment>,
    /// Default: none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub leader: Option<TabLeader>,
}

/// `moveTabStop`'s options.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MoveTabStop {
    /// The stop's current position.
    pub from: Points,
    /// Its new position (0…1584 pt).
    pub to: Points,
}

/// `setMargins`'s options.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Margins {
    /// The top margin.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top: Option<Points>,
    /// The bottom margin.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bottom: Option<Points>,
    /// The left margin.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left: Option<Points>,
    /// The right margin.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub right: Option<Points>,
    /// The gutter, added on the binding side.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gutter: Option<Points>,
}

/// `setPaperSize`'s options.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PaperSize {
    /// The paper width.
    pub width: Points,
    /// The paper height.
    pub height: Points,
}

/// `setColumns`'s options.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Columns {
    /// The number of text columns (1…45).
    pub count: u32,
    /// The space between columns.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spacing: Option<Points>,
    /// Draws a line between columns.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub separator: Option<bool>,
}

/// `setHeaderFooterDistance`'s options.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HeaderFooterDistance {
    /// The header's distance from the top edge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub header: Option<Points>,
    /// The footer's distance from the bottom edge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub footer: Option<Points>,
}

/// `moveColumnBoundary`'s options: the boundary after text column `column` (from 0) moves to `to`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ColumnBoundaryMove {
    /// The text column whose right boundary moves, from 0.
    pub column: u32,
    /// The boundary's new position.
    pub to: Points,
}

/// A table boundary to move: column or row boundary `index` (from 0) moves to `to`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BoundaryMove {
    /// The boundary's index, from 0.
    pub index: u32,
    /// The boundary's new position.
    pub to: Points,
}

/// What a page number field shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PageNumberFormat {
    /// The page number.
    Page,
    /// "Page X of Y".
    PageOfTotal,
    /// The total page count.
    Total,
}

/// Where a page number goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PageNumberPlace {
    /// At the caret.
    Caret,
    /// In the section's header.
    Header,
    /// In the section's footer.
    Footer,
}

/// `insertPageNumber`'s options.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PageNumber {
    /// What the field shows.
    pub format: PageNumberFormat,
    /// Default: caret.
    #[serde(rename = "where", default, skip_serializing_if = "Option::is_none")]
    pub place: Option<PageNumberPlace>,
    /// The alignment in the header or footer; `None` keeps the paragraph's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alignment: Option<HorizontalAlignment>,
}

/// `insertTable`'s options.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TableSize {
    /// The number of rows.
    pub rows: u32,
    /// The number of columns.
    pub columns: u32,
}

/// `insertPicture`'s options.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Picture {
    /// The image file's bytes.
    pub data: Bytes,
    /// The alternative text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alt_text: Option<String>,
}

/// `insertHyperlink`'s options.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Hyperlink {
    /// The link's address (a URL or `#bookmark`).
    pub address: String,
    /// The text to show; `None` uses the selection, or the address when collapsed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

/// `editHyperlink`'s options.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HyperlinkEdit {
    /// The new address.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    /// The new display text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

/// `insertField`'s options.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InsertField {
    /// The field code (`PAGE`, `DATE \@ "d MMMM yyyy"`).
    pub code: String,
    /// The result shown until the field updates.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display: Option<String>,
    /// Shows the code instead of the result.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub show_code: Option<bool>,
}

/// Above or below.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VerticalSide {
    /// Above the selection.
    Above,
    /// Below the selection.
    Below,
}

/// Left or right.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HorizontalSide {
    /// To the selection's left.
    Left,
    /// To the selection's right.
    Right,
}

/// `insertRows`'s options.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InsertRows {
    /// Above or below the selected rows.
    #[serde(rename = "where")]
    pub side: VerticalSide,
    /// Default: 1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count: Option<u32>,
}

/// `insertColumns`'s options.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InsertColumns {
    /// Left or right of the selected columns.
    #[serde(rename = "where")]
    pub side: HorizontalSide,
    /// Default: 1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count: Option<u32>,
}

/// Where `deleteCells` shifts the remaining cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CellShift {
    /// Shifts the cells below up.
    Up,
    /// Shifts the cells to the right left.
    Left,
}

/// `splitCells`'s options.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SplitCells {
    /// The number of columns to split each cell into.
    pub columns: u32,
    /// Default: 1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rows: Option<u32>,
}

/// Up or down.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VerticalDirection {
    /// Up.
    Up,
    /// Down.
    Down,
}

/// What `autoFitColumns` fits the columns to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AutoFit {
    /// Fits the columns to their contents.
    Contents,
    /// Fits the table to the page's text width.
    Window,
    /// Fixes the columns at their current widths.
    Fixed,
}

/// The cell edges `setCellBorders` sets.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BorderEdges {
    /// Every edge.
    All,
    /// The outside edges.
    Outside,
    /// The inside edges.
    Inside,
    /// The inside horizontal edges.
    InsideHorizontal,
    /// The inside vertical edges.
    InsideVertical,
    /// The top edge.
    Top,
    /// The bottom edge.
    Bottom,
    /// The left edge.
    Left,
    /// The right edge.
    Right,
    /// Removes the borders.
    None,
}

/// A border line's style.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BorderStyle {
    /// A single line.
    Single,
    /// A double line.
    Double,
    /// A dotted line.
    Dotted,
    /// A dashed line.
    Dashed,
    /// A thick line.
    Thick,
}

/// A border line.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Border {
    /// The line style.
    pub style: BorderStyle,
    /// 0.25…6 pt.
    pub width: Points,
    /// The line color, or automatic.
    pub color: AutoOr<Color>,
}

/// `setCellBorders`'s options.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CellBorders {
    /// Which edges of the selected cells.
    pub edges: BorderEdges,
    /// Default: a ½ pt single automatic line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub border: Option<Border>,
}

/// `setCellMargins`'s options.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CellMargins {
    /// The top margin.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top: Option<Points>,
    /// The bottom margin.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bottom: Option<Points>,
    /// The left margin.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left: Option<Points>,
    /// The right margin.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub right: Option<Points>,
}

/// `setTableStyleOptions`'s options.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TableStyleChange {
    /// Formats the first row as a header.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub header_row: Option<bool>,
    /// Formats the last row as a total.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_row: Option<bool>,
    /// Formats the first column.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_column: Option<bool>,
    /// Formats the last column.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_column: Option<bool>,
    /// Alternates row shading.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub banded_rows: Option<bool>,
    /// Alternates column shading.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub banded_columns: Option<bool>,
}

/// `setObjectSize`'s options.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ObjectSize {
    /// The width.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<Points>,
    /// The height.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<Points>,
    /// Keeps the aspect ratio when one side is set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lock_aspect: Option<bool>,
}

/// `setAltText`'s options.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AltText {
    /// The alternative text.
    pub text: String,
    /// Marks the object decorative (no alternative text).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decorative: Option<bool>,
}

/// `setField`'s options.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SetField {
    /// The field.
    pub id: ElementId,
    /// The new field code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// The new display text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display: Option<Body>,
}

/// One field's computed result.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FieldResult {
    /// The field.
    pub id: ElementId,
    /// Its result.
    pub display: Body,
}

/// `setFieldResults`'s options.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FieldResults {
    /// The results, one per field.
    pub results: Vec<FieldResult>,
    /// The token of the `fieldsUpdateRequested` event whose step these results join.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<u64>,
}

/// Elements to act on; default: the selection's.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ElementIds {
    /// The elements; `None` means the selection's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ids: Option<Vec<ElementId>>,
}

/// `setFieldsLocked`'s options.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FieldsLocked {
    /// Locks (`true`) or unlocks the fields.
    pub locked: bool,
    /// The fields; `None` means the selection's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ids: Option<Vec<ElementId>>,
}

/// `showFieldCodes`'s options.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ShowFieldCodes {
    /// Shows codes (`true`) or results.
    pub show: bool,
    /// The fields; `None` means the selection's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ids: Option<Vec<ElementId>>,
}

/// `setFieldLabels`'s options: labels by field code.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FieldLabels {
    /// The label design mode shows for each field code.
    pub labels: BTreeMap<String, String>,
    /// Keeps labels for codes not listed. Default: false.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merge: Option<bool>,
}

/// `insertControl`'s options.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InsertControl {
    /// The kind of control.
    pub kind: ControlKind,
    /// The control's tag, for the application.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    /// The control's title, shown on its handle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Whether the control can be deleted or its content edited.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lock: Option<ControlLock>,
    /// The prompt text shown while the control is empty.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    /// The custom XML data part node the control is bound to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binding: Option<DataBinding>,
    /// The kind's options (list items, date format, checkbox symbols).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options: Option<ControlOptions>,
    /// Wraps the selection instead of inserting an empty control.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wrap_selection: Option<bool>,
}

/// `setControl`'s options. `binding: null` removes the binding.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SetControl {
    /// The control.
    pub id: ElementId,
    /// The new tag.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    /// The new title.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// The new lock.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lock: Option<ControlLock>,
    /// The new prompt text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    /// The new binding; `Some(None)` removes it.
    #[serde(default, skip_serializing_if = "Option::is_none", with = "super::wire::nullable")]
    pub binding: Option<Option<DataBinding>>,
    /// The new options.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options: Option<ControlOptions>,
}

/// `removeControl`'s options.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RemoveControl {
    /// The control.
    pub id: ElementId,
    /// Keeps the control's content in the document. Default: false.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keep_content: Option<bool>,
}

/// `fillControl`'s options: exactly one of `text`, `image` and `docx`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FillControl {
    /// The control.
    pub id: ElementId,
    /// Plain text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// An image, fitted to the control's box.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<Bytes>,
    /// DOCX content.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docx: Option<Bytes>,
}

/// `setCheckbox`'s options.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SetCheckbox {
    /// The checkbox control.
    pub id: ElementId,
    /// Checks (`true`) or unchecks it.
    pub checked: bool,
}

/// `wrapInRepeatingSection`'s options.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RepeatingSection {
    /// The section's tag.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    /// The section's title.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

/// Which element property `replaceInElements` searches.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ElementField {
    /// A field's code.
    Code,
    /// A control's tag.
    Tag,
    /// A control's title.
    Title,
}

/// Fields or content controls.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ElementClass {
    /// Fields.
    Field,
    /// Content controls.
    Control,
}

/// `replaceInElements`'s options.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReplaceInElements {
    /// The text to find (case-sensitive).
    pub find: String,
    /// Its replacement, at every occurrence.
    pub replace: String,
    /// Which property to search.
    #[serde(rename = "in")]
    pub field: ElementField,
    /// Fields or controls only; `None` means both.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<ElementClass>,
    /// The elements to search; `None` means every one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ids: Option<Vec<ElementId>>,
}

/// How serious an element mark is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Severity {
    /// An error.
    Error,
    /// A warning.
    Warning,
    /// Information.
    Info,
}

/// A mark on one element.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ElementMark {
    /// The element.
    pub id: ElementId,
    /// How serious the mark is.
    pub severity: Severity,
    /// The tooltip.
    pub message: String,
}

/// `markElements`'s options.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MarkElements {
    /// The marks to draw.
    pub marks: Vec<ElementMark>,
    /// Removes the earlier marks first. Default: false.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clear: Option<bool>,
}

/// The `'default'` keyword of [`PlaceholderMatch`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DefaultKeyword {
    /// `'default'`.
    Default,
}

/// Which placeholders a look applies to.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PlaceholderMatch {
    /// Every placeholder no other rule matches.
    Default(DefaultKeyword),
    /// One placeholder.
    Id {
        /// The placeholder's element id.
        id: ElementId,
    },
    /// Placeholders with this tag.
    Tag {
        /// The tag.
        tag: String,
    },
    /// Placeholders whose key starts with this prefix.
    Prefix {
        /// The key prefix.
        prefix: String,
    },
}

/// How a placeholder looks: drawn by the engine, or an image.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PlaceholderLook {
    /// An image drawn in the placeholder's box.
    Image {
        /// The image's bytes.
        image: Bytes,
        /// The image's MIME type (`image/png`).
        mime: String,
    },
    /// A box the engine draws.
    #[serde(rename_all = "camelCase")]
    Drawn {
        /// The fill color; `None` keeps the default.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fill: Option<Color>,
        /// The label; `None` keeps the default.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
        /// An SVG icon drawn with the label.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        icon_svg: Option<String>,
    },
}

/// One look rule.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlaceholderLookRule {
    /// Which placeholders the rule applies to.
    #[serde(rename = "match")]
    pub matches: PlaceholderMatch,
    /// How they look.
    pub look: PlaceholderLook,
}

/// `setPlaceholderLooks`'s options.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlaceholderLooks {
    /// The look rules.
    pub looks: Vec<PlaceholderLookRule>,
    /// Keeps rules for matches not listed. Default: false.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merge: Option<bool>,
}

/// `fillPlaceholder`'s options.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FillPlaceholder {
    /// Fills every unfilled placeholder with this key; `None` fills the selected one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// The image's bytes.
    pub image: Bytes,
}

/// `writeDataPart`'s options.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WriteDataPart {
    /// The part's root namespace.
    pub namespace: String,
    /// The part's item id, where several share the namespace.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_id: Option<String>,
    /// The part's XML (well-formed).
    pub xml: String,
}

/// A data part by namespace, and item id where several share it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DataPartRef {
    /// The part's root namespace.
    pub namespace: String,
    /// The part's item id, where several share the namespace.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_id: Option<String>,
}

/// Every revision, or one author's.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AuthorFilter {
    /// The author; `None` means every author.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
}

/// `replyToComment`'s options.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CommentReply {
    /// The comment replied to.
    pub to: CommentId,
    /// The reply's text.
    pub body: Body,
}

/// `editComment`'s options.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CommentEdit {
    /// The comment.
    pub id: CommentId,
    /// Its new text.
    pub body: Body,
}

/// `resolveComment`'s options.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CommentResolve {
    /// The thread's first comment.
    pub id: CommentId,
    /// Resolves (`true`) or reopens the thread.
    pub done: bool,
}

/// `setAuthorColor`'s options.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AuthorColor {
    /// The author's name.
    pub author: String,
    /// The color their revisions and comments are drawn in.
    pub color: Color,
}

/// A zoom: a percentage (10…500), or a fit the host computes.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Zoom {
    /// A percentage.
    Percent(f64),
    /// A fit.
    Fit(ZoomFit),
}

/// The zoom fits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ZoomFit {
    /// The page's width fills the view.
    PageWidth,
    /// A whole page fits in the view.
    WholePage,
}

/// `setViewOptions`'s options.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ViewOptions {
    /// Shows paragraph marks, spaces and tabs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub formatting_marks: Option<bool>,
    /// Shows table gridlines.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gridlines: Option<bool>,
    /// Shows text boundaries.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_boundaries: Option<bool>,
}

/// What a command returns besides its commit, for the commands that create something.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CommandResult {
    /// A field, control or repeating section made: `{ id }`.
    Element {
        /// The element's id.
        id: ElementId,
    },
    /// A comment or reply made: `{ id }`.
    Comment {
        /// The comment's id.
        id: CommentId,
    },
    /// A data part written: `{ itemId }`.
    #[serde(rename_all = "camelCase")]
    DataPart {
        /// The data part's item id.
        item_id: String,
    },
    /// Elements changed by `replaceInElements`: `{ changed }`.
    Changed {
        /// How many elements changed.
        changed: u32,
    },
}

impl Command {
    /// Whether the command changes the document: it fails `read-only` in a read-only session (the catalog's
    /// `mutates`).
    pub fn mutates(&self) -> bool {
        super::catalog::info(self.id()).is_none_or(|i| i.mutates)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_and_types_are_one_spelling() {
        for c in [Command::SetBold(true), Command::Undo, Command::InsertTable(TableSize { rows: 3, columns: 2 })] {
            let wire = serde_json::to_value(&c).expect("serializes");
            assert_eq!(wire["type"], serde_json::to_value(c.id()).expect("serializes"));
        }
    }

    #[test]
    fn single_arguments_travel_in_value_and_options_spread() {
        let bold: Command = serde_json::from_str(r#"{"type":"setBold","value":true}"#).expect("setBold");
        assert_eq!(bold, Command::SetBold(true));
        let table: Command = serde_json::from_str(r#"{"type":"insertTable","rows":3,"columns":2}"#).expect("insertTable");
        assert_eq!(table, Command::InsertTable(TableSize { rows: 3, columns: 2 }));
        let spacing: Command = serde_json::from_str(r#"{"type":"setLineSpacing","rule":"multiple","lines":1.5}"#).expect("setLineSpacing");
        assert_eq!(spacing, Command::SetLineSpacing(LineSpacing::Multiple { lines: 1.5 }));
        let none: Command = serde_json::from_str(r#"{"type":"setHighlight","value":null}"#).expect("setHighlight");
        assert_eq!(none, Command::SetHighlight(None));
    }

    /// The command set's wire fixture: one op per variant.
    fn fixture() -> Vec<serde_json::Value> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/wire/command.json");
        serde_json::from_str(&std::fs::read_to_string(path).expect("the command fixture")).expect("JSON")
    }

    #[test]
    fn every_id_round_trips_through_its_op_type() {
        let ops = fixture();
        for id in CommandId::ALL {
            let name = serde_json::to_value(id).expect("serializes");
            assert_eq!(serde_json::from_value::<CommandId>(name.clone()).ok(), Some(*id), "{name}");
            let op = ops.iter().find(|o| o["type"] == name).unwrap_or_else(|| panic!("{name} isn't in tests/wire/command.json"));
            let command: Command = serde_json::from_value(op.clone()).unwrap_or_else(|e| panic!("{op}: {e}"));
            assert_eq!(command.id(), *id);
            assert_eq!(serde_json::to_value(&command).expect("serializes")["type"], name);
        }
        assert_eq!(ops.len(), CommandId::ALL.len(), "one op per variant");
    }

    #[test]
    fn the_catalog_has_one_entry_per_id_in_order() {
        let catalog = super::super::catalog::catalog();
        let ids: Vec<CommandId> = catalog.iter().map(|c| c.id).collect();
        assert_eq!(ids, CommandId::ALL);
        for c in catalog {
            assert!(!c.label.is_empty(), "{:?} has a label", c.id);
            assert_eq!(super::super::catalog::info(c.id), Some(c));
        }
    }

    #[test]
    fn unknown_fields_and_ids_are_refused() {
        for bad in [
            r#"{"type":"setBold","value":true,"extra":1}"#,
            r#"{"type":"insertTable","rows":3,"columns":2,"cols":2}"#,
            r#"{"type":"toggleBold"}"#,
            r#"{"type":"setBold"}"#,
        ] {
            assert!(serde_json::from_str::<Command>(bad).is_err(), "{bad}");
        }
    }
}
