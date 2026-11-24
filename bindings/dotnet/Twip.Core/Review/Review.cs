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

    /// <summary>The revision balloons' anchors. Attached only; also <see cref="EditorSession.RevisionBalloonsChanged"/>.</summary>
    public IReadOnlyList<RevisionAnchor> RevisionBalloons => _session.RevisionAnchors;

    /// <summary>Where and how a host places attached balloons; <see cref="BalloonLeft"/> applies them. Not saved.</summary>
    public BalloonOptions BalloonOptions
    {
        get => Volatile.Read(ref _balloons);
        set => Volatile.Write(ref _balloons, (value ?? throw new ArgumentNullException(nameof(value))).Clamped());
    }

    private BalloonOptions _balloons = BalloonOptions.Default;

    /// <summary>A balloon's width in the anchors' pixels, by <see cref="BalloonOptions"/>.</summary>
    /// <param name="pageLeft">The page's left edge.</param>
    /// <param name="pageEdge">The page's right edge.</param>
    /// <param name="pixelsPerPoint">The view's pixels per point (the zoom).</param>
    /// <returns>The width.</returns>
    public double BalloonWidth(double pageLeft, double pageEdge, double pixelsPerPoint)
    {
        var o = BalloonOptions;
        return o.WidthIsPercent ? (pageEdge - pageLeft) * o.Width / 100 : o.Width * pixelsPerPoint;
    }

    /// <summary>Where a balloon <paramref name="width"/> wide goes beside a page, by <see cref="BalloonOptions"/>: <paramref name="gap"/> right of its right edge, or left of its left edge.</summary>
    /// <param name="pageLeft">The page's left edge.</param>
    /// <param name="pageEdge">The page's right edge.</param>
    /// <param name="width">The balloon's width.</param>
    /// <param name="gap">The space between the page and the balloon.</param>
    /// <returns>The balloon's left.</returns>
    public double BalloonLeft(double pageLeft, double pageEdge, double width, double gap = 8) =>
        BalloonOptions.Side == BalloonSide.Left ? pageLeft - gap - width : pageEdge + gap;
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
    ColorValue Color)
{
    /// <summary>Show Markup shows their revisions and comments.</summary>
    public bool Visible { get; init; } = true;
}

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
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] RevisionId? Pair = null)
{
    /// <summary>Show Markup shows it (its kind and its reviewer); one not shown draws as if accepted.</summary>
    public bool Shown { get; init; } = true;
}

/// <summary>A comment thread.</summary>
/// <param name="Id">The thread: its first comment's id.</param>
/// <param name="Done">Whether it is resolved.</param>
/// <param name="AnchorText">The commented text, cut to its first 1,000 characters.</param>
/// <param name="Page">The page.</param>
/// <param name="Comments">The first comment, then the replies.</param>
public sealed record CommentThread(CommentId Id, bool Done, string AnchorText, int Page, IReadOnlyList<Comment> Comments)
{
    /// <summary>Show Markup shows the thread.</summary>
    public bool Shown { get; init; } = true;
}

/// <summary>A comment.</summary>
/// <param name="Id">The comment.</param>
/// <param name="Author">Who wrote it.</param>
/// <param name="Time">When, when known.</param>
/// <param name="Body">Its text, with formatting.</param>
/// <param name="PlainText">Its text, without formatting.</param>
public sealed record Comment(
    CommentId Id, Person Author,
    [property: JsonConverter(typeof(NullableUtcTimeConverter))] DateTimeOffset? Time,
    RichText Body, string PlainText)
{
    /// <summary>Show Markup shows it: a reply shows exactly when its thread does.</summary>
    public bool Shown { get; init; } = true;
}

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
    double LineTop, double LineBottom, double PageEdge)
{
    /// <summary>The x of the page's left edge, for balloons on the left.</summary>
    public double PageLeft { get; init; }
}

/// <summary>Where a revision balloon goes, for a host that draws balloons beside the page (All Markup with balloons).</summary>
/// <param name="Revision">The revision; <see cref="EditorReview.GetRevisionsAsync"/> gives its text and description.</param>
/// <param name="Kind">What changed.</param>
/// <param name="Author">Who made it.</param>
/// <param name="Page">The page.</param>
/// <param name="Start">The revision's first character, at its baseline.</param>
/// <param name="LineTop">The first line's top.</param>
/// <param name="LineBottom">The first line's bottom.</param>
/// <param name="PageEdge">The x where a leader line leaves the page.</param>
/// <param name="PageLeft">The x of the page's left edge.</param>
public sealed record RevisionAnchor(
    RevisionId Revision, RevisionKind Kind, string Author, int Page,
    [property: JsonConverter(typeof(WirePointConverter))] PointF Start,
    double LineTop, double LineBottom, double PageEdge, double PageLeft);

/// <summary>Where revisions show in All Markup.</summary>
public enum BalloonMode
{
    /// <summary>Deletions and formatting changes in balloons beside the page, insertions in place.</summary>
    Balloons,
    /// <summary>Every revision in place; comments stay in their cards.</summary>
    Inline,
    /// <summary>Formatting changes in balloons, insertions and deletions in place.</summary>
    CommentsAndFormatting,
}

/// <summary>Which kinds of markup show, and whose.</summary>
/// <param name="InsertionsDeletions">Insertions and deletions: of text, moves, rows and cells.</param>
/// <param name="Formatting">Formatting changes.</param>
/// <param name="Comments">Comments.</param>
/// <param name="HiddenReviewers">The reviewers whose markup doesn't show, by name.</param>
public sealed record ShowMarkup(bool InsertionsDeletions, bool Formatting, bool Comments, IReadOnlyList<string> HiddenReviewers);

/// <summary>How a revision marks its text; <see cref="Hidden"/>, <see cref="Caret"/> and <see cref="Pound"/> are for deleted and moved-away text only.</summary>
public enum TextMark
{
    /// <summary>Neither style nor colour.</summary>
    None,
    /// <summary>In the mark's colour only.</summary>
    ColorOnly,
    /// <summary>Bold.</summary>
    Bold,
    /// <summary>Italic.</summary>
    Italic,
    /// <summary>Underlined.</summary>
    Underline,
    /// <summary>Double underlined.</summary>
    DoubleUnderline,
    /// <summary>Struck through.</summary>
    Strikethrough,
    /// <summary>Double struck through.</summary>
    DoubleStrikethrough,
    /// <summary>The text doesn't show.</summary>
    Hidden,
    /// <summary>The text shows as one <c>^</c>.</summary>
    Caret,
    /// <summary>The text shows as one <c>#</c>.</summary>
    Pound,
}

/// <summary>The colour a revision mark or cell highlight draws in: the author's, the text's own, or a fixed one.</summary>
[JsonConverter(typeof(MarkColorConverter))]
public readonly record struct MarkColor
{
    private MarkColor(MarkColorKind kind, ColorValue? fixedColor) => (Kind, Fixed) = (kind, fixedColor);

    /// <summary>Which colour.</summary>
    public MarkColorKind Kind { get; }

    /// <summary>The fixed colour, for <see cref="MarkColorKind.Fixed"/>.</summary>
    public ColorValue? Fixed { get; }

    /// <summary>The revision's author's colour.</summary>
    public static MarkColor ByAuthor => new(MarkColorKind.ByAuthor, null);

    /// <summary>The text's own colour (a cell's own fill).</summary>
    public static MarkColor Auto => new(MarkColorKind.Auto, null);

    /// <summary>One colour.</summary>
    /// <param name="color">The colour.</param>
    /// <returns>The mark colour.</returns>
    public static MarkColor Of(ColorValue color) => new(MarkColorKind.Fixed, color);
}

/// <summary>What a <see cref="MarkColor"/> is.</summary>
public enum MarkColorKind
{
    /// <summary>The revision's author's.</summary>
    ByAuthor,
    /// <summary>The text's own.</summary>
    Auto,
    /// <summary>A fixed colour.</summary>
    Fixed,
}

/// <summary>How one kind of revision draws.</summary>
/// <param name="Mark">How the text is marked.</param>
/// <param name="Color">The mark's colour.</param>
public sealed record MarkLook(TextMark Mark, MarkColor Color);

/// <summary>A change of how one kind of revision draws; unset values are kept.</summary>
/// <param name="Mark">How the text is marked.</param>
/// <param name="Color">The mark's colour.</param>
public sealed record MarkLookChange(
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] TextMark? Mark = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] MarkColor? Color = null);

/// <summary>Which side of the text change bars go.</summary>
public enum BarSide
{
    /// <summary>No change bars.</summary>
    None,
    /// <summary>Left of the text.</summary>
    Left,
    /// <summary>Right of the text.</summary>
    Right,
    /// <summary>The page's outside edge.</summary>
    Outside,
}

/// <summary>How revisions draw (Track Changes Options). Not saved.</summary>
/// <param name="Insertions">Inserted text.</param>
/// <param name="Deletions">Deleted text.</param>
/// <param name="Formatting">Text with formatting changes.</param>
/// <param name="MovedFrom">Text moved away.</param>
/// <param name="MovedTo">Text moved here.</param>
/// <param name="ChangeBars">Where change bars go.</param>
/// <param name="ChangeBarColor">The change bars' colour, or <see cref="ColorValue.Auto"/>: red in Simple Markup, grey in All Markup.</param>
/// <param name="InsertedCells">Inserted cells' fill.</param>
/// <param name="DeletedCells">Deleted cells' fill.</param>
/// <param name="MergedCells">Merged cells' fill.</param>
/// <param name="SplitCells">Split cells' fill.</param>
public sealed record MarkupStyle(
    MarkLook Insertions, MarkLook Deletions, MarkLook Formatting, MarkLook MovedFrom, MarkLook MovedTo,
    BarSide ChangeBars, ColorValue ChangeBarColor,
    MarkColor InsertedCells, MarkColor DeletedCells, MarkColor MergedCells, MarkColor SplitCells);

/// <summary>A change of how revisions draw; unset values are kept.</summary>
/// <param name="Insertions">Inserted text.</param>
/// <param name="Deletions">Deleted text (hidden, caret and pound marks too).</param>
/// <param name="Formatting">Text with formatting changes.</param>
/// <param name="MovedFrom">Text moved away (hidden, caret and pound marks too).</param>
/// <param name="MovedTo">Text moved here.</param>
/// <param name="ChangeBars">Where change bars go.</param>
/// <param name="ChangeBarColor">The change bars' colour, or <see cref="ColorValue.Auto"/>.</param>
/// <param name="InsertedCells">Inserted cells' fill.</param>
/// <param name="DeletedCells">Deleted cells' fill.</param>
/// <param name="MergedCells">Merged cells' fill.</param>
/// <param name="SplitCells">Split cells' fill.</param>
public sealed record MarkupStyleChange(
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] MarkLookChange? Insertions = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] MarkLookChange? Deletions = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] MarkLookChange? Formatting = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] MarkLookChange? MovedFrom = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] MarkLookChange? MovedTo = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] BarSide? ChangeBars = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] ColorValue? ChangeBarColor = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] MarkColor? InsertedCells = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] MarkColor? DeletedCells = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] MarkColor? MergedCells = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] MarkColor? SplitCells = null);

/// <summary>How many revisions of each kind, and comment threads.</summary>
/// <param name="Insertions">Inserted text, rows and cells.</param>
/// <param name="Deletions">Deleted text, rows and cells.</param>
/// <param name="Moves">Both halves of each move.</param>
/// <param name="Formatting">Formatting changes.</param>
/// <param name="Comments">Comment threads.</param>
public sealed record MarkupCounts(int Insertions, int Deletions, int Moves, int Formatting, int Comments);

/// <summary>The review counts: of everything, and of what Show Markup shows.</summary>
/// <param name="All">Every revision and thread.</param>
/// <param name="Shown">Those Show Markup shows.</param>
public sealed record ReviewCounts(MarkupCounts All, MarkupCounts Shown);

/// <summary>Where and how attached balloons go (Track Changes Options' balloon settings). Not saved.</summary>
/// <param name="Side">The margin they go in.</param>
/// <param name="Width">A balloon's preferred width: points, or a percent of the page's width (<paramref name="WidthIsPercent"/>).</param>
/// <param name="WidthIsPercent"><paramref name="Width"/> is a percent (0 to 100) of the page's width.</param>
/// <param name="ConnectingLines">Leader lines join each balloon to its text.</param>
/// <param name="CommentColor">The colour comment cards take: null for their author's.</param>
public sealed record BalloonOptions(BalloonSide Side = BalloonSide.Right, double Width = 266.4, bool WidthIsPercent = false, bool ConnectingLines = true, ColorValue? CommentColor = null)
{
    /// <summary>Word's: on the right, 3.7 in wide, with connecting lines, comments in their author's colour.</summary>
    public static BalloonOptions Default { get; } = new();

    /// <summary>These options with the width kept to 0 and up, and a percent to 0 to 100.</summary>
    /// <returns>The options, in range.</returns>
    public BalloonOptions Clamped() => this with { Width = WidthIsPercent ? Math.Clamp(double.IsNaN(Width) ? 0 : Width, 0, 100) : Math.Max(0, double.IsNaN(Width) ? 0 : Width) };
}

/// <summary>The margin balloons go in.</summary>
public enum BalloonSide
{
    /// <summary>The right margin.</summary>
    Right,
    /// <summary>The left margin.</summary>
    Left,
}
