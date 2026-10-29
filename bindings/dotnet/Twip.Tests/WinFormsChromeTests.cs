using System.Windows.Forms;
using Twip.WinForms;
using Xunit;

namespace Twip.Tests;

/// <summary>The control's chrome: background, markup pane, balloons, the context menu, hit testing and drops.</summary>
[Collection("WinForms")]
public sealed class WinFormsChromeTests
{
    private static DocumentEditor Editor() => new() { Width = 1400, Height = 700 };

    /// <summary>The page's left edge on a row: the first white pixel.</summary>
    private static int PageLeft(Control control, int y)
    {
        using var shot = Ui.Snapshot(control);
        for (var x = 0; x < shot.Width; x++)
        {
            if (shot.GetPixel(x, y).ToArgb() == Color.White.ToArgb()) return x;
        }
        return -1;
    }

    [Fact]
    public void BackColorChangesTheAreaAroundThePagesAndNotThePaper() => Ui.Run(async () =>
    {
        using var editor = Editor();
        _ = editor.Handle;
        await Ui.FirstFrame(editor);
        var dark = Color.FromArgb(0x20, 0x20, 0x20);
        editor.BackColor = dark;
        await Ui.Until(() =>
        {
            using var shot = Ui.Snapshot(editor);
            return shot.GetPixel(3, 300).ToArgb() == dark.ToArgb();
        }, "the dark background");
        using var shot = Ui.Snapshot(editor);
        Assert.Equal(Color.White.ToArgb(), shot.GetPixel(shot.Width / 2, 300).ToArgb());
    });

    [Fact]
    public void MarkupPaneWidthMovesThePages() => Ui.Run(async () =>
    {
        using var editor = Editor();
        _ = editor.Handle;
        await Ui.FirstFrame(editor);
        var before = PageLeft(editor, 300);
        Assert.True(before > 0, "no page edge");
        editor.MarkupPaneWidth = 300;
        await Ui.Until(() => PageLeft(editor, 300) < before, "the pages to move aside");
    });

    [Fact]
    public void AttachedBalloonsFollowScrolling() => Ui.Run(async () =>
    {
        using var editor = Editor();
        _ = editor.Handle;
        editor.MarkupPaneWidth = 300;
        await editor.Commands.InsertTextAsync("Commented");
        for (var i = 0; i < 60; i++) await editor.Commands.InsertParagraphAsync();
        await editor.Commands.SelectAllAsync();
        var thread = await editor.Commands.AddCommentAsync("A note");

        using var card = new Panel { Width = 200, Height = 60 };
        var laidOut = 0;
        editor.BalloonsLaidOut += (_, e) => laidOut += e.Placements.Count(p => p.Thread == thread);
        using var placement = editor.AttachBalloon(thread, card);
        await Ui.Until(() => card.Visible && laidOut > 0, "the card to be placed");
        var top = card.Top;

        var bar = editor.Controls.OfType<VScrollBar>().Single();
        await Ui.Until(() => bar.Enabled, "the document to scroll");
        bar.Value = 40;
        await Ui.Until(() => card.Top <= top - 30, $"the card to scroll up from {top} (at {card.Top})");

        placement.Dispose();
        Assert.DoesNotContain(card, editor.Controls.Cast<Control>());
    });

    [Fact]
    public void ContextMenuRequestedCarriesTheModelAndTheClientLocation() => Ui.Run(async () =>
    {
        using var editor = Editor();
        _ = editor.Handle;
        await Ui.FirstFrame(editor);
        ContextMenuRequestedEventArgs? menu = null;
        editor.Session.ContextMenuRequested += (_, e) => menu = e;
        var at = new Point(700, 140);
        Ui.SendMessage(editor.Handle, Ui.WmRButtonDown, 2, Ui.LParam(at.X, at.Y));
        Ui.SendMessage(editor.Handle, Ui.WmRButtonUp, 0, Ui.LParam(at.X, at.Y));
        await Ui.Until(() => menu is not null, "the context menu");
        Assert.NotEmpty(menu!.Menu.Items);
        Assert.InRange(menu.Location.X, at.X - 1, at.X + 1);
        Assert.InRange(menu.Location.Y, at.Y - 1, at.Y + 1);
    });

    [Fact]
    public void HitTestAndDropWorkFromClientPoints() => Ui.Run(async () =>
    {
        using var editor = Editor();
        _ = editor.Handle;
        await editor.Commands.InsertTextAsync("Drop here");
        await Ui.FirstFrame(editor);
        var at = new Point(700, 140);
        var hit = await editor.HitTestAsync(at);
        Assert.True(hit.Editable, $"{at} isn't in editable text");
        await editor.DropAsync(at, EditorOperation.InsertText("Z"));
        Assert.Contains("Z", await Docx.TextAsync(editor.Session), StringComparison.Ordinal);
    });
}
