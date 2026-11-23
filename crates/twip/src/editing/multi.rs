//! Discontiguous selection

use std::cmp::Ordering;
use std::collections::HashMap;
use std::rc::Rc;

use crate::docx::{Block, Inline, RunStyle};
use crate::layout::{ParaId, Unit};

use super::Editor;
use super::clipboard::Fragment;
use super::commands::{Dir, Outcome};
use super::find::Stories;
use super::position::Position;
use super::position::StoryId;
use super::selection::{CellRef, CellShape, Granularity, Selection};
use super::step::{Map, Step, para_len};
use super::table::Grid;

/// A selection of several ranges and blocks of cells of one story: the range or block made last (the selection
/// itself) and the others. Their number is bounded by the gestures that added them, or by the document's runs for a
/// selection a command builds.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Multi {
    /// The selection the other ranges go with: they are dropped once it is another.
    pub primary: Option<Selection>,
    /// The other ranges, in document order, each apart from the others and from the selection's.
    pub others: Vec<(Position, Position)>,
    /// The other blocks of cells (each a [`Selection::Cells`]), in the order they were added, none twice.
    pub cells: Vec<Selection>,
}

/// A Ctrl click or drag that adds a range to the selection or takes what it goes over out of it, while its clicks
/// go on.
#[derive(Clone, Debug)]
pub(super) struct Adding {
    /// The ranges selected before it, in document order.
    base: Vec<(Position, Position)>,
    /// The blocks of cells selected before it.
    cells: Vec<Selection>,
    /// The selection before it.
    was: Selection,
    /// It started on selected text or a selected cell: it takes what it goes over out of the selection.
    remove: bool,
    /// What a removing drag has gone over so far: a range or a block of cells.
    swept: Option<Selection>,
}

/// Whether command `id` is one Word won't run on a discontiguous selection ("This command is not available for
/// multiple selections"): one that wraps, marks or restructures one place (a comment, a bookmark, a content control,
/// a repeating section, a sub-template, a table from text) or changes a table's rows, columns or table-wide settings.
pub fn single_only(id: crate::api::command::CommandId) -> bool {
    use crate::api::command::CommandId as C;
    matches!(
        id,
        C::AddComment
            | C::InsertBookmark
            | C::InsertControl
            | C::WrapInRepeatingSection
            | C::CreateSubTemplate
            | C::ConvertTextToTable
            | C::InsertRows
            | C::InsertColumns
            | C::DeleteRows
            | C::DeleteColumns
            | C::DeleteTable
            | C::DeleteCells
            | C::MergeCells
            | C::SplitCells
            | C::SplitTable
            | C::ConvertTableToText
            | C::DistributeRows
            | C::DistributeColumns
            | C::MoveRows
            | C::AutoFitColumns
            | C::SetRowProperties
            | C::SetRowHeight
            | C::SetRowCanBreak
            | C::SetHeaderRow
            | C::SetColumnWidth
            | C::SetTableWidth
            | C::SetTableAlignment
            | C::SetTableIndent
            | C::SetTableStyle
            | C::SetTableStyleOptions
            | C::SetTableCellSpacing
            | C::SetTableAltText
            | C::SetTablePosition
    )
}

/// Whether command `id` formats each selected cell (and the cells each selected range is in) alike.
pub fn formats_cells(id: crate::api::command::CommandId) -> bool {
    use crate::api::command::CommandId as C;
    matches!(id, C::SetCellAlignment | C::SetCellShading | C::SetCellBorders | C::SetCellMargins | C::SetCellProperties)
}

/// A block of cells' rows and columns, first to last.
fn bounds(anchor: CellRef, focus: CellRef) -> (usize, usize, usize, usize) {
    (anchor.row.min(focus.row), anchor.row.max(focus.row), anchor.col.min(focus.col), anchor.col.max(focus.col))
}

/// What a drag's selection `s` goes over: a range or a block of cells, none for a caret.
fn swept_by(s: Selection) -> Option<Selection> {
    matches!(s, Selection::Text { .. } | Selection::Cells { .. }).then_some(s)
}

impl Editor {
    /// The other ranges of a discontiguous selection, in document order; empty for a selection of one range.
    pub fn other_ranges(&self) -> &[(Position, Position)] {
        if self.multi.primary == Some(self.selection) { &self.multi.others } else { &[] }
    }

    /// The other blocks of cells of a discontiguous selection; empty for a selection of one range or block.
    pub fn other_cells(&self) -> &[Selection] {
        if self.multi.primary == Some(self.selection) { &self.multi.cells } else { &[] }
    }

    /// Whether the selection is discontiguous.
    pub fn is_discontiguous(&self) -> bool {
        !self.other_ranges().is_empty() || !self.other_cells().is_empty()
    }

    /// Every selected block of cells: the other ones, then the selection when it is one.
    pub fn cell_blocks(&self) -> Vec<Selection> {
        let mut out = self.other_cells().to_vec();
        out.extend(Some(self.selection).filter(|s| matches!(s, Selection::Cells { .. })));
        out
    }

    /// The story the selection is in: its text's, or its cells' table's.
    pub(super) fn selection_story(&self) -> Option<StoryId> {
        match self.selection {
            Selection::Cells { table, .. } => self.index.table_slot(table).map(|s| self.index.story(s.container)),
            s => s.focus().map(|p| p.story),
        }
    }

    /// Where a block of cells starts: its first cell's first paragraph.
    pub(super) fn cells_start(&self, s: &Selection) -> Option<Position> {
        let Selection::Cells { table, anchor, focus, .. } = *s else { return None };
        let t = self.index.table(&self.doc, table)?;
        let (r0, _, c0, _) = bounds(anchor, focus);
        let k = Grid::of(t).cell_at(r0, c0)?;
        let para = t.rows.get(r0)?.cells.get(k)?.blocks.iter().find_map(super::first_para_of)?;
        Some(Position::text(self.index.story(self.index.table_slot(table)?.container), para, Unit(0)))
    }

    /// The cell `p` is in that a block of `blocks` holds, the innermost, as a block of its own.
    fn held_cell(&self, p: &Position, blocks: &[Selection]) -> Option<Selection> {
        self.index.chain_of(p).into_iter().rev().find_map(|l| {
            let t = self.index.table(&self.doc, l.table)?;
            let g = Grid::of(t);
            let col = g.col_of(l.row, l.cell)?;
            let span = g.rows.get(l.row)?.get(l.cell)?.1.max(1);
            let holds = |s: &Selection| match *s {
                Selection::Cells { table, anchor, focus, .. } if table == l.table => {
                    let (r0, r1, c0, c1) = bounds(anchor, focus);
                    (r0..=r1).contains(&l.row) && col + span > c0 && col <= c1
                }
                _ => false,
            };
            blocks.iter().any(holds).then(|| Selection::Cells {
                table: l.table,
                anchor: CellRef { row: l.row, col },
                focus: CellRef { row: l.row, col: col + span - 1 },
                shape: CellShape::Rect,
            })
        })
    }

    /// Whether `p` is in a selected cell.
    pub(super) fn in_cell_blocks(&self, p: &Position) -> bool {
        self.held_cell(p, &self.cell_blocks()).is_some()
    }

    /// Block `s` without the cells of block `cut`: up to four blocks, merged cells whole.
    fn cells_without(&self, s: Selection, cut: Selection) -> Vec<Selection> {
        let (Selection::Cells { table, anchor, focus, .. }, Selection::Cells { table: ct, anchor: ca, focus: cf, .. }) = (s, cut) else {
            return vec![s];
        };
        let ((r0, r1, c0, c1), (x0, x1, y0, y1)) = (bounds(anchor, focus), bounds(ca, cf));
        if table != ct || x1 < r0 || x0 > r1 || y1 < c0 || y0 > c1 {
            return vec![s];
        }
        let block = |e: &Self, (ar, ac): (usize, usize), (fr, fc): (usize, usize)| {
            e.normalize(Selection::Cells { table, anchor: CellRef { row: ar, col: ac }, focus: CellRef { row: fr, col: fc }, shape: CellShape::Rect })
        };
        let (m0, m1) = (r0.max(x0), r1.min(x1));
        let mut out = Vec::with_capacity(4);
        if x0 > r0 {
            out.push(block(self, (r0, c0), (x0 - 1, c1)));
        }
        if x1 < r1 {
            out.push(block(self, (x1 + 1, c0), (r1, c1)));
        }
        if y0 > c0 {
            out.push(block(self, (m0, c0), (m1, y0 - 1)));
        }
        if y1 < c1 {
            out.push(block(self, (m0, y1 + 1), (m1, c1)));
        }
        out
    }

    /// Drops the other ranges once the selection is another.
    pub(super) fn sync_multi(&mut self) {
        if self.multi.primary.is_some() && self.multi.primary != Some(self.selection) {
            self.multi = Multi::default();
        }
    }

    /// The other ranges through an edit's maps, while the edit kept the selection (mapped it through the same maps).
    /// Each map moves only the ends in the paragraphs it touches, so an edit costs its maps and the ranges, not their
    /// product.
    pub(super) fn map_multi(&mut self, maps: &[Map]) {
        if let Some(Selection::Text { anchor, focus, unit }) = self.multi.primary
            && self.multi.primary != Some(self.selection)
        {
            let map = |p: Position| maps.iter().fold(p, |p, m| m.apply(p));
            let mapped = Selection::range(map(anchor), map(focus), unit);
            if mapped == self.selection {
                self.multi.primary = Some(mapped);
            }
        }
        self.sync_multi();
        if self.multi.others.is_empty() || maps.iter().all(|m| matches!(m, Map::Same)) {
            return;
        }
        let mut ends: Vec<Position> = self.multi.others.iter().flat_map(|&(a, b)| [a, b]).collect();
        // Ends by paragraph, and those not in text
        let mut by_para: HashMap<ParaId, Vec<usize>> = HashMap::new();
        let mut rest: Vec<usize> = Vec::new();
        let file = |by_para: &mut HashMap<ParaId, Vec<usize>>, rest: &mut Vec<usize>, k: usize, p: &Position| match p.para_offset() {
            Some((para, _)) => by_para.entry(para).or_default().push(k),
            None => rest.push(k),
        };
        for (k, p) in ends.iter().enumerate() {
            file(&mut by_para, &mut rest, k, p);
        }
        for m in maps {
            let moved: Vec<usize> = match m {
                Map::Same => continue,
                Map::Text { para, .. } => {
                    for &k in by_para.get(para).into_iter().flatten() {
                        ends[k] = m.apply(ends[k]);
                    }
                    continue;
                }
                Map::Split { para, .. } => by_para.remove(para).unwrap_or_default(),
                Map::Join { second, .. } => by_para.remove(second).unwrap_or_default(),
                Map::Removed { paras, .. } => {
                    let gone: Vec<ParaId> = if paras.len() < by_para.len() {
                        paras.iter().copied().filter(|p| by_para.contains_key(p)).collect()
                    } else {
                        by_para.keys().copied().filter(|p| paras.contains(p)).collect()
                    };
                    let mut moved: Vec<usize> = gone.iter().filter_map(|p| by_para.remove(p)).flatten().collect();
                    moved.append(&mut rest);
                    moved
                }
            };
            for k in moved {
                ends[k] = m.apply(ends[k]);
                file(&mut by_para, &mut rest, k, &ends[k]);
            }
        }
        self.multi.others = ends.as_chunks::<2>().0.iter().map(|&[a, b]| (a, b)).filter(|(a, b)| !a.same_place(b)).collect();
    }

    /// Every selected range in document order: the text selection's one, a column block's pieces, or a discontiguous
    /// selection's ranges. None for a caret, cells or an object.
    pub fn selection_ranges(&mut self) -> Vec<(Position, Position)> {
        if let Selection::Block { anchor, .. } = self.selection {
            return self
                .block_pieces()
                .into_iter()
                .map(|(_, _, p, a, b)| (Position::text(anchor.story, p, a), Position::text(anchor.story, p, b)))
                .collect();
        }
        let Some((a, b)) = self.selection.ordered(&self.index).filter(|(a, b)| !a.same_place(b)) else { return self.other_ranges().to_vec() };
        let others = self.other_ranges();
        let at = others.partition_point(|r| self.index.cmp(&r.0, &a) == Ordering::Less);
        let mut out = Vec::with_capacity(others.len() + 1);
        out.extend_from_slice(&others[..at]);
        out.push((a, b));
        out.extend_from_slice(&others[at..]);
        out
    }

    /// Selects `ranges` (positions in pairs, either way round) of one story, the last as the selection the next
    /// gesture or command goes on from: overlapping and touching ranges join, empty ones go.
    pub fn select_ranges(&mut self, ranges: &[(Position, Position)]) -> Outcome {
        self.select_pieces(ranges, Vec::new(), None)
    }

    /// Selects `ranges` as [`Editor::select_ranges`] does, with the blocks of cells `cells`; `active`, a block of
    /// cells, goes on as the selection in place of the last range. Each range is normalized as a selection is
    /// (snapped, over whole field codes) before they join.
    pub fn select_pieces(&mut self, ranges: &[(Position, Position)], cells: Vec<Selection>, active: Option<Selection>) -> Outcome {
        let story = ranges.last().map(|r| r.0.story);
        if ranges.iter().any(|(a, b)| Some(a.story) != story || Some(b.story) != story) {
            return Outcome::invalid("the ranges are in different stories");
        }
        let block_story = |e: &Self, c: &Selection| match *c {
            Selection::Cells { table, .. } => e.index.table_slot(table).map(|s| e.index.story(s.container)),
            _ => None,
        };
        let mut story = story;
        for c in cells.iter().chain(&active) {
            let s = block_story(self, c);
            if s.is_none() || story.is_some_and(|t| Some(t) != s) {
                return Outcome::invalid("the ranges are in different stories");
            }
            story = s;
        }
        let normal = |e: &Self, (a, b): (Position, Position)| {
            let (a, b) = if e.index.cmp(&a, &b) == Ordering::Greater { (b, a) } else { (a, b) };
            e.normalize(Selection::range(a, b, Granularity::Char)).ordered(&e.index).unwrap_or((a, b))
        };
        let mut all: Vec<(Position, Position)> = ranges.iter().map(|&r| normal(self, r)).filter(|(a, b)| !a.same_place(b)).collect();
        all.sort_by_cached_key(|r| self.index.order_key(&r.0));
        let mut joined: Vec<(Position, Position)> = Vec::with_capacity(all.len());
        for (a, b) in all {
            match joined.last_mut() {
                Some(last) if self.index.cmp(&a, &last.1) != Ordering::Greater => {
                    if self.index.cmp(&b, &last.1) == Ordering::Greater {
                        last.1 = b;
                    }
                }
                _ => joined.push((a, b)),
            }
        }
        let active = active.map(|c| self.normalize(c));
        let mut cells = {
            let mut kept: Vec<Selection> = Vec::with_capacity(cells.len());
            for c in cells.into_iter().map(|c| self.normalize(c)) {
                if Some(c) != active && !kept.contains(&c) {
                    kept.push(c);
                }
            }
            kept
        };
        let selection = match (active, ranges.last()) {
            (Some(c), _) => c,
            (None, None) => match cells.pop() {
                Some(c) => c,
                None => return Outcome::Unchanged("no ranges"),
            },
            (None, Some(&(la, lb))) => {
                let primary = normal(self, (la, lb));
                // The joined range that holds the last one given
                let k = joined
                    .iter()
                    .position(|r| self.index.cmp(&r.0, &primary.0) != Ordering::Greater && self.index.cmp(&primary.1, &r.1) != Ordering::Greater);
                let (a, b) = match k {
                    Some(k) => joined.remove(k),
                    None if primary.0.same_place(&primary.1) => match joined.pop() {
                        Some(r) => r,
                        None => match cells.pop() {
                            Some(c) => {
                                self.set_selection(c);
                                self.multi = Multi { primary: Some(self.selection), others: Vec::new(), cells };
                                return Outcome::Moved;
                            }
                            None => {
                                self.set_selection(Selection::Caret(lb));
                                self.multi = Multi::default();
                                return Outcome::Moved;
                            }
                        },
                    },
                    None => primary,
                };
                if self.index.cmp(&la, &lb) == Ordering::Greater {
                    Selection::range(b, a, Granularity::Char)
                } else {
                    Selection::range(a, b, Granularity::Char)
                }
            }
        };
        self.set_selection(selection);
        self.multi = Multi { primary: Some(self.selection), others: joined, cells };
        Outcome::Moved
    }

    /// Adds the range `a..b` to the selection, as Ctrl and a click or drag do, to go on as the selection.
    pub fn add_selection_range(&mut self, a: Position, b: Position) -> Outcome {
        let apart = matches!(self.selection, Selection::Block { .. }) || self.selection_story().is_some_and(|s| s != a.story);
        let (mut all, cells) = if apart { (Vec::new(), Vec::new()) } else { (self.selection_ranges(), self.cell_blocks()) };
        all.push((a, b));
        self.select_pieces(&all, cells, None)
    }

    /// A press at `p` with Ctrl while text or cells are selected starts adding a range: a click adds the sentence, a
    /// drag what it goes over (a block of cells once it crosses cells, as a plain drag selects), a double-click the
    /// word and a triple-click the paragraph (in place of what the gesture's first click added); Command on a Mac.
    /// Alt (Option) and a drag select a column block. Whether the press was taken so.
    pub(super) fn add_press(&mut self, p: Position, x: f64, y: f64, clicks: u8, mods: super::keymap::Mods) -> bool {
        let plain_modes = self.modes == super::extend::Modes::default();
        // Command on a Mac.
        let ctrl = mods.ctrl || (self.mac && mods.meta);
        if mods.alt && !ctrl && !mods.shift && clicks == 1 && plain_modes {
            self.adding = None;
            self.set_selection(Selection::Caret(p));
            self.drag = Some(super::Drag { anchor: p, span: None, unit: Granularity::Char, at: (x, y), block: false, adding: false, column: true });
            self.requests.push(super::input::Request::CapturePointer);
            return true;
        }
        if !ctrl || mods.shift || mods.alt || !plain_modes {
            self.adding = None;
            return false;
        }
        let (base, cells, remove) = match clicks {
            1 => {
                let takes = matches!(self.selection, Selection::Text { .. } | Selection::Cells { .. }) && self.selection_story() == Some(p.story);
                if !takes {
                    self.adding = None;
                    return false;
                }
                let base = self.selection_ranges();
                let cells = self.cell_blocks();
                let remove = base.iter().any(|(a, b)| self.index.cmp(a, &p).is_lt() && self.index.cmp(&p, b).is_lt()) || self.in_cell_blocks(&p);
                self.adding = Some(Adding { base: base.clone(), cells: cells.clone(), was: self.selection, remove, swept: None });
                (base, cells, remove)
            }
            _ => match &self.adding {
                Some(a) => (a.base.clone(), a.cells.clone(), a.remove),
                None => return false,
            },
        };
        let unit = match clicks {
            1 => Granularity::Char,
            2 => Granularity::Word,
            _ => Granularity::Paragraph,
        };
        let (selection, span) = if unit == Granularity::Char {
            (Selection::Caret(p), None)
        } else {
            let (a, b) = self.span(p, unit);
            (Selection::range(a, b, unit), Some((a, b)))
        };
        self.selection = self.normalize(selection);
        self.pending = None;
        self.history.break_typing();
        self.multi = Multi { primary: Some(self.selection), others: base, cells };
        let anchor = span.map_or(p, |s| s.0);
        self.drag = Some(super::Drag { anchor, span, unit, at: (x, y), block: false, adding: true, column: false });
        self.requests.push(super::input::Request::CapturePointer);
        if remove {
            self.show_removal(p);
        }
        true
    }

    /// Before a press in a table's selection zone selects cells of `table`: with Ctrl (Command on a Mac) while text
    /// or cells of the table's story are selected, what is selected before it, for [`Editor::zone_added`].
    pub(super) fn zone_press_adds(&mut self, table: crate::layout::TableId, mods: super::keymap::Mods) -> Option<Adding> {
        let ctrl = mods.ctrl || (self.mac && mods.meta);
        let story = self.index.table_slot(table).map(|s| self.index.story(s.container));
        let takes = ctrl
            && !mods.shift
            && !mods.alt
            && matches!(self.selection, Selection::Text { .. } | Selection::Cells { .. })
            && story.is_some()
            && self.selection_story() == story;
        takes.then(|| Adding { base: self.selection_ranges(), cells: self.cell_blocks(), was: self.selection, remove: false, swept: None })
    }

    /// After that press: the block of cells it selected goes with what was selected before, to join it on release.
    pub(super) fn zone_added(&mut self, adding: Option<Adding>) {
        if let Some(a) = &adding {
            self.multi = Multi { primary: Some(self.selection), others: a.base.clone(), cells: a.cells.clone() };
        }
        self.adding = adding;
    }

    /// A Ctrl press on the selection's own range, which a drag makes a Ctrl drag-and-drop copy: released where it
    /// was, it takes the sentence out of the selection (and a double- or triple-click the word or paragraph).
    pub(super) fn ctrl_press_on_selection(&mut self, mods: super::keymap::Mods) {
        let ctrl = mods.ctrl || (self.mac && mods.meta);
        self.adding =
            ctrl.then(|| Adding { base: self.selection_ranges(), cells: self.cell_blocks(), was: self.selection, remove: true, swept: None });
    }

    /// Whether a removing drag's selection `s` goes over other text than it did, so that what is left is shown anew.
    pub(super) fn sweeps_anew(&self, s: &Selection) -> bool {
        self.adding.as_ref().is_none_or(|a| a.swept != swept_by(*s))
    }

    /// Whether a press that started a text drag is one [`Editor::ctrl_press_on_selection`] took.
    pub(super) fn removing(&self) -> bool {
        self.adding.as_ref().is_some_and(|a| a.remove)
    }

    /// A removing drag's selection so far: the ranges and cells before it without what it has gone over (the
    /// selection the drag made), at `at` once none is left.
    pub(super) fn show_removal(&mut self, at: Position) {
        let swept = swept_by(self.selection);
        let Some(adding) = self.adding.as_mut() else { return };
        adding.swept = swept;
        let (base, cells, was) = (adding.base.clone(), adding.cells.clone(), adding.was);
        self.select_without(&base, &cells, swept, was, at);
    }

    /// Selects `base` and `cells` without `cut` (a range, or a block of cells), going on from what is left of `was`
    /// (else the last range left, else the last block), or puts the caret at `at` when nothing is left.
    fn select_without(&mut self, base: &[(Position, Position)], cells: &[Selection], cut: Option<Selection>, was: Selection, at: Position) {
        let text_cut = cut.and_then(|c| c.ordered(&self.index));
        let cell_cut = cut.filter(|c| matches!(c, Selection::Cells { .. }));
        let mut out = Vec::with_capacity(base.len() + 1);
        for &(a, b) in base {
            match text_cut {
                Some((x, y)) if self.index.cmp(&a, &y).is_lt() && self.index.cmp(&x, &b).is_lt() => {
                    if self.index.cmp(&a, &x).is_lt() {
                        out.push((a, x));
                    }
                    if self.index.cmp(&y, &b).is_lt() {
                        out.push((y, b));
                    }
                }
                _ => out.push((a, b)),
            }
        }
        let left: Vec<Selection> = match cell_cut {
            Some(c) => {
                let c = self.normalize(c);
                cells.iter().flat_map(|&s| self.cells_without(s, c)).collect()
            }
            None => cells.to_vec(),
        };
        let mut active = None;
        if let Selection::Cells { table, anchor, focus, .. } = was {
            let (r0, r1, c0, c1) = bounds(anchor, focus);
            active = left.iter().rev().copied().find(|s| {
                matches!(*s, Selection::Cells { table: t, anchor: a, focus: f, .. }
                    if t == table && { let (x0, x1, y0, y1) = bounds(a, f); r0 <= x0 && x1 <= r1 && c0 <= y0 && y1 <= c1 })
            });
        } else if let Some((wa, wb)) = was.ordered(&self.index)
            && let Some(k) = out.iter().rposition(|(a, b)| self.index.cmp(&wa, a).is_le() && self.index.cmp(b, &wb).is_le())
        {
            let r = out.remove(k);
            out.push(r);
        }
        if out.is_empty() && left.is_empty() {
            self.set_selection(Selection::Caret(at));
            self.multi = Multi::default();
        } else {
            self.select_pieces(&out, left, active);
        }
    }

    /// The end of a Ctrl press that adds a range: the range or block of cells joins the others (a click that went
    /// nowhere adds the sentence at `at`), or, for a press on selected text or a selected cell, goes out of the
    /// selection (a click the sentence, or the cell); cancelled, the selection is what it was.
    pub(super) fn add_release(&mut self, at: Position, cancel: bool) {
        let Some(Adding { base: mut ranges, cells, was, remove, swept }) = self.adding.clone() else { return };
        if remove && !cancel {
            let cut = swept.or_else(|| self.held_cell(&at, &cells)).unwrap_or_else(|| {
                let (a, b) = self.span(at, Granularity::Sentence);
                Selection::range(a, b, Granularity::Char)
            });
            self.select_without(&ranges, &cells, Some(cut), was, at);
            return;
        }
        let now = if cancel { was } else { self.selection };
        if let Selection::Cells { .. } = now {
            self.select_pieces(&ranges, cells, Some(now));
            return;
        }
        let added = match now.ordered(&self.index) {
            Some((a, b)) if !a.same_place(&b) => (a, b),
            _ => self.span(at, Granularity::Sentence),
        };
        ranges.push(added);
        self.select_pieces(&ranges, cells, None);
    }

    /// Whether any range or block of cells of a discontiguous selection is in a table.
    pub(super) fn any_piece_in_table(&mut self) -> bool {
        self.each_piece(|e| e.table_target().is_some()).into_iter().any(|t| t)
    }

    /// The ranges and blocks of cells, each as a selection of its own, in document order.
    pub(super) fn pieces(&mut self) -> Vec<Selection> {
        let mut out: Vec<Selection> = self.selection_ranges().into_iter().map(|(a, b)| Selection::range(a, b, Granularity::Char)).collect();
        let cells = self.cell_blocks();
        if !cells.is_empty() {
            out.extend(cells);
            out.sort_by_cached_key(|s| s.ordered(&self.index).map(|r| r.0).or_else(|| self.cells_start(s)).map(|p| self.index.order_key(&p)));
        }
        out
    }

    /// Runs `f` with each range and block of cells in turn as the selection, in document order, and puts the
    /// selection back.
    pub(super) fn each_piece<T>(&mut self, mut f: impl FnMut(&mut Self) -> T) -> Vec<T> {
        let pieces = self.pieces();
        let (selection, multi) = (self.selection, std::mem::take(&mut self.multi));
        let out = pieces
            .into_iter()
            .map(|s| {
                self.selection = s;
                f(self)
            })
            .collect();
        self.selection = selection;
        self.multi = multi;
        out
    }

    /// The highlight of the ranges and cells on `pages`: only the ranges that reach them are walked.
    pub(super) fn multi_rects(&mut self, pages: std::ops::Range<usize>) -> Vec<(usize, super::view::Band)> {
        let mut out = Vec::new();
        for c in self.cell_blocks() {
            out.extend(self.block_rects(c, pages.clone()).into_iter().map(|(page, r)| (page, super::view::Band::upright(r))));
        }
        let Some(story) = self.selection_story() else { return out };
        // The story's first and last places on the pages
        let mut edges: Option<(Position, Position)> = None;
        for page in pages.clone() {
            let Some(geo) = self.pages.page(page, &self.index) else { continue };
            let mut lines = geo.lines.iter().filter(|l| geo.stories[l.story].story == story);
            let first = lines.next().and_then(|l| l.paras.first().and_then(|&p| Some(Position::text(story, p, l.first_of(p)?))));
            let last = geo
                .lines
                .iter()
                .rev()
                .find(|l| geo.stories[l.story].story == story)
                .and_then(|l| l.paras.last().and_then(|&p| Some(Position::text(story, p, l.last_of(p)?))));
            if let (Some(f), Some(t)) = (first, last) {
                edges = Some((edges.map_or(f, |e| e.0), t));
            }
        }
        let Some((lo, hi)) = edges else { return out };
        let shows = |e: &Self, r: &(Position, Position)| e.index.cmp(&r.1, &lo).is_ge() && e.index.cmp(&r.0, &hi).is_le();
        let others = self.other_ranges();
        let k0 = others.partition_point(|r| self.index.cmp(&r.1, &lo).is_lt());
        let k1 = others.partition_point(|r| self.index.cmp(&r.0, &hi).is_le());
        let mut shown = others[k0..k1.max(k0)].to_vec();
        shown.extend(self.selection.ordered(&self.index).filter(|r| shows(self, r)));
        for (a, b) in shown {
            out.extend(self.range_rects(a, b, pages.clone()));
        }
        out
    }

    /// Each range's and block of cells' paragraph pieces, in document order.
    pub(super) fn multi_pieces(&mut self) -> Vec<(ParaId, Unit, Unit)> {
        self.each_piece(|e| e.selected_ranges()).into_iter().flatten().collect()
    }

    /// The paragraphs whose marks a range holds.
    pub(super) fn multi_marks(&mut self) -> Vec<ParaId> {
        self.each_piece(|e| {
            let r = e.selected_ranges();
            e.selected_marks(&r)
        })
        .into_iter()
        .flatten()
        .collect()
    }

    /// The ranges' and blocks of cells' content one after another, as Word copies a discontiguous selection: each
    /// range's text runs on from the one before's, cells as tables of their own.
    pub(super) fn multi_copy(&mut self) -> Option<Fragment> {
        let parts = self.each_piece(|e| e.copy_content());
        let mut out: Option<Fragment> = None;
        for f in parts.into_iter().flatten() {
            let Some(o) = out.as_mut() else {
                out = Some(f);
                continue;
            };
            let mut blocks = f.blocks.into_iter();
            if o.open_end
                && let Some(Block::Para(last)) = o.blocks.last_mut()
                && let Some(first) = blocks.next()
            {
                match first {
                    Block::Para(p) => last.inlines.extend(p.inlines),
                    t => o.blocks.push(t),
                }
            }
            o.blocks.extend(blocks);
            o.open_end = f.open_end;
            o.cells = None;
        }
        out
    }

    /// The ranges' text one after another, cut to its first `limit` characters, and whether that is all of it.
    pub(super) fn multi_text(&mut self, limit: usize) -> (String, bool) {
        let mut texts = HashMap::new();
        let mut left = limit;
        let mut whole = true;
        let parts = self.each_piece(|e| {
            if !whole {
                return String::new();
            }
            let (t, all) = e.text_within(left, &mut texts);
            left -= t.chars().count().min(left);
            whole &= all;
            t
        });
        (parts.concat(), whole)
    }

    /// Deletes every range, last first, and clears every block of cells, in one step (the delete has checked locks
    /// on them all), but a range within a showing placeholder, which stays; the caret goes where the selection's own
    /// range or block was. Ranges go before cells, so that a range in a cleared cell is still there to delete.
    pub(super) fn multi_delete(&mut self, dir: Dir, fonts: &crate::font::Fonts) -> Outcome {
        let ranges = self.selection_ranges();
        let cells = self.cell_blocks();
        let primary = self.selection.ordered(&self.index);
        let (selection, multi) = (self.selection, std::mem::take(&mut self.multi));
        let mut tx = self.begin(if dir == Dir::Back { "Backspace" } else { "Delete" });
        let mut caret: Option<(Position, usize)> = None;
        let mut deleted = false;
        let stale = |e: &mut Self, tx: super::Tx| {
            e.abort(tx);
            (e.selection, e.multi) = (selection, multi.clone());
            Outcome::Unchanged("stale selection")
        };
        for &(a, b) in ranges.iter().rev() {
            self.selection = Selection::range(a, b, Granularity::Char);
            let mine = primary.is_some_and(|p| p.0.same_place(&a));
            if self.placeholder_here().is_some() {
                if mine {
                    caret = Some((a, tx.maps.len()));
                }
                continue;
            }
            deleted = true;
            match self.delete_range_joining(&mut tx, false) {
                Ok(Some(p)) if mine => caret = Some((p, tx.maps.len())),
                Ok(_) => {}
                Err(_) => return stale(self, tx),
            }
        }
        for c in &cells {
            let Some((table, rows, grid)) = self.cleared_cells(*c) else { continue };
            let n = self.index.table(&self.doc, table).map_or(0, |t| t.rows.len());
            if self.step(&mut tx, Step::Rows { table, range: 0..n, with: rows, grid: Some(grid) }).is_err() {
                return stale(self, tx);
            }
            tx.stale_tables.push(table);
            deleted = true;
        }
        if matches!(selection, Selection::Cells { .. }) {
            caret = self.cells_start(&selection).map(|p| (p, tx.maps.len()));
        }
        let Some((p, from)) = caret.filter(|_| deleted) else {
            self.abort(tx);
            (self.selection, self.multi) = (selection, multi);
            return Outcome::Unchanged(if deleted { "stale selection" } else { "a placeholder" });
        };
        self.selection = Selection::Caret(tx.maps[from..].iter().fold(p, |p, m| m.apply(p)));
        self.commit(tx, None, fonts)
    }

    /// Selects all the text of the selection's story formatted as the selection's first character is (or the
    /// caret's): the same font formatting, as the Font group shows it, a range for each stretch of such runs.
    pub fn select_similar_formatting(&mut self) -> Outcome {
        let Some(focus) = self.selection.focus() else { return Outcome::Unchanged("no text selected") };
        let first = self.selection_ranges().first().and_then(|&(a, b)| {
            let (para, u) = a.para_offset()?;
            let end = b.para_offset().filter(|(q, _)| *q == para).map_or(Unit(u.0 + 1), |(_, v)| v);
            self.runs_in((para, u, end)).into_iter().next()
        });
        let Some(target) = first.or_else(|| self.run_at(&focus)) else { return Outcome::Unchanged("no text") };
        let want = super::state::character(&[&target]);
        let mut same: HashMap<*const RunStyle, bool> = HashMap::new();
        let ranges = self.story_runs(focus.story, |r| *same.entry(Rc::as_ptr(r)).or_insert_with(|| super::state::character(&[r]) == want));
        if ranges.is_empty() {
            return Outcome::Unchanged("no text formatted so");
        }
        self.select_ranges(&ranges)
    }

    /// Selects every use of style `id` in the body: each paragraph with it for a paragraph style (with its mark),
    /// each stretch of runs with it for a character style.
    pub fn select_style_instances(&mut self, id: &str) -> Outcome {
        let Some(paragraph) = self.style_kind(id) else { return Outcome::invalid("no such style") };
        let story = super::position::StoryId::Body;
        let ranges = if paragraph {
            let mut out: Vec<(Position, Position)> = Vec::new();
            for para in self.paras_of(story) {
                let Some(p) = self.index.para(&self.doc, para) else { continue };
                if self.para_style_id(p) != Some(id) {
                    continue;
                }
                let end = self.after_mark(Position::text(story, para, para_len(p)));
                let start = Position::text(story, para, Unit(0));
                match out.last_mut() {
                    Some(last) if last.1.same_place(&start) => last.1 = end,
                    _ => out.push((start, end)),
                }
            }
            out
        } else {
            self.story_runs(story, |r| r.style_id.as_deref() == Some(id))
        };
        if ranges.is_empty() {
            return Outcome::Unchanged("the style isn't used");
        }
        self.select_ranges(&ranges)
    }

    /// How many times each style is used in the body: paragraphs for a paragraph style, stretches of runs for a
    /// character style (a stretch opens on text, as [`Editor::story_runs`] opens one). One walk of the body.
    pub fn style_use_counts(&self) -> HashMap<String, usize> {
        let mut out: HashMap<String, usize> = HashMap::new();
        for para in self.story_paras(Stories::default()) {
            let Some(p) = self.index.para(&self.doc, para) else { continue };
            if let Some(s) = self.para_style_id(p) {
                *out.entry(s.to_string()).or_default() += 1;
            }
            let mut open: Option<&str> = None;
            for i in &p.inlines {
                if i.units() == 0 {
                    continue;
                }
                let style = i.run().and_then(|r| r.style_id.as_deref());
                open = match (open, style) {
                    (Some(o), Some(s)) if o == s => Some(o),
                    (_, Some(s)) if matches!(i, Inline::Text(..)) => {
                        *out.entry(s.to_string()).or_default() += 1;
                        Some(s)
                    }
                    _ => None,
                };
            }
        }
        out
    }

    /// A paragraph's style: its own, else the document's default paragraph style.
    fn para_style_id<'a>(&'a self, p: &'a crate::docx::Paragraph) -> Option<&'a str> {
        p.style.style_id.as_deref().or_else(|| self.doc.resolver.as_ref()?.styles.default_paragraph.as_deref())
    }

    /// Whether style `id` is a paragraph style (else a character style); none when the document has no such style.
    fn style_kind(&self, id: &str) -> Option<bool> {
        match self.doc.resolver.as_ref()?.styles.by_id.get(id)?.kind {
            crate::docx::styles::StyleKind::Paragraph => Some(true),
            crate::docx::styles::StyleKind::Character => Some(false),
            _ => None,
        }
    }

    /// The stretches of runs of `story` (the body: its paragraphs in tables too, not its text boxes) that `keep`
    /// takes, in document order.
    fn story_runs(&self, story: super::position::StoryId, mut keep: impl FnMut(&crate::docx::Run) -> bool) -> Vec<(Position, Position)> {
        let paras = self.paras_of(story);
        let mut out = Vec::new();
        for para in paras {
            let Some(p) = self.index.para(&self.doc, para) else { continue };
            let mut at = 0u32;
            let mut open: Option<u32> = None;
            for i in &p.inlines {
                let n = i.units() as u32;
                if n > 0 {
                    let takes = match i {
                        Inline::Text(..) => i.run().is_some_and(&mut keep),
                        _ => open.is_some() && i.run().is_some_and(&mut keep),
                    };
                    match (takes, open) {
                        (true, None) => open = Some(at),
                        (false, Some(s)) => {
                            out.push((Position::text(story, para, Unit(s)), Position::text(story, para, Unit(at))));
                            open = None;
                        }
                        _ => {}
                    }
                }
                at += n;
            }
            if let Some(s) = open {
                out.push((Position::text(story, para, Unit(s)), Position::text(story, para, Unit(at))));
            }
        }
        out
    }

    /// A text position as a paragraph and an offset in its text, in Unicode scalars: fields as their results,
    /// objects and note references as U+FFFC, every run counted (hidden and deleted text too).
    pub fn text_point(&self, p: &Position) -> Option<(ParaId, u32)> {
        let (para, u) = p.para_offset()?;
        let seen = super::text::Seen::of(self.index.para(&self.doc, para)?, |_| true, true);
        Some((para, seen.units.partition_point(|&x| x < u.0) as u32))
    }

    /// The first `limit` selected ranges ([`Editor::selection_ranges`]) as text points ([`Editor::text_point`]), and
    /// whether that is all of them. Each paragraph's text is read once, however many ends it holds.
    pub fn selection_text_ranges(&mut self, limit: usize) -> (Vec<((ParaId, u32), (ParaId, u32))>, bool) {
        let ranges = self.selection_ranges();
        let whole = ranges.len() <= limit;
        let mut read: Option<(ParaId, Vec<u32>)> = None;
        let mut point = |e: &Self, p: &Position| -> Option<(ParaId, u32)> {
            let (para, u) = p.para_offset()?;
            if read.as_ref().is_none_or(|(q, _)| *q != para) {
                read = Some((para, super::text::Seen::of(e.index.para(&e.doc, para)?, |_| true, true).units));
            }
            let units = &read.as_ref()?.1;
            Some((para, units.partition_point(|&x| x < u.0) as u32))
        };
        let out = ranges.iter().take(limit).filter_map(|(a, b)| Some((point(self, a)?, point(self, b)?))).collect();
        (out, whole)
    }

    /// The position at a paragraph and an offset in its text ([`Editor::text_point`]); past its end, its end.
    pub fn text_position(&self, para: ParaId, offset: u32) -> Option<Position> {
        let slot = self.index.para_slot(para)?;
        let p = self.index.para(&self.doc, para)?;
        let seen = super::text::Seen::of(p, |_| true, true);
        let u = seen.units.get(offset as usize).copied().unwrap_or(para_len(p).0);
        Some(Position::text(self.index.story(slot.container), para, Unit(u)))
    }
}
