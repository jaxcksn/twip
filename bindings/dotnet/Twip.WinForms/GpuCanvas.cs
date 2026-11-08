namespace Twip.WinForms;

/// <summary>
/// The child window the engine draws the frame into with the GPU (a swap chain on it). It paints nothing itself, and
/// is transparent to the mouse, so input reaches the <see cref="DocumentEditor"/> beneath it.
/// </summary>
internal sealed class GpuCanvas : Control
{
    public GpuCanvas()
    {
        SetStyle(ControlStyles.UserPaint | ControlStyles.AllPaintingInWmPaint | ControlStyles.Opaque, true);
        SetStyle(ControlStyles.Selectable, false);
        TabStop = false;
    }

    /// <summary>The window was uncovered: the swap chain keeps its last frame, but ask for a new one anyway.</summary>
    public event Action? Exposed;

    /// <inheritdoc/>
    protected override void OnPaint(PaintEventArgs e) => Exposed?.Invoke();

    /// <inheritdoc/>
    protected override void WndProc(ref Message m)
    {
        const int WmNcHitTest = 0x0084, HtTransparent = -1;
        if (m.Msg == WmNcHitTest)
        {
            m.Result = (nint)HtTransparent;
            return;
        }
        base.WndProc(ref m);
    }
}
