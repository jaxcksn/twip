//! Find and Replace: matches in the body's paragraphs (tables included) in document order, as unit ranges;
//! Replace All as one undo step.

use crate::docx::{Block, Inline};
use crate::font::Fonts;
use crate::layout::{ParaId, Unit};

use super::Editor;
use super::commands::Outcome;
use super::step::Step;
use super::text::UnitText;

/// Where `needle` occurs in `hay` (chars); `case`: exact, else ignoring case.
fn matches(hay: &[char], needle: &[char], case: bool) -> Vec<usize> {
    if needle.is_empty() || hay.len() < needle.len() {
        return Vec::new();
    }
    let eq = |a: char, b: char| if case { a == b } else { a == b || a.to_lowercase().eq(b.to_lowercase()) };
    let mut out = Vec::new();
    let mut i = 0;
    while i + needle.len() <= hay.len() {
        if hay[i..i + needle.len()].iter().zip(needle).all(|(a, b)| eq(*a, *b)) {
            out.push(i);
            i += needle.len();
        } else {
            i += 1;
        }
    }
    out
}

impl Editor {
    /// The body's paragraphs in document order.
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
        for s in &self.doc.sections {
            walk(&s.blocks, &mut out);
        }
        out
    }

    fn chars_of(&self, para: ParaId) -> Vec<char> {
        self.index.para(&self.doc, para).map_or_else(Vec::new, |p| UnitText::of(p).text.chars().collect())
    }

    /// The next match of `needle` after the selection (before it, `back`), wrapping round the document: its
    /// paragraph and units.
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
        let start = order.iter().position(|p| *p == from_para).unwrap_or(0);
        let n = order.len();
        for k in 0..=n {
            let idx = if back { (start + n * 2 - k) % n } else { (start + k) % n };
            let p = order[idx];
            let hits = matches(&self.chars_of(p), &needle, case);
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

    /// How many matches the body has.
    pub fn count_matches(&self, needle: &str, case: bool) -> usize {
        let needle: Vec<char> = needle.chars().collect();
        self.body_paras().into_iter().map(|p| matches(&self.chars_of(p), &needle, case).len()).sum()
    }

    /// Whether the selection is exactly a match of `needle` (what Replace replaces).
    pub fn selection_matches(&self, needle: &str, case: bool) -> bool {
        let Some((a, b)) = self.selection.ordered(&self.index) else { return false };
        let (Some((pa, ua)), Some((pb, ub))) = (a.para_offset(), b.para_offset()) else { return false };
        let needle: Vec<char> = needle.chars().collect();
        pa == pb && (ub.0 - ua.0) as usize == needle.len() && matches(&self.chars_of(pa)[ua.0 as usize..ub.0 as usize], &needle, case) == [0]
    }

    /// Replace All: every match in the body replaced by `with` in the matched text's first run, one undo step.
    pub(super) fn replace_all(&mut self, find: &str, with: &str, case: bool, fonts: &Fonts) -> (usize, Outcome) {
        let needle: Vec<char> = find.chars().collect();
        let mut tx = self.begin("Replace All");
        let mut count = 0;
        for para in self.body_paras() {
            let hits = matches(&self.chars_of(para), &needle, case);
            // Last to first: each replacement keeps the earlier ones' offsets.
            for &h in hits.iter().rev() {
                let Some(p) = self.index.para(&self.doc, para) else { continue };
                // The run of the match's first character.
                let mut at = 0usize;
                let run = p.inlines.iter().find_map(|i| {
                    let n = i.units();
                    let r = (at + n > h && n > 0).then(|| i.run().cloned()).flatten();
                    at += n;
                    r
                });
                let Some(run) = run.or_else(|| Some(p.mark.clone())) else { continue };
                let inl = if with.is_empty() { Vec::new() } else { vec![Inline::Text(with.to_string(), run)] };
                let range = Unit(h as u32)..Unit((h + needle.len()) as u32);
                if self.step(&mut tx, Step::Text { para, range, with: inl, edges: Default::default() }).is_err() {
                    self.abort(tx);
                    return (0, Outcome::Unchanged("replace failed"));
                }
                count += 1;
            }
        }
        if count == 0 {
            self.abort(tx);
            return (0, Outcome::Unchanged("no matches"));
        }
        // The caret goes to where the selection started, on what is there now.
        if let Some((a, _)) = self.selection.ordered(&self.index)
            && let Some((p, u)) = a.para_offset()
        {
            let len = self.index.para(&self.doc, p).map_or(Unit(0), super::step::para_len);
            self.selection = self.normalize(super::selection::Selection::Caret(super::position::Position::text(a.story, p, Unit(u.0.min(len.0)))));
        }
        (count, self.commit(tx, None, fonts))
    }
}

#[test]
fn finds_ignoring_case() {
    let h: Vec<char> = "The cat, the CAT.".chars().collect();
    assert_eq!(matches(&h, &"the".chars().collect::<Vec<_>>(), false), [0, 9]);
    assert_eq!(matches(&h, &"the".chars().collect::<Vec<_>>(), true), [9]);
    assert_eq!(matches(&h, &"cat".chars().collect::<Vec<_>>(), false), [4, 13]);
}
