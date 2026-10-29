using System.Drawing;
using System.Reflection;
using System.Text.Json;
using System.Text.Json.Nodes;
using System.Text.Json.Serialization.Metadata;
using Twip.Wire;
using Xunit;

namespace Twip.Tests;

/// <summary>Every engine wire fixture decodes into the SDK's types and encodes back to the same JSON.</summary>
public sealed class WireTests
{
    private static readonly TwipJsonContext Ctx = TwipJsonContext.Default;

    private static string FixtureDir => Repo.Path("crates", "twip", "tests", "wire");

    /// <summary>Fixture names (without separators, lower case) for types whose fixture isn't named after the C# type.</summary>
    private static readonly Dictionary<string, JsonTypeInfo> Aliases = new()
    {
        ["state"] = Ctx.EditorState,
        ["command"] = Ctx.EditorOperation,
        ["op"] = Ctx.EditorOperation,
        ["record"] = Ctx.TransactionRecord,
        ["error"] = Ctx.ErrorBody,
        ["errorcode"] = Ctx.EditorErrorCode,
        ["mode"] = Ctx.EditorMode,
        ["cursor"] = Ctx.EditorCursor,
        ["format"] = Ctx.CharacterFormatting,
        ["formatting"] = Ctx.CharacterFormatting,
        ["selection"] = Ctx.SelectionSummary,
        ["catalog"] = Ctx.CommandInfo,
        ["menuitem"] = Ctx.MenuItem,
        ["sessionoptions"] = Ctx.SessionOptionsWire,
        ["exportoptions"] = Ctx.ExportOptionsWire,
        ["export"] = Ctx.ExportMeta,
        ["exportformat"] = Ctx.String,
        ["sessionid"] = Ctx.UInt32,
        ["points"] = Ctx.Length,
        ["color"] = Ctx.ColorValue,
        ["autoor"] = Ctx.ColorValue,
        ["noneor"] = Ctx.ColorValue,
        ["tri"] = Ctx.FormatValueBoolean,
        ["bytes"] = Ctx.ReadOnlyMemoryByte,
        ["timestamp"] = Ctx.DateTimeOffset,
        ["body"] = Ctx.RichText,
        ["location"] = Ctx.WarningLocation,
        ["commandvalue"] = Ctx.Object,
        ["zoom"] = Ctx.ZoomLevel,
        ["zoomfit"] = Ctx.ZoomLevel,
        ["defaultkeyword"] = Ctx.PlaceholderMatch,
        ["placeholderlook"] = Ctx.PlaceholderAppearance,
        ["placeholderinfo"] = Ctx.PlaceholderInfo,
        ["tooltip"] = Ctx.TooltipWire,
        ["contextmenuevent"] = Ctx.ContextMenuPayload,
        ["tabstopinfo"] = Ctx.TabStop,
        ["keybinding"] = Ctx.ShortcutBinding,
        ["clipboarddata"] = Ctx.ClipboardContent,
        ["point"] = Ctx.PointF,
        ["contentpoint"] = Ctx.PointF,
        ["clientrect"] = Ctx.RectangleF,
        ["key"] = Ctx.EditorKey,
        ["modifiers"] = Ctx.KeyModifiers,
        ["alignment"] = Ctx.ParagraphAlignment,
        ["horizontalalignment"] = Ctx.TableAlignment,
        ["horizontalside"] = Ctx.ColumnPosition,
        ["verticalside"] = Ctx.RowPosition,
        ["applyto"] = Ctx.PageSetupScope,
        ["areakind"] = Ctx.GestureArea,
        ["autofit"] = Ctx.AutoFitBehavior,
        ["borderpreset"] = Ctx.ParagraphBorders,
        ["borderstate"] = Ctx.ParagraphBorders,
        ["borderstyle"] = Ctx.LineStyle,
        ["elementclass"] = Ctx.ElementCategory,
        ["elementfield"] = Ctx.ElementTextField,
        ["fontstyle"] = Ctx.FontStyleChoice,
        ["heightrule"] = Ctx.RowHeightRule,
        ["linkvia"] = Ctx.LinkActivation,
        ["liststate"] = Ctx.ListKind,
        ["loadstate"] = Ctx.LoadStatus,
        ["menuvia"] = Ctx.MenuInvocation,
        ["order"] = Ctx.NavigationDirection,
        ["orientation"] = Ctx.PageOrientation,
        ["pagenumberplace"] = Ctx.PageNumberPlacement,
        ["pasteas"] = Ctx.PasteMode,
        ["selectunit"] = Ctx.SelectionUnit,
        ["selectionmode"] = Ctx.TextSelectionMode,
        ["severity"] = Ctx.MarkSeverity,
        ["tabsaction"] = Ctx.TabsDialogAction,
    };

    /// <summary>Each engine event's payload type: the event is <c>{ "type": ..., ...payload }</c>.</summary>
    private static readonly Dictionary<string, JsonTypeInfo> EventPayloads = new()
    {
        ["stateChanged"] = Ctx.EditorState,
        ["documentChanged"] = Ctx.TransactionRecord,
        ["selectionChanged"] = Ctx.SelectionSummary,
        ["linkActivated"] = Ctx.LinkActivatedPayload,
        ["placeholderActivated"] = Ctx.PlaceholderInfo,
        ["contextMenu"] = Ctx.ContextMenuPayload,
        ["dialogRequested"] = Ctx.DialogRequestedPayload,
        ["hostCommand"] = Ctx.HostCommandPayload,
        ["shortcut"] = Ctx.ShortcutPayload,
        ["gesture"] = Ctx.GestureEvent,
        ["wordAdded"] = Ctx.WordAddedPayload,
        ["fontsChanged"] = Ctx.FontsChangedPayload,
        ["elementActivated"] = Ctx.ElementActivatedPayload,
        ["elementsChanged"] = Ctx.ElementsChangedPayload,
        ["fieldsUpdateRequested"] = Ctx.FieldsUpdateRequestedPayload,
        ["revisionsChanged"] = Ctx.EmptyPayload,
        ["commentsChanged"] = Ctx.EmptyPayload,
        ["commentAnchors"] = Ctx.CommentAnchorsPayload,
        ["balloonsLaidOut"] = Ctx.BalloonsLaidOutPayload,
        ["rulerChanged"] = Ctx.RulerView,
        ["tooltip"] = Ctx.TooltipPayload,
        ["announcement"] = Ctx.AnnouncementPayload,
        ["warning"] = Ctx.Warning,
        ["error"] = Ctx.ErrorPayload,
    };

    public static TheoryData<string> Fixtures()
    {
        var data = new TheoryData<string>();
        if (Directory.Exists(FixtureDir))
        {
            // balloons.json isn't a type: it holds the cases StackBalloons must give.
            foreach (var file in Directory.EnumerateFiles(FixtureDir, "*.json").Order().Where(f => Path.GetFileName(f) != "balloons.json")) data.Add(Path.GetFileName(file));
        }
        if (data.Count == 0) data.Add("");
        return data;
    }

    [Theory]
    [MemberData(nameof(Fixtures))]
    public void FixtureRoundTrips(string fixture)
    {
        if (fixture == "") Assert.Skip($"no wire fixtures in {FixtureDir}: run `twip-tools wire-fixtures` to write them");
        var root = JsonNode.Parse(File.ReadAllText(Path.Combine(FixtureDir, fixture)));
        var name = Path.GetFileNameWithoutExtension(fixture);
        var examples = root is JsonArray array ? array.ToList() : [root];
        for (var i = 0; i < examples.Count; i++)
        {
            var original = examples[i];
            var again = Normalize(name) == "event" ? RoundTripEvent(original) : RoundTrip(TypeFor(name), original);
            Assert.True(JsonCompare.Equal(original, again, $"{fixture}[{i}]", out var why), why);
        }
    }

    private static JsonTypeInfo TypeFor(string fixture)
    {
        var key = Normalize(fixture);
        if (Aliases.TryGetValue(key, out var info)) return info;
        foreach (var property in typeof(TwipJsonContext).GetProperties(BindingFlags.Public | BindingFlags.Instance))
        {
            if (property.PropertyType.IsAssignableTo(typeof(JsonTypeInfo)) && Normalize(property.Name) == key)
                return (JsonTypeInfo)property.GetValue(Ctx)!;
        }
        throw new Xunit.Sdk.XunitException($"the wire fixture {fixture}.json has no .NET type: add one, or an alias in {nameof(WireTests)}");
    }

    private static string Normalize(string name) => name.Replace("_", "").Replace("-", "").Replace(".", "").ToLowerInvariant();

    private static JsonNode? RoundTrip(JsonTypeInfo info, JsonNode? json)
    {
        var value = json.Deserialize(info);
        return JsonSerializer.SerializeToNode(value, info);
    }

    private static JsonNode? RoundTripEvent(JsonNode? json)
    {
        var type = json?["type"]?.GetValue<string>() ?? throw new Xunit.Sdk.XunitException("an event has no type");
        if (!EventPayloads.TryGetValue(type, out var info)) throw new Xunit.Sdk.XunitException($"the event {type} has no .NET payload type");
        var payload = RoundTrip(info, json) as JsonObject ?? [];
        var again = new JsonObject { ["type"] = type };
        foreach (var (key, value) in payload.ToArray())
        {
            payload.Remove(key);
            again[key] = value;
        }
        return again;
    }

    // The SDK's own encoding of shapes with custom converters: these run without engine fixtures.

    [Fact]
    public void FormattingValuesEncodeAsTriAndMixed()
    {
        var state = new EditorState
        {
            Formatting = new CharacterFormatting
            {
                Bold = FormatValue<bool>.Uniform(true),
                Italic = FormatValue<bool>.Mixed,
                Size = FormatValue<Length>.Uniform(Length.FromPoints(10.5)),
                Color = FormatValue<ColorValue>.Uniform(ColorValue.FromRgb(0x12, 0xAB, 0xEF)),
                Highlight = FormatValue<HighlightColor>.Uniform(HighlightColor.BrightGreen),
            },
            Commands = new Dictionary<CommandId, CommandState> { [CommandId.SetBold] = new(true, Checked: true) },
        };
        var json = JsonSerializer.SerializeToNode(state, Ctx.EditorState)!;
        var formatting = json["formatting"]!;
        Assert.True(formatting["bold"]!.GetValue<bool>());
        Assert.Equal("mixed", formatting["italic"]!.GetValue<string>());
        Assert.Null(formatting["underline"]);
        Assert.Equal(10.5, formatting["size"]!.GetValue<double>());
        Assert.Equal("#12ABEF", formatting["color"]!.GetValue<string>());
        Assert.Equal("brightGreen", formatting["highlight"]!.GetValue<string>());
        Assert.True(json["commands"]!["setBold"]!["checked"]!.GetValue<bool>());
        Assert.Equal(0, json["revision"]!.GetValue<long>());
        Assert.False(json["view"]!["rulers"]!["horizontal"]!.GetValue<bool>());

        var back = json.Deserialize(Ctx.EditorState)!;
        Assert.True(back.Formatting.Bold.IsOn);
        Assert.True(back.Formatting.Italic.IsMixed);
        Assert.Equal(FormatValueKind.Unavailable, back.Formatting.Underline.Kind);
        Assert.Equal(Color.FromArgb(0x12, 0xAB, 0xEF), back.Formatting.Color.Value.Color);
        Assert.True(back.Commands[CommandId.SetBold].Checked);
    }

    [Fact]
    public void OperationsEncodeAsTypedOps()
    {
        static string Wire(EditorOperation op) => JsonSerializer.Serialize(op, Ctx.EditorOperation);

        Assert.Equal("""{"type":"setBold","value":true}""", Wire(EditorOperation.SetBold(true)));
        Assert.Equal("""{"type":"insertTable","rows":3,"columns":2}""", Wire(EditorOperation.InsertTable(3, 2)));
        Assert.Equal("""{"type":"selectAll"}""", Wire(EditorOperation.SelectAll()));
        Assert.Equal("""{"type":"setIndents","left":36}""", Wire(EditorOperation.SetIndents(new IndentOptions(Left: Length.FromInches(0.5)))));
        Assert.Equal("""{"type":"setHighlight","value":null}""", Wire(EditorOperation.SetHighlight(HighlightColor.None)));
        Assert.Equal("""{"type":"paste","as":"text"}""", Wire(EditorOperation.Paste(PasteMode.Text)));
        Assert.Equal("""{"type":"addComment","value":"Check this"}""", Wire(EditorOperation.AddComment("Check this")));
        Assert.Equal("""{"type":"setLineSpacing","rule":"multiple","lines":1.5}""", Wire(EditorOperation.SetLineSpacing(LineSpacing.Multiple(1.5))));
        Assert.Equal("""{"type":"setRowHeight","value":"auto"}""", Wire(EditorOperation.SetRowHeight(RowHeight.Auto)));

        var op = JsonSerializer.Deserialize("""{"type":"insertText","value":"hi"}""", Ctx.EditorOperation)!;
        Assert.Equal(EditorOperation.InsertText("hi"), op);
    }

    [Fact]
    public void ErrorsUseKebabCodes()
    {
        var e = WireJson.ToException(JsonDocument.Parse("""{"code":"invalid-argument","message":"rows out of range","details":null}""").RootElement);
        Assert.Equal(EditorErrorCode.InvalidArgument, e.Code);
        Assert.Equal("\"read-only\"", JsonSerializer.Serialize(EditorErrorCode.ReadOnly, Ctx.EditorErrorCode));
        Assert.Equal("\"font-substituted\"", JsonSerializer.Serialize(WarningCode.FontSubstituted, Ctx.WarningCode));
    }

    [Fact]
    public void RecordsKeepUtcSeconds()
    {
        const string json = """{"id":1,"revision":2,"origin":"api","author":{"name":"Ann"},"time":"2026-10-27T09:30:00Z","label":"Bold","summary":{"paragraphs":1,"tables":0,"elements":[],"revisions":[],"comments":[]}}""";
        var record = JsonSerializer.Deserialize(json, Ctx.TransactionRecord)!;
        Assert.Equal(Origin.Api, record.Origin);
        Assert.True(JsonCompare.Equal(JsonNode.Parse(json), JsonSerializer.SerializeToNode(record, Ctx.TransactionRecord), "record", out var why), why);
    }

    [Fact]
    public void MenusAndBlocksAreTaggedByKind()
    {
        const string json = """{"target":{"kind":"misspelling","word":"teh","language":"en-US"},"items":[{"kind":"suggestion","label":"the","action":{"spell":1}},{"kind":"separator"}]}""";
        var menu = JsonSerializer.Deserialize(json, Ctx.ContextMenu)!;
        Assert.Equal(new MenuTarget.Misspelling("teh", "en-US"), menu.Target);
        Assert.IsType<SeparatorItem>(menu.Items[1]);
        Assert.True(JsonCompare.Equal(JsonNode.Parse(json), JsonSerializer.SerializeToNode(menu, Ctx.ContextMenu), "menu", out var why), why);
    }
}
