//! AutoCorrect as you type: after text is typed (or a paragraph ended), the words just finished are corrected in one
//! step of their own, so an undo right after it takes back only the correction. The rules: the replacement list,
//! symbols, smart quotes, dashes, fractions, ordinals, and capitals (two initial capitals, day names, the first letter
//! of a sentence or of a table cell), and lists a paragraph's start begins (`1.` or `*` then a space). Only typing
//! corrects: pasted and inserted text never do.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::OnceLock;

use crate::font::Fonts;
use crate::layout::Unit;

use super::Editor;
use super::commands::Outcome;
use super::position::{Place, Position};
use super::selection::Selection;
use super::step::Step;

/// The rules that are on, and the lists they use.
#[derive(Clone, Debug, PartialEq)]
pub struct AutoCorrect {
    pub two_initial_capitals: bool,
    pub sentence_capitals: bool,
    pub cell_capitals: bool,
    pub day_names: bool,
    pub replace_text: bool,
    pub smart_quotes: bool,
    pub ordinals: bool,
    pub fractions: bool,
    pub dashes: bool,
    /// `1.` or `1)` then a space or tab starts a numbered list.
    pub numbered_lists: bool,
    /// `*` or `-` then a space or tab starts a bulleted list.
    pub bulleted_lists: bool,
    /// The replacement list; `None`: the shipped one.
    pub entries: Option<HashMap<String, String>>,
    /// Abbreviations after which a sentence doesn't start (`e.g.`); `None`: the shipped list.
    pub first_letter_exceptions: Option<Vec<String>>,
    /// Words whose two initial capitals stay; `None`: the shipped list.
    pub initial_caps_exceptions: Option<Vec<String>>,
    /// Typed texts never corrected again (Never correct).
    pub never: Vec<String>,
}

impl Default for AutoCorrect {
    fn default() -> AutoCorrect {
        AutoCorrect {
            two_initial_capitals: true,
            sentence_capitals: true,
            cell_capitals: true,
            day_names: true,
            replace_text: true,
            smart_quotes: true,
            ordinals: true,
            fractions: true,
            dashes: true,
            numbered_lists: true,
            bulleted_lists: true,
            entries: None,
            first_letter_exceptions: None,
            initial_caps_exceptions: None,
            never: Vec::new(),
        }
    }
}

/// The shipped lists, read once.
struct Lists {
    entries: HashMap<&'static str, &'static str>,
    sentence: Vec<&'static str>,
    caps: Vec<&'static str>,
}

fn lists() -> &'static Lists {
    static LISTS: OnceLock<Lists> = OnceLock::new();
    LISTS.get_or_init(|| {
        let lines = |s: &'static str| s.lines().filter(|l| !l.starts_with('#') && !l.is_empty());
        Lists {
            entries: lines(include_str!("autocorrect/en-US.tsv")).filter_map(|l| l.split_once('\t')).collect(),
            sentence: lines(include_str!("autocorrect/en-US-sentence.txt")).collect(),
            caps: lines(include_str!("autocorrect/en-US-caps.txt")).collect(),
        }
    })
}

/// Symbols replaced as soon as their last character is typed, anywhere (inside a word too).
const SYMBOLS: [(&str, &str); 18] = [
    ("(c)", "©"),
    ("(r)", "®"),
    ("(tm)", "™"),
    ("(e)", "€"),
    ("...", "…"),
    (":-)", "☺"),
    (":)", "☺"),
    (";-)", "☺"),
    (";)", "☺"),
    (":-(", "☹"),
    (":(", "☹"),
    (":-|", "😐"),
    (":|", "😐"),
    ("<=>", "⇔"),
    ("==>", "⇒"),
    ("<==", "⇐"),
    ("-->", "→"),
    ("<--", "←"),
];

const DAYS: [&str; 7] = ["monday", "tuesday", "wednesday", "thursday", "friday", "saturday", "sunday"];

/// What ended the typing a correction looks at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trigger {
    /// The last character of the text before the caret was just typed.
    Typed,
    /// The paragraph was just ended (Enter): its whole text, with no character after it.
    ParagraphEnd,
}

/// One correction: replace `range` (in the text's units) with `with`, its `superscript` part (an ordinal's suffix)
/// raised.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fix {
    pub range: Range<usize>,
    pub with: String,
    pub superscript: Option<Range<usize>>,
    /// What was typed, for Revert and Never correct.
    pub from: String,
}

/// The list a paragraph's typed start begins, `true` for bullets: `1.` or `1)` then a space or tab a numbered one, `*`
/// or `-` then one a bulleted one.
fn list_start(before: &[char], ac: &AutoCorrect) -> Option<bool> {
    let (last, marker) = before.split_last()?;
    if !matches!(last, ' ' | '\t') {
        return None;
    }
    match marker {
        ['1', '.' | ')'] if ac.numbered_lists => Some(false),
        ['*' | '-'] if ac.bulleted_lists => Some(true),
        _ => None,
    }
}

/// Whether typing `c` can finish something to correct: a word's end, a quote, a symbol's last character.
pub fn may_trigger(c: char) -> bool {
    c.is_whitespace() || ends_word(c) || matches!(c, '"' | '\'' | ')' | '>' | '|')
}

/// Characters a word ends before (besides whitespace).
fn ends_word(c: char) -> bool {
    matches!(c, ',' | '.' | ';' | ':' | '!' | '?')
}

/// The correction `before` (a paragraph's units up to the caret, one `char` each) takes, if any. `cell_start`: the
/// paragraph is a table cell's first.
pub fn correction(before: &[char], trigger: Trigger, cell_start: bool, ac: &AutoCorrect) -> Option<Fix> {
    let n = before.len();
    let last = *before.last()?;
    let lists = lists();
    if trigger == Trigger::Typed {
        // Smart quotes: a quote opens where a word can start, else closes.
        if ac.smart_quotes && matches!(last, '"' | '\'') {
            let prev = before[..n - 1].last().copied();
            let (open, close) = if last == '"' { ('\u{201C}', '\u{201D}') } else { ('\u{2018}', '\u{2019}') };
            // A hyphen between a space and an opening quote is a dash.
            if ac.dashes && prev == Some('-') && n >= 3 && before[n - 3].is_whitespace() {
                return Some(Fix { range: n - 2..n, with: format!("\u{2013}{open}"), superscript: None, from: format!("-{last}") });
            }
            let opens = prev.is_none_or(|p| p.is_whitespace() || matches!(p, '(' | '[' | '{' | '\u{2013}' | '\u{2014}' | '\u{c}' | '\u{2028}'));
            return Some(Fix { range: n - 1..n, with: (if opens { open } else { close }).to_string(), superscript: None, from: last.to_string() });
        }
        // Symbols, longest first among those ending here.
        if ac.replace_text {
            let tail: String = before[n.saturating_sub(4)..].iter().collect::<String>().to_lowercase();
            if let Some((from, to)) = SYMBOLS.iter().filter(|(f, _)| tail.ends_with(f)).max_by_key(|(f, _)| f.len())
                && !ac.never.iter().any(|v| v == from)
            {
                let k = from.chars().count();
                return Some(Fix { range: n - k..n, with: (*to).to_string(), superscript: None, from: before[n - k..].iter().collect() });
            }
        }
        if !(last.is_whitespace() || ends_word(last)) {
            return None;
        }
    }
    // The word just finished: back from its end to whitespace.
    let end = if trigger == Trigger::Typed { n - 1 } else { n };
    let start = before[..end].iter().rposition(|c| c.is_whitespace() || *c == super::text::OBJECT).map_or(0, |i| i + 1);
    if start >= end {
        return None;
    }
    // Its core: without leading quotes or brackets and trailing punctuation.
    let lead = before[start..end].iter().take_while(|c| matches!(c, '"' | '\'' | '\u{201C}' | '\u{2018}' | '(' | '[')).count();
    let trail = before[start + lead..end]
        .iter()
        .rev()
        .take_while(|c| ends_word(**c) || matches!(c, '"' | '\'' | '\u{201D}' | '\u{2019}' | ')' | ']'))
        .count();
    let (cs, ce) = (start + lead, end - trail);
    let core: String = before[cs..ce].iter().collect();
    let mut with = core.clone();
    let mut range = cs..ce;
    let mut superscript = None;
    let mut prefix = String::new();
    let mut changed = false;
    if !core.is_empty() && !ac.never.contains(&core) {
        // The replacement list: as typed, else in lower case with the typed word's case.
        if ac.replace_text {
            let lookup = |w: &str| match &ac.entries {
                Some(m) => m.get(w).cloned(),
                None => lists.entries.get(w).map(|s| s.to_string()),
            };
            let lower = core.to_lowercase();
            let found = lookup(&core).or_else(|| {
                let to = lookup(&lower)?;
                let mut c = core.chars();
                let first_upper = c.next().is_some_and(char::is_uppercase);
                let all_upper = core.chars().filter(|c| c.is_alphabetic()).all(char::is_uppercase) && core.chars().count() > 1;
                Some(if all_upper {
                    to.to_uppercase()
                } else if first_upper {
                    capitalised(&to)
                } else {
                    to
                })
            });
            if let Some(to) = found.filter(|t| *t != core) {
                with = to;
                changed = true;
            }
        }
        // Fractions.
        if ac.fractions
            && let Some(f) = [("1/2", "½"), ("1/4", "¼"), ("3/4", "¾")].iter().find(|(f, _)| *f == with)
        {
            with = f.1.to_string();
            changed = true;
        }
        // Ordinals: a number and its own suffix, the suffix raised.
        if ac.ordinals
            && let Some(k) = ordinal_suffix(&with)
        {
            superscript = Some(k..with.chars().count());
            changed = true;
        }
        // An apostrophe before a year: '26.
        if ac.smart_quotes && lead == 1 && before[start] == '\u{2018}' && core.chars().all(|c| c.is_ascii_digit()) {
            range = start..ce;
            with = format!("\u{2019}{with}");
            changed = true;
        }
        // Dashes: word--word is an em dash; word - word and word -- word an en dash.
        if ac.dashes {
            let w: Vec<char> = with.chars().collect();
            if let Some(i) = w.windows(2).position(|p| p == ['-', '-'])
                && i > 0
                && w.get(i + 2).is_some_and(|c| *c != '-')
                && w[i - 1] != '-'
            {
                with = w[..i].iter().chain(['\u{2014}'].iter()).chain(w[i + 2..].iter()).collect();
                changed = true;
            } else if trigger == Trigger::ParagraphEnd && w.len() > 2 && w.ends_with(&['-', '-']) && w[w.len() - 3] != '-' {
                with = w[..w.len() - 2].iter().chain(['\u{2014}'].iter()).collect();
                changed = true;
            } else if start >= 2 {
                for spaced in [&[' ', '-', ' '][..], &[' ', '-', '-', ' '][..]] {
                    let k = spaced.len();
                    if start > k && before[start - k..start] == *spaced && !before[start - k - 1].is_whitespace() {
                        prefix = " \u{2013} ".to_string();
                        range = start - k..range.end;
                        changed = true;
                        break;
                    }
                }
            }
        }
        // Two initial capitals: THe → The (not a plural acronym such as CDs, nor an exception).
        let letters: Vec<char> = with.chars().collect();
        if ac.two_initial_capitals
            && letters.len() >= 3
            && letters.iter().all(|c| c.is_alphabetic())
            && letters[0].is_uppercase()
            && letters[1].is_uppercase()
            && letters[2..].iter().all(|c| c.is_lowercase())
            && letters[2..].iter().collect::<String>() != "s"
            && !exception(&with, ac.initial_caps_exceptions.as_deref(), &lists.caps)
        {
            with = letters[..1].iter().chain(letters[1].to_lowercase().collect::<Vec<_>>().iter()).chain(letters[2..].iter()).collect();
            changed = true;
        }
        // Day names.
        let lower = with.to_lowercase();
        let day = lower.split(['\'', '\u{2019}']).next().unwrap_or("");
        if ac.day_names && DAYS.contains(&day) && with.starts_with(|c: char| c.is_lowercase()) {
            with = capitalised(&with);
            changed = true;
        }
        // The first letter of a sentence (or of a table cell's text).
        let sentence_start = before[..start].iter().rposition(|c| !c.is_whitespace()).is_none_or(|i| {
            let prev_word_start = before[..i].iter().rposition(|c| c.is_whitespace()).map_or(0, |j| j + 1);
            let prev: String = before[prev_word_start..=i].iter().collect();
            matches!(before[i], '.' | '!' | '?') && !exception(&prev, ac.first_letter_exceptions.as_deref(), &lists.sentence)
        });
        let para_start = before[..start].iter().all(|c| c.is_whitespace());
        let wants = (ac.sentence_capitals && sentence_start) || (ac.cell_capitals && cell_start && para_start);
        if wants && with.starts_with(|c: char| c.is_lowercase()) && !with.contains(['.', '@', '/', '\\']) {
            with = capitalised(&with);
            changed = true;
        }
    }
    if !changed {
        return None;
    }
    let superscript = superscript.map(|r| r.start + prefix.chars().count()..r.end + prefix.chars().count());
    let with = prefix + &with;
    let from: String = before[range.clone()].iter().collect();
    (with != from || superscript.is_some()).then_some(Fix { range, with, superscript, from })
}

/// `w` with its first letter in upper case.
fn capitalised(w: &str) -> String {
    let mut c = w.chars();
    c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
}

/// Whether `w` is in the exceptions (the host's, else the shipped ones), ignoring case.
fn exception(w: &str, own: Option<&[String]>, shipped: &[&str]) -> bool {
    match own {
        Some(list) => list.iter().any(|e| e.eq_ignore_ascii_case(w)),
        None => shipped.iter().any(|e| e.eq_ignore_ascii_case(w)),
    }
}

/// Where an ordinal's suffix starts (1st, 22nd, 103rd, 11th), in characters; `None` for anything else.
fn ordinal_suffix(w: &str) -> Option<usize> {
    let digits = w.chars().take_while(char::is_ascii_digit).count();
    if digits == 0 || w.chars().count() != digits + 2 {
        return None;
    }
    let n: u64 = w[..digits].parse().ok()?;
    let suffix = w[digits..].to_lowercase();
    let want = match (n % 100, n % 10) {
        (11..=13, _) => "th",
        (_, 1) => "st",
        (_, 2) => "nd",
        (_, 3) => "rd",
        _ => "th",
    };
    (suffix == want).then_some(digits)
}

impl Editor {
    /// Corrects what was just typed at the caret (`Trigger::Typed`), or the paragraph before the caret's that Enter
    /// just ended (`Trigger::ParagraphEnd`), as one step of its own.
    pub fn auto_correct(&mut self, ac: &AutoCorrect, trigger: Trigger, fonts: &Fonts) -> Outcome {
        let Selection::Caret(caret) = self.selection else { return Outcome::Unchanged("a selection") };
        let Place::Text { para: here, offset, .. } = caret.place else { return Outcome::Unchanged("not in text") };
        let (para, at) = match trigger {
            Trigger::Typed => (here, offset),
            Trigger::ParagraphEnd => {
                let Some(slot) = self.index.para_slot(here) else { return Outcome::Unchanged("no paragraph") };
                let prev = slot.block.checked_sub(1).and_then(|b| self.index.blocks(&self.doc, slot.container)?.get(b));
                let Some(crate::docx::Block::Para(p)) = prev else { return Outcome::Unchanged("no paragraph before") };
                let id = crate::layout::ParaId(p.id);
                (id, Unit(super::text::UnitText::of(p).len().0))
            }
        };
        let Some(p) = self.index.para(&self.doc, para) else { return Outcome::Unchanged("no paragraph") };
        let text = super::text::UnitText::of(p);
        let before: Vec<char> = text.text.chars().take(at.0 as usize).collect();
        // A list's start typed at a paragraph's start: the marker goes and the paragraph joins a list, one step.
        if trigger == Trigger::Typed
            && let Some(bullet) = list_start(&before, ac)
            && self.para_list(para).is_none()
        {
            let label = if bullet { "Automatic Bullets" } else { "Automatic Numbering" };
            let n = Unit(before.len() as u32);
            return self.joined(label, fonts, |e| {
                let mut tx = e.begin(label);
                if e.step(&mut tx, Step::Text { para, range: Unit(0)..n, with: Vec::new(), edges: Default::default() }).is_err() {
                    e.abort(tx);
                    return Outcome::Unchanged("the marker stayed");
                }
                e.selection = Selection::Caret(Position::text(caret.story, para, Unit(0)));
                e.commit(tx, None, fonts);
                e.toggle_list(bullet, fonts)
            });
        }
        let cell_start = self.index.para_slot(para).is_some_and(|s| matches!(s.container, super::index::Container::Cell { .. }) && s.block == 0);
        let Some(fix) = correction(&before, trigger, cell_start, ac) else { return Outcome::Unchanged("nothing to correct") };
        let (a, b) = (Unit(fix.range.start as u32), Unit(fix.range.end as u32));
        let Some(run) = self.run_at(&Position::text(caret.story, para, Unit(a.0 + 1))) else { return Outcome::Unchanged("no run") };
        // An ordinal already raised is left as it is.
        if fix.with == fix.from
            && let Some(s) = &fix.superscript
            && self
                .run_at(&Position::text(caret.story, para, Unit(a.0 + s.end as u32)))
                .is_some_and(|r| r.vert_align == crate::docx::props::VertAlign::Superscript)
        {
            return Outcome::Unchanged("already corrected");
        }
        let raised = match &fix.superscript {
            Some(_) => match self.raised(para, &run) {
                Some(r) => Some(r),
                None => return Outcome::Unchanged("no styles"),
            },
            None => None,
        };
        let chars: Vec<char> = fix.with.chars().collect();
        let inlines = match (&fix.superscript, raised) {
            (Some(s), Some(up)) => [(0..s.start, run.clone()), (s.clone(), up), (s.end..chars.len(), run.clone())]
                .into_iter()
                .filter(|(r, _)| !r.is_empty())
                .map(|(r, run)| crate::docx::Inline::Text(chars[r].iter().collect(), run))
                .collect(),
            _ => vec![crate::docx::Inline::Text(fix.with.clone(), run)],
        };
        let mut tx = self.begin("AutoCorrect");
        if self.step(&mut tx, Step::Text { para, range: a..b, with: inlines, edges: Default::default() }).is_err() {
            self.abort(tx);
            return Outcome::Unchanged("correction failed");
        }
        if trigger == Trigger::Typed {
            let shift = chars.len() as i64 - (b.0 - a.0) as i64;
            let at = Unit((offset.0 as i64 + shift).max(0) as u32);
            self.selection = Selection::Caret(Position::text(caret.story, para, at));
        }
        self.correction = Some(super::Correction { para, range: a..Unit(a.0 + chars.len() as u32), from: fix.from.clone(), typed: 0 });
        self.commit(tx, None, fonts)
    }

    /// The correction mark's view: the corrected text's boxes, and the options button (16 × 12 px, under the text's
    /// middle) while the caret is in the text or just after it.
    pub fn correction_view(&mut self, pages: std::ops::Range<usize>) -> Option<super::view::CorrectionView> {
        let c = self.correction.clone()?;
        let story = self.caret().map(|p| p.story)?;
        let rects = self.range_rects(Position::text(story, c.para, c.range.start), Position::text(story, c.para, c.range.end), pages);
        let near = self.caret().and_then(|p| p.para_offset()).is_some_and(|(p, at)| p == c.para && at >= c.range.start && at.0 <= c.range.end.0 + 1);
        let button = near
            .then(|| {
                rects.last().map(|&(page, r)| {
                    let (w, h) = (12.0, 9.0);
                    let mid = (rects.iter().filter(|(pg, _)| *pg == page).map(|(_, r)| r[0]).fold(f64::MAX, f64::min) + r[2]) / 2.0;
                    (page, [mid - w / 2.0, r[3] + 0.75, mid + w / 2.0, r[3] + 0.75 + h])
                })
            })
            .flatten();
        Some(super::view::CorrectionView { rects, button })
    }

    /// Keeps the correction mark through typing after it, and while the caret only moves onto the corrected text (as
    /// Word does: Ctrl+Left onto the word, then Alt+Shift+F10). Drops it when the caret leaves its paragraph or goes
    /// before it, an edit lands before its end (`edited`: the input changed the document), or more than 14
    /// characters were typed after it.
    pub fn follow_correction(&mut self, typed: usize, edited: bool) {
        let caret = self.caret().and_then(|p| p.para_offset());
        let Some(c) = &mut self.correction else { return };
        let from = if edited { c.range.end } else { c.range.start };
        let keep = caret.is_some_and(|(p, at)| p == c.para && at >= from);
        c.typed += typed;
        if !keep || c.typed > 14 {
            self.correction = None;
        }
    }

    /// The correction mark, if one is showing.
    pub fn correction(&self) -> Option<&super::Correction> {
        self.correction.as_ref()
    }

    /// The correction mark's text as it is now.
    pub fn correction_text(&self) -> Option<String> {
        let c = self.correction.as_ref()?;
        let p = self.index.para(&self.doc, c.para)?;
        let t = super::text::UnitText::of(p);
        Some(t.text.chars().skip(c.range.start.0 as usize).take((c.range.end.0 - c.range.start.0) as usize).collect())
    }

    /// Puts back what was typed in place of the correction (Revert, Never correct): an undo while the correction is the
    /// last step, else a replacement of its text as a step of its own. The mark goes.
    pub fn revert_correction(&mut self, fonts: &Fonts) -> Outcome {
        let Some(c) = self.correction.take() else { return Outcome::Unchanged("no correction") };
        if self.history.next_undo() == Some("AutoCorrect") && c.typed == 0 {
            return self.execute(super::commands::Command::Undo, fonts);
        }
        let Some(story) = self.caret().map(|p| p.story) else { return Outcome::Unchanged("no caret") };
        let Some(run) = self.run_at(&Position::text(story, c.para, Unit(c.range.start.0 + 1))) else { return Outcome::Unchanged("no run") };
        let mut tx = self.begin("Revert AutoCorrect");
        let with = vec![crate::docx::Inline::Text(c.from.clone(), run)];
        if self.step(&mut tx, Step::Text { para: c.para, range: c.range.clone(), with, edges: Default::default() }).is_err() {
            self.abort(tx);
            return Outcome::Unchanged("revert failed");
        }
        self.commit(tx, None, fonts)
    }

    /// `run` raised to superscript: its direct formatting with `w:vertAlign` added, resolved again.
    fn raised(&self, para: crate::layout::ParaId, run: &crate::docx::Run) -> Option<crate::docx::Run> {
        let res = self.doc.resolver.clone()?;
        let cx = self.cx();
        let base = self.run_base(para, &res, &cx)?;
        let mut p = crate::docx::write::props::Props::parse(run.direct.0.as_deref(), "rPr", &cx);
        let w = cx.w.clone();
        p.set("vertAlign", format!("<{w}vertAlign {w}val=\"superscript\"/>"));
        Some(res.run_from_xml(&base, p.xml().as_deref(), &cx))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `typed` typed one character at a time, each followed by the correction it triggers: the text that results and
    /// the superscript ranges.
    fn type_out(typed: &str, ac: &AutoCorrect) -> (String, Vec<Range<usize>>) {
        let mut text: Vec<char> = Vec::new();
        let mut sup: Vec<Range<usize>> = Vec::new();
        for c in typed.chars() {
            text.push(c);
            if let Some(f) = correction(&text, Trigger::Typed, false, ac) {
                let shift = f.with.chars().count() as i64 - f.range.len() as i64;
                if let Some(s) = &f.superscript {
                    sup.push(f.range.start + s.start..f.range.start + s.end);
                }
                text.splice(f.range.clone(), f.with.chars());
                let _ = shift;
            }
        }
        (text.into_iter().collect(), sup)
    }

    #[test]
    fn typing_corrects_as_the_cases_show() {
        // (typed, result, superscript): from typing each case into a word processor with its defaults.
        let ac = AutoCorrect::default();
        let cases: &[(&str, &str, &[(usize, usize)])] = &[
            ("\"hello\" ", "\u{201C}Hello\u{201D} ", &[]),
            ("Word (\"hello\") ", "Word (\u{201C}hello\u{201D}) ", &[]),
            ("Word -\"hello\" ", "Word \u{2013}\u{201C}hello\u{201D} ", &[]),
            ("Word /\"hello\" ", "Word /\u{201D}hello\u{201D} ", &[]),
            ("Word don't ", "Word don\u{2019}t ", &[]),
            ("Word rock 'n' roll ", "Word rock \u{2018}n\u{2019} roll ", &[]),
            ("Word '26 ", "Word \u{2019}26 ", &[]),
            ("Word the '90s ", "Word the \u{2018}90s ", &[]),
            ("Word word--word ", "Word word\u{2014}word ", &[]),
            ("Word word -- word ", "Word word \u{2013} word ", &[]),
            ("Word word - word ", "Word word \u{2013} word ", &[]),
            ("Word 1--2 ", "Word 1\u{2014}2 ", &[]),
            ("Word word---word ", "Word word---word ", &[]),
            ("Word word -word ", "Word word -word ", &[]),
            ("Word 1/2 ", "Word ½ ", &[]),
            ("Word 1/2, ", "Word ½, ", &[]),
            ("Word 1/3 ", "Word 1/3 ", &[]),
            ("Word 1st ", "Word 1st ", &[(6, 8)]),
            ("Word 22nd ", "Word 22nd ", &[(7, 9)]),
            ("Word 11th ", "Word 11th ", &[(7, 9)]),
            ("Word 1th ", "Word 1th ", &[]),
            ("Word THe cat ", "Word The cat ", &[]),
            ("Word THE cat ", "Word THE cat ", &[]),
            ("Word CDs ", "Word CDs ", &[]),
            ("Word IDs ", "Word IDs ", &[]),
            ("Word monday ", "Word Monday ", &[]),
            ("Word mon ", "Word mon ", &[]),
            ("Word (c) ", "Word © ", &[]),
            ("Word a(c) ", "Word a© ", &[]),
            ("Word (tm) ", "Word ™ ", &[]),
            ("Word ... ", "Word … ", &[]),
            ("Word teh ", "Word the ", &[]),
            ("Word Teh ", "Word The ", &[]),
            ("Word TEH ", "Word THE ", &[]),
            ("Word teh, ", "Word the, ", &[]),
            ("Word alot ", "Word a lot ", &[]),
            ("Word dont ", "Word don\u{2019}t ", &[]),
            ("Word cant ", "Word cant ", &[]),
            ("Word i ", "Word I ", &[]),
            ("the cat. the dog ", "The cat. The dog ", &[]),
            ("see e.g. the dog ", "See e.g. the dog ", &[]),
            ("x ", "X ", &[]),
        ];
        for (typed, want, sup) in cases {
            let (got, s) = type_out(typed, &ac);
            assert_eq!(got, *want, "typing {typed:?}");
            assert_eq!(s, sup.iter().map(|&(a, b)| a..b).collect::<Vec<_>>(), "superscript in {typed:?}");
        }
    }

    #[test]
    fn rules_switch_off_and_never_correct_keeps_a_word() {
        let off = AutoCorrect { replace_text: false, sentence_capitals: false, ..AutoCorrect::default() };
        assert_eq!(type_out("teh (c) ", &off).0, "teh (c) ");
        let never = AutoCorrect { never: vec!["teh".into(), "(c)".into()], ..AutoCorrect::default() };
        assert_eq!(type_out("Word teh (c) ", &never).0, "Word teh (c) ");
        // A paragraph ended with Enter corrects its last word.
        let words: Vec<char> = "Word word--".chars().collect();
        let f = correction(&words, Trigger::ParagraphEnd, false, &AutoCorrect::default()).expect("a dash");
        assert_eq!(f.with, "word\u{2014}");
        // A table cell's first word.
        let cell = AutoCorrect { sentence_capitals: false, ..AutoCorrect::default() };
        assert_eq!(correction(&"hello ".chars().collect::<Vec<_>>(), Trigger::Typed, true, &cell).map(|f| f.with), Some("Hello".into()));
    }
}
