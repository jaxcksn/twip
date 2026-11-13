using Xunit;

namespace Twip.Tests.Pdf;

/// <summary>The headless converter: many threads, failures that stay their own, cancellation and font set lifetime.</summary>
public sealed class PdfConverterTests : IDisposable
{
    private readonly string _dir = Directory.CreateTempSubdirectory("twip-pdf-").FullName;

    private static CancellationToken Ct => TestContext.Current.CancellationToken;

    public void Dispose() => Directory.Delete(_dir, recursive: true);

    /// <summary>The fixtures the tests convert.</summary>
    private static readonly string[] Documents = ["FX-TEXT.docx", "FX-BULLETS.docx", "FX-CHECKBOXES.docx", "FX-EMPTY-TABLE.docx"];

    /// <summary>Bytes as Latin-1 text.</summary>
    private static string Latin1(byte[] bytes) => new([.. bytes.Select(b => (char)b)]);

    /// <summary>A PDF as text without what differs between two writes: the document id, uuids and dates.</summary>
    private static string Undated(byte[] pdf)
    {
        var text = Latin1(pdf);
        // The id (derived from the creation time) goes, to the trailer's end: "/ID [" or, compact, "/ID[".
        text = System.Text.RegularExpressions.Regex.Replace(text, @"/ID ?\[.*?startxref", "startxref", System.Text.RegularExpressions.RegexOptions.Singleline);
        text = System.Text.RegularExpressions.Regex.Replace(text, @"uuid:[0-9a-f-]{36}", "uuid:");
        text = System.Text.RegularExpressions.Regex.Replace(text, @"D:\d{14}", "D:");
        return System.Text.RegularExpressions.Regex.Replace(text, @"\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d", "T");
    }

    /// <summary>A PDF's page count, from its page objects.</summary>
    private static int Pages(byte[] pdf)
    {
        var text = Latin1(pdf);
        var count = 0;
        for (var i = text.IndexOf("/Type /Page", StringComparison.Ordinal); i >= 0; i = text.IndexOf("/Type /Page", i + 1, StringComparison.Ordinal))
        {
            if (i + 11 >= text.Length || text[i + 11] != 's') count++;
        }
        return count;
    }

    [Fact]
    public async Task SixteenConcurrentConversionsMatchTheirSequentialResults()
    {
        Native.Require();
        var inputs = Enumerable.Range(0, 16).Select(i => File.ReadAllBytes(Native.Fixture(Documents[i % Documents.Length]))).ToArray();
        var sequential = inputs.Select(d => PdfConverter.Convert(d)).ToArray();
        var concurrent = await Task.WhenAll(inputs.Select(d => PdfConverter.ConvertAsync(d, ct: Ct)));
        for (var i = 0; i < inputs.Length; i++)
        {
            Assert.StartsWith("%PDF", Latin1(concurrent[i].Bytes), StringComparison.Ordinal);
            Assert.Equal(Pages(sequential[i].Bytes), Pages(concurrent[i].Bytes));
            Assert.True(Undated(sequential[i].Bytes) == Undated(concurrent[i].Bytes), $"document {i}: the concurrent conversion differs");
        }
    }

    [Fact]
    public async Task ACorruptDocumentFailsAloneAmongValidOnes()
    {
        Native.Require();
        var valid = File.ReadAllBytes(Native.Fixture("FX-TEXT.docx"));
        var corrupt = valid.Take(valid.Length / 2).ToArray();
        var tasks = new[] { valid, corrupt, valid }.Select(d => PdfConverter.ConvertAsync(d, ct: Ct)).ToArray();
        await Assert.ThrowsAnyAsync<Exception>(() => Task.WhenAll(tasks));
        Assert.True(tasks[0].Status == TaskStatus.RanToCompletion && tasks[2].Status == TaskStatus.RanToCompletion);
        var error = Assert.IsType<EditorException>(tasks[1].Exception!.InnerException);
        Assert.Equal(EditorErrorCode.ImportFailed, error.Code);
    }

    [Fact]
    public async Task CancellingStopsAndLeavesNoFile()
    {
        Native.Require();
        var output = Path.Combine(_dir, "cancelled.pdf");
        using var cancel = new CancellationTokenSource();
        cancel.Cancel();
        await Assert.ThrowsAnyAsync<OperationCanceledException>(() => PdfConverter.ConvertAsync(Native.Fixture("FX-TEXT.docx"), output, ct: cancel.Token));
        Assert.False(File.Exists(output));
        Assert.Empty(Directory.EnumerateFiles(_dir));
    }

    [Fact]
    public async Task DisposingAFontSetDuringConversionsIsSafe()
    {
        Native.Require();
        var fonts = PdfFontSet.Create(new PdfFontSetOptions { IncludeSystemFonts = true });
        var docx = File.ReadAllBytes(Native.Fixture("FX-TEXT.docx"));
        var running = Enumerable.Range(0, 4).Select(_ => PdfConverter.ConvertAsync(docx, fonts, ct: Ct)).ToArray();
        fonts.Dispose();
        // Each either ran with its own reference or started after the dispose and saw it.
        foreach (var task in running)
        {
            try { Assert.NotEmpty((await task).Bytes); }
            catch (ObjectDisposedException) { }
        }
        Assert.Throws<ObjectDisposedException>(() => PdfConverter.Convert(docx, fonts));
    }

    [Fact]
    public void AFontSetWithNoFacesCannotDraw()
    {
        Native.Require();
        using var empty = PdfFontSet.Create(new PdfFontSetOptions());
        var error = Assert.Throws<EditorException>(() => PdfConverter.Convert(File.ReadAllBytes(Native.Fixture("FX-TEXT.docx")), empty));
        Assert.Equal(EditorErrorCode.ExportFailed, error.Code);
    }

    [Fact]
    public void AnEmptyPasswordIsRejectedBeforeConverting()
    {
        Assert.Throws<ArgumentException>(() => PdfConverter.Convert([1, 2, 3], options: new PdfOptions { OpenPassword = "" }));
        Assert.DoesNotContain("secret", new PdfOptions { OpenPassword = "secret" }.ToString(), StringComparison.Ordinal);
    }

    [Fact]
    public async Task StreamsStayOpenAndFontFilesLoad()
    {
        Native.Require();
        using var fonts = PdfFontSet.Create(new PdfFontSetOptions { FontFiles = [Native.Fixture("fonts/stand-in-serif.ttf")] });
        Assert.NotEmpty(fonts.Faces);
        Assert.Contains(fonts.Faces[0].Family, fonts.Families);
        using var input = File.OpenRead(Native.Fixture("FX-TEXT.docx"));
        using var output = new MemoryStream();
        await PdfConverter.ConvertAsync(input, output, fonts, ct: Ct);
        Assert.True(input.CanRead && output.CanWrite);
        Assert.True(output.Length > 0);
    }
}
