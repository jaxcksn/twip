//! Line breaking

use crate::docx::Compat;
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
use super::space::{BorderGroup, BorderKey, ParaSpace};
use super::text::{Kind, Object, Piece, ShapedPara, para_id, symbol_unicode};
use super::units::{LU_PER_PT, Lu};
use crate::docx::props::Border;

const EPS: f64 = 1e-6;

#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub start: usize,
    pub end: usize,
    pub xs: Vec<f64>,
    pub widths: Vec<f64>,
    pub ascent: f64,
    pub descent: f64,
    pub pitch: f64,
    pub baseline: f64,
    pub below: f64,
    pub hard: Option<Kind>,
    pub hyphen: bool,
    pub leaders: Vec<(usize, Leader)>,
    pub rects: Vec<([f64; 4], u32)>,
    /// How its runs sit on it (`w:textAlignment`): `baseline` is the reference baseline, each run's text its own
    /// [`align_drop`] below it.
    pub align: Align,
    /// It is a vertical line: its runs take an em/64 more ascent and descent.
    pub vertical: bool,
}

/// A line's `w:textAlignment`, `auto` resolved.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Align {
    #[default]
    Baseline,
    Top,
    Center,
    Bottom,
}

/// How far below a line's reference baseline a run's glyphs sit (points), in the run font's units with `A`, `D` its
/// win ascent and descent (an em/64 more each in a vertical line) and `h = A + D`: top `A − 1`, center
/// `A − h div 2`, bottom `−round(D × f / 1000)` (`f` 497 for TrueType outlines, else 500; an East Asian face 750,
/// else 1000), baseline 0. The run's ascent and descent on the line move with it.
pub fn align_drop(face: &crate::font::Face, size: f64, align: Align, vertical: bool) -> f64 {
    let e = if vertical { (face.upem / 64.0).round() as i64 } else { 0 };
    let (a, d) = ((face.ascent - face.ea_extra).round() as i64 + e, (face.descent - face.ea_extra).round() as i64 + e);
    let c = match align {
        Align::Baseline => return 0.0,
        Align::Top => a - 1,
        Align::Center => a - (a + d) / 2,
        Align::Bottom => {
            let f = match (face.east_asian, face.truetype) {
                (true, true) => 750,
                (true, false) => 1000,
                (false, true) => 497,
                (false, false) => 500,
            };
            -((d * f + 500) / 1000)
        }
    };
    Lu::from_font(c as f64, size, face.upem).to_pt()
}

/// [`align_drop`] for an inline picture: a run as tall as the picture (`a` above its baseline, `d` below), with no
/// line gap or vertical extra.
fn object_drop(a: f64, d: f64, align: Align) -> f64 {
    match align {
        Align::Baseline => 0.0,
        Align::Top => a - Lu(1).to_pt(),
        Align::Center => a - (a + d) / 2.0,
        // ponytail: half the depth, as for a font that isn't TrueType; a picture raised or lowered by its run is rare.
        Align::Bottom => -d / 2.0,
    }
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
            align: Align::Baseline,
            vertical: false,
        }
    }
}

#[derive(Clone, Copy)]
pub struct Frame {
    pub width: f64,
    pub default_tab: f64,
    pub compat: Compat,
    pub expand_shift_return: bool,
    pub split_page_break: bool,
    pub no_wrap: bool,
    pub tab_origin: f64,
    pub wp_justification: bool,
    pub line_grid: Option<i32>,
    /// Automatic hyphenation points may end this line.
    pub hyphenate: bool,
    /// The hyphenation zone.
    pub hyphen_zone: f64,
    /// The most consecutive hyphenated lines, 0 for no limit.
    pub hyphen_limit: i64,
    /// Where the last line that ended between letters ended: compat 15 hyphenates a word the line starts inside from
    /// there.
    pub word_from: Option<usize>,
    /// The line is vertical: its runs take an em/64 more ascent and descent.
    pub vertical: bool,
    /// Its runs are centred on the line (East Asian glyphs upright).
    pub centred: bool,
    /// East Asian line-breaking rules: breaks between East Asian characters, kinsoku classes.
    pub breaking_rules: bool,
    /// The strict kinsoku level.
    pub strict_kinsoku: bool,
    /// The hanging indent is a tab stop: for any tab, for a list number's tab.
    pub hang_stop: bool,
    pub number_hang_stop: bool,
}

impl Frame {
    pub fn new(space: &ParaSpace) -> Frame {
        let e = &space.env;
        Frame {
            width: space.width,
            default_tab: e.default_tab,
            compat: e.compat,
            expand_shift_return: !e.do_not_expand_shift_return,
            split_page_break: e.compat == Compat::Legacy && e.split_pg_break_and_para_mark,
            no_wrap: space.fit,
            tab_origin: 0.0,
            wp_justification: e.compat == Compat::Legacy && e.wp_justification,
            line_grid: e.line_grid,
            hyphenate: e.hyphenation.is_some(),
            hyphen_zone: e.hyphenation.as_ref().filter(|_| e.compat == Compat::Legacy).map_or(0.0, |h| h.zone),
            hyphen_limit: e.hyphenation.as_ref().map_or(0, |h| h.limit),
            word_from: None,
            vertical: e.vertical.is_some(),
            centred: e.vertical.is_some_and(|v| v.upright),
            breaking_rules: e.breaking_rules,
            strict_kinsoku: e.strict_kinsoku,
            hang_stop: !e.no_tab_hang_ind,
            number_hang_stop: !e.no_tab_hang_ind && !e.no_number_hang_stop,
        }
    }
}

#[derive(Debug)]
pub struct ParaLines {
    pub shaped: Rc<ShapedPara>,
    pub lines: Vec<Line>,
    pub from: usize,
    pub width: f64,
    pub x0: f64,
    pub para: ParaId,
    pub borders: (BorderKey, BorderKey),
    drawn: Box<[OnceCell<Rc<LineFragment>>]>,
    compat: Compat,
    review: ReviewView,
    ul_trail_space: bool,
    theme: std::rc::Rc<crate::drawing::theme::Theme>,
    colour_map: crate::drawing::theme::ColourMap,
    backdrop: Option<u32>,
}

impl PartialEq for ParaLines {
    fn eq(&self, o: &Self) -> bool {
        Rc::ptr_eq(&self.shaped, &o.shaped) && self.lines == o.lines && self.from == o.from && self.width == o.width && self.x0 == o.x0
    }
}

pub fn breaks_only(pieces: &[Piece], l: &Line) -> bool {
    l.hard == Some(Kind::PageBreak) && l.start < l.end && pieces[l.start..l.end].iter().all(|q| matches!(q.kind, Kind::LineBreak | Kind::PageBreak))
}

impl ParaLines {
    pub fn pieces(&self) -> &[Piece] {
        &self.shaped.pieces[self.from.min(self.shaped.pieces.len())..]
    }

    pub fn notes(&self, i: usize) -> Vec<(NoteKind, i64)> {
        let l = &self.lines[i];
        (l.start..l.end).filter_map(|k| self.shaped.note(self.from + k)).collect()
    }

    pub fn leading_page_break(&self, i: usize) -> bool {
        breaks_only(self.pieces(), &self.lines[i]) && self.lines[i + 1..].iter().any(|m| m.start < m.end)
    }

    pub fn fragment(&self, i: usize, para: &Paragraph, fonts: &Fonts) -> Rc<LineFragment> {
        self.drawn[i].get_or_init(|| Rc::new(draw_line(&self.origin(), &self.lines[i], para, fonts))).clone()
    }

    pub fn origin(&self) -> LineOrigin<'_> {
        LineOrigin {
            shaped: &self.shaped,
            from: self.from,
            width: self.width,
            x0: self.x0,
            compat: self.compat,
            para: self.para,
            review: &self.review,
            ul_trail_space: self.ul_trail_space,
            palette: crate::drawing::paint::Palette { theme: &self.theme, map: &self.colour_map },
            backdrop: self.backdrop,
        }
    }

    pub fn bytes(&self) -> usize {
        use std::mem::size_of;
        size_of::<ParaLines>() + self.lines.iter().map(|l| size_of::<Line>() + (l.xs.len() + l.widths.len()) * 8 + l.rects.len() * 40).sum::<usize>()
    }
}

#[derive(Debug)]
pub struct LinesKey {
    shaped: Rc<ShapedPara>,
    space: ParaSpace,
}

#[cfg(test)]
thread_local! {
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
    const PIECE: Option<crate::prof::Piece> = Some(crate::prof::Piece::Para);

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

pub fn break_lines(shaped: &Rc<ShapedPara>, para: &Paragraph, space: &ParaSpace, fonts: &Fonts, memos: &mut Memos) -> Result<Rc<ParaLines>, Yield> {
    let probe = LinesProbe { shaped, space };
    cached(memos, |c| &mut c.lines, probe, ParaLines::bytes, |m| break_lines_uncached(shaped, para, space, fonts, m))
}

pub fn break_lines_uncached(
    shaped: &Rc<ShapedPara>,
    para: &Paragraph,
    space: &ParaSpace,
    fonts: &Fonts,
    memos: &mut Memos,
) -> Result<ParaLines, Yield> {
    let from = space.start.min(shaped.pieces.len());
    let pieces = &shaped.pieces[from..];
    let opens = from == 0 || shaped.pieces[..from].iter().all(|q| matches!(q.kind, Kind::PageBreak | Kind::ColumnBreak));
    let frame = Frame::new(space);
    let view = space.env.review.view;
    let (style, mark) = (para.para_style_in(view), para.mark_in(view));
    let mut lines =
        super::job::unit("line breaking", || crate::prof::time("break", || break_paragraph(shaped, pieces, style, mark, &frame, fonts, from > 0)));
    let prev = if opens { space.borders.0.clone() } else { continued_key(style) };
    // A joined paragraph (compat ≤ 14): each line has the borders and shading of the paragraph its first character
    // is in, the top rule only above the first line and the bottom rule only below the last.
    let (mut a, n) = (0, lines.len());
    while a < n {
        let s = line_style(style, pieces, lines[a].start);
        let b = (a + 1..n).find(|&b| !std::ptr::eq(line_style(style, pieces, lines[b].start), s)).unwrap_or(n);
        let before = if a == 0 && opens { prev.clone() } else { continued_key(s) };
        let next = if b == n { space.borders.1.clone() } else { continued_key(s) };
        apply_borders(&mut lines[a..b], s, before.as_deref(), next.as_deref(), space.width, false, opens && a == 0, b == n);
        a = b;
    }
    memos.meter.unit_done();
    let n = lines.len();
    Ok(ParaLines {
        shaped: shaped.clone(),
        lines,
        from,
        width: space.width,
        x0: space.x0,
        para: para_id(para),
        borders: (prev, space.borders.1.clone()),
        drawn: (0..n).map(|_| OnceCell::new()).collect(),
        compat: space.env.compat,
        review: space.env.review.clone(),
        ul_trail_space: space.env.ul_trail_space,
        theme: space.env.theme.clone(),
        colour_map: space.env.colour_map,
        backdrop: space.env.backdrop,
    })
}

// Paragraph borders
pub fn border_space(b: &Border) -> f64 {
    f64::from((b.space.max(0.0) as u32) & 31)
}

fn masked(b: &Option<Border>) -> Option<Border> {
    b.as_ref().map(|b| Border { space: border_space(b), ..b.clone() })
}

pub fn border_key(s: &ParaStyle) -> BorderKey {
    let any = [&s.border_top, &s.border_left, &s.border_bottom, &s.border_right, &s.border_between].iter().any(|b| b.is_some());
    any.then(|| {
        Rc::new(BorderGroup {
            sides: [masked(&s.border_top), masked(&s.border_left), masked(&s.border_bottom), masked(&s.border_right)],
            between: masked(&s.border_between),
            edges: (s.ind_left + s.ind_first.min(0.0), s.ind_right),
            frame: s.frame.clone(),
            div: s.div,
            continued: false,
        })
    })
}

/// The key of a paragraph's part continued from the page or column before.
pub fn continued_key(s: &ParaStyle) -> BorderKey {
    border_key(s).map(|k| Rc::new(BorderGroup { continued: true, ..(*k).clone() }))
}

pub fn neighbour_borders(style: &ParaStyle, prev: Option<&ParaStyle>, next: Option<&ParaStyle>) -> (BorderKey, BorderKey) {
    match border_key(style) {
        None => (None, None),
        Some(_) => (prev.filter(|_| !style.page_break_before).and_then(border_key), next.filter(|n| !n.page_break_before).and_then(border_key)),
    }
}

fn joined(key: &BorderKey, other: Option<&BorderGroup>) -> bool {
    matches!((key, other), (Some(k), Some(o)) if k.joins(o))
}

fn shadowed(b: &Border) -> f64 {
    if b.shadow { super::table::thick(b) } else { 0.0 }
}

fn between_gap(prev: &BorderGroup) -> f64 {
    if prev.continued {
        return 0.0;
    }
    prev.between.as_ref().map_or(0.0, |b| super::table::thick(b) + b.space + shadowed(b))
}

pub fn top_border_extra(s: &ParaStyle, prev: Option<&BorderGroup>, first_part: bool) -> f64 {
    let key = border_key(s);
    let own = match (prev, &s.border_top) {
        (Some(p), _) if joined(&key, Some(p)) => between_gap(p),
        (_, Some(t)) => super::table::thick(t) + border_space(t) + shadowed(t),
        _ => 0.0,
    };
    own + if first_part { div_tops(s).1 } else { 0.0 }
}

fn div_tops(s: &ParaStyle) -> (Vec<f64>, f64) {
    let mut y = 0.0;
    let mut at = Vec::new();
    for d in s.div_boxes.iter() {
        at.push(y);
        if let (true, Some(t)) = (d.starts, &d.borders[0]) {
            y += super::table::thick(t) + border_space(t) + shadowed(t);
        }
    }
    (at, y)
}

#[allow(clippy::too_many_arguments, reason = "the paragraph part's place in its border group and its division boxes")]
pub fn apply_borders(
    lines: &mut [Line],
    s: &ParaStyle,
    prev: Option<&BorderGroup>,
    next: Option<&BorderGroup>,
    width: f64,
    top_pitched: bool,
    first_part: bool,
    last_part: bool,
) {
    let key = border_key(s);
    let n = lines.len();
    if n == 0 || (key.is_none() && s.shading.is_none() && s.div_boxes.is_empty()) {
        return;
    }
    let (div_at, div_top) = if first_part { div_tops(s) } else { (vec![0.0; s.div_boxes.len()], 0.0) };
    let lead =
        usize::from(first_part && n > 1 && matches!(lines[0].hard, Some(Kind::PageBreak | Kind::ColumnBreak)) && lines[0].end <= lines[0].start + 1);
    let lines = &mut lines[lead..];
    let n = lines.len();
    let same_prev = joined(&key, prev);
    let same_next = joined(&key, next);
    let thick = super::table::thick;
    let lx = s.border_left.as_ref().map_or(0.0, |b| border_space(b) + thick(b));
    let rx = s.border_right.as_ref().map_or(0.0, |b| border_space(b) + thick(b));
    let left = s.ind_left + s.ind_first.min(0.0);
    let (x0, x1) = (left - lx, width - s.ind_right + rx);
    // Each edge draws as a table's does: every border style, its stripes and tones.
    let edge = |rects: &mut Vec<([f64; 4], u32)>, b: &Border, horizontal: bool, far: bool, a: f64, from: f64, to: f64| {
        let w = thick(b);
        let r = if horizontal { [from, a, to, a + w] } else { [a, from, a + w, to] };
        let mut items = Vec::new();
        super::table::edge(&mut items, r, horizontal, far, b);
        rects.extend(items.into_iter().filter_map(|i| match i {
            Item::Rect { rect, colour } => Some((rect, colour)),
            _ => None,
        }));
    };
    let (mut shade_top, mut shade_bottom) = (0.0, None);
    let top_extra = top_border_extra(s, prev, first_part);
    let div_x = |d: &crate::docx::model::DivBox| {
        let side = |b: &Option<Border>| b.as_ref().map_or(0.0, |b| border_space(b) + thick(b));
        (d.left - side(&d.borders[1]), width - d.right + side(&d.borders[3]))
    };
    for (d, &y) in s.div_boxes.iter().zip(&div_at) {
        if let (true, true, Some(t)) = (first_part, d.starts, &d.borders[0]) {
            let (a, b) = div_x(d);
            edge(&mut lines[0].rects, t, true, false, y, a, b);
        }
    }
    match prev {
        Some(p) if same_prev => {
            if let Some(b) = p.between.as_ref().filter(|_| !p.continued) {
                let y = div_top + shadowed(b);
                edge(&mut lines[0].rects, b, true, false, y, x0, x1);
            }
        }
        _ => {
            if let Some(t) = &s.border_top {
                edge(&mut lines[0].rects, t, true, false, div_top + shadowed(t), x0, x1);
                shade_top = div_top + shadowed(t) + thick(t);
            }
        }
    }
    if !top_pitched && top_extra > 0.0 {
        let l = &mut lines[0];
        l.pitch += top_extra;
        l.baseline += top_extra;
    }
    if let (Some(b), false) = (&s.border_bottom, same_next) {
        let l = &mut lines[n - 1];
        let y = l.pitch + border_space(b);
        edge(&mut l.rects, b, true, true, y, x0, x1);
        shade_bottom = Some(y);
        l.pitch += border_space(b) + thick(b) + shadowed(b);
    }
    if let Some(b) = next.filter(|_| same_next && last_part).and_then(|k| k.between.as_ref()) {
        lines[n - 1].pitch += border_space(b);
    }
    let mut div_bottom = vec![None; s.div_boxes.len()];
    if last_part {
        let l = &mut lines[n - 1];
        for (k, d) in s.div_boxes.iter().enumerate().rev() {
            if let (true, Some(b)) = (d.ends, &d.borders[2]) {
                let y = l.pitch + border_space(b);
                let (a, c) = div_x(d);
                edge(&mut l.rects, b, true, true, y, a, c);
                l.pitch = y + thick(b) + shadowed(b);
                div_bottom[k] = Some(l.pitch);
            }
        }
    }
    for (i, l) in lines.iter_mut().enumerate() {
        let h = l.pitch;
        for (k, d) in s.div_boxes.iter().enumerate() {
            let (a, c) = div_x(d);
            let from = if i == 0 && first_part && d.starts { div_at[k] } else { 0.0 };
            let to = if i + 1 == n { div_bottom[k].unwrap_or(h) } else { h };
            if let Some(b) = &d.borders[1] {
                edge(&mut l.rects, b, false, false, a, from, to);
            }
            if let Some(b) = &d.borders[3] {
                edge(&mut l.rects, b, false, true, c - thick(b), from, to);
            }
        }
        if let Some(b) = &s.border_left {
            edge(&mut l.rects, b, false, false, x0, 0.0, h);
        }
        if let Some(b) = &s.border_right {
            edge(&mut l.rects, b, false, true, x1 - thick(b), 0.0, h);
        }
        if let Some(fill) = s.shading {
            let sx0 = s.border_left.as_ref().map_or(left, |b| x0 + thick(b));
            let sx1 = s.border_right.as_ref().map_or(width - s.ind_right, |b| x1 - thick(b));
            let top = if i == 0 { shade_top } else { 0.0 };
            let bottom = if i + 1 == n { shade_bottom.unwrap_or(h) } else { h };
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

/// The style a line of a joined paragraph ([`ParaStyle::joined`]) takes its indents, alignment and tabs from: that of
/// the paragraph its first character is in; a line starting at a paragraph's first character starts at the vanished
/// mark before it, so it is the earlier paragraph's.
fn line_style<'a>(style: &'a ParaStyle, pieces: &[Piece], start: usize) -> &'a ParaStyle {
    if style.joined.is_empty() {
        return style;
    }
    let u = pieces[start.min(pieces.len())..].iter().find_map(|p| p.src).map_or(0, |u| u.0);
    style.joined.iter().rev().find(|(at, _)| *at < u).map_or(style, |(_, s)| s)
}

pub fn break_paragraph(sp: &ShapedPara, pieces: &[Piece], style: &ParaStyle, mark: &Run, frame: &Frame, fonts: &Fonts, continued: bool) -> Vec<Line> {
    let mut lines = Vec::new();
    let line_size = style;
    let justify_smart = matches!(style.jc, Jc::Both | Jc::Distribute) && frame.compat == Compat::Generic && !frame.no_wrap;
    let mut start = 0;
    let n = pieces.len();
    let mut planned: Vec<usize> = Vec::new();
    if justify_smart {
        planned = plan_justified(pieces, style, frame, continued);
    }
    let with_tabs = justify_smart && pieces.iter().any(|p| p.kind == Kind::Tab);
    let mut first = !continued;
    let mut hyphens = 0;
    let mut word_from = None;
    let all = *frame;
    loop {
        let frame = &Frame { hyphenate: all.hyphenate && (all.hyphen_limit <= 0 || hyphens < all.hyphen_limit), word_from, ..all };
        let style = line_style(style, pieces, start);
        let right = frame.width - style.ind_right;
        let wrap_right = if frame.no_wrap { f64::INFINITY } else { right + wp_extension(style, frame) };
        let left = style.ind_left + if first { style.ind_first } else { 0.0 };
        let limit_end =
            if with_tabs { justified_tab_end(sp, pieces, start, left, right, style, frame) } else { planned.iter().copied().find(|&e| e > start) };
        let fit = fit_line(sp, pieces, start, left, wrap_right, style, frame, limit_end, false);
        let end = fit.end.max(start + 1).min(n.max(start + 1));
        let is_last = end >= n;
        hyphens = if fit.hyphen { hyphens + 1 } else { 0 };
        if !fit.hyphen {
            word_from = Some(end);
        }
        let mut line =
            Line { end: end.min(n), xs: fit.xs, widths: fit.widths, hard: fit.hard, hyphen: fit.hyphen, leaders: fit.leaders, ..Line::empty(start) };
        if !frame.no_wrap {
            align(&mut line, sp, pieces, style, frame, left, right, is_last);
        } else {
            fit_text(&mut line, pieces, right - 1.0);
        }
        size(&mut line, sp, pieces, line_size, mark, fonts, is_last, frame);
        lines.push(line);
        first = false;
        if end >= n {
            break;
        }
        start = end;
    }
    if let Some(last) = lines.last() {
        let trailing = last.end == n && matches!(last.hard, Some(Kind::ColumnBreak | Kind::LineBreak))
            || last.end == n && matches!(last.hard, Some(Kind::PageBreak)) && frame.split_page_break;
        if trailing {
            let mut line = Line::empty(n);
            size(&mut line, sp, pieces, style, mark, fonts, true, frame);
            lines.push(line);
        }
    }
    if lines.is_empty() || n == 0 {
        lines.clear();
        let mut line = Line::empty(0);
        size(&mut line, sp, pieces, style, mark, fonts, true, frame);
        lines.push(line);
    }
    lines
}

fn wp_extension(style: &ParaStyle, frame: &Frame) -> f64 {
    if !frame.wp_justification || style.jc != Jc::Both {
        return 0.0;
    }
    let w = ((frame.width - style.ind_left - style.ind_right) * 20.0).round().min(31680.0);
    (w * 562.0 / 14400.0).round() / 20.0
}

fn fit_text(line: &mut Line, pieces: &[Piece], fit: f64) {
    let n = line.xs.len();
    let content = (0..n)
        .rev()
        .find(|&k| !matches!(pieces[line.start + k].kind, Kind::Space | Kind::LineBreak | Kind::PageBreak | Kind::ColumnBreak | Kind::Anchor));
    let Some(last) = content else { return };
    if last == 0 {
        return;
    }
    let delta = fit - (line.xs[last] + line.widths[last]);
    for k in 1..n {
        line.xs[k] += delta * k.min(last) as f64 / last as f64;
    }
}

// Wrap bands
#[allow(
    clippy::too_many_arguments,
    reason = "the measured line-breaking helpers take the shaped paragraph, its pieces, style and frame separately, as they were ported"
)]
/// The floats a line's closing text-wrapping break clears: left, right.
pub fn clear_of(sp: &ShapedPara, pieces: &[Piece], line: &Line) -> Option<(bool, bool)> {
    let last = pieces.get(line.end.checked_sub(1)?).filter(|p| p.kind == Kind::LineBreak && line.hard == Some(Kind::LineBreak))?;
    match &sp.objects[last.object? as usize] {
        Object::Clear(l, r) => Some((*l, *r)),
        _ => None,
    }
}

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
    let (line_size, style) = (style, line_style(style, pieces, start));
    let n = pieces.len();
    let right_edge = frame.width - style.ind_right;
    let mut segs: Vec<Line> = Vec::new();
    let mut s = start;
    let mut gap = None;
    let mut word_from = frame.word_from;
    for &(a, b) in intervals {
        if s >= n && !segs.is_empty() {
            break;
        }
        let first = if first_line && segs.is_empty() { style.ind_first } else { 0.0 };
        let left = if a > EPS { a.max(style.ind_left) + first } else { a.max(style.ind_left + first) };
        let right = if b >= frame.width - EPS { right_edge } else { b.min(right_edge) };
        if right - left <= EPS {
            continue;
        }
        gap.get_or_insert(right - left);
        let piece_frame = Frame { tab_origin: if frame.compat == Compat::Legacy { a } else { 0.0 }, word_from, ..*frame };
        let frame = &piece_frame;
        let planned = if matches!(style.jc, Jc::Both | Jc::Distribute) && frame.compat == Compat::Generic && s < n {
            let seg_end = segment_end(pieces, s);
            if !pieces[s..seg_end].iter().any(|p| p.kind == Kind::Tab) {
                match justified_line(pieces, s, seg_end, right - left) {
                    LineChoice::End(e) => Some(e),
                    _ => None,
                }
            } else if a <= EPS && b >= frame.width - EPS {
                justified_tab_end(sp, pieces, s, left, right, style, frame)
            } else {
                None
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
        if !line.hyphen {
            word_from = Some(s);
        }
        let hard = line.hard.is_some_and(|k| k != Kind::LineBreak);
        let cleared = match clear_of(sp, pieces, &line) {
            Some((l, r)) => {
                let (a, b) = if frame.compat == Compat::Legacy { intervals[0] } else { (a, b) };
                (!l || a <= EPS) && (!r || b >= frame.width - EPS)
            }
            None => true,
        };
        segs.push(line);
        if hard || !cleared || s >= n {
            break;
        }
    }
    let own = |k: usize| pieces[k].ascent + pieces[k].descent;
    if segs.is_empty() {
        let mut x = 0.0;
        let k = (start..n).filter(|&k| pieces[k].kind != Kind::Anchor).find(|&k| {
            x += pieces[k].width;
            x > gap.unwrap_or(0.0) + EPS
        });
        return Err(k.or((start..n).find(|&k| pieces[k].kind != Kind::Anchor)).map_or(0.0, own));
    }
    if !force
        && frame.compat == Compat::Legacy
        && s < n
        && matches!(pieces[s].kind, Kind::Text | Kind::Object)
        && segs.last().is_some_and(|l| l.hard.is_none())
        && pieces[start..s].iter().all(|q| matches!(q.kind, Kind::Tab | Kind::Anchor))
    {
        let placed = start..s;
        let a = placed.clone().map(|k| pieces[k].ascent).fold(0.0, f64::max);
        return Err(a + placed.map(|k| pieces[k].descent).fold(0.0, f64::max));
    }
    let mut merged = segs[0].clone();
    merged.end = s;
    size(&mut merged, sp, pieces, line_size, mark, fonts, s >= n, frame);
    for l in &mut segs {
        l.ascent = merged.ascent;
        l.descent = merged.descent;
        l.pitch = merged.pitch;
        l.baseline = merged.baseline;
        l.below = merged.below;
        (l.align, l.vertical) = (merged.align, merged.vertical);
    }
    Ok((segs, s))
}

// Tabs
fn ptab_stop(t: &crate::docx::PTab, style: &ParaStyle, frame: &Frame) -> (f64, TabKind, Leader) {
    let (l, r) = if t.margin { (0.0, frame.width) } else { (style.ind_left, frame.width - style.ind_right) };
    let pos = match t.align {
        TabKind::Center => (l + r) / 2.0,
        TabKind::Right => r,
        _ => l,
    };
    (pos, t.align, t.leader)
}

/// The stop a tab at `pen` goes to; `number`: the tab after a list number.
fn next_stop(pen: f64, style: &ParaStyle, frame: &Frame, number: bool) -> (f64, TabKind, Leader) {
    let o = frame.tab_origin;
    let (stop, kind, leader) = stop_from(pen - o, style, frame, if number { frame.number_hang_stop } else { frame.hang_stop });
    (stop + o, kind, leader)
}

fn stop_from(pen: f64, style: &ParaStyle, frame: &Frame, hang_stop: bool) -> (f64, TabKind, Leader) {
    let custom = style.tabs.iter().find(|t| t.kind != TabKind::Clear && t.kind != TabKind::Bar && t.pos > pen + EPS);
    let (mut stop, mut kind, mut leader) = match custom {
        Some(t) => (t.pos, t.kind, t.leader),
        None => {
            let inc = frame.default_tab.max(1.0);
            let q = pen / inc;
            let on_negative = pen < -EPS && (q - q.round()).abs() * inc < EPS;
            let s = if frame.compat == Compat::Generic && on_negative { q.round() * inc } else { (q.floor() + 1.0) * inc };
            (s, TabKind::Left, Leader::None)
        }
    };
    if hang_stop && pen < style.ind_left - EPS && (stop > style.ind_left + EPS || custom.is_none()) {
        stop = style.ind_left;
        kind = TabKind::Left;
        leader = Leader::None;
    }
    if kind == TabKind::Num {
        kind = TabKind::Left;
    }
    (stop, kind, leader)
}

struct Pending {
    idx: usize,
    stop: f64,
    kind: TabKind,
    pen_at_tab: f64,
}

/// An East Asian language's kinsoku classes: what doesn't end a line, what doesn't start one (standard and strict
/// level), and what may hang past the right margin.
struct Kinsoku {
    no_end: &'static str,
    no_start: &'static str,
    no_start_strict: &'static str,
    hanging: &'static str,
}

const JAPANESE: Kinsoku = Kinsoku {
    no_end: "$([\\{£¥‘“〈《「『【〔＄（［｛｢￡￥",
    no_start: "!%),.:;?]}¢°’”‰′″℃、。々〉》」』】〕゛゜ゝゞ・ヽヾ！％），．：；？］｝｡｣､･ﾞﾟ￠",
    no_start_strict: "!%),.:;?]}¢°’”‰′″℃、。々〉》」』】〕ぁぃぅぇぉっゃゅょゎ゛゜ゝゞァィゥェォッャュョヮヵヶ・ーヽヾ！％），．：；？］｝｡｣､･ｧｨｩｪｫｬｭｮｯｰﾞﾟ￠",
    hanging: ",.’”、。」』】），．］｝｡､",
};

const SIMPLIFIED_CHINESE: Kinsoku = Kinsoku {
    no_end: "$([{£¥·‘“〈《「『【〔〖〝﹙﹛﹝＄（．［｛￡￥",
    no_start: "!%),.:;>?]}¢¨°·ˇˉ―‖’”…‰′″›℃∶、。〃〉》」』】〕〗〞︶︺︾﹀﹄﹚﹜﹞！＂％＇），．：；？］｀｜｝～￠",
    no_start_strict: "!%),.:;>?]}¢¨°·ˇˉ―‖’”…‰′″›℃∶、。〃〉》」』】〕〗〞︶︺︾﹀﹄﹚﹜﹞！＂％＇），．：；？］｀｜｝～￠",
    hanging: "!%),.:;>?]}¢°·ˇ’”‰′″℃∶、。〃〉》」』】〕〗〞﹚﹜﹞！＂％＇），．：；？］｝￠",
};

const TRADITIONAL_CHINESE: Kinsoku = Kinsoku {
    no_end: "([{£¥‘“‵〈《「『【〔〝︵︷︹︻︽︿﹁﹃﹙﹛﹝（｛",
    no_start: "!),.:;?]}¢·–—’”•‥…‧′╴、。〉》」』】〕〞︰︱︳︴︶︸︺︼︾﹀﹂﹄﹏﹐﹑﹒﹔﹕﹖﹗﹚﹜﹞！），．：；？］｜｝､",
    no_start_strict: "!),.:;?]}¢·–—’”•‥…‧′╴、。〉》」』】〕〞︰︱︳︴︶︸︺︼︾﹀﹂﹄﹏﹐﹑﹒﹔﹕﹖﹗﹚﹜﹞！），．：；？］｜｝､",
    hanging: "!),.:;?]}’”′、。〉》」』】〕〞﹚﹜﹞！），．：；？］｝",
};

const KOREAN: Kinsoku = Kinsoku {
    no_end: "$([\\{£¥‘“〈《「『【〔＄（［｛￡￥￦",
    no_start: "!%),.:;?]}¢°’”′″℃〉》」』】〕！％），．：；？］｝￠",
    no_start_strict: "!%),.:;?]}¢°’”′″℃〉》」』】〕！％），．：；？］｝￠",
    hanging: "!%),.:;?]}¢°’”′″℃〉》」』】〕！％），．：；？］｝￠",
};

/// The kinsoku classes of a run's East Asian language (`w:lang w:eastAsia`); none for other languages.
fn kinsoku(lang: &str) -> Option<&'static Kinsoku> {
    let is = |l: &str| lang.eq_ignore_ascii_case(l);
    if is("ko-KR") {
        Some(&KOREAN)
    } else if is("ja-JP") {
        Some(&JAPANESE)
    } else if is("zh-CN") || is("ii-CN") {
        Some(&SIMPLIFIED_CHINESE)
    } else if is("zh-TW") {
        Some(&TRADITIONAL_CHINESE)
    } else {
        None
    }
}

fn hangul(c: char) -> bool {
    matches!(c as u32, 0x1100..=0x11FF | 0x3130..=0x318F | 0xA960..=0xA97F | 0xAC00..=0xD7FF)
}

fn text_of<'a>(sp: &'a ShapedPara, p: &Piece) -> &'a str {
    &sp.text[p.text.0 as usize..p.text.1 as usize]
}

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
                if t.contains('.') {
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
    if p.pen_at_tab + tw + w - trail > right + EPS && p.stop <= right + EPS {
        tw = if p.kind == TabKind::Decimal { 0.0 } else { right - (p.pen_at_tab + w) + trail };
    }
    let tw = tw.max(0.0);
    ws[p.idx - start] = tw;
    let mut x = p.pen_at_tab + tw;
    for &k in &seg {
        xs[k - start] = x;
        x += ws[k - start];
    }
    x
}

// Greedy fit
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
    // How much wider the pieces so far count for breaking than they are drawn.
    let mut drift = 0.0;
    if start == 0 && style.list_jc != "left" {
        let w: f64 = pieces.iter().take_while(|p| p.kind == Kind::ListNumber).map(|p| p.width).sum();
        pen -= if style.list_jc == "right" { w } else { w / 2.0 };
    }
    let mut pending: Option<Pending> = None;
    let mut breaks: Vec<(usize, bool)> = Vec::new();
    // A line that starts inside a word hyphenates the rest of it as a word: from the line start in compat ≤ 14, from
    // where it was last broken between letters in compat 15. `pieces` ends the paragraph's pieces.
    let n = pieces.len();
    let (all, o) = (&sp.pieces[..], sp.pieces.len() - n);
    let in_word = |k: usize| k > 0 && k < all.len() && all[k].kind == Kind::Text && all[k - 1].kind == Kind::Text && !all[k - 1].break_after;
    let at = o + start;
    let word_start = (0..=at).rev().find(|&k| !in_word(k)).unwrap_or(0);
    // A word continued from an earlier page counts as broken at a hyphen when it was broken at a point.
    let broke = frame.word_from.map(|w| w + o).unwrap_or(if word_start < o && !all[o - 1].auto_hyphen { o } else { 0 });
    let from = if frame.compat == Compat::Legacy { at } else { broke.clamp(word_start, at) };
    let part = (frame.hyphenate && from > word_start).then(|| {
        let (e, at) = sp.points_from(all, from);
        (e - o, at.into_iter().map(|k| k - o).collect::<Vec<_>>())
    });
    let mut leaders: Vec<(usize, Leader)> = Vec::new();
    let mut squeeze: Option<(usize, f64)> = None;
    let mut past_custom = false;
    // East Asian paragraph layout: distributed, or the document's default East Asian font is an East Asian one.
    let east_asian = style.jc == Jc::Distribute || sp.east_asian_default;
    let rules = frame.breaking_rules || east_asian;
    let classes = |k: usize| kinsoku(&sp.styles[pieces[k].style as usize].run.lang_east_asia).filter(|_| rules && !style.no_kinsoku);
    let first_char = |k: usize| text_of(sp, &pieces[k]).chars().next();
    let last_char = |k: usize| text_of(sp, &pieces[k]).chars().last();
    // A break at `at` (after any spaces) is refused when the last character before it doesn't end a line or the
    // next one doesn't start one.
    let refused = |at: usize| {
        let no_start = |t: &Kinsoku| if frame.strict_kinsoku { t.no_start_strict } else { t.no_start };
        (start..at)
            .rev()
            .find(|&k| pieces[k].kind != Kind::Space)
            .is_some_and(|k| classes(k).is_some_and(|t| last_char(k).is_some_and(|c| t.no_end.contains(c))))
            || (at..n)
                .find(|&k| pieces[k].kind != Kind::Space)
                .is_some_and(|k| classes(k).is_some_and(|t| first_char(k).is_some_and(|c| no_start(t).contains(c))))
    };
    // Under the breaking rules a line may break between two characters unless both wrap as words: Hangul and every
    // character of a Korean run with `w:wordWrap` on (its punctuation too: such a run breaks only at spaces), other
    // text outside the East Asian characters only with it on.
    let korean_run = |k: usize| sp.styles[pieces[k].style as usize].run.lang_east_asia.eq_ignore_ascii_case("ko-KR");
    let wraps = |k: usize, c: char| {
        let east = super::text::is_east_asian(c as u32);
        if hangul(c) || korean_run(k) { !style.no_word_wrap || !east && c.is_alphanumeric() } else { !style.no_word_wrap && !east }
    };
    let between = |k: usize| {
        rules
            && k + 1 < n
            && pieces[k].kind == Kind::Text
            && pieces[k + 1].kind == Kind::Text
            && !pieces[k + 1].glued
            && !(last_char(k).is_some_and(|c| wraps(k, c)) && first_char(k + 1).is_some_and(|c| wraps(k + 1, c)))
    };
    // topLinePunct: an opening bracket that starts a line keeps only its right half; an opening quote too when it is
    // in the run's East Asian font (`w:hint="eastAsia"`).
    let hinted = |k: usize| sp.styles[pieces[k].style as usize].run.hint_east_asia;
    if east_asian
        && style.top_line_punct
        && pieces.get(start).is_some_and(|p| p.kind == Kind::Text)
        && first_char(start).is_some_and(|c| "〈《「『【〔（［｛".contains(c) || "‘“".contains(c) && hinted(start))
    {
        pen -= pieces[start].width / 2.0;
    }
    // Punctuation that may hang past the right margin, in East Asian paragraphs; compat 15 hangs none.
    let hangs = |k: usize| {
        east_asian
            && frame.compat == Compat::Legacy
            && !style.no_overflow_punct
            && !style.in_frame
            && classes(k).is_some_and(|t| first_char(k).is_some_and(|c| t.hanging.contains(c)))
    };
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
                let mut end = i + 1;
                if p.kind == Kind::PageBreak && !frame.split_page_break && end < n && pieces[end..].iter().all(|q| q.kind == Kind::Anchor) {
                    for _ in end..n {
                        xs.push(pen);
                        ws.push(0.0);
                    }
                    end = n;
                }
                return Fit { end, xs, widths: ws, hard: Some(p.kind), hyphen: false, leaders };
            }
            Kind::Tab => {
                if let Some(pt) = pending.take() {
                    pen = resolve_tab(sp, &pt, pieces, &mut xs, &mut ws, start, i, right);
                }
                let (mut stop, mut kind, leader) = match p.object.map(|o| &sp.objects[o as usize]) {
                    Some(Object::PTab(t)) => ptab_stop(t, style, frame),
                    _ => next_stop(pen, style, frame, i > 0 && pieces[..i].iter().all(|q| q.kind == Kind::ListNumber)),
                };
                let stop_custom = style.tabs.iter().any(|t| t.kind != TabKind::Clear && t.kind != TabKind::Bar && t.pos > pen + EPS);
                let custom = p.object.is_some() || stop_custom;
                if !custom && stop > right + EPS && !past_custom && i > start && limit_end.is_none() {
                    return Fit { end: i, xs, widths: ws, hard: None, hyphen: false, leaders };
                }
                let edge = frame.width - style.ind_right;
                if frame.compat == Compat::Generic && p.object.is_none() && stop_custom && stop > edge {
                    if kind != TabKind::Left && !pieces[i + 1..].iter().any(|q| q.kind == Kind::Tab) {
                        stop = edge;
                        if kind == TabKind::Center {
                            kind = TabKind::Right;
                        }
                    } else if i > start && limit_end.is_none() {
                        return Fit { end: i, xs, widths: ws, hard: None, hyphen: false, leaders };
                    } else {
                        stop = edge;
                        kind = TabKind::Left;
                    }
                }
                past_custom |= frame.compat == Compat::Legacy && custom && stop > right + EPS;
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
                let ink = (start..i).rev().find(|&j| pieces[j].kind != Kind::Space);
                if p.kind == Kind::Space
                    && !p.glued
                    && frame.compat == Compat::Legacy
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
                drift += p.break_extra;
                if p.break_after && !pieces[start..=i].iter().all(|q| q.kind == Kind::Anchor) && !refused(i + 1) {
                    breaks.push((i + 1, false));
                }
                i += 1;
                continue;
            }
            _ => {}
        }
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
        } + drift
            + p.break_extra;
        let forced = limit_end.is_some();
        let limit = match &pending {
            Some(pt) if pt.kind == TabKind::Right && pt.stop > right => pt.stop,
            _ if past_custom => f64::INFINITY,
            _ => right,
        };
        let over = right_edge > limit + EPS && !forced && p.kind != Kind::ListNumber && !(pen <= limit + EPS && pending.is_none() && hangs(i));
        let word = breaks.last().map_or(start, |b| b.0);
        let room: f64 = match squeeze {
            _ if !over => 0.0,
            Some((w, cap)) if w == word => cap,
            None if frame.compat == Compat::Legacy && word > start && pending.is_none() => {
                (squeeze_from(pieces, start, word)..word).map(|k| shrink(sp, pieces, k)).sum()
            }
            _ => 0.0,
        };
        if over && right_edge <= limit + room + EPS {
            squeeze = Some((word, room));
        } else if over {
            // In compat ≤ 14 an automatic point needs three letters of the word to fit and its last letter before the
            // last one that fits; when its hyphen doesn't fit, the next point must end before that letter.
            let mut exceed = i;
            while let Some(&(at, soft)) = breaks.last() {
                if at <= start {
                    breaks.pop();
                    continue;
                }
                if soft {
                    let auto = !pieces[at - 1].soft_hyphen;
                    let hy = hyphen_width(sp, &pieces[at - 1]);
                    let pen_at = xs[at - 1 - start] + if auto { ws[at - 1 - start] } else { 0.0 };
                    let fits = pen_at + hy <= right + EPS;
                    let legacy = auto && frame.compat == Compat::Legacy;
                    if legacy {
                        let word = (start..at).rev().find(|&k| pieces[k].kind != Kind::Text).map_or(start, |k| k + 1);
                        if exceed <= word + 2 || at + 1 > exceed {
                            breaks.pop();
                            continue;
                        }
                        if !fits {
                            exceed = at - 1;
                        }
                    }
                    if !fits || auto && !zoned(pieces, start, at, &xs, right, frame) {
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
            if strict {
                return Fit { end: start, xs: Vec::new(), widths: Vec::new(), hard: None, hyphen: false, leaders: Vec::new() };
            }
            // Inside a fitted region the break goes before the region, unless the region starts the line. In a
            // Korean run the cut ignores the kinsoku classes and spaces that don't fit go down with the rest.
            let mut at = (start..=i).rev().find(|&k| !pieces[k].glued).unwrap_or(start);
            if korean_run(at.min(n - 1)) {
                while at > start + 1 && pieces[at - 1].kind == Kind::Space && xs[at - 1 - start] + ws[at - 1 - start] > right + EPS {
                    at -= 1;
                }
            } else {
                while at > start + 1 && refused(at) {
                    at -= 1;
                }
            }
            if at > start && pieces[start..at].iter().any(|q| !matches!(q.kind, Kind::ListNumber | Kind::Anchor)) {
                xs.truncate(at - start);
                ws.truncate(at - start);
                leaders.retain(|(k, _)| *k < at - start);
                if let Some(pt) = pending.take().filter(|pt| pt.idx < at) {
                    resolve_tab(sp, &pt, pieces, &mut xs, &mut ws, start, at, right);
                }
                return Fit { end: at, xs, widths: ws, hard: None, hyphen: false, leaders };
            }
        }
        xs.push(pen);
        ws.push(p.width);
        pen += p.width;
        drift += p.break_extra;
        if p.break_after {
            breaks.push((i + 1, p.soft_hyphen));
        } else if frame.hyphenate
            && match &part {
                Some((e, at)) if i < *e => !p.glued && at.contains(&i),
                _ => p.auto_hyphen,
            }
        {
            breaks.push((i + 1, true));
        } else if between(i) && !refused(i + 1) {
            breaks.push((i + 1, false));
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

// Hyphenation zone
fn zoned(pieces: &[Piece], start: usize, at: usize, xs: &[f64], right: f64, frame: &Frame) -> bool {
    if frame.hyphen_zone <= 0.0 {
        return true;
    }
    // Measured from the word's first letter; a word that starts the line isn't tested.
    let word = (start..at).rev().find(|&k| pieces[k].kind != Kind::Text).map_or(start, |k| k + 1);
    word == start || right - xs[word - start] >= frame.hyphen_zone - EPS
}

// Space squeeze
fn squeeze_from(pieces: &[Piece], start: usize, word: usize) -> usize {
    pieces[start..word].iter().rposition(|q| q.kind == Kind::Tab).map_or(start, |t| start + t + 1)
}

fn shrink(sp: &ShapedPara, pieces: &[Piece], k: usize) -> f64 {
    let p = &pieces[k];
    let st = &sp.styles[p.style as usize];
    let space = |j: Option<usize>| j.and_then(|j| pieces.get(j)).is_some_and(|q| q.kind == Kind::Space);
    if p.kind != Kind::Space || st.squeeze_avg == 0.0 || text_of(sp, p) != " " || space(k.checked_sub(1)) || space(Some(k + 1)) {
        return 0.0;
    }
    (p.width - st.squeeze_avg * st.size * st.run.scale / 100.0 / 2.0).max(0.0)
}

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

// Justified line ends
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

#[allow(clippy::too_many_arguments, reason = "the same inputs as fit_line, which it runs per end")]
fn justified_tab_end(sp: &ShapedPara, pieces: &[Piece], start: usize, left: f64, right: f64, style: &ParaStyle, frame: &Frame) -> Option<usize> {
    if pieces.iter().any(|p| p.kind == Kind::Tab && p.object.is_some()) {
        return None;
    }
    let greedy = fit_line(sp, pieces, start, left, right, style, frame, None, false);
    let seg_end = segment_end(pieces, start);
    if greedy.hard.is_some() || greedy.end >= seg_end {
        return None;
    }
    let lu = |x: f64| x * LU_PER_PT;
    let end_at = |e: usize| -> (f64, i64, i64) {
        let all = fit_line(sp, pieces, start, left, right, style, frame, Some(e), false);
        let ink = (start..e).rev().find(|&k| !matches!(pieces[k].kind, Kind::Space | Kind::Anchor));
        let x = ink.map_or(left, |k| all.xs[k - start] + all.widths[k - start])
            + if pieces[e - 1].soft_hyphen { hyphen_width(sp, &pieces[e - 1]) } else { 0.0 };
        let from = (start..e).rev().find(|&k| pieces[k].kind == Kind::Tab).map_or(start, |t| t + 1);
        let first = (from..e).find(|&k| pieces[k].kind != Kind::Space).unwrap_or(e);
        let last = ink.map_or(first, |k| k + 1);
        let (mut shrink, mut stretch) = (0, 0);
        for k in (first..last).filter(|&k| pieces[k].kind == Kind::Space) {
            let w = lu(all.widths[k - start]).round() as i64;
            shrink += (w + 2) >> 2;
            stretch += (w + 1) >> 1;
        }
        (lu(x) - lu(right), shrink, stretch)
    };
    let is_end = |e: usize| (pieces[e - 1].break_after || pieces[e - 1].kind == Kind::Tab) && pieces[e].kind != Kind::Space;
    let is_end = |e: usize| e == seg_end || is_end(e);
    let longest = (greedy.end + 1..=seg_end)
        .filter(|&e| is_end(e))
        .take_while(|&e| !pieces[greedy.end..e].iter().any(|p| p.kind == Kind::Tab))
        .take_while(|&e| {
            let (over, shrink, _) = end_at(e);
            over <= shrink as f64 + LU_EPS
        })
        .last()?;
    let demerits = |e: usize| {
        let (over, shrink, stretch) = end_at(e);
        end_demerits(over, shrink, stretch)
    };
    let mut best = longest;
    let mut best_cost = demerits(longest);
    for e in (greedy.end..longest).rev().filter(|&e| e == greedy.end || is_end(e)) {
        let c = demerits(e);
        if c.is_some_and(|c| best_cost.is_none_or(|b| c < b)) {
            best = e;
            best_cost = c;
        }
    }
    (best != greedy.end).then_some(best)
}

fn segment_end(pieces: &[Piece], from: usize) -> usize {
    pieces[from..]
        .iter()
        .position(|p| matches!(p.kind, Kind::LineBreak | Kind::PageBreak | Kind::ColumnBreak))
        .map(|k| from + k + 1)
        .unwrap_or(pieces.len())
}

enum LineChoice {
    Empty,
    Last,
    Abort,
    End(usize),
}

#[derive(Clone, Copy, Default)]
struct Gap {
    width: f64,
    shrink: i64,
    stretch: i64,
}

const LU_EPS: f64 = 1e-6;

fn end_demerits(over: f64, shrink: i64, stretch: i64) -> Option<i64> {
    let (used, cap) = if over > LU_EPS { (over, shrink) } else { (-over.min(0.0), stretch) };
    if cap <= 0 {
        return None;
    }
    let l = (1000.0 * used / cap as f64).floor() as i64;
    (l <= 4700).then(|| (1000 + l * l * l / 100_000).pow(2))
}

fn justified_line(pieces: &[Piece], from: usize, seg_end: usize, column: f64) -> LineChoice {
    let mut words: Vec<(usize, usize, f64)> = Vec::new();
    let mut gaps: Vec<Gap> = Vec::new();
    let mut k = from;
    let mut gap = Gap::default();
    let mut joined = false;
    while k < seg_end {
        let p = &pieces[k];
        if matches!(p.kind, Kind::Space) {
            let w = (p.width * LU_PER_PT).round() as i64;
            gap.width += p.width + p.break_extra;
            gap.shrink += (w + 2) >> 2;
            gap.stretch += (w + 1) >> 1;
            joined |= !p.break_after;
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
            w += pieces[k].width + pieces[k].break_extra;
            k += 1;
            if pieces[k - 1].break_after && !pieces[k - 1].soft_hyphen {
                break;
            }
        }
        if joined && let (Some(last), Some(g)) = (words.last_mut(), gaps.last_mut()) {
            last.1 = k;
            last.2 += gap.width + w;
            g.shrink += gap.shrink;
            g.stretch += gap.stretch;
        } else {
            gaps.push(if words.is_empty() { Gap { width: gap.width, ..Gap::default() } } else { gap });
            words.push((ws, k, w));
        }
        gap = Gap::default();
        joined = false;
    }
    if words.is_empty() {
        return LineChoice::Empty;
    }
    let column = column * LU_PER_PT;
    let natural = |e: usize| -> f64 { (0..e).map(|j| (words[j].2 + gaps[j].width) * LU_PER_PT).sum() };
    let shrink = |e: usize| -> i64 { gaps[1..e].iter().map(|g| g.shrink).sum() };
    let stretch = |e: usize| -> i64 { gaps[1..e].iter().map(|g| g.stretch).sum() };
    let fits = |e: usize| natural(e) <= column + LU_EPS;
    if fits(words.len()) {
        return LineChoice::Last;
    }
    let longest = (1..=words.len()).take_while(|&e| natural(e) - column <= shrink(e) as f64 + LU_EPS).last().unwrap_or(0);
    let greedy = (1..=words.len()).take_while(|&e| fits(e)).last().unwrap_or(0);
    if longest == 0 {
        return LineChoice::Abort;
    }
    let demerits = |e: usize| end_demerits(natural(e) - column, shrink(e), stretch(e));
    let mut best = longest;
    let mut best_cost = demerits(longest);
    for e in (greedy.max(1)..longest).rev() {
        let c = demerits(e);
        if c.is_some_and(|c| best_cost.is_none_or(|b| c < b)) {
            best = e;
            best_cost = c;
        }
    }
    let mut end = words[best - 1].1;
    while end < seg_end && pieces[end].kind == Kind::Space {
        end += 1;
    }
    LineChoice::End(end)
}

// Alignment
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
    let mut content_end = n;
    while content_end > 0
        && matches!(pieces[idx(content_end - 1)].kind, Kind::Space | Kind::LineBreak | Kind::PageBreak | Kind::ColumnBreak | Kind::Anchor)
    {
        content_end -= 1;
    }
    let line_end_x = if content_end == 0 { left } else { line.xs[content_end - 1] + line.widths[content_end - 1] }
        + if line.hyphen { hyphen_width(sp, &pieces[idx(n - 1)]) } else { 0.0 };
    let ends_with_line_break = line.end > 0 && pieces.get(line.end - 1).is_some_and(|p| p.kind == Kind::LineBreak);
    let jc = match style.jc {
        Jc::Left if style.bidi => Jc::Right,
        Jc::Right if style.bidi => Jc::Left,
        jc => jc,
    };
    let to_right = |line: &mut Line| {
        let free = right - line_end_x;
        if free > 0.0 {
            for x in &mut line.xs {
                *x += free;
            }
        }
    };
    match jc {
        Jc::Both => {
            let justify = !is_last && (line.hard.is_none() || line.hard == Some(Kind::LineBreak));
            let justify = justify && !(ends_with_line_break && (frame.compat == Compat::Legacy || !frame.expand_shift_return));
            let overflow = line_end_x - right;
            if !(justify || (frame.compat == Compat::Generic && overflow > EPS)) {
                if style.bidi {
                    to_right(line);
                }
                return;
            }
            let last_tab = (0..content_end).rev().find(|&k| pieces[idx(k)].kind == Kind::Tab).map(|k| k + 1).unwrap_or(0);
            let mut first = last_tab;
            while first < content_end && pieces[idx(first)].kind == Kind::Space {
                first += 1;
            }
            let spaces: Vec<usize> = (first..content_end).filter(|&k| pieces[idx(k)].kind == Kind::Space).collect();
            if spaces.is_empty() {
                if style.bidi {
                    to_right(line);
                }
                return;
            }
            let extra = right - line_end_x;
            if justify && (extra > 0.0 || (extra < 0.0 && frame.wp_justification)) {
                let q = extra / spaces.len() as f64;
                for &k in &spaces {
                    line.widths[k] += q;
                }
            } else if extra < 0.0 && frame.compat == Compat::Generic {
                let caps: Vec<f64> = spaces.iter().map(|&k| line.widths[k] / 4.0).collect();
                let c: f64 = caps.iter().sum();
                let used = (-extra).min(c);
                let q = (c - used) / spaces.len() as f64;
                for (j, &k) in spaces.iter().enumerate() {
                    line.widths[k] -= caps[j] - q;
                }
            } else {
                if style.bidi {
                    to_right(line);
                }
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
            let free = right - line_end_x;
            if free > 0.0 {
                let off = if jc == Jc::Center { free / 2.0 } else { free };
                for x in &mut line.xs {
                    *x += off;
                }
            }
        }
        // Every line, the last too: equal gaps between the characters (spaces included), the first at the left edge
        // and the last ending at the right; a character alone is centred. Where East Asian text meets a Latin letter
        // or a digit the gap takes half an em more. Compat 15: each space first stretches by half its width.
        Jc::Distribute => {
            let chars: Vec<usize> = (0..content_end).filter(|&k| pieces[idx(k)].kind != Kind::Tab).collect();
            let extra = right - line_end_x;
            if chars.len() == 1 && extra > 0.0 {
                for x in &mut line.xs {
                    *x += extra / 2.0;
                }
            } else if chars.len() > 1 {
                let mut bonus: Vec<f64> = chars.windows(2).map(|w| auto_space(sp, &pieces[idx(w[0])], &pieces[idx(w[1])], style)).collect();
                if frame.compat == Compat::Generic {
                    for (j, &k) in chars[..chars.len() - 1].iter().enumerate() {
                        if pieces[idx(k)].kind == Kind::Space {
                            bonus[j] += line.widths[k] / 2.0;
                        }
                    }
                }
                let extra = extra - bonus.iter().sum::<f64>();
                if extra > 0.0 {
                    let q = extra / (chars.len() - 1) as f64;
                    for (j, &k) in chars[..chars.len() - 1].iter().enumerate() {
                        line.widths[k] += q + bonus[j];
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

/// The extra space between two characters of a distributed line: half the East Asian one's em where East Asian
/// text meets a Latin letter (`w:autoSpaceDE`) or a digit (`w:autoSpaceDN`).
fn auto_space(sp: &ShapedPara, a: &Piece, b: &Piece, style: &ParaStyle) -> f64 {
    let (Some(ca), Some(cb)) = (text_of(sp, a).chars().last(), text_of(sp, b).chars().next()) else { return 0.0 };
    let ea = |c: char| super::text::is_east_asian(c as u32);
    let latin = |c: char| !ea(c) && (c.is_ascii_digit() && !style.no_auto_space_dn || c.is_alphabetic() && !style.no_auto_space_de);
    let east = match (ea(ca), ea(cb)) {
        (true, false) if latin(cb) => a,
        (false, true) if latin(ca) => b,
        _ => return 0.0,
    };
    sp.styles[east.style as usize].size / 2.0
}

// Line height
fn picture_line_height(sp: &ShapedPara, line: &[Piece], mark: &Run, fonts: &Fonts, last: bool, compat: Compat, run_h: f64) -> f64 {
    let height = |run: &Run| {
        fonts.face(&run.ascii, None, run.bold, run.italic).map_or(0.0, |f| {
            let (a, d, e) = sp.metrics(&f, run, run.size);
            a + e + d
        })
    };
    let objects = line.iter().enumerate().filter(|(_, p)| p.kind == Kind::Object);
    match compat {
        Compat::Generic => objects.map(|(_, p)| height(&sp.styles[p.style as usize].run)).fold(0.0, f64::max),
        Compat::Legacy if run_h > 0.0 => 0.0,
        Compat::Legacy => {
            let before = objects
                .filter_map(|(k, _)| line[..k].iter().rev().find(|q| matches!(q.kind, Kind::Text | Kind::Space)))
                .map(|q| height(&sp.styles[q.style as usize].run))
                .fold(0.0, f64::max);
            if before > 0.0 {
                before
            } else if last {
                height(mark)
            } else {
                line.last().map_or(0.0, |p| height(&sp.styles[p.style as usize].run))
            }
        }
    }
}

fn times_line(h: f64, m: f64) -> f64 {
    let (h, line) = (Lu::round_half_up(h).0 as i64, (m * 240.0).round() as i64);
    Lu(((h * line + 120).div_euclid(240)) as i32).to_pt()
}

fn size(line: &mut Line, sp: &ShapedPara, pieces: &[Piece], style: &ParaStyle, mark: &Run, fonts: &Fonts, last: bool, frame: &Frame) {
    let compat = frame.compat;
    let (mut asc, mut desc) = (0.0f64, 0.0f64);
    let mark_size = style.mark_size.unwrap_or(mark.size);
    let mut object = false;
    let (mut un_asc, mut un_desc, mut un_text_asc, mut un_text_desc) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
    let mut run_h = 0.0f64;
    let (mut em_up, mut em_down) = (0.0f64, 0.0f64);
    let mut num_asc = 0.0f64;
    let mut has_content = false;
    let mut has_text = false;
    let mut separator = false;
    let mut brk: Option<(f64, f64)> = None;
    // The deepest descent an at-least line keeps below its baseline (a turned ruby annotation's lower half).
    let mut at_least_desc = 0.0f64;
    // `auto` centres the runs in flows that keep East Asian text upright, else aligns them to the baseline.
    line.align = match style.text_alignment.as_str() {
        "top" => Align::Top,
        "center" => Align::Center,
        "bottom" => Align::Bottom,
        "auto" | "" if frame.vertical && frame.centred => Align::Center,
        _ => Align::Baseline,
    };
    line.vertical = frame.vertical;
    for p in &pieces[line.start..line.end.min(pieces.len())] {
        match p.kind {
            // A note separator's line is in its run's size.
            Kind::Text | Kind::Object | Kind::Separator | Kind::ContSeparator | Kind::FullSeparator => {
                has_content = true;
                has_text = true;
                let st = &sp.styles[p.style as usize];
                let e = if frame.vertical && p.kind == Kind::Text { super::vertical::extra(st.face.as_deref(), st.size) } else { 0.0 };
                let (mut pa, mut pd) = (p.ascent + e, p.descent + e);
                if p.kind == Kind::Text
                    && let Some(f) = &st.face
                {
                    let c = align_drop(f, st.size, line.align, frame.vertical);
                    (pa, pd) = (pa - c, pd + c);
                } else if let Some(Object::Picture(_)) = p.object.map(|o| &sp.objects[o as usize]) {
                    let c = object_drop(p.ascent, p.descent, line.align);
                    (pa, pd) = (pa - c, pd + c);
                }
                asc = asc.max(pa);
                desc = desc.max(pd);
                let deeper = match p.object.map(|o| &sp.objects[o as usize]) {
                    Some(Object::Ruby(r)) => r.at_least_descent - p.descent,
                    _ => 0.0,
                };
                at_least_desc = at_least_desc.max(pd + deeper);
                let r = if sp.rules.no_raise_lower { 0.0 } else { st.run.position };
                let (a0, d0) = ((pa - r).max(0.0), (pd + r).max(0.0));
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
                } else if compat == Compat::Legacy && p.kind != Kind::Text {
                    separator = true;
                } else {
                    un_text_asc = un_text_asc.max(a0);
                    un_text_desc = un_text_desc.max(d0);
                    run_h = run_h.max(a0 + d0);
                }
            }
            // A list number is a run in its own font under the paragraph's alignment.
            Kind::ListNumber => {
                has_content = true;
                let st = &sp.styles[p.style as usize];
                let c = st.face.as_ref().map_or(0.0, |f| align_drop(f, st.size, line.align, frame.vertical));
                num_asc = num_asc.max(p.ascent - c);
            }
            Kind::LineBreak => brk = Some((p.ascent, p.descent)),
            Kind::Space | Kind::Tab => {}
            _ => {}
        }
    }
    if !has_content && let Some((a, d)) = brk {
        asc = a;
        desc = d;
        un_asc = a;
        un_desc = d;
    } else if (!has_content || (last && !has_text))
        && let Some(f) = fonts.face(&mark.ascii, None, mark.bold, mark.italic)
    {
        let (a, d, e) = sp.metrics(&f, mark, mark_size);
        let v = if frame.vertical { super::vertical::extra(Some(&f), mark_size) } else { 0.0 };
        let (a, d) = (a + e + v, d + v);
        asc = asc.max(a);
        desc = desc.max(d);
        un_asc = un_asc.max(a);
        un_desc = un_desc.max(d);
    }
    if last
        && object
        && un_text_asc + un_text_desc + num_asc == 0.0
        && let Some(f) = fonts.face(&mark.ascii, None, mark.bold, mark.italic)
    {
        let (a, d, e) = sp.metrics(&f, mark, mark_size);
        asc = asc.max(a + e + d);
    }
    let asc = asc.max(num_asc);
    let un_asc = if !has_text { un_asc.max(num_asc) } else { un_asc };
    let (asc, desc) = (asc + em_up, desc + em_down);
    let h = asc + desc;
    line.ascent = asc;
    line.descent = desc;
    let mut text_h = if has_text && run_h > 0.0 { run_h } else { un_asc + un_desc };
    if object {
        text_h = run_h.max(picture_line_height(sp, &pieces[line.start..line.end.min(pieces.len())], mark, fonts, last, compat, run_h));
    }
    // Compat ≤ 14: a note separator gives no height to space by, like an object, so the mark's is used.
    if separator
        && run_h == 0.0
        && let Some(f) = fonts.face(&mark.ascii, None, mark.bold, mark.italic)
    {
        let (a, d, e) = sp.metrics(&f, mark, mark_size);
        text_h = a + e + d;
    }
    // Line spacing
    match style.line_rule {
        LineRule::Auto => {
            let m = style.line;
            line.pitch = h + times_line(text_h, m) - text_h;
            if m >= 1.0 {
                line.baseline = asc;
                line.below = line.pitch - h;
            } else {
                line.baseline = line.pitch - times_line(desc, m);
            }
        }
        LineRule::AtLeast => {
            let x = style.line;
            line.pitch = x.max(h);
            line.baseline = if x <= h { asc } else { asc.max(x - desc.max(at_least_desc + em_down)) };
        }
        LineRule::Exact => {
            line.pitch = style.line;
            let x = Lu::round_half_up(style.line).0;
            line.baseline = Lu(x - x / 5).to_pt();
        }
    }
    // Document grid
    if let Some(p) = frame.line_grid
        && style.snap_to_grid
        && style.line_rule != LineRule::Exact
        && style.line > 0.0
    {
        let p = i64::from(p);
        let h = i64::from(Lu::round_half_up(h).0);
        let m = if style.line_rule == LineRule::Auto { (style.line * 240.0).round() as i64 } else { 240 };
        let hg = ((h + p - 1) / p * p).max(p * m / 240);
        let below = (hg - h) / 2;
        let x = if style.line_rule == LineRule::AtLeast { i64::from(Lu::round_half_up(style.line).0) } else { 0 };
        let pitch = hg.max(x);
        let lu = |v: i64| Lu(v as i32).to_pt();
        line.pitch = lu(pitch);
        line.below = lu(below);
        line.baseline = lu(pitch - below) - desc;
    }
}

// Text colour
fn ink(r: &RunStyle, review: &ReviewView, behind: Option<u32>) -> Option<u32> {
    if let Some(c) = revision_colour(r, review) {
        return Some(c);
    }
    r.color.or_else(|| r.shading.or(behind).filter(|c| super::env::is_dark(*c)).map(|_| 0xFFFFFF))
}

fn behind(o: &LineOrigin, src: &Paragraph) -> Option<u32> {
    src.para_style_in(o.review.view).shading.or(o.backdrop)
}

fn revision_colour(r: &RunStyle, review: &ReviewView) -> Option<u32> {
    let m = r.rev.del.as_ref().or(r.rev.ins.as_ref()).filter(|_| review.view == MarkupView::All)?;
    Some(if matches!(m.kind, MarkKind::MoveFrom | MarkKind::MoveTo) { AuthorPalette::MOVE } else { review.palette.color(m.author) })
}

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
        i.run().is_some_and(|r| !r.rev.is_empty() && n > 0 && if view.hides(&r.rev) { a <= hi && a + n >= lo } else { a < hi && a + n > lo })
    })
}

pub struct LineOrigin<'a> {
    pub shaped: &'a ShapedPara,
    pub from: usize,
    pub width: f64,
    pub x0: f64,
    pub compat: Compat,
    pub para: ParaId,
    pub review: &'a ReviewView,
    pub ul_trail_space: bool,
    pub palette: crate::drawing::paint::Palette<'a>,
    pub backdrop: Option<u32>,
}

impl LineOrigin<'_> {
    fn pieces(&self) -> &[Piece] {
        &self.shaped.pieces[self.from.min(self.shaped.pieces.len())..]
    }
}

// Drawing
pub fn draw_line(o: &LineOrigin, line: &Line, src: &Paragraph, fonts: &Fonts) -> LineFragment {
    {
        let sp = o.shaped;
        let pieces = o.pieces();
        let para = o.para;
        let (x0, top) = (o.x0, 0.0);
        let base = top + line.baseline;
        let mut out: Vec<Item> = Vec::with_capacity(line.rects.len() + 2);
        let (mut glyphs_left, mut chars_left) = (line.start..line.end).fold((0, 0), |(g, c), ix| {
            let (a, b) = pieces[ix].glyphs;
            (g + (b - a) as usize, c + text_of(sp, &pieces[ix]).len())
        });
        for (r, c) in &line.rects {
            out.push(Item::rect([x0 + r[0], top + r[1], x0 + r[2], top + r[3]], *c));
        }
        let rects_end = out.len();
        let behind_text = behind(o, src);
        let mut run: Option<GlyphRunBuilder> = None;
        let flush = |run: &mut Option<GlyphRunBuilder>, out: &mut Vec<Item>| {
            if let Some(r) = run.take()
                && !r.glyphs.is_empty()
            {
                out.push(Item::Glyphs(r.finish()));
            }
        };
        let mut atoms: Vec<Atom> = Vec::new();
        let mut texts: Vec<([f64; 4], crate::docx::Picture)> = Vec::new();
        let mut expansions: Vec<([f64; 4], Rc<super::expansion::Expansion>)> = Vec::new();
        let join = sp.join.as_deref();
        let source = |p: &Piece| match (join, p.src) {
            (Some(j), Some(u)) => {
                let (q, v) = j.source(u);
                (q, Some(v))
            }
            (Some(j), None) => (j.first(), None),
            (None, u) => (para, u),
        };
        let mut places = caret_places(o, line).into_iter().peekable();
        let mark = src.mark_in(o.review.view);
        let face = places.peek().and_then(|_| fonts.face(&mark.ascii, None, mark.bold, mark.italic));
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
            if matches!(p.kind, Kind::Separator | Kind::ContSeparator | Kind::FullSeparator) {
                let full = p.kind == Kind::FullSeparator || p.kind == Kind::ContSeparator && o.compat == Compat::Legacy;
                let len = if full { o.width } else { 144.0 };
                out.push(Item::rect([x, base - 2.7, x + len, base - 2.1], 0));
                continue;
            }
            let next_x = line.xs.get(k + 1).map(|&nx| x0 + nx);
            if p.kind == Kind::Tab {
                flush(&mut run, &mut out);
                draw_leader(o, line, k, x0, base, &mut out);
                if let Some(unit) = first {
                    atoms.push(Atom { x0: x, x1: next_x.unwrap_or(x + line.widths[k]), para: q, unit, brk: false });
                }
                continue;
            }
            if let (Kind::Object, Some(Object::Expansion(e))) = (p.kind, p.object.map(|o| &sp.objects[o as usize])) {
                flush(&mut run, &mut out);
                let rect = [x, base - e.size.1, x + e.size.0, base];
                expansions.push((rect, e.clone()));
                if let Some(unit) = first {
                    atoms.push(Atom { x0: x, x1: next_x.unwrap_or(x + e.size.0), para: q, unit, brk: false });
                }
                continue;
            }
            if let Some(Object::Ruby(r)) = p.object.map(|o| &sp.objects[o as usize]) {
                flush(&mut run, &mut out);
                ruby_glyphs(&r.base, &r.base_x, x, base, r.narrow, q, o.review, behind_text, &mut out);
                match r.turned {
                    Some([dx, dy]) => {
                        let (centre, mut items) = ([x + dx, base + dy], Vec::new());
                        ruby_glyphs(&r.text, &r.text_x, centre[0], centre[1], 0.0, q, o.review, behind_text, &mut items);
                        // A vertical line of its own: East Asian glyphs stand upright in it.
                        let items = super::vertical::east_asian_turned(&items);
                        out.push(Item::Turned(Rc::new(super::fragment::Turned { centre, deg: 90.0, items })));
                    }
                    None => ruby_glyphs(&r.text, &r.text_x, x, base - r.raise, 0.0, q, o.review, behind_text, &mut out),
                }
                if let Some(unit) = first {
                    atoms.push(Atom { x0: x, x1: next_x.unwrap_or(x + line.widths[k]), para: q, unit, brk: false });
                }
                continue;
            }
            // A picture bullet is a list label that is a picture.
            if p.kind == Kind::Object || (p.kind == Kind::ListNumber && p.object.is_some()) {
                if let Some(Object::Picture(pic)) = p.object.map(|o| &sp.objects[o as usize]) {
                    flush(&mut run, &mut out);
                    let drop = if p.kind == Kind::Object { object_drop(p.ascent, p.descent, line.align) } else { 0.0 };
                    let bottom = base + drop - sp.styles[p.style as usize].run.position - pic.effect[3];
                    let x0 = x + pic.effect[0];
                    let rect = [x0, bottom - pic.h, x0 + pic.w, bottom];
                    match pic.shape.as_deref() {
                        Some(shape) => out.extend(super::story::shape_items(shape, rect, 0, false, o.palette).into_iter().map(Item::Shape)),
                        None => out.push(Item::Image { rect, key: ImageKey { index: pic.image, generation: 0 } }),
                    }
                    if pic.text.is_some() {
                        texts.push((rect, pic.clone()));
                    }
                    if let Some(unit) = first {
                        atoms.push(Atom { x0: x, x1: next_x.unwrap_or(x + pic.w), para: q, unit, brk: false });
                    }
                }
                continue;
            }
            if matches!(p.kind, Kind::LineBreak | Kind::PageBreak | Kind::ColumnBreak) {
                if let Some(unit) = first {
                    atoms.push(Atom { x0: x, x1: x, para: q, unit, brk: true });
                }
                continue;
            }
            if !matches!(p.kind, Kind::Text | Kind::Space | Kind::ListNumber) {
                continue;
            }
            // A piece that draws nothing (a legacy list number's gap) ends the glyph run.
            let Some(face) = &st.face else {
                flush(&mut run, &mut out);
                continue;
            };
            if p.soft_hyphen && !(line.hyphen && ix + 1 == line.end) {
                continue;
            }
            let y = base - st.raise + align_drop(face, st.size, line.align, line.vertical);
            let color = ink(&st.run, o.review, behind_text);
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
            if line.hyphen
                && ix + 1 == line.end
                && let Some(g) = face.glyph('-')
            {
                r.glyphs.push((g, gx));
                r.chars.push(('-', gx, None));
            }
            glyphs_left = glyphs_left.saturating_sub((p.glyphs.1 - p.glyphs.0) as usize);
            chars_left = chars_left.saturating_sub(text.len());
        }
        flush(&mut run, &mut out);
        draw_decorations(o, line, base, rects_end, behind_text, &mut out);
        let end = match (line.xs.last(), line.widths.last()) {
            (Some(x), Some(w)) => x0 + x + w,
            _ => {
                let s = src.para_style_in(o.review.view);
                let left = s.ind_left + if o.from == 0 { s.ind_first } else { 0.0 };
                let right = o.width - s.ind_right;
                x0 + match (s.jc, s.bidi) {
                    (Jc::Center, _) => (left + right) / 2.0,
                    (Jc::Right, false) | (Jc::Left | Jc::Both, true) => right,
                    _ => left,
                }
            }
        };
        for (_, q) in places.by_ref() {
            out.extend(caret(end, q));
        }
        let changed = o.review.view.change_bars() && line_changed(o, line, src);
        let mark = (line.start == line.end && o.shaped.join.is_none()).then(|| Unit(src.inlines.iter().map(Inline::units).sum::<usize>() as u32));
        LineFragment {
            items: out.into_boxed_slice(),
            pitch: line.pitch,
            baseline: line.baseline,
            below: line.below,
            atoms: atoms.into_boxed_slice(),
            changed,
            mark,
            texts: texts.into_boxed_slice(),
            expansions: expansions.into_boxed_slice(),
        }
    }
}

/// A ruby's base or annotation: its pieces at `xs` from `x`, on `base`, each glyph `narrow` closer to the previous one
/// than its advance.
#[allow(clippy::too_many_arguments, reason = "the line's place, review view and backdrop, as for the line's own glyphs")]
fn ruby_glyphs(
    s: &ShapedPara,
    xs: &[f64],
    x: f64,
    base: f64,
    narrow: f64,
    para: ParaId,
    review: &ReviewView,
    behind: Option<u32>,
    out: &mut Vec<Item>,
) {
    let mut run: Option<GlyphRunBuilder> = None;
    for (i, &px) in xs.iter().enumerate() {
        let st = s.style(i);
        let Some(face) = &st.face else { continue };
        let (y, color, x0) = (base - st.raise, ink(&st.run, review, behind), x + px);
        if !run.as_ref().is_some_and(|r| Rc::ptr_eq(&r.face, face) && r.size == st.size && r.y == y && r.color == color) {
            out.extend(run.take().map(|r| Item::Glyphs(r.finish())));
            let (glyphs, chars) = (Vec::new(), Vec::new());
            run = Some(GlyphRunBuilder { face: face.clone(), size: st.size, y, glyphs, chars, color, para: Some(para), x: x0 });
        }
        let Some(r) = run.as_mut() else { continue };
        let scale = face.scale(st.size) * st.run.scale / 100.0;
        let mut gx = x0;
        for (id, advance, dx) in s.glyphs(i) {
            r.glyphs.push((id, gx + dx * scale));
            gx += (advance * scale - narrow).max(0.0);
        }
        r.chars.extend(s.text(i).chars().map(|c| (symbol_unicode(&face.family, c), x0, None)));
    }
    out.extend(run.map(|r| Item::Glyphs(r.finish())));
}

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

fn draw_leader(o: &LineOrigin, line: &Line, k: usize, x0: f64, base: f64, out: &mut Vec<Item>) {
    {
        let sp = o.shaped;
        let p = &o.pieces()[line.start + k];
        let st = &sp.styles[p.style as usize];
        let Some(leader) = line.leaders.iter().find(|(j, _)| *j == k).and_then(|(_, l)| l.glyph()) else { return };
        let Some(face) = &st.face else { return };
        let Some(g) = face.glyph(leader) else { return };
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

fn draw_decorations(o: &LineOrigin, line: &Line, base: f64, bg_at: usize, behind: Option<u32>, out: &mut Vec<Item>) {
    let (sp, x0, review) = (o.shaped, o.x0, o.review);
    let pieces = &o.pieces()[line.start..line.end];
    let content = pieces.iter().rposition(|p| p.kind != Kind::Space).map_or(0, |k| k + 1);
    let (mut bg, mut fg) = (Vec::new(), Vec::new());
    let push = |v: &mut Vec<Item>, r: [f64; 4], c: u32| {
        let joins = |it: &&mut Item| matches!(it, Item::Rect { rect: l, colour: lc } if *lc == c && (l[1] - r[1]).abs() < EPS && (l[3] - r[3]).abs() < EPS && (r[0] - l[2]).abs() < 0.01);
        match v.iter_mut().rev().take(4).find(joins) {
            Some(Item::Rect { rect: last, .. }) => last[2] = r[2],
            _ => v.push(Item::rect(r, c)),
        }
    };
    // Text borders: (border, left, right, top, bottom) of the stretch of runs drawn so far.
    let mut boxed: Option<(crate::docx::props::Border, f64, f64, f64, f64)> = None;
    let flush_box = |boxed: &mut Option<(crate::docx::props::Border, f64, f64, f64, f64)>, fg: &mut Vec<Item>| {
        let Some((b, l, r, t, btm)) = boxed.take() else { return };
        let w = super::table::thick(&b);
        let (l, r, t, btm) = (l - b.space - w, r + b.space + w, t - b.space - w, btm + b.space + w);
        super::table::edge(fg, [l, t, r, t + w], true, false, &b);
        super::table::edge(fg, [l, btm - w, r, btm], true, true, &b);
        super::table::edge(fg, [l, t, l + w, btm], false, false, &b);
        super::table::edge(fg, [r - w, t, r, btm], false, true, &b);
    };
    for (k, p) in pieces.iter().enumerate() {
        let st = &sp.styles[p.style as usize];
        let Some(face) = st.face.as_ref().filter(|_| matches!(p.kind, Kind::Text | Kind::Space | Kind::Tab | Kind::ListNumber)) else { continue };
        let (r, label) = (&st.run, p.kind == Kind::ListNumber);
        let (x, w) = (x0 + line.xs[k], line.widths[k]);
        if w <= 0.0 {
            continue;
        }
        let em = face.scale(st.size);
        let y = base - st.raise;
        let border = r.border.as_ref().filter(|_| !label && k < content);
        match (border, &mut boxed) {
            (Some(b), Some((was, _, right, top, bottom))) if b == was => {
                *right = x + w;
                *top = top.min(y - face.ascent * em);
                *bottom = bottom.max(y + face.descent * em);
            }
            (Some(b), _) => {
                flush_box(&mut boxed, &mut fg);
                boxed = Some((b.clone(), x, x + w, y - face.ascent * em, y + face.descent * em));
            }
            (None, _) => flush_box(&mut boxed, &mut fg),
        }
        let fill = r.highlight.or(r.shading).filter(|_| !label);
        if let Some(fill) = fill {
            push(&mut bg, [x, y - face.ascent * em, x + w, y + face.descent * em], fill);
        }
        let trailing = k >= content;
        if trailing && !o.ul_trail_space {
            continue;
        }
        let ink = ink(r, review, behind).unwrap_or(0);
        let line_at = |v: &mut Vec<Item>, y: f64, t: f64| push(v, [x, y - t / 2.0, x + w, y + t / 2.0], ink);
        let marked = review.view == MarkupView::All && !label;
        let kind = |m: &Option<Rc<crate::docx::review::RevisionMark>>| m.as_ref().filter(|_| marked).map(|m| m.kind);
        let (ins, del) = (kind(&r.rev.ins), kind(&r.rev.del));
        let rev_u = ins.map(|k| if k == MarkKind::MoveTo { "double" } else { "single" });
        if let Some(u) = rev_u.or(r.underline.as_deref()).filter(|u| !label && !(*u == "words" && matches!(p.kind, Kind::Space | Kind::Tab))) {
            let (pos, thick) =
                if face.underline_thick > 0.0 { (face.underline_pos, face.underline_thick) } else { (0.1 * face.upem, 0.05 * face.upem) };
            let (uy, t) = (y + pos * em, (thick * em).max(0.5));
            match u {
                "double" => {
                    line_at(&mut fg, uy, t);
                    line_at(&mut fg, uy + 2.0 * t, t);
                }
                "thick" => line_at(&mut fg, uy + t / 2.0, 2.0 * t),
                _ => line_at(&mut fg, uy, t),
            }
        }
        let dstrike = r.dstrike || del == Some(MarkKind::MoveFrom);
        if (r.strike || dstrike || del.is_some()) && !trailing {
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
    flush_box(&mut boxed, &mut fg);
    out.splice(bg_at..bg_at, bg);
    out.extend(fg);
}

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
