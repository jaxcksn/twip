//! Table layout

use crate::docx::Compat;
use std::rc::Rc;

use crate::docx::model::*;
use crate::docx::props::{Border, Jc, Width};
use crate::docx::review::MarkupView;

use super::autofit;
use super::cache::cached;
use super::fragment::{BoxKind, Child, Item, TableFragment};
use super::ids::{ParaId, TableId, Unit};
use super::job::{Memos, Yield};
use super::story::{CellEnds, NestedRow, StoryEnv, StoryLayout, StoryUnit};

mod assemble;
mod borders;
mod draw;
mod key;
mod widths;

pub use assemble::{cells_box, float_box, legacy_lead, nested_shift, outer_box};
pub(super) use borders::{drawn, edge, thick};
pub use draw::{CellPart, DrawnRow, draw_row, draw_whole_row, frame_part, whole};
pub(super) use draw::{clip_box, clip_x};
pub(crate) use key::same_shell;
pub use key::{RowKey, TableKey};

use assemble::{assemble, cell_edges, edge_add, place, row_borders, spacing_of, vertical_cell};
use key::{TableProbe, table_bytes};
use widths::{Fit, fit_inputs, grid_columns, resolve_fit, twips};

#[derive(Clone, Debug, PartialEq)]
pub struct CellBox {
    pub x: f64,
    pub w: f64,
    pub cx: f64,
    pub cw: f64,
    pub margins: [f64; 4],
    pub units: Rc<StoryLayout>,
    pub content_h: f64,
    pub align_h: f64,
    pub fixed_floats: Box<[u64]>,
    pub v_align: String,
    pub v_merge: VMerge,
    pub col: usize,
    pub span: usize,
    pub edges: Rc<[Option<Border>; 4]>,
    /// The top and bottom edges where a part of the table starts or ends on a page (the cell's own, else the table's
    /// top and bottom), and the top edge under a repeated header row.
    pub cut: Rc<[Option<Border>; 3]>,
    pub shading: Option<u32>,
    pub below: f64,
    pub index: usize,
    pub closing: Option<(ParaId, f64)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RowBox {
    pub cells: Vec<CellBox>,
    pub top_border: f64,
    pub height: f64,
    pub cant_split: bool,
    pub keep_next: bool,
    pub break_before: bool,
    pub header: bool,
    pub exact: bool,
    pub min_h: f64,
    pub gap_top: f64,
    /// Extra room above the row when it starts a later part of the table on a page.
    pub part_top: f64,
    /// Extra room above the row when it follows repeated header rows.
    pub header_top: f64,
    /// A framed row's bottom band and the gap below it, held in its height.
    pub bottom_gap: f64,
    pub seams: Vec<(f64, f64, Border)>,
    pub hidden: bool,
    pub changed: bool,
    /// The row's HTML division: room above and below it, and its boxes.
    pub lead: f64,
    pub trail: f64,
    pub div_boxes: Rc<[crate::docx::model::DivBox]>,
    pub shading: Option<u32>,
    /// The row's cell spacing; its cells are framed when it has one.
    pub spacing: Option<f64>,
    /// The next row is framed: the row draws its own bottom border.
    pub closes: bool,
    /// The bottom border the row must fit above the column bottom with: the widest of its cells' own, else the
    /// table's.
    pub fit_border: f64,
}

impl TableLayout {
    pub fn is_last(&self, ri: usize) -> bool {
        self.rows.get(ri + 1..).is_none_or(|rest| rest.iter().all(|r| r.hidden))
    }

    pub(crate) fn keeps_rows_above(&self, new: &TableLayout, row: usize) -> bool {
        self.cols == new.cols
            && self.x0 == new.x0
            && self.rows.len() == new.rows.len()
            && self.rows[..row].iter().zip(&new.rows[..row]).all(|(a, b)| a.height == b.height && a.top_border == b.top_border)
    }

    pub(crate) fn same_rows_from(&self, new: &TableLayout, r: usize) -> bool {
        self.cols == new.cols
            && self.x0 == new.x0
            && self.bottom_border == new.bottom_border
            && self.rows.len() == new.rows.len()
            && self.rows.get(r..) == new.rows.get(r..)
    }
}

#[derive(Debug, PartialEq)]
pub struct TableLayout {
    pub x0: f64,
    pub cols: Vec<f64>,
    pub rows: Vec<RowBox>,
    pub bottom_border: f64,
    pub spacing: f64,
    pub id: TableId,
    pub outer: [Option<Border>; 4],
    pub fit: Rc<[FitRow]>,
    pub rtl: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct FitRow {
    cells: Vec<autofit::Cell>,
    widths: Vec<Rc<(i64, i64, i64)>>,
}

struct CellGeo {
    ci: usize,
    col: usize,
    span: usize,
    x: f64,
    w: f64,
    cx: f64,
    cw: f64,
    /// The cell's box inside its borders, from the cell's left edge: where it starts and its width.
    inner: (f64, f64),
    edges: Rc<[Option<Border>; 4]>,
}

pub fn layout(t: &Table, text_w: f64, env: &StoryEnv, prev: Option<&ParaStyle>, measure: bool, memos: &mut Memos) -> Result<Rc<TableLayout>, Yield> {
    layout_with(t, text_w, env, prev, measure, None, false, memos)
}

/// A table inside a cell: its available width takes the compat 15 edge allowances in every mode.
pub fn layout_nested(t: &Table, text_w: f64, env: &StoryEnv, measure: bool, memos: &mut Memos) -> Result<Rc<TableLayout>, Yield> {
    layout_with(t, text_w, env, None, measure, None, true, memos)
}

pub fn relayout_rows(
    t: &Table,
    text_w: f64,
    env: &StoryEnv,
    prev: Option<&ParaStyle>,
    old: &TableLayout,
    rows: std::ops::RangeInclusive<usize>,
    memos: &mut Memos,
) -> Result<Rc<TableLayout>, Yield> {
    if old.rows.len() != t.rows.len() || old.fit.len() != t.rows.len() {
        return layout(t, text_w, env, prev, false, memos);
    }
    layout_with(t, text_w, env, prev, false, Some((old, rows)), false, memos)
}

fn layout_with(
    t: &Table,
    text_w: f64,
    env: &StoryEnv,
    prev: Option<&ParaStyle>,
    measure: bool,
    reuse: Option<(&TableLayout, std::ops::RangeInclusive<usize>)>,
    nested: bool,
    memos: &mut Memos,
) -> Result<Rc<TableLayout>, Yield> {
    // A pct indent places nothing (the indent is unset), but the autofit room still loses its share of the width
    // the table is laid out in, truncated to twips; none while measuring, and none for a floating table.
    let resolved;
    let t = match t.ind_pct {
        Some(p) => {
            let ind = if measure || t.floating.is_some() { 0 } else { (p * 5000.0).round() as i64 * twips(text_w) / 5000 };
            resolved = Table { ind: ind as f64 / 20.0, ind_pct: None, ..t.clone() };
            &resolved
        }
        None => t,
    };
    if let Some(runs) = layout_runs(t) {
        return layout_runs_of(t, &runs, text_w, env, prev, measure, nested, memos);
    }
    match in_view(t, env.ctx.env.text.review.view, env.ctx.expansions) {
        None => layout_shown(t, text_w, env, prev, measure, reuse, nested, memos),
        Some(v) => {
            let mut tb = v.expand(&*layout_shown(&v.table, text_w, env, prev, measure, None, nested, memos)?);
            if hidden_tail(t)
                && env.ctx.env.text.compat == Compat::Legacy
                && let Some(tb) = Rc::get_mut(&mut tb)
            {
                tb.bottom_border = 0.0;
            }
            Ok(tb)
        }
    }
}

/// The table to lay out in less room than its column: its pct width and pct indent stay shares of the column.
pub fn in_column(t: &Table, text_w: f64, env: &StoryEnv) -> Table {
    let mut t = t.clone();
    if let Some(p) = t.ind_pct.take() {
        t.ind = ((p * 5000.0).round() as i64 * twips(text_w) / 5000) as f64 / 20.0;
    }
    if let Width::Pct(p) = t.width {
        let (add_l, add_r) = edge_add(env.ctx.env.text.compat, &t);
        let a = twips(text_w + add_l + add_r);
        t.width = Width::Dxa(autofit::rdiv(a, (p * 5000.0).round() as i64, 5000) as f64 / 20.0);
    }
    t
}

// Layout runs
fn layout_runs(t: &Table) -> Option<Vec<std::ops::Range<usize>>> {
    if t.fixed || t.rows.iter().all(|r| r.run.is_none()) {
        return None;
    }
    let mut runs: Vec<std::ops::Range<usize>> = Vec::new();
    for ri in 0..t.rows.len() {
        match runs.last_mut() {
            Some(run) if t.rows[run.start].run.is_some() == t.rows[ri].run.is_some() => run.end = ri + 1,
            _ => runs.push(ri..ri + 1),
        }
    }
    Some(runs)
}

fn layout_runs_of(
    t: &Table,
    runs: &[std::ops::Range<usize>],
    text_w: f64,
    env: &StoryEnv,
    prev: Option<&ParaStyle>,
    measure: bool,
    nested: bool,
    memos: &mut Memos,
) -> Result<Rc<TableLayout>, Yield> {
    let mut merged: Option<TableLayout> = None;
    let mut fit: Vec<FitRow> = Vec::new();
    let n = runs.len();
    for (k, run) in runs.iter().enumerate() {
        let first = &t.rows[run.start];
        let mut sub = Table { rows: t.rows[run.clone()].to_vec(), ..t.clone() };
        if k > 0 || first.run.is_some() {
            sub.jc = first.jc.or(t.own_jc).unwrap_or(Jc::Left);
        }
        if let Some(width) = first.run {
            sub.fixed = true;
            sub.width = width;
            if let Some(ind) = first.ind {
                sub.ind = ind;
                sub.ind_set = true;
            }
            sub.borders = row_borders(t, run.start).clone();
            sub.cell_spacing = spacing_of(t, run.start).unwrap_or(0.0);
            sub.rows.iter_mut().for_each(|r| r.run = None);
        }
        let bounds = own_grid(&mut sub);
        let laid = layout_with(&sub, text_w, env, if k == 0 { prev } else { None }, measure, None, nested, memos)?;
        let old = |g: usize| bounds[g.min(bounds.len() - 1)];
        let at = |c: usize, span: usize| (old(c), old(c + span) - old(c));
        fit.extend(laid.fit.iter().cloned().map(|mut f| {
            f.cells.iter_mut().for_each(|c| (c.first, c.span) = at(c.first, c.span));
            f
        }));
        let shift = merged.as_ref().map_or(0.0, |m| laid.x0 - m.x0);
        let mut rows: Vec<RowBox> = laid.rows.clone();
        for r in &mut rows {
            r.cells.iter_mut().for_each(|c| {
                c.x += shift;
                (c.col, c.span) = at(c.col, c.span);
            });
            r.seams.iter_mut().for_each(|s| {
                s.0 += shift;
                s.1 += shift;
            });
        }
        if k + 1 < n
            && let Some(last) = rows.last_mut()
        {
            last.height += laid.bottom_border;
            last.trail += laid.bottom_border;
        }
        match merged.as_mut() {
            None => {
                // A merged column's width goes to its first grid column.
                let cols = (0..bounds[bounds.len() - 1])
                    .map(|g| bounds.iter().position(|&b| b == g).and_then(|j| laid.cols.get(j)).copied().unwrap_or(0.0))
                    .collect();
                merged = Some(TableLayout { rows, cols, fit: Rc::from([]), ..clone_layout(&laid) });
            }
            Some(m) => {
                m.rows.extend(rows);
                m.bottom_border = laid.bottom_border;
            }
        }
    }
    let mut m = merged.expect("a run");
    m.fit = fit.into();
    Ok(Rc::new(m))
}

/// Merges the grid columns no row of the table has an edge between, as a layout run's own grid; returns where each
/// remaining column edge was on the old grid.
fn own_grid(t: &mut Table) -> Vec<usize> {
    let n = grid_columns(t);
    if t.grid_change.is_some() {
        return (0..=n).collect();
    }
    let mut edge = vec![false; n + 1];
    (edge[0], edge[n]) = (true, true);
    for r in &t.rows {
        let mut x = r.grid_before.min(n);
        edge[x] = true;
        for c in &r.cells {
            x = (x + c.span).min(n);
            edge[x] = true;
        }
        edge[(x + r.grid_after).min(n)] = true;
    }
    let bounds: Vec<usize> = (0..=n).filter(|&g| edge[g]).collect();
    if bounds.len() == n + 1 {
        return bounds;
    }
    let new = |g: usize| bounds.partition_point(|&b| b < g);
    t.grid = bounds.windows(2).map(|w| (w[0]..w[1]).filter_map(|g| t.grid.get(g)).sum()).collect();
    for r in &mut t.rows {
        let mut x = r.grid_before.min(n);
        let gb = new(x);
        for c in &mut r.cells {
            let to = (x + c.span).min(n);
            c.span = new(to) - new(x);
            x = to;
        }
        let end = (x + r.grid_after).min(n);
        r.grid_after = new(end) - new(x);
        r.grid_before = gb;
    }
    bounds
}

fn clone_layout(tb: &TableLayout) -> TableLayout {
    TableLayout {
        x0: tb.x0,
        cols: tb.cols.clone(),
        rows: Vec::new(),
        bottom_border: tb.bottom_border,
        spacing: tb.spacing,
        id: tb.id,
        outer: tb.outer.clone(),
        fit: tb.fit.clone(),
        rtl: tb.rtl,
    }
}

// Markup view
struct ViewTable {
    table: Table,
    rows: Vec<Option<Vec<usize>>>,
}

fn in_view(t: &Table, view: MarkupView, expansions: Option<&super::expansion::Expansions>) -> Option<ViewTable> {
    let original = view == MarkupView::Original;
    let grid = t.table_grid_in(view);
    let differs = |c: &Cell| view.hides_cell(&c.rev) || c.v_merge_in(view) != c.v_merge || original && c.rev.change.is_some();
    let hides = t.rows.iter().any(|r| r.vanished || view.hides_row(&r.rev) || original && r.rev.change.is_some() || r.cells.iter().any(differs));
    let shown_row = |ri: usize| expansions.and_then(|e| e.rows.get(&(super::ids::TableId(t.id), ri)));
    let expanded = expansions.is_some_and(|e| !e.rows.is_empty()) && (0..t.rows.len()).any(|ri| shown_row(ri).is_some());
    if !hides && !expanded && grid == t.grid.as_slice() && !(original && t.change.is_some()) {
        return None;
    }
    let mut table = t.clone();
    table.grid = grid.to_vec();
    carry_hidden_bottoms(&mut table);
    if let Some(p) = t.change.as_ref().and_then(|c| c.previous.as_ref()).filter(|_| original) {
        p.restore_table(&mut table);
    }
    let previous = |c: &Option<std::rc::Rc<crate::docx::review::TableChange>>| c.as_ref().filter(|_| original).and_then(|c| c.previous.clone());
    let mut rows = Vec::with_capacity(t.rows.len());
    table.rows = std::mem::take(&mut table.rows)
        .into_iter()
        .enumerate()
        .filter_map(|(ri, mut r)| {
            if let Some(body) = shown_row(ri) {
                let n = r.cells.len().max(1);
                r.cells = body.cells.clone();
                rows.push(Some((0..r.cells.len()).map(|k| k.min(n - 1)).collect()));
                return Some(r);
            }
            if r.vanished || view.hides_row(&r.rev) {
                rows.push(None);
                return None;
            }
            let keep: Vec<usize> = (0..r.cells.len()).filter(|&k| !view.hides_cell(&r.cells[k].rev)).collect();
            r.cells = std::mem::take(&mut r.cells).into_iter().filter(|c| !view.hides_cell(&c.rev)).collect();
            if let Some(p) = previous(&r.rev.change) {
                p.restore_row(&mut r);
            }
            for c in &mut r.cells {
                c.v_merge = c.v_merge_in(view);
                if let Some(p) = previous(&c.rev.change) {
                    p.restore_cell(c);
                }
            }
            rows.push(Some(keep));
            Some(r)
        })
        .collect();
    Some(ViewTable { table, rows })
}

impl ViewTable {
    fn expand(&self, tb: &TableLayout) -> Rc<TableLayout> {
        let (mut shown, mut fit) = (tb.rows.iter().zip(tb.fit.iter()), Vec::with_capacity(self.rows.len()));
        let mut rows = Vec::with_capacity(self.rows.len());
        for keep in &self.rows {
            let Some((keep, (r, f))) = keep.as_ref().and_then(|k| Some((k, shown.next()?))) else {
                let row = RowBox {
                    cells: Vec::new(),
                    top_border: 0.0,
                    height: 0.0,
                    cant_split: false,
                    keep_next: false,
                    break_before: false,
                    header: false,
                    exact: false,
                    min_h: 0.0,
                    gap_top: 0.0,
                    part_top: 0.0,
                    header_top: 0.0,
                    bottom_gap: 0.0,
                    seams: Vec::new(),
                    hidden: true,
                    changed: false,
                    lead: 0.0,
                    trail: 0.0,
                    div_boxes: Rc::from([]),
                    shading: None,
                    spacing: None,
                    closes: false,
                    fit_border: 0.0,
                };
                rows.push(row);
                fit.push(FitRow::default());
                continue;
            };
            let mut r = r.clone();
            for c in &mut r.cells {
                c.index = keep.get(c.index).copied().unwrap_or(c.index);
            }
            rows.push(r);
            fit.push(f.clone());
        }
        Rc::new(TableLayout {
            x0: tb.x0,
            cols: tb.cols.clone(),
            rows,
            bottom_border: tb.bottom_border,
            spacing: tb.spacing,
            id: tb.id,
            outer: tb.outer.clone(),
            fit: fit.into(),
            rtl: tb.rtl,
        })
    }
}

fn layout_shown(
    t: &Table,
    text_w: f64,
    env: &StoryEnv,
    prev: Option<&ParaStyle>,
    measure: bool,
    reuse: Option<(&TableLayout, std::ops::RangeInclusive<usize>)>,
    nested: bool,
    memos: &mut Memos,
) -> Result<Rc<TableLayout>, Yield> {
    let kept = |ri: usize| reuse.as_ref().filter(|(_, rows)| !rows.contains(&ri)).map(|(o, _)| *o);
    let n = grid_columns(t);
    let near = |ri: usize| reuse.as_ref().is_some_and(|(_, rows)| ri + 1 >= *rows.start() && ri <= rows.end() + 1);
    let single = env.ctx.env.text.single_cell_borders;
    let edges = crate::prof::time("table:edges", || cell_edges(t, n, single, &|ri| reuse.as_ref().filter(|_| !near(ri)).map(|(o, _)| &o.rows[ri])));
    let inputs = crate::prof::time("table:fit", || fit_inputs(t, env, memos, &edges, &|ri| kept(ri).map(|o| &o.fit[ri])))?;
    let Fit { cols, min_t, width, hard_t } = resolve_fit(t, text_w, env, None, nested, &inputs);
    let geo = if cols.len() == n { place(t, &cols, &edges) } else { cell_geometry(t, &cols, single) };

    let mut stories = Vec::new();
    let mut closings = Vec::new();
    let contextual = super::paragraph::contextual_in(&env.ctx.env.text, true);
    let mark = Some(&env.ctx.env.text.default_style);
    let view = env.ctx.env.text.review.view;
    fn style(b: Option<&Block>, view: crate::docx::review::MarkupView) -> Option<&Option<String>> {
        match b {
            Some(Block::Para(p)) => Some(&p.para_style_in(view).style_id),
            _ => None,
        }
    }
    let drops = |b: Option<&Block>, other: Option<&Option<String>>| {
        contextual && matches!(b, Some(Block::Para(p)) if super::paragraph::drops_at_edge(p.para_style_in(view), other))
    };
    crate::prof::time("table:cells", || -> Result<(), Yield> {
        for (ri, (r, cells)) in t.rows.iter().zip(&geo).enumerate() {
            let old_row = kept(ri).map(|o| &o.rows[ri]).filter(|o| o.cells.len() == cells.len());
            for (gi, g) in cells.iter().enumerate() {
                let c = &r.cells[g.ci];
                closings.push(closing_para(c));
                let rotated = vertical_cell(c).is_some();
                let same = old_row.map(|o| &o.cells[gi]).filter(|o| o.index == g.ci && o.cw == g.cw);
                stories.push(if measure || c.v_merge == VMerge::Continue || rotated {
                    None
                } else if let Some(o) = same {
                    Some(o.units.clone())
                } else {
                    let n = c.blocks.len();
                    let skip = closing_para(c).is_some();
                    let blocks: Vec<&Block> = c.blocks[..if skip { n - 1 } else { n }].iter().collect();
                    let before = match g.ci {
                        0 if ri == 0 => prev.map(|p| &p.style_id),
                        0 => mark,
                        ci => style(r.cells[ci - 1].blocks.last(), view),
                    };
                    let after = r.cells.get(g.ci + 1).map_or(mark, |n| style(n.blocks.first(), view));
                    let page = (((g.cx - g.inner.0) * 20.0).round() as i32, (g.inner.1 * 20.0).round() as i32);
                    let ends = CellEnds {
                        before: drops(c.blocks.first(), before),
                        after: drops(c.blocks.last(), after),
                        fixed: t.fixed,
                        fit: c.fit_text,
                        page,
                    };
                    let shaded;
                    let cell_env = match c.shading.filter(|f| super::env::is_dark(*f)) {
                        Some(fill) => {
                            let text = super::env::TextEnv { backdrop: Some(fill), ..env.ctx.env.text.clone() };
                            shaded = super::LayoutEnv { text, ..env.ctx.env.clone() };
                            StoryEnv { ctx: super::LayoutCtx { env: &shaded, ..env.ctx }, ..*env }
                        }
                        None => *env,
                    };
                    let story = super::story::layout(&blocks, g.cw, &cell_env, Some(ends), memos)?;
                    // lrTbV: horizontal, with East Asian glyphs turned.
                    Some(if text_flow(&c.text_direction) == 4 {
                        let units =
                            story.units.iter().map(|u| StoryUnit { content: super::vertical::turn_east_asian(&u.content).into(), ..u.clone() });
                        Rc::new(StoryLayout { units: units.collect(), ..(*story).clone() })
                    } else {
                        story
                    })
                });
            }
        }
        Ok(())
    })?;
    let probe = TableProbe {
        table: t,
        text_w: text_w.to_bits(),
        measure,
        cols: &cols,
        fit: (min_t, width, hard_t),
        cells: &stories,
        text: &env.ctx.env.text,
        fonts: env.ctx.fonts.revision(),
    };
    if !memos.cache.store_tables {
        return Ok(match memos.cache.tables.lookup(&probe) {
            Some(tb) => tb,
            None => Rc::new(assemble(t, text_w, env, cols, &geo, &stories, &closings, inputs.into(), nested, memos)?),
        });
    }
    cached(
        memos,
        |c| &mut c.tables,
        probe,
        table_bytes,
        |m| assemble(t, text_w, env, cols.clone(), &geo, &stories, &closings, inputs.into(), nested, m),
    )
}

// Hidden rows
fn carry_hidden_bottoms(t: &mut Table) {
    let starts = |r: &Row| {
        let mut col = r.grid_before;
        r.cells
            .iter()
            .map(|c| {
                let s = col;
                col += c.span;
                s
            })
            .collect::<Vec<usize>>()
    };
    for ri in (1..t.rows.len()).rev() {
        if !t.rows[ri].vanished || t.rows[ri - 1].vanished {
            continue;
        }
        let below = ri + t.rows[ri..].iter().take_while(|r| r.vanished).count() - 1;
        let hidden = &t.rows[below];
        let at = starts(hidden);
        let bottoms: Vec<(usize, usize, Option<Border>)> = hidden.cells.iter().zip(&at).map(|(c, &s)| (s, s + c.span, c.bottom.clone())).collect();
        let inside = hidden.borders.as_ref().map(|b| b[4].clone());
        let above = &mut t.rows[ri - 1];
        let at = starts(above);
        for (c, s) in above.cells.iter_mut().zip(at) {
            c.bottom = bottoms.iter().find(|(a, b, _)| s >= *a && s < *b).and_then(|x| x.2.clone());
        }
        if let Some(inside) = inside {
            let mut b = above.borders.as_deref().cloned().unwrap_or_else(|| t.borders.clone());
            b[4] = inside;
            above.borders = Some(Rc::new(b));
        }
    }
}

fn hidden_tail(t: &Table) -> bool {
    t.rows.last().is_some_and(|r| r.vanished)
}

fn mark_only(p: &crate::docx::Paragraph) -> bool {
    p.inlines.iter().all(|i| match i {
        Inline::Text(t, r) => t.is_empty() || r.hidden,
        Inline::Bookmark(..) | Inline::Range(..) => true,
        i => i.run().is_some_and(|r| r.hidden),
    })
}

fn closing_para(c: &Cell) -> Option<(ParaId, f64)> {
    let bordered = |s: &crate::docx::ParaStyle| {
        [&s.border_top, &s.border_left, &s.border_bottom, &s.border_right, &s.border_between].iter().any(|b| b.is_some())
    };
    match c.blocks.as_slice() {
        [.., Block::Table(_), Block::Para(p)] if p.inlines.is_empty() => Some((ParaId(p.id), p.mark.size)),
        [.., Block::Para(p)] if c.hide_mark && mark_only(p) && p.list.is_none() && !bordered(&p.style) => Some((ParaId(p.id), p.mark.size)),
        _ => None,
    }
}

fn holds_float(c: &Cell) -> bool {
    c.blocks.iter().any(|b| matches!(b, Block::Para(p) if p.inlines.iter().any(|i| matches!(i, Inline::Float(f, _) if f.in_cell(Compat::Legacy)))))
}

// Cell geometry
fn cell_geometry(t: &Table, cols: &[f64], single: bool) -> Vec<Vec<CellGeo>> {
    place(t, cols, &cell_edges(t, cols.len(), single, &|_| None))
}

// Story units
pub fn table_units(tb: &Rc<TableLayout>) -> (Vec<StoryUnit>, f64) {
    let mut y = 0.0;
    let mut units = Vec::new();
    let n = tb.rows.len();
    for ri in (0..n).filter(|&ri| !tb.rows[ri].hidden) {
        let h = tb.rows[ri].height;
        let last = tb.is_last(ri);
        let row = draw_row(tb, ri, 0.0, 0.0, h, None, last);
        let extra = if last { tb.bottom_border } else { 0.0 };
        let notes: Vec<(NoteKind, i64)> = tb.rows[ri].cells.iter().flat_map(|c| c.units.units.iter().flat_map(|u| u.notes.iter().copied())).collect();
        let fragment = Child::Table(TableFragment { table: tb.id, rows: Box::new([row]) });
        units.push(StoryUnit {
            top: y,
            height: h + extra,
            below: 0.0,
            after: 0.0,
            hold: 0.0,
            content: Rc::from([fragment]),
            block: 0,
            from: 0,
            widow: false,
            keep_next: false,
            keep_lines: false,
            notes: notes.into_boxed_slice(),
            row: Some(Rc::new(NestedRow { table: tb.clone(), ri, shift: 0.0, rest: None })),
        });
        y += h + extra;
    }
    (units, y)
}

pub(super) const INK_SLACK: f64 = 0.06;

fn ink_top(children: &[Child]) -> Option<f64> {
    let line = |dy: f64, items: &[Item]| {
        items
            .iter()
            .flat_map(|it| if let Item::Glyphs(r) = it { r.glyphs.iter().map(|g| (r, g.0)).collect::<Vec<_>>() } else { Vec::new() })
            .filter_map(|(r, g)| r.face.ink_top(g).map(|t| dy + r.baseline - t * r.face.scale(r.size)))
            .reduce(f64::min)
    };
    children
        .iter()
        .filter_map(|c| match c {
            Child::Line { placed, .. } => line(placed.y, &placed.line.items),
            Child::Para(p) => p.lines.iter().filter_map(|l| line(l.y, &l.line.items)).reduce(f64::min),
            Child::Group { offset, children } => ink_top(children).map(|t| t + offset.1),
            _ => None,
        })
        .reduce(f64::min)
}

fn source_units(children: &[Child], out: &mut Vec<(ParaId, Unit, Unit)>) {
    for c in children {
        match c {
            Child::Para(p) => p.lines.iter().for_each(|l| line_units(&l.line.items, Some(p.para), out)),
            Child::Line { para, placed } => line_units(&placed.line.items, *para, out),
            Child::Group { children, .. } => source_units(children, out),
            Child::Box(b) if !matches!(b.kind, BoxKind::Float { .. }) => source_units(&b.children, out),
            Child::Box(_) => {}
            Child::Table(t) => t.rows.iter().flat_map(|r| r.cells.iter()).for_each(|cell| source_units(&cell.content, out)),
            Child::Items(_) => {}
        }
    }
}

fn line_units(items: &[Item], para: Option<ParaId>, out: &mut Vec<(ParaId, Unit, Unit)>) {
    for it in items {
        if let Item::Glyphs(r) = it
            && let Some(para) = r.para.or(para)
        {
            let mut units = r.chars.iter().filter_map(|c| c.2);
            if let Some(first) = units.next() {
                let (lo, hi) = units.fold((first, first), |(lo, hi), u| (lo.min(u), hi.max(u)));
                out.push((para, lo, Unit(hi.0 + 1)));
            }
        }
    }
}
