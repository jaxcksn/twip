//! Find and replace over the body's paragraphs (tables included) in document order, on the text the markup view
//! shows: text it hides matches nothing. Matches are unit ranges of one paragraph.

use crate::api::services::{FindMatch, FindQuery};
use crate::api::value::MatchId;
use crate::editing::Editor;
use crate::editing::commands::{Command, Dir, Outcome, deleting};
use crate::editing::text::UnitText;
use crate::font::Fonts;
use crate::layout::{ParaId, Unit};

use super::spelling::paras_in_order;

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
        e.select_range(self.para, Unit(self.start), Unit(self.end), fonts)
    }
}

/// Where `needle` occurs in `hay`, without overlaps; `case`: exact, else ignoring case; `whole`: only where no
/// letter or digit touches either end.
fn matches(hay: &[char], needle: &[char], case: bool, whole: bool) -> Vec<usize> {
    if needle.is_empty() || hay.len() < needle.len() {
        return Vec::new();
    }
    let eq = |a: char, b: char| if case { a == b } else { a == b || a.to_lowercase().eq(b.to_lowercase()) };
    let word = |i: usize| hay.get(i).is_some_and(|c| c.is_alphanumeric());
    let mut out = Vec::new();
    let mut i = 0;
    while i + needle.len() <= hay.len() {
        let end = i + needle.len();
        if hay[i..end].iter().zip(needle).all(|(a, b)| eq(*a, *b)) && (!whole || !(i > 0 && word(i - 1)) && !word(end)) {
            out.push(i);
            i = end;
        } else {
            i += 1;
        }
    }
    out
}

fn chars_of(e: &Editor, para: ParaId) -> Vec<char> {
    e.index().para(e.document(), para).map_or_else(Vec::new, |p| UnitText::of_view(p, e.document().review.view).text.chars().collect())
}

fn hits_in(e: &Editor, para: ParaId, q: &FindQuery) -> Vec<Hit> {
    let needle: Vec<char> = q.text.chars().collect();
    let n = needle.len() as u32;
    matches(&chars_of(e, para), &needle, q.match_case.unwrap_or(false), q.whole_words.unwrap_or(false))
        .into_iter()
        .map(|h| Hit { para, start: h as u32, end: h as u32 + n })
        .collect()
}

/// Every match, in document order.
pub fn all(e: &Editor, q: &FindQuery) -> Vec<Hit> {
    paras_in_order(e).into_iter().flat_map(|p| hits_in(e, p, q)).collect()
}

/// The next match after the selection (before it, `back`), wrapping round the document, and whether it wrapped.
pub fn next(e: &Editor, q: &FindQuery, back: bool) -> Option<(Hit, bool)> {
    let order = paras_in_order(e);
    if order.is_empty() || q.text.is_empty() {
        return None;
    }
    let (a, b) = e
        .selection()
        .ordered(e.index())
        .and_then(|(a, b)| Some((a.para_offset()?, b.para_offset()?)))
        .unwrap_or(((order[0], Unit(0)), (order[0], Unit(0))));
    let (from_para, from) = if back { a } else { b };
    let start = order.iter().position(|p| *p == from_para).unwrap_or(0);
    let n = order.len();
    // One more than the paragraphs: the start paragraph again, for the matches on the other side of the selection.
    for k in 0..=n {
        let i = if back { (start + n * 2 - k) % n } else { (start + k) % n };
        let hits = hits_in(e, order[i], q);
        let first = k == 0;
        let hit =
            if back { hits.into_iter().rev().find(|h| !first || h.end <= from.0) } else { hits.into_iter().find(|h| !first || h.start >= from.0) };
        if let Some(h) = hit {
            let wrapped = k == n || if back { i > start } else { i < start };
            return Some((h, wrapped));
        }
    }
    None
}

/// The match the selection is exactly on.
pub fn at_selection(e: &Editor, q: &FindQuery) -> Option<Hit> {
    let (a, b) = e.selection().ordered(e.index())?;
    let ((pa, ua), (pb, ub)) = (a.para_offset()?, b.para_offset()?);
    (pa == pb).then(|| hits_in(e, pa, q).into_iter().find(|h| (h.start, h.end) == (ua.0, ub.0))).flatten()
}

/// A match for a results list: up to 30 characters of context on each side, from its paragraph.
pub fn describe(e: &Editor, h: Hit, page: u32) -> FindMatch {
    let chars = chars_of(e, h.para);
    let clean = |s: &[char]| s.iter().map(|&c| if c.is_control() || c == crate::editing::text::OBJECT { ' ' } else { c }).collect::<String>();
    let (s, t) = (h.start as usize, (h.end as usize).min(chars.len()));
    FindMatch {
        id: h.id(),
        before: clean(&chars[s.saturating_sub(30)..s]),
        text: clean(&chars[s..t]),
        after: clean(&chars[t..(t + 30).min(chars.len())]),
        page,
    }
}

/// Replaces `hits` with `with`, last to first so earlier offsets hold, as one undo step.
pub fn replace(e: &mut Editor, hits: &[Hit], with: &str, fonts: &Fonts) -> Outcome {
    e.joined("Replace", fonts, |e| {
        let mut out = Outcome::Unchanged("no match");
        for h in hits.iter().rev() {
            h.select(e, fonts);
            let edit = if with.is_empty() { deleting(Dir::Forward, false) } else { Command::InsertText(with.to_string()) };
            if let o @ Outcome::Changed(_) = e.execute(edit, fonts) {
                out = o;
            }
        }
        out
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(hay: &str, needle: &str, case: bool, whole: bool) -> Vec<usize> {
        matches(&hay.chars().collect::<Vec<_>>(), &needle.chars().collect::<Vec<_>>(), case, whole)
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
