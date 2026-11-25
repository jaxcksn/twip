//! Paragraphs beside floats

use std::rc::Rc;

use crate::docx::Compat;
use crate::docx::model::*;
use crate::docx::props::LineRule;
use crate::layout::float::{self, Frames, Placed, Wrap};
use crate::layout::fragment::{Child, ParaFragment, PlacedLine};
use crate::layout::ids::ParaId;
use crate::layout::job::{Memos, Yield};
use crate::layout::join::piece_para;
use crate::layout::line::{
    Below, Frame, Line, LineOrigin, ParaLines, apply_borders, band, border_key, breaks_only, continued_key, draw_line, opens_part, shade_gap,
    top_border_extra,
};
use crate::layout::paragraph::{gap, lines_to_take, space_after, space_before};
use crate::layout::space::BorderKey;
use crate::layout::space::{ColumnSpace, ParaSpace};
use crate::layout::story::StoryEnv;
use crate::layout::text::{FieldValues, Kind, Object, Part, Piece, ShapedPara, para_id};
use crate::layout::token::{At, FlowState, NoteArea, NotePart};
use crate::layout::units::EPS;

use super::{ColumnCursor, PageFill, Stop};

// Wrapped paragraph inputs
#[derive(Clone, Copy)]
struct WrapIn<'a> {
    p: &'a Paragraph,
    pb: &'a ParaLines,
    style: &'a ParaStyle,
    col: &'a ColumnSpace,
    sp: &'a ShapedPara,
    mark: &'a Run,
    plain: Frame,
    pieces: &'a [Piece],
    n: usize,
    est: f64,
    est_below: f64,
    top_extra: f64,
    cx0: f64,
    cx1: f64,
    compat: Compat,
    restarts: bool,
    lone_picture: bool,
    kept_after: f64,
    floats: &'a [Placed],
    first_part: bool,
    at_top: bool,
    top: f64,
    share: f64,
    origin_y: f64,
    y0: f64,
    kept: f64,
    si: usize,
    bi: usize,
    page_first: bool,
    anchor: ParaId,
    after: f64,
}

impl WrapIn<'_> {
    fn own_float(&self, f: &&Placed) -> bool {
        let &WrapIn { cx0, cx1, si, bi, .. } = self;
        (f.key.0, f.key.1) == (si, bi) && f.wrap != Wrap::None && f.excl[0] < cx1 - EPS && f.excl[2] > cx0 + EPS
    }

    /// While hidden text shows, the border key above shown part `part`'s lines from band `a` on, and whether they open
    /// the part: the paragraph's own before its first, the part before's at a part's first line.
    fn part_top(&self, parts: &[Part], part: usize, a: usize) -> (BorderKey, bool) {
        let starts = a > 0 || if part == 0 { self.first_part || self.restarts } else { opens_part(self.sp, self.pieces, part) };
        let key = match part {
            0 if a == 0 && starts => self.pb.borders.0.clone(),
            1.. if starts => border_key(&parts[part - 1].style),
            _ => continued_key(&parts[part].style),
        };
        (key, starts)
    }

    /// How far below `line`, the last of its shown part, its bottom border goes before part `next`'s first line.
    fn part_bottom(&self, parts: &[Part], line: &Line, next: usize) -> f64 {
        let (s, q) = (&*parts[line.part].style, &*parts[next].style);
        if border_key(s).is_none() && s.div_boxes.is_empty() {
            return 0.0;
        }
        let mut probe = [Line { pitch: line.pitch, after: line.after, ..Line::empty(line.start) }];
        apply_borders(&mut probe, s, continued_key(s).as_deref(), border_key(q).as_deref(), self.col.width, true, false, true, 0.0, &[]);
        probe[0].pitch - line.pitch
    }
}

// A band's strip
struct Strip {
    uncut: Option<Vec<Placed>>,
    forced: Option<Vec<(f64, f64)>>,
    fitted: Option<(Vec<(f64, f64)>, f64, f64, f64)>,
    reuse: bool,
}

// A band tried
struct Tried {
    got: Option<(Vec<Line>, usize, Option<usize>)>,
    blocked: bool,
    trial: Option<f64>,
    no_room: bool,
    h: f64,
    // How far below the line's top a float already counted in the pieces it didn't fit.
    slack: f64,
}

// Anchored floats of the paragraph
struct Anchors {
    anchors: Vec<Placed>,
    kept_at: Vec<(usize, usize, usize)>,
    forced: Option<usize>,
    push: Option<usize>,
    page_end: bool,
}

fn band_of(bands: &[(f64, Vec<Line>)], k: usize) -> Option<usize> {
    bands.iter().position(|(_, segs)| segs.iter().any(|l| k >= l.start && k < l.end.max(l.start + 1)))
}

// Compat ≤ 14 steps a line that fits no piece in whole steps to the end of its strip, never past the top of a float
// starting further than the slack below it.
fn strip_step(floats: &[Placed], cx0: f64, cx1: f64, y: f64, step: f64, slack: f64) -> f64 {
    let across = |f: &&Placed| f.wrap != Wrap::None && f.excl[2] > cx0 + EPS && f.excl[0] < cx1 - EPS;
    let top = floats.iter().filter(across).map(|f| f.excl[1]).filter(|&t| t > y + slack + EPS).fold(f64::INFINITY, f64::min);
    let end = floats.iter().filter(across).filter(|f| f.excl[1] <= y + slack + EPS).map(|f| f.excl[3]).filter(|&b| b > y + EPS).fold(top, f64::min);
    let k = if end.is_finite() { ((end - y - EPS) / step).floor().max(0.0) + 1.0 } else { 1.0 };
    (y + k * step).min(top)
}

impl<'d> PageFill<'d, '_> {
    pub fn fit_frame(&self, f: &Rc<Float>, width: f64, memos: &mut Memos) -> Result<Rc<Float>, Yield> {
        let fields = FieldValues::default();
        let env = StoryEnv { ctx: self.ctx, fields: &fields, notes: self.note_numbers };
        Ok(crate::layout::story::fit_frame(f, width, &env, memos)?.map_or_else(|| f.clone(), Rc::new))
    }

    pub fn float_height(&self, f: &Float, memos: &mut Memos) -> Result<f64, Yield> {
        let fields = FieldValues::default();
        crate::layout::story::float_height(f, &StoryEnv { ctx: self.ctx, fields: &fields, notes: self.note_numbers }, memos)
    }

    fn wrapped(&self, p: Placed, memos: &mut Memos) -> Result<Placed, Yield> {
        let fields = FieldValues::default();
        let lines = crate::layout::story::tight_lines(&p.float, &StoryEnv { ctx: self.ctx, fields: &fields, notes: self.note_numbers }, memos)?;
        let p = if self.ctx.env.text.compat == Compat::Legacy { p.legacy_dist() } else { p };
        Ok(match lines {
            Some(l) => p.around_lines(&l),
            None => p,
        })
    }

    // Wrapped paragraphs
    pub(super) fn place_wrapped(
        &mut self,
        p: &Paragraph,
        pb: &ParaLines,
        cur: &mut ColumnCursor,
        col: &ColumnSpace,
        memos: &mut Memos,
    ) -> Result<Stop, Yield> {
        let (pos, carry, y, empty, area, _) = cur.parts();
        let style = p.para_style_in(self.ctx.env.text.review.view);
        let first_part = pos.at.at_start();
        let after = space_after(&self.ctx.env.text, style);
        let kept_after = self.after_kept(pos.block, style);
        let at_top = *empty;
        let top = *y + self.para_gap(p, pos, carry, *empty) - if at_top { self.top_lift(style, pb) } else { 0.0 };
        let mut floats: Vec<Placed> = self.fixed.iter().chain(self.known_at(pos.block)).cloned().collect();
        for f in floats.iter_mut().filter(|f| f.key.0 == self.sec.index && f.key.1 > pos.block) {
            if let Some(r) = f.early_right {
                (f.rect[2], f.excl[2]) = (r, r);
            }
            if let Some((r, e)) = f.early_box {
                (f.rect, f.excl) = (r, e);
            }
        }
        let letter_break = !first_part && carry.letter_break;
        let env = self.ctx.env.text.clone();
        let space = ParaSpace {
            width: col.width,
            start: pb.from,
            part: pos.at.part(),
            borders: (None, None),
            x0: col.x,
            env,
            fit: false,
            letter_break,
            shade_gap: 0.0,
        };
        let frame = Frame::new(&space);
        let sp: &ShapedPara = &pb.shaped;
        let pieces = pb.pieces();
        let n = pieces.len();
        let est = pb.lines.first().map_or(13.0, |l| l.pitch);
        let est_below = pb.lines.first().map_or(0.0, |l| l.below);
        let top_extra = top_border_extra(style, pb.borders.0.as_deref(), first_part);
        let (cx0, cx1) = (col.x, col.x + col.width);
        let compat = self.ctx.env.text.compat;
        // After a leading page or column break the paragraph starts again with its first line.
        let restarts = sp.opens
            && pb.from > 0
            && sp.pieces[..pb.from.min(sp.pieces.len())].iter().all(|q| matches!(q.kind, Kind::PageBreak | Kind::ColumnBreak));
        let mut content = pieces.iter().filter(|q| q.kind != Kind::Anchor);
        let lone_picture = compat == Compat::Legacy && content.next().is_some_and(|q| q.kind == Kind::Object) && content.next().is_none();
        // Text laid out before the paragraph in this column ends at `*y`, then the space after it keeps.
        let kept = carry.prev.as_ref().map_or(carry.prev_after, |ps| gap(&self.ctx.env.text, ps, carry.prev_after, style, 0.0));
        // The paragraph's top is above its space before: where that text and its kept space after end.
        let origin = if at_top { *y } else { *y + kept };
        // The space before is part of the first line's box: a float beside it wraps the line.
        let lead = if first_part || restarts { (top - origin).max(0.0) } else { 0.0 };
        // A first line moved below a float is no longer at the column top: it keeps its share of the gap above its
        // text, the space before suppressed at the top included.
        let share = if !(first_part || restarts) {
            0.0
        } else if at_top {
            let before = self.own_before(style, pos.block, carry);
            (crate::layout::paragraph::gap_plain(&self.ctx.env.text, carry.prev_after, before) - carry.prev_after).max(lead)
        } else {
            lead
        };
        let bi = pos.block;
        let si = self.sec.index;
        let anchor = para_id(p);
        // Compat 15, a float that can't be placed: the line holding its anchor goes to the next column (the next
        // page from a column's top) when it doesn't start the page; when it does, the page ends after it.
        let page_first = at_top && self.column == 0 && !self.mid_page;
        let w = WrapIn {
            p,
            pb,
            style,
            col,
            sp,
            mark: p.mark_in(self.ctx.env.text.review.view),
            plain: frame,
            pieces,
            n,
            cx0,
            cx1,
            compat,
            floats: &floats,
            est,
            est_below,
            top_extra,
            restarts,
            lone_picture,
            kept_after,
            first_part,
            at_top,
            top,
            share,
            origin_y: origin,
            y0: *y,
            kept,
            si,
            bi,
            page_first,
            anchor,
            after,
        };
        let (mut bands, s, more) = self.fit_bands(&w, carry, lead, frame);
        if bands.is_empty() {
            if self.at.frames_only {
                return Ok(Stop::Page);
            }
            return self.place_plain(p, pb, cur, col, memos);
        }
        if let Some(parts) = sp.parts.as_deref() {
            // Shown parts: each its own borders and shading, the parts beside it its neighbours.
            let (mut a, nb) = (0, bands.len());
            while a < nb {
                let part = bands[a].1[0].part;
                let b = (a + 1..nb).find(|&b| bands[b].1[0].part != part).unwrap_or(nb);
                let s = &*parts[part].style;
                let (before, starts) = w.part_top(parts, part, a);
                let (next, last, gap) = match bands.get(b).map(|q| &*parts[q.1[0].part].style) {
                    Some(q) => (border_key(q), true, shade_gap(&self.ctx.env.text, s, Some(q), true)),
                    None if more.is_some() => (border_key(s), false, 0.0),
                    None => (pb.borders.1.clone(), true, pb.shade_gap),
                };
                let mut firsts: Vec<Line> = bands[a..b].iter().map(|(_, segs)| segs[0].clone()).collect();
                let tops: Vec<f64> = bands[a..b].iter().map(|(y, _)| *y).collect();
                apply_borders(&mut firsts, s, before.as_deref(), next.as_deref(), col.width, true, starts, last, gap, &tops);
                for ((_, segs), l) in bands[a..b].iter_mut().zip(firsts) {
                    segs[0] = l;
                }
                a = b;
            }
        } else if border_key(style).is_some()
            || style.shading.is_some()
            || style.shd.as_ref().is_some_and(|s| s.hatch().is_some())
            || !style.div_boxes.is_empty()
        {
            let mut firsts: Vec<Line> = bands.iter().map(|(_, segs)| segs[0].clone()).collect();
            let tops: Vec<f64> = bands.iter().map(|(y, _)| *y).collect();
            let next = if more.is_none() { pb.borders.1.clone() } else { border_key(style) };
            let gap = if more.is_none() { pb.shade_gap } else { 0.0 };
            apply_borders(
                &mut firsts,
                style,
                pb.borders.0.as_deref(),
                next.as_deref(),
                col.width,
                first_part,
                first_part,
                more.is_none(),
                gap,
                &tops,
            );
            for ((_, segs), l) in bands.iter_mut().zip(firsts) {
                segs[0] = l;
            }
        }
        // Anchors
        let frames = Frames {
            page_w: self.page_w,
            page_h: self.page_h,
            margin: [self.margins.0, self.margins.1, self.margins.2, self.sec.section.section_in(self.ctx.env.text.review.view).bottom],
            column: (col.x, col.width),
            compat: self.ctx.env.text.compat,
            cell: false,
            odd: self.odd,
        };
        let Anchors { anchors, kept_at, mut forced, mut push, mut page_end } = self.place_anchors(&w, &bands, &frames, carry, memos)?;
        // Pagination
        let nb = bands.len();
        let (fit, areas, mut take) = self.page_bands(&w, &bands, area, memos)?;
        // A paragraph whose first line goes on to the next page leaves the frames before it on this page where their
        // own paragraph falls on it: one placed by the page or the margin always, one from the text when its height
        // fits below that place. They end at a frame that starts with a page break.
        let first_text = pieces.iter().position(|q| q.kind != Kind::Anchor).unwrap_or(n);
        let leading = |a: &Placed| a.float.text_box.is_some() && a.float.shape.is_none() && a.key.2 - pb.from < first_text;
        // A frame that starts with a page break ends the frames that stay; at a page's top the first one's break adds
        // no page.
        let breaks = |a: &Placed| matches!(a.float.text_box.as_ref().and_then(|tb| tb.blocks.first()), Some(Block::Para(p)) if p.para_style_in(self.ctx.env.text.review.view).page_break_before);
        let first_lead = anchors.iter().filter(|a| leading(a)).map(|a| a.key.2).min();
        let lead_end = anchors.iter().filter(|a| leading(a) && breaks(a) && !(page_first && Some(a.key.2) == first_lead)).map(|a| a.key.2).min();
        if self.at.frames_only && (page_first && lead_end.is_none() || !(first_part && col.bottom.holds(origin))) {
            return Ok(if page_first && lead_end.is_none() { Stop::Done } else { Stop::Page });
        }
        let lead_end = lead_end.unwrap_or(usize::MAX);
        let lead_only = self.at.frames_only || take == 0 && !at_top && first_part && col.bottom.holds(origin);
        if take == 0 && !lead_only {
            if at_top {
                take = fit.max(1).min(nb);
            } else {
                return Ok(Stop::Full);
            }
        }
        let stays = |a: &Placed| {
            leading(a)
                && a.key.2 < lead_end
                && (matches!(a.float.v_relative.as_str(), "page" | "margin") || col.bottom.holds(origin + a.rect[3] - a.rect[1]))
        };
        let placed_end = if lead_only { 0 } else { bands[take - 1].1.last().map_or(0, |l| l.end) };
        if !lead_only {
            carry.frames_left.clear();
        }
        for a in anchors {
            if lead_only && !stays(&a) {
                continue;
            }
            if lead_only || a.key.2 - pb.from < placed_end.max(1) {
                self.settle_anchor(&w, a, &kept_at, &bands, &frames, lead_only, &mut push, &mut forced, &mut page_end, carry);
            }
        }
        if lead_only {
            return Ok(if self.at.frames_only { Stop::Page } else { Stop::Full });
        }
        if let Some(b) = forced {
            take = take.min(b + 1);
        }
        // The split before a pushed line keeps the paragraph's own rules; the page's first paragraph keeps the lines before it.
        if let Some(b) = push {
            let keep = lines_to_take(nb, b, false, first_part, style.keep_lines, style.widow_control);
            take = take.min(if keep == 0 && page_first { b } else { keep });
            let end = if take == 0 { 0 } else { bands[take - 1].1.last().map_or(0, |l| l.end) };
            self.at.found.retain(|f| (f.key.0, f.key.1) != (si, bi) || f.key.2 == float::TABLE_PIECE || f.key.2 < pb.from + end);
            if take == 0 {
                carry.keep_top = false;
                return Ok(if at_top { Stop::Page } else { Stop::Full });
            }
        }
        self.emit_wrapped(&w, &bands, &areas, take, (s, more), page_end, cur)
    }

    // Compat ≤ 14, a keepLines paragraph and its own float: the tops of its lines from `from`, laid out from `top`
    // beside the floats while they start above `stop`, and the top of the first one at or below it when lines
    // remain. None when a line finds no piece.
    fn tops(&self, w: &WrapIn, top: f64, (from, part): (usize, usize), stop: f64) -> Option<(Vec<f64>, Option<f64>)> {
        let &WrapIn { style, sp, pieces, n, est, cx0, cx1, floats, first_part, restarts, mark, plain, .. } = w;
        let (mut t, mut k, mut v, mut frame) = (top, from, Vec::new(), plain);
        // The shown part of the next line, `None` once the paragraph is laid out
        let mut next = (k < n || sp.parts.is_some()).then_some(part);
        while let Some(part) = next {
            if t >= stop - EPS {
                return Some((v, Some(t)));
            }
            let iv = float::intervals(floats, cx0, cx1, t, t + est, true)?;
            let ivc: Vec<(f64, f64)> = iv.iter().map(|(a, b)| (a - cx0, b - cx0)).collect();
            frame.part = part;
            match band(sp, pieces, k, &ivc, k == 0 && (first_part || restarts), style, mark, &frame, self.ctx.fonts, iv.len() == 1) {
                Ok((segs, e, more)) if (e, more) != (k, Some(part)) => {
                    v.push(t);
                    (k, next) = (e, more);
                    t += segs[0].pitch;
                }
                _ => return None,
            }
        }
        Some((v, None))
    }

    fn all_beside(&self, w: &WrapIn, top: f64, from: (usize, usize), bottom: f64) -> bool {
        matches!(self.tops(w, top, from, bottom), Some((_, None)))
    }

    fn keep_lines_start(&self, w: &WrapIn, mut yy: f64) -> f64 {
        let &WrapIn { style, col, est, compat, floats, first_part, at_top, .. } = w;
        // Below a column's top it is tried as a block from where it was first laid out: below the float it stays; with
        // the float starting below its first line it goes on down to the top of the line whose band holds the float's
        // top; with the float's top in its first line it goes to the float's bottom unless the float reaches more than
        // its first place's distance to that top below the line; with the float over its first line it stays when
        // every line beside the float starts above its bottom, else it goes on down to the first one that doesn't.
        if compat == Compat::Legacy && style.keep_lines && first_part && !at_top {
            let own =
                floats.iter().filter(|f| f.frozen && f.excl[3] > yy + EPS).filter(|f| w.own_float(f)).min_by(|a, b| a.excl[1].total_cmp(&b.excl[1]));
            if let Some(f) = own {
                let (p0, ft, fb) = (yy, f.excl[1], f.excl[3]);
                let mut y = p0;
                yy = loop {
                    if y >= fb - EPS || y >= col.bottom.to_pt() {
                        break y;
                    }
                    let next = if ft >= y + est - EPS {
                        self.tops(w, y, (0, w.plain.part), ft + 2.0 * EPS).and_then(|(v, more)| more.and(v.last().copied()))
                    } else if ft >= y - EPS && fb - y <= ft - p0 + EPS {
                        break fb;
                    } else {
                        match self.tops(w, y, (0, w.plain.part), fb) {
                            Some((_, None)) => break y,
                            low => low.and_then(|(_, t)| t),
                        }
                    };
                    y = next.filter(|&t| t > y + EPS).unwrap_or(y + est);
                };
            }
        }
        yy
    }

    // Strip choice
    #[allow(clippy::too_many_arguments, reason = "the band's floats, the strip kept from the last line and where the line is tried")]
    fn choose_strip(
        &self,
        w: &WrapIn,
        here: &[Placed],
        ordered: &[&Placed],
        strip_at: &Option<(Vec<(f64, f64)>, f64, f64)>,
        slack_on: f64,
        h: f64,
        at: (usize, usize),
        mut yy: f64,
    ) -> (f64, Option<Strip>) {
        let &WrapIn { style, col, cx0, cx1, compat, first_part, at_top, .. } = w;
        let mut uncut: Option<Vec<Placed>> = None;
        let mut forced: Option<Vec<(f64, f64)>> = None;
        let mut fitted: Option<(Vec<(f64, f64)>, f64, f64, f64)> = None;
        let mut moved = None;
        let order: &[&Placed] = if here.is_empty() { &[] } else { ordered };
        let end_of = |ok: &[(f64, f64, f64)]| ok.iter().map(|p| p.2).fold(f64::INFINITY, f64::min);
        // The pieces from `v0` with that slack: lower, at where the strip ends, while none reaches further than
        // the slack below `v0`. With the top they were found at and how many pieces were found there first.
        let query = |v0: f64, slack: f64, h: f64| -> Option<(f64, Vec<(f64, f64, f64)>, usize)> {
            let mut t = v0;
            for _ in 0..64 {
                if t >= col.bottom.to_pt() - EPS {
                    return None;
                }
                let found = float::strip(order, cx0, cx1, t, t + h.max(slack), slack)?;
                let ok: Vec<(f64, f64, f64)> = found.iter().copied().filter(|p| slack < p.2 - v0 - EPS).collect();
                if !ok.is_empty() && t > v0 + EPS {
                    // Kept lower: fitted again there with a slack down to where its strip ends.
                    let slack = end_of(&ok) - v0;
                    let again = float::strip(order, cx0, cx1, t, t + h.max(slack), slack).unwrap_or_default();
                    let again: Vec<(f64, f64, f64)> = again.into_iter().filter(|p| slack < p.2 - t - EPS).collect();
                    let first = ok.len();
                    return Some((t, if again.is_empty() { ok } else { again }, first));
                }
                if !ok.is_empty() {
                    let first = ok.len();
                    return Some((t, ok, first));
                }
                let covering = order.iter().filter(|f| {
                    f.wrap != Wrap::None && f.excl[1] <= t + slack + EPS && f.excl[3] > t + EPS && f.excl[2] > cx0 + EPS && f.excl[0] < cx1 - EPS
                });
                let end = found.iter().map(|p| p.2).chain(covering.map(|f| f.excl[3])).fold(f64::INFINITY, f64::min);
                if !end.is_finite() || end <= t + EPS {
                    return None;
                }
                t = end;
            }
            None
        };
        let pieces_of = |ok: &[(f64, f64, f64)]| ok.iter().map(|p| (p.0, p.1)).collect::<Vec<(f64, f64)>>();
        let starts_below = |end: f64| {
            here.iter()
                .any(|f| f.wrap != Wrap::None && f.excl[1] > end - EPS && f.excl[1] < yy + h - EPS && f.excl[0] < cx1 - EPS && f.excl[2] > cx0 + EPS)
        };
        let reuse = compat == Compat::Legacy
            && strip_at.as_ref().is_some_and(|&(_, end, least)| {
                let room = end - yy;
                room > EPS && if slack_on > EPS { room >= h - EPS } else { room >= least - EPS && !starts_below(end) }
            });
        if reuse {
            forced = strip_at.as_ref().map(|x| x.0.clone());
        } else if compat == Compat::Legacy && slack_on > EPS {
            if let Some((t, ok, _)) = query(yy, slack_on, h) {
                fitted = Some((pieces_of(&ok), end_of(&ok), 0.0, 0.0));
                forced = Some(pieces_of(&ok));
                yy = t;
            }
        } else if compat == Compat::Legacy {
            let near = || here.iter().filter(|f| f.in_band(yy, yy + h) && f.excl[2] > cx0 + EPS && f.excl[0] < cx1 - EPS);
            let over: Vec<Placed> = near().filter(|f| f.excl[1] <= yy + EPS).cloned().collect();
            let e = over.iter().map(|f| f.excl[3]).fold(f64::INFINITY, f64::min);
            // A float starting inside the band counts only where it meets what the floats over the band's top
            // leave of the line: else it neither moves nor cuts the line. A top-and-bottom float takes the whole line.
            let left = float::intervals(&over, cx0, cx1, yy, yy + h, true).unwrap_or_default();
            let meets = |f: &Placed| f.wrap == Wrap::TopAndBottom || left.iter().any(|&(a, b)| f.excl[0] < b - EPS && f.excl[2] > a + EPS);
            let hidden = |f: &Placed| f.in_band(yy, yy + h) && f.excl[1] > yy + EPS && !meets(f);
            // The line is fitted again with a slack: where they leave it one piece, down to where that piece ends,
            // else its height. A piece is kept only if it reaches further than the slack below the line's top;
            // with none the line goes down to where the strip ends and is fitted there.
            if e < yy + h - EPS && near().any(|f| f.excl[1] > yy + EPS && meets(f)) {
                let one = float::strip(order, cx0, cx1, yy, yy + h, 0.0).filter(|p| p.len() == 1).map(|p| p[0].2 - yy);
                let slack = one.unwrap_or(h);
                match query(yy, slack, h) {
                    Some((t, ok, first)) => {
                        let frame = |f: &&&Placed| f.float.text_box.is_some() && f.float.shape.is_none() && f.key.2 != float::TABLE_PIECE;
                        let framed = || {
                            order
                                .iter()
                                .filter(frame)
                                .any(|f| f.excl[1] < t + h - EPS && f.excl[3] > yy + EPS && f.excl[0] < cx1 - EPS && f.excl[2] > cx0 + EPS)
                        };
                        // Only a line fitted at its top in one piece remembers its drop.
                        let drop = if one.is_some() && first >= 2 && framed() { t - yy } else { 0.0 };
                        fitted = Some((pieces_of(&ok), end_of(&ok), one.map_or(0.0, |_| slack), drop));
                        forced = Some(pieces_of(&ok));
                        moved = Some(t);
                    }
                    None => {
                        yy = e;
                        return (yy, None);
                    }
                }
            }
            if here.iter().any(hidden) {
                uncut = Some(here.iter().filter(|f| !hidden(f)).cloned().collect());
            }
            if style.keep_lines && forced.is_none() {
                let first = near().filter(|f| f.wrap != Wrap::None && f.excl[1] > yy + EPS).min_by(|a, b| a.excl[1].total_cmp(&b.excl[1]));
                // Its own float below a column's top was settled with the paragraph's place above.
                if let Some(f) = first.filter(|f| f.excl[1] < yy + h - EPS && (at_top || !first_part || !w.own_float(f))) {
                    // At a column's top, beside its own float only when the rest of it starts above the float's
                    // bottom there; else down past the float in steps of its height, full width.
                    let (ft, fb) = (f.excl[1], f.excl[3]);
                    yy = ft;
                    if at_top && first_part && w.own_float(&f) && !self.all_beside(w, ft, at, fb) {
                        yy = ft + ((fb - ft) / h - EPS).ceil() * h;
                    }
                    return (yy, None);
                }
            }
        }
        if let Some(t) = moved {
            yy = t;
        }
        (yy, Some(Strip { uncut, forced, fitted, reuse }))
    }

    // Fit the line's pieces, at most twice
    #[allow(clippy::too_many_arguments, reason = "the floats that cut the line, its strip, and where and how high it is tried")]
    fn try_band(
        &self,
        w: &WrapIn,
        bands: &[(f64, Vec<Line>)],
        cutting: &[Placed],
        forced: &Option<Vec<(f64, f64)>>,
        lead: f64,
        frame: &Frame,
        s: usize,
        yy: f64,
        mut h: f64,
        mut below: f64,
    ) -> Tried {
        let &WrapIn { p, style, sp, pieces, n, top_extra, cx0, cx1, compat, restarts, lone_picture, kept_after, first_part, top, origin_y, .. } = w;
        let mut got = None;
        let mut blocked = false;
        let mut trial = None;
        // Every piece of the line was cut away or too narrow.
        let mut no_room = false;
        let first = (first_part || restarts) && bands.is_empty();
        let lift = if first { (top - origin_y).max(0.0) } else { 0.0 };
        // Compat ≤ 14 first fits the line into the pieces the floats over its top leave; when it fits none of two or
        // more, it steps before a float starting lower in the line counts.
        let starts_in =
            |f: &Placed| f.wrap != Wrap::None && f.excl[1] > yy + EPS && f.excl[1] < yy + h - EPS && f.excl[0] < cx1 - EPS && f.excl[2] > cx0 + EPS;
        if compat == Compat::Legacy && forced.is_none() && cutting.iter().any(starts_in) {
            let refs: Vec<&Placed> = cutting.iter().collect();
            let over = float::strip(&refs, cx0, cx1, yy, yy + h, 0.0).unwrap_or_default();
            if over.len() > 1 {
                let ivc: Vec<(f64, f64)> = over.iter().map(|p| (p.0 - cx0, p.1 - cx0)).collect();
                let mark = p.mark_in(self.ctx.env.text.review.view);
                if let Err(t) = band(sp, pieces, s, &ivc, first, style, mark, frame, self.ctx.fonts, lone_picture) {
                    return Tried { got: None, blocked: true, trial: Some(t + lift), no_room: false, h, slack: 0.0 };
                }
            }
        }
        for attempt in 0..2 {
            let free = match &forced {
                Some(iv) => Some(iv.clone()),
                None => {
                    // A float over the whole line that reaches into its space before moves it, whichever paragraph
                    // anchors it.
                    let lead = if bands.is_empty() { lead } else { 0.0 };
                    float::text_intervals(cutting, cx0, cx1, yy - lead, yy + h, yy + h - below, compat == Compat::Legacy)
                }
            };
            match free {
                None => {
                    blocked = true;
                    break;
                }
                Some(iv) => {
                    no_room = iv.is_empty();
                    let ivc: Vec<(f64, f64)> = iv.iter().map(|(a, b)| (a - cx0, b - cx0)).collect();
                    // A word too long for the line breaks after the last character that fits: in an uncut line,
                    // and in compat ≤ 14 in a line's only piece beside a float.
                    let open = iv.len() == 1 && (compat == Compat::Legacy || (iv[0].0 - cx0).abs() < EPS && (iv[0].1 - cx1).abs() < EPS);
                    let r = band(
                        sp,
                        pieces,
                        s,
                        &ivc,
                        first,
                        style,
                        p.mark_in(self.ctx.env.text.review.view),
                        frame,
                        self.ctx.fonts,
                        open || lone_picture,
                    );
                    match r {
                        Ok((mut segs, e, more)) => {
                            let extra = match sp.parts.as_deref() {
                                // A shown part's first band takes its top border
                                Some(parts) if bands.last().is_none_or(|b| b.1[0].part != segs[0].part) => {
                                    let (key, starts) = w.part_top(parts, segs[0].part, bands.len());
                                    top_border_extra(&parts[segs[0].part].style, key.as_deref(), starts)
                                }
                                Some(_) => 0.0,
                                None if first => top_extra,
                                None => 0.0,
                            };
                            for l in &mut segs {
                                l.pitch += extra;
                                l.baseline += extra;
                            }
                            // Compat ≤ 14 tests the paragraph's last line with its space after.
                            let tail = if compat == Compat::Legacy && e >= n { kept_after } else { 0.0 };
                            let (ph, segs_below) = (segs[0].pitch + tail, segs[0].below + tail);
                            let done = (ph - h).abs() < EPS && (segs_below - below).abs() < EPS || attempt == 1;
                            got = Some((segs, e, more));
                            if done {
                                break;
                            }
                            h = ph;
                            below = segs_below;
                        }
                        Err(t) => {
                            blocked = true;
                            trial = Some(t + lift);
                            break;
                        }
                    }
                }
            }
        }
        Tried { got, blocked, trial, no_room, h, slack: h }
    }

    // Bands
    /// The paragraph's bands, where the next one would start and its shown part (`None`: the paragraph is laid out).
    fn fit_bands(&self, w: &WrapIn, carry: &FlowState, mut lead: f64, mut frame: Frame) -> (Vec<(f64, Vec<Line>)>, usize, Option<usize>) {
        let &WrapIn { style, col, sp, pieces, n, est, est_below, cx0, cx1, compat, floats, first_part, at_top, top, share, bi, .. } = w;
        let mut bands: Vec<(f64, Vec<Line>)> = Vec::new();
        let mut yy = top;
        let mut s = 0;
        let mut guard = 0;
        // Compat ≤ 14 takes the floats in the order they were placed. The strip a line was fitted into with a slack
        // (its pieces and where they end) serves the next lines too while it ends below their top, by at least that
        // slack when it was fitted in one piece, unless a float starts between its end and the line's bottom. Once a
        // line fitted at its top in one piece went down in its own query to a strip of two or more pieces, a frame
        // among the floats over its band, the paragraph's later lines reuse a strip only while the whole line fits in it, and their queries take the
        // line's height plus that drop as the slack.
        // ponytail: a strip found with no slack isn't kept (the next line's own query gives the same pieces where
        // measured), both reset with each paragraph, and a second drop keeps the first slack.
        let mut ordered: Vec<&Placed> = floats.iter().collect();
        ordered.sort_by_key(|f| f.key);
        // A strip kept from a line's own query: floating tables and frames cut the line before shapes do.
        let mut figures_first = ordered.clone();
        figures_first.sort_by_key(|f| f.key.2 != float::TABLE_PIECE && !(f.float.text_box.is_some() && f.float.shape.is_none()));
        let mut strip_at: Option<(Vec<(f64, f64)>, f64, f64)> = None;
        let mut slack_on = 0.0;
        // Compat 15: the page's first line, holding the anchor of a float that found no place, goes below the page's
        // floats, across the whole column.
        let unplaced = compat == Compat::Generic
            && at_top
            && self.column == 0
            && !self.mid_page
            && self.at.known.iter().any(|k| k.unplaced && (k.key.0, k.key.1) == (self.sec.index, bi));
        if unplaced {
            let low = floats.iter().filter(|f| f.wrap != Wrap::None).map(|f| f.excl[3]).fold(yy, f64::max);
            yy = low.min(col.bottom.to_pt());
        }
        yy = self.keep_lines_start(w, yy);
        // A paragraph ending in a line break has an empty last line after it.
        let trailing = |bands: &[(f64, Vec<Line>)]| {
            bands.last().and_then(|(_, segs)| segs.last()).is_some_and(|l| l.end == n && l.start < l.end && l.hard == Some(Kind::LineBreak))
        };
        let mut more = Some(frame.part);
        while (more.is_some() || bands.is_empty() || trailing(&bands)) && guard < 4000 {
            guard += 1;
            let h = est;
            let below = est_below;
            let here: &[Placed] = if yy >= col.bottom.to_pt() - EPS || unplaced && bands.is_empty() { &[] } else { floats };
            let (moved_to, strip) = self.choose_strip(w, here, &ordered, &strip_at, slack_on, h, (s, frame.part), yy);
            yy = moved_to;
            let Some(Strip { uncut, forced, fitted, reuse }) = strip else { continue };
            let cutting = uncut.as_deref().unwrap_or(here);
            let Tried { got, blocked, trial, no_room, h, slack } = self.try_band(w, &bands, cutting, &forced, lead, &frame, s, yy, h, below);
            match (got, blocked) {
                (Some((segs, e, next)), false) => {
                    let ph = segs[0].pitch;
                    // A shown part's bottom border below its last band
                    let below_part = match (sp.parts.as_deref(), next) {
                        (Some(parts), Some(q)) if q != segs[0].part => w.part_bottom(parts, &segs[0], q),
                        _ => 0.0,
                    };
                    let hard = segs.last().is_some_and(|l| matches!(l.hard, Some(Kind::PageBreak | Kind::ColumnBreak)));
                    let clear = segs.last().and_then(|l| Some((crate::layout::line::clear_of(sp, pieces, l)?, l.xs.first().copied().unwrap_or(0.0))));
                    if segs.last().is_some_and(|l| !l.hyphen) {
                        frame.word_from = Some(e);
                    }
                    if let Some((iv, end, stay, drop)) = fitted {
                        strip_at = Some((iv, end, stay));
                        if drop > EPS && slack_on <= EPS {
                            slack_on = ph + drop;
                        }
                    } else if !reuse {
                        // Compat ≤ 14 keeps a line's pieces for the next lines while none of them ends, though a float
                        // that took a piece away ends sooner.
                        let same = |k: &[(f64, f64, f64)], u: &[(f64, f64)]| {
                            k.len() == u.len() && k.iter().zip(u).all(|(k, u)| (k.0 - u.0).abs() < EPS && (k.1 - u.1).abs() < EPS)
                        };
                        strip_at = (compat == Compat::Legacy && forced.is_none() && !here.is_empty())
                            .then(|| {
                                let kept = float::strip(&figures_first, cx0, cx1, yy, yy + ph, 0.0)?;
                                let used = float::intervals(cutting, cx0, cx1, yy, yy + ph, true)?;
                                let end = kept.iter().map(|p| p.2).fold(f64::INFINITY, f64::min);
                                // Only a strip ending at a float's bottom, which a line may overhang; none beside a
                                // wrap polygon, which is queried again for every line.
                                let at_top = here.iter().any(|f| f.excl[1] > yy + EPS && (f.excl[1] - end).abs() < EPS);
                                let polygon = here.iter().any(|f| f.contour.is_some() && f.excl[1] < end && f.excl[3] > yy);
                                (!kept.is_empty() && end.is_finite() && !at_top && !polygon && same(&kept, &used)).then_some((used, end, 0.0))
                            })
                            .flatten();
                    }
                    bands.push((yy, segs));
                    (more, frame.part) = (next, next.unwrap_or(frame.part));
                    let next = yy + ph + below_part;
                    yy = match clear {
                        Some(((l, r), x)) if compat == Compat::Generic => {
                            let iv = float::intervals(here, cx0, cx1, next - ph, next, false).unwrap_or_default();
                            let (a, b) = iv.iter().copied().find(|&(a, b)| cx0 + x >= a - EPS && cx0 + x <= b + EPS).unwrap_or((cx0, cx1));
                            here.iter()
                                .filter(|f| f.in_band(next - ph, next) && ((l && f.excl[2] <= a + EPS) || (r && f.excl[0] >= b - EPS)))
                                .map(|f| f.excl[3])
                                .fold(next, f64::max)
                        }
                        Some(((l, r), _)) => {
                            let mut at = next;
                            while at < col.bottom.to_pt() - EPS {
                                let first = float::intervals(here, cx0, cx1, at, at + ph, true).and_then(|iv| iv.first().copied());
                                if first.is_some_and(|(a, b)| (!l || a <= cx0 + EPS) && (!r || b >= cx1 - EPS)) {
                                    break;
                                }
                                match float::next_edge(here, cx0, cx1, at, at + ph) {
                                    Some(e) => at = e,
                                    None => break,
                                }
                            }
                            at
                        }
                        None => next,
                    };
                    s = e;
                    if hard {
                        break;
                    }
                }
                _ => {
                    let meets = |f: &&Placed| f.in_band(yy, yy + h) && f.excl[2] > cx0 + EPS && f.excl[0] < cx1 - EPS;
                    // Compat ≤ 14: a line with a piece too narrow for it steps down instead (below).
                    let steps = compat == Compat::Legacy && trial.is_some() && !no_room;
                    if first_part && bands.is_empty() && !steps && here.iter().filter(meets).any(|f| f.key.2 == float::TABLE_PIECE) {
                        // To the nearest bottom among the floats it meets, then tried again there.
                        let below = here.iter().filter(meets).map(|f| f.excl[3]).filter(|&b| b > yy + EPS).fold(f64::INFINITY, f64::min);
                        let below = if below.is_finite() { below } else { yy };
                        let before = if style.before_auto && carry.first_para { 0.0 } else { space_before(&self.ctx.env.text, style) };
                        yy = (below + before).min(col.bottom.to_pt().max(yy + EPS));
                        continue;
                    }
                    // A vertical section's table moves the line to its far side whole.
                    let square = here
                        .iter()
                        .filter(|f| f.in_band(yy, yy + h))
                        .all(|f| f.float.wrap == crate::docx::FloatWrap::Square && f.key.2 != float::UPRIGHT_TABLE);
                    // Compat ≤ 14: a line whose content fits no piece retries lower in steps of its height. One left no
                    // piece at all first goes to the top of a float starting inside it, then down by its height or to
                    // the bottom of the floats over its top, whichever is nearer.
                    if let Some(t) = trial.filter(|&t| compat == Compat::Legacy && square && t > EPS) {
                        let starts = here.iter().filter(meets).filter(|f| f.excl[1] > yy + EPS).map(|f| f.excl[1]).fold(f64::INFINITY, f64::min);
                        let covers = here.iter().filter(meets).filter(|f| f.excl[1] <= yy + EPS).map(|f| f.excl[3]).fold(f64::INFINITY, f64::min);
                        let next = match no_room {
                            true if starts < yy + h - EPS => starts,
                            true => (yy + t).min(covers.max(yy + EPS)),
                            false => strip_step(here, cx0, cx1, yy, t, slack),
                        };
                        yy = next.min(col.bottom.to_pt().max(yy + EPS));
                        continue;
                    }
                    // The first line's box, its space before included, goes below the float whole.
                    let box_top = if bands.is_empty() { yy - lead } else { yy };
                    match float::next_edge(here, cx0, cx1, box_top, yy + h).filter(|_| !float::steps(here, box_top, yy + h)) {
                        Some(b) if bands.is_empty() && share > EPS => {
                            lead = share;
                            yy = (b + share).min(col.bottom.to_pt().max(yy + EPS));
                        }
                        Some(b) => yy = b.min(col.bottom.to_pt().max(yy + EPS)),
                        None => yy += h,
                    }
                }
            }
        }
        (bands, s, more)
    }

    // Anchored floats
    fn place_anchors(
        &mut self,
        w: &WrapIn,
        bands: &[(f64, Vec<Line>)],
        frames: &Frames,
        carry: &FlowState,
        memos: &mut Memos,
    ) -> Result<Anchors, Yield> {
        let &WrapIn {
            pb, style, col, sp, pieces, cx0, cx1, compat, floats, first_part, origin_y: origin, y0, kept, si, bi, page_first, anchor, ..
        } = w;
        let mut anchors: Vec<Placed> = Vec::new();
        let mut kept_at = Vec::new();
        let mut forced: Option<usize> = None;
        let mut push: Option<usize> = None;
        let mut page_end = false;
        // A character-relative float counts from the drawn start of the character before its anchor, on whichever line
        // holds it; one first in the paragraph from where its line's first glyph is drawn.
        let char_x = |k: usize, b: usize| {
            let at = |j: usize| bands.iter().flat_map(|(_, segs)| segs).find(|l| j >= l.start && j < l.end).map(|l| (l, l.xs[j - l.start]));
            let before = (0..k).rev().find(|&j| pieces[j].kind != Kind::Anchor).and_then(|j| {
                let (_, x) = at(j)?;
                let (g0, g1) = pieces[j].glyphs;
                Some(x + sp.glyph_advances[g0 as usize..(g1 as usize).max(g0 as usize + 1) - 1].iter().sum::<f64>())
            });
            let start = || {
                let l = &bands[b].1[0];
                (l.start..l.end).find(|&j| pieces[j].kind != Kind::Anchor).map_or(0.0, |j| l.xs[j - l.start])
            };
            cx0 + before.unwrap_or_else(start)
        };
        for (k, q) in pieces.iter().enumerate() {
            let (Kind::Anchor, Some(Object::Float(f))) = (q.kind, sp.object(pb.from + k)) else { continue };
            if carry.frames_left.contains(&(si, bi, pb.from + k)) {
                continue;
            }
            let Some(b) = band_of(bands, k) else { continue };
            let owner = piece_para(sp, pb.from + k, anchor);
            let f = &float::sized(f, frames).map_or_else(|| f.clone(), Rc::new);
            let f = &self.fit_frame(f, col.width, memos)?;
            let h = self.float_height(f, memos)?;
            let line_top = bands[b].0;
            let para_top = if first_part { origin } else { bands[0].0 };
            let mut x = float::h_pos(f, frames);
            // From the line laid out before the float: placed again once the line wraps it, it keeps that x, as the
            // first fill of the page that placed it found it (each fill takes the one before's), wherever later fills
            // put the paragraph.
            if f.h_relative == HorizontalRelation::Character {
                let before = self.at.known.iter().find(|o| o.key == (si, bi, pb.from + k) && o.wrap != Wrap::None).map(|o| o.rect[0]);
                x = before.unwrap_or_else(|| float::h_pos(f, &Frames { column: (char_x(k, b), 0.0), ..*frames }));
            }
            if compat == Compat::Legacy && f.h_relative == HorizontalRelation::Column {
                match f.h_align.as_deref() {
                    Some("left") => x += style.ind_left,
                    Some("right") => x -= style.ind_right,
                    // From a line that starts beside another paragraph's float (for a drawing, a frame before the
                    // paragraph included): from where its piece starts. One that doesn't wrap also counts from the
                    // piece its line was laid out again in beside the line's own wrapping floats.
                    None => {
                        let frame = |o: &Placed| o.float.text_box.is_some() && o.float.shape.is_none() && o.key.2 != float::TABLE_PIECE;
                        let drawing = f.text_box.is_none() || f.shape.is_some();
                        let segs = &bands[b].1;
                        let line = pb.from + segs[0].start..pb.from + segs.last().map_or(segs[0].end, |l| l.end).max(segs[0].start + 1);
                        let unwrapped = f.wrap == crate::docx::FloatWrap::None;
                        let others: Vec<Placed> = floats
                            .iter()
                            .chain(&self.at.found)
                            .chain(anchors.iter().filter(|o| !floats.iter().any(|f| f.key == o.key)))
                            .filter(|o| {
                                (o.key.0, o.key.1) != (si, bi)
                                    || drawing && frame(o) && o.key.2 < pb.from + k
                                    || unwrapped && o.wrap != Wrap::None && line.contains(&o.key.2)
                            })
                            .cloned()
                            .collect();
                        let piece = float::intervals(&others, cx0, cx1, line_top, line_top + bands[b].1[0].pitch, true);
                        x += piece.and_then(|iv| iv.first().map(|p| p.0)).unwrap_or(cx0) - cx0;
                    }
                    _ => {}
                }
            }
            let mut fy = float::v_pos(f, h, frames, para_top, line_top);
            // A frame from the text that would start above the page starts at its top.
            if f.text_box.is_some() && f.shape.is_none() && f.v_relative == "paragraph" {
                fy = fy.max(0.0);
            }
            // Compat ≤ 14: a drawing's page position in whole twips, for the collision with the lines above and the
            // drawing. Frames keep theirs.
            if compat == Compat::Legacy && (f.text_box.is_none() || f.shape.is_some()) {
                (x, fy) = ((x * 20.0).round() / 20.0, (fy * 20.0).round() / 20.0);
            }
            if Placed::new((si, bi, pb.from + k), owner, f, x, fy, h).wrap == Wrap::None {
                anchors.push(Placed::new((si, bi, pb.from + k), owner, f, x, fy, h));
                continue;
            }
            // One that met text laid out before it keeps the place it met it at while the page is filled again.
            let frozen = self.at.known.iter().find(|o| o.key == (si, bi, pb.from + k) && o.frozen).cloned();
            if let Some(k) = &frozen {
                let mut a = self.wrapped(Placed::new(k.key, owner, f, k.rect[0], k.rect[1], h), memos)?;
                // A frame that met only the line above it doesn't move that line.
                (a.collided, a.restarts, a.frozen, a.rounded_up) = (k.collided && !k.rounded_up, true, true, k.rounded_up);
                kept_at.push(a.key);
                anchors.push(a);
                continue;
            }
            (x, fy) = float::page_clamp(f, (x, fy), h, frames);
            // Compat 15 keeps the box with its effect extents (a VML shape's outline allowance too, not its
            // distances) on the page, its top no higher than the top margin.
            let mut clamped = None;
            if compat == Compat::Generic && matches!(f.v_relative.as_str(), "paragraph" | "line") {
                let top = frames.margin[2] + f.effect[1];
                // A VML shape moved down by it reaches no further below its box than above it.
                // ponytail: read off one shape moved by its top extent only; a shape from far above may differ.
                if f.vml && fy < top - EPS && f.effect[3] > f.effect[1] {
                    let mut g = (**f).clone();
                    g.effect[3] = g.effect[1];
                    clamped = Some(Rc::new(g));
                }
                fy = fy.max(top);
                let (l, r) = (x - f.effect[0], x + f.w + f.effect[2]);
                if l < 0.0 {
                    x -= l;
                } else if r > frames.page_w {
                    x -= (r - frames.page_w).min(l);
                }
            }
            if compat == Compat::Generic && matches!(f.v_relative.as_str(), "paragraph" | "line") && !col.bottom.holds(fy + h) {
                let up = col.bottom.to_pt() - h;
                let anchor_bottom = line_top + bands[b].1[0].pitch;
                if up >= anchor_bottom - EPS {
                    fy = up;
                } else if !(page_first && b == 0) {
                    push = Some(push.map_or(b, |x: usize| x.min(b)));
                    continue;
                } else {
                    let mut a = Placed::new((si, bi, pb.from + k), owner, f, x, fy, h);
                    a.wrap = Wrap::None;
                    forced = Some(forced.map_or(b, |x: usize| x.min(b)));
                    page_end = true;
                    anchors.push(a);
                    continue;
                }
            }
            let f = clamped.as_ref().unwrap_or(f);
            let mut a = self.wrapped(Placed::new((si, bi, pb.from + k), owner, f, x, fy, h), memos)?;
            // That text (with its space after in compat ≤ 14): a float wholly below it, or beside the column, doesn't
            // meet it. Earlier columns count as met.
            let reach = y0 + if compat == Compat::Legacy { kept } else { 0.0 };
            let met = a.excl[1] < reach - EPS && a.excl[0] < cx1 - EPS && a.excl[2] > cx0 + EPS;
            let across = a.excl[0] < cx1 - EPS && a.excl[2] > cx0 + EPS;
            let lines = a.excl[1] < line_top + bands[b].1[0].pitch - EPS && across;
            // Compat ≤ 14: a drawing meets its paragraph's lines after its anchor's too.
            let drawing = f.text_box.is_none() || f.shape.is_some();
            let (last, (last_top, last_segs)) = (bands.len() - 1, bands.last().expect("laid out"));
            let para_lines = compat == Compat::Legacy
                && drawing
                && last > b
                && a.excl[1] < last_top + last_segs[0].pitch - EPS
                && a.excl[3] > line_top + EPS
                && across;
            a.restarts = self.column > 0 || met || lines || para_lines;
            // Only the first float of the fill that meets earlier text of its column met it where it will stay: that
            // text wraps it in the next fill. A later one was placed beside text that moves, and is placed again.
            // Compat ≤ 14: one that meets the lines of its own keepLines paragraph below a column's top stays too, and
            // the paragraph starts below it; so does a drawing that meets its own paragraph's lines. Only text it met
            // before its paragraph wraps it in the next fill.
            let own = compat == Compat::Legacy && style.keep_lines && (self.column > 0 || met || lines) && a.excl[3] > bands[0].0 + EPS;
            let stays = compat == Compat::Legacy && drawing && (lines || para_lines);
            a.frozen = (met || own || stays) && !self.at.hit;
            a.collided = a.frozen && (met || own) || self.column > 0 && !met;
            anchors.push(a);
        }
        // Compat ≤ 14: the other drawings of the paragraph keep their places with one that stays.
        let drawing = |a: &Placed| a.float.text_box.is_none() || a.float.shape.is_some();
        if compat == Compat::Legacy && anchors.iter().any(|a| a.frozen && drawing(a) && !kept_at.contains(&a.key)) {
            for a in anchors.iter_mut().filter(|a| drawing(a) && a.wrap != Wrap::None) {
                a.frozen = true;
            }
        }
        Ok(Anchors { anchors, kept_at, forced, push, page_end })
    }

    // Settle a float against the page's others
    #[allow(clippy::too_many_arguments, reason = "the float, the paragraph's bands and frames, and the decisions it may change")]
    fn settle_anchor(
        &mut self,
        w: &WrapIn,
        mut a: Placed,
        kept_at: &[(usize, usize, usize)],
        bands: &[(f64, Vec<Line>)],
        frames: &Frames,
        lead_only: bool,
        push: &mut Option<usize>,
        forced: &mut Option<usize>,
        page_end: &mut bool,
        carry: &mut FlowState,
    ) {
        let &WrapIn { pb, col, compat, page_first, .. } = w;
        // A frame placed in the flow: compat ≤ 14 lays it out below an earlier frame whose box with its
        // distances it meets, from the nearest twip; compat 15 moves one that may not overlap only down.
        let frame = |o: &Placed| o.float.text_box.is_some() && o.float.shape.is_none();
        let in_flow = frame(&a) && a.float.v_relative == "paragraph" && a.float.v_offset == 0.0;
        let mut under = Vec::new();
        // Compat ≤ 14 places a frame that may not overlap, or any frame once the page holds one, at the nearest
        // twip, its height kept.
        let apart = !a.allow_overlap || self.at.found.iter().any(|o| frame(o) && !o.allow_overlap);
        let flow_y = a.rect[1];
        if frame(&a) && apart && compat == Compat::Legacy && !kept_at.contains(&a.key) {
            a.moved_to(a.rect[0], (a.rect[1] * 20.0).round() / 20.0);
            // In the flow and so placed above its flow position, it meets the line above it. Met there in two
            // fills running, it keeps that place while the page is filled again.
            a.rounded_up = in_flow && a.rect[1] < flow_y - EPS && flow_y > col.top.to_pt() + EPS;
            a.frozen |= a.rounded_up && self.at.known.iter().any(|k| k.rounded_up && k.same(&a));
        }
        // The frames anchored after such a frame in its paragraph keep their places with it.
        // ponytail: Word keeps the later floats of the page too when no drawing met earlier text; not modelled.
        if compat == Compat::Legacy
            && frame(&a)
            && !kept_at.contains(&a.key)
            && self.at.found.iter().any(|o| o.rounded_up && o.frozen && (o.key.0, o.key.1) == (a.key.0, a.key.1) && o.key.2 < a.key.2)
        {
            (a.rounded_up, a.frozen) = (true, true);
        }
        // Compat ≤ 14: after the page started again for a later frame in the flow that met the line above it,
        // and once more for a drawing after that frame that met earlier text, a frame in the flow is placed
        // against the frames after it that may not overlap, where the fill before put them: one it overlaps it
        // goes below, a twip above its bottom in whole twips.
        let later = |k: &Placed, a: &Placed| k.key.0 == a.key.0 && (k.key.1, k.key.2) > (a.key.1, a.key.2);
        let by_frame = compat == Compat::Legacy && frame(&a) && self.at.known.iter().any(|k| k.rounded_up && later(k, &a));
        let again = by_frame
            && self.at.known.iter().any(|k| {
                k.rounded_up
                    && later(k, &a)
                    && self.at.known.iter().any(|d| d.frozen && !frame(d) && d.key.2 < float::UPRIGHT_TABLE && d.wrap != Wrap::None && later(d, k))
            });
        // ponytail: one that the nearest twip would put above its flow position meets the line above it
        // instead and stays (the line isn't wrapped round it), as measured only on its own page.
        let lifted = (a.rect[1] * 20.0).round() / 20.0 < a.rect[1] - EPS && a.rect[1] > col.top.to_pt() + EPS;
        if in_flow && again && !apart && !lifted && !kept_at.contains(&a.key) {
            let key = a.key;
            let later = self.at.known.iter().filter(|o| frame(o) && !o.allow_overlap && o.key.0 == key.0 && (o.key.1, o.key.2) > (key.1, key.2));
            for o in later {
                if o.excl[0] < a.rect[2] - EPS && a.rect[0] < o.excl[2] - EPS && o.rect[1] < a.rect[3] - EPS && a.rect[1] < o.rect[3] - EPS {
                    a.moved_to(a.rect[0], ((o.rect[1] * 20.0 + ((o.rect[3] - o.rect[1]) * 20.0).round() - 1.0) / 20.0).max(a.rect[1]));
                }
            }
        }
        if in_flow && compat == Compat::Legacy {
            for o in self.at.found.iter().filter(|o| frame(o)) {
                if o.excl[0] < a.rect[2] - EPS && a.rect[0] < o.excl[2] - EPS && o.rect[1] < a.rect[3] - EPS && a.rect[1] < o.rect[3] - EPS {
                    // Over a frame placed from the page or margin it stays, unless one of the two may not
                    // overlap: then it goes a twip above that frame's bottom, in whole twips. So does one that
                    // may not overlap a frame with its own offset from the text; below a frame in the flow it
                    // goes to its bottom at the nearest twip.
                    let fixed = matches!(o.float.v_relative.as_str(), "page" | "margin");
                    let offset = fixed || o.float.v_offset != 0.0 && !(a.allow_overlap && o.allow_overlap);
                    let y = match offset {
                        false => (o.rect[3] * 20.0).round() / 20.0,
                        true if a.allow_overlap && o.allow_overlap => continue,
                        true => ((o.rect[1] * 20.0 + ((o.rect[3] - o.rect[1]) * 20.0).round() - 1.0).round() / 20.0).max(a.rect[1]),
                    };
                    a.moved_to(a.rect[0], y);
                    under.push(o.key);
                }
            }
        }
        // Its area: a frame's anchor boxes; a drawing's the page across, down the page when placed from it and
        // the column otherwise.
        let across = match a.float.h_relative {
            _ if in_flow && compat == Compat::Generic => (f64::INFINITY, f64::NEG_INFINITY),
            HorizontalRelation::Margin if frame(&a) => (frames.margin[0], frames.page_w - frames.margin[1]),
            HorizontalRelation::Column if frame(&a) => (col.x, col.x + col.width),
            _ => (0.0, frames.page_w),
        };
        let others: Vec<Placed> = self.at.found.iter().filter(|o| !under.contains(&o.key)).cloned().collect();
        // Compat ≤ 14: once a float (not a frame) later on the page met the text laid out before it, the page
        // filled again keeps a frame where it was placed, unmoved.
        // So does a later frame in the flow that met the line above it.
        let restarted = by_frame
            || compat == Compat::Legacy
                && frame(&a)
                && self
                    .at
                    .known
                    .iter()
                    .any(|k| k.restarts && !frame(k) && k.wrap != Wrap::None && k.key.0 == a.key.0 && (k.key.1, k.key.2) > (a.key.1, a.key.2));
        let down = match a.float.v_relative.as_str() {
            "page" => (0.0, frames.page_h),
            "margin" if frame(&a) => (frames.margin[2], frames.page_h - frames.margin[3]),
            _ => (col.top.to_pt(), col.bottom.to_pt()),
        };
        if !restarted && !kept_at.contains(&a.key) && !float::avoid(&mut a, &others, across, down, compat) {
            let b = band_of(bands, a.key.2 - pb.from).unwrap_or(0);
            if compat == Compat::Generic && !(page_first && b == 0) {
                *push = Some(push.map_or(b, |x: usize| x.min(b)));
                return;
            }
            // Starting the page, it stays where it was asked for and the line goes below the page's floats.
            a.unplaced = compat == Compat::Generic;
            *page_end |= compat == Compat::Generic;
            *forced = Some(forced.map_or(b, |x: usize| x.min(b)));
        }
        // New in this fill, or meeting earlier text where the last fill didn't: the floats after it were
        // placed beside text that moves.
        if a.restarts && !self.at.known.iter().any(|k| k.same(&a) && (k.frozen || !a.frozen)) {
            self.at.hit = true;
        }
        if lead_only {
            carry.frames_left.push(a.key);
        }
        self.at.found.push(a);
    }

    // Lines that fit with their notes
    fn page_bands(
        &mut self,
        w: &WrapIn,
        bands: &[(f64, Vec<Line>)],
        area: &NoteArea,
        memos: &mut Memos,
    ) -> Result<(usize, Vec<(NoteArea, Vec<NotePart>)>, usize), Yield> {
        let &WrapIn { pb, style, col, sp, pieces, n, cx0, cx1, compat, floats, first_part, after, .. } = w;
        let nb = bands.len();
        let mut fit = 0;
        let mut areas = vec![(area.clone(), Vec::<NotePart>::new())];
        for (bi_, (bt, segs)) in bands.iter().enumerate() {
            let l = &segs[0];
            let room = l.pitch - l.below;
            let refs: Vec<(NoteKind, i64)> = segs.iter().flat_map(|l| (l.start..l.end).filter_map(|k| sp.note(pb.from + k))).collect();
            let (mut a, mut rest) = areas.last().cloned().unwrap_or_default();
            let lead = segs.len() == 1 && breaks_only(pieces, l) && l.end < n;
            let tail = if bi_ + 1 == nb { after } else { 0.0 };
            let note_tail = if refs.iter().any(|r| r.0 == NoteKind::Foot) { tail } else { 0.0 };
            if !self.add_notes(&refs, &mut a, col, bt + room + note_tail, &mut rest, false, memos)? && !lead {
                break;
            }
            let above =
                if self.ctx.env.text.compat == Compat::Legacy && style.line_rule == LineRule::Auto && style.line > 1.0 { room } else { l.pitch };
            if a.total() > 0.0 && !col.bottom.holds(bt + above + tail + a.total()) && !lead {
                break;
            }
            areas.push((a, rest));
            fit += 1;
            if segs.iter().any(|l| matches!(l.hard, Some(Kind::PageBreak | Kind::ColumnBreak))) {
                break;
            }
            if areas.last().is_some_and(|x| !x.1.is_empty()) {
                break;
            }
        }
        let hard = bands[..fit].iter().any(|(_, segs)| segs.iter().any(|l| matches!(l.hard, Some(Kind::PageBreak | Kind::ColumnBreak))));
        let mut take = lines_to_take(nb, fit, hard, first_part, style.keep_lines, style.widow_control);
        // Compat ≤ 14: widow and orphan control never moves a line set beside a wrapping float that ends in the body
        // (above the page's note area); the lines from the last such one up stay.
        if compat == Compat::Legacy && take < fit && !style.keep_lines {
            let bottom = col.bottom.to_pt() - area.total();
            let beside = |b: usize| {
                let (bt, segs) = &bands[b];
                let bb = bt + segs[0].pitch;
                floats.iter().chain(&self.at.found).any(|f| {
                    f.key.2 != float::UPRIGHT_TABLE
                        && f.wrap != Wrap::None
                        && f.excl[3] <= bottom + EPS
                        && f.excl[1] < bb - EPS
                        && f.excl[3] > *bt + EPS
                        && f.excl[0] < cx1 - EPS
                        && f.excl[2] > cx0 + EPS
                })
            };
            if let Some(b) = (take..fit).rev().find(|&b| beside(b)) {
                take = b + 1;
            }
        }
        Ok((fit, areas, take))
    }

    // Final take, notes and lines
    #[allow(clippy::too_many_arguments, reason = "the paragraph, its bands and what pagination took of them")]
    fn emit_wrapped(
        &mut self,
        w: &WrapIn,
        bands: &[(f64, Vec<Line>)],
        areas: &[(NoteArea, Vec<NotePart>)],
        take: usize,
        (s, more): (usize, Option<usize>),
        page_end: bool,
        cur: &mut ColumnCursor,
    ) -> Result<Stop, Yield> {
        let &WrapIn { p, pb, style, col, sp, first_part, top, anchor, after, .. } = w;
        let (pos, carry, y, empty, area, out) = cur.parts();
        let nb = bands.len();
        let (a, rest) = areas[take.min(areas.len() - 1)].clone();
        *area = a;
        let notes_continue = !rest.is_empty();
        carry.note_rest.extend(rest);
        self.at.starts.push((bands[0].0, first_part));
        self.record(p, first_part);
        let mut stop_kind = None;
        let mut end_y = top;
        let origin = LineOrigin {
            shaped: sp,
            from: pb.from,
            width: col.width,
            x0: col.x,
            compat: self.ctx.env.text.compat,
            para: anchor,
            review: &self.ctx.env.text.review,
            ul_trail_space: self.ctx.env.text.ul_trail_space,
            palette: crate::drawing::paint::Palette { theme: &self.ctx.env.text.theme, map: &self.ctx.env.text.colour_map },
            backdrop: self.ctx.env.text.backdrop,
            page_colour: self.ctx.env.text.page_colour,
            html_auto_spacing: !self.ctx.env.text.do_not_use_html_paragraph_auto_spacing,
            below: Below::To(0.0),
        };
        let bottom = bands[take - 1].0 + bands[take - 1].1[0].pitch;
        let mut lines = Vec::new();
        let mut numbers = Vec::new();
        for (bt, segs) in &bands[..take] {
            let origin = LineOrigin { below: Below::To(bottom - bt), ..origin };
            for l in segs {
                lines.push(PlacedLine { x: 0.0, y: *bt, line: Rc::new(draw_line(&origin, l, p, self.ctx.fonts)) });
                if matches!(l.hard, Some(Kind::PageBreak | Kind::ColumnBreak)) {
                    stop_kind = l.hard;
                }
            }
            self.line_number(carry, style, col, bt + segs[0].baseline, &mut numbers);
            end_y = bt + segs[0].pitch;
        }
        if take < nb || more.is_some() {
            for l in &mut lines {
                l.line = l.line.clipped(end_y - l.y);
            }
        }
        out.push(Child::Para(ParaFragment { para: anchor, lines: lines.into_boxed_slice(), line_numbers: numbers.into_boxed_slice() }));
        *y = end_y;
        *empty = false;
        carry.first_para = false;
        carry.prev = Some(p.para_style_in(self.ctx.env.text.review.view).clone());
        carry.balanced_after = 0.0;
        carry.prev_after = after;
        let after_break = more.filter(|_| take == nb).map(|part| (s, part));
        let done = take >= nb && after_break.is_none();
        if done {
            pos.next_block();
        } else {
            pos.at = match after_break {
                Some((s, part)) => At::Piece(pb.from + s, part),
                None => At::Piece(pb.from + bands[take].1[0].start, bands[take].1[0].part),
            };
            carry.letter_break = !take.checked_sub(1).and_then(|k| bands[k].1.last()).is_some_and(|l| l.hyphen);
        }
        if notes_continue && stop_kind.is_none() {
            carry.keep_top = false;
            return Ok(Stop::Full);
        }
        Ok(match stop_kind {
            Some(Kind::PageBreak) => {
                carry.keep_top = false;
                carry.prev = None;
                Stop::Page
            }
            Some(Kind::ColumnBreak) => {
                carry.keep_top = false;
                carry.prev = None;
                Stop::Column
            }
            _ if !done || page_end => {
                carry.keep_top = false;
                if page_end { Stop::Page } else { Stop::Full }
            }
            _ => Stop::Done,
        })
    }
}
