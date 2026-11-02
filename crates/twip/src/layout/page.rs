//! Page bodies: geometry (HDR-4 to HDR-6, HDR-13), columns (COL-1), the attempts that settle floats and the shared
//! note stream (FLOAT-11, COL-3), vertical alignment (SETUP-1), balancing (COL-5) and column separators. Each
//! attempt owns its state.

use crate::docx::Compat;
use std::collections::HashMap;
use std::rc::Rc;

use crate::docx::model::*;
use crate::docx::review::MarkupView;

use super::LayoutCtx;
use super::flow::{AttemptState, NoteArea, PageFill, PassMemo, Stop, stream_fill};
use super::fragment::{BoxFragment, BoxKind, Child, Item, PlacedLine, RowFragment};
use super::ids::FragmentId;
use super::job::{Memos, Yield};
use super::node::{Notes, SectionNode};
use super::record::{BodyRecord, PageEffects, PageRecord};
use super::space::{BodyArea, BodyFields, BodySpace, ColumnSpace, HfHeights};
use super::story::{StoryEnv, hf_limit, stack_limited};
use super::text::FieldValues;
use super::token::{FlowState, FlowToken, PageToken};
use super::units::{LU_PER_PT, Lu};

const EPS: f64 = 1e-6;

/// What page-dependent fields need from a finished pass (FIELD-1).
#[derive(Clone, Default, PartialEq, Debug)]
pub struct PassInfo {
    pub num_pages: Option<String>,
    pub section_pages: Vec<String>,
    pub page_refs: Rc<HashMap<String, String>>,
}

/// Horizontal margins and the top margin of a page (gutter, mirror margins: HDR-13), of the section's page in the
/// markup view (REV-2).
pub fn margins(s: &Section, ctx: &LayoutCtx, page_number: i64) -> (f64, f64, f64) {
    let s = s.section_in(ctx.env.text.review.view);
    let (mut left, mut right, mut top) = (s.left, s.right, s.top);
    let odd = page_number % 2 == 1;
    if ctx.env.gutter_at_top {
        top += s.gutter;
    } else if ctx.env.mirror_margins {
        if odd {
            left += s.gutter;
        } else {
            std::mem::swap(&mut left, &mut right);
            right += s.gutter;
        }
    } else {
        left += s.gutter;
    }
    (left, right, top)
}

/// Which header or footer a page shows (HDR-7): its variant (0 default, 1 first, 2 even) and the part it comes from
/// (HDR-12: inherited from earlier sections per variant).
pub fn story_ref(
    sections: &[Section],
    even_and_odd: bool,
    si: usize,
    page_in_section: usize,
    page_number: i64,
    footer: bool,
) -> (usize, Option<&String>) {
    // shortcut: callers outside layout name the story the All-markup page shows; a section whose tracked change
    // turned its title page on or off picks the other variant in Original.
    story_ref_in(sections, MarkupView::All, even_and_odd, si, page_in_section, page_number, footer)
}

/// [`story_ref`] for the section's page in `view` (REV-2).
pub fn story_ref_in(
    sections: &[Section],
    view: MarkupView,
    even_and_odd: bool,
    si: usize,
    page_in_section: usize,
    page_number: i64,
    footer: bool,
) -> (usize, Option<&String>) {
    let variant = if sections[si].section_in(view).title_pg && page_in_section == 0 {
        1
    } else if even_and_odd && page_number % 2 == 0 {
        2
    } else {
        0
    };
    let part = (0..=si).rev().find_map(|k| (if footer { &sections[k].footers } else { &sections[k].headers })[variant].as_ref());
    (variant, part)
}

/// Column boxes of a section's columns on a page (COL-1), from `top`.
pub fn columns(c: &Columns, geo: &BodyArea, top: f64) -> Vec<ColumnSpace> {
    let n = c.num.max(1);
    let mut out = Vec::new();
    if c.equal || c.cols.is_empty() {
        let w = (geo.width - (n as f64 - 1.0) * c.space) / n as f64;
        for k in 0..n {
            out.push(ColumnSpace {
                x: geo.left + k as f64 * (w + c.space),
                width: w,
                top: Lu::round_half_up(top),
                bottom: geo.bottom,
                balance: false,
            });
        }
    } else {
        let mut x = geo.left;
        for (w, sp) in &c.cols {
            out.push(ColumnSpace { x, width: *w, top: Lu::round_half_up(top), bottom: geo.bottom, balance: false });
            x += w + sp;
        }
    }
    out
}

/// Every glyph run's (first glyph x, baseline, size) in `children`, page coordinates.
fn glyph_runs(children: &[Child], dx: f64, dy: f64, out: &mut Vec<(f64, f64, f64)>) {
    let item = |it: &Item, dx: f64, dy: f64, out: &mut Vec<(f64, f64, f64)>| {
        if let Item::Glyphs(r) = it {
            out.push((r.glyphs.first().map_or(0.0, |g| g.1) + dx, r.baseline + dy, r.size));
        }
    };
    for c in children {
        match c {
            // COL-1: notes aren't column text, so a page's footnotes don't stretch the lines.
            Child::Box(b) if b.kind == BoxKind::Notes => {}
            Child::Box(b) => glyph_runs(&b.children, dx + b.offset.0, dy + b.offset.1, out),
            Child::Group { offset, children } => glyph_runs(children, dx + offset.0, dy + offset.1, out),
            Child::Para(p) => {
                for l in p.lines.iter() {
                    for it in l.line.items.iter() {
                        item(it, dx + l.x, dy + l.y, out);
                    }
                }
                for it in p.line_numbers.iter().flatten() {
                    item(it, dx, dy, out);
                }
            }
            Child::Line { placed, .. } => {
                for it in placed.line.items.iter() {
                    item(it, dx + placed.x, dy + placed.y, out);
                }
            }
            Child::Table(t) => {
                for r in t.rows.iter() {
                    for cell in r.cells.iter() {
                        glyph_runs(&cell.content, dx, dy, out);
                    }
                }
            }
            Child::Items(items) => {
                for it in items.iter() {
                    item(it, dx, dy, out);
                }
            }
        }
    }
}

/// `item` moved down by `dy`.
fn item_down(it: &Item, dy: f64) -> Item {
    super::traverse::translated(it, 0.0, dy)
}

pub(crate) fn row_down(r: &RowFragment, dy: f64) -> RowFragment {
    RowFragment {
        row: r.row,
        rect: [r.rect[0], r.rect[1] + dy, r.rect[2], r.rect[3] + dy],
        top: r.top + dy,
        edges: r.edges.clone(),
        background: r.background.iter().map(|i| item_down(i, dy)).collect(),
        cells: r
            .cells
            .iter()
            .map(|c| super::fragment::CellFragment {
                cell: c.cell,
                content: c.content.iter().map(|x| child_down(x, &|_| dy)).collect(),
                place: c.place.moved_down(dy),
            })
            .collect(),
        borders: r.borders.iter().map(|i| item_down(i, dy)).collect(),
        repeat: r.repeat,
        changed: r.changed,
    }
}

/// `c` moved down: by `shift(y)` of each line's, row's and item's own top (SETUP-1's "both" spreads the free space over
/// paragraph gaps), or uniformly.
fn child_down(c: &Child, shift: &dyn Fn(f64) -> f64) -> Child {
    match c {
        Child::Para(p) => Child::Para(super::fragment::ParaFragment {
            para: p.para,
            lines: p.lines.iter().map(|l| PlacedLine { x: l.x, y: l.y + shift(l.y + EPS), line: l.line.clone() }).collect(),
            line_numbers: p.line_numbers.iter().map(|g| g.iter().map(|i| item_down(i, shift(i.top()))).collect()).collect(),
        }),
        Child::Line { para, placed } => {
            Child::Line { para: *para, placed: PlacedLine { x: placed.x, y: placed.y + shift(placed.y + EPS), line: placed.line.clone() } }
        }
        Child::Table(t) => Child::Table(super::fragment::TableFragment {
            table: t.table,
            rows: t.rows.iter().map(|r| row_down(r, shift(r.rect[1] + EPS))).collect(),
        }),
        Child::Items(items) => {
            Child::Items(items.iter().map(|i| item_down(i, shift(i.top() + if matches!(i, Item::Glyphs(_)) { 0.0 } else { EPS }))).collect())
        }
        Child::Group { offset, children } => Child::Group { offset: (offset.0, offset.1 + shift(offset.1 + EPS)), children: children.clone() },
        Child::Box(b) => {
            let mut b = b.clone();
            b.children = b.children.iter().map(|x| child_down(x, shift)).collect();
            Child::Box(b)
        }
    }
}

/// Where a pass continues. A pass advances one committed body (or one composed page) at a time, so a job can stop at
/// any of these points and resume there.
#[derive(Clone, Debug)]
pub enum Cursor {
    /// Start section `si`, with the state carried from the previous one; `y`: where it would start on the current
    /// page (a continuous break).
    Section {
        si: usize,
        carry: FlowState,
        y: Option<f64>,
    },
    /// Section `si`'s next body from `token`: at `top` on the current page (a continuous section), else at its top.
    Body {
        si: usize,
        token: PageToken,
        top: Option<f64>,
    },
    /// Compose page `i`'s header and footer.
    Compose(usize),
    Done,
}

/// The state of one pass, owned so a job can keep it between `run` calls.
pub struct PassRun {
    /// The pass number (1–4; 5 is the stabilizing pass).
    pub k: usize,
    /// The previous pass's values for page-dependent body fields (FIELD-1).
    pub known: Rc<PassInfo>,
    /// Header and footer heights to lay out page bodies with, by page index: the previous pass's composed heights, or
    /// the stabilizing pass's reservations (FIELD-6). Pages past its end measure their stories with this pass's
    /// body values.
    pub hf_in: Rc<Vec<HfHeights>>,
    /// Clip composed headers and footers to the heights the bodies used (the stabilizing pass).
    pub clip: bool,
    pub pages: Vec<PageRecord>,
    pub page_number: i64,
    pub memo: PassMemo,
    pub cursor: Cursor,
    /// Set when composition starts: what the pass's own pages give page-dependent fields.
    pub info: Option<PassInfo>,
}

impl PassRun {
    pub fn new(k: usize, known: Rc<PassInfo>, hf_in: Rc<Vec<HfHeights>>, clip: bool) -> PassRun {
        PassRun {
            k,
            known,
            hf_in,
            clip,
            pages: Vec::new(),
            page_number: 0,
            memo: PassMemo::default(),
            cursor: Cursor::Section { si: 0, carry: FlowState::default(), y: None },
            info: None,
        }
    }
}

/// What a committed body leads to.
pub enum Next {
    /// The section continues on a new page (already started) from this token.
    Page(PageToken),
    /// The section ended: the state carried on, and where the next section would start on this page.
    SectionEnd(FlowState, f64),
}

/// The document a pass pages, and its state.
pub struct Paginator<'d, 'p> {
    pub ctx: LayoutCtx<'d>,
    pub sections: &'d [Section],
    pub stories: &'d std::collections::BTreeMap<String, Vec<Block>>,
    pub notes: Notes<'d>,
    pub note_numbers: &'p HashMap<(NoteKind, i64), String>,
    pub run: &'p mut PassRun,
}

impl<'d> Paginator<'d, '_> {
    /// Body field values on the current page (FIELD-1).
    pub fn fields(&self, si: usize) -> FieldValues {
        FieldValues {
            page: Some(self.run.page_number.to_string()),
            num_pages: self.run.known.num_pages.clone(),
            section: Some((si + 1).to_string()),
            section_pages: self.run.known.section_pages.get(si).cloned(),
            page_refs: Some(self.run.known.page_refs.clone()),
            style_refs: Vec::new(),
            page_fmt: self.sections[si].pg_fmt.clone(),
        }
    }

    fn page_in_section(&self, si: usize) -> usize {
        self.run.pages.iter().filter(|p| p.section == si && !p.blank).count()
    }

    /// Starts a page; `blank` = an inserted blank page.
    pub fn new_page(&mut self, si: usize, blank: bool) {
        self.run.page_number += 1;
        let s = self.sections[si].section_in(self.ctx.env.text.review.view);
        self.run.pages.push(PageRecord {
            w: s.page_w,
            h: s.page_h,
            body_area: None,
            bodies: Vec::new(),
            section: si,
            effects: PageEffects::default(),
            hf: [None, None],
            hf_boxes: [None, None],
            hf_full: [None, None],
            reused: None,
            number: self.run.page_number,
            blank,
        });
    }

    /// The header and footer heights of the current page for section `si`, measured with this pass's body values
    /// (HDR-8 caps them).
    fn measure_hf(&self, si: usize, memos: &mut Memos) -> Result<HfHeights, Yield> {
        let view = self.ctx.env.text.review.view;
        let sec = &self.sections[si];
        if self.run.pages.last().is_some_and(|p| p.blank) {
            return Ok([None, None]);
        }
        let (left, right, _) = margins(sec, &self.ctx, self.run.page_number);
        let s = sec.section_in(view);
        let width = s.page_w - left - right;
        let pis = self.page_in_section(si).saturating_sub(1);
        let fields = self.fields(si);
        let limit = hf_limit(self.ctx.env.text.compat, s.page_h);
        let env = StoryEnv { ctx: self.ctx, fields: &fields, notes: self.note_numbers };
        let mut out = [None, None];
        for (k, footer) in [false, true].into_iter().enumerate() {
            let Some(blocks) = story_ref_in(self.sections, view, self.ctx.env.even_and_odd, si, pis, self.run.page_number, footer)
                .1
                .and_then(|p| self.stories.get(p))
            else {
                continue;
            };
            let blocks: Vec<&Block> = blocks.iter().collect();
            let layout = super::story::layout(&blocks, width, &env, None, memos)?;
            out[k] = Some(stack_limited(&layout, left, if footer { 0.0 } else { s.header }, limit).1);
        }
        Ok(out)
    }

    /// The current page's body space for section `si` (HDR-4 to HDR-6, HDR-13, COL-1), starting at `start` for a
    /// continuous section. A page's first body takes its header and footer heights from `hf_in` when the pass has them.
    pub(crate) fn body_space(&self, si: usize, start: Option<f64>, balance: bool, memos: &mut Memos) -> Result<BodySpace, Yield> {
        let sec = &self.sections[si];
        let (left, right, top) = margins(sec, &self.ctx, self.run.page_number);
        let s = sec.section_in(self.ctx.env.text.review.view);
        let width = s.page_w - left - right;
        let given = if start.is_none() { self.run.hf_in.get(self.run.pages.len() - 1).copied() } else { None };
        let hf = match given {
            Some(h) => h,
            None => self.measure_hf(si, memos)?,
        };
        let mut body_top = top;
        let mut body_bottom = s.page_h - s.bottom;
        if let Some(height) = hf[0]
            && s.top >= 0.0
        {
            body_top = body_top.max(s.header + height);
        }
        if let Some(height) = hf[1]
            && s.bottom >= 0.0
        {
            body_bottom = body_bottom.min(s.page_h - s.footer - height);
        }
        if s.top < 0.0 {
            body_top = -s.top; // HDR-5
        }
        if s.bottom < 0.0 {
            body_bottom = s.page_h + s.bottom;
        }
        let area = BodyArea { left, width, top: Lu::round_half_up(body_top), bottom: Lu::round_half_up(body_bottom) };
        Ok(BodySpace {
            section: si,
            page_w: s.page_w,
            page_h: s.page_h,
            margins: (left, right, top),
            area,
            columns: columns(&s.columns, &area, start.unwrap_or(area.top.to_pt())),
            mid_page: start.is_some(),
            balance,
            next_continuous: self.sections.get(si + 1).is_some_and(|n| n.break_kind == SectionBreak::Continuous),
            hf,
        })
    }

    /// Lays out and commits section `sec`'s next body from `token` (at `top` on the current page for a continuous
    /// section); starts the next page when the section continues.
    pub fn body(&mut self, sec: &SectionNode<'d>, token: PageToken, top: Option<f64>, balance: bool, memos: &mut Memos) -> Result<Next, Yield> {
        let si = sec.index;
        let space = self.body_space(si, top, balance, memos)?;
        let fields = self.fields(si);
        let earlier: Vec<Rc<BoxFragment>> = self.run.pages.last().map(|p| p.bodies.iter().map(|b| b.body.clone()).collect()).unwrap_or_default();
        let env = BodyEnv { ctx: self.ctx, notes: self.notes, note_numbers: self.note_numbers, pass: &mut self.run.memo };
        let b = layout_body(sec, &space, &fields, &token, &earlier, env, memos)?;
        // The table the body starts in, as the fill just laid it out at the first column's width (tables whose text
        // varies by page aren't memoized and get none: no row exception for them).
        let start_table = match (&token.flow.at, sec.blocks.get(token.flow.block)) {
            (super::token::At::Row(_) | super::token::At::RowPart { .. }, Some(Block::Table(t))) => self.run.memo.table(t, space.columns[0].width),
            _ => None,
        };
        let page = self.run.pages.last_mut().expect("a page was started");
        page.body_area = page.body_area.or(Some((space.area.top.to_pt(), space.area.bottom.to_pt())));
        page.effects.merge(&b.effects);
        // A paragraph the body ends in takes the blocks of the paragraphs joined to it (REV-6).
        let last = sec.blocks.chain_end(b.end.flow.block, self.ctx.env.text.review.view).min(sec.blocks.len().saturating_sub(1));
        let shown = sec.blocks.blocks.get(token.flow.block..=last).unwrap_or(&[]);
        let end = b.end.clone();
        let page_h = space.page_h;
        page.bodies.push(BodyRecord {
            fields: fields_used(&fields, shown),
            token,
            space,
            end: b.end,
            section_end: b.section_end,
            effects: b.effects,
            excluded: super::traverse::excluded_in(&b.fragment, page_h),
            start_table,
            held: std::mem::take(&mut self.run.memo.held),
            body: b.fragment,
        });
        Ok(match b.section_end {
            Some(y) => Next::SectionEnd(end.state, y),
            None => {
                self.new_page(si, false);
                Next::Page(end)
            }
        })
    }

    /// COL-5: refills the section's last page at the balanced column height.
    #[allow(
        clippy::type_complexity,
        reason = "one column fill's whole result (end, children, token, state, effects); a struct used only here would add nothing"
    )]
    fn balance(
        fill: &mut PageFill,
        sec: &SectionNode,
        cols: &[ColumnSpace],
        saved: &(FlowToken, FlowState),
        memos: &mut Memos,
    ) -> Result<Option<(f64, Vec<Child>, FlowToken, FlowState, PageEffects)>, Yield> {
        let (p0, c0) = saved;
        let top = cols[0].top.to_pt();
        // COL-6: the empty paragraph holding the section break takes no space. In compat 15 one right after a table
        // still takes its line, below the content of the column it ends in, past the balanced height if need be.
        let blocks = &sec.blocks;
        let n = blocks.len();
        let skip_last = matches!(blocks.get(n.wrapping_sub(1)), Some(Block::Para(p)) if p.inlines.is_empty() && n > 1);
        let mark_line = skip_last && fill.ctx.env.text.compat == Compat::Generic && matches!(blocks.get(n.wrapping_sub(2)), Some(Block::Table(_)));
        let state = fill.at.clone();
        // One fill at column height `h`: the result when the content ends in these columns, and each column's
        // statistics.
        #[allow(
            clippy::type_complexity,
            reason = "one column fill's whole result (end, children, token, state, effects); a struct used only here would add nothing"
        )]
        let try_h = |fill: &mut PageFill,
                     h: f64,
                     memos: &mut Memos|
         -> Result<(Option<(FlowToken, FlowState, Vec<Child>, f64, PageEffects)>, Vec<ColStats>), Yield> {
            let mut pos = p0.clone();
            let mut carry = c0.clone();
            let mut children = Vec::new();
            let mut stats = Vec::new();
            let mut end = top;
            fill.at = AttemptState { known: state.known.clone(), ..Default::default() };
            for (k, c) in cols.iter().enumerate() {
                let col = ColumnSpace { bottom: Lu::round_half_up(top + h).min(c.bottom), balance: true, ..*c };
                let r = fill.fill(&pos, &carry, col, false, memos)?;
                let s = r.stop;
                (pos, carry) = (r.token, r.state);
                stats.push(ColStats::of(&r.children, r.bottom - top));
                let mut kids = r.children;
                let mut bottom = r.bottom;
                let at_mark = skip_last && pos.block + 1 == n && pos.at.at_start();
                if at_mark && mark_line && !fill.at.forced {
                    let m = fill.fill(&pos, &carry, ColumnSpace { top: Lu::round_half_up(r.bottom), bottom: Lu::MAX, ..col }, false, memos)?;
                    (pos, carry, bottom) = (m.token, m.state, m.bottom);
                    kids.extend(m.children);
                }
                children.push(Child::Box(BoxFragment {
                    kind: BoxKind::Column,
                    id: None,
                    offset: (0.0, 0.0),
                    size: (0.0, 0.0),
                    children: kids.into(),
                }));
                // A column ends below its last line and that paragraph's space-after.
                end = end.max(bottom + carry.prev_after);
                if fill.at.forced {
                    return Ok((None, stats));
                }
                if s == Stop::Done || at_mark {
                    return Ok((Some((pos, carry, children, end, fill.at.effects.clone())), stats));
                }
                if k + 1 < cols.len() {
                    carry.keep_top = s == Stop::Column && carry.keep_top;
                }
            }
            Ok((None, stats))
        };
        let full = cols[0].bottom.to_pt() - top;
        let (first, stats) = try_h(fill, full, memos)?;
        if first.is_none() {
            // Nothing balances: the fill stays as it was, ending at the full column height.
            fill.at = state;
            return Ok(None);
        }
        // The height comes from an estimate, then grows (or in compat 15 shrinks) by a step: not always the smallest
        // that holds the content. Layout units.
        let full_lu = i64::from(Lu::round_half_up(full).0);
        let widths: Vec<f64> = cols.iter().map(|c| c.width).collect();
        let wsum: f64 = widths.iter().sum::<f64>().max(1.0);
        let to_pt = |h: i64| h as f64 / LU_PER_PT;
        let mean = |st: &[ColStats]| {
            let (n, total) = st.iter().fold((0i64, 0i64), |(n, t), s| (n + s.lines, t + s.height));
            if n > 0 { total / n } else { 49152 } // 240 twips without lines
        };
        let mut h;
        let mut r;
        if fill.ctx.env.text.compat == Compat::Legacy {
            // compat ≤ 14: the total height over the columns (by width from their used heights when they differ), at
            // least the mean line height; while the content doesn't fit, grow by the mean line height of what is in
            // the columns.
            let n = cols.len() as i64;
            h = if widths.iter().all(|w| (w - widths[0]).abs() < 0.01) {
                (stats.iter().map(|s| s.height).sum::<i64>() + n - 1) / n
            } else {
                let tw = |lu: i64| lu * 1440 / 294912;
                let a: f64 = widths.iter().zip(&stats).map(|(w, s)| if s.lines > 0 { (tw(s.used) * 20) as f64 * w } else { 0.0 }).sum();
                (a / wsum) as i64 / 20 * 294912 / 1440
            };
            h = h.max(mean(&stats)).min(full_lu);
            r = try_h(fill, to_pt(h), memos)?;
            while r.0.is_none() && h < full_lu {
                h = (h + mean(&r.1)).min(full_lu);
                r = try_h(fill, to_pt(h), memos)?;
            }
        } else {
            // compat 15: the columns' mean height by width, down to a multiple of the smallest line or row; grow by
            // that step while the content doesn't fit, or shrink by it while it still does.
            let step = |st: &[ColStats]| st.iter().filter(|s| s.lines > 0).map(|s| s.min).min().unwrap_or(1).max(1);
            let m = step(&stats);
            h = (widths.iter().zip(&stats).map(|(w, s)| w * s.height as f64).sum::<f64>() / wsum) as i64;
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
        }
        let Some((p, mut c, children, end, eff)) = r.0.or(first) else { unreachable!("the full height balanced before") };
        // The section ends below its tallest column, including that column's last space-after (COL-5).
        c.prev = None;
        c.prev_after = 0.0;
        Ok(Some((end, children, p, c, eff)))
    }

    /// Draws a page's shared note stream (COL-3).
    fn draw_stream(fill: &mut PageFill, area: &NoteArea, cols: &[ColumnSpace], out: &mut Vec<Child>, memos: &mut Memos) -> Result<(), Yield> {
        if area.parts.is_empty() {
            return Ok(());
        }
        fill.at.notes_drawn = true;
        let Some((parts, _)) = stream_fill(&area.parts, cols.len(), area.height + EPS) else { return Ok(()) };
        let top = cols[0].bottom.to_pt() - area.sep - area.height + 0.15;
        let per_col: Vec<(f64, &[super::token::NotePart])> = cols.iter().zip(&parts).map(|(c, p)| (c.x, &p[..])).collect();
        out.push(fill.notes_box(area, &per_col, cols[0].width, top, memos)?);
        Ok(())
    }

    /// A page's floats: pictures, and text boxes with their content (FLOAT-10).
    fn draw_floats(fill: &PageFill, placed: &[super::float::Placed], memos: &mut Memos) -> Result<Vec<Child>, Yield> {
        let fields = FieldValues::default();
        let env = StoryEnv { ctx: fill.ctx, fields: &fields, notes: fill.note_numbers };
        placed
            .iter()
            .filter(|p| p.key.2 != super::float::TABLE_PIECE)
            .map(|p| super::story::float_box(&p.float, p.rect, p.anchor, &env, memos))
            .collect()
    }
}

/// The values among `fields` that `blocks` show: the [`BodyFields`] a body depends on, usually none. A body
/// laid out with them on another page, or with other values of fields it doesn't show, is the same body. Also what a
/// header or footer story shows (its `HfRecord` fields).
pub(crate) fn fields_used(fields: &BodyFields, blocks: &[&Block]) -> BodyFields {
    fn walk(blocks: &mut dyn Iterator<Item = &Block>, out: &mut BodyFields, all: &BodyFields) {
        for b in blocks {
            match b {
                Block::Para(p) => {
                    for i in &p.inlines {
                        // A header's text boxes and frames show its fields (story::float_box).
                        if let Inline::Float(f, _) = i {
                            for tb in f.text_box.iter().chain(f.parts.iter().filter_map(|p| p.text_box.as_ref())) {
                                walk(&mut tb.blocks.iter(), out, all);
                            }
                        }
                        let Inline::FieldBegin { kind: Some(kind), .. } = i else { continue };
                        match kind {
                            FieldKind::Page => {
                                out.page.clone_from(&all.page);
                                out.page_fmt.clone_from(&all.page_fmt);
                            }
                            FieldKind::NumPages => out.num_pages.clone_from(&all.num_pages),
                            FieldKind::Section => out.section.clone_from(&all.section),
                            FieldKind::SectionPages => out.section_pages.clone_from(&all.section_pages),
                            FieldKind::StyleRef(st) => {
                                let key = crate::docx::fields::style_key(st);
                                for e in all.style_refs.iter().filter(|e| e.0 == key) {
                                    if !out.style_refs.contains(e) {
                                        out.style_refs.push(e.clone());
                                    }
                                }
                            }
                            FieldKind::PageRef(name) => {
                                let value = all.page_refs.as_ref().and_then(|m| m.get(name)).cloned();
                                let refs = Rc::make_mut(out.page_refs.get_or_insert_with(Default::default));
                                if let Some(v) = value {
                                    refs.insert(name.clone(), v);
                                }
                            }
                            _ => {}
                        }
                    }
                }
                Block::Table(t) => {
                    for r in &t.rows {
                        for c in &r.cells {
                            walk(&mut c.blocks.iter(), out, all);
                        }
                    }
                }
            }
        }
    }
    let mut out = BodyFields::default();
    walk(&mut blocks.iter().copied(), &mut out, fields);
    out
}

/// What every page body of a pass shares: the context, the notes and their numbers (NOTE-4), and the pass's memo.
pub struct BodyEnv<'d, 'p> {
    pub ctx: LayoutCtx<'d>,
    pub notes: Notes<'d>,
    pub note_numbers: &'p HashMap<(NoteKind, i64), String>,
    pub pass: &'p mut PassMemo,
}

/// A laid-out page body.
pub struct PageBody {
    pub fragment: Rc<BoxFragment>,
    /// Where the flow is after the body: the next page's token, or past the section's last block.
    pub end: PageToken,
    /// When the section ended on this page: where the next section would start.
    pub section_end: Option<f64>,
    pub effects: PageEffects,
}

/// Lays out one section's body on one page: isolated attempts until the floats met (FLOAT-11) and the shared note
/// height (COL-3) agree with the ones the attempt started from, then the stream, floats, vertical alignment (SETUP-1),
/// balancing (COL-5) and column separators. `earlier` holds the bodies of earlier sections on the page (COL-1's
/// separators look at the page's text so far).
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
    let saved = (pos.clone(), carry.clone());
    let mut bottoms = Vec::new();
    let mut stop = Stop::Full;
    // FLOAT-11: fill the page again until the floats met agree with the ones the text wrapped around.
    let mut known = Vec::new();
    // COL-3: compat 15's one note stream across the columns.
    let shared = cols.len() > 1 && env.ctx.env.text.compat == Compat::Generic;
    let mut reserve = 0.0;
    let mut fill = PageFill {
        ctx: env.ctx,
        sec,
        notes: env.notes,
        fields,
        note_numbers: env.note_numbers,
        page_w: space.page_w,
        page_h: space.page_h,
        margins: space.margins,
        pass: env.pass,
        at: AttemptState::default(),
    };
    let mut col_children: Vec<Vec<Child>> = Vec::new();
    // FLOAT-11: a float that keeps its place (known) but whose anchor a pass pushes off the page is dropped, and the
    // page then ends where that pass ended. Each pass fixes one more float: at most one pass per float and drop,
    // and one more.
    let mut end: Option<FlowToken> = None;
    let mut drops = 0;
    let mut attempts = 6;
    let mut attempt = 0;
    while attempt < attempts {
        attempt += 1;
        let r: Result<bool, Yield> = super::job::unit("page attempt", || {
            fill.at = AttemptState {
                known: known.clone(),
                end: end.clone(),
                stream: shared.then(|| NoteArea { shared: Some(reserve), cols: cols.len(), ..Default::default() }),
                ..Default::default()
            };
            pos = saved.0.clone();
            carry = saved.1.clone();
            col_children.clear();
            bottoms.clear();
            for (k, col) in cols.iter().enumerate() {
                let page_empty = k == 0 && !space.mid_page;
                let r = fill.fill(&pos, &carry, *col, page_empty, memos)?;
                let st = r.stop;
                (pos, carry) = (r.token, r.state);
                col_children.push(r.children);
                bottoms.push(r.bottom);
                stop = st;
                if matches!(st, Stop::Done | Stop::Page) {
                    break;
                }
                if k + 1 < cols.len() {
                    carry.keep_top = st == Stop::Column && carry.keep_top;
                }
            }
            let found = fill.at.found.clone();
            let notes = fill.at.stream.as_ref().map_or(0.0, |a| a.height);
            let stable = found.len() == known.len() && found.iter().all(|f| known.iter().any(|k| k.same(f)));
            attempts = attempts.max(2 + found.len() + drops).min(64);
            if (stable && (notes - reserve).abs() < EPS) || attempt >= attempts {
                return Ok(true);
            }
            if end.is_none() && known.iter().any(|k| !found.iter().any(|f| f.key == k.key)) {
                end = Some(pos.clone());
                drops += 1;
            }
            known = found;
            reserve = notes;
            Ok(false)
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
    if let Some(a) = stream.filter(|_| !(stop == Stop::Done && space.balance && cols.len() > 1)) {
        Paginator::draw_stream(&mut fill, &a, cols, &mut body, memos)?;
    }
    let next_shares = stop == Stop::Done && space.next_continuous;
    let floats = Paginator::draw_floats(&fill, &placed, memos)?;
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
        // PAGE-10: the section ends below its last space-after, so a continuous section's first paragraph keeps the
        // ordinary gap to it (PARA-1). A balanced end already includes it (COL-5).
        let mut e = bottoms.iter().copied().fold(page_top, f64::max) + carry.prev_after;
        if space.balance && cols.len() > 1 {
            match Paginator::balance(&mut fill, sec, cols, &saved, memos)? {
                Some((h, children, p, c, eff)) => {
                    e = h;
                    pos = p;
                    carry = c;
                    effects = eff;
                    // After balancing, which refills the section's content.
                    body = children;
                }
                None => e = cols[0].bottom.to_pt(),
            }
            // The floats are drawn again after balancing. When no height balanced, the unbalanced fill keeps the
            // floats it already had, so they appear twice (a known flaw, kept for now).
            body.extend(floats.iter().cloned());
        }
        section_end = Some(e);
    } else if pos.block >= nblocks {
        section_end = Some(bottoms.iter().copied().fold(page_top, f64::max));
    }
    separators(&s.section_in(env.ctx.env.text.review.view).columns, cols, page_top, earlier, &mut body);
    if env.ctx.env.text.review.view.change_bars() {
        change_bars(space, &mut body);
    }
    if section_end.is_none() {
        if s.line_numbers.as_ref().is_some_and(|l| l.restart == "newPage") {
            carry.line_no = 0;
        }
        if stop == Stop::Full {
            carry.keep_top = false;
        }
    }
    let fragment = Rc::new(BoxFragment {
        kind: BoxKind::Body,
        id: Some(FragmentId::next()),
        offset: (0.0, 0.0),
        size: (space.page_w, space.page_h),
        children: body.into(),
    });
    Ok(PageBody { fragment, end: PageToken { section: space.section, flow: pos, state: carry }, section_end, effects })
}

/// SETUP-1: shifts the section's body for `w:vAlign`. `bottom`: the last line box bottom plus the last paragraph's
/// space-after. Not applied on a page with footnotes (the caller checks).
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
            // The extra space is divided equally among the gaps between paragraphs.
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

/// REV-4: a change bar beside every changed line of the body, `min(36, left margin / 2)` left of the body, 0.72 wide
/// and black. A bar reaches down to the next line when the gap to it is no taller than the line, and the bars of
/// consecutive changed lines join.
// shortcut: headers and footers get no bars.
fn change_bars(space: &BodySpace, body: &mut Vec<Child>) {
    /// Every placed line's top, bottom and whether it changed, in page coordinates.
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
                Child::Items(_) => {}
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
    let x = space.area.left - (space.margins.0 / 2.0).min(36.0);
    let mut bars: Vec<Item> = Vec::new();
    for (k, &(top, bottom, c)) in all.iter().enumerate() {
        if !c {
            continue;
        }
        let next = all[k + 1..].iter().map(|l| l.0).find(|&t| t >= bottom - 0.01);
        let bottom = next.filter(|&t| t - bottom <= bottom - top).unwrap_or(bottom);
        match bars.last_mut() {
            Some(Item::Rect { rect, .. }) if top <= rect[3] + 0.01 => rect[3] = rect[3].max(bottom),
            _ => bars.push(Item::Rect { rect: [x, top, x + 0.72, bottom], colour: 0 }),
        }
    }
    body.push(Child::Items(bars.into()));
}

/// COL-1: separator lines between columns that hold text: the page's text so far (earlier sections' bodies and this
/// section's) decides which columns are used and how far down the lines go.
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
            items.push(Item::Rect { rect: [x - 0.36, top, x + 0.36, bottom], colour: 0 });
        }
    }
    if !items.is_empty() {
        body.push(Child::Items(items.into()));
    }
}

/// COL-5: a balanced column's content: its lines and table rows, their total height and the smallest, and the column's
/// used height (layout units).
struct ColStats {
    lines: i64,
    height: i64,
    min: i64,
    used: i64,
}

impl ColStats {
    fn of(children: &[Child], used: f64) -> ColStats {
        let mut st = ColStats { lines: 0, height: 0, min: i64::MAX, used: i64::from(Lu::round_half_up(used).0) };
        let mut add = |h: f64| {
            let h = i64::from(Lu::round_half_up(h).0);
            st.lines += 1;
            st.height += h;
            st.min = st.min.min(h);
        };
        for c in children {
            match c {
                Child::Para(p) => p.lines.iter().for_each(|l| add(l.line.pitch)),
                Child::Table(t) => t.rows.iter().for_each(|r| add(r.rect[3] - r.rect[1])),
                Child::Line { placed, .. } => add(placed.line.pitch),
                _ => {}
            }
        }
        if st.lines == 0 {
            st.min = 0;
        }
        st
    }
}
