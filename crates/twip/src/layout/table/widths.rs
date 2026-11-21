//! Column widths

use std::rc::Rc;

use crate::docx::Compat;
use crate::docx::model::*;
use crate::docx::props::{LineRule, Width};
use crate::layout::autofit;
use crate::layout::cache::cached;
use crate::layout::job::{Memos, Yield};
use crate::layout::join::LaidBlock;
use crate::layout::line::{break_lines, neighbour_borders};
use crate::layout::space::ParaSpace;
use crate::layout::story::{CellEnds, StoryEnv, StoryKey, child_ids};
use crate::layout::text::{Kind, shape};
use crate::layout::units::Lu;

use super::{
    FitRow, assemble::CellEdges, assemble::cell_edges, assemble::edge_add, assemble::spacing_of, assemble::vertical_cell, borders::space_h, in_view,
};

// Content widths
fn content_widths(blocks: &[Block], env: &StoryEnv, fixed: bool, memos: &mut Memos) -> Result<Rc<(i64, i64, i64)>, Yield> {
    let laid = crate::layout::join::laid(blocks, env.ctx.env.text.review.view);
    let cell = Some(CellEnds { fixed, ..CellEnds::default() });
    let probe = StoryKey::new(child_ids(&laid, None, env, cell, memos)?, None, cell, env, &laid);
    cached(memos, |c| &mut c.cell_widths, probe, |_| std::mem::size_of::<(i64, i64, i64)>(), |m| content_widths_uncached(&laid, env, fixed, m))
}

fn vertical_width(blocks: &[Block], env: &StoryEnv, memos: &mut Memos) -> Result<i64, Yield> {
    let view = env.ctx.env.text.review.view;
    let mut w = 6;
    for b in &crate::layout::join::laid(blocks, view) {
        let LaidBlock::Para(laid) = b else { continue };
        let p = laid.para();
        let space = laid.shape_space_in(env.fields, env.notes, &env.ctx, true, memos)?;
        let shaped = shape(p, &space, env.ctx.fonts, memos)?;
        let s = p.para_style_in(view);
        let ps = ParaSpace {
            width: 1e9,
            start: 0,
            borders: neighbour_borders(s, None, None),
            x0: 0.0,
            env: env.ctx.env.text.clone(),
            fit: false,
            letter_break: false,
            shade_gap: 0.0,
        };
        let pb = break_lines(&shaped, p, &ps, env.ctx.fonts, memos)?;
        w += twips(s.space_before + s.space_after);
        let pieces = pb.pieces();
        let m = (s.line * 240.0).round() as i64;
        for l in &pb.lines {
            let em = pieces[l.start..l.end.min(pieces.len())].iter().map(|q| shaped.styles[q.style as usize].run.size).fold(0.0, f64::max);
            let em = if em > 0.0 { em } else { s.mark_size.unwrap_or(p.mark_in(view).size) };
            let e32 = i64::from(Lu::round_half_up(em / 32.0).0);
            let h = i64::from(Lu::round_half_up(l.pitch).0)
                + match s.line_rule {
                    LineRule::Auto => (e32 * m + 120).div_euclid(240),
                    LineRule::AtLeast => e32,
                    LineRule::Exact => 0,
                };
            w += (h * 1440 + 294_911) / 294_912;
        }
    }
    Ok(w)
}

fn up(x: f64) -> i64 {
    (x * 20.0 - 1e-6).ceil().max(0.0) as i64
}

pub(super) fn twips(x: f64) -> i64 {
    (x * 20.0).round() as i64
}

/// The units of the field begins whose label stands in for an empty result.
fn label_units(inlines: &[crate::docx::Inline]) -> Vec<u32> {
    let starts: Vec<u32> = inlines.iter().scan(0u32, |at, i| Some(std::mem::replace(at, *at + i.units() as u32))).collect();
    crate::docx::fields::spans(inlines)
        .into_iter()
        .filter(|s| {
            matches!(&inlines[s.begin], crate::docx::Inline::FieldBegin { kind: None, view: crate::docx::FieldView::Result, label: Some(_), .. })
                && s.sep.is_none_or(|sep| sep + 1 == s.end)
        })
        .map(|s| starts[s.begin])
        .collect()
}

fn content_widths_uncached(blocks: &[LaidBlock], env: &StoryEnv, fixed: bool, memos: &mut Memos) -> Result<(i64, i64, i64), Yield> {
    let (mut mn, mut mx, mut hard) = (0i64, 0i64, 0i64);
    let old = env.ctx.env.text.compat == Compat::Legacy;
    for (bi, b) in blocks.iter().enumerate() {
        match b {
            LaidBlock::Para(laid) => {
                let p = laid.para();
                let space = laid.shape_space_in(env.fields, env.notes, &env.ctx, true, memos)?;
                let shaped = shape(p, &space, env.ctx.fonts, memos)?;
                let s = p.para_style_in(env.ctx.env.text.review.view);
                let ps = ParaSpace {
                    width: 1e9,
                    start: 0,
                    borders: neighbour_borders(s, None, None),
                    x0: 0.0,
                    env: env.ctx.env.text.clone(),
                    fit: false,
                    letter_break: false,
                    shade_gap: 0.0,
                };
                let pb = break_lines(&shaped, p, &ps, env.ctx.fonts, memos)?;
                let ind = if old { s.ind_left + s.ind_right + s.ind_first.min(0.0) } else { s.ind_left + s.ind_right };
                let pieces = pb.pieces();
                let text_of = |q: &crate::layout::text::Piece| shaped.text.get(q.text.0 as usize..q.text.1 as usize).unwrap_or("");
                let mut tail = vec![false; pieces.len() + 1];
                tail[pieces.len()] = true;
                for i in (0..pieces.len()).rev() {
                    let t = text_of(&pieces[i]);
                    tail[i] = tail[i + 1] && (pieces[i].kind == Kind::Space || (!t.is_empty() && t.chars().all(|c| c == '-')));
                }
                let is_num = |q: &crate::layout::text::Piece| q.src.is_none() && matches!(q.kind, Kind::ListNumber | Kind::Tab | Kind::Space);
                let numbered = old && pieces.iter().any(|q| q.kind == Kind::ListNumber);
                let start0 = if numbered { (s.ind_left + s.ind_first).max(0.0) } else { s.ind_left + s.ind_first };
                let relaid;
                let lines: &[crate::layout::line::Line] = if numbered && s.ind_left + s.ind_first < 0.0 {
                    let mut st = ParaStyle::clone(s);
                    st.ind_first = -s.ind_left;
                    let mark = p.mark_in(env.ctx.env.text.review.view);
                    relaid =
                        crate::layout::line::break_paragraph(&shaped, pieces, &st, mark, &crate::layout::line::Frame::new(&ps), env.ctx.fonts, false);
                    &relaid
                } else {
                    &pb.lines
                };
                let mut laid: Vec<f64> = pieces.iter().map(|q| q.width).collect();
                let mut ends = vec![0.0f64; pieces.len()];
                for l in lines {
                    for (k, (&x, &w)) in l.xs.iter().zip(&l.widths).enumerate() {
                        if let Some(v) = laid.get_mut(l.start + k) {
                            *v = w;
                            ends[l.start + k] = x + w;
                        }
                    }
                }
                let labels = label_units(&p.inlines);
                let mut word = 0.0f64;
                let mut first = numbered.then_some(start0);
                for (i, pc) in pieces.iter().enumerate() {
                    let sep = matches!(pc.kind, Kind::LineBreak | Kind::PageBreak | Kind::ColumnBreak | Kind::Space | Kind::Tab);
                    if let Some(end) = first {
                        if sep && !is_num(pc) {
                            mn = mn.max(up(end + ind));
                            first = None;
                        } else if pc.kind != Kind::Space {
                            first = Some(ends[i]);
                        }
                        continue;
                    }
                    if is_num(pc) {
                        word = ends[i] - s.ind_left;
                        continue;
                    }
                    match pc.kind {
                        Kind::Tab if !numbered => {
                            mn = mn.max(up(word + ind));
                            word = laid[i];
                        }
                        Kind::LineBreak | Kind::PageBreak | Kind::ColumnBreak | Kind::Space | Kind::Tab => {
                            mn = mn.max(up(word + ind));
                            word = 0.0;
                        }
                        _ => {
                            word += pc.width;
                            let joined = old && text_of(pc).starts_with('-') && tail[i + 1];
                            // A design-mode label isn't in the document: it wraps in the cell rather than widen it.
                            let label = pc.src.is_some_and(|u| labels.contains(&u.0));
                            if (pc.break_after || label) && !joined {
                                mn = mn.max(up(word + ind));
                                word = 0.0;
                            }
                        }
                    }
                }
                // A paragraph without pieces (an empty one) has no min: its indents alone don't widen the cell.
                if !pieces.is_empty() {
                    mn = mn.max(up(first.unwrap_or(word) + ind));
                }
                // A wrapping float laid out in the cell is as wide a piece as its box (turned when it turns) and its
                // effect extents, each in whole twips. Only the min: the hard min and the indents leave it out.
                for i in &p.inlines {
                    if let Inline::Float(f, _) = i
                        && f.layout_in_cell
                        && f.wrap != "none"
                        && !(f.vml || f.hidden() || f.in_note || f.is_frame())
                    {
                        let box_w = (f.w * 20.0 + 1e-6).trunc() as i64;
                        mn = mn.max(box_w + twips(f.effect[0]) + twips(f.effect[2]));
                    }
                }
                // The cell's last paragraph, when empty, doesn't widen it by its indents.
                let lines = if pieces.is_empty() && bi + 1 == blocks.len() { &[][..] } else { lines };
                for (li, l) in lines.iter().enumerate() {
                    let start_x = if li == 0 { start0 } else { s.ind_left };
                    let shift = l.xs.first().map_or(0.0, |&x| (x - start_x).max(0.0));
                    let end = (0..l.xs.len())
                        .rev()
                        .find(|&k| !matches!(pieces.get(l.start + k).map(|q| q.kind), Some(Kind::Space)))
                        .map_or(start_x, |k| l.xs[k] + l.widths[k] - shift);
                    let from = if numbered && li == 0 { 0.0 } else { start_x };
                    let mut w = if old { (end - from).max(0.0) + ind } else { end.max(0.0) + s.ind_right };
                    if !old && li + 1 == lines.len() && start_x < 0.0 {
                        let full = l.xs.last().zip(l.widths.last()).map_or(start_x, |(x, w)| x + w - shift);
                        let mark = p.mark_in(env.ctx.env.text.review.view);
                        let glyph = if bi + 1 == blocks.len() { '\u{a4}' } else { '\u{b6}' };
                        let mark_w = env
                            .ctx
                            .fonts
                            .face(&mark.ascii, None, mark.bold, mark.italic)
                            .and_then(|f| Some(f.advance(f.glyph(glyph)?) * f.scale(mark.size)));
                        w = w.max(full + mark_w.unwrap_or(0.0) + s.ind_right);
                    }
                    mx = mx.max(up(w));
                }
            }
            &LaidBlock::Table(t) => {
                let t = crate::layout::story::as_nested(t, Some(CellEnds { fixed, ..CellEnds::default() }));
                let shown = in_view(&t, env.ctx.env.text.review.view, env.ctx.expansions);
                let t = shown.as_ref().map_or(&*t, |v| &v.table);
                let f = fit(t, 0.0, env, Some(autofit::MAXW), memos)?;
                let (al, ar) = edge_add(Compat::Generic, t);
                let e = twips(al) + twips(ar) - twips(t.ind);
                // A nested table with a dxa width is as wide at least as it resolves.
                let floor = if matches!(t.width, Width::Dxa(_)) { f.width } else { f.min_t };
                mn = mn.max(floor - e);
                mx = mx.max(f.width - e);
                hard = hard.max(f.hard_t - e);
            }
        }
    }
    Ok((mn, mx.max(mn), hard))
}

// Column widths
pub(super) struct Fit {
    pub(super) cols: Vec<f64>,
    pub(super) min_t: i64,
    pub(super) width: i64,
    pub(super) hard_t: i64,
}

fn fit(t: &Table, text_w: f64, env: &StoryEnv, avail: Option<i64>, memos: &mut Memos) -> Result<Fit, Yield> {
    let edges = cell_edges(t, grid_columns(t), env.ctx.env.text.single_cell_borders, &|_| None);
    let inputs = fit_inputs(t, env, memos, &edges, &|_| None)?;
    Ok(resolve_fit(t, text_w, env, avail, false, &inputs))
}

pub(super) fn grid_columns(t: &Table) -> usize {
    t.grid.len().max(t.rows.iter().map(|r| r.grid_before + r.cells.iter().map(|c| c.span).sum::<usize>() + r.grid_after).max().unwrap_or(0))
}

/// The grid lines rows use: cell, `gridBefore` and `gridAfter` edges.
pub(super) fn grid_bounds(t: &Table, n: usize) -> Vec<usize> {
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
    (0..=n).filter(|&g| edge[g]).collect()
}

pub(super) fn fit_inputs<'k>(
    t: &Table,
    env: &StoryEnv,
    memos: &mut Memos,
    geo: &[Vec<CellEdges>],
    kept: &dyn Fn(usize) -> Option<&'k FitRow>,
) -> Result<Vec<FitRow>, Yield> {
    let n = grid_columns(t);
    let grid_tw: Vec<i64> = (0..n).map(|i| twips(t.grid.get(i).copied().unwrap_or(0.0))).collect();
    let mut rows = Vec::with_capacity(t.rows.len());
    for (ri, (r, gs)) in t.rows.iter().zip(geo).enumerate() {
        let row_s = spacing_of(t, ri);
        let s2 = twips(2.0 * row_s.unwrap_or(0.0));
        let spacing = twips(row_s.unwrap_or(0.0));
        if let Some(k) = kept(ri).filter(|k| k.cells.len() == gs.len()) {
            rows.push(k.clone());
            continue;
        }
        let mut cells: Vec<autofit::Cell> = Vec::with_capacity(gs.len());
        let mut widths = Vec::new();
        for g in gs {
            let c = &r.cells[g.ci];
            let (bl, br) = (twips(space_h(&g.edges[1])), twips(space_h(&g.edges[3])));
            let (ml, mr) = (twips(c.margins[1]), twips(c.margins[3]));
            let gap = |b: i64, m: i64| if row_s.is_some() { spacing + b + m } else { m };
            let pad = (gap(bl, ml) + gap(br, mr)).max((bl + br + 1) / 2);
            let cell_pad = (pad - 2 * spacing).max(0);
            let continuation = c.v_merge == VMerge::Continue;
            let grid_w: i64 = grid_tw[g.col..g.col + g.span].iter().sum();
            let from_grid = grid_w - s2 - s2 / 2 * (i64::from(g.col == 0) + i64::from(g.col + g.span >= n));
            let (kind, pref) = match c.width {
                Some(Width::Dxa(w)) => (autofit::Kind::Dxa, twips(w)),
                Some(Width::Pct(f)) => (autofit::Kind::Pct, (f * 5000.0).round() as i64),
                Some(Width::Nil) => (autofit::Kind::Dxa, from_grid),
                Some(Width::Auto) | None if t.fixed => (autofit::Kind::Dxa, from_grid),
                Some(Width::Auto) | None => (autofit::Kind::Auto, 0),
            };
            let (min, max, hard) = if t.fixed {
                (cell_pad + 20, cell_pad + 20, cell_pad)
            } else {
                let rotated = vertical_cell(c).is_some();
                let (mut cmn, mut cmx, chard) = if continuation {
                    (0, 0, 0)
                } else if let (true, Some(Width::Dxa(w))) = (c.fit_text, &c.width) {
                    let w = (twips(*w) - cell_pad).max(0);
                    (w, w, w)
                } else if rotated {
                    let w = vertical_width(&c.blocks, env, memos)?;
                    (w, w, 0)
                } else {
                    let w = content_widths(&c.blocks, env, t.fixed, memos)?;
                    widths.push(w.clone());
                    *w
                };
                if (cmn, cmx) == (0, 0) && !continuation {
                    (cmn, cmx) = (6, 6);
                }
                let min = (cmn + cell_pad).min(autofit::MAXW);
                let max = (cmx + cell_pad).max(min).min(autofit::MAXW);
                // A cell that doesn't wrap is as wide as its unwrapped text, or no narrower than its preferred width.
                let min = match (c.no_wrap && !continuation, c.width) {
                    (true, Some(Width::Dxa(_))) => min.max(pref),
                    (true, Some(Width::Auto) | None) => max,
                    _ => min,
                };
                (min, max, (chard + cell_pad).min(autofit::MAXW))
            };
            cells.push(autofit::Cell { first: g.col, span: g.span, min, max, hard, kind, pref, row: ri, cont: continuation });
        }
        rows.push(FitRow { cells, widths });
    }
    Ok(rows)
}

pub(super) fn resolve_fit(t: &Table, text_w: f64, env: &StoryEnv, avail: Option<i64>, nested: bool, inputs: &[FitRow]) -> Fit {
    let n = grid_columns(t);
    // Merged grid
    let bounds = grid_bounds(t, n);
    let m = bounds.len() - 1;
    let at = |g: usize| bounds.partition_point(|&b| b < g.min(n));
    let span_at = |first: usize, span: usize| (at(first), at(first + span) - at(first));
    // Compat ≤ 14 widens the room by the edge allowances only for an outermost table.
    let (add_l, add_r) = edge_add(if nested { Compat::Generic } else { env.ctx.env.text.compat }, t);
    let a = avail.unwrap_or_else(|| twips(text_w - t.ind + add_l + add_r));
    // A pct width is a share of the column with its edge allowances, whatever the indent.
    let pct_base = avail.unwrap_or_else(|| twips(text_w + add_l + add_r));
    let boxed = env.ctx.env.text.box_story && !nested && avail.is_none();
    let (a, pct_base) = if boxed { (a - 1, pct_base + 15) } else { (a, pct_base) };
    let s2 = twips(2.0 * t.cell_spacing.max(0.0));
    let own: Vec<autofit::Cell> = inputs.iter().flat_map(|r| r.cells.iter().copied()).collect();
    let mut gaps = skipped(t, n, &own);
    let mut cells = pct_skips(t, n, at);
    cells.extend(own);
    if m < n {
        for c in &mut cells {
            (c.first, c.span) = span_at(c.first, c.span);
        }
        for (e, k, _) in gaps.iter_mut().flatten() {
            let (first, span) = span_at((*e + 1).saturating_sub(*k), *k);
            (*e, *k) = ((first + span).saturating_sub(1), span);
        }
    }
    let cols = autofit::columns(m, &cells, s2, &gaps, t.fixed);
    let pad_total = s2 * (m as i64 + 1);
    let tbl_w = match t.width {
        Width::Dxa(w) => twips(w),
        Width::Pct(p) => autofit::rdiv(pct_base, (p * 5000.0).round() as i64, 5000),
        _ => 0,
    }
    .min(autofit::MAXW - twips(t.ind));
    let no_grow = !env.ctx.env.text.grow_autofit && tbl_w <= a;
    let r = autofit::resolve(&cols, pad_total, tbl_w, a, t.fixed, no_grow);
    let mut widths = r.widths.clone();
    autofit::with_spacing(&mut widths, s2);
    // Back to the table's grid
    let cols = (0..n).map(|g| bounds.binary_search(&g).map_or(0.0, |j| widths[j] as f64 / 20.0)).collect();
    Fit { cols, min_t: r.min_t, width: r.width, hard_t: r.hard_t }
}

/// The one-column pct skips (`gridBefore` / `gridAfter` 1 with a pct `wBefore` / `wAfter`) as pct cells of their
/// column, each where a cell with a written dxa or pct width reaches it. A pct skip over more columns counts nothing.
/// Skip widths count columns of the merged grid (`at`).
fn pct_skips(t: &Table, n: usize, at: impl Fn(usize) -> usize) -> Vec<autofit::Cell> {
    let written: Vec<(usize, usize)> = t
        .rows
        .iter()
        .flat_map(|r| {
            r.cells.iter().scan(r.grid_before, |x, c| {
                let at = *x;
                *x += c.span;
                Some((c, at))
            })
        })
        .filter(|(c, _)| matches!(c.width, Some(Width::Dxa(_) | Width::Pct(_))))
        .map(|(c, at)| (at, at + c.span))
        .collect();
    let cell = |row: usize, col: usize, span: usize, p: f64| autofit::Cell {
        first: col,
        span,
        min: 0,
        max: 0,
        hard: 0,
        kind: autofit::Kind::Pct,
        pref: (p * 5000.0).round() as i64,
        row,
        cont: false,
    };
    let mut out = Vec::new();
    for (ri, r) in t.rows.iter().enumerate() {
        let end = r.grid_before + r.cells.iter().map(|c| c.span).sum::<usize>();
        if let (1, Some(p)) = (at(r.grid_before), r.w_before.and_then(Width::pct))
            && written.iter().any(|&(a, _)| a < 1)
        {
            out.push(cell(ri, 0, r.grid_before, p));
        }
        if let (1, Some(p)) = (at(end + r.grid_after) - at(end), r.w_after.and_then(Width::pct))
            && end < n
            && written.iter().any(|&(_, b)| b > end)
        {
            out.push(cell(ri, end, r.grid_after, p));
        }
    }
    out
}

fn skipped(t: &Table, n: usize, cells: &[autofit::Cell]) -> Vec<Vec<autofit::Gap>> {
    let w = |w: Width| twips(w.dxa()).max(0);
    let mut xs = vec![0i64];
    for i in 0..n {
        xs.push(xs[i] + twips(t.grid.get(i).copied().unwrap_or(0.0)));
    }
    let x = |k: usize| xs[k.min(n)];
    let fixed: Vec<&autofit::Cell> = cells.iter().filter(|c| c.kind != autofit::Kind::Auto).collect();
    let (Some(lo), Some(hi)) = (fixed.iter().map(|c| x(c.first)).min(), fixed.iter().map(|c| x(c.first + c.span)).max()) else {
        return Vec::new();
    };
    t.rows
        .iter()
        .map(|r| {
            let mut out = Vec::new();
            let (gb, end) = (r.grid_before, r.grid_before + r.cells.iter().map(|c| c.span).sum::<usize>());
            let pct = |w: Option<Width>| w.and_then(Width::pct).is_some();
            if gb > 0 && gb <= n && lo < x(gb) && !pct(r.w_before) {
                out.push((gb - 1, gb, r.w_before.map_or(x(gb) - x(0), w)));
            }
            let after = if r.grid_after > 0 { r.w_after } else { None };
            if end < n && x(end) < hi && !pct(after) {
                out.push((n - 1, n - end, after.map_or(x(n) - x(end), w)));
            }
            out
        })
        .collect()
}
