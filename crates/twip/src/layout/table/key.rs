//! Table and row cache keys

use std::rc::Rc;

use crate::docx::model::*;
use crate::docx::props::Border;
use crate::layout::autofit;
use crate::layout::cache::CacheKey;
use crate::layout::env::TextEnv;
use crate::layout::story::StoryLayout;

use super::{CellBox, RowBox, TableLayout, assemble::vertical_cell, closing_para};

// Table cache
#[derive(Debug)]
pub struct TableKey {
    shell: Table,
    text_w: u64,
    measure: bool,
    cols: Vec<f64>,
    fit: (i64, i64, i64),
    cells: Vec<Option<Rc<StoryLayout>>>,
    loose: Loose,
    text: TextEnv,
    fonts: u64,
}

pub struct TableProbe<'a> {
    pub(super) table: &'a Table,
    pub(super) text_w: u64,
    pub(super) measure: bool,
    pub(super) cols: &'a [f64],
    pub(super) fit: (i64, i64, i64),
    pub(super) cells: &'a [Option<Rc<StoryLayout>>],
    pub(super) text: &'a TextEnv,
    pub(super) fonts: u64,
}

fn same_story(a: &Option<Rc<StoryLayout>>, b: &Option<Rc<StoryLayout>>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(x), Some(y)) => Rc::ptr_eq(x, y),
        _ => false,
    }
}

impl CacheKey for TableKey {
    type Probe<'a> = TableProbe<'a>;

    fn bucket(p: &TableProbe) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = crate::layout::cache::KeyHasher::default();
        (p.table.id, p.text_w, p.measure, p.cells.len()).hash(&mut h);
        for c in p.cells {
            c.as_ref().map(Rc::as_ptr).hash(&mut h);
        }
        h.finish()
    }

    fn matches(&self, p: &TableProbe) -> bool {
        self.text_w == p.text_w
            && self.measure == p.measure
            && self.fonts == p.fonts
            && self.cols == p.cols
            && self.fit == p.fit
            && self.text == *p.text
            && self.cells.len() == p.cells.len()
            && self.cells.iter().zip(p.cells).all(|(a, b)| same_story(a, b))
            && same_shell(&self.shell, p.table)
            && self.loose.matches(p.table, p.text)
    }

    fn from_probe(p: &TableProbe) -> TableKey {
        TableKey {
            shell: shell(p.table),
            text_w: p.text_w,
            measure: p.measure,
            cols: p.cols.to_vec(),
            fit: p.fit,
            cells: p.cells.to_vec(),
            loose: Loose::of(p.table, p.text),
            text: p.text.clone(),
            fonts: p.fonts,
        }
    }
}

// Cell content outside the stories
/// What assembling reads from cells besides their stories: every block of a cell without one (turned and
/// continuation cells), each cell's closing paragraph, and the keep and break flags of each row's first paragraph.
#[derive(Debug)]
struct Loose {
    blocks: Vec<Block>,
    closings: Vec<Option<(crate::layout::ids::ParaId, f64)>>,
    rows: Vec<(bool, bool)>,
}

fn storyless(c: &Cell) -> bool {
    c.v_merge == VMerge::Continue || vertical_cell(c).is_some()
}

fn loose_blocks(t: &Table) -> impl Iterator<Item = &Block> {
    t.rows.iter().flat_map(|r| &r.cells).filter(|c| storyless(c)).flat_map(|c| &c.blocks)
}

fn closings(t: &Table) -> impl Iterator<Item = Option<(crate::layout::ids::ParaId, f64)>> + '_ {
    t.rows.iter().flat_map(|r| &r.cells).map(closing_para)
}

fn row_flags<'a>(t: &'a Table, text: &TextEnv) -> impl Iterator<Item = (bool, bool)> + 'a {
    let view = text.review.view;
    t.rows.iter().map(move |r| match r.cells.first().and_then(|c| c.blocks.first()) {
        Some(Block::Para(p)) => {
            let s = p.para_style_in(view);
            (s.keep_next, s.page_break_before)
        }
        _ => (false, false),
    })
}

impl Loose {
    fn of(t: &Table, text: &TextEnv) -> Loose {
        Loose { blocks: loose_blocks(t).cloned().collect(), closings: closings(t).collect(), rows: row_flags(t, text).collect() }
    }

    fn matches(&self, t: &Table, text: &TextEnv) -> bool {
        self.blocks.iter().eq(loose_blocks(t)) && self.closings.iter().copied().eq(closings(t)) && self.rows.iter().copied().eq(row_flags(t, text))
    }
}

fn shell(t: &Table) -> Table {
    let Table {
        style_id,
        width,
        jc,
        own_jc,
        ind,
        ind_pct,
        ind_set,
        borders,
        fixed,
        cell_spacing,
        look,
        bands,
        floating,
        bidi_visual,
        alt: _,
        grid,
        rows,
        src: _,
        id,
        direct: _,
        controls: _,
        change,
        grid_change,
    } = t;
    let rows = rows
        .iter()
        .map(|r| {
            let Row {
                height,
                cant_split,
                header,
                grid_before,
                grid_after,
                w_before,
                w_after,
                ind,
                jc,
                div,
                borders,
                shading,
                hidden: _,
                vanished,
                cell_spacing: row_spacing,
                run,
                nudge,
                cells,
                src: _,
                direct: _,
                exceptions: _,
                controls: _,
                rev,
            } = r;
            let cells = cells
                .iter()
                .map(|c| {
                    let Cell {
                        width,
                        span,
                        v_merge,
                        top,
                        left,
                        bottom,
                        right,
                        shading,
                        margins,
                        v_align,
                        text_direction,
                        no_wrap,
                        fit_text,
                        hide_mark,
                        diagonals,
                        blocks: _,
                        direct: _,
                        controls: _,
                        rev,
                        ids: _,
                    } = c;
                    Cell {
                        width: *width,
                        span: *span,
                        v_merge: *v_merge,
                        top: top.clone(),
                        left: left.clone(),
                        bottom: bottom.clone(),
                        right: right.clone(),
                        shading: *shading,
                        margins: *margins,
                        v_align: v_align.clone(),
                        text_direction: text_direction.clone(),
                        no_wrap: *no_wrap,
                        fit_text: *fit_text,
                        hide_mark: *hide_mark,
                        diagonals: diagonals.clone(),
                        blocks: Vec::new(),
                        direct: Default::default(),
                        controls: Vec::new(),
                        rev: rev.clone(),
                        ids: None,
                    }
                })
                .collect();
            Row {
                height: height.clone(),
                cant_split: *cant_split,
                header: *header,
                grid_before: *grid_before,
                grid_after: *grid_after,
                w_before: *w_before,
                w_after: *w_after,
                ind: *ind,
                jc: *jc,
                div: div.clone(),
                borders: borders.clone(),
                shading: *shading,
                hidden: false,
                vanished: *vanished,
                cell_spacing: *row_spacing,
                run: *run,
                nudge: *nudge,
                cells,
                src: None,
                direct: Raw(None),
                exceptions: Raw(None),
                controls: Vec::new(),
                rev: rev.clone(),
            }
        })
        .collect();
    Table {
        style_id: style_id.clone(),
        width: *width,
        jc: *jc,
        own_jc: *own_jc,
        ind: *ind,
        ind_pct: *ind_pct,
        ind_set: *ind_set,
        borders: borders.clone(),
        fixed: *fixed,
        cell_spacing: *cell_spacing,
        look: *look,
        bands: *bands,
        alt: None,
        floating: *floating,
        bidi_visual: *bidi_visual,
        grid: grid.clone(),
        rows,
        src: None,
        id: *id,
        direct: Default::default(),
        controls: Vec::new(),
        change: change.clone(),
        grid_change: grid_change.clone(),
    }
}

pub(crate) fn same_shell(s: &Table, t: &Table) -> bool {
    let Table {
        style_id,
        width,
        jc,
        own_jc,
        ind,
        ind_pct,
        ind_set,
        borders,
        fixed,
        cell_spacing,
        look,
        bands,
        floating,
        bidi_visual,
        alt: _,
        grid,
        rows,
        src: _,
        id,
        direct: _,
        controls: _,
        change,
        grid_change,
    } = s;
    *style_id == t.style_id
        && *width == t.width
        && *jc == t.jc
        && *own_jc == t.own_jc
        && *ind == t.ind
        && *ind_pct == t.ind_pct
        && *ind_set == t.ind_set
        && *borders == t.borders
        && *fixed == t.fixed
        && *cell_spacing == t.cell_spacing
        && *look == t.look
        && *bands == t.bands
        && *floating == t.floating
        && *bidi_visual == t.bidi_visual
        && *grid == t.grid
        && *id == t.id
        && *change == t.change
        && *grid_change == t.grid_change
        && rows.len() == t.rows.len()
        && rows.iter().zip(&t.rows).all(|(r, q)| {
            let Row {
                height,
                cant_split,
                header,
                grid_before,
                grid_after,
                w_before,
                w_after,
                ind,
                jc,
                div,
                borders,
                shading,
                hidden: _,
                vanished,
                cell_spacing: row_spacing,
                run,
                nudge,
                cells,
                src: _,
                direct: _,
                exceptions: _,
                controls: _,
                rev,
            } = r;
            *height == q.height
                && *cant_split == q.cant_split
                && *header == q.header
                && *grid_before == q.grid_before
                && *grid_after == q.grid_after
                && *w_before == q.w_before
                && *w_after == q.w_after
                && *ind == q.ind
                && *jc == q.jc
                && *div == q.div
                && *borders == q.borders
                && *shading == q.shading
                && *vanished == q.vanished
                && *row_spacing == q.cell_spacing
                && *run == q.run
                && *nudge == q.nudge
                && *rev == q.rev
                && cells.len() == q.cells.len()
                && cells.iter().zip(&q.cells).all(|(c, d)| {
                    let Cell {
                        width,
                        span,
                        v_merge,
                        top,
                        left,
                        bottom,
                        right,
                        shading,
                        margins,
                        v_align,
                        text_direction,
                        no_wrap,
                        fit_text,
                        hide_mark,
                        diagonals,
                        blocks: _,
                        direct: _,
                        controls: _,
                        rev,
                        ids: _,
                    } = c;
                    *width == d.width
                        && *span == d.span
                        && *v_merge == d.v_merge
                        && *top == d.top
                        && *left == d.left
                        && *bottom == d.bottom
                        && *right == d.right
                        && *shading == d.shading
                        && *margins == d.margins
                        && *v_align == d.v_align
                        && *text_direction == d.text_direction
                        && *no_wrap == d.no_wrap
                        && *fit_text == d.fit_text
                        && *hide_mark == d.hide_mark
                        && *diagonals == d.diagonals
                        && *rev == d.rev
                })
        })
}

pub(super) fn table_bytes(t: &TableLayout) -> usize {
    std::mem::size_of::<TableLayout>()
        + t.rows.iter().map(|r| std::mem::size_of::<RowBox>() + r.cells.len() * std::mem::size_of::<CellBox>()).sum::<usize>()
        + t.fit.iter().map(|r| r.cells.len() * std::mem::size_of::<autofit::Cell>()).sum::<usize>()
}

#[derive(Debug)]
pub struct RowKey {
    row: RowBox,
    ri: usize,
    at: [u64; 2],
    last: bool,
    x0: u64,
    spacing: u64,
    cols: Vec<f64>,
    outer: [Option<Border>; 4],
    bottom_border: u64,
    rtl: bool,
}

pub struct RowProbe<'a> {
    pub(super) tb: &'a TableLayout,
    pub(super) ri: usize,
    pub(super) at: [u64; 2],
    pub(super) last: bool,
}

fn same_row(a: &RowBox, b: &RowBox) -> bool {
    a.top_border == b.top_border
        && a.changed == b.changed
        && a.exact == b.exact
        && a.gap_top == b.gap_top
        && a.part_top == b.part_top
        && a.bottom_gap == b.bottom_gap
        && a.seams == b.seams
        && a.cells.len() == b.cells.len()
        && a.cells.iter().zip(&b.cells).all(|(x, y)| {
            Rc::ptr_eq(&x.units, &y.units)
                && (x.x, x.w, x.cx, x.cw, x.content_h, x.align_h, x.below) == (y.x, y.w, y.cx, y.cw, y.content_h, y.align_h, y.below)
                && x.margins == y.margins
                && x.v_align == y.v_align
                && x.v_merge == y.v_merge
                && (x.col, x.span, x.index) == (y.col, y.span, y.index)
                && x.edges == y.edges
                && x.shading == y.shading
                && x.closing == y.closing
        })
}

impl CacheKey for RowKey {
    type Probe<'a> = RowProbe<'a>;
    const PIECE: Option<crate::prof::Piece> = Some(crate::prof::Piece::Row);

    fn bucket(p: &RowProbe) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = crate::layout::cache::KeyHasher::default();
        (p.tb.id, p.ri, p.at, p.last).hash(&mut h);
        for c in &p.tb.rows[p.ri].cells {
            Rc::as_ptr(&c.units).hash(&mut h);
        }
        h.finish()
    }

    fn matches(&self, p: &RowProbe) -> bool {
        let tb = p.tb;
        self.ri == p.ri
            && self.at == p.at
            && self.last == p.last
            && self.x0 == tb.x0.to_bits()
            && self.spacing == tb.spacing.to_bits()
            && self.bottom_border == tb.bottom_border.to_bits()
            && self.cols == tb.cols
            && self.outer == tb.outer
            && self.rtl == tb.rtl
            && same_row(&self.row, &tb.rows[p.ri])
    }

    fn from_probe(p: &RowProbe) -> RowKey {
        let tb = p.tb;
        RowKey {
            row: tb.rows[p.ri].clone(),
            ri: p.ri,
            at: p.at,
            last: p.last,
            x0: tb.x0.to_bits(),
            spacing: tb.spacing.to_bits(),
            cols: tb.cols.clone(),
            outer: tb.outer.clone(),
            bottom_border: tb.bottom_border.to_bits(),
            rtl: tb.rtl,
        }
    }
}
