using System.Text.RegularExpressions;
using Twip;

namespace Examples;

static class FontsExample
{
    static async Task Run(HttpClient http)
    {
        #region adding
        // The system's fonts, shared per process. FontLibrary.CreateEmpty() starts with none.
        var fonts = FontLibrary.System;

        // Files you ship: TTF, OTF, TTC, WOFF or WOFF2, as bytes or a path. Each returns the faces it added.
        await fonts.AddAsync("fonts/Carlito-Regular.ttf");
        #endregion

        #region aliases
        // When a document asks for Arial and the library doesn't have it, draw Arimo. A family the library has is
        // never aliased, the document keeps naming Arial, and chains (A → B → C) are followed.
        fonts.SetAliases(new Dictionary<string, string> { ["Arial"] = "Arimo", ["Times New Roman"] = "Tinos" });

        // Per style, or to a generic category's default.
        fonts.SetAliases([
            new Alias("Arial", "Arimo"),
            new Alias("Garamond", "EB Garamond SemiBold", Bold: true),
            new Alias("Brand Script", AliasTarget.ToCategory(GenericFamily.Script)),
        ]);
        #endregion

        #region replacements
        // Always draw Comic Sans MS with the house font, even where Comic Sans MS is installed. The document is unchanged.
        fonts.SetReplacements(new Dictionary<string, string> { ["Comic Sans MS"] = "Inter" });
        #endregion

        #region genericDefaults
        // Opt-in: a missing family draws with the default of its category, which the document's font table gives it.
        fonts.SetGenericDefaults(new GenericDefaults(Serif: "Tinos", SansSerif: "Arimo", Monospace: "Cousine", Script: "Dancing Script", Decorative: "Lobster"));

        // Classify only through aliases to a category and resolver answers, not the font table.
        fonts.SetClassification(fromDocument: false);

        // new GenericDefaults() turns generic defaults off again.
        fonts.SetGenericDefaults(new GenericDefaults());
        #endregion

        #region fallback
        // The family for anything nothing else resolves. With null, the first face added draws.
        fonts.SetFallback("Arimo");
        #endregion

        #region characterFallback
        // Searched in order for a character the run's font doesn't have, then the symbol fallback.
        fonts.SetCharacterFallback(["Noto Sans Symbols 2", "Noto Sans CJK JP", "Noto Color Emoji"]);
        #endregion

        #region symbolFallback
        // The bundled symbol font draws bullets and checkboxes no other font has. Replace it, or turn it off.
        await fonts.SetSymbolFallbackAsync(SymbolFallback.FromFile("fonts/MySymbols.ttf"));
        await fonts.SetSymbolFallbackAsync(SymbolFallback.Off);
        await fonts.SetSymbolFallbackAsync(SymbolFallback.Bundled);
        #endregion

        #region resolver
        // Asked once per family and style the document uses and the library lacks. Return the font, or null. It runs
        // on the thread pool; an exception reports the family missing with the error.
        fonts.SetResolver(async (request, ct) =>
        {
            var style = (request.Bold, request.Italic) switch
            {
                (true, true) => "BoldItalic",
                (true, false) => "Bold",
                (false, true) => "Italic",
                _ => "Regular",
            };
            var response = await http.GetAsync($"https://fonts.example.com/{request.Family}-{style}.ttf", ct);
            return response.IsSuccessStatusCode ? FontSource.FromBytes(await response.Content.ReadAsByteArrayAsync(ct)) : null;
        });
        #endregion

        #region resolverCategory
        // Or decide how to draw it: another family, or a category's default. The request says what the document's
        // font table states about the family (Description) and the category that gives (Proposed).
        fonts.SetResolver((request, _) => ValueTask.FromResult(request switch
        {
            { Family: "Corporate Sans" } => FontSource.FromFamily("Inter"),
            { Proposed: { } category } => FontSource.FromCategory(category),
            { Description.Pitch: FontPitch.Fixed } => FontSource.FromCategory(GenericFamily.Monospace),
            _ => null,
        }));
        #endregion

        #region reports
        await using var session = new EditorSession(new EditorSessionOptions(Fonts: fonts));
        session.FontsChanged += (_, e) =>
        {
            foreach (var m in e.Missing)
                Console.WriteLine($"{m.Family} {(m.Bold ? "bold " : "")}{(m.Italic ? "italic" : "")}: drawn with {m.DrawnWith ?? "nothing"} ({m.By}){(m.Error is { } error ? $", {error}" : "")}");
            foreach (var c in e.Characters)
                Console.WriteLine($"no font has {c.Characters} ({c.Family})");
            Console.WriteLine($"{e.Pending.Count} still loading");
        };
        Console.WriteLine(string.Join(", ", await fonts.GetFamiliesAsync()));
        #endregion

        #region replaceFont
        // Rewrite the document itself: every Calibri becomes Aptos, in the text, styles, theme, numbering and font table.
        var result = await session.Commands.ReplaceFontAsync("Calibri", "Aptos");
        Console.WriteLine($"{result.Changed} places changed, {result.Skipped} in locked content controls");

        // Only the selected text, or only bold text.
        await session.Commands.ReplaceFontAsync("Calibri", "Aptos", ReplaceScope.Selection);
        await session.Commands.ReplaceFontAsync("Calibri", "Aptos Display", bold: true);
        #endregion

        #region recommended
        // Metric-compatible open families for common office fonts: the same widths, so the same line and page breaks.
        fonts.SetAliases(new Dictionary<string, string>
        {
            ["Calibri"] = "Carlito",
            ["Calibri Light"] = "Carlito",
            ["Cambria"] = "Caladea",
            ["Arial"] = "Arimo",
            ["Helvetica"] = "Arimo",
            ["Times New Roman"] = "Tinos",
            ["Times"] = "Tinos",
            ["Courier New"] = "Cousine",
            ["Courier"] = "Cousine",
        });
        fonts.SetGenericDefaults(new GenericDefaults(Serif: "Tinos", SansSerif: "Arimo", Monospace: "Cousine"));
        fonts.SetFallback("Arimo");
        #endregion

        #region googleFonts
        // Google Fonts through a resolver: the css2 stylesheet for a family and style (TTF for clients that send no user
        // agent), then the font file it names.
        fonts.SetResolver(async (request, ct) =>
        {
            var family = Uri.EscapeDataString(request.Family).Replace("%20", "+", StringComparison.Ordinal);
            var css = $"https://fonts.googleapis.com/css2?family={family}:ital,wght@{(request.Italic ? 1 : 0)},{(request.Bold ? 700 : 400)}";
            using var response = await http.GetAsync(css, ct);
            if (!response.IsSuccessStatusCode) return null; // Google doesn't have this family or face
            var url = Regex.Match(await response.Content.ReadAsStringAsync(ct), @"src:\s*url\(([^)]+)\)");
            return url.Success ? FontSource.FromUri(new Uri(url.Groups[1].Value)) : null;
        });
        #endregion
    }
}
