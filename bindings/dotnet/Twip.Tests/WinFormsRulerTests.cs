using Twip.WinForms;
using Xunit;

namespace Twip.Tests;

/// <summary>The engine-drawn rulers in the control, and editing tab stops on them with the pointer.</summary>
[Collection("WinForms")]
public sealed class WinFormsRulerTests
{
    private static DocumentEditor Editor() => new() { Width = 1400, Height = 700 };

    /// <summary>A row of pixels across the control's top.</summary>
    private static int[] TopRow(DocumentEditor editor)
    {
        using var shot = Ui.Snapshot(editor);
        return [.. Enumerable.Range(0, shot.Width).Select(x => shot.GetPixel(x, 8).ToArgb())];
    }

    [Fact]
    public void ShowingTheRulersDrawsThem() => Ui.Run(async () =>
    {
        using var editor = Editor();
        _ = editor.Handle;
        await Ui.FirstFrame(editor);
        var plain = TopRow(editor);
        editor.ShowHorizontalRuler = true;
        editor.ShowVerticalRuler = true;
        await Ui.Until(() => !TopRow(editor).SequenceEqual(plain), "the horizontal ruler", seconds: 5);
        using var shot = Ui.Snapshot(editor);
        Assert.NotEqual(editor.BackColor.ToArgb(), shot.GetPixel(8, 350).ToArgb());
    });

    [Fact]
    public void PointerInputOnTheRulerAddsMovesAndDeletesATabStop() => Ui.Run(async () =>
    {
        using var editor = Editor();
        _ = editor.Handle;
        editor.ShowHorizontalRuler = true;
        await Ui.FirstFrame(editor);
        IReadOnlyList<TabStop> Tabs() => editor.Session.State.Ruler.Horizontal.Tabs;
        Assert.Empty(Tabs());

        // A click on the ruler's text band adds a stop there; a drag moves it; a drag off the ruler deletes it.
        var (y, x) = (12, editor.Width / 2);
        Ui.SendMessage(editor.Handle, Ui.WmLButtonDown, 1, Ui.LParam(x, y));
        Ui.SendMessage(editor.Handle, Ui.WmLButtonUp, 0, Ui.LParam(x, y));
        await Ui.Until(() => Tabs().Count == 1, "a tab stop", seconds: 5);
        var added = Tabs()[0].Position;

        Ui.SendMessage(editor.Handle, Ui.WmLButtonDown, 1, Ui.LParam(x, y));
        Ui.SendMessage(editor.Handle, Ui.WmMouseMove, 1, Ui.LParam(x + 50, y));
        Ui.SendMessage(editor.Handle, Ui.WmLButtonUp, 0, Ui.LParam(x + 50, y));
        await Ui.Until(() => Tabs().Count == 1 && Tabs()[0].Position != added, "the stop to move");

        Ui.SendMessage(editor.Handle, Ui.WmLButtonDown, 1, Ui.LParam(x + 50, y));
        Ui.SendMessage(editor.Handle, Ui.WmMouseMove, 1, Ui.LParam(x + 50, y + 200));
        Ui.SendMessage(editor.Handle, Ui.WmLButtonUp, 0, Ui.LParam(x + 50, y + 200));
        await Ui.Until(() => Tabs().Count == 0, "the stop to go");
    });
}
