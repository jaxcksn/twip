using System.Runtime.InteropServices;
using Twip.Hosting;

namespace Twip.WinForms;

/// <summary>
/// IME composition through IMM32: the composition string goes to the engine, which draws it inline at the caret, and
/// the IME's candidate window is kept beside the caret, its text running the way the caret's line does.
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
    private LineDirection _line;
    // The line the composition font was last set for.
    private LineDirection? _fontLine = LineDirection.Horizontal;
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
    public void Move(CaretInfo caret)
    {
        (_caret, _line) = (caret.Bounds, caret.Line);
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
            var (left, top, right, bottom) = ((int)_caret.X, (int)_caret.Y, (int)Math.Ceiling(_caret.Right), (int)Math.Ceiling(_caret.Bottom));
            if (_fontLine != _line) SetFont(context);
            // The composition starts where the line's text does: a vertical font's origin is its first column's
            // top-right, a font turned up its bottom-left.
            var (x, y) = _line switch
            {
                LineDirection.Down => (right, top),
                LineDirection.Up => (left, bottom),
                _ => (left, top),
            };
            var composition = new CompositionForm { Style = CfsPoint, X = x, Y = y };
            ImmSetCompositionWindow(context, ref composition);
            // The candidates go beside the line on the side the next line goes, never over it: below a horizontal
            // line, left of one running down, right of one running up.
            var (cx, cy) = _line switch
            {
                LineDirection.Down => (left, top),
                LineDirection.Up => (right, top),
                _ => (left, bottom),
            };
            var candidate = new CandidateForm { Style = CfsExclude, X = cx, Y = cy, Left = left, Top = top, Right = Math.Max(right, left + 1), Bottom = Math.Max(bottom, top + 1) };
            ImmSetCandidateWindow(context, ref candidate);
        }
        finally
        {
            ImmReleaseContext(control.Handle, context);
        }
    }

    /// <summary>The composition font for the caret's line: the control's font, as its vertical (<c>@</c>) face turned a
    /// quarter clockwise down a line, turned a quarter counter-clockwise up one.</summary>
    private void SetFont(nint context)
    {
        var font = new LogFont();
        control.Font.ToLogFont(font);
        var name = control.Font.Name;
        (font.Escapement, font.Orientation, font.FaceName) = _line switch
        {
            LineDirection.Down => (2700, 2700, "@" + name),
            LineDirection.Up => (900, 900, name),
            _ => (0, 0, name),
        };
        if (ImmSetCompositionFontW(context, font)) _fontLine = _line;
    }

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    private sealed class LogFont
    {
        public int Height;
        public int Width;
        public int Escapement;
        public int Orientation;
        public int Weight;
        public byte Italic;
        public byte Underline;
        public byte StrikeOut;
        public byte CharacterSet;
        public byte OutPrecision;
        public byte ClipPrecision;
        public byte Quality;
        public byte PitchAndFamily;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 32)]
        public string FaceName = "";
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

    [DllImport("imm32.dll")]
    private static extern nint ImmGetContext(nint hwnd);

    [DllImport("imm32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool ImmReleaseContext(nint hwnd, nint context);

    [DllImport("imm32.dll")]
    private static unsafe extern int ImmGetCompositionStringW(nint context, int index, char* buffer, int bytes);

    [DllImport("imm32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool ImmSetCompositionWindow(nint context, ref CompositionForm form);

    [DllImport("imm32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool ImmSetCandidateWindow(nint context, ref CandidateForm form);

    [DllImport("imm32.dll", CharSet = CharSet.Unicode)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool ImmSetCompositionFontW(nint context, [In] LogFont font);
}
