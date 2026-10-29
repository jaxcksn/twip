using System.ComponentModel;
using System.ComponentModel.Design;
using System.Runtime.InteropServices;
using System.Windows.Forms;
using Twip.Native;
using Twip.WinForms;
using Xunit;

namespace Twip.Tests;

/// <summary>The WinForms tests share the designer's license context and the clipboard: they run one at a time.</summary>
[CollectionDefinition("WinForms", DisableParallelization = true)]
public sealed class WinFormsCollection;

/// <summary>Runs a test on an STA thread with a message loop, as a WinForms application would.</summary>
internal static partial class Ui
{
    public const int WmChar = 0x0102;
    public const int WmRButtonDown = 0x0204;
    public const int WmRButtonUp = 0x0205;
    public const int WmLButtonDown = 0x0201;
    public const int WmLButtonUp = 0x0202;
    public const int WmMouseMove = 0x0200;

    [DllImport("user32.dll", EntryPoint = "SendMessageW")]
    public static extern nint SendMessage(nint hwnd, int msg, nint wParam, nint lParam);

    public static nint LParam(int x, int y) => (y << 16) | (x & 0xFFFF);

    public static void Run(Func<Task> body)
    {
        if (!TwipNative.IsAvailable()) Assert.Skip($"the native engine ({TwipNative.Library}) with the C ABI isn't built: run `moon run twip-capi:build`");
        Exception? failure = null;
        var ui = new Thread(() =>
        {
            try
            {
                SynchronizationContext.SetSynchronizationContext(new WindowsFormsSynchronizationContext());
                var task = body();
                while (!task.IsCompleted)
                {
                    Application.DoEvents();
                    Thread.Sleep(5);
                }
                task.GetAwaiter().GetResult();
            }
            catch (Exception e) { failure = e; }
        });
        ui.SetApartmentState(ApartmentState.STA);
        ui.Start();
        Assert.True(ui.Join(TimeSpan.FromSeconds(90)), "the UI thread didn't finish");
        if (failure is not null) throw failure;
    }

    /// <summary>Waits, with the message loop running, until <paramref name="condition"/> holds.</summary>
    public static async Task Until(Func<bool> condition, string what, int seconds = 30)
    {
        var deadline = DateTime.UtcNow.AddSeconds(seconds);
        while (!condition())
        {
            if (DateTime.UtcNow > deadline) Assert.Fail($"timed out waiting for {what}");
            await Task.Delay(10);
        }
    }

    /// <summary>The control as it paints now.</summary>
    public static Bitmap Snapshot(Control control)
    {
        var bitmap = new Bitmap(control.Width, control.Height);
        control.DrawToBitmap(bitmap, new Rectangle(Point.Empty, control.Size));
        return bitmap;
    }

    /// <summary>Waits for the first frame: the paper at the middle of the control is white.</summary>
    public static Task FirstFrame(Control control) => Until(() =>
    {
        using var shot = Snapshot(control);
        return shot.GetPixel(shot.Width / 2, shot.Height / 2).ToArgb() == Color.White.ToArgb();
    }, "the first frame");

    public static void Type(Control control, string text)
    {
        foreach (var c in text) SendMessage(control.Handle, WmChar, c, 0);
    }
}

/// <summary>The WinForms control on a real window: input, handle recreation, external sessions and state.</summary>
[Collection("WinForms")]
public sealed class WinFormsTests
{
    private sealed class Recreatable : DocumentEditor
    {
        public Recreatable(EditorSession session) : base(session) { }

        public void Recreate() => RecreateHandle();
    }

    [Fact]
    public void InTheDesignerNoEngineLoads() => Ui.Run(() =>
    {
        var context = LicenseManager.CurrentContext;
        LicenseManager.CurrentContext = new DesigntimeLicenseContext();
        try
        {
            using var editor = new DocumentEditor { Width = 400, Height = 300 };
            _ = editor.Handle;
            Assert.Null(editor.Session.Host);
            // The placeholder paints without the engine.
            using var shot = Ui.Snapshot(editor);
            Assert.Equal(editor.BackColor.ToArgb(), shot.GetPixel(2, 2).ToArgb());
        }
        finally
        {
            LicenseManager.CurrentContext = context;
        }
        return Task.CompletedTask;
    });

    [Fact]
    public void TurningTheGpuOffAndOnKeepsTheFrame() => Ui.Run(async () =>
    {
        using var editor = new DocumentEditor { Width = 800, Height = 600 };
        _ = editor.Handle;
        // A copy of the control (DrawToBitmap) shows the frame whether the GPU draws it or the CPU does.
        await Ui.FirstFrame(editor);
        var backend = editor.GpuBackend; // null with no usable GPU, as on CI
        editor.UseGpu = false;
        Assert.Null(editor.GpuBackend);
        await Ui.FirstFrame(editor);
        editor.UseGpu = true;
        Assert.Equal(backend, editor.GpuBackend);
        await Ui.FirstFrame(editor);
    });

    [Fact]
    public void TypingChangesTheDocument() => Ui.Run(async () =>
    {
        using var editor = new DocumentEditor { Width = 800, Height = 600 };
        // Creating the window attaches the control to its session.
        _ = editor.Handle;
        var before = editor.Session.State.DocumentRevision;
        Ui.Type(editor, "Hi");
        // The first state can arrive after `before` was read: wait for the typing itself.
        await Ui.Until(() => editor.Session.State.Modified, "the typing");
        var state = editor.Session.State;
        Assert.True(state.DocumentRevision > before, $"the revision stayed {before} after typing");
        Assert.True(state.History.CanUndo, "typing left nothing to undo");
    });

    [Fact]
    public void RecreatingTheHandleKeepsTheDocumentAndSelection() => Ui.Run(async () =>
    {
        await using var session = new EditorSession();
        using var editor = new Recreatable(session) { Width = 800, Height = 600 };
        _ = editor.Handle;
        Ui.Type(editor, "Hello");
        await Ui.Until(() => editor.Session.State.Modified, "the typing");
        await editor.Commands.SelectAllAsync();
        var (revision, host) = (session.State.DocumentRevision, session.Host);

        editor.Recreate();
        Assert.NotNull(session.Host);
        Assert.NotSame(host, session.Host);
        Assert.Equal(revision, session.State.DocumentRevision);
        Assert.False(session.State.Selection.Empty);
        Assert.True(session.State.History.CanUndo);
        // The new window still edits.
        Ui.Type(editor, "!");
        await Ui.Until(() => session.State.DocumentRevision > revision, "typing after the recreation");
    });

    [Fact]
    public void DisposingAControlLeavesItsExternalSessionUsable() => Ui.Run(async () =>
    {
        await using var session = new EditorSession();
        using (var first = new DocumentEditor(session) { Width = 800, Height = 600 })
        {
            _ = first.Handle;
            Ui.Type(first, "Kept");
            await Ui.Until(() => session.State.Modified, "the typing");
        }
        Assert.Null(session.Host);
        var revision = session.State.DocumentRevision;
        await session.Commands.InsertTextAsync(" still");
        Assert.True(session.State.DocumentRevision > revision);

        using var second = new DocumentEditor(session) { Width = 800, Height = 600 };
        _ = second.Handle;
        Assert.NotNull(session.Host);
        Ui.Type(second, "!");
        await Ui.Until(() => session.State.DocumentRevision > revision + 1, "typing in the second control");
    });

    [Fact]
    public void StateRaisesPropertyChangedOnTheUiThread() => Ui.Run(async () =>
    {
        using var editor = new DocumentEditor { Width = 800, Height = 600 };
        _ = editor.Handle;
        var ui = Environment.CurrentManagedThreadId;
        var changed = new List<(string? Name, int Thread)>();
        editor.State.PropertyChanged += (_, e) => changed.Add((e.PropertyName, Environment.CurrentManagedThreadId));
        Ui.Type(editor, "x");
        await Ui.Until(() => changed.Exists(c => c.Name == nameof(DocumentEditorState.CanUndo)), "CanUndo to change");
        Assert.All(changed, c => Assert.Equal(ui, c.Thread));
        Assert.True(editor.State.CanUndo);
    });

    [Fact]
    public void ToolbarButtonsBindToTheStateWithoutConverters() => Ui.Run(async () =>
    {
        using var form = new Form { Width = 800, Height = 600 };
        var editor = new DocumentEditor { Dock = DockStyle.Fill };
        var bold = new ToolStripButton("Bold");
        var toolbar = new ToolStrip();
        toolbar.Items.Add(bold);
        form.Controls.Add(editor);
        form.Controls.Add(toolbar);
        _ = editor.Handle;
#if NETFRAMEWORK
        // .NET Framework's ToolStripItem has no DataBindings: follow the state's PropertyChanged instead.
        editor.State.PropertyChanged += (_, _) => (bold.CheckState, bold.Enabled) = (editor.State.BoldCheckState, editor.State.CanSetBold);
#else
        bold.DataBindings.Add(nameof(bold.CheckState), editor.State, nameof(DocumentEditorState.BoldCheckState));
        bold.DataBindings.Add(nameof(bold.Enabled), editor.State, nameof(DocumentEditorState.CanSetBold));
#endif

        await editor.Commands.InsertTextAsync("a");
        await editor.Commands.SelectAllAsync();
        await editor.Commands.SetBoldAsync(true);
        await Ui.Until(() => bold.CheckState == CheckState.Checked, "the button to show bold");
        await editor.Commands.MoveSelectionAsync(MoveUnit.Character, Direction.Forward);
        await editor.Commands.SetBoldAsync(false);
        await editor.Commands.InsertTextAsync("b");
        await Ui.Until(() => bold.CheckState == CheckState.Unchecked, "the button to show not bold");
        await editor.Commands.SelectAllAsync();
        await Ui.Until(() => bold.CheckState == CheckState.Indeterminate, "the button to show mixed");
        Assert.True(bold.Enabled);
        editor.ReadOnly = true;
        await Ui.Until(() => !bold.Enabled, "the button to disable in a read-only editor");
    });

    [Fact]
    public void CopyNeedsASelectionAndPasteAnEditableEditor() => Ui.Run(async () =>
    {
        using var editor = new DocumentEditor { Width = 800, Height = 600 };
        _ = editor.Handle;
        await editor.Commands.InsertTextAsync("a");
        await Ui.Until(() => !editor.State.CanCopy && editor.State.CanPaste, "a caret: no copy, paste");
        await editor.Commands.SelectAllAsync();
        await Ui.Until(() => editor.State.CanCopy, "copy with a selection");
        editor.ReadOnly = true;
        await Ui.Until(() => editor.State.CanCopy && !editor.State.CanPaste, "read only: copy, no paste");
    });
}

/// <summary>The clipboard's HTML format, whose header counts UTF-8 bytes.</summary>
public sealed class ClipboardFormatTests
{
    [Fact]
    public void CfHtmlRoundTripsAFragment()
    {
        const string fragment = "<p>Café — <b>naïve</b> \U0001F600</p>";
        var cf = ClipboardFormats.ToCfHtml(fragment);
        Assert.StartsWith("Version:0.9\r\n", cf, StringComparison.Ordinal);
        Assert.Equal(fragment, ClipboardFormats.FromCfHtml(cf));
        // HTML from elsewhere with no header is taken whole.
        Assert.Equal("<i>x</i>", ClipboardFormats.FromCfHtml("<i>x</i>"));
    }
}
