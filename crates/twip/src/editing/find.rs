//! Find and replace

use crate::docx::Block;
use crate::font::Fonts;
use crate::layout::{ParaId, Unit};

use super::Editor;
use super::commands::{Command, Outcome};
use super::position::Position;
use super::selection::{Granularity, Selection};
use super::text::UnitText;

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

    fn chars_of(&self, para: ParaId) -> Vec<char> {
        self.index.para(&self.doc, para).map_or_else(Vec::new, |p| UnitText::of_view(p, self.doc.review.view).text.chars().collect())
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

    pub fn count_matches(&self, needle: &str, case: bool) -> usize {
        let needle: Vec<char> = needle.chars().collect();
        self.body_paras().into_iter().map(|p| matches(&self.chars_of(p), &needle, case).len()).sum()
    }

    pub fn selection_matches(&self, needle: &str, case: bool) -> bool {
        let Some((a, b)) = self.selection.ordered(&self.index) else { return false };
        let (Some((pa, ua)), Some((pb, ub))) = (a.para_offset(), b.para_offset()) else { return false };
        let needle: Vec<char> = needle.chars().collect();
        pa == pb && (ub.0 - ua.0) as usize == needle.len() && matches(&self.chars_of(pa)[ua.0 as usize..ub.0 as usize], &needle, case) == [0]
    }

    #[cfg(test)]
    pub(crate) fn replace_all(&mut self, find: &str, with: &str, case: bool, fonts: &Fonts) -> (usize, Outcome) {
        let needle: Vec<char> = find.chars().collect();
        let hits: Vec<(ParaId, Unit, Unit)> = self
            .body_paras()
            .into_iter()
            .flat_map(|para| {
                let n = needle.len() as u32;
                matches(&self.chars_of(para), &needle, case).into_iter().map(move |h| (para, Unit(h as u32), Unit(h as u32 + n)))
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
