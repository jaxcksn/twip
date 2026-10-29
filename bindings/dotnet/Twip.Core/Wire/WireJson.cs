using System.Text.Json;
using System.Text.Json.Nodes;
using System.Text.Json.Serialization.Metadata;
using Twip.Native;

namespace Twip.Wire;

/// <summary>Encoding and decoding through <see cref="TwipJsonContext"/>.</summary>
internal static class WireJson
{
    public static JsonTypeInfo<T> Info<T>() =>
        (JsonTypeInfo<T>?)TwipJsonContext.Default.GetTypeInfo(typeof(T)) ?? throw new InvalidOperationException($"{typeof(T).Name} is not a wire type");

    public static JsonNode? ToNode<T>(T value) => JsonSerializer.SerializeToNode(value, Info<T>());

    public static byte[] ToUtf8<T>(T value) => JsonSerializer.SerializeToUtf8Bytes(value, Info<T>());

    public static void WriteNumber(Utf8JsonWriter writer, double value)
    {
        if (value == Math.Floor(value) && Math.Abs(value) < 1e15) writer.WriteNumberValue((long)value);
        else writer.WriteNumberValue(value);
    }

    /// <summary>Reads a response: <c>{"ok": value}</c> gives the value; <c>{"error": {...}}</c> throws.</summary>
    public static JsonElement? Unwrap(ReadOnlySpan<byte> response, int status)
    {
        if (response.IsEmpty)
        {
            if (status == Status.Ok) return null;
            // A status is the negated error code, counting from 1 in EditorErrorCode's order.
            var code = status is >= -10 and <= -1 ? (EditorErrorCode)(-status - 1) : EditorErrorCode.Internal;
            throw new EditorException(code, $"the engine failed with status {status}");
        }
        using var doc = JsonDocument.Parse(response.ToArray());
        var root = doc.RootElement;
        if (root.TryGetProperty("error", out var error)) throw ToException(error);
        return root.TryGetProperty("ok", out var ok) ? ok.Clone() : null;
    }

    public static EditorException ToException(JsonElement error)
    {
        var body = error.Deserialize(TwipJsonContext.Default.ErrorBody)!;
        return new EditorException(body.Code, body.Message, body.Details);
    }
}

/// <summary>Members outside what this version implements.</summary>
internal static class Stage
{
    public static Task Unsupported(string what) => Task.FromException(EditorException.Unsupported(what));

    public static Task<T> Unsupported<T>(string what) => Task.FromException<T>(EditorException.Unsupported(what));
}

/// <summary>Reads a commit's value: <c>{"record": ..., "value": ...}</c>.</summary>
internal static class CommitValue
{
    public static JsonElement? Of(JsonElement? commit) =>
        commit is { ValueKind: JsonValueKind.Object } c && c.TryGetProperty("value", out var v) && v.ValueKind != JsonValueKind.Null ? v : null;

    private static JsonElement Field(JsonElement? commit, string name) =>
        Of(commit) is { } v && v.TryGetProperty(name, out var f) ? f : throw new EditorException(EditorErrorCode.Internal, $"the engine's commit has no {name}");

    public static ElementId ElementId(JsonElement? commit) => new(Field(commit, "id").GetString()!);

    public static CommentId CommentId(JsonElement? commit) => new(Field(commit, "id").GetString()!);

    public static int Changed(JsonElement? commit) => Field(commit, "changed").GetInt32();

    public static string ItemId(JsonElement? commit) => Field(commit, "itemId").GetString()!;
}
