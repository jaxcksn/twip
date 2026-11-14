//! Line breaking

use crate::docx::Compat;
use std::cell::OnceCell;
use std::rc::Rc;

use crate::docx::model::*;
use crate::docx::props::Leader;
use crate::docx::review::ReviewView;
use crate::font::Fonts;

use super::LayoutCtx;
use super::cache::{CacheKey, cached};
use super::fragment::LineFragment;
use super::ids::ParaId;
use super::job::{Memos, Yield};
use super::space::{BorderKey, ParaSpace};
use super::text::{Kind, Piece, ShapedPara, para_id};
use super::units::Lu;

mod borders;
mod breaking;
mod draw;
mod size;

pub use borders::{apply_borders, border_key, border_space, continued_key, neighbour_borders, top_border_extra};
pub use breaking::{band, break_paragraph, clear_of};
pub use draw::{LineOrigin, draw_label, draw_line};

use breaking::line_style;

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
    pub exact_on_top: bool,
    pub exact_extra_after: bool,
    pub drop_cut_indents: bool,
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
            word_from: space.letter_break.then_some(0),
            vertical: e.vertical.is_some(),
            centred: e.vertical.is_some_and(|v| v.upright),
            breaking_rules: e.breaking_rules,
            strict_kinsoku: e.strict_kinsoku,
            hang_stop: !e.no_tab_hang_ind,
            number_hang_stop: !e.no_tab_hang_ind && !e.no_number_hang_stop,
            exact_on_top: e.exact_on_top,
            exact_extra_after: e.exact_extra_after,
            drop_cut_indents: e.drop_cut_indents,
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
        let ParaLines {
            shaped,
            lines,
            from,
            width,
            x0,
            para: _,
            borders: _,
            drawn: _,
            compat: _,
            review: _,
            ul_trail_space: _,
            theme: _,
            colour_map: _,
            backdrop: _,
        } = self;
        Rc::ptr_eq(shaped, &o.shaped) && *lines == o.lines && *from == o.from && *width == o.width && *x0 == o.x0
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
    // After a leading page or column break the paragraph starts again: first-line indent, borders.
    let opens = shaped.opens && (from == 0 || shaped.pieces[..from].iter().all(|q| matches!(q.kind, Kind::PageBreak | Kind::ColumnBreak)));
    let frame = Frame::new(space);
    let view = space.env.review.view;
    let (style, mark) = (para.para_style_in(view), para.mark_in(view));
    let mut lines =
        super::job::unit("line breaking", || crate::prof::time("break", || break_paragraph(shaped, pieces, style, mark, &frame, fonts, !opens)));
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

pub fn para_space(
    style: &ParaStyle,
    prev: Option<&ParaStyle>,
    next: Option<&ParaStyle>,
    width: f64,
    from: usize,
    x0: f64,
    ctx: &LayoutCtx,
) -> ParaSpace {
    let borders: (BorderKey, BorderKey) = match border_key(style) {
        None => (None, None),
        Some(_) => {
            let (p, n) = neighbour_borders(style, prev, next);
            (if from > 0 { super::line::continued_key(style) } else { p }, n)
        }
    };
    ParaSpace { width, start: from, borders, x0, env: ctx.env.text.clone(), fit: false, letter_break: false }
}
