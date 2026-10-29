//! Finding revisions in document order, across stories: the body, then headers and footers, footnotes, endnotes and
//! text boxes. A revision is a stretch of one kind of change by one author: adjacent runs, paragraph marks and rows
//! inserted (or deleted, or formatted) by the same person are one revision, as are the property changes of one row.

use std::ops::Range;
use std::rc::Rc;

use crate::docx::review::{AuthorId, CellMarkKind, MarkKind, RangeKind, RevisionId, RevisionMark};
use crate::docx::{Block, Document, Inline, Paragraph, Run, Table};
use crate::layout::{FloatId, ParaId, TableId, Unit};

use super::super::index::DocIndex;
use super::super::position::{Position, StoryId};
use super::super::step::para_len;

/// What a revision changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RevisionKind {
    Insert,
    Delete,
    MovedFrom,
    MovedTo,
    /// Character formatting.
    Property,
    ParagraphProperty,
    /// Table, row, cell or column grid properties.
    TableProperty,
    SectionProperty,
    CellInsertion,
    CellDeletion,
    CellMerge,
}

impl RevisionKind {
    pub fn name(self) -> &'static str {
        match self {
            RevisionKind::Insert => "Insert",
            RevisionKind::Delete => "Delete",
            RevisionKind::MovedFrom => "MovedFrom",
            RevisionKind::MovedTo => "MovedTo",
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

/// One revision: where it starts and ends in its story, what it is, who made it, and the marks it covers.
#[derive(Clone, Debug, PartialEq)]
pub struct Revision {
    pub story: StoryId,
    pub start: Position,
    pub end: Position,
    pub kind: RevisionKind,
    pub author: AuthorId,
    pub ids: Vec<RevisionId>,
    /// The first change's date.
    pub date: Option<crate::docx::review::Timestamp>,
}

/// The slots changes sit in. Adjacent changes join into one revision only within a slot, so an insertion another
/// author deleted is both an insertion and (inside a longer one) a deletion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Channel {
    Ins,
    Del,
    Fmt,
    Para,
    Table,
    Cell,
    Section,
}

/// One change where it sits: a run's, a paragraph mark's, a paragraph's, a row's, a cell's, a table's or a section's.
#[derive(Clone, Debug)]
pub(super) struct Item {
    channel: Channel,
    pub kind: RevisionKind,
    pub mark: Rc<RevisionMark>,
    pub story: StoryId,
    pub start: Position,
    pub end: Position,
    /// A run's: its paragraph and units. Other changes resolve whole.
    pub run: Option<(ParaId, Range<u32>)>,
    /// What the review lists. A numbering change alone isn't listed: it resolves with its paragraph's other changes.
    listed: bool,
    /// The element's place in its channel's order: adjacent elements of a channel differ by one.
    seq: usize,
}

/// Every change in document order, and the move range markers (name, `w:id`) of every story.
#[derive(Debug, Default)]
pub(super) struct Found {
    pub items: Vec<Item>,
    pub move_ranges: Vec<(Rc<str>, i64)>,
    /// The stories in the order they were walked.
    pub stories: Vec<StoryId>,
}

/// The document's revisions, built when asked for and kept until the document changes.
#[derive(Debug, Default)]
pub struct List {
    at: Option<u64>,
    revisions: Vec<Revision>,
    stories: Vec<StoryId>,
}

impl List {
    /// The revisions of `doc` at document revision `revision`, built again when the document changed since.
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

/// The document's revisions in document order, story by story.
pub fn revisions(doc: &Document, idx: &DocIndex) -> Vec<Revision> {
    revisions_of(&found(doc, idx).items)
}

/// Joins adjacent listed changes of one kind and author (in one channel and story) into revisions.
fn revisions_of(items: &[Item]) -> Vec<Revision> {
    let mut out: Vec<Revision> = Vec::new();
    // Per channel: the revision its last change went to, and that change's place.
    let mut open: [Option<(usize, usize)>; 7] = [None; 7];
    for it in items.iter().filter(|i| i.listed) {
        let ch = it.channel as usize;
        // Changes of one element, and of adjacent elements, join.
        let joins = open[ch].filter(|&(e, last)| {
            let r = &out[e];
            r.kind == it.kind && r.author == it.mark.author && r.story == it.story && (it.seq == last || it.seq == last + 1)
        });
        match joins {
            Some((e, _)) => {
                let r = &mut out[e];
                r.end = it.end;
                if !r.ids.contains(&it.mark.id) {
                    r.ids.push(it.mark.id);
                }
                open[ch] = Some((e, it.seq));
            }
            None => {
                out.push(Revision {
                    story: it.story,
                    start: it.start,
                    end: it.end,
                    kind: it.kind,
                    author: it.mark.author,
                    ids: vec![it.mark.id],
                    date: it.mark.date.clone(),
                });
                open[ch] = Some((out.len() - 1, it.seq));
            }
        }
    }
    out
}

/// Every change of the document, story by story in document order.
pub(super) fn found(doc: &Document, idx: &DocIndex) -> Found {
    let mut w = Walk { found: Found::default(), story: StoryId::Body, seq: [0; 7], boxes: Vec::new() };
    for s in &doc.sections {
        if let Some(c) = &s.change {
            let (start, end) = w.span(&s.blocks);
            w.seq[Channel::Section as usize] += 1;
            w.push(Channel::Section, RevisionKind::SectionProperty, &c.mark, start, end, None, true);
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
        .filter_map(|(name, b)| Some((idx.header_footer(name)?, b)))
        .chain(notes(&doc.footnotes).into_iter().map(|(k, b)| (StoryId::Footnote(k), b)))
        .chain(notes(&doc.endnotes).into_iter().map(|(k, b)| (StoryId::Endnote(k), b)));
    for (story, blocks) in others {
        w.story = story;
        w.found.stories.push(story);
        w.blocks(blocks, false);
    }
    // Text boxes, those inside text boxes after them.
    let mut k = 0;
    while let Some(&(f, blocks)) = w.boxes.get(k) {
        w.story = StoryId::TextBox(f);
        w.found.stories.push(w.story);
        w.blocks(blocks, false);
        k += 1;
    }
    w.found
}

struct Walk<'a> {
    found: Found,
    story: StoryId,
    /// Per channel, the place of the element last walked.
    seq: [usize; 7],
    boxes: Vec<(FloatId, &'a [Block])>,
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
        run: Option<(ParaId, Range<u32>)>,
        listed: bool,
    ) {
        let seq = self.seq[channel as usize];
        self.found.items.push(Item { channel, kind, mark: mark.clone(), story: self.story, start, end, run, listed, seq });
    }

    fn at(&self, p: &Paragraph, u: u32) -> Position {
        Position::text(self.story, ParaId(p.id), Unit(u))
    }

    /// The start of the first paragraph and the end of the last in `blocks` (descending into tables).
    fn span(&self, blocks: &[Block]) -> (Position, Position) {
        fn first(bs: &[Block]) -> Option<&Paragraph> {
            bs.iter().find_map(|b| match b {
                Block::Para(p) => Some(p),
                Block::Table(t) => t.rows.iter().flat_map(|r| r.cells.iter()).find_map(|c| first(&c.blocks)),
            })
        }
        fn last(bs: &[Block]) -> Option<&Paragraph> {
            bs.iter().rev().find_map(|b| match b {
                Block::Para(p) => Some(p),
                Block::Table(t) => t.rows.iter().rev().flat_map(|r| r.cells.iter().rev()).find_map(|c| last(&c.blocks)),
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
        self.seq[Channel::Para as usize] += 1;
        if let Some(c) = &p.style.change {
            let (start, end) = (self.at(p, 0), self.at(p, len));
            self.push(Channel::Para, RevisionKind::ParagraphProperty, &c.mark, start, end, None, c.previous_raw.0.is_some());
        }
        let mut at = 0u32;
        for i in &p.inlines {
            let n = i.units() as u32;
            match i {
                Inline::Float(f, _) => {
                    if let Some(tb) = &f.text_box {
                        self.boxes.push((FloatId(f.id), &tb.blocks));
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
                self.run(r, start, end, Some((ParaId(p.id), at..at + n)));
            }
            at += n;
        }
        // A cell's last mark is its end: it neither starts nor breaks a revision unless it has one itself.
        if !(cell_end && p.mark.rev.is_empty()) {
            let end = next.map_or(self.at(p, len), |q| self.at(q, 0));
            self.run(&p.mark, self.at(p, len), end, None);
        }
    }

    /// An element of the text channels (a run, a paragraph mark): its insertion, deletion and formatting change.
    fn run(&mut self, r: &Run, start: Position, end: Position, run: Option<(ParaId, Range<u32>)>) {
        for ch in [Channel::Ins, Channel::Del, Channel::Fmt] {
            self.seq[ch as usize] += 1;
        }
        if let Some(m) = &r.rev.ins {
            let kind = if m.kind == MarkKind::MoveTo { RevisionKind::MovedTo } else { RevisionKind::Insert };
            self.push(Channel::Ins, kind, m, start, end, run.clone(), true);
        }
        if let Some(m) = &r.rev.del {
            let kind = if m.kind == MarkKind::MoveFrom { RevisionKind::MovedFrom } else { RevisionKind::Delete };
            self.push(Channel::Del, kind, m, start, end, run.clone(), true);
        }
        if let Some(f) = &r.rev.fmt {
            self.push(Channel::Fmt, RevisionKind::Property, &f.mark, start, end, run, true);
        }
    }

    fn table(&mut self, t: &'a Table) {
        let id = TableId(t.id);
        // Rows of different tables aren't adjacent.
        for ch in [Channel::Para, Channel::Table, Channel::Cell] {
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
        // The table's own property and grid changes go with the first row's property changes, or stand alone.
        let mut table: Vec<&Rc<RevisionMark>> = t.change.iter().map(|c| &c.mark).chain(t.grid_change.iter().map(|g| &g.mark)).collect();
        if !t.rows.iter().any(props) && !table.is_empty() {
            self.seq[Channel::Table as usize] += 1;
            for m in table.drain(..) {
                self.push(Channel::Table, RevisionKind::TableProperty, m, row_start(0), end, None, true);
            }
        }
        for (ri, r) in t.rows.iter().enumerate() {
            let (rs, re) = (row_start(ri), Position::row_end(story, id, ri));
            for ch in [Channel::Ins, Channel::Del, Channel::Fmt] {
                self.seq[ch as usize] += 1;
            }
            if let Some(m) = &r.rev.ins {
                self.push(Channel::Ins, RevisionKind::Insert, m, rs, re, None, true);
            }
            if let Some(m) = &r.rev.del {
                self.push(Channel::Del, RevisionKind::Delete, m, rs, re, None, true);
            }
            // Cell marks join along a row, not across rows.
            self.seq[Channel::Cell as usize] += 1;
            for c in &r.cells {
                self.seq[Channel::Cell as usize] += 1;
                if let Some((m, k)) = &c.rev.structural {
                    let kind = match k {
                        CellMarkKind::Insert => RevisionKind::CellInsertion,
                        CellMarkKind::Delete => RevisionKind::CellDeletion,
                        CellMarkKind::Merge { .. } => RevisionKind::CellMerge,
                    };
                    let (cs, ce) = self.span(&c.blocks);
                    self.push(Channel::Cell, kind, m, cs, ce, None, true);
                }
                self.blocks(&c.blocks, true);
            }
            // A row's property changes (its own, its cells', and the table's with the first such row) are one
            // revision per author, listed after its cells'.
            self.seq[Channel::Table as usize] += 1;
            if props(r) {
                let row = r.rev.change.iter().chain(r.rev.ex_change.iter()).map(|c| &c.mark);
                let cells = r.cells.iter().filter_map(|c| c.rev.change.as_ref()).map(|c| &c.mark);
                let marks: Vec<&Rc<RevisionMark>> = table.drain(..).chain(row).chain(cells).collect();
                for m in marks {
                    self.push(Channel::Table, RevisionKind::TableProperty, m, rs, re, None, true);
                }
            }
        }
    }
}
