using System.Collections.Concurrent;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using System.Text;
using Twip.Native;
using Twip.Wire;

namespace Twip.Engine;

/// <summary>
/// The one thread per process that owns the native engines. Work runs in submission order; callbacks an engine makes
/// during a call are copied, then handed to their sessions after the call returns, so a session never calls the
/// engine from inside a callback. A work item's task completes after that hand-off, so state is published first.
/// </summary>
internal sealed unsafe class EngineThread
{
    private static readonly Lazy<EngineThread> Shared = new(() => new EngineThread(), LazyThreadSafetyMode.ExecutionAndPublication);

    private readonly BlockingCollection<Action> _queue = [];
    private readonly List<(NativeEngine Engine, uint Session, CallbackKind Kind, byte[] Payload)> _pending = [];
    private readonly Thread _thread;

    private EngineThread()
    {
        _thread = new Thread(Run) { IsBackground = true, Name = "Twip engine" };
        _thread.Start();
    }

    public static EngineThread Instance => Shared.Value;

    public bool IsCurrent => Thread.CurrentThread == _thread;

    /// <summary>Runs <paramref name="work"/> on the engine thread, in order.</summary>
    public Task<T> Post<T>(Func<T> work, CancellationToken ct = default)
    {
        if (ct.IsCancellationRequested) return Task.FromCanceled<T>(ct);
        var tcs = new TaskCompletionSource<T>(TaskCreationOptions.RunContinuationsAsynchronously);
        var registration = ct.CanBeCanceled ? ct.Register(() => tcs.TrySetCanceled(ct)) : default;
        _queue.Add(() =>
        {
            registration.Dispose();
            if (tcs.Task.IsCompleted) return; // cancelled while queued: never runs
            try
            {
                var result = work();
                Deliver();
                tcs.TrySetResult(result);
            }
            catch (Exception e)
            {
                Deliver();
                tcs.TrySetException(e);
            }
        });
        return tcs.Task;
    }

    /// <summary>Runs <paramref name="work"/> on the engine thread, in order, without waiting.</summary>
    public void Post(Action work) => _queue.Add(() =>
    {
        try { work(); }
        finally { Deliver(); }
    });

    /// <summary>Calls a structured method: request JSON in, the response's <c>ok</c> value out.</summary>
    public static System.Text.Json.JsonElement? Call(nint engine, uint session, Method method, ReadOnlySpan<byte> request)
    {
        TwipBuf output = default;
        int status;
        fixed (byte* p = request)
        {
            status = TwipNative.Call(engine, session, (uint)method, p, (nuint)request.Length, &output);
        }
        try { return WireJson.Unwrap(output.Span, status); }
        finally { Free(ref output); }
    }

    public static void Free(ref TwipBuf buf)
    {
        if (buf.Ptr == 0) return;
        fixed (TwipBuf* p = &buf) TwipNative.BufFree(p);
        buf = default;
    }

    public static byte[] Utf8(string text) => Encoding.UTF8.GetBytes(text);

    private void Run()
    {
        foreach (var work in _queue.GetConsumingEnumerable())
        {
            try { work(); }
            catch
            {
                // Work items report their own failures; nothing may stop the engine thread.
            }
        }
    }

    /// <summary>Hands the callbacks of the last call to their sessions, in the order the engine made them.</summary>
    private void Deliver()
    {
        if (_pending.Count == 0) return;
        var batch = _pending.ToArray();
        _pending.Clear();
        foreach (var (engine, session, kind, payload) in batch) engine.Deliver(session, kind, payload);
    }

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    internal static void OnCallback(nint ctx, uint session, uint kind, byte* ptr, nuint len)
    {
        try
        {
            // The payload is valid only during the call: copy it.
            var payload = len == 0 ? [] : new ReadOnlySpan<byte>(ptr, checked((int)len)).ToArray();
            Instance._pending.Add(((NativeEngine)GCHandle.FromIntPtr(ctx).Target!, session, (CallbackKind)kind, payload));
        }
        catch
        {
            // An exception must never cross into native code.
        }
    }
}

/// <summary>
/// One native engine, on the engine thread: a font library and the sessions that lay out with it. The process has
/// one per <see cref="FontLibrary"/>, so sessions on different libraries don't share fonts (or the internal clipboard).
/// </summary>
internal sealed unsafe class NativeEngine(FontLibrary library, string fonts)
{
    private readonly Dictionary<uint, ICallbackSink> _sinks = [];
    private nint _handle;
    private GCHandle _self;

    /// <summary>The engine handle. Engine thread only; creates the engine on first use.</summary>
    public nint Handle
    {
        get
        {
            if (_handle != 0) return _handle;
            _self = GCHandle.Alloc(this);
            var options = WireJson.ToUtf8(new EngineOptionsWire(fonts));
            fixed (byte* p = options)
            {
                _handle = TwipNative.EngineNew(p, (nuint)options.Length, &EngineThread.OnCallback, GCHandle.ToIntPtr(_self));
            }
            if (_handle == 0) throw new EditorException(EditorErrorCode.Internal, "the engine failed to start");
            return _handle;
        }
    }

    public void Register(uint session, ICallbackSink sink) => _sinks[session] = sink;

    public void Unregister(uint session) => _sinks.Remove(session);

    public System.Text.Json.JsonElement? Call(uint session, Method method, ReadOnlySpan<byte> request) =>
        EngineThread.Call(Handle, session, method, request);

    public void Deliver(uint session, CallbackKind kind, byte[] payload)
    {
        if (kind == CallbackKind.FontRequest)
        {
            library.OnFontRequest(payload);
            return;
        }
        if (!_sinks.TryGetValue(session, out var sink)) return;
        try { sink.OnCallback(kind, payload); }
        catch (Exception e) { sink.OnCallbackFailed(e); }
    }
}

/// <summary>Receives a session's engine callbacks, on the engine thread, after the call that made them.</summary>
internal interface ICallbackSink
{
    void OnCallback(CallbackKind kind, byte[] payload);

    void OnCallbackFailed(Exception exception);
}

/// <summary>Runs notifications one at a time, in order, on the thread pool: the executor of an unattached session.</summary>
internal sealed class SerialExecutor
{
    private readonly Queue<Action> _queue = new();
    private bool _running;

    public void Post(Action action)
    {
        lock (_queue)
        {
            _queue.Enqueue(action);
            if (_running) return;
            _running = true;
        }
        ThreadPool.UnsafeQueueUserWorkItem(static self => self.Drain(), this, preferLocal: false);
    }

    private void Drain()
    {
        while (true)
        {
            Action action;
            lock (_queue)
            {
                if (_queue.Count == 0) { _running = false; return; }
                action = _queue.Dequeue();
            }
            action();
        }
    }
}
