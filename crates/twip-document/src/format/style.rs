//! Authored style definitions, independent of any file format: each style with the formatting it sets at each level
//! (and a table style's conditional regions), the document defaults and the latent styles, as a styles part defines
//! them. Unset is `None`; theme references stay references.

use std::collections::HashMap;
use std::rc::Rc;

use super::{
    CellProps, CellSet, CharProps, DefinedIn, FontRef, ForeignProps, ParaProps, ParaSet, RowProps, RowSet, ScriptFonts, Set, TableProps, TableSet,
};
use crate::authored::{Authored, Empty};
use crate::format::LineRule;

/// What a style formats.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StyleKind {
    /// Paragraphs (and their text).
    #[default]
    Paragraph,
    /// Runs of text.
    Character,
    /// Tables.
    Table,
    /// Lists: it names a list definition.
    Numbering,
}

/// A table style's conditional region: the cells it formats.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RegionKind {
    /// The whole table.
    WholeTable,
    /// The header row.
    FirstRow,
    /// The last row.
    LastRow,
    /// The first column.
    FirstCol,
    /// The last column.
    LastCol,
    /// Odd column bands.
    Band1Vert,
    /// Even column bands.
    Band2Vert,
    /// Odd row bands.
    Band1Horz,
    /// Even row bands.
    Band2Horz,
    /// The top right cell.
    NeCell,
    /// The top left cell.
    NwCell,
    /// The bottom right cell.
    SeCell,
    /// The bottom left cell.
    SwCell,
}

impl RegionKind {
    /// Every region.
    pub const ALL: [RegionKind; 13] = {
        use RegionKind::*;
        [WholeTable, FirstRow, LastRow, FirstCol, LastCol, Band1Vert, Band2Vert, Band1Horz, Band2Horz, NeCell, NwCell, SeCell, SwCell]
    };
}

/// The formatting one level of a style sets: its paragraph and character properties, and a table style's table, row
/// and cell properties. Its `==` is authored equality.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StyleFormat {
    /// The paragraph properties; a numbering style's list is its `list`.
    pub para: ParaProps,
    /// The character properties.
    pub char: CharProps,
    /// The table properties.
    pub table: TableProps,
    /// The row properties.
    pub row: RowProps,
    /// The cell properties.
    pub cell: CellProps,
}

/// A table style's conditional formatting for one region.
#[derive(Clone, Debug, PartialEq)]
pub struct Region {
    /// The cells it formats.
    pub kind: RegionKind,
    /// What it sets.
    pub format: StyleFormat,
    /// What the source format wrote for the region that this struct does not model. Owned by the adapter that read
    /// it.
    pub foreign: Option<Rc<ForeignProps>>,
}

/// A style's flags for style lists and galleries, each set or not on its own.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StyleMeta {
    /// Hidden from the recommended lists.
    pub semi_hidden: Option<bool>,
    /// Hidden from every list.
    pub hidden: Option<bool>,
    /// Shown once used.
    pub unhide_when_used: Option<bool>,
    /// Can't be applied while formatting is locked.
    pub locked: Option<bool>,
    /// Takes a paragraph's direct formatting whenever one in it is formatted.
    pub auto_redefine: Option<bool>,
}

/// A style definition. Its `==` is authored equality.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Style {
    /// Its id, which paragraphs, runs, tables and other styles name it by.
    pub id: Rc<str>,
    /// What it formats.
    pub kind: StyleKind,
    /// Its name: the first of the names written.
    pub name: Option<Rc<str>>,
    /// The other names written with its name.
    pub aliases: Vec<Rc<str>>,
    /// The style it is based on.
    pub based_on: Option<Rc<str>>,
    /// The style of the paragraph Enter starts after one in this style.
    pub next: Option<Rc<str>>,
    /// The linked character or paragraph style.
    pub link: Option<Rc<str>>,
    /// The default style of its kind.
    pub default: Option<bool>,
    /// Not one of the application's built-in styles.
    pub custom: Option<bool>,
    /// Shown in the quick styles gallery.
    pub quick: Option<bool>,
    /// Where lists sort it: lower first.
    pub priority: Option<u32>,
    /// Its flags for lists and galleries.
    pub meta: StyleMeta,
    /// What it sets; a table style's base, under its regions.
    pub format: StyleFormat,
    /// A table style's conditional regions, one of each kind at most.
    pub regions: Vec<Region>,
    /// What the source format wrote for the style that this struct does not model. Owned by the adapter that read
    /// it.
    pub foreign: Option<Rc<ForeignProps>>,
}

impl Style {
    /// Its region of `kind`.
    pub fn region(&self, kind: RegionKind) -> Option<&Region> {
        self.regions.iter().find(|r| r.kind == kind)
    }

    /// A table style named as Word's built-in default one, which is that style whatever the part defines for it.
    pub fn word_default_table(&self) -> bool {
        self.kind == StyleKind::Table && matches!(self.name.as_deref(), Some("Normal Table" | "Table Normal"))
    }
}

/// The document defaults: the character and paragraph properties under every style.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Defaults {
    /// The character defaults; `None`: none written, Word's own ([`Defaults::word_char`]) apply.
    pub char: Option<CharProps>,
    /// The paragraph defaults; `None`: none written, Word's own ([`Defaults::word_para`]) apply.
    pub para: Option<ParaProps>,
    /// What the source format wrote for the defaults that this struct does not model. Owned by the adapter that read
    /// it.
    pub foreign: Option<Rc<ForeignProps>>,
}

impl Defaults {
    /// Word's character defaults where a document writes none: Aptos, 12 points, kerning from 1 point.
    pub fn word_char() -> CharProps {
        let aptos = Some(FontRef::Family("Aptos".into()));
        CharProps {
            fonts: ScriptFonts { latin: aptos.clone(), latin_extended: aptos.clone(), east_asian: aptos.clone(), complex: aptos },
            size: Some(12.0),
            complex_size: Some(12.0),
            kern: Some(1.0),
            ..CharProps::default()
        }
    }

    /// Word's paragraph defaults where a document writes none: 8 points after, 1.16 lines.
    pub fn word_para() -> ParaProps {
        ParaProps { space_after: Some(8.0), line: Some(278.0), line_rule: Some(LineRule::Auto), ..ParaProps::default() }
    }
}

/// A built-in style the document doesn't define, as the style lists show it: the latent styles' exception for it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LatentStyle {
    /// The built-in style's name.
    pub name: Rc<str>,
    /// Where lists sort it.
    pub priority: Option<u32>,
    /// Hidden from the recommended lists.
    pub semi_hidden: Option<bool>,
    /// Shown once used.
    pub unhide_when_used: Option<bool>,
    /// Shown in the quick styles gallery.
    pub quick: Option<bool>,
    /// Can't be applied while formatting is locked.
    pub locked: Option<bool>,
    /// What the source format wrote for it that this struct does not model. Owned by the adapter that read it.
    pub foreign: Option<Rc<ForeignProps>>,
}

/// How the style lists show the built-in styles the document doesn't define: defaults, and exceptions by name.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LatentStyles {
    /// The default sort priority.
    pub priority: Option<u32>,
    /// Hidden from the recommended lists by default.
    pub semi_hidden: Option<bool>,
    /// Shown once used, by default.
    pub unhide_when_used: Option<bool>,
    /// In the quick styles gallery by default.
    pub quick: Option<bool>,
    /// Locked by default.
    pub locked: Option<bool>,
    /// How many built-in styles the application that wrote them knew.
    pub count: Option<u32>,
    /// The exceptions, in order.
    pub exceptions: Vec<LatentStyle>,
    /// What the source format wrote for them that this struct does not model. Owned by the adapter that read it.
    pub foreign: Option<Rc<ForeignProps>>,
}

/// A style in a sheet, with the facts the sheet works out for it as a whole.
#[derive(Clone, Debug)]
pub struct StyleEntry {
    /// The definition.
    pub style: Rc<Style>,
    /// Its place in the styles part as read; `None` for one made since, which comes after them by id.
    pub order: Option<u32>,
    /// Which styles part it came from.
    pub origin: DefinedIn,
    /// A table style whose properties start from Word's defaults, not its base: it was worked out before its base
    /// was read and kept what it had then.
    pub frozen: bool,
    /// A table style whose run properties start from Word's built-in table defaults (10 points, automatic colour, no
    /// spacing, 100% scale): stored at a region while its base was missing.
    pub word_run_defaults: bool,
}

/// A styles part's definitions: the styles by id, the defaults and the latent styles, with what depends on the styles
/// together (default styles, the paragraph styles' order, and the table styles' read-order facts).
#[derive(Clone, Debug, Default)]
pub struct StyleSheet {
    /// The styles by id.
    pub styles: HashMap<Rc<str>, StyleEntry>,
    /// The paragraph styles, in the order read.
    pub paragraph_order: Vec<Rc<str>>,
    /// The default paragraph style: the last paragraph style marked default, else `Normal` if it is a paragraph
    /// style.
    pub default_paragraph: Option<Rc<str>>,
    /// The default table style: the first table style marked default.
    pub default_table: Option<Rc<str>>,
    /// The default character style: the first character style marked default.
    pub default_character: Option<Rc<str>>,
    /// The default numbering style: the first numbering style marked default.
    pub default_numbering: Option<Rc<str>>,
    /// The document defaults; `None`: none written.
    pub defaults: Option<Rc<Defaults>>,
    /// The latent styles; `None`: none written.
    pub latent: Option<Rc<LatentStyles>>,
}

impl StyleSheet {
    /// The style with id `id`.
    pub fn get(&self, id: &str) -> Option<&Rc<Style>> {
        self.styles.get(id).map(|e| &e.style)
    }

    /// The styles in their order: as read, then those made since by id.
    pub fn in_order(&self) -> Vec<&StyleEntry> {
        let mut all: Vec<&StyleEntry> = self.styles.values().collect();
        all.sort_unstable_by(|a, b| (a.order.is_none(), a.order, &a.style.id).cmp(&(b.order.is_none(), b.order, &b.style.id)));
        all
    }
}

/// A style property an edit sets, as a source format writes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StyleProp {
    /// The id.
    Id,
    /// The kind.
    Kind,
    /// The name and aliases.
    Name,
    /// The base style.
    BasedOn,
    /// The next paragraph's style.
    Next,
    /// The linked style.
    Link,
    /// Default of its kind.
    Default,
    /// Not built in.
    Custom,
    /// In the quick styles gallery.
    Quick,
    /// The sort priority.
    Priority,
    /// Semi-hidden.
    SemiHidden,
    /// Hidden.
    Hidden,
    /// Shown once used.
    UnhideWhenUsed,
    /// Locked.
    Locked,
    /// Redefined from direct formatting.
    AutoRedefine,
}

/// What an edit of a style set: one of its own properties, a property of its formatting at its base (`None`) or in a
/// region, or a region whole.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StyleSet {
    /// One of the style's own properties.
    Style(StyleProp),
    /// A paragraph property.
    Para(Option<RegionKind>, ParaSet),
    /// A character property.
    Char(Option<RegionKind>, Set),
    /// A table property.
    Table(Option<RegionKind>, TableSet),
    /// A row property.
    Row(Option<RegionKind>, RowSet),
    /// A cell property.
    Cell(Option<RegionKind>, CellSet),
    /// A region whole.
    Region(RegionKind),
}

/// What an edit of the document defaults set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DefaultsSet {
    /// A character property.
    Char(Set),
    /// A paragraph property.
    Para(ParaSet),
}

/// Whether a level's paragraph or run properties hold a child, as written: what it sets, or a child the typed
/// properties don't model.
pub fn sets_text(f: &StyleFormat) -> bool {
    let unmodelled = |x: Option<&ForeignProps>| x.is_some_and(|x| !x.children.is_empty() || !x.kept.is_empty());
    let para = ParaProps { foreign: None, ..f.para.clone() } != ParaProps::default();
    let char = CharProps { foreign: None, ..f.char.clone() } != CharProps::default();
    para || char || unmodelled(f.para.foreign.as_deref()) || unmodelled(f.char.foreign.as_deref())
}

impl Empty for Style {
    fn empty() -> Rc<Style> {
        Rc::default()
    }
}

impl Empty for Defaults {
    fn empty() -> Rc<Defaults> {
        Rc::default()
    }
}

impl Empty for LatentStyles {
    fn empty() -> Rc<LatentStyles> {
        Rc::default()
    }
}

/// A style: typed, with where it was read from in its origin.
///
/// The written form stands for the definition only while it is the one it was read as; never part of equality: use
/// [`Authored::authored_eq`].
pub type StyleDef = Authored<Rc<Style>, StyleSet>;

/// The document defaults: typed, with where they were read from in their origin.
///
/// The written form stands for the definition only while it is the one it was read as; never part of equality: use
/// [`Authored::authored_eq`].
pub type DefaultsDef = Authored<Rc<Defaults>, DefaultsSet>;

/// The latent styles: typed, with where they were read from in their origin. An edit replaces them whole.
///
/// The written form stands for the definition only while it is the one it was read as; never part of equality: use
/// [`Authored::authored_eq`].
pub type LatentDef = Authored<Rc<LatentStyles>, std::convert::Infallible>;
