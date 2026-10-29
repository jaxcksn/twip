using Twip;

namespace Examples;

static class FontsExample
{
    static async Task Run(HttpClient http)
    {
        #region fonts
        // The system's fonts, shared per process. FontLibrary.CreateEmpty() starts with none.
        var fonts = FontLibrary.System;

        await fonts.AddAsync("fonts/Carlito-Regular.ttf");
        fonts.Substitute(new Dictionary<string, string> { ["Calibri"] = "Carlito", ["Cambria"] = "Caladea" });
        fonts.SetDefault("Carlito");

        // Fetch a family the first time a document needs it; null when you don't have it.
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
        fonts.UseGoogleFonts(); // what the resolver doesn't have

        await using var session = new EditorSession(new EditorSessionOptions(Fonts: fonts));
        session.FontsChanged += (_, e) => Console.WriteLine($"unresolved: {string.Join(", ", e.Unresolved)}");
        #endregion
    }
}
