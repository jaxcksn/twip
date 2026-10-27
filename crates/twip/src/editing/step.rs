//! Primitive mutations: each applies to the document, has an exact inverse, maps positions and yields a layout hint.
//! Commands are built from steps only, so undo, redo, selection mapping and incremental layout treat
//! every edit alike. Steps never re-read the document; untouched blocks keep their ids.

use std::collections::HashSet;
use std::ops::Range;
use std::rc::Rc;

use crate::docx::{Block, Document, Inline, Paragraph, Row, Run};
use crate::layout::{Change, ParaId, TableId, Unit};

use super::index::{Container, DocIndex};
use super::position::{Place, Position};

/// Where a [`Step::Text`] range starts and ends among the zero-unit inlines at its edges, which it otherwise leaves
/// (past those at its start, before those at its end): it takes `lead` more at its start and `trail` more at its end,
/// and starts `under` before the last of those at its start. An inverse undoes exactly what its step did: it takes
/// the markers the step inserted, and puts deleted content back before the markers that were after it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Edges {
    pub lead: u32,
    pub trail: u32,
    pub under: u32,
}

#[derive(Clone, Debug)]
pub enum Step {
    /// Replace units `range` of `para` with `with` (typing, deletion, run formatting). A range leaves the zero-unit
    /// inlines on its edges (control and field markers, anchors) where they are, unless `edges` says otherwise.
    Text { para: ParaId, range: Range<Unit>, with: Vec<Inline>, edges: Edges },
    /// Split `para` at `at`; the units after it move to `second`, inserted right after it. `second` comes with no
    /// inlines, except as a join's inverse, which gives it exactly the inlines it had.
    Split { para: ParaId, at: Unit, second: Paragraph },
    /// Join `second`, the paragraph right after `first` in its container, onto `first`.
    Join { first: ParaId, second: ParaId },
    /// Replace blocks `range` of a container (insert or delete paragraphs and tables, replace a nested table).
    Blocks { container: Container, range: Range<usize>, with: Vec<Block> },
    /// Replace rows `range` of a table, and its grid when given (row and column operations, merges, resizes).
    Rows { table: TableId, range: Range<usize>, with: Vec<Row>, grid: Option<Vec<f64>> },
    /// Replace a paragraph's properties, its mark's formatting and its list label (paragraph formatting, numbering).
    Para { para: ParaId, style: Rc<crate::docx::ParaStyle>, mark: crate::docx::Run, list: Option<crate::docx::ListLabel> },
    /// Replace a paragraph's inlines with the same content in other runs (formatting resolved again): units, and so
    /// positions, don't change.
    Restyle { para: ParaId, inlines: Vec<Inline> },
    /// Replace a table's preferred width (a border drag that changes the table's width).
    TableWidth { table: TableId, width: crate::docx::props::Width },
    /// An image's data replaced, and whether it fills a placeholder.
    ImageData { index: usize, data: Vec<u8>, filled: bool },
    /// A header or footer part added (`blocks` given) to a section's reference `slot` (0 default, 1 first page, 2 even
    /// pages), or taken out again.
    HeaderPart { name: String, section: usize, footer: bool, slot: usize, blocks: Option<Vec<Block>> },
    /// A table's own properties replaced (Table Properties and Table Design commands).
    TableProps { table: TableId, props: TableProps },
    /// Add (`blocks` given) or remove a footnote or endnote: inserting a note, deleting its reference mark.
    Note { kind: crate::docx::NoteKind, id: i64, blocks: Option<Vec<Block>> },
    /// A section's page replaced (Page Setup, margins, orientation, size, columns, different first page).
    SectionPage { section: usize, page: crate::docx::SectionPage },
    /// A content control's properties replaced, or the control added (`control` given) or removed.
    /// Its markers and memberships move with the content holding them; layout doesn't change.
    Control { id: crate::docx::ControlId, control: Option<crate::docx::Control> },
    /// A data part replaced, added (`part` given, none with its item id) or removed. Layout doesn't
    /// change.
    DataPart { item_id: String, part: Option<crate::docx::DataPart> },
}

/// What [`Step::TableProps`] replaces: the table's style and look, width, alignment and indent, and its borders as
/// resolved.
#[derive(Clone, Debug, PartialEq)]
pub struct TableProps {
    pub style_id: Option<String>,
    pub look: Option<crate::docx::props::Look>,
    pub width: crate::docx::props::Width,
    pub jc: crate::docx::props::Jc,
    pub ind: f64,
    pub borders: [Option<crate::docx::props::Border>; 6],
    /// Rows and columns per style band (`w:tblStyleRowBandSize`, `w:tblStyleColBandSize`).
    pub bands: (i64, i64),
}

impl Step {
    /// How many of `images`' pictures the content this step puts in holds (a split or join moves content: none).
    pub fn pictures_of(&self, images: &HashSet<usize>) -> i64 {
        self.pictures_where(&|k| images.contains(&k))
    }

    /// How many pictures whose image `images` takes the content this step puts in holds.
    pub fn pictures_where(&self, images: &dyn Fn(usize) -> bool) -> i64 {
        fn inlines(v: &[Inline], images: &dyn Fn(usize) -> bool) -> i64 {
            v.iter()
                .filter(|i| match i {
                    Inline::Picture(p, _) => p.image.is_some_and(images),
                    Inline::Float(f, _) => f.image.is_some_and(images),
                    _ => false,
                })
                .count() as i64
        }
        fn blocks(v: &[Block], images: &dyn Fn(usize) -> bool) -> i64 {
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
            _ => 0,
        }
    }
}

impl TableProps {
    pub fn of(t: &crate::docx::Table) -> TableProps {
        TableProps { style_id: t.style_id.clone(), look: t.look, width: t.width, jc: t.jc, ind: t.ind, borders: t.borders.clone(), bands: t.bands }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum StepError {
    /// The step names content that isn't in the document (a stale step: a bug in the command that built it).
    Missing,
    /// The range or offset isn't inside its paragraph, container or table.
    OutOfRange,
}

/// What applying a step did: its exact inverse, how positions move, and layout's hint (`None`: layout compares).
pub struct Applied {
    pub inverse: Step,
    pub map: Map,
    pub hint: Option<Change>,
}

/// How positions move through an applied step.
#[derive(Clone, Debug)]
pub enum Map {
    Text {
        para: ParaId,
        start: Unit,
        old_end: Unit,
        new_end: Unit,
    },
    Split {
        para: ParaId,
        at: Unit,
        len: Unit,
        second: ParaId,
    },
    Join {
        first: ParaId,
        second: ParaId,
        offset: Unit,
    },
    /// Paragraphs and tables removed; positions in them go to `fallback`. Row-end marks of `table` from `rows.end` on
    /// shift by `shift`.
    Removed {
        paras: HashSet<ParaId>,
        tables: HashSet<TableId>,
        rows: Option<(TableId, Range<usize>, isize)>,
        fallback: Option<Position>,
    },
    /// Positions don't move (a property change).
    Same,
}

impl Map {
    /// `pos` after the step.
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

// ---------------- inlines by units ----------------

pub fn para_len(p: &Paragraph) -> Unit {
    Unit(p.inlines.iter().map(Inline::units).sum::<usize>() as u32)
}

/// The formatting run an inline carries.
pub fn run_of(i: &Inline) -> Option<&Run> {
    match i {
        Inline::Text(_, r)
        | Inline::Tab(r)
        | Inline::PTab(_, r)
        | Inline::Break(_, r)
        | Inline::Picture(_, r)
        | Inline::Float(_, r)
        | Inline::NoteMark(_, _, r) => Some(r),
        Inline::Sym { run, .. }
        | Inline::NoteRef { run, .. }
        | Inline::Separator { run, .. }
        | Inline::FieldBegin { run, .. }
        | Inline::FieldSep { run, .. }
        | Inline::FieldEnd { run, .. } => Some(run),
        Inline::Bookmark(_) | Inline::ControlStart(_) | Inline::ControlEnd(_) => None,
    }
}

/// Splits a text inline straddling unit `u` so an inline boundary falls there. Returns the index of the first inline
/// at or after `u`: past zero-unit inlines at `u` when `after_markers`, before them otherwise.
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

/// Replaces units `range` of `p` with `with`; returns the removed inlines. Inserted text joins an adjacent text in the
/// same run, and texts a cut split rejoin, so an inverse restores the original inlines exactly. (Typing after deleting
/// a paragraph's text keeps that text's formatting through the editor's pending format, not through the model.)
/// Also returns how many zero-unit inlines (not texts) follow the inserted ones at their place (see [`Edges`]).
fn splice(p: &mut Paragraph, range: Range<Unit>, edges: Edges, with: Vec<Inline>) -> (Vec<Inline>, u32) {
    // Edges count markers, not the empty texts that may sit among them.
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
            // Only the last inserted inline may join the text after it, or later ones would land behind that text.
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
    // Texts the cut split, now adjacent again in the same run, rejoin: an inverse step restores the original inlines.
    merge_at(&mut p.inlines, at);
    merge_at(&mut p.inlines, a);
    // Empty texts left by the cut go, unless they are all the paragraph has.
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

/// Joins inlines `k - 1` and `k` when both are text in the same run.
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

// ---------------- hints ----------------

/// Layout's hint for a change inside top-level block `block` of a container.
fn hint_in(idx: &DocIndex, container: Container, block: usize, added: isize) -> Option<Change> {
    let (section, b, row) = idx.top_block(container, block)?;
    match (container, row) {
        (Container::Section(_), _) => Some(Change { section, first: b, old_end: b + 1, new_end: (b as isize + 1 + added) as usize, row: None }),
        (_, row) => Some(Change { section, first: b, old_end: b + 1, new_end: b + 1, row }),
    }
}

pub(super) fn para_hint(idx: &DocIndex, para: ParaId, added: isize) -> Option<Change> {
    let s = idx.para_slot(para)?;
    hint_in(idx, s.container, s.block, added)
}

// ---------------- apply ----------------

impl Step {
    pub fn apply(self, doc: &mut Document, idx: &mut DocIndex) -> Result<Applied, StepError> {
        match self {
            Step::Text { para, range, with, edges } => {
                let p = idx.para_mut(doc, para).ok_or(StepError::Missing)?;
                if range.start > range.end || range.end > para_len(p) {
                    return Err(StepError::OutOfRange);
                }
                let added = units_of(&with);
                // The zero-unit inlines `with` begins and ends with: its inverse takes them too.
                let marker = |i: &&Inline| i.units() == 0 && !matches!(i, Inline::Text(..));
                let lead = with.iter().take_while(marker).count();
                let trail = if lead == with.len() { 0 } else { with.iter().rev().take_while(marker).count() };
                let (removed, after) = splice(p, range.clone(), edges, with);
                let under = if added == 0 { after } else { 0 };
                let new_end = Unit(range.start.0 + added);
                idx.replaced_inlines(doc, para, &removed);
                Ok(Applied {
                    inverse: Step::Text {
                        para,
                        range: range.start..new_end,
                        with: removed,
                        edges: Edges { lead: lead as u32, trail: trail as u32, under },
                    },
                    map: Map::Text { para, start: range.start, old_end: range.end, new_end },
                    hint: para_hint(idx, para, 0),
                })
            }
            Step::Split { para, at, mut second } => {
                let slot = idx.para_slot(para).ok_or(StepError::Missing)?;
                let p = idx.para_mut(doc, para).ok_or(StepError::Missing)?;
                let len = para_len(p);
                if at > len {
                    return Err(StepError::OutOfRange);
                }
                // Everything from the cut on, with the zero-unit inlines at the paragraph's end (a control's end there
                // goes with its content); those at the cut stay with the first part.
                let tail: Vec<Inline> = if at < len {
                    let k = cut(&mut p.inlines, at, true);
                    p.inlines.drain(k..).collect()
                } else {
                    Vec::new()
                };
                // Its fields and controls are the new paragraph's now (indexed when it is added below).
                idx.moved_inlines(para, &tail);
                if !second.inlines.is_empty() {
                    // Undoing a join: the second paragraph gets exactly what it had; zero-unit inlines of its own (anchors,
                    // bookmarks, a control's end at its end) the cut left at the first's end are taken back.
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
                let Block::Para(tail) = blocks.remove(b.block) else { return Err(StepError::Missing) };
                // The inverse split gets the exact inlines back (see `Split`).
                let inlines = tail.inlines.clone();
                let p = match blocks.get_mut(a.block) {
                    Some(Block::Para(p)) => p,
                    _ => return Err(StepError::Missing),
                };
                let at = para_len(p);
                splice(p, at..at, Edges::default(), inlines);
                idx.replaced_blocks(doc, a.container, &[Block::Para(tail.clone())]);
                // The joined paragraph's floats moved paragraphs.
                idx.replaced_inlines(doc, first, &[]);
                let hint = match a.container {
                    Container::Section(_) => idx.top_block(a.container, a.block).map(|(section, f, _)| Change {
                        section,
                        first: f,
                        old_end: f + 2,
                        new_end: f + 1,
                        row: None,
                    }),
                    _ => hint_in(idx, a.container, a.block, 0),
                };
                Ok(Applied { inverse: Step::Split { para: first, at, second: tail }, map: Map::Join { first, second, offset: at }, hint })
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
                let hint = match container {
                    Container::Section(_) => idx.top_block(container, range.start).map(|(section, f, _)| Change {
                        section,
                        first: f,
                        old_end: f + range.len(),
                        new_end: f + added,
                        row: None,
                    }),
                    _ => hint_in(idx, container, 0, 0),
                };
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
                let old = std::mem::replace(&mut p.inlines, inlines);
                // A float taken out or put back (deleting a floating object) is indexed again.
                idx.replaced_inlines(doc, para, &old);
                Ok(Applied { inverse: Step::Restyle { para, inlines: old }, map: Map::Same, hint: para_hint(idx, para, 0) })
            }
            Step::Para { para, style, mark, list } => {
                let p = idx.para_mut(doc, para).ok_or(StepError::Missing)?;
                let old_style = std::mem::replace(&mut p.style, style);
                let old_mark = std::mem::replace(&mut p.mark, mark);
                let old_list = std::mem::replace(&mut p.list, list);
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
                // Notes have no layout hint: layout compares (as for typing in a note).
                Ok(Applied {
                    inverse: Step::Note { kind, id, blocks: old },
                    map: Map::Removed { paras, tables, rows: None, fallback: None },
                    hint: None,
                })
            }
            Step::TableWidth { table, width } => {
                let t = idx.table_mut(doc, table).ok_or(StepError::Missing)?;
                let old = std::mem::replace(&mut t.width, width);
                let slot = idx.table_slot(table).ok_or(StepError::Missing)?;
                let hint = idx.top_block(slot.container, slot.block).map(|(section, b, row)| Change {
                    section,
                    first: b,
                    old_end: b + 1,
                    new_end: b + 1,
                    row,
                });
                Ok(Applied { inverse: Step::TableWidth { table, width: old }, map: Map::Same, hint })
            }
            Step::ImageData { index, data, filled } => {
                let slot = doc.images.get_mut(index).ok_or(StepError::Missing)?;
                let old = std::mem::replace(slot, data);
                let was = if filled { !doc.filled.insert(index) } else { doc.filled.remove(&index) };
                Ok(Applied { inverse: Step::ImageData { index, data: old, filled: was }, map: Map::Same, hint: None })
            }
            Step::HeaderPart { name, section, footer, slot, blocks } => {
                let sec = doc.sections.get_mut(section).ok_or(StepError::Missing)?;
                let refs = if footer { &mut sec.footers } else { &mut sec.headers };
                let r = refs.get_mut(slot).ok_or(StepError::OutOfRange)?;
                match blocks {
                    Some(b) => {
                        if r.is_some() || doc.stories.contains_key(&name) {
                            return Err(StepError::OutOfRange);
                        }
                        *r = Some(name.clone());
                        doc.stories.insert(name.clone(), b);
                        idx.added_part(doc, &name);
                        Ok(Applied { inverse: Step::HeaderPart { name, section, footer, slot, blocks: None }, map: Map::Same, hint: None })
                    }
                    None => {
                        if r.as_deref() != Some(name.as_str()) {
                            return Err(StepError::Missing);
                        }
                        *r = None;
                        let b = doc.stories.remove(&name).ok_or(StepError::Missing)?;
                        idx.removed_part(&b);
                        Ok(Applied { inverse: Step::HeaderPart { name, section, footer, slot, blocks: Some(b) }, map: Map::Same, hint: None })
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
                        // A part read from the package and added back by an undo isn't removed on save any more.
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
            Step::SectionPage { section, page } => {
                let s = doc.sections.get_mut(section).ok_or(StepError::Missing)?;
                let old = s.page();
                s.set_page(page);
                doc.edited_sections.insert(section);
                Ok(Applied { inverse: Step::SectionPage { section, page: old }, map: Map::Same, hint: None })
            }
            Step::TableProps { table, props } => {
                let t = idx.table_mut(doc, table).ok_or(StepError::Missing)?;
                let old = TableProps::of(t);
                (t.style_id, t.look, t.width, t.jc, t.ind, t.borders, t.bands) =
                    (props.style_id, props.look, props.width, props.jc, props.ind, props.borders, props.bands);
                let slot = idx.table_slot(table).ok_or(StepError::Missing)?;
                let hint = idx.top_block(slot.container, slot.block).map(|(section, b, row)| Change {
                    section,
                    first: b,
                    old_end: b + 1,
                    new_end: b + 1,
                    row,
                });
                Ok(Applied { inverse: Step::TableProps { table, props: old }, map: Map::Same, hint })
            }
            Step::Rows { table, range, with, grid } => {
                let t = idx.table_mut(doc, table).ok_or(StepError::Missing)?;
                if range.start > range.end || range.end > t.rows.len() {
                    return Err(StepError::OutOfRange);
                }
                let added = with.len();
                let single = range.len() == 1 && added == 1;
                // Widths at the file's precision (whole twips), so what saving and reading back gives is what is kept.
                let twip = |w: f64| (w * 20.0).round() / 20.0;
                let with: Vec<Row> = with
                    .into_iter()
                    .map(|mut r| {
                        for c in &mut r.cells {
                            if let Some(crate::docx::props::Width::Dxa(w)) = &mut c.width {
                                *w = twip(*w);
                            }
                        }
                        r
                    })
                    .collect();
                let grid = grid.map(|g| g.into_iter().map(twip).collect::<Vec<f64>>());
                let removed: Vec<Row> = t.rows.splice(range.clone(), with).collect();
                let old_grid = grid.map(|g| std::mem::replace(&mut t.grid, g));
                let (mut paras, mut tables) = (HashSet::new(), HashSet::new());
                // Content of replaced rows that survives (same paragraph ids in the new rows) keeps its positions.
                let mut kept = HashSet::new();
                for r in &t.rows[range.start..range.start + added] {
                    r.cells.iter().flat_map(|c| c.blocks.iter()).for_each(|b| ids(b, &mut kept, &mut HashSet::new()));
                }
                for r in &removed {
                    r.cells.iter().flat_map(|c| c.blocks.iter()).for_each(|b| ids(b, &mut paras, &mut tables));
                }
                paras.retain(|p| !kept.contains(p));
                idx.replaced_rows(doc, table, &removed);
                let slot = idx.table_slot(table).ok_or(StepError::Missing)?;
                let fallback = first_position(doc, idx, slot.container, slot.block);
                let hint = idx.top_block(slot.container, slot.block).map(|(section, b, row)| Change {
                    section,
                    first: b,
                    old_end: b + 1,
                    new_end: b + 1,
                    // A nested table's rows are all inside one row of the outermost table.
                    row: match row {
                        Some(r) => Some(r),
                        None => single.then_some(range.start),
                    },
                });
                Ok(Applied {
                    inverse: Step::Rows { table, range: range.start..range.start + added, with: removed, grid: old_grid },
                    map: Map::Removed { paras, tables, rows: Some((table, range.clone(), added as isize - range.len() as isize)), fallback },
                    hint,
                })
            }
        }
    }
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

/// The first text position in `container` from block `from` on (descending into tables), else the last before it.
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
