//! Text boundaries

use unicode_segmentation::UnicodeSegmentation;

use crate::docx::review::{MarkupView, RunRevisions};
use crate::docx::{BreakKind, Inline, Paragraph};
use crate::layout::Unit;

pub const OBJECT: char = '\u{FFFC}';

pub struct UnitText {
    pub text: String,
    bytes: Vec<usize>,
    hidden: Vec<bool>,
    atoms: Vec<(Unit, Unit)>,
}

impl UnitText {
    pub fn of(p: &Paragraph) -> UnitText {
        UnitText::masked(p, |_| false)
    }

    pub fn of_view(p: &Paragraph, view: MarkupView) -> UnitText {
        UnitText::masked(p, |rev| view.hides(rev))
    }

    pub fn masked(p: &Paragraph, mask: impl Fn(&RunRevisions) -> bool) -> UnitText {
        let mut text = String::new();
        let mut bytes = Vec::new();
        let mut hidden = Vec::new();
        for i in &p.inlines {
            let masked = i.run().is_some_and(|r| mask(&r.rev));
            let h = masked || i.run().is_some_and(|r| r.hidden);
            let mut push = |c: char, text: &mut String| {
                bytes.push(text.len());
                hidden.push(h);
                text.push(if masked { ' ' } else { c });
            };
            match i {
                Inline::Text(s, _) => s.chars().for_each(|c| push(c, &mut text)),
                Inline::Tab(_) | Inline::PTab(..) => push('\t', &mut text),
                Inline::Break(BreakKind::Line, _) => push('\u{2028}', &mut text),
                Inline::Break(..) => push('\u{c}', &mut text),
                Inline::Sym { ch, .. } => push(*ch, &mut text),
                i if i.units() == 1 => push(OBJECT, &mut text),
                _ => {}
            }
        }
        bytes.push(text.len());
        let mut atoms: Vec<(Unit, Unit)> = Vec::new();
        let starts: Vec<u32> = p.inlines.iter().scan(0u32, |at, i| Some(std::mem::replace(at, *at + i.units() as u32))).collect();
        for s in crate::docx::fields::spans(&p.inlines) {
            let (a, b) = (Unit(starts[s.begin]), Unit(starts[s.end] + 1));
            if atoms.last().is_some_and(|l| l.1 > a) {
                continue;
            }
            match &p.inlines[s.begin] {
                Inline::FieldBegin { view: crate::docx::FieldView::Code, .. } => {
                    for u in starts[s.code_end()]..starts[s.end] {
                        if let Some(h) = hidden.get_mut(u as usize) {
                            *h = true;
                        }
                    }
                }
                _ => atoms.push((a, b)),
            }
        }
        UnitText { text, bytes, hidden, atoms }
    }

    fn atom_at(&self, u: Unit) -> Option<(Unit, Unit)> {
        let k = self.atoms.partition_point(|a| a.1 <= u);
        self.atoms.get(k).copied().filter(|a| a.0 < u && u < a.1)
    }

    pub fn is_hidden(&self, u: Unit) -> bool {
        self.hidden.get(u.0 as usize).copied().unwrap_or(false)
    }

    pub fn len(&self) -> Unit {
        Unit(self.bytes.len() as u32 - 1)
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.len() == 1
    }

    fn unit_at_byte(&self, b: usize) -> Unit {
        Unit(self.bytes.partition_point(|&x| x < b) as u32)
    }

    fn byte(&self, u: Unit) -> usize {
        self.bytes[(u.0 as usize).min(self.bytes.len() - 1)]
    }

    pub fn char_at(&self, u: Unit) -> Option<char> {
        let b = self.byte(u);
        self.text[b..].chars().next()
    }

    pub fn is_boundary(&self, u: Unit) -> bool {
        let b = self.byte(u);
        b == 0 || b == self.text.len() || self.text.grapheme_indices(true).any(|(i, _)| i == b)
    }

    pub fn next_grapheme(&self, u: Unit) -> Unit {
        let mut v = self.next_raw(u);
        while v > u && v < self.len() && self.is_hidden(Unit(v.0 - 1)) {
            v = self.next_raw(v);
        }
        self.atom_at(v).map_or(v, |a| a.1)
    }

    pub fn prev_grapheme(&self, u: Unit) -> Unit {
        let mut v = self.prev_raw(u);
        while v < u && v.0 > 0 && self.is_hidden(v) {
            v = self.prev_raw(v);
        }
        self.atom_at(v).map_or(v, |a| a.0)
    }

    fn next_raw(&self, u: Unit) -> Unit {
        let b = self.byte(u);
        match self.text[b..].graphemes(true).next() {
            Some(g) => self.unit_at_byte(b + g.len()),
            None => u,
        }
    }

    fn prev_raw(&self, u: Unit) -> Unit {
        let b = self.byte(u);
        match self.text[..b].graphemes(true).next_back() {
            Some(g) => self.unit_at_byte(b - g.len()),
            None => u,
        }
    }

    pub fn widen(&self, u: Unit, end: bool) -> Unit {
        match self.atom_at(u) {
            Some(a) if end => a.1,
            _ => self.snap(u),
        }
    }

    pub fn snap(&self, u: Unit) -> Unit {
        if let Some(a) = self.atom_at(u) {
            return a.0;
        }
        if self.is_boundary(u) { u } else { self.prev_raw(u) }
    }

    fn words(&self) -> Vec<(Unit, Unit, Class)> {
        let mut out = Vec::new();
        for (b, w) in self.text.split_word_bound_indices() {
            let chars: Vec<(usize, char)> = w.char_indices().collect();
            let mut start = 0;
            for k in 1..chars.len().saturating_sub(1) {
                let (i, c) = chars[k];
                if matches!(c, '.' | ':') && chars[k - 1].1.is_alphabetic() && chars[k + 1].1.is_alphabetic() {
                    for (x, y) in [(start, i), (i, i + c.len_utf8())] {
                        out.push((self.unit_at_byte(b + x), self.unit_at_byte(b + y), class(&w[x..y])));
                    }
                    start = i + c.len_utf8();
                }
            }
            out.push((self.unit_at_byte(b + start), self.unit_at_byte(b + w.len()), class(&w[start..])));
        }
        for &(a, b) in &self.atoms {
            out.retain(|w| !(a <= w.0 && w.1 <= b));
            let k = out.partition_point(|w| w.0 < a);
            out.insert(k, (a, b, Class::Other));
        }
        out
    }

    pub fn next_word(&self, u: Unit) -> Unit {
        let words = self.words();
        let Some(k) = words.iter().position(|w| w.1 > u) else { return self.len() };
        let mut k = k;
        if words[k].2 != Class::Space {
            k += 1;
        }
        while k < words.len() && words[k].2 == Class::Space {
            k += 1;
        }
        words.get(k).map_or(self.len(), |w| w.0)
    }

    pub fn prev_word(&self, u: Unit) -> Unit {
        let words = self.words();
        let mut k = match words.iter().rposition(|w| w.0 < u) {
            Some(k) => k,
            None => return Unit(0),
        };
        while k > 0 && words[k].2 == Class::Space {
            k -= 1;
        }
        if words[k].2 == Class::Space { Unit(0) } else { words[k].0 }
    }

    pub fn word_at(&self, u: Unit) -> (Unit, Unit) {
        let words = self.words();
        let Some(k) = words.iter().position(|w| w.1 > u).or(words.len().checked_sub(1)) else { return (u, u) };
        let (start, mut end, kind) = words[k];
        if kind != Class::Space {
            let mut j = k + 1;
            while j < words.len() && words[j].2 == Class::Space && !matches!(self.char_at(words[j].0), Some('\t' | '\u{2028}' | '\u{c}')) {
                end = words[j].1;
                j += 1;
            }
        }
        (start, end)
    }

    pub fn letters_around(&self, u: Unit) -> Option<(Unit, Unit)> {
        self.words().into_iter().find(|w| w.2 == Class::Word && w.0 < u && u < w.1).map(|w| (w.0, w.1))
    }

    pub fn sentence_at(&self, u: Unit) -> (Unit, Unit) {
        let b = self.byte(u);
        for (i, s) in self.text.split_sentence_bound_indices() {
            if b < i + s.len() || i + s.len() == self.text.len() {
                return (self.unit_at_byte(i), self.unit_at_byte(i + s.len()));
            }
        }
        (u, u)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Class {
    Word,
    Space,
    Other,
}

fn class(w: &str) -> Class {
    let c = w.chars().next().unwrap_or(' ');
    if c == OBJECT || c == '\t' || c == '\u{2028}' || c == '\u{c}' {
        Class::Other
    } else if w.chars().all(|c| c.is_whitespace()) {
        Class::Space
    } else if w.chars().any(|c| c.is_alphanumeric()) {
        Class::Word
    } else {
        Class::Other
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ut(s: &str) -> UnitText {
        let mut bytes: Vec<usize> = s.char_indices().map(|(b, _)| b).collect();
        bytes.push(s.len());
        let hidden = vec![false; bytes.len().saturating_sub(1)];
        UnitText { text: s.to_string(), bytes, hidden, atoms: Vec::new() }
    }

    #[test]
    fn graphemes_keep_clusters_whole() {
        let t = ut("a👩\u{200d}👩\u{200d}👧b");
        assert_eq!(t.next_grapheme(Unit(1)), Unit(6));
        assert_eq!(t.prev_grapheme(Unit(6)), Unit(1));
        let e = ut("e\u{301}x");
        assert_eq!(e.next_grapheme(Unit(0)), Unit(2));
        assert_eq!(e.prev_grapheme(Unit(2)), Unit(0));
        assert!(!e.is_boundary(Unit(1)));
        assert_eq!(e.snap(Unit(1)), Unit(0));
        let flag = ut("🇺🇸");
        assert_eq!(flag.next_grapheme(Unit(0)), Unit(2));
    }

    #[test]
    fn words_stop_at_the_next_word_start() {
        let t = ut("quick brown, fox");
        assert_eq!(t.next_word(Unit(0)), Unit(6));
        assert_eq!(t.next_word(Unit(2)), Unit(6));
        assert_eq!(t.next_word(Unit(6)), Unit(11));
        assert_eq!(t.next_word(Unit(11)), Unit(13));
        assert_eq!(t.next_word(Unit(13)), Unit(16));
        assert_eq!(t.prev_word(Unit(16)), Unit(13));
        assert_eq!(t.prev_word(Unit(13)), Unit(11));
        assert_eq!(t.prev_word(Unit(8)), Unit(6));
        assert_eq!(t.prev_word(Unit(6)), Unit(0));
    }

    #[test]
    fn a_full_stop_between_letters_is_its_own_word() {
        let t = ut("end.Next don't 3.14");
        assert_eq!(t.next_word(Unit(0)), Unit(3));
        assert_eq!(t.next_word(Unit(3)), Unit(4));
        assert_eq!(t.word_at(Unit(10)), (Unit(9), Unit(15)));
        assert_eq!(t.word_at(Unit(16)), (Unit(15), Unit(19)));
    }

    #[test]
    fn double_click_takes_trailing_spaces() {
        let t = ut("quick  brown fox");
        assert_eq!(t.word_at(Unit(2)), (Unit(0), Unit(7)));
        assert_eq!(t.word_at(Unit(8)), (Unit(7), Unit(13)));
        assert_eq!(t.word_at(Unit(14)), (Unit(13), Unit(16)));
    }

    #[test]
    fn sentences() {
        let t = ut("One two. Three four. Five");
        assert_eq!(t.sentence_at(Unit(2)), (Unit(0), Unit(9)));
        assert_eq!(t.sentence_at(Unit(12)), (Unit(9), Unit(21)));
        assert_eq!(t.sentence_at(Unit(23)), (Unit(21), Unit(25)));
    }
}
