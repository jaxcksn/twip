using System.Drawing;
using System.Text.Json.Serialization;
using Twip.Native;
using Twip.Wire;

namespace Twip;

/// <summary>Tracked revisions, comments and the people who made them.</summary>
public sealed class EditorReview
{
    private readonly EditorSession _session;

    internal EditorReview(EditorSession session) => _session = session;

    /// <summary>The revisions, in document order.</summary>
    /// <param name="ct">Cancels the request.</param>
    /// <returns>The revisions.</returns>
    public Task<IReadOnlyList<RevisionInfo>> GetRevisionsAsync(CancellationToken ct = default) => _session.CallAsync<IReadOnlyList<RevisionInfo>>(Method.Revisions, [], ct);

    /// <summary>The comment threads, in document order.</summary>
    /// <param name="ct">Cancels the request.</param>
    /// <returns>The threads.</returns>
    public Task<IReadOnlyList<CommentThread>> GetCommentsAsync(CancellationToken ct = default) => _session.CallAsync<IReadOnlyList<CommentThread>>(Method.Comments, [], ct);

    /// <summary>The people with revisions or comments.</summary>
    /// <param name="ct">Cancels the request.</param>
    /// <returns>The people.</returns>
    public Task<IReadOnlyList<Person>> GetPeopleAsync(CancellationToken ct = default) => _session.CallAsync<IReadOnlyList<Person>>(Method.People, [], ct);

    /// <summary>
    /// Card tops that don't overlap, <paramref name="gap"/> apart: card i, <paramref name="heights"/>[i] tall, is for
    /// <paramref name="anchors"/>[i], in the order given. The first active card sits level with its line; the cards
    /// before it rise above it only as far as they must, and the cards after it fall below it. No card goes above 0:
    /// when the cards before the active one don't fit above it, they start at 0 and push it down. With no active card,
    /// each card is level with its line or just below the card before it.
    /// </summary>
    /// <param name="anchors">The anchors.</param>
    /// <param name="heights">The card heights.</param>
    /// <param name="gap">The minimum gap between cards.</param>
    /// <returns>Each card's top, in the anchors' coordinates.</returns>
    public IReadOnlyList<double> StackBalloons(IReadOnlyList<CommentAnchor> anchors, IReadOnlyList<double> heights, double gap = 8)
    {
        ArgumentNullException.ThrowIfNull(anchors);
        ArgumentNullException.ThrowIfNull(heights);
        // The engine's algorithm (api::review::stack_balloons), run here so placing cards needs no engine call.
        var tops = new double[Math.Min(anchors.Count, heights.Count)];
        for (var i = 0; i < tops.Length; i++) tops[i] = anchors[i].LineTop;
        var active = 0; // the first active card, or the first card
        while (active < tops.Length && !anchors[active].Active) active++;
        if (active == tops.Length) active = 0;
        for (var i = active - 1; i >= 0; i--) tops[i] = Math.Min(tops[i], tops[i + 1] - gap - heights[i]);
        var bottom = double.NegativeInfinity;
        for (var i = 0; i < tops.Length; i++)
        {
            tops[i] = Math.Max(Math.Max(tops[i], 0), bottom + gap);
            bottom = tops[i] + heights[i];
        }
        return tops;
    }

    /// <summary>The visible comments' anchors. Attached only; also <see cref="EditorSession.CommentAnchorsChanged"/>.</summary>
    public IReadOnlyList<CommentAnchor> Anchors => _session.CommentAnchors;
}

