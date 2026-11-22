//! Find and replace

use crate::docx::review::MarkupView;
use crate::docx::{Block, Paragraph};
use crate::font::Fonts;
use crate::layout::{ParaId, Unit};

use super::Editor;
use super::commands::{Command, Outcome};
use super::position::Position;
use super::selection::{Granularity, Selection};
use super::text::UnitText;

/// Where `needle` occurs in `hay`, without overlaps: `case`, exactly, else ignoring case; `whole`, only where no
/// letter or digit touches either end. A match starts and ends on grapheme boundaries, and a combining mark is part of
/// the letter it follows.
pub(crate) fn matches(hay: &[char], needle: &[char], case: bool, whole: bool) -> Vec<usize> {
    if needle.is_empty() || hay.len() < needle.len() {
        return Vec::new();
    }
    let eq = |a: char, b: char| if case { a == b } else { a == b || a.to_lowercase().eq(b.to_lowercase()) };
    let mut starts: Option<Vec<bool>> = None;
    let mut out = Vec::new();
    let mut i = 0;
    while i + needle.len() <= hay.len() {
        let end = i + needle.len();
        if hay[i..end].iter().zip(needle).all(|(a, b)| eq(*a, *b)) {
            let b = starts.get_or_insert_with(|| grapheme_starts(hay));
            let word = |k: usize| hay[k].is_alphanumeric();
            // The grapheme before the match, by its first character.
            let before = || (0..i).rev().find(|&k| b[k]).is_some_and(word);
            if b[i] && b[end] && (!whole || (!(i > 0 && before()) && !(end < hay.len() && word(end)))) {
                out.push(i);
                i = end;
                continue;
            }
        }
        i += 1;
    }
    out
}

/// Where `needle` occurs in `p` as `view` shows it (see [`matches`]), leaving out the codes of fields that show their
/// results: a match there would rewrite a link's target.
pub(crate) fn para_matches(p: &Paragraph, view: MarkupView, needle: &[char], case: bool, whole: bool) -> Vec<usize> {
    let chars: Vec<char> = UnitText::of_view(p, view).text.chars().collect();
    let mut hits = matches(&chars, needle, case, whole);
    let codes = crate::docx::fields::hidden_codes(&p.inlines);
    if !codes.is_empty() {
        let n = needle.len() as u32;
        hits.retain(|&h| !codes.iter().any(|&(b, e)| h as u32 <= e && h as u32 + n > b));
    }
    hits
}

/// Whether each character of `hay` starts a grapheme, then the end.
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

impl Editor {
    fn body_paras(&self) -> Vec<ParaId> {
        fn walk(blocks: &[Block], out: &mut Vec<ParaId>) {
            for b in blocks {
                match b {
                    Block::Para(p) => out.push(ParaId(p.id)),
                    Block::Table(t) => t.rows.iter().flat_map(|r| r.cells.iter()).for_each(|c| walk(&c.blocks, out)),
                }
            }
        }
        let mut out = Vec::new();
        if let Some(part) = self.editing_sub_template().and_then(|k| self.doc.glossary.as_ref()?.parts.get(k as usize)) {
            walk(&part.body, &mut out);
            return out;
        }
        for s in &self.doc.sections {
            walk(&s.blocks, &mut out);
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

    fn chars_of(&self, para: ParaId) -> Vec<char> {
        self.index.para(&self.doc, para).map_or_else(Vec::new, |p| UnitText::of_view(p, self.doc.review.view).text.chars().collect())
    }

    fn matches_in(&self, para: ParaId, needle: &[char], case: bool) -> Vec<usize> {
        self.index.para(&self.doc, para).map_or_else(Vec::new, |p| para_matches(p, self.doc.review.view, needle, case, false))
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
            let hits = self.matches_in(p, &needle, case);
            let hit = if back {
                hits.iter().rev().copied().find(|&h| k > 0 || p != from_para || h + needle.len() <= from.0 as usize)
            } else {
                hits.iter().copied().find(|&h| k > 0 || p != from_para || h >= from.0 as usize)
            };
            if let Some(h) = hit {
                return Some((p, Unit(h as u32), Unit((h + needle.len()) as u32)));
            }
        }
        None
    }

    pub fn count_matches(&self, needle: &str, case: bool) -> usize {
        let needle: Vec<char> = needle.chars().collect();
        self.body_paras().into_iter().map(|p| self.matches_in(p, &needle, case).len()).sum()
    }

    pub fn selection_matches(&self, needle: &str, case: bool) -> bool {
        let Some((a, b)) = self.selection.ordered(&self.index) else { return false };
        let (Some((pa, ua)), Some((pb, ub))) = (a.para_offset(), b.para_offset()) else { return false };
        let needle: Vec<char> = needle.chars().collect();
        pa == pb && (ub.0 - ua.0) as usize == needle.len() && matches(&self.chars_of(pa)[ua.0 as usize..ub.0 as usize], &needle, case, false) == [0]
    }

    #[cfg(test)]
    pub(crate) fn replace_all(&mut self, find: &str, with: &str, case: bool, fonts: &Fonts) -> (usize, Outcome) {
        let needle: Vec<char> = find.chars().collect();
        let hits: Vec<(ParaId, Unit, Unit)> = self
            .body_paras()
            .into_iter()
            .flat_map(|para| {
                let n = needle.len() as u32;
                self.matches_in(para, &needle, case).into_iter().map(move |h| (para, Unit(h as u32), Unit(h as u32 + n)))
            })
            .collect();
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
                    self.selection = before;
                    self.abort(tx);
                    return (0, Outcome::Unchanged("replace failed"));
                }
            }
        }
        if count == 0 {
            self.selection = before;
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
