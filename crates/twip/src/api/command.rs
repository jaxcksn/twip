//! Commands: one typed variant per operation an application can run, applied at the selection.
//!
//! On the wire a command is an object with the command id as `type`. A command with one argument carries it in
//! `value` (`{"type":"setBold","value":true}`), and a command with an options object carries its fields beside `type`
//! (`{"type":"insertTable","rows":3,"columns":2}`). Unset optional fields keep their current value.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::drawing::{
    Arrange, Degrees, EffectsChange, FillValue, FlipAxis, LineChange, PictureAdjustChange, ShapeAdjust, ShapeInsert, ShapePreset, ShapeTextChange,
};
use super::review::{Body, MarkupView};
use super::services::TabSelector;
use super::state::{Mode, TextDistance};
use super::value::{
    Alignment, AutoOr, Bytes, CellAlignment, Color, CommentId, ControlKind, ControlLock, ControlOptions, DataBinding, Direction, ElementId,
    HighlightColor, HorizontalAlignment, HorizontalPosition, LineSpacing, NoneOr, ObjectWrap, Order, Orientation, PlaceholderImage, Points,
    RevisionId, RowHeight, StyleId, TabAlignment, TabLeader, TableWidth, UnderlineStyle, VerticalPosition, WrapSide,
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
    /// Changes a style to the selection's formatting: its paragraph's and its first run's direct formatting become the
    /// style's. Undo: one step.
    UpdateStyleToSelection(#[serde(with = "value")] StyleId),
    /// Deletes a style the document made: its paragraphs take the default paragraph style, its runs lose it, and the
    /// styles based on it take its base. Built-in styles can't be deleted. Undo: one step.
    DeleteStyle(#[serde(with = "value")] StyleId),
    /// Sets a style's list flags: visibility, priority, gallery, lock. Undo: one step.
    SetStyleMeta(StyleMetaChange),
    /// Sets the document's default formatting (Manage Styles' Set Defaults). Undo: one step.
    SetDocumentDefaults(DocumentDefaults),
    /// Bullets, numbering, or no list (`null`). Undo: one step.
    SetList(#[serde(with = "value")] Option<ListKind>),
    /// One list level deeper or shallower, in a list. Undo: one step.
    SetListLevel(#[serde(with = "value")] LevelChange),
    /// Defines the selection's list levels (Define New Bullet, Number Format, Multilevel List): for its whole list,
    /// from this paragraph on, or for the selected paragraphs. Outside a list, makes a new one. Undo: one step.
    SetListDefinition(ListDefinitionChange),
    /// Set Numbering Value: starts a new list at a value, or continues the previous list. Undo: one step.
    SetNumberingValue(NumberingValue),
    /// Applies a list gallery entry (`listGalleries`): a library or recent entry defines the selection's list (the
    /// current level for bullets and numbering, every level for a multilevel list), or makes a new one; a document
    /// list or list style continues that list. Undo: one step.
    ApplyListEntry(#[serde(with = "value")] super::services::ListEntryId),
    /// Restarts the list at this paragraph, at its level's start. Undo: one step.
    RestartNumbering,
    /// Continues the previous list from this paragraph. Undo: one step.
    ContinueNumbering,
    /// Adjust List Indents: the selection's list level's number position, text indent and what follows the number,
    /// for its whole list. Undo: one step.
    AdjustListIndents(ListIndents),
    /// Keep with next, keep lines together, page break before and widow control. Undo: one step.
    SetParagraphFlow(ParagraphFlow),
    /// A box, a bottom border or none (a ½ pt single line). Undo: one step.
    SetParagraphBorders(#[serde(with = "value")] BorderPreset),
    /// Paragraph shading, or none (`null`). Undo: one step.
    SetParagraphShading(#[serde(with = "value")] Option<Color>),
    /// The selected paragraphs' border lines: each edge set, removed (`null`) or kept (absent). Undo: one step.
    SetParagraphBorderLines(ParagraphBorderLines),
    /// Shading of the selected paragraphs or text: its fill, pattern and pattern color. Undo: one step.
    SetShading(ShadingChange),
    /// The selected text's border: a box of one line, with a shadow or not, or none. Undo: one step.
    SetTextBorder(TextBorderChange),
    /// The page borders of the selection's section, or of every section: each edge set, removed (`null`) or kept
    /// (absent), what they're measured from, and the pages they show on. Undo: one step.
    SetPageBorders(PageBordersChange),
    /// The frame the selection is in: its wrapping, size, position and distance from text. Undo: one step.
    SetFrame(FrameProperties),
    /// Removes the frame the selection is in: its paragraphs go back in the text, before the paragraph it was
    /// anchored to. Undo: one step.
    RemoveFrame,

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

    // Hyphenation
    /// Automatic hyphenation for the document, with the hyphenation patterns of the text's language. Undo: one step.
    SetAutoHyphenation(#[serde(with = "value")] bool),
    /// The hyphenation zone (0…1584 pt): in documents in compatibility mode 14 and earlier, a word is hyphenated only
    /// when moving it whole would leave this gap. Undo: one step.
    SetHyphenationZone(#[serde(with = "value")] Points),
    /// The most consecutive lines that may end in a hyphen (0…32767; 0 for no limit). Undo: one step.
    SetConsecutiveHyphenLimit(#[serde(with = "value")] u32),
    /// Whether words in capitals are hyphenated. Undo: one step.
    SetHyphenateCaps(#[serde(with = "value")] bool),

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
    /// A floating shape at the caret's paragraph, in front of the text, with the theme's default style. Undo: one step.
    InsertShape(ShapeInsert),
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
    /// Where the table is placed, and what it is measured from: a table without a position becomes positioned (text
    /// wraps around it); none (`null`) puts it back in the text. Undo: one step.
    SetTablePosition(#[serde(with = "value")] Option<TablePosition>),

    // Objects
    /// The selected object's size (1…1584 pt). Undo: one step.
    SetObjectSize(ObjectSize),
    /// The selected picture at its original size. Undo: one step.
    ResetObjectSize,
    /// How text wraps around the selected object. Undo: one step.
    SetObjectWrap(#[serde(with = "value")] ObjectWrap),
    /// Locks a floating object's anchor. Undo: one step.
    SetAnchorLocked(#[serde(with = "value")] bool),
    /// Where the selected floating object is, and what it is measured from. Undo: one step.
    SetObjectPosition(ObjectPosition),
    /// The sides text wraps on around the selected floating object, and its distance from the text. Undo: one step.
    SetObjectWrapOptions(ObjectWrapOptions),
    /// The selected object's alternative text. Undo: one step.
    SetAltText(AltText),
    /// Deletes the selected object. Undo: one step.
    DeleteObject,
    /// The selected shape's preset geometry; its text, fill, outline and effects stay and its adjust values reset.
    /// Undo: one step.
    SetShapePreset(#[serde(with = "value")] ShapePreset),
    /// One of the selected shape's adjust values. Undo: one step.
    SetShapeAdjust(ShapeAdjust),
    /// The selected objects' fill (`fromStyle`: the style reference's again). Undo: one step.
    SetShapeFill(#[serde(with = "value")] FillValue),
    /// The selected objects' outline; what isn't given is kept. Undo: one step.
    SetShapeLine(LineChange),
    /// The selected objects' shadows, glow, soft edge and blur; what isn't given is kept, `null` removes. Undo: one
    /// step.
    SetShapeEffects(EffectsChange),
    /// The selected shape's text insets, anchor, autofit, wrapping and direction. Undo: one step.
    SetShapeText(ShapeTextChange),
    /// The selected picture's transparency, greyscale, brightness and contrast. Undo: one step.
    SetPictureAdjust(PictureAdjustChange),
    /// The selected objects' rotation (0…360°, clockwise). Undo: one step.
    SetObjectRotation(#[serde(with = "value")] Degrees),
    /// Flips the selected objects. Undo: one step.
    FlipObject(#[serde(with = "value")] FlipAxis),
    /// Moves the selected floating object in the paint order, or behind or in front of the text. Undo: one step.
    ArrangeObject(#[serde(with = "value")] Arrange),
    /// Groups the selected floating objects (two or more, one story). Undo: one step.
    GroupObjects,
    /// Ungroups the selected group. Undo: one step.
    UngroupObjects,
    /// Selects the next or previous child of the selected group. Undo: view.
    SelectGroupChild(#[serde(with = "value")] Order),

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
    /// Which fields are sub-template references: replaces the rules; an empty list turns sub-templates off. Undo: view.
    SetSubTemplateRules(SubTemplateRules),
    /// The application's own field types (compared without case), replacing those set before: their fields keep their
    /// results, show their labels in design mode and are never read as bookmark references. Undo: view.
    SetApplicationFieldTypes(ApplicationFieldTypes),
    /// Shows references' sub-template bodies in their place, or their fields (default: every reference). Undo: view.
    SetSubTemplateView(SubTemplateView),
    /// Edits a sub-template's body in this editor, on its own pages; the template shows again on Close Sub-template.
    /// Undo: view.
    EditSubTemplate(SubTemplateName),
    /// Back to the template from a sub-template, at the template's selection from before. Undo: view.
    CloseSubTemplate,
    /// A new sub-template, empty or with a DOCX's body. Undo: one step.
    CreateSubTemplate(CreateSubTemplate),
    /// Renames a sub-template, and optionally every reference to it. Undo: one step.
    RenameSubTemplate(RenameSubTemplate),
    /// Removes a sub-template; its references no longer resolve. Undo: one step.
    RemoveSubTemplate(SubTemplateName),
    /// The template's body becomes a DOCX's body, on its last section's page; its sub-templates, data parts,
    /// settings, styles, headers and footers stay. Closes a sub-template being edited first. Undo: one step.
    ReplaceBody(ReplaceBody),

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
    /// Rewrites a font family's name in the document; result `{ changed, skipped }`. Undo: one step.
    ReplaceFont(ReplaceFont),
    /// Wavy marks with a tooltip on elements. Undo: view.
    MarkElements(MarkElements),
    /// The tag prefixes whose picture controls keep their pictures (compared without case), replacing those set before:
    /// editing can't delete, paste over or replace such a picture; `fillControl` and `setControl` still change it, and
    /// in design mode deleting it removes its whole control. Undo: view.
    SetProtectedPictureControls(ProtectedPictureControls),
    /// Selects a picture control (the selected one when none is named) and emits `pictureControlActivated`, in any
    /// mode. Undo: none.
    ActivatePictureControl(ActivatePictureControl),

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
    /// What a click on the ruler adds; answers the tab type picker. Undo: view.
    SetRulerTabType(#[serde(with = "value")] TabSelector),
    /// Asks for the tab type picker (`popoverRequested`). Undo: view.
    ShowTabTypes,
    /// Asks for the options of the tab stop nearest the caret (`popoverRequested`). Undo: view.
    ShowTabStop,
    /// What typing corrects. Undo: none (a session setting).
    SetAutoCorrect(super::services::AutoCorrectChange),
    /// Puts back what was typed in place of the correction just made. Undo: one step.
    RevertAutoCorrect,
    /// Reverts the correction just made and never makes it again. Undo: one step (the revert).
    NeverAutoCorrect,
    /// Asks for the options of the correction just made (`popoverRequested`). Undo: view.
    ShowAutoCorrectOptions,
    /// Dismisses the popover asked for (`popoverClosed`): what an application that closes it itself, on Esc or a
    /// click elsewhere, sends. With none open it does nothing. Undo: view.
    ClosePopover,
    /// Says the formatting at the caret to screen readers (paragraph style, font and size, emphasis, colour,
    /// highlight) as an announcement; changes nothing. Undo: view.
    DescribeFormatting,
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

/// `setParagraphBorderLines`'s options: each edge set, removed (`null`) or, absent, kept.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ParagraphBorderLines {
    /// The top edge.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "super::services::present")]
    pub top: Option<Option<Border>>,
    /// The bottom edge.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "super::services::present")]
    pub bottom: Option<Option<Border>>,
    /// The left edge.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "super::services::present")]
    pub left: Option<Option<Border>>,
    /// The right edge.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "super::services::present")]
    pub right: Option<Option<Border>>,
    /// The line between paragraphs with the same borders.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "super::services::present")]
    pub between: Option<Option<Border>>,
    /// A shadow on the bottom and right edges (Shadow setting).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shadow: Option<bool>,
}

/// What page borders are measured from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PageBorderOffset {
    /// The page's edge.
    #[default]
    Page,
    /// The text.
    Text,
}

/// The pages a section's borders show on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PageBorderDisplay {
    /// Every page.
    #[default]
    AllPages,
    /// The section's first page.
    FirstPage,
    /// Every page but the first.
    NotFirstPage,
}

/// The sections page borders apply to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PageBorderScope {
    /// Every section.
    WholeDocument,
    /// The selection's section.
    #[default]
    ThisSection,
}

/// `setPageBorders`'s options: each edge set, removed (`null`) or, absent, kept.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PageBordersChange {
    /// The top edge.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "super::services::present")]
    pub top: Option<Option<Border>>,
    /// The bottom edge.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "super::services::present")]
    pub bottom: Option<Option<Border>>,
    /// The left edge.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "super::services::present")]
    pub left: Option<Option<Border>>,
    /// The right edge.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "super::services::present")]
    pub right: Option<Option<Border>>,
    /// A shadow on the bottom and right edges.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shadow: Option<bool>,
    /// What the borders are measured from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset_from: Option<PageBorderOffset>,
    /// The pages they show on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display: Option<PageBorderDisplay>,
    /// The sections they apply to.
    #[serde(default)]
    pub apply_to: PageBorderScope,
}

/// `setTextBorder`'s options: the box's line, or none to remove it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextBorderChange {
    /// The line, or none.
    pub border: Option<Border>,
    /// A shadow.
    #[serde(default)]
    pub shadow: bool,
}

/// What shading applies to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ShadingTarget {
    /// The selected paragraphs.
    #[default]
    Paragraph,
    /// The selected text.
    Text,
    /// A table style part's cells (from the style dialog's Format > Border).
    Cell,
}

/// A shading pattern: clear (the fill only), solid (the pattern color), or a percentage of the pattern color over the
/// fill.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ShadingPattern {
    /// The fill only.
    #[default]
    Clear,
    /// The pattern color only.
    Solid,
    /// 5%.
    Pct5,
    /// 10%.
    Pct10,
    /// 20%.
    Pct20,
    /// 25%.
    Pct25,
    /// 30%.
    Pct30,
    /// 40%.
    Pct40,
    /// 50%.
    Pct50,
    /// 60%.
    Pct60,
    /// 70%.
    Pct70,
    /// 75%.
    Pct75,
    /// 80%.
    Pct80,
    /// 90%.
    Pct90,
}

impl ShadingPattern {
    const ALL: [ShadingPattern; 14] = [
        ShadingPattern::Clear,
        ShadingPattern::Solid,
        ShadingPattern::Pct5,
        ShadingPattern::Pct10,
        ShadingPattern::Pct20,
        ShadingPattern::Pct25,
        ShadingPattern::Pct30,
        ShadingPattern::Pct40,
        ShadingPattern::Pct50,
        ShadingPattern::Pct60,
        ShadingPattern::Pct70,
        ShadingPattern::Pct75,
        ShadingPattern::Pct80,
        ShadingPattern::Pct90,
    ];

    /// The pattern a `w:val` names; clear for those not offered.
    pub fn of_val(v: &str) -> ShadingPattern {
        ShadingPattern::ALL.into_iter().find(|p| p.val() == v).unwrap_or_default()
    }

    /// The `w:val` it is written as.
    pub fn val(self) -> &'static str {
        match self {
            ShadingPattern::Clear => "clear",
            ShadingPattern::Solid => "solid",
            ShadingPattern::Pct5 => "pct5",
            ShadingPattern::Pct10 => "pct10",
            ShadingPattern::Pct20 => "pct20",
            ShadingPattern::Pct25 => "pct25",
            ShadingPattern::Pct30 => "pct30",
            ShadingPattern::Pct40 => "pct40",
            ShadingPattern::Pct50 => "pct50",
            ShadingPattern::Pct60 => "pct60",
            ShadingPattern::Pct70 => "pct70",
            ShadingPattern::Pct75 => "pct75",
            ShadingPattern::Pct80 => "pct80",
            ShadingPattern::Pct90 => "pct90",
        }
    }
}

/// `setShading`'s options.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ShadingChange {
    /// The paragraphs or the text.
    pub target: ShadingTarget,
    /// The fill, or none (no shading at all, with a clear pattern).
    pub fill: NoneOr<Color>,
    /// The pattern.
    #[serde(default)]
    pub pattern: ShadingPattern,
    /// The pattern's color.
    #[serde(default)]
    pub color: AutoOr<Color>,
}

/// Whether text wraps around a frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FrameWrap {
    /// Text stays above and below the frame.
    None,
    /// Text wraps around the frame.
    #[default]
    Around,
}

/// How a frame's width is set.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FrameWidthRule {
    /// As wide as its text.
    #[default]
    Auto,
    /// The width given.
    Exactly,
}

/// How a frame's height is set.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FrameHeightRule {
    /// As tall as its text.
    #[default]
    Auto,
    /// At least the height given.
    AtLeast,
    /// The height given.
    Exactly,
}

/// A frame's horizontal alignment.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FrameHorizontalAlign {
    /// Left.
    Left,
    /// Centered.
    Center,
    /// Right.
    Right,
    /// Inside (left on odd pages).
    Inside,
    /// Outside (right on odd pages).
    Outside,
}

/// A frame's vertical alignment.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FrameVerticalAlign {
    /// Top.
    Top,
    /// Centered.
    Center,
    /// Bottom.
    Bottom,
    /// Inside.
    Inside,
    /// Outside.
    Outside,
}

/// What a frame's horizontal position is relative to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FrameHorizontalRelative {
    /// The page margins.
    Margin,
    /// The page.
    Page,
    /// The column.
    #[default]
    Column,
}

/// What a frame's vertical position is relative to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FrameVerticalRelative {
    /// The page margins.
    Margin,
    /// The page.
    Page,
    /// The paragraph it is anchored to (it moves with the text).
    #[default]
    Paragraph,
}

/// `setFrame`'s options: a frame's wrapping, size, position and distance from text.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FrameProperties {
    /// Text wrapping.
    pub wrap: FrameWrap,
    /// How the width is set.
    pub width_rule: FrameWidthRule,
    /// The width, when exact.
    pub width: Points,
    /// How the height is set.
    pub height_rule: FrameHeightRule,
    /// The height, unless automatic.
    pub height: Points,
    /// The horizontal alignment, or none for `horizontal`.
    pub horizontal_align: Option<FrameHorizontalAlign>,
    /// The horizontal position, from `horizontalRelative`.
    pub horizontal: Points,
    /// What the horizontal position is relative to.
    pub horizontal_relative: FrameHorizontalRelative,
    /// The distance from text, left and right.
    pub horizontal_distance: Points,
    /// The vertical alignment, or none for `vertical`.
    pub vertical_align: Option<FrameVerticalAlign>,
    /// The vertical position, from `verticalRelative`.
    pub vertical: Points,
    /// What the vertical position is relative to.
    pub vertical_relative: FrameVerticalRelative,
    /// The distance from text, above and below.
    pub vertical_distance: Points,
    /// The anchor stays with its paragraph.
    pub lock_anchor: bool,
}

/// `setStyleMeta`'s options: what isn't given is kept.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StyleMetaChange {
    /// The style.
    pub id: StyleId,
    /// Whether it shows in style lists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visibility: Option<super::services::StyleVisibility>,
    /// Its sort priority (`w:uiPriority`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<u32>,
    /// Shown in the Styles gallery.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quick: Option<bool>,
    /// Restricted while formatting is protected.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locked: Option<bool>,
}

/// `setDocumentDefaults`'s options: the formatting text has before any style's; what isn't given is kept.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DocumentDefaults {
    /// The font.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font: Option<String>,
    /// The size.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<Points>,
    /// The font color.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<AutoOr<Color>>,
    /// The space before and after paragraphs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spacing: Option<ParagraphSpacing>,
    /// The line spacing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_spacing: Option<LineSpacing>,
    /// The left and right indents.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub indents: Option<Indents>,
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

/// A list level's number style.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NumberFormat {
    /// 1, 2, 3.
    #[default]
    Decimal,
    /// I, II, III.
    UpperRoman,
    /// i, ii, iii.
    LowerRoman,
    /// A, B, C.
    UpperLetter,
    /// a, b, c.
    LowerLetter,
    /// 1st, 2nd, 3rd.
    Ordinal,
    /// One, Two, Three.
    CardinalText,
    /// First, Second, Third.
    OrdinalText,
    /// 01, 02, 03.
    DecimalZero,
    /// A bullet: the level's text in its font, or its picture.
    Bullet,
    /// No number.
    None,
}

impl NumberFormat {
    const ALL: [NumberFormat; 11] = [
        NumberFormat::Decimal,
        NumberFormat::UpperRoman,
        NumberFormat::LowerRoman,
        NumberFormat::UpperLetter,
        NumberFormat::LowerLetter,
        NumberFormat::Ordinal,
        NumberFormat::CardinalText,
        NumberFormat::OrdinalText,
        NumberFormat::DecimalZero,
        NumberFormat::Bullet,
        NumberFormat::None,
    ];

    /// The `w:numFmt` it is written as.
    pub fn val(self) -> &'static str {
        match self {
            NumberFormat::Decimal => "decimal",
            NumberFormat::UpperRoman => "upperRoman",
            NumberFormat::LowerRoman => "lowerRoman",
            NumberFormat::UpperLetter => "upperLetter",
            NumberFormat::LowerLetter => "lowerLetter",
            NumberFormat::Ordinal => "ordinal",
            NumberFormat::CardinalText => "cardinalText",
            NumberFormat::OrdinalText => "ordinalText",
            NumberFormat::DecimalZero => "decimalZero",
            NumberFormat::Bullet => "bullet",
            NumberFormat::None => "none",
        }
    }

    /// The style a `w:numFmt` names; decimal for those not offered.
    pub fn of_val(v: &str) -> NumberFormat {
        NumberFormat::ALL.into_iter().find(|f| f.val() == v).unwrap_or_default()
    }
}

/// How a list level's number aligns at its position.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LevelAlignment {
    /// Left.
    #[default]
    Left,
    /// Centered.
    Center,
    /// Right.
    Right,
}

/// What follows a list level's number.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FollowNumber {
    /// A tab.
    #[default]
    Tab,
    /// A space.
    Space,
    /// Nothing.
    Nothing,
}

/// A list level: its number or bullet, and where it and the text start.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListLevel {
    /// The number style.
    pub format: NumberFormat,
    /// The number format: its text, with `%1` to `%9` for the numbers of this level and the levels above it. A
    /// bullet's character.
    pub text: String,
    /// The number it starts at.
    pub start: u32,
    /// How the number aligns at `numberPosition`.
    pub alignment: LevelAlignment,
    /// What follows the number.
    pub follow: FollowNumber,
    /// The tab stop after the number, when `follow` is a tab: none uses the text indent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab: Option<Points>,
    /// Where the number is, from the margin.
    pub number_position: Points,
    /// Where the text is, from the margin.
    pub text_indent: Points,
    /// The level after which it restarts (1 to 9); 0 never; none after the level above.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restart_after: Option<u8>,
    /// Legal style numbering: the numbers of every level in Arabic numerals.
    pub legal: bool,
    /// The paragraph style linked to the level.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub linked_style: Option<StyleId>,
    /// The number's or bullet's font.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font: Option<String>,
    /// A picture bullet's image (PNG, JPEG or GIF).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub picture: Option<super::value::Bytes>,
}

/// What a list definition applies to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ListApplyTo {
    /// The selection's whole list.
    #[default]
    WholeList,
    /// The list from the selection's first paragraph on: a new list.
    ThisPointForward,
    /// The selected paragraphs: a new list.
    CurrentParagraph,
}

/// `setListDefinition`'s options: level `k` of `levels` defines level `k`; levels past its end are kept.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListDefinitionChange {
    /// The levels, from the first.
    pub levels: Vec<ListLevel>,
    /// What they apply to.
    #[serde(default)]
    pub apply_to: ListApplyTo,
    /// The list's name (the ListNum field list name); none keeps it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// How Set Numbering Value sets the number.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NumberingMode {
    /// Start a new list.
    #[default]
    StartNew,
    /// Continue from the previous list.
    Continue,
}

/// `setNumberingValue`'s options.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NumberingValue {
    /// Start a new list, or continue the previous one.
    pub mode: NumberingMode,
    /// The value to start at (a new list), or to skip to (continuing, with `advance`).
    pub value: u32,
    /// Continuing: skip numbers to `value`.
    #[serde(default)]
    pub advance: bool,
}

/// `adjustListIndents`'s options.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListIndents {
    /// Where the number is, from the margin.
    pub number_position: Points,
    /// Where the text is, from the margin.
    pub text_indent: Points,
    /// What follows the number.
    pub follow: FollowNumber,
    /// The tab stop after the number; none uses the text indent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab: Option<Points>,
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
    /// Dots and dashes.
    DotDash,
    /// Two dots and a dash.
    DotDotDash,
    /// Three lines.
    Triple,
    /// A thin line and a thick one.
    ThinThickSmallGap,
    /// A thick line and a thin one.
    ThickThinSmallGap,
    /// Thin, thick, thin.
    ThinThickThinSmallGap,
    /// A wave.
    Wave,
    /// Two waves.
    DoubleWave,
    /// Raised (3-D).
    ThreeDEmboss,
    /// Sunken (3-D).
    ThreeDEngrave,
    /// Outset.
    Outset,
    /// Inset.
    Inset,
}

impl BorderStyle {
    /// The `w:val` it is written as.
    pub fn val(self) -> &'static str {
        match self {
            BorderStyle::Single => "single",
            BorderStyle::Double => "double",
            BorderStyle::Dotted => "dotted",
            BorderStyle::Dashed => "dashed",
            BorderStyle::Thick => "thick",
            BorderStyle::DotDash => "dotDash",
            BorderStyle::DotDotDash => "dotDotDash",
            BorderStyle::Triple => "triple",
            BorderStyle::ThinThickSmallGap => "thinThickSmallGap",
            BorderStyle::ThickThinSmallGap => "thickThinSmallGap",
            BorderStyle::ThinThickThinSmallGap => "thinThickThinSmallGap",
            BorderStyle::Wave => "wave",
            BorderStyle::DoubleWave => "doubleWave",
            BorderStyle::ThreeDEmboss => "threeDEmboss",
            BorderStyle::ThreeDEngrave => "threeDEngrave",
            BorderStyle::Outset => "outset",
            BorderStyle::Inset => "inset",
        }
    }

    /// The style a `w:val` names; single for the ones not offered.
    pub fn of_val(v: &str) -> BorderStyle {
        [
            BorderStyle::Double,
            BorderStyle::Dotted,
            BorderStyle::Dashed,
            BorderStyle::Thick,
            BorderStyle::DotDash,
            BorderStyle::DotDotDash,
            BorderStyle::Triple,
            BorderStyle::ThinThickSmallGap,
            BorderStyle::ThickThinSmallGap,
            BorderStyle::ThinThickThinSmallGap,
            BorderStyle::Wave,
            BorderStyle::DoubleWave,
            BorderStyle::ThreeDEmboss,
            BorderStyle::ThreeDEngrave,
            BorderStyle::Outset,
            BorderStyle::Inset,
        ]
        .into_iter()
        .find(|s| s.val() == v)
        .unwrap_or(BorderStyle::Single)
    }
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

/// `setObjectPosition`'s options; what isn't given is kept.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ObjectPosition {
    /// The horizontal place.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub horizontal: Option<HorizontalPosition>,
    /// The vertical place.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vertical: Option<VerticalPosition>,
    /// Other floating objects may overlap it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allow_overlap: Option<bool>,
    /// In a table cell, it is placed and kept within the cell.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout_in_cell: Option<bool>,
}

/// `setTablePosition`'s value: a positioned table's place on each axis, as `setObjectPosition` gives an object's, and
/// its distance from the text around it; what isn't given is kept. A table is measured from the column, the margin or
/// the page across, and from the paragraph, the margin or the page down.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TablePosition {
    /// The horizontal place.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub horizontal: Option<HorizontalPosition>,
    /// The vertical place.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vertical: Option<VerticalPosition>,
    /// How far the text around it keeps (0…1584 pt).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wrap_distance: Option<TextDistance>,
}

/// `setObjectWrapOptions`'s options; what isn't given is kept. Distances are 0…1584 pt.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ObjectWrapOptions {
    /// The sides text wraps on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub side: Option<WrapSide>,
    /// The distance from the text above.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top: Option<Points>,
    /// The distance from the text below.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bottom: Option<Points>,
    /// The distance from the text on the left.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left: Option<Points>,
    /// The distance from the text on the right.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub right: Option<Points>,
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

/// One sub-template reference rule: a field of type `field_type` names, in argument `argument`, the building block
/// in category `category` (and `gallery`) whose body it shows.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReferenceRule {
    /// The field type (`SUBTEMPLATE`), compared without case.
    pub field_type: String,
    /// Which of the field's arguments is the sub-template's name, from 0. Default: 0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub argument: Option<u32>,
    /// The building block category the name is looked up in.
    pub category: String,
    /// The building block gallery (`custom1`), when it matters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gallery: Option<String>,
}

/// A sub-template by name, and its category where several rules name categories.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SubTemplateName {
    /// Its name.
    pub name: String,
    /// Its building block category; default: the rules' categories, in order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
}

/// `createSubTemplate`'s options.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateSubTemplate {
    /// Its name: unique in its category.
    pub name: String,
    /// Its building block category; default: the first rule's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    /// A DOCX whose body becomes its body; default: one empty paragraph.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<Bytes>,
    /// A sub-template in the same category whose body (formatting, fields, controls, pictures, references) is
    /// copied; not with `from`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub copy_of: Option<String>,
}

/// `replaceBody`'s options.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReplaceBody {
    /// The DOCX whose body becomes the template's.
    pub from: Bytes,
}

/// `renameSubTemplate`'s options.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RenameSubTemplate {
    /// Its name now.
    pub name: String,
    /// Its new name: unique in its category.
    pub to: String,
    /// Its building block category, where several rules name categories.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    /// Renames the references too, in the template and in other sub-templates. Default: false.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub update_references: Option<bool>,
}

/// `setSubTemplateView`'s options.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SubTemplateView {
    /// The reference fields; `None` means every reference.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ids: Option<Vec<ElementId>>,
    /// Their bodies show in their place (else their fields do).
    pub expanded: bool,
}

/// `setSubTemplateRules`'s options.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SubTemplateRules {
    /// The rules, replacing those set before.
    pub rules: Vec<ReferenceRule>,
}

/// `setApplicationFieldTypes`' options.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApplicationFieldTypes {
    /// The field types (a code's first word), replacing those set before; an empty list clears them.
    pub types: Vec<String>,
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
    /// A picture control's placeholder picture; default: the engine's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder_image: Option<PlaceholderImage>,
    /// Wraps the selection instead of inserting an empty control.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wrap_selection: Option<bool>,
    /// A picture control floats: positioned and wrapped as `setObjectWrap` and `setObjectPosition` place an object.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub float: Option<ControlFloat>,
}

/// How `insertControl` floats a picture control.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ControlFloat {
    /// How text wraps around it; not `inline`.
    pub wrap: ObjectWrap,
    /// The horizontal place; where it would sit in line when not given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub horizontal: Option<HorizontalPosition>,
    /// The vertical place; where it would sit in line when not given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vertical: Option<VerticalPosition>,
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
    /// A picture control's new placeholder picture, while it shows its placeholder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder_image: Option<PlaceholderImage>,
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

/// `replaceFont`'s options: rewrite `from` to `to` in the document itself, so the saved file names `to`.
///
/// In document scope with no style filter it rewrites the text (each run's family slots, paragraph marks and symbol
/// characters) and the tables below it: styles, document defaults, numbering levels, the theme's font slots and the
/// font table. In selection scope, or with a filter, it rewrites only the text, and a matching run whose family comes
/// from a style or the theme gets `to` as its own. With tracked changes on, the text's changes are formatting
/// revisions. Text inside locked content controls is skipped and counted.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReplaceFont {
    /// The family to rewrite, case-insensitive.
    pub from: String,
    /// The family it becomes.
    pub to: String,
    /// The whole document (the default) or the selection.
    #[serde(default, skip_serializing_if = "is_document")]
    pub scope: ReplaceScope,
    /// Only bold (`true`) or regular-weight (`false`) text; `None` for both.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bold: Option<bool>,
    /// Only italic (`true`) or upright (`false`) text; `None` for both.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub italic: Option<bool>,
}

fn is_document(scope: &ReplaceScope) -> bool {
    *scope == ReplaceScope::Document
}

/// Where `replaceFont` rewrites.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReplaceScope {
    /// The whole document: every story, and the tables below the text.
    #[default]
    Document,
    /// The selected text.
    Selection,
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

/// `setProtectedPictureControls`' options.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProtectedPictureControls {
    /// The tag prefixes, replacing those set before; an empty list protects none.
    pub tag_prefixes: Vec<String>,
}

/// `activatePictureControl`'s options.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActivatePictureControl {
    /// The picture control; the selected one when `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub element: Option<ElementId>,
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
    /// Text wraps around an object again as it is dragged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub live_layout: Option<bool>,
    /// The chrome's colour scheme.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<super::services::ChromeTheme>,
    /// The host's colours for the light and dark schemes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme_colors: Option<super::services::ThemeColors>,
    /// The chrome's accent: selection, handles, guides and active markers. Any colour; the dark scheme lightens it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accent: Option<Color>,
    /// How the page is drawn on screen (dark mode).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub appearance: Option<super::services::DocumentAppearance>,
    /// How much the editor measures while you drag and hover.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<super::services::MeasureDetail>,
    /// When the dot grid objects snap to shows (unrelated to `gridlines`, the table gridlines).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object_grid: Option<super::services::ObjectGrid>,
    /// Labels content controls and shown sub-templates while the caret is in them or the pointer is over them.
    /// Default: on. A control whose `w15:appearance` is `hidden` is never labelled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub control_labels: Option<bool>,
    /// When fields are shaded. Default: when selected.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field_shading: Option<super::services::FieldShading>,
    /// Pointer shapes over elements instead of the editor's own, replacing those set before; the first that matches
    /// the hovered element wins, and `[]` clears them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor_overrides: Option<Vec<super::services::CursorOverride>>,
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
    // Before `Changed`: untagged, the variant with more fields reads first.
    /// What `replaceFont` rewrote: `{ changed, skipped }`.
    FontReplaced {
        /// The places rewritten: runs, paragraph marks, symbol characters, styles, document defaults, numbering
        /// levels, theme font slots and font table entries.
        changed: u32,
        /// The places inside locked content controls, left as they were.
        skipped: u32,
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
