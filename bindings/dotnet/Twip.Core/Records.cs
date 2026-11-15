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
/// <param name="FieldLabels">What design mode shows for each field code, as SetFieldLabelsAsync sets them, from each open's first layout.</param>
/// <param name="ApplicationFieldTypes">The application's own field types, as SetApplicationFieldTypesAsync sets them, from each open.</param>
/// <param name="SubTemplateRules">Which fields are sub-template references, as SetSubTemplateRulesAsync sets them, from each open.</param>
public sealed record EditorSessionOptions(
    Author? Author = null, bool ReadOnly = false, EditorMode Mode = EditorMode.Editing, FontLibrary? Fonts = null,
    string Language = "en-US", IReadOnlyList<Dictionary>? Dictionaries = null, bool TrackChanges = false,
    IReadOnlyDictionary<string, string>? FieldLabels = null, IReadOnlyList<string>? ApplicationFieldTypes = null,
    IReadOnlyList<ReferenceRule>? SubTemplateRules = null);

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

/// <summary>Where a warning happened.</summary>
/// <param name="Part">The package part, such as <c>word/document.xml</c>.</param>
/// <param name="Paragraph">The paragraph index in the part.</param>
public sealed record WarningLocation(
    string Part, [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] int? Paragraph = null);

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

/// <summary>Key shortcuts as display strings, such as <c>Ctrl+B</c>.</summary>
/// <param name="Windows">On Windows and Linux.</param>
/// <param name="Mac">On macOS.</param>
public sealed record Shortcuts(IReadOnlyList<string> Windows, IReadOnlyList<string> Mac);

