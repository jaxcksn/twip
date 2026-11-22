using System.Text.Json.Nodes;

namespace Twip.Tests;

/// <summary>
/// The test font kit <c>crates/twip/tests/fonts/kit.json</c> lists, its faces and aliases: what the tests lay out with,
/// so results don't depend on the machine's fonts.
/// </summary>
internal static class Kit
{
    private static readonly Lazy<(string[] Files, Alias[] Aliases)> Listed = new(Read);
    private static readonly Lazy<FontLibrary> Shared = new(Create);
    private static readonly Lazy<PdfFontSet> SharedPdf = new(() => PdfFontSet.Create(PdfOptions));

    /// <summary>A library with the kit, shared by the tests that don't change its settings.</summary>
    public static FontLibrary Library => Shared.Value;

    /// <summary>A PDF font set with the kit, shared by the conversions.</summary>
    public static PdfFontSet PdfFonts => SharedPdf.Value;

    /// <summary>Options for a PDF font set with the kit.</summary>
    public static PdfFontSetOptions PdfOptions => new() { FontFiles = Listed.Value.Files, Aliases = Listed.Value.Aliases };

    /// <summary>A session laying out with <see cref="Library"/>.</summary>
    public static EditorSession Session() => new(new EditorSessionOptions(Fonts: Library));

    /// <summary>A new library with the kit, for a test that changes its settings.</summary>
    public static FontLibrary Create()
    {
        var library = FontLibrary.CreateEmpty();
        // The library adds on the engine's thread: waiting here can't deadlock.
        foreach (var file in Listed.Value.Files) library.AddAsync(file).GetAwaiter().GetResult();
        library.SetAliases(Listed.Value.Aliases);
        return library;
    }

    private static (string[] Files, Alias[] Aliases) Read()
    {
        var dir = Repo.Path("crates", "twip", "tests", "fonts");
        var kit = JsonNode.Parse(File.ReadAllText(Path.Combine(dir, "kit.json")))!;
        var files = kit["files"]!.AsArray().Select(f => Path.Combine(dir, f!.GetValue<string>())).ToArray();
        var aliases = kit["aliases"]!.AsObject().Select(a => new Alias(a.Key, a.Value!.GetValue<string>())).ToArray();
        return (files, aliases);
    }
}
