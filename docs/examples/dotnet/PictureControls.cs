using Twip;

namespace Examples;

static class PictureControlsExample
{
    static async Task Run(EditorSession session, byte[] icon, Func<Task<byte[]?>> pick)
    {
        #region picture-controls
        // A picture control showing the application's placeholder picture: an SVG with percentage geometry lays
        // itself out at whatever size the control has.
        await session.Commands.InsertControlAsync(new InsertControlOptions(ControlKind.Picture, Tag: "signature",
            Options: new ControlOptions(Width: Length.FromPoints(144), Height: Length.FromPoints(36)),
            PlaceholderImage: new PlaceholderImage(icon, "image/svg+xml")));

        // A floating one keeps its place on the page whatever the text around it does.
        await session.Commands.InsertControlAsync(new InsertControlOptions(ControlKind.Picture, Tag: "seal",
            PlaceholderImage: new PlaceholderImage(icon, "image/svg+xml"),
            Float: new ControlFloat(ObjectWrap.Square,
                new HorizontalPosition(HorizontalReference.Margin, Offset: Length.FromPoints(360)),
                new VerticalPosition(VerticalReference.Paragraph, Offset: Length.FromPoints(0)))));

        // These pictures change only through FillControlAsync and SetControlAsync.
        await session.Commands.SetProtectedPictureControlsAsync(["signature", "seal"]);

        // A click on an unfilled picture control, a double-click or Enter activates it: supply its picture.
        session.PictureControlActivated += async (_, e) =>
        {
            if (await pick() is { } image) await session.Commands.FillControlAsync(e.Element, ControlContent.FromImage(image));
        };

        // The picture controls still showing their placeholder, to fill with no click.
        var unfilled = (await session.Elements.ListAsync()).Where(e => e.Size is not null && e.ShowingPlaceholder == true).ToList();
        #endregion
        _ = unfilled;
    }
}
