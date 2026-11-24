//! Undo and redo

use std::collections::VecDeque;

use crate::docx::numbering::{AbstractNum, Level as NumLevel, Num};
use crate::docx::resolve::Resolver;
use crate::docx::styles::Style;
use crate::docx::{Block, Inline};
use crate::layout::{ParaId, Unit};

use super::index::Level;
use super::selection::Selection;
use super::span_codes::SpanKey;
use super::step::Step;

#[derive(Clone, Debug, PartialEq)]
pub struct SelectionRecord {
    pub selection: Selection,
    pub chain: Vec<Level>,
}

#[derive(Debug)]
pub struct Transaction {
    pub undo: Vec<Step>,
    pub before: SelectionRecord,
    pub after: SelectionRecord,
    pub label: std::borrow::Cow<'static, str>,
    pub bytes: usize,
    pub id: u64,
    /// A view's steps (fields across paragraphs shown as codes or results): the fields and whether each shows its
    /// code. Never undone or redone on its own: an undo or redo takes it with the edit next to it.
    pub view: Option<Vec<(SpanKey, bool)>>,
}

/// Redo steps set aside.
#[derive(Debug, Default)]
pub struct Redo {
    steps: Vec<Transaction>,
    views: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TypingRun {
    pub para: ParaId,
    pub at: Unit,
    pub kind: Typing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Typing {
    Insert,
    Backspace,
    Delete,
}

pub struct History {
    undo: VecDeque<Transaction>,
    redo: Vec<Transaction>,
    typing: Option<TypingRun>,
    budget: usize,
    bytes: usize,
    ids: u64,
    /// The position of the last step dropped, by eviction or [`History::fence`].
    evicted: u64,
    /// How many of `undo` are views' steps; two are never next to each other.
    views: usize,
    /// How many of `redo` are views' steps.
    redo_views: usize,
    /// The view's steps since the last edit, undo, or redo: taken back before the next one and shown again after an
    /// undo or redo, put before an edit's own steps.
    pending: Option<Transaction>,
    /// The edit whose record put the pending view's steps before it.
    chained: Option<u64>,
}

pub const MIN_STEPS: usize = 100;

impl Default for History {
    fn default() -> History {
        History {
            undo: VecDeque::new(),
            redo: Vec::new(),
            typing: None,
            budget: 32 << 20,
            bytes: 0,
            ids: 0,
            evicted: 0,
            views: 0,
            redo_views: 0,
            pending: None,
            chained: None,
        }
    }
}

impl History {
    pub fn record(&mut self, mut tx: Transaction, typing: Option<TypingRun>) {
        self.redo.clear();
        self.redo_views = 0;
        self.chained = None;
        let chain = self.pending.take().filter(|p| !p.undo.is_empty());
        if let Some(mut p) = chain {
            p.id = self.position();
            self.bytes += p.bytes;
            self.views += 1;
            self.undo.push_back(p);
            self.typing = None;
            self.chained = Some(self.ids + 1);
        }
        self.ids += 1;
        let joins = match (typing, self.typing) {
            (Some(t), Some(last)) => t.para == last.para && t.kind == last.kind && self.undo.back().is_some_and(|b| b.view.is_none()),
            _ => false,
        };
        if joins && let Some(last) = self.undo.back_mut() {
            last.undo.append(&mut tx.undo);
            last.after = tx.after;
            last.id = self.ids;
            last.bytes += tx.bytes;
            self.bytes += tx.bytes;
        } else {
            self.bytes += tx.bytes;
            tx.id = self.ids;
            self.undo.push_back(tx);
        }
        self.typing = typing.map(|t| TypingRun { at: t.at, ..t });
        while self.bytes > self.budget && self.undo_depth() > MIN_STEPS {
            if let Some(old) = self.undo.pop_front() {
                self.bytes -= old.bytes;
                self.evicted = old.id;
                self.views -= old.view.is_some() as usize;
            }
        }
    }

    /// The view's steps since the last edit, undo or redo.
    pub fn pending(&self) -> Option<&Transaction> {
        self.pending.as_ref()
    }

    pub fn take_pending(&mut self) -> Option<Transaction> {
        self.pending.take()
    }

    pub fn set_pending(&mut self, view: Option<Transaction>) {
        self.typing = None;
        self.pending = view;
    }

    /// The edit whose record put the pending view's steps before it.
    pub fn chained(&self) -> Option<u64> {
        self.chained
    }

    /// Makes the view's steps on top pending again, before the pending ones: the edit [`History::chained`] names was
    /// taken back as if never made.
    pub fn unchain(&mut self) {
        self.chained = None;
        let Some(mut v) = self.pop_undo_view() else { return };
        if let Some(mut p) = self.pending.take() {
            v.undo.append(&mut p.undo);
            v.bytes += p.bytes;
            v.view = Some(merge_views(v.view.take().unwrap_or_default(), p.view.unwrap_or_default()));
        }
        self.pending = Some(v);
    }

    /// Takes the view's steps on top of the undo steps, if that is what is there.
    pub fn pop_undo_view(&mut self) -> Option<Transaction> {
        self.undo.back()?.view.as_ref()?;
        self.pop_undo()
    }

    /// Takes the view's steps on top of the redo steps, if that is what is there.
    pub fn pop_redo_view(&mut self) -> Option<Transaction> {
        self.redo.last()?.view.as_ref()?;
        self.pop_redo()
    }

    /// The edits undo would take back, most recent first.
    fn edits(&self) -> impl Iterator<Item = &Transaction> {
        self.undo.iter().rev().filter(|t| t.view.is_none())
    }

    /// The edits redo would make again, the next first.
    fn redos(&self) -> impl Iterator<Item = &Transaction> {
        self.redo.iter().rev().filter(|t| t.view.is_none())
    }

    pub fn break_typing(&mut self) {
        self.typing = None;
    }

    pub fn continues(&self, para: ParaId, at: Unit, kind: Typing) -> bool {
        self.typing.is_some_and(|r| r.para == para && r.at == at && r.kind == kind)
    }

    pub fn pop_undo(&mut self) -> Option<Transaction> {
        self.typing = None;
        let tx = self.undo.pop_back()?;
        self.bytes -= tx.bytes;
        self.views -= tx.view.is_some() as usize;
        Some(tx)
    }

    pub fn pop_redo(&mut self) -> Option<Transaction> {
        self.typing = None;
        let tx = self.redo.pop()?;
        self.redo_views -= tx.view.is_some() as usize;
        Some(tx)
    }

    pub fn push_redo(&mut self, tx: Transaction) {
        self.redo_views += tx.view.is_some() as usize;
        self.redo.push(tx);
    }

    pub fn take_redo(&mut self) -> Redo {
        Redo { steps: std::mem::take(&mut self.redo), views: std::mem::take(&mut self.redo_views) }
    }

    pub fn put_redo(&mut self, redo: Redo) {
        self.redo = redo.steps;
        self.redo_views = redo.views;
    }

    pub fn push_undo_keep_redo(&mut self, tx: Transaction) {
        self.bytes += tx.bytes;
        self.views += tx.view.is_some() as usize;
        self.undo.push_back(tx);
    }

    pub fn undo_depth(&self) -> usize {
        self.undo.len() - self.views
    }

    pub fn redo_depth(&self) -> usize {
        self.redo.len() - self.redo_views
    }

    pub fn next_undo(&self) -> Option<&str> {
        self.edits().next().map(|t| t.label.as_ref())
    }

    pub fn next_redo(&self) -> Option<&str> {
        self.redos().next().map(|t| t.label.as_ref())
    }

    /// What undo would take back, most recent first.
    pub fn undo_labels(&self) -> impl Iterator<Item = &str> {
        self.edits().map(|t| t.label.as_ref())
    }

    /// What redo would do again, the next first.
    pub fn redo_labels(&self) -> impl Iterator<Item = &str> {
        self.redos().map(|t| t.label.as_ref())
    }

    pub fn bytes(&self) -> usize {
        self.bytes
    }

    pub fn position(&self) -> u64 {
        self.edits().next().map_or(self.evicted, |t| t.id)
    }

    /// Drops every undo and redo step: nothing done so far can be undone. The position stays.
    pub fn fence(&mut self) {
        self.evicted = self.position();
        self.undo.clear();
        self.redo.clear();
        self.typing = None;
        self.bytes = 0;
        self.views = 0;
        self.redo_views = 0;
        self.chained = None;
    }
}

/// `under`'s fields and how they show, `over`'s instead where both have one.
pub fn merge_views(mut under: Vec<(SpanKey, bool)>, over: Vec<(SpanKey, bool)>) -> Vec<(SpanKey, bool)> {
    if under.is_empty() {
        return over;
    }
    let mut at: std::collections::HashMap<SpanKey, usize> = under.iter().enumerate().map(|(i, (k, _))| (*k, i)).collect();
    for (k, code) in over {
        match at.get(&k) {
            Some(&i) => under[i].1 = code,
            None => {
                at.insert(k, under.len());
                under.push((k, code));
            }
        }
    }
    under
}

pub fn step_bytes(s: &Step) -> usize {
    fn inlines(v: &[Inline]) -> usize {
        v.iter()
            .map(|i| {
                48 + match i {
                    Inline::Text(t, _) => t.len(),
                    _ => i.story().map_or(0, |(_, tb)| blocks(&tb.blocks)),
                }
            })
            .sum()
    }
    fn blocks(v: &[Block]) -> usize {
        v.iter()
            .map(|b| match b {
                Block::Para(p) => 96 + inlines(&p.inlines),
                Block::Table(t) => 128 + t.rows.iter().flat_map(|r| r.cells.iter()).map(|c| 64 + blocks(&c.blocks)).sum::<usize>(),
            })
            .sum()
    }
    match s {
        Step::Text { with, .. } => 32 + inlines(with),
        Step::Split { second, .. } => 96 + inlines(&second.inlines),
        Step::Join { .. }
        | Step::Para { .. }
        | Step::TableProps { .. }
        | Step::SectionPage { .. }
        | Step::DefaultTab(_)
        | Step::Hyphenation(_)
        | Step::PageSettings(_)
        | Step::DefaultTableStyle(_)
        | Step::Info(_)
        | Step::Protection(_)
        | Step::SectionBreak { .. } => 32,
        Step::Resolver { resolver, added } => {
            32 + resolver.as_deref().map_or(0, resolver_bytes)
                + added.iter().map(|a| std::mem::size_of::<crate::docx::AddedList>() + a.abstract_xml.len()).sum::<usize>()
        }
        Step::Fonts { resolver, .. } => 32 + resolver.as_deref().map_or(0, resolver_bytes),
        Step::PageBackground(_) => 64,
        Step::ImageData { data, .. } => 32 + data.len(),
        Step::AddImage { data, .. } => 32 + data.as_ref().map_or(0, Vec::len),
        Step::FieldViews { views, .. } => 32 + 16 * views.len(),
        Step::Restyle { inlines: v, .. } => 32 + inlines(v),
        Step::Blocks { with, .. } => 32 + blocks(with),
        Step::Note { blocks: b, .. } | Step::HeaderPart { blocks: b, .. } | Step::Separator { blocks: b, .. } => 32 + b.as_deref().map_or(0, blocks),
        Step::Sections { split, .. } => 32 + split.stories().map(blocks).sum::<usize>(),
        Step::BuildingBlock { part, .. } => 64 + part.as_ref().map_or(0, |p| p.name.len() + blocks(&p.body)),
        Step::Rows { with, .. } => 32 + with.iter().flat_map(|r| r.cells.iter()).map(|c| 64 + blocks(&c.blocks)).sum::<usize>(),
        Step::Control { control, .. } => 32 + control.as_ref().map_or(0, |c| c.open.len() + c.close.len() + 128),
        Step::DataPart { part, .. } => 32 + part.as_ref().map_or(0, |p| p.xml.len()),
        Step::Comment { after, .. } => 128 + after.as_ref().map_or(0, |c| blocks(&c.body)),
    }
}

/// What a retained style, numbering and theme table holds: its entries and the XML each keeps as read.
fn resolver_bytes(r: &Resolver) -> usize {
    use std::mem::size_of;
    let styles = r.styles.by_id.values().map(|s| size_of::<Style>() + s.raw.len()).sum::<usize>();
    let n = &r.numbering;
    let abstracts =
        n.abstracts.values().map(|a| size_of::<AbstractNum>() + a.raw.len() + a.levels.len() * size_of::<Option<NumLevel>>()).sum::<usize>();
    let nums = n.nums.values().map(|m| size_of::<Num>() + m.raw.len() + m.level_overrides.len() * size_of::<NumLevel>()).sum::<usize>();
    let bullets = n.pic_bullets.values().map(|b| 64 + b.raw.len()).sum::<usize>();
    size_of::<Resolver>() + styles + abstracts + nums + bullets + n.root.len() + r.style_links.len() * 48
}

#[test]
fn added_lists_count_towards_the_budget() {
    let list = crate::docx::AddedList { abstract_id: 1, num_id: 1, abstract_xml: "x".repeat(10_000) };
    assert!(step_bytes(&Step::Resolver { resolver: None, added: vec![list] }) > 10_000);
}
