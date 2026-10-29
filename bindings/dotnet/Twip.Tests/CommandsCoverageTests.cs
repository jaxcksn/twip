using System.Reflection;
using System.Text.Json;
using System.Text.Json.Nodes;
using Twip.Wire;
using Xunit;

namespace Twip.Tests;

/// <summary>
/// Every command in the engine's <c>command.json</c> fixture has an <see cref="EditorCommands"/> method and an
/// <see cref="EditorOperation"/> factory with the same parameters, and the factory, given the fixture's arguments,
/// encodes exactly the fixture's op.
/// </summary>
public sealed class CommandsCoverageTests
{
    private static readonly TwipJsonContext Ctx = TwipJsonContext.Default;

    /// <summary>Parameters whose wire key isn't their name.</summary>
    private static readonly Dictionary<string, string> Keys = new()
    {
        ["InsertSymbol.character"] = "char",
        ["Paste.mode"] = "as",
        ["SetCheckbox.isChecked"] = "checked",
    };

    private static JsonArray Fixture() =>
        JsonNode.Parse(File.ReadAllText(Repo.Path("crates", "twip", "tests", "wire", "command.json")))!.AsArray();

    private static string Pascal(string id) => char.ToUpperInvariant(id[0]) + id[1..];

    public static TheoryData<string> Ops()
    {
        var data = new TheoryData<string>();
        foreach (var op in Fixture()) data.Add(op!.ToJsonString());
        return data;
    }

    [Theory]
    [MemberData(nameof(Ops))]
    public void CommandHasAMethodAndAnOperationThatEncodesIt(string json)
    {
        var op = JsonNode.Parse(json)!.AsObject();
        var name = Pascal(op["type"]!.GetValue<string>());

        var factories = typeof(EditorOperation).GetMethods(BindingFlags.Public | BindingFlags.Static).Where(m => m.Name == name).ToList();
        Assert.True(factories.Count > 0, $"no EditorOperation.{name}");
        var methods = typeof(EditorCommands).GetMethods().Where(m => m.Name == name + "Async").ToList();
        Assert.True(methods.Count > 0, $"no EditorCommands.{name}Async");

        // Each factory has a command method with the same parameters, then a CancellationToken.
        foreach (var factory in factories)
        {
            var want = factory.GetParameters().Select(p => (p.Name, p.ParameterType)).Append(("ct", typeof(CancellationToken))).ToList();
            Assert.Contains(methods, m => m.GetParameters().Select(p => (p.Name, p.ParameterType)).SequenceEqual(want));
            Assert.True(typeof(Task).IsAssignableFrom(methods[0].ReturnType), $"EditorCommands.{name}Async returns {methods[0].ReturnType}");
        }

        // Some overload, given the fixture's arguments, encodes exactly the fixture's op.
        var why = "";
        foreach (var factory in factories)
        {
            if (!TryArguments(factory, op, out var args, out why)) continue;
            var encoded = JsonSerializer.SerializeToNode((EditorOperation)factory.Invoke(null, args)!, Ctx.EditorOperation);
            if (JsonCompare.Equal(op, encoded, name, out why)) return;
        }
        Assert.Fail($"EditorOperation.{name} doesn't encode {json}: {why}");
    }

    [Fact]
    public void EveryCommandIdIsInTheFixtureOnce()
    {
        var types = Fixture().Select(op => op!["type"]!.GetValue<string>()).ToList();
        var ids = Enum.GetValues<CommandId>().Select(id => JsonSerializer.Serialize(id, Ctx.CommandId).Trim('"')).ToList();
        Assert.Equal(ids.Order(), types.Distinct().Order());
        var factories = typeof(EditorOperation).GetMethods(BindingFlags.Public | BindingFlags.Static).Where(m => !m.IsSpecialName).Select(m => m.Name).Distinct();
        Assert.Equal(ids.Select(Pascal).Order(), factories.Order());
    }

    /// <summary>
    /// The factory's arguments from an op: a parameter is the op's field of its name, the op's <c>value</c> when it
    /// is the only one, or (an options record) the op's remaining fields. Optional parameters may be absent.
    /// </summary>
    private static bool TryArguments(MethodInfo factory, JsonObject op, out object?[] args, out string why)
    {
        var parameters = factory.GetParameters();
        var rest = op.DeepClone().AsObject();
        rest.Remove("type");
        args = new object?[parameters.Length];
        why = "";
        for (var i = 0; i < parameters.Length; i++)
        {
            var p = parameters[i];
            var key = Keys.GetValueOrDefault($"{factory.Name}.{p.Name}", p.Name!);
            var options = IsOptions(p.ParameterType) && rest.Count > 0 && rest.All(f => p.ParameterType.GetProperty(Pascal(f.Key)) is not null);
            if (options)
            {
                args[i] = Read(rest, p.ParameterType);
                rest = [];
            }
            else if (rest.ContainsKey(key))
            {
                args[i] = Read(rest[key], p.ParameterType);
                rest.Remove(key);
            }
            else if (parameters.Length == 1 && rest.ContainsKey("value"))
            {
                args[i] = Read(rest["value"], p.ParameterType);
                rest.Remove("value");
            }
            else if (p.HasDefaultValue)
            {
                args[i] = p.DefaultValue;
            }
            else
            {
                why = $"no argument for {p.Name}";
                return false;
            }
        }
        if (rest.Count > 0) why = $"unused fields {rest.ToJsonString()}";
        return rest.Count == 0;
    }

    private static bool IsOptions(Type type) => type.IsClass && type != typeof(string) && !type.IsAssignableTo(typeof(System.Collections.IEnumerable));

    private static object? Read(JsonNode? node, Type type)
    {
        // Null is the wire form of these types' None.
        if (node is null && type == typeof(ColorValue)) return ColorValue.None;
        if (node is null && type == typeof(HighlightColor)) return HighlightColor.None;
        if (node is null && type == typeof(ListKind)) return ListKind.None;
        // Write-only: built from its factories.
        if (type == typeof(ControlContent))
        {
            var o = node!.AsObject();
            return o["text"] is { } text ? ControlContent.FromText(text.GetValue<string>())
                : o["image"] is { } image ? ControlContent.FromImage(Convert.FromBase64String(image.GetValue<string>()))
                : ControlContent.FromDocx(Convert.FromBase64String(o["docx"]!.GetValue<string>()));
        }
        var info = Ctx.GetTypeInfo(Nullable.GetUnderlyingType(type) ?? type) ?? throw new Xunit.Sdk.XunitException($"{type} is not a wire type");
        return node.Deserialize(info);
    }
}
