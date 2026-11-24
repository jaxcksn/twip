//! Tables of contents and tables of figures

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::api::review::Body;
use crate::docx::fields::{self, Shown, Switch};
use crate::docx::props::Leader;
use crate::docx::{Block, FieldEdge, FieldId, FieldKind, FieldView, Inline, Paragraph, Run};
use crate::font::Fonts;
use crate::layout::{ParaId, Unit};

use super::Editor;
use super::commands::Outcome;
use super::index::Container;
use super::position::{Position, StoryId};
use super::selection::Selection;
use super::step::{Step, para_len};

/// A table's switches: what it collects and how its entries read. A table of figures is one that collects captions.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TocOptions {
    /// Paragraphs in the built-in heading styles of these levels (`\o`).
    pub headings: Option<(u8, u8)>,
    /// Paragraphs with an outline level, as their level (`\u`); within `headings`' levels when those are set.
    pub outline_levels: bool,
    /// Paragraphs in these styles (by name), at these levels (`\t`).
    pub styles: Vec<(String, u8)>,
    /// Table entry fields of this identifier (`\f`; `C` when none is given).
    pub entry_fields: Option<String>,
    /// The table entry fields' levels taken (`\l`).
    pub entry_levels: Option<(u8, u8)>,
    /// The captions of this label (the paragraphs holding its sequence field), as one level (`\c`); without their
    /// label and number with `include_label` off (`\a`).
    pub captions: Option<Caption>,
    /// Only what this bookmark marks (`\b`).
    pub bookmark: Option<String>,
    /// Entries link to their source (`\h`).
    pub hyperlinks: bool,
    /// Entries of these levels show no page number (`\n`; all levels without a range).
    pub omit_pages: Option<(u8, u8)>,
    /// What goes between an entry and its page number in place of a tab (`\p`).
    pub separator: Option<String>,
    /// Page numbers lead with this sequence's number and `\d`'s separator (`\s`, `\d`).
    pub sequence: Option<(String, Option<String>)>,
    /// Tab leaders and page numbers hidden in a web view (`\z`).
    pub hide_in_web: bool,
    /// Tabs in entries kept (`\w`), else spaces.
    pub keep_tabs: bool,
    /// Line breaks in entries kept (`\x`), else spaces.
    pub keep_breaks: bool,
    /// Switches it doesn't model (`\*`), kept in its code.
    pub other: Vec<Switch>,
}

/// A table of figures' captions: their label (sequence identifier), and whether entries keep it and its number.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Caption {
    /// The sequence identifier the captions number with.
    pub label: String,
    /// Entries keep the label and number (`\c`), else drop them (`\a`).
    pub include_label: bool,
}

/// How a table's entries are laid out: what its code doesn't hold, read back from its entries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TocLook {
    /// Page numbers at a right tab stop at the right margin.
    pub right_align: bool,
    /// That tab stop's leader.
    pub leader: Leader,
}

impl Default for TocLook {
    fn default() -> Self {
        TocLook { right_align: true, leader: Leader::Dot }
    }
}

/// What a new table goes in beside its field: a title paragraph in the TOC Heading style, and a building block content
/// control around both (Word's built-in tables).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TocFrame {
    /// The title paragraph's text, in TOC Heading.
    pub title: Option<String>,
    /// A building block content control around the title and the table.
    pub control: bool,
}

/// What updating a table rewrites.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TocUpdate {
    /// The page numbers only: the entries' edits stay.
    Pages,
    /// Every entry, collected again.
    Entire,
}

/// One entry a table collects: where it comes from, its level and its text.
#[derive(Clone, Debug, PartialEq)]
pub struct TocEntry {
    /// The paragraph it comes from.
    pub para: ParaId,
    /// Its level (1…9): the TOC style it takes.
    pub level: u8,
    /// Its text as the entry shows it.
    pub text: String,
    /// It shows a page number.
    pub page: bool,
    /// From a table entry field, not the paragraph itself.
    pub from_field: bool,
    /// The sequence number its page number leads with (`\s`).
    pub prefix: Option<String>,
    /// The entry's inlines as its source formats them (direct formatting kept), and their runs.
    runs: Vec<(EntryPiece, Option<Run>)>,
}

#[derive(Clone, Debug, PartialEq)]
enum EntryPiece {
    Text(String),
    Tab,
    Break,
}

/// A table of contents field in the document: where it is and its code.
#[derive(Clone, Debug, PartialEq)]
pub struct TocField {
    /// The story or cell its blocks are in.
    pub container: Container,
    /// Its first block: the paragraph its field starts in.
    pub first: usize,
    /// Its last block: the paragraph its field ends in.
    pub last: usize,
    /// Its field's code.
    pub code: String,
    /// Its field is locked: updates leave it.
    pub lock: bool,
    shape: Shape,
}

impl TocField {
    /// Whether its result ends at its last paragraph's start (nothing shown before the end): that paragraph is the
    /// one after the table, not an entry.
    fn ends_at_start(&self, blocks: &[Block]) -> bool {
        let Shape::Spanning { end, .. } = self.shape else { return false };
        self.last > self.first
            && matches!(blocks.get(self.last), Some(Block::Para(p)) if p.inlines[..end.min(p.inlines.len())].iter().all(|i| i.units() == 0))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Shape {
    /// Edges at (block, inline) of its first and last blocks.
    Spanning { start: usize, end: usize },
    /// A field in one paragraph: its begin and end inlines.
    Inline { begin: usize, end: usize },
}

impl TocField {
    /// The options its code sets.
    pub fn options(&self, list_separator: char) -> TocOptions {
        TocOptions::of_code(&self.code, list_separator)
    }
}

/// A level range of a switch (`"1-3"`, `"2"`), within 1 to 9.
fn levels(arg: Option<&str>) -> Option<(u8, u8)> {
    let arg = arg?.trim();
    let num = |s: &str| s.trim().parse::<u8>().ok().filter(|n| (1..=9).contains(n));
    match arg.split_once('-') {
        Some((a, b)) => {
            let (a, b) = (num(a)?, num(b)?);
            Some((a.min(b), a.max(b)))
        }
        None => num(arg).map(|n| (n, n)),
    }
}

fn level_arg((a, b): (u8, u8)) -> String {
    format!("\"{a}-{b}\"")
}

impl TocOptions {
    /// The options a `TOC` code sets; `list_separator` separates `\t`'s names and levels.
    pub fn of_code(code: &str, list_separator: char) -> TocOptions {
        let c = fields::parse(code);
        let mut o = TocOptions::default();
        for s in &c.switches {
            let arg = s.arg.as_deref();
            match s.flag.to_ascii_lowercase() {
                'o' => o.headings = Some(levels(arg).unwrap_or((1, 9))),
                'u' => o.outline_levels = true,
                't' => {
                    let parts: Vec<&str> = arg.unwrap_or("").split(list_separator).map(str::trim).collect();
                    let mut k = 0;
                    while k < parts.len() {
                        let name = parts[k];
                        let level = parts.get(k + 1).and_then(|l| l.parse::<u8>().ok());
                        if !name.is_empty() {
                            o.styles.push((name.to_string(), level.unwrap_or(1).clamp(1, 9)));
                        }
                        k += if level.is_some() { 2 } else { 1 };
                    }
                }
                'f' => o.entry_fields = Some(arg.unwrap_or("C").to_string()),
                'l' => o.entry_levels = levels(arg),
                'c' => o.captions = Some(Caption { label: arg.unwrap_or("").to_string(), include_label: true }),
                'a' => o.captions = Some(Caption { label: arg.unwrap_or("").to_string(), include_label: false }),
                'b' => o.bookmark = arg.map(str::to_string),
                'h' => o.hyperlinks = true,
                'n' => o.omit_pages = Some(levels(arg).unwrap_or((1, 9))),
                'p' => o.separator = arg.map(str::to_string),
                's' => o.sequence = Some((arg.unwrap_or("").to_string(), o.sequence.take().and_then(|s| s.1))),
                'd' => {
                    let d = arg.map(str::to_string);
                    o.sequence = Some(o.sequence.take().map_or((String::new(), d.clone()), |s| (s.0, d)));
                }
                'z' => o.hide_in_web = true,
                'w' => o.keep_tabs = true,
                'x' => o.keep_breaks = true,
                _ => o.other.push(s.clone()),
            }
        }
        if o.sequence.as_ref().is_some_and(|s| s.0.is_empty()) {
            o.sequence = None;
        }
        o
    }

    /// The options as a `TOC` code, in Word's order.
    pub fn code(&self, list_separator: char) -> String {
        let q = fields::quote;
        let mut c = String::from(" TOC");
        let mut sw = |s: String| {
            c.push(' ');
            c.push_str(&s);
        };
        if let Some(l) = self.headings {
            sw(format!("\\o {}", level_arg(l)));
        }
        if let Some(id) = &self.entry_fields {
            sw(if id.eq_ignore_ascii_case("C") { "\\f".into() } else { format!("\\f {}", q(id)) });
        }
        if let Some(l) = self.entry_levels {
            sw(format!("\\l {}", level_arg(l)));
        }
        if let Some(l) = self.omit_pages {
            sw(if l == (1, 9) { "\\n".into() } else { format!("\\n {}", level_arg(l)) });
        }
        if let Some(p) = &self.separator {
            sw(format!("\\p {}", q(p)));
        }
        if let Some((id, d)) = &self.sequence {
            sw(format!("\\s {}", q(id)));
            if let Some(d) = d {
                sw(format!("\\d {}", q(d)));
            }
        }
        if let Some(b) = &self.bookmark {
            sw(format!("\\b {}", q(b)));
        }
        if self.hyperlinks {
            sw("\\h".into());
        }
        if self.hide_in_web {
            sw("\\z".into());
        }
        if self.keep_tabs {
            sw("\\w".into());
        }
        if self.keep_breaks {
            sw("\\x".into());
        }
        if !self.styles.is_empty() {
            let sep = list_separator.to_string();
            let list: Vec<String> = self.styles.iter().map(|(n, l)| format!("{n}{sep}{l}")).collect();
            sw(format!("\\t {}", q(&list.join(&sep))));
        }
        if self.outline_levels {
            sw("\\u".into());
        }
        if let Some(cap) = &self.captions {
            sw(format!("\\{} {}", if cap.include_label { 'c' } else { 'a' }, q(&cap.label)));
        }
        for s in &self.other {
            sw(match &s.arg {
                Some(a) => format!("\\{} {}", s.flag, fields::quote_arg(a).unwrap_or_default()),
                None => format!("\\{}", s.flag),
            });
        }
        c.push(' ');
        c
    }

    /// Word's default table of contents: `TOC \o "1-3" \h \z \u`.
    pub fn word_default() -> TocOptions {
        TocOptions { headings: Some((1, 3)), outline_levels: true, hyperlinks: true, hide_in_web: true, ..TocOptions::default() }
    }

    /// Word's default table of figures for caption label `label`: `TOC \h \z \c "label"` (its sequence).
    pub fn figures(label: &str) -> TocOptions {
        TocOptions {
            hyperlinks: true,
            hide_in_web: true,
            captions: Some(Caption { label: super::captions::sequence_of(label), include_label: true }),
            ..TocOptions::default()
        }
    }

    fn page_shown(&self, level: u8) -> bool {
        self.omit_pages.is_none_or(|(a, b)| level < a || level > b)
    }
}

/// A table entry field's settings: its text, level, identifier and whether its entry shows a page number.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TocMark {
    /// The entry's text.
    pub text: String,
    /// The entry's level (1…9).
    pub level: u8,
    /// `C` (Word's default) when `None`.
    pub identifier: Option<String>,
    /// The entry shows a page number (no `\n`).
    pub page_number: bool,
}

impl TocMark {
    /// Its `TC` code.
    pub fn code(&self) -> String {
        let mut c = format!(" TC {}", fields::quote(&self.text));
        if let Some(id) = self.identifier.as_deref().filter(|id| !id.eq_ignore_ascii_case("C")) {
            c.push_str(&format!(" \\f {}", fields::quote_arg(id).unwrap_or_default()));
        }
        c.push_str(&format!(" \\l \"{}\"", self.level.clamp(1, 9)));
        if !self.page_number {
            c.push_str(" \\n");
        }
        c.push(' ');
        c
    }

    /// A `TC` code's settings.
    pub fn of_code(code: &str) -> Option<TocMark> {
        let c = fields::parse(code);
        if c.name != "TC" {
            return None;
        }
        let sw = |f: char| c.switches.iter().find(|s| s.flag.eq_ignore_ascii_case(&f));
        Some(TocMark {
            text: c.args.first().cloned().unwrap_or_default(),
            level: sw('l').and_then(|s| s.arg.as_deref()?.trim().parse().ok()).filter(|l| (1..=9).contains(l)).unwrap_or(1),
            identifier: sw('f').map(|s| s.arg.clone().unwrap_or_else(|| "C".into())),
            page_number: sw('n').is_none(),
        })
    }
}

/// Tables an update of every table rebuilds at most: each rebuild collects its entries from the whole body, so the
/// work is this many body walks. Word's documents hold a few (contents, figures, tables); a document with more is
/// crafted, and the rest keep their entries (their page numbers are still updated).
pub const MAX_REBUILT: usize = 64;

/// How many entries a table takes at most: a hostile document's headings past it are left out.
#[cfg(not(test))]
const MAX_ENTRIES: usize = 100_000;
/// How many entries a table takes at most (a small bound, so tests reach it).
#[cfg(test)]
pub(super) const MAX_ENTRIES: usize = 300;

/// The text Word shows for a table that finds nothing.
fn none_found(figures: bool) -> &'static str {
    if figures { "No table of figures entries found." } else { "No table of contents entries found." }
}

/// The start and end edges of a container's fields across paragraphs, paired as they nest: (block, inline) of each.
pub(crate) fn edge_pairs(blocks: &[Block]) -> Vec<((usize, usize), (usize, usize))> {
    let mut open: Vec<(usize, usize)> = Vec::new();
    let mut out = Vec::new();
    for (b, block) in blocks.iter().enumerate() {
        let Block::Para(p) = block else { continue };
        for (k, i) in p.inlines.iter().enumerate() {
            match edge(i) {
                Some(FieldEdge::Start { .. }) => open.push((b, k)),
                Some(FieldEdge::End) => {
                    if let Some(s) = open.pop() {
                        out.push((s, (b, k)));
                    }
                }
                _ => {}
            }
        }
    }
    out.sort();
    out
}

/// The edge pairing the one at `(block, inline)` of `blocks`: scanning outward from it, counting nested fields.
pub(super) fn partner(blocks: &[Block], (b, k): (usize, usize)) -> Option<(usize, usize)> {
    let para = |blk: usize| if let Block::Para(p) = &blocks[blk] { Some(&p.inlines) } else { None };
    let forward = matches!(edge(&para(b)?[k])?, FieldEdge::Start { .. });
    let mut depth = 0usize;
    // One step of the scan: whether `i` at `at` closes the search.
    let mut visit = |i: &Inline| -> bool {
        match (edge(i), forward) {
            (Some(FieldEdge::Start { .. }), true) | (Some(FieldEdge::End), false) => depth += 1,
            (Some(_), _) if depth == 0 => return true,
            (Some(_), _) => depth -= 1,
            (None, _) => {}
        }
        false
    };
    if forward {
        for blk in b..blocks.len() {
            let Some(inl) = para(blk) else { continue };
            let from = if blk == b { k + 1 } else { 0 };
            for (j, i) in inl.iter().enumerate().skip(from) {
                if visit(i) {
                    return Some((blk, j));
                }
            }
        }
    } else {
        for blk in (0..=b).rev() {
            let Some(inl) = para(blk) else { continue };
            let to = if blk == b { k } else { inl.len() };
            for (j, i) in inl[..to].iter().enumerate().rev() {
                if visit(i) {
                    return Some((blk, j));
                }
            }
        }
    }
    None
}

/// The field edge an inline is: a start or an end (not a code view's markers).
pub(crate) fn edge(i: &Inline) -> Option<&FieldEdge> {
    match i {
        Inline::Opaque(o) => o.field.as_ref().filter(|f| matches!(f, FieldEdge::Start { .. } | FieldEdge::End)),
        _ => None,
    }
}

/// Whether a field's code is a table of contents'.
fn is_toc(code: &str) -> bool {
    code.split_whitespace().next().is_some_and(|w| w.eq_ignore_ascii_case("TOC"))
}

/// The paragraph's text Word's tables show for it: field results, its list number and a tab first, tabs and breaks
/// as `o` keeps them, its direct formatting kept.
fn entry_pieces(p: &Paragraph, o: &TocOptions, review: &crate::docx::review::ReviewView) -> Vec<(EntryPiece, Option<Run>)> {
    let mut out: Vec<(EntryPiece, Option<Run>)> = Vec::new();
    if let Some(l) = p.list.as_ref().filter(|l| !l.text.is_empty() && l.picture.is_none()) {
        out.push((EntryPiece::Text(l.text.clone()), None));
        out.push((EntryPiece::Tab, None));
    }
    let shown = fields::shown(&p.inlines);
    for (i, s) in p.inlines.iter().zip(shown) {
        let run = i.run().filter(|r| !r.hidden && !review.hides(&r.rev));
        if i.run().is_some() && run.is_none() {
            continue;
        }
        let run = run.cloned();
        match (s, i) {
            (Shown::Value(v), _) if !v.is_empty() => out.push((EntryPiece::Text(v), run)),
            (Shown::Itself, Inline::Text(t, _)) if !t.is_empty() => out.push((EntryPiece::Text(t.clone()), run)),
            (Shown::Itself, Inline::Sym { ch, .. }) => out.push((EntryPiece::Text(ch.to_string()), run)),
            (Shown::Itself, Inline::Tab(_) | Inline::PTab(..)) => {
                out.push(if o.keep_tabs { (EntryPiece::Tab, run) } else { (EntryPiece::Text(" ".into()), run) })
            }
            (Shown::Itself, Inline::Break(crate::docx::BreakKind::Line, _)) => {
                out.push(if o.keep_breaks { (EntryPiece::Break, run) } else { (EntryPiece::Text(" ".into()), run) })
            }
            _ => {}
        }
    }
    // Trailing white space goes.
    while let Some((EntryPiece::Text(t), _)) = out.last_mut() {
        let n = t.trim_end().len();
        if n > 0 {
            t.truncate(n);
            break;
        }
        out.pop();
    }
    out
}

fn pieces_text(pieces: &[(EntryPiece, Option<Run>)]) -> String {
    pieces
        .iter()
        .map(|(p, _)| match p {
            EntryPiece::Text(t) => t.as_str(),
            EntryPiece::Tab => "\t",
            EntryPiece::Break => "\n",
        })
        .collect()
}

/// The caption text after the sequence field of `label` (and the separator after it): a table of figures' entry
/// without its label and number (`\a`).
fn after_label(pieces: Vec<(EntryPiece, Option<Run>)>, p: &Paragraph, label: &str) -> Vec<(EntryPiece, Option<Run>)> {
    // Units of text before the sequence field's end, as entry_pieces counts them.
    let shown = fields::shown(&p.inlines);
    let mut before = 0usize;
    let mut found = false;
    let spans = fields::spans(&p.inlines);
    let end = spans
        .iter()
        .find(|s| matches!(&p.inlines[s.begin], Inline::FieldBegin { kind: Some(FieldKind::Seq(id)), .. } if id.eq_ignore_ascii_case(label)));
    if let Some(sp) = end {
        for (i, s) in p.inlines.iter().zip(&shown).take(sp.end + 1) {
            match (s, i) {
                (Shown::Value(v), _) => before += v.chars().count(),
                (Shown::Itself, Inline::Text(t, _)) => before += t.chars().count(),
                (Shown::Itself, Inline::Tab(_) | Inline::PTab(..) | Inline::Sym { .. } | Inline::Break(..)) => before += 1,
                _ => {}
            }
        }
        found = true;
    }
    if !found {
        return pieces;
    }
    if p.list.as_ref().is_some_and(|l| !l.text.is_empty() && l.picture.is_none()) {
        before += p.list.as_ref().map_or(0, |l| l.text.chars().count()) + 1;
    }
    let mut out = Vec::new();
    let mut skipping = true;
    for (piece, run) in pieces {
        let n = match &piece {
            EntryPiece::Text(t) => t.chars().count(),
            _ => 1,
        };
        if before >= n {
            before -= n;
            continue;
        }
        let piece = match piece {
            EntryPiece::Text(t) if before > 0 => EntryPiece::Text(t.chars().skip(before).collect()),
            other => other,
        };
        before = 0;
        // The separator after the number: Word's caption separators and white space.
        let piece = match piece {
            EntryPiece::Text(t) if skipping => {
                let t = t.trim_start_matches(|c: char| c.is_whitespace() || matches!(c, ':' | '.' | '-' | '\u{2013}' | '\u{2014}'));
                if t.is_empty() {
                    continue;
                }
                EntryPiece::Text(t.to_string())
            }
            EntryPiece::Tab if skipping => continue,
            other => other,
        };
        skipping = false;
        out.push((piece, run));
    }
    out
}

/// A `_Toc` bookmark name Word would give: `_Toc` and nine digits, past `taken`'s.
/// The first hidden bookmark name `prefix` + digits from `next` on that isn't `taken` (lowercase); past `u64::MAX` the
/// search starts again at 0, so it always ends (`taken` is finite).
pub(super) fn free_name(prefix: &str, taken: &HashSet<String>, next: &mut u64) -> String {
    loop {
        let name = format!("{prefix}{:09}", *next);
        *next = next.checked_add(1).unwrap_or(0);
        if !taken.contains(&name.to_ascii_lowercase()) {
            return name;
        }
    }
}

/// Where hidden bookmark names `prefix` + digits start: past the document's highest.
pub(super) fn first_free<'a>(prefix: &str, names: impl Iterator<Item = &'a str>) -> u64 {
    names.filter_map(|n| n.strip_prefix(prefix).and_then(|d| d.parse::<u64>().ok())).max().map_or(100_000_000, |n| n.checked_add(1).unwrap_or(0))
}

fn leader_xml(l: Leader) -> &'static str {
    match l {
        Leader::None => "none",
        Leader::Dot => "dot",
        Leader::Hyphen => "hyphen",
        Leader::Underscore => "underscore",
        Leader::Heavy => "heavy",
        Leader::MiddleDot => "middleDot",
    }
}

impl Editor {
    /// Every table of contents (and table of figures) field in the body, in document order: those across
    /// paragraphs, and those in one paragraph.
    pub fn tables_of_contents(&self) -> Vec<TocField> {
        let mut out = Vec::new();
        for (k, s) in self.doc.sections.iter().enumerate() {
            self.tocs_in(Container::Section(k), &s.blocks, &mut out);
        }
        out
    }

    fn tocs_in(&self, container: Container, blocks: &[Block], out: &mut Vec<TocField>) {
        let pairs = edge_pairs(blocks);
        let mut covered = 0usize;
        // Pairs are sorted by start: a cursor finds a block's first.
        let mut at = 0usize;
        for (b, block) in blocks.iter().enumerate() {
            while pairs.get(at).is_some_and(|((sb, _), _)| *sb < b) {
                at += 1;
            }
            if let Some(&((sb, si), (eb, ei))) = pairs.get(at).filter(|((sb, _), _)| *sb == b)
                && b >= covered
            {
                let Block::Para(p) = &blocks[sb] else { continue };
                if let Some(FieldEdge::Start { lock, .. }) = edge(&p.inlines[si])
                    && let code = fields::span_code(&p.inlines, si)
                    && is_toc(&code)
                {
                    out.push(TocField {
                        container,
                        first: sb,
                        last: eb,
                        code: code.into_owned(),
                        lock: *lock,
                        shape: Shape::Spanning { start: si, end: ei },
                    });
                    covered = eb + 1;
                    continue;
                }
            }
            match block {
                Block::Para(p) => {
                    for s in fields::spans(&p.inlines) {
                        let code = fields::code_text(&p.inlines, s);
                        if is_toc(&code) {
                            let lock = matches!(p.inlines[s.begin], Inline::FieldBegin { lock: true, .. });
                            out.push(TocField { container, first: b, last: b, code, lock, shape: Shape::Inline { begin: s.begin, end: s.end } });
                        }
                    }
                }
                Block::Table(t) => {
                    for (ri, r) in t.rows.iter().enumerate() {
                        for (ci, c) in r.cells.iter().enumerate() {
                            self.tocs_in(Container::Cell { table: crate::layout::TableId(t.id), row: ri, cell: ci }, &c.blocks, out);
                        }
                    }
                }
            }
        }
    }

    /// The paragraphs of tables of contents' results, which no table collects: not a last one whose result ends at
    /// its start (the paragraph after the table).
    fn toc_paras(&self) -> HashSet<ParaId> {
        let mut out = HashSet::new();
        for t in self.tables_of_contents() {
            let Some(blocks) = self.index.blocks(&self.doc, t.container) else { continue };
            let after = t.ends_at_start(blocks);
            for (k, b) in blocks[t.first..=t.last.min(blocks.len().saturating_sub(1))].iter().enumerate() {
                let Block::Para(p) = b else { continue };
                if after && t.first + k == t.last {
                    continue;
                }
                out.insert(ParaId(p.id));
            }
        }
        out
    }

    /// The entries a table with options `o` collects from the body, in document order.
    pub fn toc_entries(&self, o: &TocOptions) -> Vec<TocEntry> {
        let skip = self.toc_paras();
        let styles = self.doc.resolver.as_ref().map(|r| &r.styles);
        let name_of = |p: &Paragraph| -> Option<String> { styles?.by_id.get(p.style.style_id.as_deref()?).map(|s| s.name.clone()) };
        let review = crate::docx::review::ReviewView { view: self.doc.review.view, ..Default::default() };
        let mut out = Vec::new();
        // The bookmark's range, in paragraph order (`\b`).
        let scope: Option<(ParaId, ParaId)> = o.bookmark.as_deref().and_then(|b| {
            let f = self.bookmark(b)?;
            Some((f.start.para, f.end.map_or(f.start.para, |e| e.para)))
        });
        let mut inside = scope.is_none();
        // Sequences, as field evaluation counts them
        let mut seq = fields::Sequences::default();
        let mut walk = |p: &Paragraph, in_box: bool, out: &mut Vec<TocEntry>| {
            if out.len() >= MAX_ENTRIES {
                return;
            }
            let id = ParaId(p.id);
            if scope.is_some_and(|(a, _)| a == id) {
                inside = true;
            }
            let in_scope = inside;
            if scope.is_some_and(|(_, b)| b == id) {
                inside = false;
            }
            // Sequence numbers so far (`\s` and `\c` count them in order).
            let mut seq_here: Vec<String> = Vec::new();
            for s in fields::spans(&p.inlines) {
                if let Inline::FieldBegin { kind: Some(FieldKind::Seq(n)), run, .. } = &p.inlines[s.begin] {
                    let code = fields::code_text(&p.inlines, s);
                    seq.next(n, code.trim(), &fields::words(&code), run.rev.del.is_some());
                    seq_here.push(n.to_ascii_lowercase());
                }
            }
            seq.passed(p);
            if !in_scope || skip.contains(&id) {
                return;
            }
            let prefix = o.sequence.as_ref().map(|(n, _)| seq.current(n).to_string());
            let entry = |level: u8, pieces: Vec<(EntryPiece, Option<Run>)>, from_field: bool, page: bool| TocEntry {
                para: id,
                level,
                text: pieces_text(&pieces),
                page: page && o.page_shown(level),
                from_field,
                prefix: prefix.clone(),
                runs: pieces,
            };
            // Captions
            if let Some(cap) = &o.captions
                && seq_here.iter().any(|n| n.eq_ignore_ascii_case(&cap.label))
            {
                let mut pieces = entry_pieces(p, o, &review);
                if !cap.include_label {
                    pieces = after_label(pieces, p, &cap.label);
                }
                if !pieces.is_empty() {
                    out.push(entry(1, pieces, false, true));
                }
            }
            // A text box's captions are entries; its headings, styled paragraphs and entry fields aren't (Word).
            if in_box {
                return;
            }
            // The paragraph's own level
            let name = name_of(p);
            let styled = name.as_deref().and_then(|n| o.styles.iter().find(|(s, _)| s.eq_ignore_ascii_case(n)).map(|s| s.1));
            let heading = name.as_deref().and_then(|n| {
                let (word, l) = n.split_once(' ')?;
                let l: u8 = l.trim().parse().ok()?;
                (word.eq_ignore_ascii_case("heading") && (1..=9).contains(&l)).then_some(l)
            });
            let in_range = |l: u8| o.headings.is_none_or(|(a, b)| a <= l && l <= b);
            let level = styled.or_else(|| heading.filter(|&l| o.headings.is_some_and(|(a, b)| a <= l && l <= b))).or_else(|| {
                let l = p.style.outline_lvl.filter(|l| (0..9).contains(l))? as u8 + 1;
                (o.outline_levels && in_range(l)).then_some(l)
            });
            if let Some(level) = level {
                let pieces = entry_pieces(p, o, &review);
                if !pieces.is_empty() {
                    out.push(entry(level, pieces, false, true));
                }
            }
            // Table entry fields
            // `\l` alone collects identifier C's too.
            if let Some(id) = o.entry_fields.as_deref().or(o.entry_levels.map(|_| "C")) {
                for s in fields::spans(&p.inlines) {
                    if out.len() >= MAX_ENTRIES {
                        break;
                    }
                    let Some(m) = TocMark::of_code(&fields::code_text(&p.inlines, s)) else { continue };
                    if !m.identifier.as_deref().unwrap_or("C").eq_ignore_ascii_case(id) {
                        continue;
                    }
                    if o.entry_levels.is_some_and(|(a, b)| m.level < a || m.level > b) {
                        continue;
                    }
                    let run = p.inlines[s.begin].run().cloned();
                    out.push(entry(m.level, vec![(EntryPiece::Text(m.text.clone()), run)], true, m.page_number));
                }
            }
        };
        for s in &self.doc.sections {
            fields::each_para(&s.blocks, false, &mut |p, in_box| walk(p, in_box, &mut out));
        }
        out
    }

    /// The table of contents the selection is in, else the document's first.
    pub fn toc_here(&self) -> Option<(usize, TocField)> {
        let all = self.tables_of_contents();
        let k = self.toc_at(&all).or((!all.is_empty()).then_some(0))?;
        Some((k, all[k].clone()))
    }

    /// Whether updating table `t` is refused: its field is locked, or a control around it locks its content.
    pub fn toc_locked(&self, t: &TocField) -> bool {
        t.lock
            || self.index.blocks(&self.doc, t.container).and_then(|bl| bl.get(t.first)).is_some_and(|b| {
                super::merge::block_controls(b).iter().any(|c| {
                    self.doc.controls.get(c).is_some_and(|k| matches!(k.props.lock, crate::docx::Lock::ContentLocked | crate::docx::Lock::Both))
                })
            })
    }

    /// The table of contents the selection is in (its place in [`Editor::tables_of_contents`]), if any.
    pub fn toc_at_selection(&self) -> Option<(usize, TocField)> {
        let all = self.tables_of_contents();
        let k = self.toc_at(&all)?;
        Some((k, all[k].clone()))
    }

    /// Updates every table of contents and of figures as `mode` says (Word's update before printing), the locked ones
    /// left. One undo step; returns how many were updated. An entire update takes the first [`MAX_REBUILT`] tables.
    pub fn update_all_tables(&mut self, mode: TocUpdate, fonts: &Fonts) -> usize {
        let all: Vec<TocField> = self.tables_of_contents().into_iter().filter(|t| !self.toc_locked(t)).collect();
        if all.is_empty() {
            return 0;
        }
        self.begin_batch(Some("Update Table".into()));
        let spans: Vec<_> = all.iter().filter_map(|t| self.toc_span(t)).collect();
        self.set_span_views(&spans, false, fonts);
        let all: Vec<TocField> = self.tables_of_contents().into_iter().filter(|t| !self.toc_locked(t)).collect();
        let n = match mode {
            TocUpdate::Pages => all.len(),
            // Last first: a table's rebuild moves only the blocks after it.
            TocUpdate::Entire => {
                all[..all.len().min(MAX_REBUILT)].iter().rev().filter(|t| matches!(self.rebuild_toc(t, fonts), Outcome::Changed(_))).count()
            }
        };
        let again: Vec<TocField> = self.tables_of_contents().into_iter().filter(|t| !self.toc_locked(t)).collect();
        self.fill_pages(&again, fonts);
        for t in &again {
            self.undirty(t, fonts);
        }
        self.end_batch(true, fonts);
        n
    }

    /// Table `t`'s field across paragraphs.
    fn toc_span(&self, t: &TocField) -> Option<super::span_codes::SpanField> {
        let Shape::Spanning { start, end } = t.shape else { return None };
        Some(super::span_codes::SpanField { container: t.container, start: (t.first, start), end: (t.last, end) })
    }

    /// Table `t` showing its result (a table showing its code shows its result first: an update or removal works on
    /// the result).
    fn toc_result_shown(&mut self, t: &TocField, fonts: &Fonts) -> TocField {
        let Some(span) = self.toc_span(t).filter(|s| self.span_shows_code(s)) else { return t.clone() };
        self.set_span_views(&[span], false, fonts);
        self.tables_of_contents().into_iter().find(|x| x.container == t.container && x.first == t.first).unwrap_or_else(|| t.clone())
    }

    /// Which of `all` the selection is in: its result's paragraphs, or a building block control's title above them.
    fn toc_at(&self, all: &[TocField]) -> Option<usize> {
        let (para, _) = self.selection.focus().and_then(|p| p.para_offset())?;
        let slot = self.index.para_slot(para)?;
        let blocks = self.index.blocks(&self.doc, slot.container)?;
        all.iter().position(|t| {
            t.container == slot.container
                && (t.first <= slot.block || self.toc_blocks_from(t) <= slot.block)
                && (slot.block < t.last || slot.block == t.last && (!t.ends_at_start(blocks) || self.toc_control_holds_end(t, blocks)))
        })
    }

    /// Whether a built-in table's building block control holds its last paragraph (the end's own, Word's gallery).
    fn toc_control_holds_end(&self, t: &TocField, blocks: &[Block]) -> bool {
        let (Some(a), Some(b)) = (blocks.get(t.first), blocks.get(t.last)) else { return false };
        super::merge::block_controls(a).last().is_some_and(|c| self.is_toc_control(c) && super::merge::block_controls(b).contains(c))
    }

    /// The first block a table's removal takes: its first paragraph, or the first of a building block control (Word's
    /// built-in tables) it is in.
    fn toc_blocks_from(&self, t: &TocField) -> usize {
        let Some(blocks) = self.index.blocks(&self.doc, t.container) else { return t.first };
        let Some(c) = blocks.get(t.first).and_then(|b| super::merge::block_controls(b).last()).filter(|c| self.is_toc_control(c)) else {
            return t.first;
        };
        let mut from = t.first;
        while from > 0 && super::merge::block_controls(&blocks[from - 1]).contains(c) {
            from -= 1;
        }
        from
    }

    /// Whether control `c` is a table of contents building block.
    fn is_toc_control(&self, c: &crate::docx::ControlId) -> bool {
        self.doc
            .controls
            .get(c)
            .and_then(|k| k.props.kind_xml.as_deref())
            .is_some_and(|x| x.contains("docPartGallery") && x.contains("\"Table of Contents\""))
    }

    /// Removes table `t` (Word's Remove Table of Contents): its field and result, with the building block control and
    /// title of a built-in table. What shares its first and last paragraphs stays. One undo step; a lock on the content
    /// refuses.
    pub fn remove_table_of_contents(&mut self, t: &TocField, fonts: &Fonts) -> Outcome {
        self.begin_batch(Some("Remove Table of Contents".into()));
        let t = &self.toc_result_shown(t, fonts);
        let out = self.remove_toc(t, fonts);
        let ok = out.is_ok();
        let res = self.end_batch(ok, fonts);
        match out {
            Ok(()) => res,
            Err(o) => o,
        }
    }

    /// [`Editor::remove_table_of_contents`] in a batch; the caret ends where the table was.
    fn remove_toc(&mut self, t: &TocField, fonts: &Fonts) -> Result<(), Outcome> {
        let Some(blocks) = self.index.blocks(&self.doc, t.container) else { return Err(Outcome::unavailable("no table of contents")) };
        let (Some(Block::Para(first)), Some(Block::Para(last))) = (blocks.get(t.first), blocks.get(t.last)) else {
            return Err(Outcome::unavailable("no table of contents"));
        };
        let from = self.toc_blocks_from(t);
        let (start, end) = match t.shape {
            Shape::Spanning { start, end } => (start, end),
            Shape::Inline { begin, end } => (begin, end),
        };
        let units = |inl: &[Inline]| Unit(inl.iter().map(Inline::units).sum::<usize>() as u32);
        let head: Vec<Inline> = if from < t.first { Vec::new() } else { first.inlines[..start].to_vec() };
        let tail: Vec<Inline> = last.inlines[end + 1..].to_vec();
        let b = (ParaId(last.id), units(&last.inlines[..=end]));
        let a_unit = if from < t.first { Unit(0) } else { units(&head) };
        let n = blocks.len();
        let (first, last) = (first.clone(), last.clone());
        let Some(a_para) = blocks_para_at(self, t.container, from) else { return Err(Outcome::unavailable("no table of contents")) };
        let a = (a_para, a_unit);
        // The lock checks of deleting what it covers.
        let was = self.selection;
        self.select_between(a, b, fonts);
        if self.refuse_locked() {
            self.selection = was;
            return Err(Outcome::Unchanged("locked"));
        }
        let mut tx = self.begin("Remove Table of Contents");
        // Tracked: a tracked deletion of the same range.
        if tx.tracked {
            return match self.delete_range(&mut tx) {
                Ok(Some(at)) => {
                    self.selection = Selection::Caret(at);
                    self.commit(tx, None, fonts);
                    Ok(())
                }
                _ => {
                    self.abort(tx);
                    Err(Outcome::Unchanged("the table couldn't be removed"))
                }
            };
        }
        let shows = |inl: &[Inline]| inl.iter().any(|i| i.units() > 0 || matches!(i, Inline::Bookmark(_)));
        // The paragraph left: what came before the field in its first, then what follows it in its last, whose mark
        // is after the field (unless a building block control holds it).
        let in_control = last.controls.iter().any(|c| self.is_toc_control(c));
        let keep = if shows(&head) {
            Some(Paragraph { inlines: head.into_iter().chain(tail).collect(), ..first })
        } else if shows(&tail) || !in_control || t.last + 1 == n {
            let mut p = last;
            p.controls.retain(|c| !self.is_toc_control(c));
            p.inlines = if tail.is_empty() { vec![Inline::Text(String::new(), p.mark.clone())] } else { tail };
            Some(p)
        } else {
            None
        };
        let caret = keep.as_ref().map_or_else(|| blocks_para_at(self, t.container, t.last + 1), |p| Some(ParaId(p.id)));
        let with: Vec<Block> = keep.into_iter().map(Block::Para).collect();
        if self.step(&mut tx, Step::Blocks { container: t.container, range: from..t.last + 1, with }).is_err() {
            self.abort(tx);
            return Err(Outcome::Unchanged("the table couldn't be removed"));
        }
        if let Some(p) = caret {
            let story = self.index.story(t.container);
            self.selection = Selection::Caret(Position::text(story, p, Unit(0)));
        }
        self.commit(tx, None, fonts);
        Ok(())
    }

    /// Inserts a table of contents (or of figures) at the selection: its field over a paragraph per entry, each in its
    /// level's TOC style (the table of figures style for captions) with a right tab stop at the margin (`look`), a link
    /// to its source and a page number as `o` says, with a hidden `_Toc` bookmark on every source. `frame` adds a title
    /// and a building block control. One undo step; page numbers come from the layout once it is done.
    pub fn insert_table_of_contents(&mut self, o: &TocOptions, look: TocLook, frame: &TocFrame, fonts: &Fonts) -> Outcome {
        if self.style_target.is_some() {
            return Outcome::unavailable("not in a style");
        }
        if self.refuse_replacing() {
            return Outcome::Unchanged("locked");
        }
        let Some(caret) = self.selection.focus().filter(|p| p.para_offset().is_some()) else { return Outcome::unavailable("no caret") };
        if caret.story != StoryId::Body {
            return Outcome::unavailable("only the body holds a table of contents");
        }
        self.begin_batch(Some(if o.captions.is_some() { "Insert Table of Figures" } else { "Insert Table of Contents" }.into()));
        // In a table: it is replaced (Word's "replace the selected table of contents").
        let all = self.tables_of_contents();
        if let Some(k) = self.toc_at(&all)
            && let t = self.toc_result_shown(&all[k], fonts)
            && let Err(out) = self.remove_toc(&t, fonts)
        {
            self.end_batch(false, fonts);
            return out;
        }
        if !self.ensure_toc_styles(o, frame.title.is_some(), fonts) {
            self.end_batch(false, fonts);
            return Outcome::Unchanged("the table's styles couldn't be added");
        }
        // The selection goes first.
        if !self.selection.is_collapsed() {
            let mut tx = self.begin("Insert Table of Contents");
            match self.delete_range(&mut tx) {
                Ok(Some(at)) => {
                    self.selection = Selection::Caret(at);
                    self.commit(tx, None, fonts);
                }
                _ => {
                    self.abort(tx);
                    self.end_batch(false, fonts);
                    return Outcome::Unchanged("the selection couldn't be replaced");
                }
            }
        }
        let Some((para, offset)) = self.selection.focus().and_then(|p| p.para_offset()) else {
            self.end_batch(false, fonts);
            return Outcome::unavailable("no caret");
        };
        // Mid paragraph, the table goes before its rest.
        if offset > Unit(0) {
            let mut tx = self.begin("Insert Table of Contents");
            let second = self.split_template(para).and_then(|second| {
                let id = ParaId(second.id);
                self.step(&mut tx, Step::Split { para, at: offset, second, kept: Vec::new() }).ok().map(|_| id)
            });
            match second {
                Some(p) => {
                    self.selection = Selection::Caret(Position::text(StoryId::Body, p, Unit(0)));
                    self.commit(tx, None, fonts);
                }
                None => {
                    self.abort(tx);
                    self.end_batch(false, fonts);
                    return Outcome::Unchanged("the paragraph couldn't be split");
                }
            }
        }
        let Some((para, _)) = self.selection.focus().and_then(|p| p.para_offset()) else {
            self.end_batch(false, fonts);
            return Outcome::unavailable("no caret");
        };
        let Some(slot) = self.index.para_slot(para) else {
            self.end_batch(false, fonts);
            return Outcome::unavailable("no caret");
        };
        let mut tx = self.begin("Insert Table of Contents");
        let list_sep = self.doc.settings.list_separator.unwrap_or(',');
        let code = o.code(list_sep);
        let entries = self.toc_entries(o);
        let marks = self.mark_sources(&mut tx, &entries);
        // The end goes at the start of the paragraph the caret was in.
        let Some(here) = self.index.para(&self.doc, para).cloned() else {
            self.abort(tx);
            self.end_batch(false, fonts);
            return Outcome::unavailable("no caret");
        };
        let mut blocks = self.toc_blocks(o, look, &code, &entries, &marks, slot.container, &here);
        let end = FieldEdge::End.inline(&self.cx().w, here.mark.clone());
        // In a building block control the end has its own paragraph inside it (Word's gallery tables).
        let end = if frame.control {
            let mut p = self.new_para(&format!("<{}pPr/>", self.cx().w), &[], slot.container, &here);
            p.inlines = vec![end];
            blocks.push(Block::Para(p));
            None
        } else {
            Some(end)
        };
        if let Some(title) = &frame.title {
            blocks.insert(0, Block::Para(self.plain_para("TOCHeading", title, slot.container, &here)));
        }
        let n = blocks.len();
        // In the caret paragraph's controls, the building block's innermost.
        let mut controls = here.controls.clone();
        if frame.control {
            let id = crate::docx::ControlId::next();
            let control = self.toc_control();
            let _ = self.step(&mut tx, Step::Control { id, control: Some(control) });
            controls.push(id);
        }
        for b in &mut blocks[..n] {
            if let Block::Para(p) = b {
                p.controls = controls.clone();
            }
        }
        // The table before the caret's paragraph, its end first in that paragraph (before its bookmarks).
        let under = here.inlines.iter().take_while(|i| i.units() == 0).filter(|i| !matches!(i, Inline::Text(..))).count() as u32;
        let edges = super::step::Edges { under, ..Default::default() };
        if self.step(&mut tx, Step::Blocks { container: slot.container, range: slot.block..slot.block, with: blocks }).is_err()
            || end.is_some_and(|end| self.step(&mut tx, Step::Text { para, range: Unit(0)..Unit(0), with: vec![end], edges }).is_err())
        {
            self.abort(tx);
            self.end_batch(false, fonts);
            return Outcome::Unchanged("the table couldn't be inserted");
        }
        self.selection = Selection::Caret(Position::text(StoryId::Body, para, Unit(0)));
        self.commit(tx, None, fonts);
        self.fill_toc_pages(slot.container, slot.block + usize::from(frame.title.is_some()), fonts);
        self.end_batch(true, fonts)
    }

    /// Inserts Word's Manual Table gallery item at the caret: a building block control holding a "Table of Contents"
    /// title and placeholder entries for levels 1 to 3 (twice) with typed page numbers, no field. One undo step.
    pub fn insert_manual_table_of_contents(&mut self, fonts: &Fonts) -> Outcome {
        if self.style_target.is_some() {
            return Outcome::unavailable("not in a style");
        }
        if self.refuse_replacing() {
            return Outcome::Unchanged("locked");
        }
        let Some((para, offset)) = self.selection.focus().filter(|p| p.story == StoryId::Body).and_then(|p| p.para_offset()) else {
            return Outcome::unavailable("only the body holds a table of contents");
        };
        if offset > Unit(0) || !self.selection.is_collapsed() {
            return Outcome::unavailable("at a paragraph's start");
        }
        self.begin_batch(Some("Insert Table of Contents".into()));
        let o = TocOptions { headings: Some((1, 3)), ..TocOptions::default() };
        if !self.ensure_toc_styles(&o, true, fonts) {
            self.end_batch(false, fonts);
            return Outcome::Unchanged("the table's styles couldn't be added");
        }
        let (Some(slot), Some(here)) = (self.index.para_slot(para), self.index.para(&self.doc, para).cloned()) else {
            self.end_batch(false, fonts);
            return Outcome::unavailable("no caret");
        };
        let w = self.cx().w;
        let right = ((self.text_width(slot.container) * 20.0).round() as i64 - 10).max(0);
        let id = crate::docx::ControlId::next();
        let mut blocks = vec![Block::Para(self.plain_para("TOCHeading", "Table of Contents", slot.container, &here))];
        for (k, level) in [1u8, 2, 3, 1, 2, 3].into_iter().enumerate() {
            let ppr = format!(
                "<{w}pPr><{w}pStyle {w}val=\"TOC{level}\"/><{w}tabs><{w}tab {w}val=\"right\" {w}leader=\"dot\" {w}pos=\"{right}\"/></{w}tabs></{w}pPr>"
            );
            let mut p = self.new_para(&ppr, &[(format!("Type chapter title (level {level})"), None, false)], slot.container, &here);
            let run = p.mark.clone();
            p.inlines.push(Inline::Tab(run.clone()));
            p.inlines.push(Inline::Text((k + 1).to_string(), run));
            blocks.push(Block::Para(p));
        }
        let mut controls = here.controls.clone();
        controls.push(id);
        for b in &mut blocks {
            if let Block::Para(p) = b {
                p.controls = controls.clone();
            }
        }
        let mut tx = self.begin("Insert Table of Contents");
        let control = self.toc_control();
        if self.step(&mut tx, Step::Control { id, control: Some(control) }).is_err()
            || self.step(&mut tx, Step::Blocks { container: slot.container, range: slot.block..slot.block, with: blocks }).is_err()
        {
            self.abort(tx);
            self.end_batch(false, fonts);
            return Outcome::Unchanged("the table couldn't be inserted");
        }
        self.commit(tx, None, fonts);
        self.end_batch(true, fonts)
    }

    /// Updates the table of contents at the selection (else the document's first): its page numbers, or every entry
    /// collected again with its code's switches kept. One undo step; a locked field refuses.
    pub fn update_table_of_contents(&mut self, mode: TocUpdate, fonts: &Fonts) -> Outcome {
        let Some((_, t)) = self.toc_here() else { return Outcome::unavailable("no table of contents") };
        self.update_toc(&t, mode, fonts)
    }

    /// Updates table `t` as [`Editor::update_table_of_contents`] does.
    pub fn update_toc(&mut self, t: &TocField, mode: TocUpdate, fonts: &Fonts) -> Outcome {
        if self.toc_locked(t) {
            self.refused = Some("locked");
            return Outcome::Unchanged("locked");
        }
        let label = "Update Table";
        match mode {
            TocUpdate::Pages => {
                self.begin_batch(Some(label.into()));
                // An updated field shows its result.
                let t = &self.toc_result_shown(t, fonts);
                self.complete(fonts);
                self.fill_toc_pages(t.container, t.first, fonts);
                self.undirty(t, fonts);
                self.end_batch(true, fonts)
            }
            TocUpdate::Entire => {
                self.begin_batch(Some(label.into()));
                let t = &self.toc_result_shown(t, fonts);
                let out = self.rebuild_toc(t, fonts);
                if !matches!(out, Outcome::Changed(_)) {
                    self.end_batch(false, fonts);
                    return out;
                }
                let Some(again) = self.tables_of_contents().into_iter().find(|x| x.container == t.container && x.first == t.first) else {
                    return self.end_batch(true, fonts);
                };
                self.fill_toc_pages(again.container, again.first, fonts);
                self.end_batch(true, fonts)
            }
        }
    }

    /// A spanning table's start marked dirty (its result to update) is clean once updated.
    fn undirty(&mut self, t: &TocField, fonts: &Fonts) {
        let Shape::Spanning { start, .. } = t.shape else { return };
        let Some(Block::Para(p)) = self.index.blocks(&self.doc, t.container).and_then(|b| b.get(t.first)) else { return };
        let Some(Inline::Opaque(o)) = p.inlines.get(start) else { return };
        let Some(FieldEdge::Start { code, lock, dirty: true, view }) = &o.field else { return };
        let mut inlines = p.inlines.clone();
        inlines[start] = FieldEdge::Start { code: code.clone(), lock: *lock, dirty: false, view: *view }
            .inline(&self.cx().w, o.run.clone().unwrap_or_else(|| p.mark.clone()));
        let para = ParaId(p.id);
        let mut tx = self.begin("Update Table");
        // A flag, not content: never a tracked change.
        tx.tracked = false;
        let _ = self.step(&mut tx, Step::Restyle { para, inlines });
        self.commit(tx, None, fonts);
    }

    fn rebuild_toc(&mut self, t: &TocField, fonts: &Fonts) -> Outcome {
        let list_sep = self.doc.settings.list_separator.unwrap_or(',');
        let o = t.options(list_sep);
        if !self.ensure_toc_styles(&o, false, fonts) {
            return Outcome::Unchanged("the table's styles couldn't be added");
        }
        let Some(look) = self.index.blocks(&self.doc, t.container).and_then(|b| b.get(t.first..=t.last)).map(|b| self.toc_look(b)) else {
            return Outcome::unavailable("no table");
        };
        let mut tx = self.begin("Update Table");
        let entries = self.toc_entries(&o);
        let marks = self.mark_sources(&mut tx, &entries);
        // Unused `_Toc` bookmarks the old entries linked to go.
        let w = self.cx().w;
        // The table's blocks (offset by `t.first`).
        let Some(blocks) = self.index.blocks(&self.doc, t.container).and_then(|b| b.get(t.first..=t.last)).map(<[Block]>::to_vec) else {
            self.abort(tx);
            return Outcome::unavailable("no table");
        };
        let (Some(Block::Para(first)), Some(Block::Para(last))) = (blocks.first(), blocks.last()) else {
            self.abort(tx);
            return Outcome::unavailable("no table");
        };
        // Tracked, an end left in place goes too, and a new one comes: each field whole, deleted or inserted.
        let mut end_kept: Option<Rc<crate::docx::Opaque>> = None;
        let mut new = self.toc_blocks(&o, look, &t.code, &entries, &marks, t.container, first);
        // In the controls the table was in.
        for b in &mut new {
            if let Block::Para(p) = b {
                p.controls = first.controls.clone();
            }
        }
        let (range, start_edge, before, after): (std::ops::Range<usize>, Inline, Vec<Inline>, Option<Vec<Inline>>) = match t.shape {
            Shape::Spanning { start, end } => {
                let end_only = last.inlines[..end].iter().all(|i| i.units() == 0 && edge(i).is_none());
                let after = (!end_only).then(|| last.inlines[end..].to_vec());
                let range = if end_only { t.first..t.last } else { t.first..t.last + 1 };
                if let (true, Some(Inline::Opaque(o))) = (end_only, last.inlines.get(end)) {
                    end_kept = Some(o.clone());
                }
                // Updated, it is no longer dirty.
                let edge = match &first.inlines[start] {
                    Inline::Opaque(o) => match &o.field {
                        Some(FieldEdge::Start { code, lock, dirty: true, view }) => {
                            let run = o.run.clone().unwrap_or_else(|| first.mark.clone());
                            FieldEdge::Start { code: code.clone(), lock: *lock, dirty: false, view: *view }.inline(&w, run)
                        }
                        _ => first.inlines[start].clone(),
                    },
                    other => other.clone(),
                };
                (range, edge, first.inlines[..start].to_vec(), after)
            }
            Shape::Inline { begin, end } => {
                let Inline::FieldBegin { lock, .. } = &first.inlines[begin] else {
                    self.abort(tx);
                    return Outcome::unavailable("no table");
                };
                let edge =
                    FieldEdge::Start { code: t.code.clone(), lock: *lock, dirty: false, view: FieldView::Result }.inline(&w, first.mark.clone());
                let mut after = vec![FieldEdge::End.inline(&w, first.mark.clone())];
                after.extend(first.inlines[end + 1..].iter().cloned());
                (t.first..t.first + 1, edge, first.inlines[..begin].to_vec(), Some(after))
            }
        };
        // The start edge as it was, after what came before it in its paragraph.
        if let Some(Block::Para(p)) = new.first_mut() {
            if let Some(k) = p.inlines.iter().position(|i| edge(i).is_some()) {
                p.inlines[k] = start_edge;
            }
            let mut lead = before;
            lead.append(&mut p.inlines);
            p.inlines = lead;
        }
        if let Some(after) = after
            && let Some(Block::Para(p)) = new.last_mut()
        {
            let keep_empty = p.inlines.iter().all(|i| matches!(i, Inline::Text(t, _) if t.is_empty()) || edge(i).is_some());
            if keep_empty {
                p.inlines.retain(|i| edge(i).is_some());
            }
            p.inlines.extend(after);
        }
        let old_marks: HashSet<String> = blocks[range.start - t.first..range.end - t.first]
            .iter()
            .filter_map(|b| if let Block::Para(p) = b { Some(p) } else { None })
            .flat_map(|p| p.inlines.iter())
            .filter_map(|i| match i {
                Inline::FieldBegin { kind: Some(FieldKind::PageRef(n)), .. } => Some(n.to_ascii_lowercase()),
                _ => None,
            })
            .filter(|n| n.starts_with("_toc"))
            .collect();
        let rest = ParaId(last.id);
        if self.step(&mut tx, Step::Blocks { container: t.container, range, with: new }).is_err() {
            self.abort(tx);
            return Outcome::Unchanged("the table couldn't be updated");
        }
        if let Some(o) = end_kept.filter(|_| tx.tracked)
            && let Some(rest) = self.index.para(&self.doc, rest)
        {
            let para = ParaId(rest.id);
            let run = o.run.clone().unwrap_or_else(|| rest.mark.clone());
            let cut = vec![CutEdge { edge: o, keep: false, para }];
            let deleted = self.delete_whole_fields(&mut tx, cut, None).is_ok();
            // The new end right after the deleted one.
            let under = self.index.para(&self.doc, para).map_or(0, |p| {
                let lead: Vec<&Inline> = p.inlines.iter().take_while(|i| i.units() == 0).filter(|i| !matches!(i, Inline::Text(..))).collect();
                let k = lead.iter().position(|i| edge(i).is_some()).map_or(0, |k| k + 1);
                (lead.len() - k) as u32
            });
            let end = FieldEdge::End.inline(&w, run);
            if !deleted
                || self
                    .step(
                        &mut tx,
                        Step::Text { para, range: Unit(0)..Unit(0), with: vec![end], edges: super::step::Edges { under, ..Default::default() } },
                    )
                    .is_err()
            {
                self.abort(tx);
                return Outcome::Unchanged("the table couldn't be updated");
            }
        }
        let kept: HashSet<String> = marks.values().map(|n| n.to_ascii_lowercase()).collect();
        self.drop_toc_marks(&mut tx, &old_marks, &kept);
        self.commit(tx, None, fonts)
    }

    /// Takes out the hidden `_Toc` bookmarks of `old` no entry links to now, unless another field reads them.
    fn drop_toc_marks(&mut self, tx: &mut super::Tx, old: &HashSet<String>, kept: &HashSet<String>) {
        let gone: Vec<&String> = old.iter().filter(|n| !kept.contains(*n)).collect();
        if gone.is_empty() {
            return;
        }
        let mut read: HashSet<String> = HashSet::new();
        for para in self.paras_in_order() {
            let Some(p) = self.index.para(&self.doc, para) else { continue };
            for i in &p.inlines {
                if let Inline::FieldBegin { kind: Some(FieldKind::PageRef(n) | FieldKind::Ref(n) | FieldKind::NoteRef(n, _)), .. } = i {
                    read.insert(n.to_ascii_lowercase());
                }
            }
        }
        let gone: HashSet<&str> = gone.into_iter().filter(|n| !read.contains(*n)).map(String::as_str).collect();
        if gone.is_empty() {
            return;
        }
        // The paragraphs their markers are in, from one walk of the bookmarks.
        let mut paras: Vec<ParaId> = Vec::new();
        for f in self.bookmarks() {
            if gone.contains(f.name.to_ascii_lowercase().as_str()) {
                paras.push(f.start.para);
                paras.extend(f.end.map(|e| e.para));
            }
        }
        paras.sort_unstable_by_key(|p| p.0);
        paras.dedup();
        let dropped = |i: &Inline| matches!(i, Inline::Bookmark(b) if gone.contains(b.trim_start_matches('/').to_ascii_lowercase().as_str()));
        for para in paras {
            let Some(p) = self.index.para(&self.doc, para) else { continue };
            if p.inlines.iter().any(dropped) {
                let inlines: Vec<Inline> = p.inlines.iter().filter(|i| !dropped(i)).cloned().collect();
                let _ = self.step(tx, Step::Restyle { para, inlines });
            }
        }
    }

    /// The look of a table's entries: its first entry's right tab stop, if any.
    fn toc_look(&self, blocks: &[Block]) -> TocLook {
        let tabs = blocks.iter().find_map(|b| match b {
            Block::Para(p) if p.inlines.iter().any(|i| i.units() > 0) => Some(&p.style.tabs),
            _ => None,
        });
        match tabs.and_then(|t| t.iter().rev().find(|t| t.kind == crate::docx::props::TabKind::Right)) {
            Some(t) => TocLook { right_align: true, leader: t.leader },
            None if tabs.is_some() => TocLook { right_align: false, leader: Leader::None },
            None => TocLook::default(),
        }
    }

    /// Adds the built-in styles a table's entries (and title) take that the document lacks.
    fn ensure_toc_styles(&mut self, o: &TocOptions, title: bool, fonts: &Fonts) -> bool {
        let mut ids: Vec<String> = Vec::new();
        if o.captions.is_some() {
            ids.push("TableofFigures".into());
        }
        if o.captions.is_none()
            || o.headings.is_some()
            || o.outline_levels
            || !o.styles.is_empty()
            || o.entry_fields.is_some()
            || o.entry_levels.is_some()
        {
            let max = o.styles.iter().map(|s| s.1).chain(o.headings.map(|h| h.1)).max().unwrap_or(if o.outline_levels { 9 } else { 3 });
            let max = if o.entry_fields.is_some() || o.entry_levels.is_some() { 9 } else { max };
            ids.extend((1..=max.clamp(1, 9)).map(|n| format!("TOC{n}")));
        }
        if title {
            ids.push("TOCHeading".into());
        }
        for id in ids {
            let has = |e: &Editor| e.doc.resolver.as_ref().is_some_and(|r| r.styles.by_id.contains_key(&id));
            if has(self) {
                continue;
            }
            let Some(b) = crate::services::styles::builtin::built_in(&id) else { return false };
            // TOC Heading is based on Heading 1.
            if id == "TOCHeading"
                && !self.doc.resolver.as_ref().is_some_and(|r| r.styles.by_id.contains_key("Heading1"))
                && let Some(h) = crate::services::styles::builtin::built_in("Heading1")
            {
                self.add_built_in(h, fonts);
            }
            self.add_built_in(b, fonts);
            if !has(self) {
                return false;
            }
        }
        true
    }

    /// The hidden `_Toc` bookmark of every entry's source paragraph: one it has, else a new one around its text (put
    /// in with `tx`). By paragraph.
    fn mark_sources(&mut self, tx: &mut super::Tx, entries: &[TocEntry]) -> HashMap<ParaId, String> {
        let mut out: HashMap<ParaId, String> = HashMap::new();
        let all = self.bookmarks();
        let taken: HashSet<String> = all.iter().map(|b| b.name.to_ascii_lowercase()).collect();
        let mut next = first_free("_Toc", all.iter().map(|b| b.name.as_str()));
        let mut have: HashMap<ParaId, String> = HashMap::new();
        for b in &all {
            if b.name.len() > 4 && b.name[..4].eq_ignore_ascii_case("_toc") && b.start.unit == Unit(0) && b.end.is_none_or(|e| e.para == b.start.para)
            {
                have.entry(b.start.para).or_insert_with(|| b.name.clone());
            }
        }
        for e in entries {
            if out.contains_key(&e.para) {
                continue;
            }
            if let Some(n) = have.get(&e.para) {
                out.insert(e.para, n.clone());
                continue;
            }
            let Some(p) = self.index.para(&self.doc, e.para) else { continue };
            let len = para_len(p);
            let name = free_name("_Toc", &taken, &mut next);
            let end = Step::Text { para: e.para, range: len..len, with: vec![Inline::Bookmark(format!("/{name}"))], edges: Default::default() };
            let start = Step::Text { para: e.para, range: Unit(0)..Unit(0), with: vec![Inline::Bookmark(name.clone())], edges: Default::default() };
            if self.step(tx, end).is_ok() && self.step(tx, start).is_ok() {
                out.insert(e.para, name);
            }
        }
        out
    }

    /// A paragraph in style `style` holding `text`, for `container`.
    fn plain_para(&self, style: &str, text: &str, container: Container, like: &Paragraph) -> Paragraph {
        let cx = self.cx();
        let w = &cx.w;
        let ppr = format!("<{w}pPr><{w}pStyle {w}val=\"{}\"/></{w}pPr>", crate::docx::write::props::esc(style));
        self.new_para(&ppr, &[(text.to_string(), None, false)], container, like)
    }

    /// A new paragraph from its `w:pPr` and its texts (each with its source run's direct formatting, and bold).
    pub(super) fn new_para(&self, ppr: &str, texts: &[(String, Option<&Run>, bool)], container: Container, like: &Paragraph) -> Paragraph {
        let cx = self.cx();
        let res = self.doc.resolver.clone();
        let region = res.as_ref().and_then(|r| self.region_in(container, r));
        // Without styles, `like`'s.
        let (style, base) = match &res {
            Some(r) => {
                let x = r.para_from_xml(Some(ppr), region.as_ref(), &cx);
                (Rc::new(x.style), Some(x.base))
            }
            None => (like.style.clone(), None),
        };
        let run_of = |src: Option<&Run>, bold: bool| -> Run {
            match (&res, &base) {
                (Some(r), Some(b)) => {
                    let mut rpr = src.and_then(|s| s.direct.0.as_deref()).map(strip_rpr).unwrap_or_default();
                    if bold {
                        rpr.push_str(&format!("<{w}b/><{w}bCs/>", w = cx.w));
                    }
                    let xml = (!rpr.is_empty()).then(|| format!("<{w}rPr>{rpr}</{w}rPr>", w = cx.w));
                    r.run_from_xml(b, xml.as_deref(), &cx)
                }
                _ => src.cloned().unwrap_or_else(|| like.mark.clone()),
            }
        };
        let mark = run_of(None, false);
        let mut inlines: Vec<Inline> = texts.iter().map(|(t, src, bold)| Inline::Text(t.clone(), run_of(*src, *bold))).collect();
        if inlines.is_empty() {
            inlines.push(Inline::Text(String::new(), mark.clone()));
        }
        Paragraph { style, mark, list: None, inlines, src: None, id: crate::docx::next_para_id(), controls: Vec::new(), div: None }
    }

    /// A run of a new paragraph (`w:pPr` `ppr`, in `container`) hidden in a web view (`w:webHidden`).
    fn web_hidden(&self, ppr: &str, container: Container, run: &Run) -> Run {
        let Some(r) = self.doc.resolver.clone() else { return run.clone() };
        let cx = self.cx();
        let base = r.para_from_xml(Some(ppr), self.region_in(container, &r).as_ref(), &cx).base;
        r.run_from_xml(&base, Some(&format!("<{w}rPr><{w}webHidden/></{w}rPr>", w = cx.w)), &cx)
    }

    /// The table region a paragraph in `container` takes its table style's formatting from.
    fn region_in(&self, container: Container, r: &crate::docx::resolve::Resolver) -> Option<crate::docx::styles::Region> {
        let Container::Cell { table, row, cell } = container else { return None };
        self.index.table(&self.doc, table).map(|t| r.region_of(t, row, cell))
    }

    /// The building block control Word's built-in tables of contents are in.
    fn toc_control(&self) -> crate::docx::Control {
        let w = self.cx().w;
        let kind_xml = format!("<{w}docPartObj><{w}docPartGallery {w}val=\"Table of Contents\"/><{w}docPartUnique/></{w}docPartObj>");
        let props = crate::docx::ControlProps { kind: crate::docx::ControlKind::Other, kind_xml: Some(kind_xml), ..Default::default() };
        crate::docx::Control { props, open: String::new(), close: String::new(), read: None, at: None }
    }

    /// A table's result paragraphs: the start edge (code `code`) leading the first, one paragraph per entry, or Word's
    /// notice when there are none.
    #[allow(clippy::too_many_arguments)]
    fn toc_blocks(
        &self,
        o: &TocOptions,
        look: TocLook,
        code: &str,
        entries: &[TocEntry],
        marks: &HashMap<ParaId, String>,
        container: Container,
        like: &Paragraph,
    ) -> Vec<Block> {
        let cx = self.cx();
        let w = cx.w.clone();
        let width = self.text_width(container);
        let right = ((width * 20.0).round() as i64 - 10).max(0);
        let figures = o.captions.is_some();
        let start = FieldEdge::Start { code: code.to_string(), lock: false, dirty: false, view: FieldView::Result }.inline(&w, like.mark.clone());
        let mut out: Vec<Block> = Vec::with_capacity(entries.len().max(1));
        for e in entries {
            let style = if figures && !e.from_field { "TableofFigures".to_string() } else { format!("TOC{}", e.level.clamp(1, 9)) };
            let numbered = e.runs.first().is_some_and(|(_, r)| r.is_none()) && e.runs.get(1).is_some_and(|(p, _)| *p == EntryPiece::Tab);
            let mut tabs = String::new();
            if numbered {
                tabs.push_str(&format!("<{w}tab {w}val=\"left\" {w}pos=\"{}\"/>", 440 * i64::from(e.level)));
            }
            if look.right_align && e.page && o.separator.is_none() {
                tabs.push_str(&format!("<{w}tab {w}val=\"right\" {w}leader=\"{}\" {w}pos=\"{right}\"/>", leader_xml(look.leader)));
            }
            let tabs = if tabs.is_empty() { String::new() } else { format!("<{w}tabs>{tabs}</{w}tabs>") };
            let ppr = format!("<{w}pPr><{w}pStyle {w}val=\"{style}\"/>{tabs}</{w}pPr>");
            let mut p = self.new_para(&ppr, &[], container, like);
            let base = p.mark.clone();
            let run_for = |src: &Option<Run>| -> Run {
                let Some(src) = src else { return base.clone() };
                let mut q = self.new_para(&ppr, &[(String::new(), Some(src), false)], container, like);
                match q.inlines.pop() {
                    Some(Inline::Text(_, r)) => r,
                    _ => base.clone(),
                }
            };
            let mut inlines: Vec<Inline> = Vec::new();
            for (piece, src) in &e.runs {
                let run = run_for(src);
                inlines.push(match piece {
                    EntryPiece::Text(t) => Inline::Text(t.clone(), run),
                    EntryPiece::Tab => Inline::Tab(run),
                    EntryPiece::Break => Inline::Break(crate::docx::BreakKind::Line, run),
                });
            }
            let mark = marks.get(&e.para);
            if e.page
                && let Some(name) = mark
            {
                // `\z`: the leader and page number hidden in a web view.
                let page_run = if o.hide_in_web { self.web_hidden(&ppr, container, &base) } else { base.clone() };
                match &o.separator {
                    Some(s) => inlines.push(Inline::Text(s.clone(), page_run.clone())),
                    None => inlines.push(Inline::Tab(page_run.clone())),
                }
                if let (Some(pre), Some((_, d))) = (&e.prefix, &o.sequence) {
                    inlines.push(Inline::Text(format!("{pre}{}", d.as_deref().unwrap_or("-")), page_run.clone()));
                }
                let h = if o.hyperlinks { " \\h" } else { "" };
                inlines.extend(fields::field_inlines(&format!(" PAGEREF {name}{h} "), "", &page_run));
            }
            if o.hyperlinks
                && let Some(name) = mark
            {
                let mut linked = fields::field_inlines(&format!(" HYPERLINK \\l \"{name}\" "), "", &base);
                let end = linked.pop();
                linked.extend(inlines);
                linked.extend(end);
                inlines = linked;
            }
            if out.is_empty() {
                inlines.insert(0, start.clone());
            }
            p.inlines = inlines;
            out.push(Block::Para(p));
        }
        if out.is_empty() {
            let ppr = format!("<{w}pPr/>");
            let mut p = self.new_para(&ppr, &[(none_found(figures).to_string(), None, true)], container, like);
            p.inlines.insert(0, start);
            out.push(Block::Para(p));
        }
        out
    }

    /// Writes the layout's page numbers into the `PAGEREF` results of the table starting at `first` in `container`
    /// (lays the document out first).
    fn fill_toc_pages(&mut self, container: Container, first: usize, fonts: &Fonts) {
        let Some(t) = self.tables_of_contents().into_iter().find(|t| t.container == container && t.first == first) else { return };
        self.fill_pages(std::slice::from_ref(&t), fonts);
    }

    /// Writes the layout's page numbers into the `PAGEREF` results of `tables` (lays the document out first), in one
    /// transaction.
    fn fill_pages(&mut self, tables: &[TocField], fonts: &Fonts) {
        self.complete(fonts);
        let pages = self.layout.bookmark_pages(&self.doc);
        let mut want: Vec<(FieldId, String)> = Vec::new();
        for t in tables {
            let Some(blocks) = self.index.blocks(&self.doc, t.container) else { continue };
            for b in blocks.get(t.first..=t.last.min(blocks.len().saturating_sub(1))).unwrap_or_default() {
                let Block::Para(p) = b else { continue };
                for s in fields::spans(&p.inlines) {
                    let Inline::FieldBegin { id, kind: Some(FieldKind::PageRef(name)), .. } = &p.inlines[s.begin] else { continue };
                    let code = fields::code_text(&p.inlines, s);
                    let page = crate::layout::text::PageRef::shown(pages.get(name), &code);
                    if fields::result_text(&p.inlines, s) != page {
                        want.push((*id, page));
                    }
                }
            }
        }
        if want.is_empty() {
            return;
        }
        let mut tx = self.begin("Update Table");
        for (f, page) in &want {
            self.set_result(&mut tx, *f, &Body::Text(page.clone()));
        }
        self.commit(tx, None, fonts);
    }

    /// Marks a table of contents entry at the selection (Word's Mark Entry): a hidden `TC` field after it.
    pub fn mark_toc_entry(&mut self, m: &TocMark, fonts: &Fonts) -> Outcome {
        if m.text.trim().is_empty() {
            return Outcome::invalid("an entry has text");
        }
        if !(1..=9).contains(&m.level) {
            return Outcome::invalid("the level is 1…9");
        }
        if let Some((_, b)) = self.selection.ordered(&self.index)
            && !self.selection.is_collapsed()
        {
            self.selection = Selection::Caret(b);
        }
        let code = m.code();
        let cx = self.cx();
        let res = self.doc.resolver.clone();
        // Word marks entries as hidden text: the run at the caret, hidden.
        let base = self.selection.focus().and_then(|c| c.para_offset()).and_then(|(para, at)| self.run_base(para, at, res.as_ref()?, &cx));
        self.insert_inlines(
            |run| {
                let hidden = match (&res, &base) {
                    (Some(r), Some(b)) => {
                        let w = &cx.w;
                        let rpr = run.direct.0.as_deref().map(strip_rpr).unwrap_or_default();
                        r.run_from_xml(b, Some(&format!("<{w}rPr>{rpr}<{w}vanish/></{w}rPr>")), &cx)
                    }
                    _ => run.clone(),
                };
                fields::field_inlines(&code, "", &hidden)
            },
            "Mark Table of Contents Entry",
            fonts,
        )
    }
}

/// The paragraph of block `k` of `container` (the first of a table there).
fn blocks_para_at(e: &Editor, container: Container, k: usize) -> Option<ParaId> {
    let mut b = e.index.blocks(&e.doc, container)?.get(k)?;
    loop {
        match b {
            Block::Para(p) => return Some(ParaId(p.id)),
            Block::Table(t) => b = t.rows.first()?.cells.first()?.blocks.first()?,
        }
    }
}

/// A field edge a deletion reaches: kept (its partner is outside the range) or gone with its field.
pub(super) struct CutEdge {
    edge: Rc<crate::docx::Opaque>,
    keep: bool,
    /// The paragraph it was in.
    para: ParaId,
}

impl Editor {
    /// The field edges in the selection `a..b` (ends included) about to be deleted: each kept when its partner is
    /// outside it, so a deletion never leaves half a field. Only when the range holds one does the container's pairing
    /// get read.
    pub(super) fn edges_in(&self, a: Position, b: Position) -> Vec<CutEdge> {
        let (Some((pa, _)), Some((pb, _))) = (a.para_offset(), b.para_offset()) else { return Vec::new() };
        let (Some(sa), Some(sb)) = (self.index.para_slot(pa), self.index.para_slot(pb)) else { return Vec::new() };
        if sa.container != sb.container {
            return Vec::new();
        }
        let Some(blocks) = self.index.blocks(&self.doc, sa.container) else { return Vec::new() };
        let range = sa.block.min(sb.block)..=sa.block.max(sb.block);
        let within = |pos: &Position| self.index.cmp(&a, pos).is_le() && self.index.cmp(pos, &b).is_le();
        // Edges inside the range, by (block, inline).
        let mut inside: Vec<(usize, usize)> = Vec::new();
        for blk in range {
            let Block::Para(p) = &blocks[blk] else { continue };
            if !p.inlines.iter().any(|i| edge(i).is_some()) {
                continue;
            }
            let mut u = 0u32;
            for (k, i) in p.inlines.iter().enumerate() {
                if edge(i).is_some() && within(&Position::text(a.story, ParaId(p.id), Unit(u))) {
                    inside.push((blk, k));
                }
                u += i.units() as u32;
            }
        }
        let mut out = Vec::new();
        for &place in &inside {
            let Some(partner) = partner(blocks, place) else { continue };
            let Block::Para(p) = &blocks[place.0] else { continue };
            let Inline::Opaque(o) = &p.inlines[place.1] else { continue };
            out.push(CutEdge { edge: o.clone(), keep: !inside.contains(&partner), para: ParaId(p.id) });
        }
        // Ends before starts: an earlier field's end comes before a later one's start where both are kept.
        out.sort_by_key(|c| matches!(c.edge.field, Some(FieldEdge::Start { .. })));
        out
    }

    /// After a tracked deletion (leaving the caret at `at`): the edges of the fields it took whole are deleted with
    /// them; the others stay as they are.
    pub(super) fn delete_whole_fields(&mut self, tx: &mut super::Tx, cut: Vec<CutEdge>, at: Option<Position>) -> Result<(), super::step::StepError> {
        let caret = at.and_then(|p| p.para_offset()).map(|(p, _)| p);
        for c in cut.into_iter().filter(|c| !c.keep) {
            let holds = |e: &Editor, p: ParaId| {
                e.index.para(&e.doc, p).and_then(|q| q.inlines.iter().position(|i| matches!(i, Inline::Opaque(x) if Rc::ptr_eq(x, &c.edge))))
            };
            let Some((para, k)) = std::iter::once(c.para).chain(caret).find_map(|p| Some((p, holds(self, p)?))) else { continue };
            let Some(rec) = tx.rec.as_mut() else { continue };
            let mut ids = std::mem::take(&mut self.doc.review_ids);
            let now = rec.delete_edge(&c.edge, &mut ids);
            self.doc.review_ids = ids;
            let Some(p) = self.index.para(&self.doc, para) else { continue };
            let mut inlines = p.inlines.clone();
            match now {
                Some(i) => inlines[k] = i,
                None => {
                    inlines.remove(k);
                }
            }
            let tracked = std::mem::replace(&mut tx.tracked, false);
            let out = self.step(tx, Step::Restyle { para, inlines });
            tx.tracked = tracked;
            out?;
        }
        Ok(())
    }

    /// After a deletion leaving the caret at `at`: the edges it took that are kept go back at the caret, and those of
    /// fields it took whole go if they stayed.
    pub(super) fn settle_edges(&mut self, tx: &mut super::Tx, cut: Vec<CutEdge>, at: Option<Position>) -> Result<(), super::step::StepError> {
        if cut.is_empty() {
            return Ok(());
        }
        let Some((para, u)) = at.and_then(|p| p.para_offset()) else { return Ok(()) };
        let Some(slot) = self.index.para_slot(para) else { return Ok(()) };
        // Where each still is: the caret's paragraph and its neighbours.
        let near: Vec<ParaId> = self
            .index
            .blocks(&self.doc, slot.container)
            .map(|bl| {
                bl[slot.block.saturating_sub(1)..(slot.block + 2).min(bl.len())]
                    .iter()
                    .filter_map(|b| if let Block::Para(p) = b { Some(ParaId(p.id)) } else { None })
                    .collect()
            })
            .unwrap_or_default();
        let found = |e: &Editor, o: &Rc<crate::docx::Opaque>| -> Option<ParaId> {
            near.iter()
                .copied()
                .find(|p| e.index.para(&e.doc, *p).is_some_and(|q| q.inlines.iter().any(|i| matches!(i, Inline::Opaque(x) if Rc::ptr_eq(x, o)))))
        };
        let mut back = Vec::new();
        for c in cut {
            match (c.keep, found(self, &c.edge)) {
                (true, None) => back.push(Inline::Opaque(c.edge)),
                (false, Some(p)) => {
                    let Some(q) = self.index.para(&self.doc, p) else { continue };
                    let inlines: Vec<Inline> =
                        q.inlines.iter().filter(|i| !matches!(i, Inline::Opaque(x) if Rc::ptr_eq(x, &c.edge))).cloned().collect();
                    self.step(tx, Step::Restyle { para: p, inlines })?;
                }
                _ => {}
            }
        }
        if !back.is_empty() {
            self.step(tx, Step::Text { para, range: u..u, with: back, edges: Default::default() })?;
        }
        Ok(())
    }
}

/// A run's `w:rPr` content without its style, revision marks and the `w:rPr` element itself: the direct formatting
/// an entry keeps.
fn strip_rpr(rpr: &str) -> String {
    let inner = rpr.find('>').map_or("", |k| &rpr[k + 1..]);
    let inner = inner.rfind("</").map_or(inner, |k| &inner[..k]);
    let mut out = String::new();
    let mut rest = inner;
    // Element by element: those that aren't formatting go.
    while let Some(open) = rest.find('<') {
        let tail = &rest[open..];
        let name_end = tail[1..].find(|c: char| c.is_whitespace() || c == '/' || c == '>').map_or(tail.len(), |k| k + 1);
        let name = &tail[1..name_end];
        let close = format!("</{name}>");
        let end = match tail.find("/>") {
            Some(k) if !tail[..k].contains('>') => k + 2,
            _ => tail.find(&close).map_or(tail.len(), |k| k + close.len()),
        };
        let local = name.rsplit(':').next().unwrap_or(name);
        if !matches!(local, "rStyle" | "ins" | "del" | "rPrChange" | "moveFrom" | "moveTo" | "vanish" | "webHidden") {
            out.push_str(&tail[..end]);
        }
        rest = &tail[end..];
    }
    out
}
