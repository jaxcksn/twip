//! Document layout passes

use std::collections::HashMap;
use std::rc::Rc;

#[cfg(test)]
use std::cell::Cell;
use std::cell::RefCell;

use crate::docx::model::*;
use crate::font::Fonts;

use super::LayoutCtx;
use super::cache::LayoutCache;
use super::env::LayoutEnv;
use super::fragment::PageFragment;
use super::ids::{LayoutRevision, ParaId, Revision, Unit};
use super::incremental::Reuse;
use super::job::{Budget, Memos, Meter, Step, Yield};
use super::node::{Blocks, Notes, SectionNode};
use super::page::{Cursor, HfSources, Next, Paginator, PassInfo, PassRun};
use super::record::PageRecord;
use super::snapshot::{LayoutStatus, PageMeta, Snapshot};
use super::space::HfHeights;
use super::text::PageRef;
use super::token::{FlowToken, PageToken};
use super::units::EPS;

mod compose;
mod notes;

pub use compose::Found;
pub(crate) use compose::{finish_page, hf_fields, hf_parts};
pub use notes::JobNotes;

use compose::{Chapters, chapters, compose_page};

pub fn layout(doc: &Document, fonts: &Fonts) -> Snapshot {
    let mut cache = LayoutCache::new();
    layout_with(doc, fonts, &mut cache)
}

/// [`layout`] with the host's hyphenation patterns.
pub fn layout_hyphenated(doc: &Document, fonts: &Fonts, hyphenation: Option<&super::hyphenate::Hyphenation>) -> Snapshot {
    layout_showing(doc, fonts, &mut LayoutCache::new(), None, hyphenation).0
}

/// Stepped layout, stoppable between steps.
pub fn layout_until(doc: &Document, fonts: &Fonts, mut stop: impl FnMut() -> bool) -> Option<Snapshot> {
    let env = LayoutEnv::new(doc, None);
    let ctx = LayoutCtx { fonts, env: &env, preview: None, expansions: None };
    let mut cache = LayoutCache::new();
    let mut job = LayoutJob::new(doc, Revision(0));
    while job.run(doc, &ctx, &mut cache, None, Budget::Pages(4)) != Step::Done {
        if stop() {
            return None;
        }
    }
    Some(job.finished().expect("a run that reports done completes the job").snapshot(Revision(0), LayoutRevision(0)))
}

pub fn layout_with(doc: &Document, fonts: &Fonts, cache: &mut LayoutCache) -> Snapshot {
    layout_records(doc, fonts, cache).0
}

pub(crate) fn layout_records(doc: &Document, fonts: &Fonts, cache: &mut LayoutCache) -> (Snapshot, Vec<PageRecord>) {
    layout_showing(doc, fonts, cache, None, None)
}

pub fn layout_expanded(doc: &Document, fonts: &Fonts, expansions: &super::expansion::Expansions) -> Snapshot {
    layout_showing(doc, fonts, &mut LayoutCache::new(), Some(expansions), None).0
}

fn layout_showing(
    doc: &Document,
    fonts: &Fonts,
    cache: &mut LayoutCache,
    expansions: Option<&super::expansion::Expansions>,
    hyphenation: Option<&super::hyphenate::Hyphenation>,
) -> (Snapshot, Vec<PageRecord>) {
    let env = LayoutEnv::new(doc, hyphenation);
    let ctx = LayoutCtx { fonts, env: &env, preview: None, expansions };
    let mut job = LayoutJob::new(doc, Revision(0));
    job.run(doc, &ctx, cache, None, Budget::Unlimited);
    let done = job.finished().expect("an unlimited run completes the job");
    (done.snapshot(Revision(0), LayoutRevision(0)), done.records.clone())
}

// Test hooks
#[cfg(test)]
thread_local! {
    pub(crate) static HF_ALTERNATE: Cell<f64> = const { Cell::new(0.0) };
    pub(crate) static FAIL_VERIFY: Cell<bool> = const { Cell::new(false) };
}

// Pass schedule
const FREE_PASSES: usize = 4;
const RESERVE_PASS: usize = FREE_PASSES + 1;

fn hook_height(k: usize) -> f64 {
    #[cfg(test)]
    if matches!(k, 1 | 3) {
        return HF_ALTERNATE.get();
    }
    let _ = k;
    0.0
}

pub(crate) fn same_heights(a: &HfHeights, b: &HfHeights) -> bool {
    a.iter().zip(b).all(|(x, y)| match (x, y) {
        (None, None) => true,
        (Some(x), Some(y)) => (x - y).abs() < EPS,
        _ => false,
    })
}

pub(crate) fn used(pg: &PageRecord) -> HfHeights {
    pg.bodies.first().map_or([None, None], |b| b.space.hf)
}

pub struct PassCx<'d> {
    pub doc: &'d Document,
    pub ctx: LayoutCtx<'d>,
    pub notes: &'d JobNotes,
    facts: &'d DocFacts,
    /// The section being laid out, built when it is first asked for.
    node: RefCell<Option<Rc<SectionNode<'d>>>>,
}

/// What every pass and run of a job reads from the whole document, read once.
pub struct DocFacts {
    /// The headers' and footers' `STYLEREF` codes and what each looks for.
    wanted: Vec<(String, crate::docx::fields::StyleQuery)>,
    /// What a `STYLEREF` can show from each body paragraph.
    paras: HashMap<ParaId, Vec<(String, bool, String)>>,
    /// The body paragraphs' list numbers, when a `STYLEREF` there shows one.
    numbers: HashMap<u64, crate::docx::fields::ParaNumber>,
    chapters: Chapters,
    hf_sources: HfSources,
}

/// Section `si` with the endnotes that follow it.
fn section_node<'d>(doc: &'d Document, notes: &'d JobNotes, si: usize) -> SectionNode<'d> {
    let s = &doc.sections[si];
    let mut blocks = Blocks::of(&s.blocks);
    if notes.follow(si) {
        blocks.append(notes.separator.iter());
        for id in notes.after(si) {
            blocks.append(notes.stories(doc, 1).get(&id).into_iter().flatten());
        }
    }
    let next = match doc.sections.get(si + 1).and_then(|n| n.blocks.first()) {
        Some(Block::Para(p)) => Some(&**p.para_style_in(doc.review.view)),
        _ => None,
    };
    SectionNode { index: si, section: s, blocks, breaks: si + 1 < doc.sections.len(), next }
}

impl DocFacts {
    pub fn new(doc: &Document, notes: &JobNotes) -> DocFacts {
        fn codes(blocks: &[Block], out: &mut Vec<String>) {
            for b in blocks {
                match b {
                    Block::Para(p) => {
                        for s in crate::docx::fields::spans(&p.inlines) {
                            if let Inline::FieldBegin { kind: Some(FieldKind::StyleRef(_)), .. } = p.inlines[s.begin] {
                                let code = crate::docx::fields::code_text(&p.inlines, s).trim().to_string();
                                if !out.contains(&code) {
                                    out.push(code);
                                }
                            }
                        }
                    }
                    Block::Table(t) => t.rows.iter().flat_map(|r| &r.cells).for_each(|c| codes(&c.blocks, out)),
                }
            }
        }
        let mut wanted = Vec::new();
        doc.stories.values().for_each(|b| codes(b, &mut wanted));
        let styles = doc.resolver.as_deref().map(|r| &r.styles);
        let wanted: Vec<_> = wanted.into_iter().map(|code| (code.clone(), crate::docx::fields::StyleQuery::new(&code, styles))).collect();
        let mut paras = HashMap::new();
        if !wanted.is_empty() {
            for si in 0..doc.sections.len() {
                for b in section_node(doc, notes, si).blocks.iter() {
                    if let Block::Para(p) = b {
                        paras.insert(ParaId(p.id), crate::docx::fields::style_hits(p));
                    }
                }
            }
        }
        let numbers = if wanted.iter().any(|(_, q)| q.numbered()) { crate::docx::fields::para_numbers(doc) } else { HashMap::new() };
        DocFacts { wanted, paras, numbers, chapters: chapters(doc), hf_sources: HfSources::new(&doc.sections) }
    }
}

impl<'d> PassCx<'d> {
    pub fn new(doc: &'d Document, ctx: LayoutCtx<'d>, notes: &'d JobNotes, facts: &'d DocFacts) -> PassCx<'d> {
        PassCx { doc, ctx, notes, facts, node: RefCell::new(None) }
    }

    /// Section `si` as laid out.
    pub fn node(&self, si: usize) -> Rc<SectionNode<'d>> {
        let mut node = self.node.borrow_mut();
        match &*node {
            Some(n) if n.index == si => n.clone(),
            _ => node.insert(Rc::new(section_node(self.doc, self.notes, si))).clone(),
        }
    }

    fn notes(&self) -> Notes<'d> {
        Notes { footnotes: self.notes.stories(self.doc, 0), endnotes: self.notes.stories(self.doc, 1), separators: &self.doc.separators }
    }

    pub fn paginator<'p>(&'p self, run: &'p mut PassRun) -> Paginator<'d, 'p> {
        Paginator {
            ctx: self.ctx,
            sections: &self.doc.sections,
            stories: &self.doc.stories,
            notes: self.notes(),
            note_numbers: &self.notes.numbers,
            chapters: self.facts.chapters.body.clone(),
            hf_sources: &self.facts.hf_sources,
            run,
        }
    }
}

/// Where a section with columns ends its body before the endnotes that follow it, when its body is balanced first and
/// the endnotes start below it. Compat ≤ 14: the endnotes after a section that isn't the last stay in its flow.
pub(crate) fn endnote_split(cx: &PassCx, si: usize) -> Option<usize> {
    let doc = cx.doc;
    let in_flow = doc.settings.compat() == crate::docx::Compat::Legacy && si + 1 < doc.sections.len();
    (doc.sections[si].section_in(doc.review.view).columns.num > 1 && cx.notes.follow(si) && !in_flow).then(|| doc.sections[si].blocks.len())
}

/// The next place after `block` where the section's layout starts again below what came before: the end of its body
/// before its endnotes (see [`endnote_split`]) and, compat 15, the end of their separator, the endnotes balanced on
/// their own below it.
fn next_split(cx: &PassCx, si: usize, block: usize) -> Option<usize> {
    let e = endnote_split(cx, si)?;
    let sep = (cx.doc.settings.compat() == crate::docx::Compat::Generic).then(|| e + cx.notes.separator.len());
    [Some(e), sep].into_iter().flatten().find(|&k| block < k)
}

/// Whether the section's columns are balanced on the page starting at `token`: the endnotes started below a balanced
/// body aren't in compat ≤ 14; in compat 15 they balance on their own below their separator when the next section goes
/// on on the page.
pub(crate) fn balances(cx: &PassCx, si: usize, token: &PageToken) -> bool {
    let doc = cx.doc;
    let nsec = doc.sections.len();
    let off = doc.settings.compat() == crate::docx::Compat::Legacy && doc.settings.policy.no_column_balance;
    let mut split = endnote_split(cx, si);
    if split.is_some_and(|e| token.flow.block >= e) {
        if next_split(cx, si, token.flow.block).is_some() || doc.settings.compat() == crate::docx::Compat::Legacy {
            return false;
        }
        split = None;
    }
    (!off
        && doc.sections[si].section_in(doc.review.view).columns.num > 1
        && si + 1 < nsec
        && super::vertical::same_page(&doc.sections[si], &doc.sections[si + 1]))
        || split.is_some()
}

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
            // Compat ≤ 14: a continuous break starts a new page after a footnote in the page-bottom area, unless
            // `footnoteLayoutLikeWW8` keeps the section on the page above it.
            let bottom_notes = doc.settings.compat() == crate::docx::Compat::Legacy
                && !cx.ctx.env.notes_like_ww8
                && p.run.pages.last().is_some_and(|pg| {
                    pg.bodies
                        .iter()
                        .any(|b| !doc.sections[b.space.section].notes[0].beneath && b.effects.notes_shown.iter().any(|n| n.0 == NoteKind::Foot))
                });
            let continuous = si > 0 && super::vertical::same_page(&doc.sections[si - 1], sec) && !bottom_notes;
            let mut top = y;
            if !continuous || p.run.pages.is_empty() {
                let want_odd = match sec.break_kind {
                    SectionBreak::OddPage => Some(true),
                    SectionBreak::EvenPage => Some(false),
                    _ => None,
                };
                let wrong = |n: i64| want_odd.is_some_and(|odd| (n.rem_euclid(2) == 1) != odd);
                let bump = |n: i64| if wrong(n) { n + 1 } else { n };
                // Facing pages: a page whose number has the wrong parity for its place is preceded by a blank one;
                // the section's start is tried again on the next page.
                let facing = cx.ctx.env.mirror_margins || cx.ctx.env.even_and_odd;
                let mut blank = false;
                loop {
                    // The next page's place: odd or even counted from the first page's number.
                    let place = p.run.pages.first().map(|pg| pg.number + p.run.pages.len() as i64);
                    let n = match (sec.pg_start, place) {
                        (Some(s), _) => bump(s),
                        (None, None) => bump(1),
                        (None, Some(_)) => p.run.page_number + 1,
                    };
                    p.run.page_number = n - 1;
                    let skip = !blank && place.is_some_and(|k| wrong(n) || facing && (k - n).rem_euclid(2) != 0);
                    p.new_page(si, skip);
                    blank = skip;
                    if !skip {
                        break;
                    }
                }
                carry.keep_top = true;
                if sec.line_numbers.as_ref().is_some_and(|l| l.restart != crate::docx::LineNumberingRestart::Continuous) {
                    carry.line_no = 0;
                }
                carry.prev = None;
                top = None;
            } else if sec.line_numbers.as_ref().is_some_and(|l| l.restart == crate::docx::LineNumberingRestart::NewSection) {
                carry.line_no = 0;
            }
            run.cursor = Cursor::Body { si, token: PageToken { section: si, flow: FlowToken::default(), state: carry }, top };
        }
        Cursor::Body { si, token, top } => {
            if top.is_none()
                && let Some(r) = reuse
                && let Some(next) = r.reuse_page(run, cx, si, &token, memos)?
            {
                crate::prof::count(crate::prof::Piece::Page, crate::prof::Outcome::Reused);
                run.cursor = next;
                return memos.meter.page_done();
            }
            let balance = balances(cx, si, &token);
            let split = next_split(cx, si, token.flow.block);
            let n = cx.node(si);
            let body_only;
            let node = match split {
                Some(e) => {
                    body_only = SectionNode { index: n.index, section: n.section, blocks: n.blocks.first(e), breaks: n.breaks, next: n.next };
                    &body_only
                }
                None => &*n,
            };
            let mut p = cx.paginator(run);
            let next = p.body(node, token, top, balance, memos)?;
            crate::prof::count(crate::prof::Piece::Page, crate::prof::Outcome::Laid);
            run.cursor = match next {
                Next::Page(token) => Cursor::Body { si, token, top: None },
                Next::SectionEnd(carry, y) if let Some(e) = split => {
                    let flow = FlowToken { block: e, ..FlowToken::default() };
                    Cursor::Body { si, token: PageToken { section: si, flow, state: carry }, top: Some(y) }
                }
                Next::SectionEnd(carry, y) => Cursor::Section { si: si + 1, carry, y: Some(y) },
            };
            memos.meter.page_done()?;
        }
        Cursor::Compose(i) => {
            if run.info.is_none() {
                run.info = Some(pass_info(cx, &run.pages));
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
            if let Budget::Until(_) = memos.meter.budget() {
                memos.meter.page_done()?;
            }
        }
        Cursor::Done => {}
    }
    Ok(())
}

pub struct Finished {
    pub records: Vec<PageRecord>,
    pub pages: Vec<(Rc<PageFragment>, PageMeta)>,
    pub status: LayoutStatus,
    pub passes: usize,
    pub known: Rc<PassInfo>,
}

impl Finished {
    pub fn snapshot(&self, revision: Revision, layout_revision: LayoutRevision) -> Snapshot {
        Snapshot::new(revision, layout_revision, self.status.clone(), self.pages.clone(), excluded(&self.records))
    }
}

pub struct LayoutJob {
    pub revision: Revision,
    notes: JobNotes,
    body_fields: bool,
    pub(crate) run: PassRun,
    history: Vec<Vec<HfHeights>>,
    finished: Option<Finished>,
    /// Note numbers the last pass found different on its pages, for the next pass.
    renumber: Option<HashMap<(NoteKind, i64), String>>,
    facts: Option<Rc<DocFacts>>,
    /// The tables a live drag lays out (see [`crate::layout::flow::PassMemo::live`]).
    pub live: Rc<[u64]>,
}

impl LayoutJob {
    pub fn new(doc: &Document, revision: Revision) -> LayoutJob {
        LayoutJob::with_notes(revision, JobNotes::new(doc), crate::docx::fields::page_dependent(doc))
    }

    pub fn with_notes(revision: Revision, notes: JobNotes, body_fields: bool) -> LayoutJob {
        LayoutJob {
            revision,
            notes,
            body_fields,
            run: PassRun::new(1, Rc::new(PassInfo::default()), Rc::new(Vec::new()), false),
            history: Vec::new(),
            finished: None,
            renumber: None,
            facts: None,
            live: Rc::from([]),
        }
    }

    pub fn finished(&self) -> Option<&Finished> {
        self.finished.as_ref()
    }

    pub fn seed(&mut self, known: PassInfo, hf_in: Vec<HfHeights>) {
        self.run = PassRun::new(1, Rc::new(known), Rc::new(hf_in), false);
    }

    pub fn into_finished(self) -> Option<Finished> {
        self.finished
    }

    pub fn job_notes(&self) -> &JobNotes {
        &self.notes
    }

    pub fn note_numbers(&self) -> &HashMap<(NoteKind, i64), String> {
        &self.notes.numbers
    }

    pub fn endnotes(&self) -> &[i64] {
        &self.notes.endnotes
    }

    pub fn committed(&self) -> &[PageRecord] {
        match &self.finished {
            Some(f) => &f.records,
            None => &self.run.pages,
        }
    }

    pub fn run(&mut self, doc: &Document, ctx: &LayoutCtx, cache: &mut LayoutCache, mut reuse: Option<&mut Reuse>, budget: Budget) -> Step {
        if self.finished.is_some() {
            return Step::Done;
        }
        cache.begin();
        let mut meter = Meter::new(budget);
        let mut notes = std::mem::take(&mut self.notes);
        let facts = self.facts(doc, &notes);
        let step_result = loop {
            let cx = PassCx::new(doc, *ctx, &notes, &facts);
            let mut memos = Memos { cache, meter: &mut meter };
            let r = loop {
                if let Cursor::Done = self.run.cursor {
                    if self.end_pass(doc, &notes) {
                        break Some(Step::Done);
                    }
                    if self.renumber.is_some() {
                        break None;
                    }
                    continue;
                }
                let r = if self.run.k == 1 { reuse.as_deref_mut() } else { None };
                self.run.memo.live.clone_from(&self.live);
                if step(&mut self.run, &cx, r, &mut memos).is_err() {
                    break Some(Step::Yielded);
                }
            };
            drop(cx);
            match r {
                Some(s) => break s,
                None => notes.numbers.extend(self.renumber.take().into_iter().flatten()),
            }
        };
        self.notes = notes;
        step_result
    }

    pub fn compose_committed(&mut self, doc: &Document, ctx: &LayoutCtx, cache: &mut LayoutCache, pages: usize) {
        if self.finished.is_some() || self.run.info.is_some() {
            return;
        }
        let notes = std::mem::take(&mut self.notes);
        let facts = self.facts(doc, &notes);
        {
            let cx = PassCx::new(doc, *ctx, &notes, &facts);
            let mut meter = Meter::new(Budget::Unlimited);
            let mut memos = Memos { cache, meter: &mut meter };
            let run = &mut self.run;
            let n = run.pages.len();
            let waiting = |pg: &PageRecord| !pg.composed && (pg.blank || !pg.bodies.is_empty());
            if run.pages[run.composed_to.min(n)..].iter().any(waiting) {
                let num_pages = n.max(pages).to_string();
                let chapters = facts.chapters.body.as_deref();
                let info = run.provisional.info(doc, chapters, &run.pages, num_pages);
                #[cfg(test)]
                {
                    let all = PassInfo {
                        num_pages: info.num_pages.clone(),
                        section_pages: Rc::new(section_pages(&run.pages, doc.sections.len())),
                        page_refs: Rc::new(bookmark_pages(doc, chapters, &run.pages)),
                        para_at: Rc::new(para_at(&run.pages)),
                    };
                    assert_eq!(info, all, "the pass's pages so far, kept as they are added, are as read again");
                }
                run.info = Some(info);
                for i in run.composed_to..n {
                    if waiting(&run.pages[i]) {
                        let _ = compose_page(run, &cx, i, &mut memos);
                    }
                }
                run.info = None;
            }
            while run.composed_to < n && run.pages[run.composed_to].composed {
                run.composed_to += 1;
            }
        }
        self.notes = notes;
    }

    /// The document's [`DocFacts`], read on the job's first run.
    fn facts(&mut self, doc: &Document, notes: &JobNotes) -> Rc<DocFacts> {
        self.facts.get_or_insert_with(|| Rc::new(DocFacts::new(doc, notes))).clone()
    }

    /// The document changed in what layout reads, though nothing needs laying out again.
    pub fn document_changed(&mut self) {
        self.facts = None;
        self.run.found = Default::default();
    }

    fn end_pass(&mut self, doc: &Document, notes: &JobNotes) -> bool {
        let run = &self.run;
        let info = run.info.clone().unwrap_or_default();
        let composed: Vec<HfHeights> = run.pages.iter().map(|p| p.hf_full).collect();
        if run.k <= FREE_PASSES {
            self.renumber = notes.renumbered(&run.pages);
            let fields_agree = (!self.body_fields || info == *run.known) && self.renumber.is_none();
            let hf_agree = run.pages.iter().all(|pg| same_heights(&used(pg), &pg.hf_full));
            if fields_agree && hf_agree {
                self.finish(LayoutStatus::Converged);
                return true;
            }
            self.history.push(composed.clone());
            if run.k < FREE_PASSES {
                self.run = PassRun::new(run.k + 1, Rc::new(info), Rc::new(composed), false);
                return false;
            }
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
            self.run = PassRun::new(RESERVE_PASS, Rc::new(info), Rc::new(reserve), true);
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
        let known = run.known.clone();
        let records = run.pages;
        let pages = records
            .iter()
            .scan(HashMap::<usize, usize>::new(), |seen, pg| {
                let section_page = if pg.blank { 0 } else { *seen.entry(pg.section).and_modify(|n| *n += 1).or_insert(0) };
                Some(finish_page(pg, section_page))
            })
            .collect();
        self.finished = Some(Finished { records, pages, status, passes, known });
    }
}

fn shows_pass_fields(doc: &Document, pg: &PageRecord) -> bool {
    let _ = doc;
    pg.bodies.iter().any(|b| b.fields.num_pages.is_some() || b.fields.section_pages.is_some() || b.fields.page_refs.is_some())
}

fn verify(doc: &Document, records: &[PageRecord]) -> Vec<usize> {
    let mut bad = Vec::new();
    for (i, pg) in records.iter().enumerate() {
        let (Some((top, bottom)), Some(s)) = (pg.body_area, doc.sections.get(pg.section).map(|s| s.section_in(doc.review.view))) else { continue };
        // A vertical body doesn't give way to its header and footer.
        if super::vertical::of_section(doc.sections[pg.section].text_flow).is_some() {
            continue;
        }
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

pub(crate) fn excluded(records: &[PageRecord]) -> Vec<(usize, ParaId, std::ops::Range<Unit>)> {
    let mut out = Vec::new();
    for (i, pg) in records.iter().enumerate() {
        for (para, units) in pg.bodies.iter().flat_map(|b| b.excluded.iter()) {
            out.push((i, *para, units.clone()));
        }
    }
    out
}

fn pass_info(cx: &PassCx, pages: &[PageRecord]) -> PassInfo {
    PassInfo {
        num_pages: Some(pages.len().to_string()),
        section_pages: Rc::new(section_pages(pages, cx.doc.sections.len())),
        page_refs: Rc::new(bookmark_pages(cx.doc, cx.facts.chapters.body.as_deref(), pages)),
        para_at: Rc::new(para_at(pages)),
    }
}

/// [`PassInfo`] of a pass's pages so far, for the headers and footers composed before it ends, kept as pages are added:
/// the pages before the last are folded in once, and the last, which can still change, each time, what it changed
/// undone before the next.
#[derive(Default)]
pub struct Provisional {
    /// The pages folded in for good.
    settled: usize,
    section_pages: Rc<Vec<String>>,
    page_refs: Rc<HashMap<String, PageRef>>,
    para_at: Rc<HashMap<ParaId, (usize, usize)>>,
    /// What the last page replaced: its section's count, each bookmark's page before it, the paragraphs it added.
    undo: (Option<(usize, String)>, Vec<(String, Option<PageRef>)>, Vec<ParaId>),
}

impl Provisional {
    /// [`pass_info`] of `pages`, with `num_pages`.
    pub(super) fn info(&mut self, doc: &Document, chapters: Option<&HashMap<ParaId, String>>, pages: &[PageRecord], num_pages: String) -> PassInfo {
        let nsec = doc.sections.len();
        if self.section_pages.len() != nsec || self.settled > pages.len() {
            *self = Provisional { section_pages: Rc::new(vec!["0".to_string(); nsec]), ..Provisional::default() };
        }
        let (counts, refs, at) = (Rc::make_mut(&mut self.section_pages), Rc::make_mut(&mut self.page_refs), Rc::make_mut(&mut self.para_at));
        let (count, earlier, added) = &mut self.undo;
        if let Some((s, c)) = count.take() {
            counts[s] = c;
        }
        for (name, was) in earlier.drain(..).rev() {
            match was {
                Some(r) => refs.insert(name, r),
                None => refs.remove(&name),
            };
        }
        for p in added.drain(..) {
            at.remove(&p);
        }
        let settled = pages.len().saturating_sub(1);
        let mut fold = |k: usize, last: bool| {
            let pg = &pages[k];
            if !pg.blank
                && let Some(c) = counts.get_mut(pg.section)
            {
                let was = std::mem::replace(c, pg.in_section.to_string());
                if last {
                    *count = Some((pg.section, was));
                }
            }
            let fmt = doc.sections.get(pg.section).and_then(|s| s.pg_fmt.as_ref());
            for (name, para) in &pg.effects.bookmarks {
                let chapter = chapters.and_then(|c| c.get(para)).cloned();
                let was = refs.insert(name.clone(), PageRef { number: pg.number, fmt: fmt.cloned(), chapter, page: k, para: *para });
                if last {
                    earlier.push((name.clone(), was));
                }
            }
            if pg.effects.relative > 0 {
                for (j, p) in pg.effects.paras.iter().enumerate() {
                    if !at.contains_key(p) {
                        at.insert(*p, (k, j));
                        if last {
                            added.push(*p);
                        }
                    }
                }
            }
        };
        for k in self.settled..settled {
            fold(k, false);
        }
        if settled < pages.len() {
            fold(settled, true);
        }
        self.settled = settled;
        PassInfo {
            num_pages: Some(num_pages),
            section_pages: self.section_pages.clone(),
            page_refs: self.page_refs.clone(),
            para_at: self.para_at.clone(),
        }
    }
}

/// The page and place on it of each paragraph on the pages holding reference fields with `\p` (those fields tell
/// above from below by it).
fn para_at(pages: &[PageRecord]) -> HashMap<ParaId, (usize, usize)> {
    let mut out = HashMap::new();
    for (i, pg) in pages.iter().enumerate().filter(|(_, pg)| pg.effects.relative > 0) {
        for (k, p) in pg.effects.paras.iter().enumerate() {
            out.entry(*p).or_insert((i, k));
        }
    }
    out
}

/// Each of the `nsec` sections' pages, blank ones not counted, as `SECTIONPAGES` shows them.
fn section_pages(pages: &[PageRecord], nsec: usize) -> Vec<String> {
    let mut counts = vec![0usize; nsec];
    for pg in pages.iter().filter(|pg| !pg.blank) {
        if let Some(c) = counts.get_mut(pg.section) {
            *c += 1;
        }
    }
    counts.iter().map(usize::to_string).collect()
}

/// Each bookmark's page as `PAGEREF` shows it, with `chapters`' prefix for the paragraph it starts in.
pub(crate) fn bookmark_pages(doc: &Document, chapters: Option<&HashMap<ParaId, String>>, pages: &[PageRecord]) -> HashMap<String, PageRef> {
    let mut out = HashMap::new();
    for (page, pg) in pages.iter().enumerate() {
        let fmt = doc.sections.get(pg.section).and_then(|s| s.pg_fmt.as_ref());
        for (name, para) in &pg.effects.bookmarks {
            let chapter = chapters.and_then(|c| c.get(para)).cloned();
            out.insert(name.clone(), PageRef { number: pg.number, fmt: fmt.cloned(), chapter, page, para: *para });
        }
    }
    out
}

/// [`bookmark_pages`] of a finished layout of `doc`.
pub(crate) fn bookmark_pages_of(doc: &Document, pages: &[PageRecord]) -> HashMap<String, PageRef> {
    bookmark_pages(doc, chapters(doc).body.as_deref(), pages)
}
