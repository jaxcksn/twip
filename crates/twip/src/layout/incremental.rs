//! Incremental layout: the baseline, accumulated changes, the restart frontier and page reuse, behind
//! [`DocumentLayout`].
//!
//! **Equivalence by construction.** An incremental job runs the same passes as a fresh layout. Its first pass starts at
//! the restart page with the baseline's earlier pages copied, and takes a baseline page instead of laying one out only
//! when every input of that page is equal; later passes, when needed, run in full. Copied pages are a fresh
//! first pass's pages only if the baseline converged in its first pass, so a baseline that needed more passes gives a
//! full layout, as do changes outside the blocks (settings, section properties, stories, notes, fonts) and body fields
//! that take the previous pass's values. Every result therefore equals `layout(doc, fonts)`.

use std::collections::HashMap;
use std::rc::Rc;

use crate::docx::model::*;
use crate::docx::review::MarkupView;
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
use super::page::{Cursor, PassRun, fields_used};
use super::record::{HfRecord, PageRecord};
use super::snapshot::{LayoutStatus, PageMeta, Snapshot};
use super::space::BodyFields;
use super::text::same_para;
use super::token::{At, PageToken};

/// An edit's extent, from the editor: in section `section`, blocks `first..old_end` before the
/// edit became `first..new_end`. `row`: the edited row when the edit stayed inside one table row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Change {
    pub section: usize,
    pub first: usize,
    pub old_end: usize,
    pub new_end: usize,
    pub row: Option<usize>,
}

/// What `through` lays out to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Page(usize),
    Position { para: ParaId, unit: Unit },
}

/// The changed blocks since the baseline: baseline blocks `first..old_end` of `section` are current blocks
/// `first..new_end`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Span {
    section: usize,
    first: usize,
    old_end: usize,
    new_end: usize,
    /// When every edit stayed inside one table row of the single block `first`: the last edited row (the row
    /// exception).
    row: Option<usize>,
    /// With `row`: the first edited row (the rows between may be unchanged).
    first_row: usize,
}

impl Span {
    /// Current minus baseline block count after the span.
    fn delta(&self) -> isize {
        self.new_end as isize - self.old_end as isize
    }

    /// The span covering this one and `c` (an edit in current coordinates); `None` in another section. Blocks between
    /// the two count as changed (conservative).
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

/// Every edit since the baseline's revision.
#[derive(Clone, Debug, Default)]
struct Accumulated {
    span: Option<Span>,
    /// An edit came without a hint: find the change by comparing the document with the shadow.
    unknown: bool,
    /// A full layout is needed.
    full: bool,
}

/// The blocks as the baseline laid them out; `None`: stale (copying it would cost a pause, so it compares as changed).
type Shadow = Vec<Vec<Option<Block>>>;

/// What the baseline was laid out from besides the blocks: any change to it means a full layout.
struct Extras {
    sections: Vec<Section>,
    stories: std::collections::BTreeMap<String, Vec<Block>>,
    footnotes: HashMap<i64, Vec<Block>>,
    endnotes: HashMap<i64, Vec<Block>>,
    separators: HashMap<String, Vec<Block>>,
    env: LayoutEnv,
    fonts: u64,
    /// Endnote order (NOTE-10): the blocks appended to the last section.
    endnote_order: Vec<i64>,
    /// The document has page-dependent body fields.
    body_fields: bool,
}

/// A section's properties without its blocks. Exhaustive, so a field added to the model is decided on here.
fn props(s: &Section) -> Section {
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
        line_numbers,
        v_align,
        page_borders,
        blocks: _,
        change,
    } = s;
    Section {
        break_kind: *break_kind,
        page_w: *page_w,
        page_h: *page_h,
        top: *top,
        bottom: *bottom,
        left: *left,
        right: *right,
        header: *header,
        footer: *footer,
        gutter: *gutter,
        columns: columns.clone(),
        title_pg: *title_pg,
        headers: headers.clone(),
        footers: footers.clone(),
        pg_start: *pg_start,
        pg_fmt: pg_fmt.clone(),
        line_numbers: line_numbers.clone(),
        v_align: v_align.clone(),
        page_borders: page_borders.clone(),
        blocks: Vec::new(),
        change: change.clone(),
    }
}

impl Extras {
    fn new(doc: &Document, fonts: &Fonts, endnote_order: &[i64]) -> Extras {
        Extras {
            sections: doc.sections.iter().map(props).collect(),
            stories: doc.stories.clone(),
            footnotes: doc.footnotes.clone(),
            endnotes: doc.endnotes.clone(),
            separators: doc.separators.clone(),
            env: LayoutEnv::new(doc),
            fonts: fonts.revision(),
            endnote_order: endnote_order.to_vec(),
            body_fields: crate::docx::fields::page_dependent(doc),
        }
    }

    /// Whether the document's non-block inputs are the baseline's.
    fn same(&self, doc: &Document, fonts: &Fonts) -> bool {
        self.fonts == fonts.revision()
            && self.sections.len() == doc.sections.len()
            && self.sections.iter().zip(&doc.sections).all(|(a, b)| *a == props(b))
            && self.env == LayoutEnv::new(doc)
            && self.stories == doc.stories
            && self.footnotes == doc.footnotes
            && self.endnotes == doc.endnotes
            && self.separators == doc.separators
    }
}

/// A top-level table's layout for the document of the job that made it: section, block, width (bits), layout.
type LatestTable = (usize, usize, u64, Rc<super::table::TableLayout>);

/// The most recent layout that completed `Converged`.
struct Baseline {
    finished: Rc<Finished>,
    extras: Extras,
    shadow: Shadow,
    note_numbers: HashMap<(NoteKind, i64), String>,
    /// Its job's note references, for the next job when its edit has none.
    notes: super::document::JobNotes,
    /// The table an edit inside its rows laid out, as it is in this baseline's document: the next edit in its
    /// rows lays out only those rows.
    table: Option<LatestTable>,
}

/// Block equality for change detection: paragraphs as [`same_para`] (source positions excluded), tables by their
/// properties and their cells' blocks.
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

/// Tables with more cells than this are left stale in the shadow: copying one per keystroke would cost a pause.
const SHADOW_CELLS: usize = 1000;

/// The shadow copy of a block.
fn shadow_of(b: &Block) -> Option<Block> {
    match b {
        Block::Table(t) if t.rows.iter().map(|r| r.cells.len()).sum::<usize>() > SHADOW_CELLS => None,
        _ => Some(b.clone()),
    }
}

fn shadow(doc: &Document) -> Shadow {
    doc.sections.iter().map(|s| s.blocks.iter().map(|b| Some(b.clone())).collect()).collect()
}

/// The change between the shadow and the document: `Ok(None)` if nothing changed, `Err(())` if more than one section
/// changed or the sections differ in number.
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

/// The span extended to every block whose measurement depends on a changed block: the paragraphs joined to it
/// (REV-6: a paragraph before it whose mark the view hides lays out with it), and a bordered paragraph next to those
/// (PARA-7 neighbour keys). Border boxes depend only on immediate neighbours; a joined paragraph's border is its last
/// paragraph's.
fn close(span: Span, blocks: &[Block], view: MarkupView) -> Span {
    let bordered = |i: usize| matches!(blocks.get(i), Some(Block::Para(p)) if border_key(p.para_style_in(view)).is_some());
    let mut s = span;
    let first = chain_start(blocks, s.first, view);
    if first < s.first {
        s.first = first;
        s.row = None;
    }
    if s.first > 0 && bordered(s.first - 1) {
        s.first = chain_start(blocks, s.first - 1, view);
        s.row = None;
    }
    let next = chain_end(blocks, s.new_end, view);
    if s.new_end < blocks.len() && bordered(next) {
        s.old_end += next + 1 - s.new_end;
        s.new_end = next + 1;
        s.row = None;
    }
    s
}

/// The restart page. Let *g* be the first block of the closed span. While the block before *g* is a
/// `keepNext` paragraph in the same section, move *g* back to it. The restart page is the latest baseline page whose
/// start is strictly before (*section*, *g*): the page where the block before *g* ends, covering pull-back, keep chains
/// and page-level decisions (by starting at a page boundary). Pages without a body (blank pages)
/// fall back to an earlier page; with none, the first page.
///
/// Backward dependencies this rule covers: pull-back; keep-with-next chains before the span; floats, note streams,
/// vertical alignment, balancing and separators (page-level). Header and footer content from elsewhere is recomposed,
/// and a changed header height fails convergence; body page fields mean a full layout. A new rule that lets later
/// content change earlier placement must be added here.
fn restart_page(base: &[PageRecord], span: &Span, blocks: &[Block], view: MarkupView) -> usize {
    // A joined paragraph (REV-6) keeps with the next by its last paragraph's style, and starts at its first.
    let mut g = chain_start(blocks, span.first.min(blocks.len()), view);
    while g > 0 && matches!(&blocks[g - 1], Block::Para(p) if p.para_style_in(view).keep_next) {
        g = chain_start(blocks, g - 1, view);
    }
    let before = |pg: &PageRecord| pg.bodies.first().is_some_and(|b| (b.token.section, b.token.flow.block) < (span.section, g));
    let mut r = base.iter().rposition(before).unwrap_or(0);
    while r > 0 && base[r].bodies.is_empty() {
        r -= 1;
    }
    r
}

/// The rows rule: for an edit inside row `row` of one table, the latest baseline page starting strictly
/// before (table, `Row(row)`), when the new table's column geometry and position, and rows `..row`'s heights and top
/// borders, equal the baseline's (that page's recorded table layout). `None` when the rule doesn't apply. Header rows
/// repeat on every page the table continues on, so an edit in one gives no row restart.
#[allow(clippy::too_many_arguments)]
fn row_restart(
    base: &[PageRecord],
    span: &Span,
    doc: &Document,
    fonts: &Fonts,
    notes: &HashMap<(NoteKind, i64), String>,
    cache: &mut LayoutCache,
    memo: &mut super::flow::PassMemo,
    latest: Option<&LatestTable>,
    relaid: &mut Option<LatestTable>,
) -> Option<usize> {
    let row = span.row?;
    let blocks = &doc.sections.get(span.section)?.blocks;
    let Some(Block::Table(table)) = blocks.get(span.first) else { return None };
    // The paragraph before the table (PARA-4), as the column fill passes it.
    let prev = match span.first.checked_sub(1).and_then(|i| blocks.get(i)) {
        Some(Block::Para(p)) => Some(&**p.para_style_in(doc.review.view)),
        _ => None,
    };
    fn inside<'p>(pg: &'p PageRecord, span: &Span) -> Option<&'p super::record::BodyRecord> {
        pg.bodies.first().filter(|b| (b.token.section, b.token.flow.block) == (span.section, span.first) && b.start_table.is_some())
    }
    let key = (span.section, span.first, 2u8, row);
    // The page to restart at, if any; the table is laid out (and the pass given it) whenever a page records it,
    // which means its text doesn't vary by page: an edit on its first page has no restart page, and the pass would
    // otherwise lay the whole table out.
    let p = base.iter().rposition(|pg| inside(pg, span).is_some_and(|b| start_key(&b.token) < key));
    let rec = p.or_else(|| base.iter().position(|pg| inside(pg, span).is_some()))?;
    let width = base[rec].bodies[0].space.columns[0].width;
    let env = LayoutEnv::new(doc);
    let ctx = LayoutCtx { fonts, env: &env };
    let fields = super::text::FieldValues::default();
    let story = super::story::StoryEnv { ctx, fields: &fields, notes };
    // Laid out through the document's cache, without storing the new version (an incremental pass's);
    // the job's pass takes it from its memo.
    cache.store_tables = false;
    let mut meter = super::job::Meter::unlimited();
    let mut memos = Memos { cache, job: None, meter: &mut meter };
    // With the table as the baseline's document has it (laid out by the job that made the baseline), only the edited
    // rows are measured and laid out again. The pages' records can't stand in: a record taken over from an
    // older baseline holds the table as it was then.
    let new = match latest.filter(|l| (l.0, l.1, l.2) == (span.section, span.first, width.to_bits())) {
        Some(l) => super::table::relayout_rows(table, width, &story, prev, &l.3, span.first_row.min(row)..=row, &mut memos),
        None => super::table::layout(table, width, &story, prev, false, &mut memos),
    }
    .ok();
    memos.cache.store_tables = true;
    let new = new?;
    *relaid = Some((span.section, span.first, width.to_bits(), new.clone()));
    // The job's pass lays the table out at this width too: it takes this layout instead of building its key again
    // (exact: a table with per-page text has no recorded layout, so it never gets here).
    memo.seed(table, width, new.clone());
    let p = p?;
    let old = base[p].bodies[0].start_table.as_ref()?;
    if row < old.rows.iter().take_while(|r| r.header).count() {
        return None;
    }
    let same = old.cols == new.cols
        && old.x0 == new.x0
        && old.rows.len() == new.rows.len()
        && old.rows[..row].iter().zip(&new.rows[..row]).all(|(a, b)| a.height == b.height && a.top_border == b.top_border);
    same.then_some(p)
}

/// A page's start key for lookup: document order of its first body's token.
fn start_key(t: &PageToken) -> (usize, usize, u8, usize) {
    let (kind, i) = match &t.flow.at {
        At::Start => (0, 0),
        At::Piece(i) => (1, *i),
        At::Row(r) => (2, *r),
        At::RowPart { row, .. } => (3, *row),
    };
    (t.section, t.flow.block, kind, i)
}

/// A token of the baseline in current block coordinates.
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

/// Whether the values of the fields `used` shows are the same in `now`.
fn same_used(used: &BodyFields, now: &BodyFields) -> bool {
    (used.page.is_none() || (used.page == now.page && used.page_fmt == now.page_fmt))
        && (used.section.is_none() || used.section == now.section)
        && (used.num_pages.is_none() || used.num_pages == now.num_pages)
        && (used.section_pages.is_none() || used.section_pages == now.section_pages)
        && used.page_refs.as_ref().is_none_or(|m| m.iter().all(|(k, v)| now.page_refs.as_ref().and_then(|n| n.get(k)) == Some(v)))
}

#[cfg(test)]
thread_local! {
    /// Test hooks. Thread-local so tests running in parallel don't interfere. Forces every job to
    /// restart from the first page.
    pub(crate) static FORCE_FIRST_PAGE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// Jobs that laid out from the first page with nothing reused (full layouts).
    pub(crate) static FULL_JOBS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    /// Pages reused from a baseline.
    pub(crate) static REUSED_PAGES: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Baseline pages offered to a job's first pass.
pub struct Reuse {
    base: Rc<Finished>,
    /// The closed span; `None`: no block changed.
    span: Option<Span>,
    /// Candidates come after this baseline page.
    restart: usize,
    /// Header and footer stories are unchanged (a story edit means a full layout, so always true for a running job).
    stories_same: bool,
    /// Each baseline page's start key in current coordinates; a page without a body takes the previous page's key, so
    /// the list is sorted.
    keys: Vec<(usize, usize, u8, usize)>,
    /// Notes whose number changed since the baseline (NOTE-4): a page showing a reference to one is laid out again.
    renumbered: std::collections::HashSet<(NoteKind, i64)>,
}

/// The section, top-level block and row of a paragraph inside a top-level table.
fn table_row_of(doc: &Document, para: ParaId) -> Option<(usize, usize, usize)> {
    doc.sections.iter().enumerate().find_map(|(si, s)| {
        s.blocks.iter().enumerate().find_map(|(bi, b)| match b {
            Block::Table(t) => t.rows.iter().position(|r| r.cells.iter().any(|c| c.blocks.iter().any(|b| holds(b, para)))).map(|r| (si, bi, r)),
            Block::Para(_) => None,
        })
    })
}

/// Whether `b` is paragraph `para` or holds it (in a table cell, nested tables included).
fn holds(b: &Block, para: ParaId) -> bool {
    match b {
        Block::Para(p) => p.id == para.0,
        Block::Table(t) => t.rows.iter().any(|r| r.cells.iter().any(|c| c.blocks.iter().any(|b| holds(b, para)))),
    }
}

/// Whether `blocks` (nested tables included) reference one of `notes`.
fn references(blocks: &[Block], notes: &std::collections::HashSet<(NoteKind, i64)>) -> bool {
    blocks.iter().any(|b| match b {
        Block::Para(p) => p.inlines.iter().any(|i| matches!(i, Inline::NoteRef { kind, id, .. } if notes.contains(&(*kind, *id)))),
        Block::Table(t) => t.rows.iter().any(|r| r.cells.iter().any(|c| references(&c.blocks, notes))),
    })
}

impl Reuse {
    /// Reuses a baseline page for the page starting at `token`, if one passes the reuse checks; returns where
    /// the pass continues.
    pub fn reuse_page(&mut self, run: &mut PassRun, cx: &PassCx, si: usize, token: &PageToken, memos: &mut Memos) -> Result<Option<Cursor>, Yield> {
        // Content: the page starts after the closed span, or at a row after the edited ones of the one
        // edited table (the row exception, checked against the table's layout below).
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
            // Content, on the baseline's side: a page starting inside the span showed changed blocks.
            if let Some(s) = &self.span
                && body.token.section == s.section
                && body.token.flow.block < s.old_end
                && in_table.is_none()
            {
                continue;
            }
            let t = shifted(&body.token, self.span.as_ref());
            // Continuation: the same token, state included.
            if t != *token {
                continue;
            }
            // A body that ends its section with the next one continuous shares its page: not reusable alone.
            if body.section_end.is_some() && doc.sections.get(si + 1).is_some_and(|n| n.break_kind == SectionBreak::Continuous) {
                continue;
            }
            // Constraints: the same body space on this page.
            let p = cx.paginator(run);
            let space = p.body_space(si, None, balance, memos)?;
            if space != body.space {
                continue;
            }
            // Effects and derived values: the fields it shows, and its notes' numbers.
            if !same_used(&body.fields, &p.fields(si)) {
                continue;
            }
            if !pg.effects.notes_shown.iter().all(|(k, id, n)| cx.notes.numbers.get(&(*k, *id)) == Some(n)) {
                continue;
            }
            // The row exception: the edited table lays out the same from the page's first row on.
            if let Some(r) = in_table {
                let (Some(old), Some(Block::Table(table))) = (&body.start_table, doc.sections[si].blocks.get(token.flow.block)) else { continue };
                // The pass laid the table out on an earlier page (the edited table starts before this one).
                let Some(new) = run.memo.table(table, space.columns[0].width) else { continue };
                let same = old.cols == new.cols
                    && old.x0 == new.x0
                    && old.bottom_border == new.bottom_border
                    && old.rows.len() == new.rows.len()
                    && old.rows.get(r..) == new.rows.get(r..)
                    // A vertically merged region crossing row r is drawn from its first cell, above r (TABLE-32).
                    && new.rows.get(r).is_some_and(|row| row.cells.iter().all(|c| c.v_merge != VMerge::Continue))
                    // Header rows repeat at the top of every page the table continues on (TABLE-46): an edited one
                    // changes those pages, and they must lay out the same.
                    && self.span.and_then(|s| s.row).is_some_and(|e| e >= new.rows.iter().take_while(|row| row.header).count())
                    && old.rows.iter().take_while(|row| row.header).eq(new.rows.iter().take_while(|row| row.header));
                if !same {
                    continue;
                }
            }
            // A reference to a renumbered note shows its new number.
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
            page.bodies.push(body);
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

    /// The row a page starting at `token` starts at, when the row exception may make it reusable: `At::Row(r)` into
    /// the edited table is valid when the edited row is before `r` and the column geometry, position, bottom border,
    /// and rows `r..`'s heights and top borders are equal (the caller compares the layouts). `At::RowPart` into an
    /// edited table is never valid.
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

    /// Reuses page `i`'s header and footer from its baseline page when they show the same stories with the same values
    /// at the same place.
    pub fn reuse_composed(&mut self, run: &mut PassRun, cx: &PassCx, i: usize) -> bool {
        let Some(j) = run.pages[i].reused else { return false };
        if !self.stories_same || run.clip {
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
                (Some(part), Some(h)) => {
                    let blocks: Vec<&Block> = cx.doc.stories[part].iter().collect();
                    *part == h.part && fields_used(&fields, &blocks) == h.fields
                }
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
        true
    }
}

/// A running job and how it started.
struct Running {
    job: LayoutJob,
    reuse: Option<Reuse>,
    /// The span the job laid out, in current coordinates (`None` for a full layout or no change).
    span: Option<Span>,
    /// Whether the job reuses the baseline (its non-block inputs are the baseline's).
    incremental: bool,
    /// The table its edit's rows were laid out again in ([`Baseline::table`]).
    relaid: Option<LatestTable>,
}

/// The layout state of one document: the published snapshot, the baseline, the changes since
/// it, the cache and at most one job.
pub struct DocumentLayout {
    cache: LayoutCache,
    published: Rc<Snapshot>,
    layout_revision: u64,
    revision: Revision,
    baseline: Option<Baseline>,
    acc: Accumulated,
    job: Option<Running>,
    /// The published snapshot reflects every edit (nothing to do until the next one).
    current: bool,
    /// Pages the last completed job took from the baseline (copied or reused), and its page count.
    reused: (usize, usize),
    /// Published page fragments by their first body's id: a page published again with the same bodies, header and
    /// footer and numbering keeps its fragment (and so its converted items).
    frames: HashMap<FragmentId, Frame>,
}

/// A published page fragment with the numbering and header and footer it was published with.
type Frame = (Rc<PageFragment>, PageMeta, [Option<HfRecord>; 2]);

impl Default for DocumentLayout {
    fn default() -> Self {
        DocumentLayout::new()
    }
}

impl DocumentLayout {
    pub fn new() -> DocumentLayout {
        DocumentLayout {
            cache: LayoutCache::new(),
            published: Rc::new(Snapshot::new(Revision(0), LayoutRevision(0), LayoutStatus::Pending { stale_from: 0 }, Vec::new(), Vec::new())),
            layout_revision: 0,
            revision: Revision(0),
            baseline: None,
            acc: Accumulated { full: true, ..Default::default() },
            job: None,
            current: false,
            reused: (0, 0),
            frames: HashMap::new(),
        }
    }

    /// The latest published snapshot.
    pub fn snapshot(&self) -> Rc<Snapshot> {
        self.published.clone()
    }

    /// The revision of the last edit: a snapshot of an older one predates it.
    pub fn revision(&self) -> Revision {
        self.revision
    }

    /// The document changed to `revision` (already applied): merges the change into the accumulated span and abandons
    /// any running job (its committed pages stay in the published snapshot, provisional).
    pub fn edit(&mut self, revision: Revision, hint: Option<Change>) {
        self.revision = revision;
        self.job = None;
        self.current = false;
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

    /// The cache, for inspection.
    pub fn cache(&self) -> &LayoutCache {
        &self.cache
    }

    /// Pages the last completed job took from the baseline (before the restart page or reused after the change), and
    /// its page count.
    pub fn reused_pages(&self) -> (usize, usize) {
        self.reused
    }

    fn start(&mut self, doc: &Document, fonts: &Fonts) -> Running {
        let rev = self.revision;
        let new_job = || LayoutJob::new(doc, rev);
        let full = |job| {
            #[cfg(test)]
            FULL_JOBS.set(FULL_JOBS.get() + 1);
            Running { job, reuse: None, span: None, incremental: false, relaid: None }
        };
        let Some(b) = &self.baseline else { return full(new_job()) };
        if self.acc.full || b.finished.passes != 1 || !b.extras.same(doc, fonts) {
            return full(new_job());
        }
        let span = if self.acc.unknown {
            match compare(doc, &b.shadow) {
                Ok(s) => s,
                Err(()) => return full(new_job()),
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
                Some(_) => return full(new_job()),
                None => None,
            }
        };
        // Body fields that take the previous pass's value mean a full layout.
        if b.extras.body_fields
            || span.is_some_and(|s| crate::docx::fields::blocks_page_dependent(&doc.sections[s.section].blocks[s.first..s.new_end]))
        {
            return full(new_job());
        }
        // The baseline's note numbers and no page-dependent field, unless an edited block held or holds a note
        // reference (a block the shadow doesn't keep counts as one): the whole-document scans are skipped.
        let same_notes = span.is_none_or(|s| {
            let old = &b.shadow[s.section][s.first..s.old_end];
            old.iter().all(|o| o.as_ref().is_some_and(|x| !super::document::has_note_refs(x)))
                && !doc.sections[s.section].blocks[s.first..s.new_end].iter().any(super::document::has_note_refs)
        });
        let mut job = if same_notes { LayoutJob::with_notes(rev, b.notes.clone(), false) } else { new_job() };
        if b.extras.endnote_order != job.endnotes() {
            return full(job);
        }
        let closed = span.map(|s| close(s, &doc.sections[s.section].blocks, doc.review.view));
        let base = &b.finished.records;
        #[cfg(test)]
        let forced = FORCE_FIRST_PAGE.get();
        #[cfg(not(test))]
        let forced = false;
        let mut relaid = None;
        let r = match &closed {
            _ if forced || base.is_empty() => 0,
            Some(s) => {
                let r = restart_page(base, s, &doc.sections[s.section].blocks, doc.review.view);
                let notes = job.note_numbers().clone();
                let latest = b.table.as_ref();
                crate::prof::time("layout:row-restart", || {
                    row_restart(base, s, doc, fonts, &notes, &mut self.cache, &mut job.run.memo, latest, &mut relaid)
                })
                .filter(|&p| p > r)
                .unwrap_or(r)
            }
            None => base.len() - 1,
        };
        // The job's first pass resumes at page r: earlier pages are the baseline's.
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
            page.bodies.clear();
            page.body_area = None;
            page.effects = Default::default();
            page.hf = [None, None];
            page.hf_boxes = [None, None];
            page.hf_full = [None, None];
            page.reused = None;
            run.page_number = pg.number;
            run.pages.push(page);
            run.cursor = Cursor::Body { si: body.token.section, token: body.token.clone(), top: None };
        }
        let mut keys = Vec::with_capacity(base.len());
        let mut last = (0, 0, 0, 0);
        for pg in base {
            if let Some(b) = pg.bodies.first() {
                last = start_key(&shifted(&b.token, closed.as_ref()));
            }
            keys.push(last);
        }
        let renumbered = job.note_numbers().iter().filter(|(k, n)| b.note_numbers.get(*k) != Some(*n)).map(|(k, _)| *k).collect();
        let reuse = Reuse { base: b.finished.clone(), span: closed, restart: r, stories_same: true, keys, renumbered };
        Running { job, reuse: Some(reuse), span, incremental: true, relaid }
    }

    /// Runs the job (starting one for the latest revision if needed) until it finishes or `budget` is used up, then
    /// publishes per the publish rule.
    pub fn run(&mut self, doc: &Document, fonts: &Fonts, budget: Budget) -> Step {
        let (step, progressed) = self.step_job(doc, fonts, budget);
        self.publish(doc, fonts, step, progressed);
        step
    }

    /// Runs the job (starting one for the latest revision if needed) without publishing: the step, and whether it
    /// committed a page.
    fn step_job(&mut self, doc: &Document, fonts: &Fonts, budget: Budget) -> (Step, bool) {
        if self.current {
            return (Step::Done, false);
        }
        if self.job.is_none() {
            let r = crate::prof::time("layout:start", || self.start(doc, fonts));
            self.job = Some(r);
        }
        let env = LayoutEnv::new(doc);
        let ctx = LayoutCtx { fonts, env: &env };
        let running = self.job.as_mut().expect("started above");
        let before = running.job.committed().len();
        self.cache.store_tables = !running.incremental;
        let step = crate::prof::time("layout:run", || running.job.run(doc, &ctx, &mut self.cache, running.reuse.as_mut(), budget));
        self.cache.store_tables = true;
        (step, running.job.committed().len() > before)
    }

    /// The publish rule: a finished job is published (a converged one becomes the baseline);
    /// a running one is published, pending, if this request committed a page.
    fn publish(&mut self, doc: &Document, fonts: &Fonts, step: Step, progressed: bool) {
        match step {
            Step::Done => self.complete_job(doc, fonts),
            Step::Yielded if progressed => {
                let env = LayoutEnv::new(doc);
                let ctx = LayoutCtx { fonts, env: &env };
                let shown = self.published.pages().len();
                if let Some(running) = self.job.as_mut() {
                    running.job.compose_committed(doc, &ctx, &mut self.cache, shown);
                }
                self.publish_pending();
            }
            Step::Yielded => {}
        }
    }

    /// Runs until the page holding `target` is laid out (or the job is done), then publishes once.
    pub fn through(&mut self, doc: &Document, fonts: &Fonts, target: Target) {
        let mut progressed = false;
        loop {
            // One page at a time: runs stop only at page boundaries (no page attempt restarts), and the target is checked
            // after each page.
            let (step, p) = crate::prof::time("layout:step", || self.step_job(doc, fonts, Budget::Pages(1)));
            progressed |= p;
            if step == Step::Done || self.reached(doc, target) {
                crate::prof::time("layout:publish", || self.publish(doc, fonts, step, progressed));
                return;
            }
        }
    }

    /// Whether the running job has laid out the page holding `target`. A position is laid out once the top-level
    /// block holding its paragraph is (a paragraph in a table cell is found through its table).
    fn reached(&self, doc: &Document, target: Target) -> bool {
        let Some(r) = &self.job else { return true };
        let pages = r.job.committed();
        // The last page may still be filling.
        let complete = pages.len().saturating_sub(1);
        match target {
            Target::Page(i) => complete > i,
            // In a top-level table: laid out once a page's body ends past the target's row (the rest of a long table
            // waits for background layout).
            Target::Position { para, .. } if let Some((section, block, row)) = table_row_of(doc, para) => pages[..complete].iter().any(|pg| {
                pg.bodies.iter().any(|b| {
                    let (s, blk, kind, i) = start_key(&b.end);
                    b.section_end.is_some_and(|_| b.token.section == section)
                        || (s, blk) > (section, block)
                        || (s, blk) == (section, block) && kind >= 2 && i > row
                })
            }),
            Target::Position { para, .. } => pages[..complete].iter().any(|pg| {
                pg.bodies.iter().any(|b| {
                    let Some(blocks) = doc.sections.get(b.token.section).map(|s| &s.blocks) else { return false };
                    // The blocks the body finished: from its start to before its end's block (all of them when the
                    // section ended on this page).
                    let last = if b.section_end.is_some() { blocks.len() } else { b.end.flow.block };
                    let first = b.token.flow.block.min(last);
                    blocks[first..last.min(blocks.len())].iter().any(|blk| holds(blk, para))
                })
            }),
        }
    }

    /// Runs the job to completion and publishes it.
    pub fn complete(&mut self, doc: &Document, fonts: &Fonts) {
        loop {
            let (step, p) = self.step_job(doc, fonts, Budget::Unlimited);
            if step == Step::Done {
                self.publish(doc, fonts, step, p);
                return;
            }
        }
    }

    /// The fragment of committed page `pg`: the one published before for the same page, or a new one.
    fn fragment(&mut self, pg: &PageRecord, section_page: usize) -> (Rc<PageFragment>, PageMeta) {
        let meta = PageMeta { section: pg.section, number: pg.number, section_page, blank: pg.blank };
        let Some(key) = pg.bodies.first().and_then(|b| b.body.id) else { return finish_page(pg, section_page) };
        if let Some((frag, m, hf)) = self.frames.get(&key) {
            let bodies = frag.children.iter().filter_map(|c| if let super::fragment::PageChild::Body(b) = c { Some(b) } else { None });
            let same_bodies = bodies.clone().count() == pg.bodies.len() && bodies.zip(&pg.bodies).all(|(a, b)| Rc::ptr_eq(a, &b.body));
            if same_bodies && *m == meta && *hf == pg.hf {
                return (frag.clone(), meta);
            }
        }
        let (frag, meta) = finish_page(pg, section_page);
        self.frames.insert(key, (frag.clone(), meta.clone(), pg.hf.clone()));
        (frag, meta)
    }

    /// The fragments of committed pages, sharing the ones published before.
    fn fragments(&mut self, records: &[&PageRecord]) -> Vec<(Rc<PageFragment>, PageMeta)> {
        let mut seen = HashMap::<usize, usize>::new();
        let out: Vec<_> = records
            .iter()
            .map(|pg| {
                let section_page = if pg.blank { 0 } else { *seen.entry(pg.section).and_modify(|n| *n += 1).or_insert(0) };
                self.fragment(pg, section_page)
            })
            .collect();
        // shortcut: the memo is pruned to the pages just published when it grows past a few layouts' worth.
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

    /// Publishes a running job's committed pages, followed by the previous snapshot's later pages (provisional).
    fn publish_pending(&mut self) {
        let Some(r) = self.job.take() else { return };
        let prev = self.published.clone();
        let committed: Vec<&PageRecord> = r.job.committed().iter().filter(|p| p.blank || !p.bodies.is_empty()).collect();
        let n = committed.len();
        let mut pages = self.fragments(&committed);
        for (page, info) in prev.pages().iter().zip(prev.info()).skip(n) {
            pages.push((page.clone(), PageMeta { section: info.section, number: info.number, section_page: info.section_page, blank: info.blank }));
        }
        let excluded = super::document::excluded(r.job.committed()).into_iter().filter(|e| e.0 < n).collect();
        self.job = Some(r);
        let lrev = self.next_layout_revision();
        self.published = Rc::new(Snapshot::new(self.revision, lrev, LayoutStatus::Pending { stale_from: n }, pages, excluded));
    }

    /// Publishes a finished job; a converged layout becomes the baseline.
    fn complete_job(&mut self, doc: &Document, fonts: &Fonts) {
        let Some(r) = self.job.take() else { return };
        let incremental = r.incremental;
        let span = r.span;
        let relaid = r.relaid.clone();
        let endnotes = r.job.endnotes().to_vec();
        let note_numbers = r.job.note_numbers().clone();
        let notes = r.job.job_notes().clone();
        let Some(finished) = r.job.into_finished() else { return };
        // Pages that came back unchanged share the fragment published before.
        let records: Vec<&PageRecord> = finished.records.iter().collect();
        let pages = self.fragments(&records);
        let finished = Finished { pages, ..finished };
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
        // The new baseline: its shadow follows the document (the span's blocks copied, or everything after a full
        // layout), and its non-block inputs are the old baseline's when the job reused it.
        let (extras, shadow, table) = match (incremental, self.baseline.take()) {
            (true, Some(mut b)) => {
                if let Some(s) = span {
                    let new: Vec<Option<Block>> = doc.sections[s.section].blocks[s.first..s.new_end].iter().map(shadow_of).collect();
                    b.shadow[s.section].splice(s.first..s.old_end, new);
                }
                // The table this job laid out again, else the baseline's while the edit didn't reach it.
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
            _ => (Extras::new(doc, fonts, &endnotes), shadow(doc), None),
        };
        self.baseline = Some(Baseline { finished: Rc::new(finished), extras, shadow, note_numbers, notes, table });
        self.acc = Accumulated::default();
    }
}
