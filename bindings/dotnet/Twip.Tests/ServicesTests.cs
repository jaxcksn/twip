using System.Text.Json;
using System.Text.Json.Nodes;
using Twip.Native;
using Twip.Wire;
using Xunit;

namespace Twip.Tests;

/// <summary>
/// Every service member sends a request the engine reads: it may answer, or fail with its own code (a service that
/// hasn't landed is <c>unsupported</c>), but never with a bad request.
/// </summary>
public sealed class ServicesTests
{
    private static CancellationToken Ct => TestContext.Current.CancellationToken;

    private static JsonNode Wire(string fixture) => JsonNode.Parse(File.ReadAllText(Repo.Path("crates", "twip", "tests", "wire", fixture + ".json")))![0]!;

    private static T Wire<T>(string fixture) => Wire(fixture).Deserialize(WireJson.Info<T>())!;

    public static TheoryData<string> Members() =>
    [
        "menus.contextMenu", "menus.invoke", "dialogs.open", "dialogs.update", "dialogs.apply", "dialogs.accessibility",
        "find.next", "find.previous", "find.all", "find.select", "find.replace", "find.replaceAll",
        "spelling.addDictionary", "spelling.setEnabled", "spelling.setLanguage", "spelling.ignore", "spelling.add",
        "spelling.loadUserWords", "spelling.suggestions", "spelling.next", "navigation.headings", "navigation.select",
        "styles.list", "styles.listOne", "review.revisions", "review.comments", "review.people", "elements.list",
        "elements.listFiltered", "elements.get", "elements.structure", "elements.select", "elements.placeholders",
        "dataParts.list", "dataParts.get", "accessibility.tree", "gestures.bindings", "gestures.set", "gestures.reset",
        "shortcuts.register", "shortcuts.lookup", "wordCount",
    ];

    [Theory]
    [MemberData(nameof(Members))]
    public async Task TheEngineReadsTheRequest(string member)
    {
        Native.Require();
        await using var session = new EditorSession();
        await session.Commands.InsertTextAsync("teh total", Ct);
        try
        {
            await Call(session, member);
        }
        catch (EditorException e) when (e.Code != EditorErrorCode.InvalidArgument || !e.Message.Contains("bad request", StringComparison.Ordinal))
        {
            // The engine read the request; its answer is the service's business.
        }
    }

    [Fact]
    public void ADialogUpdateSendsOnlyTheChangedFields()
    {
        var model = Wire("dialog-model").Deserialize(TwipJsonContext.Default.DialogModel)!; // insertTable, 2 rows, 5 columns
        var dialog = new DialogSession<InsertTableDialogValues>(new EditorSession(), model);
        Assert.Equal("""{"kind":"insertTable","rows":3}""", dialog.Patch(dialog.Values with { Rows = 3 }).ToJsonString());
        Assert.Equal("""{"kind":"insertTable"}""", dialog.Patch(dialog.Values).ToJsonString());
    }

    [Fact]
    public async Task ABadRequestIsCaught()
    {
        Native.Require();
        await using var session = new EditorSession();
        var e = await Assert.ThrowsAsync<EditorException>(() => session.CallAsync(Method.Find, """{"query":{"text":"a"},"direction":"sideways"}"""u8.ToArray(), Ct));
        Assert.Equal(EditorErrorCode.InvalidArgument, e.Code);
        Assert.Contains("bad request", e.Message, StringComparison.Ordinal);
    }

    private static async Task Call(EditorSession s, string member)
    {
        var query = new FindQuery("total", MatchCase: true);
        var model = Wire("dialog-model").Deserialize(TwipJsonContext.Default.DialogModel)!;
        switch (member)
        {
            case "menus.contextMenu": await s.Menus.GetContextMenuAsync(Ct); break;
            case "menus.invoke": await s.Menus.InvokeAsync(new MenuAction(Wire("menu-action")), Ct); break;
            case "dialogs.open": await s.Dialogs.OpenInsertTableAsync(Ct); break;
            case "dialogs.update": await new DialogSession<InsertTableDialogValues>(s, model).UpdateAsync(new InsertTableDialogValues(3, 4), Ct); break;
            case "dialogs.apply": await new DialogSession<InsertTableDialogValues>(s, model).ApplyAsync(Ct); break;
            case "dialogs.accessibility": await s.Dialogs.OpenAccessibilityCheckAsync(Ct); break;
            case "find.next": await s.Find.NextAsync(query, Ct); break;
            case "find.previous": await s.Find.PreviousAsync(query, Ct); break;
            case "find.all": await s.Find.FindAllAsync(query, Ct); break;
            case "find.select": await s.Find.SelectAsync(Wire<MatchId>("match-id"), Ct); break;
            case "find.replace": await s.Find.ReplaceAsync(query, "sum", Ct); break;
            case "find.replaceAll": await s.Find.ReplaceAllAsync(query, "sum", Ct); break;
            case "spelling.addDictionary": await s.Spelling.AddDictionaryAsync(Wire<Dictionary>("dictionary"), Ct); break;
            case "spelling.setEnabled": await s.Spelling.SetEnabledAsync(true); break;
            case "spelling.setLanguage": await s.Spelling.SetLanguageAsync("en-GB"); break;
            case "spelling.ignore": await s.Spelling.IgnoreAsync("teh"); break;
            case "spelling.add": await s.Spelling.AddAsync("Twip"); break;
            case "spelling.loadUserWords": await s.Spelling.LoadUserWordsAsync("en-US", ["Twip", "Ada"]); break;
            case "spelling.suggestions": await s.Spelling.GetSuggestionsAsync("teh", "en-US"); break;
            case "spelling.next": await s.Spelling.NextAsync(Ct); break;
            case "navigation.headings": await s.Navigation.GetHeadingsAsync(Ct); break;
            case "navigation.select": await s.Navigation.SelectAsync(Wire<HeadingId>("heading-id"), Ct); break;
            case "styles.list": await s.Styles.ListAsync(ct: Ct); break;
            case "styles.listOne": await s.Styles.ListAsync(StyleType.Paragraph, Ct); break;
            case "review.revisions": await s.Review.GetRevisionsAsync(Ct); break;
            case "review.comments": await s.Review.GetCommentsAsync(Ct); break;
            case "review.people": await s.Review.GetPeopleAsync(Ct); break;
            case "elements.list": await s.Elements.ListAsync(ct: Ct); break;
            case "elements.placeholders": Assert.Empty(await s.Elements.PlaceholdersAsync(Ct)); break;
            case "elements.listFiltered": await s.Elements.ListAsync(Wire<ElementFilter>("element-filter"), Ct); break;
            case "elements.get": await s.Elements.GetAsync(Wire<ElementId>("element-id"), Ct); break;
            case "elements.structure": await s.Elements.GetStructureAsync(Wire<ElementId>("element-id"), 2, Ct); break;
            case "elements.select": await s.Elements.SelectAsync(Wire<ElementId>("element-id"), ct: Ct); break;
            case "dataParts.list": await s.DataParts.ListAsync(Ct); break;
            case "dataParts.get": await s.DataParts.GetAsync("urn:invoice", "{6A9C}", Ct); break;
            case "accessibility.tree": await s.Accessibility.GetTreeAsync(Ct); break;
            case "gestures.bindings": _ = s.Gestures.Bindings; break;
            case "gestures.set": await s.Gestures.SetAsync(GestureId.HeaderAreaDoubleClick, GestureAction.Run(EditorOperation.SelectAll())); break;
            case "gestures.reset": await s.Gestures.ResetAsync(GestureId.HeaderAreaDoubleClick); await s.Gestures.ResetAsync(); break;
            case "shortcuts.register": s.Shortcuts.Register("lock", "Ctrl+Shift+L", "Cmd+Shift+L").Dispose(); break;
            case "shortcuts.lookup": _ = s.Shortcuts.Lookup("Ctrl+B"); break;
            case "wordCount": await s.GetWordCountAsync(Ct); break;
            default: throw new Xunit.Sdk.XunitException($"no call for {member}");
        }
    }

    [Fact]
    public void BalloonsStackWithoutOverlap()
    {
        static CommentAnchor At(double lineTop) => new(new CommentId("c"), 1, false, false, default, default, lineTop, lineTop + 10, 0);
        var review = new EditorSession().Review;
        Assert.Equal([10.0, 64.0, 200.0], review.StackBalloons([At(10), At(20), At(200)], [50, 30, 10], 4));
        Assert.Empty(review.StackBalloons([], []));

    }

    public static TheoryData<string> BalloonCases()
    {
        var data = new TheoryData<string>();
        foreach (var c in JsonNode.Parse(File.ReadAllText(Repo.Path("crates", "twip", "tests", "wire", "balloons.json")))!.AsArray()) data.Add(c!.ToJsonString());
        return data;
    }

    /// <summary>Each of the engine's cases in <c>balloons.json</c>: its anchors, card heights and gap give its tops.</summary>
    [Theory]
    [MemberData(nameof(BalloonCases))]
    public void BalloonsStackAsTheEngineDoes(string json)
    {
        var c = JsonNode.Parse(json)!;
        var anchors = c["anchors"].Deserialize(TwipJsonContext.Default.IReadOnlyListCommentAnchor)!;
        var heights = c["heights"]!.AsArray().Select(h => h!.GetValue<double>()).ToList();
        var gap = c["gap"]?.GetValue<double>() ?? 8;
        var tops = new EditorSession().Review.StackBalloons(anchors, heights, gap);
        Assert.True(c["tops"]!.AsArray().Select(t => t!.GetValue<double>()).SequenceEqual(tops), $"{c["name"]}: got [{string.Join(", ", tops)}]");
    }
}
