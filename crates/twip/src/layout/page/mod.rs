//! Page bodies

use crate::docx::Compat;
use std::collections::HashMap;
use std::rc::Rc;

use crate::docx::model::*;
use crate::docx::review::MarkupView;

use super::LayoutCtx;
use super::flow::PassMemo;
use super::fragment::{BoxFragment, BoxKind, Child, Item};
use super::job::{Memos, Yield};
use super::node::{Notes, SectionNode};
use super::record::{BodyRecord, PageRecord};
use super::space::{BodyArea, BodyFields, BodySpace, ColumnSpace, HfHeights};
use super::story::{StoryEnv, hf_limit, stack_limited};
use super::text::FieldValues;
use super::token::{FlowState, PageToken};
use super::units::Lu;

mod body;

use body::{BodyEnv, layout_body};

#[derive(Clone, Default, PartialEq, Debug)]
pub struct PassInfo {
    pub num_pages: Option<String>,
    pub section_pages: Vec<String>,
    pub page_refs: Rc<HashMap<String, crate::layout::text::PageRef>>,
    /// The page and place on it of each paragraph on the pages holding reference fields with `\p`.
    pub para_at: Rc<HashMap<super::ids::ParaId, (usize, usize)>>,
}

pub fn margins(s: &Section, ctx: &LayoutCtx, page_number: i64) -> (f64, f64, f64) {
    let env = ctx.env;
    let facing = Facing { mirror: env.mirror_margins, gutter_at_top: env.gutter_at_top, book_fold: env.book_fold };
    margins_in(s, env.text.review.view, facing, page_number)
}

/// The document settings that place a page's gutter.
#[derive(Clone, Copy, Debug)]
pub struct Facing {
    /// Mirrored margins: an even page swaps its left and right.
    pub mirror: bool,
    /// The gutter is above the top margin.
    pub gutter_at_top: bool,
    /// Book fold, reversed when `true`.
    pub book_fold: Option<bool>,
}

/// Whether page `page_number` of section `s` shows the section's left margin on its right: a mirrored even page, or
/// a landscape book fold's page on the other side.
pub fn swaps_margins(s: &crate::docx::SectionPage, facing: Facing, page_number: i64) -> bool {
    let odd = page_number % 2 == 1;
    match facing.book_fold {
        Some(rev) => s.page_w > s.page_h && odd != rev,
        None => !facing.gutter_at_top && facing.mirror && !odd,
    }
}

/// Page `page_number`'s left, right and top margins in section `s` as `view` shows it, its gutter added.
pub fn margins_in(s: &Section, view: MarkupView, facing: Facing, page_number: i64) -> (f64, f64, f64) {
    // A vertical section's gutter is on the right (the left on a mirrored even page), and so is a horizontal one's
    // with `w:rtlGutter`.
    let vertical = super::vertical::of_section(s.text_flow).is_some() || s.rtl_gutter;
    let s = s.section_in(view);
    let (mut left, mut right, mut top) = (s.left, s.right, s.top);
    let odd = page_number % 2 == 1;
    if let Some(rev) = facing.book_fold {
        // Book fold: portrait pages drop the gutter; landscape ones mirror with it.
        if s.page_w > s.page_h {
            if odd != rev {
                (left, right) = (right + s.gutter, left);
            } else {
                right += s.gutter;
            }
        }
    } else if facing.gutter_at_top {
        top += s.gutter;
    } else if facing.mirror {
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
    let n = c.num.clamp(1, Columns::MAX);
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
        for (w, sp) in c.cols.iter().take(Columns::MAX) {
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
            Child::Items(items) | Child::Turned { items, .. } => {
                for it in items.iter() {
                    item(it, dx, dy, out);
                }
            }
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
            section_chapter: None,
            chapters: self.chapters.clone(),
            here: self.run.pages.len().checked_sub(1),
            para_at: Some(self.run.known.para_at.clone()),
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
            bodies: Default::default(),
            section: si,
            effects: Default::default(),
            hf: [None, None],
            hf_boxes: [None, None],
            composed: false,
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
            out[k] = Some(if footer {
                super::story::footer_layout(&blocks, width, &frames, s.page_h - s.footer, limit, &env, memos)?.2
            } else {
                stack_limited(&*super::story::hf_layout(&blocks, width, &frames, |_| s.header, &env, memos)?, left, s.header, limit).1
            });
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

    fn hf_obstacles(&self, si: usize, memos: &mut Memos) -> Result<Vec<super::float::Placed>, Yield> {
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
            let (layout, at) = if footer {
                let (l, at, _) = super::story::footer_layout(&blocks, width, &frames, s.page_h - s.footer, limit, &env, memos)?;
                (l, at)
            } else {
                (super::story::hf_layout(&blocks, width, &frames, |_| s.header, &env, memos)?, s.header)
            };
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
        let mut hf_floats = self.hf_obstacles(si, memos)?;
        // A section going on on the page wraps the floats the sections before it placed there.
        if start.is_some()
            && let Some(p) = self.run.pages.last()
        {
            hf_floats.extend(p.bodies.iter().flat_map(|b| b.effects.floats.iter().cloned()));
        }
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
        Rc::make_mut(&mut page.effects).merge(&b.effects);
        let last = sec.blocks.chain_end(b.end.flow.block, &self.ctx.env.text.review).min(sec.blocks.len().saturating_sub(1));
        let shown = sec.blocks.range(token.flow.block..last + 1);
        let end = b.end.clone();
        let page_h = space.page_h;
        Rc::make_mut(&mut page.bodies).push(BodyRecord {
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
}

pub(crate) fn fields_used<'b>(fields: &BodyFields, blocks: impl IntoIterator<Item = &'b Block>) -> BodyFields {
    fn walk(blocks: &mut dyn Iterator<Item = &Block>, out: &mut BodyFields, all: &BodyFields) {
        for b in blocks {
            match b {
                Block::Para(p) => {
                    for (k, i) in p.inlines.iter().enumerate() {
                        if let Inline::Float(f, _) = i {
                            for tb in f.text_box.iter().chain(f.parts.iter().filter_map(|p| p.text_box.as_ref())) {
                                walk(&mut tb.blocks.iter(), out, all);
                            }
                        }
                        let Inline::FieldBegin { kind: Some(kind), .. } = i else { continue };
                        match kind {
                            FieldKind::Page | FieldKind::PgNum => {
                                out.page.clone_from(&all.page);
                                out.page_fmt.clone_from(&all.page_fmt);
                                out.chapter.clone_from(&all.chapter);
                                out.section_chapter.clone_from(&all.section_chapter);
                                out.chapters.clone_from(&all.chapters);
                            }
                            FieldKind::NumPages => out.num_pages.clone_from(&all.num_pages),
                            FieldKind::Section => out.section.clone_from(&all.section),
                            FieldKind::SectionPages => out.section_pages.clone_from(&all.section_pages),
                            FieldKind::StyleRef(_) => out.style_refs.clone_from(&all.style_refs),
                            FieldKind::PageRef(name) | FieldKind::NoteRef(name, _) => {
                                let relative = crate::docx::fields::relative_at(&p.inlines, k);
                                if matches!(kind, FieldKind::NoteRef(..)) && !relative {
                                    continue;
                                }
                                let value = all.page_refs.as_ref().and_then(|m| m.get(name)).cloned();
                                let mark = value.as_ref().map(|v| v.para);
                                let refs = Rc::make_mut(out.page_refs.get_or_insert_with(Default::default));
                                if let Some(v) = value {
                                    refs.insert(name.clone(), v);
                                }
                                // `\p`: where the field's own page is, and where its paragraph and its bookmark's are.
                                if relative {
                                    out.here = all.here;
                                    let at = Rc::make_mut(out.para_at.get_or_insert_with(Default::default));
                                    for q in std::iter::once(super::ids::ParaId(p.id)).chain(mark) {
                                        if let Some(v) = all.para_at.as_ref().and_then(|m| m.get(&q)) {
                                            at.insert(q, *v);
                                        }
                                    }
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
    walk(&mut blocks.into_iter(), &mut out, fields);
    out
}
