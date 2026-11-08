namespace Twip;

/// <summary>
/// What a PDF writes beyond the pages: an open password and keywords. Used by
/// <see cref="EditorSession.ExportPdfAsync(string, PdfOptions?, CancellationToken)"/> and <see cref="PdfConverter"/>.
/// The default writes neither.
/// </summary>
/// <remarks>A class rather than a record, so no generated <c>ToString</c> or equality can expose the password.</remarks>
public sealed class PdfOptions
{
    /// <summary>
    /// Encrypts the PDF with AES-256: readers ask for this password before showing anything. Must not be empty. A
    /// protected PDF doesn't claim PDF/A conformance, and the export warns that the claim was dropped.
    /// </summary>
    public string? OpenPassword { get; init; }

    /// <summary>Written to the PDF's keywords metadata, in order. Blank entries are dropped.</summary>
    public IReadOnlyList<string> Keywords { get; init; } = [];

    /// <summary>The options, with the password hidden.</summary>
    /// <returns>A description that never contains the password.</returns>
    public override string ToString() =>
        $"PdfOptions {{ OpenPassword = {(OpenPassword is null ? "null" : "***")}, Keywords = [{string.Join(", ", Keywords)}] }}";

    /// <summary>Throws for an empty password, which would write a PDF that looks protected and isn't.</summary>
    internal void Validate()
    {
        if (OpenPassword is { Length: 0 }) throw new ArgumentException("The open password is empty.", nameof(OpenPassword));
    }
}
