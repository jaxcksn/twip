using System.Text.Json.Serialization;

namespace Twip;

/// <summary>A generic font category, like the CSS generic families.</summary>
public enum GenericFamily
{
    /// <summary>Fonts with serifs (<c>roman</c> in a document's font table).</summary>
    Serif,
    /// <summary>Fonts without serifs (<c>swiss</c>).</summary>
    SansSerif,
    /// <summary>Fixed-pitch fonts (<c>modern</c>, or any font the document says is fixed pitch).</summary>
    Monospace,
    /// <summary>Script and handwriting fonts (<c>script</c>).</summary>
    Script,
    /// <summary>Decorative and display fonts (<c>decorative</c>).</summary>
    Decorative,
}

/// <summary>Which stand-in draws a missing family.</summary>
public enum StandIn
{
    /// <summary>A replacement's target is missing, so the document's own family draws.</summary>
    Original,
    /// <summary>The default family of its generic category.</summary>
    Generic,
    /// <summary>The general fallback family.</summary>
    Fallback,
    /// <summary>The first face added to the library (or the symbol fallback, in an empty library).</summary>
    LastResort,
}

/// <summary>A family, in one style, that draws with a stand-in.</summary>
/// <param name="Family">The family as the document names it, or the replacement target when <paramref name="By"/> is <see cref="StandIn.Original"/>.</param>
/// <param name="Bold">The bold style.</param>
/// <param name="Italic">The italic style.</param>
/// <param name="Tried">The replacement target and alias targets tried, in order.</param>
/// <param name="Category">The generic category the family has, from the document's font table, an alias or a resolver.</param>
/// <param name="DrawnWith">The family that draws instead; null when nothing draws (an empty library with no symbol fallback).</param>
/// <param name="By">Why that family draws.</param>
/// <param name="Error">The resolver's failure, when it failed.</param>
public sealed record MissingFont(
    string Family,
    bool Bold,
    bool Italic,
    IReadOnlyList<string> Tried,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] GenericFamily? Category,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? DrawnWith,
    StandIn By,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? Error);

/// <summary>Characters no font has, by the family their runs name.</summary>
/// <param name="Family">The family the runs name.</param>
/// <param name="Characters">The distinct characters, in code point order, at most 64.</param>
/// <param name="Truncated">More than 64 characters are missing.</param>
public sealed record MissingCharacters(string Family, string Characters, bool Truncated);

/// <summary>
/// The document's families (per style) that drew with a stand-in in a conversion, and the characters no font has.
/// </summary>
/// <param name="Missing">Families that drew with a stand-in.</param>
/// <param name="Characters">Characters no font has, by the family their runs name.</param>
public sealed record FontsReport(IReadOnlyList<MissingFont> Missing, IReadOnlyList<MissingCharacters> Characters);

/// <summary>Where <see cref="EditorCommands.ReplaceFontAsync"/> rewrites.</summary>
public enum ReplaceScope
{
    /// <summary>The whole document: every story, and the tables below the text.</summary>
    Document,
    /// <summary>The selected text.</summary>
    Selection,
}

/// <summary>What <see cref="EditorCommands.ReplaceFontAsync"/> rewrote.</summary>
/// <param name="Changed">The places rewritten: runs, paragraph marks, symbols, styles, document defaults, numbering levels, theme font slots and font table entries.</param>
/// <param name="Skipped">The places inside locked content controls, left as they were.</param>
public sealed record FontReplaceResult(int Changed, int Skipped);
