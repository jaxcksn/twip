using System.Text.Json.Serialization;
using Twip.Wire;

namespace Twip;

/// <summary>
/// An alias: when <paramref name="From"/> (optionally one style of it) isn't in the library, it is drawn with
/// <paramref name="To"/>, another family or the default family of a generic category. Aliases chain up to 8 hops. The
/// document keeps naming <paramref name="From"/>.
/// </summary>
/// <param name="From">The family the document names, case-insensitive.</param>
/// <param name="To">The family, or the generic category, that draws instead.</param>
/// <param name="Bold">Only for bold (true) or regular (false) faces.</param>
/// <param name="Italic">Only for italic (true) or upright (false) faces.</param>
public sealed record Alias(
    string From, AliasTarget To,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? Bold = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? Italic = null);

/// <summary>What an <see cref="Alias"/> draws with: another family, or the default family of a generic category.</summary>
[JsonConverter(typeof(AliasTargetConverter))]
public sealed class AliasTarget : IEquatable<AliasTarget>
{
    private AliasTarget(string? family, GenericFamily? category) => (Family, Category) = (family, category);

    /// <summary>The family, when the target is one.</summary>
    public string? Family { get; }

    /// <summary>The category, when the target is one. When its default is unset or missing, the general fallback draws.</summary>
    public GenericFamily? Category { get; }

    /// <summary>Another family; its own aliases apply when it is missing too.</summary>
    /// <param name="family">The family.</param>
    /// <returns>The target.</returns>
    public static AliasTarget ToFamily(string family)
    {
        ArgumentNullException.ThrowIfNull(family);
        return new(family, null);
    }

    /// <summary>The default family of a generic category (<see cref="GenericDefaults"/>).</summary>
    /// <param name="category">The category.</param>
    /// <returns>The target.</returns>
    public static AliasTarget ToCategory(GenericFamily category) => new(null, category);

    /// <summary>Another family.</summary>
    /// <param name="family">The family.</param>
    public static implicit operator AliasTarget(string family) => ToFamily(family);

    /// <inheritdoc/>
    public bool Equals(AliasTarget? other) => other is not null && Family == other.Family && Category == other.Category;

    /// <inheritdoc/>
    public override bool Equals(object? obj) => Equals(obj as AliasTarget);

    /// <inheritdoc/>
    public override int GetHashCode() => (Family?.GetHashCode() ?? 0) ^ (Category?.GetHashCode() ?? 0);

    /// <inheritdoc/>
    public override string ToString() => Family ?? $"{Category}";
}

/// <summary>
/// The family that draws a missing font of each generic category. Opt-in: every category is unset by default, and
/// with none set no missing font is drawn through a category. A missing family's category comes from the document's
/// font table, from an <see cref="Alias"/> to a category, or from a resolver's answer.
/// </summary>
/// <param name="Serif">The family for missing serif fonts.</param>
/// <param name="SansSerif">The family for missing sans-serif fonts.</param>
/// <param name="Monospace">The family for missing fixed-pitch fonts.</param>
/// <param name="Script">The family for missing script fonts.</param>
/// <param name="Decorative">The family for missing decorative fonts.</param>
public sealed record GenericDefaults(
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? Serif = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? SansSerif = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? Monospace = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? Script = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? Decorative = null);

/// <summary>The face for characters no other font has: the bundled symbol font, none, or a font of the application's.</summary>
public sealed class SymbolFallback
{
    private SymbolFallback(string? name, ReadOnlyMemory<byte>? data, string? path) => (Name, Data, Path) = (name, data, path);

    internal string? Name { get; }

    internal ReadOnlyMemory<byte>? Data { get; }

    internal string? Path { get; }

    /// <summary>The bundled symbol font: bullets, checkboxes, arrows, check marks, card suits and common math signs.</summary>
    public static SymbolFallback Bundled { get; } = new("bundled", null, null);

    /// <summary>None: a character no font has draws as its font's missing-glyph box.</summary>
    public static SymbolFallback Off { get; } = new("off", null, null);

    /// <summary>A font's first face: TrueType, OpenType, a collection, WOFF or WOFF2.</summary>
    /// <param name="data">The font file.</param>
    /// <returns>The fallback.</returns>
    public static SymbolFallback FromBytes(ReadOnlyMemory<byte> data) => new(null, data, null);

    /// <summary>A font file's first face, read when it is set.</summary>
    /// <param name="path">The font file.</param>
    /// <returns>The fallback.</returns>
    public static SymbolFallback FromFile(string path)
    {
        ArgumentNullException.ThrowIfNull(path);
        return new(null, null, path);
    }
}
