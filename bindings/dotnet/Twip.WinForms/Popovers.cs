using System.Drawing.Drawing2D;
using System.Globalization;
using WinRole = System.Windows.Forms.AccessibleRole;

namespace Twip.WinForms;

/// <summary>
/// The editor's own popovers (the tab type picker, a tab stop's options, the AutoCorrect menu): an owner-drawn
/// drop-down in the editor's chrome colours, answered with the popover's commands.
/// </summary>
internal sealed class PopoverDropDown : ToolStripDropDown
{
    private enum Look { Row, Cell, Command }

    private sealed record Item(Rectangle Bounds, string Name, Look Look, bool Checked, Action Run, bool StaysOpen = false)
    {
        public string? Detail { get; init; }
        public string? Hint { get; init; }
        public TabSelector? Glyph { get; init; }
        public bool Enabled { get; init; } = true;
    }

    private readonly DocumentEditor _editor;
    private readonly List<Item> _items = [];
    private readonly List<(Rectangle At, string Text, Font Font, Color Color, bool Struck)> _texts = [];
    private readonly List<int> _rules = []; // the y of each separator line
    private readonly Font _body, _small, _label, _mono, _monoBold;
    private Palette _c;
    private Popover _popover;
    private int _focus = -1, _hover = -1;

    internal PopoverDropDown(DocumentEditor editor, Popover popover)
    {
        _editor = editor;
        _popover = popover;
        AutoSize = false;
        Padding = Padding.Empty;
        DoubleBuffered = true;
        var family = editor.Font.FontFamily;
        float Px(float px) => px * 72 / 96; // CSS px to points; the drop-down scales with the DPI
        _body = new Font(family, Px(13), GraphicsUnit.Point);
        _small = new Font(family, Px(11.5f), GraphicsUnit.Point);
        _label = new Font(family, Px(10), FontStyle.Bold, GraphicsUnit.Point);
        _mono = new Font(FontFamily.GenericMonospace, Px(10), GraphicsUnit.Point);
        _monoBold = new Font(FontFamily.GenericMonospace, Px(11), FontStyle.Bold, GraphicsUnit.Point);
        AccessibleRole = WinRole.MenuPopup;
        Build();
        _focus = Math.Max(_items.FindIndex(i => i.Checked), 0);
    }

    /// <summary>The popover shown.</summary>
    internal Popover Popover => _popover;

    /// <summary>The entries' names, in order, with whether each is checked: for tests.</summary>
    internal IReadOnlyList<(string Name, bool Checked)> Entries => [.. _items.Select(i => (i.Name, i.Checked))];

    /// <summary>Picks the entry named <paramref name="name"/>, as a click does.</summary>
    internal void Pick(string name) => Run(_items.FindIndex(i => i.Name == name));

    /// <summary>Shows it under its anchor, in the editor's client pixels.</summary>
    internal void ShowAt()
    {
        var anchor = _popover switch
        {
            Popover.TabType t => t.Anchor,
            Popover.TabStop t => t.Anchor,
            Popover.AutoCorrect a => a.Anchor,
            _ => RectangleF.Empty,
        };
        // A tab stop's options centre on the stop; the others start at their anchor's left.
        var x = _popover is Popover.TabStop ? anchor.Left + anchor.Width / 2 - Width / 2f : anchor.Left;
        Show(_editor, new Point((int)Math.Round(x), (int)Math.Round(anchor.Bottom + S(4))));
        Focus();
        Notify();
    }

    private float Dpi => _editor.DeviceDpi / 96f;

    private int S(float px) => (int)Math.Round(px * Dpi);

    /// <summary>Lays the popover out from its model and the editor's colours.</summary>
    private void Build()
    {
        _items.Clear();
        _texts.Clear();
        _rules.Clear();
        _c = Palette.From(_editor.Session.State.View);
        var commands = _editor.Commands;
        switch (_popover)
        {
            case Popover.TabType t:
            {
                var w = S(260);
                var y = S(4);
                _texts.Add((new Rectangle(S(12), y + S(8), w, S(12)), "TAB TYPE FOR NEW STOPS", _label, _c.Ink3, false));
                y += S(26);
                foreach (var choice in t.Choices)
                {
                    var value = choice.Value;
                    _items.Add(new Item(new Rectangle(S(4), y, w - S(8), S(46)), choice.Name, Look.Row, value == t.Current, () =>
                    {
                        _editor.TabSelectorPicked(value);
                        DocumentEditor.Observe(commands.SetRulerTabTypeAsync(value));
                    }) { Detail = choice.Description, Glyph = value });
                    y += S(46);
                }
                y += S(4);
                _rules.Add(y);
                _texts.Add((new Rectangle(S(12), y + S(8), w - S(24), S(16)), "Then click the ruler to place it.", _small, _c.Ink2, false));
                Size = new Size(w, y + S(34));
                AccessibleName = "Tab type for new stops";
                break;
            }
            case Popover.TabStop t:
            {
                var w = S(240);
                var name = t.Alignment switch
                {
                    TabAlignment.Center => "Center",
                    TabAlignment.Right => "Right",
                    TabAlignment.Decimal => "Decimal",
                    TabAlignment.Bar => "Bar",
                    _ => "Left",
                };
                var heading = $"{name} tab · {Format(t.Position, _editor.RulerUnit)}";
                _texts.Add((new Rectangle(S(12), S(10), w - S(24), S(14)), heading, _monoBold, _c.Ink, false));
                _rules.Add(S(32));
                _texts.Add((new Rectangle(S(12), S(42), w, S(12)), "ALIGNMENT", _label, _c.Ink3, false));
                var cell = (w - S(24) - 3 * S(4)) / 4;
                var aligns = new[] { (TabAlignment.Left, TabSelector.Left, "Left"), (TabAlignment.Center, TabSelector.Center, "Center"), (TabAlignment.Right, TabSelector.Right, "Right"), (TabAlignment.Decimal, TabSelector.Decimal, "Decimal") };
                for (var i = 0; i < aligns.Length; i++)
                {
                    var (alignment, glyph, label) = aligns[i];
                    _items.Add(new Item(new Rectangle(S(12) + i * (cell + S(4)), S(58), cell, S(40)), label, Look.Cell, alignment == t.Alignment, () => Answer(t with { Alignment = alignment }), StaysOpen: true) { Glyph = glyph });
                }
                _texts.Add((new Rectangle(S(12), S(112), w, S(12)), "LEADER", _label, _c.Ink3, false));
                var half = (w - S(24) - S(4)) / 2;
                _items.Add(new Item(new Rectangle(S(12), S(128), half, S(28)), "None", Look.Cell, t.Leader == TabLeader.None, () => Answer(t with { Leader = TabLeader.None }), StaysOpen: true));
                _items.Add(new Item(new Rectangle(S(16) + half, S(128), half, S(28)), "Dots ·····", Look.Cell, t.Leader == TabLeader.Dots, () => Answer(t with { Leader = TabLeader.Dots }), StaysOpen: true));
                _rules.Add(S(166));
                _items.Add(new Item(new Rectangle(0, S(166), w, S(32)), "Clear tab stop", Look.Command, false, () => DocumentEditor.Observe(commands.DeleteTabStopAsync(t.Position))));
                Size = new Size(w, S(198));
                AccessibleName = heading;
                break;
            }
            case Popover.AutoCorrect a:
            {
                var w = S(232);
                _texts.Add((new Rectangle(S(12), S(10), w, S(14)), a.From, _monoBold, _c.Ink3, true));
                var fromWidth = TextRenderer.MeasureText(a.From, _monoBold).Width;
                _texts.Add((new Rectangle(S(12) + fromWidth, S(10), S(20), S(14)), "→", _monoBold, _c.Ink3, false));
                _texts.Add((new Rectangle(S(32) + fromWidth, S(10), w, S(14)), a.To, _monoBold, _c.Accent, false));
                var tag = TextRenderer.MeasureText("autocorrect", _mono).Width;
                _texts.Add((new Rectangle(w - S(12) - tag, S(11), tag + S(4), S(14)), "autocorrect", _mono, _c.Ink3, false));
                _rules.Add(S(32));
                var y = S(36);
                var session = _editor.Session;
                _items.Add(new Item(new Rectangle(S(4), y, w - S(8), S(28)), $"Revert to {a.From}", Look.Command, false, () => DocumentEditor.Observe(commands.RevertAutoCorrectAsync())) { Hint = "Ctrl+Z" });
                _items.Add(new Item(new Rectangle(S(4), y + S(28), w - S(8), S(28)), $"Never correct {a.From}", Look.Command, false, () => DocumentEditor.Observe(commands.NeverAutoCorrectAsync())));
                _items.Add(new Item(new Rectangle(S(4), y + S(56), w - S(8), S(28)), "AutoCorrect rules…", Look.Command, false, () => session.RequestDialog(DialogKind.AutoCorrect)));
                Size = new Size(w, y + S(88));
                AccessibleName = $"AutoCorrect: {a.From} to {a.To}";
                break;
            }
        }
    }

    /// <summary>A tab stop's alignment or leader changed: the stop is added again at its position, and the popover stays.</summary>
    private void Answer(Popover.TabStop t)
    {
        DocumentEditor.Observe(_editor.Commands.AddTabStopAsync(t.Position, t.Alignment, t.Leader));
        _popover = t;
        var focus = _focus;
        Build();
        _focus = focus;
        Invalidate();
    }

    /// <summary>A length in the ruler's unit: <c>2.75″</c>.</summary>
    internal static string Format(Length length, RulerUnit unit)
    {
        var (value, suffix) = unit switch
        {
            RulerUnit.Centimeters => (length.Points / 72 * 2.54, " cm"),
            RulerUnit.Millimeters => (length.Points / 72 * 25.4, " mm"),
            RulerUnit.Points => (length.Points, " pt"),
            RulerUnit.Picas => (length.Points / 12, " pi"),
            _ => (length.Points / 72, "″"),
        };
        return Math.Round(value, 2).ToString("0.##", CultureInfo.InvariantCulture) + suffix;
    }

    private void Run(int index)
    {
        if ((uint)index >= (uint)_items.Count || !_items[index].Enabled) return;
        var item = _items[index];
        _focus = index;
        if (!item.StaysOpen) _editor.ClosePopover();
        item.Run();
        if (item.StaysOpen) Notify();
    }

    private void Step(int by)
    {
        if (_items.Count == 0) return;
        var i = _focus;
        for (var n = 0; n < _items.Count; n++)
        {
            i = (i + by + _items.Count) % _items.Count;
            if (_items[i].Enabled) break;
        }
        _focus = i;
        Invalidate();
        Notify();
    }

    /// <summary>Tells screen readers which entry has the focus.</summary>
    private void Notify()
    {
        if (IsHandleCreated && (uint)_focus < (uint)_items.Count) AccessibilityNotifyClients(AccessibleEvents.Focus, _focus);
    }

    /// <inheritdoc/>
    protected override bool ProcessDialogKey(Keys keyData)
    {
        switch (keyData)
        {
            case Keys.Down or Keys.Right or Keys.Tab: Step(1); return true;
            case Keys.Up or Keys.Left or (Keys.Shift | Keys.Tab): Step(-1); return true;
            case Keys.Home: _focus = 0; Step(0); return true;
            case Keys.End: _focus = _items.Count - 1; Step(0); return true;
            case Keys.Enter or Keys.Space: Run(_focus); return true;
            default: return base.ProcessDialogKey(keyData); // Esc closes
        }
    }

    /// <inheritdoc/>
    protected override void OnMouseMove(MouseEventArgs mea)
    {
        base.OnMouseMove(mea);
        var hover = _items.FindIndex(i => i.Bounds.Contains(mea.Location));
        if (hover == _hover) return;
        _hover = hover;
        Invalidate();
    }

    /// <inheritdoc/>
    protected override void OnMouseLeave(EventArgs e)
    {
        base.OnMouseLeave(e);
        _hover = -1;
        Invalidate();
    }

    /// <inheritdoc/>
    protected override void OnMouseUp(MouseEventArgs mea)
    {
        base.OnMouseUp(mea);
        if (mea.Button == MouseButtons.Left) Run(_items.FindIndex(i => i.Bounds.Contains(mea.Location)));
    }

    /// <inheritdoc/>
    protected override void OnPaintBackground(PaintEventArgs e)
    {
        using var pop = new SolidBrush(_c.Pop);
        e.Graphics.FillRectangle(pop, ClientRectangle);
    }

    /// <inheritdoc/>
    protected override void OnPaint(PaintEventArgs e)
    {
        var g = e.Graphics;
        g.SmoothingMode = SmoothingMode.None;
        using var line = new Pen(_c.Line);
        foreach (var y in _rules) g.DrawLine(line, 0, y, Width, y);
        foreach (var (at, text, font, color, struck) in _texts)
        {
            TextRenderer.DrawText(g, text, font, at, color, _c.Pop, TextFormatFlags.Left | TextFormatFlags.Top | TextFormatFlags.NoPadding | TextFormatFlags.SingleLine);
            if (!struck) continue;
            var width = TextRenderer.MeasureText(text, font).Width;
            using var strike = new Pen(color);
            g.DrawLine(strike, at.Left, at.Top + font.Height / 2, at.Left + width - S(6), at.Top + font.Height / 2);
        }
        for (var i = 0; i < _items.Count; i++) PaintItem(g, _items[i], i == _hover || i == _focus, i == _focus);
        g.DrawRectangle(line, 0, 0, Width - 1, Height - 1);
    }

    private void PaintItem(Graphics g, Item item, bool hot, bool focused)
    {
        var r = item.Bounds;
        var ink = item.Enabled ? _c.Ink : _c.Ink3;
        switch (item.Look)
        {
            case Look.Row:
            {
                // The current type: a light tint and a 2 px accent bar, its glyph in the accent.
                if (item.Checked || hot) Fill(g, _c.Line2, r);
                if (item.Checked || focused) Fill(g, _c.Accent, new Rectangle(r.Left, r.Top, S(2), r.Height));
                var text = hot && _c.HighContrast ? _c.HoverInk : ink;
                Glyph(g, item.Glyph!.Value, r.Left + S(8), r.Top + (r.Height - S(14)) / 2, item.Checked ? _c.Accent : text);
                TextRenderer.DrawText(g, item.Name, _body, new Point(r.Left + S(34), r.Top + S(9)), text, TextFormatFlags.NoPadding);
                TextRenderer.DrawText(g, item.Detail, _small, new Point(r.Left + S(34), r.Top + S(26)), hot && _c.HighContrast ? _c.HoverInk : _c.Ink2, TextFormatFlags.NoPadding);
                break;
            }
            case Look.Cell:
            {
                // The current choice fills with the accent.
                Fill(g, item.Checked ? _c.Accent : hot ? _c.Line2 : _c.Pop, r);
                using (var border = new Pen(focused ? _c.Accent : _c.Line)) g.DrawRectangle(border, r.Left, r.Top, r.Width - 1, r.Height - 1);
                var text = item.Checked ? _c.OnAccent : hot && _c.HighContrast ? _c.HoverInk : ink;
                if (item.Glyph is { } glyph)
                {
                    Glyph(g, glyph, r.Left + (r.Width - S(14)) / 2, r.Top + S(6), text);
                    TextRenderer.DrawText(g, item.Name, _label, new Rectangle(r.Left, r.Top + S(24), r.Width, S(12)), text, TextFormatFlags.HorizontalCenter | TextFormatFlags.NoPadding);
                }
                else TextRenderer.DrawText(g, item.Name, _small, r, text, TextFormatFlags.HorizontalCenter | TextFormatFlags.VerticalCenter | TextFormatFlags.NoPadding);
                break;
            }
            default:
            {
                if (hot) Fill(g, _c.Line2, r);
                if (hot) Fill(g, _c.Accent, new Rectangle(r.Left, r.Top, S(2), r.Height));
                var text = hot && _c.HighContrast ? _c.HoverInk : item.Name.EndsWith("…", StringComparison.Ordinal) || item.Name == "Clear tab stop" ? (hot ? _c.Ink : _c.Ink2) : ink;
                if (!item.Enabled) text = _c.Ink3;
                TextRenderer.DrawText(g, item.Name, _body, new Rectangle(r.Left + S(8), r.Top, r.Width - S(16), r.Height), text, TextFormatFlags.VerticalCenter | TextFormatFlags.NoPadding);
                if (item.Hint is { } hint) TextRenderer.DrawText(g, hint, _mono, new Rectangle(r.Left, r.Top, r.Width - S(8), r.Height), _c.Ink2, TextFormatFlags.Right | TextFormatFlags.VerticalCenter | TextFormatFlags.NoPadding);
                break;
            }
        }
    }

    private static void Fill(Graphics g, Color color, Rectangle r)
    {
        using var brush = new SolidBrush(color);
        g.FillRectangle(brush, r);
    }

    /// <summary>A tab type's mark, 14 px square: the ruler's stop glyphs, and the indent markers' triangles.</summary>
    private void Glyph(Graphics g, TabSelector type, int x, int y, Color color)
    {
        using var brush = new SolidBrush(color);
        void Box(float l, float t, float w, float h) => g.FillRectangle(brush, x + l * Dpi, y + t * Dpi, w * Dpi, h * Dpi);
        void Triangle(bool down)
        {
            var (top, bottom) = down ? (3f, 9f) : (9f, 3f);
            g.FillPolygon(brush, [new PointF(x + 2 * Dpi, y + top * Dpi), new PointF(x + 12 * Dpi, y + top * Dpi), new PointF(x + 7 * Dpi, y + bottom * Dpi)]);
        }
        switch (type)
        {
            case TabSelector.FirstLineIndent: Triangle(down: true); return;
            case TabSelector.HangingIndent: Triangle(down: false); return;
        }
        Box(6, 3, 2, 9);
        switch (type)
        {
            case TabSelector.Left: Box(6, 10, 7, 2); break;
            case TabSelector.Center: Box(2, 10, 10, 2); break;
            case TabSelector.Right: Box(1, 10, 7, 2); break;
            case TabSelector.Decimal: Box(2, 10, 10, 2); Box(10, 5, 2, 2); break;
        }
    }

    /// <inheritdoc/>
    protected override void OnClosed(ToolStripDropDownClosedEventArgs e)
    {
        base.OnClosed(e);
        // Focus goes back to the document.
        if (!_editor.IsDisposed) _editor.Focus();
    }

    /// <inheritdoc/>
    protected override AccessibleObject CreateAccessibilityInstance() => new PopoverAccessibleObject(this);

    /// <inheritdoc/>
    protected override void Dispose(bool disposing)
    {
        if (disposing)
        {
            _body.Dispose();
            _small.Dispose();
            _label.Dispose();
            _mono.Dispose();
            _monoBold.Dispose();
        }
        base.Dispose(disposing);
    }

    /// <summary>The menu and its entries for screen readers: radio items for the choices, menu items for the commands.</summary>
    private sealed class PopoverAccessibleObject(PopoverDropDown owner) : ControlAccessibleObject(owner)
    {
        public override WinRole Role => WinRole.MenuPopup;

        public override string? Name => owner.AccessibleName;

        public override int GetChildCount() => owner._items.Count;

        public override AccessibleObject? GetChild(int index) => (uint)index < (uint)owner._items.Count ? new Entry(owner, this, index) : null;

        public override AccessibleObject? GetFocused() => GetChild(owner._focus);
    }

    private sealed class Entry(PopoverDropDown owner, AccessibleObject parent, int index) : AccessibleObject
    {
        private Item Item => owner._items[index];

        public override AccessibleObject Parent => parent;

        public override WinRole Role => Item.Look == Look.Command ? WinRole.MenuItem : WinRole.RadioButton;

        public override string? Name => Item.Detail is { } detail ? $"{Item.Name}: {detail}" : Item.Name;

        public override string? KeyboardShortcut => Item.Hint;

        public override Rectangle Bounds => owner.RectangleToScreen(Item.Bounds);

        public override AccessibleStates State =>
            AccessibleStates.Focusable
            | (index == owner._focus ? AccessibleStates.Focused : 0)
            | (Item.Checked ? AccessibleStates.Checked : 0)
            | (Item.Enabled ? 0 : AccessibleStates.Unavailable);

        public override string? DefaultAction => "Press";

        public override void DoDefaultAction() => owner.Run(index);
    }
}

/// <summary>The popover colours: the editor's light, dark or high contrast chrome, with its accent.</summary>
internal readonly record struct Palette(Color Pop, Color Ink, Color Ink2, Color Ink3, Color Line, Color Line2, Color Accent, Color OnAccent, Color HoverInk, bool HighContrast)
{
    internal static Palette From(ViewState view)
    {
        static Color C(ColorValue c, Color fallback) => c.Color ?? fallback;
        var accent = C(view.Accent, Color.FromArgb(0x0A, 0x74, 0xE4));
        var on = C(view.AccentOn, Color.White);
        if (view.Theme.Colors is { } hc)
        {
            var (canvas, text, highlight, highlightText, gray) = (C(hc.Canvas, Color.Black), C(hc.CanvasText, Color.White), C(hc.Highlight, Color.Cyan), C(hc.HighlightText, Color.Black), C(hc.GrayText, Color.Gray));
            return new(canvas, text, text, gray, text, highlight, highlight, highlightText, highlightText, true);
        }
        return view.Theme.Kind == ChromeThemeKind.Dark
            ? new(Color.FromArgb(0x2B, 0x2C, 0x31), Color.FromArgb(0xE7, 0xE7, 0xEA), Color.FromArgb(0xA0, 0xA3, 0xAA), Color.FromArgb(0x5F, 0x62, 0x6A),
                Color.FromArgb(31, 255, 255, 255), Color.FromArgb(11, 255, 255, 255), accent, on, Color.FromArgb(0xE7, 0xE7, 0xEA), false)
            : new(Color.White, Color.FromArgb(0x1C, 0x1D, 0x1F), Color.FromArgb(0x62, 0x65, 0x6C), Color.FromArgb(0xA4, 0xA6, 0xAC),
                Color.FromArgb(31, 20, 22, 26), Color.FromArgb(13, 20, 22, 26), accent, on, Color.FromArgb(0x1C, 0x1D, 0x1F), false);
    }
}
