//! Character formatting: setters over the selection, and at a caret over the word there and what is typed next.

use crate::api::command::CaseChange;
use crate::api::value::{AutoOr, Color, HighlightColor, Points, UnderlineStyle};
use crate::docx::Inline;
use crate::font::Fonts;

use super::Outcome;
use crate::editing::format::{Format, Toggle};
use crate::editing::step::Step;
use crate::editing::{Editor, Tx};

/// `w:u`'s value for an underline style.
pub(crate) fn underline_value(style: UnderlineStyle) -> &'static str {
    match style {
        UnderlineStyle::None => "none",
        UnderlineStyle::Single => "single",
        UnderlineStyle::Words => "words",
        UnderlineStyle::Double => "double",
        UnderlineStyle::Thick => "thick",
        UnderlineStyle::Dotted => "dotted",
        UnderlineStyle::Dash => "dash",
        UnderlineStyle::DotDash => "dotDash",
        UnderlineStyle::Wave => "wave",
    }
}

/// `w:highlight`'s value for a highlight colour.
pub(crate) fn highlight_value(c: HighlightColor) -> &'static str {
    match c {
        HighlightColor::Yellow => "yellow",
        HighlightColor::BrightGreen => "green",
        HighlightColor::Turquoise => "cyan",
        HighlightColor::Pink => "magenta",
        HighlightColor::Blue => "blue",
        HighlightColor::Red => "red",
        HighlightColor::DarkBlue => "darkBlue",
        HighlightColor::Teal => "darkCyan",
        HighlightColor::Green => "darkGreen",
        HighlightColor::Violet => "darkMagenta",
        HighlightColor::DarkRed => "darkRed",
        HighlightColor::DarkYellow => "darkYellow",
        HighlightColor::Gray50 => "darkGray",
        HighlightColor::Gray25 => "lightGray",
        HighlightColor::Black => "black",
    }
}

impl Editor {
    pub(in crate::editing) fn set_toggle(&mut self, t: Toggle, on: bool, fonts: &Fonts) -> Outcome {
        self.format(&Format::Set(t, on), fonts)
    }

    /// An on/off run property (`w:dstrike`, `w:smallCaps`, `w:caps`, `w:vanish`).
    pub(in crate::editing) fn set_flag(&mut self, label: &'static str, name: &'static str, on: bool, fonts: &Fonts) -> Outcome {
        self.format_runs(label, &|p, w, _| p.set(name, if on { format!("<{w}{name}/>") } else { format!("<{w}{name} {w}val=\"0\"/>") }), fonts)
    }

    pub(in crate::editing) fn set_underline(&mut self, style: UnderlineStyle, fonts: &Fonts) -> Outcome {
        let v = underline_value(style);
        self.format_runs("Underline", &|p, w, _| p.set("u", format!("<{w}u {w}val=\"{v}\"/>")), fonts)
    }

    pub(in crate::editing) fn set_font_color(&mut self, c: AutoOr<Color>, fonts: &Fonts) -> Outcome {
        let c = match c {
            AutoOr::Auto => None,
            AutoOr::Value(c) => Some(c.0),
        };
        self.format(&Format::FontColor(c), fonts)
    }

    pub(in crate::editing) fn set_highlight(&mut self, h: Option<HighlightColor>, fonts: &Fonts) -> Outcome {
        self.format(&Format::Highlight(h.map(|h| highlight_value(h).to_string())), fonts)
    }

    pub(in crate::editing) fn set_character_spacing(&mut self, s: Points, fonts: &Fonts) -> Outcome {
        let tw = (s.0 * 20.0).round() as i64;
        self.format_runs(
            "Character Spacing",
            &|p, w, _| if tw == 0 { p.remove("spacing") } else { p.set("spacing", format!("<{w}spacing {w}val=\"{tw}\"/>")) },
            fonts,
        )
    }

    pub(in crate::editing) fn set_character_position(&mut self, s: Points, fonts: &Fonts) -> Outcome {
        let hp = (s.0 * 2.0).round() as i64;
        self.format_runs(
            "Position",
            &|p, w, _| if hp == 0 { p.remove("position") } else { p.set("position", format!("<{w}position {w}val=\"{hp}\"/>")) },
            fonts,
        )
    }

    /// The selected text in another case, its formatting kept. Each selected piece of a paragraph is changed on its
    /// own (a sentence starts at the piece's start, and after `.`, `!` or `?`).
    pub(in crate::editing) fn change_case(&mut self, to: CaseChange, fonts: &Fonts) -> Outcome {
        if self.selection.is_collapsed() {
            return Outcome::unavailable("nothing selected");
        }
        let ranges: Vec<_> = self.selected_ranges().into_iter().filter(|r| r.1 < r.2).collect();
        let mut tx = self.begin("Change Case");
        for (para, a, b) in ranges {
            let Some(p) = self.index.para(&self.doc, para) else { continue };
            let mut with = Vec::new();
            let mut at = 0u32;
            // Where the next letter starts a word or a sentence.
            let (mut word, mut sentence) = (true, true);
            for i in &p.inlines {
                let n = i.units() as u32;
                let (s, e) = (at, at + n);
                at = e;
                if n == 0 {
                    if s > a.0 && s < b.0 {
                        with.push(i.clone());
                    }
                    continue;
                }
                if e <= a.0 || s >= b.0 {
                    continue;
                }
                match i {
                    Inline::Text(text, run) => {
                        let (from, to_) = (a.0.max(s) - s, b.0.min(e) - s);
                        let part: String = text.chars().skip(from as usize).take((to_ - from) as usize).collect();
                        with.push(Inline::Text(recase(&part, to, &mut word, &mut sentence), run.clone()));
                    }
                    other => {
                        word = true;
                        with.push(other.clone());
                    }
                }
            }
            if self.step(&mut tx, Step::Text { para, range: a..b, with, edges: Default::default() }).is_err() {
                self.abort(tx);
                return Outcome::Unchanged("changing case failed");
            }
        }
        self.keep_selection_through(&tx);
        self.commit(tx, None, fonts)
    }

    /// The selection's ends carried through the transaction's steps.
    fn keep_selection_through(&mut self, tx: &Tx) {
        let map = |p: crate::editing::position::Position| tx.maps.iter().fold(p, |p, m| m.apply(p));
        if let crate::editing::selection::Selection::Text { anchor, focus, unit } = self.selection {
            self.selection = crate::editing::selection::Selection::range(map(anchor), map(focus), unit);
        }
    }

    // shortcut: the format painter isn't built yet.
    pub(in crate::editing) fn copy_formatting(&mut self) -> Outcome {
        Outcome::unsupported("the format painter")
    }

    pub(in crate::editing) fn paste_formatting(&mut self, _fonts: &Fonts) -> Outcome {
        Outcome::unsupported("the format painter")
    }
}

/// `text` in case `to`; `word` and `sentence` say whether the next letter starts one, and carry on to the next piece.
fn recase(text: &str, to: CaseChange, word: &mut bool, sentence: &mut bool) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        let up = |c: char| c.to_uppercase().collect::<String>();
        let low = |c: char| c.to_lowercase().collect::<String>();
        let s = match to {
            CaseChange::Lower => low(c),
            CaseChange::Upper => up(c),
            CaseChange::Toggle if c.is_uppercase() => low(c),
            CaseChange::Toggle => up(c),
            CaseChange::Title if *word => up(c),
            CaseChange::Title => low(c),
            CaseChange::Sentence if *sentence && c.is_alphabetic() => up(c),
            CaseChange::Sentence => low(c),
        };
        out.push_str(&s);
        if c.is_alphanumeric() {
            *word = false;
            *sentence = false;
        } else {
            *word = true;
            if matches!(c, '.' | '!' | '?') {
                *sentence = true;
            }
        }
    }
    out
}

#[test]
fn cases() {
    let run = |t: &str, to| recase(t, to, &mut true, &mut true);
    assert_eq!(run("hello World. again", CaseChange::Sentence), "Hello world. Again");
    assert_eq!(run("hello wORLD", CaseChange::Title), "Hello World");
    assert_eq!(run("Hello", CaseChange::Toggle), "hELLO");
    assert_eq!(run("Hello", CaseChange::Upper), "HELLO");
    assert_eq!(run("Hello", CaseChange::Lower), "hello");
}
