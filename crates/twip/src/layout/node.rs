//! Input nodes: borrowed views of the content an algorithm lays out, built by the document pass. Algorithms
//! below the document pass see the document only through these.

use std::collections::HashMap;

use crate::docx::{Block, NoteKind, ParaStyle, Section};

/// A section's or story's blocks in order. A section's may end with blocks owned by the pass (the endnote
/// separator and endnotes, NOTE-10), so it holds references.
pub struct Blocks<'a> {
    pub blocks: Vec<&'a Block>,
}

impl<'a> Blocks<'a> {
    pub fn of(blocks: &'a [Block]) -> Blocks<'a> {
        Blocks { blocks: blocks.iter().collect() }
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

    /// A paragraph's neighbours' styles (PARA-7: border boxes join identical neighbours).
    pub fn neighbours(&self, i: usize) -> (Option<&'a ParaStyle>, Option<&'a ParaStyle>) {
        let style = |b: Option<&&'a Block>| match b {
            Some(Block::Para(p)) => Some(&*p.style),
            _ => None,
        };
        (if i > 0 { style(self.blocks.get(i - 1)) } else { None }, style(self.blocks.get(i + 1)))
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
