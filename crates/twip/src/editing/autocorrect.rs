//! AutoCorrect

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
    pub numbered_lists: bool,
    pub bulleted_lists: bool,
    pub entries: Option<HashMap<String, String>>,
    pub first_letter_exceptions: Option<Vec<String>>,
    pub initial_caps_exceptions: Option<Vec<String>>,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trigger {
    Typed,
    ParagraphEnd,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fix {
    pub range: Range<usize>,
    pub with: String,
    pub superscript: Option<Range<usize>>,
    pub from: String,
}

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

pub fn may_trigger(c: char) -> bool {
    c.is_whitespace() || ends_word(c) || matches!(c, '"' | '\'' | ')' | '>' | '|' | '(' | '=')
}

/// The longest replacement key looked for, in characters.
const MAX_KEY: usize = 64;

/// The most words a replacement key spans.
const KEY_WORDS: usize = 4;

fn ends_word(c: char) -> bool {
    matches!(c, ',' | '.' | ';' | ':' | '!' | '?')
}

pub fn correction(before: &[char], trigger: Trigger, cell_start: bool, ac: &AutoCorrect) -> Option<Fix> {
    let n = before.len();
    let last = *before.last()?;
    let lists = lists();
    if trigger == Trigger::Typed {
        // Smart quotes
        if ac.smart_quotes && matches!(last, '"' | '\'') {
            let prev = before[..n - 1].last().copied();
            let (open, close) = if last == '"' { ('\u{201C}', '\u{201D}') } else { ('\u{2018}', '\u{2019}') };
            if ac.dashes && prev == Some('-') && n >= 3 && before[n - 3].is_whitespace() {
                return Some(Fix { range: n - 2..n, with: format!("\u{2013}{open}"), superscript: None, from: format!("-{last}") });
            }
            let opens = prev.is_none_or(|p| p.is_whitespace() || matches!(p, '(' | '[' | '{' | '\u{2013}' | '\u{2014}' | '\u{c}' | '\u{2028}'));
            return Some(Fix { range: n - 1..n, with: (if opens { open } else { close }).to_string(), superscript: None, from: last.to_string() });
        }
        // Symbols, after a replacement list entry for the whole word
        if ac.replace_text {
            let tail: String = before[n.saturating_sub(4)..].iter().collect::<String>().to_lowercase();
            let symbol = SYMBOLS.iter().filter(|(f, _)| tail.ends_with(f) && f.ends_with(may_trigger)).max_by_key(|(f, _)| f.len());
            if let Some((from, to)) = symbol
                && !ac.never.iter().any(|v| v == from)
            {
                let k = from.chars().count();
                let word = before.iter().rposition(|c| c.is_whitespace() || *c == super::text::OBJECT).map_or(0, |i| i + 1);
                if n - word > k && n - word <= MAX_KEY {
                    let key: String = before[word..].iter().collect();
                    if let Some(to) = replacement(&key, ac).filter(|t| *t != key && !ac.never.contains(&key)) {
                        return Some(Fix { range: word..n, with: to, superscript: None, from: key });
                    }
                }
                return Some(Fix { range: n - k..n, with: (*to).to_string(), superscript: None, from: before[n - k..].iter().collect() });
            }
        }
        if !(last.is_whitespace() || ends_word(last)) {
            return None;
        }
    }
    let end = if trigger == Trigger::Typed { n - 1 } else { n };
    let start = before[..end].iter().rposition(|c| c.is_whitespace() || *c == super::text::OBJECT).map_or(0, |i| i + 1);
    if start >= end {
        return None;
    }
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
    let mut start = start;
    let mut superscript = None;
    let mut prefix = String::new();
    let mut changed = false;
    if !core.is_empty() && !ac.never.contains(&core) {
        // Replacement list, longest key first: the last few words, with their closing punctuation or without
        if ac.replace_text {
            let mut starts = vec![cs];
            let mut s = start;
            while starts.len() < KEY_WORDS && s >= 2 && before[s - 1] == ' ' {
                let p = before[..s - 1].iter().rposition(|c| c.is_whitespace() || *c == super::text::OBJECT).map_or(0, |i| i + 1);
                if p + 1 >= s || ce - p > MAX_KEY {
                    break;
                }
                starts.push(p);
                s = p;
            }
            let typed_end = if trigger == Trigger::Typed && ends_word(last) { n } else { end };
            let found = starts.iter().rev().find_map(|&s| {
                [typed_end, end, ce].into_iter().filter(|&e| e > s && e - s <= MAX_KEY).find_map(|e| {
                    let key: String = before[s..e].iter().collect();
                    let to = replacement(&key, ac).filter(|t| *t != key && !ac.never.contains(&key))?;
                    Some((s, e, to))
                })
            });
            if let Some((s, e, to)) = found {
                with = to;
                range = s..e;
                start = start.min(s);
                changed = true;
            }
        }
        // Fractions
        if ac.fractions
            && let Some(f) = [("1/2", "½"), ("1/4", "¼"), ("3/4", "¾")].iter().find(|(f, _)| *f == with)
        {
            with = f.1.to_string();
            changed = true;
        }
        // Ordinals
        if ac.ordinals
            && let Some(k) = ordinal_suffix(&with)
        {
            superscript = Some(k..with.chars().count());
            changed = true;
        }
        // Year apostrophe
        if ac.smart_quotes && lead == 1 && before[start] == '\u{2018}' && core.chars().all(|c| c.is_ascii_digit()) {
            range = start..ce;
            with = format!("\u{2019}{with}");
            changed = true;
        }
        // Dashes
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
        // Two initial capitals
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
        // Day names
        let lower = with.to_lowercase();
        let day = lower.split(['\'', '\u{2019}']).next().unwrap_or("");
        if ac.day_names && DAYS.contains(&day) && with.starts_with(|c: char| c.is_lowercase()) {
            with = capitalised(&with);
            changed = true;
        }
        // Sentence start
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

/// What the replacement list puts for `key`: its entry, else its lower case entry in the key's capitals.
fn replacement(key: &str, ac: &AutoCorrect) -> Option<String> {
    let lookup = |w: &str| match &ac.entries {
        Some(m) => m.get(w).cloned(),
        None => lists().entries.get(w).map(|s| s.to_string()),
    };
    lookup(key).or_else(|| {
        let to = lookup(&key.to_lowercase())?;
        let first_upper = key.chars().next().is_some_and(char::is_uppercase);
        let all_upper = key.chars().filter(|c| c.is_alphabetic()).all(char::is_uppercase) && key.chars().count() > 1;
        Some(if all_upper {
            to.to_uppercase()
        } else if first_upper {
            capitalised(&to)
        } else {
            to
        })
    })
}

fn capitalised(w: &str) -> String {
    let mut c = w.chars();
    c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
}

fn exception(w: &str, own: Option<&[String]>, shipped: &[&str]) -> bool {
    match own {
        Some(list) => list.iter().any(|e| e.eq_ignore_ascii_case(w)),
        None => shipped.iter().any(|e| e.eq_ignore_ascii_case(w)),
    }
}

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
        if trigger == Trigger::Typed
            && let Some(bullet) = list_start(&before, ac)
            && self.para_list(para).is_none()
        {
            let label = if bullet { "Automatic Bullets" } else { "Automatic Numbering" };
            let n = Unit(before.len() as u32);
            let (pa, pb) = (Position::text(caret.story, para, Unit(0)), Position::text(caret.story, para, n));
            if self.kept(crate::api::command::CommandId::InsertText, &pa, &pb) || self.kept(crate::api::command::CommandId::SetList, &pa, &pb) {
                return Outcome::Unchanged("the marker is kept");
            }
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
        if self.kept(crate::api::command::CommandId::InsertText, &Position::text(caret.story, para, a), &Position::text(caret.story, para, b)) {
            return Outcome::Unchanged("the text is kept");
        }
        let olds = unit_runs(p, a.0..b.0);
        let Some(run) = olds.first().cloned() else { return Outcome::Unchanged("no run") };
        if fix.with == fix.from
            && let Some(s) = &fix.superscript
            && olds.get(s.end - 1).is_some_and(|r| r.vert_align == crate::docx::props::VertAlign::Superscript)
        {
            return Outcome::Unchanged("already corrected");
        }
        let chars: Vec<char> = fix.with.chars().collect();
        let tracked = self.tracking();
        let (range, inlines) = match &fix.superscript {
            Some(s) => {
                let Some(up) = self.raised(para, a, &run) else { return Outcome::Unchanged("no styles") };
                let inlines = [(0..s.start, run.clone()), (s.clone(), up), (s.end..chars.len(), run.clone())]
                    .into_iter()
                    .filter(|(r, _)| !r.is_empty())
                    .map(|(r, run)| crate::docx::Inline::Text(chars[r].iter().collect(), run))
                    .collect();
                (a..b, inlines)
            }
            // The changed characters (the whole word, tracked), each in its old character's run
            None => {
                let from: Vec<char> = fix.from.chars().collect();
                let pre = from.iter().zip(&chars).take_while(|(x, y)| x == y).count();
                let suf = from[pre..].iter().rev().zip(chars[pre..].iter().rev()).take_while(|(x, y)| x == y).count();
                let (old_end, new_end) = (from.len() - suf, chars.len() - suf);
                let source = |j: usize| match j {
                    j if j < pre => j,
                    j if j >= new_end => j - new_end + old_end,
                    j if old_end > pre => j.min(old_end - 1),
                    _ => pre.saturating_sub(1),
                };
                let (lo, hi) = if tracked { (0, chars.len()) } else { (pre, new_end) };
                let mut inlines: Vec<crate::docx::Inline> = Vec::new();
                for j in lo..hi {
                    let r = olds.get(source(j)).unwrap_or(&run);
                    match inlines.last_mut() {
                        Some(crate::docx::Inline::Text(t, last)) if last == r => t.push(chars[j]),
                        _ => inlines.push(crate::docx::Inline::Text(chars[j].to_string(), r.clone())),
                    }
                }
                let range = if tracked { a..b } else { Unit(a.0 + pre as u32)..Unit(a.0 + old_end as u32) };
                (range, inlines)
            }
        };
        let mut tx = self.begin("AutoCorrect");
        let from = tx.maps.len();
        if self.step(&mut tx, Step::Text { para, range, with: inlines, edges: Default::default() }).is_err() {
            self.abort(tx);
            return Outcome::Unchanged("correction failed");
        }
        let map = |u: Unit| match tx.maps[from..].iter().fold(Position::text(caret.story, para, u), |p, m| m.apply(p)).place {
            Place::Text { offset, .. } => offset,
            Place::RowEnd { .. } => u,
        };
        let end = map(b);
        if trigger == Trigger::Typed {
            self.selection = Selection::Caret(Position::text(caret.story, para, map(offset)));
        }
        let range = Unit(end.0.saturating_sub(chars.len() as u32))..end;
        let to = chars.iter().collect();
        self.correction = Some(super::Correction { para, range, from: fix.from.clone(), to, typed: 0 });
        self.commit(tx, None, fonts)
    }

    pub fn correction_view(&mut self, pages: std::ops::Range<usize>) -> Option<super::view::CorrectionView> {
        let c = self.correction()?.clone();
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

    pub(super) fn follow_correction(&mut self, typed: usize, edited: bool) {
        let caret = self.caret().and_then(|p| p.para_offset());
        let Some(c) = &mut self.correction else { return };
        let from = if edited { c.range.end } else { c.range.start };
        let keep = caret.is_some_and(|(p, at)| p == c.para && at >= from);
        c.typed += typed;
        if !keep || c.typed > 14 {
            self.correction = None;
        }
    }

    /// The correction mark, while its range still holds the corrected text.
    pub fn correction(&self) -> Option<&super::Correction> {
        self.correction.as_ref().filter(|c| self.text_in(c).is_some_and(|t| t == c.to))
    }

    fn text_in(&self, c: &super::Correction) -> Option<String> {
        let p = self.index.para(&self.doc, c.para)?;
        let t = super::text::UnitText::of(p);
        Some(t.text.chars().skip(c.range.start.0 as usize).take((c.range.end.0 - c.range.start.0) as usize).collect())
    }

    pub fn correction_text(&self) -> Option<String> {
        self.correction().map(|c| c.to.clone())
    }

    pub fn revert_correction(&mut self, fonts: &Fonts) -> Outcome {
        if self.correction().is_none() {
            self.correction = None;
            return Outcome::unavailable("no correction");
        }
        let Some(c) = self.correction.take() else { return Outcome::unavailable("no correction") };
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

    fn raised(&self, para: crate::layout::ParaId, at: Unit, run: &crate::docx::Run) -> Option<crate::docx::Run> {
        let res = self.doc.resolver.clone()?;
        let cx = self.cx();
        let base = self.run_base(para, at, &res, &cx)?;
        let mut p = crate::docx::write::props::Props::parse(run.direct.0.as_deref(), "rPr", &cx);
        let w = cx.w.clone();
        p.set("vertAlign", format!("<{w}vertAlign {w}val=\"superscript\"/>"));
        Some(res.rerun_xml(&base, run, p.xml().as_deref(), &cx))
    }
}

/// The run of each unit of `range` in `p`, the one before for a unit without one.
fn unit_runs(p: &crate::docx::Paragraph, range: Range<u32>) -> Vec<crate::docx::Run> {
    let mut out = Vec::with_capacity(range.len());
    let mut at = 0u32;
    let mut last = &p.mark;
    for i in &p.inlines {
        if at >= range.end {
            break;
        }
        let n = i.units() as u32;
        let run = super::step::run_of(i).unwrap_or(last);
        out.extend((at.max(range.start)..(at + n).min(range.end)).map(|_| run.clone()));
        last = run;
        at += n;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let words: Vec<char> = "Word word--".chars().collect();
        let f = correction(&words, Trigger::ParagraphEnd, false, &AutoCorrect::default()).expect("a dash");
        assert_eq!(f.with, "word\u{2014}");
        let cell = AutoCorrect { sentence_capitals: false, ..AutoCorrect::default() };
        assert_eq!(correction(&"hello ".chars().collect::<Vec<_>>(), Trigger::Typed, true, &cell).map(|f| f.with), Some("Hello".into()));
    }

    #[test]
    fn shortcodes_multi_word_and_punctuated_entries_replace() {
        let typed = |t: &str| type_out(t, &AutoCorrect::default()).0;
        assert_eq!(typed("Word :alpha: "), "Word \u{3B1} ");
        assert_eq!(typed("Word :Latin cross: "), format!("Word {} ", lists().entries[":Latin cross:"]));
        assert_eq!(typed("Word int he "), "Word in the ");
        assert_eq!(typed("Word their are "), "Word there are ");
        assert_eq!(typed("their are "), "There are ");
        assert_eq!(typed("Word I.e. "), format!("Word {} ", lists().entries["I.e."]));
    }

    #[test]
    fn a_left_right_arrow_is_not_half_converted() {
        let typed = |t: &str| type_out(t, &AutoCorrect::default()).0;
        assert_eq!(typed("<--> "), "\u{2194} ");
        assert_eq!(typed("<-- "), "\u{2190} ");
        assert_eq!(typed("--> "), "\u{2192} ");
        assert_eq!(typed(":-( :( <=="), "\u{2639} \u{2639} \u{21D0}");
    }
}
