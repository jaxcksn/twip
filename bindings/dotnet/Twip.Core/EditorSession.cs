using System.Text.Json;
using Twip.Engine;
using Twip.Hosting;
using Twip.Native;
using Twip.Wire;

namespace Twip;

/// <summary>
/// One document with its selection, undo history and ordered editing pipeline. A new session is a blank document;
/// the engine starts lazily, and operations wait for it. Operations may be called from any thread and run in the
/// order they were submitted. An unattached session is headless; <see cref="Attach"/> gives it a presentation.
/// </summary>
public sealed class EditorSession : IDisposable, IAsyncDisposable, ICallbackSink
{
    private static IReadOnlyList<CommandInfo>? s_catalog;
    private readonly EditorSessionOptions _options;
    private readonly SerialExecutor _executor = new();
    private volatile EditorState _state = new();
    private volatile bool _disposed;
    private EditorHost? _host;
    private uint _id; // engine thread only
    private long _latestState;
    private IReadOnlyList<CommentAnchor> _anchors = [];
    private Task? _closed;

    /// <summary>Creates a session over a blank document. Construction is lightweight and loads no native code.</summary>
    /// <param name="options">The session options.</param>
    public EditorSession(EditorSessionOptions? options = null)
    {
        _options = options ?? new EditorSessionOptions();
        Commands = new EditorCommands(this);
        Menus = new EditorMenus(this);
        Dialogs = new EditorDialogs(this);
        Find = new EditorFind(this);
        Spelling = new EditorSpelling(this);
        Navigation = new EditorNavigation(this);
        Styles = new EditorStyles(this);
        Review = new EditorReview(this);
        Elements = new EditorElements(this);
        DataParts = new EditorDataParts(this);
        Accessibility = new EditorAccessibility(this);
        Shortcuts = new EditorShortcuts(this);
        Gestures = new EditorGestures(this);
        Fonts = _options.Fonts ?? FontLibrary.System;
        _state = new EditorState { ReadOnly = _options.ReadOnly, Mode = _options.Mode };
    }

    // Documents

    /// <summary>Opens a DOCX file, replacing the document only when it reads successfully.</summary>
    /// <param name="path">The file.</param>
    /// <param name="ct">Cancels the open; the current document is kept.</param>
    /// <returns>The import warnings.</returns>
    public async Task<ImportResult> OpenAsync(string path, CancellationToken ct = default) =>
        await OpenBytesAsync(await File.ReadAllBytesAsync(path, ct).ConfigureAwait(false), ct).ConfigureAwait(false);

    /// <summary>Opens a DOCX stream, replacing the document only when it reads successfully. The stream stays open.</summary>
    /// <param name="stream">The stream, read from its current position.</param>
    /// <param name="ct">Cancels the open; the current document is kept.</param>
    /// <returns>The import warnings.</returns>
    public async Task<ImportResult> OpenAsync(Stream stream, CancellationToken ct = default)
    {
        ArgumentNullException.ThrowIfNull(stream);
        using var buffer = new MemoryStream();
        await stream.CopyToAsync(buffer, ct).ConfigureAwait(false);
        return await OpenBytesAsync(buffer.ToArray(), ct).ConfigureAwait(false);
    }

    /// <summary>Replaces the document with a blank one.</summary>
    /// <param name="ct">Cancels before it runs.</param>
    /// <returns>Completes when the blank document is in place.</returns>
    public Task NewDocumentAsync(CancellationToken ct = default) => CallAsync(Method.NewDocument, [], ct);

    /// <summary>Saves DOCX to a file. The file is replaced safely: a failed save leaves the old file.</summary>
    /// <param name="path">The file.</param>
    /// <param name="ct">Cancels the save.</param>
    /// <returns>The revision saved, and the export warnings.</returns>
    public Task<SaveResult> SaveAsync(string path, CancellationToken ct = default) => ExportToFileAsync("docx", path, ct);

    /// <summary>Saves DOCX to a stream. The stream stays open; a failed write may leave partial output.</summary>
    /// <param name="stream">The stream.</param>
    /// <param name="ct">Cancels the save.</param>
    /// <returns>The revision saved, and the export warnings.</returns>
    public Task<SaveResult> SaveAsync(Stream stream, CancellationToken ct = default) => ExportToStreamAsync("docx", stream, ct);

    /// <summary>Exports PDF to a file, replaced safely.</summary>
    /// <param name="path">The file.</param>
    /// <param name="ct">Cancels the export.</param>
    /// <returns>The revision exported, and the export warnings.</returns>
    public Task<SaveResult> ExportPdfAsync(string path, CancellationToken ct = default) => ExportToFileAsync("pdf", path, ct);

    /// <summary>Exports PDF to a stream. The stream stays open.</summary>
    /// <param name="stream">The stream.</param>
    /// <param name="ct">Cancels the export.</param>
    /// <returns>The revision exported, and the export warnings.</returns>
    public Task<SaveResult> ExportPdfAsync(Stream stream, CancellationToken ct = default) => ExportToStreamAsync("pdf", stream, ct);

    // Edits

    /// <summary>The commands: one method per command.</summary>
    public EditorCommands Commands { get; }

    /// <summary>Runs operations atomically: one transaction and one undo step. If one fails, none apply.</summary>
    /// <param name="operations">The operations, from the <see cref="EditorOperation"/> factories.</param>
    /// <param name="label">The undo label.</param>
    /// <param name="ct">Cancels the batch before it runs.</param>
    /// <returns>Completes when the batch has committed.</returns>
    public Task BatchAsync(IReadOnlyList<EditorOperation> operations, string? label = null, CancellationToken ct = default)
    {
        ArgumentNullException.ThrowIfNull(operations);
        return CallAsync(Method.Batch, WireJson.ToUtf8(new BatchRequest(operations, label)), ct);
    }

    // State

    /// <summary>The latest immutable snapshot. When an operation completes, this is at its revision or later.</summary>
    public EditorState State => _state;

    /// <summary>Every command with its label, group, shortcuts and undo grouping.</summary>
    /// <remarks>Read once from the engine, which starts if it hasn't yet.</remarks>
    public IReadOnlyList<CommandInfo> Catalog
    {
        get
        {
            if (Volatile.Read(ref s_catalog) is { } catalog) return catalog;
            IReadOnlyList<CommandInfo> Load() => Fonts.Native.Call(0, Method.Catalog, [])?.Deserialize(TwipJsonContext.Default.IReadOnlyListCommandInfo) ?? [];
            catalog = EngineThread.Instance.IsCurrent ? Load() : EngineThread.Instance.Post(Load).GetAwaiter().GetResult();
            return Interlocked.CompareExchange(ref s_catalog, catalog, null) ?? catalog;
        }
    }

    // Settings

    /// <summary>Makes the session read only, or editable again.</summary>
    /// <param name="value">Whether it is read only.</param>
    /// <returns>Completes when the setting applies.</returns>
    public Task SetReadOnlyAsync(bool value) => CallAsync(Method.SetReadOnly, value ? "true"u8.ToArray() : "false"u8.ToArray(), default);

    /// <summary>Switches between editing and design mode.</summary>
    /// <param name="mode">The mode.</param>
    /// <returns>Completes when the setting applies.</returns>
    public Task SetModeAsync(EditorMode mode) => CallAsync(Method.SetMode, WireJson.ToUtf8(mode), default);

    /// <summary>Sets the author of later edits, revisions and comments.</summary>
    /// <param name="author">The author.</param>
    /// <returns>Completes when the setting applies.</returns>
    public Task SetAuthorAsync(Author author)
    {
        ArgumentNullException.ThrowIfNull(author);
        return CallAsync(Method.SetAuthor, WireJson.ToUtf8(author), default);
    }

    // Services

    /// <summary>The context menu model.</summary>
    public EditorMenus Menus { get; }

    /// <summary>Dialog models.</summary>
    public EditorDialogs Dialogs { get; }

    /// <summary>Find and replace.</summary>
    public EditorFind Find { get; }

    /// <summary>Spell checking.</summary>
    public EditorSpelling Spelling { get; }

    /// <summary>Headings, for a navigation pane.</summary>
    public EditorNavigation Navigation { get; }

    /// <summary>The document's styles.</summary>
    public EditorStyles Styles { get; }

    /// <summary>Revisions and comments.</summary>
    public EditorReview Review { get; }

    /// <summary>Merge-designer elements.</summary>
    public EditorElements Elements { get; }

    /// <summary>Custom XML data parts.</summary>
    public EditorDataParts DataParts { get; }

    /// <summary>The accessible structure.</summary>
    public EditorAccessibility Accessibility { get; }

    /// <summary>Application key shortcuts.</summary>
    public EditorShortcuts Shortcuts { get; }

    /// <summary>Pointer gesture bindings; they apply while attached.</summary>
    public EditorGestures Gestures { get; }

    /// <summary>The session's font library.</summary>
    public FontLibrary Fonts { get; }

    /// <summary>Document statistics, computed on request.</summary>
    /// <param name="ct">Cancels the request.</param>
    /// <returns>The counts.</returns>
    public Task<WordCount> GetWordCountAsync(CancellationToken ct = default) => CallAsync<WordCount>(Method.WordCount, [], ct);

    // Hosting

    /// <summary>Attaches a presentation. For control authors: see <see cref="IEditorHostAdapter"/>.</summary>
    /// <param name="adapter">The platform adapter.</param>
    /// <returns>The host the adapter forwards input and paints through. Dispose it to detach.</returns>
    /// <exception cref="EditorException">Another host is attached (<see cref="EditorErrorCode.Unavailable"/>).</exception>
    public EditorHost Attach(IEditorHostAdapter adapter)
    {
        ArgumentNullException.ThrowIfNull(adapter);
        ObjectDisposedException.ThrowIf(_disposed, this);
        var host = new EditorHost(this, adapter);
        if (Interlocked.CompareExchange(ref _host, host, null) is not null)
            throw new EditorException(EditorErrorCode.Unavailable, "another host is attached to this session");
        return host;
    }

    // Events

    /// <summary>The state changed. Coalesced to the latest snapshot.</summary>
    public event EventHandler<StateChangedEventArgs>? StateChanged;

    /// <summary>A transaction committed. Exactly one per commit, in order.</summary>
    public event EventHandler<DocumentChangedEventArgs>? DocumentChanged;

    /// <summary>The selection changed.</summary>
    public event EventHandler<SelectionChangedEventArgs>? SelectionChanged;

    /// <summary>The user activated a hyperlink.</summary>
    public event EventHandler<LinkActivatedEventArgs>? LinkActivated;

    /// <summary>The user clicked an unfilled picture placeholder.</summary>
    public event EventHandler<PlaceholderActivatedEventArgs>? PlaceholderActivated;

    /// <summary>The user asked for a context menu.</summary>
    public event EventHandler<ContextMenuRequestedEventArgs>? ContextMenuRequested;

    /// <summary>The user asked for a dialog.</summary>
    public event EventHandler<DialogRequestedEventArgs>? DialogRequested;

    /// <summary>The user pressed a key the application owns.</summary>
    public event EventHandler<HostCommandEventArgs>? HostCommandRequested;

    /// <summary>The user pressed an application shortcut.</summary>
    public event EventHandler<ShortcutEventArgs>? ShortcutPressed;

    /// <summary>A gesture bound to <see cref="GestureAction.Emit"/> happened. Attached only.</summary>
    public event EventHandler<GestureEventArgs>? GestureRequested;

    /// <summary>The user added a word to the dictionary.</summary>
    public event EventHandler<WordAddedEventArgs>? WordAdded;

    /// <summary>Fonts arrived or were found missing.</summary>
    public event EventHandler<FontsChangedEventArgs>? FontsChanged;

    /// <summary>In design mode, the user activated an element.</summary>
    public event EventHandler<ElementActivatedEventArgs>? ElementActivated;

    /// <summary>Elements were added, removed or changed.</summary>
    public event EventHandler<ElementsChangedEventArgs>? ElementsChanged;

    /// <summary>Fields need results the application computes; answer with <see cref="EditorCommands.SetFieldResultsAsync"/>.</summary>
    public event EventHandler<FieldsUpdateRequestedEventArgs>? FieldsUpdateRequested;

    /// <summary>Revisions were added, removed or changed.</summary>
    public event EventHandler? RevisionsChanged;

    /// <summary>Comments were added, removed or changed.</summary>
    public event EventHandler? CommentsChanged;

    /// <summary>The visible comment anchors moved. Attached only.</summary>
    public event EventHandler<CommentAnchorsChangedEventArgs>? CommentAnchorsChanged;

    /// <summary>The ruler view changed. Attached only.</summary>
    public event EventHandler<RulerChangedEventArgs>? RulerChanged;

    /// <summary>A tooltip should show or hide. Attached only.</summary>
    public event EventHandler<TooltipEventArgs>? TooltipChanged;

    /// <summary>The editor said something to screen readers.</summary>
    public event EventHandler<AnnouncementEventArgs>? AccessibilityAnnouncement;

    /// <summary>An import, export or font approximation.</summary>
    public event EventHandler<WarningEventArgs>? Warning;

    /// <summary>A fatal fault, or an exception thrown by an event handler (<see cref="FaultedEventArgs.IsFatal"/> false).</summary>
    public event EventHandler<FaultedEventArgs>? Faulted;

    // Lifetime

    /// <summary>Releases the session: callbacks stop at once, queued work fails with <see cref="EditorErrorCode.Destroyed"/>.</summary>
    public void Dispose() => _ = Close();

    /// <summary>Releases the session and waits for the native shutdown.</summary>
    /// <returns>Completes when the engine has released the session.</returns>
    public ValueTask DisposeAsync() => new(Close());

    private Task Close()
    {
        lock (_executor)
        {
            if (_closed is not null) return _closed;
            _disposed = true;
            _host?.Dispose();
            _closed = EngineThread.Instance.Post(() =>
            {
                if (_id != 0)
                {
                    Fonts.Native.Unregister(_id);
                    TwipNative.SessionFree(Engine, _id);
                    _id = 0;
                }
                return true;
            });
            return _closed;
        }
    }

    // Internals: the pipeline

    internal bool IsDisposed => _disposed;

    internal EditorHost? Host => Volatile.Read(ref _host);

    /// <summary>The native engine this session is in. Engine thread only.</summary>
    internal nint Engine => Fonts.Native.Handle;

    internal void Detached(EditorHost host) => Interlocked.CompareExchange(ref _host, null, host);

    /// <summary>Runs <paramref name="work"/> on the engine thread with this session's id, in queue order.</summary>
    internal Task<T> Enqueue<T>(Func<uint, T> work, CancellationToken ct)
    {
        if (_disposed) return Task.FromException<T>(Destroyed());
        return EngineThread.Instance.Post(() =>
        {
            if (_disposed) throw Destroyed();
            return work(EnsureSession());
        }, ct);
    }

    internal Task<JsonElement?> CallAsync(Method method, byte[] request, CancellationToken ct) =>
        Enqueue(id => Fonts.Native.Call(id, method, request), ct);

    /// <summary>Calls a session method and reads its result as <typeparamref name="T"/>.</summary>
    internal async Task<T> CallAsync<T>(Method method, byte[] request, CancellationToken ct)
    {
        var result = await CallAsync(method, request, ct).ConfigureAwait(false);
        return result is { } json ? json.Deserialize(WireJson.Info<T>())! : default!;
    }

    /// <summary>Calls a session method and waits for it, for the members that answer at once. Not on the engine thread.</summary>
    internal T Call<T>(Method method, byte[] request) => CallAsync<T>(method, request, default).GetAwaiter().GetResult();

    /// <inheritdoc cref="Call{T}"/>
    internal void Call(Method method, byte[] request) => CallAsync(method, request, default).GetAwaiter().GetResult();

    /// <summary>The visible comments' anchors, from the latest <c>commentAnchors</c> event; empty while unattached.</summary>
    internal IReadOnlyList<CommentAnchor> CommentAnchors => Volatile.Read(ref _anchors);

    internal Task RunAsync(EditorOperation operation, CancellationToken ct) =>
        CallAsync(Method.Run, WireJson.ToUtf8(operation), ct);

    internal async Task<T> RunAsync<T>(EditorOperation operation, Func<JsonElement?, T> result, CancellationToken ct) =>
        result(await CallAsync(Method.Run, WireJson.ToUtf8(operation), ct).ConfigureAwait(false));

    /// <summary>Completes after every operation queued so far has run and its notifications were delivered.</summary>
    internal Task WhenIdleAsync()
    {
        var done = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        // Not through Notify, which drops notifications once disposed: a disposed session is idle too.
        EngineThread.Instance.Post(() => Deliver(() => done.TrySetResult()));
        return done.Task;
    }

    private static EditorException Destroyed() => new(EditorErrorCode.Destroyed, "the session was disposed");

    private uint EnsureSession()
    {
        if (_id != 0) return _id;
        var engine = Engine;
        var options = WireJson.ToUtf8(new SessionOptionsWire(
            _options.Author ?? new Author(Environment.UserName), _options.ReadOnly, _options.Mode, _options.Language, _options.TrackChanges, _options.Dictionaries));
        uint id;
        unsafe
        {
            fixed (byte* p = options) id = TwipNative.SessionNew(engine, p, (nuint)options.Length);
        }
        if (id == 0) throw new EditorException(EditorErrorCode.Internal, "the engine could not create a session");
        _id = id;
        Fonts.Native.Register(id, this);
        if (Fonts.Native.Call(id, Method.State, []) is { } state) _state = state.Deserialize(TwipJsonContext.Default.EditorState)!;
        return id;
    }

    // The engine reads into a held replacement, then swaps it in as a second call: a cancel during the read drops
    // the replacement, so the open document is never partly replaced.
    private Task<ImportResult> OpenBytesAsync(byte[] bytes, CancellationToken ct) => Enqueue(id =>
    {
        TwipBuf output = default;
        int status;
        unsafe
        {
            fixed (byte* p = bytes)
            fixed (byte* o = "{}"u8)
            {
                status = TwipNative.OpenPrepare(Engine, id, p, (nuint)bytes.Length, o, 2, &output);
            }
        }
        uint token;
        try { token = WireJson.Unwrap(output.Span, status)!.Value.GetUInt32(); }
        finally { EngineThread.Free(ref output); }
        if (ct.IsCancellationRequested)
        {
            Fonts.Native.Call(id, Method.OpenDiscard, WireJson.ToUtf8(token));
            ct.ThrowIfCancellationRequested();
        }
        var warnings = Fonts.Native.Call(id, Method.OpenCommit, WireJson.ToUtf8(token)) is { ValueKind: JsonValueKind.Array } a
            ? a.Deserialize(TwipJsonContext.Default.IReadOnlyListWarning)!
            : [];
        return new ImportResult(warnings);
    }, ct);

    private Task<(byte[] Bytes, SaveResult Result)> ExportAsync(string format, CancellationToken ct) => Enqueue(id =>
    {
        var options = WireJson.ToUtf8(new ExportOptionsWire(format));
        TwipBuf bytes = default, meta = default;
        int status;
        unsafe
        {
            fixed (byte* o = options) status = TwipNative.Export(Engine, id, o, (nuint)options.Length, &bytes, &meta);
        }
        try
        {
            var info = WireJson.Unwrap(meta.Span, status)?.Deserialize(TwipJsonContext.Default.ExportMeta)
                ?? throw new EditorException(EditorErrorCode.Internal, "the engine's export has no revision");
            return (bytes.Span.ToArray(), new SaveResult(info.Revision, info.Warnings));
        }
        finally
        {
            EngineThread.Free(ref bytes);
            EngineThread.Free(ref meta);
        }
    }, ct);

    internal async Task<SaveResult> ExportToStreamAsync(string format, Stream stream, CancellationToken ct)
    {
        ArgumentNullException.ThrowIfNull(stream);
        var (bytes, result) = await ExportAsync(format, ct).ConfigureAwait(false);
        await stream.WriteAsync(bytes, ct).ConfigureAwait(false);
        return result;
    }

    private async Task<SaveResult> ExportToFileAsync(string format, string path, CancellationToken ct)
    {
        var full = Path.GetFullPath(path);
        var (bytes, result) = await ExportAsync(format, ct).ConfigureAwait(false);
        var temp = Path.Combine(Path.GetDirectoryName(full)!, $".{Path.GetFileName(full)}.{Guid.NewGuid():N}.tmp");
        try
        {
            await File.WriteAllBytesAsync(temp, bytes, ct).ConfigureAwait(false);
            File.Move(temp, full, overwrite: true);
        }
        finally
        {
            if (File.Exists(temp)) File.Delete(temp);
        }
        return result;
    }

    // Internals: callbacks and notifications

    void ICallbackSink.OnCallback(CallbackKind kind, byte[] payload)
    {
        if (_disposed) return;
        switch (kind)
        {
            case CallbackKind.State:
                var state = JsonSerializer.Deserialize(payload, TwipJsonContext.Default.EditorState)!;
                _state = state;
                Volatile.Write(ref _latestState, state.StateSequence);
                Notify(() =>
                {
                    // Coalesce: a newer snapshot is already queued behind this one.
                    if (state.StateSequence < Volatile.Read(ref _latestState)) return;
                    Raise(StateChanged, new StateChangedEventArgs(state));
                });
                break;
            case CallbackKind.Event:
                OnEvent(payload);
                break;
            case CallbackKind.RequestFrame:
                _host?.OnFrameRequested();
                break;
            case CallbackKind.SetTimer:
                var timer = JsonSerializer.Deserialize(payload, TwipJsonContext.Default.TimerRequest)!;
                _ = Task.Delay(TimeSpan.FromMilliseconds(timer.Ms)).ContinueWith(_ => EngineThread.Instance.Post(() =>
                {
                    if (!_disposed && _id != 0) TwipNative.TimerFired(Engine, _id, timer.Timer);
                }), TaskScheduler.Default);
                break;
            case CallbackKind.HostRequest:
                _host?.OnHostRequest(payload);
                break;
            // Font requests go to the font library (NativeEngine.Deliver), not to a session.
        }
    }

    void ICallbackSink.OnCallbackFailed(Exception exception) => Notify(() => RaiseFault(exception, isFatal: false));

    private void OnEvent(byte[] payload)
    {
        var root = JsonDocument.Parse(payload).RootElement;
        var snapshot = _state;
        var ctx = TwipJsonContext.Default;
        switch (root.GetProperty("type").GetString())
        {
            case "documentChanged":
                var record = root.Deserialize(ctx.TransactionRecord)!;
                Notify(() => Raise(DocumentChanged, new DocumentChangedEventArgs(record, snapshot)));
                break;
            case "selectionChanged":
                var selection = root.Deserialize(ctx.SelectionSummary)!;
                Notify(() => Raise(SelectionChanged, new SelectionChangedEventArgs(selection, snapshot)));
                break;
            case "linkActivated":
                var link = root.Deserialize(ctx.LinkActivatedPayload)!;
                Notify(() => Raise(LinkActivated, new LinkActivatedEventArgs(link.Address, link.Via, snapshot)));
                break;
            case "placeholderActivated":
                var placeholder = root.Deserialize(ctx.PlaceholderInfo)!;
                Notify(() => Raise(PlaceholderActivated, new PlaceholderActivatedEventArgs(placeholder.Key, placeholder.Element, snapshot)));
                break;
            case "contextMenu":
                var menu = root.Deserialize(ctx.ContextMenuPayload)!;
                var at = _host?.ToClient(menu.At) ?? menu.At;
                Notify(() => Raise(ContextMenuRequested, new ContextMenuRequestedEventArgs(menu.Menu, at, menu.Via, snapshot)));
                break;
            case "dialogRequested":
                var dialog = root.Deserialize(ctx.DialogRequestedPayload)!;
                Notify(() => Raise(DialogRequested, new DialogRequestedEventArgs(dialog.Kind, snapshot)));
                break;
            case "hostCommand":
                var command = root.Deserialize(ctx.HostCommandPayload)!;
                Notify(() => Raise(HostCommandRequested, new HostCommandEventArgs(command.Command, snapshot)));
                break;
            case "shortcut":
                var shortcut = root.Deserialize(ctx.ShortcutPayload)!;
                Notify(() => Raise(ShortcutPressed, new ShortcutEventArgs(shortcut.Id, snapshot)));
                break;
            case "wordAdded":
                var word = root.Deserialize(ctx.WordAddedPayload)!;
                Notify(() => Raise(WordAdded, new WordAddedEventArgs(word.Word, word.Language, snapshot)));
                break;
            case "fontsChanged":
                var fonts = root.Deserialize(ctx.FontsChangedPayload)!;
                Notify(() => Raise(FontsChanged, new FontsChangedEventArgs(fonts.Pending, fonts.Unresolved, snapshot)));
                break;
            case "elementActivated":
                var element = root.Deserialize(ctx.ElementActivatedPayload)!;
                Notify(() => Raise(ElementActivated, new ElementActivatedEventArgs(element.Id, snapshot)));
                break;
            case "elementsChanged":
                var elements = root.Deserialize(ctx.ElementsChangedPayload)!;
                Notify(() => Raise(ElementsChanged, new ElementsChangedEventArgs(elements.Added, elements.Removed, elements.Changed, snapshot)));
                break;
            case "fieldsUpdateRequested":
                var fields = root.Deserialize(ctx.FieldsUpdateRequestedPayload)!;
                Notify(() => Raise(FieldsUpdateRequested, new FieldsUpdateRequestedEventArgs(fields.Token, fields.Fields, snapshot)));
                break;
            case "revisionsChanged":
                Notify(() => Raise(RevisionsChanged, EventArgs.Empty));
                break;
            case "commentsChanged":
                Notify(() => Raise(CommentsChanged, EventArgs.Empty));
                break;
            case "commentAnchors":
                var anchors = root.Deserialize(ctx.CommentAnchorsPayload)!;
                if (_host is { } anchorHost) anchors = anchors with { Anchors = [.. anchors.Anchors.Select(anchorHost.ToClient)] };
                Volatile.Write(ref _anchors, anchors.Anchors);
                Notify(() => Raise(CommentAnchorsChanged, new CommentAnchorsChangedEventArgs(anchors.Anchors, snapshot)));
                break;
            case "rulerChanged":
                var ruler = root.Deserialize(ctx.RulerView)!;
                if (_host is { } rulerHost) ruler = rulerHost.ToClient(ruler);
                Notify(() => Raise(RulerChanged, new RulerChangedEventArgs(ruler, snapshot)));
                break;
            case "tooltip":
                var tip = root.Deserialize(ctx.TooltipPayload)!.Tooltip is { } t ? new TooltipInfo(t.Kind, t.Text, t.At, t.Rect) : null;
                if (tip is not null && _host is { } tipHost) tip = tipHost.ToClient(tip);
                _host?.OnTooltip(tip);
                Notify(() => Raise(TooltipChanged, new TooltipEventArgs(tip, snapshot)));
                break;
            case "announcement":
                var announcement = root.Deserialize(ctx.AnnouncementPayload)!;
                _host?.OnAnnouncement(announcement.Text);
                Notify(() => Raise(AccessibilityAnnouncement, new AnnouncementEventArgs(announcement.Text, snapshot)));
                break;
            case "warning":
                var warning = root.Deserialize(ctx.Warning)!;
                Notify(() => Raise(Warning, new WarningEventArgs(warning, snapshot)));
                break;
            case "error" or "fault":
                var error = root.Deserialize(ctx.ErrorPayload)!;
                var exception = new EditorException(error.Code, error.Message, error.Details);
                Notify(() => RaiseFault(exception, isFatal: true));
                break;
            case "gesture":
                var gesture = root.Deserialize(ctx.GestureEvent)!;
                Notify(() => Raise(GestureRequested, new GestureEventArgs(gesture.Gesture, gesture.At, gesture.Page, gesture.Target, snapshot)));
                break;
        }
    }

    /// <summary>Delivers a notification on the attached host's UI thread, or the session's own executor.</summary>
    private void Notify(Action action) => Deliver(() =>
    {
        if (!_disposed) action();
    });

    /// <summary>Runs <paramref name="action"/> after the notifications queued so far, where they are delivered.</summary>
    private void Deliver(Action action)
    {
        if (Volatile.Read(ref _host) is not { } host) _executor.Post(action);
        else
        {
            try { host.Adapter.Dispatcher.Post(action); }
            catch (Exception e) when (e is InvalidOperationException or ObjectDisposedException)
            {
                // The host's UI thread is gone (its window closed before it detached): deliver unattached.
                _executor.Post(action);
            }
        }
    }

    private void Raise<T>(EventHandler<T>? handler, T args)
    {
        if (handler is null) return;
        foreach (var each in handler.GetInvocationList())
        {
            try { ((EventHandler<T>)each)(this, args); }
            catch (Exception e) { RaiseFault(e, isFatal: false); }
        }
    }

    private void Raise(EventHandler? handler, EventArgs args)
    {
        if (handler is null) return;
        foreach (var each in handler.GetInvocationList())
        {
            try { ((EventHandler)each)(this, args); }
            catch (Exception e) { RaiseFault(e, isFatal: false); }
        }
    }

    private void RaiseFault(Exception exception, bool isFatal)
    {
        var handler = Faulted;
        if (handler is null) return;
        var args = new FaultedEventArgs(exception, isFatal, _state);
        foreach (var each in handler.GetInvocationList())
        {
            try { ((EventHandler<FaultedEventArgs>)each)(this, args); }
            catch
            {
                // A throwing fault handler has nowhere left to report to.
            }
        }
    }
}
