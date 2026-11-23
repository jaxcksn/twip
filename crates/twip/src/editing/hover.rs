//! Hover tooltips

use crate::docx::review::{AuthorId, MarkupView, Timestamp};
use crate::docx::{Block, Inline, NoteKind, RunStyle};
use crate::layout::Unit;

use super::Editor;
use super::position::{Position, StoryId};
use super::revisions::index::RevisionKind;
use super::stories::inline_at;
use super::view::Tooltip;

/// What a hover tooltip is over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HoverTip {
    /// A host's mark over a range: its message.
    Mark,
    /// A hyperlink's address or screen tip.
    Link,
    /// A tracked change: its author, time and kind.
    Revision,
    /// A commented range: its author, time and text.
    Comment,
    /// A footnote or endnote reference: its note's text.
    Note,
    /// Between pages, or on their meeting line with white space hidden: "Double-click to hide (show) white space".
    WhiteSpace,
}

/// Who made a tracked change or comment under the pointer, when, and what it did: a tooltip's parts, for a host that
/// writes the tip in its own language.
#[derive(Clone, Debug, PartialEq)]
pub struct HoverDetail {
    /// Who made it.
    pub author: AuthorId,
    /// When, as the document gives it.
    pub time: Option<Timestamp>,
    /// A tracked change's kind; `None` for a comment.
    pub change: Option<RevisionKind>,
    /// What follows the byline: a change's description ("Inserted", "Formatted: Bold") or the comment's text.
    pub body: String,
}

/// A hover tooltip: what it's over, the tip, and for a change or a comment its parts.
pub type Hovered = (HoverTip, Tooltip, Option<HoverDetail>);

/// What a hover tooltip was looked up for: the pointer, the document's generation, the markup view and the story being edited.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct HoverKey {
    at: (u64, u64),
    generation: u64,
    view: MarkupView,
    story: StoryId,
}

/// The most characters a note's or comment's tooltip shows, an ellipsis after.
const MAX_TIP: usize = 1024;

impl Editor {
    /// The tooltip under the pointer: a marked range's message, a link's address, a footnote or endnote reference's
    /// note text, a tracked change's author, time and kind (in the All Markup view), or a commented range's comment;
    /// for a change or a comment, its parts too. None once the document changes, until the pointer moves again.
    /// Looked up again only when the pointer, the document, the markup view
    /// or the story being edited changed since the last call.
    pub fn hover_tip(&mut self) -> Option<&Hovered> {
        let (x, y) = self.hover?;
        if self.hover_generation != self.generation {
            return None;
        }
        let key = HoverKey { at: (x.to_bits(), y.to_bits()), generation: self.generation, view: self.doc.review.view, story: self.editing() };
        if self.hover_cache.as_ref().is_none_or(|(k, _)| *k != key) {
            #[cfg(test)]
            {
                self.hover_lookups += 1;
            }
            let tip = self.look_up_tip(x, y);
            self.hover_cache = Some((key, tip));
        }
        self.hover_cache.as_ref().and_then(|(_, t)| t.as_ref())
    }

    fn look_up_tip(&mut self, x: f64, y: f64) -> Option<Hovered> {
        if let Some(hide) = self.white_space_toggle_at(y) {
            let stack = self.stack();
            let (page, _, hy) = stack.page_at(x, y)?;
            let w = stack.pages.get(page)?[2];
            let text = if hide { "Double-click to hide white space" } else { "Double-click to show white space" };
            return Some((HoverTip::WhiteSpace, Tooltip { text: text.into(), page, rect: [0.0, hy, w, hy] }, None));
        }
        let (page, hx, hy) = self.stack().page_at(x, y)?;
        if let (_, Some(t)) = self.mark_views(page..page + 1) {
            return Some((HoverTip::Mark, t, None));
        }
        let p = self.hit_laid_out(x, y)?;
        let under = |e: &mut Editor, a: Position, b: Position| {
            e.range_rects(a, b, page..page + 1).into_iter().map(|(_, r)| r).find(|r| r.contains(hx, hy)).map(|r| r.page())
        };
        if let Some(link) = self.link_at(p) {
            let (a, b) = self.locate(crate::docx::ElementId::Field(link.field)).and_then(|w| self.element_range(&w))?;
            let rect = under(self, a, b)?;
            return Some((HoverTip::Link, Tooltip { text: link.address, page, rect }, None));
        }
        if let Some((at, text)) = self.note_under(p) {
            let end = Position::text(at.story, at.para_offset()?.0, Unit(at.para_offset()?.1.0 + 1));
            if let Some(rect) = under(self, at, end) {
                return Some((HoverTip::Note, Tooltip { text, page, rect }, None));
            }
        }
        // The pointer over a character's right half resolves to the stop after it: the character it is over is the
        // one at the stop or before it whose own rectangle holds the pointer.
        let over = [Some(p), self.char_before(p)].into_iter().flatten().find(|&q| {
            let next = q.para_offset().map(|(para, u)| Position::text(q.story, para, Unit(u.0 + 1)));
            next.is_some_and(|n| under(self, q, n).is_some())
        });
        let over = over.unwrap_or(p);
        if self.doc.review.view == MarkupView::All
            && let Some((a, b, d)) = self.revision_under(over)
            && let Some(rect) = under(self, a, b)
        {
            let text = format!("{}\n{}", self.byline(&d), d.body);
            return Some((HoverTip::Revision, Tooltip { text, page, rect }, Some(d)));
        }
        let (a, b, d) = self.comment_under(over)?;
        let rect = under(self, a, b)?;
        let text = format!("{}\n{}", self.byline(&d), d.body);
        Some((HoverTip::Comment, Tooltip { text, page, rect }, Some(d)))
    }

    /// The position a character before `p`, in its paragraph.
    fn char_before(&self, p: Position) -> Option<Position> {
        let (para, u) = p.para_offset()?;
        Some(Position::text(p.story, para, Unit(u.0.checked_sub(1)?)))
    }

    /// The footnote or endnote reference at `p` or just before it: where it is, and its note's text.
    fn note_under(&self, p: Position) -> Option<(Position, String)> {
        let (para, u) = p.para_offset()?;
        let q = self.index.para(&self.doc, para)?;
        [Some(u), u.0.checked_sub(1).map(Unit)].into_iter().flatten().find_map(|v| match inline_at(q, v) {
            Some(Inline::NoteRef { kind, id, .. }) => {
                let blocks = match kind {
                    NoteKind::Foot => self.doc.footnotes.get(id)?,
                    NoteKind::End => self.doc.endnotes.get(id)?,
                };
                Some((Position::text(p.story, para, v), tip_text(blocks)))
            }
            _ => None,
        })
    }

    /// The innermost tracked change holding `p` (the one starting last): its range and parts.
    fn revision_under(&mut self, p: Position) -> Option<(Position, Position, HoverDetail)> {
        let (list, _) = self.review_list.get(self.generation, &self.doc, &self.index);
        let idx = &self.index;
        let r = list
            .iter()
            .filter(|r| r.story == p.story && idx.cmp(&r.start, &p).is_le() && idx.cmp(&p, &r.end).is_lt())
            .max_by(|x, y| idx.cmp(&x.start, &y.start))?;
        let (start, end, kind, author, time) = (r.start, r.end, r.kind, r.author, r.date.clone());
        let body = match kind {
            RevisionKind::Insert => "Inserted".to_string(),
            RevisionKind::Delete => "Deleted".to_string(),
            RevisionKind::MovedFrom | RevisionKind::MovedTo => "Moved".to_string(),
            RevisionKind::RowInsert => "Inserted Row".to_string(),
            RevisionKind::RowDelete => "Deleted Row".to_string(),
            RevisionKind::Property => self.formatted_at(p),
            RevisionKind::ParagraphProperty => "Formatted: Paragraph".to_string(),
            RevisionKind::TableProperty => "Formatted Table".to_string(),
            RevisionKind::SectionProperty => "Formatted: Section".to_string(),
            RevisionKind::CellInsertion => "Inserted Cells".to_string(),
            RevisionKind::CellDeletion => "Deleted Cells".to_string(),
            RevisionKind::CellMerge => "Merged Cells".to_string(),
        };
        Some((start, end, HoverDetail { author, time, change: Some(kind), body }))
    }

    /// "Formatted: …" for the formatting change on the run at `p`.
    fn formatted_at(&self, p: Position) -> String {
        let run = p.para_offset().and_then(|(para, u)| inline_at(self.index.para(&self.doc, para)?, u)?.run());
        let names = run.and_then(|r| Some(format_names(&r.rev.fmt.as_ref()?.previous, r))).unwrap_or_default();
        if names.is_empty() { "Formatted".to_string() } else { format!("Formatted: {}", names.join(", ")) }
    }

    /// The innermost comment whose range holds `p`: its range and parts.
    fn comment_under(&mut self, p: Position) -> Option<(Position, Position, HoverDetail)> {
        let ranges = self.thread_ranges();
        let idx = &self.index;
        let r = ranges
            .iter()
            .filter(|r| r.start.story == p.story && idx.cmp(&r.start, &p).is_le() && idx.cmp(&p, &r.end).is_lt())
            .max_by(|x, y| idx.cmp(&x.start, &y.start))?;
        let c = self.doc.comments.by_id.get(&r.root)?;
        let time = c.date_utc.as_ref().or(c.date.as_ref()).cloned();
        Some((r.start, r.end, HoverDetail { author: c.author, time, change: None, body: tip_text(&c.body) }))
    }

    /// "Author, M/d/yyyy h:mm:ss AM/PM" in the local time zone, or the author alone without a readable date.
    fn byline(&self, d: &HoverDetail) -> String {
        let name = self.doc.people.get(d.author).map_or("", |p| &p.name);
        match d.time.as_ref().and_then(|t| t.utc) {
            Some(t) => format!("{name}, {}", crate::docx::fields::date_text("M/d/yyyy h:mm:ss AM/PM", t + crate::docx::local_offset(t))),
            None => name.to_string(),
        }
    }
}

/// The most inlines a tooltip's text walks, field codes and hidden text included.
const MAX_WALK: usize = 16 * 1024;
/// The deepest table nesting a tooltip's text walks into.
const MAX_DEPTH: usize = 16;

/// A note's or comment's plain text as a tooltip shows it: paragraphs on their own lines, field results without
/// their codes, no hidden text, trimmed, at most [`MAX_TIP`] characters and an ellipsis.
fn tip_text(blocks: &[Block]) -> String {
    let mut t = Tip { out: String::new(), chars: 0, walked: 0, fields: Vec::new(), in_code: 0, paras: 0 };
    t.blocks(blocks, 0);
    let trimmed = t.out.trim();
    match trimmed.char_indices().nth(MAX_TIP) {
        Some((at, _)) => format!("{}…", &trimmed[..at]),
        None if t.chars > MAX_TIP || t.walked >= MAX_WALK => format!("{trimmed}…"),
        None => trimmed.to_string(),
    }
}

struct Tip {
    out: String,
    chars: usize,
    walked: usize,
    /// Each open field: whether its code is still going on.
    fields: Vec<bool>,
    /// How many open fields' codes are going on.
    in_code: usize,
    paras: usize,
}

impl Tip {
    fn full(&self) -> bool {
        self.chars > MAX_TIP || self.walked >= MAX_WALK
    }

    fn blocks(&mut self, blocks: &[Block], depth: usize) {
        for b in blocks {
            if self.full() {
                return;
            }
            match b {
                Block::Para(p) => self.para(&p.inlines),
                Block::Table(t) if depth < MAX_DEPTH => t.rows.iter().flat_map(|r| r.cells.iter()).for_each(|c| self.blocks(&c.blocks, depth + 1)),
                Block::Table(_) => {}
            }
        }
    }

    fn para(&mut self, inlines: &[Inline]) {
        if self.paras > 0 {
            self.push("\n");
        }
        self.paras += 1;
        self.fields.clear();
        self.in_code = 0;
        for i in inlines {
            if self.full() {
                return;
            }
            self.walked += 1;
            let text = match i {
                Inline::FieldBegin { .. } => {
                    self.fields.push(true);
                    self.in_code += 1;
                    continue;
                }
                Inline::FieldSep { .. } => {
                    if let Some(code) = self.fields.last_mut().filter(|c| **c) {
                        *code = false;
                        self.in_code -= 1;
                    }
                    continue;
                }
                Inline::FieldEnd { .. } => {
                    if self.fields.pop() == Some(true) {
                        self.in_code -= 1;
                    }
                    continue;
                }
                _ if self.in_code > 0 => continue,
                Inline::Text(t, r) if !r.hidden => t.as_str(),
                Inline::Tab(r) if !r.hidden => "\t",
                Inline::Break(k, r) if k.is_line() && !r.hidden => "\n",
                _ => continue,
            };
            self.push(text);
        }
    }

    fn push(&mut self, text: &str) {
        let room = MAX_TIP + 1 - self.chars.min(MAX_TIP + 1);
        let end = text.char_indices().nth(room).map_or(text.len(), |(at, _)| at);
        self.chars += text[..end].chars().count();
        self.out.push_str(&text[..end]);
    }
}

/// The formatting `before` and `after` differ in, by name, as a formatting change describes it.
pub fn format_names(before: &RunStyle, after: &RunStyle) -> Vec<&'static str> {
    let diffs: [(bool, &str); 12] = [
        (before.bold != after.bold, if after.bold { "Bold" } else { "Not Bold" }),
        (before.italic != after.italic, if after.italic { "Italic" } else { "Not Italic" }),
        (before.underline != after.underline, "Underline"),
        (before.strike != after.strike, "Strikethrough"),
        (before.dstrike != after.dstrike, "Double strikethrough"),
        (before.ascii != after.ascii, "Font"),
        (before.size != after.size, "Font size"),
        (before.color != after.color, "Font color"),
        (before.highlight != after.highlight, "Highlight"),
        (before.vert_align != after.vert_align, "Position"),
        (before.caps != after.caps || before.small_caps != after.small_caps, "Caps"),
        (before.hidden != after.hidden, "Hidden"),
    ];
    diffs.iter().filter(|d| d.0).map(|d| d.1).collect()
}
