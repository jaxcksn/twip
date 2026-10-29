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

    [LibraryImport(Library, EntryPoint = "twip_engine_new")]
    public static partial nint EngineNew(byte* optionsJson, nuint len, delegate* unmanaged[Cdecl]<nint, uint, uint, byte*, nuint, void> callback, nint ctx);

    [LibraryImport(Library, EntryPoint = "twip_engine_free")]
    public static partial void EngineFree(nint engine);

    [LibraryImport(Library, EntryPoint = "twip_session_new")]
    public static partial uint SessionNew(nint engine, byte* optionsJson, nuint len);

    [LibraryImport(Library, EntryPoint = "twip_session_free")]
    public static partial void SessionFree(nint engine, uint session);

    [LibraryImport(Library, EntryPoint = "twip_call")]
    public static partial int Call(nint engine, uint session, uint method, byte* req, nuint len, TwipBuf* output);

    [LibraryImport(Library, EntryPoint = "twip_buf_free")]
    public static partial void BufFree(TwipBuf* buf);

    [LibraryImport(Library, EntryPoint = "twip_open")]
    public static partial int Open(nint engine, uint session, byte* bytes, nuint len, byte* opts, nuint olen, TwipBuf* output);

    [LibraryImport(Library, EntryPoint = "twip_open_prepare")]
    public static partial int OpenPrepare(nint engine, uint session, byte* bytes, nuint len, byte* opts, nuint olen, TwipBuf* output);

    [LibraryImport(Library, EntryPoint = "twip_export")]
    public static partial int Export(nint engine, uint session, byte* opts, nuint olen, TwipBuf* bytes, TwipBuf* meta);

    [LibraryImport(Library, EntryPoint = "twip_font_add")]
    public static partial int FontAdd(nint engine, byte* bytes, nuint len, TwipBuf* output);

    [LibraryImport(Library, EntryPoint = "twip_key")]
    public static partial uint Key(nint engine, uint session, uint key, uint mods);

    [LibraryImport(Library, EntryPoint = "twip_text")]
    public static partial void Text(nint engine, uint session, byte* utf8, nuint len);

    [LibraryImport(Library, EntryPoint = "twip_composition")]
    public static partial void Composition(nint engine, uint session, uint phase, byte* utf8, nuint len);

    [LibraryImport(Library, EntryPoint = "twip_pointer")]
    public static partial void Pointer(nint engine, uint session, uint kind, double x, double y, uint button, uint clicks, uint mods);

    [LibraryImport(Library, EntryPoint = "twip_wheel")]
    public static partial void Wheel(nint engine, uint session, double dx, double dy, uint mods);

    [LibraryImport(Library, EntryPoint = "twip_focus")]
    public static partial void Focus(nint engine, uint session, uint focused);

    [LibraryImport(Library, EntryPoint = "twip_surface")]
    public static partial void Surface(nint engine, uint session, double width, double height, double dpr);

    [LibraryImport(Library, EntryPoint = "twip_scroll")]
    public static partial void Scroll(nint engine, uint session, double x, double y);

    [LibraryImport(Library, EntryPoint = "twip_frame_rgba")]
    public static partial int FrameRgba(nint engine, uint session, byte* pixels, uint width, uint height, TwipFrameInfo* info);

    [LibraryImport(Library, EntryPoint = "twip_timer_fired")]
    public static partial void TimerFired(nint engine, uint session, uint timer);

    /// <summary>Whether the native library loads and has this C ABI.</summary>
    public static bool IsAvailable()
    {
        if (!NativeLibrary.TryLoad(Library, typeof(TwipNative).Assembly, null, out var handle)) return false;
        return NativeLibrary.TryGetExport(handle, "twip_engine_new", out _) && NativeLibrary.TryGetExport(handle, "twip_call", out _);
    }
}
