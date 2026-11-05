//! Document index

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use crate::docx::{Block, Document, ElementId, Inline, Paragraph, Table, TextBox};
use crate::layout::{FloatId, ParaId, TableId};

use super::position::{Place, Position, StoryId};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Container {
    Section(usize),
    Story(StoryId),
    Cell { table: TableId, row: usize, cell: usize },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slot {
    pub container: Container,
    pub block: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Level {
    pub table: TableId,
    pub row: usize,
    pub cell: usize,
    pub depth: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Loc {
    Para(ParaId),
    Table(TableId),
    Row(TableId, usize),
    Cell(TableId, usize, usize),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DocIndex {
    paras: HashMap<ParaId, Slot>,
    tables: HashMap<TableId, Slot>,
    floats: HashMap<FloatId, ParaId>,
    links: HashMap<FloatId, (u32, bool)>,
    elements: HashMap<ElementId, HashSet<Loc>>,
    parts: Vec<String>,
    pub flow: Option<u32>,
}

fn link_of(i: &Inline) -> Option<(u64, (u32, bool))> {
    use crate::drawing::ShapeText;
    let Inline::Float(f, _) = i else { return None };
    let sp = f.shape.as_deref()?.sp()?;
    match (&sp.text, sp.chain) {
        (ShapeText::Linked { chain, .. }, _) => Some((f.id, (*chain, false))),
        (ShapeText::Story, Some(c)) => Some((f.id, (c, true))),
        _ => None,
    }
}

fn story_of(i: &Inline) -> Option<(u64, Option<&TextBox>)> {
    match i {
        Inline::Float(f, _) => Some((f.id, f.text_box.as_ref())),
        Inline::Picture(p, _) => p.text.as_ref().map(|t| (t.id, Some(&t.text_box))),
        _ => None,
    }
}

impl DocIndex {
    pub fn build(doc: &Document) -> DocIndex {
        DocIndex::build_with_parts(doc, &[])
    }

    pub fn build_with_parts(doc: &Document, parts: &[String]) -> DocIndex {
        let mut parts = parts.to_vec();
        parts.extend(doc.stories.keys().filter(|k| !parts.contains(k)).cloned().collect::<Vec<_>>());
        let mut idx = DocIndex { parts, ..DocIndex::default() };
        for (i, s) in doc.sections.iter().enumerate() {
            idx.add_blocks(Container::Section(i), &s.blocks);
        }
        for (k, name) in idx.parts.clone().iter().enumerate() {
            if let Some(blocks) = doc.stories.get(name) {
                idx.add_blocks(Container::Story(StoryId::HeaderFooter(k as u32)), blocks);
            }
        }
        for (id, blocks) in &doc.footnotes {
            idx.add_blocks(Container::Story(StoryId::Footnote(*id)), blocks);
        }
        for (id, blocks) in &doc.endnotes {
            idx.add_blocks(Container::Story(StoryId::Endnote(*id)), blocks);
        }
        for (k, part) in doc.glossary.iter().flat_map(|g| g.parts.iter().enumerate()) {
            if part.state != crate::docx::PartState::Removed {
                idx.add_blocks(Container::Story(StoryId::BuildingBlock(k as u32)), &part.body);
            }
        }
        idx
    }

    fn add_blocks(&mut self, container: Container, blocks: &[Block]) {
        for (block, b) in blocks.iter().enumerate() {
            self.add_block(Slot { container, block }, b);
        }
    }

    fn add_block(&mut self, slot: Slot, b: &Block) {
        match b {
            Block::Para(p) => {
                self.paras.insert(ParaId(p.id), slot);
                self.add_floats(p);
                self.add_para_elements(p);
            }
            Block::Table(t) => {
                self.tables.insert(TableId(t.id), slot);
                let id = TableId(t.id);
                t.controls.iter().for_each(|&c| self.add_loc(ElementId::Control(c), Loc::Table(id)));
                self.add_row_elements(t);
                for (row, r) in t.rows.iter().enumerate() {
                    for (cell, c) in r.cells.iter().enumerate() {
                        self.add_blocks(Container::Cell { table: id, row, cell }, &c.blocks);
                    }
                }
            }
        }
    }

    // Elements

    fn add_loc(&mut self, e: ElementId, at: Loc) {
        self.elements.entry(e).or_default().insert(at);
    }

    fn remove_loc(&mut self, e: ElementId, at: Loc) {
        if let Some(s) = self.elements.get_mut(&e) {
            s.remove(&at);
            if s.is_empty() {
                self.elements.remove(&e);
            }
        }
    }

    fn add_para_elements(&mut self, p: &Paragraph) {
        let at = Loc::Para(ParaId(p.id));
        inline_elements(&p.inlines).for_each(|e| self.add_loc(e, at));
        p.controls.iter().for_each(|&c| self.add_loc(ElementId::Control(c), at));
    }

    fn remove_para_elements(&mut self, p: &Paragraph) {
        let at = Loc::Para(ParaId(p.id));
        inline_elements(&p.inlines).for_each(|e| self.remove_loc(e, at));
        p.controls.iter().for_each(|&c| self.remove_loc(ElementId::Control(c), at));
    }

    fn add_row_elements(&mut self, t: &Table) {
        let id = TableId(t.id);
        for (ri, r) in t.rows.iter().enumerate() {
            r.controls.iter().for_each(|&c| self.add_loc(ElementId::Control(c), Loc::Row(id, ri)));
            for (ci, c) in r.cells.iter().enumerate() {
                c.controls.iter().for_each(|&k| self.add_loc(ElementId::Control(k), Loc::Cell(id, ri, ci)));
            }
        }
    }

    fn remove_row_elements(&mut self, id: TableId) {
        self.elements.retain(|_, s| {
            s.retain(|l| !matches!(l, Loc::Row(t, _) | Loc::Cell(t, _, _) if *t == id));
            !s.is_empty()
        });
    }

    pub fn element(&self, e: ElementId) -> Option<&HashSet<Loc>> {
        self.elements.get(&e)
    }

    pub fn moved_inlines(&mut self, para: ParaId, moved: &[Inline]) {
        inline_elements(moved).for_each(|e| self.remove_loc(e, Loc::Para(para)));
    }

    fn add_floats(&mut self, p: &Paragraph) {
        for i in &p.inlines {
            if let Some((id, l)) = link_of(i) {
                self.links.insert(FloatId(id), l);
            }
            if let Some((id, tb)) = story_of(i) {
                self.floats.insert(FloatId(id), ParaId(p.id));
                if let Some(tb) = tb {
                    self.add_blocks(Container::Story(StoryId::TextBox(FloatId(id))), &tb.blocks);
                }
            }
        }
    }

    fn remove_block(&mut self, b: &Block) {
        match b {
            Block::Para(p) => {
                self.paras.remove(&ParaId(p.id));
                self.remove_floats(&p.inlines);
                self.remove_para_elements(p);
            }
            Block::Table(t) => {
                self.tables.remove(&TableId(t.id));
                t.controls.iter().for_each(|&c| self.remove_loc(ElementId::Control(c), Loc::Table(TableId(t.id))));
                self.remove_row_elements(TableId(t.id));
                for c in t.rows.iter().flat_map(|r| r.cells.iter()) {
                    c.blocks.iter().for_each(|b| self.remove_block(b));
                }
            }
        }
    }

    fn remove_floats(&mut self, inlines: &[Inline]) {
        for i in inlines {
            if let Some((id, _)) = link_of(i) {
                self.links.remove(&FloatId(id));
            }
            if let Some((id, tb)) = story_of(i) {
                self.floats.remove(&FloatId(id));
                if let Some(tb) = tb {
                    tb.blocks.iter().for_each(|b| self.remove_block(b));
                }
            }
        }
    }

    // Updates

    pub fn replaced_blocks(&mut self, doc: &Document, container: Container, removed: &[Block]) {
        removed.iter().for_each(|b| self.remove_block(b));
        let Some(blocks) = self.blocks(doc, container) else { return };
        let mut fresh = Vec::new();
        for (block, b) in blocks.iter().enumerate() {
            let slot = Slot { container, block };
            let known = match b {
                Block::Para(p) => self.paras.get(&ParaId(p.id)),
                Block::Table(t) => self.tables.get(&TableId(t.id)),
            };
            match known {
                Some(s) if s.container == container => {
                    let s = *s;
                    if s.block != block {
                        match b {
                            Block::Para(p) => self.paras.insert(ParaId(p.id), slot),
                            Block::Table(t) => self.tables.insert(TableId(t.id), slot),
                        };
                    }
                }
                _ => fresh.push((slot, b)),
            }
        }
        for (slot, b) in fresh {
            self.add_block(slot, b);
        }
    }

    pub fn replaced_rows(&mut self, doc: &Document, table: TableId, removed: &[crate::docx::Row]) {
        for c in removed.iter().flat_map(|r| r.cells.iter()) {
            c.blocks.iter().for_each(|b| self.remove_block(b));
        }
        self.remove_row_elements(table);
        let Some(t) = self.table(doc, table) else { return };
        self.add_row_elements(t);
        for (row, r) in t.rows.iter().enumerate() {
            for (cell, c) in r.cells.iter().enumerate() {
                self.add_blocks(Container::Cell { table, row, cell }, &c.blocks);
            }
        }
    }

    pub fn added_part(&mut self, doc: &Document, name: &str) {
        let k = match self.parts.iter().position(|p| p == name) {
            Some(k) => k,
            None => {
                self.parts.push(name.to_string());
                self.parts.len() - 1
            }
        };
        if let Some(blocks) = doc.stories.get(name) {
            self.add_blocks(Container::Story(StoryId::HeaderFooter(k as u32)), blocks);
        }
    }

    pub fn added_building_block(&mut self, k: u32, body: &[Block]) {
        self.add_blocks(Container::Story(StoryId::BuildingBlock(k)), body);
    }

    pub fn removed_part(&mut self, removed: &[Block]) {
        removed.iter().for_each(|b| self.remove_block(b));
    }

    pub fn parts(&self) -> &[String] {
        &self.parts
    }

    pub fn replaced_inlines(&mut self, doc: &Document, para: ParaId, removed: &[Inline]) {
        self.remove_floats(removed);
        inline_elements(removed).for_each(|e| self.remove_loc(e, Loc::Para(para)));
        if let Some(p) = self.para(doc, para) {
            let p = p.clone();
            self.add_floats(&p);
            inline_elements(&p.inlines).for_each(|e| self.add_loc(e, Loc::Para(para)));
        }
    }

    // Lookups

    pub fn para_slot(&self, id: ParaId) -> Option<Slot> {
        self.paras.get(&id).copied()
    }

    pub fn table_slot(&self, id: TableId) -> Option<Slot> {
        self.tables.get(&id).copied()
    }

    pub fn float_anchor(&self, id: FloatId) -> Option<ParaId> {
        self.floats.get(&id).copied()
    }

    pub fn header_footer(&self, name: &str) -> Option<StoryId> {
        self.parts.iter().position(|p| p == name).map(|k| StoryId::HeaderFooter(k as u32))
    }

    pub fn part_name(&self, k: u32) -> Option<&str> {
        self.parts.get(k as usize).map(String::as_str)
    }

    pub fn story(&self, container: Container) -> StoryId {
        match container {
            Container::Section(_) => StoryId::Body,
            Container::Story(s) => s,
            Container::Cell { table, .. } => self.table_slot(table).map_or(StoryId::Body, |s| self.story(s.container)),
        }
    }

    pub fn host(&self, mut s: StoryId) -> StoryId {
        while let StoryId::TextBox(f) = s
            && let Some(slot) = self.float_anchor(f).and_then(|p| self.para_slot(p))
        {
            s = self.story(slot.container);
        }
        s
    }

    pub fn chain(&self, container: Container) -> Vec<Level> {
        let mut out = Vec::new();
        let mut c = container;
        while let Container::Cell { table, row, cell } = c {
            out.push(Level { table, row, cell, depth: 0 });
            match self.table_slot(table) {
                Some(s) => c = s.container,
                None => break,
            }
        }
        out.reverse();
        for (k, l) in out.iter_mut().enumerate() {
            l.depth = k + 1;
        }
        out
    }

    pub fn chain_of(&self, pos: &Position) -> Vec<Level> {
        match pos.place {
            Place::Text { para, .. } => self.para_slot(para).map(|s| self.chain(s.container)).unwrap_or_default(),
            Place::RowEnd { table, row } => {
                let mut c = self.table_slot(table).map(|s| self.chain(s.container)).unwrap_or_default();
                c.push(Level { table, row, cell: usize::MAX, depth: c.len() + 1 });
                c
            }
        }
    }

    fn path(&self, container: Container) -> Vec<usize> {
        match container {
            Container::Section(i) => vec![i],
            Container::Story(_) => Vec::new(),
            Container::Cell { table, row, cell } => match self.table_slot(table) {
                Some(s) => {
                    let mut p = self.path(s.container);
                    p.extend([s.block, row, cell]);
                    p
                }
                None => Vec::new(),
            },
        }
    }

    fn order_key(&self, pos: &Position) -> Vec<usize> {
        match pos.place {
            Place::Text { para, offset, .. } => match self.para_slot(para) {
                Some(s) => {
                    let mut k = self.path(s.container);
                    k.extend([s.block, offset.0 as usize]);
                    k
                }
                None => Vec::new(),
            },
            Place::RowEnd { table, row } => match self.table_slot(table) {
                Some(s) => {
                    let mut k = self.path(s.container);
                    k.extend([s.block, row, usize::MAX]);
                    k
                }
                None => Vec::new(),
            },
        }
    }

    pub fn cmp(&self, a: &Position, b: &Position) -> Ordering {
        self.order_key(a).cmp(&self.order_key(b))
    }

    // Model access

    pub fn blocks<'d>(&self, doc: &'d Document, container: Container) -> Option<&'d Vec<Block>> {
        match container {
            Container::Section(i) => doc.sections.get(i).map(|s| &s.blocks),
            Container::Story(s) => self.story_blocks(doc, s),
            Container::Cell { table, row, cell } => Some(&self.table(doc, table)?.rows.get(row)?.cells.get(cell)?.blocks),
        }
    }

    pub fn blocks_mut<'d>(&self, doc: &'d mut Document, container: Container) -> Option<&'d mut Vec<Block>> {
        match container {
            Container::Section(i) => doc.sections.get_mut(i).map(|s| &mut s.blocks),
            Container::Story(s) => self.story_blocks_mut(doc, s),
            Container::Cell { table, row, cell } => Some(&mut self.table_mut(doc, table)?.rows.get_mut(row)?.cells.get_mut(cell)?.blocks),
        }
    }

    fn story_blocks<'d>(&self, doc: &'d Document, s: StoryId) -> Option<&'d Vec<Block>> {
        match s {
            StoryId::Body => None,
            StoryId::HeaderFooter(k) => doc.stories.get(self.part_name(k)?),
            StoryId::Footnote(id) => doc.footnotes.get(&id),
            StoryId::Endnote(id) => doc.endnotes.get(&id),
            StoryId::TextBox(f) => self.text_box(doc, f).map(|tb| &tb.blocks),
            StoryId::BuildingBlock(k) => live_part(doc.glossary.as_ref()?.parts.get(k as usize)?).map(|b| &b.body),
        }
    }

    fn story_blocks_mut<'d>(&self, doc: &'d mut Document, s: StoryId) -> Option<&'d mut Vec<Block>> {
        match s {
            StoryId::Body => None,
            StoryId::HeaderFooter(k) => doc.stories.get_mut(self.part_name(k)?),
            StoryId::Footnote(id) => doc.footnotes.get_mut(&id),
            StoryId::Endnote(id) => doc.endnotes.get_mut(&id),
            StoryId::TextBox(f) => {
                let anchor = self.float_anchor(f)?;
                self.para_mut(doc, anchor)?.inlines.iter_mut().find_map(|i| match i {
                    Inline::Float(fl, _) if FloatId(fl.id) == f => fl.text_box.as_mut().map(|tb| &mut tb.blocks),
                    Inline::Picture(p, _) => p.text.as_mut().filter(|t| FloatId(t.id) == f).map(|t| &mut t.text_box.blocks),
                    _ => None,
                })
            }
            StoryId::BuildingBlock(k) => {
                let part = doc.glossary.as_mut()?.parts.get_mut(k as usize)?;
                (part.state != crate::docx::PartState::Removed).then_some(&mut part.body)
            }
        }
    }

    pub fn box_story(&self, f: FloatId) -> FloatId {
        match self.links.get(&f) {
            Some(&(c, false)) => self.links.iter().find(|(_, l)| **l == (c, true)).map_or(f, |(h, _)| *h),
            _ => f,
        }
    }

    pub fn text_box<'d>(&self, doc: &'d Document, f: FloatId) -> Option<&'d TextBox> {
        self.para(doc, self.float_anchor(f)?)?.inlines.iter().find_map(|i| match story_of(i) {
            Some((id, tb)) if FloatId(id) == f => tb,
            _ => None,
        })
    }

    pub fn para<'d>(&self, doc: &'d Document, id: ParaId) -> Option<&'d Paragraph> {
        let s = self.para_slot(id)?;
        match self.blocks(doc, s.container)?.get(s.block)? {
            Block::Para(p) if p.id == id.0 => Some(p),
            _ => None,
        }
    }

    pub fn para_mut<'d>(&self, doc: &'d mut Document, id: ParaId) -> Option<&'d mut Paragraph> {
        let s = self.para_slot(id)?;
        match self.blocks_mut(doc, s.container)?.get_mut(s.block)? {
            Block::Para(p) if p.id == id.0 => Some(p),
            _ => None,
        }
    }

    pub fn table<'d>(&self, doc: &'d Document, id: TableId) -> Option<&'d Table> {
        let s = self.table_slot(id)?;
        match self.blocks(doc, s.container)?.get(s.block)? {
            Block::Table(t) if t.id == id.0 => Some(t),
            _ => None,
        }
    }

    pub fn table_mut<'d>(&self, doc: &'d mut Document, id: TableId) -> Option<&'d mut Table> {
        let s = self.table_slot(id)?;
        match self.blocks_mut(doc, s.container)?.get_mut(s.block)? {
            Block::Table(t) if t.id == id.0 => Some(t),
            _ => None,
        }
    }

    pub fn top_block(&self, container: Container, block: usize) -> Option<(usize, usize, Option<usize>)> {
        match container {
            Container::Section(i) => Some((i, block, None)),
            Container::Story(StoryId::BuildingBlock(k)) if self.flow == Some(k) => Some((0, block, None)),
            Container::Story(_) => None,
            Container::Cell { table, row, .. } => {
                let s = self.table_slot(table)?;
                match s.container {
                    Container::Section(i) => Some((i, s.block, Some(row))),
                    Container::Story(StoryId::BuildingBlock(k)) if self.flow == Some(k) => Some((0, s.block, Some(row))),
                    c => self.top_block(c, s.block),
                }
            }
        }
    }

    pub fn para_ids(&self) -> impl Iterator<Item = ParaId> + '_ {
        self.paras.keys().copied()
    }
}

fn inline_elements(inlines: &[Inline]) -> impl Iterator<Item = ElementId> + '_ {
    inlines.iter().filter_map(|i| match i {
        Inline::FieldBegin { id, .. } => Some(ElementId::Field(*id)),
        Inline::ControlStart(c) | Inline::Sym { control: Some(c), .. } => Some(ElementId::Control(*c)),
        _ => None,
    })
}

fn live_part(b: &crate::docx::BuildingBlock) -> Option<&crate::docx::BuildingBlock> {
    (b.state != crate::docx::PartState::Removed).then_some(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc() -> Document {
        let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/editor/FX-NEST-3.docx")).expect("FX-NEST-3 fixture");
        crate::docx::read_editable(&bytes).expect("FX-NEST-3 reads")
    }

    #[test]
    fn chains_follow_nesting() {
        let d = doc();
        let idx = DocIndex::build(&d);
        let mut deepest = 0;
        fn walk(blocks: &[Block], depth: usize, idx: &DocIndex, deepest: &mut usize) {
            for b in blocks {
                match b {
                    Block::Para(p) => {
                        let s = idx.para_slot(ParaId(p.id)).expect("indexed");
                        assert_eq!(idx.chain(s.container).len(), depth);
                        *deepest = (*deepest).max(depth);
                    }
                    Block::Table(t) => t.rows.iter().flat_map(|r| r.cells.iter()).for_each(|c| walk(&c.blocks, depth + 1, idx, deepest)),
                }
            }
        }
        walk(&d.sections[0].blocks, 0, &idx, &mut deepest);
        assert_eq!(deepest, 3);
    }

    #[test]
    fn positions_order_by_document_order() {
        let d = doc();
        let idx = DocIndex::build(&d);
        let mut ids = Vec::new();
        fn walk(blocks: &[Block], out: &mut Vec<ParaId>) {
            for b in blocks {
                match b {
                    Block::Para(p) => out.push(ParaId(p.id)),
                    Block::Table(t) => t.rows.iter().flat_map(|r| r.cells.iter()).for_each(|c| walk(&c.blocks, out)),
                }
            }
        }
        walk(&d.sections[0].blocks, &mut ids);
        for w in ids.windows(2) {
            let (a, b) = (Position::text(StoryId::Body, w[0], crate::layout::Unit(0)), Position::text(StoryId::Body, w[1], crate::layout::Unit(0)));
            assert_eq!(idx.cmp(&a, &b), Ordering::Less);
        }
    }
}
