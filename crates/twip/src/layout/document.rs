//! Document layout passes

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

pub fn layout(doc: &Document, fonts: &Fonts) -> Snapshot {
    let mut cache = LayoutCache::new();
    layout_with(doc, fonts, &mut cache)
}

/// Stepped layout, stoppable between steps.
pub fn layout_until(doc: &Document, fonts: &Fonts, mut stop: impl FnMut() -> bool) -> Option<Snapshot> {
    let env = LayoutEnv::new(doc);
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
    layout_showing(doc, fonts, cache, None)
}

pub fn layout_expanded(doc: &Document, fonts: &Fonts, expansions: &super::expansion::Expansions) -> Snapshot {
    layout_showing(doc, fonts, &mut LayoutCache::new(), Some(expansions)).0
}

fn layout_showing(
    doc: &Document,
    fonts: &Fonts,
    cache: &mut LayoutCache,
    expansions: Option<&super::expansion::Expansions>,
) -> (Snapshot, Vec<PageRecord>) {
    let env = LayoutEnv::new(doc);
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

pub(crate) fn used(pg: &PageRecord) -> HfHeights {
    pg.bodies.first().map_or([None, None], |b| b.space.hf)
}

#[derive(Clone, Default)]
pub struct JobNotes {
    pub numbers: HashMap<(NoteKind, i64), String>,
    endnotes: Vec<i64>,
    /// The section each endnote follows (by `endnotes`' order).
    ends: Vec<usize>,
    separator: Vec<Block>,
    /// The automatic notes numbered by page, in reference order, with their section's start and format.
    paged: Vec<(NoteKind, i64, i64, String)>,
    /// Compat 15: the notes numbered by their page's note keys ([`JobNotes::renumbered`]).
    keyed: Vec<Keyed>,
}

/// Compat 15, a note restarting each page: its number is the rank of its key among the keys used on its page, in the
/// footnote number format. The key is a footnote story's start: a footnote's own, or for an endnote's mark the
/// footnote story holding the endnote story's start read as a footnote position.
#[derive(Clone)]
struct Keyed {
    kind: NoteKind,
    id: i64,
    key: i64,
    fmt: String,
    /// An endnote mark: its note's first paragraph (the page it is drawn on).
    mark: Option<ParaId>,
}

/// The number key of an endnote's mark when it differs from its reference's.
pub fn mark_key(kind: NoteKind, id: i64) -> (NoteKind, i64) {
    (kind, !id)
}

/// Each note's story start in its kind's stories laid end to end in reference order, and the end of the last one:
/// a story is its paragraphs' units, one more per paragraph mark.
fn story_starts(notes: &HashMap<i64, Vec<Block>>, ids: impl Iterator<Item = i64>) -> (Vec<(i64, i64)>, i64) {
    fn len(blocks: &[Block]) -> i64 {
        blocks
            .iter()
            .map(|b| match b {
                Block::Para(p) => p.inlines.iter().map(Inline::units).sum::<usize>() as i64 + 1,
                // ponytail: a table counts its cells' paragraphs, not its cell and row marks.
                Block::Table(t) => t.rows.iter().flat_map(|r| r.cells.iter()).map(|c| len(&c.blocks)).sum(),
            })
            .sum()
    }
    let mut at = 0;
    let starts = ids
        .map(|id| {
            let start = at;
            at += notes.get(&id).map_or(0, |b| len(b));
            (id, start)
        })
        .collect();
    (starts, at)
}

impl JobNotes {
    pub fn new(doc: &Document) -> JobNotes {
        let mut refs = Vec::new();
        let mut seen = HashSet::new();
        let mut numbers = HashMap::new();
        // Automatic notes counted per kind (foot, end) since the restart origin.
        let mut count = [0, 0];
        let mut ends = Vec::new();
        let mut paged = Vec::new();
        let mut keyed = Vec::new();
        let nsec = doc.sections.len();
        let generic = doc.settings.compat() == crate::docx::Compat::Generic;
        let (mut all, mut all_seen) = (Vec::new(), HashSet::new());
        for s in &doc.sections {
            collect_refs(&s.blocks, &mut all, &mut all_seen);
        }
        let of = |k: NoteKind| all.iter().filter(move |r| r.0 == k).map(|r| r.1);
        let (foot_starts, foot_end) = story_starts(&doc.footnotes, of(NoteKind::Foot));
        let (end_starts, _) = story_starts(&doc.endnotes, of(NoteKind::End));
        for (si, s) in doc.sections.iter().enumerate() {
            let from = refs.len();
            collect_refs(&s.blocks, &mut refs, &mut seen);
            for (k, p) in s.notes.iter().enumerate() {
                if p.restart == NoteRestart::EachSect {
                    count[k] = 0;
                }
            }
            for &(kind, id, custom) in &refs[from..] {
                let k = usize::from(kind == NoteKind::End);
                let p = &s.notes[k];
                let fmt = p.fmt.as_deref().unwrap_or(if k == 0 { "decimal" } else { "lowerRoman" });
                let mut number = crate::docx::numbering::format_number(p.start + count[k], fmt);
                if p.restart == NoteRestart::EachPage && !custom {
                    if generic {
                        // Keys are looked up among the footnotes: an endnote's reference finds none and shows 8, as
                        // does a mark that resolves to none.
                        let foot_fmt = s.notes[0].fmt.as_deref().unwrap_or("decimal");
                        let start_of = |v: &[(i64, i64)], id: i64| v.iter().find(|e| e.0 == id).map(|e| e.1);
                        let key = match kind {
                            NoteKind::Foot => start_of(&foot_starts, id),
                            NoteKind::End => start_of(&end_starts, id)
                                .filter(|&cp| cp < foot_end)
                                .and_then(|cp| foot_starts.iter().rev().find(|e| e.1 <= cp).map(|e| e.1)),
                        };
                        if kind == NoteKind::End {
                            number = crate::docx::numbering::format_number(8, foot_fmt);
                        }
                        let first_para = |b: &Vec<Block>| b.iter().find_map(|b| if let Block::Para(p) = b { Some(ParaId(p.id)) } else { None });
                        let mark = if kind == NoteKind::End { doc.endnotes.get(&id).and_then(first_para) } else { None };
                        match key {
                            Some(key) if kind == NoteKind::Foot || mark.is_some() => {
                                keyed.push(Keyed { kind, id, key, fmt: foot_fmt.to_string(), mark });
                            }
                            _ if kind == NoteKind::End => {
                                numbers.insert(mark_key(kind, id), crate::docx::numbering::format_number(8, foot_fmt));
                            }
                            _ => {}
                        }
                    } else {
                        paged.push((kind, id, p.start, fmt.to_string()));
                    }
                }
                numbers.insert((kind, id), number);
                count[k] += i64::from(!custom);
                if kind == NoteKind::End {
                    // After its section, or the next one without `w:noEndnote`; all at the end with docEnd.
                    let end = (si..nsec).find(|&e| !doc.sections[e].no_endnote).unwrap_or(nsec - 1);
                    ends.push(if doc.settings.endnotes_sect_end { end } else { nsec - 1 });
                }
            }
        }
        let endnotes: Vec<i64> = refs.iter().filter(|r| r.0 == NoteKind::End).map(|r| r.1).collect();
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
        JobNotes { numbers, endnotes, ends, separator, paged, keyed }
    }
}

impl JobNotes {
    /// The numbers of the notes restarting each page, counted per kind on the pages their references landed on, where
    /// they differ from the current ones.
    fn renumbered(&self, pages: &[PageRecord]) -> Option<HashMap<(NoteKind, i64), String>> {
        if self.paged.is_empty() && self.keyed.is_empty() {
            return None;
        }
        // A footnote's first part is drawn on its reference's page; an endnote's reference is shown there.
        let page_of = |kind, id| pages.iter().position(|pg| pg.effects.notes_shown.iter().any(|n| (n.0, n.1) == (kind, id)));
        let mut count: HashMap<(NoteKind, usize), i64> = HashMap::new();
        let mut changed = HashMap::new();
        for &(kind, id, start, ref fmt) in &self.paged {
            let Some(pi) = page_of(kind, id) else { continue };
            let c = count.entry((kind, pi)).or_insert(0);
            let n = crate::docx::numbering::format_number(start + *c, fmt);
            *c += 1;
            if self.numbers.get(&(kind, id)) != Some(&n) {
                changed.insert((kind, id), n);
            }
        }
        // Compat 15: the rank of each note's key among the keys its page uses.
        let on: Vec<Option<usize>> = self
            .keyed
            .iter()
            .map(|k| match k.mark {
                Some(p) => pages.iter().position(|pg| pg.effects.paras.contains(&p)),
                None => page_of(k.kind, k.id),
            })
            .collect();
        for (k, pi) in self.keyed.iter().zip(&on) {
            let Some(pi) = pi else { continue };
            let mut keys: Vec<i64> = self.keyed.iter().zip(&on).filter(|(_, p)| **p == Some(*pi)).map(|(k, _)| k.key).collect();
            keys.sort_unstable();
            keys.dedup();
            let n = crate::docx::numbering::format_number(keys.partition_point(|&x| x < k.key) as i64 + 1, &k.fmt);
            let at = if k.mark.is_some() { mark_key(k.kind, k.id) } else { (k.kind, k.id) };
            if self.numbers.get(&at) != Some(&n) {
                changed.insert(at, n);
            }
        }
        (!changed.is_empty()).then_some(changed)
    }

    /// The endnotes that follow section `si`.
    fn after(&self, si: usize) -> impl Iterator<Item = i64> + '_ {
        self.endnotes.iter().zip(&self.ends).filter(move |e| *e.1 == si).map(|e| *e.0)
    }
}

/// The block holds a footnote reference.
pub fn has_footnote_refs(b: &Block) -> bool {
    match b {
        Block::Para(p) => p.inlines.iter().any(|i| matches!(i, Inline::NoteRef { kind: NoteKind::Foot, .. })),
        Block::Table(t) => t.rows.iter().flat_map(|r| r.cells.iter()).flat_map(|c| c.blocks.iter()).any(has_footnote_refs),
    }
}

pub fn has_note_refs(b: &Block) -> bool {
    match b {
        Block::Para(p) => p.inlines.iter().any(|i| matches!(i, Inline::NoteRef { .. })),
        Block::Table(t) => t.rows.iter().flat_map(|r| r.cells.iter()).flat_map(|c| c.blocks.iter()).any(has_note_refs),
    }
}

/// Each note's first reference: kind, id, and whether it carries a custom mark.
fn collect_refs(blocks: &[Block], out: &mut Vec<(NoteKind, i64, bool)>, seen: &mut HashSet<(NoteKind, i64)>) {
    for b in blocks {
        match b {
            Block::Para(p) => {
                for i in &p.inlines {
                    if let Inline::NoteRef { kind, id, custom, .. } = i
                        && seen.insert((*kind, *id))
                    {
                        out.push((*kind, *id, *custom));
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

pub struct PassCx<'d> {
    pub doc: &'d Document,
    pub ctx: LayoutCtx<'d>,
    pub nodes: Vec<SectionNode<'d>>,
    pub notes: &'d JobNotes,
    /// The headers' and footers' `STYLEREF` codes and what each looks for.
    wanted: Vec<(String, crate::docx::fields::StyleQuery)>,
    /// What a `STYLEREF` can show from each body paragraph.
    paras: HashMap<ParaId, Vec<(String, bool, String)>>,
    chapters: Chapters,
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
                if notes.ends.contains(&si) {
                    blocks.append(notes.separator.iter());
                    for id in notes.after(si) {
                        blocks.append(doc.endnotes.get(&id).into_iter().flatten());
                    }
                }
                let next = match doc.sections.get(si + 1).and_then(|n| n.blocks.first()) {
                    Some(Block::Para(p)) => Some(&**p.para_style_in(doc.review.view)),
                    _ => None,
                };
                SectionNode { index: si, section: s, blocks, breaks: si + 1 < nsec, next }
            })
            .collect();
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
            for n in &nodes {
                for &b in &n.blocks.blocks {
                    if let Block::Para(p) = b {
                        paras.insert(ParaId(p.id), crate::docx::fields::style_hits(p));
                    }
                }
            }
        }
        PassCx { doc, ctx, nodes, notes, wanted, paras, chapters: chapters(doc) }
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
            chapters: self.chapters.body.clone(),
            run,
        }
    }
}

/// Where a section with columns ends its body before the endnotes that follow it, when its body is balanced first and
/// the endnotes start below it. Compat ≤ 14: the endnotes after a section that isn't the last stay in its flow.
pub(crate) fn endnote_split(cx: &PassCx, si: usize) -> Option<usize> {
    let doc = cx.doc;
    let in_flow = doc.settings.compat() == crate::docx::Compat::Legacy && si + 1 < doc.sections.len();
    (doc.sections[si].section_in(doc.review.view).columns.num > 1 && cx.notes.ends.contains(&si) && !in_flow).then(|| doc.sections[si].blocks.len())
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
    let off = doc.settings.compat() == crate::docx::Compat::Legacy && doc.settings.has("noColumnBalance");
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
                if sec.line_numbers.as_ref().is_some_and(|l| l.restart != "continuous") {
                    carry.line_no = 0;
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
                crate::prof::count(crate::prof::Piece::Page, crate::prof::Outcome::Reused);
                run.cursor = next;
                return memos.meter.page_done();
            }
            let balance = balances(cx, si, &token);
            let split = next_split(cx, si, token.flow.block);
            let body_only;
            let node = match split {
                Some(e) => {
                    let n = &cx.nodes[si];
                    body_only = SectionNode {
                        index: n.index,
                        section: n.section,
                        blocks: Blocks { blocks: n.blocks.blocks[..e].to_vec(), stories: Vec::new() },
                        breaks: n.breaks,
                        next: n.next,
                    };
                    &body_only
                }
                None => &cx.nodes[si],
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
                run.info = Some(pass_info(&run.pages, nsec));
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

// Header and footer composition
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
        chapter: page_chapter(cx, &run.pages, i),
        chapters: None,
    }
}

/// Chapter numbers (`w:pgNumType w:chapStyle`): each body paragraph's page number prefix, and the headings' levels and
/// numbers, in document order.
#[derive(Default)]
struct Chapters {
    body: Option<Rc<HashMap<ParaId, String>>>,
    heads: Vec<(ParaId, i64, String)>,
}

/// A heading's level and chapter number: a "heading N" paragraph numbered through its style, its label without the
/// level text's literal text before the first and after the last number.
fn chapter_of(r: &crate::docx::resolve::Resolver, p: &Paragraph) -> Option<(i64, String)> {
    let (label, (num, ilvl), id) = (p.list.as_ref()?, p.style.num?, p.style.style_id.as_deref()?);
    let level = r.styles.by_id.get(id)?.name.to_lowercase().strip_prefix("heading ")?.parse().ok()?;
    if r.styles.chain_ppr(Some(id)).num_id != Some(num) {
        return None;
    }
    let text = &r.numbering.level(num, ilvl, &r.styles)?.text;
    let before = &text[..text.find('%').unwrap_or(0)];
    let after = text.rfind('%').and_then(|k| text.get(k + 2..)).unwrap_or("");
    let t = label.text.strip_prefix(before).unwrap_or(&label.text);
    Some((level, t.strip_suffix(after).unwrap_or(t).to_string()))
}

fn chapters(doc: &Document) -> Chapters {
    let Some(r) = doc.resolver.as_deref().filter(|_| doc.sections.iter().any(|s| s.chapter.is_some())) else { return Chapters::default() };
    fn walk<'a>(blocks: &'a [Block], si: usize, out: &mut Vec<(usize, &'a Paragraph)>) {
        for b in blocks {
            match b {
                Block::Para(p) => out.push((si, p)),
                Block::Table(t) => t.rows.iter().flat_map(|r| &r.cells).for_each(|c| walk(&c.blocks, si, out)),
            }
        }
    }
    let mut paras = Vec::new();
    doc.sections.iter().enumerate().for_each(|(si, s)| walk(&s.blocks, si, &mut paras));
    let heads: Vec<(usize, i64, String)> = paras.iter().enumerate().filter_map(|(k, (_, p))| chapter_of(r, p).map(|(l, t)| (k, l, t))).collect();
    let mut body = HashMap::new();
    for (k, (si, p)) in paras.iter().enumerate() {
        let Some((level, sep)) = &doc.sections[*si].chapter else { continue };
        // The last heading at or before the paragraph; before the first, the first.
        let mut of_level = heads.iter().filter(|h| h.1 == *level);
        if let Some(h) = of_level.clone().take_while(|h| h.0 <= k).last().or_else(|| of_level.next()) {
            body.insert(ParaId(p.id), format!("{}{sep}", h.2));
        }
    }
    let heads = heads.into_iter().map(|(k, l, t)| (ParaId(paras[k].1.id), l, t)).collect();
    Chapters { body: Some(Rc::new(body)), heads }
}

/// The chapter prefix of page `i`'s headers and footers: from the last heading at or before the page's end.
fn page_chapter(cx: &PassCx, pages: &[PageRecord], i: usize) -> Option<String> {
    let (level, sep) = cx.doc.sections.get(pages[i].section)?.chapter.as_ref()?;
    let heads = &cx.chapters.heads;
    let head = |id: &ParaId| heads.iter().find(|h| h.0 == *id && h.1 == *level);
    let h = (0..=i).rev().find_map(|k| pages[k].effects.paras.iter().rev().find_map(head)).or_else(|| heads.iter().find(|h| h.1 == *level))?;
    Some(format!("{}{sep}", h.2))
}

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

fn compose_page(run: &mut PassRun, cx: &PassCx, i: usize, memos: &mut Memos) -> Result<(), Yield> {
    let pg = &run.pages[i];
    if pg.blank {
        return Ok(());
    }
    let (si, num) = (pg.section, pg.number);
    let sec = &cx.doc.sections[si];
    let ctx = cx.ctx;
    let (left, right, top) = margins(sec, &ctx, num);
    let s = sec.section_in(ctx.env.text.review.view);
    let width = s.page_w - left - right;
    let frames = super::float::Frames {
        page_w: s.page_w,
        page_h: s.page_h,
        margin: [left, right, top, s.bottom],
        column: (left, width),
        compat: ctx.env.text.compat,
        cell: false,
        odd: num % 2 == 1,
    };
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
        let at = |l: &super::story::StoryLayout| if footer { s.page_h - s.footer - stack_limited(l, 0.0, 0.0, limit).1 } else { s.header };
        let l = super::story::hf_layout(&blocks, width, &frames, at, &env, memos)?;
        let cap = if clip { reserved[slot].map_or(limit, |r| limit.min(r)) } else { limit };
        let story_fields = super::page::fields_used(&fields, &blocks);
        if footer {
            let (_, height) = stack_limited(&l, left, 0.0, cap);
            full[1] = Some(stack_limited(&l, left, 0.0, limit).1);
            let (mut children, _) = stack_limited(&l, left, s.page_h - s.footer - height, cap);
            children.extend(super::story::hf_floats(&blocks, &l, &frames, s.page_h - s.footer - height, &env, memos)?);
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
            let (mut children, height) = if limit.is_infinite() && !clip && s.header + l.height > 0.85 * s.page_h {
                let cut = 0.435 * s.page_h;
                let mut c = Vec::new();
                for u in l.units.iter().take_while(|u| s.header + u.top < cut) {
                    c.push(super::fragment::Child::Group { offset: (left, s.header + u.top), children: u.content.clone() });
                }
                (c, l.height)
            } else {
                stack_limited(&l, left, s.header, cap)
            };
            children.extend(super::story::hf_floats(&blocks, &l, &frames, s.header, &env, memos)?);
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
        let step_result = loop {
            let cx = PassCx::new(doc, *ctx, &notes);
            let mut memos = Memos { cache, job: None, meter: &mut meter };
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
                let _ = compose_page(run, &cx, i, &mut memos);
            }
            run.info = None;
        }
        self.notes = notes;
    }

    fn end_pass(&mut self, doc: &Document, notes: &JobNotes) -> bool {
        let run = &self.run;
        let info = run.info.clone().unwrap_or_default();
        let composed: Vec<HfHeights> = run.pages.iter().map(|p| p.hf_full).collect();
        if run.k <= 4 {
            self.renumber = notes.renumbered(&run.pages);
            let fields_agree = (!self.body_fields || info == *run.known) && self.renumber.is_none();
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

fn pass_info(pages: &[PageRecord], nsec: usize) -> PassInfo {
    PassInfo {
        num_pages: Some(pages.len().to_string()),
        section_pages: (0..nsec).map(|si| pages.iter().filter(|pg| pg.section == si && !pg.blank).count().to_string()).collect(),
        page_refs: Rc::new(bookmark_pages(pages)),
    }
}

fn bookmark_pages(pages: &[PageRecord]) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for pg in pages {
        for b in &pg.effects.bookmarks {
            out.insert(b.clone(), pg.number.to_string());
        }
    }
    out
}

/// A page's border lines: each edge inside the rectangle its outer edges are on. Art borders aren't drawn.
fn page_borders(pg: &PageRecord) -> Box<[crate::layout::fragment::Item]> {
    let Some((b, [x0, y0, x1, y1])) = &pg.page_borders else { return Box::new([]) };
    let mut out = Vec::new();
    let line = |x: &Option<crate::docx::props::Border>| x.as_ref().filter(|x| x.visible() && super::table::drawn(x)).cloned();
    let w = |x: &Option<crate::docx::props::Border>| line(x).map_or(0.0, |x| super::table::thick(&x));
    let (lw, rw) = (w(&b.left), w(&b.right));
    if let Some(t) = line(&b.top) {
        super::table::edge(&mut out, [*x0, *y0, *x1, y0 + super::table::thick(&t)], true, false, &t);
    }
    if let Some(t) = line(&b.bottom) {
        super::table::edge(&mut out, [*x0, y1 - super::table::thick(&t), *x1, *y1], true, true, &t);
    }
    if let Some(l) = line(&b.left) {
        super::table::edge(&mut out, [*x0, *y0, x0 + lw, *y1], false, false, &l);
    }
    if let Some(r) = line(&b.right) {
        super::table::edge(&mut out, [x1 - rw, *y0, *x1, *y1], false, true, &r);
    }
    out.into_boxed_slice()
}

pub(crate) fn finish_page(pg: &PageRecord, section_page: usize) -> (Rc<PageFragment>, PageMeta) {
    let mut children: Vec<PageChild> = pg.bodies.iter().map(|b| PageChild::Body(b.body.clone())).collect();
    if let Some(h) = &pg.hf_boxes[0] {
        children.push(PageChild::Header(h.clone()));
    }
    if let Some(f) = &pg.hf_boxes[1] {
        children.push(PageChild::Footer(f.clone()));
    }
    let page =
        PageFragment { id: FragmentId::next(), w: pg.w, h: pg.h, body_area: pg.body_area, children: children.into(), borders: page_borders(pg) };
    (Rc::new(page), PageMeta { section: pg.section, number: pg.number, section_page, blank: pg.blank })
}

/// Each header and footer `STYLEREF`'s text on page `i`, by its code: the first in its style on the page (`\l`: the
/// last), else the last on an earlier page, else the first on a later one.
// ponytail: a page's paragraphs whole; a character style's runs on the part of a split paragraph another page holds
// count on this one too.
fn style_refs(
    wanted: &[(String, crate::docx::fields::StyleQuery)],
    pages: &[PageRecord],
    paras: &HashMap<ParaId, Vec<(String, bool, String)>>,
    i: usize,
) -> Vec<(String, String)> {
    let on = |k: usize| pages.get(k).map(|p| &p.effects.paras[..]).unwrap_or(&[]);
    wanted
        .iter()
        .filter_map(|(code, q)| {
            let hit = |list: &[ParaId], rev: bool| {
                let find = |r: &ParaId| q.find(paras.get(r)?, rev);
                if rev { list.iter().rev().find_map(find) } else { list.iter().find_map(find) }
            };
            hit(on(i), q.last)
                .or_else(|| (0..i).rev().find_map(|k| hit(on(k), true)))
                .or_else(|| (i + 1..pages.len()).find_map(|k| hit(on(k), false)))
                .map(|t| (code.clone(), t))
        })
        .collect()
}
