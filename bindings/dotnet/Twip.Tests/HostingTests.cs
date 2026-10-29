using System.IO.Compression;
using System.Text.RegularExpressions;
using Twip.Hosting;
using Xunit;

namespace Twip.Tests;

/// <summary>A document's text, read from its DOCX.</summary>
internal static partial class Docx
{
    public static async Task<string> TextAsync(EditorSession session)
    {
        var docx = new MemoryStream();
        await session.SaveAsync(docx);
        using var zip = new ZipArchive(docx);
        using var reader = new StreamReader(zip.GetEntry("word/document.xml")!.Open());
        var xml = await reader.ReadToEndAsync();
        return string.Concat(TextRun().Matches(xml).Cast<Match>().Select(m => System.Net.WebUtility.HtmlDecode(m.Groups[1].Value)));
    }

    private static readonly Regex s_textRun = new("<w:t(?: [^>]*)?>([^<]*)</w:t>");

    private static Regex TextRun() => s_textRun;
}

/// <summary>Attaching a presentation to a session, through a headless adapter.</summary>
public sealed class HostingTests
{
    private static CancellationToken Ct => TestContext.Current.CancellationToken;

    [Fact]
    public async Task AttachAndDetachKeepTheDocumentSelectionAndHistory()
    {
        Native.Require();
        await using var session = new EditorSession();
        await session.Commands.InsertTextAsync("Hello", Ct);
        await session.Commands.SelectAllAsync(Ct);

        var adapter = new TestHostAdapter();
        var host = session.Attach(adapter);
        host.OnResize(800, 600, 96);
        await session.WhenIdleAsync();
        await adapter.FlushAsync();
        // Attaching hands the host the caret and the accessibility tree.
        Assert.NotNull(adapter.Caret);
        Assert.NotNull(adapter.Tree);
        var attached = session.State;
        host.Dispose();

        var again = session.Attach(new TestHostAdapter());
        await session.WhenIdleAsync();
        Assert.Equal(attached.DocumentRevision, session.State.DocumentRevision);
        Assert.False(session.State.Selection.Empty);
        Assert.True(session.State.History.CanUndo);
        again.Dispose();
        Assert.Equal("Hello", await Docx.TextAsync(session));
    }

    [Fact]
    public async Task ASecondAttachIsRejected()
    {
        Native.Require();
        await using var session = new EditorSession();
        using var host = session.Attach(new TestHostAdapter());
        var error = Assert.Throws<EditorException>(() => session.Attach(new TestHostAdapter()));
        Assert.Equal(EditorErrorCode.Unavailable, error.Code);
        Assert.Contains("another host", error.Message, StringComparison.Ordinal);
    }

    [Fact]
    public async Task EventsArriveOnTheDispatcherWhenAttachedAndOnTheExecutorWhenNot()
    {
        Native.Require();
        await using var session = new EditorSession();
        var adapter = new TestHostAdapter();

        async Task<bool> OnUiThreadAsync()
        {
            var seen = new TaskCompletionSource<bool>(TaskCreationOptions.RunContinuationsAsynchronously);
            void Handler(object? sender, StateChangedEventArgs e) => seen.TrySetResult(adapter.Dispatcher.CheckAccess());
            session.StateChanged += Handler;
            try
            {
                await session.Commands.InsertTextAsync("x", Ct);
                return await seen.Task.WaitAsync(TimeSpan.FromSeconds(30), Ct);
            }
            finally
            {
                session.StateChanged -= Handler;
            }
        }

        var host = session.Attach(adapter);
        Assert.True(await OnUiThreadAsync(), "an attached session raised an event off the adapter's dispatcher");
        host.Dispose();
        Assert.False(await OnUiThreadAsync(), "a detached session still raised events on the adapter's dispatcher");
    }

    [Fact]
    public async Task OperationsFromEightThreadsRunInSubmissionOrder()
    {
        Native.Require();
        await using var session = new EditorSession();
        using var host = session.Attach(new TestHostAdapter());
        var gate = new object();
        var submitted = new List<string>();
        var tasks = new List<Task>();
        var threads = Enumerable.Range(0, 8).Select(t => new Thread(() =>
        {
            for (var i = 0; i < 5; i++)
            {
                // Submission is the call itself: the lock fixes the order the operations entered the queue in.
                lock (gate)
                {
                    var token = $"[{t}{i}]";
                    submitted.Add(token);
                    tasks.Add(session.Commands.InsertTextAsync(token, Ct));
                }
            }
        })).ToList();
        threads.ForEach(t => t.Start());
        threads.ForEach(t => t.Join());
        await Task.WhenAll(tasks);
        Assert.Equal(string.Concat(submitted), await Docx.TextAsync(session));
    }

    [Fact]
    public async Task DisposeWithPendingWorkNeitherDeadlocksNorCallsBack()
    {
        Native.Require();
        var session = new EditorSession();
        var adapter = new TestHostAdapter();
        var host = session.Attach(adapter);
        host.OnResize(800, 600, 96);
        var pending = Enumerable.Range(0, 50).Select(i => session.Commands.InsertTextAsync($"{i} ", Ct)).ToList();

        // On the UI thread, as a form closing would.
        var calls = 0;
        await adapter.RunAsync(() =>
        {
            session.Dispose();
            calls = adapter.Calls;
        }).WaitAsync(TimeSpan.FromSeconds(10), Ct);

        await session.DisposeAsync();
        // The native shutdown ran after everything queued before it.
        Assert.All(pending, t => Assert.True(t.IsCompleted));
        await adapter.FlushAsync();
        Assert.Equal(calls, adapter.Calls);
        await Assert.ThrowsAsync<EditorException>(() => session.Commands.InsertTextAsync("late", Ct));
    }

    [Fact]
    public async Task ACopyPastesBackWithItsFormatting()
    {
        Native.Require();
        await using var session = new EditorSession();
        var adapter = new TestHostAdapter();
        using var host = session.Attach(adapter);
        host.OnResize(800, 600, 96);
        await session.Commands.InsertTextAsync("abc", Ct);
        await session.Commands.SelectAllAsync(Ct);
        await session.Commands.CopyAsync(Ct);
        await session.WhenIdleAsync();
        await adapter.FlushAsync();
        // The host marks the copy as its own, so the paste is the engine's fragment, not the plain text.
        Assert.Equal("abc", adapter.Clipboard?.Text);
        Assert.NotNull(adapter.Clipboard?.Native);

        host.OnKey(EditorKey.End, KeyModifiers.Control, out _);
        host.OnKey(EditorKey.KeyV, KeyModifiers.Control, out var handled);
        Assert.True(handled);
        await session.WhenIdleAsync();
        // A pasted word gets the space it needs beside its neighbour.
        Assert.Equal("abc abc", await Docx.TextAsync(session));
    }

    [Fact]
    public async Task ScrollRequestsAndPresentationSettingsReachTheAdapterAndEngine()
    {
        Native.Require();
        await using var session = new EditorSession();
        var adapter = new TestHostAdapter();
        using var host = session.Attach(adapter);
        host.OnResize(800, 300, 96);
        await session.Commands.InsertTextAsync("Line", Ct);
        for (var i = 0; i < 60; i++) await session.Commands.InsertParagraphAsync(Ct);
        await session.WhenIdleAsync();

        // The caret is far below a 300 px view: revealing it asks the adapter to scroll down.
        await host.RevealSelectionAsync();
        Assert.True(adapter.ScrolledTo is { Y: > 0 }, $"scrolled to {adapter.ScrolledTo}");

        host.Background = System.Drawing.Color.FromArgb(0x20, 0x20, 0x20);
        host.MarkupPaneWidth = 200;
        Assert.NotEmpty(host.VisiblePages);
        Assert.NotNull(host.RulerView);
        var hit = await host.HitTestAsync(400, 10);
        Assert.NotNull(hit);
    }
}
