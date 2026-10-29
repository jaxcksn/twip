using System.Buffers;
using System.Drawing;
using System.Text;
using System.Text.Json;
using Twip.Native;
using Twip.Wire;

namespace Twip.Hosting;

/// <summary>
/// A session's presentation, from <see cref="EditorSession.Attach"/>. The adapter forwards raw input and size changes,
/// and calls <see cref="Render"/> when it paints; this class owns invalidation, frame scheduling and engine
/// coordination. Disposing it detaches: the session keeps its document, selection and history.
/// </summary>
public sealed class EditorHost : IDisposable
{
    private readonly EditorSession _session;
    private readonly object _frameLock = new();
    private byte[] _front = [];
    private int _frameWidth, _frameHeight;
    private byte[] _back = []; // engine thread only
    private volatile bool _disposed;
    private double _width, _height, _dpi = 96;
    private double _scrollX, _scrollY;
    private double _contentWidth, _contentHeight;
    private bool _renderQueued; // engine thread only
    private bool _hidden, _missedFrame; // engine thread only
    private bool _window; // engine thread only: frames go to the attached window, with the GPU
    private bool _horizontalRuler, _verticalRuler;
    private RulerUnit _rulerUnit;
    private TabSelector _tabSelector;
    private double _markupPaneWidth;
    private Color _background = Color.FromArgb(0xE6, 0xE6, 0xE6);

    internal EditorHost(EditorSession session, IEditorHostAdapter adapter)
    {
        _session = session;
        Adapter = adapter;
    }

    internal IEditorHostAdapter Adapter { get; }

    /// <summary>Device pixels per CSS pixel: the engine works in CSS px, adapters in client pixels.</summary>
    internal float Scale => (float)(_dpi / 96);

    /// <summary>The document's size after the last frame, in client pixels: for scrollbars.</summary>
    internal SizeF ContentSize => new((float)(_contentWidth * Scale), (float)(_contentHeight * Scale));

    /// <summary>The GPU backend frames are drawn with ("D3D12", "D3D11", "Vulkan"), or null when they're CPU pixels.</summary>
    internal string? GpuBackend { get; private set; }

    /// <summary>Raised on the engine thread when the GPU stops drawing (the device was lost): frames are CPU pixels again.</summary>
    internal event Action? GpuLost;

    /// <summary>
    /// Draws frames into <paramref name="hwnd"/> with the GPU from now on, when there is one: the backend, or null (frames
    /// stay CPU pixels for <see cref="IEditorHostAdapter.Present"/>). Blocks until the engine has it.
    /// </summary>
    internal string? AttachWindow(nint hwnd)
    {
        if (_disposed) return null;
        var attached = _session.Enqueue(id =>
        {
            string? backend;
            try { backend = TwipNative.AttachWindow(_session.Engine, id, hwnd) switch { 1 => "D3D12", 2 => "D3D11", 3 => "Vulkan", _ => null }; }
            catch (EntryPointNotFoundException) { backend = null; } // a native library from before the GPU path
            _window = backend is not null;
            GpuBackend = backend;
            ScheduleRender();
            return backend;
        }, default);
        return attached.Wait(TimeSpan.FromSeconds(5)) ? attached.Result : null;
    }

    /// <summary>Asks for a new frame, as a change does.</summary>
    internal void RequestFrame() => Fire(_ => ScheduleRender());

    /// <summary>Frames are CPU pixels again; blocks until the engine no longer draws into the window, which may then go.</summary>
    internal void DetachWindow()
    {
        if (_disposed) return;
        _session.Enqueue(id =>
        {
            if (_window) TwipNative.DetachWindow(_session.Engine, id);
            (_window, GpuBackend) = (false, null);
            ScheduleRender();
            return true;
        }, default).Wait(TimeSpan.FromSeconds(5));
    }

    // Input

    /// <summary>Forwards a key press.</summary>
    /// <param name="key">The physical key.</param>
    /// <param name="modifiers">The modifiers held.</param>
    /// <param name="handled">Whether the engine used the key; if not, let the platform handle it.</param>
    public void OnKey(EditorKey key, KeyModifiers modifiers, out bool handled)
    {
        handled = false;
        if (_disposed) return;
        if (PasteChord(key, modifiers) is { } asText)
        {
            // The system clipboard is the host's: read it here, and let the engine paste what it holds.
            handled = true;
            Paste(asText);
            return;
        }
        try
        {
            // shortcut: the UI thread waits for the engine's answer; a managed keymap built from the catalog could
            // answer at once and forward the key without waiting.
            handled = _session.Enqueue(id => TwipNative.Key(_session.Engine, id, (uint)key, (uint)modifiers), default)
                .GetAwaiter().GetResult() != 0;
        }
        catch (EditorException) { }
    }

    /// <summary>Ctrl+V and Shift+Insert paste formatted (false), Ctrl+Shift+V as text (true); null for other keys.</summary>
    private static bool? PasteChord(EditorKey key, KeyModifiers modifiers) => (key, modifiers) switch
    {
        (EditorKey.KeyV, KeyModifiers.Control) or (EditorKey.Insert, KeyModifiers.Shift) => false,
        (EditorKey.KeyV, KeyModifiers.Control | KeyModifiers.Shift) => true,
        _ => null,
    };

    private void Paste(bool asText)
    {
        var read = Adapter.GetClipboardAsync();
        if (read.IsCompletedSuccessfully) Paste(read.Result, asText);
        else Observe(read.AsTask().ContinueWith(t => Paste(t.Result, asText), CancellationToken.None, TaskContinuationOptions.OnlyOnRanToCompletion, TaskScheduler.Default));
    }

    private void Paste(ClipboardContent? content, bool asText)
    {
        if (content is null || _disposed) return;
        Send(Method.Paste, PasteRequest(content, asText));
    }

    /// <summary>The context menu's Paste: reads the system clipboard, as Ctrl+V does, then pastes.</summary>
    internal async Task PasteAsync(bool asText, CancellationToken ct)
    {
        var content = await Adapter.GetClipboardAsync().ConfigureAwait(false);
        if (content is null || _disposed) return;
        await _session.CallAsync(Method.Paste, PasteRequest(content, asText), ct).ConfigureAwait(false);
    }

    private static byte[] PasteRequest(ClipboardContent content, bool asText)
    {
        // A copy from any editor of the process carries the engine's token: it pastes with everything kept.
        var data = content with { Text = content.Text ?? "" };
        return Json(w =>
        {
            w.WriteStartObject();
            w.WritePropertyName("data");
            JsonSerializer.Serialize(w, data, TwipJsonContext.Default.ClipboardContent);
            w.WriteBoolean("asText", asText);
            w.WriteEndObject();
        });
    }

    /// <summary>Forwards typed text (after the platform's key translation).</summary>
    /// <param name="text">The text.</param>
    public void OnText(string text)
    {
        ArgumentNullException.ThrowIfNull(text);
        var utf8 = Encoding.UTF8.GetBytes(text);
        Fire(id => { unsafe { fixed (byte* p = utf8) TwipNative.Text(_session.Engine, id, p, (nuint)utf8.Length); } });
    }

    /// <summary>Forwards IME composition.</summary>
    /// <param name="phase">The phase.</param>
    /// <param name="text">The composition text.</param>
    /// <param name="cursor">The cursor offset in the composition text.</param>
    public void OnComposition(CompositionPhase phase, string text, int cursor)
    {
        ArgumentNullException.ThrowIfNull(text);
        var utf8 = Encoding.UTF8.GetBytes(text);
        // shortcut: the C call carries no cursor yet; the engine puts the caret at the composition's end. A cancel
        // ends the composition with no text, until the C ABI has a cancel phase.
        var (native, bytes) = phase switch
        {
            CompositionPhase.Start => (0u, utf8),
            CompositionPhase.Update => (1u, utf8),
            CompositionPhase.Commit => (2u, utf8),
            _ => (2u, Array.Empty<byte>()),
        };
        Fire(id => { unsafe { fixed (byte* p = bytes) TwipNative.Composition(_session.Engine, id, native, p, (nuint)bytes.Length); } });
    }

    /// <summary>Forwards raw pointer input, in control client pixels. The engine recognises the gestures.</summary>
    /// <param name="kind">What happened.</param>
    /// <param name="x">The x.</param>
    /// <param name="y">The y.</param>
    /// <param name="button">The button pressed or released (any for a move).</param>
    /// <param name="clicks">The click count (2 for a double-click).</param>
    /// <param name="modifiers">The modifiers held.</param>
    public void OnPointer(PointerKind kind, double x, double y, PointerButton button, int clicks, KeyModifiers modifiers)
    {
        // The C ABI numbers kinds and buttons as these enums do, in CSS px.
        var scale = _dpi / 96;
        Fire(id => TwipNative.Pointer(_session.Engine, id, (uint)kind, x / scale, y / scale, (uint)button, (uint)Math.Max(clicks, 0), (uint)modifiers));
    }

    /// <summary>Forwards a mouse wheel or touchpad scroll, in pixels: it scrolls the view, and with Ctrl it zooms.</summary>
    /// <param name="dx">The horizontal delta.</param>
    /// <param name="dy">The vertical delta.</param>
    /// <param name="modifiers">The modifiers held (Ctrl zooms).</param>
    public void OnWheel(double dx, double dy, KeyModifiers modifiers)
    {
        if (_disposed) return;
        if ((modifiers & KeyModifiers.Control) != 0)
        {
            Fire(id => TwipNative.Wheel(_session.Engine, id, dx * 96 / _dpi, dy * 96 / _dpi, (uint)modifiers));
            return;
        }
        // The scroll position is the host's, as on the web: the adapter scrolls and reports it back through OnScroll.
        var s = Scale;
        Adapter.ScrollTo(_scrollX * s + dx, _scrollY * s + dy);
    }

    /// <summary>Reports the client size and DPI.</summary>
    /// <param name="width">The width, in device pixels.</param>
    /// <param name="height">The height, in device pixels.</param>
    /// <param name="dpi">The DPI (96 is 100%).</param>
    public void OnResize(double width, double height, double dpi)
    {
        var dpiChanged = dpi > 0 && dpi != _dpi;
        (_width, _height, _dpi) = (Math.Max(width, 0), Math.Max(height, 0), dpi > 0 ? dpi : 96);
        var (w, h, scale) = (_width, _height, _dpi / 96);
        // The markup pane is set in client pixels, which the engine takes in CSS px.
        if (dpiChanged) MarkupPaneWidth = _markupPaneWidth;
        Fire(id =>
        {
            // The engine takes the viewport in CSS px; frames are drawn in device pixels.
            TwipNative.Surface(_session.Engine, id, w / scale, h / scale, scale);
            ScheduleRender();
        });
    }

    /// <summary>Reports the scroll position, in pixels.</summary>
    /// <param name="x">The x.</param>
    /// <param name="y">The y.</param>
    public void OnScroll(double x, double y)
    {
        (_scrollX, _scrollY) = (x * 96 / _dpi, y * 96 / _dpi);
        var (sx, sy) = (_scrollX, _scrollY);
        Fire(id => TwipNative.Scroll(_session.Engine, id, sx, sy));
    }

    /// <summary>Reports focus. The selection is kept while unfocused.</summary>
    /// <param name="focused">Whether the control has focus.</param>
    public void OnFocus(bool focused) => Fire(id => TwipNative.Focus(_session.Engine, id, focused ? 1u : 0u));

    /// <summary>Reports visibility; hidden controls aren't drawn.</summary>
    /// <param name="visible">Whether the control is visible.</param>
    public void OnVisible(bool visible) => Fire(_ =>
    {
        _hidden = !visible;
        if (visible && _missedFrame)
        {
            _missedFrame = false;
            ScheduleRender();
        }
    });

    /// <summary>An external drag is over the point: shows the drop caret.</summary>
    /// <param name="x">The x, in client pixels.</param>
    /// <param name="y">The y, in client pixels.</param>
    public void OnDragOver(double x, double y) => SendAndDraw(Method.HostDragOver, Point(x, y));

    /// <summary>The external drag left.</summary>
    public void OnDragLeave() => SendAndDraw(Method.HostDragLeave, []);

    /// <summary>Drops at the point: runs the operation there as one transaction.</summary>
    /// <param name="x">The x, in client pixels.</param>
    /// <param name="y">The y, in client pixels.</param>
    /// <param name="operation">The operation.</param>
    /// <returns>Completes when it has committed.</returns>
    public Task OnDropAsync(double x, double y, EditorOperation operation)
    {
        ArgumentNullException.ThrowIfNull(operation);
        var s = Scale;
        return _session.CallAsync(Method.HostDrop, Json(w =>
        {
            w.WriteStartObject();
            w.WritePropertyName("at");
            JsonSerializer.Serialize(w, new PointF((float)(x / s), (float)(y / s)), TwipJsonContext.Default.PointF);
            w.WritePropertyName("command");
            JsonSerializer.Serialize(w, operation, TwipJsonContext.Default.EditorOperation);
            w.WriteEndObject();
        }), default);
    }

    /// <summary>What is under a point, without moving the selection.</summary>
    /// <param name="x">The x, in client pixels.</param>
    /// <param name="y">The y, in client pixels.</param>
    /// <returns>The hit.</returns>
    public Task<HitResult> HitTestAsync(double x, double y) => _session.CallAsync<HitResult>(Method.HostHitTest, Point(x, y), default);

    // View settings that belong to the presentation

    /// <summary>Whether the engine draws the horizontal ruler.</summary>
    public bool HorizontalRuler
    {
        get => _horizontalRuler;
        set { _horizontalRuler = value; SendRulers(); }
    }

    /// <summary>Whether the engine draws the vertical ruler.</summary>
    public bool VerticalRuler
    {
        get => _verticalRuler;
        set { _verticalRuler = value; SendRulers(); }
    }

    /// <summary>The rulers' unit.</summary>
    public RulerUnit RulerUnit
    {
        get => _rulerUnit;
        set { _rulerUnit = value; SendAndDraw(Method.HostSetRulerUnit, WireJson.ToUtf8(value)); }
    }

    /// <summary>What a click on the ruler adds.</summary>
    public TabSelector TabSelector
    {
        get => _tabSelector;
        set { _tabSelector = value; SendAndDraw(Method.HostSetTabSelector, WireJson.ToUtf8(value)); }
    }

    /// <summary>The room beside the pages for comment balloons, in client pixels.</summary>
    public double MarkupPaneWidth
    {
        get => _markupPaneWidth;
        set
        {
            _markupPaneWidth = double.IsFinite(value) ? Math.Max(value, 0) : 0;
            var css = _markupPaneWidth * 96 / _dpi;
            SendAndDraw(Method.HostSetMarkupPaneWidth, Json(w => WireJson.WriteNumber(w, css)));
        }
    }

    /// <summary>The background around the pages; the paper stays white.</summary>
    public Color Background
    {
        get => _background;
        set
        {
            _background = value;
            SendAndDraw(Method.HostSetBackground, Json(w => w.WriteStringValue($"#{value.R:X2}{value.G:X2}{value.B:X2}")));
        }
    }

    /// <summary>The ruler model with its viewport mapping.</summary>
    public RulerView RulerView => ToClient(CallNow(Method.HostRulerView)?.Deserialize(TwipJsonContext.Default.RulerView)
        ?? throw new EditorException(EditorErrorCode.Internal, "the engine gave no ruler view"));

    /// <summary>The visible pages and their client rectangles.</summary>
    public IReadOnlyList<PageRect> VisiblePages
    {
        get
        {
            if (CallNow(Method.HostVisiblePages) is not { ValueKind: JsonValueKind.Array } pages) return [];
            var s = Scale;
            return [.. pages.EnumerateArray().Select(p =>
            {
                var page = p.Deserialize(TwipJsonContext.Default.PageRect)!;
                var r = page.Bounds;
                return page with { Bounds = new RectangleF(r.X * s, r.Y * s, r.Width * s, r.Height * s) };
            })];
        }
    }

    /// <summary>Scrolls the selection into view.</summary>
    /// <returns>Completes when it is in view.</returns>
    public async Task RevealSelectionAsync()
    {
        await _session.CallAsync(Method.HostRevealSelection, [], default).ConfigureAwait(false);
        // The engine's scroll request was posted to the UI thread during the call: complete once it has run.
        var done = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        try { Adapter.Dispatcher.Post(done.SetResult); }
        catch (Exception e) when (e is InvalidOperationException or ObjectDisposedException) { done.SetResult(); }
        await done.Task.ConfigureAwait(false);
    }

    // Painting

    /// <summary>Presents the latest frame through <see cref="IEditorHostAdapter.Present"/>. Call it inside the adapter's paint.</summary>
    public void Render()
    {
        if (_disposed) return;
        lock (_frameLock)
        {
            if (_frameWidth == 0 || _frameHeight == 0) return;
            var frame = new FrameView(_front, _frameWidth, _frameHeight, _frameWidth * 4);
            Adapter.Present(in frame);
        }
    }

    /// <summary>Detaches. The session keeps its document, selection and history; a new attach rebuilds the presentation.</summary>
    public void Dispose()
    {
        if (_disposed) return;
        _disposed = true;
        _session.Detached(this);
    }

    // Client coordinates: the engine's events are in viewport CSS px.

    internal PointF ToClient(PointF p) => new(p.X * Scale, p.Y * Scale);

    internal TooltipInfo ToClient(TooltipInfo t)
    {
        var s = Scale;
        var b = t.Bounds;
        return t with { Location = ToClient(t.Location), Bounds = new RectangleF(b.X * s, b.Y * s, b.Width * s, b.Height * s) };
    }

    internal CommentAnchor ToClient(CommentAnchor a)
    {
        var s = Scale;
        return a with { Start = ToClient(a.Start), End = ToClient(a.End), LineTop = a.LineTop * s, LineBottom = a.LineBottom * s, PageEdge = a.PageEdge * s };
    }

    internal RulerView ToClient(RulerView v)
    {
        var s = Scale;
        return v with
        {
            Horizontal = new RulerAxis(v.Horizontal.Origin * s, v.Horizontal.Scale * s),
            Vertical = new RulerAxis(v.Vertical.Origin * s, v.Vertical.Scale * s),
        };
    }

    // Internals, called on the engine thread

    internal void OnFrameRequested() => ScheduleRender();

    internal void OnHostRequest(byte[] payload)
    {
        var request = JsonSerializer.Deserialize(payload, TwipJsonContext.Default.HostRequest);
        if (request is null || _disposed) return;
        // Positions come in content CSS px: the adapter takes client pixels, and the host keeps the scroll.
        var s = Scale;
        switch (request)
        {
            case CursorRequest cursor:
                Post(() => Adapter.SetCursor(cursor.Cursor));
                break;
            case CaptureRequest:
                Post(Adapter.CapturePointer);
                break;
            case ReleaseRequest:
                Post(Adapter.ReleasePointer);
                break;
            case ClipboardRequest clipboard:
                Post(() => Adapter.SetClipboard(clipboard.Data));
                break;
            case CaretRequest caret:
                var bounds = new RectangleF((float)(caret.X - _scrollX) * s, (float)(caret.Y - _scrollY) * s, s, (float)caret.Height * s);
                Post(() => Adapter.SetCaret(new CaretInfo(bounds, caret.Visible)));
                break;
            case ScrollToRequest scroll:
                var (vw, vh) = (_width / s, _height / s);
                var (sx, sy) = (Reveal(scroll.X, scroll.Width, _scrollX, vw), Reveal(scroll.Y, scroll.Height, _scrollY, vh));
                if (sx != _scrollX || sy != _scrollY) Post(() => Adapter.ScrollTo(sx * s, sy * s));
                break;
            case A11yRequest a11y:
                Post(() => Adapter.UpdateAccessibility(a11y.Tree));
                break;
        }
    }

    /// <summary>The scroll that brings <c>[at, at + size]</c> into a view of <paramref name="view"/>, moving it least.</summary>
    private static double Reveal(double at, double size, double scroll, double view) =>
        at < scroll ? at : at + size > scroll + view ? Math.Min(at, at + size - view) : scroll;

    internal void OnTooltip(TooltipInfo? tooltip) => Post(() => Adapter.ShowTooltip(tooltip));

    internal void OnAnnouncement(string text) => Post(() => Adapter.Announce(text));

    private void Post(Action action) => Adapter.Dispatcher.Post(() =>
    {
        if (!_disposed) action();
    });

    private void ScheduleRender()
    {
        if (_renderQueued || _disposed) return;
        if (_hidden)
        {
            _missedFrame = true;
            return;
        }
        _renderQueued = true;
        Fire(RenderFrame);
    }

    /// <summary>
    /// Draws a frame as CPU pixels now, for <see cref="Render"/> to present, while frames otherwise go to the GPU window:
    /// for a copy of the control's content (WinForms' DrawToBitmap), which can't read the swap chain.
    /// </summary>
    internal void RenderPixels()
    {
        if (_disposed) return;
        _session.Enqueue(id => { RenderFrame(id, pixels: true); return true; }, default).Wait(TimeSpan.FromSeconds(5));
    }

    /// <summary>Draws a frame into the back buffer, swaps it to the front, and asks the adapter to paint; or draws it
    /// into the GPU window.</summary>
    private unsafe void RenderFrame(uint id) => RenderFrame(id, pixels: false);

    private unsafe void RenderFrame(uint id, bool pixels)
    {
        _renderQueued = false;
        var (w, h) = ((int)Math.Ceiling(_width), (int)Math.Ceiling(_height));
        if (_disposed || w == 0 || h == 0) return;
        TwipFrameInfo info = default;
        int status;
        if (_window && !pixels)
        {
            status = TwipNative.FrameWindow(_session.Engine, id, (uint)w, (uint)h, &info);
            if (status == Status.Ok)
            {
                (_contentWidth, _contentHeight) = (info.ContentWidth, info.ContentHeight);
                Post(Adapter.RequestRender);
                return;
            }
            // The GPU can't draw now (the device was lost): CPU pixels from here on.
            TwipNative.DetachWindow(_session.Engine, id);
            (_window, GpuBackend) = (false, null);
            GpuLost?.Invoke();
        }
        if (_back.Length != w * h * 4) _back = new byte[w * h * 4];
        fixed (byte* p = _back) status = TwipNative.FrameRgba(_session.Engine, id, p, (uint)w, (uint)h, &info);
        if (status != Status.Ok) return;
        (_contentWidth, _contentHeight) = (info.ContentWidth, info.ContentHeight);
        lock (_frameLock)
        {
            (_front, _back) = (_back, _front);
            (_frameWidth, _frameHeight) = (w, h);
        }
        Post(Adapter.RequestRender);
    }

    private void SendRulers()
    {
        var (h, v) = (_horizontalRuler, _verticalRuler);
        SendAndDraw(Method.HostSetRulers, Json(w =>
        {
            w.WriteStartObject();
            w.WriteBoolean("horizontal", h);
            w.WriteBoolean("vertical", v);
            w.WriteEndObject();
        }));
    }

    /// <summary>A client point as the engine's viewport point, CSS px.</summary>
    private byte[] Point(double x, double y)
    {
        var s = Scale;
        return WireJson.ToUtf8(new PointF((float)(x / s), (float)(y / s)));
    }

    private static byte[] Json(Action<Utf8JsonWriter> write)
    {
        using var buffer = new MemoryStream();
        using (var writer = new Utf8JsonWriter(buffer)) write(writer);
        return buffer.ToArray();
    }

    /// <summary>Calls a host method in queue order, then draws a frame that shows its effect.</summary>
    private void SendAndDraw(Method method, byte[] request) => Fire(id =>
    {
        _session.Fonts.Native.Call(id, method, request);
        ScheduleRender();
    });

    /// <summary>Calls a host method in queue order, without waiting.</summary>
    private void Send(Method method, byte[] request)
    {
        if (!_disposed) Observe(_session.CallAsync(method, request, default));
    }

    /// <summary>Calls a host method and waits for it: for the members that answer at once.</summary>
    private JsonElement? CallNow(Method method) => _session.CallAsync(method, [], default).GetAwaiter().GetResult();

    /// <summary>Queues hot input on the engine thread, in order with every other operation, without waiting.</summary>
    private void Fire(Action<uint> work)
    {
        if (_disposed) return;
        Observe(_session.Enqueue(id => { work(id); return true; }, default));
    }

    private static void Observe(Task task) => task.ContinueWith(
        static t => _ = t.Exception, CancellationToken.None, TaskContinuationOptions.OnlyOnFaulted, TaskScheduler.Default);
}
