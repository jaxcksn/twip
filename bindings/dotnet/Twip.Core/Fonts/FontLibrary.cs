using System.Text.Json;
using System.Text.Json.Serialization;
using Twip.Engine;
using Twip.Native;
using Twip.Wire;

namespace Twip;

/// <summary>
/// The fonts sessions lay out and draw with. Share one per process: fonts added after sessions exist reach every
/// session that uses the library.
/// </summary>
public sealed class FontLibrary
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
    /// Sets the resolver: the engine asks it, once per family and style, for a font a document uses that the library
    /// doesn't have. The request carries the document's description of the family and the category it gives. Return
    /// the font (<see cref="FontSource.FromBytes"/>, <see cref="FontSource.FromUri"/>), another family or a category to
    /// draw it with (<see cref="FontSource.FromFamily"/>, <see cref="FontSource.FromCategory"/>), or null. The resolver
    /// runs on the thread pool; while it runs, the family is pending and draws with a stand-in. An exception, a failed
    /// download or bytes that aren't a font report the family missing with the error.
    /// </summary>
    /// <param name="resolver">Returns the answer, or null when there is none.</param>
    public void SetResolver(Func<FontRequest, CancellationToken, ValueTask<FontSource?>> resolver)
    {
        ArgumentNullException.ThrowIfNull(resolver);
        _resolver = resolver;
        AskEngine();
    }

    /// <summary>With a resolver, the engine requests missing faces instead of reporting them missing at once.</summary>
    private void AskEngine() => Send(Method.FontSetResolver, WireJson.ToUtf8(true));

    /// <summary>
    /// Sets the draw-time replacements, replacing earlier ones: each family is drawn with another, such as Times New
    /// Roman with Tinos, even when the replaced family is in the library. Saved documents keep the original name.
    /// </summary>
    /// <param name="rules">The replacements, keyed by the family they replace.</param>
    public void SetReplacements(IReadOnlyDictionary<string, string> rules)
    {
        ArgumentNullException.ThrowIfNull(rules);
        SetReplacements(rules.Select(r => new FontRule(r.Key, r.Value)));
    }

    /// <summary>
    /// Sets the draw-time replacements per style, replacing earlier ones. A replacement is the first step of
    /// resolution; when its target isn't in the library, the replaced family resolves as if there were no rule.
    /// </summary>
    /// <param name="rules">The rules.</param>
    public void SetReplacements(IEnumerable<FontRule> rules)
    {
        ArgumentNullException.ThrowIfNull(rules);
        Send(Method.FontSetReplacements, WireJson.ToUtf8<IReadOnlyList<FontRule>>([.. rules]));
    }

    /// <summary>
    /// Sets the aliases, replacing earlier ones: each family that isn't in the library is drawn with another, such as
    /// Arial with Arimo. Saved documents keep the original name. Families a resolver answered, or found missing, are
    /// requested again.
    /// </summary>
    /// <param name="aliases">The aliases, keyed by the family they stand in for.</param>
    public void SetAliases(IReadOnlyDictionary<string, string> aliases)
    {
        ArgumentNullException.ThrowIfNull(aliases);
        SetAliases(aliases.Select(a => new Alias(a.Key, a.Value)));
    }

    /// <summary>
    /// Sets the aliases per style, or to a generic category, replacing earlier ones. An alias applies only when its
    /// family isn't in the library; aliases chain up to 8 hops.
    /// </summary>
    /// <param name="aliases">The aliases.</param>
    public void SetAliases(IEnumerable<Alias> aliases)
    {
        ArgumentNullException.ThrowIfNull(aliases);
        Send(Method.FontSetAliases, WireJson.ToUtf8<IReadOnlyList<Alias>>([.. aliases]));
    }

    /// <summary>
    /// Sets the default family of each generic category, for a missing family of that category (from the document's
    /// font table, an alias or a resolver answer). Opt-in: <c>new GenericDefaults()</c> turns it off.
    /// </summary>
    /// <param name="defaults">The defaults.</param>
    public void SetGenericDefaults(GenericDefaults defaults)
    {
        ArgumentNullException.ThrowIfNull(defaults);
        Send(Method.FontSetGenericDefaults, WireJson.ToUtf8(defaults));
    }

    /// <summary>
    /// Sets whether a document's font table gives a missing family its generic category. On by default; it has no
    /// effect until a generic default is set. Off, only aliases to a category and resolver answers give one.
    /// </summary>
    /// <param name="fromDocument">Whether the document's font table classifies.</param>
    public void SetClassification(bool fromDocument) => Send(Method.FontSetClassification, WireJson.ToUtf8(fromDocument));

    /// <summary>
    /// Sets the face for characters no other font has, after the character-fallback families: the bundled symbol font
    /// (the default), none, or a font's first face. It is never a family a document can name.
    /// </summary>
    /// <param name="fallback">The fallback.</param>
    /// <param name="ct">Cancels before setting it.</param>
    /// <returns>Completes once it is set.</returns>
    /// <exception cref="EditorException">The font holds no face (<see cref="EditorErrorCode.InvalidArgument"/>); the setting is unchanged.</exception>
    public async Task SetSymbolFallbackAsync(SymbolFallback fallback, CancellationToken ct = default)
    {
        ArgumentNullException.ThrowIfNull(fallback);
        var data = fallback.Path is { } path ? await File.ReadAllBytesAsync(path, ct).ConfigureAwait(false) : fallback.Data;
        await EngineThread.Instance.Post(() =>
        {
            if (data is { } bytes) SetSymbolFont(bytes.Span);
            else Native.Call(0, Method.FontSetSymbolFallback, WireJson.ToUtf8(fallback.Name));
            return true;
        }, ct).ConfigureAwait(false);
    }

    private unsafe void SetSymbolFont(ReadOnlySpan<byte> data)
    {
        TwipBuf output = default;
        int status;
        fixed (byte* p = data) status = TwipNative.FontSymbolFallback(Native.Handle, p, (nuint)data.Length, &output);
        try { WireJson.Unwrap(output.Span, status); }
        finally { EngineThread.Free(ref output); }
    }

    /// <summary>Sets the families searched, in order, for a character a run's font lacks, before the symbol fallback.</summary>
    /// <param name="families">The families.</param>
    public void SetCharacterFallback(IEnumerable<string> families)
    {
        ArgumentNullException.ThrowIfNull(families);
        Send(Method.FontSetCharacterFallback, WireJson.ToUtf8<IReadOnlyList<string>>([.. families]));
    }

    /// <summary>
    /// Sets the general fallback: the family that draws a font nothing else resolves (after replacements, aliases,
    /// resolver answers and generic defaults). With null, or when it isn't in the library, the first face added draws.
    /// </summary>
    /// <param name="family">The family, or null for none.</param>
    public void SetFallback(string? family) => Send(Method.FontSetFallback, WireJson.ToUtf8(family));

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

    /// <summary>On the engine thread: the engine needs a font. Runs the resolver on the thread pool, then answers.</summary>
    internal void OnFontRequest(byte[] payload)
    {
        var request = JsonSerializer.Deserialize(payload, TwipJsonContext.Default.FontRequest)!;
        if (_resolver is not { } resolver)
        {
            Send(Method.FontAnswer, Answer(request, FontAnswerWire.Missing));
            return;
        }
        _ = Task.Run(async () =>
        {
            FontAnswerWire answer;
            byte[]? data = null;
            try
            {
                var source = await resolver(request, CancellationToken.None).ConfigureAwait(false);
                answer = source switch
                {
                    null => FontAnswerWire.Missing,
                    { Family: { } family } => new FontAnswerWire("family", Family: family),
                    { Category: { } category } => new FontAnswerWire("category", Category: category),
                    _ => new FontAnswerWire("font"),
                };
                data = source switch
                {
                    { Uri: { IsFile: true } file } => await File.ReadAllBytesAsync(file.LocalPath).ConfigureAwait(false),
                    { Uri: { } uri } => await Http.Value.GetByteArrayAsync(uri).ConfigureAwait(false),
                    { Family: null, Category: null } bytes => bytes.Data.ToArray(),
                    _ => null,
                };
            }
            catch (OperationCanceledException)
            {
                answer = FontAnswerWire.Missing;
            }
            catch (Exception e)
            {
                answer = new FontAnswerWire("failed", Message: e.Message);
            }
            EngineThread.Instance.Post(() =>
            {
                if (data is not null)
                {
                    try
                    {
                        Add(data);
                    }
                    catch (EditorException e)
                    {
                        answer = new FontAnswerWire("failed", Message: e.Message);
                    }
                }
                Native.Call(0, Method.FontAnswer, Answer(request, answer));
            });
        });
    }

    private static byte[] Answer(FontRequest request, FontAnswerWire answer) => WireJson.ToUtf8(new FontAnswerPayload(request, answer));
}

/// <summary>A font the engine needs, asked once per family and style.</summary>
/// <param name="Family">The family, as the document names it.</param>
/// <param name="Bold">Bold.</param>
/// <param name="Italic">Italic.</param>
/// <param name="Description">The document's description of the family, when its font table has one.</param>
/// <param name="Proposed">The generic category that description gives, whether or not classification is on.</param>
public sealed record FontRequest(
    string Family, bool Bold, bool Italic,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] FontDescription? Description = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] GenericFamily? Proposed = null);

/// <summary>A font table's general class of a family.</summary>
public enum FontClass
{
    /// <summary>Serif (<c>roman</c>).</summary>
    Roman,
    /// <summary>Sans serif (<c>swiss</c>).</summary>
    Swiss,
    /// <summary>Fixed pitch (<c>modern</c>).</summary>
    Modern,
    /// <summary>Script (<c>script</c>).</summary>
    Script,
    /// <summary>Decorative (<c>decorative</c>).</summary>
    Decorative,
    /// <summary>Not stated (<c>auto</c>).</summary>
    Auto,
}

/// <summary>A font table's pitch of a family.</summary>
public enum FontPitch
{
    /// <summary>Every character has the same width.</summary>
    Fixed,
    /// <summary>Widths vary.</summary>
    Variable,
    /// <summary>Not stated.</summary>
    Default,
}

/// <summary>A document's description of a font family, from its font table. Each member is null when the document doesn't state it.</summary>
/// <param name="Class">The general class (<c>w:family</c>).</param>
/// <param name="Pitch">The pitch (<c>w:pitch</c>).</param>
/// <param name="Panose">The PANOSE classification (<c>w:panose1</c>), 20 hex digits.</param>
/// <param name="Charset">The character set (<c>w:charset</c>).</param>
public sealed record FontDescription(
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] FontClass? Class = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] FontPitch? Pitch = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? Panose = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] int? Charset = null);

/// <summary>A font face.</summary>
/// <param name="Family">The family.</param>
/// <param name="Bold">Bold.</param>
/// <param name="Italic">Italic.</param>
public sealed record FaceInfo(string Family, bool Bold, bool Italic);

/// <summary>A draw-time replacement: <paramref name="From"/> is drawn with <paramref name="To"/>, even when it is in the library.</summary>
/// <param name="From">The family to replace, case-insensitive.</param>
/// <param name="To">The family to draw with.</param>
/// <param name="Bold">Only for bold (true) or regular (false) faces.</param>
/// <param name="Italic">Only for italic (true) or upright (false) faces.</param>
public sealed record FontRule(
    string From, string To,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? Bold = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? Italic = null);

/// <summary>A resolver's answer: the font's bytes or a URI to fetch, or a family or category to draw the requested family with.</summary>
public sealed class FontSource
{
    private FontSource(ReadOnlyMemory<byte> data, Uri? uri, string? family = null, GenericFamily? category = null) =>
        (Data, Uri, Family, Category) = (data, uri, family, category);

    /// <summary>The family to draw with, when the answer is one.</summary>
    public string? Family { get; }

    /// <summary>The category whose default draws, when the answer is one.</summary>
    public GenericFamily? Category { get; }

    /// <summary>Draw the requested family with another, for every style, until aliases or replacements change.</summary>
    /// <param name="family">The family.</param>
    /// <returns>The answer.</returns>
    public static FontSource FromFamily(string family)
    {
        ArgumentNullException.ThrowIfNull(family);
        return new(default, null, family);
    }

    /// <summary>Draw the requested family with the default of a generic category, for every style.</summary>
    /// <param name="category">The category.</param>
    /// <returns>The answer.</returns>
    public static FontSource FromCategory(GenericFamily category) => new(default, null, null, category);

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
