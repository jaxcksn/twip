using Twip;

namespace Examples;

static class PdfConversionExample
{
    static async Task Batch(string[] files)
    {
        #region batch
        // One font set for every conversion, on any thread. Build it once; it never changes.
        using var fonts = PdfFontSet.Create(new PdfFontSetOptions
        {
            IncludeSystemFonts = true,
            FontFiles = ["fonts/Carlito-Regular.ttf"],
            Substitutions = [new SubstitutionRule("Calibri", "Carlito")],
        });

        // Each conversion runs on a thread pool thread; none waits on another.
        var warnings = await Task.WhenAll(files.Select(f => PdfConverter.ConvertAsync(f, Path.ChangeExtension(f, ".pdf"), fonts)));
        foreach (var w in warnings.SelectMany(w => w)) Console.WriteLine($"{w.Code}: {w.Message}");
        #endregion
    }

    static async Task Options(byte[] docx, EditorSession session, CancellationToken ct)
    {
        #region options
        var options = new PdfOptions { OpenPassword = "s3cret", Keywords = ["2026-CV-1042", "notice"] };

        // Headless, from bytes, cancellable.
        PdfConversion pdf = await PdfConverter.ConvertAsync(docx, options: options, ct: ct);
        await File.WriteAllBytesAsync("notice.pdf", pdf.Bytes, ct);

        // The same options on an editor session's export.
        await session.ExportPdfAsync("notice-edited.pdf", options, ct);
        #endregion
    }
}
