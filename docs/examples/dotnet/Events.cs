using Twip;

namespace Examples;

static class EventsExample
{
    static void Listen(EditorSession session, Action save)
    {
        #region events
        // Coalesced: the latest snapshot after one or more changes. Bind a toolbar here.
        session.StateChanged += (_, e) => Console.WriteLine($"page {e.Snapshot.Pages.Current} of {e.Snapshot.Pages.Count}");

        // Exactly one per commit, in order.
        session.DocumentChanged += (_, e) => Console.WriteLine($"{e.Record.Revision} {e.Record.Origin} {e.Record.Label}");

        // Keys the application owns, such as Ctrl+S.
        session.HostCommandRequested += (_, e) =>
        {
            if (e.Command == HostCommand.Save) save();
        };

        session.Warning += (_, e) => Console.WriteLine(e.Warning.Message);

        // A handler's exception lands here instead of breaking the session. A fault has IsFatal set.
        session.Faulted += (_, e) => Console.Error.WriteLine($"{(e.IsFatal ? "fault" : "handler")}: {e.Exception.Message}");
        #endregion
    }
}
