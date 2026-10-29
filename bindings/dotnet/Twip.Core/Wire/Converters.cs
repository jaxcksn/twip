using System.Drawing;
using System.Globalization;
using System.Text.Json;
using System.Text.Json.Nodes;
using System.Text.Json.Serialization;
using System.Text.Json.Serialization.Metadata;

namespace Twip.Wire;

/// <summary>An id that travels as a bare string.</summary>
internal abstract class IdConverter<T> : JsonConverter<T> where T : struct
{
    protected abstract T Create(string value);

    protected abstract string Value(T id);

    public override T Read(ref Utf8JsonReader reader, Type typeToConvert, JsonSerializerOptions options) =>
        Create(reader.GetString() ?? throw new JsonException("an id can't be null"));

    public override void Write(Utf8JsonWriter writer, T value, JsonSerializerOptions options) => writer.WriteStringValue(Value(value));

    public override T ReadAsPropertyName(ref Utf8JsonReader reader, Type typeToConvert, JsonSerializerOptions options) => Read(ref reader, typeToConvert, options);

    public override void WriteAsPropertyName(Utf8JsonWriter writer, T value, JsonSerializerOptions options) => writer.WritePropertyName(Value(value));
}

internal sealed class ElementIdConverter : IdConverter<ElementId>
{
    protected override ElementId Create(string value) => new(value);

    protected override string Value(ElementId id) => id.Value;
}

internal sealed class CommentIdConverter : IdConverter<CommentId>
{
    protected override CommentId Create(string value) => new(value);

    protected override string Value(CommentId id) => id.Value;
}

internal sealed class RevisionIdConverter : IdConverter<RevisionId>
{
    protected override RevisionId Create(string value) => new(value);

    protected override string Value(RevisionId id) => id.Value;
}

internal sealed class HeadingIdConverter : IdConverter<HeadingId>
{
    protected override HeadingId Create(string value) => new(value);

    protected override string Value(HeadingId id) => id.Value;
}

internal sealed class MatchIdConverter : IdConverter<MatchId>
{
    protected override MatchId Create(string value) => new(value);

    protected override string Value(MatchId id) => id.Value;
}

internal sealed class StyleIdConverter : IdConverter<StyleId>
{
    protected override StyleId Create(string value) => new(value);

    protected override string Value(StyleId id) => id.Value;
}

/// <summary>Enums travel camelCase (<c>setBold</c>, <c>noMarkup</c>).</summary>
internal sealed class CamelEnum<T>() : JsonStringEnumConverter<T>(JsonNamingPolicy.CamelCase, allowIntegerValues: false) where T : struct, Enum;

/// <summary>Error and warning codes travel kebab-case (<c>invalid-argument</c>).</summary>
internal sealed class KebabEnumConverter<T>() : JsonStringEnumConverter<T>(JsonNamingPolicy.KebabCaseLower, allowIntegerValues: false) where T : struct, Enum;

internal sealed class LengthConverter : JsonConverter<Length>
{
    public override Length Read(ref Utf8JsonReader reader, Type typeToConvert, JsonSerializerOptions options) => Length.FromPoints(reader.GetDouble());

    public override void Write(Utf8JsonWriter writer, Length value, JsonSerializerOptions options) => WireJson.WriteNumber(writer, value.Points);
}

internal sealed class ColorValueConverter : JsonConverter<ColorValue>
{
    public override ColorValue Read(ref Utf8JsonReader reader, Type typeToConvert, JsonSerializerOptions options) =>
        ColorValue.Parse(reader.GetString() ?? throw new JsonException("a colour can't be null"));

    public override void Write(Utf8JsonWriter writer, ColorValue value, JsonSerializerOptions options) => writer.WriteStringValue(value.ToString());
}

/// <summary><c>T | 'mixed' | null</c>.</summary>
internal sealed class FormatValueConverter<T> : JsonConverter<FormatValue<T>>
{
    public override bool HandleNull => true;

    public override FormatValue<T> Read(ref Utf8JsonReader reader, Type typeToConvert, JsonSerializerOptions options)
    {
        if (reader.TokenType == JsonTokenType.Null) return FormatValue<T>.Unavailable;
        if (reader.TokenType == JsonTokenType.String && reader.ValueTextEquals("mixed")) return FormatValue<T>.Mixed;
        return FormatValue<T>.Uniform(JsonSerializer.Deserialize(ref reader, Info(options))!);
    }

    public override void Write(Utf8JsonWriter writer, FormatValue<T> value, JsonSerializerOptions options)
    {
        switch (value.Kind)
        {
            case FormatValueKind.Uniform: JsonSerializer.Serialize(writer, value.Value, Info(options)); break;
            case FormatValueKind.Mixed: writer.WriteStringValue("mixed"); break;
            default: writer.WriteNullValue(); break;
        }
    }

    private static JsonTypeInfo<T> Info(JsonSerializerOptions options) => (JsonTypeInfo<T>)options.GetTypeInfo(typeof(T));
}

internal sealed class SpacingValueConverter : JsonConverter<SpacingValue>
{
    public override SpacingValue Read(ref Utf8JsonReader reader, Type typeToConvert, JsonSerializerOptions options) =>
        reader.TokenType == JsonTokenType.String && reader.ValueTextEquals("auto") ? SpacingValue.Auto : SpacingValue.FromLength(Length.FromPoints(reader.GetDouble()));

    public override void Write(Utf8JsonWriter writer, SpacingValue value, JsonSerializerOptions options)
    {
        if (value.IsAuto) writer.WriteStringValue("auto");
        else WireJson.WriteNumber(writer, value.Length.Points);
    }
}

internal sealed class RowHeightConverter : JsonConverter<RowHeight>
{
    public override RowHeight Read(ref Utf8JsonReader reader, Type typeToConvert, JsonSerializerOptions options)
    {
        if (reader.TokenType == JsonTokenType.String && reader.ValueTextEquals("auto")) return RowHeight.Auto;
        var obj = JsonElement.ParseValue(ref reader);
        var height = Length.FromPoints(obj.GetProperty("height").GetDouble());
        return obj.GetProperty("rule").GetString() == "exact" ? RowHeight.Exact(height) : RowHeight.AtLeast(height);
    }

    public override void Write(Utf8JsonWriter writer, RowHeight value, JsonSerializerOptions options)
    {
        if (value.IsAuto) { writer.WriteStringValue("auto"); return; }
        writer.WriteStartObject();
        writer.WritePropertyName("height");
        WireJson.WriteNumber(writer, value.Height.Points);
        writer.WriteString("rule", value.Rule == RowHeightRule.Exact ? "exact" : "atLeast");
        writer.WriteEndObject();
    }
}

internal sealed class ZoomLevelConverter : JsonConverter<ZoomLevel>
{
    public override ZoomLevel Read(ref Utf8JsonReader reader, Type typeToConvert, JsonSerializerOptions options)
    {
        if (reader.TokenType == JsonTokenType.Number) return ZoomLevel.FromPercent(reader.GetDouble());
        if (reader.ValueTextEquals("pageWidth")) return ZoomLevel.PageWidth;
        if (reader.ValueTextEquals("wholePage")) return ZoomLevel.WholePage;
        throw new JsonException("not a zoom level");
    }

    public override void Write(Utf8JsonWriter writer, ZoomLevel value, JsonSerializerOptions options)
    {
        if (value.IsPageWidth) writer.WriteStringValue("pageWidth");
        else if (value.IsWholePage) writer.WriteStringValue("wholePage");
        else WireJson.WriteNumber(writer, value.Percent);
    }
}

internal sealed class OutlineLevelConverter : JsonConverter<OutlineLevel>
{
    public override OutlineLevel Read(ref Utf8JsonReader reader, Type typeToConvert, JsonSerializerOptions options) =>
        reader.TokenType == JsonTokenType.String && reader.ValueTextEquals("body") ? OutlineLevel.Body : OutlineLevel.FromLevel(reader.GetInt32());

    public override void Write(Utf8JsonWriter writer, OutlineLevel value, JsonSerializerOptions options)
    {
        if (value.IsBody) writer.WriteStringValue("body");
        else writer.WriteNumberValue(value.Level);
    }
}

/// <summary>ISO 8601 UTC to the second, as the engine writes it: <c>2026-10-27T09:30:00Z</c>.</summary>
internal sealed class UtcTimeConverter : JsonConverter<DateTimeOffset>
{
    public override DateTimeOffset Read(ref Utf8JsonReader reader, Type typeToConvert, JsonSerializerOptions options) =>
        DateTimeOffset.Parse(reader.GetString()!, CultureInfo.InvariantCulture, DateTimeStyles.AssumeUniversal | DateTimeStyles.AdjustToUniversal);

    public override void Write(Utf8JsonWriter writer, DateTimeOffset value, JsonSerializerOptions options) =>
        writer.WriteStringValue(value.UtcDateTime.ToString(value.Millisecond == 0 ? "yyyy-MM-dd'T'HH:mm:ss'Z'" : "yyyy-MM-dd'T'HH:mm:ss.fff'Z'", CultureInfo.InvariantCulture));
}

internal sealed class NullableUtcTimeConverter : JsonConverter<DateTimeOffset?>
{
    private static readonly UtcTimeConverter Inner = new();

    public override bool HandleNull => true;

    public override DateTimeOffset? Read(ref Utf8JsonReader reader, Type typeToConvert, JsonSerializerOptions options) =>
        reader.TokenType == JsonTokenType.Null ? null : Inner.Read(ref reader, typeof(DateTimeOffset), options);

    public override void Write(Utf8JsonWriter writer, DateTimeOffset? value, JsonSerializerOptions options)
    {
        if (value is { } v) Inner.Write(writer, v, options);
        else writer.WriteNullValue();
    }
}

/// <summary><c>string | RichText</c>: text made from a string travels as the string.</summary>
internal sealed class RichTextConverter : JsonConverter<RichText>
{
    public override RichText Read(ref Utf8JsonReader reader, Type typeToConvert, JsonSerializerOptions options)
    {
        if (reader.TokenType == JsonTokenType.String) return RichText.FromText(reader.GetString()!);
        var shape = JsonSerializer.Deserialize(ref reader, TwipJsonContext.Default.RichTextShape) ?? throw new JsonException("rich text can't be null");
        return new RichText(shape.Paragraphs);
    }

    public override void Write(Utf8JsonWriter writer, RichText value, JsonSerializerOptions options)
    {
        if (value.Plain is { } plain) writer.WriteStringValue(plain);
        else JsonSerializer.Serialize(writer, new RichTextShape(value.Paragraphs), TwipJsonContext.Default.RichTextShape);
    }
}

internal sealed record RichTextShape(IReadOnlyList<RichParagraph> Paragraphs);

/// <summary>A command state value: a scalar, or any JSON kept as it came.</summary>
internal sealed class ScalarConverter : JsonConverter<object?>
{
    public override bool HandleNull => true;

    public override object? Read(ref Utf8JsonReader reader, Type typeToConvert, JsonSerializerOptions options) => reader.TokenType switch
    {
        JsonTokenType.Null => null,
        JsonTokenType.True => true,
        JsonTokenType.False => false,
        JsonTokenType.Number => reader.GetDouble(),
        JsonTokenType.String => reader.GetString(),
        // shortcut: structured values stay as JSON until each command's value has its own type.
        _ => JsonElement.ParseValue(ref reader),
    };

    public override void Write(Utf8JsonWriter writer, object? value, JsonSerializerOptions options)
    {
        switch (value)
        {
            case null: writer.WriteNullValue(); break;
            case bool b: writer.WriteBooleanValue(b); break;
            case double d: WireJson.WriteNumber(writer, d); break;
            case string s: writer.WriteStringValue(s); break;
            case JsonElement e: e.WriteTo(writer); break;
            default: throw new JsonException($"not a command state value: {value.GetType().Name}");
        }
    }
}

/// <summary>A changed binding travels as an object, a removed one as null, and an unchanged one not at all.</summary>
internal sealed class ControlChangesConverter : JsonConverter<ControlChanges>
{
    public override ControlChanges Read(ref Utf8JsonReader reader, Type typeToConvert, JsonSerializerOptions options)
    {
        var o = JsonElement.ParseValue(ref reader);
        string? Str(string name) => o.TryGetProperty(name, out var v) && v.ValueKind == JsonValueKind.String ? v.GetString() : null;
        var removed = o.TryGetProperty("binding", out var binding) && binding.ValueKind == JsonValueKind.Null;
        return new ControlChanges(
            o.GetProperty("id").Deserialize(TwipJsonContext.Default.ElementId), Str("tag"), Str("title"),
            o.TryGetProperty("lock", out var l) ? l.Deserialize(TwipJsonContext.Default.ControlLock) : null, Str("placeholder"),
            !removed && o.TryGetProperty("binding", out var b) ? b.Deserialize(TwipJsonContext.Default.DataBinding) : null,
            o.TryGetProperty("options", out var opts) ? opts.Deserialize(TwipJsonContext.Default.ControlOptions) : null,
            removed);
    }

    public override void Write(Utf8JsonWriter writer, ControlChanges value, JsonSerializerOptions options)
    {
        writer.WriteStartObject();
        writer.WriteString("id", value.Id.Value);
        if (value.Tag is not null) writer.WriteString("tag", value.Tag);
        if (value.Title is not null) writer.WriteString("title", value.Title);
        if (value.Lock is { } l) { writer.WritePropertyName("lock"); JsonSerializer.Serialize(writer, l, TwipJsonContext.Default.ControlLock); }
        if (value.Placeholder is not null) writer.WriteString("placeholder", value.Placeholder);
        if (value.Binding is not null) { writer.WritePropertyName("binding"); JsonSerializer.Serialize(writer, value.Binding, TwipJsonContext.Default.DataBinding); }
        else if (value.RemoveBinding) writer.WriteNull("binding");
        if (value.Options is not null) { writer.WritePropertyName("options"); JsonSerializer.Serialize(writer, value.Options, TwipJsonContext.Default.ControlOptions); }
        writer.WriteEndObject();
    }
}

/// <summary><c>{ id } | { tag } | { prefix } | 'default'</c>.</summary>
internal sealed class PlaceholderMatchConverter : JsonConverter<PlaceholderMatch>
{
    public override PlaceholderMatch Read(ref Utf8JsonReader reader, Type typeToConvert, JsonSerializerOptions options)
    {
        if (reader.TokenType == JsonTokenType.String && reader.ValueTextEquals("default")) return PlaceholderMatch.Default;
        var o = JsonElement.ParseValue(ref reader);
        if (o.TryGetProperty("id", out var id)) return PlaceholderMatch.ById(new ElementId(id.GetString()!));
        if (o.TryGetProperty("tag", out var tag)) return PlaceholderMatch.ByTag(tag.GetString()!);
        if (o.TryGetProperty("prefix", out var prefix)) return PlaceholderMatch.ByPrefix(prefix.GetString()!);
        throw new JsonException("not a placeholder match");
    }

    public override void Write(Utf8JsonWriter writer, PlaceholderMatch value, JsonSerializerOptions options)
    {
        if (value.IsDefault) { writer.WriteStringValue("default"); return; }
        writer.WriteStartObject();
        if (value.Id is { } id) writer.WriteString("id", id.Value);
        else if (value.Tag is not null) writer.WriteString("tag", value.Tag);
        else writer.WriteString("prefix", value.Prefix);
        writer.WriteEndObject();
    }
}

internal sealed class MenuActionConverter : JsonConverter<MenuAction>
{
    public override MenuAction Read(ref Utf8JsonReader reader, Type typeToConvert, JsonSerializerOptions options) => new(JsonNode.Parse(ref reader));

    public override void Write(Utf8JsonWriter writer, MenuAction value, JsonSerializerOptions options)
    {
        if (value.Wire is null) writer.WriteNullValue();
        else value.Wire.WriteTo(writer);
    }
}

/// <summary><c>'default' | 'emit' | 'none' | { op }</c>.</summary>
internal sealed class GestureActionConverter : JsonConverter<GestureAction>
{
    public override GestureAction Read(ref Utf8JsonReader reader, Type typeToConvert, JsonSerializerOptions options)
    {
        if (reader.TokenType == JsonTokenType.String)
        {
            return reader.GetString() switch
            {
                "default" => GestureAction.Default,
                "emit" => GestureAction.Emit,
                "none" => GestureAction.None,
                var s => throw new JsonException($"not a gesture action: {s}"),
            };
        }
        var o = JsonNode.Parse(ref reader)?["op"] as JsonObject ?? throw new JsonException("a gesture action needs an op");
        return GestureAction.Run(EditorOperation.FromWire(o));
    }

    public override void Write(Utf8JsonWriter writer, GestureAction value, JsonSerializerOptions options)
    {
        if (value.Operation is null) { writer.WriteStringValue(value.Kind); return; }
        writer.WriteStartObject();
        writer.WritePropertyName("op");
        value.Operation.ToWire().WriteTo(writer);
        writer.WriteEndObject();
    }
}

internal sealed class EditorOperationConverter : JsonConverter<EditorOperation>
{
    public override EditorOperation Read(ref Utf8JsonReader reader, Type typeToConvert, JsonSerializerOptions options) =>
        EditorOperation.FromWire(JsonNode.Parse(ref reader) as JsonObject ?? throw new JsonException("an op is an object"));

    public override void Write(Utf8JsonWriter writer, EditorOperation value, JsonSerializerOptions options) => value.ToWire().WriteTo(writer);
}

/// <summary>A point as <c>{ x, y }</c>.</summary>
internal sealed class WirePointConverter : JsonConverter<PointF>
{
    public override PointF Read(ref Utf8JsonReader reader, Type typeToConvert, JsonSerializerOptions options)
    {
        var o = JsonElement.ParseValue(ref reader);
        return new PointF(o.GetProperty("x").GetSingle(), o.GetProperty("y").GetSingle());
    }

    public override void Write(Utf8JsonWriter writer, PointF value, JsonSerializerOptions options)
    {
        writer.WriteStartObject();
        writer.WritePropertyName("x");
        WireJson.WriteNumber(writer, value.X);
        writer.WritePropertyName("y");
        WireJson.WriteNumber(writer, value.Y);
        writer.WriteEndObject();
    }
}

/// <summary>A rectangle as <c>{ left, top, width, height }</c>.</summary>
internal sealed class WireRectConverter : JsonConverter<RectangleF>
{
    public override RectangleF Read(ref Utf8JsonReader reader, Type typeToConvert, JsonSerializerOptions options)
    {
        var o = JsonElement.ParseValue(ref reader);
        return new RectangleF(o.GetProperty("left").GetSingle(), o.GetProperty("top").GetSingle(), o.GetProperty("width").GetSingle(), o.GetProperty("height").GetSingle());
    }

    public override void Write(Utf8JsonWriter writer, RectangleF value, JsonSerializerOptions options)
    {
        writer.WriteStartObject();
        foreach (var (name, v) in new[] { ("left", value.Left), ("top", value.Top), ("width", value.Width), ("height", value.Height) })
        {
            writer.WritePropertyName(name);
            WireJson.WriteNumber(writer, v);
        }
        writer.WriteEndObject();
    }
}

/// <summary>A point in client pixels as <c>{ clientX, clientY }</c>.</summary>
internal sealed class ClientPointConverter : JsonConverter<PointF>
{
    public override PointF Read(ref Utf8JsonReader reader, Type typeToConvert, JsonSerializerOptions options)
    {
        var p = JsonSerializer.Deserialize(ref reader, TwipJsonContext.Default.ClientPoint) ?? throw new JsonException("a point can't be null");
        return new PointF((float)p.ClientX, (float)p.ClientY);
    }

    public override void Write(Utf8JsonWriter writer, PointF value, JsonSerializerOptions options) =>
        JsonSerializer.Serialize(writer, new ClientPoint(value.X, value.Y), TwipJsonContext.Default.ClientPoint);
}

/// <summary>Modifier keys as <c>{ ctrl, shift, alt, meta }</c>.</summary>
internal sealed class KeyModifiersConverter : JsonConverter<Hosting.KeyModifiers>
{
    private static readonly (string Name, Hosting.KeyModifiers Flag)[] Keys =
        [("ctrl", Hosting.KeyModifiers.Control), ("shift", Hosting.KeyModifiers.Shift), ("alt", Hosting.KeyModifiers.Alt), ("meta", Hosting.KeyModifiers.Meta)];

    public override Hosting.KeyModifiers Read(ref Utf8JsonReader reader, Type typeToConvert, JsonSerializerOptions options)
    {
        var o = JsonElement.ParseValue(ref reader);
        var value = Hosting.KeyModifiers.None;
        foreach (var (name, flag) in Keys)
        {
            if (o.TryGetProperty(name, out var v) && v.GetBoolean()) value |= flag;
        }
        return value;
    }

    public override void Write(Utf8JsonWriter writer, Hosting.KeyModifiers value, JsonSerializerOptions options)
    {
        writer.WriteStartObject();
        foreach (var (name, flag) in Keys) writer.WriteBoolean(name, value.HasFlag(flag));
        writer.WriteEndObject();
    }
}

/// <summary><c>string | { windows, mac }</c>.</summary>
internal sealed class ShortcutKeysConverter : JsonConverter<ShortcutKeys>
{
    public override ShortcutKeys Read(ref Utf8JsonReader reader, Type typeToConvert, JsonSerializerOptions options)
    {
        if (reader.TokenType == JsonTokenType.String) return new ShortcutKeys(reader.GetString()!);
        var o = JsonElement.ParseValue(ref reader);
        return new ShortcutKeys(o.GetProperty("windows").GetString()!, o.GetProperty("mac").GetString()!);
    }

    public override void Write(Utf8JsonWriter writer, ShortcutKeys value, JsonSerializerOptions options)
    {
        if (value.Mac is null) { writer.WriteStringValue(value.Windows); return; }
        writer.WriteStartObject();
        writer.WriteString("windows", value.Windows);
        writer.WriteString("mac", value.Mac);
        writer.WriteEndObject();
    }
}

/// <summary>A dialog's values: its record's fields beside <c>kind</c>.</summary>
internal sealed class DialogValuesConverter : JsonConverter<DialogValues>
{
    private static JsonTypeInfo Info(DialogKind kind)
    {
        var ctx = TwipJsonContext.Default;
        return kind switch
        {
            DialogKind.Font => ctx.FontDialogValues,
            DialogKind.Paragraph => ctx.ParagraphDialogValues,
            DialogKind.PageSetup => ctx.PageSetupDialogValues,
            DialogKind.InsertTable => ctx.InsertTableDialogValues,
            DialogKind.TableProperties => ctx.TablePropertiesDialogValues,
            DialogKind.PictureSize => ctx.PictureSizeDialogValues,
            DialogKind.AltText => ctx.AltTextDialogValues,
            DialogKind.Hyperlink => ctx.HyperlinkDialogValues,
            DialogKind.Tabs => ctx.TabsDialogValues,
            DialogKind.Accessibility => ctx.AccessibilityDialogValues,
            _ => throw new JsonException($"no values for the {kind} dialog"),
        };
    }

    public override DialogValues Read(ref Utf8JsonReader reader, Type typeToConvert, JsonSerializerOptions options)
    {
        var o = JsonElement.ParseValue(ref reader);
        var kind = o.GetProperty("kind").Deserialize(TwipJsonContext.Default.DialogKind);
        return new DialogValues(kind, o.Deserialize(Info(kind)) ?? throw new JsonException("dialog values can't be null"));
    }

    public override void Write(Utf8JsonWriter writer, DialogValues value, JsonSerializerOptions options)
    {
        var o = JsonSerializer.SerializeToNode(value.Values, Info(value.Kind))!.AsObject();
        o.Insert(0, "kind", JsonSerializer.SerializeToNode(value.Kind, TwipJsonContext.Default.DialogKind));
        o.WriteTo(writer);
    }
}

/// <summary><c>MenuTarget | { kind: area }</c>: the areas' kinds are distinct from the menu targets'.</summary>
internal sealed class GestureTargetConverter : JsonConverter<GestureTarget>
{
    public override GestureTarget Read(ref Utf8JsonReader reader, Type typeToConvert, JsonSerializerOptions options)
    {
        var o = JsonElement.ParseValue(ref reader);
        if (Enum.TryParse<GestureArea>(o.GetProperty("kind").GetString(), ignoreCase: true, out var area)) return new GestureTarget(null, area);
        return new GestureTarget(o.Deserialize(TwipJsonContext.Default.MenuTarget), null);
    }

    public override void Write(Utf8JsonWriter writer, GestureTarget value, JsonSerializerOptions options)
    {
        if (value.Content is { } content) { JsonSerializer.Serialize(writer, content, TwipJsonContext.Default.MenuTarget); return; }
        writer.WriteStartObject();
        writer.WritePropertyName("kind");
        JsonSerializer.Serialize(writer, value.Area ?? throw new JsonException("a gesture target needs content or an area"), TwipJsonContext.Default.GestureArea);
        writer.WriteEndObject();
    }
}

/// <summary><c>{ kind: 'command', id } | { kind: 'shortcut', id } | { kind: 'host', command }</c>.</summary>
internal sealed class ShortcutBindingConverter : JsonConverter<ShortcutBinding>
{
    public override ShortcutBinding Read(ref Utf8JsonReader reader, Type typeToConvert, JsonSerializerOptions options)
    {
        var ctx = TwipJsonContext.Default;
        var o = JsonElement.ParseValue(ref reader);
        return o.GetProperty("kind").Deserialize(ctx.ShortcutKind) switch
        {
            ShortcutKind.Command => new ShortcutBinding(ShortcutKind.Command, Command: o.GetProperty("id").Deserialize(ctx.CommandId)),
            ShortcutKind.Shortcut => new ShortcutBinding(ShortcutKind.Shortcut, Id: o.GetProperty("id").GetString()),
            _ => new ShortcutBinding(ShortcutKind.Host, Host: o.GetProperty("command").Deserialize(ctx.HostCommand)),
        };
    }

    public override void Write(Utf8JsonWriter writer, ShortcutBinding value, JsonSerializerOptions options)
    {
        var ctx = TwipJsonContext.Default;
        writer.WriteStartObject();
        writer.WritePropertyName("kind");
        JsonSerializer.Serialize(writer, value.Kind, ctx.ShortcutKind);
        switch (value.Kind)
        {
            case ShortcutKind.Command: writer.WritePropertyName("id"); JsonSerializer.Serialize(writer, value.Command.GetValueOrDefault(), ctx.CommandId); break;
            case ShortcutKind.Shortcut: writer.WriteString("id", value.Id); break;
            default: writer.WritePropertyName("command"); JsonSerializer.Serialize(writer, value.Host.GetValueOrDefault(), ctx.HostCommand); break;
        }
        writer.WriteEndObject();
    }
}
