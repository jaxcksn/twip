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

/// <summary>A kind of revision.</summary>
public enum RevisionKind
{
    /// <summary>Inserted text.</summary>
    Insertion,
    /// <summary>Deleted text.</summary>
    Deletion,
    /// <summary>Text moved from here.</summary>
    MoveFrom,
    /// <summary>Text moved to here.</summary>
    MoveTo,
    /// <summary>A formatting change.</summary>
    Formatting,
    /// <summary>A paragraph property change.</summary>
    ParagraphProperties,
    /// <summary>A section property change.</summary>
    SectionProperties,
    /// <summary>A table property change.</summary>
    TableProperties,
    /// <summary>An inserted row.</summary>
    RowInsertion,
    /// <summary>A deleted row.</summary>
    RowDeletion,
    /// <summary>An inserted cell.</summary>
    CellInsertion,
    /// <summary>A deleted cell.</summary>
    CellDeletion,
    /// <summary>Merged cells.</summary>
    CellMerge,
}

/// <summary>A person with revisions or comments.</summary>
/// <param name="Name">The name.</param>
/// <param name="Initials">The initials.</param>
/// <param name="Identity">The sign-in identity.</param>
/// <param name="Color">The colour their markup shows in.</param>
public sealed record Person(
    string Name,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? Initials,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] Identity? Identity,
    ColorValue Color);

/// <summary>A tracked revision.</summary>
/// <param name="Id">The revision.</param>
/// <param name="Kind">What changed.</param>
/// <param name="Author">Who changed it.</param>
/// <param name="Time">When, when known.</param>
/// <param name="Text">The inserted, deleted or moved text.</param>
/// <param name="Description">A description, such as "Formatted: Bold, Italic".</param>
/// <param name="Page">The page.</param>
/// <param name="Story">The story.</param>
/// <param name="Pair">A move's other half.</param>
public sealed record RevisionInfo(
    RevisionId Id, RevisionKind Kind, Person Author,
    [property: JsonConverter(typeof(NullableUtcTimeConverter))] DateTimeOffset? Time,
    string Text, string Description, int Page, Story Story,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] RevisionId? Pair = null);

/// <summary>A comment thread.</summary>
/// <param name="Id">The thread: its first comment's id.</param>
/// <param name="Done">Whether it is resolved.</param>
/// <param name="AnchorText">The commented text.</param>
/// <param name="Page">The page.</param>
/// <param name="Comments">The first comment, then the replies.</param>
public sealed record CommentThread(CommentId Id, bool Done, string AnchorText, int Page, IReadOnlyList<Comment> Comments);

/// <summary>A comment.</summary>
/// <param name="Id">The comment.</param>
/// <param name="Author">Who wrote it.</param>
/// <param name="Time">When, when known.</param>
/// <param name="Body">Its text, with formatting.</param>
/// <param name="PlainText">Its text, without formatting.</param>
public sealed record Comment(
    CommentId Id, Person Author,
    [property: JsonConverter(typeof(NullableUtcTimeConverter))] DateTimeOffset? Time,
    RichText Body, string PlainText);

/// <summary>Where a visible comment is, in control client pixels.</summary>
/// <param name="Thread">The thread.</param>
/// <param name="Page">The page.</param>
/// <param name="Active">Whether it is the active thread.</param>
/// <param name="Done">Whether it is resolved.</param>
/// <param name="Start">The range's first character, at its baseline.</param>
/// <param name="End">The range's last character, at its baseline.</param>
/// <param name="LineTop">The first line's top.</param>
/// <param name="LineBottom">The first line's bottom.</param>
/// <param name="PageEdge">The x where a leader line leaves the page.</param>
public sealed record CommentAnchor(
    CommentId Thread, int Page, bool Active, bool Done,
    [property: JsonConverter(typeof(WirePointConverter))] PointF Start,
    [property: JsonConverter(typeof(WirePointConverter))] PointF End,
    double LineTop, double LineBottom, double PageEdge);
