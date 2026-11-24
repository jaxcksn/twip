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

    /// <summary>Inserts a line break (Shift+Enter), or a text wrapping break: the text after it goes below the floating objects on that side.</summary>
    /// <param name="clear">The floating objects a text wrapping break clears; null for a plain line break.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task InsertLineBreakAsync(BreakClear? clear = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.InsertLineBreak(clear), ct);

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

    /// <summary>Undoes the last steps, as one change (the Undo list's choice: <see cref="EditorSession.GetHistoryLabelsAsync"/>).</summary>
    /// <param name="count">How many (at least 1; more than there are undoes them all); null for one.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task UndoAsync(int? count = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.Undo(count), ct);

    /// <summary>Redoes the last undone steps, as one change.</summary>
    /// <param name="count">How many (at least 1; more than there are redoes them all); null for one.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task RedoAsync(int? count = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.Redo(count), ct);

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

    /// <summary>Selects a bookmark's text, or puts the caret at it when it holds none (Go To); <c>_top</c> is the document's start.</summary>
    /// <param name="name">The bookmark, compared without case.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task GoToBookmarkAsync(string name, CancellationToken ct = default) => _session.RunAsync(EditorOperation.GoToBookmark(name), ct);

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

    /// <summary>Sets the font of the selection: a family, or the theme's heading or body font, which follows the theme.</summary>
    /// <param name="font">The family name (non-empty; a string converts), or <see cref="FontChoice.FromTheme"/>.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetFontAsync(FontChoice font, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetFont(font), ct);

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

    /// <summary>Applies a character style to the selected text (a linked paragraph style's character half), or removes it.</summary>
    /// <param name="styleId">The style; <see langword="null"/> removes the character style.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task ApplyCharacterStyleAsync(StyleId? styleId, CancellationToken ct = default) => _session.RunAsync(EditorOperation.ApplyCharacterStyle(styleId), ct);

    /// <summary>Sets the scale, kerning and OpenType features; unset values are kept.</summary>
    /// <param name="options">The settings.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetTypographyAsync(TypographyOptions options, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetTypography(options), ct);

    /// <summary>Fits the selected text to a width (Fit Text), or lets it take its own; a caret in fitted text acts on its whole region.</summary>
    /// <param name="width">The width, more than 0 and at most 1584 pt; null removes the fitting.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetFitTextAsync(Length? width, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetFitText(width), ct);

    /// <summary>Sets the text's language and whether spelling is checked; unset values are kept.</summary>
    /// <param name="options">The settings.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetLanguageAsync(LanguageOptions options, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetLanguage(options), ct);

    /// <summary>Sets emboss, engrave, outline and shadow; unset values are kept.</summary>
    /// <param name="options">The effects.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetCharacterEffectsAsync(CharacterEffectsOptions options, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetCharacterEffects(options), ct);

    /// <summary>Sets the underline's colour, or the text's (<see cref="ColorValue.Auto"/>); underlined text only.</summary>
    /// <param name="color">The colour.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetUnderlineColorAsync(ColorValue color, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetUnderlineColor(color), ct);

    /// <summary>Removes direct character formatting from the selection.</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task ClearFormattingAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.ClearFormatting(), ct);

    /// <summary>Changes the case of the selected text, or of the word at a collapsed caret. Shift+F3 cycles lower, UPPER, Capitalize Each Word (Sentence case for a whole sentence).</summary>
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
    /// <param name="contextual">No space between paragraphs of the same style.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetParagraphSpacingAsync(SpacingValue? before = null, SpacingValue? after = null, bool? contextual = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetParagraphSpacing(before, after, contextual), ct);

    /// <summary>Sets the outline level and whether the paragraphs open collapsed; unset values are kept.</summary>
    /// <param name="options">The settings.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetOutlineAsync(OutlineOptions options, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetOutline(options), ct);

    /// <summary>Sets the selected paragraphs' text direction.</summary>
    /// <param name="direction">The direction.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetParagraphTextDirectionAsync(TextFlow direction, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetParagraphTextDirection(direction), ct);

    /// <summary>Sets the indents of the selected paragraphs. Unset values are kept.</summary>
    /// <param name="options">The indents to change.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetIndentsAsync(IndentOptions options, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetIndents(options), ct);

    /// <summary>Increases the indent (Ctrl+M): list items go a level down, or the whole list moves from its first item; other paragraphs go to the next default tab stop.</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task IncreaseIndentAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.IncreaseIndent(), ct);

    /// <summary>Decreases the indent (Ctrl+Shift+M): list items go a level up, or the whole list moves back from its first item; other paragraphs go to the previous default tab stop.</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task DecreaseIndentAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.DecreaseIndent(), ct);

    /// <summary>Applies a paragraph style to the selected paragraphs.</summary>
    /// <param name="styleId">An existing paragraph style.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetParagraphStyleAsync(StyleId styleId, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetParagraphStyle(styleId), ct);

    /// <summary>Changes a style to the selection's formatting: its paragraph's and its first run's direct formatting become the style's.</summary>
    /// <param name="styleId">The style.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task UpdateStyleToSelectionAsync(StyleId styleId, CancellationToken ct = default) => _session.RunAsync(EditorOperation.UpdateStyleToSelection(styleId), ct);

    /// <summary>Deletes a style the document made: its paragraphs take the default paragraph style, its runs lose it. Built-in styles can't be deleted.</summary>
    /// <param name="styleId">The style.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task DeleteStyleAsync(StyleId styleId, CancellationToken ct = default) => _session.RunAsync(EditorOperation.DeleteStyle(styleId), ct);

    /// <summary>Sets a style's list flags; a built-in style the document doesn't define keeps them as its latent settings.</summary>
    /// <param name="options">The style and what to set.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetStyleMetaAsync(StyleMetaOptions options, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetStyleMeta(options), ct);

    /// <summary>Sets the document's default formatting (Manage Styles' Set Defaults).</summary>
    /// <param name="defaults">The defaults to set.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetDocumentDefaultsAsync(DocumentDefaults defaults, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetDocumentDefaults(defaults), ct);

    /// <summary>
    /// Sets the document's default formatting with any character and paragraph formatting operations but frames, run
    /// on the defaults as on a style; every style's text lays out again. One undo step.
    /// </summary>
    /// <param name="commands">The formatting.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetDefaultFormattingAsync(IReadOnlyList<EditorOperation> commands, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetDefaultFormatting(commands), ct);

    /// <summary>
    /// Makes a style (New Style): its type, name, base, following style, link, list and formatting, from the
    /// selection's formatting or not, applied to the selection or not, as one undo step. Invalid for a name that's
    /// empty, taken or a built-in style's, a base of another type or that loops, and formatting the type doesn't take.
    /// </summary>
    /// <param name="style">The style to make.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>The style's id, once the change has committed.</returns>
    public Task<StyleId> CreateStyleAsync(NewStyle style, CancellationToken ct = default) => _session.RunAsync(EditorOperation.CreateStyle(style), CommitValue.StyleId, ct);

    /// <summary>
    /// Changes a style (Modify Style); a built-in style the document lacks joins it. Renaming a built-in style adds an
    /// alias; linking sets both sides and unlinks their old partners; a linked style's character formatting goes to
    /// its other half too. Unavailable for no such style.
    /// </summary>
    /// <param name="id">The style.</param>
    /// <param name="change">The change.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task ModifyStyleAsync(StyleId id, StyleChange change, CancellationToken ct = default) => _session.RunAsync(EditorOperation.ModifyStyle(id, change), ct);

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

    /// <summary>Sets the selected paragraphs' border lines: every edge, null removing it.</summary>
    /// <param name="lines">The edges.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetParagraphBorderLinesAsync(ParagraphBorderLines lines, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetParagraphBorderLines(lines), ct);

    /// <summary>Shades the selected paragraphs or text.</summary>
    /// <param name="shading">The target, fill and pattern.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetShadingAsync(ShadingOptions shading, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetShading(shading), ct);

    /// <summary>Sets or removes the selected text's border.</summary>
    /// <param name="border">The box, or a null line to remove it.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetTextBorderAsync(TextBorderOptions border, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetTextBorder(border), ct);

    /// <summary>Sets the page borders of the selection's section, or of every section.</summary>
    /// <param name="borders">The edges and options.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetPageBordersAsync(PageBordersOptions borders, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetPageBorders(borders), ct);

    /// <summary>Sets the properties of the frame the selection is in.</summary>
    /// <param name="properties">The frame's properties.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetFrameAsync(FrameProperties properties, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetFrame(properties), ct);

    /// <summary>Removes the frame the selection is in: its paragraphs go back in the text.</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task RemoveFrameAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.RemoveFrame(), ct);

    /// <summary>Sets the selection's paragraph's drop cap: its first letter dropped into the text or in the margin, or put back.</summary>
    /// <param name="dropCap">The drop cap; <see cref="DropCapPosition.None"/> puts the letter back.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetDropCapAsync(DropCap dropCap, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetDropCap(dropCap), ct);

    /// <summary>Sets the text's fill, outline, shadow, glow and reflection: each is kept (the default), removed (over a style's too; a fill is only removed) or set.</summary>
    /// <param name="change">The change.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetTextEffectsAsync(TextEffectsChange change, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetTextEffects(change), ct);

    /// <summary>Sets the document's page colour; <see cref="FillValue.None"/> removes it. A picture fill with an empty image keeps the page's own.</summary>
    /// <param name="fill">None, a solid colour (a theme colour takes one tint or shade), a gradient, a pattern or a picture (tiled, a texture).</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetPageColorAsync(FillValue fill, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetPageColor(fill), ct);

    /// <summary>Sets the document's watermark, in every header; the text or picture replaces the one there.</summary>
    /// <param name="watermark">The text or picture watermark.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetWatermarkAsync(Watermark watermark, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetWatermark(watermark), ct);

    /// <summary>Removes the document's watermark.</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task RemoveWatermarkAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.RemoveWatermark(), ct);

    /// <summary>Defines the selection's list levels for its whole list, from this paragraph on, or for the selected paragraphs; outside a list, makes a new one.</summary>
    /// <param name="definition">The levels and what they apply to.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetListDefinitionAsync(ListDefinition definition, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetListDefinition(definition), ct);

    /// <summary>Starts a new list at a value, or continues the previous one.</summary>
    /// <param name="options">The mode and value.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetNumberingValueAsync(NumberingValueOptions options, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetNumberingValue(options), ct);

    /// <summary>Restarts the list at this paragraph, at its level's start.</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task RestartNumberingAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.RestartNumbering(), ct);

    /// <summary>Continues the previous list from this paragraph.</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task ContinueNumberingAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.ContinueNumbering(), ct);

    /// <summary>Adjusts the selection's list level's indents and what follows its number.</summary>
    /// <param name="options">The indents.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task AdjustListIndentsAsync(ListIndentOptions options, CancellationToken ct = default) => _session.RunAsync(EditorOperation.AdjustListIndents(options), ct);

    /// <summary>Applies a list gallery entry (<see cref="EditorLists.GalleriesAsync"/>).</summary>
    /// <param name="id">The entry.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task ApplyListEntryAsync(string id, CancellationToken ct = default) => _session.RunAsync(EditorOperation.ApplyListEntry(id), ct);

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

    /// <summary>Turns automatic hyphenation on or off for the document.</summary>
    /// <param name="on">On, with the hyphenation patterns of the text's language.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetAutoHyphenationAsync(bool on, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetAutoHyphenation(on), ct);

    /// <summary>Sets the hyphenation zone, which documents in compatibility mode 14 and earlier use.</summary>
    /// <param name="zone">The zone, 0 to 1584 pt.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetHyphenationZoneAsync(Length zone, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetHyphenationZone(zone), ct);

    /// <summary>Sets the most consecutive lines that may end in a hyphen.</summary>
    /// <param name="limit">The limit, 0 to 32767; 0 for no limit.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetConsecutiveHyphenLimitAsync(int limit, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetConsecutiveHyphenLimit(limit), ct);

    /// <summary>Hyphenates words in capitals, or not.</summary>
    /// <param name="on">Whether words in capitals are hyphenated.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetHyphenateCapsAsync(bool on, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetHyphenateCaps(on), ct);

    /// <summary>Sets the page margins of the selection's sections. Unset values are kept.</summary>
    /// <param name="options">The margins to change.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetMarginsAsync(MarginOptions options, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetMargins(options), ct);

    /// <summary>Sets the page orientation of the selection's sections.</summary>
    /// <param name="orientation">The orientation.</param>
    /// <param name="applyTo">The sections: the selection's (null), every section, or from the selection on (a new-page section break goes at its start).</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetOrientationAsync(PageOrientation orientation, PageSetupScope? applyTo = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetOrientation(orientation, applyTo), ct);

    /// <summary>Sets the paper size of the selection's sections, given in portrait.</summary>
    /// <param name="width">The width, 36 to 1584 pt.</param>
    /// <param name="height">The height, 36 to 1584 pt.</param>
    /// <param name="applyTo">The sections: the selection's (null), every section, or from the selection on (a new-page section break goes at its start).</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetPaperSizeAsync(Length width, Length height, PageSetupScope? applyTo = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetPaperSize(width, height, applyTo), ct);

    /// <summary>Sets the text columns of the selection's sections.</summary>
    /// <param name="count">The number of columns, 1 to 45.</param>
    /// <param name="spacing">The space between columns.</param>
    /// <param name="separator">Whether a line separates the columns.</param>
    /// <param name="widths">Columns of their own widths, each with the space after it; equal columns when null.</param>
    /// <param name="applyTo">Where: the selection's sections (null), every section, from the selection on (a continuous section break at its start), or the selected text alone.</param>
    /// <param name="startNewColumn">With <see cref="ColumnsApplyTo.ThisPointForward"/>: the text from the selection on starts a new column.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetColumnsAsync(int count, Length? spacing = null, bool? separator = null, IReadOnlyList<ColumnWidth>? widths = null, ColumnsApplyTo? applyTo = null, bool? startNewColumn = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetColumns(count, spacing, separator, widths, applyTo, startNewColumn), ct);

    /// <summary>Sets the header and footer distances from the page edge. Unset values are kept.</summary>
    /// <param name="header">The header distance.</param>
    /// <param name="footer">The footer distance.</param>
    /// <param name="applyTo">The sections: the selection's (null), every section, or from the selection on (a new-page section break goes at its start).</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetHeaderFooterDistanceAsync(Length? header = null, Length? footer = null, PageSetupScope? applyTo = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetHeaderFooterDistance(header, footer, applyTo), ct);

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

    /// <summary>Sets the section's page number format, first number and chapter numbers; unset values are kept.</summary>
    /// <param name="options">The numbering.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetPageNumberingAsync(PageNumberingOptions options, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetPageNumbering(options), ct);

    /// <summary>Sets where text sits between the section's top and bottom margins.</summary>
    /// <param name="alignment">The alignment.</param>
    /// <param name="applyTo">The sections: the selection's (null), every section, or from the selection on (a new-page section break goes at its start).</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetPageVerticalAlignmentAsync(PageVerticalAlignment alignment, PageSetupScope? applyTo = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetPageVerticalAlignment(alignment, applyTo), ct);

    /// <summary>Numbers the section's lines, or stops.</summary>
    /// <param name="numbers">The numbering; <see langword="null"/> stops it.</param>
    /// <param name="applyTo">The sections: the selection's (null), every section, or from the selection on (a new-page section break goes at its start).</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetLineNumberingAsync(LineNumbers? numbers, PageSetupScope? applyTo = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetLineNumbering(numbers, applyTo), ct);

    /// <summary>Sets where the section starts: the break before it.</summary>
    /// <param name="kind">The break.</param>
    /// <param name="applyTo">The sections: the selection's (null), every section, or from the selection on (a new-page section break goes at its start).</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetSectionBreakTypeAsync(SectionBreakKind kind, PageSetupScope? applyTo = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetSectionBreakType(kind, applyTo), ct);

    /// <summary>Sets the section's text direction.</summary>
    /// <param name="direction">The direction.</param>
    /// <param name="applyTo">The sections: the selection's (null), every section, or from the selection on (a new-page section break goes at its start).</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetSectionTextDirectionAsync(TextFlow direction, PageSetupScope? applyTo = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetSectionTextDirection(direction, applyTo), ct);

    /// <summary>Sets how the document's pages pair and where the gutter is; unset values are kept.</summary>
    /// <param name="options">The settings.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetMultiplePagesAsync(MultiplePagesOptions options, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetMultiplePages(options), ct);

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
    /// <param name="showOnFirstPage">The number shows on the section's first page (null: yes); false gives the section a different first page.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task InsertPageNumberAsync(PageNumberFormat format, PageNumberPlacement? where = null, PageNumberAlignment? alignment = null, bool? showOnFirstPage = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.InsertPageNumber(format, where, alignment, showOnFirstPage), ct);

    /// <summary>The caret section's header or footer takes the section before's (Link to Previous), or gets its own copy of it.</summary>
    /// <param name="kind">A header or a footer.</param>
    /// <param name="pages">Which pages it is for.</param>
    /// <param name="linked">Takes the section before's.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetHeaderFooterLinkedAsync(HeaderFooterKind kind, HeaderFooterType pages, bool linked, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetHeaderFooterLinked(kind, pages, linked), ct);

    /// <summary>Empties the caret section's header or footer (its own, so a linked one stops linking).</summary>
    /// <param name="kind">A header or a footer.</param>
    /// <param name="pages">Which pages it is for.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task RemoveHeaderFooterAsync(HeaderFooterKind kind, HeaderFooterType pages, CancellationToken ct = default) => _session.RunAsync(EditorOperation.RemoveHeaderFooter(kind, pages), ct);

    /// <summary>Inserts a tab to a place set from the margin or the indent, not from the tab stops (Insert Alignment Tab).</summary>
    /// <param name="alignment">How the text after it aligns at its place.</param>
    /// <param name="relativeTo">What its place is measured in.</param>
    /// <param name="leader">What fills the space before it; none when null.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task InsertAlignmentTabAsync(AlignmentTabKind alignment, AlignmentTabBase relativeTo, TabLeader? leader = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.InsertAlignmentTab(alignment, relativeTo, leader), ct);

    /// <summary>Inserts a table at the selection.</summary>
    /// <param name="rows">The number of rows, 1 to 32767.</param>
    /// <param name="columns">The number of columns, 1 to 63.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task InsertTableAsync(int rows, int columns, CancellationToken ct = default) => _session.RunAsync(EditorOperation.InsertTable(rows, columns), ct);

    /// <summary>Inserts a picture at the selection.</summary>
    /// <param name="data">The image (PNG, JPEG, GIF, BMP, TIFF, JPEG XR, SVG, EMF or WMF).</param>
    /// <param name="altText">The alternative text.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task InsertPictureAsync(ReadOnlyMemory<byte> data, string? altText = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.InsertPicture(data, altText), ct);

    /// <summary>Inserts a text box.</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task InsertTextBoxAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.InsertTextBox(), ct);

    /// <summary>Inserts a floating shape at the caret's paragraph, in front of the text, with the theme's default style, and selects it.</summary>
    /// <param name="preset">The preset geometry.</param>
    /// <param name="width">The width; 72 pt when not given.</param>
    /// <param name="height">The height; 72 pt when not given, 0 for lines.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task InsertShapeAsync(ShapePreset preset, Length? width = null, Length? height = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.InsertShape(preset, width, height), ct);

    /// <summary>Inserts a footnote at the selection.</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task InsertFootnoteAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.InsertFootnote(), ct);

    /// <summary>Inserts an endnote at the selection.</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task InsertEndnoteAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.InsertEndnote(), ct);

    /// <summary>Inserts a footnote or an endnote at the caret, numbered or with its own mark.</summary>
    /// <param name="kind">A footnote or an endnote.</param>
    /// <param name="customMark">Its own mark (1 to 10 characters) instead of a number.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task InsertNoteAsync(NoteType kind, string? customMark = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.InsertNote(kind, customMark), ct);

    /// <summary>Sets footnotes' or endnotes' position, number format, first number and restarting; unset values are kept.</summary>
    /// <param name="options">The settings.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetNotePropertiesAsync(NotePropertiesOptions options, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetNoteProperties(options), ct);

    /// <summary>Turns every footnote into an endnote, every endnote into a footnote, or both.</summary>
    /// <param name="conversion">What turns into what.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task ConvertNotesAsync(NoteConversion conversion, CancellationToken ct = default) => _session.RunAsync(EditorOperation.ConvertNotes(conversion), ct);

    /// <summary>Inserts a hyperlink.</summary>
    /// <param name="address">The address (non-empty).</param>
    /// <param name="text">The text; by default the selection, or the address.</param>
    /// <param name="bookmark">The place in the target (a bookmark); it wins over a <c>#bookmark</c> in the address.</param>
    /// <param name="screenTip">The text shown when the link is hovered.</param>
    /// <param name="frame">The window or frame it opens in (<c>_blank</c>).</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task InsertHyperlinkAsync(string address, string? text = null, string? bookmark = null, string? screenTip = null, string? frame = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.InsertHyperlink(address, text, bookmark, screenTip, frame), ct);

    /// <summary>Changes the hyperlink at the selection.</summary>
    /// <param name="address">The new address.</param>
    /// <param name="text">The new text.</param>
    /// <param name="screenTip">The new ScreenTip; empty removes it.</param>
    /// <param name="frame">The new window or frame; empty removes it.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task EditHyperlinkAsync(string? address = null, string? text = null, string? screenTip = null, string? frame = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.EditHyperlink(address, text, screenTip, frame), ct);

    /// <summary>Removes the hyperlink at the selection, keeping its text.</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task RemoveHyperlinkAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.RemoveHyperlink(), ct);

    /// <summary>Bookmarks the selection. A bookmark of the same name (compared without case) moves here.</summary>
    /// <param name="name">A letter (or <c>_</c>, for a hidden bookmark) first, then letters, digits and <c>_</c>, 40 at most.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task InsertBookmarkAsync(string name, CancellationToken ct = default) => _session.RunAsync(EditorOperation.InsertBookmark(name), ct);

    /// <summary>Deletes a bookmark, keeping its text.</summary>
    /// <param name="name">The bookmark, compared without case.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task DeleteBookmarkAsync(string name, CancellationToken ct = default) => _session.RunAsync(EditorOperation.DeleteBookmark(name), ct);

    /// <summary>Restrict Editing: what the document allows, and starting or stopping its enforcement; stopping a password-protected document needs the password.</summary>
    /// <param name="protection">The settings to change.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetDocumentProtectionAsync(DocumentProtection protection, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetDocumentProtection(protection), ct);

    /// <summary>Makes the selection a range everyone (or <paramref name="editor"/>) may edit while the document is protected.</summary>
    /// <param name="editor">A person who may edit it; everyone when this and <paramref name="group"/> are null.</param>
    /// <param name="group">A group who may edit it (<c>editors</c>, <c>owners</c>, ...), in place of <paramref name="editor"/>.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task AddEditableRangeAsync(string? editor = null, string? group = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.AddEditableRange(editor, group), ct);

    /// <summary>Removes the editable ranges at the selection.</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task RemoveEditableRangesAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.RemoveEditableRanges(), ct);

    /// <summary>Inserts a legacy form field (FORMTEXT, FORMCHECKBOX, FORMDROPDOWN) at the selection.</summary>
    /// <param name="kind">Its kind.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>The new field's id, once the change has committed.</returns>
    public Task<ElementId> InsertFormFieldAsync(FormFieldType kind, CancellationToken ct = default) => _session.RunAsync(EditorOperation.InsertFormField(kind), CommitValue.ElementId, ct);

    /// <summary>Sets a legacy form field's options.</summary>
    /// <param name="change">The options to change.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetFormFieldAsync(FormFieldChange change, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetFormField(change), ct);

    /// <summary>Fills in a legacy form field: a text field's text, a check box's state, a drop-down's entry.</summary>
    /// <param name="fill">The value.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task FillFormFieldAsync(FormFieldFill fill, CancellationToken ct = default) => _session.RunAsync(EditorOperation.FillFormField(fill), ct);

    /// <summary>Shades legacy form fields, or not.</summary>
    /// <param name="on">Shaded.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetFormFieldShadingAsync(bool on, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetFormFieldShading(on), ct);

    /// <summary>Sets the document's properties; unset values are kept.</summary>
    /// <param name="change">The properties to change.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetDocumentPropertiesAsync(DocumentPropertiesChange change, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetDocumentProperties(change), ct);

    /// <summary>Sets a document variable (read by DOCVARIABLE), or removes it.</summary>
    /// <param name="variable">The variable; a null value removes it.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetDocumentVariableAsync(DocumentVariable variable, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetDocumentVariable(variable), ct);

    /// <summary>Sets the file name FILENAME shows. Not saved, and not an edit.</summary>
    /// <param name="name">The file's name, with its folder when FILENAME <c>\p</c> should show one.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when <see cref="EditorSession.State"/> shows it.</returns>
    public Task SetFileNameAsync(string name, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetFileName(name), ct);

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

    /// <summary>Converts the selected paragraphs to a table: a row for each paragraph, split into cells at the separator.</summary>
    /// <param name="separator">Where the text splits into cells.</param>
    /// <param name="character">The one character to split at, with <see cref="TextSeparator.Other"/>.</param>
    /// <param name="columns">The number of columns, 1 to 63 (by default the most pieces a paragraph has).</param>
    /// <param name="autofit">How the new table's columns are sized; fixed by default.</param>
    /// <param name="columnWidth">With fixed columns, every column this wide (1 to 1584); by default the text's width shared equally.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task ConvertTextToTableAsync(TextSeparator separator, string? character = null, int? columns = null, TableFit? autofit = null, Length? columnWidth = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.ConvertTextToTable(separator, character, columns, autofit, columnWidth), ct);

    /// <summary>Converts the selected rows, or the whole table, to text: a paragraph for each row, its cells joined by the separator.</summary>
    /// <param name="separator">What separates the cells' text.</param>
    /// <param name="character">The one character between cells, with <see cref="TextSeparator.Other"/>.</param>
    /// <param name="convertNested">Tables nested in the cells become text too; by default they stay tables.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task ConvertTableToTextAsync(TextSeparator separator, string? character = null, bool? convertNested = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.ConvertTableToText(separator, character, convertNested), ct);

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

    /// <summary>Sets the selected cells' preferred width, text direction, wrapping, fitting and end mark.</summary>
    /// <param name="options">The properties to change.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetCellPropertiesAsync(CellPropertiesOptions options, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetCellProperties(options), ct);

    /// <summary>Hides the selected rows' end marks (a row hides when its text is hidden too), and sets the table properties they override.</summary>
    /// <param name="hidden">The rows' end marks are hidden; kept when null.</param>
    /// <param name="exceptions">The table properties the rows override; kept when null.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetRowPropertiesAsync(bool? hidden = null, RowExceptions? exceptions = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetRowProperties(hidden, exceptions), ct);

    /// <summary>Sets the rows and columns in each of the table's bands, 1 to 32767; unavailable outside a table.</summary>
    /// <param name="sizes">What to set, leave to the table's style, or keep.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetTableBandSizesAsync(TableBandSizes sizes, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetTableBandSizes(sizes), ct);

    /// <summary>Sets the space between the table's cells, or none.</summary>
    /// <param name="spacing">0 to 1584 points; null for none.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetTableCellSpacingAsync(Length? spacing, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetTableCellSpacing(spacing), ct);

    /// <summary>Sets the table's alternative text.</summary>
    /// <param name="title">The title; kept when null, removed when empty.</param>
    /// <param name="description">The description; kept when null, removed when empty.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetTableAltTextAsync(string? title = null, string? description = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetTableAltText(title, description), ct);

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

    /// <summary>Sets the table style new tables take (Set as Default).</summary>
    /// <param name="styleId">A table style; null for Table Grid.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetDefaultTableStyleAsync(StyleId? styleId, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetDefaultTableStyle(styleId), ct);

    /// <summary>Sets which parts of the table style apply. Unset values are kept.</summary>
    /// <param name="options">The options to change.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetTableStyleOptionsAsync(TableStyleOptionsUpdate options, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetTableStyleOptions(options), ct);

    /// <summary>
    /// Positions the caret's table so text wraps around it, as dragging its move handle does: a table without a
    /// position gets one across from the margin and down from the paragraph, and each axis given is aligned, offset, or
    /// (with neither) kept where it is and measured from its new reference. Unset values are kept; null puts the table
    /// back in the text.
    /// </summary>
    /// <param name="position">The position, or null for a table in the text.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetTablePositionAsync(TablePosition? position, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetTablePosition(position), ct);

    /// <summary>Resizes the selected picture or text box. Unset values are kept.</summary>
    /// <param name="width">The width, 1 to 1584 pt.</param>
    /// <param name="height">The height, 1 to 1584 pt.</param>
    /// <param name="lockAspect">Keep the aspect ratio.</param>
    /// <param name="relativeWidth">A floating object's width as a share of a box on the page; <see cref="RelativeSize.Absolute"/> makes it absolute.</param>
    /// <param name="relativeHeight">A floating object's height as a share of a box on the page; <see cref="RelativeSize.Absolute"/> makes it absolute.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetObjectSizeAsync(Length? width = null, Length? height = null, bool? lockAspect = null, RelativeSize? relativeWidth = null, RelativeSize? relativeHeight = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetObjectSize(width, height, lockAspect, relativeWidth, relativeHeight), ct);

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

    /// <summary>
    /// Places the selected floating object. Each axis given is aligned, offset, or (with neither) kept where it is and
    /// measured from its new reference: <see cref="VerticalReference.Page"/> fixes it on the page,
    /// <see cref="VerticalReference.Paragraph"/> moves it with the text. Unset values are kept.
    /// </summary>
    /// <param name="horizontal">The horizontal place.</param>
    /// <param name="vertical">The vertical place.</param>
    /// <param name="allowOverlap">Whether other floating objects may overlap it.</param>
    /// <param name="layoutInCell">Whether, in a table cell, it is kept within the cell.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetObjectPositionAsync(HorizontalPosition? horizontal = null, VerticalPosition? vertical = null, bool? allowOverlap = null, bool? layoutInCell = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetObjectPosition(horizontal, vertical, allowOverlap, layoutInCell), ct);

    /// <summary>Sets the sides text wraps on around the selected floating object and its distance from the text, 0 to 1584 pt. Unset values are kept.</summary>
    /// <param name="side">The sides text wraps on.</param>
    /// <param name="top">The distance from the text above.</param>
    /// <param name="bottom">The distance from the text below.</param>
    /// <param name="left">The distance from the text on the left.</param>
    /// <param name="right">The distance from the text on the right.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetObjectWrapOptionsAsync(WrapSide? side = null, Length? top = null, Length? bottom = null, Length? left = null, Length? right = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetObjectWrapOptions(side, top, bottom, left, right), ct);

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

    /// <summary>Changes the selected shape's preset geometry. Its text, fill, outline and effects stay; its adjust values reset.</summary>
    /// <param name="preset">The preset geometry.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetShapePresetAsync(ShapePreset preset, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetShapePreset(preset), ct);

    /// <summary>Sets one of the selected shape's adjust values.</summary>
    /// <param name="index">The adjust value's index, in the preset's order.</param>
    /// <param name="value">Its new value, in the preset's units (usually 100000ths).</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetShapeAdjustAsync(int index, long value, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetShapeAdjust(index, value), ct);

    /// <summary>Sets the selected objects' fill; <see cref="FillValue.FromStyle"/> gives them the style reference's fill again.</summary>
    /// <param name="fill">The fill.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetShapeFillAsync(FillValue fill, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetShapeFill(fill), ct);

    /// <summary>Sets the selected objects' outline. Unset values are kept.</summary>
    /// <param name="options">The outline values to change.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetShapeLineAsync(LineChange options, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetShapeLine(options), ct);

    /// <summary>Sets the selected objects' shadows, glow, soft edge and blur: each effect is kept, removed or set.</summary>
    /// <param name="options">The effects to change.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetShapeEffectsAsync(EffectsChange options, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetShapeEffects(options), ct);

    /// <summary>Sets the selected shape's text insets, anchor, autofit, wrapping and direction. Unset values are kept.</summary>
    /// <param name="options">The text body values to change.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetShapeTextAsync(ShapeTextChange options, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetShapeText(options), ct);

    /// <summary>Sets the selected picture's transparency, greyscale, brightness and contrast. Unset values are kept.</summary>
    /// <param name="options">The adjustments to change.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetPictureAdjustAsync(PictureAdjustChange options, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetPictureAdjust(options), ct);

    /// <summary>Crops the selected picture, inline or floating: its box follows the part shown, at the scale it is drawn.</summary>
    /// <param name="crop">The fractions of its image cut from each edge.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetPictureCropAsync(PictureCrop crop, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetPictureCrop(crop), ct);

    /// <summary>Replaces the selected picture's image, keeping its width, effects and place; its height follows the new image and the crop is cleared.</summary>
    /// <param name="data">The image: PNG, JPEG, GIF, BMP, TIFF, JPEG XR, SVG, EMF or WMF.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task ChangePictureAsync(ReadOnlyMemory<byte> data, CancellationToken ct = default) => _session.RunAsync(EditorOperation.ChangePicture(data), ct);

    /// <summary>Aligns or distributes the selected floating objects across their page, its margins, or the box around them.</summary>
    /// <param name="align">How they line up.</param>
    /// <param name="relativeTo">What they line up in; the box around them for two or more, else the margins, when null.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task AlignObjectsAsync(ObjectAlign align, AlignReference? relativeTo = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.AlignObjects(align, relativeTo), ct);

    /// <summary>Rotates the selected objects.</summary>
    /// <param name="degrees">The rotation, 0 to 360 degrees clockwise.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetObjectRotationAsync(double degrees, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetObjectRotation(degrees), ct);

    /// <summary>Flips the selected objects.</summary>
    /// <param name="axis">Which way.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task FlipObjectAsync(FlipAxis axis, CancellationToken ct = default) => _session.RunAsync(EditorOperation.FlipObject(axis), ct);

    /// <summary>Moves the selected floating object in the paint order, or behind or in front of the text.</summary>
    /// <param name="arrange">Where it goes.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task ArrangeObjectAsync(Arrange arrange, CancellationToken ct = default) => _session.RunAsync(EditorOperation.ArrangeObject(arrange), ct);

    /// <summary>Groups the selected floating objects (two or more, in one story) and selects the group.</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task GroupObjectsAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.GroupObjects(), ct);

    /// <summary>Ungroups the selected group and selects its children.</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task UngroupObjectsAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.UngroupObjects(), ct);

    /// <summary>Selects the next or previous child of the selected group. Not an edit: no undo step.</summary>
    /// <param name="direction">Next or previous.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SelectGroupChildAsync(NavigationDirection direction, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SelectGroupChild(direction), ct);

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
    /// <param name="ids">The fields; by default the selection's, with the table of contents (or other field across paragraphs) it touches, or the one at the caret when no other field is there.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetFieldsLockedAsync(bool locked, IReadOnlyList<ElementId>? ids = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetFieldsLocked(locked, ids), ct);

    /// <summary>Replaces fields with their results; a locked field stays, with every field in it.</summary>
    /// <param name="ids">The fields; by default the selection's, with the table of contents (or other field across paragraphs) it touches, or the one at the caret when no other field is there; such a field becomes text with the fields in it.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task UnlinkFieldsAsync(IReadOnlyList<ElementId>? ids = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.UnlinkFields(ids), ct);

    /// <summary>Shows field codes instead of results. A view setting: no undo step.</summary>
    /// <param name="show">Whether codes show.</param>
    /// <param name="ids">The fields; null for all of them, tables of contents included; empty for the table of contents (or other field across paragraphs) at the selection, which has no id.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task ShowFieldCodesAsync(bool show, IReadOnlyList<ElementId>? ids = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.ShowFieldCodes(show, ids), ct);

    /// <summary>Sets what design mode shows for each field code. A view setting.</summary>
    /// <param name="labels">Labels keyed by field code.</param>
    /// <param name="merge">Merge with the current labels instead of replacing them.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetFieldLabelsAsync(IReadOnlyDictionary<string, string> labels, bool? merge = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetFieldLabels(labels, merge), ct);

    /// <summary>Sets which fields are sub-template references, replacing the rules; none turns sub-templates off. A view setting.</summary>
    /// <param name="rules">The rules.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetSubTemplateRulesAsync(IReadOnlyList<ReferenceRule> rules, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetSubTemplateRules(rules), ct);

    /// <summary>Shows references' sub-template bodies in their place, or their fields. A view setting.</summary>
    /// <param name="expanded">Show the bodies (else the fields).</param>
    /// <param name="ids">The reference fields; null for every reference.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetSubTemplateViewAsync(bool expanded, IReadOnlyList<ElementId>? ids = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetSubTemplateView(expanded, ids), ct);

    /// <summary>Edits a sub-template's body in this editor, on its own pages. A view change.</summary>
    /// <param name="name">The sub-template.</param>
    /// <param name="category">Its building block category, when several rules name categories.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the editor is on it and <see cref="EditorSession.State"/> says so.</returns>
    public Task EditSubTemplateAsync(string name, string? category = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.EditSubTemplate(name, category), ct);

    /// <summary>Back to the template from a sub-template, at the template's selection from before. A view change.</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the template shows and <see cref="EditorSession.State"/> says so.</returns>
    public Task CloseSubTemplateAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.CloseSubTemplate(), ct);

    /// <summary>A new sub-template: empty, with the body of the DOCX <paramref name="from"/>, or a copy of the sub-template <paramref name="copyOf"/>. One undo step.</summary>
    /// <param name="name">Its name, unique in its category.</param>
    /// <param name="category">Its building block category; null for the first rule's.</param>
    /// <param name="from">A DOCX whose body becomes its body.</param>
    /// <param name="copyOf">A sub-template in the same category whose body (formatting, fields, controls, pictures, references) is copied; not with <paramref name="from"/>.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task CreateSubTemplateAsync(string name, string? category = null, ReadOnlyMemory<byte>? from = null, string? copyOf = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.CreateSubTemplate(name, category, from, copyOf), ct);

    /// <summary>The template's body becomes the DOCX <paramref name="from"/>'s body, on its last section's page; its sub-templates, custom XML data parts, settings, styles, headers and footers stay. Closes a sub-template being edited first. One undo step.</summary>
    /// <param name="from">The DOCX.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task ReplaceBodyAsync(ReadOnlyMemory<byte> from, CancellationToken ct = default) => _session.RunAsync(EditorOperation.ReplaceBody(from), ct);

    /// <summary>Renames a sub-template, and optionally every reference to it. One undo step.</summary>
    /// <param name="name">Its name now.</param>
    /// <param name="to">Its new name, unique in its category.</param>
    /// <param name="updateReferences">Rename the references too.</param>
    /// <param name="category">Its building block category, when several rules name categories.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task RenameSubTemplateAsync(string name, string to, bool? updateReferences = null, string? category = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.RenameSubTemplate(name, to, updateReferences, category), ct);

    /// <summary>Removes a sub-template; its references no longer resolve. One undo step.</summary>
    /// <param name="name">The sub-template.</param>
    /// <param name="category">Its building block category, when several rules name categories.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task RemoveSubTemplateAsync(string name, string? category = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.RemoveSubTemplate(name, category), ct);

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

    /// <summary>
    /// Rewrites a font family's name in the document itself, so the saved file names <paramref name="to"/>: one undo
    /// step. In document scope with no style filter it rewrites the text (runs, paragraph marks, symbols) and the
    /// tables below it (styles, document defaults, numbering levels, the theme's font slots and the font table). In
    /// selection scope, or with a filter, it rewrites only the text, and a matching run whose family comes from a style
    /// or the theme gets <paramref name="to"/> as its own. With tracked changes on, the text's changes are formatting
    /// revisions. Text inside locked content controls is skipped and counted.
    /// </summary>
    /// <param name="from">The family to rewrite, case-insensitive.</param>
    /// <param name="to">The family it becomes.</param>
    /// <param name="scope">The whole document, or the selection.</param>
    /// <param name="bold">Only bold (true) or regular-weight (false) text.</param>
    /// <param name="italic">Only italic (true) or upright (false) text.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>What changed and what was skipped, once the change has committed.</returns>
    public Task<FontReplaceResult> ReplaceFontAsync(string from, string to, ReplaceScope scope = ReplaceScope.Document, bool? bold = null, bool? italic = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.ReplaceFont(from, to, scope, bold, italic), CommitValue.FontReplaced, ct);

    /// <summary>Marks elements with wavy lines and a tooltip. A view setting.</summary>
    /// <param name="marks">The marks.</param>
    /// <param name="clear">Clear the existing marks first.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task MarkElementsAsync(IReadOnlyList<ElementMark> marks, bool? clear = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.MarkElements(marks, clear), ct);

    /// <summary>
    /// Sets the tag prefixes (compared without case) whose picture controls keep their pictures: editing can't delete,
    /// paste over or replace such a picture; <see cref="FillControlAsync"/> and <see cref="SetControlAsync"/> still
    /// change it, and in design mode deleting it removes its whole control. Replaces the prefixes set before; an empty
    /// list protects none. Not saved.
    /// </summary>
    /// <param name="tagPrefixes">The tag prefixes.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetProtectedPictureControlsAsync(IReadOnlyList<string> tagPrefixes, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetProtectedPictureControls(tagPrefixes), ct);

    /// <summary>Selects a picture control (the selected one when none is given) and raises <see cref="EditorSession.PictureControlActivated"/>, in any mode.</summary>
    /// <param name="element">The picture control; null for the selected one.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task ActivatePictureControlAsync(ElementId? element = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.ActivatePictureControl(element), ct);

    /// <summary>Writes a custom XML data part.</summary>
    /// <param name="namespace">The part's namespace.</param>
    /// <param name="xml">The XML (well-formed).</param>
    /// <param name="itemId">The part's item id, when several share the namespace: a GUID, matched without case or braces.</param>
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

    /// <summary>Accepts every revision, one author's, or those Show Markup shows.</summary>
    /// <param name="author">Only this author's revisions.</param>
    /// <param name="shown">Only the revisions Show Markup shows; not with <paramref name="author"/>.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task AcceptAllRevisionsAsync(string? author = null, bool? shown = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.AcceptAllRevisions(author, shown), ct);

    /// <summary>Rejects every revision, one author's, or those Show Markup shows.</summary>
    /// <param name="author">Only this author's revisions.</param>
    /// <param name="shown">Only the revisions Show Markup shows; not with <paramref name="author"/>.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task RejectAllRevisionsAsync(string? author = null, bool? shown = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.RejectAllRevisions(author, shown), ct);

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

    /// <summary>Sets the view options: formatting marks, gridlines, text boundaries, the chrome's theme and accent, dark mode, the measuring detail and the object grid. Unset values are kept.</summary>
    /// <param name="formattingMarks">Formatting marks.</param>
    /// <param name="gridlines">Table gridlines.</param>
    /// <param name="textBoundaries">Text boundaries.</param>
    /// <param name="liveLayout">Text wraps around an object again as it is dragged; otherwise a preview shows until the release.</param>
    /// <param name="theme">The chrome's colour scheme.</param>
    /// <param name="accent">The chrome's accent: selection, handles, guides and active markers. Any colour; the dark scheme lightens it.</param>
    /// <param name="appearance">How the page is drawn on screen (dark mode).</param>
    /// <param name="detail">How much the editor measures while you drag and hover.</param>
    /// <param name="objectGrid">When the dot grid objects snap to shows (unrelated to <paramref name="gridlines"/>).</param>
    /// <param name="themeColors">The application's colours for the light and dark schemes, to match its own UI.</param>
    /// <param name="fieldShading">When fields are shaded (default <see cref="FieldShading.WhenSelected"/>).</param>
    /// <param name="cursorOverrides">Pointer shapes over elements instead of the editor's own, replacing those set before; the first match wins, and an empty list clears them.</param>
    /// <param name="controlLabels">Labels content controls and shown sub-templates while the caret is in them or the pointer is over them (default on); a control whose <c>w15:appearance</c> is <c>hidden</c> is never labelled.</param>
    /// <param name="bookmarks">Shows bookmarks as brackets around their text (an I-beam where they hold none); hidden ones never show.</param>
    /// <param name="tabCharacters">Shows tab characters as arrows, whatever <paramref name="formattingMarks"/> says.</param>
    /// <param name="spaces">Shows spaces as dots and non-breaking spaces as degree signs.</param>
    /// <param name="paragraphMarks">Shows paragraph marks, breaks, and end-of-cell and end-of-row marks.</param>
    /// <param name="hiddenText">Shows hidden text, laid out and dotted underneath (exports leave it out).</param>
    /// <param name="optionalHyphens">Shows optional hyphens, laid out as not signs.</param>
    /// <param name="objectAnchors">Shows the selected floating object's anchor (default on).</param>
    /// <param name="clickAndType">Click and Type: a double-click in blank space puts the caret there (default on).</param>
    /// <param name="textDragAndDrop">Selected text can be dragged and dropped, with either button and out of the view (default on).</param>
    /// <param name="hideWhiteSpace">Hides the white space between pages; a double-click between pages toggles it.</param>
    /// <param name="showEditableRegions">Under enforced protection, highlights the regions the author may edit (default on).</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetViewOptionsAsync(bool? formattingMarks = null, bool? gridlines = null, bool? textBoundaries = null, bool? liveLayout = null, ChromeTheme? theme = null, ColorValue? accent = null, DocumentAppearance? appearance = null, MeasureDetail? detail = null, ObjectGrid? objectGrid = null, ThemeColors? themeColors = null, FieldShading? fieldShading = null, IReadOnlyList<CursorOverride>? cursorOverrides = null, bool? controlLabels = null, bool? bookmarks = null, bool? tabCharacters = null, bool? spaces = null, bool? paragraphMarks = null, bool? hiddenText = null, bool? optionalHyphens = null, bool? objectAnchors = null, bool? clickAndType = null, bool? textDragAndDrop = null, bool? hideWhiteSpace = null, bool? showEditableRegions = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetViewOptions(formattingMarks, gridlines, textBoundaries, liveLayout, theme, accent, appearance, detail, objectGrid, themeColors, fieldShading, cursorOverrides, controlLabels, bookmarks, tabCharacters, spaces, paragraphMarks, hiddenText, optionalHyphens, objectAnchors, clickAndType, textDragAndDrop, hideWhiteSpace, showEditableRegions), ct);

    /// <summary>Switches between editing and design mode.</summary>
    /// <param name="mode">The mode.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetModeAsync(EditorMode mode, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetMode(mode), ct);

    /// <summary>Sets what a click on the ruler adds; answers the tab type picker. A view setting.</summary>
    /// <param name="value">The tab type or indent marker.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetRulerTabTypeAsync(TabSelector value, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetRulerTabType(value), ct);

    /// <summary>Asks for the tab type picker (<see cref="EditorSession.PopoverRequested"/>), as a click on the ruler's tab type box does.</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the command has run.</returns>
    public Task ShowTabTypesAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.ShowTabTypes(), ct);

    /// <summary>Asks for the options of the tab stop nearest the caret (<see cref="EditorSession.PopoverRequested"/>).</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the command has run.</returns>
    public Task ShowTabStopAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.ShowTabStop(), ct);

    /// <summary>Switches AutoCorrect rules on or off, and replaces its lists. A session setting, not an undo step.</summary>
    /// <param name="change">The rules and lists to change.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the setting applies and <see cref="EditorSession.State"/> shows it.</returns>
    public Task SetAutoCorrectAsync(AutoCorrectChange change, CancellationToken ct = default)
    {
        ArgumentNullException.ThrowIfNull(change);
        return _session.RunAsync(EditorOperation.SetAutoCorrect(change), ct);
    }

    /// <summary>Puts back what was typed before the correction at the caret. Undoes the correction when it is the last step, otherwise one step of its own.</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task RevertAutoCorrectAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.RevertAutoCorrect(), ct);

    /// <summary>Reverts the correction at the caret, and never makes it again.</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task NeverAutoCorrectAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.NeverAutoCorrect(), ct);

    /// <summary>Asks for the options of the correction at the caret (<see cref="EditorSession.PopoverRequested"/>, Alt+Shift+F10).</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the command has run.</returns>
    public Task ShowAutoCorrectOptionsAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.ShowAutoCorrectOptions(), ct);

    /// <summary>Dismisses the popover asked for (<see cref="EditorSession.PopoverClosed"/>), for an application that closes its own on Esc or a click elsewhere; with none open, nothing.</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the command has run.</returns>
    public Task ClosePopoverAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.ClosePopover(), ct);
    /// <summary>Says the formatting at the caret to screen readers (an <see cref="EditorSession.AccessibilityAnnouncement"/>
    /// of kind <see cref="AnnouncementKind.Formatting"/>). Shift+F1. Changes nothing.</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the command has run.</returns>
    public Task DescribeFormattingAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.DescribeFormatting(), ct);

    /// <summary>The application's own field types (a code's first word, compared without case), replacing those set before: their fields keep their results, show their labels in design mode, are listed by update requests and are never read as bookmark references; an empty list clears them. A view setting.</summary>
    /// <param name="types">The field types.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetApplicationFieldTypesAsync(IReadOnlyList<string> types, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetApplicationFieldTypes(types), ct);

    /// <summary>Inserts a style separator (Ctrl+Alt+Enter): a hidden paragraph mark that runs the paragraph before it into the one after, each keeping its own style. One undo step.</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task InsertStyleSeparatorAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.InsertStyleSeparator(), ct);

    /// <summary>Alt+X: the hexadecimal code before the caret becomes its character, or the character before the caret (or the selected one) becomes its code. One undo step.</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task ToggleCharacterCodeAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.ToggleCharacterCode(), ct);

    /// <summary>F2: the selected text moves to the place the next click or arrow keys choose, when Enter is pressed (<see cref="SelectionSummary.PendingMove"/> while it waits); Esc or any other command gives up.</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task MoveTextAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.MoveText(), ct);

    /// <summary>Shift+F2: as MoveText, copying the text.</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task CopyTextAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.CopyText(), ct);

    /// <summary>Selects the whole next or previous field in the story, outermost fields only (F11, Shift+F11). Not an edit: no undo step.</summary>
    /// <param name="direction">Next or previous.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SelectFieldAsync(NavigationDirection direction, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SelectField(direction), ct);

    /// <summary>Each selected run's size a point larger, up to 1638 (Ctrl+]).</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task GrowFontOnePointAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.GrowFontOnePoint(), ct);

    /// <summary>Each selected run's size a point smaller, down to 1 (Ctrl+[).</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task ShrinkFontOnePointAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.ShrinkFontOnePoint(), ct);

    /// <summary>Clear All Formatting: the selected paragraphs take the default paragraph style and lose their direct paragraph and character formatting; highlight and revision marks stay.</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task ClearAllFormattingAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.ClearAllFormatting(), ct);

    /// <summary>A hanging indent to the next default tab stop: the left indent moves, the first line stays (Ctrl+T).</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task IncreaseHangingIndentAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.IncreaseHangingIndent(), ct);

    /// <summary>The hanging indent back to the previous default tab stop (Ctrl+Shift+T).</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task DecreaseHangingIndentAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.DecreaseHangingIndent(), ct);

    /// <summary>Reset Paragraph: removes direct paragraph formatting, numbering included; the style stays (Ctrl+Q).</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task ClearParagraphFormattingAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.ClearParagraphFormatting(), ct);

    /// <summary>Moves the selected paragraphs above the one before or below the one after (Alt+Shift+Up, Down); tracked, a move.</summary>
    /// <param name="direction">Up or down.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task MoveParagraphsAsync(VerticalDirection direction, CancellationToken ct = default) => _session.RunAsync(EditorOperation.MoveParagraphs(direction), ct);

    /// <summary>Groups again the objects the last UngroupObjects ungrouped, wherever they moved.</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task RegroupObjectsAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.RegroupObjects(), ct);

    /// <summary>Shows the selected floating object's wrap polygon for editing (<see cref="ViewState.EditingWrapPoints"/>): dragging a vertex moves it, dragging an edge adds one, Ctrl+click removes one, each one undo step that makes the wrap tight. Esc ends it. Not an edit: no undo step.</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task EditWrapPointsAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.EditWrapPoints(), ct);

    /// <summary>Selects ranges of text and blocks of cells of one story, a discontiguous selection when there are several (<see cref="SelectionSummary.Ranges"/> and <see cref="SelectionSummary.CellBlocks"/> read it back): overlapping and touching ranges join, empty ones go. The last range, or the <paramref name="activeCells"/> block, is the selection the next command or gesture goes on from; when every range is empty, the caret goes to the last one's end. At most 100,000 ranges and blocks. Not an edit: no undo step.</summary>
    /// <param name="ranges">The ranges, in any order and either way round; the last is the active one.</param>
    /// <param name="cells">Blocks of cells selected beside the ranges.</param>
    /// <param name="activeCells">The index in <paramref name="cells"/> of the block that is the active selection, in place of the last range.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetSelectionAsync(IReadOnlyList<TextRange> ranges, IReadOnlyList<CellBlock>? cells = null, int? activeCells = null, CancellationToken ct = default) =>
        _session.RunAsync(EditorOperation.SetSelection(ranges, cells, activeCells), ct);

    /// <summary>Shrink Selection (Shift+F8), in extend mode: the selection one level smaller around where extending started (the story to the paragraph, the sentence, the word, then the caret), still extending. Not an edit: no undo step.</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task ShrinkSelectionAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.ShrinkSelection(), ct);

    /// <summary>Select Text with Similar Formatting: every stretch of text in the story with the same character formatting as the selection's first character (or the caret's), as a discontiguous selection. Not an edit: no undo step.</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SelectSimilarFormattingAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.SelectSimilarFormatting(), ct);

    /// <summary>Selects every use of a paragraph or character style in the main document: its paragraphs (marks included, consecutive ones as one range) or its stretches of runs (<see cref="StyleInfo.Uses"/> counts them). Not an edit: no undo step.</summary>
    /// <param name="styleId">The style.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SelectStyleInstancesAsync(StyleId styleId, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SelectStyleInstances(styleId), ct);

    /// <summary>Sets the table's default cell margins (Table Options), 0 to 1584 each; unset sides are kept.</summary>
    /// <param name="margins">The sides to set.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetTableCellMarginsAsync(TableCellMargins margins, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetTableCellMargins(margins), ct);

    /// <summary>Inserts blank cells the size of the selected ones, shifting the selected cells right or down (Insert Cells). Tracked when tracking changes.</summary>
    /// <param name="shift">Where the selected cells go.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task InsertCellsAsync(InsertCellShift shift, CancellationToken ct = default) => _session.RunAsync(EditorOperation.InsertCells(shift), ct);

    /// <summary>Sets the pointer's table tool: while the pen or eraser is on, a press and drag on a page draws or erases instead of selecting.</summary>
    /// <param name="tool">The tool.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetTableToolAsync(TableTool tool, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetTableTool(tool), ct);

    /// <summary>Sets the line the Draw Table pen draws (0.25 to 6 pt). Not saved.</summary>
    /// <param name="line">The line.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetTablePenAsync(CellBorder line, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetTablePen(line), ct);

    /// <summary>A Draw Table pen stroke on a page, in points from its top left: splits the cells it crosses, draws a cell's diagonal, or off any table draws a new one. Untracked.</summary>
    /// <param name="page">The page, from 0.</param>
    /// <param name="from">Where the stroke starts.</param>
    /// <param name="to">Where it ends.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task DrawTableStrokeAsync(int page, PagePoint from, PagePoint to, CancellationToken ct = default) => _session.RunAsync(EditorOperation.DrawTableStroke(page, from, to), ct);

    /// <summary>The table eraser on a page: merges the two cells either side of the border at a point.</summary>
    /// <param name="page">The page, from 0.</param>
    /// <param name="at">The point, in points from the page's top left.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task EraseTableBorderAsync(int page, PagePoint at, CancellationToken ct = default) => _session.RunAsync(EditorOperation.EraseTableBorder(page, at), ct);

    /// <summary>Names the selected object (1 to 255 characters after trimming), as the Selection Pane lists it.</summary>
    /// <param name="name">The name.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetObjectNameAsync(string name, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetObjectName(name), ct);

    /// <summary>Sets the selected object's alternative text title (at most 255 characters; empty clears it).</summary>
    /// <param name="title">The title.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetObjectTitleAsync(string title, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetObjectTitle(title), ct);

    /// <summary>Hides or shows a floating object: hidden, it isn't drawn and text doesn't wrap around it.</summary>
    /// <param name="object">The floating object.</param>
    /// <param name="hidden">Hidden, or shown.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetObjectHiddenAsync(ObjectId @object, bool hidden, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetObjectHidden(@object, hidden), ct);

    /// <summary>Selects a floating object or a group's object, as the Selection Pane does.</summary>
    /// <param name="object">The object.</param>
    /// <param name="extend">Adds it to, or takes it from, the selected floating objects.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SelectObjectAsync(ObjectId @object, bool? extend = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SelectObject(@object, extend), ct);

    /// <summary>Moves a floating object in its page's stack: just in front of another, or behind every other. It keeps its layer.</summary>
    /// <param name="object">The floating object.</param>
    /// <param name="inFrontOf">The floating object it goes just in front of; null: behind every other.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task RestackObjectAsync(ObjectId @object, ObjectId? inFrontOf = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.RestackObject(@object, inFrontOf), ct);

    /// <summary>Whether resizing the selected object keeps its aspect ratio (Lock aspect ratio), saved with it.</summary>
    /// <param name="on">Locked.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetLockAspectRatioAsync(bool on, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetLockAspectRatio(on), ct);

    /// <summary>Sets the selected pictures' recolouring (Picture Color, Recolor).</summary>
    /// <param name="recolor">The recolouring.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetPictureRecolorAsync(Recolor recolor, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetPictureRecolor(recolor), ct);

    /// <summary>Reset Picture: clears the selected picture's adjustments, colour, effects, outline, shape, rotation and flips, keeping its crop; with <paramref name="size"/>, also its crop and size.</summary>
    /// <param name="size">Reset Picture and Size.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task ResetPictureAsync(bool? size = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.ResetPicture(size), ct);

    /// <summary>Sets the selected floating object's wrap polygon (3 to 1024 points), marked edited; a wrap that is neither tight nor through becomes tight.</summary>
    /// <param name="points">The vertices, unclosed.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetWrapPointsAsync(IReadOnlyList<BoxPoint> points, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetWrapPoints(points), ct);

    /// <summary>Makes the selected floating object's wrap polygon again from its outline, no longer edited.</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task ResetWrapPointsAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.ResetWrapPoints(), ct);

    /// <summary>Links the selected text box (the last in its chain) to an empty text box in no chain, in the same story (Create Link).</summary>
    /// <param name="target">The text box its text flows on into.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task LinkTextBoxAsync(ObjectId target, CancellationToken ct = default) => _session.RunAsync(EditorOperation.LinkTextBox(target), ct);

    /// <summary>Starts linking the selected text box with the pointer: the next click on an empty text box links it; elsewhere or Escape cancels.</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task BeginTextBoxLinkAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.BeginTextBoxLink(), ct);

    /// <summary>Breaks the link from the selected text box to the next (Break Link).</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task UnlinkTextBoxAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.UnlinkTextBox(), ct);

    /// <summary>Turns Select Objects on or off: a drag off every object selects the objects wholly inside its rectangle.</summary>
    /// <param name="on">On.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetSelectObjectsAsync(bool on, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetSelectObjects(on), ct);

    /// <summary>Promote (Alt+Shift+Left): each selected paragraph a level up; body text takes the level of the heading before it (Heading 1 when none).</summary>
    /// <param name="subtopics">The headings under each heading changed move by as many levels; null for no.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task PromoteOutlineAsync(bool? subtopics = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.PromoteOutline(subtopics), ct);

    /// <summary>Demote (Alt+Shift+Right): each selected heading a level down; Heading 9 and body text stay.</summary>
    /// <param name="subtopics">The headings under each heading changed move by as many levels; null for no.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task DemoteOutlineAsync(bool? subtopics = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.DemoteOutline(subtopics), ct);

    /// <summary>Promote to Heading 1: the selected paragraphs take Heading 1, their direct outline level removed.</summary>
    /// <param name="subtopics">The headings under each heading changed move by as many levels; null for no.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task PromoteToHeading1Async(bool? subtopics = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.PromoteToHeading1(subtopics), ct);

    /// <summary>Demote to Body Text: the selected paragraphs take the default paragraph style, their direct outline level removed.</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task DemoteToBodyTextAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.DemoteToBodyText(), ct);

    /// <summary>A navigation pane's Promote on one heading.</summary>
    /// <param name="heading">The heading.</param>
    /// <param name="subtopics">Its subheadings move with it; null for yes.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task PromoteHeadingAsync(HeadingId heading, bool? subtopics = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.PromoteHeading(heading, subtopics), ct);

    /// <summary>A navigation pane's Demote on one heading.</summary>
    /// <param name="heading">The heading.</param>
    /// <param name="subtopics">Its subheadings move with it; null for yes.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task DemoteHeadingAsync(HeadingId heading, bool? subtopics = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.DemoteHeading(heading, subtopics), ct);

    /// <summary>Moves a heading with its content before another heading, or after that heading's content, optionally at a new level; the caret goes to its start.</summary>
    /// <param name="heading">The heading moved.</param>
    /// <param name="target">The heading it goes beside.</param>
    /// <param name="side">Before the target, or after its content.</param>
    /// <param name="level">Its new level, 1 to 9; null keeps it.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task MoveHeadingAsync(HeadingId heading, HeadingId target, HeadingSide side, int? level = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.MoveHeading(heading, target, side, level), ct);

    /// <summary>Deletes a heading and its content.</summary>
    /// <param name="heading">The heading.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task DeleteHeadingAsync(HeadingId heading, CancellationToken ct = default) => _session.RunAsync(EditorOperation.DeleteHeading(heading), ct);

    /// <summary>An empty heading at a heading's level just before it, the caret in it.</summary>
    /// <param name="heading">The heading.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task InsertHeadingBeforeAsync(HeadingId heading, CancellationToken ct = default) => _session.RunAsync(EditorOperation.InsertHeadingBefore(heading), ct);

    /// <summary>An empty heading at a heading's level after its content, the caret in it.</summary>
    /// <param name="heading">The heading.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task InsertHeadingAfterAsync(HeadingId heading, CancellationToken ct = default) => _session.RunAsync(EditorOperation.InsertHeadingAfter(heading), ct);

    /// <summary>An empty heading a level below a heading, after its content, the caret in it.</summary>
    /// <param name="heading">The heading.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task InsertSubheadingAsync(HeadingId heading, CancellationToken ct = default) => _session.RunAsync(EditorOperation.InsertSubheading(heading), ct);

    /// <summary>Selects a heading and its content. Not an edit: no undo step.</summary>
    /// <param name="heading">The heading.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SelectHeadingContentAsync(HeadingId heading, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SelectHeadingContent(heading), ct);

    /// <summary>Sets the separator, continuation separator or continuation notice of footnotes or endnotes, or back to the built-in one.</summary>
    /// <param name="kind">Footnotes' or endnotes'.</param>
    /// <param name="part">Which separator.</param>
    /// <param name="text">Its text, paragraphs <c>\n</c> apart (empty: an empty line); null for the built-in one (a line; no notice).</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetNoteSeparatorAsync(NoteType kind, NoteSeparatorPart part, string? text = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetNoteSeparator(kind, part, text), ct);

    /// <summary>Lets an editor edit the selection, or stops them editing the editable ranges reaching it.</summary>
    /// <param name="editor">The group or person.</param>
    /// <param name="allowed">They may edit the selection; false: no longer the ranges reaching it.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetEditableRangeEditorAsync(RangeEditorRef editor, bool allowed, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetEditableRangeEditor(editor, allowed), ct);

    /// <summary>Selects the next or previous editable range the author may edit (Find Next Region I Can Edit). Not an edit: no undo step.</summary>
    /// <param name="direction">The direction.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SelectNextEditableRangeAsync(NavigationDirection direction, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SelectNextEditableRange(direction), ct);

    /// <summary>Selects every editable range the author may edit (Show All Regions I Can Edit). Not an edit: no undo step.</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SelectEditableRangesAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.SelectEditableRanges(), ct);

    /// <summary>The groups the author is in; editable ranges for them let the author edit. Kept for the documents opened after.</summary>
    /// <param name="groups">The groups (everyone is always one).</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetEditingGroupsAsync(IReadOnlyList<string> groups, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetEditingGroups(groups), ct);

    /// <summary>Selects the next or previous legacy form field (Tab and Shift+Tab where forms are filled in). Not an edit: no undo step.</summary>
    /// <param name="direction">The direction.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SelectNextFormFieldAsync(NavigationDirection direction, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SelectNextFormField(direction), ct);

    /// <summary>Adds an item to a repeating section: a copy of one of its items, with its content; the caret goes to the copy's start.</summary>
    /// <param name="id">The repeating section.</param>
    /// <param name="item">The item copied; null for the last.</param>
    /// <param name="before">The copy goes before it; null for after.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task AddRepeatingItemAsync(ElementId id, ElementId? item = null, bool? before = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.AddRepeatingItem(id, item, before), ct);

    /// <summary>Which kinds of markup show (Show Markup); unset values are kept. Not an edit: no undo step.</summary>
    /// <param name="insertionsDeletions">Insertions and deletions.</param>
    /// <param name="formatting">Formatting changes.</param>
    /// <param name="comments">Comments.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetShowMarkupAsync(bool? insertionsDeletions = null, bool? formatting = null, bool? comments = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetShowMarkup(insertionsDeletions, formatting, comments), ct);

    /// <summary>The reviewers whose revisions and comments don't show, by name; empty: all reviewers show. Not an edit: no undo step.</summary>
    /// <param name="hidden">The reviewers hidden.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetHiddenReviewersAsync(IReadOnlyList<string> hidden, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetHiddenReviewers(hidden), ct);

    /// <summary>Where revisions show in All Markup. Not an edit: no undo step.</summary>
    /// <param name="mode">The mode.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetBalloonModeAsync(BalloonMode mode, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetBalloonMode(mode), ct);

    /// <summary>How revisions draw (Track Changes Options); unset values are kept. Not an edit: no undo step.</summary>
    /// <param name="change">The settings to change.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetMarkupStyleAsync(MarkupStyleChange change, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetMarkupStyle(change), ct);

    /// <summary>Whether tracking records moves as moves and formatting changes; unset values are kept. Saved.</summary>
    /// <param name="moves">Moves are tracked as moves.</param>
    /// <param name="formatting">Formatting changes are tracked.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetTrackOptionsAsync(bool? moves = null, bool? formatting = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetTrackOptions(moves, formatting), ct);

    /// <summary>Accepts every revision and turns tracking off (Accept All Changes and Stop Tracking).</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task AcceptAllAndStopTrackingAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.AcceptAllAndStopTracking(), ct);

    /// <summary>Deletes every comment, resolved ones too, or the threads Show Markup shows.</summary>
    /// <param name="shownOnly">Only the threads Show Markup shows; null for every one.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task DeleteAllCommentsAsync(bool? shownOnly = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.DeleteAllComments(shownOnly), ct);

    /// <summary>Pastes the paste just made again another way, in place of it (the Paste Options button's choices): refused when there is no paste to change or it can't be pasted that way. One undo step takes the document back to before the first paste.</summary>
    /// <param name="mode">How it pastes again.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task RepasteAsync(PasteMode mode, CancellationToken ct = default) => _session.RunAsync(EditorOperation.Repaste(mode), ct);

    /// <summary>Hides the Paste Options button of the paste just made. Not an edit: no undo step.</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task DismissPasteOptionsAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.DismissPasteOptions(), ct);

    /// <summary>Changes the cut, copy and paste options (<see cref="EditorState.PasteOptions"/>); null values are kept. A session setting, never saved; works when read only.</summary>
    /// <param name="change">The options that change.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetPasteOptionsAsync(PasteOptionsChange change, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetPasteOptions(change), ct);

    /// <summary>Inserts a DOCX's content in place of the selection (Insert Text from File): its paragraphs, tables, styles and sections.</summary>
    /// <param name="from">The DOCX's bytes.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task InsertFileAsync(ReadOnlyMemory<byte> from, CancellationToken ct = default) => _session.RunAsync(EditorOperation.InsertFile(from), ct);

    /// <summary>Go To: the n-th, n on or back, the next or the previous page, section, line, bookmark, comment, note, field, table, graphic, equation, embedded object or heading. Fails as unavailable when there is none to go to. Not an edit: no undo step.</summary>
    /// <param name="what">What it goes to.</param>
    /// <param name="target">Which one: numbers past the last go to the last.</param>
    /// <param name="name">A bookmark's name (with an n-th target, that bookmark), a comment's reviewer, a field's kind, an embedded object's program id.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task GoToAsync(GoToWhat what, GoToTarget target, string? name = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.GoTo(what, target, name), ct);

    /// <summary>A table of contents in place of the selection (in place of the table the selection is in); with no options, Word's. Body only.</summary>
    /// <param name="options">What it collects; null for Word's default table.</param>
    /// <param name="layout">How its entries are laid out.</param>
    /// <param name="frame">Its title and building block control.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task InsertTableOfContentsAsync(TableOfContentsOptions? options = null, TocLayout? layout = null, TocFrame? frame = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.InsertTableOfContents(options, layout, frame), ct);

    /// <summary>A table of figures of the captions labelled <paramref name="label"/>, in place of the selection (or of the table it is in).</summary>
    /// <param name="label">The captions' label.</param>
    /// <param name="includeLabelAndNumber">Entries keep the label and number; null for on.</param>
    /// <param name="options">Its other options; null for hyperlinks and hidden in a web view.</param>
    /// <param name="layout">How its entries are laid out.</param>
    /// <param name="frame">Its title and building block control.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task InsertTableOfFiguresAsync(string label, bool? includeLabelAndNumber = null, TableOfContentsOptions? options = null, TocLayout? layout = null, TocFrame? frame = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.InsertTableOfFigures(label, includeLabelAndNumber, options, layout, frame), ct);

    /// <summary>Word's Manual Table: typed entries in a building block control, at the start of a body paragraph.</summary>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task InsertManualTableOfContentsAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.InsertManualTableOfContents(), ct);

    /// <summary>Updates the table of contents or figures the selection is in (else the first; <paramref name="index"/>: that one of the tables of contents): its page numbers, or every entry again.</summary>
    /// <param name="mode">What it rewrites.</param>
    /// <param name="index">Which table; null for the one at the selection.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task UpdateTableOfContentsAsync(TocUpdateMode mode, int? index = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.UpdateTableOfContents(mode, index), ct);

    /// <summary>Updates every table of contents and of figures but the locked ones (as before printing).</summary>
    /// <param name="mode">What it rewrites.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>How many tables it updated.</returns>
    public Task<int> UpdateAllTablesOfContentsAsync(TocUpdateMode mode, CancellationToken ct = default) => _session.RunAsync(EditorOperation.UpdateAllTablesOfContents(mode), CommitValue.Changed, ct);

    /// <summary>Removes the table of contents the selection is in (else the first; <paramref name="index"/>: that one), with its building block control and title.</summary>
    /// <param name="index">Which table; null for the one at the selection.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task RemoveTableOfContentsAsync(int? index = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.RemoveTableOfContents(index), ct);

    /// <summary>Marks a table of contents entry after the selection (Mark Entry, Alt+Shift+O): a hidden table entry field.</summary>
    /// <param name="text">The entry's text.</param>
    /// <param name="level">Its level, 1 to 9.</param>
    /// <param name="identifier">The tables that collect it: those taking entry fields of this identifier; null for <c>C</c>.</param>
    /// <param name="pageNumber">It shows a page number; null for on.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>The field made.</returns>
    public Task<ElementId> MarkTableOfContentsEntryAsync(string text, int level, string? identifier = null, bool? pageNumber = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.MarkTableOfContentsEntry(text, level, identifier, pageNumber), CommitValue.ElementId, ct);

    /// <summary>A caption for the selected table, picture or object (or the caret's paragraph); the label's other captions renumber.</summary>
    /// <param name="label">The label: a letter, then letters, digits, spaces and underscores.</param>
    /// <param name="text">What follows the number, as it is (put your own separator first).</param>
    /// <param name="position">Above or below the item; null for above a table, below anything else.</param>
    /// <param name="excludeLabel">The number without the label before it.</param>
    /// <param name="format">The number's format: decimal (null), letters or roman numerals.</param>
    /// <param name="chapter">A chapter number before it.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task InsertCaptionAsync(string label, string? text = null, CaptionPosition? position = null, bool? excludeLabel = null, NumberFormat? format = null, CaptionChapter? chapter = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.InsertCaption(label, text, position, excludeLabel, format, chapter), ct);

    /// <summary>The caption every table inserted from now on gets (AutoCaption); null turns it off. A session setting, never saved; works when read only.</summary>
    /// <param name="caption">The caption; null for off.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>Completes when the change has committed and <see cref="EditorSession.State"/> is at its revision.</returns>
    public Task SetAutoCaptionAsync(AutoCaption? caption, CancellationToken ct = default) => _session.RunAsync(EditorOperation.SetAutoCaption(caption), ct);

    /// <summary>A cross-reference to the item at <see cref="CrossReferenceTargetRef.Index"/> of what <see cref="EditorReferences.GetCrossReferenceTargetsAsync"/> lists, at the selection.</summary>
    /// <param name="target">What it points to.</param>
    /// <param name="content">What it shows.</param>
    /// <param name="hyperlink">It links to its target; null for on.</param>
    /// <param name="aboveBelow">"above" or "below" follows it.</param>
    /// <param name="ct">Cancels the command before it runs.</param>
    /// <returns>The field made.</returns>
    public Task<ElementId> InsertCrossReferenceAsync(CrossReferenceTargetRef target, CrossReferenceContent content, bool? hyperlink = null, bool? aboveBelow = null, CancellationToken ct = default) => _session.RunAsync(EditorOperation.InsertCrossReference(target, content, hyperlink, aboveBelow), CommitValue.ElementId, ct);
}
