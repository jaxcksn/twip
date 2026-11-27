//! Cross-references

use std::collections::{HashMap, HashSet};

use crate::docx::fields;
use crate::docx::{FieldKind, Inline, NoteKind, Paragraph};
use crate::font::Fonts;
use crate::layout::{ParaId, Unit};

use super::Editor;
use super::commands::Outcome;
use super::merge::Where;
use super::step::{Step, para_len};

/// What a cross-reference points to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RefKind {
    /// List paragraphs with a number (not a bullet).
    NumberedItem,
    /// Paragraphs with an outline level.
    Heading,
    /// The document's bookmarks, hidden ones left out.
    Bookmark,
    /// Footnotes, by their reference marks.
    Footnote,
    /// Endnotes, by their reference marks.
    Endnote,
    /// Captions of this label (their sequence field's identifier).
    Caption(String),
}

/// One item a cross-reference can point to, in document order (bookmarks by name).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RefTarget {
    /// The kind it was listed for.
    pub kind: RefKind,
    /// The paragraph it is (a note's: the paragraph holding its reference mark).
    pub para: ParaId,
    /// What Word lists for it: a numbered item's number and text, a heading's text, a bookmark's name, a note's text,
    /// a whole caption.
    pub text: String,
    /// Its number: a list number, a heading's, a note's as laid out.
    pub number: Option<String>,
    /// Its heading level (headings).
    pub level: Option<u8>,
    /// A bookmark target's name.
    pub bookmark: Option<String>,
    /// A note target's id.
    pub note: Option<i64>,
}

/// What a cross-reference shows of its target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefContent {
    /// Its text: a heading's or numbered item's text, a bookmark's text, the entire caption.
    Text,
    /// The page it is on.
    PageNumber,
    /// Its paragraph number as Word's "Heading number"/"Paragraph number": relative to where the reference is.
    Number,
    /// Its paragraph number without the levels above it.
    NumberNoContext,
    /// Its paragraph number with every level.
    NumberFullContext,
    /// "above" or "below".
    AboveBelow,
    /// A note's number.
    NoteNumber,
    /// A note's number formatted as its reference mark.
    NoteNumberFormatted,
    /// A caption's label and number only.
    LabelAndNumber,
    /// A caption's text only (after its number).
    CaptionText,
}

/// A cross-reference to insert: what it shows, whether it links to its target and whether "above"/"below" follows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CrossReference {
    /// What it shows.
    pub content: RefContent,
    /// It links to its target.
    pub hyperlink: bool,
    /// "above"/"below" follows it.
    pub above_below: bool,
}

/// How many targets a list holds at most: a hostile document's past it are left out.
const MAX_TARGETS: usize = 100_000;

/// The unit range of a caption paragraph's label and number (from its start to its sequence field's end) and of its
/// text after them (and the separator after the number).
fn caption_ranges(p: &Paragraph) -> Option<((u32, u32), (u32, u32))> {
    let spans = fields::spans(&p.inlines);
    let seq = spans.iter().find(|s| matches!(&p.inlines[s.begin], Inline::FieldBegin { kind: Some(FieldKind::Seq(_)), .. }))?;
    let at = |k: usize| p.inlines[..k].iter().map(|i| i.units() as u32).sum::<u32>();
    let num_end = at(seq.end + 1);
    let len = para_len(p).0;
    // The text starts after the separator: Word's caption separators and white space.
    let mut start = num_end;
    let mut u = 0u32;
    'walk: for i in &p.inlines {
        let n = i.units() as u32;
        if u + n <= num_end {
            u += n;
            continue;
        }
        if let Inline::Text(t, _) = i {
            for (k, c) in t.chars().enumerate() {
                let pos = u + k as u32;
                if pos < num_end {
                    continue;
                }
                if !(c.is_whitespace() || matches!(c, ':' | '.' | '-' | '\u{2013}' | '\u{2014}')) {
                    start = pos;
                    break 'walk;
                }
                start = pos + 1;
            }
        } else if n > 0 {
            start = u;
            break;
        }
        u += n;
    }
    Some(((0, num_end), (start.min(len), len)))
}

impl Editor {
    /// The items a cross-reference of `kind` can point to, as Word's Cross-reference dialog lists them.
    pub fn cross_reference_targets(&self, kind: &RefKind) -> Vec<RefTarget> {
        let mut out = Vec::new();
        if *kind == RefKind::Bookmark {
            let mut marks: Vec<_> = self.bookmarks().into_iter().filter(|b| !b.name.starts_with('_')).collect();
            marks.sort_by_key(|b| b.name.to_lowercase());
            for b in marks.into_iter().take(MAX_TARGETS) {
                out.push(RefTarget {
                    kind: kind.clone(),
                    para: b.start.para,
                    text: b.name.clone(),
                    number: None,
                    level: None,
                    bookmark: Some(b.name),
                    note: None,
                });
            }
            return out;
        }
        let numbers = if matches!(kind, RefKind::Footnote | RefKind::Endnote) { Some(self.layout.note_refs_shown()) } else { None };
        for s in &self.doc.sections {
            let mut walk = |p: &Paragraph, in_box: bool| {
                // A text box's captions are targets; its headings, numbered items and notes aren't (Word).
                if out.len() >= MAX_TARGETS || (in_box && !matches!(kind, RefKind::Caption(_))) {
                    return;
                }
                let para = ParaId(p.id);
                let text = fields::visible(&p.inlines).trim().to_string();
                let label = p.list.as_ref().filter(|l| !l.text.is_empty() && l.picture.is_none()).map(|l| l.text.clone());
                match kind {
                    RefKind::NumberedItem => {
                        let bullet = p.style.list_fmt.as_deref().is_some_and(|f| f == "bullet");
                        if let Some(l) = label.filter(|_| !bullet) {
                            out.push(RefTarget {
                                kind: kind.clone(),
                                para,
                                text: format!("{l} {text}"),
                                number: Some(l),
                                level: None,
                                bookmark: None,
                                note: None,
                            });
                        }
                    }
                    RefKind::Heading => {
                        if let Some(l) = p.style.outline_lvl.filter(|l| (0..9).contains(l)) {
                            out.push(RefTarget {
                                kind: kind.clone(),
                                para,
                                text,
                                number: label,
                                level: Some(l as u8 + 1),
                                bookmark: None,
                                note: None,
                            });
                        }
                    }
                    RefKind::Caption(want) => {
                        if super::captions::caption_label(p).is_some_and(|l| l.eq_ignore_ascii_case(&super::captions::sequence_of(want))) {
                            out.push(RefTarget { kind: kind.clone(), para, text, number: None, level: None, bookmark: None, note: None });
                        }
                    }
                    RefKind::Footnote | RefKind::Endnote => {
                        let nk = if *kind == RefKind::Footnote { NoteKind::Foot } else { NoteKind::End };
                        for i in &p.inlines {
                            let Inline::NoteRef { kind: k, id, .. } = i else { continue };
                            if *k != nk {
                                continue;
                            }
                            let blocks = if nk == NoteKind::Foot { self.doc.footnotes.get(id) } else { self.doc.endnotes.get(id) };
                            let text = blocks
                                .map(|b| {
                                    b.iter()
                                        .filter_map(|b| if let crate::docx::Block::Para(p) = b { Some(fields::visible(&p.inlines)) } else { None })
                                        .collect::<Vec<_>>()
                                        .join(" ")
                                })
                                .unwrap_or_default();
                            let number = numbers.as_ref().and_then(|m| m.get(&(nk, *id))).map(|(n, _)| n.to_string());
                            out.push(RefTarget {
                                kind: kind.clone(),
                                para,
                                text: text.trim().to_string(),
                                number,
                                level: None,
                                bookmark: None,
                                note: Some(*id),
                            });
                        }
                    }
                    RefKind::Bookmark => {}
                }
            };
            fields::each_para(&s.blocks, false, &mut walk);
        }
        out
    }

    /// Inserts a cross-reference to `target` at the selection (Word's Insert Cross-reference): a `REF`, `PAGEREF` or
    /// `NOTEREF` field to a hidden `_Ref` bookmark around the target (one already around exactly that is reused; a
    /// bookmark target is its own), its result evaluated now. One undo step.
    pub fn insert_cross_reference(&mut self, target: &RefTarget, r: &CrossReference, fonts: &Fonts) -> Outcome {
        use RefContent as C;
        let note = matches!(target.kind, RefKind::Footnote | RefKind::Endnote);
        let caption = matches!(target.kind, RefKind::Caption(_));
        let fits = match r.content {
            C::NoteNumber | C::NoteNumberFormatted => note,
            C::LabelAndNumber | C::CaptionText => caption,
            C::Number | C::NumberNoContext | C::NumberFullContext => !note && !caption,
            C::Text => !note,
            C::PageNumber | C::AboveBelow => true,
        };
        if !fits {
            return Outcome::invalid("that content isn't one this kind of target shows");
        }
        let Some(p) = self.index.para(&self.doc, target.para).cloned() else { return Outcome::unavailable("no such target") };
        // The range the bookmark covers, in the target's paragraph.
        let range: (u32, u32) = if note {
            let want = if target.kind == RefKind::Footnote { NoteKind::Foot } else { NoteKind::End };
            let Some(k) = p.inlines.iter().position(|i| matches!(i, Inline::NoteRef { kind, id, .. } if *kind == want && Some(*id) == target.note))
            else {
                return Outcome::unavailable("no such note");
            };
            let at = p.inlines[..k].iter().map(|i| i.units() as u32).sum::<u32>();
            (at, at + p.inlines[k].units() as u32)
        } else if caption && matches!(r.content, C::LabelAndNumber | C::CaptionText) {
            let Some((label, text)) = caption_ranges(&p) else { return Outcome::unavailable("not a caption") };
            if r.content == C::LabelAndNumber { label } else { text }
        } else {
            (0, para_len(&p).0)
        };
        self.begin_batch(Some("Insert Cross-reference".into()));
        let name = match &target.bookmark {
            Some(b) => b.clone(),
            None => match self.ref_mark(target.para, range, fonts) {
                Some(n) => n,
                None => {
                    self.end_batch(false, fonts);
                    return Outcome::Unchanged("the target couldn't be marked");
                }
            },
        };
        let mut code = match r.content {
            C::PageNumber => format!(" PAGEREF {name}"),
            C::NoteNumber => format!(" NOTEREF {name}"),
            C::NoteNumberFormatted => format!(" NOTEREF {name} \\f"),
            C::Number => format!(" REF {name} \\r"),
            C::NumberNoContext => format!(" REF {name} \\n"),
            C::NumberFullContext => format!(" REF {name} \\w"),
            C::AboveBelow => format!(" REF {name} \\p"),
            C::Text | C::LabelAndNumber | C::CaptionText => format!(" REF {name}"),
        };
        // "Above"/"below" goes in a number's field (a page number's reads "on page N" from another page); after text,
        // in a field of its own (Word).
        let own = matches!(r.content, C::Number | C::NumberNoContext | C::NumberFullContext | C::PageNumber | C::NoteNumber | C::NoteNumberFormatted);
        if r.above_below && own {
            code.push_str(" \\p");
        }
        let link = if r.hyperlink { " \\h" } else { "" };
        let mut codes = vec![format!("{code}{link} ")];
        if r.above_below && !own && r.content != C::AboveBelow {
            codes.push(format!(" REF {name} \\p{link} "));
        }
        let (mut made, mut refs) = (std::collections::HashSet::new(), std::collections::HashSet::new());
        for (k, code) in codes.into_iter().enumerate() {
            if k > 0 && !matches!(self.execute(super::commands::Command::InsertText(" ".into()), fonts), Outcome::Changed(_)) {
                self.end_batch(false, fonts);
                return Outcome::Unchanged("the reference couldn't be inserted");
            }
            let out =
                self.execute(super::commands::Command::InsertField(crate::api::command::InsertField { code, display: None, show_code: None }), fonts);
            if !matches!(out, Outcome::Changed(_)) {
                self.end_batch(false, fonts);
                return out;
            }
            // Page and note numbers come from layout; the rest from evaluation.
            match self.made_field() {
                Some(f) if k == 0 && matches!(r.content, C::PageNumber | C::NoteNumber | C::NoteNumberFormatted) => {
                    made.insert(f);
                    // A note number's note, from evaluation.
                    if r.content != C::PageNumber {
                        refs.insert(f);
                    }
                }
                Some(f) => {
                    refs.insert(f);
                }
                None => {}
            }
        }
        let last = self.made_field();
        if !refs.is_empty() {
            self.refresh_values(&|f, _| refs.contains(&f), fonts);
        }
        self.fill_layout_results(&made, fonts);
        // The caret after the reference, its result now filled.
        if let Some(Where::Inline { story, para, end, .. }) = last.and_then(|f| self.locate(crate::docx::ElementId::Field(f))) {
            self.selection = super::Selection::Caret(super::Position::text(story, para, end));
        }
        self.end_batch(true, fonts)
    }

    /// The field the last command made.
    fn made_field(&self) -> Option<crate::docx::FieldId> {
        match &self.made {
            Some(crate::api::command::CommandResult::Element { id }) => match super::elements::parse_id(&id.0)? {
                crate::docx::ElementId::Field(f) => Some(f),
                _ => None,
            },
            _ => None,
        }
    }

    /// A hidden `_Ref` bookmark around units `range` of `para`: one already there, else a new one.
    fn ref_mark(&mut self, para: ParaId, (a, b): (u32, u32), fonts: &Fonts) -> Option<String> {
        let all = self.bookmarks();
        if let Some(f) = all.iter().find(|f| {
            f.name.len() > 4
                && f.name.get(..4).is_some_and(|p| p.eq_ignore_ascii_case("_ref"))
                && f.start.para == para
                && f.start.unit == Unit(a)
                && f.end.is_some_and(|e| e.para == para && e.unit == Unit(b))
        }) {
            return Some(f.name.clone());
        }
        let taken: HashSet<String> = all.iter().map(|f| f.name.to_ascii_lowercase()).collect();
        let mut next = super::toc::first_free("_Ref", all.iter().map(|f| f.name.as_str()));
        let name = super::toc::free_name("_Ref", &taken, &mut next);
        let back = self.selection;
        let mut tx = self.begin("Insert Cross-reference");
        let end = Step::Text {
            para,
            range: Unit(b)..Unit(b),
            with: vec![Inline::Range(crate::docx::review::RangeMark::bookmark_end(&*name))],
            edges: Default::default(),
        };
        let start = Step::Text {
            para,
            range: Unit(a)..Unit(a),
            with: vec![Inline::Range(crate::docx::review::RangeMark::bookmark_start(&*name))],
            edges: Default::default(),
        };
        if self.step(&mut tx, end).is_err() || self.step(&mut tx, start).is_err() {
            self.abort(tx);
            return None;
        }
        self.commit(tx, None, fonts);
        self.selection = back;
        Some(name)
    }
}

/// How many notes one update brings again into `REF \f` results: a hostile document's past it keep their results.
#[cfg(not(test))]
const MAX_NOTE_COPIES: usize = 1 << 14;
/// How many notes one update brings again into `REF \f` results (a smaller bound, so tests reach it).
#[cfg(test)]
pub(super) const MAX_NOTE_COPIES: usize = 1 << 12;

/// How many inlines one update of `REF \f` results reads (each field reads its own paragraph and its bookmark's):
/// a crafted document of many such fields over long paragraphs keeps the results past it.
const MAX_NOTE_COPY_WORK: usize = 1 << 22;

/// Units of the start of bookmark `name` in `p` and of its end there, else of the paragraph's end.
fn mark_range(p: &Paragraph, name: &str) -> Option<(u32, u32)> {
    let mut at = 0u32;
    let mut from = None;
    for i in &p.inlines {
        if let Some((n, start)) = i.bookmark() {
            let end = !start;
            if n.eq_ignore_ascii_case(name) {
                match (from, end) {
                    (None, false) => from = Some(at),
                    (Some(f), true) => return Some((f, at)),
                    _ => {}
                }
            }
        }
        at += i.units() as u32;
    }
    from.map(|f| (f, at))
}

/// A note copy staged for a `REF \f` result: its kind, id, text, and the controls its text holds.
type NoteCopy = (NoteKind, i64, Vec<crate::docx::Block>, Vec<(crate::docx::ControlId, crate::docx::Control)>);

/// Adds `by` to the count of each note reference in `blocks` (a note's text).
fn count_note_refs(blocks: &[crate::docx::Block], counts: &mut HashMap<(NoteKind, i64), isize>, by: isize) {
    for b in blocks {
        match b {
            crate::docx::Block::Para(p) => {
                for i in &p.inlines {
                    if let Inline::NoteRef { kind, id, .. } = i {
                        *counts.entry((*kind, *id)).or_default() += by;
                    }
                }
            }
            crate::docx::Block::Table(t) => t.rows.iter().flat_map(|r| &r.cells).for_each(|c| count_note_refs(&c.blocks, counts, by)),
        }
    }
}

impl Editor {
    /// Brings the notes the bookmark of each `REF \f` of `ids` (unlocked, in the body) marks into its result again, as
    /// notes of their own (Word's `\f`): the bookmark's text as shown, each note reference in it a new note holding a
    /// copy of that note's text. The notes its old result brought go when nothing else refers to them. Returns how many
    /// results changed.
    pub fn refresh_note_copies(&mut self, ids: &HashSet<crate::docx::FieldId>, fonts: &Fonts) -> usize {
        let mut todo: Vec<(ParaId, crate::docx::FieldId, String)> = Vec::new();
        for para in self.paras_in_order() {
            let Some(p) = self.index.para(&self.doc, para) else { continue };
            for s in fields::spans(&p.inlines) {
                let Inline::FieldBegin { id, kind: Some(FieldKind::Ref(name)), lock: false, .. } = &p.inlines[s.begin] else { continue };
                if ids.contains(id)
                    && s.sep.is_some()
                    && fields::words(&fields::code_text(&p.inlines, s)).iter().any(|w| w.eq_ignore_ascii_case("\\f"))
                {
                    todo.push((para, *id, name.clone()));
                }
            }
        }
        if todo.is_empty() {
            return 0;
        }
        let mut tx = self.begin("Update Fields");
        let (mut changed, mut copies, mut work) = (0, 0usize, 0usize);
        // Each bookmark's first paragraph, and how many references each note has: one walk each.
        let marks: HashMap<String, ParaId> = self.bookmarks().into_iter().rev().map(|f| (f.name.to_ascii_lowercase(), f.start.para)).collect();
        let mut counts: HashMap<(NoteKind, i64), isize> = HashMap::new();
        for pid in self.index.para_ids() {
            let Some(p) = self.index.para(&self.doc, pid) else { continue };
            for i in &p.inlines {
                if let Inline::NoteRef { kind, id, .. } = i {
                    *counts.entry((*kind, *id)).or_default() += 1;
                }
            }
        }
        let mut next = [NoteKind::Foot, NoteKind::End].map(|k| {
            let notes = if k == NoteKind::Foot { &self.doc.footnotes } else { &self.doc.endnotes };
            notes.keys().copied().chain(counts.keys().filter(|c| c.0 == k).map(|c| c.1)).max().unwrap_or(0)
        });
        // Where the search for a free id below the largest starts, once the largest is the last one there is.
        let mut low = [1i64; 2];
        'fields: for (para, f, name) in todo {
            let Some(&at) = marks.get(&name.to_ascii_lowercase()) else { continue };
            let Some(src) = self.index.para(&self.doc, at) else { continue };
            work += src.inlines.len();
            if work > MAX_NOTE_COPY_WORK || copies >= MAX_NOTE_COPIES {
                break;
            }
            if src.inlines.iter().all(|i| !matches!(i, Inline::NoteRef { .. })) {
                continue;
            }
            let Some((from, to)) = mark_range(src, &name) else { continue };
            let src = src.clone();
            // The bookmarked inlines as they show, each note reference to a copy of its note.
            let mut with: Vec<Inline> = Vec::new();
            let mut notes: Vec<NoteCopy> = Vec::new();
            let mut at = 0u32;
            for (i, shown) in src.inlines.iter().zip(fields::shown(&src.inlines)) {
                let n = i.units() as u32;
                let (s, e) = (at, at + n);
                at = e;
                if if n == 0 { s < from || s >= to } else { e <= from || s >= to } {
                    continue;
                }
                match (i, shown) {
                    (_, fields::Shown::Value(v)) => with.extend(i.run().map(|r| Inline::Text(v, r.clone()))),
                    (Inline::Text(t, r), fields::Shown::Itself) => {
                        let (a, z) = (from.saturating_sub(s) as usize, (to.min(e) - s) as usize);
                        with.push(Inline::Text(t.chars().skip(a).take(z.saturating_sub(a)).collect(), r.clone()));
                    }
                    (Inline::Tab(_) | Inline::Break(..), fields::Shown::Itself) => with.push(i.clone()),
                    (Inline::NoteRef { kind, id, custom, run }, fields::Shown::Itself) => {
                        let body = if *kind == NoteKind::Foot { self.doc.footnotes.get(id) } else { self.doc.endnotes.get(id) };
                        let Some(body) = body else { continue };
                        // A field whose notes don't all fit keeps its result; later ones may still fit.
                        if copies + notes.len() >= MAX_NOTE_COPIES {
                            continue 'fields;
                        }
                        let map = if *kind == NoteKind::Foot { &self.doc.footnotes } else { &self.doc.endnotes };
                        let k = usize::from(*kind == NoteKind::End);
                        let to_id = super::notes::free_note_id(next[k], low[k], |n| map.contains_key(&n) || counts.contains_key(&(*kind, n)));
                        if to_id > next[k] {
                            next[k] = to_id;
                        } else {
                            low[k] = to_id + 1;
                        }
                        let mut copy: Vec<crate::docx::Block> = body.iter().map(super::clipboard::fresh).collect();
                        // Its fields and controls their own.
                        let controls = super::clipboard::adopt(&mut copy, &self.doc.controls);
                        renumber_marks(&mut copy, *kind, to_id);
                        notes.push((*kind, to_id, copy, controls));
                        with.push(Inline::NoteRef { kind: *kind, id: to_id, custom: *custom, run: run.clone() });
                    }
                    _ => {}
                }
            }
            copies += notes.len();
            // The field's result now, and the notes it brought.
            let Some(p) = self.index.para(&self.doc, para) else { continue };
            work += p.inlines.len();
            let Some(s) = fields::spans(&p.inlines).into_iter().find(|s| matches!(&p.inlines[s.begin], Inline::FieldBegin { id, .. } if *id == f))
            else {
                continue;
            };
            let Some(sep) = s.sep else { continue };
            let before = |k: usize| Unit(p.inlines[..k].iter().map(Inline::units).sum::<usize>() as u32);
            let range = Unit(before(sep).0 + 1)..before(s.end);
            let old: Vec<(NoteKind, i64)> = p.inlines[sep + 1..s.end]
                .iter()
                .filter_map(|i| if let Inline::NoteRef { kind, id, .. } = i { Some((*kind, *id)) } else { None })
                .collect();
            for (kind, id, blocks, controls) in notes {
                for (c, k) in controls {
                    let _ = self.step(&mut tx, Step::Control { id: c, control: Some(k) });
                }
                count_note_refs(&blocks, &mut counts, 1);
                let _ = self.step(&mut tx, Step::Note { kind, id, blocks: Some(blocks) });
            }
            let made: Vec<(NoteKind, i64)> =
                with.iter().filter_map(|i| if let Inline::NoteRef { kind, id, .. } = i { Some((*kind, *id)) } else { None }).collect();
            if self.step(&mut tx, Step::Text { para, range, with, edges: Default::default() }).is_err() {
                continue;
            }
            for k in made {
                *counts.entry(k).or_default() += 1;
            }
            // What the result still holds of the old: tracked, a deletion a rejection brings back.
            let left: Vec<(NoteKind, i64)> = self
                .index
                .para(&self.doc, para)
                .filter(|_| tx.tracked)
                .and_then(|p| {
                    let s =
                        fields::spans(&p.inlines).into_iter().find(|s| matches!(&p.inlines[s.begin], Inline::FieldBegin { id, .. } if *id == f))?;
                    Some(
                        p.inlines[s.begin..s.end]
                            .iter()
                            .filter_map(|i| if let Inline::NoteRef { kind, id, .. } = i { Some((*kind, *id)) } else { None })
                            .filter(|k| old.contains(k))
                            .collect(),
                    )
                })
                .unwrap_or_default();
            for k in &old {
                *counts.entry(*k).or_default() -= 1;
            }
            for k in left {
                *counts.entry(k).or_default() += 1;
            }
            // Its result is what it shows now, not a value read before.
            if let Some(p) = self.index.para(&self.doc, para)
                && p.inlines.iter().any(|i| matches!(i, Inline::FieldBegin { id, value: Some(_), .. } if *id == f))
            {
                let mut inlines = p.inlines.clone();
                for i in &mut inlines {
                    if let Inline::FieldBegin { id, value, .. } = i
                        && *id == f
                    {
                        *value = None;
                    }
                }
                let _ = self.step(&mut tx, Step::Restyle { para, inlines });
            }
            for &(kind, id) in &old {
                if counts.get(&(kind, id)).is_some_and(|&n| n <= 0) {
                    counts.remove(&(kind, id));
                    let body = if kind == NoteKind::Foot { self.doc.footnotes.get(&id) } else { self.doc.endnotes.get(&id) };
                    if let Some(body) = body.cloned() {
                        count_note_refs(&body, &mut counts, -1);
                        let _ = self.step(&mut tx, Step::Note { kind, id, blocks: None });
                    }
                }
            }
            changed += 1;
        }
        self.commit(tx, None, fonts);
        changed
    }
}

/// A copied note's own mark (`kind`) given its new id `id`.
fn renumber_marks(blocks: &mut [crate::docx::Block], kind: NoteKind, id: i64) {
    for b in blocks {
        match b {
            crate::docx::Block::Para(p) => {
                for i in &mut p.inlines {
                    if let Inline::NoteMark(k, n, _) = i
                        && *k == kind
                    {
                        *n = id;
                    }
                }
            }
            crate::docx::Block::Table(t) => t.rows.iter_mut().flat_map(|r| r.cells.iter_mut()).for_each(|c| renumber_marks(&mut c.blocks, kind, id)),
        }
    }
}
