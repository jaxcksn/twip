//! Revision index

use std::ops::Range;
use std::rc::Rc;

use crate::docx::review::{AuthorId, CellMarkKind, MarkKind, RangeKind, RevisionId, RevisionMark};
use crate::docx::{Block, Document, Inline, Paragraph, Run, Table};
use crate::layout::{FloatId, ParaId, TableId, Unit};

use super::super::index::DocIndex;
use super::super::position::{Position, StoryId};
use super::super::step::para_len;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RevisionKind {
    Insert,
    Delete,
    MovedFrom,
    MovedTo,
    RowInsert,
    RowDelete,
    Property,
    ParagraphProperty,
    TableProperty,
    SectionProperty,
    CellInsertion,
    CellDeletion,
    CellMerge,
}

impl RevisionKind {
    /// Whether it is a formatting change (Show Markup's Formatting), not an insertion or deletion.
    pub fn is_formatting(self) -> bool {
        matches!(
            self,
            RevisionKind::Property
                | RevisionKind::ParagraphProperty
                | RevisionKind::TableProperty
                | RevisionKind::SectionProperty
                | RevisionKind::CellMerge
        )
    }

    pub fn name(self) -> &'static str {
        match self {
            RevisionKind::Insert => "Insert",
            RevisionKind::Delete => "Delete",
            RevisionKind::MovedFrom => "MovedFrom",
            RevisionKind::MovedTo => "MovedTo",
            RevisionKind::RowInsert => "RowInsert",
            RevisionKind::RowDelete => "RowDelete",
            RevisionKind::Property => "Property",
            RevisionKind::ParagraphProperty => "ParagraphProperty",
            RevisionKind::TableProperty => "TableProperty",
            RevisionKind::SectionProperty => "SectionProperty",
            RevisionKind::CellInsertion => "CellInsertion",
            RevisionKind::CellDeletion => "CellDeletion",
            RevisionKind::CellMerge => "CellMerge",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Revision {
    pub story: StoryId,
    pub start: Position,
    pub end: Position,
    pub kind: RevisionKind,
    pub author: AuthorId,
    pub ids: Vec<RevisionId>,
    pub date: Option<crate::docx::review::Timestamp>,
}

impl Revision {
    /// Whether Show Markup shows it: its kind and reviewer.
    pub fn shown(&self, show: &crate::docx::review::ShowMarkup) -> bool {
        show.shows_change(self.kind.is_formatting(), self.author)
    }
}

/// Revisions and comment threads by kind, as the reviewing pane's summary counts them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    /// Inserted text, rows and cells.
    pub insertions: u32,
    /// Deleted text, rows and cells.
    pub deletions: u32,
    /// Both halves of each move.
    pub moves: u32,
    /// Changes of text, paragraph, table, section and cell properties, and cell merges.
    pub formatting: u32,
    /// Comment threads.
    pub comments: u32,
}

impl Counts {
    /// The revisions: every kind but comments.
    pub fn revisions(&self) -> u32 {
        self.insertions + self.deletions + self.moves + self.formatting
    }

    /// Counts `n` revisions of `kind`.
    pub fn add(&mut self, kind: RevisionKind, n: u32) {
        let count = match kind {
            RevisionKind::Insert | RevisionKind::RowInsert | RevisionKind::CellInsertion => &mut self.insertions,
            RevisionKind::Delete | RevisionKind::RowDelete | RevisionKind::CellDeletion => &mut self.deletions,
            RevisionKind::MovedFrom | RevisionKind::MovedTo => &mut self.moves,
            RevisionKind::Property
            | RevisionKind::ParagraphProperty
            | RevisionKind::TableProperty
            | RevisionKind::SectionProperty
            | RevisionKind::CellMerge => &mut self.formatting,
        };
        *count += n;
    }
}

const CHANNELS: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Channel {
    Ins,
    Del,
    Row,
    Fmt,
    Para,
    Table,
    Cell,
    Section,
}

/// What a block-level change is a change of.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum Spot {
    Run,
    Mark(ParaId),
    Para(ParaId),
    Row(TableId, usize),
    Cell(TableId, usize, usize),
    Table(TableId),
    Section(usize),
}

#[derive(Clone, Debug)]
pub(super) struct Item {
    pub channel: Channel,
    pub kind: RevisionKind,
    pub mark: Rc<RevisionMark>,
    pub story: StoryId,
    pub start: Position,
    pub end: Position,
    pub run: Option<(ParaId, Range<u32>)>,
    pub spot: Spot,
    pub listed: bool,
    seq: usize,
    /// Which of its paragraph's walked runs (the mark's included) it is on.
    pub run_no: u32,
}

#[derive(Debug, Default)]
pub(super) struct Found {
    pub items: Vec<Item>,
    pub move_ranges: Vec<(Rc<str>, i64)>,
    pub stories: Vec<StoryId>,
}

#[derive(Debug, Default)]
pub struct List {
    at: Option<u64>,
    revisions: Vec<Revision>,
    stories: Vec<StoryId>,
}

impl List {
    pub fn get(&mut self, revision: u64, doc: &Document, idx: &DocIndex) -> (&[Revision], &[StoryId]) {
        if self.at != Some(revision) {
            let found = found(doc, idx);
            self.revisions = revisions_of(&found.items);
            self.stories = found.stories;
            self.at = Some(revision);
        }
        (&self.revisions, &self.stories)
    }
}

/// Whether `p` holds a revision mark: on a run, on its mark or on its properties.
pub fn holds_marks(p: &Paragraph) -> bool {
    p.style.change.is_some() || !p.mark.rev.is_empty() || p.inlines.iter().filter_map(Inline::run).any(|r| !r.rev.is_empty())
}

pub fn revisions(doc: &Document, idx: &DocIndex) -> Vec<Revision> {
    revisions_of(&found(doc, idx).items)
}

fn revisions_of(items: &[Item]) -> Vec<Revision> {
    let mut out: Vec<Revision> = Vec::new();
    let mut open: [Option<usize>; CHANNELS] = [None; CHANNELS];
    for (it, starts) in items.iter().zip(starts(items)).filter(|(i, _)| i.listed) {
        let ch = it.channel as usize;
        match open[ch] {
            Some(e) if !starts => {
                let r = &mut out[e];
                r.end = it.end;
                if !r.ids.contains(&it.mark.id) {
                    r.ids.push(it.mark.id);
                }
            }
            _ => {
                out.push(Revision {
                    story: it.story,
                    start: it.start,
                    end: it.end,
                    kind: it.kind,
                    author: it.mark.author,
                    ids: vec![it.mark.id],
                    date: it.mark.date.clone(),
                });
                open[ch] = Some(out.len() - 1);
            }
        }
    }
    out
}

/// Whether each item starts a revision: it is listed and doesn't continue the last listed one of its channel (the
/// same kind, author and story, on the same or the next run, row or paragraph).
pub(super) fn starts(items: &[Item]) -> Vec<bool> {
    let mut open: [Option<(RevisionKind, AuthorId, StoryId, usize)>; CHANNELS] = [None; CHANNELS];
    items
        .iter()
        .map(|it| {
            if !it.listed {
                return false;
            }
            let ch = it.channel as usize;
            let joins = open[ch]
                .is_some_and(|(k, a, s, last)| k == it.kind && a == it.mark.author && s == it.story && (it.seq == last || it.seq == last + 1));
            open[ch] = Some((it.kind, it.mark.author, it.story, it.seq));
            !joins
        })
        .collect()
}

/// The changes of paragraph `p` alone into `items`, as the whole walk finds them in `story` (`rows`: the marks of the
/// inserted or deleted rows it is in), and its walked runs; `None` when it holds a text box.
pub(super) fn para_items(
    p: &Paragraph,
    next: Option<&Paragraph>,
    cell_end: bool,
    story: StoryId,
    rows: Vec<RevisionId>,
    items: &mut Vec<Item>,
) -> Option<u32> {
    items.clear();
    let found = Found { items: std::mem::take(items), ..Found::default() };
    let mut w = Walk { found, story, seq: [0; CHANNELS], boxes: Vec::new(), rows, run_no: 0 };
    w.para(p, next, cell_end);
    *items = w.found.items;
    w.boxes.is_empty().then_some(w.run_no)
}

/// How many runs the walk takes in `p`: each run, and its mark unless it ends a cell unmarked.
pub(super) fn walked_runs(p: &Paragraph, cell_end: bool) -> u32 {
    let runs = p.inlines.iter().filter(|i| i.run().is_some()).count();
    u32::try_from(runs).unwrap_or(u32::MAX).saturating_add(u32::from(!(cell_end && p.mark.rev.is_empty())))
}

pub(super) fn found(doc: &Document, idx: &DocIndex) -> Found {
    let mut w = Walk { found: Found::default(), story: StoryId::Body, seq: [0; CHANNELS], boxes: Vec::new(), rows: Vec::new(), run_no: 0 };
    for (k, s) in doc.sections.iter().enumerate() {
        if let Some(c) = &s.change {
            let (start, end) = w.span(&s.blocks);
            w.seq[Channel::Section as usize] += 1;
            w.push(Channel::Section, RevisionKind::SectionProperty, &c.mark, start, end, (None, Spot::Section(k)), true);
        }
        w.blocks(&s.blocks, false);
    }
    w.found.stories.push(StoryId::Body);
    fn notes(m: &std::collections::HashMap<i64, Vec<Block>>) -> Vec<(i64, &Vec<Block>)> {
        let mut v: Vec<(i64, &Vec<Block>)> = m.iter().map(|(k, b)| (*k, b)).collect();
        v.sort_by_key(|x| x.0);
        v
    }
    let others = doc
        .stories
        .iter()
        .filter_map(|(id, b)| Some((idx.header_footer(*id)?, b)))
        .chain(notes(&doc.footnotes).into_iter().map(|(k, b)| (StoryId::Footnote(k), b)))
        .chain(notes(&doc.endnotes).into_iter().map(|(k, b)| (StoryId::Endnote(k), b)));
    for (story, blocks) in others {
        w.story = story;
        w.found.stories.push(story);
        w.blocks(blocks, false);
    }
    // Text boxes
    let mut k = 0;
    while let Some(&(f, blocks)) = w.boxes.get(k) {
        w.story = StoryId::TextBox(f);
        w.found.stories.push(w.story);
        w.blocks(blocks, false);
        k += 1;
    }
    // Sub-template body
    if let Some(k) = idx.flow
        && let Some(part) = doc.glossary.as_ref().and_then(|g| g.parts.get(k as usize))
    {
        w.story = StoryId::BuildingBlock(k);
        w.found.stories.push(w.story);
        w.blocks(&part.body, false);
    }
    w.found
}

/// The last paragraph of `blocks`, through the tables in them: where a span over them ends.
pub(super) fn last(bs: &[Block]) -> Option<&Paragraph> {
    bs.iter().rev().find_map(|b| match b {
        Block::Para(p) => Some(p),
        Block::Table(t) => t.rows.iter().rev().flat_map(|r| r.cells.iter().rev()).find_map(|c| last(&c.blocks)),
    })
}

struct Walk<'a> {
    found: Found,
    story: StoryId,
    seq: [usize; CHANNELS],
    boxes: Vec<(FloatId, &'a [Block])>,
    /// The marks of the inserted or deleted rows being walked: their contents' are the rows' revisions.
    rows: Vec<RevisionId>,
    /// The paragraph's runs walked so far.
    run_no: u32,
}

impl<'a> Walk<'a> {
    #[allow(clippy::too_many_arguments, reason = "an item's fields, given where each change is found")]
    fn push(
        &mut self,
        channel: Channel,
        kind: RevisionKind,
        mark: &Rc<RevisionMark>,
        start: Position,
        end: Position,
        at: (Option<(ParaId, Range<u32>)>, Spot),
        listed: bool,
    ) {
        let seq = self.seq[channel as usize];
        let (run, spot) = at;
        let listed = listed && !(matches!(channel, Channel::Ins | Channel::Del) && self.rows.contains(&mark.id));
        self.found.items.push(Item { channel, kind, mark: mark.clone(), story: self.story, start, end, run, spot, listed, seq, run_no: self.run_no });
    }

    fn at(&self, p: &Paragraph, u: u32) -> Position {
        Position::text(self.story, ParaId(p.id), Unit(u))
    }

    fn span(&self, blocks: &[Block]) -> (Position, Position) {
        fn first(bs: &[Block]) -> Option<&Paragraph> {
            bs.iter().find_map(|b| match b {
                Block::Para(p) => Some(p),
                Block::Table(t) => t.rows.iter().flat_map(|r| r.cells.iter()).find_map(|c| first(&c.blocks)),
            })
        }
        let none = Position::text(self.story, ParaId(0), Unit(0));
        let start = first(blocks).map_or(none, |p| self.at(p, 0));
        let end = last(blocks).map_or(start, |p| self.at(p, para_len(p).0));
        (start, end)
    }

    fn blocks(&mut self, blocks: &'a [Block], in_cell: bool) {
        for (k, b) in blocks.iter().enumerate() {
            match b {
                Block::Para(p) => {
                    let next = match blocks.get(k + 1) {
                        Some(Block::Para(q)) => Some(q),
                        _ => None,
                    };
                    self.para(p, next, in_cell && k + 1 == blocks.len());
                }
                Block::Table(t) => self.table(t),
            }
        }
    }

    fn para(&mut self, p: &'a Paragraph, next: Option<&Paragraph>, cell_end: bool) {
        let len = para_len(p).0;
        self.run_no = 0;
        self.seq[Channel::Para as usize] += 1;
        if let Some(c) = &p.style.change {
            let (start, end) = (self.at(p, 0), self.at(p, len));
            self.push(Channel::Para, RevisionKind::ParagraphProperty, &c.mark, start, end, (None, Spot::Para(ParaId(p.id))), !c.numbering_only());
        }
        let mut at = 0u32;
        for i in &p.inlines {
            let n = i.units() as u32;
            match i {
                i if i.story().is_some() => {
                    if let Some((id, tb)) = i.story() {
                        self.boxes.push((FloatId(id), &tb.blocks));
                    }
                }
                Inline::Range(m) if matches!(m.kind, RangeKind::MoveFromStart | RangeKind::MoveToStart) => {
                    if let Some(name) = &m.name {
                        self.found.move_ranges.push((name.clone(), m.id));
                    }
                }
                _ => {}
            }
            if let Some(r) = i.run() {
                let (start, end) = (self.at(p, at), self.at(p, at + n));
                self.run(r, start, end, (Some((ParaId(p.id), at..at + n)), Spot::Run));
            }
            at += n;
        }
        if !(cell_end && p.mark.rev.is_empty()) {
            let end = next.map_or(self.at(p, len), |q| self.at(q, 0));
            self.run(&p.mark, self.at(p, len), end, (None, Spot::Mark(ParaId(p.id))));
        }
    }

    fn run(&mut self, r: &Run, start: Position, end: Position, at: (Option<(ParaId, Range<u32>)>, Spot)) {
        for ch in [Channel::Ins, Channel::Del, Channel::Fmt] {
            self.seq[ch as usize] += 1;
        }
        if let Some(m) = &r.rev.ins {
            let kind = if m.kind == MarkKind::MoveTo { RevisionKind::MovedTo } else { RevisionKind::Insert };
            self.push(Channel::Ins, kind, m, start, end, at.clone(), true);
        }
        if let Some(m) = &r.rev.del {
            let kind = if m.kind == MarkKind::MoveFrom { RevisionKind::MovedFrom } else { RevisionKind::Delete };
            self.push(Channel::Del, kind, m, start, end, at.clone(), true);
        }
        if let Some(f) = &r.rev.fmt {
            self.push(Channel::Fmt, RevisionKind::Property, &f.mark, start, end, at, true);
        }
        self.run_no += 1;
    }

    fn table(&mut self, t: &'a Table) {
        let id = TableId(t.id);
        for ch in [Channel::Para, Channel::Table, Channel::Cell, Channel::Row] {
            self.seq[ch as usize] += 2;
        }
        let starts: Vec<Position> = t
            .rows
            .iter()
            .enumerate()
            .map(|(ri, r)| r.cells.first().map_or(Position::row_end(self.story, id, ri), |c| self.span(&c.blocks).0))
            .collect();
        let story = self.story;
        let row_start = |ri: usize| starts.get(ri).copied().unwrap_or(Position::row_end(story, id, ri));
        let end = Position::row_end(story, id, t.rows.len().saturating_sub(1));
        let props = |r: &crate::docx::Row| r.rev.change.is_some() || r.rev.ex_change.is_some() || r.cells.iter().any(|c| c.rev.change.is_some());
        let mut table: Vec<&Rc<RevisionMark>> = t.change.iter().map(|c| &c.mark).chain(t.grid_change.iter().map(|g| &g.mark)).collect();
        if !t.rows.iter().any(props) && !table.is_empty() {
            self.seq[Channel::Table as usize] += 1;
            for m in table.drain(..) {
                self.push(Channel::Table, RevisionKind::TableProperty, m, row_start(0), end, (None, Spot::Table(id)), true);
            }
        }
        for (ri, r) in t.rows.iter().enumerate() {
            let (rs, re) = (row_start(ri), Position::row_end(story, id, ri));
            for ch in [Channel::Ins, Channel::Del, Channel::Fmt] {
                self.seq[ch as usize] += 1;
            }
            self.seq[Channel::Row as usize] += 1;
            let marked = self.rows.len();
            if let Some(m) = &r.rev.ins {
                self.push(Channel::Row, RevisionKind::RowInsert, m, rs, re, (None, Spot::Row(id, ri)), true);
                self.rows.push(m.id);
            }
            if let Some(m) = &r.rev.del {
                self.push(Channel::Row, RevisionKind::RowDelete, m, rs, re, (None, Spot::Row(id, ri)), true);
                self.rows.push(m.id);
            }
            self.seq[Channel::Cell as usize] += 1;
            for (ci, c) in r.cells.iter().enumerate() {
                self.seq[Channel::Cell as usize] += 1;
                if let Some((m, k)) = &c.rev.structural {
                    let kind = match k {
                        CellMarkKind::Insert => RevisionKind::CellInsertion,
                        CellMarkKind::Delete => RevisionKind::CellDeletion,
                        CellMarkKind::Merge { .. } => RevisionKind::CellMerge,
                    };
                    let (cs, ce) = self.span(&c.blocks);
                    self.push(Channel::Cell, kind, m, cs, ce, (None, Spot::Cell(id, ri, ci)), true);
                }
                self.blocks(&c.blocks, true);
            }
            self.rows.truncate(marked);
            self.seq[Channel::Table as usize] += 1;
            if props(r) {
                let own = table.drain(..).map(|m| (m, Spot::Table(id)));
                let row = r.rev.change.iter().chain(r.rev.ex_change.iter()).map(|c| (&c.mark, Spot::Row(id, ri)));
                let cells = r.cells.iter().enumerate().filter_map(|(ci, c)| Some((&c.rev.change.as_ref()?.mark, Spot::Cell(id, ri, ci))));
                let marks: Vec<(&Rc<RevisionMark>, Spot)> = own.chain(row).chain(cells).collect();
                for (m, spot) in marks {
                    self.push(Channel::Table, RevisionKind::TableProperty, m, rs, re, (None, spot), true);
                }
            }
        }
    }
}
