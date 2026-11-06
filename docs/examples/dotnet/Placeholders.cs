using Twip;

namespace Examples;

static class PlaceholdersExample
{
    static async Task Run(EditorSession session, byte[] logo, byte[] icon, Func<Task<byte[]?>> pick)
    {
        #region placeholders
        // A picture control showing the application's placeholder picture: an SVG with percentage geometry lays
        // itself out at whatever size the control has.
        await session.Commands.InsertControlAsync(new InsertControlOptions(ControlKind.Picture, Tag: "logo",
            Options: new ControlOptions(Width: Length.FromPoints(144), Height: Length.FromPoints(72)),
            PlaceholderImage: new PlaceholderImage(icon, "image/svg+xml")));

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
