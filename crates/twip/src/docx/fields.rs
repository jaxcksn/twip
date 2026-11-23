//! Pagination-independent fields

use std::collections::HashMap;

use super::model::{Block, DATE_BLOCKS, Document, FieldId, FieldKind, FieldView, Inline, NoteKind, Paragraph, Run};

/// A field's markers in a paragraph's inlines: the indexes of its begin, separator (if it has one) and
/// end.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FieldSpan {
    pub begin: usize,
    pub sep: Option<usize>,
    pub end: usize,
}

impl FieldSpan {
    /// Where its code ends: its separator, or its end when it has none.
    pub fn code_end(&self) -> usize {
        self.sep.unwrap_or(self.end)
    }
}

/// Every complete field of `inlines`, by where it begins (an outer field before the fields inside it).
pub fn spans(inlines: &[Inline]) -> Vec<FieldSpan> {
    let mut open: Vec<(FieldId, usize, Option<usize>)> = Vec::new();
    let mut out = Vec::new();
    for (k, i) in inlines.iter().enumerate() {
        match i {
            Inline::FieldBegin { id, .. } => open.push((*id, k, None)),
            Inline::FieldSep { id, .. } => {
                if let Some(o) = open.iter_mut().rev().find(|o| o.0 == *id) {
                    o.2.get_or_insert(k);
                }
            }
            Inline::FieldEnd { id, .. } => {
                if let Some(p) = open.iter().rposition(|o| o.0 == *id) {
                    let (_, begin, sep) = open.remove(p);
                    out.push(FieldSpan { begin, sep, end: k });
                }
            }
            _ => {}
        }
    }
    out.sort_by_key(|s| s.begin);
    out
}

/// The unit ranges, ends included, of the codes (begin to separator) of `inlines`' fields that show their results.
pub fn hidden_codes(inlines: &[Inline]) -> Vec<(u32, u32)> {
    let spans = spans(inlines);
    if spans.is_empty() {
        return Vec::new();
    }
    let starts: Vec<u32> = inlines.iter().scan(0u32, |at, i| Some(std::mem::replace(at, *at + i.units() as u32))).collect();
    spans
        .iter()
        .filter(|s| !matches!(inlines[s.begin], Inline::FieldBegin { view: FieldView::Code, .. }))
        .map(|s| (starts[s.begin], starts[s.code_end()]))
        .collect()
}

/// The span in `spans` (as [`spans`] orders them) of the field beginning at inline `begin`.
pub fn span_in(spans: &[FieldSpan], begin: usize) -> Option<FieldSpan> {
    spans.binary_search_by_key(&begin, |s| s.begin).ok().map(|i| spans[i])
}

/// The span of the field beginning at inline `begin`.
pub fn span_at(inlines: &[Inline], begin: usize) -> Option<FieldSpan> {
    let Some(Inline::FieldBegin { id, .. }) = inlines.get(begin) else { return None };
    let (mut nested, mut sep) = (0usize, None);
    for (k, i) in inlines.iter().enumerate().skip(begin + 1) {
        match i {
            Inline::FieldBegin { id: o, .. } if o == id => nested += 1,
            Inline::FieldSep { id: o, .. } if o == id && nested == 0 => {
                sep.get_or_insert(k);
            }
            Inline::FieldEnd { id: o, .. } if o == id => match nested.checked_sub(1) {
                Some(n) => nested = n,
                None => return Some(FieldSpan { begin, sep, end: k }),
            },
            _ => {}
        }
    }
    None
}

/// A field's own code text: the text from its begin to its separator, the fields nested in it left out.
pub fn code_text(inlines: &[Inline], s: FieldSpan) -> String {
    code_where(inlines, s, |_| true)
}

/// [`code_text`] as `review` shows it: without the revisions it hides.
pub fn code_text_in(inlines: &[Inline], s: FieldSpan, review: &super::review::ReviewView) -> String {
    code_where(inlines, s, |r| !review.hides(&r.rev))
}

fn code_where(inlines: &[Inline], s: FieldSpan, keep: impl Fn(&Run) -> bool) -> String {
    let mut depth = 0usize;
    let mut out = String::new();
    for i in &inlines[s.begin + 1..s.code_end()] {
        match i {
            Inline::FieldBegin { .. } => depth += 1,
            Inline::FieldEnd { .. } => depth = depth.saturating_sub(1),
            Inline::Text(t, r) if depth == 0 && keep(r) => out.push_str(t),
            Inline::Tab(_) | Inline::PTab(..) if depth == 0 && i.run().is_none_or(&keep) => out.push('\t'),
            _ => {}
        }
    }
    out
}

/// A field's code text with each field nested in it as `value` gives it (by its begin), quoted so that it reads as
/// one word; `None` when a nested field gets no value.
pub fn code_with(inlines: &[Inline], s: FieldSpan, value: &dyn Fn(usize) -> Option<String>) -> Option<String> {
    let mut out = String::new();
    let mut k = s.begin + 1;
    while k < s.code_end() {
        match &inlines[k] {
            Inline::FieldBegin { .. } => {
                if let Some(n) = span_at(inlines, k) {
                    out.push('"');
                    out.push_str(&value(k)?);
                    out.push('"');
                    k = n.end + 1;
                    continue;
                }
            }
            Inline::Text(t, _) => out.push_str(t),
            Inline::Tab(_) | Inline::PTab(..) => out.push('\t'),
            _ => {}
        }
        k += 1;
    }
    Some(out)
}

/// The runs a field's value `text` draws in: with `\* MERGEFORMAT`, word by word in its cached result's formatting
/// (word i, and the white space before it, as cached word i and the white space before that), the words past the
/// cached ones in `run`; without it, all in `run`.
pub fn value_runs(inlines: &[Inline], s: FieldSpan, text: &str, run: &Run) -> Vec<(String, Run)> {
    let merge = parse(&code_text(inlines, s))
        .switches
        .iter()
        .any(|w| w.flag == '*' && w.arg.as_deref().is_some_and(|a| a.eq_ignore_ascii_case("MERGEFORMAT")));
    let Some(sep) = s.sep.filter(|_| merge) else { return vec![(text.to_string(), run.clone())] };
    // The cached words: the run of the white space before each, and of its first other character.
    let part = &inlines[sep + 1..s.end];
    let mut cached: Vec<(Option<Run>, Option<Run>)> = Vec::new();
    let mut in_word = false;
    for (i, sh) in part.iter().zip(shown(part)) {
        let (t, r) = match (&sh, i) {
            (Shown::Value(v), _) => (v.as_str(), i.run()),
            (Shown::Itself, Inline::Text(t, r)) => (t.as_str(), Some(r)),
            _ => continue,
        };
        let Some(r) = r else { continue };
        for c in t.chars() {
            match (c.is_whitespace(), in_word) {
                (true, true) | (true, false) if cached.last().is_none_or(|w| w.1.is_some()) => {
                    cached.push((Some(r.clone()), None));
                    in_word = false;
                }
                (false, false) => {
                    match cached.last_mut() {
                        Some(w) if w.1.is_none() => w.1 = Some(r.clone()),
                        _ => cached.push((None, Some(r.clone()))),
                    }
                    in_word = true;
                }
                _ => {}
            }
        }
    }
    let mut out: Vec<(String, Run)> = Vec::new();
    let (mut word, mut in_word) = (0usize, false);
    let mut started = false;
    for c in text.chars() {
        let space = c.is_whitespace();
        if (space && in_word) || (!space && !in_word && !started) {
            if started {
                word += 1;
            }
            started = true;
        }
        in_word = !space;
        let w = cached.get(word);
        let r = w.and_then(|w| if space { w.0.clone().or(w.1.clone()) } else { w.1.clone().or(w.0.clone()) }).unwrap_or_else(|| run.clone());
        match out.last_mut() {
            Some((t, last)) if std::rc::Rc::ptr_eq(last, &r) || **last == *r => t.push(c),
            _ => out.push((c.to_string(), r)),
        }
    }
    out
}

/// A field's result as shown: its result's text, the fields nested in it as theirs.
pub fn result_text(inlines: &[Inline], s: FieldSpan) -> String {
    let Some(sep) = s.sep else { return String::new() };
    visible(&inlines[sep + 1..s.end])
}

/// How results show an inline, whatever the fields' views.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Shown {
    Itself,
    /// A marker, a code, or the result of an evaluated field.
    Not,
    /// An evaluated field's begin marker: the field shows this (its value, or its result's text).
    Value(String),
}

/// How results show each of `inlines`.
pub fn shown(inlines: &[Inline]) -> Vec<Shown> {
    show(inlines, false, &|_| true)
}

/// How layout shows each of `inlines`: each field in its view. Showing its code, a field's begin
/// and end markers are its braces and its result isn't drawn; showing its result, an empty one shows its label.
pub fn layout_shown(inlines: &[Inline]) -> Vec<Shown> {
    show(inlines, true, &|_| true)
}

/// How deep evaluated fields without a value show their results' own fields: below that, such a field shows nothing.
/// A document nests fields with flat begin and end marks, so the depth is the file's to choose.
const MAX_SHOWN_DEPTH: usize = 32;

/// How each of `inlines` shows; an evaluated field without a value shows the runs of its result that `keep` takes.
fn show(inlines: &[Inline], views: bool, keep: &dyn Fn(&Run) -> bool) -> Vec<Shown> {
    show_at(inlines, views, keep, 0)
}

/// [`show`] `depth` results down.
fn show_at(inlines: &[Inline], views: bool, keep: &dyn Fn(&Run) -> bool, depth: usize) -> Vec<Shown> {
    let mut out = vec![Shown::Itself; inlines.len()];
    for s in spans(inlines) {
        // Outer fields come first: a field inside a part that isn't drawn isn't drawn either.
        let visible = out[s.begin] == Shown::Itself;
        let Inline::FieldBegin { kind, view, label, value, .. } = &inlines[s.begin] else { continue };
        let not = |out: &mut Vec<Shown>, r: std::ops::RangeInclusive<usize>| r.for_each(|k| out[k] = Shown::Not);
        if views && *view == FieldView::Code {
            not(&mut out, s.code_end()..=s.end);
            if visible {
                out[s.begin] = Shown::Value("{".into());
                out[s.end] = Shown::Value("}".into());
            } else {
                out[s.begin] = Shown::Not;
            }
            continue;
        }
        not(&mut out, s.begin..=s.code_end());
        out[s.end] = Shown::Not;
        if kind.is_some() {
            not(&mut out, s.code_end()..=s.end);
            if visible {
                out[s.begin] = Shown::Value(value.clone().unwrap_or_else(|| match s.sep {
                    Some(sep) if depth < MAX_SHOWN_DEPTH => visible_at(&inlines[sep + 1..s.end], keep, depth + 1),
                    _ => String::new(),
                }));
            }
        } else if let (true, true, Some(l)) = (views, visible, label)
            && s.sep.is_none_or(|sep| sep + 1 == s.end)
        {
            out[s.begin] = Shown::Value(l.to_string());
        }
    }
    out
}

/// The text of `inlines` as results show it: codes left out, an evaluated field as its value (or its result).
pub fn visible(inlines: &[Inline]) -> String {
    visible_where(inlines, &|_| true)
}

/// [`visible`] as `review` draws it: without hidden text and the revisions it hides.
pub fn visible_in(inlines: &[Inline], review: &super::review::ReviewView) -> String {
    visible_where(inlines, &|r| !r.hidden && !review.hides(&r.rev))
}

/// [`visible`] of the inlines whose runs `keep` takes.
fn visible_where(inlines: &[Inline], keep: &dyn Fn(&Run) -> bool) -> String {
    visible_at(inlines, keep, 0)
}

/// [`visible_where`] `depth` results down.
fn visible_at(inlines: &[Inline], keep: &dyn Fn(&Run) -> bool, depth: usize) -> String {
    let mut out = String::new();
    for (i, s) in inlines.iter().zip(show_at(inlines, false, keep, depth)) {
        if i.run().is_none_or(keep) {
            push_shown(&mut out, s, i);
        }
    }
    out
}

/// Inline `i`'s text, shown as `s`, onto `out`.
fn push_shown(out: &mut String, s: Shown, i: &Inline) {
    match (s, i) {
        (Shown::Value(v), _) => out.push_str(&v),
        (Shown::Itself, Inline::Text(t, _)) => out.push_str(t),
        (Shown::Itself, Inline::Tab(_) | Inline::PTab(..)) => out.push('\t'),
        (Shown::Itself, Inline::Sym { ch, .. }) => out.push(*ch),
        _ => {}
    }
}

/// A new field's inlines: begin, its code, separator, its display text, end, all in `run`.
pub fn field_inlines(code: &str, display: &str, run: &Run) -> Vec<Inline> {
    field_inlines_with(FieldId::next(), code, display, run)
}

/// [`field_inlines`] with the field's id given.
pub fn field_inlines_with(id: FieldId, code: &str, display: &str, run: &Run) -> Vec<Inline> {
    let kind = parse(code).kind();
    let mut v = vec![Inline::FieldBegin {
        id,
        kind,
        lock: false,
        dirty: false,
        view: FieldView::Result,
        label: None,
        value: None,
        form: None,
        run: run.clone(),
    }];
    if !code.is_empty() {
        v.push(Inline::Text(code.into(), run.clone()));
    }
    v.push(Inline::FieldSep { id, run: run.clone() });
    if !display.is_empty() {
        v.push(Inline::Text(display.into(), run.clone()));
    }
    v.push(Inline::FieldEnd { id, run: run.clone() });
    v
}

/// A field code's name, arguments and switches: Word's field syntax, quoted arguments keeping their
/// spaces and `\"` escapes. The fields nested in a code aren't in its text, so they aren't here either.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldCode {
    /// Upper-cased.
    pub name: String,
    pub args: Vec<String>,
    pub switches: Vec<Switch>,
}

/// A field switch: `\@ "M/d/yyyy"` is `@` with an argument; `\* MERGEFORMAT` is `*` with one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Switch {
    pub flag: char,
    pub arg: Option<String>,
}

const FIELD_NAMES: &str = "ADDRESSBLOCK ADVANCE ASK AUTHOR AUTONUM AUTONUMLGL AUTONUMOUT AUTOTEXT AUTOTEXTLIST BARCODE BIBLIOGRAPHY \
    BIDIOUTLINE CITATION COMMENTS COMPARE CONTROL CREATEDATE DATA DATABASE DATE DISPLAYBARCODE DOCPROPERTY DOCVARIABLE \
    EDITTIME EMBED EQ FILENAME FILESIZE FILLIN FORMCHECKBOX FORMDROPDOWN FORMTEXT GLOSSARY GOTOBUTTON GREETINGLINE \
    HYPERLINK IF IMPORT INCLUDE INCLUDEPICTURE INCLUDETEXT INDEX INFO KEYWORDS LASTSAVEDBY LINK LISTNUM MACROBUTTON \
    MERGEBARCODE MERGEFIELD MERGEREC MERGESEQ NEXT NEXTIF NOTEREF NUMCHARS NUMPAGES NUMWORDS PAGE PAGEREF PRINT PRINTDATE \
    PRIVATE QUOTE RD REF REVNUM SAVEDATE SECTION SECTIONPAGES SEQ SET SHAPE SKIPIF STYLEREF SUBJECT SYMBOL TA TC TEMPLATE \
    TIME TITLE TOA TOC USERADDRESS USERINITIALS USERNAME XE";

/// A field argument quoted so that `parse` reads it back unchanged.
pub(crate) fn quote(v: &str) -> String {
    format!("\"{}\"", v.replace('\\', "\\\\").replace('"', "\\\""))
}

/// A `HYPERLINK` field code for an external address.
pub(crate) fn link_code(address: &str) -> String {
    format!(" HYPERLINK {} ", quote(address))
}

/// Parses a field code. An empty code gives an empty name.
pub fn parse(code: &str) -> FieldCode {
    parse_spans(code, None)
}

/// [`parse`], with each argument's place in `code` (its quotes too) in `spans`.
pub fn parse_spans(code: &str, mut spans: Option<&mut Vec<std::ops::Range<usize>>>) -> FieldCode {
    // Tokens: (text, quoted, place).
    let mut tokens: Vec<(String, bool, std::ops::Range<usize>)> = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut start = 0;
    let mut chars = code.char_indices().peekable();
    while let Some((at, c)) = chars.next() {
        match c {
            '\\' if quoted && chars.peek().is_some_and(|&(_, n)| matches!(n, '"' | '\\')) => {
                cur.extend(chars.next().map(|(_, c)| c));
            }
            '"' => {
                if quoted {
                    tokens.push((std::mem::take(&mut cur), true, start..at + 1));
                } else if !cur.is_empty() {
                    tokens.push((std::mem::take(&mut cur), false, start..at));
                }
                quoted = !quoted;
                start = at;
            }
            c if c.is_whitespace() && !quoted => {
                if !cur.is_empty() {
                    tokens.push((std::mem::take(&mut cur), false, start..at));
                }
            }
            c => {
                if cur.is_empty() && !quoted {
                    start = at;
                }
                cur.push(c);
            }
        }
    }
    if !cur.is_empty() {
        tokens.push((cur, quoted, start..code.len()));
    }
    let mut it = tokens.into_iter().peekable();
    let name = it.next().map(|(t, ..)| t.to_uppercase()).unwrap_or_default();
    let (mut args, mut switches) = (Vec::new(), Vec::new());
    while let Some((t, q, at)) = it.next() {
        let flag = (!q).then(|| t.strip_prefix('\\')).flatten().and_then(|f| f.chars().next());
        match flag {
            Some(flag) => {
                let rest: String = t.chars().skip(2).collect();
                // `\@"M/d"` and `\@ "M/d"` alike; a flag alone takes the next token unless it is a switch too.
                let arg = if !rest.is_empty() {
                    Some(rest)
                } else if it.peek().is_some_and(|(n, nq, _)| *nq || !n.starts_with('\\')) {
                    it.next().map(|(n, ..)| n)
                } else {
                    None
                };
                switches.push(Switch { flag, arg });
            }
            None if switches.is_empty() => {
                args.push(t);
                if let Some(s) = spans.as_deref_mut() {
                    s.push(at);
                }
            }
            None => {}
        }
    }
    FieldCode { name, args, switches }
}

/// `arg` as a field code argument: quoted when it holds spaces, quotes or a leading backslash, `None` for one no
/// argument can hold (empty).
pub fn quote_arg(arg: &str) -> Option<String> {
    if arg.is_empty() {
        return None;
    }
    if !arg.contains(|c: char| c.is_whitespace() || c == '"') && !arg.starts_with('\\') {
        return Some(arg.to_string());
    }
    Some(quote(arg))
}

impl FieldCode {
    pub fn kind(&self) -> Option<FieldKind> {
        let arg = || self.args.first().cloned().unwrap_or_default();
        Some(match self.name.as_str() {
            "PAGE" => FieldKind::Page,
            "NUMPAGES" => FieldKind::NumPages,
            "SECTIONPAGES" => FieldKind::SectionPages,
            "SECTION" => FieldKind::Section,
            "PAGEREF" => FieldKind::PageRef(arg()),
            "REF" => FieldKind::Ref(arg()),
            "SEQ" => FieldKind::Seq(arg()),
            "STYLEREF" => FieldKind::StyleRef(arg()),
            "DATE" | "TIME" => FieldKind::Date,
            "DOCPROPERTY" => FieldKind::DocProperty(arg()),
            "DOCVARIABLE" => FieldKind::DocVariable(arg()),
            "AUTHOR" => FieldKind::DocProperty("Author".into()),
            "TITLE" => FieldKind::DocProperty("Title".into()),
            "SUBJECT" => FieldKind::DocProperty("Subject".into()),
            "KEYWORDS" => FieldKind::DocProperty("Keywords".into()),
            "COMMENTS" => FieldKind::DocProperty("Comments".into()),
            "LASTSAVEDBY" => FieldKind::DocProperty("LastSavedBy".into()),
            "REVNUM" => FieldKind::DocProperty("RevisionNumber".into()),
            "CREATEDATE" => FieldKind::DocDate("created"),
            "SAVEDATE" => FieldKind::DocDate("modified"),
            "PRINTDATE" => FieldKind::DocDate("lastPrinted"),
            "FILENAME" => FieldKind::FileName,
            "NOTEREF" => FieldKind::NoteRef(arg(), None),
            "LISTNUM" => FieldKind::ListNum,
            "AUTONUM" => FieldKind::AutoNum,
            n if n.starts_with('=') => FieldKind::Formula,
            "IF" => FieldKind::If,
            n if !n.is_empty()
                && !n.starts_with(['=', '\\'])
                && (!n.chars().all(|c| c.is_alphanumeric() || c == '_') || n.starts_with(|c: char| c.is_numeric())) =>
            {
                FieldKind::Invalid
            }
            n if !n.is_empty()
                && !FIELD_NAMES.split_whitespace().any(|f| f == n)
                && n.starts_with(|c: char| c.is_alphabetic() || c == '_')
                && n.chars().all(|c| c.is_alphanumeric() || c == '_') =>
            {
                FieldKind::Bookmark(n.into())
            }
            _ => return None,
        })
    }

    /// What an empty result shows while designing: «first argument».
    pub fn label(&self) -> String {
        format!("«{}»", self.args.first().map_or(self.name.as_str(), String::as_str))
    }
}

/// The kind of the field beginning at `begin`, from its code.
pub fn kind_at(inlines: &[Inline], s: FieldSpan) -> Option<FieldKind> {
    parse(&code_text(inlines, s)).kind()
}

pub(crate) fn paras(blocks: &mut [Block], f: &mut dyn FnMut(&mut Paragraph)) {
    for b in blocks {
        match b {
            Block::Para(p) => {
                f(p);
                // Text boxes anchored in the paragraph hold fields too.
                for i in &mut p.inlines {
                    if let Some(tb) = i.story_mut() {
                        paras(&mut tb.blocks, f);
                    }
                }
            }
            Block::Table(t) => {
                for r in &mut t.rows {
                    for c in &mut r.cells {
                        paras(&mut c.blocks, f);
                    }
                }
            }
        }
    }
}

/// A paragraph's visible text (fields as their current results).
pub fn text(p: &Paragraph) -> String {
    visible(&p.inlines)
}

/// Style ids compare without case and spaces, so a STYLEREF's "Heading 1" finds `Heading1`.
pub fn style_key(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).flat_map(char::to_lowercase).collect()
}

/// What a `STYLEREF` can show from a paragraph, `(style key, a character style, text)`: its text under its paragraph
/// style, then each stretch of runs in one character style. Hidden text is left out, white space trimmed, at most 256
/// characters kept.
pub fn style_hits(p: &Paragraph) -> Vec<(String, bool, String)> {
    let clean = |t: &str| t.trim().chars().take(256).collect::<String>();
    let (mut all, mut out, mut cur) = (String::new(), Vec::new(), None::<(String, String)>);
    for (i, s) in p.inlines.iter().zip(shown(&p.inlines)) {
        let Some(run) = i.run().filter(|r| !r.hidden) else { continue };
        let t = match (&s, i) {
            (Shown::Value(v), _) => v.as_str(),
            (Shown::Itself, Inline::Text(t, _)) => t,
            (Shown::Itself, Inline::Tab(_) | Inline::PTab(..)) => "\t",
            _ => continue,
        };
        all.push_str(t);
        let key = run.style_id.as_deref().map(style_key);
        if cur.as_ref().map(|c| &c.0) != key.as_ref() {
            out.extend(cur.take().map(|(k, t)| (k, true, clean(&t))));
            cur = key.map(|k| (k, String::new()));
        }
        if let Some(c) = &mut cur {
            c.1.push_str(t);
        }
    }
    out.extend(cur.map(|(k, t)| (k, true, clean(&t))));
    out.insert(0, (p.style.style_id.as_deref().map(style_key).unwrap_or_default(), false, clean(&all)));
    out
}

/// What a `STYLEREF` looks for: the style it names (by id or name, ignoring case and spaces; `1` to `9` name the
/// built-in headings), whether it is a character style, `\l` (the last one on the page), and the error text it shows
/// when nothing in the document has it.
#[derive(Clone, Debug, PartialEq)]
pub struct StyleQuery {
    pub key: String,
    pub char: bool,
    pub last: bool,
    pub error: String,
}

impl StyleQuery {
    pub fn new(code: &str, styles: Option<&super::styles::Styles>) -> StyleQuery {
        let c = parse(code);
        let arg = c.args.first().cloned().unwrap_or_default();
        let key = style_key(&match arg.parse::<u8>() {
            Ok(n @ 1..=9) => format!("heading {n}"),
            _ => arg.clone(),
        });
        let all = || styles.into_iter().flat_map(|s| s.by_id.values());
        let style = all()
            .find(|s| style_key(&s.id) == key)
            .or_else(|| all().find(|s| std::iter::once(&s.name).chain(&s.aliases).any(|n| style_key(n) == key)));
        StyleQuery {
            key: style.map_or(key, |s| style_key(&s.id)),
            char: style.is_some_and(|s| s.kind == super::styles::StyleKind::Character),
            last: c.switches.iter().any(|s| s.flag == 'l'),
            error: match style {
                Some(_) => "Error! No text of specified style in document.".into(),
                None => format!("Error! Use the Home tab to apply {arg} to the text that you want to appear here."),
            },
        }
    }

    /// The first of a paragraph's [`style_hits`] in the style (`rev`: the last).
    pub fn find(&self, hits: &[(String, bool, String)], rev: bool) -> Option<String> {
        let mut it = hits.iter().filter(|h| h.0 == self.key && h.1 == self.char);
        if rev { it.next_back() } else { it.next() }.map(|h| h.2.clone())
    }
}

/// Field-code words, quotes removed from quoted ones.
pub fn words(code: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for c in code.chars() {
        match c {
            '"' => {
                // A quote ends the word before it too: `"a"="b"` is three words.
                if quoted || !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
                quoted = !quoted;
            }
            c if c.is_whitespace() && !quoted => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// `run` with what `src` sets over its paragraph's style formatting `style`: a REF's source formatting on the
/// destination's paragraph style.
// ponytail: compares the common character properties only; the source's `w:rPr` itself when runs keep it.
fn with_direct(run: &Run, src: &Run, style: &Run) -> Run {
    let mut out = (**run).clone();
    macro_rules! keep {
        ($($f:ident),*) => { $(if src.$f != style.$f { out.$f = src.$f.clone(); })* };
    }
    keep!(size, bold, italic, caps, small_caps, strike, dstrike, underline, color, highlight, vert_align, position, spacing, scale);
    if src.ascii != style.ascii || src.h_ansi != style.h_ansi {
        (out.ascii, out.h_ansi, out.east_asia, out.cs) = (src.ascii.clone(), src.h_ansi.clone(), src.east_asia.clone(), src.cs.clone());
    }
    std::rc::Rc::new(out)
}

/// The run a field's error text shows in: its result's, bold toggled.
fn error_run(inlines: &[Inline], sp: FieldSpan) -> Option<Run> {
    let e = sp
        .sep
        .and_then(|k| if let Inline::FieldSep { run, .. } = &inlines[k] { Some(run) } else { None })
        .or_else(|| if let Inline::FieldEnd { run, .. } = &inlines[sp.end] { Some(run) } else { None })?;
    Some(std::rc::Rc::new(super::model::RunStyle { bold: !e.bold, ..(**e).clone() }))
}

pub fn evaluate(doc: &mut Document, now: i64) {
    date_blocks(doc, now);
    let properties = doc.properties.clone();
    let variables = doc.settings.variables.clone();
    let resolver = doc.resolver.clone();
    let styles = resolver.as_deref().map(|r| &r.styles);
    // The bookmarks fields read (REF, NOTEREF, a bookmark's own name, a formula's names), without case: only theirs
    // are found.
    let mut wanted: std::collections::HashSet<String> = std::collections::HashSet::new();
    for s in &mut doc.sections {
        paras(&mut s.blocks, &mut |p| {
            for sp in spans(&p.inlines) {
                match &p.inlines[sp.begin] {
                    Inline::FieldBegin { kind: Some(FieldKind::Ref(n) | FieldKind::Bookmark(n) | FieldKind::NoteRef(n, _)), .. } => {
                        wanted.insert(n.to_ascii_lowercase());
                    }
                    Inline::FieldBegin { kind: Some(FieldKind::Formula), .. } => {
                        let code = code_text(&p.inlines, sp);
                        wanted
                            .extend(code.split(|c: char| !(c.is_alphanumeric() || c == '_')).filter(|w| !w.is_empty()).map(str::to_ascii_lowercase));
                    }
                    _ => {}
                }
            }
        });
    }
    // Every bookmark's name, without case; the texts of those fields read (to the bookmark's end, at most the end of
    // its paragraph; codes left out) and every paragraph's style and text, in order.
    let mut names: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut marks: HashMap<String, String> = HashMap::new();
    // Each bookmark's first shown run, and its paragraph's style formatting.
    let mut sources: HashMap<String, (Run, Run)> = HashMap::new();
    let mut order: Vec<Vec<(String, bool, String)>> = Vec::new();
    // Each bookmark's first note reference.
    let mut notes: HashMap<String, (NoteKind, i64)> = HashMap::new();
    for s in &mut doc.sections {
        paras(&mut s.blocks, &mut |p| {
            bookmark_texts(p, &wanted, &mut names, &mut marks, &mut |name, first, note| {
                if let Some(n) = note {
                    notes.entry(name.to_string()).or_insert(n);
                }
                if let (Some(r), Some(first)) = (resolver.as_deref(), first) {
                    let mut base = r.styles.doc_rpr.clone();
                    base.merge_level(&r.styles.chain_rpr(r.styles.paragraph_style(p.style.style_id.as_deref())));
                    sources.entry(name.to_string()).or_insert((first.clone(), r.run_style(&base)));
                }
            });
            order.push(style_hits(p));
        });
    }
    let (decimal, list) = (doc.settings.decimal_symbol.unwrap_or('.'), doc.settings.list_separator.unwrap_or(','));
    let mut formulas = HashMap::new();
    for s in &doc.sections {
        formula_values(&s.blocks, None, decimal, list, &marks, &mut formulas);
    }
    let file_name = doc.file_name.clone();
    let mut seq: HashMap<String, i64> = HashMap::new();
    let mut lists: HashMap<String, [i64; 9]> = HashMap::new();
    let mut autonum = 0;
    let mut at = 0;
    for s in &mut doc.sections {
        paras(&mut s.blocks, &mut |p| {
            for sp in spans(&p.inlines) {
                let code = code_text(&p.inlines, sp).trim().to_string();
                let error_run = error_run(&p.inlines, sp);
                let cached_empty = result_text(&p.inlines, sp).is_empty();
                let nested = p.inlines[sp.begin + 1..sp.code_end()].iter().any(|i| matches!(i, Inline::FieldBegin { .. }));
                let Inline::FieldBegin { id, kind: Some(kind), value: slot, run, .. } = &mut p.inlines[sp.begin] else { continue };
                let code = &code;
                let w = words(code);
                let has = |sw: &str| w.iter().any(|x| x.eq_ignore_ascii_case(sw));
                let value = match kind {
                    FieldKind::Seq(id) => {
                        let n = seq.entry(id.clone()).or_insert(0);
                        if let Some(r) = w.iter().position(|x| x.eq_ignore_ascii_case("\\r")).and_then(|k| w.get(k + 1)?.parse().ok()) {
                            *n = r;
                        } else if !has("\\c") {
                            *n += 1;
                        }
                        if has("\\h") { Some(String::new()) } else { Some(format_field(&n.to_string(), code, None)) }
                    }
                    // Word's result for a bookmark the document doesn't have.
                    FieldKind::Ref(name) => {
                        let name = name.to_ascii_lowercase();
                        if let Some((src, src_style)) = sources.get(&name) {
                            *run = with_direct(run, src, src_style);
                        }
                        Some(marks.get(&name).cloned().unwrap_or_else(|| "Error! Reference source not found.".into()))
                    }
                    FieldKind::Bookmark(_) if !cached_empty => None,
                    FieldKind::Bookmark(name) => {
                        Some(marks.get(&name.to_ascii_lowercase()).cloned().unwrap_or_else(|| "Error! Bookmark not defined.".into()))
                    }
                    // One with nested fields is evaluated at layout, on their values there.
                    FieldKind::If if nested => None,
                    FieldKind::If => if_field(&w),
                    FieldKind::Invalid => Some(String::new()),
                    FieldKind::Date => Some(date(code, now)),
                    FieldKind::Formula => formulas.get(id).cloned(),
                    FieldKind::FileName => file_name.as_deref().map(|n| file_name_shown(n, code)),
                    // The note's number is the layout's; a bookmark without a note is Word's error.
                    FieldKind::NoteRef(name, note) => {
                        *note = notes.get(&name.to_ascii_lowercase()).copied();
                        note.is_none().then(|| "Error! Reference source not found.".into())
                    }
                    FieldKind::AutoNum => {
                        autonum += 1;
                        let sep = parse(code).switches.iter().find(|s| s.flag.eq_ignore_ascii_case(&'s')).and_then(|s| s.arg.clone());
                        Some(format!("{autonum}{}", sep.as_deref().and_then(|s| s.chars().next()).unwrap_or('.')))
                    }
                    FieldKind::ListNum => Some(list_num(&parse(code), &mut lists)),
                    FieldKind::DocProperty(_) | FieldKind::DocVariable(_) | FieldKind::DocDate(_) => {
                        Some(document_value(kind, code, &properties, &variables))
                    }
                    // In the body: the nearest text in the style before the field, else the next one.
                    FieldKind::StyleRef(_) => {
                        let q = StyleQuery::new(code, styles);
                        let before = order[..(at + 1).min(order.len())].iter().rev().find_map(|h| q.find(h, true));
                        Some(before.or_else(|| order[at.min(order.len())..].iter().find_map(|h| q.find(h, false))).unwrap_or(q.error))
                    }
                    _ => None,
                };
                // A PAGEREF to a bookmark the document doesn't have shows its error text at layout.
                let page_error = matches!(&*kind, FieldKind::PageRef(n) if !names.contains(&n.to_ascii_lowercase()));
                if page_error && let Some(e) = &error_run {
                    *run = e.clone();
                }
                if let Some(v) = value {
                    if v.starts_with("Error! ")
                        && let Some(e) = error_run
                    {
                        *run = e;
                    }
                    *slot = Some(v);
                }
            }
            at += 1;
        });
    }
    // A header's or footer's STYLEREF is found on each page at layout; one that finds nothing shows its error.
    for b in doc.stories.values_mut() {
        paras(b, &mut |p| {
            for sp in spans(&p.inlines) {
                if !matches!(p.inlines[sp.begin], Inline::FieldBegin { kind: Some(FieldKind::StyleRef(_)), .. }) {
                    continue;
                }
                let q = StyleQuery::new(code_text(&p.inlines, sp).trim(), styles);
                let e = error_run(&p.inlines, sp);
                if order.iter().all(|h| q.find(h, false).is_none())
                    && let Inline::FieldBegin { value, run, .. } = &mut p.inlines[sp.begin]
                {
                    *run = e.unwrap_or_else(|| run.clone());
                    *value = Some(q.error);
                }
            }
        });
    }
}

/// How much of a bookmark's text a field shows, in bytes.
const MAX_BOOKMARK_TEXT: usize = 1 << 16;

/// Paragraph `p`'s bookmarks: each name (without case) into `names`, and for each first start of a name in `wanted`,
/// its text into `marks` and to `found` its first shown run and note reference: from its start (one in a field's code
/// starts at that field's result) to its end, at most the end of the paragraph.
fn bookmark_texts(
    p: &Paragraph,
    wanted: &std::collections::HashSet<String>,
    names: &mut std::collections::HashSet<String>,
    marks: &mut HashMap<String, String>,
    found: &mut dyn FnMut(&str, Option<&Run>, Option<(NoteKind, i64)>),
) {
    if !p.inlines.iter().any(|i| matches!(i, Inline::Bookmark(_))) {
        return;
    }
    let mut whole: Option<Whole> = None;
    let mut ends: Option<HashMap<&str, Vec<usize>>> = None;
    // The texts of the fields the bookmarks cut, by where each begins.
    let mut inner: HashMap<usize, Whole> = HashMap::new();
    // Inlines read for them: linear in the paragraph
    let mut budget = 16 * p.inlines.len() + 4096;
    // The fields begun before the bookmark read last, by where they begin: the code ends of those whose codes may hold it.
    let (mut next_field, mut codes) = (0, Vec::new());
    for (k, i) in p.inlines.iter().enumerate() {
        let Inline::Bookmark(name) = i else { continue };
        if name.starts_with('/') {
            continue;
        }
        let key = name.to_ascii_lowercase();
        names.insert(key.clone());
        if !wanted.contains(&key) || marks.contains_key(&key) {
            continue;
        }
        let whole = whole.get_or_insert_with(|| Whole::of(&p.inlines));
        // One in a field's code starts at the result of the innermost field whose code holds it.
        while let Some(f) = whole.fields.get(next_field).filter(|f| f.begin < k) {
            codes.push(f.code_end());
            next_field += 1;
        }
        while codes.last().is_some_and(|&e| e <= k) {
            codes.pop();
        }
        let start = codes.last().map_or(k, |&e| e + 1);
        let ends = ends.get_or_insert_with(|| {
            let mut m: HashMap<&str, Vec<usize>> = HashMap::new();
            for (e, i) in p.inlines.iter().enumerate() {
                if let Inline::Bookmark(n) = i
                    && let Some(n) = n.strip_prefix('/')
                {
                    m.entry(n).or_default().push(e);
                }
            }
            m
        });
        let stop = ends.get(name.as_str()).and_then(|v| v.get(v.partition_point(|&e| e < start)).copied()).unwrap_or(p.inlines.len());
        let start = start.min(stop);
        let after = |v: &[usize]| v.get(v.partition_point(|&x| x < start)).copied().filter(|&x| x < stop);
        let note = after(&whole.notes).and_then(|x| if let Inline::NoteRef { kind, id, .. } = &p.inlines[x] { Some((*kind, *id)) } else { None });
        let mut text = String::new();
        let first = match slice_text(&p.inlines, whole, &mut inner, &mut budget, start..stop, &mut text) {
            Some(first) => first,
            None => {
                // Past the budget, or fields that overlap without nesting: the whole paragraph's text there
                text.clear();
                let all = &whole.text[whole.at[start]..whole.at[stop]];
                text.push_str(&all[..floor_char(all, MAX_BOOKMARK_TEXT)]);
                after(&whole.firsts)
            }
        };
        text.truncate(floor_char(&text, MAX_BOOKMARK_TEXT));
        marks.insert(key.clone(), text);
        found(&key, first.and_then(|x| p.inlines[x].run()), note);
    }
}

/// The text of `inlines[range]` as [`visible`] shows that slice, onto `out` (at most about [`MAX_BOOKMARK_TEXT`]):
/// a field it cuts shows its own inlines, read from that field's inside's [`Whole`] (made once, in `inner`, by where
/// the field begins), the fields it holds whole from `root`. With the first shown run's inline. `None` when making
/// them would pass `budget` (inlines read) or for fields that overlap without nesting.
fn slice_text(
    inlines: &[Inline],
    root: &Whole,
    inner: &mut HashMap<usize, Whole>,
    budget: &mut usize,
    range: std::ops::Range<usize>,
    out: &mut String,
) -> Option<Option<usize>> {
    let (s, t) = (range.start, range.end);
    if s == t {
        return Some(None);
    }
    // A frame: `None` the paragraph, else the inside of the field beginning there (its first inline after its begin).
    let lo = |f: Option<usize>| f.map_or(0, |b| b + 1);
    let made = |f: &FieldSpan, inner: &mut HashMap<usize, Whole>, budget: &mut usize, lo: usize| -> Option<usize> {
        let (b, e) = (lo + f.begin, lo + f.end);
        if let std::collections::hash_map::Entry::Vacant(v) = inner.entry(b) {
            *budget = budget.checked_sub(e - b)?;
            v.insert(Whole::of(&inlines[b + 1..e]));
        }
        Some(b)
    };
    // Into the innermost field holding both ends.
    let mut top: Option<usize> = None;
    loop {
        let w = top.map_or(root, |b| &inner[&b]);
        let l = lo(top);
        let (Some(g), Some(h)) = (w.outer_at(s - l), w.outer_at(t - l)) else { break };
        if g.begin != h.begin {
            break;
        }
        let g = *g;
        *budget = budget.checked_sub(1)?;
        top = Some(made(&g, inner, budget, l)?);
    }
    // The fields cut at each end, outermost first, where they are in the paragraph.
    let chain = |at: usize, inner: &mut HashMap<usize, Whole>, budget: &mut usize| -> Option<Vec<FieldSpan>> {
        let (mut steps, mut f) = (Vec::new(), top);
        loop {
            let w = f.map_or(root, |b| &inner[&b]);
            let l = lo(f);
            if w.clean[at - l] {
                return Some(steps);
            }
            let g = *w.outer_at(at - l)?;
            *budget = budget.checked_sub(1)?;
            let b = made(&g, inner, budget, l)?;
            steps.push(FieldSpan { begin: l + g.begin, sep: None, end: l + g.end });
            f = Some(b);
        }
    };
    let left = chain(s, inner, budget)?;
    let right = chain(t, inner, budget)?;
    let mut first = None;
    let mut piece = |f: Option<usize>, a: usize, b: usize| {
        let w = f.map_or(root, |k| &inner[&k]);
        let l = lo(f);
        if out.len() <= MAX_BOOKMARK_TEXT {
            out.push_str(&w.text[w.at[a - l]..w.at[b - l]]);
        }
        if first.is_none() {
            first = w.firsts.get(w.firsts.partition_point(|&x| x < a - l)).filter(|&&x| x < b - l).map(|&x| l + x);
        }
    };
    // The cut fields at the start, innermost first: from the start to each one's end, then on after it.
    let mut from = s;
    for (k, g) in left.iter().enumerate().rev() {
        let a = left.get(k + 1).map_or(s, |n| n.end + 1);
        piece(Some(g.begin), a, g.end);
        from = g.end + 1;
    }
    let to = right.first().map_or(t, |h| h.begin);
    piece(top, from.min(to), to);
    // Then into those cut at the end, outermost first.
    for (k, h) in right.iter().enumerate() {
        piece(Some(h.begin), h.begin + 1, right.get(k + 1).map_or(t, |n| n.begin));
    }
    Some(first)
}

/// The largest char boundary of `s` at most `n`.
fn floor_char(s: &str, n: usize) -> usize {
    (0..=n.min(s.len())).rev().find(|&k| s.is_char_boundary(k)).unwrap_or(0)
}

/// A paragraph's (or a field's inside's) fields and its text as [`visible`] shows it: where each inline's text starts
/// in it, whether each place between inlines is outside every field (a slice between two such places shows as that
/// part of the text), its outermost fields, its note references and its shown text that isn't hidden.
struct Whole {
    fields: Vec<FieldSpan>,
    text: String,
    at: Vec<usize>,
    clean: Vec<bool>,
    tops: Vec<usize>,
    notes: Vec<usize>,
    firsts: Vec<usize>,
}

impl Whole {
    fn of(inlines: &[Inline]) -> Whole {
        let fields = spans(inlines);
        let mut text = String::new();
        let mut at = Vec::with_capacity(inlines.len() + 1);
        let (mut notes, mut firsts) = (Vec::new(), Vec::new());
        for (k, (i, s)) in inlines.iter().zip(show_at(inlines, false, &|_| true, 0)).enumerate() {
            at.push(text.len());
            match (i, &s) {
                (Inline::NoteRef { .. }, _) => notes.push(k),
                (Inline::Text(_, r), Shown::Itself) if !r.hidden => firsts.push(k),
                _ => {}
            }
            push_shown(&mut text, s, i);
        }
        at.push(text.len());
        let mut across = vec![0i32; inlines.len() + 1];
        for s in &fields {
            across[s.begin + 1] += 1;
            across[s.end + 1] -= 1;
        }
        let mut open = 0;
        let clean = across
            .iter()
            .map(|d| {
                open += d;
                open == 0
            })
            .collect();
        let mut tops = Vec::new();
        let mut reach = None;
        for (k, f) in fields.iter().enumerate() {
            if reach.is_none_or(|e| f.begin > e) {
                tops.push(k);
                reach = Some(f.end);
            }
        }
        Whole { fields, text, at, clean, tops, notes, firsts }
    }

    /// The outermost field holding place `p` (between inlines `p - 1` and `p`).
    fn outer_at(&self, p: usize) -> Option<&FieldSpan> {
        let k = self.tops.partition_point(|&i| self.fields[i].begin < p);
        let f = &self.fields[*self.tops.get(k.checked_sub(1)?)?];
        (p <= f.end).then_some(f)
    }
}

/// Every date block's date at `now`, in its picture: the user locale's names (en-US), whatever the run's language; at
/// most 40 characters.
fn date_blocks(doc: &mut Document, now: i64) {
    let mut visit = |p: &mut Paragraph| {
        for i in &mut p.inlines {
            if let Inline::DateBlock(name, text, _) = i
                && let Some((_, picture)) = DATE_BLOCKS.iter().find(|(e, _)| e == name)
            {
                *text = date(&format!("\\@ {picture}"), now).chars().take(40).collect();
            }
        }
    };
    for s in &mut doc.sections {
        paras(&mut s.blocks, &mut visit);
    }
    for b in doc.stories.values_mut().chain(doc.footnotes.values_mut()).chain(doc.endnotes.values_mut()) {
        paras(b, &mut visit);
    }
}

/// Gives every unlocked field the kind `want` names for its code: the editor's settings (a sub-template reference, an
/// application type) over the code's own. Returns whether any changed, and whether any changed to a kind evaluate
/// fills in (fields need evaluating again).
pub fn rekind(doc: &mut Document, want: &dyn Fn(&FieldCode) -> Option<FieldKind>) -> (bool, bool) {
    let (mut changed, mut back) = (false, false);
    let mut visit = |p: &mut Paragraph| {
        for s in spans(&p.inlines) {
            let code = parse(&code_text(&p.inlines, s));
            let Inline::FieldBegin { kind, value, lock, .. } = &mut p.inlines[s.begin] else { continue };
            let want = want(&code);
            if *lock || *kind == want || kind.as_ref().is_some_and(FieldKind::is_not_from_code) {
                continue;
            }
            back |= want.is_some() && want != Some(FieldKind::SubTemplate);
            *kind = want;
            *value = None;
            changed = true;
        }
    };
    for s in &mut doc.sections {
        paras(&mut s.blocks, &mut visit);
    }
    for b in doc.stories.values_mut().chain(doc.footnotes.values_mut()).chain(doc.endnotes.values_mut()) {
        paras(b, &mut visit);
    }
    for part in doc.glossary.iter_mut().flat_map(|g| g.parts.iter_mut()) {
        paras(&mut part.body, &mut visit);
    }
    (changed, back)
}

/// Whether the body or notes hold fields that need the page count or bookmark pages (a second layout pass).
pub fn page_dependent(doc: &Document) -> bool {
    doc.sections.iter().any(|s| blocks_page_dependent(&s.blocks))
        || doc.footnotes.values().chain(doc.endnotes.values()).any(|b| blocks_page_dependent(b))
}

/// [`page_dependent`] for some blocks.
pub fn blocks_page_dependent(blocks: &[Block]) -> bool {
    blocks.iter().any(|b| match b {
        Block::Para(p) => p
            .inlines
            .iter()
            .any(|i| matches!(i, Inline::FieldBegin { kind: Some(FieldKind::NumPages | FieldKind::SectionPages | FieldKind::PageRef(_)), .. })),
        Block::Table(t) => t.rows.iter().any(|r| r.cells.iter().any(|c| blocks_page_dependent(&c.blocks))),
    })
}

/// `IF a op b "then" "else"` with literal operands: numbers compare numerically, anything else as text.
pub(crate) fn if_field(w: &[String]) -> Option<String> {
    let [_, a, op, b, yes, rest @ ..] = w else { return None };
    let no = rest.first().cloned().unwrap_or_default();
    let ord = match (a.parse::<f64>(), b.parse::<f64>()) {
        (Ok(x), Ok(y)) => x.partial_cmp(&y)?,
        _ => a.cmp(b),
    };
    use std::cmp::Ordering::*;
    let hit = match op.as_str() {
        "=" => ord == Equal,
        "<>" => ord != Equal,
        "<" => ord == Less,
        ">" => ord == Greater,
        "<=" => ord != Greater,
        ">=" => ord != Less,
        _ => return None,
    };
    Some(if hit { yes.clone() } else { no })
}

/// A document property, variable or date field's result: Word's error text when it has none.
pub(crate) fn document_value(kind: &FieldKind, code: &str, p: &crate::docx::properties::Properties, vars: &[(String, String)]) -> String {
    match kind {
        FieldKind::DocProperty(name) => {
            p.lookup(name).map_or_else(|| "Error! Unknown document property name.".into(), |v| format_field(&v, code, None))
        }
        FieldKind::DocVariable(name) => vars
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map_or_else(|| "Error! No document variable supplied.".into(), |(_, v)| format_field(v, code, None)),
        FieldKind::DocDate(name) => {
            // A date Word never set (an unprinted document's print date) shows as zero.
            let secs = p.core(name).and_then(|v| crate::docx::review::Timestamp::parse(v).utc);
            secs.map_or_else(|| "0".into(), |s| date(code, s))
        }
        _ => String::new(),
    }
}

/// Every FILENAME field's value from [`Document::file_name`]; whether one changed.
pub fn show_file_name(doc: &mut Document) -> bool {
    let Some(name) = doc.file_name.clone() else { return false };
    let mut changed = false;
    let mut visit = |p: &mut Paragraph| {
        for sp in spans(&p.inlines) {
            let code = code_text(&p.inlines, sp);
            if let Inline::FieldBegin { kind: Some(FieldKind::FileName), value, lock: false, .. } = &mut p.inlines[sp.begin] {
                let v = Some(file_name_shown(&name, &code));
                changed |= *value != v;
                *value = v;
            }
        }
    };
    for s in &mut doc.sections {
        paras(&mut s.blocks, &mut visit);
    }
    for b in doc.stories.values_mut().chain(doc.footnotes.values_mut()).chain(doc.endnotes.values_mut()) {
        paras(b, &mut visit);
    }
    changed
}

/// FILENAME's text: the name, or with `\p` its folder too.
pub fn file_name_shown(name: &str, code: &str) -> String {
    let path = parse(code).switches.iter().any(|s| s.flag.eq_ignore_ascii_case(&'p'));
    if path { name.to_string() } else { name.rsplit(['/', '\\']).next().unwrap_or(name).to_string() }
}

/// LISTNUM's number: the next at its level (`\l`, 1–9; `\s` sets it) in its list (`NumberDefault`, `OutlineDefault`
/// or `LegalDefault`; another name numbers as `NumberDefault`), deeper levels starting again.
fn list_num(code: &FieldCode, lists: &mut HashMap<String, [i64; 9]>) -> String {
    let switch = |f: char| code.switches.iter().find(|s| s.flag.eq_ignore_ascii_case(&f)).and_then(|s| s.arg.as_deref()?.trim().parse::<i64>().ok());
    let name = code.args.first().map_or_else(|| "NumberDefault".to_string(), |n| n.to_string());
    let level = switch('l').unwrap_or(1).clamp(1, 9) as usize - 1;
    let n = lists.entry(name.to_ascii_lowercase()).or_insert([0; 9]);
    n[level] = switch('s').unwrap_or(n[level].saturating_add(1));
    n[level + 1..].fill(0);
    let fmt = |v: i64, f: &str| crate::docx::numbering::format_number(v, f);
    const NUMBER: [(&str, &str, &str); 9] = [
        ("", "decimal", ")"),
        ("", "lowerLetter", ")"),
        ("", "lowerRoman", ")"),
        ("(", "decimal", ")"),
        ("(", "lowerLetter", ")"),
        ("(", "lowerRoman", ")"),
        ("", "decimal", "."),
        ("", "lowerLetter", "."),
        ("", "lowerRoman", "."),
    ];
    const OUTLINE: [(&str, &str, &str); 9] = [
        ("", "upperRoman", "."),
        ("", "upperLetter", "."),
        ("", "decimal", "."),
        ("", "lowerLetter", ")"),
        ("(", "decimal", ")"),
        ("(", "lowerLetter", ")"),
        ("(", "lowerRoman", ")"),
        ("(", "lowerLetter", ")"),
        ("(", "lowerRoman", ")"),
    ];
    match name.to_ascii_lowercase().as_str() {
        "legaldefault" => n[..=level].iter().map(|v| format!("{v}.")).collect(),
        k => {
            let (open, f, close) = if k == "outlinedefault" { OUTLINE[level] } else { NUMBER[level] };
            format!("{open}{}{close}", fmt(n[level], f))
        }
    }
}

/// The `=` formulas' values in `blocks`, by field; `table`: the cells' texts of the table they are in, and which.
fn formula_values(
    blocks: &[Block],
    table: Option<(&[Vec<String>], (usize, usize))>,
    decimal: char,
    list: char,
    marks: &HashMap<String, String>,
    out: &mut HashMap<FieldId, String>,
) {
    use super::formula;
    for b in blocks {
        match b {
            Block::Para(p) => {
                for sp in spans(&p.inlines) {
                    let Inline::FieldBegin { id, kind: Some(FieldKind::Formula), .. } = &p.inlines[sp.begin] else { continue };
                    // One with nested fields keeps its result.
                    if p.inlines[sp.begin + 1..sp.code_end()].iter().any(|i| matches!(i, Inline::FieldBegin { .. })) {
                        continue;
                    }
                    let code = code_text(&p.inlines, sp);
                    let expr = code.trim_start().trim_start_matches('=');
                    let expr = expr.split('\\').next().unwrap_or_default();
                    let bookmark = |n: &str| marks.get(&n.to_ascii_lowercase()).cloned();
                    let env = formula::Env { decimal, list, table, bookmark: &bookmark };
                    let pic = parse(&code).switches.into_iter().find(|s| s.flag == '#').and_then(|s| s.arg);
                    let v = match formula::evaluate(expr, &env) {
                        Ok(v) => pic.map_or_else(|| formula::general(v, decimal), |pic| formula::picture(v, &pic, decimal)),
                        Err(e) => e,
                    };
                    out.insert(*id, v);
                }
            }
            Block::Table(t) => {
                // Cells by grid column: a merged cell's text in each column it spans.
                let cell_text = |c: &super::model::Cell| {
                    c.blocks.iter().filter_map(|b| if let Block::Para(p) = b { Some(text(p)) } else { None }).collect::<Vec<_>>().join(" ")
                };
                let rows: Vec<Vec<String>> = t
                    .rows
                    .iter()
                    .map(|r| {
                        let mut v = vec![String::new(); r.grid_before];
                        for c in &r.cells {
                            v.extend(std::iter::repeat_n(cell_text(c), c.span.max(1)));
                        }
                        v
                    })
                    .collect();
                for (ri, r) in t.rows.iter().enumerate() {
                    let mut col = r.grid_before;
                    for c in &r.cells {
                        formula_values(&c.blocks, Some((&rows, (ri, col))), decimal, list, marks, out);
                        col += c.span.max(1);
                    }
                }
            }
        }
    }
}

const MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
const DAYS: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];

/// A date shown in date picture `picture` (as [`date_text`] writes it), in seconds since 1970 UTC: `None` for text the
/// picture doesn't read, or a picture without a year, month and day.
// shortcut: English month and day names, as `date` writes them.
pub(crate) fn parse_date_text(picture: &str, text: &str) -> Option<i64> {
    let pic: Vec<char> = picture.chars().collect();
    let txt: Vec<char> = text.trim().chars().collect();
    let (mut k, mut at) = (0, 0);
    let (mut y, mut mo, mut d, mut h, mut mi, mut s, mut pm) = (None, None, None, 0, 0, 0, None);
    let digits = |at: &mut usize, most: usize| -> Option<i64> {
        let n = txt[*at..].iter().take(most).take_while(|c| c.is_ascii_digit()).count();
        let v = txt[*at..*at + n].iter().collect::<String>().parse().ok()?;
        *at += n;
        Some(v)
    };
    let name = |at: &mut usize, names: &[&str], short: bool| -> Option<usize> {
        let found = names.iter().enumerate().find_map(|(k, n)| {
            let n: Vec<char> = if short { n.chars().take(3).collect() } else { n.chars().collect() };
            let hit = txt.len() >= *at + n.len() && txt[*at..*at + n.len()].iter().zip(&n).all(|(a, b)| a.eq_ignore_ascii_case(b));
            hit.then_some((k, n.len()))
        })?;
        *at += found.1;
        Some(found.0)
    };
    while k < pic.len() {
        let c = pic[k];
        if c == '\'' {
            let lit: Vec<char> = pic[k + 1..].iter().take_while(|&&x| x != '\'').copied().collect();
            k += lit.len() + 2;
            for l in lit {
                (txt.get(at)? == &l).then_some(())?;
                at += 1;
            }
            continue;
        }
        if pic[k..].iter().take(5).collect::<String>().eq_ignore_ascii_case("am/pm") {
            let half: String = txt.get(at..at + 2)?.iter().collect();
            pm = Some(match half.to_ascii_uppercase().as_str() {
                "AM" => false,
                "PM" => true,
                _ => return None,
            });
            (k, at) = (k + 5, at + 2);
            continue;
        }
        let n = pic[k..].iter().take_while(|&&x| x == c).count();
        match (c, n) {
            ('y', 3..) => y = Some(digits(&mut at, 4)?),
            ('y', _) => y = Some(digits(&mut at, 2).map(|v| v + if v < 30 { 2000 } else { 1900 })?),
            ('M', 4..) => mo = Some(name(&mut at, &MONTHS, false)? as i64 + 1),
            ('M', 3) => mo = Some(name(&mut at, &MONTHS, true)? as i64 + 1),
            ('M', _) => mo = Some(digits(&mut at, 2)?),
            ('d', 4..) => _ = name(&mut at, &DAYS, false)?,
            ('d', 3) => _ = name(&mut at, &DAYS, true)?,
            ('d', _) => d = Some(digits(&mut at, 2)?),
            ('H' | 'h', _) => h = digits(&mut at, 2)?,
            ('m', _) => mi = digits(&mut at, 2)?,
            ('s', _) => s = digits(&mut at, 2)?,
            (c, _) if c.is_whitespace() => {
                while txt.get(at).is_some_and(|c| c.is_whitespace()) {
                    at += 1;
                }
            }
            _ => {
                for _ in 0..n {
                    txt.get(at).filter(|t| t.eq_ignore_ascii_case(&c))?;
                    at += 1;
                }
            }
        }
        k += n;
    }
    let (y, mo, d) = (y?, mo?, d?);
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let days = [31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    if at != txt.len() || !(1..=12).contains(&mo) || d < 1 || d > days[(mo - 1) as usize] || h > 23 || mi > 59 || s > 59 {
        return None;
    }
    let h = match pm {
        Some(pm) if h <= 12 => h % 12 + if pm { 12 } else { 0 },
        _ => h,
    };
    Some(super::review::days_from_civil(y, mo, d) * 86_400 + h * 3600 + mi * 60 + s)
}

/// `secs` in a date picture (`M/d/yyyy`).
pub(crate) fn date_text(picture: &str, secs: i64) -> String {
    date(&format!("\\@ \"{picture}\""), secs)
}

/// `DATE`/`TIME` with a `\@` picture (default M/d/yyyy) at `secs` since 1970, UTC.
// shortcut: UTC, no time zone; English month/day names.
fn date(code: &str, secs: i64) -> String {
    let (days, sod) = (secs.div_euclid(86400), secs.rem_euclid(86400));
    // Civil date from days since 1970-01-01 (H. Hinnant).
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    let wd = (days + 4).rem_euclid(7) as usize; // 1970-01-01 was a Thursday
    let (hh, mi, ss) = (sod / 3600, sod / 60 % 60, sod % 60);
    let pic = code.split("\\@").nth(1).map(|p| words(p).into_iter().next().unwrap_or_default()).unwrap_or_else(|| "M/d/yyyy".into());
    let mut out = String::new();
    let chars: Vec<char> = pic.chars().collect();
    let mut k = 0;
    while k < chars.len() {
        let c = chars[k];
        // 'text' is literal; AM/PM (any case) is the half of the day, as Word writes it.
        if c == '\'' {
            let lit: String = chars[k + 1..].iter().take_while(|&&x| x != '\'').collect();
            k += lit.chars().count() + 2;
            out.push_str(&lit);
            continue;
        }
        if chars[k..].iter().take(5).collect::<String>().eq_ignore_ascii_case("am/pm") {
            out.push_str(if hh < 12 { "AM" } else { "PM" });
            k += 5;
            continue;
        }
        let n = chars[k..].iter().take_while(|&&x| x == c).count();
        let s = match (c, n) {
            ('y', 4..) => format!("{y:04}"),
            ('y', _) => format!("{:02}", y % 100),
            ('M', 4..) => MONTHS[(m - 1) as usize].to_string(),
            ('M', 3) => MONTHS[(m - 1) as usize][..3].to_string(),
            ('M', 2) => format!("{m:02}"),
            ('M', _) => m.to_string(),
            ('d', 4..) => DAYS[wd].to_string(),
            ('d', 3) => DAYS[wd][..3].to_string(),
            ('d', 2) => format!("{d:02}"),
            ('d', _) => d.to_string(),
            ('H', 2..) => format!("{hh:02}"),
            ('H', _) => hh.to_string(),
            ('h', 2..) => format!("{:02}", (hh + 11) % 12 + 1),
            ('h', _) => ((hh + 11) % 12 + 1).to_string(),
            ('m', 2..) => format!("{mi:02}"),
            ('m', _) => mi.to_string(),
            ('s', 2..) => format!("{ss:02}"),
            ('s', _) => ss.to_string(),
            _ => c.to_string().repeat(n),
        };
        out.push_str(&s);
        k += n;
    }
    out
}

pub(crate) fn format_field(value: &str, code: &str, default: Option<&str>) -> String {
    let Ok(n) = value.parse::<i64>() else { return value.to_string() };
    let mut fmt: Option<&str> = None;
    let mut case: Option<String> = None;
    for sw in code.split("\\*").skip(1) {
        let w = sw.split_whitespace().next().unwrap_or("");
        let f = match w {
            "Arabic" | "arabic" => "decimal",
            "roman" => "lowerRoman",
            "ROMAN" | "Roman" => "upperRoman",
            "alphabetic" => "lowerLetter",
            "ALPHABETIC" | "Alphabetic" => "upperLetter",
            "ArabicDash" => "numberInDash",
            "CardText" | "cardtext" => "cardinalTextLower",
            "OrdText" | "ordtext" => "ordinalTextLower",
            "Ordinal" | "ordinal" => "ordinal",
            "Hex" | "hex" => "hex",
            "Caps" | "FirstCap" | "Upper" | "Lower" | "caps" | "upper" | "lower" => {
                if fmt.is_some() {
                    case = Some(w.to_lowercase());
                }
                continue;
            }
            // MERGEFORMAT, CHARFORMAT: formatting only.
            _ => continue,
        };
        fmt.get_or_insert(f);
    }
    if let Some(pic) = code.split("\\#").nth(1) {
        let pic = pic.trim().trim_matches('"');
        let zeros = pic.bytes().filter(|c| *c == b'0').count();
        let (sign, digits) = (if n < 0 { "-" } else { "" }, n.unsigned_abs().to_string());
        let pad = zeros.saturating_sub(sign.len() + digits.len());
        let mut out = String::with_capacity(sign.len() + pad + digits.len());
        out.push_str(sign);
        out.extend(std::iter::repeat_n('0', pad));
        out.push_str(&digits);
        return out;
    }
    let text = match fmt.or(default) {
        Some("cardinalTextLower") => crate::docx::numbering::cardinal(n),
        Some("ordinalTextLower") => crate::docx::numbering::ordinal_text(n),
        Some(f) => crate::docx::numbering::format_number(n, f),
        None => n.to_string(),
    };
    match case.as_deref() {
        Some("upper") => text.to_uppercase(),
        Some("lower") => text.to_lowercase(),
        Some("caps") => text
            .split(' ')
            .map(|w| {
                let mut c = w.chars();
                c.next().map_or(String::new(), |f| f.to_uppercase().chain(c).collect())
            })
            .collect::<Vec<_>>()
            .join(" "),
        Some("firstcap") => {
            let mut c = text.chars();
            c.next().map_or(String::new(), |f| f.to_uppercase().chain(c).collect())
        }
        _ => text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_argument_quoted_parses_back_to_itself_in_its_place() {
        for name in ["Plain", "Intro Part", "say \"hi\"", "\\lead", "a\\b", "a b\\", "say \\\"hi\\\""] {
            let arg = quote_arg(name).expect("an argument");
            let code = format!(" SUBTEMPLATE {arg} \\c Boxes ");
            let mut spans = Vec::new();
            let parsed = parse_spans(&code, Some(&mut spans));
            assert_eq!(parsed.args, [name], "{code}");
            assert_eq!(&code[spans[0].clone()], arg);
        }
        assert_eq!(quote_arg(""), None);
    }

    #[test]
    fn formulas_lists_autonum_noteref_and_filename_compute() {
        let field = |code: &str, cached: &str| {
            format!(
                r#"<w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve">{code}</w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>{cached}</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r>"#
            )
        };
        let cell = |x: &str| format!("<w:tc><w:p>{x}</w:p></w:tc>");
        let p = |x: &str| format!("<w:p>{x}</w:p>");
        let body = [
            format!(
                "<w:tbl><w:tr>{}{}{}</w:tr><w:tr>{}{}{}</w:tr></w:tbl>",
                cell("<w:r><w:t>2</w:t></w:r>"),
                cell("<w:r><w:t>$1,000.50</w:t></w:r>"),
                cell(&field(r##" =SUM(LEFT) \# "#,##0.00" "##, "?")),
                cell(&field(" =A1*B1 ", "?")),
                cell(&field(" =AVERAGE(ABOVE) ", "?")),
                cell(&field(" =C1/0 ", "?")),
            ),
            p(&field(" =ROUND(2.5*2.25,1)+Total ", "?")),
            p(r#"<w:bookmarkStart w:id="0" w:name="Total"/><w:r><w:t>4</w:t></w:r><w:bookmarkEnd w:id="0"/>"#),
            p(&(field(" LISTNUM ", "") + &field(r#" LISTNUM \l 2 "#, "") + &field(r#" LISTNUM \l 2 "#, "") + &field(" LISTNUM ", ""))),
            p(&(field(r#" LISTNUM LegalDefault \l 1 \s 3 "#, "") + &field(r#" LISTNUM LegalDefault \l 2 "#, ""))),
            p(&(field(" AUTONUM ", "") + &field(r#" AUTONUM \s : "#, ""))),
            p(&(field(" FILENAME ", "old.docx") + &field(" NOTEREF Missing ", "1"))),
        ]
        .concat();
        let xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{body}<w:sectPr/></w:body></w:document>"#
        );
        let bytes = crate::docx::write::repackage(
            &crate::docx::blank::blank_docx(),
            &[("word/document.xml".into(), xml.into_bytes())],
            zip::CompressionMethod::Deflated,
        )
        .expect("repackages");
        let mut doc = crate::docx::read(&bytes).expect("reads");
        let texts = |doc: &Document| -> Vec<String> {
            let mut out = Vec::new();
            for b in &doc.sections[0].blocks {
                match b {
                    Block::Para(p) => out.push(text(p)),
                    Block::Table(t) => out.extend(t.rows.iter().flat_map(|r| r.cells.iter()).filter_map(|c| match &c.blocks[0] {
                        Block::Para(p) => Some(text(p)),
                        _ => None,
                    })),
                }
            }
            out
        };
        assert_eq!(
            texts(&doc),
            [
                "2",
                "$1,000.50",
                "1,002.50",
                "2001",
                "1000.5",
                "!Zero Divide",
                "9.6",
                "4",
                "1)a)b)2)",
                "3.3.1.",
                "1.2:",
                "old.docxError! Reference source not found.",
            ]
            .map(|s| s.to_string())
        );
        doc.file_name = Some("C:\\Cases\\Brief.docx".into());
        assert!(show_file_name(&mut doc));
        assert!(texts(&doc)[11].starts_with("Brief.docx"));
    }

    #[test]
    fn the_local_clock_is_utc_moved_by_quarter_hours() {
        let off = crate::docx::now_local() - crate::docx::now_utc();
        assert!(off.abs() <= 14 * 3600 && (off + 1).rem_euclid(900) <= 2, "{off}");
    }

    #[test]
    fn date_blocks_show_the_date_in_their_picture_whatever_the_language() {
        use crate::docx::{blank::blank_docx, write::repackage};
        let body = r#"<w:p><w:r><w:rPr><w:lang w:val="de-DE"/></w:rPr><w:t>x</w:t><w:dayShort/><w:dayLong/><w:monthShort/><w:monthLong/><w:yearShort/><w:yearLong/><w:t>y</w:t></w:r></w:p>"#;
        let xml = format!(
            r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{body}<w:sectPr/></w:body></w:document>"#
        );
        let bytes =
            repackage(&blank_docx(), &[("word/document.xml".to_string(), xml.into_bytes())], zip::CompressionMethod::Deflated).expect("repackages");
        // 2026-11-08
        let doc = crate::docx::read_at(&bytes, 1_794_096_000).expect("reads");
        let Some(Block::Para(p)) = doc.sections[0].blocks.first() else { panic!("a paragraph") };
        let shown: String = p
            .inlines
            .iter()
            .map(|i| match i {
                Inline::Text(t, _) | Inline::DateBlock(_, t, _) => t.as_str(),
                _ => "",
            })
            .collect();
        assert_eq!(shown, "x08Sunday11November262026y");
        assert_eq!(p.inlines.iter().map(Inline::units).sum::<usize>(), 8, "a block is one unit");
    }

    #[test]
    fn field_2_a_one_word_code_is_a_bookmark_reference() {
        let field = |code: &str| {
            format!(
                r#"<w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText>{code}</w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r>"#
            )
        };
        let body = format!(
            r#"<w:p><w:bookmarkStart w:id="0" w:name="Here"/><w:r><w:t>marked</w:t></w:r><w:bookmarkEnd w:id="0"/></w:p><w:p>{}</w:p><w:p>{}</w:p>"#,
            field("here"),
            field("umber")
        );
        let xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{body}<w:sectPr/></w:body></w:document>"#
        );
        let bytes = crate::docx::write::repackage(
            &crate::docx::blank::blank_docx(),
            &[("word/document.xml".into(), xml.into_bytes())],
            zip::CompressionMethod::Deflated,
        )
        .expect("repackages");
        let doc = crate::docx::read(&bytes).expect("reads");
        let texts: Vec<String> = doc.sections[0].blocks.iter().filter_map(|b| if let Block::Para(p) = b { Some(text(p)) } else { None }).collect();
        assert_eq!(texts[1..3], ["marked", "Error! Bookmark not defined."]);
    }

    #[test]
    fn field_2_a_ref_runs_from_a_bookmark_in_a_code_to_its_end() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:bookmarkStart w:id="0" w:name="G"/><w:r><w:instrText xml:space="preserve"> FORMTEXT </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>Lorem</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r><w:bookmarkEnd w:id="0"/><w:r><w:t xml:space="preserve"> end.</w:t></w:r></w:p><w:p><w:fldSimple w:instr=" REF G "><w:r><w:t>STALE</w:t></w:r></w:fldSimple></w:p><w:sectPr/></w:body></w:document>"#;
        let bytes = crate::docx::write::repackage(
            &crate::docx::blank::blank_docx(),
            &[("word/document.xml".into(), xml.as_bytes().to_vec())],
            zip::CompressionMethod::Deflated,
        )
        .expect("repackages");
        let doc = crate::docx::read(&bytes).expect("reads");
        let Some(Block::Para(p)) = doc.sections[0].blocks.get(1) else { panic!("two paragraphs") };
        assert_eq!(text(p), "Lorem");
    }

    /// A field's markers for tests: begin (of `kind`, showing `value`), separator, end.
    fn markers(run: &Run) -> impl Fn(u64, Option<FieldKind>, Option<&str>) -> [Inline; 3] + '_ {
        move |id, kind, value| {
            let begin = Inline::FieldBegin {
                id: FieldId(id),
                kind,
                lock: false,
                dirty: false,
                view: FieldView::Result,
                label: None,
                value: value.map(str::to_string),
                form: None,
                run: run.clone(),
            };
            [begin, Inline::FieldSep { id: FieldId(id), run: run.clone() }, Inline::FieldEnd { id: FieldId(id), run: run.clone() }]
        }
    }

    /// Each wanted bookmark's text in a paragraph of `inlines`.
    fn marks_of(inlines: Vec<Inline>, wanted: &[&str]) -> HashMap<String, String> {
        let doc = crate::docx::read(&crate::docx::blank::blank_docx()).expect("the blank document reads");
        let Some(Block::Para(blank)) = doc.sections[0].blocks.first() else { panic!("a paragraph") };
        let p = Paragraph { inlines, ..blank.clone() };
        let wanted = wanted.iter().map(|w| w.to_ascii_lowercase()).collect();
        let (mut names, mut marks) = (std::collections::HashSet::new(), HashMap::new());
        bookmark_texts(&p, &wanted, &mut names, &mut marks, &mut |_, _, _| {});
        marks
    }

    #[test]
    fn a_slice_shows_as_visible_shows_it_wherever_it_cuts_fields() {
        let doc = crate::docx::read(&crate::docx::blank::blank_docx()).expect("the blank document reads");
        let Some(Block::Para(p)) = doc.sections[0].blocks.first() else { panic!("a paragraph") };
        let run = p.mark.clone();
        let hidden = std::rc::Rc::new(super::super::model::RunStyle { hidden: true, ..(*run).clone() });
        let f = markers(&run);
        let t = |s: &str| Inline::Text(s.into(), run.clone());
        // "a {IF {PAGE: 7 [3]} = 1 "{MERGEFIELD x}" "n"} b {HYPERLINK [link]} c"
        let ([b1, s1, e1], [b2, s2, e2], [b3, s3, e3], [b4, s4, e4]) =
            (f(1, Some(FieldKind::If), None), f(2, Some(FieldKind::Page), Some("7")), f(3, None, None), f(4, None, None));
        let v = vec![
            t("a "),
            b1,
            t(" IF "),
            b2,
            t(" PAGE "),
            s2,
            t("3"),
            e2,
            t(" = 1 \""),
            b3,
            t(" MERGEFIELD x "),
            s3,
            t("«x»"),
            e3,
            t("\" \"n\" "),
            s1,
            Inline::Text("h".into(), hidden),
            t("y"),
            e1,
            t(" b "),
            b4,
            t(" HYPERLINK "),
            s4,
            t("link"),
            e4,
            t(" c"),
        ];
        let root = Whole::of(&v);
        let (mut inner, mut budget) = (HashMap::new(), usize::MAX);
        for s in 0..=v.len() {
            for e in s..=v.len() {
                let mut out = String::new();
                let first = slice_text(&v, &root, &mut inner, &mut budget, s..e, &mut out).expect("nested fields");
                assert_eq!(out, visible(&v[s..e]), "{s}..{e}");
                let want = v[s..e].iter().zip(shown(&v[s..e])).position(|(i, s)| matches!((i, s), (Inline::Text(_, r), Shown::Itself) if !r.hidden));
                assert_eq!(first, want.map(|k| s + k), "{s}..{e}");
            }
        }
    }

    #[test]
    fn many_bookmarks_in_one_fields_result_each_show_their_part_of_it() {
        let doc = crate::docx::read(&crate::docx::blank::blank_docx()).expect("the blank document reads");
        let Some(Block::Para(p)) = doc.sections[0].blocks.first() else { panic!("a paragraph") };
        let run = p.mark.clone();
        let [b, sep, end] = markers(&run)(1, Some(FieldKind::Page), Some("1"));
        let names: Vec<String> = (0..32).map(|k| format!("m{k}")).collect();
        let mut v = vec![b, Inline::Text(" PAGE ".into(), run.clone()), sep];
        v.extend(names.iter().map(|n| Inline::Bookmark(n.clone())));
        v.extend((0..1000).map(|k| Inline::Text(char::from(b'a' + (k % 26) as u8).to_string(), run.clone())));
        v.push(end);
        v.extend(names.iter().map(|n| Inline::Bookmark(format!("/{n}"))));
        let want: String = (0..1000).map(|k| char::from(b'a' + (k % 26) as u8)).collect();
        let wanted: Vec<&str> = names.iter().map(String::as_str).collect();
        let marks = marks_of(v, &wanted);
        for n in &names {
            assert_eq!(marks.get(n), Some(&want), "{n}");
        }
    }

    #[test]
    fn a_bookmark_in_codes_starts_at_the_innermost_fields_result() {
        let doc = crate::docx::read(&crate::docx::blank::blank_docx()).expect("the blank document reads");
        let Some(Block::Para(p)) = doc.sections[0].blocks.first() else { panic!("a paragraph") };
        let run = p.mark.clone();
        let f = markers(&run);
        let t = |s: &str| Inline::Text(s.into(), run.clone());
        let bm = |n: &str| Inline::Bookmark(n.into());
        let ([b1, s1, e1], [b2, s2, e2]) = (f(1, None, None), f(2, None, None));
        // {IF {PAGE [x} 5 [y} = 1 r} [z] q, every one ending at the end
        let v = vec![
            b1,
            t(" IF "),
            b2,
            bm("x"),
            t(" PAGE "),
            s2,
            t("5"),
            e2,
            bm("y"),
            t(" = 1 "),
            s1,
            t("r"),
            e1,
            bm("z"),
            t(" q"),
            bm("/x"),
            bm("/y"),
            bm("/z"),
        ];
        let marks = marks_of(v, &["x", "y", "z"]);
        assert_eq!((marks["x"].as_str(), marks["y"].as_str(), marks["z"].as_str()), ("5 = 1 r q", "r q", " q"));
    }

    #[test]
    fn field_4_simple_and_ref_results_take_the_paragraph_style() {
        let body = concat!(
            r#"<w:p><w:bookmarkStart w:id="0" w:name="B"/><w:r><w:rPr><w:b/><w:sz w:val="30"/></w:rPr><w:t>Ripe</w:t></w:r><w:bookmarkEnd w:id="0"/></w:p>"#,
            r#"<w:p><w:fldSimple w:instr=" PAGE "><w:r><w:rPr><w:sz w:val="40"/></w:rPr><w:t>9</w:t></w:r></w:fldSimple></w:p>"#,
            r#"<w:p><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:rPr><w:sz w:val="28"/></w:rPr><w:instrText> REF B </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>STALE</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r></w:p>"#,
        );
        let xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{body}<w:sectPr/></w:body></w:document>"#
        );
        let bytes = crate::docx::write::repackage(
            &crate::docx::blank::blank_docx(),
            &[("word/document.xml".into(), xml.into_bytes())],
            zip::CompressionMethod::Deflated,
        )
        .expect("repackages");
        let doc = crate::docx::read(&bytes).expect("reads");
        let runs: Vec<Run> = doc.sections[0]
            .blocks
            .iter()
            .filter_map(|b| {
                if let Block::Para(p) = b {
                    p.inlines.iter().find_map(|i| if let Inline::FieldBegin { run, .. } = i { Some(run.clone()) } else { None })
                } else {
                    None
                }
            })
            .collect();
        // The simple PAGE: not its result run's 20 pt; the REF: its source's bold 15 pt, not its code's 14 pt.
        assert!(runs[0].size != 20.0 && !runs[0].bold, "{}", runs[0].size);
        assert!(runs[1].bold && runs[1].size == 15.0, "{} {}", runs[1].size, runs[1].bold);
    }

    #[test]
    fn field_4_mergeformat_maps_the_cached_formatting_word_by_word() {
        let doc = crate::docx::read(&crate::docx::blank::blank_docx()).expect("reads");
        let Some(Block::Para(p)) = doc.sections[0].blocks.first() else { panic!("a paragraph") };
        let size = |pt: f64| std::rc::Rc::new(super::super::model::RunStyle { size: pt, ..(*p.mark).clone() });
        let (code, a, b) = (size(14.0), size(12.0), size(10.0));
        let id = FieldId::next();
        let inlines = vec![
            Inline::FieldBegin {
                id,
                kind: Some(FieldKind::Ref("B".into())),
                lock: false,
                dirty: false,
                view: FieldView::Result,
                label: None,
                value: None,
                form: None,
                run: code.clone(),
            },
            Inline::Text(r#" REF B \* MERGEFORMAT "#.into(), code.clone()),
            Inline::FieldSep { id, run: code.clone() },
            Inline::Text("Old".into(), a.clone()),
            Inline::Text(" two".into(), b.clone()),
            Inline::FieldEnd { id, run: code.clone() },
        ];
        let s = span_at(&inlines, 0).expect("a field");
        let got: Vec<(String, f64)> = value_runs(&inlines, s, "Ripe plum cake", &code).into_iter().map(|(t, r)| (t, r.size)).collect();
        assert_eq!(got, [("Ripe".to_string(), 12.0), (" plum".to_string(), 10.0), (" cake".to_string(), 14.0)]);
    }

    #[test]
    fn if_and_words() {
        assert_eq!(words(r#"IF 1 = 1 "yes" "no""#), ["IF", "1", "=", "1", "yes", "no"]);
        assert_eq!(if_field(&words(r#"IF 1 = 1 "yes" "no""#)).as_deref(), Some("yes"));
        assert_eq!(if_field(&words(r#"IF 2 < 1 "yes" "no""#)).as_deref(), Some("no"));
        // 2026-10-30T15:04:05Z, a Friday.
        let now = 1_793_372_645;
        assert_eq!(date(r#"DATE \@ "yyyy""#, now), "2026");
        assert_eq!(date(r#"DATE \@ "'x 'yyyy am/pm""#, now), "x 2026 PM");
        assert_eq!(date(r#"DATE \@ "dddd d MMMM HH:mm""#, now), "Friday 30 October 15:04");
    }

    #[test]
    fn parses_field_codes() {
        let c = parse(r#" mergefield Statement.Cost_Amount \# "$,0.00" \* MERGEFORMAT "#);
        assert_eq!(c.name, "MERGEFIELD");
        assert_eq!(c.args, ["Statement.Cost_Amount"]);
        assert_eq!(c.switches, [Switch { flag: '#', arg: Some("$,0.00".into()) }, Switch { flag: '*', arg: Some("MERGEFORMAT".into()) }]);
        assert_eq!(c.kind(), None);
        assert_eq!(c.label(), "«Statement.Cost_Amount»");
        let d = parse(r#"DATE \@"M/d/yyyy" \! "#);
        assert_eq!(d.switches, [Switch { flag: '@', arg: Some("M/d/yyyy".into()) }, Switch { flag: '!', arg: None }]);
        assert_eq!(d.kind(), Some(FieldKind::Date));
        let q = parse(r#"IF "a b" = "say \"hi\"" "x""#);
        assert_eq!(q.args, ["a b", "=", "say \"hi\"", "x"]);
        assert_eq!(q.kind(), Some(FieldKind::If));
        for (code, kind) in [
            (" PAGE ", Some(FieldKind::Page)),
            ("NUMPAGES", Some(FieldKind::NumPages)),
            ("SECTIONPAGES", Some(FieldKind::SectionPages)),
            ("SECTION", Some(FieldKind::Section)),
            ("PAGEREF bm \\h", Some(FieldKind::PageRef("bm".into()))),
            ("REF bm", Some(FieldKind::Ref("bm".into()))),
            ("SEQ Figure", Some(FieldKind::Seq("Figure".into()))),
            ("STYLEREF \"Heading 1\"", Some(FieldKind::StyleRef("Heading 1".into()))),
            ("TIME", Some(FieldKind::Date)),
            (r#""http://example.com/a""#, Some(FieldKind::Invalid)),
            ("=1+2", Some(FieldKind::Formula)),
            ("QUOTE x", None),
            ("umber", Some(FieldKind::Bookmark("UMBER".into()))),
            ("AUTHOR", Some(FieldKind::DocProperty("Author".into()))),
            ("DOCVARIABLE Case", Some(FieldKind::DocVariable("Case".into()))),
            ("NUM PAGES", Some(FieldKind::Bookmark("NUM".into()))),
            ("1ABC", Some(FieldKind::Invalid)),
        ] {
            assert_eq!(parse(code).kind(), kind, "{code}");
        }
        assert_eq!(parse("").name, "");
        assert_eq!(parse("   ").args, Vec::<String>::new());
        assert_eq!(parse("\\").switches, Vec::<Switch>::new());
    }

    #[test]
    fn spans_and_texts() {
        let doc = crate::docx::read(&crate::docx::blank::blank_docx()).expect("the blank document reads");
        let Some(Block::Para(p)) = doc.sections[0].blocks.first() else { panic!("a paragraph") };
        let run = p.mark.clone();
        let b = |id| Inline::FieldBegin {
            id: FieldId(id),
            kind: None,
            lock: false,
            dirty: false,
            view: FieldView::Result,
            label: None,
            value: None,
            form: None,
            run: run.clone(),
        };
        let t = |s: &str| Inline::Text(s.into(), run.clone());
        let sep = |id| Inline::FieldSep { id: FieldId(id), run: run.clone() };
        let end = |id| Inline::FieldEnd { id: FieldId(id), run: run.clone() };
        // "x {IF {MERGEFIELD A} = "1" "y" "n"}y z"
        let v = vec![
            t("x "),
            b(1),
            t(" IF "),
            b(2),
            t(" MERGEFIELD A "),
            sep(2),
            t("«A»"),
            end(2),
            t(r#" = "1" "y" "n" "#),
            sep(1),
            t("y"),
            end(1),
            t(" z"),
        ];
        let s = spans(&v);
        assert_eq!(s, [FieldSpan { begin: 1, sep: Some(9), end: 11 }, FieldSpan { begin: 3, sep: Some(5), end: 7 }]);
        assert_eq!(code_text(&v, s[0]), r#" IF  = "1" "y" "n" "#);
        assert_eq!(code_text(&v, s[1]), " MERGEFIELD A ");
        assert_eq!(result_text(&v, s[0]), "y");
        assert_eq!(visible(&v), "x y z");
        // A symbol shows as its character; one in a field's code doesn't.
        let sym = || Inline::Sym { ch: '\u{F0E0}', font: "Wingdings".into(), run: run.clone(), control: None };
        let link = vec![b(3), t(" HYPERLINK \"https://example.com\" "), sym(), sep(3), sym(), end(3)];
        assert_eq!(result_text(&link, spans(&link)[0]), "\u{F0E0}");
        assert_eq!(visible(&link), "\u{F0E0}");
    }

    #[test]
    fn deeply_nested_unvalued_fields_show_within_bounded_depth() {
        let doc = crate::docx::read(&crate::docx::blank::blank_docx()).expect("the blank document reads");
        let Some(Block::Para(p)) = doc.sections[0].blocks.first() else { panic!("a paragraph") };
        let run = p.mark.clone();
        let n = 4_000u64;
        let mut v = Vec::with_capacity(n as usize * 4 + 1);
        for id in 0..n {
            v.push(Inline::FieldBegin {
                id: FieldId(id),
                kind: Some(FieldKind::Page),
                lock: false,
                dirty: false,
                view: FieldView::Result,
                label: None,
                value: None,
                form: None,
                run: run.clone(),
            });
            v.push(Inline::FieldSep { id: FieldId(id), run: run.clone() });
        }
        v.push(Inline::Text("deep".into(), run.clone()));
        v.extend((0..n).rev().map(|id| Inline::FieldEnd { id: FieldId(id), run: run.clone() }));
        // Each field's result is the next one: past the depth bound the innermost text no longer shows.
        assert_eq!(visible(&v), "");
        let shallow: Vec<Inline> = v[2 * (n as usize) - 4..2 * (n as usize) + 3].to_vec();
        assert_eq!(visible(&shallow), "deep");
    }

    #[test]
    fn visible_in_leaves_out_an_evaluated_fields_excluded_result() {
        use crate::docx::model::RunStyle;
        use crate::docx::review::{AuthorId, MarkKind, MarkupView, RevisionId, RevisionMark, RunRevisions};
        use std::rc::Rc;
        let doc = crate::docx::read(&crate::docx::blank::blank_docx()).expect("the blank document reads");
        let Some(Block::Para(p)) = doc.sections[0].blocks.first() else { panic!("a paragraph") };
        let run = p.mark.clone();
        let hidden = Rc::new(RunStyle { hidden: true, ..(*run).clone() });
        let mark = Rc::new(RevisionMark { id: RevisionId(1), author: AuthorId(0), date: None, kind: MarkKind::Delete, move_name: None });
        let deleted = Rc::new(RunStyle { rev: RunRevisions { del: Some(mark), ..Default::default() }, ..(*run).clone() });
        let b = |id| Inline::FieldBegin {
            id: FieldId(id),
            kind: Some(FieldKind::Page),
            lock: false,
            dirty: false,
            view: FieldView::Result,
            label: None,
            value: None,
            form: None,
            run: run.clone(),
        };
        let t = |s: &str, r: &Run| Inline::Text(s.into(), r.clone());
        let sep = |id| Inline::FieldSep { id: FieldId(id), run: run.clone() };
        let end = |id| Inline::FieldEnd { id: FieldId(id), run: run.clone() };
        // "a {PAGE}1<del>secret</del><hidden>quiet</hidden>{PAGE}2<del>nested</del>{/PAGE}{/PAGE} z"
        let v = vec![
            t("a ", &run),
            b(1),
            t(" PAGE ", &run),
            sep(1),
            t("1", &run),
            t("secret", &deleted),
            t("quiet", &hidden),
            b(2),
            t(" PAGE ", &run),
            sep(2),
            t("2", &run),
            t("nested", &deleted),
            end(2),
            end(1),
            t(" z", &run),
        ];
        assert_eq!(visible(&v), "a 1secretquiet2nested z");
        let in_view = |view| crate::docx::review::ReviewView { view, ..Default::default() };
        assert_eq!(visible_in(&v, &in_view(MarkupView::NoMarkup)), "a 12 z");
        assert_eq!(visible_in(&v, &in_view(MarkupView::All)), "a 1secret2nested z");
    }
}
