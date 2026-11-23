//! Paragraphs in a column

use crate::docx::Compat;
use crate::docx::model::*;
use crate::docx::props::LineRule;
use crate::layout::float::{self, Placed, Wrap};
use crate::layout::fragment::{Child, Item, ParaFragment, PlacedLine};
use crate::layout::job::{Memos, Yield};
use crate::layout::line::{ParaLines, break_lines, draw_label};
use crate::layout::paragraph::{gap, lines_to_take, space_after, space_before};
use crate::layout::space::{ColumnSpace, ParaSpace};
use crate::layout::text::{Kind, para_id};
use crate::layout::token::{At, FlowState, FlowToken, NoteArea, NotePart};
use crate::layout::units::EPS;

use super::{ColumnCursor, PageFill, Stop};

impl<'d> PageFill<'d, '_> {
    pub(super) fn line_number(&self, carry: &mut FlowState, style: &ParaStyle, col: &ColumnSpace, baseline: f64, out: &mut Vec<Box<[Item]>>) {
        let Some(ln) = self.sec.section.line_numbers.as_ref().filter(|_| !style.suppress_line_numbers) else { return };
        carry.line_no += 1;
        let n = ln.start + carry.line_no;
        if n % ln.count_by != 0 {
            return;
        }
        let ncols = self.sec.section.section_in(self.ctx.env.text.review.view).columns.num.max(1);
        let dist = if ln.distance > 0.0 {
            ln.distance
        } else if ncols > 1 {
            9.0
        } else {
            18.0
        };
        if let Some(run) = &self.ctx.env.line_number_run {
            let label = draw_label(
                &n.to_string(),
                run,
                col.x - dist,
                baseline,
                (
                    self.ctx.env.text.page_colour,
                    crate::drawing::paint::Palette { theme: &self.ctx.env.text.theme, map: &self.ctx.env.text.colour_map },
                ),
                self.ctx.fonts,
            );
            if !label.is_empty() {
                out.push(label.into_boxed_slice());
            }
        }
    }

    // Paragraphs
    pub(super) fn place_para(
        &mut self,
        p: &Paragraph,
        pb: &ParaLines,
        cur: &mut ColumnCursor,
        col: &ColumnSpace,
        memos: &mut Memos,
    ) -> Result<Stop, Yield> {
        let (pos, carry, y, empty, _, _) = cur.parts();
        let has_anchor = pb.pieces().iter().any(|q| q.kind == Kind::Anchor);
        let bottom = *y + self.para_gap(p, pos, carry, *empty) + pb.lines.iter().map(|l| l.pitch).sum::<f64>();
        let text_bottom = match self.ctx.env.text.compat {
            Compat::Generic => bottom - pb.lines.last().map_or(0.0, |l| l.below),
            Compat::Legacy => bottom + self.after_kept(pos.block, p.para_style_in(self.ctx.env.text.review.view)),
        };
        let affected = self.fixed.iter().chain(self.known_at(pos.block)).any(|f| {
            let early = f.early_box.filter(|_| f.key.0 == self.sec.index && f.key.1 > pos.block);
            let f = &Placed { excl: early.map_or(f.excl, |b| b.1), ..f.clone() };
            let reach = if f.key.2 == float::TABLE_PIECE || self.ctx.env.text.compat == Compat::Legacy { text_bottom } else { bottom };
            f.wrap != Wrap::None && f.excl[3] > *y + EPS && f.excl[1] < reach - EPS && f.excl[0] < col.x + col.width - EPS && f.excl[2] > col.x + EPS
        });
        if has_anchor || affected {
            return self.place_wrapped(p, pb, cur, col, memos);
        }
        self.place_plain(p, pb, cur, col, memos)
    }

    pub(super) fn place_plain(
        &mut self,
        p: &Paragraph,
        pb: &ParaLines,
        cur: &mut ColumnCursor,
        col: &ColumnSpace,
        memos: &mut Memos,
    ) -> Result<Stop, Yield> {
        let (pos, carry, y, empty, area, out) = cur.parts();
        let style = p.para_style_in(self.ctx.env.text.review.view);
        let first_part = pos.at.at_start();
        let before = self.own_before(style, pos.block, carry);
        let after = space_after(&self.ctx.env.text, style);
        let n = pb.lines.len();
        let at_top = *empty;
        let g = if !first_part {
            std::mem::take(&mut carry.break_before)
        } else if at_top {
            if carry.keep_top || carry.first_para || self.keeps_top_space() {
                let after = if pos.block == 0 { carry.prev_after.max(carry.balanced_after) } else { carry.prev_after };
                (crate::layout::paragraph::gap_plain(&self.ctx.env.text, after, before) - after).max(0.0)
            } else {
                0.0
            }
        } else {
            match &carry.prev {
                Some(ps) => gap(&self.ctx.env.text, ps, carry.prev_after, style, before),
                None => self.edge_before(pos.block, style, before),
            }
        };
        let top = *y + g - if at_top { self.top_lift(style, pb) } else { 0.0 };
        let (fit, areas, mut take) = self.fit_plain(pb, style, area, col, top, first_part, at_top, after, memos)?;
        if let Some((b, cap)) = self.at.keep_cap
            && b == pos.block
            && first_part
        {
            self.at.keep_cap = None;
            take = take.min(cap);
        }
        if take == 0 {
            // Under notes going on from an earlier column or page that leave no room for it, the line waits (in
            // compat 15 a balancing trial fails instead). So does a line with a footnote reference above an earlier
            // section's closed footnote area, and a section's first line below an earlier section that leaves no
            // room for it on the page.
            let under = area.carried > 0 && (!col.balance || self.ctx.env.text.compat == Compat::Legacy);
            let waits = fit == 0 && (under || self.notes_closed || self.mid_page);
            if at_top && !waits {
                self.at.forced = self.at.forced || col.balance;
                take = fit.max(1).min(n);
            } else {
                return Ok(Stop::Full);
            }
        }
        self.at.starts.push((top, first_part));
        self.record(p, first_part);
        let mut notes_continue = false;
        if let Some((_, a, rest)) = areas.into_iter().rev().find(|e| e.0 <= take) {
            // A note waiting whole for the next page is numbered on this one.
            for r in rest.iter().filter(|r| !a.parts.iter().any(|p| (p.kind, p.id) == (r.kind, r.id))) {
                self.at.effects.notes_shown.push((r.kind, r.id, self.note_numbers.get(&(r.kind, r.id)).cloned().unwrap_or_default()));
            }
            *area = a;
            notes_continue = !rest.is_empty();
            carry.note_rest.extend(rest);
        }
        // Compat 15 with automatic hyphenation: the line that ends the page or column is broken again without it.
        let text = &self.ctx.env.text;
        let last = match &pb.lines[take - 1] {
            l if text.bottom_unhyphenated && text.hyphenation.is_some() && take < n && l.hyphen && !pb.pieces()[l.end - 1].soft_hyphen => {
                let env = crate::layout::env::TextEnv { hyphenation: None, ..text.clone() };
                let space = ParaSpace {
                    width: pb.width,
                    start: pb.from + l.start,
                    part: l.part,
                    borders: pb.borders.clone(),
                    x0: pb.x0,
                    env,
                    fit: false,
                    letter_break: false,
                    shade_gap: pb.shade_gap,
                };
                Some(break_lines(&pb.shaped, p, &space, self.ctx.fonts, memos)?)
            }
            _ => None,
        };
        let mut yy = top;
        let mut stop_kind = None;
        let mut lines = Vec::with_capacity(take);
        let mut numbers = Vec::new();
        for (i, l) in pb.lines[..take].iter().enumerate() {
            let (l, line) = match &last {
                Some(a) if i + 1 == take => (&a.lines[0], a.fragment(0, p, self.ctx.fonts)),
                _ => (l, pb.fragment(i, p, self.ctx.fonts)),
            };
            lines.push(PlacedLine { x: 0.0, y: yy, line });
            self.show_endnote_refs(pb.notes(i));
            self.line_number(carry, style, col, yy + l.baseline, &mut numbers);
            yy += l.pitch;
            if matches!(l.hard, Some(Kind::PageBreak | Kind::ColumnBreak)) {
                stop_kind = l.hard;
            }
        }
        if take < n {
            // The paragraph goes on elsewhere: an inline picture's reflection stops where this part does.
            for l in &mut lines {
                l.line = l.line.clipped(yy - l.y);
            }
        }
        out.push(Child::Para(ParaFragment { para: para_id(p), lines: lines.into_boxed_slice(), line_numbers: numbers.into_boxed_slice() }));
        *y = yy;
        *empty = false;
        carry.first_para = false;
        carry.prev = Some(p.para_style_in(self.ctx.env.text.review.view).clone());
        carry.balanced_after = 0.0;
        let prev_after = std::mem::replace(&mut carry.prev_after, after);
        let done = take >= n;
        if done {
            pos.next_block();
        } else {
            pos.at = match &last {
                Some(a) => At::Piece(a.from + a.lines[0].end, a.lines.get(1).map_or(pb.lines[take].part, |l| l.part)),
                None => At::Piece(pb.from + pb.lines[take].start, pb.lines[take].part),
            };
            carry.letter_break = !last.as_ref().map_or(&pb.lines[take - 1], |a| &a.lines[0]).hyphen;
        }
        if notes_continue && stop_kind.is_none() && !(self.hold_notes && self.at_section_end(pos)) {
            carry.keep_top = false;
            return Ok(Stop::Full);
        }
        let starts_with_break = first_part
            && take == 1
            && !done
            && pb.pieces()[pb.lines[0].start..pb.lines[0].end]
                .iter()
                .all(|q| matches!(q.kind, Kind::PageBreak | Kind::ColumnBreak | Kind::Anchor | Kind::ListNumber));
        Ok(match stop_kind {
            Some(Kind::PageBreak) => {
                carry.keep_top = false;
                carry.prev = None;
                let kept = (crate::layout::paragraph::gap_plain(&self.ctx.env.text, prev_after, before) - prev_after).max(0.0);
                carry.break_before = if starts_with_break && !self.ctx.env.text.suppress_before_after_page_break { kept } else { 0.0 };
                Stop::Page
            }
            Some(Kind::ColumnBreak) => {
                carry.keep_top = false;
                carry.prev = None;
                carry.break_before = if starts_with_break { before } else { 0.0 };
                Stop::Column
            }
            _ if !done => {
                carry.keep_top = false;
                Stop::Full
            }
            _ => Stop::Done,
        })
    }

    // Lines that fit with their notes
    #[allow(clippy::too_many_arguments, reason = "the paragraph's lines, the note area and where the lines start")]
    fn fit_plain(
        &mut self,
        pb: &ParaLines,
        style: &ParaStyle,
        area: &NoteArea,
        col: &ColumnSpace,
        top: f64,
        first_part: bool,
        at_top: bool,
        after: f64,
        memos: &mut Memos,
    ) -> Result<(usize, Vec<(usize, NoteArea, Vec<NotePart>)>, usize), Yield> {
        let n = pb.lines.len();
        let wc = style.widow_control;
        // Compat ≤ 14: when the lines can't stay on the page with a line's notes after a note placed whole, those
        // notes wait for the next page and the lines stay.
        let mut force: Option<usize> = None;
        let fitted = loop {
            let mut yy = top;
            let mut fit = 0;
            // The note area and the notes waiting after each line that changes them: (lines, area, waiting); `area`
            // before the first.
            let mut areas: Vec<(usize, NoteArea, Vec<NotePart>)> = Vec::new();
            // After a note waits for the next page, the lines its paragraph can't be parted from there.
            let mut more: Option<usize> = None;
            let mut first_ref = None;
            for (i, l) in pb.lines.iter().enumerate() {
                let room = l.pitch - l.below;
                let refs = pb.notes(i);
                let foot = refs.iter().any(|r| r.0 == NoteKind::Foot);
                if more.is_some() && foot {
                    break;
                }
                let last = areas.last().map_or(area, |e| &e.1);
                if first_ref.is_none() && foot && last.parts.len() > last.carried {
                    first_ref = Some(i);
                }
                let defer = i == 0 || first_part && wc && (i == 1 || n <= 3);
                // The last line's space after comes off the room for its notes (a line without notes fits without it).
                let tail = if i + 1 == n { after } else { 0.0 };
                let note_tail = if foot { tail } else { 0.0 };
                // Above a footnote area compat 15 tests the line's full pitch, compat ≤ 14 its height without the room
                // multiple spacing adds below it.
                let above =
                    if self.ctx.env.text.compat == Compat::Legacy && style.line_rule == LineRule::Auto && style.line > 1.0 { room } else { l.pitch };
                let over = |a: &NoteArea| a.total() > 0.0 && !col.bottom.holds(yy + above + tail + a.total()) && !pb.leading_page_break(i);
                if !foot && force != Some(i) {
                    // No footnotes: the area stays as it is.
                    if !col.bottom.holds(yy + room + last.total()) && !pb.leading_page_break(i) || over(last) {
                        break;
                    }
                } else {
                    let state = || areas.last().map_or_else(|| (area.clone(), Vec::new()), |e| (e.1.clone(), e.2.clone()));
                    let (mut a, mut rest) = state();
                    if force == Some(i) {
                        for &(k, id) in refs.iter().filter(|r| r.0 == NoteKind::Foot) {
                            rest.push(NotePart { kind: k, id, units: self.note_units(k, id, col.width, memos)?.units.clone() });
                        }
                    } else if !self.add_notes(&refs, &mut a, col, yy + room + note_tail, &mut rest, defer, memos)? && !pb.leading_page_break(i) {
                        break;
                    }
                    let waits = |a: &NoteArea, rest: &[NotePart]| rest.iter().any(|r| !a.parts.iter().any(|p| (p.kind, p.id) == (r.kind, r.id)));
                    if over(&a) && defer {
                        // The notes fit only without the line's full height: in compat ≤ 14 they may wait instead.
                        let (mut a2, mut rest2) = state();
                        if self.add_notes(&refs, &mut a2, col, yy + l.pitch + tail, &mut rest2, defer, memos)? && waits(&a2, &rest2) {
                            (a, rest) = (a2, rest2);
                        }
                    }
                    if over(&a) {
                        break;
                    }
                    areas.push((i + 1, a, rest));
                }
                yy += l.pitch;
                fit += 1;
                if matches!(l.hard, Some(Kind::PageBreak | Kind::ColumnBreak)) {
                    break;
                }
                if let Some((_, a, rest)) = areas.last().filter(|x| !x.2.is_empty()) {
                    let waits = |a: &NoteArea, rest: &[NotePart]| rest.iter().any(|r| !a.parts.iter().any(|p| (p.kind, p.id) == (r.kind, r.id)));
                    if more.is_none() && waits(a, rest) {
                        more = Some(match (first_part && wc, n <= 3) {
                            (true, true) => n - 1 - i,
                            (true, false) if i == 0 || i + 2 == n => 1,
                            _ => 0,
                        });
                    }
                    match more {
                        Some(m) if m > 0 => more = Some(m - 1),
                        _ => break,
                    }
                }
            }
            let hard = pb.lines[..fit.min(n)].iter().any(|l| matches!(l.hard, Some(Kind::PageBreak | Kind::ColumnBreak)));
            let take_of = |fit| lines_to_take(n, fit, hard, first_part, style.keep_lines, style.widow_control);
            let take = take_of(fit);
            // Compat 15: when the line that ends the page or column after widow and orphan control ends at a
            // hyphenation point, it goes on to the next one, and widow and orphan control apply once more.
            let (fit, take) = if self.ctx.env.text.bottom_hyphen_moves && take > 0 && take < n && !hard && pb.lines[take - 1].hyphen {
                (take - 1, take_of(take - 1))
            } else {
                (fit, take)
            };
            if take == 0 && force.is_none() && wc && !at_top && self.ctx.env.text.compat == Compat::Legacy && first_ref.is_some() {
                force = first_ref;
                continue;
            }
            break (fit, areas, take);
        };
        Ok(fitted)
    }

    pub(super) fn para_gap(&self, p: &Paragraph, pos: &FlowToken, carry: &FlowState, empty: bool) -> f64 {
        let style = p.para_style_in(self.ctx.env.text.review.view);
        let before = self.own_before(style, pos.block, carry);
        if !pos.at.at_start() {
            0.0
        } else if empty {
            if carry.keep_top || carry.first_para || self.keeps_top_space() {
                let after = if pos.block == 0 { carry.prev_after.max(carry.balanced_after) } else { carry.prev_after };
                (crate::layout::paragraph::gap_plain(&self.ctx.env.text, after, before) - after).max(0.0)
            } else {
                0.0
            }
        } else {
            match &carry.prev {
                Some(ps) => gap(&self.ctx.env.text, ps, carry.prev_after, style, before),
                None => self.edge_before(pos.block, style, before),
            }
        }
    }

    /// The last block of the paragraphs joined from `bi` by deleted marks when they show nothing and a table follows
    /// them (or they end the body): they aren't laid out.
    pub(super) fn deleted_before_table(&self, bi: usize) -> Option<usize> {
        let review = &self.ctx.env.text.review;
        let end = self.sec.blocks.chain_end(bi, review);
        let shows_nothing = |k: usize| matches!(self.sec.blocks.get(k), Some(Block::Para(p)) if crate::layout::paragraph::shows_nothing(p, review));
        let last = matches!(self.sec.blocks.get(end), Some(Block::Para(p)) if review.hides(&p.mark.rev));
        let next = self.sec.blocks.get(end + 1);
        let own = self.sec.section.blocks.len();
        let story_end = match end + 1 {
            k if k == own => !self.sec.breaks,
            k => k > own && (k == self.sec.blocks.len() || self.sec.blocks.stories.contains(&k)),
        };
        (last && (story_end || matches!(next, Some(Block::Table(_)))) && (bi..=end).all(shows_nothing)).then_some(end)
    }

    /// The space before of the paragraph whose mark the hidden marks ending at `end` join, when a table whose rows are
    /// all hidden follows them (else 0).
    pub(super) fn hidden_table_before(&self, end: usize) -> f64 {
        let view = self.ctx.env.text.review.view;
        let blocks = &self.sec.blocks;
        if !matches!(blocks.get(end + 1), Some(Block::Table(t)) if crate::layout::join::table_hidden(t, &self.ctx.env.text.review)) {
            return 0.0;
        }
        blocks.next_shown_mark(end + 1, &self.ctx.env.text.review).map_or(0.0, |q| space_before(&self.ctx.env.text, q.para_style_in(view)))
    }

    /// Paragraph `bi`'s own space before.
    pub(super) fn own_before(&self, style: &ParaStyle, bi: usize, carry: &FlowState) -> f64 {
        // Compat 15: the empty closing paragraph a table leaves is laid out without its space before.
        if style.before_auto && carry.first_para || self.break_para_after_table(bi) {
            return 0.0;
        }
        space_before(&self.ctx.env.text, style)
    }

    /// Whether block `bi` is the section's closing paragraph and holds nothing but its mark (bookmarks and empty runs
    /// don't count), after a block of kind `prev`.
    fn bare_closing_para(&self, bi: usize, prev: fn(&Block) -> bool) -> bool {
        let bare = |i: &Inline| match i {
            Inline::Text(t, _) => t.is_empty(),
            Inline::Bookmark(_)
            | Inline::Opaque(_)
            | Inline::Range(_)
            | Inline::ControlStart(_)
            | Inline::ControlEnd(_)
            | Inline::CommentRef { .. } => true,
            _ => false,
        };
        self.sec.breaks
            && bi + 1 == self.sec.section.blocks.len()
            && bi > 0
            && self.sec.blocks.get(bi - 1).is_some_and(prev)
            && matches!(self.sec.blocks.get(bi), Some(Block::Para(p)) if p.inlines.iter().all(bare))
    }

    pub fn break_para_after_table(&self, bi: usize) -> bool {
        self.bare_closing_para(bi, |b| matches!(b, Block::Table(_)))
    }

    pub fn break_para_after_para(&self, bi: usize) -> bool {
        self.bare_closing_para(bi, |b| matches!(b, Block::Para(_)))
    }

    /// `suppressSpacingAtTopOfPage` below the first column: the space before stays and no line rises.
    fn keeps_top_space(&self) -> bool {
        self.ctx.env.text.suppress_spacing_at_top && self.column > 0
    }

    /// How far a line at the top of a page (or of a later column of a section that starts at the page top) rises:
    /// with `suppressSpacingAtTopOfPage` (compat ≤ 14) an at-least line by its at-least height beyond its text; with
    /// `suppressTopSpacing`, an exact or at-least line of an unbordered, unshaded paragraph by its line height beyond
    /// 9.6pt.
    pub(super) fn top_lift(&self, style: &ParaStyle, pb: &ParaLines) -> f64 {
        let env = &self.ctx.env.text;
        let Some(l) = pb.lines.first().filter(|_| !self.mid_page) else { return 0.0 };
        if env.suppress_spacing_at_top {
            return if style.line_rule == LineRule::AtLeast && !self.keeps_top_space() { (style.line - l.ascent - l.descent).max(0.0) } else { 0.0 };
        }
        let framed = [&style.border_top, &style.border_bottom, &style.border_left, &style.border_right]
            .iter()
            .any(|b| b.as_ref().is_some_and(|b| b.visible()))
            || style.shading.is_some();
        if !env.suppress_top_spacing || framed {
            return 0.0;
        }
        let line = match style.line_rule {
            LineRule::AtLeast => style.line,
            // An exact line counts four fifths of its height less its first character's raise (`w:position`).
            LineRule::Exact => {
                let raise = pb.pieces()[l.start..l.end]
                    .iter()
                    .find(|q| q.kind == Kind::Text)
                    .map_or(0.0, |q| pb.shaped.styles[q.style as usize].run.position);
                let twips = (style.line.abs() * 20.0).round() as i64;
                (twips * 4 / 5 - 10 * (raise * 2.0).round() as i64) as f64 / 20.0
            }
            LineRule::Auto => return 0.0,
        };
        (line - 9.6).max(0.0)
    }

    pub(super) fn edge_before(&self, bi: usize, style: &ParaStyle, before: f64) -> f64 {
        let after_table = bi > 0 && matches!(self.sec.blocks.get(bi - 1), Some(Block::Table(_)));
        if after_table && crate::layout::paragraph::drops_at_edge(style, Some(self.ctx.env.text.default_style.as_deref())) { 0.0 } else { before }
    }

    pub(super) fn after_above_table(&self, bi: usize, carry: &FlowState) -> f64 {
        let first = match self.sec.blocks.get(bi) {
            Some(Block::Table(t)) => t.rows.first().and_then(|r| r.cells.first()).and_then(|c| match c.blocks.first() {
                Some(Block::Para(p)) => Some(p.para_style_in(self.ctx.env.text.review.view).style_id.as_deref()),
                _ => None,
            }),
            _ => None,
        };
        if carry.prev.as_ref().is_some_and(|ps| crate::layout::paragraph::drops_at_edge(ps, first)) { 0.0 } else { carry.prev_after }
    }
}
