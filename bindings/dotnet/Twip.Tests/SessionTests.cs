using Xunit;

namespace Twip.Tests;

/// <summary>A session's documents and lifetime, with no host attached.</summary>
public sealed class SessionTests : IDisposable
{
    private readonly string _dir = Directory.CreateTempSubdirectory("twip-session-").FullName;

    private static CancellationToken Ct => TestContext.Current.CancellationToken;

    public void Dispose()
    {
        foreach (var file in Directory.EnumerateFiles(_dir)) File.SetAttributes(file, FileAttributes.Normal);
        Directory.Delete(_dir, recursive: true);
    }

    private static bool IsZip(byte[] bytes) => bytes.Length > 4 && bytes[0] == 'P' && bytes[1] == 'K';

    [Fact]
    public async Task OpensEditsAndSavesBeforeAnyHostExists()
    {
        Native.Require();
        await using var session = new EditorSession();
        await session.OpenAsync(Native.Fixture("FX-TEXT.docx"), Ct);
        var opened = session.State.DocumentRevision;
        await session.Commands.InsertTextAsync("Hello", Ct);
        Assert.Equal(opened + 1, session.State.DocumentRevision);

        var path = Path.Combine(_dir, "out.docx");
        var saved = await session.SaveAsync(path, Ct);
        Assert.Equal(session.State.DocumentRevision, saved.Revision);
        Assert.True(IsZip(await File.ReadAllBytesAsync(path, Ct)));

        await using var again = new EditorSession();
        await again.OpenAsync(path, Ct);
        var pdf = Path.Combine(_dir, "out.pdf");
        await again.ExportPdfAsync(pdf, Ct);
        Assert.StartsWith("%PDF", await File.ReadAllTextAsync(pdf, Ct), StringComparison.Ordinal);
    }

    [Fact]
    public async Task CallerStreamsStayOpenAndPathsAreReleased()
    {
        Native.Require();
        await using var session = new EditorSession();
        using (var input = File.OpenRead(Native.Fixture("FX-TEXT.docx")))
        {
            await session.OpenAsync(input, Ct);
            Assert.True(input.CanRead);
        }
        var output = new MemoryStream();
        await session.SaveAsync(output, Ct);
        Assert.True(output.CanWrite);
        Assert.True(IsZip(output.ToArray()));

        // The path overloads close their files: each can be deleted at once.
        var copy = Path.Combine(_dir, "in.docx");
        File.Copy(Native.Fixture("FX-TEXT.docx"), copy);
        await session.OpenAsync(copy, Ct);
        File.Delete(copy);
        var path = Path.Combine(_dir, "out.docx");
        await session.SaveAsync(path, Ct);
        File.Delete(path);
    }

    [Fact]
    public async Task AFailedPathSaveLeavesTheOldFile()
    {
        Native.Require();
        await using var session = new EditorSession();
        await session.Commands.InsertTextAsync("new", Ct);
        var path = Path.Combine(_dir, "doc.docx");
        await File.WriteAllTextAsync(path, "old", Ct);
        // A read-only file refuses the replace on Windows; on Unix a rename replaces it anyway, so the folder refuses
        // the temp file there.
        Refuse(path, true);

        await Assert.ThrowsAnyAsync<Exception>(() => session.SaveAsync(path, Ct));
        Assert.Equal("old", await File.ReadAllTextAsync(path, Ct));
        Assert.Equal([path], Directory.GetFiles(_dir)); // no temp file left beside it

        Refuse(path, false);
        await session.SaveAsync(path, Ct);
        Assert.True(IsZip(await File.ReadAllBytesAsync(path, Ct)));
        Assert.Equal([path], Directory.GetFiles(_dir));
    }

    /// <summary>Makes a save to <paramref name="path"/> fail, or work again.</summary>
    static void Refuse(string path, bool refuse)
    {
#if NET
        if (!OperatingSystem.IsWindows())
        {
            var mode = UnixFileMode.UserRead | UnixFileMode.UserExecute | (refuse ? 0 : UnixFileMode.UserWrite);
            File.SetUnixFileMode(Path.GetDirectoryName(path)!, mode);
            return;
        }
#endif
        File.SetAttributes(path, refuse ? FileAttributes.ReadOnly : FileAttributes.Normal);
    }

    [Fact]
    public async Task CancellingAnOpenKeepsTheDocument()
    {
        Native.Require();
        await using var session = new EditorSession();
        await session.Commands.InsertTextAsync("keep", Ct);
        var before = session.State;

        await Assert.ThrowsAnyAsync<OperationCanceledException>(() => session.OpenAsync(Native.Fixture("FX-TEXT.docx"), new CancellationToken(canceled: true)));
        await Assert.ThrowsAnyAsync<OperationCanceledException>(() => session.OpenAsync(new MemoryStream(File.ReadAllBytes(Native.Fixture("FX-TEXT.docx"))), new CancellationToken(canceled: true)));
        await session.WhenIdleAsync();

        Assert.Equal(before.DocumentRevision, session.State.DocumentRevision);
        Assert.True(session.State.History.CanUndo);
    }

    [Fact]
    public async Task EditsDuringASaveStayModified()
    {
        Native.Require();
        await using var session = new EditorSession();
        await session.Commands.InsertTextAsync("a", Ct);
        // The save captures this revision; the edit queued behind it comes after.
        var save = session.SaveAsync(new MemoryStream(), Ct);
        var edit = session.Commands.InsertTextAsync("b", Ct);
        var saved = await save;
        await edit;
        Assert.True(saved.Revision < session.State.DocumentRevision);

        Assert.True(session.State.Modified, "the edit after the save isn't modified");
        await session.Commands.UndoAsync(Ct);
        Assert.False(session.State.Modified, "undoing to the saved revision leaves it modified");
    }

    [Fact]
    public async Task DisposeFailsQueuedWorkAndDisposeAsyncWaits()
    {
        Native.Require();
        var session = new EditorSession();
        await session.Commands.InsertTextAsync("a", Ct);
        session.Dispose();
        session.Dispose();
        var error = await Assert.ThrowsAsync<EditorException>(() => session.Commands.InsertTextAsync("b", Ct));
        Assert.Equal(EditorErrorCode.Destroyed, error.Code);
        await session.DisposeAsync().AsTask().WaitAsync(TimeSpan.FromSeconds(10), Ct);
    }

    [Fact]
    public async Task SettingsApplyWithoutAReopen()
    {
        Native.Require();
        await using var session = new EditorSession();
        await session.Commands.InsertTextAsync("a", Ct);
        var revision = session.State.DocumentRevision;

        await session.SetReadOnlyAsync(true);
        Assert.True(session.State.ReadOnly);
        var error = await Assert.ThrowsAsync<EditorException>(() => session.Commands.SetBoldAsync(true, Ct));
        Assert.Equal(EditorErrorCode.ReadOnly, error.Code);
        await session.SetReadOnlyAsync(false);
        Assert.False(session.State.ReadOnly);

        await session.SetAuthorAsync(new Author("Ada Lovelace", "AL"));
        DocumentChangedEventArgs? changed = null;
        session.DocumentChanged += (_, e) => changed = e;
        await session.Commands.InsertTextAsync("b", Ct);
        await session.WhenIdleAsync();
        Assert.Equal("Ada Lovelace", changed?.Record.Author.Name);
        Assert.Equal(revision + 1, changed?.Record.Revision);

        await session.SetModeAsync(EditorMode.Design);
        Assert.Equal(EditorMode.Design, session.State.Mode);
        Assert.Equal(revision + 1, session.State.DocumentRevision);
    }

    [Fact]
    public async Task BatchesCommitOnce()
    {
        Native.Require();
        await using var session = new EditorSession();
        var records = new List<TransactionRecord>();
        session.DocumentChanged += (_, e) => records.Add(e.Record);
        await session.BatchAsync([EditorOperation.InsertText("a"), EditorOperation.SelectAll(), EditorOperation.SetBold(true)], "Bold a", Ct);
        await session.WhenIdleAsync();
        var record = Assert.Single(records);
        Assert.Equal("Bold a", record.Label);
        await session.Commands.UndoAsync(Ct);
        Assert.False(session.State.History.CanUndo);
    }

    [Fact]
    public async Task TheCatalogListsEveryCommand()
    {
        Native.Require();
        await using var session = new EditorSession();
        Assert.Contains(session.Catalog, c => c.Id == CommandId.SetBold && c.Mutates);
        Assert.Equal(Enum.GetValues<CommandId>().Order(), session.Catalog.Select(c => c.Id).Order());
    }

    [Fact]
    public async Task TheShapeGalleryHasWordsGroups()
    {
        Native.Require();
        await using var session = new EditorSession();
        Assert.Equal(8, session.ShapeGallery.Count);
        Assert.Contains(session.ShapeGallery, g => g.Label == "Basic Shapes" && g.Presets.Contains(new ShapePreset("ellipse")));
    }

    [Fact]
    public async Task TheViewSettingsReachTheState()
    {
        Native.Require();
        await using var session = new EditorSession();
        await session.SetThemeAsync(ChromeTheme.Dark);
        await session.SetAppearanceAsync(DocumentAppearance.DarkWithLightPage);
        await session.SetDetailAsync(MeasureDetail.Quiet);
        await session.Commands.SetViewOptionsAsync(objectGrid: ObjectGrid.Always, ct: Ct);
        var view = session.State.View;
        Assert.Equal(ChromeThemeKind.Dark, view.Theme.Kind);
        Assert.Equal(DocumentAppearance.DarkWithLightPage, view.Appearance);
        Assert.Equal(MeasureDetail.Quiet, view.Detail);
        Assert.Equal(ObjectGrid.Always, view.ObjectGrid);

        var colors = new HighContrastColors(System.Drawing.Color.Black, System.Drawing.Color.White, ColorValue.FromRgb(0x1A, 0xEB, 0xFF), System.Drawing.Color.Black, ColorValue.FromRgb(0x3F, 0xF2, 0x3F));
        await session.SetThemeAsync(ChromeTheme.HighContrast(colors), ColorValue.FromRgb(0x7A, 0x3E, 0xC8));
        Assert.Equal(colors, session.State.View.Theme.Colors);
    }

    [Fact]
    public async Task TheTabTypePickerIsRequestedAnsweredAndClosed()
    {
        Native.Require();
        await using var session = new EditorSession();
        var requested = new TaskCompletionSource<Popover>(TaskCreationOptions.RunContinuationsAsynchronously);
        session.PopoverRequested += (_, e) => requested.TrySetResult(e.Popover);
        var closed = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        session.PopoverClosed += (_, _) => closed.TrySetResult();
        await session.Commands.ShowTabTypesAsync(Ct);
        var picker = Assert.IsType<Popover.TabType>(await requested.Task.WaitAsync(TimeSpan.FromSeconds(30), Ct));
        Assert.Equal(TabSelector.Left, picker.Current);
        Assert.Equal(7, picker.Choices.Count);
        Assert.Contains(new TabTypeChoice(TabSelector.Decimal, "Decimal", "Numbers line up on the decimal point"), picker.Choices);

        await session.Commands.SetRulerTabTypeAsync(TabSelector.Decimal, Ct);
        Assert.Equal(TabSelector.Decimal, session.State.View.TabSelector);
        Assert.Equal("decimal", session.State.Commands[CommandId.SetRulerTabType].Value);
        await closed.Task.WaitAsync(TimeSpan.FromSeconds(30), Ct);

        // The application closes its own: the engine's closes too, and with none open nothing happens.
        closed = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        await session.Commands.ShowTabTypesAsync(Ct);
        await session.Commands.ClosePopoverAsync(Ct);
        await closed.Task.WaitAsync(TimeSpan.FromSeconds(30), Ct);
        await session.Commands.ClosePopoverAsync(Ct);
    }

    [Fact]
    public async Task AutoCorrectSettingsReachTheState()
    {
        Native.Require();
        await using var session = new EditorSession();
        Assert.True(session.State.AutoCorrect.SmartQuotes);
        await session.Commands.SetAutoCorrectAsync(new AutoCorrectChange(SmartQuotes: false, Never: ["teh"]), Ct);
        Assert.False(session.State.AutoCorrect.SmartQuotes);
        Assert.Equal(["teh"], session.State.AutoCorrect.Never);
    }
}
