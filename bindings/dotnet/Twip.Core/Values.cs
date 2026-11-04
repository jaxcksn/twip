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

/// <summary>
/// A positioned table's place (<see cref="EditorCommands.SetTablePositionAsync"/>): each axis as an object's, and its
/// distance from the text around it. A table is measured from the column, the margin or the page across, and from the
/// paragraph, the margin or the page down; another reference is an invalid argument. Given to the command, what isn't
/// set is kept; as the command's state value, every property is set.
/// </summary>
/// <param name="Horizontal">The horizontal place: an alignment or an offset.</param>
/// <param name="Vertical">The vertical place: an alignment or an offset.</param>
/// <param name="WrapDistance">How far the text around it keeps, 0 to 1584 pt.</param>
public sealed record TablePosition(
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] HorizontalPosition? Horizontal = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] VerticalPosition? Vertical = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] TextDistance? WrapDistance = null);

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

/// <summary>A preset geometry by its ECMA-376 name, such as <c>wedgeRectCallout</c>. <see cref="EditorSession.ShapeGallery"/> lists Word's.</summary>
/// <param name="Value">The ECMA-376 name.</param>
[JsonConverter(typeof(ShapePresetConverter))]
public readonly record struct ShapePreset(string Value)
{
    /// <inheritdoc/>
    public override string ToString() => Value;
}

/// <summary>A theme colour, in the theme's order.</summary>
public enum ThemeColor
{
    /// <summary>Dark 1 (through the document's colour mapping).</summary>
    Text1,
    /// <summary>Light 1.</summary>
    Background1,
    /// <summary>Dark 2.</summary>
    Text2,
    /// <summary>Light 2.</summary>
    Background2,
    /// <summary>Accent 1.</summary>
    Accent1,
    /// <summary>Accent 2.</summary>
    Accent2,
    /// <summary>Accent 3.</summary>
    Accent3,
    /// <summary>Accent 4.</summary>
    Accent4,
    /// <summary>Accent 5.</summary>
    Accent5,
    /// <summary>Accent 6.</summary>
    Accent6,
    /// <summary>The hyperlink colour.</summary>
    Hyperlink,
    /// <summary>The followed hyperlink colour.</summary>
    FollowedHyperlink,
}

/// <summary>What a colour transform does.</summary>
public enum ColorTransformKind
{
    /// <summary>Towards white by <c>1 - value</c>.</summary>
    Tint,
    /// <summary>Towards black by <c>1 - value</c>.</summary>
    Shade,
    /// <summary>Luminance times the value.</summary>
    LumMod,
    /// <summary>Luminance plus the value.</summary>
    LumOff,
    /// <summary>Saturation times the value.</summary>
    SatMod,
    /// <summary>Saturation plus the value.</summary>
    SatOff,
    /// <summary>Hue times the value.</summary>
    HueMod,
    /// <summary>Hue plus the value in degrees.</summary>
    HueOff,
    /// <summary>Opacity of the value.</summary>
    Alpha,
    /// <summary>Opacity times the value.</summary>
    AlphaMod,
}

/// <summary>One colour transform.</summary>
/// <param name="Kind">What it does.</param>
/// <param name="Value">How much: a fraction (0.75), or degrees for <see cref="ColorTransformKind.HueOff"/>.</param>
public sealed record ColorTransform(ColorTransformKind Kind, double Value);

/// <summary>
/// A shape's colour as written: exactly one base (a theme colour, an RGB colour or a preset colour name), then its
/// transforms in order. Theme colours stay theme colours; they are never only the resolved RGB.
/// </summary>
/// <param name="Theme">A theme colour.</param>
/// <param name="Rgb">An RGB colour.</param>
/// <param name="Preset">A preset colour by its CSS name, such as <c>cornflowerBlue</c>.</param>
/// <param name="Transforms">The transforms, applied in order; null for none.</param>
public sealed record ShapeColor(
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] ThemeColor? Theme = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] ColorValue? Rgb = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? Preset = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] IReadOnlyList<ColorTransform>? Transforms = null)
{
    /// <summary>A theme colour with transforms, such as accent 1 at 75% luminance.</summary>
    /// <param name="theme">The theme colour.</param>
    /// <param name="transforms">The transforms, applied in order.</param>
    public static ShapeColor FromTheme(ThemeColor theme, params ColorTransform[] transforms) => new(Theme: theme, Transforms: transforms.Length > 0 ? transforms : null);

    /// <summary>An RGB colour.</summary>
    /// <param name="rgb">The colour.</param>
    public static ShapeColor FromRgb(ColorValue rgb) => new(Rgb: rgb);
}

/// <summary>A gradient's shape.</summary>
public enum GradientShape
{
    /// <summary>Along a direction.</summary>
    Linear,
    /// <summary>From the centre outward in circles.</summary>
    Radial,
    /// <summary>From the centre outward in rectangles.</summary>
    Rectangular,
    /// <summary>Following the shape's outline.</summary>
    Path,
}

/// <summary>A gradient stop.</summary>
/// <param name="Position">0 to 1 along the gradient.</param>
/// <param name="Color">The colour there.</param>
public sealed record GradientStop(double Position, [property: JsonPropertyName("colour")] ShapeColor Color);

/// <summary>A shape's fill.</summary>
[JsonPolymorphic(TypeDiscriminatorPropertyName = "type")]
[JsonDerivedType(typeof(None), "none")]
[JsonDerivedType(typeof(Solid), "solid")]
[JsonDerivedType(typeof(Gradient), "gradient")]
[JsonDerivedType(typeof(Pattern), "pattern")]
[JsonDerivedType(typeof(Picture), "picture")]
[JsonDerivedType(typeof(FromStyle), "fromStyle")]
public abstract record FillValue
{
    private protected FillValue() { }

    /// <summary>No fill.</summary>
    public sealed record None : FillValue;

    /// <summary>One colour.</summary>
    /// <param name="Color">The colour.</param>
    public sealed record Solid([property: JsonPropertyName("colour")] ShapeColor Color) : FillValue;

    /// <summary>A gradient.</summary>
    /// <param name="Kind">Its shape.</param>
    /// <param name="Stops">Two or more stops.</param>
    /// <param name="Angle">A linear gradient's direction in degrees (0: left to right).</param>
    public sealed record Gradient(
        GradientShape Kind, IReadOnlyList<GradientStop> Stops,
        [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] double? Angle = null) : FillValue;

    /// <summary>A two-colour pattern.</summary>
    /// <param name="Preset">The ECMA-376 pattern name, such as <c>pct50</c> or <c>dkDnDiag</c>.</param>
    /// <param name="Foreground">The pattern's colour.</param>
    /// <param name="Background">The colour behind it.</param>
    public sealed record Pattern(string Preset, ShapeColor Foreground, ShapeColor Background) : FillValue;

    /// <summary>A picture.</summary>
    /// <param name="Image">The image (PNG, JPEG, GIF, BMP).</param>
    /// <param name="Tile">Tiled at its own size instead of stretched.</param>
    public sealed record Picture(
        ReadOnlyMemory<byte> Image, [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? Tile = null) : FillValue;

    /// <summary>The style reference's fill: as a command argument, it removes the explicit fill.</summary>
    public sealed record FromStyle : FillValue;
}

/// <summary>How a line is drawn along its length.</summary>
public enum LineCompound
{
    /// <summary>One line.</summary>
    Single,
    /// <summary>Two equal lines.</summary>
    Double,
    /// <summary>A thick line then a thin one.</summary>
    ThickThin,
    /// <summary>A thin line then a thick one.</summary>
    ThinThick,
    /// <summary>Three lines.</summary>
    Triple,
}

/// <summary>A line's ends.</summary>
public enum LineCap
{
    /// <summary>Square, at the end.</summary>
    Flat,
    /// <summary>Rounded.</summary>
    Round,
    /// <summary>Square, half the width past the end.</summary>
    Square,
}

/// <summary>How a line's segments join.</summary>
public enum LineJoin
{
    /// <summary>Rounded.</summary>
    Round,
    /// <summary>Cut off.</summary>
    Bevel,
    /// <summary>Pointed.</summary>
    Miter,
}

/// <summary>An arrowhead's shape.</summary>
public enum ArrowKind
{
    /// <summary>A filled triangle.</summary>
    Triangle,
    /// <summary>A notched triangle.</summary>
    Stealth,
    /// <summary>A diamond.</summary>
    Diamond,
    /// <summary>An oval.</summary>
    Oval,
    /// <summary>An open V.</summary>
    Arrow,
}

/// <summary>An arrowhead's width or length.</summary>
public enum ArrowSize
{
    /// <summary>Small.</summary>
    Small,
    /// <summary>Medium.</summary>
    Medium,
    /// <summary>Large.</summary>
    Large,
}

/// <summary>An arrowhead.</summary>
/// <param name="Kind">Its shape.</param>
/// <param name="Width">Its width.</param>
/// <param name="Length">Its length.</param>
public sealed record Arrowhead(ArrowKind Kind, ArrowSize Width = ArrowSize.Medium, ArrowSize Length = ArrowSize.Medium);

/// <summary>A line's arrowheads.</summary>
/// <param name="Head">At the start.</param>
/// <param name="Tail">At the end.</param>
public sealed record LineEnds(
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] Arrowhead? Head = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] Arrowhead? Tail = null);

/// <summary>A shape's outline.</summary>
/// <param name="Width">The width (0.75 pt when the document gives none).</param>
/// <param name="Fill"><see cref="FillValue.None"/>, <see cref="FillValue.Solid"/> or <see cref="FillValue.Gradient"/>.</param>
/// <param name="Dash">The ECMA-376 dash name, such as <c>solid</c>, <c>dash</c> or <c>sysDot</c>; <c>custom</c> for a custom dash.</param>
/// <param name="Compound">Single or several lines.</param>
/// <param name="Cap">The ends.</param>
/// <param name="Join">The joins.</param>
/// <param name="Ends">The arrowheads.</param>
public sealed record LineValue(Length Width, FillValue Fill, string Dash, LineCompound Compound, LineCap Cap, LineJoin Join, LineEnds Ends);

/// <summary>A shadow.</summary>
/// <param name="Color">Its colour, with its transparency as an <see cref="ColorTransformKind.Alpha"/> transform.</param>
/// <param name="Blur">How far it blurs.</param>
/// <param name="Distance">How far from the shape.</param>
/// <param name="Direction">Which way, in degrees clockwise from the right.</param>
public sealed record Shadow([property: JsonPropertyName("colour")] ShapeColor Color, Length Blur, Length Distance, double Direction);

/// <summary>A glow.</summary>
/// <param name="Color">Its colour.</param>
/// <param name="Radius">How far it reaches.</param>
public sealed record Glow([property: JsonPropertyName("colour")] ShapeColor Color, Length Radius);

/// <summary>A shape's effects.</summary>
/// <param name="OuterShadow">A shadow outside the shape.</param>
/// <param name="InnerShadow">A shadow inside the shape.</param>
/// <param name="Glow">A glow around the shape.</param>
/// <param name="SoftEdge">Edges faded over this distance.</param>
/// <param name="Blur">The whole shape blurred by this radius.</param>
public sealed record EffectsValue(
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] Shadow? OuterShadow = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] Shadow? InnerShadow = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] Glow? Glow = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] Length? SoftEdge = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] Length? Blur = null);

/// <summary>Where text sits in a shape vertically.</summary>
public enum TextAnchor
{
    /// <summary>At the top.</summary>
    Top,
    /// <summary>In the middle.</summary>
    Middle,
    /// <summary>At the bottom.</summary>
    Bottom,
}

/// <summary>How a shape fits its text.</summary>
public enum TextAutofit
{
    /// <summary>The shape keeps its size.</summary>
    None,
    /// <summary>The shape's height follows its text.</summary>
    Shape,
}

/// <summary>Which way a shape's text runs.</summary>
public enum TextDirection
{
    /// <summary>Left to right, lines downward.</summary>
    Horizontal,
    /// <summary>Turned 90° clockwise.</summary>
    Vertical,
    /// <summary>Turned 270° clockwise.</summary>
    Vertical270,
    /// <summary>East Asian vertical: CJK upright, other text turned.</summary>
    EastAsianVertical,
    /// <summary>Mongolian vertical.</summary>
    MongolianVertical,
    /// <summary>Letters stacked, one per line.</summary>
    Stacked,
    /// <summary>Letters stacked, right to left.</summary>
    StackedRightToLeft,
}

/// <summary>
/// The editor chrome's colour scheme: the desk, rulers, handles, guides and readouts. <see cref="Light"/>,
/// <see cref="Dark"/>, or <see cref="HighContrast"/> from the platform's colours.
/// </summary>
[JsonConverter(typeof(ChromeThemeConverter))]
public sealed record ChromeTheme
{
    private ChromeTheme(ChromeThemeKind kind, HighContrastColors? colors) => (Kind, Colors) = (kind, colors);

    /// <summary>Light chrome (the default).</summary>
    public static ChromeTheme Light { get; } = new(ChromeThemeKind.Light, null);

    /// <summary>Dark chrome.</summary>
    public static ChromeTheme Dark { get; } = new(ChromeThemeKind.Dark, null);

    /// <summary>The platform's high contrast colours. The editor controls set this themselves while the platform is in high contrast mode.</summary>
    /// <param name="colors">The platform's colours.</param>
    /// <returns>The scheme.</returns>
    public static ChromeTheme HighContrast(HighContrastColors colors) =>
        new(ChromeThemeKind.HighContrast, colors ?? throw new ArgumentNullException(nameof(colors)));

    /// <summary>Which scheme this is.</summary>
    public ChromeThemeKind Kind { get; }

    /// <summary>The platform's colours, for <see cref="ChromeThemeKind.HighContrast"/>; otherwise null.</summary>
    public HighContrastColors? Colors { get; }
}

/// <summary>The kinds of <see cref="ChromeTheme"/>.</summary>
public enum ChromeThemeKind
{
    /// <summary>Light chrome.</summary>
    Light,
    /// <summary>Dark chrome.</summary>
    Dark,
    /// <summary>The platform's high contrast colours.</summary>
    HighContrast,
}

/// <summary>The platform's colours in high contrast mode.</summary>
/// <param name="Canvas">The window background.</param>
/// <param name="CanvasText">Text on it.</param>
/// <param name="Highlight">Selected items' background.</param>
/// <param name="HighlightText">Text on it.</param>
/// <param name="GrayText">Disabled and secondary text.</param>
public sealed record HighContrastColors(ColorValue Canvas, ColorValue CanvasText, ColorValue Highlight, ColorValue HighlightText, ColorValue GrayText);

/// <summary>
/// The application's colours for the light and dark chrome schemes, to match its own UI. Each scheme given replaces the
/// last one set for it, and an empty <see cref="ChromeColors"/> puts its own colours back. The high contrast scheme
/// always takes the platform's.
/// </summary>
/// <param name="Light">For the light scheme; null keeps the last one set.</param>
/// <param name="Dark">For the dark scheme; null keeps the last one set.</param>
public sealed record ThemeColors(
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] ChromeColors? Light = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] ChromeColors? Dark = null);

/// <summary>Chrome colours in place of a scheme's own; each one left null keeps the scheme's.</summary>
/// <param name="Desk">Around the pages and the rulers' strips (a background the host sets wins around the pages).</param>
/// <param name="Surface">The rulers' strips, handle fills, the AutoCorrect options button, and the readouts' and labels' backgrounds.</param>
/// <param name="Text">Text, ruler numbers and markers, and the caret.</param>
/// <param name="SecondaryText">The rulers' unit ticks, and secondary text.</param>
/// <param name="FaintText">The rulers' fine ticks, the object grid, and faint text.</param>
/// <param name="Line">The rulers' outlines and the margins' hatching.</param>
public sealed record ChromeColors(
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] ColorValue? Desk = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] ColorValue? Surface = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] ColorValue? Text = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] ColorValue? SecondaryText = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] ColorValue? FaintText = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] ColorValue? Line = null);

/// <summary>How the document's page is drawn on screen. Saving, PDF export and printing always use the document's colours.</summary>
public enum DocumentAppearance
{
    /// <summary>In the document's colours.</summary>
    Light,
    /// <summary>Dark mode: a dark page, with the document's colours adjusted to it.</summary>
    Dark,
    /// <summary>Dark mode with the page shown in its own colours (the chrome stays dark).</summary>
    DarkWithLightPage,
}

/// <summary>How much the editor measures while you drag and hover.</summary>
public enum MeasureDetail
{
    /// <summary>A chip with the value only.</summary>
    Quiet,
    /// <summary>Dimension lines with the value; the full set while Alt is held (the default).</summary>
    Standard,
    /// <summary>Everything: changes from the start, table sizes and status lines.</summary>
    Full,
}

/// <summary>When the dot grid objects snap to shows.</summary>
public enum ObjectGrid
{
    /// <summary>Never.</summary>
    Off,
    /// <summary>While an object is dragged (the default).</summary>
    WhileDragging,
    /// <summary>Always.</summary>
    Always,
}

/// <summary>
/// A popover the engine asks the application to show by its anchor, with what it offers. Only one is open at a time;
/// <see cref="EditorSession.PopoverClosed"/> says the engine dismissed it.
/// </summary>
[JsonPolymorphic(TypeDiscriminatorPropertyName = "kind")]
[JsonDerivedType(typeof(TabType), "tabType")]
[JsonDerivedType(typeof(TabStop), "tabStop")]
[JsonDerivedType(typeof(AutoCorrect), "autoCorrect")]
public abstract record Popover
{
    private protected Popover() { }

    /// <summary>The tab type picker: answered with <see cref="EditorCommands.SetRulerTabTypeAsync"/>.</summary>
    /// <param name="Anchor">The tab type box, in client pixels.</param>
    /// <param name="Current">The type a click on the ruler adds now.</param>
    /// <param name="Choices">Every type, in order, with its name and what it does.</param>
    public sealed record TabType(RectangleF Anchor, TabSelector Current, IReadOnlyList<TabTypeChoice> Choices) : Popover;

    /// <summary>
    /// One tab stop's options: <see cref="EditorCommands.AddTabStopAsync"/> at its position changes the alignment or
    /// leader, <see cref="EditorCommands.DeleteTabStopAsync"/> clears it.
    /// </summary>
    /// <param name="Anchor">The stop's mark on the ruler, in client pixels.</param>
    /// <param name="Position">Its position, from the paragraph's left text edge.</param>
    /// <param name="Alignment">Its alignment.</param>
    /// <param name="Leader">Its leader.</param>
    public sealed record TabStop(RectangleF Anchor, Length Position, TabAlignment Alignment, TabLeader Leader) : Popover;

    /// <summary>The options of the correction just made.</summary>
    /// <param name="Anchor">The corrected text, in client pixels.</param>
    /// <param name="From">What was typed.</param>
    /// <param name="To">What it became.</param>
    public sealed record AutoCorrect(RectangleF Anchor, string From, string To) : Popover;
}

/// <summary>One entry of the tab type picker.</summary>
/// <param name="Value">The type.</param>
/// <param name="Name">Its name (<c>Decimal</c>).</param>
/// <param name="Description">What it does, in one line (<c>Numbers line up on the decimal point</c>).</param>
public sealed record TabTypeChoice(TabSelector Value, string Name, string Description);
