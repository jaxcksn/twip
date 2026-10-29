using System.Text.Json;
using System.Text.Json.Nodes;
using Twip.Hosting;
using Twip.Native;
using Twip.Wire;
using Xunit;
using Xunit.Sdk;

namespace Twip.Tests;

/// <summary>
/// The shared contract scripts in <c>tests/contract</c> (format in its README), on an unattached session
/// (<c>dotnet-unattached</c>) and on one attached to a headless host (<c>dotnet-attached</c>). Scripts listed in
/// <c>pending.txt</c> are skipped while they fail, and fail the run once they pass.
/// </summary>
public sealed class ContractTests
{
    private static string ScriptDir => Repo.Path("tests", "contract");

    private static readonly HashSet<string> Pending = File.Exists(Path.Combine(ScriptDir, "pending.txt"))
        ? File.ReadAllLines(Path.Combine(ScriptDir, "pending.txt")).Select(l => l.Trim()).Where(l => l.Length > 0 && !l.StartsWith('#')).ToHashSet()
        : [];

    public static TheoryData<string, string> Scripts()
    {
        var data = new TheoryData<string, string>();
        var scripts = Directory.Exists(ScriptDir) ? Directory.EnumerateFiles(ScriptDir, "*.json").Order().Select(Path.GetFileName).ToList() : [];
        if (scripts.Count == 0) scripts.Add("");
        foreach (var script in scripts)
        {
            data.Add("dotnet-unattached", script!);
            data.Add("dotnet-attached", script!);
        }
        return data;
    }

    [Theory]
    [MemberData(nameof(Scripts))]
    public async Task ScriptPasses(string runner, string script)
    {
        if (script == "") Assert.Skip($"no contract scripts in {ScriptDir}");
        if (!TwipNative.IsAvailable()) Assert.Skip($"the native engine ({TwipNative.Library}) with the C ABI isn't built: run `moon run twip-capi:build`");
        var json = JsonNode.Parse(await File.ReadAllTextAsync(Path.Combine(ScriptDir, script), TestContext.Current.CancellationToken))!.AsObject();
        var name = Path.GetFileNameWithoutExtension(script);
        var attached = runner == "dotnet-attached";
        if (!attached && json["steps"]!.AsArray().Any(TakesInput))
            Assert.Skip($"{runner}: {name} sends input, which an unattached session takes none of");
        var run = new ScriptRunner(runner, name, json, attached);
        if (!Pending.Contains(name))
        {
            await run.RunAsync();
            return;
        }
        string? why = null;
        try { await run.RunAsync(); }
        catch (Exception e) { why = e.Message; }
        if (why is not null) Assert.Skip($"pending: {why}");
        Assert.Fail($"{runner}: {name}: passes, but pending.txt lists it: delete its line");
    }

    private static bool TakesInput(JsonNode? step) => step?["op"]?.GetValue<string>() switch
    {
        "key" or "text" or "click" => true,
        "concurrent" => step["steps"]!.AsArray().Any(TakesInput),
        "repeat" => TakesInput(step["step"]),
        _ => false,
    };

    private sealed class ScriptRunner(string runner, string name, JsonObject script, bool attached)
    {
        private static readonly TwipJsonContext Ctx = TwipJsonContext.Default;
        private readonly List<JsonObject> _events = [];
        private readonly List<Task> _background = [];
        private readonly List<(string Type, JsonObject Command, string At)> _listeners = [];
        private string? _inconsistent; // the first event whose snapshot wasn't at its revision
        private readonly Dictionary<string, JsonNode?> _saved = [];
        private readonly Dictionary<string, DialogModel> _dialogs = [];
        private DialogModel? _dialog;
        private EditorSession _session = null!;
        private EditorHost? _host;
        private TestHostAdapter? _adapter;
        private string _at = "";
        private (Exception Error, string At)? _failed;
        private string? _abort;
        private byte[]? _exported;

        private static string Fixture(string file) => Repo.Path("crates", "twip", "tests", "fixtures", file);

        public async Task RunAsync()
        {
            var fonts = script["fonts"]?.GetValue<string>() == "empty" ? FontLibrary.CreateEmpty() : null;
            _session = new EditorSession(new EditorSessionOptions(Fonts: fonts));
            if (attached)
            {
                _adapter = new TestHostAdapter();
                _host = _session.Attach(_adapter);
                _host.OnResize(800, 600, 96);
            }
            Subscribe();
            try
            {
                var steps = script["steps"]!.AsArray();
                for (var i = 0; i < steps.Count; i++)
                {
                    var step = steps[i]!.AsObject();
                    var at = $"step {i} ({step["op"]?.GetValue<string>()})";
                    try { await RunStepAsync(step, at); }
                    catch (Exception e) when (e is not XunitException) { Fail(at, $"{e.GetType().Name}: {e.Message}"); }
                }
                if (_failed is { } f) Fail(f.At, $"failed: {Describe(f.Error)}");
                await IdleAsync();
                CheckEvents();
            }
            finally
            {
                await _session.DisposeAsync();
            }
        }

        private async Task RunStepAsync(JsonObject raw, string at)
        {
            _at = at;
            var op = raw["op"]!.GetValue<string>();
            if (_failed is { } f && op != "expectError") Fail(f.At, $"failed: {Describe(f.Error)}");
            var step = op == "repeat" ? raw : Resolve(raw)!.AsObject();
            if (op.StartsWith("expect", StringComparison.Ordinal))
            {
                await IdleAsync();
                CheckEvents();
            }
            switch (op)
            {
                case "expectState":
                    var state = JsonSerializer.SerializeToNode(_session.State, Ctx.EditorState);
                    if (!JsonCompare.Matches(step["match"], state, "state", out var why)) Fail(at, why);
                    return;
                case "expectEvents":
                    JsonObject[] since;
                    lock (_events)
                    {
                        since = [.. _events];
                        _events.Clear();
                    }
                    if (!JsonCompare.Subsequence(step["match"]!.AsArray(), since, out var whyEvents)) Fail(at, whyEvents);
                    return;
                case "expectError":
                    var want = step["code"]!.GetValue<string>();
                    var got = _failed is { } e ? CodeOf(e.Error) : "no error";
                    if (got != want) Fail(at, $"expected the previous step to fail {want}, it {(_failed is { } ee ? $"failed: {Describe(ee.Error)}" : "succeeded")}");
                    _failed = null;
                    return;
                case "abort":
                    _abort = step["late"]?.GetValue<bool>() == true ? "late" : "early";
                    return;
                case "listen":
                    lock (_listeners) _listeners.Add((step["type"]!.GetValue<string>(), step["command"]!.AsObject(), at));
                    return;
                case "repeat":
                    var count = step["count"]!.GetValue<int>();
                    for (var i = 0; i < count; i++) await RunStepAsync(step["step"]!.AsObject(), $"{at}, repeat {i}");
                    return;
                case "concurrent":
                    var tasks = step["steps"]!.AsArray().Select((s, i) => Start(s!.AsObject(), $"{at}.{i}")).ToList();
                    await Settle(Task.WhenAll(tasks), at);
                    return;
                default:
                    await Settle(Start(step, at), at);
                    return;
            }
        }

        private void CheckEvents()
        {
            if (_inconsistent is { } why) Fail(_at, why);
        }

        /// <summary>Waits until the session is idle, including the commands that listeners submitted.</summary>
        private async Task IdleAsync()
        {
            for (var n = -1; ;)
            {
                Task[] pending;
                lock (_events) pending = [.. _background];
                if (pending.Length == n) break;
                n = pending.Length;
                await Task.WhenAll(pending);
                await _session.WhenIdleAsync();
            }
            // Background layout runs on the engine's timers: to the end, so the page count is the document's.
            while (!_session.IsDisposed && !_session.State.Pages.Complete)
            {
                await Task.Delay(5);
                await _session.WhenIdleAsync();
            }
        }

        /// <summary>The token for the next operation: cancelled before it runs, or only after it settles (late).</summary>
        private Task WithToken(Func<CancellationToken, Task> use)
        {
            var how = _abort;
            _abort = null;
            if (how == "early") return use(new CancellationToken(canceled: true));
            if (how != "late") return use(default);
            var late = new CancellationTokenSource();
            var task = use(late.Token);
            return task.ContinueWith(t => { late.Cancel(); return t; }, TaskScheduler.Default).Unwrap();
        }

        /// <summary>Starts an operation step without waiting for it.</summary>
        private Task Start(JsonObject step, string at)
        {
            switch (step["op"]!.GetValue<string>())
            {
                case "open":
                    var fixture = script["fixture"]?.GetValue<string>() ?? throw new XunitException($"{runner}: {name}, {at}: the script has no fixture");
                    return WithToken(async ct =>
                    {
                        var result = await _session.OpenAsync(Fixture(fixture), ct);
                        Check(step["match"], new JsonObject { ["warnings"] = JsonSerializer.SerializeToNode(result.Warnings, Ctx.IReadOnlyListWarning) }, "open", at);
                    });
                case "openBytes":
                    return WithToken(ct => _session.OpenAsync(new MemoryStream(Convert.FromBase64String(step["base64"]!.GetValue<string>())), ct));
                case "reopen":
                    var bytes = _exported ?? throw new XunitException($"{runner}: {name}, {at}: reopen before any docx export");
                    return WithToken(ct => _session.OpenAsync(new MemoryStream(bytes), ct));
                case "export":
                    var format = step["format"]?.GetValue<string>() ?? "docx";
                    return WithToken(async ct =>
                    {
                        var output = new MemoryStream();
                        var result = format switch
                        {
                            "docx" => await _session.SaveAsync(output, ct),
                            "pdf" => await _session.ExportPdfAsync(output, ct),
                            _ => await _session.ExportToStreamAsync(format, output, ct), // the engine rejects it
                        };
                        var now = _session.State.DocumentRevision;
                        if (result.Revision != now) Fail(at, $"the export reports revision {result.Revision}, the state is at {now}");
                        var head = new string([.. output.ToArray().Take(4).Select(b => (char)b)]);
                        Check(step["match"], new JsonObject
                        {
                            ["revision"] = result.Revision,
                            ["warnings"] = JsonSerializer.SerializeToNode(result.Warnings, Ctx.IReadOnlyListWarning),
                            ["head"] = head,
                        }, "export", at);
                        if (format == "docx") _exported = output.ToArray();
                    });
                case "command":
                    return _session.RunAsync(EditorOperation.FromWire(step["command"]!.AsObject()), default);
                case "batch":
                    var ops = step["commands"]!.AsArray().Select(c => EditorOperation.FromWire(c!.AsObject())).ToList();
                    return _session.BatchAsync(ops, step["label"]?.GetValue<string>());
                case "call":
                    var method = step["method"]!.GetValue<string>();
                    var args = step["args"]?.AsArray().ToArray() ?? [];
                    return Call();

                    async Task Call()
                    {
                        _dialog = null;
                        var result = await CallAsync(method, args, default);
                        Check(step["result"], result, "result", at);
                        if (step["save"]?.GetValue<string>() is not { } save) return;
                        _saved[save] = result?.DeepClone();
                        if (_dialog is { } dialog) _dialogs[save] = dialog;
                    }
                case "font":
                    return _session.Fonts.AddAsync(Fixture(step["file"]!.GetValue<string>()));
                case "key":
                    var key = Enum.Parse<EditorKey>(KeyCode(step["key"]!.GetValue<string>()));
                    var mods = ModsOf(step, at);
                    return Input(() => _host!.OnKey(key, mods, out _));
                case "text":
                    return Input(() => _host!.OnText(step["text"]!.GetValue<string>()));
                case "click":
                    return ClickAsync(step["text"]!.GetValue<string>(), ModsOf(step, at), at);
                case "destroy":
                    return _session.DisposeAsync().AsTask();
                case var other:
                    throw new XunitException($"{runner}: {name}, {at}: unknown step {other}");
            }
        }

        /// <summary>
        /// A session member by its TypeScript path, with its result in the wire form. A dialog is its model, and
        /// <c>@name.update</c>, <c>@name.apply</c> and <c>@name.cancel</c> act on the dialog saved as <c>name</c>.
        /// </summary>
        private async Task<JsonNode?> CallAsync(string method, JsonNode?[] args, CancellationToken ct)
        {
            JsonNode? Arg(int i) => i < args.Length ? args[i] : null;
            T Parse<T>(int i) => Arg(i).Deserialize(WireJson.Info<T>())!;
            JsonNode? Json<T>(T value) => WireJson.ToNode(value);
            if (method.StartsWith('@'))
            {
                var (saved, member) = (method[1..method.IndexOf('.')], method[(method.IndexOf('.') + 1)..]);
                if (!_dialogs.TryGetValue(saved, out var model)) Fail(_at, $"no dialog saved as {saved}");
                var dialog = new DialogSession<object>(_session, model!);
                switch (member)
                {
                    case "update":
                        // The patch's fields over the current values, as the TypeScript update(patch) does.
                        var values = Json(model!.Values)!.AsObject();
                        foreach (var (k, v) in Arg(0)!.AsObject()) values[k] = v?.DeepClone();
                        _dialog = (await dialog.UpdateAsync(values.Deserialize(Ctx.DialogValues)!.Values, ct)).Model;
                        return Json(_dialog);
                    case "apply": await dialog.ApplyAsync(ct); return null;
                    case "cancel": dialog.Cancel(); return null;
                }
            }
            switch (method)
            {
                case "dialogs.open":
                    _dialog = await _session.CallAsync<DialogModel>(Method.DialogOpen, WireJson.ToUtf8(Parse<DialogKind>(0)), ct);
                    return Json(_dialog);
                case "setReadOnly": await _session.SetReadOnlyAsync(Parse<bool>(0)); return null;
                case "setAuthor": await _session.SetAuthorAsync(Parse<Author>(0)); return null;
                case "catalog": return Json(_session.Catalog);
                case "fonts.substitute": _session.Fonts.Substitute(Parse<IReadOnlyDictionary<string, string>>(0)); return null;
                case "find.next": return Json(await _session.Find.NextAsync(Parse<FindQuery>(0), ct));
                case "find.previous": return Json(await _session.Find.PreviousAsync(Parse<FindQuery>(0), ct));
                case "find.all": return Json(await _session.Find.FindAllAsync(Parse<FindQuery>(0), ct));
                case "find.select": await _session.Find.SelectAsync(Parse<MatchId>(0), ct); return null;
                case "find.replace": return Json(await _session.Find.ReplaceAsync(Parse<FindQuery>(0), Parse<string>(1), ct));
                // The scripts take the TypeScript shape, `{ count }`.
                case "find.replaceAll": return new JsonObject { ["count"] = await _session.Find.ReplaceAllAsync(Parse<FindQuery>(0), Parse<string>(1), ct) };
                case "menus.contextMenu": return Json(await _session.Menus.GetContextMenuAsync(ct));
                case "menus.invoke": await _session.Menus.InvokeAsync(new MenuAction(Arg(0)?.DeepClone()), ct); return null;
                case "spelling.addDictionary": await _session.Spelling.AddDictionaryAsync(Parse<Dictionary>(0), ct); return null;
                case "spelling.setEnabled": await _session.Spelling.SetEnabledAsync(Parse<bool>(0)); return null;
                case "spelling.setLanguage": await _session.Spelling.SetLanguageAsync(Parse<string>(0)); return null;
                case "spelling.ignore": await _session.Spelling.IgnoreAsync(Parse<string>(0)); return null;
                case "spelling.add": await _session.Spelling.AddAsync(Parse<string>(0)); return null;
                case "spelling.loadUserWords": await _session.Spelling.LoadUserWordsAsync(Parse<string>(0), Arg(1)!.AsArray().Select(w => w!.GetValue<string>())); return null;
                case "spelling.suggestions": return Json(await _session.Spelling.GetSuggestionsAsync(Parse<string>(0), Arg(1)?.GetValue<string>()));
                case "spelling.next": return Json(await _session.Spelling.NextAsync(ct));
                case "navigation.headings": return Json(await _session.Navigation.GetHeadingsAsync(ct));
                case "navigation.select": await _session.Navigation.SelectAsync(Parse<HeadingId>(0), ct); return null;
                case "styles.list": return Json(await _session.Styles.ListAsync(Arg(0)?["type"]?.Deserialize(WireJson.Info<StyleType>()), ct));
                case "review.revisions": return Json(await _session.Review.GetRevisionsAsync(ct));
                case "review.comments": return Json(await _session.Review.GetCommentsAsync(ct));
            }
            if (method.StartsWith("commands.", StringComparison.Ordinal))
            {
                // One argument is the op's value; an options object is spread into the op.
                var id = method["commands.".Length..];
                var op = new JsonObject { ["type"] = id };
                if (Arg(0) is JsonObject fields) foreach (var (k, v) in fields) op[k] = v?.DeepClone();
                else if (args.Length > 0) op["value"] = Arg(0)?.DeepClone();
                var commit = await _session.CallAsync(Method.Run, WireJson.ToUtf8(EditorOperation.FromWire(op)), ct);
                return CommitValue.Of(commit) is { } value ? JsonNode.Parse(value.GetRawText()) : null;
            }
            throw new XunitException($"{runner}: {name}, {_at}: no call method {method} in this runner");
        }

        /// <summary><c>v</c> with every <c>$file</c> replaced by its bytes (base64, their wire form) and every <c>$saved</c> by the saved value.</summary>
        private JsonNode? Resolve(JsonNode? v)
        {
            switch (v)
            {
                case JsonObject o when o.ContainsKey("$file"):
                    return Convert.ToBase64String(File.ReadAllBytes(Fixture(o["$file"]!.GetValue<string>())));
                case JsonObject o when o.ContainsKey("$saved"):
                    var key = o["$saved"]!.GetValue<string>();
                    if (!_saved.TryGetValue(key, out var value)) Fail(_at, $"nothing saved as {key}");
                    foreach (var step in o["path"]?.AsArray() ?? [])
                    {
                        value = step switch
                        {
                            JsonValue s when s.GetValueKind() == JsonValueKind.String => value?[s.GetValue<string>()],
                            JsonValue n when n.GetValueKind() == JsonValueKind.Number => value?[n.GetValue<int>()],
                            _ => JsonCompare.Items(value).FirstOrDefault(e => JsonCompare.Matches(step, e, "", out _)),
                        };
                        if (value is null) Fail(_at, $"$saved {key}: no {step?.ToJsonString()} in it");
                    }
                    return value?.DeepClone();
                case JsonObject o:
                    var copy = new JsonObject();
                    foreach (var (k, x) in o) copy[k] = Resolve(x);
                    return copy;
                case JsonArray a:
                    return new JsonArray([.. a.Select(Resolve)]);
                default:
                    return v?.DeepClone();
            }
        }

        /// <summary>The <c>KeyboardEvent.code</c> for a <c>KeyboardEvent.key</c> value.</summary>
        private static string KeyCode(string key) => key switch
        {
            " " => "Space",
            "-" => "Minus",
            "=" => "Equal",
            "[" => "BracketLeft",
            "]" => "BracketRight",
            "\\" => "Backslash",
            ";" => "Semicolon",
            "'" => "Quote",
            "`" => "Backquote",
            "," => "Comma",
            "." => "Period",
            "/" => "Slash",
            [var c] when char.IsAsciiLetter(c) => $"Key{char.ToUpperInvariant(c)}",
            [var c] when char.IsAsciiDigit(c) => $"Digit{c}",
            _ => key,
        };

        private void Check(JsonNode? match, JsonNode? actual, string what, string at)
        {
            if (match is not null && !JsonCompare.Matches(match, actual, what, out var why)) Fail(at, why);
        }

        private KeyModifiers ModsOf(JsonNode step, string at) =>
            step["mods"]?.AsArray().Aggregate(KeyModifiers.None, (m, x) => m | x!.GetValue<string>() switch
            {
                "ctrl" => KeyModifiers.Control,
                "shift" => KeyModifiers.Shift,
                "alt" => KeyModifiers.Alt,
                "meta" => KeyModifiers.Meta,
                var other => throw new XunitException($"{runner}: {name}, {at}: unknown modifier {other}"),
            }) ?? KeyModifiers.None;

        /// <summary>
        /// Clicks the middle of the first occurrence of <paramref name="text"/> (of its first line): it is found, and
        /// the caret the host reports at its end and, collapsed, at its start gives the line.
        /// </summary>
        private async Task ClickAsync(string text, KeyModifiers mods, string at)
        {
            if (!(await _session.Find.NextAsync(new FindQuery(text), default)).Found) Fail(at, $"no {text} to click");
            await _adapter!.FlushAsync();
            var end = _adapter.Caret!.Bounds;
            await _session.RunAsync(EditorOperation.FromWire(new JsonObject { ["type"] = "moveSelection", ["by"] = "character", ["direction"] = "backward" }), default);
            await _adapter.FlushAsync();
            var start = _adapter.Caret!.Bounds;
            var x = start.Y == end.Y ? (start.X + end.X) / 2 : start.X + 2;
            var y = start.Y + start.Height / 2;
            await Input(() =>
            {
                _host!.OnPointer(PointerKind.Down, x, y, PointerButton.Primary, 1, mods);
                _host.OnPointer(PointerKind.Up, x, y, PointerButton.Primary, 1, mods);
            });
        }

        private Task Input(Action send)
        {
            send();
            return _session.WhenIdleAsync();
        }

        private async Task Settle(Task task, string at)
        {
            try { await task; }
            catch (XunitException) { throw; }
            catch (Exception e) { _failed = (e, at); }
        }

        private static string CodeOf(Exception e) => e switch
        {
            EditorException ee => JsonSerializer.Serialize(ee.Code, Ctx.EditorErrorCode).Trim('"'),
            OperationCanceledException => "aborted",
            _ => e.GetType().Name,
        };

        private static string Describe(Exception e) => $"{CodeOf(e)}: {e.Message}";

        private void Record(string type, JsonNode? payload)
        {
            var e = new JsonObject { ["type"] = type };
            if (payload is JsonObject fields)
            {
                foreach (var (key, value) in fields.ToArray())
                {
                    fields.Remove(key);
                    e[key] = value;
                }
            }
            lock (_events) _events.Add(e);
            lock (_listeners)
            {
                var index = _listeners.FindIndex(l => l.Type == type);
                if (index < 0) return;
                var (_, command, at) = _listeners[index];
                _listeners.RemoveAt(index);
                lock (_events) _background.Add(Listener(command, at));
            }
        }

        /// <summary>A listener's command; a failure fails the script at the listen step.</summary>
        private async Task Listener(JsonObject command, string at)
        {
            try { await _session.RunAsync(EditorOperation.FromWire(command), default); }
            catch (Exception e) when (e is not XunitException) { Fail($"{at} (listener)", $"failed: {Describe(e)}"); }
        }

        private void Subscribe()
        {
            _session.DocumentChanged += (_, e) =>
            {
                // The event's snapshot is at its revision; State may already be later, never earlier.
                var (revision, snapshot, now) = (e.Record.Revision, e.Snapshot.DocumentRevision, _session.State.DocumentRevision);
                if (snapshot != revision || now < revision)
                    _inconsistent ??= $"documentChanged at revision {revision}: its snapshot is at {snapshot} and State at {now}";
                Record("documentChanged", JsonSerializer.SerializeToNode(e.Record, Ctx.TransactionRecord));
            };
            _session.SelectionChanged += (_, e) => Record("selectionChanged", JsonSerializer.SerializeToNode(e.Selection, Ctx.SelectionSummary));
            _session.Warning += (_, e) => Record("warning", JsonSerializer.SerializeToNode(e.Warning, Ctx.Warning));
            _session.HostCommandRequested += (_, e) => Record("hostCommand", new JsonObject { ["command"] = WireJson.ToNode(e.Command) });
            _session.ShortcutPressed += (_, e) => Record("shortcut", new JsonObject { ["id"] = e.Id });
            _session.DialogRequested += (_, e) => Record("dialogRequested", new JsonObject { ["kind"] = WireJson.ToNode(e.Kind) });
            _session.LinkActivated += (_, e) => Record("linkActivated", new JsonObject { ["address"] = e.Address, ["via"] = WireJson.ToNode(e.Via) });
            _session.ContextMenuRequested += (_, e) => Record("contextMenu", new JsonObject { ["menu"] = WireJson.ToNode(e.Menu), ["via"] = WireJson.ToNode(e.Via) });
            _session.WordAdded += (_, e) => Record("wordAdded", new JsonObject { ["word"] = e.Word, ["language"] = e.Language });
            _session.FontsChanged += (_, e) => Record("fontsChanged", new JsonObject { ["pending"] = WireJson.ToNode(e.Pending), ["unresolved"] = WireJson.ToNode(e.Unresolved) });
            _session.ElementActivated += (_, e) => Record("elementActivated", new JsonObject { ["id"] = e.Id.Value });
            _session.RevisionsChanged += (_, _) => Record("revisionsChanged", null);
            _session.CommentsChanged += (_, _) => Record("commentsChanged", null);
            _session.AccessibilityAnnouncement += (_, e) => Record("announcement", new JsonObject { ["text"] = e.Text });
            _session.PlaceholderActivated += (_, e) => Record("placeholderActivated", JsonSerializer.SerializeToNode(new PlaceholderInfo(e.Key, e.Element), Ctx.PlaceholderInfo));
            _session.GestureRequested += (_, e) => Record("gesture", JsonSerializer.SerializeToNode(new GestureEvent(e.Gesture, e.Location, e.Page, e.Target), Ctx.GestureEvent));
            _session.ElementsChanged += (_, e) => Record("elementsChanged", JsonSerializer.SerializeToNode(new ElementsChangedPayload(e.Added, e.Removed, e.Changed), Ctx.ElementsChangedPayload));
            _session.FieldsUpdateRequested += (_, e) => Record("fieldsUpdateRequested", JsonSerializer.SerializeToNode(new FieldsUpdateRequestedPayload(e.Token, e.Fields), Ctx.FieldsUpdateRequestedPayload));
            _session.CommentAnchorsChanged += (_, e) => Record("commentAnchors", JsonSerializer.SerializeToNode(new CommentAnchorsPayload(e.Anchors), Ctx.CommentAnchorsPayload));
            _session.RulerChanged += (_, e) => Record("rulerChanged", JsonSerializer.SerializeToNode(e.View, Ctx.RulerView));
            _session.TooltipChanged += (_, e) => Record("tooltip", JsonSerializer.SerializeToNode(
                new TooltipPayload(e.Tooltip is { } t ? new TooltipWire(t.Kind, t.Text, t.Location, t.Bounds) : null), Ctx.TooltipPayload));
            _session.Faulted += (_, e) => Record("error", new JsonObject
            {
                ["code"] = e.Exception is EditorException ee ? WireJson.ToNode(ee.Code) : "internal",
                ["message"] = e.Exception.Message,
                ["fatal"] = e.IsFatal,
            });
        }

        private void Fail(string at, string why) => throw new XunitException($"{runner}: {name}, {at}: {why}");
    }
}
