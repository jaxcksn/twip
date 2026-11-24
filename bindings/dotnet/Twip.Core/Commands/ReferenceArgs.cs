using System.Text.Json.Serialization;
using Twip.Wire;

namespace Twip;

/// <summary>What Go To goes to.</summary>
public enum GoToWhat
{
    /// <summary>A page's first line.</summary>
    Page,
    /// <summary>A section's first paragraph.</summary>
    Section,
    /// <summary>A line of the body.</summary>
    Line,
    /// <summary>A bookmark (selected), by name or in order.</summary>
    Bookmark,
    /// <summary>A comment thread (its text selected), by its reviewer when named.</summary>
    Comment,
    /// <summary>A footnote's reference.</summary>
    Footnote,
    /// <summary>An endnote's reference.</summary>
    Endnote,
    /// <summary>A field (selected), of a kind when named: its code's first word.</summary>
    Field,
    /// <summary>A table's first cell.</summary>
    Table,
    /// <summary>An inline picture or a floating object (selected).</summary>
    Graphic,
    /// <summary>An equation.</summary>
    Equation,
    /// <summary>An embedded object (selected), of a kind when named: its program id or the start of one.</summary>
    Object,
    /// <summary>A paragraph with an outline level.</summary>
    Heading,
}

/// <summary>Which one Go To goes to: the n-th, n on or back, the next or the previous.</summary>
[JsonConverter(typeof(GoToTargetConverter))]
public sealed record GoToTarget
{
    private GoToTarget(int? absolute, int? relative, NavigationDirection? step) => (Absolute, Relative, Step) = (absolute, relative, step);

    /// <summary>The n-th (from 1), for <see cref="Nth"/>.</summary>
    public int? Absolute { get; }

    /// <summary>How many on (back, negative), for <see cref="By"/>.</summary>
    public int? Relative { get; }

    /// <summary>The next or the previous.</summary>
    public NavigationDirection? Step { get; }

    /// <summary>The n-th, from 1; numbers past the last go to the last.</summary>
    /// <param name="n">From 1.</param>
    /// <returns>The target.</returns>
    public static GoToTarget Nth(int n) => new(n, null, null);

    /// <summary>n on from the current one (back, negative); not 0.</summary>
    /// <param name="n">How many.</param>
    /// <returns>The target.</returns>
    public static GoToTarget By(int n) => new(null, n, null);

    /// <summary>The next one.</summary>
    public static GoToTarget Next { get; } = new(null, null, NavigationDirection.Next);

    /// <summary>The previous one.</summary>
    public static GoToTarget Previous { get; } = new(null, null, NavigationDirection.Previous);
}

/// <summary>A range of heading or entry levels, 1 to 9.</summary>
/// <param name="From">The first level.</param>
/// <param name="To">The last level, <paramref name="From"/> or more.</param>
public sealed record TocLevels(int From, int To);

/// <summary>A style whose paragraphs a table of contents collects, as entries of a level.</summary>
/// <param name="Style">The style.</param>
/// <param name="Level">The entries' level, 1 to 9.</param>
public sealed record TocStyleLevel(StyleId Style, int Level);

/// <summary>The table entry fields a table of contents collects.</summary>
/// <param name="Identifier">Those of this identifier; null for <c>C</c> (Mark Entry's).</param>
/// <param name="Levels">Those of these levels; null for every level.</param>
public sealed record TocEntryFields(
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? Identifier = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] TocLevels? Levels = null);

/// <summary>The captions a table of figures collects.</summary>
/// <param name="Label">Their label.</param>
/// <param name="IncludeLabelAndNumber">Entries keep the label and number, or have the caption's text alone.</param>
public sealed record TocCaptions(string Label, bool IncludeLabelAndNumber);

/// <summary>Which entries of a table of contents show a page number.</summary>
[JsonConverter(typeof(TocPageNumbersConverter))]
public sealed record TocPageNumbers
{
    private TocPageNumbers(bool none, TocLevels? omit) => (IsNone, OmitLevels) = (none, omit);

    /// <summary>No entry shows one.</summary>
    public bool IsNone { get; }

    /// <summary>The levels whose entries show none, for <see cref="Omit"/>.</summary>
    public TocLevels? OmitLevels { get; }

    /// <summary>Every entry.</summary>
    public static TocPageNumbers All { get; } = new(false, null);

    /// <summary>No entry.</summary>
    public static TocPageNumbers None { get; } = new(true, null);

    /// <summary>Every entry but those of these levels.</summary>
    /// <param name="levels">The levels.</param>
    /// <returns>The choice.</returns>
    public static TocPageNumbers Omit(TocLevels levels) => new(false, levels);
}

/// <summary>Page numbers led by a chapter number.</summary>
/// <param name="Sequence">The sequence whose number leads.</param>
/// <param name="Separator">What goes between it and the page number; null for <c>-</c>.</param>
public sealed record TocChapterNumbers(string Sequence, [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? Separator = null);

/// <summary>What a table of contents collects and how its entries read; each one left null is off. All null is Word's default table.</summary>
public sealed record TableOfContentsOptions
{
    /// <summary>Paragraphs in the built-in heading styles of these levels.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public TocLevels? HeadingLevels { get; init; }

    /// <summary>Paragraphs with an outline level, as their level (within <see cref="HeadingLevels"/> when set).</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public bool? OutlineLevels { get; init; }

    /// <summary>Paragraphs in these styles, at these levels. A style the document doesn't define reads back left out.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public IReadOnlyList<TocStyleLevel>? StyleLevels { get; init; }

    /// <summary>Table entry fields (Mark Entry's).</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public TocEntryFields? EntryFields { get; init; }

    /// <summary>Captions of a label: a table of figures.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public TocCaptions? Captions { get; init; }

    /// <summary>Only what this bookmark marks.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public string? Bookmark { get; init; }

    /// <summary>Entries link to their source.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public bool? Hyperlinks { get; init; }

    /// <summary>Which entries show a page number; null for all.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public TocPageNumbers? PageNumbers { get; init; }

    /// <summary>What goes between an entry and its page number in place of a tab.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public string? Separator { get; init; }

    /// <summary>Page numbers led by a chapter number.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public TocChapterNumbers? ChapterNumbers { get; init; }

    /// <summary>Tab leaders and page numbers hidden in a web view.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public bool? HideInWebView { get; init; }

    /// <summary>Tabs in entries kept (else spaces).</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public bool? KeepTabs { get; init; }

    /// <summary>Line breaks in entries kept (else spaces).</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public bool? KeepLineBreaks { get; init; }
}

/// <summary>How a new table's entries are laid out.</summary>
/// <param name="RightAlignPageNumbers">Page numbers at a right tab stop at the right margin; null for on.</param>
/// <param name="TabLeader">That tab stop's leader; null for dots.</param>
public sealed record TocLayout(
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? RightAlignPageNumbers = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] TabLeader? TabLeader = null);

/// <summary>A new table's title and building block control (Word's Automatic Table 1: title "Contents", a building block).</summary>
/// <param name="Title">A title paragraph before it, in the TOC Heading style.</param>
/// <param name="BuildingBlock">A building block content control around the title and the table.</param>
public sealed record TocFrame(
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? Title = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? BuildingBlock = null);

/// <summary>What updating a table of contents rewrites.</summary>
public enum TocUpdateMode
{
    /// <summary>The page numbers only: edits of the entries stay.</summary>
    PageNumbers,
    /// <summary>Every entry, collected again.</summary>
    EntireTable,
}

/// <summary>Where a caption goes beside its item.</summary>
public enum CaptionPosition
{
    /// <summary>Above it.</summary>
    Above,
    /// <summary>Below it.</summary>
    Below,
}

/// <summary>A chapter number before a caption's number: the number of the last heading of a level before it.</summary>
/// <param name="Level">The heading level, 1 to 9.</param>
/// <param name="Separator">What goes between it and the caption's number.</param>
public sealed record CaptionChapter(int Level, ChapterSeparator Separator);

/// <summary>The caption every inserted table gets (AutoCaption).</summary>
/// <param name="Label">The label: a letter, then letters, digits, spaces and underscores.</param>
/// <param name="Position">Above or below the table; null for above.</param>
/// <param name="ExcludeLabel">The number without the label before it.</param>
/// <param name="Format">The number's format: decimal (null), letters or roman numerals.</param>
/// <param name="Chapter">A chapter number before it.</param>
public sealed record AutoCaption(
    string Label,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] CaptionPosition? Position = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? ExcludeLabel = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] NumberFormat? Format = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] CaptionChapter? Chapter = null);

/// <summary>What a cross-reference can point to.</summary>
[JsonConverter(typeof(CrossReferenceKindConverter))]
public sealed record CrossReferenceKind
{
    private CrossReferenceKind(string name, string? label) => (Name, CaptionLabel) = (name, label);

    internal string Name { get; }

    /// <summary>The caption label, for <see cref="Caption"/>; null for the other kinds.</summary>
    public string? CaptionLabel { get; }

    /// <summary>Numbered list paragraphs (not bullets).</summary>
    public static CrossReferenceKind NumberedItem { get; } = new("numberedItem", null);

    /// <summary>Headings: paragraphs with an outline level.</summary>
    public static CrossReferenceKind Heading { get; } = new("heading", null);

    /// <summary>Bookmarks, hidden ones left out.</summary>
    public static CrossReferenceKind Bookmark { get; } = new("bookmark", null);

    /// <summary>Footnotes.</summary>
    public static CrossReferenceKind Footnote { get; } = new("footnote", null);

    /// <summary>Endnotes.</summary>
    public static CrossReferenceKind Endnote { get; } = new("endnote", null);

    /// <summary>Captions of a label.</summary>
    /// <param name="label">The label.</param>
    /// <returns>The kind.</returns>
    public static CrossReferenceKind Caption(string label) => new("caption", label);

    internal static CrossReferenceKind? Of(string name) => name switch
    {
        "numberedItem" => NumberedItem,
        "heading" => Heading,
        "bookmark" => Bookmark,
        "footnote" => Footnote,
        "endnote" => Endnote,
        _ => null,
    };
}

/// <summary>What a cross-reference shows of its target.</summary>
public enum CrossReferenceContent
{
    /// <summary>Its text: a heading's or numbered item's, a bookmark's, the whole caption.</summary>
    Text,
    /// <summary>The page it is on.</summary>
    PageNumber,
    /// <summary>Its paragraph number, relative to where the reference is.</summary>
    Number,
    /// <summary>Its paragraph number without the levels above.</summary>
    NumberNoContext,
    /// <summary>Its paragraph number with every level.</summary>
    NumberFullContext,
    /// <summary>"above" or "below".</summary>
    AboveBelow,
    /// <summary>A note's number.</summary>
    NoteNumber,
    /// <summary>A note's number formatted as its reference mark.</summary>
    NoteNumberFormatted,
    /// <summary>A caption's label and number.</summary>
    LabelAndNumber,
    /// <summary>A caption's text after its number.</summary>
    CaptionText,
}

/// <summary>A cross-reference's target: the item at <paramref name="Index"/> of what <see cref="EditorReferences.GetCrossReferenceTargetsAsync"/> lists for <paramref name="Kind"/>.</summary>
/// <param name="Kind">The kind listed.</param>
/// <param name="Index">The item's place in that list, from 0.</param>
public sealed record CrossReferenceTargetRef(CrossReferenceKind Kind, int Index);

/// <summary>An item a cross-reference can point to, as Word's Cross-reference dialog lists it.</summary>
/// <param name="Text">What the list shows: a numbered item's number and text, a heading's text, a bookmark's name, a note's text, a caption.</param>
/// <param name="Number">Its number: a list number, a heading's, a note's as laid out.</param>
/// <param name="Level">A heading's level.</param>
/// <param name="Bookmark">A bookmark's name.</param>
/// <param name="Note">A note's id, as the notes list gives it.</param>
public sealed record CrossReferenceTarget(
    string Text,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? Number = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] int? Level = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? Bookmark = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] long? Note = null);

/// <summary>A table of contents or of figures in the document.</summary>
/// <param name="Index">Its place among them, in document order: what an index names in the commands.</param>
/// <param name="Figures">It collects captions: a table of figures.</param>
/// <param name="Options">What it collects and how its entries read, from its code.</param>
/// <param name="Locked">Updating and removing it are refused: its field is locked, or a control around it locks its content.</param>
/// <param name="FirstParagraph">The paragraph its field starts in.</param>
/// <param name="LastParagraph">The paragraph its field ends in.</param>
public sealed record TableOfContentsInfo(int Index, bool Figures, TableOfContentsOptions Options, bool Locked, ParagraphId FirstParagraph, ParagraphId LastParagraph);

/// <summary>The document's tables of contents and of figures.</summary>
/// <param name="Tables">Each one, in document order.</param>
/// <param name="Here">The one the selection is in, by index; null when it is in none.</param>
public sealed record TablesOfContents(IReadOnlyList<TableOfContentsInfo> Tables, [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] int? Here = null);

/// <summary>An entry a table of contents would collect, for a preview.</summary>
/// <param name="Level">Its level, 1 to 9.</param>
/// <param name="Text">Its text.</param>
/// <param name="PageNumber">It shows a page number.</param>
public sealed record TocEntryPreview(int Level, string Text, bool PageNumber);

/// <summary>The smart cut and paste adjustments.</summary>
/// <param name="WordSpacing">Adjust sentence and word spacing.</param>
/// <param name="ParagraphSpacing">Adjust paragraph spacing on paste.</param>
/// <param name="Tables">Adjust table formatting and alignment: copied cells go into a table as the selection suggests.</param>
/// <param name="Styles">Smart style behavior.</param>
/// <param name="MergeLists">Merge pasted lists with surrounding lists.</param>
/// <param name="FromPowerPoint">Merge formatting when pasting from PowerPoint.</param>
/// <param name="FromExcel">Adjust formatting when pasting from Excel.</param>
public sealed record SmartPasteSettings(bool WordSpacing, bool ParagraphSpacing, bool Tables, bool Styles, bool MergeLists, bool FromPowerPoint, bool FromExcel);

/// <summary>The cut, copy and paste options: what a paste does when it isn't told, and what goes with it. Word's defaults until changed.</summary>
/// <param name="Within">Pasting within the same document.</param>
/// <param name="Between">Pasting between documents.</param>
/// <param name="Conflict">Pasting between documents when style definitions conflict.</param>
/// <param name="Other">Pasting from other programs.</param>
/// <param name="KeepBullets">Keep bullets and numbers when pasting with Keep Text Only.</param>
/// <param name="ShowButton">Show the Paste Options button when content is pasted.</param>
/// <param name="SmartCutPaste">Use smart cut and paste: the <paramref name="Smart"/> adjustments are made only while it is on.</param>
/// <param name="Smart">The smart cut and paste adjustments.</param>
public sealed record PasteSettings(PasteMode Within, PasteMode Between, PasteMode Conflict, PasteMode Other, bool KeepBullets, bool ShowButton, bool SmartCutPaste, SmartPasteSettings Smart);

/// <summary>The smart cut and paste adjustments that change; null keeps one.</summary>
/// <param name="WordSpacing">Adjust sentence and word spacing.</param>
/// <param name="ParagraphSpacing">Adjust paragraph spacing on paste.</param>
/// <param name="Tables">Adjust table formatting and alignment.</param>
/// <param name="Styles">Smart style behavior.</param>
/// <param name="MergeLists">Merge pasted lists with surrounding lists.</param>
/// <param name="FromPowerPoint">Merge formatting when pasting from PowerPoint.</param>
/// <param name="FromExcel">Adjust formatting when pasting from Excel.</param>
public sealed record SmartPasteChange(
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? WordSpacing = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? ParagraphSpacing = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? Tables = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? Styles = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? MergeLists = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? FromPowerPoint = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? FromExcel = null);

/// <summary>
/// The cut, copy and paste options that change; null keeps one. <paramref name="Within"/>, <paramref name="Between"/>
/// and <paramref name="Other"/> take keep source, merge or text only; <paramref name="Conflict"/> also destination styles.
/// </summary>
/// <param name="Within">Pasting within the same document.</param>
/// <param name="Between">Pasting between documents.</param>
/// <param name="Conflict">Pasting between documents when style definitions conflict.</param>
/// <param name="Other">Pasting from other programs.</param>
/// <param name="KeepBullets">Keep bullets and numbers when pasting with Keep Text Only.</param>
/// <param name="ShowButton">Show the Paste Options button.</param>
/// <param name="SmartCutPaste">Use smart cut and paste.</param>
/// <param name="Smart">The smart adjustments that change.</param>
public sealed record PasteOptionsChange(
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] PasteMode? Within = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] PasteMode? Between = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] PasteMode? Conflict = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] PasteMode? Other = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? KeepBullets = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? ShowButton = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? SmartCutPaste = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] SmartPasteChange? Smart = null);

/// <summary>What clipboard content can paste as at the selection: Paste Special's list.</summary>
/// <param name="Choices">The ways it can paste, in Word's order; empty where nothing can be pasted.</param>
/// <param name="Default">The one Ctrl+V takes; null when the clipboard holds nothing to paste.</param>
public sealed record PasteChoices(IReadOnlyList<PasteMode> Choices, [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] PasteMode? Default = null);

/// <summary>A picture on the clipboard.</summary>
/// <param name="Mime">Its media type as the clipboard names it (<c>image/png</c>).</param>
/// <param name="Data">Its bytes.</param>
public sealed record ClipboardImage(string Mime, ReadOnlyMemory<byte> Data);
