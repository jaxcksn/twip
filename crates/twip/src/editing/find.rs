//! Find and replace

use crate::docx::Block;
use crate::font::Fonts;
use crate::layout::{ParaId, Unit};

use super::Editor;
use super::commands::{Command, Outcome};
use super::position::Position;
use super::search::{Clusters, Piece};
use super::selection::{Granularity, Selection};
use super::text::{Seen, UnitText};

/// Where `needle` occurs in `hay`, without overlaps: `case`, exactly, else ignoring case; `whole`, only where no
/// letter or digit touches either end. A match starts and ends on grapheme boundaries, and a combining mark is part of
/// the letter it follows.
#[cfg(test)]
pub(crate) fn matches(hay: &[char], needle: &[char], case: bool, whole: bool) -> Vec<usize> {
    scan(hay, needle, case, whole, || grapheme_starts(hay), |c, i, end| c.starts(i) && c.starts(end)).0
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
        let review = &self.review;
        let t = UnitText::masked(p, |rev| rev.del.is_some() || review.hides(rev));
        !t.is_hidden(u) && t.char_at(u) == Some(' ')
    }

    /// A paragraph's text as Find reads it: what the markup view shows, fields as they show, values aside.
    pub(crate) fn find_text(&self, para: ParaId) -> Seen {
        let review = &self.review;
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
        let hits: Vec<Span> = self
            .body_paras()
            .into_iter()
            .flat_map(|para| self.unit_hits(para, &needle, case).into_iter().map(move |(a, b)| (para, a, para, b)))
            .collect();
        self.replace_hits("Replace All", &hits, &[Replacement::text(with)], true, None, fonts)
    }

    /// Selects from units `a` of `para` to units `b` of `last`, a later paragraph of its story, and reveals it. Each
    /// end is clamped to its paragraph; paragraphs in different stories, or `last` before `para`, are refused.
    pub fn select_span(&mut self, para: ParaId, a: Unit, last: ParaId, b: Unit, fonts: &Fonts) -> Outcome {
        let (Some(sa), Some(sb)) = (self.index.para_slot(para), self.index.para_slot(last)) else { return Outcome::Unchanged("no paragraph") };
        let story = self.index.story(sa.container);
        if self.index.story(sb.container) != story {
            return Outcome::invalid("the paragraphs are in different stories");
        }
        let len = |p: ParaId| self.index.para(&self.doc, p).map_or(Unit(0), super::step::para_len);
        let (a, b) = (Unit(a.0.min(len(para).0)), Unit(b.0.min(len(last).0)));
        let (pa, pb) = (Position::text(story, para, a), Position::text(story, last, b));
        if self.index.cmp(&pa, &pb) != std::cmp::Ordering::Less {
            return Outcome::invalid("the span ends before it starts");
        }
        self.history.break_typing();
        self.set_selection(Selection::range(pa, pb, Granularity::Char));
        self.ensure(&pb, fonts);
        self.jump_to_caret();
        Outcome::Moved
    }

    /// Where a span of a paragraph's (or paragraphs') units is, or `None` when it's gone or every character the view
    /// shows in it is tracked as deleted (Replace leaves deleted text be). `views` keeps the views of the paragraphs
    /// last read, for spans taken last to first (or first to last with no edit between): the units before an edit
    /// read as they did.
    fn span_at(&self, &(para, a, last, b): &Span, views: &mut Views) -> Option<(Position, Position)> {
        let (sa, sb) = (self.index.para_slot(para)?, self.index.para_slot(last)?);
        let (story, story_b) = (self.index.story(sa.container), self.index.story(sb.container));
        let (la, lb) = (super::step::para_len(self.index.para(&self.doc, para)?), super::step::para_len(self.index.para(&self.doc, last)?));
        if a.0 > la.0 || b.0 > lb.0 || story != story_b || (para == last && a >= b) {
            return None;
        }
        if para == last && self.all_deleted(views, para, a, b) {
            return None;
        }
        if para != last && self.span_deleted(views, para, a, last, b) {
            return None;
        }
        Some((Position::text(story, para, a), Position::text(story, last, b)))
    }

    /// Whether a span across paragraph marks, from units `a` of `para` to units `b` of `last`, is all tracked as
    /// deleted: every mark it crosses, and every character the view shows in it.
    fn span_deleted(&self, views: &mut Views, para: ParaId, a: Unit, last: ParaId, b: Unit) -> bool {
        let (mut p, mut x) = (para, a);
        // A match crosses at most as many marks as Find what has characters.
        for _ in 0..=super::search::MAX_TEXT {
            let Some(q) = self.index.para(&self.doc, p) else { return false };
            if p == last {
                return !self.shows_live(views, p, x, b);
            }
            if q.mark.rev.del.is_none() || self.shows_live(views, p, x, super::step::para_len(q)) {
                return false;
            }
            let Some((n, _)) = crate::services::find::next_para(self, p) else { return false };
            (p, x) = (n, Unit(0));
        }
        false
    }

    /// Whether the view shows a character in units `x..y` of `p` that isn't tracked as deleted.
    fn shows_live(&self, views: &mut Views, p: ParaId, x: Unit, y: Unit) -> bool {
        if x >= y {
            return false;
        }
        let any = |s: &Seen| s.units.get(s.units.partition_point(|&u| u < x.0)).is_some_and(|&u| u < y.0);
        if let Some((_, live)) = self.views_of(views, p) {
            return any(live);
        }
        // Nothing in it is deleted: whatever the view shows is live.
        let view = self.review.view;
        self.index.para(&self.doc, p).is_some_and(|q| any(&Seen::of(q, |r| !view.hides(&r.rev), false)))
    }

    /// Whether the view shows characters in units `x..y` of `p` and all of them are tracked as deleted.
    fn all_deleted(&self, views: &mut Views, p: ParaId, x: Unit, y: Unit) -> bool {
        if self.index.para(&self.doc, p).is_none() {
            return true;
        }
        let Some((all, live)) = self.views_of(views, p) else { return false };
        let any = |s: &Seen| s.units.get(s.units.partition_point(|&u| u < x.0)).is_some_and(|&u| u < y.0);
        any(all) && !any(live)
    }

    /// Paragraph `p`'s views (kept in `views`): all the view shows of it, and that without what is tracked as
    /// deleted; none when nothing in it is.
    fn views_of<'v>(&self, views: &'v mut Views, p: ParaId) -> Option<&'v (Seen, Seen)> {
        let k = match views.iter().position(|v| v.0 == p) {
            Some(k) => k,
            None => {
                let q = self.index.para(&self.doc, p)?;
                let view = self.review.view;
                let read = q
                    .inlines
                    .iter()
                    .any(|i| i.run().is_some_and(|r| r.rev.del.is_some()))
                    .then(|| (Seen::of(q, |r| !view.hides(&r.rev), false), Seen::of(q, |r| !view.hides(&r.rev) && r.rev.del.is_none(), false)));
                if views.len() >= 2 {
                    views.remove(0);
                }
                views.push((p, read));
                views.len() - 1
            }
        };
        views[k].1.as_ref()
    }

    /// `hit` with its end where the replacement of the match after it went (`next`: that match's start and where its
    /// replacement went), when the two met at a paragraph's start and replacing the later one removed that paragraph.
    fn before_next(&self, hit: &Span, next: Option<((ParaId, Unit), Position)>) -> Span {
        match next.and_then(|(start, at)| Some((start, at.para_offset()?))) {
            Some((start, (p, u))) if start == (hit.2, hit.3) && self.index.para_slot(hit.2).is_none() => (hit.0, hit.1, p, u),
            _ => *hit,
        }
    }

    /// Deletes the selection, joining the paragraphs it crosses when `join`: where its replacement goes. Across
    /// paragraph marks, that is where the first mark was (before it, when tracking keeps it as deleted).
    fn delete_replaced(&mut self, tx: &mut super::Tx, join: bool) -> Result<Option<Position>, super::step::StepError> {
        let mark = self.selection.ordered(&self.index).filter(|_| join).and_then(|(a, _)| {
            let (p, _) = a.para_offset()?;
            Some(Position::text(a.story, p, super::step::para_len(self.index.para(&self.doc, p)?)))
        });
        let from = tx.maps.len();
        let at = self.delete_range_joining(tx, join)?;
        Ok(match (at, mark) {
            (Some(_), Some(m)) => Some(tx.maps[from..].iter().fold(m, |p, m| m.apply(p))),
            _ => at,
        })
    }

    /// Whether the span from `pa` to `pb` may be replaced as typing over it would: document protection (the reason,
    /// when it refuses), a locked control or a shown sub-template keep it.
    fn replaceable(&mut self, pa: Position, pb: Position, edit: &Command) -> Result<bool, crate::api::error::Error> {
        self.selection = Selection::range(pa, pb, Granularity::Char);
        if let Some(e) = self.protection_refusal(edit) {
            return Err(e);
        }
        Ok(!(self.locked_edit(&pa, &pb) || self.expanded_in(&pa, &pb).is_some()))
    }

    /// Replaces `hits` (from units of one paragraph to units of it or a later one of its story, each, in document
    /// order) with `with` (one for all, or one each) as one transaction, last to first so earlier offsets hold. Each
    /// is replaced as typing over it would be: where document protection, a locked control or a shown sub-template
    /// keeps the text, it stays; text the markup view shows only as deleted stays too. How many it replaced; refused
    /// when protection kept every one. `stay`: the caret goes back to where the selection started, else it follows
    /// the first. `keep`: the selection afterwards runs from its start to its end as the replacements moved it.
    pub(crate) fn replace_hits(
        &mut self,
        label: &'static str,
        hits: &[Span],
        with: &[Replacement],
        stay: bool,
        keep: Option<(Position, Position)>,
        fonts: &Fonts,
    ) -> (usize, Outcome) {
        let before = self.selection;
        self.pending = None;
        let mut tx = self.begin(label);
        let (mut count, mut protected, mut caret) = (0, None, None);
        let mut views = Views::new();
        let mut next = None;
        for (k, hit) in hits.iter().enumerate().rev() {
            let Some(rep) = with.get(k.min(with.len().saturating_sub(1))) else { continue };
            let typed = rep.typed();
            let hit = &self.before_next(hit, next.take());
            let edit =
                if typed.is_empty() { super::commands::deleting(super::commands::Dir::Forward, false) } else { Command::InsertText(typed.clone()) };
            let Some((pa, pb)) = self.span_at(hit, &mut views) else { continue };
            match self.replaceable(pa, pb, &edit) {
                Err(e) => {
                    protected.get_or_insert(e);
                    continue;
                }
                Ok(false) => continue,
                Ok(true) => {}
            }
            let join = hit.0 != hit.2;
            // The replacement takes the formatting of the text found's first character.
            let run = self.run_in(hit.0, hit.1);
            let done = if typed.is_empty() || join {
                let at = self.delete_replaced(&mut tx, join).map_err(|_| "stale match").and_then(|p| p.ok_or("no caret"));
                next = at.ok().map(|at| ((hit.0, hit.1), at));
                match at {
                    Ok(at) if !typed.is_empty() => {
                        self.selection = Selection::Caret(at);
                        self.pending = run;
                        let typed = self.type_into_with(&mut tx, &typed, true).map(|(p, _)| p);
                        self.pending = None;
                        typed
                    }
                    at => at,
                }
            } else {
                self.pending = run;
                let typed = self.type_into_with(&mut tx, &typed, true).map(|(p, _)| p);
                self.pending = None;
                typed
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
        let keep = keep.map(|(a, b)| (a, tx.maps.iter().fold(b, |p, m| m.apply(p))));
        self.place_after_replace(before, caret, stay, keep);
        (count, self.commit(tx, None, fonts))
    }

    /// The selection after a replace: kept round what it covered (`keep`), back at its start (`stay`), or a caret
    /// after the last replacement made (the first in the document), from which Find goes on.
    fn place_after_replace(&mut self, before: Selection, caret: Option<Position>, stay: bool, keep: Option<(Position, Position)>) {
        if let Some((a, b)) = keep {
            self.selection = self.normalize(Selection::range(a, b, Granularity::Char));
            return;
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
    }

    /// [`Editor::replace_hits`] with replacements that paste (the text found, the clipboard) or keep the text, then
    /// `format` run on each replaced span, as one undo step: each replacement and formatting command an edit of one
    /// batch, the layout settled once after.
    pub(crate) fn replace_formatted(
        &mut self,
        label: &'static str,
        hits: &[Span],
        with: &[Replacement],
        format: &[Command],
        stay: bool,
        keep: Option<(Position, Position)>,
        fonts: &Fonts,
    ) -> (usize, Outcome) {
        let before = self.selection;
        self.pending = None;
        // What each replacement pastes from the text found (all of it, a group's part), as copies, before any is
        // replaced: by piece.
        let mut views = Views::new();
        let mut pastes: Vec<Pastes> = vec![Pastes::default(); hits.len()];
        for (k, hit) in hits.iter().enumerate() {
            let rep = &with[k.min(with.len().saturating_sub(1))];
            if !rep.pieces.iter().any(|p| matches!(p, Piece::Found | Piece::Copy(..))) {
                continue;
            }
            let Some((pa, pb)) = self.span_at(hit, &mut views) else { continue };
            if rep.pieces.contains(&Piece::Found) {
                self.selection = Selection::range(pa, pb, Granularity::Char);
                pastes[k].found = self.clip();
            }
            for piece in &rep.pieces {
                let Piece::Copy(a, b) = *piece else { continue };
                self.selection = Selection::range(Position::text(pa.story, hit.0, a), Position::text(pa.story, hit.0, b), Granularity::Char);
                pastes[k].copies.push(self.clip());
            }
        }
        self.selection = before;
        let clipboard = self.clipboard.clone();
        self.begin_batch(Some(std::borrow::Cow::Borrowed(label)));
        self.deferring = true;
        let (mut count, mut protected, mut caret) = (0, None, None);
        let mut keep = keep;
        views.clear();
        let mut next = None;
        for (k, hit) in hits.iter().enumerate().rev() {
            let rep = &with[k.min(with.len().saturating_sub(1))];
            let hit = &self.before_next(hit, next.take());
            let Some((pa, pb)) = self.span_at(hit, &mut views) else { continue };
            let edit = match format.first() {
                Some(cmd) if rep.keep => cmd.clone(),
                _ => Command::InsertText(String::new()),
            };
            match self.replaceable(pa, pb, &edit) {
                Err(e) => {
                    protected.get_or_insert(e);
                    continue;
                }
                Ok(false) => continue,
                Ok(true) => {}
            }
            let end = if rep.keep {
                pb
            } else {
                let Some((at, end)) = self.insert_replacement(rep, &pastes[k], clipboard.as_ref(), hit.0 != hit.2, &mut keep, fonts) else {
                    continue;
                };
                next = Some(((hit.0, hit.1), at));
                end
            };
            count += 1;
            caret = Some(end);
            if !format.is_empty() && pa != end {
                self.selection = Selection::range(pa, end, Granularity::Char);
                for cmd in format {
                    if matches!(self.execute(cmd.clone(), fonts), Outcome::Changed(_)) {
                        self.map_kept(&mut keep);
                    }
                }
                caret = self.selection.ordered(&self.index).map(|(_, b)| b).or(caret);
            }
        }
        self.deferring = false;
        if count == 0 {
            let _ = self.end_batch(false, fonts);
            return (0, protected.map_or(Outcome::Unchanged("no matches"), Outcome::Refused));
        }
        self.place_after_replace(before, caret, stay, keep);
        let out = self.end_batch(true, fonts);
        self.after_edit(before, fonts);
        (count, out)
    }

    /// The run of the first unit from `u` of `para` that has one.
    fn run_in(&self, para: ParaId, u: Unit) -> Option<crate::docx::Run> {
        let p = self.index.para(&self.doc, para)?;
        let mut at = 0u32;
        for i in &p.inlines {
            let n = i.units() as u32;
            if at + n > u.0
                && n > 0
                && let Some(r) = i.run()
            {
                return Some(r.clone());
            }
            at += n;
        }
        Some(p.mark.clone())
    }

    /// Replaces comments' marks: each comment deleted, `rep` typed where its reference was (`marks`: each comment and
    /// its reference's place, in document order), as one transaction labelled `label`. A mark stays whole where
    /// document protection refuses deleting its comment or typing at its reference, or a locked control or a shown
    /// sub-template keeps the typing. How many; refused when protection kept every one. `stay`: the caret goes back
    /// to where the selection started, else it follows the first replacement.
    pub(crate) fn replace_comment_marks(
        &mut self,
        label: &'static str,
        marks: &[(crate::docx::review::CommentId, ParaId, Unit)],
        rep: &Replacement,
        stay: bool,
        fonts: &Fonts,
    ) -> (usize, Outcome) {
        let before = self.selection;
        let typed = rep.typed();
        let edit = Command::InsertText(typed.clone());
        let mut protected = None;
        let mut kept = Vec::with_capacity(marks.len());
        for &(id, para, u) in marks {
            if !self.doc.comments.by_id.contains_key(&id) {
                continue;
            }
            let Some(story) = self.index.para_slot(para).map(|s| self.index.story(s.container)) else { continue };
            let at = Position::text(story, para, u);
            if let Some(e) = self.protection_refuses_at(crate::api::command::CommandId::DeleteComment, &at, &at) {
                protected.get_or_insert(e);
                continue;
            }
            if !typed.is_empty() {
                match self.replaceable(at, at, &edit) {
                    Err(e) => {
                        protected.get_or_insert(e);
                        continue;
                    }
                    Ok(false) => continue,
                    Ok(true) => {}
                }
            }
            kept.push((id, at));
        }
        self.selection = before;
        let ids: Vec<_> = kept.iter().map(|m| m.0).collect();
        if ids.is_empty() {
            return (0, protected.map_or(Outcome::Unchanged("no matches"), Outcome::Refused));
        }
        let mut tx = self.begin(label);
        let mut caret = None;
        // Typed last to first, so earlier places hold; the markers removed after are zero-width, so none moves.
        for &(_, at) in kept.iter().rev() {
            caret = Some(at);
            if typed.is_empty() {
                continue;
            }
            self.selection = Selection::Caret(at);
            self.pending = at.para_offset().and_then(|(para, u)| self.run_in(para, u));
            let done = self.type_into_with(&mut tx, &typed, true);
            self.pending = None;
            match done {
                Ok((end, _)) => caret = Some(end),
                Err(_) => {
                    self.abort(tx);
                    return (0, Outcome::Unchanged("replace failed"));
                }
            }
        }
        if self.remove_comments(&mut tx, &ids).is_err() {
            self.abort(tx);
            return (0, Outcome::Unchanged("stale comment"));
        }
        self.place_after_replace(before, caret, stay, None);
        match self.commit(tx, None, fonts) {
            out @ Outcome::Changed(_) => (ids.len(), out),
            out => (0, out),
        }
    }

    /// Moves the end of a selection kept round a replace through the edit just made.
    fn map_kept(&self, keep: &mut Option<(Position, Position)>) {
        if let Some((_, b)) = keep {
            *b = self.last_maps.iter().fold(*b, |p, m| m.apply(p));
        }
    }

    /// Types `text` at `at` as its own edit of the batch open; where it ends, `None` when it couldn't.
    fn type_at(
        &mut self,
        at: Position,
        text: &str,
        run: Option<&crate::docx::Run>,
        keep: &mut Option<(Position, Position)>,
        fonts: &Fonts,
    ) -> Option<Position> {
        if text.is_empty() {
            return Some(at);
        }
        self.selection = Selection::Caret(at);
        let mut tx = self.begin("Replace");
        self.pending = run.cloned();
        let typed = self.type_into_with(&mut tx, text, true);
        self.pending = None;
        match typed {
            Ok((p, _)) => {
                if matches!(self.commit(tx, None, fonts), Outcome::Changed(_)) {
                    self.map_kept(keep);
                }
                Some(p)
            }
            Err(_) => {
                self.abort(tx);
                None
            }
        }
    }

    /// Replaces the selection with `rep`'s pieces, each its own edit of the batch open: typed text, paragraph marks
    /// and breaks at once, the copies of the text found (`pastes`) and the `clipboard` pasted. Where the replacement
    /// starts and ends; `None`, every edit of it undone, when a piece couldn't be put in. `join`: the selection
    /// crosses a paragraph mark, which deleting it joins.
    fn insert_replacement(
        &mut self,
        rep: &Replacement,
        pastes: &Pastes,
        clipboard: Option<&super::clipboard::Fragment>,
        join: bool,
        keep: &mut Option<(Position, Position)>,
        fonts: &Fonts,
    ) -> Option<(Position, Position)> {
        let kept = *keep;
        self.begin_batch(None);
        let done = self.replacement_pieces(rep, pastes, clipboard, join, keep, fonts);
        let _ = self.end_batch(done.is_some(), fonts);
        if done.is_none() {
            *keep = kept;
        }
        done
    }

    /// [`Editor::insert_replacement`]'s edits, in the batch it opens.
    fn replacement_pieces(
        &mut self,
        rep: &Replacement,
        pastes: &Pastes,
        clipboard: Option<&super::clipboard::Fragment>,
        join: bool,
        keep: &mut Option<(Position, Position)>,
        fonts: &Fonts,
    ) -> Option<(Position, Position)> {
        let run = self.selection.ordered(&self.index).and_then(|(a, _)| a.para_offset()).and_then(|(p, u)| self.run_in(p, u));
        let mut tx = self.begin("Replace");
        let at = match self.delete_replaced(&mut tx, join) {
            Ok(Some(p)) => p,
            _ => {
                self.abort(tx);
                return None;
            }
        };
        self.selection = Selection::Caret(at);
        if !tx.undo.is_empty() && matches!(self.commit(tx, None, fonts), Outcome::Changed(_)) {
            self.map_kept(keep);
        }
        let mut end = at;
        let mut typed = String::new();
        let mut copies = pastes.copies.iter();
        for piece in &rep.pieces {
            let paste = match piece {
                Piece::Found => pastes.found.as_ref(),
                Piece::Copy(..) => copies.next().and_then(Option::as_ref),
                Piece::Clipboard => clipboard,
                other => {
                    piece_text(other, &mut typed);
                    continue;
                }
            };
            end = self.type_at(end, &std::mem::take(&mut typed), run.as_ref(), keep, fonts)?;
            if let Some(f) = paste {
                self.selection = Selection::Caret(end);
                self.pasting_replacement = true;
                let pasted = self.paste_fragment(f, fonts);
                self.pasting_replacement = false;
                if !matches!(pasted, Outcome::Changed(_)) {
                    return None;
                }
                self.map_kept(keep);
                end = self.caret().unwrap_or(end);
            }
        }
        Some((at, self.type_at(end, &typed, run.as_ref(), keep, fonts)?))
    }
}

impl Editor {
    /// Highlights `pieces` (paragraph unit ranges, as find matches cover them; one past its paragraph's end takes its
    /// mark too) on the pages, as Word's Highlight All does, without changing the document: until it next changes or
    /// [`Editor::clear_find_highlights`].
    pub fn set_find_highlights(&mut self, pieces: impl IntoIterator<Item = (ParaId, Unit, Unit)>) {
        let mut by: std::collections::HashMap<ParaId, Vec<(Unit, Unit)>> = std::collections::HashMap::new();
        for (p, a, b) in pieces.into_iter().filter(|r| r.1 < r.2) {
            by.entry(p).or_default().push((a, b));
        }
        // Overlapping ranges merged: each paragraph's are disjoint, in order.
        for v in by.values_mut() {
            v.sort_unstable();
            let mut n = 0;
            for k in 0..v.len() {
                if n > 0 && v[k].0 < v[n - 1].1 {
                    v[n - 1].1 = v[n - 1].1.max(v[k].1);
                } else {
                    v[n] = v[k];
                    n += 1;
                }
            }
            v.truncate(n);
        }
        self.find_highlights = Some((self.revision, by));
    }

    /// Removes the find highlights.
    pub fn clear_find_highlights(&mut self) {
        self.find_highlights = None;
    }

    /// The find highlights, by paragraph, while the document is as they were found in.
    pub fn find_highlights(&self) -> Option<&std::collections::HashMap<ParaId, Vec<(Unit, Unit)>>> {
        self.find_highlights.as_ref().filter(|(rev, _)| *rev == self.revision).map(|(_, by)| by)
    }

    /// The find highlights' rectangles on `pages`, one per line part of each, in page points.
    pub fn find_highlight_bands(&mut self, pages: std::ops::Range<usize>) -> Vec<(usize, super::view::Band)> {
        if self.find_highlights.as_ref().is_some_and(|(rev, _)| *rev != self.revision) {
            self.find_highlights = None;
        }
        let Some((_, by)) = &self.find_highlights else { return Vec::new() };
        let mut out = Vec::new();
        for page in pages {
            let Some(geo) = self.pages.page(page, &self.index) else { continue };
            for l in &geo.lines {
                for &para in l.paras.iter() {
                    let Some(ranges) = by.get(&para) else { continue };
                    let (Some(lf), Some(ll)) = (l.first_of(para), l.last_of(para)) else { continue };
                    let shown_end = l.stops.last().is_some_and(|s| s.end && s.para == para);
                    let first = ranges.partition_point(|r| r.1 <= lf);
                    for &(a, b) in ranges[first..].iter().take_while(|r| r.0 < ll || (r.0 == ll && shown_end)) {
                        let (from, to) = (a.max(lf), b.min(ll));
                        let (Some(x0), Some(x1)) = (l.x_of(para, from), l.x_of(para, to)) else { continue };
                        // The paragraph's mark, as a selection across it shows it.
                        let x1 = x1 + if b > ll && shown_end { 4.0 } else { 0.0 };
                        if (x1 - x0).abs() > 0.01 {
                            out.push((page, l.strip(x0.min(x1), x0.max(x1))));
                        }
                    }
                }
            }
        }
        out
    }
}

/// A paragraph's own tab stops (its style's and its own, cleared ones left out).
pub fn tab_stops_of(p: &crate::docx::Paragraph) -> Vec<crate::api::services::TabStopInfo> {
    p.style.tabs.iter().filter(|t| !t.is_clear()).map(|t| super::commands::tabs::info(t, false)).collect()
}

/// A paragraph's frame, when it is in one.
pub fn frame_of(p: &crate::docx::Paragraph) -> Option<crate::api::command::FrameProperties> {
    p.style.frame.as_ref().map(super::frames::properties)
}

/// A span of units: from units of one paragraph to units of it or a later one of its story.
pub type Span = (ParaId, Unit, ParaId, Unit);

/// The views of the paragraphs a replace read last: each with all its text the markup view shows, and that without
/// what is tracked as deleted (none when nothing in it is). At most two (a span's first and last paragraph).
type Views = Vec<(ParaId, Option<(Seen, Seen)>)>;

/// What one replacement pastes from its match, copied before any match is replaced: the text found (once, for every
/// [`Piece::Found`]) and each [`Piece::Copy`] in order.
#[derive(Clone, Default)]
struct Pastes {
    found: Option<super::clipboard::Fragment>,
    copies: Vec<Option<super::clipboard::Fragment>>,
}

/// What a match is replaced with: its pieces, or (`keep`) the text found kept and only formatted.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Replacement {
    /// What goes in, in order.
    pub pieces: Vec<Piece>,
    /// The text found stays, only formatted.
    pub keep: bool,
}

impl Replacement {
    /// Plain text (tabs and line feeds as typing them makes them).
    pub fn text(s: &str) -> Replacement {
        Replacement { pieces: if s.is_empty() { Vec::new() } else { vec![Piece::Text(s.to_string())] }, keep: false }
    }

    /// Typed in one go: text, paragraph marks and breaks only, nothing kept or pasted.
    pub fn is_typed(&self) -> bool {
        !self.keep && !self.pieces.iter().any(|p| matches!(p, Piece::Found | Piece::Copy(..) | Piece::Clipboard))
    }

    /// The text that types the pieces: paragraph marks as line feeds, breaks as their control characters.
    fn typed(&self) -> String {
        let mut out = String::new();
        self.pieces.iter().for_each(|p| piece_text(p, &mut out));
        out
    }
}

/// What typing `p` takes, onto `out` ([`Editor::type_into_with`]'s breaks); the text found and the clipboard paste.
fn piece_text(p: &Piece, out: &mut String) {
    match p {
        Piece::Text(s) => out.push_str(s),
        Piece::Paragraph => out.push('\n'),
        Piece::Break(k) => out.push(match k {
            crate::docx::BreakKind::Page => '\u{c}',
            crate::docx::BreakKind::Column => '\u{e}',
            _ => '\u{b}',
        }),
        Piece::Found | Piece::Copy(..) | Piece::Clipboard => {}
    }
}

#[test]
fn finds_ignoring_case() {
    let h: Vec<char> = "The cat, the CAT.".chars().collect();
    assert_eq!(matches(&h, &"the".chars().collect::<Vec<_>>(), false, false), [0, 9]);
    assert_eq!(matches(&h, &"the".chars().collect::<Vec<_>>(), true, false), [9]);
    assert_eq!(matches(&h, &"cat".chars().collect::<Vec<_>>(), false, false), [4, 13]);
}
