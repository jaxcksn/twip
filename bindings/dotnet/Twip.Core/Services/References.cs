using Twip.Native;
using Twip.Wire;

namespace Twip;

/// <summary>Tables of contents, captions and cross-references: what a References tab and its panes read.</summary>
public sealed class EditorReferences
{
    private readonly EditorSession _session;

    internal EditorReferences(EditorSession session) => _session = session;

    /// <summary>
    /// The items a cross-reference of <paramref name="kind"/> can point to, as Word's Cross-reference dialog lists them
    /// (bookmarks by name, the rest in document order): <see cref="EditorCommands.InsertCrossReferenceAsync"/> names one by its place here.
    /// </summary>
    /// <param name="kind">What it points to.</param>
    /// <param name="ct">Cancels the request.</param>
    /// <returns>The targets.</returns>
    public Task<IReadOnlyList<CrossReferenceTarget>> GetCrossReferenceTargetsAsync(CrossReferenceKind kind, CancellationToken ct = default) =>
        _session.CallAsync<IReadOnlyList<CrossReferenceTarget>>(Method.CrossReferenceTargets, WireJson.ToUtf8(kind), ct);

    /// <summary>The labels a caption can take: Equation, Figure and Table, then <paramref name="custom"/> (the host's), then the document's own.</summary>
    /// <param name="custom">The host's own labels.</param>
    /// <param name="ct">Cancels the request.</param>
    /// <returns>The labels.</returns>
    public Task<IReadOnlyList<string>> GetCaptionLabelsAsync(IReadOnlyList<string>? custom = null, CancellationToken ct = default) =>
        _session.CallAsync<IReadOnlyList<string>>(Method.CaptionLabels, WireJson.ToUtf8(custom ?? []), ct);

    /// <summary>The document's tables of contents and of figures, and the one the selection is in. Ask on selection changes.</summary>
    /// <param name="ct">Cancels the request.</param>
    /// <returns>The tables.</returns>
    public Task<TablesOfContents> GetTablesOfContentsAsync(CancellationToken ct = default) =>
        _session.CallAsync<TablesOfContents>(Method.TablesOfContents, [], ct);

    /// <summary>The entries a table with <paramref name="options"/> would collect now (empty options: Word's default), for a preview.</summary>
    /// <param name="options">What it collects.</param>
    /// <param name="ct">Cancels the request.</param>
    /// <returns>The entries.</returns>
    public Task<IReadOnlyList<TocEntryPreview>> GetTocEntriesAsync(TableOfContentsOptions options, CancellationToken ct = default) =>
        _session.CallAsync<IReadOnlyList<TocEntryPreview>>(Method.TocEntries, WireJson.ToUtf8(options), ct);
}
