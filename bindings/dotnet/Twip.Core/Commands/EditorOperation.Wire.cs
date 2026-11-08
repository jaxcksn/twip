using System.Text.Json;
using System.Text.Json.Nodes;
using Twip.Wire;

namespace Twip;

public abstract partial record EditorOperation
{
    /// <summary>One argument of an op: a field of the wire object, or <c>*</c> to flatten an options record into it.</summary>
    private readonly record struct Arg(string Key, JsonNode? Node, bool Optional = false)
    {
        public static Arg Flatten<T>(T options) => new("*", EditorOperation.Node(options));
    }

    private static EditorOperation Op(CommandId id, params ReadOnlySpan<Arg> args)
    {
        var obj = new JsonObject();
        foreach (var arg in args)
        {
            if (arg.Key == "*")
            {
                if (arg.Node is not JsonObject fields) continue;
                foreach (var (key, value) in fields.ToArray())
                {
                    fields.Remove(key);
                    obj[key] = value;
                }
            }
            else if (arg.Node is not null || !arg.Optional)
            {
                obj[arg.Key] = arg.Node;
            }
        }
        return new Wired(id, obj);
    }

    private static JsonNode? Node<T>(T value) => WireJson.ToNode(value);

    private static JsonNode? NoneAsNull(HighlightColor color) => color == HighlightColor.None ? null : Node(color);

    private static JsonNode? NoneAsNull(ListKind kind) => kind == ListKind.None ? null : Node(kind);

    private static JsonNode? NoneAsNull(ColorValue color) => color.IsNone ? null : Node(color);

    private static JsonNode? Nullable<T>(T? value) where T : struct => value is { } v ? Node(v) : null;

    /// <summary>Reads an op from its wire form: the command id as <c>type</c>, then its arguments.</summary>
    internal static EditorOperation FromWire(JsonObject obj)
    {
        var args = (JsonObject)obj.DeepClone();
        var type = args["type"] ?? throw new JsonException("an op needs a type");
        args.Remove("type");
        var id = type.Deserialize(TwipJsonContext.Default.CommandId);
        return new Wired(id, args);
    }

    private sealed record Wired(CommandId Command, JsonObject Args) : EditorOperation
    {
        internal override CommandId Id => Command;

        internal override JsonObject ToWire()
        {
            var obj = new JsonObject { ["type"] = WireJson.ToNode(Command) };
            foreach (var (key, value) in Args) obj[key] = value?.DeepClone();
            return obj;
        }

        public bool Equals(Wired? other) => other is not null && Command == other.Command && JsonNode.DeepEquals(Args, other.Args);

        public override int GetHashCode() => Command.GetHashCode();

        public override string ToString() => ToWire().ToJsonString();
    }
}
