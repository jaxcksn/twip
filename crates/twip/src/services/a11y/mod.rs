//! The accessibility surface: the reading tree ([`tree`]) and what to announce as the caret moves ([`announce`]). The
//! review marks both report (insertions, deletions and commented ranges, and their authors) are found here.

mod announce;
mod tree;
mod window;

pub use announce::{Announcer, CellContext};
pub(crate) use tree::caret_para;
pub use tree::diff;
pub use tree::{TreeCache, tree, tree_with};
pub use window::{Window, window};

use std::rc::Rc;

use crate::api::services::AccessibleMark;
use crate::docx::review::{AuthorId, CommentId, MarkupView, RangeKind};
use crate::docx::{Block, Document, Inline, Paragraph};
use crate::editing::Editor;
use crate::editing::text::{OBJECT, UnitText};
use crate::layout::{ParaId, Unit};

use super::spelling::{SpellError, Spellers};

/// What a review mark says its text is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReviewMark {
    pub kind: ReviewKind,
    pub author: AuthorId,
}

/// A run of units carrying one review mark: (start, end, mark).
pub(super) type MarkSpan = (u32, u32, ReviewMark);

/// The review marks of `p`'s units that `view` shows. `open`: the comments whose ranges are open at its start, kept
/// up to date for the next paragraph.
pub(super) fn para_marks(doc: &Document, p: &Paragraph, view: MarkupView, open: &mut Vec<CommentId>) -> Vec<MarkSpan> {
    let mut spans: Vec<MarkSpan> = Vec::new();
    let mut at = 0u32;
    for i in &p.inlines {
        match i {
            Inline::Range(m) if m.kind == RangeKind::CommentStart => open.push(CommentId(m.id)),
            Inline::Range(m) if m.kind == RangeKind::CommentEnd => open.retain(|c| c.0 != m.id),
            _ => {}
        }
        let n = i.units() as u32;
        let rev = i.run().map(|r| &r.rev);
        if n == 0 || rev.is_some_and(|r| view.hides(r)) {
            at += n;
            continue;
        }
        let revs = rev
            .into_iter()
            .flat_map(|r| [r.ins.as_ref().map(|m| (ReviewKind::Insertion, m.author)), r.del.as_ref().map(|m| (ReviewKind::Deletion, m.author))]);
        let comments =
            open.iter().filter_map(|c| doc.comments.by_id.get(&doc.comments.root(*c))).map(|c| Some((ReviewKind::Comment(c.id), c.author)));
        for (kind, author) in revs.chain(comments).flatten() {
            let mark = ReviewMark { kind, author };
            match spans.iter_mut().rev().find(|s| s.2 == mark && s.1 == at) {
                Some(s) => s.1 = at + n,
                None => spans.push((at, at + n, mark)),
            }
        }
        at += n;
    }
    spans
}

/// The review marks of the body paragraph `para`, with the comments open before it followed from the start.
pub(super) fn marks_of(doc: &Document, para: ParaId) -> Vec<MarkSpan> {
    fn walk(doc: &Document, list: &[Block], para: ParaId, open: &mut Vec<CommentId>) -> Option<Vec<MarkSpan>> {
        for b in list {
            match b {
                Block::Para(p) => {
                    let spans = para_marks(doc, p, doc.review.view, open);
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
    errors: Rc<[SpellError]>,
    text: UnitText,
    chars: Vec<char>,
    /// The run language of each unit.
    langs: Vec<&'a str>,
    /// The units a field showing its result has its code in.
    codes: Vec<(u32, u32)>,
    doc_lang: &'a str,
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
        let view = doc.review.view;
        let spans = para_marks(doc, p, view, open);
        let text = UnitText::of_view(p, view);
        let chars: Vec<char> = text.text.chars().collect();
        let errors = spelling.errors(e, ParaId(p.id), language).map(|(errs, _)| errs).unwrap_or_default();
        let starts: Vec<u32> = p.inlines.iter().scan(0u32, |at, i| Some(std::mem::replace(at, *at + i.units() as u32))).collect();
        let at = |k: usize| starts.get(k).copied().unwrap_or(chars.len() as u32);
        let codes = crate::docx::fields::spans(&p.inlines)
            .iter()
            .filter(|s| !matches!(p.inlines[s.begin], Inline::FieldBegin { view: crate::docx::FieldView::Code, .. }))
            .map(|s| (at(s.begin), at(s.code_end())))
            .collect();
        let langs = p.inlines.iter().flat_map(|i| std::iter::repeat_n(i.run().map_or("", |r| r.lang.as_str()), i.units())).collect();
        Reading { doc, spans, errors, text, chars, langs, codes, doc_lang }
    }

    /// The paragraph's units.
    pub(super) fn len(&self) -> u32 {
        self.chars.len() as u32
    }

    /// The character read at unit `u`: none for a hidden unit, an object or a field's code.
    pub(super) fn char(&self, u: u32) -> Option<char> {
        let c = *self.chars.get(u as usize)?;
        if c == OBJECT || self.text.is_hidden(Unit(u)) || self.codes.iter().any(|&(b, e)| b <= u && u <= e) {
            return None;
        }
        Some(if matches!(c, '\u{2028}' | '\u{c}') { '\n' } else { c })
    }

    /// What unit `u` is to a reader.
    pub(super) fn marks(&self, u: u32) -> TextMarks {
        let mut m = TextMarks::default();
        for s in self.spans.iter().filter(|s| s.0 <= u && u < s.1) {
            match s.2.kind {
                ReviewKind::Comment(c) => m.comment = m.comment.or(Some(c)),
                kind => {
                    m.marks.extend(kind.mark());
                    m.author.get_or_insert_with(|| author(self.doc, s.2.author).to_string());
                }
            }
        }
        if self.errors.iter().any(|x| x.start <= u as usize && (u as usize) < x.end()) {
            m.marks.push(AccessibleMark::Misspelling);
        }
        let lang = self.langs.get(u as usize).copied().unwrap_or("");
        m.lang = Some(lang.to_string()).filter(|l| !l.is_empty() && !l.eq_ignore_ascii_case(self.doc_lang));
        m
    }
}

/// The document's language: its defaults', else `language`.
pub(super) fn document_language(doc: &Document, language: &str) -> String {
    doc.resolver.as_ref().and_then(|r| r.styles.doc_rpr.lang.clone()).filter(|l| !l.is_empty()).unwrap_or_else(|| language.to_string())
}
