using System.Drawing;
using Twip;
using Twip.Hosting;

namespace Examples;

#region adapter
// What a UI framework's control gives the session. The session calls these on the dispatcher's thread.
sealed class CanvasAdapter(IEditorDispatcher dispatcher, ICanvas canvas) : IEditorHostAdapter
{
    public IEditorDispatcher Dispatcher => dispatcher;
    public void RequestRender() => canvas.Invalidate(); // the control's paint then calls EditorHost.Render
    public void Present(in FrameView frame) => canvas.Blit(frame.Pixels, frame.Width, frame.Height, frame.Stride);
    public void SetCursor(EditorCursor cursor) => canvas.Cursor = cursor;
    public void CapturePointer() => canvas.Capture(true);
    public void ReleasePointer() => canvas.Capture(false);
    public void SetClipboard(ClipboardContent content) => canvas.Clipboard = content;
    public ValueTask<ClipboardContent?> GetClipboardAsync() => ValueTask.FromResult(canvas.Clipboard);
    public void SetCaret(CaretInfo caret) => canvas.ImeWindow = caret.Bounds; // where the IME shows its candidates
    public void ScrollTo(double x, double y) => canvas.ScrollTo(x, y);
    public void ShowTooltip(TooltipInfo? tooltip) => canvas.Tooltip = tooltip;
    public void UpdateAccessibility(AccessibleNode tree) => canvas.Accessibility = tree;
    public void Announce(string text) => canvas.Announce(text);
}
#endregion

// A stand-in for a UI framework's drawing surface.
interface ICanvas
{
    void Invalidate();
    void Blit(ReadOnlySpan<byte> pixels, int width, int height, int stride);
    EditorCursor Cursor { set; }
    void Capture(bool on);
    ClipboardContent? Clipboard { get; set; }
    RectangleF ImeWindow { set; }
    void ScrollTo(double x, double y);
    TooltipInfo? Tooltip { set; }
    AccessibleNode Accessibility { set; }
    void Announce(string text);
}

static class HostingExample
{
    static void Attach(EditorSession session, IEditorDispatcher dispatcher, ICanvas canvas)
    {
        #region attach
        using EditorHost host = session.Attach(new CanvasAdapter(dispatcher, canvas));
        host.OnResize(800, 600, 96);
        host.OnFocus(true);

        // Forward raw input. The engine does the rest: keys, gestures, rulers, IME.
        host.OnKey(EditorKey.KeyB, KeyModifiers.Control, out bool handled); // Ctrl+B
        host.OnText("Hello");
        host.OnPointer(PointerKind.Down, 120, 80, PointerButton.Primary, clicks: 2, KeyModifiers.None);
        host.OnWheel(0, -120, KeyModifiers.None);

        host.Render(); // inside the control's paint: the session calls Present with the frame
        #endregion
    }
}
