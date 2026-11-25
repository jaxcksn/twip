//! Revision marks by paragraph

use std::collections::{HashMap, HashSet};

use crate::docx::review::{AuthorId, RevisionId, ShowMarkup};
use crate::docx::{Block, Document, Paragraph, Table};
use crate::layout::ParaId;

use super::super::index::{Container, DocIndex};
use super::super::position::{Position, StoryId};
use super::super::step::{Step, para_len};
use super::index::{Channel, Counts, Item, RevisionKind, Spot, found, last, para_items, starts, walked_runs};

/// How many containers a step between neighbouring paragraphs passes, or paragraphs a revision is followed through,
/// before the marks are found again from the whole document.
pub(super) const REACH: usize = 64;

/// The document's revision marks by the paragraph they are in, and which start a revision (as the revision list groups
/// them), kept up to date edit by edit: an edit within paragraphs finds those paragraphs' marks again, and the
/// starts of their own and their neighbours' (a revision continues only from the run or paragraph just before).
#[derive(Debug, Default)]
pub struct Marks {
    paras: HashMap<ParaId, Marked>,
    /// Section, table, row and cell changes.
    other: Vec<Other>,
    /// `other` by the paragraph each starts in.
    other_at: HashMap<ParaId, Vec<usize>>,
    /// `other` by the paragraph its span ends in: a section's or a cell's last.
    ends: HashMap<ParaId, Vec<usize>>,
    /// The revisions by kind and author.
    starts: HashMap<(RevisionKind, AuthorId), u32>,
}

#[derive(Debug, Default)]
struct Marked {
    items: Vec<Item>,
    starts: Vec<bool>,
    /// The walked runs.
    runs: u32,
}

/// A section, table, row or cell change.
#[derive(Debug)]
struct Other {
    item: Item,
    /// Whether it starts a revision.
    start: bool,
    /// Its revision's end: its last item's.
    end: Position,
    /// The item its revision starts at.
    head: usize,
}

/// A revision starting in a paragraph: what a balloon is made from.
#[derive(Clone, Copy, Debug)]
pub struct Start {
    /// The paragraph it starts in.
    pub para: ParaId,
    /// Its first mark's id.
    pub id: RevisionId,
    /// What it changes.
    pub kind: RevisionKind,
    /// Who made it.
    pub author: AuthorId,
    /// Where it starts.
    pub start: Position,
    /// Its item: in the paragraph's marks, or in `other`.
    item: (bool, usize),
}

/// A step from a paragraph to the one the walk takes next or before it, and what lies between: a row's start parts
/// runs' changes, a table's start paragraphs' too.
#[derive(Clone, Copy, Debug)]
struct Hop {
    para: ParaId,
    row: bool,
    table: bool,
}

/// Too far or a malformed table: the marks are found again.
#[derive(Debug)]
pub struct Lost;

impl Marks {
    /// The marks of the whole document.
    pub fn build(doc: &Document, idx: &DocIndex) -> Marks {
        let found = found(doc, idx);
        let flags = starts(&found.items);
        let mut m = Marks::default();
        let mut open: [Option<usize>; 8] = [None; 8];
        for (it, s) in found.items.into_iter().zip(flags) {
            if s {
                *m.starts.entry((it.kind, it.mark.author)).or_default() += 1;
            }
            match owner(&it) {
                Some(p) => {
                    let e = m.paras.entry(p).or_default();
                    e.items.push(it);
                    e.starts.push(s);
                }
                None => {
                    let (ch, k) = (it.channel as usize, m.other.len());
                    let mut head = k;
                    if it.listed {
                        match open[ch] {
                            Some(g) if !s => {
                                m.other[g].end = it.end;
                                head = g;
                            }
                            _ => open[ch] = Some(k),
                        }
                    }
                    if let Some((p, _)) = it.start.para_offset() {
                        m.other_at.entry(p).or_default().push(k);
                    }
                    if spans(&it)
                        && let Some((p, _)) = it.end.para_offset()
                    {
                        m.ends.entry(p).or_default().push(k);
                    }
                    let end = it.end;
                    m.other.push(Other { item: it, start: s, end, head });
                }
            }
        }
        for (&p, e) in &mut m.paras {
            e.runs = place(doc, idx, p).map_or(0, |a| walked_runs(a.para, a.cell_end));
        }
        m
    }

    /// Whether the document has no revision marks.
    pub fn is_empty(&self) -> bool {
        self.paras.is_empty() && self.other.is_empty()
    }

    /// The revisions by kind: all of them, and those `show` shows.
    pub fn counts(&self, show: &ShowMarkup) -> (Counts, Counts) {
        let (mut all, mut shown) = (Counts::default(), Counts::default());
        for (&(kind, author), &n) in &self.starts {
            all.add(kind, n);
            if show.shows_change(kind.is_formatting(), author) {
                shown.add(kind, n);
            }
        }
        (all, shown)
    }

    /// The revisions starting in `para`.
    pub fn starts_in(&self, para: ParaId) -> impl Iterator<Item = Start> + '_ {
        let own = self.paras.get(&para).into_iter().flat_map(move |e| {
            e.items.iter().zip(&e.starts).enumerate().filter(|(_, (_, s))| **s).map(move |(i, (it, _))| start(para, it, (true, i)))
        });
        let other = self.other_at.get(&para).into_iter().flatten().filter_map(move |&i| {
            let o = &self.other[i];
            o.start.then(|| start(para, &o.item, (false, i)))
        });
        own.chain(other)
    }

    /// Where the revision `s` ends: its last mark's end.
    pub fn end_of(&self, doc: &Document, idx: &DocIndex, s: &Start) -> Position {
        let (own, i) = s.item;
        if !own {
            return self.other[i].end;
        }
        let Some(e) = self.paras.get(&s.para) else { return s.start };
        let first = &e.items[i];
        let (ch, mut end, mut last) = (first.channel, first.end, first.run_no);
        let (mut e, mut from, mut para) = (e, i + 1, s.para);
        // Each step goes on to a later paragraph with marks: as many as have them, the whole revision
        for _ in 0..self.paras.len() {
            for (it, &st) in e.items.iter().zip(&e.starts).skip(from) {
                if it.channel != ch || !it.listed {
                    continue;
                }
                if st {
                    return end;
                }
                (end, last) = (it.end, it.run_no);
            }
            // On into the next paragraph's first run, or (paragraph properties) the next paragraph's own
            let next = if on_runs(ch) {
                if last + 1 != e.runs {
                    return end;
                }
                let Ok(Some(next)) = run_after(doc, idx, para) else { return end };
                next
            } else if ch == Channel::Para {
                let Ok(Some(h)) = hop(doc, idx, para, true) else { return end };
                match hop(doc, idx, h.para, false) {
                    Ok(Some(b)) if b.para == para && !b.table => h.para,
                    _ => return end,
                }
            } else {
                return end;
            };
            let Some(n) = self.paras.get(&next) else { return end };
            match n.items.iter().zip(&n.starts).find(|(it, _)| it.channel == ch && it.listed) {
                Some((it, false)) if !on_runs(ch) || it.run_no == 0 => {}
                _ => return end,
            }
            (e, from, para) = (n, 0, next);
        }
        end
    }

    /// Follows an edit that changed only what is in paragraphs (`steps`, its undo, the paragraphs it touched): their
    /// marks are found again, the starts of theirs and of the paragraphs after them, and the ends of the section and
    /// cell changes that ended in them.
    pub fn patch(&mut self, doc: &Document, idx: &DocIndex, steps: &[Step], paras: &HashSet<ParaId>) -> Result<(), Lost> {
        let boxed = |v: &[crate::docx::Inline]| v.iter().any(|i| i.story().is_some());
        for s in steps {
            match s {
                Step::Text { with, .. } | Step::Restyle { inlines: with, .. } if boxed(with) => return Err(Lost),
                Step::Split { second, .. } if boxed(&second.inlines) => return Err(Lost),
                Step::Text { .. }
                | Step::Restyle { .. }
                | Step::Split { .. }
                | Step::Join { .. }
                | Step::Para { .. }
                | Step::FieldViews { .. }
                | Step::Comment { .. }
                | Step::Control { .. }
                | Step::DataPart { .. }
                | Step::ImageData { .. }
                | Step::AddImage { .. }
                | Step::Resolver { .. }
                | Step::Fonts { .. }
                | Step::DefaultTab(_)
                | Step::Hyphenation(_)
                | Step::PageSettings(_)
                | Step::DefaultTableStyle(_)
                | Step::Protection(_)
                | Step::Info(_)
                | Step::PageBackground(_) => {}
                Step::Blocks { .. }
                | Step::Rows { .. }
                | Step::HeaderPart { .. }
                | Step::BuildingBlock { .. }
                | Step::TableProps { .. }
                | Step::Note { .. }
                | Step::Separator { .. }
                | Step::SectionPage { .. }
                | Step::SectionBreak { .. } => return Err(Lost),
            }
        }
        for &p in paras {
            if self.other_at.contains_key(&p) && idx.para_slot(p).is_none() {
                return Err(Lost);
            }
            self.find(doc, idx, p)?;
        }
        self.follow_ends(doc, idx, paras)?;
        for &p in paras.iter().filter(|p| idx.para_slot(**p).is_some()) {
            self.restart(doc, idx, p)?;
            // The paragraph after, and the runs after
            let (mut q, mut ended) = (p, false);
            for _ in 0..REACH {
                let Some(h) = hop(doc, idx, q, true)? else {
                    ended = true;
                    break;
                };
                self.restart(doc, idx, h.para)?;
                if !transparent(doc, idx, h.para) {
                    ended = true;
                    break;
                }
                q = h.para;
            }
            if !ended {
                return Err(Lost);
            }
        }
        Ok(())
    }

    fn count(&mut self, it: &Item, by: i64) {
        let n = self.starts.entry((it.kind, it.mark.author)).or_default();
        *n = u32::try_from(i64::from(*n) + by).unwrap_or(0);
    }

    /// Finds `p`'s marks again (none if it went), none of them counted as a start yet.
    fn find(&mut self, doc: &Document, idx: &DocIndex, p: ParaId) -> Result<(), Lost> {
        let mut e = self.paras.remove(&p).unwrap_or_default();
        for (it, _) in e.items.iter().zip(&e.starts).filter(|(_, s)| **s) {
            self.count(it, -1);
        }
        let Some(a) = place(doc, idx, p) else { return Ok(()) };
        let story = idx.story(a.container);
        if let StoryId::BuildingBlock(k) = idx.host(story)
            && idx.flow != Some(k)
        {
            return Ok(());
        }
        e.runs = para_items(a.para, a.next, a.cell_end, story, a.rows, &mut e.items).ok_or(Lost)?;
        if !e.items.is_empty() {
            e.starts.clear();
            e.starts.resize(e.items.len(), false);
            self.paras.insert(p, e);
        }
        Ok(())
    }

    /// Follows an edit of `paras` that joins or parts no marks: where the section and cell changes ending in them end.
    pub fn follow_ends(&mut self, doc: &Document, idx: &DocIndex, paras: &HashSet<ParaId>) -> Result<(), Lost> {
        for &p in paras {
            self.reend(doc, idx, p)?;
        }
        Ok(())
    }

    /// Finds again where the spans ending in `p` end: in it, or in the paragraph now last in their section or cell.
    fn reend(&mut self, doc: &Document, idx: &DocIndex, p: ParaId) -> Result<(), Lost> {
        let mut k = 0;
        while let Some(&j) = self.ends.get(&p).and_then(|v| v.get(k)) {
            let o = &self.other[j];
            let (old, head) = (o.item.end, o.head);
            let new = span_end(doc, idx, &o.item).ok_or(Lost)?;
            self.other[j].item.end = new;
            if self.other[j].end == old {
                self.other[j].end = new;
            }
            if self.other[head].end == old {
                self.other[head].end = new;
            }
            match new.para_offset() {
                Some((q, _)) if q == p => k += 1,
                Some((q, _)) => {
                    if let Some(v) = self.ends.get_mut(&p) {
                        v.swap_remove(k);
                    }
                    self.ends.entry(q).or_default().push(j);
                }
                None => return Err(Lost),
            }
        }
        if self.ends.get(&p).is_some_and(Vec::is_empty) {
            self.ends.remove(&p);
        }
        Ok(())
    }

    /// Works out again which of `p`'s marks start a revision, from the runs and paragraph before it.
    fn restart(&mut self, doc: &Document, idx: &DocIndex, p: ParaId) -> Result<(), Lost> {
        let Some(e) = self.paras.get(&p) else { return Ok(()) };
        let mut open: [Option<(RevisionKind, AuthorId, i64)>; 8] = [None; 8];
        if e.items.iter().any(|it| on_runs(it.channel) && it.run_no == 0)
            && let Some(q) = run_before(doc, idx, p)?
            && let Some(b) = self.paras.get(&q)
        {
            for it in b.items.iter().filter(|it| on_runs(it.channel) && it.listed && it.run_no + 1 == b.runs) {
                open[it.channel as usize] = Some((it.kind, it.mark.author, -1));
            }
        }
        if e.items.iter().any(|it| it.channel == Channel::Para)
            && let Some(h) = hop(doc, idx, p, false)?
            && !h.table
            && let Some(it) = self.paras.get(&h.para).and_then(|b| b.items.iter().find(|it| it.channel == Channel::Para && it.listed))
        {
            open[Channel::Para as usize] = Some((it.kind, it.mark.author, -1));
        }
        let Some(mut e) = self.paras.remove(&p) else { return Ok(()) };
        for (it, was) in e.items.iter().zip(e.starts.iter_mut()) {
            let now = it.listed && {
                let ch = it.channel as usize;
                let at = i64::from(it.run_no);
                let joins = open[ch].is_some_and(|(k, a, last)| k == it.kind && a == it.mark.author && at <= last + 1);
                open[ch] = Some((it.kind, it.mark.author, at));
                !joins
            };
            if now != *was {
                self.count(it, if now { 1 } else { -1 });
                *was = now;
            }
        }
        self.paras.insert(p, e);
        Ok(())
    }

    /// Whether `other` equals `self`: the same marks, starts and counts.
    #[cfg(test)]
    pub fn same(&self, other: &Marks) -> Result<(), String> {
        let counts = |m: &Marks| {
            let mut v: Vec<_> = m.starts.iter().filter(|(_, n)| **n > 0).map(|(k, n)| (format!("{k:?}"), *n)).collect();
            v.sort();
            v
        };
        if counts(self) != counts(other) {
            return Err(format!("counts {:?} != {:?}", counts(self), counts(other)));
        }
        for (p, e) in &other.paras {
            let Some(m) = self.paras.get(p) else { return Err(format!("{p:?} missing")) };
            let key = |e: &Marked| e.items.iter().zip(&e.starts).map(|(i, s)| (i.kind, i.mark.id, i.start, i.end, i.listed, *s)).collect::<Vec<_>>();
            if key(m) != key(e) || m.runs != e.runs {
                return Err(format!("{p:?}: {:?} ({}) != {:?} ({})", key(m), m.runs, key(e), e.runs));
            }
        }
        if self.paras.len() != other.paras.len() {
            return Err(format!("{} paragraphs != {}", self.paras.len(), other.paras.len()));
        }
        let key = |m: &Marks| m.other.iter().map(|o| (o.item.kind, o.item.mark.id, o.item.start, o.item.end, o.start, o.end)).collect::<Vec<_>>();
        if key(self) != key(other) {
            return Err(format!("other {:?} != {:?}", key(self), key(other)));
        }
        Ok(())
    }
}

/// Whether `it` spans a section or cell, ending where its last paragraph does.
fn spans(it: &Item) -> bool {
    matches!(it.channel, Channel::Section | Channel::Cell)
}

/// Where the span of section or cell change `it` ends now: its last paragraph's end.
fn span_end(doc: &Document, idx: &DocIndex, it: &Item) -> Option<Position> {
    let blocks = match it.spot {
        Spot::Section(k) => &doc.sections.get(k)?.blocks,
        Spot::Cell(t, r, c) => &idx.table(doc, t)?.rows.get(r)?.cells.get(c)?.blocks,
        _ => return None,
    };
    let p = last(blocks)?;
    Some(Position::text(it.story, ParaId(p.id), para_len(p)))
}

/// Whether changes on `c` are of runs, continued run to run.
fn on_runs(c: Channel) -> bool {
    matches!(c, Channel::Ins | Channel::Del | Channel::Fmt)
}

fn start(para: ParaId, it: &Item, item: (bool, usize)) -> Start {
    Start { para, id: it.mark.id, kind: it.kind, author: it.mark.author, start: it.start, item }
}

/// The paragraph a change of text, a paragraph mark or paragraph properties is in.
fn owner(it: &Item) -> Option<ParaId> {
    match it.spot {
        Spot::Para(p) | Spot::Mark(p) => Some(p),
        Spot::Run => it.run.as_ref().map(|r| r.0),
        Spot::Row(..) | Spot::Cell(..) | Spot::Table(_) | Spot::Section(_) => None,
    }
}

/// A paragraph where the walk finds it: the paragraph after it in its blocks, whether it ends a cell, and the marks
/// of the inserted or deleted rows it is in.
struct Place<'d> {
    para: &'d Paragraph,
    next: Option<&'d Paragraph>,
    cell_end: bool,
    container: Container,
    rows: Vec<RevisionId>,
}

fn place<'d>(doc: &'d Document, idx: &DocIndex, id: ParaId) -> Option<Place<'d>> {
    let s = idx.para_slot(id)?;
    let blocks = idx.blocks(doc, s.container)?;
    let Some(Block::Para(para)) = blocks.get(s.block).filter(|b| matches!(b, Block::Para(p) if p.id == id.0)) else { return None };
    let next = match blocks.get(s.block + 1) {
        Some(Block::Para(q)) => Some(q),
        _ => None,
    };
    let cell_end = matches!(s.container, Container::Cell { .. }) && s.block + 1 == blocks.len();
    let (mut rows, mut c) = (Vec::new(), s.container);
    for _ in 0..REACH {
        let Container::Cell { table, row, .. } = c else { break };
        if let Some(r) = idx.table(doc, table).and_then(|t| t.rows.get(row)) {
            rows.extend(r.rev.ins.iter().chain(r.rev.del.iter()).map(|m| m.id));
        }
        c = idx.table_slot(table)?.container;
    }
    Some(Place { para, next, cell_end, container: s.container, rows })
}

/// Whether the walk takes no run in `p`: it ends a cell, has no runs and its mark isn't marked.
fn transparent(doc: &Document, idx: &DocIndex, p: ParaId) -> bool {
    place(doc, idx, p).is_some_and(|a| walked_runs(a.para, a.cell_end) == 0)
}

/// The paragraph whose last run the walk takes just before `p`'s first, with no row starting between.
fn run_before(doc: &Document, idx: &DocIndex, p: ParaId) -> Result<Option<ParaId>, Lost> {
    let mut q = p;
    for _ in 0..REACH {
        match hop(doc, idx, q, false)? {
            Some(h) if !h.row => {
                if !transparent(doc, idx, h.para) {
                    return Ok(Some(h.para));
                }
                q = h.para;
            }
            _ => return Ok(None),
        }
    }
    Err(Lost)
}

/// The paragraph whose first run the walk takes just after `p`'s last, with no row starting between.
fn run_after(doc: &Document, idx: &DocIndex, p: ParaId) -> Result<Option<ParaId>, Lost> {
    let mut q = p;
    for _ in 0..REACH {
        match hop(doc, idx, q, true)? {
            Some(h) if !h.row => {
                if !transparent(doc, idx, h.para) {
                    return Ok(Some(h.para));
                }
                q = h.para;
            }
            _ => return Ok(None),
        }
    }
    Err(Lost)
}

/// The paragraph after (`forward`) or before `p` in its story, through tables: none at the story's end or when the
/// table walk goes wrong.
pub(super) fn next_para(doc: &Document, idx: &DocIndex, p: ParaId, forward: bool) -> Option<ParaId> {
    hop(doc, idx, p, forward).ok().flatten().map(|h| h.para)
}

/// The paragraph the walk takes after (`forward`) or before `p` in its story.
fn hop(doc: &Document, idx: &DocIndex, p: ParaId, forward: bool) -> Result<Option<Hop>, Lost> {
    let s = idx.para_slot(p).ok_or(Lost)?;
    let (mut c, mut k) = (s.container, Some(s.block));
    let (mut row, mut table) = (false, false);
    for _ in 0..REACH {
        let blocks = idx.blocks(doc, c).ok_or(Lost)?;
        if blocks.is_empty() && matches!(c, Container::Cell { .. }) {
            return Err(Lost);
        }
        let at = if forward { k.map_or(Some(0), |k| Some(k + 1)) } else { k.unwrap_or(blocks.len()).checked_sub(1) };
        if let Some(b) = at.and_then(|i| blocks.get(i)) {
            return match b {
                Block::Para(q) => Ok(Some(Hop { para: ParaId(q.id), row, table })),
                Block::Table(t) if forward => Ok(Some(Hop { para: edge(t, true)?, row: true, table: true })),
                Block::Table(t) => Ok(Some(Hop { para: edge(t, false)?, row, table })),
            };
        }
        match c {
            Container::Section(i) if forward && i + 1 < doc.sections.len() => (c, k) = (Container::Section(i + 1), None),
            Container::Section(i) if !forward && i > 0 => (c, k) = (Container::Section(i - 1), None),
            Container::Section(_) | Container::Story(_) => return Ok(None),
            Container::Cell { table: t, row: r, cell } => {
                let rows = &idx.table(doc, t).ok_or(Lost)?.rows;
                let cells = |r: usize| rows.get(r).map_or(0, |r| r.cells.len());
                let to = if forward {
                    if cell + 1 < cells(r) {
                        Some((r, cell + 1))
                    } else {
                        row = true;
                        (r + 1 < rows.len()).then_some((r + 1, 0))
                    }
                } else if cell > 0 {
                    Some((r, cell - 1))
                } else {
                    row = true;
                    r.checked_sub(1).map(|r| (r, cells(r).wrapping_sub(1)))
                };
                match to {
                    Some((r, ci)) if ci < cells(r) => (c, k) = (Container::Cell { table: t, row: r, cell: ci }, None),
                    Some(_) => return Err(Lost),
                    None => {
                        // Out of the table
                        let ts = idx.table_slot(t).ok_or(Lost)?;
                        if forward {
                            row = false;
                        } else {
                            table = true;
                        }
                        (c, k) = (ts.container, Some(ts.block));
                    }
                }
            }
        }
    }
    Err(Lost)
}

/// A table's first (`first`) or last paragraph, through the tables in its first or last cell.
fn edge(t: &Table, first: bool) -> Result<ParaId, Lost> {
    let mut t = t;
    for _ in 0..REACH {
        let row = if first { t.rows.first() } else { t.rows.last() };
        let cell = row.and_then(|r| if first { r.cells.first() } else { r.cells.last() }).ok_or(Lost)?;
        match if first { cell.blocks.first() } else { cell.blocks.last() }.ok_or(Lost)? {
            Block::Para(p) => return Ok(ParaId(p.id)),
            Block::Table(n) => t = n,
        }
    }
    Err(Lost)
}
