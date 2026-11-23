//! Comments

pub mod rich;

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::docx::resolve::Resolver;
use crate::docx::review::{Comment, CommentId, RangeKind, RangeMark, Timestamp};
use crate::docx::write::props::{Cx, Props};
use crate::docx::{Block, Inline, Paragraph, Raw, Run};
use crate::font::Fonts;
use crate::layout::{ParaId, Unit};

pub use rich::{RichParagraph, RichRun, RichText};

use super::commands::Outcome;
use super::position::{Position, StoryId};
use super::selection::{Granularity, Selection};
use super::step::{Edges, Step, StepError, run_of};
use super::text::UnitText;
use super::view::CommentView;
use super::{Editor, Tx};

/// The most characters of the text a thread comments on that it shows.
const SCOPE_LIMIT: usize = crate::api::review::ANCHOR_TEXT_LIMIT;

#[derive(Clone, Debug, PartialEq)]
pub enum CommentCmd {
    Add(RichText),
    Reply { to: CommentId, body: RichText },
    Edit { id: CommentId, body: RichText },
    Delete(CommentId),
    Resolve { id: CommentId, done: bool },
    Activate(Option<CommentId>),
}

#[derive(Clone, Debug, Default)]
pub(super) struct State {
    chosen: Option<(CommentId, Selection)>,
    known: Option<Known>,
}

/// The comment marks, the paragraphs they are in and the thread ranges they make, as of an edit mark and a count of
/// comment edits.
#[derive(Clone, Debug, Default)]
struct Known {
    mark: super::EditMark,
    comments: u64,
    marks: HashMap<CommentId, Marks>,
    by_para: HashMap<ParaId, Vec<CommentId>>,
    ranges: Rc<Vec<ThreadRange>>,
}

#[derive(Clone, Copy, Debug)]
pub(in crate::editing) struct ThreadRange {
    pub root: CommentId,
    pub start: Position,
    pub end: Position,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ThreadInfo {
    pub root: CommentId,
    pub replies: Vec<CommentId>,
    pub scope: Option<String>,
    pub active: bool,
}

#[derive(Clone, Copy, Debug)]
struct Mark {
    para: ParaId,
    inline: usize,
    unit: Unit,
}

#[derive(Clone, Copy, Debug, Default)]
struct Marks {
    start: Option<Mark>,
    end: Option<Mark>,
    reference: Option<Mark>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Which {
    Start,
    End,
    Reference,
}

fn marker(i: &Inline) -> Option<(CommentId, Which)> {
    match i {
        Inline::Range(m) if m.kind == RangeKind::CommentStart => Some((CommentId(m.id), Which::Start)),
        Inline::Range(m) if m.kind == RangeKind::CommentEnd => Some((CommentId(m.id), Which::End)),
        Inline::CommentRef { id, .. } => Some((*id, Which::Reference)),
        _ => None,
    }
}

fn range_mark(kind: RangeKind, id: CommentId) -> Inline {
    Inline::Range(RangeMark { kind, id: id.0, name: None })
}

fn markers_at(p: &Paragraph, u: Unit) -> u32 {
    let mut at = 0u32;
    let mut n = 0;
    for i in &p.inlines {
        if at == u.0 && i.units() == 0 && !matches!(i, Inline::Text(..)) {
            n += 1;
        }
        at += i.units() as u32;
        if at > u.0 {
            break;
        }
    }
    n
}

fn run_before(p: &Paragraph, u: Unit) -> Run {
    let mut at = 0u32;
    let mut found = None;
    for i in &p.inlines {
        if at >= u.0 && found.is_some() {
            break;
        }
        if i.units() > 0
            && let Some(r) = run_of(i)
        {
            found = Some(r);
        }
        at += i.units() as u32;
    }
    let r = found.unwrap_or(&p.mark);
    if r.rev.is_empty() {
        return r.clone();
    }
    let mut s = (**r).clone();
    s.rev = Default::default();
    Rc::new(s)
}

fn without_markers(p: &Paragraph, gone: &HashSet<CommentId>) -> Vec<Inline> {
    let mut out: Vec<Inline> = Vec::with_capacity(p.inlines.len());
    for i in &p.inlines {
        if marker(i).is_some_and(|(id, _)| gone.contains(&id)) {
            continue;
        }
        if let (Inline::Text(t, r), Some(Inline::Text(prev, pr))) = (i, out.last_mut())
            && Rc::ptr_eq(r, pr)
        {
            prev.push_str(t);
            continue;
        }
        out.push(i.clone());
    }
    if out.is_empty() {
        out.push(Inline::Text(String::new(), p.mark.clone()));
    }
    out
}

fn initials(name: &str) -> String {
    name.split_whitespace().filter_map(|w| w.chars().next()).flat_map(char::to_uppercase).collect()
}

pub(super) fn deletion_cuts(p: &Paragraph, a: u32, b: u32) -> (Vec<u32>, Vec<CommentId>) {
    let mut at = 0u32;
    let mut marks: Vec<(u32, CommentId, Which)> = Vec::new();
    for i in &p.inlines {
        if let Some((id, kind)) = marker(i) {
            marks.push((at, id, kind));
        }
        at += i.units() as u32;
    }
    let pos = |id: CommentId, kind: Which| marks.iter().find(|m| m.1 == id && m.2 == kind).map(|m| m.0);
    let mut emptied: Vec<CommentId> = Vec::new();
    for &(_, id, _) in &marks {
        if let (Some(s), Some(e)) = (pos(id, Which::Start), pos(id, Which::End))
            && s < e
            && a <= s
            && e <= b
            && !emptied.contains(&id)
        {
            emptied.push(id);
        }
    }
    let cuts = marks.iter().filter(|m| a < m.0 && m.0 < b && !emptied.contains(&m.1)).map(|m| m.0).collect();
    (cuts, emptied)
}

/// The units before comment `id`'s `which` marker in `inlines`.
fn units_to(inlines: &[Inline], id: CommentId, which: Which) -> Option<u32> {
    let mut at = 0u32;
    for i in inlines {
        if marker(i) == Some((id, which)) {
            return Some(at);
        }
        at += i.units() as u32;
    }
    None
}

/// The paragraph in `blocks` holding comment `id`'s `which` marker, and the units before it.
fn in_blocks(blocks: &[Block], id: CommentId, which: Which) -> Option<(ParaId, u32)> {
    blocks.iter().find_map(|b| match b {
        Block::Para(p) => units_to(&p.inlines, id, which).map(|u| (ParaId(p.id), u)),
        Block::Table(t) => t.rows.iter().flat_map(|r| &r.cells).find_map(|c| in_blocks(&c.blocks, id, which)),
    })
}

fn ids_in(inlines: &[Inline], out: &mut Vec<CommentId>) {
    for i in inlines {
        match i {
            i if i.story().is_some() => {
                if let Some((_, tb)) = i.story() {
                    ids_in_blocks(&tb.blocks, out);
                }
            }
            i => out.extend(marker(i).map(|m| m.0)),
        }
    }
}

pub(in crate::editing) fn ids_in_blocks(blocks: &[Block], out: &mut Vec<CommentId>) {
    for b in blocks {
        match b {
            Block::Para(p) => ids_in(&p.inlines, out),
            Block::Table(t) => t.rows.iter().flat_map(|r| r.cells.iter()).for_each(|c| ids_in_blocks(&c.blocks, out)),
        }
    }
}

impl Editor {
    pub(super) fn comment_cmd(&mut self, c: CommentCmd, fonts: &Fonts) -> Outcome {
        match c {
            CommentCmd::Add(body) => {
                let author = self.author.0.clone();
                self.add_comment(&body, &author, fonts)
            }
            CommentCmd::Reply { to, body } => {
                let author = self.author.0.clone();
                self.reply_comment(to, &body, &author, fonts)
            }
            CommentCmd::Edit { id, body } => self.edit_comment(id, &body, fonts),
            CommentCmd::Delete(id) => self.delete_comment(id, fonts),
            CommentCmd::Resolve { id, done } => self.resolve_comment(id, done, fonts),
            CommentCmd::Activate(id) => {
                if id.is_some_and(|id| !self.doc.comments.by_id.contains_key(&id)) {
                    return self.comment_refused("no such comment");
                }
                self.comments.chosen = id.map(|id| (self.doc.comments.root(id), self.selection));
                Outcome::Moved
            }
        }
    }

    fn comment_refused(&mut self, why: &'static str) -> Outcome {
        self.refused = Some(why);
        Outcome::Unchanged(why)
    }

    /// Whether a comment can go on the selection.
    pub(in crate::editing) fn takes_comments_here(&self) -> bool {
        self.selection.ordered(&self.index).is_some_and(|(a, _)| self.takes_comments(a.story))
    }

    fn takes_comments(&self, story: StoryId) -> bool {
        match story {
            StoryId::Body => true,
            StoryId::TextBox(f) => {
                self.index.float_anchor(f).and_then(|p| self.index.para_slot(p)).is_some_and(|s| self.takes_comments(self.index.story(s.container)))
            }
            StoryId::HeaderFooter(_) | StoryId::Footnote(_) | StoryId::Endnote(_) | StoryId::BuildingBlock(_) => false,
        }
    }

    fn word_around(&self, para: ParaId, u: Unit) -> (Unit, Unit) {
        let Some(p) = self.index.para(&self.doc, para) else { return (u, u) };
        let t = UnitText::of(p);
        let chars: Vec<char> = t.text.chars().collect();
        let trimmed = |(s, e): (Unit, Unit)| {
            let mut e = e.0;
            while e > s.0 && chars.get(e as usize - 1).is_some_and(|c| c.is_whitespace()) {
                e -= 1;
            }
            (s, Unit(e))
        };
        let here = trimmed(t.word_at(u));
        if here.0 < here.1 && here.0 <= u {
            return here;
        }
        if u.0 > 0 {
            let before = trimmed(t.word_at(Unit(u.0 - 1)));
            if before.0 < before.1 && before.1 >= u {
                return before;
            }
        }
        (u, u)
    }

    fn comment_marks(&self) -> HashMap<CommentId, Marks> {
        let mut out: HashMap<CommentId, Marks> = HashMap::new();
        if self.doc.comments.by_id.is_empty() {
            return out;
        }
        for para in self.index.para_ids() {
            self.para_marks(para, &mut out);
        }
        out
    }

    /// Adds the marks in `para` to `out`; whether it has any.
    fn para_marks(&self, para: ParaId, out: &mut HashMap<CommentId, Marks>) -> bool {
        let Some(p) = self.index.para(&self.doc, para) else { return false };
        let mut at = 0u32;
        let mut any = false;
        for (k, i) in p.inlines.iter().enumerate() {
            if let Some((id, kind)) = marker(i) {
                let m = Some(Mark { para, inline: k, unit: Unit(at) });
                let e = out.entry(id).or_default();
                match kind {
                    Which::Start => e.start = m,
                    Which::End => e.end = m,
                    Which::Reference => e.reference = m,
                }
                any = true;
            }
            at += i.units() as u32;
        }
        any
    }

    /// The comment marks known before, with those in the paragraphs touched since read again; `None` when the touched
    /// paragraphs are no longer known. Whether any mark moved, came or went.
    fn update_marks(&self, known: &mut Known) -> Option<bool> {
        let touched = self.touched.since(known.mark)?;
        let mut seen = std::collections::HashSet::new();
        let mut moved = false;
        for &p in touched.iter().filter(|p| seen.insert(**p)) {
            for id in known.by_para.remove(&p).unwrap_or_default() {
                let Some(m) = known.marks.get_mut(&id) else { continue };
                for slot in [&mut m.start, &mut m.end, &mut m.reference] {
                    if slot.is_some_and(|x| x.para == p) {
                        *slot = None;
                    }
                }
                if m.start.is_none() && m.end.is_none() && m.reference.is_none() {
                    known.marks.remove(&id);
                }
                moved = true;
            }
            if self.para_marks(p, &mut known.marks) {
                let ids = self.index.para(&self.doc, p).into_iter().flat_map(|q| q.inlines.iter().filter_map(marker)).map(|(id, _)| id);
                known.by_para.insert(p, ids.collect());
                moved = true;
            }
        }
        Some(moved)
    }

    fn mark_position(&self, m: Mark) -> Option<Position> {
        let slot = self.index.para_slot(m.para)?;
        Some(Position::text(self.index.story(slot.container), m.para, m.unit))
    }

    fn story_order(&self, a: &Position, b: &Position) -> Ordering {
        let rank = |s: StoryId| match s {
            StoryId::Body => (0, 0),
            StoryId::HeaderFooter(k) => (1, i64::from(k)),
            StoryId::Footnote(id) => (2, id),
            StoryId::Endnote(id) => (3, id),
            StoryId::TextBox(f) => (4, f.0 as i64),
            StoryId::BuildingBlock(k) => (5, i64::from(k)),
        };
        if a.story == b.story { self.index.cmp(a, b) } else { rank(a.story).cmp(&rank(b.story)) }
    }

    /// The threads' ranges in story order: kept while edits touch no paragraph with comment marks and no comment.
    pub(in crate::editing) fn thread_ranges(&mut self) -> Rc<Vec<ThreadRange>> {
        if self.doc.comments.by_id.is_empty() {
            self.comments.known = None;
            return Rc::default();
        }
        let mut known = self.comments.known.take().unwrap_or_default();
        let moved = self.update_marks(&mut known).unwrap_or_else(|| {
            known.marks = self.comment_marks();
            known.by_para.clear();
            for (&id, m) in &known.marks {
                for at in [m.start, m.end, m.reference].into_iter().flatten() {
                    known.by_para.entry(at.para).or_default().push(id);
                }
            }
            true
        });
        if moved || known.comments != self.comment_edits {
            known.ranges = crate::prof::time("comments:ranges", || {
                let mut v: Vec<ThreadRange> = self
                    .doc
                    .comments
                    .threads()
                    .into_iter()
                    .filter_map(|(root, _)| {
                        let m = known.marks.get(&root)?;
                        Some(ThreadRange { root, start: self.mark_position(m.start?)?, end: self.mark_position(m.end?)? })
                    })
                    .collect();
                v.sort_by(|a, b| self.story_order(&a.start, &b.start));
                Rc::new(v)
            });
        }
        (known.mark, known.comments) = (self.touched.mark(), self.comment_edits);
        let ranges = known.ranges.clone();
        self.comments.known = Some(known);
        ranges
    }

    pub(in crate::editing) fn comment_range(&mut self, id: CommentId) -> Option<(Position, Position)> {
        let root = self.doc.comments.root(id);
        self.thread_ranges().iter().find(|r| r.root == root).map(|r| (r.start, r.end))
    }

    pub fn active_comment(&mut self) -> Option<CommentId> {
        if let Some((id, at)) = self.comments.chosen
            && at == self.selection
            && self.doc.comments.by_id.contains_key(&id)
        {
            return Some(id);
        }
        let focus = self.selection.focus()?;
        let ranges = self.thread_ranges();
        ranges
            .iter()
            .filter(|r| {
                r.start.story == focus.story
                    && self.index.cmp(&r.start, &focus) != Ordering::Greater
                    && self.index.cmp(&focus, &r.end) != Ordering::Greater
            })
            .max_by(|x, y| self.index.cmp(&x.start, &y.start))
            .map(|r| r.root)
    }

    pub fn comment_threads(&mut self) -> Vec<ThreadInfo> {
        let active = self.active_comment();
        let ranges = self.thread_ranges();
        let mut threads = self.doc.comments.threads();
        let mut at: HashMap<CommentId, usize> = HashMap::with_capacity(ranges.len());
        for (k, r) in ranges.iter().enumerate() {
            at.entry(r.root).or_insert(k);
        }
        threads.sort_by_key(|(root, _)| at.get(root).copied().unwrap_or(usize::MAX));
        let mut texts = HashMap::new();
        threads
            .into_iter()
            .map(|(root, replies)| {
                let scope = at.get(&root).map(|&k| &ranges[k]).map(|r| {
                    let kept = std::mem::replace(&mut self.selection, Selection::range(r.start, r.end, Granularity::Char));
                    let (text, _) = self.text_within(SCOPE_LIMIT, &mut texts);
                    self.selection = kept;
                    text
                });
                ThreadInfo { root, replies, scope, active: active == Some(root) }
            })
            .collect()
    }

    pub(super) fn comment_views(&mut self, visible: std::ops::Range<usize>) -> (Vec<CommentView>, Vec<(CommentId, usize, [f64; 2])>) {
        if self.doc.comments.by_id.is_empty() {
            return (Vec::new(), Vec::new());
        }
        let active = self.active_comment();
        let ranges = self.thread_ranges();
        let (mut views, mut anchors) = (Vec::new(), Vec::new());
        let near = self.near;
        for r in ranges.iter() {
            if let Some(c) = self.caret_at(r.start).filter(|c| visible.contains(&c.page)) {
                anchors.push((r.root, c.page, [c.x, c.top]));
            }
            let rects = self.range_rects(r.start, r.end, visible.clone());
            if !rects.is_empty() {
                views.push(CommentView { thread: r.root, rects, active: active == Some(r.root) });
            }
        }
        self.near = near;
        (views, anchors)
    }

    fn insert_before(&self, para: ParaId, k: usize, with: Vec<Inline>) -> Option<Step> {
        let p = self.index.para(&self.doc, para)?;
        let at: u32 = p.inlines.get(..k)?.iter().map(|i| i.units() as u32).sum();
        let under = p.inlines[k..].iter().take_while(|i| i.units() == 0).filter(|i| !matches!(i, Inline::Text(..))).count() as u32;
        Some(Step::Text { para, range: Unit(at)..Unit(at), with, edges: Edges { under, ..Edges::default() } })
    }

    fn new_comment(&mut self, id: CommentId, parent: Option<CommentId>, body: &RichText, author: &str, res: &Resolver, cx: &Cx) -> Comment {
        let who = self.doc.people.author(author, None);
        let initials = self.doc.people.get(who).and_then(|p| p.initials.clone()).unwrap_or_else(|| initials(author).into());
        let secs = crate::clock::unix_secs();
        let now = Timestamp::from_utc(secs);
        Comment {
            id,
            author: who,
            initials: Some(initials),
            date: Some(now.clone()),
            date_utc: Some(now),
            body: body.to_blocks(res, cx),
            para_id: self.doc.review_ids.next_para_id(),
            durable_id: Some(self.doc.review_ids.next_para_id()),
            parent,
            done: false,
            extensible: Raw::default(),
        }
    }

    fn reference_run(&self, para: ParaId, at: Unit, res: &Resolver, cx: &Cx) -> Option<Run> {
        if res.styles.by_id.contains_key("CommentReference") {
            let base = self.run_base(para, at, res, cx)?;
            let mut p = Props::parse(None, "rPr", cx);
            let w = &cx.w;
            p.set("rStyle", format!("<{w}rStyle {w}val=\"CommentReference\"/>"));
            return Some(res.run_from_xml(&base, p.xml().as_deref(), cx));
        }
        Some(run_before(self.index.para(&self.doc, para)?, at))
    }

    pub fn add_comment(&mut self, body: &RichText, author: &str, fonts: &Fonts) -> Outcome {
        let Some((a, b)) = self.selection.ordered(&self.index) else { return Outcome::Unchanged("no selection") };
        if !self.takes_comments(a.story) {
            return self.comment_refused("unavailable");
        }
        let (Some((pa, oa)), Some((pb, ob))) = (a.para_offset(), b.para_offset()) else { return self.comment_refused("no text range") };
        let (oa, ob) = if a.same_place(&b) { self.word_around(pa, oa) } else { (oa, ob) };
        let Some(res) = self.doc.resolver.clone() else { return Outcome::Unchanged("no styles") };
        let cx = self.cx();
        let Some(run) = self.reference_run(pb, ob, &res, &cx) else { return Outcome::Unchanged("no paragraph") };
        let id = CommentId(self.doc.review_ids.next_revision_id());
        let (start, end, reference) =
            (range_mark(RangeKind::CommentStart, id), range_mark(RangeKind::CommentEnd, id), Inline::CommentRef { id, run });
        let steps = if pa == pb && oa == ob {
            vec![Step::Text { para: pa, range: oa..oa, with: vec![start, end, reference], edges: Edges::default() }]
        } else {
            let Some(under) = self.index.para(&self.doc, pb).map(|p| markers_at(p, ob)) else { return Outcome::Unchanged("no paragraph") };
            vec![
                Step::Text { para: pb, range: ob..ob, with: vec![end, reference], edges: Edges { under, ..Edges::default() } },
                Step::Text { para: pa, range: oa..oa, with: vec![start], edges: Edges::default() },
            ]
        };
        let mut tx = self.begin("New Comment");
        for s in steps {
            if self.step(&mut tx, s).is_err() {
                self.abort(tx);
                return Outcome::Unchanged("stale selection");
            }
        }
        let comment = self.new_comment(id, None, body, author, &res, &cx);
        if self.step(&mut tx, Step::Comment { id, after: Some(comment) }).is_err() {
            self.abort(tx);
            return Outcome::Unchanged("comment failed");
        }
        let out = self.commit(tx, None, fonts);
        if matches!(out, Outcome::Changed(_)) {
            self.comments.chosen = Some((id, self.selection));
            self.made = Some(crate::api::command::CommandResult::Comment { id: crate::api::record::comment_id(id) });
        }
        out
    }

    pub fn reply_comment(&mut self, to: CommentId, body: &RichText, author: &str, fonts: &Fonts) -> Outcome {
        if !self.doc.comments.by_id.contains_key(&to) {
            return self.comment_refused("no such comment");
        }
        let root = self.doc.comments.root(to);
        let Some(res) = self.doc.resolver.clone() else { return Outcome::Unchanged("no styles") };
        let cx = self.cx();
        let id = CommentId(self.doc.review_ids.next_revision_id());
        let mut reply = self.new_comment(id, Some(root), body, author, &res, &cx);
        reply.done = self.doc.comments.by_id.get(&root).is_some_and(|c| c.done);
        let mut tx = self.begin("Reply");
        if self
            .reply_markers(&mut tx, root, id, &res, &cx)
            .and_then(|()| self.step(&mut tx, Step::Comment { id, after: Some(reply) }).map(|_| ()))
            .is_err()
        {
            self.abort(tx);
            return Outcome::Unchanged("stale comment");
        }
        let out = self.commit(tx, None, fonts);
        if matches!(out, Outcome::Changed(_)) {
            self.made = Some(crate::api::command::CommandResult::Comment { id: crate::api::record::comment_id(id) });
        }
        out
    }

    fn reply_markers(&mut self, tx: &mut Tx, root: CommentId, id: CommentId, res: &Resolver, cx: &Cx) -> Result<(), StepError> {
        let m = self.comment_marks().get(&root).copied().unwrap_or_default();
        let ranged = m.start.is_some() && m.end.is_some();
        if let Some(after) = m.reference.or(m.end) {
            let run = m
                .reference
                .and_then(|r| match self.index.para(&self.doc, r.para)?.inlines.get(r.inline)? {
                    Inline::CommentRef { run, .. } => Some(run.clone()),
                    _ => None,
                })
                .or_else(|| self.reference_run(after.para, after.unit, res, cx))
                .ok_or(StepError::Missing)?;
            let mut with = if ranged { vec![range_mark(RangeKind::CommentEnd, id)] } else { Vec::new() };
            with.push(Inline::CommentRef { id, run });
            let s = self.insert_before(after.para, after.inline + 1, with).ok_or(StepError::Missing)?;
            self.step(tx, s)?;
        }
        if ranged && let Some(start) = self.comment_marks().get(&root).and_then(|m| m.start) {
            let s = self.insert_before(start.para, start.inline + 1, vec![range_mark(RangeKind::CommentStart, id)]).ok_or(StepError::Missing)?;
            self.step(tx, s)?;
        }
        Ok(())
    }

    pub fn edit_comment(&mut self, id: CommentId, body: &RichText, fonts: &Fonts) -> Outcome {
        let Some(res) = self.doc.resolver.clone() else { return Outcome::Unchanged("no styles") };
        let cx = self.cx();
        let Some(mut c) = self.doc.comments.by_id.get(&id).cloned() else { return self.comment_refused("no such comment") };
        c.body = body.to_blocks(&res, &cx);
        let mut tx = self.begin("Edit Comment");
        if self.step(&mut tx, Step::Comment { id, after: Some(c) }).is_err() {
            self.abort(tx);
            return Outcome::Unchanged("comment failed");
        }
        self.commit(tx, None, fonts)
    }

    pub fn delete_comment(&mut self, id: CommentId, fonts: &Fonts) -> Outcome {
        if !self.doc.comments.by_id.contains_key(&id) {
            return self.comment_refused("no such comment");
        }
        let mut tx = self.begin("Delete Comment");
        if self.remove_comments(&mut tx, &[id]).is_err() {
            self.abort(tx);
            return Outcome::Unchanged("stale comment");
        }
        self.commit(tx, None, fonts)
    }

    pub fn resolve_comment(&mut self, id: CommentId, done: bool, fonts: &Fonts) -> Outcome {
        if !self.doc.comments.by_id.contains_key(&id) {
            return self.comment_refused("no such comment");
        }
        let roots = self.doc.comments.roots();
        let root = roots[&id];
        let thread: Vec<Comment> =
            self.doc.comments.by_id.values().filter(|c| c.done != done && roots[&c.id] == root).map(|c| Comment { done, ..c.clone() }).collect();
        let mut tx = self.begin(if done { "Resolve Comment" } else { "Reopen Comment" });
        for c in thread {
            if self.step(&mut tx, Step::Comment { id: c.id, after: Some(c) }).is_err() {
                self.abort(tx);
                return Outcome::Unchanged("comment failed");
            }
        }
        self.commit(tx, None, fonts)
    }

    pub(super) fn remove_comments(&mut self, tx: &mut Tx, ids: &[CommentId]) -> Result<(), StepError> {
        let mut replies: HashMap<CommentId, Vec<CommentId>> = HashMap::new();
        for c in self.doc.comments.by_id.values() {
            if let Some(p) = c.parent {
                replies.entry(p).or_default().push(c.id);
            }
        }
        let mut gone: Vec<CommentId> = ids.iter().copied().filter(|id| self.doc.comments.by_id.contains_key(id)).collect();
        let mut seen: HashSet<CommentId> = gone.iter().copied().collect();
        let mut k = 0;
        while let Some(&id) = gone.get(k) {
            k += 1;
            for &r in replies.get(&id).into_iter().flatten() {
                if seen.insert(r) {
                    gone.push(r);
                }
            }
        }
        let marks = self.comment_marks();
        let mut paras: Vec<ParaId> =
            gone.iter().filter_map(|id| marks.get(id)).flat_map(|m| [m.start, m.end, m.reference]).flatten().map(|m| m.para).collect();
        paras.sort();
        paras.dedup();
        for para in paras {
            let p = self.index.para(&self.doc, para).ok_or(StepError::Missing)?;
            let inlines = without_markers(p, &seen);
            self.step(tx, Step::Restyle { para, inlines })?;
        }
        for id in gone {
            self.step(tx, Step::Comment { id, after: None })?;
        }
        if self.comments.chosen.is_some_and(|(c, _)| !self.doc.comments.by_id.contains_key(&c)) {
            self.comments.chosen = None;
        }
        Ok(())
    }

    pub(super) fn drop_orphan_comments(&mut self, tx: &mut Tx) {
        if self.doc.comments.by_id.is_empty() {
            return;
        }
        let mut hit = Vec::new();
        for s in &tx.undo {
            match s {
                Step::Text { with, .. } | Step::Restyle { inlines: with, .. } => ids_in(with, &mut hit),
                Step::Blocks { with, .. } => ids_in_blocks(with, &mut hit),
                Step::Rows { with, .. } => with.iter().flat_map(|r| r.cells.iter()).for_each(|c| ids_in_blocks(&c.blocks, &mut hit)),
                Step::Note { blocks: Some(b), .. } | Step::HeaderPart { blocks: Some(b), .. } => ids_in_blocks(b, &mut hit),
                _ => {}
            }
        }
        hit.sort();
        hit.dedup();
        hit.retain(|id| self.doc.comments.by_id.contains_key(id));
        if hit.is_empty() {
            return;
        }
        let mut marks = self.comment_marks();
        let mut gone = Vec::new();
        for id in hit {
            for _ in 0..3 {
                let m = marks.get(&id).copied().unwrap_or_default();
                let run = |e: &Editor, m: Mark| e.index.para(&e.doc, m.para).map(|p| run_before(p, m.unit));
                let repair = match (m.start, m.end, m.reference) {
                    (None, None, None) => {
                        gone.push(id);
                        None
                    }
                    (Some(_), Some(_), Some(_)) | (None, None, Some(_)) => None,
                    (None, Some(e), _) => self
                        .lost_marker(tx, id, Which::Start, e)
                        .or_else(|| self.insert_before(e.para, e.inline, vec![range_mark(RangeKind::CommentStart, id)])),
                    (Some(s), None, _) => self
                        .lost_marker(tx, id, Which::End, s)
                        .or_else(|| self.insert_before(s.para, s.inline + 1, vec![range_mark(RangeKind::CommentEnd, id)])),
                    (Some(_), Some(e), None) => {
                        run(self, e).and_then(|run| self.insert_before(e.para, e.inline + 1, vec![Inline::CommentRef { id, run }]))
                    }
                };
                let Some(step) = repair else { break };
                let Step::Text { para, .. } = step else { break };
                if self.step(tx, step).is_err() {
                    break;
                }
                self.rescan_comment_marks(para, &mut marks);
            }
        }
        if !gone.is_empty() {
            let _ = self.remove_comments(tx, &gone);
        }
    }

    /// The step putting back comment `id`'s `which` marker where the transaction removed it, mapped through the
    /// steps after; `None` when that place is gone or would put the marker on the wrong side of `other`.
    fn lost_marker(&self, tx: &Tx, id: CommentId, which: Which, other: Mark) -> Option<Step> {
        let (k, para, u) = tx.undo.iter().enumerate().rev().find_map(|(k, s)| {
            let (para, u) = match s {
                Step::Text { para, range, with, .. } => (*para, range.start.0 + units_to(with, id, which)?),
                Step::Restyle { para, inlines } => (*para, units_to(inlines, id, which)?),
                Step::Blocks { with, .. } => in_blocks(with, id, which)?,
                Step::Rows { with, .. } => with.iter().flat_map(|r| &r.cells).find_map(|c| in_blocks(&c.blocks, id, which))?,
                _ => return None,
            };
            Some((k, para, u))
        })?;
        let story = self.mark_position(other)?.story;
        let at = tx.maps[k..].iter().fold(Position::text(story, para, Unit(u)), |p, m| m.apply(p));
        let (para, unit) = at.para_offset()?;
        let p = self.index.para(&self.doc, para)?;
        let survivor = self.mark_position(other)?;
        let order = self.index.cmp(&at, &survivor);
        let edges = match which {
            Which::Start if order != Ordering::Greater => Edges { under: markers_at(p, unit), ..Edges::default() },
            Which::End if order != Ordering::Less => Edges::default(),
            _ => return None,
        };
        let kind = if which == Which::Start { RangeKind::CommentStart } else { RangeKind::CommentEnd };
        (unit <= super::step::para_len(p)).then(|| Step::Text { para, range: unit..unit, with: vec![range_mark(kind, id)], edges })
    }

    /// Paragraph `para`'s comment markers, put in `marks` again after a step changed it.
    fn rescan_comment_marks(&self, para: ParaId, marks: &mut HashMap<CommentId, Marks>) {
        let Some(p) = self.index.para(&self.doc, para) else { return };
        let mut at = 0u32;
        for (k, i) in p.inlines.iter().enumerate() {
            if let Some((id, kind)) = marker(i) {
                let m = Some(Mark { para, inline: k, unit: Unit(at) });
                let e = marks.entry(id).or_default();
                match kind {
                    Which::Start => e.start = m,
                    Which::End => e.end = m,
                    Which::Reference => e.reference = m,
                }
            }
            at += i.units() as u32;
        }
    }

    /// Removes the comments whose whole range lies in `a..b` across paragraphs: deleting it leaves them no text.
    pub(in crate::editing) fn remove_comments_within(&mut self, tx: &mut Tx, a: Position, b: Position) -> Result<(), StepError> {
        if self.doc.comments.by_id.is_empty() || a.para_offset().map(|x| x.0) == b.para_offset().map(|x| x.0) {
            return Ok(());
        }
        let marks = self.comment_marks();
        let mut emptied: Vec<CommentId> = marks
            .iter()
            .filter_map(|(id, m)| {
                let (s, e) = (self.mark_position(m.start?)?, self.mark_position(m.end?)?);
                let inside = s.story == a.story
                    && e.story == a.story
                    && self.index.cmp(&a, &s) != Ordering::Greater
                    && self.index.cmp(&e, &b) != Ordering::Greater
                    && self.index.cmp(&s, &e) == Ordering::Less;
                inside.then_some(*id)
            })
            .collect();
        if emptied.is_empty() {
            return Ok(());
        }
        emptied.sort();
        self.remove_comments(tx, &emptied)
    }
}
