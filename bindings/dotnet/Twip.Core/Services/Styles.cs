using System.Text.Json.Serialization;
using Twip.Wire;

namespace Twip;

/// <summary>A style's type as Word's style lists show it: a paragraph style linked to a character style is one linked style.</summary>
public enum StyleCategory
{
    /// <summary>A paragraph style.</summary>
    Paragraph,
    /// <summary>A paragraph style with a character style linked to it: applied to a whole paragraph or to text.</summary>
    Linked,
    /// <summary>A character style.</summary>
    Character,
    /// <summary>A table style.</summary>
    Table,
    /// <summary>A list style.</summary>
    List,
}

/// <summary>The list a paragraph style numbers its paragraphs with.</summary>
/// <param name="List">The list: a <see cref="ListDefinitionInfo.Id"/>.</param>
/// <param name="Level">The level, 0 to 8.</param>
public sealed record StyleList(long List, int Level);

/// <summary>Formatting for a part of a table style.</summary>
/// <param name="Part">The part.</param>
public sealed record TablePartFormatting(TableStylePart Part)
{
    /// <summary>The part's own formatting goes first, so <see cref="Formatting"/> gives all of it.</summary>
    public bool Clear { get; init; }

    /// <summary>Character, paragraph and table formatting operations (no table alignment or indent in a part).</summary>
    public IReadOnlyList<EditorOperation> Formatting { get; init; } = [];
}

/// <summary>
/// A style change: what is null is kept. <c>NoBase</c>, <c>NextIsItself</c>, <c>Unlink</c> and <c>NoList</c> remove
/// a base, following style, link or list (ignored when the matching value is set).
/// </summary>
/// <param name="Name">The name (not empty, not another style's or a built-in style's). A built-in style keeps its own: the new one becomes an alias.</param>
/// <param name="BasedOn">The style it is based on, of its own type.</param>
/// <param name="NoBase">It is based on no style.</param>
/// <param name="Next">The paragraph style Enter starts after a paragraph in it. Paragraph styles only.</param>
/// <param name="NextIsItself">Enter starts a paragraph in the style itself.</param>
/// <param name="Link">The style of the other type it is linked to; the other style links back.</param>
/// <param name="Unlink">It is linked to no style.</param>
/// <param name="AutoUpdate">Formatting a paragraph in it directly changes the style. Paragraph styles only.</param>
/// <param name="Quick">Shown in the Styles gallery.</param>
/// <param name="List">The list its paragraphs are numbered with. Paragraph styles only.</param>
/// <param name="NoList">Its paragraphs are numbered with no list.</param>
/// <param name="ListLevels">A list style's levels, from the first.</param>
/// <param name="Clear">Its own character and paragraph formatting goes first (a table style's table, row and cell properties too), so <paramref name="Formatting"/> gives all of it.</param>
/// <param name="Formatting">Character and paragraph formatting operations, run on the style as on text; a table style's whole table takes table ones too.</param>
/// <param name="Parts">A table style's parts' formatting.</param>
/// <param name="RowBand">A table style's rows in each band, 1 to 32767.</param>
/// <param name="ColumnBand">A table style's columns in each band, 1 to 32767.</param>
[JsonConverter(typeof(StyleChangeConverter))]
public sealed record StyleChange(
    string? Name = null,
    StyleId? BasedOn = null,
    bool NoBase = false,
    StyleId? Next = null,
    bool NextIsItself = false,
    StyleId? Link = null,
    bool Unlink = false,
    bool? AutoUpdate = null,
    bool? Quick = null,
    StyleList? List = null,
    bool NoList = false,
    IReadOnlyList<ListLevel>? ListLevels = null,
    bool Clear = false,
    IReadOnlyList<EditorOperation>? Formatting = null,
    IReadOnlyList<TablePartFormatting>? Parts = null,
    int? RowBand = null,
    int? ColumnBand = null);

/// <summary>A new style for <see cref="EditorCommands.CreateStyleAsync"/>.</summary>
/// <param name="Category">Its type.</param>
/// <param name="Change">Its name (none: the first free "Style N"), base, following style and formatting.</param>
/// <param name="FromSelection">It starts from the selection's formatting, based on its paragraph's style (New Style from Formatting).</param>
/// <param name="Apply">It is applied once made: to the selected paragraphs, text or table.</param>
public sealed record NewStyle(StyleCategory Category, StyleChange Change, bool FromSelection = false, bool Apply = false);

/// <summary>A table style's borders.</summary>
public sealed record TableBorders
{
    /// <summary>The top edge.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public CellBorder? Top { get; init; }

    /// <summary>The left edge.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public CellBorder? Left { get; init; }

    /// <summary>The bottom edge.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public CellBorder? Bottom { get; init; }

    /// <summary>The right edge.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public CellBorder? Right { get; init; }

    /// <summary>Between rows.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public CellBorder? InsideHorizontal { get; init; }

    /// <summary>Between columns.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public CellBorder? InsideVertical { get; init; }

    /// <summary>Each cell's top left to bottom right diagonal.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public CellBorder? DiagonalDown { get; init; }

    /// <summary>Each cell's top right to bottom left diagonal.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public CellBorder? DiagonalUp { get; init; }
}

/// <summary>A table style's part as its cells show it: the whole table's formatting with the part's over it.</summary>
public sealed record TablePartDefinition
{
    /// <summary>The part.</summary>
    public TableStylePart Part { get; init; }

    /// <summary>The style or a style it is based on defines the part.</summary>
    public bool Defined { get; init; }

    /// <summary>Text's character formatting.</summary>
    public CharacterFormatting Character { get; init; } = new();

    /// <summary>Text's paragraph formatting.</summary>
    public ParagraphFormatting Paragraph { get; init; } = new();

    /// <summary>The cells' fill.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public ColorValue? Fill { get; init; }

    /// <summary>The cells' vertical alignment.</summary>
    public CellAlignment CellAlignment { get; init; }

    /// <summary>The borders: the table's for the whole table, the cells' for a part.</summary>
    public TableBorders Borders { get; init; } = new();

    /// <summary>The cell margins: the table's default ones for the whole table.</summary>
    public CellMarginOptions CellMargins { get; init; } = new();

    /// <summary>Rows may break across pages.</summary>
    public bool RowCanBreak { get; init; }
}

/// <summary>What a table style holds besides its text's formatting.</summary>
public sealed record TableStyleDefinition
{
    /// <summary>The table's alignment.</summary>
    public TableAlignment Alignment { get; init; }

    /// <summary>The table's indent.</summary>
    public Length Indent { get; init; }

    /// <summary>The table's own fill, behind its cells.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public ColorValue? Fill { get; init; }

    /// <summary>Rows in each band.</summary>
    public int RowBand { get; init; }

    /// <summary>Columns in each band.</summary>
    public int ColumnBand { get; init; }

    /// <summary>Every part, the whole table first.</summary>
    public IReadOnlyList<TablePartDefinition> Parts { get; init; } = [];
}

/// <summary>A style as <see cref="EditorStyles.GetDefinitionAsync"/> reads it.</summary>
public sealed record StyleDefinition
{
    /// <summary>Its id.</summary>
    public StyleId Id { get; init; }

    /// <summary>Its name as style lists show it.</summary>
    public string Name { get; init; } = "";

    /// <summary>Its other names.</summary>
    public IReadOnlyList<string> Aliases { get; init; } = [];

    /// <summary>Its type.</summary>
    public StyleCategory Category { get; init; }

    /// <summary>Built into Word.</summary>
    public bool BuiltIn { get; init; }

    /// <summary>The document defines it; a built-in style it doesn't is added when first used.</summary>
    public bool Defined { get; init; }

    /// <summary>The style it is based on.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public StyleId? BasedOn { get; init; }

    /// <summary>The style of the paragraph after one in it (null: itself).</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public StyleId? Next { get; init; }

    /// <summary>The style it is linked to.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public StyleId? Link { get; init; }

    /// <summary>Formatting its paragraphs directly changes it.</summary>
    public bool AutoUpdate { get; init; }

    /// <summary>Shown in the Styles gallery.</summary>
    public bool Quick { get; init; }

    /// <summary>Whether style lists show it.</summary>
    public StyleVisibility Visibility { get; init; }

    /// <summary>Its sort priority.</summary>
    public int Priority { get; init; }

    /// <summary>Restricted while formatting is protected.</summary>
    public bool Locked { get; init; }

    /// <summary>The list its paragraphs are numbered with.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public StyleList? List { get; init; }

    /// <summary>Text in it, through its base and the document defaults (a table style's whole table).</summary>
    public CharacterFormatting Character { get; init; } = new();

    /// <summary>Text in it's paragraph formatting.</summary>
    public ParagraphFormatting Paragraph { get; init; } = new();

    /// <summary>Text in it's tab stops.</summary>
    public IReadOnlyList<TabStop> Tabs { get; init; } = [];

    /// <summary>What it sets itself: <see cref="Character"/> where it does, unavailable where it inherits.</summary>
    public CharacterFormatting OwnCharacter { get; init; } = new();

    /// <summary>What it sets itself: <see cref="Paragraph"/> where it does, unavailable where it inherits.</summary>
    public ParagraphFormatting OwnParagraph { get; init; } = new();

    /// <summary>Word's description of it: its formatting over its base's.</summary>
    public string Description { get; init; } = "";

    /// <summary>A table style's table formatting and parts.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public TableStyleDefinition? Table { get; init; }

    /// <summary>A list style's levels, in full.</summary>
    public IReadOnlyList<ListLevel> ListLevels { get; init; } = [];
}

/// <summary>The document's default formatting as <see cref="EditorStyles.GetDocumentDefaultsAsync"/> reads it.</summary>
public sealed record DefaultsDefinition
{
    /// <summary>Text's character formatting before any style's.</summary>
    public CharacterFormatting Character { get; init; } = new();

    /// <summary>Text's paragraph formatting before any style's.</summary>
    public ParagraphFormatting Paragraph { get; init; } = new();

    /// <summary>The default tab stops of every paragraph.</summary>
    public IReadOnlyList<TabStop> Tabs { get; init; } = [];

    /// <summary>What the defaults set: <see cref="Character"/> where they do, unavailable where the application's apply.</summary>
    public CharacterFormatting OwnCharacter { get; init; } = new();

    /// <summary>What the defaults set: <see cref="Paragraph"/> where they do, unavailable where the application's apply.</summary>
    public ParagraphFormatting OwnParagraph { get; init; } = new();
}

/// <summary>A built-in style, defined by the document or not, with the flags style lists use.</summary>
/// <param name="Id">Its id.</param>
/// <param name="Name">Its name as style lists show it.</param>
/// <param name="Category">Its type.</param>
/// <param name="Defined">The document defines it.</param>
/// <param name="Priority">Its sort priority.</param>
/// <param name="Visibility">Whether style lists show it.</param>
/// <param name="Quick">Shown in the Styles gallery.</param>
/// <param name="Locked">Restricted while formatting is protected.</param>
public sealed record BuiltInStyle(StyleId Id, string Name, StyleCategory Category, bool Defined, int Priority, StyleVisibility Visibility, bool Quick, bool Locked);

/// <summary>A list as <see cref="EditorLists.GetDefinitionAsync"/> reads it.</summary>
/// <param name="Id">The list instance. Paragraphs number together when their lists share a definition, whatever their ids.</param>
/// <param name="Bullet">Whether the current level is a bullet.</param>
/// <param name="Level">The current level, 0 to 8.</param>
/// <param name="Levels">Its nine levels, in full.</param>
/// <param name="Name">Its name, which ListNum fields name it by.</param>
/// <param name="Style">The list style it is defined by.</param>
public sealed record ListDefinitionInfo(
    long Id,
    bool Bullet,
    int Level,
    IReadOnlyList<ListLevel> Levels,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? Name = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] StyleId? Style = null);

/// <summary>The undo and redo lists (the Undo and Redo drop-downs).</summary>
/// <param name="Undo">The steps undo would undo, most recent first.</param>
/// <param name="Redo">The steps redo would redo, next first.</param>
/// <param name="MoreUndo">There are more undo steps than listed.</param>
/// <param name="MoreRedo">There are more redo steps than listed.</param>
public sealed record HistoryLabels(IReadOnlyList<string> Undo, IReadOnlyList<string> Redo, bool MoreUndo, bool MoreRedo);

/// <summary>What <see cref="EditorSession.GetWordCountAsync"/> counts.</summary>
/// <param name="Selection">The selected text's (the document's when nothing is selected).</param>
/// <param name="IncludeTextboxesAndNotes">Text boxes, footnotes and endnotes count as well as the body.</param>
public sealed record WordCountOptions(bool Selection = false, bool IncludeTextboxesAndNotes = true);

/// <summary>One cell of the theme colour grid.</summary>
/// <param name="Colour">The colour as commands take it: the theme colour with its tint or shade.</param>
/// <param name="Rgb">What it shows.</param>
/// <param name="Label">Its name: "Accent 1", "Accent 1, Lighter 40%".</param>
public sealed record ThemeSwatch(ColorValue Colour, ColorValue Rgb, string Label);

/// <summary>A column of the theme colour grid: the theme colour, then its five lighter and darker variants.</summary>
/// <param name="Theme">The theme colour.</param>
/// <param name="Swatches">The theme colour itself, then its variants, top to bottom.</param>
public sealed record ThemeColumn(ThemeColor Theme, IReadOnlyList<ThemeSwatch> Swatches);

/// <summary>A table's size.</summary>
/// <param name="Rows">The number of rows.</param>
/// <param name="Columns">The number of columns.</param>
public sealed record TableSize(int Rows, int Columns);

internal sealed record TextToTableRequest(
    TextSeparator Separator,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? Character,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] int? Columns);

internal sealed record HistoryLabelsRequest([property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] int? Limit);
