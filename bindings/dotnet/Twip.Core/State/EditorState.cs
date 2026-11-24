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

    /// <summary>The cut, copy and paste options; change them with <see cref="EditorCommands.SetPasteOptionsAsync"/>.</summary>
    public PasteSettings PasteOptions { get; init; } = new(PasteMode.KeepSource, PasteMode.KeepSource, PasteMode.DestinationStyles, PasteMode.KeepSource, true, true, true, new(true, true, true, true, true, true, true));

    /// <summary>The caption inserted tables get (<see cref="EditorCommands.SetAutoCaptionAsync"/>); null when off.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public AutoCaption? AutoCaption { get; init; }

    /// <summary>The theme's heading and body font families.</summary>
    public ThemeFonts ThemeFonts { get; init; } = new("", "");
}

/// <summary>The theme's font families.</summary>
/// <param name="Heading">The heading font (<c>+Headings</c>).</param>
/// <param name="Body">The body font (<c>+Body</c>).</param>
public sealed record ThemeFonts(string Heading, string Body);

/// <summary>What typing corrects (AutoCorrect): each rule on or off (all on by default), and the texts never corrected again.</summary>
public sealed record AutoCorrectOptions
{
    /// <summary>Two initial capitals: <c>THe</c> becomes <c>The</c>.</summary>
    public bool TwoInitialCapitals { get; init; } = true;

    /// <summary>The first letter of a sentence is capitalised.</summary>
    public bool SentenceCapitals { get; init; } = true;

    /// <summary>The first letter of a table cell is capitalised.</summary>
    public bool CellCapitals { get; init; } = true;

    /// <summary>Day names are capitalised: <c>monday</c> becomes <c>Monday</c>.</summary>
    public bool DayNames { get; init; } = true;

    /// <summary>The replacement list and the symbols: <c>(c)</c> becomes <c>©</c>.</summary>
    public bool ReplaceText { get; init; } = true;

    /// <summary>Straight quotes become curly ones.</summary>
    public bool SmartQuotes { get; init; } = true;

    /// <summary>Ordinals get their suffix raised: <c>1st</c>.</summary>
    public bool Ordinals { get; init; } = true;

    /// <summary>Fractions: <c>1/2</c> becomes <c>½</c>.</summary>
    public bool Fractions { get; init; } = true;

    /// <summary>Hyphens become en and em dashes.</summary>
    public bool Dashes { get; init; } = true;

    /// <summary><c>1.</c> or <c>1)</c> then a space or tab at a paragraph's start makes it a numbered list.</summary>
    public bool NumberedLists { get; init; } = true;

    /// <summary><c>*</c> or <c>-</c> then a space or tab at a paragraph's start makes it a bulleted list.</summary>
    public bool BulletedLists { get; init; } = true;

    /// <summary>AutoFormat: three or more <c>-</c>, <c>_</c>, <c>=</c>, <c>*</c>, <c>~</c> or <c>#</c> then Enter make the paragraph a bottom border line.</summary>
    public bool BorderLines { get; init; } = true;

    /// <summary>AutoFormat: an Internet or network path then a space or Enter becomes a hyperlink.</summary>
    public bool InternetPaths { get; init; } = true;

    /// <summary>AutoFormat: Tab and Backspace at a paragraph's start set its indents.</summary>
    public bool TabIndents { get; init; } = true;

    /// <summary>The texts Never correct added: never corrected again.</summary>
    [JsonIgnore]
    public IReadOnlyList<string> Never { get; init; } = [];

    [JsonInclude, JsonPropertyName("never"), JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    internal IReadOnlyList<string>? NeverWire
    {
        get => Never.Count == 0 ? null : Never;
        init => Never = value ?? [];
    }
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
    /// <summary>A sub-template's body (a building block), while the editor is on it.</summary>
    SubTemplate,
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

    /// <summary>Every selected range of text in document order: one for a text selection, one per line for a column block, several for a discontiguous selection; empty for a caret, cells or an object. At most 1,000.</summary>
    public IReadOnlyList<TextRange> Ranges { get; init; } = [];

    /// <summary>The selection has more ranges than <see cref="Ranges"/> lists.</summary>
    public bool RangesTruncated { get; init; }

    /// <summary>The selected blocks of cells of a discontiguous selection, in the order they were added; the selection's own block is the last when <see cref="Kind"/> is cells. Empty when at most one block is selected (<see cref="Table"/> describes it).</summary>
    public IReadOnlyList<CellBlock> CellBlocks { get; init; } = [];

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

    /// <summary>Set while F2 (<see cref="EditorCommands.MoveTextAsync"/>) or Shift+F2 waits for its place: a click or the arrow keys choose it, Enter moves the text there, Esc gives up.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public PendingMove? PendingMove { get; init; }

    /// <summary>Set while a list's numbers are selected (a click on a number): character formatting then formats that list level's numbers; any selection move ends it.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public ListNumbers? ListNumbers { get; init; }
}

/// <summary>A list level whose numbers are selected.</summary>
/// <param name="List">The list: a <see cref="ListDefinitionInfo.Id"/>.</param>
/// <param name="Level">The level, 0 to 8.</param>
public sealed record ListNumbers(long List, int Level);

/// <summary>An F2 or Shift+F2 move waiting for its place.</summary>
/// <param name="Copy">Copies (Shift+F2) rather than moves.</param>
/// <param name="Placed">A place is chosen: Enter moves the text there.</param>
public sealed record PendingMove(bool Copy, bool Placed);

/// <summary>A hyperlink.</summary>
/// <param name="Address">The address.</param>
/// <param name="Text">The link text.</param>
public sealed record LinkInfo(string Address, string Text)
{
    /// <summary>The target without its bookmark: empty for a place in this document.</summary>
    public string Target { get; init; } = "";

    /// <summary>The place in the target it goes to (a bookmark name).</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public string? Bookmark { get; init; }

    /// <summary>The ScreenTip shown when it is hovered.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public string? ScreenTip { get; init; }

    /// <summary>The window or frame it opens in.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public string? Frame { get; init; }
}

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

    /// <summary>The cells' preferred width.</summary>
    public FormatValue<TableWidth> CellWidth { get; init; }

    /// <summary>The direction the cells' text runs in.</summary>
    public FormatValue<TextFlow> TextDirection { get; init; }

    /// <summary>The cells' text wraps.</summary>
    public FormatValue<bool> WrapText { get; init; }

    /// <summary>The cells' text is squeezed to fit.</summary>
    public FormatValue<bool> FitText { get; init; }

    /// <summary>The rows' end marks are hidden.</summary>
    public FormatValue<bool> RowHidden { get; init; }

    /// <summary>The space between the table's cells; null for none.</summary>
    public Length? CellSpacing { get; init; }

    /// <summary>The table's alternative text title.</summary>
    public string Title { get; init; } = "";

    /// <summary>The table's alternative text description.</summary>
    public string Description { get; init; } = "";

    /// <summary>The rows and columns in each band, as it lays out, and the table's own (null: its style's).</summary>
    public TableBands Bands { get; init; } = new(1, 1);

    /// <summary>The selected cells' borders, edge by edge: each cell's own line, else its row's or table's for that edge.</summary>
    public CellBorderLines CellBorders { get; init; } = new();

    /// <summary>The table's own borders.</summary>
    public TableBorderLines TableBorders { get; init; } = new();

    /// <summary>The selected cells' shading with its pattern (<see cref="CellShading"/> is its fill alone).</summary>
    public FormatValue<NoneOr<ShadingValue>> CellShadingValue { get; init; }

    /// <summary>The table's shading (behind spaced cells); null for none.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public ShadingValue? TableShading { get; init; }

    /// <summary>The selected cells' margins on each side.</summary>
    public CellMarginsState CellMargins { get; init; } = new();

    /// <summary>The table's default cell margins (Table Options).</summary>
    public TableCellMargins DefaultCellMargins { get; init; } = new();

    /// <summary>The selected cells' end marks take no height.</summary>
    public FormatValue<bool> HideMark { get; init; }

    /// <summary>The selected rows' exceptions to the table's properties.</summary>
    public RowExceptionsState RowExceptions { get; init; } = new();

    /// <summary>How the table's columns are sized.</summary>
    public TableFit Fit { get; init; }

    /// <summary>The columns the current cell spans.</summary>
    public int ColumnSpan { get; init; }

    /// <summary>The rows the current cell spans (more than one: merged down).</summary>
    public int RowSpan { get; init; }
}

/// <summary>How a table's columns are sized.</summary>
public enum TableFit
{
    /// <summary>Fixed column widths.</summary>
    Fixed,
    /// <summary>Columns fitted to their contents (AutoFit Contents).</summary>
    Contents,
    /// <summary>The table fills the width it is in (AutoFit Window).</summary>
    Window,
}

/// <summary>The selected cells' edges, each the same line over them, mixed, or none.</summary>
public sealed record CellBorderLines
{
    /// <summary>The selection's top edge.</summary>
    public FormatValue<NoneOr<CellBorder>> Top { get; init; }

    /// <summary>Its left edge.</summary>
    public FormatValue<NoneOr<CellBorder>> Left { get; init; }

    /// <summary>Its bottom edge.</summary>
    public FormatValue<NoneOr<CellBorder>> Bottom { get; init; }

    /// <summary>Its right edge.</summary>
    public FormatValue<NoneOr<CellBorder>> Right { get; init; }

    /// <summary>The lines between the selected rows (unavailable with one row).</summary>
    public FormatValue<NoneOr<CellBorder>> InsideHorizontal { get; init; }

    /// <summary>The lines between the selected columns (unavailable with one column).</summary>
    public FormatValue<NoneOr<CellBorder>> InsideVertical { get; init; }

    /// <summary>The diagonal from top left to bottom right.</summary>
    public FormatValue<NoneOr<CellBorder>> DiagonalDown { get; init; }

    /// <summary>The diagonal from bottom left to top right.</summary>
    public FormatValue<NoneOr<CellBorder>> DiagonalUp { get; init; }
}

/// <summary>A table's own borders; null for none.</summary>
public sealed record TableBorderLines
{
    /// <summary>The top edge.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public CellBorder? Top { get; init; }

    /// <summary>The left edge.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public CellBorder? Left { get; init; }

    /// <summary>The bottom edge.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public CellBorder? Bottom { get; init; }

    /// <summary>The right edge.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public CellBorder? Right { get; init; }

    /// <summary>The lines between rows.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public CellBorder? InsideHorizontal { get; init; }

    /// <summary>The lines between columns.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public CellBorder? InsideVertical { get; init; }
}

/// <summary>The selected cells' margins, each the same over them or mixed.</summary>
public sealed record CellMarginsState
{
    /// <summary>The top margin.</summary>
    public FormatValue<Length> Top { get; init; }

    /// <summary>The left margin.</summary>
    public FormatValue<Length> Left { get; init; }

    /// <summary>The bottom margin.</summary>
    public FormatValue<Length> Bottom { get; init; }

    /// <summary>The right margin.</summary>
    public FormatValue<Length> Right { get; init; }
}

/// <summary>The selected rows' exceptions to their table's properties; none where a row takes the table's.</summary>
public sealed record RowExceptionsState
{
    /// <summary>The rows carry exceptions at all.</summary>
    public FormatValue<bool> Any { get; init; }

    /// <summary>The rows' own alignment.</summary>
    public FormatValue<NoneOr<TableAlignment>> Alignment { get; init; }

    /// <summary>The rows' own indent from the table's left edge.</summary>
    public FormatValue<NoneOr<Length>> Indent { get; init; }

    /// <summary>The rows' own space between cells.</summary>
    public FormatValue<NoneOr<Length>> CellSpacing { get; init; }

    /// <summary>The rows' own shading.</summary>
    public FormatValue<NoneOr<ShadingValue>> Shading { get; init; }

    /// <summary>The rows' own borders.</summary>
    public FormatValue<NoneOr<TableBorderLines>> Borders { get; init; }
}

/// <summary>The rows and columns in each of a table's bands.</summary>
/// <param name="Rows">Rows in each band, as it lays out.</param>
/// <param name="Columns">Columns in each band, as it lays out.</param>
/// <param name="OwnRows">The table's own rows in each band; null: its style's.</param>
/// <param name="OwnColumns">The table's own columns in each band; null: its style's.</param>
public sealed record TableBands(
    int Rows,
    int Columns,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] int? OwnRows = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] int? OwnColumns = null);

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

/// <summary>What kind of object is selected.</summary>
public enum ObjectKind
{
    /// <summary>A picture.</summary>
    Picture,
    /// <summary>A text box.</summary>
    TextBox,
    /// <summary>A shape.</summary>
    Shape,
    /// <summary>A line or connector (a shape whose preset has no filled path).</summary>
    Line,
    /// <summary>A group of objects.</summary>
    Group,
    /// <summary>A frame: paragraphs placed on the page (a drop cap is one).</summary>
    Frame,
}

/// <summary>The selected object.</summary>
public sealed record ObjectSelection
{
    /// <summary>What kind of object.</summary>
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

    /// <summary>Where a floating object is and how text keeps from it; null for inline objects.</summary>
    public ObjectPlacement? Placement { get; init; }

    /// <summary>The shape's properties as written; null for objects without shape properties (frames) and kept graphics.</summary>
    public ShapeState? Shape { get; init; }

    /// <summary>Its rotation in degrees, clockwise.</summary>
    public double Rotation { get; init; }

    /// <summary>Flipped left to right.</summary>
    public bool FlipH { get; init; }

    /// <summary>Flipped top to bottom.</summary>
    public bool FlipV { get; init; }

    /// <summary>The selected child's place in its group; empty unless a group child is selected.</summary>
    public IReadOnlyList<int> ChildPath { get; init; } = [];

    /// <summary>How many objects are selected: 2 or more is a multi-selection, the other properties describing the first.</summary>
    public int Count { get; init; }

    /// <summary>A picture's crop, by fractions of its image from each edge; null when uncropped.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public PictureCrop? Crop { get; init; }

    /// <summary>A picture's Set Transparent Colour; null for none.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public ColorValue? TransparentColor { get; init; }

    /// <summary>A floating object's width as a share of a box on the page; null for an absolute width.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public RelativeSize? RelativeWidth { get; init; }

    /// <summary>Its height as a share of a box on the page; null for an absolute height.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public RelativeSize? RelativeHeight { get; init; }

    /// <summary>A floating object's id (a group child's when one is selected); null for inline objects and frames.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public ObjectId? Id { get; init; }

    /// <summary>Its name, as the Selection Pane lists it.</summary>
    public string Name { get; init; } = "";

    /// <summary>Its alternative text's title.</summary>
    public string Title { get; init; } = "";

    /// <summary>Hidden: not drawn, and text doesn't wrap around it.</summary>
    public bool Hidden { get; init; }

    /// <summary>Resizing it keeps its aspect ratio (Lock aspect ratio).</summary>
    public bool LockAspect { get; init; }

    /// <summary>A picture's image at its own size; null for other objects.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public PictureSize? OriginalSize { get; init; }

    /// <summary>Its place in the stack of its page's floating objects, 0 at the front; null for inline objects and frames.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public int? StackIndex { get; init; }

    /// <summary>A picture's adjustments; null for other objects.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public PictureAdjustState? PictureAdjust { get; init; }

    /// <summary>A text box's place in a chain of linked text boxes; null when it is in none.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public TextBoxLinkState? TextBoxLink { get; init; }

    /// <summary>A floating object's wrap polygon; null when it has none and its wrapping is neither tight nor through.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public WrapPolygonState? WrapPolygon { get; init; }

    /// <summary>A frame's placement and whether other frames and objects may overlap it; null for other objects.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public FrameState? Frame { get; init; }
}

/// <summary>A text box's place in a chain of linked text boxes.</summary>
/// <param name="Index">Its place in the chain, from 0.</param>
/// <param name="Count">How many boxes are in the chain.</param>
/// <param name="Previous">The box before it; null for the first.</param>
/// <param name="Next">The box after it; null for the last.</param>
public sealed record TextBoxLinkState(
    int Index,
    int Count,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] ObjectId? Previous = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] ObjectId? Next = null);

/// <summary>A floating object's wrap polygon.</summary>
/// <param name="Points">Its vertices, unclosed.</param>
/// <param name="Edited">Edited by hand: kept when the object changes.</param>
public sealed record WrapPolygonState(IReadOnlyList<BoxPoint> Points, bool Edited);

/// <summary>A frame's placement.</summary>
/// <param name="Properties">Its wrapping, size, position and distance from text.</param>
/// <param name="AllowOverlap">Other floating objects may overlap it.</param>
public sealed record FrameState(FrameProperties Properties, bool AllowOverlap);

/// <summary>A floating object as the Selection Pane lists it.</summary>
/// <param name="Object">The object.</param>
/// <param name="Name">Its name.</param>
/// <param name="Kind">What it is.</param>
/// <param name="Hidden">Hidden.</param>
/// <param name="Children">A group's objects, front first.</param>
public sealed record ObjectListEntry(
    ObjectId Object, string Name, ObjectKind Kind, bool Hidden,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] IReadOnlyList<ObjectListEntry>? Children = null);

/// <summary>The selected shape's properties, as written.</summary>
public sealed record ShapeState
{
    /// <summary>The preset geometry; null for a custom geometry.</summary>
    public ShapePreset? Preset { get; init; }

    /// <summary>The adjust values in effect, in the preset's order.</summary>
    public IReadOnlyList<long> Adjust { get; init; } = [];

    /// <summary>The fill.</summary>
    public FillState Fill { get; init; } = new();

    /// <summary>The outline.</summary>
    public LineState Line { get; init; } = new();

    /// <summary>The effects.</summary>
    public EffectsValue Effects { get; init; } = new();

    /// <summary>The text body; null for pictures and groups.</summary>
    public ShapeTextState? Text { get; init; }
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

/// <summary>A shape's text body as written.</summary>
public sealed record ShapeTextState
{
    /// <summary>The left, top, right and bottom insets.</summary>
    public IReadOnlyList<Length> Insets { get; init; } = [];

    /// <summary>Where the text sits.</summary>
    public TextAnchor Anchor { get; init; }

    /// <summary>How the shape fits it.</summary>
    public TextAutofit Autofit { get; init; }

    /// <summary>Lines wrap at the shape's width.</summary>
    public bool WrapText { get; init; }

    /// <summary>Which way it runs.</summary>
    public TextDirection Direction { get; init; }

    /// <summary>Its columns.</summary>
    public TextColumns Columns { get; init; } = new(1, default);

    /// <summary>The text stays upright when the shape turns ("Do not rotate text within shape").</summary>
    public bool Upright { get; init; }
}

/// <summary>A floating object's place and its wrapping options.</summary>
public sealed record ObjectPlacement
{
    /// <summary>The horizontal place: an alignment or an offset.</summary>
    public HorizontalPosition Horizontal { get; init; } = new(HorizontalReference.Column);

    /// <summary>The vertical place: an alignment or an offset.</summary>
    public VerticalPosition Vertical { get; init; } = new(VerticalReference.Paragraph);

    /// <summary>Whether other floating objects may overlap it.</summary>
    public bool AllowOverlap { get; init; }

    /// <summary>Whether, in a table cell, it is kept within the cell.</summary>
    public bool LayoutInCell { get; init; }

    /// <summary>The sides text wraps on.</summary>
    public WrapSide WrapSide { get; init; }

    /// <summary>The distance from the text.</summary>
    public TextDistance Distance { get; init; } = new();
}

/// <summary>How far text keeps from a floating object.</summary>
public sealed record TextDistance
{
    /// <summary>Above it.</summary>
    public Length Top { get; init; }

    /// <summary>Below it.</summary>
    public Length Bottom { get; init; }

    /// <summary>On its left.</summary>
    public Length Left { get; init; }

    /// <summary>On its right.</summary>
    public Length Right { get; init; }
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
    /// <summary>A building block gallery control.</summary>
    BuildingBlockGallery,
    /// <summary>A building block inserted from a gallery.</summary>
    BuildingBlock,
    /// <summary>A citation control.</summary>
    Citation,
    /// <summary>A bibliography control.</summary>
    Bibliography,
    /// <summary>An equation control.</summary>
    Equation,
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
    /// <summary>Locked against editing: a locked field, or a control whose content can't be edited.</summary>
    public bool Locked { get; init; }

    /// <summary>A field shows its code rather than its result.</summary>
    public bool ShowingCode { get; init; }

    /// <summary>A field's result is out of date: the next field update refreshes it.</summary>
    public bool Dirty { get; init; }

    /// <summary>A picture control's picture size (inline or floating).</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public PictureSize? Size { get; init; }

    /// <summary>A legacy form field's options and value.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public FormFieldInfo? Form { get; init; }

    /// <summary>A content control's look and options.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public ControlInfo? Control { get; init; }
}

/// <summary>A content control's look and options; what doesn't apply to its kind is null.</summary>
public sealed record ControlInfo
{
    /// <summary>How it shows its edges.</summary>
    public ControlAppearance Appearance { get; init; }
    /// <summary>Its colour; null for the default.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public ColorValue? Color { get; init; }
    /// <summary>It goes when its content is edited.</summary>
    public bool Temporary { get; init; }
    /// <summary>The character style text typed into it takes.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public StyleId? Style { get; init; }
    /// <summary>A checkbox's checked symbol.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public string? CheckedSymbol { get; init; }
    /// <summary>A checkbox's unchecked symbol.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public string? UncheckedSymbol { get; init; }
    /// <summary>What a repeating section's items are called.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public string? SectionTitle { get; init; }
    /// <summary>A repeating section's items can be added and removed.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public bool? AllowInsertDelete { get; init; }
    /// <summary>A building block gallery's entries: the package's building blocks in its category, by name.</summary>
    public IReadOnlyList<string> BuildingBlocks { get; init; } = [];
    /// <summary>A building block gallery's (or building block's) gallery; null for any.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public string? Gallery { get; init; }
    /// <summary>A building block gallery's (or building block's) category; null for any.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public string? Category { get; init; }
    /// <summary>A date control's date (<c>YYYY-MM-DDTHH:MM:SSZ</c>); null while it shows its placeholder or no date.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public string? Date { get; init; }
    /// <summary>A date control's calendar.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public CalendarType? Calendar { get; init; }
    /// <summary>What a date control stores in its data.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public DateStorage? StoreAs { get; init; }
}

/// <summary>A legacy form field's options and value; what doesn't apply to its kind is null.</summary>
public sealed record FormFieldInfo
{
    /// <summary>Its kind.</summary>
    public FormFieldType Kind { get; init; }
    /// <summary>Its bookmark name.</summary>
    public string Name { get; init; } = "";
    /// <summary>It can be filled in.</summary>
    public bool Enabled { get; init; }
    /// <summary>The document's fields update when it is left.</summary>
    public bool CalculateOnExit { get; init; }
    /// <summary>The help F1 shows.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public string? HelpText { get; init; }
    /// <summary>The help the status bar shows.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public string? StatusText { get; init; }
    /// <summary>What a text field takes.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public TextInputType? InputType { get; init; }
    /// <summary>A text field's default text.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public string? DefaultText { get; init; }
    /// <summary>The most characters a text field takes; 0 for any.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public int? MaxLength { get; init; }
    /// <summary>A check box is checked.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public bool? Checked { get; init; }
    /// <summary>A drop-down's entries.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public IReadOnlyList<string>? Entries { get; init; }
    /// <summary>The entry a drop-down shows.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public int? Entry { get; init; }
    /// <summary>The help text names an AutoText entry whose text is the help.</summary>
    public bool HelpIsAutoText { get; init; }
    /// <summary>The status text names an AutoText entry whose text is the help.</summary>
    public bool StatusIsAutoText { get; init; }
    /// <summary>The macro run on entry.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public string? EntryMacro { get; init; }
    /// <summary>The macro run on exit.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public string? ExitMacro { get; init; }
    /// <summary>A text field's text format.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public string? Format { get; init; }
    /// <summary>A check box's size, or its text's (<see cref="SpacingValue.Auto"/>).</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public SpacingValue? Size { get; init; }
    /// <summary>A check box is checked by default.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public bool? CheckedByDefault { get; init; }
    /// <summary>The entry a drop-down shows by default, from 0.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public int? DefaultEntry { get; init; }
}

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

    /// <summary>The character style; <see cref="StyleId.None"/> for none.</summary>
    public FormatValue<StyleId> CharacterStyle { get; init; }

    /// <summary>The horizontal scale, percent.</summary>
    public FormatValue<double> Scale { get; init; }

    /// <summary>Kerning at or above this size; 0 when off.</summary>
    public FormatValue<Length> Kerning { get; init; }

    /// <summary>The ligatures used (a combination Word's dialog doesn't offer reads as mixed).</summary>
    public FormatValue<Ligatures> Ligatures { get; init; }

    /// <summary>The digits' widths.</summary>
    public FormatValue<NumberSpacing> NumberSpacing { get; init; }

    /// <summary>The digits' forms.</summary>
    public FormatValue<NumberForm> NumberForm { get; init; }

    /// <summary>The stylistic set (1 to 20), or 0 for the font's default.</summary>
    public FormatValue<int> StylisticSet { get; init; }

    /// <summary>Contextual alternates.</summary>
    public FormatValue<bool> ContextualAlternates { get; init; }

    /// <summary>The language (a BCP 47 tag).</summary>
    public FormatValue<string> Language { get; init; }

    /// <summary>Spelling and grammar are not checked.</summary>
    public FormatValue<bool> NoProof { get; init; }

    /// <summary>Emboss.</summary>
    public FormatValue<bool> Emboss { get; init; }

    /// <summary>Engrave.</summary>
    public FormatValue<bool> Engrave { get; init; }

    /// <summary>Outline.</summary>
    public FormatValue<bool> Outline { get; init; }

    /// <summary>Shadow.</summary>
    public FormatValue<bool> Shadow { get; init; }

    /// <summary>The underline's colour, or the text's (<see cref="ColorValue.Auto"/>).</summary>
    public FormatValue<ColorValue> UnderlineColor { get; init; }

    /// <summary>The text effects (fill, outline, shadow, glow and reflection), through the character style.</summary>
    public FormatValue<TextEffects> TextEffects { get; init; }

    /// <summary>The theme font the font is, or none (a family by name).</summary>
    public FormatValue<NoneOr<ThemeFont>> FontTheme { get; init; }

    /// <summary>The colour as written: RGB, or a theme colour with its tint or shade (<see cref="ColorValue.Value"/>); <see cref="Color"/> is what it shows.</summary>
    [JsonPropertyName("colorValue")]
    public FormatValue<ColorValue> ColorAsWritten { get; init; }

    /// <summary>The underline's colour as written, or the text's (<see cref="ColorValue.Auto"/>).</summary>
    [JsonPropertyName("underlineColorValue")]
    public FormatValue<ColorValue> UnderlineColorAsWritten { get; init; }

    /// <summary>The text's border, with its distance and shadow, or none.</summary>
    public FormatValue<NoneOr<CellBorder>> TextBorder { get; init; }

    /// <summary>The text's shading, or none.</summary>
    public FormatValue<NoneOr<ShadingValue>> TextShading { get; init; }

    /// <summary>The width the text is fitted to (Fit Text), or none.</summary>
    public FormatValue<NoneOr<Length>> FitText { get; init; }

    /// <summary>Each kind of OpenType ligature on or off.</summary>
    public FormatValue<LigatureFlags> LigatureFlags { get; init; }

    /// <summary>The OpenType stylistic sets on (1 to 20), in order.</summary>
    public FormatValue<IReadOnlyList<int>> StylisticSets { get; init; }
}

/// <summary>Shading as the editor reads it back: a fill, and a pattern in the pattern colour over it.</summary>
/// <param name="Fill">The fill, or <see cref="ColorValue.None"/>.</param>
/// <param name="Pattern">The pattern.</param>
/// <param name="Color">The pattern's colour, or <see cref="ColorValue.Auto"/>.</param>
public sealed record ShadingValue(ColorValue Fill, ShadingPattern Pattern, ColorValue Color);

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

    /// <summary>No space between paragraphs of the same style.</summary>
    public FormatValue<bool> ContextualSpacing { get; init; }

    /// <summary>The outline level (1 to 9), or body text (0).</summary>
    public FormatValue<int> OutlineLevel { get; init; }

    /// <summary>Collapsed when the document opens.</summary>
    public FormatValue<bool> Collapsed { get; init; }

    /// <summary>The text direction.</summary>
    public FormatValue<TextFlow> TextDirection { get; init; }

    /// <summary>The selection's paragraph's drop cap, when it has one.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public DropCap? DropCap { get; init; }

    /// <summary>The top border, with its colour as written, distance and shadow, or none.</summary>
    public FormatValue<NoneOr<CellBorder>> BorderTop { get; init; }

    /// <summary>The left border.</summary>
    public FormatValue<NoneOr<CellBorder>> BorderLeft { get; init; }

    /// <summary>The bottom border.</summary>
    public FormatValue<NoneOr<CellBorder>> BorderBottom { get; init; }

    /// <summary>The right border.</summary>
    public FormatValue<NoneOr<CellBorder>> BorderRight { get; init; }

    /// <summary>The line between paragraphs with the same borders.</summary>
    public FormatValue<NoneOr<CellBorder>> BorderBetween { get; init; }

    /// <summary>The shading in full (<see cref="Shading"/> is its fill as shown), or none.</summary>
    [JsonPropertyName("shadingValue")]
    public FormatValue<NoneOr<ShadingValue>> ShadingAsWritten { get; init; }

    /// <summary>The tab stops, own and inherited, in position order: the same for every selected paragraph, or mixed.</summary>
    public FormatValue<IReadOnlyList<TabStop>> Tabs { get; init; }
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

    /// <summary>Mirror indents (inside and outside on facing pages).</summary>
    public FormatValue<bool> Mirror { get; init; }
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

    /// <summary>Don't hyphenate.</summary>
    public FormatValue<bool> DontHyphenate { get; init; }

    /// <summary>Left out of line numbering.</summary>
    public FormatValue<bool> SuppressLineNumbers { get; init; }

    /// <summary>In a text box, which lines text around it wraps tightly to.</summary>
    public FormatValue<TightWrap> TextboxTightWrap { get; init; }
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

    /// <summary>The page number format.</summary>
    public NumberFormat PageNumberFormat { get; init; } = NumberFormat.Decimal;

    /// <summary>The section's first page number, or null when it continues from the section before.</summary>
    public int? PageNumberStart { get; init; }

    /// <summary>Chapter numbers in page numbers, or null.</summary>
    public ChapterNumbers? ChapterNumbers { get; init; }

    /// <summary>Where text sits between the top and bottom margins.</summary>
    public PageVerticalAlignment VerticalAlignment { get; init; }

    /// <summary>The line numbering, or null.</summary>
    public LineNumbers? LineNumbers { get; init; }

    /// <summary>The break the section starts with.</summary>
    public SectionBreakKind BreakType { get; init; }

    /// <summary>The section's text direction.</summary>
    public TextFlow TextDirection { get; init; }

    /// <summary>How the document's pages pair.</summary>
    public MultiplePagesMode MultiplePages { get; init; }

    /// <summary>The gutter is at the top.</summary>
    public bool GutterAtTop { get; init; }

    /// <summary>Which of the section's headers take the section before's (Link to Previous).</summary>
    public HeaderFooterLinks HeaderLinks { get; init; } = new(false, false, false);

    /// <summary>Which of its footers do.</summary>
    public HeaderFooterLinks FooterLinks { get; init; } = new(false, false, false);

    /// <summary>The section's footnote settings.</summary>
    public NoteSettings Footnotes { get; init; } = new(NotePosition.PageBottom, NumberFormat.Decimal, 1, NoteRestart.Continuous);

    /// <summary>The section's endnote settings.</summary>
    public NoteSettings Endnotes { get; init; } = new(NotePosition.DocumentEnd, NumberFormat.LowerRoman, 1, NoteRestart.Continuous);

    /// <summary>The footnote area's columns; 0 for the section's own.</summary>
    public int FootnoteColumns { get; init; }

    /// <summary>The page colour, gradient, pattern or picture (its image empty), or null for none.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public FillValue? Background { get; init; }

    /// <summary>The watermark the pages show (a picture's image empty), or null.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public Watermark? Watermark { get; init; }

    /// <summary>The section's page borders.</summary>
    public PageBorders PageBorders { get; init; } = new();
}

/// <summary>A section's page borders.</summary>
public sealed record PageBorders
{
    /// <summary>The top edge, if any; its distance is from the page's edge or the text, as <see cref="OffsetFrom"/> says.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public CellBorder? Top { get; init; }

    /// <summary>The left edge.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public CellBorder? Left { get; init; }

    /// <summary>The bottom edge.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public CellBorder? Bottom { get; init; }

    /// <summary>The right edge.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public CellBorder? Right { get; init; }

    /// <summary>What the distances are measured from.</summary>
    public PageBorderOffset OffsetFrom { get; init; }

    /// <summary>The pages they show on.</summary>
    public PageBorderDisplay Display { get; init; }

    /// <summary>They go around the header.</summary>
    public bool SurroundHeader { get; init; }

    /// <summary>They go around the footer.</summary>
    public bool SurroundFooter { get; init; }

    /// <summary>They are behind the text.</summary>
    public bool BehindText { get; init; }
}

/// <summary>A section's footnote or endnote settings.</summary>
/// <param name="Position">Where the notes go.</param>
/// <param name="Format">The number format.</param>
/// <param name="Start">The first number.</param>
/// <param name="Restart">When numbering starts again.</param>
public sealed record NoteSettings(NotePosition Position, NumberFormat Format, int Start, NoteRestart Restart);

/// <summary>Which of a section's headers or footers take the section before's.</summary>
/// <param name="Default">For every page.</param>
/// <param name="First">For the first page.</param>
/// <param name="Even">For even pages.</param>
public sealed record HeaderFooterLinks(bool Default, bool First, bool Even);

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
/// <param name="Widths">Each column's width and the space after it, when they differ.</param>
public sealed record PageColumns(int Count, Length Spacing, bool Separator, IReadOnlyList<ColumnWidth>? Widths = null)
{
    /// <summary>The columns are of equal width (<see cref="Widths"/> is then empty).</summary>
    public bool Equal { get; init; }
}

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

    /// <summary>Bookmarks are shown.</summary>
    public bool Bookmarks { get; init; }

    /// <summary>Tab characters are shown (or every mark is).</summary>
    public bool TabCharacters { get; init; }

    /// <summary>Spaces are shown.</summary>
    public bool Spaces { get; init; }

    /// <summary>Paragraph marks and breaks are shown.</summary>
    public bool ParagraphMarks { get; init; }

    /// <summary>Hidden text is shown.</summary>
    public bool HiddenText { get; init; }

    /// <summary>Optional hyphens are shown.</summary>
    public bool OptionalHyphens { get; init; }

    /// <summary>The selected floating object's anchor is shown.</summary>
    public bool ObjectAnchors { get; init; } = true;

    /// <summary>Click and Type is on.</summary>
    public bool ClickAndType { get; init; } = true;

    /// <summary>Selected text can be dragged and dropped.</summary>
    public bool TextDragAndDrop { get; init; } = true;

    /// <summary>The white space between pages is hidden. It opens as the document was saved.</summary>
    public bool HideWhiteSpace { get; init; }

    /// <summary>Under enforced protection, the regions the author may edit are highlighted.</summary>
    public bool ShowEditableRegions { get; init; } = true;

    /// <summary>The selected floating object's wrap points are being edited.</summary>
    public bool EditingWrapPoints { get; init; }

    /// <summary>Highlight All's highlights show (<see cref="EditorFind.HighlightAllAsync"/>); they go when the document changes.</summary>
    public bool FindHighlights { get; init; }

    /// <summary>The highlighter's colour while Highlight with nothing selected made the pointer one; null when off.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public HighlightColor? Highlighter { get; init; }

    /// <summary>The pointer shapes shown over elements instead of the editor's own; none when null.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public IReadOnlyList<CursorOverride>? CursorOverrides { get; init; }
    /// <summary>Which fields are sub-template references (<see cref="EditorCommands.SetSubTemplateRulesAsync"/>); none when null.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public IReadOnlyList<ReferenceRule>? SubTemplateRules { get; init; }

    /// <summary>Create Link's pointer is on: the next click on an empty text box links the selected one to it.</summary>
    public bool LinkingTextBox { get; init; }

    /// <summary>Select Objects is on.</summary>
    public bool SelectingObjects { get; init; }

    /// <summary>The Select Objects rectangle being dragged; null when none is.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public PageBox? ObjectMarquee { get; init; }

    /// <summary>The pointer's table tool.</summary>
    public TableTool TableTool { get; init; }

    /// <summary>The line the Draw Table pen draws.</summary>
    public CellBorder TablePen { get; init; } = new(LineStyle.Single, Length.FromPoints(0.5), ColorValue.Auto, Length.FromPoints(0), false);

    /// <summary>What the Draw Table pen's stroke being dragged would draw, for the host to show dashed in the pen's colour; null when none is dragged.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public TableToolFeedback? TableToolFeedback { get; init; }

    /// <summary>The border under the pointer the eraser would remove, for the host to highlight; null when there is none.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public PageSegment? TableEraserTarget { get; init; }
}

/// <summary>A box on a page, in points from its top left corner.</summary>
/// <param name="Page">The page, from 0.</param>
/// <param name="Rect">Left, top, right and bottom.</param>
public sealed record PageBox(int Page, IReadOnlyList<Length> Rect);

/// <summary>A line on a page, in points from its top left corner.</summary>
/// <param name="Page">The page, from 0.</param>
/// <param name="From">One end.</param>
/// <param name="To">The other end.</param>
public sealed record PageSegment(int Page, PagePoint From, PagePoint To);

/// <summary>What a Draw Table pen stroke would draw.</summary>
public enum TableStrokeKind
{
    /// <summary>A line across or down a table.</summary>
    Line,
    /// <summary>A cell's diagonal.</summary>
    Diagonal,
    /// <summary>A new table's outline: the rectangle from its top left to its bottom right corner.</summary>
    Outline,
}

/// <summary>The Draw Table pen's stroke being dragged: what it would draw, and where.</summary>
/// <param name="Page">The page, from 0.</param>
/// <param name="Kind">What it would draw.</param>
/// <param name="From">A line's or diagonal's start, an outline's top left corner.</param>
/// <param name="To">A line's or diagonal's end, an outline's bottom right corner.</param>
public sealed record TableToolFeedback(int Page, TableStrokeKind Kind, PagePoint From, PagePoint To);

/// <summary>Change tracking, revisions and comments.</summary>
public sealed record ReviewState
{
    /// <summary>Whether changes are tracked.</summary>
    public bool Tracking { get; init; }

    /// <summary>Whether document protection keeps tracking on.</summary>
    public bool TrackingLocked { get; init; }

    /// <summary>What the document's protection allows; null when it has none.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public ProtectionState? Protection { get; init; }

    /// <summary>How markup shows.</summary>
    public MarkupView MarkupView { get; init; }

    /// <summary>The number of revisions.</summary>
    public int Revisions { get; init; }

    /// <summary>The number of comment threads.</summary>
    public int Comments { get; init; }

    /// <summary>The highlighted thread.</summary>
    public CommentId? ActiveComment { get; init; }

    /// <summary>Which kinds of markup show, and whose. It opens as the document was saved.</summary>
    public ShowMarkup ShowMarkup { get; init; } = new(true, true, true, []);

    /// <summary>Where revisions show in All Markup.</summary>
    public BalloonMode BalloonMode { get; init; } = BalloonMode.CommentsAndFormatting;

    /// <summary>How revisions draw.</summary>
    public MarkupStyle? MarkupStyle { get; init; }

    /// <summary>Tracking records moves as moves.</summary>
    public bool TrackMoves { get; init; }

    /// <summary>Tracking records formatting changes.</summary>
    public bool TrackFormatting { get; init; }

    /// <summary>The revisions and threads by kind: all, and those Show Markup shows.</summary>
    public ReviewCounts Counts { get; init; } = new(new(0, 0, 0, 0, 0), new(0, 0, 0, 0, 0));
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

/// <summary>The ruler model for the selection, in points, with no viewport mapping.</summary>
public sealed record RulerModel
{
    /// <summary>Whether the selected paragraphs differ (the first one's values are shown).</summary>
    public bool Mixed { get; init; }

    /// <summary>The way the caret's lines run. Across the page, the horizontal ruler shows the indents and tab stops; down or up the page (vertical text), the vertical ruler shows them, <see cref="VerticalRulerModel.Text"/> is where the lines run, and the horizontal ruler shows the text columns across the page in the order the lines stack.</summary>
    public LineDirection Lines { get; init; }

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

    /// <summary>The gutter across the page, beside the margin it widens; null without one.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public RulerSpan? Gutter { get; init; }

    /// <summary>The margin the page's text starts after: left, or with mirrored margins inside or outside.</summary>
    public MarginSide StartMargin { get; init; }

    /// <summary>The margin the page's text ends before.</summary>
    public MarginSide EndMargin { get; init; }
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

    /// <summary>The gutter down the page (at the top); null without one.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public RulerSpan? Gutter { get; init; }

    /// <summary>The paragraph's indents when its lines run down or up the page, measured along them from the text's start (down) or end (up); zero otherwise.</summary>
    public RulerIndents Indents { get; init; } = new(Length.Zero, Length.Zero, Length.Zero);

    /// <summary>The paragraph's tab stops when its lines run down or up the page, along them as <see cref="Indents"/> are.</summary>
    public IReadOnlyList<TabStop> Tabs { get; init; } = [];

    /// <summary>The default tab spacing when the lines run down or up the page.</summary>
    public Length DefaultTabStop { get; init; }
}

/// <summary>A document's protection (Restrict Editing).</summary>
/// <param name="Kind">What it allows.</param>
/// <param name="Enforced">It is enforced.</param>
/// <param name="Formatting">Formatting is limited to the styles that aren't locked.</param>
/// <param name="Password">Stopping it needs a password.</param>
/// <param name="InEditableRange">The selection is in a range the author may edit.</param>
/// <param name="UnprotectedSections">Filling in forms: the sections (by index) anyone may edit.</param>
public sealed record ProtectionState(
    ProtectionKind Kind,
    bool Enforced,
    bool Formatting,
    bool Password,
    bool InEditableRange,
    IReadOnlyList<uint> UnprotectedSections)
{
    /// <summary>The editing restriction takes exceptions: editable ranges and who may edit them (read only, comments).</summary>
    public bool ExceptionsAllowed { get; init; }

    /// <summary>The editing restriction leaves chosen sections open (filling in forms).</summary>
    public bool SectionsAllowed { get; init; }

    /// <summary>AutoFormat as you type may override the formatting restriction.</summary>
    public bool AutoFormatOverride { get; init; }

    /// <summary>The formatting restriction keeps the theme from changing.</summary>
    public bool BlockThemeSwitching { get; init; }

    /// <summary>The formatting restriction keeps the style set from changing.</summary>
    public bool BlockStyleSetSwitching { get; init; }

    /// <summary>Who may edit the editable ranges holding the selection.</summary>
    public IReadOnlyList<RangeEditorRef> EditorsHere { get; init; } = [];

    /// <summary>The number of editable ranges.</summary>
    public int RangeCount { get; init; }
}
