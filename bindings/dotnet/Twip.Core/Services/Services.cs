using System.Text.Json;
using System.Text.Json.Serialization;
using Twip.Native;
using Twip.Wire;

namespace Twip;

/// <summary>Find and replace, over the text of the current markup view.</summary>
public sealed class EditorFind
{
    private readonly EditorSession _session;

    internal EditorFind(EditorSession session) => _session = session;

    /// <summary>Finds and selects the next match, and reveals it.</summary>
    /// <param name="query">What to find.</param>
    /// <param name="ct">Cancels the search.</param>
    /// <returns>Whether a match was found, and whether the search wrapped.</returns>
    public Task<FindResult> NextAsync(FindQuery query, CancellationToken ct = default) =>
        _session.CallAsync<FindResult>(Method.Find, WireJson.ToUtf8(new FindRequest(query, NavigationDirection.Next)), ct);

    /// <summary>Finds and selects the previous match, and reveals it.</summary>
    /// <param name="query">What to find.</param>
    /// <param name="ct">Cancels the search.</param>
    /// <returns>Whether a match was found, and whether the search wrapped.</returns>
    public Task<FindResult> PreviousAsync(FindQuery query, CancellationToken ct = default) =>
        _session.CallAsync<FindResult>(Method.Find, WireJson.ToUtf8(new FindRequest(query, NavigationDirection.Previous)), ct);

    /// <summary>Every match, for a results list.</summary>
    /// <param name="query">What to find.</param>
    /// <param name="ct">Cancels the search.</param>
    /// <returns>The matches in document order, and whether the search stopped early.</returns>
    public Task<FindMatches> FindAllAsync(FindQuery query, CancellationToken ct = default) =>
        _session.CallAsync<FindMatches>(Method.FindAll, WireJson.ToUtf8(query), ct);

    /// <summary>Selects and reveals a match from <see cref="FindAllAsync"/>.</summary>
    /// <param name="match">The match.</param>
    /// <param name="ct">Cancels the request.</param>
    /// <returns>Completes when it is selected.</returns>
    public Task SelectAsync(MatchId match, CancellationToken ct = default) => _session.CallAsync(Method.SelectMatch, WireJson.ToUtf8(match), ct);

    /// <summary>
    /// Replaces the current match, then finds the next. The replacement takes Word's <c>^</c> codes (<c>^&amp;</c> the
    /// text found, <c>^c</c> the editor's last copy), or a regular expression's <c>$1</c> and <c>${name}</c> groups.
    /// </summary>
    /// <param name="query">What to find.</param>
    /// <param name="replacement">The replacement; empty with <paramref name="replaceFormat"/> formats the match only.</param>
    /// <param name="replaceFormat">The formatting the replacement gets.</param>
    /// <param name="ct">Cancels before replacing.</param>
    /// <returns>The result of finding the next match.</returns>
    public Task<FindResult> ReplaceAsync(FindQuery query, string replacement, FindFormat? replaceFormat = null, CancellationToken ct = default) =>
        _session.CallAsync<FindResult>(Method.Replace, WireJson.ToUtf8(new ReplaceRequest(query, replacement, replaceFormat)), ct);

    /// <summary>Replaces every match in one transaction, as <see cref="ReplaceAsync"/> does.</summary>
    /// <param name="query">What to find.</param>
    /// <param name="replacement">The replacement.</param>
    /// <param name="replaceFormat">The formatting the replacements get.</param>
    /// <param name="ct">Cancels before replacing.</param>
    /// <returns>How many were replaced, and whether the search stopped early.</returns>
    public Task<FindCount> ReplaceAllAsync(FindQuery query, string replacement, FindFormat? replaceFormat = null, CancellationToken ct = default) =>
        _session.CallAsync<FindCount>(Method.ReplaceAll, WireJson.ToUtf8(new ReplaceRequest(query, replacement, replaceFormat)), ct);

    /// <summary>Highlight All: every match highlighted on the pages until the document changes or <see cref="ClearHighlightsAsync"/>; none clears them.</summary>
    /// <param name="query">What to find.</param>
    /// <param name="ct">Cancels the search.</param>
    /// <returns>How many it highlighted.</returns>
    public Task<FindCount> HighlightAllAsync(FindQuery query, CancellationToken ct = default) =>
        _session.CallAsync<FindCount>(Method.FindHighlightAll, WireJson.ToUtf8(query), ct);

    /// <summary>Takes Highlight All's highlights away.</summary>
    /// <param name="ct">Cancels the request.</param>
    /// <returns>Completes when they are gone.</returns>
    public Task ClearHighlightsAsync(CancellationToken ct = default) => _session.CallAsync(Method.FindClearHighlights, [], ct);
}

/// <summary>What to find. A query whose text doesn't parse fails with an invalid argument error.</summary>
/// <param name="Text">
/// The text, with Word's <c>^</c> codes (<c>^p</c> a paragraph mark, <c>^t</c> a tab, <c>^?</c> any character,
/// <c>^#</c> any digit, <c>^^</c> a caret), or a regular expression with <see cref="Regex"/>. At most 255 characters.
/// </param>
/// <param name="MatchCase">Match case.</param>
/// <param name="WholeWords">Match whole words only.</param>
public sealed record FindQuery(string Text, bool MatchCase = false, bool WholeWords = false)
{
    /// <summary>The text is a regular expression, searched paragraph by paragraph.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public bool? Regex { get; init; }

    /// <summary>A match starts at a word's start.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public bool? MatchPrefix { get; init; }

    /// <summary>A match ends at a word's end.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public bool? MatchSuffix { get; init; }

    /// <summary>Punctuation is passed over.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public bool? IgnorePunctuation { get; init; }

    /// <summary>White space is passed over.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public bool? IgnoreWhiteSpace { get; init; }

    /// <summary>Finding wraps round the document's end (null: on); off, it stops there, not found.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public bool? Wrap { get; init; }

    /// <summary>Only within the selection, with no wrapping.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public bool? InSelection { get; init; }

    /// <summary>The formatting a match must have; with empty text, each stretch that has it matches.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public FindFormat? Format { get; init; }
}

/// <summary>Character formatting a search looks for, or a replacement applies; each one left null is any.</summary>
public sealed record FindFont
{
    /// <summary>The font name, matched ignoring case.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] public string? Font { get; init; }
    /// <summary>Exactly this combination of bold and italic.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] public FontStyleChoice? Style { get; init; }
    /// <summary>The size.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] public Length? Size { get; init; }
    /// <summary>The text colour.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] public ColorValue? Color { get; init; }
    /// <summary>The underline.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] public UnderlineStyle? Underline { get; init; }
    /// <summary>The underline's colour.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] public ColorValue? UnderlineColor { get; init; }
    /// <summary>Strikethrough.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] public bool? Strikethrough { get; init; }
    /// <summary>Double strikethrough.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] public bool? DoubleStrikethrough { get; init; }
    /// <summary>Superscript.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] public bool? Superscript { get; init; }
    /// <summary>Subscript.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] public bool? Subscript { get; init; }
    /// <summary>Small capitals.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] public bool? SmallCaps { get; init; }
    /// <summary>All capitals.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] public bool? AllCaps { get; init; }
    /// <summary>Hidden.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] public bool? Hidden { get; init; }
    /// <summary>Character spacing; negative condenses.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] public Length? CharacterSpacing { get; init; }
    /// <summary>The baseline offset; positive raises.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] public Length? Position { get; init; }
    /// <summary>Emboss.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] public bool? Emboss { get; init; }
    /// <summary>Engrave.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] public bool? Engrave { get; init; }
    /// <summary>Outline.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] public bool? Outline { get; init; }
    /// <summary>Shadow.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] public bool? Shadow { get; init; }
    /// <summary>The character width scale, percent.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] public double? Scale { get; init; }
    /// <summary>Kerning from this size up; 0 for none.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] public Length? Kerning { get; init; }
    /// <summary>The OpenType ligatures.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] public Ligatures? Ligatures { get; init; }
    /// <summary>The number spacing.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] public NumberSpacing? NumberSpacing { get; init; }
    /// <summary>The number forms.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] public NumberForm? NumberForm { get; init; }
    /// <summary>The stylistic set, 1 to 20; 0 for the default.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] public int? StylisticSet { get; init; }
    /// <summary>Contextual alternates.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] public bool? ContextualAlternates { get; init; }
    /// <summary>The text fill, or none of the text's own.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] public NoneOr<FillValue>? TextFill { get; init; }
    /// <summary>The text outline, or none.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] public NoneOr<TextOutline>? TextOutline { get; init; }
    /// <summary>The shadow, or none.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] public NoneOr<TextShadow>? TextShadow { get; init; }
    /// <summary>The reflection, or none.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] public NoneOr<TextReflection>? TextReflection { get; init; }
    /// <summary>The glow, or none.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] public NoneOr<Glow>? TextGlow { get; init; }
}

/// <summary>Paragraph formatting a match's paragraphs must have, or a replacement applies; each one left null is any.</summary>
public sealed record FindParagraph
{
    /// <summary>The alignment.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] public ParagraphAlignment? Alignment { get; init; }
    /// <summary>The outline level.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] public OutlineLevel? OutlineLevel { get; init; }
    /// <summary>The left indent.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] public Length? Left { get; init; }
    /// <summary>The right indent.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] public Length? Right { get; init; }
    /// <summary>The special indent.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] public SpecialIndent? Special { get; init; }
    /// <summary>The special indent's size.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] public Length? By { get; init; }
    /// <summary>The space before, or automatic.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] public SpacingValue? Before { get; init; }
    /// <summary>The space after, or automatic.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] public SpacingValue? After { get; init; }
    /// <summary>The line spacing.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] public LineSpacing? LineSpacing { get; init; }
    /// <summary>Keep with next.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] public bool? KeepWithNext { get; init; }
    /// <summary>Keep lines together.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] public bool? KeepLinesTogether { get; init; }
    /// <summary>Page break before.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] public bool? PageBreakBefore { get; init; }
    /// <summary>Widow and orphan control.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] public bool? WidowControl { get; init; }
    /// <summary>Collapsed by default.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] public bool? Collapsed { get; init; }
    /// <summary>Mirror indents.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] public bool? MirrorIndents { get; init; }
    /// <summary>Don't add space between paragraphs of the same style.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] public bool? ContextualSpacing { get; init; }
    /// <summary>Suppress line numbers.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] public bool? SuppressLineNumbers { get; init; }
    /// <summary>Don't hyphenate.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] public bool? DontHyphenate { get; init; }
}

/// <summary>Formatting a search looks for (each value given must be the text's), or a replacement applies.</summary>
public sealed record FindFormat
{
    /// <summary>Character formatting.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] public FindFont? Font { get; init; }
    /// <summary>Paragraph formatting.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] public FindParagraph? Paragraph { get; init; }
    /// <summary>A paragraph style the paragraphs have, or a character style the text has.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] public StyleId? Style { get; init; }
    /// <summary>Highlighted in any colour, or not.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] public bool? Highlight { get; init; }
    /// <summary>The colour a replacement highlights in (yellow when null).</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] public HighlightColor? HighlightColor { get; init; }
    /// <summary>The language, a BCP 47 tag, matched ignoring case.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] public string? Language { get; init; }
    /// <summary>Spelling and grammar not checked.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] public bool? NoProof { get; init; }
    /// <summary>Tab stops the paragraphs have (each of these, among others); a replacement adds them.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] public IReadOnlyList<TabStopSpec>? Tabs { get; init; }
    /// <summary>The frame the paragraphs are in; a replacement frames them so.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] public FrameProperties? Frame { get; init; }
}

/// <summary>A tab stop as find formatting names it.</summary>
/// <param name="Position">Its position.</param>
/// <param name="Alignment">Its alignment; null for left.</param>
/// <param name="Leader">Its leader; null for none.</param>
public sealed record TabStopSpec(
    Length Position,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] TabAlignment? Alignment = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] TabLeader? Leader = null);

/// <summary>The result of a find.</summary>
/// <param name="Found">Whether a match was found.</param>
/// <param name="Wrapped">Whether the search wrapped around the document.</param>
public sealed record FindResult(bool Found, bool Wrapped)
{
    /// <summary>The search stopped early: matches past its budget of work were given up.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingDefault)]
    public bool Truncated { get; init; }
}

/// <summary>Every match, for a results list.</summary>
/// <param name="Matches">The matches, in document order.</param>
public sealed record FindMatches(IReadOnlyList<FindMatch> Matches)
{
    /// <summary>The search stopped early: matches past its budget of work were given up.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingDefault)]
    public bool Truncated { get; init; }
}

/// <summary>How many matches Replace All replaced or Highlight All highlighted.</summary>
/// <param name="Count">How many.</param>
public sealed record FindCount(int Count)
{
    /// <summary>The search stopped early: matches past its budget of work were given up.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingDefault)]
    public bool Truncated { get; init; }
}

/// <summary>A find match, with context for a results list.</summary>
/// <param name="Id">The match.</param>
/// <param name="Before">Text before the match.</param>
/// <param name="Text">The matched text.</param>
/// <param name="After">Text after the match.</param>
/// <param name="Page">The page.</param>
public sealed record FindMatch(MatchId Id, string Before, string Text, string After, int Page)
{
    /// <summary>The heading it is under, for grouping results by heading; null before the first heading or outside the body.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public HeadingId? Heading { get; init; }
}

/// <summary>Spell checking.</summary>
public sealed class EditorSpelling
{
    private readonly EditorSession _session;

    internal EditorSpelling(EditorSession session) => _session = session;

    /// <summary>Adds a dictionary.</summary>
    /// <param name="dictionary">The dictionary.</param>
    /// <param name="ct">Cancels loading it.</param>
    /// <returns>Completes when it is loaded.</returns>
    public Task AddDictionaryAsync(Dictionary dictionary, CancellationToken ct = default) => _session.CallAsync(Method.SpellingAddDictionary, WireJson.ToUtf8(dictionary), ct);

    /// <summary>Turns checking on or off.</summary>
    /// <param name="on">Whether checking is on.</param>
    /// <returns>Completes when the setting applies.</returns>
    public Task SetEnabledAsync(bool on) => _session.CallAsync(Method.SpellingSetEnabled, WireJson.ToUtf8(on), default);

    /// <summary>Sets the language for text with no language.</summary>
    /// <param name="language">A BCP 47 language.</param>
    /// <returns>Completes when the setting applies.</returns>
    public Task SetLanguageAsync(string language) => _session.CallAsync(Method.SpellingSetLanguage, WireJson.ToUtf8(language), default);

    /// <summary>Stops flagging a word for this session.</summary>
    /// <param name="word">The word.</param>
    /// <returns>Completes when the setting applies.</returns>
    public Task IgnoreAsync(string word) => _session.CallAsync(Method.SpellingIgnore, WireJson.ToUtf8(word), default);

    /// <summary>Adds a word to the user's dictionary; raises <see cref="EditorSession.WordAdded"/>.</summary>
    /// <param name="word">The word.</param>
    /// <returns>Completes when the word is added.</returns>
    public Task AddAsync(string word) => _session.CallAsync(Method.SpellingAdd, WireJson.ToUtf8(word), default);

    /// <summary>Loads the user's words for a language, kept until that language's dictionary arrives.</summary>
    /// <param name="language">The language.</param>
    /// <param name="words">The words.</param>
    /// <returns>Completes when they are loaded.</returns>
    public Task LoadUserWordsAsync(string language, IEnumerable<string> words) =>
        _session.CallAsync(Method.SpellingLoadUserWords, WireJson.ToUtf8(new UserWordsRequest(language, [.. words])), default);

    /// <summary>Spelling suggestions for a word.</summary>
    /// <param name="word">The word.</param>
    /// <param name="language">The language; by default the session's.</param>
    /// <returns>The suggestions, best first.</returns>
    public Task<IReadOnlyList<string>> GetSuggestionsAsync(string word, string? language = null) =>
        _session.CallAsync<IReadOnlyList<string>>(Method.SpellingSuggestions, WireJson.ToUtf8(new SuggestionsRequest(word, language)), default);

    /// <summary>Selects the next misspelling (F7).</summary>
    /// <param name="ct">Cancels the search.</param>
    /// <returns>The misspelling, or null when there are none.</returns>
    public Task<Misspelling?> NextAsync(CancellationToken ct = default) => _session.CallAsync<Misspelling?>(Method.SpellingNext, [], ct);
}

/// <summary>A misspelled word and its suggestions.</summary>
/// <param name="Word">The word.</param>
/// <param name="Suggestions">The suggestions, best first.</param>
public sealed record Misspelling(string Word, IReadOnlyList<string> Suggestions);

/// <summary>Headings and bookmarks, for a navigation pane and the Bookmark dialog.</summary>
public sealed class EditorNavigation
{
    private readonly EditorSession _session;

    internal EditorNavigation(EditorSession session) => _session = session;

    /// <summary>The document's headings.</summary>
    /// <param name="ct">Cancels the request.</param>
    /// <returns>The headings in document order.</returns>
    public Task<IReadOnlyList<Heading>> GetHeadingsAsync(CancellationToken ct = default) => _session.CallAsync<IReadOnlyList<Heading>>(Method.Headings, [], ct);

    /// <summary>The document's bookmarks, hidden ones too. Go to one with <see cref="EditorCommands.GoToBookmarkAsync"/>.</summary>
    /// <param name="ct">Cancels the request.</param>
    /// <returns>The bookmarks in document order.</returns>
    public Task<IReadOnlyList<BookmarkInfo>> GetBookmarksAsync(CancellationToken ct = default) => _session.CallAsync<IReadOnlyList<BookmarkInfo>>(Method.Bookmarks, [], ct);

    /// <summary>Selects and reveals a heading.</summary>
    /// <param name="heading">The heading.</param>
    /// <param name="ct">Cancels the request.</param>
    /// <returns>Completes when it is selected.</returns>
    public Task SelectAsync(HeadingId heading, CancellationToken ct = default) => _session.CallAsync(Method.SelectHeading, WireJson.ToUtf8(heading), ct);

    /// <summary>The heading the selection is under, as a navigation pane highlights it; null before the first heading or outside the body. Ask on selection changes.</summary>
    /// <param name="ct">Cancels the request.</param>
    /// <returns>The heading.</returns>
    public Task<HeadingId?> GetCurrentHeadingAsync(CancellationToken ct = default) => _session.CallAsync<HeadingId?>(Method.CurrentHeading, [], ct);
}

/// <summary>A heading.</summary>
/// <param name="Id">The heading.</param>
/// <param name="Level">Its outline level.</param>
/// <param name="Text">Its text.</param>
/// <param name="Page">Its page.</param>
public sealed record Heading(HeadingId Id, int Level, string Text, int Page);

/// <summary>A bookmark.</summary>
/// <param name="Name">Its name.</param>
/// <param name="Hidden">Hidden: Word's own (<c>_Toc</c>, <c>_Ref</c>), named with a leading <c>_</c>.</param>
/// <param name="Empty">Its text holds nothing: it marks a place.</param>
public sealed record BookmarkInfo(string Name, bool Hidden, bool Empty);

/// <summary>The document's styles, for a style gallery.</summary>
public sealed class EditorStyles
{
    private readonly EditorSession _session;

    internal EditorStyles(EditorSession session) => _session = session;

    /// <summary>The styles, optionally of one type.</summary>
    /// <param name="type">Only styles of this type.</param>
    /// <param name="ct">Cancels the request.</param>
    /// <returns>The styles.</returns>
    public Task<IReadOnlyList<StyleInfo>> ListAsync(StyleType? type = null, CancellationToken ct = default) =>
        _session.CallAsync<IReadOnlyList<StyleInfo>>(Method.Styles, type is { } t ? WireJson.ToUtf8(t) : [], ct);

    /// <summary>A style's gallery tile: its sample text in the style's character formatting, scaled to fit.</summary>
    /// <param name="id">The style.</param>
    /// <param name="size">The tile's size.</param>
    /// <param name="ct">Cancels the request.</param>
    /// <returns>The tile as PNG.</returns>
    public Task<PreviewPicture> TileAsync(StyleId id, PreviewSize size, CancellationToken ct = default) =>
        _session.CallAsync<PreviewPicture>(Method.StyleTile, WireJson.ToUtf8(new StyleTileRequest(id, size)), ct);

    /// <summary>A style as it is defined, without a dialog: the document's, or a built-in one it doesn't define yet.</summary>
    /// <param name="id">The style.</param>
    /// <param name="ct">Cancels the request.</param>
    /// <returns>The definition; null for an id that is neither.</returns>
    public Task<StyleDefinition?> GetDefinitionAsync(StyleId id, CancellationToken ct = default) =>
        _session.CallAsync<StyleDefinition?>(Method.StyleDefinition, WireJson.ToUtf8(id), ct);

    /// <summary>Every built-in style, defined by the document or not, with its list flags (<see cref="EditorCommands.SetStyleMetaAsync"/> sets them).</summary>
    /// <param name="ct">Cancels the request.</param>
    /// <returns>The built-in styles.</returns>
    public Task<IReadOnlyList<BuiltInStyle>> GetBuiltInAsync(CancellationToken ct = default) =>
        _session.CallAsync<IReadOnlyList<BuiltInStyle>>(Method.BuiltInStyles, [], ct);

    /// <summary>The document's default formatting (<see cref="EditorCommands.SetDefaultFormattingAsync"/> sets it).</summary>
    /// <param name="ct">Cancels the request.</param>
    /// <returns>The defaults; null without a styles part.</returns>
    public Task<DefaultsDefinition?> GetDocumentDefaultsAsync(CancellationToken ct = default) =>
        _session.CallAsync<DefaultsDefinition?>(Method.DocumentDefaults, [], ct);
}

/// <summary>A preview picture's size: device-independent pixels, and device pixels per one.</summary>
/// <param name="Width">The width.</param>
/// <param name="Height">The height.</param>
/// <param name="Scale">Device pixels per device-independent pixel.</param>
public sealed record PreviewSize(double Width, double Height, double Scale = 1);

/// <summary>A picture the editor drew.</summary>
/// <param name="Png">The PNG.</param>
/// <param name="Width">Its width, device pixels.</param>
/// <param name="Height">Its height, device pixels.</param>
public sealed record PreviewPicture(byte[] Png, int Width, int Height);

/// <summary>A list gallery.</summary>
public enum ListGalleryKind
{
    /// <summary>The bullet library.</summary>
    Bullet,
    /// <summary>The numbering library.</summary>
    Numbering,
    /// <summary>The multilevel list library.</summary>
    Multilevel,
}

/// <summary>A section of a list gallery.</summary>
public enum ListGallerySectionKind
{
    /// <summary>The built-in library.</summary>
    Library,
    /// <summary>The lists used in this session.</summary>
    RecentlyUsed,
    /// <summary>The document's lists.</summary>
    Document,
    /// <summary>The document's list styles.</summary>
    ListStyles,
}

/// <summary>A list gallery entry.</summary>
/// <param name="Id">The entry, for <see cref="EditorCommands.ApplyListEntryAsync"/>.</param>
/// <param name="Label">A short label: its first levels' numbers or its bullet.</param>
/// <param name="Levels">Its levels.</param>
public sealed record ListEntry(string Id, string Label, IReadOnlyList<ListLevel> Levels);

/// <summary>A section of a list gallery and its entries.</summary>
/// <param name="Kind">Which section.</param>
/// <param name="Entries">Its entries.</param>
public sealed record ListGallerySection(ListGallerySectionKind Kind, IReadOnlyList<ListEntry> Entries);

/// <summary>A run of characters a font has.</summary>
/// <param name="First">The first code point.</param>
/// <param name="Last">The last code point.</param>
public sealed record CharRange(int First, int Last);

/// <summary>The list galleries, and the characters of fonts for a symbol picker.</summary>
public sealed class EditorLists
{
    private readonly EditorSession _session;

    internal EditorLists(EditorSession session) => _session = session;

    /// <summary>A list gallery's sections: the built-in library, the lists used in this session, the document's lists, and its list styles.</summary>
    /// <param name="kind">The gallery.</param>
    /// <param name="ct">Cancels the request.</param>
    /// <returns>The sections.</returns>
    public Task<IReadOnlyList<ListGallerySection>> GalleriesAsync(ListGalleryKind kind, CancellationToken ct = default) =>
        _session.CallAsync<IReadOnlyList<ListGallerySection>>(Method.ListGalleries, WireJson.ToUtf8(kind), ct);

    /// <summary>A list gallery entry's preview: three items, or three levels of a multilevel list.</summary>
    /// <param name="id">The entry.</param>
    /// <param name="size">The picture's size.</param>
    /// <param name="ct">Cancels the request.</param>
    /// <returns>The preview as PNG.</returns>
    public Task<PreviewPicture> PreviewAsync(string id, PreviewSize size, CancellationToken ct = default) =>
        _session.CallAsync<PreviewPicture>(Method.ListEntryPreview, WireJson.ToUtf8(new ListEntryPreviewRequest(id, size)), ct);

    /// <summary>The list of the paragraph the selection's focus is in, its nine levels in full (<see cref="EditorCommands.SetListDefinitionAsync"/> sets them).</summary>
    /// <param name="ct">Cancels the request.</param>
    /// <returns>The list; null outside a list.</returns>
    public Task<ListDefinitionInfo?> GetDefinitionAsync(CancellationToken ct = default) =>
        _session.CallAsync<ListDefinitionInfo?>(Method.ListDefinition, [], ct);

    /// <summary>The characters a font has, as runs of code points in its Basic Multilingual Plane.</summary>
    /// <param name="family">The font family.</param>
    /// <param name="ct">Cancels the request.</param>
    /// <returns>The runs, in order.</returns>
    public Task<IReadOnlyList<CharRange>> FontCharactersAsync(string family, CancellationToken ct = default) =>
        _session.CallAsync<IReadOnlyList<CharRange>>(Method.FontCharacters, WireJson.ToUtf8(family), ct);

    /// <summary>A character in a font, drawn to fit.</summary>
    /// <param name="family">The font family.</param>
    /// <param name="character">The character.</param>
    /// <param name="size">The picture's size.</param>
    /// <param name="ct">Cancels the request.</param>
    /// <returns>The picture as PNG.</returns>
    public Task<PreviewPicture> CharacterPictureAsync(string family, string character, PreviewSize size, CancellationToken ct = default) =>
        _session.CallAsync<PreviewPicture>(Method.CharacterPicture, WireJson.ToUtf8(new CharacterPictureRequest(family, character, size)), ct);
}

/// <summary>A style type.</summary>
public enum StyleType
{
    /// <summary>A paragraph style.</summary>
    Paragraph,
    /// <summary>A character style.</summary>
    Character,
    /// <summary>A table style.</summary>
    Table,
    /// <summary>A list (numbering) style.</summary>
    List,
}

/// <summary>How a style gallery tile looks.</summary>
/// <param name="Font">The family.</param>
/// <param name="Size">The size.</param>
/// <param name="Bold">Bold.</param>
/// <param name="Italic">Italic.</param>
/// <param name="Color">The colour.</param>
public sealed record StylePreview(string Font, Length Size, bool Bold, bool Italic, ColorValue Color);

/// <summary>A style.</summary>
/// <param name="Id">The style id.</param>
/// <param name="Name">The display name.</param>
/// <param name="Type">The type.</param>
/// <param name="BuiltIn">Whether it is built in.</param>
/// <param name="Quick">Whether it shows in a style gallery.</param>
/// <param name="Priority">Its sort priority.</param>
/// <param name="Hidden">Whether it is hidden.</param>
/// <param name="BasedOn">The style it is based on.</param>
/// <param name="Preview">How its tile looks.</param>
/// <param name="Link">The linked character or paragraph style.</param>
/// <param name="Visibility">Whether it shows in style lists.</param>
/// <param name="Locked">Restricted: unavailable while the document's formatting is protected.</param>
/// <param name="Uses">How many times the main document uses it: paragraphs for a paragraph style, stretches of runs for a character style, 0 for table and list styles (<see cref="EditorCommands.SelectStyleInstancesAsync"/> selects them).</param>
public sealed record StyleInfo(
    StyleId Id, string Name, StyleType Type, bool BuiltIn, bool Quick, int Priority, bool Hidden, StyleId? BasedOn, StylePreview Preview,
    StyleId? Link = null, StyleVisibility Visibility = StyleVisibility.Show, bool Locked = false, int Uses = 0)
{
    /// <summary>Its other names; none when null.</summary>
    [System.Text.Json.Serialization.JsonIgnore(Condition = System.Text.Json.Serialization.JsonIgnoreCondition.WhenWritingNull)]
    public IReadOnlyList<string>? Aliases { get; init; }
}

/// <summary>Whether a style shows in style lists.</summary>
public enum StyleVisibility
{
    /// <summary>Shown.</summary>
    Show,
    /// <summary>Hidden until a paragraph or run uses it.</summary>
    HideUntilUsed,
    /// <summary>Hidden.</summary>
    Hide,
}

/// <summary>Merge-designer elements: fields and content controls.</summary>
public sealed class EditorElements
{
    private readonly EditorSession _session;

    internal EditorElements(EditorSession session) => _session = session;

    /// <summary>The elements, in document order.</summary>
    /// <param name="filter">Which elements.</param>
    /// <param name="ct">Cancels the request.</param>
    /// <returns>The elements.</returns>
    public Task<IReadOnlyList<ElementInfo>> ListAsync(ElementFilter? filter = null, CancellationToken ct = default) =>
        _session.CallAsync<IReadOnlyList<ElementInfo>>(Method.Elements, filter is null ? [] : WireJson.ToUtf8(filter), ct);

    /// <summary>One element in detail.</summary>
    /// <param name="id">The element.</param>
    /// <param name="ct">Cancels the request.</param>
    /// <returns>The element.</returns>
    public Task<ElementDetail> GetAsync(ElementId id, CancellationToken ct = default) => _session.CallAsync<ElementDetail>(Method.Element, WireJson.ToUtf8(id), ct);

    /// <summary>Insert Field's fields, of one category or all, in its order.</summary>
    /// <param name="category">Only this category's; null for all.</param>
    /// <param name="ct">Cancels the request.</param>
    /// <returns>The fields.</returns>
    public Task<IReadOnlyList<FieldDescription>> GetFieldCatalogueAsync(FieldCategory? category = null, CancellationToken ct = default) =>
        _session.CallAsync<IReadOnlyList<FieldDescription>>(Method.FieldCatalogue, category is { } c ? WireJson.ToUtf8(c) : [], ct);

    /// <summary>The formats and pictures Insert Field offers.</summary>
    /// <param name="ct">Cancels the request.</param>
    /// <returns>The formats.</returns>
    public Task<FieldFormats> GetFieldFormatsAsync(CancellationToken ct = default) => _session.CallAsync<FieldFormats>(Method.FieldFormats, [], ct);

    /// <summary>The field code Insert Field's choices make, and its problems (none when it takes it as it is).</summary>
    /// <param name="choice">The choices.</param>
    /// <param name="ct">Cancels the request.</param>
    /// <returns>The code and its problems.</returns>
    public Task<ComposedField> ComposeFieldAsync(FieldChoice choice, CancellationToken ct = default) =>
        _session.CallAsync<ComposedField>(Method.ComposeField, WireJson.ToUtf8(choice), ct);

    /// <summary>The choices a field code is made of, as Insert Field opens on it.</summary>
    /// <param name="code">The field code.</param>
    /// <param name="ct">Cancels the request.</param>
    /// <returns>The choices.</returns>
    public Task<FieldChoice> ParseFieldAsync(string code, CancellationToken ct = default) => _session.CallAsync<FieldChoice>(Method.ParseField, WireJson.ToUtf8(code), ct);

    /// <summary>The field the selection is in, with its code's choices; null outside a field.</summary>
    /// <param name="ct">Cancels the request.</param>
    /// <returns>The field and its choices.</returns>
    public Task<FieldHere?> GetFieldHereAsync(CancellationToken ct = default) => _session.CallAsync<FieldHere?>(Method.FieldHere, [], ct);

    /// <summary>The display formats a date control offers for a locale.</summary>
    /// <param name="locale">The locale (BCP 47).</param>
    /// <param name="ct">Cancels the request.</param>
    /// <returns>The formats.</returns>
    public Task<IReadOnlyList<string>> GetDateFormatsAsync(string locale, CancellationToken ct = default) =>
        _session.CallAsync<IReadOnlyList<string>>(Method.DateFormats, WireJson.ToUtf8(locale), ct);

    /// <summary>
    /// An element's boxes on screen in client coordinates, one per line it spans, for anchoring your own chrome to it:
    /// none while it is out of view.
    /// </summary>
    /// <param name="id">The element.</param>
    /// <param name="ct">Cancels the request.</param>
    /// <returns>The boxes.</returns>
    public async Task<IReadOnlyList<System.Drawing.RectangleF>> BoxesAsync(ElementId id, CancellationToken ct = default)
    {
        var boxes = await _session.CallAsync<IReadOnlyList<System.Drawing.RectangleF>>(Method.ElementBoxes, WireJson.ToUtf8(id), ct).ConfigureAwait(false);
        return _session.ToClient(boxes);
    }

    /// <summary>The body, or an element's content, as blocks.</summary>
    /// <param name="root">The element; by default the body.</param>
    /// <param name="depth">How deep to go.</param>
    /// <param name="ct">Cancels the request.</param>
    /// <returns>The blocks.</returns>
    /// <exception cref="EditorException"><paramref name="root"/> is not an element id (<see cref="EditorErrorCode.InvalidArgument"/>), or
    /// the document has no such element (<see cref="EditorErrorCode.Unavailable"/>).</exception>
    public Task<IReadOnlyList<Block>> GetStructureAsync(ElementId? root = null, int? depth = null, CancellationToken ct = default) =>
        _session.CallAsync<IReadOnlyList<Block>>(Method.Structure, WireJson.ToUtf8(new StructureRequest(root, depth)), ct);

    /// <summary>Selects an element, entering a header or footer when needed.</summary>
    /// <param name="id">The element.</param>
    /// <param name="reveal">Scroll it into view.</param>
    /// <param name="ct">Cancels the request.</param>
    /// <returns>Completes when it is selected.</returns>
    public Task SelectAsync(ElementId id, bool reveal = true, CancellationToken ct = default) =>
        _session.CallAsync(Method.SelectElement, WireJson.ToUtf8(new SelectElementRequest(id, reveal)), ct);
}

/// <summary>A text match on element codes, tags or titles.</summary>
/// <param name="In">Which text.</param>
/// <param name="Text">The text to contain.</param>
public sealed record ElementMatch(ElementTextField In, string Text);

/// <summary>Which elements to list.</summary>
/// <param name="Kind">Only this kind.</param>
/// <param name="Match">Only elements whose text matches.</param>
/// <param name="Story">Only this story.</param>
public sealed record ElementFilter(
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] ElementKind? Kind = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] ElementMatch? Match = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] Story? Story = null)
{
    /// <summary>Only the elements in this sub-template's body.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public string? SubTemplate { get; init; }
}

/// <summary>A sub-template: a building block in a category the rules declare.</summary>
/// <param name="Name">Its name.</param>
/// <param name="Category">Its building block category.</param>
/// <param name="References">The reference fields naming it in the template's body, headers, footers and notes.</param>
/// <param name="NestedReferences">The reference fields naming it in other sub-templates' bodies.</param>
/// <param name="Elements">The fields and controls in its body.</param>
public sealed record SubTemplateInfo(string Name, string Category, int References, int NestedReferences, IReadOnlyList<ElementInfo> Elements);

/// <summary>
/// The template's sub-templates, and editing one in this editor. While one is edited, <see cref="EditorState.Editing"/>
/// names it and every command, query and event concerns its body; <see cref="EditorSession.EditingTargetChanged"/>
/// is raised on each switch.
/// </summary>
public sealed class EditorSubTemplates
{
    private readonly EditorSession _session;

    internal EditorSubTemplates(EditorSession session) => _session = session;

    /// <summary>The sub-templates the rules declare, in glossary order.</summary>
    /// <param name="ct">Cancels the request.</param>
    /// <returns>The sub-templates.</returns>
    public Task<IReadOnlyList<SubTemplateInfo>> ListAsync(CancellationToken ct = default) =>
        _session.CallAsync<IReadOnlyList<SubTemplateInfo>>(Method.SubTemplates, [], ct);

    /// <summary>Edits a sub-template's body in this editor, on its own pages.</summary>
    /// <param name="name">The sub-template.</param>
    /// <param name="category">Its building block category, when several rules name categories.</param>
    /// <param name="ct">Cancels the request.</param>
    /// <returns>Completes when the editor is on it.</returns>
    public Task EditAsync(string name, string? category = null, CancellationToken ct = default) =>
        _session.RunAsync(EditorOperation.EditSubTemplate(name, category), ct);

    /// <summary>Back to the template, at its selection from before.</summary>
    /// <param name="ct">Cancels the request.</param>
    /// <returns>Completes when the template shows.</returns>
    public Task CloseAsync(CancellationToken ct = default) => _session.RunAsync(EditorOperation.CloseSubTemplate(), ct);

    /// <summary>A new sub-template: empty, with the body of the DOCX <paramref name="from"/>, or a copy of the sub-template <paramref name="copyOf"/>. One undo step.</summary>
    /// <param name="name">Its name, unique in its category.</param>
    /// <param name="category">Its building block category; null for the first rule's.</param>
    /// <param name="from">A DOCX whose body becomes its body.</param>
    /// <param name="copyOf">A sub-template in the same category whose body (formatting, fields, controls, pictures, references) is copied; not with <paramref name="from"/>.</param>
    /// <param name="ct">Cancels the request.</param>
    /// <returns>Completes when it has committed.</returns>
    public Task CreateAsync(string name, string? category = null, ReadOnlyMemory<byte>? from = null, string? copyOf = null, CancellationToken ct = default) =>
        _session.RunAsync(EditorOperation.CreateSubTemplate(name, category, from, copyOf), ct);

    /// <summary>A standalone DOCX of a sub-template's body, with the document's styles, numbering, theme and fonts and without its headers, footers or other sub-templates; <see cref="CreateAsync"/>'s <c>from</c> reads it back unchanged.</summary>
    /// <param name="name">The sub-template.</param>
    /// <param name="category">Its building block category, when several rules name categories.</param>
    /// <param name="ct">Cancels the request.</param>
    /// <returns>The DOCX.</returns>
    public Task<byte[]> ExportAsync(string name, string? category = null, CancellationToken ct = default) =>
        _session.CallAsync<byte[]>(Method.SubTemplateExport, WireJson.ToUtf8(new SubTemplateExportRequest(name, category)), ct);

    /// <summary>Renames a sub-template, and optionally every reference to it. One undo step.</summary>
    /// <param name="name">Its name now.</param>
    /// <param name="to">Its new name.</param>
    /// <param name="updateReferences">Rename the references too.</param>
    /// <param name="category">Its building block category, when several rules name categories.</param>
    /// <param name="ct">Cancels the request.</param>
    /// <returns>Completes when it has committed.</returns>
    public Task RenameAsync(string name, string to, bool updateReferences = false, string? category = null, CancellationToken ct = default) =>
        _session.RunAsync(EditorOperation.RenameSubTemplate(name, to, updateReferences, category), ct);

    /// <summary>Removes a sub-template; its references no longer resolve. One undo step.</summary>
    /// <param name="name">The sub-template.</param>
    /// <param name="category">Its building block category, when several rules name categories.</param>
    /// <param name="ct">Cancels the request.</param>
    /// <returns>Completes when it has committed.</returns>
    public Task RemoveAsync(string name, string? category = null, CancellationToken ct = default) =>
        _session.RunAsync(EditorOperation.RemoveSubTemplate(name, category), ct);
}

/// <summary>An element.</summary>
/// <param name="Id">The element.</param>
/// <param name="Kind">What it is.</param>
/// <param name="Code">A field's code.</param>
/// <param name="Tag">A control's tag.</param>
/// <param name="Title">A control's title.</param>
/// <param name="Display">The result or content text, up to 200 characters.</param>
/// <param name="Story">The story it is in.</param>
/// <param name="Parent">The element it is in.</param>
/// <param name="Locked">Whether it is locked.</param>
public record ElementInfo(
    ElementId Id, ElementKind Kind,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? Code,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? Tag,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? Title,
    string Display, Story Story, ElementId? Parent, bool Locked)
{
    /// <summary>A field shows its code rather than its result.</summary>
    public bool ShowingCode { get; init; }

    /// <summary>A field's result is out of date: the next field update refreshes it.</summary>
    public bool Dirty { get; init; }

    /// <summary>A date control's date (<c>YYYY-MM-DDTHH:MM:SSZ</c>); null while it shows its placeholder or no date.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public string? Date { get; init; }

    /// <summary>The sub-template whose body it is in, when <see cref="Story"/> is <see cref="Story.SubTemplate"/>.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public string? SubTemplate { get; init; }

    /// <summary>A sub-template reference field's sub-template.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public SubTemplateReference? Reference { get; init; }

    /// <summary>Whether a content control shows its placeholder.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public bool? ShowingPlaceholder { get; init; }

    /// <summary>A picture control's picture size (inline or floating).</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public PictureSize? Size { get; init; }
}

/// <summary>A picture's width and height.</summary>
/// <param name="Width">The width.</param>
/// <param name="Height">The height.</param>
public sealed record PictureSize(Length Width, Length Height);

/// <summary>
/// One sub-template reference rule: a field of type <paramref name="FieldType"/> names, in its <see cref="Argument"/>
/// argument, the building block in <paramref name="Category"/> (and <see cref="Gallery"/>) whose body it shows.
/// </summary>
/// <param name="FieldType">The field type (<c>SUBTEMPLATE</c>), compared without case.</param>
/// <param name="Category">The building block category the name is looked up in.</param>
public sealed record ReferenceRule(string FieldType, string Category)
{
    /// <summary>Which of the field's arguments is the sub-template's name, from 0; null for 0.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public int? Argument { get; init; }

    /// <summary>The building block gallery (<c>custom1</c>), when it matters.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public string? Gallery { get; init; }
}

/// <summary>Whether a reference's sub-template resolves, or why its body can't show.</summary>
public enum ReferenceResolution
{
    /// <summary>A sub-template has the name.</summary>
    Resolved,
    /// <summary>No sub-template has the name.</summary>
    Missing,
    /// <summary>Only a name in another case matches.</summary>
    NearMatch,
    /// <summary>The sub-template is already being shown around it.</summary>
    Cycle,
    /// <summary>More than 16 sub-templates deep.</summary>
    TooDeep,
    /// <summary>The field shares its paragraph with other content.</summary>
    NotAlone,
    /// <summary>The field is in a header, footer, note or text box.</summary>
    NotInBody,
}

/// <summary>What a sub-template reference field names and whether its body shows.</summary>
/// <param name="Name">The sub-template's name, as the field gives it.</param>
/// <param name="Resolution">Whether it names a sub-template whose body can show in its place.</param>
/// <param name="Expanded">Its body shows in its place.</param>
public sealed record SubTemplateReference(string Name, ReferenceResolution Resolution, bool Expanded)
{
    /// <summary>The sub-template's name when only a name in another case matches.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public string? NearMatch { get; init; }
}

/// <summary>An element in detail.</summary>
/// <param name="Id">The element.</param>
/// <param name="Kind">What it is.</param>
/// <param name="Code">A field's code.</param>
/// <param name="Tag">A control's tag.</param>
/// <param name="Title">A control's title.</param>
/// <param name="Display">The result or content text, up to 200 characters.</param>
/// <param name="Story">The story it is in.</param>
/// <param name="Parent">The element it is in.</param>
/// <param name="Locked">Whether it is locked.</param>
/// <param name="Lock">What its lock prevents.</param>
/// <param name="Placeholder">Its placeholder text.</param>
/// <param name="Binding">Its data binding.</param>
/// <param name="Options">Kind-specific options.</param>
/// <param name="Children">The elements in it.</param>
public sealed record ElementDetail(
    ElementId Id, ElementKind Kind, string? Code, string? Tag, string? Title, string Display, Story Story, ElementId? Parent, bool Locked,
    ControlLock Lock,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? Placeholder,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] DataBinding? Binding,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] ControlOptions? Options,
    IReadOnlyList<ElementId> Children) : ElementInfo(Id, Kind, Code, Tag, Title, Display, Story, Parent, Locked);

/// <summary>A vertical cell merge.</summary>
public enum VerticalMerge
{
    /// <summary>Not merged.</summary>
    None,
    /// <summary>Starts a merge.</summary>
    Start,
    /// <summary>Continues the merge above.</summary>
    Continue,
}

/// <summary>A block of the document structure: a paragraph or a table.</summary>
[JsonPolymorphic(TypeDiscriminatorPropertyName = "kind")]
[JsonDerivedType(typeof(Block.Paragraph), "paragraph")]
[JsonDerivedType(typeof(Block.Table), "table")]
public abstract record Block
{
    private protected Block() { }

    /// <summary>A paragraph.</summary>
    /// <param name="TextLength">Its text length.</param>
    /// <param name="Elements">The elements in it.</param>
    /// <param name="HiddenMarks">Its hidden paragraph marks: where each paragraph joined into it by one ends, and its own mark when that is hidden; empty when none.</param>
    public sealed record Paragraph(int TextLength, IReadOnlyList<ElementId> Elements, [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingDefault)] IReadOnlyList<HiddenMark>? HiddenMarks = null) : Block;

    /// <summary>A hidden paragraph mark: a style separator runs a heading into the paragraph after it.</summary>
    /// <param name="At">Where it is, in characters from the paragraph's start.</param>
    /// <param name="StyleSeparator">It is a style separator (Ctrl+Alt+Enter), not text formatted hidden.</param>
    public sealed record HiddenMark(int At, bool StyleSeparator);

    /// <summary>A table.</summary>
    /// <param name="Rows">Its rows.</param>
    public sealed record Table(IReadOnlyList<TableRow> Rows) : Block;

    /// <summary>A table row.</summary>
    /// <param name="Cells">Its cells.</param>
    public sealed record TableRow(IReadOnlyList<TableCell> Cells);

    /// <summary>A table cell.</summary>
    /// <param name="Span">The columns it spans.</param>
    /// <param name="VerticalMerge">Its vertical merge.</param>
    /// <param name="Empty">Whether it is empty.</param>
    /// <param name="Blocks">Its content.</param>
    public sealed record TableCell(int Span, VerticalMerge VerticalMerge, bool Empty, IReadOnlyList<Block> Blocks);
}

/// <summary>Custom XML data parts. Writing and removing are the WriteDataPart and RemoveDataPart commands.</summary>
public sealed class EditorDataParts
{
    private readonly EditorSession _session;

    internal EditorDataParts(EditorSession session) => _session = session;

    /// <summary>The data parts.</summary>
    /// <param name="ct">Cancels the request.</param>
    /// <returns>The parts.</returns>
    public Task<IReadOnlyList<DataPartInfo>> ListAsync(CancellationToken ct = default) => _session.CallAsync<IReadOnlyList<DataPartInfo>>(Method.DataParts, [], ct);

    /// <summary>Data parts with their XML.</summary>
    /// <param name="namespace">Only parts with this namespace.</param>
    /// <param name="itemId">Only the part with this item id.</param>
    /// <param name="ct">Cancels the request.</param>
    /// <returns>The parts.</returns>
    public Task<IReadOnlyList<DataPart>> GetAsync(string? @namespace = null, string? itemId = null, CancellationToken ct = default) =>
        _session.CallAsync<IReadOnlyList<DataPart>>(Method.DataParts, WireJson.ToUtf8(new DataPartsRequest(@namespace, itemId)), ct);
}

/// <summary>A data part.</summary>
/// <param name="Namespace">Its root namespace.</param>
/// <param name="ItemId">Its item id.</param>
public sealed record DataPartInfo(string? Namespace, string ItemId);

/// <summary>A data part with its XML.</summary>
/// <param name="Namespace">Its root namespace.</param>
/// <param name="ItemId">Its item id.</param>
/// <param name="Xml">Its XML.</param>
public sealed record DataPart(string? Namespace, string ItemId, string Xml);

/// <summary>The document's accessible structure.</summary>
public sealed class EditorAccessibility
{
    private readonly EditorSession _session;

    internal EditorAccessibility(EditorSession session) => _session = session;

    /// <summary>The accessible tree.</summary>
    /// <param name="ct">Cancels the request.</param>
    /// <returns>The root node.</returns>
    public Task<AccessibleNode> GetTreeAsync(CancellationToken ct = default) => _session.CallAsync<AccessibleNode>(Method.AccessibleTree, [], ct);

    /// <summary>Puts the caret at the start of a node's content and reveals it: a note reference's note, a comment's
    /// commented text. Fails with <c>InvalidArgument</c> when no node has the id, and <c>Unavailable</c> when the node
    /// holds no caret position (a region).</summary>
    /// <param name="id">The node's <see cref="AccessibleNode.Id"/>.</param>
    /// <param name="ct">Cancels the request.</param>
    public Task GoAsync(long id, CancellationToken ct = default) => _session.CallAsync(Method.AccessibleGo, WireJson.ToUtf8(id), ct);

    /// <summary>The kinds of announcements the editor makes; the others are neither said nor raised. Default: all.</summary>
    /// <param name="kinds">The kinds to make.</param>
    /// <param name="ct">Cancels the request.</param>
    public Task SetAnnouncementsAsync(IReadOnlyList<AnnouncementKind> kinds, CancellationToken ct = default) =>
        _session.CallAsync(Method.AnnouncementsSet, WireJson.ToUtf8(kinds), ct);
}

/// <summary>An accessible node's role.</summary>
public enum AccessibleRole
{
    /// <summary>The document.</summary>
    Document,
    /// <summary>A paragraph.</summary>
    Paragraph,
    /// <summary>A heading (its <c>Level</c>, 1 to 9).</summary>
    Heading,
    /// <summary>A list.</summary>
    List,
    /// <summary>A list item (its <c>Level</c> and <c>Position</c>, and a numbered item's <c>Label</c>).</summary>
    ListItem,
    /// <summary>A table (its nesting <c>Level</c>).</summary>
    Table,
    /// <summary>A row.</summary>
    Row,
    /// <summary>A cell (its <c>Cell</c> place).</summary>
    Cell,
    /// <summary>A cell of a header row.</summary>
    ColumnHeader,
    /// <summary>A cell that heads its row.</summary>
    RowHeader,
    /// <summary>A picture or shape, named by its alternative text.</summary>
    Image,
    /// <summary>A link.</summary>
    Link,
    /// <summary>A text box or a shape's text.</summary>
    TextBox,
    /// <summary>A footnote or endnote.</summary>
    Note,
    /// <summary>A footnote or endnote reference (its <c>Target</c> is the note).</summary>
    NoteRef,
    /// <summary>A comment (its <c>Author</c>), with its replies as children.</summary>
    Comment,
    /// <summary>A header.</summary>
    Header,
    /// <summary>A footer.</summary>
    Footer,
    /// <summary>A named group after the body: headers and footers, footnotes, endnotes, comments.</summary>
    Region,
    /// <summary>A run of text with the same marks, author, language and comment.</summary>
    Text,
}

/// <summary>A mark on accessible text.</summary>
public enum AccessibleMark
{
    /// <summary>An insertion.</summary>
    Insertion,
    /// <summary>A deletion.</summary>
    Deletion,
    /// <summary>A misspelling.</summary>
    Misspelling,
}

/// <summary>Where a cell is in its table's grid: its row and column (from 1) and how many it spans.</summary>
/// <param name="Row">Its row, from 1.</param>
/// <param name="Col">Its first grid column, from 1.</param>
/// <param name="RowSpan">How many rows it spans.</param>
/// <param name="ColSpan">How many grid columns it spans.</param>
public sealed record CellPlace(int Row, int Col, int RowSpan, int ColSpan);

/// <summary>Where a list item is in its list: its index (from 1) among the items at its level, and how many there are.</summary>
/// <param name="Index">Its index, from 1.</param>
/// <param name="Count">The number of items.</param>
public sealed record ListPosition(int Index, int Count);

/// <summary>A node of the accessible tree.</summary>
/// <param name="Id">Its identity: the same across edits while the thing it names exists.</param>
/// <param name="Role">Its role.</param>
/// <param name="Children">Its children.</param>
public sealed record AccessibleNode(long Id, AccessibleRole Role, IReadOnlyList<AccessibleNode> Children)
{
    /// <summary>Its name.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public string? Name { get; init; }

    /// <summary>A text node's text.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public string? Text { get; init; }

    /// <summary>A heading's level, a list item's nesting level, or a table's nesting level, from 1.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public int? Level { get; init; }

    /// <summary>A numbered list item's number as shown (<c>3.</c>, <c>a)</c>).</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public string? Label { get; init; }

    /// <summary>A list item's position in its list.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public ListPosition? Position { get; init; }

    /// <summary>A cell's place in its table.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public CellPlace? Cell { get; init; }

    /// <summary>Review and spelling marks on a text node.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public IReadOnlyList<AccessibleMark>? Marks { get; init; }

    /// <summary>Whose marks or comment it is.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public string? Author { get; init; }

    /// <summary>Its language (BCP 47), where it differs from its parent's.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public string? Lang { get; init; }

    /// <summary>The comment the text is in the range of.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public long? Details { get; init; }

    /// <summary>The note a note reference points to; for a sub-template body shown in place, its reference's paragraph.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public long? Target { get; init; }
}

/// <summary>What an announcement is about.</summary>
public enum AnnouncementKind
{
    /// <summary>Entering, leaving or moving through a table.</summary>
    Table,
    /// <summary>Entering or leaving a list.</summary>
    List,
    /// <summary>The caret moved to another page.</summary>
    Page,
    /// <summary>An insertion or deletion at the caret.</summary>
    Review,
    /// <summary>A commented range at the caret, with the comment.</summary>
    Comment,
    /// <summary>A misspelled word at the caret.</summary>
    Spelling,
    /// <summary>Formatting turned on or off, or described on request.</summary>
    Formatting,
    /// <summary>Entering or leaving a header, footer, note or text box.</summary>
    Mode,
    /// <summary>Read-only content: entered, or an edit refused there.</summary>
    ReadOnly,
    /// <summary>What a command did (undo, redo, cut, copy, paste).</summary>
    Command,
    /// <summary>The selection (select all).</summary>
    Selection,
}

/// <summary>A pointer gesture the engine recognises.</summary>
public enum GestureId
{
    /// <summary>A double-click in a page's top margin.</summary>
    HeaderAreaDoubleClick,
    /// <summary>A double-click in a page's bottom margin.</summary>
    FooterAreaDoubleClick,
    /// <summary>A double-click in the body while a header or footer is being edited.</summary>
    BodyDoubleClickInHeaderFooter,
    /// <summary>A double-click on text.</summary>
    TextDoubleClick,
    /// <summary>A triple-click on text.</summary>
    TextTripleClick,
    /// <summary>A Ctrl+click on text: selects the sentence, or, with text selected, adds the sentence to the selection, or takes it out when it is selected.</summary>
    TextCtrlClick,
    /// <summary>A Ctrl+click on a link, or a click when read only.</summary>
    LinkActivate,
    /// <summary>A click in the left margin.</summary>
    MarginClick,
    /// <summary>A double-click in the left margin.</summary>
    MarginDoubleClick,
    /// <summary>A double-click on a table column border.</summary>
    TableBorderDoubleClick,
    /// <summary>Dragging a table border.</summary>
    TableBorderDrag,
    /// <summary>Dragging a picture or text box, or its handles.</summary>
    ObjectDrag,
    /// <summary>Dragging selected text.</summary>
    TextDrag,
    /// <summary>A double-click on an element in design mode.</summary>
    ElementDoubleClick,
    /// <summary>A click on an unfilled picture control in editing and read-only modes: activates it.</summary>
    PictureControlClick,
    /// <summary>A double-click on a picture control in editing and read-only modes: activates it.</summary>
    PictureControlDoubleClick,
    /// <summary>A click on an empty spot of the horizontal ruler.</summary>
    RulerClick,
    /// <summary>Dragging a ruler marker or boundary.</summary>
    RulerDrag,
    /// <summary>Dragging a tab stop off the ruler.</summary>
    RulerDragOff,
    /// <summary>A double-click on the ruler.</summary>
    RulerDoubleClick,
    /// <summary>A right-click, or the context-menu key.</summary>
    ContextClick,
    /// <summary>A double-click on a picture, shape or text box.</summary>
    ObjectDoubleClick,
    /// <summary>Dragging an object's sizing handle.</summary>
    ObjectResizeDrag,
    /// <summary>Dragging a shape's adjustment or rotation handle.</summary>
    ShapeHandleDrag,
    /// <summary>Dragging a table's move handle.</summary>
    TableMoveDrag,
    /// <summary>A click in a cell's, row's or column's selection zone.</summary>
    TableZoneClick,
    /// <summary>A click on a table's insert mark.</summary>
    TableInsertClick,
    /// <summary>A double-click on a note reference or a note's number.</summary>
    NoteDoubleClick,
    /// <summary>A double-click in blank space with Click and Type on.</summary>
    ClickAndType,
    /// <summary>A double-click between pages, or on their meeting line.</summary>
    PageGapDoubleClick,
    /// <summary>A click on an enabled drop-down form field where forms are filled in: the host opens its list.</summary>
    FormFieldClick,
    /// <summary>A double-click on a legacy form field outside forms protection: the host opens its options.</summary>
    FormFieldDoubleClick,
}

/// <summary>What a gesture does: its default, an event, nothing, or an operation.</summary>
[JsonConverter(typeof(GestureActionConverter))]
public sealed class GestureAction
{
    private GestureAction(string kind, EditorOperation? operation) => (Kind, Operation) = (kind, operation);

    internal string Kind { get; }

    /// <summary>The operation, for <see cref="Run"/>.</summary>
    public EditorOperation? Operation { get; }

    /// <summary>The engine's default action.</summary>
    public static GestureAction Default { get; } = new("default", null);

    /// <summary>Run nothing; raise <see cref="EditorSession.GestureRequested"/>.</summary>
    public static GestureAction Emit { get; } = new("emit", null);

    /// <summary>Ignore the gesture.</summary>
    public static GestureAction None { get; } = new("none", null);

    /// <summary>Run an operation instead.</summary>
    /// <param name="operation">The operation.</param>
    public static GestureAction Run(EditorOperation operation) => new("op", operation);

    /// <inheritdoc/>
    public override string ToString() => Operation?.ToString() ?? Kind;
}

/// <summary>A gesture's binding.</summary>
/// <param name="Gesture">The gesture.</param>
/// <param name="Action">What it does.</param>
/// <param name="Label">A description, such as "Double-click the header area".</param>
public sealed record GestureBinding(GestureId Gesture, GestureAction Action, string Label);

/// <summary>A non-content area a gesture can hit.</summary>
public enum GestureArea
{
    /// <summary>A page's header area.</summary>
    HeaderArea,
    /// <summary>A page's footer area.</summary>
    FooterArea,
    /// <summary>The left margin.</summary>
    Margin,
    /// <summary>A ruler.</summary>
    Ruler,
    /// <summary>A table border.</summary>
    TableBorder,
    /// <summary>Between pages, or their meeting line while white space is hidden.</summary>
    PageGap,
}

/// <summary>What is under a gesture: content (as a menu target) or an area.</summary>
/// <param name="Content">The content, when the gesture hit some.</param>
/// <param name="Area">The area, when the gesture hit one.</param>
[JsonConverter(typeof(GestureTargetConverter))]
public sealed record GestureTarget(MenuTarget? Content, GestureArea? Area);

/// <summary>The engine's pointer gestures and their bindings. They apply while attached.</summary>
public sealed class EditorGestures
{
    private readonly EditorSession _session;

    internal EditorGestures(EditorSession session) => _session = session;

    /// <summary>Every gesture's binding. Reading it waits for the operations queued before it.</summary>
    public IReadOnlyList<GestureBinding> Bindings => _session.Call<IReadOnlyList<GestureBinding>>(Method.GesturesBindings, []);

    /// <summary>Changes what a gesture does.</summary>
    /// <param name="gesture">The gesture.</param>
    /// <param name="action">What it does now.</param>
    /// <returns>Completes when the binding applies.</returns>
    public Task SetAsync(GestureId gesture, GestureAction action) =>
        _session.CallAsync(Method.GesturesSet, WireJson.ToUtf8(new GestureSetRequest(gesture, action)), default);

    /// <summary>Restores a gesture's default, or every gesture's.</summary>
    /// <param name="gesture">The gesture; all of them when null.</param>
    /// <returns>Completes when the bindings apply.</returns>
    public Task ResetAsync(GestureId? gesture = null) => _session.CallAsync(Method.GesturesReset, gesture is { } g ? WireJson.ToUtf8(g) : [], default);
}

/// <summary>What a key does.</summary>
public enum ShortcutKind
{
    /// <summary>Runs a command.</summary>
    Command,
    /// <summary>Raises an application shortcut.</summary>
    Shortcut,
    /// <summary>Raises a host command.</summary>
    Host,
}

/// <summary>What a key does now.</summary>
/// <param name="Kind">What kind of binding.</param>
/// <param name="Command">The command, for <see cref="ShortcutKind.Command"/>.</param>
/// <param name="Id">The shortcut id, for <see cref="ShortcutKind.Shortcut"/>.</param>
/// <param name="Host">The host command, for <see cref="ShortcutKind.Host"/>.</param>
[JsonConverter(typeof(ShortcutBindingConverter))]
public sealed record ShortcutBinding(ShortcutKind Kind, CommandId? Command = null, string? Id = null, HostCommand? Host = null);

/// <summary>Application key shortcuts.</summary>
public sealed class EditorShortcuts
{
    private readonly EditorSession _session;

    internal EditorShortcuts(EditorSession session) => _session = session;

    /// <summary>Claims a key for the application: pressing it raises <see cref="EditorSession.ShortcutPressed"/>.</summary>
    /// <param name="id">The shortcut's id.</param>
    /// <param name="windowsKeys">The keys on Windows, such as <c>Ctrl+Shift+L</c>.</param>
    /// <param name="macKeys">The keys on macOS.</param>
    /// <returns>Unregisters the shortcut when disposed.</returns>
    /// <exception cref="EditorException">The keys collide with a command or another shortcut (<see cref="EditorErrorCode.Unavailable"/>).</exception>
    /// <remarks>It waits for the operations queued before it, so a collision throws here.</remarks>
    public IDisposable Register(string id, string windowsKeys, string? macKeys = null)
    {
        ArgumentNullException.ThrowIfNull(id);
        ArgumentNullException.ThrowIfNull(windowsKeys);
        _session.Call(Method.ShortcutsRegister, WireJson.ToUtf8(new Shortcut(id, new ShortcutKeys(windowsKeys, macKeys))));
        return new Registration(_session, id);
    }

    /// <summary>What a key does now.</summary>
    /// <param name="keys">The keys, such as <c>Ctrl+B</c>.</param>
    /// <returns>The binding, or null when the key does nothing.</returns>
    public ShortcutBinding? Lookup(string keys) => _session.Call<ShortcutBinding?>(Method.ShortcutsLookup, WireJson.ToUtf8(keys));

    private sealed class Registration(EditorSession session, string id) : IDisposable
    {
        private int _disposed;

        public void Dispose()
        {
            if (Interlocked.Exchange(ref _disposed, 1) == 0 && !session.IsDisposed)
                _ = session.CallAsync(Method.ShortcutsUnregister, WireJson.ToUtf8(id), default);
        }
    }
}

/// <summary>A footnote or endnote, as a notes list shows it.</summary>
/// <param name="Kind">Footnote or endnote.</param>
/// <param name="Id">Its id as the document numbers it: stable while the document is open.</param>
/// <param name="Mark">Its reference mark: its custom mark, else its number as laid out (empty before layout reaches it).</param>
/// <param name="CustomMark">Its mark is a custom one.</param>
/// <param name="Text">Its text, plain, at most 1,024 characters and then an ellipsis.</param>
/// <param name="Page">The page showing its reference; null while nothing refers to it or layout hasn't reached it.</param>
public sealed record NoteInfo(NoteType Kind, long Id, string Mark, bool CustomMark, string Text, [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] int? Page = null);

/// <summary>One note separator's text.</summary>
/// <param name="Text">Its plain text, paragraphs <c>\n</c> apart (empty for the built-in line).</param>
/// <param name="Builtin">It is the built-in one: a line.</param>
public sealed record NoteSeparatorState(string Text, bool Builtin);

/// <summary>The separators of footnotes or endnotes.</summary>
/// <param name="Separator">Above the notes on the page they start.</param>
/// <param name="ContinuationSeparator">Above the notes continued from the page before.</param>
/// <param name="ContinuationNotice">Below notes that continue on the next page; null when there is none.</param>
public sealed record NoteSeparators(NoteSeparatorState Separator, NoteSeparatorState ContinuationSeparator, [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] NoteSeparatorState? ContinuationNotice = null);

/// <summary>An editable range, as Restrict Editing lists it: the permissions over one stretch of the document.</summary>
/// <param name="Ids">Its permissions' ids, one per editor.</param>
/// <param name="Editors">Who may edit it.</param>
/// <param name="Page">The page it starts on; null before layout reaches it.</param>
public sealed record EditableRangeInfo(IReadOnlyList<string> Ids, IReadOnlyList<RangeEditorRef> Editors, [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] int? Page = null);

/// <summary>The columns a column preset gives a section, for <see cref="EditorCommands.SetColumnsAsync"/>.</summary>
/// <param name="Count">The number of columns.</param>
/// <param name="Spacing">The space between columns.</param>
/// <param name="Separator">A line is drawn between columns.</param>
/// <param name="Widths">Columns of their own widths; null for equal ones.</param>
/// <param name="ApplyTo">Where they apply.</param>
/// <param name="StartNewColumn">The text from the selection on starts a new column.</param>
public sealed record ColumnSettings(
    int Count,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] Length? Spacing = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? Separator = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] IReadOnlyList<ColumnWidth>? Widths = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] ColumnsApplyTo? ApplyTo = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? StartNewColumn = null);

/// <summary>Insert Field's categories.</summary>
public enum FieldCategory
{
    /// <summary>Date and Time.</summary>
    DateAndTime,
    /// <summary>Document Automation.</summary>
    DocumentAutomation,
    /// <summary>Document Information.</summary>
    DocumentInformation,
    /// <summary>Equations and Formulas.</summary>
    EquationsAndFormulas,
    /// <summary>Index and Tables.</summary>
    IndexAndTables,
    /// <summary>Links and References.</summary>
    LinksAndReferences,
    /// <summary>Mail Merge.</summary>
    MailMerge,
    /// <summary>Numbering.</summary>
    Numbering,
    /// <summary>User Information.</summary>
    UserInformation,
}

/// <summary>What a field argument names.</summary>
public enum FieldArgKind
{
    /// <summary>Any text.</summary>
    Text,
    /// <summary>A bookmark of the document.</summary>
    Bookmark,
    /// <summary>A style's name.</summary>
    Style,
    /// <summary>A document property's name.</summary>
    Property,
    /// <summary>A document variable's name.</summary>
    Variable,
    /// <summary>A file name or address.</summary>
    File,
    /// <summary>A sequence or list name, an entry or a merge field's name.</summary>
    Name,
    /// <summary>An expression or a comparison.</summary>
    Expression,
    /// <summary>A whole number.</summary>
    Number,
}

/// <summary>What a field switch takes after it.</summary>
public enum SwitchArgKind
{
    /// <summary>Nothing: it is on or off.</summary>
    None,
    /// <summary>Text.</summary>
    Text,
    /// <summary>A whole number.</summary>
    Number,
    /// <summary>A bookmark.</summary>
    Bookmark,
    /// <summary>A style's name.</summary>
    Style,
    /// <summary>A sequence or caption label's name.</summary>
    Name,
    /// <summary>Heading or outline levels.</summary>
    Levels,
}

/// <summary>An argument a field takes.</summary>
/// <param name="Name">Its name.</param>
/// <param name="Kind">What it names.</param>
/// <param name="Required">The field needs it.</param>
public sealed record FieldArgInfo(string Name, FieldArgKind Kind, bool Required);

/// <summary>A switch of a field's own.</summary>
/// <param name="Flag">The letter (or <c>!</c>) after the backslash.</param>
/// <param name="Arg">What it takes after it.</param>
/// <param name="Description">What it does.</param>
public sealed record FieldSwitchInfo(string Flag, SwitchArgKind Arg, string Description);

/// <summary>The general switches a field takes.</summary>
/// <param name="Text"><c>\*</c>: text and number formats.</param>
/// <param name="Numeric"><c>\#</c>: a numeric picture.</param>
/// <param name="Date"><c>\@</c>: a date-time picture.</param>
public sealed record FieldGeneralSwitches(bool Text, bool Numeric, bool Date);

/// <summary>A field as Insert Field describes it.</summary>
/// <param name="Name">Its name as a code writes it.</param>
/// <param name="Categories">The categories it is listed under.</param>
/// <param name="Description">What it does.</param>
/// <param name="Syntax">Its syntax, as Insert Field shows it under Field codes.</param>
/// <param name="Args">Its arguments, in order.</param>
/// <param name="Switches">Its own switches.</param>
/// <param name="Formats">The general switches it takes.</param>
/// <param name="Evaluated">The engine computes its result.</param>
public sealed record FieldDescription(
    string Name, IReadOnlyList<FieldCategory> Categories, string Description, string Syntax,
    IReadOnlyList<FieldArgInfo> Args, IReadOnlyList<FieldSwitchInfo> Switches, FieldGeneralSwitches Formats, bool Evaluated);

/// <summary>A format a <c>\*</c> switch takes, and its name.</summary>
/// <param name="Value">As the code writes it.</param>
/// <param name="Label">Its name in Insert Field.</param>
public sealed record FieldFormatOption(string Value, string Label);

/// <summary>The formats Insert Field offers.</summary>
/// <param name="TextFormats"><c>\*</c> text formats.</param>
/// <param name="NumberFormats"><c>\*</c> number formats.</param>
/// <param name="NumericPictures"><c>\#</c> numeric pictures.</param>
/// <param name="DatePictures"><c>\@</c> date-time pictures.</param>
public sealed record FieldFormats(
    IReadOnlyList<FieldFormatOption> TextFormats, IReadOnlyList<FieldFormatOption> NumberFormats, IReadOnlyList<string> NumericPictures, IReadOnlyList<string> DatePictures);

/// <summary>A field switch chosen, with what follows it.</summary>
/// <param name="Flag">The letter (or <c>!</c>) after the backslash.</param>
/// <param name="Value">What follows it.</param>
public sealed record FieldSwitchChoice(string Flag, [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? Value = null);

/// <summary>What a field code is made of: Insert Field's choices.</summary>
/// <param name="Name">The field's name.</param>
public sealed record FieldChoice(string Name)
{
    /// <summary>Its arguments, in order.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public IReadOnlyList<string> Args { get; init; } = [];

    /// <summary>Its own switches.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public IReadOnlyList<FieldSwitchChoice> Switches { get; init; } = [];

    /// <summary><c>\*</c> formats other than MERGEFORMAT.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public IReadOnlyList<string> Formats { get; init; } = [];

    /// <summary><c>\#</c>: a numeric picture.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public string? NumericPicture { get; init; }

    /// <summary><c>\@</c>: a date-time picture.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public string? DatePicture { get; init; }

    /// <summary><c>\* MERGEFORMAT</c>: the result keeps its formatting through updates.</summary>
    public bool PreserveFormatting { get; init; }
}

/// <summary>What is wrong with a field code, as Insert Field flags it.</summary>
[JsonPolymorphic(TypeDiscriminatorPropertyName = "kind")]
[JsonDerivedType(typeof(Empty), "empty")]
[JsonDerivedType(typeof(UnknownField), "unknownField")]
[JsonDerivedType(typeof(MissingArgument), "missingArgument")]
[JsonDerivedType(typeof(UnknownSwitch), "unknownSwitch")]
[JsonDerivedType(typeof(MissingSwitch), "missingSwitch")]
[JsonDerivedType(typeof(MissingSwitchValue), "missingSwitchValue")]
[JsonDerivedType(typeof(NotANumber), "notANumber")]
[JsonDerivedType(typeof(FormatNotTaken), "formatNotTaken")]
public abstract record FieldProblem
{
    private protected FieldProblem() { }

    /// <summary>No field name.</summary>
    public sealed record Empty : FieldProblem;

    /// <summary>A field Word has no such name for.</summary>
    /// <param name="Name">The name.</param>
    public sealed record UnknownField(string Name) : FieldProblem;

    /// <summary>A required argument left out.</summary>
    /// <param name="Name">The argument's name.</param>
    public sealed record MissingArgument(string Name) : FieldProblem;

    /// <summary>A switch the field doesn't take.</summary>
    /// <param name="Flag">The switch's flag.</param>
    public sealed record UnknownSwitch(string Flag) : FieldProblem;

    /// <summary>A switch the field needs left out.</summary>
    /// <param name="Flag">The switch's flag.</param>
    public sealed record MissingSwitch(string Flag) : FieldProblem;

    /// <summary>A switch that takes a value given none.</summary>
    /// <param name="Flag">The switch's flag.</param>
    public sealed record MissingSwitchValue(string Flag) : FieldProblem;

    /// <summary>A switch that takes a whole number given something else.</summary>
    /// <param name="Flag">The switch's flag.</param>
    public sealed record NotANumber(string Flag) : FieldProblem;

    /// <summary>A general switch the field doesn't take.</summary>
    /// <param name="Flag">The switch's flag: <c>*</c>, <c>#</c> or <c>@</c>.</param>
    public sealed record FormatNotTaken(string Flag) : FieldProblem;
}

/// <summary>A field code made from choices, and its problems.</summary>
/// <param name="Code">The code, with Word's spaces around it.</param>
/// <param name="Problems">What is wrong with it; empty when Insert Field takes it as it is.</param>
public sealed record ComposedField(string Code, IReadOnlyList<FieldProblem> Problems);

/// <summary>The field the selection is in, and its code's choices.</summary>
/// <param name="Id">The field.</param>
/// <param name="Choice">Its code's choices.</param>
public sealed record FieldHere(ElementId Id, FieldChoice Choice);

/// <summary>A page thumbnail's drawn size, in pixels, centred in the buffer.</summary>
/// <param name="Width">The page's width.</param>
/// <param name="Height">The page's height.</param>
public sealed record ThumbnailSize(int Width, int Height);
