//! The document pass: the only layout code that reads the whole document. It builds the nodes, numbers notes
//! (NOTE-4), appends the endnotes (NOTE-10), pages the sections in order and composes headers and footers with final
//! values (HDR-10). Passes repeat until the layout converges, with a stabilizing pass after 4 (FIELD-6).
//!
//! A pass is a state machine ([`Cursor`]) advanced one committed body or one composed page at a time, so a
//! [`LayoutJob`] can stop between any two steps and resume. A full layout is a job run without a budget.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

#[cfg(test)]
use std::cell::Cell;

use crate::docx::model::*;
use crate::font::Fonts;

use super::LayoutCtx;
use super::cache::LayoutCache;
use super::env::LayoutEnv;
use super::fragment::{BoxFragment, BoxKind, PageChild, PageFragment};
use super::ids::{FragmentId, LayoutRevision, ParaId, Revision, Unit};
use super::incremental::Reuse;
use super::job::{Budget, Memos, Meter, Step, Yield};
use super::node::{Blocks, Notes, SectionNode};
use super::page::{Cursor, Next, Paginator, PassInfo, PassRun, margins, story_ref_in};
use super::record::{HfRecord, PageRecord};
use super::snapshot::{LayoutStatus, PageMeta, Snapshot};
use super::space::HfHeights;
use super::story::{StoryEnv, hf_limit, stack_limited};
use super::text::FieldValues;
use super::token::{FlowToken, PageToken};

/// Lays out a document with the given fonts: a full layout without a cache, the reference every other layout is
/// compared with.
pub fn layout(doc: &Document, fonts: &Fonts) -> Snapshot {
    let mut cache = LayoutCache::new();
    layout_with(doc, fonts, &mut cache)
}

/// [`layout`] reusing (and filling) `cache`.
pub fn layout_with(doc: &Document, fonts: &Fonts, cache: &mut LayoutCache) -> Snapshot {
    layout_records(doc, fonts, cache).0
}

/// [`layout_with`], and the records of the committed pages (tests read them).
pub(crate) fn layout_records(doc: &Document, fonts: &Fonts, cache: &mut LayoutCache) -> (Snapshot, Vec<PageRecord>) {
    let env = LayoutEnv::new(doc);
    let ctx = LayoutCtx { fonts, env: &env };
    let mut job = LayoutJob::new(doc, Revision(0));
    // Full layouts never yield (no budget).
    job.run(doc, &ctx, cache, None, Budget::Unlimited);
    let done = job.finished().expect("an unlimited run completes the job");
    (done.snapshot(Revision(0), LayoutRevision(0)), done.records.clone())
}

#[cfg(test)]
thread_local! {
    /// Test hooks for the convergence tests. Thread-local so tests running in parallel don't interfere. A height
    /// added to every composed header on passes 1 and 3, so the layout never converges.
    pub(crate) static HF_ALTERNATE: Cell<f64> = const { Cell::new(0.0) };
    /// Makes the stabilizing pass's verification fail on every page with a header.
    pub(crate) static FAIL_VERIFY: Cell<bool> = const { Cell::new(false) };
}

/// The height the alternating test hook adds to a header composed in pass `k`.
fn hook_height(k: usize) -> f64 {
    #[cfg(test)]
    if matches!(k, 1 | 3) {
        return HF_ALTERNATE.get();
    }
    let _ = k;
    0.0
}

const EPS: f64 = 1e-6;

pub(crate) fn same_heights(a: &HfHeights, b: &HfHeights) -> bool {
    a.iter().zip(b).all(|(x, y)| match (x, y) {
        (None, None) => true,
        (Some(x), Some(y)) => (x - y).abs() < EPS,
        _ => false,
    })
}

/// The header and footer heights page `pg`'s first body was laid out with.
pub(crate) fn used(pg: &PageRecord) -> HfHeights {
    pg.bodies.first().map_or([None, None], |b| b.space.hf)
}

/// What a job derives from the document once: note numbers in reference order (NOTE-4), and the endnotes appended to
/// the last section with their separator, which gains keepNext (NOTE-10).
#[derive(Clone)]
pub struct JobNotes {
    pub numbers: HashMap<(NoteKind, i64), String>,
    endnotes: Vec<i64>,
    separator: Vec<Block>,
}

impl JobNotes {
    pub fn new(doc: &Document) -> JobNotes {
        let mut refs = Vec::new();
        let mut seen = HashSet::new();
        for s in &doc.sections {
            collect_refs(&s.blocks, &mut refs, &mut seen);
        }
        let mut numbers = HashMap::new();
        let (mut nf, mut ne) = (0, 0);
        for (k, id) in &refs {
            let n = match k {
                NoteKind::Foot => {
                    nf += 1;
                    nf.to_string()
                }
                NoteKind::End => {
                    ne += 1;
                    crate::docx::numbering::roman(ne).to_lowercase()
                }
            };
            numbers.insert((*k, *id), n);
        }
        let endnotes: Vec<i64> = refs.iter().filter(|(k, _)| *k == NoteKind::End).map(|(_, id)| *id).collect();
        let mut separator = Vec::new();
        if !endnotes.is_empty() {
            separator = doc.separators.get("endnote:separator").cloned().unwrap_or_default();
            for b in &mut separator {
                if let Block::Para(p) = b {
                    let mut st = (*p.style).clone();
                    st.keep_next = true;
                    p.style = Rc::new(st);
                }
            }
        }
        JobNotes { numbers, endnotes, separator }
    }
}

/// Whether a block holds a note reference (what [`JobNotes`] is derived from).
pub fn has_note_refs(b: &Block) -> bool {
    match b {
        Block::Para(p) => p.inlines.iter().any(|i| matches!(i, Inline::NoteRef { .. })),
        Block::Table(t) => t.rows.iter().flat_map(|r| r.cells.iter()).flat_map(|c| c.blocks.iter()).any(has_note_refs),
    }
}

/// Note references in document order (body, then tables).
fn collect_refs(blocks: &[Block], out: &mut Vec<(NoteKind, i64)>, seen: &mut HashSet<(NoteKind, i64)>) {
    for b in blocks {
        match b {
            Block::Para(p) => {
                for i in &p.inlines {
                    if let Inline::NoteRef { kind, id, custom, .. } = i
                        && !custom
                        && seen.insert((*kind, *id))
                    {
                        out.push((*kind, *id));
                    }
                }
            }
            Block::Table(t) => {
                for r in &t.rows {
                    for c in &r.cells {
                        collect_refs(&c.blocks, out, seen);
                    }
                }
            }
        }
    }
}

/// What a pass step reads besides its state, rebuilt for each `run` call (the document may only change between jobs).
pub struct PassCx<'d> {
    pub doc: &'d Document,
    pub ctx: LayoutCtx<'d>,
    pub nodes: Vec<SectionNode<'d>>,
    pub notes: &'d JobNotes,
    /// The STYLEREF keys the headers and footers show (usually none), and the paragraphs they look up.
    wanted: Vec<String>,
    paras: HashMap<ParaId, &'d Paragraph>,
}

impl<'d> PassCx<'d> {
    pub fn new(doc: &'d Document, ctx: LayoutCtx<'d>, notes: &'d JobNotes) -> PassCx<'d> {
        let nsec = doc.sections.len();
        let nodes: Vec<SectionNode> = doc
            .sections
            .iter()
            .enumerate()
            .map(|(si, s)| {
                let mut blocks = Blocks::of(&s.blocks);
                if si + 1 == nsec && !notes.endnotes.is_empty() {
                    blocks.append(notes.separator.iter());
                    for id in &notes.endnotes {
                        blocks.append(doc.endnotes.get(id).into_iter().flatten());
                    }
                }
                SectionNode { index: si, section: s, blocks }
            })
            .collect();
        let mut wanted: Vec<String> = Vec::new();
        for blocks in doc.stories.values() {
            for b in blocks {
                if let Block::Para(p) = b {
                    for x in &p.inlines {
                        if let Inline::FieldBegin { kind: Some(FieldKind::StyleRef(st)), .. } = x {
                            wanted.push(crate::docx::fields::style_key(st));
                        }
                    }
                }
            }
        }
        let mut paras = HashMap::new();
        if !wanted.is_empty() {
            for n in &nodes {
                for &b in &n.blocks.blocks {
                    if let Block::Para(p) = b {
                        paras.insert(ParaId(p.id), p);
                    }
                }
            }
        }
        PassCx { doc, ctx, nodes, notes, wanted, paras }
    }

    fn notes(&self) -> Notes<'d> {
        Notes { footnotes: &self.doc.footnotes, endnotes: &self.doc.endnotes, separators: &self.doc.separators }
    }

    pub fn paginator<'p>(&'p self, run: &'p mut PassRun) -> Paginator<'d, 'p> {
        Paginator {
            ctx: self.ctx,
            sections: &self.doc.sections,
            stories: &self.doc.stories,
            notes: self.notes(),
            note_numbers: &self.notes.numbers,
            run,
        }
    }
}

/// NOTE-10 with columns: in a last section of several columns with endnotes, the body
/// balances as before a continuous section break, and the endnotes start below its tallest column, in its first
/// column. The block the endnotes start at, when that applies.
pub(crate) fn endnote_split(cx: &PassCx, si: usize) -> Option<usize> {
    let doc = cx.doc;
    (si + 1 == doc.sections.len() && doc.sections[si].section_in(doc.review.view).columns.num > 1 && !cx.notes.endnotes.is_empty())
        .then(|| doc.sections[si].blocks.len())
}

/// Whether section `si`'s body from `token` balances its last page (COL-5): before a continuous section, and the last
/// section's body before its endnotes ([`endnote_split`]).
pub(crate) fn balances(cx: &PassCx, si: usize, token: &PageToken) -> bool {
    let doc = cx.doc;
    let nsec = doc.sections.len();
    (doc.sections[si].section_in(doc.review.view).columns.num > 1 && si + 1 < nsec && doc.sections[si + 1].break_kind == SectionBreak::Continuous)
        || endnote_split(cx, si).is_some_and(|e| token.flow.block < e)
}

/// Advances `run` by one step: a section start, one committed body (or one page reused from the baseline), or one
/// composed page. A `Yield` leaves the cursor where it was: the step runs again on resume, its completed units
/// cached.
pub fn step(run: &mut PassRun, cx: &PassCx, reuse: Option<&mut Reuse>, memos: &mut Memos) -> Result<(), Yield> {
    let doc = cx.doc;
    let nsec = doc.sections.len();
    match run.cursor.clone() {
        Cursor::Section { si, mut carry, y } => {
            if si >= nsec {
                run.cursor = Cursor::Compose(0);
                return Ok(());
            }
            let sec = &doc.sections[si];
            let mut p = cx.paginator(run);
            let continuous = si > 0 && sec.break_kind == SectionBreak::Continuous;
            let mut top = y;
            if !continuous || p.run.pages.is_empty() {
                if si > 0 {
                    // Odd/even page breaks insert a blank page when needed (HDR-12); it counts in the numbering.
                    let next = p.run.page_number + 1;
                    let want_odd = match sec.break_kind {
                        SectionBreak::OddPage => Some(true),
                        SectionBreak::EvenPage => Some(false),
                        _ => None,
                    };
                    if let Some(odd) = want_odd
                        && (next % 2 == 1) != odd
                        && sec.pg_start.is_none()
                    {
                        p.new_page(si, true);
                    }
                }
                if let Some(n) = sec.pg_start {
                    p.run.page_number = n - 1;
                }
                p.new_page(si, false);
                // PARA-2, PAGE-7: a next-page section keeps its first space-before, less the previous paragraph's
                // space-after (`prev_after`, kept for it).
                carry.keep_top = true;
                if sec.line_numbers.as_ref().is_some_and(|l| l.restart != "continuous") {
                    carry.line_no = 0; // LNUM-2: newPage / newSection restart
                }
                carry.prev = None;
                top = None;
            } else if sec.line_numbers.as_ref().is_some_and(|l| l.restart == "newSection") {
                carry.line_no = 0;
            }
            run.cursor = Cursor::Body { si, token: PageToken { section: si, flow: FlowToken::default(), state: carry }, top };
        }
        Cursor::Body { si, token, top } => {
            if top.is_none()
                && let Some(r) = reuse
                && let Some(next) = r.reuse_page(run, cx, si, &token, memos)?
            {
                run.cursor = next;
                return memos.meter.page_done();
            }
            let balance = balances(cx, si, &token);
            // Before the endnotes of a section in columns: the body alone, ending as a section does.
            let split = endnote_split(cx, si).filter(|&e| token.flow.block < e);
            let body_only;
            let node = match split {
                Some(e) => {
                    let n = &cx.nodes[si];
                    body_only = SectionNode {
                        index: n.index,
                        section: n.section,
                        blocks: Blocks { blocks: n.blocks.blocks[..e].to_vec(), stories: Vec::new() },
                    };
                    &body_only
                }
                None => &cx.nodes[si],
            };
            let mut p = cx.paginator(run);
            let next = p.body(node, token, top, balance, memos)?;
            run.cursor = match next {
                Next::Page(token) => Cursor::Body { si, token, top: None },
                // The endnotes continue on the page, below the balanced body (as a continuous section starts).
                Next::SectionEnd(carry, y) if let Some(e) = split => {
                    let flow = FlowToken { block: e, ..FlowToken::default() };
                    Cursor::Body { si, token: PageToken { section: si, flow, state: carry }, top: Some(y) }
                }
                Next::SectionEnd(carry, y) => Cursor::Section { si: si + 1, carry, y: Some(y) },
            };
            // A page boundary: the body is committed, so a used-up budget can stop here.
            memos.meter.page_done()?;
        }
        Cursor::Compose(i) => {
            if run.info.is_none() {
                run.info = Some(PassInfo {
                    num_pages: Some(run.pages.len().to_string()),
                    section_pages: (0..nsec).map(|si| run.pages.iter().filter(|pg| pg.section == si && !pg.blank).count().to_string()).collect(),
                    page_refs: Rc::new(bookmark_pages(&run.pages)),
                });
            }
            if i >= run.pages.len() {
                run.cursor = Cursor::Done;
                return Ok(());
            }
            let reused = match reuse {
                Some(r) => r.reuse_composed(run, cx, i),
                None => false,
            };
            if !reused {
                super::job::unit("composition", || compose_page(run, cx, i, memos))?;
            }
            run.cursor = Cursor::Compose(i + 1);
            // Composed: a deadline can stop here (composition completes no layout unit of its own).
            if let Budget::Until(_) = memos.meter.budget() {
                memos.meter.page_done()?;
            }
        }
        Cursor::Done => {}
    }
    Ok(())
}

/// The header and footer values page `i` shows (HDR-10): page numbers, the page count, STYLEREF from the pass's pages
/// and PAGEREF from its bookmarks. Set once composition starts.
pub(crate) fn hf_fields(run: &PassRun, cx: &PassCx, i: usize) -> FieldValues {
    let pg = &run.pages[i];
    let si = pg.section;
    let info = run.info.as_ref().expect("composition started");
    FieldValues {
        page: Some(pg.number.to_string()),
        num_pages: info.num_pages.clone(),
        section: Some((si + 1).to_string()),
        section_pages: info.section_pages.get(si).cloned(),
        page_refs: Some(info.page_refs.clone()),
        style_refs: style_refs(&cx.wanted, &run.pages, &cx.paras, i),
        page_fmt: cx.doc.sections[si].pg_fmt.clone(),
    }
}

/// The story parts page `i` shows (HDR-7, HDR-12): (header, footer).
pub(crate) fn hf_parts<'d>(run: &PassRun, cx: &PassCx<'d>, i: usize) -> [Option<&'d String>; 2] {
    let pg = &run.pages[i];
    if pg.blank {
        return [None, None];
    }
    let pis = run.pages[..i].iter().filter(|p| p.section == pg.section && !p.blank).count();
    let part = |footer| {
        story_ref_in(&cx.doc.sections, cx.ctx.env.text.review.view, cx.ctx.env.even_and_odd, pg.section, pis, pg.number, footer)
            .1
            .filter(|p| cx.doc.stories.contains_key(*p))
    };
    [part(false), part(true)]
}

/// Composes page `i`'s header and footer with final values. In the stabilizing pass each is cut to whole lines within
/// the height its page's body was laid out with (FIELD-6).
fn compose_page(run: &mut PassRun, cx: &PassCx, i: usize, memos: &mut Memos) -> Result<(), Yield> {
    let pg = &run.pages[i];
    if pg.blank {
        return Ok(());
    }
    let (si, num) = (pg.section, pg.number);
    let sec = &cx.doc.sections[si];
    let ctx = cx.ctx;
    let (left, right, _) = margins(sec, &ctx, num);
    let s = sec.section_in(ctx.env.text.review.view);
    let width = s.page_w - left - right;
    let fields = hf_fields(run, cx, i);
    let parts = hf_parts(run, cx, i);
    let env = StoryEnv { ctx, fields: &fields, notes: &cx.notes.numbers };
    let limit = hf_limit(ctx.env.text.compat, s.page_h);
    let reserved = used(pg);
    let (clip, k) = (run.clip, run.k);
    let mut full: HfHeights = [None, None];
    let mut records: [Option<HfRecord>; 2] = [None, None];
    let mut boxes: [Option<BoxFragment>; 2] = [None, None];
    for (slot, footer) in [false, true].into_iter().enumerate() {
        let Some(part) = parts[slot] else { continue };
        let blocks: Vec<&Block> = cx.doc.stories[part].iter().collect();
        let l = super::story::layout(&blocks, width, &env, false, memos)?;
        let cap = if clip { reserved[slot].map_or(limit, |r| limit.min(r)) } else { limit };
        let story_fields = super::page::fields_used(&fields, &blocks);
        if footer {
            let (_, height) = stack_limited(&l, left, 0.0, cap);
            full[1] = Some(stack_limited(&l, left, 0.0, limit).1);
            let (children, _) = stack_limited(&l, left, s.page_h - s.footer - height, cap);
            records[1] = Some(HfRecord { part: part.clone(), fields: story_fields, height });
            boxes[1] = Some(BoxFragment {
                kind: BoxKind::Footer { story: part.clone() },
                id: None,
                offset: (0.0, 0.0),
                size: (width, height),
                children: children.into(),
            });
        } else {
            full[0] = Some(stack_limited(&l, left, s.header, limit).1 + hook_height(k));
            let (children, height) = if limit.is_infinite() && !clip && s.header + l.height > 0.85 * s.page_h {
                // HDR-8: a very tall compat-15 header is drawn only down to 0.435 × page height.
                let cut = 0.435 * s.page_h;
                let mut c = Vec::new();
                for u in l.units.iter().take_while(|u| s.header + u.top < cut) {
                    c.push(super::fragment::Child::Group { offset: (left, s.header + u.top), children: u.content.clone() });
                }
                (c, l.height)
            } else {
                stack_limited(&l, left, s.header, cap)
            };
            records[0] = Some(HfRecord { part: part.clone(), fields: story_fields, height: if clip { height } else { full[0].unwrap_or(height) } });
            boxes[0] = Some(BoxFragment {
                kind: BoxKind::Header { story: part.clone() },
                id: None,
                offset: (0.0, 0.0),
                size: (width, if clip { height } else { l.height }),
                children: children.into(),
            });
        }
    }
    let pg = &mut run.pages[i];
    pg.hf = records;
    pg.hf_boxes = boxes;
    pg.hf_full = full;
    Ok(())
}

/// A finished job: the committed pages, their fragments and the layout status.
pub struct Finished {
    pub records: Vec<PageRecord>,
    pub pages: Vec<(Rc<PageFragment>, PageMeta)>,
    pub status: LayoutStatus,
    /// The number of passes that ran.
    pub passes: usize,
}

impl Finished {
    pub fn snapshot(&self, revision: Revision, layout_revision: LayoutRevision) -> Snapshot {
        Snapshot::new(revision, layout_revision, self.status.clone(), self.pages.clone(), excluded(&self.records))
    }
}

/// A layout of one document revision, run in steps under a budget: passes until convergence, then the
/// stabilizing pass if needed. Owns its state, so it survives between `run` calls; the document must not change
/// while it runs.
pub struct LayoutJob {
    pub revision: Revision,
    notes: JobNotes,
    /// The document has page-dependent body fields (FIELD-1), which take the previous pass's values.
    body_fields: bool,
    pub(crate) run: PassRun,
    /// Composed heights of every finished pass.
    history: Vec<Vec<HfHeights>>,
    finished: Option<Finished>,
}

impl LayoutJob {
    pub fn new(doc: &Document, revision: Revision) -> LayoutJob {
        LayoutJob::with_notes(revision, JobNotes::new(doc), crate::docx::fields::page_dependent(doc))
    }

    /// A job with what [`LayoutJob::new`] derives from the whole document already known (an incremental job whose
    /// edit changed neither: the scans grow with the document).
    pub fn with_notes(revision: Revision, notes: JobNotes, body_fields: bool) -> LayoutJob {
        LayoutJob {
            revision,
            notes,
            body_fields,
            run: PassRun::new(1, Rc::new(PassInfo::default()), Rc::new(Vec::new()), false),
            history: Vec::new(),
            finished: None,
        }
    }

    pub fn finished(&self) -> Option<&Finished> {
        self.finished.as_ref()
    }

    pub fn into_finished(self) -> Option<Finished> {
        self.finished
    }

    /// What the job derived from the document's note references.
    pub fn job_notes(&self) -> &JobNotes {
        &self.notes
    }

    /// Note numbers of the job's document.
    pub fn note_numbers(&self) -> &HashMap<(NoteKind, i64), String> {
        &self.notes.numbers
    }

    /// The endnotes in reference order (NOTE-10), as appended to the last section.
    pub fn endnotes(&self) -> &[i64] {
        &self.notes.endnotes
    }

    /// The pages committed so far in the current pass.
    pub fn committed(&self) -> &[PageRecord] {
        match &self.finished {
            Some(f) => &f.records,
            None => &self.run.pages,
        }
    }

    /// Runs until the job finishes or `budget` is used up (at least one atomic unit completes). `reuse` offers baseline
    /// pages to the first pass.
    pub fn run(&mut self, doc: &Document, ctx: &LayoutCtx, cache: &mut LayoutCache, mut reuse: Option<&mut Reuse>, budget: Budget) -> Step {
        if self.finished.is_some() {
            return Step::Done;
        }
        cache.begin();
        let mut meter = Meter::new(budget);
        let notes = std::mem::replace(&mut self.notes, JobNotes { numbers: HashMap::new(), endnotes: Vec::new(), separator: Vec::new() });
        let step_result = {
            let cx = PassCx::new(doc, *ctx, &notes);
            let mut memos = Memos { cache, job: None, meter: &mut meter };
            loop {
                if let Cursor::Done = self.run.cursor {
                    if self.end_pass(doc) {
                        break Step::Done;
                    }
                    continue;
                }
                let r = if self.run.k == 1 { reuse.as_deref_mut() } else { None };
                if step(&mut self.run, &cx, r, &mut memos).is_err() {
                    break Step::Yielded;
                }
            }
        };
        self.notes = notes;
        step_result
    }

    /// Composes the headers and footers of committed pages that have none yet, with provisional values (the page count
    /// is at least `pages`): what a pending snapshot shows. The pass's own composition recomposes every page with final
    /// values.
    pub fn compose_committed(&mut self, doc: &Document, ctx: &LayoutCtx, cache: &mut LayoutCache, pages: usize) {
        if self.finished.is_some() || self.run.info.is_some() {
            return;
        }
        let notes = std::mem::replace(&mut self.notes, JobNotes { numbers: HashMap::new(), endnotes: Vec::new(), separator: Vec::new() });
        {
            let cx = PassCx::new(doc, *ctx, &notes);
            let mut meter = Meter::new(Budget::Unlimited);
            let mut memos = Memos { cache, job: None, meter: &mut meter };
            let run = &mut self.run;
            let n = run.pages.len();
            let nsec = doc.sections.len();
            run.info = Some(PassInfo {
                num_pages: Some(n.max(pages).to_string()),
                section_pages: (0..nsec).map(|si| run.pages.iter().filter(|pg| pg.section == si && !pg.blank).count().to_string()).collect(),
                page_refs: Rc::new(bookmark_pages(&run.pages)),
            });
            for i in 0..n {
                let pg = &run.pages[i];
                if (pg.bodies.is_empty() && !pg.blank) || pg.hf_boxes.iter().any(Option::is_some) {
                    continue;
                }
                // Without a budget a composition can't yield.
                let _ = compose_page(run, &cx, i, &mut memos);
            }
            run.info = None;
        }
        self.notes = notes;
    }

    /// Ends the current pass: finishes the job when the layout converged (or the stabilizing pass ran), else starts the
    /// next pass. Returns whether the job finished.
    fn end_pass(&mut self, doc: &Document) -> bool {
        let run = &self.run;
        let info = run.info.clone().unwrap_or_default();
        let composed: Vec<HfHeights> = run.pages.iter().map(|p| p.hf_full).collect();
        if run.k <= 4 {
            let fields_agree = !self.body_fields || info == *run.known;
            let hf_agree = run.pages.iter().all(|pg| same_heights(&used(pg), &pg.hf_full));
            if fields_agree && hf_agree {
                self.finish(LayoutStatus::Converged);
                return true;
            }
            self.history.push(composed.clone());
            if run.k < 4 {
                self.run = PassRun::new(run.k + 1, Rc::new(info), Rc::new(composed), false);
                return false;
            }
            // The stabilizing pass (FIELD-6). Pages no earlier pass reached measure their stories with this pass's
            // values.
            let n = self.history.iter().map(Vec::len).max().unwrap_or(0);
            let reserve: Vec<HfHeights> = (0..n)
                .map(|i| {
                    let mut r: HfHeights = [None, None];
                    for c in self.history.iter().filter_map(|c| c.get(i)) {
                        for k in 0..2 {
                            r[k] = match (r[k], c[k]) {
                                (Some(a), Some(b)) => Some(a.max(b)),
                                (a, b) => a.or(b),
                            };
                        }
                    }
                    r
                })
                .collect();
            self.run = PassRun::new(5, Rc::new(info), Rc::new(reserve), true);
            return false;
        }
        let frozen_hf: Box<[usize]> = run.pages.iter().enumerate().filter(|(_, pg)| !same_heights(&used(pg), &pg.hf_full)).map(|(i, _)| i).collect();
        let unresolved_fields: Box<[usize]> = if self.body_fields && info != *run.known {
            run.pages.iter().enumerate().filter(|(_, pg)| shows_pass_fields(doc, pg)).map(|(i, _)| i).collect()
        } else {
            Box::new([])
        };
        let bad = verify(doc, &run.pages);
        let status =
            if bad.is_empty() { LayoutStatus::Stabilized { frozen_hf, unresolved_fields } } else { LayoutStatus::Inconsistent { pages: bad.into() } };
        self.finish(status);
        true
    }

    fn finish(&mut self, status: LayoutStatus) {
        let run = std::mem::replace(&mut self.run, PassRun::new(0, Rc::new(PassInfo::default()), Rc::new(Vec::new()), false));
        let passes = run.k;
        let records = run.pages;
        let pages = records
            .iter()
            .scan(HashMap::<usize, usize>::new(), |seen, pg| {
                let section_page = if pg.blank { 0 } else { *seen.entry(pg.section).and_modify(|n| *n += 1).or_insert(0) };
                Some(finish_page(pg, section_page))
            })
            .collect();
        self.finished = Some(Finished { records, pages, status, passes });
    }
}

/// Whether a page shows a body field that takes the previous pass's value (NUMPAGES, SECTIONPAGES, PAGEREF).
fn shows_pass_fields(doc: &Document, pg: &PageRecord) -> bool {
    let _ = doc;
    pg.bodies.iter().any(|b| b.fields.num_pages.is_some() || b.fields.section_pages.is_some() || b.fields.page_refs.is_some())
}

/// The pages whose composed header or footer overlaps the body where the rules make the body depend on them (HDR-4 to
/// HDR-6; HDR-5's negative margins are exempt by rule).
fn verify(doc: &Document, records: &[PageRecord]) -> Vec<usize> {
    let mut bad = Vec::new();
    for (i, pg) in records.iter().enumerate() {
        let (Some((top, bottom)), Some(s)) = (pg.body_area, doc.sections.get(pg.section).map(|s| s.section_in(doc.review.view))) else { continue };
        let header = pg.hf[0].as_ref().is_some_and(|h| s.top >= 0.0 && s.header + h.height > top + EPS);
        let footer = pg.hf[1].as_ref().is_some_and(|f| s.bottom >= 0.0 && s.page_h - s.footer - f.height < bottom - EPS);
        #[cfg(test)]
        let header = header || (FAIL_VERIFY.get() && pg.hf[0].is_some());
        if header || footer {
            bad.push(i);
        }
    }
    bad
}

/// Text pushed entirely below its page (HDR-9), by page: what the traversals leave out. Each body's list is computed
/// once and moves with the body.
pub(crate) fn excluded(records: &[PageRecord]) -> Vec<(usize, ParaId, std::ops::Range<Unit>)> {
    let mut out = Vec::new();
    for (i, pg) in records.iter().enumerate() {
        for (para, units) in pg.bodies.iter().flat_map(|b| b.excluded.iter()) {
            out.push((i, *para, units.clone()));
        }
    }
    out
}

/// PAGEREF: each bookmark's page number, from the page where its paragraph starts (the last one placed wins).
fn bookmark_pages(pages: &[PageRecord]) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for pg in pages {
        for b in &pg.effects.bookmarks {
            out.insert(b.clone(), pg.number.to_string());
        }
    }
    out
}

/// A page's fragment from its record: the bodies, then the header and footer.
pub(crate) fn finish_page(pg: &PageRecord, section_page: usize) -> (Rc<PageFragment>, PageMeta) {
    let mut children: Vec<PageChild> = pg.bodies.iter().map(|b| PageChild::Body(b.body.clone())).collect();
    if let Some(h) = &pg.hf_boxes[0] {
        children.push(PageChild::Header(h.clone()));
    }
    if let Some(f) = &pg.hf_boxes[1] {
        children.push(PageChild::Footer(f.clone()));
    }
    let page = PageFragment { id: FragmentId::next(), w: pg.w, h: pg.h, body_area: pg.body_area, children: children.into() };
    (Rc::new(page), PageMeta { section: pg.section, number: pg.number, section_page, blank: pg.blank })
}

/// STYLEREF in a header or footer on page `i` (FIELD-2), for every key in `wanted`.
fn style_refs(wanted: &[String], pages: &[PageRecord], paras: &HashMap<ParaId, &Paragraph>, i: usize) -> Vec<(String, String)> {
    if wanted.is_empty() {
        return Vec::new();
    }
    let style = |p: &Paragraph| p.style.style_id.as_deref().map(crate::docx::fields::style_key).unwrap_or_default();
    let on = |k: usize| pages.get(k).map(|p| &p.effects.paras[..]).unwrap_or(&[]);
    wanted
        .iter()
        .filter_map(|key| {
            let hit = |list: &[ParaId], rev: bool| {
                let mut it: Box<dyn Iterator<Item = &ParaId>> = if rev { Box::new(list.iter().rev()) } else { Box::new(list.iter()) };
                it.find_map(|r| paras.get(r).copied().filter(|p| style(p) == *key)).map(crate::docx::fields::text)
            };
            hit(on(i), false)
                .or_else(|| (0..i).rev().find_map(|k| hit(on(k), true)))
                .or_else(|| (i + 1..pages.len()).find_map(|k| hit(on(k), false)))
                .map(|t| (key.clone(), t))
        })
        .collect()
}
