using Twip;

namespace Examples;

/// <summary>Sub-templates: rules, editing one in the same session, and authoring.</summary>
public static class SubTemplatesExample
{
    /// <summary>Declares the references and shows the application's own banner while a sub-template is edited.</summary>
    public static async Task WireAsync(EditorSession session, Action<string?> banner)
    {
        await session.Commands.SetSubTemplateRulesAsync([new ReferenceRule("SUBTEMPLATE", "Sub-templates")]);
        session.EditingTargetChanged += (_, e) =>
            banner(e.Target.Kind == EditingTargetKind.SubTemplate ? $"Editing sub-template: {e.Target.Name}" : null);
        foreach (var t in await session.SubTemplates.ListAsync())
            Console.WriteLine($"{t.Name}: {t.References} references, {t.Elements.Count} fields");
        await session.SubTemplates.EditAsync("Violation");
        await session.SubTemplates.CloseAsync();
    }
}
