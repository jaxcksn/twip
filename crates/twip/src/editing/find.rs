//! Find and replace

use crate::docx::Block;
use crate::font::Fonts;
use crate::layout::{ParaId, Unit};

use super::Editor;
use super::commands::{Command, Outcome};
use super::position::Position;
use super::selection::{Granularity, Selection};
use super::text::{Seen, UnitText};

/// Where `needle` occurs in `hay`, without overlaps: `case`, exactly, else ignoring case; `whole`, only where no
/// letter or digit touches either end. A match starts and ends on grapheme boundaries, and a combining mark is part of
/// the letter it follows.
#[cfg(test)]
pub(crate) fn matches(hay: &[char], needle: &[char], case: bool, whole: bool) -> Vec<usize> {
    scan(hay, needle, case, whole, || grapheme_starts(hay), |c, i, end| c.starts(i) && c.starts(end)).0
}

/// A text's grapheme clusters by character index, its end included: where the cluster a character is in starts, and
/// the first cluster start at or after it.
struct Clusters {
    start: Vec<u32>,
    next: Vec<u32>,
}

impl Clusters {
    /// From whether each character starts a grapheme, then the end, which does.
    fn of(starts: &[bool]) -> Clusters {
        let mut start = Vec::with_capacity(starts.len());
        let mut at = 0u32;
        for (k, &s) in starts.iter().enumerate() {
            if s {
                at = k as u32;
            }
            start.push(at);
        }
        let mut next = vec![0u32; starts.len()];
        let mut at = starts.len().saturating_sub(1) as u32;
        for (k, &s) in starts.iter().enumerate().rev() {
            if s {
                at = k as u32;
            }
            next[k] = at;
        }
        Clusters { start, next }
    }

    fn starts(&self, k: usize) -> bool {
        self.start[k] as usize == k
    }

    fn start(&self, k: usize) -> usize {
        self.start[k] as usize
    }

    fn next(&self, k: usize) -> usize {
        self.next[k] as usize
    }
}

/// Whether `needle` is at character `i` of `hay`: `case`, exactly, else ignoring case.
fn same_at(hay: &[char], needle: &[char], case: bool, i: usize) -> bool {
    let eq = |a: char, b: char| if case { a == b } else { a == b || a.to_lowercase().eq(b.to_lowercase()) };
    hay.get(i..i + needle.len()).is_some_and(|h| h.iter().zip(needle).all(|(a, b)| eq(*a, *b)))
}

/// Whether a match at characters `i..end` of `hay` has the ends `fits` takes and, `whole`, no letter or digit in the
/// graphemes either side.
fn fits_at(hay: &[char], whole: bool, c: &Clusters, fits: &impl Fn(&Clusters, usize, usize) -> bool, i: usize, end: usize) -> bool {
    let word = |k: usize| hay[k].is_alphanumeric();
    let before = || i > 0 && word(c.start(i - 1));
    let after = || c.next(end) < hay.len() && word(c.next(end));
    fits(c, i, end) && (!whole || (!before() && !after()))
}

/// Where `needle` occurs in `hay`, without overlaps, at the ends `fits` takes (given the clusters of the grapheme
/// starts `bounds` makes, then a match's start and end): `case` and `whole` as for [`matches`]. The clusters, when a
/// match needed them.
fn scan(
    hay: &[char],
    needle: &[char],
    case: bool,
    whole: bool,
    bounds: impl FnOnce() -> Vec<bool>,
    fits: impl Fn(&Clusters, usize, usize) -> bool,
) -> (Vec<usize>, Option<Clusters>) {
    if needle.is_empty() || hay.len() < needle.len() {
        return (Vec::new(), None);
    }
    let mut bounds = Some(bounds);
    let mut clusters: Option<Clusters> = None;
    let mut out = Vec::new();
    let mut i = 0;
    while i + needle.len() <= hay.len() {
        let end = i + needle.len();
        if same_at(hay, needle, case, i) {
            let c = clusters.get_or_insert_with(|| Clusters::of(&bounds.take().map_or_else(Vec::new, |f| f())));
            if fits_at(hay, whole, c, &fits, i, end) {
                out.push(i);
                i = end;
                continue;
            }
        }
        i += 1;
    }
    (out, clusters)
}

/// Whether each character of `hay` starts a grapheme, then the end.
#[cfg(test)]
fn grapheme_starts(hay: &[char]) -> Vec<bool> {
    use unicode_segmentation::UnicodeSegmentation;
    let s: String = hay.iter().collect();
    let mut out = vec![false; hay.len() + 1];
    let mut k = 0;
    for g in s.graphemes(true) {
        out[k] = true;
        k += g.chars().count();
    }
    out[hay.len()] = true;
    out
}

/// The ends a match in a paragraph's text takes: none inside a letter's cluster (the accents on it make it another
/// letter), and none across a field marker once widened to whole clusters.
fn paragraph_fits(seen: &Seen) -> impl Fn(&Clusters, usize, usize) -> bool + '_ {
    let hay = &seen.chars;
    let fits = move |c: &Clusters, k: usize| c.starts(k) || !hay[c.start(k)].is_alphanumeric();
    move |c, i, end| fits(c, i) && fits(c, end) && !seen.crosses(c.start(i), c.next(end))
}

/// The matches of `needle` in a paragraph's text as it reads, as unit ranges: one inside a symbol's grapheme cluster
/// (an emoji's skin tone) widened to the whole cluster, so a match always selects what it found; none inside a letter's
/// (the accents on it make it another letter), and none across a field marker, so a replacement never takes part of a
/// field.
pub(crate) fn hits(seen: &Seen, needle: &[char], case: bool, whole: bool) -> Vec<(Unit, Unit)> {
    let (found, clusters) = scan(&seen.chars, needle, case, whole, || seen.boundaries(), paragraph_fits(seen));
    let Some(c) = clusters.filter(|_| !found.is_empty()) else { return Vec::new() };
    let mut out: Vec<(Unit, Unit)> = Vec::with_capacity(found.len());
    let mut last = 0;
    for h in found {
        let (a, b) = (c.start(h), c.next(h + needle.len()));
        if a < last {
            continue;
        }
        last = b;
        out.push(seen.units_of(a, b));
    }
    out
}

/// Whether units `a..b` of a paragraph's text hold a match of `needle` as [`hits`] finds them, whatever match before
/// it overlaps.
pub(crate) fn hit_at(seen: &Seen, needle: &[char], case: bool, whole: bool, a: Unit, b: Unit) -> bool {
    let hay = &seen.chars;
    let (from, to) = (seen.units.partition_point(|&u| u < a.0), seen.units.partition_point(|&u| u < b.0));
    if needle.is_empty() || to < from + needle.len() {
        return false;
    }
    let c = Clusters::of(&seen.boundaries());
    let fits = paragraph_fits(seen);
    (from..=to - needle.len()).any(|i| {
        let end = i + needle.len();
        same_at(hay, needle, case, i) && fits_at(hay, whole, &c, &fits, i, end) && seen.units_of(c.start(i), c.next(end)) == (a, b)
    })
}

/// The stories a walk of the document takes besides the body (or the sub-template being edited).
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Stories {
    /// The text boxes, after the story they are in.
    pub boxes: bool,
    /// Footnotes and endnotes.
    pub notes: bool,
    /// Headers and footers.
    pub headers: bool,
}

impl Stories {
    /// Every story the canvas edits: what Find, Replace and spelling go through.
    pub const ALL: Stories = Stories { boxes: true, notes: true, headers: true };
}

/// The paragraph ids of `blocks` in order, tables included, with the text boxes they anchor.
fn walk<'a>(blocks: &'a [Block], out: &mut Vec<ParaId>, boxes: &mut Vec<&'a [Block]>) {
    for b in blocks {
        match b {
            Block::Para(p) => {
                out.push(ParaId(p.id));
                boxes.extend(p.inlines.iter().filter_map(|i| i.story()).map(|(_, tb)| tb.blocks.as_slice()));
            }
            // Nesting is bounded by the reader's XML depth.
            Block::Table(t) => t.rows.iter().flat_map(|r| r.cells.iter()).for_each(|c| walk(&c.blocks, out, boxes)),
        }
    }
}

impl Editor {
    fn body_paras(&self) -> Vec<ParaId> {
        self.story_paras(Stories::ALL)
    }

    /// The paragraph ids of `story` in document order, tables included, not the text boxes it anchors.
    pub(crate) fn paras_of(&self, story: super::position::StoryId) -> Vec<ParaId> {
        let mut out = Vec::new();
        if story == super::position::StoryId::Body {
            // The document's own body, whatever sub-template is edited.
            self.doc.sections.iter().for_each(|s| walk(&s.blocks, &mut out, &mut Vec::new()));
            return out;
        }
        if let Some(blocks) = self.index.blocks(&self.doc, crate::editing::index::Container::Story(story)) {
            walk(blocks, &mut out, &mut Vec::new());
        }
        out
    }

    /// The paragraph ids of the body (or the sub-template being edited) in document order, tables included, then
    /// those of the stories `also` takes: the text boxes, the headers and footers in part order, the footnotes and the
    /// endnotes in id order.
    pub(crate) fn story_paras(&self, also: Stories) -> Vec<ParaId> {
        let mut out = Vec::new();
        let mut boxes = Vec::new();
        let take_boxes = |out: &mut Vec<ParaId>, boxes: &mut Vec<&[Block]>| {
            let mut k = 0;
            while also.boxes && k < boxes.len() {
                let mut more = Vec::new();
                walk(boxes[k], out, &mut more);
                boxes.extend(more);
                k += 1;
            }
            boxes.clear();
        };
        // While a sub-template is edited, its body; never a sub-template's body from the template.
        if let Some(part) = self.editing_sub_template().and_then(|k| self.doc.glossary.as_ref()?.parts.get(k as usize)) {
            walk(&part.body, &mut out, &mut boxes);
            take_boxes(&mut out, &mut boxes);
            return out;
        }
        for s in &self.doc.sections {
            walk(&s.blocks, &mut out, &mut boxes);
        }
        take_boxes(&mut out, &mut boxes);
        if also.headers {
            for blocks in self.doc.stories.values() {
                walk(blocks, &mut out, &mut boxes);
                take_boxes(&mut out, &mut boxes);
            }
        }
        if also.notes {
            for notes in [&self.doc.footnotes, &self.doc.endnotes] {
                let mut ids: Vec<&i64> = notes.keys().collect();
                ids.sort_unstable();
                for id in ids {
                    walk(&notes[id], &mut out, &mut boxes);
                    take_boxes(&mut out, &mut boxes);
                }
            }
        }
        out
    }

    /// Selects units `a..b` of `para` as a find match: [`Editor::selected_match`] knows it while it stays selected.
    pub fn select_match(&mut self, para: ParaId, a: Unit, b: Unit, fonts: &Fonts) -> Outcome {
        let out = self.select_range(para, a, b, fonts);
        self.found = Some((self.selection, para, a, b));
        out
    }

    /// The find match the selection is on: the one selected last while the selection hasn't moved since. A match in a
    /// field's result selects the whole field, so the selection alone doesn't say which.
    pub fn selected_match(&self) -> Option<(ParaId, Unit, Unit)> {
        self.found.filter(|f| f.0 == self.selection).map(|f| (f.1, f.2, f.3))
    }

    /// Whether the unit at `u` is a space the markup view shows (not a tab, break, object, or hidden or deleted text).
    pub fn space_at(&self, para: ParaId, u: Unit) -> bool {
        let Some(p) = self.index.para(&self.doc, para) else { return false };
        let review = &self.doc.review;
        let t = UnitText::masked(p, |rev| rev.del.is_some() || review.hides(rev));
        !t.is_hidden(u) && t.char_at(u) == Some(' ')
    }

    /// A paragraph's text as Find reads it: what the markup view shows, fields as they show, values aside.
    pub(crate) fn find_text(&self, para: ParaId) -> Seen {
        let review = &self.doc.review;
        self.index.para(&self.doc, para).map_or_else(Seen::default, |p| Seen::of(p, |r| !review.hides(&r.rev), false))
    }

    fn unit_hits(&self, para: ParaId, needle: &[char], case: bool) -> Vec<(Unit, Unit)> {
        hits(&self.find_text(para), needle, case, false)
    }

    pub fn find(&self, needle: &str, case: bool, back: bool) -> Option<(ParaId, Unit, Unit)> {
        let needle: Vec<char> = needle.chars().collect();
        let order = self.body_paras();
        if order.is_empty() || needle.is_empty() {
            return None;
        }
        let (a, b) = self
            .selection
            .ordered(&self.index)
            .and_then(|(a, b)| Some((a.para_offset()?, b.para_offset()?)))
            .unwrap_or(((order[0], Unit(0)), (order[0], Unit(0))));
        let (from_para, from) = if back { a } else { b };
        let n = order.len();
        // A selection outside the paragraphs searched: from the top (the end, back).
        let start = order.iter().position(|p| *p == from_para).unwrap_or(if back { n - 1 } else { 0 });
        for k in 0..=n {
            let idx = if back { (start + n * 2 - k) % n } else { (start + k) % n };
            let p = order[idx];
            let hits = self.unit_hits(p, &needle, case);
            let hit = if back {
                hits.iter().rev().copied().find(|h| k > 0 || p != from_para || h.1 <= from)
            } else {
                hits.iter().copied().find(|h| k > 0 || p != from_para || h.0 >= from)
            };
            if let Some((a, b)) = hit {
                return Some((p, a, b));
            }
        }
        None
    }

    pub fn count_matches(&self, needle: &str, case: bool) -> usize {
        let needle: Vec<char> = needle.chars().collect();
        self.body_paras().into_iter().map(|p| self.unit_hits(p, &needle, case).len()).sum()
    }

    pub fn selection_matches(&self, needle: &str, case: bool) -> bool {
        if self.is_discontiguous() {
            return false;
        }
        let Some((a, b)) = self.selection.ordered(&self.index) else { return false };
        let (Some((pa, ua)), Some((pb, ub))) = (a.para_offset(), b.para_offset()) else { return false };
        let needle: Vec<char> = needle.chars().collect();
        pa == pb && hit_at(&self.find_text(pa), &needle, case, false, ua, ub)
    }

    #[cfg(test)]
    pub(crate) fn replace_all(&mut self, find: &str, with: &str, case: bool, fonts: &Fonts) -> (usize, Outcome) {
        let needle: Vec<char> = find.chars().collect();
        let hits: Vec<(ParaId, Unit, Unit)> =
            self.body_paras().into_iter().flat_map(|para| self.unit_hits(para, &needle, case).into_iter().map(move |(a, b)| (para, a, b))).collect();
        self.replace_hits("Replace All", &hits, with, true, fonts)
    }

    /// Replaces `hits` (a paragraph's units `start..end` each, in document order) with `with` as one transaction, last
    /// to first so earlier offsets hold. Each is replaced as typing over it would be: where document protection, a
    /// locked control or a shown sub-template keeps the text, it stays. How many it replaced; refused when protection
    /// kept every one. `stay`: the caret goes back to where the selection started, else it follows the first.
    pub(crate) fn replace_hits(
        &mut self,
        label: &'static str,
        hits: &[(ParaId, Unit, Unit)],
        with: &str,
        stay: bool,
        fonts: &Fonts,
    ) -> (usize, Outcome) {
        let before = self.selection;
        let edit =
            if with.is_empty() { super::commands::deleting(super::commands::Dir::Forward, false) } else { Command::InsertText(with.to_string()) };
        self.pending = None;
        let mut tx = self.begin(label);
        let (mut count, mut protected, mut caret) = (0, None, None);
        for &(para, a, b) in hits.iter().rev() {
            let Some(slot) = self.index.para_slot(para) else { continue };
            let story = self.index.story(slot.container);
            let len = self.index.para(&self.doc, para).map_or(Unit(0), super::step::para_len);
            if b.0 > len.0 || a >= b {
                continue;
            }
            let (pa, pb) = (Position::text(story, para, a), Position::text(story, para, b));
            self.selection = Selection::range(pa, pb, Granularity::Char);
            if let Some(e) = self.protection_refusal(&edit) {
                protected.get_or_insert(e);
                continue;
            }
            if self.locked_edit(&pa, &pb) || self.expanded_in(&pa, &pb).is_some() {
                continue;
            }
            let done = if with.is_empty() {
                self.delete_range_joining(&mut tx, false).map_err(|_| "stale match").and_then(|p| p.ok_or("no caret"))
            } else {
                self.type_into(&mut tx, with).map(|(p, _)| p)
            };
            match done {
                Ok(p) => {
                    count += 1;
                    caret = Some(p);
                }
                Err(_) => {
                    self.abort(tx);
                    return (0, Outcome::Unchanged("replace failed"));
                }
            }
        }
        if count == 0 {
            self.abort(tx);
            return (0, protected.map_or(Outcome::Unchanged("no matches"), Outcome::Refused));
        }
        let start = before.ordered(&self.index).map(|(a, _)| a).and_then(|a| {
            let (p, u) = a.para_offset()?;
            let len = super::step::para_len(self.index.para(&self.doc, p)?);
            Some(Position::text(a.story, p, Unit(u.0.min(len.0))))
        });
        let at = if stay { start.or(caret) } else { caret };
        if let Some(p) = at {
            self.selection = self.normalize(Selection::Caret(p));
            // Find goes on after the replacement, even where the caret can't be (in a field's result).
            if !stay && let Some((para, u)) = p.para_offset() {
                self.found = Some((self.selection, para, u, u));
            }
        }
        (count, self.commit(tx, None, fonts))
    }
}

#[test]
fn finds_ignoring_case() {
    let h: Vec<char> = "The cat, the CAT.".chars().collect();
    assert_eq!(matches(&h, &"the".chars().collect::<Vec<_>>(), false, false), [0, 9]);
    assert_eq!(matches(&h, &"the".chars().collect::<Vec<_>>(), true, false), [9]);
    assert_eq!(matches(&h, &"cat".chars().collect::<Vec<_>>(), false, false), [4, 13]);
}
