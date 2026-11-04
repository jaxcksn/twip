//! The built-in rulers' look: one painting boundary, [`paint`], that draws them from the ruler view and the frame's
//! interaction state alone, so a later painter can replace it without touching the model, the commands or the
//! interaction. Flat bands on the paper colour with hatched margins, hairline ticks and markers, and the numbers
//! over the ticks; the drag's and hover's measurements over the document.

use skia_safe::{Canvas, Color, Font, Paint, PathEffect, RRect, Rect};

use super::theme::Theme;
use super::{ChromeFace, chrome_typeface};
use crate::api::services::{Popover, RulerView, TabSelector};
use crate::api::value::TabAlignment;
use crate::editing::view::{RulerMeasure, Tone};
use crate::services::ruler::{BAND, Bands, HINT_Y, Marker, SELECTOR, marks};

/// The vertical ruler's paper strip, CSS px wide, along the band's left edge.
const STRIP: f32 = 18.0;

/// A marker a drag moves: where the drag puts it (page points along its ruler), and whether the release removes it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dragged {
    pub marker: Marker,
    pub at: f64,
    pub removing: bool,
}

/// Where the rulers go and what else they show: which bands, the viewport (CSS px), device pixels per CSS pixel, the
/// tab type box's type, the guide line of a drag (horizontal ruler: an x, else a y; client px), the dragged marker,
/// the ghost of the stop a click adds (page points), the hovered marker, the popover open, and the measurements.
pub struct RulerFrame<'a> {
    pub bands: Bands,
    pub width: f64,
    pub height: f64,
    pub dpr: f64,
    pub selector: TabSelector,
    pub guide: Option<(bool, f64)>,
    pub drag: Option<Dragged>,
    pub ghost: Option<f64>,
    pub hover: Option<Marker>,
    pub popover: Option<&'a Popover>,
    pub measures: &'a [RulerMeasure],
}

fn fill(c: Color) -> Paint {
    let mut p = Paint::default();
    p.set_anti_alias(true);
    p.set_color(c);
    p
}

fn faded(c: Color, alpha: f32) -> Paint {
    let mut p = fill(c);
    p.set_alpha_f(alpha);
    p
}

fn stroke(c: Color) -> Paint {
    let mut p = fill(c);
    p.set_style(skia_safe::paint::Style::Stroke);
    p.set_stroke_width(1.0);
    p
}

fn rect(canvas: &Canvas, l: f32, t: f32, r: f32, b: f32, paint: &Paint) {
    canvas.draw_rect(Rect::new(l, t, r, b), paint);
}

/// 135° hairlines every `gap` px (measured across them) over `r`.
fn hatch(canvas: &Canvas, r: Rect, gap: f32, c: Color) {
    if r.width() <= 0.0 || r.height() <= 0.0 {
        return;
    }
    canvas.save();
    canvas.clip_rect(r, None, true);
    let ink = stroke(c);
    let (step, h) = (gap * std::f32::consts::SQRT_2, r.height());
    let mut x = r.left - h;
    while x < r.right {
        canvas.draw_line((x, r.bottom), (x + h, r.top), &ink);
        x += step;
    }
    canvas.restore();
}

/// An indent marker at `x`: a 2 px stem and a 10 px bar, the bar at the top of the 12 px box from `top`, or at its
/// bottom.
fn indent(canvas: &Canvas, x: f32, top: f32, bar_on_top: bool, paint: &Paint) {
    if bar_on_top {
        rect(canvas, x - 1.0, top, x + 1.0, top + 11.0, paint);
        rect(canvas, x - 5.0, top, x + 5.0, top + 2.0, paint);
    } else {
        rect(canvas, x - 1.0, top + 1.0, x + 1.0, top + 12.0, paint);
        rect(canvas, x - 5.0, top + 10.0, x + 5.0, top + 12.0, paint);
    }
}

/// A tab stop's glyph, centred on `x` in the 14 px box from `top`: a stem, and a foot where the text goes (the
/// decimal one with its point; the bar tab the stem alone).
fn tab_glyph(canvas: &Canvas, kind: TabAlignment, x: f32, top: f32, paint: &Paint) {
    let left = x - 7.0;
    rect(canvas, left + 6.0, top + 3.0, left + 8.0, top + 12.0, paint);
    let (from, width) = match kind {
        TabAlignment::Left => (6.0, 7.0),
        TabAlignment::Center | TabAlignment::Decimal => (2.0, 10.0),
        TabAlignment::Right => (1.0, 7.0),
        TabAlignment::Bar => return,
    };
    rect(canvas, left + from, top + 10.0, left + from + width, top + 12.0, paint);
    if kind == TabAlignment::Decimal {
        rect(canvas, left + 10.0, top + 5.0, left + 12.0, top + 7.0, paint);
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

/// The baseline that sets `font`'s text in a line `height` px tall from `top`, as CSS does.
fn baseline(font: &Font, top: f32, height: f32) -> f32 {
    let (_, m) = font.metrics();
    top + (height - (m.descent - m.ascent)) / 2.0 - m.ascent
}

/// A chrome face at `size` px.
fn face(face: ChromeFace, size: f32) -> Option<Font> {
    chrome_typeface(face).map(|tf| Font::from_typeface(tf, size))
}

/// `text` on a box of `bg` padded `pad` px across and `pad_y` px down, its top-left corner at (`x`, `top`), the text
/// in `ink`. Its width.
#[allow(clippy::too_many_arguments, reason = "one label's geometry and colours, drawn in one place")]
fn label(canvas: &Canvas, font: &Font, text: &str, x: f32, top: f32, pad: f32, pad_y: f32, bg: Option<(Color, f32)>, ink: Color) -> f32 {
    let (tw, _) = font.measure_str(text, None);
    let (w, h) = (tw + 2.0 * pad, font.size() + 2.0 * pad_y);
    if let Some((bg, radius)) = bg {
        canvas.draw_rrect(RRect::new_rect_xy(Rect::from_xywh(x, top, w, h), radius, radius), &fill(bg));
    }
    canvas.draw_str(text, (x + pad, baseline(font, top + pad_y, font.size())), font, &fill(ink));
    w
}

/// Draws the rulers shown along the top and left of the viewport, and the measurements over the document, from the
/// view and the frame alone.
// shortcut: one built-in look; a painter plugin will replace this function.
pub fn paint(view: &RulerView, theme: &Theme, canvas: &Canvas, f: &RulerFrame) {
    let t = &theme.tokens;
    let accent = theme.accent.solid;
    let band = BAND as f32;
    let (w, h) = (f.width as f32, f.height as f32);
    let (major, minor, per) = marks(view.unit);
    let numbers = face(ChromeFace::Mono600, 9.0);
    let ink = fill(t.ink);
    canvas.save();
    canvas.scale((f.dpr as f32, f.dpr as f32));

    let dragged = |m: Marker| f.drag.filter(|d| d.marker == m);
    let moved = |m: Marker, now: f64| dragged(m).map_or(now, |d| d.at);
    let colour = |on: bool| fill(if on { accent } else { t.ink });

    // Ticks from `zero` (the text start) both ways along `0..len` points, `at` placing a point along the ruler:
    // `tick` draws one (its length, CSS px, and its ink) and `number` a labelled mark's number.
    let ticks = |zero: f64, len: f64, at: &dyn Fn(f64) -> f32, tick: &dyn Fn(f32, f32, &Paint), number: &dyn Fn(f32, &str)| {
        let steps = (major / minor).round() as i64;
        let (lo, hi) = (((0.0 - zero) / minor).ceil() as i64, ((len - zero) / minor).floor() as i64);
        let (unit, fine) = (fill(t.ink2), fill(t.ink3));
        for i in lo..=hi {
            let p = at(zero + i as f64 * minor).round();
            if i % steps == 0 {
                tick(p, 10.0, &unit);
            } else if steps % 2 == 0 && i % (steps / 2) == 0 {
                tick(p, 6.0, &fine);
            } else {
                tick(p, 3.0, &fine);
            }
        }
        for i in lo..=hi {
            if i != 0 && i % steps == 0 {
                let n = (i / steps).abs() as f64 * per;
                number(at(zero + i as f64 * minor).round(), &format!("{}", n.round() as i64));
            }
        }
    };

    if f.bands.horizontal {
        let (o, k) = (view.horizontal.origin, view.horizontal.scale);
        let x = |pt: f64| (o + pt * k) as f32;
        let r = &view.model.horizontal;
        rect(canvas, 0.0, 0.0, w, band, &fill(t.desk));
        canvas.save();
        canvas.clip_rect(Rect::new(if f.bands.vertical { band } else { 0.0 }, 0.0, w, band), None, true);
        let start = moved(Marker::TextStart, r.text.start.0);
        let end = moved(Marker::TextEnd, r.text.end.0);
        let (page_l, page_r) = (x(0.0), x(r.page_width.0));
        let paper = Rect::new(page_l, 0.0, page_r, band);
        canvas.draw_rrect(RRect::new_rect_xy(paper, 2.0, 2.0), &fill(t.paper));
        canvas.draw_rrect(RRect::new_rect_xy(paper.with_outset((0.5, 0.5)), 2.5, 2.5), &stroke(t.line));
        hatch(canvas, Rect::new(page_l, 0.0, x(start), band), 5.0, t.line);
        hatch(canvas, Rect::new(x(end), 0.0, page_r, band), 5.0, t.line);
        for c in r.columns.windows(2) {
            hatch(canvas, Rect::new(x(c[0].end.0), 0.0, x(c[1].start.0), band), 5.0, t.line);
        }
        ticks(start, r.page_width.0, &|pt| x(pt), &|px, len, paint| rect(canvas, px, band - len, px + 1.0, band, paint), &|px, n| {
            if let Some(font) = &numbers {
                let (tw, _) = font.measure_str(n, None);
                label(canvas, font, n, px - tw / 2.0 - 2.0, 3.0, 2.0, 0.0, Some((t.paper, 0.0)), t.ink);
            }
        });
        // The margins' handles tint while hovered.
        for m in [Marker::TextStart, Marker::TextEnd].into_iter().filter(|m| f.hover == Some(*m)) {
            let at = x(if m == Marker::TextStart { start } else { end });
            rect(canvas, at - 4.0, 0.0, at + 4.0, band, &fill(theme.accent.soft));
        }
        // Table column markers: hatched boxes, accent while dragged.
        for (i, b) in r.table_columns.iter().enumerate() {
            let m = Marker::TableColumn(i as u32);
            let at = x(moved(m, b.0));
            let cell = Rect::new(at - 4.0, 0.0, at + 4.0, band);
            if dragged(m).is_some() {
                canvas.draw_rect(cell, &fill(accent));
            } else {
                canvas.draw_rect(cell, &fill(t.paper));
                hatch(canvas, cell, 4.0, t.ink3);
                canvas.draw_rect(cell.with_inset((0.5, 0.5)), &stroke(t.ink3));
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
            let paint = faded(if on { accent } else { t.ink }, if d.is_some_and(|d| d.removing) { 0.25 } else { 1.0 });
            tab_glyph(canvas, s.alignment, x(moved(m, start + s.position.0)), 10.0, &paint);
        }
        // The indent markers: first line at the top, hanging (with the left indent) and right at the bottom.
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
        indent(canvas, x(first), 0.0, true, &colour(on(&[Marker::FirstLine, Marker::Left])));
        indent(canvas, x(left), 12.0, false, &colour(on(&[Marker::Hanging, Marker::Left])));
        indent(canvas, x(right), 12.0, false, &colour(on(&[Marker::Right])));
        // The ghost of the stop a click adds.
        if let Some((g, a)) = f.ghost.zip(alignment(f.selector)) {
            tab_glyph(canvas, a, x(g), 10.0, &faded(accent, 0.5));
        }
        if let Some((true, at)) = f.guide {
            rect(canvas, at as f32, 0.0, at as f32 + 1.0, band, &fill(accent));
        }
        canvas.restore();
        // The tab type box in the corner: the type a click adds; accent while its picker is open.
        let b = Rect::from_xywh(SELECTOR.left as f32, SELECTOR.top as f32, SELECTOR.width as f32, SELECTOR.height as f32);
        let picking = matches!(f.popover, Some(Popover::TabType { .. }));
        canvas.draw_rrect(RRect::new_rect_xy(b, 2.0, 2.0), &fill(t.paper));
        canvas.draw_rrect(RRect::new_rect_xy(b.with_inset((0.5, 0.5)), 1.5, 1.5), &stroke(if picking { accent } else { t.ink3 }));
        match (alignment(f.selector), f.selector) {
            (Some(a), _) => tab_glyph(canvas, a, b.left + 8.0, b.top, &ink),
            (None, TabSelector::FirstLineIndent) => indent(canvas, b.left + 9.0, b.top + 3.0, true, &ink),
            (None, _) => indent(canvas, b.left + 9.0, b.top + 3.0, false, &ink),
        }
    }

    if f.bands.vertical {
        let (o, k) = (view.vertical.origin, view.vertical.scale);
        let y = |pt: f64| (o + pt * k) as f32;
        let r = &view.model.vertical;
        let from = if f.bands.horizontal { band } else { 0.0 };
        rect(canvas, 0.0, from, band, h, &fill(t.desk));
        canvas.save();
        canvas.clip_rect(Rect::new(0.0, from, band, h), None, true);
        let top = moved(Marker::Top, r.text.start.0);
        let bottom = moved(Marker::Bottom, r.text.end.0);
        let (page_t, page_b) = (y(0.0), y(r.page_height.0));
        let paper = Rect::new(0.0, page_t, STRIP, page_b);
        canvas.draw_rrect(RRect::new_rect_xy(paper, 2.0, 2.0), &fill(t.paper));
        canvas.draw_rrect(RRect::new_rect_xy(paper.with_outset((0.5, 0.5)), 2.5, 2.5), &stroke(t.line));
        hatch(canvas, Rect::new(0.0, page_t, STRIP, y(top)), 5.0, t.line);
        hatch(canvas, Rect::new(0.0, y(bottom), STRIP, page_b), 5.0, t.line);
        ticks(top, r.page_height.0, &|pt| y(pt), &|py, len, paint| rect(canvas, STRIP - len, py, STRIP, py + 1.0, paint), &|py, n| {
            if let Some(font) = &numbers {
                label(canvas, font, n, 2.0, py - font.size() / 2.0 - 2.0, 0.0, 2.0, Some((t.paper, 0.0)), t.ink);
            }
        });
        for m in [Marker::Top, Marker::Bottom].into_iter().filter(|m| f.hover == Some(*m)) {
            let at = y(if m == Marker::Top { top } else { bottom });
            rect(canvas, 0.0, at - 4.0, STRIP, at + 4.0, &fill(theme.accent.soft));
        }
        for (i, b) in r.table_rows.iter().enumerate() {
            let m = Marker::TableRow(i as u32);
            let at = y(moved(m, b.0));
            let cell = Rect::new(0.0, at - 4.0, STRIP, at + 4.0);
            if dragged(m).is_some() {
                canvas.draw_rect(cell, &fill(accent));
            } else {
                canvas.draw_rect(cell, &fill(t.paper));
                hatch(canvas, cell, 4.0, t.ink3);
                canvas.draw_rect(cell.with_inset((0.5, 0.5)), &stroke(t.ink3));
            }
        }
        if let Some((false, at)) = f.guide {
            rect(canvas, 0.0, at as f32, STRIP, at as f32 + 1.0, &fill(accent));
        }
        canvas.restore();
    }

    // A drag's guide line over the document.
    if let Some((horizontal, at)) = f.guide {
        let mut guide = stroke(accent);
        guide.set_path_effect(PathEffect::dash(&[3.0, 3.0], 0.0));
        let at = at as f32 + 0.5;
        if horizontal {
            canvas.draw_line((at, band), (at, h), &guide);
        } else {
            canvas.draw_line((band, at), (w, at), &guide);
        }
    }
    measures(view, theme, canvas, f.measures);
    canvas.restore();
}

/// Draws the measurements: dimension lines with end ticks and their label on the paper colour, dark chips and hints,
/// and status lines; dimension and status points are the ruler page's, mapped by `view`.
fn measures(view: &RulerView, theme: &Theme, canvas: &Canvas, measures: &[RulerMeasure]) {
    if measures.is_empty() {
        return;
    }
    let t = &theme.tokens;
    let (Some(value), Some(chip)) = (face(ChromeFace::Mono600, 10.0), face(ChromeFace::Mono500, 10.0)) else { return };
    let (h, v) = (view.horizontal, view.vertical);
    let client = |p: [f64; 2]| ((h.origin + p[0] * h.scale) as f32, (v.origin + p[1] * v.scale) as f32);
    let tone = |tone: Tone| match tone {
        Tone::Accent => theme.accent.solid,
        Tone::Muted => t.ink2,
    };
    for m in measures {
        match m {
            RulerMeasure::Dimension { from, to, label: text, tone: c } => {
                let ((x0, y0), (x1, y1)) = (client(*from), client(*to));
                let paint = fill(tone(*c));
                if (y1 - y0).abs() <= (x1 - x0).abs() {
                    let (a, z, y) = (x0.min(x1), x0.max(x1).max(x0.min(x1) + 2.0), y0.round());
                    rect(canvas, a, y - 3.0, a + 1.0, y + 4.0, &paint);
                    rect(canvas, z - 1.0, y - 3.0, z, y + 4.0, &paint);
                    rect(canvas, a, y, z, y + 1.0, &paint);
                    let (tw, _) = value.measure_str(text, None);
                    label(canvas, &value, text, (a + z) / 2.0 - tw / 2.0 - 4.0, y - 5.0, 4.0, 0.0, Some((t.paper, 0.0)), tone(*c));
                } else {
                    let (a, z, x) = (y0.min(y1), y0.max(y1).max(y0.min(y1) + 2.0), x0.round());
                    rect(canvas, x - 3.0, a, x + 4.0, a + 1.0, &paint);
                    rect(canvas, x - 3.0, z - 1.0, x + 4.0, z, &paint);
                    rect(canvas, x, a, x + 1.0, z, &paint);
                    label(canvas, &value, text, x - 4.0, (a + z) / 2.0 - 5.0, 4.0, 0.0, Some((t.paper, 0.0)), tone(*c));
                }
            }
            RulerMeasure::Chip { at, label: text } => {
                label(canvas, &chip, text, at[0] as f32, at[1] as f32, 7.0, 4.0, Some((t.ink, 2.0)), t.paper);
            }
            RulerMeasure::Hint { at, label: text } => {
                label(canvas, &chip, text, *at as f32, (BAND + HINT_Y) as f32, 7.0, 4.0, Some((t.ink, 2.0)), t.paper);
            }
            RulerMeasure::Status { at, label: text, tone: c } => {
                let (x, y) = client(*at);
                label(canvas, &value, text, x, y, 0.0, 0.0, None, tone(*c));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::services::{RulerAxis, RulerModel, Span, TabStopInfo};
    use crate::api::value::Points;
    use crate::render::theme::{AZURE, Scheme};

    fn view() -> RulerView {
        let mut m = RulerModel::default();
        m.horizontal.page_width = Points(612.0);
        m.horizontal.text = Span { start: Points(72.0), end: Points(540.0) };
        m.horizontal.tabs = vec![TabStopInfo { position: Points(144.0), alignment: TabAlignment::Center, ..TabStopInfo::default() }];
        m.vertical.page_height = Points(792.0);
        m.vertical.text = Span { start: Points(72.0), end: Points(720.0) };
        RulerView {
            model: m,
            page: 1,
            horizontal: RulerAxis { origin: 24.0, scale: 1.0 },
            vertical: RulerAxis { origin: 24.0, scale: 1.0 },
            ..RulerView::default()
        }
    }

    fn frame<'a>(measures: &'a [RulerMeasure], drag: Option<Dragged>) -> RulerFrame<'a> {
        RulerFrame {
            bands: Bands { horizontal: true, vertical: true },
            width: 400.0,
            height: 300.0,
            dpr: 1.0,
            selector: TabSelector::Left,
            guide: Some((true, 200.0)),
            drag,
            ghost: None,
            hover: None,
            popover: None,
            measures,
        }
    }

    /// The rulers drawn on a grey viewport 400 by 300, as RGBA.
    fn draw(theme: &Theme, f: &RulerFrame) -> Vec<u8> {
        let mut pixels = vec![0u8; 400 * 300 * 4];
        let info = skia_safe::ImageInfo::new((400, 300), skia_safe::ColorType::RGBA8888, skia_safe::AlphaType::Premul, None);
        let mut surface = skia_safe::surfaces::wrap_pixels(&info, &mut pixels, 400 * 4, None).expect("a surface");
        surface.canvas().clear(Color::from_rgb(0x80, 0x80, 0x80));
        paint(&view(), theme, surface.canvas(), f);
        drop(surface);
        pixels
    }

    fn at(pixels: &[u8], x: usize, y: usize) -> [u8; 4] {
        pixels[(y * 400 + x) * 4..][..4].try_into().unwrap_or_default()
    }

    #[test]
    fn the_rulers_draw_on_the_paper_with_hatched_margins() {
        let theme = Theme::default();
        let p = draw(&theme, &frame(&[], None));
        // The text area is the paper; the band past the page is the desk; the document below is untouched.
        assert_eq!(at(&p, 24 + 300, 4), [255, 255, 255, 255]);
        assert_eq!(at(&p, 21, 1)[..3], [0xe8, 0xe7, 0xe3]);
        assert_eq!(at(&p, 300, 150), [0x80, 0x80, 0x80, 255]);
        // The margins are hatched: some pixels of the left margin's band are darker than the paper.
        assert!((30..90).any(|x| at(&p, x, 6)[0] < 0xf8), "hatched margin");
        // The vertical ruler's text area is the paper, its strip 18 px wide on the desk.
        assert_eq!(at(&p, 10, 200)[..3], [255, 255, 255]);
        assert_eq!(at(&p, 21, 200)[..3], [0xe8, 0xe7, 0xe3]);
        // The guide is accent in the band.
        assert_eq!(at(&p, 200, 5)[..3], [0x0a, 0x74, 0xe4]);
        // A unit tick, 10 px at the band's bottom, one inch from the text start.
        assert_eq!(at(&p, 24 + 72 + 72, 20)[..3], [0x62, 0x65, 0x6c]);
        // The dark theme draws dark rulers.
        let dark = draw(&Theme::new(Scheme::Dark, AZURE), &frame(&[], None));
        assert_eq!(at(&dark, 24 + 300, 4)[..3], [0x22, 0x23, 0x27]);
    }

    #[test]
    fn a_dragged_marker_is_drawn_where_the_drag_puts_it_in_accent() {
        let theme = Theme::default();
        let drag = Dragged { marker: Marker::Tab(144.0), at: 72.0 + 198.0, removing: false };
        let p = draw(&theme, &frame(&[], Some(drag)));
        // The centre tab's stem, 2 px wide at the stop, from 13 to 22 px.
        let stem = |p: &[u8], x: usize| at(p, x, 17);
        assert_eq!(stem(&p, 24 + 270)[..3], [0x0a, 0x74, 0xe4], "at the drag, accent");
        assert_ne!(stem(&p, 24 + 216)[..3], [0x1c, 0x1d, 0x1f], "no longer where it was");
        let removing = draw(&theme, &frame(&[], Some(Dragged { removing: true, ..drag })));
        assert!(stem(&removing, 24 + 270)[0] > 0x80, "faded while a release removes it");
        // The ghost of the stop a click adds: the box's type, half-faded accent.
        let ghost = draw(&theme, &RulerFrame { ghost: Some(72.0 + 198.0), ..frame(&[], None) });
        let g = stem(&ghost, 24 + 270);
        assert!(g[2] > g[0] && g[0] > 0x60 && g[0] < 0xd0, "{g:?}");
    }

    #[test]
    fn measurements_draw_over_the_document() {
        let theme = Theme::default();
        let dimension = RulerMeasure::Dimension { from: [10.0, 60.0], to: [350.0, 60.0], label: "First line 1.00″".into(), tone: Tone::Accent };
        let hint = RulerMeasure::Hint { at: 100.0, label: "First line · 0.50″ — indents only the first line".into() };
        let p = draw(&theme, &frame(&[dimension, hint], None));
        // The dimension's end ticks, at the page's x 10 and 350 and y 60, and its line.
        let accent = [0x0a, 0x74, 0xe4];
        assert_eq!(at(&p, 24 + 10, 24 + 60 + 2)[..3], accent);
        assert_eq!(at(&p, 24 + 349, 24 + 60 - 2)[..3], accent);
        assert_eq!(at(&p, 24 + 40, 24 + 60)[..3], accent);
        // The chips' face has the hints' dash and the values' prime and delta.
        let mono = chrome_typeface(ChromeFace::Mono500).expect("the chips' face");
        assert!(['—', '″', 'Δ', '·', '−'].iter().all(|c| mono.unichar_to_glyph(*c as i32) != 0));
        // The hint's dark chip under the band.
        assert_eq!(at(&p, 102, 24 + 16 + 1)[..3], [0x1c, 0x1d, 0x1f]);
    }
}
