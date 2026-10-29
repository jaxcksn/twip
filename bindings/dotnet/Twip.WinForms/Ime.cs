using System.Runtime.InteropServices;
using Twip.Hosting;

namespace Twip.WinForms;

/// <summary>
/// IME composition through IMM32: the composition string goes to the engine, which draws it inline at the caret, and
/// the IME's candidate window is kept beside the caret.
/// </summary>
internal sealed partial class Ime(Control control)
{
    private const int WmImeSetContext = 0x0281;
    private const int WmImeStartComposition = 0x010D;
    private const int WmImeEndComposition = 0x010E;
    private const int WmImeComposition = 0x010F;
    private const int GcsCompStr = 0x0008;
    private const int GcsCursorPos = 0x0080;
    private const int GcsResultStr = 0x0800;
    private const uint IscShowUiCompositionWindow = 0x80000000;
    private const int CfsPoint = 0x0002;
    private const int CfsExclude = 0x0080;

    private RectangleF _caret;
    private bool _composing;

    /// <summary>Handles an IME message; false lets the control's default handling run.</summary>
    public bool WndProc(ref Message m, EditorHost host)
    {
        switch (m.Msg)
        {
            case WmImeSetContext:
                // The engine draws the composition: the IME shows only its candidates.
                m.LParam = (nint)((long)m.LParam & ~(long)IscShowUiCompositionWindow);
                return false;
            case WmImeStartComposition:
                Start(host);
                Place();
                return true;
            case WmImeComposition:
                var flags = (long)m.LParam;
                if ((flags & GcsResultStr) != 0)
                {
                    var result = Read(GcsResultStr);
                    if (!_composing) Start(host);
                    host.OnComposition(CompositionPhase.Commit, result, result.Length);
                    _composing = false;
                }
                if ((flags & GcsCompStr) != 0)
                {
                    var text = Read(GcsCompStr);
                    if (text.Length > 0 || _composing)
                    {
                        if (!_composing) Start(host);
                        host.OnComposition(CompositionPhase.Update, text, (flags & GcsCursorPos) != 0 ? CursorPos() : text.Length);
                    }
                }
                // Not to DefWindowProc, which would send the result again as WM_IME_CHAR.
                return true;
            case WmImeEndComposition:
                if (_composing) host.OnComposition(CompositionPhase.Cancel, "", 0);
                _composing = false;
                return true;
            default:
                return false;
        }
    }

    /// <summary>The caret moved: the candidate window follows it.</summary>
    public void Move(RectangleF caret)
    {
        _caret = caret;
        if (_composing) Place();
    }

    private void Start(EditorHost host)
    {
        host.OnComposition(CompositionPhase.Start, "", 0);
        _composing = true;
    }

    private unsafe string Read(int kind)
    {
        var context = ImmGetContext(control.Handle);
        if (context == 0) return "";
        try
        {
            var bytes = ImmGetCompositionStringW(context, kind, null, 0);
            if (bytes <= 0) return "";
            var buffer = new char[bytes / 2];
            fixed (char* p = buffer) ImmGetCompositionStringW(context, kind, p, bytes);
            return new string(buffer);
        }
        finally
        {
            ImmReleaseContext(control.Handle, context);
        }
    }

    private unsafe int CursorPos()
    {
        var context = ImmGetContext(control.Handle);
        if (context == 0) return 0;
        try { return Math.Max(0, ImmGetCompositionStringW(context, GcsCursorPos, null, 0)); }
        finally { ImmReleaseContext(control.Handle, context); }
    }

    private void Place()
    {
        var context = ImmGetContext(control.Handle);
        if (context == 0) return;
        try
        {
            var (x, top, bottom) = ((int)_caret.X, (int)_caret.Y, (int)Math.Ceiling(_caret.Bottom));
            var composition = new CompositionForm { Style = CfsPoint, X = x, Y = top };
            ImmSetCompositionWindow(context, ref composition);
            // The candidates go below the line, never over it.
            var candidate = new CandidateForm { Style = CfsExclude, X = x, Y = bottom, Left = x, Top = top, Right = x + 1, Bottom = bottom };
            ImmSetCandidateWindow(context, ref candidate);
        }
        finally
        {
            ImmReleaseContext(control.Handle, context);
        }
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct CompositionForm
    {
        public int Style;
        public int X, Y;
        public int Left, Top, Right, Bottom;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct CandidateForm
    {
        public int Index;
        public int Style;
        public int X, Y;
        public int Left, Top, Right, Bottom;
    }

    [LibraryImport("imm32.dll")]
    private static partial nint ImmGetContext(nint hwnd);

    [LibraryImport("imm32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static partial bool ImmReleaseContext(nint hwnd, nint context);

    [LibraryImport("imm32.dll")]
    private static unsafe partial int ImmGetCompositionStringW(nint context, int index, char* buffer, int bytes);

    [LibraryImport("imm32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static partial bool ImmSetCompositionWindow(nint context, ref CompositionForm form);

    [LibraryImport("imm32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static partial bool ImmSetCandidateWindow(nint context, ref CandidateForm form);
}
