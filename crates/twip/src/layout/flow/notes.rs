//! Footnote and endnote areas

use std::rc::Rc;

use crate::docx::Compat;
use crate::docx::model::*;
use crate::layout::float::Wrap;
use crate::layout::fragment::{BoxFragment, BoxKind, Child, Item};
use crate::layout::job::{Memos, Yield};
use crate::layout::space::ColumnSpace;
use crate::layout::story::{StoryEnv, StoryLayout, StoryUnit};
use crate::layout::text::{FieldValues, FontMetrics};
use crate::layout::token::{FlowState, FlowToken, NoteArea, NotePart};
use crate::layout::units::EPS;

use super::{PageFill, place_units, rebase};

/// Default footnote separator line metrics.
const SEPARATOR_METRICS: FontMetrics = FontMetrics { ascent: 1950.0, descent: 550.0, leading: 0.0, upem: 2048.0 };

/// The space shared between two notes in an area: the gap between them is the later one's space before or the
/// earlier one's space after, whichever is larger.
fn note_join(prev: Option<&NotePart>, next: &NotePart) -> f64 {
    let after = prev.and_then(|p| p.units.last()).map_or(0.0, |u| u.after);
    after.min(next.units.first().map_or(0.0, |u| u.top))
}

/// A note's height in its area, its space after included.
fn note_extent(part: &NotePart) -> f64 {
    part.height() + part.units.last().map_or(0.0, |u| u.after)
}

/// The height of `parts` stacked in one column.
fn stacked(parts: &[NotePart]) -> f64 {
    parts.iter().enumerate().map(|(i, p)| note_extent(p) - note_join(i.checked_sub(1).map(|j| &parts[j]), p)).sum()
}

pub fn note_cut(units: &[StoryUnit], room: f64) -> usize {
    let mut fit = units.iter().position(|x| x.top + x.height > room + EPS).unwrap_or(units.len());
    if fit == 0 || fit >= units.len() || units[fit - 1].block != units[fit].block || !units[fit].widow {
        return fit;
    }
    let para = units[fit].block;
    let start = units[..fit].iter().rposition(|u| u.block != para).map_or(0, |k| k + 1);
    let end = units[fit..].iter().position(|u| u.block != para).map_or(units.len(), |k| fit + k);
    let (before, after) = (fit - start, end - fit);
    if after < 2 {
        fit = fit.saturating_sub(2 - after);
    }
    if fit - start < 2 && before > 0 && units[start].from == 0 {
        fit = start;
    }
    fit
}

// Note streams
/// Lays a note part out again at a column's width: the note's text from the part's first line on.
pub type Rewrap<'w> = dyn FnMut(&NotePart, f64) -> Result<NotePart, Yield> + 'w;

/// `notes` flowed through columns of `widths`, `h` high: the columns, the tallest one's height, and the parts that don't
/// fit. A note (or its rest) is wrapped at the width of the column it starts in.
pub fn stream_flow(notes: &[NotePart], widths: &[f64], h: f64, wrap: &mut Rewrap) -> Result<(Vec<Vec<NotePart>>, f64, Vec<NotePart>), Yield> {
    let ncols = widths.len().max(1);
    let uneven = widths.iter().any(|w| (w - widths[0]).abs() > EPS);
    let mut cols: Vec<Vec<NotePart>> = vec![Vec::new()];
    let (mut used, mut tallest) = (0.0, 0.0f64);
    for (i, note) in notes.iter().enumerate() {
        let mut part = note.clone();
        loop {
            if uneven {
                part = wrap(&part, widths[cols.len() - 1])?;
            }
            let total = part.height();
            if total <= h - used + EPS {
                used += total;
                cols.last_mut().expect("a column").push(part);
                break;
            }
            let fit = note_cut(&part.units, h - used);
            if fit > 0 {
                used += part.units[fit - 1].top + part.units[fit - 1].height;
                cols.last_mut().expect("a column").push(NotePart { units: part.units[..fit].to_vec(), ..part.clone() });
                part = NotePart { units: rebase(&part.units[fit..]), ..part };
            }
            tallest = tallest.max(used);
            if cols.len() >= ncols {
                let rest = std::iter::once(part).chain(notes[i + 1..].iter().cloned()).collect();
                return Ok((cols, tallest, rest));
            }
            cols.push(Vec::new());
            used = 0.0;
        }
    }
    Ok((cols, tallest.max(used), Vec::new()))
}

pub fn stream_fill(notes: &[NotePart], widths: &[f64], h: f64, wrap: &mut Rewrap) -> Result<Option<(Vec<Vec<NotePart>>, f64)>, Yield> {
    let (cols, tallest, rest) = stream_flow(notes, widths, h, wrap)?;
    Ok(rest.is_empty().then_some((cols, tallest)))
}

/// The last note's space after, part of a stream area below its notes' columns.
fn stream_tail(notes: &[NotePart]) -> f64 {
    notes.last().and_then(|p| p.units.last()).map_or(0.0, |u| u.after)
}

/// The smallest height `notes` balance to in columns of `widths` within `room`, with the last note's space after.
pub fn stream_height(notes: &[NotePart], widths: &[f64], room: f64, wrap: &mut Rewrap) -> Result<Option<f64>, Yield> {
    if notes.is_empty() {
        return Ok(Some(0.0));
    }
    if stream_fill(notes, widths, room, wrap)?.is_none() {
        return Ok(None);
    }
    let (mut lo, mut hi) = (0.0, room);
    for _ in 0..40 {
        let mid = (lo + hi) / 2.0;
        if stream_fill(notes, widths, mid, wrap)?.is_some() {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    Ok(stream_fill(notes, widths, hi, wrap)?.map(|(_, t)| t + stream_tail(notes)))
}

impl<'d> PageFill<'d, '_> {
    // Footnotes
    pub(super) fn note_units(&mut self, kind: NoteKind, id: i64, width: f64, memos: &mut Memos) -> Result<Rc<StoryLayout>, Yield> {
        let blocks: Vec<&Block> = self.notes.blocks(kind, id).map(|b| b.iter().collect()).unwrap_or_default();
        let env = StoryEnv { ctx: self.ctx, fields: self.fields, notes: self.note_numbers };
        crate::layout::story::layout(&blocks, width, &env, None, memos)
    }

    /// The separator story `name` laid out at `width`, when the document has it.
    pub(super) fn separator_story(&mut self, name: &str, width: f64, memos: &mut Memos) -> Result<Option<Rc<StoryLayout>>, Yield> {
        let Some(b) = self.notes.separators.get(name) else { return Ok(None) };
        let blocks: Vec<&Block> = b.iter().collect();
        let fields = FieldValues::default();
        let env = StoryEnv { ctx: self.ctx, fields: &fields, notes: self.note_numbers };
        crate::layout::story::layout(&blocks, width, &env, None, memos).map(Some)
    }

    /// Whether only the section's empty closing paragraph, or nothing, is left from `pos`.
    pub(super) fn at_section_end(&self, pos: &FlowToken) -> bool {
        let n = self.sec.blocks.len();
        pos.block >= n
            || pos.at.at_start()
                && pos.block + 1 == n
                && n > 1
                && matches!(self.sec.blocks.get(pos.block), Some(Block::Para(p)) if p.inlines.is_empty())
    }

    /// Whether `pos` is at or inside an endnote's text (past the endnote separator).
    pub(super) fn in_endnote_text(&self, pos: &FlowToken) -> bool {
        let s = &self.sec.blocks.stories;
        s.len() > 1 && pos.block >= s[1] && pos.block < self.sec.blocks.len()
    }

    /// Whether `pos` is inside an endnote that began on an earlier page.
    pub(super) fn continues_endnote(&self, pos: &FlowToken) -> bool {
        let s = &self.sec.blocks.stories;
        s.len() > 1 && pos.block >= s[1] && !(pos.at.at_start() && s[1..].contains(&pos.block))
    }

    pub(super) fn separator(&mut self, cont: bool, width: f64, memos: &mut Memos) -> Result<Rc<StoryLayout>, Yield> {
        let name = if cont { "footnote:continuationSeparator" } else { "footnote:separator" };
        self.separator_or_rule(name, 144.0, width, memos)
    }

    /// The separator story `name` laid out at `width`, else the built-in one: an 11pt line with a rule `rule` wide.
    pub(super) fn separator_or_rule(&mut self, name: &str, rule: f64, width: f64, memos: &mut Memos) -> Result<Rc<StoryLayout>, Yield> {
        let s = match self.separator_story(name, width, memos)? {
            Some(s) => s,
            None => Rc::new({
                let (a, d, e) = crate::layout::text::metrics_from(SEPARATOR_METRICS, 11.0);
                let h = a + d + e;
                let rule = Item::rect([0.0, h * 0.78 - 2.7, rule, h * 0.78 - 2.1], 0);
                let content: Rc<[Child]> = Rc::from([Child::Items(Box::new([rule]))]);
                StoryLayout {
                    units: vec![StoryUnit {
                        top: 0.0,
                        height: h,
                        below: 0.0,
                        after: 0.0,
                        hold: 0.0,
                        content,
                        block: 0,
                        from: 0,
                        widow: false,
                        keep_next: false,
                        keep_lines: false,
                        notes: Box::new([]),
                        row: None,
                    }],
                    height: h,
                    float_bottom: 0.0,
                    frozen: Vec::new(),
                    origins: Vec::new(),
                    tables: Vec::new(),
                }
            }),
        };
        Ok(s)
    }

    /// The height of the footnote continuation notice, 0 without one.
    fn notice_height(&mut self, width: f64, memos: &mut Memos) -> Result<f64, Yield> {
        Ok(self.separator_story("footnote:continuationNotice", width, memos)?.map_or(0.0, |s| s.height))
    }

    /// Records the endnote references among `refs` as shown on this page (numbers restarting each page count from there).
    pub(super) fn show_endnote_refs(&mut self, refs: impl IntoIterator<Item = (NoteKind, i64)>) {
        for (kind, id) in refs.into_iter().filter(|r| r.0 == NoteKind::End) {
            self.at.effects.notes_shown.push((kind, id, self.note_numbers.get(&(kind, id)).cloned().unwrap_or_default()));
        }
    }

    /// Adds the footnotes `refs` of a line ending at `line_bottom` to `area`, the parts that don't fit to `rest`; whether
    /// the line fits. `defer`: the line starts its paragraph's part in the column (or may not be parted from it), so in
    /// compat ≤ 14 a note that can't start here waits whole for the next page when a new note before it on the page
    /// was placed whole.
    #[allow(clippy::too_many_arguments, reason = "the line's notes, the area and column they go in, and where the line ends")]
    pub(super) fn add_notes(
        &mut self,
        refs: &[(NoteKind, i64)],
        area: &mut NoteArea,
        col: &ColumnSpace,
        line_bottom: f64,
        rest: &mut Vec<NotePart>,
        defer: bool,
        memos: &mut Memos,
    ) -> Result<bool, Yield> {
        if !refs.iter().any(|r| r.0 == NoteKind::Foot) {
            return Ok(col.bottom.holds(line_bottom + area.total()));
        }
        if self.notes_closed {
            return Ok(false);
        }
        let mut a = area.clone();
        if a.sep == 0.0 {
            a.sep = self.separator(false, col.width, memos)?.height;
            if self.ctx.env.text.compat == Compat::Legacy && a.shared.is_none() {
                a.notice = self.notice_height(col.width, memos)?;
            }
        }
        if a.shared.is_some() {
            return self.add_shared_notes(refs, a, area, col, line_bottom, rest, memos);
        }
        self.add_column_notes(refs, a, area, col, line_bottom, rest, defer, memos)
    }

    // Shared page area
    #[allow(clippy::too_many_arguments, reason = "as add_notes")]
    fn add_shared_notes(
        &mut self,
        refs: &[(NoteKind, i64)],
        mut a: NoteArea,
        area: &mut NoteArea,
        col: &ColumnSpace,
        line_bottom: f64,
        rest: &mut Vec<NotePart>,
        memos: &mut Memos,
    ) -> Result<bool, Yield> {
        // ponytail: after a note is cut, a later line's notes move it to the next page; where the body after the
        // cut goes on in a later column, its notes might share the next page's stream instead.
        if a.cut && !a.below {
            return Ok(false);
        }
        for &(k, id) in refs.iter().filter(|r| r.0 == NoteKind::Foot) {
            let u = self.note_units(k, id, a.cols.first().map_or(col.width, |c| c.1), memos)?;
            a.parts.push(NotePart { kind: k, id, units: u.units.clone() });
        }
        if a.below {
            *area = a;
            return Ok(col.bottom.holds(line_bottom));
        }
        let room = col.bottom.to_pt() - line_bottom - a.sep;
        let widths: Vec<f64> = if a.cols.is_empty() { vec![col.width] } else { a.cols.iter().map(|c| c.1).collect() };
        // With columns the page's text up to the line can be spread over them, which leaves the area the room under
        // it: before the last column, notes that need more than the room under the line make it wait for a later
        // column; a cut note fills the room the spread text leaves.
        let cs = self.sec.section.section_in(self.ctx.env.text.review.view).columns;
        let n = if cs.equal || cs.cols.is_empty() { cs.num } else { cs.cols.len() }.max(1);
        let full = col.bottom.to_pt() - col.top.to_pt();
        let used = self.column as f64 * (full - area.total()) + line_bottom - col.top.to_pt();
        let spread = (widths.len() > 1 && n > 1).then(|| full - used / n as f64 - a.sep);
        let waits = self.column + 1 < n && spread.is_some_and(|s| s > room + EPS);
        let mut wrap = |p: &NotePart, w: f64| self.rewrap(p, w, memos);
        let mut cut = Vec::new();
        let h =
            if widths.len() > 1 { stream_height(&a.parts, &widths, room, &mut wrap)? } else { Some(stacked(&a.parts)).filter(|&h| h <= room + EPS) };
        if h.is_none() && waits {
            return Ok(false);
        }
        match h {
            Some(h) => a.height = h,
            // Compat 15: the line's last note goes on on the next page when its first part fits.
            None => {
                let room = spread.map_or(room, |s| s.min(room));
                let (cols, tallest, over) = stream_flow(&a.parts, &widths, room, &mut wrap)?;
                let last = a.parts.last().expect("the line's notes");
                let Some(first) = over.first().filter(|o| (o.kind, o.id) == (last.kind, last.id)) else { return Ok(false) };
                let placed: usize = cols.iter().flatten().filter(|p| (p.kind, p.id) == (first.kind, first.id)).map(|p| p.units.len()).sum();
                let units = &last.units;
                let first_para = units.iter().take_while(|x| x.block == units[0].block).count();
                if placed < if units.first().is_some_and(|u| u.widow) { 2.min(first_para) } else { 1.min(first_para) } {
                    return Ok(false);
                }
                a.height = tallest;
                a.cut = true;
                cut = over;
            }
        }
        // A cut area is as high as it is, whatever an earlier fill of the page reserved.
        if !col.bottom.holds(line_bottom + if a.cut { a.sep + a.height + a.kept } else { a.total() }) {
            return Ok(false);
        }
        *area = a;
        rest.extend(cut);
        Ok(true)
    }

    // Column area
    #[allow(clippy::too_many_arguments, reason = "as add_notes")]
    fn add_column_notes(
        &mut self,
        refs: &[(NoteKind, i64)],
        mut a: NoteArea,
        area: &mut NoteArea,
        col: &ColumnSpace,
        line_bottom: f64,
        rest: &mut Vec<NotePart>,
        defer: bool,
        memos: &mut Memos,
    ) -> Result<bool, Yield> {
        let mut new_rest = Vec::new();
        let mut deferred = false;
        for &(k, id) in refs {
            if k != NoteKind::Foot {
                continue;
            }
            let u = self.note_units(k, id, col.width, memos)?;
            let (units, total) = (&u.units, u.height);
            // Compat ≤ 14: a later note on the line follows whole after an earlier one cut here.
            if deferred || !new_rest.is_empty() && self.ctx.env.text.compat == Compat::Legacy {
                new_rest.push(NotePart { kind: k, id, units: units.clone() });
                continue;
            }
            let mut room = col.bottom.to_pt() - line_bottom - a.total();
            // A note cut here is cut in the room the continuation notice leaves.
            let notice = if total > room + EPS && !a.noticed { self.notice_height(col.width, memos)? } else { a.notice };
            if total > room + EPS {
                room -= notice - a.notice;
            }
            let first_para = units.iter().take_while(|x| x.block == units[0].block).count();
            let min = if units.first().is_some_and(|x| x.widow) { 2.min(first_para) } else { 1.min(first_para) };
            let min_h = units.get(min.saturating_sub(1)).map_or(0.0, |x| x.top + x.height);
            let fit = if total <= room + EPS { units.len() } else { note_cut(units, room) };
            if min_h > room + EPS || !new_rest.is_empty() || fit < min {
                let waits = defer
                    && new_rest.is_empty()
                    && a.parts.len() > a.carried
                    && self.ctx.env.text.compat == Compat::Legacy
                    && col.bottom.holds(line_bottom + a.total());
                if !waits {
                    return Ok(false);
                }
                deferred = true;
                new_rest.push(NotePart { kind: k, id, units: units.clone() });
                continue;
            }
            if fit == units.len() {
                let part = NotePart { kind: k, id, units: units.clone() };
                a.height += note_extent(&part) - note_join(a.parts.last(), &part);
                a.parts.push(part);
                continue;
            }
            let part = NotePart { kind: k, id, units: units[..fit].to_vec() };
            a.height += note_extent(&part) - note_join(a.parts.last(), &part);
            a.parts.push(part);
            (a.notice, a.noticed) = (notice, notice > 0.0);
            new_rest.push(NotePart { kind: k, id, units: rebase(&units[fit..]) });
        }
        *area = a;
        rest.extend(new_rest);
        Ok(true)
    }

    pub(super) fn continue_notes(&mut self, carry: &mut FlowState, col: &ColumnSpace, area: &mut NoteArea, memos: &mut Memos) -> Result<(), Yield> {
        // A cut area's rest goes on on the next page.
        if carry.note_rest.is_empty() || area.cut {
            return Ok(());
        }
        area.cont = true;
        area.sep = self.separator(true, col.width, memos)?.height;
        if self.ctx.env.text.compat == Compat::Legacy {
            area.notice = self.notice_height(col.width, memos)?;
        }
        let mut rest = Vec::new();
        for part in std::mem::take(&mut carry.note_rest) {
            // In a later column of the page the note goes on at that column's width; compat 15 on the next page at
            // the width of the section laying it out.
            let part = if self.column > 0 || self.ctx.env.text.compat == Compat::Generic { self.rewrap(&part, col.width, memos)? } else { part };
            let room = col.bottom.to_pt() - col.top.to_pt() - area.total();
            // Cut as a new note is, the orphan step only for a part holding its paragraph's first line.
            // ponytail: measured between a balanced section's columns; a continuation going on from page to page is
            // assumed to cut the same way.
            let fit = note_cut(&part.units, room);
            if fit == 0 || !rest.is_empty() {
                rest.push(part);
                continue;
            }
            area.height += part.units[fit - 1].top + part.units[fit - 1].height;
            if fit < part.units.len() {
                rest.push(NotePart { units: rebase(&part.units[fit..]), ..part.clone() });
            }
            area.parts.push(NotePart { units: part.units[..fit].to_vec(), ..part });
        }
        carry.note_rest = rest;
        area.carried = area.parts.len();
        Ok(())
    }

    /// `part` laid out at `width`: its note's text from the part's first line on. A part starting at a table row keeps
    /// its layout.
    fn rewrap(&mut self, part: &NotePart, width: f64, memos: &mut Memos) -> Result<NotePart, Yield> {
        let Some(u) = part.units.first().filter(|u| u.row.is_none()) else { return Ok(part.clone()) };
        let key = (part.kind, part.id, u.block, u.from, width.to_bits());
        if let Some(units) = self.pass.rewrapped.get(&key) {
            return Ok(NotePart { units: units.clone(), ..part.clone() });
        }
        let blocks: Vec<&Block> = self.notes.blocks(part.kind, part.id).map(|b| b.iter().collect()).unwrap_or_default();
        let env = StoryEnv { ctx: self.ctx, fields: self.fields, notes: self.note_numbers };
        let units = rebase(&crate::layout::story::layout_from(&blocks, width, &env, (u.block, u.from), memos)?.units);
        self.pass.rewrapped.insert(key, units.clone());
        Ok(NotePart { units, ..part.clone() })
    }

    /// Starts the page's note stream with the notes going on from the last page, flowed through its columns.
    pub(super) fn continue_stream(&mut self, carry: &mut FlowState, col: &ColumnSpace, area: &mut NoteArea, memos: &mut Memos) -> Result<(), Yield> {
        area.cont = true;
        area.sep = self.separator(true, col.width, memos)?.height;
        let room = col.bottom.to_pt() - col.top.to_pt() - area.sep;
        let widths: Vec<f64> = if area.cols.is_empty() { vec![col.width] } else { area.cols.iter().map(|c| c.1).collect() };
        let mut wrap = |p: &NotePart, w: f64| self.rewrap(p, w, memos);
        // The notes going on are laid out again at this page's first column width.
        carry.note_rest = std::mem::take(&mut carry.note_rest).iter().map(|p| wrap(p, widths[0])).collect::<Result<_, _>>()?;
        let (_, tallest, over) = stream_flow(&carry.note_rest, &widths, room, &mut wrap)?;
        area.parts = std::mem::take(&mut carry.note_rest);
        area.height = if over.is_empty() { stream_height(&area.parts, &widths, room, &mut wrap)?.unwrap_or(tallest) } else { tallest };
        area.carried = area.parts.len();
        area.cut = !over.is_empty();
        carry.note_rest = over;
        Ok(())
    }

    pub fn notes_box(&mut self, area: &NoteArea, cols: &[(f64, &[NotePart])], width: f64, top: f64, memos: &mut Memos) -> Result<Child, Yield> {
        let sep = self.separator(area.cont, width, memos)?;
        let mut children = Vec::new();
        let mut sep_children = Vec::new();
        let x0 = cols.first().map_or(0.0, |c| c.0);
        place_units(&sep.units, x0, top, &mut sep_children);
        children.push(Child::Box(BoxFragment {
            kind: BoxKind::Separator,
            id: None,
            offset: (0.0, 0.0),
            size: (width, sep.height),
            children: sep_children.into(),
        }));
        let mut end = (x0, top + sep.height);
        for (x, parts) in cols {
            let mut y = top + sep.height;
            for (i, part) in parts.iter().enumerate() {
                y -= note_join(i.checked_sub(1).map(|j| &parts[j]), part);
                let mut nc = Vec::new();
                place_units(&part.units, *x, y, &mut nc);
                self.at.effects.notes_shown.push((part.kind, part.id, self.note_numbers.get(&(part.kind, part.id)).cloned().unwrap_or_default()));
                children.push(Child::Box(BoxFragment {
                    kind: BoxKind::Note { kind: part.kind, id: part.id },
                    id: None,
                    offset: (0.0, 0.0),
                    size: (width, part.height()),
                    children: nc.into(),
                }));
                y += note_extent(part);
                end = (*x, y);
            }
        }
        // The continuation notice, an ordinary paragraph under the cut note's last line.
        if area.noticed
            && let Some(n) = self.separator_story("footnote:continuationNotice", width, memos)?
        {
            place_units(&n.units, end.0, end.1, &mut children);
        }
        Ok(Child::Box(BoxFragment { kind: BoxKind::Notes, id: None, offset: (0.0, 0.0), size: (width, area.total()), children: children.into() }))
    }

    /// Whether the page's footnote area goes beneath the text.
    pub(super) fn beneath(&self) -> bool {
        self.at.beneath.unwrap_or(self.sec.section.notes[0].beneath)
    }

    /// The top of a footnote area of height `h` in `col`: at the column bottom; with `beneathText`, at `text` (the
    /// bottom of the column's text), below the floats it would overlap in compat 15, and at the bottom when it
    /// doesn't fit there.
    pub fn note_top(&self, h: f64, col: &ColumnSpace, text: f64) -> f64 {
        let bottom = col.bottom.to_pt() - h;
        if !self.beneath() {
            return bottom;
        }
        let mut top = text.max(col.top.to_pt());
        if self.ctx.env.text.compat == Compat::Generic {
            let (x0, x1) = (col.x, col.x + col.width);
            while let Some(f) = self
                .at
                .found
                .iter()
                .chain(&self.fixed)
                .find(|f| f.wrap != Wrap::None && f.rect[1] < top + h - EPS && f.rect[3] > top + EPS && f.rect[0] < x1 && f.rect[2] > x0)
            {
                top = f.rect[3];
            }
        }
        if top > bottom + EPS { bottom } else { top }
    }

    /// Draws `area` in its columns, or in `col` when it has none of its own, below `text` or at the bottom of `col`.
    pub fn draw_notes(&mut self, area: &NoteArea, col: &ColumnSpace, text: f64, out: &mut Vec<Child>, memos: &mut Memos) -> Result<(), Yield> {
        if area.parts.is_empty() {
            return Ok(());
        }
        self.at.notes_drawn = true;
        let cols = if area.cols.is_empty() { vec![(col.x, col.width)] } else { area.cols.clone() };
        let (first, last) = (cols[0], cols[cols.len() - 1]);
        let span = ColumnSpace { x: first.0, width: last.0 + last.1 - first.0, ..*col };
        let h = area.sep + area.height + area.notice;
        let y = self.note_top(h, &span, text) + 0.15;
        if self.beneath() {
            self.at.notes_bottom = self.at.notes_bottom.max(y - 0.15 + h);
        } else {
            self.at.effects.notes_top = self.at.effects.notes_top.into_iter().chain([y - 0.15]).reduce(f64::min);
        }
        let widths: Vec<f64> = cols.iter().map(|c| c.1).collect();
        let parts = if cols.len() > 1 || area.cut {
            let mut wrap = |p: &NotePart, w: f64| self.rewrap(p, w, memos);
            let tail = if area.cut { 0.0 } else { stream_tail(&area.parts) };
            stream_flow(&area.parts, &widths, area.height - tail + EPS, &mut wrap)?.0
        } else {
            vec![area.parts.clone()]
        };
        let per_col: Vec<(f64, &[NotePart])> = cols.iter().zip(&parts).map(|(c, p)| (c.0, &p[..])).collect();
        out.push(self.notes_box(area, &per_col, first.1, y, memos)?);
        Ok(())
    }
}
