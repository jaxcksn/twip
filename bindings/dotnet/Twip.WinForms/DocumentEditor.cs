using System.ComponentModel;
using System.Drawing.Imaging;
using Twip.Hosting;

namespace Twip.WinForms;

/// <summary>
/// A document editor control. It owns its session by default (or hosts one you pass in), and handles input,
/// painting, sizing and DPI itself: an application opens, edits through <see cref="Commands"/>, reads
/// <see cref="State"/> and saves. In the designer it loads no native code and paints a placeholder.
/// </summary>
[DesignerCategory("Code")]
public class DocumentEditor : Control
{
    private const int WmKeyDown = 0x0100;
    private const int WmSysKeyDown = 0x0104;
    private const int WmChar = 0x0102;

    private readonly bool _ownsSession;
    private readonly bool _designTime;
    private readonly VScrollBar _vertical = new() { Dock = DockStyle.Right, TabStop = false };
    private readonly HScrollBar _horizontal = new() { Dock = DockStyle.Bottom, TabStop = false, Visible = false };
    private readonly Ime _ime;
    private readonly SystemCaret _caret;
    private readonly ToolTip _tooltip = new();
    private readonly Dictionary<CommentId, Control> _balloons = [];
    private IReadOnlyList<CommentAnchor> _anchors = [];
    private EditorHost? _host;
    private Bitmap? _frame;
    private GpuCanvas? _canvas; // while the engine draws with the GPU
    private bool _gpu = true;
    private int _gpuLosses; // counts device losses: a retry belongs to the latest
    private PaintEventHandler? _paintMarkupPane;
    private bool _leadersSent; // the engine draws leader lines now
    private bool _printing; // a WM_PRINTCLIENT paint (DrawToBitmap), which can't see the GPU's swap chain
    private TimeSpan? _caretBlink;
    private bool _caretBlinkSet; // the application set CaretBlink: Windows' blink time no longer applies
    private OverlayColors? _overlayColors; // the application's; until set, the default or high contrast's
    private char _highSurrogate;
    private Keys _routed; // the key ProcessCmdKey already gave the engine
    private bool _readOnly;
    private bool _horizontalRuler, _verticalRuler;
    private RulerUnit _rulerUnit;
    private TabSelector _tabSelector;
    private int _markupPaneWidth;
    private (double X, double Y)? _scrolled; // what the engine was last told

    /// <summary>Creates an editor with its own session over a blank document. Designer-safe.</summary>
    public DocumentEditor() : this(new EditorSession(), ownsSession: true) { }

    /// <summary>Creates an editor over an external session. Disposing the control only detaches it.</summary>
    /// <param name="session">The session; it must not be attached elsewhere.</param>
    public DocumentEditor(EditorSession session) : this(session, ownsSession: false) { }

    private DocumentEditor(EditorSession session, bool ownsSession)
    {
        ArgumentNullException.ThrowIfNull(session);
        _designTime = LicenseManager.UsageMode == LicenseUsageMode.Designtime;
        _ownsSession = ownsSession;
        _ime = new Ime(this);
        _caret = new SystemCaret(this);
        Session = session;
        State = new DocumentEditorState(session.State);
        _readOnly = session.State.ReadOnly;
        SetStyle(ControlStyles.AllPaintingInWmPaint | ControlStyles.UserPaint | ControlStyles.Selectable | ControlStyles.ResizeRedraw, true);
        BackColor = Color.FromArgb(0xE6, 0xE6, 0xE6);
        _vertical.Visible = !_designTime;
        _vertical.ValueChanged += OnScrollbar;
        _horizontal.ValueChanged += OnScrollbar;
        Controls.Add(_horizontal);
        Controls.Add(_vertical);
        session.StateChanged += OnStateChanged;
        session.CommentAnchorsChanged += OnCommentAnchorsChanged;
        if (!_designTime) Microsoft.Win32.SystemEvents.UserPreferenceChanged += OnUserPreferenceChanged;
    }

    /// <summary>The session this control presents.</summary>
    [Browsable(false), DesignerSerializationVisibility(DesignerSerializationVisibility.Hidden)]
    public EditorSession Session { get; }

    /// <summary>The session's commands.</summary>
    [Browsable(false), DesignerSerializationVisibility(DesignerSerializationVisibility.Hidden)]
    public EditorCommands Commands => Session.Commands;

    /// <summary>A bindable state projection, updated on the UI thread.</summary>
    [Browsable(false), DesignerSerializationVisibility(DesignerSerializationVisibility.Hidden)]
    public DocumentEditorState State { get; }

    /// <summary>Whether the document is read only.</summary>
    [DefaultValue(false)]
    public bool ReadOnly
    {
        get => _readOnly;
        set
        {
            if (_readOnly == value) return;
            _readOnly = value;
            if (!_designTime) Observe(Session.SetReadOnlyAsync(value));
        }
    }

    /// <summary>Opens a DOCX file.</summary>
    /// <param name="path">The file.</param>
    /// <param name="ct">Cancels the open; the current document is kept.</param>
    /// <returns>The import warnings.</returns>
    public Task<ImportResult> OpenAsync(string path, CancellationToken ct = default) => Session.OpenAsync(path, ct);

    /// <summary>Opens a DOCX stream. The stream stays open.</summary>
    /// <param name="stream">The stream.</param>
    /// <param name="ct">Cancels the open; the current document is kept.</param>
    /// <returns>The import warnings.</returns>
    public Task<ImportResult> OpenAsync(Stream stream, CancellationToken ct = default) => Session.OpenAsync(stream, ct);

    /// <summary>Saves DOCX to a file, replaced safely.</summary>
    /// <param name="path">The file.</param>
    /// <param name="ct">Cancels the save.</param>
    /// <returns>The revision saved, and the export warnings.</returns>
    public Task<SaveResult> SaveAsync(string path, CancellationToken ct = default) => Session.SaveAsync(path, ct);

    /// <summary>Saves DOCX to a stream. The stream stays open.</summary>
    /// <param name="stream">The stream.</param>
    /// <param name="ct">Cancels the save.</param>
    /// <returns>The revision saved, and the export warnings.</returns>
    public Task<SaveResult> SaveAsync(Stream stream, CancellationToken ct = default) => Session.SaveAsync(stream, ct);

    /// <summary>Whether the engine draws the horizontal ruler.</summary>
    [DefaultValue(false)]
    public bool ShowHorizontalRuler
    {
        get => _horizontalRuler;
        set { _horizontalRuler = value; if (_host is { } h) h.HorizontalRuler = value; }
    }

    /// <summary>Whether the engine draws the vertical ruler.</summary>
    [DefaultValue(false)]
    public bool ShowVerticalRuler
    {
        get => _verticalRuler;
        set { _verticalRuler = value; if (_host is { } h) h.VerticalRuler = value; }
    }

    /// <summary>The rulers' unit.</summary>
    [DefaultValue(RulerUnit.Inches)]
    public RulerUnit RulerUnit
    {
        get => _rulerUnit;
        set { _rulerUnit = value; if (_host is { } h) h.RulerUnit = value; }
    }

    /// <summary>What a click on the ruler adds.</summary>
    [DefaultValue(TabSelector.Left)]
    public TabSelector TabSelector
    {
        get => _tabSelector;
        set { _tabSelector = value; if (_host is { } h) h.TabSelector = value; }
    }

    /// <summary>The room beside the pages for comment balloons, in pixels.</summary>
    [DefaultValue(0)]
    public int MarkupPaneWidth
    {
        get => _markupPaneWidth;
        set { _markupPaneWidth = Math.Max(value, 0); if (_host is { } h) h.MarkupPaneWidth = _markupPaneWidth; }
    }

    /// <summary>Places a card control beside its comment thread, stacked with the others, through scroll and zoom.</summary>
    /// <param name="thread">The thread.</param>
    /// <param name="card">The card; the application draws it.</param>
    /// <returns>Removes the card from placement when disposed.</returns>
    public IDisposable AttachBalloon(CommentId thread, Control card)
    {
        ArgumentNullException.ThrowIfNull(card);
        if (_balloons.Remove(thread, out var old) && old != card) Controls.Remove(old);
        _balloons[thread] = card;
        card.Visible = false;
        Controls.Add(card);
        card.BringToFront();
        PlaceBalloons();
        return new Balloon(() =>
        {
            if (!_balloons.TryGetValue(thread, out var current) || current != card) return;
            _balloons.Remove(thread);
            Controls.Remove(card);
            PlaceBalloons();
        });
    }

    /// <summary>The attached balloons were placed: their positions and leader lines.</summary>
    public event EventHandler<BalloonsLaidOutEventArgs>? BalloonsLaidOut;

    /// <summary>Paint the markup pane's background and leader lines. The editor draws with the CPU while it has a handler.</summary>
    public event PaintEventHandler? PaintMarkupPane
    {
        add
        {
            _paintMarkupPane += value;
            DetachGpu(); // GDI can't draw over the GPU's swap chain
            SendLeaders([]); // the handler draws them
        }
        remove
        {
            _paintMarkupPane -= value;
            AttachGpu();
            if (_paintMarkupPane is null) PlaceBalloons(); // the engine draws the leaders again
        }
    }

    /// <summary>Whether the engine draws with the GPU when it can (Graphite on Dawn: D3D12, D3D11, then Vulkan). Off, or
    /// with no usable GPU, it draws with the CPU.</summary>
    [DefaultValue(true)]
    public bool UseGpu
    {
        get => _gpu;
        set
        {
            _gpu = value;
            if (value) AttachGpu();
            else DetachGpu();
        }
    }

    /// <summary>
    /// A phase of the caret's blink (at least 100 ms); null is a steady caret. Until set, it follows Windows' caret
    /// blink rate (Control Panel's keyboard settings), and is steady when Windows doesn't blink the caret.
    /// </summary>
    [Browsable(false), DesignerSerializationVisibility(DesignerSerializationVisibility.Hidden)]
    public TimeSpan? CaretBlink
    {
        get => _caretBlinkSet ? _caretBlink : SystemCaretBlink();
        set
        {
            (_caretBlink, _caretBlinkSet) = (value, true);
            if (_host is { } h) h.CaretBlink = value;
        }
    }

    /// <summary>
    /// The colours of what the editor draws over the document. Until set, the engine's defaults, or Windows' system
    /// colours while high contrast is on.
    /// </summary>
    [Browsable(false), DesignerSerializationVisibility(DesignerSerializationVisibility.Hidden)]
    public OverlayColors OverlayColors
    {
        get => _overlayColors ?? SystemOverlayColors();
        set
        {
            _overlayColors = value;
            if (_host is { } h) h.OverlayColors = value;
        }
    }

    private static OverlayColors SystemOverlayColors() => SystemInformation.HighContrast
        ? new OverlayColors(SystemColors.WindowText, SystemColors.Highlight, SystemColors.WindowText, SystemColors.WindowText, SystemColors.GrayText, SystemColors.GrayText)
        : OverlayColors.Default;

    /// <summary>Windows' caret blink: <c>GetCaretBlinkTime</c>, which is -1 (INFINITE) when the caret doesn't blink.</summary>
    private static TimeSpan? SystemCaretBlink() => SystemInformation.CaretBlinkTime is > 0 and var ms ? TimeSpan.FromMilliseconds(ms) : null;

    private void OnUserPreferenceChanged(object? sender, Microsoft.Win32.UserPreferenceChangedEventArgs e)
    {
        if (_host is not { } h) return;
        BeginInvoke(() =>
        {
            if (!_caretBlinkSet) h.CaretBlink = SystemCaretBlink();
            if (_overlayColors is null) h.OverlayColors = SystemOverlayColors();
        });
    }

    /// <summary>The GPU backend the editor draws with ("D3D12", "D3D11" or "Vulkan"), or null when it draws with the CPU.</summary>
    [Browsable(false), DesignerSerializationVisibility(DesignerSerializationVisibility.Hidden)]
    public string? GpuBackend => _canvas is null ? null : _host?.GpuBackend;

    /// <summary>An external drag (a field list beside the editor) is over the point: shows the drop caret.</summary>
    /// <param name="client">The point, in client pixels.</param>
    public void DragOverElementTarget(Point client) => AttachedHost.OnDragOver(client.X, client.Y);

    /// <summary>The external drag left.</summary>
    public void DragLeaveElementTarget() => AttachedHost.OnDragLeave();

    /// <summary>Drops at the point: runs the operation there as one transaction.</summary>
    /// <param name="client">The point, in client pixels.</param>
    /// <param name="operation">The operation.</param>
    /// <returns>Completes when it has committed.</returns>
    public Task DropAsync(Point client, EditorOperation operation) => AttachedHost.OnDropAsync(client.X, client.Y, operation);

    /// <summary>What is under a point, without moving the selection.</summary>
    /// <param name="client">The point, in client pixels.</param>
    /// <returns>The hit.</returns>
    public Task<HitResult> HitTestAsync(Point client) => AttachedHost.HitTestAsync(client.X, client.Y);

    /// <summary>Scrolls the selection into view.</summary>
    /// <returns>Completes when it is in view.</returns>
    public Task RevealSelectionAsync() => AttachedHost.RevealSelectionAsync();

    private EditorHost AttachedHost => _host ?? throw new InvalidOperationException("the editor has no window yet");

    /// <summary>The area the frame covers: the client area less the scrollbars.</summary>
    private Size Viewport => new(
        Math.Max(ClientSize.Width - (_vertical.Visible ? _vertical.Width : 0), 0),
        Math.Max(ClientSize.Height - (_horizontal.Visible ? _horizontal.Height : 0), 0));

    /// <inheritdoc/>
    protected override AccessibleObject CreateAccessibilityInstance() => new DocumentAccessibleObject(this);

    /// <inheritdoc/>
    protected override void OnHandleCreated(EventArgs e)
    {
        base.OnHandleCreated(e);
        if (_designTime) return;
        _host = Session.Attach(new WinFormsAdapter(this));
        (_host.HorizontalRuler, _host.VerticalRuler, _host.RulerUnit, _host.TabSelector, _host.MarkupPaneWidth, _host.Background) =
            (_horizontalRuler, _verticalRuler, _rulerUnit, _tabSelector, _markupPaneWidth, BackColor);
        SendSize();
        _scrolled = null;
        SendScroll();
        _host.OnVisible(Visible);
        if (Focused) _host.OnFocus(true);
        State.Update(Session.State);
        _host.GpuLost += () => BeginInvoke(OnGpuLost);
        _host.CaretBlink = CaretBlink;
        if (_overlayColors is not null || SystemInformation.HighContrast) _host.OverlayColors = OverlayColors;
        AttachGpu();
    }

    /// <summary>The waits before each try at the GPU again after a device loss; CPU frames for the session after them.</summary>
    private static readonly TimeSpan[] GpuRetries = [TimeSpan.FromSeconds(1), TimeSpan.FromSeconds(5), TimeSpan.FromSeconds(30)];

    /// <summary>CPU frames while the GPU recovers, then the GPU again (a new device) when one of the retries gets it.</summary>
    private async void OnGpuLost()
    {
        DetachGpu();
        var loss = ++_gpuLosses;
        foreach (var wait in GpuRetries)
        {
            await Task.Delay(wait);
            // Another loss, the GPU turned off, or the window gone: those decide from here.
            if (loss != _gpuLosses || !_gpu || _host is null || IsDisposed) return;
            AttachGpu();
            if (_canvas is not null) return;
        }
    }

    /// <summary>Has the engine draw into a child window with the GPU, when it can and nothing paints over the frame.</summary>
    private void AttachGpu()
    {
        if (!_gpu || _canvas is not null || _paintMarkupPane is not null || _host is null) return;
        var canvas = new GpuCanvas { Bounds = new Rectangle(Point.Empty, Viewport) };
        Controls.Add(canvas);
        canvas.SendToBack(); // the scrollbars and balloon cards stay above it
        if (_host.AttachWindow(canvas.Handle) is null)
        {
            Controls.Remove(canvas);
            canvas.Dispose();
            return;
        }
        canvas.Exposed += () => _host?.RequestFrame();
        _canvas = canvas;
    }

    /// <summary>Back to CPU frames; the engine lets go of the window before it goes.</summary>
    private void DetachGpu()
    {
        if (_canvas is not { } canvas) return;
        _host?.DetachWindow();
        _canvas = null;
        Controls.Remove(canvas);
        canvas.Dispose();
        Invalidate();
    }

    /// <inheritdoc/>
    protected override void OnHandleDestroyed(EventArgs e)
    {
        // Handle recreation detaches and reattaches; the session keeps the document and history.
        DetachGpu();
        _caret.Destroy();
        _host?.Dispose();
        _host = null;
        base.OnHandleDestroyed(e);
    }

    /// <inheritdoc/>
    protected override void OnPaintBackground(PaintEventArgs pevent)
    {
        // The frame covers the client area; OnPaint clears what it doesn't.
    }

    /// <inheritdoc/>
    protected override void OnPaint(PaintEventArgs e)
    {
        if (_host is null)
        {
            e.Graphics.Clear(BackColor);
            if (_designTime) TextRenderer.DrawText(e.Graphics, "Twip DocumentEditor", Font, ClientRectangle, ForeColor, TextFormatFlags.HorizontalCenter | TextFormatFlags.VerticalCenter);
            base.OnPaint(e);
            return;
        }
        if (_canvas is not null && _printing) _host.RenderPixels();
        else if (_canvas is not null)
        {
            // The GPU draws the frame in the canvas; only the corner between the scrollbars shows here.
            e.Graphics.Clear(BackColor);
            base.OnPaint(e);
            return;
        }
        _host.Render();
        if (_frame is null) e.Graphics.Clear(BackColor);
        else
        {
            e.Graphics.DrawImageUnscaled(_frame, 0, 0);
            using var brush = new SolidBrush(BackColor);
            if (_frame.Width < ClientSize.Width) e.Graphics.FillRectangle(brush, _frame.Width, 0, ClientSize.Width - _frame.Width, ClientSize.Height);
            if (_frame.Height < ClientSize.Height) e.Graphics.FillRectangle(brush, 0, _frame.Height, ClientSize.Width, ClientSize.Height - _frame.Height);
        }
        if (_markupPaneWidth > 0) _paintMarkupPane?.Invoke(this, e);
        base.OnPaint(e);
    }

    /// <summary>Copies the engine's RGBA frame into the control's BGRA bitmap.</summary>
    internal unsafe void Blit(in FrameView frame)
    {
        if (_frame is null || _frame.Width != frame.Width || _frame.Height != frame.Height)
        {
            _frame?.Dispose();
            _frame = new Bitmap(frame.Width, frame.Height, PixelFormat.Format32bppPArgb);
        }
        var data = _frame.LockBits(new Rectangle(0, 0, frame.Width, frame.Height), ImageLockMode.WriteOnly, PixelFormat.Format32bppPArgb);
        try
        {
            for (var y = 0; y < frame.Height; y++)
            {
                var src = frame.Pixels.Slice(y * frame.Stride, frame.Width * 4);
                var dst = new Span<byte>((byte*)data.Scan0 + (long)y * data.Stride, frame.Width * 4);
                for (var x = 0; x < src.Length; x += 4)
                {
                    dst[x] = src[x + 2];
                    dst[x + 1] = src[x + 1];
                    dst[x + 2] = src[x];
                    dst[x + 3] = src[x + 3];
                }
            }
        }
        finally
        {
            _frame.UnlockBits(data);
        }
    }

    /// <inheritdoc/>
    protected override void OnSizeChanged(EventArgs e)
    {
        base.OnSizeChanged(e);
        SendSize();
        UpdateScrollbars();
    }

    /// <inheritdoc/>
    protected override void OnDpiChangedAfterParent(EventArgs e)
    {
        base.OnDpiChangedAfterParent(e);
        SendSize();
    }

    private void SendSize()
    {
        if (_canvas is { } canvas) canvas.Bounds = new Rectangle(Point.Empty, Viewport);
        _host?.OnResize(Viewport.Width, Viewport.Height, DeviceDpi);
    }

    /// <inheritdoc/>
    protected override void OnBackColorChanged(EventArgs e)
    {
        base.OnBackColorChanged(e);
        if (_host is { } h) h.Background = BackColor;
    }

    /// <inheritdoc/>
    protected override void OnVisibleChanged(EventArgs e)
    {
        base.OnVisibleChanged(e);
        _host?.OnVisible(Visible);
    }

    /// <inheritdoc/>
    protected override void OnGotFocus(EventArgs e)
    {
        base.OnGotFocus(e);
        _host?.OnFocus(true);
        _caret.Focus(true);
    }

    /// <inheritdoc/>
    protected override void OnLostFocus(EventArgs e)
    {
        base.OnLostFocus(e);
        _host?.OnFocus(false);
        _caret.Focus(false);
        ShowTooltip(null);
    }

    // Scrolling: the control owns the scroll position and tells the engine.

    private void OnScrollbar(object? sender, EventArgs e) => SendScroll();

    /// <summary>Tells the engine the scroll position. A document narrower than the view is centred in it.</summary>
    private void SendScroll()
    {
        if (_host is null) return;
        var spare = Viewport.Width - _host.ContentSize.Width;
        var at = (X: spare > 0 ? -Math.Floor(spare / 2) : _horizontal.Value, Y: (double)_vertical.Value);
        if (at == _scrolled) return;
        _scrolled = at;
        _host.OnScroll(at.X, at.Y);
        PlaceBalloons();
    }

    /// <summary>Fits the scrollbars to the document's size after a frame.</summary>
    internal void UpdateScrollbars()
    {
        if (_host is null) return;
        var content = _host.ContentSize;
        var wide = content.Width > ClientSize.Width - _vertical.Width;
        if (_horizontal.Visible != wide)
        {
            _horizontal.Visible = wide;
            SendSize();
        }
        var view = Viewport;
        var step = (int)Math.Ceiling(40 * DeviceDpi / 96.0);
        Fit(_vertical, (int)Math.Ceiling(content.Height), view.Height, step);
        Fit(_horizontal, (int)Math.Ceiling(content.Width), view.Width, step);
        SendScroll();

        static void Fit(ScrollBar bar, int content, int view, int step)
        {
            // A WinForms scrollbar reaches Maximum - LargeChange + 1.
            bar.Enabled = content > view;
            bar.LargeChange = Math.Max(view, 1);
            bar.SmallChange = step;
            bar.Maximum = Math.Max(content - 1, 0);
            bar.Value = Math.Clamp(bar.Value, 0, MaxValue(bar));
        }
    }

    private static int MaxValue(ScrollBar bar) => Math.Max(bar.Maximum - bar.LargeChange + 1, 0);

    /// <summary>Scrolls so a content point, in client pixels, is at the top left.</summary>
    internal void ScrollTo(double x, double y)
    {
        _horizontal.Value = Math.Clamp((int)Math.Round(x), 0, MaxValue(_horizontal));
        _vertical.Value = Math.Clamp((int)Math.Round(y), 0, MaxValue(_vertical));
    }

    // Platform services the adapter forwards

    internal void MoveCaret(CaretInfo caret)
    {
        _caret.Move(caret);
        _ime.Move(caret.Bounds);
    }

    internal void ShowTooltip(TooltipInfo? tooltip)
    {
        if (tooltip is null) _tooltip.Hide(this);
        else _tooltip.Show(tooltip.Text, this, (int)tooltip.Location.X, (int)tooltip.Location.Y);
    }

    internal void UpdateAccessibility(AccessibleNode tree)
    {
        ((DocumentAccessibleObject)AccessibilityObject).Update(tree);
        AccessibilityNotifyClients(AccessibleEvents.Reorder, -1);
    }

    internal void Announce(string text) => ((DocumentAccessibleObject)AccessibilityObject).Announce(text);

    // Comment balloons

    private void OnCommentAnchorsChanged(object? sender, CommentAnchorsChangedEventArgs e)
    {
        // Anchors come only while attached, on the UI thread.
        if (InvokeRequired || IsDisposed) return;
        _anchors = e.Anchors;
        PlaceBalloons();
    }

    /// <summary>Stacks the attached cards beside their threads' lines, and hides the cards whose thread isn't in view.</summary>
    private void PlaceBalloons()
    {
        if (_balloons.Count == 0)
        {
            SendLeaders([]);
            return;
        }
        var anchors = _anchors.Where(a => _balloons.ContainsKey(a.Thread)).ToList();
        var gap = 8 * DeviceDpi / 96.0;
        var tops = Session.Review.StackBalloons(anchors, [.. anchors.Select(a => (double)_balloons[a.Thread].Height)], gap);
        var placements = new List<BalloonPlacement>(anchors.Count);
        for (var i = 0; i < anchors.Count; i++)
        {
            var (a, card) = (anchors[i], _balloons[anchors[i].Thread]);
            var (left, top) = ((int)Math.Round(a.PageEdge + gap), (int)Math.Round(tops[i]));
            card.Location = new Point(left, top);
            card.Visible = true;
            var line = new Point((int)Math.Round(a.PageEdge), (int)Math.Round(a.End.Y));
            placements.Add(new BalloonPlacement(a.Thread, top, left, [new Point((int)Math.Round(a.End.X), line.Y), line, new Point(left, top + Math.Min(card.Height / 2, 12))]));
        }
        foreach (var (thread, card) in _balloons)
        {
            if (!anchors.Exists(a => a.Thread == thread)) card.Visible = false;
        }
        BalloonsLaidOut?.Invoke(this, new BalloonsLaidOutEventArgs(placements));
        SendLeaders(placements);
        if (_markupPaneWidth > 0) Invalidate();
    }

    /// <summary>The engine draws the leader lines while no <see cref="PaintMarkupPane"/> handler draws them, so balloons
    /// keep the GPU path.</summary>
    private void SendLeaders(IReadOnlyList<BalloonPlacement> placements)
    {
        if (_host is null) return;
        var lines = _paintMarkupPane is null ? placements.Select(p => (IReadOnlyList<PointF>)[.. p.Leader.Select(q => (PointF)q)]).ToList() : [];
        if (lines.Count == 0 && !_leadersSent) return;
        _leadersSent = lines.Count > 0;
        _host.SetLeaders(lines);
    }

    private sealed class Balloon(Action remove) : IDisposable
    {
        private Action? _remove = remove;

        public void Dispose()
        {
            _remove?.Invoke();
            _remove = null;
        }
    }

    // Input

    /// <inheritdoc/>
    protected override bool IsInputKey(Keys keyData) => _host is not null || base.IsInputKey(keyData);

    /// <inheritdoc/>
    protected override bool ProcessCmdKey(ref Message msg, Keys keyData)
    {
        // Ctrl and Alt chords reach the engine before the form's menus and accelerators: a key the engine owns (Ctrl+B)
        // stays here, and an application key (Ctrl+S) raises HostCommandRequested instead.
        if (_host is not null && (msg.Msg is WmKeyDown or WmSysKeyDown) && (keyData & (Keys.Control | Keys.Alt)) != 0)
        {
            var key = KeyMap.ToEditorKey(keyData & Keys.KeyCode);
            if (key != EditorKey.None)
            {
                _routed = keyData;
                _host.OnKey(key, KeyMap.Modifiers(keyData), out var handled);
                if (handled) return true;
            }
        }
        return base.ProcessCmdKey(ref msg, keyData);
    }

    /// <inheritdoc/>
    protected override void OnKeyDown(KeyEventArgs e)
    {
        base.OnKeyDown(e);
        ModifierChanged(e);
        if (e.Handled || _host is null) return;
        if (e.KeyData == _routed)
        {
            // ProcessCmdKey gave it to the engine, which left it to the platform.
            _routed = Keys.None;
            return;
        }
        var key = KeyMap.ToEditorKey(e.KeyCode);
        if (key == EditorKey.None) return;
        _host.OnKey(key, KeyMap.Modifiers(e.Modifiers), out var handled);
        if (handled)
        {
            e.Handled = true;
            e.SuppressKeyPress = true;
        }
    }

    /// <inheritdoc/>
    protected override void OnKeyUp(KeyEventArgs e)
    {
        _routed = Keys.None;
        base.OnKeyUp(e);
        ModifierChanged(e);
    }

    /// <summary>Ctrl, Shift or the Windows key pressed or released with the pointer still over the document: the same
    /// move again with the new modifiers, so the cursor follows what a press would now do (the hand over a link with
    /// Ctrl).</summary>
    private void ModifierChanged(KeyEventArgs e)
    {
        if (_host is null || e.KeyCode is not (Keys.ControlKey or Keys.ShiftKey or Keys.LWin or Keys.RWin)) return;
        var at = PointToClient(MousePosition);
        if (!ClientRectangle.Contains(at)) return;
        var button = MouseButtons switch
        {
            MouseButtons.Right => PointerButton.Secondary,
            MouseButtons.Middle => PointerButton.Middle,
            _ => PointerButton.Primary,
        };
        _host.OnPointer(PointerKind.Move, at.X, at.Y, button, 0, KeyMap.Modifiers(ModifierKeys));
    }

    /// <inheritdoc/>
    protected override void WndProc(ref Message m)
    {
        const int WmPrintClient = 0x0318;
        if (m.Msg == WmPrintClient)
        {
            _printing = true;
            try { base.WndProc(ref m); }
            finally { _printing = false; }
            return;
        }
        if (_host is not null)
        {
            if (m.Msg == WmChar && OnChar((char)(nint)m.WParam)) return;
            if (_ime.WndProc(ref m, _host)) return;
        }
        base.WndProc(ref m);
    }

    /// <summary>Forwards a typed character; control characters come through <see cref="OnKeyDown"/> instead.</summary>
    private bool OnChar(char c)
    {
        if (char.IsHighSurrogate(c))
        {
            _highSurrogate = c;
            return true;
        }
        if (char.IsLowSurrogate(c))
        {
            if (_highSurrogate != 0) _host!.OnText(new string([_highSurrogate, c]));
            _highSurrogate = '\0';
            return true;
        }
        _highSurrogate = '\0';
        if (char.IsControl(c)) return false;
        _host!.OnText(c.ToString());
        return true;
    }

    /// <inheritdoc/>
    protected override void OnMouseDown(MouseEventArgs e)
    {
        base.OnMouseDown(e);
        if (!Focused) Focus();
        if (_host is null) return;
        _host.OnPointer(PointerKind.Down, e.X, e.Y, ButtonOf(e), e.Clicks, KeyMap.Modifiers(ModifierKeys));
    }

    /// <inheritdoc/>
    protected override void OnMouseMove(MouseEventArgs e)
    {
        base.OnMouseMove(e);
        _host?.OnPointer(PointerKind.Move, e.X, e.Y, ButtonOf(e), 0, KeyMap.Modifiers(ModifierKeys));
    }

    /// <inheritdoc/>
    protected override void OnMouseUp(MouseEventArgs e)
    {
        base.OnMouseUp(e);
        _host?.OnPointer(PointerKind.Up, e.X, e.Y, ButtonOf(e), e.Clicks, KeyMap.Modifiers(ModifierKeys));
    }

    /// <inheritdoc/>
    protected override void OnMouseCaptureChanged(EventArgs e)
    {
        base.OnMouseCaptureChanged(e);
        // Losing a capture the engine asked for (another window took it) cancels the drag.
        if (!Capture && MouseButtons != MouseButtons.None) _host?.OnPointer(PointerKind.Cancel, 0, 0, PointerButton.Primary, 0, KeyModifiers.None);
    }

    private static PointerButton ButtonOf(MouseEventArgs e) => e.Button switch
    {
        MouseButtons.Right => PointerButton.Secondary,
        MouseButtons.Middle => PointerButton.Middle,
        _ => PointerButton.Primary,
    };

    /// <inheritdoc/>
    protected override void OnMouseLeave(EventArgs e)
    {
        base.OnMouseLeave(e);
        _host?.OnPointer(PointerKind.Leave, 0, 0, PointerButton.Primary, 0, KeyModifiers.None);
    }

    /// <inheritdoc/>
    protected override void OnMouseWheel(MouseEventArgs e)
    {
        base.OnMouseWheel(e);
        var lines = SystemInformation.MouseWheelScrollLines;
        // A notch (120) scrolls the system's line count, a line being 16 px at 100%; -1 means a page per notch.
        var line = 16 * DeviceDpi / 96.0;
        var notch = lines < 0 ? Viewport.Height : lines * line;
        var modifiers = KeyMap.Modifiers(ModifierKeys);
        var delta = -e.Delta / 120.0 * notch;
        if ((modifiers & KeyModifiers.Shift) != 0) _host?.OnWheel(delta, 0, modifiers & ~KeyModifiers.Shift);
        else _host?.OnWheel(0, delta, modifiers);
    }

    // State and lifetime

    private void OnStateChanged(object? sender, StateChangedEventArgs e)
    {
        // Attached, this runs on the UI thread. Before the handle exists, OnHandleCreated catches up instead.
        if (!IsHandleCreated || IsDisposed) return;
        if (InvokeRequired) BeginInvoke(() => State.Update(Session.State));
        else State.Update(e.Snapshot);
    }

    private static void Observe(Task task) => task.ContinueWith(static t => _ = t.Exception, CancellationToken.None, TaskContinuationOptions.OnlyOnFaulted, TaskScheduler.Default);

    /// <inheritdoc/>
    protected override void Dispose(bool disposing)
    {
        if (disposing)
        {
            Session.StateChanged -= OnStateChanged;
            Session.CommentAnchorsChanged -= OnCommentAnchorsChanged;
            Microsoft.Win32.SystemEvents.UserPreferenceChanged -= OnUserPreferenceChanged;
            _caret.Destroy();
            _host?.Dispose();
            _host = null;
            if (_ownsSession) Session.Dispose();
            _tooltip.Dispose();
            _frame?.Dispose();
            _frame = null;
        }
        base.Dispose(disposing);
    }
}

/// <summary>The attached balloons' placement.</summary>
public sealed class BalloonsLaidOutEventArgs : EventArgs
{
    internal BalloonsLaidOutEventArgs(IReadOnlyList<BalloonPlacement> placements) => Placements = placements;

    /// <summary>Each balloon's position and leader line.</summary>
    public IReadOnlyList<BalloonPlacement> Placements { get; }
}

/// <summary>Where a balloon was placed.</summary>
/// <param name="Thread">The comment thread.</param>
/// <param name="Top">The card's top, in client pixels.</param>
/// <param name="Left">The card's left, in client pixels.</param>
/// <param name="Leader">The leader line, as a polyline in client pixels.</param>
public sealed record BalloonPlacement(CommentId Thread, int Top, int Left, IReadOnlyList<Point> Leader);
