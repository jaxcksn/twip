using System.Buffers;
using System.Runtime.InteropServices;
using System.Text.Json;
using Twip.Engine;
using Twip.Native;
using Twip.Wire;

namespace Twip;

/// <summary>What a <see cref="PdfFontSet"/> is built from.</summary>
public sealed record PdfFontSetOptions
{
    /// <summary>Whether the set includes the fonts installed on this system.</summary>
    public bool IncludeSystemFonts { get; init; }

    /// <summary>Font files' bytes: TrueType, OpenType, collections, WOFF or WOFF2.</summary>
    public IReadOnlyList<ReadOnlyMemory<byte>> Fonts { get; init; } = [];

    /// <summary>Font files by path, read when the set is created.</summary>
    public IReadOnlyList<string> FontFiles { get; init; } = [];

    /// <summary>Rules that draw one family in another.</summary>
    public IReadOnlyList<SubstitutionRule> Substitutions { get; init; } = [];

    /// <summary>The family drawn when nothing else resolves a name.</summary>
    public string? DefaultFamily { get; init; }
}

/// <summary>
/// The faces <see cref="PdfConverter"/> draws with, and how document font names map to them. Fixed once created and
/// safe to share between threads: create one and use it for every conversion. Unlike a <see cref="FontLibrary"/>,
/// which belongs to editor sessions and changes over time, a font set never changes.
/// </summary>
public sealed class PdfFontSet : IDisposable
{
    private static readonly Lazy<PdfFontSet> SystemSet = new(() => Create(new PdfFontSetOptions { IncludeSystemFonts = true }, owned: false));

    private readonly FontsHandle _handle;
    private readonly bool _owned;

    private PdfFontSet(FontsHandle handle, IReadOnlyList<FaceInfo> faces, bool owned)
    {
        _handle = handle;
        Faces = faces;
        _owned = owned;
    }

    /// <summary>The fonts installed on this system, created on first use. Disposing it does nothing.</summary>
    public static PdfFontSet System => SystemSet.Value;

    /// <summary>The faces the options' fonts added, in the order given.</summary>
    public IReadOnlyList<FaceInfo> Faces { get; }

    /// <summary>The families in the set, sorted.</summary>
    /// <exception cref="ObjectDisposedException">The set was disposed.</exception>
    public unsafe IReadOnlyList<string> Families
    {
        get
        {
            using var use = Use();
            TwipBuf output = default;
            var status = TwipNative.PdfFontsFamilies(use.Pointer, &output);
            try
            {
                return WireJson.Unwrap(output.Span, status)?.Deserialize(TwipJsonContext.Default.IReadOnlyListString) ?? [];
            }
            finally
            {
                EngineThread.Free(ref output);
            }
        }
    }

    /// <summary>Creates a font set.</summary>
    /// <param name="options">The fonts, substitutions and default family.</param>
    /// <returns>The set. Dispose it once no conversion needs it.</returns>
    /// <exception cref="EditorException">A font doesn't read (<see cref="EditorErrorCode.InvalidArgument"/>).</exception>
    public static PdfFontSet Create(PdfFontSetOptions options) => Create(options, owned: true);

    private static unsafe PdfFontSet Create(PdfFontSetOptions options, bool owned)
    {
        ArgumentNullException.ThrowIfNull(options);
        var json = WireJson.ToUtf8(new PdfFontSetWire(options.IncludeSystemFonts, options.FontFiles, options.Substitutions, options.DefaultFamily));
        var pins = new MemoryHandle[options.Fonts.Count];
        var pointers = new nint[pins.Length];
        var lengths = new nuint[pins.Length];
        try
        {
            for (var i = 0; i < pins.Length; i++)
            {
                pins[i] = options.Fonts[i].Pin();
                pointers[i] = (nint)pins[i].Pointer;
                lengths[i] = (nuint)options.Fonts[i].Length;
            }
            nint set = 0;
            TwipBuf faces = default;
            int status;
            fixed (byte* o = json)
            fixed (nint* p = pointers)
            fixed (nuint* l = lengths)
            {
                status = TwipNative.PdfFontsNew(o, (nuint)json.Length, (byte**)p, l, (nuint)pins.Length, &set, &faces);
            }
            try
            {
                var added = WireJson.Unwrap(faces.Span, status)?.Deserialize(TwipJsonContext.Default.IReadOnlyListFaceInfo) ?? [];
                return new PdfFontSet(new FontsHandle(set), added, owned);
            }
            catch
            {
                if (set != 0) TwipNative.PdfFontsFree(set);
                throw;
            }
            finally
            {
                EngineThread.Free(ref faces);
            }
        }
        finally
        {
            foreach (var pin in pins) pin.Dispose();
        }
    }

    /// <summary>Releases the set. Conversions already running with it finish first: each holds its own reference.</summary>
    public void Dispose()
    {
        if (_owned) _handle.Dispose();
    }

    /// <summary>A reference to the native set for one call; throws once the set is disposed.</summary>
    internal Lease Use() => new(_handle);

    internal readonly struct Lease : IDisposable
    {
        private readonly FontsHandle _handle;

        public Lease(FontsHandle handle)
        {
            var added = false;
            // Throws ObjectDisposedException once the handle is closed.
            handle.DangerousAddRef(ref added);
            _handle = handle;
            Pointer = handle.DangerousGetHandle();
        }

        public nint Pointer { get; }

        public void Dispose() => _handle.DangerousRelease();
    }

    internal sealed class FontsHandle : SafeHandle
    {
        public FontsHandle(nint set) : base(IntPtr.Zero, ownsHandle: true) => SetHandle(set);

        public override bool IsInvalid => handle == IntPtr.Zero;

        protected override bool ReleaseHandle()
        {
            TwipNative.PdfFontsFree(handle);
            return true;
        }
    }
}
