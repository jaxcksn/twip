using System.Drawing;
using Twip.Hosting;
using System.Text.Json;
using System.Text.Json.Serialization;

namespace Twip.Wire;

// Shapes that exist only on the wire: requests, results, and the payloads of engine events.

internal sealed record RulerVisibility(bool Horizontal, bool Vertical);

internal sealed record SessionOptionsWire(
    Author Author, bool ReadOnly, EditorMode Mode, string Language, bool TrackChanges,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] IReadOnlyList<Dictionary>? Dictionaries = null);

internal sealed record EngineOptionsWire(string Fonts);

internal sealed record ExportOptionsWire(string Format);

/// <summary>An export's metadata; the C ABI returns the bytes apart, so <see cref="Bytes"/> is unset there.</summary>
internal sealed record ExportMeta(
    long Revision, IReadOnlyList<Warning> Warnings,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] ReadOnlyMemory<byte>? Bytes = null);

internal sealed record BatchRequest(IReadOnlyList<EditorOperation> Commands, [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? Label);

internal sealed record TimerRequest(uint Timer, double Ms);

internal sealed record ErrorBody(
    EditorErrorCode Code, string Message, [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] JsonElement? Details);

internal sealed record LinkActivatedPayload(string Address, LinkActivation Via);

internal sealed record ContextMenuPayload(ContextMenu Menu, [property: JsonConverter(typeof(ClientPointConverter))] PointF At, MenuInvocation Via);

internal sealed record DialogRequestedPayload(DialogKind Kind);

internal sealed record HostCommandPayload(HostCommand Command);

internal sealed record ShortcutPayload(string Id);

internal sealed record WordAddedPayload(string Word, string Language);

internal sealed record FontsChangedPayload(IReadOnlyList<string> Pending, IReadOnlyList<string> Unresolved);

internal sealed record ElementActivatedPayload(ElementId Id);

internal sealed record ElementsChangedPayload(IReadOnlyList<ElementInfo> Added, IReadOnlyList<ElementId> Removed, IReadOnlyList<ElementInfo> Changed);

internal sealed record FieldsUpdateRequestedPayload(long Token, IReadOnlyList<FieldUpdateRequest> Fields);

internal sealed record CommentAnchorsPayload([property: JsonPropertyName("value")] IReadOnlyList<CommentAnchor> Anchors);

internal sealed record BalloonsLaidOutPayload([property: JsonPropertyName("value")] IReadOnlyList<BalloonPlacement> Placements);

/// <summary>Where the SDK placed an attached comment card, and its leader line.</summary>
internal sealed record BalloonPlacement(CommentId Thread, double Top, double Left, IReadOnlyList<PointF> Leader);

internal sealed record TooltipPayload([property: JsonPropertyName("value")] TooltipWire? Tooltip);

internal sealed record TooltipWire(
    TooltipKind Kind, string Text,
    [property: JsonConverter(typeof(ClientPointConverter))] PointF At, RectangleF Rect);

internal sealed record AnnouncementPayload(string Text);

internal sealed record ErrorPayload(
    EditorErrorCode Code, string Message, [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] JsonElement? Details, bool Fatal);

internal sealed record EmptyPayload;

/// <summary>Something only the view showing a session can do: the host request callback, tagged by <c>type</c>. Positions
/// are content pixels. A type this SDK doesn't know reads as the base record.</summary>
[JsonPolymorphic(TypeDiscriminatorPropertyName = "type", IgnoreUnrecognizedTypeDiscriminators = true)]
[JsonDerivedType(typeof(CursorRequest), "cursor")]
[JsonDerivedType(typeof(CaptureRequest), "capture")]
[JsonDerivedType(typeof(ReleaseRequest), "release")]
[JsonDerivedType(typeof(ClipboardRequest), "clipboard")]
[JsonDerivedType(typeof(CaretRequest), "caret")]
[JsonDerivedType(typeof(ScrollToRequest), "scroll_to")]
[JsonDerivedType(typeof(A11yRequest), "a11y")]
internal record HostRequest;

/// <summary>Show this pointer shape.</summary>
internal sealed record CursorRequest(EditorCursor Cursor) : HostRequest;

/// <summary>Send every pointer event to the view until <see cref="ReleaseRequest"/>.</summary>
internal sealed record CaptureRequest : HostRequest;

/// <summary>End the pointer capture.</summary>
internal sealed record ReleaseRequest : HostRequest;

/// <summary>A cut or copy from the keyboard, for the system clipboard.</summary>
internal sealed record ClipboardRequest(ClipboardContent Data) : HostRequest;

/// <summary>The caret moved: for the IME window and the system caret.</summary>
internal sealed record CaretRequest(double X, double Y, double Height, bool Visible) : HostRequest;

/// <summary>Scroll so this rectangle shows.</summary>
internal sealed record ScrollToRequest(double X, double Y, double Width, double Height) : HostRequest;

/// <summary>The accessibility tree changed: mirror it.</summary>
internal sealed record A11yRequest(AccessibleNode Tree) : HostRequest;

/// <summary>A point in the control, in client pixels: <c>{ clientX, clientY }</c>.</summary>
internal sealed record ClientPoint(double ClientX, double ClientY);

/// <summary>A gesture bound to emit: the <c>gesture</c> event.</summary>
internal sealed record GestureEvent(GestureId Gesture, [property: JsonConverter(typeof(ClientPointConverter))] PointF At, int Page, GestureTarget Target);

/// <summary>What a key press did: <c>handled</c>, <c>unhandled</c>, or a host command or shortcut id in <c>value</c>.</summary>
internal sealed record KeyOutcome(string Type, [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? Value = null);

/// <summary>The drawn frame's document size and caret, for scrollbars and the IME.</summary>
internal sealed record FrameInfo(
    double ContentWidth, double ContentHeight, double CaretX, double CaretY, double CaretHeight, bool CaretVisible, int Pages);

/// <summary>What a command made: an element or comment <c>id</c>, a data part's <c>itemId</c>, or a <c>changed</c> count.</summary>
internal sealed record CommandResult(
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? Id = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? ItemId = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] int? Changed = null);

/// <summary>A command's commit: its record, and what it made.</summary>
internal sealed record Commit(TransactionRecord? Record, CommandResult? Value);

/// <summary>An application shortcut to register.</summary>
internal sealed record Shortcut(string Id, ShortcutKeys Keys);

/// <summary>Keys on every platform (<see cref="Mac"/> null), or one per platform.</summary>
[JsonConverter(typeof(ShortcutKeysConverter))]
internal sealed record ShortcutKeys(string Windows, string? Mac = null);

/// <summary>An open dialog: its values, and each field's state by the values' camelCase keys.</summary>
internal sealed record DialogModel(DialogKind Kind, DialogValues Values, IReadOnlyDictionary<string, FieldInfo> Fields);

/// <summary>A dialog's values record (<see cref="FontDialogValues"/>, ...), tagged on the wire with its kind.</summary>
[JsonConverter(typeof(DialogValuesConverter))]
internal sealed record DialogValues(DialogKind Kind, object Values);

internal sealed record AccessibilityDialogValues(IReadOnlyList<AccessibilityIssue> Issues);

// Service requests and results.

internal sealed record FindRequest(FindQuery Query, NavigationDirection Direction);

internal sealed record ReplaceRequest(FindQuery Query, string With);

internal sealed record ReplaceAllResult(int Count);

internal sealed record UserWordsRequest(string Language, IReadOnlyList<string> Words);

internal sealed record SuggestionsRequest(string Word, [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? Language);

internal sealed record StructureRequest(
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] ElementId? Root,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] int? Depth);

internal sealed record SelectElementRequest(ElementId Id, bool Reveal);

internal sealed record DataPartsRequest(
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? Namespace,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? ItemId);

internal sealed record GestureSetRequest(GestureId Gesture, GestureAction Action);

/// <summary>A dialog update: the model, and a patch of its <c>kind</c> and the changed fields.</summary>
internal sealed record DialogUpdateRequest(DialogModel Model, System.Text.Json.Nodes.JsonObject Patch);
