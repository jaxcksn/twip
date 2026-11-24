//! Style and list definitions: what a style, the document's defaults or a list define, read back and changed without
//! a dialog.

use serde::{Deserialize, Serialize};

use super::command::{Border, CellMargins, Command, ListLevel};
use super::services::{StyleVisibility, TabStopInfo, present};
use super::state::{CharacterFormatting, ParagraphFormatting};
use super::value::{CellAlignment, Color, HorizontalAlignment, Points, StyleId};

/// A style's type as Word's style lists show it: a paragraph style linked to a character style is one linked style.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StyleCategory {
    /// A paragraph style.
    #[default]
    Paragraph,
    /// A paragraph style with a character style linked to it: applied to a whole paragraph or to text.
    Linked,
    /// A character style.
    Character,
    /// A table style.
    Table,
    /// A list style.
    List,
}

/// The parts of a table a table style formats.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TableStylePart {
    /// The whole table.
    #[default]
    WholeTable,
    /// The header row.
    HeaderRow,
    /// The total row.
    TotalRow,
    /// The first column.
    FirstColumn,
    /// The last column.
    LastColumn,
    /// Odd banded rows.
    OddBandedRows,
    /// Even banded rows.
    EvenBandedRows,
    /// Odd banded columns.
    OddBandedColumns,
    /// Even banded columns.
    EvenBandedColumns,
    /// The top left cell.
    TopLeftCell,
    /// The top right cell.
    TopRightCell,
    /// The bottom left cell.
    BottomLeftCell,
    /// The bottom right cell.
    BottomRightCell,
}

impl TableStylePart {
    /// The `w:tblStylePr/@w:type` it is, or none for the whole table.
    pub fn region(self) -> Option<&'static str> {
        Some(match self {
            TableStylePart::WholeTable => return None,
            TableStylePart::HeaderRow => "firstRow",
            TableStylePart::TotalRow => "lastRow",
            TableStylePart::FirstColumn => "firstCol",
            TableStylePart::LastColumn => "lastCol",
            TableStylePart::OddBandedRows => "band1Horz",
            TableStylePart::EvenBandedRows => "band2Horz",
            TableStylePart::OddBandedColumns => "band1Vert",
            TableStylePart::EvenBandedColumns => "band2Vert",
            TableStylePart::TopLeftCell => "nwCell",
            TableStylePart::TopRightCell => "neCell",
            TableStylePart::BottomLeftCell => "swCell",
            TableStylePart::BottomRightCell => "seCell",
        })
    }
}

/// The list a paragraph style numbers its paragraphs with.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StyleList {
    /// The list: a [`ListDefinition::id`].
    pub list: i64,
    /// The level, 0 to 8.
    pub level: u8,
}

/// Formatting for a part of a table style.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TablePartFormatting {
    /// The part.
    pub part: TableStylePart,
    /// The part's own formatting goes first, so the commands give all of it.
    #[serde(default)]
    pub clear: bool,
    /// Character and paragraph formatting commands, and table ones: `setCellShading`, `setShading` (cells or the
    /// table), `setCellBorders`, `setCellAlignment`, `setCellMargins`, `setRowCanBreak`, and for the whole table
    /// `setTableAlignment` and `setTableIndent`.
    #[serde(default)]
    pub formatting: Vec<Command>,
}

/// A style change: what isn't given is kept, and `null` removes.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StyleChange {
    /// The name (not empty, not another style's or a built-in style's). A built-in style keeps its own: the new one
    /// becomes an alias.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The style it is based on (of its own type, not itself or one based on it), or none.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "present")]
    pub based_on: Option<Option<StyleId>>,
    /// The paragraph style Enter starts after a paragraph in it; none for itself. Paragraph styles only.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "present")]
    pub next: Option<Option<StyleId>>,
    /// The style of the other type it is linked to (a character style for a paragraph style, and the other way), or
    /// none. The other style links back; what either was linked to before no longer is.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "present")]
    pub link: Option<Option<StyleId>>,
    /// Formatting a paragraph in it directly changes the style instead. Paragraph styles only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_update: Option<bool>,
    /// Shown in the Styles gallery.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quick: Option<bool>,
    /// The list its paragraphs are numbered with, or none. Paragraph styles only; changing it renumbers every
    /// paragraph in the style.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "present")]
    pub list: Option<Option<StyleList>>,
    /// A list style's levels, from the first (the rest defaulted). List styles only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub list_levels: Option<Vec<ListLevel>>,
    /// Its own character and paragraph formatting goes first (a table style's table, row and cell properties too), so
    /// `formatting` gives all of it.
    #[serde(default)]
    pub clear: bool,
    /// Character and paragraph formatting commands, run on the style as on text (a character style takes character
    /// formatting only; a list style none). A table style's whole table takes table commands too, as
    /// [`TablePartFormatting::formatting`] lists them.
    #[serde(default)]
    pub formatting: Vec<Command>,
    /// A table style's parts' formatting.
    #[serde(default)]
    pub parts: Vec<TablePartFormatting>,
    /// A table style's rows in each band (1…32767).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub row_band: Option<u32>,
    /// A table style's columns in each band (1…32767).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column_band: Option<u32>,
}

/// `createStyle`'s options.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NewStyle {
    /// Its type.
    pub category: StyleCategory,
    /// It starts from the selection's formatting (its paragraph's and its first run's direct formatting), based on its
    /// paragraph's style, as Word's New Style from Formatting does.
    #[serde(default)]
    pub from_selection: bool,
    /// It is applied once made: a paragraph or linked style to the selected paragraphs, a character style to the
    /// selected text, a table style to the selection's table, a list style to the selected paragraphs.
    #[serde(default)]
    pub apply: bool,
    /// Its name (none: the first free "Style N"), base, following style and formatting.
    #[serde(default)]
    pub change: StyleChange,
}

/// `modifyStyle`'s options.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StyleModification {
    /// The style: the document's, or a built-in one, which joins the document.
    pub id: StyleId,
    /// The change.
    pub change: StyleChange,
}

/// A table style's borders.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TableBorders {
    /// The top edge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top: Option<Border>,
    /// The left edge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left: Option<Border>,
    /// The bottom edge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bottom: Option<Border>,
    /// The right edge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub right: Option<Border>,
    /// Between rows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inside_horizontal: Option<Border>,
    /// Between columns.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inside_vertical: Option<Border>,
    /// Each cell's top left to bottom right diagonal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagonal_down: Option<Border>,
    /// Each cell's top right to bottom left diagonal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagonal_up: Option<Border>,
}

/// A table style's part as its cells show it: the whole table's formatting with the part's over it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TablePartDefinition {
    /// The part.
    pub part: TableStylePart,
    /// The style or a style it is based on defines the part.
    pub defined: bool,
    /// Text's character formatting.
    pub character: CharacterFormatting,
    /// Text's paragraph formatting.
    pub paragraph: ParagraphFormatting,
    /// The cells' fill.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill: Option<Color>,
    /// The cells' vertical alignment.
    pub cell_alignment: CellAlignment,
    /// The borders: the table's for the whole table, the cells' for a part.
    pub borders: TableBorders,
    /// The cell margins: the table's default ones for the whole table.
    pub cell_margins: CellMargins,
    /// Rows may break across pages.
    pub row_can_break: bool,
}

/// What a table style holds besides its text's formatting.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TableStyleDefinition {
    /// The table's alignment.
    pub alignment: HorizontalAlignment,
    /// The table's indent.
    pub indent: Points,
    /// The table's own fill, behind its cells.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill: Option<Color>,
    /// Rows in each band.
    pub row_band: u32,
    /// Columns in each band.
    pub column_band: u32,
    /// Every part, the whole table first (13).
    pub parts: Vec<TablePartDefinition>,
}

/// A style as `styleDefinition` reads it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StyleDefinition {
    /// Its id.
    pub id: StyleId,
    /// Its name as style lists show it.
    pub name: String,
    /// Its other names.
    pub aliases: Vec<String>,
    /// Its type.
    pub category: StyleCategory,
    /// Built into Word (not one the document made).
    pub built_in: bool,
    /// The document defines it; a built-in style it doesn't is added when first used.
    pub defined: bool,
    /// The style it is based on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub based_on: Option<StyleId>,
    /// The style of the paragraph after one in it (none: itself).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next: Option<StyleId>,
    /// The style it is linked to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<StyleId>,
    /// Formatting its paragraphs directly changes it.
    pub auto_update: bool,
    /// Shown in the Styles gallery.
    pub quick: bool,
    /// Whether style lists show it.
    pub visibility: StyleVisibility,
    /// Its sort priority.
    pub priority: u32,
    /// Restricted while formatting is protected.
    pub locked: bool,
    /// The list its paragraphs are numbered with.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub list: Option<StyleList>,
    /// Text in it, resolved through its base and the document defaults: a paragraph in it (for a character style, a
    /// run in it in a default paragraph); a table style's whole table.
    pub character: CharacterFormatting,
    /// Text in it's paragraph formatting.
    pub paragraph: ParagraphFormatting,
    /// Text in it's tab stops.
    pub tabs: Vec<TabStopInfo>,
    /// What it sets itself: `character` where it does, `null` where it inherits.
    pub own_character: CharacterFormatting,
    /// What it sets itself: `paragraph` where it does, `null` where it inherits.
    pub own_paragraph: ParagraphFormatting,
    /// Word's description of it: its formatting over its base's.
    pub description: String,
    /// A table style's table formatting and parts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table: Option<TableStyleDefinition>,
    /// A list style's levels, in full.
    pub list_levels: Vec<ListLevel>,
}

/// The document's default formatting as `documentDefaults` reads it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DefaultsDefinition {
    /// Text's character formatting before any style's.
    pub character: CharacterFormatting,
    /// Text's paragraph formatting before any style's.
    pub paragraph: ParagraphFormatting,
    /// The default tab stops of every paragraph.
    pub tabs: Vec<TabStopInfo>,
    /// What the defaults set: `character` where they do, `null` where the application's apply.
    pub own_character: CharacterFormatting,
    /// What the defaults set: `paragraph` where they do, `null` where the application's apply.
    pub own_paragraph: ParagraphFormatting,
}

/// A built-in style, defined by the document or not, with the flags style lists use.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BuiltInStyle {
    /// Its id.
    pub id: StyleId,
    /// Its name as style lists show it.
    pub name: String,
    /// Its type.
    pub category: StyleCategory,
    /// The document defines it.
    pub defined: bool,
    /// Its sort priority.
    pub priority: u32,
    /// Whether style lists show it.
    pub visibility: StyleVisibility,
    /// Shown in the Styles gallery.
    pub quick: bool,
    /// Restricted while formatting is protected.
    pub locked: bool,
}

/// A list as `listDefinition` reads it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListDefinition {
    /// The list instance. Paragraphs number together when their lists share a definition, whatever their ids.
    pub id: i64,
    /// Its name, which ListNum fields name it by.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The list style it is defined by.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<StyleId>,
    /// Whether the current level is a bullet.
    pub bullet: bool,
    /// The current level, 0 to 8.
    pub level: u8,
    /// Its nine levels, in full.
    pub levels: Vec<ListLevel>,
}
