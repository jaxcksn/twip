//! Incremental layout

use std::collections::HashMap;
use std::rc::Rc;

use crate::docx::model::*;
use crate::docx::review::ReviewView;
use crate::font::Fonts;

use super::LayoutCtx;
use super::cache::LayoutCache;
use super::document::{Finished, LayoutJob, PassCx, finish_page, hf_fields, hf_parts};
use super::env::LayoutEnv;
use super::fragment::PageFragment;
use super::ids::{FragmentId, LayoutRevision, ParaId, Revision, Unit};
use super::job::{Budget, Memos, Step, Yield};
use super::join::{chain_end, chain_start};
use super::line::border_key;
use super::page::{Cursor, PassInfo, PassRun, fields_used};
use super::record::{HfRecord, PageRecord};
use super::snapshot::{LayoutStatus, PageMeta, Snapshot};
use super::space::BodyFields;
use super::text::same_para;
use super::token::{At, PageToken};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Change {
    pub section: usize,
    pub first: usize,
    pub old_end: usize,
    pub new_end: usize,
    pub row: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Page(usize),
    Position {
        para: ParaId,
        unit: Unit,
    },
    /// [`Target::Position`] for a body paragraph where the caller knows it: block `block` of section `section`, or
    /// in row `row` of the table there.
    Block {
        section: usize,
        block: usize,
        row: Option<usize>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Span {
    section: usize,
    first: usize,
    old_end: usize,
    new_end: usize,
    row: Option<usize>,
    first_row: usize,
}

impl Span {
    fn delta(&self) -> isize {
        self.new_end as isize - self.old_end as isize
    }

    fn merge(self, c: Span) -> Option<Span> {
        if c.section != self.section {
            return None;
        }
        let d = self.delta();
        let end = self.new_end.max(c.old_end);
        let old_end = (self.old_end as isize).max(c.old_end as isize - d).max(0) as usize;
        let new_end = (end as isize + c.delta()).max(0) as usize;
        let one = |s: &Span| s.first + 1 == s.old_end && s.old_end == s.new_end;
        let row = match (self.row, c.row) {
            (Some(a), Some(b)) if one(&self) && one(&c) && self.first == c.first => Some(a.max(b)),
            _ => None,
        };
        let first_row = self.first_row.min(c.first_row);
        Some(Span { section: self.section, first: self.first.min(c.first), old_end, new_end, row, first_row })
    }
}

#[derive(Clone, Debug, Default)]
struct Accumulated {
    span: Option<Span>,
    unknown: bool,
    full: bool,
    /// Header and footer parts changed.
    parts: bool,
}

type Shadow = Vec<Vec<Option<Block>>>;

struct Extras {
    sections: Vec<Section>,
    stories: std::collections::BTreeMap<HeaderFooterId, Vec<Block>>,
    footnotes: HashMap<i64, Vec<Block>>,
    endnotes: HashMap<i64, Vec<Block>>,
    separators: HashMap<String, Vec<Block>>,
    env: LayoutEnv,
    fonts: u64,
    endnote_order: Vec<i64>,
    endnotes_sect_end: bool,
    body_fields: bool,
}

fn props(s: &Section) -> Section {
    s.settings()
}

/// Whether section `b` has the settings `a` holds (its [`props`]), without making them.
fn same_props(a: &Section, b: &Section) -> bool {
    let Section {
        break_kind,
        page_w,
        page_h,
        top,
        bottom,
        left,
        right,
        header,
        footer,
        gutter,
        columns,
        title_pg,
        headers,
        footers,
        pg_start,
        pg_fmt,
        chapter,
        line_numbers,
        v_align,
        page_borders,
        page_borders_direct,
        line_grid,
        notes,
        no_endnote,
        text_flow,
        rtl_gutter,
        footnote_columns,
        form_protected,
        blocks: _,
        change,
        origin: _,
    } = a;
    *break_kind == b.break_kind
        && [*page_w, *page_h, *top, *bottom, *left, *right, *header, *footer, *gutter]
            == [b.page_w, b.page_h, b.top, b.bottom, b.left, b.right, b.header, b.footer, b.gutter]
        && *columns == b.columns
        && *title_pg == b.title_pg
        && *headers == b.headers
        && *footers == b.footers
        && *pg_start == b.pg_start
        && *pg_fmt == b.pg_fmt
        && *chapter == b.chapter
        && *line_numbers == b.line_numbers
        && *v_align == b.v_align
        && *page_borders == b.page_borders
        && *page_borders_direct == b.page_borders_direct
        && *line_grid == b.line_grid
        && *notes == b.notes
        && *no_endnote == b.no_endnote
        && *text_flow == b.text_flow
        && *rtl_gutter == b.rtl_gutter
        && *footnote_columns == b.footnote_columns
        && *form_protected == b.form_protected
        && *change == b.change
}

impl Extras {
    fn new(doc: &Document, fonts: &Fonts, endnote_order: &[i64], env: &LayoutEnv) -> Extras {
        Extras {
            sections: doc.sections.iter().map(props).collect(),
            stories: doc.stories.clone(),
            footnotes: doc.footnotes.clone(),
            endnotes: doc.endnotes.clone(),
            separators: doc.separators.clone(),
            env: env.clone(),
            fonts: fonts.revision(),
            endnote_order: endnote_order.to_vec(),
            endnotes_sect_end: doc.settings.endnotes_sect_end,
            body_fields: crate::docx::fields::page_dependent(doc),
        }
    }

    /// The header and footer parts that changed, when nothing else did.
    fn changed_parts(&self, doc: &Document, fonts: &Fonts, env: &LayoutEnv) -> Option<Vec<HeaderFooterId>> {
        let rest = self.fonts == fonts.revision()
            && self.endnotes_sect_end == doc.settings.endnotes_sect_end
            && self.sections.len() == doc.sections.len()
            && self.sections.iter().zip(&doc.sections).all(|(a, b)| same_props(a, b))
            && self.env == *env
            && self.footnotes == doc.footnotes
            && self.endnotes == doc.endnotes
            && self.separators == doc.separators
            && self.stories.len() == doc.stories.len();
        if !rest {
            return None;
        }
        let mut changed = Vec::new();
        for ((name, was), (now_name, now)) in self.stories.iter().zip(&doc.stories) {
            if name != now_name {
                return None;
            }
            if was != now {
                changed.push(*name);
            }
        }
        Some(changed)
    }
}

type LatestTable = (usize, usize, u64, Rc<super::table::TableLayout>);

struct Baseline {
    finished: Rc<Finished>,
    extras: Extras,
    shadow: Shadow,
    note_numbers: HashMap<(NoteKind, i64), String>,
    notes: super::document::JobNotes,
    table: Option<LatestTable>,
}

fn same_block(a: &Block, b: &Block) -> bool {
    match (a, b) {
        (Block::Para(p), Block::Para(q)) => same_para(p, q),
        (Block::Table(s), Block::Table(t)) => {
            super::table::same_shell(s, t)
                && s.rows.iter().zip(&t.rows).all(|(r, q)| {
                    r.cells
                        .iter()
                        .zip(&q.cells)
                        .all(|(c, d)| c.blocks.len() == d.blocks.len() && c.blocks.iter().zip(&d.blocks).all(|(x, y)| same_block(x, y)))
                })
        }
        _ => false,
    }
}

const SHADOW_CELLS: usize = 1000;

fn shadow_of(b: &Block) -> Option<Block> {
    match b {
        Block::Table(t) if t.rows.iter().map(|r| r.cells.len()).sum::<usize>() > SHADOW_CELLS => None,
        _ => Some(b.clone()),
    }
}

/// Every block, the largest tables too: a full layout clones them once (about 4 ms for 10,000 cells against a second to
/// lay them out), and unhinted edits then compare against them. [`shadow_of`] leaves out the large tables an edit lays
/// out again, so typing in one doesn't clone it each key.
fn shadow(doc: &Document) -> Shadow {
    doc.sections.iter().map(|s| s.blocks.iter().map(|b| Some(b.clone())).collect()).collect()
}

fn compare(doc: &Document, shadow: &Shadow) -> Result<Option<Span>, ()> {
    if doc.sections.len() != shadow.len() {
        return Err(());
    }
    let mut found = None;
    for (si, (s, old)) in doc.sections.iter().zip(shadow).enumerate() {
        let new = &s.blocks;
        let same = |o: &Option<Block>, n: &Block| o.as_ref().is_some_and(|o| same_block(o, n));
        let n = new.len().min(old.len());
        let first = (0..n).find(|&i| !same(&old[i], &new[i])).unwrap_or(n);
        if first == n && new.len() == old.len() {
            continue;
        }
        let tail = (0..n - first).take_while(|&k| same(&old[old.len() - 1 - k], &new[new.len() - 1 - k])).count();
        if found.is_some() {
            return Err(());
        }
        found = Some(Span { section: si, first, old_end: old.len() - tail, new_end: new.len() - tail, row: None, first_row: 0 });
    }
    Ok(found)
}

fn close(span: Span, blocks: &[Block], review: &ReviewView) -> Span {
    let view = review.view;
    let bordered = |i: usize| matches!(blocks.get(i), Some(Block::Para(p)) if border_key(p.para_style_in(view)).is_some());
    let mut s = span;
    let first = chain_start(blocks, s.first, review);
    if first < s.first {
        s.first = first;
        s.row = None;
    }
    if s.first > 0 && bordered(s.first - 1) {
        s.first = chain_start(blocks, s.first - 1, review);
        s.row = None;
    }
    let next = chain_end(blocks, s.new_end, review);
    if s.new_end < blocks.len() && bordered(next) {
        s.old_end += next + 1 - s.new_end;
        s.new_end = next + 1;
        s.row = None;
    }
    s
}

fn restart_page(base: &[PageRecord], span: &Span, blocks: &[Block], review: &ReviewView) -> usize {
    let view = review.view;
    let mut g = chain_start(blocks, span.first.min(blocks.len()), review);
    while g > 0 && matches!(&blocks[g - 1], Block::Para(p) if p.para_style_in(view).keep_next) {
        g = chain_start(blocks, g - 1, review);
    }
    let before = |pg: &PageRecord| pg.bodies.first().is_some_and(|b| (b.token.section, b.token.flow.block) < (span.section, g));
    let mut r = base.iter().rposition(before).unwrap_or(0);
    while r > 0 && base[r].bodies.is_empty() {
        r -= 1;
    }
    r
}

#[allow(clippy::too_many_arguments)]
fn row_restart(
    base: &[PageRecord],
    span: &Span,
    doc: &Document,
    fonts: &Fonts,
    notes: &HashMap<(NoteKind, i64), String>,
    env: &LayoutEnv,
    cache: &mut LayoutCache,
    memo: &mut super::flow::PassMemo,
    latest: Option<&LatestTable>,
    relaid: &mut Option<LatestTable>,
    preview: Option<&super::Preview>,
    expansions: Option<&super::expansion::Expansions>,
) -> Option<usize> {
    let row = span.row?;
    let blocks = &doc.sections.get(span.section)?.blocks;
    let Some(Block::Table(table)) = blocks.get(span.first) else { return None };
    let prev = match span.first.checked_sub(1).and_then(|i| blocks.get(i)) {
        Some(Block::Para(p)) => Some(&**p.para_style_in(env.text.review.view)),
        _ => None,
    };
    fn inside<'p>(pg: &'p PageRecord, span: &Span) -> Option<&'p super::record::BodyRecord> {
        pg.bodies.first().filter(|b| (b.token.section, b.token.flow.block) == (span.section, span.first) && b.start_table.is_some())
    }
    let first_row = span.first_row.min(row);
    let key = (span.section, span.first, 2u8, first_row, 0);
    let p = base.iter().rposition(|pg| inside(pg, span).is_some_and(|b| start_key(&b.token) < key));
    let rec = p.or_else(|| base.iter().position(|pg| inside(pg, span).is_some()))?;
    let width = base[rec].bodies[0].space.columns[0].width;
    let ctx = LayoutCtx { fonts, env, preview, expansions };
    let fields = super::text::FieldValues::default();
    let story = super::story::StoryEnv { ctx, fields: &fields, notes };
    cache.store_tables = false;
    let mut meter = super::job::Meter::unlimited();
    let mut memos = Memos { cache, meter: &mut meter };
    let new = match latest.filter(|l| (l.0, l.1, l.2) == (span.section, span.first, width.to_bits())) {
        Some(l) => super::table::relayout_rows(table, width, &story, prev, &l.3, first_row..=row, &mut memos),
        None => super::table::layout(table, width, &story, prev, false, &mut memos),
    }
    .ok();
    memos.cache.store_tables = true;
    let new = new?;
    *relaid = Some((span.section, span.first, width.to_bits(), new.clone()));
    memo.seed(table, width, new.clone());
    let p = p?;
    let old = base[p].bodies[0].start_table.as_ref()?;
    if first_row < old.rows.iter().take_while(|r| r.header).count() {
        return None;
    }
    old.keeps_rows_above(&new, first_row).then_some(p)
}

fn start_key(t: &PageToken) -> (usize, usize, u8, usize, usize) {
    let (kind, i, part) = match &t.flow.at {
        At::Start => (0, 0, 0),
        At::Piece(i, part) => (1, *i, *part),
        At::Row(r) => (2, *r, 0),
        At::RowPart { row, .. } => (3, *row, 0),
    };
    (t.section, t.flow.block, kind, i, part)
}

fn shifted(t: &PageToken, span: Option<&Span>) -> PageToken {
    let mut t = t.clone();
    if let Some(s) = span
        && t.section == s.section
        && t.flow.block >= s.old_end
    {
        t.flow.block = (t.flow.block as isize + s.delta()) as usize;
    }
    t
}

fn same_used(used: &BodyFields, now: &BodyFields) -> bool {
    (used.page.is_none() || (used.page == now.page && used.page_fmt == now.page_fmt && used.chapters == now.chapters))
        && (used.section.is_none() || used.section == now.section)
        && (used.num_pages.is_none() || used.num_pages == now.num_pages)
        && (used.section_pages.is_none() || used.section_pages == now.section_pages)
        && used.page_refs.as_ref().is_none_or(|m| m.iter().all(|(k, v)| now.page_refs.as_ref().and_then(|n| n.get(k)) == Some(v)))
        && (used.here.is_none() || used.here == now.here)
        && used.para_at.as_ref().is_none_or(|m| m.iter().all(|(k, v)| now.para_at.as_ref().and_then(|n| n.get(k)) == Some(v)))
}

#[cfg(test)]
thread_local! {
    // Test hooks
    pub(crate) static FORCE_FIRST_PAGE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    pub(crate) static FULL_JOBS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    pub(crate) static REUSED_PAGES: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

pub struct Reuse {
    base: Rc<Finished>,
    span: Option<Span>,
    restart: usize,
    /// The header and footer parts that changed: pages showing one compose them again.
    parts: Vec<HeaderFooterId>,
    keys: Vec<(usize, usize, u8, usize, usize)>,
    renumbered: std::collections::HashSet<(NoteKind, i64)>,
}

/// Whether a page of `pages` goes on past row `row` of the table at block `block` of section `section`.
fn past_row(pages: &[PageRecord], section: usize, block: usize, row: usize) -> bool {
    pages.iter().any(|pg| {
        pg.bodies.iter().any(|b| {
            let (s, blk, kind, i, _) = start_key(&b.end);
            b.section_end.is_some_and(|_| b.token.section == section)
                || (s, blk) > (section, block)
                || (s, blk) == (section, block) && kind >= 2 && i > row
        })
    })
}

fn table_row_of(doc: &Document, para: ParaId) -> Option<(usize, usize, usize)> {
    doc.sections.iter().enumerate().find_map(|(si, s)| {
        s.blocks.iter().enumerate().find_map(|(bi, b)| match b {
            Block::Table(t) => t.rows.iter().position(|r| r.cells.iter().any(|c| c.blocks.iter().any(|b| holds(b, para)))).map(|r| (si, bi, r)),
            Block::Para(_) => None,
        })
    })
}

fn holds(b: &Block, para: ParaId) -> bool {
    match b {
        Block::Para(p) => p.id == para.0,
        Block::Table(t) => t.rows.iter().any(|r| r.cells.iter().any(|c| c.blocks.iter().any(|b| holds(b, para)))),
    }
}

fn references(blocks: &[Block], notes: &std::collections::HashSet<(NoteKind, i64)>) -> bool {
    blocks.iter().any(|b| match b {
        Block::Para(p) => p.inlines.iter().any(|i| match i {
            Inline::NoteRef { kind, id, .. } => notes.contains(&(*kind, *id)),
            Inline::NoteMark(kind, id, _) => notes.contains(&super::node::mark_key(*kind, *id)),
            Inline::FieldBegin { kind: Some(crate::docx::FieldKind::NoteRef(_, Some(n))), .. } => notes.contains(n),
            _ => false,
        }),
        Block::Table(t) => t.rows.iter().any(|r| r.cells.iter().any(|c| references(&c.blocks, notes))),
    })
}

impl Reuse {
    pub fn reuse_page(&mut self, run: &mut PassRun, cx: &PassCx, si: usize, token: &PageToken, memos: &mut Memos) -> Result<Option<Cursor>, Yield> {
        // Content
        let in_table = self.row_exception(si, token);
        if let Some(s) = &self.span
            && si == s.section
            && token.flow.block < s.new_end
            && in_table.is_none()
        {
            return Ok(None);
        }
        if si < self.span.map_or(0, |s| s.section) {
            return Ok(None);
        }
        let base = &self.base.records;
        let key = start_key(token);
        let from = (self.restart + 1).min(base.len());
        let lo = from + self.keys[from..].partition_point(|k| *k < key);
        let doc = cx.doc;
        let balance = super::document::balances(cx, si, token);
        for (j, pg) in base.iter().enumerate().skip(lo) {
            if self.keys[j] != key {
                break;
            }
            let [body] = &pg.bodies[..] else { continue };
            if let Some(s) = &self.span
                && body.token.section == s.section
                && body.token.flow.block < s.old_end
                && in_table.is_none()
            {
                continue;
            }
            // Continuation
            let t = shifted(&body.token, self.span.as_ref());
            if t != *token {
                continue;
            }
            if body.section_end.is_some() && doc.sections.get(si + 1).is_some_and(|n| n.break_kind == SectionBreak::Continuous) {
                continue;
            }
            // Constraints
            let p = cx.paginator(run);
            let space = p.body_space(si, None, balance, memos)?;
            if space != body.space {
                continue;
            }
            // Effects
            if !same_used(&body.fields, &p.fields(si)) {
                continue;
            }
            if !pg.effects.notes_shown.iter().all(|(k, id, n)| cx.notes.numbers.get(&(*k, *id)) == Some(n)) {
                continue;
            }
            // Row exception
            if let Some(r) = in_table {
                let (Some(old), Some(Block::Table(table))) = (&body.start_table, doc.sections[si].blocks.get(token.flow.block)) else { continue };
                let Some(new) = run.memo.table(table, space.columns[0].width) else { continue };
                // A live drag's table laid out only in part holds no rows to compare past its part.
                let same = new.rows.len() == table.rows.len()
                    && old.same_rows_from(&new, r)
                    && new.rows.get(r).is_some_and(|row| row.cells.iter().all(|c| c.v_merge != VMerge::Continue))
                    && self.span.filter(|s| s.row.is_some()).is_some_and(|s| s.first_row >= new.rows.iter().take_while(|row| row.header).count())
                    && old.rows.iter().take_while(|row| row.header).eq(new.rows.iter().take_while(|row| row.header));
                if !same {
                    continue;
                }
            }
            if !self.renumbered.is_empty() {
                let blocks = &doc.sections[si].blocks;
                let end = shifted(&body.end, self.span.as_ref()).flow.block;
                let a = t.flow.block.min(blocks.len());
                if references(&blocks[a..(end + 1).clamp(a, blocks.len())], &self.renumbered) {
                    continue;
                }
            }
            let mut body = body.clone();
            body.token = t;
            body.end = shifted(&body.end, self.span.as_ref());
            let end = body.end.clone();
            let section_end = body.section_end;
            let page = run.pages.last_mut().expect("a page was started");
            page.body_area = pg.body_area;
            page.effects = pg.effects.clone();
            Rc::make_mut(&mut page.bodies).push(body);
            page.reused = Some(j);
            #[cfg(test)]
            REUSED_PAGES.set(REUSED_PAGES.get() + 1);
            return Ok(Some(match section_end {
                Some(y) => Cursor::Section { si: si + 1, carry: end.state, y: Some(y) },
                None => {
                    cx.paginator(run).new_page(si, false);
                    Cursor::Body { si, token: end, top: None }
                }
            }));
        }
        Ok(None)
    }

    fn row_exception(&self, si: usize, token: &PageToken) -> Option<usize> {
        let s = self.span.as_ref()?;
        let edited = s.row?;
        match token.flow.at {
            At::Row(r) if si == s.section && token.flow.block == s.first && s.first + 1 == s.new_end && s.old_end == s.new_end && edited < r => {
                Some(r)
            }
            _ => None,
        }
    }

    pub fn reuse_composed(&mut self, run: &mut PassRun, cx: &PassCx, i: usize) -> bool {
        let Some(j) = run.pages[i].reused else { return false };
        if run.clip {
            return false;
        }
        let b = &self.base.records[j];
        let pg = &run.pages[i];
        if cx.ctx.env.mirror_margins && (b.number - pg.number) % 2 != 0 {
            return false;
        }
        let parts = hf_parts(run, cx, i);
        let fields = hf_fields(run, cx, i);
        for (part, h) in parts.iter().zip(&b.hf) {
            let same = match (*part, h) {
                (None, None) => true,
                (Some(part), Some(h)) => part == h.part && !self.parts.contains(&part) && fields_used(&fields, &cx.doc.stories[&part]) == h.fields,
                _ => false,
            };
            if !same {
                return false;
            }
        }
        let (hf, boxes, full) = (b.hf.clone(), b.hf_boxes.clone(), b.hf_full);
        let pg = &mut run.pages[i];
        pg.hf = hf;
        pg.hf_boxes = boxes;
        pg.hf_full = full;
        pg.composed = true;
        true
    }
}

struct Running {
    job: LayoutJob,
    reuse: Option<Reuse>,
    span: Option<Span>,
    incremental: bool,
    relaid: Option<LatestTable>,
    seeded: bool,
    /// The pass the published pages are of, and how many of its pages they hold.
    shown: (usize, usize),
}

pub struct DocumentLayout {
    cache: LayoutCache,
    published: Rc<Snapshot>,
    layout_revision: u64,
    revision: Revision,
    baseline: Option<Baseline>,
    /// The last finished layout, converged or not.
    last: Option<Rc<Finished>>,
    acc: Accumulated,
    job: Option<Running>,
    current: bool,
    reused: (usize, usize),
    frames: HashMap<FragmentId, Frame>,
    preview: Option<Rc<super::Preview>>,
    expansions: Option<Rc<super::expansion::Expansions>>,
    /// The environment for the document since the last edit.
    env: Option<Rc<LayoutEnv>>,
    /// The text box chains the last environment found.
    chains: Option<Rc<[super::env::Chain]>>,
    /// The tables a live drag lays out.
    live: Rc<[u64]>,
    /// The hyphenation patterns the host supplied.
    hyphenation: Option<super::hyphenate::Hyphenation>,
    /// The view the document is laid out in.
    view: ReviewView,
}

type Frame = (Rc<PageFragment>, PageMeta, [Option<HfRecord>; 2]);

impl Default for DocumentLayout {
    fn default() -> Self {
        DocumentLayout::new(ReviewView::default())
    }
}

impl DocumentLayout {
    /// A layout of nothing yet, laying documents out in `view`.
    pub fn new(view: ReviewView) -> DocumentLayout {
        DocumentLayout {
            cache: LayoutCache::new(),
            published: Rc::new(Snapshot::new(Revision(0), LayoutRevision(0), LayoutStatus::Pending { stale_from: 0 }, Vec::new(), Vec::new())),
            layout_revision: 0,
            revision: Revision(0),
            baseline: None,
            last: None,
            acc: Accumulated { full: true, ..Default::default() },
            job: None,
            current: false,
            reused: (0, 0),
            frames: HashMap::new(),
            preview: None,
            expansions: None,
            env: None,
            chains: None,
            live: Rc::from([]),
            hyphenation: None,
            view,
        }
    }

    /// The view the document is laid out in.
    pub fn view(&self) -> &ReviewView {
        &self.view
    }

    /// Takes the view to lay the document out in; whether it differs from the one it had. The environment is read
    /// again when it does; the document is laid out again from the next edit.
    pub fn set_view(&mut self, view: ReviewView) -> bool {
        if self.view == view {
            return false;
        }
        self.view = view;
        self.env = None;
        true
    }

    /// The tables (model ids) a live drag lays out as it would leave them: each is laid out only as far as the pages
    /// laid out need, the rest of it as later pages do.
    pub fn set_live_tables(&mut self, tables: Rc<[u64]>) {
        self.live = tables;
    }

    /// The hyphenation patterns the host supplied.
    pub fn hyphenation(&self) -> Option<&super::hyphenate::Hyphenation> {
        self.hyphenation.as_ref()
    }

    /// Takes the host's hyphenation patterns; whether they differ from the ones it had. The environment is read again
    /// when they do.
    pub fn set_hyphenation(&mut self, h: Option<super::hyphenate::Hyphenation>) -> bool {
        if self.hyphenation == h {
            return false;
        }
        self.hyphenation = h;
        self.env = None;
        true
    }

    /// The document changed: its environment is read again, though nothing needs laying out again.
    pub fn document_changed(&mut self) {
        self.env = None;
        if let Some(r) = &mut self.job {
            r.job.document_changed();
        }
    }

    fn env(&mut self, doc: &Document) -> Rc<LayoutEnv> {
        if let Some(env) = &self.env {
            return env.clone();
        }
        // A document without text box chains has none still when the edits since put none in.
        let acc = &self.acc;
        let kept = self.chains.take().filter(|c| {
            c.is_empty()
                && !acc.unknown
                && !acc.full
                && (!acc.parts || doc.stories.values().all(|b| !super::env::chained(b)))
                && acc.span.is_none_or(|s| {
                    doc.sections.get(s.section).and_then(|sec| sec.blocks.get(s.first..s.new_end)).is_some_and(|b| !super::env::chained(b))
                })
        });
        let env = Rc::new(LayoutEnv::with_chains(doc, &self.view, kept, self.hyphenation.as_ref()));
        self.chains = Some(env.chains.clone());
        self.env = Some(env.clone());
        env
    }

    pub fn snapshot(&self) -> Rc<Snapshot> {
        self.published.clone()
    }

    /// Each bookmark's page number in the last finished layout (the first page its start is on), as `PAGEREF` shows it:
    /// one walk over the pages, when asked.
    pub fn bookmark_pages(&self, doc: &Document) -> HashMap<String, crate::layout::text::PageRef> {
        self.last.as_ref().map(|f| super::document::bookmark_pages_of(doc, &f.records)).unwrap_or_default()
    }

    /// The page (0 the first) each paragraph of the last finished layout starts on.
    pub fn para_pages(&self) -> HashMap<ParaId, usize> {
        let mut out = HashMap::new();
        for (i, pg) in self.last.iter().flat_map(|f| f.records.iter().enumerate()) {
            for p in &pg.effects.paras {
                out.entry(*p).or_insert(i);
            }
        }
        out
    }

    /// The footnote and endnote references of the last finished layout: each note's number as laid out and the first
    /// page showing its reference.
    pub fn note_refs_shown(&self) -> HashMap<(NoteKind, i64), (&str, usize)> {
        let mut out = HashMap::new();
        let Some(b) = &self.baseline else { return out };
        for (page, r) in b.finished.records.iter().enumerate() {
            for (kind, id, number) in &r.effects.notes_shown {
                out.entry((*kind, *id)).or_insert((number.as_str(), page));
            }
        }
        out
    }

    pub fn revision(&self) -> Revision {
        self.revision
    }

    pub fn preview(&mut self, revision: Revision, preview: Option<super::Preview>, hints: &[Option<Change>]) -> bool {
        if self.preview.as_deref() == preview.as_ref() {
            return false;
        }
        self.preview = preview.map(Rc::new);
        for &h in hints {
            self.edit(revision, h);
        }
        if hints.iter().any(Option::is_none) {
            self.acc.full = true;
        }
        true
    }

    pub fn set_expansions(&mut self, revision: Revision, expansions: Option<Rc<super::expansion::Expansions>>, hints: &[Option<Change>]) -> bool {
        if self.expansions == expansions {
            return false;
        }
        self.expansions = expansions;
        for &h in hints {
            self.edit(revision, h);
        }
        if hints.iter().any(Option::is_none) {
            self.acc.full = true;
        }
        true
    }

    /// Header and footer text changed, and nothing else: pages showing a changed part lay out again from the first.
    pub fn edit_parts(&mut self, revision: Revision) {
        self.revision = revision;
        self.job = None;
        self.current = false;
        self.env = None;
        self.acc.parts = true;
    }

    pub fn edit(&mut self, revision: Revision, hint: Option<Change>) {
        self.revision = revision;
        self.job = None;
        self.current = false;
        self.env = None;
        let Some(c) = hint else {
            self.acc.unknown = true;
            return;
        };
        if c.first > c.old_end || c.first > c.new_end {
            self.acc.unknown = true;
            return;
        }
        let single = c.first + 1 == c.old_end && c.old_end == c.new_end;
        let row = c.row.filter(|_| single);
        let s = Span { section: c.section, first: c.first, old_end: c.old_end, new_end: c.new_end, row, first_row: row.unwrap_or(0) };
        self.acc.span = match self.acc.span {
            None => Some(s),
            Some(a) => match a.merge(s) {
                Some(m) => Some(m),
                None => {
                    self.acc.full = true;
                    None
                }
            },
        };
    }

    pub fn cache(&self) -> &LayoutCache {
        &self.cache
    }

    pub fn reused_pages(&self) -> (usize, usize) {
        self.reused
    }

    fn start(&mut self, doc: &Document, fonts: &Fonts) -> Running {
        let rev = self.revision;
        let env = self.env(doc);
        let new_job = || LayoutJob::new(doc, &env.text.review, rev);
        let full = |job, why: &'static str| {
            crate::prof::time(why, || ());
            #[cfg(test)]
            FULL_JOBS.set(FULL_JOBS.get() + 1);
            Running { job, reuse: None, span: None, incremental: false, relaid: None, seeded: false, shown: (0, 0) }
        };
        let Some(b) = &self.baseline else { return full(new_job(), "full:no-baseline") };
        if self.acc.full {
            return full(new_job(), "full:asked");
        }
        let Some(parts) = b.extras.changed_parts(doc, fonts, &env) else {
            return full(new_job(), "full:extras");
        };
        let span = if self.acc.unknown {
            match compare(doc, &b.shadow) {
                Ok(s) => s,
                Err(()) => return full(new_job(), "full:compare"),
            }
        } else {
            match self.acc.span {
                Some(s)
                    if s.section < doc.sections.len()
                        && s.new_end <= doc.sections[s.section].blocks.len()
                        && s.old_end <= b.shadow[s.section].len() =>
                {
                    Some(s)
                }
                Some(_) => return full(new_job(), "full:span"),
                None => None,
            }
        };
        let fields = b.extras.body_fields
            || span.is_some_and(|s| crate::docx::fields::blocks_page_dependent(&doc.sections[s.section].blocks[s.first..s.new_end]));
        let seeded = b.finished.passes > 1 || *b.finished.known != PassInfo::default();
        if fields && !seeded {
            return full(new_job(), "full:body-fields");
        }
        let same_notes = span.is_none_or(|s| {
            let old = &b.shadow[s.section][s.first..s.old_end];
            old.iter().all(|o| o.as_ref().is_some_and(|x| !super::node::has_note_refs(x)))
                && !doc.sections[s.section].blocks[s.first..s.new_end].iter().any(super::node::has_note_refs)
        });
        let mut job = if same_notes { LayoutJob::with_notes(rev, b.notes.clone(), fields) } else { new_job() };
        if b.extras.endnote_order != job.endnotes() {
            return full(job, "full:endnotes");
        }
        if seeded {
            job.seed((*b.finished.known).clone(), b.finished.records.iter().map(|p| p.hf_full).collect());
        }
        let closed = span.map(|s| close(s, &doc.sections[s.section].blocks, &env.text.review));
        let base = &b.finished.records;
        #[cfg(test)]
        let forced = FORCE_FIRST_PAGE.get();
        #[cfg(not(test))]
        let forced = false;
        let mut relaid = None;
        let r = match &closed {
            _ if forced || base.is_empty() => 0,
            Some(s) => {
                let r = restart_page(base, s, &doc.sections[s.section].blocks, &env.text.review);
                let notes = job.note_numbers().clone();
                let latest = b.table.as_ref();
                crate::prof::time("layout:row-restart", || {
                    row_restart(
                        base,
                        s,
                        doc,
                        fonts,
                        &notes,
                        &env,
                        &mut self.cache,
                        &mut job.run.memo,
                        latest,
                        &mut relaid,
                        self.preview.as_deref(),
                        self.expansions.as_deref(),
                    )
                })
                .filter(|&p| p > r)
                .unwrap_or(r)
            }
            None => base.len() - 1,
        };
        // The first page showing a changed header or footer
        let shows = |pg: &PageRecord| pg.hf.iter().flatten().any(|h| parts.contains(&h.part));
        let r = match base.iter().position(shows) {
            Some(mut p) if p < r => {
                while p > 0 && base[p].bodies.is_empty() {
                    p -= 1;
                }
                p
            }
            _ => r,
        };
        if !parts.is_empty() {
            self.frames.retain(|_, (_, _, hf)| !hf.iter().flatten().any(|h| parts.contains(&h.part)));
        }
        let run = &mut job.run;
        run.pages = base[..r]
            .iter()
            .enumerate()
            .map(|(i, pg)| {
                let mut p = pg.clone();
                p.reused = Some(i);
                p
            })
            .collect();
        if let Some(first) = base.get(r).and_then(|pg| pg.bodies.first().map(|b| (pg, b))) {
            let (pg, body) = first;
            let mut page = pg.clone();
            page.bodies = Rc::default();
            page.body_area = None;
            page.effects = Default::default();
            page.hf = [None, None];
            page.hf_boxes = [None, None];
            page.hf_full = [None, None];
            page.composed = false;
            page.reused = None;
            run.page_number = pg.number;
            run.pages.push(page);
            run.cursor = Cursor::Body { si: body.token.section, token: body.token.clone(), top: None };
        }
        let mut keys = Vec::with_capacity(base.len());
        let mut last = (0, 0, 0, 0, 0);
        for pg in base {
            if let Some(b) = pg.bodies.first() {
                last = start_key(&shifted(&b.token, closed.as_ref()));
            }
            keys.push(last);
        }
        let renumbered = job.note_numbers().iter().filter(|(k, n)| b.note_numbers.get(*k) != Some(*n)).map(|(k, _)| *k).collect();
        let reuse = Reuse { base: b.finished.clone(), span: closed, restart: r, parts, keys, renumbered };
        Running { job, reuse: Some(reuse), span, incremental: true, relaid, seeded, shown: (0, 0) }
    }

    pub fn run(&mut self, doc: &Document, fonts: &Fonts, budget: Budget) -> Step {
        let (step, progressed) = self.step_job(doc, fonts, budget);
        self.publish(doc, fonts, step, progressed);
        step
    }

    fn step_job(&mut self, doc: &Document, fonts: &Fonts, budget: Budget) -> (Step, bool) {
        if self.current {
            return (Step::Done, false);
        }
        if self.job.is_none() {
            let r = crate::prof::time("layout:start", || self.start(doc, fonts));
            self.job = Some(r);
        }
        let env = self.env(doc);
        let ctx = LayoutCtx { fonts, env: &env, preview: self.preview.as_deref(), expansions: self.expansions.as_deref() };
        let running = self.job.as_mut().expect("started above");
        let before = running.job.committed().len();
        self.cache.store_tables = !running.incremental;
        running.job.live.clone_from(&self.live);
        let step = crate::prof::time("layout:run", || running.job.run(doc, &ctx, &mut self.cache, running.reuse.as_mut(), budget));
        self.cache.store_tables = true;
        if step == Step::Done && running.seeded && running.job.finished().is_some_and(|f| f.passes != 1) {
            crate::prof::time("full:seed-missed", || ());
            #[cfg(test)]
            FULL_JOBS.set(FULL_JOBS.get() + 1);
            self.job = Some(Running {
                job: LayoutJob::new(doc, &self.view, self.revision),
                reuse: None,
                span: None,
                incremental: false,
                relaid: None,
                seeded: false,
                shown: (0, 0),
            });
            return (Step::Yielded, false);
        }
        (step, running.job.committed().len() > before)
    }

    fn publish(&mut self, doc: &Document, fonts: &Fonts, step: Step, progressed: bool) {
        match step {
            Step::Done => self.complete_job(doc, fonts),
            Step::Yielded if progressed => {
                let env = self.env(doc);
                let ctx = LayoutCtx { fonts, env: &env, preview: self.preview.as_deref(), expansions: self.expansions.as_deref() };
                let shown = self.published.pages().len();
                if let Some(running) = self.job.as_mut() {
                    running.job.compose_committed(doc, &ctx, &mut self.cache, shown);
                }
                self.publish_pending();
            }
            Step::Yielded => {}
        }
    }

    pub fn through(&mut self, doc: &Document, fonts: &Fonts, target: Target) {
        let mut progressed = false;
        loop {
            let (step, p) = crate::prof::time("layout:step", || self.step_job(doc, fonts, Budget::Pages(1)));
            progressed |= p;
            if step == Step::Done || self.reached(doc, target) {
                crate::prof::time("layout:publish", || self.publish(doc, fonts, step, progressed));
                return;
            }
        }
    }

    fn reached(&self, doc: &Document, target: Target) -> bool {
        let Some(r) = &self.job else { return true };
        let pages = r.job.committed();
        let complete = pages.len().saturating_sub(1);
        match target {
            Target::Page(i) => complete > i,
            Target::Block { section, block, row: Some(row) } => past_row(&pages[..complete], section, block, row),
            Target::Block { section, block, row: None } => pages[..complete].iter().any(|pg| {
                pg.bodies.iter().any(|b| {
                    let len = doc.sections.get(section).map_or(0, |s| s.blocks.len());
                    let last = if b.section_end.is_some() { len } else { b.end.flow.block.min(len) };
                    b.token.section == section && b.token.flow.block.min(last) <= block && block < last
                })
            }),
            Target::Position { para, .. } if let Some((section, block, row)) = table_row_of(doc, para) => {
                past_row(&pages[..complete], section, block, row)
            }
            Target::Position { para, .. } => pages[..complete].iter().any(|pg| {
                pg.bodies.iter().any(|b| {
                    let Some(blocks) = doc.sections.get(b.token.section).map(|s| &s.blocks) else { return false };
                    let last = if b.section_end.is_some() { blocks.len() } else { b.end.flow.block };
                    let first = b.token.flow.block.min(last);
                    blocks[first..last.min(blocks.len())].iter().any(|blk| holds(blk, para))
                })
            }),
        }
    }

    pub fn complete(&mut self, doc: &Document, fonts: &Fonts) {
        loop {
            let (step, p) = self.step_job(doc, fonts, Budget::Unlimited);
            if step == Step::Done {
                self.publish(doc, fonts, step, p);
                return;
            }
        }
    }

    fn fragment(&mut self, pg: &PageRecord, section_page: usize) -> (Rc<PageFragment>, PageMeta) {
        let meta = PageMeta { section: pg.section, number: pg.number, section_page, blank: pg.blank };
        let Some(key) = pg.bodies.first().and_then(|b| b.body.id) else { return finish_page(pg, section_page) };
        if let Some((frag, m, hf)) = self.frames.get(&key) {
            let bodies = frag.children.iter().filter_map(|c| if let super::fragment::PageChild::Body(b) = c { Some(b) } else { None });
            let same_bodies = bodies.clone().count() == pg.bodies.len() && bodies.zip(pg.bodies.iter()).all(|(a, b)| Rc::ptr_eq(a, &b.body));
            if same_bodies && *m == meta && *hf == pg.hf {
                return (frag.clone(), meta);
            }
        }
        let (frag, meta) = finish_page(pg, section_page);
        self.frames.insert(key, (frag.clone(), meta.clone(), pg.hf.clone()));
        (frag, meta)
    }

    fn fragments(&mut self, records: &[&PageRecord]) -> Vec<(Rc<PageFragment>, PageMeta)> {
        let mut seen = HashMap::<usize, usize>::new();
        let out: Vec<_> = records
            .iter()
            .map(|pg| {
                let section_page = if pg.blank { 0 } else { *seen.entry(pg.section).and_modify(|n| *n += 1).or_insert(0) };
                self.fragment(pg, section_page)
            })
            .collect();
        if self.frames.len() > 4 * records.len() + 64 {
            let keep: std::collections::HashSet<FragmentId> = records.iter().filter_map(|p| p.bodies.first().and_then(|b| b.body.id)).collect();
            self.frames.retain(|k, _| keep.contains(k));
        }
        out
    }

    fn next_layout_revision(&mut self) -> LayoutRevision {
        self.layout_revision += 1;
        LayoutRevision(self.layout_revision)
    }

    fn publish_pending(&mut self) {
        let Some(mut r) = self.job.take() else { return };
        let prev = self.published.clone();
        let records = r.job.committed();
        // The pages laid out: blank or with a body. Only the last page can be neither, just started.
        let mut n = records.len();
        while n > 0 && !records[n - 1].blank && records[n - 1].bodies.is_empty() {
            n -= 1;
        }
        // What this pass published is as it was, but for its last page, which can have taken more bodies since.
        let pass = r.job.run.k;
        let from = if r.shown.0 == pass { r.shown.1.saturating_sub(1).min(n).min(prev.pages().len()) } else { 0 };
        #[cfg(test)]
        {
            let committed = records.iter().filter(|p| p.blank || !p.bodies.is_empty()).count();
            assert_eq!(committed, n, "only the last page is without a body and not blank");
            for (i, pg) in records[..from].iter().enumerate() {
                let section_page = if pg.blank { 0 } else { pg.in_section - 1 };
                let (f, m) = self.fragment(pg, section_page);
                // A blank page is made again each time, the same.
                let was = &prev.info()[i];
                let same = if pg.blank { was.blank && (was.section, was.number) == (m.section, m.number) } else { Rc::ptr_eq(&f, &prev.pages()[i]) };
                assert!(same, "page {i} published before is as it was");
            }
        }
        let mut fresh = Vec::with_capacity(n - from);
        for pg in &records[from..n] {
            let section_page = if pg.blank { 0 } else { pg.in_section - 1 };
            fresh.push(self.fragment(pg, section_page));
        }
        let excluded = super::document::excluded(&records[from..n]).into_iter().map(|(i, p, u)| (i + from, p, u)).collect();
        if self.frames.len() > 4 * n + 64 {
            let keep: std::collections::HashSet<FragmentId> = records[..n].iter().filter_map(|p| p.bodies.first().and_then(|b| b.body.id)).collect();
            self.frames.retain(|k, _| keep.contains(k));
        }
        r.shown = (pass, n);
        self.job = Some(r);
        let lrev = self.next_layout_revision();
        self.published = Rc::new(Snapshot::pending_after(&prev, self.revision, lrev, from, fresh, excluded));
    }

    fn complete_job(&mut self, doc: &Document, fonts: &Fonts) {
        let Some(r) = self.job.take() else { return };
        let incremental = r.incremental;
        let span = r.span;
        let relaid = r.relaid.clone();
        let env = self.env(doc);
        let endnotes = r.job.endnotes().to_vec();
        let note_numbers = r.job.note_numbers().clone();
        let notes = r.job.job_notes().clone();
        let parts = r.reuse.map(|x| x.parts).unwrap_or_default();
        let Some(finished) = r.job.into_finished() else { return };
        let records: Vec<&PageRecord> = finished.records.iter().collect();
        let pages = self.fragments(&records);
        let finished = Rc::new(Finished { pages, ..finished });
        self.last = Some(finished.clone());
        self.reused = (finished.records.iter().filter(|p| p.reused.is_some()).count(), finished.records.len());
        self.cache.fit(finished.records.len());
        let lrev = self.next_layout_revision();
        self.published = Rc::new(finished.snapshot(self.revision, lrev));
        self.current = true;
        if finished.status != LayoutStatus::Converged {
            self.baseline = None;
            self.acc = Accumulated { full: true, ..Default::default() };
            return;
        }
        let (extras, shadow, table) = match (incremental, self.baseline.take()) {
            (true, Some(mut b)) => {
                if let Some(s) = span {
                    let blocks = &doc.sections[s.section].blocks[s.first..s.new_end];
                    // ponytail: stays set when the edit removed the last page-dependent field; a full job clears it.
                    b.extras.body_fields |= crate::docx::fields::blocks_page_dependent(blocks);
                    b.shadow[s.section].splice(s.first..s.old_end, blocks.iter().map(shadow_of));
                }
                // Changed parts only: the names and every other part match the document.
                for name in parts {
                    if let (Some(now), Some(was)) = (doc.stories.get(&name), b.extras.stories.get_mut(&name)) {
                        was.clone_from(now);
                    }
                }
                let kept = b.table.and_then(|mut t| match span {
                    Some(s) if s.section == t.0 && t.1 >= s.old_end => {
                        t.1 = (t.1 as isize + s.delta()) as usize;
                        Some(t)
                    }
                    Some(s) if s.section == t.0 && t.1 >= s.first => None,
                    _ => Some(t),
                });
                (b.extras, b.shadow, relaid.or(kept))
            }
            _ => (Extras::new(doc, fonts, &endnotes, &env), shadow(doc), None),
        };
        self.baseline = Some(Baseline { finished, extras, shadow, note_numbers, notes, table });
        self.acc = Accumulated::default();
    }
}
