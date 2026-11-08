using System.Text.Json;
using Twip.Engine;
using Twip.Native;
using Twip.Wire;

namespace Twip;

/// <summary>A converted document.</summary>
/// <param name="Bytes">The PDF.</param>
/// <param name="Warnings">
/// What the PDF couldn't keep exactly: the read's approximations, text drawn in another font, and what the options
/// dropped.
/// </param>
/// <param name="Fonts">The document's fonts the set couldn't draw as the document asks, and the characters no font has.</param>
public sealed record PdfConversion(byte[] Bytes, IReadOnlyList<Warning> Warnings, FontsReport Fonts);

/// <summary>
/// Converts DOCX to PDF without an editor session: the final document, without revision marks or comments. Every
/// method is safe to call from any number of threads at the same time, and no conversion waits on another. Each runs
/// on the calling thread (<see cref="Convert"/>) or a thread pool thread (the async methods), never on the editor's
/// engine thread.
/// </summary>
public static class PdfConverter
{
    /// <summary>Converts DOCX bytes on the calling thread.</summary>
    /// <param name="docx">The document.</param>
    /// <param name="fonts">The fonts to draw with; <see cref="PdfFontSet.System"/> when null.</param>
    /// <param name="options">An open password and keywords; none when null.</param>
    /// <returns>The PDF and its warnings.</returns>
    /// <exception cref="ArgumentException">The open password is empty.</exception>
    /// <exception cref="ObjectDisposedException">The font set was disposed.</exception>
    /// <exception cref="EditorException">
    /// The bytes aren't a readable DOCX (<see cref="EditorErrorCode.ImportFailed"/>), the document is encrypted or
    /// binary (<see cref="EditorErrorCode.Unsupported"/>), or the font set has no faces
    /// (<see cref="EditorErrorCode.ExportFailed"/>).
    /// </exception>
    public static PdfConversion Convert(ReadOnlySpan<byte> docx, PdfFontSet? fonts = null, PdfOptions? options = null)
    {
        options?.Validate();
        return Run(docx, fonts ?? PdfFontSet.System, options, default);
    }

    /// <summary>Converts DOCX bytes on a thread pool thread.</summary>
    /// <param name="docx">The document.</param>
    /// <param name="fonts">The fonts to draw with; <see cref="PdfFontSet.System"/> when null.</param>
    /// <param name="options">An open password and keywords; none when null.</param>
    /// <param name="ct">Cancels the conversion.</param>
    /// <returns>The PDF and its warnings.</returns>
    /// <exception cref="ArgumentException">The open password is empty.</exception>
    /// <exception cref="OperationCanceledException">The conversion was cancelled.</exception>
    /// <exception cref="EditorException">As for <see cref="Convert"/>.</exception>
    public static Task<PdfConversion> ConvertAsync(
        ReadOnlyMemory<byte> docx, PdfFontSet? fonts = null, PdfOptions? options = null, CancellationToken ct = default)
    {
        options?.Validate();
        var set = fonts ?? PdfFontSet.System;
        return Task.Run(() => Run(docx.Span, set, options, ct), ct);
    }

    /// <summary>Converts a DOCX stream into a PDF stream. Both streams stay open.</summary>
    /// <param name="docx">The document, read to its end.</param>
    /// <param name="pdf">Where the PDF is written. A failed write may leave partial output.</param>
    /// <param name="fonts">The fonts to draw with; <see cref="PdfFontSet.System"/> when null.</param>
    /// <param name="options">An open password and keywords; none when null.</param>
    /// <param name="ct">Cancels the conversion.</param>
    /// <returns>The warnings.</returns>
    /// <exception cref="ArgumentException">The open password is empty.</exception>
    /// <exception cref="OperationCanceledException">The conversion was cancelled.</exception>
    /// <exception cref="EditorException">As for <see cref="Convert"/>.</exception>
    public static async Task<IReadOnlyList<Warning>> ConvertAsync(
        Stream docx, Stream pdf, PdfFontSet? fonts = null, PdfOptions? options = null, CancellationToken ct = default)
    {
        ArgumentNullException.ThrowIfNull(docx);
        ArgumentNullException.ThrowIfNull(pdf);
        options?.Validate();
        var input = new MemoryStream();
        await docx.CopyToAsync(input, 81920, ct).ConfigureAwait(false);
        var result = await ConvertAsync(input.GetBuffer().AsMemory(0, (int)input.Length), fonts, options, ct).ConfigureAwait(false);
        await pdf.WriteAsync(result.Bytes, ct).ConfigureAwait(false);
        return result.Warnings;
    }

    /// <summary>
    /// Converts a DOCX file into a PDF file. The PDF is written to a temporary file beside it and moved into place, so a
    /// failed conversion leaves an existing file as it was.
    /// </summary>
    /// <param name="docxPath">The document.</param>
    /// <param name="pdfPath">The PDF, replaced if it exists.</param>
    /// <param name="fonts">The fonts to draw with; <see cref="PdfFontSet.System"/> when null.</param>
    /// <param name="options">An open password and keywords; none when null.</param>
    /// <param name="ct">Cancels the conversion.</param>
    /// <returns>The warnings.</returns>
    /// <exception cref="ArgumentException">The open password is empty.</exception>
    /// <exception cref="OperationCanceledException">The conversion was cancelled.</exception>
    /// <exception cref="EditorException">As for <see cref="Convert"/>.</exception>
    public static async Task<IReadOnlyList<Warning>> ConvertAsync(
        string docxPath, string pdfPath, PdfFontSet? fonts = null, PdfOptions? options = null, CancellationToken ct = default)
    {
        ArgumentNullException.ThrowIfNull(docxPath);
        ArgumentNullException.ThrowIfNull(pdfPath);
        options?.Validate();
        var bytes = await File.ReadAllBytesAsync(docxPath, ct).ConfigureAwait(false);
        var result = await ConvertAsync(bytes, fonts, options, ct).ConfigureAwait(false);
        var full = Path.GetFullPath(pdfPath);
        var temp = Path.Combine(Path.GetDirectoryName(full)!, $".{Path.GetFileName(full)}.{Guid.NewGuid():N}.tmp");
        try
        {
            await File.WriteAllBytesAsync(temp, result.Bytes, ct).ConfigureAwait(false);
            File.Move(temp, full, overwrite: true);
        }
        finally
        {
            if (File.Exists(temp)) File.Delete(temp);
        }
        return result.Warnings;
    }

    private static unsafe PdfConversion Run(ReadOnlySpan<byte> docx, PdfFontSet fonts, PdfOptions? options, CancellationToken ct)
    {
        ct.ThrowIfCancellationRequested();
        var json = WireJson.ToUtf8(new PdfOptionsWire(options?.OpenPassword, options?.Keywords is { Count: > 0 } k ? k : null));
        using var set = fonts.Use();
        var cancel = ct.CanBeCanceled ? TwipNative.PdfCancelNew() : 0;
        TwipBuf pdf = default, meta = default;
        int status;
        try
        {
            // Disposing the registration waits for a running callback, so the flag is freed only after it.
            using (ct.CanBeCanceled ? ct.Register(() => TwipNative.PdfCancelSet(cancel)) : default)
            {
                fixed (byte* d = docx)
                fixed (byte* o = json)
                {
                    status = TwipNative.PdfConvert(d, (nuint)docx.Length, set.Pointer, o, (nuint)json.Length, cancel, &pdf, &meta);
                }
            }
            var info = WireJson.Unwrap(meta.Span, status)?.Deserialize(TwipJsonContext.Default.PdfConvertMeta)
                ?? throw new EditorException(EditorErrorCode.Internal, "the conversion returned no result");
            return new PdfConversion(pdf.Span.ToArray(), info.Warnings, info.Fonts);
        }
        catch (EditorException e) when (e.Code == EditorErrorCode.Aborted)
        {
            throw new OperationCanceledException(e.Message, e, ct);
        }
        finally
        {
            EngineThread.Free(ref pdf);
            EngineThread.Free(ref meta);
            if (cancel != 0) TwipNative.PdfCancelFree(cancel);
        }
    }
}
