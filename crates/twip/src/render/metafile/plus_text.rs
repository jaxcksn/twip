//! EMF+ text: strings laid out in a rect by a string format (lines broken at spaces and line breaks, aligned,
//! clipped), and driver strings, each glyph at its own place.

use twip_graphics::kurbo::{self, Affine, BezPath, Point};

use super::Player;
use super::plus::Plus;
use super::plus_objects::{Fill, Font, Format};
use crate::render::svg::{LineMetrics, Sink, Text};

/// `StringFormatFlags`.
const RIGHT_TO_LEFT: u32 = 0x1;
const VERTICAL: u32 = 0x2;
const TRAILING_SPACES: u32 = 0x800;
const NO_WRAP: u32 = 0x1000;
const LINE_LIMIT: u32 = 0x2000;
const NO_CLIP: u32 = 0x4000;

/// `DriverStringOptions`: the glyphs are characters; each glyph after the first at the font's advance.
const CMAP_LOOKUP: u32 = 0x1;
const REALIZED_ADVANCE: u32 = 0x4;

/// A line of a laid-out string: its characters' range and width.
struct Line {
    from: usize,
    to: usize,
    width: f64,
}

impl Plus {
    /// The font's em in world units, or none when it has no size.
    fn em(&self, font: &Font) -> Option<f64> {
        let em = self.to_world(font.em, font.unit);
        (em > 0.0 && em.is_finite()).then_some(em)
    }

    /// A line of `font`'s text: its characters and advances (none: the font's), placed by `transform` (from the
    /// line's space, its origin at `align`), coloured when the brush is a colour.
    fn line<'a>(font: &'a Font, em: f64, chars: &'a [u32], advances: Option<&'a [f64]>, align: (u8, u8), transform: Affine, fill: &Fill) -> Text<'a> {
        Text {
            family: &font.family,
            bold: font.bold,
            italic: font.italic,
            underline: font.underline,
            strike: font.strike,
            height: em,
            cell: false,
            chars,
            glyph_ids: false,
            advances,
            extra: 0.0,
            space_extra: 0.0,
            rises: None,
            align,
            transform,
            colour: if let Fill::Solid(c) = fill { *c } else { [0.0, 0.0, 0.0, 1.0] },
            background: None,
        }
    }

    /// Draws a line of text in `fill`: as text when it is a colour, else its outlines filled.
    fn draw_line<S: Sink>(&mut self, p: &mut Player<'_, S>, t: &Text<'_>, fill: &Fill, clip: Option<&BezPath>) {
        if !p.spend(t.chars.len()) {
            return;
        }
        if !matches!(fill, Fill::Solid(_)) {
            let mut outline = BezPath::new();
            p.sink.text_outline(t, &mut outline);
            if let Some(c) = clip {
                self.use_clip(p);
                p.sink.save();
                p.sink.clip_path(c, false);
                self.paint(p, &outline, false, fill);
                p.sink.restore();
            } else {
                self.paint(p, &outline, false, fill);
            }
            return;
        }
        self.use_clip(p);
        p.under_clip(super::COPY, |me| {
            if let Some(c) = clip {
                me.sink.save();
                me.sink.clip_path(c, false);
            }
            me.sink.text(t);
            if clip.is_some() {
                me.sink.restore();
            }
        });
    }

    /// `DrawString`: `chars` in `font` laid out in `rect` (world units; an empty one places the text at its corner
    /// unbroken) by `format`.
    pub(super) fn string<S: Sink>(&mut self, p: &mut Player<'_, S>, font: &Font, fill: &Fill, format: &Format, rect: kurbo::Rect, chars: &[u32]) {
        let Some(em) = self.em(font) else { return };
        if !p.spend(chars.len()) {
            return;
        }
        // Hotkey prefixes shown or hidden: the ampersand goes, and a doubled one is one; shown, the character after
        // it is underlined.
        let mut text = Vec::with_capacity(chars.len());
        let mut hot = Vec::new();
        let mut amp = false;
        for &c in chars {
            if format.hotkey != 0 && c == u32::from('&') && !amp {
                amp = true;
                continue;
            }
            if amp && format.hotkey == 1 && c != u32::from('&') {
                hot.push(text.len());
            }
            amp = false;
            text.push(c);
        }
        let vertical = format.flags & VERTICAL != 0;
        // Vertical text is horizontal text turned a quarter clockwise, its lines running down and on to the left.
        let (box_, into) = if vertical {
            (kurbo::Rect::new(0.0, 0.0, rect.height(), rect.width()), Affine::new([0.0, 1.0, -1.0, 0.0, rect.x1, rect.y0]))
        } else {
            (kurbo::Rect::new(0.0, 0.0, rect.width(), rect.height()), Affine::translate((rect.x0, rect.y0)))
        };
        let mut advances = std::mem::take(&mut self.advances);
        advances.clear();
        let probe = Self::line(font, em, &text, None, (0, 0), Affine::IDENTITY, fill);
        let metrics = p.sink.measure(&probe, &mut advances).unwrap_or(LineMetrics { ascent: 0.8 * em, descent: 0.2 * em, leading: 0.0 });
        advances.resize(text.len(), 0.0);
        for a in &mut advances {
            *a *= format.tracking;
        }
        // Tabs reach the next stop.
        let tab = |x: f64| {
            let mut at = format.first_tab;
            for s in &format.tabs {
                at += s;
                if at > x + 1e-9 {
                    return Some(at);
                }
            }
            None
        };
        let (lead, trail) = (format.leading * em, format.trailing * em);
        let room = box_.width() - lead - trail;
        let wrap = box_.width() > 0.0 && format.flags & NO_WRAP == 0;
        let lines = break_lines(&text, &mut advances, room, wrap, format.flags & TRAILING_SPACES != 0, tab);
        // A tab draws as the space it spans.
        for c in &mut text {
            if *c == 0x09 {
                *c = 0x20;
            }
        }
        let pitch = metrics.ascent + metrics.descent + metrics.leading;
        let count = if format.flags & LINE_LIMIT != 0 && box_.height() > 0.0 && pitch > 0.0 {
            lines.len().min((box_.height() / pitch + 1e-9).floor() as usize)
        } else {
            lines.len()
        };
        let total = count as f64 * pitch;
        let place = |align: u32, span: f64, size: f64| match align {
            1 => (span - size) / 2.0,
            2 => span - size,
            _ => 0.0,
        };
        let top = place(format.line_align, box_.height(), total);
        let w = self.w(p) * into;
        let clip = (format.flags & NO_CLIP == 0 && rect.width() > 0.0 && rect.height() > 0.0).then(|| super::plus::rect_path(box_, w));
        let rtl = format.flags & RIGHT_TO_LEFT != 0;
        let align = match (format.align, rtl) {
            (0, true) => 2,
            (2, true) => 0,
            (a, _) => a,
        };
        for (n, l) in lines.iter().take(count).enumerate() {
            let x = lead + place(align, if box_.width() > 0.0 { room } else { 0.0 }, l.width);
            let y = top + n as f64 * pitch;
            // The line in runs: a hotkey's character alone, underlined.
            let (mut from, mut at) = (l.from, x);
            let first = hot.partition_point(|&h| h < l.from);
            let cuts = hot[first..].iter().take_while(|&&h| h < l.to).flat_map(|&h| [h, h + 1]).chain([l.to]);
            for to in cuts {
                if to > from {
                    let mut t = Self::line(font, em, &text[from..to], Some(&advances[from..to]), (0, 0), w * Affine::translate((at, y)), fill);
                    t.underline |= hot.binary_search(&from).is_ok() && to == from + 1;
                    self.draw_line(p, &t, fill, clip.as_ref());
                    at += advances[from..to].iter().sum::<f64>();
                }
                from = to;
            }
            if p.stopped.get() {
                break;
            }
        }
        self.advances = advances;
    }

    /// `DrawDriverString`: glyphs (characters, with `CmapLookup`) each at its own baseline origin (or the first
    /// alone, the rest at the font's advances), through `m`.
    #[allow(clippy::too_many_arguments, reason = "the record's fields")]
    pub(super) fn driver_string<S: Sink>(
        &mut self,
        p: &mut Player<'_, S>,
        font: &Font,
        fill: &Fill,
        options: u32,
        glyphs: &[u32],
        at: &[Point],
        m: Affine,
    ) {
        let (Some(em), Some(&first)) = (self.em(font), at.first()) else { return };
        let w = self.w(p) * m;
        let mut t = Self::line(font, em, glyphs, None, (0, 1), w * Affine::translate(first.to_vec2()), fill);
        t.glyph_ids = options & CMAP_LOOKUP == 0;
        if options & REALIZED_ADVANCE != 0 {
            self.draw_line(p, &t, fill, None);
            return;
        }
        // Each glyph's move to the next one's origin; the last its own advance.
        let mut last = std::mem::take(&mut self.advances);
        last.clear();
        if let Some(g) = glyphs.last() {
            let one = Text { chars: std::slice::from_ref(g), ..t.clone() };
            p.sink.measure(&one, &mut last);
        }
        let mut advances = Vec::with_capacity(glyphs.len());
        let mut rises = Vec::with_capacity(glyphs.len());
        for i in 0..glyphs.len() {
            match at.get(i + 1) {
                Some(next) => {
                    advances.push(next.x - at[i].x);
                    rises.push(next.y - at[i].y);
                }
                None => {
                    advances.push(last.first().copied().unwrap_or(0.0));
                    rises.push(0.0);
                }
            }
        }
        let t = Text { advances: Some(&advances), rises: Some(&rises), ..t };
        self.draw_line(p, &t, fill, None);
        self.advances = last;
    }
}

/// `text` broken into lines at line breaks and, when `wrap`, at spaces (else between characters) before a line
/// passes `room`; tabs' advances set to reach their stops. A line's width leaves out its trailing spaces unless
/// `trailing`.
fn break_lines(text: &[u32], advances: &mut [f64], room: f64, wrap: bool, trailing: bool, tab: impl Fn(f64) -> Option<f64>) -> Vec<Line> {
    let space = |c: u32| c == 0x20 || c == 0x09;
    let mut lines = Vec::new();
    let (mut from, mut x) = (0usize, 0.0f64);
    // The last place a line may break: after a run of spaces, and the width before them.
    let mut brk: Option<(usize, f64)> = None;
    let width = |text: &[u32], advances: &[f64], from: usize, to: usize| -> f64 {
        let mut end = to;
        if !trailing {
            while end > from && space(text[end - 1]) {
                end -= 1;
            }
        }
        advances[from..end].iter().sum()
    };
    let mut i = 0;
    while i < text.len() {
        let c = text[i];
        if c == 0x0A || c == 0x0D {
            lines.push(Line { from, to: i, width: width(text, advances, from, i) });
            advances[i] = 0.0;
            if c == 0x0D && text.get(i + 1) == Some(&0x0A) {
                advances[i + 1] = 0.0;
                i += 1;
            }
            i += 1;
            (from, x, brk) = (i, 0.0, None);
            continue;
        }
        if c == 0x09
            && let Some(stop) = tab(x)
        {
            advances[i] = stop - x;
        }
        if wrap && !space(c) && x + advances[i] > room && i > from {
            let at = brk.map_or(i, |b| b.0);
            lines.push(Line { from, to: at, width: width(text, advances, from, at) });
            from = at;
            x = advances[from..i].iter().sum();
            brk = None;
        }
        x += advances[i];
        if space(c) && !text.get(i + 1).is_some_and(|&n| space(n)) {
            brk = Some((i + 1, x));
        }
        i += 1;
    }
    lines.push(Line { from, to: text.len(), width: width(text, advances, from, text.len()) });
    lines
}
