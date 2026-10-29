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
/// undo label or an enum value's wire name), or null.
/// </param>
public sealed record CommandState(
    bool Enabled,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? Checked = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull), JsonConverter(typeof(ScalarConverter))] object? Value = null);

/// <summary>What kind of selection there is.</summary>
public enum SelectionKind
{
    /// <summary>A caret.</summary>
    Caret,
    /// <summary>A text range.</summary>
    Text,
    /// <summary>Table cells.</summary>
    Cells,
    /// <summary>A picture or text box.</summary>
    Object,
    /// <summary>A merge-designer element.</summary>
    Element,
}

/// <summary>Which story the selection is in.</summary>
public enum Story
{
    /// <summary>The body.</summary>
    Body,
    /// <summary>A header.</summary>
    Header,
    /// <summary>A footer.</summary>
    Footer,
    /// <summary>A footnote.</summary>
    Footnote,
    /// <summary>An endnote.</summary>
    Endnote,
    /// <summary>A text box.</summary>
    TextBox,
}

/// <summary>The selection.</summary>
public sealed record SelectionSummary
{
    /// <summary>What kind of selection.</summary>
    public SelectionKind Kind { get; init; }

    /// <summary>The story.</summary>
    public Story Story { get; init; }

    /// <summary>The page of the selection's focus.</summary>
    public int Page { get; init; } = 1;

    /// <summary>The selection mode.</summary>
    public TextSelectionMode Mode { get; init; }

    /// <summary>Whether nothing is selected (a caret).</summary>
    public bool Empty { get; init; } = true;

    /// <summary>
    /// The selection as plain text, paragraphs separated by line feeds (empty for a caret), or null when it is longer
    /// than 10,000 characters.
    /// </summary>
    public string? Text { get; init; } = "";

    /// <summary>The table, when the selection is in one.</summary>
    public TableSelection? Table { get; init; }

    /// <summary>The selected object.</summary>
    public ObjectSelection? Object { get; init; }

    /// <summary>The elements around the selection, innermost first.</summary>
    public IReadOnlyList<ElementContext> Context { get; init; } = [];

    /// <summary>The hyperlink at the selection.</summary>
    public LinkInfo? Link { get; init; }

    /// <summary>The comment thread at the caret.</summary>
    public CommentId? Comment { get; init; }

    /// <summary>The revision at the caret.</summary>
    public RevisionId? Revision { get; init; }

    /// <summary>The header or footer being edited.</summary>
    public HeaderFooterInfo? HeaderFooter { get; init; }
}

/// <summary>A hyperlink.</summary>
/// <param name="Address">The address.</param>
/// <param name="Text">The link text.</param>
public sealed record LinkInfo(string Address, string Text);

/// <summary>A header or a footer.</summary>
public enum HeaderFooterKind
{
    /// <summary>A header.</summary>
    Header,
    /// <summary>A footer.</summary>
    Footer,
}

/// <summary>Which pages a header or footer is for.</summary>
public enum HeaderFooterType
{
    /// <summary>Every page without a more specific one.</summary>
    Default,
    /// <summary>The first page.</summary>
    First,
    /// <summary>Even pages.</summary>
    Even,
}

/// <summary>The header or footer being edited.</summary>
/// <param name="Kind">Header or footer.</param>
/// <param name="Type">Which pages it is for.</param>
/// <param name="Section">The section index.</param>
/// <param name="Page">The page being edited.</param>
/// <param name="Label">A label such as "First Page Header -Section 2-".</param>
public sealed record HeaderFooterInfo(HeaderFooterKind Kind, HeaderFooterType Type, int Section, int Page, string Label);

/// <summary>The table the selection is in.</summary>
public sealed record TableSelection
{
    /// <summary>The number of rows.</summary>
    public int Rows { get; init; }

    /// <summary>The number of columns.</summary>
    public int Columns { get; init; }

    /// <summary>The current row, from 0.</summary>
    public int Row { get; init; }

    /// <summary>The current column, from 0.</summary>
    public int Column { get; init; }

    /// <summary>Whether the selected cells can merge.</summary>
    public bool CanMerge { get; init; }

    /// <summary>Whether the rows repeat as a header row.</summary>
    public FormatValue<bool> HeaderRow { get; init; }

    /// <summary>Whether the rows can break across pages.</summary>
    public FormatValue<bool> RowCanBreak { get; init; }

    /// <summary>The cells' vertical alignment.</summary>
    public FormatValue<CellAlignment> CellAlignment { get; init; }

    /// <summary>The cells' shading; <see cref="ColorValue.None"/> for none.</summary>
    public FormatValue<ColorValue> CellShading { get; init; }

    /// <summary>The rows' height.</summary>
    public FormatValue<RowHeight> RowHeight { get; init; }

    /// <summary>The columns' width.</summary>
    public FormatValue<Length> ColumnWidth { get; init; }

    /// <summary>The table style.</summary>
    public StyleId? Style { get; init; }

    /// <summary>Which parts of the table style apply.</summary>
    public TableStyleOptions StyleOptions { get; init; } = new();

    /// <summary>The table width.</summary>
    public TableWidth Width { get; init; } = TableWidth.Auto;

    /// <summary>The table's alignment.</summary>
    public TableAlignment Alignment { get; init; }

    /// <summary>The table's indent.</summary>
    public Length Indent { get; init; }
}

/// <summary>Which parts of a table style apply.</summary>
public sealed record TableStyleOptions
{
    /// <summary>The header row.</summary>
    public bool HeaderRow { get; init; }

    /// <summary>The total row.</summary>
    public bool TotalRow { get; init; }

    /// <summary>The first column.</summary>
    public bool FirstColumn { get; init; }

    /// <summary>The last column.</summary>
    public bool LastColumn { get; init; }

    /// <summary>Banded rows.</summary>
    public bool BandedRows { get; init; }

    /// <summary>Banded columns.</summary>
    public bool BandedColumns { get; init; }
}

/// <summary>A picture or a text box.</summary>
public enum ObjectKind
{
    /// <summary>A picture.</summary>
    Picture,
    /// <summary>A text box.</summary>
    TextBox,
}

/// <summary>The selected object.</summary>
public sealed record ObjectSelection
{
    /// <summary>Picture or text box.</summary>
    public ObjectKind Kind { get; init; }

    /// <summary>How text wraps around it.</summary>
    public ObjectWrap Wrap { get; init; }

    /// <summary>Its width.</summary>
    public Length Width { get; init; }

    /// <summary>Its height.</summary>
    public Length Height { get; init; }

    /// <summary>Its alternative text.</summary>
    public string AltText { get; init; } = "";

    /// <summary>Whether it is decorative.</summary>
    public bool Decorative { get; init; }

    /// <summary>Whether its anchor is locked; null for inline objects.</summary>
    public bool? AnchorLocked { get; init; }
}

/// <summary>What a merge-designer element is.</summary>
public enum ElementKind
{
    /// <summary>A field.</summary>
    Field,
    /// <summary>A rich text control.</summary>
    RichText,
    /// <summary>A plain text control.</summary>
    PlainText,
    /// <summary>A picture control.</summary>
    Picture,
    /// <summary>A checkbox control.</summary>
    Checkbox,
    /// <summary>A date control.</summary>
    Date,
    /// <summary>A drop-down list control.</summary>
    DropDown,
    /// <summary>A combo box control.</summary>
    ComboBox,
    /// <summary>A group control.</summary>
    Group,
    /// <summary>A repeating section.</summary>
    RepeatingSection,
    /// <summary>One item of a repeating section.</summary>
    RepeatingItem,
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
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? Title = null);

/// <summary>The selection's character formatting.</summary>
public sealed record CharacterFormatting
{
    /// <summary>The document's font family, never the substituted one.</summary>
    public FormatValue<string> Font { get; init; }

    /// <summary>The font size.</summary>
    public FormatValue<Length> Size { get; init; }

    /// <summary>Bold.</summary>
    public FormatValue<bool> Bold { get; init; }

    /// <summary>Italic.</summary>
    public FormatValue<bool> Italic { get; init; }

    /// <summary>The underline style.</summary>
    public FormatValue<UnderlineStyle> Underline { get; init; }

    /// <summary>Strikethrough.</summary>
    public FormatValue<bool> Strikethrough { get; init; }

    /// <summary>Double strikethrough.</summary>
    public FormatValue<bool> DoubleStrikethrough { get; init; }

    /// <summary>Subscript.</summary>
    public FormatValue<bool> Subscript { get; init; }

    /// <summary>Superscript.</summary>
    public FormatValue<bool> Superscript { get; init; }

    /// <summary>Small capitals.</summary>
    public FormatValue<bool> SmallCaps { get; init; }

    /// <summary>All capitals.</summary>
    public FormatValue<bool> AllCaps { get; init; }

    /// <summary>Hidden text.</summary>
    public FormatValue<bool> Hidden { get; init; }

    /// <summary>The text colour.</summary>
    public FormatValue<ColorValue> Color { get; init; }

    /// <summary>The highlight; <see cref="HighlightColor.None"/> for none.</summary>
    public FormatValue<HighlightColor> Highlight { get; init; }

    /// <summary>The character spacing.</summary>
    public FormatValue<Length> CharacterSpacing { get; init; }

    /// <summary>The raised or lowered position.</summary>
    public FormatValue<Length> Position { get; init; }
}

/// <summary>The selected paragraphs' formatting.</summary>
public sealed record ParagraphFormatting
{
    /// <summary>The paragraph style.</summary>
    public FormatValue<StyleId> Style { get; init; }

    /// <summary>The alignment.</summary>
    public FormatValue<ParagraphAlignment> Alignment { get; init; }

    /// <summary>The line spacing.</summary>
    public FormatValue<LineSpacing> LineSpacing { get; init; }

    /// <summary>The space before.</summary>
    public FormatValue<SpacingValue> SpaceBefore { get; init; }

    /// <summary>The space after.</summary>
    public FormatValue<SpacingValue> SpaceAfter { get; init; }

    /// <summary>The indents.</summary>
    public ParagraphIndents Indents { get; init; } = new();

    /// <summary>The list kind; <see cref="ListKind.None"/> outside a list.</summary>
    public FormatValue<ListKind> List { get; init; }

    /// <summary>The list level.</summary>
    public FormatValue<int> ListLevel { get; init; }

    /// <summary>How the paragraphs break across pages.</summary>
    public ParagraphFlow Flow { get; init; } = new();

    /// <summary>The border preset.</summary>
    public FormatValue<ParagraphBorders> Borders { get; init; }

    /// <summary>The shading; <see cref="ColorValue.None"/> for none.</summary>
    public FormatValue<ColorValue> Shading { get; init; }
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

/// <summary>The selection's section page layout.</summary>
public sealed record PageFormatting
{
    /// <summary>The paper size.</summary>
    public PageSize Size { get; init; } = new(Length.FromInches(8.5), Length.FromInches(11));

    /// <summary>The orientation.</summary>
    public PageOrientation Orientation { get; init; }

    /// <summary>The margins.</summary>
    public PageMargins Margins { get; init; } = new(Length.FromInches(1), Length.FromInches(1), Length.FromInches(1), Length.FromInches(1), Length.Zero);

    /// <summary>The text columns.</summary>
    public PageColumns Columns { get; init; } = new(1, Length.FromInches(0.5), false);

    /// <summary>The header's distance from the page top.</summary>
    public Length HeaderDistance { get; init; }

    /// <summary>The footer's distance from the page bottom.</summary>
    public Length FooterDistance { get; init; }

    /// <summary>Whether the first page has its own header and footer.</summary>
    public bool DifferentFirstPage { get; init; }

    /// <summary>Whether odd and even pages have their own headers and footers.</summary>
    public bool DifferentOddEven { get; init; }
}

/// <summary>A paper size.</summary>
/// <param name="Width">The width.</param>
/// <param name="Height">The height.</param>
public sealed record PageSize(Length Width, Length Height);

/// <summary>Page margins.</summary>
/// <param name="Top">Top.</param>
/// <param name="Bottom">Bottom.</param>
/// <param name="Left">Left.</param>
/// <param name="Right">Right.</param>
/// <param name="Gutter">The gutter.</param>
public sealed record PageMargins(Length Top, Length Bottom, Length Left, Length Right, Length Gutter);

/// <summary>A section's text columns.</summary>
/// <param name="Count">The number of columns.</param>
/// <param name="Spacing">The space between them.</param>
/// <param name="Separator">Whether a line separates them.</param>
public sealed record PageColumns(int Count, Length Spacing, bool Separator);

/// <summary>Undo and redo availability.</summary>
public sealed record HistoryState
{
    /// <summary>Whether there is something to undo.</summary>
    public bool CanUndo { get; init; }

    /// <summary>Whether there is something to redo.</summary>
    public bool CanRedo { get; init; }

    /// <summary>The label of the step undo would undo.</summary>
    public string? UndoLabel { get; init; }

    /// <summary>The label of the step redo would redo.</summary>
    public string? RedoLabel { get; init; }
}

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

/// <summary>A ruler measurement unit.</summary>
public enum RulerUnit
{
    /// <summary>Inches.</summary>
    Inches,
    /// <summary>Centimetres.</summary>
    Centimeters,
    /// <summary>Millimetres.</summary>
    Millimeters,
    /// <summary>Points.</summary>
    Points,
    /// <summary>Picas.</summary>
    Picas,
}

/// <summary>What a click on the ruler adds: a tab stop of an alignment, or an indent marker.</summary>
public enum TabSelector
{
    /// <summary>A left tab.</summary>
    Left,
    /// <summary>A centre tab.</summary>
    Center,
    /// <summary>A right tab.</summary>
    Right,
    /// <summary>A decimal tab.</summary>
    Decimal,
    /// <summary>A bar tab.</summary>
    Bar,
    /// <summary>The first-line indent.</summary>
    FirstLineIndent,
    /// <summary>The hanging indent.</summary>
    HangingIndent,
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

    /// <summary>The background around the pages.</summary>
    public ColorValue Background { get; init; } = ColorValue.FromRgb(0xE6, 0xE6, 0xE6);
}

/// <summary>Change tracking, revisions and comments.</summary>
public sealed record ReviewState
{
    /// <summary>Whether changes are tracked.</summary>
    public bool Tracking { get; init; }

    /// <summary>Whether document protection keeps tracking on.</summary>
    public bool TrackingLocked { get; init; }

    /// <summary>How markup shows.</summary>
    public MarkupView MarkupView { get; init; }

    /// <summary>The number of revisions.</summary>
    public int Revisions { get; init; }

    /// <summary>The number of comment threads.</summary>
    public int Comments { get; init; }

    /// <summary>The highlighted thread.</summary>
    public CommentId? ActiveComment { get; init; }
}

/// <summary>Spell checking.</summary>
public sealed record SpellingState
{
    /// <summary>Whether checking is on.</summary>
    public bool Enabled { get; init; }

    /// <summary>The default language.</summary>
    public string Language { get; init; } = "en-US";

    /// <summary>The number of misspellings.</summary>
    public int Misspellings { get; init; }
}

/// <summary>Fonts still loading or not found.</summary>
public sealed record FontStatus
{
    /// <summary>Families being resolved.</summary>
    public IReadOnlyList<string> Pending { get; init; } = [];

    /// <summary>Families that couldn't be found.</summary>
    public IReadOnlyList<string> Unresolved { get; init; } = [];
}

/// <summary>The ruler model for the selection, in points, with no viewport mapping.</summary>
public sealed record RulerModel
{
    /// <summary>Whether the selected paragraphs differ (the first one's values are shown).</summary>
    public bool Mixed { get; init; }

    /// <summary>The horizontal ruler.</summary>
    public HorizontalRulerModel Horizontal { get; init; } = new();

    /// <summary>The vertical ruler.</summary>
    public VerticalRulerModel Vertical { get; init; } = new();
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

/// <summary>Ruler indents.</summary>
/// <param name="Left">The left indent.</param>
/// <param name="Right">The right indent.</param>
/// <param name="FirstLine">The first-line indent, relative to the left; negative is hanging.</param>
public sealed record RulerIndents(Length Left, Length Right, Length FirstLine);

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
