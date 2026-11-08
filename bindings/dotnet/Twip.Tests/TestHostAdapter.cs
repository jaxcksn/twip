using System.Collections.Concurrent;
using Twip.Hosting;

namespace Twip.Tests;

/// <summary>A headless host: a UI thread of its own, and a frame buffer instead of a window.</summary>
internal sealed class TestHostAdapter : IEditorHostAdapter
{
    private readonly TestDispatcher _dispatcher = new();
    private int _calls;

    public IEditorDispatcher Dispatcher => _dispatcher;

    /// <summary>How many times the host called the adapter, besides <see cref="Dispatcher"/>.</summary>
    public int Calls => Volatile.Read(ref _calls);

    public int Presented { get; private set; }

    public (int Width, int Height) LastFrame { get; private set; }

    public EditorCursor Cursor { get; private set; }

    public ClipboardContent? Clipboard { get; set; }

    public CaretInfo? Caret { get; private set; }

    public (double X, double Y)? ScrolledTo { get; private set; }

    public AccessibleNode? Tree { get; private set; }

    public List<string> Announcements { get; } = [];

    /// <summary>Set when the host asked for a paint; the test paints by calling <see cref="EditorHost.Render"/>.</summary>
    public bool RenderRequested { get; set; }

    public void RequestRender() { Called(); RenderRequested = true; }

    public void Present(in FrameView frame)
    {
        Called();
        Presented++;
        LastFrame = (frame.Width, frame.Height);
    }

    public void SetCursor(EditorCursor cursor) { Called(); Cursor = cursor; }

    public void CapturePointer() => Called();

    public void ReleasePointer() => Called();

    public void SetClipboard(ClipboardContent content) { Called(); Clipboard = content; }

    public ValueTask<ClipboardContent?> GetClipboardAsync() { Called(); return new(Clipboard); }

    public void SetCaret(CaretInfo caret) { Called(); Caret = caret; }

    public void ScrollTo(double x, double y) { Called(); ScrolledTo = (x, y); }

    public void ShowTooltip(TooltipInfo? tooltip) => Called();

    public void UpdateAccessibility(AccessibleNode tree) { Called(); Tree = tree; }

    public void Announce(string text) { Called(); Announcements.Add(text); }

    /// <summary>Completes once what the host posted before it has run.</summary>
    public Task FlushAsync()
    {
        var done = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        _dispatcher.Post(done.SetResult);
        return done.Task;
    }

    /// <summary>Runs <paramref name="action"/> on the UI thread.</summary>
    public Task RunAsync(Action action)
    {
        var done = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        _dispatcher.Post(() =>
        {
            try { action(); done.SetResult(); }
            catch (Exception e) { done.SetException(e); }
        });
        return done.Task;
    }

    private void Called() => Interlocked.Increment(ref _calls);

    private sealed class TestDispatcher : IEditorDispatcher
    {
        private readonly BlockingCollection<Action> _queue = [];
        private readonly Thread _thread;

        public TestDispatcher()
        {
            _thread = new Thread(() =>
            {
                foreach (var action in _queue.GetConsumingEnumerable()) action();
            }) { IsBackground = true, Name = "Test UI" };
            _thread.Start();
        }

        public void Post(Action action) => _queue.Add(action);

        public bool CheckAccess() => Thread.CurrentThread == _thread;
    }
}
