using Twip.WinForms;
using Xunit;

namespace Twip.Tests;

/// <summary>The control's look properties, and its own popovers and their answers.</summary>
[Collection("WinForms")]
public sealed class WinFormsPopoverTests
{
    private static DocumentEditor Editor() => new() { Width = 1000, Height = 600, ShowHorizontalRuler = true };

    [Fact]
    public void TheLookPropertiesReachTheState() => Ui.Run(async () =>
    {
        using var editor = Editor();
        _ = editor.Handle;
        await Ui.FirstFrame(editor);
        editor.Theme = ChromeTheme.Dark;
        editor.Appearance = DocumentAppearance.DarkWithLightPage;
        editor.MeasureDetail = MeasureDetail.Full;
        editor.Accent = Color.FromArgb(0x7A, 0x3E, 0xC8);
        // High contrast (when the machine has it on) keeps the page light, and wins over the application's scheme.
        var hc = SystemInformation.HighContrast;
        await Ui.Until(() =>
        {
            var v = editor.Session.State.View;
            return v.Detail == MeasureDetail.Full
                && v.Theme.Kind == (hc ? ChromeThemeKind.HighContrast : ChromeThemeKind.Dark)
                && v.Appearance == (hc ? DocumentAppearance.Light : DocumentAppearance.DarkWithLightPage);
        }, "the look in the state");
        // Until the application sets BackColor, it follows the theme's desk.
        await Ui.Until(() => editor.Session.State.View.Background.Color?.ToArgb() == editor.BackColor.ToArgb(), "the desk colour");
    });

    [Fact]
    public void TheTabTypePickerAnswersWithItsCommand() => Ui.Run(async () =>
    {
        using var editor = Editor();
        _ = editor.Handle;
        await Ui.FirstFrame(editor);
        await editor.Commands.ShowTabTypesAsync();
        await Ui.Until(() => editor.OpenPopover is not null, "the tab type picker");
        var picker = editor.OpenPopover!;
        Assert.Equal(7, picker.Entries.Count);
        Assert.Equal(("Left", true), picker.Entries[0]);
        picker.Pick("Decimal");
        await Ui.Until(() => editor.Session.State.View.TabSelector == TabSelector.Decimal, "the decimal tab type");
        Assert.Null(editor.OpenPopover);
        Assert.Equal(TabSelector.Decimal, editor.TabSelector);
    });

    [Fact]
    public void TheTabStopPopoverStaysOpenForAlignmentAndClears() => Ui.Run(async () =>
    {
        using var editor = Editor();
        _ = editor.Handle;
        await editor.Commands.InsertTextAsync("Name");
        await editor.Commands.AddTabStopAsync(Length.FromPoints(144), TabAlignment.Center);
        await Ui.FirstFrame(editor);
        await editor.Commands.ShowTabStopAsync();
        await Ui.Until(() => editor.OpenPopover is not null, "the tab stop popover");
        var popover = editor.OpenPopover!;
        var stop = Assert.IsType<Popover.TabStop>(popover.Popover);
        Assert.Equal(144, stop.Position.Points);
        Assert.Contains(("Center", true), popover.Entries);

        popover.Pick("Right");
        popover.Pick("Dots ·····");
        await Ui.Until(() => editor.Session.State.Ruler.Horizontal.Tabs.Any(t => t is { Alignment: TabAlignment.Right, Leader: TabLeader.Dots }), "a right tab with dots");
        Assert.Same(popover, editor.OpenPopover);
        Assert.Contains(("Right", true), popover.Entries);

        popover.Pick("Clear tab stop");
        await Ui.Until(() => editor.Session.State.Ruler.Horizontal.Tabs.Count == 0, "the stop cleared");
        Assert.Null(editor.OpenPopover);
    });

    [Fact]
    public void AnApplicationHandlerTakesThePopoverInstead() => Ui.Run(async () =>
    {
        using var editor = Editor();
        _ = editor.Handle;
        await Ui.FirstFrame(editor);
        Popover? got = null;
        editor.Session.PopoverRequested += (_, e) => got = e.Popover;
        await editor.Commands.ShowTabTypesAsync();
        await Ui.Until(() => got is not null, "the application's popover");
        Assert.IsType<Popover.TabType>(got);
        Assert.Null(editor.OpenPopover);
    });

    [Fact]
    public void LengthsShowInTheRulerUnit()
    {
        Assert.Equal("2.75″", PopoverDropDown.Format(Length.FromInches(2.75), RulerUnit.Inches));
        Assert.Equal("2.54 cm", PopoverDropDown.Format(Length.FromInches(1), RulerUnit.Centimeters));
        Assert.Equal("36 pt", PopoverDropDown.Format(Length.FromPoints(36), RulerUnit.Points));
    }
}
