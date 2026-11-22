//! Find and replace over every story's paragraphs (the body's first, tables included) in document order, on the
//! text the markup view shows: text it hides, and field codes, match nothing. Matches are unit ranges of one
//! paragraph.

use crate::api::services::{FindMatch, FindQuery};
use crate::api::value::MatchId;
use crate::editing::Editor;
use crate::editing::commands::Outcome;
use crate::font::Fonts;
use crate::layout::{ParaId, Unit};

use super::spelling::searched_paras;

/// A match: its paragraph and units `start..end`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hit {
    pub para: ParaId,
    pub start: u32,
    pub end: u32,
}

impl Hit {
    /// The match's public id: `paragraph:start:end`.
    pub fn id(&self) -> MatchId {
        MatchId(format!("{}:{}:{}", self.para.0, self.start, self.end))
    }

    pub fn parse(id: &MatchId) -> Option<Hit> {
        let mut parts = id.0.split(':').map(str::parse::<u64>);
        let (Some(Ok(p)), Some(Ok(start)), Some(Ok(end)), None) = (parts.next(), parts.next(), parts.next(), parts.next()) else { return None };
        let (start, end) = (u32::try_from(start).ok()?, u32::try_from(end).ok()?);
        (start < end).then_some(Hit { para: ParaId(p), start, end })
    }

    /// Selects the match (and reveals it).
    pub fn select(&self, e: &mut Editor, fonts: &Fonts) -> Outcome {
        e.select_match(self.para, Unit(self.start), Unit(self.end), fonts)
    }
}

fn hits_in(e: &Editor, para: ParaId, q: &FindQuery) -> Vec<Hit> {
    let needle: Vec<char> = q.text.chars().collect();
    crate::editing::find::hits(&e.find_text(para), &needle, q.match_case.unwrap_or(false), q.whole_words.unwrap_or(false))
        .into_iter()
        .map(|(a, b)| Hit { para, start: a.0, end: b.0 })
        .collect()
}

/// Every match, in document order. A field showing its result is searched in its result only.
pub fn all(e: &Editor, q: &FindQuery) -> Vec<Hit> {
    searched_paras(e).into_iter().flat_map(|p| hits_in(e, p, q)).collect()
}

/// The next match after the selection (before it, `back`), wrapping round the document, and whether it wrapped.
pub fn next(e: &Editor, q: &FindQuery, back: bool) -> Option<(Hit, bool)> {
    let order = searched_paras(e);
    if order.is_empty() || q.text.is_empty() {
        return None;
    }
    let (a, b) = match e.selected_match() {
        Some((para, a, b)) => ((para, a), (para, b)),
        None => e
            .selection()
            .ordered(e.index())
            .and_then(|(a, b)| Some((a.para_offset()?, b.para_offset()?)))
            .unwrap_or(((order[0], Unit(0)), (order[0], Unit(0)))),
    };
    let (from_para, from) = if back { a } else { b };
    let n = order.len();
    // A selection outside the paragraphs searched (a header, a note, a text box): from the top, or the end going back.
    let (start, inside) = match order.iter().position(|p| *p == from_para) {
        Some(k) => (k, true),
        None => (if back { n - 1 } else { 0 }, false),
    };
    // One more than the paragraphs: the start paragraph again, for the matches on the other side of the selection.
    for k in 0..=n {
        let i = if back { (start + n * 2 - k) % n } else { (start + k) % n };
        let hits = hits_in(e, order[i], q);
        let first = k == 0 && inside;
        let hit =
            if back { hits.into_iter().rev().find(|h| !first || h.end <= from.0) } else { hits.into_iter().find(|h| !first || h.start >= from.0) };
        if let Some(h) = hit {
            let wrapped = (k == n && inside) || if back { i > start } else { i < start };
            return Some((h, wrapped));
        }
    }
    None
}

/// The match the selection is on: the find match selected last while it stays selected, else one exactly selected.
pub fn at_selection(e: &Editor, q: &FindQuery) -> Option<Hit> {
    if let Some((para, a, b)) = e.selected_match()
        && let Some(h) = hits_in(e, para, q).into_iter().find(|h| (h.start, h.end) == (a.0, b.0))
    {
        return Some(h);
    }
    let (a, b) = e.selection().ordered(e.index())?;
    let ((pa, ua), (pb, ub)) = (a.para_offset()?, b.para_offset()?);
    (pa == pb).then(|| hits_in(e, pa, q).into_iter().find(|h| (h.start, h.end) == (ua.0, ub.0))).flatten()
}

/// A match for a results list: up to 30 characters of context on each side, from its paragraph.
pub fn describe(e: &Editor, h: Hit, page: u32) -> FindMatch {
    let seen = e.find_text(h.para);
    let chars = &seen.chars;
    let clean = |s: &[char]| s.iter().map(|&c| if c.is_control() || c == crate::editing::text::OBJECT { ' ' } else { c }).collect::<String>();
    let (s, t) = (seen.units.partition_point(|&u| u < h.start), seen.units.partition_point(|&u| u < h.end));
    FindMatch {
        id: h.id(),
        before: clean(&chars[s.saturating_sub(30)..s]),
        text: clean(&chars[s..t]),
        after: clean(&chars[t..(t + 30).min(chars.len())]),
        page,
    }
}

/// Replaces `hits` with `with` as one undo step ([`Editor::replace_hits`]): how many. `all`: Replace All, which leaves
/// the caret where it was; else the caret follows the replacement.
pub fn replace(e: &mut Editor, hits: &[Hit], with: &str, all: bool, fonts: &Fonts) -> (usize, Outcome) {
    let hits: Vec<(ParaId, Unit, Unit)> = hits.iter().map(|h| (h.para, Unit(h.start), Unit(h.end))).collect();
    e.replace_hits(if all { "Replace All" } else { "Replace" }, &hits, with, all, fonts)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(hay: &str, needle: &str, case: bool, whole: bool) -> Vec<usize> {
        crate::editing::find::matches(&hay.chars().collect::<Vec<_>>(), &needle.chars().collect::<Vec<_>>(), case, whole)
    }

    #[test]
    fn matches_by_case_and_whole_words() {
        assert_eq!(at("The cat, the CAT.", "the", false, false), [0, 9]);
        assert_eq!(at("The cat, the CAT.", "the", true, false), [9]);
        assert_eq!(at("cat concat cats cat", "cat", false, true), [0, 16]);
    }

    #[test]
    fn match_ids_round_trip() {
        let h = Hit { para: ParaId(7), start: 3, end: 8 };
        assert_eq!(Hit::parse(&h.id()), Some(h));
        assert_eq!(Hit::parse(&MatchId("7:3".into())), None);
        assert_eq!(Hit::parse(&MatchId("7:8:3".into())), None);
    }
}
