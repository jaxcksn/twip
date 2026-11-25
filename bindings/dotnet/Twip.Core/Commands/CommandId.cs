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
    /// <summary>Inserts a floating shape at the caret's paragraph, in front of the text, with the theme's default style, and selects it.</summary>
    InsertShape,
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
    /// <summary>Places the selected floating object. Unset values are kept.</summary>
    SetObjectPosition,
    /// <summary>Sets the sides text wraps on around the selected floating object and its distance from the text. Unset values are kept.</summary>
    SetObjectWrapOptions,
    /// <summary>Sets the alternative text of the selected object.</summary>
    SetAltText,
    /// <summary>Deletes the selected object.</summary>
    DeleteObject,
    /// <summary>Changes the selected shape's preset geometry. Its text, fill, outline and effects stay; its adjust values reset.</summary>
    SetShapePreset,
    /// <summary>Sets one of the selected shape's adjust values.</summary>
    SetShapeAdjust,
    /// <summary>Sets the selected objects' fill; <see cref="FillValue.FromStyle"/> gives them the style reference's fill again.</summary>
    SetShapeFill,
    /// <summary>Sets the selected objects' outline. Unset values are kept.</summary>
    SetShapeLine,
    /// <summary>Sets the selected objects' shadows, glow, soft edge and blur: each effect is kept, removed or set.</summary>
    SetShapeEffects,
    /// <summary>Sets the selected shape's text insets, anchor, autofit, wrapping and direction. Unset values are kept.</summary>
    SetShapeText,
    /// <summary>Sets the selected picture's transparency, greyscale, brightness and contrast. Unset values are kept.</summary>
    SetPictureAdjust,
    /// <summary>Rotates the selected objects.</summary>
    SetObjectRotation,
    /// <summary>Flips the selected objects.</summary>
    FlipObject,
    /// <summary>Moves the selected floating object in the paint order, or behind or in front of the text.</summary>
    ArrangeObject,
    /// <summary>Groups the selected floating objects (two or more, in one story) and selects the group.</summary>
    GroupObjects,
    /// <summary>Ungroups the selected group and selects its children.</summary>
    UngroupObjects,
    /// <summary>Selects the next or previous child of the selected group. Not an edit: no undo step.</summary>
    SelectGroupChild,
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
    /// <summary>Rewrites a font family's name in the document itself.</summary>
    ReplaceFont,
    /// <summary>Marks elements with wavy lines and a tooltip. A view setting.</summary>
    MarkElements,
    /// <summary>Sets the tag prefixes whose picture controls keep their pictures. A view setting.</summary>
    SetProtectedPictureControls,
    /// <summary>Selects a picture control and raises <see cref="EditorSession.PictureControlActivated"/>.</summary>
    ActivatePictureControl,
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
    /// <summary>Sets what a click on the ruler adds; answers the tab type picker. A view setting.</summary>
    SetRulerTabType,
    /// <summary>Asks for the tab type picker.</summary>
    ShowTabTypes,
    /// <summary>Asks for the options of the tab stop nearest the caret.</summary>
    ShowTabStop,
    /// <summary>Switches AutoCorrect rules on or off, and replaces its lists. A session setting.</summary>
    SetAutoCorrect,
    /// <summary>Puts back what was typed before the correction at the caret.</summary>
    RevertAutoCorrect,
    /// <summary>Reverts the correction at the caret, and never makes it again.</summary>
    NeverAutoCorrect,
    /// <summary>Asks for the options of the correction at the caret.</summary>
    ShowAutoCorrectOptions,
    /// <summary>Positions the caret's table so text wraps around it, or puts it back in the text. Unset values are kept.</summary>
    SetTablePosition,
    /// <summary>Dismisses the popover asked for; with none open, nothing.</summary>
    ClosePopover,
    /// <summary>Which fields are sub-template references.</summary>
    SetSubTemplateRules,
    /// <summary>Shows references' sub-template bodies in their place, or their fields.</summary>
    SetSubTemplateView,
    /// <summary>Edits a sub-template's body in this editor.</summary>
    EditSubTemplate,
    /// <summary>Back to the template from a sub-template.</summary>
    CloseSubTemplate,
    /// <summary>A new sub-template.</summary>
    CreateSubTemplate,
    /// <summary>Renames a sub-template.</summary>
    RenameSubTemplate,
    /// <summary>Removes a sub-template.</summary>
    RemoveSubTemplate,
    /// <summary>Says the formatting at the caret to screen readers, as an announcement. Changes nothing.</summary>
    DescribeFormatting,
    /// <summary>The application's own field types, never read as bookmark references.</summary>
    SetApplicationFieldTypes,
    /// <summary>The template's body from a DOCX's body; its sub-templates, data parts and settings stay.</summary>
    ReplaceBody,
    /// <summary>Changes a style to the selection's formatting.</summary>
    UpdateStyleToSelection,
    /// <summary>Deletes a style the document made.</summary>
    DeleteStyle,
    /// <summary>Sets a style's list flags.</summary>
    SetStyleMeta,
    /// <summary>Sets the document's default formatting.</summary>
    SetDocumentDefaults,
    /// <summary>Sets the selected paragraphs' border lines.</summary>
    SetParagraphBorderLines,
    /// <summary>Shades the selected paragraphs or text.</summary>
    SetShading,
    /// <summary>Sets the properties of the frame the selection is in.</summary>
    SetFrame,
    /// <summary>Removes the frame the selection is in.</summary>
    RemoveFrame,
    /// <summary>Defines the selection's list levels.</summary>
    SetListDefinition,
    /// <summary>Starts a new list at a value, or continues the previous one.</summary>
    SetNumberingValue,
    /// <summary>Applies a list gallery entry.</summary>
    ApplyListEntry,
    /// <summary>Restarts the list at this paragraph.</summary>
    RestartNumbering,
    /// <summary>Continues the previous list from this paragraph.</summary>
    ContinueNumbering,
    /// <summary>Adjusts the selection's list level's indents.</summary>
    AdjustListIndents,
    /// <summary>Sets or removes the selected text's border.</summary>
    SetTextBorder,
    /// <summary>Sets the page borders.</summary>
    SetPageBorders,
    /// <summary>Turns automatic hyphenation on or off for the document.</summary>
    SetAutoHyphenation,
    /// <summary>Sets the hyphenation zone, which documents in compatibility mode 14 and earlier use.</summary>
    SetHyphenationZone,
    /// <summary>Sets the most consecutive lines that may end in a hyphen.</summary>
    SetConsecutiveHyphenLimit,
    /// <summary>Hyphenates words in capitals, or not.</summary>
    SetHyphenateCaps,
    /// <summary>Applies a character style to the selected text, or removes it.</summary>
    ApplyCharacterStyle,
    /// <summary>Sets the scale, kerning and OpenType features.</summary>
    SetTypography,
    /// <summary>Sets the text's language and whether spelling is checked.</summary>
    SetLanguage,
    /// <summary>Sets emboss, engrave, outline and shadow.</summary>
    SetCharacterEffects,
    /// <summary>Sets the underline's colour.</summary>
    SetUnderlineColor,
    /// <summary>Sets the outline level and whether paragraphs open collapsed.</summary>
    SetOutline,
    /// <summary>Sets the paragraphs' text direction.</summary>
    SetParagraphTextDirection,
    /// <summary>Sets the section's page number format, first number and chapter numbers.</summary>
    SetPageNumbering,
    /// <summary>Sets where text sits between the section's top and bottom margins.</summary>
    SetPageVerticalAlignment,
    /// <summary>Numbers the section's lines, or stops.</summary>
    SetLineNumbering,
    /// <summary>Sets the break the section starts with.</summary>
    SetSectionBreakType,
    /// <summary>Sets the section's text direction.</summary>
    SetSectionTextDirection,
    /// <summary>Sets how the document's pages pair and where the gutter is.</summary>
    SetMultiplePages,
    /// <summary>Links a header or footer to the section before's, or gives it its own.</summary>
    SetHeaderFooterLinked,
    /// <summary>Empties a header or footer.</summary>
    RemoveHeaderFooter,
    /// <summary>Inserts an alignment tab.</summary>
    InsertAlignmentTab,
    /// <summary>Inserts a footnote or an endnote, numbered or with its own mark.</summary>
    InsertNote,
    /// <summary>Sets footnote or endnote options.</summary>
    SetNoteProperties,
    /// <summary>Converts footnotes and endnotes.</summary>
    ConvertNotes,
    /// <summary>Selects a bookmark's text (Go To).</summary>
    GoToBookmark,
    /// <summary>Bookmarks the selection.</summary>
    InsertBookmark,
    /// <summary>Deletes a bookmark.</summary>
    DeleteBookmark,
    /// <summary>Sets the selected cells' preferred width, text direction, wrapping and fitting.</summary>
    SetCellProperties,
    /// <summary>Hides rows and sets their exceptions.</summary>
    SetRowProperties,
    /// <summary>Sets the space between a table's cells.</summary>
    SetTableCellSpacing,
    /// <summary>Sets a table's alternative text.</summary>
    SetTableAltText,
    /// <summary>Sets the table style new tables take.</summary>
    SetDefaultTableStyle,
    /// <summary>Crops the selected picture.</summary>
    SetPictureCrop,
    /// <summary>Replaces the selected picture's image.</summary>
    ChangePicture,
    /// <summary>Aligns or distributes the selected floating objects.</summary>
    AlignObjects,
    /// <summary>Sets the document's properties.</summary>
    SetDocumentProperties,
    /// <summary>Sets or removes a document variable.</summary>
    SetDocumentVariable,
    /// <summary>Inserts a legacy form field.</summary>
    InsertFormField,
    /// <summary>Sets a legacy form field's options.</summary>
    SetFormField,
    /// <summary>Fills in a legacy form field.</summary>
    FillFormField,
    /// <summary>Shades legacy form fields, or not.</summary>
    SetFormFieldShading,
    /// <summary>Restrict Editing: sets, starts or stops the document's protection.</summary>
    SetDocumentProtection,
    /// <summary>Makes the selection an editable range.</summary>
    AddEditableRange,
    /// <summary>Removes the editable ranges at the selection.</summary>
    RemoveEditableRanges,
    /// <summary>Sets the file name FILENAME shows.</summary>
    SetFileName,
    /// <summary>Sets the selection's paragraph's drop cap.</summary>
    SetDropCap,
    /// <summary>Sets the text's fill, outline, shadow, glow and reflection.</summary>
    SetTextEffects,
    /// <summary>Sets the document's page colour: none, a colour, a gradient, a pattern or a picture.</summary>
    SetPageColor,
    /// <summary>Sets the document's watermark, text or picture, in every header.</summary>
    SetWatermark,
    /// <summary>Removes the document's watermark.</summary>
    RemoveWatermark,
    /// <summary>Converts the selected paragraphs to a table.</summary>
    ConvertTextToTable,
    /// <summary>Converts the selected rows, or the whole table, to text.</summary>
    ConvertTableToText,
    /// <summary>Inserts a style separator (Ctrl+Alt+Enter): a hidden paragraph mark that runs the paragraph before it into the one after, each keeping its own style. One undo step.</summary>
    InsertStyleSeparator,
    /// <summary>Alt+X: the hexadecimal code before the caret becomes its character, or the character before the caret (or the selected one) becomes its code. One undo step.</summary>
    ToggleCharacterCode,
    /// <summary>F2: the selected text moves to the place the next click or arrow keys choose, when Enter is pressed (<see cref="SelectionSummary.PendingMove"/> while it waits); Esc or any other command gives up.</summary>
    MoveText,
    /// <summary>Shift+F2: as MoveText, copying the text.</summary>
    CopyText,
    /// <summary>Selects the whole next or previous field in the story, outermost fields only (F11, Shift+F11). Not an edit: no undo step.</summary>
    SelectField,
    /// <summary>Each selected run's size a point larger, up to 1638 (Ctrl+]).</summary>
    GrowFontOnePoint,
    /// <summary>Each selected run's size a point smaller, down to 1 (Ctrl+[).</summary>
    ShrinkFontOnePoint,
    /// <summary>Clear All Formatting: the selected paragraphs take the default paragraph style and lose their direct paragraph and character formatting; highlight and revision marks stay.</summary>
    ClearAllFormatting,
    /// <summary>A hanging indent to the next default tab stop: the left indent moves, the first line stays (Ctrl+T).</summary>
    IncreaseHangingIndent,
    /// <summary>The hanging indent back to the previous default tab stop (Ctrl+Shift+T).</summary>
    DecreaseHangingIndent,
    /// <summary>Reset Paragraph: removes direct paragraph formatting, numbering included; the style stays (Ctrl+Q).</summary>
    ClearParagraphFormatting,
    /// <summary>Moves the selected paragraphs above the one before or below the one after (Alt+Shift+Up, Down); tracked, a move.</summary>
    MoveParagraphs,
    /// <summary>Groups again the objects the last UngroupObjects ungrouped, wherever they moved.</summary>
    RegroupObjects,
    /// <summary>Shows the selected floating object's wrap polygon for editing (<see cref="ViewState.EditingWrapPoints"/>): dragging a vertex moves it, dragging an edge adds one, Ctrl+click removes one, each one undo step that makes the wrap tight. Esc ends it. Not an edit: no undo step.</summary>
    EditWrapPoints,
    /// <summary>Fits the selected text to a width (Fit Text), or lets it take its own.</summary>
    SetFitText,
    /// <summary>Sets the document's default formatting with any character and paragraph formatting operations but frames, as one undo step.</summary>
    SetDefaultFormatting,
    /// <summary>Makes a style (New Style); its id is the command's result.</summary>
    CreateStyle,
    /// <summary>Changes a style (Modify Style); a built-in style the document lacks joins it.</summary>
    ModifyStyle,
    /// <summary>Sets the rows and columns in each of the table's bands.</summary>
    SetTableBandSizes,
    /// <summary>Selects ranges of text and blocks of cells of one story, a discontiguous selection when there are several. Not an edit: no undo step.</summary>
    SetSelection,
    /// <summary>Shrink Selection (Shift+F8), in extend mode: the selection one level smaller around where extending started. Not an edit: no undo step.</summary>
    ShrinkSelection,
    /// <summary>Select Text with Similar Formatting: every stretch of text in the story formatted as the selection's first character. Not an edit: no undo step.</summary>
    SelectSimilarFormatting,
    /// <summary>Selects every use of a paragraph or character style in the main document. Not an edit: no undo step.</summary>
    SelectStyleInstances,
    /// <summary>Sets the table's default cell margins (Table Options).</summary>
    SetTableCellMargins,
    /// <summary>Inserts blank cells beside the selected ones, shifting them right or down (Insert Cells).</summary>
    InsertCells,
    /// <summary>Sets the pointer's table tool: none, the Draw Table pen or the eraser. Not an edit: no undo step.</summary>
    SetTableTool,
    /// <summary>Sets the line the Draw Table pen draws. Not an edit: no undo step.</summary>
    SetTablePen,
    /// <summary>A Draw Table pen stroke on a page.</summary>
    DrawTableStroke,
    /// <summary>The table eraser on a page: merges the cells either side of a border.</summary>
    EraseTableBorder,
    /// <summary>Names the selected object.</summary>
    SetObjectName,
    /// <summary>Sets the selected object's alternative text title.</summary>
    SetObjectTitle,
    /// <summary>Hides or shows a floating object.</summary>
    SetObjectHidden,
    /// <summary>Selects a floating object or a group's object, as the Selection Pane does. Not an edit: no undo step.</summary>
    SelectObject,
    /// <summary>Moves a floating object in its page's stack.</summary>
    RestackObject,
    /// <summary>Whether resizing the selected object keeps its aspect ratio.</summary>
    SetLockAspectRatio,
    /// <summary>Sets the selected pictures' recolouring.</summary>
    SetPictureRecolor,
    /// <summary>Reset Picture, or Reset Picture and Size.</summary>
    ResetPicture,
    /// <summary>Sets the selected floating object's wrap polygon.</summary>
    SetWrapPoints,
    /// <summary>Makes the selected floating object's wrap polygon again from its outline.</summary>
    ResetWrapPoints,
    /// <summary>Links the selected text box to an empty one (Create Link).</summary>
    LinkTextBox,
    /// <summary>Starts linking the selected text box with the pointer. Not an edit: no undo step.</summary>
    BeginTextBoxLink,
    /// <summary>Breaks the link from the selected text box to the next (Break Link).</summary>
    UnlinkTextBox,
    /// <summary>Turns Select Objects on or off. Not an edit: no undo step.</summary>
    SetSelectObjects,
    /// <summary>Promote (Alt+Shift+Left): each selected paragraph a level up; body text takes the level of the heading before it (Heading 1 when none).</summary>
    PromoteOutline,
    /// <summary>Demote (Alt+Shift+Right): each selected heading a level down; Heading 9 and body text stay.</summary>
    DemoteOutline,
    /// <summary>Promote to Heading 1: the selected paragraphs take Heading 1, their direct outline level removed.</summary>
    PromoteToHeading1,
    /// <summary>Demote to Body Text: the selected paragraphs take the default paragraph style, their direct outline level removed.</summary>
    DemoteToBodyText,
    /// <summary>A navigation pane's Promote on one heading.</summary>
    PromoteHeading,
    /// <summary>A navigation pane's Demote on one heading.</summary>
    DemoteHeading,
    /// <summary>Moves a heading with its content before another heading, or after that heading's content, optionally at a new level; the caret goes to its start.</summary>
    MoveHeading,
    /// <summary>Deletes a heading and its content.</summary>
    DeleteHeading,
    /// <summary>An empty heading at a heading's level just before it, the caret in it.</summary>
    InsertHeadingBefore,
    /// <summary>An empty heading at a heading's level after its content, the caret in it.</summary>
    InsertHeadingAfter,
    /// <summary>An empty heading a level below a heading, after its content, the caret in it.</summary>
    InsertSubheading,
    /// <summary>Selects a heading and its content. Not an edit: no undo step.</summary>
    SelectHeadingContent,
    /// <summary>Sets the separator, continuation separator or continuation notice of footnotes or endnotes, or back to the built-in one.</summary>
    SetNoteSeparator,
    /// <summary>Lets an editor edit the selection, or stops them editing the editable ranges reaching it.</summary>
    SetEditableRangeEditor,
    /// <summary>Selects the next or previous editable range the author may edit (Find Next Region I Can Edit). Not an edit: no undo step.</summary>
    SelectNextEditableRange,
    /// <summary>Selects every editable range the author may edit (Show All Regions I Can Edit). Not an edit: no undo step.</summary>
    SelectEditableRanges,
    /// <summary>The groups the author is in; editable ranges for them let the author edit. Kept for the documents opened after.</summary>
    SetEditingGroups,
    /// <summary>Selects the next or previous legacy form field (Tab and Shift+Tab where forms are filled in). Not an edit: no undo step.</summary>
    SelectNextFormField,
    /// <summary>Adds an item to a repeating section: a copy of one of its items, with its content; the caret goes to the copy's start.</summary>
    AddRepeatingItem,
    /// <summary>Which kinds of markup show (Show Markup); unset values are kept. Not an edit: no undo step.</summary>
    SetShowMarkup,
    /// <summary>The reviewers whose revisions and comments don't show, by name; empty: all reviewers show. Not an edit: no undo step.</summary>
    SetHiddenReviewers,
    /// <summary>Where revisions show in All Markup. Not an edit: no undo step.</summary>
    SetBalloonMode,
    /// <summary>How revisions draw (Track Changes Options); unset values are kept. Not an edit: no undo step.</summary>
    SetMarkupStyle,
    /// <summary>Whether tracking records moves as moves and formatting changes; unset values are kept. Saved.</summary>
    SetTrackOptions,
    /// <summary>Accepts every revision and turns tracking off (Accept All Changes and Stop Tracking).</summary>
    AcceptAllAndStopTracking,
    /// <summary>Deletes every comment, resolved ones too, or the threads Show Markup shows.</summary>
    DeleteAllComments,
    /// <summary>Pastes the paste just made again another way, in place of it (the Paste Options button).</summary>
    Repaste,
    /// <summary>Hides the Paste Options button of the paste just made. Not an edit: no undo step.</summary>
    DismissPasteOptions,
    /// <summary>Changes the cut, copy and paste options. A session setting, never saved.</summary>
    SetPasteOptions,
    /// <summary>Inserts a document's content in place of the selection (Insert Text from File).</summary>
    InsertFile,
    /// <summary>Go To: a page, section, line, bookmark, comment, note, field, table, graphic, equation, object or heading. Not an edit: no undo step.</summary>
    GoTo,
    /// <summary>A table of contents in place of the selection.</summary>
    InsertTableOfContents,
    /// <summary>A table of figures of a caption label's captions, in place of the selection.</summary>
    InsertTableOfFigures,
    /// <summary>Word's Manual Table: typed entries in a building block control.</summary>
    InsertManualTableOfContents,
    /// <summary>Updates a table of contents or figures: its page numbers, or every entry again.</summary>
    UpdateTableOfContents,
    /// <summary>Updates every table of contents and of figures but the locked ones.</summary>
    UpdateAllTablesOfContents,
    /// <summary>Removes a table of contents, with its building block control and title.</summary>
    RemoveTableOfContents,
    /// <summary>Marks a table of contents entry after the selection (Mark Entry).</summary>
    MarkTableOfContentsEntry,
    /// <summary>A caption for the selected table, picture or object (or the caret's paragraph).</summary>
    InsertCaption,
    /// <summary>The caption every table inserted from now on gets (AutoCaption). A session setting, never saved.</summary>
    SetAutoCaption,
    /// <summary>A cross-reference to an item the cross-reference targets list.</summary>
    InsertCrossReference,
}
