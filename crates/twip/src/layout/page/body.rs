//! Page body

use std::collections::HashMap;
use std::rc::Rc;

use crate::docx::Compat;
use crate::docx::model::*;
use crate::layout::LayoutCtx;
use crate::layout::flow::{AttemptState, PageFill, PassMemo, Stop};
use crate::layout::fragment::{BoxFragment, BoxKind, Child, Item, child_down};
use crate::layout::ids::{FragmentId, ParaId, TableId};
use crate::layout::job::{Memos, Yield};
use crate::layout::node::{Notes, SectionNode};
use crate::layout::record::PageEffects;
use crate::layout::space::{BodyArea, BodyFields, BodySpace, ColumnSpace};
use crate::layout::story::StoryEnv;
use crate::layout::text::FieldValues;
use crate::layout::token::{FlowState, FlowToken, NoteArea, PageToken};
use crate::layout::units::{EPS, LU_PER_PT, Lu};

use super::glyph_runs;

// Column balancing
#[allow(
    clippy::type_complexity,
    reason = "one column fill's whole result (end, children, token, state, effects); a struct used only here would add nothing"
)]
fn balance(
    fill: &mut PageFill,
    sec: &SectionNode,
    cols: &[ColumnSpace],
    saved: &(FlowToken, FlowState),
    mid: bool,
    stream: Option<&NoteArea>,
    memos: &mut Memos,
) -> Result<Option<(f64, Vec<Child>, FlowToken, FlowState, PageEffects)>, Yield> {
    let (p0, c0) = saved;
    let top = cols[0].top.to_pt();
    let blocks = &sec.blocks;
    let n = blocks.len();
    let skip_last = matches!(blocks.get(n.wrapping_sub(1)), Some(Block::Para(p)) if p.inlines.is_empty() && n > 1);
    let mark_line = skip_last && fill.ctx.env.text.compat == Compat::Generic && matches!(blocks.get(n.wrapping_sub(2)), Some(Block::Table(_)));
    let state = fill.at.clone();
    let view = fill.ctx.env.text.review.view;
    let spacing: HashMap<_, _> = blocks
        .iter()
        .filter_map(|b| if let Block::Para(p) = b { Some(p) } else { None })
        .map(|p| {
            let (text, style) = (&fill.ctx.env.text, p.para_style_in(view));
            (
                crate::layout::text::para_id(p),
                (crate::layout::paragraph::space_before(text, style), crate::layout::paragraph::space_after(text, style)),
            )
        })
        .collect();
    let legacy = fill.ctx.env.text.compat == Compat::Legacy;
    let mut bottoms = HashMap::new();
    if fill.ctx.env.text.compat == Compat::Legacy {
        for bi in (0..n).filter(|&bi| matches!(blocks.get(bi), Some(Block::Table(_)))) {
            let tb = fill.table_layout(bi, cols[0].width, memos)?;
            bottoms.insert(tb.id, tb.bottom_border);
        }
    }
    let trial = Trial { cols, p0, c0, top, state: &state, stream, n, skip_last, mark_line, legacy, mid, spacing: &spacing, bottoms: &bottoms };
    let try_h = |fill: &mut PageFill, h: f64, memos: &mut Memos| trial.run(fill, h, memos);
    let full = cols[0].bottom.to_pt() - top;
    let (first, stats) = try_h(fill, full, memos)?;
    if first.is_none() {
        fill.at = state;
        return Ok(None);
    }
    let full_lu = i64::from(Lu::round_half_up(full).0);
    let widths: Vec<f64> = cols.iter().map(|c| c.width).collect();
    let wsum: f64 = widths.iter().sum::<f64>().max(1.0);
    let r = if fill.ctx.env.text.compat == Compat::Legacy {
        trial.legacy_height(fill, sec, &stats, full_lu, &widths, wsum, memos)?
    } else {
        trial.generic_height(fill, &stats, full_lu, &widths, wsum, memos)?
    };
    let Some((p, mut c, children, end, eff)) = r.0.or(first) else { unreachable!("the full height balanced before") };
    c.prev = None;
    c.balanced_after = c.prev_after;
    c.prev_after = 0.0;
    Ok(Some((end, children, p, c, eff)))
}

// One balancing trial's inputs
struct Trial<'t> {
    cols: &'t [ColumnSpace],
    p0: &'t FlowToken,
    c0: &'t FlowState,
    top: f64,
    state: &'t AttemptState,
    stream: Option<&'t NoteArea>,
    n: usize,
    skip_last: bool,
    mark_line: bool,
    legacy: bool,
    mid: bool,
    spacing: &'t HashMap<ParaId, (f64, f64)>,
    bottoms: &'t HashMap<TableId, f64>,
}

impl Trial<'_> {
    // One trial at a height
    #[allow(
        clippy::type_complexity,
        reason = "one column fill's whole result (end, children, token, state, effects); a struct used only here would add nothing"
    )]
    fn run(
        &self,
        fill: &mut PageFill,
        h: f64,
        memos: &mut Memos,
    ) -> Result<(Option<(FlowToken, FlowState, Vec<Child>, f64, PageEffects)>, Vec<ColStats>), Yield> {
        let &Trial { cols, p0, c0, top, state, stream, n, skip_last, mark_line, legacy, mid, spacing, bottoms } = self;
        let mut pos = p0.clone();
        let mut carry = c0.clone();
        let mut children = Vec::new();
        let mut stats = Vec::new();
        let mut end = top;
        let mut lines_end = top;
        let stream = stream.map(|a| NoteArea { below: true, ..a.clone() });
        fill.at = AttemptState { known: state.known.clone(), stream, beneath: state.beneath, ..Default::default() };
        for (k, c) in cols.iter().enumerate() {
            let col = ColumnSpace { bottom: Lu::round_half_up(top + h).min(c.bottom), balance: true, ..*c };
            fill.column = k;
            let r = fill.fill(&pos, &carry, col, false, memos)?;
            let s = r.stop;
            (pos, carry) = (r.token, r.state);
            // A column's height takes its last paragraph's space after, unless the paragraph goes on in the next
            // column (the last column's bottom has it already).
            let hang = |pos: &FlowToken, carry: &FlowState| if s == Stop::Done || !pos.at.at_start() { 0.0 } else { carry.prev_after };
            let tail = if s == Stop::Done || pos.at.at_start() { carry.prev_after } else { 0.0 };
            stats.push(ColStats::of(&r.children, top, r.bottom - top + hang(&pos, &carry), tail, spacing, bottoms));
            // The column's last line bottom: a finished fill's bottom holds the last space after.
            lines_end = lines_end.max(r.bottom - if s == Stop::Done { carry.prev_after } else { 0.0 });
            let mut kids = r.children;
            let mut bottom = r.bottom;
            let at_mark = skip_last && pos.block + 1 == n && pos.at.at_start();
            if at_mark && mark_line && !fill.at.forced {
                let m = fill.fill(&pos, &carry, ColumnSpace { top: Lu::round_half_up(r.bottom), bottom: Lu::MAX, ..col }, false, memos)?;
                (pos, carry, bottom) = (m.token, m.state, m.bottom);
                kids.extend(m.children);
            }

            children.push(Child::Box(BoxFragment { kind: BoxKind::Column, id: None, offset: (0.0, 0.0), size: (0.0, 0.0), children: kids.into() }));
            end = end.max(bottom + hang(&pos, &carry));
            if fill.at.forced {
                // Compat 15: a column whose first line doesn't fit stays empty, and so do the columns after it.
                if fill.ctx.env.text.compat == Compat::Generic {
                    stats.pop();
                }
                return Ok((None, stats));
            }
            if s == Stop::Done || at_mark {
                // Compat ≤ 14: the section ends at the height tried or at its tallest column, whichever is less,
                // but never above a column's last line (with the room multiple spacing adds below it).
                if legacy {
                    end = end.min(top + h).max(lines_end);
                }
                return Ok((Some((pos, carry, children, end.max(fill.at.notes_bottom), fill.at.effects.clone())), stats));
            }
            if k + 1 < cols.len() {
                carry.keep_top = s == Stop::Column && carry.keep_top || s == Stop::Full && mid;
            }
        }
        Ok((None, stats))
    }

    // Compat ≤ 14 search
    #[allow(clippy::type_complexity, reason = "as run")]
    #[allow(clippy::too_many_arguments, reason = "the first fill's statistics and the columns' widths")]
    fn legacy_height(
        &self,
        fill: &mut PageFill,
        sec: &SectionNode,
        stats: &[ColStats],
        full_lu: i64,
        widths: &[f64],
        wsum: f64,
        memos: &mut Memos,
    ) -> Result<(Option<(FlowToken, FlowState, Vec<Child>, f64, PageEffects)>, Vec<ColStats>), Yield> {
        let cols = self.cols;
        let try_h = |fill: &mut PageFill, h: f64, memos: &mut Memos| self.run(fill, h, memos);
        let to_pt = |h: i64| h as f64 / LU_PER_PT;
        let mut h;
        let mut r;
        // Compat ≤ 14: the sums count every table row with the table's bottom border (the fill keeps the layout's
        // heights). Equal columns start at the unbalanced fill's total over the column count, rounded up; explicit
        // widths at its layout height weighted by the first column's width, in whole twips. Never below the mean
        // line height, the height grows while content is left over by the failed fill's mean line height, each
        // column's height counted at most up to the height tried.
        let (lines, total) = stats.iter().fold((0, 0), |(n, t), s| (n + s.lines, t + s.used + s.extra));
        let columns = &sec.section.section_in(fill.ctx.env.text.review.view).columns;
        h = if columns.equal {
            (total + cols.len() as i64 - 1) / cols.len().max(1) as i64
        } else {
            let lu_tw = LU_PER_PT / 20.0;
            let layout: i64 = stats.iter().map(|s| s.used).sum();
            (((layout as f64 / lu_tw).trunc() * widths[0] / wsum).trunc() * lu_tw) as i64
        };
        h = h.max(total / lines.max(1)).min(full_lu);
        r = try_h(fill, to_pt(h), memos)?;
        while r.0.is_none() && h < full_lu {
            let (lines, used) = r.1.iter().fold((0, 0), |(n, t), s| (n + s.lines, t + (s.used + s.extra).min(h)));
            h = (h + if lines > 0 { (used / lines).max(1) } else { 49152 }).min(full_lu);
            r = try_h(fill, to_pt(h), memos)?;
        }
        Ok(r)
    }

    // Compat 15 search
    #[allow(clippy::type_complexity, reason = "as run")]
    fn generic_height(
        &self,
        fill: &mut PageFill,
        stats: &[ColStats],
        full_lu: i64,
        widths: &[f64],
        wsum: f64,
        memos: &mut Memos,
    ) -> Result<(Option<(FlowToken, FlowState, Vec<Child>, f64, PageEffects)>, Vec<ColStats>), Yield> {
        let try_h = |fill: &mut PageFill, h: f64, memos: &mut Memos| self.run(fill, h, memos);
        let to_pt = |h: i64| h as f64 / LU_PER_PT;
        let mut h;
        let mut r;
        // From the unbalanced fill's paragraph heights weighted by the columns' widths, rounded down to its
        // smallest paragraph step, the height grows by the failed fill's smallest step, or shrinks by the fitting
        // fill's while it still fits; a grow or shrink step is never below 240 tw, and is 240 tw after a fill
        // that placed nothing.
        const FLOOR: i64 = 49152;
        let smallest = |st: &[ColStats]| st.iter().filter(|s| s.min > 0).map(|s| s.min).min();
        let m = smallest(stats).unwrap_or(FLOOR);
        let step = |st: &[ColStats]| smallest(st).unwrap_or(FLOOR).max(FLOOR);
        h = (widths.iter().zip(stats).map(|(w, s)| w * s.height as f64).sum::<f64>() / wsum) as i64;
        h = (h - h % m).min(full_lu);
        r = try_h(fill, to_pt(h), memos)?;
        if r.0.is_some() {
            loop {
                let m = step(&r.1);
                if h - m <= 0 {
                    break;
                }
                let r2 = try_h(fill, to_pt(h - m), memos)?;
                if r2.0.is_none() {
                    break;
                }
                h -= m;
                r = r2;
            }
        } else {
            while r.0.is_none() && h < full_lu {
                h = (h + step(&r.1)).min(full_lu);
                r = try_h(fill, to_pt(h), memos)?;
            }
        }
        Ok(r)
    }
}

fn draw_floats(fill: &PageFill, placed: &[crate::layout::float::Placed], memos: &mut Memos) -> Result<Vec<Child>, Yield> {
    let fields = FieldValues::default();
    let env = StoryEnv { ctx: fill.ctx, fields: &fields, notes: fill.note_numbers };
    placed
        .iter()
        .filter(|p| !matches!(p.key.2, crate::layout::float::TABLE_PIECE | crate::layout::float::UPRIGHT_TABLE))
        .map(|p| crate::layout::story::float_box(&p.float, p.rect, p.anchor, &env, memos))
        .collect()
}

pub struct BodyEnv<'d, 'p> {
    pub ctx: LayoutCtx<'d>,
    pub notes: Notes<'d>,
    pub note_numbers: &'p HashMap<(NoteKind, i64), String>,
    pub pass: &'p mut PassMemo,
}

pub struct PageBody {
    pub fragment: Rc<BoxFragment>,
    pub end: PageToken,
    pub section_end: Option<f64>,
    pub effects: PageEffects,
}

pub fn layout_body(
    sec: &SectionNode,
    space: &BodySpace,
    fields: &BodyFields,
    token: &PageToken,
    earlier: &[Rc<BoxFragment>],
    env: BodyEnv,
    memos: &mut Memos,
) -> Result<PageBody, Yield> {
    let s = sec.section;
    let nblocks = sec.blocks.len();
    let cols = &space.columns;
    let page_top = cols[0].top.to_pt();
    let mut pos = token.flow.clone();
    let mut carry = token.state.clone();
    let mut bottoms = Vec::new();
    let mut stop = Stop::Full;
    let mut known = Vec::new();
    let generic = env.ctx.env.text.compat == Compat::Generic;
    let geometry = note_columns(if generic { s.footnote_columns } else { 0 }, &space.area, cols);
    let shared = generic && (cols.len() > 1 || geometry.len() > 1);
    // Compat 15: the page's footnote area, started by an earlier section on the page, takes this section's notes too.
    let held = if shared { carry.page_notes.take() } else { None };
    let beneath = held.as_ref().map_or(s.notes[0].beneath, |h| h.1);
    let stream_area = |reserve: f64| match &held {
        Some((a, _)) => NoteArea { shared: Some(reserve), ..a.clone() },
        None => NoteArea { shared: Some(reserve), cols: geometry.clone(), ..Default::default() },
    };
    let mut reserve = held.as_ref().map_or(0.0, |h| h.0.height);
    let grid_env;
    let vertical = crate::layout::vertical::of_section(s.text_flow);
    let ctx = match (s.line_grid, vertical) {
        (None, None) => env.ctx,
        (g, vertical) => {
            let text = crate::layout::env::TextEnv { line_grid: g.or(env.ctx.env.text.line_grid), vertical, ..env.ctx.env.text.clone() };
            grid_env = crate::layout::LayoutEnv { text, ..env.ctx.env.clone() };
            LayoutCtx { env: &grid_env, ..env.ctx }
        }
    };
    let mut fill = PageFill {
        ctx,
        sec,
        notes: env.notes,
        fields,
        note_numbers: env.note_numbers,
        page_w: space.page_w,
        page_h: space.page_h,
        margins: space.margins,
        pass: env.pass,
        at: AttemptState::default(),
        fixed: space.hf_floats.clone(),
        odd: space.odd,
        mirror: space.mirror,
        hold_notes: space.hold_notes,
        mid_page: space.mid_page,
        notes_closed: space.notes_closed,
        column: 0,
    };
    let mut col_children: Vec<Vec<Child>> = Vec::new();
    let mut end: Option<FlowToken> = None;
    let saved = (pos.clone(), carry.clone());
    let mut drops = 0;
    let mut attempts = 6;
    let mut attempt = 0;
    while attempt < attempts {
        attempt += 1;
        let r: Result<bool, Yield> = crate::prof::time("layout:attempt", || {
            crate::layout::job::unit("page attempt", || {
                fill.at = AttemptState {
                    known: known.clone(),
                    end: end.clone(),
                    stream: shared.then(|| stream_area(reserve)),
                    beneath: held.as_ref().map(|h| h.1),
                    ..Default::default()
                };
                pos = saved.0.clone();
                carry = saved.1.clone();
                col_children.clear();
                bottoms.clear();
                fill_columns(&mut fill, cols, space.mid_page, &mut pos, &mut carry, &mut col_children, &mut bottoms, &mut stop, memos)?;
                let found = fill.at.found.clone();
                let notes = fill.at.stream.as_ref().map_or(0.0, |a| a.height);
                // A float that met earlier text only in this fill is seen by that text in the next.
                // Compat ≤ 14: a drawing after a frame placed above its flow position that meets earlier text and
                // didn't stay in the fill before starts the page again, until it stays and its frames are placed
                // against each other.
                let frame = |f: &crate::layout::float::Placed| {
                    f.float.text_box.is_some() && f.float.shape.is_none() || f.key.2 >= crate::layout::float::UPRIGHT_TABLE
                };
                let again = found.iter().any(|f| {
                    f.restarts
                        && f.wrap != crate::layout::float::Wrap::None
                        && !frame(f)
                        && !known.iter().any(|k| k.same(f) && k.frozen)
                        && found.iter().any(|r| r.rounded_up && r.key.0 == f.key.0 && (r.key.1, r.key.2) < (f.key.1, f.key.2))
                });
                if again {
                    attempts = attempts.max(attempt + 2).min(64);
                }
                let stable =
                    !again && found.len() == known.len() && found.iter().all(|f| known.iter().any(|k| k.same(f) && k.collided >= f.collided));
                attempts = attempts.max(2 + found.len() + drops).min(64);
                if (stable && (notes - reserve).abs() < EPS) || attempt >= attempts {
                    return Ok(true);
                }
                // A float gone from the page takes its anchor paragraph with it: the page ends before that paragraph
                // when it starts on the page, else where this fill ended.
                if end.is_none()
                    && let Some(k) = known.iter().find(|k| !found.iter().any(|f| f.key == k.key))
                {
                    let starts_here = k.key.0 == sec.index && (k.key.1 > saved.0.block || k.key.1 == saved.0.block && saved.0.at.at_start());
                    end = Some(if starts_here { FlowToken { block: k.key.1, at: crate::layout::token::At::Start } } else { pos.clone() });
                    drops += 1;
                }
                known = found.into_iter().filter(|f| f.settled).collect();
                reserve = notes;
                Ok(false)
            })
        });
        if r? {
            break;
        }
    }
    let placed = fill.at.found.clone();
    let stream = fill.at.stream.take();
    let mut body: Vec<Child> = col_children
        .drain(..)
        .map(|c| Child::Box(BoxFragment { kind: BoxKind::Column, id: None, offset: (0.0, 0.0), size: (0.0, 0.0), children: c.into() }))
        .collect();
    let next_shares = stop == Stop::Done && space.next_continuous;
    // The area passes to the next section on the page, drawn after the page's last text.
    // Compat 15: the first section on the page with a footnote reference (on this page or an earlier one) gives the
    // page's footnote area its columns, notes on this page or not.
    let starts_area = |a: &NoteArea| !a.parts.is_empty() || held.is_some() || sec.blocks.iter().any(crate::layout::node::has_footnote_refs);
    let (stream, passed) = match stream {
        Some(a) if next_shares && space.hold_notes && starts_area(&a) => {
            // Across columns a later section's notes keep flowing through the columns; in one column they stack.
            let shared = (a.cols.len() > 1).then_some(0.0);
            (None, Some((NoteArea { shared, ..a }, beneath)))
        }
        a => (a, None),
    };
    if let Some(a) = &stream {
        let text = bottoms.iter().copied().fold(page_top, f64::max) + if pos.at.at_start() { carry.prev_after } else { 0.0 };
        fill.draw_notes(a, &cols[0], text, &mut body, memos)?;
    }
    let floats = crate::prof::time("layout:floats", || draw_floats(&fill, &placed, memos))?;
    if !space.mid_page && !next_shares && cols.len() == 1 {
        let done = pos.at.at_start();
        let bottom = bottoms.first().copied().unwrap_or(page_top) + if done { carry.prev_after } else { 0.0 };
        body.extend(floats.iter().cloned());
        if !fill.at.notes_drawn && !fill.at.starts.is_empty() {
            body = v_align(&s.v_align, &fill.at.starts, &space.area, bottom, body);
        }
    } else {
        body.extend(floats.iter().cloned());
    }
    let mut effects = fill.at.effects.clone();
    let mut section_end = None;
    if stop == Stop::Done {
        let mut e = bottoms.iter().copied().fold(page_top, f64::max) + carry.prev_after;
        // Compat ≤ 14: after a page-bottom footnote the next continuous break starts a new page, so the columns end
        // unbalanced as before a page break.
        // ponytail: with `footnoteLayoutLikeWW8` the break stays continuous and the columns balance as before any
        // continuous break; a balanced section above page-bottom notes wasn't measured.
        let breaks = env.ctx.env.text.compat == Compat::Legacy
            && !env.ctx.env.notes_like_ww8
            && space.next_continuous
            && !s.notes[0].beneath
            && effects.notes_shown.iter().any(|n| n.0 == NoteKind::Foot);
        if space.balance && cols.len() > 1 && !breaks {
            let template = shared.then(|| stream_area(reserve));
            match balance(&mut fill, sec, cols, &saved, space.mid_page, template.as_ref(), memos)? {
                Some((h, children, p, mut c, eff)) => {
                    e = h;
                    pos = p;
                    // The trials leave the page's shared area out: what goes on of its notes comes from the fill.
                    if template.is_some() {
                        c.note_rest = std::mem::take(&mut carry.note_rest);
                    }
                    carry = c;
                    effects = eff;
                    body = children;
                }
                None => e = cols[0].bottom.to_pt(),
            }
            body.extend(floats.iter().cloned());
            if let Some(a) = &stream {
                fill.draw_notes(a, &cols[0], e, &mut body, memos)?;
            }
        }
        section_end = Some(e.max(fill.at.notes_bottom));
        carry.page_notes = carry.page_notes.take().or(passed);
    } else if pos.block >= nblocks && (carry.note_rest.is_empty() && carry.table_rest.is_none() || stop != Stop::Full) {
        section_end = Some(bottoms.iter().copied().fold(page_top, f64::max));
    }
    inline_text_boxes(&fill, &mut body, memos)?;
    let shown = crate::layout::expansion::shown(&body);
    body.extend(shown);
    separators(&s.section_in(env.ctx.env.text.review.view).columns, cols, page_top, earlier, &mut body);
    if env.ctx.env.text.review.change_bars() {
        change_bars(space, &env.ctx.env.text.review, env.ctx.env.mirror_margins, &mut body);
    }
    if section_end.is_none() {
        if s.line_numbers.as_ref().is_some_and(|l| l.restart == "newPage") {
            carry.line_no = 0;
        }
        if stop == Stop::Full {
            // A continuous section whose first paragraph found no room on the page it starts keeps that paragraph's
            // space before on the next page, as after a next-page break.
            carry.keep_top = space.mid_page && pos.block == 0 && pos.at.at_start();
        }
    }
    let (size, children) = match vertical {
        Some(v) => {
            let mut turned = crate::layout::vertical::turn_body(&body, v, [space.page_h, space.page_w]);
            turned.append(&mut fill.at.upright);
            ((space.page_h, space.page_w), turned.into())
        }
        None if s.text_flow == 4 => ((space.page_w, space.page_h), crate::layout::vertical::turn_east_asian(&body).into()),
        None => ((space.page_w, space.page_h), body.into()),
    };
    effects.floats = placed.iter().filter(|f| f.wrap != crate::layout::float::Wrap::None).cloned().collect();
    let fragment = Rc::new(BoxFragment { kind: BoxKind::Body, id: Some(FragmentId::next()), offset: (0.0, 0.0), size, children });
    Ok(PageBody { fragment, end: PageToken { section: space.section, flow: pos, state: carry }, section_end, effects })
}

// One attempt's columns
#[allow(clippy::too_many_arguments, reason = "the attempt's flow position, carried state and the columns' results")]
fn fill_columns(
    fill: &mut PageFill,
    cols: &[ColumnSpace],
    mid_page: bool,
    pos: &mut FlowToken,
    carry: &mut FlowState,
    col_children: &mut Vec<Vec<Child>>,
    bottoms: &mut Vec<f64>,
    stop: &mut Stop,
    memos: &mut Memos,
) -> Result<(), Yield> {
    for (k, col) in cols.iter().enumerate() {
        let page_empty = k == 0 && !mid_page;
        fill.column = k;
        let r = fill.fill(pos, carry, *col, page_empty, memos)?;
        let st = r.stop;
        (*pos, *carry) = (r.token, r.state);
        col_children.push(r.children);
        bottoms.push(r.bottom);
        *stop = st;
        if matches!(st, Stop::Done | Stop::Page) {
            break;
        }
        if k + 1 < cols.len() {
            carry.keep_top = st == Stop::Column && carry.keep_top || st == Stop::Full && mid_page;
        }
    }
    Ok(())
}

// Inline text boxes
fn inline_text_boxes(fill: &PageFill, body: &mut Vec<Child>, memos: &mut Memos) -> Result<(), Yield> {
    let mut texts = Vec::new();
    crate::layout::traverse::lines_in(body, &mut |para, l, dx, dy| {
        for (r, pic) in l.line.texts.iter() {
            let (ox, oy) = (dx + l.x, dy + l.y);
            texts.push((para.unwrap_or(crate::layout::ids::ParaId(0)), [r[0] + ox, r[1] + oy, r[2] + ox, r[3] + oy], pic.clone()));
        }
    });
    if !texts.is_empty() {
        let fields = crate::layout::text::FieldValues::default();
        let senv = crate::layout::story::StoryEnv { ctx: fill.ctx, fields: &fields, notes: fill.note_numbers };
        for (para, rect, pic) in texts {
            body.extend(crate::layout::story::inline_text_box(&pic, rect, para, &senv, memos)?);
        }
    }
    Ok(())
}

/// The footnote area's columns (x, width): with `w15:footnoteColumns` (`n` > 0) n even columns 36pt apart across the
/// text width, else the section's.
fn note_columns(n: u32, area: &BodyArea, cols: &[ColumnSpace]) -> Vec<(f64, f64)> {
    if n == 0 {
        return cols.iter().map(|c| (c.x, c.width)).collect();
    }
    let w = (area.width - f64::from(n - 1) * 36.0) / f64::from(n);
    (0..n).map(|k| (area.left + f64::from(k) * (w + 36.0), w)).collect()
}

// Vertical alignment
fn v_align(va: &str, starts: &[(f64, bool)], geo: &BodyArea, bottom: f64, body: Vec<Child>) -> Vec<Child> {
    if va == "top" || starts.is_empty() {
        return body;
    }
    let top = starts[0].0;
    let (geo_top, geo_bottom) = (geo.top.to_pt(), geo.bottom.to_pt());
    let free = geo_bottom - geo_top - (bottom - top);
    match va {
        "center" => {
            let dy = geo_top + free / 2.0 - top;
            body.iter().map(|c| child_down(c, &|_| dy)).collect()
        }
        "bottom" => {
            let dy = geo_bottom - bottom;
            body.iter().map(|c| child_down(c, &|_| dy)).collect()
        }
        "both" => {
            let gaps: Vec<f64> = starts[1..].iter().filter(|s| s.1).map(|s| s.0).collect();
            if gaps.is_empty() {
                return body;
            }
            let each = (geo_bottom - bottom) / gaps.len() as f64;
            let shift = |y: f64| each * gaps.iter().filter(|&&g| g < y).count() as f64;
            body.iter().map(|c| child_down(c, &shift)).collect()
        }
        _ => body,
    }
}

// Change bars
fn change_bars(space: &BodySpace, review: &crate::docx::review::ReviewView, mirror_margins: bool, body: &mut Vec<Child>) {
    use crate::docx::review::BarSide;
    fn lines(children: &[Child], dy: f64, out: &mut dyn FnMut(f64, f64, bool)) {
        for c in children {
            match c {
                Child::Box(b) => lines(&b.children, dy + b.offset.1, out),
                Child::Group { offset, children } => lines(children, dy + offset.1, out),
                Child::Para(p) => p.lines.iter().for_each(|l| out(dy + l.y, dy + l.y + l.line.pitch, l.line.changed)),
                Child::Line { placed: l, .. } => out(dy + l.y, dy + l.y + l.line.pitch, l.line.changed),
                Child::Table(t) => {
                    for r in t.rows.iter() {
                        let mut row = |top: f64, bottom: f64, c: bool| out(top, bottom, c || r.changed);
                        r.cells.iter().for_each(|cell| lines(&cell.content, dy, &mut row));
                    }
                }
                Child::Items(_) | Child::Turned { .. } => {}
            }
        }
    }
    let mut changed = false;
    lines(body, 0.0, &mut |_, _, c| changed |= c);
    if !changed {
        return;
    }
    let mut all = Vec::new();
    lines(body, 0.0, &mut |top, bottom, c| all.push((top, bottom, c)));
    all.sort_by(|a, b| a.0.total_cmp(&b.0));
    let right = match review.markup.style.change_bars {
        BarSide::Right => true,
        BarSide::Outside => mirror_margins && space.odd,
        BarSide::Left | BarSide::None => false,
    };
    let x = if right {
        space.area.left + space.area.width + (space.margins.1 / 2.0).min(36.0) - 0.72
    } else {
        space.area.left - (space.margins.0 / 2.0).min(36.0)
    };
    let colour = review.change_bar_color();
    let mut bars: Vec<Item> = Vec::new();
    for (k, &(top, bottom, c)) in all.iter().enumerate() {
        if !c {
            continue;
        }
        let next = all[k + 1..].iter().map(|l| l.0).find(|&t| t >= bottom - 0.01);
        let bottom = next.filter(|&t| t - bottom <= bottom - top).unwrap_or(bottom);
        match bars.last_mut() {
            Some(Item::Rect { rect, .. }) if top <= rect[3] + 0.01 => rect[3] = rect[3].max(bottom),
            _ => bars.push(Item::rect([x, top, x + 0.72, bottom], colour)),
        }
    }
    body.push(Child::Items(bars.into()));
}

// Column separators
fn separators(c: &Columns, cols: &[ColumnSpace], top: f64, earlier: &[Rc<BoxFragment>], body: &mut Vec<Child>) {
    if !c.separator || cols.len() < 2 {
        return;
    }
    let mut runs = Vec::new();
    for b in earlier {
        glyph_runs(&b.children, b.offset.0, b.offset.1, &mut runs);
    }
    glyph_runs(body, 0.0, 0.0, &mut runs);
    let mut bottom = top;
    let mut used = vec![false; cols.len()];
    for &(x, y, size) in &runs {
        if y < top - EPS {
            continue;
        }
        if let Some(k) = cols.iter().position(|c| x >= c.x - 1.0 && x <= c.x + c.width + 1.0) {
            used[k] = true;
            bottom = bottom.max(y + size * 0.25);
        }
    }
    let mut items = Vec::new();
    for k in 1..cols.len() {
        if used[k] {
            let x = (cols[k - 1].x + cols[k - 1].width + cols[k].x) / 2.0;
            items.push(Item::rect([x - 0.36, top, x + 0.36, bottom], 0));
        }
    }
    if !items.is_empty() {
        body.push(Child::Items(items.into()));
    }
}

struct ColStats {
    lines: i64,
    /// The sum of the column's paragraph piece heights, each with the spacing it holds.
    height: i64,
    /// The smallest paragraph piece step: its smallest line with the spacing attached to it (0 with none).
    min: i64,
    used: i64,
    /// What compat ≤ 14 balancing counts beyond `used`: every table row's bottom border, not only the part's last.
    extra: i64,
}

impl ColStats {
    /// `tail` is the space after of the column's last piece as laid out (0 when its paragraph goes on), `spacing` each
    /// paragraph's own space before and after.
    fn of(
        children: &[Child],
        top: f64,
        used: f64,
        tail: f64,
        spacing: &HashMap<crate::layout::ids::ParaId, (f64, f64)>,
        bottoms: &HashMap<crate::layout::ids::TableId, f64>,
    ) -> ColStats {
        let lu = |h: f64| i64::from(Lu::round_half_up(h).0);
        let mut st = ColStats { lines: 0, height: 0, min: i64::MAX, used: lu(used), extra: 0 };
        // The column's pieces in order: a paragraph's lines (top, pitch) with its id, or one table row.
        let mut pieces: Vec<(Option<crate::layout::ids::ParaId>, Vec<(f64, f64)>)> = Vec::new();
        for c in children {
            match c {
                Child::Para(p) => pieces.push((Some(p.para), p.lines.iter().map(|l| (l.y, l.line.pitch)).collect())),
                Child::Line { para, placed } => match pieces.last_mut() {
                    Some((Some(q), ls)) if Some(*q) == *para => ls.push((placed.y, placed.line.pitch)),
                    _ => pieces.push((*para, vec![(placed.y, placed.line.pitch)])),
                },
                Child::Table(t) => {
                    let b = bottoms.get(&t.table).copied().unwrap_or(0.0);
                    st.extra += lu(b * t.rows.len().saturating_sub(1) as f64);
                    pieces.extend(t.rows.iter().map(|r| (None, vec![(r.rect[1], r.rect[3] - r.rect[1])])));
                }
                _ => {}
            }
        }
        let mut prev_end = top;
        for (i, (para, ls)) in pieces.iter().enumerate() {
            let Some(&(y0, _)) = ls.first() else { continue };
            let (yl, pl) = ls[ls.len() - 1];
            let bottom = yl + pl;
            let after = match pieces.get(i + 1).and_then(|p| Some((p.0, p.1.first()?.0))) {
                Some((next_para, next)) => {
                    let gap = (next - bottom).max(0.0);
                    match para {
                        Some(q) => spacing.get(q).map_or(0.0, |s| s.1).min(gap),
                        // A table part's closing border belongs to its last row.
                        None => (gap - next_para.and_then(|q| spacing.get(&q)).map_or(0.0, |s| s.0)).max(0.0),
                    }
                }
                None => tail,
            };
            let before = (y0 - prev_end).max(0.0);
            let h = bottom + after - prev_end.min(y0);
            prev_end = bottom + after;
            st.lines += ls.len() as i64;
            st.height += lu(h);
            let step = if ls.len() == 1 {
                h
            } else {
                ls.iter()
                    .enumerate()
                    .map(|(j, &(_, p))| p + if j == 0 { before } else { 0.0 } + if j + 1 == ls.len() { after } else { 0.0 })
                    .fold(f64::MAX, f64::min)
            };
            st.min = st.min.min(lu(step));
        }
        if st.min == i64::MAX {
            st.min = 0;
        }
        st
    }
}
