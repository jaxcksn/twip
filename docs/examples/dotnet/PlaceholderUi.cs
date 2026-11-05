using Twip;

namespace Examples;

static class PlaceholderUiExample
{
    static async Task Run(EditorSession session, Action<PlaceholderTarget?> popover, Func<string, Task<byte[]?>> sign)
    {
        #region ui
        // Placeholders drawn as editor UI: field marks, repeating sections, sub-template cards and input blocks.
        await session.SetModeAsync(EditorMode.Design);
        await session.SetPlaceholderUiAsync(true);
        await session.Commands.SetFieldTypesAsync(new Dictionary<string, string> { ["MERGEFIELD ApplicantName"] = "Text" });

        // Your popover, at the selected field; null when the selection leaves it.
        session.PlaceholderSelected += (_, e) => popover(e.Target);

        // Signature blocks: their colour, label, hint and buttons.
        await session.Commands.SetPlaceholderLooksAsync(
        [
            new PlaceholderLook(PlaceholderMatch.ByPrefix("Signature"), new PlaceholderAppearance(Fill: ColorValue.FromRgb(0x2E, 0x7D, 0x32), Label: "Signature"))
            {
                Hint = "Click to sign",
                Actions = [new PlaceholderAction("sign", "Sign", Primary: true), new PlaceholderAction("upload", "Upload image")],
            },
        ]);
        session.PlaceholderActivated += async (_, e) =>
        {
            if (e.Action is "sign" or "replace" && await sign(e.Key) is { } image) await session.Commands.FillPlaceholderAsync(image, key: e.Key);
        };

        // Fields the data source lacks, and the sub-templates' versions.
        await session.Commands.SetUnmappedFieldsAsync(["Inspector_Name"]);
        await session.Commands.SetSubTemplateVersionsAsync([new SubTemplateVersion("Appeals notice", "v4", Newer: "v5 available")]);
        session.SubTemplateUpdateRequested += (_, e) => Console.WriteLine($"Update {e.Name} in {e.Category}");
        #endregion
    }
}
