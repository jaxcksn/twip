using Twip;

namespace Examples;

static class ShapesExample
{
    static async Task Run(EditorSession session)
    {
        #region shapes
        // A shape at the caret's paragraph, in front of the text, in the theme's default style. It is selected.
        await session.Commands.InsertShapeAsync(new ShapePreset("roundRect"), Length.FromPoints(144), Length.FromPoints(72));

        // The selection describes it as written: a theme colour stays a theme colour.
        var fill = session.State.Selection.Object?.Shape?.Fill;
        if (fill?.FromStyle is { } styled) Console.WriteLine($"styled fill {styled}");

        // Accent 1 at 75% luminance, a 2 pt dashed outline, a soft shadow, a quarter turn.
        var accent = ShapeColor.FromTheme(ThemeColor.Accent1, new ColorTransform(ColorTransformKind.LumMod, 0.75));
        await session.Commands.SetShapeFillAsync(new FillValue.Solid(accent));
        await session.Commands.SetShapeLineAsync(new LineChange(Width: Length.FromPoints(2), Dash: "dash"));
        var shadow = new Shadow(ShapeColor.FromRgb(ColorValue.FromRgb(0, 0, 0)), Length.FromPoints(4), Length.FromPoints(3), 45);
        await session.Commands.SetShapeEffectsAsync(new EffectsChange(OuterShadow: shadow));
        await session.Commands.SetObjectRotationAsync(90);

        // FromStyle takes the explicit fill away again.
        await session.Commands.SetShapeFillAsync(new FillValue.FromStyle());

        // Word's gallery groups, for an "Insert shape" menu, and the theme colours for a colour picker.
        foreach (var group in session.ShapeGallery) Console.WriteLine($"{group.Label}: {group.Presets.Count}");
        Console.WriteLine(string.Join(", ", session.State.ThemeColors));
        #endregion
    }
}
