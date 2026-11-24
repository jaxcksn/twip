//! Find patterns: special characters, match options, formatting and regular expressions, over one paragraph's text

use std::cell::Cell;
use std::rc::Rc;

use crate::docx::{BreakKind, Inline, NoteKind, Paragraph};
use crate::layout::Unit;

use super::text::{OBJECT, Seen};

/// The longest Find what (and Replace with) Word takes, in characters.
pub const MAX_TEXT: usize = 255;
/// The largest compiled regular expression, in bytes of its program. Matching costs time in proportion to it for
/// each character scanned (about 0.15 ns per byte of program, release), so a search is charged by it too.
pub const REGEX_SIZE: usize = 32 * 1024;
/// The program bytes one unit of [`REGEX_BUDGET`] pays for, per character scanned: a program of up to this many
/// bytes (`a`, `\w+`) is charged one per character, a larger one in proportion.
pub const REGEX_WEIGHT: usize = 256;
/// The deepest nesting a regular expression may have.
pub const REGEX_NEST: u32 = 64;
/// The characters one search's matching may step over in all, over the whole document (white space and passed-over
/// characters can make each start's match run long), before its remaining matches are given up.
pub const MATCH_BUDGET: usize = 1 << 25;
/// The characters one regular expression search may scan in all, over the whole document, each weighted by the
/// program's size ([`REGEX_WEIGHT`]), before its remaining matches are given up: about 6 s of matching at most
/// (release), whatever the expression. Each search from a place is charged to its paragraph's end, the most it can
/// scan (leftmost-first matching may scan past the match it returns), so dense matches in very long paragraphs
/// charge quadratically.
pub const REGEX_BUDGET: usize = 1 << 27;
/// The most units one replace may copy in all, over every match: the text found (`^&`), a regular expression's
/// groups and the clipboard (`^c`), each as often as the replacement names it.
pub const MAX_EXPANSION: usize = 1 << 22;
/// The most pieces one replace may paste in all past each match's first, over every match: each `^&`, group and
/// `^c` is a copy and an edit of its own, whatever its length; one per match costs what typing the replacement does.
pub const MAX_PASTES: usize = 1 << 16;

/// Why a Find what or Replace with can't be searched.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SearchError {
    /// Longer than [`MAX_TEXT`].
    TooLong,
    /// A `^` code Word doesn't know (the code, `^` included).
    Code(String),
    /// A code that only Replace with takes (`^&`, `^c`) in Find what, or one only Find what takes in Replace with.
    Misplaced(String),
    /// The regular expression doesn't compile (the reason).
    Regex(String),
    /// The replacements would copy more than [`MAX_EXPANSION`] characters or more than [`MAX_PASTES`] pieces past each match's first.
    Expansion,
}

impl std::fmt::Display for SearchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SearchError::TooLong => write!(f, "longer than {MAX_TEXT} characters"),
            SearchError::Code(c) => write!(f, "{c} is not a valid special character"),
            SearchError::Misplaced(c) => write!(f, "{c} can't be used here"),
            SearchError::Regex(why) => write!(f, "not a valid regular expression: {why}"),
            SearchError::Expansion => write!(f, "the replacements would copy more than {MAX_EXPANSION} characters or {MAX_PASTES} extra pieces"),
        }
    }
}

/// One character (or, for white space and a field, a stretch) a Find what matches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Token {
    /// The character, by case as the search says.
    Char(char),
    /// `^?`: any character.
    Any,
    /// `^#`: a digit.
    Digit,
    /// `^$`: a letter.
    Letter,
    /// `^w`: one or more spaces, non-breaking spaces and tabs.
    White,
    /// `^t`.
    Tab,
    /// `^l`: a manual line break.
    LineBreak,
    /// `^m`: a manual page break.
    PageBreak,
    /// `^12`: a manual page break, or a section break (between a section's last paragraph and the next's first).
    PageOrSection,
    /// `^n`: a column break.
    ColumnBreak,
    /// `^s`.
    NonBreakingSpace,
    /// `^~`.
    NonBreakingHyphen,
    /// `^-`.
    OptionalHyphen,
    /// `^f`: a footnote reference.
    Footnote,
    /// `^e`: an endnote reference.
    Endnote,
    /// `^g`: an inline picture.
    Graphic,
    /// `^d`: a whole field, begin to end.
    Field,
    /// `^a`: a comment's reference.
    Comment,
}

/// Where a Find what crosses from one paragraph into the next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sep {
    /// `^p`: a paragraph mark, to the next paragraph of the same story or cell.
    Paragraph,
    /// `^b`: a section break, from a section's last paragraph to the next section's first.
    Section,
}

/// A parsed Find what: the tokens of each paragraph's part, and the marks between them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Needle {
    /// The tokens of each paragraph's part: one more than `seps`.
    pub parts: Vec<Vec<Token>>,
    /// The marks between the parts.
    pub seps: Vec<Sep>,
}

impl Needle {
    /// Nothing to find.
    pub fn is_empty(&self) -> bool {
        self.seps.is_empty() && self.parts.iter().all(Vec::is_empty)
    }

    /// Whether a token is one `f` takes.
    pub fn has(&self, f: impl Fn(Token) -> bool) -> bool {
        self.parts.iter().flatten().any(|&t| f(t))
    }
}

/// A piece of a parsed Replace with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Piece {
    /// Text; tabs as `\t`.
    Text(String),
    /// `^p`: a paragraph mark.
    Paragraph,
    /// `^l`, `^m`, `^n`.
    Break(BreakKind),
    /// `^&`: the text found.
    Found,
    /// A copy of units `a..b` of the paragraph the match is in (a regular expression's group), formatting and
    /// objects kept.
    Copy(Unit, Unit),
    /// `^c`: the clipboard's contents.
    Clipboard,
}

/// The Windows-1252 characters 128 to 159, which `^0nnn` names.
const CP1252: [char; 32] = [
    '\u{20AC}', '\u{81}', '\u{201A}', '\u{192}', '\u{201E}', '\u{2026}', '\u{2020}', '\u{2021}', '\u{2C6}', '\u{2030}', '\u{160}', '\u{2039}',
    '\u{152}', '\u{8D}', '\u{17D}', '\u{8F}', '\u{90}', '\u{2018}', '\u{2019}', '\u{201C}', '\u{201D}', '\u{2022}', '\u{2013}', '\u{2014}',
    '\u{2DC}', '\u{2122}', '\u{161}', '\u{203A}', '\u{153}', '\u{9D}', '\u{17E}', '\u{178}',
];

/// What a `^` code names: a token, a mark between paragraphs, or a replacement piece.
enum Code {
    Token(Token),
    Sep(Sep),
    Piece(Piece),
}

/// The code at the start of `rest` (after its `^`) and how many characters it took.
fn code(rest: &[char]) -> Result<(Code, usize), SearchError> {
    let bad = |n: usize| SearchError::Code(std::iter::once('^').chain(rest.iter().take(n.max(1)).copied()).collect());
    let Some(&c) = rest.first() else { return Err(SearchError::Code("^".into())) };
    let digits = |from: usize, max: usize| rest[from..].iter().take(max).take_while(|c| c.is_ascii_digit()).count();
    let number = |from: usize, n: usize| rest[from..from + n].iter().fold(0u32, |v, d| v * 10 + d.to_digit(10).unwrap_or(0));
    let t = |t: Token| Ok((Code::Token(t), 1));
    match c {
        'p' => Ok((Code::Sep(Sep::Paragraph), 1)),
        'b' => Ok((Code::Sep(Sep::Section), 1)),
        't' => t(Token::Tab),
        '?' => t(Token::Any),
        '#' => t(Token::Digit),
        '$' => t(Token::Letter),
        '^' => t(Token::Char('^')),
        '%' => t(Token::Char('\u{A7}')),
        'v' => t(Token::Char('\u{B6}')),
        'n' => t(Token::ColumnBreak),
        '+' => t(Token::Char('\u{2014}')),
        '=' => t(Token::Char('\u{2013}')),
        'e' => t(Token::Endnote),
        'd' => t(Token::Field),
        'f' => t(Token::Footnote),
        'g' => t(Token::Graphic),
        'l' => t(Token::LineBreak),
        'm' => t(Token::PageBreak),
        '~' => t(Token::NonBreakingHyphen),
        's' => t(Token::NonBreakingSpace),
        '-' => t(Token::OptionalHyphen),
        'w' => t(Token::White),
        'a' => t(Token::Comment),
        '&' => Ok((Code::Piece(Piece::Found), 1)),
        'c' => Ok((Code::Piece(Piece::Clipboard), 1)),
        'u' => {
            let n = digits(1, 7);
            let ch = (n > 0).then(|| number(1, n)).and_then(char::from_u32).ok_or_else(|| bad(n + 1))?;
            Ok((Code::Token(Token::Char(ch)), n + 1))
        }
        '0'..='9' => {
            // `^0nnn` takes four digits, `^nnn` three.
            let n = digits(0, if c == '0' { 4 } else { 3 });
            let v = number(0, n);
            let ansi = c == '0' && n > 1;
            let tok = match v {
                _ if ansi && (128..160).contains(&v) => Token::Char(CP1252[(v - 128) as usize]),
                _ if ansi && v < 256 => Token::Char(char::from_u32(v).ok_or_else(|| bad(n))?),
                9 => Token::Tab,
                11 => Token::LineBreak,
                12 => Token::PageOrSection,
                13 => return Ok((Code::Sep(Sep::Paragraph), n)),
                14 => Token::ColumnBreak,
                30 => Token::NonBreakingHyphen,
                31 => Token::OptionalHyphen,
                160 => Token::NonBreakingSpace,
                1..=255 if !(1..=31).contains(&v) => Token::Char(char::from_u32(v).ok_or_else(|| bad(n))?),
                _ => return Err(bad(n)),
            };
            Ok((Code::Token(tok), n))
        }
        _ => Err(bad(1)),
    }
}

/// A Find what in Word's normal mode: its characters, `^` codes for special characters, `^p` and `^b` between
/// paragraphs.
pub fn parse(text: &str) -> Result<Needle, SearchError> {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() > MAX_TEXT {
        return Err(SearchError::TooLong);
    }
    let mut out = Needle { parts: vec![Vec::new()], seps: Vec::new() };
    let mut k = 0;
    while k < chars.len() {
        if chars[k] != '^' {
            let tok = match chars[k] {
                '\t' => Token::Tab,
                '\u{A0}' => Token::NonBreakingSpace,
                '\u{2011}' => Token::NonBreakingHyphen,
                '\u{AD}' => Token::OptionalHyphen,
                '\u{b}' | '\u{2028}' => Token::LineBreak,
                '\u{c}' => Token::PageOrSection,
                '\u{e}' => Token::ColumnBreak,
                '\r' | '\n' | '\u{2029}' => {
                    out.seps.push(Sep::Paragraph);
                    out.parts.push(Vec::new());
                    k += 1 + usize::from(chars[k] == '\r' && chars.get(k + 1) == Some(&'\n'));
                    continue;
                }
                c => Token::Char(c),
            };
            if let Some(p) = out.parts.last_mut() {
                p.push(tok);
            }
            k += 1;
            continue;
        }
        let (c, n) = code(&chars[k + 1..])?;
        match c {
            Code::Token(t) => out.parts.last_mut().into_iter().for_each(|p| p.push(t)),
            Code::Sep(s) => {
                out.seps.push(s);
                out.parts.push(Vec::new());
            }
            Code::Piece(_) => return Err(SearchError::Misplaced(chars[k..k + 1 + n].iter().collect())),
        }
        k += 1 + n;
    }
    Ok(out)
}

/// A Replace with in Word's normal mode: text, and the `^` codes Replace takes (`^p`, `^t`, `^l`, `^m`, `^n`, `^s`,
/// `^~`, `^-`, `^+`, `^=`, `^%`, `^v`, `^^`, `^&`, `^c` and character codes). `regex`: a regular expression's
/// replacement, its text as given (groups are expanded per match).
pub fn parse_replacement(text: &str, regex: bool) -> Result<Vec<Piece>, SearchError> {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() > MAX_TEXT {
        return Err(SearchError::TooLong);
    }
    let mut out: Vec<Piece> = Vec::new();
    if regex {
        push_text(&mut out, text);
        return Ok(out);
    }
    let push = |out: &mut Vec<Piece>, c: char| match out.last_mut() {
        Some(Piece::Text(s)) => s.push(c),
        _ => out.push(Piece::Text(c.to_string())),
    };
    let mut k = 0;
    while k < chars.len() {
        if chars[k] != '^' {
            match break_of(chars[k]) {
                Some(b) => out.push(Piece::Break(b)),
                None => push(&mut out, chars[k]),
            }
            k += 1;
            continue;
        }
        let (c, n) = code(&chars[k + 1..])?;
        let here = || chars[k..k + 1 + n].iter().collect::<String>();
        match c {
            Code::Sep(Sep::Paragraph) => out.push(Piece::Paragraph),
            Code::Token(Token::LineBreak) => out.push(Piece::Break(BreakKind::Line)),
            Code::Token(Token::PageBreak | Token::PageOrSection) => out.push(Piece::Break(BreakKind::Page)),
            Code::Token(Token::ColumnBreak) => out.push(Piece::Break(BreakKind::Column)),
            Code::Token(Token::Tab) => push(&mut out, '\t'),
            Code::Token(Token::NonBreakingSpace) => push(&mut out, '\u{A0}'),
            Code::Token(Token::NonBreakingHyphen) => push(&mut out, '\u{2011}'),
            Code::Token(Token::OptionalHyphen) => push(&mut out, '\u{AD}'),
            Code::Token(Token::Char(c)) => push(&mut out, c),
            Code::Piece(p) => out.push(p),
            Code::Sep(Sep::Section) | Code::Token(_) => return Err(SearchError::Misplaced(here())),
        }
        k += 1 + n;
    }
    Ok(out)
}

/// The break a raw control character in Replace with stands for, as Find what reads it.
fn break_of(c: char) -> Option<BreakKind> {
    match c {
        '\u{b}' => Some(BreakKind::Line),
        '\u{c}' => Some(BreakKind::Page),
        '\u{e}' => Some(BreakKind::Column),
        _ => None,
    }
}

/// `text` onto `out` as typed pieces: its break characters as [`Piece::Break`], so [`Piece::Text`] carries none.
fn push_text(out: &mut Vec<Piece>, text: &str) {
    let mut rest = text;
    while !rest.is_empty() {
        let (run, b) = match rest.char_indices().find_map(|(i, c)| break_of(c).map(|b| (i, b))) {
            Some((i, b)) => (&rest[..i], Some((i, b))),
            None => (rest, None),
        };
        match out.last_mut() {
            Some(Piece::Text(s)) if !run.is_empty() => s.push_str(run),
            _ if !run.is_empty() => out.push(Piece::Text(run.to_string())),
            _ => {}
        }
        let Some((i, b)) = b else { break };
        out.push(Piece::Break(b));
        rest = &rest[i + 1..];
    }
}

/// Word's white space: spaces (every Unicode space separator), non-breaking spaces and tabs.
pub fn is_white(c: char) -> bool {
    c == '\t' || (c.is_whitespace() && !matches!(c, '\n' | '\r' | '\u{b}' | '\u{c}' | '\u{85}' | '\u{2028}' | '\u{2029}'))
}

/// Punctuation as Word ignores it: Unicode's punctuation blocks (general, CJK and full-width), ASCII and Latin-1
/// punctuation and the common symbols set with it.
pub fn is_punct(c: char) -> bool {
    let u = c as u32;
    c.is_ascii_punctuation()
        || matches!(u, 0xA1 | 0xA7 | 0xAB | 0xB6 | 0xB7 | 0xBB | 0xBF | 0x37E | 0x387)
        || matches!(u, 0x55A..=0x55F | 0x589 | 0x5BE | 0x5C0 | 0x5C3 | 0x5C6 | 0x5F3 | 0x5F4 | 0x60C | 0x60D | 0x61B | 0x61E | 0x61F | 0x66A..=0x66D | 0x6D4)
        || matches!(u, 0x964 | 0x965 | 0x970 | 0xE4F | 0xE5A | 0xE5B | 0x10FB | 0x1361..=0x1368)
        || matches!(u, 0x2010..=0x2027 | 0x2030..=0x205E | 0x207D | 0x207E | 0x208D | 0x208E | 0x2308..=0x230B | 0x2329 | 0x232A)
        || matches!(u, 0x2768..=0x2775 | 0x27C5 | 0x27C6 | 0x27E6..=0x27EF | 0x2983..=0x2998 | 0x29D8..=0x29DB | 0x29FC | 0x29FD)
        || matches!(u, 0x2E00..=0x2E4F | 0x3001..=0x3003 | 0x3008..=0x3011 | 0x3014..=0x301F | 0x3030 | 0x303D | 0x30A0 | 0x30FB)
        || matches!(u, 0xFE10..=0xFE19 | 0xFE30..=0xFE52 | 0xFE54..=0xFE61 | 0xFE63 | 0xFE68 | 0xFE6A | 0xFE6B)
        || matches!(u, 0xFF01..=0xFF03 | 0xFF05..=0xFF0A | 0xFF0C..=0xFF0F | 0xFF1A | 0xFF1B | 0xFF1F | 0xFF20 | 0xFF3B..=0xFF3D | 0xFF3F)
        || matches!(u, 0xFF5B | 0xFF5D | 0xFF5F..=0xFF65)
}

/// How a Find what's characters match: case, whole words, the word edges a match takes, and the characters it
/// passes over.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MatchOptions {
    /// Letters match only in the same case.
    pub case: bool,
    /// A match starts at a word's start and ends at a word's end (whole words only).
    pub whole: bool,
    /// A match starts at a word's start.
    pub prefix: bool,
    /// A match ends at a word's end.
    pub suffix: bool,
    /// Punctuation in the text is passed over, and punctuation in Find what ignored.
    pub ignore_punct: bool,
    /// White space in the text is passed over, and white space in Find what ignored.
    pub ignore_space: bool,
}

impl MatchOptions {
    fn ignored(&self, c: char) -> bool {
        (self.ignore_space && is_white(c)) || (self.ignore_punct && is_punct(c))
    }

    /// `needle` without the characters these options pass over.
    pub fn strip(&self, mut needle: Needle) -> Needle {
        for part in &mut needle.parts {
            part.retain(|t| !matches!(*t, Token::Char(c) if self.ignored(c)) && !(self.ignore_space && *t == Token::White));
        }
        needle
    }
}

/// What a special character in a paragraph's text is: what [`Seen`] shows as an object or a break.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Plain,
    Footnote,
    Endnote,
    Picture,
    Page,
    Column,
    Field,
    Comment,
}

/// The placeholder a collapsed field reads as.
const FIELD: char = '\u{FFFC}';
/// The placeholder a comment's reference reads as.
const COMMENT: char = '\u{FFFC}';

/// A paragraph's text as a search reads it: [`Seen`], with what each object is, each character's units, and which
/// characters have the formatting searched for.
pub struct Hay {
    /// The text.
    pub seen: Seen,
    /// Where each character's units end (a collapsed field's are its whole field's); none when each is one unit.
    ends: Option<Vec<u32>>,
    kinds: Vec<Kind>,
    /// Whether each character has the formatting searched for; none when no formatting is.
    pub fmt: Option<Vec<bool>>,
    /// Whether the paragraph has the paragraph formatting searched for (its mark matches `^p`).
    pub para_ok: bool,
    /// The work left to the search reading it ([`MATCH_BUDGET`], [`REGEX_BUDGET`]), shared by every paragraph it
    /// reads.
    pub budget: Rc<Cell<usize>>,
}

/// What a [`Hay`] needs from its paragraph beyond its text.
#[derive(Clone, Copy, Debug, Default)]
pub struct Needs {
    /// What each object and break is.
    pub kinds: bool,
    /// Each outermost field read as one character.
    pub fields: bool,
    /// Each comment's reference read as a character.
    pub comments: bool,
}

impl Needs {
    /// What `needle` needs.
    pub fn of(needle: &Needle) -> Needs {
        use Token::*;
        Needs {
            kinds: needle.has(|t| matches!(t, Footnote | Endnote | Graphic | PageBreak | PageOrSection | ColumnBreak)),
            fields: needle.has(|t| t == Field),
            comments: needle.has(|t| t == Comment),
        }
    }
}

impl Hay {
    /// `seen` (paragraph `p`'s text as Find reads it), with what `needs` takes added, its work charged to `budget`.
    pub fn new(seen: Seen, p: &Paragraph, needs: Needs, budget: Rc<Cell<usize>>) -> Hay {
        let mut hay = Hay { seen, ends: None, kinds: Vec::new(), fmt: None, para_ok: true, budget };
        if !(needs.kinds || needs.fields || needs.comments) {
            return hay;
        }
        let starts: Vec<u32> = p.inlines.iter().scan(0u32, |at, i| Some(std::mem::replace(at, *at + i.units() as u32))).collect();
        let at_unit = |u: u32| {
            let k = starts.partition_point(|&s| s <= u).saturating_sub(1);
            // The inline holding unit `u`: the last starting at or before it with units.
            (0..=k).rev().find(|&j| starts[j] <= u && p.inlines[j].units() > 0).map(|j| &p.inlines[j])
        };
        let kind = |c: char, u: u32| {
            if c != OBJECT && c != '\u{c}' {
                return Kind::Plain;
            }
            match at_unit(u) {
                Some(Inline::NoteRef { kind: NoteKind::Foot, .. }) => Kind::Footnote,
                Some(Inline::NoteRef { kind: NoteKind::End, .. }) => Kind::Endnote,
                Some(Inline::Picture(..)) => Kind::Picture,
                Some(Inline::Break(BreakKind::Page, _)) => Kind::Page,
                Some(Inline::Break(BreakKind::Column, _)) => Kind::Column,
                _ => Kind::Plain,
            }
        };
        hay.kinds = hay.seen.chars.iter().zip(&hay.seen.units).map(|(&c, &u)| kind(c, u)).collect();
        if needs.fields || needs.comments {
            hay.rebuild(p, &starts, needs);
        }
        hay
    }

    /// Each outermost field read as one character over its whole field (`needs.fields`), each comment's reference
    /// as a character of no units (`needs.comments`).
    fn rebuild(&mut self, p: &Paragraph, starts: &[u32], needs: Needs) {
        let mut fields: Vec<(u32, u32)> = Vec::new();
        if needs.fields {
            let mut outer = None;
            for s in crate::docx::fields::spans(&p.inlines) {
                if outer.is_some_and(|end| s.end < end) {
                    continue;
                }
                outer = Some(s.end);
                fields.push((starts[s.begin], starts[s.end] + 1));
            }
        }
        let comments: Vec<u32> = if needs.comments {
            p.inlines.iter().zip(starts).filter(|(i, _)| matches!(i, Inline::CommentRef { .. })).map(|(_, &u)| u).collect()
        } else {
            Vec::new()
        };
        let old = std::mem::take(&mut self.seen);
        let kinds = std::mem::take(&mut self.kinds);
        let mut seen = Seen::default();
        let (mut ends, mut out_kinds) = (Vec::with_capacity(old.chars.len()), Vec::with_capacity(old.chars.len()));
        let (mut f, mut c) = (0, 0);
        let mut push = |seen: &mut Seen, ch: char, u: u32, e: u32, k: Kind| {
            seen.chars.push(ch);
            seen.units.push(u);
            ends.push(e);
            out_kinds.push(k);
        };
        let mut k = 0;
        let mut breaks = old.breaks.iter().peekable();
        loop {
            let u = old.units.get(k).copied().unwrap_or(u32::MAX);
            // The comments and fields before this character, in unit order.
            loop {
                let next_comment = comments.get(c).copied().filter(|&cu| cu <= u);
                let next_field = fields.get(f).copied().filter(|&(b, _)| b <= u);
                match (next_comment, next_field) {
                    (Some(cu), Some((b, _))) if cu <= b => {
                        push(&mut seen, COMMENT, cu, cu, Kind::Comment);
                        c += 1;
                    }
                    (_, Some((b, e))) => {
                        push(&mut seen, FIELD, b, e, Kind::Field);
                        f += 1;
                        while old.units.get(k).is_some_and(|&x| x < e) {
                            k += 1;
                        }
                        while breaks.peek().is_some_and(|&&x| x <= k) {
                            breaks.next();
                        }
                    }
                    (Some(cu), None) => {
                        push(&mut seen, COMMENT, cu, cu, Kind::Comment);
                        c += 1;
                    }
                    (None, None) => break,
                }
            }
            if k >= old.chars.len() {
                break;
            }
            let u = old.units[k];
            if fields.get(f).is_some_and(|&(b, _)| b <= u) {
                continue;
            }
            while breaks.peek().is_some_and(|&&x| x <= k) {
                breaks.next();
                let at = seen.chars.len();
                if at > 0 && seen.breaks.last() != Some(&at) {
                    seen.breaks.push(at);
                }
            }
            push(&mut seen, old.chars[k], u, u + 1, kinds.get(k).copied().unwrap_or(Kind::Plain));
            k += 1;
        }
        self.seen = seen;
        self.ends = Some(ends);
        self.kinds = out_kinds;
    }

    /// The characters.
    pub fn chars(&self) -> &[char] {
        &self.seen.chars
    }

    fn kind(&self, k: usize) -> Kind {
        self.kinds.get(k).copied().unwrap_or(Kind::Plain)
    }

    fn fmt_ok(&self, k: usize) -> bool {
        self.fmt.as_ref().is_none_or(|f| f.get(k).copied().unwrap_or(false))
    }

    /// Whether characters `a..b` all have the formatting searched for.
    pub fn formatted(&self, a: usize, b: usize) -> bool {
        self.fmt.is_none() || (a..b).all(|k| self.fmt_ok(k))
    }

    /// Whether the characters read at units `a..b` all have the formatting searched for, comments' references aside,
    /// in a paragraph that does.
    pub fn formatted_units(&self, a: Unit, b: Unit) -> bool {
        let (s, t) = (self.seen.units.partition_point(|&u| u < a.0), self.seen.units.partition_point(|&u| u < b.0));
        self.fmt.is_none() || (self.para_ok && (s..t).all(|k| self.fmt_ok(k) || self.kind(k) == Kind::Comment))
    }

    /// Whether character `k` is white space of the formatting searched for.
    pub fn white_at(&self, k: usize) -> bool {
        self.one(Token::White, k, false)
    }

    /// The units characters `a..b` read at.
    pub fn units_of(&self, a: usize, b: usize) -> (Unit, Unit) {
        let Some(ends) = &self.ends else { return self.seen.units_of(a, b) };
        let start = self.seen.units.get(a).copied().unwrap_or_else(|| ends.last().copied().unwrap_or(0));
        let end = b.checked_sub(1).and_then(|k| ends.get(k)).copied().unwrap_or(start);
        (Unit(start), Unit(end.max(start)))
    }

    /// Whether token `t` matches the single character `k`.
    fn one(&self, t: Token, k: usize, case: bool) -> bool {
        let Some(&c) = self.seen.chars.get(k) else { return false };
        if !self.fmt_ok(k) {
            return false;
        }
        let kind = self.kind(k);
        // A collapsed field or a comment's reference is only ever its own token.
        if matches!(kind, Kind::Field | Kind::Comment) {
            return (kind == Kind::Field && t == Token::Field) || (kind == Kind::Comment && t == Token::Comment);
        }
        match t {
            Token::Char(n) => c == n || (!case && c.to_lowercase().eq(n.to_lowercase())),
            Token::Any => true,
            Token::Digit => c.is_numeric(),
            Token::Letter => c.is_alphabetic(),
            Token::White => is_white(c),
            Token::Tab => c == '\t',
            Token::LineBreak => c == '\u{2028}',
            Token::PageBreak | Token::PageOrSection => c == '\u{c}' && kind == Kind::Page,
            Token::ColumnBreak => c == '\u{c}' && kind == Kind::Column,
            Token::NonBreakingSpace => c == '\u{A0}',
            Token::NonBreakingHyphen => c == '\u{2011}' || c == '\u{1E}',
            Token::OptionalHyphen => c == '\u{AD}' || c == '\u{1F}',
            Token::Footnote => kind == Kind::Footnote,
            Token::Endnote => kind == Kind::Endnote,
            Token::Graphic => kind == Kind::Picture,
            Token::Field | Token::Comment => false,
        }
    }

    /// Where token `t` matched at character `k` ends, the characters `o` passes over skipped before it (`skip`: it
    /// isn't its part's first); else where it failed. White space is taken greedily.
    pub fn step(&self, t: Token, mut k: usize, o: &MatchOptions, skip: bool) -> Result<usize, usize> {
        let n = self.seen.chars.len();
        if skip && (o.ignore_punct || o.ignore_space) {
            while k < n && self.fmt_ok(k) && o.ignored(self.seen.chars[k]) && self.kind(k) == Kind::Plain && !self.one(t, k, o.case) {
                k += 1;
            }
        }
        if !self.one(t, k, o.case) {
            return Err(k);
        }
        k += 1;
        if t == Token::White {
            while k < n && self.one(Token::White, k, o.case) {
                k += 1;
            }
        }
        Ok(k)
    }

    /// Where `toks` matched from character `i` ends, the characters `o` passes over skipped between them; `None`
    /// when they don't match there. White space is taken greedily. At most `toks.len()` plus the characters passed
    /// over in steps, charged to the budget.
    pub fn match_at(&self, toks: &[Token], i: usize, o: &MatchOptions) -> Option<usize> {
        if self.spent() {
            return None;
        }
        let mut k = i;
        for (j, &t) in toks.iter().enumerate() {
            match self.step(t, k, o, j > 0) {
                Ok(e) => k = e,
                Err(at) => {
                    self.charge(at - i + 1);
                    return None;
                }
            }
        }
        self.charge(k - i + 1);
        Some(k)
    }

    /// Takes `n` from the budget.
    pub fn charge(&self, n: usize) {
        self.budget.set(self.budget.get().saturating_sub(n));
    }

    /// Whether the search's budget is spent: its remaining matches are given up.
    pub fn spent(&self) -> bool {
        self.budget.get() == 0
    }

    /// Where `toks` match from character `i` to exactly the text's end: the first start from `from` that does.
    pub fn match_to_end(&self, toks: &[Token], from: usize, o: &MatchOptions) -> Option<usize> {
        let n = self.seen.chars.len();
        if toks.is_empty() {
            return Some(n);
        }
        let fixed = !(o.ignore_punct || o.ignore_space) && !toks.contains(&Token::White);
        if fixed {
            let i = n.checked_sub(toks.len()).filter(|&i| i >= from)?;
            return (self.match_at(toks, i, o) == Some(n)).then_some(i);
        }
        // Every start tried: the steps are bounded by MATCH_BUDGET.
        (from..n).find(|&i| self.match_at(toks, i, o) == Some(n))
    }

    /// Whether the characters from `k` are all the formatting searched for, and none is a special token's own.
    pub fn plain_run(&self, k: usize) -> bool {
        self.fmt_ok(k) && !matches!(self.kind(k), Kind::Field | Kind::Comment)
    }
}

/// A text's grapheme clusters by character index, its end included: where the cluster a character is in starts, and
/// the first cluster start at or after it.
pub struct Clusters {
    start: Vec<u32>,
    next: Vec<u32>,
}

impl Clusters {
    /// From whether each character starts a grapheme, then the end, which does.
    pub fn of(starts: &[bool]) -> Clusters {
        let mut start = Vec::with_capacity(starts.len());
        let mut at = 0u32;
        for (k, &s) in starts.iter().enumerate() {
            if s {
                at = k as u32;
            }
            start.push(at);
        }
        let mut next = vec![0u32; starts.len()];
        let mut at = starts.len().saturating_sub(1) as u32;
        for (k, &s) in starts.iter().enumerate().rev() {
            if s {
                at = k as u32;
            }
            next[k] = at;
        }
        Clusters { start, next }
    }

    /// Whether character `k` starts a cluster.
    pub fn starts(&self, k: usize) -> bool {
        self.start[k] as usize == k
    }

    /// Where the cluster character `k` is in starts.
    pub fn start(&self, k: usize) -> usize {
        self.start[k] as usize
    }

    /// The first cluster start at or after character `k`.
    pub fn next(&self, k: usize) -> usize {
        self.next[k] as usize
    }
}

/// The word edges a match takes, on `hay` with clusters `c`: no letter or digit in the grapheme before it (`before`)
/// or after it (`after`).
pub fn edges(hay: &[char], c: &Clusters, i: usize, end: usize, before: bool, after: bool) -> bool {
    let word = |k: usize| hay[k].is_alphanumeric();
    (!before || i == 0 || !word(c.start(i - 1))) && (!after || c.next(end) >= hay.len() || !word(c.next(end)))
}

/// A regular expression search, in place of Word's wildcards.
pub struct Pattern {
    re: regex_lite::Regex,
    /// Budget units per character scanned: the program's size over [`REGEX_WEIGHT`], at least one.
    weight: usize,
}

/// A piece of a regular expression's replacement.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Part {
    /// Text as given.
    Text(String),
    /// The text of the group at this index.
    Group(usize),
}

/// A regular expression's replacement, parsed: text, and the groups `$n`, `$name`, `${n}` and `${name}` name (`$$`
/// is a `$`), as the `regex` crates expand them. A group that doesn't exist reads as nothing.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Template {
    parts: Vec<Part>,
}

impl Pattern {
    /// `text` compiled: `case`, letters match only in their case (ignoring it otherwise for ASCII letters, as the
    /// engine folds no other case). Its size is bounded by [`REGEX_SIZE`] and [`REGEX_NEST`], its text by
    /// [`MAX_TEXT`].
    pub fn new(text: &str, case: bool) -> Result<Pattern, SearchError> {
        if text.chars().count() > MAX_TEXT {
            return Err(SearchError::TooLong);
        }
        let build = |limit: usize| regex_lite::RegexBuilder::new(text).case_insensitive(!case).size_limit(limit).nest_limit(REGEX_NEST).build();
        let re = build(REGEX_SIZE).map_err(|e| SearchError::Regex(e.to_string()))?;
        // Program size: the least limit it builds under (the compiler's size only grows, so building is monotonic in
        // the limit), at most log2(REGEX_SIZE) more builds of a program under REGEX_SIZE.
        let (mut lo, mut hi) = (0, REGEX_SIZE);
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if build(mid).is_ok() {
                hi = mid;
            } else {
                lo = mid + 1;
            }
        }
        Ok(Pattern { re, weight: hi.div_ceil(REGEX_WEIGHT).max(1) })
    }

    /// `with` as this expression's replacement.
    pub fn template(&self, with: &str) -> Template {
        let group = |name: &str| match name.parse::<usize>() {
            Ok(i) => Some(i),
            Err(_) => self.re.capture_names().position(|n| n == Some(name)),
        };
        let mut parts: Vec<Part> = Vec::new();
        let mut text = String::new();
        let mut rest = with;
        while let Some(i) = rest.find('$') {
            text.push_str(&rest[..i]);
            rest = &rest[i + 1..];
            if let Some(after) = rest.strip_prefix('$') {
                text.push('$');
                rest = after;
                continue;
            }
            let (name, len) = if let Some(braced) = rest.strip_prefix('{') {
                match braced.find('}') {
                    Some(e) => (&braced[..e], e + 2),
                    None => ("", 0),
                }
            } else {
                let e = rest.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).unwrap_or(rest.len());
                (&rest[..e], e)
            };
            if len == 0 {
                text.push('$');
                continue;
            }
            rest = &rest[len..];
            if let Some(g) = group(name) {
                if !text.is_empty() {
                    parts.push(Part::Text(std::mem::take(&mut text)));
                }
                parts.push(Part::Group(g));
            }
        }
        text.push_str(rest);
        if !text.is_empty() {
            parts.push(Part::Text(text));
        }
        Template { parts }
    }

    /// The matches in `hay` from character `from`, as character ranges with their replacement (`with` expanded:
    /// text as typed, each group a [`Piece::Copy`] of its units), each of more than nothing, all of the formatting
    /// searched for and taken by `ok`; past one refused, from the character after its start. Each search is charged
    /// to the text's end against the budget; past it, the remaining matches are given up.
    pub fn matches(&self, hay: &Hay, from: usize, with: Option<&Template>, ok: impl Fn(usize, usize) -> bool) -> Vec<(usize, usize, Vec<Piece>)> {
        let text: String = hay.chars().iter().collect();
        // Byte offset of each character, and the end.
        let mut bytes: Vec<usize> = text.char_indices().map(|(b, _)| b).collect();
        bytes.push(text.len());
        let char_of = |b: usize| bytes.partition_point(|&x| x < b);
        let mut out = Vec::new();
        let mut at = bytes.get(from).copied().unwrap_or(text.len());
        while at <= text.len() && !hay.spent() {
            // A search that would overrun what's left isn't started: the budget bounds the time spent, not just
            // the searches begun.
            let cost = (text.len() - at + 1).saturating_mul(self.weight);
            if cost > hay.budget.get() {
                hay.budget.set(0);
                break;
            }
            hay.charge(cost);
            let Some(caps) = self.re.captures_at(&text, at) else { break };
            let Some(m) = caps.get(0) else { break };
            let (a, b) = (char_of(m.start()), char_of(m.end()));
            if a < b && hay.formatted(a, b) && ok(a, b) {
                let mut pieces = Vec::new();
                for part in with.map_or(&[][..], |t| &t.parts) {
                    match part {
                        Part::Text(s) => push_text(&mut pieces, s),
                        Part::Group(g) => {
                            if let Some(c) = caps.get(*g).filter(|c| c.start() < c.end()) {
                                let (x, y) = hay.units_of(char_of(c.start()), char_of(c.end()));
                                pieces.push(Piece::Copy(x, y));
                            }
                        }
                    }
                }
                out.push((a, b, pieces));
                at = m.end();
            } else {
                // Past an empty or refused match: from the character after its start.
                let Some(&next) = bytes.get(char_of(m.start()) + 1) else { break };
                at = next;
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(s: &str) -> Vec<Token> {
        let n = parse(s).expect("parses");
        assert!(n.seps.is_empty());
        n.parts.into_iter().next().unwrap_or_default()
    }

    #[test]
    fn codes_parse() {
        assert_eq!(toks("a^tb"), [Token::Char('a'), Token::Tab, Token::Char('b')]);
        assert_eq!(toks("^?^#^$^w"), [Token::Any, Token::Digit, Token::Letter, Token::White]);
        assert_eq!(toks("^^^+^="), [Token::Char('^'), Token::Char('\u{2014}'), Token::Char('\u{2013}')]);
        assert_eq!(toks("^u8364^0128^65"), [Token::Char('\u{20AC}'), Token::Char('\u{20AC}'), Token::Char('A')]);
        assert_eq!(toks("^9^11^12^14^160"), [Token::Tab, Token::LineBreak, Token::PageOrSection, Token::ColumnBreak, Token::NonBreakingSpace]);
        let n = parse("end.^pStart^13x^by").expect("parses");
        assert_eq!(n.seps, [Sep::Paragraph, Sep::Paragraph, Sep::Section]);
        assert_eq!(n.parts.len(), 4);
        assert_eq!(parse("^q"), Err(SearchError::Code("^q".into())));
        assert_eq!(parse("ab^"), Err(SearchError::Code("^".into())));
        assert_eq!(parse("^&"), Err(SearchError::Misplaced("^&".into())));
        assert_eq!(parse(&"x".repeat(256)), Err(SearchError::TooLong));
    }

    #[test]
    fn replacements_parse() {
        let r = parse_replacement("a^pb^t^&^c^l^^", false).expect("parses");
        assert_eq!(
            r,
            [
                Piece::Text("a".into()),
                Piece::Paragraph,
                Piece::Text("b\t".into()),
                Piece::Found,
                Piece::Clipboard,
                Piece::Break(BreakKind::Line),
                Piece::Text("^".into())
            ]
        );
        assert_eq!(parse_replacement("^?", false), Err(SearchError::Misplaced("^?".into())));
        assert_eq!(parse_replacement("$1^p", true).expect("as given"), [Piece::Text("$1^p".into())]);
        assert_eq!(
            parse_replacement("a\u{b}b\u{c}", false).expect("parses"),
            [Piece::Text("a".into()), Piece::Break(BreakKind::Line), Piece::Text("b".into()), Piece::Break(BreakKind::Page)],
            "raw break characters are breaks, never text"
        );
        assert_eq!(parse_replacement("\u{e}x", true).expect("parses"), [Piece::Break(BreakKind::Column), Piece::Text("x".into())]);
    }

    #[test]
    fn white_space_and_punctuation() {
        assert!(is_white(' ') && is_white('\t') && is_white('\u{A0}') && is_white('\u{3000}'));
        assert!(!is_white('\u{2028}') && !is_white('a'));
        assert!(is_punct(',') && is_punct('\u{201C}') && is_punct('\u{3002}') && is_punct('\u{FF01}') && is_punct('\u{BF}'));
        assert!(!is_punct('a') && !is_punct(' ') && !is_punct('\u{4E00}'));
    }

    fn hay(s: &str) -> Hay {
        let chars: Vec<char> = s.chars().collect();
        let units = (0..chars.len() as u32).collect();
        let budget = Rc::new(Cell::new(MATCH_BUDGET));
        Hay { seen: Seen { chars, units, breaks: Vec::new() }, ends: None, kinds: Vec::new(), fmt: None, para_ok: true, budget }
    }

    #[test]
    fn options_pass_over_what_they_ignore() {
        let h = hay("ice  cream, ice-cream");
        let o = MatchOptions { ignore_space: true, ignore_punct: true, ..Default::default() };
        let n = o.strip(parse("ice cream").expect("parses"));
        assert_eq!(h.match_at(&n.parts[0], 0, &o), Some(10));
        assert_eq!(h.match_at(&n.parts[0], 12, &o), Some(21));
        let plain = MatchOptions::default();
        assert_eq!(h.match_at(&toks("ice^wcream"), 0, &plain), Some(10), "white space greedily");
        assert_eq!(h.match_at(&toks("ice cream"), 0, &plain), None);
    }

    #[test]
    fn a_hostile_regex_is_bounded() {
        // Each search scans to the end before matching one capital: charged so, the rest is given up past the budget.
        let p = Pattern::new(".*[^A-Z]|[A-Z]", true).expect("compiles");
        assert_eq!(p.weight, 2, "a program of a few hundred bytes");
        let h = hay(&"A".repeat(20_000));
        h.budget.set(2_000_000);
        let found = p.matches(&h, 0, None, |_, _| true);
        assert!(h.spent(), "spent");
        assert!((40..60).contains(&found.len()), "gives up the rest past its budget: {}", found.len());
        // A large program costs time per character in proportion to its size, and is charged so.
        let big = Pattern::new("(?:.{0,10}){0,10}!", true).expect("compiles");
        assert!(big.weight > 30, "weighted by its program: {}", big.weight);
        assert_eq!(Pattern::new("a", true).expect("compiles").weight, 1);
        let h = hay(&"x".repeat(4_000_000));
        h.budget.set(REGEX_BUDGET);
        let started = std::time::Instant::now();
        assert!(big.matches(&h, 0, None, |_, _| true).is_empty());
        assert!(h.spent(), "a search past the budget isn't started");
        assert!(started.elapsed() < std::time::Duration::from_secs(5), "{:?}", started.elapsed());
        assert!(matches!(Pattern::new("(?:.{0,50}){0,50}!", true), Err(SearchError::Regex(_))), "past REGEX_SIZE");
        assert!(Pattern::new("(((a)))", true).is_ok());
        assert!(matches!(
            Pattern::new(&format!("{}a{}", "(".repeat(65), ")".repeat(65)), true),
            Err(SearchError::Regex(_)) | Err(SearchError::TooLong)
        ));
        assert!(matches!(Pattern::new("a{1000}{1000}", true), Err(SearchError::Regex(_))), "too big");
    }

    #[test]
    fn regex_groups_expand() {
        let p = Pattern::new(r"(\w+)@(?<host>\w+)", true).expect("compiles");
        let h = hay("mail bob@example now");
        let t = p.template("${host}:$1 $$ $9 $x ${2}");
        let copy = |a, b| Piece::Copy(Unit(a), Unit(b));
        let text = |s: &str| Piece::Text(s.into());
        assert_eq!(p.matches(&h, 0, Some(&t), |_, _| true), [(5, 16, vec![copy(9, 16), text(":"), copy(5, 8), text(" $   "), copy(9, 16)])]);
        assert_eq!(p.template("a$").parts, [Part::Text("a$".into())]);
        assert_eq!(p.template("${1").parts, [Part::Text("${1".into())]);
        let i = Pattern::new("BOB", false).expect("compiles");
        assert_eq!(i.matches(&h, 0, None, |_, _| true).len(), 1, "ASCII case folded");
    }

    #[test]
    fn dense_matches_are_all_found() {
        let p = Pattern::new("a", true).expect("compiles");
        let h = hay(&"a".repeat(10_000));
        h.budget.set(REGEX_BUDGET);
        assert_eq!(p.matches(&h, 0, None, |_, _| true).len(), 10_000);
        assert!(!h.spent());
    }
}
