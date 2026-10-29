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
