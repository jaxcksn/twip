using System.Runtime.InteropServices;
using Twip.Hosting;

namespace Twip.WinForms;

/// <summary>
/// An invisible system caret kept where the engine draws its caret, so Magnifier and Narrator follow typing. It is
/// never shown: the engine paints the visible one.
/// </summary>
internal sealed partial class SystemCaret(Control control)
{
    private RectangleF _bounds;
    private Size _size;
    private bool _created;

    /// <summary>The caret moved.</summary>
    public void Move(CaretInfo caret)
    {
        _bounds = caret.Bounds;
        if (control.Focused) Place();
    }

    /// <summary>The control got or lost the focus: the caret exists only while it has it.</summary>
    public void Focus(bool focused)
    {
        if (focused) Place();
        else Destroy();
    }

    public void Destroy()
    {
        if (!_created) return;
        DestroyCaret();
        _created = false;
    }

    private void Place()
    {
        if (!control.IsHandleCreated) return;
        var size = new Size(Math.Max(1, (int)Math.Round(_bounds.Width)), Math.Max(1, (int)Math.Round(_bounds.Height)));
        if (!_created || size != _size)
        {
            Destroy();
            _created = CreateCaret(control.Handle, 0, size.Width, size.Height);
            _size = size;
        }
        if (_created) SetCaretPos((int)Math.Round(_bounds.X), (int)Math.Round(_bounds.Y));
    }

    [DllImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool CreateCaret(nint hwnd, nint bitmap, int width, int height);

    [DllImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool SetCaretPos(int x, int y);

    [DllImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool DestroyCaret();
}
