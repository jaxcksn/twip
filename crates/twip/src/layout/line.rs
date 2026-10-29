//! Breaking a shaped paragraph into lines (BREAK-1 to BREAK-9), placing them horizontally (JUST-1 to JUST-6, TAB-1 to
//! TAB-10), sizing them (LINE-1 to LINE-15), the PARA-7 border box, and drawing each line once as a shared
//! [`LineFragment`].
//!
//! Where the rules live: `fit_line` (BREAK-1 to BREAK-4), tabs (TAB-1 to TAB-10), `plan_justified` (BREAK-6), `align`
//! (JUST-1 to JUST-6), `size` (LINE-1 to LINE-15, LIST-7), `apply_borders` (PARA-7) and the line drawing.

use std::cell::OnceCell;
use std::rc::Rc;

use crate::docx::model::*;
use crate::docx::props::{Jc, Leader, LineRule, TabKind};
use crate::docx::review::{AuthorPalette, MarkKind, MarkupView, ReviewView};
use crate::font::Fonts;

use super::cache::{CacheKey, cached};
use super::fragment::{Atom, GlyphRun, ImageKey, Item, LineFragment};
use super::ids::{ParaId, Unit};
use super::job::{Memos, Yield};
use super::space::{BorderKey, ParaSpace};
use super::text::{Kind, Object, Piece, ShapedPara, metrics, para_id};

const EPS: f64 = 1e-6;

/// One line of a paragraph: pieces `start..end` (relative to the lines' start piece), their x positions (from the
/// column's left edge) and drawn widths.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub start: usize,
    pub end: usize,
    pub xs: Vec<f64>,
    pub widths: Vec<f64>,
    /// LINE-1: max(A + E) and max(D).
    pub ascent: f64,
    pub descent: f64,
    /// Line pitch and the baseline's distance from the line top (LINE-6 to LINE-12).
    pub pitch: f64,
    pub baseline: f64,
    /// Extra space below the text from multiple spacing: needn't fit at a page bottom (PAGE-2).
    pub below: f64,
    /// The line ends with a page or column break.
    pub hard: Option<Kind>,
    /// A soft hyphen ends the line: draw a hyphen after it.
    pub hyphen: bool,
    /// Tab leaders on this line: (index into `xs`, leader).
    pub leaders: Vec<(usize, Leader)>,
    /// Rectangles drawn with the line (paragraph borders), relative to the line top and the column's left edge.
    pub rects: Vec<([f64; 4], u32)>,
}

impl Line {
    fn empty(start: usize) -> Line {
        Line {
            start,
            end: start,
            xs: Vec::new(),
            widths: Vec::new(),
            ascent: 0.0,
            descent: 0.0,
            pitch: 0.0,
            baseline: 0.0,
            below: 0.0,
            hard: None,
            hyphen: false,
            leaders: Vec::new(),
            rects: Vec::new(),
        }
    }
}

/// Horizontal context of a paragraph's lines.
pub struct Frame {
    /// Column (or cell content) width.
    pub width: f64,
    pub default_tab: f64,
    pub compat: i64,
    pub expand_shift_return: bool,
    /// COMPAT-3 `splitPgBreakAndParaMark` (compat <= 14): the mark after a page break moves on as an empty line.
    pub split_page_break: bool,
}

impl Frame {
    pub fn new(space: &ParaSpace) -> Frame {
        let e = &space.env;
        Frame {
            width: space.width,
            default_tab: e.default_tab,
            compat: e.compat,
            expand_shift_return: !e.do_not_expand_shift_return,
            split_page_break: e.compat <= 14 && e.split_pg_break_and_para_mark,
        }
    }
}

/// A paragraph's lines at one width, and their drawings, built on first use and shared.
#[derive(Debug)]
pub struct ParaLines {
    pub shaped: Rc<ShapedPara>,
    pub lines: Vec<Line>,
    /// The first piece these lines start at (0 unless continued).
    pub from: usize,
    pub width: f64,
    /// The x the lines are drawn at ([`ParaSpace::x0`]).
    pub x0: f64,
    pub para: ParaId,
    /// Lazily built drawings, one per line. A cache filled behind `&self`: lines are measured far more often than
    /// they are drawn (table autofit, keep-with-next tests), so drawing waits for the first placement.
    drawn: Box<[OnceCell<Rc<LineFragment>>]>,
    compat: i64,
    /// How revisions draw ([`ParaSpace::env`]).
    review: ReviewView,
}

impl PartialEq for ParaLines {
    fn eq(&self, o: &Self) -> bool {
        Rc::ptr_eq(&self.shaped, &o.shaped) && self.lines == o.lines && self.from == o.from && self.width == o.width && self.x0 == o.x0
    }
}

impl ParaLines {
    /// The pieces the lines index into.
    pub fn pieces(&self) -> &[Piece] {
        &self.shaped.pieces[self.from.min(self.shaped.pieces.len())..]
    }

    /// The notes referenced on line `i` (NOTE-5).
    pub fn notes(&self, i: usize) -> Vec<(NoteKind, i64)> {
        let l = &self.lines[i];
        (l.start..l.end).filter_map(|k| self.shaped.note(self.from + k)).collect()
    }

    /// Line `i` drawn (with its top at 0, the column's left edge at [`ParaSpace::x0`]).
    pub fn fragment(&self, i: usize, para: &Paragraph, fonts: &Fonts) -> Rc<LineFragment> {
        self.drawn[i].get_or_init(|| Rc::new(draw_line(&self.origin(), &self.lines[i], para, fonts))).clone()
    }

    /// Where these lines are drawn from.
    pub fn origin(&self) -> LineOrigin<'_> {
        LineOrigin {
            shaped: &self.shaped,
            from: self.from,
            width: self.width,
            x0: self.x0,
            compat: self.compat,
            para: self.para,
            review: &self.review,
        }
    }

    pub fn bytes(&self) -> usize {
        use std::mem::size_of;
        size_of::<ParaLines>() + self.lines.iter().map(|l| size_of::<Line>() + (l.xs.len() + l.widths.len()) * 8 + l.rects.len() * 40).sum::<usize>()
    }
}

/// What a paragraph's lines depend on: its shaped pieces (by identity: the shaped paragraph's own key covers its
/// content) and the line space.
#[derive(Debug)]
pub struct LinesKey {
    shaped: Rc<ShapedPara>,
    space: ParaSpace,
}

#[cfg(test)]
thread_local! {
    /// Test fault (`cache_keys::omitted_key_is_caught`): the key leaves out `ParaSpace.width`, which verify mode
    /// must catch.
    pub static OMIT_WIDTH: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn omit_width() -> bool {
    #[cfg(test)]
    return OMIT_WIDTH.get();
    #[cfg(not(test))]
    false
}

pub struct LinesProbe<'a> {
    pub shaped: &'a Rc<ShapedPara>,
    pub space: &'a ParaSpace,
}

impl CacheKey for LinesKey {
    type Probe<'a> = LinesProbe<'a>;

    fn bucket(p: &LinesProbe) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = super::cache::KeyHasher::default();
        (Rc::as_ptr(p.shaped) as usize).hash(&mut h);
        if !omit_width() {
            p.space.width.to_bits().hash(&mut h);
        }
        p.space.start.hash(&mut h);
        h.finish()
    }

    fn matches(&self, p: &LinesProbe) -> bool {
        Rc::ptr_eq(&self.shaped, p.shaped)
            && (self.space == *p.space || omit_width() && self.space == ParaSpace { width: self.space.width, ..p.space.clone() })
    }

    fn from_probe(p: &LinesProbe) -> LinesKey {
        LinesKey { shaped: p.shaped.clone(), space: p.space.clone() }
    }
}

/// Breaks a shaped paragraph into lines at a width (cached by the shaped paragraph and [`ParaSpace`]).
pub fn break_lines(shaped: &Rc<ShapedPara>, para: &Paragraph, space: &ParaSpace, fonts: &Fonts, memos: &mut Memos) -> Result<Rc<ParaLines>, Yield> {
    let probe = LinesProbe { shaped, space };
    cached(memos, |c| &mut c.lines, probe, ParaLines::bytes, |m| break_lines_uncached(shaped, para, space, fonts, m))
}

/// [`break_lines`] without the cache.
pub fn break_lines_uncached(
    shaped: &Rc<ShapedPara>,
    para: &Paragraph,
    space: &ParaSpace,
    fonts: &Fonts,
    memos: &mut Memos,
) -> Result<ParaLines, Yield> {
    let from = space.start.min(shaped.pieces.len());
    let pieces = &shaped.pieces[from..];
    let frame = Frame::new(space);
    let view = space.env.review.view;
    // REV-2
    let (style, mark) = (para.para_style_in(view), para.mark_in(view));
    let mut lines =
        super::job::unit("line breaking", || crate::prof::time("break", || break_paragraph(shaped, pieces, style, mark, &frame, fonts, from > 0)));
    let prev = if from > 0 { border_key(style) } else { space.borders.0.clone() };
    apply_borders(&mut lines, style, prev.as_deref(), space.borders.1.as_deref(), space.width);
    // shortcut: line breaking is one unit per paragraph until jobs keep partial lines (JobMemo).
    memos.meter.unit_done();
    let n = lines.len();
    Ok(ParaLines {
        shaped: shaped.clone(),
        lines,
        from,
        width: space.width,
        x0: space.x0,
        para: para_id(para),
        drawn: (0..n).map(|_| OnceCell::new()).collect(),
        compat: space.env.compat,
        review: space.env.review.clone(),
    })
}

/// The border settings that make paragraphs share one box (PARA-7: identical settings).
pub fn border_key(s: &ParaStyle) -> BorderKey {
    let k = [s.border_top.clone(), s.border_left.clone(), s.border_bottom.clone(), s.border_right.clone(), s.border_between.clone()];
    k.iter().any(|b| b.is_some()).then(|| Rc::new(k))
}

/// The neighbour keys a paragraph's lines depend on (PARA-7): only a bordered paragraph's box depends on them.
pub fn neighbour_borders(style: &ParaStyle, prev: Option<&ParaStyle>, next: Option<&ParaStyle>) -> (BorderKey, BorderKey) {
    match border_key(style) {
        None => (None, None),
        Some(_) => (prev.and_then(border_key), next.and_then(border_key)),
    }
}

/// PARA-7: the extra space goes into the first and last line; the borders are drawn with them.
fn apply_borders(
    lines: &mut [Line],
    s: &ParaStyle,
    prev: Option<&[Option<crate::docx::props::Border>; 5]>,
    next: Option<&[Option<crate::docx::props::Border>; 5]>,
    width: f64,
) {
    let key = border_key(s);
    let n = lines.len();
    if n == 0 || (key.is_none() && s.shading.is_none()) {
        return;
    }
    let same_prev = key.is_some() && prev == key.as_deref();
    let same_next = key.is_some() && next == key.as_deref();
    let thick = |b: &crate::docx::props::Border| if b.style == "double" { 3.0 * b.width } else { b.width };
    // shortcut: the box's horizontal extent (text edge − space − width) is an approximation (PARA-7).
    let lx = s.border_left.as_ref().map_or(0.0, |b| b.space + thick(b));
    let rx = s.border_right.as_ref().map_or(0.0, |b| b.space + thick(b));
    // From the paragraph's leftmost indent: a hanging first line (a heading's number) is inside the box (PARA-7).
    let left = s.ind_left + s.ind_first.min(0.0);
    let (x0, x1) = (left - lx, width - s.ind_right + rx);
    let color = |b: &crate::docx::props::Border| b.color.unwrap_or(0);
    // A border across `[a, b]` (horizontal: y; else x) from `from` to `to`: one rect, or a double border's two.
    let edge = |rects: &mut Vec<([f64; 4], u32)>, b: &crate::docx::props::Border, horizontal: bool, a: f64, from: f64, to: f64| {
        let parts: Vec<(f64, f64)> =
            if b.style == "double" { vec![(a, a + b.width), (a + 2.0 * b.width, a + 3.0 * b.width)] } else { vec![(a, a + b.width)] };
        for (p, q) in parts {
            rects.push((if horizontal { [from, p, to, q] } else { [p, from, q, to] }, color(b)));
        }
    };
    let (mut shade_top, mut shade_bottom) = (0.0, None);
    if let (Some(t), false) = (&s.border_top, same_prev) {
        let extra = thick(t) + t.space;
        let l = &mut lines[0];
        edge(&mut l.rects, t, true, 0.0, x0, x1);
        l.pitch += extra;
        l.baseline += extra;
        shade_top = thick(t);
    }
    if let (Some(b), false) = (&s.border_bottom, same_next) {
        let l = &mut lines[n - 1];
        let y = l.pitch + b.space;
        edge(&mut l.rects, b, true, y, x0, x1);
        shade_bottom = Some(y);
        l.pitch += b.space + thick(b);
    }
    for (i, l) in lines.iter_mut().enumerate() {
        let h = l.pitch;
        if let Some(b) = &s.border_left {
            edge(&mut l.rects, b, false, x0, 0.0, h);
        }
        if let Some(b) = &s.border_right {
            edge(&mut l.rects, b, false, x1 - thick(b), 0.0, h);
        }
        if let Some(fill) = s.shading {
            let sx0 = s.border_left.as_ref().map_or(left, |b| x0 + thick(b));
            let sx1 = s.border_right.as_ref().map_or(width - s.ind_right, |b| x1 - thick(b));
            let top = if i == 0 { shade_top } else { 0.0 };
            let bottom = if i + 1 == n { shade_bottom.unwrap_or(h) } else { h };
            // Under the borders and the text: first.
            l.rects.insert(0, ([sx0, top, sx1, bottom], fill));
        }
    }
}

struct Fit {
    end: usize,
    xs: Vec<f64>,
    widths: Vec<f64>,
    hard: Option<Kind>,
    hyphen: bool,
    leaders: Vec<(usize, Leader)>,
}

/// Breaks and places a paragraph's pieces.
/// `continued`: the pieces continue a paragraph from an earlier column (no first-line indent).
pub fn break_paragraph(sp: &ShapedPara, pieces: &[Piece], style: &ParaStyle, mark: &Run, frame: &Frame, fonts: &Fonts, continued: bool) -> Vec<Line> {
    let mut lines = Vec::new();
    let right = frame.width - style.ind_right;
    let justify_smart = style.jc == Jc::Both && frame.compat >= 15;
    let mut start = 0;
    let n = pieces.len();
    // Precomputed BREAK-6 line ends, per hard-break segment.
    let mut planned: Vec<usize> = Vec::new();
    if justify_smart {
        planned = plan_justified(pieces, style, frame, continued);
    }
    let mut first = !continued;
    loop {
        let left = style.ind_left + if first { style.ind_first } else { 0.0 };
        let limit_end = planned.iter().copied().find(|&e| e > start);
        let fit = fit_line(sp, pieces, start, left, right, style, frame, limit_end, false);
        let end = fit.end.max(start + 1).min(n.max(start + 1));
        let is_last = end >= n;
        let mut line =
            Line { end: end.min(n), xs: fit.xs, widths: fit.widths, hard: fit.hard, hyphen: fit.hyphen, leaders: fit.leaders, ..Line::empty(start) };
        align(&mut line, sp, pieces, style, frame, left, right, is_last);
        size(&mut line, sp, pieces, style, mark, fonts, is_last, frame.compat);
        lines.push(line);
        first = false;
        if end >= n {
            break;
        }
        start = end;
    }
    // COL-2, LINE-4, COMPAT-3: after a paragraph-final column break, line break, or page break with
    // splitPgBreakAndParaMark, the mark goes on a line of its own.
    if let Some(last) = lines.last() {
        let trailing = last.end == n && matches!(last.hard, Some(Kind::ColumnBreak | Kind::LineBreak))
            || last.end == n && matches!(last.hard, Some(Kind::PageBreak)) && frame.split_page_break;
        if trailing {
            let mut line = Line::empty(n);
            size(&mut line, sp, pieces, style, mark, fonts, true, frame.compat);
            lines.push(line);
        }
    }
    if lines.is_empty() || n == 0 {
        // An empty paragraph: one line from the mark (LINE-4, STYLE-5).
        lines.clear();
        let mut line = Line::empty(0);
        size(&mut line, sp, pieces, style, mark, fonts, true, frame.compat);
        lines.push(line);
    }
    lines
}

/// One band of a paragraph wrapped around floats (FLOAT-7): the free `intervals` (column coordinates), filled one after
/// another word by word, each a segment sharing the band's height. When the band can't take the next word, the
/// trial height a compat <= 14 retry steps down by (FLOAT-4).
#[allow(
    clippy::too_many_arguments,
    reason = "the measured line-breaking helpers take the shaped paragraph, its pieces, style and frame separately, as they were ported"
)]
pub fn band(
    sp: &ShapedPara,
    pieces: &[Piece],
    start: usize,
    intervals: &[(f64, f64)],
    first_line: bool,
    style: &ParaStyle,
    mark: &Run,
    frame: &Frame,
    fonts: &Fonts,
    force: bool,
) -> Result<(Vec<Line>, usize), f64> {
    let n = pieces.len();
    let right_edge = frame.width - style.ind_right;
    let mut segs: Vec<Line> = Vec::new();
    let mut s = start;
    let mut gap = None;
    for &(a, b) in intervals {
        if s >= n && !segs.is_empty() {
            break;
        }
        let first = if first_line && segs.is_empty() { style.ind_first } else { 0.0 };
        // FLOAT-7: beside a float the interval's edge stands in for the left indent; the first-line indent still adds.
        let left = if a > EPS { a.max(style.ind_left) + first } else { a.max(style.ind_left + first) };
        let right = b.min(right_edge);
        if right - left <= EPS {
            continue;
        }
        gap.get_or_insert(right - left);
        // BREAK-6 in the interval too (compat-15 justified), unless its segment has tabs.
        let planned = if style.jc == Jc::Both && frame.compat >= 15 && s < n {
            let seg_end = segment_end(pieces, s);
            match justified_line(pieces, s, seg_end, right - left) {
                LineChoice::End(e) if !pieces[s..seg_end].iter().any(|p| p.kind == Kind::Tab) => Some(e),
                _ => None,
            }
        } else {
            None
        };
        let fit = fit_line(sp, pieces, s, left, right, style, frame, planned, !force && planned.is_none());
        if fit.end <= s && s < n {
            continue;
        }
        let mut line =
            Line { end: fit.end.min(n), xs: fit.xs, widths: fit.widths, hard: fit.hard, hyphen: fit.hyphen, leaders: fit.leaders, ..Line::empty(s) };
        align(&mut line, sp, pieces, style, frame, left, right, fit.end >= n);
        s = fit.end;
        let hard = line.hard.is_some();
        segs.push(line);
        if hard || s >= n {
            break;
        }
    }
    // FLOAT-4 (compat <= 14): the trial height is what was placed when the fit failed, else the failing item's own
    // height: the piece the first gap's width ends in (an inline picture's height alone, a font's line height).
    let own = |k: usize| pieces[k].ascent + pieces[k].descent;
    if segs.is_empty() {
        let mut x = 0.0;
        let k = (start..n).filter(|&k| pieces[k].kind != Kind::Anchor).find(|&k| {
            x += pieces[k].width;
            x > gap.unwrap_or(0.0) + EPS
        });
        return Err(k.or((start..n).find(|&k| pieces[k].kind != Kind::Anchor)).map_or(0.0, own));
    }
    // FLOAT-4 (compat <= 14): tabs alone before content that fits in no gap don't make the line fit; the line retries
    // lower with their height.
    if !force
        && frame.compat <= 14
        && s < n
        && matches!(pieces[s].kind, Kind::Text | Kind::Object)
        && segs.last().is_some_and(|l| l.hard.is_none())
        && pieces[start..s].iter().all(|q| matches!(q.kind, Kind::Tab | Kind::Anchor))
    {
        let placed = start..s;
        let a = placed.clone().map(|k| pieces[k].ascent).fold(0.0, f64::max);
        return Err(a + placed.map(|k| pieces[k].descent).fold(0.0, f64::max));
    }
    // The band's height covers every segment (LINE-1 over the band's pieces).
    let mut merged = segs[0].clone();
    merged.end = s;
    size(&mut merged, sp, pieces, style, mark, fonts, s >= n, frame.compat);
    for l in &mut segs {
        l.ascent = merged.ascent;
        l.descent = merged.descent;
        l.pitch = merged.pitch;
        l.baseline = merged.baseline;
        l.below = merged.below;
    }
    Ok((segs, s))
}

/// A `w:ptab`'s stop (TAB-10): left, centre or right of the text area or of the paragraph's indents, whatever stops it passes.
fn ptab_stop(t: &crate::docx::PTab, style: &ParaStyle, frame: &Frame) -> (f64, TabKind, Leader) {
    let (l, r) = if t.margin { (0.0, frame.width) } else { (style.ind_left, frame.width - style.ind_right) };
    let pos = match t.align {
        TabKind::Center => (l + r) / 2.0,
        TabKind::Right => r,
        _ => l,
    };
    (pos, t.align, t.leader)
}

/// The stop a tab at `pen` goes to (TAB-1 to TAB-3), and its kind.
fn next_stop(pen: f64, style: &ParaStyle, frame: &Frame) -> (f64, TabKind, Leader) {
    let custom = style.tabs.iter().find(|t| t.kind != TabKind::Clear && t.kind != TabKind::Bar && t.pos > pen + EPS);
    let (mut stop, mut kind, mut leader) = match custom {
        Some(t) => (t.pos, t.kind, t.leader),
        None => {
            let inc = frame.default_tab.max(1.0);
            let s = ((pen / inc).floor() + 1.0) * inc;
            (s, TabKind::Left, Leader::None)
        }
    };
    // TAB-3: a hanging indent's left edge acts as a left stop, and default stops before it don't count.
    if pen < style.ind_left - EPS && (stop > style.ind_left + EPS || custom.is_none()) {
        stop = style.ind_left;
        kind = TabKind::Left;
        leader = Leader::None;
    }
    if kind == TabKind::Num {
        kind = TabKind::Left;
    }
    (stop, kind, leader)
}

/// A right/center/decimal tab waiting for its text (TAB-5).
struct Pending {
    idx: usize,
    stop: f64,
    kind: TabKind,
    pen_at_tab: f64,
}

fn text_of<'a>(sp: &'a ShapedPara, p: &Piece) -> &'a str {
    &sp.text[p.text.0 as usize..p.text.1 as usize]
}

/// Resolves a pending tab over pieces `p.idx+1..upto`, shifting them; returns the new pen.
#[allow(
    clippy::too_many_arguments,
    reason = "the measured line-breaking helpers take the shaped paragraph, its pieces, style and frame separately, as they were ported"
)]
fn resolve_tab(sp: &ShapedPara, p: &Pending, pieces: &[Piece], xs: &mut [f64], ws: &mut [f64], start: usize, upto: usize, right: f64) -> f64 {
    let seg: Vec<usize> = (p.idx + 1..upto).collect();
    let w: f64 = seg.iter().map(|&k| ws[k - start]).sum();
    let trail: f64 = seg.iter().rev().take_while(|&&k| pieces[k].kind == Kind::Space).map(|&k| ws[k - start]).sum();
    let d = match p.kind {
        TabKind::Right => w - trail,
        TabKind::Center => (w - trail) / 2.0,
        TabKind::Decimal => {
            let mut acc = 0.0;
            let mut found = false;
            for &k in &seg {
                let t = text_of(sp, &pieces[k]);
                if t.contains('.') || t.contains(',') {
                    found = true;
                    break;
                }
                acc += ws[k - start];
            }
            if found { acc } else { w - trail }
        }
        _ => 0.0,
    };
    let mut tw = (p.stop - p.pen_at_tab) - d;
    // TAB-6: text that would pass the right limit narrows the tab so it ends there.
    if p.pen_at_tab + tw + w - trail > right + EPS && p.stop <= right + EPS {
        tw = right - (p.pen_at_tab + w) + trail;
    }
    let tw = tw.max(0.0); // TAB-8
    ws[p.idx - start] = tw;
    let mut x = p.pen_at_tab + tw;
    for &k in &seg {
        xs[k - start] = x;
        x += ws[k - start];
    }
    x
}

/// Greedy first-fit (BREAK-1 to BREAK-4) from `start`, or up to `limit_end` when the justified planner chose the end.
#[allow(
    clippy::too_many_arguments,
    reason = "the measured line-breaking helpers take the shaped paragraph, its pieces, style and frame separately, as they were ported"
)]
fn fit_line(
    sp: &ShapedPara,
    pieces: &[Piece],
    start: usize,
    left: f64,
    right: f64,
    style: &ParaStyle,
    frame: &Frame,
    limit_end: Option<usize>,
    strict: bool,
) -> Fit {
    let mut xs: Vec<f64> = Vec::new();
    let mut ws: Vec<f64> = Vec::new();
    let mut pen = left;
    // LIST-1: a right-aligned number ends at the first-line position, a centred one is centred there.
    if start == 0 && style.list_jc != "left" {
        let w: f64 = pieces.iter().take_while(|p| p.kind == Kind::ListNumber).map(|p| p.width).sum();
        pen -= if style.list_jc == "right" { w } else { w / 2.0 };
    }
    let mut pending: Option<Pending> = None;
    // Break opportunities: (index after the break, soft hyphen).
    let mut breaks: Vec<(usize, bool)> = Vec::new();
    let mut leaders: Vec<(usize, Leader)> = Vec::new();
    // BREAK-9: the word squeezed in past the right edge (its first piece) and the line's space shrink available.
    let mut squeeze: Option<(usize, f64)> = None;
    let mut past_custom = false;
    let n = pieces.len();
    let mut i = start;
    while i < n {
        if let Some(le) = limit_end
            && i >= le
        {
            break;
        }
        let p = &pieces[i];
        match p.kind {
            Kind::LineBreak | Kind::PageBreak | Kind::ColumnBreak => {
                if let Some(pt) = pending.take() {
                    pen = resolve_tab(sp, &pt, pieces, &mut xs, &mut ws, start, i, right);
                }
                xs.push(pen);
                ws.push(0.0);
                if let Some((w, cap)) = squeeze {
                    shrink_spaces(sp, pieces, start, w, cap, right, &mut xs, &mut ws);
                }
                return Fit { end: i + 1, xs, widths: ws, hard: Some(p.kind), hyphen: false, leaders };
            }
            Kind::Tab => {
                if let Some(pt) = pending.take() {
                    pen = resolve_tab(sp, &pt, pieces, &mut xs, &mut ws, start, i, right);
                }
                let (mut stop, mut kind, leader) = match p.object.map(|o| &sp.objects[o as usize]) {
                    Some(Object::PTab(t)) => ptab_stop(t, style, frame),
                    _ => next_stop(pen, style, frame),
                };
                // TAB-7: a default stop past the right edge starts the next line with the tab, unless (compat <= 14) a
                // custom stop past the edge came first on this line.
                let stop_custom = style.tabs.iter().any(|t| t.kind != TabKind::Clear && t.kind != TabKind::Bar && t.pos > pen + EPS);
                let custom = p.object.is_some() || stop_custom;
                if !custom && stop > right + EPS && !past_custom && i > start && limit_end.is_none() {
                    return Fit { end: i, xs, widths: ws, hard: None, hyphen: false, leaders };
                }
                // TAB-7 (compat 15): a right, centre or decimal stop past the right edge moves to it when its text runs
                // to the paragraph's end (centred text then ends there too). Any other custom stop past the edge ends
                // the line before the tab; a tab first on its line stays, and its text starts the next line.
                let edge = frame.width - style.ind_right;
                if frame.compat >= 15 && p.object.is_none() && stop_custom && stop > edge {
                    if kind != TabKind::Left && !pieces[i + 1..].iter().any(|q| q.kind == Kind::Tab) {
                        stop = edge;
                        if kind == TabKind::Center {
                            kind = TabKind::Right;
                        }
                    } else if i > start && limit_end.is_none() {
                        return Fit { end: i, xs, widths: ws, hard: None, hyphen: false, leaders };
                    } else {
                        kind = TabKind::Left;
                    }
                }
                past_custom |= frame.compat <= 14 && custom && stop > right + EPS;
                if leader != Leader::None {
                    leaders.push((i - start, leader));
                }
                xs.push(pen);
                if kind == TabKind::Left || kind == TabKind::Bar {
                    let w = (stop - pen).max(0.0);
                    ws.push(w);
                    pen += w;
                } else {
                    ws.push(0.0);
                    pending = Some(Pending { idx: i, stop, kind, pen_at_tab: pen });
                }
                breaks.push((i + 1, false));
                i += 1;
                continue;
            }
            Kind::Space | Kind::Anchor => {
                // BREAK-4: after a character forced onto a line it overflows (wider than the line), a space doesn't
                // hang (BREAK-2): it starts the next line, where spaces hang again.
                // The character ends past the line, not just the spaces hanging after it.
                let ink = (start..i).rev().find(|&j| pieces[j].kind != Kind::Space);
                if p.kind == Kind::Space
                    && ink.is_some_and(|j| pen - pieces[j + 1..i].iter().map(|q| q.width).sum::<f64>() > right + EPS)
                    && squeeze.is_none()
                    && pending.is_none()
                    && limit_end.is_none()
                    && !pieces[start..i].iter().any(|q| matches!(q.kind, Kind::Tab | Kind::ListNumber))
                {
                    return Fit { end: i, xs, widths: ws, hard: None, hyphen: false, leaders };
                }
                xs.push(pen);
                ws.push(p.width);
                pen += p.width;
                if p.break_after {
                    breaks.push((i + 1, false));
                }
                i += 1;
                continue;
            }
            _ => {}
        }
        // A character or object: the fit test (BREAK-1), with a pending right/center tab pulling text left.
        let right_edge = match &pending {
            Some(pt) if pt.kind == TabKind::Right => {
                let w: f64 = ws[pt.idx - start + 1..].iter().sum::<f64>() + p.width;
                (pt.pen_at_tab + w).max(pt.stop)
            }
            Some(pt) if pt.kind == TabKind::Center => {
                let w: f64 = ws[pt.idx - start + 1..].iter().sum::<f64>() + p.width;
                (pt.pen_at_tab + w).max(pt.stop + w / 2.0)
            }
            _ => pen + p.width,
        };
        let forced = limit_end.is_some();
        // TAB-7 (compat <= 14): a right stop past the right limit is used as is: the text runs to the stop, past the
        // margin. After any other custom stop past the limit the line doesn't wrap: text is centred on a centre stop,
        // and later default stops are used where they are.
        let limit = match &pending {
            Some(pt) if pt.kind == TabKind::Right && pt.stop > right => pt.stop,
            _ if past_custom => f64::INFINITY,
            _ => right,
        };
        // BREAK-9: the word crossing the right edge may squeeze in (`shrink` is 0 where the rule doesn't apply).
        let over = right_edge > limit + EPS && !forced && p.kind != Kind::ListNumber;
        let word = breaks.last().map_or(start, |b| b.0);
        let room: f64 = match squeeze {
            _ if !over => 0.0,
            Some((w, cap)) if w == word => cap,
            None if frame.compat <= 14 && word > start && pending.is_none() => {
                (squeeze_from(pieces, start, word)..word).map(|k| shrink(sp, pieces, k)).sum()
            }
            _ => 0.0,
        };
        if over && right_edge <= limit + room + EPS {
            squeeze = Some((word, room));
        } else if over {
            // Back up to the last break opportunity whose soft hyphen (if any) fits with its hyphen.
            while let Some(&(at, soft)) = breaks.last() {
                if at <= start {
                    breaks.pop();
                    continue;
                }
                if soft {
                    let hy = hyphen_width(sp, &pieces[at - 1]);
                    let pen_at = xs[at - 1 - start];
                    if pen_at + hy > right + EPS {
                        breaks.pop();
                        continue;
                    }
                }
                xs.truncate(at - start);
                ws.truncate(at - start);
                leaders.retain(|(k, _)| *k < at - start);
                if let Some((w, cap)) = squeeze.filter(|&(w, _)| at > w) {
                    shrink_spaces(sp, pieces, start, w, cap, limit, &mut xs, &mut ws);
                }
                if let Some(pt) = pending.take()
                    && pt.idx < at
                {
                    resolve_tab(sp, &pt, pieces, &mut xs, &mut ws, start, at, right);
                }
                return Fit { end: at, xs, widths: ws, hard: None, hyphen: soft, leaders };
            }
            // FLOAT-7: in a wrap interval a word that doesn't fit leaves the interval empty.
            if strict {
                return Fit { end: start, xs: Vec::new(), widths: Vec::new(), hard: None, hyphen: false, leaders: Vec::new() };
            }
            // BREAK-4: no break fits; split at the last character that fits (at least one).
            if i > start && pieces[start..i].iter().any(|q| q.kind != Kind::ListNumber) {
                if let Some(pt) = pending.take() {
                    resolve_tab(sp, &pt, pieces, &mut xs, &mut ws, start, i, right);
                }
                return Fit { end: i, xs, widths: ws, hard: None, hyphen: false, leaders };
            }
        }
        xs.push(pen);
        ws.push(p.width);
        pen += p.width;
        if p.break_after {
            breaks.push((i + 1, p.soft_hyphen));
        }
        i += 1;
    }
    if let Some(pt) = pending.take() {
        resolve_tab(sp, &pt, pieces, &mut xs, &mut ws, start, i, right);
    }
    if let Some((w, cap)) = squeeze {
        shrink_spaces(sp, pieces, start, w, cap, right, &mut xs, &mut ws);
    }
    let hyphen = i < n && i > start && pieces[i - 1].soft_hyphen;
    Fit { end: i, xs, widths: ws, hard: None, hyphen, leaders }
}

/// BREAK-9: the first piece whose space may give way before the squeezed-in word `word`: the line's start, or just
/// after its last tab (spaces before a tab only move the tab's text, which the tab takes back).
fn squeeze_from(pieces: &[Piece], start: usize, word: usize) -> usize {
    pieces[start..word].iter().rposition(|q| q.kind == Kind::Tab).map_or(start, |t| start + t + 1)
}

/// BREAK-9: what piece `k` may give up when its line squeezes.
fn shrink(sp: &ShapedPara, pieces: &[Piece], k: usize) -> f64 {
    let p = &pieces[k];
    let st = &sp.styles[p.style as usize];
    let space = |j: Option<usize>| j.and_then(|j| pieces.get(j)).is_some_and(|q| q.kind == Kind::Space);
    if p.kind != Kind::Space || st.squeeze_avg == 0.0 || text_of(sp, p) != " " || space(k.checked_sub(1)) || space(Some(k + 1)) {
        return 0.0;
    }
    (p.width - st.squeeze_avg * st.size * st.run.scale / 100.0 / 2.0).max(0.0)
}

/// BREAK-9: shrinks the spaces before the squeezed-in word `word`, in proportion to what each may give up, until the
/// line's last non-space ends at `limit`.
#[allow(clippy::too_many_arguments, reason = "the line's pieces, the squeezed word and its room, and the positions it rewrites")]
fn shrink_spaces(sp: &ShapedPara, pieces: &[Piece], start: usize, word: usize, cap: f64, limit: f64, xs: &mut [f64], ws: &mut [f64]) {
    let Some(last) = (0..xs.len()).rev().find(|&k| pieces[start + k].kind != Kind::Space) else { return };
    let over = xs[last] + ws[last] - limit;
    if over <= 0.0 {
        return;
    }
    let f = (over / cap).min(1.0);
    for k in squeeze_from(pieces, start, word) - start..(word - start).min(ws.len()) {
        ws[k] -= shrink(sp, pieces, start + k) * f;
    }
    for k in 1..xs.len() {
        xs[k] = xs[k - 1] + ws[k - 1];
    }
}

fn hyphen_width(sp: &ShapedPara, p: &Piece) -> f64 {
    let st = &sp.styles[p.style as usize];
    match &st.face {
        Some(f) => f.glyph('-').map(|g| f.advance(g) * f.scale(st.size)).unwrap_or(0.0),
        None => 0.0,
    }
}

/// BREAK-6: line ends for a compat-15 justified paragraph, as piece indices, planned per hard-break segment over
/// space-separated words; empty (greedy for the whole paragraph) when the paragraph has a tab.
fn plan_justified(pieces: &[Piece], style: &ParaStyle, frame: &Frame, continued: bool) -> Vec<usize> {
    let mut ends = Vec::new();
    let right = frame.width - style.ind_right;
    let mut seg_start = 0;
    let mut first_line = !continued;
    while seg_start < pieces.len() {
        let seg_end = segment_end(pieces, seg_start);
        if pieces[seg_start..seg_end].iter().any(|p| p.kind == Kind::Tab) {
            return Vec::new();
        }
        let mut pos = seg_start;
        loop {
            let column = right - style.ind_left - if first_line { style.ind_first } else { 0.0 };
            match justified_line(pieces, pos, seg_end, column) {
                LineChoice::Empty => break,
                LineChoice::Last => {
                    first_line = false;
                    break;
                }
                LineChoice::Abort => return Vec::new(),
                LineChoice::End(end) => {
                    first_line = false;
                    ends.push(end);
                    pos = end;
                }
            }
        }
        seg_start = seg_end;
        if seg_end < pieces.len() {
            ends.push(seg_end);
        }
    }
    ends
}

/// The end of the hard-break segment (its break included) that `from` is in.
fn segment_end(pieces: &[Piece], from: usize) -> usize {
    pieces[from..]
        .iter()
        .position(|p| matches!(p.kind, Kind::LineBreak | Kind::PageBreak | Kind::ColumnBreak))
        .map(|k| from + k + 1)
        .unwrap_or(pieces.len())
}

/// Where a justified line ends (BREAK-6).
enum LineChoice {
    /// No words left in the segment.
    Empty,
    /// The rest fits without shrinking: the segment's last line.
    Last,
    /// Not even one word fits: the greedy breaker splits it (BREAK-4).
    Abort,
    /// The line ends at this piece (after the spaces that follow its last word).
    End(usize),
}

/// BREAK-6: where a justified line starting at `from` ends in `column` points: a per-line choice, so it serves a full
/// line and an interval beside a float alike.
fn justified_line(pieces: &[Piece], from: usize, seg_end: usize, column: f64) -> LineChoice {
    // Words (start, end, width) and the gap width before each word.
    let mut words: Vec<(usize, usize, f64)> = Vec::new();
    let mut gaps: Vec<f64> = Vec::new();
    let mut k = from;
    let mut gap = 0.0;
    while k < seg_end {
        let p = &pieces[k];
        if matches!(p.kind, Kind::Space) {
            gap += p.width;
            k += 1;
            continue;
        }
        if matches!(p.kind, Kind::LineBreak | Kind::PageBreak | Kind::ColumnBreak | Kind::Anchor) {
            k += 1;
            continue;
        }
        let ws = k;
        let mut w = 0.0;
        while k < seg_end && !matches!(pieces[k].kind, Kind::Space | Kind::LineBreak | Kind::PageBreak | Kind::ColumnBreak) {
            w += pieces[k].width;
            k += 1;
            // A break opportunity inside it (after a hyphen) ends a word too, with no gap to the next (BREAK-3).
            if pieces[k - 1].break_after && !pieces[k - 1].soft_hyphen {
                break;
            }
        }
        gaps.push(if words.is_empty() { 0.0 } else { gap });
        gap = 0.0;
        words.push((ws, k, w));
    }
    if words.is_empty() {
        return LineChoice::Empty;
    }
    let i = 0;
    let natural = |a: usize, e: usize| -> f64 { (a..e).map(|j| words[j].2 + if j > a { gaps[j] } else { 0.0 }).sum() };
    let gapsum = |a: usize, e: usize| -> f64 { (a + 1..e).map(|j| gaps[j]).sum() };
    // jmax: the most words that fit with every space shrunk by 25% (hard limit).
    let mut jmax = i;
    for e in i + 1..=words.len() {
        if natural(i, e) - 0.25 * gapsum(i, e) <= column + EPS {
            jmax = e;
        } else {
            break;
        }
    }
    if natural(i, words.len()) <= column + EPS {
        return LineChoice::Last;
    }
    if jmax <= i {
        return LineChoice::Abort;
    }
    let mut best = jmax;
    let mut best_r = f64::INFINITY;
    for e in i + 1..=jmax {
        let g = gapsum(i, e);
        if e == i + 1 && jmax > i + 1 {
            continue; // a one-word line only if nothing else fits
        }
        let excess = natural(i, e) - column;
        let r = if g <= 0.0 {
            if excess > 0.0 { f64::INFINITY } else { f64::MAX / 2.0 }
        } else if excess > 0.0 {
            excess / (g * 0.25)
        } else {
            -excess / (g * 0.5)
        };
        if r <= best_r + 1e-12 {
            best_r = r;
            best = e;
        }
    }
    // The line ends after word best-1, including the spaces that follow it (they hang, BREAK-2).
    let mut end = words[best - 1].1;
    while end < seg_end && pieces[end].kind == Kind::Space {
        end += 1;
    }
    LineChoice::End(end)
}

/// Justification and alignment (JUST-1 to JUST-6).
#[allow(
    clippy::too_many_arguments,
    reason = "the measured line-breaking helpers take the shaped paragraph, its pieces, style and frame separately, as they were ported"
)]
fn align(line: &mut Line, sp: &ShapedPara, pieces: &[Piece], style: &ParaStyle, frame: &Frame, left: f64, right: f64, is_last: bool) {
    let n = line.xs.len();
    if n == 0 {
        return;
    }
    let idx = |k: usize| line.start + k;
    // Trailing spaces (and the break) hang (BREAK-2).
    let mut content_end = n;
    while content_end > 0
        && matches!(pieces[idx(content_end - 1)].kind, Kind::Space | Kind::LineBreak | Kind::PageBreak | Kind::ColumnBreak | Kind::Anchor)
    {
        content_end -= 1;
    }
    let line_end_x = if content_end == 0 { left } else { line.xs[content_end - 1] + line.widths[content_end - 1] }
        + if line.hyphen { hyphen_width(sp, &pieces[idx(n - 1)]) } else { 0.0 };
    let ends_with_line_break = line.end > 0 && pieces.get(line.end - 1).is_some_and(|p| p.kind == Kind::LineBreak);
    match style.jc {
        Jc::Both => {
            let justify = !is_last && (line.hard.is_none() || line.hard == Some(Kind::LineBreak));
            let justify = justify && !(ends_with_line_break && (frame.compat < 15 || !frame.expand_shift_return));
            // BREAK-6 may leave the last line compressed.
            let overflow = line_end_x - right;
            if !(justify || (frame.compat >= 15 && overflow > EPS)) {
                return;
            }
            // JUST-6: after the last tab, skipping leading spaces.
            let last_tab = (0..content_end).rev().find(|&k| pieces[idx(k)].kind == Kind::Tab).map(|k| k + 1).unwrap_or(0);
            let mut first = last_tab;
            while first < content_end && pieces[idx(first)].kind == Kind::Space {
                first += 1;
            }
            let spaces: Vec<usize> = (first..content_end).filter(|&k| pieces[idx(k)].kind == Kind::Space).collect();
            if spaces.is_empty() {
                return;
            }
            let extra = right - line_end_x;
            if extra > 0.0 && justify {
                // JUST-1: shared equally among the spaces.
                let q = extra / spaces.len() as f64;
                for &k in &spaces {
                    line.widths[k] += q;
                }
            } else if extra < 0.0 && frame.compat >= 15 {
                // JUST-2: each space shrinks by at most a quarter; the unused capacity is shared equally.
                let caps: Vec<f64> = spaces.iter().map(|&k| line.widths[k] / 4.0).collect();
                let c: f64 = caps.iter().sum();
                let used = (-extra).min(c);
                let q = (c - used) / spaces.len() as f64;
                for (j, &k) in spaces.iter().enumerate() {
                    line.widths[k] -= caps[j] - q;
                }
            } else {
                return;
            }
            let mut x = line.xs[0];
            for k in 0..n {
                if k >= first || k == 0 {
                    line.xs[k] = x;
                }
                x = line.xs[k] + line.widths[k];
            }
        }
        Jc::Center | Jc::Right => {
            // JUST-4.
            let free = right - line_end_x;
            if free > 0.0 {
                let off = if style.jc == Jc::Center { free / 2.0 } else { free };
                for x in &mut line.xs {
                    *x += off;
                }
            }
        }
        Jc::Distribute => {
            // JUST-5: shared per character, excluding the last.
            let chars: Vec<usize> = (0..content_end).filter(|&k| pieces[idx(k)].kind != Kind::Tab).collect();
            if chars.len() > 1 {
                let extra = right - line_end_x;
                if extra > 0.0 {
                    let q = extra / (chars.len() - 1) as f64;
                    for &k in &chars[..chars.len() - 1] {
                        line.widths[k] += q;
                    }
                    let mut x = line.xs[0];
                    for k in 0..n {
                        line.xs[k] = x;
                        x += line.widths[k];
                    }
                }
            }
        }
        Jc::Left => {}
    }
}

/// Vertical size of a line: LINE-1 to LINE-5, LIST-7, FLOAT-5, then the line spacing rule (LINE-6 to LINE-13).
fn size(line: &mut Line, sp: &ShapedPara, pieces: &[Piece], style: &ParaStyle, mark: &Run, fonts: &Fonts, last: bool, compat: i64) {
    let (mut asc, mut desc) = (0.0f64, 0.0f64);
    let mut object = false;
    // The same without raised or lowered text's shift (w:position): what a line multiple scales, so the line grows by
    // the shift once (LINE-13).
    let (mut un_asc, mut un_desc, mut un_text_asc, mut un_text_desc) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
    let (mut em_up, mut em_down) = (0.0f64, 0.0f64);
    let mut num_asc = 0.0f64;
    let mut has_content = false;
    let mut all_shifted = true;
    let mut brk: Option<(f64, f64)> = None;
    for p in &pieces[line.start..line.end.min(pieces.len())] {
        match p.kind {
            Kind::Text | Kind::Object => {
                has_content = true;
                all_shifted &= p.kind == Kind::Text && sp.styles[p.style as usize].run.vert_align != crate::docx::props::VertAlign::Baseline;
                asc = asc.max(p.ascent);
                desc = desc.max(p.descent);
                let st = &sp.styles[p.style as usize];
                let r = st.run.position; // w:position only: a superscript's raise is scaled
                let (a0, d0) = ((p.ascent - r).max(0.0), (p.descent + r).max(0.0));
                // LINE-15: an emphasis mark sits over the line's tallest text, adding its height to the line (in
                // proportion to the marked text's size; an under-dot below), unscaled by a line multiple.
                if p.kind == Kind::Text {
                    match st.run.em.as_deref() {
                        Some("underDot") => em_down = em_down.max(super::text::EM_MARK * st.size),
                        Some(_) => em_up = em_up.max(super::text::EM_MARK * st.size),
                        None => {}
                    }
                }
                un_asc = un_asc.max(a0);
                un_desc = un_desc.max(d0);
                if p.kind == Kind::Object {
                    object = true;
                } else {
                    un_text_asc = un_text_asc.max(a0);
                    un_text_desc = un_text_desc.max(d0);
                }
            }
            Kind::ListNumber => {
                has_content = true;
                num_asc = num_asc.max(p.ascent);
            }
            Kind::LineBreak => brk = Some((p.ascent, p.descent)),
            // LINE-5: spaces and tabs don't raise a line.
            Kind::Space | Kind::Tab => {}
            _ => {}
        }
    }
    // LINE-4: a line holding only a line break (or spaces before it) takes the break's font.
    if !has_content && let Some((a, d)) = brk {
        asc = a;
        desc = d;
        un_asc = a;
        un_desc = d;
    }
    // LINE-4, LINE-2, STYLE-5: an empty line (or only spaces), and a paragraph's last line holding only
    // superscript/subscript text, take the paragraph mark's font.
    else if (!has_content || (last && all_shifted))
        && let Some(f) = fonts.face(&mark.ascii, mark.bold, mark.italic)
    {
        let (a, d, e) = metrics(&f, mark.size);
        asc = asc.max(a + e);
        desc = desc.max(d);
        un_asc = un_asc.max(a + e);
        un_desc = un_desc.max(d);
    }
    // LINE-4: a paragraph's last line holding pictures but no text is at least the mark's single height, all of it above
    // the baseline (the pictures' bottom stays on the line's bottom).
    if last
        && object
        && un_text_asc + un_text_desc + num_asc == 0.0
        && let Some(f) = fonts.face(&mark.ascii, mark.bold, mark.italic)
    {
        let (a, d, e) = metrics(&f, mark.size);
        asc = asc.max(a + e + d);
    }
    let asc = asc.max(num_asc); // LIST-7: the number's ascent, not its descent
    // LIST-7: a line multiple scales the text's height, not a taller number's (a line holding only the number: its).
    let un_asc = if un_asc > 0.0 { un_asc } else { num_asc };
    let (asc, desc) = (asc + em_up, desc + em_down);
    let h = asc + desc;
    line.ascent = asc;
    line.descent = desc;
    // LINE-13: on a line holding a picture the multiple scales the text's height only: the mark's when the line has no
    // text, and in compat 15 the mark's too when it's taller (not on text-only lines, where H = h).
    let mut text_h = un_asc + un_desc;
    if object {
        text_h = un_text_asc + un_text_desc;
        if (text_h == 0.0 || compat >= 15 && last)
            && let Some(f) = fonts.face(&mark.ascii, mark.bold, mark.italic)
        {
            let (a, d, e) = metrics(&f, mark.size);
            text_h = text_h.max(a + e + d);
        }
    }
    match style.line_rule {
        LineRule::Auto => {
            // Height h + (m − 1)·H; below single the cut comes off the top (a picture rises above the line, LINE-13)
            // and m·descent stays under the baseline.
            let m = style.line;
            line.pitch = h + (m - 1.0) * text_h;
            if m >= 1.0 {
                line.baseline = asc;
                line.below = line.pitch - h;
            } else {
                line.baseline = line.pitch - m * desc;
            }
        }
        LineRule::AtLeast => {
            let x = style.line;
            line.pitch = x.max(h);
            line.baseline = if x <= h { asc } else { x - desc };
        }
        LineRule::Exact => {
            line.pitch = style.line;
            // LINE-10, LINE-11.
            line.baseline = style.line * 0.8
                + match style.text_alignment.as_str() {
                    "top" => asc,
                    "center" => (asc - desc) / 2.0,
                    "bottom" => -desc / 2.0,
                    _ => 0.0,
                };
        }
    }
}

/// A Symbol/Wingdings private-use character (U+F0xx), or its Latin-1 code (a symbol font shows the same glyph: bullet
/// "·" in Symbol, "§" in Wingdings, FONT-13), as the Unicode character it shows, for the text a page carries
/// (accessibility, copy, comparison); other characters unchanged.
pub fn symbol_unicode(family: &str, c: char) -> char {
    let cp = c as u32;
    let code = match cp {
        0xF020..=0xF0FF => cp - 0xF000,
        0x20..=0xFF if matches!(family, "Symbol" | "Wingdings") => cp,
        _ => return c,
    };
    let mapped = match family {
        "Symbol" => match code {
            0xB7 => 0x2022, // bullet
            0xA7 => 0x2663,
            0xA8 => 0x2666,
            0xA9 => 0x2665,
            0xAA => 0x2660,
            0xB0 => 0xB0,
            0xB1 => 0xB1,
            0xB4 => 0xD7,
            0xB8 => 0xF7,
            0xAE => 0x2192,
            0xAC => 0x2190,
            0xD6 => 0x221A,
            0xE5 => 0x2211,
            _ => 0,
        },
        "Wingdings" => match code {
            0xA7 => 0x25AA, // small square bullet
            0x6C => 0x25CF,
            0x6E => 0x25A0,
            0x6F => 0x25A1,
            0x71 => 0x2751,
            0x75 => 0x25C6,
            0x76 => 0x2756,
            0xA8 => 0x25FB,
            0xD8 => 0x27A2,
            0xFC => 0x2713,
            0xFD => 0x2612,
            0xFE => 0x2611,
            0x9F => 0x2022,
            _ => 0,
        },
        _ => 0,
    };
    char::from_u32(mapped).filter(|_| mapped != 0).unwrap_or(c)
}

/// A run's text colour: its revision's colour in All markup (REV-3); else automatic text is white on a dark highlight
/// or shading (FONT-15), else black (`None`).
fn ink(r: &RunStyle, review: &ReviewView) -> Option<u32> {
    if let Some(c) = revision_colour(r, review) {
        return Some(c);
    }
    let dark = |c: &u32| (0.299 * (c >> 16) as f64 + 0.587 * ((c >> 8) & 0xFF) as f64 + 0.114 * (c & 0xFF) as f64) < 110.0;
    r.color.or_else(|| r.highlight.or(r.shading).filter(dark).map(|_| 0xFFFFFF))
}

/// REV-3: in All markup, an inserted, deleted or moved run draws in its author's colour, a move in the move colour.
/// A deleted insertion takes its deletion's.
fn revision_colour(r: &RunStyle, review: &ReviewView) -> Option<u32> {
    let m = r.rev.del.as_ref().or(r.rev.ins.as_ref()).filter(|_| review.view == MarkupView::All)?;
    Some(if matches!(m.kind, MarkKind::MoveFrom | MarkKind::MoveTo) { AuthorPalette::MOVE } else { review.palette.color(m.author) })
}

/// REV-4: whether a line shows a revision: a property change of its paragraph (a list numbering change alone isn't one),
/// revised text on it (or hidden by the view where it sits), or, on the last line, a revised paragraph mark.
fn line_changed(o: &LineOrigin, line: &Line, src: &Paragraph) -> bool {
    let view = o.review.view;
    let last = line.end >= o.pieces().len();
    if src.style.change.as_ref().is_some_and(|c| c.previous_raw.0.is_some()) || (last && !src.mark.rev.is_empty()) {
        return true;
    }
    if !src.inlines.iter().any(|i| i.run().is_some_and(|r| !r.rev.is_empty())) {
        return false;
    }
    let pieces = &o.pieces()[line.start..line.end];
    let lo = pieces.iter().filter_map(|p| p.src).map(|u| u.0).min();
    let hi = pieces.iter().filter_map(|p| p.src.map(|u| u.0 + text_of(o.shaped, p).chars().count().max(1) as u32)).max();
    let (Some(lo), Some(hi)) = (lo, hi) else { return false };
    let mut at = 0u32;
    src.inlines.iter().any(|i| {
        let (a, n) = (at, i.units() as u32);
        at += n;
        // Hidden content counts on the line it would sit at the end of.
        i.run().is_some_and(|r| !r.rev.is_empty() && n > 0 && if view.hides(&r.rev) { a <= hi && a + n >= lo } else { a < hi && a + n > lo })
    })
}

/// What drawing a paragraph's line needs: its shaped pieces from the lines' start piece, the width and origin the lines
/// were laid out for, the compat mode (NOTE-3's separator length) and the paragraph's identity.
pub struct LineOrigin<'a> {
    pub shaped: &'a ShapedPara,
    pub from: usize,
    pub width: f64,
    pub x0: f64,
    pub compat: i64,
    pub para: ParaId,
    pub review: &'a ReviewView,
}

impl LineOrigin<'_> {
    fn pieces(&self) -> &[Piece] {
        &self.shaped.pieces[self.from.min(self.shaped.pieces.len())..]
    }
}

/// Draws a line with its top at 0 and the column's left edge at the origin's x.
pub fn draw_line(o: &LineOrigin, line: &Line, src: &Paragraph, fonts: &Fonts) -> LineFragment {
    {
        let sp = o.shaped;
        let pieces = o.pieces();
        let para = o.para;
        let (x0, top) = (o.x0, 0.0);
        let base = top + line.baseline;
        let mut out: Vec<Item> = Vec::with_capacity(line.rects.len() + 2);
        // Runs are sized for the rest of the line: one allocation each instead of growing.
        let (mut glyphs_left, mut chars_left) = (line.start..line.end).fold((0, 0), |(g, c), ix| {
            let (a, b) = pieces[ix].glyphs;
            (g + (b - a) as usize, c + text_of(sp, &pieces[ix]).len())
        });
        for (r, c) in &line.rects {
            out.push(Item::Rect { rect: [x0 + r[0], top + r[1], x0 + r[2], top + r[3]], colour: *c });
        }
        let rects_end = out.len();
        let mut run: Option<GlyphRunBuilder> = None;
        let flush = |run: &mut Option<GlyphRunBuilder>, out: &mut Vec<Item>| {
            if let Some(r) = run.take()
                && !r.glyphs.is_empty()
            {
                out.push(Item::Glyphs(r.finish()));
            }
        };
        let mut atoms: Vec<Atom> = Vec::new();
        // REV-6: a joined paragraph's pieces keep their own paragraphs and units (generated text, its first's).
        let join = sp.join.as_deref();
        let source = |p: &Piece| match (join, p.src) {
            (Some(j), Some(u)) => {
                let (q, v) = j.source(u);
                (q, Some(v))
            }
            (Some(j), None) => (j.first(), None),
            (None, u) => (para, u),
        };
        // A paragraph that draws no characters on the line still gets a caret position there: a run without glyphs, in
        // the paragraph mark's font, drawn before the piece it is at.
        let mut places = caret_places(o, line).into_iter().peekable();
        let mark = src.mark_in(o.review.view);
        let face = places.peek().and_then(|_| fonts.face(&mark.ascii, mark.bold, mark.italic));
        let caret = |x: f64, q: ParaId| {
            face.as_ref().map(|f| {
                let (glyphs, chars) = (Box::new([]), Box::new([]));
                Item::Glyphs(GlyphRun { face: f.clone(), size: mark.size, baseline: base, x, glyphs, chars, colour: None, para: Some(q) })
            })
        };
        for (k, ix) in (line.start..line.end).enumerate() {
            let p = &pieces[ix];
            let st = &sp.styles[p.style as usize];
            let x = x0 + line.xs[k];
            while let Some((_, q)) = places.next_if(|e| e.0 == Some(k)) {
                flush(&mut run, &mut out);
                out.extend(caret(x, q));
            }
            let (q, first) = source(p);
            if matches!(p.kind, Kind::Separator | Kind::ContSeparator) {
                // NOTE-3: the separator rule.
                let len = if p.kind == Kind::ContSeparator && o.compat <= 14 { o.width } else { 144.0 };
                out.push(Item::Rect { rect: [x, base - 2.7, x + len, base - 2.1], colour: 0 });
                continue;
            }
            // A unit drawn without characters ends where the next piece starts.
            let next_x = line.xs.get(k + 1).map(|&nx| x0 + nx);
            if p.kind == Kind::Tab {
                flush(&mut run, &mut out);
                draw_leader(o, line, k, x0, base, &mut out);
                if let Some(unit) = first {
                    atoms.push(Atom { x0: x, x1: next_x.unwrap_or(x), para: q, unit, brk: false });
                }
                continue;
            }
            if p.kind == Kind::Object {
                // FLOAT-5: an inline picture's bottom sits on the baseline.
                if let Some(Object::Picture(pic)) = p.object.map(|o| &sp.objects[o as usize]) {
                    flush(&mut run, &mut out);
                    out.push(Item::Image { rect: [x, base - pic.h, x + pic.w, base], key: ImageKey { index: pic.image, generation: 0 } });
                    if let Some(unit) = first {
                        atoms.push(Atom { x0: x, x1: next_x.unwrap_or(x + pic.w), para: q, unit, brk: false });
                    }
                }
                continue;
            }
            // A break draws nothing; its unit still gets a caret stop where it sits (the end of its line).
            if matches!(p.kind, Kind::LineBreak | Kind::PageBreak | Kind::ColumnBreak) {
                if let Some(unit) = first {
                    atoms.push(Atom { x0: x, x1: x, para: q, unit, brk: true });
                }
                continue;
            }
            if !matches!(p.kind, Kind::Text | Kind::Space | Kind::ListNumber) {
                continue;
            }
            let Some(face) = &st.face else { continue };
            if p.soft_hyphen && !(line.hyphen && ix + 1 == line.end) {
                continue;
            }
            let y = base - st.raise;
            let color = ink(&st.run, o.review);
            let same = run.as_ref().is_some_and(|r| {
                Rc::ptr_eq(&r.face, face) && (r.size - st.size).abs() < EPS && (r.y - y).abs() < EPS && r.color == color && r.para == Some(q)
            });
            if !same {
                flush(&mut run, &mut out);
                run = Some(GlyphRunBuilder {
                    face: face.clone(),
                    size: st.size,
                    y,
                    glyphs: Vec::with_capacity(glyphs_left + 1),
                    chars: Vec::with_capacity(chars_left + 1),
                    color,
                    para: Some(q),
                    x,
                });
            }
            let Some(r) = run.as_mut() else { continue };
            let scale = face.scale(st.size) * st.run.scale / 100.0;
            let mut gx = x;
            if p.soft_hyphen {
                if let Some(g) = face.glyph('-') {
                    r.glyphs.push((g, gx));
                    r.chars.push(('-', gx, first));
                }
                continue;
            }
            for (id, advance, dx) in sp.glyphs(o.from + ix) {
                r.glyphs.push((id, gx + dx * scale));
                gx += advance * scale;
            }
            let text = text_of(sp, p);
            for (j, c) in text.chars().enumerate() {
                r.chars.push((symbol_unicode(&face.family, c), x, first.map(|u| Unit(u.0 + j as u32))));
            }
            glyphs_left = glyphs_left.saturating_sub((p.glyphs.1 - p.glyphs.0) as usize);
            chars_left = chars_left.saturating_sub(text.len());
        }
        flush(&mut run, &mut out);
        draw_decorations(o, line, base, rects_end, &mut out);
        // The places left are at the line's end: its aligned start when it has no pieces.
        let end = match (line.xs.last(), line.widths.last()) {
            (Some(x), Some(w)) => x0 + x + w,
            _ => {
                let s = src.para_style_in(o.review.view);
                let left = s.ind_left + if o.from == 0 { s.ind_first } else { 0.0 };
                let right = o.width - s.ind_right;
                x0 + match s.jc {
                    Jc::Center => (left + right) / 2.0,
                    Jc::Right => right,
                    _ => left,
                }
            }
        };
        for (_, q) in places.by_ref() {
            out.extend(caret(end, q));
        }
        let changed = o.review.view.change_bars() && line_changed(o, line, src);
        LineFragment { items: out.into_boxed_slice(), pitch: line.pitch, baseline: line.baseline, atoms: atoms.into_boxed_slice(), changed }
    }
}

/// The paragraphs that draw no characters on a line but have their caret position on it, in order, each with the
/// line's piece it is before (`None`: the line's end). A line without pieces (an empty paragraph) has its paragraph's.
/// REV-6: a joined paragraph's paragraph that draws nothing at all (empty, or hidden by the view) has its place before
/// the next paragraph's first piece, or at the end of the last line.
fn caret_places(o: &LineOrigin, line: &Line) -> Vec<(Option<usize>, ParaId)> {
    let mut out = Vec::new();
    if let Some(j) = o.shaped.join.as_deref() {
        let all = &o.shaped.pieces;
        let here = o.from + line.start..o.from + line.end;
        for (q, start, end) in j.parts() {
            if all.iter().any(|p| p.src.is_some_and(|u| u >= start && end.is_none_or(|e| u < e))) {
                continue;
            }
            match all.iter().position(|p| p.src.is_some_and(|u| u >= start)) {
                Some(g) if here.contains(&g) => out.push((Some(g - here.start), q)),
                None if here.end >= all.len() => out.push((None, q)),
                _ => {}
            }
        }
    }
    if line.start == line.end && !out.iter().any(|e| e.1 == o.para) {
        out.push((None, o.para));
    }
    out
}

/// TAB-9: leader glyphs on a grid from the drawing origin's left edge, whole glyphs only.
fn draw_leader(o: &LineOrigin, line: &Line, k: usize, x0: f64, base: f64, out: &mut Vec<Item>) {
    {
        let sp = o.shaped;
        let p = &o.pieces()[line.start + k];
        let st = &sp.styles[p.style as usize];
        let Some(leader) = line.leaders.iter().find(|(j, _)| *j == k).and_then(|(_, l)| l.glyph()) else { return };
        let Some(face) = &st.face else { return };
        let Some(g) = face.glyph(leader) else { return };
        // TAB-9: the grid pitch is the advance rounded to 1/600 in.
        let w = (face.advance(g) * face.scale(st.size) * 600.0 / 72.0).round() * 72.0 / 600.0;
        if w <= 0.0 {
            return;
        }
        let from = x0 + line.xs[k];
        let to = from + line.widths[k];
        let mut slot = (from / w - 1e-9).ceil();
        let mut r = GlyphRunBuilder {
            face: face.clone(),
            size: st.size,
            y: base - st.raise,
            glyphs: Vec::new(),
            chars: Vec::new(),
            color: st.run.color,
            para: None,
            x: from,
        };
        while (slot + 1.0) * w <= to + 1e-6 {
            r.glyphs.push((g, slot * w));
            r.chars.push((leader, slot * w, None));
            slot += 1.0;
        }
        if !r.glyphs.is_empty() {
            out.push(Item::Glyphs(r.finish()));
        }
    }
}

/// A glyph run under construction.
struct GlyphRunBuilder {
    face: Rc<crate::font::Face>,
    size: f64,
    y: f64,
    glyphs: Vec<(u32, f64)>,
    chars: Vec<(char, f64, Option<Unit>)>,
    color: Option<u32>,
    para: Option<ParaId>,
    x: f64,
}

impl GlyphRunBuilder {
    fn finish(self) -> GlyphRun {
        GlyphRun {
            face: self.face,
            size: self.size,
            baseline: self.y,
            x: self.x,
            glyphs: self.glyphs.into_boxed_slice(),
            chars: self.chars.into_boxed_slice(),
            colour: self.color,
            para: self.para,
        }
    }
}

/// A line's highlight and run shading behind its text (inserted into `out` at `bg_at`), underlines and strikethrough
/// over it, as rectangles (FONT-15), with REV-3's revision marks in All markup.
fn draw_decorations(o: &LineOrigin, line: &Line, base: f64, bg_at: usize, out: &mut Vec<Item>) {
    let (sp, x0, review) = (o.shaped, o.x0, o.review);
    let pieces = &o.pieces()[line.start..line.end];
    let content = pieces.iter().rposition(|p| p.kind != Kind::Space).map_or(0, |k| k + 1);
    let (mut bg, mut fg) = (Vec::new(), Vec::new());
    // Pieces of one run draw one decoration: adjacent rectangles join.
    let push = |v: &mut Vec<Item>, r: [f64; 4], c: u32| {
        let joins = |it: &&mut Item| matches!(it, Item::Rect { rect: l, colour: lc } if *lc == c && (l[1] - r[1]).abs() < EPS && (l[3] - r[3]).abs() < EPS && (r[0] - l[2]).abs() < 0.01);
        match v.iter_mut().rev().take(4).find(joins) {
            Some(Item::Rect { rect: last, .. }) => last[2] = r[2],
            _ => v.push(Item::Rect { rect: r, colour: c }),
        }
    };
    for (k, p) in pieces.iter().enumerate() {
        let st = &sp.styles[p.style as usize];
        let Some(face) = st.face.as_ref().filter(|_| matches!(p.kind, Kind::Text | Kind::Space | Kind::ListNumber)) else { continue };
        let (r, label) = (&st.run, p.kind == Kind::ListNumber);
        let (x, w) = (x0 + line.xs[k], line.widths[k]);
        if w <= 0.0 {
            continue;
        }
        let em = face.scale(st.size);
        let y = base - st.raise;
        let fill = r.highlight.or(r.shading).filter(|_| !label);
        if let Some(fill) = fill {
            push(&mut bg, [x, y - face.ascent * em, x + w, y + face.descent * em], fill);
        }
        if k >= content {
            continue;
        }
        let ink = ink(r, review).unwrap_or(0);
        let line_at = |v: &mut Vec<Item>, y: f64, t: f64| push(v, [x, y - t / 2.0, x + w, y + t / 2.0], ink);
        // REV-3: in All markup an insertion is underlined (a move's destination twice) and a deletion struck through (a
        // move's source twice).
        let marked = review.view == MarkupView::All && !label;
        let kind = |m: &Option<Rc<crate::docx::review::RevisionMark>>| m.as_ref().filter(|_| marked).map(|m| m.kind);
        let (ins, del) = (kind(&r.rev.ins), kind(&r.rev.del));
        let rev_u = ins.map(|k| if k == MarkKind::MoveTo { "double" } else { "single" });
        if let Some(u) = rev_u.or(r.underline.as_deref()).filter(|u| !label && !(*u == "words" && p.kind == Kind::Space)) {
            let (pos, thick) =
                if face.underline_thick > 0.0 { (face.underline_pos, face.underline_thick) } else { (0.1 * face.upem, 0.05 * face.upem) };
            let (uy, t) = (y + pos * em, (thick * em).max(0.5));
            match u {
                "double" => {
                    line_at(&mut fg, uy, t);
                    line_at(&mut fg, uy + 2.0 * t, t);
                }
                "thick" => line_at(&mut fg, uy + t / 2.0, 2.0 * t),
                // shortcut: dotted, dashed and wavy underlines draw solid; dash them when someone needs the look.
                _ => line_at(&mut fg, uy, t),
            }
        }
        let dstrike = r.dstrike || del == Some(MarkKind::MoveFrom);
        if r.strike || dstrike || del.is_some() {
            let t = (face.strike_thick * em).max(0.5);
            let sy = y - face.strike_pos * em;
            if dstrike {
                line_at(&mut fg, sy - t, t);
                line_at(&mut fg, sy + t, t);
            } else {
                line_at(&mut fg, sy, t);
            }
        }
    }
    out.splice(bg_at..bg_at, bg);
    out.extend(fg);
}

/// Draws `text` in `run` ending at `x_end` on baseline `y` (LNUM-1 line numbers): one run per piece, all of one label;
/// nothing if it would start left of 0.
pub fn draw_label(text: &str, run: &Run, x_end: f64, y: f64, fonts: &Fonts) -> Vec<Item> {
    let sp = super::text::shape_run(text, run, fonts);
    let w: f64 = sp.pieces.iter().map(|p| p.width).sum();
    let mut x = x_end - w;
    let mut out = Vec::new();
    if x < 0.0 {
        return out;
    }
    for (i, p) in sp.pieces.iter().enumerate() {
        let st = &sp.styles[p.style as usize];
        let Some(face) = &st.face else { continue };
        let scale = face.scale(st.size);
        let mut r =
            GlyphRunBuilder { face: face.clone(), size: st.size, y, glyphs: Vec::new(), chars: Vec::new(), color: st.run.color, para: None, x };
        let mut gx = x;
        for (id, advance, dx) in sp.glyphs(i) {
            r.glyphs.push((id, gx + dx * scale));
            gx += advance * scale;
        }
        for c in text_of(&sp, p).chars() {
            r.chars.push((c, x, None));
        }
        out.push(Item::Glyphs(r.finish()));
        x += p.width;
    }
    out
}
