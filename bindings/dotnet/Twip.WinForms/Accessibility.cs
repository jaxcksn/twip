using System.Windows.Forms.Automation;
using WinRole = System.Windows.Forms.AccessibleRole;

namespace Twip.WinForms;

// shortcut: no UIA TextPattern; add a provider for richer screen-reader text navigation.

/// <summary>The control's accessible object: the document, with the engine's accessible tree below it.</summary>
internal sealed class DocumentAccessibleObject(DocumentEditor owner) : Control.ControlAccessibleObject(owner)
{
    private NodeAccessibleObject[] _children = [];

    public override WinRole Role => WinRole.Document;

    public override string? Name => owner.AccessibleName ?? "Document";

    public override int GetChildCount() => _children.Length;

    public override AccessibleObject? GetChild(int index) => (uint)index < (uint)_children.Length ? _children[index] : null;

    /// <summary>Replaces the tree below the document.</summary>
    public void Update(AccessibleNode tree) => _children = NodeAccessibleObject.Of(this, tree.Children);

    /// <summary>Says something to screen readers.</summary>
    public void Announce(string text) =>
        RaiseAutomationNotification(AutomationNotificationKind.Other, AutomationNotificationProcessing.ImportantMostRecent, text);
}

/// <summary>A paragraph, table, cell or other node of the accessible tree.</summary>
internal sealed class NodeAccessibleObject : AccessibleObject
{
    private readonly AccessibleObject _parent;
    private readonly AccessibleNode _node;
    private readonly NodeAccessibleObject[] _children;

    private NodeAccessibleObject(AccessibleObject parent, AccessibleNode node)
    {
        _parent = parent;
        _node = node;
        _children = Of(this, node.Children);
    }

    public static NodeAccessibleObject[] Of(AccessibleObject parent, IReadOnlyList<AccessibleNode> nodes) =>
        [.. nodes.Select(n => new NodeAccessibleObject(parent, n))];

    public override AccessibleObject Parent => _parent;

    public override WinRole Role => _node.Role switch
    {
        Twip.AccessibleRole.Document => WinRole.Document,
        Twip.AccessibleRole.Page => WinRole.Grouping,
        Twip.AccessibleRole.Paragraph or Twip.AccessibleRole.Heading => WinRole.Text,
        Twip.AccessibleRole.List => WinRole.List,
        Twip.AccessibleRole.ListItem => WinRole.ListItem,
        Twip.AccessibleRole.Table => WinRole.Table,
        Twip.AccessibleRole.Row => WinRole.Row,
        Twip.AccessibleRole.Cell => WinRole.Cell,
        Twip.AccessibleRole.Image => WinRole.Graphic,
        Twip.AccessibleRole.Link => WinRole.Link,
        Twip.AccessibleRole.Note => WinRole.Grouping,
        _ => WinRole.Grouping,
    };

    public override string? Name => _node.Name ?? _node.Text;

    public override string? Value => _node.Text;

    public override string? Description => _node.Level is { } level ? $"Heading level {level}" : null;

    // The tree carries no geometry: a node takes the bounds of the node it is in.
    public override Rectangle Bounds => _parent.Bounds;

    public override AccessibleStates State => AccessibleStates.ReadOnly;

    public override int GetChildCount() => _children.Length;

    public override AccessibleObject? GetChild(int index) => (uint)index < (uint)_children.Length ? _children[index] : null;
}
