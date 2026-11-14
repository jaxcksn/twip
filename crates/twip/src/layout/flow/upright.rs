//! Upright tables in vertical sections

use crate::docx::Compat;
use crate::docx::model::*;
use crate::docx::props::{Jc, TableAnchor};
use crate::layout::LayoutCtx;
use crate::layout::float::{self, Placed};
use crate::layout::ids::ParaId;
use crate::layout::job::{Memos, Yield};
use crate::layout::space::ColumnSpace;
use crate::layout::story::StoryEnv;
use crate::layout::table::TableLayout;
use crate::layout::token::{FlowState, NoteArea, NotePart};
use crate::layout::units::{EPS, Lu};

use super::{ColumnCursor, PageFill, Stop};

impl<'d> PageFill<'d, '_> {
    /// A table in a vertical section, laid out horizontally and drawn upright: at the stack position the next line
    /// would take (compat ≤ 14 moves it back towards the earlier lines by its legacy lead), from the line start or
    /// below an earlier such table it meets, its own `jc` and indent ignored. It doesn't advance the stack: the lines
    /// it meets, 9pt either side across the page, wrap past it along the line. In compat 15 a table that doesn't fit
    /// across the rest of the page goes to the next one, unless the page has nothing else. A floating table keeps its
    /// left and right distances from the text, on those sides as printed, and starts `tblpY` (or where `tblpYSpec`
    /// puts it) along the line, a `tblpX` from the text further along the stack; one placed from the page on both
    /// axes sits at `tblpX`, `tblpY` as printed.
    #[allow(clippy::too_many_arguments, reason = "as place_table")]
    pub(super) fn place_upright_table(&mut self, bi: usize, cur: &mut ColumnCursor, col: &ColumnSpace, memos: &mut Memos) -> Result<Stop, Yield> {
        let (pos, carry, y, empty, notes, _) = cur.parts();
        let empty = *empty;
        let Some(Block::Table(t)) = self.sec.blocks.get(bi) else { unreachable!("an upright table") };
        let Some(v) = self.ctx.env.text.vertical else { unreachable!("a vertical section") };
        if !empty {
            *y += self.after_above_table(bi, carry);
        }
        let text = crate::layout::env::TextEnv { vertical: None, ..self.ctx.env.text.clone() };
        let le = crate::layout::LayoutEnv { text, ..self.ctx.env.clone() };
        let env = StoryEnv { ctx: LayoutCtx { env: &le, ..self.ctx }, fields: self.fields, notes: self.note_numbers };
        let prev = self.sec.blocks.neighbours(bi, bi, self.ctx.env.text.review.view).0;
        let tb = crate::layout::table::layout(t, col.bottom.to_pt() - col.top.to_pt(), &env, prev, false, memos)?;
        let (bl, br) = crate::layout::table::outer_box(&tb);
        let h: f64 = tb.rows.iter().filter(|r| !r.hidden).map(|r| r.height).sum::<f64>() + tb.bottom_border;
        let legacy = self.ctx.env.text.compat == Compat::Legacy;
        let back = if legacy { crate::layout::table::legacy_lead(t) } else { 0.0 };
        let down = v.flow != crate::layout::vertical::Flow::DownEast;
        let fp = t.floating;
        let paged = fp.is_some_and(|fp| fp.v_anchor == TableAnchor::Page && fp.h_page);
        let y0 = match fp {
            Some(fp) if paged && down => self.page_h - (fp.x - back) - (br - bl),
            Some(fp) if paged => fp.x - back,
            // `tblpX` from the text runs along the stack.
            Some(fp) if fp.h_text => *y - back + fp.x,
            _ => *y - back,
        };
        let end = col.x + col.width;
        let meeting = |y0: f64| {
            let y1 = y0 + br - bl;
            self.at.found.iter().filter(move |o| o.key.2 == float::UPRIGHT_TABLE && o.excl[1] < y1 - EPS && o.excl[3] > y0 + EPS)
        };
        let below_tables = |y0: f64| meeting(y0).map(|o| o.excl[2]).fold(col.x, f64::max);
        // The distances across, on the turned page's top and bottom: the sides as printed swap when the lines stack
        // from the right.
        let (l, r) = fp.map_or((9.0, 9.0), |fp| (fp.dist[2], fp.dist[3]));
        let (above, below) = if down { (r, l) } else { (l, r) };
        // Lines pushed past it don't move it: it stays where an earlier attempt at the page put it. In compat 15 the
        // lines before it that went past an earlier table take it along when it was there moved past that table or
        // ran past the line end below it.
        let passed = |k: f64| {
            let (moved, short) = (|o: &Placed| (o.excl[3] + above - k).abs() < EPS, below_tables(k) + h > end + EPS);
            self.at
                .found
                .iter()
                .filter(|o| o.key.2 == float::UPRIGHT_TABLE && o.excl[3] <= y0 + EPS)
                .any(|o| moved(o) || (short && meeting(k).any(|m| m.same(o))))
        };
        let known_rect = self.at.known.iter().find(|k| k.key == (self.sec.index, bi, float::UPRIGHT_TABLE)).map(|k| k.rect);
        let known = known_rect.map(|r| r[1]).filter(|&k| legacy || !passed(k));
        // Compat ≤ 14 after lines that went across past an earlier table (a line starts at its obstacle's far edge
        // `e`): it takes the stack position after them, and keeps it when the lines then went past it too. It stays
        // there with the lines that meet it below it when its obstacle clears the first of those lines or spans the
        // earlier table's on the page; else, when the earlier table's far edge (less its legacy lead) is at or past
        // the middle of the text across and the lines still fit past it before the far margin, it runs the line for
        // them and they go across past it again; else it goes to the next page.
        let mut full_line = false;
        let mut jumped_at = None;
        if legacy && fp.is_none() && carry.upright_at.is_none() {
            let upright = || self.at.found.iter().filter(|o| o.key.2 == float::UPRIGHT_TABLE && o.excl[3] <= *y + EPS);
            let after = known_rect.filter(|r| *y >= r[3] + below - EPS && upright().any(|o| o.excl[3] <= r[1] + EPS));
            if let Some(r) = after {
                (full_line, jumped_at) = (true, Some(r[1]));
            } else if let Some(o) =
                upright().filter(|o| self.at.starts.iter().any(|s| (s.0 - o.excl[3]).abs() < EPS)).max_by(|a, b| a.excl[3].total_cmp(&b.excl[3]))
            {
                let e = o.excl[3];
                let lead = match self.sec.blocks.get(o.key.1) {
                    Some(Block::Table(t1)) => crate::layout::table::legacy_lead(t1),
                    _ => back,
                };
                let second = self.at.starts.iter().map(|s| s.0).filter(|&s| s > e + EPS && s < *y - EPS).fold(*y, f64::min);
                let (top, bottom) = (col.top.to_pt(), col.bottom.to_pt());
                // ponytail: an earlier table at the stack's start keeps it there too; Word then draws the line between
                // them over it and the earlier table further out, which isn't modelled.
                let clears = y0 - above >= second - EPS || br - bl + above + below >= e - top - EPS || o.rect[1] <= top + EPS;
                if !clears {
                    let fits = y0 + br - bl + below + (*y - e) <= bottom + EPS;
                    if e + lead > (top + bottom) / 2.0 + EPS || !fits {
                        return Ok(Stop::Full);
                    }
                    full_line = true;
                }
                jumped_at = Some(y0);
            }
        }
        let stack = y0;
        // Its later parts go on where the first one was, across the page.
        let y0 = carry.upright_at.filter(|_| !pos.at.at_start()).or(jumped_at).or(known).unwrap_or(y0);
        // Compat 15 moves a table that runs past the line end below the tables it meets across the stack, past the
        // one that holds it back (their distances touching). Where that would put its far distance past the page's
        // edge it stays, drawn from the line start over them.
        let (own, mut y0) = (y0, y0);
        while !legacy && fp.is_none() && below_tables(y0) > col.x + EPS && below_tables(y0) + h > end + EPS {
            let Some(b) = meeting(y0).max_by(|a, b| a.excl[2].total_cmp(&b.excl[2])) else { break };
            y0 = b.excl[3] + above;
        }
        let over = y0 > own + EPS && y0 + br - bl + below > self.page_h + EPS;
        if over {
            y0 = own;
        }
        let y1 = y0 + br - bl;
        // Compat ≤ 14 has no fit test: the table stays at its stack position, over the margin if need be.
        if y1 > col.bottom.to_pt() + EPS && !empty && !paged && !legacy {
            return Ok(Stop::Full);
        }
        let along = match fp {
            _ if over => col.x,
            Some(fp) if paged => fp.y,
            // `tblpYSpec` from the margin or the page: along the line as printed.
            Some(fp) if fp.v_anchor != TableAnchor::Text && fp.y_spec.is_some() => {
                let (a0, a1) = if fp.v_anchor == TableAnchor::Page { (0.0, self.page_w) } else { (col.x, col.x + col.width) };
                match fp.y_spec {
                    Some(Jc::Center) => (a0 + a1 - h) / 2.0,
                    Some(Jc::Right) => a1 - h,
                    _ => a0,
                }
            }
            Some(fp) => col.x + fp.y,
            None => below_tables(y0),
        };
        // Compat 15 also moves a table longer than the rest of the line to the next page; one that starts a page or
        // was moved across splits by rows at the line end, in either mode, its header rows repeated.
        if along + h > end + EPS && !empty && !legacy && fp.is_none() && y0 <= stack + EPS {
            return Ok(Stop::Full);
        }
        // On the page as printed: the turned page's y runs leftwards from its right edge, or rightwards when the
        // lines stack from the left.
        let left = if v.flow == crate::layout::vertical::Flow::DownEast { y0 } else { self.page_h - y1 };
        let bottom = if fp.is_some() { Lu::MAX } else { Lu::round_half_up(end) };
        let (along, bottom, reach) = if legacy && along > col.x + EPS && along + h > end + EPS && fp.is_none() {
            legacy_over(&tb, pos.at.index(), col, end, along)
        } else {
            (along, bottom, None)
        };
        // Compat 15: a table whose notes can't start past it goes to the next page with them.
        if !legacy && !empty && !paged && pos.at.at_start() {
            let refs: Vec<(NoteKind, i64)> =
                tb.rows.iter().flat_map(|r| r.cells.iter()).flat_map(|c| c.units.units.iter().flat_map(|u| u.notes.iter().copied())).collect();
            if refs.iter().any(|r| r.0 == NoteKind::Foot) && !self.add_notes(&refs, &mut notes.clone(), col, y1, &mut Vec::new(), false, memos)? {
                return Ok(Stop::Full);
            }
        }
        let page_col = ColumnSpace { x: left - bl, width: br - bl, top: Lu::round_half_up(along), bottom, balance: false };
        // Its notes don't count against its fit: they are collected aside and go to the page's note area.
        let mut area = NoteArea { shared: Some(0.0), below: true, ..NoteArea::default() };
        let (mut top, mut fresh, mut rows) = (along, true, Vec::new());
        let saved = std::mem::take(&mut self.at.starts);
        let mut inner = ColumnCursor { pos, carry, y: &mut top, empty: &mut fresh, area: &mut area, out: &mut rows };
        let stop = self.place_table(&tb, &mut inner, &page_col, memos)?;
        carry.upright_at = (stop == Stop::Full).then_some(y0);
        self.at.starts = saved;
        self.at.upright.extend(rows);
        self.upright_notes(&area, y0, y1, notes, carry, col, memos)?;
        let (from, to) = if full_line { (col.x, end) } else { (along, top) };
        let f = float::table_float(t.id, to - from, y1 - y0, [above, below, 0.0, 0.0], true);
        let mut o = Placed::new((self.sec.index, bi, float::UPRIGHT_TABLE), ParaId(t.id), &f, from, y0, y1 - y0);
        o.early_right = reach.map(|r| along + r);
        self.at.found.push(o);
        Ok(stop)
    }

    // An upright table's notes
    #[allow(clippy::too_many_arguments, reason = "the table's notes, where it stands, and the column's note area")]
    fn upright_notes(
        &mut self,
        area: &NoteArea,
        y0: f64,
        y1: f64,
        notes: &mut NoteArea,
        carry: &mut FlowState,
        col: &ColumnSpace,
        memos: &mut Memos,
    ) -> Result<(), Yield> {
        let legacy = self.ctx.env.text.compat == Compat::Legacy;
        let refs: Vec<(NoteKind, i64)> = area.parts.iter().map(|p| (p.kind, p.id)).collect();
        if !refs.is_empty() {
            // From past the table across the page; compat ≤ 14 from where it starts. What doesn't fit goes on on
            // the next page.
            let from = if legacy { y0 } else { y1 };
            let (mut a, mut rest) = (notes.clone(), Vec::new());
            if self.add_notes(&refs, &mut a, col, from, &mut rest, false, memos)? {
                // A note cut there leaves no room for the text after the table.
                if !rest.is_empty() {
                    a.kept = (col.bottom.to_pt() - from - a.total()).max(0.0);
                }
                *notes = a;
                carry.note_rest.extend(rest);
            } else {
                // ponytail: a note that waits whole for the next page is drawn there as a continuation, under the
                // continuation separator; give it a fresh start if Word draws the plain separator there.
                for &(kind, id) in refs.iter().filter(|r| r.0 == NoteKind::Foot) {
                    let units = self.note_units(kind, id, col.width, memos)?.units.clone();
                    self.at.effects.notes_shown.push((kind, id, self.note_numbers.get(&(kind, id)).cloned().unwrap_or_default()));
                    carry.note_rest.push(NotePart { kind, id, units });
                }
            }
        }
        Ok(())
    }
}

// Compat ≤ 14: one that runs past the line end below an earlier table is drawn from the line start, over
// it. It keeps the more of the rows that fit below that table (all that fit the line, when those reach past
// its end) and the rows that fit once the table is stacked below its own image until it no longer fits
// whole; at least one. When the rows that fit below that table reach past its end, the lines before meet
// those rows only.
fn legacy_over(tb: &TableLayout, from: usize, col: &ColumnSpace, end: f64, along: f64) -> (f64, Lu, Option<f64>) {
    let mut reach = None;
    let hs: Vec<f64> = tb.rows.iter().skip(from).filter(|r| !r.hidden).map(|r| r.height).collect();
    let bb = tb.bottom_border;
    let fit = |room: f64| {
        let mut s = bb;
        hs.iter()
            .take_while(|&&r| {
                s += r;
                s <= room + EPS
            })
            .count()
    };
    let mut under = fit(end - along);
    let piece = hs[..under].iter().sum::<f64>() + bb;
    if under > 0 && col.x + piece - bb > along + EPS {
        reach = Some(piece);
        under = fit(col.width);
    }
    let whole = hs.iter().sum::<f64>() + bb;
    let mut room = col.width - whole;
    while whole > EPS && room >= whole {
        room -= whole;
    }
    let kept = under.max(fit(room)).max(1).min(hs.len());
    (col.x, Lu::round_half_up(col.x + hs[..kept].iter().sum::<f64>() + bb), reach)
}
