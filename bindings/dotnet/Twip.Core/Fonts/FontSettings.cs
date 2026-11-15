using System.Text.Json.Serialization;
using Twip.Wire;

namespace Twip;

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
