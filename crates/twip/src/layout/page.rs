//! Page bodies

use crate::docx::Compat;
use std::collections::HashMap;
use std::rc::Rc;

use crate::docx::model::*;
use crate::docx::review::MarkupView;

use super::LayoutCtx;
use super::flow::{AttemptState, NoteArea, PageFill, PassMemo, Stop};
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

#[derive(Clone, Default, PartialEq, Debug)]
pub struct PassInfo {
    pub num_pages: Option<String>,
    pub section_pages: Vec<String>,
    pub page_refs: Rc<HashMap<String, String>>,
}

pub fn margins(s: &Section, ctx: &LayoutCtx, page_number: i64) -> (f64, f64, f64) {
    // A vertical section's gutter is on the right (the left on a mirrored even page), and so is a horizontal one's
    // with `w:rtlGutter`.
    let vertical = super::vertical::of_section(s.text_flow).is_some() || s.rtl_gutter;
    let s = s.section_in(ctx.env.text.review.view);
    let (mut left, mut right, mut top) = (s.left, s.right, s.top);
    let odd = page_number % 2 == 1;
    if let Some(rev) = ctx.env.book_fold {
        // Book fold: portrait pages drop the gutter; landscape ones mirror with it.
        if s.page_w > s.page_h {
            if odd != rev {
                (left, right) = (right + s.gutter, left);
            } else {
                right += s.gutter;
            }
        }
    } else if ctx.env.gutter_at_top {
        top += s.gutter;
    } else if ctx.env.mirror_margins {
        if odd {
            *(if vertical { &mut right } else { &mut left }) += s.gutter;
        } else {
            std::mem::swap(&mut left, &mut right);
            *(if vertical { &mut left } else { &mut right }) += s.gutter;
        }
    } else {
        *(if vertical { &mut right } else { &mut left }) += s.gutter;
    }
    (left, right, top)
}

pub fn story_ref(
    sections: &[Section],
    even_and_odd: bool,
    si: usize,
    page_in_section: usize,
    page_number: i64,
    footer: bool,
) -> (usize, Option<&String>) {
    story_ref_in(sections, MarkupView::All, even_and_odd, si, page_in_section, page_number, footer)
}

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

fn glyph_runs(children: &[Child], dx: f64, dy: f64, out: &mut Vec<(f64, f64, f64)>) {
    let item = |it: &Item, dx: f64, dy: f64, out: &mut Vec<(f64, f64, f64)>| {
        if let Item::Glyphs(r) = it {
            out.push((r.glyphs.first().map_or(0.0, |g| g.1) + dx, r.baseline + dy, r.size));
        }
    };
    for c in children {
        match c {
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

#[derive(Clone, Debug)]
pub enum Cursor {
    Section { si: usize, carry: FlowState, y: Option<f64> },
    Body { si: usize, token: PageToken, top: Option<f64> },
    Compose(usize),
    Done,
}

pub struct PassRun {
    pub k: usize,
    pub known: Rc<PassInfo>,
    pub hf_in: Rc<Vec<HfHeights>>,
    pub clip: bool,
    pub pages: Vec<PageRecord>,
    pub page_number: i64,
    pub memo: PassMemo,
    pub cursor: Cursor,
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

pub enum Next {
    Page(PageToken),
    SectionEnd(FlowState, f64),
}

pub struct Paginator<'d, 'p> {
    pub ctx: LayoutCtx<'d>,
    pub sections: &'d [Section],
    pub stories: &'d std::collections::BTreeMap<String, Vec<Block>>,
    pub notes: Notes<'d>,
    pub note_numbers: &'p HashMap<(NoteKind, i64), String>,
    /// Each body paragraph's chapter number for its page numbers.
    pub chapters: Option<Rc<HashMap<super::ids::ParaId, String>>>,
    pub run: &'p mut PassRun,
}

impl<'d> Paginator<'d, '_> {
    pub fn fields(&self, si: usize) -> FieldValues {
        FieldValues {
            page: Some(self.run.page_number.to_string()),
            num_pages: self.run.known.num_pages.clone(),
            section: Some((si + 1).to_string()),
            section_pages: self.run.known.section_pages.get(si).cloned(),
            page_refs: Some(self.run.known.page_refs.clone()),
            style_refs: Vec::new(),
            page_fmt: self.sections[si].pg_fmt.clone(),
            chapter: None,
            chapters: self.chapters.clone(),
        }
    }

    fn page_in_section(&self, si: usize) -> usize {
        self.run.pages.iter().filter(|p| p.section == si && !p.blank).count()
    }

    pub fn new_page(&mut self, si: usize, blank: bool) {
        self.run.page_number += 1;
        let first = self.page_in_section(si) == 0;
        let s = self.sections[si].section_in(self.ctx.env.text.review.view);
        let page_borders = self.sections[si].page_borders.as_ref().filter(|b| match b.display.as_str() {
            "firstPage" => first,
            "notFirstPage" => !first,
            _ => true,
        });
        let page_borders = page_borders.filter(|_| !blank).map(|b| {
            let (left, right, _) = margins(&self.sections[si], &self.ctx, self.run.page_number);
            let out = |x: &Option<crate::docx::props::Border>| {
                x.as_ref().filter(|x| x.visible()).map_or(0.0, |x| x.space.clamp(0.0, 31.0) + super::table::thick(x))
            };
            let rect = if b.offset_from_page {
                // From the page's edge: each line `space` in from it.
                let inn = |x: &Option<crate::docx::props::Border>| x.as_ref().filter(|x| x.visible()).map_or(0.0, |x| x.space.clamp(0.0, 31.0));
                [inn(&b.left), inn(&b.top), s.page_w - inn(&b.right), s.page_h - inn(&b.bottom)]
            } else {
                // From the text: each line `space` out from the margins (around the header and footer when they're surrounded).
                let top = if b.surround_header { s.header.min(s.top.abs()) } else { s.top.abs() };
                let bottom = s.page_h - if b.surround_footer { s.footer.min(s.bottom.abs()) } else { s.bottom.abs() };
                [left - out(&b.left), top - out(&b.top), s.page_w - right + out(&b.right), bottom + out(&b.bottom)]
            };
            (b.clone(), rect)
        });
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
            page_borders,
        });
    }

    fn measure_hf(&self, si: usize, memos: &mut Memos) -> Result<HfHeights, Yield> {
        let view = self.ctx.env.text.review.view;
        let sec = &self.sections[si];
        if self.run.pages.last().is_some_and(|p| p.blank) {
            return Ok([None, None]);
        }
        let (left, right, _) = margins(sec, &self.ctx, self.run.page_number);
        let s = sec.section_in(view);
        let width = s.page_w - left - right;
        let frames = self.hf_frames(si);
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
            let at = |l: &super::story::StoryLayout| if footer { s.page_h - s.footer - stack_limited(l, 0.0, 0.0, limit).1 } else { s.header };
            let layout = super::story::hf_layout(&blocks, width, &frames, at, &env, memos)?;
            out[k] = Some(stack_limited(&layout, left, if footer { 0.0 } else { s.header }, limit).1);
        }
        Ok(out)
    }

    /// The page the current page's header and footer floats are placed on.
    fn hf_frames(&self, si: usize) -> super::float::Frames {
        let sec = &self.sections[si];
        let (left, right, top) = margins(sec, &self.ctx, self.run.page_number);
        let s = sec.section_in(self.ctx.env.text.review.view);
        super::float::Frames {
            page_w: s.page_w,
            page_h: s.page_h,
            margin: [left, right, top, s.bottom],
            column: (left, s.page_w - left - right),
            compat: self.ctx.env.text.compat,
            cell: false,
            odd: self.run.page_number % 2 == 1,
        }
    }

    fn hf_obstacles(&self, si: usize, hf: &HfHeights, memos: &mut Memos) -> Result<Vec<super::float::Placed>, Yield> {
        let view = self.ctx.env.text.review.view;
        let sec = &self.sections[si];
        if self.run.pages.last().is_some_and(|p| p.blank) {
            return Ok(Vec::new());
        }
        let (left, right, _) = margins(sec, &self.ctx, self.run.page_number);
        let s = sec.section_in(view);
        let width = s.page_w - left - right;
        let frames = self.hf_frames(si);
        let pis = self.page_in_section(si).saturating_sub(1);
        let fields = self.fields(si);
        let env = StoryEnv { ctx: self.ctx, fields: &fields, notes: self.note_numbers };
        let mut out = Vec::new();
        for (k, footer) in [false, true].into_iter().enumerate() {
            let Some(blocks) = story_ref_in(self.sections, view, self.ctx.env.even_and_odd, si, pis, self.run.page_number, footer)
                .1
                .and_then(|p| self.stories.get(p))
            else {
                continue;
            };
            let blocks: Vec<&Block> = blocks.iter().collect();
            let tables = blocks.iter().any(|b| matches!(b, Block::Table(t) if t.floating.is_some()));
            if !tables && !blocks.iter().any(|b| matches!(b, Block::Para(p) if !self.ctx.floats_of(p).is_empty())) {
                continue;
            }
            let limit = hf_limit(self.ctx.env.text.compat, s.page_h);
            let top = |l: &super::story::StoryLayout| if footer { s.page_h - s.footer - stack_limited(l, 0.0, 0.0, limit).1 } else { s.header };
            let layout = super::story::hf_layout(&blocks, width, &frames, top, &env, memos)?;
            let at = if footer { s.page_h - s.footer - hf[1].unwrap_or(layout.height) } else { s.header };
            for (i, (f, r, para)) in super::story::hf_float_rects(&blocks, &layout, &frames, at, &env, memos)?.into_iter().enumerate() {
                let p = super::float::Placed::new((usize::MAX, k, i), para, &f, r[0], r[1], r[3] - r[1]);
                if p.wrap != super::float::Wrap::None {
                    out.push(p);
                }
            }
            // Its floating tables, in both modes.
            for (i, t) in layout.tables.iter().enumerate() {
                let key = (usize::MAX, 2 + 2 * i + k, super::float::TABLE_PIECE);
                out.push(super::float::Placed::new(key, t.anchor, &t.float, left + t.rect[0], at + t.rect[1], t.rect[3] - t.rect[1]));
            }
        }
        Ok(out)
    }

    /// Whether `w:mirrorIndents` paragraphs mirror on the current page: even pages in compat 15, odd ones in compat
    /// ≤ 14, counted from the first page (which has its number's parity) whatever later sections restart.
    fn mirrors_indents(&self) -> bool {
        let first = &self.sections[0];
        let mut n = first.pg_start.unwrap_or(1);
        if matches!((first.break_kind, n.rem_euclid(2)), (SectionBreak::EvenPage, 1) | (SectionBreak::OddPage, 0)) {
            n += 1;
        }
        let odd = (n + self.run.pages.len() as i64 - 1).rem_euclid(2) == 1;
        odd == (self.ctx.env.text.compat == Compat::Legacy)
    }

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
            body_top = -s.top;
        }
        if s.bottom < 0.0 {
            body_bottom = s.page_h + s.bottom;
        }
        if let Some(v) = super::vertical::of_section(sec.text_flow) {
            // On the page turned a quarter: lines from the top margin to the bottom one, stacked from the right
            // margin (the left one for lines stacked left to right).
            let (first, last) = if v.flow == super::vertical::Flow::DownEast { (left, s.page_w - right) } else { (right, s.page_w - left) };
            let area = BodyArea { left: top, width: s.page_h - top - s.bottom, top: Lu::round_half_up(first), bottom: Lu::round_half_up(last) };
            return Ok(BodySpace {
                section: si,
                page_w: s.page_h,
                page_h: s.page_w,
                margins: (top, s.bottom, first),
                area,
                columns: columns(&s.columns, &area, start.unwrap_or(area.top.to_pt())),
                mid_page: start.is_some(),
                balance,
                next_continuous: self.sections.get(si + 1).is_some_and(|n| super::vertical::same_page(sec, n)),
                hold_notes: false,
                notes_closed: false,
                hf_floats: Vec::new(),
                odd: self.run.page_number % 2 == 1,
                mirror: self.mirrors_indents(),
                hf,
            });
        }
        // Compat ≤ 14 with `footnoteLayoutLikeWW8`: a section going on on the page ends above the page-bottom
        // footnotes of the sections before it.
        let notes_top = match start {
            Some(_) if self.ctx.env.notes_like_ww8 => {
                self.run.pages.last().and_then(|p| p.bodies.iter().filter_map(|b| b.effects.notes_top).reduce(f64::min))
            }
            _ => None,
        };
        let area = BodyArea {
            left,
            width,
            top: Lu::round_half_up(body_top),
            bottom: Lu::round_half_up(notes_top.map_or(body_bottom, |t| t.min(body_bottom))),
        };
        let hf_floats = self.hf_obstacles(si, &hf, memos)?;
        Ok(BodySpace {
            section: si,
            page_w: s.page_w,
            page_h: s.page_h,
            margins: (left, right, top),
            area,
            columns: columns(&s.columns, &area, start.unwrap_or(area.top.to_pt())),
            mid_page: start.is_some(),
            balance,
            next_continuous: self.sections.get(si + 1).is_some_and(|n| super::vertical::same_page(sec, n)),
            hold_notes: self.ctx.env.text.compat == Compat::Generic && self.sections.get(si + 1).is_some_and(|n| super::vertical::same_page(sec, n)),
            notes_closed: notes_top.is_some(),
            hf_floats,
            odd: self.run.page_number % 2 == 1,
            mirror: self.mirrors_indents(),
            hf,
        })
    }

    pub fn body(&mut self, sec: &SectionNode<'d>, token: PageToken, top: Option<f64>, balance: bool, memos: &mut Memos) -> Result<Next, Yield> {
        let si = sec.index;
        let space = self.body_space(si, top, balance, memos)?;
        let fields = self.fields(si);
        let earlier: Vec<Rc<BoxFragment>> = self.run.pages.last().map(|p| p.bodies.iter().map(|b| b.body.clone()).collect()).unwrap_or_default();
        let env = BodyEnv { ctx: self.ctx, notes: self.notes, note_numbers: self.note_numbers, pass: &mut self.run.memo };
        let b = layout_body(sec, &space, &fields, &token, &earlier, env, memos)?;
        let start_table = match (&token.flow.at, sec.blocks.get(token.flow.block)) {
            (super::token::At::Row(_) | super::token::At::RowPart { .. }, Some(Block::Table(t))) => self.run.memo.table(t, space.columns[0].width),
            _ => None,
        };
        let page = self.run.pages.last_mut().expect("a page was started");
        let band = match super::vertical::of_section(sec.section.text_flow) {
            Some(_) => (space.area.left, space.area.left + space.area.width),
            None => (space.area.top.to_pt(), space.area.bottom.to_pt()),
        };
        page.body_area = page.body_area.or(Some(band));
        page.effects.merge(&b.effects);
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
                stats.push(ColStats::of(&r.children, r.bottom - top + hang(&pos, &carry)));
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
                end = end.max(bottom + hang(&pos, &carry));
                if fill.at.forced {
                    return Ok((None, stats));
                }
                if s == Stop::Done || at_mark {
                    return Ok((Some((pos, carry, children, end.max(fill.at.notes_bottom), fill.at.effects.clone())), stats));
                }
                if k + 1 < cols.len() {
                    carry.keep_top = s == Stop::Column && carry.keep_top || s == Stop::Full && mid;
                }
            }
            Ok((None, stats))
        };
        let full = cols[0].bottom.to_pt() - top;
        let (first, stats) = try_h(fill, full, memos)?;
        if first.is_none() {
            fill.at = state;
            return Ok(None);
        }
        let full_lu = i64::from(Lu::round_half_up(full).0);
        let widths: Vec<f64> = cols.iter().map(|c| c.width).collect();
        let wsum: f64 = widths.iter().sum::<f64>().max(1.0);
        let to_pt = |h: i64| h as f64 / LU_PER_PT;
        let mut h;
        let mut r;
        if fill.ctx.env.text.compat == Compat::Legacy {
            // Compat ≤ 14: from the unbalanced fill's column heights weighted by the columns' widths, never below its
            // mean line height, the height grows while content is left over by the failed fill's mean line height,
            // each column's height counted at most up to the height tried. The section ends at that height or at its
            // tallest column, whichever is less.
            let (lines, total) = stats.iter().fold((0, 0), |(n, t), s| (n + s.lines, t + s.used));
            h = (widths.iter().zip(&stats).map(|(w, s)| w * s.used as f64).sum::<f64>() / wsum) as i64;
            h = h.max(total / lines.max(1)).min(full_lu);
            r = try_h(fill, to_pt(h), memos)?;
            while r.0.is_none() && h < full_lu {
                let (lines, used) = r.1.iter().fold((0, 0), |(n, t), s| (n + s.lines, t + s.used.min(h)));
                h = (h + if lines > 0 { (used / lines).max(1) } else { 49152 }).min(full_lu);
                r = try_h(fill, to_pt(h), memos)?;
            }
            if let Some(done) = &mut r.0 {
                done.3 = done.3.min(top + to_pt(h)).max(fill.at.notes_bottom);
            }
        } else {
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
        c.prev = None;
        c.balanced_after = c.prev_after;
        c.prev_after = 0.0;
        Ok(Some((end, children, p, c, eff)))
    }

    fn draw_floats(fill: &PageFill, placed: &[super::float::Placed], memos: &mut Memos) -> Result<Vec<Child>, Yield> {
        let fields = FieldValues::default();
        let env = StoryEnv { ctx: fill.ctx, fields: &fields, notes: fill.note_numbers };
        placed
            .iter()
            .filter(|p| !matches!(p.key.2, super::float::TABLE_PIECE | super::float::UPRIGHT_TABLE))
            .map(|p| super::story::float_box(&p.float, p.rect, p.anchor, &env, memos))
            .collect()
    }
}

pub(crate) fn fields_used(fields: &BodyFields, blocks: &[&Block]) -> BodyFields {
    fn walk(blocks: &mut dyn Iterator<Item = &Block>, out: &mut BodyFields, all: &BodyFields) {
        for b in blocks {
            match b {
                Block::Para(p) => {
                    for i in &p.inlines {
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
                                out.chapter.clone_from(&all.chapter);
                                out.chapters.clone_from(&all.chapters);
                            }
                            FieldKind::NumPages => out.num_pages.clone_from(&all.num_pages),
                            FieldKind::Section => out.section.clone_from(&all.section),
                            FieldKind::SectionPages => out.section_pages.clone_from(&all.section_pages),
                            FieldKind::StyleRef(_) => out.style_refs.clone_from(&all.style_refs),
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
    let vertical = super::vertical::of_section(s.text_flow);
    let ctx = match (s.line_grid, vertical) {
        (None, None) => env.ctx,
        (g, vertical) => {
            let text = super::env::TextEnv { line_grid: g.or(env.ctx.env.text.line_grid), vertical, ..env.ctx.env.text.clone() };
            grid_env = super::LayoutEnv { text, ..env.ctx.env.clone() };
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
            super::job::unit("page attempt", || {
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
                for (k, col) in cols.iter().enumerate() {
                    let page_empty = k == 0 && !space.mid_page;
                    fill.column = k;
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
                        carry.keep_top = st == Stop::Column && carry.keep_top || st == Stop::Full && space.mid_page;
                    }
                }
                let found = fill.at.found.clone();
                let notes = fill.at.stream.as_ref().map_or(0.0, |a| a.height);
                // A float that met earlier text only in this fill is seen by that text in the next.
                let stable = found.len() == known.len() && found.iter().all(|f| known.iter().any(|k| k.same(f) && k.collided >= f.collided));
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
                    end = Some(if starts_here { FlowToken { block: k.key.1, at: super::token::At::Start } } else { pos.clone() });
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
    let starts_area = |a: &NoteArea| !a.parts.is_empty() || held.is_some() || sec.blocks.blocks.iter().any(|b| super::document::has_footnote_refs(b));
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
    let floats = crate::prof::time("layout:floats", || Paginator::draw_floats(&fill, &placed, memos))?;
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
            match Paginator::balance(&mut fill, sec, cols, &saved, space.mid_page, template.as_ref(), memos)? {
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
    let mut texts = Vec::new();
    super::traverse::lines_in(&body, &mut |para, l, dx, dy| {
        for (r, pic) in l.line.texts.iter() {
            let (ox, oy) = (dx + l.x, dy + l.y);
            texts.push((para.unwrap_or(super::ids::ParaId(0)), [r[0] + ox, r[1] + oy, r[2] + ox, r[3] + oy], pic.clone()));
        }
    });
    if !texts.is_empty() {
        let fields = super::text::FieldValues::default();
        let senv = super::story::StoryEnv { ctx: fill.ctx, fields: &fields, notes: fill.note_numbers };
        for (para, rect, pic) in texts {
            body.extend(super::story::inline_text_box(&pic, rect, para, &senv, memos)?);
        }
    }
    let shown = super::expansion::shown(&body);
    body.extend(shown);
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
    let (size, children) = match vertical {
        Some(v) => {
            let mut turned = super::vertical::turn_body(&body, v, [space.page_h, space.page_w]);
            turned.append(&mut fill.at.upright);
            ((space.page_h, space.page_w), turned.into())
        }
        None if s.text_flow == 4 => ((space.page_w, space.page_h), super::vertical::turn_east_asian(&body).into()),
        None => ((space.page_w, space.page_h), body.into()),
    };
    let fragment = Rc::new(BoxFragment { kind: BoxKind::Body, id: Some(FragmentId::next()), offset: (0.0, 0.0), size, children });
    Ok(PageBody { fragment, end: PageToken { section: space.section, flow: pos, state: carry }, section_end, effects })
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
fn change_bars(space: &BodySpace, body: &mut Vec<Child>) {
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
            _ => bars.push(Item::rect([x, top, x + 0.72, bottom], 0)),
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
