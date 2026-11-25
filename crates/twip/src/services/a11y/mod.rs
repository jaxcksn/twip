//! The accessibility surface: the reading tree ([`tree`]) and what to announce as the caret moves ([`announce`]). The
//! review marks both report (insertions, deletions and commented ranges, and their authors) are found here.

mod announce;
pub(crate) mod roles;
mod tree;
mod window;

pub use announce::{Announcer, CellContext};
pub(crate) use tree::caret_para;
pub use tree::diff;
pub use tree::{TreeCache, tree, tree_with};
pub use window::{Window, window};

use std::collections::HashMap;

use crate::api::services::AccessibleMark;
use crate::docx::review::{AuthorId, CommentId, MarkupView, RangeKind, ReviewView, RunRevisions};
use crate::docx::{Block, Document, Inline, Paragraph};
use crate::editing::Editor;
use crate::editing::text::{OBJECT, UnitText};
use crate::layout::{ParaId, Unit};

use super::spelling::Spellers;

/// What a review mark says its text is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ReviewKind {
    Insertion,
    Deletion,
    /// In the range of a comment thread: its first comment.
    Comment(CommentId),
}

impl ReviewKind {
    fn name(self) -> &'static str {
        match self {
            ReviewKind::Insertion => "insertion",
            ReviewKind::Deletion => "deletion",
            ReviewKind::Comment(_) => "comment",
        }
    }

    /// The text mark it puts on its text; a comment's range is linked to the comment instead.
    pub(super) fn mark(self) -> Option<AccessibleMark> {
        match self {
            ReviewKind::Insertion => Some(AccessibleMark::Insertion),
            ReviewKind::Deletion => Some(AccessibleMark::Deletion),
            ReviewKind::Comment(_) => None,
        }
    }
}

/// An insertion, a deletion or a commented range, and whose it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ReviewMark {
    pub kind: ReviewKind,
    pub author: AuthorId,
}

/// A run of units carrying one review mark: (start, end, mark).
pub(super) type MarkSpan = (u32, u32, ReviewMark);

/// The review marks of `p`'s units that `review` shows. `open`: the comments whose ranges are open at its start, kept
/// up to date for the next paragraph.
pub(super) fn para_marks(doc: &Document, p: &Paragraph, review: &ReviewView, open: &mut Vec<CommentId>) -> Vec<MarkSpan> {
    let mut spans: Vec<MarkSpan> = Vec::new();
    let mut at = 0u32;
    // The spans ending where the next run starts (the last run's): only they go on over it.
    let (mut live, mut now): (HashMap<ReviewMark, usize>, HashMap<ReviewMark, usize>) = (HashMap::new(), HashMap::new());
    for i in &p.inlines {
        match i {
            Inline::Range(m) if m.kind == RangeKind::CommentStart => open.push(CommentId(m.id)),
            Inline::Range(m) if m.kind == RangeKind::CommentEnd => open.retain(|c| c.0 != m.id),
            _ => {}
        }
        let n = i.units() as u32;
        if n == 0 {
            continue;
        }
        let rev = i.run().map(|r| &r.rev);
        if rev.is_some_and(|r| unread(review, r)) {
            at += n;
            live.clear();
            continue;
        }
        let marked = |m: &&std::rc::Rc<crate::docx::review::RevisionMark>| review.markup.show.shows(m);
        let revs = rev.into_iter().flat_map(|r| {
            [
                r.ins.as_ref().filter(marked).map(|m| (ReviewKind::Insertion, m.author)),
                r.del.as_ref().filter(marked).map(|m| (ReviewKind::Deletion, m.author)),
            ]
        });
        let comments = open
            .iter()
            .filter_map(|c| doc.comments.by_id.get(&doc.comments.root(*c)))
            .filter(|c| review.shows_comment(c.author))
            .map(|c| Some((ReviewKind::Comment(c.id), c.author)));
        now.clear();
        for (kind, author) in revs.chain(comments).flatten() {
            let mark = ReviewMark { kind, author };
            let k = match live.get(&mark) {
                Some(&k) => {
                    spans[k].1 = at + n;
                    k
                }
                None => {
                    spans.push((at, at + n, mark));
                    spans.len() - 1
                }
            };
            now.insert(mark, k);
        }
        std::mem::swap(&mut live, &mut now);
        at += n;
    }
    spans
}

/// Whether a run with `rev` isn't read: as [`ReviewView::hides`], but a deletion in a balloon is read where it was.
pub(super) fn unread(review: &ReviewView, rev: &RunRevisions) -> bool {
    let m = &review.markup;
    review.view.hides(rev)
        || rev.del.as_ref().is_some_and(|d| review.view == MarkupView::All && (!m.show.shows(d) || m.style.look(d.kind).mark.hides_text()))
}

/// The review marks of the body paragraph `para`, with the comments open before it followed from the start.
pub(super) fn marks_of(doc: &Document, para: ParaId) -> Vec<MarkSpan> {
    fn walk(doc: &Document, list: &[Block], para: ParaId, open: &mut Vec<CommentId>) -> Option<Vec<MarkSpan>> {
        for b in list {
            match b {
                Block::Para(p) => {
                    let spans = para_marks(doc, p, &doc.review, open);
                    if p.id == para.0 {
                        return Some(spans);
                    }
                }
                Block::Table(t) => {
                    if let Some(s) = t.rows.iter().flat_map(|r| &r.cells).find_map(|c| walk(doc, &c.blocks, para, open)) {
                        return Some(s);
                    }
                }
            }
        }
        None
    }
    let mut open = Vec::new();
    doc.sections.iter().find_map(|s| walk(doc, &s.blocks, para, &mut open)).unwrap_or_default()
}

pub(super) fn author(doc: &Document, id: AuthorId) -> &str {
    doc.people.get(id).map_or("", |p| &p.name)
}

/// What a character is to a reader: its marks, their author, its language where it differs from the document's, and
/// the comment thread it is in the range of.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct TextMarks {
    pub marks: Vec<AccessibleMark>,
    pub author: Option<String>,
    pub lang: Option<String>,
    pub comment: Option<CommentId>,
}

/// A paragraph as a reader gets it, unit by unit: the shown text (a field's result, not its code; a line break as
/// `'\n'`; no objects) and each character's [`TextMarks`].
pub(super) struct Reading<'a> {
    doc: &'a Document,
    spans: Vec<MarkSpan>,
    text: UnitText,
    chars: Vec<char>,
    /// The run language of each unit.
    langs: Vec<&'a str>,
    /// Each unit's [`CODE`] and [`MISSPELT`] flags (empty: none has one).
    flags: Vec<u8>,
    /// Each unit's insertion and deletion spans, by place in `spans` (empty: none has one).
    revs: Vec<[u32; 2]>,
    /// Each unit's first comment span (empty: none is in one).
    comments: Vec<u32>,
    doc_lang: &'a str,
}

/// No span.
const NONE: u32 = u32::MAX;
/// A unit of a field's code (the field shows its result).
const CODE: u8 = 1;
/// A unit of a misspelt word.
const MISSPELT: u8 = 2;

/// For each of `len` units, the first of `ranges` (place, start, end) holding it, or [`NONE`]: each unit is set once,
/// so overlapping ranges cost their units once, not once each.
pub(super) fn first_cover(len: usize, ranges: impl Iterator<Item = (u32, u32, u32)>) -> Vec<u32> {
    let mut out = vec![NONE; len];
    // The first unit at or after each that isn't set yet.
    let mut next: Vec<u32> = (0..=len as u32).collect();
    fn find(next: &mut [u32], mut i: u32) -> u32 {
        while next[i as usize] != i {
            let up = next[next[i as usize] as usize];
            next[i as usize] = up;
            i = up;
        }
        i
    }
    for (k, a, b) in ranges {
        let b = b.min(len as u32);
        let mut u = find(&mut next, a.min(len as u32));
        while u < b {
            out[u as usize] = k;
            next[u as usize] = u + 1;
            u = find(&mut next, u + 1);
        }
    }
    out
}

impl<'a> Reading<'a> {
    /// `p`'s reading. `open`: the comments whose ranges are open at its start, kept up to date for the next paragraph.
    /// `doc_lang`: the document's language.
    pub(super) fn new(
        e: &'a Editor,
        p: &'a Paragraph,
        spelling: &Spellers,
        language: &str,
        doc_lang: &'a str,
        open: &mut Vec<CommentId>,
    ) -> Reading<'a> {
        let doc = e.document();
        let spans = para_marks(doc, p, &doc.review, open);
        let text = UnitText::masked(p, |rev| unread(&doc.review, rev));
        let chars: Vec<char> = text.text.chars().collect();
        let errors = spelling.errors(e, ParaId(p.id), language).unwrap_or_default();
        let codes = crate::docx::fields::hidden_codes(&p.inlines);
        let n = chars.len();
        let mut flags = Vec::new();
        if !codes.is_empty() || !errors.is_empty() {
            flags = vec![0u8; n];
            // Code ranges (their ends included) by how many are open at each unit.
            let mut depth = vec![0i32; n + 1];
            for &(b, e) in &codes {
                depth[(b as usize).min(n)] += 1;
                depth[(e as usize + 1).min(n)] -= 1;
            }
            let mut open = 0;
            for (u, f) in flags.iter_mut().enumerate() {
                open += depth[u];
                if open > 0 {
                    *f |= CODE;
                }
            }
            for x in errors.iter() {
                flags[x.start.min(n)..x.end().min(n)].iter_mut().for_each(|f| *f |= MISSPELT);
            }
        }
        let mut revs = Vec::new();
        if spans.iter().any(|s| !matches!(s.2.kind, ReviewKind::Comment(_))) {
            revs = vec![[NONE; 2]; n];
            for (k, s) in spans.iter().enumerate().filter(|(_, s)| !matches!(s.2.kind, ReviewKind::Comment(_))) {
                for slot in &mut revs[(s.0 as usize).min(n)..(s.1 as usize).min(n)] {
                    // A unit is in one insertion and one deletion at most.
                    if let Some(free) = slot.iter_mut().find(|x| **x == NONE) {
                        *free = k as u32;
                    }
                }
            }
        }
        let comments = if spans.iter().any(|s| matches!(s.2.kind, ReviewKind::Comment(_))) {
            first_cover(n, spans.iter().enumerate().filter(|(_, s)| matches!(s.2.kind, ReviewKind::Comment(_))).map(|(k, s)| (k as u32, s.0, s.1)))
        } else {
            Vec::new()
        };
        let langs = p.inlines.iter().flat_map(|i| std::iter::repeat_n(i.run().map_or("", |r| r.lang.as_str()), i.units())).collect();
        Reading { doc, spans, text, chars, langs, flags, revs, comments, doc_lang }
    }

    /// The paragraph's units.
    pub(super) fn len(&self) -> u32 {
        self.chars.len() as u32
    }

    /// The character read at unit `u`: none for a hidden unit, an object or a field's code.
    pub(super) fn char(&self, u: u32) -> Option<char> {
        let c = *self.chars.get(u as usize)?;
        if c == OBJECT || self.text.is_hidden(Unit(u)) || self.flags.get(u as usize).is_some_and(|f| f & CODE != 0) {
            return None;
        }
        Some(if matches!(c, '\u{2028}' | '\u{c}') { '\n' } else { c })
    }

    /// Whether unit `u` is hidden: a revision the view leaves out, or hidden text.
    pub(super) fn hidden(&self, u: u32) -> bool {
        self.text.is_hidden(Unit(u))
    }

    /// What unit `u` is to a reader.
    pub(super) fn marks(&self, u: u32) -> TextMarks {
        let mut m = TextMarks::default();
        let mut revs = self.revs.get(u as usize).copied().unwrap_or([NONE; 2]);
        revs.sort_unstable();
        for s in revs.iter().filter(|&&k| k != NONE).map(|&k| &self.spans[k as usize]) {
            m.marks.extend(s.2.kind.mark());
            m.author.get_or_insert_with(|| author(self.doc, s.2.author).to_string());
        }
        if let Some(&k) = self.comments.get(u as usize).filter(|&&k| k != NONE)
            && let ReviewKind::Comment(c) = self.spans[k as usize].2.kind
        {
            m.comment = Some(c);
        }
        if self.flags.get(u as usize).is_some_and(|f| f & MISSPELT != 0) {
            m.marks.push(AccessibleMark::Misspelling);
        }
        let lang = self.langs.get(u as usize).copied().unwrap_or("");
        m.lang = roles::other_language(lang, self.doc_lang).map(str::to_string);
        m
    }
}

pub(super) use roles::document_language;
