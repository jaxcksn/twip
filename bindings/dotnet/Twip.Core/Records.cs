using System.Text.Json.Serialization;
using Twip.Wire;

namespace Twip;

/// <summary>Options for a new <see cref="EditorSession"/>.</summary>
/// <param name="Author">The author of edits, revisions and comments; by default the signed-in user's name.</param>
/// <param name="ReadOnly">Start read only.</param>
/// <param name="Mode">Start in editing or design mode.</param>
/// <param name="Fonts">The font library; by default <see cref="FontLibrary.System"/>.</param>
/// <param name="Language">The BCP 47 language for text with no language.</param>
/// <param name="Dictionaries">Spelling dictionaries to load.</param>
/// <param name="TrackChanges">Turn change tracking on after open, unless the document locks it.</param>
public sealed record EditorSessionOptions(
    Author? Author = null, bool ReadOnly = false, EditorMode Mode = EditorMode.Editing, FontLibrary? Fonts = null,
    string Language = "en-US", IReadOnlyList<Dictionary>? Dictionaries = null, bool TrackChanges = false);

/// <summary>The person making edits.</summary>
/// <param name="Name">The display name.</param>
/// <param name="Initials">The initials shown on comments.</param>
/// <param name="Identity">The identity from a sign-in provider.</param>
public sealed record Author(
    string Name,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? Initials = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] Identity? Identity = null);

/// <summary>A sign-in identity.</summary>
/// <param name="Provider">The provider, such as <c>AD</c>.</param>
/// <param name="UserId">The user id at the provider.</param>
public sealed record Identity(string Provider, string UserId);

/// <summary>A Hunspell spelling dictionary.</summary>
/// <param name="Language">The BCP 47 language it checks.</param>
/// <param name="Aff">The <c>.aff</c> file.</param>
/// <param name="Dic">The <c>.dic</c> file.</param>
public sealed record Dictionary(string Language, ReadOnlyMemory<byte> Aff, ReadOnlyMemory<byte> Dic);

/// <summary>The result of an open.</summary>
/// <param name="Warnings">What the import approximated.</param>
public sealed record ImportResult(IReadOnlyList<Warning> Warnings);

/// <summary>The result of a save or export.</summary>
/// <param name="Revision">The document revision that was written.</param>
/// <param name="Warnings">What the export approximated.</param>
public sealed record SaveResult(long Revision, IReadOnlyList<Warning> Warnings);

/// <summary>Editing or design mode.</summary>
public enum EditorMode
{
    /// <summary>Normal editing.</summary>
    Editing,
    /// <summary>The merge designer: fields and controls show as labelled elements.</summary>
    Design,
}

/// <summary>Where a committed change came from.</summary>
public enum Origin
{
    /// <summary>The end user's input.</summary>
    User,
    /// <summary>An API call.</summary>
    Api,
    /// <summary>An undo.</summary>
    Undo,
    /// <summary>A redo.</summary>
    Redo,
}

/// <summary>One committed transaction.</summary>
/// <param name="Id">The transaction id.</param>
/// <param name="Revision">The document revision it produced.</param>
/// <param name="Origin">Where it came from.</param>
/// <param name="Author">Who made it.</param>
/// <param name="Time">When it committed (UTC).</param>
/// <param name="Label">The undo label.</param>
/// <param name="Summary">What it touched.</param>
public sealed record TransactionRecord(
    long Id, long Revision, Origin Origin, Author Author,
    [property: JsonConverter(typeof(UtcTimeConverter))] DateTimeOffset Time, string Label, ChangeSummary Summary);

/// <summary>What a transaction touched.</summary>
/// <param name="Paragraphs">The number of paragraphs changed.</param>
/// <param name="Tables">The number of tables changed.</param>
/// <param name="Elements">The elements changed.</param>
/// <param name="Revisions">The revisions changed.</param>
/// <param name="Comments">The comments changed.</param>
public sealed record ChangeSummary(
    int Paragraphs, int Tables, IReadOnlyList<ElementId> Elements, IReadOnlyList<RevisionId> Revisions,
    IReadOnlyList<CommentId> Comments);

/// <summary>A stable error code.</summary>
[JsonConverter(typeof(KebabEnumConverter<EditorErrorCode>))]
public enum EditorErrorCode
{
    /// <summary>The session was disposed.</summary>
    Destroyed,
    /// <summary>An argument is out of range, or an id doesn't exist.</summary>
    InvalidArgument,
    /// <summary>The selection or document state doesn't allow it.</summary>
    Unavailable,
    /// <summary>The session is read only.</summary>
    ReadOnly,
    /// <summary>The document couldn't be read.</summary>
    ImportFailed,
    /// <summary>The document couldn't be written.</summary>
    ExportFailed,
    /// <summary>The operation was cancelled.</summary>
    Aborted,
    /// <summary>The platform denied it.</summary>
    NotAllowed,
    /// <summary>Not supported in this version.</summary>
    Unsupported,
    /// <summary>An engine fault.</summary>
    Internal,
}

/// <summary>An expected failure of an editor operation, with a stable <see cref="Code"/>.</summary>
public sealed class EditorException : Exception
{
    internal EditorException(EditorErrorCode code, string message, object? details = null) : base(message)
    {
        Code = code;
        Details = details;
    }

    /// <summary>The stable error code.</summary>
    public EditorErrorCode Code { get; }

    /// <summary>Structured details, when the engine gives some.</summary>
    public object? Details { get; }

    internal static EditorException Unsupported(string what) => new(EditorErrorCode.Unsupported, $"{what} is not supported yet");
}

/// <summary>A kind of import or export approximation.</summary>
[JsonConverter(typeof(KebabEnumConverter<WarningCode>))]
public enum WarningCode
{
    /// <summary>An element the engine doesn't support was kept but not shown.</summary>
    UnsupportedElement,
    /// <summary>An image couldn't be decoded.</summary>
    ImageUndecodable,
    /// <summary>A font couldn't be found.</summary>
    FontUnresolved,
    /// <summary>A font was substituted.</summary>
    FontSubstituted,
    /// <summary>Content was rewritten to be valid.</summary>
    RewrittenContent,
    /// <summary>A PDF used a fallback font.</summary>
    PdfFontFallback,
    /// <summary>A revision had no id.</summary>
    RevisionIdMissing,
    /// <summary>A range start or end had no partner.</summary>
    RangeUnpaired,
    /// <summary>A revision or comment had no author.</summary>
    AuthorMissing,
    /// <summary>A comment had no anchor in the text.</summary>
    CommentUnanchored,
    /// <summary>A comment reference pointed at no comment.</summary>
    CommentReferenceOrphan,
}

/// <summary>An import or export approximation, aggregated per code and part.</summary>
/// <param name="Code">The kind.</param>
/// <param name="Message">A description.</param>
/// <param name="Count">How many times it happened.</param>
/// <param name="Location">Where, when known.</param>
public sealed record Warning(
    WarningCode Code, string Message, int Count,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] WarningLocation? Location = null);

/// <summary>Where a warning happened.</summary>
/// <param name="Part">The package part, such as <c>word/document.xml</c>.</param>
/// <param name="Paragraph">The paragraph index in the part.</param>
public sealed record WarningLocation(
    string Part, [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] int? Paragraph = null);

/// <summary>Document statistics, computed on request.</summary>
/// <param name="Words">Words.</param>
/// <param name="Characters">Characters without spaces.</param>
/// <param name="CharactersWithSpaces">Characters with spaces.</param>
/// <param name="Paragraphs">Paragraphs.</param>
/// <param name="Lines">Lines.</param>
/// <param name="Pages">Pages.</param>
public sealed record WordCount(int Words, int Characters, int CharactersWithSpaces, int Paragraphs, int Lines, int Pages);

/// <summary>Formatted text: paragraphs of runs. A string converts to plain text.</summary>
[JsonConverter(typeof(RichTextConverter))]
public sealed record RichText
{
    /// <summary>Creates formatted text.</summary>
    /// <param name="paragraphs">The paragraphs.</param>
    public RichText(IReadOnlyList<RichParagraph> paragraphs) => Paragraphs = paragraphs;

    /// <summary>The paragraphs.</summary>
    public IReadOnlyList<RichParagraph> Paragraphs { get; }

    /// <summary>Whether this came from a plain string; it then travels as plain text.</summary>
    internal string? Plain { get; private init; }

    /// <summary>Plain text, one paragraph per line.</summary>
    /// <param name="text">The text.</param>
    public static RichText FromText(string text) => new(text.Split('\n').Select(line => new RichParagraph([new RichRun(line.TrimEnd('\r'))])).ToArray()) { Plain = text };

    /// <summary>Plain text, one paragraph per line.</summary>
    /// <param name="text">The text.</param>
    public static implicit operator RichText(string text) => FromText(text);

    /// <summary>The text without formatting, paragraphs separated by newlines.</summary>
    public override string ToString() => Plain ?? string.Join("\n", Paragraphs.Select(p => string.Concat(p.Runs.Select(r => r.Text))));
}

/// <summary>A paragraph of formatted text.</summary>
/// <param name="Runs">The runs.</param>
public sealed record RichParagraph(IReadOnlyList<RichRun> Runs);

/// <summary>A run of formatted text.</summary>
/// <param name="Text">The text.</param>
/// <param name="Bold">Bold.</param>
/// <param name="Italic">Italic.</param>
/// <param name="Underline">Underlined.</param>
/// <param name="Strikethrough">Struck through.</param>
/// <param name="Link">A hyperlink address.</param>
public sealed record RichRun(
    string Text,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? Bold = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? Italic = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? Underline = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? Strikethrough = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? Link = null);

/// <summary>A catalog entry: what an application needs to show a command in its UI.</summary>
/// <param name="Id">The command.</param>
/// <param name="Label">The English label.</param>
/// <param name="Group">The group, for menus and ribbons.</param>
/// <param name="Shortcuts">The key shortcuts per platform.</param>
/// <param name="TakesArguments">Whether the command takes arguments.</param>
/// <param name="Mutates">Whether it changes the document.</param>
/// <param name="Undo">How it groups into undo steps.</param>
public sealed record CommandInfo(
    CommandId Id, string Label, CommandGroup Group, Shortcuts Shortcuts, bool TakesArguments, bool Mutates, UndoGrouping Undo);

/// <summary>Key shortcuts as display strings, such as <c>Ctrl+B</c>.</summary>
/// <param name="Windows">On Windows and Linux.</param>
/// <param name="Mac">On macOS.</param>
public sealed record Shortcuts(IReadOnlyList<string> Windows, IReadOnlyList<string> Mac);

/// <summary>A command group.</summary>
public enum CommandGroup
{
    /// <summary>Typing and deleting.</summary>
    Text,
    /// <summary>Cut, copy and paste.</summary>
    Clipboard,
    /// <summary>Undo and redo.</summary>
    History,
    /// <summary>Selection and navigation.</summary>
    Selection,
    /// <summary>Character formatting.</summary>
    Character,
    /// <summary>Paragraph formatting.</summary>
    Paragraph,
    /// <summary>Tab stops.</summary>
    Tabs,
    /// <summary>Page layout.</summary>
    Page,
    /// <summary>Headers and footers.</summary>
    HeaderFooter,
    /// <summary>Inserting objects.</summary>
    Insert,
    /// <summary>Tables.</summary>
    Table,
    /// <summary>Pictures and text boxes.</summary>
    Object,
    /// <summary>Fields.</summary>
    Field,
    /// <summary>Content controls.</summary>
    Control,
    /// <summary>Data parts.</summary>
    Data,
    /// <summary>Review.</summary>
    Review,
    /// <summary>View settings.</summary>
    View,
}

/// <summary>How a command groups into undo steps.</summary>
public enum UndoGrouping
{
    /// <summary>One undo step.</summary>
    Step,
    /// <summary>Joins consecutive typing in one paragraph.</summary>
    Typing,
    /// <summary>Not an edit: no undo step and no revision.</summary>
    View,
}
