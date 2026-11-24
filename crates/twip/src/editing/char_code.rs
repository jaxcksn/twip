//! Character codes (Alt+X)

use crate::font::Fonts;
use crate::layout::Unit;

use super::Editor;
use super::commands::Outcome;
use super::position::Position;
use super::selection::{Granularity, Selection};
use super::text::{OBJECT, UnitText};

/// The most hexadecimal digits a code takes (U+10FFFF).
const MAX_DIGITS: usize = 6;

/// What Alt+X does to `text`, the characters before the caret (or the selection, `whole`): how many of its last
/// characters change, and to what. A hexadecimal code (with `U+` before it, or without) becomes its character, else
/// the last character becomes its code; a selection converts only when all of it does.
pub fn code_toggle(text: &[char], whole: bool) -> Option<(usize, String)> {
    let n = text.len();
    let hex = text.iter().rev().take_while(|c| c.is_ascii_hexdigit()).count();
    for k in (1..=hex.min(MAX_DIGITS)).rev() {
        let start = n - k;
        let Some(c) = text[start..].iter().try_fold(0u32, |v, d| Some(v * 16 + d.to_digit(16)?)).and_then(char::from_u32) else { continue };
        if c.is_control() {
            continue;
        }
        let prefix = if start >= 2 && matches!(text[start - 2..start], ['U' | 'u', '+']) { 2 } else { 0 };
        if whole && start != prefix {
            continue;
        }
        return Some((k + prefix, c.to_string()));
    }
    let &c = text.last()?;
    if (whole && n != 1) || c == OBJECT || c.is_control() {
        return None;
    }
    Some((1, format!("{:04X}", u32::from(c))))
}

impl Editor {
    /// Alt+X: the hexadecimal code before the caret becomes its character, or the character before it its code (the
    /// selection, when it is one or the other). One undo step; the caret ends after the new text.
    pub fn toggle_character_code(&mut self, fonts: &Fonts) -> Outcome {
        let Some((a, b)) = self.selection.ordered(&self.index) else { return Outcome::Unchanged("no caret") };
        let (Some((para, from)), Some((para_b, to))) = (a.para_offset(), b.para_offset()) else { return Outcome::unavailable("not in text") };
        if para != para_b {
            return Outcome::unavailable("the selection spans paragraphs");
        }
        let Some(p) = self.index.para(&self.doc, para) else { return Outcome::Unchanged("no paragraph") };
        let whole = from != to;
        let start = if whole { from.0 as usize } else { (to.0 as usize).saturating_sub(MAX_DIGITS + 2) };
        let text: Vec<char> = UnitText::of(p).text.chars().skip(start).take(to.0 as usize - start).collect();
        let Some((k, with)) = code_toggle(&text, whole) else { return Outcome::unavailable("no character code there") };
        let at = Unit(to.0 - k as u32);
        let original = self.selection;
        let mut tx = self.begin("Unicode");
        self.selection = Selection::range(Position::text(a.story, para, at), b, Granularity::Char);
        if self.refuse_expanded() || self.refuse_locked() {
            self.selection = original;
            return Outcome::Unchanged("locked");
        }
        match self.type_into(&mut tx, &with) {
            Ok((caret, _)) => {
                self.selection = Selection::Caret(caret);
                self.commit(tx, None, fonts)
            }
            Err(why) => {
                self.abort(tx);
                Outcome::Unchanged(why)
            }
        }
    }
}

#[test]
fn codes_and_characters_toggle() {
    let t = |s: &str, whole| code_toggle(&s.chars().collect::<Vec<_>>(), whole);
    assert_eq!(t("x 00e9", false), Some((4, "é".into())));
    assert_eq!(t("xU+00E9", false), Some((6, "é".into())));
    assert_eq!(t("1F600", false), Some((5, "\u{1F600}".into())));
    assert_eq!(t("é", false), Some((1, "00E9".into())));
    assert_eq!(t("x\u{1F600}", false), Some((1, "1F600".into())));
    assert_eq!(t("2", false), Some((1, "0032".into())), "a control code's digit becomes its code");
    assert_eq!(t("1234567", false), Some((5, "\u{34567}".into())), "the longest code there is");
    assert_eq!(t("D800", false), Some((3, "\u{800}".into())), "no surrogate");
    assert_eq!(t("ab", true), Some((2, "\u{AB}".into())));
    assert_eq!(t("xab", true), None, "all of a selection or none");
    assert_eq!(t("", false), None);
}
