//! Input nodes

use std::collections::{HashMap, HashSet};

use crate::docx::review::MarkupView;
use crate::docx::{Block, Inline, NoteKind, ParaStyle, Section};

pub struct Blocks<'a> {
    pub blocks: Vec<&'a Block>,
    pub stories: Vec<usize>,
}

impl<'a> Blocks<'a> {
    pub fn of(blocks: &'a [Block]) -> Blocks<'a> {
        Blocks { blocks: blocks.iter().collect(), stories: Vec::new() }
    }

    pub fn append(&mut self, story: impl IntoIterator<Item = &'a Block>) {
        self.stories.push(self.blocks.len());
        self.blocks.extend(story);
    }

    pub fn chain_end(&self, i: usize, view: MarkupView) -> usize {
        let story_end = self.stories.iter().copied().find(|&s| s > i).unwrap_or(self.blocks.len()).min(self.blocks.len());
        super::join::chain_end(&self.blocks[..story_end], i, view)
    }

    pub fn len(&self) -> usize {
        self.blocks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }

    pub fn get(&self, i: usize) -> Option<&'a Block> {
        self.blocks.get(i).copied()
    }

    pub fn neighbours(&self, first: usize, last: usize, view: MarkupView) -> (Option<&'a ParaStyle>, Option<&'a ParaStyle>) {
        let style = |b: Option<&&'a Block>| match b {
            Some(Block::Para(p)) => Some(&**p.para_style_in(view)),
            _ => None,
        };
        let prev = if first > 0 { style(self.blocks.get(first - 1)) } else { None };
        (prev, style(self.blocks.get(self.chain_end(last + 1, view))))
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
