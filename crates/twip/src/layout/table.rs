//! Tables: column widths (TABLE-4 to TABLE-22), cell content, row heights (TABLE-27 to TABLE-32), borders
//! (TABLE-36), and drawing a row as a [`RowFragment`]. Rows are paginated by the column fill (TABLE-40 to TABLE-47).

use std::rc::Rc;

use crate::docx::model::*;
use crate::docx::props::{Border, Jc, Width};
use crate::docx::review::{MarkupView, ReviewView};

use super::autofit;
use super::cache::{CacheKey, cached};
use super::env::TextEnv;
use super::fragment::{BoxKind, CellFragment, CellPlace, Child, GlyphRun, Item, LineFragment, PlacedLine, RowFragment, TableFragment};
use super::ids::{ParaId, TableId, Unit};
use super::job::{Memos, Yield};
use super::join::LaidBlock;
use super::line::{break_lines, neighbour_borders};
use super::space::ParaSpace;
use super::story::{StoryEnv, StoryKey, StoryLayout, StoryUnit, child_ids};
use super::text::{Kind, shape};

const EPS: f64 = 1e-6;

/// What a table's layout depends on: the table's own properties (its cells' content is in `cells`), the
/// available width, the column widths, each cell's story by identity (`None`: not laid out), and the `TextEnv` and
/// font revision.
#[derive(Debug)]
pub struct TableKey {
    shell: Table,
    text_w: u64,
    measure: bool,
    cols: Vec<f64>,
    /// The table's autofit minimum, width and hard minimum: a measured table's key has no cells, and what a cell
    /// holding it needs comes from these, not from the columns.
    fit: (i64, i64, i64),
    cells: Vec<Option<Rc<StoryLayout>>>,
    text: TextEnv,
    fonts: u64,
}

/// A [`TableKey`] borrowed.
pub struct TableProbe<'a> {
    table: &'a Table,
    text_w: u64,
    measure: bool,
    cols: &'a [f64],
    fit: (i64, i64, i64),
    cells: &'a [Option<Rc<StoryLayout>>],
    text: &'a TextEnv,
    fonts: u64,
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
        let mut h = super::cache::KeyHasher::default();
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
    }

    fn from_probe(p: &TableProbe) -> TableKey {
        TableKey {
            shell: shell(p.table),
            text_w: p.text_w,
            measure: p.measure,
            cols: p.cols.to_vec(),
            fit: p.fit,
            cells: p.cells.to_vec(),
            text: p.text.clone(),
            fonts: p.fonts,
        }
    }
}

/// A table's properties without its cells' content (which its key holds by identity). The destructuring is
/// exhaustive, so a field added to the model must be decided on here and in [`same_shell`].
fn shell(t: &Table) -> Table {
    let Table {
        style_id,
        width,
        jc,
        ind,
        borders,
        fixed,
        cell_spacing,
        look,
        bands,
        floating,
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
            let Row { height, cant_split, header, grid_before, grid_after, w_before, w_after, cells, src: _, controls: _, rev } = r;
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
                        blocks: _,
                        direct: _,
                        controls: _,
                        rev,
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
                        blocks: Vec::new(),
                        direct: Default::default(),
                        controls: Vec::new(),
                        rev: rev.clone(),
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
                cells,
                src: None,
                controls: Vec::new(),
                rev: rev.clone(),
            }
        })
        .collect();
    Table {
        style_id: style_id.clone(),
        width: *width,
        jc: *jc,
        ind: *ind,
        borders: borders.clone(),
        fixed: *fixed,
        cell_spacing: *cell_spacing,
        look: *look,
        bands: *bands,
        floating: *floating,
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

/// Whether `t`'s properties equal the shell's (cells' content not compared).
pub(crate) fn same_shell(s: &Table, t: &Table) -> bool {
    let Table {
        style_id,
        width,
        jc,
        ind,
        borders,
        fixed,
        cell_spacing,
        look,
        bands,
        floating,
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
        && *ind == t.ind
        && *borders == t.borders
        && *fixed == t.fixed
        && *cell_spacing == t.cell_spacing
        && *look == t.look
        && *bands == t.bands
        && *floating == t.floating
        && *grid == t.grid
        && *id == t.id
        && *change == t.change
        && *grid_change == t.grid_change
        && rows.len() == t.rows.len()
        && rows.iter().zip(&t.rows).all(|(r, q)| {
            let Row { height, cant_split, header, grid_before, grid_after, w_before, w_after, cells, src: _, controls: _, rev } = r;
            *height == q.height
                && *cant_split == q.cant_split
                && *header == q.header
                && *grid_before == q.grid_before
                && *grid_after == q.grid_after
                && *w_before == q.w_before
                && *w_after == q.w_after
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
                        blocks: _,
                        direct: _,
                        controls: _,
                        rev,
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
                        && *rev == d.rev
                })
        })
}

// shortcut: entry sizes are rough (the structs, not what they share); refine with memory budgets.
fn table_bytes(t: &TableLayout) -> usize {
    std::mem::size_of::<TableLayout>()
        + t.rows.iter().map(|r| std::mem::size_of::<RowBox>() + r.cells.len() * std::mem::size_of::<CellBox>()).sum::<usize>()
        + t.fit.iter().map(|r| r.len() * std::mem::size_of::<autofit::Cell>()).sum::<usize>()
}

/// A cell's laid-out content.
#[derive(Clone, Debug, PartialEq)]
pub struct CellBox {
    /// Left border centre, relative to the table's left border centre.
    pub x: f64,
    /// Border centre to border centre.
    pub w: f64,
    /// Content box: left offset from `x` and width (TABLE-24).
    pub cx: f64,
    /// The width its content was laid out at (an edit in another row reuses `units` at this width).
    pub cw: f64,
    pub margins: [f64; 4],
    pub units: Rc<StoryLayout>,
    pub content_h: f64,
    pub v_align: String,
    pub v_merge: VMerge,
    /// The grid columns this cell spans.
    pub col: usize,
    pub span: usize,
    /// Resolved edges (TABLE-36): top, left, bottom, right; shared with the cell's geometry.
    pub edges: Rc<[Option<Border>; 4]>,
    pub shading: Option<u32>,
    /// A merge's first cell: the height of the rows it covers beyond its own row (TABLE-31).
    pub below: f64,
    /// The cell's index in its model row.
    pub index: usize,
    /// A closing empty paragraph after a nested table, which takes no space (TABLE-26): its id and its mark's
    /// font size, for the caret place editing shows there.
    pub closing: Option<(ParaId, f64)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RowBox {
    pub cells: Vec<CellBox>,
    /// TABLE-27: the widest top border among the row's cells.
    pub top_border: f64,
    /// The row's pitch (top border included).
    pub height: f64,
    pub cant_split: bool,
    pub header: bool,
    pub exact: bool,
    /// Cell spacing (TABLE-39): from the row top to its cell boxes (2s, plus the table's top border on row 0).
    pub gap_top: f64,
    /// Bottom borders of the row above whose cells no cell of this row starts under (their edge was resolved against
    /// the cell below, which draws its own top from another cell, TABLE-38): left, right (table coordinates) and
    /// border.
    pub seams: Vec<(f64, f64, Border)>,
    /// The markup view hides the row (REV-1): it has no cells and takes no space, and the fill skips it.
    pub hidden: bool,
    /// The row or one of its cells was inserted, deleted or changed, and the view draws change bars (REV-4).
    pub changed: bool,
}

impl TableLayout {
    /// Whether row `ri` is the last one shown (the rows after it are hidden by the markup view, REV-1).
    pub fn is_last(&self, ri: usize) -> bool {
        self.rows.get(ri + 1..).is_none_or(|rest| rest.iter().all(|r| r.hidden))
    }
}

/// A table laid out for a width.
#[derive(Debug, PartialEq)]
pub struct TableLayout {
    /// Left border centre, relative to the column's left edge.
    pub x0: f64,
    pub cols: Vec<f64>,
    pub rows: Vec<RowBox>,
    /// TABLE-27: the last row's bottom border.
    pub bottom_border: f64,
    /// `w:tblCellSpacing` and the table's outer borders (top, left, bottom, right), drawn separately with spacing.
    pub spacing: f64,
    pub id: TableId,
    pub outer: [Option<Border>; 4],
    /// Each row's cells' autofit inputs (width-free): an edit in other rows reuses them.
    pub fit: Rc<[Vec<autofit::Cell>]>,
}

fn style_factor(style: &str) -> f64 {
    match style {
        "double" => 3.0,
        "triple" => 3.5,
        "dashed" | "dotted" | "dashSmallGap" | "dotDash" | "dotDotDash" => 0.75,
        _ => 1.0,
    }
}

/// ST_Border enumeration order (ties: the later style wins, TABLE-36).
fn style_rank(style: &str) -> usize {
    const ORDER: [&str; 27] = [
        "nil",
        "none",
        "single",
        "thick",
        "double",
        "dotted",
        "dashed",
        "dotDash",
        "dotDotDash",
        "triple",
        "thinThickSmallGap",
        "thickThinSmallGap",
        "thinThickThinSmallGap",
        "thinThickMediumGap",
        "thickThinMediumGap",
        "thinThickThinMediumGap",
        "thinThickLargeGap",
        "thickThinLargeGap",
        "thinThickThinLargeGap",
        "wave",
        "doubleWave",
        "dashSmallGap",
        "dashDotStroked",
        "threeDEmboss",
        "threeDEngrave",
        "outset",
        "inset",
    ];
    ORDER.iter().position(|s| *s == style).unwrap_or(2)
}

/// TABLE-36: which of two borders meeting on an edge is drawn. `(border, cell_level)`.
fn conflict(a: (Option<&Border>, bool), b: (Option<&Border>, bool)) -> Option<Border> {
    let real = |x: Option<&Border>| x.filter(|b| b.visible()).cloned();
    let (ra, rb) = (real(a.0), real(b.0));
    match (ra, rb) {
        (None, None) => None,
        (Some(x), None) => Some(x),
        (None, Some(y)) => Some(y),
        (Some(x), Some(y)) => {
            if a.1 != b.1 {
                return Some(if a.1 { x } else { y });
            }
            let (wx, wy) = (x.width * style_factor(&x.style), y.width * style_factor(&y.style));
            if (wx - wy).abs() > EPS {
                return Some(if wx > wy { x } else { y });
            }
            let (sx, sy) = (style_rank(&x.style), style_rank(&y.style));
            if sx != sy {
                return Some(if sx > sy { x } else { y });
            }
            let key = |c: Option<u32>| {
                let c = c.unwrap_or(0);
                let (r, g, b) = ((c >> 16) & 255, (c >> 8) & 255, c & 255);
                (r + b + 2 * g, b + 2 * g, g)
            };
            Some(if key(x.color) <= key(y.color) { x } else { y })
        }
    }
}

fn bw(b: &Option<Border>) -> f64 {
    b.as_ref().filter(|b| b.visible()).map_or(0.0, thick)
}

/// A border's thickness (TABLE-37).
fn thick(b: &Border) -> f64 {
    if b.style == "double" { 3.0 * b.width } else { b.width }
}

/// Draws border `b` filling `rect` (its thickness across, horizontal or vertical): a double border as its two lines.
fn edge(out: &mut Vec<Item>, rect: [f64; 4], horizontal: bool, b: &Border) {
    let colour = b.color.unwrap_or(0);
    let [x0, y0, x1, y1] = rect;
    if b.style != "double" {
        out.push(Item::Rect { rect, colour });
    } else if horizontal {
        out.push(Item::Rect { rect: [x0, y0, x1, y0 + b.width], colour });
        out.push(Item::Rect { rect: [x0, y1 - b.width, x1, y1], colour });
    } else {
        out.push(Item::Rect { rect: [x0, y0, x0 + b.width, y1], colour });
        out.push(Item::Rect { rect: [x1 - b.width, y0, x1, y1], colour });
    }
}

/// TABLE-17: half a border in whole twips, rounded up.
fn half_border(w: f64) -> f64 {
    let tw = (w * 20.0).round() as i64;
    ((tw + 1) / 2) as f64 / 20.0
}

/// TABLE-25: what a cell story shifts a nested table right by, the difference [`assemble`]'s compat <= 14 position
/// leaves. Left-aligned tables only.
pub fn nested_shift(t: &Table) -> f64 {
    if matches!(t.jc, Jc::Center | Jc::Right) {
        return 0.0;
    }
    // The first cell's own left border, nil included, else the table's (TABLE-25).
    let (lw, lmar) = t
        .rows
        .first()
        .and_then(|r| r.cells.first())
        .map_or((bw(&t.borders[1]), 0.0), |c| (if c.left.is_some() { bw(&c.left) } else { bw(&t.borders[1]) }, c.margins[1]));
    lw / 2.0 + lmar.max(half_border(lw))
}

/// TABLE-17: the per-side edge allowance added to the available width (left, right).
fn edge_add(compat: i64, t: &Table) -> (f64, f64) {
    let first = t.rows.first();
    let lc = first.and_then(|r| r.cells.first());
    let rc = first.and_then(|r| r.cells.last());
    let lw = bw(&conflict((lc.and_then(|c| c.left.as_ref()), true), (t.borders[1].as_ref(), false)));
    let rw = bw(&conflict((rc.and_then(|c| c.right.as_ref()), true), (t.borders[3].as_ref(), false)));
    if compat >= 15 {
        (-half_border(lw), -half_border(rw))
    } else {
        (half_border(lw).max(lc.map_or(0.0, |c| c.margins[1])), half_border(rw).max(rc.map_or(0.0, |c| c.margins[3])))
    }
}

/// TABLE-39: the cell-spacing gap on a cell side: 2s on the table's outer boundary, s inside.
fn gap(s: f64, outer: bool) -> f64 {
    if outer { 2.0 * s } else { s }
}

/// Content min, max and hard minimum of blocks in twips (TABLE-7), cached and width-free (LayoutNG's
/// min/max sizes).
fn content_widths(blocks: &[Block], env: &StoryEnv, memos: &mut Memos) -> Result<(i64, i64, i64), Yield> {
    let laid = super::join::laid(blocks, env.ctx.env.text.review.view);
    let probe = StoryKey::new(child_ids(&laid, None, env, memos)?, None, true, env);
    cached(memos, |c| &mut c.cell_widths, probe, |_| std::mem::size_of::<(i64, i64, i64)>(), |m| content_widths_uncached(&laid, env, m)).map(|w| *w)
}

/// Points to twips, rounded up (TABLE-6: content widths round up).
fn up(x: f64) -> i64 {
    (x * 20.0 - 1e-6).ceil().max(0.0) as i64
}

/// Points to twips, rounded.
fn twips(x: f64) -> i64 {
    (x * 20.0).round() as i64
}

/// TABLE-7, TABLE-8: the widest unbreakable piece and the widest unwrapped line, with the paragraph's indents.
/// A nested table supplies its own autofit's minimum, width with A = MAXW, and hard minimum (TABLE-11).
fn content_widths_uncached(blocks: &[LaidBlock], env: &StoryEnv, memos: &mut Memos) -> Result<(i64, i64, i64), Yield> {
    let (mut mn, mut mx, mut hard) = (0i64, 0i64, 0i64);
    let old = env.ctx.env.text.compat < 15;
    for b in blocks {
        match b {
            LaidBlock::Para(laid) => {
                let p = laid.para();
                let space = laid.shape_space(env.fields, env.notes, &env.ctx.env.text);
                let shaped = shape(p, &space, env.ctx.fonts, memos)?;
                let s = p.para_style_in(env.ctx.env.text.review.view);
                let ps = ParaSpace { width: 1e9, start: 0, borders: neighbour_borders(s, None, None), x0: 0.0, env: env.ctx.env.text.clone() };
                let pb = break_lines(&shaped, p, &ps, env.ctx.fonts, memos)?;
                let ind = if old { s.ind_left + s.ind_right + s.ind_first.min(0.0) } else { s.ind_left + s.ind_right };
                let pieces = pb.pieces();
                // TABLE-9: hyphen runs in the minimum.
                let text_of = |q: &super::text::Piece| shaped.text.get(q.text.0 as usize..q.text.1 as usize).unwrap_or("");
                let mut tail = vec![false; pieces.len() + 1]; // pieces from here on are hyphens or spaces only
                tail[pieces.len()] = true;
                for i in (0..pieces.len()).rev() {
                    let t = text_of(&pieces[i]);
                    tail[i] = tail[i + 1] && (pieces[i].kind == Kind::Space || (!t.is_empty() && t.chars().all(|c| c == '-')));
                }
                let mut word = 0.0f64;
                for (i, pc) in pieces.iter().enumerate() {
                    match pc.kind {
                        Kind::LineBreak | Kind::PageBreak | Kind::ColumnBreak | Kind::Space => {
                            mn = mn.max(up(word + ind));
                            word = 0.0;
                        }
                        _ => {
                            word += pc.width;
                            let joined = old && text_of(pc).starts_with('-') && tail[i + 1];
                            if pc.break_after && !joined {
                                mn = mn.max(up(word + ind));
                                word = 0.0;
                            }
                        }
                    }
                }
                mn = mn.max(up(word + ind));
                // Each line unwrapped: from its start (the first line's indent included, so a list number in the
                // hanging indent counts) to its last piece that isn't a space.
                for (li, l) in pb.lines.iter().enumerate() {
                    let start_x = s.ind_left + if li == 0 { s.ind_first } else { 0.0 };
                    // (Centred or right-aligned in the 1e9pt measuring width, a line starts far right: take that off.)
                    let shift = l.xs.first().map_or(0.0, |&x| (x - start_x).max(0.0));
                    let end = (0..l.xs.len())
                        .rev()
                        .find(|&k| !matches!(pieces.get(l.start + k).map(|q| q.kind), Some(Kind::Space)))
                        .map_or(start_x, |k| l.xs[k] + l.widths[k] - shift);
                    let w = if old { (end - start_x).max(0.0) + ind } else { end.max(0.0) + s.ind_right };
                    mx = mx.max(up(w));
                }
            }
            &LaidBlock::Table(t) => {
                // The text width it takes: its widths less TABLE-17's edge allowances (TABLE-11), or laid out
                // in a cell that wide its A falls short and its words break.
                let shown = in_view(t, env.ctx.env.text.review.view);
                let t = shown.as_ref().map_or(t, |v| &v.table);
                let f = fit(t, 0.0, env, Some(autofit::MAXW), memos)?;
                let (al, ar) = edge_add(env.ctx.env.text.compat, t);
                let e = twips(al) + twips(ar) - twips(t.ind);
                mn = mn.max(f.min_t - e);
                mx = mx.max(f.width - e);
                hard = hard.max(f.hard_t - e);
            }
        }
    }
    Ok((mn, mx.max(mn), hard))
}

/// A table's column widths (points, border centre to border centre) and its autofit minimum, width and hard minimum
/// in twips (what a cell holding it needs).
struct Fit {
    cols: Vec<f64>,
    min_t: i64,
    width: i64,
    hard_t: i64,
}

/// Column widths (TABLE-4 to TABLE-21), for fixed and autofit layout. `avail`: A in twips instead of the text width's.
fn fit(t: &Table, text_w: f64, env: &StoryEnv, avail: Option<i64>, memos: &mut Memos) -> Result<Fit, Yield> {
    let edges = cell_edges(t, grid_columns(t), &|_| None);
    let inputs = fit_inputs(t, env, memos, &edges, &|_| None)?;
    Ok(resolve_fit(t, text_w, env, avail, &inputs))
}

/// The grid's column count.
fn grid_columns(t: &Table) -> usize {
    t.grid.len().max(t.rows.iter().map(|r| r.grid_before + r.cells.iter().map(|c| c.span).sum::<usize>() + r.grid_after).max().unwrap_or(0))
}

/// TABLE-7, by row: each cell's min, max and hard minimum, with its padding. `kept`: a row's inputs as they were,
/// when its content didn't change.
fn fit_inputs<'k>(
    t: &Table,
    env: &StoryEnv,
    memos: &mut Memos,
    geo: &[Vec<CellEdges>],
    kept: &dyn Fn(usize) -> Option<&'k Vec<autofit::Cell>>,
) -> Result<Vec<Vec<autofit::Cell>>, Yield> {
    let n = grid_columns(t);
    let s2 = twips(2.0 * t.cell_spacing.max(0.0));
    let spacing = twips(t.cell_spacing.max(0.0));
    let grid_tw: Vec<i64> = (0..n).map(|i| twips(t.grid.get(i).copied().unwrap_or(0.0))).collect();
    let mut rows = Vec::with_capacity(t.rows.len());
    for (ri, (r, gs)) in t.rows.iter().zip(geo).enumerate() {
        if let Some(k) = kept(ri).filter(|k| k.len() == gs.len()) {
            rows.push(k.clone());
            continue;
        }
        let mut cells: Vec<autofit::Cell> = Vec::with_capacity(gs.len());
        for g in gs {
            let c = &r.cells[g.ci];
            let (bl, br) = (twips(bw(&g.edges[1])), twips(bw(&g.edges[3])));
            let (ml, mr) = (twips(c.margins[1]), twips(c.margins[3]));
            let gap = |b: i64, m: i64| if t.cell_spacing > 0.0 { spacing + b + m } else { m };
            let pad = (gap(bl, ml) + gap(br, mr)).max((bl + br + 1) / 2);
            let cell_pad = (pad - 2 * spacing).max(0);
            let continuation = c.v_merge == VMerge::Continue;
            let grid_w: i64 = grid_tw[g.col..g.col + g.span].iter().sum();
            let (kind, pref) = match c.width {
                Some(Width::Dxa(w)) => (autofit::Kind::Dxa, twips(w)),
                Some(Width::Pct(f)) => (autofit::Kind::Pct, (f * 5000.0).round() as i64),
                // TABLE-12: w:tcW type="nil": dxa at the cell's grid width less its spacing.
                Some(Width::Nil) => (autofit::Kind::Dxa, grid_w - s2),
                // TABLE-12: fixed layout, an auto cell (or one without w:tcW) takes its grid width.
                Some(Width::Auto) | None if t.fixed => (autofit::Kind::Dxa, grid_w - s2),
                // TABLE-12: no w:tcW reads as auto.
                Some(Width::Auto) | None => (autofit::Kind::Auto, 0),
            };
            let (min, max, hard) = if t.fixed {
                // TABLE-5: fixed layout ignores the content: min = max = padding + 20.
                (cell_pad + 20, cell_pad + 20, cell_pad)
            } else {
                let rotated = matches!(c.text_direction.as_str(), "btLr" | "tbRl" | "tbRlV" | "tbLrV");
                let (mut cmn, mut cmx, chard) = if continuation {
                    (0, 0, 0)
                } else if rotated {
                    (284, 284, 0) // TABLE-33: one line thick (≈ 14.2pt for Calibri 11)
                } else {
                    content_widths(&c.blocks, env, memos)?
                };
                if (cmn, cmx) == (0, 0) && !continuation {
                    (cmn, cmx) = (6, 6); // TABLE-7: an empty cell
                }
                // (w:noWrap doesn't change the widths, TABLE-10.)
                let min = (cmn + cell_pad).min(autofit::MAXW);
                let max = (cmx + cell_pad).max(min).min(autofit::MAXW);
                (min, max, (chard + cell_pad).min(autofit::MAXW))
            };
            let explicit = matches!(c.width, Some(Width::Dxa(_)));
            cells.push(autofit::Cell { first: g.col, span: g.span, min, max, hard, kind, pref, skip: continuation && ri > 0, explicit });
        }
        rows.push(cells);
    }
    Ok(rows)
}

/// TABLE-13 to TABLE-19 from the cells' inputs: the column widths and the table's autofit minimum, width and hard
/// minimum.
fn resolve_fit(t: &Table, text_w: f64, env: &StoryEnv, avail: Option<i64>, inputs: &[Vec<autofit::Cell>]) -> Fit {
    let n = grid_columns(t);
    // TABLE-17: A = text width − indent + the edge allowances.
    let (add_l, add_r) = edge_add(env.ctx.env.text.compat, t);
    let a = avail.unwrap_or_else(|| twips(text_w - t.ind + add_l + add_r));
    let s2 = twips(2.0 * t.cell_spacing.max(0.0));
    let cells: Vec<autofit::Cell> = inputs.iter().flatten().copied().collect();
    // TABLE-13 to TABLE-19: the columns, then the widths from the table-wide inputs.
    let gaps: Vec<autofit::Gap> = t
        .rows
        .iter()
        .flat_map(|r| {
            let after = r.grid_before + r.cells.iter().map(|c| c.span).sum::<usize>();
            [(0, r.grid_before, twips(r.w_before)), (after, r.grid_after, twips(r.w_after))]
        })
        .filter(|&(f, s, w)| s > 0 && w > 0 && f + s <= n)
        .collect();
    let cols = autofit::columns(n, &cells, s2, &gaps, t.fixed);
    let pad_total = s2 * (n as i64 + 1);
    let tbl_w = match t.width {
        Width::Dxa(w) => twips(w),
        Width::Pct(p) => autofit::rdiv(a, (p * 5000.0).round() as i64, 5000),
        _ => 0,
    }
    .min(autofit::MAXW - twips(t.ind));
    let no_grow = !env.ctx.env.text.grow_autofit && tbl_w <= a;
    let r = autofit::resolve(&cols, pad_total, tbl_w, a, t.fixed, no_grow);
    let mut widths = r.widths.clone();
    autofit::with_spacing(&mut widths, s2);
    // shortcut: TABLE-21's second pass (an edge reaching MAXW with growAutofit) isn't done; tables 22in wide only.
    Fit { cols: widths.iter().map(|&w| w as f64 / 20.0).collect(), min_t: r.min_t, width: r.width, hard_t: r.hard_t }
}

/// A cell's place in its table: grid column and span, box x and width, text x and width, and its four edges after
/// conflict resolution (TABLE-36).
struct CellGeo {
    ci: usize,
    col: usize,
    span: usize,
    x: f64,
    w: f64,
    cx: f64,
    cw: f64,
    edges: Rc<[Option<Border>; 4]>,
}

/// Lays out a table for a column (or cell) of width `text_w`. `measure`: widths only (no content layout). The column
/// widths and the cells' stories come from their own cache entries; the rows are cached by those results' identity.
pub fn layout(t: &Table, text_w: f64, env: &StoryEnv, measure: bool, memos: &mut Memos) -> Result<Rc<TableLayout>, Yield> {
    layout_with(t, text_w, env, measure, None, memos)
}

/// [`layout`] after an edit inside rows `rows`, with `old` the table's layout before it: every other row's
/// cells keep their autofit inputs, and their content where it keeps its width, so a keystroke in a long table
/// doesn't measure and look up every cell again. The columns, geometry and rows are still worked out whole.
pub fn relayout_rows(
    t: &Table,
    text_w: f64,
    env: &StoryEnv,
    old: &TableLayout,
    rows: std::ops::RangeInclusive<usize>,
    memos: &mut Memos,
) -> Result<Rc<TableLayout>, Yield> {
    if old.rows.len() != t.rows.len() || old.fit.len() != t.rows.len() {
        return layout(t, text_w, env, false, memos);
    }
    layout_with(t, text_w, env, false, Some((old, rows)), memos)
}

fn layout_with(
    t: &Table,
    text_w: f64,
    env: &StoryEnv,
    measure: bool,
    reuse: Option<(&TableLayout, std::ops::RangeInclusive<usize>)>,
    memos: &mut Memos,
) -> Result<Rc<TableLayout>, Yield> {
    match in_view(t, env.ctx.env.text.review.view) {
        None => layout_shown(t, text_w, env, measure, reuse, memos),
        Some(v) => Ok(v.expand(&*layout_shown(&v.table, text_w, env, measure, None, memos)?)),
    }
}

/// A table as the markup view shows it (REV-1, REV-2): without the rows and cells the view hides, with the grid,
/// vertical merges and table, row and cell properties before their tracked changes in Original. `rows`: per model row,
/// its cells' model indices, or `None` when hidden.
struct ViewTable {
    table: Table,
    rows: Vec<Option<Vec<usize>>>,
}

/// The table `view` shows, when it isn't `t` itself.
// shortcut: in Original a cell keeps the formatting its current table style gives it, even when the table's earlier
// properties name another style; resolving the cells again over that style would show it.
fn in_view(t: &Table, view: MarkupView) -> Option<ViewTable> {
    let original = view == MarkupView::Original;
    let grid = t.table_grid_in(view);
    let differs = |c: &Cell| view.hides_cell(&c.rev) || c.v_merge_in(view) != c.v_merge || original && c.rev.change.is_some();
    let hides = t.rows.iter().any(|r| view.hides_row(&r.rev) || original && r.rev.change.is_some() || r.cells.iter().any(differs));
    if !hides && grid == t.grid.as_slice() && !(original && t.change.is_some()) {
        return None;
    }
    let mut table = t.clone();
    table.grid = grid.to_vec();
    if let Some(p) = t.change.as_ref().and_then(|c| c.previous.as_ref()).filter(|_| original) {
        p.restore_table(&mut table);
    }
    let previous = |c: &Option<std::rc::Rc<crate::docx::review::TableChange>>| c.as_ref().filter(|_| original).and_then(|c| c.previous.clone());
    let mut rows = Vec::with_capacity(t.rows.len());
    table.rows = std::mem::take(&mut table.rows)
        .into_iter()
        .filter_map(|mut r| {
            if view.hides_row(&r.rev) {
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
    /// The shown table's layout with the model's rows: a hidden row takes no space, and cells keep their model index.
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
                    header: false,
                    exact: false,
                    gap_top: 0.0,
                    seams: Vec::new(),
                    hidden: true,
                    changed: false,
                };
                rows.push(row);
                fit.push(Vec::new());
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
        })
    }
}

/// [`layout_with`] for a table as shown.
fn layout_shown(
    t: &Table,
    text_w: f64,
    env: &StoryEnv,
    measure: bool,
    reuse: Option<(&TableLayout, std::ops::RangeInclusive<usize>)>,
    memos: &mut Memos,
) -> Result<Rc<TableLayout>, Yield> {
    let kept = |ri: usize| reuse.as_ref().filter(|(_, rows)| !rows.contains(&ri)).map(|(o, _)| *o);
    let n = grid_columns(t);
    // A row's edges depend on its neighbours: those of rows at least one away from the edited ones are as they were.
    let near = |ri: usize| reuse.as_ref().is_some_and(|(_, rows)| ri + 1 >= *rows.start() && ri <= rows.end() + 1);
    let edges = cell_edges(t, n, &|ri| reuse.as_ref().filter(|_| !near(ri)).map(|(o, _)| &o.rows[ri]));
    let inputs = fit_inputs(t, env, memos, &edges, &|ri| kept(ri).map(|o| &o.fit[ri]))?;
    let Fit { cols, min_t, width, hard_t } = resolve_fit(t, text_w, env, None, &inputs);
    let geo = if cols.len() == n { place(t, &cols, &edges) } else { cell_geometry(t, &cols) };
    let mut stories = Vec::new();
    let mut closings = Vec::new();
    for (ri, (r, cells)) in t.rows.iter().zip(&geo).enumerate() {
        let old_row = kept(ri).map(|o| &o.rows[ri]).filter(|o| o.cells.len() == cells.len());
        for (gi, g) in cells.iter().enumerate() {
            let c = &r.cells[g.ci];
            closings.push(closing_para(c));
            let rotated = matches!(c.text_direction.as_str(), "btLr" | "tbRl" | "tbRlV" | "tbLrV");
            let same = old_row.map(|o| &o.cells[gi]).filter(|o| o.index == g.ci && o.cw == g.cw);
            stories.push(if measure || c.v_merge == VMerge::Continue || rotated {
                None
            } else if let Some(o) = same {
                Some(o.units.clone())
            } else {
                // TABLE-26: a cell's closing empty paragraph after a nested table takes no space.
                let n = c.blocks.len();
                let skip = closing_para(c).is_some();
                let blocks: Vec<&Block> = c.blocks[..if skip { n - 1 } else { n }].iter().collect();
                Some(super::story::layout(&blocks, g.cw, env, true, memos)?)
            });
        }
    }
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
            None => Rc::new(assemble(t, text_w, env, cols, &geo, &stories, &closings, inputs.into())),
        });
    }
    cached(memos, |c| &mut c.tables, probe, table_bytes, |_| Ok(assemble(t, text_w, env, cols.clone(), &geo, &stories, &closings, inputs.into())))
}

/// A cell's closing empty paragraph after a nested table, which takes no space (TABLE-26): its id and mark size.
fn closing_para(c: &Cell) -> Option<(ParaId, f64)> {
    match c.blocks.as_slice() {
        [.., Block::Table(_), Block::Para(p)] if p.inlines.is_empty() => Some((ParaId(p.id), p.mark.size)),
        _ => None,
    }
}

/// Each cell's geometry, by row (cells outside the grid are left out).
fn cell_geometry(t: &Table, cols: &[f64]) -> Vec<Vec<CellGeo>> {
    place(t, cols, &cell_edges(t, cols.len(), &|_| None))
}

/// A cell's grid place and resolved edges (TABLE-36): what its geometry is, before the column widths.
struct CellEdges {
    ci: usize,
    col: usize,
    span: usize,
    edges: Rc<[Option<Border>; 4]>,
}

/// Each cell's grid place and edges, by row, for `ncols` grid columns. `kept`: a row as laid out before, when it and
/// its neighbours didn't change (resolving every cell's borders is most of a long table's keystroke).
fn cell_edges<'k>(t: &Table, ncols: usize, kept: &dyn Fn(usize) -> Option<&'k RowBox>) -> Vec<Vec<CellEdges>> {
    let nrows = t.rows.len();
    // Each cell's side borders (cell level beats table level, TABLE-36).
    struct Side {
        b: Option<Border>,
        cell: bool,
    }
    let side = |own: &Option<Border>, table: &Option<Border>| -> Side {
        match own {
            Some(b) => Side { b: Some(b.clone()), cell: true },
            None => Side { b: table.clone(), cell: false },
        }
    };
    let mut grid_cells: Vec<Vec<(usize, usize)>> = Vec::new(); // per row: (col, span) of each cell
    for r in &t.rows {
        let mut col = r.grid_before;
        let mut v = Vec::new();
        for c in &r.cells {
            v.push((col, c.span));
            col += c.span;
        }
        grid_cells.push(v);
    }
    let mut out = Vec::new();
    for (ri, r) in t.rows.iter().enumerate() {
        if let Some(k) = kept(ri).filter(|k| k.cells.iter().all(|c| c.index < r.cells.len())) {
            out.push(k.cells.iter().map(|c| CellEdges { ci: c.index, col: c.col, span: c.span, edges: c.edges.clone() }).collect());
            continue;
        }
        let mut cells = Vec::new();
        for (ci, c) in r.cells.iter().enumerate() {
            let (col, span) = grid_cells[ri][ci];
            let span = span.min(ncols.saturating_sub(col)).max(if col < ncols { 1 } else { 0 });
            if span == 0 {
                continue;
            }
            let top_t = if ri == 0 { &t.borders[0] } else { &t.borders[4] };
            let bot_t = if ri + 1 == nrows { &t.borders[2] } else { &t.borders[4] };
            let left_t = if col == 0 { &t.borders[1] } else { &t.borders[5] };
            let right_t = if col + span >= ncols { &t.borders[3] } else { &t.borders[5] };
            let (top, left, bottom, right) = (side(&c.top, top_t), side(&c.left, left_t), side(&c.bottom, bot_t), side(&c.right, right_t));
            // Neighbours on shared edges.
            let neighbour = |rr: usize, cc: usize| -> Option<&Cell> {
                t.rows.get(rr).and_then(|row| grid_cells[rr].iter().position(|&(c0, s)| cc >= c0 && cc < c0 + s).map(|i| &row.cells[i]))
            };
            let top_e = if ri == 0 {
                conflict((top.b.as_ref(), top.cell), (None, false))
            } else {
                let above = neighbour(ri - 1, col);
                let ab = above.map(|a| side(&a.bottom, &t.borders[4]));
                conflict((top.b.as_ref(), top.cell), ab.as_ref().map_or((None, false), |s| (s.b.as_ref(), s.cell)))
            };
            let bot_e = if ri + 1 == nrows {
                conflict((bottom.b.as_ref(), bottom.cell), (None, false))
            } else {
                let below = neighbour(ri + 1, col);
                let bl = below.map(|a| side(&a.top, &t.borders[4]));
                conflict((bottom.b.as_ref(), bottom.cell), bl.as_ref().map_or((None, false), |s| (s.b.as_ref(), s.cell)))
            };
            let left_e = if ci == 0 {
                conflict((left.b.as_ref(), left.cell), (None, false))
            } else {
                let lc = &r.cells[ci - 1];
                let ls = side(&lc.right, &t.borders[5]);
                conflict((left.b.as_ref(), left.cell), (ls.b.as_ref(), ls.cell))
            };
            let right_e = if ci + 1 == r.cells.len() {
                conflict((right.b.as_ref(), right.cell), (None, false))
            } else {
                let rc = &r.cells[ci + 1];
                let rs = side(&rc.left, &t.borders[5]);
                conflict((right.b.as_ref(), right.cell), (rs.b.as_ref(), rs.cell))
            };
            cells.push(CellEdges { ci, col, span, edges: Rc::new([top_e, left_e, bot_e, right_e]) });
        }
        out.push(cells);
    }
    out
}

/// The cells' geometry from their edges and the column widths.
fn place(t: &Table, cols: &[f64], edges: &[Vec<CellEdges>]) -> Vec<Vec<CellGeo>> {
    let col_x: Vec<f64> = std::iter::once(0.0)
        .chain(cols.iter().scan(0.0, |acc, w| {
            *acc += w;
            Some(*acc)
        }))
        .collect();
    let ncols = cols.len();
    edges
        .iter()
        .zip(&t.rows)
        .map(|(row, r)| {
            row.iter()
                .map(|e| {
                    let (c, col, span) = (&r.cells[e.ci], e.col, e.span);
                    let (left_e, right_e) = (&e.edges[1], &e.edges[3]);
                    let (x, w) = (col_x[col], col_x[col + span] - col_x[col]);
                    // TABLE-24; with cell spacing, TABLE-39.
                    let (cx, cw) = if t.cell_spacing > 0.0 {
                        let (gl, gr) = (gap(t.cell_spacing, col == 0), gap(t.cell_spacing, col + span >= ncols));
                        let cx = gl + bw(left_e) + c.margins[1];
                        (cx, (w - cx - c.margins[3] - bw(right_e) - gr).max(0.0))
                    } else {
                        let cx = c.margins[1].max(bw(left_e) / 2.0);
                        (cx, (w - cx - c.margins[3].max(bw(right_e) / 2.0)).max(0.0))
                    };
                    CellGeo { ci: e.ci, col, span, x, w, cx, cw, edges: e.edges.clone() }
                })
                .collect()
        })
        .collect()
}

/// The table's rows from its cells' geometry and stories (TABLE-23, TABLE-24, TABLE-27 to TABLE-31, TABLE-39).
fn assemble(
    t: &Table,
    text_w: f64,
    env: &StoryEnv,
    cols: Vec<f64>,
    geo: &[Vec<CellGeo>],
    stories: &[Option<Rc<StoryLayout>>],
    closings: &[Option<(ParaId, f64)>],
    fit: Rc<[Vec<autofit::Cell>]>,
) -> TableLayout {
    let total: f64 = cols.iter().sum();
    let compat = env.ctx.env.text.compat;
    // TABLE-23: the left border centre.
    let first_left = t.rows.first().and_then(|r| r.cells.first()).map(|c| {
        let own = conflict((c.left.as_ref(), true), (t.borders[1].as_ref(), false));
        (bw(&own), c.margins[1])
    });
    let (lw, lmar) = first_left.unwrap_or((bw(&t.borders[1]), 0.0));
    // TABLE-23.
    let mut x0 = if compat >= 15 { t.ind + lw / 2.0 } else { t.ind - lmar.max(half_border(lw)) };
    match t.jc {
        Jc::Center => x0 = (text_w - total) / 2.0,
        Jc::Right => x0 = text_w - total - bw(&t.borders[3]) / 2.0,
        _ => {}
    }
    let mut stories = stories.iter();
    let mut closings = closings.iter();
    let mut rows = Vec::new();
    let review = &env.ctx.env.text.review;
    for ((ri, r), cells_geo) in t.rows.iter().enumerate().zip(geo) {
        let mut cells = Vec::new();
        for g in cells_geo {
            let c = &r.cells[g.ci];
            let shading = tint(review, r, c);
            let units = stories.next().cloned().flatten().unwrap_or_default();
            let content_h = units.height;
            cells.push(CellBox {
                x: g.x,
                w: g.w,
                cx: g.cx,
                cw: g.cw,
                margins: c.margins,
                units,
                content_h,
                v_align: c.v_align.clone(),
                v_merge: c.v_merge,
                col: g.col,
                span: g.span,
                edges: g.edges.clone(),
                shading,
                below: 0.0,
                index: g.ci,
                closing: closings.next().copied().flatten(),
            });
        }
        // REV-4: a revised row or cell marks the row's lines.
        let changed = review.view.change_bars()
            && (r.rev.ins.is_some()
                || r.rev.del.is_some()
                || r.rev.change.is_some()
                || cells_geo.iter().any(|g| r.cells[g.ci].rev.structural.is_some() || r.cells[g.ci].rev.change.is_some()));
        let mut top_border = cells.iter().map(|c| bw(&c.edges[0])).fold(0.0, f64::max);
        // TABLE-38: a bottom border over part of a wider cell, counted in this row's top border.
        let seams: Vec<(f64, f64, Border)> = match ri.checked_sub(1).and_then(|p| geo.get(p)) {
            Some(above) if t.cell_spacing == 0.0 => above
                .iter()
                .filter(|a| !cells_geo.iter().any(|b| b.col == a.col))
                .filter(|a| {
                    cells_geo.iter().find(|b| a.col >= b.col && a.col < b.col + b.span).is_some_and(|b| r.cells[b.ci].v_merge != VMerge::Continue)
                })
                .filter_map(|a| {
                    let b = a.edges[2].clone().filter(|b| b.visible())?;
                    Some((a.x - bw(&a.edges[1]) / 2.0, a.x + a.w + bw(&a.edges[3]) / 2.0, b))
                })
                .collect(),
            _ => Vec::new(),
        };
        top_border = seams.iter().map(|s| thick(&s.2)).fold(top_border, f64::max);
        let mut gap_top = 0.0;
        // TABLE-39: with cell spacing each cell draws its own box.
        let mut cell_bottom = 0.0;
        if t.cell_spacing > 0.0 {
            gap_top = 2.0 * t.cell_spacing + if ri == 0 { bw(&t.borders[0]) } else { 0.0 };
            top_border += gap_top;
            cell_bottom = cells.iter().map(|c| bw(&c.edges[2])).fold(0.0, f64::max);
        }
        let content =
            cells.iter().filter(|c| c.v_merge == VMerge::None).map(|c| c.margins[0] + c.content_h + c.margins[2] + cell_bottom).fold(0.0, f64::max);
        let min_margins = cells.iter().map(|c| c.margins[0] + c.margins[2]).fold(0.0, f64::max);
        let content = content.max(min_margins);
        let (height, exact) = match &r.height {
            Some((h, rule)) if rule == "exact" && *h > 0.0 => (*h, true),
            Some((h, rule)) if rule != "auto" => (top_border + content.max(*h), false),
            _ => (top_border + content, false),
        };
        rows.push(RowBox { cells, top_border, height, cant_split: r.cant_split, header: r.header, exact, gap_top, seams, hidden: false, changed });
    }
    // TABLE-31: a vertically merged region's extra height goes to its last row.
    for ri in 0..rows.len() {
        for ci in 0..rows[ri].cells.len() {
            if rows[ri].cells[ci].v_merge != VMerge::Restart {
                continue;
            }
            let col = rows[ri].cells[ci].col;
            let mut last = ri;
            while last + 1 < rows.len() && rows[last + 1].cells.iter().any(|c| c.col == col && c.v_merge == VMerge::Continue) {
                last += 1;
            }
            let c = &rows[ri].cells[ci];
            let need = rows[ri].top_border + c.margins[0] + c.content_h + c.margins[2];
            let have: f64 = rows[ri..=last].iter().map(|r| r.height).sum();
            if need > have + EPS && !rows[last].exact {
                rows[last].height += need - have;
            }
            rows[ri].cells[ci].below = rows[ri + 1..=last].iter().map(|r| r.height).sum();
        }
    }
    let mut bottom_border = rows.last().map(|r| r.cells.iter().map(|c| bw(&c.edges[2])).fold(0.0, f64::max)).unwrap_or(0.0);
    if t.cell_spacing > 0.0 {
        bottom_border = 2.0 * t.cell_spacing + bw(&t.borders[2]);
    }
    let outer = [t.borders[0].clone(), t.borders[1].clone(), t.borders[2].clone(), t.borders[3].clone()];
    TableLayout { x0, cols, rows, bottom_border, spacing: t.cell_spacing, outer, id: TableId(t.id), fit }
}

/// REV-5: in All markup an inserted or deleted row or cell is tinted with its author's colour at a quarter over its
/// shading.
fn tint(review: &ReviewView, r: &Row, c: &Cell) -> Option<u32> {
    let cell =
        c.rev.structural.as_ref().filter(|(_, k)| matches!(k, crate::docx::review::CellMarkKind::Insert | crate::docx::review::CellMarkKind::Delete));
    let Some(m) = cell.map(|(m, _)| m).or(r.rev.del.as_ref()).or(r.rev.ins.as_ref()).filter(|_| review.view == MarkupView::All) else {
        return c.shading;
    };
    let (a, b) = (review.palette.color(m.author), c.shading.unwrap_or(0xFFFFFF));
    let mix = |s: u32| ((((a >> s) & 0xFF) + 3 * ((b >> s) & 0xFF)) / 4) << s;
    Some(mix(16) | mix(8) | mix(0))
}

/// A cell's units to draw: a slice of its story's units, and the top they are rebased to 0 from (a split row's part).
#[derive(Clone, Copy)]
pub struct CellPart<'a> {
    pub units: &'a [StoryUnit],
    pub base: f64,
    /// The part reaches the end of the cell's content (the last part of a split row, or a whole cell).
    pub ends: bool,
}

impl CellPart<'_> {
    /// The part's height: its last unit's bottom, rebased.
    pub fn height(&self) -> f64 {
        self.units.last().map_or(0.0, |l| (l.top - self.base) + l.height)
    }
}

/// The whole of every cell's content (an unsplit row).
pub fn whole(row: &RowBox) -> Vec<CellPart<'_>> {
    row.cells.iter().map(|c| CellPart { units: &c.units.units, base: 0.0, ends: true }).collect()
}

/// A cell unit's lines clipped to the cell's box `lo..hi` (x in the unit's coordinates, TABLE-35). None when nothing
/// is outside.
// shortcut: whole glyphs are kept or dropped here; cutting a partly visible glyph at the edge is the painter's job
// (a clip rectangle per cell), not done yet.
fn clip_x(children: &Rc<[Child]>, lo: f64, hi: f64, bottom: f64) -> Option<Rc<[Child]>> {
    let inside = |r: &GlyphRun, g: &(u32, f64), dx: f64, dy: f64| {
        let k = r.size / r.face.upem;
        // (Below an exact row's bottom, TABLE-34: the glyph's ink top must reach above it.)
        if bottom.is_finite() && r.face.ink_top(g.0).is_some_and(|t| dy + r.baseline - t * k >= bottom - EPS) {
            return false;
        }
        match r.face.ink(g.0) {
            Some((bearing, width)) => {
                let x0 = dx + g.1 + bearing * k;
                x0 < hi - EPS && x0 + width * k > lo + EPS
            }
            None => (lo..hi).contains(&(dx + g.1)),
        }
    };
    let spills = |l: &PlacedLine| {
        l.line.items.iter().any(|i| match i {
            Item::Glyphs(r) => r.glyphs.iter().any(|g| !inside(r, g, l.x, l.y)),
            Item::Rect { rect, .. } => l.x + rect[0] < lo - EPS || l.x + rect[2] > hi + EPS,
            _ => false,
        })
    };
    if !children.iter().any(|c| matches!(c, Child::Line { placed, .. } if spills(placed))) {
        return None;
    }
    let clipped = children.iter().map(|c| match c {
        Child::Line { para, placed } if spills(placed) => {
            let (dx, dy) = (placed.x, placed.y);
            // Characters (at their clusters' origins) follow the glyphs kept.
            let items: Vec<Item> = placed
                .line
                .items
                .iter()
                .filter_map(|i| match i {
                    Item::Glyphs(r) => {
                        let glyphs: Box<[(u32, f64)]> = r.glyphs.iter().copied().filter(|g| inside(r, g, dx, dy)).collect();
                        let (from, to) = glyphs.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), g| (a.min(g.1), b.max(g.1)));
                        let chars: Box<[(char, f64, Option<Unit>)]> =
                            r.chars.iter().copied().filter(|ch| ch.1 >= from - EPS && ch.1 <= to + EPS).collect();
                        (!glyphs.is_empty() || r.glyphs.is_empty()).then(|| Item::Glyphs(GlyphRun { glyphs, chars, ..r.clone() }))
                    }
                    Item::Rect { rect, colour } => {
                        let (x0, x1) = (rect[0].max(lo - dx), rect[2].min(hi - dx));
                        (x1 > x0).then_some(Item::Rect { rect: [x0, rect[1], x1, rect[3]], colour: *colour })
                    }
                    other => Some(other.clone()),
                })
                .collect();
            let atoms = placed.line.atoms.iter().copied().filter(|a| (lo..hi).contains(&(dx + a.x0))).collect();
            let line = Rc::new(LineFragment {
                items: items.into_boxed_slice(),
                pitch: placed.line.pitch,
                baseline: placed.line.baseline,
                atoms,
                changed: placed.line.changed,
            });
            Child::Line { para: *para, placed: PlacedLine { line, ..placed.clone() } }
        }
        other => other.clone(),
    });
    Some(clipped.collect())
}

/// Draws row `ri` with its top at `top` and the column's left edge at `left`; `parts`: the units of each cell to draw
/// (a row broken across pages), else all of them.
pub fn draw_row(tb: &TableLayout, ri: usize, left: f64, top: f64, height: f64, parts: Option<&[CellPart]>, last_of_table: bool) -> RowFragment {
    let row = &tb.rows[ri];
    let x0 = left + tb.x0;
    let (mut background, mut cells, mut borders) = (Vec::new(), Vec::new(), Vec::new());
    // The row's place for table handles: its extent and the column borders crossing it.
    let mut edges: Vec<(f64, usize)> = Vec::new();
    let mut rect = [x0, top, x0, top + height];
    if let (Some(first), Some(end)) = (row.cells.first(), row.cells.last()) {
        for c in &row.cells {
            for e in [(x0 + c.x, c.col), (x0 + c.x + c.w, c.col + c.span)] {
                if !edges.iter().any(|x| x.1 == e.1) {
                    edges.push(e);
                }
            }
        }
        rect = [x0 + first.x, top, x0 + end.x + end.w, top + height];
    }
    for (k, c) in row.cells.iter().enumerate() {
        let part = parts.map(|p| p[k]).unwrap_or(CellPart { units: &c.units.units, base: 0.0, ends: true });
        // Merged regions: the content sits in the first cell, over the whole region (TABLE-32). TABLE-30: the cell's
        // content height as the row measured it (its last space-after included) when the cell is whole; a part of a
        // split row, its lines.
        let ch = if part.ends && part.base == 0.0 && part.units.len() == c.units.units.len() { c.content_h } else { part.height() };
        // A merge's content, alignment, clip, shading and side borders cover the whole region.
        let height = height + c.below;
        let inner = height - row.top_border - c.margins[0] - c.margins[2];
        let off = match c.v_align.as_str() {
            "center" => ((inner - ch) / 2.0).max(0.0),
            "bottom" => (inner - ch).max(0.0),
            _ => 0.0,
        };
        let base_y = top + row.top_border + c.margins[0] + off;
        let base_x = x0 + c.x + c.cx;
        // shortcut: a region is clipped at its rows' total when its own row is exact.
        let clip = if row.exact { top + height } else { f64::INFINITY };
        let mut content = Vec::new();
        let mut hidden = Vec::new();
        for u in part.units {
            let ut = u.top - part.base;
            // TABLE-28, TABLE-34: an exact row clips its content per glyph.
            let bottom = clip - (base_y + ut) + INK_SLACK;
            let hide = match ink_top(&u.content) {
                Some(top) => top >= bottom - EPS,
                None => base_y + ut >= clip - EPS,
            };
            if hide {
                source_units(&u.content, &mut hidden);
                continue;
            }
            // Text past the cell's box (a negative indent) is clipped at its edges, in the content's coordinates.
            let children = clip_x(&u.content, -c.cx, c.w - c.cx, bottom).unwrap_or_else(|| u.content.clone());
            content.push(Child::Group { offset: (base_x, base_y + ut), children });
        }
        // The closing paragraph's caret place: below the cell's last content, on the part that shows it.
        let closing = c.closing.filter(|_| part.ends).map(|(para, size)| (para, [base_x, base_y + ch, size * 1.2]));
        let place = CellPlace {
            index: c.index,
            col: c.col,
            span: c.span,
            rect: [x0 + c.x, top, x0 + c.x + c.w, top + height],
            clip: row.exact.then_some(clip),
            v_merge: c.v_merge,
            hidden: hidden.into_boxed_slice(),
            closing,
        };
        cells.push(CellFragment { cell: k, content: content.into_boxed_slice(), place });
        if tb.spacing > 0.0 {
            // TABLE-39: the cell's own box, its borders inside it.
            let ncols = tb.cols.len();
            let bx0 = x0 + c.x + gap(tb.spacing, c.col == 0);
            let bx1 = x0 + c.x + c.w - gap(tb.spacing, c.col + c.span >= ncols);
            let (by0, by1) = (top + row.gap_top, top + height);
            if let Some(fill) = c.shading {
                background.push(Item::Rect { rect: [bx0, by0, bx1, by1], colour: fill });
            }
            let [t, l, b, r] = &*c.edges;
            let vis = |e: &'_ Option<Border>| e.as_ref().filter(|b| b.visible()).map(|b| (thick(b), b.clone()));
            if let Some((w, e)) = vis(t) {
                edge(&mut borders, [bx0, by0, bx1, by0 + w], true, &e);
            }
            if let Some((w, e)) = vis(b) {
                edge(&mut borders, [bx0, by1 - w, bx1, by1], true, &e);
            }
            if let Some((w, e)) = vis(l) {
                edge(&mut borders, [bx0, by0, bx0 + w, by1], false, &e);
            }
            if let Some((w, e)) = vis(r) {
                edge(&mut borders, [bx1 - w, by0, bx1, by1], false, &e);
            }
            continue;
        }
        if let Some(fill) = c.shading {
            background.push(Item::Rect { rect: [x0 + c.x, top, x0 + c.x + c.w, top + height], colour: fill });
        }
        // Borders as filled rectangles centred on the cell edges; edges without one are gridlines (screen only).
        let [t, l, b, r] = &*c.edges;
        let (cx0, cx1) = (x0 + c.x, x0 + c.x + c.w);
        let shown = |e: &Option<Border>| e.as_ref().is_some_and(|b| b.visible());
        if !shown(t) && c.v_merge != VMerge::Continue {
            borders.push(Item::Gridline { line: [cx0, top, cx1, top] });
        }
        if !shown(l) {
            borders.push(Item::Gridline { line: [cx0, top, cx0, top + height] });
        }
        if !shown(r) {
            borders.push(Item::Gridline { line: [cx1, top, cx1, top + height] });
        }
        if last_of_table && !shown(b) {
            borders.push(Item::Gridline { line: [cx0, top + height, cx1, top + height] });
        }
        // Inside a merge there is no edge between its rows.
        if let Some(t) = t.as_ref().filter(|b| b.visible() && c.v_merge != VMerge::Continue) {
            edge(&mut borders, [cx0 - bw(l) / 2.0, top, cx1 + bw(r) / 2.0, top + thick(t)], true, t);
        }
        if let Some(l) = l.as_ref().filter(|b| b.visible()) {
            edge(&mut borders, [cx0 - thick(l) / 2.0, top, cx0 + thick(l) / 2.0, top + height], false, l);
        }
        if let Some(r) = r.as_ref().filter(|b| b.visible()) {
            edge(&mut borders, [cx1 - thick(r) / 2.0, top, cx1 + thick(r) / 2.0, top + height], false, r);
        }
        if last_of_table && let Some(b) = b.as_ref().filter(|b| b.visible()) {
            edge(&mut borders, [cx0 - bw(l) / 2.0, top + height, cx1 + bw(r) / 2.0, top + height + thick(b)], true, b);
        }
    }
    for (a, b, s) in &row.seams {
        edge(&mut borders, [x0 + a, top, x0 + b, top + thick(s)], true, s);
    }
    if tb.spacing > 0.0 {
        // The table's own outer border, drawn as usual around the spaced cells (junction ticks not drawn).
        let [t, l, b, r] = &tb.outer;
        let x1 = x0 + tb.cols.iter().sum::<f64>();
        let (lw, rw) = (bw(l), bw(r));
        let bottom = top + height + if last_of_table { tb.bottom_border - bw(b) } else { 0.0 };
        if ri == 0
            && let Some(t) = t.as_ref().filter(|b| b.visible())
        {
            edge(&mut borders, [x0 - lw / 2.0, top, x1 + rw / 2.0, top + thick(t)], true, t);
        }
        if let Some(l) = l.as_ref().filter(|b| b.visible()) {
            edge(&mut borders, [x0 - lw / 2.0, top, x0 + lw / 2.0, bottom], false, l);
        }
        if let Some(r) = r.as_ref().filter(|b| b.visible()) {
            edge(&mut borders, [x1 - rw / 2.0, top, x1 + rw / 2.0, bottom], false, r);
        }
        if last_of_table && let Some(b) = b.as_ref().filter(|b| b.visible()) {
            edge(&mut borders, [x0 - lw / 2.0, bottom, x1 + rw / 2.0, bottom + thick(b)], true, b);
        }
    }
    RowFragment {
        row: ri,
        rect,
        top,
        edges: edges.into_boxed_slice(),
        background: background.into_boxed_slice(),
        cells: cells.into_boxed_slice(),
        borders: borders.into_boxed_slice(),
        repeat: false,
        changed: row.changed,
    }
}

/// A whole row drawn ([`draw_whole_row`]). Compared only by the cache's verify mode, through its debug form (the
/// fragment types have no equality).
#[derive(Debug)]
pub struct DrawnRow(RowFragment);

impl PartialEq for DrawnRow {
    fn eq(&self, o: &Self) -> bool {
        format!("{:?}", self.0) == format!("{:?}", o.0)
    }
}

/// What drawing a whole row reads: the row (its cells' content by identity), where it goes, its height,
/// whether it ends the table, and the table's left edge, cell spacing, columns and outer borders.
#[derive(Debug)]
pub struct RowKey {
    row: RowBox,
    ri: usize,
    at: [u64; 3],
    last: bool,
    x0: u64,
    spacing: u64,
    cols: Vec<f64>,
    outer: [Option<Border>; 4],
    bottom_border: u64,
}

/// A [`RowKey`] borrowed.
pub struct RowProbe<'a> {
    tb: &'a TableLayout,
    ri: usize,
    at: [u64; 3],
    last: bool,
}

/// Rows equal for drawing: their cells' content the same layouts, everything else equal.
fn same_row(a: &RowBox, b: &RowBox) -> bool {
    a.top_border == b.top_border
        && a.changed == b.changed
        && a.exact == b.exact
        && a.gap_top == b.gap_top
        && a.seams == b.seams
        && a.cells.len() == b.cells.len()
        && a.cells.iter().zip(&b.cells).all(|(x, y)| {
            Rc::ptr_eq(&x.units, &y.units)
                && (x.x, x.w, x.cx, x.cw, x.content_h, x.below) == (y.x, y.w, y.cx, y.cw, y.content_h, y.below)
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

    fn bucket(p: &RowProbe) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = super::cache::KeyHasher::default();
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
        }
    }
}

/// [`draw_row`] for a whole row, through the cache: a page step draws its rows again, and while typing most
/// are where they were.
pub fn draw_whole_row(tb: &TableLayout, ri: usize, left: f64, top: f64, height: f64, last_of_table: bool, memos: &mut Memos) -> RowFragment {
    let probe = RowProbe { tb, ri, at: [left.to_bits(), top.to_bits(), height.to_bits()], last: last_of_table };
    let bytes = |d: &DrawnRow| {
        std::mem::size_of::<RowFragment>()
            + d.0.cells.len() * std::mem::size_of::<CellFragment>()
            + (d.0.borders.len() + d.0.background.len()) * std::mem::size_of::<Item>()
    };
    match cached(memos, |c| &mut c.rows, probe, bytes, |_| Ok(DrawnRow(draw_row(tb, ri, left, top, height, None, last_of_table)))) {
        Ok(d) => d.0.clone(),
        Err(_) => draw_row(tb, ri, left, top, height, None, last_of_table),
    }
}

/// The units of a whole table, for stories and cells (no page breaks): one unit per row, drawn relative to its top.
pub fn units(t: &Table, width: f64, env: &StoryEnv, memos: &mut Memos) -> Result<(Vec<StoryUnit>, f64), Yield> {
    Ok(table_units(&*layout(t, width, env, false, memos)?))
}

/// A laid-out table's rows as story units, and their height.
pub fn table_units(tb: &TableLayout) -> (Vec<StoryUnit>, f64) {
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
            content: Rc::from([fragment]),
            block: 0,
            widow: false,
            notes: notes.into_boxed_slice(),
        });
        y += h + extra;
    }
    (units, y)
}

/// Exact rows: slack below the row bottom for a glyph's ink top (1 twip, TABLE-34).
const INK_SLACK: f64 = 0.06;

/// The highest ink top among a unit's glyphs, in its coordinates.
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

/// The source units drawn in `children`, by paragraph: (paragraph, first unit, end unit) per glyph run.
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
