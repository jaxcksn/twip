using System.Text.Json.Serialization;

namespace Twip;

/// <summary>
/// The document's families (per style) that drew with a stand-in in a conversion, and the characters no font has.
/// </summary>
/// <param name="Missing">Families that drew with a stand-in.</param>
/// <param name="Characters">Characters no font has, by the family their runs name.</param>
public sealed record FontsReport(IReadOnlyList<MissingFont> Missing, IReadOnlyList<MissingCharacters> Characters);

/// <summary>What <see cref="EditorCommands.ReplaceFontAsync"/> rewrote.</summary>
/// <param name="Changed">The places rewritten: runs, paragraph marks, symbols, styles, document defaults, numbering levels, theme font slots and font table entries.</param>
/// <param name="Skipped">The places inside locked content controls, left as they were.</param>
public sealed record FontReplaceResult(int Changed, int Skipped);
