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
use crate::docx::run_xml;

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
    /// AutoFormat: three or more of `-`, `_`, `=`, `*`, `~` or `#` alone in a paragraph, then Enter, make a border
    /// below it.
    pub border_lines: bool,
    /// AutoFormat: an Internet address, email address or network path, then a space or Enter, becomes a link.
    pub internet_paths: bool,
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
            border_lines: true,
            internet_paths: true,
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

/// The bottom border a border line's character makes: its line style and width in eighths of a point.
pub fn border_line(text: &[char]) -> Option<(crate::docx::props::BorderStyle, u32)> {
    let &c = text.first()?;
    if text.len() < 3 || text.iter().any(|&x| x != c) {
        return None;
    }
    use crate::docx::props::BorderStyle;
    Some(match c {
        '-' => (BorderStyle::Single, 6),
        '_' => (BorderStyle::Single, 12),
        '=' => (BorderStyle::Double, 6),
        '*' => (BorderStyle::Dotted, 24),
        '~' => (BorderStyle::Wave, 6),
        '#' => (BorderStyle::ThinThickThinSmallGap, 24),
        _ => return None,
    })
}

/// The longest address or path taken, in characters.
const MAX_PATH: usize = 2048;

/// Where an Internet address, email address or network path ends `before` (the text up to the space or paragraph end
/// that finishes it): its first character, how many it has (closing punctuation left out) and the link's address.
pub fn internet_path(before: &[char]) -> Option<(usize, usize, String)> {
    let start = before.iter().rposition(|c| c.is_whitespace() || *c == super::text::OBJECT).map_or(0, |i| i + 1);
    if before.len() - start > MAX_PATH {
        return None;
    }
    let word = &before[start..];
    let lead = word.iter().take_while(|c| matches!(c, '(' | '[' | '<' | '"' | '\'' | '\u{201C}' | '\u{2018}')).count();
    let core = &word[lead..];
    let core = &core[..trimmed_len(core)];
    // What it can be is told from the characters: text is made only for an address.
    let after = |prefix: &str| {
        core.len() > prefix.len()
            && core.iter().zip(prefix.chars()).all(|(c, p)| c.to_ascii_lowercase() == p)
            && core[prefix.len()..].iter().any(|c| c.is_alphanumeric())
    };
    let text = || core.iter().collect::<String>();
    let address = if ["http://", "https://", "ftp://", "file://", "mailto:", "news:"].iter().any(|p| after(p)) {
        text()
    } else if after("www.") {
        format!("http://{}", text())
    } else if after("ftp.") {
        format!("ftp://{}", text())
    } else if core.starts_with(&['\\', '\\']) && core.get(2).is_some_and(|c| c.is_alphanumeric()) && core[2..].contains(&'\\') {
        format!("file:///{}", text())
    } else if core.contains(&'@') && is_email(&text()) {
        // The mailbox's own `?`, `%` and `#` are not the URI's delimiters.
        let mut uri = String::from("mailto:");
        for c in core {
            match c {
                '?' => uri.push_str("%3F"),
                '%' => uri.push_str("%25"),
                '#' => uri.push_str("%23"),
                _ => uri.push(*c),
            }
        }
        uri
    } else {
        return None;
    };
    Some((start + lead, core.len(), address))
}

/// How many of `core`'s characters stay once its closing punctuation is dropped: a closing bracket stays when it
/// balances an opening one before it.
fn trimmed_len(core: &[char]) -> usize {
    let mut n = core.len();
    while let Some(&c) = n.checked_sub(1).map(|i| &core[i]) {
        let open = match c {
            ')' => '(',
            ']' => '[',
            '>' => '<',
            _ if ends_word(c) || matches!(c, '"' | '\'' | '\u{201D}' | '\u{2019}') => ' ',
            _ => break,
        };
        if open != ' ' {
            let (opens, closes) = core[..n].iter().fold((0, 0), |(o, k), &x| (o + usize::from(x == open), k + usize::from(x == c)));
            if opens >= closes {
                break;
            }
        }
        n -= 1;
    }
    n
}

/// `local@domain.tld`, as Word takes an email address.
fn is_email(t: &str) -> bool {
    let Some((local, domain)) = t.split_once('@') else { return false };
    !local.is_empty()
        && local.chars().all(|c| {
            c.is_alphanumeric()
                || matches!(c, '.' | '-' | '_' | '+')
                || matches!(c, '!' | '#' | '$' | '%' | '&' | '\'' | '*' | '/' | '=' | '?' | '^' | '`' | '{' | '|' | '}' | '~')
        })
        && domain.chars().all(|c| c.is_alphanumeric() || matches!(c, '.' | '-'))
        && domain.split('.').filter(|l| !l.is_empty()).count() >= 2
        && !domain.starts_with('.')
        && !domain.ends_with('.')
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
            if self.kept(crate::api::command::CommandId::InsertText, &pa, &pb)
                || self.auto_format_kept(crate::api::command::CommandId::SetList, &pa, &pb)
            {
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
        // AutoFormat as you type
        if trigger == Trigger::Typed
            && before.last() == Some(&'\t')
            && let Some(out) = self.tab_indent(caret.story, para, before.len(), fonts)
        {
            return out;
        }
        if trigger == Trigger::ParagraphEnd
            && ac.border_lines
            && let Some((line, size)) = border_line(&before)
        {
            return self.border_line(caret.story, para, before.len(), line, size, fonts);
        }
        if ac.internet_paths
            && let Some(end) = match trigger {
                Trigger::ParagraphEnd => Some(before.len()),
                Trigger::Typed => before.last().filter(|c| c.is_whitespace()).map(|_| before.len() - 1),
            }
            && let Some((start, n, address)) = internet_path(&before[..end])
            && let Some(out) = self.auto_link(caret, para, start..start + n, address, fonts)
        {
            return out;
        }
        let Some(p) = self.index.para(&self.doc, para) else { return Outcome::Unchanged("no paragraph") };
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
                        Some(crate::docx::Inline::Text(t, last)) if last.same_run(r) => t.push(chars[j]),
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
        self.correction =
            Some(super::Correction { para, range, from: fix.from.clone(), to, typed: 0, kind: super::CorrectionKind::Text, applied: None });
        self.commit(tx, None, fonts)
    }

    /// Tab typed at the start of a paragraph with text: a first-line indent of a default tab stop in place of the tab,
    /// or, with one already, the left indent a tab stop further. Its own undo step; none in a list or a hanging indent.
    fn tab_indent(&mut self, story: super::position::StoryId, para: crate::layout::ParaId, at: usize, fonts: &Fonts) -> Option<Outcome> {
        if !self.indent_keys || at != 1 || self.para_list(para).is_some() {
            return None;
        }
        let p = self.index.para(&self.doc, para)?;
        if !matches!(p.inlines.iter().find(|i| i.units() > 0), Some(crate::docx::Inline::Tab(_))) || super::step::para_len(p).0 < 2 {
            return None;
        }
        let (left, first) = (p.style.ind_left, p.style.ind_first);
        if first < 0.0 {
            return None;
        }
        let ind = self.direct_ppr(para, "ind");
        let (pa, pb) = (Position::text(story, para, Unit(0)), Position::text(story, para, Unit(1)));
        if self.kept(crate::api::command::CommandId::InsertText, &pa, &pb)
            || self.auto_format_kept(crate::api::command::CommandId::SetIndents, &pa, &pb)
        {
            return None;
        }
        let tab = self.tab_grid();
        let twips = |pt: f64| ((pt * 20.0).round() as i64).to_string();
        let cx = self.cx();
        let set: Vec<(&str, Option<String>)> = if first > 0.0 {
            vec![("left", Some(twips(super::format::grid_step(left, tab, true)))), ("start", None)]
        } else {
            vec![("firstLine", Some(twips(tab))), ("hanging", None)]
        };
        let mut tx = self.begin("AutoFormat");
        let ok = self.step(&mut tx, Step::Text { para, range: Unit(0)..Unit(1), with: Vec::new(), edges: Default::default() }).is_ok()
            && self.repara(&mut tx, para, &|p, _| p.attrs("ind", &set, &cx));
        if !ok {
            self.abort(tx);
            return Some(Outcome::Unchanged("the indent didn't change"));
        }
        self.selection = Selection::Caret(pa);
        let out = self.commit(tx, None, fonts);
        self.auto_formatted(para, Unit(0)..Unit(0), "\t".into(), super::CorrectionKind::Indent { ind });
        Some(out)
    }

    /// The direct paragraph property `local` of `para` (in its `w:pPr`), as written.
    fn direct_ppr(&self, para: crate::layout::ParaId, local: &str) -> Option<String> {
        let p = self.index.para(&self.doc, para)?;
        crate::docx::write::props::Props::parse(p.style.direct.0.as_deref(), "pPr", &self.cx()).get(local).map(str::to_string)
    }

    /// The AutoFormat change just made, which its AutoCorrect Options undo or stop.
    fn auto_formatted(&mut self, para: crate::layout::ParaId, range: Range<Unit>, from: String, kind: super::CorrectionKind) {
        let mut c = super::Correction { para, range, from, to: String::new(), typed: 0, kind, applied: None };
        c.to = self.text_in(&c).unwrap_or_default();
        c.applied = Self::auto_ppr(&c.kind).map(|local| self.direct_ppr(para, local));
        self.correction = Some(c);
    }

    /// A border line typed, then Enter: the paragraph's `n` characters go and it takes the line as its bottom border.
    fn border_line(
        &mut self,
        story: super::position::StoryId,
        para: crate::layout::ParaId,
        n: usize,
        line: crate::docx::props::BorderStyle,
        size: u32,
        fonts: &Fonts,
    ) -> Outcome {
        use crate::api::command::CommandId as C;
        let (pa, pb) = (Position::text(story, para, Unit(0)), Position::text(story, para, Unit(n as u32)));
        if self.kept(C::InsertText, &pa, &pb) || self.auto_format_kept(C::SetParagraphBorders, &pa, &pb) {
            return Outcome::Unchanged("the line is kept");
        }
        let borders = self.direct_ppr(para, "pBdr");
        let typed = self.index.para(&self.doc, para).map(|p| super::text::UnitText::of(p).text.chars().take(n).collect()).unwrap_or_default();
        let mut tx = self.begin("AutoFormat");
        let line = crate::docx::line_style::border_val(line);
        let bottom = |w: &str| format!("<{w}pBdr><{w}bottom {w}val=\"{line}\" {w}sz=\"{size}\" {w}space=\"1\" {w}color=\"auto\"/></{w}pBdr>");
        let ok = self.step(&mut tx, Step::Text { para, range: Unit(0)..Unit(n as u32), with: Vec::new(), edges: Default::default() }).is_ok()
            && self.repara(&mut tx, para, &|p, w| p.set("pBdr", bottom(w)));
        if !ok {
            self.abort(tx);
            return Outcome::Unchanged("the line stayed");
        }
        let out = self.commit(tx, None, fonts);
        self.auto_formatted(para, Unit(0)..Unit(0), typed, super::CorrectionKind::BorderLine { borders });
        out
    }

    /// An Internet or network path typed, then a space or Enter: `range` of `para` becomes a link to `address`, the
    /// caret where it was. Its own undo step; `None` when it is in a field already or kept.
    fn auto_link(&mut self, caret: Position, para: crate::layout::ParaId, range: Range<usize>, address: String, fonts: &Fonts) -> Option<Outcome> {
        let story = self.index.para_slot(para).map(|s| self.index.story(s.container))?;
        let (a, b) = (Unit(range.start as u32), Unit(range.end as u32));
        let p = self.index.para(&self.doc, para)?;
        let mut at = 0u32;
        let mut starts = Vec::with_capacity(p.inlines.len());
        for i in &p.inlines {
            starts.push(at);
            at += i.units() as u32;
        }
        let unit = |k: usize| starts.get(k).copied().unwrap_or(at);
        if crate::docx::fields::spans(&p.inlines).iter().any(|f| unit(f.begin) < b.0 && a.0 <= unit(f.end)) {
            return None;
        }
        let (pa, pb) = (Position::text(story, para, a), Position::text(story, para, b));
        if self.kept(crate::api::command::CommandId::InsertHyperlink, &pa, &pb) {
            return None;
        }
        let typed = super::text::UnitText::of(p).text.chars().skip(range.start).take(range.len()).collect();
        // One step that undoes to the typing caret and redoes to the caret after the link.
        self.begin_batch(Some("AutoFormat".into()));
        self.selection = Selection::range(pa, pb, super::selection::Granularity::Char);
        let out = self.insert_hyperlink(crate::api::command::Hyperlink { address, ..Default::default() }, fonts);
        let linked = matches!(out, Outcome::Changed(_));
        let field_end = linked.then(|| self.index.para(&self.doc, para).and_then(|p| field_end_at(&p.inlines, a.0))).flatten();
        self.selection = match (self.selection, caret.para_offset()) {
            (Selection::Caret(end), Some((p, off))) if p == para => match end.para_offset() {
                Some((q, e)) => Selection::Caret(Position::text(caret.story, q, Unit(e.0 + off.0 - b.0))),
                None => Selection::Caret(caret),
            },
            _ => Selection::Caret(caret),
        };
        let end = self.end_batch(!matches!(out, Outcome::Refused(_)), fonts);
        let out = if matches!(end, Outcome::Changed(_)) { end } else { out };
        if let Some(end) = field_end {
            self.auto_formatted(para, a..Unit(end), typed, super::CorrectionKind::Hyperlink);
        }
        Some(out)
    }

    pub fn correction_view(&mut self, pages: std::ops::Range<usize>) -> Option<super::view::CorrectionView> {
        let c = self.live_correction()?.clone();
        let story = self.index.para_slot(c.para).map(|s| self.index.story(s.container))?;
        let (a, b) = (Position::text(story, c.para, c.range.start), Position::text(story, c.para, c.range.end));
        // A border or an indent changes no text: its mark is where the paragraph starts.
        let bands = if c.range.is_empty() {
            self.caret_at(a).map(|v| vec![(v.page, super::view::Band::upright(v.rect()))]).unwrap_or_default()
        } else {
            self.range_rects(a, b, pages)
        };
        let near = matches!(c.kind, super::CorrectionKind::BorderLine { .. })
            || self.caret().and_then(|p| p.para_offset()).is_some_and(|(p, at)| p == c.para && at >= c.range.start && at.0 <= c.range.end.0 + 1);
        // Options button
        let button = near
            .then(|| {
                bands.last().map(|&(page, b)| {
                    let (w, h) = (12.0, 9.0);
                    let start = bands.iter().filter(|(pg, o)| *pg == page && o.turn == b.turn).map(|(_, o)| o.rect[0]).fold(f64::MAX, f64::min);
                    let [x, y] = b.turn.at([(start + b.rect[2]) / 2.0, b.rect[3]]);
                    let y = y + 0.75;
                    (page, [x - w / 2.0, y, x + w / 2.0, y + h])
                })
            })
            .flatten();
        let rects = bands.iter().map(|&(page, b)| (page, b.page())).collect();
        Some(super::view::CorrectionView { rects, bands, button })
    }

    pub(super) fn follow_correction(&mut self, typed: usize, edited: bool) {
        let caret = self.caret().and_then(|p| p.para_offset());
        // A border line's paragraph ended with Enter: its mark stays while the caret is in the paragraph after it.
        let after = self.correction.as_ref().filter(|c| matches!(c.kind, super::CorrectionKind::BorderLine { .. })).and_then(|c| {
            let s = self.index.para_slot(c.para)?;
            match self.index.blocks(&self.doc, s.container)?.get(s.block + 1)? {
                crate::docx::Block::Para(q) => Some(crate::layout::ParaId(q.id)),
                crate::docx::Block::Table(_) => None,
            }
        });
        let Some(c) = &mut self.correction else { return };
        let from = if edited { c.range.end } else { c.range.start };
        let keep = caret.is_some_and(|(p, at)| (p == c.para && at >= from) || Some(p) == after);
        c.typed += typed;
        if !keep || c.typed > 14 {
            self.correction = None;
        }
    }

    /// The automatic change just made, while its range still holds what it made.
    fn live_correction(&self) -> Option<&super::Correction> {
        self.correction.as_ref().filter(|c| {
            self.text_in(c).is_some_and(|t| t == c.to)
                && c.applied.as_ref().is_none_or(|a| Self::auto_ppr(&c.kind).is_some_and(|local| self.direct_ppr(c.para, local) == *a))
        })
    }

    /// The paragraph property an AutoFormat change writes.
    fn auto_ppr(kind: &super::CorrectionKind) -> Option<&'static str> {
        match kind {
            super::CorrectionKind::Indent { .. } => Some("ind"),
            super::CorrectionKind::BorderLine { .. } => Some("pBdr"),
            _ => None,
        }
    }

    /// The AutoCorrect replacement just made, while its range still holds the corrected text.
    pub fn correction(&self) -> Option<&super::Correction> {
        self.live_correction().filter(|c| c.kind == super::CorrectionKind::Text)
    }

    /// The AutoFormat change just made (a link, a border line, an indent), while it stands: what its AutoCorrect
    /// Options undo ([`Self::revert_correction`]) or stop ([`Self::stop_auto_format`]).
    pub fn auto_format_change(&self) -> Option<&super::Correction> {
        self.live_correction().filter(|c| c.kind != super::CorrectionKind::Text)
    }

    /// Stop Automatically Creating Hyperlinks (Border Lines, Indents): the AutoFormat change just made undone and its
    /// option turned off.
    pub fn stop_auto_format(&mut self, ac: &mut AutoCorrect, fonts: &Fonts) -> Outcome {
        let Some(kind) = self.auto_format_change().map(|c| c.kind.clone()) else { return Outcome::unavailable("no AutoFormat change") };
        let out = self.revert_correction(fonts);
        if !matches!(out, Outcome::Changed(_)) {
            return out;
        }
        match kind {
            super::CorrectionKind::Hyperlink => ac.internet_paths = false,
            super::CorrectionKind::BorderLine { .. } => ac.border_lines = false,
            super::CorrectionKind::Indent { .. } => self.indent_keys = false,
            super::CorrectionKind::Text => {}
        }
        out
    }

    fn text_in(&self, c: &super::Correction) -> Option<String> {
        let p = self.index.para(&self.doc, c.para)?;
        let t = super::text::UnitText::of(p);
        Some(t.text.chars().skip(c.range.start.0 as usize).take((c.range.end.0 - c.range.start.0) as usize).collect())
    }

    pub fn correction_text(&self) -> Option<String> {
        self.correction().map(|c| c.to.clone())
    }

    /// Change Back (Undo Hyperlink, Undo Border Line, Undo Automatic Indent): the automatic change just made undone, by
    /// Undo right after it, else put back as it was.
    pub fn revert_correction(&mut self, fonts: &Fonts) -> Outcome {
        if self.live_correction().is_none() {
            self.correction = None;
            return Outcome::unavailable("no correction");
        }
        let Some(c) = self.correction.take() else { return Outcome::unavailable("no correction") };
        let out = self.revert_taken(&c, fonts);
        // A revert that could not run leaves the change, so its record stays.
        if !matches!(out, Outcome::Changed(_)) {
            self.correction = Some(c);
        }
        out
    }

    fn revert_taken(&mut self, c: &super::Correction, fonts: &Fonts) -> Outcome {
        let label = if c.kind == super::CorrectionKind::Text { "AutoCorrect" } else { "AutoFormat" };
        if self.history.next_undo() == Some(label) && c.typed == 0 {
            return self.execute(super::commands::Command::Undo(Default::default()), fonts);
        }
        if c.kind != super::CorrectionKind::Text {
            return self.revert_auto_format(c, fonts);
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

    /// An AutoFormat change put back once more was done after it: the link's text plain again, the characters typed in
    /// place of the border, the tab in place of the indent.
    fn revert_auto_format(&mut self, c: &super::Correction, fonts: &Fonts) -> Outcome {
        let Some(story) = self.index.para_slot(c.para).map(|s| self.index.story(s.container)) else { return Outcome::Unchanged("no paragraph") };
        let start = Position::text(story, c.para, c.range.start);
        let caret = self.selection;
        let out = match &c.kind {
            super::CorrectionKind::Hyperlink => {
                self.selection = Selection::Caret(Position::text(story, c.para, Unit(c.range.start.0 + 1)));
                let out = self.remove_hyperlink(fonts);
                if !matches!(out, Outcome::Changed(_)) {
                    self.selection = caret;
                }
                return out;
            }
            super::CorrectionKind::BorderLine { borders } => (c.from.clone(), "pBdr", borders.clone()),
            super::CorrectionKind::Indent { ind } => ("\t".to_string(), "ind", ind.clone()),
            super::CorrectionKind::Text => return Outcome::Unchanged("not an AutoFormat change"),
        };
        let (text, local, was) = out;
        let Some(run) = self.run_at(&start).or_else(|| self.index.para(&self.doc, c.para).map(|p| p.mark.clone())) else {
            return Outcome::Unchanged("no run");
        };
        let with = if text == "\t" { vec![crate::docx::Inline::Tab(run)] } else { vec![crate::docx::Inline::Text(text, run)] };
        let mut tx = self.begin("AutoFormat");
        let ok = self.step(&mut tx, Step::Text { para: c.para, range: c.range.start..c.range.start, with, edges: Default::default() }).is_ok()
            && self.repara(&mut tx, c.para, &|p, _| match &was {
                Some(x) => p.set(local, x.clone()),
                None => p.remove(local),
            });
        if !ok {
            self.abort(tx);
            return Outcome::Unchanged("the change stayed");
        }
        self.commit(tx, None, fonts)
    }

    fn raised(&self, para: crate::layout::ParaId, at: Unit, run: &crate::docx::Run) -> Option<crate::docx::Run> {
        let res = self.doc.resolver.clone()?;
        let cx = self.cx();
        let base = self.run_base(para, at, &res, &cx)?;
        let mut p = crate::docx::write::props::Props::parse(run_xml::rpr(&res, run, &cx).as_deref(), "rPr", &cx);
        let w = cx.w.clone();
        p.set("vertAlign", format!("<{w}vertAlign {w}val=\"superscript\"/>"));
        Some(run_xml::rerun_xml(&res, &base, run, p.xml().as_deref(), &cx))
    }
}

/// The unit after the end of the field that begins at unit `at` of `inlines`.
fn field_end_at(inlines: &[crate::docx::Inline], at: u32) -> Option<u32> {
    let mut u = 0u32;
    for (k, i) in inlines.iter().enumerate() {
        if u == at && matches!(i, crate::docx::Inline::FieldBegin { .. }) {
            let s = crate::docx::fields::span_at(inlines, k)?;
            return Some(inlines[..=s.end].iter().map(|i| i.units() as u32).sum());
        }
        u += i.units() as u32;
        if u > at {
            return None;
        }
    }
    None
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
