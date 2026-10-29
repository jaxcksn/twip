using System.Text.Json;
using System.Text.Json.Serialization;
using System.Text.RegularExpressions;
using Twip.Engine;
using Twip.Native;
using Twip.Wire;

namespace Twip;

/// <summary>
/// The fonts sessions lay out and draw with. Share one per process: fonts added after sessions exist reach every
/// session that uses the library.
/// </summary>
public sealed partial class FontLibrary
{
    private static readonly Lazy<HttpClient> Http = new(() => new HttpClient());
    private volatile Func<FontRequest, CancellationToken, ValueTask<FontSource?>>? _resolver;

    private FontLibrary(bool system) => Native = new NativeEngine(this, system ? "system" : "empty");

    /// <summary>The engine that holds this library's fonts, and the sessions that use it.</summary>
    internal NativeEngine Native { get; }

    /// <summary>The system fonts, shared per process.</summary>
    public static FontLibrary System { get; } = new(true);

    /// <summary>A library with no fonts, for applications that supply every font.</summary>
    /// <returns>The library.</returns>
    public static FontLibrary CreateEmpty() => new(false);

    /// <summary>Adds fonts from bytes: TTF, OTF, TTC, WOFF or WOFF2.</summary>
    /// <param name="data">The font file.</param>
    /// <param name="ct">Cancels before adding.</param>
    /// <returns>The faces added.</returns>
    /// <exception cref="EditorException">The bytes hold no font (<see cref="EditorErrorCode.InvalidArgument"/>).</exception>
    public Task<IReadOnlyList<FaceInfo>> AddAsync(ReadOnlyMemory<byte> data, CancellationToken ct = default) =>
        EngineThread.Instance.Post(() => Add(data.Span), ct);

    /// <summary>Adds fonts from a file: TTF, OTF, TTC, WOFF or WOFF2.</summary>
    /// <param name="path">The font file.</param>
    /// <param name="ct">Cancels the read.</param>
    /// <returns>The faces added.</returns>
    /// <exception cref="EditorException">The file holds no font (<see cref="EditorErrorCode.InvalidArgument"/>).</exception>
    public async Task<IReadOnlyList<FaceInfo>> AddAsync(string path, CancellationToken ct = default) =>
        await AddAsync(await File.ReadAllBytesAsync(path, ct).ConfigureAwait(false), ct).ConfigureAwait(false);

    /// <summary>
    /// Resolves fonts a document uses that the library doesn't have. The engine asks once per family and style; the
    /// resolver runs on the thread pool, and a null answer (or an exception) reports the family unresolved.
    /// </summary>
    /// <param name="resolver">Returns the font, or null when it can't be found.</param>
    public void SetResolver(Func<FontRequest, CancellationToken, ValueTask<FontSource?>> resolver)
    {
        ArgumentNullException.ThrowIfNull(resolver);
        _resolver = resolver;
        AskEngine();
    }

    /// <summary>Resolves missing fonts from Google Fonts, after the resolver set before it.</summary>
    /// <param name="options">Which families to allow.</param>
    public void UseGoogleFonts(GoogleFontsOptions? options = null)
    {
        var before = _resolver;
        var allowed = options?.Families?.ToHashSet(StringComparer.OrdinalIgnoreCase);
        _resolver = async (request, ct) =>
        {
            if (before is not null && await before(request, ct).ConfigureAwait(false) is { } found) return found;
            if (allowed is not null && !allowed.Contains(request.Family)) return null;
            return await GoogleFontAsync(request, ct).ConfigureAwait(false);
        };
        AskEngine();
    }

    /// <summary>With a resolver, the engine requests missing faces instead of reporting them unresolved at once.</summary>
    private void AskEngine() => Send(Method.FontSetResolver, WireJson.ToUtf8(true));

    /// <summary>Draws one family with another, such as Times New Roman with Tinos. Saved documents keep the original name.</summary>
    /// <param name="rules">The substitutes, keyed by the family they replace.</param>
    public void Substitute(IReadOnlyDictionary<string, string> rules)
    {
        ArgumentNullException.ThrowIfNull(rules);
        Substitute(rules.Select(r => new SubstitutionRule(r.Key, r.Value)));
    }

    /// <summary>Draws one family with another, per style. The rules replace the ones set before.</summary>
    /// <param name="rules">The rules.</param>
    public void Substitute(IEnumerable<SubstitutionRule> rules)
    {
        ArgumentNullException.ThrowIfNull(rules);
        Send(Method.FontSetSubstitutions, WireJson.ToUtf8<IReadOnlyList<SubstitutionRule>>([.. rules]));
    }

    /// <summary>Sets the family used when nothing else resolves.</summary>
    /// <param name="family">The family.</param>
    public void SetDefault(string family)
    {
        ArgumentNullException.ThrowIfNull(family);
        Send(Method.FontSetDefault, WireJson.ToUtf8(family));
    }

    /// <summary>Every family the library can draw now, for a font box.</summary>
    /// <param name="ct">Cancels the request.</param>
    /// <returns>The family names.</returns>
    public Task<IReadOnlyList<string>> GetFamiliesAsync(CancellationToken ct = default) =>
        EngineThread.Instance.Post(() => Native.Call(0, Method.FontFamilies, [])?.Deserialize(TwipJsonContext.Default.IReadOnlyListString) ?? [], ct);

    /// <summary>A setting: it runs on the engine thread before any operation queued after it.</summary>
    private void Send(Method method, byte[] request) => EngineThread.Instance.Post(() => { Native.Call(0, method, request); });

    private unsafe IReadOnlyList<FaceInfo> Add(ReadOnlySpan<byte> data)
    {
        TwipBuf output = default;
        int status;
        fixed (byte* p = data) status = TwipNative.FontAdd(Native.Handle, p, (nuint)data.Length, &output);
        try { return WireJson.Unwrap(output.Span, status)?.Deserialize(TwipJsonContext.Default.IReadOnlyListFaceInfo) ?? []; }
        finally { EngineThread.Free(ref output); }
    }

    /// <summary>On the engine thread: the engine needs a font. Runs the resolver, then adds what it found or reports the family unresolved.</summary>
    internal void OnFontRequest(byte[] payload)
    {
        var request = JsonSerializer.Deserialize(payload, TwipJsonContext.Default.FontRequest)!;
        if (_resolver is not { } resolver)
        {
            Send(Method.FontMarkUnresolved, WireJson.ToUtf8(request));
            return;
        }
        _ = Task.Run(async () =>
        {
            byte[]? data = null;
            try
            {
                data = await resolver(request, CancellationToken.None).ConfigureAwait(false) switch
                {
                    null => null,
                    { Uri: { IsFile: true } file } => await File.ReadAllBytesAsync(file.LocalPath).ConfigureAwait(false),
                    { Uri: { } uri } => await Http.Value.GetByteArrayAsync(uri).ConfigureAwait(false),
                    var source => source.Data.ToArray(),
                };
            }
            catch
            {
                // A failing resolver or download leaves the family unresolved, as a null answer does.
            }
            EngineThread.Instance.Post(() =>
            {
                try
                {
                    if (data is not null && Add(data).Count > 0) return;
                }
                catch (EditorException)
                {
                    // Not a font: unresolved.
                }
                Native.Call(0, Method.FontMarkUnresolved, WireJson.ToUtf8(request));
            });
        });
    }

    /// <summary>One face from the Google Fonts CSS API, as TTF (which it serves to clients that send no user agent).</summary>
    private static async ValueTask<FontSource?> GoogleFontAsync(FontRequest request, CancellationToken ct)
    {
        var family = Uri.EscapeDataString(request.Family).Replace("%20", "+", StringComparison.Ordinal);
        var css = $"https://fonts.googleapis.com/css2?family={family}:ital,wght@{(request.Italic ? 1 : 0)},{(request.Bold ? 700 : 400)}";
        using var response = await Http.Value.GetAsync(css, ct).ConfigureAwait(false);
        if (!response.IsSuccessStatusCode) return null; // 400: Google doesn't have this family or face
        var url = GoogleSource().Match(await response.Content.ReadAsStringAsync(ct).ConfigureAwait(false));
        return url.Success ? FontSource.FromUri(new Uri(url.Groups[1].Value)) : null;
    }

    [GeneratedRegex(@"src:\s*url\(([^)]+)\)")]
    private static partial Regex GoogleSource();
}

/// <summary>A font the engine needs.</summary>
/// <param name="Family">The family.</param>
/// <param name="Bold">Bold.</param>
/// <param name="Italic">Italic.</param>
public sealed record FontRequest(string Family, bool Bold, bool Italic);

/// <summary>A font face.</summary>
/// <param name="Family">The family.</param>
/// <param name="Bold">Bold.</param>
/// <param name="Italic">Italic.</param>
public sealed record FaceInfo(string Family, bool Bold, bool Italic);

/// <summary>A substitution rule.</summary>
/// <param name="From">The family to replace.</param>
/// <param name="To">The family to draw with.</param>
/// <param name="Bold">Only for bold (true) or regular (false) faces.</param>
/// <param name="Italic">Only for italic (true) or upright (false) faces.</param>
public sealed record SubstitutionRule(
    string From, string To,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? Bold = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? Italic = null);

/// <summary>Where a resolved font comes from: its bytes, or a URI to fetch.</summary>
public sealed class FontSource
{
    private FontSource(ReadOnlyMemory<byte> data, Uri? uri) => (Data, Uri) = (data, uri);

    /// <summary>The font bytes, when given as bytes.</summary>
    public ReadOnlyMemory<byte> Data { get; }

    /// <summary>The URI, when given as one.</summary>
    public Uri? Uri { get; }

    /// <summary>A font from bytes.</summary>
    /// <param name="data">The font file.</param>
    public static FontSource FromBytes(ReadOnlyMemory<byte> data) => new(data, null);

    /// <summary>A font to fetch.</summary>
    /// <param name="uri">The URI.</param>
    public static FontSource FromUri(Uri uri) => new(default, uri);
}

/// <summary>Options for Google Fonts.</summary>
/// <param name="Families">Only these families; by default any.</param>
public sealed record GoogleFontsOptions(IReadOnlyList<string>? Families = null);
