//! Edit steps

use std::collections::HashSet;
use std::ops::Range;
use std::rc::Rc;

use crate::docx::{Block, Document, HeaderFooterId, Inline, Paragraph, ResourceId, Row, Run};
use crate::layout::{Change, ParaId, TableId, Unit};

use super::index::{Container, DocIndex};
use super::position::{Place, Position};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Edges {
    pub lead: u32,
    pub trail: u32,
    pub under: u32,
    /// The ends of the paragraphs a vanished mark joined onto the paragraph's start ([`crate::docx::JoinedPart`])
    /// once applied, when they aren't where the splice moves them: an undone splice's.
    pub parts: Option<Rc<[u32]>>,
}

#[derive(Clone, Debug)]
pub enum Step {
    Text {
        para: ParaId,
        range: Range<Unit>,
        with: Vec<Inline>,
        edges: Edges,
    },
    Split {
        para: ParaId,
        at: Unit,
        second: Paragraph,
        /// What `para` gets back once its tail is gone: the empty runs a join took from it.
        kept: Vec<Inline>,
    },
    Join {
        first: ParaId,
        second: ParaId,
    },
    Blocks {
        container: Container,
        range: Range<usize>,
        with: Vec<Block>,
    },
    Rows {
        table: TableId,
        range: Range<usize>,
        with: Vec<Row>,
        grid: Option<Vec<f64>>,
    },
    Para {
        para: ParaId,
        style: Rc<crate::docx::ParaStyle>,
        mark: crate::docx::Run,
        list: Option<crate::docx::ListLabel>,
    },
    Restyle {
        para: ParaId,
        inlines: Vec<Inline>,
    },
    ImageData {
        image: ResourceId,
        data: Vec<u8>,
        filled: bool,
    },
    HeaderPart {
        id: HeaderFooterId,
        section: usize,
        footer: bool,
        slot: usize,
        blocks: Option<Vec<Block>>,
    },
    BuildingBlock {
        k: u32,
        part: Option<Box<crate::docx::BuildingBlock>>,
    },
    TableProps {
        table: TableId,
        props: TableProps,
    },
    Note {
        kind: crate::docx::NoteKind,
        id: i64,
        blocks: Option<Vec<Block>>,
    },
    /// A note separator story (by [`crate::docx::NoteSeparator::key`]) replaced, or taken away.
    Separator {
        key: String,
        blocks: Option<Vec<Block>>,
    },
    SectionPage {
        section: usize,
        page: Box<crate::docx::SectionPage>,
        change: Option<Rc<crate::docx::review::SectionChange>>,
    },
    Control {
        id: crate::docx::ControlId,
        control: Option<crate::docx::Control>,
    },
    DataPart {
        item_id: String,
        part: Option<crate::docx::DataPart>,
    },
    Comment {
        id: crate::docx::review::CommentId,
        after: Option<crate::docx::review::Comment>,
    },
    FieldViews {
        para: ParaId,
        views: Vec<(crate::docx::FieldId, crate::docx::FieldView)>,
    },
    AddImage {
        image: ResourceId,
        data: Option<Vec<u8>>,
    },
    /// The styles and numbering tables, and the lists added since reading.
    Resolver {
        resolver: Option<Rc<crate::docx::resolvers::Resolvers>>,
        added: Vec<crate::docx::AddedList>,
    },
    /// The style, numbering and theme tables with a family renamed, and the renames the save applies.
    Fonts {
        resolver: Option<Rc<crate::docx::resolvers::Resolvers>>,
        renames: Vec<crate::docx::font_table::FontRename>,
    },
    DefaultTab(f64),
    Hyphenation(Hyphenating),
    PageSettings(PageSettings),
    SectionBreak {
        section: usize,
        back: Option<Box<SectionBack>>,
    },
    /// Sections split off the front of section `section`, or the ones before it joined back into it.
    Sections {
        section: usize,
        split: Box<Split>,
    },
    /// The style new tables take.
    DefaultTableStyle(Option<String>),
    /// The document's protection.
    Protection(Option<Box<crate::docx::review::Protection>>),
    /// The document's properties and variables.
    Info(Box<(crate::docx::properties::Properties, Vec<(String, String)>)>),
    /// The page background.
    PageBackground(Box<PageBackground>),
}

/// The page background: its colour, the theme colour that is, its fill over it, and whether it shows.
#[derive(Clone, Debug, PartialEq)]
pub struct PageBackground {
    pub colour: Option<u32>,
    pub theme: Option<crate::docx::ThemeColourRef>,
    pub fill: Option<crate::drawing::paint::Fill>,
    pub shown: bool,
}

impl PageBackground {
    pub fn of(doc: &Document) -> PageBackground {
        PageBackground {
            colour: doc.page_colour,
            theme: doc.page_colour_theme,
            fill: doc.page_fill.clone(),
            shown: doc.settings.display_background_shape,
        }
    }
}

/// The document's page settings: odd and even headers, how pages pair, the gutter's place, and the page borders
/// around headers and footers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PageSettings {
    pub even_and_odd: bool,
    pub mirror_margins: bool,
    pub gutter_at_top: bool,
    pub book_fold: Option<bool>,
    pub no_surround_header: bool,
    pub no_surround_footer: bool,
    pub endnotes_sect_end: bool,
    pub no_form_shading: bool,
    pub auto_format_override: bool,
    pub style_lock_theme: bool,
    pub style_lock_style_set: bool,
}

impl PageSettings {
    pub fn of(s: &crate::docx::Settings) -> PageSettings {
        PageSettings {
            even_and_odd: s.even_and_odd,
            mirror_margins: s.mirror_margins,
            gutter_at_top: s.gutter_at_top,
            book_fold: s.book_fold,
            no_surround_header: s.borders_do_not_surround_header,
            no_surround_footer: s.borders_do_not_surround_footer,
            endnotes_sect_end: s.endnotes_sect_end,
            no_form_shading: s.no_form_shading,
            auto_format_override: s.auto_format_override,
            style_lock_theme: s.style_lock_theme,
            style_lock_style_set: s.style_lock_style_set,
        }
    }
}

/// The document's hyphenation settings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hyphenating {
    pub auto: bool,
    pub zone: f64,
    pub limit: i64,
    /// Words in capitals aren't hyphenated.
    pub caps: bool,
}

#[derive(Clone, Debug)]
pub struct SectionBack {
    pub blocks: usize,
    pub section: crate::docx::Section,
    pub sources: usize,
    pub edited: bool,
}

/// Sections split off the front of one, in order, with the header and footer stories only they refer to; or how many
/// before one join back into it, and the stories they take out.
#[derive(Clone, Debug)]
pub enum Split {
    Apart { backs: Vec<SectionBack>, stories: Vec<(HeaderFooterId, Vec<Block>)> },
    Joined { count: usize, stories: Vec<HeaderFooterId> },
}

#[derive(Clone, Debug, PartialEq)]
pub struct TableProps {
    pub style_id: Option<String>,
    pub look: Option<crate::docx::props::Look>,
    pub width: crate::docx::props::Width,
    pub jc: crate::docx::props::Jc,
    pub ind: f64,
    pub ind_pct: Option<f64>,
    /// Whether its indent is a twips one (`w:tblInd` dxa).
    pub ind_set: bool,
    pub borders: [Option<crate::docx::props::Border>; 6],
    /// Fixed column widths (`w:tblLayout` fixed), not fitted to the content.
    pub fixed: bool,
    pub bands: (i64, i64),
    pub floating: Option<crate::docx::props::TablePos>,
    pub cell_spacing: f64,
    /// The table's shading, its own over its style's.
    pub shd: Option<crate::docx::props::Shd>,
    pub alt: Option<Rc<crate::docx::TableAlt>>,
    /// Its authored properties, its own shading among them.
    pub direct: crate::docx::table_props::TableDirect,
}

impl Step {
    pub fn pictures_of(&self, images: &HashSet<ResourceId>) -> i64 {
        self.pictures_where(&|k| images.contains(&k))
    }

    pub fn pictures_where(&self, images: &dyn Fn(ResourceId) -> bool) -> i64 {
        fn inlines(v: &[Inline], images: &dyn Fn(ResourceId) -> bool) -> i64 {
            v.iter()
                .filter(|i| match i {
                    Inline::Picture(p, _) => p.image.is_some_and(images),
                    Inline::Float(f, _) => f.image.is_some_and(images),
                    _ => false,
                })
                .count() as i64
        }
        fn blocks(v: &[Block], images: &dyn Fn(ResourceId) -> bool) -> i64 {
            v.iter()
                .map(|b| match b {
                    Block::Para(p) => inlines(&p.inlines, images),
                    Block::Table(t) => t.rows.iter().flat_map(|r| r.cells.iter()).map(|c| blocks(&c.blocks, images)).sum(),
                })
                .sum()
        }
        match self {
            Step::Text { with, .. } | Step::Restyle { inlines: with, .. } => inlines(with, images),
            Step::Blocks { with, .. } => blocks(with, images),
            Step::Rows { with, .. } => with.iter().flat_map(|r| r.cells.iter()).map(|c| blocks(&c.blocks, images)).sum(),
            Step::HeaderPart { blocks: Some(b), .. } | Step::Note { blocks: Some(b), .. } => blocks(b, images),
            Step::Sections { split, .. } => split.stories().map(|b| blocks(b, images)).sum(),
            _ => 0,
        }
    }

    pub fn lays_out(&self) -> bool {
        !matches!(
            self,
            Step::Control { .. } | Step::DataPart { .. } | Step::Comment { .. } | Step::AddImage { .. } | Step::Resolver { .. } | Step::Fonts { .. }
        )
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Targets {
    pub paras: HashSet<ParaId>,
    pub tables: HashSet<TableId>,
    pub elements: HashSet<crate::docx::ElementId>,
    pub revisions: HashSet<crate::docx::review::RevisionId>,
    pub comments: HashSet<crate::docx::review::CommentId>,
}

impl Targets {
    pub fn add(&mut self, s: &Step) {
        use crate::docx::ElementId;
        match s {
            Step::Text { para, with, .. } | Step::Restyle { para, inlines: with } => {
                self.paras.insert(*para);
                self.inlines(with);
            }
            Step::Split { para, second, .. } => {
                self.paras.insert(*para);
                self.para(second);
            }
            Step::Join { first, second } => self.paras.extend([*first, *second]),
            Step::Blocks { container, with, .. } => {
                if let Container::Cell { table, .. } = container {
                    self.tables.insert(*table);
                }
                self.blocks(with);
            }
            Step::Rows { table, with, .. } => {
                self.tables.insert(*table);
                with.iter().flat_map(|r| r.cells.iter()).for_each(|c| self.blocks(&c.blocks));
            }
            Step::Para { para, style, mark, .. } => {
                self.paras.insert(*para);
                self.run(mark);
                self.revisions.extend(style.change.as_ref().map(|c| c.mark.id));
            }
            Step::TableProps { table, .. } => {
                self.tables.insert(*table);
            }
            Step::HeaderPart { blocks: Some(b), .. } | Step::Note { blocks: Some(b), .. } => self.blocks(b),
            Step::Sections { split, .. } => split.stories().for_each(|b| self.blocks(b)),
            Step::SectionPage { change: Some(c), .. } => {
                self.revisions.insert(c.mark.id);
            }
            Step::Control { id, .. } => {
                self.elements.insert(ElementId::Control(*id));
            }
            Step::Comment { id, .. } => {
                self.comments.insert(*id);
            }
            Step::FieldViews { para, views } => {
                self.paras.insert(*para);
                self.elements.extend(views.iter().map(|(f, _)| ElementId::Field(*f)));
            }
            _ => {}
        }
    }

    fn blocks(&mut self, v: &[Block]) {
        for b in v {
            match b {
                Block::Para(p) => self.para(p),
                Block::Table(t) => {
                    self.tables.insert(TableId(t.id));
                    self.elements.extend(t.controls.iter().map(|c| crate::docx::ElementId::Control(*c)));
                    t.rows.iter().flat_map(|r| r.cells.iter()).for_each(|c| self.blocks(&c.blocks));
                }
            }
        }
    }

    fn para(&mut self, p: &Paragraph) {
        self.paras.insert(ParaId(p.id));
        self.run(&p.mark);
        self.revisions.extend(p.style.change.as_ref().map(|c| c.mark.id));
        self.elements.extend(p.controls.iter().map(|c| crate::docx::ElementId::Control(*c)));
        self.inlines(&p.inlines);
    }

    fn inlines(&mut self, v: &[Inline]) {
        use crate::docx::ElementId;
        use crate::docx::review::{CommentId, RangeKind};
        for i in v {
            if let Some(r) = run_of(i) {
                self.run(r);
            }
            match i {
                Inline::FieldBegin { id, .. } => {
                    self.elements.insert(ElementId::Field(*id));
                }
                Inline::ControlStart(c) | Inline::Sym { control: Some(c), .. } => {
                    self.elements.insert(ElementId::Control(*c));
                }
                Inline::CommentRef { id, .. } => {
                    self.comments.insert(*id);
                }
                Inline::Range(r) if matches!(r.kind, RangeKind::CommentStart | RangeKind::CommentEnd) => {
                    self.comments.insert(CommentId(r.id));
                }
                i if i.story().is_some() => {
                    if let Some((_, b)) = i.story() {
                        self.blocks(&b.blocks);
                    }
                }
                _ => {}
            }
        }
    }

    fn run(&mut self, r: &Run) {
        let rev = &r.rev;
        self.revisions.extend(rev.ins.iter().chain(&rev.del).map(|m| m.id).chain(rev.fmt.as_ref().map(|f| f.mark.id)));
    }

    pub fn extend(&mut self, other: Targets) {
        self.paras.extend(other.paras);
        self.tables.extend(other.tables);
        self.elements.extend(other.elements);
        self.revisions.extend(other.revisions);
        self.comments.extend(other.comments);
    }
}

impl TableProps {
    /// Sets `t`'s properties to these, returning the ones it had.
    pub(super) fn put(self, t: &mut crate::docx::Table) -> TableProps {
        let (old, props) = (TableProps::of(t), self);
        (
            t.style_id,
            t.look,
            t.width,
            t.jc,
            t.ind,
            t.ind_pct,
            t.ind_set,
            t.borders,
            t.fixed,
            t.bands,
            t.floating,
            t.cell_spacing,
            t.shd,
            t.alt,
            t.direct,
        ) = (
            props.style_id,
            props.look,
            props.width,
            props.jc,
            props.ind,
            props.ind_pct,
            props.ind_set,
            props.borders,
            props.fixed,
            props.bands,
            props.floating,
            props.cell_spacing,
            props.shd,
            props.alt,
            props.direct,
        );
        old
    }

    pub fn of(t: &crate::docx::Table) -> TableProps {
        TableProps {
            style_id: t.style_id.clone(),
            look: t.look,
            width: t.width,
            jc: t.jc,
            ind: t.ind,
            ind_pct: t.ind_pct,
            ind_set: t.ind_set,
            borders: t.borders.clone(),
            fixed: t.fixed,
            bands: t.bands,
            floating: t.floating,
            cell_spacing: t.cell_spacing,
            shd: t.shd.clone(),
            alt: t.alt.clone(),
            direct: t.direct.clone(),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum StepError {
    Missing,
    OutOfRange,
    ReadOnly,
}

pub struct Applied {
    pub inverse: Step,
    pub map: Map,
    pub hint: Option<Change>,
}

#[derive(Clone, Debug)]
pub enum Map {
    Text { para: ParaId, start: Unit, old_end: Unit, new_end: Unit },
    Split { para: ParaId, at: Unit, len: Unit, second: ParaId },
    Join { first: ParaId, second: ParaId, offset: Unit },
    Removed { paras: HashSet<ParaId>, tables: HashSet<TableId>, rows: Option<(TableId, Range<usize>, isize)>, fallback: Option<Position> },
    Same,
}

impl Map {
    pub fn apply(&self, pos: Position) -> Position {
        match (self, pos.place) {
            (Map::Text { para, start, old_end, new_end }, Place::Text { para: p, offset, affinity }) if p == *para => {
                let offset = if offset >= *old_end {
                    Unit(offset.0 - old_end.0 + new_end.0)
                } else if offset > *start {
                    *start
                } else {
                    offset
                };
                Position { place: Place::Text { para: p, offset, affinity }, ..pos }
            }
            (Map::Split { para, at, len, second }, Place::Text { para: p, offset, affinity }) if p == *para => {
                if offset >= *at && *at < *len {
                    Position { place: Place::Text { para: *second, offset: Unit(offset.0 - at.0), affinity }, ..pos }
                } else {
                    pos
                }
            }
            (Map::Join { first, second, offset: shift }, Place::Text { para: p, offset, affinity }) if p == *second => {
                Position { place: Place::Text { para: *first, offset: Unit(offset.0 + shift.0), affinity }, ..pos }
            }
            (Map::Removed { paras, fallback, .. }, Place::Text { para, .. }) if paras.contains(&para) => fallback.unwrap_or(pos),
            (Map::Removed { tables, rows, fallback, .. }, Place::RowEnd { table, row }) => {
                if tables.contains(&table) {
                    return fallback.unwrap_or(pos);
                }
                match rows {
                    Some((t, r, shift)) if *t == table && row >= r.end => {
                        Position { place: Place::RowEnd { table, row: (row as isize + shift) as usize }, ..pos }
                    }
                    Some((t, r, _)) if *t == table && r.contains(&row) => fallback.unwrap_or(pos),
                    _ => pos,
                }
            }
            _ => pos,
        }
    }
}

// Inlines by units

/// The markers at unit `u` of `p` from the first one `first` takes on: what an insertion there goes before (`Edges`'s
/// `under`; empty text isn't counted).
pub fn markers_from(p: &Paragraph, u: Unit, first: impl Fn(&Inline) -> bool) -> u32 {
    let (mut at, mut n, mut seen) = (0u32, 0, false);
    for i in &p.inlines {
        let w = i.units() as u32;
        if at > u.0 || (at == u.0 && w > 0) {
            break;
        }
        if at == u.0 && !matches!(i, Inline::Text(..)) {
            seen |= first(i);
            n += u32::from(seen);
        }
        at += w;
    }
    n
}

pub fn para_len(p: &Paragraph) -> Unit {
    Unit(p.inlines.iter().map(Inline::units).sum::<usize>() as u32)
}

pub fn run_of(i: &Inline) -> Option<&Run> {
    match i {
        Inline::Text(_, r)
        | Inline::Tab(r)
        | Inline::PTab(_, r)
        | Inline::Break(_, r)
        | Inline::Picture(_, r)
        | Inline::Float(_, r)
        | Inline::NoteMark(_, _, r)
        | Inline::Ruby(_, r)
        | Inline::DateBlock(_, _, r) => Some(r),
        Inline::Sym { run, .. }
        | Inline::NoteRef { run, .. }
        | Inline::Separator { run, .. }
        | Inline::FieldBegin { run, .. }
        | Inline::FieldSep { run, .. }
        | Inline::FieldEnd { run, .. }
        | Inline::CommentRef { run, .. } => Some(run),
        Inline::Range(_) | Inline::ControlStart(_) | Inline::ControlEnd(_) | Inline::Opaque(_) => None,
    }
}

fn cut(inlines: &mut Vec<Inline>, u: Unit, after_markers: bool) -> usize {
    let mut at = 0u32;
    let mut k = 0;
    while k < inlines.len() {
        let n = inlines[k].units() as u32;
        if at == u.0 && (n > 0 || !after_markers) {
            return k;
        }
        if at < u.0
            && u.0 < at + n
            && let Inline::Text(t, r) = &mut inlines[k]
        {
            let byte = t.char_indices().nth((u.0 - at) as usize).map_or(t.len(), |(b, _)| b);
            let rest = Inline::Text(t.split_off(byte), r.clone());
            inlines.insert(k + 1, rest);
            return k + 1;
        }
        at += n;
        k += 1;
    }
    inlines.len()
}

/// Moves the ends of the paragraphs a vanished mark joined onto `p`'s start past a splice of `range` with `added`
/// units, or to `exact`: their ends before, when that moved them.
fn shift_parts(p: &mut Paragraph, range: &Range<Unit>, added: u32, exact: Option<&[u32]>) -> Option<Rc<[u32]>> {
    let src = p.src.as_mut().filter(|s| !s.joined.is_empty())?;
    let (start, end) = (range.start.0, range.end.0);
    let exact = exact.filter(|e| e.len() == src.joined.len());
    let moved = |k: usize, at: u32| match exact {
        Some(e) => e[k],
        None if at > start => at.max(end) - end + start + added,
        None => at,
    };
    if src.joined.iter().enumerate().all(|(k, j)| moved(k, j.end) == j.end) {
        return None;
    }
    let before = src.joined.iter().map(|j| j.end).collect();
    for (k, j) in Rc::make_mut(src).joined.iter_mut().enumerate() {
        j.end = moved(k, j.end);
    }
    Some(before)
}

fn splice(p: &mut Paragraph, range: Range<Unit>, edges: &Edges, with: Vec<Inline>) -> (Vec<Inline>, u32) {
    let back = |inlines: &[Inline], mut k: usize, mut n: u32| {
        while n > 0 && k > 0 {
            k -= 1;
            if !matches!(inlines[k], Inline::Text(..)) {
                n -= 1;
            }
        }
        k
    };
    let a0 = cut(&mut p.inlines, range.start, true);
    let under = back(&p.inlines, a0, edges.under);
    let b0 = if range.end == range.start { under } else { cut(&mut p.inlines, range.end, false).max(a0) };
    let a = back(&p.inlines, under, edges.lead);
    let mut b = b0;
    let mut trail = edges.trail;
    while trail > 0 && b < p.inlines.len() {
        if !matches!(p.inlines[b], Inline::Text(..)) {
            trail -= 1;
        }
        b += 1;
    }
    let b = b.max(a);
    let empty = |i: &Inline| matches!(i, Inline::Text(t, _) if t.is_empty());
    // Empty runs beside the edit, when the edit leaves units; otherwise the sole carrier stays
    let units = |s: &[Inline]| s.iter().any(|i| i.units() > 0);
    let (a, b) = if units(&with) || units(&p.inlines[..a]) || units(&p.inlines[b..]) {
        (a - p.inlines[..a].iter().rev().take_while(|i| empty(i)).count(), b + p.inlines[b..].iter().take_while(|i| empty(i)).count())
    } else {
        (a, b)
    };
    let removed: Vec<Inline> = p.inlines.splice(a..b, std::iter::empty()).collect();
    let mut at = a;
    let n = with.len();
    for (k, i) in with.into_iter().enumerate() {
        if let Inline::Text(s, r) = &i {
            if let Some(Inline::Text(prev, pr)) = at.checked_sub(1).and_then(|k| p.inlines.get_mut(k))
                && std::rc::Rc::ptr_eq(pr, r)
            {
                prev.push_str(s);
                continue;
            }
            if k + 1 == n
                && let Some(Inline::Text(next, nr)) = p.inlines.get_mut(at)
                && std::rc::Rc::ptr_eq(nr, r)
            {
                next.insert_str(0, s);
                at += 1;
                continue;
            }
        }
        p.inlines.insert(at, i);
        at += 1;
    }
    let after = p.inlines[at..].iter().take_while(|i| i.units() == 0).filter(|i| !matches!(i, Inline::Text(..))).count() as u32;
    merge_at(&mut p.inlines, at);
    merge_at(&mut p.inlines, a);
    let has_units = p.inlines.iter().any(|i| i.units() > 0);
    let mut kept_one = false;
    p.inlines.retain(|i| match i {
        Inline::Text(s, _) if s.is_empty() => {
            if has_units || kept_one {
                return false;
            }
            kept_one = true;
            true
        }
        _ => true,
    });
    (removed, after)
}

fn merge_at(inlines: &mut Vec<Inline>, k: usize) {
    if k == 0 || k >= inlines.len() {
        return;
    }
    let same = matches!((&inlines[k - 1], &inlines[k]), (Inline::Text(_, a), Inline::Text(_, b)) if std::rc::Rc::ptr_eq(a, b));
    if same
        && let Inline::Text(t, _) = inlines.remove(k)
        && let Some(Inline::Text(prev, _)) = inlines.get_mut(k - 1)
    {
        prev.push_str(&t);
    }
}

fn units_of(inlines: &[Inline]) -> u32 {
    inlines.iter().map(Inline::units).sum::<usize>() as u32
}

/// The line numbers' run, from the styles now in place.
fn refresh_line_number_run(doc: &mut Document) {
    if let Some(res) = &doc.resolver {
        doc.line_number_run = Some(crate::docx::line_number_run(res));
    }
}

// Hints

/// Whether the container's blocks are a section's as layout sees them: a section's, or the sub-template being edited.
fn lays_out_as_section(idx: &DocIndex, container: Container) -> bool {
    match container {
        Container::Section(_) => true,
        Container::Story(super::position::StoryId::BuildingBlock(k)) => idx.flow == Some(k),
        _ => false,
    }
}

fn hint_in(idx: &DocIndex, container: Container, block: usize, added: isize) -> Option<Change> {
    span_hint(idx, container, block, 1, (1 + added).max(0) as usize)
}

/// `old_len` blocks from `first` became `new_len`; outside a section, the top block holding them changed.
fn span_hint(idx: &DocIndex, container: Container, first: usize, old_len: usize, new_len: usize) -> Option<Change> {
    let (section, b, row) = idx.top_block(container, first)?;
    match row {
        None if lays_out_as_section(idx, container) => Some(Change { section, first: b, old_end: b + old_len, new_end: b + new_len, row: None }),
        row => Some(Change { section, first: b, old_end: b + 1, new_end: b + 1, row }),
    }
}

pub(super) fn para_hint(idx: &DocIndex, para: ParaId, added: isize) -> Option<Change> {
    let s = idx.para_slot(para)?;
    hint_in(idx, s.container, s.block, added)
}

// Apply

impl Step {
    pub fn apply(self, doc: &mut Document, idx: &mut DocIndex) -> Result<Applied, StepError> {
        match self {
            Step::Text { para, range, with, edges } => {
                let p = idx.para_mut(doc, para).ok_or(StepError::Missing)?;
                if range.start > range.end || range.end > para_len(p) {
                    return Err(StepError::OutOfRange);
                }
                let added = units_of(&with);
                let indexed = super::index::indexes(&with);
                let marker = |i: &&Inline| i.units() == 0 && !matches!(i, Inline::Text(..));
                let lead = with.iter().take_while(marker).count();
                let trail = if lead == with.len() { 0 } else { with.iter().rev().take_while(marker).count() };
                let (removed, after) = splice(p, range.clone(), &edges, with);
                let parts = shift_parts(p, &range, added, edges.parts.as_deref());
                let under = if added == 0 { after } else { 0 };
                let new_end = Unit(range.start.0 + added);
                idx.replaced_inlines(doc, para, &removed, indexed);
                Ok(Applied {
                    inverse: Step::Text {
                        para,
                        range: range.start..new_end,
                        with: removed,
                        edges: Edges { lead: lead as u32, trail: trail as u32, under, parts },
                    },
                    map: Map::Text { para, start: range.start, old_end: range.end, new_end },
                    hint: para_hint(idx, para, 0),
                })
            }
            Step::Split { para, at, mut second, kept } => {
                let slot = idx.para_slot(para).ok_or(StepError::Missing)?;
                let p = idx.para_mut(doc, para).ok_or(StepError::Missing)?;
                let len = para_len(p);
                if at > len {
                    return Err(StepError::OutOfRange);
                }
                let tail: Vec<Inline> = if at < len {
                    let k = cut(&mut p.inlines, at, true);
                    p.inlines.drain(k..).collect()
                } else {
                    Vec::new()
                };
                idx.moved_inlines(para, &tail);
                if !second.inlines.is_empty() {
                    let keep = p.inlines.iter().rposition(|i| i.units() > 0 || !second.inlines.contains(i)).map_or(0, |k| k + 1);
                    if keep < p.inlines.len() {
                        let taken: Vec<Inline> = p.inlines.drain(keep..).collect();
                        idx.moved_inlines(para, &taken);
                    }
                } else if !tail.iter().any(|i| i.units() > 0 || matches!(i, Inline::Text(..)))
                    && let Some(r) = p.inlines.iter().rev().find_map(run_of)
                {
                    second.inlines = vec![Inline::Text(String::new(), r.clone())];
                    second.inlines.splice(0..0, tail);
                } else {
                    second.inlines = tail;
                }
                p.inlines.extend(kept);
                let second_id = ParaId(second.id);
                let blocks = idx.blocks_mut(doc, slot.container).ok_or(StepError::Missing)?;
                blocks.insert(slot.block + 1, Block::Para(second));
                idx.replaced_blocks(doc, slot.container, &[]);
                Ok(Applied {
                    inverse: Step::Join { first: para, second: second_id },
                    map: Map::Split { para, at, len, second: second_id },
                    hint: hint_in(idx, slot.container, slot.block, 1),
                })
            }
            Step::Join { first, second } => {
                let a = idx.para_slot(first).ok_or(StepError::Missing)?;
                let b = idx.para_slot(second).ok_or(StepError::Missing)?;
                if a.container != b.container || b.block != a.block + 1 {
                    return Err(StepError::OutOfRange);
                }
                let blocks = idx.blocks_mut(doc, a.container).ok_or(StepError::Missing)?;
                let tail = blocks.remove(b.block);
                let Block::Para(t) = &tail else { return Err(StepError::Missing) };
                let (inlines, indexed) = (t.inlines.clone(), super::index::indexes(&t.inlines));
                let p = match blocks.get_mut(a.block) {
                    Some(Block::Para(p)) => p,
                    _ => return Err(StepError::Missing),
                };
                let at = para_len(p);
                let (kept, _) = splice(p, at..at, &Edges::default(), inlines);
                idx.replaced_blocks(doc, a.container, std::slice::from_ref(&tail));
                idx.replaced_inlines(doc, first, &[], indexed);
                let Block::Para(tail) = tail else { return Err(StepError::Missing) };
                let hint = span_hint(idx, a.container, a.block, 2, 1);
                Ok(Applied { inverse: Step::Split { para: first, at, second: tail, kept }, map: Map::Join { first, second, offset: at }, hint })
            }
            Step::Blocks { container, range, with } => {
                let blocks = idx.blocks_mut(doc, container).ok_or(StepError::Missing)?;
                if range.start > range.end || range.end > blocks.len() {
                    return Err(StepError::OutOfRange);
                }
                let added = with.len();
                let removed: Vec<Block> = blocks.splice(range.clone(), with).collect();
                let kept: HashSet<u64> = blocks[range.start..range.start + added].iter().map(block_id).collect();
                let (mut paras, mut tables) = (HashSet::new(), HashSet::new());
                removed.iter().filter(|b| !kept.contains(&block_id(b))).for_each(|b| ids(b, &mut paras, &mut tables));
                idx.replaced_blocks(doc, container, &removed);
                let fallback = first_position(doc, idx, container, range.start);
                let first = if lays_out_as_section(idx, container) { range.start } else { 0 };
                let hint = span_hint(idx, container, first, range.len(), added);
                Ok(Applied {
                    inverse: Step::Blocks { container, range: range.start..range.start + added, with: removed },
                    map: Map::Removed { paras, tables, rows: None, fallback },
                    hint,
                })
            }
            Step::Restyle { para, inlines } => {
                let p = idx.para_mut(doc, para).ok_or(StepError::Missing)?;
                if units_of(&inlines) != units_of(&p.inlines) {
                    return Err(StepError::OutOfRange);
                }
                let indexed = super::index::indexes(&inlines);
                let old = std::mem::replace(&mut p.inlines, inlines);
                idx.replaced_inlines(doc, para, &old, indexed);
                Ok(Applied { inverse: Step::Restyle { para, inlines: old }, map: Map::Same, hint: para_hint(idx, para, 0) })
            }
            Step::Para { para, style, mark, list } => {
                let p = idx.para_mut(doc, para).ok_or(StepError::Missing)?;
                let old_style = std::mem::replace(&mut p.style, style);
                let old_mark = std::mem::replace(&mut p.mark, mark);
                let old_list = std::mem::replace(&mut p.list, list);
                p.refresh_div();
                Ok(Applied {
                    inverse: Step::Para { para, style: old_style, mark: old_mark, list: old_list },
                    map: Map::Same,
                    hint: para_hint(idx, para, 0),
                })
            }
            Step::Note { kind, id, blocks } => {
                let map = match kind {
                    crate::docx::NoteKind::Foot => &mut doc.footnotes,
                    crate::docx::NoteKind::End => &mut doc.endnotes,
                };
                let adding = blocks.is_some();
                let old = match blocks {
                    Some(b) => map.insert(id, b),
                    None => map.remove(&id),
                };
                if !adding && old.is_none() {
                    return Err(StepError::Missing);
                }
                let story = match kind {
                    crate::docx::NoteKind::Foot => super::position::StoryId::Footnote(id),
                    crate::docx::NoteKind::End => super::position::StoryId::Endnote(id),
                };
                let removed = old.clone().unwrap_or_default();
                idx.replaced_blocks(doc, Container::Story(story), &removed);
                let (mut paras, mut tables) = (HashSet::new(), HashSet::new());
                removed.iter().for_each(|b| ids(b, &mut paras, &mut tables));
                Ok(Applied {
                    inverse: Step::Note { kind, id, blocks: old },
                    map: Map::Removed { paras, tables, rows: None, fallback: None },
                    hint: None,
                })
            }
            Step::Separator { key, blocks } => {
                let old = match blocks {
                    Some(b) => doc.separators.insert(key.clone(), b),
                    None => doc.separators.remove(&key),
                };
                Ok(Applied { inverse: Step::Separator { key, blocks: old }, map: Map::Same, hint: None })
            }
            Step::ImageData { image, data, filled } => {
                let slot = doc.images.get_mut(image).ok_or(StepError::Missing)?;
                let old = std::mem::replace(slot, data);
                let was = if filled { !doc.filled.insert(image) } else { doc.filled.remove(&image) };
                Ok(Applied { inverse: Step::ImageData { image, data: old, filled: was }, map: Map::Same, hint: None })
            }
            Step::BuildingBlock { k, part } => {
                let g = doc.glossary.get_or_insert_with(Default::default);
                let live = |b: &crate::docx::BuildingBlock| b.state != crate::docx::PartState::Removed;
                let at = k as usize;
                let old = match part {
                    Some(new) if at == g.parts.len() => {
                        g.parts.push(*new);
                        None
                    }
                    Some(new) => Some(Box::new(std::mem::replace(g.parts.get_mut(at).ok_or(StepError::OutOfRange)?, *new))),
                    None if at + 1 == g.parts.len() => g.parts.pop().map(Box::new),
                    None => return Err(StepError::OutOfRange),
                };
                if let Some(o) = old.as_deref().filter(|o| live(o)) {
                    idx.removed_part(&o.body);
                }
                if let Some(now) = g.parts.get(at).filter(|b| live(b)) {
                    let body = now.body.clone();
                    idx.added_building_block(k, &body);
                }
                Ok(Applied { inverse: Step::BuildingBlock { k, part: old }, map: Map::Same, hint: None })
            }
            Step::HeaderPart { id, section, footer, slot, blocks } => {
                let sec = doc.sections.get_mut(section).ok_or(StepError::Missing)?;
                let refs = if footer { &mut sec.footers } else { &mut sec.headers };
                let r = refs.get_mut(slot).ok_or(StepError::OutOfRange)?;
                match blocks {
                    Some(_) if r.is_some() || doc.stories.contains_key(&id) => return Err(StepError::OutOfRange),
                    None if *r != Some(id) || !doc.stories.contains_key(&id) => return Err(StepError::Missing),
                    _ => {}
                }
                // Its `w:sectPr` loses the reference of a header it no longer has.
                doc.edited_sections.insert(section);
                match blocks {
                    Some(b) => {
                        *r = Some(id);
                        doc.stories.insert(id, b);
                        idx.added_part(doc, id);
                        Ok(Applied { inverse: Step::HeaderPart { id, section, footer, slot, blocks: None }, map: Map::Same, hint: None })
                    }
                    None => {
                        *r = None;
                        let b = doc.stories.remove(&id).ok_or(StepError::Missing)?;
                        idx.removed_part(&b);
                        Ok(Applied { inverse: Step::HeaderPart { id, section, footer, slot, blocks: Some(b) }, map: Map::Same, hint: None })
                    }
                }
            }
            Step::Control { id, control } => {
                let old = match control {
                    Some(c) => doc.controls.insert(id, c),
                    None => doc.controls.remove(&id),
                };
                Ok(Applied { inverse: Step::Control { id, control: old }, map: Map::Same, hint: None })
            }
            Step::DataPart { item_id, part } => {
                let at = doc.data_parts.iter().position(|p| p.item_id == item_id);
                let old = match (at, part) {
                    (Some(k), Some(p)) => Some(std::mem::replace(&mut doc.data_parts[k], p)),
                    (None, Some(p)) => {
                        if let Some(src) = &p.src {
                            doc.removed_data.retain(|r| r != src);
                        }
                        doc.data_parts.push(p);
                        None
                    }
                    (Some(k), None) => {
                        let p = doc.data_parts.remove(k);
                        doc.removed_data.extend(p.src.clone());
                        Some(p)
                    }
                    (None, None) => None,
                };
                Ok(Applied { inverse: Step::DataPart { item_id, part: old }, map: Map::Same, hint: None })
            }
            Step::Comment { id, after } => {
                let before = match after {
                    Some(c) => doc.comments.by_id.insert(id, c),
                    None => doc.comments.by_id.remove(&id),
                };
                Ok(Applied { inverse: Step::Comment { id, after: before }, map: Map::Same, hint: None })
            }
            Step::FieldViews { para, views } => {
                let p = idx.para_mut(doc, para).ok_or(StepError::Missing)?;
                let mut old = Vec::with_capacity(views.len());
                for (f, view) in views {
                    for i in p.inlines.iter_mut() {
                        if let Inline::FieldBegin { id, view: v, .. } = i
                            && *id == f
                        {
                            old.push((f, std::mem::replace(v, view)));
                        }
                    }
                }
                Ok(Applied { inverse: Step::FieldViews { para, views: old }, map: Map::Same, hint: para_hint(idx, para, 0) })
            }
            Step::AddImage { image, data: Some(data) } => {
                if doc.images.contains(image) {
                    return Err(StepError::OutOfRange);
                }
                doc.images.insert(image, data);
                Ok(Applied { inverse: Step::AddImage { image, data: None }, map: Map::Same, hint: None })
            }
            Step::AddImage { image, data: None } => {
                let data = Some(doc.images.remove(image).ok_or(StepError::Missing)?);
                Ok(Applied { inverse: Step::AddImage { image, data }, map: Map::Same, hint: None })
            }
            Step::Resolver { resolver, added } => {
                let resolver = std::mem::replace(&mut doc.resolver, resolver);
                in_step(doc);
                let added = std::mem::replace(&mut doc.added_lists, added);
                refresh_line_number_run(doc);
                Ok(Applied { inverse: Step::Resolver { resolver, added }, map: Map::Same, hint: None })
            }
            Step::Fonts { resolver, renames } => {
                let resolver = std::mem::replace(&mut doc.resolver, resolver);
                in_step(doc);
                let renames = std::mem::replace(&mut doc.font_renames, renames);
                refresh_line_number_run(doc);
                Ok(Applied { inverse: Step::Fonts { resolver, renames }, map: Map::Same, hint: None })
            }
            Step::DefaultTab(v) => {
                let old = std::mem::replace(&mut doc.settings.default_tab, v);
                Ok(Applied { inverse: Step::DefaultTab(old), map: Map::Same, hint: None })
            }
            Step::Hyphenation(h) => {
                let s = &mut doc.settings;
                let old = Hyphenating {
                    auto: s.auto_hyphenation,
                    zone: s.hyphenation_zone,
                    limit: s.consecutive_hyphen_limit,
                    caps: s.do_not_hyphenate_caps,
                };
                (s.auto_hyphenation, s.hyphenation_zone, s.consecutive_hyphen_limit, s.do_not_hyphenate_caps) = (h.auto, h.zone, h.limit, h.caps);
                Ok(Applied { inverse: Step::Hyphenation(old), map: Map::Same, hint: None })
            }
            Step::Protection(p) => {
                let old = std::mem::replace(&mut doc.settings.protection, p.map(|p| *p));
                Ok(Applied { inverse: Step::Protection(old.map(Box::new)), map: Map::Same, hint: None })
            }
            Step::Info(info) => {
                let (p, v) = *info;
                let old = (std::mem::replace(&mut doc.properties, p), std::mem::replace(&mut doc.settings.variables, v));
                Ok(Applied { inverse: Step::Info(Box::new(old)), map: Map::Same, hint: None })
            }
            Step::DefaultTableStyle(id) => {
                let old = std::mem::replace(&mut doc.settings.default_table_style, id);
                Ok(Applied { inverse: Step::DefaultTableStyle(old), map: Map::Same, hint: None })
            }
            Step::PageBackground(b) => {
                let old = PageBackground::of(doc);
                let PageBackground { colour, theme, fill, shown } = *b;
                (doc.page_colour, doc.page_colour_theme, doc.page_fill) = (colour, theme, fill);
                doc.settings.display_background_shape = shown;
                Ok(Applied { inverse: Step::PageBackground(Box::new(old)), map: Map::Same, hint: None })
            }
            Step::PageSettings(p) => {
                let old = PageSettings::of(&doc.settings);
                let s = &mut doc.settings;
                (s.even_and_odd, s.mirror_margins, s.gutter_at_top, s.book_fold) = (p.even_and_odd, p.mirror_margins, p.gutter_at_top, p.book_fold);
                (s.borders_do_not_surround_header, s.borders_do_not_surround_footer) = (p.no_surround_header, p.no_surround_footer);
                s.endnotes_sect_end = p.endnotes_sect_end;
                s.no_form_shading = p.no_form_shading;
                (s.auto_format_override, s.style_lock_theme, s.style_lock_style_set) =
                    (p.auto_format_override, p.style_lock_theme, p.style_lock_style_set);
                Ok(Applied { inverse: Step::PageSettings(old), map: Map::Same, hint: None })
            }
            Step::SectionBreak { section, back } => {
                let n = doc.sections.len();
                if doc.section_sources.len() != n {
                    doc.section_sources = vec![1; n];
                }
                let inverse = match back {
                    None => {
                        if section + 1 >= n {
                            return Err(StepError::OutOfRange);
                        }
                        let mut gone = doc.sections.remove(section);
                        let blocks = std::mem::take(&mut gone.blocks);
                        let back = SectionBack { blocks: blocks.len(), section: gone, sources: doc.section_sources.remove(section), edited: false };
                        doc.sections[section].blocks.splice(0..0, blocks);
                        doc.section_sources[section] += back.sources;
                        let edited = doc.edited_sections.remove(&section);
                        doc.edited_sections = doc.edited_sections.iter().map(|&k| if k > section { k - 1 } else { k }).collect();
                        Step::SectionBreak { section, back: Some(Box::new(SectionBack { edited, ..back })) }
                    }
                    Some(back) => {
                        let SectionBack { blocks, section: mut s, sources, edited } = *back;
                        let next = doc.sections.get_mut(section).ok_or(StepError::Missing)?;
                        if blocks >= next.blocks.len() || doc.section_sources[section] < sources {
                            return Err(StepError::OutOfRange);
                        }
                        s.blocks = next.blocks.drain(..blocks).collect();
                        doc.section_sources[section] -= sources;
                        doc.section_sources.insert(section, sources);
                        doc.sections.insert(section, s);
                        doc.edited_sections = doc.edited_sections.iter().map(|&k| if k >= section { k + 1 } else { k }).collect();
                        if edited {
                            doc.edited_sections.insert(section);
                        }
                        Step::SectionBreak { section, back: None }
                    }
                };
                let parts = idx.parts().to_vec();
                let flow = idx.flow;
                *idx = DocIndex::build_with_parts(doc, &parts);
                idx.flow = flow;
                Ok(Applied { inverse, map: Map::Same, hint: None })
            }
            Step::Sections { section, split } => {
                let n = doc.sections.len();
                if doc.section_sources.len() != n {
                    doc.section_sources = vec![1; n];
                }
                let inverse = match *split {
                    Split::Apart { backs, stories } => split_sections(doc, section, backs, stories)?,
                    Split::Joined { count, stories } => join_sections(doc, section, count, stories)?,
                };
                let parts = idx.parts().to_vec();
                let flow = idx.flow;
                *idx = DocIndex::build_with_parts(doc, &parts);
                idx.flow = flow;
                Ok(Applied { inverse: Step::Sections { section, split: Box::new(inverse) }, map: Map::Same, hint: None })
            }
            Step::SectionPage { section, page, change } => {
                let s = doc.sections.get_mut(section).ok_or(StepError::Missing)?;
                let old = Box::new(s.page());
                s.set_page(*page);
                let old_change = std::mem::replace(&mut s.change, change);
                doc.edited_sections.insert(section);
                Ok(Applied { inverse: Step::SectionPage { section, page: old, change: old_change }, map: Map::Same, hint: None })
            }
            Step::TableProps { table, props } => {
                let t = idx.table_mut(doc, table).ok_or(StepError::Missing)?;
                let old = props.put(t);
                idx.table_slot(table).ok_or(StepError::Missing)?;
                let hint = table_hint(idx, table, None);
                Ok(Applied { inverse: Step::TableProps { table, props: old }, map: Map::Same, hint })
            }
            Step::Rows { table, range, with, grid } => {
                let t = idx.table_mut(doc, table).ok_or(StepError::Missing)?;
                if range.start > range.end || range.end > t.rows.len() {
                    return Err(StepError::OutOfRange);
                }
                let added = with.len();
                let single = range.len() == 1 && added == 1;
                let (removed, old_grid) = put_rows(t, range.clone(), with, grid);
                let (mut paras, mut tables) = (HashSet::new(), HashSet::new());
                let mut kept = HashSet::new();
                for r in &t.rows[range.start..range.start + added] {
                    r.cells.iter().flat_map(|c| c.blocks.iter()).for_each(|b| ids(b, &mut kept, &mut HashSet::new()));
                }
                for r in &removed {
                    r.cells.iter().flat_map(|c| c.blocks.iter()).for_each(|b| ids(b, &mut paras, &mut tables));
                }
                paras.retain(|p| !kept.contains(p));
                idx.replaced_rows(doc, table, range.start, added, &removed);
                let slot = idx.table_slot(table).ok_or(StepError::Missing)?;
                let fallback = first_position(doc, idx, slot.container, slot.block);
                let hint = table_hint(idx, table, single.then_some(range.start));
                Ok(Applied {
                    inverse: Step::Rows { table, range: range.start..range.start + added, with: removed, grid: old_grid },
                    map: Map::Removed { paras, tables, rows: Some((table, range.clone(), added as isize - range.len() as isize)), fallback },
                    hint,
                })
            }
        }
    }
}

/// Puts `with` in place of `t`'s rows in `range` and `grid` (when given) as its grid, every width rounded to a twip:
/// the rows and the grid it had.
pub(super) fn put_rows(t: &mut crate::docx::Table, range: Range<usize>, with: Vec<Row>, grid: Option<Vec<f64>>) -> (Vec<Row>, Option<Vec<f64>>) {
    let twip = |w: f64| (w * 20.0).round() / 20.0;
    let with = with.into_iter().map(|mut r| {
        for c in &mut r.cells {
            if let Some(crate::docx::props::Width::Dxa(w)) = &mut c.width {
                *w = twip(*w);
            }
        }
        r
    });
    let removed = t.rows.splice(range, with).collect();
    (removed, grid.map(|g| std::mem::replace(&mut t.grid, g.into_iter().map(twip).collect())))
}

/// Whether `container` is in a header or footer, a text box in one included.
pub(super) fn in_header_footer(idx: &DocIndex, container: Container) -> bool {
    matches!(idx.host(idx.story(container)), super::position::StoryId::HeaderFooter(_))
}

/// Whether `s` changes header or footer text and nothing else.
pub(super) fn changes_header_footer(idx: &DocIndex, s: &Step) -> bool {
    let container = match s {
        Step::Text { para, .. }
        | Step::Split { para, .. }
        | Step::Join { first: para, .. }
        | Step::Para { para, .. }
        | Step::Restyle { para, .. }
        | Step::FieldViews { para, .. } => idx.para_slot(*para).map(|s| s.container),
        Step::Blocks { container, .. } => Some(*container),
        Step::Rows { table, .. } | Step::TableProps { table, .. } => idx.table_slot(*table).map(|s| s.container),
        _ => None,
    };
    container.is_some_and(|c| in_header_footer(idx, c))
}

/// Table `table` changed; `row`, when it is a section's own table, the one row of it that did.
pub(super) fn table_hint(idx: &DocIndex, table: TableId, row: Option<usize>) -> Option<Change> {
    let slot = idx.table_slot(table)?;
    // The row is the table's own only when the table is the block the hint names; in a text box or a cell it isn't.
    let own = matches!(slot.container, Container::Section(_))
        || matches!(slot.container, Container::Story(super::position::StoryId::BuildingBlock(k)) if idx.flow == Some(k));
    idx.top_block(slot.container, slot.block).map(|(section, b, r)| Change {
        section,
        first: b,
        old_end: b + 1,
        new_end: b + 1,
        row: r.or(row.filter(|_| own)),
    })
}

fn block_id(b: &Block) -> u64 {
    match b {
        Block::Para(p) => p.id,
        Block::Table(t) => t.id,
    }
}

fn ids(b: &Block, paras: &mut HashSet<ParaId>, tables: &mut HashSet<TableId>) {
    match b {
        Block::Para(p) => {
            paras.insert(ParaId(p.id));
        }
        Block::Table(t) => {
            tables.insert(TableId(t.id));
            for c in t.rows.iter().flat_map(|r| r.cells.iter()) {
                c.blocks.iter().for_each(|b| ids(b, paras, tables));
            }
        }
    }
}

pub fn first_position(doc: &Document, idx: &DocIndex, container: Container, from: usize) -> Option<Position> {
    let blocks = idx.blocks(doc, container)?;
    let story = idx.story(container);
    fn first_in(b: &Block) -> Option<ParaId> {
        match b {
            Block::Para(p) => Some(ParaId(p.id)),
            Block::Table(t) => t.rows.iter().flat_map(|r| r.cells.iter()).flat_map(|c| c.blocks.iter()).find_map(first_in),
        }
    }
    if let Some(p) = blocks[from.min(blocks.len())..].iter().find_map(first_in) {
        return Some(Position::text(story, p, Unit(0)));
    }
    blocks[..from.min(blocks.len())].iter().rev().find_map(|b| match b {
        Block::Para(p) => Some(Position::text(story, ParaId(p.id), para_len(p))),
        Block::Table(_) => first_in(b).map(|p| Position::text(story, p, Unit(0))),
    })
}

impl Split {
    /// The blocks of the stories it puts in.
    pub fn stories(&self) -> impl Iterator<Item = &[Block]> {
        let v = match self {
            Split::Apart { stories, .. } => &stories[..],
            Split::Joined { .. } => &[],
        };
        v.iter().map(|(_, b)| &b[..])
    }
}

// Section splits

fn split_sections(
    doc: &mut Document,
    section: usize,
    backs: Vec<SectionBack>,
    stories: Vec<(HeaderFooterId, Vec<Block>)>,
) -> Result<Split, StepError> {
    let (blocks, sources) = backs.iter().fold((0, 0), |(b, s), x| (b + x.blocks, s + x.sources));
    let next = doc.sections.get_mut(section).ok_or(StepError::Missing)?;
    if backs.is_empty()
        || blocks >= next.blocks.len()
        || doc.section_sources[section] < sources
        || stories.iter().any(|(k, _)| doc.stories.contains_key(k))
    {
        return Err(StepError::OutOfRange);
    }
    let count = backs.len();
    let mut taken = next.blocks.drain(..blocks);
    let mut made = Vec::with_capacity(count);
    let mut made_sources = Vec::with_capacity(count);
    let mut edited = Vec::new();
    for (k, b) in backs.into_iter().enumerate() {
        let mut s = b.section;
        s.blocks = taken.by_ref().take(b.blocks).collect();
        made.push(s);
        made_sources.push(b.sources);
        if b.edited {
            edited.push(section + k);
        }
    }
    drop(taken);
    doc.section_sources[section] -= sources;
    doc.section_sources.splice(section..section, made_sources);
    doc.sections.splice(section..section, made);
    doc.edited_sections = doc.edited_sections.iter().map(|&k| if k >= section { k + count } else { k }).chain(edited).collect();
    let ids = stories.iter().map(|(k, _)| *k).collect();
    doc.stories.extend(stories);
    Ok(Split::Joined { count, stories: ids })
}

fn join_sections(doc: &mut Document, section: usize, count: usize, ids: Vec<HeaderFooterId>) -> Result<Split, StepError> {
    let end = section + count;
    // Into a section with blocks: splitting them back leaves it some.
    if count == 0 || end >= doc.sections.len() || doc.sections[end].blocks.is_empty() || ids.iter().any(|k| !doc.stories.contains_key(k)) {
        return Err(StepError::OutOfRange);
    }
    let mut blocks = Vec::new();
    let mut backs = Vec::with_capacity(count);
    for (k, (mut s, sources)) in doc.sections.drain(section..end).zip(doc.section_sources.drain(section..end)).enumerate() {
        let b = std::mem::take(&mut s.blocks);
        backs.push(SectionBack { blocks: b.len(), section: s, sources, edited: doc.edited_sections.contains(&(section + k)) });
        blocks.extend(b);
    }
    doc.sections[section].blocks.splice(0..0, blocks);
    doc.section_sources[section] += backs.iter().map(|b| b.sources).sum::<usize>();
    doc.edited_sections = doc.edited_sections.iter().filter(|&&k| k < section || k >= end).map(|&k| if k >= end { k - count } else { k }).collect();
    let stories = ids.into_iter().filter_map(|k| doc.stories.remove(&k).map(|b| (k, b))).collect();
    Ok(Split::Apart { backs, stories })
}

/// In a debug build, the document's typed definitions brought in step with its resolver, which checks they agree.
fn in_step(doc: &Document) {
    if cfg!(debug_assertions)
        && let Some(r) = &doc.resolver
    {
        r.typed();
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn sections_join_only_into_one_with_blocks_a_split_gives_back() {
        let mut doc = crate::docx::read(&crate::editing::tests::blank_body("<w:p/>")).expect("reads");
        let empty = crate::docx::Section { blocks: Vec::new(), ..doc.sections[0].clone() };
        doc.sections.push(empty);
        doc.section_sources = vec![1, 0];
        assert!(super::join_sections(&mut doc, 0, 1, Vec::new()).is_err());
        assert_eq!((doc.sections.len(), doc.sections[0].blocks.len()), (2, 1), "as it was");
    }
}
