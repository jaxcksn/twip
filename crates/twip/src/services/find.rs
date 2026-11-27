//! Find and replace over every story's paragraphs (the body's first, tables included) in document order, on the
//! text the markup view shows: text it hides, and field codes, match nothing. A match starts in one paragraph and,
//! across `^p` or `^b`, may end in a later one of the same story or cell.

use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::api::command::Command;
use crate::api::error::Error;
use crate::api::services::{FindMatch, FindQuery, FontDialog, ParagraphDialog};
use crate::api::state::Tri;
use crate::api::value::{HighlightColor, MatchId, NoneOr, StyleId};
use crate::docx::review::CommentId;
use crate::docx::{Inline, Paragraph, RunStyle};
use crate::editing::Editor;
use crate::editing::commands::Outcome;
use crate::editing::find::Replacement;
use crate::editing::index::Container;
use crate::editing::position::Position;
use crate::editing::search::{self, Clusters, Hay, MatchOptions, Needle, Needs, Pattern, Piece, SearchError, Sep, Template, Token};
use crate::font::Fonts;
use crate::format::StyleKind;
use crate::layout::{ParaId, Unit};

use super::spelling::searched_paras;

/// A match: from units `start` of paragraph `para` to units `end` of paragraph `last` (`para` itself unless the
/// match crosses a paragraph mark or section break).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Hit {
    /// The paragraph it starts in.
    pub para: ParaId,
    /// Its first unit there.
    pub start: u32,
    /// The unit it ends before in `last`.
    pub end: u32,
    /// The paragraph it ends in.
    pub last: ParaId,
}

impl Hit {
    /// A match within one paragraph.
    pub fn within(para: ParaId, start: u32, end: u32) -> Hit {
        Hit { para, start, end, last: para }
    }

    /// The match's public id: `paragraph:start:end`, or `paragraph:start:last:end` across paragraphs.
    pub fn id(&self) -> MatchId {
        if self.last == self.para {
            MatchId(format!("{}:{}:{}", self.para.0, self.start, self.end))
        } else {
            MatchId(format!("{}:{}:{}:{}", self.para.0, self.start, self.last.0, self.end))
        }
    }

    /// The match an id names, as [`Hit::id`] writes them.
    pub fn parse(id: &MatchId) -> Option<Hit> {
        let parts: Vec<u64> = id.0.split(':').map(str::parse::<u64>).collect::<Result<_, _>>().ok()?;
        let unit = |v: u64| u32::try_from(v).ok();
        match parts[..] {
            [p, s, e] => {
                let (start, end) = (unit(s)?, unit(e)?);
                (start < end).then_some(Hit::within(ParaId(p), start, end))
            }
            [p, s, l, e] if p != l => Some(Hit { para: ParaId(p), start: unit(s)?, last: ParaId(l), end: unit(e)? }),
            _ => None,
        }
    }

    /// Selects the match (and reveals it).
    pub fn select(&self, e: &mut Editor, fonts: &Fonts) -> Outcome {
        if self.last == self.para {
            return e.select_match(self.para, Unit(self.start), Unit(self.end), fonts);
        }
        e.select_span(self.para, Unit(self.start), self.last, Unit(self.end), fonts)
    }

    /// How many units the match covers, its paragraph marks between aside.
    pub fn units(&self, e: &Editor) -> usize {
        if self.last == self.para {
            return self.end.saturating_sub(self.start) as usize;
        }
        self.pieces(e).iter().map(|&(_, a, b)| b.0.saturating_sub(a.0) as usize).sum()
    }

    /// The paragraph pieces the match covers, as unit ranges, in order.
    pub fn pieces(&self, e: &Editor) -> Vec<(ParaId, Unit, Unit)> {
        if self.last == self.para {
            return vec![(self.para, Unit(self.start), Unit(self.end))];
        }
        let mut out = Vec::new();
        self.each_piece(e, search::MAX_TEXT + 1, |p, a, b| {
            out.push((p, a, b));
            true
        });
        out
    }

    /// [`Hit::pieces`], up to `most` of them, each given to `f` until it says no: whether every piece was, `last`'s
    /// among them.
    fn each_piece(&self, e: &Editor, most: usize, mut f: impl FnMut(ParaId, Unit, Unit) -> bool) -> bool {
        let mut p = self.para;
        for _ in 0..most {
            let a = if p == self.para { Unit(self.start) } else { Unit(0) };
            if p == self.last {
                return f(p, a, Unit(self.end));
            }
            let len = e.index().para(e.document(), p).map_or(Unit(0), crate::editing::step::para_len);
            if !f(p, a, len) {
                return false;
            }
            let Some((n, _)) = next_para(e, p) else { return false };
            p = n;
        }
        false
    }
}

/// The paragraph after `para` in its story or cell, and what separates them: its mark, or (from a section's last
/// paragraph) the section break. None past a table or the story's end.
pub fn next_para(e: &Editor, para: ParaId) -> Option<(ParaId, Sep)> {
    use crate::docx::Block;
    let slot = e.index().para_slot(para)?;
    let blocks = e.index().blocks(e.document(), slot.container)?;
    if let Some(b) = blocks.get(slot.block + 1) {
        return match b {
            Block::Para(p) => Some((ParaId(p.id), Sep::Paragraph)),
            Block::Table(_) => None,
        };
    }
    let Container::Section(s) = slot.container else { return None };
    match e.document().sections.get(s + 1)?.blocks.first()? {
        Block::Para(p) => Some((ParaId(p.id), Sep::Section)),
        Block::Table(_) => None,
    }
}

/// [`next_para`] across a mark the markup view shows: one it hides (a deleted mark where deletions don't show)
/// joins the two paragraphs as they read, so no match crosses it.
fn shown_next(e: &Editor, para: ParaId) -> Option<(ParaId, Sep)> {
    let p = e.index().para(e.document(), para)?;
    if e.document().review.hides(&p.mark.rev) {
        return None;
    }
    next_para(e, para)
}

/// The formatting a search looks for (each value given must be the text's) and a replacement applies. Values left
/// unavailable (or `None`) are any.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FindFormat {
    /// The character formatting, as the Font dialog has it.
    pub font: FontDialog,
    /// The paragraph formatting, as the Paragraph dialog has it.
    pub paragraph: ParagraphDialog,
    /// A paragraph style the paragraphs have, or a character style the text has.
    pub style: Option<StyleId>,
    /// Highlighted (in any colour), or not. A replacement highlights in `highlight_color`.
    pub highlight: Option<bool>,
    /// The colour a replacement highlights in (Word's highlighter's; yellow when unset).
    pub highlight_color: Option<HighlightColor>,
    /// The language (a BCP 47 tag), matched ignoring case.
    pub language: Option<String>,
    /// Spelling and grammar not checked.
    pub no_proof: Option<bool>,
    /// Tab stops the paragraphs have (each of these, among others); a replacement adds them.
    pub tabs: Vec<crate::api::command::AddTabStop>,
    /// The frame the paragraphs are in, as set; a replacement frames them so.
    pub frame: Option<crate::api::command::FrameProperties>,
}

/// Whether `v` is what `q` asks for, when it asks.
fn tri<T: PartialEq>(q: &Tri<T>, v: &Tri<T>) -> bool {
    match q {
        Tri::Uniform(x) => matches!(v, Tri::Uniform(y) if y == x),
        _ => true,
    }
}

impl FindFormat {
    /// The engine's form of an API [`crate::api::services::FindFormat`].
    pub fn of_api(f: &crate::api::services::FindFormat) -> FindFormat {
        FindFormat {
            font: f.font.as_ref().map(|v| v.to_tri()).unwrap_or_default(),
            paragraph: f.paragraph.as_ref().map(|v| v.to_tri()).unwrap_or_default(),
            style: f.style.clone(),
            highlight: f.highlight,
            highlight_color: f.highlight_color,
            language: f.language.clone(),
            no_proof: f.no_proof,
            tabs: f.tabs.clone(),
            frame: f.frame.clone(),
        }
    }

    /// Nothing asked for.
    pub fn is_empty(&self) -> bool {
        *self == FindFormat::default()
    }

    fn style_kind(&self, e: &Editor) -> Option<StyleKind> {
        let id = self.style.as_ref()?;
        Some(e.document().resolver.as_ref().and_then(|r| r.style_kind(&id.0)).unwrap_or(StyleKind::Paragraph))
    }

    fn asks_characters(&self, e: &Editor) -> bool {
        self.font != FontDialog::default()
            || self.highlight.is_some()
            || self.language.is_some()
            || self.no_proof.is_some()
            || self.style_kind(e).is_some_and(|k| k != StyleKind::Paragraph)
    }

    /// Whether a run formatted as `run` has the character formatting asked for (`kind`: the style's kind).
    fn run_fits(&self, run: &RunStyle, kind: Option<StyleKind>) -> bool {
        let c = crate::editing::state::character(&[run]);
        let f = super::dialogs::font_values(&c);
        let q = &self.font;
        let FontDialog {
            font,
            style,
            size,
            color,
            underline,
            underline_color,
            strikethrough,
            double_strikethrough,
            superscript,
            subscript,
            small_caps,
            all_caps,
            hidden,
            character_spacing,
            position,
            emboss,
            engrave,
            outline,
            shadow,
            scale,
            kerning,
            ligatures,
            number_spacing,
            number_form,
            stylistic_set,
            contextual_alternates,
            text_fill,
            text_outline,
            text_shadow,
            text_reflection,
            text_glow,
        } = q;
        let font_ok = match (font, &f.font) {
            (Tri::Uniform(a), Tri::Uniform(b)) => a.eq_ignore_ascii_case(b),
            (Tri::Uniform(_), _) => false,
            _ => true,
        };
        let style_ok = match (&self.style, kind) {
            (Some(id), Some(k)) if k != StyleKind::Paragraph => c.character_style == Tri::Uniform(NoneOr::Value(id.clone())),
            _ => true,
        };
        let highlighted = matches!(c.highlight, Tri::Uniform(NoneOr::Value(_)));
        let language = match (&self.language, &c.language) {
            (Some(l), Tri::Uniform(v)) => l.eq_ignore_ascii_case(v),
            (Some(_), _) => false,
            _ => true,
        };
        font_ok
            && style_ok
            && language
            && self.highlight.is_none_or(|h| h == highlighted)
            && self.no_proof.is_none_or(|n| c.no_proof == Tri::Uniform(n))
            && tri(style, &f.style)
            && tri(size, &f.size)
            && tri(color, &f.color)
            && tri(underline, &f.underline)
            && tri(underline_color, &f.underline_color)
            && tri(strikethrough, &f.strikethrough)
            && tri(double_strikethrough, &f.double_strikethrough)
            && tri(superscript, &f.superscript)
            && tri(subscript, &f.subscript)
            && tri(small_caps, &f.small_caps)
            && tri(all_caps, &f.all_caps)
            && tri(hidden, &f.hidden)
            && tri(character_spacing, &f.character_spacing)
            && tri(position, &f.position)
            && tri(emboss, &f.emboss)
            && tri(engrave, &f.engrave)
            && tri(outline, &f.outline)
            && tri(shadow, &f.shadow)
            && tri(scale, &f.scale)
            && tri(kerning, &f.kerning)
            && tri(ligatures, &f.ligatures)
            && tri(number_spacing, &f.number_spacing)
            && tri(number_form, &f.number_form)
            && tri(stylistic_set, &f.stylistic_set)
            && tri(contextual_alternates, &f.contextual_alternates)
            && tri(text_fill, &f.text_fill)
            && tri(text_outline, &f.text_outline)
            && tri(text_shadow, &f.text_shadow)
            && tri(text_reflection, &f.text_reflection)
            && tri(text_glow, &f.text_glow)
    }

    /// Whether paragraph `p` has the paragraph formatting asked for (`kind`: the style's kind).
    fn para_fits(&self, p: &Paragraph, kind: Option<StyleKind>) -> bool {
        let pf = crate::editing::state::paragraph(&[(&p.style, None)]);
        let style_ok = match (&self.style, kind) {
            (Some(id), Some(StyleKind::Paragraph)) => pf.style == Tri::Uniform(id.clone()),
            _ => true,
        };
        if !style_ok {
            return false;
        }
        if self.frame.as_ref().is_some_and(|f| crate::editing::find::frame_of(p).as_ref() != Some(f)) {
            return false;
        }
        if !self.tabs.is_empty() {
            use crate::api::value::{TabAlignment, TabLeader};
            let have = crate::editing::find::tab_stops_of(p);
            let has = |t: &crate::api::command::AddTabStop| {
                have.iter().any(|h| {
                    (h.position.0 - t.position.0).abs() < 0.05
                        && h.alignment == t.alignment.unwrap_or(TabAlignment::Left)
                        && h.leader == t.leader.unwrap_or(TabLeader::None)
                })
            };
            if !self.tabs.iter().all(has) {
                return false;
            }
        }
        if self.paragraph == ParagraphDialog::default() {
            return true;
        }
        let v = super::dialogs::paragraph_values(&pf);
        let ParagraphDialog {
            alignment,
            outline_level,
            left,
            right,
            special,
            by,
            before,
            after,
            line_spacing,
            keep_with_next,
            keep_lines_together,
            page_break_before,
            widow_control,
            collapsed,
            mirror_indents,
            contextual_spacing,
            suppress_line_numbers,
            dont_hyphenate,
        } = &self.paragraph;
        tri(alignment, &v.alignment)
            && tri(outline_level, &v.outline_level)
            && tri(left, &v.left)
            && tri(right, &v.right)
            && tri(special, &v.special)
            && tri(by, &v.by)
            && tri(before, &v.before)
            && tri(after, &v.after)
            && tri(line_spacing, &v.line_spacing)
            && tri(keep_with_next, &v.keep_with_next)
            && tri(keep_lines_together, &v.keep_lines_together)
            && tri(page_break_before, &v.page_break_before)
            && tri(widow_control, &v.widow_control)
            && tri(collapsed, &v.collapsed)
            && tri(mirror_indents, &v.mirror_indents)
            && tri(contextual_spacing, &v.contextual_spacing)
            && tri(suppress_line_numbers, &v.suppress_line_numbers)
            && tri(dont_hyphenate, &v.dont_hyphenate)
    }

    /// The commands that give text this formatting, in order (a replacement's formatting): its style, then its
    /// paragraph and character formatting, highlight and language.
    pub fn commands(&self, e: &Editor) -> Result<Vec<Command>, Error> {
        let mut out = Vec::new();
        match (&self.style, self.style_kind(e)) {
            (Some(id), Some(StyleKind::Paragraph)) => out.push(Command::SetParagraphStyle(id.clone())),
            (Some(id), Some(_)) => out.push(Command::ApplyCharacterStyle(Some(id.clone()))),
            _ => {}
        }
        super::dialogs::paragraph_commands(&self.paragraph, &ParagraphDialog::default(), &mut out)?;
        out.extend(self.tabs.iter().cloned().map(Command::AddTabStop));
        out.extend(self.frame.clone().map(Command::SetFrame));
        super::dialogs::font_commands(&self.font, &FontDialog::default(), &mut out)?;
        if let Some(h) = self.highlight {
            out.push(Command::SetHighlight(h.then(|| self.highlight_color.unwrap_or(HighlightColor::Yellow))));
        }
        if self.language.is_some() || self.no_proof.is_some() {
            out.push(Command::SetLanguage(crate::api::command::Language { language: self.language.clone(), no_proof: self.no_proof }));
        }
        Ok(out)
    }
}

/// What a search looks for and how.
#[derive(Clone, Debug, PartialEq)]
pub struct FindSpec {
    /// Find what: text with Word's `^` codes, or a regular expression.
    pub text: String,
    /// How its characters match (all but `case` are passed over by a regular expression, as Word's wildcards
    /// disable them).
    pub options: MatchOptions,
    /// `text` is a regular expression.
    pub regex: bool,
    /// The formatting a match must have; with an empty `text`, each stretch that has it matches.
    pub format: Option<FindFormat>,
    /// Only matches within these paragraph ranges (the selection's, as [`Editor::selected_ranges`] gives them), with
    /// no wrapping.
    pub within: Option<Vec<(ParaId, Unit, Unit)>>,
    /// Find next wraps past the document's end (Word's All); else it stops there (Up and Down).
    pub wrap: bool,
}

impl Default for FindSpec {
    fn default() -> FindSpec {
        FindSpec { text: String::new(), options: MatchOptions::default(), regex: false, format: None, within: None, wrap: true }
    }
}

impl FindSpec {
    /// The spec of a [`FindQuery`], but for its `inSelection` (the caller gives `within` the selection's ranges).
    pub fn of_query(q: &FindQuery) -> FindSpec {
        let on = |v: Option<bool>| v.unwrap_or(false);
        let options = MatchOptions {
            case: on(q.match_case),
            whole: on(q.whole_words),
            prefix: on(q.match_prefix),
            suffix: on(q.match_suffix),
            ignore_punct: on(q.ignore_punctuation),
            ignore_space: on(q.ignore_white_space),
        };
        FindSpec {
            text: q.text.clone(),
            options,
            regex: on(q.regex),
            format: q.format.as_ref().map(FindFormat::of_api),
            within: None,
            wrap: q.wrap.unwrap_or(true),
        }
    }
}

/// The run each character of a paragraph's text is in, by its units.
fn runs_by_unit(p: &Paragraph) -> Vec<(u32, Option<&crate::docx::Run>)> {
    let mut at = 0u32;
    let mut out = Vec::with_capacity(p.inlines.len());
    for i in &p.inlines {
        let n = i.units() as u32;
        if n > 0 {
            out.push((at, if matches!(i, Inline::FieldBegin { .. }) { None } else { i.run() }));
        }
        at += n;
    }
    out
}

/// What one search works out once: whether each run has the formatting asked for, the comments' spans, and the work
/// it has left.
struct Memo {
    runs: HashMap<*const RunStyle, bool>,
    comments: Option<HashMap<ParaId, Vec<Hit>>>,
    budget: Rc<Cell<usize>>,
}

/// Takes `n` from `cache`'s budget: whether any was left.
fn charged(cache: &Memo, n: usize) -> bool {
    let left = cache.budget.get();
    cache.budget.set(left.saturating_sub(n));
    left > 0
}

/// The paragraphs a match crossing into them read, after the one it starts in: each with its grapheme clusters.
type Later = Vec<(ParaId, Hay, Clusters)>;

/// A compiled search: its spec, parsed.
pub struct Search {
    spec: FindSpec,
    needle: Needle,
    needs: Needs,
    pattern: Option<Pattern>,
    /// Find what has `^12`, which crosses a section break where it meets a paragraph's end.
    flex: bool,
    /// `spec.within`'s ranges by paragraph.
    ranges: HashMap<ParaId, Vec<(Unit, Unit)>>,
    truncated: Cell<bool>,
}

impl Search {
    /// What it was made from.
    pub fn spec(&self) -> &FindSpec {
        &self.spec
    }

    /// `spec` parsed (and its regular expression compiled), or why it can't be searched.
    pub fn new(spec: FindSpec) -> Result<Search, SearchError> {
        let (needle, pattern) = if spec.regex {
            (Needle::default(), (!spec.text.is_empty()).then(|| Pattern::new(&spec.text, spec.options.case)).transpose()?)
        } else {
            (spec.options.strip(search::parse(&spec.text)?), None)
        };
        let needs = Needs::of(&needle);
        let flex = needle.has(|t| t == Token::PageOrSection);
        let mut ranges: HashMap<ParaId, Vec<(Unit, Unit)>> = HashMap::new();
        for &(p, a, b) in spec.within.iter().flatten() {
            ranges.entry(p).or_default().push((a, b));
        }
        Ok(Search { spec, needle, needs, pattern, flex, ranges, truncated: Cell::new(false) })
    }

    /// Whether the last pass over the document (a find, all matches, the match at the selection or replacements)
    /// gave up matches past its budget ([`search::MATCH_BUDGET`], [`search::REGEX_BUDGET`]).
    pub fn truncated(&self) -> bool {
        self.truncated.get()
    }

    /// A pass's memo, with a full budget.
    fn memo(&self) -> Memo {
        let budget = if self.pattern.is_some() { search::REGEX_BUDGET } else { search::MATCH_BUDGET };
        Memo { runs: HashMap::new(), comments: None, budget: Rc::new(Cell::new(budget)) }
    }

    /// Ends a pass: whether it spent its budget.
    fn done(&self, cache: &Memo) {
        self.truncated.set(cache.budget.get() == 0);
    }

    /// A [`FindQuery`]'s search; one whose text isn't valid finds nothing.
    pub fn of_query(q: &FindQuery) -> Search {
        Search::new(FindSpec::of_query(q)).unwrap_or_else(|_| Search {
            spec: FindSpec::default(),
            needle: Needle::default(),
            needs: Needs::default(),
            pattern: None,
            flex: false,
            ranges: HashMap::new(),
            truncated: Cell::new(false),
        })
    }

    fn format(&self) -> Option<&FindFormat> {
        self.spec.format.as_ref().filter(|f| !f.is_empty())
    }

    /// Find what is a comment's mark alone (`^a`): each match is a comment's span, the commented text.
    pub fn comment_marks_only(&self) -> bool {
        self.pattern.is_none() && self.needle.seps.is_empty() && self.needle.parts[..] == [vec![search::Token::Comment]]
    }

    /// Each comment's span in the stories searched, from its range's start to its reference, and its id; none for a
    /// comment with no range start before its reference.
    pub fn comment_spans(e: &Editor) -> Vec<(Hit, CommentId)> {
        let mut starts: HashMap<i64, (ParaId, u32)> = HashMap::new();
        let mut out = Vec::new();
        for para in searched_paras(e) {
            let Some(p) = e.index().para(e.document(), para) else { continue };
            let mut u = 0u32;
            for i in &p.inlines {
                match i {
                    Inline::Range(m) if m.kind == crate::docx::review::RangeKind::CommentStart => {
                        starts.entry(m.id).or_insert((para, u));
                    }
                    Inline::CommentRef { id, .. } => {
                        if let Some(&(sp, su)) = starts.get(&id.0)
                            && (sp != para || su < u)
                        {
                            out.push((Hit { para: sp, start: su, end: u, last: para }, *id));
                        }
                    }
                    _ => {}
                }
                u += i.units() as u32;
            }
        }
        out
    }

    /// Nothing to look for.
    pub fn is_empty(&self) -> bool {
        self.pattern.is_none() && self.needle.is_empty() && self.format().is_none()
    }

    /// Paragraph `para`'s text as the search reads it.
    fn hay(&self, e: &Editor, para: ParaId, cache: &mut Memo) -> Option<Hay> {
        let p = e.index().para(e.document(), para)?;
        let mut hay = Hay::new(e.find_text(para), p, self.needs, cache.budget.clone());
        if let Some(f) = self.format() {
            let kind = f.style_kind(e);
            let n = hay.chars().len();
            hay.para_ok = f.para_fits(p, kind);
            hay.fmt = Some(if !hay.para_ok {
                vec![false; n]
            } else if !f.asks_characters(e) {
                vec![true; n]
            } else {
                let runs = runs_by_unit(p);
                hay.seen
                    .units
                    .iter()
                    .map(|&u| {
                        let k = runs.partition_point(|r| r.0 <= u).saturating_sub(1);
                        runs.get(k).and_then(|r| r.1).is_some_and(|r| *cache.runs.entry(Rc::as_ptr(r)).or_insert_with(|| f.run_fits(r, kind)))
                    })
                    .collect()
            });
        }
        Some(hay)
    }

    /// The matches within one paragraph's text, as character ranges (widened to whole grapheme clusters) with a
    /// regular expression's replacement; overlapping each other when `overlap`.
    fn single(&self, hay: &Hay, with: Option<&Template>, overlap: bool) -> Vec<(usize, usize, Vec<Piece>)> {
        let chars = hay.chars();
        let n = chars.len();
        let o = &self.spec.options;
        let mut found: Vec<(usize, usize, Vec<Piece>)> = Vec::new();
        let c = std::cell::OnceCell::new();
        let fits = |i: usize, end: usize, edges: bool| -> bool {
            let c: &Clusters = c.get_or_init(|| Clusters::of(&hay.seen.boundaries()));
            let at = |k: usize| c.starts(k) || !chars[c.start(k)].is_alphanumeric();
            at(i)
                && at(end)
                && !hay.seen.crosses(c.start(i), c.next(end))
                && hay.formatted(c.start(i), c.next(end))
                && (!edges || search::edges(chars, c, i, end, o.whole || o.prefix, o.whole || o.suffix))
        };
        if let Some(p) = &self.pattern {
            found = p.matches(hay, 0, with, overlap, |a, b| fits(a, b, false));
        } else if self.needle.is_empty() {
            // Formatting alone: each stretch of the text that has it, between field markers.
            let mut k = 0;
            while k < n {
                if !hay.plain_run(k) {
                    k += 1;
                    continue;
                }
                let start = k;
                while k < n && hay.plain_run(k) && !(k > start && hay.seen.breaks.binary_search(&k).is_ok()) {
                    k += 1;
                }
                found.push((start, k, Vec::new()));
            }
            return found;
        } else {
            let toks = &self.needle.parts[0];
            if toks.is_empty() {
                return found;
            }
            let lead_white = toks[0] == search::Token::White;
            let mut failed: Option<usize> = None;
            let mut i = 0;
            while i < n && !hay.spent() {
                // A run of white space matched greedily: a start inside it ends where one before it did.
                if lead_white && !overlap && i > 0 && failed == Some(i - 1) && hay.white_at(i - 1) && hay.white_at(i) {
                    failed = Some(i);
                    i += 1;
                    continue;
                }
                match hay.match_at(toks, i, o) {
                    Some(end) if end > i && fits(i, end, true) => {
                        found.push((i, end, Vec::new()));
                        i = if overlap { i + 1 } else { end };
                        continue;
                    }
                    None => failed = Some(i),
                    _ => {}
                }
                i += 1;
            }
        }
        let Some(c) = c.get().filter(|_| !found.is_empty()) else { return found };
        let mut out: Vec<(usize, usize, Vec<Piece>)> = Vec::with_capacity(found.len());
        let mut last = 0;
        for (i, end, r) in found {
            let (a, b) = (c.start(i), c.next(end));
            if a < last && !overlap {
                continue;
            }
            last = b;
            out.push((a, b, r));
        }
        out
    }

    /// The match across paragraph marks that starts in `para` at or after character `from`, if any; with `overlap`,
    /// one from each later start too.
    fn across(&self, e: &Editor, para: ParaId, first: &Hay, from: usize, overlap: bool, cache: &mut Memo) -> Vec<Hit> {
        let o = &self.spec.options;
        let parts = &self.needle.parts;
        let n0 = first.chars().len();
        let c0 = Clusters::of(&first.seen.boundaries());
        // The first part runs to the paragraph's end: its leftmost start from `i` that takes the word edges asked for.
        let start_from = |mut i: usize| loop {
            let s = first.match_to_end(&parts[0], i, o)?;
            let ok = s == n0
                || ((c0.starts(s) || !first.chars()[c0.start(s)].is_alphanumeric())
                    && !first.seen.crosses(c0.start(s), n0)
                    && first.formatted(c0.start(s), s)
                    && search::edges(first.chars(), &c0, s, n0, o.whole || o.prefix, false));
            if ok && (s < n0 || first.para_ok) {
                return Some(s);
            }
            if s >= n0 {
                return None;
            }
            i = s + 1;
        };
        let Some(s0) = start_from(from) else { return Vec::new() };
        let Some((last, end)) = self.across_rest(e, para, cache) else { return Vec::new() };
        let len_first = e.index().para(e.document(), para).map_or(0, |q| crate::editing::step::para_len(q).0);
        let hit = |s: usize| {
            let start = c0.start(s.min(n0));
            Hit { para, start: if start >= n0 { len_first } else { first.units_of(start, n0).0.0 }, end, last }
        };
        let mut out = vec![hit(s0)];
        // Each later start takes the same rest.
        let mut s = s0;
        while overlap && s < n0 {
            let Some(next) = start_from(s + 1) else { break };
            let h = hit(next);
            if out.last().is_none_or(|l| l.start != h.start) {
                out.push(h);
            }
            s = next;
        }
        out
    }

    /// Where a match across paragraph marks from `para`'s end ends: the paragraph and unit, if its parts after the
    /// first match there.
    fn across_rest(&self, e: &Editor, para: ParaId, cache: &mut Memo) -> Option<(ParaId, u32)> {
        let o = &self.spec.options;
        let parts = &self.needle.parts;
        let mut p = para;
        let mut hay;
        for (k, sep) in self.needle.seps.iter().enumerate() {
            let (next, kind) = shown_next(e, p)?;
            if kind != *sep {
                return None;
            }
            p = next;
            hay = self.hay(e, p, cache)?;
            let toks = &parts[k + 1];
            let len = hay.chars().len();
            if k + 1 < parts.len() - 1 {
                // A middle part is its paragraph's whole text.
                let whole = if toks.is_empty() { len == 0 } else { hay.match_at(toks, 0, o) == Some(len) && !hay.seen.crosses(0, len) };
                if !whole || !hay.para_ok {
                    return None;
                }
                continue;
            }
            let end = if toks.is_empty() { 0 } else { hay.match_at(toks, 0, o)? };
            let c = Clusters::of(&hay.seen.boundaries());
            if end > 0 {
                let fits = (c.starts(end) || !hay.chars()[c.start(end)].is_alphanumeric())
                    && !hay.seen.crosses(0, c.next(end))
                    && hay.formatted(end, c.next(end))
                    && search::edges(hay.chars(), &c, 0, end, false, o.whole || o.suffix);
                if !fits {
                    return None;
                }
            }
            let t = if end == 0 { 0 } else { hay.units_of(0, c.next(end)).1.0 };
            return Some((p, t));
        }
        None
    }

    /// The match of a Find what with `^12` from character `i` of paragraph `para` (read as `first`, its clusters
    /// `c0`), if any. `^12` matches a page break, or at a paragraph's end the section break after it; `^p` and `^b`
    /// cross as they do. `later`: the paragraphs after it read so far, kept for the next start.
    #[allow(clippy::too_many_arguments)]
    fn flex_at(&self, e: &Editor, para: ParaId, first: &Hay, c0: &Clusters, i: usize, later: &mut Later, cache: &mut Memo) -> Option<Hit> {
        let o = &self.spec.options;
        let n0 = first.chars().len();
        let starts = |k: usize| k == n0 || c0.starts(k) || !first.chars()[c0.start(k)].is_alphanumeric();
        if !starts(i) || (i < n0 && !search::edges(first.chars(), c0, i, i, o.whole || o.prefix, false)) {
            return None;
        }
        let i0 = if i < n0 { c0.start(i) } else { i };
        let (mut cur, mut p, mut k, mut from) = (None::<usize>, para, i, i0);
        // Into the next paragraph across `sep`, when the text read so far ends there and crosses no field marker.
        let cross = |later: &mut Later, cur: &mut Option<usize>, p: &mut ParaId, k: &mut usize, from: &mut usize, sep: Sep, cache: &mut Memo| {
            let hay = cur.map_or(first, |x| &later[x].1);
            if *k != hay.chars().len() || !hay.para_ok || hay.seen.crosses(*from, *k) {
                return None;
            }
            let (n, kind) = shown_next(e, *p)?;
            if kind != sep {
                return None;
            }
            let x = match later.iter().position(|l| l.0 == n) {
                Some(x) => x,
                None => {
                    let hay = self.hay(e, n, cache)?;
                    let c = Clusters::of(&hay.seen.boundaries());
                    later.push((n, hay, c));
                    later.len() - 1
                }
            };
            (*cur, *p, *k, *from) = (Some(x), n, 0, 0);
            Some(())
        };
        for (part, toks) in self.needle.parts.iter().enumerate() {
            let mut in_para = 0;
            for &t in toks {
                let hay = cur.map_or(first, |x| &later[x].1);
                if hay.spent() {
                    return None;
                }
                let len = hay.chars().len();
                let stepped = hay.step(t, k, o, in_para > 0);
                hay.charge(stepped.unwrap_or_else(|at| at).saturating_sub(k) + 1);
                match stepped {
                    Ok(end) => {
                        k = end;
                        in_para += 1;
                    }
                    Err(at) if t == Token::PageOrSection && at == len => {
                        k = at;
                        cross(later, &mut cur, &mut p, &mut k, &mut from, Sep::Section, cache)?;
                        in_para = 0;
                    }
                    Err(_) => return None,
                }
            }
            if let Some(&sep) = self.needle.seps.get(part) {
                cross(later, &mut cur, &mut p, &mut k, &mut from, sep, cache)?;
            }
        }
        let (last, c) = cur.map_or((first, c0), |x| (&later[x].1, &later[x].2));
        let len = last.chars().len();
        let ends = k == len || c.starts(k) || !last.chars()[c.start(k)].is_alphanumeric();
        if (p == para && k == i) || !ends || last.seen.crosses(from, c.next(k.min(len))) {
            return None;
        }
        if !first.formatted(i0, i.min(n0)) || !last.formatted(k.min(len), c.next(k.min(len))) {
            return None;
        }
        if k < len && !search::edges(last.chars(), c, from, k, false, o.whole || o.suffix) {
            return None;
        }
        if p == para {
            let (s, t) = first.units_of(i0, c.next(k));
            return Some(Hit::within(para, s.0, t.0));
        }
        let s =
            if i >= n0 { e.index().para(e.document(), para).map_or(0, |q| crate::editing::step::para_len(q).0) } else { first.units_of(i0, n0).0.0 };
        let t = if k == 0 { 0 } else { last.units_of(0, c.next(k)).1.0 };
        Some(Hit { para, start: s, end: t, last: p })
    }

    /// The matches of a Find what with `^12` starting in paragraph `para` (read as `hay`) from character `k`.
    fn flex_hits(&self, e: &Editor, para: ParaId, hay: &Hay, mut k: usize, overlap: bool, cache: &mut Memo) -> Vec<Hit> {
        let mut out = Vec::new();
        let n = hay.chars().len();
        let c = Clusters::of(&hay.seen.boundaries());
        let mut later = Later::new();
        while k <= n && !hay.spent() {
            match self.flex_at(e, para, hay, &c, k, &mut later, cache) {
                Some(h) => {
                    out.push(h);
                    if overlap {
                        k += 1;
                        continue;
                    }
                    if h.last != para {
                        break;
                    }
                    k = hay.seen.units.partition_point(|&u| u < h.end).max(k + 1);
                }
                None => k += 1,
            }
        }
        out
    }

    /// The matches starting in paragraph `para` (from unit `from`), in order; overlapping each other when `overlap`.
    fn para_hits(&self, e: &Editor, para: ParaId, from: u32, overlap: bool, cache: &mut Memo) -> Vec<Hit> {
        if self.is_empty() {
            return Vec::new();
        }
        if self.comment_marks_only() {
            let spans = cache.comments.get_or_insert_with(|| {
                let mut by: HashMap<ParaId, Vec<Hit>> = HashMap::new();
                for (h, _) in Search::comment_spans(e) {
                    by.entry(h.para).or_default().push(h);
                }
                by
            });
            let hits: Vec<Hit> = spans.get(&para).map_or_else(Vec::new, |v| v.iter().copied().filter(|h| h.start >= from).collect());
            if self.format().is_none() {
                return hits;
            }
            return hits.into_iter().filter(|h| self.formatted(e, h, cache)).collect();
        }
        let Some(hay) = self.hay(e, para, cache) else { return Vec::new() };
        if self.flex {
            let k = hay.seen.units.partition_point(|&u| u < from);
            return self.flex_hits(e, para, &hay, k, overlap, cache).into_iter().filter(|h| h.start >= from).collect();
        }
        if !self.needle.seps.is_empty() {
            let k = hay.seen.units.partition_point(|&u| u < from);
            return self.across(e, para, &hay, k, overlap, cache).into_iter().filter(|h| h.start >= from).collect();
        }
        self.single(&hay, None, overlap)
            .into_iter()
            .map(|(a, b, _)| {
                let (s, t) = hay.units_of(a, b);
                Hit::within(para, s.0, t.0)
            })
            .filter(|h| h.start >= from && h.start < h.end)
            .collect()
    }

    /// Whether `h` is within the ranges the search is limited to. A match across paragraphs (a comment's span) is
    /// walked whole, each paragraph charged to the budget.
    fn inside(&self, e: &Editor, h: &Hit, cache: &Memo) -> bool {
        if self.spec.within.is_none() {
            return true;
        }
        let fits = |p: ParaId, a: Unit, b: Unit| self.ranges.get(&p).is_some_and(|r| r.iter().any(|&(x, y)| x <= a && b <= y));
        h.each_piece(e, usize::MAX, |p, a, b| (h.last == h.para || charged(cache, 1)) && fits(p, a, b))
    }

    /// Whether the text `h` covers, all of it read, has the formatting searched for: a comment's span whole, each
    /// paragraph charged to the budget.
    fn formatted(&self, e: &Editor, h: &Hit, cache: &mut Memo) -> bool {
        h.each_piece(e, usize::MAX, |p, a, b| {
            self.hay(e, p, cache).is_some_and(|hay| charged(cache, hay.chars().len().max(1)) && hay.formatted_units(a, b))
        })
    }

    /// The matches starting in paragraph `para` (from unit `from`) within the ranges searched, in order, none
    /// overlapping another. Limited to ranges, each candidate is tried, so one cut off by a range's start yields to one
    /// starting inside it.
    fn scoped_hits(&self, e: &Editor, para: ParaId, from: u32, cache: &mut Memo) -> Vec<Hit> {
        if self.spec.within.is_none() {
            return self.para_hits(e, para, from, false, cache);
        }
        let mut end = 0u32;
        let mut out = Vec::new();
        for h in self.para_hits(e, para, from, true, cache) {
            if h.start < end || !self.inside(e, &h, cache) {
                continue;
            }
            end = if h.last == para { h.end } else { u32::MAX };
            out.push(h);
        }
        out
    }

    /// The paragraphs searched, in order.
    fn order(&self, e: &Editor) -> Vec<ParaId> {
        let all = searched_paras(e);
        match &self.spec.within {
            Some(_) => all.into_iter().filter(|p| self.ranges.contains_key(p)).collect(),
            None => all,
        }
    }

    /// Every match, in document order, none overlapping another.
    pub fn all(&self, e: &Editor) -> Vec<Hit> {
        let mut cache = self.memo();
        let mut out: Vec<Hit> = Vec::new();
        let order = self.order(e);
        let (mut k, mut from) = (0, 0);
        while k < order.len() {
            let n = out.len();
            out.extend(self.scoped_hits(e, order[k], from, &mut cache));
            from = 0;
            k += 1;
            // Past a match across paragraph marks: on from its end, in the paragraph it ends in.
            if let Some(h) = out[n..].last().filter(|h| h.last != h.para)
                && let Some(j) = order[k..].iter().position(|&p| p == h.last)
            {
                k += j;
                from = h.end;
            }
        }
        self.done(&cache);
        out
    }

    /// The next match after the selection (before it, `back`), wrapping round the document when the spec does, and
    /// whether it wrapped.
    pub fn next(&self, e: &Editor, back: bool) -> Option<(Hit, bool)> {
        let order = self.order(e);
        if order.is_empty() || self.is_empty() {
            return None;
        }
        let (from_para, from) = match self.spec.within.as_deref() {
            // Within the selection: on from the match it sits on, else from the selection's start (its end, back).
            Some(w @ [first, ..]) => match self.at_selection(e) {
                Some(h) if back => (h.para, Unit(h.start)),
                Some(h) => (h.last, Unit(h.end)),
                None if back => w.last().map_or((first.0, first.2), |l| (l.0, l.2)),
                None => (first.0, first.1),
            },
            _ => {
                let (a, b) = match e.selected_match() {
                    Some((para, a, b)) => ((para, a), (para, b)),
                    None => e
                        .selection()
                        .ordered(e.index())
                        .and_then(|(a, b)| Some((a.para_offset()?, b.para_offset()?)))
                        .unwrap_or(((order[0], Unit(0)), (order[0], Unit(0)))),
                };
                if back { a } else { b }
            }
        };
        let mut cache = self.memo();
        let n = order.len();
        // A selection outside the paragraphs searched (a header, a note, a text box): from the top, or the end going back.
        let (start, inside) = match order.iter().position(|p| *p == from_para) {
            Some(k) => (k, true),
            None => (if back { n - 1 } else { 0 }, false),
        };
        let wrap = self.spec.wrap && self.spec.within.is_none();
        // One more than the paragraphs: the start paragraph again, for the matches on the other side of the selection.
        for k in 0..=n {
            let i = if back { (start + n * 2 - k) % n } else { (start + k) % n };
            let wrapped = (k == n && inside) || if back { i > start } else { i < start };
            if wrapped && !wrap {
                self.done(&cache);
                return None;
            }
            let hits = self.scoped_hits(e, order[i], 0, &mut cache);
            let first = k == 0 && inside;
            // Going back, before wrapping: ending at or before where the search starts, across paragraphs too.
            let ends_before = |h: &Hit| {
                if h.last == h.para {
                    return h.para != from_para || h.end <= from.0;
                }
                let story = |p: ParaId| e.index().para_slot(p).map(|s| e.index().story(s.container));
                match (story(h.last), story(from_para)) {
                    (Some(a), Some(b)) if a == b => {
                        e.index().cmp(&Position::text(a, h.last, Unit(h.end)), &Position::text(b, from_para, from)) != std::cmp::Ordering::Greater
                    }
                    _ => true,
                }
            };
            let hit = if back {
                hits.into_iter().rev().find(|h| wrapped || ends_before(h))
            } else {
                hits.into_iter().find(|h| !first || h.start >= from.0)
            };
            if let Some(h) = hit {
                self.done(&cache);
                return Some((h, wrapped));
            }
        }
        self.done(&cache);
        None
    }

    /// The match the selection is on: the find match selected last while it stays selected, else one exactly selected.
    pub fn at_selection(&self, e: &Editor) -> Option<Hit> {
        let mut cache = self.memo();
        let found = self.on_selection(e, &mut cache);
        self.done(&cache);
        found
    }

    fn on_selection(&self, e: &Editor, cache: &mut Memo) -> Option<Hit> {
        if let Some((para, a, b)) = e.selected_match()
            && let Some(h) =
                self.para_hits(e, para, 0, true, cache).into_iter().find(|h| (h.para, h.start, h.end) == (para, a.0, b.0) && self.inside(e, h, cache))
        {
            return Some(h);
        }
        let (x, y) = e.selection().ordered(e.index())?;
        let ((pa, ua), (pb, ub)) = (x.para_offset()?, y.para_offset()?);
        let want = Hit { para: pa, start: ua.0, end: ub.0, last: pb };
        self.para_hits(e, pa, 0, true, cache).into_iter().find(|h| *h == want && self.inside(e, h, cache))
    }

    /// The replacement of each of `hits`: `with` (text with `^` codes, or a regular expression's replacement with
    /// `$n` and `${name}` groups, each group a copy of what it matched) as pieces; one for all in normal mode, one
    /// each (`None` for a hit that is no longer a match) with a regular expression.
    pub fn replacements(&self, e: &Editor, hits: &[Hit], with: &str) -> Result<Vec<Option<Vec<Piece>>>, SearchError> {
        let pieces = search::parse_replacement(with, self.spec.regex)?;
        // Units the pass copies in all (the text found, groups, the clipboard), held to MAX_EXPANSION; pastes past
        // each match's first, held to MAX_PASTES.
        let clip = if pieces.contains(&Piece::Clipboard) { e.clipboard().map_or(0, |f| f.text().chars().count()) } else { 0 };
        let (mut copied, mut pasted) = (0usize, 0usize);
        let mut charge = |h: &Hit, ps: &[Piece]| {
            let mut first = true;
            for p in ps {
                let n = match *p {
                    Piece::Found => h.units(e),
                    Piece::Copy(a, b) => b.0.saturating_sub(a.0) as usize,
                    Piece::Clipboard => clip,
                    _ => continue,
                };
                copied = copied.saturating_add(n);
                if !std::mem::take(&mut first) {
                    pasted += 1;
                }
            }
            if copied > search::MAX_EXPANSION || pasted > search::MAX_PASTES { Err(SearchError::Expansion) } else { Ok(()) }
        };
        let Some(p) = &self.pattern else {
            if pieces.iter().any(|p| matches!(p, Piece::Found | Piece::Clipboard)) {
                hits.iter().try_for_each(|h| charge(h, &pieces))?;
            }
            return Ok(vec![Some(pieces)]);
        };
        let template = p.template(with);
        let mut cache = self.memo();
        let mut out = Vec::with_capacity(hits.len());
        let mut k = 0;
        // Each paragraph's matches once, then its hits looked up among them.
        while k < hits.len() {
            let para = hits[k].para;
            let n = hits[k..].iter().take_while(|h| h.para == para).count();
            let mut by: HashMap<(u32, u32), Vec<Piece>> = HashMap::new();
            if let Some(hay) = self.hay(e, para, &mut cache) {
                // Limited to ranges, a hit may overlap a match out of them.
                for (a, b, pieces) in self.single(&hay, Some(&template), self.spec.within.is_some()) {
                    let (s, t) = hay.units_of(a, b);
                    by.insert((s.0, t.0), pieces);
                }
            }
            for h in &hits[k..k + n] {
                let pieces = by.remove(&(h.start, h.end));
                charge(h, pieces.as_deref().unwrap_or_default())?;
                out.push(pieces);
            }
            k += n;
        }
        self.done(&cache);
        Ok(out)
    }
}

/// Highlights every match of `search` on the pages (Word's Highlight All, [`Editor::set_find_highlights`]): how many.
pub fn highlight_all(e: &mut Editor, search: &Search) -> usize {
    let hits = search.all(e);
    if hits.is_empty() {
        e.clear_find_highlights();
        return 0;
    }
    // Each piece but a match's last runs on across its paragraph's mark.
    let mut pieces = Vec::new();
    for h in &hits {
        let from = pieces.len();
        pieces.extend(h.pieces(e));
        let last = pieces.len() - 1;
        pieces[from..last].iter_mut().for_each(|p: &mut (ParaId, Unit, Unit)| p.2 = Unit(p.2.0 + 1));
    }
    e.set_find_highlights(pieces);
    hits.len()
}

/// Every match, in document order. A field showing its result is searched in its result only.
pub fn all(e: &Editor, q: &FindQuery) -> Vec<Hit> {
    Search::of_query(q).all(e)
}

/// The next match after the selection (before it, `back`), wrapping round the document, and whether it wrapped.
pub fn next(e: &Editor, q: &FindQuery, back: bool) -> Option<(Hit, bool)> {
    Search::of_query(q).next(e, back)
}

/// The match the selection is on: the find match selected last while it stays selected, else one exactly selected.
pub fn at_selection(e: &Editor, q: &FindQuery) -> Option<Hit> {
    Search::of_query(q).at_selection(e)
}

/// A match for a results list: up to 30 characters of context on each side, from its paragraphs.
pub fn describe(e: &Editor, h: Hit, page: u32) -> FindMatch {
    let clean = |s: &[char]| {
        s.iter().map(|&c| if c.is_control() || matches!(c, crate::editing::text::OBJECT | '\u{2028}') { ' ' } else { c }).collect::<String>()
    };
    let first = e.find_text(h.para);
    let last = if h.last == h.para { None } else { Some(e.find_text(h.last)) };
    let s = first.units.partition_point(|&u| u < h.start);
    let end_seen = last.as_ref().unwrap_or(&first);
    let t = end_seen.units.partition_point(|&u| u < h.end);
    let text = match &last {
        None => clean(&first.chars[s..t]),
        Some(l) => {
            let mut v: Vec<char> = first.chars[s..].to_vec();
            for (p, _, _) in h.pieces(e).iter().skip(1).take_while(|piece| piece.0 != h.last) {
                v.push(' ');
                v.extend(e.find_text(*p).chars);
            }
            v.push(' ');
            v.extend(&l.chars[..t]);
            clean(&v)
        }
    };
    FindMatch {
        id: h.id(),
        before: clean(&first.chars[s.saturating_sub(30)..s]),
        text,
        after: clean(&end_seen.chars[t..(t + 30).min(end_seen.chars.len())]),
        page,
        heading: None,
    }
}

/// Replaces `hits` with `with` as one undo step ([`Editor::replace_hits`]): how many. `all`: Replace All, which leaves
/// the caret where it was; else the caret follows the replacement.
pub fn replace(e: &mut Editor, hits: &[Hit], with: &str, all: bool, fonts: &Fonts) -> (usize, Outcome) {
    let hits: Vec<_> = hits.iter().map(|h| (h.para, Unit(h.start), h.last, Unit(h.end))).collect();
    e.replace_hits(if all { "Replace All" } else { "Replace" }, &hits, &[Replacement::text(with)], all, None, fonts)
}

/// Replaces `hits` of `search` with `with` (its `^` codes or groups expanded per match), then gives the replaced text
/// `format`'s formatting, as one undo step; with an empty `with` and a format, only formats them. `all`: Replace All,
/// which leaves the caret where it was (the selection kept round what it covered, with `within`); else the caret
/// follows the replacement. How many it replaced.
pub fn replace_with(
    e: &mut Editor,
    search: &Search,
    hits: &[Hit],
    with: &str,
    format: Option<&FindFormat>,
    all: bool,
    fonts: &Fonts,
) -> Result<(usize, Outcome), Error> {
    let mut pieces = search.replacements(e, hits, with).map_err(|why| Error::invalid(why.to_string()))?;
    let label = if all { "Replace All" } else { "Replace" };
    let format = format.filter(|f| !f.is_empty());
    // A comment's mark with formatting alone: the commented text formatted, its comment kept.
    if search.comment_marks_only() && !(with.is_empty() && format.is_some()) {
        if format.is_some() {
            return Err(Error::invalid("a comment's mark is replaced with text or formatted, not both"));
        }
        // The text found is the commented text, which stays: nothing to paste.
        if let Some(c) = pieces.iter().flatten().flatten().find_map(|p| match p {
            Piece::Found => Some("^&"),
            Piece::Clipboard => Some("^c"),
            _ => None,
        }) {
            return Err(Error::invalid(SearchError::Misplaced(c.into()).to_string()));
        }
        // Replacing a comment's mark deletes the comment; the replacement goes where its reference was.
        // Each span's comments, last first, so equal spans take them in order.
        let mut ids: HashMap<Hit, Vec<CommentId>> = HashMap::new();
        for (h, id) in Search::comment_spans(e).into_iter().rev() {
            ids.entry(h).or_default().push(id);
        }
        let marks: Vec<_> = hits.iter().filter_map(|h| Some((ids.get_mut(h)?.pop()?, h.last, Unit(h.end)))).collect();
        let rep = Replacement { pieces: pieces.into_iter().next().flatten().unwrap_or_default(), keep: false };
        return Ok(e.replace_comment_marks(label, &marks, &rep, all, fonts));
    }
    let commands = format.map(|f| f.commands(e)).transpose()?.unwrap_or_default();
    let keep_text = with.is_empty() && format.is_some();
    // With a regular expression, one replacement each: hits no longer matched are left.
    let hits: Vec<&Hit> = if pieces.len() == hits.len() && search.pattern.is_some() {
        let kept = hits.iter().zip(&pieces).filter(|(_, p)| p.is_some()).map(|(h, _)| h).collect();
        pieces.retain(Option::is_some);
        kept
    } else {
        hits.iter().collect()
    };
    let spans: Vec<_> = hits.iter().map(|h| (h.para, Unit(h.start), h.last, Unit(h.end))).collect();
    let reps: Vec<Replacement> = pieces.into_iter().map(|p| Replacement { pieces: p.unwrap_or_default(), keep: keep_text }).collect();
    // Replace All within the selection keeps it round what it covered.
    let keep = search.spec.within.as_ref().filter(|_| all).and_then(|w| {
        let (&(p, a, _), &(q, _, b)) = (w.first()?, w.last()?);
        let at = |para: ParaId, u: Unit| e.index().para_slot(para).map(|s| Position::text(e.index().story(s.container), para, u));
        Some((at(p, a)?, at(q, b)?))
    });
    Ok(if commands.is_empty() && reps.iter().all(Replacement::is_typed) {
        e.replace_hits(label, &spans, &reps, all, keep, fonts)
    } else {
        e.replace_formatted(label, &spans, &reps, &commands, all, keep, fonts)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(hay: &str, needle: &str, case: bool, whole: bool) -> Vec<usize> {
        crate::editing::find::matches(&hay.chars().collect::<Vec<_>>(), &needle.chars().collect::<Vec<_>>(), case, whole)
    }

    #[test]
    fn matches_by_case_and_whole_words() {
        assert_eq!(at("The cat, the CAT.", "the", false, false), [0, 9]);
        assert_eq!(at("The cat, the CAT.", "the", true, false), [9]);
        assert_eq!(at("cat concat cats cat", "cat", false, true), [0, 16]);
    }

    #[test]
    fn match_ids_round_trip() {
        let h = Hit::within(ParaId(7), 3, 8);
        assert_eq!(Hit::parse(&h.id()), Some(h));
        let across = Hit { para: ParaId(7), start: 3, end: 0, last: ParaId(9) };
        assert_eq!(Hit::parse(&across.id()), Some(across));
        assert_eq!(Hit::parse(&MatchId("7:3".into())), None);
        assert_eq!(Hit::parse(&MatchId("7:8:3".into())), None);
        assert_eq!(Hit::parse(&MatchId("7:1:7:3".into())), None);
    }
}
