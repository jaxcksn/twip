//! Column filling

use crate::docx::Compat;
use std::collections::HashMap;
use std::rc::Rc;

use crate::docx::model::*;

use super::LayoutCtx;
use super::float::Placed;
use super::fragment::Child;
use super::job::{Memos, Yield};
use super::join::{Joined, LaidPara, laid_para};
use super::line::{ParaLines, break_lines, para_space};
use super::node::{Notes, SectionNode};
use super::paragraph::{gap, space_after, space_before};
use super::record::PageEffects;
use super::space::{ColumnSpace, ShapeSpace};
use super::story::{StoryEnv, StoryUnit};
use super::table::{DrawnRow, TableLayout};
use super::text::{FieldValues, Kind, para_id, shape};
use super::token::{FlowState, FlowToken, NoteArea};
use super::units::EPS;

mod notes;
mod para;
mod tables;
mod upright;
mod wrapped;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Stop {
    Done,
    Full,
    Column,
    Page,
}

pub struct ColumnResult {
    pub children: Vec<Child>,
    pub stop: Stop,
    pub bottom: f64,
    pub token: FlowToken,
    pub state: FlowState,
}

#[derive(Default)]
pub struct PassMemo {
    tables: HashMap<(usize, u64), Rc<TableLayout>>,
    varies: HashMap<usize, bool>,
    joined: HashMap<(usize, usize), Rc<Joined>>,
    mirrored: HashMap<(usize, usize), LaidPara<'static>>,
    restyled: HashMap<(usize, usize), LaidPara<'static>>,
    rewrapped: HashMap<(NoteKind, i64, usize, usize, u64), Vec<StoryUnit>>,
    pub(crate) held: Held,
}

#[derive(Clone, Debug, Default)]
pub struct Held {
    lines: Vec<Rc<ParaLines>>,
    tables: Vec<Rc<TableLayout>>,
    rows: Vec<Rc<DrawnRow>>,
}

fn hold<T>(v: &mut Vec<Rc<T>>, x: &Rc<T>) {
    if !v.iter().any(|y| Rc::ptr_eq(y, x)) {
        v.push(x.clone());
    }
}

impl PassMemo {
    pub fn table(&self, t: &Table, width: f64) -> Option<Rc<TableLayout>> {
        self.tables.get(&(std::ptr::from_ref(t) as usize, width.to_bits())).cloned()
    }

    pub fn seed(&mut self, t: &Table, width: f64, tb: Rc<TableLayout>) {
        self.tables.insert((std::ptr::from_ref(t) as usize, width.to_bits()), tb);
    }
}

fn varies_by_page(blocks: &[Block]) -> bool {
    blocks.iter().any(|b| match b {
        Block::Para(p) => p.inlines.iter().any(|i| {
            matches!(i, Inline::FieldBegin { kind: Some(FieldKind::Page | FieldKind::PgNum | FieldKind::Section | FieldKind::SectionPages), .. })
        }),
        Block::Table(t) => t.rows.iter().any(|r| r.cells.iter().any(|c| varies_by_page(&c.blocks))),
    })
}

#[derive(Clone, Default)]
pub struct AttemptState {
    pub known: Vec<Placed>,
    pub found: Vec<Placed>,
    pub stream: Option<NoteArea>,
    pub forced: bool,
    /// A float placed in this fill met text laid out before it where the last fill didn't have it: the floats after
    /// it were placed beside text that moves.
    pub hit: bool,
    pub starts: Vec<(f64, bool)>,
    pub notes_drawn: bool,
    /// The bottom of the column footnote areas drawn under their text (`beneathText`).
    pub notes_bottom: f64,
    /// The footnote position of the page's area when an earlier section on the page started it.
    pub beneath: Option<bool>,
    pub effects: PageEffects,
    pub keep_cap: Option<(usize, usize)>,
    pub end: Option<FlowToken>,
    /// Tables of a vertical section, drawn upright on the page as printed.
    pub upright: Vec<Child>,
    /// The paragraph being placed starts the next page: only the frames before it that stay on this page are.
    pub frames_only: bool,
}

pub struct PageFill<'d, 'p> {
    pub ctx: LayoutCtx<'d>,
    pub sec: &'p SectionNode<'d>,
    pub notes: Notes<'d>,
    pub fields: &'p FieldValues,
    pub note_numbers: &'p HashMap<(NoteKind, i64), String>,
    pub page_w: f64,
    pub page_h: f64,
    pub margins: (f64, f64, f64),
    pub pass: &'p mut PassMemo,
    pub at: AttemptState,
    pub fixed: Vec<Placed>,
    pub odd: bool,
    /// Paragraphs with `w:mirrorIndents` mirror on this page.
    pub mirror: bool,
    /// The section's footnote area passes to the next section on the page when the section ends.
    pub hold_notes: bool,
    /// The section starts below the page top.
    pub mid_page: bool,
    /// The page's footnote area is closed: a line with a footnote reference goes to the next page.
    pub notes_closed: bool,
    /// The column being filled.
    pub column: usize,
}

pub fn rebase(units: &[StoryUnit]) -> Vec<StoryUnit> {
    let first = units.first().map_or(0.0, |u| u.top);
    units.iter().map(|x| StoryUnit { top: x.top - first, ..x.clone() }).collect()
}

pub fn place_units(units: &[StoryUnit], x: f64, y: f64, out: &mut Vec<Child>) {
    for u in units {
        out.push(Child::Group { offset: (x, y + u.top), children: u.content.clone() });
    }
}

impl<'d> PageFill<'d, '_> {
    fn laid(&mut self, bi: usize) -> Option<(LaidPara<'d>, usize)> {
        let (laid, end) = self.laid_own(bi)?;
        let view = self.ctx.env.text.review.view;
        let style = laid.para().para_style_in(view);
        if !self.mirror || !style.mirror_indents || style.frame.is_some() {
            return Some((laid, end));
        }
        let key = (std::ptr::from_ref(laid.para()) as usize, end - bi);
        if let Some(m) = self.pass.mirrored.get(&key) {
            return Some((m.clone(), end));
        }
        // A mirrored page: the right indent takes the first line's start, the left the right indent (plus a hanging
        // indent); the first-line indent stays.
        let mut s = (**style).clone();
        (s.ind_left, s.ind_right) = (style.ind_right + (-style.ind_first).max(0.0), style.ind_left + style.ind_first);
        s.change = None;
        let m = laid.restyled(Rc::new(s));
        self.pass.mirrored.insert(key, m.clone());
        Some((m, end))
    }

    fn laid_own(&mut self, bi: usize) -> Option<(LaidPara<'d>, usize)> {
        let (laid, end) = self.laid_chain(bi)?;
        let view = self.ctx.env.text.review.view;
        let Some(Block::Para(last)) = self.sec.blocks.get(end) else { return Some((laid, end)) };
        if !view.hides(&last.mark.rev) {
            return Some((laid, end));
        }
        // A hidden last mark takes the next shown mark's properties; at the end of an endnote the story's own final
        // mark's.
        let key = (std::ptr::from_ref(laid.para()) as usize, end - bi);
        if let Some(r) = self.pass.restyled.get(&key) {
            return Some((r.clone(), end));
        }
        let blocks = &self.sec.blocks;
        let story_end = blocks.stories.iter().copied().find(|&s| s > end).unwrap_or(blocks.len());
        let in_note = blocks.stories.first().is_some_and(|&s| bi >= s);
        let last = if in_note { self.ctx.env.normal.as_ref() } else { None };
        let Some(style) = super::join::joined_target(&blocks.blocks[..story_end], end, view, last) else { return Some((laid, end)) };
        let r = laid.restyled(style.clone());
        self.pass.restyled.insert(key, r.clone());
        Some((r, end))
    }

    fn laid_chain(&mut self, bi: usize) -> Option<(LaidPara<'d>, usize)> {
        let Some(Block::Para(p)) = self.sec.blocks.get(bi) else { return None };
        let end = self.sec.blocks.chain_end(bi, self.ctx.env.text.review.view);
        if end == bi {
            return Some((LaidPara::One(p), bi));
        }
        let key = (std::ptr::from_ref(p) as usize, end - bi);
        let j = match self.pass.joined.get(&key) {
            Some(j) => j.clone(),
            None => {
                let Some(LaidPara::Joined(j)) = laid_para(&self.sec.blocks.blocks[bi..=end]) else { return Some((LaidPara::One(p), bi)) };
                self.pass.joined.insert(key, j.clone());
                j
            }
        };
        Some((LaidPara::Joined(j), end))
    }

    #[allow(clippy::too_many_arguments, reason = "the paragraph, its blocks, and the line space it is laid out in")]
    fn para_lines(
        &mut self,
        laid: &LaidPara,
        bi: usize,
        end: usize,
        width: f64,
        (from, letter_break): (usize, bool),
        x0: f64,
        memos: &mut Memos,
    ) -> Result<Rc<ParaLines>, Yield> {
        let p = laid.para();
        let view = self.ctx.env.text.review.view;
        let (prev, next) = self.sec.blocks.neighbours(bi, end, view);
        let style = p.para_style_in(view);
        let breaks_first = |b: Option<&Block>| {
            let Some(Block::Para(q)) = b else { return false };
            let mut shown = q.inlines.iter().filter(|i| !matches!(i, Inline::Bookmark(..) | Inline::Range(..)));
            matches!(shown.next(), Some(Inline::Break(crate::docx::model::BreakKind::Page | crate::docx::model::BreakKind::Column, _)))
        };
        let prev = prev.filter(|_| !breaks_first(self.sec.blocks.get(bi)));
        let next = next.filter(|_| !breaks_first(self.sec.blocks.get(end + 1)));
        let next = next.or_else(|| self.sec.next.filter(|_| end + 1 >= self.sec.blocks.len()));
        let mut space = laid.shape_space(self.fields, self.note_numbers, &self.ctx, memos)?;
        space.ends_section = self.sec.breaks && end + 1 == self.sec.section.blocks.len();
        if space.stale_fit && self.sec.index == 0 && bi == 0 && p.inlines.iter().any(|i| i.run().is_some_and(|r| r.fit_text.is_some())) {
            // The document's first line as broken with its regions unadjusted: real tab stops and list number.
            let probe = ShapeSpace { unfitted: true, ..space.clone() };
            let shaped = shape(p, &probe, self.ctx.fonts, memos)?;
            let lines = break_lines(&shaped, p, &para_space(style, prev, next, width, 0, x0, &self.ctx), self.ctx.fonts, memos)?;
            space.first_line = Some(lines.lines.first().map_or(0, |l| l.end));
        }
        let shaped = shape(p, &space, self.ctx.fonts, memos)?;
        let mut ps = para_space(style, prev, next, width, from, x0, &self.ctx);
        ps.letter_break = from > 0 && letter_break;
        if from > 0 && shaped.pieces[..from.min(shaped.pieces.len())].iter().all(|q| matches!(q.kind, Kind::PageBreak | Kind::ColumnBreak)) {
            ps.borders.0 = super::line::neighbour_borders(style, prev, None).0;
        }

        let lines = break_lines(&shaped, p, &ps, self.ctx.fonts, memos)?;
        hold(&mut self.pass.held.lines, &lines);
        Ok(lines)
    }

    pub(super) fn table_layout(&mut self, bi: usize, width: f64, memos: &mut Memos) -> Result<Rc<TableLayout>, Yield> {
        let Some(Block::Table(t)) = self.sec.blocks.get(bi) else { unreachable!("table_layout on a paragraph block") };
        let id = std::ptr::from_ref(t) as usize;
        let varies = *self.pass.varies.entry(id).or_insert_with(|| t.rows.iter().any(|r| r.cells.iter().any(|c| varies_by_page(&c.blocks))));
        if !varies && let Some(tb) = self.pass.tables.get(&(id, width.to_bits())) {
            hold(&mut self.pass.held.tables, tb);
            return Ok(tb.clone());
        }
        let env = StoryEnv { ctx: self.ctx, fields: self.fields, notes: self.note_numbers };
        let prev = self.sec.blocks.neighbours(bi, bi, self.ctx.env.text.review.view).0;
        let tb = crate::prof::time("table", || super::table::layout(t, width, &env, prev, false, memos))?;
        hold(&mut self.pass.held.tables, &tb);
        if !varies {
            self.pass.tables.insert((id, width.to_bits()), tb.clone());
        }
        Ok(tb)
    }

    // Column fill
    pub fn fill(
        &mut self,
        token: &FlowToken,
        state: &FlowState,
        col: ColumnSpace,
        page_empty: bool,
        memos: &mut Memos,
    ) -> Result<ColumnResult, Yield> {
        let (mut pos, mut carry, mut out) = (token.clone(), state.clone(), Vec::new());
        let (stop, bottom) = if let Some(mut area) = self.at.stream.take() {
            if area.sep == 0.0 && area.shared.is_some_and(|r| r > 0.0) {
                area.sep = self.separator(false, col.width, memos)?.height;
            }
            if self.column == 0 && area.parts.is_empty() && !carry.note_rest.is_empty() {
                self.continue_stream(&mut carry, &col, &mut area, memos)?;
            }
            let r = self.fill_body(&mut pos, &mut carry, col, page_empty, &mut area, &mut out, memos);
            self.at.stream = Some(area);
            r?
        } else {
            let mut area = NoteArea::default();
            let received = carry.page_notes.is_some();
            if let Some((held, beneath)) = carry.page_notes.take() {
                area = held;
                self.at.beneath = Some(beneath);
            }
            self.continue_notes(&mut carry, &col, &mut area, memos)?;
            // Compat ≤ 14, a balanced column: under a continuation that goes on past it the column holds only the note.
            let r = if col.balance && self.ctx.env.text.compat == Compat::Legacy && !carry.note_rest.is_empty() {
                (Stop::Full, col.top.to_pt())
            } else {
                self.fill_body(&mut pos, &mut carry, col, page_empty, &mut area, &mut out, memos)?
            };
            let text = r.1 + if pos.at.at_start() { carry.prev_after } else { 0.0 };
            // Compat 15: the first section on the page with a footnote reference (on this page or an earlier one)
            // gives the page's footnote area its columns, notes on this page or not.
            let starts_area = || !area.parts.is_empty() || received || self.sec.blocks.blocks.iter().any(|b| super::node::has_footnote_refs(b));
            if r.0 == Stop::Done && self.hold_notes && starts_area() {
                if area.cols.is_empty() {
                    area.cols = vec![(col.x, col.width)];
                }
                carry.page_notes = Some((area, self.beneath()));
            } else {
                self.draw_notes(&area, &col, text, &mut out, memos)?;
            }
            r
        };
        Ok(ColumnResult { children: out, stop, bottom, token: pos, state: carry })
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "the column fill passes its state (flow position, carried state, column, y, emptiness, note area, output) explicitly, so what each step reads and changes stays visible"
    )]
    fn fill_body(
        &mut self,
        pos: &mut FlowToken,
        carry: &mut FlowState,
        col: ColumnSpace,
        page_empty: bool,
        area: &mut NoteArea,
        out: &mut Vec<Child>,
        memos: &mut Memos,
    ) -> Result<(Stop, f64), Yield> {
        let nblocks = self.sec.blocks.len();
        let mut y = col.top.to_pt();
        let mut empty = true;
        if let Some(mut t) = carry.table_rest.take() {
            let tb = self.table_layout(t.block, col.width, memos)?;
            let (prev, prev_after) = (carry.prev.clone(), carry.prev_after);
            let (mut ty, mut te) = (y, empty);
            if self.place_table(&tb, &mut t, carry, &col, &mut ty, &mut te, area, out, memos)? != Stop::Done {
                carry.table_rest.get_or_insert(t);
            }
            (carry.prev, carry.prev_after) = (prev, prev_after);
        }
        if page_empty
            && self.continues_endnote(pos)
            && let Some(sep) = self.separator_story("endnote:continuationSeparator", col.width, memos)?
        {
            place_units(&sep.units, col.x, y, out);
            y += sep.height;
        } else if col.balance && self.column > 0 && self.ctx.env.text.compat == Compat::Legacy && self.in_endnote_text(pos) {
            // Compat ≤ 14: in a balanced section every later column that starts with endnote text starts with the
            // continuation separator, its rule across the column.
            let sep = self.separator_or_rule("endnote:continuationSeparator", col.width, col.width, memos)?;
            place_units(&sep.units, col.x, y, out);
            y += sep.height;
        }
        // Keep-with-next chain
        #[derive(Clone)]
        struct Breakpoint {
            pos: FlowToken,
            carry: FlowState,
            out: usize,
            y: f64,
            empty: bool,
            area: NoteArea,
            starts: usize,
            effects: PageEffects,
        }
        fn step_back(members: &[(Breakpoint, usize, bool, bool)]) -> Option<(Breakpoint, usize)> {
            for (bp, here, keep_lines, widow) in members.iter().rev() {
                if bp.empty {
                    return None;
                }
                if *keep_lines {
                    continue;
                }
                let keep = here.saturating_sub(if *widow { 2 } else { 1 });
                if keep >= if *widow { 2 } else { 1 } {
                    return Some((bp.clone(), keep));
                }
            }
            None
        }
        let mut chain: Option<Breakpoint> = None;
        let mut members: Vec<(Breakpoint, usize, bool, bool)> = Vec::new();
        let generic = self.ctx.env.text.compat == Compat::Generic;
        loop {
            // Compat 15: a note cut at the end of a section that the next one follows on the page goes on in the next
            // section's layout.
            let passes = self.hold_notes && (area.cut || !carry.note_rest.is_empty()) && self.at_section_end(pos);
            if pos.block >= nblocks || passes {
                let end = y + if col.balance { carry.prev_after } else { 0.0 };
                // A note cut on this page goes on on the next one, even past the end of the text.
                let more = !carry.note_rest.is_empty() && !area.parts.is_empty() && !passes || carry.table_rest.is_some();
                return Ok((if more { Stop::Full } else { Stop::Done }, end));
            }
            if self.at.end.as_ref() == Some(&*pos) {
                return Ok((Stop::Page, y));
            }
            let vanished = matches!(self.sec.blocks.get(pos.block), Some(Block::Para(p)) if super::paragraph::vanished(p));
            if pos.at.at_start()
                && let Some(end) = self.deleted_before_table(pos.block)
            {
                // Compat ≤ 14: before a table whose rows are all hidden they keep the space before of the paragraph
                // whose mark they join.
                if !generic && !empty {
                    y += self.hidden_table_before(end);
                }
                pos.block = end;
                pos.next_block();
                continue;
            }
            if pos.at.at_start()
                && (self.break_para_after_table(pos.block) && self.ctx.env.text.compat == Compat::Legacy
                    || self.break_para_after_para(pos.block)
                    || vanished)
            {
                pos.next_block();
                continue;
            }
            // Compat ≤ 14: a beneath-text footnote area goes right under the section's own text, and the endnotes
            // that follow the section go on below it.
            if !generic
                && self.beneath()
                && pos.block == self.sec.section.blocks.len()
                && pos.at.at_start()
                && !area.parts.is_empty()
                && !area.cut
                && carry.note_rest.is_empty()
            {
                self.draw_notes(area, &col, y + carry.prev_after, out, memos)?;
                *area = NoteArea::default();
                (y, carry.prev_after) = (self.at.notes_bottom, 0.0);
            }
            // Compat 15: a continuous section's text waits for the notes going on from an earlier section, on pages
            // of their own.
            if generic
                && pos.block == 0
                && pos.at.at_start()
                && self.sec.index > 0
                && self.sec.section.break_kind == crate::docx::SectionBreak::Continuous
                && (area.carried > 0 || !carry.note_rest.is_empty())
            {
                return Ok((Stop::Full, y));
            }
            let bi = pos.block;
            let start_state = Breakpoint {
                pos: pos.clone(),
                carry: carry.clone(),
                out: out.len(),
                y,
                empty,
                area: area.clone(),
                starts: self.at.starts.len(),
                effects: self.at.effects.clone(),
            };
            let found_len = self.at.found.len();
            match self.sec.blocks.get(bi) {
                Some(Block::Para(_)) => {
                    let Some((laid, end)) = self.laid(bi) else { unreachable!("a paragraph block") };
                    let p = laid.para();
                    let style = p.para_style_in(self.ctx.env.text.review.view);
                    // The frames before the paragraph that stay on this page are placed first; a frame that starts
                    // with the break takes the break with it.
                    let framed = matches!(p.inlines.first(), Some(Inline::Float(f, _)) if f.text_box.is_some() && f.shape.is_none());
                    let mut breaks = pos.at.at_start() && style.page_break_before && !(empty && page_empty);
                    if pos.at.at_start() && style.page_break_before && framed {
                        let pb = self.para_lines(&laid, bi, end, col.width, (0, false), col.x, memos)?;
                        self.at.frames_only = true;
                        let r = self.place_para(p, &pb, pos, carry, &col, &mut y, &mut empty, area, out, memos);
                        self.at.frames_only = false;
                        breaks = r? == Stop::Page;
                    }
                    if breaks {
                        carry.keep_top = self.ctx.env.text.compat == Compat::Legacy
                            || bi == 0 && self.sec.section.break_kind == crate::docx::SectionBreak::Continuous;
                        return Ok((Stop::Page, y));
                    }
                    // Compat ≤ 14: a page-break-before paragraph keeps its space-before on a page that is already
                    // empty too (after a page-break character).
                    if pos.at.at_start() && style.page_break_before && self.ctx.env.text.compat == Compat::Legacy {
                        carry.keep_top = true;
                    }
                    let pb = self.para_lines(&laid, bi, end, col.width, (pos.at.index(), carry.letter_break), col.x, memos)?;
                    let placed = self.place_para(p, &pb, pos, carry, &col, &mut y, &mut empty, area, out, memos)?;
                    if pos.block > bi {
                        pos.block = end + 1;
                    }
                    match placed {
                        Stop::Done => {}
                        Stop::Full if pos.block == bi && pos.at.at_start() => {
                            if let Some(c) = chain.take()
                                && !c.empty
                            {
                                *pos = c.pos;
                                *carry = c.carry;
                                *area = c.area;
                                out.truncate(c.out);
                                self.at.starts.truncate(c.starts);
                                self.at.found.truncate(found_len);
                                self.at.effects = c.effects;
                                carry.keep_top = false;
                                return Ok((Stop::Full, c.y));
                            }
                            return Ok((Stop::Full, y));
                        }
                        s => return Ok((s, y)),
                    }
                    if style.keep_next {
                        let here = out[start_state.out..].iter().map(|c| if let Child::Para(f) = c { f.lines.len() } else { 0 }).sum();
                        members.push((start_state.clone(), here, style.keep_lines, style.widow_control));
                        if chain.is_none() {
                            chain = Some(start_state);
                        }
                        let fits = self.next_fits(pos.block, carry, &col, y, area.total(), memos)?;
                        if !fits && let Some((c, cap)) = generic.then(|| step_back(&members)).flatten() {
                            *pos = c.pos;
                            *carry = c.carry;
                            *area = c.area;
                            out.truncate(c.out);
                            self.at.starts.truncate(c.starts);
                            self.at.found.truncate(found_len);
                            self.at.effects = c.effects;
                            self.at.keep_cap = Some((pos.block, cap));
                            (chain, members) = (None, Vec::new());
                            y = c.y;
                            empty = c.empty;
                            continue;
                        }
                        if !fits
                            && let Some(c) = chain.take()
                            && !c.empty
                        {
                            *pos = c.pos;
                            *carry = c.carry;
                            *area = c.area;
                            out.truncate(c.out);
                            self.at.starts.truncate(c.starts);
                            self.at.found.truncate(found_len);
                            self.at.effects = c.effects;
                            carry.keep_top = false;
                            return Ok((Stop::Full, c.y));
                        }
                    } else {
                        (chain, members) = (None, Vec::new());
                    }
                }
                Some(Block::Table(_)) if self.ctx.env.text.vertical.is_some() => {
                    match self.place_upright_table(bi, pos, carry, &col, &mut y, empty, area, memos)? {
                        Stop::Done => (chain, members) = (None, Vec::new()),
                        s => return Ok((s, y)),
                    }
                }
                Some(Block::Table(t)) => {
                    let breaks = t.rows.first().is_some_and(|r| {
                        r.cells.iter().any(
                            |c| matches!(c.blocks.first(), Some(Block::Para(p)) if p.para_style_in(self.ctx.env.text.review.view).page_break_before),
                        )
                    });
                    if pos.at.at_start() && breaks && !(empty && page_empty) {
                        return Ok((Stop::Page, y));
                    }
                    let tb = self.table_layout(bi, col.width, memos)?;
                    match self.place_table(&tb, pos, carry, &col, &mut y, &mut empty, area, out, memos)? {
                        Stop::Done => (chain, members) = (None, Vec::new()),
                        s => {
                            if pos.block == bi
                                && pos.at.at_start()
                                && let Some((c, cap)) = generic.then(|| step_back(&members)).flatten()
                            {
                                *pos = c.pos;
                                *carry = c.carry;
                                *area = c.area;
                                out.truncate(c.out);
                                self.at.starts.truncate(c.starts);
                                self.at.effects = c.effects;
                                self.at.keep_cap = Some((pos.block, cap));
                                (chain, members) = (None, Vec::new());
                                y = c.y;
                                empty = c.empty;
                                continue;
                            }
                            if let Some(c) = chain.take()
                                && !c.empty
                                && pos.block == bi
                                && pos.at.at_start()
                            {
                                *pos = c.pos;
                                *carry = c.carry;
                                *area = c.area;
                                out.truncate(c.out);
                                self.at.starts.truncate(c.starts);
                                self.at.effects = c.effects;
                                return Ok((Stop::Full, c.y));
                            }
                            return Ok((s, y));
                        }
                    }
                }
                None => unreachable!("block index checked above"),
            }
        }
    }

    fn next_fits(&mut self, bi: usize, carry: &FlowState, col: &ColumnSpace, y: f64, notes: f64, memos: &mut Memos) -> Result<bool, Yield> {
        let bottom = col.bottom.to_pt() - notes;
        match self.sec.blocks.get(bi) {
            None => Ok(true),
            Some(Block::Para(_)) => {
                let Some((laid, end)) = self.laid(bi) else { unreachable!("a paragraph block") };
                let pb = self.para_lines(&laid, bi, end, col.width, (0, false), col.x, memos)?;
                let s = laid.para().para_style_in(self.ctx.env.text.review.view);
                let g = match &carry.prev {
                    Some(ps) => gap(&self.ctx.env.text, ps, carry.prev_after, s, space_before(&self.ctx.env.text, s)),
                    None => self.edge_before(bi, s, space_before(&self.ctx.env.text, s)),
                };
                let n = pb.lines.len();
                let need = if s.keep_lines {
                    n
                } else if s.widow_control {
                    2.min(n)
                } else {
                    1.min(n)
                };
                let h: f64 = pb.lines[..need].iter().map(|l| l.pitch).sum::<f64>() - pb.lines.get(need.saturating_sub(1)).map_or(0.0, |l| l.below);
                Ok(y + g + h <= bottom + EPS)
            }
            Some(Block::Table(_)) => {
                let tb = self.table_layout(bi, col.width, memos)?;
                let after = self.after_above_table(bi, carry);
                Ok(tb.rows.first().is_none_or(|r| y + after + r.height + tb.bottom_border <= bottom + EPS))
            }
        }
    }

    /// The space after the paragraph at `block` that its next paragraph keeps: none between paragraphs of one style
    /// with contextual spacing.
    fn after_kept(&self, block: usize, style: &ParaStyle) -> f64 {
        let text = &self.ctx.env.text;
        let after = space_after(text, style);
        match self.sec.blocks.neighbours(block, block, text.review.view).1 {
            Some(next) => gap(text, style, after, next, 0.0),
            None => after,
        }
    }

    /// The floats of the last fill that wrap the text at `block`: one anchored further on only if it met the text laid
    /// out before its paragraph; one that didn't is placed below that text and wraps only what follows it.
    fn known_at(&self, block: usize) -> impl Iterator<Item = &Placed> {
        let si = self.sec.index;
        self.at.known.iter().filter(move |f| f.collided || (f.key.0, f.key.1) <= (si, block))
    }

    fn record(&mut self, p: &Paragraph, first_part: bool) {
        let id = para_id(p);
        if !self.at.effects.paras.contains(&id) {
            self.at.effects.paras.push(id);
        }
        if first_part {
            for i in &p.inlines {
                if let Inline::Bookmark(name) = i
                    && !name.starts_with('/')
                {
                    self.at.effects.bookmarks.retain(|b| b != name);
                    self.at.effects.bookmarks.push(name.clone());
                }
            }
        }
    }
}
