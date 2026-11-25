using System.Text;
using Twip.Hosting;

namespace Twip.WinForms;

/// <summary>The WinForms side of the hosting contract: the control's UI thread, its bitmap, and the OS services.</summary>
internal sealed class WinFormsAdapter(DocumentEditor control) : IEditorHostAdapter, IEditorDispatcher
{
    public IEditorDispatcher Dispatcher => this;

    public void Post(Action action)
    {
        if (control.IsDisposed || !control.IsHandleCreated) throw new InvalidOperationException("the control has no window");
        control.BeginInvoke(action);
    }

    public bool CheckAccess() => !control.InvokeRequired;

    public void RequestRender()
    {
        control.UpdateScrollbars();
        control.Invalidate();
    }

    public void Present(in FrameView frame) => control.Blit(frame);

    public void SetCursor(EditorCursor cursor) => control.Cursor = cursor switch
    {
        EditorCursor.Text => Cursors.IBeam,
        EditorCursor.Pointer => Cursors.Hand,
        EditorCursor.Move => Cursors.SizeAll,
        EditorCursor.ResizeHorizontal => Cursors.SizeWE,
        EditorCursor.ResizeVertical => Cursors.SizeNS,
        EditorCursor.ResizeNorthWestSouthEast => Cursors.SizeNWSE,
        EditorCursor.ResizeNorthEastSouthWest => Cursors.SizeNESW,
        EditorCursor.Crosshair => Cursors.Cross,
        EditorCursor.NotAllowed => Cursors.No,
        EditorCursor.Copy => Cursors.Arrow,
        EditorCursor.CellSelect => CellSelect.Value,
        EditorCursor.ColumnSelect => ColumnSelect.Value,
        EditorCursor.RowSelect => RowSelect.Value,
        EditorCursor.Rotate => Rotate.Value,
        // Click and Type's I-beams and the highlighter pen: Windows has no cursor for them, the I-beam stands in.
        EditorCursor.ClickAndTypeLeft or EditorCursor.ClickAndTypeLeftIndent or EditorCursor.ClickAndTypeCenter or EditorCursor.ClickAndTypeRight => Cursors.IBeam,
        EditorCursor.Highlighter => Cursors.IBeam,
        EditorCursor.VerticalText => VerticalText.Value,
        EditorCursor.HideWhiteSpace or EditorCursor.ShowWhiteSpace => Cursors.SizeNS,
        EditorCursor.LinkReady => LinkReady.Value,
        EditorCursor.Linking => Linking.Value,
        EditorCursor.Pencil => Pencil.Value,
        EditorCursor.Eraser => Eraser.Value,
        _ => Cursors.Default,
    };

    // The selection and rotate arrows, the vertical I-beam, Create Link's pitchers and the table pen and eraser Windows has no cursor
    // for (Cursors/, written by its make-cursors.py), loaded once and shared by every control for the process's life, as the system
    // cursors are.
    private static readonly Lazy<Cursor> CellSelect = new(() => Load("cell-select.cur"));
    private static readonly Lazy<Cursor> ColumnSelect = new(() => Load("column-select.cur"));
    private static readonly Lazy<Cursor> RowSelect = new(() => Load("row-select.cur"));
    private static readonly Lazy<Cursor> Rotate = new(() => Load("rotate.cur"));
    private static readonly Lazy<Cursor> VerticalText = new(() => Load("vertical-text.cur"));
    private static readonly Lazy<Cursor> LinkReady = new(() => Load("link-ready.cur"));
    private static readonly Lazy<Cursor> Linking = new(() => Load("linking.cur"));
    private static readonly Lazy<Cursor> Pencil = new(() => Load("pencil.cur"));
    private static readonly Lazy<Cursor> Eraser = new(() => Load("eraser.cur"));

    private static Cursor Load(string name)
    {
        using var stream = typeof(WinFormsAdapter).Assembly.GetManifestResourceStream($"Twip.WinForms.Cursors.{name}");
        if (stream is null) return Cursors.Arrow;
        try { return new Cursor(stream); }
        catch (ArgumentException) { return Cursors.Arrow; }
    }

    public void CapturePointer() => control.Capture = true;

    public void ReleasePointer() => control.Capture = false;

    public void SetClipboard(ClipboardContent content) => ClipboardFormats.Set(content, control.Session);

    public ValueTask<ClipboardContent?> GetClipboardAsync() => new(ClipboardFormats.Get());

    public void SetCaret(CaretInfo caret) => control.MoveCaret(caret);

    public void ScrollTo(double x, double y) => control.ScrollTo(x, y);

    public void ShowTooltip(TooltipInfo? tooltip) => control.ShowTooltip(tooltip);

    public void UpdateAccessibility(AccessibleNode tree) => control.UpdateAccessibility(tree);

    public void Announce(string text) => control.Announce(text);
}

/// <summary>The clipboard formats the engine reads and writes: Unicode text, CF_HTML, pictures (PNG, then a DIB), and the engine's own token.</summary>
internal static class ClipboardFormats
{
    /// <summary>The engine's token for its own copy: a paste back into a session keeps everything.</summary>
    public const string Native = "Twip.Fragment";

    /// <summary>The registered PNG format browsers and Office put on the clipboard.</summary>
    private const string Png = "PNG";

    /// <summary>The most a picture read from the clipboard may be: the most a pasted picture can be.</summary>
    private const long MaxPicture = 256L << 20;

    public static void Set(ClipboardContent content, EditorSession? session = null)
    {
        var data = new DelayedHtml(content, session);
        if (!string.IsNullOrEmpty(content.Text)) data.SetText(content.Text, TextDataFormat.UnicodeText);
        if (content.Html is { Length: > 0 } html) data.SetText(ToCfHtml(html), TextDataFormat.Html);
        if (content.Native is { } token) data.SetData(Native, token);
        if (content.Image is { } image && image.Data.Length > 0)
        {
            var png = image.Data.ToArray();
            data.SetData(Png, false, new MemoryStream(png));
            try
            {
                // A DIB for the programs that read no PNG.
                using var decoded = new Bitmap(new MemoryStream(png));
                data.SetImage(new Bitmap(decoded));
            }
            catch (ArgumentException) { } // not a picture GDI+ reads: the PNG alone
        }
        try { Clipboard.SetDataObject(data, copy: true, retryTimes: 5, retryDelay: 50); }
        catch (System.Runtime.InteropServices.ExternalException) { } // another program holds the clipboard
    }

    public static ClipboardContent? Get()
    {
        try
        {
            var text = Clipboard.ContainsText(TextDataFormat.UnicodeText) ? Clipboard.GetText(TextDataFormat.UnicodeText) : null;
            var html = Clipboard.ContainsText(TextDataFormat.Html) ? FromCfHtml(Clipboard.GetText(TextDataFormat.Html)) : null;
            var image = Picture();
            if (text is null && html is null && image is null) return null;
            return new ClipboardContent(text, html, Clipboard.TryGetData<string>(Native, out var token) ? token : null, image);
        }
        catch (System.Runtime.InteropServices.ExternalException)
        {
            return null;
        }
    }

    /// <summary>The clipboard's picture: PNG, else a DIB as a BMP file, else any image saved as PNG, else the first image file of a file drop list.</summary>
    private static ClipboardImage? Picture()
    {
        if (Clipboard.TryGetData<MemoryStream>(Png, out var png) && png is { Length: <= MaxPicture }) return new ClipboardImage("image/png", png.ToArray());
        if (Clipboard.TryGetData<MemoryStream>(DataFormats.Dib, out var dib) && dib is { Length: > 40 and <= MaxPicture }) return new ClipboardImage("image/bmp", Bmp(dib.ToArray()));
        if (Clipboard.ContainsImage() && Clipboard.GetImage() is { } any)
        {
            using (any)
            using (var ms = new MemoryStream())
            {
                any.Save(ms, System.Drawing.Imaging.ImageFormat.Png);
                return new ClipboardImage("image/png", ms.ToArray());
            }
        }
        if (Clipboard.ContainsFileDropList())
        {
            foreach (var file in Clipboard.GetFileDropList())
            {
                if (file is null || !IsPictureFile(file)) continue;
                var info = new FileInfo(file);
                if (info.Exists && info.Length <= MaxPicture) return new ClipboardImage(MimeOf(file), File.ReadAllBytes(file));
            }
        }
        return null;
    }

    private static readonly string[] PictureExtensions = [".png", ".jpg", ".jpeg", ".gif", ".bmp", ".tif", ".tiff", ".svg", ".emf", ".wmf"];

    private static bool IsPictureFile(string path) => PictureExtensions.Contains(Path.GetExtension(path).ToLowerInvariant());

    private static string MimeOf(string path) => Path.GetExtension(path).ToLowerInvariant() switch
    {
        ".png" => "image/png",
        ".jpg" or ".jpeg" => "image/jpeg",
        ".gif" => "image/gif",
        ".bmp" => "image/bmp",
        ".tif" or ".tiff" => "image/tiff",
        ".svg" => "image/svg+xml",
        ".emf" => "image/emf",
        _ => "image/wmf",
    };

    /// <summary>A packed DIB with a BITMAPFILEHEADER before it: a BMP file, which the engine reads.</summary>
    internal static byte[] Bmp(byte[] dib)
    {
        var header = BitConverter.ToInt32(dib, 0);
        var bits = BitConverter.ToInt16(dib, 14);
        var compression = BitConverter.ToInt32(dib, 16);
        var used = BitConverter.ToInt32(dib, 32);
        var colours = used != 0 ? used : bits <= 8 ? 1 << bits : 0;
        // BI_BITFIELDS with a BITMAPINFOHEADER: three masks follow it.
        var masks = header == 40 && compression == 3 ? 12 : 0;
        var offset = 14 + header + masks + colours * 4;
        var bmp = new byte[14 + dib.Length];
        bmp[0] = (byte)'B';
        bmp[1] = (byte)'M';
        BitConverter.GetBytes(bmp.Length).CopyTo(bmp, 2);
        BitConverter.GetBytes(offset).CopyTo(bmp, 10);
        dib.CopyTo(bmp, 14);
        return bmp;
    }

    /// <summary>A copy whose HTML, its objects drawn as pictures, is asked of the engine when a program reads <c>HTML Format</c>: a copy stays as fast as its text.</summary>
    private sealed class DelayedHtml(ClipboardContent content, EditorSession? session) : DataObject
    {
        private string? _drawn;

        /// <summary>The drawn HTML for <paramref name="format"/> <c>HTML Format</c>; null for the other formats, or when the engine can't give it.</summary>
        private string? Drawn(string format)
        {
            if (format != DataFormats.Html || session is null || content.Native is not { } token) return null;
            if (_drawn is null)
            {
                try
                {
                    var html = session.GetClipboardHtmlAsync(token);
                    _drawn = html.Wait(TimeSpan.FromSeconds(5)) ? ToCfHtml(html.Result) : null;
                }
                catch (AggregateException) { } // the clipboard changed: the HTML set with the copy stands
            }
            return _drawn;
        }

#if NETFRAMEWORK
        public override object? GetData(string format, bool autoConvert) => Drawn(format) ?? base.GetData(format, autoConvert);
#else
        [Obsolete("the clipboard reads HTML Format through it")]
        public override object? GetData(string format, bool autoConvert) =>
#pragma warning disable WFDEV005
            Drawn(format) ?? base.GetData(format, autoConvert);
#pragma warning restore WFDEV005

        protected override bool TryGetDataCore<T>(string format, Func<System.Reflection.Metadata.TypeName, Type?>? resolver, bool autoConvert, [System.Diagnostics.CodeAnalysis.MaybeNullWhen(false), System.Diagnostics.CodeAnalysis.NotNullWhen(true)] out T data)
        {
            if (Drawn(format) is { } html && html is T value)
            {
                data = value;
                return true;
            }
            return base.TryGetDataCore(format, resolver, autoConvert, out data);
        }
#endif
    }

    private const string Header = "Version:0.9\r\nStartHTML:{0:D10}\r\nEndHTML:{1:D10}\r\nStartFragment:{2:D10}\r\nEndFragment:{3:D10}\r\n";
    private const string Before = "<html><body>\r\n<!--StartFragment-->";
    private const string After = "<!--EndFragment-->\r\n</body></html>";

    /// <summary>Wraps an HTML fragment in the CF_HTML header, whose offsets count UTF-8 bytes.</summary>
    internal static string ToCfHtml(string fragment)
    {
        var start = string.Format(System.Globalization.CultureInfo.InvariantCulture, Header, 0, 0, 0, 0).Length;
        var fragmentStart = start + Encoding.UTF8.GetByteCount(Before);
        var fragmentEnd = fragmentStart + Encoding.UTF8.GetByteCount(fragment);
        var end = fragmentEnd + Encoding.UTF8.GetByteCount(After);
        return string.Format(System.Globalization.CultureInfo.InvariantCulture, Header, start, end, fragmentStart, fragmentEnd) + Before + fragment + After;
    }

    /// <summary>The fragment of a CF_HTML string, or the whole HTML when it has no fragment markers.</summary>
    internal static string FromCfHtml(string cfHtml)
    {
        var bytes = Encoding.UTF8.GetBytes(cfHtml);
        int Offset(string name)
        {
            var at = cfHtml.IndexOf(name + ":", StringComparison.Ordinal);
            if (at < 0) return -1;
            var digits = cfHtml.AsSpan(at + name.Length + 1).TrimStart();
            var length = 0;
            while (length < digits.Length && char.IsAsciiDigit(digits[length])) length++;
            return int.TryParse(digits[..length], System.Globalization.CultureInfo.InvariantCulture, out var value) ? value : -1;
        }
        var (start, end) = (Offset("StartFragment"), Offset("EndFragment"));
        if (start < 0 || end < start || end > bytes.Length) (start, end) = (Offset("StartHTML"), Offset("EndHTML"));
        if (start < 0 || end < start || end > bytes.Length) return cfHtml;
        return Encoding.UTF8.GetString(bytes, start, end - start);
    }
}

/// <summary>WinForms key codes to the engine's physical keys.</summary>
internal static class KeyMap
{
    public static KeyModifiers Modifiers(Keys modifiers) =>
        ((modifiers & Keys.Shift) != 0 ? KeyModifiers.Shift : 0)
        | ((modifiers & Keys.Control) != 0 ? KeyModifiers.Control : 0)
        | ((modifiers & Keys.Alt) != 0 ? KeyModifiers.Alt : 0);

    public static EditorKey ToEditorKey(Keys key) => key switch
    {
        >= Keys.A and <= Keys.Z => EditorKey.KeyA + (key - Keys.A),
        >= Keys.D0 and <= Keys.D9 => EditorKey.Digit0 + (key - Keys.D0),
        >= Keys.F1 and <= Keys.F12 => EditorKey.F1 + (key - Keys.F1),
        >= Keys.NumPad0 and <= Keys.NumPad9 => EditorKey.Numpad0 + (key - Keys.NumPad0),
        Keys.Enter => EditorKey.Enter,
        Keys.Escape => EditorKey.Escape,
        Keys.Back => EditorKey.Backspace,
        Keys.Tab => EditorKey.Tab,
        Keys.Space => EditorKey.Space,
        Keys.OemMinus => EditorKey.Minus,
        Keys.Oemplus => EditorKey.Equal,
        Keys.OemOpenBrackets => EditorKey.BracketLeft,
        Keys.OemCloseBrackets => EditorKey.BracketRight,
        Keys.OemPipe => EditorKey.Backslash,
        Keys.OemSemicolon => EditorKey.Semicolon,
        Keys.OemQuotes => EditorKey.Quote,
        Keys.Oemtilde => EditorKey.Backquote,
        Keys.Oemcomma => EditorKey.Comma,
        Keys.OemPeriod => EditorKey.Period,
        Keys.OemQuestion => EditorKey.Slash,
        Keys.Insert => EditorKey.Insert,
        Keys.Delete => EditorKey.Delete,
        Keys.Home => EditorKey.Home,
        Keys.End => EditorKey.End,
        Keys.PageUp => EditorKey.PageUp,
        Keys.PageDown => EditorKey.PageDown,
        Keys.Left => EditorKey.ArrowLeft,
        Keys.Right => EditorKey.ArrowRight,
        Keys.Up => EditorKey.ArrowUp,
        Keys.Down => EditorKey.ArrowDown,
        Keys.Apps => EditorKey.ContextMenu,
        Keys.Add => EditorKey.NumpadAdd,
        Keys.Subtract => EditorKey.NumpadSubtract,
        Keys.Multiply => EditorKey.NumpadMultiply,
        Keys.Divide => EditorKey.NumpadDivide,
        Keys.Decimal => EditorKey.NumpadDecimal,
        _ => EditorKey.None,
    };
}
