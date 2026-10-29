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
        _ => Cursors.Default,
    };

    public void CapturePointer() => control.Capture = true;

    public void ReleasePointer() => control.Capture = false;

    public void SetClipboard(ClipboardContent content) => ClipboardFormats.Set(content);

    public ValueTask<ClipboardContent?> GetClipboardAsync() => new(ClipboardFormats.Get());

    public void SetCaret(CaretInfo caret) => control.MoveCaret(caret);

    public void ScrollTo(double x, double y) => control.ScrollTo(x, y);

    public void ShowTooltip(TooltipInfo? tooltip) => control.ShowTooltip(tooltip);

    public void UpdateAccessibility(AccessibleNode tree) => control.UpdateAccessibility(tree);

    public void Announce(string text) => control.Announce(text);
}

/// <summary>The clipboard formats the engine reads and writes: Unicode text, CF_HTML, and the engine's own token.</summary>
internal static class ClipboardFormats
{
    /// <summary>The engine's token for its own copy: a paste back into a session keeps everything.</summary>
    public const string Native = "Twip.Fragment";

    public static void Set(ClipboardContent content)
    {
        var data = new DataObject();
        if (!string.IsNullOrEmpty(content.Text)) data.SetText(content.Text, TextDataFormat.UnicodeText);
        if (!string.IsNullOrEmpty(content.Html)) data.SetText(ToCfHtml(content.Html), TextDataFormat.Html);
        if (content.Native is { } token) data.SetData(Native, token);
        try { Clipboard.SetDataObject(data, copy: true, retryTimes: 5, retryDelay: 50); }
        catch (System.Runtime.InteropServices.ExternalException) { } // another program holds the clipboard
    }

    public static ClipboardContent? Get()
    {
        try
        {
            var text = Clipboard.ContainsText(TextDataFormat.UnicodeText) ? Clipboard.GetText(TextDataFormat.UnicodeText) : null;
            var html = Clipboard.ContainsText(TextDataFormat.Html) ? FromCfHtml(Clipboard.GetText(TextDataFormat.Html)) : null;
            if (text is null && html is null) return null;
            return new ClipboardContent(text, html, Clipboard.TryGetData<string>(Native, out var token) ? token : null);
        }
        catch (System.Runtime.InteropServices.ExternalException)
        {
            return null;
        }
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
