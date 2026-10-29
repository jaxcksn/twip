using System.Drawing;
using System.Globalization;
using System.Text.Json.Serialization;
using Twip.Wire;

namespace Twip;

/// <summary>A length in the document, held in points (1/72 inch).</summary>
[JsonConverter(typeof(LengthConverter))]
public readonly record struct Length : IComparable<Length>
{
    private Length(double points) => Points = points;

    /// <summary>The length in points.</summary>
    public double Points { get; }

    /// <summary>Zero points.</summary>
    public static Length Zero => default;

    /// <summary>A length of <paramref name="points"/> points.</summary>
    /// <param name="points">The length in points.</param>
    public static Length FromPoints(double points) => new(points);

    /// <summary>A length of <paramref name="inches"/> inches.</summary>
    /// <param name="inches">The length in inches.</param>
    public static Length FromInches(double inches) => new(inches * 72);

    /// <summary>A length of <paramref name="cm"/> centimetres.</summary>
    /// <param name="cm">The length in centimetres.</param>
    public static Length FromCm(double cm) => new(cm * 72 / 2.54);

    /// <summary>A length of <paramref name="mm"/> millimetres.</summary>
    /// <param name="mm">The length in millimetres.</param>
    public static Length FromMm(double mm) => new(mm * 72 / 25.4);

    /// <summary>The length in inches.</summary>
    public double ToInches() => Points / 72;

    /// <summary>The length in centimetres.</summary>
    public double ToCm() => Points * 2.54 / 72;

    /// <summary>The length in millimetres.</summary>
    public double ToMm() => Points * 25.4 / 72;

    /// <summary>The sum of two lengths.</summary>
    /// <param name="a">The first length.</param>
    /// <param name="b">The second length.</param>
    public static Length operator +(Length a, Length b) => new(a.Points + b.Points);

    /// <summary>The difference of two lengths.</summary>
    /// <param name="a">The first length.</param>
    /// <param name="b">The length to subtract.</param>
    public static Length operator -(Length a, Length b) => new(a.Points - b.Points);

    /// <summary>The negated length.</summary>
    /// <param name="a">The length.</param>
    public static Length operator -(Length a) => new(-a.Points);

    /// <summary>A length scaled by a factor.</summary>
    /// <param name="a">The length.</param>
    /// <param name="factor">The factor.</param>
    public static Length operator *(Length a, double factor) => new(a.Points * factor);

    /// <summary>A length scaled by a factor.</summary>
    /// <param name="factor">The factor.</param>
    /// <param name="a">The length.</param>
    public static Length operator *(double factor, Length a) => new(a.Points * factor);

    /// <summary>A length divided by a factor.</summary>
    /// <param name="a">The length.</param>
    /// <param name="divisor">The divisor.</param>
    public static Length operator /(Length a, double divisor) => new(a.Points / divisor);

    /// <summary>Whether <paramref name="a"/> is shorter than <paramref name="b"/>.</summary>
    /// <param name="a">The first length.</param>
    /// <param name="b">The second length.</param>
    public static bool operator <(Length a, Length b) => a.Points < b.Points;

    /// <summary>Whether <paramref name="a"/> is longer than <paramref name="b"/>.</summary>
    /// <param name="a">The first length.</param>
    /// <param name="b">The second length.</param>
    public static bool operator >(Length a, Length b) => a.Points > b.Points;

    /// <summary>Whether <paramref name="a"/> is at most <paramref name="b"/>.</summary>
    /// <param name="a">The first length.</param>
    /// <param name="b">The second length.</param>
    public static bool operator <=(Length a, Length b) => a.Points <= b.Points;

    /// <summary>Whether <paramref name="a"/> is at least <paramref name="b"/>.</summary>
    /// <param name="a">The first length.</param>
    /// <param name="b">The second length.</param>
    public static bool operator >=(Length a, Length b) => a.Points >= b.Points;

    /// <inheritdoc/>
    public int CompareTo(Length other) => Points.CompareTo(other.Points);

    /// <inheritdoc/>
    public override string ToString() => Points.ToString(CultureInfo.InvariantCulture) + " pt";
}

/// <summary>A colour in the document: an RGB colour (alpha is ignored), <see cref="Auto"/>, or <see cref="None"/>.</summary>
[JsonConverter(typeof(ColorValueConverter))]
public readonly record struct ColorValue
{
    private readonly byte _kind; // 0 rgb, 1 auto, 2 none
    private readonly Color _color;

    private ColorValue(byte kind, Color color) => (_kind, _color) = (kind, color);

    /// <summary>The automatic colour (black text on a light background).</summary>
    public static ColorValue Auto => new(1, default);

    /// <summary>No colour: no shading or highlight. As a command argument it removes the colour.</summary>
    public static ColorValue None => new(2, default);

    /// <summary>An RGB colour; alpha is ignored.</summary>
    /// <param name="color">The colour.</param>
    public static ColorValue FromColor(Color color) => new(0, System.Drawing.Color.FromArgb(255, color.R, color.G, color.B));

    /// <summary>An RGB colour from its components.</summary>
    /// <param name="r">Red.</param>
    /// <param name="g">Green.</param>
    /// <param name="b">Blue.</param>
    public static ColorValue FromRgb(byte r, byte g, byte b) => new(0, System.Drawing.Color.FromArgb(255, r, g, b));

    /// <summary>An RGB colour.</summary>
    /// <param name="color">The colour.</param>
    public static implicit operator ColorValue(Color color) => FromColor(color);

    /// <summary>Whether this is <see cref="Auto"/>.</summary>
    public bool IsAuto => _kind == 1;

    /// <summary>Whether this is <see cref="None"/>.</summary>
    public bool IsNone => _kind == 2;

    /// <summary>The RGB colour, or null for <see cref="Auto"/> and <see cref="None"/>.</summary>
    public Color? Color => _kind == 0 ? _color : null;

    /// <summary>The colour as <c>#RRGGBB</c>, <c>auto</c> or <c>none</c>.</summary>
    public override string ToString() => _kind switch
    {
        1 => "auto",
        2 => "none",
        _ => $"#{_color.R:X2}{_color.G:X2}{_color.B:X2}",
    };

    internal static ColorValue Parse(string text) => text switch
    {
        "auto" => Auto,
        "none" => None,
        _ when text.Length == 7 && text[0] == '#' && int.TryParse(text.Substring(1), NumberStyles.HexNumber, CultureInfo.InvariantCulture, out var rgb)
            => FromRgb((byte)(rgb >> 16), (byte)(rgb >> 8), (byte)rgb),
        _ => throw new FormatException($"not a colour: {text}"),
    };
}

/// <summary>The id of a merge-designer element (a field or a content control).</summary>
/// <param name="Value">The opaque id.</param>
[JsonConverter(typeof(ElementIdConverter))]
public readonly record struct ElementId(string Value)
{
    /// <inheritdoc/>
    public override string ToString() => Value;
}

/// <summary>The id of a comment. A thread's id is its first comment's id.</summary>
/// <param name="Value">The opaque id.</param>
[JsonConverter(typeof(CommentIdConverter))]
public readonly record struct CommentId(string Value)
{
    /// <inheritdoc/>
    public override string ToString() => Value;
}

/// <summary>The id of a tracked revision.</summary>
/// <param name="Value">The opaque id.</param>
[JsonConverter(typeof(RevisionIdConverter))]
public readonly record struct RevisionId(string Value)
{
    /// <inheritdoc/>
    public override string ToString() => Value;
}

/// <summary>The id of a heading, for a navigation pane.</summary>
/// <param name="Value">The opaque id.</param>
[JsonConverter(typeof(HeadingIdConverter))]
public readonly record struct HeadingId(string Value)
{
    /// <inheritdoc/>
    public override string ToString() => Value;
}

/// <summary>The id of a find match.</summary>
/// <param name="Value">The opaque id.</param>
[JsonConverter(typeof(MatchIdConverter))]
public readonly record struct MatchId(string Value)
{
    /// <inheritdoc/>
    public override string ToString() => Value;
}

/// <summary>The id of a style (<c>w:styleId</c>).</summary>
/// <param name="Value">The style id.</param>
[JsonConverter(typeof(StyleIdConverter))]
public readonly record struct StyleId(string Value)
{
    /// <inheritdoc/>
    public override string ToString() => Value;
}

/// <summary>Whether a formatting value is the same across the selection.</summary>
public enum FormatValueKind
{
    /// <summary>The value doesn't apply here (no table, nothing selected).</summary>
    Unavailable,
    /// <summary>One value for the whole selection.</summary>
    Uniform,
    /// <summary>The selection has different values.</summary>
    Mixed,
}

/// <summary>A formatting value over the selection: uniform, mixed, or unavailable.</summary>
/// <typeparam name="T">The value type.</typeparam>
public readonly record struct FormatValue<T>
{
    private FormatValue(FormatValueKind kind, T value) => (Kind, Value) = (kind, value);

    /// <summary>Whether the value is uniform, mixed or unavailable.</summary>
    public FormatValueKind Kind { get; }

    /// <summary>The value when <see cref="Kind"/> is <see cref="FormatValueKind.Uniform"/>; otherwise the default.</summary>
    public T Value { get; }

    /// <summary>A uniform value.</summary>
    /// <param name="value">The value.</param>
    public static FormatValue<T> Uniform(T value) => new(FormatValueKind.Uniform, value);

    /// <summary>A mixed value.</summary>
    public static FormatValue<T> Mixed => new(FormatValueKind.Mixed, default!);

    /// <summary>An unavailable value.</summary>
    public static FormatValue<T> Unavailable => default;

    /// <summary>Whether the value is uniform.</summary>
    public bool IsUniform => Kind == FormatValueKind.Uniform;

    /// <summary>Whether the value is mixed.</summary>
    public bool IsMixed => Kind == FormatValueKind.Mixed;

    /// <summary>True only for a uniform <c>true</c>: what a toggle button shows as pressed.</summary>
    public bool IsOn => Kind == FormatValueKind.Uniform && Value is true;

    /// <inheritdoc/>
    public override string ToString() => Kind switch
    {
        FormatValueKind.Uniform => Value?.ToString() ?? "",
        FormatValueKind.Mixed => "mixed",
        _ => "unavailable",
    };
}

/// <summary>Paragraph spacing: a length, or automatic spacing.</summary>
[JsonConverter(typeof(SpacingValueConverter))]
public readonly record struct SpacingValue
{
    private SpacingValue(bool auto, Length length) => (IsAuto, Length) = (auto, length);

    /// <summary>Automatic spacing.</summary>
    public static SpacingValue Auto => new(true, default);

    /// <summary>Spacing of a fixed length.</summary>
    /// <param name="length">The spacing.</param>
    public static SpacingValue FromLength(Length length) => new(false, length);

    /// <summary>Spacing of a fixed length.</summary>
    /// <param name="length">The spacing.</param>
    public static implicit operator SpacingValue(Length length) => FromLength(length);

    /// <summary>Whether the spacing is automatic.</summary>
    public bool IsAuto { get; }

    /// <summary>The spacing, when not automatic.</summary>
    public Length Length { get; }
}

/// <summary>A row height rule.</summary>
public enum RowHeightRule
{
    /// <summary>At least the height.</summary>
    AtLeast,
    /// <summary>Exactly the height.</summary>
    Exact,
}

/// <summary>A table row height: automatic, or a height with a rule.</summary>
[JsonConverter(typeof(RowHeightConverter))]
public sealed record RowHeight
{
    private RowHeight(bool auto, Length height, RowHeightRule rule) => (IsAuto, Height, Rule) = (auto, height, rule);

    /// <summary>The height follows the content.</summary>
    public static RowHeight Auto { get; } = new(true, default, RowHeightRule.AtLeast);

    /// <summary>A row at least <paramref name="height"/> tall.</summary>
    /// <param name="height">The height.</param>
    public static RowHeight AtLeast(Length height) => new(false, height, RowHeightRule.AtLeast);

    /// <summary>A row exactly <paramref name="height"/> tall.</summary>
    /// <param name="height">The height.</param>
    public static RowHeight Exact(Length height) => new(false, height, RowHeightRule.Exact);

    /// <summary>Whether the height follows the content.</summary>
    public bool IsAuto { get; }

    /// <summary>The height, when not automatic.</summary>
    public Length Height { get; }

    /// <summary>The rule, when not automatic.</summary>
    public RowHeightRule Rule { get; }
}

/// <summary>A zoom level: a percentage, or a fit computed by the host.</summary>
[JsonConverter(typeof(ZoomLevelConverter))]
public readonly record struct ZoomLevel
{
    private ZoomLevel(double percent, byte fit) => (Percent, _fit) = (percent, fit);
    private readonly byte _fit; // 0 percent, 1 page width, 2 whole page

    /// <summary>A zoom percentage, 10 to 500.</summary>
    /// <param name="percent">The percentage (100 is actual size).</param>
    public static ZoomLevel FromPercent(double percent) => new(percent, 0);

    /// <summary>Fit the page width to the view.</summary>
    public static ZoomLevel PageWidth => new(0, 1);

    /// <summary>Fit the whole page in the view.</summary>
    public static ZoomLevel WholePage => new(0, 2);

    /// <summary>The percentage, when not a fit.</summary>
    public double Percent { get; }

    /// <summary>Whether this fits the page width.</summary>
    public bool IsPageWidth => _fit == 1;

    /// <summary>Whether this fits the whole page.</summary>
    public bool IsWholePage => _fit == 2;
}

/// <summary>How a table's width is set.</summary>
public enum TableWidthKind
{
    /// <summary>The width follows the content.</summary>
    Auto,
    /// <summary>A width in points.</summary>
    Points,
    /// <summary>A percentage of the text width.</summary>
    Percent,
}

/// <summary>A table width.</summary>
/// <param name="Kind">How the width is set.</param>
/// <param name="Value">The points or the percentage (1 to 600); absent for <see cref="TableWidthKind.Auto"/>.</param>
public sealed record TableWidth(TableWidthKind Kind, [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] double? Value = null)
{
    /// <summary>An automatic width.</summary>
    public static TableWidth Auto { get; } = new(TableWidthKind.Auto);

    /// <summary>A width in points.</summary>
    /// <param name="width">The width.</param>
    public static TableWidth FromLength(Length width) => new(TableWidthKind.Points, width.Points);

    /// <summary>A percentage of the text width.</summary>
    /// <param name="percent">The percentage, 1 to 600.</param>
    public static TableWidth FromPercent(double percent) => new(TableWidthKind.Percent, percent);
}

/// <summary>How line spacing is measured.</summary>
public enum LineSpacingRule
{
    /// <summary>A multiple of single spacing.</summary>
    Multiple,
    /// <summary>Exactly a height.</summary>
    Exact,
    /// <summary>At least a height.</summary>
    AtLeast,
}

/// <summary>Line spacing: a multiple of single lines, or a height with a rule.</summary>
/// <param name="Rule">How the spacing is measured.</param>
/// <param name="Lines">The multiple, for <see cref="LineSpacingRule.Multiple"/> (0.06 to 132).</param>
/// <param name="Size">The height, for <see cref="LineSpacingRule.Exact"/> and <see cref="LineSpacingRule.AtLeast"/>.</param>
public sealed record LineSpacing(
    LineSpacingRule Rule,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] double? Lines = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] Length? Size = null)
{
    /// <summary>A multiple of single spacing (1, 1.5, 2...).</summary>
    /// <param name="lines">The multiple.</param>
    public static LineSpacing Multiple(double lines) => new(LineSpacingRule.Multiple, Lines: lines);

    /// <summary>Exactly <paramref name="size"/> per line.</summary>
    /// <param name="size">The line height.</param>
    public static LineSpacing Exactly(Length size) => new(LineSpacingRule.Exact, Size: size);

    /// <summary>At least <paramref name="size"/> per line.</summary>
    /// <param name="size">The minimum line height.</param>
    public static LineSpacing AtLeast(Length size) => new(LineSpacingRule.AtLeast, Size: size);
}
