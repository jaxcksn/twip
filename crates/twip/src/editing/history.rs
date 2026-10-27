//! Undo and redo: transactions of steps, typing runs, the memory budget and the repeatable command.

use std::collections::VecDeque;

use crate::docx::{Block, Inline};
use crate::layout::{ParaId, Unit};

use super::index::Level;
use super::selection::Selection;
use super::step::Step;

/// The selection a transaction restores, with the table context it was in.
#[derive(Clone, Debug, PartialEq)]
pub struct SelectionRecord {
    pub selection: Selection,
    pub chain: Vec<Level>,
}

/// One undo step: the steps that reverse it (applied last to first), and the selections before and after it.
#[derive(Debug)]
pub struct Transaction {
    pub undo: Vec<Step>,
    pub before: SelectionRecord,
    pub after: SelectionRecord,
    /// What the undo menu shows: a command's name, or the label a host's batch gave.
    pub label: std::borrow::Cow<'static, str>,
    pub bytes: usize,
}

/// Consecutive typing in one paragraph, which joins one undo step; deleting keys don't join.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TypingRun {
    pub para: ParaId,
    /// Where the next keystroke continues the run.
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
    /// The typing run the last undo step can still take.
    typing: Option<TypingRun>,
    /// Estimated bytes the history may hold; at least [`MIN_STEPS`] steps are kept regardless.
    budget: usize,
    bytes: usize,
}

/// Word's default undo depth.
pub const MIN_STEPS: usize = 100;

impl Default for History {
    fn default() -> History {
        History { undo: VecDeque::new(), redo: Vec::new(), typing: None, budget: 32 << 20, bytes: 0 }
    }
}

impl History {
    /// Records a transaction. With `typing` continuing the last step's run, it joins that step (one undo).
    pub fn record(&mut self, mut tx: Transaction, typing: Option<TypingRun>) {
        self.redo.clear();
        let joins = match (typing, self.typing) {
            // Typing joins; each Backspace or Delete is its own step.
            (Some(t), Some(last)) => t.kind == Typing::Insert && t.para == last.para && t.kind == last.kind && self.undo.back().is_some(),
            _ => false,
        };
        if joins && let Some(last) = self.undo.back_mut() {
            // The new keystroke's inverse runs first when undoing, then the run's earlier ones.
            last.undo.append(&mut tx.undo);
            last.after = tx.after;
            last.bytes += tx.bytes;
            self.bytes += tx.bytes;
        } else {
            self.bytes += tx.bytes;
            self.undo.push_back(tx);
        }
        self.typing = typing.map(|t| TypingRun { at: t.at, ..t });
        while self.bytes > self.budget && self.undo.len() > MIN_STEPS {
            if let Some(old) = self.undo.pop_front() {
                self.bytes -= old.bytes;
            }
        }
    }

    /// Ends the current typing run: the next typing starts a new undo step (a caret move, a command, a click).
    pub fn break_typing(&mut self) {
        self.typing = None;
    }

    /// Whether `t` would continue the current typing run.
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

    /// An undone transaction becomes redoable (its steps now redo it).
    pub fn push_redo(&mut self, tx: Transaction) {
        self.redo.push(tx);
    }

    /// A redone transaction goes back on the undo stack without clearing redo.
    pub fn push_undo_keep_redo(&mut self, tx: Transaction) {
        self.bytes += tx.bytes;
        self.undo.push_back(tx);
    }

    /// The last `n` transactions as one, labelled `label` (a dialog's commands are one undo step).
    pub fn merge_last(&mut self, n: usize, label: impl Into<std::borrow::Cow<'static, str>>) {
        if n < 2 || self.undo.len() < n {
            return;
        }
        let mut txs: Vec<Transaction> = self.undo.drain(self.undo.len() - n..).collect();
        let last = txs.pop().unwrap();
        let mut first = txs.remove(0);
        for t in txs {
            first.undo.extend(t.undo);
            first.bytes += t.bytes;
        }
        first.undo.extend(last.undo);
        first.bytes += last.bytes;
        first.after = last.after;
        first.label = label.into();
        self.undo.push_back(first);
        self.typing = None;
    }

    /// Nothing to redo (a rolled-back batch's undone steps aren't redone).
    pub fn clear_redo(&mut self) {
        self.redo.clear();
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
}

/// A rough size of what a step holds, for the history budget.
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
        Step::Join { .. } | Step::Para { .. } | Step::TableWidth { .. } | Step::TableProps { .. } | Step::SectionPage { .. } => 32,
        Step::ImageData { data, .. } => 32 + data.len(),
        Step::Restyle { inlines: v, .. } => 32 + inlines(v),
        Step::Blocks { with, .. } => 32 + blocks(with),
        Step::Note { blocks: b, .. } | Step::HeaderPart { blocks: b, .. } => 32 + b.as_deref().map_or(0, blocks),
        Step::Rows { with, .. } => 32 + with.iter().flat_map(|r| r.cells.iter()).map(|c| 64 + blocks(&c.blocks)).sum::<usize>(),
        Step::Control { control, .. } => 32 + control.as_ref().map_or(0, |c| c.open.len() + c.close.len() + 128),
        Step::DataPart { part, .. } => 32 + part.as_ref().map_or(0, |p| p.xml.len()),
    }
}
