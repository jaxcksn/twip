//! Undo and redo

use std::collections::VecDeque;

use crate::docx::{Block, Inline};
use crate::layout::{ParaId, Unit};

use super::index::Level;
use super::selection::Selection;
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
}

pub const MIN_STEPS: usize = 100;

impl Default for History {
    fn default() -> History {
        History { undo: VecDeque::new(), redo: Vec::new(), typing: None, budget: 32 << 20, bytes: 0, ids: 0 }
    }
}

impl History {
    pub fn record(&mut self, mut tx: Transaction, typing: Option<TypingRun>) {
        self.redo.clear();
        self.ids += 1;
        let joins = match (typing, self.typing) {
            (Some(t), Some(last)) => t.kind == Typing::Insert && t.para == last.para && t.kind == last.kind && self.undo.back().is_some(),
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
        while self.bytes > self.budget && self.undo.len() > MIN_STEPS {
            if let Some(old) = self.undo.pop_front() {
                self.bytes -= old.bytes;
            }
        }
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
        Some(tx)
    }

    pub fn pop_redo(&mut self) -> Option<Transaction> {
        self.typing = None;
        self.redo.pop()
    }

    pub fn push_redo(&mut self, tx: Transaction) {
        self.redo.push(tx);
    }

    pub fn push_undo_keep_redo(&mut self, tx: Transaction) {
        self.bytes += tx.bytes;
        self.undo.push_back(tx);
    }

    pub fn undo_depth(&self) -> usize {
        self.undo.len()
    }

    pub fn redo_depth(&self) -> usize {
        self.redo.len()
    }

    pub fn next_undo(&self) -> Option<&str> {
        self.undo.back().map(|t| t.label.as_ref())
    }

    pub fn next_redo(&self) -> Option<&str> {
        self.redo.last().map(|t| t.label.as_ref())
    }

    pub fn bytes(&self) -> usize {
        self.bytes
    }

    pub fn position(&self) -> u64 {
        self.undo.back().map_or(0, |t| t.id)
    }
}

pub fn step_bytes(s: &Step) -> usize {
    fn inlines(v: &[Inline]) -> usize {
        v.iter().map(|i| 48 + if let Inline::Text(t, _) = i { t.len() } else { 0 }).sum()
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
        | Step::TableWidth { .. }
        | Step::TableProps { .. }
        | Step::SectionPage { .. }
        | Step::Resolver { .. }
        | Step::Fonts { .. }
        | Step::DefaultTab(_)
        | Step::Hyphenation(_)
        | Step::SectionBreak { .. } => 32,
        Step::ImageData { data, .. } => 32 + data.len(),
        Step::AddImage { data, .. } => 32 + data.as_ref().map_or(0, Vec::len),
        Step::FieldViews { views, .. } => 32 + 16 * views.len(),
        Step::Restyle { inlines: v, .. } => 32 + inlines(v),
        Step::Blocks { with, .. } => 32 + blocks(with),
        Step::Note { blocks: b, .. } | Step::HeaderPart { blocks: b, .. } => 32 + b.as_deref().map_or(0, blocks),
        Step::BuildingBlock { part, .. } => 64 + part.as_ref().map_or(0, |p| p.name.len() + blocks(&p.body)),
        Step::Rows { with, .. } => 32 + with.iter().flat_map(|r| r.cells.iter()).map(|c| 64 + blocks(&c.blocks)).sum::<usize>(),
        Step::Control { control, .. } => 32 + control.as_ref().map_or(0, |c| c.open.len() + c.close.len() + 128),
        Step::DataPart { part, .. } => 32 + part.as_ref().map_or(0, |p| p.xml.len()),
        Step::Comment { after, .. } => 128 + after.as_ref().map_or(0, |c| blocks(&c.body)),
    }
}
