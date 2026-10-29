namespace Twip;

/// <summary>Every command's id. Its camelCase name is the op type, the catalog id and the key in <see cref="EditorState.Commands"/>.</summary>
public enum CommandId
{
    /// <summary>Inserts text at the selection, replacing it. Consecutive typing joins one undo step.</summary>
    InsertText,
    /// <summary>Starts a new paragraph at the selection (Enter).</summary>
    InsertParagraph,
    /// <summary>Inserts a line break (Shift+Enter).</summary>
    InsertLineBreak,
    /// <summary>Inserts a page break (Ctrl+Enter). Only in the body, not in a table.</summary>
    InsertPageBreak,
    /// <summary>Inserts a column break. Only in the body, not in a table.</summary>
    InsertColumnBreak,
    /// <summary>Inserts a section break. Only in the body, not in a table.</summary>
    InsertSectionBreak,
    /// <summary>Inserts a tab character. Joins the typing undo step.</summary>
    InsertTab,
    /// <summary>Inserts one character, optionally in a symbol font.</summary>
    InsertSymbol,
    /// <summary>Deletes the selection, or one unit next to the caret. Joins the typing undo step.</summary>
    Delete,
    /// <summary>Cuts the selection to the clipboard.</summary>
    Cut,
    /// <summary>Copies the selection to the clipboard. Works when read only.</summary>
    Copy,
    /// <summary>Pastes from the clipboard.</summary>
    Paste,
    /// <summary>Pastes the given text as plain text.</summary>
    PasteText,
    /// <summary>Undoes the last step.</summary>
    Undo,
    /// <summary>Redoes the last undone step.</summary>
    Redo,
    /// <summary>Repeats the last action, or redoes (F4).</summary>
    Repeat,
    /// <summary>Selects the whole story.</summary>
    SelectAll,
    /// <summary>Extends the selection to a unit around it.</summary>
    Select,
    /// <summary>Moves or extends the selection.</summary>
    MoveSelection,
    /// <summary>Moves to the next or previous table cell. Next from the last cell adds a row.</summary>
    MoveToCell,
    /// <summary>Sets the selection mode (F8 extends, Ctrl+Shift+F8 selects a block).</summary>
    SetSelectionMode,
    /// <summary>Returns to an earlier edit location (Shift+F5).</summary>
    GoBack,
    /// <summary>Leaves a header or footer, a text box, or an object selection.</summary>
    Escape,
    /// <summary>Selects the next or previous floating object.</summary>
    SelectNextObject,
    /// <summary>Moves the caret to the start of a page.</summary>
    GoToPage,
    /// <summary>Sets or clears bold on the selection.</summary>
    SetBold,
    /// <summary>Sets or clears italic on the selection.</summary>
    SetItalic,
    /// <summary>Sets the underline style of the selection.</summary>
    SetUnderline,
    /// <summary>Sets or clears strikethrough on the selection.</summary>
    SetStrikethrough,
    /// <summary>Sets or clears double strikethrough on the selection.</summary>
    SetDoubleStrikethrough,
    /// <summary>Sets or clears subscript on the selection.</summary>
    SetSubscript,
    /// <summary>Sets or clears superscript on the selection.</summary>
    SetSuperscript,
    /// <summary>Sets or clears small capitals on the selection.</summary>
    SetSmallCaps,
    /// <summary>Sets or clears all capitals on the selection.</summary>
    SetAllCaps,
    /// <summary>Sets or clears hidden text on the selection.</summary>
    SetHidden,
    /// <summary>Sets the font family of the selection.</summary>
    SetFont,
    /// <summary>Sets the font size of the selection.</summary>
    SetFontSize,
    /// <summary>Grows the font to the next size in the standard list.</summary>
    GrowFont,
    /// <summary>Shrinks the font to the previous size in the standard list.</summary>
    ShrinkFont,
    /// <summary>Sets the text colour of the selection.</summary>
    SetFontColor,
    /// <summary>Sets the highlight colour of the selection.</summary>
    SetHighlight,
    /// <summary>Sets the spacing between characters.</summary>
    SetCharacterSpacing,
    /// <summary>Raises or lowers the text.</summary>
    SetCharacterPosition,
    /// <summary>Removes direct character formatting from the selection.</summary>
    ClearFormatting,
    /// <summary>Changes the case of the selected text.</summary>
    ChangeCase,
    /// <summary>Holds the selection's formatting for PasteFormatting (the format painter).</summary>
    CopyFormatting,
    /// <summary>Applies the held formatting to the selection.</summary>
    PasteFormatting,
    /// <summary>Sets the alignment of the selected paragraphs.</summary>
    SetAlignment,
    /// <summary>Sets the line spacing of the selected paragraphs.</summary>
    SetLineSpacing,
    /// <summary>Sets the space before and after the selected paragraphs. Unset values are kept.</summary>
    SetParagraphSpacing,
    /// <summary>Sets the indents of the selected paragraphs. Unset values are kept.</summary>
    SetIndents,
    /// <summary>Increases the left indent to the next tab stop.</summary>
    IncreaseIndent,
    /// <summary>Decreases the left indent to the previous tab stop.</summary>
    DecreaseIndent,
    /// <summary>Applies a paragraph style to the selected paragraphs.</summary>
    SetParagraphStyle,
    /// <summary>Makes the selected paragraphs a list, or removes them from one.</summary>
    SetList,
    /// <summary>Moves the selected list items a level in or out.</summary>
    SetListLevel,
    /// <summary>Sets how the selected paragraphs break across pages. Unset values are kept.</summary>
    SetParagraphFlow,
    /// <summary>Sets a border preset on the selected paragraphs (a half-point single line).</summary>
    SetParagraphBorders,
    /// <summary>Sets the shading of the selected paragraphs.</summary>
    SetParagraphShading,
    /// <summary>Adds a tab stop to the selected paragraphs, replacing one at the same position.</summary>
    AddTabStop,
    /// <summary>Moves a tab stop.</summary>
    MoveTabStop,
    /// <summary>Deletes a tab stop. A stop inherited from the style is cleared in the paragraph.</summary>
    DeleteTabStop,
    /// <summary>Clears every tab stop of the selected paragraphs, inherited ones included.</summary>
    ClearTabStops,
    /// <summary>Sets the document's default tab spacing.</summary>
    SetDefaultTabStop,
    /// <summary>Sets the page margins of the selection's sections. Unset values are kept.</summary>
    SetMargins,
    /// <summary>Sets the page orientation of the selection's sections.</summary>
    SetOrientation,
    /// <summary>Sets the paper size of the selection's sections, given in portrait.</summary>
    SetPaperSize,
    /// <summary>Sets the text columns of the selection's sections.</summary>
    SetColumns,
    /// <summary>Sets the header and footer distances from the page edge. Unset values are kept.</summary>
    SetHeaderFooterDistance,
    /// <summary>Gives the first page its own header and footer.</summary>
    SetDifferentFirstPage,
    /// <summary>Gives odd and even pages their own headers and footers.</summary>
    SetDifferentOddEvenPages,
    /// <summary>Moves a text column boundary of the section.</summary>
    MoveColumnBoundary,
    /// <summary>Edits the header of the caret's page, creating it when the page has none.</summary>
    EditHeader,
    /// <summary>Edits the footer of the caret's page, creating it when the page has none.</summary>
    EditFooter,
    /// <summary>Leaves the header or footer being edited.</summary>
    CloseHeaderFooter,
    /// <summary>Inserts a page number.</summary>
    InsertPageNumber,
    /// <summary>Inserts a table at the selection.</summary>
    InsertTable,
    /// <summary>Inserts a picture at the selection.</summary>
    InsertPicture,
    /// <summary>Inserts a text box.</summary>
    InsertTextBox,
    /// <summary>Inserts a footnote at the selection.</summary>
    InsertFootnote,
    /// <summary>Inserts an endnote at the selection.</summary>
    InsertEndnote,
    /// <summary>Inserts a hyperlink.</summary>
    InsertHyperlink,
    /// <summary>Changes the hyperlink at the selection.</summary>
    EditHyperlink,
    /// <summary>Removes the hyperlink at the selection, keeping its text.</summary>
    RemoveHyperlink,
    /// <summary>Inserts a field. With a selection and an empty code, wraps the selection.</summary>
    InsertField,
    /// <summary>Inserts rows above or below the selection.</summary>
    InsertRows,
    /// <summary>Inserts columns left or right of the selection.</summary>
    InsertColumns,
    /// <summary>Deletes the selected rows.</summary>
    DeleteRows,
    /// <summary>Deletes the selected columns.</summary>
    DeleteColumns,
    /// <summary>Deletes the table.</summary>
    DeleteTable,
    /// <summary>Deletes the selected cells, shifting the rest.</summary>
    DeleteCells,
    /// <summary>Merges a rectangular block of selected cells.</summary>
    MergeCells,
    /// <summary>Splits the selected cells.</summary>
    SplitCells,
    /// <summary>Splits the table above the selected row.</summary>
    SplitTable,
    /// <summary>Gives the selected rows equal heights.</summary>
    DistributeRows,
    /// <summary>Gives the selected columns equal widths.</summary>
    DistributeColumns,
    /// <summary>Moves the selected rows up or down.</summary>
    MoveRows,
    /// <summary>Fits the column widths.</summary>
    AutoFitColumns,
    /// <summary>Sets the vertical alignment of the selected cells.</summary>
    SetCellAlignment,
    /// <summary>Sets the shading of the selected cells.</summary>
    SetCellShading,
    /// <summary>Sets borders on the selected cells.</summary>
    SetCellBorders,
    /// <summary>Sets the margins of the selected cells. Unset values are kept.</summary>
    SetCellMargins,
    /// <summary>Repeats the selected rows as a header row on each page.</summary>
    SetHeaderRow,
    /// <summary>Sets the height of the selected rows.</summary>
    SetRowHeight,
    /// <summary>Lets the selected rows break across pages.</summary>
    SetRowCanBreak,
    /// <summary>Sets the width of the selected columns.</summary>
    SetColumnWidth,
    /// <summary>Moves a column boundary of the table.</summary>
    MoveTableColumnBoundary,
    /// <summary>Moves a row boundary of the table.</summary>
    MoveTableRowBoundary,
    /// <summary>Sets the table width.</summary>
    SetTableWidth,
    /// <summary>Sets the table's alignment on the page.</summary>
    SetTableAlignment,
    /// <summary>Sets the table's indent from the margin.</summary>
    SetTableIndent,
    /// <summary>Applies a table style.</summary>
    SetTableStyle,
    /// <summary>Sets which parts of the table style apply. Unset values are kept.</summary>
    SetTableStyleOptions,
    /// <summary>Resizes the selected picture or text box. Unset values are kept.</summary>
    SetObjectSize,
    /// <summary>Restores the selected picture's original size.</summary>
    ResetObjectSize,
    /// <summary>Sets how text wraps around the selected object.</summary>
    SetObjectWrap,
    /// <summary>Locks the selected floating object's anchor.</summary>
    SetAnchorLocked,
    /// <summary>Sets the alternative text of the selected object.</summary>
    SetAltText,
    /// <summary>Deletes the selected object.</summary>
    DeleteObject,
    /// <summary>Changes a field's code or display text.</summary>
    SetField,
    /// <summary>Sets computed field results. A token joins the update step a FieldsUpdateRequested event started.</summary>
    SetFieldResults,
    /// <summary>Updates fields (F9); raises FieldsUpdateRequested for fields the application computes.</summary>
    UpdateFields,
    /// <summary>Locks or unlocks fields against updates.</summary>
    SetFieldsLocked,
    /// <summary>Replaces fields with their results.</summary>
    UnlinkFields,
    /// <summary>Shows field codes instead of results. A view setting: no undo step.</summary>
    ShowFieldCodes,
    /// <summary>Sets what design mode shows for each field code. A view setting.</summary>
    SetFieldLabels,
    /// <summary>Inserts a content control.</summary>
    InsertControl,
    /// <summary>Changes a content control. Unset values are kept.</summary>
    SetControl,
    /// <summary>Removes a content control.</summary>
    RemoveControl,
    /// <summary>Fills a content control with text, a picture or a document.</summary>
    FillControl,
    /// <summary>Checks or clears a checkbox control.</summary>
    SetCheckbox,
    /// <summary>Wraps the selection in a repeating section.</summary>
    WrapInRepeatingSection,
    /// <summary>Removes a field, or a control with its content.</summary>
    RemoveElement,
    /// <summary>Replaces text in element codes, tags or titles (case-sensitive, every occurrence).</summary>
    ReplaceInElements,
    /// <summary>Marks elements with wavy lines and a tooltip. A view setting.</summary>
    MarkElements,
    /// <summary>Sets how placeholders look in design mode. A view setting.</summary>
    SetPlaceholderLooks,
    /// <summary>Fills the selected picture placeholder, or every unfilled one with a key.</summary>
    FillPlaceholder,
    /// <summary>Writes a custom XML data part.</summary>
    WriteDataPart,
    /// <summary>Removes a custom XML data part.</summary>
    RemoveDataPart,
    /// <summary>Turns change tracking on or off.</summary>
    SetTrackChanges,
    /// <summary>Sets how revisions and comments show. A view setting.</summary>
    SetMarkupView,
    /// <summary>Accepts a revision. A move resolves as a pair.</summary>
    AcceptRevision,
    /// <summary>Rejects a revision. A move resolves as a pair.</summary>
    RejectRevision,
    /// <summary>Accepts the revisions in the selection.</summary>
    AcceptRevisionsInSelection,
    /// <summary>Rejects the revisions in the selection.</summary>
    RejectRevisionsInSelection,
    /// <summary>Accepts every revision, or one author's.</summary>
    AcceptAllRevisions,
    /// <summary>Rejects every revision, or one author's.</summary>
    RejectAllRevisions,
    /// <summary>Selects the next or previous revision.</summary>
    SelectRevision,
    /// <summary>Adds a comment on the selection, or the word at the caret.</summary>
    AddComment,
    /// <summary>Replies to a comment thread.</summary>
    ReplyToComment,
    /// <summary>Changes a comment's text.</summary>
    EditComment,
    /// <summary>Deletes a reply, or the whole thread when it is the first comment.</summary>
    DeleteComment,
    /// <summary>Marks a thread done or open.</summary>
    ResolveComment,
    /// <summary>Highlights a thread. A view setting.</summary>
    SetActiveComment,
    /// <summary>Selects and reveals a comment's range.</summary>
    SelectComment,
    /// <summary>Sets the colour an author's revisions and comments show in. A view setting.</summary>
    SetAuthorColor,
    /// <summary>Sets the zoom. A view setting.</summary>
    SetZoom,
    /// <summary>Shows or hides formatting marks, gridlines and text boundaries. Unset values are kept.</summary>
    SetViewOptions,
    /// <summary>Switches between editing and design mode.</summary>
    SetMode,
}
