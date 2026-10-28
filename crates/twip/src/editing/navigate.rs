//! Caret movement: logical units from the model, lines and pages from the geometry of the snapshot shown. Movement between cells and tables follows the containment structure and the table grid, never drawing order.

use std::rc::Rc;

use crate::docx::{Block, Document, Table};
use crate::layout::{ParaId, TableId, Unit};

use super::geometry::{PageGeometry, Pages};
use super::index::{Container, DocIndex};
use super::position::{Affinity, Place, Position, StoryId};
use super::step::para_len;
use super::table::Grid;
use super::text::UnitText;

/// What navigation reads: the document, its index and page geometry, and the page the caret was last on.
pub struct Nav<'a> {
    pub doc: &'a Document,
    pub idx: &'a DocIndex,
    pub pages: &'a mut Pages,
    pub near: usize,
}

/// A caret's line: page, line index on it, and the page's geometry.
pub struct LineAt {
    pub page: usize,
    pub line: usize,
    pub geo: Rc<PageGeometry>,
}

impl Nav<'_> {
    fn text(&self, para: ParaId) -> Option<UnitText> {
        self.idx.para(self.doc, para).map(UnitText::of)
    }

    fn story_of_container(&self, c: Container) -> StoryId {
        self.idx.story(c)
    }

    // ---------------- structure: first and last places ----------------

    /// The first caret place of a block: a paragraph's start, a table's first cell's first place.
    fn block_start(&self, story: StoryId, b: &Block) -> Option<Position> {
        match b {
            Block::Para(p) => Some(Position::text(story, ParaId(p.id), Unit(0))),
            Block::Table(t) => self.cell_first(story, t, 0, self.first_cell(t, 0)?),
        }
    }

    /// The last caret place of a block: a paragraph's end, a table's last row's end-of-row mark.
    fn block_end(&self, story: StoryId, b: &Block) -> Option<Position> {
        match b {
            Block::Para(p) => Some(Position::text(story, ParaId(p.id), para_len(p))),
            Block::Table(t) => Some(Position::row_end(story, TableId(t.id), t.rows.len().checked_sub(1)?)),
        }
    }

    fn first_cell(&self, t: &Table, row: usize) -> Option<usize> {
        let g = Grid::of(t);
        (0..t.rows.get(row)?.cells.len()).find(|&c| !g.continues(row, c))
    }

    fn cell_first(&self, story: StoryId, t: &Table, row: usize, cell: usize) -> Option<Position> {
        t.rows.get(row)?.cells.get(cell)?.blocks.first().and_then(|b| self.block_start(story, b))
    }

    fn cell_last(&self, story: StoryId, t: &Table, row: usize, cell: usize) -> Option<Position> {
        t.rows.get(row)?.cells.get(cell)?.blocks.last().and_then(|b| self.block_end(story, b))
    }

    /// The first place after block `block` of `container` in reading order: the next block, the next
    /// cell (merge continuations skipped), the row's end mark after its last cell.
    fn after(&self, container: Container, block: usize) -> Option<Position> {
        let story = self.story_of_container(container);
        let blocks = self.idx.blocks(self.doc, container)?;
        if let Some(b) = blocks.get(block + 1) {
            return self.block_start(story, b);
        }
        let Container::Cell { table, row, cell } = container else { return None };
        let t = self.idx.table(self.doc, table)?;
        let g = Grid::of(t);
        match (cell + 1..t.rows[row].cells.len()).find(|&c| !g.continues(row, c)) {
            Some(c) => self.cell_first(story, t, row, c),
            None => Some(Position::row_end(story, table, row)),
        }
    }

    /// The place after a row's end mark: the next row's first cell, or after the table.
    fn after_row(&self, story: StoryId, table: TableId, row: usize) -> Option<Position> {
        let t = self.idx.table(self.doc, table)?;
        if row + 1 < t.rows.len() {
            return match self.first_cell(t, row + 1) {
                Some(c) => self.cell_first(story, t, row + 1, c),
                None => Some(Position::row_end(story, table, row + 1)),
            };
        }
        let s = self.idx.table_slot(table)?;
        self.after(s.container, s.block)
    }

    /// The last place before block `block` of `container` (the mirror of [`Nav::after`]).
    fn before(&self, container: Container, block: usize) -> Option<Position> {
        let story = self.story_of_container(container);
        let blocks = self.idx.blocks(self.doc, container)?;
        if block > 0 {
            return self.block_end(story, &blocks[block - 1]);
        }
        let Container::Cell { table, row, cell } = container else { return None };
        let t = self.idx.table(self.doc, table)?;
        let g = Grid::of(t);
        match (0..cell).rev().find(|&c| !g.continues(row, c)) {
            Some(c) => self.cell_last(story, t, row, c),
            None if row > 0 => Some(Position::row_end(story, table, row - 1)),
            None => {
                let s = self.idx.table_slot(table)?;
                self.before(s.container, s.block)
            }
        }
    }

    /// The place before a row's end mark: its last cell's last place.
    fn before_row_end(&self, story: StoryId, table: TableId, row: usize) -> Option<Position> {
        let t = self.idx.table(self.doc, table)?;
        let g = Grid::of(t);
        let c = (0..t.rows.get(row)?.cells.len()).rev().find(|&c| !g.continues(row, c))?;
        self.cell_last(story, t, row, c)
    }

    // ---------------- logical movement ----------------

    /// Right: one grapheme, or with `word` to the next word start; at a paragraph's end, the next
    /// place in reading order.
    pub fn right(&self, pos: Position, word: bool) -> Position {
        match pos.place {
            Place::Text { para, offset, .. } => {
                let Some(t) = self.text(para) else { return pos };
                if offset < t.len() {
                    let u = if word { t.next_word(offset) } else { t.next_grapheme(offset) };
                    return Position::text(pos.story, para, u);
                }
                let Some(s) = self.idx.para_slot(para) else { return pos };
                self.after(s.container, s.block).unwrap_or(pos)
            }
            Place::RowEnd { table, row } => self.after_row(pos.story, table, row).unwrap_or(pos),
        }
    }

    /// Left: the mirror of [`Nav::right`].
    pub fn left(&self, pos: Position, word: bool) -> Position {
        match pos.place {
            Place::Text { para, offset, .. } => {
                let Some(t) = self.text(para) else { return pos };
                if offset > Unit(0) {
                    let u = if word { t.prev_word(offset) } else { t.prev_grapheme(offset) };
                    return Position::text(pos.story, para, u);
                }
                let Some(s) = self.idx.para_slot(para) else { return pos };
                self.before(s.container, s.block).unwrap_or(pos)
            }
            Place::RowEnd { table, row } => self.before_row_end(pos.story, table, row).unwrap_or(pos),
        }
    }

    /// Ctrl+Down: the start of the next paragraph in document order; a cell's end and a row's end
    /// mark count as paragraphs, so from a row's last cell it stops at the mark.
    pub fn next_para(&self, pos: Position) -> Position {
        self.right(self.para_end(pos), false)
    }

    /// Ctrl+Up: the start of this paragraph, or else the previous paragraph's start (or a row end mark before it).
    pub fn prev_para(&self, pos: Position) -> Position {
        match pos.place {
            Place::Text { para, offset, .. } if offset > Unit(0) => Position::text(pos.story, para, Unit(0)),
            _ => {
                let p = self.left(pos, false);
                match p.place {
                    Place::Text { para, .. } => Position::text(p.story, para, Unit(0)),
                    Place::RowEnd { .. } => p,
                }
            }
        }
    }

    fn para_end(&self, pos: Position) -> Position {
        match pos.place {
            Place::Text { para, .. } => match self.idx.para(self.doc, para) {
                Some(p) => Position::text(pos.story, para, para_len(p)),
                None => pos,
            },
            Place::RowEnd { .. } => pos,
        }
    }

    /// Ctrl+Home / Ctrl+End: the start or end of the story.
    pub fn story_edge(&self, story: StoryId, end: bool) -> Option<Position> {
        let roots: Vec<Container> = match story {
            StoryId::Body => (0..self.doc.sections.len()).map(Container::Section).collect(),
            s => vec![Container::Story(s)],
        };
        let pick = |c: &Container| -> Option<Position> {
            let blocks = self.idx.blocks(self.doc, *c)?;
            if end { blocks.last().and_then(|b| self.block_end(story, b)) } else { blocks.first().and_then(|b| self.block_start(story, b)) }
        };
        if end { roots.iter().rev().find_map(pick) } else { roots.iter().find_map(pick) }
    }

    /// Alt+Home/End (`row`) and Alt+Page Up/Down: the start of the first or last cell of the caret's
    /// row, or of its column, in its innermost table.
    pub fn table_edge(&self, pos: Position, row_wise: bool, end: bool) -> Option<Position> {
        let level = *self.idx.chain_of(&pos).last()?;
        let t = self.idx.table(self.doc, level.table)?;
        let g = Grid::of(t);
        let (r, c) = if row_wise {
            let mut cells = (0..t.rows.get(level.row)?.cells.len()).filter(|&c| !g.continues(level.row, c));
            (level.row, if end { cells.next_back()? } else { cells.min()? })
        } else {
            let cell = level.cell.min(t.rows.get(level.row)?.cells.len().checked_sub(1)?);
            let col = g.col_of(level.row, cell)?;
            let mut rows = (0..t.rows.len()).filter_map(|r| g.cell_at(r, col).map(|c| g.merge_start(r, c)));
            if end { rows.next_back()? } else { rows.min()? }
        };
        self.cell_first(pos.story, t, r, c)
    }

    // ---------------- geometry ----------------

    /// The line showing a position: on the page nearest the caret's last page.
    pub fn line_of(&mut self, pos: Position) -> Option<LineAt> {
        let (para, unit, affinity) = match pos.place {
            Place::Text { para, offset, affinity } => (para, offset, affinity),
            Place::RowEnd { .. } => return None,
        };
        let (page, geo, line) = self.pages.line_showing(para, unit, affinity, self.near, self.idx)?;
        Some(LineAt { page, line, geo })
    }

    /// Home / End: the start or end of the caret's line; End shows at the line's end (upstream).
    pub fn line_edge(&mut self, pos: Position, end: bool) -> Option<Position> {
        let at = self.line_of(pos)?;
        let l = &at.geo.lines[at.line];
        let stop = if end { l.stops.last()? } else { l.stops.first()? };
        let wraps = end && at.geo.lines.iter().any(|m| m.first_of(stop.para) == Some(stop.unit) && m.top > l.top);
        Some(Position {
            story: pos.story,
            place: Place::Text { para: stop.para, offset: stop.unit, affinity: if wraps { Affinity::Upstream } else { Affinity::Downstream } },
        })
    }

    /// The caret's x on its line.
    pub fn caret_x(&mut self, pos: Position) -> Option<f64> {
        let at = self.line_of(pos)?;
        let (para, unit) = pos.para_offset()?;
        at.geo.lines[at.line].x_of(para, unit)
    }

    /// The position on paragraph `para`'s first (or last) line nearest `x`.
    fn on_line(&mut self, story: StoryId, para: ParaId, x: f64, last: bool) -> Option<Position> {
        let pages = self.pages.pages_of(para, self.near, self.idx);
        let page = if last { *pages.last()? } else { *pages.first()? };
        let geo = self.pages.page(page, self.idx)?;
        let line = if last { geo.lines_of(para).last() } else { geo.lines_of(para).next() };
        let Some((_, line)) = line else {
            // A cell's closing paragraph after a nested table is drawn without lines; the caret still stops there.
            return geo.cells.iter().any(|c| c.closing.is_some_and(|(p, _)| p == para)).then(|| Position::text(story, para, Unit(0)));
        };
        let stop = line.nearest(x)?;
        let wraps = stop.end && geo.lines.iter().any(|m| m.first_of(stop.para) == Some(stop.unit) && m.top > line.top);
        Some(Position {
            story,
            place: Place::Text { para: stop.para, offset: stop.unit, affinity: if wraps { Affinity::Upstream } else { Affinity::Downstream } },
        })
    }

    /// Up / Down with goal x: the next line of the paragraph, else the
    /// first (last) line of the next (previous) content in the containment structure, entering tables at the cell
    /// under the goal x and leaving them to the cell below (above) by grid column.
    pub fn vertical(&mut self, pos: Position, x: f64, down: bool) -> Option<Position> {
        let mut from = pos.para_offset().map(|(p, _)| p);
        if let Some(at) = self.line_of(pos) {
            let l = &at.geo.lines[at.line];
            // A line showing several paragraphs (REV-6) moves on from its last one going down, its first going up.
            let para = if down { l.paras.last() } else { l.paras.first() }.copied().or(from)?;
            from = Some(para);
            // Another line of the same paragraph, on this page or the next.
            let same: Vec<usize> = at.geo.lines_of(para).map(|(i, _)| i).collect();
            let k = same.iter().position(|&i| i == at.line)?;
            let next = if down { same.get(k + 1) } else { k.checked_sub(1).and_then(|j| same.get(j)) };
            if let Some(&i) = next {
                let stop = at.geo.lines[i].nearest(x)?;
                return Some(Position::text(pos.story, stop.para, stop.unit));
            }
            let page = if down { Some(at.page + 1) } else { at.page.checked_sub(1) };
            if let Some(g) = page.and_then(|page| self.pages.page(page, self.idx)) {
                let lines: Vec<usize> = g.lines_of(para).map(|(i, _)| i).collect();
                if let Some(&i) = if down { lines.first() } else { lines.last() } {
                    let stop = g.lines[i].nearest(x)?;
                    return Some(Position::text(pos.story, stop.para, stop.unit));
                }
            }
        }
        // Past the paragraph (or a row end mark): the structural neighbour.
        let target = match pos.place {
            Place::Text { .. } => {
                let para = from?;
                let s = self.idx.para_slot(para)?;
                if down { self.below(s.container, s.block, x)? } else { self.above(s.container, s.block, x)? }
            }
            Place::RowEnd { table, row } => {
                let s = self.idx.table_slot(table)?;
                let t = self.idx.table(self.doc, table)?;
                if down {
                    if row + 1 < t.rows.len() { self.enter_row(pos.story, t, row + 1, x, true)? } else { self.below(s.container, s.block, x)? }
                } else if row > 0 {
                    self.enter_row(pos.story, t, row - 1, x, false)?
                } else {
                    self.above(s.container, s.block, x)?
                }
            }
        };
        Some(target)
    }

    /// The first line below block `block` of `container` at x.
    fn below(&mut self, container: Container, block: usize, x: f64) -> Option<Position> {
        let story = self.story_of_container(container);
        let blocks = self.idx.blocks(self.doc, container)?;
        if let Some(b) = blocks.get(block + 1) {
            return self.enter(story, b, x, true);
        }
        let Container::Cell { table, row, cell } = container else { return None };
        let t = self.idx.table(self.doc, table)?;
        let g = Grid::of(t);
        let col = self.column_at(t, row, cell, x, &g);
        match g.below(row, col) {
            Some((r, _)) => self.enter_row(story, t, r, x, true),
            None => {
                let s = self.idx.table_slot(table)?;
                self.below(s.container, s.block, x)
            }
        }
    }

    /// The last line above block `block` of `container` at x.
    fn above(&mut self, container: Container, block: usize, x: f64) -> Option<Position> {
        let story = self.story_of_container(container);
        let blocks = self.idx.blocks(self.doc, container)?;
        if block > 0 {
            return self.enter(story, &blocks[block - 1], x, false);
        }
        let Container::Cell { table, row, cell } = container else { return None };
        let t = self.idx.table(self.doc, table)?;
        let g = Grid::of(t);
        let col = self.column_at(t, row, cell, x, &g);
        match g.above(row, col) {
            Some((r, _)) => self.enter_row(story, t, r, x, false),
            None => {
                let s = self.idx.table_slot(table)?;
                self.above(s.container, s.block, x)
            }
        }
    }

    /// The grid column under x in a row (the cell's own column when x is outside the row's drawn cells).
    fn column_at(&mut self, t: &Table, row: usize, cell: usize, x: f64, g: &Grid) -> usize {
        let own = g.col_of(row, cell).unwrap_or(0);
        let Some(c) = self.cell_rect_cell(TableId(t.id), row, x) else { return own };
        g.col_of(row, c).unwrap_or(own)
    }

    /// The cell of a row whose drawn box spans x (on any page the row is drawn on).
    fn cell_rect_cell(&mut self, table: TableId, row: usize, x: f64) -> Option<usize> {
        for i in 0..self.pages.len() {
            let page = (self.near + i) % self.pages.len().max(1);
            let Some(geo) = self.pages.page(page, self.idx) else { continue };
            if !geo.shows_table(table) {
                continue;
            }
            let cells: Vec<(usize, [f64; 4])> = geo
                .cells
                .iter()
                .filter(|c| c.chain.last().is_some_and(|k| k.0 == table && k.1 == row))
                .map(|c| (c.chain.last().map_or(0, |k| k.2), c.rect))
                .collect();
            if cells.is_empty() {
                continue;
            }
            return cells
                .iter()
                .find(|(_, r)| x >= r[0] && x < r[2])
                .or_else(|| {
                    cells.iter().min_by(|a, b| (a.1[0] - x).abs().min((a.1[2] - x).abs()).total_cmp(&(b.1[0] - x).abs().min((b.1[2] - x).abs())))
                })
                .map(|c| c.0);
        }
        None
    }

    /// Into a block from above (`down`) or below: a paragraph's first (last) line, a table's first (last) row.
    fn enter(&mut self, story: StoryId, b: &Block, x: f64, down: bool) -> Option<Position> {
        match b {
            Block::Para(p) => self.on_line(story, ParaId(p.id), x, !down),
            Block::Table(t) => {
                let row = if down { 0 } else { t.rows.len().checked_sub(1)? };
                self.enter_row(story, t, row, x, down)
            }
        }
    }

    /// Into the cell of `row` under x, at its first (last) content; a merge continuation means its region's first
    /// cell.
    fn enter_row(&mut self, story: StoryId, t: &Table, row: usize, x: f64, down: bool) -> Option<Position> {
        let g = Grid::of(t);
        let cell = self.cell_rect_cell(TableId(t.id), row, x).or_else(|| self.first_cell(t, row))?;
        let (r, c) = g.merge_start(row, cell);
        let blocks = &t.rows.get(r)?.cells.get(c)?.blocks;
        let b = if down { blocks.first()? } else { blocks.last()? };
        self.enter(story, b, x, down)
    }
}
