//! Smart cut and paste

use super::Editor;
use super::text::UnitText;

fn word(c: char) -> bool {
    c.is_alphanumeric()
}

fn punct(c: char) -> bool {
    matches!(c, '.' | ',' | ';' | ':' | '!' | '?')
}

impl Editor {
    fn around(&self) -> Option<(Option<char>, Option<char>)> {
        let (a, b) = self.selection.ordered(&self.index)?;
        let (pa, oa) = a.para_offset()?;
        let (pb, ob) = b.para_offset()?;
        let ta = UnitText::of(self.index.para(&self.doc, pa)?).text;
        let tb = UnitText::of(self.index.para(&self.doc, pb)?).text;
        let before = (oa.0 as usize).checked_sub(1).and_then(|u| ta.chars().nth(u));
        Some((before, tb.chars().nth(ob.0 as usize)))
    }

    pub(super) fn smart_spaces(&self, t: &str) -> (bool, bool) {
        let (Some(first), Some(last)) = (t.chars().next(), t.chars().last()) else { return (false, false) };
        if t.contains('\n') {
            return (false, false);
        }
        let Some((before, after)) = self.around() else { return (false, false) };
        (before.is_some_and(word) && word(first), after.is_some_and(word) && word(last))
    }

    pub(super) fn smart_cut(&mut self) {
        let Some((a, b)) = self.selection.ordered(&self.index) else { return };
        let (Some((pa, oa)), Some((pb, ob))) = (a.para_offset(), b.para_offset()) else { return };
        if pa != pb || oa >= ob {
            return;
        }
        let Some((before, after)) = self.around() else { return };
        if before.is_some_and(word) || after.is_some_and(word) {
            return;
        }
        let widen = |s: &super::position::Position, by: i64| {
            super::position::Position::text(s.story, pa, crate::layout::Unit((ob.0 as i64 + by).max(0) as u32))
        };
        let (na, nb) = match (before, after) {
            (Some(' '), Some(' ')) | (None, Some(' ')) => (a, widen(&b, 1)),
            (Some(' '), Some(c)) if punct(c) => (super::position::Position::text(a.story, pa, crate::layout::Unit(oa.0 - 1)), b),
            (Some(' '), None) => (super::position::Position::text(a.story, pa, crate::layout::Unit(oa.0 - 1)), b),
            _ => return,
        };
        self.selection = super::selection::Selection::range(na, nb, super::selection::Granularity::Char);
    }
}
