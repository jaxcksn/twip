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

    /// <summary>Inserts a line break (Shift+Enter), or a text wrapping break: the text after it goes below the floating objects on that side.</summary>
    /// <param name="clear">The floating objects a text wrapping break clears; null for a plain line break.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation InsertLineBreak(BreakClear? clear = null) => Op(CommandId.InsertLineBreak, new Arg("clear", Nullable(clear), Optional: true));

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

    /// <summary>Undoes the last steps, as one change.</summary>
    /// <param name="count">How many (at least 1; more than there are undoes them all); null for one.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation Undo(int? count = null) => Op(CommandId.Undo, new Arg("count", Nullable(count), Optional: true));

    /// <summary>Redoes the last undone steps, as one change.</summary>
    /// <param name="count">How many (at least 1; more than there are redoes them all); null for one.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation Redo(int? count = null) => Op(CommandId.Redo, new Arg("count", Nullable(count), Optional: true));

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

    /// <summary>Selects a bookmark's text, or puts the caret at it when it holds none (Go To); <c>_top</c> is the document's start.</summary>
    /// <param name="name">The bookmark, compared without case.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation GoToBookmark(string name) => Op(CommandId.GoToBookmark, new Arg("value", Node(name)));

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

    /// <summary>Sets the font of the selection: a family, or the theme's heading or body font, which follows the theme.</summary>
    /// <param name="font">The family name (non-empty; a string converts), or <see cref="FontChoice.FromTheme"/>.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetFont(FontChoice font) => Op(CommandId.SetFont, new Arg("value", Node(font)));

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
    /// <summary>Applies a character style to the selected text (a linked paragraph style's character half), or removes it.</summary>
    /// <param name="styleId">The style; <see langword="null"/> removes the character style.</param>
    public static EditorOperation ApplyCharacterStyle(StyleId? styleId) => Op(CommandId.ApplyCharacterStyle, new Arg("value", styleId is { } id ? Node(id) : null));
    /// <summary>Sets the scale, kerning and OpenType features; unset values are kept.</summary>
    /// <param name="options">The settings.</param>
    public static EditorOperation SetTypography(TypographyOptions options) => Op(CommandId.SetTypography, Arg.Flatten(options));

    /// <summary>Fits the selected text to a width (Fit Text), or lets it take its own; a caret in fitted text acts on its whole region.</summary>
    /// <param name="width">The width, more than 0 and at most 1584 pt; null removes the fitting.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetFitText(Length? width) => Op(CommandId.SetFitText, new Arg("value", Nullable(width)));
    /// <summary>Sets the text's language and whether spelling is checked; unset values are kept.</summary>
    /// <param name="options">The settings.</param>
    public static EditorOperation SetLanguage(LanguageOptions options) => Op(CommandId.SetLanguage, Arg.Flatten(options));
    /// <summary>Sets emboss, engrave, outline and shadow; unset values are kept.</summary>
    /// <param name="options">The effects.</param>
    public static EditorOperation SetCharacterEffects(CharacterEffectsOptions options) => Op(CommandId.SetCharacterEffects, Arg.Flatten(options));
    /// <summary>Sets the underline's colour, or the text's (<see cref="ColorValue.Auto"/>); underlined text only.</summary>
    /// <param name="color">The colour.</param>
    public static EditorOperation SetUnderlineColor(ColorValue color) => Op(CommandId.SetUnderlineColor, new Arg("value", Node(color)));

    /// <summary>Removes direct character formatting from the selection.</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation ClearFormatting() => Op(CommandId.ClearFormatting);

    /// <summary>Changes the case of the selected text, or of the word at a collapsed caret. Shift+F3 cycles lower, UPPER, Capitalize Each Word (Sentence case for a whole sentence).</summary>
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
    /// <param name="contextual">No space between paragraphs of the same style.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetParagraphSpacing(SpacingValue? before = null, SpacingValue? after = null, bool? contextual = null) => Op(CommandId.SetParagraphSpacing, new Arg("before", before is { } before_ ? Node(before_) : null, Optional: true), new Arg("after", after is { } after_ ? Node(after_) : null, Optional: true), new Arg("contextual", contextual is { } contextual_ ? Node(contextual_) : null, Optional: true));

    /// <summary>Sets the indents of the selected paragraphs. Unset values are kept.</summary>
    /// <param name="options">The indents to change.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetIndents(IndentOptions options) => Op(CommandId.SetIndents, Arg.Flatten(options));

    /// <summary>Sets the outline level and whether the paragraphs open collapsed; unset values are kept.</summary>
    /// <param name="options">The settings.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetOutline(OutlineOptions options) => Op(CommandId.SetOutline, Arg.Flatten(options));

    /// <summary>Sets the selected paragraphs' text direction.</summary>
    /// <param name="direction">The direction.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetParagraphTextDirection(TextFlow direction) => Op(CommandId.SetParagraphTextDirection, new Arg("value", Node(direction)));

    /// <summary>Increases the indent (Ctrl+M): list items go a level down, or the whole list moves from its first item; other paragraphs go to the next default tab stop.</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation IncreaseIndent() => Op(CommandId.IncreaseIndent);

    /// <summary>Decreases the indent (Ctrl+Shift+M): list items go a level up, or the whole list moves back from its first item; other paragraphs go to the previous default tab stop.</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation DecreaseIndent() => Op(CommandId.DecreaseIndent);

    /// <summary>Applies a paragraph style to the selected paragraphs.</summary>
    /// <param name="styleId">An existing paragraph style.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetParagraphStyle(StyleId styleId) => Op(CommandId.SetParagraphStyle, new Arg("value", Node(styleId)));

    /// <summary>Changes a style to the selection's formatting.</summary>
    /// <param name="styleId">The style.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation UpdateStyleToSelection(StyleId styleId) => Op(CommandId.UpdateStyleToSelection, new Arg("value", Node(styleId)));

    /// <summary>Deletes a style the document made.</summary>
    /// <param name="styleId">The style.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation DeleteStyle(StyleId styleId) => Op(CommandId.DeleteStyle, new Arg("value", Node(styleId)));

    /// <summary>Sets a style's list flags.</summary>
    /// <param name="options">The style and what to set.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetStyleMeta(StyleMetaOptions options) => Op(CommandId.SetStyleMeta, Arg.Flatten(options));

    /// <summary>Sets the document's default formatting.</summary>
    /// <param name="defaults">The defaults to set.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetDocumentDefaults(DocumentDefaults defaults) => Op(CommandId.SetDocumentDefaults, Arg.Flatten(defaults));

    /// <summary>
    /// Sets the document's default formatting with any character and paragraph formatting operations but
    /// <see cref="SetFrame"/> and <see cref="RemoveFrame"/>, run on the defaults as on a style, as one undo step.
    /// </summary>
    /// <param name="commands">The formatting.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetDefaultFormatting(IReadOnlyList<EditorOperation> commands) => Op(CommandId.SetDefaultFormatting, new Arg("value", Node(commands)));

    /// <summary>Makes a style (New Style), from the selection's formatting or not, applied to the selection or not, as one undo step. Its id is the result.</summary>
    /// <param name="style">Its type, name, base, following style, link, list and formatting.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation CreateStyle(NewStyle style) => Op(CommandId.CreateStyle, Arg.Flatten(style));

    /// <summary>
    /// Changes a style (Modify Style); a built-in style the document lacks joins it. Renaming a built-in style adds an
    /// alias; linking sets both sides; a linked style's character formatting goes to its other half too.
    /// </summary>
    /// <param name="id">The style.</param>
    /// <param name="change">The change.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation ModifyStyle(StyleId id, StyleChange change) => Op(CommandId.ModifyStyle, new Arg("id", Node(id)), new Arg("change", Node(change)));

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

    /// <summary>Sets the selected paragraphs' border lines: every edge, null removing it.</summary>
    /// <param name="lines">The edges.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetParagraphBorderLines(ParagraphBorderLines lines) => Op(CommandId.SetParagraphBorderLines, Arg.Flatten(lines));

    /// <summary>Shades the selected paragraphs or text.</summary>
    /// <param name="shading">The target, fill and pattern.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetShading(ShadingOptions shading) => Op(CommandId.SetShading, Arg.Flatten(shading));

    /// <summary>Sets or removes the selected text's border.</summary>
    /// <param name="border">The box, or a null line to remove it.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetTextBorder(TextBorderOptions border) => Op(CommandId.SetTextBorder, Arg.Flatten(border));

    /// <summary>Sets the page borders.</summary>
    /// <param name="borders">The edges and options.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetPageBorders(PageBordersOptions borders) => Op(CommandId.SetPageBorders, Arg.Flatten(borders));

    /// <summary>Sets the properties of the frame the selection is in.</summary>
    /// <param name="properties">The frame's properties.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetFrame(FrameProperties properties) => Op(CommandId.SetFrame, Arg.Flatten(properties));

    /// <summary>Removes the frame the selection is in.</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation RemoveFrame() => Op(CommandId.RemoveFrame);

    /// <summary>Sets the selection's paragraph's drop cap: its first letter dropped into the text or in the margin, or put back.</summary>
    /// <param name="dropCap">The drop cap; <see cref="DropCapPosition.None"/> puts the letter back.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetDropCap(DropCap dropCap) => Op(CommandId.SetDropCap, Arg.Flatten(dropCap));

    /// <summary>Sets the text's fill, outline, shadow, glow and reflection; each is kept, removed or set.</summary>
    /// <param name="change">The change.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetTextEffects(TextEffectsChange change) => Op(CommandId.SetTextEffects, Arg.Flatten(change));

    /// <summary>Sets the document's page colour; <see cref="FillValue.None"/> removes it.</summary>
    /// <param name="fill">None, a solid colour, a gradient, a pattern or a picture (tiled, a texture).</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetPageColor(FillValue fill) => Op(CommandId.SetPageColor, new Arg("value", Node(fill)));

    /// <summary>Sets the document's watermark, in every header.</summary>
    /// <param name="watermark">The text or picture watermark.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetWatermark(Watermark watermark) => Op(CommandId.SetWatermark, new Arg("value", Node(watermark)));

    /// <summary>Removes the document's watermark.</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation RemoveWatermark() => Op(CommandId.RemoveWatermark);

    /// <summary>Defines the selection's list levels.</summary>
    /// <param name="definition">The levels and what they apply to.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetListDefinition(ListDefinition definition) => Op(CommandId.SetListDefinition, Arg.Flatten(definition));

    /// <summary>Starts a new list at a value, or continues the previous one.</summary>
    /// <param name="options">The mode and value.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetNumberingValue(NumberingValueOptions options) => Op(CommandId.SetNumberingValue, Arg.Flatten(options));

    /// <summary>Restarts the list at this paragraph.</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation RestartNumbering() => Op(CommandId.RestartNumbering);

    /// <summary>Continues the previous list from this paragraph.</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation ContinueNumbering() => Op(CommandId.ContinueNumbering);

    /// <summary>Adjusts the selection's list level's indents.</summary>
    /// <param name="options">The indents.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation AdjustListIndents(ListIndentOptions options) => Op(CommandId.AdjustListIndents, Arg.Flatten(options));

    /// <summary>Applies a list gallery entry.</summary>
    /// <param name="id">The entry.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation ApplyListEntry(string id) => Op(CommandId.ApplyListEntry, new Arg("value", Node(id)));

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

    /// <summary>Turns automatic hyphenation on or off for the document.</summary>
    /// <param name="on">On, with the hyphenation patterns of the text's language.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetAutoHyphenation(bool on) => Op(CommandId.SetAutoHyphenation, new Arg("value", Node(on)));

    /// <summary>Sets the hyphenation zone: in documents in compatibility mode 14 and earlier, a word is hyphenated only
    /// when moving it whole would leave this gap.</summary>
    /// <param name="zone">The zone, 0 to 1584 pt.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetHyphenationZone(Length zone) => Op(CommandId.SetHyphenationZone, new Arg("value", Node(zone)));

    /// <summary>Sets the most consecutive lines that may end in a hyphen.</summary>
    /// <param name="limit">The limit, 0 to 32767; 0 for no limit.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetConsecutiveHyphenLimit(int limit) => Op(CommandId.SetConsecutiveHyphenLimit, new Arg("value", Node(limit)));

    /// <summary>Hyphenates words in capitals, or not.</summary>
    /// <param name="on">Whether words in capitals are hyphenated.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetHyphenateCaps(bool on) => Op(CommandId.SetHyphenateCaps, new Arg("value", Node(on)));

    /// <summary>Sets the page margins of the selection's sections. Unset values are kept.</summary>
    /// <param name="options">The margins to change.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetMargins(MarginOptions options) => Op(CommandId.SetMargins, Arg.Flatten(options));

    /// <summary>Sets the page orientation of the selection's sections.</summary>
    /// <param name="orientation">The orientation.</param>
    /// <param name="applyTo">The sections: the selection's (null), every section, or from the selection on (a new-page section break goes at its start).</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetOrientation(PageOrientation orientation, PageSetupScope? applyTo = null) => Op(CommandId.SetOrientation, new Arg("value", Node(orientation)), new Arg("applyTo", applyTo is { } applyTo_ ? Node(applyTo_) : null, Optional: true));

    /// <summary>Sets the paper size of the selection's sections, given in portrait.</summary>
    /// <param name="width">The width, 36 to 1584 pt.</param>
    /// <param name="height">The height, 36 to 1584 pt.</param>
    /// <param name="applyTo">The sections: the selection's (null), every section, or from the selection on (a new-page section break goes at its start).</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetPaperSize(Length width, Length height, PageSetupScope? applyTo = null) => Op(CommandId.SetPaperSize, new Arg("width", Node(width)), new Arg("height", Node(height)), new Arg("applyTo", applyTo is { } applyTo_ ? Node(applyTo_) : null, Optional: true));

    /// <summary>Sets the text columns of the selection's sections.</summary>
    /// <param name="count">The number of columns, 1 to 45.</param>
    /// <param name="spacing">The space between columns.</param>
    /// <param name="separator">Whether a line separates the columns.</param>
    /// <param name="widths">Columns of their own widths, each with the space after it; equal columns when null.</param>
    /// <param name="applyTo">Where: the selection's sections (null), every section, from the selection on (a continuous section break at its start), or the selected text alone.</param>
    /// <param name="startNewColumn">With <see cref="ColumnsApplyTo.ThisPointForward"/>: the text from the selection on starts a new column.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetColumns(int count, Length? spacing = null, bool? separator = null, IReadOnlyList<ColumnWidth>? widths = null, ColumnsApplyTo? applyTo = null, bool? startNewColumn = null) => Op(CommandId.SetColumns, new Arg("count", Node(count)), new Arg("spacing", spacing is { } spacing_ ? Node(spacing_) : null, Optional: true), new Arg("separator", separator is { } separator_ ? Node(separator_) : null, Optional: true), new Arg("widths", widths is { } widths_ ? Node(widths_) : null, Optional: true), new Arg("applyTo", applyTo is { } applyTo_ ? Node(applyTo_) : null, Optional: true), new Arg("startNewColumn", startNewColumn is { } startNewColumn_ ? Node(startNewColumn_) : null, Optional: true));

    /// <summary>Sets the header and footer distances from the page edge. Unset values are kept.</summary>
    /// <param name="header">The header distance.</param>
    /// <param name="footer">The footer distance.</param>
    /// <param name="applyTo">The sections: the selection's (null), every section, or from the selection on (a new-page section break goes at its start).</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetHeaderFooterDistance(Length? header = null, Length? footer = null, PageSetupScope? applyTo = null) => Op(CommandId.SetHeaderFooterDistance, new Arg("header", header is { } header_ ? Node(header_) : null, Optional: true), new Arg("footer", footer is { } footer_ ? Node(footer_) : null, Optional: true), new Arg("applyTo", applyTo is { } applyTo_ ? Node(applyTo_) : null, Optional: true));

    /// <summary>Gives the first page its own header and footer.</summary>
    /// <param name="on">Whether the first page differs.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetDifferentFirstPage(bool on) => Op(CommandId.SetDifferentFirstPage, new Arg("value", Node(on)));

    /// <summary>Gives odd and even pages their own headers and footers.</summary>
    /// <param name="on">Whether odd and even pages differ.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetDifferentOddEvenPages(bool on) => Op(CommandId.SetDifferentOddEvenPages, new Arg("value", Node(on)));

    /// <summary>Sets the section's page number format, first number and chapter numbers; unset values are kept.</summary>
    /// <param name="options">The numbering.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetPageNumbering(PageNumberingOptions options) => Op(CommandId.SetPageNumbering, Arg.Flatten(options));

    /// <summary>Sets where text sits between the section's top and bottom margins.</summary>
    /// <param name="alignment">The alignment.</param>
    /// <param name="applyTo">The sections: the selection's (null), every section, or from the selection on (a new-page section break goes at its start).</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetPageVerticalAlignment(PageVerticalAlignment alignment, PageSetupScope? applyTo = null) => Op(CommandId.SetPageVerticalAlignment, new Arg("value", Node(alignment)), new Arg("applyTo", applyTo is { } applyTo_ ? Node(applyTo_) : null, Optional: true));

    /// <summary>Numbers the section's lines, or stops.</summary>
    /// <param name="numbers">The numbering; <see langword="null"/> stops it.</param>
    /// <param name="applyTo">The sections: the selection's (null), every section, or from the selection on (a new-page section break goes at its start).</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetLineNumbering(LineNumbers? numbers, PageSetupScope? applyTo = null) => Op(CommandId.SetLineNumbering, new Arg("value", numbers is { } n ? Node(n) : null), new Arg("applyTo", applyTo is { } applyTo_ ? Node(applyTo_) : null, Optional: true));

    /// <summary>Sets where the section starts: the break before it.</summary>
    /// <param name="kind">The break.</param>
    /// <param name="applyTo">The sections: the selection's (null), every section, or from the selection on (a new-page section break goes at its start).</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetSectionBreakType(SectionBreakKind kind, PageSetupScope? applyTo = null) => Op(CommandId.SetSectionBreakType, new Arg("value", Node(kind)), new Arg("applyTo", applyTo is { } applyTo_ ? Node(applyTo_) : null, Optional: true));

    /// <summary>Sets the section's text direction.</summary>
    /// <param name="direction">The direction.</param>
    /// <param name="applyTo">The sections: the selection's (null), every section, or from the selection on (a new-page section break goes at its start).</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetSectionTextDirection(TextFlow direction, PageSetupScope? applyTo = null) => Op(CommandId.SetSectionTextDirection, new Arg("value", Node(direction)), new Arg("applyTo", applyTo is { } applyTo_ ? Node(applyTo_) : null, Optional: true));

    /// <summary>Sets how the document's pages pair and where the gutter is; unset values are kept.</summary>
    /// <param name="options">The settings.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetMultiplePages(MultiplePagesOptions options) => Op(CommandId.SetMultiplePages, Arg.Flatten(options));

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
    /// <param name="showOnFirstPage">The number shows on the section's first page (null: yes); false gives the section a different first page.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation InsertPageNumber(PageNumberFormat format, PageNumberPlacement? where = null, PageNumberAlignment? alignment = null, bool? showOnFirstPage = null) => Op(CommandId.InsertPageNumber, new Arg("format", Node(format)), new Arg("where", where is { } where_ ? Node(where_) : null, Optional: true), new Arg("alignment", alignment is { } alignment_ ? Node(alignment_) : null, Optional: true), new Arg("showOnFirstPage", showOnFirstPage is { } showOnFirstPage_ ? Node(showOnFirstPage_) : null, Optional: true));

    /// <summary>The caret section's header or footer takes the section before's (Link to Previous), or gets its own copy of it.</summary>
    /// <param name="kind">A header or a footer.</param>
    /// <param name="pages">Which pages it is for.</param>
    /// <param name="linked">Takes the section before's.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetHeaderFooterLinked(HeaderFooterKind kind, HeaderFooterType pages, bool linked) => Op(CommandId.SetHeaderFooterLinked, new Arg("kind", Node(kind)), new Arg("pages", Node(pages)), new Arg("linked", Node(linked)));

    /// <summary>Empties the caret section's header or footer (its own, so a linked one stops linking).</summary>
    /// <param name="kind">A header or a footer.</param>
    /// <param name="pages">Which pages it is for.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation RemoveHeaderFooter(HeaderFooterKind kind, HeaderFooterType pages) => Op(CommandId.RemoveHeaderFooter, new Arg("kind", Node(kind)), new Arg("pages", Node(pages)));

    /// <summary>Inserts a tab to a place set from the margin or the indent, not from the tab stops (Insert Alignment Tab).</summary>
    /// <param name="alignment">How the text after it aligns at its place.</param>
    /// <param name="relativeTo">What its place is measured in.</param>
    /// <param name="leader">What fills the space before it; none when null.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation InsertAlignmentTab(AlignmentTabKind alignment, AlignmentTabBase relativeTo, TabLeader? leader = null) => Op(CommandId.InsertAlignmentTab, new Arg("alignment", Node(alignment)), new Arg("relativeTo", Node(relativeTo)), new Arg("leader", leader is { } leader_ ? Node(leader_) : null, Optional: true));

    /// <summary>Inserts a table at the selection.</summary>
    /// <param name="rows">The number of rows, 1 to 32767.</param>
    /// <param name="columns">The number of columns, 1 to 63.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation InsertTable(int rows, int columns) => Op(CommandId.InsertTable, new Arg("rows", Node(rows)), new Arg("columns", Node(columns)));

    /// <summary>Inserts a picture at the selection.</summary>
    /// <param name="data">The image (PNG, JPEG, GIF, BMP, TIFF, JPEG XR, SVG, EMF or WMF).</param>
    /// <param name="altText">The alternative text.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation InsertPicture(ReadOnlyMemory<byte> data, string? altText = null) => Op(CommandId.InsertPicture, new Arg("data", Node(data)), new Arg("altText", altText is { } altText_ ? Node(altText_) : null, Optional: true));

    /// <summary>Inserts a text box.</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation InsertTextBox() => Op(CommandId.InsertTextBox);

    /// <summary>Inserts a floating shape at the caret's paragraph, in front of the text, with the theme's default style, and selects it.</summary>
    /// <param name="preset">The preset geometry.</param>
    /// <param name="width">The width; 72 pt when not given.</param>
    /// <param name="height">The height; 72 pt when not given, 0 for lines.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation InsertShape(ShapePreset preset, Length? width = null, Length? height = null) => Op(CommandId.InsertShape, Arg.Flatten(new ShapeInsert(preset, width, height)));

    /// <summary>Inserts a footnote at the selection.</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation InsertFootnote() => Op(CommandId.InsertFootnote);

    /// <summary>Inserts an endnote at the selection.</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation InsertEndnote() => Op(CommandId.InsertEndnote);

    /// <summary>Inserts a footnote or an endnote at the caret, numbered or with its own mark.</summary>
    /// <param name="kind">A footnote or an endnote.</param>
    /// <param name="customMark">Its own mark (1 to 10 characters) instead of a number.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation InsertNote(NoteType kind, string? customMark = null) => Op(CommandId.InsertNote, new Arg("kind", Node(kind)), new Arg("customMark", customMark is { } m ? Node(m) : null, Optional: true));

    /// <summary>Sets footnotes' or endnotes' position, number format, first number and restarting; unset values are kept.</summary>
    /// <param name="options">The settings.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetNoteProperties(NotePropertiesOptions options) => Op(CommandId.SetNoteProperties, Arg.Flatten(options));

    /// <summary>Turns every footnote into an endnote, every endnote into a footnote, or both.</summary>
    /// <param name="conversion">What turns into what.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation ConvertNotes(NoteConversion conversion) => Op(CommandId.ConvertNotes, new Arg("value", Node(conversion)));

    /// <summary>Inserts a hyperlink.</summary>
    /// <param name="address">The address (non-empty).</param>
    /// <param name="text">The text; by default the selection, or the address.</param>
    /// <param name="bookmark">The place in the target (a bookmark); it wins over a <c>#bookmark</c> in the address.</param>
    /// <param name="screenTip">The text shown when the link is hovered.</param>
    /// <param name="frame">The window or frame it opens in (<c>_blank</c>).</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation InsertHyperlink(string address, string? text = null, string? bookmark = null, string? screenTip = null, string? frame = null) => Op(CommandId.InsertHyperlink, new Arg("address", Node(address)), new Arg("text", text is { } text_ ? Node(text_) : null, Optional: true), new Arg("bookmark", bookmark is { } bookmark_ ? Node(bookmark_) : null, Optional: true), new Arg("screenTip", screenTip is { } screenTip_ ? Node(screenTip_) : null, Optional: true), new Arg("frame", frame is { } frame_ ? Node(frame_) : null, Optional: true));

    /// <summary>Changes the hyperlink at the selection.</summary>
    /// <param name="address">The new address.</param>
    /// <param name="text">The new text.</param>
    /// <param name="screenTip">The new ScreenTip; empty removes it.</param>
    /// <param name="frame">The new window or frame; empty removes it.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation EditHyperlink(string? address = null, string? text = null, string? screenTip = null, string? frame = null) => Op(CommandId.EditHyperlink, new Arg("address", address is { } address_ ? Node(address_) : null, Optional: true), new Arg("text", text is { } text_ ? Node(text_) : null, Optional: true), new Arg("screenTip", screenTip is { } screenTip_ ? Node(screenTip_) : null, Optional: true), new Arg("frame", frame is { } frame_ ? Node(frame_) : null, Optional: true));

    /// <summary>Removes the hyperlink at the selection, keeping its text.</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation RemoveHyperlink() => Op(CommandId.RemoveHyperlink);

    /// <summary>Bookmarks the selection. A bookmark of the same name (compared without case) moves here.</summary>
    /// <param name="name">A letter (or <c>_</c>, for a hidden bookmark) first, then letters, digits and <c>_</c>, 40 at most.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation InsertBookmark(string name) => Op(CommandId.InsertBookmark, new Arg("value", Node(name)));

    /// <summary>Deletes a bookmark, keeping its text.</summary>
    /// <param name="name">The bookmark, compared without case.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation DeleteBookmark(string name) => Op(CommandId.DeleteBookmark, new Arg("value", Node(name)));

    /// <summary>Restrict Editing: what the document allows, and starting or stopping its enforcement; stopping a password-protected document needs the password.</summary>
    /// <param name="protection">The settings to change.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetDocumentProtection(DocumentProtection protection) => Op(CommandId.SetDocumentProtection, Arg.Flatten(protection));

    /// <summary>Makes the selection a range everyone (or <paramref name="editor"/>) may edit while the document is protected.</summary>
    /// <param name="editor">A person who may edit it; everyone when this and <paramref name="group"/> are null.</param>
    /// <param name="group">A group who may edit it (<c>editors</c>, <c>owners</c>, ...), in place of <paramref name="editor"/>.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation AddEditableRange(string? editor = null, string? group = null) => Op(CommandId.AddEditableRange, new Arg("editor", editor is { } e ? Node(e) : null, Optional: true), new Arg("group", group is { } g ? Node(g) : null, Optional: true));

    /// <summary>Removes the editable ranges at the selection.</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation RemoveEditableRanges() => Op(CommandId.RemoveEditableRanges);

    /// <summary>Inserts a legacy form field (FORMTEXT, FORMCHECKBOX, FORMDROPDOWN) at the selection.</summary>
    /// <param name="kind">Its kind.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation InsertFormField(FormFieldType kind) => Op(CommandId.InsertFormField, new Arg("value", Node(kind)));

    /// <summary>Sets a legacy form field's options.</summary>
    /// <param name="change">The options to change.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetFormField(FormFieldChange change) => Op(CommandId.SetFormField, Arg.Flatten(change));

    /// <summary>Fills in a legacy form field: a text field's text, a check box's state, a drop-down's entry.</summary>
    /// <param name="fill">The value.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation FillFormField(FormFieldFill fill) => Op(CommandId.FillFormField, Arg.Flatten(fill));

    /// <summary>Shades legacy form fields, or not.</summary>
    /// <param name="on">Shaded.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetFormFieldShading(bool on) => Op(CommandId.SetFormFieldShading, new Arg("value", Node(on)));

    /// <summary>Sets the document's properties; unset values are kept.</summary>
    /// <param name="change">The properties to change.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetDocumentProperties(DocumentPropertiesChange change) => Op(CommandId.SetDocumentProperties, Arg.Flatten(change));

    /// <summary>Sets a document variable (read by DOCVARIABLE), or removes it.</summary>
    /// <param name="variable">The variable; a null value removes it.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetDocumentVariable(DocumentVariable variable) => Op(CommandId.SetDocumentVariable, Arg.Flatten(variable));

    /// <summary>Sets the file name FILENAME shows. Not saved, and not an edit.</summary>
    /// <param name="name">The file's name, with its folder when FILENAME <c>\p</c> should show one.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetFileName(string name) => Op(CommandId.SetFileName, new Arg("value", Node(name)));

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

    /// <summary>Converts the selected paragraphs to a table: a row for each paragraph, split into cells at the separator.</summary>
    /// <param name="separator">Where the text splits into cells.</param>
    /// <param name="character">The one character to split at, with <see cref="TextSeparator.Other"/>.</param>
    /// <param name="columns">The number of columns, 1 to 63 (by default the most pieces a paragraph has).</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    /// <param name="autofit">How the new table's columns are sized; fixed by default.</param>
    /// <param name="columnWidth">With fixed columns, every column this wide (1 to 1584); by default the text's width shared equally.</param>
    public static EditorOperation ConvertTextToTable(TextSeparator separator, string? character = null, int? columns = null, TableFit? autofit = null, Length? columnWidth = null) => Op(CommandId.ConvertTextToTable, new Arg("separator", Node(separator)), new Arg("character", character is { } c ? Node(c) : null, Optional: true), new Arg("columns", columns is { } n ? Node(n) : null, Optional: true), new Arg("autofit", autofit is { } a ? Node(a) : null, Optional: true), new Arg("columnWidth", columnWidth is { } w ? Node(w) : null, Optional: true));

    /// <summary>Converts the selected rows, or the whole table, to text: a paragraph for each row, its cells joined by the separator.</summary>
    /// <param name="separator">What separates the cells' text.</param>
    /// <param name="character">The one character between cells, with <see cref="TextSeparator.Other"/>.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    /// <param name="convertNested">Tables nested in the cells become text too; by default they stay tables.</param>
    public static EditorOperation ConvertTableToText(TextSeparator separator, string? character = null, bool? convertNested = null) => Op(CommandId.ConvertTableToText, new Arg("separator", Node(separator)), new Arg("character", character is { } c ? Node(c) : null, Optional: true), new Arg("convertNested", Nullable(convertNested), Optional: true));

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

    /// <summary>Sets the selected cells' preferred width, text direction, wrapping, fitting and end mark.</summary>
    /// <param name="options">The properties to change.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetCellProperties(CellPropertiesOptions options) => Op(CommandId.SetCellProperties, Arg.Flatten(options));

    /// <summary>Hides the selected rows' end marks (a row hides when its text is hidden too), and sets the table properties they override.</summary>
    /// <param name="hidden">The rows' end marks are hidden; kept when null.</param>
    /// <param name="exceptions">The table properties the rows override; kept when null.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetRowProperties(bool? hidden = null, RowExceptions? exceptions = null) => Op(CommandId.SetRowProperties, new Arg("hidden", Nullable(hidden), Optional: true), new Arg("exceptions", exceptions is null ? null : Node(exceptions), Optional: true));

    /// <summary>Sets the rows and columns in each of the table's bands, 1 to 32767; unavailable outside a table.</summary>
    /// <param name="sizes">What to set, leave to the table's style, or keep.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetTableBandSizes(TableBandSizes sizes) => Op(CommandId.SetTableBandSizes, Arg.Flatten(sizes));

    /// <summary>Sets the space between the table's cells, or none.</summary>
    /// <param name="spacing">0 to 1584 points; null for none.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetTableCellSpacing(Length? spacing) => Op(CommandId.SetTableCellSpacing, new Arg("value", Nullable(spacing)));

    /// <summary>Sets the table's alternative text.</summary>
    /// <param name="title">The title; kept when null, removed when empty.</param>
    /// <param name="description">The description; kept when null, removed when empty.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetTableAltText(string? title = null, string? description = null) => Op(CommandId.SetTableAltText, new Arg("title", title is { } title_ ? Node(title_) : null, Optional: true), new Arg("description", description is { } description_ ? Node(description_) : null, Optional: true));

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

    /// <summary>Sets the table style new tables take (Set as Default).</summary>
    /// <param name="styleId">A table style; null for Table Grid.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetDefaultTableStyle(StyleId? styleId) => Op(CommandId.SetDefaultTableStyle, new Arg("value", styleId is { } s ? Node(s) : null));

    /// <summary>Sets which parts of the table style apply. Unset values are kept.</summary>
    /// <param name="options">The options to change.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetTableStyleOptions(TableStyleOptionsUpdate options) => Op(CommandId.SetTableStyleOptions, Arg.Flatten(options));

    /// <summary>
    /// Positions the caret's table so text wraps around it, as dragging its move handle does: a table without a
    /// position gets one across from the margin and down from the paragraph, and each axis given is aligned, offset, or
    /// (with neither) kept where it is and measured from its new reference. Unset values are kept; null puts the table
    /// back in the text.
    /// </summary>
    /// <param name="position">The position, or null for a table in the text.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetTablePosition(TablePosition? position) => Op(CommandId.SetTablePosition, new Arg("value", position is null ? null : Node(position)));

    /// <summary>Resizes the selected picture or text box. Unset values are kept.</summary>
    /// <param name="width">The width, 1 to 1584 pt.</param>
    /// <param name="height">The height, 1 to 1584 pt.</param>
    /// <param name="lockAspect">Keep the aspect ratio.</param>
    /// <param name="relativeWidth">A floating object's width as a share of a box on the page; <see cref="RelativeSize.Absolute"/> makes it absolute.</param>
    /// <param name="relativeHeight">A floating object's height as a share of a box on the page; <see cref="RelativeSize.Absolute"/> makes it absolute.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetObjectSize(Length? width = null, Length? height = null, bool? lockAspect = null, RelativeSize? relativeWidth = null, RelativeSize? relativeHeight = null) => Op(CommandId.SetObjectSize, new Arg("width", width is { } width_ ? Node(width_) : null, Optional: true), new Arg("height", height is { } height_ ? Node(height_) : null, Optional: true), new Arg("lockAspect", lockAspect is { } lockAspect_ ? Node(lockAspect_) : null, Optional: true), new Arg("relativeWidth", relativeWidth is null ? null : Node(relativeWidth), Optional: true), new Arg("relativeHeight", relativeHeight is null ? null : Node(relativeHeight), Optional: true));

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

    /// <summary>
    /// Places the selected floating object. Each axis given is aligned, offset, or (with neither) kept where it is and
    /// measured from its new reference: <see cref="VerticalReference.Page"/> fixes it on the page,
    /// <see cref="VerticalReference.Paragraph"/> moves it with the text. Unset values are kept.
    /// </summary>
    /// <param name="horizontal">The horizontal place.</param>
    /// <param name="vertical">The vertical place.</param>
    /// <param name="allowOverlap">Whether other floating objects may overlap it.</param>
    /// <param name="layoutInCell">Whether, in a table cell, it is kept within the cell.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetObjectPosition(HorizontalPosition? horizontal = null, VerticalPosition? vertical = null, bool? allowOverlap = null, bool? layoutInCell = null) => Op(CommandId.SetObjectPosition, new Arg("horizontal", horizontal is { } horizontal_ ? Node(horizontal_) : null, Optional: true), new Arg("vertical", vertical is { } vertical_ ? Node(vertical_) : null, Optional: true), new Arg("allowOverlap", allowOverlap is { } allowOverlap_ ? Node(allowOverlap_) : null, Optional: true), new Arg("layoutInCell", layoutInCell is { } layoutInCell_ ? Node(layoutInCell_) : null, Optional: true));

    /// <summary>Sets the sides text wraps on around the selected floating object and its distance from the text, 0 to 1584 pt. Unset values are kept.</summary>
    /// <param name="side">The sides text wraps on.</param>
    /// <param name="top">The distance from the text above.</param>
    /// <param name="bottom">The distance from the text below.</param>
    /// <param name="left">The distance from the text on the left.</param>
    /// <param name="right">The distance from the text on the right.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetObjectWrapOptions(WrapSide? side = null, Length? top = null, Length? bottom = null, Length? left = null, Length? right = null) => Op(CommandId.SetObjectWrapOptions, new Arg("side", side is { } side_ ? Node(side_) : null, Optional: true), new Arg("top", top is { } top_ ? Node(top_) : null, Optional: true), new Arg("bottom", bottom is { } bottom_ ? Node(bottom_) : null, Optional: true), new Arg("left", left is { } left_ ? Node(left_) : null, Optional: true), new Arg("right", right is { } right_ ? Node(right_) : null, Optional: true));

    /// <summary>Sets the alternative text of the selected object.</summary>
    /// <param name="text">The text.</param>
    /// <param name="decorative">Marks the object as decorative.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetAltText(string text, bool? decorative = null) => Op(CommandId.SetAltText, new Arg("text", Node(text)), new Arg("decorative", decorative is { } decorative_ ? Node(decorative_) : null, Optional: true));

    /// <summary>Deletes the selected object.</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation DeleteObject() => Op(CommandId.DeleteObject);

    /// <summary>Changes the selected shape's preset geometry. Its text, fill, outline and effects stay; its adjust values reset.</summary>
    /// <param name="preset">The preset geometry.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetShapePreset(ShapePreset preset) => Op(CommandId.SetShapePreset, new Arg("value", Node(preset)));

    /// <summary>Sets one of the selected shape's adjust values.</summary>
    /// <param name="index">The adjust value's index, in the preset's order.</param>
    /// <param name="value">Its new value, in the preset's units (usually 100000ths).</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetShapeAdjust(int index, long value) => Op(CommandId.SetShapeAdjust, Arg.Flatten(new ShapeAdjust(index, value)));

    /// <summary>Sets the selected objects' fill; <see cref="FillValue.FromStyle"/> gives them the style reference's fill again.</summary>
    /// <param name="fill">The fill.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetShapeFill(FillValue fill) => Op(CommandId.SetShapeFill, new Arg("value", Node(fill)));

    /// <summary>Sets the selected objects' outline. Unset values are kept.</summary>
    /// <param name="options">The outline values to change.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetShapeLine(LineChange options) => Op(CommandId.SetShapeLine, Arg.Flatten(options));

    /// <summary>Sets the selected objects' shadows, glow, soft edge and blur: each effect is kept, removed or set.</summary>
    /// <param name="options">The effects to change.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetShapeEffects(EffectsChange options) => Op(CommandId.SetShapeEffects, Arg.Flatten(options));

    /// <summary>Sets the selected shape's text insets, anchor, autofit, wrapping and direction. Unset values are kept.</summary>
    /// <param name="options">The text body values to change.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetShapeText(ShapeTextChange options) => Op(CommandId.SetShapeText, Arg.Flatten(options));

    /// <summary>Sets the selected picture's transparency, greyscale, brightness and contrast. Unset values are kept.</summary>
    /// <param name="options">The adjustments to change.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetPictureAdjust(PictureAdjustChange options) => Op(CommandId.SetPictureAdjust, Arg.Flatten(options));

    /// <summary>Crops the selected picture, inline or floating: its box follows the part shown, at the scale it is drawn.</summary>
    /// <param name="crop">The fractions of its image cut from each edge.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetPictureCrop(PictureCrop crop) => Op(CommandId.SetPictureCrop, Arg.Flatten(crop));

    /// <summary>Replaces the selected picture's image, keeping its width, effects and place; its height follows the new image and the crop is cleared.</summary>
    /// <param name="data">The image: PNG, JPEG, GIF, BMP, TIFF, JPEG XR, SVG, EMF or WMF.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation ChangePicture(ReadOnlyMemory<byte> data) => Op(CommandId.ChangePicture, new Arg("value", Node(data)));

    /// <summary>Aligns or distributes the selected floating objects across their page, its margins, or the box around them.</summary>
    /// <param name="align">How they line up.</param>
    /// <param name="relativeTo">What they line up in; the box around them for two or more, else the margins, when null.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation AlignObjects(ObjectAlign align, AlignReference? relativeTo = null) => Op(CommandId.AlignObjects, new Arg("align", Node(align)), new Arg("relativeTo", Nullable(relativeTo), Optional: true));

    /// <summary>Rotates the selected objects.</summary>
    /// <param name="degrees">The rotation, 0 to 360 degrees clockwise.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetObjectRotation(double degrees) => Op(CommandId.SetObjectRotation, new Arg("value", Node(degrees)));

    /// <summary>Flips the selected objects.</summary>
    /// <param name="axis">Which way.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation FlipObject(FlipAxis axis) => Op(CommandId.FlipObject, new Arg("value", Node(axis)));

    /// <summary>Moves the selected floating object in the paint order, or behind or in front of the text.</summary>
    /// <param name="arrange">Where it goes.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation ArrangeObject(Arrange arrange) => Op(CommandId.ArrangeObject, new Arg("value", Node(arrange)));

    /// <summary>Groups the selected floating objects (two or more, in one story) and selects the group.</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation GroupObjects() => Op(CommandId.GroupObjects);

    /// <summary>Ungroups the selected group and selects its children.</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation UngroupObjects() => Op(CommandId.UngroupObjects);

    /// <summary>Selects the next or previous child of the selected group. Not an edit: no undo step.</summary>
    /// <param name="direction">Next or previous.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SelectGroupChild(NavigationDirection direction) => Op(CommandId.SelectGroupChild, new Arg("value", Node(direction)));

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
    /// <param name="ids">The fields; null for all of them, tables of contents included; empty for the table of contents (or other field across paragraphs) at the selection, which has no id.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation ShowFieldCodes(bool show, IReadOnlyList<ElementId>? ids = null) => Op(CommandId.ShowFieldCodes, new Arg("show", Node(show)), new Arg("ids", ids is { } ids_ ? Node(ids_) : null, Optional: true));

    /// <summary>Sets what design mode shows for each field code. A view setting.</summary>
    /// <param name="labels">Labels keyed by field code.</param>
    /// <param name="merge">Merge with the current labels instead of replacing them.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetFieldLabels(IReadOnlyDictionary<string, string> labels, bool? merge = null) => Op(CommandId.SetFieldLabels, new Arg("labels", Node(labels)), new Arg("merge", merge is { } merge_ ? Node(merge_) : null, Optional: true));

    /// <summary>Sets which fields are sub-template references, replacing the rules; none turns sub-templates off. A view setting.</summary>
    /// <param name="rules">The rules.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetSubTemplateRules(IReadOnlyList<ReferenceRule> rules) => Op(CommandId.SetSubTemplateRules, new Arg("rules", Node(rules)));

    /// <summary>Edits a sub-template's body in this editor, on its own pages. A view change.</summary>
    /// <param name="name">The sub-template.</param>
    /// <param name="category">Its building block category, when several rules name categories.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation EditSubTemplate(string name, string? category = null) => Op(CommandId.EditSubTemplate, new Arg("name", Node(name)), new Arg("category", category is { } category_ ? Node(category_) : null, Optional: true));

    /// <summary>Back to the template from a sub-template, at the template's selection from before. A view change.</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation CloseSubTemplate() => Op(CommandId.CloseSubTemplate);

    /// <summary>A new sub-template: empty, with the body of the DOCX <paramref name="from"/>, or a copy of the sub-template <paramref name="copyOf"/>. One undo step.</summary>
    /// <param name="name">Its name, unique in its category.</param>
    /// <param name="category">Its building block category; null for the first rule's.</param>
    /// <param name="from">A DOCX whose body becomes its body.</param>
    /// <param name="copyOf">A sub-template in the same category whose body (formatting, fields, controls, pictures, references) is copied; not with <paramref name="from"/>.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation CreateSubTemplate(string name, string? category = null, ReadOnlyMemory<byte>? from = null, string? copyOf = null) => Op(CommandId.CreateSubTemplate, new Arg("name", Node(name)), new Arg("category", category is { } category_ ? Node(category_) : null, Optional: true), new Arg("from", from is { } from_ ? Node(from_) : null, Optional: true), new Arg("copyOf", copyOf is { } copyOf_ ? Node(copyOf_) : null, Optional: true));

    /// <summary>The template's body becomes the DOCX <paramref name="from"/>'s body, on its last section's page; its sub-templates, custom XML data parts, settings, styles, headers and footers stay. Closes a sub-template being edited first. One undo step.</summary>
    /// <param name="from">The DOCX.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation ReplaceBody(ReadOnlyMemory<byte> from) => Op(CommandId.ReplaceBody, new Arg("from", Node(from)));

    /// <summary>Renames a sub-template, and optionally every reference to it. One undo step.</summary>
    /// <param name="name">Its name now.</param>
    /// <param name="to">Its new name, unique in its category.</param>
    /// <param name="updateReferences">Rename the references too.</param>
    /// <param name="category">Its building block category, when several rules name categories.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation RenameSubTemplate(string name, string to, bool? updateReferences = null, string? category = null) => Op(CommandId.RenameSubTemplate, new Arg("name", Node(name)), new Arg("to", Node(to)), new Arg("category", category is { } category_ ? Node(category_) : null, Optional: true), new Arg("updateReferences", updateReferences is { } u_ ? Node(u_) : null, Optional: true));

    /// <summary>Removes a sub-template; its references no longer resolve. One undo step.</summary>
    /// <param name="name">The sub-template.</param>
    /// <param name="category">Its building block category, when several rules name categories.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation RemoveSubTemplate(string name, string? category = null) => Op(CommandId.RemoveSubTemplate, new Arg("name", Node(name)), new Arg("category", category is { } category_ ? Node(category_) : null, Optional: true));

    /// <summary>Shows references' sub-template bodies in their place, or their fields. A view setting.</summary>
    /// <param name="expanded">Show the bodies (else the fields).</param>
    /// <param name="ids">The reference fields; null for every reference.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetSubTemplateView(bool expanded, IReadOnlyList<ElementId>? ids = null) => Op(CommandId.SetSubTemplateView, new Arg("ids", ids is { } ids_ ? Node(ids_) : null, Optional: true), new Arg("expanded", Node(expanded)));

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

    /// <summary>Rewrites a font family's name in the document itself: see <see cref="EditorCommands.ReplaceFontAsync"/>.</summary>
    /// <param name="from">The family to rewrite, case-insensitive.</param>
    /// <param name="to">The family it becomes.</param>
    /// <param name="scope">The whole document, or the selection.</param>
    /// <param name="bold">Only bold (true) or regular-weight (false) text.</param>
    /// <param name="italic">Only italic (true) or upright (false) text.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation ReplaceFont(string from, string to, ReplaceScope scope = ReplaceScope.Document, bool? bold = null, bool? italic = null) => Op(CommandId.ReplaceFont, new Arg("from", Node(from)), new Arg("to", Node(to)), new Arg("scope", Node(scope)), new Arg("bold", bold is { } bold_ ? Node(bold_) : null, Optional: true), new Arg("italic", italic is { } italic_ ? Node(italic_) : null, Optional: true));

    /// <summary>Marks elements with wavy lines and a tooltip. A view setting.</summary>
    /// <param name="marks">The marks.</param>
    /// <param name="clear">Clear the existing marks first.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation MarkElements(IReadOnlyList<ElementMark> marks, bool? clear = null) => Op(CommandId.MarkElements, new Arg("marks", Node(marks)), new Arg("clear", clear is { } clear_ ? Node(clear_) : null, Optional: true));

    /// <summary>Sets the tag prefixes (compared without case) whose picture controls keep their pictures, replacing those set before; an empty list protects none.</summary>
    /// <param name="tagPrefixes">The tag prefixes.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetProtectedPictureControls(IReadOnlyList<string> tagPrefixes) => Op(CommandId.SetProtectedPictureControls, new Arg("tagPrefixes", Node(tagPrefixes)));

    /// <summary>Selects a picture control (the selected one when none is given) and raises <see cref="EditorSession.PictureControlActivated"/>, in any mode.</summary>
    /// <param name="element">The picture control; null for the selected one.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation ActivatePictureControl(ElementId? element = null) => Op(CommandId.ActivatePictureControl, new Arg("element", element is { } element_ ? Node(element_) : null, Optional: true));

    /// <summary>Writes a custom XML data part.</summary>
    /// <param name="namespace">The part's namespace.</param>
    /// <param name="xml">The XML (well-formed).</param>
    /// <param name="itemId">The part's item id, when several share the namespace: a GUID, matched without case or braces.</param>
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

    /// <summary>Accepts every revision, one author's, or those Show Markup shows.</summary>
    /// <param name="author">Only this author's revisions.</param>
    /// <param name="shown">Only the revisions Show Markup shows; not with <paramref name="author"/>.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation AcceptAllRevisions(string? author = null, bool? shown = null) => Op(CommandId.AcceptAllRevisions, new Arg("author", author is { } author_ ? Node(author_) : null, Optional: true), new Arg("shown", shown is { } shown_ ? Node(shown_) : null, Optional: true));

    /// <summary>Rejects every revision, one author's, or those Show Markup shows.</summary>
    /// <param name="author">Only this author's revisions.</param>
    /// <param name="shown">Only the revisions Show Markup shows; not with <paramref name="author"/>.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation RejectAllRevisions(string? author = null, bool? shown = null) => Op(CommandId.RejectAllRevisions, new Arg("author", author is { } author_ ? Node(author_) : null, Optional: true), new Arg("shown", shown is { } shown_ ? Node(shown_) : null, Optional: true));

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
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetViewOptions(bool? formattingMarks = null, bool? gridlines = null, bool? textBoundaries = null, bool? liveLayout = null, ChromeTheme? theme = null, ColorValue? accent = null, DocumentAppearance? appearance = null, MeasureDetail? detail = null, ObjectGrid? objectGrid = null, ThemeColors? themeColors = null, FieldShading? fieldShading = null, IReadOnlyList<CursorOverride>? cursorOverrides = null, bool? controlLabels = null, bool? bookmarks = null, bool? tabCharacters = null, bool? spaces = null, bool? paragraphMarks = null, bool? hiddenText = null, bool? optionalHyphens = null, bool? objectAnchors = null, bool? clickAndType = null, bool? textDragAndDrop = null, bool? hideWhiteSpace = null, bool? showEditableRegions = null) => Op(CommandId.SetViewOptions, new Arg("formattingMarks", Nullable(formattingMarks), Optional: true), new Arg("gridlines", Nullable(gridlines), Optional: true), new Arg("textBoundaries", Nullable(textBoundaries), Optional: true), new Arg("liveLayout", Nullable(liveLayout), Optional: true), new Arg("theme", theme is null ? null : Node(theme), Optional: true), new Arg("accent", Nullable(accent), Optional: true), new Arg("appearance", Nullable(appearance), Optional: true), new Arg("detail", Nullable(detail), Optional: true), new Arg("objectGrid", Nullable(objectGrid), Optional: true), new Arg("themeColors", themeColors is null ? null : Node(themeColors), Optional: true), new Arg("fieldShading", Nullable(fieldShading), Optional: true), new Arg("cursorOverrides", cursorOverrides is null ? null : Node(cursorOverrides), Optional: true), new Arg("controlLabels", Nullable(controlLabels), Optional: true), new Arg("bookmarks", Nullable(bookmarks), Optional: true), new Arg("tabCharacters", Nullable(tabCharacters), Optional: true), new Arg("spaces", Nullable(spaces), Optional: true), new Arg("paragraphMarks", Nullable(paragraphMarks), Optional: true), new Arg("hiddenText", Nullable(hiddenText), Optional: true), new Arg("optionalHyphens", Nullable(optionalHyphens), Optional: true), new Arg("objectAnchors", Nullable(objectAnchors), Optional: true), new Arg("clickAndType", Nullable(clickAndType), Optional: true), new Arg("textDragAndDrop", Nullable(textDragAndDrop), Optional: true), new Arg("hideWhiteSpace", Nullable(hideWhiteSpace), Optional: true), new Arg("showEditableRegions", Nullable(showEditableRegions), Optional: true));

    /// <summary>Switches between editing and design mode.</summary>
    /// <param name="mode">The mode.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetMode(EditorMode mode) => Op(CommandId.SetMode, new Arg("value", Node(mode)));

    /// <summary>Sets what a click on the ruler adds; answers the tab type picker. A view setting.</summary>
    /// <param name="value">The tab type or indent marker.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetRulerTabType(TabSelector value) => Op(CommandId.SetRulerTabType, new Arg("value", Node(value)));

    /// <summary>Asks for the tab type picker (<see cref="EditorSession.PopoverRequested"/>), as a click on the ruler's tab type box does.</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation ShowTabTypes() => Op(CommandId.ShowTabTypes);

    /// <summary>Asks for the options of the tab stop nearest the caret (<see cref="EditorSession.PopoverRequested"/>).</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation ShowTabStop() => Op(CommandId.ShowTabStop);

    /// <summary>Switches AutoCorrect rules on or off, and replaces its lists. A session setting, not an undo step.</summary>
    /// <param name="change">The rules and lists to change.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetAutoCorrect(AutoCorrectChange change) => Op(CommandId.SetAutoCorrect, Arg.Flatten(change));

    /// <summary>Puts back what was typed before the correction at the caret. Undoes the correction when it is the last step, otherwise one step of its own.</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation RevertAutoCorrect() => Op(CommandId.RevertAutoCorrect);

    /// <summary>Reverts the correction at the caret, and never makes it again.</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation NeverAutoCorrect() => Op(CommandId.NeverAutoCorrect);

    /// <summary>Asks for the options of the correction at the caret (<see cref="EditorSession.PopoverRequested"/>, Alt+Shift+F10).</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation ShowAutoCorrectOptions() => Op(CommandId.ShowAutoCorrectOptions);

    /// <summary>Dismisses the popover asked for (<see cref="EditorSession.PopoverClosed"/>), for an application that closes its own on Esc or a click elsewhere; with none open, nothing.</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation ClosePopover() => Op(CommandId.ClosePopover);
    /// <summary>Says the formatting at the caret to screen readers, as an announcement.</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation DescribeFormatting() => Op(CommandId.DescribeFormatting);

    /// <summary>The application's own field types (a code's first word, compared without case), replacing those set before: their fields keep their results, show their labels in design mode, are listed by update requests and are never read as bookmark references; an empty list clears them. A view setting.</summary>
    /// <param name="types">The field types.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetApplicationFieldTypes(IReadOnlyList<string> types) => Op(CommandId.SetApplicationFieldTypes, new Arg("types", Node(types)));

    /// <summary>Inserts a style separator (Ctrl+Alt+Enter): a hidden paragraph mark that runs the paragraph before it into the one after, each keeping its own style. One undo step.</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation InsertStyleSeparator() => Op(CommandId.InsertStyleSeparator);

    /// <summary>Alt+X: the hexadecimal code before the caret becomes its character, or the character before the caret (or the selected one) becomes its code. One undo step.</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation ToggleCharacterCode() => Op(CommandId.ToggleCharacterCode);

    /// <summary>F2: the selected text moves to the place the next click or arrow keys choose, when Enter is pressed (<see cref="SelectionSummary.PendingMove"/> while it waits); Esc or any other command gives up.</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation MoveText() => Op(CommandId.MoveText);

    /// <summary>Shift+F2: as MoveText, copying the text.</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation CopyText() => Op(CommandId.CopyText);

    /// <summary>Selects the whole next or previous field in the story, outermost fields only (F11, Shift+F11). Not an edit: no undo step.</summary>
    /// <param name="direction">Next or previous.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SelectField(NavigationDirection direction) => Op(CommandId.SelectField, new Arg("value", Node(direction)));

    /// <summary>Each selected run's size a point larger, up to 1638 (Ctrl+]).</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation GrowFontOnePoint() => Op(CommandId.GrowFontOnePoint);

    /// <summary>Each selected run's size a point smaller, down to 1 (Ctrl+[).</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation ShrinkFontOnePoint() => Op(CommandId.ShrinkFontOnePoint);

    /// <summary>Clear All Formatting: the selected paragraphs take the default paragraph style and lose their direct paragraph and character formatting; highlight and revision marks stay.</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation ClearAllFormatting() => Op(CommandId.ClearAllFormatting);

    /// <summary>A hanging indent to the next default tab stop: the left indent moves, the first line stays (Ctrl+T).</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation IncreaseHangingIndent() => Op(CommandId.IncreaseHangingIndent);

    /// <summary>The hanging indent back to the previous default tab stop (Ctrl+Shift+T).</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation DecreaseHangingIndent() => Op(CommandId.DecreaseHangingIndent);

    /// <summary>Reset Paragraph: removes direct paragraph formatting, numbering included; the style stays (Ctrl+Q).</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation ClearParagraphFormatting() => Op(CommandId.ClearParagraphFormatting);

    /// <summary>Moves the selected paragraphs above the one before or below the one after (Alt+Shift+Up, Down); tracked, a move.</summary>
    /// <param name="direction">Up or down.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation MoveParagraphs(VerticalDirection direction) => Op(CommandId.MoveParagraphs, new Arg("value", Node(direction)));

    /// <summary>Groups again the objects the last UngroupObjects ungrouped, wherever they moved.</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation RegroupObjects() => Op(CommandId.RegroupObjects);

    /// <summary>Shows the selected floating object's wrap polygon for editing (<see cref="ViewState.EditingWrapPoints"/>): dragging a vertex moves it, dragging an edge adds one, Ctrl+click removes one, each one undo step that makes the wrap tight. Esc ends it. Not an edit: no undo step.</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation EditWrapPoints() => Op(CommandId.EditWrapPoints);

    /// <summary>Selects ranges of text and blocks of cells of one story, a discontiguous selection when there are several (<see cref="SelectionSummary.Ranges"/> and <see cref="SelectionSummary.CellBlocks"/> read it back): overlapping and touching ranges join, empty ones go. The last range, or the <paramref name="activeCells"/> block, is the selection the next command or gesture goes on from; when every range is empty, the caret goes to the last one's end. At most 100,000 ranges and blocks. Not an edit: no undo step.</summary>
    /// <param name="ranges">The ranges, in any order and either way round; the last is the active one.</param>
    /// <param name="cells">Blocks of cells selected beside the ranges.</param>
    /// <param name="activeCells">The index in <paramref name="cells"/> of the block that is the active selection, in place of the last range.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetSelection(IReadOnlyList<TextRange> ranges, IReadOnlyList<CellBlock>? cells = null, int? activeCells = null) =>
        Op(CommandId.SetSelection, new Arg("ranges", Node(ranges)), new Arg("cells", cells is { Count: > 0 } ? Node(cells) : null, Optional: true), new Arg("activeCells", Nullable(activeCells), Optional: true));

    /// <summary>Shrink Selection (Shift+F8), in extend mode: the selection one level smaller around where extending started (the story to the paragraph, the sentence, the word, then the caret), still extending. Not an edit: no undo step.</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation ShrinkSelection() => Op(CommandId.ShrinkSelection);

    /// <summary>Select Text with Similar Formatting: every stretch of text in the story with the same character formatting as the selection's first character (or the caret's), as a discontiguous selection. Not an edit: no undo step.</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SelectSimilarFormatting() => Op(CommandId.SelectSimilarFormatting);

    /// <summary>Selects every use of a paragraph or character style in the main document: its paragraphs (marks included, consecutive ones as one range) or its stretches of runs (<see cref="StyleInfo.Uses"/> counts them). Not an edit: no undo step.</summary>
    /// <param name="styleId">The style.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SelectStyleInstances(StyleId styleId) => Op(CommandId.SelectStyleInstances, new Arg("value", Node(styleId)));
    /// <summary>Sets the table's default cell margins (Table Options), 0 to 1584 each; unset sides are kept.</summary>
    /// <param name="margins">The sides to set.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetTableCellMargins(TableCellMargins margins) => Op(CommandId.SetTableCellMargins, Arg.Flatten(margins));

    /// <summary>Inserts blank cells the size of the selected ones, shifting the selected cells right or down (Insert Cells). Tracked when tracking changes.</summary>
    /// <param name="shift">Where the selected cells go.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation InsertCells(InsertCellShift shift) => Op(CommandId.InsertCells, new Arg("value", Node(shift)));

    /// <summary>Sets the pointer's table tool: while the pen or eraser is on, a press and drag on a page draws or erases instead of selecting.</summary>
    /// <param name="tool">The tool.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetTableTool(TableTool tool) => Op(CommandId.SetTableTool, new Arg("value", Node(tool)));

    /// <summary>Sets the line the Draw Table pen draws (0.25 to 6 pt). Not saved.</summary>
    /// <param name="line">The line.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetTablePen(CellBorder line) => Op(CommandId.SetTablePen, new Arg("value", Node(line)));

    /// <summary>A Draw Table pen stroke on a page, in points from its top left: splits the cells it crosses, draws a cell's diagonal, or off any table draws a new one. Untracked.</summary>
    /// <param name="page">The page, from 0.</param>
    /// <param name="from">Where the stroke starts.</param>
    /// <param name="to">Where it ends.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation DrawTableStroke(int page, PagePoint from, PagePoint to) => Op(CommandId.DrawTableStroke, new Arg("page", Node(page)), new Arg("from", Node(from)), new Arg("to", Node(to)));

    /// <summary>The table eraser on a page: merges the two cells either side of the border at a point.</summary>
    /// <param name="page">The page, from 0.</param>
    /// <param name="at">The point, in points from the page's top left.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation EraseTableBorder(int page, PagePoint at) => Op(CommandId.EraseTableBorder, new Arg("page", Node(page)), new Arg("at", Node(at)));

    /// <summary>Names the selected object (1 to 255 characters after trimming), as the Selection Pane lists it.</summary>
    /// <param name="name">The name.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetObjectName(string name) => Op(CommandId.SetObjectName, new Arg("value", Node(name)));

    /// <summary>Sets the selected object's alternative text title (at most 255 characters; empty clears it).</summary>
    /// <param name="title">The title.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetObjectTitle(string title) => Op(CommandId.SetObjectTitle, new Arg("value", Node(title)));

    /// <summary>Hides or shows a floating object: hidden, it isn't drawn and text doesn't wrap around it.</summary>
    /// <param name="object">The floating object.</param>
    /// <param name="hidden">Hidden, or shown.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetObjectHidden(ObjectId @object, bool hidden) => Op(CommandId.SetObjectHidden, new Arg("object", Node(@object)), new Arg("hidden", Node(hidden)));

    /// <summary>Selects a floating object or a group's object, as the Selection Pane does.</summary>
    /// <param name="object">The object.</param>
    /// <param name="extend">Adds it to, or takes it from, the selected floating objects.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SelectObject(ObjectId @object, bool? extend = null) => Op(CommandId.SelectObject, new Arg("object", Node(@object)), new Arg("extend", Nullable(extend), Optional: true));

    /// <summary>Moves a floating object in its page's stack: just in front of another, or behind every other. It keeps its layer.</summary>
    /// <param name="object">The floating object.</param>
    /// <param name="inFrontOf">The floating object it goes just in front of; null: behind every other.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation RestackObject(ObjectId @object, ObjectId? inFrontOf = null) => Op(CommandId.RestackObject, new Arg("object", Node(@object)), new Arg("inFrontOf", inFrontOf is null ? null : Node(inFrontOf), Optional: true));

    /// <summary>Whether resizing the selected object keeps its aspect ratio (Lock aspect ratio), saved with it.</summary>
    /// <param name="on">Locked.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetLockAspectRatio(bool on) => Op(CommandId.SetLockAspectRatio, new Arg("value", Node(on)));

    /// <summary>Sets the selected pictures' recolouring (Picture Color, Recolor).</summary>
    /// <param name="recolor">The recolouring.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetPictureRecolor(Recolor recolor) => Op(CommandId.SetPictureRecolor, new Arg("value", Node(recolor)));

    /// <summary>Reset Picture: clears the selected picture's adjustments, colour, effects, outline, shape, rotation and flips, keeping its crop; with <paramref name="size"/>, also its crop and size.</summary>
    /// <param name="size">Reset Picture and Size.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation ResetPicture(bool? size = null) => Op(CommandId.ResetPicture, new Arg("size", Nullable(size), Optional: true));

    /// <summary>Sets the selected floating object's wrap polygon (3 to 1024 points), marked edited; a wrap that is neither tight nor through becomes tight.</summary>
    /// <param name="points">The vertices, unclosed.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetWrapPoints(IReadOnlyList<BoxPoint> points) => Op(CommandId.SetWrapPoints, new Arg("value", Node(points)));

    /// <summary>Makes the selected floating object's wrap polygon again from its outline, no longer edited.</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation ResetWrapPoints() => Op(CommandId.ResetWrapPoints);

    /// <summary>Links the selected text box (the last in its chain) to an empty text box in no chain, in the same story (Create Link).</summary>
    /// <param name="target">The text box its text flows on into.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation LinkTextBox(ObjectId target) => Op(CommandId.LinkTextBox, new Arg("value", Node(target)));

    /// <summary>Starts linking the selected text box with the pointer: the next click on an empty text box links it; elsewhere or Escape cancels.</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation BeginTextBoxLink() => Op(CommandId.BeginTextBoxLink);

    /// <summary>Breaks the link from the selected text box to the next (Break Link).</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation UnlinkTextBox() => Op(CommandId.UnlinkTextBox);

    /// <summary>Turns Select Objects on or off: a drag off every object selects the objects wholly inside its rectangle.</summary>
    /// <param name="on">On.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetSelectObjects(bool on) => Op(CommandId.SetSelectObjects, new Arg("value", Node(on)));

    /// <summary>Promote (Alt+Shift+Left): each selected paragraph a level up; body text takes the level of the heading before it (Heading 1 when none).</summary>
    /// <param name="subtopics">The headings under each heading changed move by as many levels; null for no.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation PromoteOutline(bool? subtopics = null) => Op(CommandId.PromoteOutline, new Arg("subtopics", subtopics is { } subtopics_ ? Node(subtopics_) : null, Optional: true));

    /// <summary>Demote (Alt+Shift+Right): each selected heading a level down; Heading 9 and body text stay.</summary>
    /// <param name="subtopics">The headings under each heading changed move by as many levels; null for no.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation DemoteOutline(bool? subtopics = null) => Op(CommandId.DemoteOutline, new Arg("subtopics", subtopics is { } subtopics_ ? Node(subtopics_) : null, Optional: true));

    /// <summary>Promote to Heading 1: the selected paragraphs take Heading 1, their direct outline level removed.</summary>
    /// <param name="subtopics">The headings under each heading changed move by as many levels; null for no.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation PromoteToHeading1(bool? subtopics = null) => Op(CommandId.PromoteToHeading1, new Arg("subtopics", subtopics is { } subtopics_ ? Node(subtopics_) : null, Optional: true));

    /// <summary>Demote to Body Text: the selected paragraphs take the default paragraph style, their direct outline level removed.</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation DemoteToBodyText() => Op(CommandId.DemoteToBodyText);

    /// <summary>A navigation pane's Promote on one heading.</summary>
    /// <param name="heading">The heading.</param>
    /// <param name="subtopics">Its subheadings move with it; null for yes.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation PromoteHeading(HeadingId heading, bool? subtopics = null) => Op(CommandId.PromoteHeading, new Arg("heading", Node(heading)), new Arg("subtopics", subtopics is { } subtopics_ ? Node(subtopics_) : null, Optional: true));

    /// <summary>A navigation pane's Demote on one heading.</summary>
    /// <param name="heading">The heading.</param>
    /// <param name="subtopics">Its subheadings move with it; null for yes.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation DemoteHeading(HeadingId heading, bool? subtopics = null) => Op(CommandId.DemoteHeading, new Arg("heading", Node(heading)), new Arg("subtopics", subtopics is { } subtopics_ ? Node(subtopics_) : null, Optional: true));

    /// <summary>Moves a heading with its content before another heading, or after that heading's content, optionally at a new level; the caret goes to its start.</summary>
    /// <param name="heading">The heading moved.</param>
    /// <param name="target">The heading it goes beside.</param>
    /// <param name="side">Before the target, or after its content.</param>
    /// <param name="level">Its new level, 1 to 9; null keeps it.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation MoveHeading(HeadingId heading, HeadingId target, HeadingSide side, int? level = null) => Op(CommandId.MoveHeading, new Arg("heading", Node(heading)), new Arg("target", Node(target)), new Arg("side", Node(side)), new Arg("level", level is { } level_ ? Node(level_) : null, Optional: true));

    /// <summary>Deletes a heading and its content.</summary>
    /// <param name="heading">The heading.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation DeleteHeading(HeadingId heading) => Op(CommandId.DeleteHeading, new Arg("value", Node(heading)));

    /// <summary>An empty heading at a heading's level just before it, the caret in it.</summary>
    /// <param name="heading">The heading.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation InsertHeadingBefore(HeadingId heading) => Op(CommandId.InsertHeadingBefore, new Arg("value", Node(heading)));

    /// <summary>An empty heading at a heading's level after its content, the caret in it.</summary>
    /// <param name="heading">The heading.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation InsertHeadingAfter(HeadingId heading) => Op(CommandId.InsertHeadingAfter, new Arg("value", Node(heading)));

    /// <summary>An empty heading a level below a heading, after its content, the caret in it.</summary>
    /// <param name="heading">The heading.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation InsertSubheading(HeadingId heading) => Op(CommandId.InsertSubheading, new Arg("value", Node(heading)));

    /// <summary>Selects a heading and its content. Not an edit: no undo step.</summary>
    /// <param name="heading">The heading.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SelectHeadingContent(HeadingId heading) => Op(CommandId.SelectHeadingContent, new Arg("value", Node(heading)));

    /// <summary>Sets the separator, continuation separator or continuation notice of footnotes or endnotes, or back to the built-in one.</summary>
    /// <param name="kind">Footnotes' or endnotes'.</param>
    /// <param name="part">Which separator.</param>
    /// <param name="text">Its text, paragraphs <c>\n</c> apart (empty: an empty line); null for the built-in one (a line; no notice).</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetNoteSeparator(NoteType kind, NoteSeparatorPart part, string? text = null) => Op(CommandId.SetNoteSeparator, new Arg("kind", Node(kind)), new Arg("part", Node(part)), new Arg("text", text is { } text_ ? Node(text_) : null, Optional: true));

    /// <summary>Lets an editor edit the selection, or stops them editing the editable ranges reaching it.</summary>
    /// <param name="editor">The group or person.</param>
    /// <param name="allowed">They may edit the selection; false: no longer the ranges reaching it.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetEditableRangeEditor(RangeEditorRef editor, bool allowed) => Op(CommandId.SetEditableRangeEditor, new Arg("editor", Node(editor)), new Arg("allowed", Node(allowed)));

    /// <summary>Selects the next or previous editable range the author may edit (Find Next Region I Can Edit). Not an edit: no undo step.</summary>
    /// <param name="direction">The direction.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SelectNextEditableRange(NavigationDirection direction) => Op(CommandId.SelectNextEditableRange, new Arg("value", Node(direction)));

    /// <summary>Selects every editable range the author may edit (Show All Regions I Can Edit). Not an edit: no undo step.</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SelectEditableRanges() => Op(CommandId.SelectEditableRanges);

    /// <summary>The groups the author is in; editable ranges for them let the author edit. Kept for the documents opened after.</summary>
    /// <param name="groups">The groups (everyone is always one).</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetEditingGroups(IReadOnlyList<string> groups) => Op(CommandId.SetEditingGroups, new Arg("value", Node(groups)));

    /// <summary>Selects the next or previous legacy form field (Tab and Shift+Tab where forms are filled in). Not an edit: no undo step.</summary>
    /// <param name="direction">The direction.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SelectNextFormField(NavigationDirection direction) => Op(CommandId.SelectNextFormField, new Arg("value", Node(direction)));

    /// <summary>Adds an item to a repeating section: a copy of one of its items, with its content; the caret goes to the copy's start.</summary>
    /// <param name="id">The repeating section.</param>
    /// <param name="item">The item copied; null for the last.</param>
    /// <param name="before">The copy goes before it; null for after.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation AddRepeatingItem(ElementId id, ElementId? item = null, bool? before = null) => Op(CommandId.AddRepeatingItem, new Arg("id", Node(id)), new Arg("item", item is { } item_ ? Node(item_) : null, Optional: true), new Arg("before", before is { } before_ ? Node(before_) : null, Optional: true));

    /// <summary>Which kinds of markup show (Show Markup); unset values are kept. Not an edit: no undo step.</summary>
    /// <param name="insertionsDeletions">Insertions and deletions.</param>
    /// <param name="formatting">Formatting changes.</param>
    /// <param name="comments">Comments.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetShowMarkup(bool? insertionsDeletions = null, bool? formatting = null, bool? comments = null) => Op(CommandId.SetShowMarkup, new Arg("insertionsDeletions", insertionsDeletions is { } insertionsDeletions_ ? Node(insertionsDeletions_) : null, Optional: true), new Arg("formatting", formatting is { } formatting_ ? Node(formatting_) : null, Optional: true), new Arg("comments", comments is { } comments_ ? Node(comments_) : null, Optional: true));

    /// <summary>The reviewers whose revisions and comments don't show, by name; empty: all reviewers show. Not an edit: no undo step.</summary>
    /// <param name="hidden">The reviewers hidden.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetHiddenReviewers(IReadOnlyList<string> hidden) => Op(CommandId.SetHiddenReviewers, new Arg("hidden", Node(hidden)));

    /// <summary>Where revisions show in All Markup. Not an edit: no undo step.</summary>
    /// <param name="mode">The mode.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetBalloonMode(BalloonMode mode) => Op(CommandId.SetBalloonMode, new Arg("value", Node(mode)));

    /// <summary>How revisions draw (Track Changes Options); unset values are kept. Not an edit: no undo step.</summary>
    /// <param name="change">The settings to change.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetMarkupStyle(MarkupStyleChange change) => Op(CommandId.SetMarkupStyle, Arg.Flatten(change));

    /// <summary>Whether tracking records moves as moves and formatting changes; unset values are kept. Saved.</summary>
    /// <param name="moves">Moves are tracked as moves.</param>
    /// <param name="formatting">Formatting changes are tracked.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetTrackOptions(bool? moves = null, bool? formatting = null) => Op(CommandId.SetTrackOptions, new Arg("moves", moves is { } moves_ ? Node(moves_) : null, Optional: true), new Arg("formatting", formatting is { } formatting_ ? Node(formatting_) : null, Optional: true));

    /// <summary>Accepts every revision and turns tracking off (Accept All Changes and Stop Tracking).</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation AcceptAllAndStopTracking() => Op(CommandId.AcceptAllAndStopTracking);

    /// <summary>Deletes every comment, resolved ones too, or the threads Show Markup shows.</summary>
    /// <param name="shownOnly">Only the threads Show Markup shows; null for every one.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation DeleteAllComments(bool? shownOnly = null) => Op(CommandId.DeleteAllComments, new Arg("shownOnly", shownOnly is { } shownOnly_ ? Node(shownOnly_) : null, Optional: true));

    /// <summary>Pastes the paste just made again another way, in place of it (the Paste Options button's choices): refused when there is no paste to change or it can't be pasted that way. One undo step takes the document back to before the first paste.</summary>
    /// <param name="mode">How it pastes again.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation Repaste(PasteMode mode) => Op(CommandId.Repaste, new Arg("value", Node(mode)));

    /// <summary>Hides the Paste Options button of the paste just made. Not an edit: no undo step.</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation DismissPasteOptions() => Op(CommandId.DismissPasteOptions);

    /// <summary>Changes the cut, copy and paste options (<see cref="EditorState.PasteOptions"/>); null values are kept. A session setting, never saved; works when read only.</summary>
    /// <param name="change">The options that change.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetPasteOptions(PasteOptionsChange change) => Op(CommandId.SetPasteOptions, Arg.Flatten(change));

    /// <summary>Inserts a DOCX's content in place of the selection (Insert Text from File): its paragraphs, tables, styles and sections.</summary>
    /// <param name="from">The DOCX's bytes.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation InsertFile(ReadOnlyMemory<byte> from) => Op(CommandId.InsertFile, new Arg("from", Node(from)));

    /// <summary>Go To: the n-th, n on or back, the next or the previous page, section, line, bookmark, comment, note, field, table, graphic, equation, embedded object or heading. Fails as unavailable when there is none to go to. Not an edit: no undo step.</summary>
    /// <param name="what">What it goes to.</param>
    /// <param name="target">Which one: numbers past the last go to the last.</param>
    /// <param name="name">A bookmark's name (with an n-th target, that bookmark), a comment's reviewer, a field's kind, an embedded object's program id.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation GoTo(GoToWhat what, GoToTarget target, string? name = null) => Op(CommandId.GoTo, new Arg("what", Node(what)), new Arg("target", Node(target)), new Arg("name", name is { } name_ ? Node(name_) : null, Optional: true));

    /// <summary>A table of contents in place of the selection (in place of the table the selection is in); with no options, Word's. Body only.</summary>
    /// <param name="options">What it collects; null for Word's default table.</param>
    /// <param name="layout">How its entries are laid out.</param>
    /// <param name="frame">Its title and building block control.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation InsertTableOfContents(TableOfContentsOptions? options = null, TocLayout? layout = null, TocFrame? frame = null) => Op(CommandId.InsertTableOfContents, new Arg("options", options is { } o ? Node(o) : null, Optional: true), new Arg("layout", layout is { } l ? Node(l) : null, Optional: true), new Arg("frame", frame is { } f ? Node(f) : null, Optional: true));

    /// <summary>A table of figures of the captions labelled <paramref name="label"/>, in place of the selection (or of the table it is in).</summary>
    /// <param name="label">The captions' label.</param>
    /// <param name="includeLabelAndNumber">Entries keep the label and number; null for on.</param>
    /// <param name="options">Its other options; null for hyperlinks and hidden in a web view.</param>
    /// <param name="layout">How its entries are laid out.</param>
    /// <param name="frame">Its title and building block control.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation InsertTableOfFigures(string label, bool? includeLabelAndNumber = null, TableOfContentsOptions? options = null, TocLayout? layout = null, TocFrame? frame = null) => Op(CommandId.InsertTableOfFigures, new Arg("label", Node(label)), new Arg("includeLabelAndNumber", includeLabelAndNumber is { } i ? Node(i) : null, Optional: true), new Arg("options", options is { } o ? Node(o) : null, Optional: true), new Arg("layout", layout is { } l ? Node(l) : null, Optional: true), new Arg("frame", frame is { } f ? Node(f) : null, Optional: true));

    /// <summary>Word's Manual Table: typed entries in a building block control, at the start of a body paragraph.</summary>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation InsertManualTableOfContents() => Op(CommandId.InsertManualTableOfContents);

    /// <summary>Updates the table of contents or figures the selection is in (else the first; <paramref name="index"/>: that one of the tables of contents): its page numbers, or every entry again.</summary>
    /// <param name="mode">What it rewrites.</param>
    /// <param name="index">Which table; null for the one at the selection.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation UpdateTableOfContents(TocUpdateMode mode, int? index = null) => Op(CommandId.UpdateTableOfContents, new Arg("mode", Node(mode)), new Arg("index", index is { } k ? Node(k) : null, Optional: true));

    /// <summary>Updates every table of contents and of figures but the locked ones (as before printing).</summary>
    /// <param name="mode">What it rewrites.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation UpdateAllTablesOfContents(TocUpdateMode mode) => Op(CommandId.UpdateAllTablesOfContents, new Arg("value", Node(mode)));

    /// <summary>Removes the table of contents the selection is in (else the first; <paramref name="index"/>: that one), with its building block control and title.</summary>
    /// <param name="index">Which table; null for the one at the selection.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation RemoveTableOfContents(int? index = null) => Op(CommandId.RemoveTableOfContents, new Arg("index", index is { } k ? Node(k) : null, Optional: true));

    /// <summary>Marks a table of contents entry after the selection (Mark Entry, Alt+Shift+O): a hidden table entry field.</summary>
    /// <param name="text">The entry's text.</param>
    /// <param name="level">Its level, 1 to 9.</param>
    /// <param name="identifier">The tables that collect it: those taking entry fields of this identifier; null for <c>C</c>.</param>
    /// <param name="pageNumber">It shows a page number; null for on.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation MarkTableOfContentsEntry(string text, int level, string? identifier = null, bool? pageNumber = null) => Op(CommandId.MarkTableOfContentsEntry, new Arg("text", Node(text)), new Arg("level", Node(level)), new Arg("identifier", identifier is { } i ? Node(i) : null, Optional: true), new Arg("pageNumber", pageNumber is { } p ? Node(p) : null, Optional: true));

    /// <summary>A caption for the selected table, picture or object (or the caret's paragraph); the label's other captions renumber.</summary>
    /// <param name="label">The label: a letter, then letters, digits, spaces and underscores.</param>
    /// <param name="text">What follows the number, as it is (put your own separator first).</param>
    /// <param name="position">Above or below the item; null for above a table, below anything else.</param>
    /// <param name="excludeLabel">The number without the label before it.</param>
    /// <param name="format">The number's format: decimal (null), letters or roman numerals.</param>
    /// <param name="chapter">A chapter number before it.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation InsertCaption(string label, string? text = null, CaptionPosition? position = null, bool? excludeLabel = null, NumberFormat? format = null, CaptionChapter? chapter = null) => Op(CommandId.InsertCaption, new Arg("label", Node(label)), new Arg("text", text is { } t ? Node(t) : null, Optional: true), new Arg("position", position is { } p ? Node(p) : null, Optional: true), new Arg("excludeLabel", excludeLabel is { } e ? Node(e) : null, Optional: true), new Arg("format", format is { } f ? Node(f) : null, Optional: true), new Arg("chapter", chapter is { } c ? Node(c) : null, Optional: true));

    /// <summary>The caption every table inserted from now on gets (AutoCaption); null turns it off. A session setting, never saved; works when read only.</summary>
    /// <param name="caption">The caption; null for off.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation SetAutoCaption(AutoCaption? caption) => Op(CommandId.SetAutoCaption, new Arg("value", caption is { } c ? Node(c) : null));

    /// <summary>A cross-reference to the item at <see cref="CrossReferenceTargetRef.Index"/> of what <see cref="EditorReferences.GetCrossReferenceTargetsAsync"/> lists, at the selection.</summary>
    /// <param name="target">What it points to.</param>
    /// <param name="content">What it shows.</param>
    /// <param name="hyperlink">It links to its target; null for on.</param>
    /// <param name="aboveBelow">"above" or "below" follows it.</param>
    /// <returns>The operation, for <see cref="EditorSession.BatchAsync"/>.</returns>
    public static EditorOperation InsertCrossReference(CrossReferenceTargetRef target, CrossReferenceContent content, bool? hyperlink = null, bool? aboveBelow = null) => Op(CommandId.InsertCrossReference, new Arg("target", Node(target)), new Arg("content", Node(content)), new Arg("hyperlink", hyperlink is { } h ? Node(h) : null, Optional: true), new Arg("aboveBelow", aboveBelow is { } a ? Node(a) : null, Optional: true));
}
