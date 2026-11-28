//! The style cascade on typed definitions: style chains, table style regions, list levels and counters, and the
//! effective formats of paragraphs and runs over the document defaults, their styles, their list level and their
//! table style region

use std::collections::HashMap;
use std::rc::Rc;

use super::resolve::{self, RunEnv};
use super::{
    CellMargins, CellProps, CharProps, Color, ListChoice, ListDefId, ListLevel, Lists, ParaProps, RegionKind, Style, StyleEntry, StyleFormat,
    StyleKind, StyleSheet, TableBorders, TableProps,
};
use crate::fonts::GenericFamily;
use crate::format::{Look, ThemeColours, ThemeFonts, Width};
use crate::model::{Division, ListRef, ParaStyle, Run, RunStyle, Table};
use crate::paragraph::{ListId, ListJc};

/// How far a style chain is followed: a chain longer than this is cut, as the reader cuts it.
const MAX_CHAIN: usize = 32;

/// The order the conditional regions apply in, the later over the earlier.
pub const REGION_ORDER: [RegionKind; 12] = {
    use RegionKind::*;
    [Band1Horz, Band2Horz, Band1Vert, Band2Vert, LastCol, FirstCol, LastRow, FirstRow, SeCell, SwCell, NeCell, NwCell]
};

/// The document settings the cascade follows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rules {
    /// The generation of Word's layout the document asks for.
    pub compat: crate::policy::Compat,
    /// Word 2002's table style rules: a corner region doesn't turn on its row's or column's look bit.
    pub word2002_rules: bool,
    /// A 12pt Normal gives way to a table style's size in cells, and an 11pt cell mark takes its paragraph's.
    pub cell_size_rules: bool,
    /// OpenType features (ligatures, number forms and spacing, stylistic sets) apply.
    pub open_type: bool,
}

/// A table style region's formatting for one cell: the base and the active conditional regions of the style's chain,
/// folded.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TableRegion {
    /// What it sets.
    pub format: StyleFormat,
    /// The size a 12pt Normal paragraph's text takes in the cell, when the style sets one.
    pub size_over_normal: Option<f64>,
    /// An 11pt cell mark takes its paragraph's size.
    pub mark_at_style: bool,
}

/// A paragraph's effective format and what its runs and label resolve over.
#[derive(Clone, Debug)]
pub struct ParaCascade {
    /// The effective format.
    pub style: ParaStyle,
    /// Its runs' levels below their own: the defaults, the region's, the paragraph style chain's.
    pub base: CharProps,
    /// The list level it is at.
    pub level: Option<Rc<ListLevel>>,
    /// Its list.
    pub list: Option<ListId>,
    /// Its level in the list.
    pub ilvl: i64,
    /// The list comes from the paragraph style, not its own properties.
    pub from_style: bool,
    /// Its paragraph style.
    pub style_id: Option<Rc<str>>,
}

/// What the cascade works out of a style once: its levels as they cascade, and whether it counts as setting text.
#[derive(Clone, Debug)]
struct Derived {
    /// Its base in place of the one it defines: Word's built-in default table style's fixed properties, or its run
    /// properties from Word's built-in table defaults.
    format: Option<Rc<StyleFormat>>,
    /// It sets text, or has conditional regions.
    sets: bool,
}

/// The typed definitions a document resolves with, and what resolving needs beside them.
#[derive(Clone, Debug)]
pub struct Cascade {
    /// The styles.
    pub styles: Rc<StyleSheet>,
    /// The list definitions and lists.
    pub lists: Rc<Lists>,
    /// The theme's fonts.
    pub theme: ThemeFonts,
    /// The theme's colours.
    pub colours: ThemeColours,
    /// The settings the cascade follows.
    pub rules: Rules,
    /// The paragraph style linked to each list definition's level.
    pub style_links: HashMap<(ListDefId, i64), Rc<str>>,
    /// The generic category of each family the document's font table describes, by lowercase name.
    pub categories: Rc<HashMap<String, GenericFamily>>,
    /// The document's HTML divisions, by id.
    pub divisions: Rc<HashMap<i64, Division>>,
    /// Whether a style has conditional regions its typed definition doesn't model: the source format's reading of
    /// what it kept of the style.
    other_regions: fn(&Style) -> bool,
    doc_char: CharProps,
    doc_para: ParaProps,
    derived: HashMap<Rc<str>, Derived>,
}

/// A list choice as the list's id: 0 for none.
fn list_id(c: ListChoice) -> i64 {
    match c {
        ListChoice::None => 0,
        ListChoice::In(id) => id.0,
    }
}

/// Word's built-in default table style's properties: indent 0, cell margins 108 twips left and right.
fn default_table_format() -> StyleFormat {
    let margins =
        CellMargins { top: Some(Width::Dxa(0.0)), left: Some(Width::Dxa(5.4)), bottom: Some(Width::Dxa(0.0)), right: Some(Width::Dxa(5.4)) };
    StyleFormat { table: TableProps { indent: Some(Width::Dxa(0.0)), margins, ..TableProps::default() }, ..StyleFormat::default() }
}

/// A division and its ancestors, innermost first.
pub fn div_chain(divs: &HashMap<i64, Division>, id: Option<i64>) -> Vec<i64> {
    let mut out = Vec::new();
    let mut at = id.filter(|i| divs.contains_key(i));
    while let Some(i) = at.filter(|i| !out.contains(i)) {
        out.push(i);
        at = divs.get(&i).and_then(|d| d.parent);
    }
    out
}

impl Cascade {
    /// The cascade of `styles` and `lists` under `theme` and `colours`, following `rules`, a style's unmodelled
    /// conditional regions as `other_regions` finds them.
    pub fn new(
        styles: Rc<StyleSheet>,
        lists: Rc<Lists>,
        theme: ThemeFonts,
        colours: ThemeColours,
        rules: Rules,
        other_regions: fn(&Style) -> bool,
    ) -> Cascade {
        let mut c = Cascade {
            styles,
            lists,
            theme,
            colours,
            rules,
            style_links: HashMap::new(),
            categories: Rc::default(),
            divisions: Rc::default(),
            other_regions,
            doc_char: CharProps::default(),
            doc_para: ParaProps::default(),
            derived: HashMap::new(),
        };
        c.styles_changed();
        c
    }

    /// The cascade over `styles` and `lists` with what this one resolves with beside its definitions.
    pub fn over(&self, styles: Rc<StyleSheet>, lists: Rc<Lists>) -> Cascade {
        let mut next = Cascade::new(styles, lists, self.theme.clone(), self.colours, self.rules, self.other_regions);
        (next.categories, next.divisions) = (self.categories.clone(), self.divisions.clone());
        next
    }

    /// Works again what depends on the styles and lists: the defaults, each style's levels, the style links.
    pub fn styles_changed(&mut self) {
        let d = self.styles.defaults.as_deref();
        self.doc_char = d.and_then(|d| d.char.clone()).unwrap_or_else(super::Defaults::word_char);
        self.doc_para = d.and_then(|d| d.para.clone()).unwrap_or_else(super::Defaults::word_para);
        self.derived = self.styles.styles.iter().map(|(id, e)| (id.clone(), derive(e, self.other_regions))).collect();
        self.compute_style_links();
    }

    /// The document's character defaults: its own, else Word's.
    pub fn doc_char(&self) -> &CharProps {
        &self.doc_char
    }

    /// The document's paragraph defaults: its own, else Word's.
    pub fn doc_para(&self) -> &ParaProps {
        &self.doc_para
    }

    // Styles

    /// What style `e` sets at its base as it cascades.
    pub fn format_of<'a>(&'a self, e: &'a StyleEntry) -> &'a StyleFormat {
        self.derived.get(&e.style.id).and_then(|d| d.format.as_deref()).unwrap_or(&e.style.format)
    }

    /// The style `e` is based on as it cascades: none for Word's built-in default table style.
    pub fn based_on<'a>(&self, e: &'a StyleEntry) -> Option<&'a str> {
        e.style.based_on.as_deref().filter(|_| !e.style.word_default_table())
    }

    /// The conditional regions of `e` as it cascades: none for Word's built-in default table style.
    fn regions_of<'a>(&self, e: &'a StyleEntry) -> &'a [super::Region] {
        if e.style.word_default_table() { &[] } else { &e.style.regions }
    }

    /// A style and its base styles, root first: a frozen table style's chain stops at it, a style met again or a chain
    /// past `MAX_CHAIN` ends it.
    pub fn chain(&self, id: Option<&str>) -> Vec<&StyleEntry> {
        let mut out: Vec<&StyleEntry> = Vec::new();
        let mut cur = id.and_then(|i| self.styles.styles.get(i));
        while let Some(s) = cur {
            if out.iter().any(|o| o.style.id == s.style.id) || out.len() > MAX_CHAIN {
                break;
            }
            out.push(s);
            cur = self.based_on(s).filter(|_| !s.frozen).and_then(|b| self.styles.styles.get(b));
        }
        out.reverse();
        out
    }

    /// The paragraph properties of style `id`'s chain, folded.
    pub fn chain_para(&self, id: Option<&str>) -> ParaProps {
        let mut p = ParaProps::default();
        for s in self.chain(id) {
            p.merge(&self.format_of(s).para);
        }
        p
    }

    /// The character properties of style `id`'s chain, folded.
    pub fn chain_char(&self, id: Option<&str>) -> CharProps {
        CharProps::fold_chain(self.chain(id).into_iter().map(|s| &self.format_of(s).char))
    }

    /// A paragraph's style: `id` if it names a paragraph style, else the default one.
    pub fn paragraph_style<'a>(&'a self, id: Option<&'a str>) -> Option<&'a str> {
        match id {
            Some(id) if self.styles.styles.get(id).is_some_and(|s| s.style.kind == StyleKind::Paragraph) => Some(id),
            _ => self.styles.default_paragraph.as_deref(),
        }
    }

    /// A table's style: `id` if it names a style, else the default table style.
    pub fn table_style<'a>(&'a self, id: Option<&'a str>) -> Option<&'a str> {
        match id {
            Some(id) if self.styles.styles.contains_key(id) => Some(id),
            _ => self.styles.default_table.as_deref(),
        }
    }

    /// The styles whose properties come from `id`'s: it, and every style based on it, directly or not; by id.
    pub fn dependants(&self, id: &str) -> Vec<Rc<str>> {
        let mut out: Vec<Rc<str>> = self
            .styles
            .styles
            .values()
            .filter(|s| self.chain(Some(&s.style.id)).iter().any(|c| &*c.style.id == id))
            .map(|s| s.style.id.clone())
            .collect();
        out.sort();
        out
    }

    // Tables

    /// Table style `id`'s formatting for a cell in the `active` regions: each style of its chain's base (but its own
    /// cell borders and no-wrap; a style's own table borders replace its base's), then each active region in
    /// [`REGION_ORDER`] of each style.
    pub fn table_region(&self, id: Option<&str>, active: &[RegionKind]) -> TableRegion {
        let chain = self.chain(id);
        let mut out = TableRegion::default();
        let mut chars: Vec<&CharProps> = Vec::new();
        let add = |f: &StyleFormat, cell: &CellProps, out: &mut TableRegion| {
            out.format.para.merge(&f.para);
            out.format.table.merge(&f.table);
            out.format.cell.merge(cell);
            out.format.row.merge(&f.row);
        };
        for s in &chain {
            let f = self.format_of(s);
            let cell = CellProps { borders: TableBorders::default(), no_wrap: None, ..f.cell.clone() };
            if f.table.borders != TableBorders::default() {
                out.format.table.borders = TableBorders::default();
            }
            add(f, &cell, &mut out);
            chars.push(&f.char);
        }
        for kind in REGION_ORDER.iter().filter(|k| active.contains(k)) {
            for s in &chain {
                if let Some(r) = self.regions_of(s).iter().find(|r| r.kind == *kind) {
                    add(&r.format, &r.format.cell, &mut out);
                    chars.push(&r.format.char);
                }
            }
        }
        out.format.char = CharProps::fold_chain(chars);
        let sets = chain.iter().any(|s| self.derived.get(&s.style.id).is_some_and(|d| d.sets));
        let default_table = self.styles.default_table.as_deref();
        if self.rules.cell_size_rules && id.is_some() && id != default_table && sets {
            out.size_over_normal = out.format.char.size.or(self.doc_char.size).filter(|s| *s != 10.0);
        }
        out.mark_at_style = self.rules.cell_size_rules
            && (id.is_none() || id == default_table)
            && self.doc_char.size == Some(11.0)
            && self.chain_char(self.styles.default_paragraph.as_deref()).size == Some(12.0);
        out
    }

    /// Whether region `kind` is defined anywhere in style `id`'s chain.
    pub fn table_defines(&self, id: Option<&str>, kind: RegionKind) -> bool {
        self.chain(id).iter().any(|s| self.regions_of(s).iter().any(|r| r.kind == kind))
    }

    /// The regions of table style `style_id` active for cell (`ri`, `ci`) of a table of `nrows` rows (this row
    /// `ncols` cells long) with look `look`, bands `bands` and `headers` leading header rows.
    pub fn active_regions(
        &self,
        style_id: Option<&str>,
        look: Look,
        bands: (i64, i64),
        (ri, ci): (usize, usize),
        (nrows, ncols): (usize, usize),
        headers: usize,
    ) -> Vec<RegionKind> {
        use RegionKind::*;
        let defines = |r: RegionKind| self.table_defines(style_id, r);
        // A look bit counts only when the style formats one of its regions; with Word 2002's rules, not a corner.
        let corner = |r: RegionKind| !self.rules.word2002_rules && defines(r);
        let first_row = look.first_row && (defines(FirstRow) || look.first_col && corner(NwCell) || look.last_col && corner(NeCell));
        let last_row = look.last_row && (defines(LastRow) || look.first_col && corner(SwCell) || look.last_col && corner(SeCell));
        let first_col = look.first_col && (defines(FirstCol) || look.first_row && corner(NwCell) || look.last_row && corner(SwCell));
        let last_col = look.last_col && (defines(LastCol) || look.first_row && corner(NeCell) || look.last_row && corner(SeCell));
        // The first-row run: the leading header rows, row 0 with the firstRow bit
        let run = if first_row || defines(FirstRow) { headers.max(usize::from(first_row)) } else { 0 };
        let in_run = ri < run;
        let (last, end) = (ri + 1 == nrows, ci + 1 == ncols);
        let (row_band, col_band) = bands;
        let band = |i: i64, size: i64, one: RegionKind, two: RegionKind| if i >= 0 && (i / size) % 2 == 0 { one } else { two };
        let mut active = Vec::new();
        // One row region, one column region, at most one corner
        if in_run && first_row {
            active.push(FirstRow);
        } else if last && last_row {
            active.push(LastRow);
        } else if row_band > 0 && !look.no_h_band {
            active.push(band(ri as i64 - run as i64, row_band, Band1Horz, Band2Horz));
        }
        if ci == 0 && first_col {
            active.push(FirstCol);
        } else if end && last_col {
            active.push(LastCol);
        } else if col_band > 0 && !look.no_v_band {
            active.push(band(ci as i64 - i64::from(first_col), col_band, Band1Vert, Band2Vert));
        }
        let corners = [
            (ci == 0 && in_run && first_row && first_col, NwCell),
            (end && in_run && first_row && last_col, NeCell),
            (ci == 0 && last && last_row && first_col, SwCell),
            (end && last && last_row && last_col, SeCell),
        ];
        active.extend(corners.into_iter().find_map(|(on, c)| on.then_some(c)));
        active
    }

    /// Table style `style_id`'s formatting for a cell, as [`Self::active_regions`] places it.
    pub fn cell_region(
        &self,
        style_id: Option<&str>,
        look: Look,
        bands: (i64, i64),
        at: (usize, usize),
        size: (usize, usize),
        headers: usize,
    ) -> TableRegion {
        let active = self.active_regions(style_id, look, bands, at, size, headers);
        self.table_region(style_id, &active)
    }

    /// The look a table without its own has: header row and first column.
    pub fn default_look() -> Look {
        Look { first_row: true, last_row: false, first_col: true, last_col: false, no_h_band: false, no_v_band: true }
    }

    /// The region of cell (`row`, `cell`) of `t`.
    pub fn region_of(&self, t: &Table, row: usize, cell: usize) -> TableRegion {
        let (look, at, size, headers) = Self::placed(t, row, cell);
        self.cell_region(t.style_id.as_deref(), look, t.bands, at, size, headers)
    }

    /// Where cell (`row`, `cell`) of `t` is: its table's look, its place, the table's size at it and its header rows.
    pub fn placed(t: &Table, row: usize, cell: usize) -> (Look, (usize, usize), (usize, usize), usize) {
        let look = t.look.unwrap_or_else(Self::default_look);
        let ncols = t.rows.get(row).map_or(0, |r| r.cells.len());
        let headers = t.rows.iter().enumerate().take_while(|(i, r)| r.header || *i == 0 && look.first_row).count();
        (look, (row, cell), (t.rows.len(), ncols), headers)
    }

    // Lists

    /// The list a numbering style puts paragraphs in, through its chain: 0 for none over another.
    fn linked(&self, style: &str) -> Option<i64> {
        self.chain_para(Some(style)).list.map(list_id)
    }

    /// The definition `list` numbers with, through its definition's numbering style link once; a list without a
    /// definition numbers with none (-1).
    pub fn abstract_of(&self, list: ListId) -> Option<ListDefId> {
        let def = self.lists.lists.get(&list)?.def.unwrap_or(ListDefId(-1));
        if let Some(link) = self.lists.defs.get(&def).and_then(|d| d.num_style_link.as_deref()) {
            let linked = self.linked(link)?;
            if linked != list.0 {
                return self.lists.lists.get(&ListId(linked)).map(|n| n.def.unwrap_or(ListDefId(-1)));
            }
        }
        Some(def)
    }

    /// The level `level` (clamped to 0 to 8) of `list` numbers with: the list's own, else its definition's.
    pub fn level(&self, list: ListId, level: i64) -> Option<&Rc<ListLevel>> {
        let level = level.clamp(0, 8);
        if let Some(l) = self.lists.lists.get(&list).and_then(|n| n.override_of(level)).and_then(|o| o.def.as_ref()) {
            return Some(l);
        }
        let def = self.abstract_of(list)?;
        self.lists.defs.get(&def)?.levels[level as usize].as_ref()
    }

    /// The nine levels `list` numbers with: its definition's, with the list's own.
    pub fn levels(&self, list: ListId) -> Option<[Option<&Rc<ListLevel>>; 9]> {
        let def = self.lists.defs.get(&self.abstract_of(list)?)?;
        let own = self.lists.lists.get(&list);
        Some(std::array::from_fn(|k| own.and_then(|n| n.override_of(k as i64)).and_then(|o| o.def.as_ref()).or(def.levels[k].as_ref())))
    }

    /// The image a bullet level draws: its picture bullet's.
    pub fn bullet_image(&self, level: &ListLevel) -> Option<crate::ident::ResourceId> {
        self.lists.bullet_image(level)
    }

    /// Works out the paragraph styles linked to list levels: the definitions' levels' styles, then the paragraph styles'
    /// own lists.
    pub fn compute_style_links(&mut self) {
        let mut links = HashMap::new();
        for (a, def) in &self.lists.defs {
            for (i, l) in def.levels.iter().enumerate() {
                if let Some(s) = l.as_ref().and_then(|l| l.style.clone()) {
                    links.insert((*a, i as i64), s);
                }
            }
        }
        for id in &self.styles.paragraph_order {
            let Some(e) = self.styles.styles.get(id) else { continue };
            let para = &self.format_of(e).para;
            let Some(list) = para.list.map(list_id).filter(|n| *n != 0) else { continue };
            let Some(a) = self.abstract_of(ListId(list)) else { continue };
            links.entry((a, para.list_level.unwrap_or(0))).or_insert_with(|| id.clone());
        }
        self.style_links = links;
    }

    /// Whether a paragraph at level `ilvl` of `list` shows its label: one in the list through its style `style_id`
    /// (`from_style`) does when no style is linked to the level, or when it is that style or based on it without a
    /// list of its own.
    pub fn label_shows(&self, list: ListId, ilvl: i64, from_style: bool, style_id: Option<&str>) -> bool {
        !from_style || {
            let a = self.abstract_of(list).unwrap_or(ListDefId(-1));
            self.style_links.get(&(a, ilvl)).is_none_or(|linked| {
                let chain = self.chain(style_id);
                chain
                    .iter()
                    .rposition(|s| s.style.id == *linked)
                    .is_some_and(|i| chain[i + 1..].iter().all(|s| self.format_of(s).para.list.is_none()))
            })
        }
    }

    // Paragraphs and runs

    /// A paragraph with authored properties `direct`, its mark's size `mark_size` (points, when its mark sets one), in
    /// `region` when it is in a cell. The effective format's authored properties are left default.
    pub fn paragraph(&self, direct: &ParaProps, mark_size: Option<f64>, region: Option<&TableRegion>) -> ParaCascade {
        let style_id: Option<Rc<str>> = self.paragraph_style(direct.style.as_deref()).map(Rc::from);
        let style_para = self.chain_para(style_id.as_deref());
        let direct_list = direct.list.map(list_id);
        let (list, ilvl, from_style) = match direct_list {
            Some(id) => (Some(id), direct.list_level.or(style_para.list_level).unwrap_or(0), false),
            None => match style_para.list.map(list_id) {
                Some(id) => (Some(id), direct.list_level.or(style_para.list_level).unwrap_or(0), true),
                None => (None, 0, false),
            },
        };
        let list = list.filter(|&id| id != 0).map(ListId);
        let level = list.and_then(|id| self.level(id, ilvl)).cloned();

        let mut para = self.doc_para.clone();
        if let Some(t) = region {
            para.merge(&t.format.para);
        }
        match (&level, from_style) {
            (Some(l), true) => {
                para.merge(&l.para);
                para.merge(&style_para);
            }
            _ => {
                para.merge(&style_para);
                if let Some(l) = &level {
                    para.merge(&l.para);
                } else if direct_list == Some(0) {
                    // No list over a style's: start 0, first line 0 over the style chain's
                    (para.indent_left, para.indent_first) = (Some(0.0), Some(0.0));
                }
            }
        }
        para.merge(direct);
        para.style = style_id.clone();
        let mut style = resolve::para_style(&self.colours, &para);
        if let Some(l) = &level {
            // A legacy level places its number itself, in the label
            let legacy = l.legacy.is_some_and(|g| g.on == Some(true));
            style.list_jc = if legacy { ListJc::Left } else { super::list_jc(super::jc_name(l.jc.as_ref())) };
            style.num = list.map(|id| ListRef { list: id, level: ilvl });
            style.list_fmt = Some(super::format_name(l.format.as_ref()).to_string());
            style.list = list.and_then(|id| self.abstract_of(id)).map(|a| a.0);
        }

        let mut base = self.doc_char.clone();
        if let Some(t) = region {
            base.merge_level(&t.format.char);
        }
        base.merge_level(&self.chain_char(style_id.as_deref()));
        if let Some(size) = region.and_then(|t| t.size_over_normal) {
            let chain = self.chain(style_id.as_deref());
            let sizer = chain.into_iter().rev().find(|s| self.format_of(s).char.size.is_some());
            let normal = |s: &StyleEntry| self.styles.default_paragraph.as_deref() == Some(&*s.style.id) && self.format_of(s).char.size == Some(12.0);
            if sizer.is_some_and(normal) {
                base.size = Some(size);
            }
        }
        if region.is_some_and(|t| t.mark_at_style) && mark_size == Some(11.0) {
            style.mark_size = base.size;
        }
        // Division margins: the paragraph's division's and its ancestors'; in a table cell only its own's
        let mut chain = div_chain(&self.divisions, style.div);
        if region.is_some() {
            chain.truncate(1);
        }
        if !chain.is_empty() {
            let sum = |i: usize| chain.iter().map(|id| self.divisions[id].margins[i]).sum::<f64>();
            style.ind_left += sum(0);
            style.ind_right += sum(1);
        }
        ParaCascade { style, base, level, list, ilvl, from_style, style_id }
    }

    /// What resolving a run needs.
    pub fn env(&self) -> RunEnv<'_> {
        RunEnv { colours: &self.colours, fonts: &self.theme, categories: &self.categories, open_type: self.rules.open_type }
    }

    /// A run's levels over its paragraph's `base`: its character style's chain, then its own properties `direct`.
    pub fn run_props(&self, base: &CharProps, direct: &CharProps) -> CharProps {
        let style = direct.style.as_deref().map(|id| self.chain_char(Some(id)));
        resolve::run_props(base, style.as_ref(), direct)
    }

    /// The effective format of a run whose levels fold to `p`.
    pub fn run_style(&self, p: &CharProps) -> RunStyle {
        resolve::run_style(&self.env(), p)
    }

    /// A list label's run: its level's character properties `level` over `under`, what its paragraph mark's levels
    /// fold to.
    pub fn label(&self, under: &CharProps, level: Option<&CharProps>) -> Run {
        let mut p = under.clone();
        if let Some(l) = level {
            p.merge(l);
        }
        Rc::new(self.run_style(&p))
    }
}

/// What the cascade works out of style `e`, its unmodelled conditional regions as `other_regions` finds them.
fn derive(e: &StyleEntry, other_regions: fn(&Style) -> bool) -> Derived {
    let s = &e.style;
    if s.word_default_table() {
        return Derived { format: Some(Rc::new(default_table_format())), sets: false };
    }
    let format = e.word_run_defaults.then(|| {
        let mut f = s.format.clone();
        let c = &mut f.char;
        c.size = c.size.or(Some(10.0));
        c.color = c.color.or(Some(Color::Auto));
        c.spacing = c.spacing.or(Some(0.0));
        c.scale = c.scale.or(Some(100.0));
        Rc::new(f)
    });
    let sets = super::sets_text(&s.format) || !s.regions.is_empty() || other_regions(s);
    Derived { format, sets }
}
