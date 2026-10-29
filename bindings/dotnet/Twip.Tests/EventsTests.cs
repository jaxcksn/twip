using Twip.Engine;
using Xunit;

namespace Twip.Tests;

/// <summary>Event delivery on an unattached session's own executor: isolation, snapshots, unsubscribing and disposal.</summary>
public sealed class EventsTests
{
    private static CancellationToken Ct => TestContext.Current.CancellationToken;

    [Fact]
    public async Task AThrowingHandlerDoesNotStopTheOthers()
    {
        Native.Require();
        await using var session = new EditorSession();
        var after = 0;
        var faults = new List<FaultedEventArgs>();
        session.DocumentChanged += (_, _) => throw new InvalidOperationException("handler bug");
        session.DocumentChanged += (_, _) => after++;
        session.Faulted += (_, e) => faults.Add(e);

        await session.Commands.InsertTextAsync("a", Ct);
        await session.WhenIdleAsync();

        Assert.Equal(1, after);
        var fault = Assert.Single(faults);
        Assert.False(fault.IsFatal);
        Assert.Equal("handler bug", Assert.IsType<InvalidOperationException>(fault.Exception).Message);
    }

    [Fact]
    public async Task StateInAHandlerIsAlreadyAtTheEventsRevision()
    {
        Native.Require();
        await using var session = new EditorSession();
        var seen = new List<(long Record, long Snapshot, long State, bool EngineThread)>();
        session.DocumentChanged += (s, e) =>
            seen.Add((e.Record.Revision, e.Snapshot.DocumentRevision, ((EditorSession)s!).State.DocumentRevision, EngineThread.Instance.IsCurrent));

        await session.Commands.InsertTextAsync("a", Ct);
        await session.Commands.InsertTextAsync("b", Ct);
        await session.WhenIdleAsync();

        Assert.Equal(2, seen.Count);
        foreach (var (record, snapshot, state, onEngine) in seen)
        {
            Assert.Equal(record, snapshot);
            Assert.True(state >= record, $"State is at {state}, before the event's revision {record}");
            Assert.False(onEngine, "a handler ran on the engine thread");
        }
        Assert.True(seen[1].Record > seen[0].Record);
    }

    [Fact]
    public async Task UnsubscribedHandlersAreNotCalled()
    {
        Native.Require();
        await using var session = new EditorSession();
        var calls = 0;
        void Count(object? sender, DocumentChangedEventArgs e) => calls++;
        session.DocumentChanged += Count;
        await session.Commands.InsertTextAsync("a", Ct);
        await session.WhenIdleAsync();
        session.DocumentChanged -= Count;
        await session.Commands.InsertTextAsync("b", Ct);
        await session.WhenIdleAsync();

        Assert.Equal(1, calls);
    }

    [Fact]
    public async Task DisposingInAHandlerStopsDispatch()
    {
        Native.Require();
        var session = new EditorSession();
        var events = 0;
        var faults = 0;
        session.Faulted += (_, _) => faults++;
        session.DocumentChanged += (s, _) =>
        {
            events++;
            ((EditorSession)s!).Dispose();
        };
        session.StateChanged += (_, _) => { };

        // Both commands are queued before the first one's event disposes the session.
        var first = session.Commands.InsertTextAsync("a", Ct);
        var second = session.Commands.InsertTextAsync("b", Ct);
        await first;
        // A command completes before its events are delivered: let the first one's handler run.
        await session.WhenIdleAsync();
        try
        {
            // It ran before the dispose reached the engine (its event is dropped), or it failed as destroyed.
            await second.WaitAsync(TimeSpan.FromSeconds(10), Ct);
        }
        catch (EditorException e)
        {
            Assert.Equal(EditorErrorCode.Destroyed, e.Code);
        }
        await session.DisposeAsync();
        await session.WhenIdleAsync();

        Assert.Equal(1, events);
        Assert.Equal(0, faults);
        var late = await Assert.ThrowsAsync<EditorException>(() => session.Commands.InsertTextAsync("c", Ct));
        Assert.Equal(EditorErrorCode.Destroyed, late.Code);
    }
}
