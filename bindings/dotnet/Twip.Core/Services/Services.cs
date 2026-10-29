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
    /// <returns>The matches in document order.</returns>
    public Task<IReadOnlyList<FindMatch>> FindAllAsync(FindQuery query, CancellationToken ct = default) =>
        _session.CallAsync<IReadOnlyList<FindMatch>>(Method.FindAll, WireJson.ToUtf8(query), ct);

    /// <summary>Selects and reveals a match from <see cref="FindAllAsync"/>.</summary>
    /// <param name="match">The match.</param>
    /// <param name="ct">Cancels the request.</param>
    /// <returns>Completes when it is selected.</returns>
    public Task SelectAsync(MatchId match, CancellationToken ct = default) => _session.CallAsync(Method.SelectMatch, WireJson.ToUtf8(match), ct);

    /// <summary>Replaces the current match, then finds the next.</summary>
    /// <param name="query">What to find.</param>
    /// <param name="replacement">The replacement text.</param>
    /// <param name="ct">Cancels before replacing.</param>
    /// <returns>The result of finding the next match.</returns>
    public Task<FindResult> ReplaceAsync(FindQuery query, string replacement, CancellationToken ct = default) =>
        _session.CallAsync<FindResult>(Method.Replace, WireJson.ToUtf8(new ReplaceRequest(query, replacement)), ct);

    /// <summary>Replaces every match in one transaction.</summary>
    /// <param name="query">What to find.</param>
    /// <param name="replacement">The replacement text.</param>
    /// <param name="ct">Cancels before replacing.</param>
    /// <returns>The number of replacements.</returns>
    public async Task<int> ReplaceAllAsync(FindQuery query, string replacement, CancellationToken ct = default) =>
        (await _session.CallAsync<ReplaceAllResult>(Method.ReplaceAll, WireJson.ToUtf8(new ReplaceRequest(query, replacement)), ct).ConfigureAwait(false)).Count;
}

/// <summary>What to find.</summary>
/// <param name="Text">The text.</param>
/// <param name="MatchCase">Match case.</param>
/// <param name="WholeWords">Match whole words only.</param>
public sealed record FindQuery(string Text, bool MatchCase = false, bool WholeWords = false);

/// <summary>The result of a find.</summary>
/// <param name="Found">Whether a match was found.</param>
/// <param name="Wrapped">Whether the search wrapped around the document.</param>
public sealed record FindResult(bool Found, bool Wrapped);

/// <summary>A find match, with context for a results list.</summary>
/// <param name="Id">The match.</param>
/// <param name="Before">Text before the match.</param>
/// <param name="Text">The matched text.</param>
/// <param name="After">Text after the match.</param>
/// <param name="Page">The page.</param>
public sealed record FindMatch(MatchId Id, string Before, string Text, string After, int Page);

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

/// <summary>Headings, for a navigation pane.</summary>
public sealed class EditorNavigation
{
    private readonly EditorSession _session;

    internal EditorNavigation(EditorSession session) => _session = session;

    /// <summary>The document's headings.</summary>
    /// <param name="ct">Cancels the request.</param>
    /// <returns>The headings in document order.</returns>
    public Task<IReadOnlyList<Heading>> GetHeadingsAsync(CancellationToken ct = default) => _session.CallAsync<IReadOnlyList<Heading>>(Method.Headings, [], ct);

    /// <summary>Selects and reveals a heading.</summary>
    /// <param name="heading">The heading.</param>
    /// <param name="ct">Cancels the request.</param>
    /// <returns>Completes when it is selected.</returns>
    public Task SelectAsync(HeadingId heading, CancellationToken ct = default) => _session.CallAsync(Method.SelectHeading, WireJson.ToUtf8(heading), ct);
}

/// <summary>A heading.</summary>
/// <param name="Id">The heading.</param>
/// <param name="Level">Its outline level.</param>
/// <param name="Text">Its text.</param>
/// <param name="Page">Its page.</param>
public sealed record Heading(HeadingId Id, int Level, string Text, int Page);

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
public sealed record StyleInfo(
    StyleId Id, string Name, StyleType Type, bool BuiltIn, bool Quick, int Priority, bool Hidden, StyleId? BasedOn, StylePreview Preview);

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

    /// <summary>The body, or an element's content, as blocks.</summary>
    /// <param name="root">The element; by default the body.</param>
    /// <param name="depth">How deep to go.</param>
    /// <param name="ct">Cancels the request.</param>
    /// <returns>The blocks.</returns>
    public Task<IReadOnlyList<Block>> GetStructureAsync(ElementId? root = null, int? depth = null, CancellationToken ct = default) =>
        _session.CallAsync<IReadOnlyList<Block>>(Method.Structure, WireJson.ToUtf8(new StructureRequest(root, depth)), ct);

    /// <summary>Selects an element, entering a header or footer when needed.</summary>
    /// <param name="id">The element.</param>
    /// <param name="reveal">Scroll it into view.</param>
    /// <param name="ct">Cancels the request.</param>
    /// <returns>Completes when it is selected.</returns>
    public Task SelectAsync(ElementId id, bool reveal = true, CancellationToken ct = default) =>
        _session.CallAsync(Method.SelectElement, WireJson.ToUtf8(new SelectElementRequest(id, reveal)), ct);

    /// <summary>The unfilled picture placeholders, in document order (the headers and footers after the body).</summary>
    /// <param name="ct">Cancels the request.</param>
    /// <returns>The placeholders.</returns>
    public Task<IReadOnlyList<PlaceholderInfo>> PlaceholdersAsync(CancellationToken ct = default) =>
        _session.CallAsync<IReadOnlyList<PlaceholderInfo>>(Method.Placeholders, [], ct);
}

/// <summary>An unfilled picture placeholder.</summary>
/// <param name="Key">Its key, what filling it takes.</param>
/// <param name="Element">The control it is in.</param>
public sealed record PlaceholderInfo(string Key, ElementId? Element);

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
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] Story? Story = null);

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
    string Display, Story Story, ElementId? Parent, bool Locked);

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
    public sealed record Paragraph(int TextLength, IReadOnlyList<ElementId> Elements) : Block;

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
}

/// <summary>An accessible node's role.</summary>
public enum AccessibleRole
{
    /// <summary>The document.</summary>
    Document,
    /// <summary>A page.</summary>
    Page,
    /// <summary>A paragraph.</summary>
    Paragraph,
    /// <summary>A heading.</summary>
    Heading,
    /// <summary>A list.</summary>
    List,
    /// <summary>A list item.</summary>
    ListItem,
    /// <summary>A table.</summary>
    Table,
    /// <summary>A row.</summary>
    Row,
    /// <summary>A cell.</summary>
    Cell,
    /// <summary>An image.</summary>
    Image,
    /// <summary>A link.</summary>
    Link,
    /// <summary>A text box.</summary>
    TextBox,
    /// <summary>A footnote or endnote.</summary>
    Note,
    /// <summary>A header.</summary>
    Header,
    /// <summary>A footer.</summary>
    Footer,
}

/// <summary>A mark on accessible text.</summary>
public enum AccessibleMark
{
    /// <summary>An insertion.</summary>
    Insertion,
    /// <summary>A deletion.</summary>
    Deletion,
    /// <summary>A comment.</summary>
    Comment,
    /// <summary>A misspelling.</summary>
    Misspelling,
}

/// <summary>A node of the accessible tree.</summary>
/// <param name="Role">Its role.</param>
/// <param name="Name">Its name.</param>
/// <param name="Text">Its text.</param>
/// <param name="Level">A heading's level.</param>
/// <param name="Marks">Marks on it.</param>
/// <param name="Children">Its children.</param>
public sealed record AccessibleNode(
    AccessibleRole Role,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? Name,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? Text,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] int? Level,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] IReadOnlyList<AccessibleMark>? Marks,
    IReadOnlyList<AccessibleNode> Children);

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
    /// <summary>A Ctrl+click on text.</summary>
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
    /// <summary>A click on an unfilled picture placeholder.</summary>
    PlaceholderClick,
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
