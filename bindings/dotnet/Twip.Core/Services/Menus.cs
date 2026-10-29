using System.Text.Json.Nodes;
using System.Text.Json.Serialization;
using Twip.Native;
using Twip.Wire;

namespace Twip;

/// <summary>The context menu model: what to show for the selection, and running an item.</summary>
public sealed class EditorMenus
{
    private readonly EditorSession _session;

    internal EditorMenus(EditorSession session) => _session = session;

    /// <summary>The menu for the selection, as the context-menu key would open it.</summary>
    /// <param name="ct">Cancels the request.</param>
    /// <returns>The menu model.</returns>
    public Task<ContextMenu> GetContextMenuAsync(CancellationToken ct = default) => _session.CallAsync<ContextMenu>(Method.ContextMenu, [], ct);

    /// <summary>Runs a menu item's action as one transaction.</summary>
    /// <param name="action">The item's action.</param>
    /// <param name="ct">Cancels the action before it runs.</param>
    /// <returns>Completes when the action has committed.</returns>
    public Task InvokeAsync(MenuAction action, CancellationToken ct = default)
    {
        ArgumentNullException.ThrowIfNull(action);
        // An attached host's Paste reads the system clipboard, as Ctrl+V does.
        if (_session.Host is { } host && action.Wire?["command"] is JsonObject command && (string?)command["type"] == "paste")
            return host.PasteAsync((string?)command["as"] == "text", ct);
        return _session.CallAsync(Method.Invoke, WireJson.ToUtf8(action), ct);
    }
}

/// <summary>A context menu: what it is for, and its items.</summary>
/// <param name="Target">What the menu is for.</param>
/// <param name="Items">The items, in order.</param>
public sealed record ContextMenu(MenuTarget Target, IReadOnlyList<MenuItem> Items);

/// <summary>A context menu item.</summary>
[JsonPolymorphic(TypeDiscriminatorPropertyName = "kind")]
[JsonDerivedType(typeof(CommandItem), "command")]
[JsonDerivedType(typeof(SuggestionItem), "suggestion")]
[JsonDerivedType(typeof(SubmenuItem), "submenu")]
[JsonDerivedType(typeof(SeparatorItem), "separator")]
public abstract record MenuItem
{
    private protected MenuItem() { }
}

/// <summary>A command item.</summary>
/// <param name="Command">The command it runs.</param>
/// <param name="Label">The label.</param>
/// <param name="Enabled">Whether it can run.</param>
/// <param name="Checked">Whether it shows as checked.</param>
/// <param name="Shortcut">Its shortcut, as a display string.</param>
/// <param name="Action">What to pass to <see cref="EditorMenus.InvokeAsync"/>.</param>
public sealed record CommandItem(
    CommandId Command, string Label, bool Enabled,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? Checked,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? Shortcut,
    MenuAction Action) : MenuItem;

/// <summary>A spelling item: a suggestion, Ignore, or Add to Dictionary.</summary>
/// <param name="Label">The label.</param>
/// <param name="Action">What to pass to <see cref="EditorMenus.InvokeAsync"/>.</param>
public sealed record SuggestionItem(string Label, MenuAction Action) : MenuItem;

/// <summary>A submenu.</summary>
/// <param name="Label">The label.</param>
/// <param name="Items">Its items.</param>
public sealed record SubmenuItem(string Label, IReadOnlyList<MenuItem> Items) : MenuItem;

/// <summary>A separator.</summary>
public sealed record SeparatorItem : MenuItem;

/// <summary>An item's action. Opaque: only passed back to <see cref="EditorMenus.InvokeAsync"/>.</summary>
[JsonConverter(typeof(MenuActionConverter))]
public sealed class MenuAction
{
    internal MenuAction(JsonNode? wire) => Wire = wire;

    internal JsonNode? Wire { get; }
}

/// <summary>What a context menu is for.</summary>
[JsonPolymorphic(TypeDiscriminatorPropertyName = "kind")]
[JsonDerivedType(typeof(MenuTarget.Text), "text")]
[JsonDerivedType(typeof(MenuTarget.Misspelling), "misspelling")]
[JsonDerivedType(typeof(MenuTarget.Link), "link")]
[JsonDerivedType(typeof(MenuTarget.Table), "table")]
[JsonDerivedType(typeof(MenuTarget.Picture), "picture")]
[JsonDerivedType(typeof(MenuTarget.TextBox), "textBox")]
[JsonDerivedType(typeof(MenuTarget.Element), "element")]
[JsonDerivedType(typeof(MenuTarget.Revision), "revision")]
[JsonDerivedType(typeof(MenuTarget.Comment), "comment")]
[JsonDerivedType(typeof(MenuTarget.HeaderFooter), "headerFooter")]
public abstract record MenuTarget
{
    private protected MenuTarget() { }

    /// <summary>Text.</summary>
    public sealed record Text : MenuTarget;

    /// <summary>A misspelled word.</summary>
    /// <param name="Word">The word.</param>
    /// <param name="Language">Its language.</param>
    public sealed record Misspelling(string Word, string Language) : MenuTarget;

    /// <summary>A hyperlink.</summary>
    /// <param name="Address">Its address.</param>
    public sealed record Link(string Address) : MenuTarget;

    /// <summary>A table.</summary>
    public sealed record Table : MenuTarget;

    /// <summary>A picture.</summary>
    public sealed record Picture : MenuTarget;

    /// <summary>A text box.</summary>
    public sealed record TextBox : MenuTarget;

    /// <summary>A merge-designer element.</summary>
    /// <param name="Id">The element.</param>
    public sealed record Element(ElementId Id) : MenuTarget;

    /// <summary>A revision.</summary>
    /// <param name="Id">The revision.</param>
    public sealed record Revision(RevisionId Id) : MenuTarget;

    /// <summary>A comment.</summary>
    /// <param name="Id">The comment.</param>
    public sealed record Comment(CommentId Id) : MenuTarget;

    /// <summary>A header or footer.</summary>
    public sealed record HeaderFooter : MenuTarget;
}
