using System.Text.Json.Serialization;
using Twip.Wire;

namespace Twip;

/// <summary>Where a new section starts.</summary>
public enum SectionBreakKind
{
    /// <summary>On the next page.</summary>
    NextPage,
    /// <summary>On the same page.</summary>
    Continuous,
    /// <summary>On the next even page.</summary>
    EvenPage,
    /// <summary>On the next odd page.</summary>
    OddPage,
}

/// <summary>A direction in the text.</summary>
public enum Direction
{
    /// <summary>Towards the start.</summary>
    Backward,
    /// <summary>Towards the end.</summary>
    Forward,
}

/// <summary>What one delete removes next to the caret.</summary>
public enum DeleteUnit
{
    /// <summary>One character.</summary>
    Character,
    /// <summary>One word.</summary>
    Word,
}

/// <summary>How to paste.</summary>
public enum PasteMode
{
    /// <summary>With formatting.</summary>
    Formatted,
    /// <summary>As plain text.</summary>
    Text,
}

/// <summary>A unit to extend the selection to.</summary>
public enum SelectionUnit
{
    /// <summary>The word.</summary>
    Word,
    /// <summary>The sentence.</summary>
    Sentence,
    /// <summary>The paragraph.</summary>
    Paragraph,
    /// <summary>The table cell.</summary>
    Cell,
    /// <summary>The table row.</summary>
    Row,
    /// <summary>The table column.</summary>
    Column,
    /// <summary>The table.</summary>
    Table,
}

/// <summary>A unit to move the selection by.</summary>
public enum MoveUnit
{
    /// <summary>A character.</summary>
    Character,
    /// <summary>A word.</summary>
    Word,
    /// <summary>A line.</summary>
    Line,
    /// <summary>A paragraph.</summary>
    Paragraph,
    /// <summary>To the start or end of the line.</summary>
    LineEdge,
    /// <summary>To the start or end of the document.</summary>
    Document,
    /// <summary>A screen.</summary>
    Screen,
    /// <summary>A page.</summary>
    Page,
    /// <summary>To the first or last cell of the row.</summary>
    RowEdge,
    /// <summary>To the first or last cell of the column.</summary>
    ColumnEdge,
}

/// <summary>Next or previous.</summary>
public enum NavigationDirection
{
    /// <summary>The next one.</summary>
    Next,
    /// <summary>The previous one.</summary>
    Previous,
}

/// <summary>How the selection extends.</summary>
public enum TextSelectionMode
{
    /// <summary>Normal selection.</summary>
    Normal,
    /// <summary>Movement extends the selection (F8).</summary>
    Extend,
    /// <summary>A rectangular block (Ctrl+Shift+F8).</summary>
    Block,
}

/// <summary>An underline style.</summary>
public enum UnderlineStyle
{
    /// <summary>No underline.</summary>
    None,
    /// <summary>A single line.</summary>
    Single,
    /// <summary>Words only.</summary>
    Words,
    /// <summary>A double line.</summary>
    Double,
    /// <summary>A thick line.</summary>
    Thick,
    /// <summary>A dotted line.</summary>
    Dotted,
    /// <summary>A dashed line.</summary>
    Dash,
    /// <summary>Dots and dashes.</summary>
    DotDash,
    /// <summary>A wavy line.</summary>
    Wave,
}

/// <summary>A highlight colour.</summary>
public enum HighlightColor
{
    /// <summary>No highlight.</summary>
    None,
    /// <summary>Yellow.</summary>
    Yellow,
    /// <summary>Bright green.</summary>
    BrightGreen,
    /// <summary>Turquoise.</summary>
    Turquoise,
    /// <summary>Pink.</summary>
    Pink,
    /// <summary>Blue.</summary>
    Blue,
    /// <summary>Red.</summary>
    Red,
    /// <summary>Dark blue.</summary>
    DarkBlue,
    /// <summary>Teal.</summary>
    Teal,
    /// <summary>Green.</summary>
    Green,
    /// <summary>Violet.</summary>
    Violet,
    /// <summary>Dark red.</summary>
    DarkRed,
    /// <summary>Dark yellow.</summary>
    DarkYellow,
    /// <summary>50% gray.</summary>
    Gray50,
    /// <summary>25% gray.</summary>
    Gray25,
    /// <summary>Black.</summary>
    Black,
}

/// <summary>A case change.</summary>
public enum CaseChange
{
    /// <summary>lowercase.</summary>
    Lower,
    /// <summary>UPPERCASE.</summary>
    Upper,
    /// <summary>Sentence case.</summary>
    Sentence,
    /// <summary>Title Case.</summary>
    Title,
    /// <summary>tOGGLE cASE.</summary>
    Toggle,
}

/// <summary>A paragraph alignment.</summary>
public enum ParagraphAlignment
{
    /// <summary>Left.</summary>
    Left,
    /// <summary>Centred.</summary>
    Center,
    /// <summary>Right.</summary>
    Right,
    /// <summary>Justified.</summary>
    Justify,
}

/// <summary>A list kind.</summary>
public enum ListKind
{
    /// <summary>Not a list.</summary>
    None,
    /// <summary>A bulleted list.</summary>
    Bullet,
    /// <summary>A numbered list.</summary>
    Numbered,
}

/// <summary>A list level change.</summary>
public enum LevelChange
{
    /// <summary>One level in.</summary>
    Increase,
    /// <summary>One level out.</summary>
    Decrease,
}

/// <summary>Paragraph borders.</summary>
public enum ParagraphBorders
{
    /// <summary>No borders.</summary>
    None,
    /// <summary>A box.</summary>
    Box,
    /// <summary>A bottom border.</summary>
    Bottom,
    /// <summary>Some other borders (reported only).</summary>
    Other,
}

/// <summary>A tab stop alignment.</summary>
public enum TabAlignment
{
    /// <summary>Left.</summary>
    Left,
    /// <summary>Centred.</summary>
    Center,
    /// <summary>Right.</summary>
    Right,
    /// <summary>On the decimal point.</summary>
    Decimal,
    /// <summary>A vertical bar.</summary>
    Bar,
}

/// <summary>A tab leader.</summary>
public enum TabLeader
{
    /// <summary>No leader.</summary>
    None,
    /// <summary>Dots.</summary>
    Dots,
    /// <summary>Dashes.</summary>
    Dashes,
    /// <summary>An underline.</summary>
    Underline,
    /// <summary>A heavy line.</summary>
    Heavy,
    /// <summary>Middle dots.</summary>
    MiddleDot,
}

/// <summary>A page orientation.</summary>
public enum PageOrientation
{
    /// <summary>Portrait.</summary>
    Portrait,
    /// <summary>Landscape.</summary>
    Landscape,
}

/// <summary>What a page number shows.</summary>
public enum PageNumberFormat
{
    /// <summary>The page number.</summary>
    Page,
    /// <summary>"Page X of Y".</summary>
    PageOfTotal,
    /// <summary>The page count.</summary>
    Total,
}

/// <summary>Where a page number goes.</summary>
public enum PageNumberPlacement
{
    /// <summary>At the caret.</summary>
    Caret,
    /// <summary>In the header.</summary>
    Header,
    /// <summary>In the footer.</summary>
    Footer,
}

/// <summary>A page number's alignment in a header or footer.</summary>
public enum PageNumberAlignment
{
    /// <summary>Left.</summary>
    Left,
    /// <summary>Centred.</summary>
    Center,
    /// <summary>Right.</summary>
    Right,
}

/// <summary>Where new rows go.</summary>
public enum RowPosition
{
    /// <summary>Above the selection.</summary>
    Above,
    /// <summary>Below the selection.</summary>
    Below,
}

/// <summary>Where new columns go.</summary>
public enum ColumnPosition
{
    /// <summary>Left of the selection.</summary>
    Left,
    /// <summary>Right of the selection.</summary>
    Right,
}

/// <summary>Which way deleted cells shift the rest.</summary>
public enum CellShift
{
    /// <summary>Cells below move up.</summary>
    Up,
    /// <summary>Cells to the right move left.</summary>
    Left,
}

/// <summary>Up or down.</summary>
public enum VerticalDirection
{
    /// <summary>Up.</summary>
    Up,
    /// <summary>Down.</summary>
    Down,
}

/// <summary>How columns fit.</summary>
public enum AutoFitBehavior
{
    /// <summary>To the contents.</summary>
    Contents,
    /// <summary>To the window.</summary>
    Window,
    /// <summary>Fixed widths.</summary>
    Fixed,
}

/// <summary>A cell's vertical alignment.</summary>
public enum CellAlignment
{
    /// <summary>Top.</summary>
    Top,
    /// <summary>Centred.</summary>
    Center,
    /// <summary>Bottom.</summary>
    Bottom,
}

/// <summary>Which cell edges a border applies to.</summary>
public enum BorderEdges
{
    /// <summary>All edges.</summary>
    All,
    /// <summary>The outside edges.</summary>
    Outside,
    /// <summary>The inside edges.</summary>
    Inside,
    /// <summary>The inside horizontal edges.</summary>
    InsideHorizontal,
    /// <summary>The inside vertical edges.</summary>
    InsideVertical,
    /// <summary>The top edge.</summary>
    Top,
    /// <summary>The bottom edge.</summary>
    Bottom,
    /// <summary>The left edge.</summary>
    Left,
    /// <summary>The right edge.</summary>
    Right,
    /// <summary>No edges: removes the borders.</summary>
    None,
}

/// <summary>A border line style.</summary>
public enum LineStyle
{
    /// <summary>A single line.</summary>
    Single,
    /// <summary>A double line.</summary>
    Double,
    /// <summary>A dotted line.</summary>
    Dotted,
    /// <summary>A dashed line.</summary>
    Dashed,
    /// <summary>A thick line.</summary>
    Thick,
}

/// <summary>A table's alignment on the page.</summary>
public enum TableAlignment
{
    /// <summary>Left.</summary>
    Left,
    /// <summary>Centred.</summary>
    Center,
    /// <summary>Right.</summary>
    Right,
}

/// <summary>How text wraps around an object.</summary>
public enum ObjectWrap
{
    /// <summary>In line with the text.</summary>
    Inline,
    /// <summary>Square.</summary>
    Square,
    /// <summary>Tight.</summary>
    Tight,
    /// <summary>Through.</summary>
    Through,
    /// <summary>Top and bottom.</summary>
    TopAndBottom,
    /// <summary>Behind the text.</summary>
    BehindText,
    /// <summary>In front of the text.</summary>
    InFrontOfText,
}

/// <summary>A content control kind.</summary>
public enum ControlKind
{
    /// <summary>Rich text.</summary>
    RichText,
    /// <summary>Plain text.</summary>
    PlainText,
    /// <summary>A picture.</summary>
    Picture,
    /// <summary>A checkbox.</summary>
    Checkbox,
    /// <summary>A date picker.</summary>
    Date,
    /// <summary>A drop-down list.</summary>
    DropDown,
    /// <summary>A combo box.</summary>
    ComboBox,
    /// <summary>A group.</summary>
    Group,
    /// <summary>A repeating section.</summary>
    RepeatingSection,
}

/// <summary>What a content control's lock prevents.</summary>
public enum ControlLock
{
    /// <summary>Nothing.</summary>
    None,
    /// <summary>Deleting the control.</summary>
    Control,
    /// <summary>Editing the content.</summary>
    Content,
    /// <summary>Both.</summary>
    Both,
}

/// <summary>Which element text a replacement applies to.</summary>
public enum ElementTextField
{
    /// <summary>Field codes.</summary>
    Code,
    /// <summary>Control tags.</summary>
    Tag,
    /// <summary>Control titles.</summary>
    Title,
}

/// <summary>Fields or controls.</summary>
public enum ElementCategory
{
    /// <summary>Fields.</summary>
    Field,
    /// <summary>Content controls.</summary>
    Control,
}

/// <summary>An element mark's severity.</summary>
public enum MarkSeverity
{
    /// <summary>An error.</summary>
    Error,
    /// <summary>A warning.</summary>
    Warning,
    /// <summary>Information.</summary>
    Info,
}

/// <summary>How revisions and comments show.</summary>
public enum MarkupView
{
    /// <summary>All markup.</summary>
    All,
    /// <summary>Simple markup.</summary>
    Simple,
    /// <summary>No markup: the document as if accepted.</summary>
    NoMarkup,
    /// <summary>The original, as if rejected.</summary>
    Original,
}

/// <summary>Indents to change; unset values are kept. <c>FirstLine</c> and <c>Hanging</c> are exclusive.</summary>
/// <param name="Left">The left indent.</param>
/// <param name="Right">The right indent.</param>
/// <param name="FirstLine">The first-line indent (at least 0).</param>
/// <param name="Hanging">The hanging indent (at least 0).</param>
public sealed record IndentOptions(
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] Length? Left = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] Length? Right = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] Length? FirstLine = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] Length? Hanging = null);

/// <summary>Paragraph flow settings to change; unset values are kept.</summary>
/// <param name="KeepWithNext">Keep with the next paragraph.</param>
/// <param name="KeepLinesTogether">Keep the lines on one page.</param>
/// <param name="PageBreakBefore">Start on a new page.</param>
/// <param name="WidowControl">Avoid single lines at a page's top or bottom.</param>
public sealed record ParagraphFlowOptions(
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? KeepWithNext = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? KeepLinesTogether = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? PageBreakBefore = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? WidowControl = null);

/// <summary>Page margins to change, 0 to 1584 pt; unset values are kept.</summary>
/// <param name="Top">The top margin.</param>
/// <param name="Bottom">The bottom margin.</param>
/// <param name="Left">The left margin.</param>
/// <param name="Right">The right margin.</param>
/// <param name="Gutter">The gutter.</param>
public sealed record MarginOptions(
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] Length? Top = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] Length? Bottom = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] Length? Left = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] Length? Right = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] Length? Gutter = null);

/// <summary>Cell margins to change, 0 to 1584 pt; unset values are kept.</summary>
/// <param name="Top">The top margin.</param>
/// <param name="Bottom">The bottom margin.</param>
/// <param name="Left">The left margin.</param>
/// <param name="Right">The right margin.</param>
public sealed record CellMarginOptions(
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] Length? Top = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] Length? Bottom = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] Length? Left = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] Length? Right = null);

/// <summary>Table style options to change; unset values are kept.</summary>
/// <param name="HeaderRow">Format the header row.</param>
/// <param name="TotalRow">Format the total row.</param>
/// <param name="FirstColumn">Format the first column.</param>
/// <param name="LastColumn">Format the last column.</param>
/// <param name="BandedRows">Band the rows.</param>
/// <param name="BandedColumns">Band the columns.</param>
public sealed record TableStyleOptionsUpdate(
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? HeaderRow = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? TotalRow = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? FirstColumn = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? LastColumn = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? BandedRows = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? BandedColumns = null);

/// <summary>A cell border line.</summary>
/// <param name="Style">The line style.</param>
/// <param name="Width">The width, 0.25 to 6 pt.</param>
/// <param name="Color">The colour, or <see cref="ColorValue.Auto"/>.</param>
public sealed record CellBorder(LineStyle Style, Length Width, ColorValue Color);

/// <summary>A field to insert.</summary>
/// <param name="Code">The field code, such as <c>MERGEFIELD Name</c>.</param>
/// <param name="Display">The display text.</param>
/// <param name="ShowCode">Show the code instead of the result.</param>
public sealed record FieldOptions(
    string Code,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? Display = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? ShowCode = null);

/// <summary>A computed field result.</summary>
/// <param name="Id">The field.</param>
/// <param name="Display">The result; a string converts to plain text.</param>
public sealed record FieldResult(ElementId Id, RichText Display);

/// <summary>A content control's data binding.</summary>
/// <param name="XPath">The XPath into the data part.</param>
/// <param name="StoreItemId">The data part's item id.</param>
/// <param name="PrefixMappings">The namespace prefix mappings.</param>
public sealed record DataBinding(
    [property: JsonPropertyName("xpath")] string XPath,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? StoreItemId = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? PrefixMappings = null);

/// <summary>An item of a drop-down list or combo box.</summary>
/// <param name="Text">The shown text.</param>
/// <param name="Value">The stored value.</param>
public sealed record ControlListItem(string Text, string Value);

/// <summary>Kind-specific content control options. Only the fields of the control's kind apply.</summary>
/// <param name="Checked">A checkbox's state.</param>
/// <param name="Format">A date's display format.</param>
/// <param name="Locale">A date's locale.</param>
/// <param name="Items">A drop-down list's or combo box's items.</param>
/// <param name="Multiline">Whether plain text allows line breaks.</param>
public sealed record ControlOptions(
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? Checked = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? Format = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? Locale = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] IReadOnlyList<ControlListItem>? Items = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? Multiline = null);

/// <summary>A content control to insert.</summary>
/// <param name="Kind">The kind.</param>
/// <param name="Tag">The tag.</param>
/// <param name="Title">The title.</param>
/// <param name="Lock">What the lock prevents.</param>
/// <param name="Placeholder">The placeholder text.</param>
/// <param name="Binding">The data binding.</param>
/// <param name="Options">Kind-specific options.</param>
/// <param name="WrapSelection">Wrap the selection instead of inserting an empty control.</param>
public sealed record InsertControlOptions(
    ControlKind Kind,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? Tag = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? Title = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] ControlLock? Lock = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? Placeholder = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] DataBinding? Binding = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] ControlOptions? Options = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? WrapSelection = null);

/// <summary>Changes to a content control; unset values are kept.</summary>
/// <param name="Id">The control.</param>
/// <param name="Tag">The new tag.</param>
/// <param name="Title">The new title.</param>
/// <param name="Lock">The new lock.</param>
/// <param name="Placeholder">The new placeholder text.</param>
/// <param name="Binding">The new data binding.</param>
/// <param name="Options">The new kind-specific options.</param>
/// <param name="RemoveBinding">Remove the data binding (ignored when <paramref name="Binding"/> is set).</param>
[JsonConverter(typeof(ControlChangesConverter))]
public sealed record ControlChanges(
    ElementId Id, string? Tag = null, string? Title = null, ControlLock? Lock = null, string? Placeholder = null,
    DataBinding? Binding = null, ControlOptions? Options = null, bool RemoveBinding = false);

/// <summary>What to fill a content control with: exactly one of text, a picture or a document.</summary>
public sealed record ControlContent
{
    private ControlContent() { }

    /// <summary>The text.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public string? Text { get; private init; }

    /// <summary>The picture bytes.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public ReadOnlyMemory<byte>? Image { get; private init; }

    /// <summary>The DOCX bytes.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public ReadOnlyMemory<byte>? Docx { get; private init; }

    /// <summary>Plain text.</summary>
    /// <param name="text">The text.</param>
    public static ControlContent FromText(string text) => new() { Text = text };

    /// <summary>A picture, fitted to the control's box.</summary>
    /// <param name="image">The image bytes.</param>
    public static ControlContent FromImage(ReadOnlyMemory<byte> image) => new() { Image = image };

    /// <summary>The body of a DOCX document.</summary>
    /// <param name="docx">The DOCX bytes.</param>
    public static ControlContent FromDocx(ReadOnlyMemory<byte> docx) => new() { Docx = docx };
}

/// <summary>A mark on an element: a wavy line with a tooltip.</summary>
/// <param name="Id">The element.</param>
/// <param name="Severity">The severity.</param>
/// <param name="Message">The tooltip text.</param>
public sealed record ElementMark(ElementId Id, MarkSeverity Severity, string Message);

/// <summary>Which placeholders a look applies to.</summary>
[JsonConverter(typeof(PlaceholderMatchConverter))]
public sealed record PlaceholderMatch
{
    private PlaceholderMatch() { }

    /// <summary>The element id, for <see cref="ById"/>.</summary>
    public ElementId? Id { get; private init; }

    /// <summary>The tag, for <see cref="ByTag"/>.</summary>
    public string? Tag { get; private init; }

    /// <summary>The tag prefix, for <see cref="ByPrefix"/>.</summary>
    public string? Prefix { get; private init; }

    /// <summary>Whether this is the default look.</summary>
    public bool IsDefault { get; private init; }

    /// <summary>Every placeholder without a more specific look.</summary>
    public static PlaceholderMatch Default { get; } = new() { IsDefault = true };

    /// <summary>One placeholder.</summary>
    /// <param name="id">The element.</param>
    public static PlaceholderMatch ById(ElementId id) => new() { Id = id };

    /// <summary>Placeholders with a tag.</summary>
    /// <param name="tag">The tag.</param>
    public static PlaceholderMatch ByTag(string tag) => new() { Tag = tag };

    /// <summary>Placeholders whose tag starts with a prefix.</summary>
    /// <param name="prefix">The prefix.</param>
    public static PlaceholderMatch ByPrefix(string prefix) => new() { Prefix = prefix };
}

/// <summary>How placeholders look: a fill, label and icon, or an image.</summary>
/// <param name="Fill">The fill colour.</param>
/// <param name="Label">The label.</param>
/// <param name="IconSvg">An SVG icon.</param>
/// <param name="Image">An image instead of the fill, label and icon.</param>
/// <param name="Mime">The image's MIME type.</param>
public sealed record PlaceholderAppearance(
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] ColorValue? Fill = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? Label = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? IconSvg = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] ReadOnlyMemory<byte>? Image = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? Mime = null);

/// <summary>A placeholder look and the placeholders it applies to.</summary>
/// <param name="Match">The placeholders.</param>
/// <param name="Look">The look.</param>
public sealed record PlaceholderLook(PlaceholderMatch Match, PlaceholderAppearance Look);
