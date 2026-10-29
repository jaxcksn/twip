using System.Text.Json.Nodes;
using Twip.Wire;

namespace Twip;

/// <summary>
/// One command as a value, for <see cref="EditorSession.BatchAsync"/>, gestures and drops:
/// <c>EditorOperation.SetBold(true)</c>, <c>EditorOperation.InsertTable(3, 2)</c>. There is one factory per command.
/// </summary>
[System.Text.Json.Serialization.JsonConverter(typeof(EditorOperationConverter))]
public abstract partial record EditorOperation
{
    private protected EditorOperation() { }

    internal abstract CommandId Id { get; }

    internal abstract JsonObject ToWire();

    /// <summary>Adds a comment on the selection, or the word at the caret.</summary>
    /// <param name="body">The comment text; a string converts to plain text.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation AddComment(RichText body) => Op(CommandId.AddComment, new Arg("value", Node(body)));

    /// <summary>Inserts text at the selection, replacing it. Consecutive typing joins one undo step.</summary>
    /// <param name="text">The text (non-empty).</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation InsertText(string text) => Op(CommandId.InsertText, new Arg("value", Node(text)));

    /// <summary>Starts a new paragraph at the selection (Enter).</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation InsertParagraph() => Op(CommandId.InsertParagraph);

    /// <summary>Inserts a line break (Shift+Enter).</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation InsertLineBreak() => Op(CommandId.InsertLineBreak);

    /// <summary>Inserts a page break (Ctrl+Enter). Only in the body, not in a table.</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation InsertPageBreak() => Op(CommandId.InsertPageBreak);

    /// <summary>Inserts a column break. Only in the body, not in a table.</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation InsertColumnBreak() => Op(CommandId.InsertColumnBreak);

    /// <summary>Inserts a section break. Only in the body, not in a table.</summary>
    /// <param name="kind">Where the new section starts.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation InsertSectionBreak(SectionBreakKind kind) => Op(CommandId.InsertSectionBreak, new Arg("value", Node(kind)));

    /// <summary>Inserts a tab character. Joins the typing undo step.</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation InsertTab() => Op(CommandId.InsertTab);

    /// <summary>Inserts one character, optionally in a symbol font.</summary>
    /// <param name="character">The character (exactly one).</param>
    /// <param name="font">The font to draw it in, such as Symbol or Wingdings.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation InsertSymbol(string character, string? font = null) => Op(CommandId.InsertSymbol, new Arg("char", Node(character)), new Arg("font", font is { } font_ ? Node(font_) : null, Optional: true));

    /// <summary>Deletes the selection, or one unit next to the caret. Joins the typing undo step.</summary>
    /// <param name="direction">Which side of the caret to delete.</param>
    /// <param name="unit">A character (the default) or a word.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation Delete(Direction direction, DeleteUnit? unit = null) => Op(CommandId.Delete, new Arg("direction", Node(direction)), new Arg("unit", unit is { } unit_ ? Node(unit_) : null, Optional: true));

    /// <summary>Cuts the selection to the clipboard.</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation Cut() => Op(CommandId.Cut);

    /// <summary>Copies the selection to the clipboard. Works when read only.</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation Copy() => Op(CommandId.Copy);

    /// <summary>Pastes from the clipboard.</summary>
    /// <param name="mode">Formatted (the default) or plain text.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation Paste(PasteMode? mode = null) => Op(CommandId.Paste, new Arg("as", mode is { } mode_ ? Node(mode_) : null, Optional: true));

    /// <summary>Pastes the given text as plain text.</summary>
    /// <param name="text">The text.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation PasteText(string text) => Op(CommandId.PasteText, new Arg("value", Node(text)));

    /// <summary>Undoes the last step.</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation Undo() => Op(CommandId.Undo);

    /// <summary>Redoes the last undone step.</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation Redo() => Op(CommandId.Redo);

    /// <summary>Repeats the last action, or redoes (F4).</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation Repeat() => Op(CommandId.Repeat);

    /// <summary>Selects the whole story.</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SelectAll() => Op(CommandId.SelectAll);

    /// <summary>Extends the selection to a unit around it.</summary>
    /// <param name="unit">The unit. The table units need a table.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation Select(SelectionUnit unit) => Op(CommandId.Select, new Arg("value", Node(unit)));

    /// <summary>Moves or extends the selection.</summary>
    /// <param name="by">The unit to move by.</param>
    /// <param name="direction">The direction.</param>
    /// <param name="extend">Extend the selection instead of moving the caret.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation MoveSelection(MoveUnit by, Direction direction, bool? extend = null) => Op(CommandId.MoveSelection, new Arg("by", Node(by)), new Arg("direction", Node(direction)), new Arg("extend", extend is { } extend_ ? Node(extend_) : null, Optional: true));

    /// <summary>Moves to the next or previous table cell. Next from the last cell adds a row.</summary>
    /// <param name="direction">The direction.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation MoveToCell(NavigationDirection direction) => Op(CommandId.MoveToCell, new Arg("value", Node(direction)));

    /// <summary>Sets the selection mode (F8 extends, Ctrl+Shift+F8 selects a block).</summary>
    /// <param name="mode">The mode.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetSelectionMode(TextSelectionMode mode) => Op(CommandId.SetSelectionMode, new Arg("value", Node(mode)));

    /// <summary>Returns to an earlier edit location (Shift+F5).</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation GoBack() => Op(CommandId.GoBack);

    /// <summary>Leaves a header or footer, a text box, or an object selection.</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation Escape() => Op(CommandId.Escape);

    /// <summary>Selects the next or previous floating object.</summary>
    /// <param name="direction">The direction.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SelectNextObject(NavigationDirection direction) => Op(CommandId.SelectNextObject, new Arg("value", Node(direction)));

    /// <summary>Moves the caret to the start of a page.</summary>
    /// <param name="page">The page number, from 1.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation GoToPage(int page) => Op(CommandId.GoToPage, new Arg("value", Node(page)));

    /// <summary>Sets or clears bold on the selection.</summary>
    /// <param name="on">Whether bold is on.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetBold(bool on) => Op(CommandId.SetBold, new Arg("value", Node(on)));

    /// <summary>Sets or clears italic on the selection.</summary>
    /// <param name="on">Whether italic is on.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetItalic(bool on) => Op(CommandId.SetItalic, new Arg("value", Node(on)));

    /// <summary>Sets the underline style of the selection.</summary>
    /// <param name="style">The style; None removes the underline.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetUnderline(UnderlineStyle style) => Op(CommandId.SetUnderline, new Arg("value", Node(style)));

    /// <summary>Sets or clears strikethrough on the selection.</summary>
    /// <param name="on">Whether strikethrough is on.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetStrikethrough(bool on) => Op(CommandId.SetStrikethrough, new Arg("value", Node(on)));

    /// <summary>Sets or clears double strikethrough on the selection.</summary>
    /// <param name="on">Whether double strikethrough is on.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetDoubleStrikethrough(bool on) => Op(CommandId.SetDoubleStrikethrough, new Arg("value", Node(on)));

    /// <summary>Sets or clears subscript on the selection.</summary>
    /// <param name="on">Whether subscript is on.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetSubscript(bool on) => Op(CommandId.SetSubscript, new Arg("value", Node(on)));

    /// <summary>Sets or clears superscript on the selection.</summary>
    /// <param name="on">Whether superscript is on.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetSuperscript(bool on) => Op(CommandId.SetSuperscript, new Arg("value", Node(on)));

    /// <summary>Sets or clears small capitals on the selection.</summary>
    /// <param name="on">Whether small capitals is on.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetSmallCaps(bool on) => Op(CommandId.SetSmallCaps, new Arg("value", Node(on)));

    /// <summary>Sets or clears all capitals on the selection.</summary>
    /// <param name="on">Whether all capitals is on.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetAllCaps(bool on) => Op(CommandId.SetAllCaps, new Arg("value", Node(on)));

    /// <summary>Sets or clears hidden text on the selection.</summary>
    /// <param name="on">Whether hidden text is on.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetHidden(bool on) => Op(CommandId.SetHidden, new Arg("value", Node(on)));

    /// <summary>Sets the font family of the selection.</summary>
    /// <param name="family">The family name (non-empty).</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetFont(string family) => Op(CommandId.SetFont, new Arg("value", Node(family)));

    /// <summary>Sets the font size of the selection.</summary>
    /// <param name="size">The size, 1 to 1638 pt in half-point steps.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetFontSize(Length size) => Op(CommandId.SetFontSize, new Arg("value", Node(size)));

    /// <summary>Grows the font to the next size in the standard list.</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation GrowFont() => Op(CommandId.GrowFont);

    /// <summary>Shrinks the font to the previous size in the standard list.</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation ShrinkFont() => Op(CommandId.ShrinkFont);

    /// <summary>Sets the text colour of the selection.</summary>
    /// <param name="color">The colour, or <see cref="ColorValue.Auto"/>.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetFontColor(ColorValue color) => Op(CommandId.SetFontColor, new Arg("value", Node(color)));

    /// <summary>Sets the highlight colour of the selection.</summary>
    /// <param name="color">The colour; <see cref="HighlightColor.None"/> removes the highlight.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetHighlight(HighlightColor color) => Op(CommandId.SetHighlight, new Arg("value", NoneAsNull(color)));

    /// <summary>Sets the spacing between characters.</summary>
    /// <param name="spacing">The spacing, -1584 to 1584 pt.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetCharacterSpacing(Length spacing) => Op(CommandId.SetCharacterSpacing, new Arg("value", Node(spacing)));

    /// <summary>Raises or lowers the text.</summary>
    /// <param name="position">The offset, -1584 to 1584 pt; positive raises.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetCharacterPosition(Length position) => Op(CommandId.SetCharacterPosition, new Arg("value", Node(position)));

    /// <summary>Removes direct character formatting from the selection.</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation ClearFormatting() => Op(CommandId.ClearFormatting);

    /// <summary>Changes the case of the selected text.</summary>
    /// <param name="to">The case to change to.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation ChangeCase(CaseChange to) => Op(CommandId.ChangeCase, new Arg("value", Node(to)));

    /// <summary>Holds the selection's formatting for PasteFormatting (the format painter).</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation CopyFormatting() => Op(CommandId.CopyFormatting);

    /// <summary>Applies the held formatting to the selection.</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation PasteFormatting() => Op(CommandId.PasteFormatting);

    /// <summary>Sets the alignment of the selected paragraphs.</summary>
    /// <param name="alignment">The alignment.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetAlignment(ParagraphAlignment alignment) => Op(CommandId.SetAlignment, new Arg("value", Node(alignment)));

    /// <summary>Sets the line spacing of the selected paragraphs.</summary>
    /// <param name="spacing">The spacing.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetLineSpacing(LineSpacing spacing) => Op(CommandId.SetLineSpacing, Arg.Flatten(spacing));

    /// <summary>Sets the space before and after the selected paragraphs. Unset values are kept.</summary>
    /// <param name="before">The space before, 0 to 1584 pt, or automatic.</param>
    /// <param name="after">The space after, 0 to 1584 pt, or automatic.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetParagraphSpacing(SpacingValue? before = null, SpacingValue? after = null) => Op(CommandId.SetParagraphSpacing, new Arg("before", before is { } before_ ? Node(before_) : null, Optional: true), new Arg("after", after is { } after_ ? Node(after_) : null, Optional: true));

    /// <summary>Sets the indents of the selected paragraphs. Unset values are kept.</summary>
    /// <param name="options">The indents to change.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetIndents(IndentOptions options) => Op(CommandId.SetIndents, Arg.Flatten(options));

    /// <summary>Increases the left indent to the next tab stop.</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation IncreaseIndent() => Op(CommandId.IncreaseIndent);

    /// <summary>Decreases the left indent to the previous tab stop.</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation DecreaseIndent() => Op(CommandId.DecreaseIndent);

    /// <summary>Applies a paragraph style to the selected paragraphs.</summary>
    /// <param name="styleId">An existing paragraph style.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetParagraphStyle(StyleId styleId) => Op(CommandId.SetParagraphStyle, new Arg("value", Node(styleId)));

    /// <summary>Makes the selected paragraphs a list, or removes them from one.</summary>
    /// <param name="kind">The list kind; <see cref="ListKind.None"/> removes the list.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetList(ListKind kind) => Op(CommandId.SetList, new Arg("value", NoneAsNull(kind)));

    /// <summary>Moves the selected list items a level in or out.</summary>
    /// <param name="change">The direction.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetListLevel(LevelChange change) => Op(CommandId.SetListLevel, new Arg("value", Node(change)));

    /// <summary>Sets how the selected paragraphs break across pages. Unset values are kept.</summary>
    /// <param name="options">The flow settings to change.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetParagraphFlow(ParagraphFlowOptions options) => Op(CommandId.SetParagraphFlow, Arg.Flatten(options));

    /// <summary>Sets a border preset on the selected paragraphs (a half-point single line).</summary>
    /// <param name="preset">Box, Bottom or None.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetParagraphBorders(ParagraphBorders preset) => Op(CommandId.SetParagraphBorders, new Arg("value", Node(preset)));

    /// <summary>Sets the shading of the selected paragraphs.</summary>
    /// <param name="color">The colour; <see cref="ColorValue.None"/> removes the shading.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetParagraphShading(ColorValue color) => Op(CommandId.SetParagraphShading, new Arg("value", NoneAsNull(color)));

    /// <summary>Adds a tab stop to the selected paragraphs, replacing one at the same position.</summary>
    /// <param name="position">The position from the paragraph's left text edge, 0 to 1584 pt.</param>
    /// <param name="alignment">The alignment.</param>
    /// <param name="leader">The leader.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation AddTabStop(Length position, TabAlignment alignment = TabAlignment.Left, TabLeader leader = TabLeader.None) => Op(CommandId.AddTabStop, new Arg("position", Node(position)), new Arg("alignment", Node(alignment)), new Arg("leader", Node(leader)));

    /// <summary>Moves a tab stop.</summary>
    /// <param name="from">The stop's position (within 0.5 pt).</param>
    /// <param name="to">The new position.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation MoveTabStop(Length from, Length to) => Op(CommandId.MoveTabStop, new Arg("from", Node(from)), new Arg("to", Node(to)));

    /// <summary>Deletes a tab stop. A stop inherited from the style is cleared in the paragraph.</summary>
    /// <param name="position">The stop's position.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation DeleteTabStop(Length position) => Op(CommandId.DeleteTabStop, new Arg("value", Node(position)));

    /// <summary>Clears every tab stop of the selected paragraphs, inherited ones included.</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation ClearTabStops() => Op(CommandId.ClearTabStops);

    /// <summary>Sets the document's default tab spacing.</summary>
    /// <param name="spacing">The spacing, 1 to 1584 pt.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetDefaultTabStop(Length spacing) => Op(CommandId.SetDefaultTabStop, new Arg("value", Node(spacing)));

    /// <summary>Sets the page margins of the selection's sections. Unset values are kept.</summary>
    /// <param name="options">The margins to change.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetMargins(MarginOptions options) => Op(CommandId.SetMargins, Arg.Flatten(options));

    /// <summary>Sets the page orientation of the selection's sections.</summary>
    /// <param name="orientation">The orientation.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetOrientation(PageOrientation orientation) => Op(CommandId.SetOrientation, new Arg("value", Node(orientation)));

    /// <summary>Sets the paper size of the selection's sections, given in portrait.</summary>
    /// <param name="width">The width, 36 to 1584 pt.</param>
    /// <param name="height">The height, 36 to 1584 pt.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetPaperSize(Length width, Length height) => Op(CommandId.SetPaperSize, new Arg("width", Node(width)), new Arg("height", Node(height)));

    /// <summary>Sets the text columns of the selection's sections.</summary>
    /// <param name="count">The number of columns, 1 to 45.</param>
    /// <param name="spacing">The space between columns.</param>
    /// <param name="separator">Whether a line separates the columns.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetColumns(int count, Length? spacing = null, bool? separator = null) => Op(CommandId.SetColumns, new Arg("count", Node(count)), new Arg("spacing", spacing is { } spacing_ ? Node(spacing_) : null, Optional: true), new Arg("separator", separator is { } separator_ ? Node(separator_) : null, Optional: true));

    /// <summary>Sets the header and footer distances from the page edge. Unset values are kept.</summary>
    /// <param name="header">The header distance.</param>
    /// <param name="footer">The footer distance.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetHeaderFooterDistance(Length? header = null, Length? footer = null) => Op(CommandId.SetHeaderFooterDistance, new Arg("header", header is { } header_ ? Node(header_) : null, Optional: true), new Arg("footer", footer is { } footer_ ? Node(footer_) : null, Optional: true));

    /// <summary>Gives the first page its own header and footer.</summary>
    /// <param name="on">Whether the first page differs.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetDifferentFirstPage(bool on) => Op(CommandId.SetDifferentFirstPage, new Arg("value", Node(on)));

    /// <summary>Gives odd and even pages their own headers and footers.</summary>
    /// <param name="on">Whether odd and even pages differ.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetDifferentOddEvenPages(bool on) => Op(CommandId.SetDifferentOddEvenPages, new Arg("value", Node(on)));

    /// <summary>Moves a text column boundary of the section.</summary>
    /// <param name="column">The column index.</param>
    /// <param name="to">The new position.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation MoveColumnBoundary(int column, Length to) => Op(CommandId.MoveColumnBoundary, new Arg("column", Node(column)), new Arg("to", Node(to)));

    /// <summary>Edits the header of the caret's page, creating it when the page has none.</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation EditHeader() => Op(CommandId.EditHeader);

    /// <summary>Edits the footer of the caret's page, creating it when the page has none.</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation EditFooter() => Op(CommandId.EditFooter);

    /// <summary>Leaves the header or footer being edited.</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation CloseHeaderFooter() => Op(CommandId.CloseHeaderFooter);

    /// <summary>Inserts a page number.</summary>
    /// <param name="format">What to show.</param>
    /// <param name="where">At the caret (the default), or in the header or footer.</param>
    /// <param name="alignment">The alignment in a header or footer.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation InsertPageNumber(PageNumberFormat format, PageNumberPlacement? where = null, PageNumberAlignment? alignment = null) => Op(CommandId.InsertPageNumber, new Arg("format", Node(format)), new Arg("where", where is { } where_ ? Node(where_) : null, Optional: true), new Arg("alignment", alignment is { } alignment_ ? Node(alignment_) : null, Optional: true));

    /// <summary>Inserts a table at the selection.</summary>
    /// <param name="rows">The number of rows, 1 to 32767.</param>
    /// <param name="columns">The number of columns, 1 to 63.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation InsertTable(int rows, int columns) => Op(CommandId.InsertTable, new Arg("rows", Node(rows)), new Arg("columns", Node(columns)));

    /// <summary>Inserts a picture at the selection.</summary>
    /// <param name="data">The image (PNG, JPEG, GIF, BMP or TIFF).</param>
    /// <param name="altText">The alternative text.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation InsertPicture(ReadOnlyMemory<byte> data, string? altText = null) => Op(CommandId.InsertPicture, new Arg("data", Node(data)), new Arg("altText", altText is { } altText_ ? Node(altText_) : null, Optional: true));

    /// <summary>Inserts a text box.</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation InsertTextBox() => Op(CommandId.InsertTextBox);

    /// <summary>Inserts a footnote at the selection.</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation InsertFootnote() => Op(CommandId.InsertFootnote);

    /// <summary>Inserts an endnote at the selection.</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation InsertEndnote() => Op(CommandId.InsertEndnote);

    /// <summary>Inserts a hyperlink.</summary>
    /// <param name="address">The address (non-empty).</param>
    /// <param name="text">The text; by default the selection, or the address.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation InsertHyperlink(string address, string? text = null) => Op(CommandId.InsertHyperlink, new Arg("address", Node(address)), new Arg("text", text is { } text_ ? Node(text_) : null, Optional: true));

    /// <summary>Changes the hyperlink at the selection.</summary>
    /// <param name="address">The new address.</param>
    /// <param name="text">The new text.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation EditHyperlink(string? address = null, string? text = null) => Op(CommandId.EditHyperlink, new Arg("address", address is { } address_ ? Node(address_) : null, Optional: true), new Arg("text", text is { } text_ ? Node(text_) : null, Optional: true));

    /// <summary>Removes the hyperlink at the selection, keeping its text.</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation RemoveHyperlink() => Op(CommandId.RemoveHyperlink);

    /// <summary>Inserts a field. With a selection and an empty code, wraps the selection.</summary>
    /// <param name="options">The field.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation InsertField(FieldOptions options) => Op(CommandId.InsertField, Arg.Flatten(options));

    /// <summary>Inserts rows above or below the selection.</summary>
    /// <param name="where">Above or below.</param>
    /// <param name="count">The number of rows (1 by default).</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation InsertRows(RowPosition where, int? count = null) => Op(CommandId.InsertRows, new Arg("where", Node(where)), new Arg("count", count is { } count_ ? Node(count_) : null, Optional: true));

    /// <summary>Inserts columns left or right of the selection.</summary>
    /// <param name="where">Left or right.</param>
    /// <param name="count">The number of columns (1 by default).</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation InsertColumns(ColumnPosition where, int? count = null) => Op(CommandId.InsertColumns, new Arg("where", Node(where)), new Arg("count", count is { } count_ ? Node(count_) : null, Optional: true));

    /// <summary>Deletes the selected rows.</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation DeleteRows() => Op(CommandId.DeleteRows);

    /// <summary>Deletes the selected columns.</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation DeleteColumns() => Op(CommandId.DeleteColumns);

    /// <summary>Deletes the table.</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation DeleteTable() => Op(CommandId.DeleteTable);

    /// <summary>Deletes the selected cells, shifting the rest.</summary>
    /// <param name="shift">Shift cells up or left.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation DeleteCells(CellShift shift) => Op(CommandId.DeleteCells, new Arg("value", Node(shift)));

    /// <summary>Merges a rectangular block of selected cells.</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation MergeCells() => Op(CommandId.MergeCells);

    /// <summary>Splits the selected cells.</summary>
    /// <param name="columns">The number of columns, 1 to 63.</param>
    /// <param name="rows">The number of rows (1 by default).</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SplitCells(int columns, int? rows = null) => Op(CommandId.SplitCells, new Arg("columns", Node(columns)), new Arg("rows", rows is { } rows_ ? Node(rows_) : null, Optional: true));

    /// <summary>Splits the table above the selected row.</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SplitTable() => Op(CommandId.SplitTable);

    /// <summary>Gives the selected rows equal heights.</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation DistributeRows() => Op(CommandId.DistributeRows);

    /// <summary>Gives the selected columns equal widths.</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation DistributeColumns() => Op(CommandId.DistributeColumns);

    /// <summary>Moves the selected rows up or down.</summary>
    /// <param name="direction">The direction.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation MoveRows(VerticalDirection direction) => Op(CommandId.MoveRows, new Arg("value", Node(direction)));

    /// <summary>Fits the column widths.</summary>
    /// <param name="to">To the contents, to the window, or fixed.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation AutoFitColumns(AutoFitBehavior to) => Op(CommandId.AutoFitColumns, new Arg("value", Node(to)));

    /// <summary>Sets the vertical alignment of the selected cells.</summary>
    /// <param name="vertical">The alignment.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetCellAlignment(CellAlignment vertical) => Op(CommandId.SetCellAlignment, new Arg("value", Node(vertical)));

    /// <summary>Sets the shading of the selected cells.</summary>
    /// <param name="color">The colour; <see cref="ColorValue.None"/> removes the shading.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetCellShading(ColorValue color) => Op(CommandId.SetCellShading, new Arg("value", NoneAsNull(color)));

    /// <summary>Sets borders on the selected cells.</summary>
    /// <param name="edges">Which edges.</param>
    /// <param name="border">The line; none for no border.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetCellBorders(BorderEdges edges, CellBorder? border = null) => Op(CommandId.SetCellBorders, new Arg("edges", Node(edges)), new Arg("border", border is { } border_ ? Node(border_) : null, Optional: true));

    /// <summary>Sets the margins of the selected cells. Unset values are kept.</summary>
    /// <param name="options">The margins to change.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetCellMargins(CellMarginOptions options) => Op(CommandId.SetCellMargins, Arg.Flatten(options));

    /// <summary>Repeats the selected rows as a header row on each page.</summary>
    /// <param name="on">Whether they repeat.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetHeaderRow(bool on) => Op(CommandId.SetHeaderRow, new Arg("value", Node(on)));

    /// <summary>Sets the height of the selected rows.</summary>
    /// <param name="height">The height, or <see cref="RowHeight.Auto"/>.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetRowHeight(RowHeight height) => Op(CommandId.SetRowHeight, new Arg("value", Node(height)));

    /// <summary>Lets the selected rows break across pages.</summary>
    /// <param name="on">Whether they can break.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetRowCanBreak(bool on) => Op(CommandId.SetRowCanBreak, new Arg("value", Node(on)));

    /// <summary>Sets the width of the selected columns.</summary>
    /// <param name="width">The width, 1 to 1584 pt.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetColumnWidth(Length width) => Op(CommandId.SetColumnWidth, new Arg("value", Node(width)));

    /// <summary>Moves a column boundary of the table.</summary>
    /// <param name="index">The boundary index.</param>
    /// <param name="to">The new position.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation MoveTableColumnBoundary(int index, Length to) => Op(CommandId.MoveTableColumnBoundary, new Arg("index", Node(index)), new Arg("to", Node(to)));

    /// <summary>Moves a row boundary of the table.</summary>
    /// <param name="index">The boundary index.</param>
    /// <param name="to">The new position.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation MoveTableRowBoundary(int index, Length to) => Op(CommandId.MoveTableRowBoundary, new Arg("index", Node(index)), new Arg("to", Node(to)));

    /// <summary>Sets the table width.</summary>
    /// <param name="width">The width.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetTableWidth(TableWidth width) => Op(CommandId.SetTableWidth, Arg.Flatten(width));

    /// <summary>Sets the table's alignment on the page.</summary>
    /// <param name="alignment">The alignment.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetTableAlignment(TableAlignment alignment) => Op(CommandId.SetTableAlignment, new Arg("value", Node(alignment)));

    /// <summary>Sets the table's indent from the margin.</summary>
    /// <param name="indent">The indent, -1584 to 1584 pt.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetTableIndent(Length indent) => Op(CommandId.SetTableIndent, new Arg("value", Node(indent)));

    /// <summary>Applies a table style.</summary>
    /// <param name="styleId">An existing table style.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetTableStyle(StyleId styleId) => Op(CommandId.SetTableStyle, new Arg("value", Node(styleId)));

    /// <summary>Sets which parts of the table style apply. Unset values are kept.</summary>
    /// <param name="options">The options to change.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetTableStyleOptions(TableStyleOptionsUpdate options) => Op(CommandId.SetTableStyleOptions, Arg.Flatten(options));

    /// <summary>Resizes the selected picture or text box. Unset values are kept.</summary>
    /// <param name="width">The width, 1 to 1584 pt.</param>
    /// <param name="height">The height, 1 to 1584 pt.</param>
    /// <param name="lockAspect">Keep the aspect ratio.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetObjectSize(Length? width = null, Length? height = null, bool? lockAspect = null) => Op(CommandId.SetObjectSize, new Arg("width", width is { } width_ ? Node(width_) : null, Optional: true), new Arg("height", height is { } height_ ? Node(height_) : null, Optional: true), new Arg("lockAspect", lockAspect is { } lockAspect_ ? Node(lockAspect_) : null, Optional: true));

    /// <summary>Restores the selected picture's original size.</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation ResetObjectSize() => Op(CommandId.ResetObjectSize);

    /// <summary>Sets how text wraps around the selected object.</summary>
    /// <param name="wrap">The wrapping.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetObjectWrap(ObjectWrap wrap) => Op(CommandId.SetObjectWrap, new Arg("value", Node(wrap)));

    /// <summary>Locks the selected floating object's anchor.</summary>
    /// <param name="on">Whether the anchor is locked.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetAnchorLocked(bool on) => Op(CommandId.SetAnchorLocked, new Arg("value", Node(on)));

    /// <summary>Sets the alternative text of the selected object.</summary>
    /// <param name="text">The text.</param>
    /// <param name="decorative">Marks the object as decorative.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetAltText(string text, bool? decorative = null) => Op(CommandId.SetAltText, new Arg("text", Node(text)), new Arg("decorative", decorative is { } decorative_ ? Node(decorative_) : null, Optional: true));

    /// <summary>Deletes the selected object.</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation DeleteObject() => Op(CommandId.DeleteObject);

    /// <summary>Changes a field's code or display text.</summary>
    /// <param name="id">The field.</param>
    /// <param name="code">The new code.</param>
    /// <param name="display">The new display text.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetField(ElementId id, string? code = null, RichText? display = null) => Op(CommandId.SetField, new Arg("id", Node(id)), new Arg("code", code is { } code_ ? Node(code_) : null, Optional: true), new Arg("display", display is { } display_ ? Node(display_) : null, Optional: true));

    /// <summary>Sets computed field results. A token joins the update step a FieldsUpdateRequested event started.</summary>
    /// <param name="results">The results.</param>
    /// <param name="token">The token from FieldsUpdateRequested.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetFieldResults(IReadOnlyList<FieldResult> results, long? token = null) => Op(CommandId.SetFieldResults, new Arg("results", Node(results)), new Arg("token", token is { } token_ ? Node(token_) : null, Optional: true));

    /// <summary>Updates fields (F9); raises FieldsUpdateRequested for fields the application computes.</summary>
    /// <param name="ids">The fields; by default the selection's.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation UpdateFields(IReadOnlyList<ElementId>? ids = null) => Op(CommandId.UpdateFields, new Arg("ids", ids is { } ids_ ? Node(ids_) : null, Optional: true));

    /// <summary>Locks or unlocks fields against updates.</summary>
    /// <param name="locked">Whether the fields are locked.</param>
    /// <param name="ids">The fields; by default the selection's.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetFieldsLocked(bool locked, IReadOnlyList<ElementId>? ids = null) => Op(CommandId.SetFieldsLocked, new Arg("locked", Node(locked)), new Arg("ids", ids is { } ids_ ? Node(ids_) : null, Optional: true));

    /// <summary>Replaces fields with their results.</summary>
    /// <param name="ids">The fields; by default the selection's.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation UnlinkFields(IReadOnlyList<ElementId>? ids = null) => Op(CommandId.UnlinkFields, new Arg("ids", ids is { } ids_ ? Node(ids_) : null, Optional: true));

    /// <summary>Shows field codes instead of results. A view setting: no undo step.</summary>
    /// <param name="show">Whether codes show.</param>
    /// <param name="ids">The fields; by default all of them.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation ShowFieldCodes(bool show, IReadOnlyList<ElementId>? ids = null) => Op(CommandId.ShowFieldCodes, new Arg("show", Node(show)), new Arg("ids", ids is { } ids_ ? Node(ids_) : null, Optional: true));

    /// <summary>Sets what design mode shows for each field code. A view setting.</summary>
    /// <param name="labels">Labels keyed by field code.</param>
    /// <param name="merge">Merge with the current labels instead of replacing them.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetFieldLabels(IReadOnlyDictionary<string, string> labels, bool? merge = null) => Op(CommandId.SetFieldLabels, new Arg("labels", Node(labels)), new Arg("merge", merge is { } merge_ ? Node(merge_) : null, Optional: true));

    /// <summary>Inserts a content control.</summary>
    /// <param name="options">The control.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation InsertControl(InsertControlOptions options) => Op(CommandId.InsertControl, Arg.Flatten(options));

    /// <summary>Changes a content control. Unset values are kept.</summary>
    /// <param name="changes">The changes.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetControl(ControlChanges changes) => Op(CommandId.SetControl, Arg.Flatten(changes));

    /// <summary>Removes a content control.</summary>
    /// <param name="id">The control.</param>
    /// <param name="keepContent">Keep its content in the document.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation RemoveControl(ElementId id, bool? keepContent = null) => Op(CommandId.RemoveControl, new Arg("id", Node(id)), new Arg("keepContent", keepContent is { } keepContent_ ? Node(keepContent_) : null, Optional: true));

    /// <summary>Fills a content control with text, a picture or a document.</summary>
    /// <param name="id">The control.</param>
    /// <param name="content">The content.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation FillControl(ElementId id, ControlContent content) => Op(CommandId.FillControl, new Arg("id", Node(id)), Arg.Flatten(content));

    /// <summary>Checks or clears a checkbox control.</summary>
    /// <param name="id">The control.</param>
    /// <param name="isChecked">Whether it is checked.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetCheckbox(ElementId id, bool isChecked) => Op(CommandId.SetCheckbox, new Arg("id", Node(id)), new Arg("checked", Node(isChecked)));

    /// <summary>Wraps the selection in a repeating section.</summary>
    /// <param name="tag">The tag.</param>
    /// <param name="title">The title.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation WrapInRepeatingSection(string? tag = null, string? title = null) => Op(CommandId.WrapInRepeatingSection, new Arg("tag", tag is { } tag_ ? Node(tag_) : null, Optional: true), new Arg("title", title is { } title_ ? Node(title_) : null, Optional: true));

    /// <summary>Removes a field, or a control with its content.</summary>
    /// <param name="id">The element.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation RemoveElement(ElementId id) => Op(CommandId.RemoveElement, new Arg("value", Node(id)));

    /// <summary>Replaces text in element codes, tags or titles (case-sensitive, every occurrence).</summary>
    /// <param name="find">The text to find.</param>
    /// <param name="replace">The replacement.</param>
    /// <param name="in">Where to replace.</param>
    /// <param name="kind">Only fields or only controls.</param>
    /// <param name="ids">Only these elements.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation ReplaceInElements(string find, string replace, ElementTextField @in, ElementCategory? kind = null, IReadOnlyList<ElementId>? ids = null) => Op(CommandId.ReplaceInElements, new Arg("find", Node(find)), new Arg("replace", Node(replace)), new Arg("in", Node(@in)), new Arg("kind", kind is { } kind_ ? Node(kind_) : null, Optional: true), new Arg("ids", ids is { } ids_ ? Node(ids_) : null, Optional: true));

    /// <summary>Marks elements with wavy lines and a tooltip. A view setting.</summary>
    /// <param name="marks">The marks.</param>
    /// <param name="clear">Clear the existing marks first.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation MarkElements(IReadOnlyList<ElementMark> marks, bool? clear = null) => Op(CommandId.MarkElements, new Arg("marks", Node(marks)), new Arg("clear", clear is { } clear_ ? Node(clear_) : null, Optional: true));

    /// <summary>Sets how placeholders look in design mode. A view setting.</summary>
    /// <param name="looks">The looks.</param>
    /// <param name="merge">Merge with the current looks.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetPlaceholderLooks(IReadOnlyList<PlaceholderLook> looks, bool? merge = null) => Op(CommandId.SetPlaceholderLooks, new Arg("looks", Node(looks)), new Arg("merge", merge is { } merge_ ? Node(merge_) : null, Optional: true));

    /// <summary>Fills the selected picture placeholder, or every unfilled one with a key.</summary>
    /// <param name="image">The picture.</param>
    /// <param name="key">The placeholder key.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation FillPlaceholder(ReadOnlyMemory<byte> image, string? key = null) => Op(CommandId.FillPlaceholder, new Arg("image", Node(image)), new Arg("key", key is { } key_ ? Node(key_) : null, Optional: true));

    /// <summary>Writes a custom XML data part.</summary>
    /// <param name="namespace">The part's namespace.</param>
    /// <param name="xml">The XML (well-formed).</param>
    /// <param name="itemId">The part's item id, when several share the namespace.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation WriteDataPart(string @namespace, string xml, string? itemId = null) => Op(CommandId.WriteDataPart, new Arg("namespace", Node(@namespace)), new Arg("xml", Node(xml)), new Arg("itemId", itemId is { } itemId_ ? Node(itemId_) : null, Optional: true));

    /// <summary>Removes a custom XML data part.</summary>
    /// <param name="namespace">The part's namespace.</param>
    /// <param name="itemId">The part's item id.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation RemoveDataPart(string @namespace, string? itemId = null) => Op(CommandId.RemoveDataPart, new Arg("namespace", Node(@namespace)), new Arg("itemId", itemId is { } itemId_ ? Node(itemId_) : null, Optional: true));

    /// <summary>Turns change tracking on or off.</summary>
    /// <param name="on">Whether changes are tracked.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetTrackChanges(bool on) => Op(CommandId.SetTrackChanges, new Arg("value", Node(on)));

    /// <summary>Sets how revisions and comments show. A view setting.</summary>
    /// <param name="view">The view.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetMarkupView(MarkupView view) => Op(CommandId.SetMarkupView, new Arg("value", Node(view)));

    /// <summary>Accepts a revision. A move resolves as a pair.</summary>
    /// <param name="id">The revision.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation AcceptRevision(RevisionId id) => Op(CommandId.AcceptRevision, new Arg("value", Node(id)));

    /// <summary>Rejects a revision. A move resolves as a pair.</summary>
    /// <param name="id">The revision.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation RejectRevision(RevisionId id) => Op(CommandId.RejectRevision, new Arg("value", Node(id)));

    /// <summary>Accepts the revisions in the selection.</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation AcceptRevisionsInSelection() => Op(CommandId.AcceptRevisionsInSelection);

    /// <summary>Rejects the revisions in the selection.</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation RejectRevisionsInSelection() => Op(CommandId.RejectRevisionsInSelection);

    /// <summary>Accepts every revision, or one author's.</summary>
    /// <param name="author">Only this author's revisions.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation AcceptAllRevisions(string? author = null) => Op(CommandId.AcceptAllRevisions, new Arg("author", author is { } author_ ? Node(author_) : null, Optional: true));

    /// <summary>Rejects every revision, or one author's.</summary>
    /// <param name="author">Only this author's revisions.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation RejectAllRevisions(string? author = null) => Op(CommandId.RejectAllRevisions, new Arg("author", author is { } author_ ? Node(author_) : null, Optional: true));

    /// <summary>Selects the next or previous revision.</summary>
    /// <param name="direction">The direction.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SelectRevision(NavigationDirection direction) => Op(CommandId.SelectRevision, new Arg("value", Node(direction)));

    /// <summary>Replies to a comment thread.</summary>
    /// <param name="to">The thread.</param>
    /// <param name="body">The reply.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation ReplyToComment(CommentId to, RichText body) => Op(CommandId.ReplyToComment, new Arg("to", Node(to)), new Arg("body", Node(body)));

    /// <summary>Changes a comment's text.</summary>
    /// <param name="id">The comment.</param>
    /// <param name="body">The new text.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation EditComment(CommentId id, RichText body) => Op(CommandId.EditComment, new Arg("id", Node(id)), new Arg("body", Node(body)));

    /// <summary>Deletes a reply, or the whole thread when it is the first comment.</summary>
    /// <param name="id">The comment.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation DeleteComment(CommentId id) => Op(CommandId.DeleteComment, new Arg("value", Node(id)));

    /// <summary>Marks a thread done or open.</summary>
    /// <param name="id">The thread.</param>
    /// <param name="done">Whether it is done.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation ResolveComment(CommentId id, bool done) => Op(CommandId.ResolveComment, new Arg("id", Node(id)), new Arg("done", Node(done)));

    /// <summary>Highlights a thread. A view setting.</summary>
    /// <param name="id">The thread, or null for none.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetActiveComment(CommentId? id) => Op(CommandId.SetActiveComment, new Arg("value", Nullable(id)));

    /// <summary>Selects and reveals a comment's range.</summary>
    /// <param name="id">The comment.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SelectComment(CommentId id) => Op(CommandId.SelectComment, new Arg("value", Node(id)));

    /// <summary>Sets the colour an author's revisions and comments show in. A view setting.</summary>
    /// <param name="author">The author name.</param>
    /// <param name="color">The colour.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetAuthorColor(string author, ColorValue color) => Op(CommandId.SetAuthorColor, new Arg("author", Node(author)), new Arg("color", Node(color)));

    /// <summary>Sets the zoom. A view setting.</summary>
    /// <param name="zoom">A percentage (10 to 500), or a fit.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetZoom(ZoomLevel zoom) => Op(CommandId.SetZoom, new Arg("value", Node(zoom)));

    /// <summary>Shows or hides formatting marks, gridlines and text boundaries. Unset values are kept.</summary>
    /// <param name="formattingMarks">Formatting marks.</param>
    /// <param name="gridlines">Table gridlines.</param>
    /// <param name="textBoundaries">Text boundaries.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetViewOptions(bool? formattingMarks = null, bool? gridlines = null, bool? textBoundaries = null) => Op(CommandId.SetViewOptions, new Arg("formattingMarks", formattingMarks is { } formattingMarks_ ? Node(formattingMarks_) : null, Optional: true), new Arg("gridlines", gridlines is { } gridlines_ ? Node(gridlines_) : null, Optional: true), new Arg("textBoundaries", textBoundaries is { } textBoundaries_ ? Node(textBoundaries_) : null, Optional: true));

    /// <summary>Switches between editing and design mode.</summary>
    /// <param name="mode">The mode.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetMode(EditorMode mode) => Op(CommandId.SetMode, new Arg("value", Node(mode)));
}
