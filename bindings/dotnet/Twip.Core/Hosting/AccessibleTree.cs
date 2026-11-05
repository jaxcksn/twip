using Twip.Wire;

namespace Twip.Hosting;

/// <summary>The whole accessibility tree, kept from the engine's updates: the first is the tree, later ones replace
/// nodes and reorder children.</summary>
internal static class AccessibleTree
{
    /// <summary>The tree after <paramref name="update"/>; null until the first whole tree.</summary>
    public static AccessibleNode? Apply(AccessibleNode? tree, A11yUpdate update)
    {
        if (update.Full is { } full) return full;
        if (tree is null || update.Patch is null) return tree;
        // Replaced nodes not in the tree yet wait for the children change that places them.
        var detached = new Dictionary<long, AccessibleNode>();
        foreach (var change in update.Patch)
        {
            switch (change)
            {
                case A11yReplace r:
                    var (next, found) = Map(tree, r.Node.Id, _ => r.Node);
                    if (found) tree = next;
                    else detached[r.Node.Id] = r.Node;
                    break;
                case A11yChildren c:
                    (tree, _) = Map(tree, c.Parent, parent =>
                    {
                        var kids = parent.Children.ToDictionary(k => k.Id);
                        return parent with { Children = [.. c.Ids.Select(id => detached.TryGetValue(id, out var d) ? d : kids[id])] };
                    });
                    break;
            }
        }
        return tree;
    }

    /// <summary><paramref name="n"/> with the node <paramref name="id"/> replaced by <paramref name="change"/>'s result,
    /// and whether there was one.</summary>
    private static (AccessibleNode Node, bool Found) Map(AccessibleNode n, long id, Func<AccessibleNode, AccessibleNode> change)
    {
        if (n.Id == id) return (change(n), true);
        for (var i = 0; i < n.Children.Count; i++)
        {
            var (child, found) = Map(n.Children[i], id, change);
            if (!found) continue;
            var kids = n.Children.ToArray();
            kids[i] = child;
            return (n with { Children = kids }, true);
        }
        return (n, false);
    }
}
