//! Input nodes: borrowed views of the content an algorithm lays out, built by the document pass. Algorithms
//! below the document pass see the document only through these.

use std::collections::HashMap;

use crate::docx::review::MarkupView;
use crate::docx::{Block, NoteKind, ParaStyle, Section};

/// A section's or story's blocks in order. A section's may end with blocks owned by the pass (the endnote
/// separator and endnotes, NOTE-10), so it holds references.
pub struct Blocks<'a> {
    pub blocks: Vec<&'a Block>,
    /// Where each story appended to a section's own blocks starts (the endnote separator and each endnote): no
    /// paragraphs join across them (REV-6).
    pub stories: Vec<usize>,
}

impl<'a> Blocks<'a> {
    pub fn of(blocks: &'a [Block]) -> Blocks<'a> {
        Blocks { blocks: blocks.iter().collect(), stories: Vec::new() }
    }

    /// Appends a story's blocks.
    pub fn append(&mut self, story: impl IntoIterator<Item = &'a Block>) {
        self.stories.push(self.blocks.len());
        self.blocks.extend(story);
    }

    /// The last block laid out with block `i` (REV-6): `i`, unless its paragraph joins those after it.
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

    /// The neighbours' styles in `view` of the paragraph laid out from blocks `first..=last` (PARA-7: border boxes join
    /// identical neighbours; REV-2). A paragraph joined to others has its last one's (REV-6).
    pub fn neighbours(&self, first: usize, last: usize, view: MarkupView) -> (Option<&'a ParaStyle>, Option<&'a ParaStyle>) {
        let style = |b: Option<&&'a Block>| match b {
            Some(Block::Para(p)) => Some(&**p.para_style_in(view)),
            _ => None,
        };
        let prev = if first > 0 { style(self.blocks.get(first - 1)) } else { None };
        (prev, style(self.blocks.get(self.chain_end(last + 1, view))))
    }
}

/// A section: its properties and its blocks.
pub struct SectionNode<'a> {
    pub index: usize,
    pub section: &'a Section,
    pub blocks: Blocks<'a>,
}

/// Note content and separators, by id.
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
