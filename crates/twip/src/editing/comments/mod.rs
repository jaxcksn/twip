//! Comments in the editor: adding, replying, editing, resolving and deleting them, and their ranges.
//!
//! A comment's range is two zero-unit markers (`Inline::Range` start and end) and a reference mark
//! (`Inline::CommentRef`) after the end; replies have their own markers beside their thread's. The markers move with
//! the text around them through `Text` steps; the comment itself (author, date, body, thread, done flag) changes
//! through [`Step::Comment`]. Every command here is one transaction, and none is recorded as a revision.

pub mod rich;

use std::cmp::Ordering;
use std::collections::HashMap;
use std::rc::Rc;

use serde_json::json;

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

/// A comment command (`review.comment.*`).
#[derive(Clone, Debug, PartialEq)]
pub enum CommentCmd {
    /// A new thread on the selection, or on the word at the caret.
    Add(RichText),
    /// A reply at the end of the thread holding `to`.
    Reply {
        to: CommentId,
        body: RichText,
    },
    Edit {
        id: CommentId,
        body: RichText,
    },
    /// A reply, or a whole thread with its range when `id` is its first comment.
    Delete(CommentId),
    /// The thread holding `id` resolved (`done`) or reopened.
    Resolve {
        id: CommentId,
        done: bool,
    },
    /// The host's choice of active thread; `None` gives it back to the selection.
    Activate(Option<CommentId>),
}

/// The editor's comment state: the host's choice of active thread, and the threads' ranges as found at a revision.
#[derive(Clone, Debug, Default)]
pub(super) struct State {
    /// The thread the host made active, and the selection then: moving the selection gives the choice up.
    chosen: Option<(CommentId, Selection)>,
    ranges: Option<(u64, Rc<Vec<ThreadRange>>)>,
}

/// A thread's range: its first comment's start and end.
#[derive(Clone, Copy, Debug)]
struct ThreadRange {
    root: CommentId,
    start: Position,
    end: Position,
}

/// A thread as the host lists it.
#[derive(Clone, Debug, PartialEq)]
pub struct ThreadInfo {
    pub root: CommentId,
    /// The replies, in order.
    pub replies: Vec<CommentId>,
    /// The text of the thread's range (paragraphs separated by newlines); `None` when it has no range.
    pub scope: Option<String>,
    pub active: bool,
}

/// One marker of a comment: its paragraph, index among the paragraph's inlines, and unit offset.
#[derive(Clone, Copy, Debug)]
struct Mark {
    para: ParaId,
    inline: usize,
    unit: Unit,
}

/// Where a comment's range start, range end and reference mark are.
#[derive(Clone, Copy, Debug, Default)]
struct Marks {
    start: Option<Mark>,
    end: Option<Mark>,
    reference: Option<Mark>,
}

/// Which of a comment's markers an inline is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Which {
    Start,
    End,
    Reference,
}

/// The comment a marker belongs to, and which marker it is.
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

/// How many markers (zero-unit inlines other than text) sit at unit `u` of `p`.
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

/// The formatting of the text ending at `u` (the first text's at the start), without its revisions, else the
/// paragraph mark's.
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

/// `p`'s inlines without the markers of `gone`, texts the removal brings together in one run joined.
fn without_markers(p: &Paragraph, gone: &[CommentId]) -> Vec<Inline> {
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

/// "Author A" → "AA".
fn initials(name: &str) -> String {
    name.split_whitespace().filter_map(|w| w.chars().next()).flat_map(char::to_uppercase).collect()
}

/// How deleting units `a..b` of `p` treats the comments there: a range whose text it takes whole goes with it
/// (returned second), and the markers of the others stay, the deletion cut at each one strictly inside it (returned
/// first).
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

/// The comments whose markers `inlines` holds (text boxes' content included).
fn ids_in(inlines: &[Inline], out: &mut Vec<CommentId>) {
    for i in inlines {
        match i {
            Inline::Float(f, _) => {
                if let Some(tb) = &f.text_box {
                    ids_in_blocks(&tb.blocks, out);
                }
            }
            i => out.extend(marker(i).map(|m| m.0)),
        }
    }
}

fn ids_in_blocks(blocks: &[Block], out: &mut Vec<CommentId>) {
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
        self.answer = Some(json!({ "error": why }));
        Outcome::Unchanged(why)
    }

    /// Whether new comments can go in a story: the body and text boxes there, not headers, footers or notes.
    fn takes_comments(&self, story: StoryId) -> bool {
        match story {
            StoryId::Body => true,
            StoryId::TextBox(f) => {
                self.index.float_anchor(f).and_then(|p| self.index.para_slot(p)).is_some_and(|s| self.takes_comments(self.index.story(s.container)))
            }
            StoryId::HeaderFooter(_) | StoryId::Footnote(_) | StoryId::Endnote(_) => false,
        }
    }

    /// The word at a caret, without the spaces after it: the word the caret is in or starts, else the one it ends.
    /// A caret between spaces stays a point.
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

    /// Every comment's markers, found in every story.
    fn comment_marks(&self) -> HashMap<CommentId, Marks> {
        let mut out: HashMap<CommentId, Marks> = HashMap::new();
        if self.doc.comments.by_id.is_empty() {
            return out;
        }
        for para in self.index.para_ids() {
            let Some(p) = self.index.para(&self.doc, para) else { continue };
            let mut at = 0u32;
            for (k, i) in p.inlines.iter().enumerate() {
                if let Some((id, kind)) = marker(i) {
                    let m = Some(Mark { para, inline: k, unit: Unit(at) });
                    let e = out.entry(id).or_default();
                    match kind {
                        Which::Start => e.start = m,
                        Which::End => e.end = m,
                        Which::Reference => e.reference = m,
                    }
                }
                at += i.units() as u32;
            }
        }
        out
    }

    fn mark_position(&self, m: Mark) -> Option<Position> {
        let slot = self.index.para_slot(m.para)?;
        Some(Position::text(self.index.story(slot.container), m.para, m.unit))
    }

    /// Document order across stories: the body, headers and footers, notes, then text boxes.
    fn story_order(&self, a: &Position, b: &Position) -> Ordering {
        let rank = |s: StoryId| match s {
            StoryId::Body => (0, 0),
            StoryId::HeaderFooter(k) => (1, i64::from(k)),
            StoryId::Footnote(id) => (2, id),
            StoryId::Endnote(id) => (3, id),
            StoryId::TextBox(f) => (4, f.0 as i64),
        };
        if a.story == b.story { self.index.cmp(a, b) } else { rank(a.story).cmp(&rank(b.story)) }
    }

    /// Each thread with a range, by its first comment's markers, in document order (found again after each change).
    fn thread_ranges(&mut self) -> Rc<Vec<ThreadRange>> {
        if let Some((rev, r)) = &self.comments.ranges
            && *rev == self.revision
        {
            return r.clone();
        }
        let marks = self.comment_marks();
        let mut v: Vec<ThreadRange> = self
            .doc
            .comments
            .threads()
            .into_iter()
            .filter_map(|(root, _)| {
                let m = marks.get(&root)?;
                Some(ThreadRange { root, start: self.mark_position(m.start?)?, end: self.mark_position(m.end?)? })
            })
            .collect();
        v.sort_by(|a, b| self.story_order(&a.start, &b.start));
        let v = Rc::new(v);
        self.comments.ranges = Some((self.revision, v.clone()));
        v
    }

    /// The active thread: the one the host chose while the selection stays where it was then, else the innermost
    /// thread whose range holds the selection's focus.
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

    /// The threads in the order of their ranges (those without one last), with their range's text.
    pub fn comment_threads(&mut self) -> Vec<ThreadInfo> {
        let active = self.active_comment();
        let ranges = self.thread_ranges();
        let mut threads = self.doc.comments.threads();
        let at = |id: CommentId| ranges.iter().position(|r| r.root == id).unwrap_or(usize::MAX);
        threads.sort_by_key(|(root, _)| at(*root));
        threads
            .into_iter()
            .map(|(root, replies)| {
                let scope = ranges.iter().find(|r| r.root == root).map(|r| {
                    let kept = std::mem::replace(&mut self.selection, Selection::range(r.start, r.end, Granularity::Char));
                    let text = self.selected_text();
                    self.selection = kept;
                    text
                });
                ThreadInfo { root, replies, scope, active: active == Some(root) }
            })
            .collect()
    }

    /// Each thread's range on the visible pages, and where each visible range starts (page coordinates).
    pub(super) fn comment_views(&mut self, visible: std::ops::Range<usize>) -> (Vec<CommentView>, Vec<(CommentId, usize, [f64; 2])>) {
        if self.doc.comments.by_id.is_empty() {
            return (Vec::new(), Vec::new());
        }
        let active = self.active_comment();
        let ranges = self.thread_ranges();
        let (mut views, mut anchors) = (Vec::new(), Vec::new());
        // Finding a range's start moves the page the caret was last shown on; it stays the caret's.
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

    /// The `Text` step that puts markers right before inline `k` of `para`.
    fn insert_before(&self, para: ParaId, k: usize, with: Vec<Inline>) -> Option<Step> {
        let p = self.index.para(&self.doc, para)?;
        let at: u32 = p.inlines.get(..k)?.iter().map(|i| i.units() as u32).sum();
        // The insertion goes before the markers from `k` to the next unit.
        let under = p.inlines[k..].iter().take_while(|i| i.units() == 0).filter(|i| !matches!(i, Inline::Text(..))).count() as u32;
        Some(Step::Text { para, range: Unit(at)..Unit(at), with, edges: Edges { under, ..Edges::default() } })
    }

    /// A new comment by `author`, now, with a body in the comment styles.
    fn new_comment(&mut self, id: CommentId, parent: Option<CommentId>, body: &RichText, author: &str, res: &Resolver, cx: &Cx) -> Comment {
        let who = self.doc.people.author(author, None);
        let initials = self.doc.people.get(who).and_then(|p| p.initials.clone()).unwrap_or_else(|| initials(author).into());
        let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs()) as i64;
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

    /// The reference mark's formatting at the end of a new range: the CommentReference style when the document has
    /// it, else the text's before it.
    fn reference_run(&self, para: ParaId, at: Unit, res: &Resolver, cx: &Cx) -> Option<Run> {
        if res.styles.by_id.contains_key("CommentReference") {
            let base = self.run_base(para, res, cx)?;
            let mut p = Props::parse(None, "rPr", cx);
            let w = &cx.w;
            p.set("rStyle", format!("<{w}rStyle {w}val=\"CommentReference\"/>"));
            return Some(res.run_from_xml(&base, p.xml().as_deref(), cx));
        }
        Some(run_before(self.index.para(&self.doc, para)?, at))
    }

    /// Adds a thread by `author` on the selection, or on the word at the caret. Headers, footers and notes take no
    /// new comments (`unavailable`).
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
            // The end first: right after the range's text, before markers there; then the start, right before its text.
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
            self.answer = Some(json!({ "id": id.0 }));
        }
        out
    }

    /// Adds a reply by `author` at the end of the thread holding `to`, its markers beside the thread's.
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
            self.answer = Some(json!({ "id": id.0 }));
        }
        out
    }

    /// A reply's markers beside its thread's: its end and reference after the thread's reference mark (else after
    /// its range's end), and its start after the thread's start.
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
        // Found again: the end may have gone into the start's paragraph.
        if ranged && let Some(start) = self.comment_marks().get(&root).and_then(|m| m.start) {
            let s = self.insert_before(start.para, start.inline + 1, vec![range_mark(RangeKind::CommentStart, id)]).ok_or(StepError::Missing)?;
            self.step(tx, s)?;
        }
        Ok(())
    }

    /// Replaces a comment's body.
    pub fn edit_comment(&mut self, id: CommentId, body: &RichText, fonts: &Fonts) -> Outcome {
        let Some(res) = self.doc.resolver.clone() else { return Outcome::Unchanged("no styles") };
        let cx = self.cx();
        // The comment as it is, with a new body: an independent copy for the step.
        let Some(mut c) = self.doc.comments.by_id.get(&id).cloned() else { return self.comment_refused("no such comment") };
        c.body = body.to_blocks(&res, &cx);
        let mut tx = self.begin("Edit Comment");
        if self.step(&mut tx, Step::Comment { id, after: Some(c) }).is_err() {
            self.abort(tx);
            return Outcome::Unchanged("comment failed");
        }
        self.commit(tx, None, fonts)
    }

    /// Deletes a reply, or a thread's first comment with its replies and range.
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

    /// Resolves (`done`) or reopens the thread holding `id`: every comment in it.
    pub fn resolve_comment(&mut self, id: CommentId, done: bool, fonts: &Fonts) -> Outcome {
        if !self.doc.comments.by_id.contains_key(&id) {
            return self.comment_refused("no such comment");
        }
        let root = self.doc.comments.root(id);
        let thread: Vec<Comment> = self
            .doc
            .comments
            .by_id
            .values()
            .filter(|c| c.done != done && self.doc.comments.root(c.id) == root)
            .map(|c| Comment { done, ..c.clone() })
            .collect();
        let mut tx = self.begin(if done { "Resolve Comment" } else { "Reopen Comment" });
        for c in thread {
            if self.step(&mut tx, Step::Comment { id: c.id, after: Some(c) }).is_err() {
                self.abort(tx);
                return Outcome::Unchanged("comment failed");
            }
        }
        self.commit(tx, None, fonts)
    }

    /// Removes comments with the replies to them, their markers first.
    pub(super) fn remove_comments(&mut self, tx: &mut Tx, ids: &[CommentId]) -> Result<(), StepError> {
        let mut gone: Vec<CommentId> = ids.iter().copied().filter(|id| self.doc.comments.by_id.contains_key(id)).collect();
        loop {
            let more: Vec<CommentId> = self
                .doc
                .comments
                .by_id
                .values()
                .filter(|c| !gone.contains(&c.id) && c.parent.is_some_and(|p| gone.contains(&p)))
                .map(|c| c.id)
                .collect();
            if more.is_empty() {
                break;
            }
            gone.extend(more);
        }
        let marks = self.comment_marks();
        let mut paras: Vec<ParaId> =
            gone.iter().filter_map(|id| marks.get(id)).flat_map(|m| [m.start, m.end, m.reference]).flatten().map(|m| m.para).collect();
        paras.sort();
        paras.dedup();
        for para in paras {
            let p = self.index.para(&self.doc, para).ok_or(StepError::Missing)?;
            let inlines = without_markers(p, &gone);
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

    /// Comments whose markers the transaction took out with their content (whole paragraphs, rows, tables or notes):
    /// one left with none goes too, with its replies, in the same transaction; one left with some gets the others back
    /// beside them.
    // shortcut: a range that lost one end collapses to the end it kept; finding where the deletion was would keep
    // the rest of its text in it.
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
        let mut gone = Vec::new();
        for id in hit {
            // One repair at a time, the markers found again after each: a repair moves the indices in its paragraph.
            for _ in 0..3 {
                let m = self.comment_marks().get(&id).copied().unwrap_or_default();
                let run = |e: &Editor, m: Mark| e.index.para(&e.doc, m.para).map(|p| run_before(p, m.unit));
                let repair = match (m.start, m.end, m.reference) {
                    (None, None, None) => {
                        gone.push(id);
                        None
                    }
                    (Some(_), Some(_), Some(_)) | (None, None, Some(_)) => None,
                    (None, Some(e), _) => self.insert_before(e.para, e.inline, vec![range_mark(RangeKind::CommentStart, id)]),
                    (Some(s), None, _) => self.insert_before(s.para, s.inline + 1, vec![range_mark(RangeKind::CommentEnd, id)]),
                    (Some(_), Some(e), None) => {
                        run(self, e).and_then(|run| self.insert_before(e.para, e.inline + 1, vec![Inline::CommentRef { id, run }]))
                    }
                };
                if repair.is_none_or(|s| self.step(tx, s).is_err()) {
                    break;
                }
            }
        }
        if !gone.is_empty() {
            let _ = self.remove_comments(tx, &gone);
        }
    }
}
