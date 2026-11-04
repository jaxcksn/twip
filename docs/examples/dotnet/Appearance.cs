using System.Windows.Forms;
using Twip;
using Twip.WinForms;

namespace Examples;

static class AppearanceExample
{
    static async Task Look(EditorSession session, DocumentEditor editor)
    {
        #region appearance
        // The chrome's scheme and accent. Any colour works as the accent: the dark scheme lightens it.
        await session.SetThemeAsync(ChromeTheme.Dark, ColorValue.FromRgb(0x7A, 0x3E, 0xC8));

        // Dark mode for the page. DarkWithLightPage keeps the page in its own colours with dark chrome.
        // Saving, PDF export and printing always use the document's colours.
        await session.SetAppearanceAsync(DocumentAppearance.Dark);

        // How much the editor measures while you drag and hover.
        await session.SetDetailAsync(MeasureDetail.Quiet);

        // The state reports the accent as drawn, and the text colour on it, for chrome of your own.
        var view = session.State.View;
        Console.WriteLine($"{view.Theme.Kind} {view.Accent} on {view.AccentOn}, page {view.Appearance}");

        // The control has the same settings as properties. While Windows' high contrast is on, it uses the
        // system colours and a light page instead, and follows Windows when the setting changes.
        editor.Theme = ChromeTheme.Dark;
        editor.Accent = Color.FromArgb(0x7A, 0x3E, 0xC8);
        editor.Appearance = DocumentAppearance.DarkWithLightPage;
        editor.MeasureDetail = MeasureDetail.Full;
        #endregion
    }

    static async Task Corrections(EditorSession session, Control owner)
    {
        #region autocorrect
        // Switch a rule off and replace the replacement list. Unset values are kept.
        await session.Commands.SetAutoCorrectAsync(new AutoCorrectChange(SmartQuotes: false, Entries: [new("adn", "and"), new("teh", "the")]));
        var options = session.State.AutoCorrect;
        Console.WriteLine($"smart quotes {options.SmartQuotes}, never corrected: {string.Join(", ", options.Never)}");

        // The correction at the caret: put back what was typed, or never make it again.
        await session.Commands.RevertAutoCorrectAsync();
        await session.Commands.NeverAutoCorrectAsync();

        // Handling PopoverRequested replaces DocumentEditor's own popovers with yours.
        session.PopoverRequested += (_, e) =>
        {
            if (e.Popover is not Popover.AutoCorrect ac) return;
            var menu = new ContextMenuStrip();
            menu.Items.Add($"Revert to {ac.From}", null, async (_, _) => await session.Commands.RevertAutoCorrectAsync());
            menu.Items.Add($"Never correct {ac.From}", null, async (_, _) => await session.Commands.NeverAutoCorrectAsync());
            menu.Show(owner, Point.Round(new PointF(ac.Anchor.Left, ac.Anchor.Bottom)));
        };
        #endregion
    }
}
