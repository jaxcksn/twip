using System.Runtime.InteropServices;

namespace Twip.Native;

/// <summary>A byte buffer the engine allocated; free it with <see cref="TwipNative.BufFree"/>.</summary>
[StructLayout(LayoutKind.Sequential)]
internal struct TwipBuf
{
    public nint Ptr;
    public nuint Len;

    public readonly unsafe ReadOnlySpan<byte> Span => Ptr == 0 ? default : new((void*)Ptr, checked((int)Len));
}

/// <summary>What a frame call reports: the page stack's size and the caret, CSS px.</summary>
[StructLayout(LayoutKind.Sequential)]
internal struct TwipFrameInfo
{
    public double ContentWidth;
    public double ContentHeight;
    public double CaretX;
    public double CaretY;
    public double CaretHeight;
    public uint CaretVisible;
    public uint Pages;
}

/// <summary>Callback kinds, engine to SDK.</summary>
internal enum CallbackKind : uint
{
    Event = 1,
    State = 2,
    RequestFrame = 3,
    SetTimer = 4,
    FontRequest = 5,
    HostRequest = 6,
}

/// <summary>Method numbers for <see cref="TwipNative.Call"/>: one per engine session method.</summary>
/// <remarks>These must equal <c>TwipMethod</c> in <c>twip.h</c> one for one.</remarks>
internal enum Method : uint
{
    Run = 1,
    Batch = 2,
    State = 3,
    Catalog = 4,
    NewDocument = 5,
    SetReadOnly = 6,
    SetMode = 7,
    SetAuthor = 8,
    ContextMenu = 9,
    Invoke = 10,
    DialogOpen = 11,
    DialogUpdate = 12,
    DialogApply = 13,
    Find = 14,
    FindAll = 15,
    SelectMatch = 16,
    Replace = 17,
    ReplaceAll = 18,
    SpellingAddDictionary = 19,
    SpellingSetEnabled = 20,
    SpellingSetLanguage = 21,
    SpellingIgnore = 22,
    SpellingAdd = 23,
    SpellingLoadUserWords = 24,
    SpellingSuggestions = 25,
    SpellingNext = 26,
    Headings = 27,
    SelectHeading = 28,
    Styles = 29,
    Revisions = 30,
    Comments = 31,
    People = 32,
    Elements = 33,
    Element = 34,
    Structure = 35,
    SelectElement = 36,
    DataParts = 37,
    AccessibleTree = 38,
    ShortcutsRegister = 39,
    ShortcutsUnregister = 40,
    ShortcutsLookup = 41,
    GesturesBindings = 42,
    GesturesSet = 43,
    GesturesReset = 44,
    WordCount = 45,
    Copy = 46,
    Paste = 47,
    FontFamilies = 48,
    FontSetSubstitutions = 49,
    FontSetDefault = 50,
    FontMarkUnresolved = 51,
    HostSetBackground = 52,
    HostSetMarkupPaneWidth = 53,
    HostSetRulers = 54,
    HostSetRulerUnit = 55,
    HostSetTabSelector = 56,
    HostRulerView = 57,
    HostCommentAnchors = 58,
    HostHitTest = 59,
    HostDragOver = 60,
    HostDragLeave = 61,
    HostDrop = 62,
    HostRevealSelection = 63,
    HostVisiblePages = 64,
    StackBalloons = 65,
    OpenCommit = 66,
    OpenDiscard = 67,
    DialogSelectIssue = 68,
    FontSetResolver = 69,
    Placeholders = 70,
    HostSetCaretBlink = 71,
    HostFrameStats = 72,
    HostSetOverlayPalette = 73,
    HostSetPaintBudget = 74,
    HostSetLeaders = 75,
}

/// <summary>Status codes the C functions return.</summary>
internal static class Status
{
    public const int Ok = 0;
}

/// <summary>The engine's C ABI (<c>twip.h</c>). Every call is made on the engine thread.</summary>
internal static unsafe partial class TwipNative
{
    public const string Library = "twip";

    [DllImport(Library, EntryPoint = "twip_engine_new")]
    public static extern nint EngineNew(byte* optionsJson, nuint len, nint callback, nint ctx);

    [DllImport(Library, EntryPoint = "twip_engine_free")]
    public static extern void EngineFree(nint engine);

    [DllImport(Library, EntryPoint = "twip_session_new")]
    public static extern uint SessionNew(nint engine, byte* optionsJson, nuint len);

    [DllImport(Library, EntryPoint = "twip_session_free")]
    public static extern void SessionFree(nint engine, uint session);

    [DllImport(Library, EntryPoint = "twip_call")]
    public static extern int Call(nint engine, uint session, uint method, byte* req, nuint len, TwipBuf* output);

    [DllImport(Library, EntryPoint = "twip_buf_free")]
    public static extern void BufFree(TwipBuf* buf);

    [DllImport(Library, EntryPoint = "twip_open")]
    public static extern int Open(nint engine, uint session, byte* bytes, nuint len, byte* opts, nuint olen, TwipBuf* output);

    [DllImport(Library, EntryPoint = "twip_open_prepare")]
    public static extern int OpenPrepare(nint engine, uint session, byte* bytes, nuint len, byte* opts, nuint olen, TwipBuf* output);

    [DllImport(Library, EntryPoint = "twip_export")]
    public static extern int Export(nint engine, uint session, byte* opts, nuint olen, TwipBuf* bytes, TwipBuf* meta);

    [DllImport(Library, EntryPoint = "twip_font_add")]
    public static extern int FontAdd(nint engine, byte* bytes, nuint len, TwipBuf* output);

    [DllImport(Library, EntryPoint = "twip_key")]
    public static extern uint Key(nint engine, uint session, uint key, uint mods);

    [DllImport(Library, EntryPoint = "twip_text")]
    public static extern void Text(nint engine, uint session, byte* utf8, nuint len);

    [DllImport(Library, EntryPoint = "twip_composition")]
    public static extern void Composition(nint engine, uint session, uint phase, byte* utf8, nuint len);

    [DllImport(Library, EntryPoint = "twip_pointer")]
    public static extern void Pointer(nint engine, uint session, uint kind, double x, double y, uint button, uint clicks, uint mods);

    [DllImport(Library, EntryPoint = "twip_wheel")]
    public static extern void Wheel(nint engine, uint session, double dx, double dy, uint mods);

    [DllImport(Library, EntryPoint = "twip_focus")]
    public static extern void Focus(nint engine, uint session, uint focused);

    [DllImport(Library, EntryPoint = "twip_surface")]
    public static extern void Surface(nint engine, uint session, double width, double height, double dpr);

    [DllImport(Library, EntryPoint = "twip_scroll")]
    public static extern void Scroll(nint engine, uint session, double x, double y);

    [DllImport(Library, EntryPoint = "twip_frame_rgba")]
    public static extern int FrameRgba(nint engine, uint session, byte* pixels, uint width, uint height, TwipFrameInfo* info);

    [DllImport(Library, EntryPoint = "twip_attach_window")]
    public static extern int AttachWindow(nint engine, uint session, nint hwnd);

    [DllImport(Library, EntryPoint = "twip_detach_window")]
    public static extern void DetachWindow(nint engine, uint session);

    [DllImport(Library, EntryPoint = "twip_frame_window")]
    public static extern int FrameWindow(nint engine, uint session, uint width, uint height, TwipFrameInfo* info);

    [DllImport(Library, EntryPoint = "twip_test_lose_gpu")]
    public static extern void TestLoseGpu(nint engine, uint session);

    [DllImport(Library, EntryPoint = "twip_timer_fired")]
    public static extern void TimerFired(nint engine, uint session, uint timer);

    /// <summary>Whether the native library loads and has this C ABI.</summary>
    public static bool IsAvailable()
    {
#if NETFRAMEWORK
        var handle = LoadLibrary(Library + ".dll");
        return handle != 0 && GetProcAddress(handle, "twip_engine_new") != 0 && GetProcAddress(handle, "twip_call") != 0;
#else
        if (!NativeLibrary.TryLoad(Library, typeof(TwipNative).Assembly, null, out var handle)) return false;
        return NativeLibrary.TryGetExport(handle, "twip_engine_new", out _) && NativeLibrary.TryGetExport(handle, "twip_call", out _);
#endif
    }
#if NETFRAMEWORK

    [DllImport("kernel32.dll", CharSet = CharSet.Unicode)]
    private static extern nint LoadLibrary(string name);

    [DllImport("kernel32.dll", CharSet = CharSet.Ansi, BestFitMapping = false)]
    private static extern nint GetProcAddress(nint module, string name);
#endif
}
