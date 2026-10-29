using System.Text.Json;
using System.Text.Json.Nodes;
using Xunit;

namespace Twip.Tests;

/// <summary>The repo layout and JSON comparison the wire and contract tests share.</summary>
internal static class Repo
{
    /// <summary>The repo root: the first parent of the test binaries with a <c>crates</c> folder.</summary>
    public static string Root { get; } = FindRoot();

    public static string Path(params string[] parts) => System.IO.Path.Combine([Root, .. parts]);

    private static string FindRoot()
    {
        for (var dir = new DirectoryInfo(AppContext.BaseDirectory); dir is not null; dir = dir.Parent)
        {
            if (Directory.Exists(System.IO.Path.Combine(dir.FullName, "crates")) && File.Exists(System.IO.Path.Combine(dir.FullName, "Cargo.toml"))) return dir.FullName;
        }
        throw new InvalidOperationException("the tests must run inside the repo");
    }
}

/// <summary>The engine the session tests drive.</summary>
internal static class Native
{
    /// <summary>Skips the test when the native engine isn't built.</summary>
    public static void Require()
    {
        if (!Twip.Native.TwipNative.IsAvailable()) Assert.Skip($"the native engine ({Twip.Native.TwipNative.Library}) with the C ABI isn't built: run `moon run twip-capi:build`");
    }

    public static string Fixture(string file) => Repo.Path("crates", "twip", "tests", "fixtures", "editor", file);

    /// <summary>
    /// A check of engine behaviour that hasn't landed: it skips while it fails, and fails once it passes, so the
    /// wrapper is removed when the engine catches up (as <c>tests/contract/pending.txt</c> does for scripts).
    /// </summary>
    public static async Task PendingAsync(string waitsOn, Func<Task> check)
    {
        try { await check(); }
        catch (Exception e)
        {
            Assert.Skip($"waits on {waitsOn}: {e.Message}");
        }
        Assert.Fail($"passes now that {waitsOn} landed: remove its {nameof(PendingAsync)} wrapper");
    }
}

internal static class JsonCompare
{
    /// <summary>
    /// Exact equality, with numbers compared by value and object keys in any order. With
    /// <paramref name="missingIsNull"/> (<c>$exact</c>), a missing key equals a null one; the wire round trips keep
    /// them apart.
    /// </summary>
    public static bool Equal(JsonNode? a, JsonNode? b, string path, out string why, bool missingIsNull = false)
    {
        why = "";
        switch (a, b)
        {
            case (null, null):
                return true;
            case (JsonObject x, JsonObject y):
                foreach (var (key, value) in x)
                {
                    if (!y.ContainsKey(key) && !(missingIsNull && value is null)) { why = $"{path}.{key}: missing after the round trip"; return false; }
                    if (!Equal(value, y[key], $"{path}.{key}", out why, missingIsNull)) return false;
                }
                foreach (var (key, value) in y)
                {
                    if (!x.ContainsKey(key) && !(missingIsNull && value is null)) { why = $"{path}.{key}: added by the round trip"; return false; }
                }
                return true;
            case (JsonArray x, JsonArray y):
                if (x.Count != y.Count) { why = $"{path}: {x.Count} elements, then {y.Count}"; return false; }
                for (var i = 0; i < x.Count; i++)
                {
                    if (!Equal(x[i], y[i], $"{path}[{i}]", out why, missingIsNull)) return false;
                }
                return true;
            default:
                if (ScalarEqual(a, b)) return true;
                why = $"{path}: {a?.ToJsonString() ?? "null"} became {b?.ToJsonString() ?? "null"}";
                return false;
        }
    }

    /// <summary>
    /// A partial match (tests/contract/README.md, "Matching"): only the keys <paramref name="want"/> names (a missing
    /// key is null), arrays compare their leading elements, and <c>$exact</c>, <c>$has</c> and <c>$type</c> test differently.
    /// </summary>
    public static bool Matches(JsonNode? want, JsonNode? got, string path, out string why)
    {
        why = "";
        var shown = got?.ToJsonString() ?? "null";
        if (want is JsonObject { Count: 1 } special)
        {
            if (special.TryGetPropertyValue("$exact", out var exact))
            {
                if (Equal(exact, got, path, out _, missingIsNull: true)) return true;
                why = $"{path}: expected exactly {exact?.ToJsonString() ?? "null"}, got {shown}";
                return false;
            }
            if (special.TryGetPropertyValue("$type", out var type))
            {
                if (TypeName(got) == type?.GetValue<string>()) return true;
                why = $"{path}: expected a {type}, got {shown}";
                return false;
            }
            if (special.TryGetPropertyValue("$has", out var has))
            {
                var all = Items(got).ToList();
                if (got is JsonArray && Assign([.. has!.AsArray()], all, new bool[all.Count])) return true;
                why = $"{path}: {shown} doesn't have each of {has!.ToJsonString()}";
                return false;
            }
        }
        switch (want)
        {
            case JsonObject w:
                if (got is not JsonObject g) { why = $"{path}: expected an object, got {shown}"; return false; }
                foreach (var (key, value) in w)
                {
                    if (!Matches(value, g[key], $"{path}.{key}", out why)) return false;
                }
                return true;
            case JsonArray w:
                if (got is not JsonArray ga) { why = $"{path}: expected an array, got {shown}"; return false; }
                if (ga.Count < w.Count) { why = $"{path}: expected at least {w.Count} elements, got {ga.Count}: {shown}"; return false; }
                for (var i = 0; i < w.Count; i++)
                {
                    if (!Matches(w[i], ga[i], $"{path}[{i}]", out why)) return false;
                }
                return true;
            default:
                if (ScalarEqual(want, got)) return true;
                why = $"{path}: expected {want?.ToJsonString() ?? "null"}, got {shown}";
                return false;
        }
    }

    /// <summary><paramref name="wants"/>, in order, against a subsequence of <paramref name="events"/>.</summary>
    public static bool Subsequence(JsonArray wants, IReadOnlyList<JsonNode> events, out string why)
    {
        why = "";
        var next = 0;
        for (var i = 0; i < wants.Count; i++)
        {
            while (next < events.Count && !Matches(wants[i], events[next], "", out _)) next++;
            if (next == events.Count)
            {
                why = $"events[{i}]: no later event matches {wants[i]?.ToJsonString()}; the events were {new JsonArray([.. events.Select(e => e.DeepClone())]).ToJsonString()}";
                return false;
            }
            next++;
        }
        return true;
    }

    /// <summary>An array's elements and those of their nested <c>items</c> arrays (submenus).</summary>
    public static IEnumerable<JsonNode?> Items(JsonNode? v)
    {
        if (v is not JsonArray a) yield break;
        foreach (var e in a)
        {
            yield return e;
            foreach (var nested in Items((e as JsonObject)?["items"])) yield return nested;
        }
    }

    private static bool Assign(JsonNode?[] wants, List<JsonNode?> got, bool[] used)
    {
        if (wants.Length == 0) return true;
        for (var i = 0; i < got.Count; i++)
        {
            if (used[i] || !Matches(wants[0], got[i], "", out _)) continue;
            used[i] = true;
            if (Assign(wants.Skip(1).ToArray(), got, used)) return true;
            used[i] = false;
        }
        return false;
    }

    private static string TypeName(JsonNode? v) => v switch
    {
        null => "null",
        JsonObject => "object",
        JsonArray => "array",
        JsonValue x => x.GetValueKind() switch
        {
            JsonValueKind.String => "string",
            JsonValueKind.Number => "number",
            JsonValueKind.True or JsonValueKind.False => "boolean",
            _ => "null",
        },
        _ => "null",
    };

    private static bool ScalarEqual(JsonNode? a, JsonNode? b)
    {
        if (a is null || b is null) return a is null && b is null;
        if (a is not JsonValue x || b is not JsonValue y) return false;
        var (kx, ky) = (x.GetValueKind(), y.GetValueKind());
        if (kx == JsonValueKind.Number && ky == JsonValueKind.Number) return Number(x) == Number(y);
        return kx == ky && x.ToJsonString() == y.ToJsonString();
    }

    private static double Number(JsonValue v) => double.Parse(v.ToJsonString(), System.Globalization.CultureInfo.InvariantCulture);
}
