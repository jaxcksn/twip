using System.Text.Json.Serialization;
using Twip.Wire;

namespace Twip;

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

/// <summary>A page orientation.</summary>
public enum PageOrientation
{
    /// <summary>Portrait.</summary>
    Portrait,
    /// <summary>Landscape.</summary>
    Landscape,
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
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] byte[]? Picture = null);

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

/// <summary>The text's border for <see cref="EditorCommands.SetTextBorderAsync"/>.</summary>
/// <param name="Border">The box's line; null removes it.</param>
/// <param name="Shadow">A shadow.</param>
public sealed record TextBorderOptions(CellBorder? Border, bool Shadow = false);

/// <summary>Page borders for <see cref="EditorCommands.SetPageBordersAsync"/>: every edge is set, and null removes it.</summary>
/// <param name="Top">The top edge.</param>
/// <param name="Bottom">The bottom edge.</param>
/// <param name="Left">The left edge.</param>
/// <param name="Right">The right edge.</param>
/// <param name="Shadow">A shadow on the bottom and right edges.</param>
/// <param name="OffsetFrom">What the borders are measured from; null keeps it.</param>
/// <param name="Display">The pages they show on; null keeps it.</param>
/// <param name="ApplyTo">The sections they apply to.</param>
public sealed record PageBordersOptions(
    CellBorder? Top = null,
    CellBorder? Bottom = null,
    CellBorder? Left = null,
    CellBorder? Right = null,
    bool Shadow = false,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] PageBorderOffset? OffsetFrom = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] PageBorderDisplay? Display = null,
    PageBorderScope ApplyTo = PageBorderScope.ThisSection);

/// <summary>The paragraph border lines for <see cref="EditorCommands.SetParagraphBorderLinesAsync"/>: every edge is set, and null removes it.</summary>
/// <param name="Top">The top edge.</param>
/// <param name="Bottom">The bottom edge.</param>
/// <param name="Left">The left edge.</param>
/// <param name="Right">The right edge.</param>
/// <param name="Between">The line between paragraphs with the same borders.</param>
/// <param name="Shadow">A shadow on the bottom and right edges.</param>
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
    bool LockAnchor);

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

/// <summary>
/// A floating object's horizontal place: aligned across its reference, or an offset from the reference's left edge.
/// Set with neither, the object stays where it is, measured from the new reference.
/// </summary>
/// <param name="RelativeTo">What it is measured from.</param>
/// <param name="Align">Aligned across the reference.</param>
/// <param name="Offset">Right of the reference's left edge, −1584 to 1584 pt.</param>
public sealed record HorizontalPosition(
    HorizontalReference RelativeTo,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] ObjectHorizontalAlignment? Align = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] Length? Offset = null);

/// <summary>
/// A floating object's vertical place: aligned across its reference, or an offset below the reference's top. Set with
/// neither, the object stays where it is, measured from the new reference.
/// </summary>
/// <param name="RelativeTo">What it is measured from.</param>
/// <param name="Align">Aligned across the reference.</param>
/// <param name="Offset">Below the reference's top, −1584 to 1584 pt.</param>
public sealed record VerticalPosition(
    VerticalReference RelativeTo,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] ObjectVerticalAlignment? Align = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] Length? Offset = null);

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
public sealed record EffectsChange(
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingDefault)] EffectChange<Shadow> OuterShadow = default,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingDefault)] EffectChange<Shadow> InnerShadow = default,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingDefault)] EffectChange<Glow> Glow = default,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingDefault)] EffectChange<Length> SoftEdge = default,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingDefault)] EffectChange<Length> Blur = default);

/// <summary>A shape text body change; unset values are kept.</summary>
/// <param name="Insets">The left, top, right and bottom insets, 0 to 1584 pt.</param>
/// <param name="Anchor">Where the text sits.</param>
/// <param name="Autofit">How the shape fits it.</param>
/// <param name="WrapText">Lines wrap at the shape's width.</param>
/// <param name="Direction">Which way it runs.</param>
public sealed record ShapeTextChange(
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] IReadOnlyList<Length>? Insets = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] TextAnchor? Anchor = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] TextAutofit? Autofit = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? WrapText = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] TextDirection? Direction = null);

/// <summary>A picture adjustment change; unset values are kept.</summary>
/// <param name="Transparency">How see-through the picture is, 0 (opaque) to 1 (invisible).</param>
/// <param name="Grayscale">Shown in shades of grey.</param>
/// <param name="Brightness">Lighter (positive) or darker, −1 to 1.</param>
/// <param name="Contrast">More (positive) or less contrast, −1 to 1.</param>
public sealed record PictureAdjustChange(
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] double? Transparency = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? Grayscale = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] double? Brightness = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] double? Contrast = null);

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
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] IReadOnlyList<string>? Never = null);

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
/// <param name="Font">The font.</param>
/// <param name="Size">The size.</param>
/// <param name="Color">The font color.</param>
/// <param name="Spacing">The space before and after paragraphs.</param>
/// <param name="LineSpacing">The line spacing.</param>
/// <param name="Indents">The left and right indents.</param>
public sealed record DocumentDefaults(
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? Font = null,
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
/// <param name="Width">A picture's width, 1 to 1584 pt: its placeholder's, and the picture's once filled.</param>
/// <param name="Height">A picture's height, 1 to 1584 pt.</param>
public sealed record ControlOptions(
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? Checked = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? Format = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? Locale = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] IReadOnlyList<ControlListItem>? Items = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? Multiline = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] Length? Width = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] Length? Height = null);

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
    DataBinding? Binding = null, ControlOptions? Options = null, bool RemoveBinding = false, PlaceholderImage? PlaceholderImage = null);

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
