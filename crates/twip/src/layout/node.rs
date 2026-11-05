//! Input nodes

use std::collections::HashMap;

use crate::docx::review::MarkupView;
use crate::docx::{Block, NoteKind, ParaStyle, Section};

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
