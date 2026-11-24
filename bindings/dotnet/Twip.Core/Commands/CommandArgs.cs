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
    /// <summary>In the next column.</summary>
    NextColumn,
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
    /// <summary>As the paste options say for where it came from.</summary>
    Formatted,
    /// <summary>The clipboard's plain text as it is.</summary>
    Text,
    /// <summary>Keep Source Formatting.</summary>
    KeepSource,
    /// <summary>Merge Formatting: the look of the text it lands in, keeping emphasis applied to part of it.</summary>
    Merge,
    /// <summary>Use Destination Styles.</summary>
    DestinationStyles,
    /// <summary>Keep Text Only: list numbers and bullets as text when the options keep bullets.</summary>
    TextOnly,
    /// <summary>The clipboard's HTML.</summary>
    Html,
    /// <summary>The clipboard's picture, in line.</summary>
    Picture,
    /// <summary>Copied cells as a table nested in the cell.</summary>
    NestTable,
    /// <summary>Copied cells as rows above the caret's row, made like this table's.</summary>
    MergeTable,
    /// <summary>Copied cells as rows above the caret's row, as they were copied.</summary>
    InsertRows,
    /// <summary>Copied cells over the cells from the caret's, adding rows the table lacks.</summary>
    OverwriteCells,
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
    /// <summary>To the top or bottom line of the window (Ctrl+Alt+Page Up, Page Down).</summary>
    WindowEdge,
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
    /// <summary>A heavy dotted line.</summary>
    DottedHeavy,
    /// <summary>A heavy dashed line.</summary>
    DashedHeavy,
    /// <summary>A line of long dashes.</summary>
    DashLong,
    /// <summary>A heavy line of long dashes.</summary>
    DashLongHeavy,
    /// <summary>Heavy alternating dots and dashes.</summary>
    DashDotHeavy,
    /// <summary>A dash and two dots, repeated.</summary>
    DotDotDash,
    /// <summary>A heavy dash and two dots, repeated.</summary>
    DashDotDotHeavy,
    /// <summary>A heavy wavy line.</summary>
    WavyHeavy,
    /// <summary>Two wavy lines.</summary>
    WavyDouble,
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

/// <summary>The scale, kerning and OpenType features to set (the Font dialog's Advanced tab); unset values are kept.</summary>
/// <param name="Scale">The horizontal scale, percent (1 to 600).</param>
/// <param name="Kerning">Kerning for text at or above this size (0 to 1638 pt); 0 turns it off.</param>
/// <param name="Ligatures">Which ligatures the font may use.</param>
/// <param name="NumberSpacing">The digits' widths.</param>
/// <param name="NumberForm">The digits' forms.</param>
/// <param name="StylisticSet">The font's stylistic set (1 to 20), or its default (0).</param>
/// <param name="ContextualAlternates">The font's contextual alternates.</param>
/// <param name="LigatureFlags">Each kind of ligature on or off, over <paramref name="Ligatures"/>.</param>
/// <param name="StylisticSets">The stylistic sets on (each 1 to 20; empty: none), over <paramref name="StylisticSet"/>.</param>
public sealed record TypographyOptions(
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] double? Scale = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] Length? Kerning = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] Ligatures? Ligatures = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] NumberSpacing? NumberSpacing = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] NumberForm? NumberForm = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] int? StylisticSet = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? ContextualAlternates = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] LigatureFlags? LigatureFlags = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] IReadOnlyList<int>? StylisticSets = null);

/// <summary>OpenType ligatures, each kind on or off.</summary>
/// <param name="Standard">Standard ligatures (<c>liga</c>).</param>
/// <param name="Contextual">Contextual ligatures (<c>clig</c>).</param>
/// <param name="Historical">Historical ligatures (<c>hlig</c>).</param>
/// <param name="Discretionary">Discretionary ligatures (<c>dlig</c>).</param>
public sealed record LigatureFlags(bool Standard = false, bool Contextual = false, bool Historical = false, bool Discretionary = false);

/// <summary>Which floating objects a text wrapping break moves the text below.</summary>
public enum BreakClear
{
    /// <summary>Those on the left.</summary>
    Left,
    /// <summary>Those on the right.</summary>
    Right,
    /// <summary>Those on either side (Layout &gt; Breaks &gt; Text Wrapping).</summary>
    Both,
}

/// <summary>One of the theme's two fonts.</summary>
public enum ThemeFont
{
    /// <summary>The heading font (<c>+Headings</c>).</summary>
    Headings,
    /// <summary>The body font (<c>+Body</c>).</summary>
    Body,
}

/// <summary>A font: a family by name, or the theme's heading or body font, which follows the theme.</summary>
[JsonConverter(typeof(FontChoiceConverter))]
public sealed record FontChoice
{
    private FontChoice(string? name, ThemeFont? theme) => (Name, Theme) = (name, theme);

    /// <summary>The family, for a font by name.</summary>
    public string? Name { get; }

    /// <summary>The theme's font, for a theme font.</summary>
    public ThemeFont? Theme { get; }

    /// <summary>A family by name.</summary>
    /// <param name="name">The family (non-empty).</param>
    public static FontChoice Family(string name) => new(name ?? throw new ArgumentNullException(nameof(name)), null);

    /// <summary>The theme's heading or body font.</summary>
    /// <param name="theme">Which.</param>
    public static FontChoice FromTheme(ThemeFont theme) => new(null, theme);

    /// <summary>A family by name.</summary>
    /// <param name="name">The family.</param>
    public static implicit operator FontChoice(string name) => Family(name);

    /// <inheritdoc/>
    public override string ToString() => Name ?? $"+{Theme}";
}

/// <summary>The ligatures text may use (Word's choices).</summary>
public enum Ligatures
{
    /// <summary>None.</summary>
    None,
    /// <summary>Standard ligatures only.</summary>
    Standard,
    /// <summary>Standard and contextual.</summary>
    StandardContextual,
    /// <summary>Historical and discretionary.</summary>
    HistoricalDiscretional,
    /// <summary>Every kind.</summary>
    All,
}

/// <summary>The digits' widths.</summary>
public enum NumberSpacing
{
    /// <summary>The font's.</summary>
    Default,
    /// <summary>Each digit its own width.</summary>
    Proportional,
    /// <summary>All digits one width.</summary>
    Tabular,
}

/// <summary>The digits' forms.</summary>
public enum NumberForm
{
    /// <summary>The font's.</summary>
    Default,
    /// <summary>Lining figures, all at cap height.</summary>
    Lining,
    /// <summary>Old-style figures, with ascenders and descenders.</summary>
    OldStyle,
}

/// <summary>The text's language and proofing to set; unset values are kept.</summary>
/// <param name="Language">The language, a BCP 47 tag (<c>en-GB</c>).</param>
/// <param name="NoProof">Do not check spelling or grammar.</param>
public sealed record LanguageOptions(
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? Language = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? NoProof = null);

/// <summary>Character effects to set; unset values are kept. Emboss and engrave exclude each other.</summary>
/// <param name="Emboss">Raised relief.</param>
/// <param name="Engrave">Sunken relief.</param>
/// <param name="Outline">The letters' outlines only.</param>
/// <param name="Shadow">A shadow behind the letters.</param>
public sealed record CharacterEffectsOptions(
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? Emboss = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? Engrave = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? Outline = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? Shadow = null);

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

/// <summary>Where a page number goes: at the caret, in the header or footer, or in a page margin.</summary>
[JsonConverter(typeof(PageNumberPlacementConverter))]
public sealed record PageNumberPlacement
{
    private PageNumberPlacement(string place, MarginSide? side) => (Place, Side) = (place, side);

    internal string Place { get; }

    /// <summary>The margin, for <see cref="Margin"/>; null otherwise.</summary>
    public MarginSide? Side { get; }

    /// <summary>At the caret.</summary>
    public static PageNumberPlacement Caret { get; } = new("caret", null);

    /// <summary>In the header.</summary>
    public static PageNumberPlacement Header { get; } = new("header", null);

    /// <summary>In the footer.</summary>
    public static PageNumberPlacement Footer { get; } = new("footer", null);

    /// <summary>In a page margin (Page Margins): in a frame anchored in the header, as wide as the margin.</summary>
    /// <param name="side">Which margin.</param>
    public static PageNumberPlacement Margin(MarginSide side) => new("margin", side);

    internal static PageNumberPlacement? Of(string place) => place switch
    {
        "caret" => Caret,
        "header" => Header,
        "footer" => Footer,
        _ => null,
    };

    /// <inheritdoc/>
    public override string ToString() => Side is { } s ? $"margin {s}" : Place;
}

/// <summary>A page margin, or a facing page's inside or outside one.</summary>
public enum MarginSide
{
    /// <summary>The left side of every page.</summary>
    Left,
    /// <summary>The right side of every page.</summary>
    Right,
    /// <summary>By the binding: the left on odd pages, the right on even ones.</summary>
    Inside,
    /// <summary>Away from the binding.</summary>
    Outside,
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
    /// <summary>On the inside edge: the left on odd pages, the right on even ones (in a frame).</summary>
    Inside,
    /// <summary>On the outside edge (in a frame).</summary>
    Outside,
}

/// <summary>Where new rows go.</summary>
public enum RowPosition
{
    /// <summary>Above the selection.</summary>
    Above,
    /// <summary>Below the selection.</summary>
    Below,
}

/// <summary>What separates a table's cells in text.</summary>
public enum TextSeparator
{
    /// <summary>Paragraph marks: a cell for each paragraph.</summary>
    Paragraphs,
    /// <summary>Tab characters.</summary>
    Tabs,
    /// <summary>Commas.</summary>
    Commas,
    /// <summary>Another character, given with the command.</summary>
    Other,
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
    /// <summary>No edges: removes the borders, diagonals too.</summary>
    None,
    /// <summary>Each cell's diagonal from its top left to its bottom right corner (cells only).</summary>
    DiagonalDown,
    /// <summary>Each cell's diagonal from its top right to its bottom left corner (cells only).</summary>
    DiagonalUp,
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
    /// <summary>Dots and dashes.</summary>
    DotDash,
    /// <summary>Two dots and a dash.</summary>
    DotDotDash,
    /// <summary>Three lines.</summary>
    Triple,
    /// <summary>A thin line and a thick one.</summary>
    ThinThickSmallGap,
    /// <summary>A thick line and a thin one.</summary>
    ThickThinSmallGap,
    /// <summary>Thin, thick, thin.</summary>
    ThinThickThinSmallGap,
    /// <summary>Short dashes.</summary>
    DashSmallGap,
    /// <summary>Slanted dashes.</summary>
    DashDotStroked,
    /// <summary>A thin line and a thick one, a medium gap apart.</summary>
    ThinThickMediumGap,
    /// <summary>A thick line and a thin one, a medium gap apart.</summary>
    ThickThinMediumGap,
    /// <summary>Thin, thick, thin, a medium gap apart.</summary>
    ThinThickThinMediumGap,
    /// <summary>A thin line and a thick one, a large gap apart.</summary>
    ThinThickLargeGap,
    /// <summary>A thick line and a thin one, a large gap apart.</summary>
    ThickThinLargeGap,
    /// <summary>Thin, thick, thin, a large gap apart.</summary>
    ThinThickThinLargeGap,
    /// <summary>A wave.</summary>
    Wave,
    /// <summary>Two waves.</summary>
    DoubleWave,
    /// <summary>Raised (3-D).</summary>
    ThreeDEmboss,
    /// <summary>Sunken (3-D).</summary>
    ThreeDEngrave,
    /// <summary>Outset.</summary>
    Outset,
    /// <summary>Inset.</summary>
    Inset,
}

/// <summary>A list level's number style's kind: the named ones, and <see cref="Other"/>.</summary>
public enum NumberFormatKind
{
    /// <summary>1, 2, 3.</summary>
    Decimal,
    /// <summary>I, II, III.</summary>
    UpperRoman,
    /// <summary>i, ii, iii.</summary>
    LowerRoman,
    /// <summary>A, B, C.</summary>
    UpperLetter,
    /// <summary>a, b, c.</summary>
    LowerLetter,
    /// <summary>1st, 2nd, 3rd.</summary>
    Ordinal,
    /// <summary>One, Two, Three.</summary>
    CardinalText,
    /// <summary>First, Second, Third.</summary>
    OrdinalText,
    /// <summary>01, 02, 03.</summary>
    DecimalZero,
    /// <summary>A bullet.</summary>
    Bullet,
    /// <summary>No number.</summary>
    None,
    /// <summary>- 1 -, - 2 -, - 3 -.</summary>
    NumberInDash,
    /// <summary>1, ..., 9, A, B: hexadecimal.</summary>
    Hex,
    /// <summary>*, †, ‡, §, then doubled.</summary>
    Chicago,
    /// <summary>а, б, в.</summary>
    RussianLower,
    /// <summary>А, Б, В.</summary>
    RussianUpper,
    /// <summary>क, ख, ग.</summary>
    HindiVowels,
    /// <summary>अ, आ, इ.</summary>
    HindiConsonants,
    /// <summary>१, २, ३.</summary>
    HindiNumbers,
    /// <summary>ก, ข, ค.</summary>
    ThaiLetters,
    /// <summary>๑, ๒, ๓.</summary>
    ThaiNumbers,
    /// <summary>Another style, by its WordprocessingML name (<see cref="NumberFormat.Other"/>), numbered in decimal.</summary>
    Other,
}

/// <summary>
/// A list level's number style: a named one (<see cref="Decimal"/>, <see cref="Chicago"/>), or any other by its
/// WordprocessingML name (<see cref="Named"/>, 1 to 64 ASCII letters and digits).
/// </summary>
[JsonConverter(typeof(NumberFormatConverter))]
public sealed record NumberFormat
{
    private NumberFormat(NumberFormatKind kind, string? other) => (Kind, Other) = (kind, other);

    /// <summary>Which style.</summary>
    public NumberFormatKind Kind { get; }

    /// <summary>The style's name, for <see cref="NumberFormatKind.Other"/>; null for the named ones.</summary>
    public string? Other { get; }

    /// <summary>1, 2, 3.</summary>
    public static NumberFormat Decimal { get; } = new(NumberFormatKind.Decimal, null);

    /// <summary>I, II, III.</summary>
    public static NumberFormat UpperRoman { get; } = new(NumberFormatKind.UpperRoman, null);

    /// <summary>i, ii, iii.</summary>
    public static NumberFormat LowerRoman { get; } = new(NumberFormatKind.LowerRoman, null);

    /// <summary>A, B, C.</summary>
    public static NumberFormat UpperLetter { get; } = new(NumberFormatKind.UpperLetter, null);

    /// <summary>a, b, c.</summary>
    public static NumberFormat LowerLetter { get; } = new(NumberFormatKind.LowerLetter, null);

    /// <summary>1st, 2nd, 3rd.</summary>
    public static NumberFormat Ordinal { get; } = new(NumberFormatKind.Ordinal, null);

    /// <summary>One, Two, Three.</summary>
    public static NumberFormat CardinalText { get; } = new(NumberFormatKind.CardinalText, null);

    /// <summary>First, Second, Third.</summary>
    public static NumberFormat OrdinalText { get; } = new(NumberFormatKind.OrdinalText, null);

    /// <summary>01, 02, 03.</summary>
    public static NumberFormat DecimalZero { get; } = new(NumberFormatKind.DecimalZero, null);

    /// <summary>A bullet.</summary>
    public static NumberFormat Bullet { get; } = new(NumberFormatKind.Bullet, null);

    /// <summary>No number.</summary>
    public static NumberFormat None { get; } = new(NumberFormatKind.None, null);

    /// <summary>- 1 -, - 2 -, - 3 -.</summary>
    public static NumberFormat NumberInDash { get; } = new(NumberFormatKind.NumberInDash, null);

    /// <summary>1, ..., 9, A, B: hexadecimal.</summary>
    public static NumberFormat Hex { get; } = new(NumberFormatKind.Hex, null);

    /// <summary>*, †, ‡, §, then doubled.</summary>
    public static NumberFormat Chicago { get; } = new(NumberFormatKind.Chicago, null);

    /// <summary>а, б, в.</summary>
    public static NumberFormat RussianLower { get; } = new(NumberFormatKind.RussianLower, null);

    /// <summary>А, Б, В.</summary>
    public static NumberFormat RussianUpper { get; } = new(NumberFormatKind.RussianUpper, null);

    /// <summary>क, ख, ग.</summary>
    public static NumberFormat HindiVowels { get; } = new(NumberFormatKind.HindiVowels, null);

    /// <summary>अ, आ, इ.</summary>
    public static NumberFormat HindiConsonants { get; } = new(NumberFormatKind.HindiConsonants, null);

    /// <summary>१, २, ३.</summary>
    public static NumberFormat HindiNumbers { get; } = new(NumberFormatKind.HindiNumbers, null);

    /// <summary>ก, ข, ค.</summary>
    public static NumberFormat ThaiLetters { get; } = new(NumberFormatKind.ThaiLetters, null);

    /// <summary>๑, ๒, ๓.</summary>
    public static NumberFormat ThaiNumbers { get; } = new(NumberFormatKind.ThaiNumbers, null);

    /// <summary>A style by its WordprocessingML name: a named one when it names one, else <see cref="NumberFormatKind.Other"/>.</summary>
    /// <param name="name">The name, such as <c>aiueoFullWidth</c>.</param>
    public static NumberFormat Named(string name) =>
        Of(name) ?? new NumberFormat(NumberFormatKind.Other, name ?? throw new ArgumentNullException(nameof(name)));

    /// <summary>The named style of a kind; <see cref="NumberFormatKind.Other"/> needs <see cref="Named"/>.</summary>
    /// <param name="kind">The kind.</param>
    public static NumberFormat FromKind(NumberFormatKind kind) =>
        kind == NumberFormatKind.Other ? throw new ArgumentException("an other style needs its name", nameof(kind)) : All[(int)kind];

    private static readonly NumberFormat[] All =
    [
        Decimal,
        UpperRoman,
        LowerRoman,
        UpperLetter,
        LowerLetter,
        Ordinal,
        CardinalText,
        OrdinalText,
        DecimalZero,
        Bullet,
        None,
        NumberInDash,
        Hex,
        Chicago,
        RussianLower,
        RussianUpper,
        HindiVowels,
        HindiConsonants,
        HindiNumbers,
        ThaiLetters,
        ThaiNumbers,
    ];

    /// <summary>The named style a camelCase name is, or null.</summary>
    internal static NumberFormat? Of(string name) => Array.Find(All, f => f.Name == name);

    /// <summary>A named style's camelCase name, or the other style's name.</summary>
    internal string Name => Other ?? char.ToLowerInvariant(Kind.ToString()[0]) + Kind.ToString().Substring(1);

    /// <inheritdoc/>
    public override string ToString() => Name;
}

/// <summary>How a list level's number aligns.</summary>
public enum LevelAlignment
{
    /// <summary>Left.</summary>
    Left,
    /// <summary>Centered.</summary>
    Center,
    /// <summary>Right.</summary>
    Right,
}

/// <summary>What follows a list level's number.</summary>
public enum FollowNumber
{
    /// <summary>A tab.</summary>
    Tab,
    /// <summary>A space.</summary>
    Space,
    /// <summary>Nothing.</summary>
    Nothing,
}

/// <summary>What a list definition applies to.</summary>
public enum ListApplyTo
{
    /// <summary>The selection's whole list.</summary>
    WholeList,
    /// <summary>The list from the selection's first paragraph on.</summary>
    ThisPointForward,
    /// <summary>The selected paragraphs.</summary>
    CurrentParagraph,
}

/// <summary>How Set Numbering Value sets the number.</summary>
public enum NumberingMode
{
    /// <summary>Start a new list.</summary>
    StartNew,
    /// <summary>Continue from the previous list.</summary>
    Continue,
}

/// <summary>A list level: its number or bullet, and where it and the text start.</summary>
/// <param name="Format">The number style.</param>
/// <param name="Text">The number format, with %1 to %9 for the numbers of this level and those above it; a bullet's character.</param>
/// <param name="Start">The number it starts at.</param>
/// <param name="Alignment">How the number aligns.</param>
/// <param name="Follow">What follows the number.</param>
/// <param name="NumberPosition">Where the number is, from the margin.</param>
/// <param name="TextIndent">Where the text is, from the margin.</param>
/// <param name="Legal">Legal style numbering.</param>
/// <param name="Tab">The tab stop after the number; null uses the text indent.</param>
/// <param name="RestartAfter">The level after which it restarts (1 to 9), 0 never; null after the level above.</param>
/// <param name="LinkedStyle">The paragraph style linked to the level.</param>
/// <param name="Font">The number's or bullet's font.</param>
/// <param name="Picture">A picture bullet's image (PNG, JPEG or GIF).</param>
/// <param name="Bold">The number's own bold; null: the paragraph's. Setting a level, a null one is cleared, as the others below.</param>
/// <param name="Italic">The number's own italic.</param>
/// <param name="Underline">The number's own underline.</param>
/// <param name="Size">The number's own size, 1 to 1638 pt.</param>
/// <param name="Color">The number's own colour, or <see cref="ColorValue.Auto"/>.</param>
/// <param name="Highlight">The number's own highlight, or <see cref="HighlightColor.None"/>.</param>
public sealed record ListLevel(
    NumberFormat Format,
    string Text,
    int Start,
    LevelAlignment Alignment,
    FollowNumber Follow,
    Length NumberPosition,
    Length TextIndent,
    bool Legal,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] Length? Tab = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] int? RestartAfter = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] StyleId? LinkedStyle = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? Font = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] byte[]? Picture = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? Bold = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? Italic = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] UnderlineStyle? Underline = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] Length? Size = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] ColorValue? Color = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] HighlightColor? Highlight = null);

/// <summary>A list definition for <see cref="EditorCommands.SetListDefinitionAsync"/>: level k of Levels is level k; the levels after them are kept.</summary>
/// <param name="Levels">The levels, from the first.</param>
/// <param name="ApplyTo">What they apply to.</param>
/// <param name="Name">The list's name (the ListNum field list name); null keeps it.</param>
public sealed record ListDefinition(
    IReadOnlyList<ListLevel> Levels,
    ListApplyTo ApplyTo = ListApplyTo.WholeList,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? Name = null);

/// <summary>Set Numbering Value's options.</summary>
/// <param name="Mode">Start a new list, or continue the previous one.</param>
/// <param name="Value">The value to start at, or to skip to.</param>
/// <param name="Advance">Continuing: skip numbers to the value.</param>
public sealed record NumberingValueOptions(NumberingMode Mode, int Value, bool Advance = false);

/// <summary>Adjust List Indents' options.</summary>
/// <param name="NumberPosition">Where the number is.</param>
/// <param name="TextIndent">Where the text is.</param>
/// <param name="Follow">What follows the number.</param>
/// <param name="Tab">The tab stop after the number; null uses the text indent.</param>
public sealed record ListIndentOptions(
    Length NumberPosition,
    Length TextIndent,
    FollowNumber Follow,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] Length? Tab = null);

/// <summary>What shading applies to.</summary>
public enum ShadingTarget
{
    /// <summary>The selected paragraphs.</summary>
    Paragraph,
    /// <summary>The selected text.</summary>
    Text,
    /// <summary>The selected cells, or a table style part's cells.</summary>
    Cell,
    /// <summary>The selected tables: shows around their cells where they have cell spacing.</summary>
    Table,
}

/// <summary>A shading pattern (<c>ST_Shd</c>): none, the fill only, the pattern color only, a percentage of the pattern color over the fill, or stripes and crosses of it over the fill.</summary>
public enum ShadingPattern
{
    /// <summary>The fill only.</summary>
    Clear,
    /// <summary>No shading.</summary>
    Nil,
    /// <summary>The pattern color only.</summary>
    Solid,
    /// <summary>5% of the pattern color over the fill.</summary>
    Pct5,
    /// <summary>10% of the pattern color over the fill.</summary>
    Pct10,
    /// <summary>12.5% of the pattern color over the fill.</summary>
    Pct12,
    /// <summary>15% of the pattern color over the fill.</summary>
    Pct15,
    /// <summary>20% of the pattern color over the fill.</summary>
    Pct20,
    /// <summary>25% of the pattern color over the fill.</summary>
    Pct25,
    /// <summary>30% of the pattern color over the fill.</summary>
    Pct30,
    /// <summary>35% of the pattern color over the fill.</summary>
    Pct35,
    /// <summary>37.5% of the pattern color over the fill.</summary>
    Pct37,
    /// <summary>40% of the pattern color over the fill.</summary>
    Pct40,
    /// <summary>45% of the pattern color over the fill.</summary>
    Pct45,
    /// <summary>50% of the pattern color over the fill.</summary>
    Pct50,
    /// <summary>55% of the pattern color over the fill.</summary>
    Pct55,
    /// <summary>60% of the pattern color over the fill.</summary>
    Pct60,
    /// <summary>62.5% of the pattern color over the fill.</summary>
    Pct62,
    /// <summary>65% of the pattern color over the fill.</summary>
    Pct65,
    /// <summary>70% of the pattern color over the fill.</summary>
    Pct70,
    /// <summary>75% of the pattern color over the fill.</summary>
    Pct75,
    /// <summary>80% of the pattern color over the fill.</summary>
    Pct80,
    /// <summary>85% of the pattern color over the fill.</summary>
    Pct85,
    /// <summary>87.5% of the pattern color over the fill.</summary>
    Pct87,
    /// <summary>90% of the pattern color over the fill.</summary>
    Pct90,
    /// <summary>95% of the pattern color over the fill.</summary>
    Pct95,
    /// <summary>Horizontal stripe of the pattern color over the fill.</summary>
    HorzStripe,
    /// <summary>Vertical stripe of the pattern color over the fill.</summary>
    VertStripe,
    /// <summary>Reverse diagonal stripe of the pattern color over the fill.</summary>
    ReverseDiagStripe,
    /// <summary>Diag stripe of the pattern color over the fill.</summary>
    DiagStripe,
    /// <summary>Horizontal cross of the pattern color over the fill.</summary>
    HorzCross,
    /// <summary>Diag cross of the pattern color over the fill.</summary>
    DiagCross,
    /// <summary>Thin horizontal stripe of the pattern color over the fill.</summary>
    ThinHorzStripe,
    /// <summary>Thin vertical stripe of the pattern color over the fill.</summary>
    ThinVertStripe,
    /// <summary>Thin reverse diagonal stripe of the pattern color over the fill.</summary>
    ThinReverseDiagStripe,
    /// <summary>Thin diagonal stripe of the pattern color over the fill.</summary>
    ThinDiagStripe,
    /// <summary>Thin horizontal cross of the pattern color over the fill.</summary>
    ThinHorzCross,
    /// <summary>Thin diagonal cross of the pattern color over the fill.</summary>
    ThinDiagCross,
    /// <summary>A pattern the document names that isn't listed: reading gives it, and setting it keeps each target's own pattern.</summary>
    Other,
}

/// <summary>The text's border for <see cref="EditorCommands.SetTextBorderAsync"/>.</summary>
/// <param name="Border">The box's line; null removes it.</param>
/// <param name="Shadow">A shadow, when the line gives none of its own.</param>
public sealed record TextBorderOptions(CellBorder? Border, bool Shadow = false);

/// <summary>What page borders are measured from.</summary>
public enum PageBorderOffset
{
    /// <summary>The page's edge.</summary>
    Page,
    /// <summary>The text.</summary>
    Text,
}

/// <summary>The pages a section's borders show on.</summary>
public enum PageBorderDisplay
{
    /// <summary>Every page.</summary>
    AllPages,
    /// <summary>The section's first page.</summary>
    FirstPage,
    /// <summary>Every page but the first.</summary>
    NotFirstPage,
}

/// <summary>The sections page borders apply to.</summary>
public enum PageBorderScope
{
    /// <summary>Every section.</summary>
    WholeDocument,
    /// <summary>The selection's section.</summary>
    ThisSection,
}

/// <summary>Page borders for <see cref="EditorCommands.SetPageBordersAsync"/>: every edge is set, and null removes it.</summary>
/// <param name="Top">The top edge.</param>
/// <param name="Bottom">The bottom edge.</param>
/// <param name="Left">The left edge.</param>
/// <param name="Right">The right edge.</param>
/// <param name="Shadow">A shadow on the bottom and right edges.</param>
/// <param name="OffsetFrom">What the borders are measured from; null keeps it.</param>
/// <param name="Display">The pages they show on; null keeps it.</param>
/// <param name="ApplyTo">The sections they apply to.</param>
/// <param name="SurroundHeader">The borders go around the header (every section's); null keeps it.</param>
/// <param name="SurroundFooter">The borders go around the footer (every section's); null keeps it.</param>
/// <param name="BehindText">The borders are behind the text and objects they meet (Word's Always display in front, off); null keeps it.</param>
public sealed record PageBordersOptions(
    CellBorder? Top = null,
    CellBorder? Bottom = null,
    CellBorder? Left = null,
    CellBorder? Right = null,
    bool Shadow = false,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] PageBorderOffset? OffsetFrom = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] PageBorderDisplay? Display = null,
    PageBorderScope ApplyTo = PageBorderScope.ThisSection,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? SurroundHeader = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? SurroundFooter = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? BehindText = null);

/// <summary>A column's width and the space after it.</summary>
/// <param name="Width">The width.</param>
/// <param name="Spacing">The space after it (none after the last).</param>
public sealed record ColumnWidth(Length Width, Length Spacing);

/// <summary>Page numbering to change for <see cref="EditorCommands.SetPageNumberingAsync"/>; unset values are kept.</summary>
/// <param name="Format">The number format (decimal, Roman or letters).</param>
/// <param name="Start">The section's first page number, 0 to 32767.</param>
/// <param name="ContinueFromPrevious">Continue from the section before (ignored when <paramref name="Start"/> is set).</param>
/// <param name="Chapter">Chapter numbers before the page number.</param>
/// <param name="NoChapterNumbers">Remove the chapter numbers (ignored when <paramref name="Chapter"/> is set).</param>
/// <param name="ApplyTo">The sections it applies to; null: the selection's.</param>
[JsonConverter(typeof(PageNumberingOptionsConverter))]
public sealed record PageNumberingOptions(
    NumberFormat? Format = null, int? Start = null, bool ContinueFromPrevious = false, ChapterNumbers? Chapter = null, bool NoChapterNumbers = false,
    PageSetupScope? ApplyTo = null);

/// <summary>Chapter numbers in page numbers: the number of the last heading of a level, then a separator.</summary>
/// <param name="Level">The heading level whose number leads (1 to 9).</param>
/// <param name="Separator">Between the chapter and page numbers.</param>
public sealed record ChapterNumbers(int Level, ChapterSeparator Separator);

/// <summary>What separates a chapter number from a page number.</summary>
public enum ChapterSeparator
{
    /// <summary><c>-</c>.</summary>
    Hyphen,
    /// <summary><c>.</c>.</summary>
    Period,
    /// <summary><c>:</c>.</summary>
    Colon,
    /// <summary>An em dash.</summary>
    EmDash,
    /// <summary>An en dash.</summary>
    EnDash,
}

/// <summary>Where text sits between a section's top and bottom margins.</summary>
public enum PageVerticalAlignment
{
    /// <summary>At the top.</summary>
    Top,
    /// <summary>In the middle.</summary>
    Center,
    /// <summary>Spread from top to bottom.</summary>
    Justified,
    /// <summary>At the bottom.</summary>
    Bottom,
}

/// <summary>A section's line numbering.</summary>
/// <param name="Start">The first number (1 to 32767); 1 when null.</param>
/// <param name="CountBy">Shows every nth number (1 to 100); 1 when null.</param>
/// <param name="Distance">From the text (0 to 1584 pt); automatic when null.</param>
/// <param name="Restart">When numbering starts again; each page when null.</param>
public sealed record LineNumbers(
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] int? Start = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] int? CountBy = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] Length? Distance = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] LineNumberRestart? Restart = null);

/// <summary>When line numbering starts again.</summary>
public enum LineNumberRestart
{
    /// <summary>On each page.</summary>
    EachPage,
    /// <summary>In each section.</summary>
    EachSection,
    /// <summary>Never: it continues.</summary>
    Continuous,
}

/// <summary>Footnote or endnote.</summary>
public enum NoteType
{
    /// <summary>A footnote.</summary>
    Footnote,
    /// <summary>An endnote.</summary>
    Endnote,
}

/// <summary>Where notes go.</summary>
public enum NotePosition
{
    /// <summary>Footnotes at the bottom of the page.</summary>
    PageBottom,
    /// <summary>Footnotes right after the page's text.</summary>
    BeneathText,
    /// <summary>Endnotes at the end of each section.</summary>
    SectionEnd,
    /// <summary>Endnotes at the end of the document.</summary>
    DocumentEnd,
}

/// <summary>When note numbering starts again.</summary>
public enum NoteRestart
{
    /// <summary>Never: it continues.</summary>
    Continuous,
    /// <summary>In each section.</summary>
    EachSection,
    /// <summary>On each page (footnotes).</summary>
    EachPage,
}

/// <summary>Which sections note settings apply to.</summary>
public enum NoteScope
{
    /// <summary>Every section.</summary>
    WholeDocument,
    /// <summary>The caret section.</summary>
    ThisSection,
}

/// <summary>What <see cref="EditorCommands.ConvertNotesAsync"/> turns into what.</summary>
public enum NoteConversion
{
    /// <summary>Every footnote becomes an endnote.</summary>
    FootnotesToEndnotes,
    /// <summary>Every endnote becomes a footnote.</summary>
    EndnotesToFootnotes,
    /// <summary>Both at once.</summary>
    Swap,
}

/// <summary>Footnote or endnote settings to change; unset values are kept.</summary>
/// <param name="Kind">Footnotes' or endnotes'.</param>
/// <param name="Position">Where the notes go (footnotes: page bottom or beneath text; endnotes, the whole document's: section or document end).</param>
/// <param name="Format">The number format.</param>
/// <param name="Start">The first number (1 to 16383).</param>
/// <param name="Restart">When numbering starts again.</param>
/// <param name="ApplyTo">The caret section, or every section when null.</param>
/// <param name="Columns">Footnotes only: the footnote area's columns, 1 to 4, or the section's own (0).</param>
public sealed record NotePropertiesOptions(
    NoteType Kind,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] NotePosition? Position = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] NumberFormat? Format = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] int? Start = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] NoteRestart? Restart = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] NoteScope? ApplyTo = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] int? Columns = null);

/// <summary>How text aligns at an alignment tab.</summary>
public enum AlignmentTabKind
{
    /// <summary>Left, at the left edge.</summary>
    Left,
    /// <summary>Centred, in the middle.</summary>
    Center,
    /// <summary>Right, at the right edge.</summary>
    Right,
}

/// <summary>What an alignment tab's place is measured in.</summary>
public enum AlignmentTabBase
{
    /// <summary>The margins.</summary>
    Margin,
    /// <summary>The paragraph's indents.</summary>
    Indent,
}

/// <summary>How the document's pages pair.</summary>
public enum MultiplePagesMode
{
    /// <summary>Each page on its own.</summary>
    Normal,
    /// <summary>Facing pages: inside and outside margins.</summary>
    MirrorMargins,
    /// <summary>A booklet folded down the middle.</summary>
    BookFold,
    /// <summary>A booklet folded down the middle, read right to left.</summary>
    ReverseBookFold,
}

/// <summary>Multiple pages settings to change; unset values are kept.</summary>
/// <param name="Mode">How pages pair.</param>
/// <param name="GutterAtTop">The gutter is at the top rather than the left.</param>
public sealed record MultiplePagesOptions(
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] MultiplePagesMode? Mode = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? GutterAtTop = null);

/// <summary>The paragraph border lines for <see cref="EditorCommands.SetParagraphBorderLinesAsync"/>: every edge is set, and null removes it.</summary>
/// <param name="Top">The top edge.</param>
/// <param name="Bottom">The bottom edge.</param>
/// <param name="Left">The left edge.</param>
/// <param name="Right">The right edge.</param>
/// <param name="Between">The line between paragraphs with the same borders.</param>
/// <param name="Shadow">A shadow on the bottom and right edges whose line gives none of its own.</param>
public sealed record ParagraphBorderLines(
    CellBorder? Top = null,
    CellBorder? Bottom = null,
    CellBorder? Left = null,
    CellBorder? Right = null,
    CellBorder? Between = null,
    bool Shadow = false);

/// <summary>Shading for <see cref="EditorCommands.SetShadingAsync"/>.</summary>
/// <param name="Target">The paragraphs or the text.</param>
/// <param name="Fill">The fill; <see cref="ColorValue.None"/> with a clear pattern removes the shading.</param>
/// <param name="Pattern">The pattern.</param>
/// <param name="Color">The pattern's color; null is automatic.</param>
public sealed record ShadingOptions(
    ShadingTarget Target,
    ColorValue Fill,
    ShadingPattern Pattern = ShadingPattern.Clear,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] ColorValue? Color = null);

/// <summary>Where a drop cap goes.</summary>
public enum DropCapPosition
{
    /// <summary>No drop cap: the letter goes back in the text.</summary>
    None,
    /// <summary>Dropped into the text, which wraps beside it.</summary>
    Dropped,
    /// <summary>In the margin, beside the text.</summary>
    InMargin,
}

/// <summary>A paragraph's drop cap (the Drop Cap dialog).</summary>
/// <param name="Position">Where the letter goes.</param>
/// <param name="Font">The letter's font; null for the text's.</param>
/// <param name="Lines">How many lines it drops, 1 to 10; null for 3.</param>
/// <param name="Distance">How far the text keeps from it; null for none.</param>
public sealed record DropCap(
    DropCapPosition Position,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? Font = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] uint? Lines = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] Length? Distance = null);

/// <summary>Whether text wraps around a frame.</summary>
public enum FrameWrap
{
    /// <summary>Text stays above and below the frame.</summary>
    None,
    /// <summary>Text wraps around the frame.</summary>
    Around,
}

/// <summary>How a frame's width is set.</summary>
public enum FrameWidthRule
{
    /// <summary>As wide as its text.</summary>
    Auto,
    /// <summary>The width given.</summary>
    Exactly,
}

/// <summary>How a frame's height is set.</summary>
public enum FrameHeightRule
{
    /// <summary>As tall as its text.</summary>
    Auto,
    /// <summary>At least the height given.</summary>
    AtLeast,
    /// <summary>The height given.</summary>
    Exactly,
}

/// <summary>A frame's horizontal alignment.</summary>
public enum FrameHorizontalAlign
{
    /// <summary>Left.</summary>
    Left,
    /// <summary>Centered.</summary>
    Center,
    /// <summary>Right.</summary>
    Right,
    /// <summary>Inside.</summary>
    Inside,
    /// <summary>Outside.</summary>
    Outside,
}

/// <summary>A frame's vertical alignment.</summary>
public enum FrameVerticalAlign
{
    /// <summary>Top.</summary>
    Top,
    /// <summary>Centered.</summary>
    Center,
    /// <summary>Bottom.</summary>
    Bottom,
    /// <summary>Inside.</summary>
    Inside,
    /// <summary>Outside.</summary>
    Outside,
}

/// <summary>What a frame's horizontal position is relative to.</summary>
public enum FrameHorizontalRelative
{
    /// <summary>The page margins.</summary>
    Margin,
    /// <summary>The page.</summary>
    Page,
    /// <summary>The column.</summary>
    Column,
}

/// <summary>What a frame's vertical position is relative to.</summary>
public enum FrameVerticalRelative
{
    /// <summary>The page margins.</summary>
    Margin,
    /// <summary>The page.</summary>
    Page,
    /// <summary>The paragraph it is anchored to: it moves with the text.</summary>
    Paragraph,
}

/// <summary>A frame's wrapping, size, position and distance from text.</summary>
/// <param name="Wrap">Text wrapping.</param>
/// <param name="WidthRule">How the width is set.</param>
/// <param name="Width">The width, when exact.</param>
/// <param name="HeightRule">How the height is set.</param>
/// <param name="Height">The height, unless automatic.</param>
/// <param name="HorizontalAlign">The horizontal alignment, or null for <paramref name="Horizontal"/>.</param>
/// <param name="Horizontal">The horizontal position.</param>
/// <param name="HorizontalRelative">What the horizontal position is relative to.</param>
/// <param name="HorizontalDistance">The distance from text, left and right.</param>
/// <param name="VerticalAlign">The vertical alignment, or null for <paramref name="Vertical"/>.</param>
/// <param name="Vertical">The vertical position.</param>
/// <param name="VerticalRelative">What the vertical position is relative to.</param>
/// <param name="VerticalDistance">The distance from text, above and below.</param>
/// <param name="LockAnchor">The anchor stays with its paragraph.</param>
/// <param name="AllowOverlap">Other frames and floating objects may overlap it (false: kept apart); null keeps it. Set Frame only.</param>
public sealed record FrameProperties(
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
    bool LockAnchor,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? AllowOverlap = null);

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

/// <summary>What a floating object's horizontal position is measured from.</summary>
public enum HorizontalReference
{
    /// <summary>The column it is anchored in.</summary>
    Column,
    /// <summary>The area between the left and right margins.</summary>
    Margin,
    /// <summary>The page's edges.</summary>
    Page,
    /// <summary>The character its anchor is at.</summary>
    Character,
    /// <summary>The left margin's area.</summary>
    LeftMargin,
    /// <summary>The right margin's area.</summary>
    RightMargin,
    /// <summary>The inside margin's area (left on odd pages, right on even ones).</summary>
    InsideMargin,
    /// <summary>The outside margin's area.</summary>
    OutsideMargin,
}

/// <summary>What a floating object's vertical position is measured from.</summary>
public enum VerticalReference
{
    /// <summary>Its anchor paragraph's top: it moves with the text.</summary>
    Paragraph,
    /// <summary>Its anchor line's top: it moves with the text.</summary>
    Line,
    /// <summary>The area between the top and bottom margins.</summary>
    Margin,
    /// <summary>The page's edges: it stays where it is on the page.</summary>
    Page,
    /// <summary>The top margin's area.</summary>
    TopMargin,
    /// <summary>The bottom margin's area.</summary>
    BottomMargin,
    /// <summary>The inside margin's area.</summary>
    InsideMargin,
    /// <summary>The outside margin's area.</summary>
    OutsideMargin,
}

/// <summary>A floating object aligned across its horizontal reference.</summary>
public enum ObjectHorizontalAlignment
{
    /// <summary>At its left edge.</summary>
    Left,
    /// <summary>Centred.</summary>
    Center,
    /// <summary>At its right edge.</summary>
    Right,
    /// <summary>At its inside edge (left on odd pages).</summary>
    Inside,
    /// <summary>At its outside edge.</summary>
    Outside,
}

/// <summary>A floating object aligned across its vertical reference.</summary>
public enum ObjectVerticalAlignment
{
    /// <summary>At its top.</summary>
    Top,
    /// <summary>Centred.</summary>
    Center,
    /// <summary>At its bottom.</summary>
    Bottom,
    /// <summary>At its inside edge.</summary>
    Inside,
    /// <summary>At its outside edge.</summary>
    Outside,
}

/// <summary>Which sides of a floating object text wraps on.</summary>
public enum WrapSide
{
    /// <summary>Both sides.</summary>
    BothSides,
    /// <summary>Its left only.</summary>
    Left,
    /// <summary>Its right only.</summary>
    Right,
    /// <summary>Whichever side has more room.</summary>
    Largest,
}

/// <summary>
/// A floating object's horizontal place: aligned across its reference, or an offset from the reference's left edge.
/// Set with neither, the object stays where it is, measured from the new reference.
/// </summary>
/// <param name="RelativeTo">What it is measured from.</param>
/// <param name="Align">Aligned across the reference.</param>
/// <param name="Offset">Right of the reference's left edge, −1584 to 1584 pt.</param>
/// <param name="Percent">Right of the reference's left edge by a share of its width, −1000 to 1000 percent.</param>
public sealed record HorizontalPosition(
    HorizontalReference RelativeTo,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] ObjectHorizontalAlignment? Align = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] Length? Offset = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] double? Percent = null);

/// <summary>
/// A floating object's vertical place: aligned across its reference, or an offset below the reference's top. Set with
/// neither, the object stays where it is, measured from the new reference.
/// </summary>
/// <param name="RelativeTo">What it is measured from.</param>
/// <param name="Align">Aligned across the reference.</param>
/// <param name="Offset">Below the reference's top, −1584 to 1584 pt.</param>
/// <param name="Percent">Below the reference's top by a share of its height, −1000 to 1000 percent.</param>
public sealed record VerticalPosition(
    VerticalReference RelativeTo,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] ObjectVerticalAlignment? Align = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] Length? Offset = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] double? Percent = null);

/// <summary>A floating object's size as a share of a box on the page.</summary>
/// <param name="Percent">The share, 1 to 1000 percent; 0 (<see cref="Absolute"/>) makes the size absolute.</param>
/// <param name="RelativeTo">The box.</param>
[JsonConverter(typeof(RelativeSizeConverter))]
public sealed record RelativeSize(double Percent, RelativeSizeReference RelativeTo)
{
    /// <summary>No relative size: the object's size is its own.</summary>
    public static RelativeSize Absolute { get; } = new(0, RelativeSizeReference.Margin);
}

/// <summary>The box a relative size is a share of.</summary>
public enum RelativeSizeReference
{
    /// <summary>Between the margins.</summary>
    Margin,
    /// <summary>The page.</summary>
    Page,
    /// <summary>The left margin (widths).</summary>
    LeftMargin,
    /// <summary>The right margin (widths).</summary>
    RightMargin,
    /// <summary>The inside margin (widths).</summary>
    InsideMargin,
    /// <summary>The outside margin (widths).</summary>
    OutsideMargin,
    /// <summary>The top margin (heights).</summary>
    TopMargin,
    /// <summary>The bottom margin (heights).</summary>
    BottomMargin,
}

/// <summary>A picture's crop, by fractions of its image from each edge (0 to 1; negative pads it); unset values are kept.</summary>
/// <param name="Left">Cut from the left.</param>
/// <param name="Top">Cut from the top.</param>
/// <param name="Right">Cut from the right.</param>
/// <param name="Bottom">Cut from the bottom.</param>
public sealed record PictureCrop(
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] double? Left = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] double? Top = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] double? Right = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] double? Bottom = null);

/// <summary>How <see cref="EditorCommands.AlignObjectsAsync"/> lines objects up.</summary>
public enum ObjectAlign
{
    /// <summary>Left edges.</summary>
    Left,
    /// <summary>Centres, across.</summary>
    Center,
    /// <summary>Right edges.</summary>
    Right,
    /// <summary>Top edges.</summary>
    Top,
    /// <summary>Middles, down.</summary>
    Middle,
    /// <summary>Bottom edges.</summary>
    Bottom,
    /// <summary>Even gaps across.</summary>
    DistributeHorizontally,
    /// <summary>Even gaps down.</summary>
    DistributeVertically,
}

/// <summary>What <see cref="EditorCommands.AlignObjectsAsync"/> lines objects up in.</summary>
public enum AlignReference
{
    /// <summary>The page.</summary>
    Page,
    /// <summary>The page's margins.</summary>
    Margin,
    /// <summary>The box around the selected objects.</summary>
    SelectedObjects,
}

/// <summary>What a protected document allows.</summary>
public enum ProtectionKind
{
    /// <summary>Any change.</summary>
    None,
    /// <summary>No changes, but in editable ranges.</summary>
    ReadOnly,
    /// <summary>Comments, and changes in editable ranges.</summary>
    Comments,
    /// <summary>Changes, tracked.</summary>
    TrackedChanges,
    /// <summary>Filling in forms.</summary>
    Forms,
}

/// <summary>Restrict Editing settings to change; unset values are kept.</summary>
/// <param name="Kind">What the document allows.</param>
/// <param name="Formatting">Formatting is limited to the styles that aren't locked.</param>
/// <param name="Enforce">Starts (true) or stops enforcing it.</param>
/// <param name="Password">Starting: the password stopping will need (none when null or empty). Stopping: the password.</param>
/// <param name="UnprotectedSections">Filling in forms: the sections (by index) anyone may edit. Not while enforced.</param>
/// <param name="ConfirmPassword">Starting with a password: the password again; it must match.</param>
/// <param name="AutoFormatOverride">AutoFormat as you type may override the formatting restriction. Not while enforced.</param>
/// <param name="BlockThemeSwitching">The formatting restriction keeps the theme from changing. Not while enforced.</param>
/// <param name="BlockStyleSetSwitching">The formatting restriction keeps the style set from changing. Not while enforced.</param>
public sealed record DocumentProtection(
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] ProtectionKind? Kind = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? Formatting = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? Enforce = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? Password = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] IReadOnlyList<uint>? UnprotectedSections = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? ConfirmPassword = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? AutoFormatOverride = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? BlockThemeSwitching = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? BlockStyleSetSwitching = null);

/// <summary>A legacy form field's kind.</summary>
public enum FormFieldType
{
    /// <summary>A text form field (FORMTEXT).</summary>
    Text,
    /// <summary>A check box (FORMCHECKBOX).</summary>
    CheckBox,
    /// <summary>A drop-down list (FORMDROPDOWN).</summary>
    DropDown,
}

/// <summary>What a text form field takes.</summary>
public enum TextInputType
{
    /// <summary>Any text.</summary>
    Regular,
    /// <summary>A number.</summary>
    Number,
    /// <summary>A date.</summary>
    Date,
    /// <summary>Today's date.</summary>
    CurrentDate,
    /// <summary>The time now.</summary>
    CurrentTime,
    /// <summary>A calculation.</summary>
    Calculation,
}

/// <summary>A legacy form field's options to change; unset values are kept, and only the field's own kind's options apply.</summary>
/// <param name="Id">The field; the one at the selection when null.</param>
/// <param name="Name">Its bookmark name.</param>
/// <param name="Enabled">It can be filled in.</param>
/// <param name="CalculateOnExit">The document's fields update when it is left.</param>
/// <param name="EntryMacro">The macro run on entry.</param>
/// <param name="ExitMacro">The macro run on exit.</param>
/// <param name="HelpText">The help F1 shows.</param>
/// <param name="StatusText">The help the status bar shows.</param>
/// <param name="Text">A text form field's options.</param>
/// <param name="CheckBox">A check box's options.</param>
/// <param name="DropDown">A drop-down's options.</param>
/// <param name="HelpIsAutoText"><paramref name="HelpText"/> names an AutoText entry whose text is the help.</param>
/// <param name="StatusIsAutoText"><paramref name="StatusText"/> names an AutoText entry whose text is the help.</param>
public sealed record FormFieldChange(
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] ElementId? Id = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? Name = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? Enabled = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? CalculateOnExit = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? EntryMacro = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? ExitMacro = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? HelpText = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? StatusText = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] TextFormOptions? Text = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] CheckBoxFormOptions? CheckBox = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] DropDownFormOptions? DropDown = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? HelpIsAutoText = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? StatusIsAutoText = null);

/// <summary>A text form field's options.</summary>
/// <param name="InputType">What it takes.</param>
/// <param name="Default">Its default text.</param>
/// <param name="MaxLength">The most characters it takes; 0 for any.</param>
/// <param name="Format">Its text format.</param>
public sealed record TextFormOptions(
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] TextInputType? InputType = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? Default = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] int? MaxLength = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? Format = null);

/// <summary>A check box form field's options.</summary>
/// <param name="Size">Its size, 1 to 1584 pt, or its text's (<see cref="SpacingValue.Auto"/>).</param>
/// <param name="CheckedByDefault">Checked by default.</param>
public sealed record CheckBoxFormOptions(
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] SpacingValue? Size = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? CheckedByDefault = null);

/// <summary>A drop-down form field's options.</summary>
/// <param name="Entries">Its entries: 25 at most, each 1 to 50 characters.</param>
/// <param name="Default">The entry shown by default, from 0.</param>
public sealed record DropDownFormOptions(
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] IReadOnlyList<string>? Entries = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] int? Default = null);

/// <summary>A legacy form field's value to fill in.</summary>
/// <param name="Id">The field; the one at the selection when null.</param>
/// <param name="Text">A text field's text.</param>
/// <param name="Checked">A check box's state.</param>
/// <param name="Entry">A drop-down's entry, from 0.</param>
public sealed record FormFieldFill(
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] ElementId? Id = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? Text = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? Checked = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] int? Entry = null);

/// <summary>Document properties to change; unset values are kept, an empty text removes one, and <paramref name="Custom"/> replaces the custom properties.</summary>
/// <param name="Title">The title.</param>
/// <param name="Subject">The subject.</param>
/// <param name="Author">The author.</param>
/// <param name="Keywords">The keywords.</param>
/// <param name="Comments">The comments.</param>
/// <param name="Category">The category.</param>
/// <param name="Status">The status.</param>
/// <param name="LastModifiedBy">Who saved it last.</param>
/// <param name="Company">The company.</param>
/// <param name="Manager">The manager.</param>
/// <param name="HyperlinkBase">The base relative hyperlinks resolve against.</param>
/// <param name="Custom">The custom properties.</param>
public sealed record DocumentPropertiesChange(
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? Title = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? Subject = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? Author = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? Keywords = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? Comments = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? Category = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? Status = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? LastModifiedBy = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? Company = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? Manager = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? HyperlinkBase = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] IReadOnlyList<CustomProperty>? Custom = null);

/// <summary>A custom document property.</summary>
/// <param name="Name">Its name.</param>
/// <param name="Kind">What it holds.</param>
/// <param name="Value">Its value as text: a decimal number, <c>true</c> or <c>false</c>, or an ISO 8601 date and time for those kinds.</param>
public sealed record CustomProperty(string Name, CustomPropertyKind Kind, string Value);

/// <summary>What a custom property holds.</summary>
public enum CustomPropertyKind
{
    /// <summary>Text.</summary>
    Text,
    /// <summary>A number.</summary>
    Number,
    /// <summary>Yes or no.</summary>
    YesNo,
    /// <summary>A date.</summary>
    Date,
}

/// <summary>A document variable.</summary>
/// <param name="Name">Its name.</param>
/// <param name="Value">Its value; null removes it.</param>
public sealed record DocumentVariable(string Name, [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? Value = null);

/// <summary>A text box's text columns.</summary>
/// <param name="Count">How many, 1 to 16.</param>
/// <param name="Spacing">The space between them, 0 to 1584 pt.</param>
public sealed record TextColumns(int Count, Length Spacing);

/// <summary>Which of a text box's lines text around it wraps tightly to.</summary>
public enum TightWrap
{
    /// <summary>The box's edges.</summary>
    None,
    /// <summary>Every line.</summary>
    AllLines,
    /// <summary>The first and last lines.</summary>
    FirstAndLastLine,
    /// <summary>The first line.</summary>
    FirstLineOnly,
    /// <summary>The last line.</summary>
    LastLineOnly,
}

/// <summary>Which way to flip an object.</summary>
public enum FlipAxis
{
    /// <summary>Left to right.</summary>
    Horizontal,
    /// <summary>Top to bottom.</summary>
    Vertical,
}

/// <summary>Where an object goes in the paint order.</summary>
public enum Arrange
{
    /// <summary>Over the next object above it.</summary>
    Forward,
    /// <summary>Under the next object below it.</summary>
    Backward,
    /// <summary>Over every object.</summary>
    ToFront,
    /// <summary>Under every object.</summary>
    ToBack,
    /// <summary>In front of the text.</summary>
    InFrontOfText,
    /// <summary>Behind the text.</summary>
    BehindText,
}

/// <summary>An outline change; unset values are kept.</summary>
/// <param name="Width">The width, 0 to 1584 pt.</param>
/// <param name="Fill">
/// <see cref="FillValue.None"/>, <see cref="FillValue.Solid"/> or <see cref="FillValue.Gradient"/>;
/// <see cref="FillValue.FromStyle"/> removes the whole explicit outline.
/// </param>
/// <param name="Dash">The ECMA-376 dash name, such as <c>dash</c> or <c>sysDot</c>.</param>
/// <param name="Compound">Single or several lines.</param>
/// <param name="Cap">The ends.</param>
/// <param name="Join">The joins.</param>
/// <param name="Ends">The arrowheads; they replace both ends.</param>
public sealed record LineChange(
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] Length? Width = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] FillValue? Fill = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? Dash = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] LineCompound? Compound = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] LineCap? Cap = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] LineJoin? Join = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] LineEnds? Ends = null);

/// <summary>One effect of an <see cref="EffectsChange"/>: kept (the default), removed, or set. A value converts to set.</summary>
/// <typeparam name="T">The effect's value type.</typeparam>
public readonly record struct EffectChange<T>
{
    private readonly byte _kind; // 0 keep, 1 remove, 2 set

    private EffectChange(byte kind, T value) => (_kind, Value) = (kind, value);

    /// <summary>The effect stays as it is.</summary>
    public static EffectChange<T> Keep => default;

    /// <summary>The effect is removed.</summary>
    public static EffectChange<T> Remove => new(1, default!);

    /// <summary>The effect is set to <paramref name="value"/>.</summary>
    /// <param name="value">The effect.</param>
    public static EffectChange<T> Set(T value) => new(2, value);

    /// <summary>The effect is set to <paramref name="value"/>.</summary>
    /// <param name="value">The effect.</param>
    public static implicit operator EffectChange<T>(T value) => Set(value);

    /// <summary>Whether the effect is removed.</summary>
    public bool IsRemove => _kind == 1;

    /// <summary>Whether the effect is set to <see cref="Value"/>.</summary>
    public bool IsSet => _kind == 2;

    /// <summary>The effect, when set.</summary>
    public T Value { get; }
}

/// <summary>An effects change: each effect is kept, removed or set.</summary>
/// <param name="OuterShadow">A shadow outside the shape.</param>
/// <param name="InnerShadow">A shadow inside the shape.</param>
/// <param name="Glow">A glow around the shape.</param>
/// <param name="SoftEdge">Edges faded over this distance.</param>
/// <param name="Blur">The whole shape blurred by this radius.</param>
/// <param name="Reflection">A reflection.</param>
public sealed record EffectsChange(
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingDefault)] EffectChange<Shadow> OuterShadow = default,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingDefault)] EffectChange<Shadow> InnerShadow = default,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingDefault)] EffectChange<Glow> Glow = default,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingDefault)] EffectChange<Length> SoftEdge = default,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingDefault)] EffectChange<Length> Blur = default,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingDefault)] EffectChange<Reflection> Reflection = default);

/// <summary>A text effects change: each effect is kept, removed (over a style's too; a fill is only removed) or set.</summary>
/// <param name="Fill">The letters' fill: none (hollow), solid or gradient of two to ten stops.</param>
/// <param name="Outline">The letters' outline.</param>
/// <param name="Shadow">A shadow behind the letters.</param>
/// <param name="Glow">A glow around the letters, 0 to 150 pt.</param>
/// <param name="Reflection">A reflection under the letters.</param>
public sealed record TextEffectsChange(
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingDefault)] EffectChange<FillValue> Fill = default,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingDefault)] EffectChange<TextOutline> Outline = default,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingDefault)] EffectChange<TextShadow> Shadow = default,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingDefault)] EffectChange<Glow> Glow = default,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingDefault)] EffectChange<TextReflection> Reflection = default);

/// <summary>A shape text body change; unset values are kept.</summary>
/// <param name="Insets">The left, top, right and bottom insets, 0 to 1584 pt.</param>
/// <param name="Anchor">Where the text sits.</param>
/// <param name="Autofit">How the shape fits it.</param>
/// <param name="WrapText">Lines wrap at the shape's width.</param>
/// <param name="Direction">Which way it runs.</param>
/// <param name="Columns">Its columns.</param>
/// <param name="Upright">The text stays upright when the shape turns ("Do not rotate text within shape").</param>
public sealed record ShapeTextChange(
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] IReadOnlyList<Length>? Insets = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] TextAnchor? Anchor = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] TextAutofit? Autofit = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? WrapText = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] TextDirection? Direction = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] TextColumns? Columns = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? Upright = null);

/// <summary>A picture adjustment change; unset values are kept.</summary>
/// <param name="Transparency">How see-through the picture is, 0 (opaque) to 1 (invisible).</param>
/// <param name="Grayscale">Shown in shades of grey.</param>
/// <param name="Brightness">Lighter (positive) or darker, −1 to 1.</param>
/// <param name="Contrast">More (positive) or less contrast, −1 to 1.</param>
/// <param name="TransparentColor">Set Transparent Colour: the colour drawn see-through, or <see cref="ColorValue.None"/>.</param>
public sealed record PictureAdjustChange(
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] double? Transparency = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? Grayscale = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] double? Brightness = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] double? Contrast = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] ColorValue? TransparentColor = null);

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
    /// <summary>A building block gallery.</summary>
    BuildingBlockGallery,
    /// <summary>A building block inserted from a gallery; read only.</summary>
    BuildingBlock,
    /// <summary>A citation; read only.</summary>
    Citation,
    /// <summary>A bibliography; read only.</summary>
    Bibliography,
    /// <summary>An equation; read only.</summary>
    Equation,
}

/// <summary>A date control's calendar.</summary>
public enum CalendarType
{
    /// <summary>Gregorian (localized).</summary>
    Gregorian,
    /// <summary>Hijri.</summary>
    Hijri,
    /// <summary>Hebrew lunar.</summary>
    Hebrew,
    /// <summary>Taiwan.</summary>
    Taiwan,
    /// <summary>Japanese emperor era.</summary>
    Japan,
    /// <summary>Thai.</summary>
    Thai,
    /// <summary>Korean Tangun era.</summary>
    Korea,
    /// <summary>Saka era.</summary>
    Saka,
    /// <summary>Gregorian, transliterated English.</summary>
    GregorianXlitEnglish,
    /// <summary>Gregorian, transliterated French.</summary>
    GregorianXlitFrench,
    /// <summary>Gregorian, US English.</summary>
    GregorianUs,
    /// <summary>Gregorian, Middle East French.</summary>
    GregorianMeFrench,
    /// <summary>Gregorian, Arabic.</summary>
    GregorianArabic,
}

/// <summary>What a bound date control stores in its data.</summary>
public enum DateStorage
{
    /// <summary>The text it shows.</summary>
    Text,
    /// <summary>The date.</summary>
    Date,
    /// <summary>The date and time.</summary>
    DateTime,
}

/// <summary>Where <see cref="EditorCommands.SetColumnsAsync"/> applies.</summary>
public enum ColumnsApplyTo
{
    /// <summary>The selection's sections.</summary>
    Section,
    /// <summary>Every section.</summary>
    WholeDocument,
    /// <summary>From the selection on: a continuous section break goes there.</summary>
    ThisPointForward,
    /// <summary>The selected text alone: continuous section breaks around it.</summary>
    SelectedText,
}

/// <summary>One of Word's column presets.</summary>
public enum ColumnPreset
{
    /// <summary>One column.</summary>
    One,
    /// <summary>Two equal columns.</summary>
    Two,
    /// <summary>Three equal columns.</summary>
    Three,
    /// <summary>Two, the narrow one on the left.</summary>
    Left,
    /// <summary>Two, the narrow one on the right.</summary>
    Right,
}

/// <summary>Which side of a heading another goes.</summary>
public enum HeadingSide
{
    /// <summary>Just before the heading.</summary>
    Before,
    /// <summary>After the heading's whole content.</summary>
    After,
}

/// <summary>One of the separators between the text and the notes.</summary>
public enum NoteSeparatorPart
{
    /// <summary>Above the notes on the page they start.</summary>
    Separator,
    /// <summary>Above the notes continued from the page before.</summary>
    ContinuationSeparator,
    /// <summary>Below notes that continue on the next page.</summary>
    ContinuationNotice,
}

/// <summary>A group or a person an editable range lets edit: exactly one of <see cref="Group"/> and <see cref="User"/>.</summary>
public sealed record RangeEditorRef
{
    /// <summary>A group: <c>everyone</c>, <c>administrators</c>, <c>contributors</c>, <c>editors</c>, <c>owners</c>, <c>current</c>.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public string? Group { get; init; }

    /// <summary>One person: a name or email address.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public string? User { get; init; }

    /// <summary>A group.</summary>
    /// <param name="name">The group's name.</param>
    /// <returns>The reference.</returns>
    public static RangeEditorRef ForGroup(string name) => new() { Group = name };

    /// <summary>One person.</summary>
    /// <param name="name">A name or email address.</param>
    /// <returns>The reference.</returns>
    public static RangeEditorRef ForUser(string name) => new() { User = name };
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

/// <summary>An AutoCorrect change: the rules given switch, the lists given replace the shipped ones; unset values are kept.</summary>
/// <param name="TwoInitialCapitals">Two initial capitals: <c>THe</c> becomes <c>The</c>.</param>
/// <param name="SentenceCapitals">The first letter of a sentence.</param>
/// <param name="CellCapitals">The first letter of a table cell.</param>
/// <param name="DayNames">Day names.</param>
/// <param name="ReplaceText">The replacement list and the symbols.</param>
/// <param name="SmartQuotes">Smart quotes.</param>
/// <param name="Ordinals">Ordinals.</param>
/// <param name="Fractions">Fractions.</param>
/// <param name="Dashes">Dashes.</param>
/// <param name="NumberedLists">Automatic numbered lists: <c>1.</c> then a space at a paragraph's start.</param>
/// <param name="BulletedLists">Automatic bulleted lists: <c>*</c> then a space at a paragraph's start.</param>
/// <param name="Entries">The replacement list, in place of the shipped one.</param>
/// <param name="FirstLetterExceptions">Abbreviations after which a sentence doesn't start (<c>e.g.</c>), in place of the shipped ones.</param>
/// <param name="InitialCapsExceptions">Words whose two initial capitals stay (<c>IDs</c>), in place of the shipped ones.</param>
/// <param name="Never">The texts never corrected.</param>
/// <param name="BorderLines">AutoFormat border lines: three or more <c>-</c>, <c>_</c>, <c>=</c>, <c>*</c>, <c>~</c> or <c>#</c> then Enter.</param>
/// <param name="InternetPaths">AutoFormat: Internet and network paths to hyperlinks.</param>
/// <param name="TabIndents">AutoFormat: indents set with Tab and Backspace at a paragraph's start.</param>
public sealed record AutoCorrectChange(
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? TwoInitialCapitals = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? SentenceCapitals = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? CellCapitals = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? DayNames = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? ReplaceText = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? SmartQuotes = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? Ordinals = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? Fractions = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? Dashes = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? NumberedLists = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? BulletedLists = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] IReadOnlyList<AutoCorrectEntry>? Entries = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] IReadOnlyList<string>? FirstLetterExceptions = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] IReadOnlyList<string>? InitialCapsExceptions = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] IReadOnlyList<string>? Never = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? BorderLines = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? InternetPaths = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? TabIndents = null);

/// <summary>One AutoCorrect replacement.</summary>
/// <param name="From">What is typed.</param>
/// <param name="To">What it becomes.</param>
public sealed record AutoCorrectEntry(string From, string To);

/// <summary>A style's list flags for <see cref="EditorCommands.SetStyleMetaAsync"/>: what is null is kept.</summary>
/// <param name="Id">The style.</param>
/// <param name="Visibility">Whether it shows in style lists.</param>
/// <param name="Priority">Its sort priority.</param>
/// <param name="Quick">Shown in the Styles gallery.</param>
/// <param name="Locked">Restricted while formatting is protected.</param>
public sealed record StyleMetaOptions(
    StyleId Id,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] StyleVisibility? Visibility = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] int? Priority = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? Quick = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? Locked = null);

/// <summary>The space before and after paragraphs, in <see cref="DocumentDefaults"/>.</summary>
/// <param name="Before">The space before.</param>
/// <param name="After">The space after.</param>
public sealed record ParagraphSpacingOptions(
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] SpacingValue? Before = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] SpacingValue? After = null);

/// <summary>The document's default formatting: what text has before any style's. What is null is kept.</summary>
/// <param name="Font">The font: a family, or the theme's heading or body font.</param>
/// <param name="Size">The size.</param>
/// <param name="Color">The font colour: RGB, a theme colour with at most one tint or shade, or automatic.</param>
/// <param name="Spacing">The space before and after paragraphs.</param>
/// <param name="LineSpacing">The line spacing.</param>
/// <param name="Indents">The left and right indents.</param>
public sealed record DocumentDefaults(
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] FontChoice? Font = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] Length? Size = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] ColorValue? Color = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] ParagraphSpacingOptions? Spacing = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] LineSpacing? LineSpacing = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] IndentOptions? Indents = null);

/// <summary>Indents to change; unset values are kept. <c>FirstLine</c> and <c>Hanging</c> are exclusive.</summary>
/// <param name="Left">The left indent.</param>
/// <param name="Right">The right indent.</param>
/// <param name="FirstLine">The first-line indent (at least 0).</param>
/// <param name="Hanging">The hanging indent (at least 0).</param>
/// <param name="Mirror">Mirror indents: on facing pages <c>Left</c> is the inside indent and <c>Right</c> the outside.</param>
public sealed record IndentOptions(
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] Length? Left = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] Length? Right = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] Length? FirstLine = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] Length? Hanging = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? Mirror = null);

/// <summary>Paragraph flow settings to change; unset values are kept.</summary>
/// <param name="KeepWithNext">Keep with the next paragraph.</param>
/// <param name="KeepLinesTogether">Keep the lines on one page.</param>
/// <param name="PageBreakBefore">Start on a new page.</param>
/// <param name="WidowControl">Avoid single lines at a page's top or bottom.</param>
/// <param name="DontHyphenate">Don't hyphenate the paragraph.</param>
/// <param name="SuppressLineNumbers">Leave the paragraph's lines out of line numbering.</param>
/// <param name="TextboxTightWrap">In a text box, which lines text around the box wraps tightly to.</param>
public sealed record ParagraphFlowOptions(
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? KeepWithNext = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? KeepLinesTogether = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? PageBreakBefore = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? WidowControl = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? DontHyphenate = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? SuppressLineNumbers = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] TightWrap? TextboxTightWrap = null);

/// <summary>Outline settings to change; unset values are kept.</summary>
/// <param name="Level">The outline level (1 to 9), or body text (0).</param>
/// <param name="Collapsed">Collapsed when the document opens.</param>
public sealed record OutlineOptions(
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] int? Level = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? Collapsed = null);

/// <summary>
/// Which way a paragraph's, section's or cell's text flows: lines left to right, top to bottom (<see cref="LrTb"/>);
/// vertical lines right to left (<see cref="TbRl"/>) or bottom to top (<see cref="BtLr"/>); and their East Asian forms.
/// </summary>
public enum TextFlow
{
    /// <summary>Horizontal.</summary>
    LrTb,
    /// <summary>Vertical, lines right to left.</summary>
    TbRl,
    /// <summary>Vertical, read from the bottom.</summary>
    BtLr,
    /// <summary>Horizontal, East Asian characters turned.</summary>
    LrTbV,
    /// <summary>Vertical, lines right to left, East Asian characters upright.</summary>
    TbRlV,
    /// <summary>Vertical, lines left to right, East Asian characters upright.</summary>
    TbLrV,
}

/// <summary>Page margins to change, 0 to 1584 pt; unset values are kept.</summary>
/// <param name="Top">The top margin.</param>
/// <param name="Bottom">The bottom margin.</param>
/// <param name="Left">The left margin.</param>
/// <param name="Right">The right margin.</param>
/// <param name="Gutter">The gutter.</param>
/// <param name="ApplyTo">The sections they apply to; null: the selection's.</param>
public sealed record MarginOptions(
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] Length? Top = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] Length? Bottom = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] Length? Left = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] Length? Right = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] Length? Gutter = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] PageSetupScope? ApplyTo = null);

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

/// <summary>Cell properties to change (Table Properties' Cell tab); unset values are kept.</summary>
/// <param name="Width">The preferred width: automatic, points (0 to 1584) or percent of the table (0 to 100).</param>
/// <param name="TextDirection">The direction text runs in.</param>
/// <param name="WrapText">Text wraps in the cell.</param>
/// <param name="FitText">Text is squeezed to fit the cell's width.</param>
/// <param name="HideMark">The cell's end mark takes no height: an empty cell is as tall as its margins.</param>
public sealed record CellPropertiesOptions(
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] TableWidth? Width = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] TextFlow? TextDirection = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? WrapText = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? FitText = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? HideMark = null);

/// <summary>The table properties rows override (row exceptions); unset values are kept.</summary>
/// <param name="Reset">Removes the rows' exceptions first.</param>
/// <param name="Alignment">The rows' alignment.</param>
/// <param name="CellSpacing">The space between the rows' cells, 0 to 1584.</param>
/// <param name="Indent">The rows' indent from the table's left edge, -1584 to 1584; not in a fixed-width table.</param>
/// <param name="Shading">The rows' shading: a fill, and a pattern in the pattern colour over it.</param>
/// <param name="Borders">The table borders on the rows' edges.</param>
public sealed record RowExceptions(
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? Reset = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] TableAlignment? Alignment = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] Length? CellSpacing = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] Length? Indent = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] ShadingValue? Shading = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] RowBorders? Borders = null);

/// <summary>A table's default cell margins (Table Options), 0 to 1584 pt; unset sides are kept (in the state, every side is set).</summary>
/// <param name="Top">The top margin.</param>
/// <param name="Left">The left margin.</param>
/// <param name="Bottom">The bottom margin.</param>
/// <param name="Right">The right margin.</param>
public sealed record TableCellMargins(
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] Length? Top = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] Length? Left = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] Length? Bottom = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] Length? Right = null);

/// <summary>Where Insert Cells shifts the selected cells.</summary>
public enum InsertCellShift
{
    /// <summary>Right: the rows grow by the inserted cells.</summary>
    Right,
    /// <summary>Down: the selected columns' cells move down, rows added at the table's end.</summary>
    Down,
}

/// <summary>The pointer's table tool.</summary>
public enum TableTool
{
    /// <summary>No tool: the pointer selects.</summary>
    None,
    /// <summary>Draw Table: a drag draws a line, a diagonal or a new table.</summary>
    Pen,
    /// <summary>Eraser: a press, or a drag, over a border merges the cells either side.</summary>
    Eraser,
}

/// <summary>Borders on some edges of rows.</summary>
/// <param name="Edges">Which edges.</param>
/// <param name="Border">The line; a ½ pt single automatic line when null.</param>
public sealed record RowBorders(BorderEdges Edges, [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] CellBorder? Border = null);

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

/// <summary>A border line: of text, paragraphs, cells, tables and pages.</summary>
/// <param name="Style">The line style.</param>
/// <param name="Width">The width, 0.25 to 6 pt.</param>
/// <param name="Color">The colour (RGB or a theme colour with at most one tint or shade), or <see cref="ColorValue.Auto"/>.</param>
/// <param name="Space">
/// The distance from what it borders, 0 to 31 pt (a page border measured from the page, from the page's edge). Null:
/// 0 for text and cells, 1 for paragraphs, and for a page border 24 from the page's edge, 1 (top and bottom) or 4
/// (left and right) from the text; a kept page border edge keeps its own.
/// </param>
/// <param name="Shadow">A shadow; null: none.</param>
public sealed record CellBorder(
    LineStyle Style,
    Length Width,
    ColorValue Color,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] Length? Space = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? Shadow = null);

/// <summary>
/// The rows and columns in each of a table's bands for <see cref="EditorCommands.SetTableBandSizesAsync"/>, 1 to
/// 32767: each given is set; <c>RowsFromStyle</c> and <c>ColumnsFromStyle</c> leave one to the table's style; the rest
/// are kept.
/// </summary>
/// <param name="Rows">Rows in each band.</param>
/// <param name="Columns">Columns in each band.</param>
/// <param name="RowsFromStyle">The rows in each band are the style's (ignored when <paramref name="Rows"/> is set).</param>
/// <param name="ColumnsFromStyle">The columns in each band are the style's (ignored when <paramref name="Columns"/> is set).</param>
[JsonConverter(typeof(TableBandSizesConverter))]
public sealed record TableBandSizes(int? Rows = null, int? Columns = null, bool RowsFromStyle = false, bool ColumnsFromStyle = false);

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
/// <param name="Width">A picture's width, 1 to 1584 pt: its placeholder's, and the picture's once filled.</param>
/// <param name="Height">A picture's height, 1 to 1584 pt.</param>
/// <param name="CheckedSymbol">A checkbox's checked symbol.</param>
/// <param name="UncheckedSymbol">A checkbox's unchecked symbol.</param>
/// <param name="SectionTitle">What a repeating section's items are called.</param>
/// <param name="AllowInsertDelete">A repeating section's items can be added and removed.</param>
/// <param name="Calendar">A date's calendar.</param>
/// <param name="StoreAs">What a bound date control stores in its data.</param>
/// <param name="Gallery">A building block gallery's gallery (Word's name for it, <c>Quick Parts</c>); empty clears it to any.</param>
/// <param name="Category">A building block gallery's category; empty clears it to any.</param>
public sealed record ControlOptions(
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? Checked = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? Format = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? Locale = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] IReadOnlyList<ControlListItem>? Items = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? Multiline = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] Length? Width = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] Length? Height = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] CheckboxSymbol? CheckedSymbol = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] CheckboxSymbol? UncheckedSymbol = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? SectionTitle = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? AllowInsertDelete = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] CalendarType? Calendar = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] DateStorage? StoreAs = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? Gallery = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? Category = null);

/// <summary>A checkbox control's checked or unchecked symbol.</summary>
/// <param name="Font">Its font.</param>
/// <param name="Character">The character, one.</param>
public sealed record CheckboxSymbol(string Font, string Character);

/// <summary>How a content control shows its edges.</summary>
public enum ControlAppearance
{
    /// <summary>A box around it.</summary>
    BoundingBox,
    /// <summary>Start and end tags.</summary>
    Tags,
    /// <summary>Nothing.</summary>
    Hidden,
}

/// <summary>A content control to insert.</summary>
/// <param name="Kind">The kind.</param>
/// <param name="Tag">The tag.</param>
/// <param name="Title">The title.</param>
/// <param name="Lock">What the lock prevents.</param>
/// <param name="Placeholder">The placeholder text.</param>
/// <param name="Binding">The data binding.</param>
/// <param name="Options">Kind-specific options.</param>
/// <param name="WrapSelection">Wrap the selection instead of inserting an empty control.</param>
/// <param name="PlaceholderImage">A picture control's placeholder picture; null for the engine's.</param>
/// <param name="Float">Floats a picture control; null keeps it in line.</param>
public sealed record InsertControlOptions(
    ControlKind Kind,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? Tag = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? Title = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] ControlLock? Lock = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? Placeholder = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] DataBinding? Binding = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] ControlOptions? Options = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? WrapSelection = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] PlaceholderImage? PlaceholderImage = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] ControlFloat? Float = null);

/// <summary>
/// Floats a picture control, positioned and wrapped as <see cref="EditorCommands.SetObjectWrapAsync"/> and
/// <see cref="EditorCommands.SetObjectPositionAsync"/> place an object.
/// </summary>
/// <param name="Wrap">How text wraps around it; it can't be <see cref="ObjectWrap.Inline"/>.</param>
/// <param name="Horizontal">The horizontal place; null for where it would sit in line.</param>
/// <param name="Vertical">The vertical place; null for where it would sit in line.</param>
public sealed record ControlFloat(
    ObjectWrap Wrap,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] HorizontalPosition? Horizontal = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] VerticalPosition? Vertical = null);

/// <summary>A picture control's placeholder picture: what the editor, the saved DOCX and a PDF show until it is filled. An SVG is drawn with the picture's box as its viewport.</summary>
/// <param name="Data">The picture: PNG, JPEG, GIF, BMP or SVG.</param>
/// <param name="Mime">Its MIME type (<c>image/png</c>, <c>image/svg+xml</c>); the data must be one.</param>
public sealed record PlaceholderImage(ReadOnlyMemory<byte> Data, string Mime);

/// <summary>Changes to a content control; unset values are kept.</summary>
/// <param name="Id">The control.</param>
/// <param name="Tag">The new tag.</param>
/// <param name="Title">The new title.</param>
/// <param name="Lock">The new lock.</param>
/// <param name="Placeholder">The new placeholder text.</param>
/// <param name="Binding">The new data binding.</param>
/// <param name="Options">The new kind-specific options.</param>
/// <param name="RemoveBinding">Remove the data binding (ignored when <paramref name="Binding"/> is set).</param>
/// <param name="PlaceholderImage">A picture control's new placeholder picture, while it shows its placeholder.</param>
[JsonConverter(typeof(ControlChangesConverter))]
public sealed record ControlChanges(
    ElementId Id, string? Tag = null, string? Title = null, ControlLock? Lock = null, string? Placeholder = null,
    DataBinding? Binding = null, ControlOptions? Options = null, bool RemoveBinding = false, PlaceholderImage? PlaceholderImage = null)
{
    /// <summary>How it shows its edges.</summary>
    public ControlAppearance? Appearance { get; init; }

    /// <summary>Its colour; <see cref="ColorValue.None"/> for the default.</summary>
    public ColorValue? Color { get; init; }

    /// <summary>It goes when its content is edited, leaving the content.</summary>
    public bool? Temporary { get; init; }

    /// <summary>The character style text typed into it takes; <see cref="StyleId.None"/> for none.</summary>
    public StyleId? Style { get; init; }
}

/// <summary>What to fill a content control with: exactly one of text, a picture, a document or a building block.</summary>
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

    /// <summary>The building block's name.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public string? BuildingBlock { get; private init; }

    /// <summary>A date control's date.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public string? Date { get; private init; }

    /// <summary>A date control's date: its text that date in its format, its stored date that date.</summary>
    /// <param name="date">ISO 8601: <c>YYYY-MM-DD</c>, with a time and zone or without.</param>
    public static ControlContent FromDate(string date) => new() { Date = date };

    /// <summary>Plain text.</summary>
    /// <param name="text">The text.</param>
    public static ControlContent FromText(string text) => new() { Text = text };

    /// <summary>A picture, fitted to the control's box.</summary>
    /// <param name="image">The image bytes.</param>
    public static ControlContent FromImage(ReadOnlyMemory<byte> image) => new() { Image = image };

    /// <summary>The body of a DOCX document.</summary>
    /// <param name="docx">The DOCX bytes.</param>
    public static ControlContent FromDocx(ReadOnlyMemory<byte> docx) => new() { Docx = docx };

    /// <summary>A building block gallery's entry (<see cref="ControlInfo.BuildingBlocks"/>): its content.</summary>
    /// <param name="name">The building block's name.</param>
    public static ControlContent FromBuildingBlock(string name) => new() { BuildingBlock = name };
}

/// <summary>A mark on an element: a wavy line with a tooltip.</summary>
/// <param name="Id">The element.</param>
/// <param name="Severity">The severity.</param>
/// <param name="Message">The tooltip text.</param>
public sealed record ElementMark(ElementId Id, MarkSeverity Severity, string Message);

