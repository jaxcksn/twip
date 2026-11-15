using System.Text.Json.Serialization;
using Twip.Wire;

namespace Twip;

/// <summary>An immutable snapshot of the session: selection, formatting, command availability and history.</summary>
public sealed record EditorState
{
    /// <summary>The document revision: +1 per committed transaction.</summary>
    [JsonPropertyName("revision")]
    public long DocumentRevision { get; init; }

    /// <summary>The state sequence: +1 per observable change, selection included.</summary>
    [JsonPropertyName("sequence")]
    public long StateSequence { get; init; }

    /// <summary>Whether a document is opening or laying out.</summary>
    public LoadStatus Load { get; init; } = LoadStatus.Ready;

    /// <summary>Editing or design mode.</summary>
    public EditorMode Mode { get; init; }

    /// <summary>The template, or the sub-template being edited.</summary>
    public EditingTarget Editing { get; init; } = EditingTarget.Template;

    /// <summary>Whether the session is read only.</summary>
    public bool ReadOnly { get; init; }

    /// <summary>Whether the document changed since the last open or DOCX save. Undoing back to it clears this.</summary>
    public bool Modified { get; init; }

    /// <summary>The selection.</summary>
    public SelectionSummary Selection { get; init; } = new();

    /// <summary>The selection's character formatting.</summary>
    public CharacterFormatting Formatting { get; init; } = new();

    /// <summary>The selected paragraphs' formatting.</summary>
    public ParagraphFormatting Paragraph { get; init; } = new();

    /// <summary>The selection's section page layout.</summary>
    public PageFormatting Page { get; init; } = new();

    /// <summary>Every command's availability, checked state and value.</summary>
    public IReadOnlyDictionary<CommandId, CommandState> Commands { get; init; } = new Dictionary<CommandId, CommandState>();

    /// <summary>Each dialog's availability: whether <see cref="EditorDialogs"/> opens it now.</summary>
    public IReadOnlyDictionary<DialogKind, bool> Dialogs { get; init; } = new Dictionary<DialogKind, bool>();

    /// <summary>Undo and redo availability.</summary>
    public HistoryState History { get; init; } = new();

    /// <summary>
    /// The pages laid out so far and the caret's page. Background layout adds pages until
    /// <see cref="PageCounts.Complete"/>; each change is a new state with the same revision.
    /// </summary>
    public PageCounts Pages { get; init; } = new();

    /// <summary>View settings.</summary>
    public ViewState View { get; init; } = new();

    /// <summary>Change tracking, revisions and comments.</summary>
    public ReviewState Review { get; init; } = new();

    /// <summary>Spell checking.</summary>
    public SpellingState Spelling { get; init; } = new();

    /// <summary>Fonts still loading or not found.</summary>
    public FontStatus Fonts { get; init; } = new();

    /// <summary>The ruler model for the selection, in points.</summary>
    public RulerModel Ruler { get; init; } = new();

    /// <summary>Whether formatting is held for <see cref="EditorCommands.PasteFormattingAsync"/>.</summary>
    public bool FormatPainter { get; init; }

    /// <summary>The document's 12 theme colours in <see cref="ThemeColor"/> order, for an application's colour picker.</summary>
    [JsonPropertyName("themeColours")]
    public IReadOnlyList<ColorValue> ThemeColors { get; init; } = [];

    /// <summary>What typing corrects; change it with <see cref="EditorCommands.SetAutoCorrectAsync"/>.</summary>
    public AutoCorrectOptions AutoCorrect { get; init; } = new();
}

/// <summary>Whether a document is ready.</summary>
public enum LoadStatus
{
    /// <summary>A document is being read.</summary>
    Opening,
    /// <summary>Ready.</summary>
    Ready,
    /// <summary>Laid out in the background; page counts may still change.</summary>
    LayingOut,
}

/// <summary>A command's state.</summary>
/// <param name="Enabled">Whether it can run now.</param>
/// <param name="Checked">Whether a toggle shows as pressed.</param>
/// <param name="Value">
/// The command's state value, where it has one: a <see cref="bool"/>, a <see cref="double"/>, a <see cref="string"/> (an
/// undo label or an enum value's wire name), a <see cref="System.Text.Json.JsonElement"/> in the wire shape for a
/// structured value (such as <see cref="CommandId.SetTablePosition"/>'s <see cref="TablePosition"/>, absent for a table
/// in the text), or null.
/// </param>
public sealed record CommandState(
    bool Enabled,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? Checked = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull), JsonConverter(typeof(ScalarConverter))] object? Value = null);

/// <summary>What the editor is on: the template, or one of its sub-templates.</summary>
public enum EditingTargetKind
{
    /// <summary>The document itself.</summary>
    Template,
    /// <summary>A sub-template's body.</summary>
    SubTemplate,
}

/// <summary>What the editor is on.</summary>
/// <param name="Kind">The template or a sub-template.</param>
public sealed record EditingTarget(EditingTargetKind Kind)
{
    /// <summary>The template.</summary>
    public static EditingTarget Template { get; } = new(EditingTargetKind.Template);

    /// <summary>The sub-template's name.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public string? Name { get; init; }

    /// <summary>The sub-template's building block category.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public string? Category { get; init; }
}

/// <summary>A fill as written: the shape's own (<see cref="Explicit"/>) or its style reference's (<see cref="FromStyle"/>).</summary>
public sealed record FillState
{
    /// <summary>The shape's own fill.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public FillValue? Explicit { get; init; }

    /// <summary>The style reference's fill (the theme's, with the reference's colour), when the shape has none of its own.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public FillValue? FromStyle { get; init; }
}

/// <summary>An outline as written: the shape's own (<see cref="Explicit"/>) or its style reference's (<see cref="FromStyle"/>).</summary>
public sealed record LineState
{
    /// <summary>The shape's own outline.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public LineValue? Explicit { get; init; }

    /// <summary>The style reference's outline, when the shape has none of its own.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public LineValue? FromStyle { get; init; }
}

/// <summary>An element around the selection.</summary>
/// <param name="Id">The element.</param>
/// <param name="Kind">What it is.</param>
/// <param name="Exact">Whether the selection is exactly the element.</param>
/// <param name="Code">A field's code.</param>
/// <param name="Tag">A control's tag.</param>
/// <param name="Title">A control's title.</param>
public sealed record ElementContext(
    ElementId Id, ElementKind Kind, bool Exact,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? Code = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? Tag = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? Title = null)
{
    /// <summary>A picture control's picture size (inline or floating).</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public PictureSize? Size { get; init; }
}

/// <summary>Paragraph indents.</summary>
public sealed record ParagraphIndents
{
    /// <summary>The left indent.</summary>
    public FormatValue<Length> Left { get; init; }

    /// <summary>The right indent.</summary>
    public FormatValue<Length> Right { get; init; }

    /// <summary>The first-line indent.</summary>
    public FormatValue<Length> FirstLine { get; init; }

    /// <summary>The hanging indent.</summary>
    public FormatValue<Length> Hanging { get; init; }
}

/// <summary>How paragraphs break across pages.</summary>
public sealed record ParagraphFlow
{
    /// <summary>Keep with the next paragraph.</summary>
    public FormatValue<bool> KeepWithNext { get; init; }

    /// <summary>Keep the lines on one page.</summary>
    public FormatValue<bool> KeepLinesTogether { get; init; }

    /// <summary>Start on a new page.</summary>
    public FormatValue<bool> PageBreakBefore { get; init; }

    /// <summary>Avoid single lines at a page's top or bottom.</summary>
    public FormatValue<bool> WidowControl { get; init; }
}

/// <summary>A section's text columns.</summary>
/// <param name="Count">The number of columns.</param>
/// <param name="Spacing">The space between them.</param>
/// <param name="Separator">Whether a line separates them.</param>
public sealed record PageColumns(int Count, Length Spacing, bool Separator);

/// <summary>The pages laid out so far and the caret's page.</summary>
public sealed record PageCounts
{
    /// <summary>The number of pages laid out so far: all of them once <see cref="Complete"/>.</summary>
    public int Count { get; init; } = 1;

    /// <summary>The caret's page, from 1.</summary>
    public int Current { get; init; } = 1;

    /// <summary>Whether background layout has finished, so <see cref="Count"/> is the document's page count.</summary>
    public bool Complete { get; init; } = true;
}

/// <summary>View settings.</summary>
public sealed record ViewState
{
    /// <summary>The zoom percentage, after any fit.</summary>
    public double Zoom { get; init; } = 100;

    /// <summary>Whether formatting marks show.</summary>
    public bool FormattingMarks { get; init; }

    /// <summary>Whether table gridlines show.</summary>
    public bool Gridlines { get; init; }

    /// <summary>Whether text boundaries show.</summary>
    public bool TextBoundaries { get; init; }

    /// <summary>Whether text wraps around an object again as it is dragged; otherwise a preview shows until the release.</summary>
    public bool LiveLayout { get; init; } = true;

    /// <summary>Whether field codes show instead of results.</summary>
    public bool FieldCodes { get; init; }

    /// <summary>Whether the horizontal ruler shows.</summary>
    [JsonIgnore]
    public bool HorizontalRuler { get; init; }

    /// <summary>Whether the vertical ruler shows.</summary>
    [JsonIgnore]
    public bool VerticalRuler { get; init; }

    [JsonInclude, JsonPropertyName("rulers")]
    internal RulerVisibility Rulers
    {
        get => new(HorizontalRuler, VerticalRuler);
        init => (HorizontalRuler, VerticalRuler) = (value.Horizontal, value.Vertical);
    }

    /// <summary>The ruler unit.</summary>
    public RulerUnit RulerUnit { get; init; }

    /// <summary>What a click on the ruler adds.</summary>
    public TabSelector TabSelector { get; init; }

    /// <summary>The room beside the pages for comment balloons, in pixels.</summary>
    public double MarkupPaneWidth { get; init; }

    /// <summary>The background around the pages: the host's colour, or the theme's desk.</summary>
    public ColorValue Background { get; init; } = ColorValue.FromRgb(0xE8, 0xE7, 0xE3);

    /// <summary>The chrome's colour scheme.</summary>
    public ChromeTheme Theme { get; init; } = ChromeTheme.Light;

    /// <summary>The application's colours for the light and dark schemes.</summary>
    public ThemeColors ThemeColors { get; init; } = new();

    /// <summary>The chrome's accent as drawn (the dark scheme's is lightened): for popovers and other chrome that matches the editor's.</summary>
    public ColorValue Accent { get; init; } = ColorValue.FromRgb(0x0A, 0x74, 0xE4);

    /// <summary>Text and glyphs drawn on <see cref="Accent"/>.</summary>
    public ColorValue AccentOn { get; init; } = ColorValue.FromRgb(0xFF, 0xFF, 0xFF);

    /// <summary>How the page is drawn on screen.</summary>
    public DocumentAppearance Appearance { get; init; }

    /// <summary>How much the editor measures while you drag and hover.</summary>
    public MeasureDetail Detail { get; init; } = MeasureDetail.Standard;

    /// <summary>When the objects' dot grid shows.</summary>
    public ObjectGrid ObjectGrid { get; init; } = ObjectGrid.WhileDragging;

    /// <summary>Content controls and shown sub-templates are labelled while selected or hovered.</summary>
    public bool ControlLabels { get; init; } = true;

    /// <summary>When fields are shaded.</summary>
    public FieldShading FieldShading { get; init; } = FieldShading.WhenSelected;

    /// <summary>The pointer shapes shown over elements instead of the editor's own; none when null.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public IReadOnlyList<CursorOverride>? CursorOverrides { get; init; }
    /// <summary>Which fields are sub-template references (<see cref="EditorCommands.SetSubTemplateRulesAsync"/>); none when null.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public IReadOnlyList<ReferenceRule>? SubTemplateRules { get; init; }
}

/// <summary>
/// The document's fonts the library can't draw as the document asks: per family and style, those a resolver is
/// fetching and those nothing provided, and the characters no font has. Each list is sorted by family, then style.
/// </summary>
public sealed record FontStatus
{
    /// <summary>Families a resolver is fetching; they draw with a stand-in meanwhile.</summary>
    public IReadOnlyList<MissingFont> Pending { get; init; } = [];

    /// <summary>Families nothing provided; they draw with a stand-in.</summary>
    public IReadOnlyList<MissingFont> Missing { get; init; } = [];

    /// <summary>
    /// Characters that neither their run's font, the character-fallback families nor the symbol fallback has; they
    /// draw as their font's missing-glyph box.
    /// </summary>
    public IReadOnlyList<MissingCharacters> Characters { get; init; } = [];
}

/// <summary>A span on a ruler.</summary>
/// <param name="Start">The start.</param>
/// <param name="End">The end.</param>
public sealed record RulerSpan(Length Start, Length End);

/// <summary>A tab stop.</summary>
/// <param name="Position">The position from the paragraph's left text edge.</param>
/// <param name="Alignment">The alignment.</param>
/// <param name="Leader">The leader.</param>
/// <param name="Inherited">Whether it comes from the style.</param>
public sealed record TabStop(Length Position, TabAlignment Alignment, TabLeader Leader, bool Inherited);

/// <summary>The horizontal ruler model.</summary>
public sealed record HorizontalRulerModel
{
    /// <summary>The page width.</summary>
    public Length PageWidth { get; init; }

    /// <summary>The margins, or the cell or text column around the caret.</summary>
    public RulerSpan Text { get; init; } = new(Length.Zero, Length.Zero);

    /// <summary>The indents.</summary>
    public RulerIndents Indents { get; init; } = new(Length.Zero, Length.Zero, Length.Zero);

    /// <summary>The tab stops.</summary>
    public IReadOnlyList<TabStop> Tabs { get; init; } = [];

    /// <summary>The default tab spacing.</summary>
    public Length DefaultTabStop { get; init; }

    /// <summary>The section's text columns.</summary>
    public IReadOnlyList<RulerSpan> Columns { get; init; } = [];

    /// <summary>The cell boundaries of the caret's row.</summary>
    public IReadOnlyList<Length> TableColumns { get; init; } = [];
}

/// <summary>The vertical ruler model.</summary>
public sealed record VerticalRulerModel
{
    /// <summary>The page height.</summary>
    public Length PageHeight { get; init; }

    /// <summary>The top and bottom margins.</summary>
    public RulerSpan Text { get; init; } = new(Length.Zero, Length.Zero);

    /// <summary>The row boundaries of the caret's table on this page.</summary>
    public IReadOnlyList<Length> TableRows { get; init; } = [];
}
