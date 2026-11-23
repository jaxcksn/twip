//! Input nodes

use std::collections::{HashMap, HashSet};

use crate::docx::review::MarkupView;
use std::rc::Rc;

use crate::docx::{Block, Inline, NoteKind, ParaStyle, Paragraph, Section};

/// A section's blocks, then the stories laid out after them (endnotes), as one run of blocks.
pub struct Blocks<'a> {
    /// The section's own blocks, borrowed.
    body: &'a [Block],
    /// The stories' blocks after them.
    tail: Vec<&'a Block>,
    /// Where each story starts.
    pub stories: Vec<usize>,
}

impl<'a> Blocks<'a> {
    pub fn of(blocks: &'a [Block]) -> Blocks<'a> {
        Blocks { body: blocks, tail: Vec::new(), stories: Vec::new() }
    }

    pub fn append(&mut self, story: impl IntoIterator<Item = &'a Block>) {
        self.stories.push(self.len());
        self.tail.extend(story);
    }

    /// The first `n` blocks, without the stories.
    pub fn first(&self, n: usize) -> Blocks<'a> {
        let b = self.body.len();
        Blocks { body: &self.body[..n.min(b)], tail: self.tail[..n.saturating_sub(b).min(self.tail.len())].to_vec(), stories: Vec::new() }
    }

    pub fn chain_end(&self, i: usize, view: MarkupView) -> usize {
        let (b, story_end) = (self.body.len(), self.story_end(i));
        match i < b {
            true => super::join::chain_end(&self.body[..story_end.min(b)], i, view),
            false => b + super::join::chain_end(&self.tail[..story_end.saturating_sub(b)], i - b, view),
        }
    }

    pub fn len(&self) -> usize {
        self.body.len() + self.tail.len()
    }

    pub fn is_empty(&self) -> bool {
        self.body.is_empty() && self.tail.is_empty()
    }

    pub fn get(&self, i: usize) -> Option<&'a Block> {
        match self.body.get(i) {
            Some(b) => Some(b),
            None => self.tail.get(i - self.body.len()).copied(),
        }
    }

    /// Every block, in order.
    pub fn iter(&self) -> impl Iterator<Item = &'a Block> + '_ {
        self.body.iter().chain(self.tail.iter().copied())
    }

    /// Blocks `range`, as many as there are.
    pub fn range(&self, range: std::ops::Range<usize>) -> impl Iterator<Item = &'a Block> + '_ {
        let (b, t) = (self.body.len(), self.tail.len());
        let body = &self.body[range.start.min(b)..range.end.clamp(range.start.min(b), b)];
        let (s, e) = (range.start.saturating_sub(b).min(t), range.end.saturating_sub(b).min(t));
        body.iter().chain(self.tail[s..e.max(s)].iter().copied())
    }

    /// Blocks `first..=last` of one story laid out as one paragraph (see [`super::join::laid_para`]).
    pub fn laid_para(&self, first: usize, last: usize, legacy: bool) -> Option<super::join::LaidPara<'_>> {
        let b = self.body.len();
        match last < b {
            true => super::join::laid_para(self.body.get(first..=last)?, legacy),
            false => super::join::laid_para(self.tail.get(first.checked_sub(b)?..=last - b)?, legacy),
        }
    }

    /// The end of the story holding block `i`.
    fn story_end(&self, i: usize) -> usize {
        self.stories.iter().copied().find(|&s| s > i).unwrap_or(self.len()).min(self.len())
    }

    /// [`super::join::next_shown_mark`] from block `from` in the story holding block `from - 1`.
    pub fn next_shown_mark(&self, from: usize, view: MarkupView) -> Option<&Paragraph> {
        let (b, end) = (self.body.len(), self.story_end(from.saturating_sub(1)));
        match from <= b {
            true => super::join::next_shown_mark(&self.body[..end.min(b)], from, view),
            false => super::join::next_shown_mark(&self.tail[..end - b], from - b, view),
        }
    }

    /// [`super::join::joined_target`] of a chain ending at block `end`, in its story.
    pub fn joined_target<'s>(&'s self, end: usize, view: MarkupView, last: Option<&'s Rc<ParaStyle>>) -> Option<&'s Rc<ParaStyle>> {
        let (b, story_end) = (self.body.len(), self.story_end(end));
        match end < b {
            true => super::join::joined_target(&self.body[..story_end.min(b)], end, view, last),
            false => super::join::joined_target(&self.tail[..story_end - b], end - b, view, last),
        }
    }

    pub fn neighbours(&self, first: usize, last: usize, view: MarkupView) -> (Option<&'a ParaStyle>, Option<&'a ParaStyle>) {
        let style = |b: Option<&'a Block>| match b {
            Some(Block::Para(p)) => Some(&**p.para_style_in(view)),
            _ => None,
        };
        let prev = if first > 0 { style(self.get(first - 1)) } else { None };
        (prev, style(self.get(self.chain_end(last + 1, view))))
    }
}

pub struct SectionNode<'a> {
    pub index: usize,
    pub section: &'a Section,
    pub blocks: Blocks<'a>,
    pub breaks: bool,
    /// The next section's first paragraph, which the last paragraph's border group looks down to.
    pub next: Option<&'a ParaStyle>,
}

#[derive(Clone, Copy)]
pub struct Notes<'a> {
    pub footnotes: &'a HashMap<i64, Vec<Block>>,
    pub endnotes: &'a HashMap<i64, Vec<Block>>,
    pub separators: &'a HashMap<String, Vec<Block>>,
}

impl<'a> Notes<'a> {
    pub fn blocks(&self, kind: NoteKind, id: i64) -> Option<&'a Vec<Block>> {
        match kind {
            NoteKind::Foot => self.footnotes.get(&id),
            NoteKind::End => self.endnotes.get(&id),
        }
    }
}

/// The number key of an endnote's mark when it differs from its reference's.
pub fn mark_key(kind: NoteKind, id: i64) -> (NoteKind, i64) {
    (kind, !id)
}

/// The block holds a footnote reference.
pub fn has_footnote_refs(b: &Block) -> bool {
    match b {
        Block::Para(p) => p.inlines.iter().any(|i| matches!(i, Inline::NoteRef { kind: NoteKind::Foot, .. })),
        Block::Table(t) => t.rows.iter().flat_map(|r| r.cells.iter()).flat_map(|c| c.blocks.iter()).any(has_footnote_refs),
    }
}

pub fn has_note_refs(b: &Block) -> bool {
    match b {
        Block::Para(p) => p.inlines.iter().any(|i| matches!(i, Inline::NoteRef { .. })),
        Block::Table(t) => t.rows.iter().flat_map(|r| r.cells.iter()).flat_map(|c| c.blocks.iter()).any(has_note_refs),
    }
}

/// Each note's first reference: kind, id, and whether it carries a custom mark.
pub(super) fn collect_refs(blocks: &[Block], out: &mut Vec<(NoteKind, i64, bool)>, seen: &mut HashSet<(NoteKind, i64)>) {
    for b in blocks {
        match b {
            Block::Para(p) => {
                for i in &p.inlines {
                    if let Inline::NoteRef { kind, id, custom, .. } = i
                        && seen.insert((*kind, *id))
                    {
                        out.push((*kind, *id, *custom));
                    }
                }
            }
            Block::Table(t) => {
                for r in &t.rows {
                    for c in &r.cells {
                        collect_refs(&c.blocks, out, seen);
                    }
                }
            }
        }
    }
}
