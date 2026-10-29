using Twip;

namespace Examples;

static class PlaceholdersExample
{
    static async Task Run(EditorSession session, byte[] logo, Func<Task<byte[]?>> pick)
    {
        #region placeholders
        // How unfilled picture placeholders look: by element, tag, tag prefix, or the default. A view setting.
        await session.Commands.SetPlaceholderLooksAsync(
        [
            new PlaceholderLook(PlaceholderMatch.ByTag("logo"), new PlaceholderAppearance(Image: logo, Mime: "image/png")),
            new PlaceholderLook(PlaceholderMatch.ByPrefix("photo."), new PlaceholderAppearance(Label: "Click to add a photo")),
            new PlaceholderLook(PlaceholderMatch.Default, new PlaceholderAppearance(Label: "Picture")),
        ]);

        // A click on an unfilled placeholder selects it and raises this: supply its picture.
        session.PlaceholderActivated += async (_, e) =>
        {
            if (await pick() is { } image) await session.Commands.FillPlaceholderAsync(image);
        };

        // Or fill every unfilled placeholder with one key, with no click.
        await session.Commands.FillPlaceholderAsync(logo, key: "logo");
        #endregion
    }
}
