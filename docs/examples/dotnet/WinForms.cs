using System.Windows.Forms;
using Twip;
using Twip.WinForms;

namespace Examples;

#region form
sealed class MainForm : Form
{
    readonly DocumentEditor editor = new() { Dock = DockStyle.Fill, ShowHorizontalRuler = true };
    readonly ToolStripButton bold = new("Bold");
    readonly ToolStripStatusLabel page = new();

    public MainForm()
    {
        var toolbar = new ToolStrip();
        toolbar.Items.Add(bold);
        var status = new StatusStrip();
        status.Items.Add(page);
        Controls.Add(editor);
        Controls.Add(toolbar);
        Controls.Add(status);

        // DocumentEditorState raises PropertyChanged on the UI thread: bind to it, or read it there.
        bold.DataBindings.Add(nameof(bold.CheckState), editor.State, nameof(editor.State.BoldCheckState));
        bold.DataBindings.Add(nameof(bold.Enabled), editor.State, nameof(editor.State.CanSetBold));
        editor.State.PropertyChanged += (_, _) => page.Text = $"Page {editor.State.CurrentPage} of {editor.State.PageCount}";
        // Setters take a value: pass the opposite of what the state shows.
        bold.Click += async (_, _) => await editor.Commands.SetBoldAsync(!editor.State.Bold.IsOn);

        editor.Session.HostCommandRequested += async (_, e) =>
        {
            if (e.Command == HostCommand.Save) await editor.SaveAsync("report.docx");
        };
        editor.Session.ContextMenuRequested += (_, e) => ShowMenu(e.Menu, Point.Round(e.Location));
        Load += async (_, _) => await editor.OpenAsync("report.docx");
    }

    void ShowMenu(Twip.ContextMenu menu, Point at)
    {
        var strip = new ContextMenuStrip();
        foreach (var item in menu.Items.OfType<CommandItem>())
        {
            strip.Items.Add(item.Label, null, async (_, _) => await editor.Session.Menus.InvokeAsync(item.Action)).Enabled = item.Enabled;
        }
        strip.Show(editor, at);
    }
}
#endregion
