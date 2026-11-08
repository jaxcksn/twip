using System.Text.Json.Nodes;
using System.Text.Json.Serialization;
using Twip.Native;
using Twip.Wire;

namespace Twip;

/// <summary>Dialog models: the values and field states an application's own dialogs show, and applying them.</summary>
public sealed class EditorDialogs
{
    private readonly EditorSession _session;

    internal EditorDialogs(EditorSession session) => _session = session;

    /// <summary>Opens the Font dialog model for the selection.</summary>
    /// <param name="ct">Cancels the request.</param>
    /// <returns>The dialog session.</returns>
    public Task<DialogSession<FontDialogValues>> OpenFontAsync(CancellationToken ct = default) => OpenAsync<FontDialogValues>(DialogKind.Font, ct);

    /// <summary>Opens the Paragraph dialog model for the selection.</summary>
    /// <param name="ct">Cancels the request.</param>
    /// <returns>The dialog session.</returns>
    public Task<DialogSession<ParagraphDialogValues>> OpenParagraphAsync(CancellationToken ct = default) => OpenAsync<ParagraphDialogValues>(DialogKind.Paragraph, ct);

    /// <summary>Opens the Page Setup dialog model for the selection.</summary>
    /// <param name="ct">Cancels the request.</param>
    /// <returns>The dialog session.</returns>
    public Task<DialogSession<PageSetupDialogValues>> OpenPageSetupAsync(CancellationToken ct = default) => OpenAsync<PageSetupDialogValues>(DialogKind.PageSetup, ct);

    /// <summary>Opens the Insert Table dialog model.</summary>
    /// <param name="ct">Cancels the request.</param>
    /// <returns>The dialog session.</returns>
    public Task<DialogSession<InsertTableDialogValues>> OpenInsertTableAsync(CancellationToken ct = default) => OpenAsync<InsertTableDialogValues>(DialogKind.InsertTable, ct);

    /// <summary>Opens the Table Properties dialog model for the selection.</summary>
    /// <param name="ct">Cancels the request.</param>
    /// <returns>The dialog session.</returns>
    public Task<DialogSession<TablePropertiesDialogValues>> OpenTablePropertiesAsync(CancellationToken ct = default) => OpenAsync<TablePropertiesDialogValues>(DialogKind.TableProperties, ct);

    /// <summary>Opens the picture Size dialog model for the selected picture.</summary>
    /// <param name="ct">Cancels the request.</param>
    /// <returns>The dialog session.</returns>
    public Task<DialogSession<PictureSizeDialogValues>> OpenPictureSizeAsync(CancellationToken ct = default) => OpenAsync<PictureSizeDialogValues>(DialogKind.PictureSize, ct);

    /// <summary>Opens the Alt Text dialog model for the selected object.</summary>
    /// <param name="ct">Cancels the request.</param>
    /// <returns>The dialog session.</returns>
    public Task<DialogSession<AltTextDialogValues>> OpenAltTextAsync(CancellationToken ct = default) => OpenAsync<AltTextDialogValues>(DialogKind.AltText, ct);

    /// <summary>Opens the Hyperlink dialog model for the selection.</summary>
    /// <param name="ct">Cancels the request.</param>
    /// <returns>The dialog session.</returns>
    public Task<DialogSession<HyperlinkDialogValues>> OpenHyperlinkAsync(CancellationToken ct = default) => OpenAsync<HyperlinkDialogValues>(DialogKind.Hyperlink, ct);

    /// <summary>Opens the Tabs dialog model for the selected paragraphs.</summary>
    /// <param name="ct">Cancels the request.</param>
    /// <returns>The dialog session.</returns>
    public Task<DialogSession<TabsDialogValues>> OpenTabsAsync(CancellationToken ct = default) => OpenAsync<TabsDialogValues>(DialogKind.Tabs, ct);

    /// <summary>Runs the accessibility check.</summary>
    /// <param name="ct">Cancels the request.</param>
    /// <returns>The issues found.</returns>
    public async Task<AccessibilityCheck> OpenAccessibilityCheckAsync(CancellationToken ct = default)
    {
        var model = await _session.CallAsync<DialogModel>(Method.DialogOpen, WireJson.ToUtf8(DialogKind.Accessibility), ct).ConfigureAwait(false);
        return new AccessibilityCheck(((AccessibilityDialogValues)model.Values.Values).Issues);
    }

    /// <summary>Opens Manage Styles: its Edit, Recommend, Restrict and Set Defaults tabs.</summary>
    /// <param name="ct">Cancels the request.</param>
    /// <returns>The dialog session.</returns>
    public Task<DialogSession<ManageStylesDialogValues>> OpenManageStylesAsync(CancellationToken ct = default) => OpenAsync<ManageStylesDialogValues>(DialogKind.ManageStyles, ct);

    /// <summary>Opens Borders and Shading on the selection's paragraphs and text.</summary>
    /// <param name="ct">Cancels the request.</param>
    /// <returns>The dialog session.</returns>
    public Task<DialogSession<BordersShadingDialogValues>> OpenBordersShadingAsync(CancellationToken ct = default) => OpenAsync<BordersShadingDialogValues>(DialogKind.BordersShading, ct);

    /// <summary>Opens the Frame dialog on the frame the selection is in.</summary>
    /// <param name="ct">Cancels the request.</param>
    /// <returns>The dialog session.</returns>
    public Task<DialogSession<FrameDialogValues>> OpenFrameAsync(CancellationToken ct = default) => OpenAsync<FrameDialogValues>(DialogKind.Frame, ct);

    /// <summary>Opens Define New Bullet on the selection's list level.</summary>
    /// <param name="ct">Cancels the request.</param>
    /// <returns>The dialog session.</returns>
    public Task<DialogSession<DefineBulletDialogValues>> OpenDefineBulletAsync(CancellationToken ct = default) => OpenAsync<DefineBulletDialogValues>(DialogKind.DefineBullet, ct);

    /// <summary>Opens Define New Number Format on the selection's list level.</summary>
    /// <param name="ct">Cancels the request.</param>
    /// <returns>The dialog session.</returns>
    public Task<DialogSession<DefineNumberFormatDialogValues>> OpenDefineNumberFormatAsync(CancellationToken ct = default) => OpenAsync<DefineNumberFormatDialogValues>(DialogKind.DefineNumberFormat, ct);

    /// <summary>Opens Define New Multilevel List on the selection's list.</summary>
    /// <param name="ct">Cancels the request.</param>
    /// <returns>The dialog session.</returns>
    public Task<DialogSession<DefineMultilevelDialogValues>> OpenDefineMultilevelListAsync(CancellationToken ct = default) => OpenAsync<DefineMultilevelDialogValues>(DialogKind.DefineMultilevelList, ct);

    /// <summary>Opens Set Numbering Value on the selection's list paragraph.</summary>
    /// <param name="ct">Cancels the request.</param>
    /// <returns>The dialog session.</returns>
    public Task<DialogSession<SetNumberingValueDialogValues>> OpenSetNumberingValueAsync(CancellationToken ct = default) => OpenAsync<SetNumberingValueDialogValues>(DialogKind.SetNumberingValue, ct);

    /// <summary>Opens Adjust List Indents on the selection's list level.</summary>
    /// <param name="ct">Cancels the request.</param>
    /// <returns>The dialog session.</returns>
    public Task<DialogSession<AdjustListIndentsDialogValues>> OpenAdjustListIndentsAsync(CancellationToken ct = default) => OpenAsync<AdjustListIndentsDialogValues>(DialogKind.AdjustListIndents, ct);

    /// <summary>
    /// Opens the style dialog: Modify Style on a style, or Create New Style from Formatting on the selection's formatting
    /// (in its short form, the name and the preview, until <see cref="StyleDialogValues.ShortForm"/> is turned off).
    /// </summary>
    /// <param name="open">Create a style, or modify one.</param>
    /// <param name="ct">Cancels the request.</param>
    /// <returns>The style dialog.</returns>
    public async Task<StyleDialogSession> OpenStyleAsync(StyleOpen open, CancellationToken ct = default)
    {
        ArgumentNullException.ThrowIfNull(open);
        return new(_session, await _session.CallAsync<DialogModel>(Method.DialogOpenStyle, WireJson.ToUtf8(open), ct).ConfigureAwait(false));
    }

    private async Task<DialogSession<T>> OpenAsync<T>(DialogKind kind, CancellationToken ct) where T : class =>
        new(_session, await _session.CallAsync<DialogModel>(Method.DialogOpen, WireJson.ToUtf8(kind), ct).ConfigureAwait(false));
}

/// <summary>An open dialog model. Update values, then apply or cancel.</summary>
/// <typeparam name="TValues">The dialog's values record.</typeparam>
public sealed class DialogSession<TValues> where TValues : class
{
    private readonly EditorSession _session;
    private readonly DialogModel _model;

    internal DialogSession(EditorSession session, DialogModel model) => (_session, _model) = (session, model);

    internal DialogModel Model => _model;

    /// <summary>The selection's current values, mixed where they differ.</summary>
    public TValues Values => (TValues)_model.Values.Values;

    /// <summary>Each field's label, kind, range and enabled state, keyed by property name.</summary>
    public IReadOnlyDictionary<string, FieldInfo> Fields => _model.Fields;

    /// <summary>Changes values; dependent values and field states are recomputed.</summary>
    /// <param name="changed">The values with the changes, made with <c>with</c>.</param>
    /// <param name="ct">Cancels the update.</param>
    /// <returns>The recomputed dialog model.</returns>
    public async Task<DialogSession<TValues>> UpdateAsync(TValues changed, CancellationToken ct = default)
    {
        ArgumentNullException.ThrowIfNull(changed);
        var request = new DialogUpdateRequest(_model, Patch(changed));
        return new(_session, await _session.CallAsync<DialogModel>(Method.DialogUpdate, WireJson.ToUtf8(request), ct).ConfigureAwait(false));
    }

    /// <summary>The dialog's kind and the fields of <paramref name="changed"/> that differ, so the engine knows which ones the user changed.</summary>
    internal JsonObject Patch(TValues changed)
    {
        var before = WireJson.ToNode(_model.Values)!.AsObject();
        var patch = new JsonObject();
        foreach (var (key, value) in WireJson.ToNode(new DialogValues(_model.Kind, changed))!.AsObject())
        {
            if (key == "kind" || !JsonNode.DeepEquals(value, before[key])) patch[key] = value?.DeepClone();
        }
        return patch;
    }

    /// <summary>Validates and applies the values: one transaction, one undo step.</summary>
    /// <param name="ct">Cancels before applying.</param>
    /// <returns>Completes when the change has committed.</returns>
    public Task ApplyAsync(CancellationToken ct = default) => _session.CallAsync(Method.DialogApply, WireJson.ToUtf8(_model), ct);

    /// <summary>Closes the dialog model without changing anything. The engine holds no open dialog, so this frees nothing.</summary>
    public void Cancel() { }
}

/// <summary>How the style dialog opens: creating a style from the selection's formatting, or modifying one.</summary>
[JsonConverter(typeof(StyleOpenConverter))]
public sealed record StyleOpen
{
    private StyleOpen(StyleId? style) => Style = style;

    /// <summary>The style being modified; null when creating one.</summary>
    public StyleId? Style { get; }

    /// <summary>Create New Style from Formatting.</summary>
    public static StyleOpen Create { get; } = new((StyleId?)null);

    /// <summary>Modify Style.</summary>
    /// <param name="style">The style.</param>
    /// <returns>The opening.</returns>
    public static StyleOpen Modify(StyleId style) => new(style);
}

/// <summary>The kind of style the style dialog makes.</summary>
public enum StyleDialogType
{
    /// <summary>A paragraph style.</summary>
    Paragraph,
    /// <summary>A character style.</summary>
    Character,
    /// <summary>A paragraph style and a character style, linked.</summary>
    Linked,
    /// <summary>A table style.</summary>
    Table,
    /// <summary>A list style.</summary>
    List,
}

/// <summary>The style dialog's values.</summary>
/// <param name="Open">Creating a style, or modifying one.</param>
/// <param name="ShortForm">The short form: the name and the preview.</param>
/// <param name="Name">The style's name; a built-in style renamed keeps its name and takes the new one as an alias.</param>
/// <param name="Type">Its type, chosen when creating.</param>
/// <param name="BasedOn">The style it is based on.</param>
/// <param name="Next">The style of the paragraph after one in this style.</param>
/// <param name="Font">The font.</param>
/// <param name="Size">The size.</param>
/// <param name="Bold">Bold.</param>
/// <param name="Italic">Italic.</param>
/// <param name="Underline">The underline.</param>
/// <param name="Color">The font color.</param>
/// <param name="Alignment">The alignment (paragraph and linked styles).</param>
/// <param name="LineSpacing">The line spacing.</param>
/// <param name="SpaceBefore">The space before paragraphs.</param>
/// <param name="SpaceAfter">The space after paragraphs.</param>
/// <param name="LeftIndent">The left indent.</param>
/// <param name="Quick">Shown in the Styles gallery.</param>
/// <param name="AutoUpdate">Direct formatting of a paragraph in the style changes the style.</param>
/// <param name="TablePart">A table style: the part the formatting applies to.</param>
/// <param name="Fill">A table style: the part's cell fill, or null.</param>
/// <param name="CellAlignment">A table style: the part's cells' vertical alignment.</param>
/// <param name="Border">A table style: the part's border line, on every edge, or null.</param>
/// <param name="RowBand">A table style: the rows in each band.</param>
/// <param name="ColumnBand">A table style: the columns in each band.</param>
/// <param name="ListLevels">A list style: its levels.</param>
/// <param name="ListLevel">A list style: the level shown (0 to 8).</param>
/// <param name="Description">The style's formatting in words, written by the editor.</param>
/// <param name="Pending">The style as changed so far, the Format menu's dialogs included: give it back unchanged.</param>
public sealed record StyleDialogValues(
    StyleOpen Open, bool ShortForm, string Name, StyleDialogType Type, StyleId? BasedOn, StyleId? Next, string Font, Length Size, bool Bold, bool Italic,
    UnderlineStyle Underline, ColorValue Color, ParagraphAlignment Alignment, LineSpacing LineSpacing, Length SpaceBefore, Length SpaceAfter,
    Length LeftIndent, bool Quick, bool AutoUpdate, TableStylePart TablePart, ColorValue? Fill, CellAlignment CellAlignment, CellBorder? Border,
    int RowBand, int ColumnBand, IReadOnlyList<ListLevel> ListLevels, int ListLevel, string Description, string Pending);

/// <summary>The parts of a table a table style formats.</summary>
public enum TableStylePart
{
    /// <summary>The whole table.</summary>
    WholeTable,
    /// <summary>The header row.</summary>
    HeaderRow,
    /// <summary>The total row.</summary>
    TotalRow,
    /// <summary>The first column.</summary>
    FirstColumn,
    /// <summary>The last column.</summary>
    LastColumn,
    /// <summary>Odd banded rows.</summary>
    OddBandedRows,
    /// <summary>Even banded rows.</summary>
    EvenBandedRows,
    /// <summary>Odd banded columns.</summary>
    OddBandedColumns,
    /// <summary>Even banded columns.</summary>
    EvenBandedColumns,
    /// <summary>The top left cell.</summary>
    TopLeftCell,
    /// <summary>The top right cell.</summary>
    TopRightCell,
    /// <summary>The bottom left cell.</summary>
    BottomLeftCell,
    /// <summary>The bottom right cell.</summary>
    BottomRightCell,
}

/// <summary>Define New Bullet's values.</summary>
/// <param name="Text">The bullet's character.</param>
/// <param name="Font">Its font.</param>
/// <param name="Picture">A picture bullet's image, instead of the character.</param>
/// <param name="Alignment">Its alignment.</param>
public sealed record DefineBulletDialogValues(string Text, string? Font, byte[]? Picture, LevelAlignment Alignment);

/// <summary>Define New Number Format's values.</summary>
/// <param name="Format">The number style.</param>
/// <param name="Text">The number format, with %1 for the number.</param>
/// <param name="Font">The number's font.</param>
/// <param name="Alignment">Its alignment.</param>
public sealed record DefineNumberFormatDialogValues(NumberFormat Format, string Text, string? Font, LevelAlignment Alignment);

/// <summary>Define New Multilevel List's values.</summary>
/// <param name="Levels">The nine levels.</param>
/// <param name="Level">The level shown (0 to 8).</param>
/// <param name="ApplyTo">What the levels apply to.</param>
/// <param name="GalleryLevel">The level the gallery shows (1 to 9).</param>
/// <param name="ListName">The ListNum field list name.</param>
public sealed record DefineMultilevelDialogValues(IReadOnlyList<ListLevel> Levels, int Level, ListApplyTo ApplyTo, int GalleryLevel, string ListName);

/// <summary>Set Numbering Value's values.</summary>
/// <param name="Mode">Start a new list, or continue.</param>
/// <param name="Value">The value: the paragraph's number when it opens.</param>
/// <param name="Advance">Continuing: skip numbers to the value.</param>
public sealed record SetNumberingValueDialogValues(NumberingMode Mode, int Value, bool Advance);

/// <summary>Adjust List Indents' values.</summary>
/// <param name="NumberPosition">Where the number is.</param>
/// <param name="TextIndent">Where the text is.</param>
/// <param name="Follow">What follows the number.</param>
/// <param name="Tab">The tab stop after the number; null uses the text indent.</param>
public sealed record AdjustListIndentsDialogValues(Length NumberPosition, Length TextIndent, FollowNumber Follow, Length? Tab);

/// <summary>Bullets and Numbering's values (the style dialog's Format > Numbering).</summary>
/// <param name="Gallery">The gallery shown.</param>
/// <param name="Entry">The gallery entry, or null for no list.</param>
public sealed record BulletsNumberingDialogValues(ListGalleryKind Gallery, string? Entry);

/// <summary>Borders and Shading's tabs.</summary>
public enum BordersTab
{
    /// <summary>The paragraph's or text's borders.</summary>
    Borders,
    /// <summary>The page's borders.</summary>
    PageBorder,
    /// <summary>The shading.</summary>
    Shading,
}

/// <summary>Borders and Shading's settings.</summary>
public enum BorderSetting
{
    /// <summary>No borders.</summary>
    None,
    /// <summary>The four edges in one line.</summary>
    Box,
    /// <summary>A box with a shadow on its bottom and right edges.</summary>
    Shadow,
    /// <summary>The edges set one by one.</summary>
    Custom,
}

/// <summary>Borders and Shading's values: a setting sets the edges with the line; setting an edge makes it custom.</summary>
/// <param name="Tab">The tab shown.</param>
/// <param name="Setting">The setting.</param>
/// <param name="Style">The line style the setting and new edges take.</param>
/// <param name="Color">Their color.</param>
/// <param name="Width">Their width.</param>
/// <param name="Top">The top edge, or null.</param>
/// <param name="Bottom">The bottom edge, or null.</param>
/// <param name="Left">The left edge, or null.</param>
/// <param name="Right">The right edge, or null.</param>
/// <param name="Between">The line between paragraphs (a table style part's inside horizontal line), or null.</param>
/// <param name="InsideVertical">A table style part's inside vertical line, or null.</param>
/// <param name="ApplyTo">The paragraphs, the text (a box only), or a table style part's cells.</param>
/// <param name="Fill">The shading's fill.</param>
/// <param name="Pattern">The shading's pattern.</param>
/// <param name="PatternColor">The pattern's color.</param>
/// <param name="PageSetting">Page Border: the setting.</param>
/// <param name="PageTop">Page Border: the top edge, or null.</param>
/// <param name="PageBottom">Page Border: the bottom edge, or null.</param>
/// <param name="PageLeft">Page Border: the left edge, or null.</param>
/// <param name="PageRight">Page Border: the right edge, or null.</param>
/// <param name="PageOffsetFrom">Page Border: what the borders are measured from.</param>
/// <param name="PageDisplay">Page Border: the pages they show on.</param>
/// <param name="PageApplyTo">Page Border: the sections they apply to.</param>
public sealed record BordersShadingDialogValues(
    BordersTab Tab,
    BorderSetting Setting,
    LineStyle Style,
    ColorValue Color,
    Length Width,
    CellBorder? Top,
    CellBorder? Bottom,
    CellBorder? Left,
    CellBorder? Right,
    CellBorder? Between,
    CellBorder? InsideVertical,
    ShadingTarget ApplyTo,
    ColorValue Fill,
    ShadingPattern Pattern,
    ColorValue PatternColor,
    BorderSetting PageSetting,
    CellBorder? PageTop,
    CellBorder? PageBottom,
    CellBorder? PageLeft,
    CellBorder? PageRight,
    PageBorderOffset PageOffsetFrom,
    PageBorderDisplay PageDisplay,
    PageBorderScope PageApplyTo);

/// <summary>Banding's values: a table style's rows and columns in each band.</summary>
/// <param name="RowBand">The rows in each band.</param>
/// <param name="ColumnBand">The columns in each band.</param>
public sealed record BandingDialogValues(int RowBand, int ColumnBand);

/// <summary>The Frame dialog's values: the frame's properties, Move with text and Remove Frame.</summary>
/// <param name="Wrap">Text wrapping.</param>
/// <param name="WidthRule">How the width is set.</param>
/// <param name="Width">The width, when exact.</param>
/// <param name="HeightRule">How the height is set.</param>
/// <param name="Height">The height, unless automatic.</param>
/// <param name="HorizontalAlign">The horizontal alignment, or null for <paramref name="Horizontal"/>.</param>
/// <param name="Horizontal">The horizontal position.</param>
/// <param name="HorizontalRelative">What it is relative to.</param>
/// <param name="HorizontalDistance">The distance from text, left and right.</param>
/// <param name="VerticalAlign">The vertical alignment, or null for <paramref name="Vertical"/>.</param>
/// <param name="Vertical">The vertical position.</param>
/// <param name="VerticalRelative">What it is relative to.</param>
/// <param name="VerticalDistance">The distance from text, above and below.</param>
/// <param name="MoveWithText">The vertical position is relative to the paragraph.</param>
/// <param name="LockAnchor">Lock anchor.</param>
/// <param name="Remove">Remove Frame: applying removes the frame.</param>
public sealed record FrameDialogValues(
    FrameWrap Wrap,
    FrameWidthRule WidthRule,
    Length Width,
    FrameHeightRule HeightRule,
    Length Height,
    FrameHorizontalAlign? HorizontalAlign,
    Length Horizontal,
    FrameHorizontalRelative HorizontalRelative,
    Length HorizontalDistance,
    FrameVerticalAlign? VerticalAlign,
    Length Vertical,
    FrameVerticalRelative VerticalRelative,
    Length VerticalDistance,
    bool MoveWithText,
    bool LockAnchor,
    bool Remove);

/// <summary>Manage Styles' tabs.</summary>
public enum ManageStylesTab
{
    /// <summary>The styles, to modify, delete or add to.</summary>
    Edit,
    /// <summary>Which styles show in lists, and in what order.</summary>
    Recommend,
    /// <summary>Which styles formatting protection permits.</summary>
    Restrict,
    /// <summary>The document's default formatting.</summary>
    SetDefaults,
}

/// <summary>How Manage Styles sorts its list.</summary>
public enum StyleSort
{
    /// <summary>By priority, then name.</summary>
    AsRecommended,
    /// <summary>By name.</summary>
    Alphabetical,
    /// <summary>By the style each is based on.</summary>
    BasedOn,
    /// <summary>By type.</summary>
    ByType,
}

/// <summary>A style in Manage Styles.</summary>
/// <param name="Id">The style.</param>
/// <param name="Name">Its name.</param>
/// <param name="Type">Its type.</param>
/// <param name="BuiltIn">Built in (can't be deleted).</param>
/// <param name="Visibility">Whether it shows in style lists.</param>
/// <param name="Priority">Its sort priority.</param>
/// <param name="Locked">Restricted while formatting is protected.</param>
public sealed record ManagedStyle(StyleId Id, string Name, StyleType Type, bool BuiltIn, StyleVisibility Visibility, int Priority, bool Locked);

/// <summary>Manage Styles' values.</summary>
/// <param name="Tab">The tab shown.</param>
/// <param name="Sort">How the list sorts.</param>
/// <param name="RecommendedOnly">Only the styles that show in lists.</param>
/// <param name="Styles">The styles in <paramref name="Sort"/> order: their visibility, priority and lock are what Recommend and Restrict change.</param>
/// <param name="Selected">The style selected in the list.</param>
/// <param name="Description">The selected style's formatting in words, written by the editor.</param>
/// <param name="Defaults">The document's default formatting (Set Defaults).</param>
public sealed record ManageStylesDialogValues(
    ManageStylesTab Tab, StyleSort Sort, bool RecommendedOnly, IReadOnlyList<ManagedStyle> Styles, StyleId? Selected, string Description, DocumentDefaults Defaults);

/// <summary>The style dialog: its values and fields, and its Format menu.</summary>
public sealed class StyleDialogSession
{
    private readonly EditorSession _session;
    private readonly DialogSession<StyleDialogValues> _dialog;

    internal StyleDialogSession(EditorSession session, DialogModel model) => (_session, _dialog) = (session, new(session, model));

    /// <summary>The style's values.</summary>
    public StyleDialogValues Values => _dialog.Values;

    /// <summary>Each field's label, kind, range and enabled state, keyed by property name.</summary>
    public IReadOnlyDictionary<string, FieldInfo> Fields => _dialog.Fields;

    /// <summary>Changes values; the style's formatting, description and field states are recomputed.</summary>
    /// <param name="changed">The values with the changes, made with <c>with</c>.</param>
    /// <param name="ct">Cancels the update.</param>
    /// <returns>The recomputed style dialog.</returns>
    public async Task<StyleDialogSession> UpdateAsync(StyleDialogValues changed, CancellationToken ct = default) =>
        new(_session, (await _dialog.UpdateAsync(changed, ct).ConfigureAwait(false)).Model);

    /// <summary>Validates and applies: the style added or changed (a new one applied to the selection), one undo step.</summary>
    /// <param name="ct">Cancels before applying.</param>
    /// <returns>Completes when the change has committed.</returns>
    public Task ApplyAsync(CancellationToken ct = default) => _dialog.ApplyAsync(ct);

    /// <summary>Opens a Format menu dialog (Font, Paragraph, Tabs) on the style: applying it changes this dialog, not the document.</summary>
    /// <typeparam name="TValues">The dialog's values record: <see cref="FontDialogValues"/>, <see cref="ParagraphDialogValues"/>, <see cref="TabsDialogValues"/>, <see cref="BordersShadingDialogValues"/> or <see cref="FrameDialogValues"/>.</typeparam>
    /// <param name="kind">The dialog.</param>
    /// <param name="ct">Cancels the request.</param>
    /// <returns>The Format menu dialog.</returns>
    public async Task<FormatDialogSession<TValues>> OpenFormatAsync<TValues>(DialogKind kind, CancellationToken ct = default) where TValues : class
    {
        var request = new DialogOnStyleRequest(kind, Values);
        return new(_session, await _session.CallAsync<DialogModel>(Method.DialogOpenOnStyle, WireJson.ToUtf8(request), ct).ConfigureAwait(false));
    }

    /// <summary>The preview box with the changes not yet applied: a greyed previous paragraph, text in the style, a greyed following one.</summary>
    /// <param name="size">The picture's size.</param>
    /// <param name="ct">Cancels the request.</param>
    /// <returns>The preview as PNG.</returns>
    public Task<PreviewPicture> PreviewAsync(PreviewSize size, CancellationToken ct = default) =>
        _session.CallAsync<PreviewPicture>(Method.DialogPreview, WireJson.ToUtf8(new DialogPreviewRequest(_dialog.Model, size)), ct);

    /// <summary>Closes the dialog without changing anything.</summary>
    public void Cancel() { }
}

/// <summary>A dialog of the style dialog's Format menu.</summary>
/// <typeparam name="TValues">The dialog's values record.</typeparam>
public sealed class FormatDialogSession<TValues> where TValues : class
{
    private readonly EditorSession _session;
    private readonly DialogSession<TValues> _dialog;

    internal FormatDialogSession(EditorSession session, DialogModel model) => (_session, _dialog) = (session, new(session, model));

    /// <summary>The style's values for this dialog.</summary>
    public TValues Values => _dialog.Values;

    /// <summary>Each field's label, kind, range and enabled state, keyed by property name.</summary>
    public IReadOnlyDictionary<string, FieldInfo> Fields => _dialog.Fields;

    /// <summary>Changes values; dependent values and field states are recomputed.</summary>
    /// <param name="changed">The values with the changes, made with <c>with</c>.</param>
    /// <param name="ct">Cancels the update.</param>
    /// <returns>The recomputed dialog.</returns>
    public async Task<FormatDialogSession<TValues>> UpdateAsync(TValues changed, CancellationToken ct = default) =>
        new(_session, (await _dialog.UpdateAsync(changed, ct).ConfigureAwait(false)).Model);

    /// <summary>Applies it to the style dialog.</summary>
    /// <param name="ct">Cancels the request.</param>
    /// <returns>The style dialog after it.</returns>
    public async Task<StyleDialogSession> ApplyAsync(CancellationToken ct = default) =>
        new(_session, await _session.CallAsync<DialogModel>(Method.DialogApplyToStyle, WireJson.ToUtf8(_dialog.Model), ct).ConfigureAwait(false));

    /// <summary>Closes the dialog without changing anything.</summary>
    public void Cancel() { }
}

/// <summary>What kind of input a dialog field takes.</summary>
public enum FieldKind
{
    /// <summary>A number.</summary>
    Number,
    /// <summary>A length.</summary>
    Length,
    /// <summary>Text.</summary>
    Text,
    /// <summary>One of a set of options.</summary>
    Choice,
    /// <summary>On or off.</summary>
    Toggle,
    /// <summary>A colour.</summary>
    Color,
    /// <summary>A list.</summary>
    List,
}

/// <summary>An option of a choice field.</summary>
/// <param name="Value">The value.</param>
/// <param name="Label">The label.</param>
public sealed record FieldOption(string Value, string Label);

/// <summary>A dialog field's label and state.</summary>
/// <param name="Label">The label.</param>
/// <param name="Enabled">Whether it can be changed now.</param>
/// <param name="Kind">The input kind.</param>
/// <param name="Min">The minimum.</param>
/// <param name="Max">The maximum.</param>
/// <param name="Step">The step.</param>
/// <param name="Options">A choice field's options.</param>
public sealed record FieldInfo(
    string Label, bool Enabled, FieldKind Kind,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] double? Min = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] double? Max = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] double? Step = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] IReadOnlyList<FieldOption>? Options = null);

/// <summary>A font style in the Font dialog.</summary>
public enum FontStyleChoice
{
    /// <summary>Regular.</summary>
    Regular,
    /// <summary>Italic.</summary>
    Italic,
    /// <summary>Bold.</summary>
    Bold,
    /// <summary>Bold italic.</summary>
    BoldItalic,
}

/// <summary>The Font dialog's values.</summary>
/// <param name="Font">The family.</param>
/// <param name="Style">The style.</param>
/// <param name="Size">The size.</param>
/// <param name="Color">The text colour.</param>
/// <param name="Underline">The underline style.</param>
/// <param name="UnderlineColor">The underline colour.</param>
/// <param name="Strikethrough">Strikethrough.</param>
/// <param name="DoubleStrikethrough">Double strikethrough.</param>
/// <param name="Superscript">Superscript.</param>
/// <param name="Subscript">Subscript.</param>
/// <param name="SmallCaps">Small capitals.</param>
/// <param name="AllCaps">All capitals.</param>
/// <param name="Hidden">Hidden.</param>
/// <param name="CharacterSpacing">Character spacing.</param>
/// <param name="Position">Raised or lowered position.</param>
public sealed record FontDialogValues(
    FormatValue<string> Font, FormatValue<FontStyleChoice> Style, FormatValue<Length> Size, FormatValue<ColorValue> Color,
    FormatValue<UnderlineStyle> Underline, FormatValue<ColorValue> UnderlineColor, FormatValue<bool> Strikethrough,
    FormatValue<bool> DoubleStrikethrough, FormatValue<bool> Superscript, FormatValue<bool> Subscript, FormatValue<bool> SmallCaps,
    FormatValue<bool> AllCaps, FormatValue<bool> Hidden, FormatValue<Length> CharacterSpacing, FormatValue<Length> Position);

/// <summary>A special indent in the Paragraph dialog.</summary>
public enum SpecialIndent
{
    /// <summary>None.</summary>
    None,
    /// <summary>A first-line indent.</summary>
    FirstLine,
    /// <summary>A hanging indent.</summary>
    Hanging,
}

/// <summary>A paragraph's outline level: 1 to 9, or body text.</summary>
[JsonConverter(typeof(OutlineLevelConverter))]
public readonly record struct OutlineLevel
{
    private OutlineLevel(int level) => Level = level;

    /// <summary>The level, 1 to 9; 0 for body text.</summary>
    public int Level { get; }

    /// <summary>Whether this is body text.</summary>
    public bool IsBody => Level == 0;

    /// <summary>Body text.</summary>
    public static OutlineLevel Body => default;

    /// <summary>An outline level.</summary>
    /// <param name="level">The level, 1 to 9.</param>
    public static OutlineLevel FromLevel(int level) => new(level);
}

/// <summary>The Paragraph dialog's values.</summary>
/// <param name="Alignment">The alignment.</param>
/// <param name="OutlineLevel">The outline level.</param>
/// <param name="Left">The left indent.</param>
/// <param name="Right">The right indent.</param>
/// <param name="Special">The special indent.</param>
/// <param name="By">The special indent's size.</param>
/// <param name="Before">The space before.</param>
/// <param name="After">The space after.</param>
/// <param name="LineSpacing">The line spacing.</param>
/// <param name="KeepWithNext">Keep with next.</param>
/// <param name="KeepLinesTogether">Keep lines together.</param>
/// <param name="PageBreakBefore">Page break before.</param>
/// <param name="WidowControl">Widow control.</param>
public sealed record ParagraphDialogValues(
    FormatValue<ParagraphAlignment> Alignment, FormatValue<OutlineLevel> OutlineLevel, FormatValue<Length> Left, FormatValue<Length> Right,
    FormatValue<SpecialIndent> Special, FormatValue<Length> By, FormatValue<SpacingValue> Before, FormatValue<SpacingValue> After,
    FormatValue<LineSpacing> LineSpacing, FormatValue<bool> KeepWithNext, FormatValue<bool> KeepLinesTogether,
    FormatValue<bool> PageBreakBefore, FormatValue<bool> WidowControl);

/// <summary>What page setup applies to.</summary>
public enum PageSetupScope
{
    /// <summary>The selection's sections.</summary>
    Section,
    /// <summary>The whole document.</summary>
    WholeDocument,
}

/// <summary>The Page Setup dialog's values.</summary>
/// <param name="Top">The top margin.</param>
/// <param name="Bottom">The bottom margin.</param>
/// <param name="Left">The left margin.</param>
/// <param name="Right">The right margin.</param>
/// <param name="Gutter">The gutter.</param>
/// <param name="Orientation">The orientation.</param>
/// <param name="Width">The paper width.</param>
/// <param name="Height">The paper height.</param>
/// <param name="HeaderDistance">The header distance.</param>
/// <param name="FooterDistance">The footer distance.</param>
/// <param name="DifferentFirstPage">A different first page.</param>
/// <param name="DifferentOddEven">Different odd and even pages.</param>
/// <param name="ApplyTo">What the values apply to.</param>
public sealed record PageSetupDialogValues(
    Length Top, Length Bottom, Length Left, Length Right, Length Gutter, PageOrientation Orientation, Length Width, Length Height,
    Length HeaderDistance, Length FooterDistance, bool DifferentFirstPage, bool DifferentOddEven, PageSetupScope ApplyTo);

/// <summary>The Insert Table dialog's values.</summary>
/// <param name="Rows">The rows.</param>
/// <param name="Columns">The columns.</param>
public sealed record InsertTableDialogValues(int Rows, int Columns);

/// <summary>Cell margins.</summary>
/// <param name="Top">Top.</param>
/// <param name="Bottom">Bottom.</param>
/// <param name="Left">Left.</param>
/// <param name="Right">Right.</param>
public sealed record CellMargins(Length Top, Length Bottom, Length Left, Length Right);

/// <summary>The Table Properties dialog's values.</summary>
/// <param name="Width">The table width.</param>
/// <param name="Alignment">The table alignment.</param>
/// <param name="Indent">The table indent.</param>
/// <param name="RowHeight">The row height.</param>
/// <param name="RowCanBreak">Rows can break across pages.</param>
/// <param name="HeaderRow">Repeat as header row.</param>
/// <param name="ColumnWidth">The column width.</param>
/// <param name="CellAlignment">The cell vertical alignment.</param>
/// <param name="CellMargins">The default cell margins.</param>
public sealed record TablePropertiesDialogValues(
    TableWidth Width, TableAlignment Alignment, Length Indent, FormatValue<RowHeight> RowHeight, FormatValue<bool> RowCanBreak,
    FormatValue<bool> HeaderRow, FormatValue<Length> ColumnWidth, FormatValue<CellAlignment> CellAlignment, CellMargins CellMargins);

/// <summary>The picture Size dialog's values.</summary>
/// <param name="Width">The width.</param>
/// <param name="Height">The height.</param>
/// <param name="ScaleWidth">The width scale, in percent.</param>
/// <param name="ScaleHeight">The height scale, in percent.</param>
/// <param name="LockAspect">Lock the aspect ratio.</param>
/// <param name="Reset">Reset to the original size.</param>
public sealed record PictureSizeDialogValues(Length Width, Length Height, double ScaleWidth, double ScaleHeight, bool LockAspect, bool Reset);

/// <summary>The Alt Text dialog's values.</summary>
/// <param name="Text">The alternative text.</param>
/// <param name="Decorative">Decorative.</param>
public sealed record AltTextDialogValues(string Text, bool Decorative);

/// <summary>The Hyperlink dialog's values.</summary>
/// <param name="Text">The text to display.</param>
/// <param name="Address">The address.</param>
public sealed record HyperlinkDialogValues(string Text, string Address);

/// <summary>The Tabs dialog's button actions, applied by <see cref="DialogSession{TValues}.UpdateAsync"/>.</summary>
public enum TabsDialogAction
{
    /// <summary>No action.</summary>
    None,
    /// <summary>Set the stop being edited.</summary>
    Set,
    /// <summary>Clear the stop being edited.</summary>
    Clear,
    /// <summary>Clear every stop.</summary>
    ClearAll,
}

/// <summary>The Tabs dialog's values.</summary>
/// <param name="Stops">The tab stops.</param>
/// <param name="DefaultStop">The default tab spacing.</param>
/// <param name="Position">The stop being edited.</param>
/// <param name="Alignment">Its alignment.</param>
/// <param name="Leader">Its leader.</param>
/// <param name="Cleared">Stops to clear on apply.</param>
/// <param name="Action">The button pressed.</param>
public sealed record TabsDialogValues(
    IReadOnlyList<TabStop> Stops, Length DefaultStop, Length? Position, TabAlignment Alignment, TabLeader Leader,
    IReadOnlyList<Length> Cleared, TabsDialogAction Action);

/// <summary>An accessibility issue.</summary>
/// <param name="Id">The issue id.</param>
/// <param name="Text">A description.</param>
/// <param name="Tip">Whether it is a tip rather than an error.</param>
public sealed record AccessibilityIssue(string Id, string Text, bool Tip);

/// <summary>The accessibility check's result.</summary>
public sealed class AccessibilityCheck
{
    internal AccessibilityCheck(IReadOnlyList<AccessibilityIssue> issues) => Issues = issues;

    /// <summary>The issues found.</summary>
    public IReadOnlyList<AccessibilityIssue> Issues { get; }

    // shortcut: fails unsupported; the C ABI has no call that selects an issue's object yet.
    /// <summary>Selects the issue's object.</summary>
    /// <param name="issue">The issue.</param>
    /// <param name="ct">Cancels the request.</param>
    /// <returns>Completes when it is selected.</returns>
    public Task SelectAsync(AccessibilityIssue issue, CancellationToken ct = default) => Stage.Unsupported("selecting an accessibility issue");
}
