//! The built-in rulers with twip-graphics: flat bands on the paper colour with hatched margins, hairline ticks and
//! markers, and the numbers over the ticks; the drag's and hover's measurements over the document, as the Skia path
//! draws them ([`crate::render::ruler`]), from the same frame.

use std::rc::Rc;

use twip_graphics::kurbo::{self, Affine};
use twip_graphics::paint::Color;
use twip_graphics::scene::{Clip, Recorder};

use super::{Graphics, chrome_face, colour, dashed, measure, pen, rect_path};
use crate::api::services::{Popover, RulerView, TabSelector};
use crate::api::value::TabAlignment;
use crate::editing::view::{RulerMeasure, Tone};
use crate::font::Face;
use crate::render::ChromeFace;
use crate::render::theme::Theme;
use crate::services::ruler::{BAND, INSET, Marker, SELECTOR, marks};

pub use crate::render::ruler::RulerFrame;

/// A ruler's paper strip, from `S0` to `S1` across its band (CSS px): it floats on the desk.
const S0: f64 = INSET;
const S1: f64 = BAND - INSET;

fn fill(rec: &mut Recorder, l: f64, t: f64, r: f64, b: f64, c: Color) {
    rec.fill_rect(kurbo::Rect::new(l, t, r, b), c);
}

/// 135° hairlines every `gap` px (measured across them) over `r`.
fn hatch(rec: &mut Recorder, r: kurbo::Rect, gap: f64, c: Color) {
    if r.width() <= 0.0 || r.height() <= 0.0 {
        return;
    }
    rec.push_clip(Clip::Rect { rect: r, aa: true });
    let (step, h) = (gap * std::f64::consts::SQRT_2, r.height());
    let mut x = r.x0 - h;
    while x < r.x1 {
        rec.line((x, r.y1).into(), (x + h, r.y0).into(), &pen(1.0), c);
        x += step;
    }
    rec.pop();
}

/// An indent marker at `x`: a 2 px stem and a 10 px bar, the bar at the top of the 12 px box from `top`, or at its
/// bottom.
fn indent(rec: &mut Recorder, x: f64, top: f64, bar_on_top: bool, c: Color) {
    if bar_on_top {
        fill(rec, x - 1.0, top, x + 1.0, top + 11.0, c);
        fill(rec, x - 5.0, top, x + 5.0, top + 2.0, c);
    } else {
        fill(rec, x - 1.0, top + 1.0, x + 1.0, top + 12.0, c);
        fill(rec, x - 5.0, top + 10.0, x + 5.0, top + 12.0, c);
    }
}

/// A tab stop's glyph, centred on `x` in the 14 px box from `top`: a stem, and a foot where the text goes (the decimal
/// one with its point; the bar tab the stem alone).
fn tab_glyph(rec: &mut Recorder, kind: TabAlignment, x: f64, top: f64, c: Color) {
    let left = x - 7.0;
    fill(rec, left + 6.0, top + 3.0, left + 8.0, top + 12.0, c);
    let (from, width) = match kind {
        TabAlignment::Left => (6.0, 7.0),
        TabAlignment::Center | TabAlignment::Decimal => (2.0, 10.0),
        TabAlignment::Right => (1.0, 7.0),
        TabAlignment::Bar => return,
    };
    fill(rec, left + from, top + 10.0, left + from + width, top + 12.0, c);
    if kind == TabAlignment::Decimal {
        fill(rec, left + 10.0, top + 5.0, left + 12.0, top + 7.0, c);
    }
}

/// The tab alignment a type adds (none: the indent types).
fn alignment(t: TabSelector) -> Option<TabAlignment> {
    match t {
        TabSelector::Left => Some(TabAlignment::Left),
        TabSelector::Center => Some(TabAlignment::Center),
        TabSelector::Right => Some(TabAlignment::Right),
        TabSelector::Decimal => Some(TabAlignment::Decimal),
        TabSelector::Bar => Some(TabAlignment::Bar),
        TabSelector::FirstLineIndent | TabSelector::HangingIndent => None,
    }
}

/// The baseline that sets `face`'s text, `size` px, in a line `height` px tall from `top`, as CSS does.
fn baseline(face: &Face, size: f64, top: f64, height: f64) -> f64 {
    let k = face.scale(size);
    let (ascent, descent) = (face.ascent * k, face.descent * k);
    top + (height - (descent + ascent)) / 2.0 + ascent
}

/// A ruler tick: a mark (place, length in CSS px, ink) or a labelled mark's number.
enum Tick {
    Mark(f64, f64, Color),
    Number(f64, i64),
}

/// A label's text, face and size.
struct Text<'a> {
    face: &'a Rc<Face>,
    size: f64,
    text: &'a str,
}

impl Graphics {
    /// `text` on a box of `bg` (with its corner radius) padded `pad` px across and `pad_y` px down, its top-left corner
    /// at (`x`, `top`), the text in `ink`. Its width.
    #[allow(clippy::too_many_arguments, reason = "one label's geometry and colours, drawn in one place")]
    fn label(&mut self, rec: &mut Recorder, t: &Text<'_>, x: f64, top: f64, pad: f64, pad_y: f64, bg: Option<(Color, f64)>, ink: Color) -> f64 {
        let tw = measure(t.face, t.text, t.size);
        let (w, h) = (tw + 2.0 * pad, t.size + 2.0 * pad_y);
        if let Some((bg, radius)) = bg {
            rec.fill_rrect(kurbo::Rect::new(x, top, x + w, top + h), radius, bg);
        }
        self.text(rec, t.face, t.text, x + pad, baseline(t.face, t.size, top + pad_y, t.size), t.size, ink);
        w
    }
}

/// Draws the rulers shown along the top and left of the viewport, and the measurements over the document, from the
/// view and the frame alone, into `rec` (device pixels).
// shortcut: one built-in look; a painter plugin will replace this function.
pub(super) fn paint(g: &mut Graphics, rec: &mut Recorder, view: &RulerView, theme: &Theme, f: &RulerFrame) {
    let t = &theme.tokens;
    let accent = colour(theme.accent.solid);
    let (ink, ink2, ink3, paper, line, desk) = (colour(t.ink), colour(t.ink2), colour(t.ink3), colour(t.paper), colour(t.line), colour(t.desk));
    let band = BAND;
    let (w, h) = (f.width, f.height);
    let (major, minor, per) = marks(view.unit);
    let numbers = chrome_face(ChromeFace::Mono600);
    rec.push_transform(Affine::scale(f.dpr));

    let dragged = |m: Marker| f.drag.filter(|d| d.marker == m);
    let moved = |m: Marker, now: f64| dragged(m).map_or(now, |d| d.at);
    let colour_of = |on: bool| if on { accent } else { ink };

    // Ticks from `zero` (the text start) both ways along `0..len` points, `at` placing a point along the ruler: every
    // tick first (its place, length in CSS px and ink), then each labelled mark's place and number.
    let ticks = |zero: f64, len: f64, at: &dyn Fn(f64) -> f64, each: &mut dyn FnMut(Tick)| {
        let steps = (major / minor).round() as i64;
        let (lo, hi) = (((0.0 - zero) / minor).ceil() as i64, ((len - zero) / minor).floor() as i64);
        for i in lo..=hi {
            let p = at(zero + i as f64 * minor).round();
            let (len, c) = if i % steps == 0 {
                (6.0, ink2)
            } else if steps % 2 == 0 && i % (steps / 2) == 0 {
                (4.0, ink3)
            } else {
                (2.0, ink3)
            };
            each(Tick::Mark(p, len, c));
        }
        for i in lo..=hi {
            if i != 0 && i % steps == 0 {
                let n = (i / steps).abs() as f64 * per;
                each(Tick::Number(at(zero + i as f64 * minor).round(), n.round() as i64));
            }
        }
    };

    if f.bands.horizontal {
        let (o, k) = (view.horizontal.origin, view.horizontal.scale);
        let x = |pt: f64| o + pt * k;
        let r = &view.model.horizontal;
        fill(rec, 0.0, 0.0, w, band, desk);
        rec.push_clip(Clip::Rect { rect: kurbo::Rect::new(if f.bands.vertical { band } else { 0.0 }, 0.0, w, band), aa: true });
        let start = moved(Marker::TextStart, r.text.start.0);
        let end = moved(Marker::TextEnd, r.text.end.0);
        let (page_l, page_r) = (x(0.0), x(r.page_width.0));
        let strip = kurbo::Rect::new(page_l, S0, page_r, S1);
        rec.fill_rrect(strip, 2.0, paper);
        g.draw_shape(rec, kurbo::RoundedRect::from_rect(strip.inflate(0.5, 0.5), 2.5), Some(&pen(1.0)), line);
        hatch(rec, kurbo::Rect::new(page_l, S0, x(start), S1), 5.0, line);
        hatch(rec, kurbo::Rect::new(x(end), S0, page_r, S1), 5.0, line);
        for c in r.columns.windows(2) {
            hatch(rec, kurbo::Rect::new(x(c[0].end.0), S0, x(c[1].start.0), S1), 5.0, line);
        }
        ticks(start, r.page_width.0, &x, &mut |tick| match tick {
            Tick::Mark(px, len, c) => fill(rec, px, S1 - len, px + 1.0, S1, c),
            Tick::Number(px, n) => {
                if let Some(face) = &numbers {
                    let n = n.to_string();
                    let tw = measure(face, &n, 9.0);
                    g.label(rec, &Text { face, size: 9.0, text: &n }, px - tw / 2.0 - 2.0, S0 + 1.0, 2.0, 0.0, Some((paper, 0.0)), ink);
                }
            }
        });
        // The margins' handles tint while hovered.
        for m in [Marker::TextStart, Marker::TextEnd].into_iter().filter(|m| f.hover == Some(*m)) {
            let at = x(if m == Marker::TextStart { start } else { end });
            fill(rec, at - 4.0, S0, at + 4.0, S1, colour(theme.accent.soft));
        }
        // Table column markers: hatched boxes, accent while dragged.
        for (i, b) in r.table_columns.iter().enumerate() {
            let m = Marker::TableColumn(i as u32);
            let at = x(moved(m, b.0));
            let cell = kurbo::Rect::new(at - 4.0, S0, at + 4.0, S1);
            if dragged(m).is_some() {
                rec.fill_rect(cell, accent);
            } else {
                rec.fill_rect(cell, paper);
                hatch(rec, cell, 4.0, ink3);
                rec.stroke_path(&rect_path(cell.inflate(-0.5, -0.5)), &pen(1.0), ink3);
            }
        }
        // Tab stops: accent while dragged or while their popover is open, faint while a release removes them.
        let open = match f.popover {
            Some(Popover::TabStop { position, .. }) => Some(position.0),
            Some(Popover::TabType { .. } | Popover::AutoCorrect { .. }) | None => None,
        };
        for s in &r.tabs {
            let m = Marker::Tab(s.position.0);
            let d = dragged(m);
            let on = d.is_some() || open.is_some_and(|p| (p - s.position.0).abs() < 0.5);
            let c = colour_of(on).with_alpha_f(if d.is_some_and(|d| d.removing) { 0.25 } else { 1.0 });
            tab_glyph(rec, s.alignment, x(moved(m, start + s.position.0)), S1 - 12.0, c);
        }
        // The indent markers on the strip's edges: first line at the top, hanging (with the left indent) and right at
        // the bottom.
        let left = match f.drag {
            Some(d) if matches!(d.marker, Marker::Hanging | Marker::Left) => d.at,
            _ => start + r.indents.left.0,
        };
        let first = match f.drag {
            Some(d) if d.marker == Marker::FirstLine => d.at,
            Some(d) if d.marker == Marker::Left => d.at + r.indents.first_line.0,
            _ => start + r.indents.left.0 + r.indents.first_line.0,
        };
        let right = moved(Marker::Right, end - r.indents.right.0);
        let on = |ms: &[Marker]| f.drag.is_some_and(|d| ms.contains(&d.marker));
        indent(rec, x(first), S0 - 2.0, true, colour_of(on(&[Marker::FirstLine, Marker::Left])));
        indent(rec, x(left), S1 - 10.0, false, colour_of(on(&[Marker::Hanging, Marker::Left])));
        indent(rec, x(right), S1 - 10.0, false, colour_of(on(&[Marker::Right])));
        // The ghost of the stop a click adds.
        if let Some((gh, a)) = f.ghost.zip(alignment(f.selector)) {
            tab_glyph(rec, a, x(gh), S1 - 12.0, accent.with_alpha_f(0.5));
        }
        if let Some((true, at)) = f.guide {
            fill(rec, at, S0, at + 1.0, S1, accent);
        }
        rec.pop();
        // The tab type box in the corner: the type a click adds; accent while its picker is open.
        let b = kurbo::Rect::new(SELECTOR.left, SELECTOR.top, SELECTOR.left + SELECTOR.width, SELECTOR.top + SELECTOR.height);
        let picking = matches!(f.popover, Some(Popover::TabType { .. }));
        rec.fill_rrect(b, 2.0, paper);
        g.draw_shape(rec, kurbo::RoundedRect::from_rect(b.inflate(-0.5, -0.5), 1.5), Some(&pen(1.0)), if picking { accent } else { ink3 });
        let cx = b.center().x;
        match (alignment(f.selector), f.selector) {
            (Some(a), _) => tab_glyph(rec, a, cx, b.y0, ink),
            (None, TabSelector::FirstLineIndent) => indent(rec, cx, b.y0 + 1.0, true, ink),
            (None, _) => indent(rec, cx, b.y0 + 1.0, false, ink),
        }
    }

    if f.bands.vertical {
        let (o, k) = (view.vertical.origin, view.vertical.scale);
        let y = |pt: f64| o + pt * k;
        let r = &view.model.vertical;
        let from = if f.bands.horizontal { band } else { 0.0 };
        fill(rec, 0.0, from, band, h, desk);
        rec.push_clip(Clip::Rect { rect: kurbo::Rect::new(0.0, from, band, h), aa: true });
        let top = moved(Marker::Top, r.text.start.0);
        let bottom = moved(Marker::Bottom, r.text.end.0);
        let (page_t, page_b) = (y(0.0), y(r.page_height.0));
        let strip = kurbo::Rect::new(S0, page_t, S1, page_b);
        rec.fill_rrect(strip, 2.0, paper);
        g.draw_shape(rec, kurbo::RoundedRect::from_rect(strip.inflate(0.5, 0.5), 2.5), Some(&pen(1.0)), line);
        hatch(rec, kurbo::Rect::new(S0, page_t, S1, y(top)), 5.0, line);
        hatch(rec, kurbo::Rect::new(S0, y(bottom), S1, page_b), 5.0, line);
        ticks(top, r.page_height.0, &y, &mut |tick| match tick {
            Tick::Mark(py, len, c) => fill(rec, S1 - len, py, S1, py + 1.0, c),
            Tick::Number(py, n) => {
                if let Some(face) = &numbers {
                    let n = n.to_string();
                    let tw = measure(face, &n, 9.0);
                    let left = S0 + 2.0 + ((S1 - S0 - 4.0) - tw).max(0.0) / 2.0;
                    g.label(rec, &Text { face, size: 9.0, text: &n }, left, py - 9.0 / 2.0 - 2.0, 0.0, 2.0, Some((paper, 0.0)), ink);
                }
            }
        });
        for m in [Marker::Top, Marker::Bottom].into_iter().filter(|m| f.hover == Some(*m)) {
            let at = y(if m == Marker::Top { top } else { bottom });
            fill(rec, S0, at - 4.0, S1, at + 4.0, colour(theme.accent.soft));
        }
        for (i, b) in r.table_rows.iter().enumerate() {
            let m = Marker::TableRow(i as u32);
            let at = y(moved(m, b.0));
            let cell = kurbo::Rect::new(S0, at - 4.0, S1, at + 4.0);
            if dragged(m).is_some() {
                rec.fill_rect(cell, accent);
            } else {
                rec.fill_rect(cell, paper);
                hatch(rec, cell, 4.0, ink3);
                rec.stroke_path(&rect_path(cell.inflate(-0.5, -0.5)), &pen(1.0), ink3);
            }
        }
        if let Some((false, at)) = f.guide {
            fill(rec, S0, at, S1, at + 1.0, accent);
        }
        rec.pop();
    }

    // A drag's guide line over the document.
    if let Some((horizontal, at)) = f.guide {
        let guide = dashed(1.0, 3.0, 3.0);
        let at = at + 0.5;
        let c = colour(f.page.accent.solid);
        if horizontal {
            rec.line((at, band).into(), (at, h).into(), &guide, c);
        } else {
            rec.line((band, at).into(), (w, at).into(), &guide, c);
        }
    }
    measures(g, rec, view, &f.page, f.measures);
    rec.pop();
}

/// Draws the measurements in the page's theme: dimension lines with end ticks and their label on the paper colour,
/// dark chips and hints, and status lines; dimension and status points are the ruler page's, mapped by `view`.
fn measures(g: &mut Graphics, rec: &mut Recorder, view: &RulerView, theme: &Theme, measures: &[RulerMeasure]) {
    if measures.is_empty() {
        return;
    }
    let t = &theme.tokens;
    let (Some(value), Some(chip)) = (chrome_face(ChromeFace::Mono600), chrome_face(ChromeFace::Mono500)) else { return };
    let (h, v) = (view.horizontal, view.vertical);
    let client = |p: [f64; 2]| (h.origin + p[0] * h.scale, v.origin + p[1] * v.scale);
    let tone = |tone: Tone| {
        colour(match tone {
            Tone::Accent => theme.accent.solid,
            Tone::Muted => t.ink2,
        })
    };
    let (paper, ink) = (colour(t.paper), colour(t.ink));
    for m in measures {
        match m {
            RulerMeasure::Dimension { from, to, label: text, tone: c } => {
                let ((x0, y0), (x1, y1)) = (client(*from), client(*to));
                let c = tone(*c);
                let label = Text { face: &value, size: 10.0, text };
                if (y1 - y0).abs() <= (x1 - x0).abs() {
                    let (a, z, y) = (x0.min(x1), x0.max(x1).max(x0.min(x1) + 2.0), y0.round());
                    fill(rec, a, y - 3.0, a + 1.0, y + 4.0, c);
                    fill(rec, z - 1.0, y - 3.0, z, y + 4.0, c);
                    fill(rec, a, y, z, y + 1.0, c);
                    let tw = measure(&value, text, 10.0);
                    g.label(rec, &label, (a + z) / 2.0 - tw / 2.0 - 4.0, y - 5.0, 4.0, 0.0, Some((paper, 0.0)), c);
                } else {
                    let (a, z, x) = (y0.min(y1), y0.max(y1).max(y0.min(y1) + 2.0), x0.round());
                    fill(rec, x - 3.0, a, x + 4.0, a + 1.0, c);
                    fill(rec, x - 3.0, z - 1.0, x + 4.0, z, c);
                    fill(rec, x, a, x + 1.0, z, c);
                    g.label(rec, &label, x - 4.0, (a + z) / 2.0 - 5.0, 4.0, 0.0, Some((paper, 0.0)), c);
                }
            }
            RulerMeasure::Chip { at, label: text } => {
                g.label(rec, &Text { face: &chip, size: 10.0, text }, at[0], at[1], 7.0, 4.0, Some((ink, 2.0)), paper);
            }
            RulerMeasure::Status { at, label: text, tone: c } => {
                let (x, y) = client(*at);
                g.label(rec, &Text { face: &value, size: 10.0, text }, x, y, 0.0, 0.0, None, tone(*c));
            }
        }
    }
}
