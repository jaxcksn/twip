using Twip.Wire;

namespace Twip;

/// <summary>
/// The session's commands: one method per command. Each acts on the selection, enters the session's ordered queue,
/// and completes once the change has committed and <see cref="EditorSession.State"/> is at its revision.
/// </summary>
/// <remarks>
/// A command fails with <see cref="EditorErrorCode.ReadOnly"/> when it changes the document and the session is read
/// only, with <see cref="EditorErrorCode.Unavailable"/> when the selection doesn't allow it, and with
/// <see cref="EditorErrorCode.InvalidArgument"/> for an out-of-range argument.
/// </remarks>
public sealed class EditorCommands
{
    private readonly EditorSession _session;

    internal EditorCommands(EditorSession session) => _session = session;

    /// <summary>Adds a comment on the selection, or the word at the caret.</summary>
    /// <param name="body">The comment text, with formatting.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>The new comment's id, once the change has committed.</returns>
    public Task<CommentId> AddCommentAsync(RichText body, CancellationToken ct = default) => _session.RunAsync(EditorOperation.AddComment(body), CommitValue.CommentId, ct);

    /// <summary>Adds a plain-text comment on the selection, or the word at the caret.</summary>
    /// <param name="body">The comment text.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>The new comment's id, once the change has committed.</returns>
    public Task<CommentId> AddCommentAsync(string body, CancellationToken ct = default) => _session.RunAsync(EditorOperation.AddComment(body), CommitValue.CommentId, ct);

    /// <summary>Inserts text at the selection, replacing it. Consecutive typing joins one undo step.</summary>
    /// <param name="text">The text (non-empty).</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task InsertTextAsync(string text, CancellationToken ct = default) => _session.RunAsync(EditorOperation.InsertText(text), ct);

    /// <summary>Starts a new paragraph at the selection (Enter).</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task InsertParagraphAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.InsertParagraph(), ct);

    /// <summary>Inserts a line break (Shift+Enter).</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task InsertLineBreakAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.InsertLineBreak(), ct);

    /// <summary>Inserts a page break (Ctrl+Enter). Only in the body, not in a table.</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task InsertPageBreakAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.InsertPageBreak(), ct);

    /// <summary>Inserts a column break. Only in the body, not in a table.</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task InsertColumnBreakAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.InsertColumnBreak(), ct);

    /// <summary>Inserts a section break. Only in the body, not in a table.</summary>
    /// <param name="kind">Where the new section starts.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task InsertSectionBreakAsync(SectionBreakKind kind, CancellationToken ct = default) => _session.RunAsync(EditorOperation.InsertSectionBreak(kind), ct);

    /// <summary>Inserts a tab character. Joins the typing undo step.</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task InsertTabAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.InsertTab(), ct);

    /// <summary>Inserts one character, optionally in a symbol font.</summary>
    /// <param name="character">The character (exactly one).</param>
    /// <param name="font">The font to draw it in, such as Symbol or Wingdings.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task InsertSymbolAsync(string character, string? font = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.InsertSymbol(character, font), ct);

    /// <summary>Deletes the selection, or one unit next to the caret. Joins the typing undo step.</summary>
    /// <param name="direction">Which side of the caret to delete.</param>
    /// <param name="unit">A character (the default) or a word.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task DeleteAsync(Direction direction, DeleteUnit? unit = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.Delete(direction, unit), ct);

    /// <summary>Cuts the selection to the clipboard.</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task CutAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.Cut(), ct);

    /// <summary>Copies the selection to the clipboard. Works when read only.</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task CopyAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.Copy(), ct);

    /// <summary>Pastes from the clipboard.</summary>
    /// <param name="mode">Formatted (the default) or plain text.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task PasteAsync(PasteMode? mode = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.Paste(mode), ct);

    /// <summary>Pastes the given text as plain text.</summary>
    /// <param name="text">The text.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task PasteTextAsync(string text, CancellationToken ct = default) => _session.RunAsync(EditorOperation.PasteText(text), ct);

    /// <summary>Undoes the last step.</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task UndoAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.Undo(), ct);

    /// <summary>Redoes the last undone step.</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task RedoAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.Redo(), ct);

    /// <summary>Repeats the last action, or redoes (F4).</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task RepeatAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.Repeat(), ct);

    /// <summary>Selects the whole story.</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SelectAllAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.SelectAll(), ct);

    /// <summary>Extends the selection to a unit around it.</summary>
    /// <param name="unit">The unit. The table units need a table.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SelectAsync(SelectionUnit unit, CancellationToken ct = default) => _session.RunAsync(EditorOperation.Select(unit), ct);

    /// <summary>Moves or extends the selection.</summary>
    /// <param name="by">The unit to move by.</param>
    /// <param name="direction">The direction.</param>
    /// <param name="extend">Extend the selection instead of moving the caret.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task MoveSelectionAsync(MoveUnit by, Direction direction, bool? extend = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.MoveSelection(by, direction, extend), ct);

    /// <summary>Moves to the next or previous table cell. Next from the last cell adds a row.</summary>
    /// <param name="direction">The direction.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task MoveToCellAsync(NavigationDirection direction, CancellationToken ct = default) => _session.RunAsync(EditorOperation.MoveToCell(direction), ct);

    /// <summary>Sets the selection mode (F8 extends, Ctrl+Shift+F8 selects a block).</summary>
    /// <param name="mode">The mode.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetSelectionModeAsync(TextSelectionMode mode, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetSelectionMode(mode), ct);

    /// <summary>Returns to an earlier edit location (Shift+F5).</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task GoBackAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.GoBack(), ct);

    /// <summary>Leaves a header or footer, a text box, or an object selection.</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task EscapeAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.Escape(), ct);

    /// <summary>Selects the next or previous floating object.</summary>
    /// <param name="direction">The direction.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SelectNextObjectAsync(NavigationDirection direction, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SelectNextObject(direction), ct);

    /// <summary>Moves the caret to the start of a page.</summary>
    /// <param name="page">The page number, from 1.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task GoToPageAsync(int page, CancellationToken ct = default) => _session.RunAsync(EditorOperation.GoToPage(page), ct);

    /// <summary>Sets or clears bold on the selection.</summary>
    /// <param name="on">Whether bold is on.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetBoldAsync(bool on, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetBold(on), ct);

    /// <summary>Sets or clears italic on the selection.</summary>
    /// <param name="on">Whether italic is on.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetItalicAsync(bool on, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetItalic(on), ct);

    /// <summary>Sets the underline style of the selection.</summary>
    /// <param name="style">The style; None removes the underline.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetUnderlineAsync(UnderlineStyle style, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetUnderline(style), ct);

    /// <summary>Sets or clears strikethrough on the selection.</summary>
    /// <param name="on">Whether strikethrough is on.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetStrikethroughAsync(bool on, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetStrikethrough(on), ct);

    /// <summary>Sets or clears double strikethrough on the selection.</summary>
    /// <param name="on">Whether double strikethrough is on.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetDoubleStrikethroughAsync(bool on, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetDoubleStrikethrough(on), ct);

    /// <summary>Sets or clears subscript on the selection.</summary>
    /// <param name="on">Whether subscript is on.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetSubscriptAsync(bool on, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetSubscript(on), ct);

    /// <summary>Sets or clears superscript on the selection.</summary>
    /// <param name="on">Whether superscript is on.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetSuperscriptAsync(bool on, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetSuperscript(on), ct);

    /// <summary>Sets or clears small capitals on the selection.</summary>
    /// <param name="on">Whether small capitals is on.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetSmallCapsAsync(bool on, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetSmallCaps(on), ct);

    /// <summary>Sets or clears all capitals on the selection.</summary>
    /// <param name="on">Whether all capitals is on.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetAllCapsAsync(bool on, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetAllCaps(on), ct);

    /// <summary>Sets or clears hidden text on the selection.</summary>
    /// <param name="on">Whether hidden text is on.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetHiddenAsync(bool on, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetHidden(on), ct);

    /// <summary>Sets the font family of the selection.</summary>
    /// <param name="family">The family name (non-empty).</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetFontAsync(string family, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetFont(family), ct);

    /// <summary>Sets the font size of the selection.</summary>
    /// <param name="size">The size, 1 to 1638 pt in half-point steps.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetFontSizeAsync(Length size, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetFontSize(size), ct);

    /// <summary>Grows the font to the next size in the standard list.</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task GrowFontAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.GrowFont(), ct);

    /// <summary>Shrinks the font to the previous size in the standard list.</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task ShrinkFontAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.ShrinkFont(), ct);

    /// <summary>Sets the text colour of the selection.</summary>
    /// <param name="color">The colour, or <see cref="ColorValue.Auto"/>.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetFontColorAsync(ColorValue color, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetFontColor(color), ct);

    /// <summary>Sets the highlight colour of the selection.</summary>
    /// <param name="color">The colour; <see cref="HighlightColor.None"/> removes the highlight.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetHighlightAsync(HighlightColor color, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetHighlight(color), ct);

    /// <summary>Sets the spacing between characters.</summary>
    /// <param name="spacing">The spacing, -1584 to 1584 pt.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetCharacterSpacingAsync(Length spacing, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetCharacterSpacing(spacing), ct);

    /// <summary>Raises or lowers the text.</summary>
    /// <param name="position">The offset, -1584 to 1584 pt; positive raises.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetCharacterPositionAsync(Length position, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetCharacterPosition(position), ct);

    /// <summary>Removes direct character formatting from the selection.</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task ClearFormattingAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.ClearFormatting(), ct);

    /// <summary>Changes the case of the selected text.</summary>
    /// <param name="to">The case to change to.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task ChangeCaseAsync(CaseChange to, CancellationToken ct = default) => _session.RunAsync(EditorOperation.ChangeCase(to), ct);

    /// <summary>Holds the selection's formatting for PasteFormatting (the format painter).</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task CopyFormattingAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.CopyFormatting(), ct);

    /// <summary>Applies the held formatting to the selection.</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task PasteFormattingAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.PasteFormatting(), ct);

    /// <summary>Sets the alignment of the selected paragraphs.</summary>
    /// <param name="alignment">The alignment.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetAlignmentAsync(ParagraphAlignment alignment, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetAlignment(alignment), ct);

    /// <summary>Sets the line spacing of the selected paragraphs.</summary>
    /// <param name="spacing">The spacing.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetLineSpacingAsync(LineSpacing spacing, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetLineSpacing(spacing), ct);

    /// <summary>Sets the space before and after the selected paragraphs. Unset values are kept.</summary>
    /// <param name="before">The space before, 0 to 1584 pt, or automatic.</param>
    /// <param name="after">The space after, 0 to 1584 pt, or automatic.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetParagraphSpacingAsync(SpacingValue? before = null, SpacingValue? after = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetParagraphSpacing(before, after), ct);

    /// <summary>Sets the indents of the selected paragraphs. Unset values are kept.</summary>
    /// <param name="options">The indents to change.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetIndentsAsync(IndentOptions options, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetIndents(options), ct);

    /// <summary>Increases the left indent to the next tab stop.</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task IncreaseIndentAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.IncreaseIndent(), ct);

    /// <summary>Decreases the left indent to the previous tab stop.</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task DecreaseIndentAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.DecreaseIndent(), ct);

    /// <summary>Applies a paragraph style to the selected paragraphs.</summary>
    /// <param name="styleId">An existing paragraph style.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetParagraphStyleAsync(StyleId styleId, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetParagraphStyle(styleId), ct);

    /// <summary>Makes the selected paragraphs a list, or removes them from one.</summary>
    /// <param name="kind">The list kind; <see cref="ListKind.None"/> removes the list.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetListAsync(ListKind kind, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetList(kind), ct);

    /// <summary>Moves the selected list items a level in or out.</summary>
    /// <param name="change">The direction.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetListLevelAsync(LevelChange change, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetListLevel(change), ct);

    /// <summary>Sets how the selected paragraphs break across pages. Unset values are kept.</summary>
    /// <param name="options">The flow settings to change.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetParagraphFlowAsync(ParagraphFlowOptions options, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetParagraphFlow(options), ct);

    /// <summary>Sets a border preset on the selected paragraphs (a half-point single line).</summary>
    /// <param name="preset">Box, Bottom or None.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetParagraphBordersAsync(ParagraphBorders preset, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetParagraphBorders(preset), ct);

    /// <summary>Sets the shading of the selected paragraphs.</summary>
    /// <param name="color">The colour; <see cref="ColorValue.None"/> removes the shading.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetParagraphShadingAsync(ColorValue color, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetParagraphShading(color), ct);

    /// <summary>Adds a tab stop to the selected paragraphs, replacing one at the same position.</summary>
    /// <param name="position">The position from the paragraph's left text edge, 0 to 1584 pt.</param>
    /// <param name="alignment">The alignment.</param>
    /// <param name="leader">The leader.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task AddTabStopAsync(Length position, TabAlignment alignment = TabAlignment.Left, TabLeader leader = TabLeader.None, CancellationToken ct = default) => _session.RunAsync(EditorOperation.AddTabStop(position, alignment, leader), ct);

    /// <summary>Moves a tab stop.</summary>
    /// <param name="from">The stop's position (within 0.5 pt).</param>
    /// <param name="to">The new position.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task MoveTabStopAsync(Length from, Length to, CancellationToken ct = default) => _session.RunAsync(EditorOperation.MoveTabStop(from, to), ct);

    /// <summary>Deletes a tab stop. A stop inherited from the style is cleared in the paragraph.</summary>
    /// <param name="position">The stop's position.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task DeleteTabStopAsync(Length position, CancellationToken ct = default) => _session.RunAsync(EditorOperation.DeleteTabStop(position), ct);

    /// <summary>Clears every tab stop of the selected paragraphs, inherited ones included.</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task ClearTabStopsAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.ClearTabStops(), ct);

    /// <summary>Sets the document's default tab spacing.</summary>
    /// <param name="spacing">The spacing, 1 to 1584 pt.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetDefaultTabStopAsync(Length spacing, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetDefaultTabStop(spacing), ct);

    /// <summary>Sets the page margins of the selection's sections. Unset values are kept.</summary>
    /// <param name="options">The margins to change.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetMarginsAsync(MarginOptions options, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetMargins(options), ct);

    /// <summary>Sets the page orientation of the selection's sections.</summary>
    /// <param name="orientation">The orientation.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetOrientationAsync(PageOrientation orientation, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetOrientation(orientation), ct);

    /// <summary>Sets the paper size of the selection's sections, given in portrait.</summary>
    /// <param name="width">The width, 36 to 1584 pt.</param>
    /// <param name="height">The height, 36 to 1584 pt.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetPaperSizeAsync(Length width, Length height, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetPaperSize(width, height), ct);

    /// <summary>Sets the text columns of the selection's sections.</summary>
    /// <param name="count">The number of columns, 1 to 45.</param>
    /// <param name="spacing">The space between columns.</param>
    /// <param name="separator">Whether a line separates the columns.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetColumnsAsync(int count, Length? spacing = null, bool? separator = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetColumns(count, spacing, separator), ct);

    /// <summary>Sets the header and footer distances from the page edge. Unset values are kept.</summary>
    /// <param name="header">The header distance.</param>
    /// <param name="footer">The footer distance.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetHeaderFooterDistanceAsync(Length? header = null, Length? footer = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetHeaderFooterDistance(header, footer), ct);

    /// <summary>Gives the first page its own header and footer.</summary>
    /// <param name="on">Whether the first page differs.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetDifferentFirstPageAsync(bool on, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetDifferentFirstPage(on), ct);

    /// <summary>Gives odd and even pages their own headers and footers.</summary>
    /// <param name="on">Whether odd and even pages differ.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetDifferentOddEvenPagesAsync(bool on, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetDifferentOddEvenPages(on), ct);

    /// <summary>Moves a text column boundary of the section.</summary>
    /// <param name="column">The column index.</param>
    /// <param name="to">The new position.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task MoveColumnBoundaryAsync(int column, Length to, CancellationToken ct = default) => _session.RunAsync(EditorOperation.MoveColumnBoundary(column, to), ct);

    /// <summary>Edits the header of the caret's page, creating it when the page has none.</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task EditHeaderAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.EditHeader(), ct);

    /// <summary>Edits the footer of the caret's page, creating it when the page has none.</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task EditFooterAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.EditFooter(), ct);

    /// <summary>Leaves the header or footer being edited.</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task CloseHeaderFooterAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.CloseHeaderFooter(), ct);

    /// <summary>Inserts a page number.</summary>
    /// <param name="format">What to show.</param>
    /// <param name="where">At the caret (the default), or in the header or footer.</param>
    /// <param name="alignment">The alignment in a header or footer.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task InsertPageNumberAsync(PageNumberFormat format, PageNumberPlacement? where = null, PageNumberAlignment? alignment = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.InsertPageNumber(format, where, alignment), ct);

    /// <summary>Inserts a table at the selection.</summary>
    /// <param name="rows">The number of rows, 1 to 32767.</param>
    /// <param name="columns">The number of columns, 1 to 63.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task InsertTableAsync(int rows, int columns, CancellationToken ct = default) => _session.RunAsync(EditorOperation.InsertTable(rows, columns), ct);

    /// <summary>Inserts a picture at the selection.</summary>
    /// <param name="data">The image (PNG, JPEG, GIF, BMP or TIFF).</param>
    /// <param name="altText">The alternative text.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task InsertPictureAsync(ReadOnlyMemory<byte> data, string? altText = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.InsertPicture(data, altText), ct);

    /// <summary>Inserts a text box.</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task InsertTextBoxAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.InsertTextBox(), ct);

    /// <summary>Inserts a footnote at the selection.</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task InsertFootnoteAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.InsertFootnote(), ct);

    /// <summary>Inserts an endnote at the selection.</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task InsertEndnoteAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.InsertEndnote(), ct);

    /// <summary>Inserts a hyperlink.</summary>
    /// <param name="address">The address (non-empty).</param>
    /// <param name="text">The text; by default the selection, or the address.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task InsertHyperlinkAsync(string address, string? text = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.InsertHyperlink(address, text), ct);

    /// <summary>Changes the hyperlink at the selection.</summary>
    /// <param name="address">The new address.</param>
    /// <param name="text">The new text.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task EditHyperlinkAsync(string? address = null, string? text = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.EditHyperlink(address, text), ct);

    /// <summary>Removes the hyperlink at the selection, keeping its text.</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task RemoveHyperlinkAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.RemoveHyperlink(), ct);

    /// <summary>Inserts a field. With a selection and an empty code, wraps the selection.</summary>
    /// <param name="options">The field.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>The new element's id, once the change has committed.</returns>
    public Task<ElementId> InsertFieldAsync(FieldOptions options, CancellationToken ct = default) => _session.RunAsync(EditorOperation.InsertField(options), CommitValue.ElementId, ct);

    /// <summary>Inserts rows above or below the selection.</summary>
    /// <param name="where">Above or below.</param>
    /// <param name="count">The number of rows (1 by default).</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task InsertRowsAsync(RowPosition where, int? count = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.InsertRows(where, count), ct);

    /// <summary>Inserts columns left or right of the selection.</summary>
    /// <param name="where">Left or right.</param>
    /// <param name="count">The number of columns (1 by default).</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task InsertColumnsAsync(ColumnPosition where, int? count = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.InsertColumns(where, count), ct);

    /// <summary>Deletes the selected rows.</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task DeleteRowsAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.DeleteRows(), ct);

    /// <summary>Deletes the selected columns.</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task DeleteColumnsAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.DeleteColumns(), ct);

    /// <summary>Deletes the table.</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task DeleteTableAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.DeleteTable(), ct);

    /// <summary>Deletes the selected cells, shifting the rest.</summary>
    /// <param name="shift">Shift cells up or left.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task DeleteCellsAsync(CellShift shift, CancellationToken ct = default) => _session.RunAsync(EditorOperation.DeleteCells(shift), ct);

    /// <summary>Merges a rectangular block of selected cells.</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task MergeCellsAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.MergeCells(), ct);

    /// <summary>Splits the selected cells.</summary>
    /// <param name="columns">The number of columns, 1 to 63.</param>
    /// <param name="rows">The number of rows (1 by default).</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SplitCellsAsync(int columns, int? rows = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SplitCells(columns, rows), ct);

    /// <summary>Splits the table above the selected row.</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SplitTableAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.SplitTable(), ct);

    /// <summary>Gives the selected rows equal heights.</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task DistributeRowsAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.DistributeRows(), ct);

    /// <summary>Gives the selected columns equal widths.</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task DistributeColumnsAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.DistributeColumns(), ct);

    /// <summary>Moves the selected rows up or down.</summary>
    /// <param name="direction">The direction.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task MoveRowsAsync(VerticalDirection direction, CancellationToken ct = default) => _session.RunAsync(EditorOperation.MoveRows(direction), ct);

    /// <summary>Fits the column widths.</summary>
    /// <param name="to">To the contents, to the window, or fixed.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task AutoFitColumnsAsync(AutoFitBehavior to, CancellationToken ct = default) => _session.RunAsync(EditorOperation.AutoFitColumns(to), ct);

    /// <summary>Sets the vertical alignment of the selected cells.</summary>
    /// <param name="vertical">The alignment.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetCellAlignmentAsync(CellAlignment vertical, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetCellAlignment(vertical), ct);

    /// <summary>Sets the shading of the selected cells.</summary>
    /// <param name="color">The colour; <see cref="ColorValue.None"/> removes the shading.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetCellShadingAsync(ColorValue color, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetCellShading(color), ct);

    /// <summary>Sets borders on the selected cells.</summary>
    /// <param name="edges">Which edges.</param>
    /// <param name="border">The line; none for no border.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetCellBordersAsync(BorderEdges edges, CellBorder? border = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetCellBorders(edges, border), ct);

    /// <summary>Sets the margins of the selected cells. Unset values are kept.</summary>
    /// <param name="options">The margins to change.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetCellMarginsAsync(CellMarginOptions options, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetCellMargins(options), ct);

    /// <summary>Repeats the selected rows as a header row on each page.</summary>
    /// <param name="on">Whether they repeat.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetHeaderRowAsync(bool on, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetHeaderRow(on), ct);

    /// <summary>Sets the height of the selected rows.</summary>
    /// <param name="height">The height, or <see cref="RowHeight.Auto"/>.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetRowHeightAsync(RowHeight height, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetRowHeight(height), ct);

    /// <summary>Lets the selected rows break across pages.</summary>
    /// <param name="on">Whether they can break.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetRowCanBreakAsync(bool on, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetRowCanBreak(on), ct);

    /// <summary>Sets the width of the selected columns.</summary>
    /// <param name="width">The width, 1 to 1584 pt.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetColumnWidthAsync(Length width, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetColumnWidth(width), ct);

    /// <summary>Moves a column boundary of the table.</summary>
    /// <param name="index">The boundary index.</param>
    /// <param name="to">The new position.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task MoveTableColumnBoundaryAsync(int index, Length to, CancellationToken ct = default) => _session.RunAsync(EditorOperation.MoveTableColumnBoundary(index, to), ct);

    /// <summary>Moves a row boundary of the table.</summary>
    /// <param name="index">The boundary index.</param>
    /// <param name="to">The new position.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task MoveTableRowBoundaryAsync(int index, Length to, CancellationToken ct = default) => _session.RunAsync(EditorOperation.MoveTableRowBoundary(index, to), ct);

    /// <summary>Sets the table width.</summary>
    /// <param name="width">The width.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetTableWidthAsync(TableWidth width, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetTableWidth(width), ct);

    /// <summary>Sets the table's alignment on the page.</summary>
    /// <param name="alignment">The alignment.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetTableAlignmentAsync(TableAlignment alignment, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetTableAlignment(alignment), ct);

    /// <summary>Sets the table's indent from the margin.</summary>
    /// <param name="indent">The indent, -1584 to 1584 pt.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetTableIndentAsync(Length indent, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetTableIndent(indent), ct);

    /// <summary>Applies a table style.</summary>
    /// <param name="styleId">An existing table style.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetTableStyleAsync(StyleId styleId, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetTableStyle(styleId), ct);

    /// <summary>Sets which parts of the table style apply. Unset values are kept.</summary>
    /// <param name="options">The options to change.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetTableStyleOptionsAsync(TableStyleOptionsUpdate options, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetTableStyleOptions(options), ct);

    /// <summary>Resizes the selected picture or text box. Unset values are kept.</summary>
    /// <param name="width">The width, 1 to 1584 pt.</param>
    /// <param name="height">The height, 1 to 1584 pt.</param>
    /// <param name="lockAspect">Keep the aspect ratio.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetObjectSizeAsync(Length? width = null, Length? height = null, bool? lockAspect = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetObjectSize(width, height, lockAspect), ct);

    /// <summary>Restores the selected picture's original size.</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task ResetObjectSizeAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.ResetObjectSize(), ct);

    /// <summary>Sets how text wraps around the selected object.</summary>
    /// <param name="wrap">The wrapping.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetObjectWrapAsync(ObjectWrap wrap, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetObjectWrap(wrap), ct);

    /// <summary>Locks the selected floating object's anchor.</summary>
    /// <param name="on">Whether the anchor is locked.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetAnchorLockedAsync(bool on, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetAnchorLocked(on), ct);

    /// <summary>Sets the alternative text of the selected object.</summary>
    /// <param name="text">The text.</param>
    /// <param name="decorative">Marks the object as decorative.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetAltTextAsync(string text, bool? decorative = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetAltText(text, decorative), ct);

    /// <summary>Deletes the selected object.</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task DeleteObjectAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.DeleteObject(), ct);

    /// <summary>Changes a field's code or display text.</summary>
    /// <param name="id">The field.</param>
    /// <param name="code">The new code.</param>
    /// <param name="display">The new display text.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetFieldAsync(ElementId id, string? code = null, RichText? display = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetField(id, code, display), ct);

    /// <summary>Sets computed field results. A token joins the update step a FieldsUpdateRequested event started.</summary>
    /// <param name="results">The results.</param>
    /// <param name="token">The token from FieldsUpdateRequested.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetFieldResultsAsync(IReadOnlyList<FieldResult> results, long? token = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetFieldResults(results, token), ct);

    /// <summary>Updates fields (F9); raises FieldsUpdateRequested for fields the application computes.</summary>
    /// <param name="ids">The fields; by default the selection's.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task UpdateFieldsAsync(IReadOnlyList<ElementId>? ids = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.UpdateFields(ids), ct);

    /// <summary>Locks or unlocks fields against updates.</summary>
    /// <param name="locked">Whether the fields are locked.</param>
    /// <param name="ids">The fields; by default the selection's.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetFieldsLockedAsync(bool locked, IReadOnlyList<ElementId>? ids = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetFieldsLocked(locked, ids), ct);

    /// <summary>Replaces fields with their results.</summary>
    /// <param name="ids">The fields; by default the selection's.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task UnlinkFieldsAsync(IReadOnlyList<ElementId>? ids = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.UnlinkFields(ids), ct);

    /// <summary>Shows field codes instead of results. A view setting: no undo step.</summary>
    /// <param name="show">Whether codes show.</param>
    /// <param name="ids">The fields; by default all of them.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task ShowFieldCodesAsync(bool show, IReadOnlyList<ElementId>? ids = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.ShowFieldCodes(show, ids), ct);

    /// <summary>Sets what design mode shows for each field code. A view setting.</summary>
    /// <param name="labels">Labels keyed by field code.</param>
    /// <param name="merge">Merge with the current labels instead of replacing them.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetFieldLabelsAsync(IReadOnlyDictionary<string, string> labels, bool? merge = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetFieldLabels(labels, merge), ct);

    /// <summary>Inserts a content control.</summary>
    /// <param name="options">The control.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>The new element's id, once the change has committed.</returns>
    public Task<ElementId> InsertControlAsync(InsertControlOptions options, CancellationToken ct = default) => _session.RunAsync(EditorOperation.InsertControl(options), CommitValue.ElementId, ct);

    /// <summary>Changes a content control. Unset values are kept.</summary>
    /// <param name="changes">The changes.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetControlAsync(ControlChanges changes, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetControl(changes), ct);

    /// <summary>Removes a content control.</summary>
    /// <param name="id">The control.</param>
    /// <param name="keepContent">Keep its content in the document.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task RemoveControlAsync(ElementId id, bool? keepContent = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.RemoveControl(id, keepContent), ct);

    /// <summary>Fills a content control with text, a picture or a document.</summary>
    /// <param name="id">The control.</param>
    /// <param name="content">The content.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task FillControlAsync(ElementId id, ControlContent content, CancellationToken ct = default) => _session.RunAsync(EditorOperation.FillControl(id, content), ct);

    /// <summary>Checks or clears a checkbox control.</summary>
    /// <param name="id">The control.</param>
    /// <param name="isChecked">Whether it is checked.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetCheckboxAsync(ElementId id, bool isChecked, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetCheckbox(id, isChecked), ct);

    /// <summary>Wraps the selection in a repeating section.</summary>
    /// <param name="tag">The tag.</param>
    /// <param name="title">The title.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>The new element's id, once the change has committed.</returns>
    public Task<ElementId> WrapInRepeatingSectionAsync(string? tag = null, string? title = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.WrapInRepeatingSection(tag, title), CommitValue.ElementId, ct);

    /// <summary>Removes a field, or a control with its content.</summary>
    /// <param name="id">The element.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task RemoveElementAsync(ElementId id, CancellationToken ct = default) => _session.RunAsync(EditorOperation.RemoveElement(id), ct);

    /// <summary>Replaces text in element codes, tags or titles (case-sensitive, every occurrence).</summary>
    /// <param name="find">The text to find.</param>
    /// <param name="replace">The replacement.</param>
    /// <param name="in">Where to replace.</param>
    /// <param name="kind">Only fields or only controls.</param>
    /// <param name="ids">Only these elements.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>The number of elements changed, once the change has committed.</returns>
    public Task<int> ReplaceInElementsAsync(string find, string replace, ElementTextField @in, ElementCategory? kind = null, IReadOnlyList<ElementId>? ids = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.ReplaceInElements(find, replace, @in, kind, ids), CommitValue.Changed, ct);

    /// <summary>Marks elements with wavy lines and a tooltip. A view setting.</summary>
    /// <param name="marks">The marks.</param>
    /// <param name="clear">Clear the existing marks first.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task MarkElementsAsync(IReadOnlyList<ElementMark> marks, bool? clear = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.MarkElements(marks, clear), ct);

    /// <summary>Sets how placeholders look in design mode. A view setting.</summary>
    /// <param name="looks">The looks.</param>
    /// <param name="merge">Merge with the current looks.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetPlaceholderLooksAsync(IReadOnlyList<PlaceholderLook> looks, bool? merge = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetPlaceholderLooks(looks, merge), ct);

    /// <summary>Fills the selected picture placeholder, or every unfilled one with a key.</summary>
    /// <param name="image">The picture.</param>
    /// <param name="key">The placeholder key.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task FillPlaceholderAsync(ReadOnlyMemory<byte> image, string? key = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.FillPlaceholder(image, key), ct);

    /// <summary>Writes a custom XML data part.</summary>
    /// <param name="namespace">The part's namespace.</param>
    /// <param name="xml">The XML (well-formed).</param>
    /// <param name="itemId">The part's item id, when several share the namespace.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>The part's item id, once the change has committed.</returns>
    public Task<string> WriteDataPartAsync(string @namespace, string xml, string? itemId = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.WriteDataPart(@namespace, xml, itemId), CommitValue.ItemId, ct);

    /// <summary>Removes a custom XML data part.</summary>
    /// <param name="namespace">The part's namespace.</param>
    /// <param name="itemId">The part's item id.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task RemoveDataPartAsync(string @namespace, string? itemId = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.RemoveDataPart(@namespace, itemId), ct);

    /// <summary>Turns change tracking on or off.</summary>
    /// <param name="on">Whether changes are tracked.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetTrackChangesAsync(bool on, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetTrackChanges(on), ct);

    /// <summary>Sets how revisions and comments show. A view setting.</summary>
    /// <param name="view">The view.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetMarkupViewAsync(MarkupView view, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetMarkupView(view), ct);

    /// <summary>Accepts a revision. A move resolves as a pair.</summary>
    /// <param name="id">The revision.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task AcceptRevisionAsync(RevisionId id, CancellationToken ct = default) => _session.RunAsync(EditorOperation.AcceptRevision(id), ct);

    /// <summary>Rejects a revision. A move resolves as a pair.</summary>
    /// <param name="id">The revision.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task RejectRevisionAsync(RevisionId id, CancellationToken ct = default) => _session.RunAsync(EditorOperation.RejectRevision(id), ct);

    /// <summary>Accepts the revisions in the selection.</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task AcceptRevisionsInSelectionAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.AcceptRevisionsInSelection(), ct);

    /// <summary>Rejects the revisions in the selection.</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task RejectRevisionsInSelectionAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.RejectRevisionsInSelection(), ct);

    /// <summary>Accepts every revision, or one author's.</summary>
    /// <param name="author">Only this author's revisions.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task AcceptAllRevisionsAsync(string? author = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.AcceptAllRevisions(author), ct);

    /// <summary>Rejects every revision, or one author's.</summary>
    /// <param name="author">Only this author's revisions.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task RejectAllRevisionsAsync(string? author = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.RejectAllRevisions(author), ct);

    /// <summary>Selects the next or previous revision.</summary>
    /// <param name="direction">The direction.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SelectRevisionAsync(NavigationDirection direction, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SelectRevision(direction), ct);

    /// <summary>Replies to a comment thread.</summary>
    /// <param name="to">The thread.</param>
    /// <param name="body">The reply.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>The new comment's id, once the change has committed.</returns>
    public Task<CommentId> ReplyToCommentAsync(CommentId to, RichText body, CancellationToken ct = default) => _session.RunAsync(EditorOperation.ReplyToComment(to, body), CommitValue.CommentId, ct);

    /// <summary>Changes a comment's text.</summary>
    /// <param name="id">The comment.</param>
    /// <param name="body">The new text.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task EditCommentAsync(CommentId id, RichText body, CancellationToken ct = default) => _session.RunAsync(EditorOperation.EditComment(id, body), ct);

    /// <summary>Deletes a reply, or the whole thread when it is the first comment.</summary>
    /// <param name="id">The comment.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task DeleteCommentAsync(CommentId id, CancellationToken ct = default) => _session.RunAsync(EditorOperation.DeleteComment(id), ct);

    /// <summary>Marks a thread done or open.</summary>
    /// <param name="id">The thread.</param>
    /// <param name="done">Whether it is done.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task ResolveCommentAsync(CommentId id, bool done, CancellationToken ct = default) => _session.RunAsync(EditorOperation.ResolveComment(id, done), ct);

    /// <summary>Highlights a thread. A view setting.</summary>
    /// <param name="id">The thread, or null for none.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetActiveCommentAsync(CommentId? id, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetActiveComment(id), ct);

    /// <summary>Selects and reveals a comment's range.</summary>
    /// <param name="id">The comment.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SelectCommentAsync(CommentId id, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SelectComment(id), ct);

    /// <summary>Sets the colour an author's revisions and comments show in. A view setting.</summary>
    /// <param name="author">The author name.</param>
    /// <param name="color">The colour.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetAuthorColorAsync(string author, ColorValue color, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetAuthorColor(author, color), ct);

    /// <summary>Sets the zoom. A view setting.</summary>
    /// <param name="zoom">A percentage (10 to 500), or a fit.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetZoomAsync(ZoomLevel zoom, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetZoom(zoom), ct);

    /// <summary>Shows or hides formatting marks, gridlines and text boundaries. Unset values are kept.</summary>
    /// <param name="formattingMarks">Formatting marks.</param>
    /// <param name="gridlines">Table gridlines.</param>
    /// <param name="textBoundaries">Text boundaries.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetViewOptionsAsync(bool? formattingMarks = null, bool? gridlines = null, bool? textBoundaries = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetViewOptions(formattingMarks, gridlines, textBoundaries), ct);

    /// <summary>Switches between editing and design mode.</summary>
    /// <param name="mode">The mode.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetModeAsync(EditorMode mode, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetMode(mode), ct);
}
