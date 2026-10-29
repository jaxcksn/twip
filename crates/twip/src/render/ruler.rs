//! The built-in rulers' look: one painting boundary, [`paint`], that draws them from the ruler view alone, so a later
//! painter can replace it without touching the model, the commands or the interaction.

use skia_safe::{Canvas, Color, Font, Paint, Path, Point, Rect, Typeface};

use crate::api::services::{RulerView, TabSelector};
use crate::api::value::TabAlignment;
use crate::services::ruler::{BAND, Bands, marks};

/// The rulers' colours: the band, the margins on it, the text area, the ticks and numbers, and the markers.
#[derive(Clone, Copy, Debug)]
pub struct RulerTheme {
    pub band: Color,
    pub margin: Color,
    pub text: Color,
    pub ink: Color,
    pub marker: Color,
}

impl RulerTheme {
    /// Light rulers around a light background, dark ones around a dark one.
    pub fn of(background: Color) -> RulerTheme {
        let dark = u32::from(background.r()) + u32::from(background.g()) + u32::from(background.b()) < 3 * 128;
        if dark {
            RulerTheme {
                band: Color::from_rgb(0x2b, 0x2b, 0x2b),
                margin: Color::from_rgb(0x3c, 0x3c, 0x3c),
                text: Color::from_rgb(0x55, 0x55, 0x55),
                ink: Color::from_rgb(0xd0, 0xd0, 0xd0),
                marker: Color::from_rgb(0xe8, 0xe8, 0xe8),
            }
        } else {
            RulerTheme {
                band: Color::from_rgb(0xf0, 0xf0, 0xf0),
                margin: Color::from_rgb(0xc8, 0xc8, 0xc8),
                text: Color::WHITE,
                ink: Color::from_rgb(0x50, 0x50, 0x50),
                marker: Color::from_rgb(0x60, 0x60, 0x60),
            }
        }
    }
}

/// Where the rulers go and what else they show: which bands, the viewport (CSS px), device pixels per CSS pixel, the
/// tab selector, the guide line of a drag (horizontal ruler: an x, else a y; client px) and the numbers' face.
pub struct RulerFrame {
    pub bands: Bands,
    pub width: f64,
    pub height: f64,
    pub dpr: f64,
    pub selector: TabSelector,
    pub guide: Option<(bool, f64)>,
    pub typeface: Option<Typeface>,
}

fn fill(c: Color) -> Paint {
    let mut p = Paint::default();
    p.set_anti_alias(true);
    p.set_color(c);
    p
}

fn stroke(c: Color) -> Paint {
    let mut p = fill(c);
    p.set_style(skia_safe::paint::Style::Stroke);
    p.set_stroke_width(1.0);
    p
}

fn poly(canvas: &Canvas, pts: &[(f32, f32)], paint: &Paint) {
    let pts: Vec<Point> = pts.iter().map(|&(x, y)| Point::new(x, y)).collect();
    canvas.draw_path(&Path::polygon(&pts, true, None, None), paint);
}

/// A tab stop's mark of `kind` at `x`, its foot on `y` (also the tab selector's glyph).
fn tab_mark(canvas: &Canvas, kind: TabAlignment, x: f32, y: f32, ink: &Paint) {
    let line = |a: (f32, f32), b: (f32, f32)| {
        canvas.draw_line(a, b, ink);
    };
    match kind {
        TabAlignment::Left => {
            line((x, y - 6.0), (x, y));
            line((x, y), (x + 5.0, y));
        }
        TabAlignment::Right => {
            line((x, y - 6.0), (x, y));
            line((x - 5.0, y), (x, y));
        }
        TabAlignment::Center | TabAlignment::Decimal => {
            line((x, y - 6.0), (x, y));
            line((x - 4.0, y), (x + 4.0, y));
            if kind == TabAlignment::Decimal {
                canvas.draw_circle((x + 2.5, y - 3.0), 0.9, ink);
            }
        }
        TabAlignment::Bar => {
            line((x, y - 7.0), (x, y));
        }
    }
}

/// A face for the rulers' numbers: any the renderer has made for the document.
pub fn typeface(r: &super::Renderer) -> Option<Typeface> {
    r.typefaces.values().flatten().next().cloned()
}

/// Draws the rulers shown along the top and left of the viewport, from `view` alone.
// shortcut: one built-in look; a painter plugin will replace this function.
pub fn paint(view: &RulerView, theme: &RulerTheme, canvas: &Canvas, f: &RulerFrame) {
    let band = BAND as f32;
    let (w, h) = (f.width as f32, f.height as f32);
    let (major, minor, per) = marks(view.unit);
    let ink = stroke(theme.ink);
    let marker = fill(theme.marker);
    let font = f.typeface.clone().map(|tf| Font::from_typeface(tf, 8.0));
    canvas.save();
    canvas.scale((f.dpr as f32, f.dpr as f32));

    // Ticks from `zero` (the text start) both ways along `0..len` points: numbers at the labelled marks, longer ticks
    // at the half marks. `at` places a point along the ruler; `tick` draws one across it.
    let ticks = |zero: f64, len: f64, at: &dyn Fn(f64) -> f32, tick: &dyn Fn(f32, f32, Option<String>)| {
        let steps = (major / minor).round() as i64;
        let (lo, hi) = (((0.0 - zero) / minor).ceil() as i64, ((len - zero) / minor).floor() as i64);
        for i in lo..=hi {
            let p = at(zero + i as f64 * minor);
            if i == 0 {
                continue;
            }
            if i % steps == 0 {
                let n = (i / steps).abs() as f64 * per;
                tick(p, 0.0, Some(format!("{}", n.round() as i64)));
            } else {
                tick(p, if steps % 2 == 0 && i % (steps / 2) == 0 { 0.3 } else { 0.15 }, None);
            }
        }
    };

    if f.bands.horizontal {
        let (o, k) = (view.horizontal.origin, view.horizontal.scale);
        let x = |pt: f64| (o + pt * k) as f32;
        let r = &view.model.horizontal;
        canvas.draw_rect(Rect::new(0.0, 0.0, w, band), &fill(theme.band));
        canvas.save();
        canvas.clip_rect(Rect::new(if f.bands.vertical { band } else { 0.0 }, 0.0, w, band), None, true);
        let (top, bottom) = (3.0, band - 3.0);
        canvas.draw_rect(Rect::new(x(0.0), top, x(r.page_width.0), bottom), &fill(theme.margin));
        canvas.draw_rect(Rect::new(x(r.text.start.0), top, x(r.text.end.0), bottom), &fill(theme.text));
        for c in r.columns.windows(2) {
            canvas.draw_rect(Rect::new(x(c[0].end.0), top, x(c[1].start.0), bottom), &fill(theme.margin));
        }
        ticks(r.text.start.0, r.page_width.0, &|pt| x(pt), &|px, len, label| match (label, &font) {
            (Some(t), Some(font)) => {
                let (tw, _) = font.measure_str(&t, None);
                canvas.draw_str(&t, (px - tw / 2.0, band / 2.0 + 3.0), font, &fill(theme.ink));
            }
            (Some(_), None) => {
                canvas.draw_line((px, band / 2.0 - 3.0), (px, band / 2.0 + 3.0), &ink);
            }
            (None, _) => {
                canvas.draw_line((px, band / 2.0 - band * len / 2.0), (px, band / 2.0 + band * len / 2.0), &ink);
            }
        });
        for b in &r.table_columns {
            canvas.draw_rect(Rect::new(x(b.0) - 2.5, top, x(b.0) + 2.5, bottom), &fill(theme.margin));
            canvas.draw_rect(Rect::new(x(b.0) - 2.5, top, x(b.0) + 2.5, bottom), &ink);
        }
        for t in &r.tabs {
            tab_mark(canvas, t.alignment, x(r.text.start.0 + t.position.0), bottom - 1.0, &stroke(theme.marker));
        }
        // The indent markers: first line on top, hanging with the left indent's box below it, right indent.
        let left = x(r.text.start.0 + r.indents.left.0);
        let first = x(r.text.start.0 + r.indents.left.0 + r.indents.first_line.0);
        let right = x(r.text.end.0 - r.indents.right.0);
        poly(canvas, &[(first - 4.5, 0.0), (first + 4.5, 0.0), (first, band * 0.4)], &marker);
        poly(canvas, &[(left - 4.5, band * 0.75), (left + 4.5, band * 0.75), (left, band * 0.5)], &marker);
        canvas.draw_rect(Rect::new(left - 4.5, band * 0.75, left + 4.5, band), &marker);
        poly(canvas, &[(right - 4.5, band * 0.75), (right + 4.5, band * 0.75), (right, band * 0.5)], &marker);
        canvas.restore();
        // The tab selector box in the corner.
        let r = Rect::new(2.0, 2.0, band - 2.0, band - 2.0);
        canvas.draw_rect(r, &fill(theme.text));
        canvas.draw_rect(r, &ink);
        let c = band / 2.0;
        let alignment = match f.selector {
            TabSelector::Left => Some(TabAlignment::Left),
            TabSelector::Center => Some(TabAlignment::Center),
            TabSelector::Right => Some(TabAlignment::Right),
            TabSelector::Decimal => Some(TabAlignment::Decimal),
            TabSelector::Bar => Some(TabAlignment::Bar),
            TabSelector::FirstLineIndent => {
                poly(canvas, &[(c - 4.0, c - 3.0), (c + 4.0, c - 3.0), (c, c + 3.0)], &marker);
                None
            }
            TabSelector::HangingIndent => {
                poly(canvas, &[(c - 4.0, c + 3.0), (c + 4.0, c + 3.0), (c, c - 3.0)], &marker);
                None
            }
        };
        if let Some(a) = alignment {
            tab_mark(canvas, a, c - if a == TabAlignment::Right { -2.0 } else { 2.0 }, c + 3.0, &stroke(theme.marker));
        }
    }

    if f.bands.vertical {
        let (o, k) = (view.vertical.origin, view.vertical.scale);
        let y = |pt: f64| (o + pt * k) as f32;
        let r = &view.model.vertical;
        let from = if f.bands.horizontal { band } else { 0.0 };
        canvas.draw_rect(Rect::new(0.0, from, band, h), &fill(theme.band));
        canvas.save();
        canvas.clip_rect(Rect::new(0.0, from, band, h), None, true);
        let (l, rr) = (3.0, band - 3.0);
        canvas.draw_rect(Rect::new(l, y(0.0), rr, y(r.page_height.0)), &fill(theme.margin));
        canvas.draw_rect(Rect::new(l, y(r.text.start.0), rr, y(r.text.end.0)), &fill(theme.text));
        ticks(r.text.start.0, r.page_height.0, &|pt| y(pt), &|py, len, label| match (label, &font) {
            (Some(t), Some(font)) => {
                let (tw, _) = font.measure_str(&t, None);
                canvas.draw_str(&t, (band / 2.0 - tw / 2.0, py + 3.0), font, &fill(theme.ink));
            }
            (Some(_), None) => {
                canvas.draw_line((band / 2.0 - 3.0, py), (band / 2.0 + 3.0, py), &ink);
            }
            (None, _) => {
                canvas.draw_line((band / 2.0 - band * len / 2.0, py), (band / 2.0 + band * len / 2.0, py), &ink);
            }
        });
        for b in &r.table_rows {
            canvas.draw_rect(Rect::new(l, y(b.0) - 2.5, rr, y(b.0) + 2.5), &fill(theme.margin));
            canvas.draw_rect(Rect::new(l, y(b.0) - 2.5, rr, y(b.0) + 2.5), &ink);
        }
        canvas.restore();
    }

    // A drag's guide line over the document.
    if let Some((horizontal, at)) = f.guide {
        let mut guide = stroke(theme.ink);
        guide.set_path_effect(skia_safe::PathEffect::dash(&[3.0, 3.0], 0.0));
        let at = at as f32;
        if horizontal {
            canvas.draw_line((at, band), (at, h), &guide);
        } else {
            canvas.draw_line((band, at), (w, at), &guide);
        }
    }
    canvas.restore();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::services::{RulerAxis, RulerModel, Span, TabStopInfo};
    use crate::api::value::Points;

    #[test]
    fn the_rulers_draw_from_the_view_alone() {
        let mut m = RulerModel::default();
        m.horizontal.page_width = Points(612.0);
        m.horizontal.text = Span { start: Points(72.0), end: Points(540.0) };
        m.horizontal.tabs = vec![TabStopInfo { position: Points(144.0), alignment: TabAlignment::Center, ..TabStopInfo::default() }];
        m.vertical.page_height = Points(792.0);
        m.vertical.text = Span { start: Points(72.0), end: Points(720.0) };
        let view = RulerView {
            model: m,
            page: 1,
            horizontal: RulerAxis { origin: 24.0, scale: 1.0 },
            vertical: RulerAxis { origin: 24.0, scale: 1.0 },
            ..RulerView::default()
        };
        let f = RulerFrame {
            bands: Bands { horizontal: true, vertical: true },
            width: 400.0,
            height: 300.0,
            dpr: 1.0,
            selector: TabSelector::Left,
            guide: Some((true, 200.0)),
            typeface: None,
        };
        let mut pixels = vec![0u8; 400 * 300 * 4];
        let info = skia_safe::ImageInfo::new((400, 300), skia_safe::ColorType::RGBA8888, skia_safe::AlphaType::Premul, None);
        let mut surface = skia_safe::surfaces::wrap_pixels(&info, &mut pixels, 400 * 4, None).expect("a surface");
        surface.canvas().clear(Color::from_rgb(0x80, 0x80, 0x80));
        paint(&view, &RulerTheme::of(Color::WHITE), surface.canvas(), &f);
        drop(surface);
        let at = |x: usize, y: usize| -> [u8; 4] { pixels[(y * 400 + x) * 4..][..4].try_into().unwrap_or_default() };
        // The text area is white; the band outside the page is the band's colour; the document below is untouched.
        assert_eq!(at(24 + 300, 4), [255, 255, 255, 255]);
        assert_eq!(at(390, 1), [0xf0, 0xf0, 0xf0, 255]);
        assert_eq!(at(300, 150), [0x80, 0x80, 0x80, 255]);
        assert_eq!(at(10, 200)[..3], [255, 255, 255]);
        // A dark background gives dark rulers.
        assert!(RulerTheme::of(Color::from_rgb(0x20, 0x20, 0x20)).band.r() < 0x80);
    }
}
